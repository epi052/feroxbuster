use crate::utils::{module_colorizer, status_colorizer};
use reqwest::header::HeaderMap;
use reqwest::{redirect::Policy, Client, Proxy};
use std::collections::HashMap;
use std::convert::TryInto;
#[cfg(not(test))]
use std::process::exit;
use std::time::Duration;

/// Create and return an instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
pub fn initialize(
    timeout: u64,
    user_agent: &str,
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

    // try_into returns infallible as its error, unwrap is safe here
    let header_map: HeaderMap = headers.try_into().unwrap();

    let client = Client::builder()
        .timeout(Duration::new(timeout, 0))
        .user_agent(user_agent)
        .danger_accept_invalid_certs(insecure)
        .default_headers(header_map)
        .redirect(policy);

    let client = match proxy {
        // a proxy is specified, need to add it to the client
        Some(some_proxy) => {
            if !some_proxy.is_empty() {
                // it's not an empty string
                match Proxy::all(some_proxy) {
                    Ok(proxy_obj) => client.proxy(proxy_obj),
                    Err(e) => {
                        eprintln!(
                            "{} {} {}",
                            status_colorizer("ERROR"),
                            module_colorizer("Client::initialize"),
                            e
                        );

                        #[cfg(test)]
                        panic!();
                        #[cfg(not(test))]
                        exit(1);
                    }
                }
            } else {
                client // Some("") was used?
            }
        }
        // no proxy specified
        None => client,
    };

    match client.build() {
        Ok(client) => client,
        Err(e) => {
            eprintln!(
                "{} {} {}",
                status_colorizer("ERROR"),
                module_colorizer("Client::build"),
                e
            );

            #[cfg(test)]
            panic!();
            #[cfg(not(test))]
            exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    /// create client with a bad proxy, expect panic
    fn client_with_bad_proxy() {
        let headers = HashMap::new();
        initialize(0, "stuff", true, false, &headers, Some("not a valid proxy"));
    }

    #[test]
    /// create client with a proxy, expect no error
    fn client_with_good_proxy() {
        let headers = HashMap::new();
        let proxy = "http://127.0.0.1:8080";
        initialize(0, "stuff", true, true, &headers, Some(proxy));
    }
}
