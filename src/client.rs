use crate::url::UrlExt;
use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use reqwest::{redirect::Policy, Client, Proxy};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;
use std::time::Duration;
use url::Url;

/// Configuration struct for initializing a reqwest client
pub struct ClientConfig<'a, I>
where
    I: IntoIterator,
    I::Item: AsRef<Path> + std::fmt::Debug,
{
    /// The timeout for requests in seconds
    pub timeout: u64,
    /// The User-Agent string to use for requests
    pub user_agent: &'a str,
    /// Whether to follow redirects
    pub redirects: bool,
    /// Whether to allow insecure connections
    pub insecure: bool,
    /// Headers to include in requests
    pub headers: &'a HashMap<String, String>,
    /// Proxy server to use for requests
    pub proxy: Option<&'a str>,
    /// Server certificates to use for requests
    pub server_certs: Option<I>,
    /// Client certificate to use for requests
    pub client_cert: Option<&'a str>,
    /// Client key to use for requests
    pub client_key: Option<&'a str>,
    /// scope for redirect handling
    pub scope: &'a [Url],
}

/// Create a redirect policy based on the provided config
fn create_redirect_policy<I>(config: &ClientConfig<'_, I>) -> Policy
where
    I: IntoIterator,
    I::Item: AsRef<Path> + std::fmt::Debug,
{
    // old behavior set Policy::limited(10) if redirects were enabled
    // and Policy::none() if they were not. New policy behavior is
    // scope-aware when redirects are enabled and scope is provided.

    if config.redirects && config.scope.is_empty() {
        // scope should never be empty, so this should never be hit, just a fallback
        Policy::limited(10)
    } else if config.redirects {
        // create a custom policy that checks scope for each redirect
        let scoped_urls = config.scope.to_vec();

        Policy::custom(move |attempt| {
            let redirect_url = attempt.url();

            if redirect_url.is_in_scope(&scoped_urls) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        })
    } else {
        Policy::none()
    }
}

/// Create and return an instance of [reqwest::Client](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)
/// with optional scope-aware redirect handling
pub fn initialize<I>(config: ClientConfig<'_, I>) -> Result<Client>
where
    I: IntoIterator,
    I::Item: AsRef<Path> + std::fmt::Debug,
{
    let policy = create_redirect_policy(&config);

    let header_map: HeaderMap = config.headers.try_into()?;

    let mut client = Client::builder()
        .timeout(Duration::new(config.timeout, 0))
        .user_agent(config.user_agent)
        .danger_accept_invalid_certs(config.insecure)
        .default_headers(header_map)
        .redirect(policy)
        .http1_title_case_headers();

    if let Some(some_proxy) = config.proxy {
        if !some_proxy.is_empty() {
            // it's not an empty string; set the proxy
            let proxy_obj = Proxy::all(some_proxy)?;
            // just add the proxy to the client
            // don't build and return it just yet
            client = client.proxy(proxy_obj);
        }
    }

    for cert_path in config.server_certs.into_iter().flatten() {
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

    if let (Some(cert_path), Some(key_path)) = (config.client_cert, config.client_key) {
        if !cert_path.is_empty() && !key_path.is_empty() {
            let cert = std::fs::read(cert_path)?;
            let key = std::fs::read(key_path)?;

            let identity = reqwest::Identity::from_pkcs8_pem(&cert, &key).with_context(|| {
                format!(
                    "either {cert_path} or {key_path} are invalid; expecting PEM encoded certificate and key")
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
        let client_config = ClientConfig {
            timeout: 0,
            user_agent: "stuff",
            redirects: true,
            insecure: false,
            headers: &headers,
            proxy: Some("not a valid proxy"),
            server_certs: Option::<Vec<String>>::None,
            client_cert: None,
            client_key: None,
            scope: &Vec::new(),
        };
        initialize(client_config).unwrap();
    }

    #[test]
    /// create client with a proxy, expect no error
    fn client_with_good_proxy() {
        let headers = HashMap::new();
        let proxy = "http://127.0.0.1:8080";
        let client_config = ClientConfig {
            timeout: 0,
            user_agent: "stuff",
            redirects: true,
            insecure: true,
            headers: &headers,
            proxy: Some(proxy),
            server_certs: Option::<Vec<String>>::None,
            client_cert: None,
            client_key: None,
            scope: &Vec::new(),
        };
        initialize(client_config).unwrap();
    }

    #[test]
    /// create client with a server cert in pem format, expect no error
    fn client_with_valid_server_pem() {
        let headers = HashMap::new();
        let server_certs = vec!["tests/mutual-auth/certs/server/server.crt.1".to_string()];
        let client_config = ClientConfig {
            timeout: 0,
            user_agent: "stuff",
            redirects: true,
            insecure: true,
            headers: &headers,
            proxy: None,
            server_certs: Some(server_certs),
            client_cert: None,
            client_key: None,
            scope: &Vec::new(),
        };
        initialize(client_config).unwrap();
    }

    #[test]
    /// create client with a server cert in der format, expect no error
    fn client_with_valid_server_der() {
        let headers = HashMap::new();
        let server_certs = vec!["tests/mutual-auth/certs/server/server.der".to_string()];
        let client_config = ClientConfig {
            timeout: 0,
            user_agent: "stuff",
            redirects: true,
            insecure: true,
            headers: &headers,
            proxy: None,
            server_certs: Some(server_certs),
            client_cert: None,
            client_key: None,
            scope: &Vec::new(),
        };
        initialize(client_config).unwrap();
    }

    #[test]
    /// create client with two server certs (pem and der), expect no error
    fn client_with_valid_server_pem_and_der() {
        let headers = HashMap::new();
        let server_certs = vec![
            "tests/mutual-auth/certs/server/server.crt.1".to_string(),
            "tests/mutual-auth/certs/server/server.der".to_string(),
        ];

        println!("{}", std::env::current_dir().unwrap().display());

        let client_config = ClientConfig {
            timeout: 0,
            user_agent: "stuff",
            redirects: true,
            insecure: true,
            headers: &headers,
            proxy: None,
            server_certs: Some(server_certs),
            client_cert: None,
            client_key: None,
            scope: &Vec::new(),
        };
        initialize(client_config).unwrap();
    }

    /// create client with invalid certificate, expect panic
    #[test]
    #[should_panic]
    fn client_with_invalid_server_cert() {
        let headers = HashMap::new();
        let server_certs = vec!["tests/mutual-auth/certs/client/client.key".to_string()];
        let client_config = ClientConfig {
            timeout: 0,
            user_agent: "stuff",
            redirects: true,
            insecure: true,
            headers: &headers,
            proxy: None,
            server_certs: Some(server_certs),
            client_cert: None,
            client_key: None,
            scope: &Vec::new(),
        };
        initialize(client_config).unwrap();
    }

    #[test]
    /// test that scope-aware client can be created with valid parameters
    fn initialize_with_scope_creates_client() {
        let headers = HashMap::new();
        let scope = vec![
            Url::parse("https://api.example.com").unwrap(),
            Url::parse("https://cdn.example.com").unwrap(),
        ];

        let client_config = ClientConfig {
            timeout: 5,
            user_agent: "test-agent",
            redirects: true,
            insecure: false,
            headers: &headers,
            proxy: None,
            server_certs: Option::<Vec<String>>::None,
            client_cert: None,
            client_key: None,
            scope: &scope,
        };
        let client = initialize(client_config);

        assert!(client.is_ok());
    }

    #[test]
    /// test that scope-aware client works without scope (should use default behavior)
    fn initialize_with_scope_empty_scope() {
        let headers = HashMap::new();
        let scope = vec![];

        let client_config = ClientConfig {
            timeout: 5,
            user_agent: "test-agent",
            redirects: true,
            insecure: false,
            headers: &headers,
            proxy: None,
            server_certs: Option::<Vec<String>>::None,
            client_cert: None,
            client_key: None,
            scope: &scope,
        };
        let client = initialize(client_config);

        assert!(client.is_ok());
    }
}
