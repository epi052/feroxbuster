use super::Configuration;
use crate::{
    utils::{module_colorizer, parse_url_with_raw_path, status_colorizer},
    DEFAULT_BACKUP_EXTENSIONS, DEFAULT_IGNORED_EXTENSIONS, DEFAULT_METHOD, DEFAULT_STATUS_CODES,
    DEFAULT_WORDLIST, VERSION,
};
use anyhow::{bail, Result};
use std::collections::HashMap;

#[cfg(not(test))]
use std::process::exit;

/// simple helper to clean up some code reuse below; panics under test / exits in prod
pub(super) fn report_and_exit(err: &str) -> ! {
    eprintln!(
        "{} {}: {}",
        status_colorizer("ERROR"),
        module_colorizer("Configuration::new"),
        err
    );

    #[cfg(test)]
    panic!();
    #[cfg(not(test))]
    exit(1);
}

// functions timeout, threads, status_codes, user_agent, wordlist, save_state, and depth are used to provide
// defaults in the event that a ferox-config.toml is found but one or more of the values below
// aren't listed in the config.  This way, we get the correct defaults upon Deserialization

/// default Configuration type for use in json output
pub(super) fn serialized_type() -> String {
    String::from("configuration")
}

/// default timeout value
pub(super) fn timeout() -> u64 {
    7
}

/// default save_state value
pub(super) fn save_state() -> bool {
    true
}

/// default threads value
pub(super) fn threads() -> usize {
    50
}

/// default protocol value
pub(super) fn request_protocol() -> String {
    String::from("https")
}

/// default status codes
pub(super) fn status_codes() -> Vec<u16> {
    DEFAULT_STATUS_CODES
        .iter()
        .map(|code| code.as_u16())
        // add experimental codes not found in reqwest
        // - 103 - EARLY_HINTS
        // - 425 - TOO_EARLY
        .chain([103, 425])
        .collect()
}

/// default HTTP Method
pub(super) fn methods() -> Vec<String> {
    vec![DEFAULT_METHOD.to_owned()]
}

/// default extensions to ignore while auto-collecting
pub(super) fn ignored_extensions() -> Vec<String> {
    DEFAULT_IGNORED_EXTENSIONS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// default backup extensions to collect
pub(super) fn backup_extensions() -> Vec<String> {
    DEFAULT_BACKUP_EXTENSIONS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// default wordlist
pub(super) fn wordlist() -> String {
    String::from(DEFAULT_WORDLIST)
}

/// default user-agent
pub(super) fn user_agent() -> String {
    format!("feroxbuster/{VERSION}")
}

/// default recursion depth
pub(super) fn depth() -> usize {
    4
}

/// default extract links
pub(super) fn extract_links() -> bool {
    true
}

/// enum representing the three possible states for informational output (not logging verbosity)
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum OutputLevel {
    /// normal scan, no --quiet|--silent
    Default,

    /// quiet scan, print some information, but not all (new in versions >= 2.0.0)
    Quiet,

    /// silent scan, only print urls (used to be --quiet in versions 1.x.x)
    Silent,

    /// silent scan, but with JSON output
    SilentJSON,
}

/// implement a default for OutputLevel
impl Default for OutputLevel {
    /// return Default
    fn default() -> Self {
        Self::Default
    }
}

/// given the current settings for quiet and silent, determine output_level (DRY helper)
pub fn determine_output_level(quiet: bool, silent: bool, json: bool) -> OutputLevel {
    if quiet && silent {
        // user COULD have both as true in config file, take the more quiet of the two
        if json {
            OutputLevel::SilentJSON
        } else {
            OutputLevel::Silent
        }
    } else if quiet {
        OutputLevel::Quiet
    } else if silent {
        if json {
            OutputLevel::SilentJSON
        } else {
            OutputLevel::Silent
        }
    } else {
        OutputLevel::Default
    }
}

/// represents actions the Requester should take in certain situations
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum RequesterPolicy {
    /// automatically try to lower request rate in order to reduce errors
    AutoTune,

    /// automatically bail at certain error thresholds
    AutoBail,

    /// just let that junk run super natural
    Default,
}

/// default implementation for RequesterPolicy
impl Default for RequesterPolicy {
    /// Default as default
    fn default() -> Self {
        Self::Default
    }
}

/// given the current settings for quiet and silent, determine output_level (DRY helper)
pub fn determine_requester_policy(auto_tune: bool, auto_bail: bool) -> RequesterPolicy {
    if auto_tune && auto_bail {
        // user COULD have both as true in config file, take the more aggressive of the two
        RequesterPolicy::AutoBail
    } else if auto_tune {
        RequesterPolicy::AutoTune
    } else if auto_bail {
        RequesterPolicy::AutoBail
    } else {
        RequesterPolicy::Default
    }
}

/// Splits a query string into a key-value pair.
///
/// This function takes a query string in the format of `"key=value"` and splits it into
/// a tuple containing the key and value as separate strings. If the query string is
/// malformed (e.g., empty or without a key), it returns an error.
///
/// # Arguments
///
/// * `query` - A string slice that holds the query string to be split.
///
/// # Returns
///
/// * `Result<(String, String)>` - A tuple containing the key and value as `String`s,
///   or an error if the input is invalid.
///
/// # Errors
///
/// This function will return an error if:
/// * The input string is empty or equal to `"="`.
/// * The key part of the query string is empty (i.e., if the string starts with `"="`).
///
/// # Examples
///
/// ```
/// let result = split_query("name=John");
/// assert_eq!(result.unwrap(), ("name".to_string(), "John".to_string()));
///
/// let result = split_query("name=");
/// assert_eq!(result.unwrap(), ("name".to_string(), "".to_string()));
///
/// let result = split_query("name=John=Doe");
/// assert_eq!(result.unwrap(), ("name".to_string(), "John=Doe".to_string()));
///
/// let result = split_query("=John");
/// assert!(result.is_err());
///
/// let result = split_query("");
/// assert!(result.is_err());
/// ```
pub fn split_query(query: &str) -> Result<(String, String)> {
    if query.is_empty() || query == "=" {
        bail!("Empty query string provided");
    }

    let mut split_val = query.split('=');

    let name = split_val.next().unwrap().trim();

    if name.is_empty() {
        bail!("Empty key in query string");
    }

    let value = split_val.collect::<Vec<&str>>().join("=");

    Ok((name.to_string(), value.to_string()))
}

/// Splits an HTTP header string into a key-value pair.
///
/// This function takes a header string in the format of `"Key: Value"` and splits it into
/// a tuple containing the key and value as separate strings. If the header string is
/// malformed (e.g., empty or missing a key), it returns an error.
///
/// # Arguments
///
/// * `header` - A string slice that holds the header string to be split.
///
/// # Returns
///
/// * `Result<(String, String)>` - A tuple containing the key and value as `String`s,
///   or an error if the input is invalid.
///
/// # Errors
///
/// This function will return an error if:
/// * The input string is empty.
/// * The key part of the header string is empty (i.e., if the string starts with `":"`).
///
/// # Examples
///
/// ```
/// let result = split_header("Content-Type: application/json");
/// assert_eq!(result.unwrap(), ("Content-Type".to_string(), "application/json".to_string()));
///
/// let result = split_header("Content-Length: 1234");
/// assert_eq!(result.unwrap(), ("Content-Length".to_string(), "1234".to_string()));
///
/// let result = split_header("Authorization: Bearer token");
/// assert_eq!(result.unwrap(), ("Authorization".to_string(), "Bearer token".to_string()));
///
/// let result = split_header("InvalidHeader");
/// assert!(result.is_err());
///
/// let result = split_header("");
/// assert!(result.is_err());
/// ```
pub fn split_header(header: &str) -> Result<(String, String)> {
    if header.is_empty() {
        bail!("Empty header provided");
    }

    let mut split_val = header.split(':');

    // explicitly take first split value as header's name
    let name = split_val.next().unwrap().trim().to_string();

    if name.is_empty() {
        bail!("Empty header name provided");
    }

    // all other items in the iterator returned by split, when combined with the
    // original split deliminator (:), make up the header's final value
    let value = split_val.collect::<Vec<&str>>().join(":");

    if value.starts_with(' ') && !value.starts_with("  ") {
        // first character is a space and the second character isn't
        // we can trim the leading space
        let trimmed = value.trim_start();
        Ok((name, trimmed.to_string()))
    } else {
        Ok((name, value))
    }
}

/// Combines two `Cookie` header strings into a single, unified `Cookie` header string.
///
/// The function parses both input strings into individual key-value pairs, ensuring that each
/// key is unique. If a key appears in both input strings, the value from the second string
/// will override the value from the first string. The resulting combined `Cookie` header string
/// is returned with all key-value pairs separated by `;`.
///
/// # Arguments
///
/// * `cookie1` - A string slice representing the first `Cookie` header.
/// * `cookie2` - A string slice representing the second `Cookie` header.
///
/// # Returns
///
/// * A `String` containing the combined `Cookie` header with unique keys.
///
/// # Example
///
/// ```
/// let cookie1 = "super=duper; stuff=things";
/// let cookie2 = "stuff=mothings; derp=tronic";
/// let combined_cookie = combine_cookies(cookie1, cookie2);
/// assert_eq!(combined_cookie, "super=duper; stuff=mothings; derp=tronic");
/// ```
///
/// The output string will contain all unique keys from both input strings, with the value
/// from the second string taking precedence in the case of key collisions.
fn combine_cookies(cookie1: &str, cookie2: &str) -> String {
    let mut cookie_map = HashMap::new();

    // Helper function to parse a cookie string and insert it into the map
    let parse_cookie = |cookie_str: &str, map: &mut HashMap<String, String>| {
        for pair in cookie_str.split(';') {
            let mut key_value = pair.trim().splitn(2, '=');
            if let (Some(key), Some(value)) = (key_value.next(), key_value.next()) {
                map.insert(key.to_string(), value.to_string());
            }
        }
    };

    // Parse both cookie strings into the map
    parse_cookie(cookie1, &mut cookie_map);
    parse_cookie(cookie2, &mut cookie_map);

    // Build the final cookie header string
    cookie_map
        .into_iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Parses a raw HTTP request from a file and updates the provided configuration.
///
/// This function reads an HTTP request from the file specified by `config.request_file`,
/// parses the request line, headers, and body, and updates the `config` object
/// with the parsed values. If certain elements (e.g., headers or body) are
/// already provided via the CLI, they take precedence over the parsed values.
///
/// # Arguments
///
/// * `config` - A mutable reference to a `Configuration` object that will be
///   updated with the parsed request data.
///
/// # Returns
///
/// * `Result<()>` - Returns `Ok(())` if parsing and configuration updates
///   were successful, or an error if the raw file or request is invalid.
///
/// # Errors
///
/// This function will return an error if:
/// * The file specified in `config.request_file` is empty.
/// * The request is malformed (e.g., missing the request line, method, or URI).
/// * Required headers are missing (e.g., `Host` when the request line URI is not a full URL).
///
/// # Details
///
/// * The request body is only set if it hasn't been overridden by the CLI options.
/// * The request line method is added to `config.methods` if it's not already present.
/// * Headers from the raw request are added to `config.headers`, unless overridden
///   by CLI options. Special handling is applied to `User-Agent`, `Content-Length`,
///   and `Cookie` headers.
/// * The request URI is validated and parsed. If it's not a full URL, it will be
///   combined with the `Host` header to form a full target URL.
/// * Query parameters are extracted from the URI and added to `config.queries`,
///   unless overridden by CLI options.
///
/// # Examples
///
/// ```rust
/// let mut config = Configuration::default();
/// config.request_file = "path/to/raw/request.txt".to_string();
///
/// let result = parse_request_file(&mut config);
/// assert!(result.is_ok());
/// assert_eq!(config.methods, vec!["GET".to_string()]);
/// assert_eq!(config.target_url, "http://example.com/path".to_string());
/// assert_eq!(config.headers.get("User-Agent").unwrap(), "MyCustomAgent");
/// assert_eq!(config.data, b"key=value".to_vec());
/// ```
pub fn parse_request_file(config: &mut Configuration) -> Result<()> {
    // read in the file located at config.request_file
    // parse the file into a Request struct
    let contents = std::fs::read_to_string(&config.request_file)?;

    if contents.is_empty() {
        bail!("Empty --request-file file provided");
    }

    // this should split the body from the request line and headers
    let lines = contents.split("\r\n\r\n").collect::<Vec<&str>>();

    if lines.len() < 2 {
        bail!("Invalid request: Missing head/body CRLF separator");
    }

    let head = lines[0];
    let body = lines[1].as_bytes().to_vec();

    // we only want to use the request's body if the user hasn't
    // overridden it on the cli
    if config.data.is_empty() {
        config.data = body;
    }

    // begin parsing the request line and headers
    let mut head_parts = head.split("\r\n");

    let Some(request_line) = head_parts.next() else {
        bail!("Invalid request: Missing request line");
    };

    if request_line.is_empty() {
        bail!("Invalid request: Empty request line");
    }

    let mut request_parts = request_line.split_whitespace();

    let Some(method) = request_parts.next() else {
        bail!("Invalid request: Missing method");
    };

    if method.is_empty() {
        bail!("Invalid request: Empty method");
    }

    let method = method.to_string();

    if !config.methods.contains(&method) {
        config.methods.push(method);
    }

    let Some(uri) = request_parts.next() else {
        bail!("Invalid request: Missing request line URI");
    };

    if uri.is_empty() {
        bail!("Invalid request: Empty request line URI");
    }

    for mut line in head_parts {
        line = line.trim();

        if line.is_empty() {
            break; // Empty line signals the end of headers
        }

        let Ok((name, value)) = split_header(line) else {
            log::warn!("Invalid header: {}", line);
            continue;
        };

        if name.is_empty() {
            log::warn!("Invalid header name: {}", line);
            continue;
        }

        if name.to_lowercase() == "user-agent" {
            if config.user_agent == user_agent() {
                config.user_agent = value;
            }
            continue;
        }

        if name.to_lowercase() == "content-length" {
            log::debug!("Skipping content-length header, a new one will be created");
            continue;
        }

        if config.headers.contains_key(&name) {
            if name.to_lowercase() == "cookie" {
                // the cookie header already exists, so we need to extend it with
                // our values and ensure cli-provided cookie values override those
                // from the request
                let existing = config.headers.get_mut(&name).unwrap();
                // second param takes precedence over first
                let combined = combine_cookies(&value, existing);
                *existing = combined;
                continue;
            }
            log::debug!("Found header from cli, overriding raw request with cli entry: {name}");
            continue;
        }

        config.headers.insert(name, value);
    }

    let url = parse_url_with_raw_path(uri);

    if url.is_err() {
        // uri in request line is not a valid URL, so it's most likely a path/relative url
        // we need to combine it with the host header
        for (key, value) in &config.headers {
            if key.to_lowercase() == "host" {
                config.target_url = format!("{}{}", value, uri);
                break;
            }
        }

        if config.target_url.is_empty() {
            bail!("Invalid request: Missing Host header and request line URI isn't a full URL");
        }

        // need to parse queries from the uri, if any are present
        let mut uri_parts = uri.splitn(2, '?');

        // skip the path
        uri_parts.next();

        if let Some(queries) = uri_parts.next() {
            let query_parts = queries.split("&");

            query_parts.into_iter().for_each(|query| {
                let Ok((name, value)) = split_query(query) else {
                    return;
                };
                for (k, _) in &config.queries {
                    if k.to_lowercase() == name.to_lowercase() {
                        // allow cli options to take precedent when query names match
                        return;
                    }
                }

                config.queries.push((name, value));
            });
        }
    } else {
        let mut url = url.unwrap();

        if let Some(host) = config.headers.get("Host") {
            url.set_host(Some(host)).unwrap();
        }

        url.query_pairs().for_each(|(key, value)| {
            for (k, _) in &config.queries {
                if k.to_lowercase() == key.to_lowercase() {
                    // allow cli options to take precedent when query names match
                    return;
                }
            }

            config.queries.push((key.to_string(), value.to_string()));
        });

        url.set_query(None);
        url.set_fragment(None);

        config.target_url = url.to_string();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs::{self, File};
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempSetup {
        pub path: PathBuf,
        pub config: Configuration,
        pub file: File,
    }

    impl TempSetup {
        pub fn new() -> Self {
            let mut temp_dir: PathBuf = env::temp_dir();

            temp_dir.push(format!(
                "temp_request_file_{}.txt",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));

            let config: Configuration = Configuration {
                request_file: temp_dir.to_str().unwrap().to_string(),
                ..Default::default()
            };

            let file = File::create(&temp_dir).unwrap();

            Self {
                path: temp_dir,
                config,
                file,
            }
        }

        pub fn cleanup(self) {
            fs::remove_file(self.path).unwrap();
        }
    }

    #[test]
    /// test determine_output_level returns higher of the two levels if both given values are true
    fn determine_output_level_returns_correct_results() {
        let mut level = determine_output_level(true, true, false);
        assert_eq!(level, OutputLevel::Silent);

        level = determine_output_level(false, true, false);
        assert_eq!(level, OutputLevel::Silent);

        let mut level = determine_output_level(true, true, true);
        assert_eq!(level, OutputLevel::SilentJSON);

        level = determine_output_level(false, true, true);
        assert_eq!(level, OutputLevel::SilentJSON);

        level = determine_output_level(false, false, false);
        assert_eq!(level, OutputLevel::Default);

        level = determine_output_level(true, false, false);
        assert_eq!(level, OutputLevel::Quiet);

        level = determine_output_level(false, false, true);
        assert_eq!(level, OutputLevel::Default);

        level = determine_output_level(true, false, true);
        assert_eq!(level, OutputLevel::Quiet);
    }

    #[test]
    /// test determine_requester_policy returns higher of the two levels if both given values are true
    fn determine_requester_policy_returns_correct_results() {
        let mut level = determine_requester_policy(true, true);
        assert_eq!(level, RequesterPolicy::AutoBail);

        level = determine_requester_policy(false, true);
        assert_eq!(level, RequesterPolicy::AutoBail);

        level = determine_requester_policy(false, false);
        assert_eq!(level, RequesterPolicy::Default);

        level = determine_requester_policy(true, false);
        assert_eq!(level, RequesterPolicy::AutoTune);
    }

    #[test]
    #[should_panic]
    /// report_and_exit should panic/exit when called
    fn report_and_exit_panics_under_test() {
        report_and_exit("test");
    }

    #[test]
    fn test_split_query_simple() {
        let query = "name=value";
        let result = split_query(query).unwrap();
        assert_eq!(result, ("name".to_string(), "value".to_string()));
    }

    #[test]
    fn test_split_query_with_spaces() {
        let query = " name = value ";
        let result = split_query(query).unwrap();
        assert_eq!(result, ("name".to_string(), " value ".to_string()));
    }

    #[test]
    fn test_split_query_empty_value() {
        let query = "name=";
        let result = split_query(query).unwrap();
        assert_eq!(result, ("name".to_string(), "".to_string()));
    }

    #[test]
    fn test_split_query_no_value() {
        let query = "name";
        let result = split_query(query).unwrap();
        assert_eq!(result, ("name".to_string(), "".to_string()));
    }

    #[test]
    fn test_split_query_multiple_equals() {
        let query = "name=value=another";
        let result = split_query(query).unwrap();
        assert_eq!(result, ("name".to_string(), "value=another".to_string()));
    }

    #[test]
    fn test_split_query_empty_key_and_value() {
        let query = "=";
        let result = split_query(query);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_query_empty_key() {
        let query = "=value";
        let result = split_query(query);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_query_trailing_equals_in_value() {
        let query = "name=value=";
        let result = split_query(query).unwrap();
        assert_eq!(result, ("name".to_string(), "value=".to_string()));
    }

    #[test]
    fn test_split_query_no_equals() {
        let query = "just_a_key";
        let result = split_query(query).unwrap();
        assert_eq!(result, ("just_a_key".to_string(), "".to_string()));
    }

    #[test]
    fn test_split_query_empty_input() {
        let query = "";
        assert!(split_query(query).is_err());
    }

    #[test]
    fn test_split_header_simple() -> Result<()> {
        let header = "Content-Type: text/html";
        let result = split_header(header)?;
        assert_eq!(
            result,
            ("Content-Type".to_string(), "text/html".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_split_header_with_leading_space_in_value() -> Result<()> {
        let header = "Content-Type:  text/html";
        let result = split_header(header)?;
        assert_eq!(
            result,
            ("Content-Type".to_string(), "  text/html".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_split_header_with_trimmed_leading_space() -> Result<()> {
        let header = "Content-Type: text/html";
        let result = split_header(header)?;
        assert_eq!(
            result,
            ("Content-Type".to_string(), "text/html".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_split_header_with_multiple_colons() -> Result<()> {
        let header = "Date: Mon, 27 Jul 2009 12:28:53 GMT";
        let result = split_header(header)?;
        assert_eq!(
            result,
            (
                "Date".to_string(),
                "Mon, 27 Jul 2009 12:28:53 GMT".to_string()
            )
        );
        Ok(())
    }

    #[test]
    fn test_split_header_empty_value() -> Result<()> {
        let header = "X-Custom-Header: ";
        let result = split_header(header)?;
        assert_eq!(result, ("X-Custom-Header".to_string(), "".to_string()));
        Ok(())
    }

    #[test]
    fn test_split_header_no_value() -> Result<()> {
        let header = "X-Custom-Header:";
        let result = split_header(header)?;
        assert_eq!(result, ("X-Custom-Header".to_string(), "".to_string()));
        Ok(())
    }

    #[test]
    fn test_split_header_no_colon() -> Result<()> {
        let header = "InvalidHeader";
        let result = split_header(header)?;
        assert_eq!(result, ("InvalidHeader".to_string(), "".to_string()));
        Ok(())
    }

    #[test]
    fn test_split_header_empty_key() {
        let header = ": value";
        let result = split_header(header);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Empty header name provided"
        );
    }

    #[test]
    fn test_split_header_empty_key_and_value() {
        let header = ": ";
        let result = split_header(header);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Empty header name provided"
        );
    }

    #[test]
    fn test_split_header_empty_input() {
        let header = "";
        let result = split_header(header);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Empty header provided");
    }

    #[test]
    fn test_split_header_value_with_leading_single_space() -> Result<()> {
        let header = "Authorization: Bearer token";
        let result = split_header(header)?;
        assert_eq!(
            result,
            ("Authorization".to_string(), "Bearer token".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_split_header_value_with_leading_multiple_spaces() -> Result<()> {
        let header = "Authorization:  Bearer token";
        let result = split_header(header)?;
        assert_eq!(
            result,
            ("Authorization".to_string(), "  Bearer token".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_empty_request() {
        let mut config = Configuration::new().unwrap();
        let result = parse_request_file(&mut config);
        assert!(result.is_err());
    }
    #[test]
    fn test_parse_raw_with_empty_file() -> io::Result<()> {
        let mut tmp = TempSetup::new();

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Empty --request-file file provided"
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_without_head_body_crlf() -> io::Result<()> {
        let mut tmp = TempSetup::new();

        write!(tmp.file, "GET / HTTP/1.1\r\n")?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid request: Missing head/body CRLF separator"
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_only_head_body_crlf() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        writeln!(tmp.file, "\r\n\r\n")?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid request: Empty request line"
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_body_is_overridden_by_cli() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET http://localhost/srv HTTP/1.0\r\n\r\nrequest-body"
        )?;

        parse_request_file(&mut tmp.config).unwrap();
        assert_eq!(tmp.config.data, b"request-body".to_vec());

        tmp.config.data = b"cli-data".to_vec();

        parse_request_file(&mut tmp.config).unwrap();
        assert_eq!(tmp.config.data, b"cli-data".to_vec());

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_empty_request_line() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(tmp.file, "\r\nHost: example.com\r\n\r\n")?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid request: Empty request line"
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_missing_uri() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(tmp.file, "GET\r\nHost: example.com\r\n\r\n")?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid request: Missing request line URI"
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_missing_method() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(tmp.file, "  \r\nHost: example.com\r\n\r\n")?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid request: Missing method"
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_methods_are_appended_if_unique() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "POST / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test-agent\r\n\r\n"
        )?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(tmp.config.methods, vec!["GET", "POST"]);

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_methods_are_ignored_if_already_present_from_cli() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test-agent\r\n\r\n"
        )?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(tmp.config.methods, vec!["GET"]);

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_headers_added_to_config_if_missing_else_overridden_from_cli() -> io::Result<()>
    {
        let mut tmp: TempSetup = TempSetup::new();

        // header from cli
        tmp.config
            .headers
            .insert(String::from("stuff"), String::from("things"));

        // stuff header will be overridden by the one in the cli config (i.e. the raw request's
        // stuff header will be ignored because of the cli config)
        write!(
            tmp.file,
            "GET / HTTP/1.1\r\nHost: example.com\r\nstuff: mothings\r\n\r\n"
        )?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert!(tmp.config.headers.contains_key("Host"));
        assert_eq!(tmp.config.headers.get("stuff").unwrap(), "things");

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_user_agent_in_request() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test-agent\r\n\r\n"
        )?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(tmp.config.user_agent, "test-agent");

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_user_agent_in_request_and_cli() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET / HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test-agent\r\n\r\n"
        )?;

        tmp.config.user_agent = "cli-agent".to_string();

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(tmp.config.user_agent, "cli-agent");

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_content_length_is_always_skipped() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET / HTTP/1.1\r\nHost: example.com\r\nContent-length: 21\r\n\r\n"
        )?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert!(!tmp.config.headers.contains_key("Content-length"));

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_cookie_header_appended_or_overridden() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET / HTTP/1.1\r\nHost: example.com\r\nCookie: derp=tronic2; super=duper2\r\n\r\n"
        )?;

        tmp.config.headers.insert(
            "Cookie".to_string(),
            "derp=tronic; stuff=things".to_string(),
        );

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());

        let cookies = tmp.config.headers.get("Cookie").unwrap();

        assert!(cookies.contains("derp=tronic"));
        assert!(cookies.contains("stuff=things"));
        assert!(cookies.contains("super=duper2"));

        // got overridden
        assert!(!cookies.contains("derp=tronic2"));

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_relative_path_and_partial_host_header() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(tmp.file, "GET /srv HTTP/1.1\r\nHost: example.com\r\n\r\n")?;

        let result = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(tmp.config.target_url, "example.com/srv");

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_relative_path_and_no_host_header() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(tmp.file, "GET /srv HTTP/1.1\r\n\r\n")?;

        let result: std::result::Result<(), anyhow::Error> = parse_request_file(&mut tmp.config);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Invalid request: Missing Host header and request line URI isn't a full URL"
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_full_url_and_no_host_header() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(tmp.file, "GET http://localhost/srv HTTP/1.1\r\n\r\n")?;

        let result: std::result::Result<(), anyhow::Error> = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(tmp.config.target_url, "http://localhost/srv");

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_full_url_and_host_header() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET http://localhost/srv HTTP/1.1\r\nHost: example.com\r\n\r\n"
        )?;

        let result: std::result::Result<(), anyhow::Error> = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(tmp.config.target_url, "http://example.com/srv");

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_partial_url_and_queries() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET /srv?mostuff=mothings&derp=tronic2 HTTP/1.1\r\nHost: example.com\r\n\r\n"
        )?;

        tmp.config
            .queries
            .push(("derp".to_string(), "tronic".to_string()));
        tmp.config
            .queries
            .push(("stuff".to_string(), "things".to_string()));

        let result: std::result::Result<(), anyhow::Error> = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(
            tmp.config.queries,
            vec![
                (String::from("derp"), String::from("tronic")),
                (String::from("stuff"), String::from("things")),
                (String::from("mostuff"), String::from("mothings"))
            ]
        );

        tmp.cleanup();
        Ok(())
    }

    #[test]
    fn test_parse_raw_with_full_url_and_queries() -> io::Result<()> {
        let mut tmp: TempSetup = TempSetup::new();

        write!(
            tmp.file,
            "GET http://localhost/srv?mostuff=mothings&derp=tronic2 HTTP/1.1\r\nHost: example.com\r\n\r\n"
        )?;

        tmp.config
            .queries
            .push(("derp".to_string(), "tronic".to_string()));
        tmp.config
            .queries
            .push(("stuff".to_string(), "things".to_string()));

        let result: std::result::Result<(), anyhow::Error> = parse_request_file(&mut tmp.config);

        assert!(result.is_ok());
        assert_eq!(
            tmp.config.queries,
            vec![
                (String::from("derp"), String::from("tronic")),
                (String::from("stuff"), String::from("things")),
                (String::from("mostuff"), String::from("mothings"))
            ]
        );

        tmp.cleanup();
        Ok(())
    }
}
