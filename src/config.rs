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
use std::process::exit;

lazy_static! {
    /// Global configuration state
    pub static ref CONFIGURATION: Configuration = Configuration::new();

    /// Global progress bar that houses other progress bars
    pub static ref PROGRESS_BAR: MultiProgress = MultiProgress::with_draw_target(ProgressDrawTarget::stdout());

    /// Global progress bar that is only used for printing messages that don't jack up other bars
    pub static ref PROGRESS_PRINTER: ProgressBar = progress::add_bar("", 0, true);
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

    /// The target URL
    #[serde(default)]
    pub target_url: String,

    /// Status Codes of interest (default: 200 204 301 302 307 308 401 403 405)
    #[serde(default = "statuscodes")]
    pub statuscodes: Vec<u16>,

    /// Instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
    #[serde(skip)]
    pub client: Client,

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
    #[serde(default = "useragent")]
    pub useragent: String,

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
    pub norecursion: bool,

    /// Append / to each request
    #[serde(default)]
    pub addslash: bool,

    /// Read url(s) from STDIN
    #[serde(default)]
    pub stdin: bool,

    /// Maximum recursion depth, a depth of 0 is infinite recursion
    #[serde(default = "depth")]
    pub depth: usize,

    /// Filter out messages of a particular size
    #[serde(default)]
    pub sizefilters: Vec<u64>,

    /// Don't auto-filter wildcard responses
    #[serde(default)]
    pub dontfilter: bool,
}

// functions timeout, threads, statuscodes, useragent, wordlist, and depth are used to provide
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
fn statuscodes() -> Vec<u16> {
    DEFAULT_STATUS_CODES
        .iter()
        .map(|code| code.as_u16())
        .collect()
}

/// default wordlist
fn wordlist() -> String {
    String::from(DEFAULT_WORDLIST)
}

/// default useragent
fn useragent() -> String {
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
        let useragent = useragent();
        let client = client::initialize(timeout, &useragent, false, false, &HashMap::new(), None);

        Configuration {
            client,
            timeout,
            useragent,
            dontfilter: false,
            quiet: false,
            stdin: false,
            verbosity: 0,
            addslash: false,
            insecure: false,
            norecursion: false,
            redirects: false,
            proxy: String::new(),
            config: String::new(),
            output: String::new(),
            target_url: String::new(),
            queries: Vec::new(),
            extensions: Vec::new(),
            sizefilters: Vec::new(),
            headers: HashMap::new(),
            threads: threads(),
            depth: depth(),
            wordlist: wordlist(),
            statuscodes: statuscodes(),
        }
    }
}

impl Configuration {
    /// Creates a [Configuration](struct.Configuration.html) object with the following
    /// built-in default values
    ///
    /// - **timeout**: `5` seconds
    /// - **redirects**: `false`
    /// - **wordlist**: [`DEFAULT_WORDLIST`](constant.DEFAULT_WORDLIST.html)
    /// - **config**: `None`
    /// - **threads**: `50`
    /// - **timeout**: `7` seconds
    /// - **verbosity**: `0` (no logging enabled)
    /// - **proxy**: `None`
    /// - **statuscodes**: [`DEFAULT_RESPONSE_CODES`](constant.DEFAULT_RESPONSE_CODES.html)
    /// - **output**: `None` (print to stdout)
    /// - **quiet**: `false`
    /// - **useragent**: `feroxer/VERSION`
    /// - **insecure**: `false` (don't be insecure, i.e. don't allow invalid certs)
    /// - **extensions**: `None`
    /// - **sizefilters**: `None`
    /// - **headers**: `None`
    /// - **queries**: `None`
    /// - **norecursion**: `false` (recursively scan enumerated sub-directories)
    /// - **addslash**: `false`
    /// - **stdin**: `false`
    /// - **dontfilter**: `false` (auto filter wildcard responses)
    /// - **depth**: `4` (maximum recursion depth)
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

        if args.values_of("extensions").is_some() {
            config.extensions = args
                .values_of("extensions")
                .unwrap()
                .map(|val| val.to_string())
                .collect();
        }

        if args.values_of("sizefilters").is_some() {
            config.sizefilters = args
                .values_of("sizefilters")
                .unwrap() // already known good
                .map(|size| {
                    size.parse::<u64>().unwrap_or_else(|e| {
                        eprintln!("[!] Error encountered: {}", e);
                        exit(1)
                    })
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

        if args.is_present("dontfilter") {
            config.dontfilter = args.is_present("dontfilter");
        }

        if args.occurrences_of("verbosity") > 0 {
            // occurrences_of returns 0 if none are found; this is protected in
            // an if block for the same reason as the quiet option
            config.verbosity = args.occurrences_of("verbosity") as u8;
        }

        if args.is_present("norecursion") {
            config.norecursion = args.is_present("norecursion");
        }

        if args.is_present("addslash") {
            config.addslash = args.is_present("addslash");
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

        if args.value_of("useragent").is_some() {
            config.useragent = String::from(args.value_of("useragent").unwrap());
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
            || config.useragent != useragent()
            || config.redirects
            || config.insecure
            || !config.headers.is_empty()
        {
            if config.proxy.is_empty() {
                config.client = client::initialize(
                    config.timeout,
                    &config.useragent,
                    config.redirects,
                    config.insecure,
                    &config.headers,
                    None,
                )
            } else {
                config.client = client::initialize(
                    config.timeout,
                    &config.useragent,
                    config.redirects,
                    config.insecure,
                    &config.headers,
                    Some(&config.proxy),
                )
            }
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
        settings.statuscodes = settings_to_merge.statuscodes;
        settings.proxy = settings_to_merge.proxy;
        settings.timeout = settings_to_merge.timeout;
        settings.verbosity = settings_to_merge.verbosity;
        settings.quiet = settings_to_merge.quiet;
        settings.output = settings_to_merge.output;
        settings.useragent = settings_to_merge.useragent;
        settings.redirects = settings_to_merge.redirects;
        settings.insecure = settings_to_merge.insecure;
        settings.extensions = settings_to_merge.extensions;
        settings.headers = settings_to_merge.headers;
        settings.queries = settings_to_merge.queries;
        settings.norecursion = settings_to_merge.norecursion;
        settings.addslash = settings_to_merge.addslash;
        settings.stdin = settings_to_merge.stdin;
        settings.depth = settings_to_merge.depth;
        settings.sizefilters = settings_to_merge.sizefilters;
        settings.dontfilter = settings_to_merge.dontfilter;
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
            statuscodes = [201, 301, 401]
            threads = 40
            timeout = 5
            proxy = "http://127.0.0.1:8080"
            quiet = true
            verbosity = 1
            output = "/some/otherpath"
            redirects = true
            insecure = true
            extensions = ["html", "php", "js"]
            headers = {stuff = "things", mostuff = "mothings"}
            queries = [["name","value"], ["rick", "astley"]]
            norecursion = true
            addslash = true
            stdin = true
            dontfilter = true
            depth = 1
            sizefilters = [4120]
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
        assert_eq!(config.statuscodes, statuscodes());
        assert_eq!(config.threads, threads());
        assert_eq!(config.depth, depth());
        assert_eq!(config.timeout, timeout());
        assert_eq!(config.verbosity, 0);
        assert_eq!(config.quiet, false);
        assert_eq!(config.dontfilter, false);
        assert_eq!(config.norecursion, false);
        assert_eq!(config.stdin, false);
        assert_eq!(config.addslash, false);
        assert_eq!(config.redirects, false);
        assert_eq!(config.insecure, false);
        assert_eq!(config.queries, Vec::new());
        assert_eq!(config.extensions, Vec::<String>::new());
        assert_eq!(config.sizefilters, Vec::<u64>::new());
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
    fn config_reads_statuscodes() {
        let config = setup_config_test();
        assert_eq!(config.statuscodes, vec![201, 301, 401]);
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
    fn config_reads_norecursion() {
        let config = setup_config_test();
        assert_eq!(config.norecursion, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_stdin() {
        let config = setup_config_test();
        assert_eq!(config.stdin, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_dontfilter() {
        let config = setup_config_test();
        assert_eq!(config.dontfilter, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_addslash() {
        let config = setup_config_test();
        assert_eq!(config.addslash, true);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_extensions() {
        let config = setup_config_test();
        assert_eq!(config.extensions, vec!["html", "php", "js"]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_sizefilters() {
        let config = setup_config_test();
        assert_eq!(config.sizefilters, vec![4120]);
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
}
