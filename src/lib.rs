pub mod banner;
pub mod client;
pub mod config;
pub mod extractor;
pub mod filters;
pub mod heuristics;
pub mod logger;
pub mod parser;
pub mod progress;
pub mod reporter;
pub mod scanner;
pub mod utils;

use indicatif::ProgressBar;
use reqwest::{
    header::HeaderMap,
    {Response, StatusCode, Url},
};
use std::{
    cmp::PartialEq,
    error, fmt,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Generic Result type to ease error handling in async contexts
pub type FeroxResult<T> = std::result::Result<T, Box<dyn error::Error + Send + Sync + 'static>>;

/// Simple Error implementation to allow for custom error returns
#[derive(Debug, Default)]
pub struct FeroxError {
    /// fancy string that can be printed via Display
    pub message: String,
}

impl error::Error for FeroxError {}

impl fmt::Display for FeroxError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.message)
    }
}

/// Generic mpsc::unbounded_channel type to tidy up some code
pub type FeroxChannel<T> = (UnboundedSender<T>, UnboundedReceiver<T>);

/// Version pulled from Cargo.toml at compile time
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum number of file descriptors that can be opened during a scan
pub const DEFAULT_OPEN_FILE_LIMIT: usize = 8192;

/// Default wordlist to use when `-w|--wordlist` isn't specified and not `wordlist` isn't set
/// in a [ferox-config.toml](constant.DEFAULT_CONFIG_NAME.html) config file.
///
/// defaults to kali's default install location:
/// - `/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt`
pub const DEFAULT_WORDLIST: &str =
    "/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt";

/// Number of milliseconds to wait between polls of `PAUSE_SCAN` when user pauses a scan
pub static SLEEP_DURATION: u64 = 500;

/// Default list of status codes to report
///
/// * 200 Ok
/// * 204 No Content
/// * 301 Moved Permanently
/// * 302 Found
/// * 307 Temporary Redirect
/// * 308 Permanent Redirect
/// * 401 Unauthorized
/// * 403 Forbidden
/// * 405 Method Not Allowed
pub const DEFAULT_STATUS_CODES: [StatusCode; 9] = [
    StatusCode::OK,
    StatusCode::NO_CONTENT,
    StatusCode::MOVED_PERMANENTLY,
    StatusCode::FOUND,
    StatusCode::TEMPORARY_REDIRECT,
    StatusCode::PERMANENT_REDIRECT,
    StatusCode::UNAUTHORIZED,
    StatusCode::FORBIDDEN,
    StatusCode::METHOD_NOT_ALLOWED,
];

/// Default filename for config file settings
///
/// Expected location is in the same directory as the feroxbuster binary.
pub const DEFAULT_CONFIG_NAME: &str = "ferox-config.toml";

/// A `FeroxResponse`, derived from a `Response` to a submitted `Request`
#[derive(Debug, Clone)]
pub struct FeroxResponse {
    /// The final `Url` of this `FeroxResponse`
    url: Url,

    /// The `StatusCode` of this `FeroxResponse`
    status: StatusCode,

    /// The full response text
    text: String,

    /// The content-length of this response, if known
    content_length: u64,

    /// The `Headers` of this `FeroxResponse`
    headers: HeaderMap,
}

/// `FeroxResponse` implementation
impl FeroxResponse {
    /// Get the `StatusCode` of this `FeroxResponse`
    pub fn status(&self) -> &StatusCode {
        &self.status
    }

    /// Get the final `Url` of this `FeroxResponse`.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the full response text
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the `Headers` of this `FeroxResponse`
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Get the content-length of this response, if known
    pub fn content_length(&self) -> u64 {
        self.content_length
    }

    /// Set `FeroxResponse`'s `url` attribute, has no affect if an error occurs
    pub fn set_url(&mut self, url: &str) {
        match Url::parse(&url) {
            Ok(url) => {
                self.url = url;
            }
            Err(e) => {
                log::error!("Could not parse {} into a Url: {}", url, e);
            }
        };
    }

    /// Make a reasonable guess at whether the response is a file or not
    ///
    /// Examines the last part of a path to determine if it has an obvious extension
    /// i.e. http://localhost/some/path/stuff.js where stuff.js indicates a file
    ///
    /// Additionally, inspects query parameters, as they're also often indicative of a file
    pub fn is_file(&self) -> bool {
        let has_extension = match self.url.path_segments() {
            Some(path) => {
                if let Some(last) = path.last() {
                    last.contains('.') // last segment has some sort of extension, probably
                } else {
                    false
                }
            }
            None => false,
        };

        self.url.query_pairs().count() > 0 || has_extension
    }

    /// Create a new `FeroxResponse` from the given `Response`
    pub async fn from(response: Response, read_body: bool) -> Self {
        let url = response.url().clone();
        let status = response.status();
        let headers = response.headers().clone();
        let content_length = response.content_length().unwrap_or(0);

        let text = if read_body {
            // .text() consumes the response, must be called last
            // additionally, --extract-links is currently the only place we use the body of the
            // response, so we forego the processing if not performing extraction
            match response.text().await {
                // await the response's body
                Ok(text) => text,
                Err(e) => {
                    log::error!("Could not parse body from response: {}", e);
                    String::new()
                }
            }
        } else {
            String::new()
        };

        FeroxResponse {
            url,
            status,
            content_length,
            text,
            headers,
        }
    }
}

/// Struct to hold scan-related state
///
/// The purpose of this container is to open up the pathway to aborting currently running tasks and
/// serialization of all scan state into a state file in order to resume scans that were cut short
#[derive(Debug)]
struct FeroxScan {
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

        me.url = utils::normalize_url(url);
        me.progress_bar = Some(pb);
        Arc::new(Mutex::new(me))
    }

    /// Mark the scan as complete and stop the scan's progress bar
    pub fn finish(&mut self) {
        self.complete = true;
        self.stop_progress_bar();
    }
}

// /// Eq implementation
// impl Eq for FeroxScan {}

/// PartialEq implementation; uses FeroxScan.id for comparison
impl PartialEq for FeroxScan {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

// /// Hash implementation; uses uses FeroxScan.id and uses FeroxScan.url for hashing
// impl Hash for FeroxScan {
//     /// Do the hashing with the hasher
//     fn hash<H: Hasher>(&self, state: &mut H) {
//         self.id.hash(state);
//         self.url.hash(state);
//     }
// }

/// Container around a locked hashset of `FeroxScan`s, adds wrappers for insertion and searching
#[derive(Debug, Default)]
struct FeroxScans {
    scans: Mutex<Vec<Arc<Mutex<FeroxScan>>>>,
}

/// Implementation of `FeroxScans`
impl FeroxScans {
    /// Add a `FeroxScan` to the internal container
    ///
    /// If the internal container did NOT contain the scan, true is returned; else false
    pub fn insert(&mut self, scan: Arc<Mutex<FeroxScan>>) -> bool {
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
        let normalized_url = utils::normalize_url(url);

        match self.scans.lock() {
            Ok(scans) => {
                for scan in scans.iter() {
                    if let Ok(locked_scan) = scan.lock() {
                        if locked_scan.url == normalized_url {
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
        let normalized_url = utils::normalize_url(url);

        match self.scans.lock() {
            Ok(scans) => {
                for scan in scans.iter() {
                    if let Ok(locked_scan) = scan.lock() {
                        if locked_scan.url == normalized_url {
                            return Some(scan.clone());
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("FeroxScans' container's mutex is poisoned: {}", e);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// asserts default config name is correct
    fn default_config_name() {
        assert_eq!(DEFAULT_CONFIG_NAME, "ferox-config.toml");
    }

    #[test]
    /// asserts default wordlist is correct
    fn default_wordlist() {
        assert_eq!(
            DEFAULT_WORDLIST,
            "/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt"
        );
    }

    #[test]
    /// asserts default version is correct
    fn default_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
