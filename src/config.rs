use crate::utils::{module_colorizer, status_colorizer};
use crate::{client, parser, progress};
use crate::{DEFAULT_CONFIG_NAME, DEFAULT_STATUS_CODES, DEFAULT_WORDLIST, VERSION};
use clap::value_t;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget};
use lazy_static::lazy_static;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::collections::HashMap;
use std::env::{current_dir, current_exe};
use std::fs::read_to_string;
use std::path::PathBuf;
#[cfg(not(test))]
use std::process::exit;

lazy_static! {
    /// Global configuration state
    pub static ref CONFIGURATION: Configuration = Configuration::new();

    /// Global progress bar that houses other progress bars
    pub static ref PROGRESS_BAR: MultiProgress = MultiProgress::with_draw_target(ProgressDrawTarget::stdout());

    /// Global progress bar that is only used for printing messages that don't jack up other bars
    pub static ref PROGRESS_PRINTER: ProgressBar = progress::add_bar("", 0, true);
}

/// simple helper to clean up some code reuse below; panics under test / exits in prod
fn report_and_exit(err: &str) -> ! {
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

/// Represents the final, global configuration of the program.
///
/// This struct is the combination of the following:
/// - default configuration values
/// - plus overrides read from a configuration file
/// - plus command-line options
///
/// In that order.
///
/// Inspired by and derived from https://github.com/PhilipDaniels/rust-config-example
#[derive(Debug, Clone, Deserialize)]
pub struct Configuration {
    /// Path to the wordlist
    #[serde(default = "wordlist")]
    pub wordlist: String,

    /// Path to the config file used
    #[serde(default)]
    pub config: String,

    /// Proxy to use for requests (ex: http(s)://host:port, socks5://host:port)
    #[serde(default)]
    pub proxy: String,

    /// Replay Proxy to use for requests (ex: http(s)://host:port, socks5://host:port)
    #[serde(default)]
    pub replay_proxy: String,

    /// The target URL
    #[serde(default)]
    pub target_url: String,

    /// Status Codes to include (allow list) (default: 200 204 301 302 307 308 401 403 405)
    #[serde(default = "status_codes")]
    pub status_codes: Vec<u16>,

    /// Status Codes to replay to the Replay Proxy (default: whatever is passed to --status-code)
    #[serde(default)]
    pub replay_codes: Vec<u16>,

    /// Status Codes to filter out (deny list)
    #[serde(default)]
    pub filter_status: Vec<u16>,

    /// Instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
    #[serde(skip)]
    pub client: Client,

    /// Instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
    #[serde(skip)]
    pub replay_client: Option<Client>,

    /// Number of concurrent threads (default: 50)
    #[serde(default = "threads")]
    pub threads: usize,

    /// Number of seconds before a request times out (default: 7)
    #[serde(default = "timeout")]
    pub timeout: u64,

    /// Level of verbosity, equates to log level
    #[serde(default)]
    pub verbosity: u8,

    /// Only print URLs
    #[serde(default)]
    pub quiet: bool,

    /// Output file to write results to (default: stdout)
    #[serde(default)]
    pub output: String,

    /// Sets the User-Agent (default: feroxbuster/VERSION)
    #[serde(default = "user_agent")]
    pub user_agent: String,

    /// Follow redirects
    #[serde(default)]
    pub redirects: bool,

    /// Disables TLS certificate validation
    #[serde(default)]
    pub insecure: bool,

    /// File extension(s) to search for
    #[serde(default)]
    pub extensions: Vec<String>,

    /// HTTP headers to be used in each request
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// URL query parameters
    #[serde(default)]
    pub queries: Vec<(String, String)>,

    /// Do not scan recursively
    #[serde(default)]
    pub no_recursion: bool,

    /// Extract links from html/javscript
    #[serde(default)]
    pub extract_links: bool,

    /// Append / to each request
    #[serde(default)]
    pub add_slash: bool,

    /// Read url(s) from STDIN
    #[serde(default)]
    pub stdin: bool,

    /// Maximum recursion depth, a depth of 0 is infinite recursion
    #[serde(default = "depth")]
    pub depth: usize,

    /// Number of concurrent scans permitted; a limit of 0 means no limit is imposed
    #[serde(default)]
    pub scan_limit: usize,

    /// Filter out messages of a particular size
    #[serde(default)]
    pub filter_size: Vec<u64>,

    /// Don't auto-filter wildcard responses
    #[serde(default)]
    pub dont_filter: bool,
}

// functions timeout, threads, status_codes, user_agent, wordlist, and depth are used to provide
// defaults in the event that a ferox-config.toml is found but one or more of the values below
// aren't listed in the config.  This way, we get the correct defaults upon Deserialization

/// default timeout value
fn timeout() -> u64 {
    7
}

/// default threads value
fn threads() -> usize {
    50
}

/// default status codes
fn status_codes() -> Vec<u16> {
    DEFAULT_STATUS_CODES
        .iter()
        .map(|code| code.as_u16())
        .collect()
}

/// default wordlist
fn wordlist() -> String {
    String::from(DEFAULT_WORDLIST)
}

/// default user-agent
fn user_agent() -> String {
    format!("feroxbuster/{}", VERSION)
}

/// default recursion depth
fn depth() -> usize {
    4
}

impl Default for Configuration {
    /// Builds the default Configuration for feroxbuster
    fn default() -> Self {
        let timeout = timeout();
        let user_agent = user_agent();
        let client = client::initialize(timeout, &user_agent, false, false, &HashMap::new(), None);
        let replay_client = None;
        let status_codes = status_codes();
        let replay_codes = status_codes.clone();

        Configuration {
            client,
            timeout,
            user_agent,
            replay_codes,
            status_codes,
            replay_client,
            dont_filter: false,
            quiet: false,
            stdin: false,
            verbosity: 0,
            scan_limit: 0,
            add_slash: false,
            insecure: false,
            redirects: false,
            no_recursion: false,
            extract_links: false,
            proxy: String::new(),
            config: String::new(),
            output: String::new(),
            target_url: String::new(),
            replay_proxy: String::new(),
            queries: Vec::new(),
            extensions: Vec::new(),
            filter_size: Vec::new(),
            filter_status: Vec::new(),
            headers: HashMap::new(),
            depth: depth(),
            threads: threads(),
            wordlist: wordlist(),
        }
    }
}

impl Configuration {
    /// Creates a [Configuration](struct.Configuration.html) object with the following
    /// built-in default values
    ///
    /// - **timeout**: `5` seconds
    /// - **redirects**: `false`
    /// - **extract-links**: `false`
    /// - **wordlist**: [`DEFAULT_WORDLIST`](constant.DEFAULT_WORDLIST.html)
    /// - **config**: `None`
    /// - **threads**: `50`
    /// - **timeout**: `7` seconds
    /// - **verbosity**: `0` (no logging enabled)
    /// - **proxy**: `None`
    /// - **status_codes**: [`DEFAULT_RESPONSE_CODES`](constant.DEFAULT_RESPONSE_CODES.html)
    /// - **filter_status**: `None`
    /// - **output**: `None` (print to stdout)
    /// - **quiet**: `false`
    /// - **user_agent**: `feroxer/VERSION`
    /// - **insecure**: `false` (don't be insecure, i.e. don't allow invalid certs)
    /// - **extensions**: `None`
    /// - **filter_size**: `None`
    /// - **headers**: `None`
    /// - **queries**: `None`
    /// - **no_recursion**: `false` (recursively scan enumerated sub-directories)
    /// - **add_slash**: `false`
    /// - **stdin**: `false`
    /// - **dont_filter**: `false` (auto filter wildcard responses)
    /// - **depth**: `4` (maximum recursion depth)
    /// - **scan_limit**: `0` (no limit on concurrent scans imposed)
    /// - **replay_proxy**: `None` (no limit on concurrent scans imposed)
    /// - **replay_codes**: [`DEFAULT_RESPONSE_CODES`](constant.DEFAULT_RESPONSE_CODES.html)
    ///
    /// After which, any values defined in a
    /// [ferox-config.toml](constant.DEFAULT_CONFIG_NAME.html) config file will override the
    /// built-in defaults.
    ///
    /// `ferox-config.toml` can be placed in any of the following locations (in the order shown):
    /// - `/etc/feroxbuster/`
    /// - `CONFIG_DIR/ferxobuster/`
    /// - The same directory as the `feroxbuster` executable
    /// - The user's current working directory
    ///
    /// If more than one valid configuration file is found, each one overwrites the values found previously.
    ///
    /// Finally, any options/arguments given on the commandline will override both built-in and
    /// config-file specified values.
    ///
    /// The resulting [Configuration](struct.Configuration.html) is a singleton with a `static`
    /// lifetime.
    pub fn new() -> Self {
        // when compiling for test, we want to eliminate the runtime dependency of the parser
        if cfg!(test) {
            return Configuration::default();
        }

        // Get the default configuration, this is what will apply if nothing
        // else is specified.
        let mut config = Configuration::default();

        // Next, we parse the ferox-config.toml file, if present and set the values
        // therein to overwrite our default values. Deserialized defaults are specified
        // in the Configuration struct so that we don't change anything that isn't
        // actually specified in the config file
        //
        // search for a config using the following order of precedence
        //   - /etc/feroxbuster/
        //   - CONFIG_DIR/ferxobuster/
        //   - same directory as feroxbuster executable
        //   - current directory

        // merge a config found at /etc/feroxbuster/ferox-config.toml
        let config_file = PathBuf::new()
            .join("/etc/feroxbuster")
            .join(DEFAULT_CONFIG_NAME);
        Self::parse_and_merge_config(config_file, &mut config);

        // merge a config found at ~/.config/feroxbuster/ferox-config.toml
        if let Some(config_dir) = dirs::config_dir() {
            // config_dir() resolves to one of the following
            //   - linux: $XDG_CONFIG_HOME or $HOME/.config
            //   - macOS: $HOME/Library/Application Support
            //   - windows: {FOLDERID_RoamingAppData}

            let config_file = config_dir.join("feroxbuster").join(DEFAULT_CONFIG_NAME);
            Self::parse_and_merge_config(config_file, &mut config);
        };

        // merge a config found in same the directory as feroxbuster executable
        if let Ok(exe_path) = current_exe() {
            if let Some(bin_dir) = exe_path.parent() {
                let config_file = bin_dir.join(DEFAULT_CONFIG_NAME);
                Self::parse_and_merge_config(config_file, &mut config);
            };
        };

        // merge a config found in the user's current working directory
        if let Ok(cwd) = current_dir() {
            let config_file = cwd.join(DEFAULT_CONFIG_NAME);
            Self::parse_and_merge_config(config_file, &mut config);
        }

        let args = parser::initialize().get_matches();

        // the .is_some appears clunky, but it allows default values to be incrementally
        // overwritten from Struct defaults, to file config, to command line args, soooo ¯\_(ツ)_/¯
        if args.value_of("threads").is_some() {
            let threads = value_t!(args.value_of("threads"), usize).unwrap_or_else(|e| e.exit());
            config.threads = threads;
        }

        if args.value_of("depth").is_some() {
            let depth = value_t!(args.value_of("depth"), usize).unwrap_or_else(|e| e.exit());
            config.depth = depth;
        }

        if args.value_of("scan_limit").is_some() {
            let scan_limit =
                value_t!(args.value_of("scan_limit"), usize).unwrap_or_else(|e| e.exit());
            config.scan_limit = scan_limit;
        }

        if args.value_of("wordlist").is_some() {
            config.wordlist = String::from(args.value_of("wordlist").unwrap());
        }

        if args.value_of("output").is_some() {
            config.output = String::from(args.value_of("output").unwrap());
        }

        if args.values_of("status_codes").is_some() {
            config.status_codes = args
                .values_of("status_codes")
                .unwrap() // already known good
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_u16()
                })
                .collect();
        }

        if args.values_of("replay_codes").is_some() {
            // replay codes passed in by the user
            config.replay_codes = args
                .values_of("replay_codes")
                .unwrap() // already known good
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_u16()
                })
                .collect();
        } else {
            // not passed in by the user, use whatever value is held in status_codes
            config.replay_codes = config.status_codes.clone();
        }

        if args.values_of("filter_status").is_some() {
            config.filter_status = args
                .values_of("filter_status")
                .unwrap() // already known good
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_u16()
                })
                .collect();
        }

        if args.values_of("extensions").is_some() {
            config.extensions = args
                .values_of("extensions")
                .unwrap()
                .map(|val| val.to_string())
                .collect();
        }

        if args.values_of("filter_size").is_some() {
            config.filter_size = args
                .values_of("filter_size")
                .unwrap() // already known good
                .map(|size| {
                    size.parse::<u64>()
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                })
                .collect();
        }

        if args.is_present("quiet") {
            // the reason this is protected by an if statement:
            // consider a user specifying quiet = true in ferox-config.toml
            // if the line below is outside of the if, we'd overwrite true with
            // false if no -q is used on the command line
            config.quiet = args.is_present("quiet");
        }

        if args.is_present("dont_filter") {
            config.dont_filter = args.is_present("dont_filter");
        }

        if args.occurrences_of("verbosity") > 0 {
            // occurrences_of returns 0 if none are found; this is protected in
            // an if block for the same reason as the quiet option
            config.verbosity = args.occurrences_of("verbosity") as u8;
        }

        if args.is_present("no_recursion") {
            config.no_recursion = args.is_present("no_recursion");
        }

        if args.is_present("add_slash") {
            config.add_slash = args.is_present("add_slash");
        }

        if args.is_present("extract_links") {
            config.extract_links = args.is_present("extract_links");
        }

        if args.is_present("stdin") {
            config.stdin = args.is_present("stdin");
        } else {
            config.target_url = String::from(args.value_of("url").unwrap());
        }

        ////
        // organizational breakpoint; all options below alter the Client configuration
        ////
        if args.value_of("proxy").is_some() {
            config.proxy = String::from(args.value_of("proxy").unwrap());
        }

        if args.value_of("replay_proxy").is_some() {
            config.replay_proxy = String::from(args.value_of("replay_proxy").unwrap());
        }

        if args.value_of("user_agent").is_some() {
            config.user_agent = String::from(args.value_of("user_agent").unwrap());
        }

        if args.value_of("timeout").is_some() {
            let timeout = value_t!(args.value_of("timeout"), u64).unwrap_or_else(|e| e.exit());
            config.timeout = timeout;
        }

        if args.is_present("redirects") {
            config.redirects = args.is_present("redirects");
        }

        if args.is_present("insecure") {
            config.insecure = args.is_present("insecure");
        }

        if args.values_of("headers").is_some() {
            for val in args.values_of("headers").unwrap() {
                let mut split_val = val.split(':');

                // explicitly take first split value as header's name
                let name = split_val.next().unwrap().trim();

                // all other items in the iterator returned by split, when combined with the
                // original split deliminator (:), make up the header's final value
                let value = split_val.collect::<Vec<&str>>().join(":");
                config.headers.insert(name.to_string(), value.to_string());
            }
        }

        if args.values_of("queries").is_some() {
            for val in args.values_of("queries").unwrap() {
                // same basic logic used as reading in the headers HashMap above
                let mut split_val = val.split('=');

                let name = split_val.next().unwrap().trim();

                let value = split_val.collect::<Vec<&str>>().join("=");

                config.queries.push((name.to_string(), value.to_string()));
            }
        }

        // this if statement determines if we've gotten a Client configuration change from
        // either the config file or command line arguments; if we have, we need to rebuild
        // the client and store it in the config struct
        if !config.proxy.is_empty()
            || config.timeout != timeout()
            || config.user_agent != user_agent()
            || config.redirects
            || config.insecure
            || !config.headers.is_empty()
        {
            if config.proxy.is_empty() {
                config.client = client::initialize(
                    config.timeout,
                    &config.user_agent,
                    config.redirects,
                    config.insecure,
                    &config.headers,
                    None,
                )
            } else {
                config.client = client::initialize(
                    config.timeout,
                    &config.user_agent,
                    config.redirects,
                    config.insecure,
                    &config.headers,
                    Some(&config.proxy),
                )
            }
        }

        if !config.replay_proxy.is_empty() {
            // only set replay_client when replay_proxy is set
            config.replay_client = Some(client::initialize(
                config.timeout,
                &config.user_agent,
                config.redirects,
                config.insecure,
                &config.headers,
                Some(&config.replay_proxy),
            ));
        }

        config
    }

    /// Given a configuration file's location and an instance of `Configuration`, read in
    /// the config file if found and update the current settings with the settings found therein
    fn parse_and_merge_config(config_file: PathBuf, mut config: &mut Self) {
        if config_file.exists() {
            // save off a string version of the path before it goes out of scope
            let conf_str = match config_file.to_str() {
                Some(cs) => String::from(cs),
                None => String::new(),
            };

            if let Some(settings) = Self::parse_config(config_file) {
                // set the config used for viewing in the banner
                config.config = conf_str;

                // update the settings
                Self::merge_config(&mut config, settings);
            }
        }
    }

    /// Given two Configurations, overwrite `settings` with the fields found in `settings_to_merge`
    fn merge_config(settings: &mut Self, settings_to_merge: Self) {
        settings.threads = settings_to_merge.threads;
        settings.wordlist = settings_to_merge.wordlist;
        settings.status_codes = settings_to_merge.status_codes;
        settings.proxy = settings_to_merge.proxy;
        settings.timeout = settings_to_merge.timeout;
        settings.verbosity = settings_to_merge.verbosity;
        settings.quiet = settings_to_merge.quiet;
        settings.output = settings_to_merge.output;
        settings.user_agent = settings_to_merge.user_agent;
        settings.redirects = settings_to_merge.redirects;
        settings.insecure = settings_to_merge.insecure;
        settings.extract_links = settings_to_merge.extract_links;
        settings.extensions = settings_to_merge.extensions;
        settings.headers = settings_to_merge.headers;
        settings.queries = settings_to_merge.queries;
        settings.no_recursion = settings_to_merge.no_recursion;
        settings.add_slash = settings_to_merge.add_slash;
        settings.stdin = settings_to_merge.stdin;
        settings.depth = settings_to_merge.depth;
        settings.filter_size = settings_to_merge.filter_size;
        settings.filter_status = settings_to_merge.filter_status;
        settings.dont_filter = settings_to_merge.dont_filter;
        settings.scan_limit = settings_to_merge.scan_limit;
        settings.replay_proxy = settings_to_merge.replay_proxy;
        settings.replay_codes = settings_to_merge.replay_codes;
    }

    /// If present, read in `DEFAULT_CONFIG_NAME` and deserialize the specified values
    ///
    /// uses serde to deserialize the toml into a `Configuration` struct
    fn parse_config(config_file: PathBuf) -> Option<Self> {
        if let Ok(content) = read_to_string(config_file) {
            match toml::from_str(content.as_str()) {
                Ok(config) => {
                    return Some(config);
                }
                Err(e) => {
                    println!(
                        "{} {} {}",
                        status_colorizer("ERROR"),
                        module_colorizer("config::parse_config"),
                        e
                    );
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::TempDir;

    /// creates a dummy configuration file for testing
    fn setup_config_test() -> Configuration {
        let data = r#"
            wordlist = "/some/path"
            status_codes = [201, 301, 401]
            replay_codes = [201, 301]
            threads = 40
            timeout = 5
            proxy = "http://127.0.0.1:8080"
            replay_proxy = "http://127.0.0.1:8081"
            quiet = true
            verbosity = 1
            scan_limit = 6
            output = "/some/otherpath"
            redirects = true
            insecure = true
            extensions = ["html", "php", "js"]
            headers = {stuff = "things", mostuff = "mothings"}
            queries = [["name","value"], ["rick", "astley"]]
            no_recursion = true
            add_slash = true
            stdin = true
            dont_filter = true
            extract_links = true
            depth = 1
            filter_size = [4120]
            filter_status = [201]
        "#;
        let tmp_dir = TempDir::new().unwrap();
        let file = tmp_dir.path().join(DEFAULT_CONFIG_NAME);
        write(&file, data).unwrap();
        Configuration::parse_config(file).unwrap()
    }

    #[test]
    /// test that all default config values meet expectations
    fn default_configuration() {
        let config = Configuration::default();
        assert_eq!(config.wordlist, wordlist());
        assert_eq!(config.proxy, String::new());
        assert_eq!(config.target_url, String::new());
        assert_eq!(config.config, String::new());
        assert_eq!(config.replay_proxy, String::new());
        assert_eq!(config.status_codes, status_codes());
        assert_eq!(config.replay_codes, config.status_codes);
        assert!(config.replay_client.is_none());
        assert_eq!(config.threads, threads());
        assert_eq!(config.depth, depth());
        assert_eq!(config.timeout, timeout());
        assert_eq!(config.verbosity, 0);
        assert_eq!(config.scan_limit, 0);
        assert_eq!(config.quiet, false);
        assert_eq!(config.dont_filter, false);
        assert_eq!(config.no_recursion, false);
        assert_eq!(config.stdin, false);
        assert_eq!(config.add_slash, false);
        assert_eq!(config.redirects, false);
        assert_eq!(config.extract_links, false);
        assert_eq!(config.insecure, false);
        assert_eq!(config.queries, Vec::new());
        assert_eq!(config.extensions, Vec::<String>::new());
        assert_eq!(config.filter_size, Vec::<u64>::new());
        assert_eq!(config.filter_status, Vec::<u16>::new());
        assert_eq!(config.headers, HashMap::new());
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_wordlist() {
        let config = setup_config_test();
        assert_eq!(config.wordlist, "/some/path");
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_status_codes() {
        let config = setup_config_test();
        assert_eq!(config.status_codes, vec![201, 301, 401]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_replay_codes() {
        let config = setup_config_test();
        assert_eq!(config.replay_codes, vec![201, 301]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_threads() {
        let config = setup_config_test();
        assert_eq!(config.threads, 40);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_depth() {
        let config = setup_config_test();
        assert_eq!(config.depth, 1);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_scan_limit() {
        let config = setup_config_test();
        assert_eq!(config.scan_limit, 6);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_timeout() {
        let config = setup_config_test();
        assert_eq!(config.timeout, 5);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_proxy() {
        let config = setup_config_test();
        assert_eq!(config.proxy, "http://127.0.0.1:8080");
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_replay_proxy() {
        let config = setup_config_test();
        assert_eq!(config.replay_proxy, "http://127.0.0.1:8081");
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_quiet() {
        let config = setup_config_test();
        assert_eq!(config.quiet, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_verbosity() {
        let config = setup_config_test();
        assert_eq!(config.verbosity, 1);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_output() {
        let config = setup_config_test();
        assert_eq!(config.output, "/some/otherpath");
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_redirects() {
        let config = setup_config_test();
        assert_eq!(config.redirects, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_insecure() {
        let config = setup_config_test();
        assert_eq!(config.insecure, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_no_recursion() {
        let config = setup_config_test();
        assert_eq!(config.no_recursion, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_stdin() {
        let config = setup_config_test();
        assert_eq!(config.stdin, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_dont_filter() {
        let config = setup_config_test();
        assert_eq!(config.dont_filter, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_add_slash() {
        let config = setup_config_test();
        assert_eq!(config.add_slash, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_extract_links() {
        let config = setup_config_test();
        assert_eq!(config.extract_links, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_extensions() {
        let config = setup_config_test();
        assert_eq!(config.extensions, vec!["html", "php", "js"]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_filter_size() {
        let config = setup_config_test();
        assert_eq!(config.filter_size, vec![4120]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_filter_status() {
        let config = setup_config_test();
        assert_eq!(config.filter_status, vec![201]);
    }

    #[test]
    /// parse the test config and see that the values parsed are correct
    fn config_reads_headers() {
        let config = setup_config_test();
        let mut headers = HashMap::new();
        headers.insert("stuff".to_string(), "things".to_string());
        headers.insert("mostuff".to_string(), "mothings".to_string());
        assert_eq!(config.headers, headers);
    }

    #[test]
    /// parse the test config and see that the values parsed are correct
    fn config_reads_queries() {
        let config = setup_config_test();
        let mut queries = vec![];
        queries.push(("name".to_string(), "value".to_string()));
        queries.push(("rick".to_string(), "astley".to_string()));
        assert_eq!(config.queries, queries);
    }

    #[test]
    #[should_panic]
    /// test that an error message is printed and panic is called when report_and_exit is called
    fn config_report_and_exit_works() {
        report_and_exit("some message");
    }
}
