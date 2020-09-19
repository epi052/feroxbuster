use uuid::Uuid;
use crate::scanner::{make_request, format_url};
use crate::config::CONFIGURATION;
use crate::utils::status_colorizer;
use std::process;
use reqwest::Response;


const UUID_LENGTH: u64 = 32;


/// todo document
pub async fn initialize(target_urls: &[String]) {
    log::trace!("enter: initialize({:?})", target_urls);

    let target_urls = connectivity_test(&target_urls).await;
    smart_scan(&target_urls).await;

    log::trace!("exit: initialize");
}

/// Simple helper to return a uuid, formatted as lowercase without hyphens
fn unique_string(length: usize) -> String {
    log::trace!("enter: unique_string({})", length);
    let mut ids = vec![];

    for _ in 0..length {
        ids.push(Uuid::new_v4().to_simple().to_string());
    }

    let unique_id = ids.join("");

    log::trace!("exit: unique_string -> {}", unique_id);
    unique_id
}

/// todo document
pub async fn smart_scan(target_urls: &[String]) {
    log::trace!("enter: smart_scan({:?})", target_urls);

    for target_url in target_urls {

        if let Some(resp_one) = wildcard_request(&target_url, 1).await {

            let wc_length = resp_one.content_length().unwrap_or(0);

            if wc_length == 0 {
                continue;
            }
            // content length of wildcard is non-zero

            if let Some(resp_two) = wildcard_request(&target_url, 3).await {
                // make a second request, with a known-sized longer request
                let wc2_length = resp_one.content_length().unwrap_or(0);
                if wc2_length == wc_length + (UUID_LENGTH * 2) {
                    // second length is what we'd expect to see if the requested url is
                    // reflected in the response along with some static content; aka custom 404
                    println!("[{}] - Url is being reflected in wildcard response", status_colorizer("WILDCARD"));
                } else if wc_length == wc2_length {
                    println!("[{}] - Wildcard response is a static size; consider filtering by adding -S {} to your command", status_colorizer("WILDCARD"), wc_length);
                }
            }
        }

    }

    log::trace!("exit: smart_scan");
}

/// todo doc
async fn wildcard_request(target_url: &str, length: usize) -> Option<Response> {
    // todo trace
    let unique_str = unique_string(length);

    let nonexistent = match format_url(target_url, &unique_str, CONFIGURATION.addslash, None) {
        Ok(url) => url,
        Err(e) => {
            log::error!("{}", e);
            return None;
        }
    };

    let wildcard = status_colorizer("WILDCARD");

    match make_request(&CONFIGURATION.client, nonexistent.to_owned()).await {
        Ok(response) => {
            if CONFIGURATION.statuscodes.contains(&response.status().as_u16()) {
                // found a wildcard response
                println!("[{}] - Received [{}] for {} ({} bytes)", wildcard, status_colorizer(&response.status().to_string()), response.url(), response.content_length().unwrap_or(0));

                if response.status().is_redirection() {
                    // show where it goes, if possible
                    if let Some(next_loc) = response.headers().get("Location") {
                        if let Ok(next_loc_str) = next_loc.to_str() {
                            println!("[{}] {} redirects to => {}", wildcard, response.url(), next_loc_str);
                        } else {
                            println!("[{}] {} redirects to => {:?}", wildcard, response.url(), next_loc);
                        }
                    }
                }
                return Some(response);
            }
        },
        Err(e) => {
            log::warn!("{}", e);
            return None;
        }
    }
    None
}

/// todo document
async fn connectivity_test(target_urls: &[String]) -> Vec<String> {
    log::trace!("enter: connectivity_test({:?})", target_urls);

    let mut good_urls = vec![];

    for target_url in target_urls {
        let request = match format_url(target_url, "", CONFIGURATION.addslash, None) {
            Ok(url) => url,
            Err(e) => {
                log::error!("{}", e);
                continue;
            }
        };

        match make_request(&CONFIGURATION.client, request).await {
            Ok(_) => {
                good_urls.push(target_url.to_owned());
            },
            Err(e) => {
                println!("Could not connect to {}, skipping...", target_url);
                log::error!("{}", e);
            }
        }
    }

    if good_urls.is_empty() {
        log::error!("Could not connect to any target provided, exiting.");
        log::trace!("exit: connectivity_test");
        process::exit(1);
    }

    log::trace!("exit: connectivity_test -> {:?}", good_urls);

    good_urls
}
//
// async fn connectivity_test(target_urls: &[String]) {
//
//
//
//
// }