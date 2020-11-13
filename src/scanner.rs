use crate::{
    config::CONFIGURATION,
    extractor::get_links,
    filters::{FeroxFilter, StatusCodeFilter, WildcardFilter},
    heuristics, progress,
    utils::{format_url, get_current_depth, make_request},
    FeroxChannel, FeroxResponse, SLEEP_DURATION,
};
use console::style;
use futures::{
    future::{BoxFuture, FutureExt},
    stream, StreamExt,
};
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use reqwest::Url;
use std::{
    collections::HashSet,
    convert::TryInto,
    io::{stderr, Write},
    ops::Deref,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    sync::{Arc, RwLock},
};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        Semaphore,
    },
    task::JoinHandle,
    time,
};

/// Single atomic number that gets incremented once, used to track first scan vs. all others
static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Atomic boolean flag, used to determine whether or not a scan should pause or resume
pub static PAUSE_SCAN: AtomicBool = AtomicBool::new(false);

lazy_static! {
    /// Set of urls that have been sent to [scan_url](fn.scan_url.html), used for deduplication
    static ref SCANNED_URLS: RwLock<HashSet<String>> = RwLock::new(HashSet::new());

    /// A clock spinner protected with a RwLock to allow for a single thread to use at a time
    static ref SINGLE_SPINNER: RwLock<ProgressBar> = RwLock::new(get_single_spinner());

    /// Vector of implementors of the FeroxFilter trait
    static ref FILTERS: Arc<RwLock<Vec<Box<dyn FeroxFilter>>>> = Arc::new(RwLock::new(Vec::<Box<dyn FeroxFilter>>::new()));

    /// Bounded semaphore used as a barrier to limit concurrent scans
    static ref SCAN_LIMITER: Semaphore = Semaphore::new(CONFIGURATION.scan_limit);
}

/// Return a clock spinner, used when scans are paused
fn get_single_spinner() -> ProgressBar {
    log::trace!("enter: get_single_spinner");

    let spinner = ProgressBar::new_spinner().with_style(
        ProgressStyle::default_spinner()
            .tick_strings(&[
                "🕛", "🕐", "🕑", "🕒", "🕓", "🕔", "🕕", "🕖", "🕗", "🕘", "🕙", "🕚",
            ])
            .template(&format!(
                "\t-= All Scans {{spinner}} {} =-",
                style("Paused").red()
            )),
    );

    log::trace!("exit: get_single_spinner -> {:?}", spinner);
    spinner
}

/// Forced the calling thread into a busy loop
///
/// Every `SLEEP_DURATION` milliseconds, the function examines the result stored in `PAUSE_SCAN`
///
/// When the value stored in `PAUSE_SCAN` becomes `false`, the function returns, exiting the busy
/// loop
async fn pause_scan() {
    log::trace!("enter: pause_scan");
    // function uses tokio::time, not std

    // local testing showed a pretty slow increase (less than linear) in CPU usage as # of
    // concurrent scans rose when SLEEP_DURATION was set to 500, using that as the default for now
    let mut interval = time::interval(time::Duration::from_millis(SLEEP_DURATION));

    // ignore any error returned
    let _ = stderr().flush();

    if SINGLE_SPINNER.read().unwrap().is_finished() {
        // in order to not leave draw artifacts laying around in the terminal, we call
        // finish_and_clear on the progress bar when resuming scans. For this reason, we need to
        // check if the spinner is finished, and repopulate the RwLock with a new spinner if
        // necessary
        if let Ok(mut guard) = SINGLE_SPINNER.write() {
            *guard = get_single_spinner();
        }
    }

    if let Ok(spinner) = SINGLE_SPINNER.write() {
        spinner.enable_steady_tick(120);
    }

    loop {
        // first tick happens immediately, all others wait the specified duration
        interval.tick().await;

        if !PAUSE_SCAN.load(Ordering::Acquire) {
            // PAUSE_SCAN is false, so we can exit the busy loop
            if let Ok(spinner) = SINGLE_SPINNER.write() {
                spinner.finish_and_clear();
            }
            let _ = stderr().flush();
            log::trace!("exit: pause_scan");
            return;
        }
    }
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
            // If the set did not contain resp, true is returned.
            // If the set did contain resp, false is returned.
            let response = urls.insert(resp.to_string());

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

/// Adds the given FeroxFilter to the given list of FeroxFilter implementors
///
/// If the given list did not already contain the filter, return true; otherwise return false
fn add_filter_to_list_of_ferox_filters(
    filter: Box<dyn FeroxFilter>,
    ferox_filters: Arc<RwLock<Vec<Box<dyn FeroxFilter>>>>,
) -> bool {
    log::trace!(
        "enter: add_filter_to_list_of_ferox_filters({:?}, {:?})",
        filter,
        ferox_filters
    );

    match ferox_filters.write() {
        Ok(mut filters) => {
            // If the set did not contain the assigned filter, true is returned.
            // If the set did contain the assigned filter, false is returned.
            if filters.contains(&filter) {
                log::trace!("exit: add_filter_to_list_of_ferox_filters -> false");
                return false;
            }

            filters.push(filter);

            log::trace!("exit: add_filter_to_list_of_ferox_filters -> true");
            true
        }
        Err(e) => {
            // poisoned lock
            log::error!("Set of wildcard filters poisoned: {}", e);
            log::trace!("exit: add_filter_to_list_of_ferox_filters -> false");
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
        CONFIGURATION.add_slash,
        &CONFIGURATION.queries,
        None,
    ) {
        urls.push(url); // default request, i.e. no extension
    }

    for ext in extensions.iter() {
        if let Ok(url) = format_url(
            &target_url,
            &word,
            CONFIGURATION.add_slash,
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
pub fn should_filter_response(response: &FeroxResponse) -> bool {
    if CONFIGURATION
        .filter_size
        .contains(&response.content_length())
    || CONFIGURATION
        .filter_line_count
        .contains(&response.line_count())
    || CONFIGURATION
        .filter_word_count
        .contains(&response.word_count())
    {
        // filtered value from --filter-size, size filters and wildcards are two separate filters
        // and are applied independently
        log::debug!("size filter: filtered out {}", response.url());
        return true;
    }

    match FILTERS.read() {
        Ok(filters) => {
            for filter in filters.iter() {
                // wildcard.should_filter goes here
                if filter.should_filter_response(&response) {
                    return true;
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
            if !CONFIGURATION.no_recursion {
                try_recursion(&ferox_response, base_depth, dir_chan.clone()).await;
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not

            if should_filter_response(&ferox_response) {
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
                        CONFIGURATION.add_slash,
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
                    if should_filter_response(&new_ferox_response) {
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

                    if !CONFIGURATION.no_recursion {
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
        CALL_COUNT.fetch_add(1, Ordering::Relaxed);

        // this protection allows us to add the first scanned url to SCANNED_URLS
        // from within the scan_url function instead of the recursion handler
        add_url_to_list_of_scanned_urls(&target_url, &SCANNED_URLS);

        if CONFIGURATION.scan_limit == 0 {
            // scan_limit == 0 means no limit should be imposed... however, scoping the Semaphore
            // permit is tricky, so as a workaround, we'll add a ridiculous number of permits to
            // the semaphore (1,152,921,504,606,846,975 to be exact) and call that 'unlimited'
            SCAN_LIMITER.add_permits(usize::MAX >> 4);
        }
    }

    // When acquire is called and the semaphore has remaining permits, the function immediately
    // returns a permit. However, if no remaining permits are available, acquire (asynchronously)
    // waits until an outstanding permit is dropped. At this point, the freed permit is assigned
    // to the caller.
    let permit = SCAN_LIMITER.acquire().await;

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

    // add any wildcard filters to `FILTERS`
    let filter =
        match heuristics::wildcard_test(&target_url, wildcard_bar, heuristics_file_clone).await {
            Some(f) => Box::new(f),
            None => Box::new(WildcardFilter::default()),
        };

    add_filter_to_list_of_ferox_filters(filter, FILTERS.clone());

    // add any status code filters to `FILTERS`
    for code_filter in &CONFIGURATION.filter_status {
        let filter = StatusCodeFilter {
            filter_code: *code_filter,
        };
        let boxed_filter = Box::new(filter);
        add_filter_to_list_of_ferox_filters(boxed_filter, FILTERS.clone());
    }

    // producer tasks (mp of mpsc); responsible for making requests
    let producers = stream::iter(looping_words.deref().to_owned())
        .map(|word| {
            let txd = tx_dir.clone();
            let txr = tx_term.clone();
            let pb = progress_bar.clone(); // progress bar is an Arc around internal state
            let tgt = target_url.to_string(); // done to satisfy 'static lifetime below
            (
                tokio::spawn(async move {
                    if PAUSE_SCAN.load(Ordering::Acquire) {
                        // for every word in the wordlist, check to see if PAUSE_SCAN is set to true
                        // when true; enter a busy loop that only exits by setting PAUSE_SCAN back
                        // to false
                        pause_scan().await;
                    }
                    make_requests(&tgt, &word, base_depth, txd, txr).await
                }),
                pb,
            )
        })
        .for_each_concurrent(CONFIGURATION.threads, |(resp, bar)| async move {
            match resp.await {
                Ok(_) => {
                    bar.inc((CONFIGURATION.extensions.len() + 1) as u64);
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

    // drop the current permit so the semaphore will allow another scan to proceed
    drop(permit);

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
                .insert("http://unknown_url".to_string()),
            true
        );

        assert_eq!(add_url_to_list_of_scanned_urls(url, &urls), false);
    }

    #[test]
    /// test that get_single_spinner returns the correct spinner
    fn scanner_get_single_spinner_returns_spinner() {
        let spinner = get_single_spinner();
        assert!(!spinner.is_finished());
    }

    #[tokio::test(core_threads = 1)]
    /// tests that pause_scan pauses execution and releases execution when PAUSE_SCAN is toggled
    /// the spinner used during the test has had .finish_and_clear called on it, meaning that
    /// a new one will be created, taking the if branch within the function
    async fn scanner_pause_scan_with_finished_spinner() {
        let now = time::Instant::now();

        PAUSE_SCAN.store(true, Ordering::Relaxed);
        SINGLE_SPINNER.write().unwrap().finish_and_clear();

        let expected = time::Duration::from_secs(2);

        tokio::spawn(async move {
            time::delay_for(expected).await;
            PAUSE_SCAN.store(false, Ordering::Relaxed);
        });

        pause_scan().await;

        assert!(now.elapsed() > expected);
    }
}
