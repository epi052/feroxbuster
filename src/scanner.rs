use crate::{
    config::{Configuration, CONFIGURATION},
    event_handlers::{
        Command::{self, AddFilter, UpdateF64Field, UpdateUsizeField},
        Handles,
    },
    extractor::ExtractorBuilder,
    filters::{
        LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter, WildcardFilter,
        WordsFilter,
    },
    heuristics,
    scan_manager::{FeroxResponses, FeroxScans, ScanOrder, ScanStatus, PAUSE_SCAN},
    statistics::StatField::{DirScanTimes, ExpectedPerScan},
    traits::FeroxFilter,
    utils::{fmt_err, format_url, get_url_depth, make_request},
    CommandSender, FeroxResponse, SIMILARITY_THRESHOLD,
};
use anyhow::{bail, Result};
use futures::{stream, StreamExt};
use fuzzyhash::FuzzyHash;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Url;
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

    let scanned_urls = handles.ferox_scans().expect("Could not get FeroxScans");
    // todo abstract away, and by that i mean that extractor and try_recursion should either take
    // Handles or be put into a struct somewhere
    let tx_scans = handles.scans.read().unwrap().as_ref().unwrap().tx.clone();

    for url in urls {
        if let Ok(response) =
            make_request(&CONFIGURATION.client, &url, handles.stats.tx.clone()).await
        {
            // response came back without error, convert it to FeroxResponse
            let ferox_response = FeroxResponse::from(response, true).await;

            // do recursion if appropriate
            if !CONFIGURATION.no_recursion {
                tx_scans
                    .send(Command::TryRecursion(ferox_response.clone()))
                    .unwrap_or_else(|e| log::warn!("Could not send {} for recursion: {}", url, e));
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not
            if handles
                .filters
                .data
                .should_filter_response(&ferox_response, handles.stats.tx.clone())
            {
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
    order: ScanOrder,
    wordlist: Arc<HashSet<String>>,
    handles: Arc<Handles>,
) -> Result<()> {
    log::trace!(
        "enter: scan_url({:?}, {:?}, wordlist[{} words...], {:?})",
        target_url,
        order,
        wordlist.len(),
        handles
    );

    let depth = get_url_depth(&target_url); // todo

    log::info!("Starting scan against: {}", target_url);

    let scan_timer = Instant::now();

    if matches!(order, ScanOrder::Initial) && CONFIGURATION.extract_links {
        // only grab robots.txt on the initial scan_url calls. all fresh dirs will be passed
        // to try_recursion
        // todo Extractor should just take Handles
        let extractor = ExtractorBuilder::with_url(target_url)
            .depth(depth)
            .config(&CONFIGURATION)
            // todo abstract the call here, or just leave it til i put handles in extractor instead
            .recursion_transmitter(handles.scans.read().unwrap().as_ref().unwrap().tx.clone())
            .stats_transmitter(handles.stats.tx.clone())
            .reporter_transmitter(handles.output.tx.clone())
            .scanned_urls(handles.ferox_scans()?)
            .stats(handles.stats.data.clone())
            .build()?;

        let _ = extractor.extract().await;
    }

    let ferox_scans = handles.ferox_scans()?;

    let ferox_scan = match ferox_scans.get_scan_by_url(&target_url) {
        Some(scan) => {
            scan.set_status(ScanStatus::Running)?;
            scan
        }
        None => {
            let msg = format!(
                "Could not find FeroxScan associated with {}; this shouldn't happen... exiting",
                target_url
            );
            bail!(fmt_err(&msg))
        }
    };

    let progress_bar = ferox_scan.progress_bar();

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

    // todo should take handles
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

    handles.filters.send(AddFilter(filter))?;

    let scanned_urls = handles.ferox_scans()?;

    // producer tasks (mp of mpsc); responsible for making requests
    let producers = stream::iter(looping_words.deref().to_owned())
        .map(|word| {
            let handles_clone = handles.clone();
            let pb = progress_bar.clone(); // progress bar is an Arc around internal state
            let tgt = target_url.to_string(); // done to satisfy 'static lifetime below
            let scanned_urls_clone = scanned_urls.clone();
            (
                tokio::spawn(async move {
                    if PAUSE_SCAN.load(Ordering::Acquire) {
                        // for every word in the wordlist, check to see if PAUSE_SCAN is set to true
                        // when true; enter a busy loop that only exits by setting PAUSE_SCAN back
                        // to false
                        scanned_urls_clone.pause(true).await;
                    }
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

    handles.stats.send(UpdateF64Field(
        DirScanTimes,
        scan_timer.elapsed().as_secs_f64(),
    ))?;

    // drop the current permit so the semaphore will allow another scan to proceed
    drop(permit);

    ferox_scan.finish()?;

    log::trace!("exit: scan_url");

    Ok(())
}

/// Perform steps necessary to run scans that only need to be performed once (warming up the
/// engine, as it were)
pub async fn initialize(
    num_words: usize,
    config: &Configuration,
    handles: Arc<Handles>,
) -> Result<()> {
    log::trace!(
        "enter: initialize({}, {:?}, {:?})",
        num_words,
        config,
        handles
    );

    // number of requests only needs to be calculated once, and then can be reused
    let num_reqs_expected: u64 = if config.extensions.is_empty() {
        num_words.try_into()?
    } else {
        let total = num_words * (config.extensions.len() + 1);
        total.try_into()?
    };

    // tell Stats object about the number of expected requests
    handles.stats.send(UpdateUsizeField(
        ExpectedPerScan,
        num_reqs_expected as usize,
    ))?;

    // add any status code filters to `FILTERS` (-C|--filter-status)
    for code_filter in &config.filter_status {
        let filter = StatusCodeFilter {
            filter_code: *code_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any line count filters to `FILTERS` (-N|--filter-lines)
    for lines_filter in &config.filter_line_count {
        let filter = LinesFilter {
            line_count: *lines_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any line count filters to `FILTERS` (-W|--filter-words)
    for words_filter in &config.filter_word_count {
        let filter = WordsFilter {
            word_count: *words_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any line count filters to `FILTERS` (-S|--filter-size)
    for size_filter in &config.filter_size {
        let filter = SizeFilter {
            content_length: *size_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
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
        handles.filters.send(AddFilter(boxed_filter))?;
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
            handles.stats.tx.clone(),
        ) {
            // attempt to request the given url
            if let Ok(resp) =
                make_request(&CONFIGURATION.client, &url, handles.stats.tx.clone()).await
            {
                // if successful, create a filter based on the response's body
                let fr = FeroxResponse::from(resp, true).await;

                // hash the response body and store the resulting hash in the filter object
                let hash = FuzzyHash::new(&fr.text()).to_string();

                let filter = SimilarityFilter {
                    text: hash,
                    threshold: SIMILARITY_THRESHOLD,
                };

                let boxed_filter = Box::new(filter);
                handles.filters.send(AddFilter(boxed_filter))?;
            }
        }
    }

    if config.scan_limit == 0 {
        // scan_limit == 0 means no limit should be imposed... however, scoping the Semaphore
        // permit is tricky, so as a workaround, we'll add a ridiculous number of permits to
        // the semaphore (1,152,921,504,606,846,975 to be exact) and call that 'unlimited'
        SCAN_LIMITER.add_permits(usize::MAX >> 4);
    }

    handles.filters.sync().await?;

    log::trace!("exit: initialize");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeroxChannel;
    use tokio::sync::mpsc;

    #[test]
    /// sending url + word without any extensions should get back one url with the joined word
    fn create_urls_no_extension_returns_base_url_with_word() {
        let (tx, _) = mpsc::unbounded_channel::<Command>();
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
