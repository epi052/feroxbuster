use ansi_term::Color::{Blue, Cyan, Green, Red, Yellow};
use console::{strip_ansi_codes, user_attended};
use indicatif::ProgressBar;
use reqwest::Url;
use std::convert::TryInto;
use crate::FeroxResult;

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

    let target = if !target.ends_with('/') {
        // target url doesn't end with a /, for the purposes of determining depth, we'll normalize
        // all urls to end in a / and then calculate accordingly
        format!("{}/", target)
    } else {
        String::from(target)
    };

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
        Some('1') => Blue.paint(status).to_string(), // informational
        Some('2') => Green.bold().paint(status).to_string(), // success
        Some('3') => Yellow.paint(status).to_string(), // redirects
        Some('4') => Red.paint(status).to_string(),  // client error
        Some('5') => Red.paint(status).to_string(),  // server error
        Some('W') => Cyan.paint(status).to_string(), // wildcard
        Some('E') => Red.paint(status).to_string(),  // wildcard
        _ => status.to_string(),                     // ¯\_(ツ)_/¯
    }
}

/// Gets the length of a url's path
///
/// example: http://localhost/stuff -> 5
pub fn get_url_path_length(url: &Url) -> u64 {
    log::trace!("enter: get_url_path_length({})", url);

    let path = url.path();

    let segments = if path.starts_with('/') {
        path[1..].split_terminator('/')
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

/// Simple helper to generate a `Url`
///
/// Errors during parsing `url` or joining `word` are propagated up the call stack
pub fn format_url(
    url: &str,
    word: &str,
    addslash: bool,
    queries: &[(String, String)],
    extension: Option<&str>,
) -> FeroxResult<Url> {
    log::trace!(
        "enter: format_url({}, {}, {}, {:?} {:?})",
        url,
        word,
        addslash,
        queries,
        extension
    );

    // from reqwest::Url::join
    //   Note: a trailing slash is significant. Without it, the last path component
    //   is considered to be a “file” name to be removed to get at the “directory”
    //   that is used as the base
    //
    // the transforms that occur here will need to keep this in mind, i.e. add a slash to preserve
    // the current directory sent as part of the url
    let url = if !url.ends_with('/') {
        format!("{}/", url)
    } else {
        url.to_string()
    };

    let base_url = reqwest::Url::parse(&url)?;

    // extensions and slashes are mutually exclusive cases
    let word = if extension.is_some() {
        format!("{}.{}", word, extension.unwrap())
    } else if addslash && !word.ends_with('/') {
        // -f used, and word doesn't already end with a /
        format!("{}/", word)
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
            log::trace!("exit: format_url -> {}", e);
            log::error!("Could not join {} with {}", word, base_url);
            Err(Box::new(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_returns_1() {
        let depth = get_current_depth("http://localhost");
        assert_eq!(depth, 1);
    }

    #[test]
    fn base_url_with_slash_returns_1() {
        let depth = get_current_depth("http://localhost/");
        assert_eq!(depth, 1);
    }

    #[test]
    fn one_dir_returns_2() {
        let depth = get_current_depth("http://localhost/src");
        assert_eq!(depth, 2);
    }

    #[test]
    fn one_dir_with_slash_returns_2() {
        let depth = get_current_depth("http://localhost/src/");
        assert_eq!(depth, 2);
    }

    #[test]
    fn test_format_url_normal() {
        assert_eq!(
            format_url("http://localhost", "stuff", false, &Vec::new(), None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_no_word() {
        assert_eq!(
            format_url("http://localhost", "", false, &Vec::new(), None).unwrap(),
            reqwest::Url::parse("http://localhost").unwrap()
        );
    }

    #[test]
    #[should_panic]
    fn test_format_url_no_url() {
        format_url("", "stuff", false, &Vec::new(), None).unwrap();
    }

    #[test]
    fn test_format_url_word_with_preslash() {
        assert_eq!(
            format_url("http://localhost", "/stuff", false, &Vec::new(), None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_word_with_postslash() {
        assert_eq!(
            format_url("http://localhost", "stuff/", false, &Vec::new(), None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff/").unwrap()
        );
    }

}
