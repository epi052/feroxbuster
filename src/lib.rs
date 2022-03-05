#![deny(clippy::all)]
#![allow(clippy::mutex_atomic)]
use anyhow::Result;
use reqwest::StatusCode;
use std::collections::HashSet;
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

use crate::event_handlers::Command;

pub mod banner;
pub mod config;
mod client;
pub mod event_handlers;
pub mod filters;
pub mod heuristics;
pub mod logger;
mod parser;
pub mod progress;
pub mod scan_manager;
pub mod scanner;
pub mod statistics;
mod traits;
pub mod utils;
mod extractor;
mod macros;
mod url;
mod response;
mod message;
mod nlp;

/// Alias for tokio::sync::mpsc::UnboundedSender<Command>
pub(crate) type CommandSender = UnboundedSender<Command>;

/// Alias for tokio::sync::mpsc::UnboundedSender<Command>
pub(crate) type CommandReceiver = UnboundedReceiver<Command>;

/// Alias for tokio::task::JoinHandle<anyhow::Result<()>>
pub(crate) type Joiner = JoinHandle<Result<()>>;

/// Generic mpsc::unbounded_channel type to tidy up some code
pub(crate) type FeroxChannel<T> = (UnboundedSender<T>, UnboundedReceiver<T>);

/// Wrapper around the results of performing any kind of extraction against a target web page
pub(crate) type ExtractionResult = HashSet<String>;

/// Version pulled from Cargo.toml at compile time
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum number of file descriptors that can be opened during a scan
pub const DEFAULT_OPEN_FILE_LIMIT: u64 = 8192;

/// Default value used to determine near-duplicate web pages (equivalent to 95%)
pub const SIMILARITY_THRESHOLD: u32 = 95;

/// Default set of extensions to Ignore when auto-collecting extensions during scans
pub(crate) const DEFAULT_IGNORED_EXTENSIONS: [&str; 38] = [
    "tif", "tiff", "ico", "cur", "bmp", "webp", "svg", "png", "jpg", "jpeg", "jfif", "gif", "avif",
    "apng", "pjpeg", "pjp", "mov", "wav", "mpg", "mpeg", "mp3", "mp4", "m4a", "m4p", "m4v", "ogg",
    "webm", "ogv", "oga", "flac", "aac", "3gp", "css", "zip", "xls", "xml", "gz", "tgz",
];

/// Default wordlist to use when `-w|--wordlist` isn't specified and not `wordlist` isn't set
/// in a [ferox-config.toml](constant.DEFAULT_CONFIG_NAME.html) config file.
///
/// defaults to kali's default install location:
/// - `/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt`
pub const DEFAULT_WORDLIST: &str =
    "/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt";

/// Number of milliseconds to wait between polls of `PAUSE_SCAN` when user pauses a scan
pub(crate) const SLEEP_DURATION: u64 = 500;

/// The percentage of requests as errors it takes to be deemed too high
pub const HIGH_ERROR_RATIO: f64 = 0.90;

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
/// * 500 Internal Server Error
pub const DEFAULT_STATUS_CODES: [StatusCode; 10] = [
    StatusCode::OK,
    StatusCode::NO_CONTENT,
    StatusCode::MOVED_PERMANENTLY,
    StatusCode::FOUND,
    StatusCode::TEMPORARY_REDIRECT,
    StatusCode::PERMANENT_REDIRECT,
    StatusCode::UNAUTHORIZED,
    StatusCode::FORBIDDEN,
    StatusCode::METHOD_NOT_ALLOWED,
    StatusCode::INTERNAL_SERVER_ERROR,
];

/// Default method for requests
pub(crate) const DEFAULT_METHOD: &str = "GET";

/// Default filename for config file settings
///
/// Expected location is in the same directory as the feroxbuster binary.
pub const DEFAULT_CONFIG_NAME: &str = "ferox-config.toml";
/// User agents to select from when random agent is being used
pub const USER_AGENTS: [&str; 12] = [
    "Mozilla/5.0 (Linux; Android 8.0.0; SM-G960F Build/R16NW) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/62.0.3202.84 Mobile Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 12_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/12.0 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (Windows Phone 10.0; Android 6.0.1; Microsoft; RM-1152) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/52.0.2743.116 Mobile Safari/537.36 Edge/15.15254",
    "Mozilla/5.0 (Linux; Android 7.0; Pixel C Build/NRD90M; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/52.0.2743.98 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/42.0.2311.135 Safari/537.36 Edge/12.246",
    "Mozilla/5.0 (X11; CrOS x86_64 8172.45.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/51.0.2704.64 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_11_2) AppleWebKit/601.3.9 (KHTML, like Gecko) Version/9.0.2 Safari/601.3.9",
    "Mozilla/5.0 (Windows NT 6.1; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/47.0.2526.111 Safari/537.36",
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:15.0) Gecko/20100101 Firefox/15.0.1",
    "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
    "Mozilla/5.0 (compatible; Yahoo! Slurp; http://help.yahoo.com/help/us/ysearch/slurp)",
];

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
