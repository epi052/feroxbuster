use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use reqwest::{redirect::Policy, Client, Proxy};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;
use std::time::Duration;

/// Create and return an instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
/// For now, silence clippy for this one
#[allow(clippy::too_many_arguments)]
pub fn initialize<I>(
    timeout: u64,
    user_agent: &str,
    redirects: bool,
    insecure: bool,
    headers: &HashMap<String, String>,
    proxy: Option<&str>,
    server_certs: I,
    client_cert: Option<&str>,
    client_key: Option<&str>,
) -> Result<Client>
where
    I: IntoIterator,
    I::Item: AsRef<Path> + std::fmt::Debug,
{
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

    for cert_path in server_certs {
        let buf = std::fs::read(&cert_path)?;

        let cert = match reqwest::Certificate::from_pem(&buf) {
            Ok(cert) => cert,
            Err(err) => reqwest::Certificate::from_der(&buf).with_context(|| {
                format!(
                    "{:?} does not contain a valid PEM or DER certificate\n{}",
                    &cert_path, err
                )
            })?,
        };

        client = client.add_root_certificate(cert);
    }

    if let (Some(cert_path), Some(key_path)) = (client_cert, client_key) {
        if !cert_path.is_empty() && !key_path.is_empty() {
            let cert = std::fs::read(cert_path)?;
            let key = std::fs::read(key_path)?;

            let identity = reqwest::Identity::from_pkcs8_pem(&cert, &key).with_context(|| {
                format!(
                    "either {} or {} are invalid; expecting PEM encoded certificate and key",
                    cert_path, key_path
                )
            })?;

            client = client.identity(identity);
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
            Vec::<String>::new(),
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
            Vec::<String>::new(),
            None,
            None,
        )
        .unwrap();
    }

    #[test]
    /// create client with a server cert in pem format, expect no error
    fn client_with_valid_server_pem() {
        let headers = HashMap::new();

        initialize(
            0,
            "stuff",
            true,
            true,
            &headers,
            None,
            vec!["tests/mutual-auth/certs/server/server.crt.1".to_string()],
            None,
            None,
        )
        .unwrap();
    }

    #[test]
    /// create client with a server cert in der format, expect no error
    fn client_with_valid_server_der() {
        let headers = HashMap::new();

        initialize(
            0,
            "stuff",
            true,
            true,
            &headers,
            None,
            vec!["tests/mutual-auth/certs/server/server.der".to_string()],
            None,
            None,
        )
        .unwrap();
    }

    #[test]
    /// create client with two server certs (pem and der), expect no error
    fn client_with_valid_server_pem_and_der() {
        let headers = HashMap::new();

        println!("{}", std::env::current_dir().unwrap().display());

        initialize(
            0,
            "stuff",
            true,
            true,
            &headers,
            None,
            vec![
                "tests/mutual-auth/certs/server/server.crt.1".to_string(),
                "tests/mutual-auth/certs/server/server.der".to_string(),
            ],
            None,
            None,
        )
        .unwrap();
    }

    /// create client with invalid certificate, expect panic
    #[test]
    #[should_panic]
    fn client_with_invalid_server_cert() {
        let headers = HashMap::new();

        initialize(
            0,
            "stuff",
            true,
            true,
            &headers,
            None,
            vec!["tests/mutual-auth/certs/client/client.key".to_string()],
            None,
            None,
        )
        .unwrap();
    }
}
