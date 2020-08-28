use clap::{value_t, App, Arg, ArgMatches};
use reqwest::{redirect::Policy, Client, StatusCode};
use std::time::Duration;
use lazy_static::lazy_static;


// todo!("update to use Kali's default install location for seclists");
/// Default wordlist to use when `-w|--wordlist` isn't specified
///
/// defaults to kali's default install location of
/// `seclists/Discovery/Web-Content/raft-medium-directories.txt`
pub const DEFAULT_WORDLIST: &str =
    "/wordlists/seclists/Discovery/Web-Content/raft-medium-directories.txt";

/// Default list of response codes to report, very similar to Gobuster's defaults
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

lazy_static! {
    // Global configuration variable.
    pub static ref CONFIGURATION: Configuration = Configuration::new();
}

/// Represents the final, global configuration of the program.
/// This struct is the combination of
/// - default configuration values
/// - command-line options
#[derive(Debug, Clone)]
pub struct Configuration {
    pub wordlist: String,
    pub target_url: String,
    pub extensions: Vec<String>,
    pub client: Client,
    pub threads: usize,
}

impl Default for Configuration {
    fn default() -> Self {
        let client = Client::builder()
            .timeout(Duration::new(5, 0))
            .redirect(Policy::none())
            .build()
            .unwrap();

        Configuration {
            wordlist: String::from(DEFAULT_WORDLIST),
            target_url: String::new(),
            extensions: vec![],
            threads: 50,
            client,
        }
    }
}

impl Configuration {
    pub fn new() -> Self {
        // Get the default configuration, this is what will apply if nothing
        // else is specified.
        let mut config = Configuration::default();

        let args = Self::parse_args();
        let threads = value_t!(args.value_of("threads"), usize).unwrap_or_else(|e| e.exit());

        config.target_url = String::from(args.value_of("url").unwrap());
        config.wordlist = String::from(args.value_of("wordlist").unwrap());
        config.threads = threads;

        config
    }

    fn parse_args() -> ArgMatches<'static> {
        // todo!("update about section with an actual description");
        // todo!("add timeout option");
        // todo!("add proxy option");
        // todo!("add ignore certs option");
        // todo!("add headers option");
        // todo!("add user-agent option");
        // todo!("add redirect/no-redirect? option");

        App::new("feroxbuster-bak")
            .version("0.0.1")
            .author("epi <epibar052@gmail.com>")
            .about("A fast, simple, recursive content discovery tool written in Rust")
            .arg(
                Arg::with_name("wordlist")
                    .short("w")
                    .long("wordlist")
                    .value_name("FILE")
                    .help("Path to the wordlist")
                    .takes_value(true)
                    .default_value(DEFAULT_WORDLIST),
            )
            .arg(
                Arg::with_name("url")
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
                    .default_value("50")
                    .help("Number of concurrent threads (default: 50)"),
            )
            .get_matches()
    }
}
