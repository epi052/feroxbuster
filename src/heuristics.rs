use crate::config::CONFIGURATION;
use crate::progress;
use crate::scanner::{format_url, make_request};
use crate::utils::{get_url_path_length, status_colorizer};
use reqwest::Response;
use std::process;
use uuid::Uuid;
use indicatif::ProgressBar;

const UUID_LENGTH: u64 = 32;

/// Data holder for two pieces of data needed when auto-filtering out wildcard responses
///
/// `dynamic` is the size of the response that will later be combined with the length
/// of the path of the url requested and used to determine interesting pages from custom
/// 404s where the requested url is reflected back in the response
///
/// `size` is size of the response that should be included with filters passed via runtime
/// configuration and any static wildcard lengths.
#[derive(Default, Debug)]
pub struct WildcardFilter {
    pub dynamic: u64,
    pub size: u64,
}

/// Simple helper to return a uuid, formatted as lowercase without hyphens
///
/// `length` determines the number of uuids to string together. Each uuid
/// is 32 characters long. So, a length of 1 return a 32 character string,
/// a length of 2 returns a 64 character string, and so on...
fn unique_string(length: usize) -> String {
    log::trace!("enter: unique_string({})", length);
    let mut ids = vec![];

    for _ in 0..length {
        ids.push(Uuid::new_v4().to_simple().to_string());
    }

    let unique_id = ids.join("");

    log::trace!("exit: unique_string -> {}", unique_id);
    unique_id
}

/// Tests the given url to see if it issues a wildcard response
///
/// In the event that url returns a wildcard response, a
/// [WildcardFilter](struct.WildcardFilter.html) is created and returned to the caller.
pub async fn wildcard_test(target_url: &str, bar: ProgressBar) -> Option<WildcardFilter> {
    log::trace!("enter: wildcard_test({:?})", target_url);

    if CONFIGURATION.dontfilter {
        // early return, dontfilter scans don't need tested
        log::trace!("exit: wildcard_test -> None");
        return None;
    }

    if let Some(resp_one) = make_wildcard_request(&target_url, 1, bar.clone()).await {
        bar.inc(1);

        // found a wildcard response
        let mut wildcard = WildcardFilter::default();

        let wc_length = resp_one.content_length().unwrap_or(0);

        if wc_length == 0 {
            log::trace!("exit: wildcard_test -> Some({:?})", wildcard);
            return Some(wildcard);
        }

        // content length of wildcard is non-zero, perform additional tests:
        //   make a second request, with a known-sized (64) longer request
        if let Some(resp_two) = make_wildcard_request(&target_url, 3, bar.clone()).await {
            bar.inc(1);

            let wc2_length = resp_two.content_length().unwrap_or(0);

            if wc2_length == wc_length + (UUID_LENGTH * 2) {
                // second length is what we'd expect to see if the requested url is
                // reflected in the response along with some static content; aka custom 404
                let url_len = get_url_path_length(&resp_one.url());

                bar.println(format!(
                    "[{}] - Url is being reflected in wildcard response, i.e. a dynamic wildcard",
                    status_colorizer("WILDCARD")
                ));
                bar.println(
                    format!(
                        "[{}] - Auto-filtering out responses that are [({} + url length) bytes] long; this behavior can be turned off by using --dontfilter",
                        status_colorizer("WILDCARD"),
                        wc_length - url_len,
                    )
                );

                wildcard.dynamic = wc_length - url_len;
            } else if wc_length == wc2_length {
                bar.println(format!(
                    "[{}] - Wildcard response is a static size; auto-filtering out responses of size [{} bytes]; this behavior can be turned off by using --dontfilter",
                    status_colorizer("WILDCARD"),
                    wc_length
                ));

                wildcard.size = wc_length;
            }
        } else {
            bar.inc(2);
        }

        log::trace!("exit: wildcard_test -> Some({:?})", wildcard);
        return Some(wildcard);
    }

    log::trace!("exit: wildcard_test -> None");
    None
}

/// Generates a uuid and appends it to the given target url. The reasoning is that the randomly
/// generated unique string should not exist on and be served by the target web server.
///
/// Once the unique url is created, the request is sent to the server. If the server responds
/// back with a valid status code, the response is considered to be a wildcard response. If that
/// wildcard response has a 3xx status code, that redirection location is displayed to the user.
async fn make_wildcard_request(target_url: &str, length: usize, bar: ProgressBar) -> Option<Response> {
    log::trace!("enter: make_wildcard_request({}, {})", target_url, length);

    let unique_str = unique_string(length);

    let nonexistent = match format_url(
        target_url,
        &unique_str,
        CONFIGURATION.addslash,
        &CONFIGURATION.queries,
        None,
    ) {
        Ok(url) => url,
        Err(e) => {
            log::error!("{}", e);
            log::trace!("exit: make_wildcard_request -> None");
            return None;
        }
    };

    let wildcard = status_colorizer("WILDCARD");

    match make_request(&CONFIGURATION.client, &nonexistent.to_owned()).await {
        Ok(response) => {
            if CONFIGURATION
                .statuscodes
                .contains(&response.status().as_u16())
            {
                // found a wildcard response
                let url_len = get_url_path_length(&response.url());

                bar.println(
                    format!(
                        "[{}] - Received [{}] for {} (content: {} bytes, url length: {})",
                        wildcard,
                        status_colorizer(&response.status().to_string()),
                        response.url(),
                        response.content_length().unwrap_or(0),
                        url_len
                    )
                );

                if response.status().is_redirection() {
                    // show where it goes, if possible
                    if let Some(next_loc) = response.headers().get("Location") {
                        if let Ok(next_loc_str) = next_loc.to_str() {
                            bar.println(
                                format!(
                                    "[{}] {} redirects to => {}",
                                    wildcard,
                                    response.url(),
                                    next_loc_str
                                )
                            );
                        } else {
                            bar.println(
                                format!(
                                    "[{}] {} redirects to => {:?}",
                                    wildcard,
                                    response.url(),
                                    next_loc
                                )
                            );
                        }
                    }
                }
                log::trace!("exit: make_wildcard_request -> {:?}", response);
                return Some(response);
            }
        }
        Err(e) => {
            log::warn!("{}", e);
            log::trace!("exit: make_wildcard_request -> None");
            return None;
        }
    }
    log::trace!("exit: make_wildcard_request -> None");
    None
}

/// Simply tries to connect to all given sites before starting to scan
///
/// In the event that no sites can be reached, the program will exit.
///
/// Any urls that are found to be alive are returned to the caller.
pub async fn connectivity_test(target_urls: &[String]) -> Vec<String> {
    log::trace!("enter: connectivity_test({:?})", target_urls);

    let mut good_urls = vec![];

    // hidden bar just to get ProgressBar::println functionality
    let bar = progress::add_bar("", 1, true);

    for target_url in target_urls {
        let request = match format_url(
            target_url,
            "",
            CONFIGURATION.addslash,
            &CONFIGURATION.queries,
            None,
        ) {
            Ok(url) => url,
            Err(e) => {
                log::error!("{}", e);
                bar.inc(1);
                continue;
            }
        };

        match make_request(&CONFIGURATION.client, &request).await {
            Ok(_) => {
                good_urls.push(target_url.to_owned());
            }
            Err(e) => {
                bar.println(format!("Could not connect to {}, skipping...", target_url));
                log::error!("{}", e);
            }
        }
    }

    bar.finish();

    if good_urls.is_empty() {
        log::error!("Could not connect to any target provided, exiting.");
        log::trace!("exit: connectivity_test");
        process::exit(1);
    }

    log::trace!("exit: connectivity_test -> {:?}", good_urls);

    good_urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_string_returns_correct_length() {
        for i in 0..10 {
            assert_eq!(unique_string(i).len(), i * 32);
        }
    }
}
