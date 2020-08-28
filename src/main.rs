#![allow(unused_imports)]
use feroxbuster::logger::init_logger;
use feroxbuster::FeroxResult;
mod config;
use crate::config::{Configuration, DEFAULT_RESPONSE_CODES, CONFIGURATION};
use futures::stream;
use futures::StreamExt;
use log::*;
use reqwest::{Response, Url, Client};
use std::fs::File;
use std::io::{BufRead, BufReader, Lines};
use tokio::task;
use std::iter::Map;
use std::collections::HashSet;

/// Simple helper to generate a `reqwest::Url`
///
/// the function relies on the application's `config::CONFIGURATION` struct
/// in order to know the base target URL
pub fn format_url(word: &str, url: &str) -> Url {
    // TODO: formatted variable should be smarter about combining url and word
    let formatted = format!("{}/{}", &url, word);
    let req = reqwest::Url::parse(&formatted).unwrap();
    debug!("Requested URL: {}", req);
    req
}

async fn make_request(client: &Client, url: Url) -> Response {
    client.get(url).send().await.unwrap()
}
//
fn process_wordlist(config: &Configuration) -> Lines<BufReader<File>> {
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
                    format_url(&w, &base_url)
                }
                Err(e) => {
                    warn!("get_urls: {}", e);
                    format_url(&"", &base_url)
                }
            }
        })
    })
    .await?.collect();

    bust_dirs(&urls, &CONFIGURATION.client, CONFIGURATION.threads).await?;
    Ok(())
}

fn main() {
    init_logger();

    info!("The configuration is {:#?}", *CONFIGURATION);

    //
    //
    // info!("{:?}", words);
    //
    //
    //
    // let mut buffered_futures = stream::iter(urls)
    //     .map(move |url| {
    //         let future = task::spawn(async move { make_request(url).await });
    //         future
    //     })
    //     .buffer_unordered(CONFIGURATION.threads);
    //
    // while let Some(item) = buffered_futures.next().await {
    //     match item {
    //         Ok(response) => {
    //             let response_code = &response.status();
    //             for code in DEFAULT_RESPONSE_CODES.iter() {
    //                 if response_code == code {
    //                     println!("[{}] - {}", response.status(), response.url());
    //                     break;
    //                 }
    //             }
    //         }
    //         Err(e) => {
    //             println!("Err: {}", e);
    //         }
    //     }
    // }
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    match rt.block_on(app()) {
        Ok(_) => info!("Done"),
        Err(e) => error!("An error occurred: {}", e),
    };
}
