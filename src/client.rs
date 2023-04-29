use anyhow::Result;
use reqwest::header::HeaderMap;
use reqwest::{redirect::Policy, Client, Proxy};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// Create and return an instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
pub fn initialize(
    timeout: u64,
    user_agent: &str,
    redirects: bool,
    insecure: bool,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    certificate: Option<&str>,
) -> Result<Client> {
    let policy = if redirects {
        Policy::limited(10)
    } else {
        Policy::none()
    };

    let header_map: HeaderMap = headers.try_into()?;

    let mut client = Client::builder()
        .timeout(Duration::new(timeout, 0))
        .user_agent(user_agent)
        .danger_accept_invalid_certs(insecure)
        .default_headers(header_map)
        .redirect(policy)
        .http1_title_case_headers();

    if let Some(some_proxy) = proxy {
        if !some_proxy.is_empty() {
            // it's not an empty string; set the proxy
            let proxy_obj = Proxy::all(some_proxy)?;
            client = client.proxy(proxy_obj);
        }
    }

    if let Some(cert_path) = certificate {
        let cert_path = Path::new(cert_path);
        let mut buf = Vec::new();
        File::open(cert_path)?.read_to_end(&mut buf)?;
        if let Some(extension) = cert_path.extension() {
            match extension.to_str() {
                Some("pem") => {
                    let cert = reqwest::Certificate::from_pem(&buf)?;
                    client = client.add_root_certificate(cert);
                }
                Some("der") => {
                    let cert = reqwest::Certificate::from_der(&buf)?;
                    client = client.add_root_certificate(cert);
                }
                _ => {}
            }
        }
    }

    Ok(client.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    /// create client with a bad proxy, expect panic
    fn client_with_bad_proxy() {
        let headers = HashMap::new();
        initialize(
            0,
            "stuff",
            true,
            false,
            &headers,
            Some("not a valid proxy"),
            None,
        )
        .unwrap();
    }

    #[test]
    /// create client with a proxy, expect no error
    fn client_with_good_proxy() {
        let headers = HashMap::new();
        let proxy = "http://127.0.0.1:8080";
        initialize(0, "stuff", true, true, &headers, Some(proxy), None).unwrap();
    }
}
