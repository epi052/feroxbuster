#![macro_use]
use crate::{
    config::{CONFIGURATION, PROGRESS_PRINTER},
    statistics::{
        StatCommand::{self, AddError, AddStatus},
        StatError::{Connection, Other, Redirection, Request, Timeout, UrlFormat},
    },
    FeroxError, FeroxResult,
};
use console::{strip_ansi_codes, style, user_attended};
use indicatif::ProgressBar;
use reqwest::{Client, Response, Url};
#[cfg(not(target_os = "windows"))]
use rlimit::{getrlimit, setrlimit, Resource, Rlim};
use std::convert::TryInto;
use std::sync::{Arc, RwLock};
use std::{fs, io};
use tokio::sync::mpsc::UnboundedSender;

/// Given the path to a file, open the file in append mode (create it if it doesn't exist) and
/// return a reference to the file that is buffered and locked
pub fn open_file(filename: &str) -> Option<Arc<RwLock<io::BufWriter<fs::File>>>> {
    log::trace!("enter: open_file({})", filename);

    match fs::OpenOptions::new() // std fs
        .create(true)
        .append(true)
        .open(filename)
    {
        Ok(file) => {
            let writer = io::BufWriter::new(file); // std io

            let locked_file = Some(Arc::new(RwLock::new(writer)));

            log::trace!("exit: open_file -> {:?}", locked_file);
            locked_file
        }
        Err(e) => {
            log::error!("{}", e);
            log::trace!("exit: open_file -> None");
            None
        }
    }
}

/// Helper function that determines the current depth of a given url
///
/// Essentially looks at the Url path and determines how many directories are present in the
/// given Url
///
/// http://localhost -> 1
/// http://localhost/ -> 1
/// http://localhost/stuff -> 2
/// ...
///
/// returns 0 on error and relative urls
pub fn get_current_depth(target: &str) -> usize {
    log::trace!("enter: get_current_depth({})", target);

    let target = normalize_url(target);

    match Url::parse(&target) {
        Ok(url) => {
            if let Some(parts) = url.path_segments() {
                // at least an empty string returned by the Split, meaning top-level urls
                let mut depth = 0;

                for _ in parts {
                    depth += 1;
                }

                let return_val = depth;

                log::trace!("exit: get_current_depth -> {}", return_val);
                return return_val;
            };

            log::debug!(
                "get_current_depth called on a Url that cannot be a base: {}",
                url
            );
            log::trace!("exit: get_current_depth -> 0");

            0
        }
        Err(e) => {
            log::error!("could not parse to url: {}", e);
            log::trace!("exit: get_current_depth -> 0");
            0
        }
    }
}

/// Takes in a string and examines the first character to return a color version of the same string
pub fn status_colorizer(status: &str) -> String {
    match status.chars().next() {
        Some('1') => style(status).blue().to_string(), // informational
        Some('2') => style(status).green().to_string(), // success
        Some('3') => style(status).yellow().to_string(), // redirects
        Some('4') => style(status).red().to_string(),  // client error
        Some('5') => style(status).red().to_string(),  // server error
        Some('W') => style(status).cyan().to_string(), // wildcard
        Some('E') => style(status).red().to_string(),  // error
        _ => status.to_string(),                       // ¯\_(ツ)_/¯
    }
}

/// Takes in a string and colors it using console::style
///
/// mainly putting this here in case i want to change the color later, making any changes easy
pub fn module_colorizer(modname: &str) -> String {
    style(modname).cyan().to_string()
}

/// Gets the length of a url's path
///
/// example: http://localhost/stuff -> 5
pub fn get_url_path_length(url: &Url) -> u64 {
    log::trace!("enter: get_url_path_length({})", url);

    let path = url.path();

    let segments = if let Some(split) = path.strip_prefix('/') {
        split.split_terminator('/')
    } else {
        log::trace!("exit: get_url_path_length -> 0");
        return 0;
    };

    if let Some(last) = segments.last() {
        // failure on conversion should be very unlikely. While a usize can absolutely overflow a
        // u64, the generally accepted maximum for the length of a url is ~2000.  so the value we're
        // putting into the u64 should never realistically be anywhere close to producing an
        // overflow.
        // usize max: 18,446,744,073,709,551,615
        // u64 max:   9,223,372,036,854,775,807
        let url_len: u64 = last
            .len()
            .try_into()
            .expect("Failed usize -> u64 conversion");

        log::trace!("exit: get_url_path_length -> {}", url_len);
        return url_len;
    }

    log::trace!("exit: get_url_path_length -> 0");
    0
}

/// Simple helper to abstract away the check for an attached terminal.
///
/// If a terminal is attached, progress bars are visible and the progress bar is used to print
/// to stderr. The progress bar must be used when bars are visible in order to not jack up any
/// progress bar output (the bar knows how to print above itself)
///
/// If a terminal is not attached, `msg` is printed to stdout, with its ansi
/// color codes stripped.
///
/// additionally, provides a location for future printing options (no color, etc) to be handled
pub fn ferox_print(msg: &str, bar: &ProgressBar) {
    if user_attended() {
        bar.println(msg);
    } else {
        let stripped = strip_ansi_codes(msg);
        println!("{}", stripped);
    }
}

#[macro_export]
/// wrapper to improve code readability
macro_rules! update_stat {
    ($tx:expr, $value:expr) => {
        $tx.send($value).unwrap_or_default();
    };
}

/// Simple helper to generate a `Url`
///
/// Errors during parsing `url` or joining `word` are propagated up the call stack
pub fn format_url(
    url: &str,
    word: &str,
    add_slash: bool,
    queries: &[(String, String)],
    extension: Option<&str>,
    tx_stats: UnboundedSender<StatCommand>,
) -> FeroxResult<Url> {
    log::trace!(
        "enter: format_url({}, {}, {}, {:?} {:?}, {:?})",
        url,
        word,
        add_slash,
        queries,
        extension,
        tx_stats
    );

    if Url::parse(&word).is_ok() {
        // when a full url is passed in as a word to be joined to a base url using
        // reqwest::Url::join, the result is that the word (url) completely overwrites the base
        // url, potentially resulting in requests to places that aren't actually the target
        // specified.
        //
        // in order to resolve the issue, we check if the word from the wordlist is a parsable URL
        // and if so, don't do any further processing
        let message = format!(
            "word ({}) from the wordlist is actually a URL, skipping...",
            word
        );
        log::warn!("{}", message);

        let err = FeroxError { message };

        update_stat!(tx_stats, AddError(UrlFormat));

        log::trace!("exit: format_url -> {}", err);
        return Err(Box::new(err));
    }

    // from reqwest::Url::join
    //   Note: a trailing slash is significant. Without it, the last path component
    //   is considered to be a “file” name to be removed to get at the “directory”
    //   that is used as the base
    //
    // the transforms that occur here will need to keep this in mind, i.e. add a slash to preserve
    // the current directory sent as part of the url
    let url = if word.is_empty() {
        // v1.0.6: added during --extract-links feature implementation to support creating urls
        // that were extracted from response bodies, i.e. http://localhost/some/path/js/main.js
        url.to_string()
    } else if !url.ends_with('/') {
        format!("{}/", url)
    } else {
        url.to_string()
    };

    let base_url = reqwest::Url::parse(&url)?;

    // extensions and slashes are mutually exclusive cases
    let word = if extension.is_some() {
        format!("{}.{}", word, extension.unwrap())
    } else if add_slash && !word.ends_with('/') {
        // -f used, and word doesn't already end with a /
        format!("{}/", word)
    } else if word.starts_with("//") {
        // bug ID'd by @Sicks3c, when a wordlist contains words that begin with 2 forward slashes
        // i.e. //1_40_0/static/js, it gets joined onto the base url in a surprising way
        // ex: https://localhost/ + //1_40_0/static/js -> https://1_40_0/static/js
        // this is due to the fact that //... is a valid url. The fix is introduced here in 1.12.2
        // and simply removes prefixed forward slashes if there are two of them. Additionally,
        // trim_start_matches will trim the pattern until it's gone, so even if there are more than
        // 2 /'s, they'll still be trimmed
        word.trim_start_matches('/').to_string()
    } else {
        String::from(word)
    };

    match base_url.join(&word) {
        Ok(request) => {
            if queries.is_empty() {
                // no query params to process
                log::trace!("exit: format_url -> {}", request);
                Ok(request)
            } else {
                match reqwest::Url::parse_with_params(request.as_str(), queries) {
                    Ok(req_w_params) => {
                        log::trace!("exit: format_url -> {}", req_w_params);
                        Ok(req_w_params) // request with params attached
                    }
                    Err(e) => {
                        log::error!(
                            "Could not add query params {:?} to {}: {}",
                            queries,
                            request,
                            e
                        );
                        log::trace!("exit: format_url -> {}", request);
                        Ok(request) // couldn't process params, return initially ok url
                    }
                }
            }
        }
        Err(e) => {
            update_stat!(tx_stats, AddError(UrlFormat));
            log::trace!("exit: format_url -> {}", e);
            log::error!("Could not join {} with {}", word, base_url);
            Err(Box::new(e))
        }
    }
}

/// Initiate request to the given `Url` using `Client`
pub async fn make_request(
    client: &Client,
    url: &Url,
    tx_stats: UnboundedSender<StatCommand>,
) -> FeroxResult<Response> {
    log::trace!(
        "enter: make_request(CONFIGURATION.Client, {}, {:?})",
        url,
        tx_stats
    );

    match client.get(url.to_owned()).send().await {
        Err(e) => {
            let mut log_level = log::Level::Error;

            log::trace!("exit: make_request -> {}", e);
            if e.is_timeout() {
                // only warn for timeouts, while actual errors are still left as errors
                log_level = log::Level::Warn;
                update_stat!(tx_stats, AddError(Timeout));
            } else if e.is_redirect() {
                if let Some(last_redirect) = e.url() {
                    // get where we were headed (last_redirect) and where we came from (url)
                    let fancy_message = format!("{} !=> {}", url, last_redirect);

                    let report = if let Some(msg_status) = e.status() {
                        update_stat!(tx_stats, AddStatus(msg_status));
                        create_report_string(msg_status.as_str(), "-1", "-1", "-1", &fancy_message)
                    } else {
                        create_report_string("UNK", "-1", "-1", "-1", &fancy_message)
                    };

                    update_stat!(tx_stats, AddError(Redirection));

                    ferox_print(&report, &PROGRESS_PRINTER)
                };
            } else if e.is_connect() {
                update_stat!(tx_stats, AddError(Connection));
            } else if e.is_request() {
                update_stat!(tx_stats, AddError(Request));
            } else {
                update_stat!(tx_stats, AddError(Other));
            }

            if matches!(log_level, log::Level::Error) {
                log::error!("Error while making request: {}", e);
            } else {
                log::warn!("Error while making request: {}", e);
            }

            Err(Box::new(e))
        }
        Ok(resp) => {
            log::trace!("exit: make_request -> {:?}", resp);
            update_stat!(tx_stats, AddStatus(resp.status()));
            Ok(resp)
        }
    }
}

/// Helper to create the standard line for output to file/terminal
///
/// example output:
/// 200      127l      283w     4134c http://localhost/faq
pub fn create_report_string(
    status: &str,
    line_count: &str,
    word_count: &str,
    content_length: &str,
    url: &str,
) -> String {
    if CONFIGURATION.quiet {
        // -q used, just need the url
        format!("{}\n", url)
    } else {
        // normal printing with status and sizes
        let color_status = status_colorizer(status);
        format!(
            "{} {:>8}l {:>8}w {:>8}c {}\n",
            color_status, line_count, word_count, content_length, url
        )
    }
}

/// Attempts to set the soft limit for the RLIMIT_NOFILE resource
///
/// RLIMIT_NOFILE is the maximum number of file descriptors that can be opened by this process
///
/// The soft limit is the value that the kernel enforces for the corresponding resource.
/// The hard limit acts as a ceiling for the soft limit: an unprivileged process may set only its
/// soft limit to a value in the range from 0 up to the hard limit, and (irreversibly) lower its
/// hard limit.
///
/// A child process created via fork(2) inherits its parent's resource limits. Resource limits are
/// per-process attributes that are shared by all of the threads in a process.
///
/// Based on the above information, no attempt is made to restore the limit to its pre-scan value
/// as the adjustment made here is only valid for the scan itself (and any child processes, of which
/// there are none).
#[cfg(not(target_os = "windows"))]
pub fn set_open_file_limit(limit: usize) -> bool {
    log::trace!("enter: set_open_file_limit");

    if let Ok((soft, hard)) = getrlimit(Resource::NOFILE) {
        if hard.as_usize() > limit {
            // our default open file limit is less than the current hard limit, this means we can
            // set the soft limit to our default
            let new_soft_limit = Rlim::from_usize(limit);

            if setrlimit(Resource::NOFILE, new_soft_limit, hard).is_ok() {
                log::debug!("set open file descriptor limit to {}", limit);

                log::trace!("exit: set_open_file_limit -> {}", true);
                return true;
            }
        } else if soft != hard {
            // hard limit is lower than our default, the next best option is to set the soft limit as
            // high as the hard limit will allow
            if setrlimit(Resource::NOFILE, hard, hard).is_ok() {
                log::debug!("set open file descriptor limit to {}", limit);

                log::trace!("exit: set_open_file_limit -> {}", true);
                return true;
            }
        }
    }

    // failed to set a new limit, as limit adjustments are a 'nice to have', we'll just log
    // and move along
    log::warn!("could not set open file descriptor limit to {}", limit);

    log::trace!("exit: set_open_file_limit -> {}", false);
    false
}

/// Simple helper to abstract away adding a forward-slash to a url if not present
///
/// used mostly for deduplication purposes and url state tracking
pub fn normalize_url(url: &str) -> String {
    log::trace!("enter: normalize_url({})", url);

    let normalized = if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{}/", url)
    };

    log::trace!("exit: normalize_url -> {}", normalized);
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeroxChannel;
    use tokio::sync::mpsc;

    #[test]
    /// set_open_file_limit with a low requested limit succeeds
    fn utils_set_open_file_limit_with_low_requested_limit() {
        let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
        let lower_limit = hard.as_usize() - 1;
        assert!(set_open_file_limit(lower_limit));
    }

    #[test]
    /// set_open_file_limit with a high requested limit succeeds
    fn utils_set_open_file_limit_with_high_requested_limit() {
        let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
        let higher_limit = hard.as_usize() + 1;
        // calculate a new soft to ensure soft != hard and hit that logic branch
        let new_soft = Rlim::from_usize(hard.as_usize() - 1);
        setrlimit(Resource::NOFILE, new_soft, hard).unwrap();
        assert!(set_open_file_limit(higher_limit));
    }

    #[test]
    /// set_open_file_limit should fail when hard == soft
    fn utils_set_open_file_limit_with_fails_when_both_limits_are_equal() {
        let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
        // calculate a new soft to ensure soft == hard and hit the failure logic branch
        setrlimit(Resource::NOFILE, hard, hard).unwrap();
        assert!(!set_open_file_limit(hard.as_usize())); // returns false
    }

    #[test]
    /// base url returns 1
    fn get_current_depth_base_url_returns_1() {
        let depth = get_current_depth("http://localhost");
        assert_eq!(depth, 1);
    }

    #[test]
    /// base url with slash returns 1
    fn get_current_depth_base_url_with_slash_returns_1() {
        let depth = get_current_depth("http://localhost/");
        assert_eq!(depth, 1);
    }

    #[test]
    /// base url + 1 dir returns 2
    fn get_current_depth_one_dir_returns_2() {
        let depth = get_current_depth("http://localhost/src");
        assert_eq!(depth, 2);
    }

    #[test]
    /// base url + 1 dir and slash returns 2
    fn get_current_depth_one_dir_with_slash_returns_2() {
        let depth = get_current_depth("http://localhost/src/");
        assert_eq!(depth, 2);
    }

    #[test]
    /// base url + 1 dir and slash returns 2
    fn get_current_depth_single_forward_slash_is_zero() {
        let depth = get_current_depth("");
        assert_eq!(depth, 0);
    }

    #[test]
    /// base url + 1 word + no slash + no extension
    fn format_url_normal() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        assert_eq!(
            format_url("http://localhost", "stuff", false, &Vec::new(), None, tx).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    /// base url + no word + no slash + no extension
    fn format_url_no_word() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        assert_eq!(
            format_url("http://localhost", "", false, &Vec::new(), None, tx).unwrap(),
            reqwest::Url::parse("http://localhost").unwrap()
        );
    }

    #[test]
    /// base url + word + no slash + no extension + queries
    fn format_url_joins_queries() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        assert_eq!(
            format_url(
                "http://localhost",
                "lazer",
                false,
                &[(String::from("stuff"), String::from("things"))],
                None,
                tx
            )
            .unwrap(),
            reqwest::Url::parse("http://localhost/lazer?stuff=things").unwrap()
        );
    }

    #[test]
    /// base url + no word + no slash + no extension + queries
    fn format_url_without_word_joins_queries() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        assert_eq!(
            format_url(
                "http://localhost",
                "",
                false,
                &[(String::from("stuff"), String::from("things"))],
                None,
                tx
            )
            .unwrap(),
            reqwest::Url::parse("http://localhost/?stuff=things").unwrap()
        );
    }

    #[test]
    #[should_panic]
    /// no base url is an error
    fn format_url_no_url() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        format_url("", "stuff", false, &Vec::new(), None, tx).unwrap();
    }

    #[test]
    /// word prepended with slash is adjusted for correctness
    fn format_url_word_with_preslash() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        assert_eq!(
            format_url("http://localhost", "/stuff", false, &Vec::new(), None, tx).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    /// word with appended slash allows the slash to persist
    fn format_url_word_with_postslash() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        assert_eq!(
            format_url("http://localhost", "stuff/", false, &Vec::new(), None, tx).unwrap(),
            reqwest::Url::parse("http://localhost/stuff/").unwrap()
        );
    }

    #[test]
    /// word with two prepended slashes doesn't discard the entire domain
    fn format_url_word_with_two_prepended_slashes() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let result = format_url(
            "http://localhost",
            "//upload/img",
            false,
            &Vec::new(),
            None,
            tx,
        )
        .unwrap();

        assert_eq!(
            result,
            reqwest::Url::parse("http://localhost/upload/img").unwrap()
        );
    }

    #[test]
    /// word that is a fully formed url, should return an error
    fn format_url_word_that_is_a_url() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        let url = format_url(
            "http://localhost",
            "http://schmocalhost",
            false,
            &Vec::new(),
            None,
            tx,
        );
        assert!(url.is_err());
    }

    #[test]
    /// status colorizer uses red for 500s
    fn status_colorizer_uses_red_for_500s() {
        assert_eq!(status_colorizer("500"), style("500").red().to_string());
    }

    #[test]
    /// status colorizer uses red for 400s
    fn status_colorizer_uses_red_for_400s() {
        assert_eq!(status_colorizer("400"), style("400").red().to_string());
    }

    #[test]
    /// status colorizer uses red for errors
    fn status_colorizer_uses_red_for_errors() {
        assert_eq!(status_colorizer("ERROR"), style("ERROR").red().to_string());
    }

    #[test]
    /// status colorizer uses cyan for wildcards
    fn status_colorizer_uses_cyan_for_wildcards() {
        assert_eq!(status_colorizer("WLD"), style("WLD").cyan().to_string());
    }

    #[test]
    /// status colorizer uses blue for 100s
    fn status_colorizer_uses_blue_for_100s() {
        assert_eq!(status_colorizer("100"), style("100").blue().to_string());
    }

    #[test]
    /// status colorizer uses green for 200s
    fn status_colorizer_uses_green_for_200s() {
        assert_eq!(status_colorizer("200"), style("200").green().to_string());
    }

    #[test]
    /// status colorizer uses yellow for 300s
    fn status_colorizer_uses_yellow_for_300s() {
        assert_eq!(status_colorizer("300"), style("300").yellow().to_string());
    }

    #[test]
    /// status colorizer doesnt color anything else
    fn status_colorizer_returns_as_is() {
        assert_eq!(status_colorizer("farfignewton"), "farfignewton".to_string());
    }
}
