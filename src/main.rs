use feroxbuster::logger;
use feroxbuster::FeroxResult;
mod config;
use crate::config::{Configuration, CONFIGURATION, DEFAULT_RESPONSE_CODES};
use futures::stream;
use futures::StreamExt;
use reqwest::{Client, Response, Url};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use tokio::task;

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

async fn make_request(client: &Client, url: Url) -> Response {
    // todo: remove unwrap
    client.get(url).send().await.unwrap()
}

/// Creates a Set of Strings from the given wordlist
fn get_unique_words_from_wordlist(config: &Configuration) -> FeroxResult<HashSet<String>> {
    //todo: stupid function name
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

async fn bust_dir(
    base_url: &'static str,
    words: &HashSet<String>,
    client: &'static Client,
    threads: usize,
) -> FeroxResult<()> {
    let mut buffered_futures = stream::iter(words.to_owned())
        .map(move |directory| {
            // todo: can i remove the unwrap? map_err or something?
            let url = format_url(&directory, &base_url).unwrap();
            task::spawn(async move { make_request(&client, url).await })
        })
        .buffer_unordered(threads);

    log::debug!("{:?}", buffered_futures);

    while let Some(item) = buffered_futures.next().await {
        match item {
            Ok(response) => {
                let response_code = &response.status();
                for code in DEFAULT_RESPONSE_CODES.iter() {
                    if response_code == code {
                        println!(
                            "[{}] - {} - [{} bytes]",
                            response.status(),
                            response.url(),
                            response.content_length().unwrap_or(0)
                        );
                        break;
                    }
                }
            }
            Err(e) => {
                println!("Err: {}", e);
            }
        }
    }

    Ok(())
}

async fn app() -> FeroxResult<()> {
    let words = task::spawn(async move { get_unique_words_from_wordlist(&CONFIGURATION) }).await??;

    bust_dir(
        &CONFIGURATION.target_url,
        &words,
        &CONFIGURATION.client,
        CONFIGURATION.threads,
    )
    .await?;

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
