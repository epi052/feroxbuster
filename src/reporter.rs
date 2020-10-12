use crate::config::{CONFIGURATION, PROGRESS_PRINTER};
use crate::utils::{ferox_print, status_colorizer};
use crate::FeroxChannel;
use console::strip_ansi_codes;
use reqwest::Response;
use std::io::Write;
use std::sync::{Arc, Once, RwLock};
use std::{fs, io};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

pub static mut LOCKED_FILE: Option<Arc<RwLock<io::BufWriter<fs::File>>>> = None;
static INIT: Once = Once::new();

// Accessing a `static mut` is unsafe much of the time, but if we do so
// in a synchronized fashion (e.g., write once or read all) then we're
// good to go!
//
// This function will only call `open_file` once, and will
// otherwise always return the value returned from the first invocation.
pub fn get_cached_file_handle() -> Option<Arc<RwLock<io::BufWriter<fs::File>>>> {
    unsafe {
        INIT.call_once(|| {
            LOCKED_FILE = open_file(&CONFIGURATION.output);
        });
        LOCKED_FILE.clone()
    }
}

/// Creates all required output handlers (terminal, file) and returns
/// the transmitter side of an mpsc and the primary output handler's JoinHandle
/// to be awaited
///
/// Any other module that needs to write a Response to stdout or output results to a file should
/// be passed a clone of the appropriate returned transmitter
pub fn initialize(
    output_file: &str,
    save_output: bool,
) -> (
    UnboundedSender<Response>,
    UnboundedSender<String>,
    JoinHandle<()>,
    Option<JoinHandle<()>>,
) {
    log::trace!("enter: initialize({}, {})", output_file, save_output);

    let (tx_rpt, rx_rpt): FeroxChannel<Response> = mpsc::unbounded_channel();
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
    mut resp_chan: UnboundedReceiver<Response>,
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
        log::debug!("received {} on reporting channel", resp.url());

        if CONFIGURATION.statuscodes.contains(&resp.status().as_u16()) {
            let report = if CONFIGURATION.quiet {
                // -q used, just need the url
                format!("{}\n", resp.url())
            } else {
                // normal printing with status and size
                let status = status_colorizer(&resp.status().as_str());
                format!(
                    // example output
                    // 200       3280 https://localhost.com/FAQ
                    "{} {:>10} {}\n",
                    status,
                    resp.content_length().unwrap_or(0),
                    resp.url()
                )
            };

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
        log::debug!("report complete: {}", resp.url());
    }
    log::trace!("exit: spawn_terminal_reporter");
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives responses and writes them to the given output file if they meet
/// the given reporting criteria
async fn spawn_file_reporter(mut report_channel: UnboundedReceiver<String>, output_file: &str) {
    let buffered_file = match get_cached_file_handle() {
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
