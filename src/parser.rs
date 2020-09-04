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
                .required_unless("stdin")
                .value_name("URL")
                .help("The target URL (required, unless --stdin used)"),
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
                .value_name("PROXY")
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
            Arg::with_name("norecursion")
                .short("n")
                .long("norecursion")
                .takes_value(false)
                .help("Do not scan recursively")
        )
        .arg(
            Arg::with_name("addslash")
                .short("f")
                .long("addslash")
                .takes_value(false)
                .help("Append / to each request")
        )
        .arg(
            Arg::with_name("stdin")
                .long("stdin")
                .takes_value(false)
                .help("Read url(s) from STDIN")
                .conflicts_with("url")
        )

        .after_help(r#"NOTE:
    Options that take multiple values are very flexible.  Consider the following ways of specifying
    extensions:
        ./feroxbuster -u http://127.1 -x pdf -x js,html -x php txt json,docx

    All of the methods above are valid and interchangeable.  The same goes for headers and status
    codes.

EXAMPLES:
    Multiple headers:
        ./feroxbuster -u http://127.1 -H Accept:application/json "Authorization: Bearer {token}"

    IPv6, non-recursive scan with INFO-level logging enabled:
        ./feroxbuster -u http://[::1] --norecursion -vv

    Read urls from STDIN; pipe only resulting urls out to another tool
        cat targets | ./feroxbuster --stdin -q -s 200 301 302 --redirects -x js | fff -s 200 -o js-files

    Ludicrous speed... go!
        ./feroxbuster -u http://127.1 -t 200
    "#)
}
