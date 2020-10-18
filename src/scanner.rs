use crate::config::{CONFIGURATION, PROGRESS_BAR};
use crate::extractor::get_links;
use crate::heuristics::WildcardFilter;
use crate::utils::{
    format_url, get_current_depth, get_unique_words_from_wordlist, get_url_path_length,
    make_request, module_colorizer, response_is_directory, status_colorizer,
};
use crate::{heuristics, progress, FeroxChannel, FeroxResult};
use futures::future::{BoxFuture, FutureExt};
use futures::{stream, StreamExt};
use lazy_static::lazy_static;
use reqwest::{Response, Url};
use std::collections::HashSet;
use std::convert::TryInto;
use std::iter::FromIterator;
use std::ops::Deref;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

/// Single atomic number that gets incremented once, used to track first scan vs. all others
static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    /// Set of urls that have been sent to [scan_url](fn.scan_url.html), used for deduplication
    static ref SCANNED_URLS: RwLock<HashSet<String>> = RwLock::new(HashSet::new());
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

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives Urls and scans them
fn spawn_recursion_handler(
    mut recursion_channel: UnboundedReceiver<String>,
    wordlist: Arc<HashSet<String>>,
    base_depth: usize,
    tx_term: UnboundedSender<Response>,
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
    response: &Response,
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

/// Wrapper for [make_request](fn.make_request.html)
///
/// Handles making multiple requests based on the presence of extensions
///
/// Attempts recursion when appropriate and sends Responses to the report handler for processing
async fn make_requests(
    target_url: &str,
    word: &str,
    base_depth: usize,
    filter: Arc<WildcardFilter>,
    dir_chan: UnboundedSender<String>,
    report_chan: UnboundedSender<Response>,
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
            // response came back without error

            // do recursion if appropriate
            if !CONFIGURATION.norecursion && response_is_directory(&response) {
                try_recursion(&response, base_depth, dir_chan.clone()).await;
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not be

            let content_len = &response.content_length().unwrap_or(0);

            if CONFIGURATION.sizefilters.contains(content_len) {
                // filtered value from --sizefilters, move on to the next url
                log::debug!("size filter: filtered out {}", response.url());
                continue;
            }

            if filter.size > 0 && filter.size == *content_len && !CONFIGURATION.dontfilter {
                // static wildcard size found during testing
                // size isn't default, size equals response length, and auto-filter is on
                log::debug!("static wildcard: filtered out {}", response.url());
                continue;
            }

            if filter.dynamic > 0 && !CONFIGURATION.dontfilter {
                // dynamic wildcard offset found during testing

                // I'm about to manually split this url path instead of using reqwest::Url's
                // builtin parsing. The reason is that they call .split() on the url path
                // except that I don't want an empty string taking up the last index in the
                // event that the url ends with a forward slash.  It's ugly enough to be split
                // into its own function for readability.
                let url_len = get_url_path_length(&response.url());

                if url_len + filter.dynamic == *content_len {
                    log::debug!("dynamic wildcard: filtered out {}", response.url());
                    continue;
                }
            }

            // everything else should be reported
            match report_chan.send(response) {
                Ok(_) => {
                    log::debug!("sent {}/{} over reporting channel", &target_url, &word);
                }
                Err(e) => {
                    log::error!("wtf: {}", e);
                }
            }
        }
    }
    log::trace!("exit: make_requests");
}

// /// Simple helper to determine whether the given url has been scanned already or not
// fn url_has_been_scanned(url: &str, scanned_urls: &RwLock<HashSet<String>>) -> bool {
//     log::trace!("enter: url_has_been_scanned({}, {:?})", url, scanned_urls);
//
//     match scanned_urls.read() {
//         Ok(urls) => {
//             log::warn!("scanned urls: {:?}", urls);  // todo remove
//             let seen = urls.contains(url);
//             log::trace!("exit: url_has_been_scanned -> {}", seen);
//             seen
//         }
//         Err(e) => {
//             // poisoned lock
//             log::error!("Set of scanned urls poisoned: {}", e);
//             log::trace!("exit: url_has_been_scanned -> false");
//             false
//         }
//     }
// }

/// todo doc
pub async fn extract_new_content_from_response(
    response: Response,
    response_sender: UnboundedSender<Response>,
    file_sender: UnboundedSender<String>,
) {
    // todo trace
    log::trace!("enter: extract_new_content_from_response({:?})", response);

    // response should have a [1,2,4,5]xx status code, based on if branch of caller

    // get set of strings that are full urls
    // get_links internally consumes the Response given
    let new_links: Vec<String> = Vec::from_iter(get_links(response).await);
    log::info!("Extracted links: {:?}", new_links);

    for new_link in new_links {
        let unknown = add_url_to_list_of_scanned_urls(&new_link, &SCANNED_URLS);

        if !unknown {
            // not unknown, i.e. we've seen the url before and don't need to scan again
            continue;
        }

        let _test_url = Url::parse(&new_link).unwrap();

        // if test_url.query_pairs().count() > 0 {
        //     // very likely a file, simply request and report
        //     // todo: request and report
        //     continue;
        // }
        // else if test_url.path_segments().unwrap().last().unwrap().contains('.') {
        //     // might be a file, pure supposition, but should cut down on noise significantly
        //     // todo: request and report
        //     continue;
        // }

        // haven't seen this url before, scan it

        let scan_links = vec![new_link];
        scan(scan_links, response_sender.clone(), file_sender.clone())
            .await
            .unwrap();
    }

    drop(file_sender);
    drop(response_sender);
    // if !new_links.is_empty() {
    //     scan(new_links, response_sender.clone(), file_sender.clone()).await.unwrap();
    // }

    // for link in new_links {
    //     if url_has_been_scanned(&link, &SCANNED_URLS) {
    //         // this url has been processed before, skip it
    //         continue;
    //     }
    //
    //     // create a Url type from string
    //     let url = Url::parse(&link);
    //
    //     if url.is_err() {
    //         continue;
    //     }
    //     // url is Ok() at this point; safe to unwrap
    //
    //     // make a request to the newly discovered url
    //     match make_request(&CONFIGURATION.client, &url.unwrap()).await {
    //         Ok(response) => {
    //             if !CONFIGURATION.norecursion && response_is_directory(&response) {
    //                 try_recursion(&response, base_depth, dir_chan.clone()).await;
    //             }
    //             else {
    //                 // todo make a helper function
    //                 match report_chan.send(response) {
    //                     Ok(_) => {
    //                         log::debug!("sent {} over reporting channel", &link);
    //                     }
    //                     Err(e) => {
    //                         log::error!("wtf: {}", e);
    //                     }
    //                 }
    //             }
    //         }
    //         Err(e) => {
    //             log::error!("{}", e);
    //         }
    //     }
    // }
    log::trace!("exit: extract_new_content_from_response");
}

/// Determine whether it's a single url scan or urls are coming from stdin, then scan as needed
pub async fn scan(
    targets: Vec<String>,
    tx_term: UnboundedSender<Response>,
    tx_file: UnboundedSender<String>,
) -> FeroxResult<()> {
    log::trace!("enter: scan({:?}, {:?}, {:?})", targets, tx_term, tx_file);
    // cloning an Arc is cheap (it's basically a pointer into the heap)
    // so that will allow for cheap/safe sharing of a single wordlist across multi-target scans
    // as well as additional directories found as part of recursion
    let words =
        tokio::spawn(async move { get_unique_words_from_wordlist(&CONFIGURATION.wordlist) })
            .await??;

    if words.len() == 0 {
        eprintln!(
            "{} {} Did not find any words in {}",
            status_colorizer("ERROR"),
            module_colorizer("main::scan"),
            CONFIGURATION.wordlist
        );
        process::exit(1);
    }

    let mut tasks = vec![];

    for target in targets {
        let word_clone = words.clone();
        let term_clone = tx_term.clone();
        let file_clone = tx_file.clone();

        let task = tokio::spawn(async move {
            let base_depth = get_current_depth(&target);
            scan_url(&target, word_clone, base_depth, term_clone, file_clone).await;
        });

        tasks.push(task);
    }

    // drive execution of all accumulated futures
    futures::future::join_all(tasks).await;
    log::trace!("exit: scan");

    drop(tx_file);
    drop(tx_term);

    Ok(())
}

/// Scan a given url using a given wordlist
///
/// This is the primary entrypoint for the scanner
pub async fn scan_url(
    target_url: &str,
    wordlist: Arc<HashSet<String>>,
    base_depth: usize,
    tx_term: UnboundedSender<Response>,
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

    let (tx_dir, rx_dir): FeroxChannel<String> = mpsc::unbounded_channel(); // todo

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

    // producer tasks (mp of mpsc); responsible for making requests
    let producers = stream::iter(looping_words.deref().to_owned())
        .map(|word| {
            let wc_filter = filter.clone();
            let txd = tx_dir.clone();
            let txr = tx_term.clone();
            let pb = progress_bar.clone(); // progress bar is an Arc around internal state
            let tgt = target_url.to_string(); // done to satisfy 'static lifetime below
            (
                tokio::spawn(async move {
                    make_requests(&tgt, &word, base_depth, wc_filter, txd, txr).await
                }),
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

    // todo test url_has_been_scanned
}
