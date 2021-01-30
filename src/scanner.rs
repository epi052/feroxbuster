use crate::ferox_url::FeroxUrl;
use crate::{
    config::CONFIGURATION,
    event_handlers::{
        Command::{self, AddFilter, UpdateF64Field, UpdateUsizeField},
        Handles,
    },
    extractor::ExtractorBuilder,
    filters::{
        LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter, WordsFilter,
    },
    heuristics,
    scan_manager::{FeroxResponses, ScanOrder, ScanStatus, PAUSE_SCAN},
    skip_fail,
    statistics::StatField::{DirScanTimes, ExpectedPerScan},
    utils::{fmt_err, make_request},
    CommandSender, FeroxResponse, SIMILARITY_THRESHOLD,
};
use anyhow::{bail, Result};
use futures::{stream, StreamExt};
use fuzzyhash::FuzzyHash;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Url;
use std::{
    collections::HashSet, convert::TryInto, ops::Deref, sync::atomic::Ordering, sync::Arc,
    time::Instant,
};
use tokio::sync::{oneshot, Semaphore};

lazy_static! {
    /// Vector of FeroxResponse objects
    pub static ref RESPONSES: FeroxResponses = FeroxResponses::default();

    /// Bounded semaphore used as a barrier to limit concurrent scans
    static ref SCAN_LIMITER: Semaphore = Semaphore::new(CONFIGURATION.scan_limit);


}

/// Wrapper for [make_request](fn.make_request.html)
///
/// Makes multiple requests based on the presence of extensions
///
/// Attempts recursion when appropriate and sends Responses to the output handler for processing
async fn make_requests(target_url: &str, word: &str, handles: Arc<Handles>) -> Result<()> {
    log::trace!(
        "enter: make_requests({}, {}, {:?})",
        target_url,
        word,
        handles
    );

    let urls = FeroxUrl::from_string(target_url, handles.clone()).formatted_urls(word)?;

    for url in urls {
        let response = make_request(&handles.config.client, &url, handles.stats.tx.clone()).await?;

        // response came back without error, convert it to FeroxResponse
        let ferox_response = FeroxResponse::from(response, true).await;

        // do recursion if appropriate
        if !handles.config.no_recursion {
            handles.send_scan_command(Command::TryRecursion(Box::new(ferox_response.clone())))?;
            let (tx, rx) = oneshot::channel::<bool>();
            handles.send_scan_command(Command::Sync(tx))?;
            rx.await?;
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

        if handles.config.extract_links && !ferox_response.status().is_redirection() {
            let extractor = ExtractorBuilder::with_response(&ferox_response)
                .config(&handles.config)
                .handles(handles.clone())
                .build()?;

            extractor.extract().await?;
        }

        // everything else should be reported
        send_report(handles.output.tx.clone(), ferox_response);
    }

    log::trace!("exit: make_requests");
    Ok(())
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

    log::info!("Starting scan against: {}", target_url);

    let scan_timer = Instant::now();

    if matches!(order, ScanOrder::Initial) && handles.config.extract_links {
        // only grab robots.txt on the initial scan_url calls. all fresh dirs will be passed
        // to try_recursion
        let extractor = ExtractorBuilder::with_url(target_url)
            .config(&handles.config)
            .handles(handles.clone())
            .build()?;

        let _ = extractor.extract().await;
    }

    let scanned_urls = handles.ferox_scans()?;

    let ferox_scan = match scanned_urls.get_scan_by_url(&target_url) {
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

    // Arc clones to be passed around to the various scans
    let looping_words = wordlist.clone();

    {
        let test = heuristics::HeuristicTests::new(handles.clone());
        if let Ok(num_reqs) = test.wildcard(&target_url).await {
            progress_bar.inc(num_reqs);
        }
    }

    let increment_len = (handles.config.extensions.len() + 1) as u64;

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
                    make_requests(&tgt, &word, handles_clone).await
                }),
                pb,
            )
        })
        .for_each_concurrent(handles.config.threads, |(resp, bar)| async move {
            match resp.await {
                Ok(_) => {
                    bar.inc(increment_len);
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
pub async fn initialize(num_words: usize, handles: Arc<Handles>) -> Result<()> {
    log::trace!("enter: initialize({}, {:?})", num_words, handles);

    // number of requests only needs to be calculated once, and then can be reused
    let num_reqs_expected: u64 = if handles.config.extensions.is_empty() {
        num_words.try_into()?
    } else {
        let total = num_words * (handles.config.extensions.len() + 1);
        total.try_into()?
    };

    {
        // no real reason to keep the arc around beyond this call
        let scans = handles.ferox_scans()?;
        scans.set_bar_length(num_reqs_expected);
    }

    // tell Stats object about the number of expected requests
    handles.stats.send(UpdateUsizeField(
        ExpectedPerScan,
        num_reqs_expected as usize,
    ))?;

    // add any status code filters to filters handler's FeroxFilters  (-C|--filter-status)
    for code_filter in &handles.config.filter_status {
        let filter = StatusCodeFilter {
            filter_code: *code_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any line count filters to filters handler's FeroxFilters  (-N|--filter-lines)
    for lines_filter in &handles.config.filter_line_count {
        let filter = LinesFilter {
            line_count: *lines_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any line count filters to filters handler's FeroxFilters  (-W|--filter-words)
    for words_filter in &handles.config.filter_word_count {
        let filter = WordsFilter {
            word_count: *words_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any line count filters to filters handler's FeroxFilters  (-S|--filter-size)
    for size_filter in &handles.config.filter_size {
        let filter = SizeFilter {
            content_length: *size_filter,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any regex filters to filters handler's FeroxFilters  (-X|--filter-regex)
    for regex_filter in &handles.config.filter_regex {
        let raw = regex_filter;
        let compiled = skip_fail!(Regex::new(&raw));

        let filter = RegexFilter {
            raw_string: raw.to_owned(),
            compiled,
        };
        let boxed_filter = Box::new(filter);
        handles.filters.send(AddFilter(boxed_filter))?;
    }

    // add any similarity filters to filters handler's FeroxFilters  (--filter-similar-to)
    for similarity_filter in &handles.config.filter_similar {
        // url as-is based on input, ignores user-specified url manipulation options (add-slash etc)
        let url = skip_fail!(Url::parse(&similarity_filter));

        // attempt to request the given url
        let resp =
            skip_fail!(make_request(&handles.config.client, &url, handles.stats.tx.clone()).await);

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

    if handles.config.scan_limit == 0 {
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
    use crate::config::Configuration;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[should_panic]
    /// call initialize with a bad regex, triggering a panic
    async fn initialize_panics_on_bad_regex() {
        let config = Configuration {
            filter_regex: vec![r"(".to_string()],
            ..Default::default()
        };
        let handles = Arc::new(Handles::for_testing(None, Some(Arc::new(config))).0);
        initialize(1, handles).await.unwrap();
    }
}
