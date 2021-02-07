use super::FeroxScanner;
use crate::{
    config::RequesterPolicy,
    event_handlers::{
        Command::{self, AddError},
        Handles,
    },
    extractor::{ExtractionTarget::ResponseBody, ExtractorBuilder},
    progress::PROGRESS_PRINTER,
    response::FeroxResponse,
    statistics::StatError::Other,
    url::FeroxUrl,
    utils::logged_request,
    HIGH_ERROR_RATIO,
};
use anyhow::Result;
use leaky_bucket::LeakyBucket;
use std::ops::Index;
use std::sync::atomic::Ordering;
use std::{cmp::max, sync::Arc};
use tokio::{sync::oneshot, time::Duration};

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

/// Makes multiple requests based on the presence of extensions
pub(super) struct Requester {
    /// handles to handlers and config
    handles: Arc<Handles>,

    /// url that will be scanned
    target_url: String,

    /// limits requests per second if present
    rate_limiter: Option<LeakyBucket>,

    /// how to handle exceptional cases such as too many errors / 403s / 429s etc
    policy: RequesterPolicy,
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

        let policy = scanner.handles.config.requester_policy;

        Ok(Self {
            policy,
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

    /// determine whether or not a policy needs to be enforce
    ///
    /// criteria:
    /// - threads * 2 for general errors (timeouts etc)
    /// - 90% of requests are 403
    /// - 30% of requests are 429
    fn should_enforce_policy(&self) -> Option<PolicyTrigger> {
        let requests = self.handles.stats.data.requests.load(Ordering::Relaxed);

        if requests < max(self.handles.config.threads, 50) {
            // check whether at least a full round of threads has made requests or 50 (default # of
            // threads), whichever is higher
            return None;
        }

        let errors = self.handles.stats.data.errors.load(Ordering::Relaxed);
        let s403s = self.handles.stats.data.status_403s.load(Ordering::Relaxed);
        let s429s = self.handles.stats.data.status_429s.load(Ordering::Relaxed);

        let threshold = self.handles.config.threads * 2;
        if errors >= threshold {
            // general errors should not exceed the given threshold
            return Some(PolicyTrigger::Errors);
        }

        let ratio_403s = s403s as f64 / requests as f64;
        if ratio_403s >= HIGH_ERROR_RATIO {
            // almost exclusively 403
            return Some(PolicyTrigger::Status403);
        }

        let ratio_429s = s429s as f64 / requests as f64;
        if ratio_429s >= HIGH_ERROR_RATIO / 3.0 {
            // high # of 429 responses
            return Some(PolicyTrigger::Status429);
        }

        None
    }

    /// enforce auto-tune policy
    fn tune(&self, _trigger: PolicyTrigger) {}

    /// enforce auto-bail policy
    async fn bail(&self, trigger: PolicyTrigger) -> Result<()> {
        let scans = self.handles.ferox_scans()?;

        let mut scan_tuples = vec![];

        {
            if let Ok(guard) = scans.scans.read() {
                for (i, scan) in guard.iter().enumerate() {
                    PROGRESS_PRINTER.println(format!(
                        "{} {}",
                        scan.is_active(),
                        scan.num_errors(trigger)
                    ));
                    if scan.is_active() && scan.num_errors(trigger) > 0 {
                        // only active scans that have at least 1 error

                        scan_tuples.push((i, scan.num_errors(trigger)));
                    }
                }
            }
        }

        if scan_tuples.len() == 0 {
            return Ok(());
        }

        // sort by number of errors
        scan_tuples.sort_unstable_by(|x, y| y.1.cmp(&x.1));

        for (idx, _errors) in scan_tuples {
            let scan = if let Ok(guard) = scans.scans.read() {
                guard.index(idx).clone()
            } else {
                // todo think about logging
                continue;
            };

            if scan.is_active() {
                scan.abort()
                    .await
                    .unwrap_or_else(|e| log::warn!("Could not bail on scan: {}", e));
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
            if self.rate_limiter.is_some() {
                // found a rate limiter, limit that junk!
                if let Err(e) = self.limit().await {
                    log::warn!("Could not rate limit scan: {}", e);
                    self.handles.stats.send(AddError(Other)).unwrap_or_default();
                }
            }

            let response = logged_request(&url, self.handles.clone()).await?;

            match self.policy {
                RequesterPolicy::AutoTune => {
                    if let Some(trigger) = self.should_enforce_policy() {
                        self.tune(trigger);
                    }
                }
                RequesterPolicy::AutoBail => {
                    if let Some(trigger) = self.should_enforce_policy() {
                        self.bail(trigger).await?; // todo may or may not be right to bubble up
                    }
                }
                RequesterPolicy::Default => {}
            }

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
            scans.increment_error(url);
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
            scans.increment_status_code(url, code);
        }
    }

    /// helper to stay DRY
    async fn increment_status_codes(handles: Arc<Handles>, num_codes: usize, code: StatusCode) {
        for _ in 0..num_codes {
            handles.stats.send(Command::AddStatus(code)).unwrap();
        }

        handles.stats.sync().await.unwrap();
    }

    /// helper to stay DRY
    fn get_requests(handles: Arc<Handles>) -> usize {
        handles.stats.data.requests.load(Ordering::Relaxed)
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
            rate_limiter: None,
            policy: Default::default(),
        };

        increment_errors(requester.handles.clone(), 49).await;
        // 49 errors is false because we haven't hit the min threshold
        assert_eq!(get_requests(requester.handles.clone()), 49);
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
            rate_limiter: None,
            policy: Default::default(),
        };

        increment_errors(requester.handles.clone(), 50).await;
        assert_eq!(requester.should_enforce_policy(), None);
        increment_errors(requester.handles.clone(), 50).await;
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
            rate_limiter: None,
            policy: Default::default(),
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
            rate_limiter: None,
            policy: Default::default(),
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
    /// bail should return call abort on the scan with the most errors
    async fn bail_calls_abort_on_highest_errored_feroxscan() {
        let url = "http://one";

        let (handles, _) = setup_requester_test(None).await;

        let scan_one = create_scan(handles.clone(), url, 10, PolicyTrigger::Errors).await;
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
            target_url: url.to_string(),
            rate_limiter: None,
            policy: Default::default(),
        };

        requester.bail(PolicyTrigger::Errors).await.unwrap();
        assert_eq!(scans.get_active_scans().len(), 3);
        assert!(scan_one.is_active());
        assert!(scan_three.is_active());
        assert!(scan_four.is_active());
        assert!(!scan_two.is_active());
    }
}
