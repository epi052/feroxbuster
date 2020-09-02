use crate::{client, parser};
use clap::value_t;
use lazy_static::lazy_static;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::fs::read_to_string;
use std::path::Path;
use std::process::exit;

/// Version pulled from Cargo.toml at compile time
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub output: String,
    #[serde(default = "useragent")]
    pub useragent: String,

}

// functions timeout, threads, extensions, useragent, and wordlist are used to provide defaults in the
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
fn useragent() -> String {
    format!("feroxbuster/{}", VERSION)
}

impl Default for Configuration {
    fn default() -> Self {
        let timeout = timeout();
        let useragent = useragent();
        let client = client::initialize(timeout, &useragent, None);

        Configuration {
            client,
            timeout,
            useragent,
            quiet: false,
            verbosity: 0,
            proxy: String::new(),
            output: String::new(),
            target_url: String::new(),
            threads: threads(),
            wordlist: wordlist(),
            statuscodes: statuscodes(),
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
    /// - output: None (print to stdout)
    /// - quiet: false
    /// - useragent: "feroxbuster/VERSION"
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
            config.quiet = settings.quiet;
            config.output = settings.output;
            config.useragent = settings.useragent
        }

        let args = parser::initialize().get_matches();

        // the .is_some appears clunky, but it allows default values to be incrementally
        // overwritten from Struct defaults, to file config, to command line args, soooo ¯\_(ツ)_/¯
        if args.value_of("threads").is_some() {
            let threads = value_t!(args.value_of("threads"), usize).unwrap_or_else(|e| e.exit());
            config.threads = threads;
        }

        if args.value_of("wordlist").is_some() {
            config.wordlist = String::from(args.value_of("wordlist").unwrap());
        }

        if args.value_of("output").is_some() {
            config.output = String::from(args.value_of("output").unwrap());
        }

        if args.values_of("statuscodes").is_some() {
            config.statuscodes = args
                .values_of("statuscodes")
                .unwrap() // already known good
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| {
                            eprintln!("[!] Error encountered: {}", e);
                            exit(1)
                        })
                        .as_u16()
                })
                .collect();
        }

        if args.is_present("quiet") {
            // the reason this is protected by an if statement:
            // consider a user specifying quiet = true in feroxbuster.toml
            // if the line below is outside of the if, we'd overwrite true with
            // false if no -q is used on the command line
            config.quiet = args.is_present("quiet");
        }

        if args.occurrences_of("verbosity") > 0 {
            // occurrences_of returns 0 if none are found; this is protected in
            // an if block for the same reason as the quiet option
            config.verbosity = args.occurrences_of("verbosity") as u8;
        }

        // target_url is required, so no if statement is required
        config.target_url = String::from(args.value_of("url").unwrap());

        ////
        // organizational breakpoint; all options below alter the Client configuration
        ////
        if args.value_of("proxy").is_some() {
            config.proxy = String::from(args.value_of("proxy").unwrap());
        }

        if args.value_of("useragent").is_some() {
            config.useragent = String::from(args.value_of("useragent").unwrap());
        }

        if args.value_of("timeout").is_some() {
            let timeout = value_t!(args.value_of("timeout"), u64).unwrap_or_else(|e| e.exit());
            config.timeout = timeout;
        }

        // this if statement determines if we've gotten a Client configuration change from
        // either the config file or command line arguments; if we have, we need to rebuild
        // the client and store it in the config struct
        if !config.proxy.is_empty() || config.timeout != timeout() || config.useragent != useragent() {
            if config.proxy.is_empty() {
                config.client = client::initialize(config.timeout, &config.useragent, None)
            }
            else {
                config.client = client::initialize(config.timeout, &config.useragent, Some(&config.proxy))
            }
        }

        println!("{:#?}", config); // todo: remove eventually or turn into banner
        config
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
            quiet = true
            verbosity = 1
            output = "/some/otherpath"
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
        assert_eq!(config.quiet, false);
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

    #[test]
    fn config_reads_quiet() {
        let config = setup_config_test();
        assert_eq!(config.quiet, true);
    }

    #[test]
    fn config_reads_verbosity() {
        let config = setup_config_test();
        assert_eq!(config.verbosity, 1);
    }

    #[test]
    fn config_reads_output() {
        let config = setup_config_test();
        assert_eq!(config.output, "/some/otherpath");
    }
}
