use crate::{
    client, parser,
    progress::{add_bar, BarType},
    scan_manager::resume_scan,
    utils::{module_colorizer, status_colorizer},
    FeroxSerialize, DEFAULT_CONFIG_NAME, DEFAULT_STATUS_CODES, DEFAULT_WORDLIST, VERSION,
};
use clap::{value_t, ArgMatches};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget};
use lazy_static::lazy_static;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use std::process::exit;
use std::{
    collections::HashMap,
    env::{current_dir, current_exe},
    fs::read_to_string,
    path::PathBuf,
};

lazy_static! {
    /// Global configuration state
    pub static ref CONFIGURATION: Configuration = Configuration::new();

    /// Global progress bar that houses other progress bars
    pub static ref PROGRESS_BAR: MultiProgress = MultiProgress::with_draw_target(ProgressDrawTarget::stdout());

    /// Global progress bar that is only used for printing messages that don't jack up other bars
    pub static ref PROGRESS_PRINTER: ProgressBar = add_bar("", 0, BarType::Hidden);
}

/// macro helper to abstract away repetitive configuration updates
macro_rules! update_config_if_present {
    ($c:expr, $m:ident, $v:expr, $t:ty) => {
        match value_t!($m, $v, $t) {
            Ok(value) => *$c = value, // Update value
            Err(clap::Error {
                kind: clap::ErrorKind::ArgumentNotFound,
                message: _,
                info: _,
            }) => {
                // Do nothing if argument not found
            }
            Err(e) => e.exit(), // Exit with error on parse error
        }
    };
}

/// macro helper to abstract away repetitive if not default: update checks
macro_rules! update_if_not_default {
    ($old:expr, $new:expr, $default:expr) => {
        if $new != $default {
            *$old = $new;
        }
    };
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Configuration {
    #[serde(rename = "type", default = "serialized_type")]
    /// Name of this type of struct, used for serialization, i.e. `{"type":"configuration"}`
    pub kind: String,

    /// Path to the wordlist
    #[serde(default = "wordlist")]
    pub wordlist: String,

    /// Path to the config file used
    #[serde(default)]
    pub config: String,

    /// Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)
    #[serde(default)]
    pub proxy: String,

    /// Replay Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)
    #[serde(default)]
    pub replay_proxy: String,

    /// The target URL
    #[serde(default)]
    pub target_url: String,

    /// Status Codes to include (allow list) (default: 200 204 301 302 307 308 401 403 405)
    #[serde(default = "status_codes")]
    pub status_codes: Vec<u16>,

    /// Status Codes to replay to the Replay Proxy (default: whatever is passed to --status-code)
    #[serde(default = "status_codes")]
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

    /// Store log output as NDJSON
    #[serde(default)]
    pub json: bool,

    /// Output file to write results to (default: stdout)
    #[serde(default)]
    pub output: String,

    /// File in which to store debug output, used in conjunction with verbosity to dictate which
    /// logs are written
    #[serde(default)]
    pub debug_log: String,

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

    /// Filter out messages of a particular line count
    #[serde(default)]
    pub filter_line_count: Vec<usize>,

    /// Filter out messages of a particular word count
    #[serde(default)]
    pub filter_word_count: Vec<usize>,

    /// Filter out messages by regular expression
    #[serde(default)]
    pub filter_regex: Vec<String>,

    /// Don't auto-filter wildcard responses
    #[serde(default)]
    pub dont_filter: bool,

    /// Scan started from a state file, not from CLI args
    #[serde(default)]
    pub resumed: bool,

    /// Resume scan from this file
    #[serde(default)]
    pub resume_from: String,

    /// Whether or not a scan's current state should be saved when user presses Ctrl+C
    ///
    /// Not configurable from CLI; can only be set from a config file
    #[serde(default = "save_state")]
    pub save_state: bool,

    /// The maximum runtime for a scan, expressed as N[smdh] where N can be parsed into a
    /// non-negative integer and the next character is either s, m, h, or d (case insensitive)
    #[serde(default)]
    pub time_limit: String,

    /// Filter out response bodies that meet a certain threshold of similarity
    #[serde(default)]
    pub filter_similar: Vec<String>,
}

// functions timeout, threads, status_codes, user_agent, wordlist, save_state, and depth are used to provide
// defaults in the event that a ferox-config.toml is found but one or more of the values below
// aren't listed in the config.  This way, we get the correct defaults upon Deserialization

/// default Configuration type for use in json output
fn serialized_type() -> String {
    String::from("configuration")
}

/// default timeout value
fn timeout() -> u64 {
    7
}

/// default save_state value
fn save_state() -> bool {
    true
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
        let kind = serialized_type();

        Configuration {
            kind,
            client,
            timeout,
            user_agent,
            replay_codes,
            status_codes,
            replay_client,
            dont_filter: false,
            quiet: false,
            resumed: false,
            stdin: false,
            json: false,
            verbosity: 0,
            scan_limit: 0,
            add_slash: false,
            insecure: false,
            redirects: false,
            no_recursion: false,
            extract_links: false,
            save_state: true,
            proxy: String::new(),
            config: String::new(),
            output: String::new(),
            debug_log: String::new(),
            target_url: String::new(),
            time_limit: String::new(),
            resume_from: String::new(),
            replay_proxy: String::new(),
            queries: Vec::new(),
            extensions: Vec::new(),
            filter_size: Vec::new(),
            filter_regex: Vec::new(),
            filter_line_count: Vec::new(),
            filter_word_count: Vec::new(),
            filter_status: Vec::new(),
            filter_similar: Vec::new(),
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
    /// - **debug_log**: `None`
    /// - **quiet**: `false`
    /// - **save_state**: `true`
    /// - **user_agent**: `feroxbuster/VERSION`
    /// - **insecure**: `false` (don't be insecure, i.e. don't allow invalid certs)
    /// - **extensions**: `None`
    /// - **filter_size**: `None`
    /// - **filter_similar**: `None`
    /// - **filter_regex**: `None`
    /// - **filter_word_count**: `None`
    /// - **filter_line_count**: `None`
    /// - **headers**: `None`
    /// - **queries**: `None`
    /// - **no_recursion**: `false` (recursively scan enumerated sub-directories)
    /// - **add_slash**: `false`
    /// - **stdin**: `false`
    /// - **json**: `false`
    /// - **dont_filter**: `false` (auto filter wildcard responses)
    /// - **depth**: `4` (maximum recursion depth)
    /// - **scan_limit**: `0` (no limit on concurrent scans imposed)
    /// - **time_limit**: `None` (no limit on length of scan imposed)
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
            let test_config = Configuration {
                save_state: false, // don't clutter up junk when testing
                ..Default::default()
            };
            return test_config;
        }

        let args = parser::initialize().get_matches();

        // Get the default configuration, this is what will apply if nothing
        // else is specified.
        let mut config = Configuration::default();

        // read in all config files
        Self::parse_config_files(&mut config);

        // read in the user provided options, this produces a separate instance of Configuration
        // in order to allow for potentially merging into a --resume-from Configuration
        let cli_config = Self::parse_cli_args(&args);

        // --resume-from used, need to first read the Configuration from disk, and then
        // merge the cli_config into the resumed config
        if let Some(filename) = args.value_of("resume_from") {
            // when resuming a scan, instead of normal configuration loading, we just
            // load the config from disk by calling resume_scan
            let mut previous_config = resume_scan(filename);

            // if any other arguments were passed on the command line, the theory is that the
            // user meant to modify the previously cancelled/saved scan in some way that we
            // should take into account
            Self::merge_config(&mut previous_config, cli_config);

            // the resumed flag isn't printed in the banner and really has no business being
            // serialized or included in much of the usual config logic; simply setting it to true
            // here and being done with it
            previous_config.resumed = true;

            // if the user used --stdin, we already have all the scans started (or complete), we
            // need to flip stdin to false so that the 'read from stdin' logic doesn't fire (if
            // not flipped to false, the program hangs waiting for input from stdin again)
            previous_config.stdin = false;

            // clients aren't serialized, have to remake them from the previous config
            Self::try_rebuild_clients(&mut previous_config);

            return previous_config;
        }

        // if we've gotten to this point in the code, --resume-from was not used, so we need to
        // merge the cli options into the config file options and return the result
        Self::merge_config(&mut config, cli_config);

        // rebuild clients is the last step in either code branch
        Self::try_rebuild_clients(&mut config);

        config
    }

    /// Parse all possible versions of the ferox-config.toml file, adhering to the order of
    /// precedence outlined above
    fn parse_config_files(mut config: &mut Self) {
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
    }

    /// Given a set of ArgMatches read from the CLI, update and return the default Configuration
    /// settings
    fn parse_cli_args(args: &ArgMatches) -> Self {
        let mut config = Configuration::default();

        update_config_if_present!(&mut config.threads, args, "threads", usize);
        update_config_if_present!(&mut config.depth, args, "depth", usize);
        update_config_if_present!(&mut config.scan_limit, args, "scan_limit", usize);
        update_config_if_present!(&mut config.wordlist, args, "wordlist", String);
        update_config_if_present!(&mut config.output, args, "output", String);
        update_config_if_present!(&mut config.debug_log, args, "debug_log", String);
        update_config_if_present!(&mut config.time_limit, args, "time_limit", String);
        update_config_if_present!(&mut config.resume_from, args, "resume_from", String);

        if let Some(arg) = args.values_of("status_codes") {
            config.status_codes = arg
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_u16()
                })
                .collect();
        }

        if let Some(arg) = args.values_of("replay_codes") {
            // replay codes passed in by the user
            config.replay_codes = arg
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

        if let Some(arg) = args.values_of("filter_status") {
            config.filter_status = arg
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_u16()
                })
                .collect();
        }

        if let Some(arg) = args.values_of("extensions") {
            config.extensions = arg.map(|val| val.to_string()).collect();
        }

        if let Some(arg) = args.values_of("filter_regex") {
            config.filter_regex = arg.map(|val| val.to_string()).collect();
        }

        if let Some(arg) = args.values_of("filter_similar") {
            config.filter_similar = arg.map(|val| val.to_string()).collect();
        }

        if let Some(arg) = args.values_of("filter_size") {
            config.filter_size = arg
                .map(|size| {
                    size.parse::<u64>()
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                })
                .collect();
        }

        if let Some(arg) = args.values_of("filter_words") {
            config.filter_word_count = arg
                .map(|size| {
                    size.parse::<usize>()
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                })
                .collect();
        }

        if let Some(arg) = args.values_of("filter_lines") {
            config.filter_line_count = arg
                .map(|size| {
                    size.parse::<usize>()
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                })
                .collect();
        }

        if args.is_present("quiet") {
            // the reason this is protected by an if statement:
            // consider a user specifying quiet = true in ferox-config.toml
            // if the line below is outside of the if, we'd overwrite true with
            // false if no -q is used on the command line
            config.quiet = true;
        }

        if args.is_present("dont_filter") {
            config.dont_filter = true;
        }

        if args.occurrences_of("verbosity") > 0 {
            // occurrences_of returns 0 if none are found; this is protected in
            // an if block for the same reason as the quiet option
            config.verbosity = args.occurrences_of("verbosity") as u8;
        }

        if args.is_present("no_recursion") {
            config.no_recursion = true;
        }

        if args.is_present("add_slash") {
            config.add_slash = true;
        }

        if args.is_present("extract_links") {
            config.extract_links = true;
        }

        if args.is_present("json") {
            config.json = true;
        }

        if args.is_present("stdin") {
            config.stdin = true;
        } else if let Some(url) = args.value_of("url") {
            config.target_url = String::from(url);
        }

        ////
        // organizational breakpoint; all options below alter the Client configuration
        ////
        update_config_if_present!(&mut config.proxy, args, "proxy", String);
        update_config_if_present!(&mut config.replay_proxy, args, "replay_proxy", String);
        update_config_if_present!(&mut config.user_agent, args, "user_agent", String);
        update_config_if_present!(&mut config.timeout, args, "timeout", u64);

        if args.is_present("redirects") {
            config.redirects = true;
        }

        if args.is_present("insecure") {
            config.insecure = true;
        }

        if let Some(headers) = args.values_of("headers") {
            for val in headers {
                let mut split_val = val.split(':');

                // explicitly take first split value as header's name
                let name = split_val.next().unwrap().trim();

                // all other items in the iterator returned by split, when combined with the
                // original split deliminator (:), make up the header's final value
                let value = split_val.collect::<Vec<&str>>().join(":");
                config.headers.insert(name.to_string(), value.to_string());
            }
        }

        if let Some(queries) = args.values_of("queries") {
            for val in queries {
                // same basic logic used as reading in the headers HashMap above
                let mut split_val = val.split('=');

                let name = split_val.next().unwrap().trim();

                let value = split_val.collect::<Vec<&str>>().join("=");

                config.queries.push((name.to_string(), value.to_string()));
            }
        }

        config
    }

    /// this function determines if we've gotten a Client configuration change from
    /// either the config file or command line arguments; if we have, we need to rebuild
    /// the client and store it in the config struct
    fn try_rebuild_clients(configuration: &mut Configuration) {
        if !configuration.proxy.is_empty()
            || configuration.timeout != timeout()
            || configuration.user_agent != user_agent()
            || configuration.redirects
            || configuration.insecure
            || !configuration.headers.is_empty()
            || configuration.resumed
        {
            if configuration.proxy.is_empty() {
                configuration.client = client::initialize(
                    configuration.timeout,
                    &configuration.user_agent,
                    configuration.redirects,
                    configuration.insecure,
                    &configuration.headers,
                    None,
                )
            } else {
                configuration.client = client::initialize(
                    configuration.timeout,
                    &configuration.user_agent,
                    configuration.redirects,
                    configuration.insecure,
                    &configuration.headers,
                    Some(&configuration.proxy),
                )
            }
        }

        if !configuration.replay_proxy.is_empty() {
            // only set replay_client when replay_proxy is set
            configuration.replay_client = Some(client::initialize(
                configuration.timeout,
                &configuration.user_agent,
                configuration.redirects,
                configuration.insecure,
                &configuration.headers,
                Some(&configuration.replay_proxy),
            ));
        }
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
    fn merge_config(conf: &mut Self, new: Self) {
        // does not include the following Configuration fields, as they don't make sense here
        //  - kind
        //  - client
        //  - replay_client
        //  - resumed
        //  - config
        update_if_not_default!(&mut conf.target_url, new.target_url, "");
        update_if_not_default!(&mut conf.time_limit, new.time_limit, "");
        update_if_not_default!(&mut conf.proxy, new.proxy, "");
        update_if_not_default!(&mut conf.verbosity, new.verbosity, 0);
        update_if_not_default!(&mut conf.quiet, new.quiet, false);
        update_if_not_default!(&mut conf.output, new.output, "");
        update_if_not_default!(&mut conf.redirects, new.redirects, false);
        update_if_not_default!(&mut conf.insecure, new.insecure, false);
        update_if_not_default!(&mut conf.extract_links, new.extract_links, false);
        update_if_not_default!(&mut conf.extensions, new.extensions, Vec::<String>::new());
        update_if_not_default!(&mut conf.headers, new.headers, HashMap::new());
        update_if_not_default!(&mut conf.queries, new.queries, Vec::new());
        update_if_not_default!(&mut conf.no_recursion, new.no_recursion, false);
        update_if_not_default!(&mut conf.add_slash, new.add_slash, false);
        update_if_not_default!(&mut conf.stdin, new.stdin, false);
        update_if_not_default!(&mut conf.filter_size, new.filter_size, Vec::<u64>::new());
        update_if_not_default!(
            &mut conf.filter_regex,
            new.filter_regex,
            Vec::<String>::new()
        );
        update_if_not_default!(
            &mut conf.filter_similar,
            new.filter_similar,
            Vec::<String>::new()
        );
        update_if_not_default!(
            &mut conf.filter_word_count,
            new.filter_word_count,
            Vec::<usize>::new()
        );
        update_if_not_default!(
            &mut conf.filter_line_count,
            new.filter_line_count,
            Vec::<usize>::new()
        );
        update_if_not_default!(
            &mut conf.filter_status,
            new.filter_status,
            Vec::<u16>::new()
        );
        update_if_not_default!(&mut conf.dont_filter, new.dont_filter, false);
        update_if_not_default!(&mut conf.scan_limit, new.scan_limit, 0);
        update_if_not_default!(&mut conf.replay_proxy, new.replay_proxy, "");
        update_if_not_default!(&mut conf.debug_log, new.debug_log, "");
        update_if_not_default!(&mut conf.resume_from, new.resume_from, "");
        update_if_not_default!(&mut conf.json, new.json, false);

        update_if_not_default!(&mut conf.timeout, new.timeout, timeout());
        update_if_not_default!(&mut conf.user_agent, new.user_agent, user_agent());
        update_if_not_default!(&mut conf.threads, new.threads, threads());
        update_if_not_default!(&mut conf.depth, new.depth, depth());
        update_if_not_default!(&mut conf.wordlist, new.wordlist, wordlist());
        update_if_not_default!(&mut conf.status_codes, new.status_codes, status_codes());
        // status_codes() is the default for replay_codes, if they're not provided
        update_if_not_default!(&mut conf.replay_codes, new.replay_codes, status_codes());
        update_if_not_default!(&mut conf.save_state, new.save_state, save_state());
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

/// Implementation of FeroxMessage
impl FeroxSerialize for Configuration {
    /// Simple wrapper around create_report_string
    fn as_str(&self) -> String {
        format!("{:#?}\n", *self)
    }

    /// Create an NDJSON representation of the current scan's Configuration
    ///
    /// (expanded for clarity)
    /// ex:
    /// {
    ///    "type":"configuration",
    ///    "wordlist":"test",
    ///    "config":"/home/epi/.config/feroxbuster/ferox-config.toml",
    ///    "proxy":"",
    ///    "replay_proxy":"",
    ///    "target_url":"https://localhost.com",
    ///    "status_codes":[
    ///       200,
    ///       204,
    ///       301,
    ///       302,
    ///       307,
    ///       308,
    ///       401,
    ///       403,
    ///       405
    ///    ],
    /// ...
    /// }\n
    fn as_json(&self) -> String {
        if let Ok(mut json) = serde_json::to_string(&self) {
            json.push('\n');
            json
        } else {
            String::from("{\"error\":\"could not Configuration convert to json\"}")
        }
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
            time_limit = "10m"
            output = "/some/otherpath"
            debug_log = "/yet/anotherpath"
            resume_from = "/some/state/file"
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
            json = true
            save_state = false
            depth = 1
            filter_size = [4120]
            filter_regex = ["^ignore me$"]
            filter_similar = ["https://somesite.com/soft404"]
            filter_word_count = [994, 992]
            filter_line_count = [34]
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
        assert_eq!(config.time_limit, String::new());
        assert_eq!(config.resume_from, String::new());
        assert_eq!(config.debug_log, String::new());
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
        assert_eq!(config.json, false);
        assert_eq!(config.save_state, true);
        assert_eq!(config.stdin, false);
        assert_eq!(config.add_slash, false);
        assert_eq!(config.redirects, false);
        assert_eq!(config.extract_links, false);
        assert_eq!(config.insecure, false);
        assert_eq!(config.queries, Vec::new());
        assert_eq!(config.extensions, Vec::<String>::new());
        assert_eq!(config.filter_size, Vec::<u64>::new());
        assert_eq!(config.filter_regex, Vec::<String>::new());
        assert_eq!(config.filter_similar, Vec::<String>::new());
        assert_eq!(config.filter_word_count, Vec::<usize>::new());
        assert_eq!(config.filter_line_count, Vec::<usize>::new());
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
    fn config_reads_debug_log() {
        let config = setup_config_test();
        assert_eq!(config.debug_log, "/yet/anotherpath");
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
    fn config_reads_json() {
        let config = setup_config_test();
        assert_eq!(config.json, true);
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
    fn config_reads_filter_regex() {
        let config = setup_config_test();
        assert_eq!(config.filter_regex, vec!["^ignore me$"]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_filter_similar() {
        let config = setup_config_test();
        assert_eq!(config.filter_similar, vec!["https://somesite.com/soft404"]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_filter_size() {
        let config = setup_config_test();
        assert_eq!(config.filter_size, vec![4120]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_filter_word_count() {
        let config = setup_config_test();
        assert_eq!(config.filter_word_count, vec![994, 992]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_filter_line_count() {
        let config = setup_config_test();
        assert_eq!(config.filter_line_count, vec![34]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_filter_status() {
        let config = setup_config_test();
        assert_eq!(config.filter_status, vec![201]);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_save_state() {
        let config = setup_config_test();
        assert_eq!(config.save_state, false);
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_time_limit() {
        let config = setup_config_test();
        assert_eq!(config.time_limit, "10m");
    }

    #[test]
    /// parse the test config and see that the value parsed is correct
    fn config_reads_resume_from() {
        let config = setup_config_test();
        assert_eq!(config.resume_from, "/some/state/file");
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

    #[test]
    /// test as_str method of Configuration
    fn as_str_returns_string_with_newline() {
        let config = Configuration::new();
        let config_str = config.as_str();
        println!("{}", config_str);
        assert!(config_str.starts_with("Configuration {"));
        assert!(config_str.ends_with("}\n"));
        assert!(config_str.contains("replay_codes:"));
        assert!(config_str.contains("client: Client {"));
        assert!(config_str.contains("user_agent: \"feroxbuster"));
    }

    #[test]
    /// test as_json method of Configuration
    fn as_json_returns_json_representation_of_configuration_with_newline() {
        let mut config = Configuration::new();
        config.timeout = 12;
        config.depth = 2;
        let config_str = config.as_json();
        let json: Configuration = serde_json::from_str(&config_str).unwrap();
        assert_eq!(json.config, config.config);
        assert_eq!(json.wordlist, config.wordlist);
        assert_eq!(json.replay_codes, config.replay_codes);
        assert_eq!(json.timeout, config.timeout);
        assert_eq!(json.depth, config.depth);
    }
}
