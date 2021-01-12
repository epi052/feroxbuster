use crate::{
    config::{Configuration, CONFIGURATION, PROGRESS_BAR, PROGRESS_PRINTER},
    parser::TIMESPEC_REGEX,
    progress::{add_bar, BarType},
    reporter::safe_file_write,
    scanner::{RESPONSES, SCANNED_URLS},
    statistics::Stats,
    utils::open_file,
    FeroxResponse, FeroxSerialize, SLEEP_DURATION,
};
use console::{measure_text_width, pad_str, style, Alignment, Term};
use indicatif::{ProgressBar, ProgressDrawTarget};
use serde::{
    ser::{SerializeSeq, SerializeStruct},
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_json::Value;
use std::{
    cmp::PartialEq,
    collections::HashMap,
    fmt,
    fs::File,
    io::BufReader,
    ops::Index,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    sync::{Arc, Mutex, RwLock},
    thread::sleep,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::Duration;
use tokio::{task::JoinHandle, time};
use uuid::Uuid;

/// Single atomic number that gets incremented once, used to track first thread to interact with
/// when pausing a scan
static INTERACTIVE_BARRIER: AtomicUsize = AtomicUsize::new(0);

/// Atomic boolean flag, used to determine whether or not a scan should pause or resume
pub static PAUSE_SCAN: AtomicBool = AtomicBool::new(false);

/// Simple enum used to flag a `FeroxScan` as likely a directory or file
#[derive(Debug, Serialize, Deserialize)]
pub enum ScanType {
    File,
    Directory,
}

/// Default implementation for ScanType
impl Default for ScanType {
    /// Return ScanType::File as default
    fn default() -> Self {
        Self::File
    }
}

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

    /// The type of scan
    pub scan_type: ScanType,

    /// Number of requests to populate the progress bar with
    pub num_requests: u64,

    /// Status of this scan
    pub status: ScanStatus,

    /// The spawned tokio task performing this scan
    pub task: Option<Arc<JoinHandle<()>>>,

    /// The progress bar associated with this scan
    pub progress_bar: Option<ProgressBar>,
}

/// Default implementation for FeroxScan
impl Default for FeroxScan {
    /// Create a default FeroxScan, populates ID with a new UUID
    fn default() -> Self {
        let new_id = Uuid::new_v4().to_simple().to_string();

        FeroxScan {
            id: new_id,
            task: None,
            status: ScanStatus::default(),
            num_requests: 0,
            url: String::new(),
            progress_bar: None,
            scan_type: ScanType::File,
        }
    }
}

/// Implementation of FeroxScan
impl FeroxScan {
    /// Stop a currently running scan
    pub fn abort(&mut self) {
        if let Some(task) = &self.task {
            self.status = ScanStatus::Cancelled;
            self.stop_progress_bar();
            task.abort();
        }
    }

    /// Simple helper to call .finish on the scan's progress bar
    fn stop_progress_bar(&self) {
        if let Some(pb) = &self.progress_bar {
            pb.finish_at_current_pos();
        }
    }

    /// Simple helper get a progress bar
    pub fn progress_bar(&mut self) -> ProgressBar {
        if let Some(pb) = &self.progress_bar {
            pb.clone()
        } else {
            let pb = add_bar(&self.url, self.num_requests, BarType::Default);

            pb.reset_elapsed();

            self.progress_bar = Some(pb.clone());

            pb
        }
    }

    /// Given a URL and ProgressBar, create a new FeroxScan, wrap it in an Arc and return it
    pub fn new(
        url: &str,
        scan_type: ScanType,
        num_requests: u64,
        pb: Option<ProgressBar>,
    ) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            url: url.to_string(),
            scan_type,
            num_requests,
            progress_bar: pb,
            ..Default::default()
        }))
    }

    /// Mark the scan as complete and stop the scan's progress bar
    pub fn finish(&mut self) {
        self.status = ScanStatus::Complete;
        self.stop_progress_bar();
    }
}

/// Display implementation
impl fmt::Display for FeroxScan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = match self.status {
            ScanStatus::NotStarted => style("not started").bright().blue(),
            ScanStatus::Complete => style("complete").green(),
            ScanStatus::Cancelled => style("cancelled").red(),
            ScanStatus::Running => style("running").bright().yellow(),
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
                        scan.status = match status {
                            "NotStarted" => ScanStatus::NotStarted,
                            "Running" => ScanStatus::Running,
                            "Complete" => ScanStatus::Complete,
                            "Cancelled" => ScanStatus::Cancelled,
                            _ => ScanStatus::default(),
                        }
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

#[derive(Debug, Serialize, Deserialize)]
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

/// Interactive scan cancellation menu
#[derive(Debug)]
struct Menu {
    /// character to use as visual separator of lines
    separator: String,

    /// name of menu
    name: String,

    /// header: name surrounded by separators
    header: String,

    /// instructions
    instructions: String,

    /// footer: instructions surrounded by separators
    footer: String,

    /// target for output
    term: Term,
}

/// Implementation of Menu
impl Menu {
    /// Creates new Menu
    fn new() -> Self {
        let separator = "─".to_string();

        let instructions = format!(
            "Enter a {} list of indexes to {} (ex: 2,3)",
            style("comma-separated").yellow(),
            style("cancel").red(),
        );

        let name = format!(
            "{} {} {}",
            "💀",
            style("Scan Cancel Menu").bright().yellow(),
            "💀"
        );

        let longest = measure_text_width(&instructions).max(measure_text_width(&name));

        let border = separator.repeat(longest);

        let padded_name = pad_str(&name, longest, Alignment::Center, None);

        let header = format!("{}\n{}\n{}", border, padded_name, border);
        let footer = format!("{}\n{}\n{}", border, instructions, border);

        Self {
            separator,
            name,
            header,
            instructions,
            footer,
            term: Term::stderr(),
        }
    }

    /// print menu header
    fn print_header(&self) {
        self.println(&self.header);
    }

    /// print menu footer
    fn print_footer(&self) {
        self.println(&self.footer);
    }

    /// set PROGRESS_BAR bar target to hidden
    fn hide_progress_bars(&self) {
        PROGRESS_BAR.set_draw_target(ProgressDrawTarget::hidden());
    }

    /// set PROGRESS_BAR bar target to hidden
    fn show_progress_bars(&self) {
        PROGRESS_BAR.set_draw_target(ProgressDrawTarget::stdout());
    }

    /// Wrapper around console's Term::clear_screen and flush
    fn clear_screen(&self) {
        self.term.clear_screen().unwrap_or_default();
        self.term.flush().unwrap_or_default();
    }

    /// Wrapper around console's Term::write_line
    fn println(&self, msg: &str) {
        self.term.write_line(msg).unwrap_or_default();
    }

    /// split a string into vec of usizes
    fn split_to_nums(&self, line: &str) -> Vec<usize> {
        line.split(',')
            .map(|s| {
                s.trim().to_string().parse::<usize>().unwrap_or_else(|e| {
                    self.println(&format!("Found non-numeric input: {}", e));
                    0
                })
            })
            .filter(|m| *m != 0)
            .collect()
    }

    /// get comma-separated list of scan indexes from the user
    fn get_scans_from_user(&self) -> Option<Vec<usize>> {
        if let Ok(line) = self.term.read_line() {
            Some(self.split_to_nums(&line))
        } else {
            None
        }
    }

    /// Given a url, confirm with user that we should cancel
    fn confirm_cancellation(&self, url: &str) -> char {
        self.println(&format!(
            "You sure you wanna cancel this scan: {}? [Y/n]",
            url
        ));

        self.term.read_char().unwrap_or('n')
    }
}

/// Default implementation for Menu
impl Default for Menu {
    /// return Menu::new as default
    fn default() -> Menu {
        Menu::new()
    }
}

/// Container around a locked hashset of `FeroxScan`s, adds wrappers for insertion and searching
#[derive(Debug, Default)]
pub struct FeroxScans {
    /// Internal structure: locked hashset of `FeroxScan`s
    pub scans: Mutex<Vec<Arc<Mutex<FeroxScan>>>>,

    /// menu used for providing a way for users to cancel a scan
    menu: Menu,
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
        if let Ok(scans) = self.scans.lock() {
            let mut seq = serializer.serialize_seq(Some(scans.len()))?;

            for scan in scans.iter() {
                if let Ok(unlocked) = scan.lock() {
                    seq.serialize_element(&*unlocked)?;
                }
            }

            seq.end()
        } else {
            // if for some reason we can't unlock the mutex, just write an empty list
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

    /// Print all FeroxScans of type Directory
    ///
    /// Example:
    ///   0: complete   https://10.129.45.20
    ///   9: complete   https://10.129.45.20/images
    ///  10: complete   https://10.129.45.20/assets
    pub fn display_scans(&self) {
        if let Ok(scans) = self.scans.lock() {
            for (i, scan) in scans.iter().enumerate() {
                if let Ok(unlocked_scan) = scan.lock() {
                    if unlocked_scan.task.is_none() {
                        // no JoinHandle associated with this FeroxScan, meaning it was an original
                        // target passed in via either -u or --stdin
                        continue;
                    }

                    if matches!(unlocked_scan.scan_type, ScanType::Directory) {
                        // we're only interested in displaying directory scans, as those are
                        // the only ones that make sense to be stopped
                        let scan_msg = format!("{:3}: {}", i, unlocked_scan);
                        self.menu.println(&scan_msg);
                    }
                }
            }
        }
    }

    /// Given a list of indexes, cancel their associated FeroxScans
    fn cancel_scans(&self, indexes: Vec<usize>) {
        let menu_pause_duration = Duration::from_millis(SLEEP_DURATION);

        for num in indexes {
            if let Ok(u_scans) = self.scans.lock() {
                // check if number provided is out of range
                if num >= u_scans.len() {
                    // usize can't be negative, just need to handle exceeding bounds
                    self.menu
                        .println(&format!("The number {} is not a valid choice.", num));
                    sleep(menu_pause_duration);
                    continue;
                }

                let selected = u_scans.index(num).clone();

                if let Ok(mut ferox_scan) = selected.lock() {
                    let input = self.menu.confirm_cancellation(&ferox_scan.url);

                    if input == 'y' || input == '\n' {
                        self.menu
                            .println(&format!("Stopping {}...", ferox_scan.url));
                        ferox_scan.abort();
                    } else {
                        self.menu.println("Ok, doing nothing...");
                    }
                }

                sleep(menu_pause_duration);
            }
        }
    }

    /// CLI menu that allows for interactive cancellation of recursed-into directories
    async fn interactive_menu(&self) {
        self.menu.hide_progress_bars();
        self.menu.clear_screen();
        self.menu.print_header();
        self.display_scans();
        self.menu.print_footer();

        if let Some(input) = self.menu.get_scans_from_user() {
            self.cancel_scans(input);
        }

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

    /// Given a url, create a new `FeroxScan` and add it to `FeroxScans`
    ///
    /// If `FeroxScans` did not already contain the scan, return true; otherwise return false
    ///
    /// Also return a reference to the new `FeroxScan`
    fn add_scan(
        &self,
        url: &str,
        scan_type: ScanType,
        stats: Arc<Stats>,
    ) -> (bool, Arc<Mutex<FeroxScan>>) {
        let num_requests = stats.expected_per_scan.load(Ordering::Relaxed) as u64;

        let bar = match scan_type {
            ScanType::Directory => {
                let progress_bar = add_bar(&url, num_requests, BarType::Default);

                progress_bar.reset_elapsed();

                Some(progress_bar)
            }
            ScanType::File => None,
        };

        let ferox_scan = FeroxScan::new(&url, scan_type, num_requests, bar);

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
        stats: Arc<Stats>,
    ) -> (bool, Arc<Mutex<FeroxScan>>) {
        self.add_scan(&url, ScanType::Directory, stats)
    }

    /// Given a url, create a new `FeroxScan` and add it to `FeroxScans` as a File Scan
    ///
    /// If `FeroxScans` did not already contain the scan, return true; otherwise return false
    ///
    /// Also return a reference to the new `FeroxScan`
    pub fn add_file_scan(&self, url: &str, stats: Arc<Stats>) -> (bool, Arc<Mutex<FeroxScan>>) {
        self.add_scan(&url, ScanType::File, stats)
    }
}

/// Container around a locked vector of `FeroxResponse`s, adds wrappers for insertion and search
#[derive(Debug, Default)]
pub struct FeroxResponses {
    /// Internal structure: locked hashset of `FeroxScan`s
    pub responses: Arc<RwLock<Vec<FeroxResponse>>>,
}

/// Serialize implementation for FeroxResponses
impl Serialize for FeroxResponses {
    /// Function that handles serialization of FeroxResponses
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Ok(responses) = self.responses.read() {
            let mut seq = serializer.serialize_seq(Some(responses.len()))?;

            for response in responses.iter() {
                seq.serialize_element(response)?;
            }

            seq.end()
        } else {
            // if for some reason we can't unlock the mutex, just write an empty list
            let seq = serializer.serialize_seq(Some(0))?;
            seq.end()
        }
    }
}

/// Implementation of `FeroxResponses`
impl FeroxResponses {
    /// Add a `FeroxResponse` to the internal container
    pub fn insert(&self, response: FeroxResponse) {
        match self.responses.write() {
            Ok(mut responses) => {
                responses.push(response);
            }
            Err(e) => {
                log::error!("FeroxResponses' container's mutex is poisoned: {}", e);
            }
        }
    }

    /// Simple check for whether or not a FeroxResponse is contained within the inner container
    pub fn contains(&self, other: &FeroxResponse) -> bool {
        match self.responses.read() {
            Ok(responses) => {
                for response in responses.iter() {
                    if response.url == other.url {
                        return true;
                    }
                }
            }
            Err(e) => {
                log::error!("FeroxResponses' container's mutex is poisoned: {}", e);
            }
        }
        false
    }
}
/// Data container for (de)?serialization of multiple items
#[derive(Serialize, Debug)]
pub struct FeroxState {
    /// Known scans
    scans: &'static FeroxScans,

    /// Current running config
    config: &'static Configuration,

    /// Known responses
    responses: &'static FeroxResponses,

    /// Gathered statistics
    statistics: Arc<Stats>,
}

/// FeroxSerialize implementation for FeroxState
impl FeroxSerialize for FeroxState {
    /// Simply return debug format of FeroxState to satisfy as_str
    fn as_str(&self) -> String {
        format!("{:?}", self)
    }

    /// Simple call to produce a JSON string using the given FeroxState
    fn as_json(&self) -> String {
        serde_json::to_string(&self).unwrap_or_default()
    }
}

/// Given a string representing some number of seconds, minutes, hours, or days, convert
/// that representation to seconds and then wait for those seconds to elapse.  Once that period
/// of time has elapsed, kill all currently running scans and dump a state file to disk that can
/// be used to resume any unfinished scan.
pub async fn start_max_time_thread(time_spec: &str, stats: Arc<Stats>) {
    log::trace!("enter: start_max_time_thread({})", time_spec);

    // as this function has already made it through the parser, which calls is_match on
    // the value passed to --time-limit using TIMESPEC_REGEX; we can safely assume that
    // the capture groups are populated; can expect something like 10m, 30s, 1h, etc...
    let captures = TIMESPEC_REGEX.captures(&time_spec).unwrap();
    let length_match = captures.get(1).unwrap();
    let measurement_match = captures.get(2).unwrap();

    if let Ok(length) = length_match.as_str().parse::<u64>() {
        let length_in_secs = match measurement_match.as_str().to_ascii_lowercase().as_str() {
            "s" => length,
            "m" => length * 60,           // minutes
            "h" => length * 60 * 60,      // hours
            "d" => length * 60 * 60 * 24, // days
            _ => length,
        };

        log::debug!(
            "max time limit as string: {} and as seconds: {}",
            time_spec,
            length_in_secs
        );

        time::sleep(time::Duration::new(length_in_secs, 0)).await;

        log::trace!("exit: start_max_time_thread");

        #[cfg(test)]
        panic!(stats);
        #[cfg(not(test))]
        sigint_handler(stats);
    }

    log::error!(
        "Could not parse the value provided ({}), can't enforce time limit",
        length_match.as_str()
    );
}

/// Writes the current state of the program to disk (if save_state is true) and then exits
fn sigint_handler(stats: Arc<Stats>) {
    log::trace!("enter: sigint_handler({:?})", stats);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let slug = if !CONFIGURATION.target_url.is_empty() {
        // target url populated
        CONFIGURATION
            .target_url
            .replace("://", "_")
            .replace("/", "_")
            .replace(".", "_")
    } else {
        // stdin used
        "stdin".to_string()
    };

    let filename = format!("ferox-{}-{}.state", slug, ts);
    let warning = format!(
        "🚨 Caught {} 🚨 saving scan state to {} ...",
        style("ctrl+c").yellow(),
        filename
    );

    PROGRESS_PRINTER.println(warning);

    let state = FeroxState {
        config: &CONFIGURATION,
        scans: &SCANNED_URLS,
        responses: &RESPONSES,
        statistics: stats,
    };

    let state_file = open_file(&filename);

    if let Some(buffered_file) = state_file {
        safe_file_write(&state, buffered_file, true);
    }

    log::trace!("exit: sigint_handler (end of program)");
    std::process::exit(1);
}

/// Initialize the ctrl+c handler that saves scan state to disk
pub fn initialize(stats: Arc<Stats>) {
    log::trace!("enter: initialize({:?})", stats);

    let result = ctrlc::set_handler(move || {
        sigint_handler(stats.clone());
    });

    if result.is_err() {
        log::error!("Could not set Ctrl+c handler");
        std::process::exit(1);
    }

    log::trace!("exit: initialize");
}

/// Primary logic used to load a Configuration from disk and populate the appropriate data
/// structures
pub fn resume_scan(filename: &str) -> Configuration {
    log::trace!("enter: resume_scan({})", filename);

    let file = File::open(filename).unwrap_or_else(|e| {
        log::error!("{}", e);
        log::error!("Could not open state file, exiting");
        std::process::exit(1);
    });

    let reader = BufReader::new(file);
    let state: serde_json::Value = serde_json::from_reader(reader).unwrap();

    let conf = state.get("config").unwrap_or_else(|| {
        log::error!("Could not load configuration from state file, exiting");
        std::process::exit(1);
    });

    let config = serde_json::from_value(conf.clone()).unwrap_or_else(|e| {
        log::error!("{}", e);
        log::error!("Could not deserialize configuration found in state file, exiting");
        std::process::exit(1);
    });

    if let Some(responses) = state.get("responses") {
        if let Some(arr_responses) = responses.as_array() {
            for response in arr_responses {
                if let Ok(deser_resp) = serde_json::from_value(response.clone()) {
                    RESPONSES.insert(deser_resp);
                }
            }
        }
    }

    if let Some(scans) = state.get("scans") {
        if let Some(arr_scans) = scans.as_array() {
            for scan in arr_scans {
                let deser_scan: FeroxScan =
                    serde_json::from_value(scan.clone()).unwrap_or_default();
                // need to determine if it's complete and based on that create a progress bar
                // populate it accordingly based on completion
                SCANNED_URLS.insert(Arc::new(Mutex::new(deser_scan)));
            }
        }
    }

    log::trace!("exit: resume_scan -> {:?}", config);
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VERSION;
    use predicates::prelude::*;

    #[test]
    /// test that ScanType's default is File
    fn default_scantype_is_file() {
        match ScanType::default() {
            ScanType::File => {}
            ScanType::Directory => panic!(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// tests that pause_scan pauses execution and releases execution when PAUSE_SCAN is toggled
    /// the spinner used during the test has had .finish_and_clear called on it, meaning that
    /// a new one will be created, taking the if branch within the function
    async fn scanner_pause_scan_with_finished_spinner() {
        let now = time::Instant::now();
        let urls = FeroxScans::default();

        PAUSE_SCAN.store(true, Ordering::Relaxed);

        let expected = time::Duration::from_secs(2);

        tokio::spawn(async move {
            time::sleep(expected).await;
            PAUSE_SCAN.store(false, Ordering::Relaxed);
        });

        urls.pause(false).await;

        assert!(now.elapsed() > expected);
    }

    #[test]
    /// add an unknown url to the hashset, expect true
    fn add_url_to_list_of_scanned_urls_with_unknown_url() {
        let urls = FeroxScans::default();
        let stats = Arc::new(Stats::new());
        let url = "http://unknown_url";
        let (result, _scan) = urls.add_scan(url, ScanType::Directory, stats);
        assert_eq!(result, true);
    }

    #[test]
    /// add a known url to the hashset, with a trailing slash, expect false
    fn add_url_to_list_of_scanned_urls_with_known_url() {
        let urls = FeroxScans::default();
        let pb = ProgressBar::new(1);
        let url = "http://unknown_url/";
        let stats = Arc::new(Stats::new());

        let scan = FeroxScan::new(url, ScanType::Directory, pb.length(), Some(pb));

        assert_eq!(urls.insert(scan), true);

        let (result, _scan) = urls.add_scan(url, ScanType::Directory, stats);

        assert_eq!(result, false);
    }

    #[test]
    /// stop_progress_bar should stop the progress bar
    fn stop_progress_bar_stops_bar() {
        let pb = ProgressBar::new(1);
        let url = "http://unknown_url/";

        let scan = FeroxScan::new(url, ScanType::Directory, pb.length(), Some(pb));

        assert_eq!(
            scan.lock()
                .unwrap()
                .progress_bar
                .as_ref()
                .unwrap()
                .is_finished(),
            false
        );

        scan.lock().unwrap().stop_progress_bar();

        assert_eq!(
            scan.lock()
                .unwrap()
                .progress_bar
                .as_ref()
                .unwrap()
                .is_finished(),
            true
        );
    }

    #[test]
    /// add a known url to the hashset, without a trailing slash, expect false
    fn add_url_to_list_of_scanned_urls_with_known_url_without_slash() {
        let urls = FeroxScans::default();
        let url = "http://unknown_url";
        let stats = Arc::new(Stats::new());

        let scan = FeroxScan::new(url, ScanType::File, 0, None);

        assert_eq!(urls.insert(scan), true);

        let (result, _scan) = urls.add_scan(url, ScanType::File, stats);

        assert_eq!(result, false);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// just increasing coverage, no real expectations
    async fn call_display_scans() {
        let urls = FeroxScans::default();
        let pb = ProgressBar::new(1);
        let pb_two = ProgressBar::new(2);
        let url = "http://unknown_url/";
        let url_two = "http://unknown_url/fa";
        let scan = FeroxScan::new(url, ScanType::Directory, pb.length(), Some(pb));
        let scan_two = FeroxScan::new(url_two, ScanType::Directory, pb_two.length(), Some(pb_two));

        if let Ok(mut s2) = scan_two.lock() {
            s2.finish(); // one complete, one incomplete
            s2.task = Some(Arc::new(tokio::spawn(async move {
                sleep(Duration::from_millis(SLEEP_DURATION));
            })));
        }

        assert_eq!(urls.insert(scan), true);
        assert_eq!(urls.insert(scan_two), true);

        urls.display_scans();
    }

    #[test]
    /// ensure that PartialEq compares FeroxScan.id fields
    fn partial_eq_compares_the_id_field() {
        let url = "http://unknown_url/";
        let scan = FeroxScan::new(url, ScanType::Directory, 0, None);
        let scan_two = FeroxScan::new(url, ScanType::Directory, 0, None);

        assert!(!scan.lock().unwrap().eq(&scan_two.lock().unwrap()));

        scan_two.lock().unwrap().id = scan.lock().unwrap().id.clone();

        assert!(scan.lock().unwrap().eq(&scan_two.lock().unwrap()));
    }

    #[test]
    /// show that a new progress bar is created if one doesn't exist
    fn ferox_scan_get_progress_bar_when_none_is_set() {
        let mut scan = FeroxScan::default();

        assert!(scan.progress_bar.is_none()); // no pb exists

        let pb = scan.progress_bar();

        assert!(scan.progress_bar.is_some()); // new pb created
        assert!(!pb.is_finished()) // not finished
    }

    #[test]
    /// given a JSON entry representing a FeroxScan, test that it deserializes into the proper type
    /// with the right attributes
    fn ferox_scan_deserialize() {
        let fs_json = r#"{"id":"057016a14769414aac9a7a62707598cb","url":"https://spiritanimal.com","scan_type":"Directory","status":"Complete"}"#;
        let fs_json_two = r#"{"id":"057016a14769414aac9a7a62707598cb","url":"https://spiritanimal.com","scan_type":"Not Correct","status":"Cancelled"}"#;
        let fs_json_three = r#"{"id":"057016a14769414aac9a7a62707598cb","url":"https://spiritanimal.com","scan_type":"Not Correct","status":"","num_requests":42}"#;

        let fs: FeroxScan = serde_json::from_str(fs_json).unwrap();
        let fs_two: FeroxScan = serde_json::from_str(fs_json_two).unwrap();
        let fs_three: FeroxScan = serde_json::from_str(fs_json_three).unwrap();
        assert_eq!(fs.url, "https://spiritanimal.com");

        match fs.scan_type {
            ScanType::Directory => {}
            ScanType::File => {
                panic!();
            }
        }
        match fs_two.scan_type {
            ScanType::Directory => {
                panic!();
            }
            ScanType::File => {}
        }

        match fs.progress_bar {
            None => {}
            Some(_) => {
                panic!();
            }
        }
        assert!(matches!(fs.status, ScanStatus::Complete));
        assert!(matches!(fs_two.status, ScanStatus::Cancelled));
        assert!(matches!(fs_three.status, ScanStatus::NotStarted));
        assert_eq!(fs_three.num_requests, 42);
        assert_eq!(fs.id, "057016a14769414aac9a7a62707598cb");
    }

    #[test]
    /// given a FeroxScan, test that it serializes into the proper JSON entry
    fn ferox_scan_serialize() {
        let fs = FeroxScan::new("https://spiritanimal.com", ScanType::Directory, 0, None);
        let fs_json = format!(
            r#"{{"id":"{}","url":"https://spiritanimal.com","scan_type":"Directory","status":"NotStarted","num_requests":0}}"#,
            fs.lock().unwrap().id
        );
        assert_eq!(
            fs_json,
            serde_json::to_string(&*fs.lock().unwrap()).unwrap()
        );
    }

    #[test]
    /// given a FeroxScans, test that it serializes into the proper JSON entry
    fn ferox_scans_serialize() {
        let ferox_scan = FeroxScan::new("https://spiritanimal.com", ScanType::Directory, 0, None);
        let ferox_scans = FeroxScans::default();
        let ferox_scans_json = format!(
            r#"[{{"id":"{}","url":"https://spiritanimal.com","scan_type":"Directory","status":"NotStarted","num_requests":0}}]"#,
            ferox_scan.lock().unwrap().id
        );
        ferox_scans.scans.lock().unwrap().push(ferox_scan);
        assert_eq!(
            ferox_scans_json,
            serde_json::to_string(&ferox_scans).unwrap()
        );
    }

    #[test]
    /// given a FeroxResponses, test that it serializes into the proper JSON entry
    fn ferox_responses_serialize() {
        let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"}}"#;
        let response: FeroxResponse = serde_json::from_str(json_response).unwrap();

        let responses = FeroxResponses::default();
        responses.insert(response);
        // responses has a response now

        // serialized should be a list of responses
        let expected = format!("[{}]", json_response);

        let serialized = serde_json::to_string(&responses).unwrap();
        assert_eq!(expected, serialized);
    }

    #[test]
    /// given a FeroxResponse, test that it serializes into the proper JSON entry
    fn ferox_response_serialize_and_deserialize() {
        // deserialize
        let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"}}"#;
        let response: FeroxResponse = serde_json::from_str(json_response).unwrap();

        assert_eq!(response.url.as_str(), "https://nerdcore.com/css");
        assert_eq!(response.url.path(), "/css");
        assert_eq!(response.wildcard, true);
        assert_eq!(response.status.as_u16(), 301);
        assert_eq!(response.content_length, 173);
        assert_eq!(response.line_count, 10);
        assert_eq!(response.word_count, 16);
        assert_eq!(response.headers.get("server").unwrap(), "nginx/1.16.1");

        // serialize, however, this can fail when headers are out of order
        let new_json = serde_json::to_string(&response).unwrap();
        assert_eq!(json_response, new_json);
    }

    #[test]
    /// test FeroxSerialize implementation of FeroxState
    fn feroxstates_feroxserialize_implementation() {
        let ferox_scan = FeroxScan::new("https://spiritanimal.com", ScanType::Directory, 0, None);
        let saved_id = ferox_scan.lock().unwrap().id.clone();
        SCANNED_URLS.insert(ferox_scan);

        let stats = Arc::new(Stats::new());

        let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"}}"#;
        let response: FeroxResponse = serde_json::from_str(json_response).unwrap();
        RESPONSES.insert(response);

        let ferox_state = FeroxState {
            scans: &SCANNED_URLS,
            responses: &RESPONSES,
            config: &CONFIGURATION,
            statistics: stats,
        };

        let expected_strs = predicates::str::contains("scans: FeroxScans").and(
            predicate::str::contains("config: Configuration")
                .and(predicate::str::contains("responses: FeroxResponses"))
                .and(predicate::str::contains("nerdcore.com"))
                .and(predicate::str::contains("/css"))
                .and(predicate::str::contains("https://spiritanimal.com")),
        );

        assert!(expected_strs.eval(&ferox_state.as_str()));

        let json_state = ferox_state.as_json();
        let expected = format!(
            r#"{{"scans":[{{"id":"{}","url":"https://spiritanimal.com","scan_type":"Directory","status":"NotStarted","num_requests":0}}],"config":{{"type":"configuration","wordlist":"/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt","config":"","proxy":"","replay_proxy":"","target_url":"","status_codes":[200,204,301,302,307,308,401,403,405],"replay_codes":[200,204,301,302,307,308,401,403,405],"filter_status":[],"threads":50,"timeout":7,"verbosity":0,"quiet":false,"json":false,"output":"","debug_log":"","user_agent":"feroxbuster/{}","redirects":false,"insecure":false,"extensions":[],"headers":{{}},"queries":[],"no_recursion":false,"extract_links":false,"add_slash":false,"stdin":false,"depth":4,"scan_limit":0,"filter_size":[],"filter_line_count":[],"filter_word_count":[],"filter_regex":[],"dont_filter":false,"resumed":false,"resume_from":"","save_state":false,"time_limit":"","filter_similar":[]}},"responses":[{{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{{"server":"nginx/1.16.1"}}}}]"#,
            saved_id, VERSION
        );
        println!("{}\n{}", expected, json_state);
        assert!(predicates::str::contains(expected).eval(&json_state));
    }

    #[should_panic]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// call start_max_time_thread with a valid timespec, expect a panic, but only after a certain
    /// number of seconds
    async fn start_max_time_thread_panics_after_delay() {
        let now = time::Instant::now();
        let delay = time::Duration::new(3, 0);
        let stats = Arc::new(Stats::new());

        start_max_time_thread("3s", stats).await;

        assert!(now.elapsed() > delay);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// call start_max_time_thread with a timespec that's too large to be parsed correctly, expect
    /// immediate return and no panic, as the sigint handler is never called
    async fn start_max_time_thread_returns_immediately_with_too_large_input() {
        let now = time::Instant::now();
        let delay = time::Duration::new(1, 0);
        let stats = Arc::new(Stats::new());

        // pub const MAX: usize = usize::MAX; // 18_446_744_073_709_551_615usize
        start_max_time_thread("18446744073709551616m", stats).await; // can't fit in dest u64

        assert!(now.elapsed() < delay); // assuming function call will take less than 1second
    }

    #[test]
    /// coverage for FeroxScan's Display implementation
    fn feroxscan_display() {
        let mut scan = FeroxScan {
            id: "".to_string(),
            url: String::from("http://localhost"),
            scan_type: Default::default(),
            num_requests: 0,
            status: Default::default(),
            task: None,
            progress_bar: None,
        };

        let not_started = format!("{}", scan);

        assert!(predicate::str::contains("not started")
            .and(predicate::str::contains("localhost"))
            .eval(&not_started));

        scan.status = ScanStatus::Complete;
        let complete = format!("{}", scan);
        assert!(predicate::str::contains("complete")
            .and(predicate::str::contains("localhost"))
            .eval(&complete));

        scan.status = ScanStatus::Cancelled;
        let cancelled = format!("{}", scan);
        assert!(predicate::str::contains("cancelled")
            .and(predicate::str::contains("localhost"))
            .eval(&cancelled));

        scan.status = ScanStatus::Running;
        let running = format!("{}", scan);
        assert!(predicate::str::contains("running")
            .and(predicate::str::contains("localhost"))
            .eval(&running));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// call FeroxScan::abort, ensure status becomes cancelled
    async fn ferox_scan_abort() {
        let mut scan = FeroxScan {
            id: "".to_string(),
            url: String::from("http://localhost"),
            scan_type: Default::default(),
            num_requests: 0,
            status: ScanStatus::Running,
            task: Some(Arc::new(tokio::spawn(async move {
                sleep(Duration::from_millis(SLEEP_DURATION * 2));
            }))),
            progress_bar: None,
        };

        scan.abort();

        assert!(matches!(scan.status, ScanStatus::Cancelled));
    }

    #[test]
    /// call a few menu functions for coverage's sake
    ///
    /// there's not a trivial way to test these programmatically (at least i'm too lazy rn to do it)
    /// and their correctness can be verified easily manually; just calling for now
    fn menu_print_header_and_footer() {
        let menu = Menu::new();
        menu.clear_screen();
        menu.print_header();
        menu.print_footer();
        menu.hide_progress_bars();
        menu.show_progress_bars();
    }

    #[test]
    /// ensure spaces are trimmed and numbers are returned from split_to_nums
    fn split_to_nums_is_correct() {
        let menu = Menu::new();

        let nums = menu.split_to_nums("1, 3,      4");

        assert_eq!(nums, vec![1, 3, 4]);
    }
}
