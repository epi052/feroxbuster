use clap::ArgAction;
use clap::{
    crate_authors, crate_description, crate_name, crate_version, Arg, ArgGroup, Command, ValueHint,
};
use lazy_static::lazy_static;
use regex::Regex;
use std::env;
use std::process;

lazy_static! {
    /// Regex used to validate values passed to --time-limit
    ///
    /// Examples of expected values that will this regex will match:
    /// - 30s
    /// - 20m
    /// - 1h
    /// - 1d
    pub static ref TIMESPEC_REGEX: Regex =
        Regex::new(r"^(?i)(?P<n>\d+)(?P<m>[smdh])$").expect("Could not compile regex");

    /// help string for user agent, your guess is as good as mine as to why this is required...
    static ref DEFAULT_USER_AGENT: String = format!(
        "Sets the User-Agent (default: feroxbuster/{})",
        crate_version!()
    );
}

/// Create and return an instance of [clap::App](https://docs.rs/clap/latest/clap/struct.App.html), i.e. the Command Line Interface's configuration
pub fn initialize() -> Command {
    let app = Command::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!());

    /////////////////////////////////////////////////////////////////////
    // group - target selection
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("url")
                .short('u')
                .long("url")
                .required_unless_present_any(["stdin", "resume_from", "update_app", "request_file"])
                .help_heading("Target selection")
                .value_name("URL")
                .use_value_delimiter(true)
                .value_hint(ValueHint::Url)
                .help("The target URL (required, unless [--stdin || --resume-from || --request-file] used)"),
        )
        .arg(
            Arg::new("stdin")
                .long("stdin")
                .help_heading("Target selection")
                .num_args(0)
                .help("Read url(s) from STDIN")
                .conflicts_with("url")
        )
        .arg(
            Arg::new("resume_from")
                .long("resume-from")
                .value_hint(ValueHint::FilePath)
                .value_name("STATE_FILE")
                .help_heading("Target selection")
                .help("State file from which to resume a partially complete scan (ex. --resume-from ferox-1606586780.state)")
                .conflicts_with("url")
                .num_args(1),
        ).arg(
            Arg::new("request_file")
                .long("request-file")
                .help_heading("Target selection")
                .value_hint(ValueHint::FilePath)
                .conflicts_with("url")
                .num_args(1)
                .value_name("REQUEST_FILE")
                .help("Raw HTTP request file to use as a template for all requests"),
        );

    /////////////////////////////////////////////////////////////////////
    // group - composite settings
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("burp")
                .long("burp")
                .num_args(0)
                .help_heading("Composite settings")
                .conflicts_with_all(["proxy", "insecure", "burp_replay"])
                .help("Set --proxy to http://127.0.0.1:8080 and set --insecure to true"),
        )
        .arg(
            Arg::new("burp_replay")
                .long("burp-replay")
                .num_args(0)
                .help_heading("Composite settings")
                .conflicts_with_all(["replay_proxy", "insecure"])
                .help("Set --replay-proxy to http://127.0.0.1:8080 and set --insecure to true"),
        )
        .arg(
            Arg::new("smart")
                .long("smart")
                .num_args(0)
                .help_heading("Composite settings")
                .conflicts_with_all(["rate_limit", "auto_bail"])
                .help("Set --auto-tune, --collect-words, and --collect-backups to true"),
        )
        .arg(
            Arg::new("thorough")
                .long("thorough")
                .num_args(0)
                .help_heading("Composite settings")
                .conflicts_with_all(["rate_limit", "auto_bail"])
                .help("Use the same settings as --smart and set --collect-extensions and --scan-dir-listings to true"),
        );

    /////////////////////////////////////////////////////////////////////
    // group - proxy settings
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("proxy")
                .short('p')
                .long("proxy")
                .num_args(1)
                .value_name("PROXY")
                .value_hint(ValueHint::Url)
                .help_heading("Proxy settings")
                .help(
                    "Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)",
                ),
        )
        .arg(
            Arg::new("replay_proxy")
                .short('P')
                .long("replay-proxy")
                .num_args(1)
                .value_hint(ValueHint::Url)
                .value_name("REPLAY_PROXY")
                .help_heading("Proxy settings")
                .help(
                    "Send only unfiltered requests through a Replay Proxy, instead of all requests",
                ),
        )
        .arg(
            Arg::new("replay_codes")
                .short('R')
                .long("replay-codes")
                .value_name("REPLAY_CODE")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .requires("replay_proxy")
                .help_heading("Proxy settings")
                .help(
                    "Status Codes to send through a Replay Proxy when found (default: --status-codes value)",
                ),
        );

    /////////////////////////////////////////////////////////////////////
    // group - request settings
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("user_agent")
                .short('a')
                .long("user-agent")
                .value_name("USER_AGENT")
                .num_args(1)
                .help_heading("Request settings")
                .help(&**DEFAULT_USER_AGENT),
        )
        .arg(
            Arg::new("random_agent")
                .short('A')
                .long("random-agent")
                .num_args(0)
                .help_heading("Request settings")
                .help("Use a random User-Agent"),
        )
        .arg(
            Arg::new("extensions")
                .short('x')
                .long("extensions")
                .value_name("FILE_EXTENSION")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Request settings")
                .help(
                    "File extension(s) to search for (ex: -x php -x pdf js); reads values (newline-separated) from file if input starts with an @ (ex: @ext.txt)",
                ),
        )
        .arg(
            Arg::new("methods")
                .short('m')
                .long("methods")
                .value_name("HTTP_METHODS")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Request settings")
                .help(
                    "Which HTTP request method(s) should be sent (default: GET)",
                ),
        )
        .arg(
            Arg::new("data")
                .long("data")
                .value_name("DATA")
                .num_args(1)
                .help_heading("Request settings")
                .help(
                    "Request's Body; can read data from a file if input starts with an @ (ex: @post.bin)",
                ),
        )
        .arg(
            Arg::new("headers")
                .short('H')
                .long("headers")
                .value_name("HEADER")
                .num_args(1..)
                .action(ArgAction::Append)
                .help_heading("Request settings")
                .help(
                    "Specify HTTP headers to be used in each request (ex: -H Header:val -H 'stuff: things')",
                ),
        )
        .arg(
            Arg::new("cookies")
                .short('b')
                .long("cookies")
                .value_name("COOKIE")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Request settings")
                .help(
                    "Specify HTTP cookies to be used in each request (ex: -b stuff=things)",
                ),
        )
        .arg(
            Arg::new("queries")
                .short('Q')
                .long("query")
                .value_name("QUERY")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Request settings")
                .help(
                    "Request's URL query parameters (ex: -Q token=stuff -Q secret=key)",
                ),
        )
        .arg(
            Arg::new("add_slash")
                .short('f')
                .long("add-slash")
                .help_heading("Request settings")
                .num_args(0)
                .help("Append / to each request's URL")
        ).arg(
            Arg::new("protocol")
                .long("protocol")
                .value_name("PROTOCOL")
                .num_args(1)
                .help_heading("Request settings")
                .help("Specify the protocol to use when targeting via --request-file or --url with domain only (default: https)"),
        );

    /////////////////////////////////////////////////////////////////////
    // group - request filters
    /////////////////////////////////////////////////////////////////////
    let app = app.arg(
        Arg::new("url_denylist")
            .long("dont-scan")
            .value_name("URL")
            .num_args(1..)
            .action(ArgAction::Append)
            .use_value_delimiter(true)
            .help_heading("Request filters")
            .help("URL(s) or Regex Pattern(s) to exclude from recursion/scans"),
    );

    /////////////////////////////////////////////////////////////////////
    // group - response filters
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("filter_size")
                .short('S')
                .long("filter-size")
                .value_name("SIZE")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Response filters")
                .help(
                    "Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)",
                ),
        )
        .arg(
            Arg::new("filter_regex")
                .short('X')
                .long("filter-regex")
                .value_name("REGEX")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Response filters")
                .help(
                    "Filter out messages via regular expression matching on the response's body/headers (ex: -X '^ignore me$')",
                ),
        )
        .arg(
            Arg::new("filter_words")
                .short('W')
                .long("filter-words")
                .value_name("WORDS")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Response filters")
                .help(
                    "Filter out messages of a particular word count (ex: -W 312 -W 91,82)",
                ),
        )
        .arg(
            Arg::new("filter_lines")
                .short('N')
                .long("filter-lines")
                .value_name("LINES")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Response filters")
                .help(
                    "Filter out messages of a particular line count (ex: -N 20 -N 31,30)",
                ),
        )
        .arg(
            Arg::new("filter_status")
                .short('C')
                .long("filter-status")
                .value_name("STATUS_CODE")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .conflicts_with("status_codes")
                .help_heading("Response filters")
                .help(
                    "Filter out status codes (deny list) (ex: -C 200 -C 401)",
                ),
        )
        .arg(
            Arg::new("filter_similar")
                .long("filter-similar-to")
                .value_name("UNWANTED_PAGE")
                .num_args(1..)
                .action(ArgAction::Append)
                .value_hint(ValueHint::Url)
                .use_value_delimiter(true)
                .help_heading("Response filters")
                .help(
                    "Filter out pages that are similar to the given page (ex. --filter-similar-to http://site.xyz/soft404)",
                ),
        )
        .arg(
            Arg::new("status_codes")
                .short('s')
                .long("status-codes")
                .value_name("STATUS_CODE")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Response filters")
                .help(
                    "Status Codes to include (allow list) (default: All Status Codes)",
                ),
        );

    /////////////////////////////////////////////////////////////////////
    // group - client settings
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("timeout")
                .short('T')
                .long("timeout")
                .value_name("SECONDS")
                .num_args(1)
                .help_heading("Client settings")
                .help("Number of seconds before a client's request times out (default: 7)"),
        )
        .arg(
            Arg::new("redirects")
                .short('r')
                .long("redirects")
                .num_args(0)
                .help_heading("Client settings")
                .help("Allow client to follow redirects"),
        )
        .arg(
            Arg::new("insecure")
                .short('k')
                .long("insecure")
                .num_args(0)
                .help_heading("Client settings")
                .help("Disables TLS certificate validation in the client"),
        )
        .arg(
            Arg::new("server_certs")
                .long("server-certs")
                .value_name("PEM|DER")
                .value_hint(ValueHint::FilePath)
                .num_args(1..)
                .help_heading("Client settings")
                .help("Add custom root certificate(s) for servers with unknown certificates"),
        )
        .arg(
            Arg::new("client_cert")
                .long("client-cert")
                .value_name("PEM")
                .value_hint(ValueHint::FilePath)
                .num_args(1)
                .requires("client_key")
                .help_heading("Client settings")
                .help("Add a PEM encoded certificate for mutual authentication (mTLS)"),
        )
        .arg(
            Arg::new("client_key")
                .long("client-key")
                .value_name("PEM")
                .value_hint(ValueHint::FilePath)
                .num_args(1)
                .requires("client_cert")
                .help_heading("Client settings")
                .help("Add a PEM encoded private key for mutual authentication (mTLS)"),
        );

    /////////////////////////////////////////////////////////////////////
    // group - scan settings
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("threads")
                .short('t')
                .long("threads")
                .value_name("THREADS")
                .num_args(1)
                .help_heading("Scan settings")
                .help("Number of concurrent threads (default: 50)"),
        )
        .arg(
            Arg::new("no_recursion")
                .short('n')
                .long("no-recursion")
                .num_args(0)
                .help_heading("Scan settings")
                .help("Do not scan recursively"),
        )
        .arg(
            Arg::new("depth")
                .short('d')
                .long("depth")
                .value_name("RECURSION_DEPTH")
                .num_args(1)
                .help_heading("Scan settings")
                .help("Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)"),
        ).arg(
            Arg::new("force_recursion")
                .long("force-recursion")
                .num_args(0)
                .conflicts_with("no_recursion")
                .help_heading("Scan settings")
                .help("Force recursion attempts on all 'found' endpoints (still respects recursion depth)"),
        ).arg(
            Arg::new("extract_links")
                .short('e')
                .long("extract-links")
                .num_args(0)
                .help_heading("Scan settings")
                .hide(true)
                .help("Extract links from response body (html, javascript, etc...); make new requests based on findings (default: true)")
        )
        .arg(
            Arg::new("dont_extract_links")
                .long("dont-extract-links")
                .num_args(0)
                .help_heading("Scan settings")
                .help("Don't extract links from response body (html, javascript, etc...)")
        )
        .arg(
            Arg::new("scan_limit")
                .short('L')
                .long("scan-limit")
                .value_name("SCAN_LIMIT")
                .num_args(1)
                .help_heading("Scan settings")
                .help("Limit total number of concurrent scans (default: 0, i.e. no limit)")
        )
        .arg(
            Arg::new("parallel")
                .long("parallel")
                .value_name("PARALLEL_SCANS")
                .conflicts_with("verbosity")
                .conflicts_with("url")
                .num_args(1)
                .requires("stdin")
                .help_heading("Scan settings")
                .help("Run parallel feroxbuster instances (one child process per url passed via stdin)")
        )
        .arg(
            Arg::new("rate_limit")
                .long("rate-limit")
                .value_name("RATE_LIMIT")
                .num_args(1)
                .conflicts_with("auto_tune")
                .help_heading("Scan settings")
                .help("Limit number of requests per second (per directory) (default: 0, i.e. no limit)")
        )
        .arg(
            Arg::new("time_limit")
                .long("time-limit")
                .value_name("TIME_SPEC")
                .num_args(1)
                .value_parser(valid_time_spec)
                .help_heading("Scan settings")
                .help("Limit total run time of all scans (ex: --time-limit 10m)")
        )
        .arg(
            Arg::new("wordlist")
                .short('w')
                .long("wordlist")
                .value_hint(ValueHint::FilePath)
                .value_name("FILE")
                .help("Path or URL of the wordlist")
                .help_heading("Scan settings")
                .num_args(1),
        ).arg(
            Arg::new("auto_tune")
                .long("auto-tune")
                .num_args(0)
                .conflicts_with("auto_bail")
                .help_heading("Scan settings")
                .help("Automatically lower scan rate when an excessive amount of errors are encountered")
        )
        .arg(
            Arg::new("auto_bail")
                .long("auto-bail")
                .num_args(0)
                .help_heading("Scan settings")
                .help("Automatically stop scanning when an excessive amount of errors are encountered")
        ).arg(
            Arg::new("dont_filter")
                .short('D')
                .long("dont-filter")
                .num_args(0)
                .help_heading("Scan settings")
                .help("Don't auto-filter wildcard responses")
        ).arg(
            Arg::new("collect_extensions")
                .short('E')
                .long("collect-extensions")
                .num_args(0)
                .help_heading("Dynamic collection settings")
                .help("Automatically discover extensions and add them to --extensions (unless they're in --dont-collect)")
        ).arg(
            Arg::new("collect_backups")
                .short('B')
                .long("collect-backups")
                .num_args(0..)
                .help_heading("Dynamic collection settings")
                .help("Automatically request likely backup extensions for \"found\" urls (default: ~, .bak, .bak2, .old, .1)")
        )
        .arg(
            Arg::new("collect_words")
                .short('g')
                .long("collect-words")
                .num_args(0)
                .help_heading("Dynamic collection settings")
                .help("Automatically discover important words from within responses and add them to the wordlist")
        ).arg(
            Arg::new("dont_collect")
                .short('I')
                .long("dont-collect")
                .value_name("FILE_EXTENSION")
                .num_args(1..)
                .action(ArgAction::Append)
                .use_value_delimiter(true)
                .help_heading("Dynamic collection settings")
                .help(
                    "File extension(s) to Ignore while collecting extensions (only used with --collect-extensions)",
                ),
        ).arg(
            Arg::new("scan_dir_listings")
                .long("scan-dir-listings")
                .num_args(0)
                .help_heading("Scan settings")
                .help("Force scans to recurse into directory listings")
        );

    /////////////////////////////////////////////////////////////////////
    // group - output settings
    /////////////////////////////////////////////////////////////////////
    let app = app
        .arg(
            Arg::new("verbosity")
                .short('v')
                .long("verbosity")
                .num_args(0)
                .action(ArgAction::Count)
                .conflicts_with("silent")
                .help_heading("Output settings")
                .help("Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v's is probably too much)"),
        ).arg(
            Arg::new("silent")
                .long("silent")
                .num_args(0)
                .conflicts_with("quiet")
                .help_heading("Output settings")
                .help("Only print URLs (or JSON w/ --json) + turn off logging (good for piping a list of urls to other commands)")
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .num_args(0)
                .help_heading("Output settings")
                .help("Hide progress bars and banner (good for tmux windows w/ notifications)")
        )

        .arg(
            Arg::new("json")
                .long("json")
                .num_args(0)
                .requires("output_files")
                .help_heading("Output settings")
                .help("Emit JSON logs to --output and --debug-log instead of normal text")
        ).arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_hint(ValueHint::FilePath)
                .value_name("FILE")
                .help_heading("Output settings")
                .help("Output file to write results to (use w/ --json for JSON entries)")
                .num_args(1),
        )
        .arg(
            Arg::new("debug_log")
                .long("debug-log")
                .value_name("FILE")
                .value_hint(ValueHint::FilePath)
                .help_heading("Output settings")
                .help("Output file to write log entries (use w/ --json for JSON entries)")
                .num_args(1),
        )
        .arg(
            Arg::new("no_state")
                .long("no-state")
                .num_args(0)
                .help_heading("Output settings")
                .help("Disable state output file (*.state)")
        ).arg(
            Arg::new("limit_bars")
                .long("limit-bars")
                .value_name("NUM_BARS_TO_SHOW")
                .num_args(1)
                .help_heading("Output settings")
                .help("Number of directory scan bars to show at any given time (default: no limit)"),
        );

    /////////////////////////////////////////////////////////////////////
    // group - miscellaneous
    /////////////////////////////////////////////////////////////////////
    let mut app = app
        .group(
            ArgGroup::new("output_files")
                .args(["debug_log", "output", "silent"])
                .multiple(true),
        )
        .group(
            ArgGroup::new("output_limiters")
                .args(["quiet", "silent"])
                .multiple(false),
        )
        .arg(
            Arg::new("update_app")
                .short('U')
                .long("update")
                .exclusive(true)
                .num_args(0)
                .help_heading("Update settings")
                .help("Update feroxbuster to the latest version"),
        )
        .after_long_help(EPILOGUE);

    /////////////////////////////////////////////////////////////////////
    // end parser
    /////////////////////////////////////////////////////////////////////
    for arg in env::args() {
        // secure-77 noticed that when an incorrect flag/option is used, the short help message is printed
        // which is fine, but if you add -h|--help, it still errors out on the bad flag/option,
        // never showing the full help message. This code addresses that behavior
        if arg == "--help" {
            app.print_long_help().unwrap();
            println!(); // just a newline to mirror original --help output
            process::exit(0);
        } else if arg == "-h" {
            // same for -h, just shorter
            app.print_help().unwrap();
            println!();
            process::exit(0);
        }
    }

    app
}

/// Validate that a string is formatted as a number followed by s, m, h, or d (10d, 30s, etc...)
fn valid_time_spec(time_spec: &str) -> Result<String, String> {
    match TIMESPEC_REGEX.is_match(time_spec) {
        true => Ok(time_spec.to_string()),
        false => {
            let msg = format!(
                "Expected a non-negative, whole number followed by s, m, h, or d (case insensitive); received {time_spec}"
            );
            Err(msg)
        }
    }
}

const EPILOGUE: &str = r#"NOTE:
    Options that take multiple values are very flexible.  Consider the following ways of specifying
    extensions:
        ./feroxbuster -u http://127.1 -x pdf -x js,html -x php txt json,docx

    The command above adds .pdf, .js, .html, .php, .txt, .json, and .docx to each url

    All of the methods above (multiple flags, space separated, comma separated, etc...) are valid
    and interchangeable.  The same goes for urls, headers, status codes, queries, and size filters.

EXAMPLES:
    Multiple headers:
        ./feroxbuster -u http://127.1 -H Accept:application/json "Authorization: Bearer {token}"

    IPv6, non-recursive scan with INFO-level logging enabled:
        ./feroxbuster -u http://[::1] --no-recursion -vv

    Read urls from STDIN; pipe only resulting urls out to another tool
        cat targets | ./feroxbuster --stdin --silent -s 200 301 302 --redirects -x js | fff -s 200 -o js-files

    Proxy traffic through Burp
        ./feroxbuster -u http://127.1 --burp

    Proxy traffic through a SOCKS proxy
        ./feroxbuster -u http://127.1 --proxy socks5://127.0.0.1:9050

    Pass auth token via query parameter
        ./feroxbuster -u http://127.1 --query token=0123456789ABCDEF

    Ludicrous speed... go!
        ./feroxbuster -u http://127.1 --threads 200
        
    Limit to a total of 60 active requests at any given time (threads * scan limit)
        ./feroxbuster -u http://127.1 --threads 30 --scan-limit 2
    
    Send all 200/302 responses to a proxy (only proxy requests/responses you care about)
        ./feroxbuster -u http://127.1 --replay-proxy http://localhost:8080 --replay-codes 200 302 --insecure
        
    Abort or reduce scan speed to individual directory scans when too many errors have occurred
        ./feroxbuster -u http://127.1 --auto-bail
        ./feroxbuster -u http://127.1 --auto-tune
        
    Examples and demonstrations of all features
        https://epi052.github.io/feroxbuster-docs/docs/examples/
    "#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// initialize parser, expect a clap::App returned
    fn parser_initialize_gives_defaults() {
        let app = initialize();
        assert_eq!(app.get_name(), "feroxbuster");
    }

    #[test]
    /// sanity checks that valid_time_spec correctly checks and rejects a given string
    ///
    /// instead of having a bunch of single tests here, they're all quick and are mostly checking
    /// that i didn't hose up the regex.  Going to consolidate them into a single test
    fn validate_valid_time_spec_validation() {
        let float_rejected = "1.4m";
        assert!(valid_time_spec(float_rejected).is_err());

        let negative_rejected = "-1m";
        assert!(valid_time_spec(negative_rejected).is_err());

        let only_number_rejected = "1";
        assert!(valid_time_spec(only_number_rejected).is_err());

        let only_measurement_rejected = "m";
        assert!(valid_time_spec(only_measurement_rejected).is_err());

        for accepted_measurement in &["s", "m", "h", "d", "S", "M", "H", "D"] {
            // all upper/lowercase should be good
            assert!(valid_time_spec(&format!("1{}", *accepted_measurement)).is_ok());
        }

        let leading_space_rejected = " 14m";
        assert!(valid_time_spec(leading_space_rejected).is_err());

        let trailing_space_rejected = "14m ";
        assert!(valid_time_spec(trailing_space_rejected).is_err());

        let space_between_rejected = "1 4m";
        assert!(valid_time_spec(space_between_rejected).is_err());
    }
}
