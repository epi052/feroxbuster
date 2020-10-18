pub mod banner;
pub mod client;
pub mod config;
pub mod extractor;
pub mod heuristics;
pub mod logger;
pub mod parser;
pub mod progress;
pub mod reporter;
pub mod scanner;
pub mod utils;

use crate::config::CONFIGURATION;

use reqwest::{Url, StatusCode, Response};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Generic Result type to ease error handling in async contexts
pub type FeroxResult<T> =
    std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

/// Generic mpsc::unbounded_channel type to tidy up some code
pub type FeroxChannel<T> = (UnboundedSender<T>, UnboundedReceiver<T>);

/// Version pulled from Cargo.toml at compile time
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default wordlist to use when `-w|--wordlist` isn't specified and not `wordlist` isn't set
/// in a [ferox-config.toml](constant.DEFAULT_CONFIG_NAME.html) config file.
///
/// defaults to kali's default install location:
/// - `/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt`
pub const DEFAULT_WORDLIST: &str =
    "/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt";

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
#[derive(Debug)]
pub struct FeroxResponse {
    /// todo doc
    pub url: Url,

    /// todo doc
    pub status: StatusCode,

    /// todo doc
    pub text: String,

    /// todo doc
    pub content_length: u64
}

/// todo doc
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

    /// Get the content-length of this response, if known
    pub fn content_length(&self) -> u64 {
        self.content_length
    }

    /// todo doc
    pub async fn new(response: Response) -> Self {
        let url = response.url().clone();
        let status = response.status().clone();
        let content_length = response.content_length().unwrap_or(0);

        let text = if CONFIGURATION.extract_links {
            // .text() consumes the response, must be called last
            response.text().await.unwrap()
        } else {
            String::new()
        };

        FeroxResponse {
            url,
            status,
            content_length,
            text
        }
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
