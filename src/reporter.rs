use crate::config::{CONFIGURATION, PROGRESS_PRINTER};
use crate::utils::{create_report_string, ferox_print, make_request};
use crate::{FeroxChannel, FeroxResponse};
use console::strip_ansi_codes;
use std::io::Write;
use std::sync::{Arc, Once, RwLock};
use std::{fs, io};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

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
) -> (
    UnboundedSender<FeroxResponse>,
    UnboundedSender<String>,
    JoinHandle<()>,
    Option<JoinHandle<()>>,
) {
    log::trace!("enter: initialize({}, {})", output_file, save_output);

    let (tx_rpt, rx_rpt): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();
    let (tx_file, rx_file): FeroxChannel<String> = mpsc::unbounded_channel();

    let file_clone = tx_file.clone();

    let term_reporter =
        tokio::spawn(async move { spawn_terminal_reporter(rx_rpt, file_clone, save_output).await });

    let file_reporter = if save_output {
        // -o used, need to spawn the thread for writing to disk
        let file_clone = output_file.to_string();
        Some(tokio::spawn(async move {
            spawn_file_reporter(rx_file, &file_clone).await
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
    file_chan: UnboundedSender<String>,
    save_output: bool,
) {
    log::trace!(
        "enter: spawn_terminal_reporter({:?}, {:?}, {})",
        resp_chan,
        file_chan,
        save_output
    );

    while let Some(resp) = resp_chan.recv().await {
        log::trace!("received {} on reporting channel", resp.url());

        if CONFIGURATION.status_codes.contains(&resp.status().as_u16()) {
            let report = create_report_string(
                resp.status().as_str(),
                &resp.content_length().to_string(),
                &resp.url().to_string(),
            );

            // print to stdout
            ferox_print(&report, &PROGRESS_PRINTER);

            if save_output {
                // -o used, need to send the report to be written out to disk
                match file_chan.send(report.to_string()) {
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

        if CONFIGURATION.replay_client.is_some()
            && CONFIGURATION.replay_codes.contains(&resp.status().as_u16())
        {
            // replay proxy specified/client created and this response's status code is one that
            // should be replayed
            match make_request(CONFIGURATION.replay_client.as_ref().unwrap(), &resp.url()).await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("{}", e);
                }
            }
        }
    }
    log::trace!("exit: spawn_terminal_reporter");
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives responses and writes them to the given output file if they meet
/// the given reporting criteria
async fn spawn_file_reporter(mut report_channel: UnboundedReceiver<String>, output_file: &str) {
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

    while let Some(report) = report_channel.recv().await {
        safe_file_write(&report, buffered_file.clone());
    }

    log::trace!("exit: spawn_file_reporter");
}

/// Given the path to a file, open the file in append mode (create it if it doesn't exist) and
/// return a reference to the file that is buffered and locked
fn open_file(filename: &str) -> Option<Arc<RwLock<io::BufWriter<fs::File>>>> {
    log::trace!("enter: open_file({})", filename);

    match fs::OpenOptions::new() // std fs
        .create(true)
        .append(true)
        .open(filename)
    {
        Ok(file) => {
            let writer = io::BufWriter::new(file); // std io

            let locked_file = Some(Arc::new(RwLock::new(writer)));

            log::trace!("exit: open_file -> {:?}", locked_file);
            locked_file
        }
        Err(e) => {
            log::error!("{}", e);
            log::trace!("exit: open_file -> None");
            None
        }
    }
}

/// Given a string and a reference to a locked buffered file, write the contents and flush
/// the buffer to disk.
pub fn safe_file_write(contents: &str, locked_file: Arc<RwLock<io::BufWriter<fs::File>>>) {
    // note to future self: adding logging of anything other than error to this function
    // is a bad idea. we call this function while processing records generated by the logger.
    // If we then call log::... while already processing some logging output, it results in
    // the second log entry being injected into the first.

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
