use super::scan::ScanType;
use super::*;
use crate::{
    config::PROGRESS_PRINTER,
    progress::{add_bar, BarType},
    scanner::RESPONSES,
    FeroxSerialize, SLEEP_DURATION,
};
use anyhow::Result;
use serde::{ser::SerializeSeq, Serialize, Serializer};
use std::{
    convert::TryInto,
    fs::File,
    io::BufReader,
    ops::Index,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
    thread::sleep,
};
use tokio::time::{self, Duration};

/// Single atomic number that gets incremented once, used to track first thread to interact with
/// when pausing a scan
static INTERACTIVE_BARRIER: AtomicUsize = AtomicUsize::new(0);

/// Atomic boolean flag, used to determine whether or not a scan should pause or resume
pub static PAUSE_SCAN: AtomicBool = AtomicBool::new(false);

/// Container around a locked hashset of `FeroxScan`s, adds wrappers for insertion and searching
#[derive(Debug, Default)]
pub struct FeroxScans {
    /// Internal structure: locked hashset of `FeroxScan`s
    pub scans: RwLock<Vec<Arc<FeroxScan>>>,

    /// menu used for providing a way for users to cancel a scan
    menu: Menu,

    /// number of requests expected per scan (mirrors the same on Stats); used for initializing
    /// progress bars and feroxscans
    bar_length: Mutex<u64>,
}

/// Serialize implementation for FeroxScans
///
/// purposefully skips menu attribute
impl Serialize for FeroxScans {
    /// Function that handles serialization of FeroxScans
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Ok(scans) = self.scans.read() {
            let mut seq = serializer.serialize_seq(Some(scans.len()))?;
            for scan in scans.iter() {
                seq.serialize_element(&*scan).unwrap_or_default();
            }

            seq.end()
        } else {
            // if for some reason we can't unlock the RwLock, just write an empty list
            let seq = serializer.serialize_seq(Some(0))?;
            seq.end()
        }
    }
}

/// Implementation of `FeroxScans`
impl FeroxScans {
    /// Add a `FeroxScan` to the internal container
    ///
    /// If the internal container did NOT contain the scan, true is returned; else false
    pub fn insert(&self, scan: Arc<FeroxScan>) -> bool {
        // If the container did contain the scan, set sentry to false
        // If the container did not contain the scan, set sentry to true
        let sentry = !self.contains(&scan.url);

        if sentry {
            // can't update the internal container while the scan itself is locked, so first
            // lock the scan and check the container for the scan's presence, then add if
            // not found
            match self.scans.write() {
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

    /// load serialized FeroxScan(s) into this FeroxScans  
    pub fn add_serialized_scans(&self, filename: &str) -> Result<()> {
        log::trace!("enter: add_serialized_scans({})", filename);
        let file = File::open(filename)?;

        let reader = BufReader::new(file);
        let state: serde_json::Value = serde_json::from_reader(reader)?;

        if let Some(scans) = state.get("scans") {
            if let Some(arr_scans) = scans.as_array() {
                for scan in arr_scans {
                    let deser_scan: FeroxScan =
                        serde_json::from_value(scan.clone()).unwrap_or_default();
                    // need to determine if it's complete and based on that create a progress bar
                    // populate it accordingly based on completion
                    log::debug!("added: {}", deser_scan);
                    self.insert(Arc::new(deser_scan));
                }
            }
        }

        log::trace!("exit: add_serialized_scans");
        Ok(())
    }

    /// Simple check for whether or not a FeroxScan is contained within the inner container based
    /// on the given URL
    pub fn contains(&self, url: &str) -> bool {
        if let Ok(scans) = self.scans.read() {
            for scan in scans.iter() {
                if scan.url == url {
                    return true;
                }
            }
        }
        false
    }

    /// Find and return a `FeroxScan` based on the given URL
    pub fn get_scan_by_url(&self, url: &str) -> Option<Arc<FeroxScan>> {
        if let Ok(guard) = self.scans.read() {
            for scan in guard.iter() {
                if scan.url == url {
                    return Some(scan.clone());
                }
            }
        }
        None
    }

    /// Print all FeroxScans of type Directory
    ///
    /// Example:
    ///   0: complete   https://10.129.45.20
    ///   9: complete   https://10.129.45.20/images
    ///  10: complete   https://10.129.45.20/assets
    pub async fn display_scans(&self) {
        let scans = {
            // written this way in order to grab the vector and drop the lock immediately
            // otherwise the spawned task that this is a part of is no longer Send due to
            // the scan.task.lock().await below while the lock is held (RwLock is not Send)
            self.scans
                .read()
                .expect("Could not acquire lock in display_scans")
                .clone()
        };

        for (i, scan) in scans.iter().enumerate() {
            if matches!(scan.scan_order, ScanOrder::Initial) || scan.task.try_lock().is_err() {
                // original target passed in via either -u or --stdin
                continue;
            }

            if matches!(scan.scan_type, ScanType::Directory) {
                // we're only interested in displaying directory scans, as those are
                // the only ones that make sense to be stopped
                let scan_msg = format!("{:3}: {}", i, scan);
                self.menu.println(&scan_msg);
            }
        }
    }

    /// Given a list of indexes, cancel their associated FeroxScans
    async fn cancel_scans(&self, indexes: Vec<usize>) {
        let menu_pause_duration = Duration::from_millis(SLEEP_DURATION);

        for num in indexes {
            let selected = match self.scans.read() {
                Ok(u_scans) => {
                    // check if number provided is out of range
                    if num >= u_scans.len() {
                        // usize can't be negative, just need to handle exceeding bounds
                        self.menu
                            .println(&format!("The number {} is not a valid choice.", num));
                        sleep(menu_pause_duration);
                        continue;
                    }
                    u_scans.index(num).clone()
                }
                Err(..) => continue,
            };

            let input = self.menu.confirm_cancellation(&selected.url);

            if input == 'y' || input == '\n' {
                self.menu.println(&format!("Stopping {}...", selected.url));
                selected
                    .abort()
                    .await
                    .unwrap_or_else(|e| log::warn!("Could not cancel task: {}", e));
            } else {
                self.menu.println("Ok, doing nothing...");
            }

            sleep(menu_pause_duration);
        }
    }

    /// CLI menu that allows for interactive cancellation of recursed-into directories
    async fn interactive_menu(&self) {
        self.menu.hide_progress_bars();
        self.menu.clear_screen();
        self.menu.print_header();
        self.display_scans().await;
        self.menu.print_footer();

        if let Some(input) = self.menu.get_scans_from_user() {
            self.cancel_scans(input).await
        };

        self.menu.clear_screen();
        self.menu.show_progress_bars();
    }

    /// prints all known responses that the scanner has already seen
    pub fn print_known_responses(&self) {
        if let Ok(responses) = RESPONSES.responses.read() {
            for response in responses.iter() {
                PROGRESS_PRINTER.println(response.as_str());
            }
        }
    }

    /// if a resumed scan is already complete, display a completed progress bar to the user
    pub fn print_completed_bars(&self, bar_length: usize) -> Result<()> {
        if let Ok(scans) = self.scans.read() {
            for scan in scans.iter() {
                if scan.is_complete() {
                    // these scans are complete, and just need to be shown to the user
                    let pb = add_bar(
                        &scan.url,
                        bar_length.try_into().unwrap_or_default(),
                        BarType::Message,
                    );
                    pb.finish();
                }
            }
        }
        Ok(())
    }

    /// Forced the calling thread into a busy loop
    ///
    /// Every `SLEEP_DURATION` milliseconds, the function examines the result stored in `PAUSE_SCAN`
    ///
    /// When the value stored in `PAUSE_SCAN` becomes `false`, the function returns, exiting the busy
    /// loop
    pub async fn pause(&self, get_user_input: bool) {
        // function uses tokio::time, not std

        // local testing showed a pretty slow increase (less than linear) in CPU usage as # of
        // concurrent scans rose when SLEEP_DURATION was set to 500, using that as the default for now
        let mut interval = time::interval(time::Duration::from_millis(SLEEP_DURATION));

        if INTERACTIVE_BARRIER.load(Ordering::Relaxed) == 0 {
            INTERACTIVE_BARRIER.fetch_add(1, Ordering::Relaxed);

            if get_user_input {
                self.interactive_menu().await;
                PAUSE_SCAN.store(false, Ordering::Relaxed);
                self.print_known_responses();
            }
        }

        loop {
            // first tick happens immediately, all others wait the specified duration
            interval.tick().await;

            if !PAUSE_SCAN.load(Ordering::Acquire) {
                // PAUSE_SCAN is false, so we can exit the busy loop

                if INTERACTIVE_BARRIER.load(Ordering::Relaxed) == 1 {
                    INTERACTIVE_BARRIER.fetch_sub(1, Ordering::Relaxed);
                }

                log::trace!("exit: pause_scan");
                return;
            }
        }
    }

    /// set the bar length of FeroxScans
    pub fn set_bar_length(&self, bar_length: u64) {
        if let Ok(mut guard) = self.bar_length.lock() {
            *guard = bar_length;
        }
    }

    /// Given a url, create a new `FeroxScan` and add it to `FeroxScans`
    ///
    /// If `FeroxScans` did not already contain the scan, return true; otherwise return false
    ///
    /// Also return a reference to the new `FeroxScan`
    pub(super) fn add_scan(
        &self,
        url: &str,
        scan_type: ScanType,
        scan_order: ScanOrder,
    ) -> (bool, Arc<FeroxScan>) {
        let bar_length = if let Ok(guard) = self.bar_length.lock() {
            *guard
        } else {
            0
        };

        let bar = match scan_type {
            ScanType::Directory => {
                let progress_bar = add_bar(&url, bar_length, BarType::Default);

                progress_bar.reset_elapsed();

                Some(progress_bar)
            }
            ScanType::File => None,
        };

        let ferox_scan = FeroxScan::new(&url, scan_type, scan_order, bar_length, bar);

        // If the set did not contain the scan, true is returned.
        // If the set did contain the scan, false is returned.
        let response = self.insert(ferox_scan.clone());

        (response, ferox_scan)
    }

    /// Given a url, create a new `FeroxScan` and add it to `FeroxScans` as a Directory Scan
    ///
    /// If `FeroxScans` did not already contain the scan, return true; otherwise return false
    ///
    /// Also return a reference to the new `FeroxScan`
    pub fn add_directory_scan(&self, url: &str, scan_order: ScanOrder) -> (bool, Arc<FeroxScan>) {
        self.add_scan(&url, ScanType::Directory, scan_order)
    }

    /// Given a url, create a new `FeroxScan` and add it to `FeroxScans` as a File Scan
    ///
    /// If `FeroxScans` did not already contain the scan, return true; otherwise return false
    ///
    /// Also return a reference to the new `FeroxScan`
    pub fn add_file_scan(&self, url: &str, scan_order: ScanOrder) -> (bool, Arc<FeroxScan>) {
        self.add_scan(&url, ScanType::File, scan_order)
    }

    /// small helper to determine whether any scans are active or not
    pub fn has_active_scans(&self) -> bool {
        if let Ok(guard) = self.scans.read() {
            for scan in guard.iter() {
                if scan.is_active() {
                    return true;
                }
            }
        }
        false
    }

    /// Retrieve all active scans
    pub fn get_active_scans(&self) -> Vec<Arc<FeroxScan>> {
        let mut scans = vec![];

        if let Ok(guard) = self.scans.read() {
            for scan in guard.iter() {
                if !scan.is_active() {
                    continue;
                }
                scans.push(scan.clone());
            }
        }
        scans
    }
}
