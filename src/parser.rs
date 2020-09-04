use crate::VERSION;
use clap::{App, Arg};

/// Create and return an instance of `clap::App`, i.e. the Command Line Interface's configuration
pub fn initialize() -> App<'static, 'static> {
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
        .arg(
            Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .takes_value(false)
                .help("Only print URLs; Don't print status codes, response size, running config, etc...")
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("Output file to write results to (default: stdout)")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("useragent")
                .short("a")
                .long("useragent")
                .takes_value(true)
                .help(
                    "Sets the User-Agent (default: feroxbuster/VERSION)"
                ),
        )
        .arg(
            Arg::with_name("follow_redirects")
                .short("r")
                .long("follow_redirects")
                .takes_value(false)
                .help("Follow redirects (default: false)")
        )
        .arg(
            Arg::with_name("insecure")
                .short("k")
                .long("insecure")
                .takes_value(false)
                .help("Disables TLS certificate validation (default: false)")
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
                    "File extension(s) to search for (accepts multi-flag and space or comma-delimited: -x php -x pdf js)",
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
                    "Specify HTTP headers, -H 'Header1: val1' -H 'Header2: val2'",
                ),
        )
        .arg(
            Arg::with_name("norecursion")
                .short("n")
                .long("norecursion")
                .takes_value(false)
                .help("Do not scan recursively (default: scan recursively)")
        )
        .arg(
            Arg::with_name("addslash")
                .short("f")
                .long("addslash")
                .takes_value(false)
                .help("Append / to each request (default: false)")
        )
}
