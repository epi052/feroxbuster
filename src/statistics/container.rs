use std::{
    fs::File,
    io::BufReader,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{
    traits::FeroxSerialize,
    utils::{fmt_err, open_file, write_to},
};

use super::{error::StatError, field::StatField};

/// Data collection of statistics related to a scan
#[derive(Default, Deserialize, Debug, Serialize)]
pub struct Stats {
    #[serde(rename = "type")]
    /// Name of this type of struct, used for serialization, i.e. `{"type":"statistics"}`
    kind: String,

    /// tracker for number of timeouts seen by the client
    timeouts: AtomicUsize,

    /// tracker for total number of requests sent by the client
    pub(crate) requests: AtomicUsize,

    /// tracker for total number of requests expected to send if the scan runs to completion
    ///
    /// Note: this is a per-scan expectation; `expected_requests * current # of scans` would be
    /// indicative of the current expectation at any given time, but is a moving target.  
    expected_per_scan: AtomicUsize,

    /// tracker for accumulating total number of requests expected (i.e. as a new scan is started
    /// this value should increase by `expected_requests`
    total_expected: AtomicUsize,

    /// tracker for total number of errors encountered by the client
    pub(crate) errors: AtomicUsize,

    /// tracker for total number of errors that have been used to enforce a policy
    enforced_errors: AtomicUsize,

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
    pub(crate) total_scans: AtomicUsize,

    /// tracker for initial number of requested targets
    initial_targets: AtomicUsize,

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
    pub(crate) status_403s: AtomicUsize,

    /// tracker for total number of 403s that have been used to enforce a policy
    enforced_403s: AtomicUsize,

    /// tracker for overall number of 429s seen by the client
    pub(crate) status_429s: AtomicUsize,

    /// tracker for total number of 429s that have been used to enforce a policy
    enforced_429s: AtomicUsize,

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

    /// tracker for the number of extensions the user specified
    #[serde(skip)]
    num_extensions: usize,

    /// tracker for whether to use json during serialization or not
    #[serde(skip)]
    json: bool,
}

/// FeroxSerialize implementation for Stats
impl FeroxSerialize for Stats {
    /// Simply return empty string here to disable serializing this to the output file as a string
    /// due to it looking like garbage
    fn as_str(&self) -> String {
        String::new()
    }

    /// Simple call to produce a JSON string using the given Stats object
    fn as_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self)?)
    }
}

/// implementation of statistics data collection struct
impl Stats {
    /// Small wrapper for default to set `kind` to "statistics" and `total_runtime` to have at least
    /// one value
    pub fn new(num_extensions: usize, is_json: bool) -> Self {
        Self {
            num_extensions,
            json: is_json,
            kind: String::from("statistics"),
            total_runtime: Mutex::new(vec![0.0]),
            ..Default::default()
        }
    }

    /// public getter for expected_per_scan
    pub fn expected_per_scan(&self) -> usize {
        atomic_load!(self.expected_per_scan)
    }

    /// public getter for resources_discovered
    pub fn resources_discovered(&self) -> usize {
        atomic_load!(self.resources_discovered)
    }

    /// public getter for errors
    pub fn errors(&self) -> usize {
        atomic_load!(self.errors)
    }

    /// public getter for enforced_errors
    pub fn enforced_errors(&self) -> usize {
        atomic_load!(self.enforced_errors)
    }

    /// public getter for enforced_403s
    pub fn enforced_403s(&self) -> usize {
        atomic_load!(self.enforced_403s)
    }

    /// public getter for enforced_429s
    pub fn enforced_429s(&self) -> usize {
        atomic_load!(self.enforced_429s)
    }

    /// public getter for status_403s
    pub fn status_403s(&self) -> usize {
        atomic_load!(self.status_403s)
    }

    /// public getter for status_429s
    pub fn status_429s(&self) -> usize {
        atomic_load!(self.status_429s)
    }

    /// public getter for total_expected
    pub fn total_expected(&self) -> usize {
        atomic_load!(self.total_expected)
    }

    /// public getter for initial_targets
    pub fn initial_targets(&self) -> usize {
        atomic_load!(self.initial_targets)
    }

    /// increment `requests` field by one
    pub fn add_request(&self) {
        atomic_increment!(self.requests);
    }

    /// given an `Instant` update total runtime
    fn update_runtime(&self, seconds: f64) {
        if let Ok(mut runtime) = self.total_runtime.lock() {
            runtime[0] = seconds;
        }
    }

    /// save an instance of `Stats` to disk after updating the total runtime for the scan
    pub fn save(&self, seconds: f64, location: &str) -> Result<()> {
        let mut file = open_file(location)?;

        self.update_runtime(seconds);

        write_to(self, &mut file, self.json)?;

        Ok(())
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
            _ => {} // no need to hit Other as we always increment self.errors anyway
        }
    }

    /// Inspect the given `StatusCode` and increment the appropriate fields
    ///
    /// Implies incrementing:
    ///     - requests
    ///     - appropriate status_* codes
    ///     - errors (when code is [45]xx)
    pub fn add_status_code(&self, status: StatusCode) {
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
            StatusCode::FORBIDDEN => {
                atomic_increment!(self.status_403s);
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
    pub fn update_f64_field(&self, field: StatField, value: f64) {
        if let StatField::DirScanTimes = field {
            if let Ok(mut locked_times) = self.directory_scan_times.lock() {
                locked_times.push(value);
            }
        }
    }

    /// subtract a value from the given field
    pub fn subtract_from_usize_field(&self, field: StatField, value: usize) {
        if let StatField::TotalExpected = field {
            self.total_expected.fetch_sub(value, Ordering::Relaxed);
        }
    }

    /// Update a `Stats` field of type usize
    pub fn update_usize_field(&self, field: StatField, value: usize) {
        match field {
            StatField::ExpectedPerScan => {
                atomic_increment!(self.expected_per_scan, value);
            }
            StatField::TotalScans => {
                let multiplier = self.num_extensions.max(1);

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
            StatField::EnforcedErrors => {
                atomic_increment!(self.enforced_errors, value);
            }
            StatField::Enforced403s => {
                atomic_increment!(self.enforced_403s, value);
            }
            StatField::Enforced429s => {
                atomic_increment!(self.enforced_429s, value);
            }
            _ => {} // f64 fields
        }
    }

    /// Merge a given `Stats` object from a json entry written to disk when handling a Ctrl+c
    ///
    /// This is only ever called when resuming a scan from disk
    pub fn merge_from(&self, filename: &str) -> Result<()> {
        let file = File::open(filename)
            .with_context(|| fmt_err(&format!("Could not open {}", filename)))?;
        let reader = BufReader::new(file);
        let state: serde_json::Value = serde_json::from_reader(reader)?;

        if let Some(state_stats) = state.get("statistics") {
            let d_stats = serde_json::from_value::<Stats>(state_stats.clone())?;
            atomic_increment!(self.successes, atomic_load!(d_stats.successes));
            atomic_increment!(self.timeouts, atomic_load!(d_stats.timeouts));
            atomic_increment!(self.requests, atomic_load!(d_stats.requests));
            atomic_increment!(self.errors, atomic_load!(d_stats.errors));
            atomic_increment!(self.redirects, atomic_load!(d_stats.redirects));
            atomic_increment!(self.client_errors, atomic_load!(d_stats.client_errors));
            atomic_increment!(self.server_errors, atomic_load!(d_stats.server_errors));
            atomic_increment!(self.links_extracted, atomic_load!(d_stats.links_extracted));
            atomic_increment!(self.enforced_errors, atomic_load!(d_stats.enforced_errors));
            atomic_increment!(self.enforced_403s, atomic_load!(d_stats.enforced_403s));
            atomic_increment!(self.enforced_429s, atomic_load!(d_stats.enforced_429s));
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
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{config::Configuration, Command};
    use std::fs::write;
    use tempfile::NamedTempFile;

    use super::super::*;
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// when sent StatCommand::AddRequest, stats object should reflect the change
    async fn statistics_handler_increments_requests() -> Result<()> {
        let (task, handle) = setup_stats_test();

        handle.tx.send(Command::AddRequest)?;
        handle.tx.send(Command::AddRequest)?;
        handle.tx.send(Command::AddRequest)?;

        teardown_stats_test(handle.tx.clone(), task).await;

        assert_eq!(handle.data.requests.load(Ordering::Relaxed), 3);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// when sent StatCommand::AddRequest, stats object should reflect the change
    ///
    /// incrementing a 403 (tracked in status_403s) should also increment:
    ///     - requests
    ///     - client_errors
    async fn statistics_handler_increments_403_via_status_code() {
        let (task, handle) = setup_stats_test();

        let err = Command::AddStatus(reqwest::StatusCode::FORBIDDEN);
        let err2 = Command::AddStatus(reqwest::StatusCode::FORBIDDEN);

        handle.tx.send(err).unwrap_or_default();
        handle.tx.send(err2).unwrap_or_default();

        teardown_stats_test(handle.tx.clone(), task).await;

        assert_eq!(handle.data.requests.load(Ordering::Relaxed), 2);
        assert_eq!(handle.data.status_403s.load(Ordering::Relaxed), 2);
        assert_eq!(handle.data.client_errors.load(Ordering::Relaxed), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// when sent StatCommand::AddStatus, stats object should reflect the change
    ///
    /// incrementing a 500 (tracked in server_errors) should also increment:
    ///     - requests
    async fn statistics_handler_increments_500_via_status_code() -> Result<()> {
        let (task, handle) = setup_stats_test();

        let err = Command::AddStatus(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        let err2 = Command::AddStatus(reqwest::StatusCode::INTERNAL_SERVER_ERROR);

        handle.tx.send(err)?;
        handle.tx.send(err2)?;

        teardown_stats_test(handle.tx.clone(), task).await;

        assert_eq!(handle.data.requests.load(Ordering::Relaxed), 2);
        assert_eq!(handle.data.server_errors.load(Ordering::Relaxed), 2);

        Ok(())
    }

    #[test]
    /// when Stats::add_error receives StatError::Timeout, it should increment the following:
    ///     - timeouts
    ///     - requests
    ///     - errors
    fn stats_increments_timeouts() {
        let config = Configuration::new().unwrap();
        let stats = Stats::new(config.extensions.len(), config.json);

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
        let config = Configuration::new().unwrap();
        let stats = Stats::new(config.extensions.len(), config.json);

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
        let config = Configuration::new().unwrap();
        let stats = Stats::new(config.extensions.len(), config.json);

        assert_eq!(stats.responses_filtered.load(Ordering::Relaxed), 0);

        stats.update_usize_field(StatField::ResponsesFiltered, 1);
        stats.update_usize_field(StatField::ResponsesFiltered, 1);
        stats.update_usize_field(StatField::ResponsesFiltered, 1);

        assert_eq!(stats.responses_filtered.load(Ordering::Relaxed), 3);
    }

    #[test]
    /// Stats::merge_from should properly increment expected fields and ignore others
    fn stats_merge_from_alters_correct_fields() {
        let contents = r#"{"statistics":{"type":"statistics","timeouts":1,"requests":9207,"expected_per_scan":707,"total_expected":9191,"errors":3,"successes":720,"enforced_errors":41,"enforced_429s":42,"enforced_403s":43,"redirects":13,"client_errors":8474,"server_errors":2,"total_scans":13,"initial_targets":1,"links_extracted":51,"status_403s":3,"status_200s":720,"status_301s":12,"status_302s":1,"status_401s":4,"status_429s":2,"status_500s":5,"status_503s":9,"status_504s":6,"status_508s":7,"wildcards_filtered":707,"responses_filtered":707,"resources_discovered":27,"directory_scan_times":[2.211973078,1.989015505,1.898675839,3.9714468910000003,4.938152838,5.256073528,6.021986595,6.065740734,6.42633762,7.095142125,7.336982137,5.319785619,4.843649778],"total_runtime":[11.556575456000001],"url_format_errors":17,"redirection_errors":12,"connection_errors":21,"request_errors":4}}"#;
        let config = Configuration::new().unwrap();
        let stats = Stats::new(config.extensions.len(), config.json);

        let tfile = NamedTempFile::new().unwrap();
        write(&tfile, contents).unwrap();

        stats.merge_from(tfile.path().to_str().unwrap()).unwrap();

        // as of 2.1.0; all Stats fields are accounted for whether they're updated in merge_from
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
        assert_eq!(atomic_load!(stats.enforced_errors), 41);
        assert_eq!(atomic_load!(stats.enforced_429s), 42);
        assert_eq!(atomic_load!(stats.enforced_403s), 43);
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
        let config = Configuration::new().unwrap();
        let stats = Stats::new(config.extensions.len(), config.json);

        assert!((stats.total_runtime.lock().unwrap()[0] - 0.0).abs() < f64::EPSILON);
        stats.update_runtime(20.2);
        assert!((stats.total_runtime.lock().unwrap()[0] - 20.2).abs() < f64::EPSILON);
    }
}
