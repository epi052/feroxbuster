use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
    sync::Arc,
};

use anyhow::{Context, Result};
use console::style;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method, Response, StatusCode, Url,
};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{
    config::OutputLevel,
    event_handlers::{Command, Handles},
    traits::FeroxSerialize,
    url::FeroxUrl,
    utils::{self, fmt_err, parse_url_with_raw_path, status_colorizer, timestamp},
    CommandSender,
};

/// A `FeroxResponse`, derived from a `Response` to a submitted `Request`
#[derive(Debug, Clone)]
pub struct FeroxResponse {
    /// The final `Url` of this `FeroxResponse`
    url: Url,

    /// The original url from which the final `Url` was derived
    original_url: String,

    /// The `StatusCode` of this `FeroxResponse`
    status: StatusCode,

    /// The HTTP Request `Method` of this `FeroxResponse`
    method: Method,

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

    /// whether the user passed --quiet|--silent on the command line
    pub(crate) output_level: OutputLevel,

    /// Url's file extension, if one exists
    pub(crate) extension: Option<String>,

    /// Timestamp of when this response was received
    timestamp: f64,
}

/// implement Default trait for FeroxResponse
impl Default for FeroxResponse {
    /// return a default reqwest::Url and then normal defaults after that
    fn default() -> Self {
        Self {
            url: Url::parse("http://localhost").unwrap(),
            original_url: "".to_string(),
            status: Default::default(),
            method: Method::default(),
            text: "".to_string(),
            content_length: 0,
            line_count: 0,
            word_count: 0,
            headers: Default::default(),
            wildcard: false,
            output_level: Default::default(),
            extension: None,
            timestamp: timestamp(),
        }
    }
}

/// Implement Display for FeroxResponse
impl fmt::Display for FeroxResponse {
    /// formatter for Display
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "FeroxResponse {{ url: {}, method: {}, status: {}, content-length: {} }}",
            self.url(),
            self.method(),
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

    /// Get the `Method` of this `FeroxResponse`
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Get the `wildcard` of this `FeroxResponse`
    pub fn wildcard(&self) -> bool {
        self.wildcard
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

    /// Get the timestamp of this response
    pub fn timestamp(&self) -> f64 {
        self.timestamp
    }

    /// Set `FeroxResponse`'s `url` attribute, has no affect if an error occurs
    pub fn set_url(&mut self, url: &str) {
        match parse_url_with_raw_path(url) {
            Ok(url) => {
                self.url = url;
            }
            Err(e) => {
                log::warn!("Could not parse {} into a Url: {}", url, e);
            }
        };
    }

    /// set `wildcard` attribute
    pub fn set_wildcard(&mut self, is_wildcard: bool) {
        self.wildcard = is_wildcard;
    }

    /// set `text` attribute; update words/lines/content_length
    #[cfg(test)]
    pub fn set_text(&mut self, text: &str) {
        self.text = String::from(text);
        self.content_length = self.text.len() as u64;
        self.line_count = self.text.lines().count();
        self.word_count = self
            .text
            .lines()
            .map(|s| s.split_whitespace().count())
            .sum();
    }

    /// free the `text` data, reducing memory usage
    pub fn drop_text(&mut self) {
        self.text.clear(); // length is set to 0
        self.text.shrink_to_fit(); // allocated capacity shrinks to reflect the new size
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
    pub async fn from(
        response: Response,
        original_url: &str,
        method: &str,
        output_level: OutputLevel,
    ) -> Self {
        let url = response.url().clone();
        let status = response.status();
        let headers = response.headers().clone();
        let content_length = response.content_length().unwrap_or(0);
        let timestamp = timestamp();

        // .text() consumes the response, must be called last
        let text = response
            .text()
            .await
            .with_context(|| "Could not parse body from response")
            .unwrap_or_default();

        // in the event that the content_length was 0, we can try to get the length
        // of the body we just parsed. At worst, it's still 0; at best we've accounted
        // for sites that reply without a content-length header and yet still have
        // contents in the body.
        //
        // thanks to twitter use @f3rn0s for pointing out the possibility
        let content_length = content_length.max(text.len() as u64);

        let line_count = text.lines().count();
        let word_count = text.lines().map(|s| s.split_whitespace().count()).sum();

        FeroxResponse {
            url,
            original_url: original_url.to_string(),
            status,
            method: Method::from_bytes(method.as_bytes()).unwrap_or(Method::GET),
            content_length,
            text,
            headers,
            line_count,
            word_count,
            output_level,
            wildcard: false,
            extension: None,
            timestamp,
        }
    }

    /// if --collect-extensions is used, examine the response's url and grab the file's extension
    /// if one is available to be grabbed. If an extension is found, send it to the ScanHandler
    /// for further processing
    pub(crate) fn parse_extension(&mut self, handles: Arc<Handles>) -> Result<()> {
        log::trace!("enter: parse_extension");

        if !handles.config.collect_extensions {
            // early return, --collect-extensions not used
            return Ok(());
        }

        // path_segments:
        //   Return None for cannot-be-a-base URLs.
        //   When Some is returned, the iterator always contains at least one string
        //     (which may be empty).
        //
        // meaning: the two unwraps here are fine, the worst outcome is an empty string
        let filename = self.url.path_segments().unwrap().last().unwrap();

        if !filename.is_empty() {
            // non-empty string, try to get extension
            let parts: Vec<_> = filename
                .split('.')
                // keep things like /.bash_history from becoming an extension
                .filter(|part| !part.is_empty())
                .collect();

            if parts.len() > 1 {
                // filename + at least one extension, i.e. whatever.js becomes ["whatever", "js"]
                self.extension = Some(parts.last().unwrap().to_string())
            }
        }

        if let Some(extension) = &self.extension {
            if handles
                .config
                .status_codes
                .contains(&self.status().as_u16())  // in -s list
                // or -C was used, and -s should be all responses that aren't filtered
                || !handles.config.filter_status.is_empty()
            {
                // only add extensions to those responses that pass our checks; filtered out
                // status codes are handled by should_filter, but we need to still check against
                // the allow list for what we want to keep
                #[cfg(test)]
                handles
                    .send_scan_command(Command::AddDiscoveredExtension(extension.to_owned()))
                    .unwrap_or_default();
                #[cfg(not(test))]
                handles.send_scan_command(Command::AddDiscoveredExtension(extension.to_owned()))?;
            }
        }

        log::trace!("exit: parse_extension");
        Ok(())
    }

    /// Helper function that determines if the configured maximum recursion depth has been reached
    ///
    /// Essentially looks at the Url path and determines how many directories are present in the
    /// given Url
    pub(crate) fn reached_max_depth(
        &self,
        base_depth: usize,
        max_depth: usize,
        handles: Arc<Handles>,
    ) -> bool {
        log::trace!(
            "enter: reached_max_depth({}, {}, {:?})",
            base_depth,
            max_depth,
            handles
        );

        if max_depth == 0 {
            // early return, as 0 means recurse forever; no additional processing needed
            log::trace!("exit: reached_max_depth -> false");
            return false;
        }
        let url = FeroxUrl::from_url(&self.url, handles);
        let depth = url.depth().unwrap_or_default(); // 0 on error

        if depth - base_depth >= max_depth {
            return true;
        }

        log::trace!("exit: reached_max_depth -> false");
        false
    }

    /// Helper function to determine suitability for recursion
    ///
    /// handles 2xx and 3xx responses by either checking if the url ends with a / (2xx)
    /// or if the Location header is present and matches the base url + / (3xx)
    pub fn is_directory(&self) -> bool {
        log::trace!("enter: is_directory({})", self);

        if self.status().is_redirection() {
            // status code is 3xx
            match self.headers().get("Location") {
                // and has a Location header
                Some(loc) => {
                    // get absolute redirect Url based on the already known base url
                    log::debug!("Location header: {:?}", loc);

                    if let Ok(loc_str) = loc.to_str() {
                        if let Ok(abs_url) = self.url().join(loc_str) {
                            if format!("{}/", self.url()) == abs_url.as_str() {
                                // if current response's Url + / == the absolute redirection
                                // location, we've found a directory suitable for recursion
                                log::debug!(
                                    "found directory suitable for recursion: {}",
                                    self.url()
                                );
                                log::trace!("exit: is_directory -> true");
                                return true;
                            }
                        }
                    }
                }
                None => {
                    log::debug!("expected Location header, but none was found: {}", self);
                    log::trace!("exit: is_directory -> false");
                    return false;
                }
            }
        } else if self.status().is_success() || matches!(self.status(), &StatusCode::FORBIDDEN) {
            // status code is 2xx or 403, need to check if it ends in /

            if self.url().as_str().ends_with('/') {
                log::debug!("{} is directory suitable for recursion", self.url());
                log::trace!("exit: is_directory -> true");
                return true;
            }
        }

        log::trace!("exit: is_directory -> false");
        false
    }

    /// Simple helper to send a `FeroxResponse` over the tx side of an `mpsc::unbounded_channel`
    pub fn send_report(self, report_sender: CommandSender) -> Result<()> {
        log::trace!("enter: send_report({:?}", report_sender);

        // there's no reason to send the response body across the mpsc
        //
        // the only possible reason is for filtering on the body, but both `send_report`
        // calls are gated behind checks for `should_filter_response`
        let mut me = self;
        me.drop_text();

        report_sender.send(Command::Report(Box::new(me)))?;

        log::trace!("exit: send_report");
        Ok(())
    }
}

/// Implement FeroxSerialize for FeroxResponse
impl FeroxSerialize for FeroxResponse {
    /// Simple wrapper around create_report_string
    fn as_str(&self) -> String {
        let lines = self.line_count().to_string();
        let words = self.word_count().to_string();
        let chars = self.content_length().to_string();
        let status = self.status().as_str();
        let method = self.method().as_str();
        let wild_status = status_colorizer("WLD");

        let mut url_with_redirect = match (
            self.status().is_redirection(),
            self.headers().get("Location").is_some(),
            matches!(
                self.output_level,
                OutputLevel::Silent | OutputLevel::SilentJSON
            ),
        ) {
            (true, true, false) => {
                // redirect with Location header, show where it goes if possible
                let loc = self
                    .headers()
                    .get("Location")
                    .unwrap() // known Some() already
                    .to_str()
                    .unwrap_or("Unknown")
                    .to_string();

                let loc = if loc.starts_with('/') {
                    if let Ok(joined) = self.url().join(&loc) {
                        joined.to_string()
                    } else {
                        loc
                    }
                } else {
                    loc
                };

                // prettify the redirect target
                let loc = style(loc).yellow();

                format!("{} => {loc}", self.url())
            }
            _ => {
                // no redirect, just use the normal url
                self.url().to_string()
            }
        };

        if self.wildcard && matches!(self.output_level, OutputLevel::Default | OutputLevel::Quiet) {
            // --silent was not used and response is a wildcard, special messages abound when
            // this is the case...

            // create the base message
            let mut message = format!(
                "{} {:>8} {:>8}l {:>8}w {:>8}c Got {} for {}\n",
                wild_status,
                method,
                lines,
                words,
                chars,
                status_colorizer(status),
                self.url(),
            );

            if self.status().is_redirection() {
                // initial wildcard messages are wordy enough, put the redirect by itself
                url_with_redirect = format!(
                    "{} {:>9} {:>9} {:>9} {}\n",
                    wild_status, "-", "-", "-", url_with_redirect
                );

                // base message + redirection message (either empty string or redir msg)
                message.push_str(&url_with_redirect);
            }

            message
        } else {
            // not a wildcard, just create a normal entry
            if matches!(self.output_level, OutputLevel::SilentJSON) {
                self.as_json().unwrap_or_default()
            } else {
                utils::create_report_string(
                    self.status.as_str(),
                    method,
                    &lines,
                    &words,
                    &chars,
                    &url_with_redirect,
                    self.output_level,
                )
            }
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
    fn as_json(&self) -> anyhow::Result<String> {
        let mut json = serde_json::to_string(&self)
            .with_context(|| fmt_err(&format!("Could not convert {} to JSON", self.url())))?;
        json.push('\n');
        Ok(json)
    }
}

/// Serialize implementation for FeroxResponse
impl Serialize for FeroxResponse {
    /// Function that handles serialization of a FeroxResponse to NDJSON
    fn serialize<S>(&self, serializer: S) -> anyhow::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut headers = HashMap::new();
        let mut state = serializer.serialize_struct("FeroxResponse", 8)?;

        // need to convert the HeaderMap to a HashMap in order to pass it to the serializer
        for (key, value) in &self.headers {
            let k = key.as_str().to_owned();
            let v = String::from_utf8_lossy(value.as_bytes());
            headers.insert(k, v);
        }

        state.serialize_field("type", "response")?;
        state.serialize_field("url", self.url.as_str())?;
        state.serialize_field("original_url", self.original_url.as_str())?;
        state.serialize_field("path", self.url.path())?;
        state.serialize_field("wildcard", &self.wildcard)?;
        state.serialize_field("status", &self.status.as_u16())?;
        state.serialize_field("method", &self.method.as_str())?;
        state.serialize_field("content_length", &self.content_length)?;
        state.serialize_field("line_count", &self.line_count)?;
        state.serialize_field("word_count", &self.word_count)?;
        state.serialize_field("headers", &headers)?;
        state.serialize_field(
            "extension",
            self.extension.as_ref().unwrap_or(&String::new()),
        )?;
        state.serialize_field("timestamp", &self.timestamp)?;

        state.end()
    }
}

/// Deserialize implementation for FeroxResponse
impl<'de> Deserialize<'de> for FeroxResponse {
    /// Deserialize a FeroxResponse from a serde_json::Value
    fn deserialize<D>(deserializer: D) -> anyhow::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut response = Self {
            url: Url::parse("http://localhost").unwrap(),
            original_url: String::new(),
            status: StatusCode::OK,
            method: Method::GET,
            text: String::new(),
            content_length: 0,
            headers: HeaderMap::new(),
            wildcard: false,
            output_level: Default::default(),
            line_count: 0,
            word_count: 0,
            extension: None,
            timestamp: timestamp(),
        };

        let map: HashMap<String, Value> = HashMap::deserialize(deserializer)?;

        for (key, value) in &map {
            match key.as_str() {
                "url" => {
                    if let Some(url) = value.as_str() {
                        if let Ok(parsed) = parse_url_with_raw_path(url) {
                            response.url = parsed;
                        }
                    }
                }
                "original_url" => {
                    if let Some(og_url) = value.as_str() {
                        response.original_url = String::from(og_url);
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
                "method" => {
                    if let Some(method) = value.as_str() {
                        response.method = Method::from_bytes(method.as_bytes()).unwrap_or_default();
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
                "extension" => {
                    if let Some(result) = value.as_str() {
                        response.extension = Some(result.to_string());
                    }
                }
                "timestamp" => {
                    if let Some(result) = value.as_f64() {
                        response.timestamp = result;
                    }
                }
                _ => {}
            }
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Configuration;
    use std::default::Default;

    #[test]
    /// call reached_max_depth with max depth of zero, which is infinite recursion, expect false
    fn reached_max_depth_returns_early_on_zero() {
        let handles = Arc::new(Handles::for_testing(None, None).0);
        let url = Url::parse("http://localhost").unwrap();
        let response = FeroxResponse {
            url,
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        };

        let result = response.reached_max_depth(0, 2, handles);
        assert!(result);
    }

    #[test]
    /// simple case of a single extension gets parsed correctly and stored on the `FeroxResponse`
    fn parse_extension_finds_simple_extension() {
        let config = Configuration {
            collect_extensions: true,
            ..Default::default()
        };

        let (handles, _) = Handles::for_testing(None, Some(Arc::new(config)));

        let url = Url::parse("http://localhost/derp.js").unwrap();

        let mut response = FeroxResponse {
            url,
            ..Default::default()
        };

        response.parse_extension(Arc::new(handles)).unwrap();

        assert_eq!(response.extension, Some(String::from("js")));
    }

    #[test]
    /// hidden files shouldn't be parsed as extensions, i.e. `/.bash_history`
    fn parse_extension_ignores_hidden_files() {
        let config = Configuration {
            collect_extensions: true,
            ..Default::default()
        };

        let (handles, _) = Handles::for_testing(None, Some(Arc::new(config)));

        let url = Url::parse("http://localhost/.bash_history").unwrap();

        let mut response = FeroxResponse {
            url,
            ..Default::default()
        };

        response.parse_extension(Arc::new(handles)).unwrap();

        assert_eq!(response.extension, None);
    }

    #[test]
    /// `parse_extension` should return immediately if `--collect-extensions` isn't used
    fn parse_extension_early_returns_based_on_config() {
        let (handles, _) = Handles::for_testing(None, None);

        let url = Url::parse("http://localhost/derp.js").unwrap();

        let mut response = FeroxResponse {
            url,
            ..Default::default()
        };

        response.parse_extension(Arc::new(handles)).unwrap();

        assert_eq!(response.extension, None);
    }
}
