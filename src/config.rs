use clap::{value_t, App, Arg};
use lazy_static::lazy_static;
use reqwest::{redirect::Policy, Client, Proxy, StatusCode};
use serde::Deserialize;
use std::process::exit;
use std::fs::read_to_string;
use std::time::Duration;
use std::path::Path;


/// Version pulled from Cargo.toml at compile time
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default wordlist to use when `-w|--wordlist` isn't specified and not `wordlist` isn't set
/// in a [feroxbuster.toml](constant.DEFAULT_CONFIG_PATH.html) config file.
///
/// defaults to kali's default install location:
/// - `/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt`
pub const DEFAULT_WORDLIST: &str =
    "/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt";

/// Default list of response codes to report
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
pub const DEFAULT_RESPONSE_CODES: [StatusCode; 9] = [
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
pub const DEFAULT_CONFIG_NAME: &str = "feroxbuster.toml";

lazy_static! {
    /// Global configuration variable.
    pub static ref CONFIGURATION: Configuration = Configuration::new();
}

/// Represents the final, global configuration of the program.
///
/// This struct is the combination of the following:
/// - default configuration values
/// - plus overrides read from a configuration file
/// - plus command-line options
///
/// In that order.
#[derive(Debug, Clone, Deserialize)]
pub struct Configuration {
    #[serde(default = "wordlist")]
    pub wordlist: String,
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub target_url: String,
    #[serde(default = "statuscodes")]
    pub statuscodes: Vec<u16>,
    #[serde(skip)]
    pub client: Client,
    #[serde(default = "threads")]
    pub threads: usize,
    #[serde(default = "timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub verbosity: u8,
}

// functions timeout, threads, extensions, and wordlist are used to provide defaults in the
// event that a feroxbuster.toml is found but one or more of the values below aren't listed
// in the config.  This way, we get the correct defaults upon Deserialization
fn timeout() -> u64 {
    7
}
fn threads() -> usize {
    50
}
fn statuscodes() -> Vec<u16> {
    DEFAULT_RESPONSE_CODES
        .iter()
        .map(|code| code.as_u16())
        .collect()
}
fn wordlist() -> String {
    String::from(DEFAULT_WORDLIST)
}

impl Default for Configuration {
    fn default() -> Self {
        let timeout = timeout();

        let client = Self::create_client(timeout, None);

        Configuration {
            wordlist: wordlist(),
            target_url: String::new(),
            proxy: String::new(),
            statuscodes: statuscodes(),
            threads: threads(),
            timeout,
            verbosity: 0,
            client,
        }
    }
}

impl Configuration {
    /// Creates a [Configuration](struct.Configuration.html) object with the following
    /// built-in default values
    ///
    /// - timeout: 5 seconds
    /// - follow redirects: false
    /// - wordlist: [`DEFAULT_WORDLIST`](constant.DEFAULT_WORDLIST.html)
    /// - threads: 50
    /// - timeout: 7
    /// - verbosity: 0 (no logging enabled)
    /// - proxy: None
    /// - statuscodes: [`DEFAULT_RESPONSE_CODES`](constant.DEFAULT_RESPONSE_CODES.html)
    ///
    /// After which, any values defined in a
    /// [feroxbuster.toml](constant.DEFAULT_CONFIG_PATH.html) config file will override the
    /// built-in defaults.
    ///
    /// Finally, any options/arguments given on the commandline will override both built-in and
    /// config-file specified values.
    ///
    /// The resulting [Configuration](struct.Configuration.html) is a singleton with a `static`
    /// lifetime.
    pub fn new() -> Self {
        // todo: write integration test to handle this function; maybe with assert_cli
        // Get the default configuration, this is what will apply if nothing
        // else is specified.
        let mut config = Configuration::default();

        // Next, we parse the feroxbuster.toml file, if present and set the values
        // therein to overwrite our default values. Deserialized defaults are specified
        // in the Configuration struct so that we don't change anything that isn't
        // actually specified in the config file
        if let Some(settings) = Self::parse_config(Path::new(".")) {
            config.threads = settings.threads;
            config.wordlist = settings.wordlist;
            config.statuscodes = settings.statuscodes;
            config.proxy = settings.proxy;
            config.timeout = settings.timeout;
            config.verbosity = settings.verbosity;
        }

        let args = Self::arg_parser().get_matches();

        // the .is_some appears clunky, but it allows default values to be incrementally
        // overwritten from Struct defaults, to file config, to command line args, soooo ¯\_(ツ)_/¯
        if args.value_of("threads").is_some() {
            let threads = value_t!(args.value_of("threads"), usize).unwrap_or_else(|e| e.exit());
            config.threads = threads;
        }

        if args.value_of("timeout").is_some() {
            let timeout = value_t!(args.value_of("timeout"), u64).unwrap_or_else(|e| e.exit());
            config.timeout = timeout;
        }

        if args.value_of("proxy").is_some() {
            let tmp_proxy = args.value_of("proxy").unwrap();
            config.client = Self::create_client(config.timeout, Some(tmp_proxy));
            config.proxy = String::from(tmp_proxy);
        }

        if args.value_of("wordlist").is_some() {
            config.wordlist = String::from(args.value_of("wordlist").unwrap());
        }

        if args.values_of("statuscodes").is_some() {
            config.statuscodes = args
                .values_of("statuscodes")
                .unwrap()  // already known good
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes()).unwrap_or_else(|e| {eprintln!("[!] Error encountered: {}", e); exit(1)})
                        .as_u16()
                })
                .collect();
        }

        // occurrences_of returns 0 if none are found, which is desired behavior
        config.verbosity = args.occurrences_of("verbosity") as u8;

        // target_url is required, so no if statement is required
        config.target_url = String::from(args.value_of("url").unwrap());

        println!("{:#?}", config); // todo: remove eventually

        config
    }

    fn arg_parser() -> App<'static, 'static> {
        // todo!("add ignore certs option");
        // todo!("add headers option");
        // todo!("add user-agent option");
        // todo!("add redirect/no-redirect? option");
        // todo!("add quiet/include status codes in output option");

        App::new("feroxbuster")
            .version(VERSION)
            .author("epi (@epi052)")
            .about("A fast, simple, recursive content discovery tool written in Rust")
            .arg(
                Arg::with_name("wordlist")
                    .short("w")
                    .long("wordlist")
                    .value_name("FILE")
                    .help("Path to the wordlist")
                    .takes_value(true),
            )
            .arg(
                Arg::with_name("url")
                    .short("u")
                    .long("url")
                    .required(true)
                    .value_name("URL")
                    .help("The target URL"),
            )
            .arg(
                Arg::with_name("threads")
                    .short("t")
                    .long("threads")
                    .value_name("THREADS")
                    .takes_value(true)
                    .help("Number of concurrent threads (default: 50)"),
            )
            .arg(
                Arg::with_name("timeout")
                    .short("T")
                    .long("timeout")
                    .value_name("SECONDS")
                    .takes_value(true)
                    .help("Number of seconds before a request times out (default: 7)"),
            )
            .arg(
                Arg::with_name("verbosity")
                    .short("v")
                    .long("verbosity")
                    .takes_value(false)
                    .multiple(true)
                    .help("Increase verbosity level (use -vv or more for greater effect)"),
            )
            .arg(
                Arg::with_name("proxy")
                    .short("p")
                    .long("proxy")
                    .takes_value(true)
                    .help(
                        "Proxy to use for requests (ex: http(s)://host:port, socks5://host:port)",
                    ),
            )
            .arg(
                Arg::with_name("statuscodes")
                    .short("s")
                    .long("statuscodes")
                    .value_name("STATUS_CODE")
                    .takes_value(true)
                    .multiple(true)
                    .use_delimiter(true)
                    .help(
                        "Status Codes of interest (default: 200 204 301 302 307 308 401 403 405)",
                    ),
            )
    }

    /// If present, read in `DEFAULT_CONFIG_PATH` and deserialize the specified values
    ///
    /// uses serde to deserialize the toml into a `Configuration` struct
    ///
    /// If toml cannot be parsed a `Configuration::default` instance is returned
    fn parse_config(directory: &Path) -> Option<Self> {
        let directory = Path::new(directory);
        let directory = directory.join(DEFAULT_CONFIG_NAME);

        if let Ok(content) = read_to_string(directory) {
            // todo: remove unwrap
            let config: Self = toml::from_str(content.as_str()).unwrap();
            return Some(config);
        }
        None
    }

    fn create_client(timeout: u64, proxy: Option<&str>) -> Client {
        // todo: integration test for this as well, specifically redirect, timeout, proxy, etc
        let client = Client::builder()
            .timeout(Duration::new(timeout, 0))
            .redirect(Policy::none());

        let client = if proxy.is_some() && !proxy.unwrap().is_empty() {
            match Proxy::all(proxy.unwrap()) {
                Ok(proxy_obj) => client.proxy(proxy_obj),
                Err(e) => {
                    eprintln!(
                        "[!] Could not add proxy ({:?}) to Client configuration: {}",
                        proxy, e
                    );
                    client
                }
            }
        } else {
            // todo: do i wanna see this at the start of every run??
            eprintln!("[!] proxy ({:?}) not added to Client configuration", proxy);
            client
        };

        match client.build() {
            Ok(client) => client,
            Err(e) => {
                eprintln!("[!] Could not create a Client with the given configuration, exiting.");
                panic!("Client::build: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::TempDir;

    fn setup_config_test() -> Configuration {
        let data = r#"
            wordlist = "/some/path"
            statuscodes = [201, 301, 401]
            threads = 40
            timeout = 5
            proxy = "http://127.0.0.1:8080"
        "#;
        let tmp_dir = TempDir::new().unwrap();
        let file = tmp_dir.path().join(DEFAULT_CONFIG_NAME);
        write(file, data).unwrap();
        Configuration::parse_config(tmp_dir.path()).unwrap()
    }

    #[test]
    fn default_configuration() {
        let config = Configuration::default();
        assert_eq!(config.wordlist, wordlist());
        assert_eq!(config.proxy, String::new());
        assert_eq!(config.target_url, String::new());
        assert_eq!(config.statuscodes, statuscodes());
        assert_eq!(config.threads, threads());
        assert_eq!(config.timeout, timeout());
        assert_eq!(config.verbosity, 0);
    }

    #[test]
    fn config_reads_wordlist() {
        let config = setup_config_test();
        assert_eq!(config.wordlist, "/some/path");
    }

    #[test]
    fn config_reads_statuscodes() {
        let config = setup_config_test();
        assert_eq!(config.statuscodes, vec![201, 301, 401]);
    }

    #[test]
    fn config_reads_threads() {
        let config = setup_config_test();
        assert_eq!(config.threads, 40);
    }

    #[test]
    fn config_reads_timeout() {
        let config = setup_config_test();
        assert_eq!(config.timeout, 5);
    }

    #[test]
    fn config_reads_proxy() {
        let config = setup_config_test();
        assert_eq!(config.proxy, "http://127.0.0.1:8080");
    }
}
