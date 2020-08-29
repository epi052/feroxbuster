use feroxbuster::logger::init_logger;
use feroxbuster::FeroxResult;
mod config;
use crate::config::{Configuration, DEFAULT_RESPONSE_CODES, CONFIGURATION};
use futures::stream;
use futures::StreamExt;
use log;
use reqwest::{Response, Url, Client};
use std::fs::File;
use std::io::{BufRead, BufReader, Lines};
use tokio::task;
use std::collections::HashSet;
use std::env;


/// Simple helper to generate a `reqwest::Url`
///
/// If an error occurs during parsing `url` or joining `word`, None is returned
pub fn format_url(word: &str, url: &str) -> Option<Url> {
    let base_url = match reqwest::Url::parse(&url) {
        Ok(url) => url,
        Err(e) => {
            log::warn!("Could not convert {} into a URL: {}", url, e);
            return None;
        }
    };

    if let Ok(req) = base_url.join(word) {
        log::debug!("Requested URL: {}", req);
        Some(req)
    }
    else {
        log::warn!("Could not join {} with {}", word, base_url);
        None
    }
}

async fn make_request(client: &Client, url: Url) -> Response {
    // todo: remove unwrap
    client.get(url).send().await.unwrap()
}

fn process_wordlist(config: &Configuration) -> Lines<BufReader<File>> {
    // todo: remove unwrap
    let file = File::open(&config.wordlist).unwrap();
    let reader = BufReader::new(file);

    let words = reader.lines();

    words
}

async fn bust_dirs(urls: &HashSet<Url>, client: &'static Client, threads: usize) -> FeroxResult<()> {
    let mut buffered_futures = stream::iter(urls.to_owned())
        .map(move |url| {
            let future = task::spawn(async move { make_request(&client, url).await });
            future

        })
        .buffer_unordered(threads);

        log::debug!("{:#?}", buffered_futures);

        while let Some(item) = buffered_futures.next().await {
            match item {
                Ok(response) => {
                    let response_code = &response.status();
                    for code in DEFAULT_RESPONSE_CODES.iter() {
                        if response_code == code {
                            println!("[{}] - {}", response.status(), response.url());
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
    let words = task::spawn_blocking(move || process_wordlist(&CONFIGURATION)).await?;

    // urls is a Set of urls, invalid UTF-8 words result in the base url being accumulated
    // as this is a set, the base url will only be in the set once, even if there are multiple
    // invalid UTF-8 results from the wordlist
    let base_url = CONFIGURATION.target_url.to_owned();

    let urls: HashSet<Url> = task::spawn_blocking(move || {
        words.map(move |word| {
            match word {
                Ok(w) => {
                    // todo: remove unwrap here
                    format_url(&w, &base_url).unwrap()
                }
                Err(e) => {
                    log::warn!("get_urls: {}", e);
                    // todo: remove unwrap here
                    format_url(&"", &base_url).unwrap()
                }
            }
        })
    })
    .await?.collect();

    log::debug!("{:#?}", urls);

    bust_dirs(&urls, &CONFIGURATION.client, CONFIGURATION.threads).await?;
    Ok(())
}

fn main() {
    // use occurrences of -v on commandline to or verbosity = N in feroxconfig.toml to set
    // log level for the application; respects already specified RUST_LOG environment variable
    match CONFIGURATION.verbosity {
        0 => (),
        1 => env::set_var("RUST_LOG", "warn"),
        2 => env::set_var("RUST_LOG", "info"),
        _ => env::set_var("RUST_LOG", "debug"),
    }

    init_logger();

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
        assert_eq!(format_url("stuff", "http://localhost").unwrap(), reqwest::Url::parse("http://localhost/stuff").unwrap());
    }

    #[test]
    fn test_format_url_no_word() {
        assert_eq!(format_url("", "http://localhost").unwrap(), reqwest::Url::parse("http://localhost").unwrap());
    }

    #[test]
    #[should_panic]
    fn test_format_url_no_url() {
        format_url("stuff", "").unwrap();
    }

    #[test]
    fn test_format_url_word_with_preslash() {
        assert_eq!(format_url("/stuff", "http://localhost").unwrap(), reqwest::Url::parse("http://localhost/stuff").unwrap());
    }

    #[test]
    fn test_format_url_word_with_postslash() {
        assert_eq!(format_url("stuff/", "http://localhost").unwrap(), reqwest::Url::parse("http://localhost/stuff/").unwrap());
    }
}