use feroxbuster::config::{Configuration, CONFIGURATION};
use feroxbuster::{logger, FeroxResult};
use futures::{stream, StreamExt};
use reqwest::{Client, Response, Url};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::io;
use tokio_util::codec::{FramedRead, LinesCodec};


/// Simple helper to generate a `reqwest::Url`
///
/// Errors during parsing `url` or joining `word` are propagated up the call stack
pub fn format_url(word: &str, url: &str) -> FeroxResult<Url> {
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

async fn make_request(client: &Client, url: Url) -> FeroxResult<Response> {
    match client.get(url).send().await {
        Ok(resp) => Ok(resp),
        Err(e) => {
            log::error!("make_request: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Create a Set of Strings from the given wordlist
fn get_unique_words_from_wordlist(config: &Configuration) -> FeroxResult<HashSet<String>> {
    let file = match File::open(&config.wordlist) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Could not open wordlist: {}", e);
            return Err(Box::new(e));
        }
    };

    let reader = BufReader::new(file);

    let mut words = HashSet::new();

    for line in reader.lines() {
        match line {
            Ok(word) => {
                words.insert(word);
            }
            Err(e) => {
                log::warn!("Could not parse current line from wordlist : {}", e);
            }
        }
    }

    Ok(words)
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

async fn app() -> FeroxResult<()> {
    let words =
        tokio::spawn(async move { get_unique_words_from_wordlist(&CONFIGURATION) }).await??;

    // mpsc for request making/response reporting
    let (tx_report, rx_report): (Sender<Response>, Receiver<Response>) =
        mpsc::channel(CONFIGURATION.threads);

    // mpsc for kicking off a scan of a new directory
    let (tx_new_directory, rx_new_directory): (Sender<Response>, Receiver<Response>) =
        mpsc::channel(CONFIGURATION.threads);

    spawn_reporter(rx_report);

    if CONFIGURATION.stdin {
        let stdin = io::stdin();  // tokio's stdin, not std
        let mut reader = FramedRead::new(stdin, LinesCodec::new());

        while let Some(item) = reader.next().await {
            match item {
                Ok(line) => {
                    println!("FOUND: {}", line);
                    // call bust_dir or w/e here
                }
                Err(e) => {
                    println!("FOUND: ERROR: {}", e);
                }
            }
        }
    } else {
        // producer tasks (mp of mpsc); responsible for making requests
        let producers = stream::iter(words)
            .map(|word| {
                // closure to pass the word through to for_each_concurrent along with a
                // cloned Sender for message passing
                let report_sender = tx_report.clone();
                let directory_sender = tx_new_directory.clone();
                (word, report_sender, directory_sender)
            })
            .for_each_concurrent(
                // where the magic happens
                CONFIGURATION.threads, // concurrency limit (i.e. # of buffered requests)
                |(word, mut report_chan, mut directory_chan)| async move {
                    // closure to make the request and send it over the channel to be
                    // reported (or not) to the user
                    match format_url(&word, &CONFIGURATION.target_url) {
                        Ok(url) => {
                            // url is good to go
                            match make_request(&CONFIGURATION.client, url).await {
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

    Ok(())
}

fn main() {
    logger::initialize(CONFIGURATION.verbosity);

    log::debug!("{:#?}", *CONFIGURATION);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    match rt.block_on(app()) {
        Ok(_) => log::info!("Done"),
        Err(e) => log::error!("An error occurred: {}", e),
    };
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
