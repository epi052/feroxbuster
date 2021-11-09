use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
    sync::Arc,
};

use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Response, StatusCode, Url,
};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{
    config::OutputLevel,
    event_handlers::{Command, Handles},
    traits::FeroxSerialize,
    url::FeroxUrl,
    utils::{self, fmt_err, status_colorizer},
    CommandSender,
};

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

    /// whether the user passed --quiet|--silent on the command line
    pub(crate) output_level: OutputLevel,
}

/// implement Default trait for FeroxResponse
impl Default for FeroxResponse {
    /// return a default reqwest::Url and then normal defaults after that
    fn default() -> Self {
        Self {
            url: Url::parse("http://localhost").unwrap(),
            status: Default::default(),
            text: "".to_string(),
            content_length: 0,
            line_count: 0,
            word_count: 0,
            headers: Default::default(),
            wildcard: false,
            output_level: Default::default(),
        }
    }
}

/// Implement Display for FeroxResponse
impl fmt::Display for FeroxResponse {
    /// formatter for Display
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

    /// Set `FeroxResponse`'s `url` attribute, has no affect if an error occurs
    pub fn set_url(&mut self, url: &str) {
        match Url::parse(url) {
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
        self.text = String::new();
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
    pub async fn from(response: Response, read_body: bool, output_level: OutputLevel) -> Self {
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
                    log::warn!("Could not parse body from response: {}", e);
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
            output_level,
            wildcard: false,
        }
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

        report_sender.send(Command::Report(Box::new(self)))?;

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
        let wild_status = status_colorizer("WLD");

        if self.wildcard && matches!(self.output_level, OutputLevel::Default | OutputLevel::Quiet) {
            // --silent was not used and response is a wildcard, special messages abound when
            // this is the case...

            // create the base message
            let mut message = format!(
                "{} {:>8}l {:>8}w {:>8}c Got {} for {} (url length: {})\n",
                wild_status,
                lines,
                words,
                chars,
                status_colorizer(status),
                self.url(),
                FeroxUrl::path_length_of_url(&self.url)
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
                self.output_level,
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
    fn as_json(&self) -> anyhow::Result<String> {
        let mut json = serde_json::to_string(&self)
            .with_context(|| fmt_err(&format!("Could not convert {} to JSON", self.url())))?;
        json.push('\n');
        Ok(json)
    }

    fn as_csv(&self) -> String {
        format!(
            "{},{},{},{},{},{}\n",
            self.url,
            self.status,
            self.wildcard,
            self.content_length,
            self.line_count,
            self.word_count,
          
        )
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
    fn deserialize<D>(deserializer: D) -> anyhow::Result<Self, D::Error>
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
            output_level: Default::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
            output_level: Default::default(),
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
            output_level: Default::default(),
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
            output_level: Default::default(),
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
            output_level: Default::default(),
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
            output_level: Default::default(),
        };

        let result = response.reached_max_depth(0, 2, handles);
        assert!(result);
    }
}
