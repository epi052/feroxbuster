use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    str::FromStr,
    sync::Arc,
    {error, fmt},
};

use anyhow::{Context, Result};
use console::{style, Color};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Response, StatusCode, Url,
};
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

use crate::{
    event_handlers::{Command, Handles},
    ferox_url::FeroxUrl,
    traits::FeroxSerialize,
    utils::{fmt_err, status_colorizer},
};

pub mod banner;
pub mod config;
mod client;
pub mod event_handlers;
mod filters;
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
mod ferox_url;
mod ferox_response;

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
pub const DEFAULT_OPEN_FILE_LIMIT: usize = 8192;

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
pub(crate) static SLEEP_DURATION: u64 = 500;

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

#[derive(Serialize, Deserialize, Default)]
/// Representation of a log entry, can be represented as a human readable string or JSON
pub struct FeroxMessage {
    #[serde(rename = "type")]
    /// Name of this type of struct, used for serialization, i.e. `{"type":"log"}`
    kind: String,

    /// The log message
    pub message: String,

    /// The log level
    pub level: String,

    /// The number of seconds elapsed since the scan started
    pub time_offset: f32,

    /// The module from which log::* was called
    pub module: String,
}

/// Implementation of FeroxMessage
impl FeroxSerialize for FeroxMessage {
    /// Create a string representation of the log message
    ///
    /// ex:  301       10l       16w      173c https://localhost/api
    fn as_str(&self) -> String {
        let (level_name, level_color) = match self.level.as_str() {
            "ERROR" => ("ERR", Color::Red),
            "WARN" => ("WRN", Color::Red),
            "INFO" => ("INF", Color::Cyan),
            "DEBUG" => ("DBG", Color::Yellow),
            "TRACE" => ("TRC", Color::Magenta),
            "WILDCARD" => ("WLD", Color::Cyan),
            _ => ("UNK", Color::White),
        };

        format!(
            "{} {:10.03} {} {}\n",
            style(level_name).bg(level_color).black(),
            style(self.time_offset).dim(),
            self.module,
            style(&self.message).dim(),
        )
    }

    /// Create an NDJSON representation of the log message
    ///
    /// (expanded for clarity)
    /// ex:
    /// {
    ///   "type": "log",
    ///   "message": "Sent https://localhost/api to file handler",
    ///   "level": "DEBUG",
    ///   "time_offset": 0.86333454,
    ///   "module": "feroxbuster::reporter"
    /// }\n
    fn as_json(&self) -> Result<String> {
        let mut json = serde_json::to_string(&self).with_context(|| {
            fmt_err(&format!(
                "Could not convert {}:{} to JSON",
                self.level, self.message
            ))
        })?;
        json.push('\n');
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Url;

    use crate::ferox_response::FeroxResponse;

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

    #[test]
    /// test as_str method of FeroxMessage
    fn ferox_message_as_str_returns_string_with_newline() {
        let message = FeroxMessage {
            message: "message".to_string(),
            module: "utils".to_string(),
            time_offset: 1.0,
            level: "INFO".to_string(),
            kind: "log".to_string(),
        };
        let message_str = message.as_str();

        assert!(message_str.contains("INF"));
        assert!(message_str.contains("1.000"));
        assert!(message_str.contains("utils"));
        assert!(message_str.contains("message"));
        assert!(message_str.ends_with('\n'));
    }

    #[test]
    /// test as_json method of FeroxMessage
    fn ferox_message_as_json_returns_json_representation_of_ferox_message_with_newline() {
        let message = FeroxMessage {
            message: "message".to_string(),
            module: "utils".to_string(),
            time_offset: 1.0,
            level: "INFO".to_string(),
            kind: "log".to_string(),
        };

        let message_str = message.as_json().unwrap();

        let error_margin = f32::EPSILON;

        let json: FeroxMessage = serde_json::from_str(&message_str).unwrap();
        assert_eq!(json.module, message.module);
        assert_eq!(json.message, message.message);
        assert!((json.time_offset - message.time_offset).abs() < error_margin);
        assert_eq!(json.level, message.level);
        assert_eq!(json.kind, message.kind);
    }
    #[test]
    /// call reached_max_depth with max depth of zero, which is infinite recursion, expect false
    fn reached_max_depth_returns_early_on_zero() {
        let handles = Arc::new(Handles::for_testing(None, None).0);
        let url = Url::parse("http://localhost").unwrap();
        let response = FeroxResponse {
            url,
            status: Default::default(),
            text: "".to_string(),
            content_length: 0,
            line_count: 0,
            word_count: 0,
            headers: Default::default(),
            wildcard: false,
        };
        let result = response.reached_max_depth(0, 0, handles);
        assert!(!result);
    }

    #[test]
    /// call reached_max_depth with url depth equal to max depth, expect true
    fn reached_max_depth_current_depth_equals_max() {
        let handles = Arc::new(Handles::for_testing(None, None).0);

        let url = Url::parse("http://localhost/one/two").unwrap();
        let response = FeroxResponse {
            url,
            status: Default::default(),
            text: "".to_string(),
            content_length: 0,
            line_count: 0,
            word_count: 0,
            headers: Default::default(),
            wildcard: false,
        };

        let result = response.reached_max_depth(0, 2, handles);
        assert!(result);
    }

    #[test]
    /// call reached_max_depth with url dpeth less than max depth, expect false
    fn reached_max_depth_current_depth_less_than_max() {
        let handles = Arc::new(Handles::for_testing(None, None).0);
        let url = Url::parse("http://localhost").unwrap();
        let response = FeroxResponse {
            url,
            status: Default::default(),
            text: "".to_string(),
            content_length: 0,
            line_count: 0,
            word_count: 0,
            headers: Default::default(),
            wildcard: false,
        };

        let result = response.reached_max_depth(0, 2, handles);
        assert!(!result);
    }

    #[test]
    /// call reached_max_depth with url of 2, base depth of 2, and max depth of 2, expect false
    fn reached_max_depth_base_depth_equals_max_depth() {
        let handles = Arc::new(Handles::for_testing(None, None).0);
        let url = Url::parse("http://localhost/one/two").unwrap();
        let response = FeroxResponse {
            url,
            status: Default::default(),
            text: "".to_string(),
            content_length: 0,
            line_count: 0,
            word_count: 0,
            headers: Default::default(),
            wildcard: false,
        };

        let result = response.reached_max_depth(2, 2, handles);
        assert!(!result);
    }

    #[test]
    /// call reached_max_depth with url depth greater than max depth, expect true
    fn reached_max_depth_current_greater_than_max() {
        let handles = Arc::new(Handles::for_testing(None, None).0);
        let url = Url::parse("http://localhost/one/two/three").unwrap();
        let response = FeroxResponse {
            url,
            status: Default::default(),
            text: "".to_string(),
            content_length: 0,
            line_count: 0,
            word_count: 0,
            headers: Default::default(),
            wildcard: false,
        };

        let result = response.reached_max_depth(0, 2, handles);
        assert!(result);
    }
}
