use anyhow::Result;
use reqwest::StatusCode;
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

/// Alias for tokio::sync::mpsc::UnboundedSender<Command>
pub(crate) type CommandSender = UnboundedSender<Command>;

/// Alias for tokio::sync::mpsc::UnboundedSender<Command>
pub(crate) type CommandReceiver = UnboundedReceiver<Command>;

/// Alias for tokio::task::JoinHandle<anyhow::Result<()>>
pub(crate) type Joiner = JoinHandle<Result<()>>;

/// Generic mpsc::unbounded_channel type to tidy up some code
pub(crate) type FeroxChannel<T> = (UnboundedSender<T>, UnboundedReceiver<T>);

/// Version pulled from Cargo.toml at compile time
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Maximum number of file descriptors that can be opened during a scan
pub const DEFAULT_OPEN_FILE_LIMIT: u64 = 8192;

/// Default value used to determine near-duplicate web pages (equivalent to 95%)
pub const SIMILARITY_THRESHOLD: u32 = 95;

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
