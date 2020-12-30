// todo needs to be serializable and added to scan save/resume/output
// todo consider batch size for stats update/display (if display is used)
// todo are there more metrics to capture?
// - domains redirected to?
// - number of links extracted vs busted?
// - number of borked urls?
// - total time to run
// - time per directory
// - wildcards filtered
// todo integration test that hits some/all of the errors in make_request
// todo create a summary report to be shown when the scan ends, should present the accumulated data in a way that makes interpretation easy

use crate::{config::PROGRESS_PRINTER, FeroxChannel};
use console::{pad_str, Alignment};
use indicatif::ProgressBar;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

/// Wrapper to save me from writing Ordering::Relaxed a bajillion times
///
/// default is to increment by 1, second arg can be used to increment by a different value
macro_rules! atomic_increment {
    ($metric:expr) => {
        $metric.fetch_add(1, Ordering::Relaxed);
    };

    ($metric:expr, $value:expr) => {
        $metric.fetch_add($value, Ordering::Relaxed);
    };
}

/// Wrapper to save me from writing Ordering::Relaxed a bajillion times
macro_rules! atomic_load {
    ($metric:expr) => {
        $metric.load(Ordering::Relaxed);
    };
}

/// Data collection of statistics related to a scan
#[derive(Default, Serialize, Deserialize, Debug)]
pub struct Stats {
    /// tracker for number of timeouts seen by the client
    timeouts: AtomicUsize,

    /// tracker for total number of requests sent by the client
    requests: AtomicUsize,

    /// tracker for total number of requests expected to send if the scan runs to completion
    ///
    /// Note: this is a per-scan expectation; `expected_requests * current # of scans` would be
    /// indicative of the current expectation at any given time, but is a moving target.  
    expected_per_scan: AtomicUsize,

    /// tracker for accumulating total number of requests expected (i.e. as a new scan is started
    /// this value should increase by `expected_requests`
    total_expected: AtomicUsize,

    /// tracker for total number of errors encountered by the client
    errors: AtomicUsize,

    /// tracker for overall number of 2xx status codes seen by the client
    successes: AtomicUsize,

    /// tracker for overall number of 3xx status codes seen by the client
    redirects: AtomicUsize,

    /// tracker for overall number of 4xx status codes seen by the client
    client_errors: AtomicUsize,

    /// tracker for overall number of 5xx status codes seen by the client
    server_errors: AtomicUsize,

    /// tracker for number of scans performed, this directly equates to number of directories
    /// recursed into and affects the total number of expected requests
    total_scans: AtomicUsize,

    /// tracker for number of links extracted when `--extract-links` is used; sources are
    /// response bodies and robots.txt as of v1.11.0
    links_extracted: AtomicUsize,

    /// tracker for overall number of 403s seen by the client
    status_403s: AtomicUsize,

    /// tracker for overall number of wildcard urls filtered out by the client
    wildcards_filtered: AtomicUsize,

    /// tracker for overall number of all filtered responses
    responses_filtered: AtomicUsize,
}

/// implementation of statistics data collection struct
impl Stats {
    /// increment `requests` field by one
    fn add_request(&self) {
        atomic_increment!(self.requests);
    }

    /// Inspect the given `StatError` and increment the appropriate fields
    ///
    /// Implies incrementing:
    ///     - requests
    ///     - errors
    pub fn add_error(&self, error: StatError) {
        self.add_request();
        atomic_increment!(self.errors);

        match error {
            StatError::Timeout => {
                atomic_increment!(self.timeouts);
            }
            StatError::Status403 => {
                atomic_increment!(self.status_403s);
                atomic_increment!(self.client_errors);
            }
            _ => {
                // todo implement the rest of the errors
            }
        }
    }

    /// Inspect the given `StatusCode` and increment the appropriate fields
    ///
    /// Implies incrementing:
    ///     - requests
    ///     - status_403s (when code is 403)
    ///     - errors (when code is [45]xx)
    fn add_status_code(&self, status: StatusCode) {
        self.add_request();

        if status.is_success() {
            atomic_increment!(self.successes);
        } else if status.is_redirection() {
            atomic_increment!(self.redirects);
        } else if status.is_client_error() {
            if status.as_u16() != 404 {
                // don't increment errors for 404, they're expected when busting
                atomic_increment!(self.errors);
            }
            atomic_increment!(self.client_errors);
        } else if status.is_server_error() {
            atomic_increment!(self.errors);
            atomic_increment!(self.server_errors);
        }
        // todo consider else / other status codes etc...

        if matches!(status, StatusCode::FORBIDDEN) {
            atomic_increment!(self.status_403s);
        }
    }

    fn update_field(&self, field: StatField, value: usize) {
        match field {
            StatField::ExpectedPerScan => {
                atomic_increment!(self.expected_per_scan, value);
            }
            StatField::TotalScans => {
                atomic_increment!(self.total_scans, value);
                atomic_increment!(
                    self.total_expected,
                    value * self.expected_per_scan.load(Ordering::Relaxed)
                );
            }
            StatField::TotalExpected => {
                atomic_increment!(self.total_expected, value);
            }
            StatField::LinksExtracted => {
                atomic_increment!(self.links_extracted, value);
            }
            StatField::WildcardsFiltered => {
                atomic_increment!(self.wildcards_filtered, value);
                atomic_increment!(self.responses_filtered, value);
            }
            StatField::ResponsesFiltered => {
                atomic_increment!(self.responses_filtered, value);
            }
        }
    }

    /// Print a summary of all information accumulated/surmised during the scan(s)
    pub fn print_summary(&self, printer: &ProgressBar) {
        let results_bottom = "───────────────────────────┬──────────────────────";
        let results_top = "──────────────────────────────────────────────────";
        let bottom = "───────────────────────────┴──────────────────────";

        macro_rules! format_summary_item {
            ($title:expr, $value:expr) => {
                format!(
                    "\u{0020}{:\u{0020}<26}\u{2502}\u{0020}{:\u{0020}^21}",
                    $title, $value
                )
            };
        }

        let results_header = format!("📊{}📊", pad_str("Results", 46, Alignment::Center, None));

        printer.println(format!("{}", results_top));
        printer.println(results_header);
        printer.println(format!("{}", results_bottom));

        printer.println(format_summary_item!(
            "Requests Sent / Expected",
            format!(
                "{} / {}",
                atomic_load!(self.requests),
                atomic_load!(self.total_expected)
            )
        ));

        let mut fields = BTreeMap::new();

        fields.insert("Errors", &self.errors);
        fields.insert("403 Forbidden", &self.status_403s);
        fields.insert("Success Status Codes", &self.successes);
        fields.insert("Redirects", &self.redirects);
        fields.insert("Links Extracted", &self.links_extracted);
        fields.insert("Timeouts", &self.timeouts);
        fields.insert("Requests Expected per Dir", &self.expected_per_scan);
        fields.insert("Client Error Codes", &self.client_errors);
        fields.insert("Server Error Codes", &self.server_errors);
        fields.insert("Non-404s Filtered", &self.responses_filtered);
        fields.insert("Wildcard Responses", &self.wildcards_filtered);

        for (key, value) in &fields {
            let loaded = atomic_load!(value);
            if loaded > 0 {
                let msg = format_summary_item!(key, loaded);
                printer.println(msg);
            }
        }

        printer.println(format!("{}", bottom));
    }
}

#[derive(Debug)]
/// Enum variants used to inform the `StatCommand` protocol what `Stats` fields should be updated
pub enum StatError {
    /// Represents a 403 response code
    Status403,

    /// Represents a timeout error
    Timeout,

    /// Represents a URL formatting error
    UrlFormat,

    /// Represents an error encountered during redirection
    Redirection,

    /// Represents an error encountered during connection
    Connection,

    /// Represents an error resulting from the client's request
    Request,

    /// Represents any other error not explicitly defined above
    Other,
}

/// Protocol definition for updating a Stats object via mpsc
pub enum StatCommand {
    /// Add one to the total number of requests
    AddRequest,

    /// Add one to the proper field(s) based on the given `StatError`
    AddError(StatError),

    /// Add one to the proper field(s) based on the given `StatusCode`
    AddStatus(StatusCode),

    /// Update a `Stats` field that corresponds to the given `StatField` by the given value
    UpdateField(StatField, usize),

    /// Break out of the (infinite) mpsc receive loop
    Exit,
}

/// Enum representing fields whose updates need to be performed in batches instead of one at
/// a time
pub enum StatField {
    /// Due to the necessary order of events, the number of requests expected to be sent isn't
    /// known until after `statistics::initialize` is called. This command allows for updating
    /// the `expected_per_scan` field after initialization
    ExpectedPerScan,

    /// Translates to Stats::total_scans
    TotalScans,

    /// Translates to links_extracted
    LinksExtracted,

    /// Translates to total_expected
    TotalExpected,

    /// Translates to wildcards_filtered
    WildcardsFiltered,

    /// Translates to responses_filtered
    ResponsesFiltered,
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives `StatCommands` and updates the given `Stats` object as appropriate
pub async fn spawn_statistics_handler(
    mut stats_channel: UnboundedReceiver<StatCommand>,
    stats: Arc<Stats>,
) {
    log::trace!(
        "enter: spawn_statistics_handler({:?}, {:?})",
        stats_channel,
        stats
    );

    while let Some(command) = stats_channel.recv().await {
        match command as StatCommand {
            StatCommand::AddError(err) => {
                stats.add_error(err);
            }
            StatCommand::AddStatus(status) => {
                stats.add_status_code(status);
            }
            StatCommand::AddRequest => stats.add_request(),
            StatCommand::UpdateField(field, value) => stats.update_field(field, value),
            StatCommand::Exit => break,
        }
    }

    stats.print_summary(&*PROGRESS_PRINTER);

    log::trace!("exit: spawn_statistics_handler")
}

/// Initialize new `Stats` object and the sc side of an mpsc channel that is responsible for
/// updates to the aforementioned object.
pub fn initialize() -> (Arc<Stats>, UnboundedSender<StatCommand>, JoinHandle<()>) {
    log::trace!("enter: initialize");

    let stats_tracker = Arc::new(Stats::default());
    let cloned = stats_tracker.clone();
    let (tx_stats, rx_stats): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
    let stats_thread =
        tokio::spawn(async move { spawn_statistics_handler(rx_stats, cloned).await });

    log::trace!(
        "exit: initialize -> ({:?}, {:?}, {:?})",
        stats_tracker,
        tx_stats,
        stats_thread
    );

    (stats_tracker, tx_stats, stats_thread)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// simple helper to reduce code reuse
    fn setup_stats_test() -> (Arc<Stats>, UnboundedSender<StatCommand>, JoinHandle<()>) {
        initialize()
    }

    /// another helper to stay DRY; must be called after any sent commands and before any checks
    /// performed against the Stats object
    async fn teardown_stats_test(sender: UnboundedSender<StatCommand>, handle: JoinHandle<()>) {
        // send exit and await, once the await completes, stats should be updated
        sender.send(StatCommand::Exit).unwrap_or_default();
        handle.await.unwrap();
    }

    #[tokio::test(core_threads = 1)]
    /// when sent StatCommand::Exit, function should exit its while loop (runs forever otherwise)
    async fn statistics_handler_exits() {
        let (_, sender, handle) = setup_stats_test();

        sender.send(StatCommand::Exit).unwrap_or_default();

        handle.await.unwrap(); // blocks on the handler's while loop

        // if we've made it here, the test has succeeded
    }

    #[tokio::test(core_threads = 1)]
    /// when sent StatCommand::IncrementRequest, stats object should reflect the change
    async fn statistics_handler_increments_requests() {
        let (stats, tx, handle) = setup_stats_test();

        tx.send(StatCommand::AddRequest).unwrap_or_default();
        tx.send(StatCommand::AddRequest).unwrap_or_default();
        tx.send(StatCommand::AddRequest).unwrap_or_default();

        teardown_stats_test(tx, handle).await;

        assert_eq!(stats.requests.load(Ordering::Relaxed), 3);
    }

    #[tokio::test(core_threads = 1)]
    /// when sent StatCommand::IncrementRequest, stats object should reflect the change
    ///
    /// incrementing a 403 (tracked in status_403s) should also increment:
    ///     - errors
    ///     - requests
    ///     - client_errors
    async fn statistics_handler_increments_403() {
        let (stats, tx, handle) = setup_stats_test();

        let err = StatCommand::AddError(StatError::Status403);
        let err2 = StatCommand::AddError(StatError::Status403);

        tx.send(err).unwrap_or_default();
        tx.send(err2).unwrap_or_default();

        teardown_stats_test(tx, handle).await;

        assert_eq!(stats.errors.load(Ordering::Relaxed), 2);
        assert_eq!(stats.requests.load(Ordering::Relaxed), 2);
        assert_eq!(stats.status_403s.load(Ordering::Relaxed), 2);
        assert_eq!(stats.client_errors.load(Ordering::Relaxed), 2);
    }

    #[tokio::test(core_threads = 1)]
    /// when sent StatCommand::IncrementRequest, stats object should reflect the change
    ///
    /// incrementing a 403 (tracked in status_403s) should also increment:
    ///     - errors
    ///     - requests
    ///     - client_errors
    async fn statistics_handler_increments_403_via_status_code() {
        let (stats, tx, handle) = setup_stats_test();

        let err = StatCommand::AddStatus(reqwest::StatusCode::FORBIDDEN);
        let err2 = StatCommand::AddStatus(reqwest::StatusCode::FORBIDDEN);

        tx.send(err).unwrap_or_default();
        tx.send(err2).unwrap_or_default();

        teardown_stats_test(tx, handle).await;

        assert_eq!(stats.errors.load(Ordering::Relaxed), 2);
        assert_eq!(stats.requests.load(Ordering::Relaxed), 2);
        assert_eq!(stats.status_403s.load(Ordering::Relaxed), 2);
        assert_eq!(stats.client_errors.load(Ordering::Relaxed), 2);
    }

    #[test]
    /// when Stats::add_error receives StatError::Timeout, it should increment the following:
    ///     - timeouts
    ///     - requests
    ///     - errors
    fn stats_increments_timeouts() {
        let stats = Stats::default();
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);

        assert_eq!(stats.errors.load(Ordering::Relaxed), 4);
        assert_eq!(stats.requests.load(Ordering::Relaxed), 4);
        assert_eq!(stats.timeouts.load(Ordering::Relaxed), 4);
    }

    #[test]
    /// when Stats::new is called, the value is properly assigned to expected_requests
    fn stats_new_sets_expected_requests() {
        let stats = Stats::new(42);
        assert_eq!(stats.expected_per_scan.load(Ordering::Relaxed), 42);
    }
}
