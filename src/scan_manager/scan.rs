use super::*;
use crate::{
    config::OutputLevel,
    progress::{add_bar, BarType},
    scanner::PolicyTrigger,
};
use anyhow::Result;
use console::style;
use indicatif::ProgressBar;
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::Instant,
};

use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{sync, task::JoinHandle};
use uuid::Uuid;

/// Struct to hold scan-related state
///
/// The purpose of this container is to open up the pathway to aborting currently running tasks and
/// serialization of all scan state into a state file in order to resume scans that were cut short
#[derive(Debug)]
pub struct FeroxScan {
    /// UUID that uniquely ID's the scan
    pub(super) id: String,

    /// The URL that to be scanned
    pub(super) url: String,

    /// The type of scan
    pub scan_type: ScanType,

    /// The order in which the scan was received
    pub(crate) scan_order: ScanOrder,

    /// Number of requests to populate the progress bar with
    pub(super) num_requests: u64,

    /// Status of this scan
    pub status: Mutex<ScanStatus>,

    /// The spawned tokio task performing this scan (uses tokio::sync::Mutex)
    pub(super) task: sync::Mutex<Option<JoinHandle<()>>>,

    /// The progress bar associated with this scan
    pub(super) progress_bar: Mutex<Option<ProgressBar>>,

    /// whether or not the user passed --silent|--quiet on the command line
    pub(super) output_level: OutputLevel,

    /// tracker for overall number of 403s seen by the FeroxScan instance
    pub(super) status_403s: AtomicUsize,

    /// tracker for overall number of 429s seen by the FeroxScan instance
    pub(super) status_429s: AtomicUsize,

    /// tracker for total number of errors encountered by the FeroxScan instance
    pub(super) errors: AtomicUsize,

    /// tracker for the time at which this scan was started
    pub(super) start_time: Instant,
}

/// Default implementation for FeroxScan
impl Default for FeroxScan {
    /// Create a default FeroxScan, populates ID with a new UUID
    fn default() -> Self {
        let new_id = Uuid::new_v4().to_simple().to_string();

        FeroxScan {
            id: new_id,
            task: sync::Mutex::new(None), // tokio mutex
            status: Mutex::new(ScanStatus::default()),
            num_requests: 0,
            scan_order: ScanOrder::Latest,
            url: String::new(),
            progress_bar: Mutex::new(None),
            scan_type: ScanType::File,
            output_level: Default::default(),
            errors: Default::default(),
            status_429s: Default::default(),
            status_403s: Default::default(),
            start_time: Instant::now(),
        }
    }
}

/// Implementation of FeroxScan
impl FeroxScan {
    /// Stop a currently running scan
    pub async fn abort(&self) -> Result<()> {
        log::trace!("enter: abort");

        match self.task.try_lock() {
            Ok(mut guard) => {
                if let Some(task) = std::mem::replace(&mut *guard, None) {
                    log::trace!("aborting {:?}", self);
                    task.abort();
                    self.set_status(ScanStatus::Cancelled)?;
                    self.stop_progress_bar();
                }
            }
            Err(e) => {
                log::warn!("Could not acquire lock to abort scan (we're already waiting for its results): {:?} {}", self, e);
            }
        }
        log::trace!("exit: abort");
        Ok(())
    }

    /// getter for url
    pub fn url(&self) -> &str {
        &self.url
    }

    /// small wrapper to set the JoinHandle
    pub async fn set_task(&self, task: JoinHandle<()>) -> Result<()> {
        let mut guard = self.task.lock().await;
        let _ = std::mem::replace(&mut *guard, Some(task));
        Ok(())
    }

    /// small wrapper to set ScanStatus
    pub fn set_status(&self, status: ScanStatus) -> Result<()> {
        if let Ok(mut guard) = self.status.lock() {
            let _ = std::mem::replace(&mut *guard, status);
        }
        Ok(())
    }

    /// Simple helper to call .finish on the scan's progress bar
    pub(super) fn stop_progress_bar(&self) {
        if let Ok(guard) = self.progress_bar.lock() {
            if guard.is_some() {
                (*guard).as_ref().unwrap().finish_at_current_pos()
            }
        }
    }

    /// Simple helper get a progress bar
    pub fn progress_bar(&self) -> ProgressBar {
        match self.progress_bar.lock() {
            Ok(mut guard) => {
                if guard.is_some() {
                    (*guard).as_ref().unwrap().clone()
                } else {
                    let bar_type = match self.output_level {
                        OutputLevel::Default => BarType::Default,
                        OutputLevel::Quiet => BarType::Quiet,
                        OutputLevel::Silent => BarType::Hidden,
                    };

                    let pb = add_bar(&self.url, self.num_requests, bar_type);
                    pb.reset_elapsed();

                    let _ = std::mem::replace(&mut *guard, Some(pb.clone()));

                    pb
                }
            }
            Err(_) => {
                log::warn!("Could not unlock progress bar on {:?}", self);

                let bar_type = match self.output_level {
                    OutputLevel::Default => BarType::Default,
                    OutputLevel::Quiet => BarType::Quiet,
                    OutputLevel::Silent => BarType::Hidden,
                };

                let pb = add_bar(&self.url, self.num_requests, bar_type);
                pb.reset_elapsed();

                pb
            }
        }
    }

    /// Given a URL and ProgressBar, create a new FeroxScan, wrap it in an Arc and return it
    pub fn new(
        url: &str,
        scan_type: ScanType,
        scan_order: ScanOrder,
        num_requests: u64,
        output_level: OutputLevel,
        pb: Option<ProgressBar>,
    ) -> Arc<Self> {
        Arc::new(Self {
            url: url.to_string(),
            scan_type,
            scan_order,
            num_requests,
            output_level,
            progress_bar: Mutex::new(pb),
            ..Default::default()
        })
    }

    /// Mark the scan as complete and stop the scan's progress bar
    pub fn finish(&self) -> Result<()> {
        self.set_status(ScanStatus::Complete)?;
        self.stop_progress_bar();
        Ok(())
    }

    /// small wrapper to inspect ScanType and ScanStatus to see if a Directory scan is running or
    /// in the queue to be run
    pub fn is_active(&self) -> bool {
        if let Ok(guard) = self.status.lock() {
            return matches!(
                (self.scan_type, *guard),
                (ScanType::Directory, ScanStatus::Running)
                    | (ScanType::Directory, ScanStatus::NotStarted)
            );
        }
        false
    }

    /// small wrapper to inspect ScanStatus and see if it's Complete
    pub fn is_complete(&self) -> bool {
        if let Ok(guard) = self.status.lock() {
            return matches!(*guard, ScanStatus::Complete);
        }
        false
    }

    /// await a task's completion, similar to a thread's join; perform necessary bookkeeping
    pub async fn join(&self) {
        log::trace!("enter join({:?})", self);
        let mut guard = self.task.lock().await;

        if guard.is_some() {
            if let Some(task) = std::mem::replace(&mut *guard, None) {
                task.await.unwrap();
                self.set_status(ScanStatus::Complete)
                    .unwrap_or_else(|e| log::warn!("Could not mark scan complete: {}", e))
            }
        }

        log::trace!("exit join({:?})", self);
    }
    /// increment the value in question by 1
    pub(crate) fn add_403(&self) {
        self.status_403s.fetch_add(1, Ordering::Relaxed);
    }

    /// increment the value in question by 1
    pub(crate) fn add_429(&self) {
        self.status_429s.fetch_add(1, Ordering::Relaxed);
    }

    /// increment the value in question by 1
    pub(crate) fn add_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// simple wrapper to call the appropriate getter based on the given PolicyTrigger
    pub fn num_errors(&self, trigger: PolicyTrigger) -> usize {
        match trigger {
            PolicyTrigger::Status403 => self.status_403s(),
            PolicyTrigger::Status429 => self.status_429s(),
            PolicyTrigger::Errors => self.errors(),
        }
    }

    /// return the number of errors seen by this scan
    fn errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }

    /// return the number of 403s seen by this scan
    fn status_403s(&self) -> usize {
        self.status_403s.load(Ordering::Relaxed)
    }

    /// return the number of 429s seen by this scan
    fn status_429s(&self) -> usize {
        self.status_429s.load(Ordering::Relaxed)
    }

    /// return the number of requests per second performed by this scan's scanner
    pub fn requests_per_second(&self) -> u64 {
        if !self.is_active() {
            return 0;
        }

        let reqs = self.requests();
        let seconds = self.start_time.elapsed().as_secs();

        reqs.checked_div(seconds).unwrap_or(0)
    }

    /// return the number of requests performed by this scan's scanner
    pub fn requests(&self) -> u64 {
        self.progress_bar().position()
    }
}

/// Display implementation
impl fmt::Display for FeroxScan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if let Ok(guard) = self.status.lock() {
            match *guard {
                ScanStatus::NotStarted => style("not started").bright().blue(),
                ScanStatus::Complete => style("complete").green(),
                ScanStatus::Cancelled => style("cancelled").red(),
                ScanStatus::Running => style("running").bright().yellow(),
            }
        } else {
            style("unknown").red()
        };

        write!(f, "{:12} {}", status, self.url)
    }
}

/// PartialEq implementation; uses FeroxScan.id for comparison
impl PartialEq for FeroxScan {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Serialize implementation for FeroxScan
impl Serialize for FeroxScan {
    /// Function that handles serialization of a FeroxScan
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("FeroxScan", 4)?;

        state.serialize_field("id", &self.id)?;
        state.serialize_field("url", &self.url)?;
        state.serialize_field("scan_type", &self.scan_type)?;
        state.serialize_field("status", &self.status)?;
        state.serialize_field("num_requests", &self.num_requests)?;

        state.end()
    }
}

/// Deserialize implementation for FeroxScan
impl<'de> Deserialize<'de> for FeroxScan {
    /// Deserialize a FeroxScan from a serde_json::Value
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut scan = Self::default();

        let map: HashMap<String, Value> = HashMap::deserialize(deserializer)?;

        for (key, value) in &map {
            match key.as_str() {
                "id" => {
                    if let Some(id) = value.as_str() {
                        scan.id = id.to_string();
                    }
                }
                "scan_type" => {
                    if let Some(scan_type) = value.as_str() {
                        scan.scan_type = match scan_type {
                            "File" => ScanType::File,
                            "Directory" => ScanType::Directory,
                            _ => ScanType::File,
                        }
                    }
                }
                "status" => {
                    if let Some(status) = value.as_str() {
                        scan.status = Mutex::new(match status {
                            "NotStarted" => ScanStatus::NotStarted,
                            "Running" => ScanStatus::Running,
                            "Complete" => ScanStatus::Complete,
                            "Cancelled" => ScanStatus::Cancelled,
                            _ => ScanStatus::default(),
                        })
                    }
                }
                "url" => {
                    if let Some(url) = value.as_str() {
                        scan.url = url.to_string();
                    }
                }
                "num_requests" => {
                    if let Some(num_requests) = value.as_u64() {
                        scan.num_requests = num_requests;
                    }
                }
                _ => {}
            }
        }

        Ok(scan)
    }
}

/// Simple enum used to flag a `FeroxScan` as likely a directory or file
#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum ScanType {
    /// Just a file being requested
    File,

    /// A an entire directory that might be scanned
    Directory,
}

/// Default implementation for ScanType
impl Default for ScanType {
    /// Return ScanType::File as default
    fn default() -> Self {
        Self::File
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
/// Simple enum to represent a scan's current status ([in]complete, cancelled)
pub enum ScanStatus {
    /// Scan hasn't started yet
    NotStarted,

    /// Scan finished normally
    Complete,

    /// Scan was cancelled by the user
    Cancelled,

    /// Scan has started, but hasn't finished, nor been cancelled
    Running,
}

/// Default implementation for ScanStatus
impl Default for ScanStatus {
    /// Default variant for ScanStatus is NotStarted
    fn default() -> Self {
        Self::NotStarted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use tokio::time::Duration;

    #[test]
    /// ensure that num_errors returns the correct values for the given PolicyTrigger
    ///
    /// covers tests for add_[403,429,error] and the related getters in addition to num_errors
    fn num_errors_returns_correct_values() {
        let scan = FeroxScan::new(
            "http://localhost",
            ScanType::Directory,
            ScanOrder::Latest,
            1000,
            OutputLevel::Default,
            None,
        );

        scan.add_error();
        scan.add_403();
        scan.add_403();
        scan.add_429();
        scan.add_429();
        scan.add_429();

        assert_eq!(scan.num_errors(PolicyTrigger::Errors), 1);
        assert_eq!(scan.num_errors(PolicyTrigger::Status403), 2);
        assert_eq!(scan.num_errors(PolicyTrigger::Status429), 3);
    }

    #[test]
    /// ensure that requests_per_second returns the correct values
    fn requests_per_second_returns_correct_values() {
        let scan = FeroxScan {
            id: "".to_string(),
            url: "".to_string(),
            scan_type: ScanType::Directory,
            scan_order: ScanOrder::Initial,
            num_requests: 0,
            status: Mutex::new(ScanStatus::Running),
            task: Default::default(),
            progress_bar: Mutex::new(None),
            output_level: Default::default(),
            status_403s: Default::default(),
            status_429s: Default::default(),
            errors: Default::default(),
            start_time: Instant::now(),
        };

        let pb = scan.progress_bar();
        pb.set_position(100);

        sleep(Duration::new(1, 0));

        let req_sec = scan.requests_per_second();

        assert_eq!(req_sec, 100);

        scan.finish().unwrap();
        assert_eq!(scan.requests_per_second(), 0);
    }
}
