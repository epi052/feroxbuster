use super::scan::ScanType;
use super::*;
use crate::event_handlers::Handles;
use crate::filters::{
    EmptyFilter, LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter,
    WildcardFilter, WordsFilter,
};
use crate::traits::FeroxFilter;
use crate::Command::AddFilter;
use crate::{
    banner::Banner,
    config::OutputLevel,
    progress::PROGRESS_PRINTER,
    progress::{add_bar, BarType},
    scan_manager::utils::determine_bar_type,
    scan_manager::{MenuCmd, MenuCmdResult},
    scanner::RESPONSES,
    traits::FeroxSerialize,
    Command, SLEEP_DURATION,
};
use anyhow::Result;
use console::style;
use reqwest::StatusCode;
use serde::{ser::SerializeSeq, Serialize, Serializer};
use std::{
    collections::HashSet,
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
use tokio::sync::oneshot;
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

    /// whether or not the user passed --silent|--quiet on the command line
    output_level: OutputLevel,

    /// vector of extensions discovered and collected during scans
    pub(crate) collected_extensions: RwLock<HashSet<String>>,

    /// stored value for Configuration.limit_bars
    bar_limit: usize,
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
        match self.scans.read() {
            Ok(scans) => {
                let mut seq = serializer.serialize_seq(Some(scans.len() + 1))?;

                for scan in scans.iter() {
                    seq.serialize_element(scan).unwrap_or_default();
                }
                seq.end()
            }
            Err(_) => {
                // if for some reason we can't unlock the RwLock, just write an empty list
                let seq = serializer.serialize_seq(Some(0))?;
                seq.end()
            }
        }
    }
}

/// Implementation of `FeroxScans`
impl FeroxScans {
    /// given an OutputLevel, create a new FeroxScans object
    pub fn new(output_level: OutputLevel, bar_limit: usize) -> Self {
        Self {
            output_level,
            bar_limit,
            ..Default::default()
        }
    }

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
                    log::warn!("FeroxScans' container's mutex is poisoned: {}", e);
                    return false;
                }
            }
        }

        sentry
    }

    /// load serialized FeroxScan(s) and any previously collected extensions into this FeroxScans  
    pub fn add_serialized_scans(&self, filename: &str, handles: Arc<Handles>) -> Result<()> {
        log::trace!("enter: add_serialized_scans({})", filename);
        let file = File::open(filename)?;

        let reader = BufReader::new(file);
        let state: serde_json::Value = serde_json::from_reader(reader)?;

        if let Some(scans) = state.get("scans") {
            if let Some(arr_scans) = scans.as_array() {
                for scan in arr_scans {
                    let mut deser_scan: FeroxScan =
                        serde_json::from_value(scan.clone()).unwrap_or_default();

                    if deser_scan.is_cancelled() {
                        // if the scan was cancelled by the user, mark it as complete. This will
                        // prevent the scan from being resumed as well as prevent the wordlist
                        // from requesting it again
                        if let Ok(mut guard) = deser_scan.status.lock() {
                            *guard = ScanStatus::Complete;
                        }
                    }

                    // FeroxScans gets -q value from config as usual; the FeroxScans themselves
                    // rely on that value being passed in. If the user starts a scan without -q
                    // and resumes the scan but adds -q, FeroxScan will not have the proper value
                    // without the line below
                    deser_scan.output_level = self.output_level;

                    self.insert(Arc::new(deser_scan));
                }
            }
        }

        if let Some(extensions) = state.get("collected_extensions") {
            if let Some(arr_exts) = extensions.as_array() {
                if let Ok(mut guard) = self.collected_extensions.write() {
                    for ext in arr_exts {
                        let deser_ext: String =
                            serde_json::from_value(ext.clone()).unwrap_or_default();

                        guard.insert(deser_ext);
                    }
                }
            }
        }

        if let Some(filters) = state.get("filters") {
            if let Some(arr_filters) = filters.as_array() {
                for filter in arr_filters {
                    let final_filter: Box<dyn FeroxFilter> = if let Ok(deserialized) =
                        serde_json::from_value::<RegexFilter>(filter.clone())
                    {
                        Box::new(deserialized)
                    } else if let Ok(deserialized) =
                        serde_json::from_value::<WordsFilter>(filter.clone())
                    {
                        Box::new(deserialized)
                    } else if let Ok(deserialized) =
                        serde_json::from_value::<WildcardFilter>(filter.clone())
                    {
                        Box::new(deserialized)
                    } else if let Ok(deserialized) =
                        serde_json::from_value::<SizeFilter>(filter.clone())
                    {
                        Box::new(deserialized)
                    } else if let Ok(deserialized) =
                        serde_json::from_value::<LinesFilter>(filter.clone())
                    {
                        Box::new(deserialized)
                    } else if let Ok(deserialized) =
                        serde_json::from_value::<SimilarityFilter>(filter.clone())
                    {
                        Box::new(deserialized)
                    } else if let Ok(deserialized) =
                        serde_json::from_value::<StatusCodeFilter>(filter.clone())
                    {
                        Box::new(deserialized)
                    } else {
                        Box::new(EmptyFilter {})
                    };

                    handles
                        .filters
                        .send(AddFilter(final_filter))
                        .unwrap_or_default();
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
            let normalized = format!("{}/", url.trim_end_matches('/'));

            for scan in scans.iter() {
                if scan.normalized_url == normalized {
                    return true;
                }
            }
        }
        false
    }

    /// Find and return a `FeroxScan` based on the given URL
    pub fn get_scan_by_url(&self, url: &str) -> Option<Arc<FeroxScan>> {
        if let Ok(guard) = self.scans.read() {
            let normalized = format!("{}/", url.trim_end_matches('/'));

            for scan in guard.iter() {
                if scan.normalized_url == normalized {
                    return Some(scan.clone());
                }
            }
        }
        None
    }

    pub fn get_base_scan_by_url(&self, url: &str) -> Option<Arc<FeroxScan>> {
        log::trace!("enter: get_base_scan_by_url({})", url);

        // rmatch_indices returns tuples in index, match form, i.e. (10, "/")
        // with the furthest-right match in the first position in the vector
        let matches: Vec<_> = url.rmatch_indices('/').collect();

        // iterate from the furthest right matching index and check the given url from the
        // start to the furthest-right '/' character. compare that slice to the urls associated
        // with directory scans and return the first match, since it should be the 'deepest'
        // match.
        // Example:
        //   url: http://shmocalhost/src/release/examples/stuff.php
        //   scans:
        //      http://shmocalhost/src/statistics
        //      http://shmocalhost/src/banner
        //      http://shmocalhost/src/release
        //      http://shmocalhost/src/release/examples
        //
        //  returns: http://shmocalhost/src/release/examples
        if let Ok(guard) = self.scans.read() {
            for (idx, _) in &matches {
                for scan in guard.iter() {
                    let slice = url.index(0..*idx);
                    if slice == scan.url || format!("{slice}/").as_str() == scan.url {
                        log::trace!("enter: get_base_scan_by_url -> {}", scan);
                        return Some(scan.clone());
                    }
                }
            }
        }

        log::trace!("enter: get_base_scan_by_url -> None");
        None
    }
    /// add one to either 403 or 429 tracker in the scan related to the given url
    pub fn increment_status_code(&self, url: &str, code: StatusCode) {
        if let Some(scan) = self.get_base_scan_by_url(url) {
            match code {
                StatusCode::TOO_MANY_REQUESTS => {
                    scan.add_429();
                }
                StatusCode::FORBIDDEN => {
                    scan.add_403();
                }
                _ => {}
            }
        }
    }

    /// add one to either 403 or 429 tracker in the scan related to the given url
    pub fn increment_error(&self, url: &str) {
        if let Some(scan) = self.get_base_scan_by_url(url) {
            scan.add_error();
        }
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

        let mut printed = 0;

        for (i, scan) in scans.iter().enumerate() {
            if matches!(scan.scan_type, ScanType::Directory) {
                if printed == 0 {
                    self.menu
                        .println(&format!("{}:", style("Scans").bright().blue()));
                }

                if let Ok(guard) = scan.status.lock() {
                    if matches!(*guard, ScanStatus::Cancelled) {
                        continue;
                    }
                }

                // we're only interested in displaying directory scans, as those are
                // the only ones that make sense to be stopped
                let scan_msg = format!("{i:3}: {scan}");
                self.menu.println(&scan_msg);
                printed += 1;
            }
        }

        if printed > 0 {
            self.menu.print_border();
        }
    }

    /// Given a list of indexes, cancel their associated FeroxScans
    async fn cancel_scans(&self, indexes: Vec<usize>, force: bool) -> usize {
        let menu_pause_duration = Duration::from_millis(SLEEP_DURATION);

        let mut num_cancelled = 0_usize;

        for num in indexes {
            let selected = match self.scans.read() {
                Ok(u_scans) => {
                    // check if number provided is out of range
                    if num >= u_scans.len() {
                        // usize can't be negative, just need to handle exceeding bounds
                        self.menu
                            .println(&format!("The number {num} is not a valid choice."));
                        sleep(menu_pause_duration);
                        continue;
                    }

                    let selected = u_scans.index(num);

                    if matches!(selected.scan_type, ScanType::File) {
                        continue;
                    }

                    selected.clone()
                }
                Err(..) => continue,
            };

            let input = if force {
                'y'
            } else {
                self.menu.confirm_cancellation(&selected.url)
            };

            if input == 'y' || input == '\n' {
                self.menu.println(&format!("Stopping {}...", selected.url));
                let active_bars = self.number_of_bars();
                selected
                    .abort(active_bars)
                    .await
                    .unwrap_or_else(|e| log::warn!("Could not cancel task: {}", e));

                let pb = selected.progress_bar();
                num_cancelled += pb.length().unwrap_or(0) as usize - pb.position() as usize;
            } else {
                self.menu.println("Ok, doing nothing...");
            }

            sleep(menu_pause_duration);
        }

        num_cancelled
    }

    fn display_filters(&self, handles: Arc<Handles>) {
        let mut printed = 0;

        if let Ok(guard) = handles.filters.data.filters.read() {
            for (i, filter) in guard.iter().enumerate() {
                if i == 0 {
                    self.menu
                        .println(&format!("{}:", style("Filters").bright().blue()));
                }

                let filter_msg = format!("{:3}: {}", i + 1, filter);
                self.menu.println(&filter_msg);
                printed += 1;
            }

            if printed > 0 {
                self.menu.print_border();
            }
        }
    }

    /// CLI menu that allows for interactive cancellation of recursed-into directories
    async fn interactive_menu(&self, handles: Arc<Handles>) -> Option<MenuCmdResult> {
        self.menu.hide_progress_bars();
        self.menu.clear_screen();
        self.menu.print_header();
        let (tx, rx) = oneshot::channel::<Duration>();
        if handles.stats.send(Command::QueryOverallBarEta(tx)).is_ok() {
            if let Ok(y) = rx.await {
                self.menu.print_eta(y);
            }
        }

        self.display_scans().await;
        self.display_filters(handles.clone());
        self.menu.print_footer();

        let menu_cmd = if let Ok(line) = self.menu.term.read_line() {
            self.menu.get_command_input_from_user(&line)
        } else {
            None
        };

        let result = match menu_cmd {
            Some(MenuCmd::Cancel(indices, should_force)) => {
                // cancel the things
                let num_cancelled = self.cancel_scans(indices, should_force).await;
                Some(MenuCmdResult::NumCancelled(num_cancelled))
            }
            Some(MenuCmd::AddUrl(url)) => Some(MenuCmdResult::Url(url)),
            Some(MenuCmd::AddFilter(filter)) => Some(MenuCmdResult::Filter(filter)),
            Some(MenuCmd::RemoveFilter(indices)) => {
                handles
                    .filters
                    .send(Command::RemoveFilters(indices))
                    .unwrap_or_default();
                None
            }
            None => None,
        };

        self.menu.clear_screen();

        let banner = Banner::new(&[handles.config.target_url.clone()], &handles.config);
        banner
            .print_to(&self.menu.term, handles.config.clone())
            .unwrap_or_default();

        self.menu.show_progress_bars();

        let has_active_scans = if let Ok(guard) = self.scans.read() {
            guard.iter().any(|s| s.is_active())
        } else {
            // if we can't tell for sure, we'll let it ride
            //
            // i'm not sure which is the better option here:
            // either return true and let it potentially hang, or
            // return false and exit, so just going with not
            // abruptly exiting for maybe no reason
            true
        };

        if !has_active_scans {
            // the last active scan was cancelled, so we can exit
            self.menu.println(&format!(
                " 😱 no more active scans... {}",
                style("exiting").red()
            ));

            let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
            handles
                .send_scan_command(Command::JoinTasks(tx))
                .unwrap_or_default();
            rx.await.unwrap_or_default();
        }

        result
    }

    /// prints all known responses that the scanner has already seen
    pub fn print_known_responses(&self) {
        if let Ok(mut responses) = RESPONSES.responses.write() {
            for response in responses.iter_mut() {
                if self.output_level != response.output_level {
                    // set the output_level prior to printing the response to ensure that the
                    // response's setting aligns with the overall configuration (since we're
                    // calling this from a resumed state)
                    response.output_level = self.output_level;
                }
                PROGRESS_PRINTER.println(response.as_str());
            }
        }
    }

    /// if a resumed scan is already complete, display a completed progress bar to the user
    pub fn print_completed_bars(&self, bar_length: usize) -> Result<()> {
        if self.output_level == OutputLevel::SilentJSON || self.output_level == OutputLevel::Silent
        {
            // fast exit when --silent was used
            return Ok(());
        }

        let bar_type: BarType =
            determine_bar_type(self.bar_limit, self.number_of_bars(), self.output_level);

        if let Ok(scans) = self.scans.read() {
            for scan in scans.iter() {
                if matches!(bar_type, BarType::Hidden) {
                    // no need to show hidden bars
                    continue;
                }

                if scan.is_complete() {
                    // these scans are complete, and just need to be shown to the user
                    let pb = add_bar(
                        &scan.url,
                        bar_length.try_into().unwrap_or_default(),
                        bar_type,
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
    pub async fn pause(
        &self,
        get_user_input: bool,
        handles: Arc<Handles>,
    ) -> Option<MenuCmdResult> {
        // function uses tokio::time, not std

        // local testing showed a pretty slow increase (less than linear) in CPU usage as # of
        // concurrent scans rose when SLEEP_DURATION was set to 500, using that as the default for now
        let mut interval = time::interval(time::Duration::from_millis(SLEEP_DURATION));
        let mut command_result = None;

        if INTERACTIVE_BARRIER.load(Ordering::Relaxed) == 0 {
            INTERACTIVE_BARRIER.fetch_add(1, Ordering::Relaxed);

            if get_user_input {
                command_result = self.interactive_menu(handles).await;
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

                log::trace!("exit: pause_scan -> {:?}", command_result);
                return command_result;
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
        handles: Arc<Handles>,
    ) -> (bool, Arc<FeroxScan>) {
        let bar_length = if let Ok(guard) = self.bar_length.lock() {
            *guard
        } else {
            0
        };

        let active_bars = self.number_of_bars();
        let bar_type = determine_bar_type(self.bar_limit, active_bars, self.output_level);

        let bar = match scan_type {
            ScanType::Directory => {
                let progress_bar = add_bar(url, bar_length, bar_type);

                progress_bar.reset_elapsed();

                Some(progress_bar)
            }
            ScanType::File => None,
        };

        let is_visible = !matches!(bar_type, BarType::Hidden);

        let ferox_scan = FeroxScan::new(
            url,
            scan_type,
            scan_order,
            bar_length,
            self.output_level,
            bar,
            is_visible,
            handles,
        );

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
    pub fn add_directory_scan(
        &self,
        url: &str,
        scan_order: ScanOrder,
        handles: Arc<Handles>,
    ) -> (bool, Arc<FeroxScan>) {
        let normalized = format!("{}/", url.trim_end_matches('/'));
        self.add_scan(&normalized, ScanType::Directory, scan_order, handles)
    }

    /// Given a url, create a new `FeroxScan` and add it to `FeroxScans` as a File Scan
    ///
    /// If `FeroxScans` did not already contain the scan, return true; otherwise return false
    ///
    /// Also return a reference to the new `FeroxScan`
    pub fn add_file_scan(
        &self,
        url: &str,
        scan_order: ScanOrder,
        handles: Arc<Handles>,
    ) -> (bool, Arc<FeroxScan>) {
        self.add_scan(url, ScanType::File, scan_order, handles)
    }

    /// returns the number of active AND visible scans; supports --limit-bars functionality
    pub fn number_of_bars(&self) -> usize {
        let Ok(scans) = self.scans.read() else {
            return 0;
        };

        // starting at one ensures we don't have an extra bar
        // due to counting up from 0 when there's actually 1 bar
        let mut count = 1;

        for scan in &*scans {
            if scan.is_active() && scan.visible() {
                count += 1;
            }
        }

        count
    }

    /// make one hidden bar visible; supports --limit-bars functionality
    pub fn make_visible(&self) {
        if let Ok(guard) = self.scans.read() {
            // when swapping visibility, we'll prefer an actively running scan
            // if none are found, we'll
            let mut queued = None;

            for scan in &*guard {
                if !matches!(scan.scan_type, ScanType::Directory) {
                    // visibility only makes sense for directory scans
                    continue;
                }

                if scan.visible() {
                    continue;
                }

                if scan.is_running() {
                    scan.swap_visibility();
                    return;
                }

                if queued.is_none() && scan.is_not_started() {
                    queued = Some(scan.clone());
                }
            }

            if let Some(scan) = queued {
                scan.swap_visibility();
            }
        }
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

    /// given an extension, add it to `collected_extensions` if all constraints are met
    /// returns `true` if an extension was added, `false` otherwise
    pub fn add_discovered_extension(&self, extension: String) -> bool {
        log::trace!("enter: add_discovered_extension({})", extension);
        let mut extension_added = false;

        // note: the filter by --dont-collect happens in the event handler, since it has access
        // to a Handles object form which it can check the config value. additionally, the check
        // against --extensions is performed there for the same reason

        if let Ok(extensions) = self.collected_extensions.read() {
            // quicker to allow most to read and return and then reopen for write if necessary
            if extensions.contains(&extension) {
                return extension_added;
            }
        }

        if let Ok(mut extensions) = self.collected_extensions.write() {
            log::info!("discovered new extension: {}", extension);
            extensions.insert(extension);
            extension_added = true;
        }

        log::trace!("exit: add_discovered_extension -> {}", extension_added);
        extension_added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// unknown extension should be added to collected_extensions
    fn unknown_extension_is_added_to_collected_extensions() {
        let scans = FeroxScans::new(OutputLevel::Default, 0);

        assert_eq!(0, scans.collected_extensions.read().unwrap().len());

        let added = scans.add_discovered_extension(String::from("js"));

        assert!(added);
        assert_eq!(1, scans.collected_extensions.read().unwrap().len());
    }

    #[test]
    /// known extension should not be added to collected_extensions
    fn known_extension_is_added_to_collected_extensions() {
        let scans = FeroxScans::new(OutputLevel::Default, 0);
        scans
            .collected_extensions
            .write()
            .unwrap()
            .insert(String::from("js"));

        assert_eq!(1, scans.collected_extensions.read().unwrap().len());

        let added = scans.add_discovered_extension(String::from("js"));

        assert!(!added);
        assert_eq!(1, scans.collected_extensions.read().unwrap().len());
    }
}
