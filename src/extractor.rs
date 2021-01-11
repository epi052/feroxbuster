use crate::{
    client,
    config::{Configuration, CONFIGURATION},
    scanner::SCANNED_URLS,
    statistics::{
        StatCommand::{self, UpdateUsizeField},
        StatField::{LinksExtracted, TotalExpected},
    },
    utils::{format_url, make_request},
    FeroxResponse,
};
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Url;
use std::collections::HashSet;
use tokio::sync::mpsc::UnboundedSender;

/// Regular expression used in [LinkFinder](https://github.com/GerbenJavado/LinkFinder)
///
/// Incorporates change from this [Pull Request](https://github.com/GerbenJavado/LinkFinder/pull/66/files)
const LINKFINDER_REGEX: &str = r#"(?:"|')(((?:[a-zA-Z]{1,10}://|//)[^"'/]{1,}\.[a-zA-Z]{2,}[^"']{0,})|((?:/|\.\./|\./)[^"'><,;| *()(%%$^/\\\[\]][^"'><,;|()]{1,})|([a-zA-Z0-9_\-/]{1,}/[a-zA-Z0-9_\-/]{1,}\.(?:[a-zA-Z]{1,4}|action)(?:[\?|#][^"|']{0,}|))|([a-zA-Z0-9_\-/]{1,}/[a-zA-Z0-9_\-/]{3,}(?:[\?|#][^"|']{0,}|))|([a-zA-Z0-9_\-.]{1,}\.(?:php|asp|aspx|jsp|json|action|html|js|txt|xml)(?:[\?|#][^"|']{0,}|)))(?:"|')"#;

/// Regular expression to pull url paths from robots.txt
///
/// ref: https://developers.google.com/search/reference/robots_txt
const ROBOTS_TXT_REGEX: &str =
    r#"(?m)^ *(Allow|Disallow): *(?P<url_path>[a-zA-Z0-9._/?#@!&'()+,;%=-]+?)$"#; // multi-line (?m)

lazy_static! {
    /// `LINKFINDER_REGEX` as a regex::Regex type
    static ref LINKS_REGEX: Regex = Regex::new(LINKFINDER_REGEX).unwrap();

    /// `ROBOTS_TXT_REGEX` as a regex::Regex type
    static ref ROBOTS_REGEX: Regex = Regex::new(ROBOTS_TXT_REGEX).unwrap();
}

/// Iterate over a given path, return a list of every sub-path found
///
/// example: `path` contains a link fragment `homepage/assets/img/icons/handshake.svg`
/// the following fragments would be returned:
///   - homepage/assets/img/icons/handshake.svg
///   - homepage/assets/img/icons/
///   - homepage/assets/img/
///   - homepage/assets/
///   - homepage/
fn get_sub_paths_from_path(path: &str) -> Vec<String> {
    log::trace!("enter: get_sub_paths_from_path({})", path);
    let mut paths = vec![];

    // filter out any empty strings caused by .split
    let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let length = parts.len();

    for i in 0..length {
        // iterate over all parts of the path
        if parts.is_empty() {
            // pop left us with an empty vector, we're done
            break;
        }

        let mut possible_path = parts.join("/");

        if possible_path.is_empty() {
            // .join can result in an empty string, which we don't need, ignore
            continue;
        }

        if i > 0 {
            // this isn't the last index of the parts array
            // ex: /buried/misc/stupidfile.php
            // this block skips the file but sees all parent folders
            possible_path = format!("{}/", possible_path);
        }

        paths.push(possible_path); // good sub-path found
        parts.pop(); // use .pop() to remove the last part of the path and continue iteration
    }

    log::trace!("exit: get_sub_paths_from_path -> {:?}", paths);
    paths
}

/// simple helper to stay DRY, trys to join a url + fragment and add it to the `links` HashSet
fn add_link_to_set_of_links(link: &str, url: &Url, links: &mut HashSet<String>) {
    log::trace!(
        "enter: add_link_to_set_of_links({}, {}, {:?})",
        link,
        url.to_string(),
        links
    );
    match url.join(&link) {
        Ok(new_url) => {
            links.insert(new_url.to_string());
        }
        Err(e) => {
            log::error!("Could not join given url to the base url: {}", e);
        }
    }
    log::trace!("exit: add_link_to_set_of_links");
}

/// Given a `reqwest::Response`, perform the following actions
///   - parse the response's text for links using the linkfinder regex
///   - for every link found take its url path and parse each sub-path
///     - example: Response contains a link fragment `homepage/assets/img/icons/handshake.svg`
///       with a base url of http://localhost, the following urls would be returned:
///         - homepage/assets/img/icons/handshake.svg
///         - homepage/assets/img/icons/
///         - homepage/assets/img/
///         - homepage/assets/
///         - homepage/
pub async fn get_links(
    response: &FeroxResponse,
    tx_stats: UnboundedSender<StatCommand>,
) -> HashSet<String> {
    log::trace!(
        "enter: get_links({}, {:?})",
        response.url().as_str(),
        tx_stats
    );

    let mut links = HashSet::<String>::new();

    let body = response.text();

    for capture in LINKS_REGEX.captures_iter(&body) {
        // remove single & double quotes from both ends of the capture
        // capture[0] is the entire match, additional capture groups start at [1]
        let link = capture[0].trim_matches(|c| c == '\'' || c == '"');

        match Url::parse(link) {
            Ok(absolute) => {
                if absolute.domain() != response.url().domain()
                    || absolute.host() != response.url().host()
                {
                    // domains/ips are not the same, don't scan things that aren't part of the original
                    // target url
                    continue;
                }

                add_all_sub_paths(absolute.path(), &response, &mut links);
            }
            Err(e) => {
                // this is the expected error that happens when we try to parse a url fragment
                //     ex: Url::parse("/login") -> Err("relative URL without a base")
                // while this is technically an error, these are good results for us
                if e.to_string().contains("relative URL without a base") {
                    add_all_sub_paths(link, &response, &mut links);
                } else {
                    // unexpected error has occurred
                    log::error!("Could not parse given url: {}", e);
                }
            }
        }
    }

    let multiplier = CONFIGURATION.extensions.len().max(1);

    update_stat!(tx_stats, UpdateUsizeField(LinksExtracted, links.len()));
    update_stat!(
        tx_stats,
        UpdateUsizeField(TotalExpected, links.len() * multiplier)
    );

    log::trace!("exit: get_links -> {:?}", links);

    links
}

/// take a url fragment like homepage/assets/img/icons/handshake.svg and
/// incrementally add
///     - homepage/assets/img/icons/
///     - homepage/assets/img/
///     - homepage/assets/
///     - homepage/
fn add_all_sub_paths(url_path: &str, response: &FeroxResponse, mut links: &mut HashSet<String>) {
    log::trace!(
        "enter: add_all_sub_paths({}, {}, {:?})",
        url_path,
        response,
        links
    );

    for sub_path in get_sub_paths_from_path(url_path) {
        log::debug!("Adding {} to {:?}", sub_path, links);
        add_link_to_set_of_links(&sub_path, &response.url(), &mut links);
    }

    log::trace!("exit: add_all_sub_paths");
}

/// Wrapper around link extraction logic
/// currently used in two places:
///   - links from response bodys
///   - links from robots.txt responses
///
/// general steps taken:
///   - create a new Url object based on cli options/args
///   - check if the new Url has already been seen/scanned -> None
///   - make a request to the new Url ? -> Some(response) : None
pub async fn request_feroxresponse_from_new_link(
    url: &str,
    tx_stats: UnboundedSender<StatCommand>,
) -> Option<FeroxResponse> {
    log::trace!(
        "enter: request_feroxresponse_from_new_link({}, {:?})",
        url,
        tx_stats
    );

    // create a url based on the given command line options, return None on error
    let new_url = match format_url(
        &url,
        &"",
        CONFIGURATION.add_slash,
        &CONFIGURATION.queries,
        None,
        tx_stats.clone(),
    ) {
        Ok(url) => url,
        Err(_) => {
            log::trace!("exit: request_feroxresponse_from_new_link -> None");
            return None;
        }
    };

    if SCANNED_URLS.get_scan_by_url(&new_url.to_string()).is_some() {
        //we've seen the url before and don't need to scan again
        log::trace!("exit: request_feroxresponse_from_new_link -> None");
        return None;
    }

    // make the request and store the response
    let new_response = match make_request(&CONFIGURATION.client, &new_url, tx_stats).await {
        Ok(resp) => resp,
        Err(_) => {
            log::trace!("exit: request_feroxresponse_from_new_link -> None");
            return None;
        }
    };

    let new_ferox_response = FeroxResponse::from(new_response, true).await;

    log::trace!(
        "exit: request_feroxresponse_from_new_link -> {:?}",
        new_ferox_response
    );
    Some(new_ferox_response)
}

/// helper function that simply requests /robots.txt on the given url's base url
///
/// example:
///     http://localhost/api/users -> http://localhost/robots.txt
///     
/// The length of the given path has no effect on what's requested; it's always
/// base url + /robots.txt
pub async fn request_robots_txt(
    base_url: &str,
    config: &Configuration,
    tx_stats: UnboundedSender<StatCommand>,
) -> Option<FeroxResponse> {
    log::trace!(
        "enter: get_robots_file({}, CONFIGURATION, {:?})",
        base_url,
        tx_stats
    );

    // more often than not, domain/robots.txt will redirect to www.domain/robots.txt or something
    // similar; to account for that, create a client that will follow redirects, regardless of
    // what the user specified for the scanning client. Other than redirects, it will respect
    // all other user specified settings
    let follow_redirects = true;

    let proxy = if config.proxy.is_empty() {
        None
    } else {
        Some(config.proxy.as_str())
    };

    let client = client::initialize(
        config.timeout,
        &config.user_agent,
        follow_redirects,
        config.insecure,
        &config.headers,
        proxy,
    );

    if let Ok(mut url) = Url::parse(base_url) {
        url.set_path("/robots.txt"); // overwrite existing path with /robots.txt

        if let Ok(response) = make_request(&client, &url, tx_stats).await {
            let ferox_response = FeroxResponse::from(response, true).await;

            log::trace!("exit: get_robots_file -> {}", ferox_response);
            return Some(ferox_response);
        }
    }

    None
}

/// Entry point to perform link extraction from robots.txt
///
/// `base_url` can have paths and subpaths, however robots.txt will be requested from the
/// root of the url
/// given the url:
///     http://localhost/stuff/things
/// this function requests:
///     http://localhost/robots.txt
pub async fn extract_robots_txt(
    base_url: &str,
    config: &Configuration,
    tx_stats: UnboundedSender<StatCommand>,
) -> HashSet<String> {
    log::trace!(
        "enter: extract_robots_txt({}, CONFIGURATION, {:?})",
        base_url,
        tx_stats
    );
    let mut links = HashSet::new();

    if let Some(response) = request_robots_txt(&base_url, &config, tx_stats.clone()).await {
        for capture in ROBOTS_REGEX.captures_iter(response.text.as_str()) {
            if let Some(new_path) = capture.name("url_path") {
                if let Ok(mut new_url) = Url::parse(base_url) {
                    new_url.set_path(new_path.as_str());
                    add_all_sub_paths(new_url.path(), &response, &mut links);
                }
            }
        }
    }

    let multiplier = CONFIGURATION.extensions.len().max(1);

    update_stat!(tx_stats, UpdateUsizeField(LinksExtracted, links.len()));
    update_stat!(
        tx_stats,
        UpdateUsizeField(TotalExpected, links.len() * multiplier)
    );

    log::trace!("exit: extract_robots_txt -> {:?}", links);
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::make_request;
    use crate::FeroxChannel;
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use reqwest::Client;
    use tokio::sync::mpsc;

    #[test]
    /// extract sub paths from the given url fragment; expect 4 sub paths and that all are
    /// in the expected array
    fn extractor_get_sub_paths_from_path_with_multiple_paths() {
        let path = "homepage/assets/img/icons/handshake.svg";
        let paths = get_sub_paths_from_path(&path);
        let expected = vec![
            "homepage/",
            "homepage/assets/",
            "homepage/assets/img/",
            "homepage/assets/img/icons/",
            "homepage/assets/img/icons/handshake.svg",
        ];

        assert_eq!(paths.len(), expected.len());
        for expected_path in expected {
            assert_eq!(paths.contains(&expected_path.to_string()), true);
        }
    }

    #[test]
    /// extract sub paths from the given url fragment; expect 2 sub paths and that all are
    /// in the expected array. the fragment is wrapped in slashes to ensure no empty strings are
    /// returned
    fn extractor_get_sub_paths_from_path_with_enclosing_slashes() {
        let path = "/homepage/assets/";
        let paths = get_sub_paths_from_path(&path);
        let expected = vec!["homepage/", "homepage/assets"];

        assert_eq!(paths.len(), expected.len());
        for expected_path in expected {
            assert_eq!(paths.contains(&expected_path.to_string()), true);
        }
    }

    #[test]
    /// extract sub paths from the given url fragment; expect 1 sub path, no forward slashes are
    /// included
    fn extractor_get_sub_paths_from_path_with_only_a_word() {
        let path = "homepage";
        let paths = get_sub_paths_from_path(&path);
        let expected = vec!["homepage"];

        assert_eq!(paths.len(), expected.len());
        for expected_path in expected {
            assert_eq!(paths.contains(&expected_path.to_string()), true);
        }
    }

    #[test]
    /// extract sub paths from the given url fragment; expect 1 sub path, forward slash removed
    fn extractor_get_sub_paths_from_path_with_an_absolute_word() {
        let path = "/homepage";
        let paths = get_sub_paths_from_path(&path);
        let expected = vec!["homepage"];

        assert_eq!(paths.len(), expected.len());
        for expected_path in expected {
            assert_eq!(paths.contains(&expected_path.to_string()), true);
        }
    }

    #[test]
    /// test that a full url and fragment are joined correctly, then added to the given list
    /// i.e. the happy path
    fn extractor_add_link_to_set_of_links_happy_path() {
        let url = Url::parse("https://localhost").unwrap();
        let mut links = HashSet::<String>::new();
        let link = "admin";

        assert_eq!(links.len(), 0);
        add_link_to_set_of_links(link, &url, &mut links);

        assert_eq!(links.len(), 1);
        assert!(links.contains("https://localhost/admin"));
    }

    #[test]
    /// test that an invalid path fragment doesn't add anything to the set of links
    fn extractor_add_link_to_set_of_links_with_non_base_url() {
        let url = Url::parse("https://localhost").unwrap();
        let mut links = HashSet::<String>::new();
        let link = "\\\\\\\\";

        assert_eq!(links.len(), 0);
        add_link_to_set_of_links(link, &url, &mut links);

        assert_eq!(links.len(), 0);
        assert!(links.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// use make_request to generate a Response, and use the Response to test get_links;
    /// the response will contain an absolute path to a domain that is not part of the scanned
    /// domain; expect an empty set returned
    async fn extractor_get_links_with_absolute_url_that_differs_from_target_domain(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then|{
            when.method(GET)
                .path("/some-path");
            then.status(200)
                .body("\"http://defintely.not.a.thing.probably.com/homepage/assets/img/icons/handshake.svg\"");
        });

        let client = Client::new();
        let url = Url::parse(&srv.url("/some-path")).unwrap();
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let response = make_request(&client, &url, tx.clone()).await.unwrap();

        let ferox_response = FeroxResponse::from(response, true).await;

        let links = get_links(&ferox_response, tx).await;

        assert!(links.is_empty());

        assert_eq!(mock.hits(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test that /robots.txt is correctly requested given a base url (happy path)
    async fn request_robots_txt_with_and_without_proxy() {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then| {
            when.method(GET).path("/robots.txt");
            then.status(200).body("this is a test");
        });

        let mut config = Configuration::default();

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        request_robots_txt(&srv.url("/api/users/stuff/things"), &config, tx.clone()).await;

        // note: the proxy doesn't actually do anything other than hit a different code branch
        // in this unit test; it would however have an effect on an integration test
        config.proxy = srv.url("/ima-proxy");

        request_robots_txt(&srv.url("/api/different/path"), &config, tx).await;

        assert_eq!(mock.hits(), 2);
    }
}
