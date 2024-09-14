use super::utils::{
    backup_extensions, depth, determine_requester_policy, extract_links, ignored_extensions,
    methods, parse_request_file, report_and_exit, request_protocol, save_state, serialized_type,
    split_header, split_query, status_codes, threads, timeout, user_agent, wordlist, OutputLevel,
    RequesterPolicy,
};

use crate::config::determine_output_level;
use crate::{
    client, parser,
    scan_manager::resume_scan,
    traits::FeroxSerialize,
    utils::{fmt_err, module_colorizer, parse_url_with_raw_path, status_colorizer},
    DEFAULT_CONFIG_NAME,
};
use anyhow::{anyhow, Context, Result};
use clap::{parser::ValueSource, ArgMatches};
use regex::Regex;
use reqwest::{Client, Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env::{current_dir, current_exe},
    fs::read_to_string,
    path::{Path, PathBuf},
};

/// macro helper to abstract away repetitive configuration updates
macro_rules! update_config_if_present {
    ($conf_val:expr, $matches:ident, $arg_name:expr, $arg_type:ty) => {
        match $matches.get_one::<$arg_type>($arg_name) {
            Some(value) => *$conf_val = value.to_owned(), // Update value
            None => {}
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

/// macro helper to abstract away repetitive checks to see if the user has specified a value
/// for a given argument from the commandline or if we just had a default value in the parser
macro_rules! came_from_cli {
    ($matches:ident, $arg_name:expr) => {
        matches!(
            $matches.value_source($arg_name),
            Some(ValueSource::CommandLine)
        )
    };
}

/// macro helper to abstract away repetitive if not default: update checks, specifically for
/// values that are number types, i.e. usize, u64, etc
macro_rules! update_config_with_num_type_if_present {
    ($conf_val:expr, $matches:ident, $arg_name:expr, $arg_type:ty) => {
        if let Some(val) = $matches.get_one::<String>($arg_name) {
            match val.parse::<$arg_type>() {
                Ok(v) => *$conf_val = v,
                Err(_) => {
                    report_and_exit(&format!(
                        "Invalid value for --{}, must be a positive integer",
                        $arg_name
                    ));
                }
            }
        }
    };
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

    /// Path to a custom root certificate for connecting to servers with a self-signed certificate
    #[serde(default)]
    pub server_certs: Vec<String>,

    /// Path to a client's PEM encoded X509 certificate used during mutual authentication
    #[serde(default)]
    pub client_cert: String,

    /// Path to a client's PEM encoded PKSC #8 private key used during mutual authentication
    #[serde(default)]
    pub client_key: String,

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

    /// Only print URLs (was --quiet in versions < 2.0.0)
    #[serde(default)]
    pub silent: bool,

    /// No header, no status bars
    #[serde(default)]
    pub quiet: bool,

    /// more easily differentiate between the three states of output levels
    #[serde(skip)]
    pub output_level: OutputLevel,

    /// automatically bail at certain error thresholds
    #[serde(default)]
    pub auto_bail: bool,

    /// automatically try to lower request rate in order to reduce errors
    #[serde(default)]
    pub auto_tune: bool,

    /// more easily differentiate between the three requester policies
    #[serde(skip)]
    pub requester_policy: RequesterPolicy,

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

    /// Use random User-Agent
    #[serde(default)]
    pub random_agent: bool,

    /// Follow redirects
    #[serde(default)]
    pub redirects: bool,

    /// Disables TLS certificate validation
    #[serde(default)]
    pub insecure: bool,

    /// File extension(s) to search for
    #[serde(default)]
    pub extensions: Vec<String>,

    /// HTTP requests methods(s) to search for
    #[serde(default = "methods")]
    pub methods: Vec<String>,

    /// HTTP Body data to send during request
    #[serde(default)]
    pub data: Vec<u8>,

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
    #[serde(default = "extract_links")]
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

    /// Number of parallel scans permitted; a limit of 0 means no limit is imposed
    #[serde(default)]
    pub parallel: usize,

    /// Number of requests per second permitted (per directory); a limit of 0 means no limit is imposed
    #[serde(default)]
    pub rate_limit: usize,

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
    #[serde(default = "save_state")]
    pub save_state: bool,

    /// The maximum runtime for a scan, expressed as N[smdh] where N can be parsed into a
    /// non-negative integer and the next character is either s, m, h, or d (case insensitive)
    #[serde(default)]
    pub time_limit: String,

    /// Filter out response bodies that meet a certain threshold of similarity
    #[serde(default)]
    pub filter_similar: Vec<String>,

    /// URLs that should never be scanned/recursed into
    #[serde(default)]
    pub url_denylist: Vec<Url>,

    /// URLs that should never be scanned/recursed into based on a regular expression
    #[serde(with = "serde_regex", default)]
    pub regex_denylist: Vec<Regex>,

    /// Automatically discover extensions and add them to --extensions (unless they're in --dont-collect)
    #[serde(default)]
    pub collect_extensions: bool,

    /// don't collect any of these extensions when --collect-extensions is used
    #[serde(default = "ignored_extensions")]
    pub dont_collect: Vec<String>,

    /// Automatically request likely backup extensions on "found" urls
    #[serde(default)]
    pub collect_backups: bool,

    #[serde(default = "backup_extensions")]
    pub backup_extensions: Vec<String>,

    /// Automatically discover important words from within responses and add them to the wordlist
    #[serde(default)]
    pub collect_words: bool,

    /// override recursion logic to always attempt recursion, still respects --depth
    #[serde(default)]
    pub force_recursion: bool,

    /// Auto update app feature
    #[serde(skip)]
    pub update_app: bool,

    /// whether to recurse into directory listings or not
    #[serde(default)]
    pub scan_dir_listings: bool,

    /// path to a raw request file generated by burp or similar
    #[serde(skip)]
    pub request_file: String,

    /// default request protocol
    #[serde(default = "request_protocol")]
    pub protocol: String,

    /// number of directory scan bars to show at any given time, 0 is no limit
    #[serde(default)]
    pub limit_bars: usize,
}

impl Default for Configuration {
    /// Builds the default Configuration for feroxbuster
    fn default() -> Self {
        let timeout = timeout();
        let user_agent = user_agent();
        let client = client::initialize(
            timeout,
            &user_agent,
            false,
            false,
            &HashMap::new(),
            None,
            Vec::<String>::new(),
            None,
            None,
        )
        .expect("Could not build client");
        let replay_client = None;
        let status_codes = status_codes();
        let replay_codes = status_codes.clone();
        let kind = serialized_type();
        let output_level = OutputLevel::Default;
        let requester_policy = RequesterPolicy::Default;
        let extract_links = extract_links();

        Configuration {
            kind,
            client,
            timeout,
            user_agent,
            replay_codes,
            status_codes,
            extract_links,
            replay_client,
            requester_policy,
            dont_filter: false,
            auto_bail: false,
            auto_tune: false,
            silent: false,
            quiet: false,
            output_level,
            resumed: false,
            stdin: false,
            json: false,
            scan_dir_listings: false,
            verbosity: 0,
            scan_limit: 0,
            parallel: 0,
            rate_limit: 0,
            limit_bars: 0,
            add_slash: false,
            insecure: false,
            redirects: false,
            no_recursion: false,
            random_agent: false,
            collect_extensions: false,
            collect_backups: false,
            collect_words: false,
            save_state: true,
            force_recursion: false,
            update_app: false,
            proxy: String::new(),
            client_cert: String::new(),
            client_key: String::new(),
            config: String::new(),
            output: String::new(),
            debug_log: String::new(),
            target_url: String::new(),
            time_limit: String::new(),
            resume_from: String::new(),
            replay_proxy: String::new(),
            request_file: String::new(),
            protocol: request_protocol(),
            server_certs: Vec::new(),
            queries: Vec::new(),
            extensions: Vec::new(),
            methods: methods(),
            data: Vec::new(),
            filter_size: Vec::new(),
            filter_regex: Vec::new(),
            url_denylist: Vec::new(),
            regex_denylist: Vec::new(),
            filter_line_count: Vec::new(),
            filter_word_count: Vec::new(),
            filter_status: Vec::new(),
            filter_similar: Vec::new(),
            headers: HashMap::new(),
            depth: depth(),
            threads: threads(),
            wordlist: wordlist(),
            dont_collect: ignored_extensions(),
            backup_extensions: backup_extensions(),
        }
    }
}

impl Configuration {
    /// Creates a [Configuration](struct.Configuration.html) object with the following
    /// built-in default values
    ///
    /// - **timeout**: `5` seconds
    /// - **redirects**: `false`
    /// - **extract_links**: `true`
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
    /// - **silent**: `false`
    /// - **auto_tune**: `false`
    /// - **auto_bail**: `false`
    /// - **save_state**: `true`
    /// - **user_agent**: `feroxbuster/VERSION`
    /// - **random_agent**: `false`
    /// - **insecure**: `false` (don't be insecure, i.e. don't allow invalid certs)
    /// - **extensions**: `None`
    /// - **collect_extensions**: `false`
    /// - **collect_backups**: `false`
    /// - **backup_extensions**: [`DEFAULT_BACKUP_EXTENSIONS`](constant.DEFAULT_BACKUP_EXTENSIONS.html)
    /// - **collect_words**: `false`
    /// - **dont_collect**: [`DEFAULT_IGNORED_EXTENSIONS`](constant.DEFAULT_RESPONSE_CODES.html)
    /// - **methods**: [`DEFAULT_METHOD`](constant.DEFAULT_METHOD.html)
    /// - **data**: `None`
    /// - **url_denylist**: `None`
    /// - **regex_denylist**: `None`
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
    /// - **force_recursion**: `false` (still respects recursion depth)
    /// - **scan_limit**: `0` (no limit on concurrent scans imposed)
    /// - **limit_bars**: `0` (no limit on number of directory scan bars shown)
    /// - **parallel**: `0` (no limit on parallel scans imposed)
    /// - **rate_limit**: `0` (no limit on requests per second imposed)
    /// - **time_limit**: `None` (no limit on length of scan imposed)
    /// - **replay_proxy**: `None` (no limit on concurrent scans imposed)
    /// - **replay_codes**: [`DEFAULT_RESPONSE_CODES`](constant.DEFAULT_RESPONSE_CODES.html)
    /// - **update_app**: `false`
    /// - **scan_dir_listings**: `false`
    /// - **request_file**: `None`
    /// - **protocol**: `https`
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
    pub fn new() -> Result<Self> {
        // when compiling for test, we want to eliminate the runtime dependency of the parser
        if cfg!(test) {
            let test_config = Configuration {
                save_state: false, // don't clutter up junk when testing
                ..Default::default()
            };
            return Ok(test_config);
        }

        let args = parser::initialize().get_matches();

        // Get the default configuration, this is what will apply if nothing
        // else is specified.
        let mut config = Configuration::default();

        // read in all config files
        Self::parse_config_files(&mut config)?;

        // read in the user provided options, this produces a separate instance of Configuration
        // in order to allow for potentially merging into a --resume-from Configuration
        let cli_config = Self::parse_cli_args(&args);

        // --resume-from used, need to first read the Configuration from disk, and then
        // merge the cli_config into the resumed config
        if let Some(filename) = args.get_one::<String>("resume_from") {
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

            return Ok(previous_config);
        }

        // if we've gotten to this point in the code, --resume-from was not used, so we need to
        // merge the cli options into the config file options and return the result
        Self::merge_config(&mut config, cli_config);

        // if the user provided a raw request file as the target, we'll need to parse out
        // the provided info and update the config with those values. This call needs to
        // come after the cli/config merge so we can allow the cli options to override
        // the raw request values (i.e. --headers "stuff: things" should override a "stuff"
        // header from the raw request).
        //
        // Additionally, this call needs to come before client rebuild so that the things
        // like user-agent can be set at the client level instead of the header level.
        if !config.request_file.is_empty() {
            parse_request_file(&mut config)?;
        }

        // rebuild clients is the last step in either code branch
        Self::try_rebuild_clients(&mut config);

        Ok(config)
    }

    /// Parse all possible versions of the ferox-config.toml file, adhering to the order of
    /// precedence outlined above
    fn parse_config_files(config: &mut Self) -> Result<()> {
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
        let config_file = Path::new("/etc/feroxbuster").join(DEFAULT_CONFIG_NAME);
        Self::parse_and_merge_config(config_file, config)?;

        // merge a config found at ~/.config/feroxbuster/ferox-config.toml
        // config_dir() resolves to one of the following
        //   - linux: $XDG_CONFIG_HOME or $HOME/.config
        //   - macOS: $HOME/Library/Application Support
        //   - windows: {FOLDERID_RoamingAppData}
        let config_dir = dirs::config_dir().ok_or_else(|| anyhow!("Couldn't load config"))?;
        let config_file = config_dir.join("feroxbuster").join(DEFAULT_CONFIG_NAME);
        Self::parse_and_merge_config(config_file, config)?;

        // merge a config found in same the directory as feroxbuster executable
        let exe_path = current_exe()?;
        let bin_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow!("Couldn't load config"))?;
        let config_file = bin_dir.join(DEFAULT_CONFIG_NAME);
        Self::parse_and_merge_config(config_file, config)?;

        // merge a config found in the user's current working directory
        let cwd = current_dir()?;
        let config_file = cwd.join(DEFAULT_CONFIG_NAME);
        Self::parse_and_merge_config(config_file, config)?;

        Ok(())
    }

    /// Given a set of ArgMatches read from the CLI, update and return the default Configuration
    /// settings
    fn parse_cli_args(args: &ArgMatches) -> Self {
        let mut config = Configuration::default();

        update_config_with_num_type_if_present!(&mut config.threads, args, "threads", usize);
        update_config_with_num_type_if_present!(&mut config.parallel, args, "parallel", usize);
        update_config_with_num_type_if_present!(&mut config.depth, args, "depth", usize);
        update_config_with_num_type_if_present!(&mut config.scan_limit, args, "scan_limit", usize);
        update_config_with_num_type_if_present!(&mut config.rate_limit, args, "rate_limit", usize);
        update_config_with_num_type_if_present!(&mut config.limit_bars, args, "limit_bars", usize);
        update_config_if_present!(&mut config.wordlist, args, "wordlist", String);
        update_config_if_present!(&mut config.output, args, "output", String);
        update_config_if_present!(&mut config.debug_log, args, "debug_log", String);
        update_config_if_present!(&mut config.resume_from, args, "resume_from", String);
        update_config_if_present!(&mut config.request_file, args, "request_file", String);
        update_config_if_present!(&mut config.protocol, args, "protocol", String);

        if let Ok(Some(inner)) = args.try_get_one::<String>("time_limit") {
            inner.clone_into(&mut config.time_limit);
        }

        if let Some(arg) = args.get_many::<String>("status_codes") {
            config.status_codes = arg
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_u16()
                })
                .collect();
        }

        if let Some(arg) = args.get_many::<String>("replay_codes") {
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
            config.replay_codes.clone_from(&config.status_codes);
        }

        if let Some(arg) = args.get_many::<String>("filter_status") {
            config.filter_status = arg
                .map(|code| {
                    StatusCode::from_bytes(code.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_u16()
                })
                .collect();
        }

        if let Some(arg) = args.get_many::<String>("extensions") {
            let mut extensions = Vec::<String>::new();
            for ext in arg {
                if let Some(stripped) = ext.strip_prefix('@') {
                    let contents = read_to_string(stripped)
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()));
                    let exts_from_file = contents.split('\n').filter_map(|s| {
                        let trimmed = s.trim().trim_start_matches('.');

                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    });

                    extensions.extend(exts_from_file);
                } else {
                    extensions.push(ext.trim().trim_start_matches('.').to_string());
                }
            }
            config.extensions = extensions;
        }

        if let Some(arg) = args.get_many::<String>("dont_collect") {
            config.dont_collect = arg.map(|val| val.to_string()).collect();
        }

        if let Some(arg) = args.get_many::<String>("methods") {
            config.methods = arg
                .map(|val| {
                    // Check methods if they are correct
                    Method::from_bytes(val.as_bytes())
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                        .as_str()
                        .to_string()
                })
                .collect();
        }

        if let Some(arg) = args.get_one::<String>("data") {
            if let Some(stripped) = arg.strip_prefix('@') {
                config.data =
                    std::fs::read(stripped).unwrap_or_else(|e| report_and_exit(&e.to_string()));
            } else {
                config.data = arg.as_bytes().to_vec();
            }

            if config.methods == methods() {
                // if the user didn't specify a method, we're going to assume they meant to use POST
                config.methods = vec![Method::POST.as_str().to_string()];
            }
        }

        if came_from_cli!(args, "stdin") {
            config.stdin = true;
        } else if let Some(url) = args.get_one::<String>("url") {
            config.target_url = url.into();
        }

        if let Some(arg) = args.get_many::<String>("url_denylist") {
            // compile all regular expressions and absolute urls used for --dont-scan
            //
            // when --dont-scan is used, the should_deny_url function is called at least once per
            // url to be scanned. With the addition of regex support, I want to move parsing
            // out of should_deny_url and into here, so it's performed once instead of thousands
            // of times
            for denier in arg {
                // could be an absolute url or a regex, need to determine which and populate the
                // appropriate vector
                match parse_url_with_raw_path(denier.trim_end_matches('/')) {
                    Ok(absolute) => {
                        // denier is an absolute url and can be parsed as such
                        config.url_denylist.push(absolute);
                    }
                    Err(err) => {
                        // there are some expected errors that happen when we try to parse a url
                        //     ex: Url::parse("/login") -> Err("relative URL without a base")
                        //     ex: Url::parse("http:") -> Err("empty host")
                        //
                        // these are known errors and are used to determine a valid value to
                        // --dont-scan, when it's not an absolute url
                        //
                        // when expected errors are encountered, we're going to assume
                        // that the input is a regular expression to be parsed. The possibility
                        // exists that the user rolled their face across the keyboard and we're
                        // dealing with the results, in which case we'll report it as an error and
                        // give up
                        if err.to_string().contains("relative URL without a base")
                            || err.to_string().contains("empty host")
                        {
                            let regex = Regex::new(denier)
                                .unwrap_or_else(|e| report_and_exit(&e.to_string()));

                            config.regex_denylist.push(regex);
                        } else {
                            // unexpected error has occurred; bail
                            report_and_exit(&err.to_string());
                        }
                    }
                }
            }
        }

        if let Some(arg) = args.get_many::<String>("filter_regex") {
            config.filter_regex = arg.map(|val| val.to_string()).collect();
        }

        if let Some(arg) = args.get_many::<String>("filter_similar") {
            config.filter_similar = arg.map(|val| val.to_string()).collect();
        }

        if let Some(arg) = args.get_many::<String>("filter_size") {
            config.filter_size = arg
                .map(|size| {
                    size.parse::<u64>()
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                })
                .collect();
        }

        if let Some(arg) = args.get_many::<String>("filter_words") {
            config.filter_word_count = arg
                .map(|size| {
                    size.parse::<usize>()
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                })
                .collect();
        }

        if let Some(arg) = args.get_many::<String>("filter_lines") {
            config.filter_line_count = arg
                .map(|size| {
                    size.parse::<usize>()
                        .unwrap_or_else(|e| report_and_exit(&e.to_string()))
                })
                .collect();
        }

        if came_from_cli!(args, "quiet") {
            config.quiet = true;
            config.output_level = OutputLevel::Quiet;
        }

        if came_from_cli!(args, "silent") || (config.parallel > 0 && !config.quiet) {
            // the reason this is protected by an if statement:
            // consider a user specifying silent = true in ferox-config.toml
            // if the line below is outside of the if, we'd overwrite true with
            // false if no --silent is used on the command line
            config.silent = true;
            config.output_level = if config.json {
                OutputLevel::SilentJSON
            } else {
                OutputLevel::Silent
            };
        }

        if came_from_cli!(args, "auto_tune")
            || came_from_cli!(args, "smart")
            || came_from_cli!(args, "thorough")
        {
            config.auto_tune = true;
            config.requester_policy = RequesterPolicy::AutoTune;
        }

        if came_from_cli!(args, "auto_bail") {
            config.auto_bail = true;
            config.requester_policy = RequesterPolicy::AutoBail;
        }

        if came_from_cli!(args, "no_state") {
            config.save_state = false;
        }

        if came_from_cli!(args, "scan_dir_listings") || came_from_cli!(args, "thorough") {
            config.scan_dir_listings = true;
        }

        if came_from_cli!(args, "dont_filter") {
            config.dont_filter = true;
        }

        if came_from_cli!(args, "collect_extensions") || came_from_cli!(args, "thorough") {
            config.collect_extensions = true;
        }

        if came_from_cli!(args, "collect_backups")
            || came_from_cli!(args, "smart")
            || came_from_cli!(args, "thorough")
        {
            config.collect_backups = true;
            config.backup_extensions = backup_extensions();

            if came_from_cli!(args, "collect_backups") {
                if let Some(arg) = args.get_many::<String>("collect_backups") {
                    let backup_exts = arg
                        .map(|ext| ext.trim().to_string())
                        .collect::<Vec<String>>();

                    if !backup_exts.is_empty() {
                        // have at least one cli backup, override the defaults
                        config.backup_extensions = backup_exts;
                    }
                }
            }
        }

        if came_from_cli!(args, "collect_words")
            || came_from_cli!(args, "smart")
            || came_from_cli!(args, "thorough")
        {
            config.collect_words = true;
        }

        if args.get_count("verbosity") > 0 {
            // occurrences_of returns 0 if none are found; this is protected in
            // an if block for the same reason as the quiet option
            config.verbosity = args.get_count("verbosity");

            // todo: starting on 2.11.0 (907-dont-skip-dir-listings), trace-level
            //   logging started causing the following error:
            //
            // thread 'tokio-runtime-worker' has overflowed its stack
            // fatal runtime error: stack overflow
            // Aborted (core dumped)
            //
            // as a temporary fix, we'll disable trace logging to prevent the stack
            // overflow until I get time to investigate the root cause
            if config.verbosity > 3 {
                eprintln!(
                    "{} {}: Trace level logging is disabled; setting log level to debug",
                    status_colorizer("WRN"),
                    module_colorizer("Configuration::parse_cli_args"),
                );

                config.verbosity = 3;
            }
        }

        if came_from_cli!(args, "no_recursion") {
            config.no_recursion = true;
        }

        if came_from_cli!(args, "add_slash") {
            config.add_slash = true;
        }

        if came_from_cli!(args, "dont_extract_links") {
            config.extract_links = false;
        }

        if came_from_cli!(args, "json") {
            config.json = true;
        }

        if came_from_cli!(args, "force_recursion") {
            config.force_recursion = true;
        }

        if came_from_cli!(args, "update_app") {
            config.update_app = true;
        }

        ////
        // organizational breakpoint; all options below alter the Client configuration
        ////
        update_config_if_present!(&mut config.proxy, args, "proxy", String);
        update_config_if_present!(&mut config.client_cert, args, "client_cert", String);
        update_config_if_present!(&mut config.client_key, args, "client_key", String);
        update_config_if_present!(&mut config.replay_proxy, args, "replay_proxy", String);
        update_config_if_present!(&mut config.user_agent, args, "user_agent", String);
        update_config_with_num_type_if_present!(&mut config.timeout, args, "timeout", u64);

        if came_from_cli!(args, "burp") {
            config.proxy = String::from("http://127.0.0.1:8080");
        }

        if came_from_cli!(args, "burp_replay") {
            config.replay_proxy = String::from("http://127.0.0.1:8080");
        }

        if came_from_cli!(args, "random_agent") {
            config.random_agent = true;
        }

        if came_from_cli!(args, "redirects") {
            config.redirects = true;
        }

        if came_from_cli!(args, "insecure")
            || came_from_cli!(args, "burp")
            || came_from_cli!(args, "burp_replay")
        {
            config.insecure = true;
        }

        if let Some(headers) = args.get_many::<String>("headers") {
            for val in headers {
                let Ok((name, value)) = split_header(val) else {
                    log::warn!("Invalid header: {}", val);
                    continue;
                };
                config.headers.insert(name, value);
            }
        }

        if let Some(cookies) = args.get_many::<String>("cookies") {
            config.headers.insert(
                // we know the header name is always "cookie"
                "Cookie".to_string(),
                cookies
                    .flat_map(|cookie| {
                        cookie.split(';').filter_map(|part| {
                            // trim the spaces
                            let trimmed = part.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                // join with an equals sign
                                let parts = trimmed.split('=').collect::<Vec<&str>>();
                                Some(format!(
                                    "{}={}",
                                    parts[0].trim(),
                                    parts[1..].join("").trim()
                                ))
                            }
                        })
                    })
                    .collect::<Vec<String>>()
                    // join all the cookies with semicolons for the final header
                    .join("; "),
            );
        }

        if let Some(queries) = args.get_many::<String>("queries") {
            for val in queries {
                let Ok((name, value)) = split_query(val) else {
                    log::warn!("Invalid query string: {}", val);
                    continue;
                };
                config.queries.push((name, value));
            }
        }

        if let Some(certs) = args.get_many::<String>("server_certs") {
            for val in certs {
                config.server_certs.push(val.to_string());
            }
        }

        config
    }

    /// this function determines if we've gotten a Client configuration change from
    /// either the config file or command line arguments; if we have, we need to rebuild
    /// the client and store it in the config struct
    fn try_rebuild_clients(configuration: &mut Configuration) {
        // check if the proxy and certificate fields are empty
        // and parse them into Some or None variants ahead of time
        // so we may use the is_some method on them instead of
        // multiple initializations
        let proxy = if configuration.proxy.is_empty() {
            None
        } else {
            Some(configuration.proxy.as_str())
        };

        let server_certs = &configuration.server_certs;

        let client_cert = if configuration.client_cert.is_empty() {
            None
        } else {
            Some(configuration.client_cert.as_str())
        };

        let client_key = if configuration.client_key.is_empty() {
            None
        } else {
            Some(configuration.client_key.as_str())
        };

        if proxy.is_some()
            || configuration.timeout != timeout()
            || configuration.user_agent != user_agent()
            || configuration.redirects
            || configuration.insecure
            || !configuration.headers.is_empty()
            || configuration.resumed
            || !server_certs.is_empty()
            || client_cert.is_some()
            || client_key.is_some()
        {
            configuration.client = client::initialize(
                configuration.timeout,
                &configuration.user_agent,
                configuration.redirects,
                configuration.insecure,
                &configuration.headers,
                proxy,
                server_certs,
                client_cert,
                client_key,
            )
            .expect("Could not rebuild client");
        }

        if !configuration.replay_proxy.is_empty() {
            // only set replay_client when replay_proxy is set
            configuration.replay_client = Some(
                client::initialize(
                    configuration.timeout,
                    &configuration.user_agent,
                    configuration.redirects,
                    configuration.insecure,
                    &configuration.headers,
                    Some(&configuration.replay_proxy),
                    server_certs,
                    client_cert,
                    client_key,
                )
                .expect("Could not rebuild client"),
            );
        }
    }

    /// Given a configuration file's location and an instance of `Configuration`, read in
    /// the config file if found and update the current settings with the settings found therein
    fn parse_and_merge_config(config_file: PathBuf, config: &mut Self) -> Result<()> {
        if config_file.exists() {
            // save off a string version of the path before it goes out of scope
            let conf_str = config_file.to_str().unwrap_or("").to_string();
            let settings = Self::parse_config(config_file)?;

            // set the config used for viewing in the banner
            config.config = conf_str;

            // update the settings
            Self::merge_config(config, settings);
        }
        Ok(())
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
        update_if_not_default!(
            &mut conf.server_certs,
            new.server_certs,
            Vec::<String>::new()
        );
        update_if_not_default!(&mut conf.json, new.json, false);
        update_if_not_default!(&mut conf.client_cert, new.client_cert, "");
        update_if_not_default!(&mut conf.client_key, new.client_key, "");
        update_if_not_default!(&mut conf.verbosity, new.verbosity, 0);
        update_if_not_default!(&mut conf.limit_bars, new.limit_bars, 0);
        update_if_not_default!(&mut conf.silent, new.silent, false);
        update_if_not_default!(&mut conf.quiet, new.quiet, false);
        update_if_not_default!(&mut conf.auto_bail, new.auto_bail, false);
        update_if_not_default!(&mut conf.auto_tune, new.auto_tune, false);
        update_if_not_default!(&mut conf.collect_extensions, new.collect_extensions, false);
        update_if_not_default!(&mut conf.collect_backups, new.collect_backups, false);
        update_if_not_default!(&mut conf.collect_words, new.collect_words, false);
        // use updated quiet/silent values to determine output level; same for requester policy
        conf.output_level = determine_output_level(conf.quiet, conf.silent, conf.json);
        conf.requester_policy = determine_requester_policy(conf.auto_tune, conf.auto_bail);
        update_if_not_default!(&mut conf.output, new.output, "");
        update_if_not_default!(&mut conf.redirects, new.redirects, false);
        update_if_not_default!(&mut conf.insecure, new.insecure, false);
        update_if_not_default!(&mut conf.force_recursion, new.force_recursion, false);
        update_if_not_default!(&mut conf.extract_links, new.extract_links, extract_links());
        update_if_not_default!(&mut conf.extensions, new.extensions, Vec::<String>::new());
        update_if_not_default!(&mut conf.methods, new.methods, methods());
        update_if_not_default!(&mut conf.data, new.data, Vec::<u8>::new());
        update_if_not_default!(&mut conf.url_denylist, new.url_denylist, Vec::<Url>::new());
        update_if_not_default!(&mut conf.update_app, new.update_app, false);
        if !new.regex_denylist.is_empty() {
            // cant use the update_if_not_default macro due to the following error
            //
            //    binary operation `!=` cannot be applied to type `Vec<regex::Regex>`
            //
            // if we get a non-empty list of regex in the new config, override the old
            conf.regex_denylist = new.regex_denylist;
        }
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
        update_if_not_default!(&mut conf.scan_dir_listings, new.scan_dir_listings, false);
        update_if_not_default!(&mut conf.scan_limit, new.scan_limit, 0);
        update_if_not_default!(&mut conf.parallel, new.parallel, 0);
        update_if_not_default!(&mut conf.rate_limit, new.rate_limit, 0);
        update_if_not_default!(&mut conf.replay_proxy, new.replay_proxy, "");
        update_if_not_default!(&mut conf.debug_log, new.debug_log, "");
        update_if_not_default!(&mut conf.resume_from, new.resume_from, "");
        update_if_not_default!(&mut conf.request_file, new.request_file, "");
        update_if_not_default!(&mut conf.protocol, new.protocol, request_protocol());

        update_if_not_default!(&mut conf.timeout, new.timeout, timeout());
        update_if_not_default!(&mut conf.user_agent, new.user_agent, user_agent());
        update_if_not_default!(
            &mut conf.backup_extensions,
            new.backup_extensions,
            backup_extensions()
        );
        update_if_not_default!(&mut conf.random_agent, new.random_agent, false);
        update_if_not_default!(&mut conf.threads, new.threads, threads());
        update_if_not_default!(&mut conf.depth, new.depth, depth());
        update_if_not_default!(&mut conf.wordlist, new.wordlist, wordlist());
        update_if_not_default!(&mut conf.status_codes, new.status_codes, status_codes());
        // status_codes() is the default for replay_codes, if they're not provided
        update_if_not_default!(&mut conf.replay_codes, new.replay_codes, status_codes());
        update_if_not_default!(&mut conf.save_state, new.save_state, save_state());
        update_if_not_default!(
            &mut conf.dont_collect,
            new.dont_collect,
            ignored_extensions()
        );
    }

    /// If present, read in `DEFAULT_CONFIG_NAME` and deserialize the specified values
    ///
    /// uses serde to deserialize the toml into a `Configuration` struct
    pub(super) fn parse_config(config_file: PathBuf) -> Result<Self> {
        let content = read_to_string(config_file)?;
        let mut config: Self = toml::from_str(content.as_str())?;

        if !config.extensions.is_empty() {
            // remove leading periods, if any are found
            config.extensions = config
                .extensions
                .iter()
                .map(|ext| ext.trim_start_matches('.').to_string())
                .collect();
        }

        Ok(config)
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
    fn as_json(&self) -> Result<String> {
        let mut json = serde_json::to_string(&self)
            .with_context(|| fmt_err("Could not convert Configuration to JSON"))?;
        json.push('\n');
        Ok(json)
    }
}
