pub mod utils;
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
pub mod scan_manager;
pub mod scanner;
pub mod statistics;

use crate::utils::{get_url_path_length, status_colorizer};
use console::{style, Color};
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{header::HeaderMap, Response, StatusCode, Url};
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::str::FromStr;
use std::{error, fmt};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

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

/// FeroxSerialize trait; represents different types that are Serialize and also implement
/// as_str / as_json methods
pub trait FeroxSerialize: Serialize {
    /// Return a String representation of the object, generally the human readable version of the
    /// implementor
    fn as_str(&self) -> String;

    /// Return an NDJSON representation of the object
    fn as_json(&self) -> String;
}

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

    /// The number of lines contained in the body of this response, if known
    line_count: usize,

    /// The number of words contained in the body of this response, if known
    word_count: usize,

    /// The `Headers` of this `FeroxResponse`
    headers: HeaderMap,

    /// Wildcard response status
    wildcard: bool,
}

/// Implement Display for FeroxResponse
impl fmt::Display for FeroxResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "FeroxResponse {{ url: {}, status: {}, content-length: {} }}",
            self.url(),
            self.status(),
            self.content_length()
        )
    }
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

    /// Returns line count of the response text.
    pub fn line_count(&self) -> usize {
        self.line_count
    }

    /// Returns word count of the response text.
    pub fn word_count(&self) -> usize {
        self.word_count
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

        let line_count = text.lines().count();
        let word_count = text.lines().map(|s| s.split_whitespace().count()).sum();

        FeroxResponse {
            url,
            status,
            content_length,
            text,
            headers,
            line_count,
            word_count,
            wildcard: false,
        }
    }
}

/// Implement FeroxSerialusize::from(ize for FeroxRespons)e
impl FeroxSerialize for FeroxResponse {
    /// Simple wrapper around create_report_string
    fn as_str(&self) -> String {
        let lines = self.line_count().to_string();
        let words = self.word_count().to_string();
        let chars = self.content_length().to_string();
        let status = self.status().as_str();
        let wild_status = status_colorizer("WLD");

        if self.wildcard {
            // response is a wildcard, special messages abound when this is the case...

            // create the base message
            let mut message = format!(
                "{} {:>8}l {:>8}w {:>8}c Got {} for {} (url length: {})\n",
                wild_status,
                lines,
                words,
                chars,
                status_colorizer(&status),
                self.url(),
                get_url_path_length(&self.url())
            );

            if self.status().is_redirection() {
                // when it's a redirect, show where it goes, if possible
                if let Some(next_loc) = self.headers().get("Location") {
                    let next_loc_str = next_loc.to_str().unwrap_or("Unknown");

                    let redirect_msg = format!(
                        "{} {:>9} {:>9} {:>9} {} redirects to => {}\n",
                        wild_status,
                        "-",
                        "-",
                        "-",
                        self.url(),
                        next_loc_str
                    );

                    message.push_str(&redirect_msg);
                }
            }

            // base message + redirection message (if appropriate)
            message
        } else {
            // not a wildcard, just create a normal entry
            utils::create_report_string(
                self.status.as_str(),
                &lines,
                &words,
                &chars,
                self.url().as_str(),
            )
        }
    }

    /// Create an NDJSON representation of the FeroxResponse
    ///
    /// (expanded for clarity)
    /// ex:
    /// {
    ///    "type":"response",
    ///    "url":"https://localhost.com/images",
    ///    "path":"/images",
    ///    "status":301,
    ///    "content_length":179,
    ///    "line_count":10,
    ///    "word_count":16,
    ///    "headers":{
    ///       "x-content-type-options":"nosniff",
    ///       "strict-transport-security":"max-age=31536000; includeSubDomains",
    ///       "x-frame-options":"SAMEORIGIN",
    ///       "connection":"keep-alive",
    ///       "server":"nginx/1.16.1",
    ///       "content-type":"text/html; charset=UTF-8",
    ///       "referrer-policy":"origin-when-cross-origin",
    ///       "content-security-policy":"default-src 'none'",
    ///       "access-control-allow-headers":"X-Requested-With",
    ///       "x-xss-protection":"1; mode=block",
    ///       "content-length":"179",
    ///       "date":"Mon, 23 Nov 2020 15:33:24 GMT",
    ///       "location":"/images/",
    ///       "access-control-allow-origin":"https://localhost.com"
    ///    }
    /// }\n
    fn as_json(&self) -> String {
        if let Ok(mut json) = serde_json::to_string(&self) {
            json.push('\n');
            json
        } else {
            format!("{{\"error\":\"could not convert {} to json\"}}", self.url())
        }
    }
}

/// Serialize implementation for FeroxResponse
impl Serialize for FeroxResponse {
    /// Function that handles serialization of a FeroxResponse to NDJSON
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut headers = HashMap::new();
        let mut state = serializer.serialize_struct("FeroxResponse", 7)?;

        // need to convert the HeaderMap to a HashMap in order to pass it to the serializer
        for (key, value) in &self.headers {
            let k = key.as_str().to_owned();
            let v = String::from_utf8_lossy(value.as_bytes());
            headers.insert(k, v);
        }

        state.serialize_field("type", "response")?;
        state.serialize_field("url", self.url.as_str())?;
        state.serialize_field("path", self.url.path())?;
        state.serialize_field("wildcard", &self.wildcard)?;
        state.serialize_field("status", &self.status.as_u16())?;
        state.serialize_field("content_length", &self.content_length)?;
        state.serialize_field("line_count", &self.line_count)?;
        state.serialize_field("word_count", &self.word_count)?;
        state.serialize_field("headers", &headers)?;

        state.end()
    }
}

/// Deserialize implementation for FeroxResponse
impl<'de> Deserialize<'de> for FeroxResponse {
    /// Deserialize a FeroxResponse from a serde_json::Value
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut response = Self {
            url: Url::parse("http://localhost").unwrap(),
            status: StatusCode::OK,
            text: String::new(),
            content_length: 0,
            headers: HeaderMap::new(),
            wildcard: false,
            line_count: 0,
            word_count: 0,
        };

        let map: HashMap<String, Value> = HashMap::deserialize(deserializer)?;

        for (key, value) in &map {
            match key.as_str() {
                "url" => {
                    if let Some(url) = value.as_str() {
                        if let Ok(parsed) = Url::parse(url) {
                            response.url = parsed;
                        }
                    }
                }
                "status" => {
                    if let Some(num) = value.as_u64() {
                        if let Ok(smaller) = u16::try_from(num) {
                            if let Ok(status) = StatusCode::from_u16(smaller) {
                                response.status = status;
                            }
                        }
                    }
                }
                "content_length" => {
                    if let Some(num) = value.as_u64() {
                        response.content_length = num;
                    }
                }
                "line_count" => {
                    if let Some(num) = value.as_u64() {
                        response.line_count = num.try_into().unwrap_or_default();
                    }
                }
                "word_count" => {
                    if let Some(num) = value.as_u64() {
                        response.word_count = num.try_into().unwrap_or_default();
                    }
                }
                "headers" => {
                    let mut headers = HeaderMap::<HeaderValue>::default();

                    if let Some(map_headers) = value.as_object() {
                        for (h_key, h_value) in map_headers {
                            let h_value_str = h_value.as_str().unwrap_or("");
                            let h_name = HeaderName::from_str(h_key)
                                .unwrap_or_else(|_| HeaderName::from_str("Unknown").unwrap());
                            let h_value_parsed = HeaderValue::from_str(h_value_str)
                                .unwrap_or_else(|_| HeaderValue::from_str("Unknown").unwrap());
                            headers.insert(h_name, h_value_parsed);
                        }
                    }

                    response.headers = headers;
                }
                "wildcard" => {
                    if let Some(result) = value.as_bool() {
                        response.wildcard = result;
                    }
                }
                _ => {}
            }
        }

        Ok(response)
    }
}

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
    fn as_json(&self) -> String {
        if let Ok(mut json) = serde_json::to_string(&self) {
            json.push('\n');
            json
        } else {
            String::from("{\"error\":\"could not convert to json\"}")
        }
    }

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

        let message_str = message.as_json();

        let error_margin = f32::EPSILON;

        let json: FeroxMessage = serde_json::from_str(&message_str).unwrap();
        assert_eq!(json.module, message.module);
        assert_eq!(json.message, message.message);
        assert!((json.time_offset - message.time_offset).abs() < error_margin);
        assert_eq!(json.level, message.level);
        assert_eq!(json.kind, message.kind);
    }
}
