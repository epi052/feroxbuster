use clap::{App, Arg, ArgGroup};
use lazy_static::lazy_static;
use regex::Regex;

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
}

/// Create and return an instance of [clap::App](https://docs.rs/clap/latest/clap/struct.App.html), i.e. the Command Line Interface's configuration
pub fn initialize() -> App<'static, 'static> {
    App::new("feroxbuster")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Ben 'epi' Risher (@epi052)")
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
                .required_unless_one(&["stdin", "resume_from"])
                .value_name("URL")
                .multiple(true)
                .use_delimiter(true)
                .help("The target URL(s) (required, unless --stdin used)"),
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
            Arg::with_name("depth")
                .short("d")
                .long("depth")
                .value_name("RECURSION_DEPTH")
                .takes_value(true)
                .help("Maximum recursion depth, a depth of 0 is infinite recursion (default: 4)"),
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
                .help("Increase verbosity level (use -vv or more for greater effect. [CAUTION] 4 -v's is probably too much)"),
        )
        .arg(
            Arg::with_name("proxy")
                .short("p")
                .long("proxy")
                .takes_value(true)
                .value_name("PROXY")
                .help(
                    "Proxy to use for requests (ex: http(s)://host:port, socks5(h)://host:port)",
                ),
        )
        .arg(
            Arg::with_name("replay_proxy")
                .short("P")
                .long("replay-proxy")
                .takes_value(true)
                .value_name("REPLAY_PROXY")
                .help(
                    "Send only unfiltered requests through a Replay Proxy, instead of all requests",
                ),
        )
        .arg(
            Arg::with_name("replay_codes")
                .short("R")
                .long("replay-codes")
                .value_name("REPLAY_CODE")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .requires("replay_proxy")
                .help(
                    "Status Codes to send through a Replay Proxy when found (default: --status-codes value)",
                ),
        )
        .arg(
            Arg::with_name("status_codes")
                .short("s")
                .long("status-codes")
                .value_name("STATUS_CODE")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Status Codes to include (allow list) (default: 200 204 301 302 307 308 401 403 405)",
                ),
        )
        .arg(
            Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .takes_value(false)
                .help("Only print URLs; Don't print status codes, response size, running config, etc...")
        )
        .arg(
            Arg::with_name("json")
                .long("json")
                .takes_value(false)
                .requires("output_files")
                .help("Emit JSON logs to --output and --debug-log instead of normal text")
        )
        .arg(
            Arg::with_name("dont_filter")
                .short("D")
                .long("dont-filter")
                .takes_value(false)
                .help("Don't auto-filter wildcard responses")
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("Output file to write results to (use w/ --json for JSON entries)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("resume_from")
                .long("resume-from")
                .value_name("STATE_FILE")
                .help("State file from which to resume a partially complete scan (ex. --resume-from ferox-1606586780.state)")
                .conflicts_with("url")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("debug_log")
                .long("debug-log")
                .value_name("FILE")
                .help("Output file to write log entries (use w/ --json for JSON entries)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("user_agent")
                .short("a")
                .long("user-agent")
                .value_name("USER_AGENT")
                .takes_value(true)
                .help(
                    "Sets the User-Agent (default: feroxbuster/VERSION)"
                ),
        )
        .arg(
            Arg::with_name("redirects")
                .short("r")
                .long("redirects")
                .takes_value(false)
                .help("Follow redirects")
        )
        .arg(
            Arg::with_name("insecure")
                .short("k")
                .long("insecure")
                .takes_value(false)
                .help("Disables TLS certificate validation")
        )
        .arg(
            Arg::with_name("extensions")
                .short("x")
                .long("extensions")
                .value_name("FILE_EXTENSION")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "File extension(s) to search for (ex: -x php -x pdf js)",
                ),
        )
        .arg(
            Arg::with_name("headers")
                .short("H")
                .long("headers")
                .value_name("HEADER")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Specify HTTP headers (ex: -H Header:val 'stuff: things')",
                ),
        )
        .arg(
            Arg::with_name("queries")
                .short("Q")
                .long("query")
                .value_name("QUERY")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Specify URL query parameters (ex: -Q token=stuff -Q secret=key)",
                ),
        )
        .arg(
            Arg::with_name("no_recursion")
                .short("n")
                .long("no-recursion")
                .takes_value(false)
                .help("Do not scan recursively")
        )
        .arg(
            Arg::with_name("add_slash")
                .short("f")
                .long("add-slash")
                .takes_value(false)
                .conflicts_with("extensions")
                .help("Append / to each request")
        )
        .arg(
            Arg::with_name("stdin")
                .long("stdin")
                .takes_value(false)
                .help("Read url(s) from STDIN")
                .conflicts_with("url")
        )
        .arg(
            Arg::with_name("filter_size")
                .short("S")
                .long("filter-size")
                .value_name("SIZE")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Filter out messages of a particular size (ex: -S 5120 -S 4927,1970)",
                ),
        )
        .arg(
            Arg::with_name("filter_regex")
                .short("X")
                .long("filter-regex")
                .value_name("REGEX")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Filter out messages via regular expression matching on the response's body (ex: -X '^ignore me$')",
                ),
        )
        .arg(
            Arg::with_name("filter_words")
                .short("W")
                .long("filter-words")
                .value_name("WORDS")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Filter out messages of a particular word count (ex: -W 312 -W 91,82)",
                ),
        )
        .arg(
            Arg::with_name("filter_lines")
                .short("N")
                .long("filter-lines")
                .value_name("LINES")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Filter out messages of a particular line count (ex: -N 20 -N 31,30)",
                ),
        )
        .arg(
            Arg::with_name("filter_status")
                .short("C")
                .long("filter-status")
                .value_name("STATUS_CODE")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Filter out status codes (deny list) (ex: -C 200 -C 401)",
                ),
        )
        .arg(
            Arg::with_name("filter_similar")
                .long("filter-similar-to")
                .value_name("UNWANTED_PAGE")
                .takes_value(true)
                .multiple(true)
                .use_delimiter(true)
                .help(
                    "Filter out pages that are similar to the given page (ex. --filter-similar-to http://site.xyz/soft404)",
                ),
        )
        .arg(
            Arg::with_name("extract_links")
                .short("e")
                .long("extract-links")
                .takes_value(false)
                .help("Extract links from response body (html, javascript, etc...); make new requests based on findings (default: false)")
        )
        .arg(
            Arg::with_name("scan_limit")
                .short("L")
                .long("scan-limit")
                .value_name("SCAN_LIMIT")
                .takes_value(true)
                .help("Limit total number of concurrent scans (default: 0, i.e. no limit)")
        )
        .arg(
            Arg::with_name("time_limit")
                .long("time-limit")
                .value_name("TIME_SPEC")
                .takes_value(true)
                .validator(valid_time_spec)
                .help("Limit total run time of all scans (ex: --time-limit 10m)")
        )
        .group(ArgGroup::with_name("output_files")
            .args(&["debug_log", "output"])
            .multiple(true)
        )
        .after_help(r#"NOTE:
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
        cat targets | ./feroxbuster --stdin --quiet -s 200 301 302 --redirects -x js | fff -s 200 -o js-files

    Proxy traffic through Burp
        ./feroxbuster -u http://127.1 --insecure --proxy http://127.0.0.1:8080

    Proxy traffic through a SOCKS proxy
        ./feroxbuster -u http://127.1 --proxy socks5://127.0.0.1:9050

    Pass auth token via query parameter
        ./feroxbuster -u http://127.1 --query token=0123456789ABCDEF

    Find links in javascript/html and make additional requests based on results
        ./feroxbuster -u http://127.1 --extract-links

    Ludicrous speed... go!
        ./feroxbuster -u http://127.1 -t 200
    "#)
}

/// Validate that a string is formatted as a number followed by s, m, h, or d (10d, 30s, etc...)
fn valid_time_spec(time_spec: String) -> Result<(), String> {
    match TIMESPEC_REGEX.is_match(&time_spec) {
        true => Ok(()),
        false => {
            let msg = format!(
                "Expected a non-negative, whole number followed by s, m, h, or d (case insensitive); received {}",
                time_spec
            );
            Err(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// initalize parser, expect a clap::App returned
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
        assert!(valid_time_spec(float_rejected.into()).is_err());

        let negative_rejected = "-1m";
        assert!(valid_time_spec(negative_rejected.into()).is_err());

        let only_number_rejected = "1";
        assert!(valid_time_spec(only_number_rejected.into()).is_err());

        let only_measurement_rejected = "m";
        assert!(valid_time_spec(only_measurement_rejected.into()).is_err());

        for accepted_measurement in &["s", "m", "h", "d", "S", "M", "H", "D"] {
            // all upper/lowercase should be good
            assert!(valid_time_spec(format!("1{}", *accepted_measurement)).is_ok());
        }

        let leading_space_rejected = " 14m";
        assert!(valid_time_spec(leading_space_rejected.into()).is_err());

        let trailing_space_rejected = "14m ";
        assert!(valid_time_spec(trailing_space_rejected.into()).is_err());

        let space_between_rejected = "1 4m";
        assert!(valid_time_spec(space_between_rejected.into()).is_err());
    }
}
