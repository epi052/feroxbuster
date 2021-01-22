use crate::{
    config::{Configuration, CONFIGURATION},
    event_handlers::{
        Command::{self, DecrementActiveScans, UpdateF64Field, UpdateUsizeField},
        Handles,
    },
    extractor::ExtractorBuilder,
    filters::{
        LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter, WildcardFilter,
        WordsFilter,
    },
    heuristics,
    scan_manager::{FeroxResponses, FeroxScans, ScanStatus, PAUSE_SCAN},
    send_command,
    statistics::StatField::{DirScanTimes, ExpectedPerScan, WildcardsFiltered},
    traits::FeroxFilter,
    utils::{format_url, get_current_depth, make_request},
    CommandSender, FeroxResponse, SIMILARITY_THRESHOLD,
};
use futures::{stream, StreamExt};
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
    sync::atomic::Ordering,
    sync::{Arc, RwLock},
    time::Instant,
};
use tokio::sync::{mpsc::UnboundedSender, Semaphore};

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
    // todo move to filters handler

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

// /// Spawn a single consumer task (sc side of mpsc)
// ///
// /// The consumer simply receives Urls and scans them
// fn spawn_recursion_handler(
//     mut recursion_channel: UnboundedReceiver<String>,
//     wordlist: Arc<HashSet<String>>,
//     base_depth: usize,
//     stats: Arc<Stats>,
//     tx_term: CommandSender,
//     tx_stats: CommandSender,
// ) -> BoxFuture<'static, ()> {
//     log::trace!(
//         "enter: spawn_recursion_handler({:?}, wordlist[{} words...], {}, {:?}, {:?}, {:?})",
//         recursion_channel,
//         wordlist.len(),
//         base_depth,
//         stats,
//         tx_term,
//         tx_stats
//     );
//
//     async move {
//         while let Some(resp) = recursion_channel.recv().await {
//             let (unknown, scan) = SCANNED_URLS.add_directory_scan(&resp, stats.clone());
//
//             if !unknown {
//                 // not unknown, i.e. we've seen the url before and don't need to scan again
//                 continue;
//             }
//
//             send_command!(tx_stats, UpdateUsizeField(TotalScans, 1));
//
//             log::info!("received {} on recursion channel", resp);
//
//             let term_clone = tx_term.clone();
//             let tx_stats_clone = tx_stats.clone();
//             let stats_clone = stats.clone();
//             let resp_clone = resp.clone();
//             let list_clone = wordlist.clone();
//
//             let future = tokio::spawn(async move {
//                 scan_url(
//                     resp_clone.to_owned().as_str(),
//                     list_clone,
//                     stats_clone,
//                     term_clone,
//                     tx_stats_clone,
//                 )
//                 .await
//             });
//
//             if let Ok(mut u_scan) = scan.lock() {
//                 u_scan.task = Some(future);
//             };
//         }
//         log::trace!("exit: spawn_recursion_handler -> BoxFuture<'static, ()>");
//     }
//     .boxed()
// }
// todo remove above

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
    tx_stats: UnboundedSender<Command>,
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
    // todo move to feroxscan
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
    // todo move to feroxscan

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
pub async fn try_recursion(
    response: &FeroxResponse,
    base_depth: usize,
    transmitter: CommandSender,
) {
    // todo this should be part of the recursion handler
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

            match transmitter.send(Command::ScanUrl(String::from(response.url().as_str()))) {
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

            match transmitter.send(Command::ScanUrl(new_url)) {
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
    tx_stats: UnboundedSender<Command>,
) -> bool {
    // todo move to ... feroxscan ?? seems like it could be placed elsewhere, but it's a tougher choice than other fns
    // perhaps wait and add it to w/e struct we use for the scanner module
    match FILTERS.read() {
        Ok(filters) => {
            for filter in filters.iter() {
                // wildcard.should_filter goes here
                if filter.should_filter_response(&response) {
                    if filter.as_any().downcast_ref::<WildcardFilter>().is_some() {
                        send_command!(tx_stats, UpdateUsizeField(WildcardsFiltered, 1))
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
/// Makes multiple requests based on the presence of extensions
///
/// Attempts recursion when appropriate and sends Responses to the output handler for processing
async fn make_requests(target_url: &str, word: &str, base_depth: usize, handles: Arc<Handles>) {
    log::trace!(
        "enter: make_requests({}, {}, {}, {:?})",
        target_url,
        word,
        base_depth,
        handles
    );

    let urls = create_urls(
        &target_url,
        &word,
        &CONFIGURATION.extensions,
        handles.stats.tx.clone(),
    );

    let scanned_urls = handles.scans.read().unwrap().as_ref().unwrap().data.clone();
    let tx_scans = handles.scans.read().unwrap().as_ref().unwrap().tx.clone();

    for url in urls {
        if let Ok(response) =
            make_request(&CONFIGURATION.client, &url, handles.stats.tx.clone()).await
        {
            // response came back without error, convert it to FeroxResponse
            let ferox_response = FeroxResponse::from(response, true).await;

            // do recursion if appropriate
            if !CONFIGURATION.no_recursion {
                // todo abstract
                try_recursion(&ferox_response, base_depth, tx_scans.clone()).await;
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not

            if should_filter_response(&ferox_response, handles.stats.tx.clone()) {
                continue;
            }

            if CONFIGURATION.extract_links && !ferox_response.status().is_redirection() {
                // todo extractor should probably just take Handles
                let extractor = ExtractorBuilder::with_response(&ferox_response)
                    .depth(base_depth)
                    .config(&CONFIGURATION)
                    .recursion_transmitter(tx_scans.clone())
                    .stats_transmitter(handles.stats.tx.clone())
                    .reporter_transmitter(handles.output.tx.clone())
                    // todo abstract scanned_urls
                    .scanned_urls(scanned_urls.clone())
                    .stats(handles.stats.data.clone())
                    .build()
                    .unwrap(); // todo change once this function returns Result

                let _ = extractor.extract().await;
            }

            // everything else should be reported
            send_report(handles.output.tx.clone(), ferox_response);
        }
    }
    log::trace!("exit: make_requests");
}

/// Simple helper to send a `FeroxResponse` over the tx side of an `mpsc::unbounded_channel`
pub fn send_report(report_sender: CommandSender, response: FeroxResponse) {
    log::trace!("enter: send_report({:?}, {}", report_sender, response);

    match report_sender.send(Command::Report(Box::new(response))) {
        Ok(_) => {}
        Err(e) => {
            log::warn!("{}", e);
            // todo back to error
        }
    }

    log::trace!("exit: send_report");
}

#[derive(Debug, Copy, Clone)]
/// todo doc and is this the right location?
pub enum ScanOrder {
    /// todo
    Initial,

    /// todo
    Latest,
}

/// Scan a given url using a given wordlist
///
/// This is the primary entrypoint for the scanner
pub async fn scan_url(
    target_url: &str,
    order: ScanOrder,
    wordlist: Arc<HashSet<String>>,
    handles: Arc<Handles>,
) {
    log::trace!(
        "enter: scan_url({:?}, {:?}, wordlist[{} words...], {:?})",
        target_url,
        order,
        wordlist.len(),
        handles
    );

    let depth = get_current_depth(&target_url);

    log::info!("Starting scan against: {}", target_url);

    let scan_timer = Instant::now();

    if matches!(order, ScanOrder::Initial) {
        if CONFIGURATION.extract_links {
            // only grab robots.txt on the initial scan_url calls. all fresh dirs will be passed
            // to try_recursion

            let extractor = ExtractorBuilder::with_url(target_url)
                .depth(depth)
                .config(&CONFIGURATION)
                .recursion_transmitter(handles.scans.read().unwrap().as_ref().unwrap().tx.clone())
                .stats_transmitter(handles.stats.tx.clone())
                .reporter_transmitter(handles.output.tx.clone())
                // todo abstract scanned_urls
                .scanned_urls(handles.scans.read().unwrap().as_ref().unwrap().data.clone())
                .stats(handles.stats.data.clone())
                .build()
                .unwrap(); // todo change once this function returns Result

            let _ = extractor.extract().await;
        }
    }

    // todo abstract away scan.get_scan probably
    let ferox_scan = match handles
        .scans
        .read()
        .unwrap()
        .as_ref()
        .unwrap()
        .data
        .get_scan_by_url(&target_url)
    {
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
    // todo can be moved to scan handler, just acquire before calling scan

    // Arc clones to be passed around to the various scans
    let wildcard_bar = progress_bar.clone();
    let looping_words = wordlist.clone();

    // add any wildcard filters to `FILTERS`
    // todo if you want to remove the 0-based skipping of wildcards, this needs addressed
    // todo wildcard_test should take handles probably? idk, could see tradeoff between memsize
    // of two clones vs the handles clone
    log::error!("Size of Handles: {}", std::mem::size_of::<Handles>());
    log::error!(
        "Size of Arc<Handles>: {}",
        std::mem::size_of::<Arc<Handles>>()
    );
    log::error!(
        "Size of CommandSender: {}",
        std::mem::size_of::<CommandSender>()
    );
    log::error!("Size of Handles: {}", std::mem::size_of::<Arc<Handles>>());
    // todo remove above

    let filter = match heuristics::wildcard_test(
        &target_url,
        wildcard_bar,
        handles.output.tx.clone(),
        handles.stats.tx.clone(),
    )
    .await
    {
        Some(f) => Box::new(f),
        None => Box::new(WildcardFilter::default()),
    };

    add_filter_to_list_of_ferox_filters(filter, FILTERS.clone());
    let scanned_urls = handles
        .scans
        .read()
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .data
        .clone();

    // producer tasks (mp of mpsc); responsible for making requests
    // todo .deref().to_owned() seems like they cancel eachother out
    let producers = stream::iter(looping_words.deref().to_owned())
        .map(|word| {
            // todo abstract away more scans shit
            let handles_clone = handles.clone();
            let txs = handles.stats.tx.clone();
            let pb = progress_bar.clone(); // progress bar is an Arc around internal state
            let tgt = target_url.to_string(); // done to satisfy 'static lifetime below
            let scanned_urls_clone = scanned_urls.clone();
            (
                tokio::spawn(async move {
                    if PAUSE_SCAN.load(Ordering::Acquire) {
                        // for every word in the wordlist, check to see if PAUSE_SCAN is set to true
                        // when true; enter a busy loop that only exits by setting PAUSE_SCAN back
                        // to false
                        let num_cancelled = scanned_urls_clone.pause(true).await;
                        for _ in 0..num_cancelled {
                            txs.send(DecrementActiveScans).unwrap_or_default();
                        }
                    }
                    // todo the sender for dir_chan (change name) is <String>, i.e. going nowhere
                    make_requests(&tgt, &word, depth, handles_clone).await
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

    send_command!(
        handles.stats.tx,
        UpdateF64Field(DirScanTimes, scan_timer.elapsed().as_secs_f64())
    );

    // drop the current permit so the semaphore will allow another scan to proceed
    drop(permit);

    if let Ok(mut scan) = ferox_scan.lock() {
        scan.finish();
    }

    // todo remove
    // // manually drop tx in order for the rx task's while loops to eval to false
    // log::trace!("dropped recursion handler's transmitter");
    // drop(tx_dir);

    handles
        .stats
        .tx
        .send(DecrementActiveScans)
        .unwrap_or_default();

    // note: in v1.11.2 i removed the join_all call that used to handle the recurser handles.
    // nothing appears to change by having them removed, however, if ever a revert is needed
    // this is the place and anything prior to 1.11.2 will have the code to do so
    // {
    //     if let Ok(urls) = SCANNED_URLS.get_scan_by_url(target_url).unwrap().lock() {
    //         urls.task.as_ref().unwrap().into_inner().unwrap().await;
    //     }
    // }

    // todo remove
    log::error!("SCAN URL EXIT: {}", target_url);

    // for mut fut in futures {
    //     let x = Arc::try_unwrap(fut).unwrap();
    //     x.await;
    // }

    log::trace!("exit: scan_url");
}

/// Perform steps necessary to run scans that only need to be performed once (warming up the
/// engine, as it were)
pub async fn initialize(
    num_words: usize,
    config: &Configuration,
    tx_stats: UnboundedSender<Command>,
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
    send_command!(
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
        let (tx, _): FeroxChannel<Command> = mpsc::unbounded_channel();
        let urls = create_urls("http://localhost", "turbo", &[], tx);
        assert_eq!(urls, [Url::parse("http://localhost/turbo").unwrap()])
    }

    #[test]
    /// sending url + word + 1 extension should get back two urls, one base and one with extension
    fn create_urls_one_extension_returns_two_urls() {
        let (tx, _): FeroxChannel<Command> = mpsc::unbounded_channel();
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

        let (tx, _): FeroxChannel<Command> = mpsc::unbounded_channel();

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
        let (tx, _): FeroxChannel<Command> = mpsc::unbounded_channel();
        initialize(1, &config, tx).await;
    }
}
