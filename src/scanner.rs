use std::{
    cmp::max, collections::HashSet, convert::TryInto, ops::Deref, sync::atomic::Ordering,
    sync::Arc, time::Instant,
};

use anyhow::{bail, Result};
use futures::{stream, StreamExt};
use lazy_static::lazy_static;
use leaky_bucket::LeakyBucket;
use tokio::sync::{oneshot, Semaphore};

use crate::{
    event_handlers::{
        Command::{self, AddError, UpdateF64Field, UpdateUsizeField},
        Handles,
    },
    extractor::{
        ExtractionTarget::{ResponseBody, RobotsTxt},
        ExtractorBuilder, // todo this isn't really necessary anymore
    },
    heuristics,
    response::FeroxResponse,
    scan_manager::{FeroxResponses, ScanOrder, ScanStatus, PAUSE_SCAN},
    statistics::{
        StatError::Other,
        StatField::{DirScanTimes, ExpectedPerScan},
    },
    url::FeroxUrl,
    utils::{fmt_err, make_request},
};
use tokio::time::Duration;

lazy_static! {
    /// Vector of FeroxResponse objects
    pub static ref RESPONSES: FeroxResponses = FeroxResponses::default();
    // todo consider removing this
}

/// Makes multiple requests based on the presence of extensions
struct Requester {
    /// handles to handlers and config
    handles: Arc<Handles>,

    /// url that will be scanned
    target_url: String,

    /// limits requests per second if present
    rate_limiter: Option<LeakyBucket>,
}

/// Requester implementation
impl Requester {
    /// given a FeroxScanner, create a Requester
    pub fn from(scanner: &FeroxScanner) -> Result<Self> {
        let limit = scanner.handles.config.rate_limit;
        let refill = max(limit / 10, 1); // minimum of 1 per second
        let tokens = max(limit / 2, 1);
        let interval = if refill == 1 { 1000 } else { 100 }; // 1 second if refill is 1

        let rate_limiter = if limit > 0 {
            let bucket = LeakyBucket::builder()
                .refill_interval(Duration::from_millis(interval)) // add tokens every 0.1s
                .refill_amount(refill) // ex: 100 req/s -> 10 tokens per 0.1s
                .tokens(tokens) // reduce initial burst, 2 is arbitrary, but felt good
                .max(limit)
                .build()?;
            Some(bucket)
        } else {
            None
        };

        Ok(Self {
            rate_limiter,
            handles: scanner.handles.clone(),
            target_url: scanner.target_url.to_owned(),
        })
    }

    /// limit the number of requests per second
    pub async fn limit(&self) -> Result<()> {
        self.rate_limiter.as_ref().unwrap().acquire_one().await?;
        Ok(())
    }

    /// Wrapper for [make_request](fn.make_request.html)
    ///
    /// Attempts recursion when appropriate and sends Responses to the output handler for processing
    async fn request(&self, word: &str) -> Result<()> {
        log::trace!("enter: request({})", word);

        let urls =
            FeroxUrl::from_string(&self.target_url, self.handles.clone()).formatted_urls(word)?;

        for url in urls {
            if self.rate_limiter.is_some() {
                // found a rate limiter, limit that junk!
                if let Err(e) = self.limit().await {
                    log::warn!("Could not rate limit scan: {}", e);
                    self.handles.stats.send(AddError(Other)).unwrap_or_default();
                }
            }

            let response = make_request(
                &self.handles.config.client,
                &url,
                self.handles.config.output_level,
                self.handles.stats.tx.clone(),
            )
            .await?;

            // response came back without error, convert it to FeroxResponse
            let ferox_response =
                FeroxResponse::from(response, true, self.handles.config.output_level).await;

            // do recursion if appropriate
            if !self.handles.config.no_recursion {
                self.handles
                    .send_scan_command(Command::TryRecursion(Box::new(ferox_response.clone())))?;
                let (tx, rx) = oneshot::channel::<bool>();
                self.handles.send_scan_command(Command::Sync(tx))?;
                rx.await?;
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not
            if self
                .handles
                .filters
                .data
                .should_filter_response(&ferox_response, self.handles.stats.tx.clone())
            {
                continue;
            }

            if self.handles.config.extract_links && !ferox_response.status().is_redirection() {
                let extractor = ExtractorBuilder::default()
                    .target(ResponseBody)
                    .response(&ferox_response)
                    .handles(self.handles.clone())
                    .build()?;

                extractor.extract().await?;
            }

            // everything else should be reported
            if let Err(e) = ferox_response.send_report(self.handles.output.tx.clone()) {
                log::warn!("Could not send FeroxResponse to output handler: {}", e);
            }
        }

        log::trace!("exit: request");
        Ok(())
    }
}

/// handles the main muscle movement of scanning a url
pub struct FeroxScanner {
    /// handles to handlers and config
    handles: Arc<Handles>,

    /// url that will be scanned
    target_url: String,

    /// whether or not this scanner is targeting an initial target specified by the user or one
    /// found via recursion
    order: ScanOrder,

    /// wordlist that's already been read from disk
    wordlist: Arc<HashSet<String>>,

    /// limiter that restricts the number of active FeroxScanners  
    scan_limiter: Arc<Semaphore>,
}

/// FeroxScanner implementation
impl FeroxScanner {
    /// create a new FeroxScanner
    pub fn new(
        target_url: &str,
        order: ScanOrder,
        wordlist: Arc<HashSet<String>>,
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

        if matches!(self.order, ScanOrder::Initial) && self.handles.config.extract_links {
            // only grab robots.txt on the initial scan_url calls. all fresh dirs will be passed
            // to try_recursion
            let extractor = ExtractorBuilder::default()
                .url(&self.target_url)
                .handles(self.handles.clone())
                .target(RobotsTxt)
                .build()?;

            let _ = extractor.extract().await;
        }

        let scanned_urls = self.handles.ferox_scans()?;

        let ferox_scan = match scanned_urls.get_scan_by_url(&self.target_url) {
            Some(scan) => {
                scan.set_status(ScanStatus::Running)?;
                scan
            }
            None => {
                // todo unit test to hit this branch
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

        // Arc clones to be passed around to the various scans
        let looping_words = self.wordlist.clone();

        {
            let test = heuristics::HeuristicTests::new(self.handles.clone());
            if let Ok(num_reqs) = test.wildcard(&self.target_url).await {
                progress_bar.inc(num_reqs);
            }
        }

        let requester = Arc::new(Requester::from(self)?);
        let increment_len = (self.handles.config.extensions.len() + 1) as u64;

        // producer tasks (mp of mpsc); responsible for making requests
        let producers = stream::iter(looping_words.deref().to_owned())
            .map(|word| {
                let pb = progress_bar.clone(); // progress bar is an Arc around internal state
                let scanned_urls_clone = scanned_urls.clone();
                let requester_clone = requester.clone();
                (
                    tokio::spawn(async move {
                        if PAUSE_SCAN.load(Ordering::Acquire) {
                            // for every word in the wordlist, check to see if PAUSE_SCAN is set to true
                            // when true; enter a busy loop that only exits by setting PAUSE_SCAN back
                            // to false
                            scanned_urls_clone.pause(true).await;
                        }
                        requester_clone.request(&word).await
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

        self.handles.stats.send(UpdateF64Field(
            DirScanTimes,
            scan_timer.elapsed().as_secs_f64(),
        ))?;

        ferox_scan.finish()?;

        log::trace!("exit: scan_url");

        Ok(())
    }
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

    log::trace!("exit: initialize");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputLevel;
    use crate::scan_manager::FeroxScans;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[should_panic]
    /// try to hit struct field coverage of FileOutHandler
    async fn get_scan_by_url_bails_on_unfound_url() {
        let sem = Semaphore::new(10);
        let urls = FeroxScans::new(OutputLevel::Default);

        let scanner = FeroxScanner::new(
            "http://localhost",
            ScanOrder::Initial,
            Arc::new(Default::default()),
            Arc::new(sem),
            Arc::new(Handles::for_testing(Some(Arc::new(urls)), None).0),
        );
        scanner.scan_url().await.unwrap();
    }
}
