use std::fmt::Write as _;
use std::sync::atomic::AtomicBool;
use std::{ops::Deref, sync::atomic::Ordering, sync::Arc, time::Instant};

use anyhow::{bail, Result};
use console::style;
use futures::{stream, StreamExt};
use indicatif::ProgressBar;
use lazy_static::lazy_static;
use tokio::sync::Semaphore;

use crate::filters::{create_similarity_filter, EmptyFilter, SimilarityFilter};
use crate::heuristics::WildcardResult;
use crate::Command::AddFilter;
use crate::{
    event_handlers::{
        Command::{AddError, AddToF64Field, AddToUsizeField, SubtractFromUsizeField},
        Handles,
    },
    extractor::{ExtractionTarget, ExtractorBuilder},
    heuristics,
    scan_manager::{FeroxResponses, FeroxScans, MenuCmdResult, ScanOrder, ScanStatus, PAUSE_SCAN},
    scanner::requester::TF_IDF,
    statistics::{
        StatError::Other,
        StatField::{DirScanTimes, TotalExpected},
    },
    utils::fmt_err,
    Command,
};

use super::requester::Requester;

lazy_static! {
    /// Vector of FeroxResponse objects
    pub static ref RESPONSES: FeroxResponses = FeroxResponses::default();
    // todo consider removing this
}

/// check to see if `pause_flag` is set to true. when true; enter a busy loop that only exits
/// by setting PAUSE_SCAN back to false
async fn check_for_user_input(
    pause_flag: &AtomicBool,
    scanned_urls: Arc<FeroxScans>,
    handles: Arc<Handles>,
) {
    log::trace!(
        "enter: check_for_user_input({:?}, SCANNED_URLS, HANDLES)",
        pause_flag
    );

    // todo write a test or two for this function at some point...
    if pause_flag.load(Ordering::Acquire) {
        match scanned_urls.pause(true, handles.clone()).await {
            Some(MenuCmdResult::Url(url)) => {
                // user wants to add a new url to be scanned, need to send
                // it over to the event handler for processing
                handles
                    .send_scan_command(Command::ScanNewUrl(url))
                    .unwrap_or_else(|e| log::warn!("Could not add scan to scan queue: {}", e))
            }
            Some(MenuCmdResult::NumCancelled(num_canx)) => {
                if num_canx > 0 {
                    handles
                        .stats
                        .send(SubtractFromUsizeField(TotalExpected, num_canx))
                        .unwrap_or_else(|e| log::warn!("Could not update overall scan bar: {}", e));
                }
            }
            Some(MenuCmdResult::Filter(mut filter)) => {
                let url = if let Some(SimilarityFilter { original_url, .. }) =
                    filter.as_any().downcast_ref::<SimilarityFilter>()
                {
                    original_url.to_owned()
                } else {
                    String::new()
                };

                if !url.is_empty() {
                    // filter was a SimilarityFilter and now we have a url to request.
                    //
                    // The reason for this janky structure is that `filter.as_any().downcast_ref`
                    // isn't Send so we can't call create_similarity_filter(...).await, within
                    // the if let Some ipso-facto, janky code /shrug
                    let real_filter = create_similarity_filter(&url, handles.clone())
                        .await
                        .unwrap_or_default();

                    if real_filter.original_url.is_empty() {
                        // failed to create filter
                        filter = Box::new(EmptyFilter {});
                    } else {
                        filter = Box::new(real_filter)
                    }
                }

                handles
                    .filters
                    .send(AddFilter(filter))
                    .unwrap_or_else(|e| log::warn!("Could not add new filter: {}", e));
            }
            _ => {}
        }
    }
    log::trace!("exit: check_for_user_input");
}

/// handles the main muscle movement of scanning a url
pub struct FeroxScanner {
    /// handles to handlers and config
    pub(super) handles: Arc<Handles>,

    /// url that will be scanned
    pub(super) target_url: String,

    /// whether or not this scanner is targeting an initial target specified by the user or one
    /// found via recursion
    order: ScanOrder,

    /// wordlist that's already been read from disk
    wordlist: Arc<Vec<String>>,

    /// limiter that restricts the number of active FeroxScanners
    scan_limiter: Arc<Semaphore>,
}

/// FeroxScanner implementation
impl FeroxScanner {
    /// create a new FeroxScanner
    pub fn new(
        target_url: &str,
        order: ScanOrder,
        wordlist: Arc<Vec<String>>,
        scan_limiter: Arc<Semaphore>,
        handles: Arc<Handles>,
    ) -> Self {
        Self {
            order,
            handles,
            wordlist,
            scan_limiter,
            target_url: target_url.to_string(),
        }
    }

    /// produces and awaits tasks (mp of mpsc); responsible for making requests
    async fn stream_requests(
        &self,
        looping_words: Arc<Vec<String>>,
        progress_bar: ProgressBar,
        scanned_urls: Arc<FeroxScans>,
        requester: Arc<Requester>,
    ) {
        log::trace!("enter: stream_requests(params too verbose to print)");

        let producers = stream::iter(looping_words.deref().to_owned())
            .map(|word| {
                let pb = progress_bar.clone(); // progress bar is an Arc around internal state
                let scanned_urls_clone = scanned_urls.clone();
                let requester_clone = requester.clone();
                let handles_clone = self.handles.clone();
                (
                    tokio::spawn(async move {
                        // for every word in the wordlist, check to see if user has pressed enter
                        // in order to go into the interactive menu
                        check_for_user_input(&PAUSE_SCAN, scanned_urls_clone, handles_clone).await;

                        // after checking for user input, send the request
                        requester_clone
                            .request(&word)
                            .await
                            .unwrap_or_else(|e| log::warn!("Requester encountered an error: {}", e))
                    }),
                    pb,
                )
            })
            .for_each_concurrent(self.handles.config.threads, |(resp, bar)| async move {
                match resp.await {
                    Ok(_) => {
                        let increment_len = self.handles.expected_num_requests_multiplier() as u64;
                        bar.inc(increment_len);
                    }
                    Err(e) => {
                        log::warn!("error awaiting a response: {}", e);
                        self.handles.stats.send(AddError(Other)).unwrap_or_default();
                        std::process::exit(1);
                    }
                }
            });

        // await tx tasks
        log::trace!("awaiting scan producers");
        producers.await;
        log::trace!("done awaiting scan producers");
        log::trace!("exit: stream_requests");
    }

    /// Scan a given url using a given wordlist
    ///
    /// This is the primary entrypoint for the scanner
    pub async fn scan_url(&self) -> Result<()> {
        log::trace!("enter: scan_url");
        log::info!("Starting scan against: {}", self.target_url);

        let mut scan_timer = Instant::now();
        // every time we extract links we'll need to await the task to make sure
        // it completes before the scan ends
        let mut extraction_tasks = Vec::new();

        if self.handles.config.extract_links && matches!(self.order, ScanOrder::Initial) {
            // check for robots.txt (cannot be in sub-directories, so limited to Initial)
            let mut extractor = ExtractorBuilder::default()
                .target(ExtractionTarget::RobotsTxt)
                .url(&self.target_url)
                .handles(self.handles.clone())
                .build()?;
            if let Ok(result) = extractor.extract().await {
                extraction_tasks.push(extractor.request_links(result).await?)
            }
        }

        let scanned_urls = self.handles.ferox_scans()?;
        let ferox_scan = match scanned_urls.get_scan_by_url(&self.target_url) {
            Some(scan) => {
                scan.set_status(ScanStatus::Running)?;
                scan
            }
            None => {
                let msg = format!(
                    "Could not find FeroxScan associated with {}; this shouldn't happen... exiting",
                    self.target_url
                );
                bail!(fmt_err(&msg))
            }
        };

        let progress_bar = ferox_scan.progress_bar();

        // When acquire is called and the semaphore has remaining permits, the function immediately
        // returns a permit. However, if no remaining permits are available, acquire (asynchronously)
        // waits until an outstanding permit is dropped, at which point, the freed permit is assigned
        // to the caller.
        let _permit = self.scan_limiter.acquire().await;

        if self.handles.config.scan_limit > 0 {
            scan_timer = Instant::now();
            progress_bar.reset();
        }

        {
            // heuristics test block:
            let test = heuristics::HeuristicTests::new(self.handles.clone());

            if let Ok(Some(dirlist_result)) = test.directory_listing(&self.target_url).await {
                // at this point, we have a DirListingType, and it's not the None variant
                // which means we found directory listing based on the heuristic; now we need
                // to process the links that are available if --extract-links was used

                if self.handles.config.extract_links {
                    let mut extractor = ExtractorBuilder::default()
                        .response(&dirlist_result.response)
                        .target(ExtractionTarget::DirectoryListing)
                        .url(&self.target_url)
                        .handles(self.handles.clone())
                        .build()?;

                    let result = extractor.extract_from_dir_listing().await?;

                    extraction_tasks.push(extractor.request_links(result).await?);

                    log::trace!("exit: scan_url -> Directory listing heuristic");

                    self.handles.stats.send(AddToF64Field(
                        DirScanTimes,
                        scan_timer.elapsed().as_secs_f64(),
                    ))?;

                    self.handles.stats.send(SubtractFromUsizeField(
                        TotalExpected,
                        progress_bar.length().unwrap_or(0) as usize,
                    ))?;
                }

                let mut message = format!("=> {}", style("Directory listing").blue().bright());

                if !self.handles.config.scan_dir_listings {
                    write!(
                        message,
                        " (add {} to scan)",
                        style("--scan-dir-listings").bright().yellow()
                    )?;
                }

                if !self.handles.config.extract_links {
                    write!(
                        message,
                        " (remove {} to scan)",
                        style("--dont-extract-links").bright().yellow()
                    )?;
                }

                if !self.handles.config.force_recursion && !self.handles.config.scan_dir_listings {
                    for handle in extraction_tasks.into_iter().flatten() {
                        _ = handle.await;
                    }

                    progress_bar.reset_eta();
                    progress_bar.finish_with_message(message);

                    if self.handles.config.limit_bars > 0 {
                        let scans = self.handles.ferox_scans()?;
                        let num_bars = scans.number_of_bars();
                        ferox_scan.finish(num_bars)?;
                        scans.make_visible();
                    } else {
                        ferox_scan.finish(0)?;
                    }

                    return Ok(()); // nothing left to do if we found a dir listing
                }
            }

            // now that we haven't found a directory listing, we'll attempt to derive whatever
            // the server is using to respond to resources that don't exist (could be a
            // traditional 404, or a custom response)
            //
            // `detect_404_like_responses` will make the requests that the wildcard test used to
            // perform pre-2.8 in addition to new detection techniques, superseding the old
            // wildcard test
            let num_reqs_made = test.detect_404_like_responses(&self.target_url).await?;

            match num_reqs_made {
                Some(WildcardResult::WildcardDirectory(num_reqs)) => {
                    let message = format!(
                        "=> {} dir! {} recursion",
                        style("Wildcard").blue().bright(),
                        style("stopped").red()
                    );
                    progress_bar.set_message(message);
                    progress_bar.inc(num_reqs as u64);
                }
                Some(WildcardResult::FourOhFourLike(num_reqs)) => {
                    progress_bar.inc(num_reqs as u64);
                }
                _ => {}
            }
        }

        // Arc clones to be passed around to the various scans
        let looping_words = self.wordlist.clone();

        let requester = Arc::new(Requester::from(self, ferox_scan.clone())?);

        self.stream_requests(
            looping_words.clone(),
            progress_bar.clone(),
            scanned_urls.clone(),
            requester.clone(),
        )
        .await;

        if self.handles.config.collect_words {
            let new_words = TF_IDF.read().unwrap().all_words();
            let new_words_len = new_words.len();

            let cur_length = progress_bar.length().unwrap_or(0);
            let new_length = cur_length + new_words_len as u64;

            progress_bar.set_length(new_length);

            self.handles
                .stats
                .send(AddToUsizeField(TotalExpected, new_words.len()))
                .unwrap_or_default();

            log::info!(
                "requesting {} collected words: {:?}...",
                new_words_len,
                &new_words[..new_words_len.min(3)]
            );

            self.stream_requests(
                Arc::new(new_words),
                progress_bar.clone(),
                scanned_urls.clone(),
                requester.clone(),
            )
            .await;
        }

        self.handles.stats.send(AddToF64Field(
            DirScanTimes,
            scan_timer.elapsed().as_secs_f64(),
        ))?;

        for handle in extraction_tasks.into_iter().flatten() {
            _ = handle.await;
        }

        if self.handles.config.limit_bars > 0 {
            let scans = self.handles.ferox_scans()?;
            let num_bars = scans.number_of_bars();
            ferox_scan.finish(num_bars)?;
            scans.make_visible();
        } else {
            ferox_scan.finish(0)?;
        }

        log::trace!("exit: scan_url");

        Ok(())
    }
}
