use crate::{
    config::{CONFIGURATION, PROGRESS_PRINTER},
    filters::WildcardFilter,
    scanner::should_filter_response,
    statistics::StatCommand,
    utils::{ferox_print, format_url, get_url_path_length, make_request, status_colorizer},
    FeroxResponse,
};
use console::style;
use indicatif::ProgressBar;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

/// length of a standard UUID, used when determining wildcard responses
const UUID_LENGTH: u64 = 32;

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
pub async fn wildcard_test(
    target_url: &str,
    bar: ProgressBar,
    tx_term: UnboundedSender<FeroxResponse>,
    tx_stats: UnboundedSender<StatCommand>,
) -> Option<WildcardFilter> {
    log::trace!(
        "enter: wildcard_test({:?}, {:?}, {:?}, {:?})",
        target_url,
        bar,
        tx_term,
        tx_stats
    );

    if CONFIGURATION.dont_filter {
        // early return, dont_filter scans don't need tested
        log::trace!("exit: wildcard_test -> None");
        return None;
    }

    let tx_term_mwcr1 = tx_term.clone();
    let tx_term_mwcr2 = tx_term.clone();
    let tx_stats_mwcr1 = tx_stats.clone();
    let tx_stats_mwcr2 = tx_stats.clone();

    if let Some(ferox_response) =
        make_wildcard_request(&target_url, 1, tx_term_mwcr1, tx_stats_mwcr1).await
    {
        bar.inc(1);

        // found a wildcard response
        let mut wildcard = WildcardFilter::default();

        let wc_length = ferox_response.content_length();

        if wc_length == 0 {
            log::trace!("exit: wildcard_test -> Some({:?})", wildcard);
            return Some(wildcard);
        }

        // content length of wildcard is non-zero, perform additional tests:
        //   make a second request, with a known-sized (64) longer request
        if let Some(resp_two) =
            make_wildcard_request(&target_url, 3, tx_term_mwcr2, tx_stats_mwcr2).await
        {
            bar.inc(1);

            let wc2_length = resp_two.content_length();

            if wc2_length == wc_length + (UUID_LENGTH * 2) {
                // second length is what we'd expect to see if the requested url is
                // reflected in the response along with some static content; aka custom 404
                let url_len = get_url_path_length(&ferox_response.url());

                wildcard.dynamic = wc_length - url_len;

                if !CONFIGURATION.quiet {
                    let msg = format!(
                            "{} {:>9} {:>9} {:>9} Wildcard response is dynamic; {} ({} + url length) responses; toggle this behavior by using {}\n",
                            status_colorizer("WLD"),
                            "-",
                            "-",
                            "-",
                            style("auto-filtering").yellow(),
                            style(wc_length - url_len).cyan(),
                            style("--dont-filter").yellow()
                    );

                    ferox_print(&msg, &PROGRESS_PRINTER);
                }
            } else if wc_length == wc2_length {
                wildcard.size = wc_length;

                if !CONFIGURATION.quiet {
                    let msg = format!(
                        "{} {:>9} {:>9} {:>9} Wildcard response is static; {} {} responses; toggle this behavior by using {}\n",
                        status_colorizer("WLD"),
                        "-",
                        "-",
                        "-",
                        style("auto-filtering").yellow(),
                        style(wc_length).cyan(),
                        style("--dont-filter").yellow()
                    );

                    ferox_print(&msg, &PROGRESS_PRINTER);
                }
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
async fn make_wildcard_request(
    target_url: &str,
    length: usize,
    tx_file: UnboundedSender<FeroxResponse>,
    tx_stats: UnboundedSender<StatCommand>,
) -> Option<FeroxResponse> {
    log::trace!(
        "enter: make_wildcard_request({}, {}, {:?}, {:?})",
        target_url,
        length,
        tx_file,
        tx_stats,
    );

    let unique_str = unique_string(length);

    let nonexistent = match format_url(
        target_url,
        &unique_str,
        CONFIGURATION.add_slash,
        &CONFIGURATION.queries,
        None,
        tx_stats.clone(),
    ) {
        Ok(url) => url,
        Err(e) => {
            log::error!("{}", e);
            log::trace!("exit: make_wildcard_request -> None");
            return None;
        }
    };

    match make_request(
        &CONFIGURATION.client,
        &nonexistent.to_owned(),
        tx_stats.clone(),
    )
    .await
    {
        Ok(response) => {
            if CONFIGURATION
                .status_codes
                .contains(&response.status().as_u16())
            {
                // found a wildcard response
                let mut ferox_response = FeroxResponse::from(response, true).await;
                ferox_response.wildcard = true;

                if !CONFIGURATION.quiet
                    && !should_filter_response(&ferox_response, tx_stats.clone())
                    && tx_file.send(ferox_response.clone()).is_err()
                {
                    return None;
                }

                log::trace!("exit: make_wildcard_request -> {}", ferox_response);
                return Some(ferox_response);
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
pub async fn connectivity_test(
    target_urls: &[String],
    tx_stats: UnboundedSender<StatCommand>,
) -> Vec<String> {
    log::trace!(
        "enter: connectivity_test({:?}, {:?})",
        target_urls,
        tx_stats
    );

    let mut good_urls = vec![];

    for target_url in target_urls {
        let request = match format_url(
            target_url,
            "",
            CONFIGURATION.add_slash,
            &CONFIGURATION.queries,
            None,
            tx_stats.clone(),
        ) {
            Ok(url) => url,
            Err(e) => {
                log::error!("{}", e);
                continue;
            }
        };

        match make_request(&CONFIGURATION.client, &request, tx_stats.clone()).await {
            Ok(_) => {
                good_urls.push(target_url.to_owned());
            }
            Err(e) => {
                if !CONFIGURATION.quiet {
                    ferox_print(
                        &format!("Could not connect to {}, skipping...", target_url),
                        &PROGRESS_PRINTER,
                    );
                }
                log::error!("{}", e);
            }
        }
    }

    if good_urls.is_empty() {
        log::error!("Could not connect to any target provided, exiting.");
    }

    log::trace!("exit: connectivity_test -> {:?}", good_urls);

    good_urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// request a unique string of 32bytes * a value returns correct result
    fn heuristics_unique_string_returns_correct_length() {
        for i in 0..10 {
            assert_eq!(unique_string(i).len(), i * 32);
        }
    }

    #[test]
    /// simply test the default values for wildcardfilter, expect 0, 0
    fn heuristics_wildcardfilter_dafaults() {
        let wcf = WildcardFilter::default();
        assert_eq!(wcf.size, 0);
        assert_eq!(wcf.dynamic, 0);
    }
}
