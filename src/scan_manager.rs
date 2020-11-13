use crate::{config::PROGRESS_PRINTER, progress, SLEEP_DURATION, scanner::NUMBER_OF_REQUESTS};
use console::style;
use indicatif::ProgressBar;
use std::{
    fmt,
    cmp::PartialEq,
    sync::{Arc, Mutex},
};
use std::{
    io::{stderr, Write},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::{task::JoinHandle, time};
use uuid::Uuid;

/// Single atomic number that gets incremented once, used to track first thread to interact with
/// when pausing a scan
static INTERACTIVE_BARRIER: AtomicUsize = AtomicUsize::new(0);

/// Atomic boolean flag, used to determine whether or not a scan should pause or resume
pub static PAUSE_SCAN: AtomicBool = AtomicBool::new(false);

/// Struct to hold scan-related state
///
/// The purpose of this container is to open up the pathway to aborting currently running tasks and
/// serialization of all scan state into a state file in order to resume scans that were cut short
#[derive(Debug)]
pub struct FeroxScan {
    /// UUID that uniquely ID's the scan
    pub id: String,

    /// The URL that to be scanned
    pub url: String,

    /// Whether or not this scan has completed
    pub complete: bool,

    /// The spawned tokio task performing this scan
    pub task: Option<JoinHandle<()>>,

    /// The progress bar associated with this scan
    pub progress_bar: Option<ProgressBar>,
}

/// Implementation of FeroxScan
impl FeroxScan {
    /// Stop a currently running scan
    pub fn abort(&self) {
        if let Some(_task) = &self.task {
            // task.abort();  todo uncomment once upgraded to tokio 0.3
        }
        self.stop_progress_bar();
    }

    /// Create a default FeroxScan, populates ID with a new UUID
    fn default() -> Self {
        let new_id = Uuid::new_v4().to_simple().to_string();

        FeroxScan {
            id: new_id,
            complete: false,
            url: String::new(),
            task: None,
            progress_bar: None,
        }
    }

    /// Simple helper to call .finish on the scan's progress bar
    fn stop_progress_bar(&self) {
        if let Some(pb) = &self.progress_bar {
            pb.finish();
        }
    }

    /// Given a URL and ProgressBar, create a new FeroxScan, wrap it in an Arc and return it
    pub fn new(url: &str, pb: ProgressBar) -> Arc<Mutex<Self>> {
        let mut me = Self::default();

        me.url = url.to_string();
        me.progress_bar = Some(pb);
        Arc::new(Mutex::new(me))
    }

    /// Mark the scan as complete and stop the scan's progress bar
    pub fn finish(&mut self) {
        self.complete = true;
        self.stop_progress_bar();
    }
}

/// Display implementation
impl fmt::Display for FeroxScan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let complete = if self.complete {
            style("complete").green()
        } else {
            style("incomplete").red()
        };

        write!(f, "{:10} {}", complete, self.url)
    }
}


/// PartialEq implementation; uses FeroxScan.id for comparison
impl PartialEq for FeroxScan {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Container around a locked hashset of `FeroxScan`s, adds wrappers for insertion and searching
#[derive(Debug, Default)]
pub struct FeroxScans {
    /// Internal structure: locked hashset of `FeroxScan`s
    pub scans: Mutex<Vec<Arc<Mutex<FeroxScan>>>>,
}

/// Implementation of `FeroxScans`
impl FeroxScans {
    /// Add a `FeroxScan` to the internal container
    ///
    /// If the internal container did NOT contain the scan, true is returned; else false
    pub fn insert(&self, scan: Arc<Mutex<FeroxScan>>) -> bool {
        let sentry = match scan.lock() {
            Ok(locked_scan) => {
                // If the container did contain the scan, set sentry to false
                // If the container did not contain the scan, set sentry to true
                !self.contains(&locked_scan.url)
            }
            Err(e) => {
                // poisoned lock
                log::error!("FeroxScan's ({:?}) mutex is poisoned: {}", self, e);
                false
            }
        };

        if sentry {
            // can't update the internal container while the scan itself is locked, so first
            // lock the scan and check the container for the scan's presence, then add if
            // not found
            match self.scans.lock() {
                Ok(mut scans) => {
                    scans.push(scan);
                }
                Err(e) => {
                    log::error!("FeroxScans' container's mutex is poisoned: {}", e);
                    return false;
                }
            }
        }

        sentry
    }

    /// Simple check for whether or not a FeroxScan is contained within the inner container based
    /// on the given URL
    pub fn contains(&self, url: &str) -> bool {
        match self.scans.lock() {
            Ok(scans) => {
                for scan in scans.iter() {
                    if let Ok(locked_scan) = scan.lock() {
                        if locked_scan.url == url {
                            return true;
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("FeroxScans' container's mutex is poisoned: {}", e);
            }
        }
        false
    }

    /// Find and return a `FeroxScan` based on the given URL
    pub fn get_scan_by_url(&self, url: &str) -> Option<Arc<Mutex<FeroxScan>>> {
        if let Ok(scans) = self.scans.lock() {
            for scan in scans.iter() {
                if let Ok(locked_scan) = scan.lock() {
                    if locked_scan.url == url {
                        return Some(scan.clone());
                    }
                }
            }
        }
        None
    }

    /// todo doc
    pub fn display_scans(&self) {
        if let Ok(scans) = self.scans.lock() {
            for (i, scan) in scans.iter().enumerate() {
                let msg = format!(
                    "{:3}: {}",
                    i,
                    scan.lock().unwrap()
                );
                PROGRESS_PRINTER.println(format!("{}", msg));
            }
        }
    }

    /// Forced the calling thread into a busy loop
    ///
    /// Every `SLEEP_DURATION` milliseconds, the function examines the result stored in `PAUSE_SCAN`
    ///
    /// When the value stored in `PAUSE_SCAN` becomes `false`, the function returns, exiting the busy
    /// loop
    pub async fn pause(&self) {
        log::trace!("enter: pause_scan");
        // function uses tokio::time, not std

        // local testing showed a pretty slow increase (less than linear) in CPU usage as # of
        // concurrent scans rose when SLEEP_DURATION was set to 500, using that as the default for now
        let mut interval = time::interval(time::Duration::from_millis(SLEEP_DURATION));

        // ignore any error returned
        let _ = stderr().flush();

        if INTERACTIVE_BARRIER.load(Ordering::Relaxed) == 0 {
            INTERACTIVE_BARRIER.fetch_add(1, Ordering::Relaxed);

            self.display_scans();

            let mut s = String::new();
            std::io::stdin().read_line(&mut s).unwrap();
            PROGRESS_PRINTER.println(format!("Here's your shit: {}", s));
        }

        loop {
            // first tick happens immediately, all others wait the specified duration
            interval.tick().await;

            if !PAUSE_SCAN.load(Ordering::Acquire) {
                // PAUSE_SCAN is false, so we can exit the busy loop
                let _ = stderr().flush();
                if INTERACTIVE_BARRIER.load(Ordering::Relaxed) == 1 {
                    INTERACTIVE_BARRIER.fetch_sub(1, Ordering::Relaxed);
                }
                log::trace!("exit: pause_scan");
                return;
            }
        }
    }

    /// Given a url, create a new `FeroxScan` and add it to `FeroxScans`
    ///
    /// If `FeroxScans` did not already contain the scan, return true; otherwise return false
    ///
    /// Also return a reference to the new `FeroxScan`
    pub fn add_scan(&self, url: &str) -> (bool, Arc<Mutex<FeroxScan>>) {
        let progress_bar =
            progress::add_bar(&url, NUMBER_OF_REQUESTS.load(Ordering::Relaxed), false);

        progress_bar.reset_elapsed();

        let ferox_scan = FeroxScan::new(&url, progress_bar);

        // If the set did not contain the scan, true is returned.
        // If the set did contain the scan, false is returned.
        let response = self.insert(ferox_scan.clone());

        (response, ferox_scan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // todo add_url_* and pause_scan tests need to be redone

    #[tokio::test(core_threads = 1)]
    /// tests that pause_scan pauses execution and releases execution when PAUSE_SCAN is toggled
    /// the spinner used during the test has had .finish_and_clear called on it, meaning that
    /// a new one will be created, taking the if branch within the function
    async fn scanner_pause_scan_with_finished_spinner() {
        let now = time::Instant::now();

        PAUSE_SCAN.store(true, Ordering::Relaxed);
        // BARRIER.write().unwrap().finish_and_clear();

        let expected = time::Duration::from_secs(2);

        tokio::spawn(async move {
            time::delay_for(expected).await;
            PAUSE_SCAN.store(false, Ordering::Relaxed);
        });

        pause_scan().await;

        assert!(now.elapsed() > expected);
    }

    #[test]
    /// add an unknown url to the hashset, expect true
    fn add_url_to_list_of_scanned_urls_with_unknown_url() {
        let urls = FeroxScans::default();
        let url = "http://unknown_url";
        let (result, _scan) = add_url_to_list_of_scanned_urls(url, &urls);
        assert_eq!(result, true);
    }

    #[test]
    /// add a known url to the hashset, with a trailing slash, expect false
    fn add_url_to_list_of_scanned_urls_with_known_url() {
        let urls = FeroxScans::default();
        let pb = ProgressBar::new(1);
        let url = "http://unknown_url/";
        let mut scan = FeroxScan::new(url, pb);

        assert_eq!(urls.insert(scan), true);

        let (result, _scan) = add_url_to_list_of_scanned_urls(url, &urls);

        assert_eq!(result, false);
    }

    #[test]
    /// add a known url to the hashset, without a trailing slash, expect false
    fn add_url_to_list_of_scanned_urls_with_known_url_without_slash() {
        let urls = FeroxScans::default();
        let pb = ProgressBar::new(1);
        let url = "http://unknown_url";
        let mut scan = FeroxScan::new(url, pb);

        assert_eq!(urls.insert(scan), true);

        let (result, _scan) = add_url_to_list_of_scanned_urls(url, &urls);

        assert_eq!(result, false);
    }

}