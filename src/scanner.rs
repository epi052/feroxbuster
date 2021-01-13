use crate::{
    config::{Configuration, CONFIGURATION},
    extractor::{extract_robots_txt, get_links, request_feroxresponse_from_new_link},
    filters::{
        FeroxFilter, LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter,
        WildcardFilter, WordsFilter,
    },
    heuristics,
    scan_manager::{FeroxResponses, FeroxScans, ScanStatus, PAUSE_SCAN},
    statistics::{
        StatCommand::{self, UpdateF64Field, UpdateUsizeField},
        StatField::{DirScanTimes, ExpectedPerScan, TotalScans, WildcardsFiltered},
        Stats,
    },
    utils::{format_url, get_current_depth, make_request},
    FeroxChannel, FeroxResponse, SIMILARITY_THRESHOLD,
};
use futures::{
    future::{BoxFuture, FutureExt},
    stream, StreamExt,
};
use fuzzyhash::FuzzyHash;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::{StatusCode, Url};
#[cfg(not(test))]
use std::process::exit;
use std::{
    collections::HashSet,
    convert::TryInto,
    ops::Deref,
    sync::atomic::{AtomicUsize, Ordering},
    sync::{Arc, RwLock},
    time::Instant,
};
use tokio::{
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        Semaphore,
    },
    task::JoinHandle,
};

/// Single atomic number that gets incremented at least once, used to track first scan(s) vs. all
/// others found during recursion
///
/// -u means this will be incremented once
/// --stdin means this will be incremented by the number of targets passed via STDIN
static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    /// Set of urls that have been sent to [scan_url](fn.scan_url.html), used for deduplication
    pub static ref SCANNED_URLS: FeroxScans = FeroxScans::default();

    /// Vector of implementors of the FeroxFilter trait
    static ref FILTERS: Arc<RwLock<Vec<Box<dyn FeroxFilter>>>> = Arc::new(RwLock::new(Vec::<Box<dyn FeroxFilter>>::new()));

    /// Vector of FeroxResponse objects
    pub static ref RESPONSES: FeroxResponses = FeroxResponses::default();

    /// Bounded semaphore used as a barrier to limit concurrent scans
    static ref SCAN_LIMITER: Semaphore = Semaphore::new(CONFIGURATION.scan_limit);


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
    stats: Arc<Stats>,
    tx_term: UnboundedSender<FeroxResponse>,
    tx_file: UnboundedSender<FeroxResponse>,
    tx_stats: UnboundedSender<StatCommand>,
) -> BoxFuture<'static, Vec<Arc<JoinHandle<()>>>> {
    log::trace!(
        "enter: spawn_recursion_handler({:?}, wordlist[{} words...], {}, {:?}, {:?}, {:?}, {:?})",
        recursion_channel,
        wordlist.len(),
        base_depth,
        stats,
        tx_term,
        tx_file,
        tx_stats
    );

    let boxed_future = async move {
        let mut scans = vec![];

        while let Some(resp) = recursion_channel.recv().await {
            let (unknown, scan) = SCANNED_URLS.add_directory_scan(&resp, stats.clone());

            if !unknown {
                // not unknown, i.e. we've seen the url before and don't need to scan again
                continue;
            }

            update_stat!(tx_stats, UpdateUsizeField(TotalScans, 1));

            log::info!("received {} on recursion channel", resp);

            let term_clone = tx_term.clone();
            let file_clone = tx_file.clone();
            let tx_stats_clone = tx_stats.clone();
            let stats_clone = stats.clone();
            let resp_clone = resp.clone();
            let list_clone = wordlist.clone();

            let future = tokio::spawn(async move {
                scan_url(
                    resp_clone.to_owned().as_str(),
                    list_clone,
                    base_depth,
                    stats_clone,
                    term_clone,
                    file_clone,
                    tx_stats_clone,
                )
                .await
            });

            let shared_task = Arc::new(future);

            if let Ok(mut u_scan) = scan.lock() {
                u_scan.task = Some(shared_task.clone());
            }

            scans.push(shared_task);
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
fn create_urls(
    target_url: &str,
    word: &str,
    extensions: &[String],
    tx_stats: UnboundedSender<StatCommand>,
) -> Vec<Url> {
    log::trace!(
        "enter: create_urls({}, {}, {:?}, {:?})",
        target_url,
        word,
        extensions,
        tx_stats
    );

    let mut urls = vec![];

    if let Ok(url) = format_url(
        &target_url,
        &word,
        CONFIGURATION.add_slash,
        &CONFIGURATION.queries,
        None,
        tx_stats.clone(),
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
            tx_stats.clone(),
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
    log::trace!("enter: is_directory({})", response);

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
                log::debug!("expected Location header, but none was found: {}", response);
                log::trace!("exit: is_directory -> false");
                return false;
            }
        }
    } else if response.status().is_success() || matches!(response.status(), &StatusCode::FORBIDDEN)
    {
        // status code is 2xx or 403, need to check if it ends in /

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
        "enter: try_recursion({}, {}, {:?})",
        response,
        base_depth,
        transmitter,
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
pub fn should_filter_response(
    response: &FeroxResponse,
    tx_stats: UnboundedSender<StatCommand>,
) -> bool {
    match FILTERS.read() {
        Ok(filters) => {
            for filter in filters.iter() {
                // wildcard.should_filter goes here
                if filter.should_filter_response(&response) {
                    if filter.as_any().downcast_ref::<WildcardFilter>().is_some() {
                        update_stat!(tx_stats, UpdateUsizeField(WildcardsFiltered, 1))
                    }
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
    stats: Arc<Stats>,
    dir_chan: UnboundedSender<String>,
    report_chan: UnboundedSender<FeroxResponse>,
    tx_stats: UnboundedSender<StatCommand>,
) {
    log::trace!(
        "enter: make_requests({}, {}, {}, {:?}, {:?}, {:?}, {:?})",
        target_url,
        word,
        base_depth,
        stats,
        dir_chan,
        report_chan,
        tx_stats
    );

    let urls = create_urls(
        &target_url,
        &word,
        &CONFIGURATION.extensions,
        tx_stats.clone(),
    );

    for url in urls {
        if let Ok(response) = make_request(&CONFIGURATION.client, &url, tx_stats.clone()).await {
            // response came back without error, convert it to FeroxResponse
            let ferox_response = FeroxResponse::from(response, true).await;

            // do recursion if appropriate
            if !CONFIGURATION.no_recursion {
                try_recursion(&ferox_response, base_depth, dir_chan.clone()).await;
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not

            if should_filter_response(&ferox_response, tx_stats.clone()) {
                continue;
            }

            if CONFIGURATION.extract_links && !ferox_response.status().is_redirection() {
                let new_links = get_links(&ferox_response, tx_stats.clone()).await;

                for new_link in new_links {
                    let mut new_ferox_response = match request_feroxresponse_from_new_link(
                        &new_link,
                        tx_stats.clone(),
                    )
                    .await
                    {
                        Some(resp) => resp,
                        None => continue,
                    };

                    // filter if necessary
                    if should_filter_response(&new_ferox_response, tx_stats.clone()) {
                        continue;
                    }

                    if new_ferox_response.is_file() {
                        // very likely a file, simply request and report
                        log::debug!("Singular extraction: {}", new_ferox_response);

                        SCANNED_URLS
                            .add_file_scan(&new_ferox_response.url().to_string(), stats.clone());

                        send_report(report_chan.clone(), new_ferox_response);

                        continue;
                    }

                    if !CONFIGURATION.no_recursion {
                        log::debug!("Recursive extraction: {}", new_ferox_response);

                        if !new_ferox_response.url().as_str().ends_with('/')
                            && (new_ferox_response.status().is_success()
                                || matches!(new_ferox_response.status(), &StatusCode::FORBIDDEN))
                        {
                            // if the url doesn't end with a /
                            // and the response code is either a 2xx or 403

                            // since all of these are 2xx or 403, recursion is only attempted if the
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
pub fn send_report(report_sender: UnboundedSender<FeroxResponse>, response: FeroxResponse) {
    log::trace!("enter: send_report({:?}, {}", report_sender, response);

    match report_sender.send(response) {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e);
        }
    }

    log::trace!("exit: send_report");
}

/// Request /robots.txt from given url
async fn scan_robots_txt(
    target_url: &str,
    base_depth: usize,
    stats: Arc<Stats>,
    tx_term: UnboundedSender<FeroxResponse>,
    tx_dir: UnboundedSender<String>,
    tx_stats: UnboundedSender<StatCommand>,
) {
    log::trace!(
        "enter: scan_robots_txt({}, {}, {:?}, {:?}, {:?}, {:?})",
        target_url,
        base_depth,
        stats,
        tx_term,
        tx_dir,
        tx_stats
    );

    let robots_links = extract_robots_txt(&target_url, &CONFIGURATION, tx_stats.clone()).await;

    for robot_link in robots_links {
        // create a url based on the given command line options, continue on error
        let mut ferox_response =
            match request_feroxresponse_from_new_link(&robot_link, tx_stats.clone()).await {
                Some(resp) => resp,
                None => continue,
            };

        if should_filter_response(&ferox_response, tx_stats.clone()) {
            continue;
        }

        if ferox_response.is_file() {
            log::debug!("File extracted from robots.txt: {}", ferox_response);
            SCANNED_URLS.add_file_scan(&robot_link, stats.clone());
            send_report(tx_term.clone(), ferox_response);
        } else if !CONFIGURATION.no_recursion {
            log::debug!("Directory extracted from robots.txt: {}", ferox_response);
            // todo this code is essentially the same as another piece around ~467 of this file
            if !ferox_response.url().as_str().ends_with('/')
                && (ferox_response.status().is_success()
                    || matches!(ferox_response.status(), &StatusCode::FORBIDDEN))
            {
                // if the url doesn't end with a /
                // and the response code is either a 2xx or 403
                ferox_response.set_url(&format!("{}/", ferox_response.url()));
            }

            try_recursion(&ferox_response, base_depth, tx_dir.clone()).await;
        }
    }
    log::trace!("exit: scan_robots_txt");
}

/// Scan a given url using a given wordlist
///
/// This is the primary entrypoint for the scanner
pub async fn scan_url(
    target_url: &str,
    wordlist: Arc<HashSet<String>>,
    base_depth: usize,
    stats: Arc<Stats>,
    tx_term: UnboundedSender<FeroxResponse>,
    tx_file: UnboundedSender<FeroxResponse>,
    tx_stats: UnboundedSender<StatCommand>,
) {
    log::trace!(
        "enter: scan_url({:?}, wordlist[{} words...], {}, {:?}, {:?}, {:?}, {:?})",
        target_url,
        wordlist.len(),
        base_depth,
        stats,
        tx_term,
        tx_file,
        tx_stats
    );

    log::info!("Starting scan against: {}", target_url);

    let scan_timer = Instant::now();

    let (tx_dir, rx_dir): FeroxChannel<String> = mpsc::unbounded_channel();

    if CALL_COUNT.load(Ordering::Relaxed) < stats.initial_targets.load(Ordering::Relaxed) {
        CALL_COUNT.fetch_add(1, Ordering::Relaxed);

        if CONFIGURATION.extract_links {
            // only grab robots.txt on the initial scan_url calls. all fresh dirs will be passed
            // to try_recursion
            scan_robots_txt(
                target_url,
                base_depth,
                stats.clone(),
                tx_term.clone(),
                tx_dir.clone(),
                tx_stats.clone(),
            )
            .await;
        }

        update_stat!(tx_stats, UpdateUsizeField(TotalScans, 1));

        // this protection allows us to add the first scanned url to SCANNED_URLS
        // from within the scan_url function instead of the recursion handler
        SCANNED_URLS.add_directory_scan(&target_url, stats.clone());
    }

    let ferox_scan = match SCANNED_URLS.get_scan_by_url(&target_url) {
        Some(scan) => {
            if let Ok(mut u_scan) = scan.lock() {
                u_scan.status = ScanStatus::Running;
            }
            scan
        }
        None => {
            log::error!(
                "Could not find FeroxScan associated with {}; this shouldn't happen... exiting",
                target_url
            );
            return;
        }
    };

    let progress_bar = match ferox_scan.lock() {
        Ok(mut scan) => scan.progress_bar(),
        Err(e) => {
            log::error!("FeroxScan's ({:?}) mutex is poisoned: {}", ferox_scan, e);
            return;
        }
    };

    // When acquire is called and the semaphore has remaining permits, the function immediately
    // returns a permit. However, if no remaining permits are available, acquire (asynchronously)
    // waits until an outstanding permit is dropped. At this point, the freed permit is assigned
    // to the caller.
    let permit = SCAN_LIMITER.acquire().await;

    // Arc clones to be passed around to the various scans
    let wildcard_bar = progress_bar.clone();
    let heuristics_term_clone = tx_term.clone();
    let heuristics_stats_clone = tx_stats.clone();
    let recurser_term_clone = tx_term.clone();
    let recurser_file_clone = tx_file.clone();
    let recurser_stats_clone = tx_stats.clone();
    let recurser_words = wordlist.clone();
    let looping_words = wordlist.clone();
    let looping_stats = stats.clone();

    let recurser = tokio::spawn(async move {
        spawn_recursion_handler(
            rx_dir,
            recurser_words,
            base_depth,
            stats.clone(),
            recurser_term_clone,
            recurser_file_clone,
            recurser_stats_clone,
        )
        .await
    });

    // add any wildcard filters to `FILTERS`
    let filter = match heuristics::wildcard_test(
        &target_url,
        wildcard_bar,
        heuristics_term_clone,
        heuristics_stats_clone,
    )
    .await
    {
        Some(f) => Box::new(f),
        None => Box::new(WildcardFilter::default()),
    };

    add_filter_to_list_of_ferox_filters(filter, FILTERS.clone());

    // producer tasks (mp of mpsc); responsible for making requests
    let producers = stream::iter(looping_words.deref().to_owned())
        .map(|word| {
            let txd = tx_dir.clone();
            let txr = tx_term.clone();
            let txs = tx_stats.clone();
            let pb = progress_bar.clone(); // progress bar is an Arc around internal state
            let tgt = target_url.to_string(); // done to satisfy 'static lifetime below
            let lst = looping_stats.clone();
            (
                tokio::spawn(async move {
                    if PAUSE_SCAN.load(Ordering::Acquire) {
                        // for every word in the wordlist, check to see if PAUSE_SCAN is set to true
                        // when true; enter a busy loop that only exits by setting PAUSE_SCAN back
                        // to false
                        SCANNED_URLS.pause(true).await;
                    }
                    make_requests(&tgt, &word, base_depth, lst, txd, txr, txs).await
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

    update_stat!(
        tx_stats,
        UpdateF64Field(DirScanTimes, scan_timer.elapsed().as_secs_f64())
    );

    // drop the current permit so the semaphore will allow another scan to proceed
    drop(permit);

    if let Ok(mut scan) = ferox_scan.lock() {
        scan.finish();
    }

    // manually drop tx in order for the rx task's while loops to eval to false
    log::trace!("dropped recursion handler's transmitter");
    drop(tx_dir);

    // note: in v1.11.2 i removed the join_all call that used to handle the recurser handles.
    // nothing appears to change by having them removed, however, if ever a revert is needed
    // this is the place and anything prior to 1.11.2 will have the code to do so
    let _ = recurser.await.unwrap_or_default();

    log::trace!("exit: scan_url");
}

/// Perform steps necessary to run scans that only need to be performed once (warming up the
/// engine, as it were)
pub async fn initialize(
    num_words: usize,
    config: &Configuration,
    tx_stats: UnboundedSender<StatCommand>,
) {
    log::trace!(
        "enter: initialize({}, {:?}, {:?})",
        num_words,
        config,
        tx_stats
    );

    // number of requests only needs to be calculated once, and then can be reused
    let num_reqs_expected: u64 = if config.extensions.is_empty() {
        num_words.try_into().unwrap()
    } else {
        let total = num_words * (config.extensions.len() + 1);
        total.try_into().unwrap()
    };

    // tell Stats object about the number of expected requests
    update_stat!(
        tx_stats,
        UpdateUsizeField(ExpectedPerScan, num_reqs_expected as usize)
    );

    // add any status code filters to `FILTERS` (-C|--filter-status)
    for code_filter in &config.filter_status {
        let filter = StatusCodeFilter {
            filter_code: *code_filter,
        };
        let boxed_filter = Box::new(filter);
        add_filter_to_list_of_ferox_filters(boxed_filter, FILTERS.clone());
    }

    // add any line count filters to `FILTERS` (-N|--filter-lines)
    for lines_filter in &config.filter_line_count {
        let filter = LinesFilter {
            line_count: *lines_filter,
        };
        let boxed_filter = Box::new(filter);
        add_filter_to_list_of_ferox_filters(boxed_filter, FILTERS.clone());
    }

    // add any line count filters to `FILTERS` (-W|--filter-words)
    for words_filter in &config.filter_word_count {
        let filter = WordsFilter {
            word_count: *words_filter,
        };
        let boxed_filter = Box::new(filter);
        add_filter_to_list_of_ferox_filters(boxed_filter, FILTERS.clone());
    }

    // add any line count filters to `FILTERS` (-S|--filter-size)
    for size_filter in &config.filter_size {
        let filter = SizeFilter {
            content_length: *size_filter,
        };
        let boxed_filter = Box::new(filter);
        add_filter_to_list_of_ferox_filters(boxed_filter, FILTERS.clone());
    }

    // add any regex filters to `FILTERS` (-X|--filter-regex)
    for regex_filter in &config.filter_regex {
        let raw = regex_filter;
        let compiled = match Regex::new(&raw) {
            Ok(regex) => regex,
            Err(e) => {
                log::error!("Invalid regular expression: {}", e);
                #[cfg(test)]
                panic!();
                #[cfg(not(test))]
                exit(1);
            }
        };

        let filter = RegexFilter {
            raw_string: raw.to_owned(),
            compiled,
        };
        let boxed_filter = Box::new(filter);
        add_filter_to_list_of_ferox_filters(boxed_filter, FILTERS.clone());
    }

    // add any similarity filters to `FILTERS` (--filter-similar-to)
    for similarity_filter in &config.filter_similar {
        // url as-is based on input, ignores user-specified url manipulation options (add-slash etc)
        if let Ok(url) = format_url(
            &similarity_filter,
            &"",
            false,
            &Vec::new(),
            None,
            tx_stats.clone(),
        ) {
            // attempt to request the given url
            if let Ok(resp) = make_request(&CONFIGURATION.client, &url, tx_stats.clone()).await {
                // if successful, create a filter based on the response's body
                let fr = FeroxResponse::from(resp, true).await;

                // hash the response body and store the resulting hash in the filter object
                let hash = FuzzyHash::new(&fr.text()).to_string();

                let filter = SimilarityFilter {
                    text: hash,
                    threshold: SIMILARITY_THRESHOLD,
                };

                let boxed_filter = Box::new(filter);
                add_filter_to_list_of_ferox_filters(boxed_filter, FILTERS.clone());
            }
        }
    }

    if config.scan_limit == 0 {
        // scan_limit == 0 means no limit should be imposed... however, scoping the Semaphore
        // permit is tricky, so as a workaround, we'll add a ridiculous number of permits to
        // the semaphore (1,152,921,504,606,846,975 to be exact) and call that 'unlimited'
        SCAN_LIMITER.add_permits(usize::MAX >> 4);
    }

    log::trace!("exit: initialize");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// sending url + word without any extensions should get back one url with the joined word
    fn create_urls_no_extension_returns_base_url_with_word() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        let urls = create_urls("http://localhost", "turbo", &[], tx);
        assert_eq!(urls, [Url::parse("http://localhost/turbo").unwrap()])
    }

    #[test]
    /// sending url + word + 1 extension should get back two urls, one base and one with extension
    fn create_urls_one_extension_returns_two_urls() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        let urls = create_urls("http://localhost", "turbo", &[String::from("js")], tx);
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

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        for (i, ext_set) in ext_vec.into_iter().enumerate() {
            let urls = create_urls("http://localhost", "turbo", &ext_set, tx.clone());
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[should_panic]
    /// call initialize with a bad regex, triggering a panic
    async fn initialize_panics_on_bad_regex() {
        let config = Configuration {
            filter_regex: vec![r"(".to_string()],
            ..Default::default()
        };
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
        initialize(1, &config, tx).await;
    }
}
