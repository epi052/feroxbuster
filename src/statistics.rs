use crate::{
    config::CONFIGURATION,
    progress::{add_bar, BarType},
    reporter::{get_cached_file_handle, safe_file_write},
    FeroxChannel, FeroxSerialize,
};
use console::style;
use indicatif::ProgressBar;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::BufReader,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

/// Wrapper `Atomic*.fetch_add` to save me from writing Ordering::Relaxed a bajillion times
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

/// Wrapper around `Atomic*.load` to save me from writing Ordering::Relaxed a bajillion times
macro_rules! atomic_load {
    ($metric:expr) => {
        $metric.load(Ordering::Relaxed);
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

    /// tracker for overall number of 200s seen by the client
    status_200s: AtomicUsize,

    /// tracker for overall number of 301s seen by the client
    status_301s: AtomicUsize,

    /// tracker for overall number of 302s seen by the client
    status_302s: AtomicUsize,

    /// tracker for overall number of 401s seen by the client
    status_401s: AtomicUsize,

    /// tracker for overall number of 403s seen by the client
    status_403s: AtomicUsize,

    /// tracker for overall number of 429s seen by the client
    status_429s: AtomicUsize,

    /// tracker for overall number of 500s seen by the client
    status_500s: AtomicUsize,

    /// tracker for overall number of 503s seen by the client
    status_503s: AtomicUsize,

    /// tracker for overall number of 504s seen by the client
    status_504s: AtomicUsize,

    /// tracker for overall number of 508s seen by the client
    status_508s: AtomicUsize,

    /// tracker for overall number of wildcard urls filtered out by the client
    wildcards_filtered: AtomicUsize,

    /// tracker for overall number of all filtered responses
    responses_filtered: AtomicUsize,

    /// tracker for number of files found
    resources_discovered: AtomicUsize,

    /// tracker for number of errors triggered during URL formatting
    url_format_errors: AtomicUsize,

    /// tracker for number of errors triggered by the `reqwest::RedirectPolicy`
    redirection_errors: AtomicUsize,

    /// tracker for number of errors related to the connecting
    connection_errors: AtomicUsize,

    /// tracker for number of errors related to the request used
    request_errors: AtomicUsize,

    /// tracker for each directory's total scan time in seconds as a float
    directory_scan_times: Mutex<Vec<f64>>,

    /// tracker for total runtime
    total_runtime: Mutex<Vec<f64>>,
}

/// FeroxSerialize implementation for Stats
impl FeroxSerialize for Stats {
    /// Simply return empty string here to disable serializing this to the output file as a string
    /// due to it looking like garbage
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

    /// save an instance of `Stats` to disk after updating the total runtime for the scan
    fn save(&self, seconds: f64, location: &str) {
        let buffered_file = match get_cached_file_handle(location) {
            Some(file) => file,
            None => {
                return;
            }
        };

        self.update_runtime(seconds);

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

        match status {
            StatusCode::FORBIDDEN => {
                atomic_increment!(self.status_403s);
            }
            StatusCode::OK => {
                atomic_increment!(self.status_200s);
            }
            StatusCode::MOVED_PERMANENTLY => {
                atomic_increment!(self.status_301s);
            }
            StatusCode::FOUND => {
                atomic_increment!(self.status_302s);
            }
            StatusCode::UNAUTHORIZED => {
                atomic_increment!(self.status_401s);
            }
            StatusCode::TOO_MANY_REQUESTS => {
                atomic_increment!(self.status_429s);
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                atomic_increment!(self.status_500s);
            }
            StatusCode::SERVICE_UNAVAILABLE => {
                atomic_increment!(self.status_503s);
            }
            StatusCode::GATEWAY_TIMEOUT => {
                atomic_increment!(self.status_504s);
            }
            StatusCode::LOOP_DETECTED => {
                atomic_increment!(self.status_508s);
            }
            _ => {} // other status codes ignored for stat gathering
        }
    }

    /// Update a `Stats` field of type f64
    fn update_f64_field(&self, field: StatField, value: f64) {
        if let StatField::DirScanTimes = field {
            if let Ok(mut locked_times) = self.directory_scan_times.lock() {
                locked_times.push(value);
            }
        }
    }

    /// Update a `Stats` field of type usize
    fn update_usize_field(&self, field: StatField, value: usize) {
        match field {
            StatField::ExpectedPerScan => {
                atomic_increment!(self.expected_per_scan, value);
            }
            StatField::TotalScans => {
                let multiplier = CONFIGURATION.extensions.len().max(1);

                atomic_increment!(self.total_scans, value);
                atomic_increment!(
                    self.total_expected,
                    value * self.expected_per_scan.load(Ordering::Relaxed) * multiplier
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

    /// Merge a given `Stats` object from a json entry written to disk when handling a Ctrl+c
    ///
    /// This is only ever called when resuming a scan from disk
    pub fn merge_from(&self, filename: &str) {
        if let Ok(file) = File::open(filename) {
            let reader = BufReader::new(file);
            let state: serde_json::Value = serde_json::from_reader(reader).unwrap();

            if let Some(state_stats) = state.get("statistics") {
                if let Ok(d_stats) = serde_json::from_value::<Stats>(state_stats.clone()) {
                    atomic_increment!(self.successes, atomic_load!(d_stats.successes));
                    atomic_increment!(self.timeouts, atomic_load!(d_stats.timeouts));
                    atomic_increment!(self.requests, atomic_load!(d_stats.requests));
                    atomic_increment!(self.errors, atomic_load!(d_stats.errors));
                    atomic_increment!(self.redirects, atomic_load!(d_stats.redirects));
                    atomic_increment!(self.client_errors, atomic_load!(d_stats.client_errors));
                    atomic_increment!(self.server_errors, atomic_load!(d_stats.server_errors));
                    atomic_increment!(self.links_extracted, atomic_load!(d_stats.links_extracted));
                    atomic_increment!(self.status_200s, atomic_load!(d_stats.status_200s));
                    atomic_increment!(self.status_301s, atomic_load!(d_stats.status_301s));
                    atomic_increment!(self.status_302s, atomic_load!(d_stats.status_302s));
                    atomic_increment!(self.status_401s, atomic_load!(d_stats.status_401s));
                    atomic_increment!(self.status_403s, atomic_load!(d_stats.status_403s));
                    atomic_increment!(self.status_429s, atomic_load!(d_stats.status_429s));
                    atomic_increment!(self.status_500s, atomic_load!(d_stats.status_500s));
                    atomic_increment!(self.status_503s, atomic_load!(d_stats.status_503s));
                    atomic_increment!(self.status_504s, atomic_load!(d_stats.status_504s));
                    atomic_increment!(self.status_508s, atomic_load!(d_stats.status_508s));
                    atomic_increment!(
                        self.wildcards_filtered,
                        atomic_load!(d_stats.wildcards_filtered)
                    );
                    atomic_increment!(
                        self.responses_filtered,
                        atomic_load!(d_stats.responses_filtered)
                    );
                    atomic_increment!(
                        self.resources_discovered,
                        atomic_load!(d_stats.resources_discovered)
                    );
                    atomic_increment!(
                        self.url_format_errors,
                        atomic_load!(d_stats.url_format_errors)
                    );
                    atomic_increment!(
                        self.connection_errors,
                        atomic_load!(d_stats.connection_errors)
                    );
                    atomic_increment!(
                        self.redirection_errors,
                        atomic_load!(d_stats.redirection_errors)
                    );
                    atomic_increment!(self.request_errors, atomic_load!(d_stats.request_errors));

                    if let Ok(scan_times) = d_stats.directory_scan_times.lock() {
                        for scan_time in scan_times.iter() {
                            self.update_f64_field(StatField::DirScanTimes, *scan_time);
                        }
                    }
                }
            }
        }
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

    /// Load a `Stats` object from disk
    LoadStats(String),

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
    mut rx_stats: UnboundedReceiver<StatCommand>,
    stats: Arc<Stats>,
    tx_stats: UnboundedSender<StatCommand>,
) {
    log::trace!(
        "enter: spawn_statistics_handler({:?}, {:?}, {:?})",
        rx_stats,
        stats,
        tx_stats
    );

    // will be updated later via StatCommand; delay is for banner to print first
    let mut bar = ProgressBar::hidden();

    let start = Instant::now();

    while let Some(command) = rx_stats.recv().await {
        match command as StatCommand {
            StatCommand::AddError(err) => {
                stats.add_error(err);
                increment_bar(&bar, stats.clone());
            }
            StatCommand::AddStatus(status) => {
                stats.add_status_code(status);
                increment_bar(&bar, stats.clone());
            }
            StatCommand::AddRequest => {
                stats.add_request();
                increment_bar(&bar, stats.clone());
            }
            StatCommand::Save => stats.save(start.elapsed().as_secs_f64(), &CONFIGURATION.output),
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
            StatCommand::LoadStats(filename) => {
                stats.merge_from(&filename);
            }
            StatCommand::Exit => break,
        }
    }

    bar.finish();

    log::debug!("{:#?}", *stats);
    log::trace!("exit: spawn_statistics_handler")
}

/// Wrapper around incrementing the overall scan's progress bar
fn increment_bar(bar: &ProgressBar, stats: Arc<Stats>) {
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

/// Initialize new `Stats` object and the sc side of an mpsc channel that is responsible for
/// updates to the aforementioned object.
pub fn initialize() -> (Arc<Stats>, UnboundedSender<StatCommand>, JoinHandle<()>) {
    log::trace!("enter: initialize");

    let stats_tracker = Arc::new(Stats::new());
    let stats_cloned = stats_tracker.clone();
    let (tx_stats, rx_stats): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
    let tx_stats_cloned = tx_stats.clone();
    let stats_thread = tokio::spawn(async move {
        spawn_statistics_handler(rx_stats, stats_cloned, tx_stats_cloned).await
    });

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
    use std::fs::write;
    use tempfile::NamedTempFile;

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// when sent StatCommand::Exit, function should exit its while loop (runs forever otherwise)
    async fn statistics_handler_exits() {
        let (_, sender, handle) = setup_stats_test();

        sender.send(StatCommand::Exit).unwrap_or_default();

        handle.await.unwrap(); // blocks on the handler's while loop

        // if we've made it here, the test has succeeded
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// when sent StatCommand::AddRequest, stats object should reflect the change
    async fn statistics_handler_increments_requests() {
        let (stats, tx, handle) = setup_stats_test();

        tx.send(StatCommand::AddRequest).unwrap_or_default();
        tx.send(StatCommand::AddRequest).unwrap_or_default();
        tx.send(StatCommand::AddRequest).unwrap_or_default();

        teardown_stats_test(tx, handle).await;

        assert_eq!(stats.requests.load(Ordering::Relaxed), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
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

    #[test]
    /// Stats::merge_from should properly incrememnt expected fields and ignore others
    fn stats_merge_from_alters_correct_fields() {
        let contents = r#"{"statistics":{"type":"statistics","timeouts":1,"requests":9207,"expected_per_scan":707,"total_expected":9191,"errors":3,"successes":720,"redirects":13,"client_errors":8474,"server_errors":2,"total_scans":13,"initial_targets":1,"links_extracted":51,"status_403s":3,"status_200s":720,"status_301s":12,"status_302s":1,"status_401s":4,"status_429s":2,"status_500s":5,"status_503s":9,"status_504s":6,"status_508s":7,"wildcards_filtered":707,"responses_filtered":707,"resources_discovered":27,"directory_scan_times":[2.211973078,1.989015505,1.898675839,3.9714468910000003,4.938152838,5.256073528,6.021986595,6.065740734,6.42633762,7.095142125,7.336982137,5.319785619,4.843649778],"total_runtime":[11.556575456000001],"url_format_errors":17,"redirection_errors":12,"connection_errors":21,"request_errors":4}}"#;
        let stats = Stats::new();
        let tfile = NamedTempFile::new().unwrap();
        write(&tfile, contents).unwrap();

        stats.merge_from(tfile.path().to_str().unwrap());

        // as of 1.11.1; all Stats fields are accounted for whether they're updated in merge_from
        // or not
        assert_eq!(atomic_load!(stats.timeouts), 1);
        assert_eq!(atomic_load!(stats.requests), 9207);
        assert_eq!(atomic_load!(stats.expected_per_scan), 0); // not updated in merge_from
        assert_eq!(atomic_load!(stats.total_expected), 0); // not updated in merge_from
        assert_eq!(atomic_load!(stats.errors), 3);
        assert_eq!(atomic_load!(stats.successes), 720);
        assert_eq!(atomic_load!(stats.redirects), 13);
        assert_eq!(atomic_load!(stats.client_errors), 8474);
        assert_eq!(atomic_load!(stats.server_errors), 2);
        assert_eq!(atomic_load!(stats.total_scans), 0); // not updated in merge_from
        assert_eq!(atomic_load!(stats.initial_targets), 0); // not updated in merge_from
        assert_eq!(atomic_load!(stats.links_extracted), 51);
        assert_eq!(atomic_load!(stats.status_200s), 720);
        assert_eq!(atomic_load!(stats.status_301s), 12);
        assert_eq!(atomic_load!(stats.status_302s), 1);
        assert_eq!(atomic_load!(stats.status_401s), 4);
        assert_eq!(atomic_load!(stats.status_403s), 3);
        assert_eq!(atomic_load!(stats.status_429s), 2);
        assert_eq!(atomic_load!(stats.status_500s), 5);
        assert_eq!(atomic_load!(stats.status_503s), 9);
        assert_eq!(atomic_load!(stats.status_504s), 6);
        assert_eq!(atomic_load!(stats.status_508s), 7);
        assert_eq!(atomic_load!(stats.wildcards_filtered), 707);
        assert_eq!(atomic_load!(stats.responses_filtered), 707);
        assert_eq!(atomic_load!(stats.resources_discovered), 27);
        assert_eq!(atomic_load!(stats.url_format_errors), 17);
        assert_eq!(atomic_load!(stats.redirection_errors), 12);
        assert_eq!(atomic_load!(stats.connection_errors), 21);
        assert_eq!(atomic_load!(stats.request_errors), 4);
        assert_eq!(stats.directory_scan_times.lock().unwrap().len(), 13);
        for scan in stats.directory_scan_times.lock().unwrap().iter() {
            assert!(scan.max(0.0) > 0.0); // all scans are non-zero
        }
        // total_runtime not updated in merge_from
        assert_eq!(stats.total_runtime.lock().unwrap().len(), 1);
        assert!((stats.total_runtime.lock().unwrap()[0] - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    /// ensure update runtime overwrites the default 0th entry
    fn update_runtime_works() {
        let stats = Stats::new();
        assert!((stats.total_runtime.lock().unwrap()[0] - 0.0).abs() < f64::EPSILON);
        stats.update_runtime(20.2);
        assert!((stats.total_runtime.lock().unwrap()[0] - 20.2).abs() < f64::EPSILON);
    }

    #[test]
    /// Stats::save should write contents of Stats to disk
    fn save_writes_stats_object_to_disk() {
        let stats = Stats::new();
        stats.add_request();
        stats.add_request();
        stats.add_request();
        stats.add_request();
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);
        stats.add_error(StatError::Timeout);
        stats.add_status_code(StatusCode::OK);
        stats.add_status_code(StatusCode::OK);
        stats.add_status_code(StatusCode::OK);
        let outfile = "/tmp/stuff";
        stats.save(174.33, outfile);
        assert!(stats.as_json().contains("statistics"));
        assert!(stats.as_json().contains("11")); // requests made
        assert!(stats.as_str().is_empty());
    }
}
