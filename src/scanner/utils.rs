use super::FeroxScanner;
use crate::{
    event_handlers::{
        Command::{self, AddError},
        Handles,
    },
    extractor::{ExtractionTarget::ResponseBody, ExtractorBuilder},
    response::FeroxResponse,
    statistics::StatError::Other,
    url::FeroxUrl,
    utils::make_request,
};
use anyhow::Result;
use leaky_bucket::LeakyBucket;
use std::{cmp::max, sync::Arc};
use tokio::{sync::oneshot, time::Duration};

/// represents actions the Requester should take in certain situations
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum RequesterPolicy {
    /// automatically try to lower request rate in order to reduce errors
    AutoTune,

    /// automatically bail at certain error thresholds
    AutoBail,

    /// just let that junk run super natural
    Default,
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

        // let policy = scanner.handles.config.config.policy; todo

        Ok(Self {
            policy: RequesterPolicy::Default, // todo replace with dynamic from config
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

            let response = make_request(
                &self.handles.config.client,
                &url,
                self.handles.config.output_level,
                self.handles.stats.tx.clone(),
            )
            .await?;

            // todo this is where bail should go, tune can probably just set a limiter if one isn't
            // already present

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
