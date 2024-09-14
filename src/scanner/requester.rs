use std::{
    cmp::max,
    collections::HashSet,
    sync::{
        self,
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use anyhow::Result;
use console::style;
use lazy_static::lazy_static;
use leaky_bucket::RateLimiter;
use tokio::{
    sync::RwLock,
    time::{sleep, Duration},
};

use crate::{
    atomic_load, atomic_store,
    config::RequesterPolicy,
    event_handlers::{
        Command::{AddError, SubtractFromUsizeField},
        Handles,
    },
    extractor::{ExtractionTarget, ExtractorBuilder},
    nlp::{Document, TfIdf},
    response::FeroxResponse,
    scan_manager::{FeroxScan, ScanStatus},
    statistics::{StatError::Other, StatField::TotalExpected},
    url::FeroxUrl,
    utils::{logged_request, send_try_recursion_command, should_deny_url},
    HIGH_ERROR_RATIO,
};

use super::{policy_data::PolicyData, FeroxScanner, PolicyTrigger};

lazy_static! {
    /// make sure to note that this is a std rwlock and not tokio
    pub(crate) static ref TF_IDF: Arc<sync::RwLock<TfIdf>> = Arc::new(sync::RwLock::new(TfIdf::new()));
}

/// Makes multiple requests based on the presence of extensions
pub(super) struct Requester {
    /// handles to handlers and config
    handles: Arc<Handles>,

    /// url that will be scanned
    target_url: String,

    /// limits requests per second if present
    rate_limiter: RwLock<Option<RateLimiter>>,

    /// data regarding policy and metadata about last enforced trigger etc...
    policy_data: PolicyData,

    /// FeroxScan associated with the creation of this Requester
    ferox_scan: Arc<FeroxScan>,

    /// cache of previously seen links gotten via link extraction. since the requester is passed
    /// around as an arc, and seen_links needs to be mutable, putting it behind a lock for
    /// interior mutability, similar to the tuning_lock below
    seen_links: RwLock<HashSet<String>>,

    /// simple lock to control access to tuning to a single thread (per-scan)
    ///
    /// need a usize to determine the number of consecutive non-error calls that a requester has
    /// seen; this will satisfy the non-mut self constraint (due to us being behind an Arc, and
    /// the need for a counter)
    tuning_lock: Mutex<usize>,

    policy_triggered: AtomicBool,
}

/// Requester implementation
impl Requester {
    /// given a FeroxScanner, create a Requester
    pub fn from(scanner: &FeroxScanner, ferox_scan: Arc<FeroxScan>) -> Result<Self> {
        let limit = scanner.handles.config.rate_limit;

        let rate_limiter = if limit > 0 {
            Some(Self::build_a_bucket(limit)?)
        } else {
            None
        };

        let policy_data = PolicyData::new(
            scanner.handles.config.requester_policy,
            scanner.handles.config.timeout,
        );

        Ok(Self {
            ferox_scan,
            policy_data,
            seen_links: RwLock::new(HashSet::<String>::new()),
            rate_limiter: RwLock::new(rate_limiter),
            handles: scanner.handles.clone(),
            target_url: scanner.target_url.to_owned(),
            tuning_lock: Mutex::new(0),
            policy_triggered: AtomicBool::new(false),
        })
    }

    /// build a RateLimiter, given a rate limit (as requests per second)
    fn build_a_bucket(limit: usize) -> Result<RateLimiter> {
        let refill = max((limit as f64 / 10.0).round() as usize, 1); // minimum of 1 per second
        let tokens = max((limit as f64 / 2.0).round() as usize, 1);
        let interval = if refill == 1 { 1000 } else { 100 }; // 1 second if refill is 1

        Ok(RateLimiter::builder()
            .interval(Duration::from_millis(interval)) // add tokens every 0.1s
            .refill(refill) // ex: 100 req/s -> 10 tokens per 0.1s
            .initial(tokens) // reduce initial burst, 2 is arbitrary, but felt good
            .max(limit)
            .build())
    }

    /// sleep and set a flag that can be checked by other threads
    async fn cool_down(&self) {
        if atomic_load!(self.policy_data.cooling_down, Ordering::SeqCst) {
            // prevents a few racy threads making it in here and doubling the wait time erroneously
            return;
        }

        atomic_store!(self.policy_data.cooling_down, true, Ordering::SeqCst);

        sleep(Duration::from_millis(self.policy_data.wait_time)).await;
        self.ferox_scan.progress_bar().set_message("");

        atomic_store!(self.policy_data.cooling_down, false, Ordering::SeqCst);
    }

    /// limit the number of requests per second
    pub async fn limit(&self) -> Result<()> {
        let guard = self.rate_limiter.read().await;

        if guard.is_some() {
            guard.as_ref().unwrap().acquire_one().await;
        }

        Ok(())
    }

    /// small function to break out different error checking mechanisms
    fn too_many_errors(&self) -> bool {
        let total = self.ferox_scan.num_errors(PolicyTrigger::Errors);

        // at least 25 errors
        let threshold = max(self.handles.config.threads / 2, 25);

        total >= threshold
    }

    /// small function to break out different error checking mechanisms
    fn too_many_status_errors(&self, trigger: PolicyTrigger) -> bool {
        let total = self.ferox_scan.num_errors(trigger);
        let requests = self.ferox_scan.requests();

        let ratio = total as f64 / requests as f64;

        match trigger {
            PolicyTrigger::Status403 => ratio >= HIGH_ERROR_RATIO,
            PolicyTrigger::Status429 => ratio >= HIGH_ERROR_RATIO / 3.0,
            _ => false,
        }
    }

    /// determine whether or not a policy needs to be enforced
    ///
    /// criteria:
    /// - number of threads (50 default) for general errors (timeouts etc)
    /// - 90% of requests are 403
    /// - 30% of requests are 429
    fn should_enforce_policy(&self) -> Option<PolicyTrigger> {
        if atomic_load!(self.policy_data.cooling_down, Ordering::SeqCst) {
            // prevents a few racy threads making it in here and doubling the wait time erroneously
            return None;
        }

        let requests = atomic_load!(self.handles.stats.data.requests);

        if requests < max(self.handles.config.threads, 50) {
            // check whether at least a full round of threads has made requests or 50 (default # of
            // threads), whichever is higher
            return None;
        }

        if self.too_many_errors() {
            return Some(PolicyTrigger::Errors);
        }

        if self.too_many_status_errors(PolicyTrigger::Status403) {
            return Some(PolicyTrigger::Status403);
        }

        if self.too_many_status_errors(PolicyTrigger::Status429) {
            return Some(PolicyTrigger::Status429);
        }

        None
    }

    /// wrapper for adjust_[up,down] functions, checks error levels to determine adjustment direction
    async fn adjust_limit(&self, trigger: PolicyTrigger, create_limiter: bool) -> Result<()> {
        let scan_errors = self.ferox_scan.num_errors(trigger);
        let policy_errors = atomic_load!(self.policy_data.errors, Ordering::SeqCst);

        if let Ok(mut guard) = self.tuning_lock.try_lock() {
            if scan_errors > policy_errors {
                // errors have increased, need to reduce the requests/sec limit
                *guard = 0; // reset streak counter to 0
                if atomic_load!(self.policy_data.errors) != 0 {
                    self.policy_data.adjust_down();

                    let styled_direction = style("reduced").red();

                    self.ferox_scan
                        .progress_bar()
                        .set_message(format!("=> 🚦 {styled_direction} scan speed",));
                }
                self.policy_data.set_errors(scan_errors);
            } else {
                // errors can only be incremented, so an else is sufficient
                *guard += 1;

                self.policy_data.adjust_up(&guard);

                let styled_direction = style("increased").green();

                self.ferox_scan
                    .progress_bar()
                    .set_message(format!("=> 🚦 {styled_direction} scan speed",));
            }
        }

        if atomic_load!(self.policy_data.remove_limit) {
            self.set_rate_limiter(None).await?;
            atomic_store!(self.policy_data.remove_limit, false);
            self.ferox_scan
                .progress_bar()
                .set_message("=> 🚦 removed rate limiter 🚀");
        } else if create_limiter {
            // create_limiter is really just used for unit testing situations, it's true anytime
            // during actual execution
            let new_limit = self.policy_data.get_limit(); // limit is set from within the lock
            self.set_rate_limiter(Some(new_limit)).await?;
        }

        Ok(())
    }

    /// lock the rate limiter and set its value to ta new leaky_bucket
    async fn set_rate_limiter(&self, new_limit: Option<usize>) -> Result<()> {
        let mut guard = self.rate_limiter.write().await;

        let new_bucket = if new_limit.is_none() {
            // got None, need to remove the rate_limiter
            None
        } else if guard.is_some() && guard.as_ref().unwrap().max() == new_limit.unwrap() {
            // new_limit is checked for None in first branch, should be fine to unwrap

            // this function is called more often than i'd prefer due to Send requirements of
            // mutex/rwlock primitives and awaits, this will minimize the cost of the extra calls
            return Ok(());
        } else {
            Some(Self::build_a_bucket(new_limit.unwrap())?)
        };

        let _ = std::mem::replace(&mut *guard, new_bucket);
        Ok(())
    }

    /// enforce auto-tune policy
    async fn tune(&self, trigger: PolicyTrigger) -> Result<()> {
        if atomic_load!(self.policy_data.errors) == 0 {
            // set original number of reqs/second the first time tune is called, skip otherwise
            let reqs_sec = self.ferox_scan.requests_per_second() as usize;
            self.policy_data.set_reqs_sec(reqs_sec);

            // set the flag to indicate that we have triggered the rate limiter
            // at least once
            atomic_store!(self.policy_triggered, true);

            let new_limit = self.policy_data.get_limit();
            self.set_rate_limiter(Some(new_limit)).await?;
            self.ferox_scan
                .progress_bar()
                .set_message(format!("=> 🚦 set rate limit ({new_limit}/s)"));
        }

        self.adjust_limit(trigger, true).await?;
        self.cool_down().await;

        Ok(())
    }

    /// enforce auto-bail policy
    async fn bail(&self, trigger: PolicyTrigger) -> Result<()> {
        if self.ferox_scan.is_active() {
            log::warn!(
                "too many {:?} ({}) triggered {:?} Policy on {}",
                trigger,
                self.ferox_scan.num_errors(trigger),
                self.handles.config.requester_policy,
                self.ferox_scan
            );

            // if allowed to be called within .abort, the inner .await makes it so other
            // in-flight requests don't see the Cancelled status, doing it here ensures a
            // minimum number of requests entering this block
            self.ferox_scan
                .set_status(ScanStatus::Cancelled)
                .unwrap_or_else(|e| log::warn!("Could not set scan status: {}", e));

            let scans = self.handles.ferox_scans()?;
            let active_bars = scans.number_of_bars();

            // kill the scan
            self.ferox_scan
                .abort(active_bars)
                .await
                .unwrap_or_else(|e| log::warn!("Could not bail on scan: {}", e));

            // figure out how many requests are skipped as a result
            let pb = self.ferox_scan.progress_bar();
            let num_skipped = pb.length().unwrap_or(0).saturating_sub(pb.position()) as usize;

            let styled_trigger = style(format!("{trigger:?}")).red();

            pb.set_message(format!(
                "=> 💀 too many {} ({}) 💀 bailing",
                styled_trigger,
                self.ferox_scan.num_errors(trigger),
            ));

            // update the overall scan bar by subtracting the number of skipped requests from
            // the total
            self.handles
                .stats
                .send(SubtractFromUsizeField(TotalExpected, num_skipped))
                .unwrap_or_else(|e| log::warn!("Could not update overall scan bar: {}", e));
        }

        Ok(())
    }

    /// Wrapper for make_request
    ///
    /// Attempts recursion when appropriate and sends Responses to the output handler for processing
    pub async fn request(&self, word: &str) -> Result<()> {
        log::trace!("enter: request({})", word);

        let collected = self.handles.collected_extensions();

        let urls = FeroxUrl::from_string(&self.target_url, self.handles.clone())
            .formatted_urls(word, collected)?;

        let should_test_deny = !self.handles.config.url_denylist.is_empty()
            || !self.handles.config.regex_denylist.is_empty();

        for url in urls {
            for method in self.handles.config.methods.iter() {
                // auto_tune is true, or rate_limit was set (mutually exclusive to user)
                // and a rate_limiter has been created
                // short-circuiting the lock access behind the first boolean check
                let should_tune =
                    self.handles.config.auto_tune || self.handles.config.rate_limit > 0;
                let should_limit = should_tune && self.rate_limiter.read().await.is_some();

                if should_limit {
                    // found a rate limiter, limit that junk!
                    if let Err(e) = self.limit().await {
                        log::warn!("Could not rate limit scan: {}", e);
                        self.handles.stats.send(AddError(Other)).unwrap_or_default();
                    }
                }

                if should_test_deny && should_deny_url(&url, self.handles.clone())? {
                    // can't allow a denied url to be requested
                    continue;
                }

                let data = if self.handles.config.data.is_empty() {
                    None
                } else {
                    Some(self.handles.config.data.as_slice())
                };

                let response =
                    logged_request(&url, method.as_str(), data, self.handles.clone()).await?;

                if (should_tune || self.handles.config.auto_bail)
                    && !atomic_load!(self.policy_data.cooling_down, Ordering::SeqCst)
                {
                    // only check for policy enforcement when the trigger isn't on cooldown and tuning
                    // or bailing is in place (should_tune used here because when auto-tune is on, we'll
                    // reach this without a rate_limiter in place)
                    match self.policy_data.policy {
                        RequesterPolicy::AutoTune => {
                            if let Some(trigger) = self.should_enforce_policy() {
                                self.tune(trigger).await?;
                            } else if atomic_load!(self.policy_triggered) {
                                self.adjust_limit(PolicyTrigger::TryAdjustUp, true).await?;
                                self.cool_down().await;
                            }
                        }
                        RequesterPolicy::AutoBail => {
                            if let Some(trigger) = self.should_enforce_policy() {
                                self.bail(trigger).await?;
                            }
                        }
                        RequesterPolicy::Default => {}
                    }
                }

                // response came back without error, convert it to FeroxResponse
                let mut ferox_response = FeroxResponse::from(
                    response,
                    &self.target_url,
                    method,
                    self.handles.config.output_level,
                )
                .await;

                // do recursion if appropriate
                if !self.handles.config.no_recursion && !self.handles.config.force_recursion {
                    // to support --force-recursion, we want to limit recursive calls to only
                    // 'found' assets. That means we need to either gate or delay the call.
                    //
                    // this branch will retain the 'old' behavior by checking that
                    // --force-recursion isn't turned on
                    send_try_recursion_command(self.handles.clone(), ferox_response.clone())
                        .await?;
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

                if !self.handles.config.no_recursion && self.handles.config.force_recursion {
                    // in this branch, we're saying that both recursion AND force recursion
                    // are turned on. It comes after should_filter_response, so those cases
                    // are handled. Now we need to account for -s/-C options.

                    if self.handles.config.filter_status.is_empty() {
                        // -C wasn't used, so -s is the only 'filter' left to account for
                        if self
                            .handles
                            .config
                            .status_codes
                            .contains(&ferox_response.status().as_u16())
                        {
                            send_try_recursion_command(
                                self.handles.clone(),
                                ferox_response.clone(),
                            )
                            .await?;
                        }
                    } else {
                        // -C was used, that means the filters above would have removed
                        // those responses, and anything else should be let through
                        send_try_recursion_command(self.handles.clone(), ferox_response.clone())
                            .await?;
                    }
                }

                if self.handles.config.collect_extensions {
                    ferox_response.parse_extension(self.handles.clone())?;
                }

                if self.handles.config.collect_words {
                    if let Ok(mut guard) = TF_IDF.write() {
                        if let Some(doc) = Document::from_html(ferox_response.text()) {
                            guard.add_document(doc);
                            if guard.num_documents() % 12 == 0
                                || (guard.num_documents() < 5 && guard.num_documents() % 2 == 0)
                            {
                                guard.calculate_tf_idf_scores();
                            }
                        }
                    }
                }

                if self.handles.config.extract_links {
                    let mut extractor = ExtractorBuilder::default()
                        .target(ExtractionTarget::ResponseBody)
                        .response(&ferox_response)
                        .handles(self.handles.clone())
                        .url(self.ferox_scan.url())
                        .build()?;

                    let new_links: HashSet<_>;

                    let result = extractor.extract().await?;

                    {
                        // gain and quickly drop the read lock on seen_links, using it while unlocked
                        // to determine if there are any new links to process
                        let read_links = self.seen_links.read().await;
                        new_links = result.difference(&read_links).cloned().collect();
                    }

                    if !new_links.is_empty() {
                        // using is_empty instead of direct iteration to acquire the write lock behind
                        // some kind of less expensive gate (and not in a loop, obv)
                        let mut write_links = self.seen_links.write().await;
                        for new_link in &new_links {
                            write_links.insert(new_link.to_owned());
                        }
                    }

                    if !new_links.is_empty() {
                        let extraction_task = extractor.request_links(new_links).await?;

                        if let Some(task) = extraction_task {
                            _ = task.await;
                        }
                    }
                }

                // everything else should be reported
                if let Err(e) = ferox_response.send_report(self.handles.output.tx.clone()) {
                    log::warn!("Could not send FeroxResponse to output handler: {}", e);
                }
            }
        }

        log::trace!("exit: request");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use reqwest::StatusCode;

    use crate::{
        config::Configuration,
        config::OutputLevel,
        event_handlers::Command::AddStatus,
        event_handlers::{FiltersHandler, ScanHandler, StatsHandler, Tasks, TermOutHandler},
        filters,
        scan_manager::{ScanOrder, ScanType},
        statistics::StatError,
    };

    use super::*;

    /// helper to setup a realistic requester test
    async fn setup_requester_test(config: Option<Arc<Configuration>>) -> (Arc<Handles>, Tasks) {
        // basically C&P from main::wrapped_main, can look there for comments etc if needed
        let configuration = config.unwrap_or_else(|| Arc::new(Configuration::new().unwrap()));

        let (stats_task, stats_handle) = StatsHandler::initialize(configuration.clone());
        let (filters_task, filters_handle) = FiltersHandler::initialize();
        let (out_task, out_handle) =
            TermOutHandler::initialize(configuration.clone(), stats_handle.tx.clone());
        let wordlist = Arc::new(vec![String::from("this_is_a_test")]);

        let handles = Arc::new(Handles::new(
            stats_handle,
            filters_handle,
            out_handle,
            configuration.clone(),
            wordlist,
        ));

        let (scan_task, scan_handle) = ScanHandler::initialize(handles.clone());

        handles.set_scan_handle(scan_handle);
        filters::initialize(handles.clone()).await.unwrap();

        let tasks = Tasks::new(out_task, stats_task, filters_task, scan_task);

        (handles, tasks)
    }

    /// helper to stay DRY
    async fn increment_errors(handles: Arc<Handles>, scan: Arc<FeroxScan>, num_errors: usize) {
        for _ in 0..num_errors {
            handles.stats.send(AddError(StatError::Other)).unwrap();
            scan.add_error();
        }

        handles.stats.sync().await.unwrap();
    }

    /// helper to stay DRY
    async fn increment_scan_errors(handles: Arc<Handles>, url: &str, num_errors: usize) {
        let scans = handles.ferox_scans().unwrap();

        for _ in 0..num_errors {
            scans.increment_error(format!("{url}/").as_str());
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
            scans.increment_status_code(format!("{url}/").as_str(), code);
        }
    }

    /// helper to stay DRY
    async fn increment_status_codes(
        handles: Arc<Handles>,
        scan: Arc<FeroxScan>,
        num_codes: usize,
        code: StatusCode,
    ) {
        for _ in 0..num_codes {
            handles.stats.send(AddStatus(code)).unwrap();
            if code == StatusCode::FORBIDDEN {
                scan.add_403();
            } else {
                scan.add_429();
            }
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
            true,
            handles.clone(),
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
            _ => {}
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
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(FeroxScan::default()),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
            policy_triggered: AtomicBool::new(false),
        };

        let ferox_scan = Arc::new(FeroxScan::default());

        increment_errors(requester.handles.clone(), ferox_scan.clone(), 49).await;
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

        let ferox_scan = Arc::new(FeroxScan::default());

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: ferox_scan.clone(),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
            policy_triggered: AtomicBool::new(false),
        };

        increment_errors(requester.handles.clone(), ferox_scan.clone(), 25).await;
        assert_eq!(requester.should_enforce_policy(), None);
        increment_errors(requester.handles.clone(), ferox_scan, 25).await;
        assert_eq!(
            requester.should_enforce_policy(),
            Some(PolicyTrigger::Errors)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// should_enforce_policy should return true when # of requests is >= 50 and 403s >= 45 (90%)
    async fn should_enforce_policy_returns_true_on_excessive_403s() {
        let (handles, _) = setup_requester_test(None).await;
        let ferox_scan = Arc::new(FeroxScan::default());

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: ferox_scan.clone(),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
            policy_triggered: AtomicBool::new(false),
        };

        increment_status_codes(
            requester.handles.clone(),
            ferox_scan.clone(),
            45,
            StatusCode::FORBIDDEN,
        )
        .await;
        assert_eq!(requester.should_enforce_policy(), None);
        increment_status_codes(
            requester.handles.clone(),
            ferox_scan.clone(),
            5,
            StatusCode::OK,
        )
        .await;
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
        let ferox_scan = Arc::new(FeroxScan::default());

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: ferox_scan.clone(),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
            policy_triggered: AtomicBool::new(false),
        };

        increment_status_codes(
            requester.handles.clone(),
            ferox_scan.clone(),
            15,
            StatusCode::TOO_MANY_REQUESTS,
        )
        .await;
        assert_eq!(requester.should_enforce_policy(), None);
        increment_status_codes(
            requester.handles.clone(),
            ferox_scan.clone(),
            35,
            StatusCode::OK,
        )
        .await;
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

        let req_clone = scan_two.clone();
        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: req_clone,
            target_url: "http://one/one/stuff.php".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
            policy_triggered: AtomicBool::new(false),
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
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(FeroxScan::default()),
            target_url: "http://one/one/stuff.php".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
            policy_triggered: AtomicBool::new(false),
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
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(FeroxScan::default()),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: Default::default(),
            policy_triggered: AtomicBool::new(false),
        };

        requester
            .policy_data
            .cooling_down
            .store(true, Ordering::Relaxed);

        assert_eq!(requester.should_enforce_policy(), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// cooldown should pause execution and prevent others calling it by setting cooling_down flag
    async fn cooldown_pauses_and_sets_flag() {
        let (handles, _) = setup_requester_test(None).await;

        let requester = Arc::new(Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(FeroxScan::default()),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: PolicyData::new(RequesterPolicy::AutoBail, 7),
            policy_triggered: AtomicBool::new(false),
        });

        let start = Instant::now();
        let clone = requester.clone();
        let resp = tokio::task::spawn(async move {
            sleep(Duration::new(1, 0)).await;
            clone.policy_data.cooling_down.load(Ordering::Relaxed)
        });

        requester.cool_down().await;

        assert!(resp.await.unwrap());
        println!("{}", start.elapsed().as_millis());
        assert!(start.elapsed().as_millis() >= 3500);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// adjust_limit should add one to the streak counter when errors from scan equal policy and
    /// increase the scan rate
    async fn adjust_limit_increments_streak_counter_on_upward_movement() {
        let (handles, _) = setup_requester_test(None).await;

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(FeroxScan::default()),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: PolicyData::new(RequesterPolicy::AutoBail, 7),
            policy_triggered: AtomicBool::new(false),
        };

        requester.policy_data.set_reqs_sec(400);
        requester
            .adjust_limit(PolicyTrigger::Errors, true)
            .await
            .unwrap();

        assert_eq!(*requester.tuning_lock.lock().unwrap(), 1);
        assert_eq!(requester.policy_data.get_limit(), 300);
        assert_eq!(
            requester.rate_limiter.read().await.as_ref().unwrap().max(),
            300
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// adjust_limit should reset the streak counter when errors from scan are > policy and
    /// decrease the scan rate
    async fn adjust_limit_resets_streak_counter_on_downward_movement() {
        let (handles, _) = setup_requester_test(None).await;
        let limiter = RateLimiter::builder()
            .interval(Duration::from_secs(1))
            .max(200)
            .build();

        let scan = FeroxScan::default();
        scan.add_error();
        scan.add_error();

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(scan),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(Some(limiter)),
            policy_data: PolicyData::new(RequesterPolicy::AutoBail, 7),
            policy_triggered: AtomicBool::new(false),
        };

        requester.policy_data.set_reqs_sec(400);
        requester.policy_data.set_errors(1);

        {
            let mut guard = requester.tuning_lock.lock().unwrap();
            *guard = 2;
        }

        requester
            .adjust_limit(PolicyTrigger::Errors, false)
            .await
            .unwrap();

        assert_eq!(*requester.tuning_lock.lock().unwrap(), 0);
        assert_eq!(requester.policy_data.get_limit(), 100);
        assert_eq!(requester.policy_data.errors.load(Ordering::Relaxed), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// adjust_limit should remove the rate limiter when remove_limit is set
    async fn adjust_limit_removes_rate_limiter() {
        let (handles, _) = setup_requester_test(None).await;

        let scan = FeroxScan::default();
        scan.add_error();
        scan.add_error();

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(scan),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: PolicyData::new(RequesterPolicy::AutoBail, 7),
            policy_triggered: AtomicBool::new(false),
        };

        requester.policy_data.set_reqs_sec(400);
        requester
            .policy_data
            .remove_limit
            .store(true, Ordering::Relaxed);

        requester
            .adjust_limit(PolicyTrigger::Errors, true)
            .await
            .unwrap();
        assert!(requester.rate_limiter.read().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// errors policytrigger should always be false, 403 is high ratio, and 429 is high ratio / 3
    async fn too_many_status_errors_returns_correct_values() {
        let (handles, _) = setup_requester_test(None).await;

        let mut requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(FeroxScan::default()),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(None),
            policy_data: PolicyData::new(RequesterPolicy::AutoBail, 7),
            policy_triggered: AtomicBool::new(false),
        };

        assert!(!requester.too_many_status_errors(PolicyTrigger::Errors));

        assert!(!requester.too_many_status_errors(PolicyTrigger::Status429));
        requester.ferox_scan.progress_bar().set_position(10);
        requester.ferox_scan.add_429();
        requester.ferox_scan.add_429();
        requester.ferox_scan.add_429();
        assert!(requester.too_many_status_errors(PolicyTrigger::Status429));

        assert!(!requester.too_many_status_errors(PolicyTrigger::Status403));
        requester.ferox_scan = Arc::new(FeroxScan::default());
        requester.ferox_scan.progress_bar().set_position(10);
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        requester.ferox_scan.add_403();
        assert!(requester.too_many_status_errors(PolicyTrigger::Status403));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// set_rate_limiter should exit early when new limit equals the current bucket's max
    async fn set_rate_limiter_early_exit() {
        let (handles, _) = setup_requester_test(None).await;
        let limiter = RateLimiter::builder()
            .interval(Duration::from_secs(1))
            .max(200)
            .build();

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: Arc::new(FeroxScan::default()),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(Some(limiter)),
            policy_data: PolicyData::new(RequesterPolicy::AutoBail, 7),
            policy_triggered: AtomicBool::new(false),
        };

        requester.set_rate_limiter(Some(200)).await.unwrap();
        assert_eq!(
            requester.rate_limiter.read().await.as_ref().unwrap().max(),
            200
        );
        requester.set_rate_limiter(Some(200)).await.unwrap();
        assert_eq!(
            requester.rate_limiter.read().await.as_ref().unwrap().max(),
            200
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// tune should set req/sec and rate_limiter, adjust the limit and cooldown
    async fn tune_sets_expected_values_and_then_waits() {
        let (handles, _) = setup_requester_test(None).await;

        let limiter = RateLimiter::builder()
            .interval(Duration::from_secs(1))
            .max(200)
            .build();

        let scan = FeroxScan::new(
            "http://localhost",
            ScanType::Directory,
            ScanOrder::Initial,
            1000,
            OutputLevel::Default,
            None,
            true,
            Arc::new(Handles::for_testing(None, None).0),
        );
        scan.set_status(ScanStatus::Running).unwrap();
        scan.add_429();

        let requester = Requester {
            handles,
            seen_links: RwLock::new(HashSet::<String>::new()),
            tuning_lock: Mutex::new(0),
            ferox_scan: scan.clone(),
            target_url: "http://localhost".to_string(),
            rate_limiter: RwLock::new(Some(limiter)),
            policy_data: PolicyData::new(RequesterPolicy::AutoTune, 4),
            policy_triggered: AtomicBool::new(false),
        };

        let start = Instant::now();

        let pb = scan.progress_bar();
        pb.set_length(1000);
        pb.set_position(400);
        sleep(Duration::new(1, 0)).await; // used to get req/sec up to 400

        assert_eq!(requester.policy_data.errors.load(Ordering::Relaxed), 0);

        requester.tune(PolicyTrigger::Status429).await.unwrap();

        assert_eq!(requester.policy_data.heap.read().unwrap().original, 400);
        assert_eq!(requester.policy_data.get_limit(), 200);
        assert_eq!(
            requester.rate_limiter.read().await.as_ref().unwrap().max(),
            200
        );

        scan.finish(0).unwrap();
        assert!(start.elapsed().as_millis() >= 2000);
    }
}
