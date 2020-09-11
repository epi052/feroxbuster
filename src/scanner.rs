use crate::config::CONFIGURATION;
use crate::FeroxResult;
use futures::{stream, StreamExt};
use futures::future::{FutureExt, BoxFuture};
use reqwest::{Client, Response, Url};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use std::sync::Arc;



pub struct FeroxScan {
    wordlist: Arc<HashSet<String>>,
}

impl FeroxScan {
    /// Creates an [Atomic Reference](https://doc.rust-lang.org/std/sync/struct.Arc.html) to
    /// a [FeroxScan](struct.FeroxScan.html) object that contains a reference to the given wordlist
    pub fn new(wordlist: Arc<HashSet<String>>) -> Arc<Self> {
        // went a little off the deep end here; to solve a lifetime issue when dealing with
        // the recursion handler, i ended up needing to wrap Self in an Arc...
        // https://www.reddit.com/r/rust/comments/csz49l/async_fn_painful_self_lifetime_imposition
        // this should allow for cheap clones of self as they're simply atomic references to
        // the underlying FeroxScan object
        let scanner = FeroxScan {
            wordlist,
        };

        Arc::new(scanner)
    }

    /// Simple helper to generate a `Url`
    ///
    /// Errors during parsing `url` or joining `word` are propagated up the call stack
    fn format_url(url: &str, word: &str, extension: Option<&str>) -> FeroxResult<Url> {
        let base_url = reqwest::Url::parse(&url)?;

        let word = if extension.is_some() {
            format!("{}.{}", word, extension.unwrap())
        } else {
            String::from(word)
        };

        match base_url.join(word.as_str()) {
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
                log::error!("Error while making request: {}", e);
                Err(Box::new(e))
            }
        }
    }

    async fn spawn_file_reporter(mut report_channel: Receiver<Response>) {
        log::info!("Writing scan results to {}", CONFIGURATION.output);


            let outfile = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&CONFIGURATION.output)
                .unwrap();

            let mut writer = BufWriter::new(outfile);

            while let Some(resp) = report_channel.recv().await {
                log::debug!("received {} on reporting channel", resp.url());
                let response_code = &resp.status();
                for code in CONFIGURATION.statuscodes.iter() {
                    if response_code == code {
                        let report = format!(
                            "[{}] - {} - [{} bytes]\n",
                            resp.status(),
                            resp.url(),
                            resp.content_length().unwrap_or(0)
                        );
                        println!("{:?}", report.as_bytes());
                        write!(writer, "{}", report).unwrap();
                        break;
                    }
                }
                log::debug!("Processed: {}", resp.url());
            }

    }

    /// Spawn a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives responses and prints them if they meet the given
    /// reporting criteria
    async fn spawn_terminal_reporter(mut report_channel: Receiver<Response>) {
        while let Some(resp) = report_channel.recv().await {
            log::debug!("received {} on reporting channel", resp.url());
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
    }

    /// Spawn a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives Urls and scans them
    fn spawn_recursion_handler(mut recursion_channel: Receiver<String>, wordlist: Arc<HashSet<String>>) -> BoxFuture<'static, Vec<JoinHandle<()>>> {
        async move {
            let mut scans = vec![];
            while let Some(resp) = recursion_channel.recv().await {
                log::info!("received {} on recursion channel", resp);
                let scanner = FeroxScan::new(wordlist.clone());
                scans.push(tokio::spawn(async move {scanner.scan_directory(&resp).await}));
            }
            scans
        }.boxed()
    }


    /// Creates a vector of formatted Urls
    ///
    /// At least one value will be returned (base_url + word)
    ///
    /// If any extensions were passed to the program, each extension will add a
    /// (base_url + word + ext) Url to the vector
    fn create_urls(self: Arc<Self>, target_url: &str, word: &str, extensions: &Vec<String>) -> Vec<Url> {
        let mut urls = vec![];

        match FeroxScan::format_url(&target_url, &word, None) {
            Ok(url) => {
                urls.push(url); // default request, i.e. no extension
            }
            Err(_) => {} // already logged in format_url
        }

        for ext in extensions.iter() {
            match FeroxScan::format_url(&target_url, &word, Some(ext)) {
                Ok(url) => {
                    urls.push(url); // any extensions passed in
                }
                Err(_) => {} // already logged in format_url
            }
        }

        urls
    }

    /// Helper function to determine suitability for recursion
    ///
    /// handles 2xx and 3xx responses by either checking if the url ends with a / (2xx)
    /// or if the Location header is present and matches the base url + / (3xx)
    fn response_is_directory(self: Arc<Self>, response: &Response) -> bool {
        log::trace!("is_directory({:?})", response);
        if response.status().is_redirection() {
            // status code is 3xx
            match response.headers().get("Location") {
                // and has a Location header
                Some(loc) => {
                    // get absolute redirect Url based on the already known base url
                    log::debug!("Location header: {:?}", loc);
                    if let Ok(loc_str) = loc.to_str() {
                        if let Ok(abs_url) = response.url().join(loc_str) {
                            if format!("{}/", response.url()) == abs_url.as_str() {
                                // if current response's Url + / == the absolute redirection
                                // location, we've found a directory suitable for recursion
                                log::debug!(
                                    "{} is directory suitable for recursion",
                                    response.url()
                                );
                                return true;
                            }
                        }
                    }
                }
                None => {
                    return false;
                }
            }
        } else if response.status().is_success() {
            // status code is 2xx, need to check if it ends in / and isnt the base url?
            if response.url().as_str().ends_with("/") {
                log::debug!("{} is directory suitable for recursion", response.url());
                return true;
            }
        }

        false
    }

    async fn try_recursion(self: Arc<Self>, response: &Response, mut transmitter: Sender<String>) {
        let arc_self = self.clone();
        log::info!("called try_recursion");
        if self.response_is_directory(&response) {
            if CONFIGURATION.redirects {
                // response is 2xx can simply send it because we're
                // following redirects
                log::info!("Added new directory to recursive scan: {}", response.url());
                log::debug!("New directory sent across directory channel");
                // tokio::spawn(async move {
                //     arc_self.scan_directory(&response.url().as_str()).await;
                // });
                transmitter.send(String::from(response.url().as_str())).await.unwrap();
            } else {
                // response is 3xx, need to add a /
                let new_url = format!("{}/", response.url());

                log::debug!("{:#?}", response);
                log::debug!("Added / to {}, making {}", response.url(), new_url);
                log::info!("Added new directory to recursive scan: {}", new_url);
                transmitter.send(new_url).await.unwrap();
                // tokio::spawn(async move {
                //     arc_self.scan_directory(&new_url).await;
                // });
            }
        }
    }

    /// TODO: documentation
    pub async fn scan_directory(self: Arc<Self>, target_url: &str) {
        log::debug!("Starting scan against: {}", target_url);
        let (tx_rpt, rx_rpt): (Sender<Response>, Receiver<Response>) =
            mpsc::channel(CONFIGURATION.threads);

        let (tx_dir, rx_dir): (Sender<String>, Receiver<String>) =
            mpsc::channel(CONFIGURATION.threads);

        let self_ptr = self.clone();
        let self_task_ptr = self_ptr.clone();

        let reporter = if !CONFIGURATION.output.is_empty() {
            // output file defined
            tokio::spawn(async move {FeroxScan::spawn_file_reporter(rx_rpt).await})
        } else {
            tokio::spawn(async move {FeroxScan::spawn_terminal_reporter(rx_rpt).await})
        };

        let recurser = tokio::spawn(async move {FeroxScan::spawn_recursion_handler(rx_dir, self_task_ptr.wordlist.clone()).await});

        // producer tasks (mp of mpsc); responsible for making requests
        let producers = stream::iter(self.wordlist.iter())
            .map(|word| {
                // closure to pass the word through to for_each_concurrent along with a
                // cloned Sender for message passing
                let report_sender = tx_rpt.clone();
                let atomic_ref_self = self_ptr.clone();
                (word, report_sender, atomic_ref_self, tx_dir.clone())
            })
            .for_each_concurrent(
                // where the magic happens
                CONFIGURATION.threads, // concurrency limit (i.e. # of buffered requests)
                |(word, mut report_chan, arc_self, dir_chan)| async move {
                    // closure to make the request and send it over the channel to be
                    // reported (or not) to the user

                    let urls = arc_self.clone().create_urls(&target_url, &word, &CONFIGURATION.extensions);

                    for url in urls {
                        match FeroxScan::make_request(&CONFIGURATION.client, url).await {
                            // response came back without error
                            Ok(response) => {
                                // do recursion if appropriate
                                if !CONFIGURATION.norecursion {
                                    if arc_self.clone().response_is_directory(&response) {
                                        arc_self.clone().try_recursion(&response, dir_chan.clone()).await;
                                    }
                                }

                                //     //     if CONFIGURATION.redirects {
                                //     //         async move {
                                //     //             arc_self.scan_directory(&response.url().as_str()).await;
                                //     //         }
                                //     //     } else {
                                //     //
                                //     //         async move {
                                //     //             arc_self.scan_directory(&response.url().as_str()).await;
                                //     //         }
                                //     //     }
                                //     // }
                                //
                                //
                                //     log::debug!("Calling try_recursion with: {}", response.url());
                                //     match arc_self.clone().try_recursion(&response) {
                                //         Some(scan_future) => {
                                //             scan_future.await;
                                //         }
                                //         None => {}
                                //     }
                                // }

                                match report_chan.send(response).await {
                                    Ok(_) => {
                                        log::debug!("sent {}/{} over reporting channel", &target_url, &word);
                                    }
                                    Err(e) => {
                                        log::error!("wtf: {}", e);
                                    }
                                }
                            }
                            Err(_) => {} // already logged in make_request; no add'l action req'd
                        }
                    }
                },
            );

        // await tx tasks
        producers.await;

        // manually drop tx in order for the rx task's while loops to eval to false

        // await rx tasks


        drop(tx_dir);
        futures::future::join_all(recurser.await.unwrap()).await;
        drop(tx_rpt);
        reporter.await;

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_url_normal() {
        assert_eq!(
            format_url("http://localhost", "stuff", None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_no_word() {
        assert_eq!(
            format_url("http://localhost", "", None).unwrap(),
            reqwest::Url::parse("http://localhost").unwrap()
        );
    }

    #[test]
    #[should_panic]
    fn test_format_url_no_url() {
        format_url("", "stuff", None).unwrap();
    }

    #[test]
    fn test_format_url_word_with_preslash() {
        assert_eq!(
            format_url("http://localhost", "/stuff", None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_word_with_postslash() {
        assert_eq!(
            format_url("http://localhost", "stuff/", None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff/").unwrap()
        );
    }
}
