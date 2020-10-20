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

use reqwest::header::HeaderMap;
use reqwest::{Response, StatusCode, Url};
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
    /// The final `Url` of this `FeroxResponse`
    pub url: Url,

    /// The `StatusCode` of this `FeroxResponse`
    pub status: StatusCode,

    /// The full response text
    pub text: String,

    /// The content-length of this response, if known
    pub content_length: u64,

    /// The `Headers` of this `FeroxResponse`
    pub headers: HeaderMap,
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
    pub async fn from(response: Response) -> Self {
        let url = response.url().clone();
        let status = response.status();
        let headers = response.headers().clone();
        let content_length = response.content_length().unwrap_or(0);

        let text = if CONFIGURATION.extract_links {
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
