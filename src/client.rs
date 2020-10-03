use crate::utils::status_colorizer;
use ansi_term::Color::Cyan;
use reqwest::header::HeaderMap;
use reqwest::{redirect::Policy, Client, Proxy};
use std::collections::HashMap;
use std::convert::TryInto;
use std::process::exit;
use std::time::Duration;

/// Create and return an instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
pub fn initialize(
    timeout: u64,
    useragent: &str,
    redirects: bool,
    insecure: bool,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
) -> Client {
    let policy = if redirects {
        Policy::limited(10)
    } else {
        Policy::none()
    };

    let header_map: HeaderMap = match headers.try_into() {
        Ok(map) => map,
        Err(e) => {
            eprintln!(
                "{} {} {}",
                status_colorizer("ERROR"),
                Cyan.paint("Client::initialize"),
                e
            );
            exit(1);
        }
    };

    let client = Client::builder()
        .timeout(Duration::new(timeout, 0))
        .user_agent(useragent)
        .danger_accept_invalid_certs(insecure)
        .default_headers(header_map)
        .redirect(policy);

    let client = if proxy.is_some() && !proxy.unwrap().is_empty() {
        match Proxy::all(proxy.unwrap()) {
            Ok(proxy_obj) => client.proxy(proxy_obj),
            Err(e) => {
                eprintln!(
                    "{} {} Could not add proxy ({:?}) to Client configuration",
                    status_colorizer("ERROR"),
                    Cyan.paint("Client::initialize"),
                    proxy
                );
                eprintln!(
                    "{} {} {}",
                    status_colorizer("ERROR"),
                    Cyan.paint("Client::initialize"),
                    e
                );
                exit(1);
            }
        }
    } else {
        client
    };

    match client.build() {
        Ok(client) => client,
        Err(e) => {
            eprintln!(
                "{} {} Could not create a Client with the given configuration, exiting.",
                status_colorizer("ERROR"),
                Cyan.paint("Client::build")
            );
            eprintln!(
                "{} {} {}",
                status_colorizer("ERROR"),
                Cyan.paint("Client::build"),
                e
            );
            exit(1);
        }
    }
}
