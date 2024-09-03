use std::sync::Arc;

use anyhow::{bail, Result};
use tokio::sync::{mpsc, Semaphore};

use crate::{
    response::FeroxResponse,
    scan_manager::{FeroxScan, FeroxScans, ScanOrder},
    scanner::{FeroxScanner, RESPONSES},
    statistics::StatField::TotalScans,
    url::FeroxUrl,
    utils::should_deny_url,
    CommandReceiver, CommandSender, FeroxChannel, Joiner, SLEEP_DURATION,
};

use super::command::Command::AddToUsizeField;
use super::*;
use crate::statistics::StatField;
use crate::utils::parse_url_with_raw_path;
use tokio::time::Duration;

#[derive(Debug)]
/// Container for recursion transmitter and FeroxScans object
pub struct ScanHandle {
    /// FeroxScans object used across modules to track scans
    pub data: Arc<FeroxScans>,

    /// transmitter used to update `data`
    pub tx: CommandSender,
}

/// implementation of RecursionHandle
impl ScanHandle {
    /// Given an Arc-wrapped FeroxScans and CommandSender, create a new RecursionHandle
    pub fn new(data: Arc<FeroxScans>, tx: CommandSender) -> Self {
        Self { data, tx }
    }

    /// Send the given Command over `tx`
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command)?;
        Ok(())
    }
}

/// event handler for updating a single data structure of all FeroxScans
#[derive(Debug)]
pub struct ScanHandler {
    /// collection of FeroxScans
    data: Arc<FeroxScans>,

    /// handles to other handlers needed to kick off a scan while already past main
    handles: Arc<Handles>,

    /// Receiver half of mpsc from which `Command`s are processed
    receiver: CommandReceiver,

    /// wordlist (re)used for each scan
    wordlist: std::sync::Mutex<Option<Arc<Vec<String>>>>,

    /// group of scans that need to be joined
    tasks: Vec<Arc<FeroxScan>>,

    /// Maximum recursion depth, a depth of 0 is infinite recursion
    max_depth: usize,

    /// depths associated with the initial targets provided by the user
    depths: Vec<(String, usize)>,

    /// Bounded semaphore used as a barrier to limit concurrent scans
    limiter: Arc<Semaphore>,
}

/// implementation of event handler for filters
impl ScanHandler {
    /// create new event handler
    pub fn new(
        data: Arc<FeroxScans>,
        handles: Arc<Handles>,
        max_depth: usize,
        receiver: CommandReceiver,
    ) -> Self {
        let limit = handles.config.scan_limit;
        let limiter = Semaphore::new(limit);

        if limit == 0 {
            // scan_limit == 0 means no limit should be imposed... however, scoping the Semaphore
            // permit is tricky, so as a workaround, we'll add a ridiculous number of permits to
            // the semaphore (1,152,921,504,606,846,975 to be exact) and call that 'unlimited'

            // note to self: the docs say max is usize::MAX >> 3, however, threads will panic if
            // that value is used (says adding (1) will overflow the semaphore, even though none
            // are being added...)
            limiter.add_permits(usize::MAX >> 4);
        }

        Self {
            data,
            handles,
            receiver,
            max_depth,
            tasks: Vec::new(),
            depths: Vec::new(),
            limiter: Arc::new(limiter),
            wordlist: std::sync::Mutex::new(None),
        }
    }

    /// Set the wordlist
    fn wordlist(&self, wordlist: Arc<Vec<String>>) {
        if let Ok(mut guard) = self.wordlist.lock() {
            if guard.is_none() {
                let _ = std::mem::replace(&mut *guard, Some(wordlist));
            }
        }
    }

    /// Initialize new `FeroxScans` and the sc side of an mpsc channel that is responsible for
    /// updates to the aforementioned object.
    pub fn initialize(handles: Arc<Handles>) -> (Joiner, ScanHandle) {
        log::trace!("enter: initialize");

        let data = Arc::new(FeroxScans::new(
            handles.config.output_level,
            handles.config.limit_bars,
        ));
        let (tx, rx): FeroxChannel<Command> = mpsc::unbounded_channel();

        let max_depth = handles.config.depth;

        let mut handler = Self::new(data.clone(), handles, max_depth, rx);

        let task = tokio::spawn(async move { handler.start().await });

        let event_handle = ScanHandle::new(data, tx);

        log::trace!("exit: initialize -> ({:?}, {:?})", task, event_handle);

        (task, event_handle)
    }

    /// Start a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives `Command` and acts accordingly
    pub async fn start(&mut self) -> Result<()> {
        log::trace!("enter: start({:?})", self);

        while let Some(command) = self.receiver.recv().await {
            match command {
                Command::ScanInitialUrls(targets) => {
                    self.ordered_scan_url(targets, ScanOrder::Initial).await?;
                }
                Command::ScanNewUrl(target) => {
                    // added as part of interactive menu ability (2.4.1) to add a new scan.
                    // we don't have a way of knowing if they're adding a new url entirely (i.e.
                    // new base url), or simply adding a new sub-directory found some other way.
                    // Since we can't know, we'll start a scan as though we received the scan
                    // from -u | --stdin
                    self.ordered_scan_url(vec![target], ScanOrder::Initial)
                        .await?;
                }
                Command::UpdateWordlist(wordlist) => {
                    self.wordlist(wordlist);
                }
                Command::JoinTasks(sender) => {
                    let ferox_scans = self.handles.ferox_scans().unwrap_or_default();
                    let limiter_clone = self.limiter.clone();

                    tokio::spawn(async move {
                        while ferox_scans.has_active_scans() {
                            tokio::time::sleep(Duration::from_millis(SLEEP_DURATION + 250)).await;
                        }
                        limiter_clone.close();
                        sender.send(true).expect("oneshot channel failed");
                    });
                }
                Command::TryRecursion(response) => {
                    self.try_recursion(response).await?;
                }
                Command::Sync(sender) => {
                    sender.send(true).unwrap_or_default();
                }
                Command::AddDiscoveredExtension(new_extension) => {
                    // if --collect-extensions was used, AND the new extension isn't in
                    // the --dont-collect list AND it's also not in the --extensions list, AND
                    // we actually added a new extension (i.e. wasn't previously known), add
                    // it to FeroxScans.collected_extensions
                    if self.handles.config.collect_extensions
                        && !self.handles.config.dont_collect.contains(&new_extension)
                        && !self.handles.config.extensions.contains(&new_extension)
                        && self.data.add_discovered_extension(new_extension)
                    {
                        self.update_all_bar_lengths()?;
                        self.handles
                            .stats
                            .send(Command::AddToUsizeField(StatField::ExtensionsCollected, 1))
                            .unwrap_or_default();
                    }
                }
                _ => {} // no other commands needed for RecursionHandler
            }
        }

        log::trace!("exit: start");
        Ok(())
    }

    /// update all current and future bar lengths
    ///
    /// updating all bar lengths correctly requires a few different actions on our part.
    /// - get the current number of requests expected per scan (dynamic when --collect-extensions
    ///     is used)
    /// - update the overall progress bar via the statistics handler (total expected)
    /// - update the expected per scan value tracked in the statistics handler
    /// - update progress bars on each FeroxScan (type::directory) that are running/not-started
    /// - update progress bar length on FeroxScans (this is used when creating new a FeroxScan and
    ///     determines the new scan's progress bar length)
    fn update_all_bar_lengths(&self) -> Result<()> {
        log::trace!("enter: update_all_bar_lengths");

        // current number of requests expected per scan
        // ExpectedPerScan and TotalExpected are a += action, so we need the wordlist length to
        // update them while the other updates use expected_num_requests_per_dir
        let num_words = self.get_wordlist(0)?.len();
        let current_expectation = self.handles.expected_num_requests_per_dir() as u64;

        // used in the calculation of bar width down below, see explanation there
        let divisor = (self.handles.expected_num_requests_multiplier() as u64 - 1).max(1);

        // add another `wordlist.len` to the expected per scan tracker in the statistics handler
        self.handles
            .stats
            .send(AddToUsizeField(StatField::ExpectedPerScan, num_words))?;

        // since we're adding extensions in the middle of scans (potentially), we need to take
        // current number of requests into account, new_total will be used as an accumulator
        // used to increment the overall progress bar
        let mut new_total = 0;

        if let Ok(ferox_scans) = self.handles.ferox_scans() {
            // update progress bar length on FeroxScans, which used when creating a new FeroxScan's
            // progress bar and should mirror the expected_per_scan field on Statistics
            ferox_scans.set_bar_length(current_expectation);

            if let Ok(scans_guard) = ferox_scans.scans.read() {
                // update progress bars on each FeroxScan where its scan type is directory and
                // scan status is either running or not-started
                for scan in scans_guard.iter() {
                    if scan.is_active() {
                        // current number of words left in the 'to-scan' bin, for example:
                        //
                        // say we have a 2000 word wordlist, have `-x js` on the command line, and
                        // just found `php` as a new extension
                        //
                        // that puts our state at:
                        // - wordlist length: 2000
                        // - total expected: 4000 (original length * 2 for -x js)
                        //
                        // let's assume the current scan has sent 3000 requests so far
                        // that means to get the number of `words` left to send, we need to take
                        // the difference of 4000 and 3000 and then divide that by the current
                        // multiplier (2 in the example)
                        //
                        // (4000 - 3000) / 2 => 500 words left to send
                        //
                        // the remaining 500 words will be sent as 3 variations (word, word.js,
                        // word.php). So, we would then need to increment the bar by 500 to
                        // reflect the dynamism of adding extensions mid-scan.
                        let bar = scan.progress_bar();

                        // (4000 - 3000) / 2 => 500 words left to send
                        let length = bar.length().unwrap_or(1);
                        let num_words_left = (length - bar.position()) / divisor;

                        // accumulate each bar's increment value for incrementing the total bar
                        new_total += num_words_left;

                        bar.inc_length(num_words_left);
                    }
                }
            }

            // add the total number of newly expected requests to the overall progress bar
            // via the statistics handler
            self.handles.stats.send(AddToUsizeField(
                StatField::TotalExpected,
                new_total as usize,
            ))?;
        }

        log::trace!("exit: update_all_bar_lengths");
        Ok(())
    }

    /// Helper to easily get the (locked) underlying wordlist
    pub fn get_wordlist(&self, offset: usize) -> Result<Arc<Vec<String>>> {
        if let Ok(guard) = self.wordlist.lock().as_ref() {
            if let Some(list) = guard.as_ref() {
                return if offset > 0 {
                    Ok(Arc::new(list[offset..].to_vec()))
                } else {
                    Ok(list.clone())
                };
            }
        }

        bail!("Could not get underlying wordlist")
    }

    /// wrapper around scanning a url to stay DRY
    async fn ordered_scan_url(&mut self, targets: Vec<String>, order: ScanOrder) -> Result<()> {
        log::trace!("enter: ordered_scan_url({:?}, {:?})", targets, order);
        let should_test_deny = !self.handles.config.url_denylist.is_empty()
            || !self.handles.config.regex_denylist.is_empty();

        for target in targets {
            if self.data.contains(&target) && matches!(order, ScanOrder::Latest) {
                // FeroxScans knows about this url and scan isn't an Initial scan
                // initial scans are skipped because when resuming from a .state file, the scans
                // will already be populated in FeroxScans, so we need to not skip kicking off
                // their scans
                continue;
            }

            let scan = if let Some(ferox_scan) = self.data.get_scan_by_url(&target) {
                ferox_scan // scan already known
            } else {
                self.data
                    .add_directory_scan(&target, order, self.handles.clone())
                    .1 // add the new target; return FeroxScan
            };

            if should_test_deny
                && should_deny_url(&parse_url_with_raw_path(&target)?, self.handles.clone())?
            {
                // response was caught by a user-provided deny list
                // checking this last, since it's most susceptible to longer runtimes due to what
                // input is received
                continue;
            }

            let divisor = self.handles.expected_num_requests_multiplier();

            let list = if divisor > 1 && scan.requests() > 0 {
                // if there were extensions provided and/or more than a single method used, and some
                // number of requests have already been sent, we need to adjust the offset into the
                // wordlist to ensure we don't index out of bounds

                let adjusted = scan.requests_made_so_far() as f64 / (divisor as f64 - 1.0).max(1.0);
                self.get_wordlist(adjusted as usize)?
            } else {
                self.get_wordlist(scan.requests_made_so_far() as usize)?
            };

            log::info!("scan handler received {} - beginning scan", target);

            if matches!(order, ScanOrder::Initial) {
                // keeps track of the initial targets' scan depths in order to enforce the
                // maximum recursion depth on any identified sub-directories
                let url = FeroxUrl::from_string(&target, self.handles.clone());
                let depth = url.depth().unwrap_or(0);
                self.depths.push((target.clone(), depth));
            }

            let scanner = FeroxScanner::new(
                &target,
                order,
                list,
                self.limiter.clone(),
                self.handles.clone(),
            );

            let task = tokio::spawn(async move {
                if let Err(e) = scanner.scan_url().await {
                    log::warn!("{}", e);
                }
            });

            self.handles.stats.send(AddToUsizeField(TotalScans, 1))?;

            scan.set_task(task).await?;

            self.tasks.push(scan.clone());
        }

        log::trace!("exit: ordered_scan_url");
        Ok(())
    }

    async fn try_recursion(&mut self, response: Box<FeroxResponse>) -> Result<()> {
        log::trace!("enter: try_recursion({:?})", response,);

        if !self.handles.config.force_recursion && !response.is_directory() {
            // not a directory and --force-recursion wasn't used, quick exit
            return Ok(());
        }

        let mut base_depth = 1_usize;

        for (base_url, base_url_depth) in &self.depths {
            if response.url().as_str().starts_with(base_url) {
                base_depth = *base_url_depth;
            }
        }

        if response.reached_max_depth(base_depth, self.max_depth, self.handles.clone()) {
            // at or past recursion depth
            return Ok(());
        }

        if let Ok(responses) = RESPONSES.responses.read() {
            for maybe_wild in responses.iter() {
                if !maybe_wild.wildcard() || !maybe_wild.is_directory() {
                    // if the stored response isn't a wildcard, skip it
                    // if the stored response isn't a directory, skip it
                    // we're only interested in preventing recursion into wildcard directories
                    continue;
                }

                if maybe_wild.method() != response.method() {
                    // methods don't match, skip it
                    continue;
                }

                // methods match and is a directory wildcard
                // need to check the wildcard's parent directory
                // for equality with the incoming response's parent directory
                //
                // if the parent directories match, we need to prevent recursion
                // into the wildcard directory

                match (
                    maybe_wild.url().path_segments(),
                    response.url().path_segments(),
                ) {
                    // both urls must have path segments
                    (Some(mut maybe_wild_segments), Some(mut response_segments)) => {
                        match (
                            maybe_wild_segments.nth_back(1),
                            response_segments.nth_back(1),
                        ) {
                            // both urls must have at least 2 path segments, the next to last being the parent
                            (Some(maybe_wild_parent), Some(response_parent)) => {
                                if maybe_wild_parent == response_parent {
                                    // the parent directories match, so we need to prevent recursion
                                    return Ok(());
                                }
                            }
                            _ => {
                                // we couldn't get the parent directory, so we'll skip this
                                continue;
                            }
                        }
                    }
                    _ => {
                        // we couldn't get the path segments, so we'll skip this
                        continue;
                    }
                }
            }
        }

        let targets = vec![response.url().to_string()];
        self.ordered_scan_url(targets, ScanOrder::Latest).await?;

        log::info!("Added new directory to recursive scan: {}", response.url());

        log::trace!("exit: try_recursion");
        Ok(())
    }
}
