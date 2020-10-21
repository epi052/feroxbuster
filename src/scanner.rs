use crate::config::{CONFIGURATION, PROGRESS_BAR};
use crate::extractor::get_links;
use crate::heuristics::WildcardFilter;
use crate::utils::{format_url, get_current_depth, get_url_path_length, make_request};
use crate::{heuristics, progress, FeroxChannel, FeroxResponse};
use futures::future::{BoxFuture, FutureExt};
use futures::{stream, StreamExt};
use lazy_static::lazy_static;
use reqwest::Url;
use std::collections::HashSet;
use std::convert::TryInto;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

/// Single atomic number that gets incremented once, used to track first scan vs. all others
static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    /// Set of urls that have been sent to [scan_url](fn.scan_url.html), used for deduplication
    static ref SCANNED_URLS: RwLock<HashSet<String>> = RwLock::new(HashSet::new());

    /// Vector of WildcardFilters that have been ID'd through heuristics
    static ref WILDCARD_FILTERS: Arc<RwLock<Vec<Arc<WildcardFilter>>>> = Arc::new(RwLock::new(Vec::<Arc<WildcardFilter>>::new()));
}

/// Adds the given url to `SCANNED_URLS`
///
/// If `SCANNED_URLS` did not already contain the url, return true; otherwise return false
fn add_url_to_list_of_scanned_urls(resp: &str, scanned_urls: &RwLock<HashSet<String>>) -> bool {
    log::trace!(
        "enter: add_url_to_list_of_scanned_urls({}, {:?})",
        resp,
        scanned_urls
    );

    match scanned_urls.write() {
        // check new url against what's already been scanned
        Ok(mut urls) => {
            let normalized_url = if resp.ends_with('/') {
                // append a / to the list of 'seen' urls, this is to prevent the case where
                // 3xx and 2xx duplicate eachother
                resp.to_string()
            } else {
                format!("{}/", resp)
            };

            // If the set did not contain resp, true is returned.
            // If the set did contain resp, false is returned.
            let response = urls.insert(normalized_url);

            log::trace!("exit: add_url_to_list_of_scanned_urls -> {}", response);
            response
        }
        Err(e) => {
            // poisoned lock
            log::error!("Set of scanned urls poisoned: {}", e);
            log::trace!("exit: add_url_to_list_of_scanned_urls -> false");
            false
        }
    }
}

/// Adds the given WildcardFilter to `WILDCARD_FILTERS`
///
/// If `WILDCARD_FILTERS` did not already contain the filter, return true; otherwise return false
fn add_filter_to_list_of_wildcard_filters(
    filter: Arc<WildcardFilter>,
    wildcard_filters: Arc<RwLock<Vec<Arc<WildcardFilter>>>>,
) -> bool {
    log::trace!(
        "enter: add_filter_to_list_of_wildcard_filters({:?}, {:?})",
        filter,
        wildcard_filters
    );

    match wildcard_filters.write() {
        Ok(mut filters) => {
            // If the set did not contain the assigned filter, true is returned.
            // If the set did contain the assigned filter, false is returned.
            if filters.contains(&filter) {
                log::trace!("exit: add_filter_to_list_of_wildcard_filters -> false");
                return false;
            }

            filters.push(filter);

            log::trace!("exit: add_filter_to_list_of_wildcard_filters -> true");
            true
        }
        Err(e) => {
            // poisoned lock
            log::error!("Set of wildcard filters poisoned: {}", e);
            log::trace!("exit: add_filter_to_list_of_wildcard_filters -> false");
            false
        }
    }
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives Urls and scans them
fn spawn_recursion_handler(
    mut recursion_channel: UnboundedReceiver<String>,
    wordlist: Arc<HashSet<String>>,
    base_depth: usize,
    tx_term: UnboundedSender<FeroxResponse>,
    tx_file: UnboundedSender<String>,
) -> BoxFuture<'static, Vec<JoinHandle<()>>> {
    log::trace!(
        "enter: spawn_recursion_handler({:?}, wordlist[{} words...], {}, {:?}, {:?})",
        recursion_channel,
        wordlist.len(),
        base_depth,
        tx_term,
        tx_file
    );

    let boxed_future = async move {
        let mut scans = vec![];
        while let Some(resp) = recursion_channel.recv().await {
            let unknown = add_url_to_list_of_scanned_urls(&resp, &SCANNED_URLS);

            if !unknown {
                // not unknown, i.e. we've seen the url before and don't need to scan again
                continue;
            }

            log::info!("received {} on recursion channel", resp);

            let term_clone = tx_term.clone();
            let file_clone = tx_file.clone();
            let resp_clone = resp.clone();
            let list_clone = wordlist.clone();

            scans.push(tokio::spawn(async move {
                scan_url(
                    resp_clone.to_owned().as_str(),
                    list_clone,
                    base_depth,
                    term_clone,
                    file_clone,
                )
                .await
            }));
        }
        scans
    }
    .boxed();

    log::trace!("exit: spawn_recursion_handler -> BoxFuture<'static, Vec<JoinHandle<()>>>");
    boxed_future
}

/// Creates a vector of formatted Urls
///
/// At least one value will be returned (base_url + word)
///
/// If any extensions were passed to the program, each extension will add a
/// (base_url + word + ext) Url to the vector
fn create_urls(target_url: &str, word: &str, extensions: &[String]) -> Vec<Url> {
    log::trace!(
        "enter: create_urls({}, {}, {:?})",
        target_url,
        word,
        extensions
    );

    let mut urls = vec![];

    if let Ok(url) = format_url(
        &target_url,
        &word,
        CONFIGURATION.addslash,
        &CONFIGURATION.queries,
        None,
    ) {
        urls.push(url); // default request, i.e. no extension
    }

    for ext in extensions.iter() {
        if let Ok(url) = format_url(
            &target_url,
            &word,
            CONFIGURATION.addslash,
            &CONFIGURATION.queries,
            Some(ext),
        ) {
            urls.push(url); // any extensions passed in
        }
    }

    log::trace!("exit: create_urls -> {:?}", urls);
    urls
}

/// Helper function to determine suitability for recursion
///
/// handles 2xx and 3xx responses by either checking if the url ends with a / (2xx)
/// or if the Location header is present and matches the base url + / (3xx)
fn response_is_directory(response: &FeroxResponse) -> bool {
    log::trace!("enter: is_directory({:?})", response);

    if response.status().is_redirection() {
        // status code is 3xx
        match response.headers().get("Location") {
            // and has a Location header
            Some(loc) => {
                // get absolute redirect Url based on the already known base url
                log::debug!("Location header: {:?}", loc);

                if let Ok(loc_str) = loc.to_str() {
                    if let Ok(abs_url) = response.url().join(loc_str) {
                        if format!("{}/", response.url()) == abs_url.as_str() {
                            // if current response's Url + / == the absolute redirection
                            // location, we've found a directory suitable for recursion
                            log::debug!(
                                "found directory suitable for recursion: {}",
                                response.url()
                            );
                            log::trace!("exit: is_directory -> true");
                            return true;
                        }
                    }
                }
            }
            None => {
                log::debug!(
                    "expected Location header, but none was found: {:?}",
                    response
                );
                log::trace!("exit: is_directory -> false");
                return false;
            }
        }
    } else if response.status().is_success() {
        // status code is 2xx, need to check if it ends in /
        if response.url().as_str().ends_with('/') {
            log::debug!("{} is directory suitable for recursion", response.url());
            log::trace!("exit: is_directory -> true");
            return true;
        }
    }

    log::trace!("exit: is_directory -> false");
    false
}

/// Helper function that determines if the configured maximum recursion depth has been reached
///
/// Essentially looks at the Url path and determines how many directories are present in the
/// given Url
fn reached_max_depth(url: &Url, base_depth: usize, max_depth: usize) -> bool {
    log::trace!(
        "enter: reached_max_depth({}, {}, {})",
        url,
        base_depth,
        max_depth
    );

    if max_depth == 0 {
        // early return, as 0 means recurse forever; no additional processing needed
        log::trace!("exit: reached_max_depth -> false");
        return false;
    }

    let depth = get_current_depth(url.as_str());

    if depth - base_depth >= max_depth {
        return true;
    }

    log::trace!("exit: reached_max_depth -> false");
    false
}

/// Helper function that wraps logic to check for recursion opportunities
///
/// When a recursion opportunity is found, the new url is sent across the recursion channel
async fn try_recursion(
    response: &FeroxResponse,
    base_depth: usize,
    transmitter: UnboundedSender<String>,
) {
    log::trace!(
        "enter: try_recursion({:?}, {}, {:?})",
        response,
        base_depth,
        transmitter
    );

    if !reached_max_depth(response.url(), base_depth, CONFIGURATION.depth)
        && response_is_directory(&response)
    {
        if CONFIGURATION.redirects {
            // response is 2xx can simply send it because we're following redirects
            log::info!("Added new directory to recursive scan: {}", response.url());

            match transmitter.send(String::from(response.url().as_str())) {
                Ok(_) => {
                    log::debug!("sent {} across channel to begin a new scan", response.url());
                }
                Err(e) => {
                    log::error!(
                        "Could not send {} to recursion handler: {}",
                        response.url(),
                        e
                    );
                }
            }
        } else {
            let new_url = String::from(response.url().as_str());

            log::info!("Added new directory to recursive scan: {}", new_url);

            match transmitter.send(new_url) {
                Ok(_) => {}
                Err(e) => {
                    log::error!(
                        "Could not send {}/ to recursion handler: {}",
                        response.url(),
                        e
                    );
                }
            }
        }
    }
    log::trace!("exit: try_recursion");
}

/// Simple helper to stay DRY; determines whether or not a given `FeroxResponse` should be reported
/// to the user or not.
pub fn should_filter_response(content_len: &u64, url: &Url) -> bool {
    if CONFIGURATION.sizefilters.contains(content_len) {
        // filtered value from --sizefilters, move on to the next url
        log::debug!("size filter: filtered out {}", url);
        return true;
    }

    match WILDCARD_FILTERS.read() {
        Ok(filters) => {
            for filter in filters.iter() {
                if CONFIGURATION.dontfilter {
                    // quick return if dontfilter is set
                    return false;
                }

                if filter.size > 0 && filter.size == *content_len {
                    // static wildcard size found during testing
                    // size isn't default, size equals response length, and auto-filter is on
                    log::debug!("static wildcard: filtered out {}", url);
                    return true;
                }

                if filter.dynamic > 0 {
                    // dynamic wildcard offset found during testing

                    // I'm about to manually split this url path instead of using reqwest::Url's
                    // builtin parsing. The reason is that they call .split() on the url path
                    // except that I don't want an empty string taking up the last index in the
                    // event that the url ends with a forward slash.  It's ugly enough to be split
                    // into its own function for readability.
                    let url_len = get_url_path_length(&url);

                    if url_len + filter.dynamic == *content_len {
                        log::debug!("dynamic wildcard: filtered out {}", url);
                        return true;
                    }
                }
            }
        }
        Err(e) => {
            log::error!("{}", e);
        }
    }
    false
}

/// Wrapper for [make_request](fn.make_request.html)
///
/// Handles making multiple requests based on the presence of extensions
///
/// Attempts recursion when appropriate and sends Responses to the report handler for processing
async fn make_requests(
    target_url: &str,
    word: &str,
    base_depth: usize,
    dir_chan: UnboundedSender<String>,
    report_chan: UnboundedSender<FeroxResponse>,
) {
    log::trace!(
        "enter: make_requests({}, {}, {}, {:?}, {:?})",
        target_url,
        word,
        base_depth,
        dir_chan,
        report_chan
    );

    let urls = create_urls(&target_url, &word, &CONFIGURATION.extensions);

    for url in urls {
        if let Ok(response) = make_request(&CONFIGURATION.client, &url).await {
            // response came back without error, convert it to FeroxResponse
            let ferox_response = FeroxResponse::from(response, CONFIGURATION.extract_links).await;

            // do recursion if appropriate
            if !CONFIGURATION.norecursion {
                try_recursion(&ferox_response, base_depth, dir_chan.clone()).await;
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not

            let content_len = &ferox_response.content_length();

            if should_filter_response(content_len, &ferox_response.url()) {
                continue;
            }

            if CONFIGURATION.extract_links && !ferox_response.status().is_redirection() {
                let new_links = get_links(&ferox_response).await;

                for new_link in new_links {
                    let unknown = add_url_to_list_of_scanned_urls(&new_link, &SCANNED_URLS);

                    if !unknown {
                        // not unknown, i.e. we've seen the url before and don't need to scan again
                        continue;
                    }

                    // create a url based on the given command line options, continue on error
                    let new_url = match format_url(
                        &new_link,
                        &"",
                        CONFIGURATION.addslash,
                        &CONFIGURATION.queries,
                        None,
                    ) {
                        Ok(url) => url,
                        Err(_) => continue,
                    };

                    // make the request and store the response
                    let new_response = match make_request(&CONFIGURATION.client, &new_url).await {
                        Ok(resp) => resp,
                        Err(_) => continue,
                    };

                    let mut new_ferox_response =
                        FeroxResponse::from(new_response, CONFIGURATION.extract_links).await;

                    // filter if necessary
                    let new_content_len = &new_ferox_response.content_length();
                    if should_filter_response(new_content_len, &new_ferox_response.url()) {
                        continue;
                    }

                    if new_ferox_response.is_file() {
                        // very likely a file, simply request and report
                        log::debug!(
                            "Singular extraction: {} ({})",
                            new_ferox_response.url(),
                            new_ferox_response.status().as_str(),
                        );

                        send_report(report_chan.clone(), new_ferox_response);

                        continue;
                    }

                    if !CONFIGURATION.norecursion {
                        log::debug!(
                            "Recursive extraction: {} ({})",
                            new_ferox_response.url(),
                            new_ferox_response.status().as_str()
                        );

                        if new_ferox_response.status().is_success()
                            && !new_ferox_response.url().as_str().ends_with('/')
                        {
                            // since all of these are 2xx, recursion is only attempted if the
                            // url ends in a /. I am actually ok with adding the slash and not
                            // adding it, as both have merit.  Leaving it in for now to see how
                            // things turn out (current as of: v1.1.0)
                            new_ferox_response.set_url(&format!("{}/", new_ferox_response.url()));
                        }

                        try_recursion(&new_ferox_response, base_depth, dir_chan.clone()).await;
                    }
                }
            }

            // everything else should be reported
            send_report(report_chan.clone(), ferox_response);
        }
    }
    log::trace!("exit: make_requests");
}

/// Simple helper to send a `FeroxResponse` over the tx side of an `mpsc::unbounded_channel`
fn send_report(report_sender: UnboundedSender<FeroxResponse>, response: FeroxResponse) {
    log::trace!("enter: send_report({:?}, {:?}", report_sender, response);

    match report_sender.send(response) {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e);
        }
    }

    log::trace!("exit: send_report");
}

/// Scan a given url using a given wordlist
///
/// This is the primary entrypoint for the scanner
pub async fn scan_url(
    target_url: &str,
    wordlist: Arc<HashSet<String>>,
    base_depth: usize,
    tx_term: UnboundedSender<FeroxResponse>,
    tx_file: UnboundedSender<String>,
) {
    log::trace!(
        "enter: scan_url({:?}, wordlist[{} words...], {}, {:?}, {:?})",
        target_url,
        wordlist.len(),
        base_depth,
        tx_term,
        tx_file
    );

    log::info!("Starting scan against: {}", target_url);

    let (tx_dir, rx_dir): FeroxChannel<String> = mpsc::unbounded_channel();

    let num_reqs_expected: u64 = if CONFIGURATION.extensions.is_empty() {
        wordlist.len().try_into().unwrap()
    } else {
        let total = wordlist.len() * (CONFIGURATION.extensions.len() + 1);
        total.try_into().unwrap()
    };

    let progress_bar = progress::add_bar(&target_url, num_reqs_expected, false);
    progress_bar.reset_elapsed();

    if CALL_COUNT.load(Ordering::Relaxed) == 0 {
        // join can only be called once, otherwise it causes the thread to panic
        tokio::task::spawn_blocking(move || PROGRESS_BAR.join().unwrap());
        CALL_COUNT.fetch_add(1, Ordering::Relaxed);

        // this protection around join also allows us to add the first scanned url to SCANNED_URLS
        // from within the scan_url function instead of the recursion handler
        add_url_to_list_of_scanned_urls(&target_url, &SCANNED_URLS);
    }

    // Arc clones to be passed around to the various scans
    let wildcard_bar = progress_bar.clone();
    let heuristics_file_clone = tx_file.clone();
    let recurser_term_clone = tx_term.clone();
    let recurser_file_clone = tx_file.clone();
    let recurser_words = wordlist.clone();
    let looping_words = wordlist.clone();

    let recurser = tokio::spawn(async move {
        spawn_recursion_handler(
            rx_dir,
            recurser_words,
            base_depth,
            recurser_term_clone,
            recurser_file_clone,
        )
        .await
    });

    let filter =
        match heuristics::wildcard_test(&target_url, wildcard_bar, heuristics_file_clone).await {
            Some(f) => Arc::new(f),
            None => Arc::new(WildcardFilter::default()),
        };

    add_filter_to_list_of_wildcard_filters(filter.clone(), WILDCARD_FILTERS.clone());

    // producer tasks (mp of mpsc); responsible for making requests
    let producers = stream::iter(looping_words.deref().to_owned())
        .map(|word| {
            let txd = tx_dir.clone();
            let txr = tx_term.clone();
            let pb = progress_bar.clone(); // progress bar is an Arc around internal state
            let tgt = target_url.to_string(); // done to satisfy 'static lifetime below
            (
                tokio::spawn(async move { make_requests(&tgt, &word, base_depth, txd, txr).await }),
                pb,
            )
        })
        .for_each_concurrent(CONFIGURATION.threads, |(resp, bar)| async move {
            match resp.await {
                Ok(_) => {
                    bar.inc(1);
                }
                Err(e) => {
                    log::error!("error awaiting a response: {}", e);
                }
            }
        });

    // await tx tasks
    log::trace!("awaiting scan producers");
    producers.await;
    log::trace!("done awaiting scan producers");

    progress_bar.finish();

    // manually drop tx in order for the rx task's while loops to eval to false
    log::trace!("dropped recursion handler's transmitter");
    drop(tx_dir);

    // await rx tasks
    log::trace!("awaiting recursive scan receiver/scans");
    futures::future::join_all(recurser.await.unwrap()).await;
    log::trace!("done awaiting recursive scan receiver/scans");

    log::trace!("exit: scan_url");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// sending url + word without any extensions should get back one url with the joined word
    fn create_urls_no_extension_returns_base_url_with_word() {
        let urls = create_urls("http://localhost", "turbo", &[]);
        assert_eq!(urls, [Url::parse("http://localhost/turbo").unwrap()])
    }

    #[test]
    /// sending url + word + 1 extension should get back two urls, one base and one with extension
    fn create_urls_one_extension_returns_two_urls() {
        let urls = create_urls("http://localhost", "turbo", &[String::from("js")]);
        assert_eq!(
            urls,
            [
                Url::parse("http://localhost/turbo").unwrap(),
                Url::parse("http://localhost/turbo.js").unwrap()
            ]
        )
    }

    #[test]
    /// sending url + word + multiple extensions should get back n+1 urls
    fn create_urls_multiple_extensions_returns_n_plus_one_urls() {
        let ext_vec = vec![
            vec![String::from("js")],
            vec![String::from("js"), String::from("php")],
            vec![String::from("js"), String::from("php"), String::from("pdf")],
            vec![
                String::from("js"),
                String::from("php"),
                String::from("pdf"),
                String::from("tar.gz"),
            ],
        ];

        let base = Url::parse("http://localhost/turbo").unwrap();
        let js = Url::parse("http://localhost/turbo.js").unwrap();
        let php = Url::parse("http://localhost/turbo.php").unwrap();
        let pdf = Url::parse("http://localhost/turbo.pdf").unwrap();
        let tar = Url::parse("http://localhost/turbo.tar.gz").unwrap();

        let expected = vec![
            vec![base.clone(), js.clone()],
            vec![base.clone(), js.clone(), php.clone()],
            vec![base.clone(), js.clone(), php.clone(), pdf.clone()],
            vec![base, js, php, pdf, tar],
        ];

        for (i, ext_set) in ext_vec.into_iter().enumerate() {
            let urls = create_urls("http://localhost", "turbo", &ext_set);
            assert_eq!(urls, expected[i]);
        }
    }

    #[test]
    /// call reached_max_depth with max depth of zero, which is infinite recursion, expect false
    fn reached_max_depth_returns_early_on_zero() {
        let url = Url::parse("http://localhost").unwrap();
        let result = reached_max_depth(&url, 0, 0);
        assert!(!result);
    }

    #[test]
    /// call reached_max_depth with url depth equal to max depth, expect true
    fn reached_max_depth_current_depth_equals_max() {
        let url = Url::parse("http://localhost/one/two").unwrap();
        let result = reached_max_depth(&url, 0, 2);
        assert!(result);
    }

    #[test]
    /// call reached_max_depth with url dpeth less than max depth, expect false
    fn reached_max_depth_current_depth_less_than_max() {
        let url = Url::parse("http://localhost").unwrap();
        let result = reached_max_depth(&url, 0, 2);
        assert!(!result);
    }

    #[test]
    /// call reached_max_depth with url of 2, base depth of 2, and max depth of 2, expect false
    fn reached_max_depth_base_depth_equals_max_depth() {
        let url = Url::parse("http://localhost/one/two").unwrap();
        let result = reached_max_depth(&url, 2, 2);
        assert!(!result);
    }

    #[test]
    /// call reached_max_depth with url depth greater than max depth, expect true
    fn reached_max_depth_current_greater_than_max() {
        let url = Url::parse("http://localhost/one/two/three").unwrap();
        let result = reached_max_depth(&url, 0, 2);
        assert!(result);
    }

    #[test]
    /// add an unknown url to the hashset, expect true
    fn add_url_to_list_of_scanned_urls_with_unknown_url() {
        let urls = RwLock::new(HashSet::<String>::new());
        let url = "http://unknown_url";
        assert_eq!(add_url_to_list_of_scanned_urls(url, &urls), true);
    }

    #[test]
    /// add a known url to the hashset, with a trailing slash, expect false
    fn add_url_to_list_of_scanned_urls_with_known_url() {
        let urls = RwLock::new(HashSet::<String>::new());
        let url = "http://unknown_url/";

        assert_eq!(urls.write().unwrap().insert(url.to_string()), true);

        assert_eq!(add_url_to_list_of_scanned_urls(url, &urls), false);
    }

    #[test]
    /// add a known url to the hashset, without a trailing slash, expect false
    fn add_url_to_list_of_scanned_urls_with_known_url_without_slash() {
        let urls = RwLock::new(HashSet::<String>::new());
        let url = "http://unknown_url";

        assert_eq!(
            urls.write()
                .unwrap()
                .insert("http://unknown_url/".to_string()),
            true
        );

        assert_eq!(add_url_to_list_of_scanned_urls(url, &urls), false);
    }

    #[test]
    /// add a wildcard filter with the `size` attribute set to WILDCARD_FILTERS and ensure that
    /// should_filter_response correctly returns true
    fn should_filter_response_filters_wildcard_size() {
        let mut filter = WildcardFilter::default();
        let url = Url::parse("http://localhost").unwrap();
        filter.size = 18;
        let filter = Arc::new(filter);
        add_filter_to_list_of_wildcard_filters(filter, WILDCARD_FILTERS.clone());
        let result = should_filter_response(&18, &url);
        assert!(result);

    }

    #[test]
    /// add a wildcard filter with the `dynamic` attribute set to WILDCARD_FILTERS and ensure that
    /// should_filter_response correctly returns true
    fn should_filter_response_filters_wildcard_dynamic() {
        let mut filter = WildcardFilter::default();
        let url = Url::parse("http://localhost/some-path").unwrap();
        filter.dynamic = 9;
        let filter = Arc::new(filter);
        add_filter_to_list_of_wildcard_filters(filter, WILDCARD_FILTERS.clone());
        let result = should_filter_response(&18, &url);
        assert!(result);
    }
}
