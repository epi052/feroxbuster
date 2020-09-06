use crate::config::{CONFIGURATION};
use tokio::sync::mpsc::{self, Receiver, Sender};
use std::collections::HashSet;
use futures::{stream, StreamExt};
use reqwest::{Client, Response, Url};
use crate::FeroxResult;

pub struct FeroxScan<'scan> {
    wordlist: &'scan HashSet<String>,
    report_channel: Sender<Response>,
    directory_channel: Sender<Response>
}

impl<'scan> FeroxScan<'scan> {
    /// DOCUMENTATION GOES HERE
    pub fn new(wordlist: &'scan HashSet<String>) -> Self {
        // mpsc for request making/response reporting
        let (tx_report, rx_report): (Sender<Response>, Receiver<Response>) =
            mpsc::channel(CONFIGURATION.threads);

        // mpsc for kicking off a scan of a new directory
        let (tx_new_directory, rx_new_directory): (Sender<Response>, Receiver<Response>) =
            mpsc::channel(CONFIGURATION.threads);

        Self::spawn_reporter(rx_report);

        FeroxScan {
            wordlist,
            report_channel: tx_report,
            directory_channel: tx_new_directory
        }

    }

    /// Simple helper to generate a `Url`
    ///
    /// Errors during parsing `url` or joining `word` are propagated up the call stack
    fn format_url(word: &str, url: &str) -> FeroxResult<Url> {
        let base_url = reqwest::Url::parse(&url)?;

        match base_url.join(word) {
            Ok(request) => {
                log::debug!("Requested URL: {}", request);
                Ok(request)
            }
            Err(e) => {
                log::warn!("Could not join {} with {}", word, base_url);
                Err(Box::new(e))
            }
        }
    }

    /// Initiate request to the given `Url` using the pre-configured `Client`
    async fn make_request(client: &Client, url: Url) -> FeroxResult<Response> {
        match client.get(url).send().await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                log::error!("make_request: {}", e);
                Err(Box::new(e))
            }
        }
    }

    /// Spawn a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives responses and prints them if they meet the given
    /// reporting criteria
    fn spawn_reporter(mut report_channel: Receiver<Response>) {
        tokio::spawn(async move {
            while let Some(resp) = report_channel.recv().await {
                let response_code = &resp.status();
                for code in CONFIGURATION.statuscodes.iter() {
                    if response_code == code {
                        println!(
                            "[{}] - {} - [{} bytes]",
                            resp.status(),
                            resp.url(),
                            resp.content_length().unwrap_or(0)
                        );
                        break;
                    }
                }
            }
        });
    }

    pub async fn scan_directory(&self, target_url: &str) {
        // producer tasks (mp of mpsc); responsible for making requests
        let producers = stream::iter(self.wordlist)
            .map(|word| {
                // closure to pass the word through to for_each_concurrent along with a
                // cloned Sender for message passing
                let report_sender = self.report_channel.clone();
                let directory_sender = self.directory_channel.clone();
                (word, report_sender, directory_sender)
            })
            .for_each_concurrent(
                // where the magic happens
                CONFIGURATION.threads, // concurrency limit (i.e. # of buffered requests)
                |(word, mut report_chan, mut directory_chan)| async move {
                    // closure to make the request and send it over the channel to be
                    // reported (or not) to the user
                    match FeroxScan::format_url(&word, &target_url) {
                        Ok(url) => {
                            // url is good to go
                            match FeroxScan::make_request(&CONFIGURATION.client, url).await {
                                // response came back without error
                                Ok(response) => {
                                    report_chan.send(response).await.unwrap();
                                    // is directory? send over the dir channel
                                }
                                Err(_) => {} // already logged in make_request; no add'l action req'd
                            }
                        }
                        Err(_) => {} // already logged in format_url
                    }
                },
            );
        producers.await;
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_url_normal() {
        assert_eq!(
            format_url("stuff", "http://localhost").unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_no_word() {
        assert_eq!(
            format_url("", "http://localhost").unwrap(),
            reqwest::Url::parse("http://localhost").unwrap()
        );
    }

    #[test]
    #[should_panic]
    fn test_format_url_no_url() {
        format_url("stuff", "").unwrap();
    }

    #[test]
    fn test_format_url_word_with_preslash() {
        assert_eq!(
            format_url("/stuff", "http://localhost").unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_word_with_postslash() {
        assert_eq!(
            format_url("stuff/", "http://localhost").unwrap(),
            reqwest::Url::parse("http://localhost/stuff/").unwrap()
        );
    }
}