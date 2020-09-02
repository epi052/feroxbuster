use crate::config::VERSION;
use clap::{App, Arg};

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
                .help("Output file to write results to (defaults to stdout)")
                .takes_value(true),
        )
}
