use crate::{
    config::{CONFIGURATION, PROGRESS_PRINTER},
    scanner::RESPONSES,
    statistics::{
        StatCommand::{self, UpdateUsizeField},
        StatField::ResourcesDiscovered,
    },
    utils::{ferox_print, make_request, open_file},
    FeroxChannel, FeroxResponse, FeroxSerialize,
};
use console::strip_ansi_codes;
use std::{
    fs, io,
    io::Write,
    sync::{Arc, Once, RwLock},
};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

/// Singleton buffered file behind an Arc/RwLock; used for file writes from two locations:
///     - [logger::initialize](../logger/fn.initialize.html) (specifically a closure on the global logger instance)
///     - `reporter::spawn_file_handler`
pub static mut LOCKED_FILE: Option<Arc<RwLock<io::BufWriter<fs::File>>>> = None;

/// An initializer Once variable used to create `LOCKED_FILE`
static INIT: Once = Once::new();

// Accessing a `static mut` is unsafe much of the time, but if we do so
// in a synchronized fashion (e.g., write once or read all) then we're
// good to go!
//
// This function will only call `open_file` once, and will
// otherwise always return the value returned from the first invocation.
pub fn get_cached_file_handle(filename: &str) -> Option<Arc<RwLock<io::BufWriter<fs::File>>>> {
    unsafe {
        INIT.call_once(|| {
            LOCKED_FILE = open_file(&filename);
        });
        LOCKED_FILE.clone()
    }
}

/// Creates all required output handlers (terminal, file) and returns
/// the transmitter sides of each mpsc along with each receiver's future's JoinHandle to be awaited
///
/// Any other module that needs to write a Response to stdout or output results to a file should
/// be passed a clone of the appropriate returned transmitter
pub fn initialize(
    output_file: &str,
    save_output: bool,
    tx_stats: UnboundedSender<StatCommand>,
) -> (
    UnboundedSender<FeroxResponse>,
    UnboundedSender<FeroxResponse>,
    JoinHandle<()>,
    Option<JoinHandle<()>>,
) {
    log::trace!(
        "enter: initialize({}, {}, {:?})",
        output_file,
        save_output,
        tx_stats
    );

    let (tx_rpt, rx_rpt): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();
    let (tx_file, rx_file): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();

    let file_clone = tx_file.clone();
    let stats_clone = tx_stats.clone();

    let term_reporter = tokio::spawn(async move {
        spawn_terminal_reporter(rx_rpt, file_clone, stats_clone, save_output).await
    });

    let file_reporter = if save_output {
        // -o used, need to spawn the thread for writing to disk
        let file_clone = output_file.to_string();
        Some(tokio::spawn(async move {
            spawn_file_reporter(rx_file, tx_stats, &file_clone).await
        }))
    } else {
        None
    };

    log::trace!(
        "exit: initialize -> ({:?}, {:?}, {:?}, {:?})",
        tx_rpt,
        tx_file,
        term_reporter,
        file_reporter
    );
    (tx_rpt, tx_file, term_reporter, file_reporter)
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives responses and prints them if they meet the given
/// reporting criteria
async fn spawn_terminal_reporter(
    mut resp_chan: UnboundedReceiver<FeroxResponse>,
    file_chan: UnboundedSender<FeroxResponse>,
    tx_stats: UnboundedSender<StatCommand>,
    save_output: bool,
) {
    log::trace!(
        "enter: spawn_terminal_reporter({:?}, {:?}, {:?}, {})",
        resp_chan,
        file_chan,
        tx_stats,
        save_output
    );

    while let Some(mut resp) = resp_chan.recv().await {
        log::trace!("received {} on reporting channel", resp.url());

        let contains_sentry = CONFIGURATION.status_codes.contains(&resp.status().as_u16());
        let unknown_sentry = !RESPONSES.contains(&resp); // !contains == unknown
        let should_process_response = contains_sentry && unknown_sentry;

        if should_process_response {
            // print to stdout
            ferox_print(&resp.as_str(), &PROGRESS_PRINTER);

            update_stat!(tx_stats, UpdateUsizeField(ResourcesDiscovered, 1));

            if save_output {
                // -o used, need to send the report to be written out to disk
                match file_chan.send(resp.clone()) {
                    Ok(_) => {
                        log::debug!("Sent {} to file handler", resp.url());
                    }
                    Err(e) => {
                        log::error!("Could not send {} to file handler: {}", resp.url(), e);
                    }
                }
            }
        }
        log::trace!("report complete: {}", resp.url());

        if CONFIGURATION.replay_client.is_some() && should_process_response {
            // replay proxy specified/client created and this response's status code is one that
            // should be replayed
            match make_request(
                CONFIGURATION.replay_client.as_ref().unwrap(),
                &resp.url(),
                tx_stats.clone(),
            )
            .await
            {
                Ok(_) => {}
                Err(e) => {
                    log::error!("{}", e);
                }
            }
        }

        if should_process_response {
            // add response to RESPONSES for serialization in case of ctrl+c
            // placed all by its lonesome like this so that RESPONSES can take ownership
            // of the FeroxResponse

            // before ownership is transferred, there's no real reason to keep the body anymore
            // so we can free that piece of data, reducing memory usage
            resp.text = String::new();

            RESPONSES.insert(resp);
        }
    }
    log::trace!("exit: spawn_terminal_reporter");
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives responses and writes them to the given output file if they meet
/// the given reporting criteria
async fn spawn_file_reporter(
    mut report_channel: UnboundedReceiver<FeroxResponse>,
    tx_stats: UnboundedSender<StatCommand>,
    output_file: &str,
) {
    let buffered_file = match get_cached_file_handle(&CONFIGURATION.output) {
        Some(file) => file,
        None => {
            log::trace!("exit: spawn_file_reporter");
            return;
        }
    };

    log::trace!(
        "enter: spawn_file_reporter({:?}, {})",
        report_channel,
        output_file
    );

    log::info!("Writing scan results to {}", output_file);

    while let Some(response) = report_channel.recv().await {
        safe_file_write(&response, buffered_file.clone(), CONFIGURATION.json);
    }

    update_stat!(tx_stats, StatCommand::Save);

    log::trace!("exit: spawn_file_reporter");
}

/// Given a string and a reference to a locked buffered file, write the contents and flush
/// the buffer to disk.
pub fn safe_file_write<T>(
    value: &T,
    locked_file: Arc<RwLock<io::BufWriter<fs::File>>>,
    convert_to_json: bool,
) where
    T: FeroxSerialize,
{
    // note to future self: adding logging of anything other than error to this function
    // is a bad idea. we call this function while processing records generated by the logger.
    // If we then call log::... while already processing some logging output, it results in
    // the second log entry being injected into the first.

    let contents = if convert_to_json {
        value.as_json()
    } else {
        value.as_str()
    };

    let contents = strip_ansi_codes(&contents);

    if let Ok(mut handle) = locked_file.write() {
        // write lock acquired
        match handle.write(contents.as_bytes()) {
            Ok(_) => {}
            Err(e) => {
                log::error!("could not write report to disk: {}", e);
            }
        }

        match handle.flush() {
            // this function is used within async functions/loops, so i'm flushing so that in
            // the event of a ctrl+c or w/e results seen so far are saved instead of left lying
            // around in the buffer
            Ok(_) => {}
            Err(e) => {
                log::error!("error writing to file: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    /// asserts that an empty string for a filename returns None
    fn reporter_get_cached_file_handle_without_filename_returns_none() {
        let _used = get_cached_file_handle(&"").unwrap();
    }
}
