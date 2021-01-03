// todo integration test that hits some/all of the errors in make_request
// todo resume_scan should repopulate statistics if possible or at least update an already existing Stats
// todo logic for determining if tuning is required

use crate::{
    config::CONFIGURATION,
    progress::{add_bar, BarType},
    reporter::{get_cached_file_handle, safe_file_write},
    FeroxChannel, FeroxSerialize,
};
use console::{pad_str, style, Alignment};
use indicatif::ProgressBar;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Instant;
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

/// Wrapper around consistent formatting for summary table items
macro_rules! format_summary_item {
    ($title:expr, $value:expr) => {
        format!(
            "\u{0020}{:\u{0020}<26}\u{2502}\u{0020}{:\u{0020}^21}",
            $title, $value
        )
    };
}

/// Data collection of statistics related to a scan
#[derive(Default, Deserialize, Debug, Serialize)]
pub struct Stats {
    #[serde(rename = "type")]
    /// Name of this type of struct, used for serialization, i.e. `{"type":"statistics"}`
    kind: String,

    /// tracker for number of timeouts seen by the client
    timeouts: AtomicUsize,

    /// tracker for total number of requests sent by the client
    requests: AtomicUsize,

    /// tracker for total number of requests expected to send if the scan runs to completion
    ///
    /// Note: this is a per-scan expectation; `expected_requests * current # of scans` would be
    /// indicative of the current expectation at any given time, but is a moving target.  
    pub expected_per_scan: AtomicUsize,

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

    /// tracker for initial number of requested targets
    pub initial_targets: AtomicUsize,

    /// tracker for number of links extracted when `--extract-links` is used; sources are
    /// response bodies and robots.txt as of v1.11.0
    links_extracted: AtomicUsize,

    /// tracker for overall number of 403s seen by the client
    status_403s: AtomicUsize,

    /// tracker for overall number of wildcard urls filtered out by the client
    wildcards_filtered: AtomicUsize,

    /// tracker for overall number of all filtered responses
    responses_filtered: AtomicUsize,

    /// tracker for number of files found
    resources_discovered: AtomicUsize,

    /// tracker for each directory's total scan time in seconds as a float
    directory_scan_times: Mutex<Vec<f64>>,

    /// tracker for total runtime
    total_runtime: Mutex<Vec<f64>>,

    /// tracker for number of errors triggered during URL formatting
    url_format_errors: AtomicUsize,

    /// tracker for number of errors triggered by the `reqwest::RedirectPolicy`
    redirection_errors: AtomicUsize,

    /// tracker for number of errors related to the connecting
    connection_errors: AtomicUsize,

    /// tracker for number of errors related to the request used
    request_errors: AtomicUsize,
}

/// FeroxSerialize implementation for Stats
impl FeroxSerialize for Stats {
    /// Simply return debug format of Stats to satisfy as_str
    fn as_str(&self) -> String {
        String::new()
    }

    /// Simple call to produce a JSON string using the given Stats object
    fn as_json(&self) -> String {
        serde_json::to_string(&self).unwrap_or_default()
    }
}

/// implementation of statistics data collection struct
impl Stats {
    /// Small wrapper for default to set `kind` to "statistics" and `total_runtime` to have at least
    /// one value
    pub fn new() -> Self {
        Self {
            kind: String::from("statistics"),
            total_runtime: Mutex::new(vec![0.0]),
            ..Default::default()
        }
    }

    /// increment `requests` field by one
    fn add_request(&self) {
        atomic_increment!(self.requests);
    }

    /// given an `Instant` update total runtime
    fn update_runtime(&self, seconds: f64) {
        if let Ok(mut runtime) = self.total_runtime.lock() {
            runtime[0] = seconds;
        }
    }

    /// save an instance of `Stats` to disk
    fn save(&self) {
        let buffered_file = match get_cached_file_handle(&CONFIGURATION.output) {
            Some(file) => file,
            None => {
                return;
            }
        };

        safe_file_write(self, buffered_file, CONFIGURATION.json);
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
            StatError::UrlFormat => {
                atomic_increment!(self.url_format_errors);
            }
            StatError::Redirection => {
                atomic_increment!(self.redirection_errors);
            }
            StatError::Connection => {
                atomic_increment!(self.connection_errors);
            }
            StatError::Request => {
                atomic_increment!(self.request_errors);
            }
            StatError::Other => {
                atomic_increment!(self.errors);
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
            atomic_increment!(self.client_errors);
        } else if status.is_server_error() {
            atomic_increment!(self.server_errors);
        }
        // todo consider else / other status codes etc...

        if matches!(status, StatusCode::FORBIDDEN) {
            atomic_increment!(self.status_403s);
        }
    }

    /// Takes all known directory scan times from `directory_scan_times` and calculates the
    /// shortest, longest, average, and total scan times (returned in that order)
    ///
    /// If a mutex can't be acquired, 0.0 is returned for the values behind the mutex
    fn calculate_scan_times(&self) -> (f64, f64, f64, f64) {
        let mut shortest = 0.0;
        let mut longest = 0.0;
        let mut average = 0.0;
        let mut total = 0.0;

        if let Ok(scans) = self.directory_scan_times.lock() {
            shortest = scans.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            longest = scans.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            average = scans.iter().sum::<f64>() / scans.len() as f64;
        }

        if let Ok(runtime) = self.total_runtime.lock() {
            total = runtime[0];
        }

        (shortest, longest, average, total)
    }

    /// Update a `Stats` field of type f64
    fn update_f64_field(&self, field: StatField, value: f64) {
        if let StatField::DirScanTimes = field {
            // todo unwrap
            self.directory_scan_times.lock().unwrap().push(value);
        }
    }

    /// Update a `Stats` field of type usize
    fn update_usize_field(&self, field: StatField, value: usize) {
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
            StatField::ResourcesDiscovered => {
                atomic_increment!(self.resources_discovered, value);
            }
            StatField::InitialTargets => {
                atomic_increment!(self.initial_targets, value);
            }
            _ => {} // f64 fields
        }
    }

    /// simple encapsulation to keep `summary` a bit cleaner
    fn add_f64_summary_data(&self, lines: &mut Vec<String>) {
        let mut fields = BTreeMap::new();

        let (shortest, longest, avg, total) = self.calculate_scan_times();

        fields.insert("Shortest Dir Scan", &shortest);
        fields.insert("Longest Dir Scan", &longest);
        fields.insert("Average Dir Scan", &avg);
        fields.insert("Total Scan Time", &total);

        for (key, value) in &fields {
            if **value > 0.0 {
                let msg = format!(
                    "\u{0020}{:\u{0020}<26}\u{2502}\u{0020}{:\u{0020}^21}",
                    key,
                    format!("{:.4} secs", value)
                );

                lines.push(msg);
            }
        }
    }

    /// simple encapsulation to keep `summary` a bit cleaner
    fn add_usize_summary_data(&self, lines: &mut Vec<String>) {
        let mut fields = BTreeMap::new();

        fields.insert("Requests Sent", &self.requests);
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
        fields.insert("Resources Discovered", &self.resources_discovered);

        for (key, value) in &fields {
            let loaded = atomic_load!(value);
            if loaded > 0 {
                let msg = format_summary_item!(key, loaded);
                lines.push(msg);
            }
        }
    }

    /// Build out the summary string of a `Stats` object
    fn summary(&self) -> String {
        let results_bottom = "───────────────────────────┬──────────────────────";
        let results_top = "──────────────────────────────────────────────────";
        let bottom = "───────────────────────────┴──────────────────────";

        let mut lines = Vec::new();

        let padded_results = pad_str("Scan Summary", 44, Alignment::Center, None);
        let results_header = format!("\u{0020}📊{}📊\u{0020}", padded_results);

        lines.push(results_top.to_string());
        lines.push(results_header);
        lines.push(results_bottom.to_string());

        self.add_f64_summary_data(&mut lines);
        self.add_usize_summary_data(&mut lines);

        lines.push(bottom.to_string());

        lines.join("\n")
    }

    /// Print a summary of all information accumulated/surmised during the scan(s)
    pub fn print_summary(&self, printer: &ProgressBar) {
        printer.println(self.summary());
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
#[derive(Debug)]
pub enum StatCommand {
    /// Add one to the total number of requests
    AddRequest,

    /// Add one to the proper field(s) based on the given `StatError`
    AddError(StatError),

    /// Add one to the proper field(s) based on the given `StatusCode`
    AddStatus(StatusCode),

    /// Create the progress bar (`BarType::Total`) that is updated from the stats thread
    CreateBar,

    /// Update a `Stats` field that corresponds to the given `StatField` by the given `usize` value
    UpdateUsizeField(StatField, usize),

    /// Update a `Stats` field that corresponds to the given `StatField` by the given `f64` value
    UpdateF64Field(StatField, f64),

    /// Save a `Stats` object to disk using `reporter::get_cached_file_handle`
    Save,

    /// Break out of the (infinite) mpsc receive loop
    Exit,
}

/// Enum representing fields whose updates need to be performed in batches instead of one at
/// a time
#[derive(Debug)]
pub enum StatField {
    /// Due to the necessary order of events, the number of requests expected to be sent isn't
    /// known until after `statistics::initialize` is called. This command allows for updating
    /// the `expected_per_scan` field after initialization
    ExpectedPerScan,

    /// Translates to `total_scans`
    TotalScans,

    /// Translates to `links_extracted`
    LinksExtracted,

    /// Translates to `total_expected`
    TotalExpected,

    /// Translates to `wildcards_filtered`
    WildcardsFiltered,

    /// Translates to `responses_filtered`
    ResponsesFiltered,

    /// Translates to `resources_discovered`
    ResourcesDiscovered,

    /// Translates to `initial_targets`
    InitialTargets,

    /// Translates to `directory_scan_times`; assumes a single append to the vector
    DirScanTimes,
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

    // will be updated later via StatCommand; delay is for banner to print first
    let mut bar = ProgressBar::hidden();

    let start = Instant::now();

    while let Some(command) = stats_channel.recv().await {
        log::info!("command: {:?}", command);
        match command as StatCommand {
            StatCommand::AddError(err) => {
                stats.add_error(err);
            }
            StatCommand::AddStatus(status) => {
                stats.add_status_code(status);
            }
            StatCommand::AddRequest => stats.add_request(),
            StatCommand::Save => stats.save(),
            StatCommand::UpdateUsizeField(field, value) => {
                let update_len = matches!(field, StatField::TotalScans);
                stats.update_usize_field(field, value);

                if update_len {
                    bar.set_length(atomic_load!(stats.total_expected) as u64)
                }
            }
            StatCommand::UpdateF64Field(field, value) => stats.update_f64_field(field, value),
            StatCommand::CreateBar => {
                bar = add_bar(
                    "",
                    atomic_load!(stats.total_expected) as u64,
                    BarType::Total,
                );
            }
            StatCommand::Exit => break,
        }

        let msg = format!(
            "{}:{:<7} {}:{:<7}",
            style("found").green(),
            atomic_load!(stats.resources_discovered),
            style("errors").red(),
            atomic_load!(stats.errors),
        );

        bar.set_message(&msg);
        bar.inc(1);
    }

    stats.update_runtime(start.elapsed().as_secs_f64());

    bar.finish();

    // stats.print_summary(&*PROGRESS_PRINTER);

    log::trace!("exit: spawn_statistics_handler")
}

/// Initialize new `Stats` object and the sc side of an mpsc channel that is responsible for
/// updates to the aforementioned object.
pub fn initialize() -> (Arc<Stats>, UnboundedSender<StatCommand>, JoinHandle<()>) {
    log::trace!("enter: initialize");

    let stats_tracker = Arc::new(Stats::new());
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
    /// when sent StatCommand::AddRequest, stats object should reflect the change
    async fn statistics_handler_increments_requests() {
        let (stats, tx, handle) = setup_stats_test();

        tx.send(StatCommand::AddRequest).unwrap_or_default();
        tx.send(StatCommand::AddRequest).unwrap_or_default();
        tx.send(StatCommand::AddRequest).unwrap_or_default();

        teardown_stats_test(tx, handle).await;

        assert_eq!(stats.requests.load(Ordering::Relaxed), 3);
    }

    #[tokio::test(core_threads = 1)]
    /// when sent StatCommand::AddRequest, stats object should reflect the change
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
    /// when sent StatCommand::AddRequest, stats object should reflect the change
    ///
    /// incrementing a 403 (tracked in status_403s) should also increment:
    ///     - requests
    ///     - client_errors
    async fn statistics_handler_increments_403_via_status_code() {
        let (stats, tx, handle) = setup_stats_test();

        let err = StatCommand::AddStatus(reqwest::StatusCode::FORBIDDEN);
        let err2 = StatCommand::AddStatus(reqwest::StatusCode::FORBIDDEN);

        tx.send(err).unwrap_or_default();
        tx.send(err2).unwrap_or_default();

        teardown_stats_test(tx, handle).await;

        assert_eq!(stats.requests.load(Ordering::Relaxed), 2);
        assert_eq!(stats.status_403s.load(Ordering::Relaxed), 2);
        assert_eq!(stats.client_errors.load(Ordering::Relaxed), 2);
    }

    #[tokio::test(core_threads = 1)]
    /// when sent StatCommand::AddStatus, stats object should reflect the change
    ///
    /// incrementing a 500 (tracked in server_errors) should also increment:
    ///     - requests
    async fn statistics_handler_increments_500_via_status_code() {
        let (stats, tx, handle) = setup_stats_test();

        let err = StatCommand::AddStatus(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        let err2 = StatCommand::AddStatus(reqwest::StatusCode::INTERNAL_SERVER_ERROR);

        tx.send(err).unwrap_or_default();
        tx.send(err2).unwrap_or_default();

        teardown_stats_test(tx, handle).await;

        assert_eq!(stats.requests.load(Ordering::Relaxed), 2);
        assert_eq!(stats.server_errors.load(Ordering::Relaxed), 2);
    }

    #[test]
    /// when Stats::add_error receives StatError::Timeout, it should increment the following:
    ///     - timeouts
    ///     - requests
    ///     - errors
    fn stats_increments_timeouts() {
        let stats = Stats::new();
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);

        assert_eq!(stats.errors.load(Ordering::Relaxed), 4);
        assert_eq!(stats.requests.load(Ordering::Relaxed), 4);
        assert_eq!(stats.timeouts.load(Ordering::Relaxed), 4);
    }

    #[test]
    /// when Stats::update_usize_field receives StatField::WildcardsFiltered, it should increment
    /// the following:
    ///     - responses_filtered
    fn stats_increments_wildcards() {
        let stats = Stats::new();
        assert_eq!(stats.responses_filtered.load(Ordering::Relaxed), 0);
        assert_eq!(stats.wildcards_filtered.load(Ordering::Relaxed), 0);

        stats.update_usize_field(StatField::WildcardsFiltered, 1);
        stats.update_usize_field(StatField::WildcardsFiltered, 1);

        assert_eq!(stats.responses_filtered.load(Ordering::Relaxed), 2);
        assert_eq!(stats.wildcards_filtered.load(Ordering::Relaxed), 2);
    }

    #[test]
    /// when Stats::update_usize_field receives StatField::ResponsesFiltered, it should increment
    fn stats_increments_responses_filtered() {
        let stats = Stats::new();
        assert_eq!(stats.responses_filtered.load(Ordering::Relaxed), 0);

        stats.update_usize_field(StatField::ResponsesFiltered, 1);
        stats.update_usize_field(StatField::ResponsesFiltered, 1);
        stats.update_usize_field(StatField::ResponsesFiltered, 1);

        assert_eq!(stats.responses_filtered.load(Ordering::Relaxed), 3);
    }
}
