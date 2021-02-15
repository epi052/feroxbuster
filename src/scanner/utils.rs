use super::FeroxScanner;
use crate::{
    atomic_load, atomic_store,
    config::RequesterPolicy,
    event_handlers::{
        Command::{self, AddError, GetRuntime, SubtractFromUsizeField},
        Handles,
    },
    extractor::{ExtractionTarget::ResponseBody, ExtractorBuilder},
    response::FeroxResponse,
    scan_manager::ScanStatus,
    statistics::{
        StatError::Other,
        StatField::{Enforced403s, Enforced429s, EnforcedErrors, TotalExpected},
    },
    url::FeroxUrl,
    utils::logged_request,
    HIGH_ERROR_RATIO,
};
use anyhow::Result;

use lazy_static::lazy_static;
use leaky_bucket::LeakyBucket;
use std::{
    cmp::max,
    ops::Index,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    sync::{oneshot, RwLock},
    time::{sleep, Duration},
};

/// default number of milliseconds to wait during a cooldown period
const WAIT_TIME: u64 = 1250;

lazy_static! {
    /// todo doc
    static ref SHOULD_TUNE: AtomicBool = AtomicBool::new(false);

    /// todo doc
    static ref TUNE_TRIGGER: std::sync::Mutex<PolicyTrigger> = std::sync::Mutex::new(PolicyTrigger::Errors);
}

#[derive(Copy, Clone, PartialEq, Debug)]
/// represents different situations where different criteria can trigger auto-tune/bail behavior
pub enum PolicyTrigger {
    /// excessive 403 trigger
    Status403,

    /// excessive 429 trigger
    Status429,

    /// excessive general errors
    Errors,
}

/// data regarding policy and metadata about last enforced trigger etc...
#[derive(Default, Debug)]
pub struct PolicyData {
    /// how to handle exceptional cases such as too many errors / 403s / 429s etc
    policy: RequesterPolicy,

    /// whether or not we're in the middle of a cooldown period
    cooling_down: AtomicBool,

    /// rate limit (at last interval)
    limit: AtomicUsize,

    /// number of errors (at last interval)
    errors: AtomicUsize,

    /// heap of values used for adjusting # of requests/second
    heap: std::sync::RwLock<LimitHeap>,
}

/// implementation of PolicyData
impl PolicyData {
    /// given a RequesterPolicy, create a new PolicyData
    fn new(policy: RequesterPolicy) -> Self {
        Self {
            policy,
            ..Default::default()
        }
    }

    /// setter for requests / second; populates the underlying heap with values from req/sec seed
    fn set_reqs_sec(&self, reqs_sec: usize) {
        if let Ok(mut guard) = self.heap.write() {
            guard.original = reqs_sec as i32;
            guard.build();
            self.set_limit(guard.value() as usize);
        }
    }

    /// setter for errors
    fn set_errors(&self, errors: usize) {
        atomic_store!(self.errors, errors);
    }

    /// setter for limit
    fn set_limit(&self, limit: usize) {
        atomic_store!(self.limit, limit);
    }

    /// getter for limit
    fn get_limit(&self) -> usize {
        atomic_load!(self.limit)
    }

    /// adjust the rate of requests per second up (increase rate)
    fn adjust_up(&self) {
        // log::error!("enter: adjust up"); // todo remove
        if let Ok(mut heap) = self.heap.write() {
            if heap.has_children() {
                let old_limit = heap.value(); // todo remove
                heap.move_left();
                self.set_limit(heap.value() as usize);
            // log::error!(
            //     "[UP ({})] current limit: {} new limit: {}",
            //     atomic_load!(self.errors),
            //     old_limit,
            //     heap.value()
            // ); // todo remove
            } else {
                let old_limit = heap.value(); // todo remove
                heap.move_up();
                self.set_limit(heap.value() as usize);
                // log::error!(
                //     "[UP ({})] current limit: {} new limit: {}",
                //     atomic_load!(self.errors),
                //     old_limit,
                //     heap.value()
                // ); // todo remove
            }
        }
        // log::error!("exit: adjust up"); // todo remove
    }

    /// adjust the rate of requests per second down (decrease rate)
    fn adjust_down(&self) {
        // log::error!("enter: adjust down: {:?}", self); // todo remove

        if let Ok(mut heap) = self.heap.write() {
            if heap.has_children() {
                let old_limit = heap.value(); // todo remove
                heap.move_right();
                self.set_limit(heap.value() as usize);
                // log::error!(
                //     "[DOWN ({})] current limit: {} new limit: {}",
                //     atomic_load!(self.errors),
                //     old_limit,
                //     heap.value()
                // ); // todo remove
            }
        }
        // log::error!("exit: adjust down"); // todo remove
    }
}

/// bespoke variation on an array-backed max-heap
///
/// 255 possible values generated from the initial requests/second
///
/// when no additional errors are encountered, the left child is taken (increasing req/sec)
/// if errors have increased since the last interval, the right child is taken (decreasing req/sec)
///
/// formula for each child:
/// - left: (|parent - current|) / 2 + current
/// - right: current - ((|parent - current|) / 2)
#[derive(Debug)]
struct LimitHeap {
    /// backing array, 255 nodes == height of 7 ( 2^(h+1) -1 nodes )
    inner: [i32; 255],

    /// original # of requests / second
    original: i32,

    /// current position w/in the backing array
    current: usize,
}

/// default implementation of a LimitHeap
impl Default for LimitHeap {
    /// zero-initialize the backing array
    fn default() -> Self {
        Self {
            inner: [0; 255],
            original: 0,
            current: 0,
        }
    }
}

/// implementation of a LimitHeap
impl LimitHeap {
    /// move to right child, return node's index from which the move was requested
    fn move_right(&mut self) -> usize {
        if self.has_children() {
            let tmp = self.current;
            self.current = self.current * 2 + 2;
            return tmp;
        }
        self.current
    }

    /// move to left child, return node's index from which the move was requested
    fn move_left(&mut self) -> usize {
        if self.has_children() {
            let tmp = self.current;
            self.current = self.current * 2 + 1;
            return tmp;
        }
        self.current
    }

    /// move to parent, return node's index from which the move was requested
    fn move_up(&mut self) -> usize {
        if self.has_parent() {
            let tmp = self.current;
            self.current = (self.current - 1) / 2;
            return tmp;
        }
        self.current
    }

    /// move directly to the given index
    fn move_to(&mut self, index: usize) {
        self.current = index;
    }

    /// get the current node's value
    fn value(&self) -> i32 {
        self.inner[self.current]
    }

    /// set the current node's value
    fn set_value(&mut self, value: i32) {
        self.inner[self.current] = value;
    }

    /// check that this node has a parent (true for all except root)
    fn has_parent(&self) -> bool {
        self.current > 0
    }

    /// get node's parent's value or self.original if at the root
    fn parent_value(&mut self) -> i32 {
        if self.has_parent() {
            let current = self.move_up();
            let val = self.value();
            self.move_to(current);
            return val;
        }
        self.original
    }

    /// check if the current node has children
    fn has_children(&self) -> bool {
        // inner structure is a complete tree, just check for the right child
        self.current * 2 + 2 <= self.inner.len()
    }

    /// get current node's right child's value
    fn right_child_value(&mut self) -> i32 {
        let tmp = self.move_right();
        let val = self.value();
        self.move_to(tmp);
        val
    }

    /// set current node's left child's value
    fn set_left_child(&mut self) {
        let parent = self.parent_value();
        let current = self.value();
        let value = ((parent - current).abs() / 2) + current;

        self.move_left();
        self.set_value(value);
        self.move_up();
    }

    /// set current node's right child's value
    fn set_right_child(&mut self) {
        let parent = self.parent_value();
        let current = self.value();
        let value = current - ((parent - current).abs() / 2);

        self.move_right();
        self.set_value(value);
        self.move_up();
    }

    /// iterate over the backing array, filling in each child's value based on the original value
    fn build(&mut self) {
        // ex: original is 400
        // arr[0] == 200
        // arr[1] (left child) == 300
        // arr[2] (right child) == 100
        let root = self.original / 2;

        self.inner[0] = root; // set root node to half of the original value
        self.inner[1] = ((self.original - root).abs() / 2) + root;
        self.inner[2] = root - ((self.original - root).abs() / 2);

        // start with index 1 and fill in each child below that node
        for i in 1..self.inner.len() {
            self.move_to(i);

            if self.has_children() && self.right_child_value() == 0 {
                // this node has an unset child since the rchild is 0
                self.set_left_child();
                self.set_right_child();
            }
        }
        self.move_to(0); // reset current index to the root of the tree
    }
}

/// Makes multiple requests based on the presence of extensions
pub(super) struct Requester {
    /// handles to handlers and config
    handles: Arc<Handles>,

    /// url that will be scanned
    target_url: String,

    /// limits requests per second if present
    rate_limiter: RwLock<Option<LeakyBucket>>,

    /// data regarding policy and metadata about last enforced trigger etc...
    policy_data: PolicyData,
}

/// Requester implementation
impl Requester {
    /// given a FeroxScanner, create a Requester
    pub fn from(scanner: &FeroxScanner) -> Result<Self> {
        let limit = scanner.handles.config.rate_limit;

        let rate_limiter = if limit > 0 {
            Some(Self::build_a_bucket(limit)?)
        } else {
            None
        };

        let policy_data = PolicyData::new(scanner.handles.config.requester_policy);

        Ok(Self {
            policy_data,
            rate_limiter: RwLock::new(rate_limiter),
            handles: scanner.handles.clone(),
            target_url: scanner.target_url.to_owned(),
        })
    }

    /// build a LeakyBucket, given a rate limit (as requests per second)
    fn build_a_bucket(limit: usize) -> Result<LeakyBucket> {
        let refill = max(limit / 10, 1); // minimum of 1 per second
        let tokens = max(limit / 2, 1);
        let interval = if refill == 1 { 1000 } else { 100 }; // 1 second if refill is 1

        Ok(LeakyBucket::builder()
            .refill_interval(Duration::from_millis(interval)) // add tokens every 0.1s
            .refill_amount(refill) // ex: 100 req/s -> 10 tokens per 0.1s
            .tokens(tokens) // reduce initial burst, 2 is arbitrary, but felt good
            .max(limit)
            .build()?)
    }

    /// query the statistics handler in order to get the (current) number of requests/second
    async fn get_reqs_sec(&self) -> Result<f64> {
        let reqs = atomic_load!(self.handles.stats.data.requests) as f64;

        let (tx, rx) = oneshot::channel::<f64>();
        self.handles.stats.send(GetRuntime(tx))?;
        let secs = rx.await?;

        Ok(reqs / secs)
    }

    /// sleep and set a flag that can be checked by other threads
    async fn cool_down(&self, wait_time: u64) {
        if atomic_load!(self.policy_data.cooling_down) {
            // prevents a few racy threads making it in here and doubling the wait time erroneously
            return;
        }

        atomic_store!(self.policy_data.cooling_down, true);

        sleep(Duration::from_millis(wait_time)).await;

        atomic_store!(self.policy_data.cooling_down, false);
    }

    /// limit the number of requests per second
    pub async fn limit(&self) -> Result<()> {
        self.rate_limiter
            .read()
            .await
            .as_ref()
            .unwrap()
            .acquire_one()
            .await?;
        Ok(())
    }

    /// simple wrapper to update the appropriate usize field on the Stats object
    fn update_error_field(&self, num_errors: usize, trigger: PolicyTrigger) {
        let field = match trigger {
            PolicyTrigger::Status403 => Enforced403s,
            PolicyTrigger::Status429 => Enforced429s,
            PolicyTrigger::Errors => EnforcedErrors,
        };

        self.handles
            .stats
            .data
            .update_usize_field(field, num_errors);
    }

    /// determine whether or not a policy needs to be enforced
    ///
    /// criteria:
    /// - number of threads (50 default) for general errors (timeouts etc)
    /// - 90% of requests are 403
    /// - 30% of requests are 429
    fn should_enforce_policy(&self) -> Option<PolicyTrigger> {
        if atomic_load!(self.policy_data.cooling_down) {
            // prevents a few racy threads making it in here and doubling the wait time erroneously
            return None;
        }

        let requests = atomic_load!(self.handles.stats.data.requests);

        if requests < max(self.handles.config.threads, 50) {
            // check whether at least a full round of threads has made requests or 50 (default # of
            // threads), whichever is higher
            return None;
        }

        let total_errors = self.handles.stats.data.errors();
        let enforced_errors = self.handles.stats.data.enforced_errors();

        let unenforced_errors = total_errors.saturating_sub(enforced_errors);

        // at least 50 errors
        let threshold = max(self.handles.config.threads, 50);

        if unenforced_errors >= threshold {
            self.update_error_field(unenforced_errors, PolicyTrigger::Errors);
            return Some(PolicyTrigger::Errors);
        }

        let total_403s = self.handles.stats.data.status_403s();
        let enforced_403s = self.handles.stats.data.enforced_403s();

        let unenforced_403s = total_403s.saturating_sub(enforced_403s);

        let ratio_403s = unenforced_403s as f64 / requests as f64;
        if ratio_403s >= HIGH_ERROR_RATIO {
            // almost exclusively 403
            self.update_error_field(unenforced_403s, PolicyTrigger::Status403);
            return Some(PolicyTrigger::Status403);
        }

        let total_429s = self.handles.stats.data.status_429s();
        let enforced_429s = self.handles.stats.data.enforced_429s();

        let unenforced_429s = total_429s.saturating_sub(enforced_429s);

        let ratio_429s = unenforced_429s as f64 / requests as f64;
        if ratio_429s >= HIGH_ERROR_RATIO / 3.0 {
            // high # of 429 responses
            self.update_error_field(unenforced_429s, PolicyTrigger::Status429);
            return Some(PolicyTrigger::Status429);
        }

        None
    }

    /// query the statistics handler for the current number of errors based on the given policy
    async fn get_errors_by_policy(&self, trigger: PolicyTrigger) -> Result<usize> {
        match (self.policy_data.policy, trigger) {
            (RequesterPolicy::AutoBail, PolicyTrigger::Status403) => {
                Ok(self.handles.stats.data.status_403s())
            }
            (RequesterPolicy::AutoBail, PolicyTrigger::Status429) => {
                Ok(self.handles.stats.data.status_429s())
            }
            (RequesterPolicy::AutoBail, PolicyTrigger::Errors) => {
                Ok(self.handles.stats.data.errors())
            }
            (RequesterPolicy::AutoTune, _) => {
                // todo unwrap etc
                let errors = self
                    .handles
                    .ferox_scans()
                    .unwrap()
                    .get_scan_by_url(&self.target_url)
                    .unwrap()
                    .num_errors(trigger);
                Ok(errors)
            }
            // (RequesterPolicy::AutoTune, PolicyTrigger::Status429) => {
            //     // todo unwrap etc
            //     let errors = self.handles.ferox_scans().unwrap().get_scan_by_url(&self.target_url).unwrap().num_errors(trigger);
            //     Ok(errors)
            // }
            // (RequesterPolicy::AutoTune, PolicyTrigger::Errors) => {
            //     // todo unwrap etc
            //     let errors = self.handles.ferox_scans().unwrap().get_scan_by_url(&self.target_url).unwrap().num_errors(trigger);
            //     Ok(errors)
            // }
            // todo autotune error checking isn't quite right, it's checking overall errors then whether
            // or not its personal errors are > or ==, which often leads to over-adjusting down (probably)
            (RequesterPolicy::Default, _) => Ok(0),
        }
    }

    /// todo doc
    async fn adjust_limit(&self, trigger: PolicyTrigger) -> Result<()> {
        let errors = self.get_errors_by_policy(trigger).await?;
        // log::error!("[ADJUST ({})] {}", errors, self.target_url); // todo remove

        if errors > atomic_load!(self.policy_data.errors) {
            // errors have increased, need to reduce the requests/sec limit
            self.policy_data.adjust_down();
            self.policy_data.set_errors(errors);
        } else {
            // errors can only be incremented, so an else is sufficient
            self.policy_data.adjust_up();
        }

        self.set_rate_limiter().await?;

        Ok(())
    }

    /// lock the rate limiter and set its value to ta new leaky_bucket
    async fn set_rate_limiter(&self) -> Result<()> {
        let new_bucket = Self::build_a_bucket(self.policy_data.get_limit())?;
        let mut guard = self.rate_limiter.write().await;
        let _ = std::mem::replace(&mut *guard, Some(new_bucket));
        Ok(())
    }

    /// enforce auto-tune policy
    async fn tune(&self, trigger: PolicyTrigger) -> Result<()> {
        if self.rate_limiter.read().await.is_none() {
            // set original number of reqs/second the first time tune is called, skip otherwise
            let reqs_sec = self.get_reqs_sec().await? as usize;
            self.policy_data.set_reqs_sec(reqs_sec);
            self.set_rate_limiter().await?;
        }

        self.adjust_limit(trigger).await?;

        self.cool_down(WAIT_TIME).await;

        // todo consider setting a 'tune' flag that prevents checking should_enforce, the thought
        // being that once it's a yes, it's always a yes and tuning should be a time based thing

        Ok(())
    }

    /// enforce auto-bail policy
    async fn bail(&self, trigger: PolicyTrigger) -> Result<()> {
        let scans = self.handles.ferox_scans()?;

        let mut scan_tuples = vec![];

        if let Ok(guard) = scans.scans.read() {
            for (i, scan) in guard.iter().enumerate() {
                if scan.is_active() && scan.num_errors(trigger) > 0 {
                    // only active scans that have at least 1 error
                    scan_tuples.push((i, scan.num_errors(trigger)));
                }
            }
        }

        if scan_tuples.is_empty() {
            return Ok(());
        }

        // sort by number of errors
        scan_tuples.sort_unstable_by(|x, y| y.1.cmp(&x.1));

        for (idx, _) in scan_tuples {
            let scan = if let Ok(guard) = scans.scans.read() {
                guard.index(idx).clone()
            } else {
                log::warn!("Could not acquire the FeroxScans.scans lock");
                continue;
            };

            if scan.is_active() {
                log::debug!(
                    "too many {:?} ({}) triggered {:?} Policy on {}",
                    trigger,
                    scan.num_errors(trigger),
                    self.handles.config.requester_policy,
                    scan
                );

                // if allowed to be called within .abort, the inner .await makes it so other
                // in-flight requests don't see the Cancelled status, doing it here ensures a
                // minimum number of requests entering this block
                scan.set_status(ScanStatus::Cancelled)
                    .unwrap_or_else(|e| log::warn!("Could not set scan status: {}", e));

                // set cooldown flag before awaiting the abort to reduce chance of races
                self.cool_down(1500).await;

                // kill the scan
                scan.abort()
                    .await
                    .unwrap_or_else(|e| log::warn!("Could not bail on scan: {}", e));

                // figure out how many requests are skipped as a result
                let pb = scan.progress_bar();
                let num_skipped = pb.length().saturating_sub(pb.position()) as usize;

                // update the overall scan bar by subtracting the number of skipped requests from
                // the total
                self.handles
                    .stats
                    .send(SubtractFromUsizeField(TotalExpected, num_skipped))
                    .unwrap_or_else(|e| log::warn!("Could not update overall scan bar: {}", e));

                break;
            }
        }

        Ok(())
    }

    /// Wrapper for make_request
    ///
    /// Attempts recursion when appropriate and sends Responses to the output handler for processing
    pub async fn request(&self, word: &str) -> Result<()> {
        log::trace!("enter: request({})", word);

        let urls =
            FeroxUrl::from_string(&self.target_url, self.handles.clone()).formatted_urls(word)?;

        for url in urls {
            let should_limit = self.rate_limiter.read().await.is_some();

            if should_limit {
                // found a rate limiter, limit that junk!
                if let Err(e) = self.limit().await {
                    log::warn!("Could not rate limit scan: {}", e);
                    self.handles.stats.send(AddError(Other)).unwrap_or_default();
                }
            }

            let response = logged_request(&url, self.handles.clone()).await?;

            if !atomic_load!(self.policy_data.cooling_down) {
                // only check for policy enforcement when the trigger isn't on cooldown
                match self.policy_data.policy {
                    RequesterPolicy::AutoTune => {
                        // todo check for tune flag and short-circuit the enforce call
                        if atomic_load!(SHOULD_TUNE) {
                            let trigger = *TUNE_TRIGGER.lock().unwrap();
                            self.tune(trigger).await?; // todo may or may not be right to bubble up
                        } else if let Some(trigger) = self.should_enforce_policy() {
                            if let Ok(mut guard) = TUNE_TRIGGER.lock() {
                                *guard = trigger;
                            }
                            atomic_store!(SHOULD_TUNE, true);
                            self.tune(trigger).await?; // todo may or may not be right to bubble up
                        }
                    }
                    RequesterPolicy::AutoBail => {
                        if let Some(trigger) = self.should_enforce_policy() {
                            self.bail(trigger).await?; // todo may or may not be right to bubble up
                        }
                    }
                    RequesterPolicy::Default => {}
                }
            }
            // todo requests/second on scan bar aren't showing
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputLevel;
    use crate::scan_manager::ScanStatus;
    use crate::statistics::StatError;
    use crate::{
        config::Configuration,
        event_handlers::{FiltersHandler, ScanHandler, StatsHandler, Tasks, TermOutHandler},
        filters,
    };
    use crate::{
        scan_manager::FeroxScan,
        scan_manager::{ScanOrder, ScanType},
    };
    use reqwest::StatusCode;

    /// helper to setup a realistic requester test
    async fn setup_requester_test(config: Option<Arc<Configuration>>) -> (Arc<Handles>, Tasks) {
        // basically C&P from main::wrapped_main, can look there for comments etc if needed
        let configuration = config.unwrap_or_else(|| Arc::new(Configuration::new().unwrap()));

        let (stats_task, stats_handle) = StatsHandler::initialize(configuration.clone());
        let (filters_task, filters_handle) = FiltersHandler::initialize();
        let (out_task, out_handle) =
            TermOutHandler::initialize(configuration.clone(), stats_handle.tx.clone());

        let handles = Arc::new(Handles::new(
            stats_handle,
            filters_handle,
            out_handle,
            configuration.clone(),
        ));

        let (scan_task, scan_handle) = ScanHandler::initialize(handles.clone());

        handles.set_scan_handle(scan_handle);
        filters::initialize(handles.clone()).await.unwrap();

        let tasks = Tasks::new(out_task, stats_task, filters_task, scan_task);

        (handles, tasks)
    }

    /// helper to stay DRY
    async fn increment_errors(handles: Arc<Handles>, num_errors: usize) {
        for _ in 0..num_errors {
            handles
                .stats
                .send(Command::AddError(StatError::Other))
                .unwrap();
        }

        handles.stats.sync().await.unwrap();
    }

    /// helper to stay DRY
    async fn increment_scan_errors(handles: Arc<Handles>, url: &str, num_errors: usize) {
        let scans = handles.ferox_scans().unwrap();

        for _ in 0..num_errors {
            if !url.ends_with('/') {
                scans.increment_error(format!("{}/", url).as_str());
            } else {
                scans.increment_error(url);
            };
        }
    }

    /// helper to stay DRY
    async fn increment_scan_status_codes(
        handles: Arc<Handles>,
        url: &str,
        code: StatusCode,
        num_errors: usize,
    ) {
        let scans = handles.ferox_scans().unwrap();
        for _ in 0..num_errors {
            if !url.ends_with('/') {
                scans.increment_status_code(format!("{}/", url).as_str(), code);
            } else {
                scans.increment_status_code(url, code);
            };
        }
    }

    /// helper to stay DRY
    async fn increment_status_codes(handles: Arc<Handles>, num_codes: usize, code: StatusCode) {
        for _ in 0..num_codes {
            handles.stats.send(Command::AddStatus(code)).unwrap();
        }

        handles.stats.sync().await.unwrap();
    }

    async fn create_scan(
        handles: Arc<Handles>,
        url: &str,
        num_errors: usize,
        trigger: PolicyTrigger,
    ) -> Arc<FeroxScan> {
        let scan = FeroxScan::new(
            url,
            ScanType::Directory,
            ScanOrder::Initial,
            1000,
            OutputLevel::Default,
            None,
        );

        scan.set_status(ScanStatus::Running).unwrap();
        scan.progress_bar(); // create a new pb

        let scans = handles.ferox_scans().unwrap();
        scans.insert(scan.clone());

        match trigger {
            PolicyTrigger::Status403 => {
                increment_scan_status_codes(
                    handles.clone(),
                    url,
                    StatusCode::FORBIDDEN,
                    num_errors,
                )
                .await;
            }
            PolicyTrigger::Status429 => {
                increment_scan_status_codes(
                    handles.clone(),
                    url,
                    StatusCode::TOO_MANY_REQUESTS,
                    num_errors,
                )
                .await;
            }
            PolicyTrigger::Errors => {
                increment_scan_errors(handles.clone(), url, num_errors).await;
            }
        }

        assert_eq!(scan.num_errors(trigger), num_errors);

        scan
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// should_enforce_policy should return false when # of requests is < threads; also when < 50
    async fn should_enforce_policy_returns_false_on_not_enough_requests_seen() {
        let (handles, _) = setup_requester_test(None).await;

        let requester = Requester {
            handles,
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
        };

        increment_errors(requester.handles.clone(), 49).await;
        // 49 errors is false because we haven't hit the min threshold
        assert_eq!(atomic_load!(requester.handles.stats.data.requests), 49);
        assert_eq!(requester.should_enforce_policy(), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// should_enforce_policy should return true when # of requests is >= 50 and errors >= threads * 2
    async fn should_enforce_policy_returns_true_on_error_times_threads() {
        let mut config = Configuration::new().unwrap_or_default();
        config.threads = 50;

        let (handles, _) = setup_requester_test(Some(Arc::new(config))).await;

        let requester = Requester {
            handles,
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
        };

        increment_errors(requester.handles.clone(), 25).await;
        assert_eq!(requester.should_enforce_policy(), None);
        increment_errors(requester.handles.clone(), 25).await;
        assert_eq!(
            requester.should_enforce_policy(),
            Some(PolicyTrigger::Errors)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// should_enforce_policy should return true when # of requests is >= 50 and 403s >= 45 (90%)
    async fn should_enforce_policy_returns_true_on_excessive_403s() {
        let (handles, _) = setup_requester_test(None).await;

        let requester = Requester {
            handles,
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
        };

        increment_status_codes(requester.handles.clone(), 45, StatusCode::FORBIDDEN).await;
        assert_eq!(requester.should_enforce_policy(), None);
        increment_status_codes(requester.handles.clone(), 5, StatusCode::OK).await;
        assert_eq!(
            requester.should_enforce_policy(),
            Some(PolicyTrigger::Status403)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// should_enforce_policy should return true when # of requests is >= 50 and errors >= 45 (90%)
    async fn should_enforce_policy_returns_true_on_excessive_429s() {
        let mut config = Configuration::new().unwrap_or_default();
        config.threads = 50;

        let (handles, _) = setup_requester_test(Some(Arc::new(config))).await;

        let requester = Requester {
            handles,
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
        };

        increment_status_codes(requester.handles.clone(), 15, StatusCode::TOO_MANY_REQUESTS).await;
        assert_eq!(requester.should_enforce_policy(), None);
        increment_status_codes(requester.handles.clone(), 35, StatusCode::OK).await;
        assert_eq!(
            requester.should_enforce_policy(),
            Some(PolicyTrigger::Status429)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// bail should call abort on the scan with the most errors
    async fn bail_calls_abort_on_highest_errored_feroxscan() {
        let (handles, _) = setup_requester_test(None).await;

        let scan_one = create_scan(handles.clone(), "http://one", 10, PolicyTrigger::Errors).await;
        let scan_two = create_scan(handles.clone(), "http://two", 14, PolicyTrigger::Errors).await;
        let scan_three =
            create_scan(handles.clone(), "http://three", 4, PolicyTrigger::Errors).await;
        let scan_four = create_scan(handles.clone(), "http://four", 7, PolicyTrigger::Errors).await;

        // set up a fake JoinHandle for the scan that's expected to have .abort called on it
        // the reason being if there's no task, the status is never updated, so can't be checked
        let dummy_task =
            tokio::spawn(async move { tokio::time::sleep(Duration::new(15, 0)).await });
        scan_two.set_task(dummy_task).await.unwrap();

        assert!(scan_one.is_active());
        assert!(scan_two.is_active());

        let scans = handles.ferox_scans().unwrap();
        assert_eq!(scans.get_active_scans().len(), 4);

        let requester = Requester {
            handles,
            target_url: "http://one/one/stuff.php".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
        };

        requester.bail(PolicyTrigger::Errors).await.unwrap();
        assert_eq!(scans.get_active_scans().len(), 3);
        assert!(scan_one.is_active());
        assert!(scan_three.is_active());
        assert!(scan_four.is_active());
        assert!(!scan_two.is_active());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// bail is ok when no active scans are found
    async fn bail_returns_ok_on_no_active_scans() {
        let (handles, _) = setup_requester_test(None).await;

        let scan_one =
            create_scan(handles.clone(), "http://one", 10, PolicyTrigger::Status403).await;
        let scan_two =
            create_scan(handles.clone(), "http://two", 10, PolicyTrigger::Status429).await;

        scan_one.set_status(ScanStatus::Complete).unwrap();
        scan_two.set_status(ScanStatus::Cancelled).unwrap();

        let scans = handles.ferox_scans().unwrap();
        assert_eq!(scans.get_active_scans().len(), 0);

        let requester = Requester {
            handles,
            target_url: "http://one/one/stuff.php".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
        };

        let result = requester.bail(PolicyTrigger::Status403).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// should_enforce should early exit when cooldown flag is set
    async fn should_enforce_policy_returns_none_on_cooldown() {
        let mut config = Configuration::new().unwrap_or_default();
        config.threads = 50;

        let (handles, _) = setup_requester_test(Some(Arc::new(config))).await;

        let requester = Requester {
            handles,
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
        };

        requester
            .policy_data
            .cooling_down
            .store(true, Ordering::Relaxed);

        assert_eq!(requester.should_enforce_policy(), None);
    }
}
