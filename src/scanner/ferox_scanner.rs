use std::{ops::Deref, sync::atomic::Ordering, sync::Arc, time::Instant};

use anyhow::{bail, Result};
use console::style;
use futures::{stream, StreamExt};
use lazy_static::lazy_static;
use tokio::sync::Semaphore;

use crate::{
    event_handlers::{
        Command::{AddError, AddToF64Field, SubtractFromUsizeField},
        Handles,
    },
    extractor::{ExtractionTarget, ExtractorBuilder},
    heuristics,
    scan_manager::{FeroxResponses, MenuCmdResult, ScanOrder, ScanStatus, PAUSE_SCAN},
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

    /// Scan a given url using a given wordlist
    ///
    /// This is the primary entrypoint for the scanner
    pub async fn scan_url(&self) -> Result<()> {
        log::trace!("enter: scan_url");
        log::info!("Starting scan against: {}", self.target_url);

        let scan_timer = Instant::now();
        let mut dirlist_flag = false;

        if self.handles.config.extract_links {
            // parse html for links (i.e. web scraping)
            let extractor = ExtractorBuilder::default()
                .target(ExtractionTarget::ParseHtml)
                .url(&self.target_url)
                .handles(self.handles.clone())
                .build()?;
            let extract_out = extractor.extract().await?;
            let links = extract_out.0;
            dirlist_flag = extract_out.1;
            extractor.request_links(links).await?;

            if matches!(self.order, ScanOrder::Initial) {
                // check for robots.txt (cannot be in subdirs)
                let extractor = ExtractorBuilder::default()
                    .target(ExtractionTarget::RobotsTxt)
                    .url(&self.target_url)
                    .handles(self.handles.clone())
                    .build()?;
                let links = (extractor.extract().await?).0;
                extractor.request_links(links).await?;
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

        // Directory listing heuristic detection to not continue scanning
        if dirlist_flag {
            log::trace!("exit: scan_url -> Directory listing heuristic");

            self.handles.stats.send(AddToF64Field(
                DirScanTimes,
                scan_timer.elapsed().as_secs_f64(),
            ))?;

            self.handles.stats.send(SubtractFromUsizeField(
                TotalExpected,
                progress_bar.length() as usize,
            ))?;

            progress_bar.reset_eta();
            progress_bar.finish_with_message(&format!("=> {}", style("Directory listing").green()));

            ferox_scan.finish()?;

            return Ok(());
        }

        // When acquire is called and the semaphore has remaining permits, the function immediately
        // returns a permit. However, if no remaining permits are available, acquire (asynchronously)
        // waits until an outstanding permit is dropped, at which point, the freed permit is assigned
        // to the caller.
        let _permit = self.scan_limiter.acquire().await;

        // Arc clones to be passed around to the various scans
        let looping_words = self.wordlist.clone();

        {
            let test = heuristics::HeuristicTests::new(self.handles.clone());
            if let Ok(num_reqs) = test.wildcard(&self.target_url).await {
                progress_bar.inc(num_reqs);
            }
        }

        let requester = Arc::new(Requester::from(self, ferox_scan.clone())?);
        let increment_len =
            ((self.handles.config.extensions.len() + 1) * self.handles.config.methods.len()) as u64;

        // producer tasks (mp of mpsc); responsible for making requests
        let producers = stream::iter(looping_words.deref().to_owned())
            .map(|word| {
                let pb = progress_bar.clone(); // progress bar is an Arc around internal state
                let scanned_urls_clone = scanned_urls.clone();
                let requester_clone = requester.clone();
                let handles_clone = self.handles.clone();
                (
                    tokio::spawn(async move {
                        if PAUSE_SCAN.load(Ordering::Acquire) {
                            // for every word in the wordlist, check to see if PAUSE_SCAN is set to true
                            // when true; enter a busy loop that only exits by setting PAUSE_SCAN back
                            // to false
                            match scanned_urls_clone.pause(true).await {
                                Some(MenuCmdResult::Url(url)) => {
                                    // user wants to add a new url to be scanned, need to send
                                    // it over to the event handler for processing
                                    handles_clone
                                        .send_scan_command(Command::ScanNewUrl(url))
                                        .unwrap_or_else(|e| {
                                            log::warn!("Could not add scan to scan queue: {}", e)
                                        })
                                }
                                Some(MenuCmdResult::NumCancelled(num_canx)) => {
                                    if num_canx > 0 {
                                        handles_clone
                                            .stats
                                            .send(SubtractFromUsizeField(TotalExpected, num_canx))
                                            .unwrap_or_else(|e| {
                                                log::warn!(
                                                    "Could not update overall scan bar: {}",
                                                    e
                                                )
                                            });
                                    }
                                }
                                _ => {}
                            }
                        }
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
                        bar.inc(increment_len);
                    }
                    Err(e) => {
                        log::warn!("error awaiting a response: {}", e);
                        self.handles.stats.send(AddError(Other)).unwrap_or_default();
                    }
                }
            });

        // await tx tasks
        log::trace!("awaiting scan producers");
        producers.await;
        log::trace!("done awaiting scan producers");

        self.handles.stats.send(AddToF64Field(
            DirScanTimes,
            scan_timer.elapsed().as_secs_f64(),
        ))?;

        ferox_scan.finish()?;

        log::trace!("exit: scan_url");

        Ok(())
    }
}
