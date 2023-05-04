use anyhow::Result;
use reqwest::header::HeaderMap;
use reqwest::{redirect::Policy, Client, Proxy};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;
use std::time::Duration;

/// Create and return an instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
/// For now, silence clippy for this one
#[allow(clippy::too_many_arguments)]
pub fn initialize(
    timeout: u64,
    user_agent: &str,
    redirects: bool,
    insecure: bool,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    server_cert: Option<&str>,
    client_cert: Option<&str>,
    client_key: Option<&str>,
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
            // just add the proxy to the client
            // don't build and return it just yet
            client = client.proxy(proxy_obj);
        }
    }

    if let Some(cert_path) = server_cert {
        let cert_path = Path::new(cert_path);

        // if the root certificate path is not empty, open it
        // and read it into a buffer

        let buf = std::fs::read(cert_path)?;
        let cert = match cert_path
            .extension()
            .map(|s| s.to_str().unwrap_or_default())
        {
            // depending upon the extension of the file, create a
            // certificate object from it using either the "pem" or "der" parser
            Some("pem") => reqwest::Certificate::from_pem(&buf)?,
            Some("der") => reqwest::Certificate::from_der(&buf)?,

            // if we cannot determine the extension, do nothing
            _ => {
                log::warn!(
                    "unable to determine extension: assuming PEM format for root certificate"
                );
                reqwest::Certificate::from_pem(&buf)?
            }
        };

        // in either case, add the root certificate to the client
        client = client.add_root_certificate(cert);
    }

    if let (Some(cert_path), Some(key_path)) = (client_cert, client_key) {
        let cert_path = Path::new(cert_path);

        let cert = std::fs::read(cert_path)?;
        let key = std::fs::read(key_path)?;

        let identity = match cert_path
            .extension()
            .map(|s| s.to_str().unwrap_or_default())
        {
            Some("pem") => reqwest::Identity::from_pkcs8_pem(&cert, &key)?,
            _ => {
                log::warn!("unable to determine extension: assuming PEM format for client key and certificate");
                reqwest::Identity::from_pkcs8_pem(&cert, &key)?
            }
        };

        client = client.identity(identity);
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
            None,
            None,
        )
        .unwrap();
    }

    #[test]
    /// create client with a proxy, expect no error
    fn client_with_good_proxy() {
        let headers = HashMap::new();
        let proxy = "http://127.0.0.1:8080";
        initialize(
            0,
            "stuff",
            true,
            true,
            &headers,
            Some(proxy),
            None,
            None,
            None,
        )
        .unwrap();
    }
}
