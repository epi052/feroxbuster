use clap::{value_t, App, Arg, ArgMatches};
use reqwest::{redirect::Policy, Client, StatusCode};
use std::time::Duration;
use lazy_static::lazy_static;
use std::fs::read_to_string;
use serde::{Deserialize};


/// Version pulled from Cargo.toml at compile time
const VERSION: &'static str = env!("CARGO_PKG_VERSION");

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
pub const DEFAULT_CONFIG_PATH: &str = "feroxbuster.toml";

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
    #[serde(default)]
    pub wordlist: String,
    #[serde(default)]
    pub target_url: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(skip)]
    pub client: Client,
    #[serde(default = "fifty")]
    pub threads: usize,
    #[serde(default = "seven")]
    pub timeout: u64,
    #[serde(default)]
    pub verbosity: u8,
}

fn seven() -> u64 {7}
fn fifty() -> usize {50}


impl Default for Configuration {
    fn default() -> Self {
        let timeout = 7;

        let client = Client::builder()
            .timeout(Duration::new(timeout, 0))
            .redirect(Policy::none())
            .build()
            .unwrap();

        Configuration {
            wordlist: String::from(DEFAULT_WORDLIST),
            target_url: String::new(),
            extensions: vec![],
            threads: 50,
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
    /// - wordlist: [DEFAULT_WORDLIST](constant.DEFAULT_WORDLIST.html)
    /// - threads: 50
    /// - timeout: 7
    /// - verbosity: 0 (no logging enabled)
    ///
    /// After which, any values defined in the settings section of a
    /// [feroxbuster.toml](constant.DEFAULT_CONFIG_PATH.html) config file will override the
    /// built-in defaults.
    ///
    /// Finally, any options/arguments given on the commandline will override both built-in and
    /// config-file specified values.
    ///
    /// The resulting [Configuration](struct.Configuration.html) is a singleton with a `static`
    /// lifetime.
    pub fn new() -> Self {
        // Get the default configuration, this is what will apply if nothing
        // else is specified.
        let mut config = Configuration::default();



        if let Some(settings) = Self::parse_config() {
            config.target_url = settings.target_url;
            config.threads = settings.threads;
            config.wordlist = settings.wordlist;
            config.extensions = settings.extensions;
        }

        let args = Self::parse_args();

        // the .is_some appears clunky, but it allows default values to be incrementally
        // overwritten from Struct defaults, to file config, to command line args, ¯\_(ツ)_/¯
        if args.value_of("threads").is_some() {
            let threads = value_t!(args.value_of("threads"), usize).unwrap_or_else(|e| e.exit());
            config.threads = threads;
        }

        if args.value_of("timeout").is_some() {
            let timeout = value_t!(args.value_of("timeout"), u64).unwrap_or_else(|e| e.exit());
            config.timeout = timeout;
        }

        if args.value_of("wordlist").is_some() {
            config.wordlist = String::from(args.value_of("wordlist").unwrap());
        }

        // occurrences_of returns 0 if none are found, which is desired behavior
        config.verbosity = args.occurrences_of("verbosity") as u8;

        // target_url is required, so no if statement is required
        config.target_url = String::from(args.value_of("url").unwrap());

        config
    }

    fn parse_args() -> ArgMatches<'static> {
        // todo!("add proxy option");
        // todo!("add ignore certs option");
        // todo!("add headers option");
        // todo!("add user-agent option");
        // todo!("add status codes option");
        // todo!("add redirect/no-redirect? option");
        // todo!("add quiet/include status codes in output option");

        App::new("feroxbuster")
            .version(VERSION)
            .author("epi <epibar052@gmail.com>")
            .about("A fast, simple, recursive content discovery tool written in Rust")
            .arg(
                Arg::with_name("wordlist")
                    .short("w")
                    .long("wordlist")
                    .value_name("FILE")
                    .help("Path to the wordlist")
                    .takes_value(true)
            )
            .arg(
                Arg::with_name("url")
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
                    .help("Increase verbosity level (use -vv or more for greater effect)"))

            .get_matches()
    }

    /// If present, read in `DEFAULT_CONFIG_PATH` and deserialize the specified values
    ///
    /// uses serde to deserialize the toml into a `Configuration` struct
    ///
    /// If toml cannot be parsed a `Configuration::default` instance is returned
    fn parse_config() -> Option<Self> {
        if let Ok(content) = read_to_string(DEFAULT_CONFIG_PATH) {
            let config: Self = toml::from_str(content.as_str()).unwrap();
            return Some(config);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_configuration() {
        let config = Configuration::default();
        assert_eq!(config.wordlist, DEFAULT_WORDLIST);
        assert_eq!(config.threads, 50);
        assert_eq!(config.timeout, 7);
        assert_eq!(config.target_url, String::new());
        assert_eq!(config.extensions, Vec::<String>::new());
        assert_eq!(config.target_url, String::new());
        assert_eq!(config.verbosity, 0);
    }
}