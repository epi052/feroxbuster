use super::*;
use crate::{
    client,
    event_handlers::{
        Command::{AddError, AddToUsizeField},
        Handles,
    },
    scan_manager::ScanOrder,
    statistics::{
        StatError::Other,
        StatField::{LinksExtracted, TotalExpected},
    },
    url::FeroxUrl,
    utils::{
        logged_request, make_request, parse_url_with_raw_path, send_try_recursion_command,
        should_deny_url,
    },
    ExtractionResult, DEFAULT_METHOD,
};
use anyhow::{bail, Context, Result};
use futures::StreamExt;
use reqwest::{Client, Response, StatusCode, Url};
use scraper::{Html, Selector};
use std::{borrow::Cow, collections::HashSet};

/// Wrapper around link extraction logic
///   - create a new Url object based on cli options/args
///   - check if the new Url has already been seen/scanned -> None
///   - make a request to the new Url ? -> Some(response) : None
pub(super) async fn request_link(url: &str, handles: Arc<Handles>) -> Result<Response> {
    log::trace!("enter: request_link({})", url);

    let ferox_url = FeroxUrl::from_string(url, handles.clone());

    // create a url based on the given command line options
    let new_url = ferox_url.format("", None)?;

    let scanned_urls = handles.ferox_scans()?;

    if scanned_urls.get_scan_by_url(new_url.as_ref()).is_some() {
        //we've seen the url before and don't need to scan again
        log::trace!("exit: request_link -> None");
        bail!("previously seen url");
    }

    if (!handles.config.url_denylist.is_empty() || !handles.config.regex_denylist.is_empty())
        && should_deny_url(&new_url, handles.clone())?
    {
        // can't allow a denied url to be requested
        bail!(
            "prevented request to {} due to {:?} || {:?}",
            url,
            handles.config.url_denylist,
            handles.config.regex_denylist,
        );
    }

    // make the request and store the response
    let new_response = logged_request(&new_url, DEFAULT_METHOD, None, handles.clone()).await?;

    log::trace!("exit: request_link -> {:?}", new_response);

    Ok(new_response)
}

/// Whether an active scan is recursive or not
#[derive(Debug, Copy, Clone)]
enum RecursionStatus {
    /// Scan is recursive
    Recursive,

    /// Scan is not recursive
    NotRecursive,
}

/// Handles all logic related to extracting links from requested source code
#[derive(Debug)]
pub struct Extractor<'a> {
    /// `LINKFINDER_REGEX` as a regex::Regex type
    pub(super) links_regex: Regex,

    /// `ROBOTS_TXT_REGEX` as a regex::Regex type
    pub(super) robots_regex: Regex,

    /// regex to validate a url
    pub(super) url_regex: Regex,

    /// Response from which to extract links
    pub(super) response: Option<&'a FeroxResponse>,

    /// URL of where to extract links
    pub(super) url: String,

    /// Handles object to house the underlying mpsc transmitters
    pub(super) handles: Arc<Handles>,

    /// type of extraction to be performed
    pub(super) target: ExtractionTarget,
}

/// Extractor implementation
impl<'a> Extractor<'a> {
    /// perform extraction from the given target and return any links found
    pub async fn extract(&self) -> Result<ExtractionResult> {
        log::trace!(
            "enter: extract({:?}) (this fn has no associated trace exit msg)",
            self.target
        );
        match self.target {
            ExtractionTarget::ResponseBody => Ok(self.extract_from_body().await?),
            ExtractionTarget::RobotsTxt => Ok(self.extract_from_robots().await?),
            ExtractionTarget::DirectoryListing => Ok(self.extract_from_dir_listing().await?),
        }
    }

    /// wrapper around logic that performs the following:
    /// - parses `url_to_parse`
    /// - bails if the parsed url doesn't belong to the original host/domain
    /// - otherwise, calls `add_all_sub_paths` with the parsed result
    fn parse_url_and_add_subpaths(
        &self,
        url_to_parse: &str,
        original_url: &Url,
        links: &mut HashSet<String>,
    ) -> Result<()> {
        log::trace!("enter: parse_url_and_add_subpaths({:?})", links);

        match parse_url_with_raw_path(url_to_parse) {
            Ok(absolute) => {
                if absolute.domain() != original_url.domain()
                    || absolute.host() != original_url.host()
                {
                    // domains/ips are not the same, don't scan things that aren't part of the original
                    // target url
                    bail!("parsed url does not belong to original domain/host");
                }

                if self.add_all_sub_paths(absolute.path(), links).is_err() {
                    log::warn!("could not add sub-paths from {} to {:?}", absolute, links);
                }
            }
            Err(e) => {
                // this is the expected error that happens when we try to parse a url fragment
                //     ex: Url::parse("/login") -> Err("relative URL without a base")
                // while this is technically an error, these are good results for us
                if e.to_string().contains("relative URL without a base") {
                    if self.add_all_sub_paths(url_to_parse, links).is_err() {
                        log::warn!(
                            "could not add sub-paths from {} to {:?}",
                            url_to_parse,
                            links
                        );
                    }
                } else {
                    // unexpected error has occurred
                    log::warn!("Could not parse given url: {}", e);
                    self.handles.stats.send(AddError(Other)).unwrap_or_default();
                }
            }
        }

        log::trace!("exit: parse_url_and_add_subpaths");
        Ok(())
    }

    /// given a set of links from a normal http body response, task the request handler to make
    /// the requests
    pub async fn request_links(
        &mut self,
        links: HashSet<String>,
    ) -> Result<Option<tokio::task::JoinHandle<()>>> {
        log::trace!("enter: request_links({:?})", links);

        if links.is_empty() {
            return Ok(None);
        }

        self.update_stats(links.len())?;

        // create clones/remove use of self of/from everything the async move block will need to function
        let cloned_scanned_urls = self.handles.ferox_scans()?;
        let cloned_handles = self.handles.clone();
        let cloned_url = self.url.clone();
        let threads = self.handles.config.threads;
        let recursive = if self.handles.config.no_recursion {
            RecursionStatus::NotRecursive
        } else {
            RecursionStatus::Recursive
        };

        let link_request_task = tokio::spawn(async move {
            let producers = futures::stream::iter(links.into_iter())
                .map(|link| {
                    // another clone to satisfy the async move block
                    let inner_clone = cloned_handles.clone();

                    (
                        tokio::spawn(async move { request_link(&link, inner_clone).await }),
                        cloned_handles.clone(),
                        cloned_scanned_urls.clone(),
                        recursive,
                        cloned_url.clone(),
                    )
                })
                .for_each_concurrent(
                    threads,
                    |(join_handle, c_handles, c_scanned_urls, c_recursive, og_url)| async move {
                        match join_handle.await {
                            Ok(Ok(reqwest_response)) => {
                                let mut resp = FeroxResponse::from(
                                    reqwest_response,
                                    &og_url,
                                    DEFAULT_METHOD,
                                    c_handles.config.output_level,
                                )
                                .await;

                                // filter if necessary
                                if c_handles
                                    .filters
                                    .data
                                    .should_filter_response(&resp, c_handles.stats.tx.clone())
                                {
                                    return;
                                }

                                // request and report assumed file
                                if resp.is_file() || !resp.is_directory() {
                                    log::debug!("Extracted File: {}", resp);

                                    c_scanned_urls.add_file_scan(
                                        resp.url().as_str(),
                                        ScanOrder::Latest,
                                        c_handles.clone(),
                                    );

                                    if c_handles.config.collect_extensions {
                                        // no real reason this should fail
                                        resp.parse_extension(c_handles.clone()).unwrap();
                                    }

                                    if let Err(e) = resp.send_report(c_handles.output.tx.clone()) {
                                        log::warn!(
                                            "Could not send FeroxResponse to output handler: {}",
                                            e
                                        );
                                    }

                                    return;
                                }

                                if matches!(c_recursive, RecursionStatus::Recursive) {
                                    log::debug!("Extracted Directory: {}", resp);

                                    if !resp.url().as_str().ends_with('/')
                                        && (resp.status().is_success()
                                            || matches!(resp.status(), &StatusCode::FORBIDDEN))
                                    {
                                        // if the url doesn't end with a /
                                        // and the response code is either a 2xx or 403

                                        // since all of these are 2xx or 403, recursion is only attempted if the
                                        // url ends in a /. I am actually ok with adding the slash and not
                                        // adding it, as both have merit.  Leaving it in for now to see how
                                        // things turn out (current as of: v1.1.0)
                                        resp.set_url(&format!("{}/", resp.url()));
                                    }

                                    if c_handles.config.filter_status.is_empty() {
                                        // -C wasn't used, so -s is the only 'filter' left to account for
                                        if c_handles
                                            .config
                                            .status_codes
                                            .contains(&resp.status().as_u16())
                                        {
                                            send_try_recursion_command(c_handles.clone(), resp)
                                                .await
                                                .unwrap_or_default();
                                        }
                                    } else {
                                        // -C was used, that means the filters above would have removed
                                        // those responses, and anything else should be let through
                                        send_try_recursion_command(c_handles.clone(), resp)
                                            .await
                                            .unwrap_or_default();
                                    }
                                }
                            }
                            Ok(Err(err)) => {
                                log::warn!("Error during link extraction: {}", err);
                            }
                            Err(err) => {
                                log::warn!("JoinError during link extraction: {}", err);
                            }
                        }
                    },
                );

            // wait for the requests to finish
            producers.await;
        });

        log::trace!("exit: request_links");
        Ok(Some(link_request_task))
    }

    /// wrapper around link extraction via html attributes
    fn extract_all_links_from_html_tags(
        &self,
        resp_url: &Url,
        links: &mut HashSet<String>,
        html: &Html,
    ) {
        self.extract_links_by_attr(resp_url, links, html, "a", "href");
        self.extract_links_by_attr(resp_url, links, html, "img", "src");
        self.extract_links_by_attr(resp_url, links, html, "form", "action");
        self.extract_links_by_attr(resp_url, links, html, "script", "src");
        self.extract_links_by_attr(resp_url, links, html, "iframe", "src");
        self.extract_links_by_attr(resp_url, links, html, "div", "src");
        self.extract_links_by_attr(resp_url, links, html, "frame", "src");
        self.extract_links_by_attr(resp_url, links, html, "embed", "src");
        self.extract_links_by_attr(resp_url, links, html, "link", "href");
    }

    /// Given the body of a `reqwest::Response`, perform the following actions
    ///   - parse the body for links using the linkfinder regex
    ///   - for every link found take its url path and parse each sub-path
    ///     - example: Response contains a link fragment `homepage/assets/img/icons/handshake.svg`
    ///       with a base url of http://localhost, the following urls would be returned:
    ///         - homepage/assets/img/icons/handshake.svg
    ///         - homepage/assets/img/icons/
    ///         - homepage/assets/img/
    ///         - homepage/assets/
    ///         - homepage/
    fn extract_all_links_from_javascript(
        &self,
        response_body: &str,
        response_url: &Url,
        links: &mut HashSet<String>,
    ) {
        log::trace!(
            "enter: extract_all_links_from_javascript(html body..., {}, {:?})",
            response_url.as_str(),
            links
        );

        for capture in self.links_regex.captures_iter(response_body) {
            // remove single & double quotes from both ends of the capture
            // capture[0] is the entire match, additional capture groups start at [1]
            let link = capture[0].trim_matches(|c| c == '\'' || c == '"');

            if self
                .parse_url_and_add_subpaths(link, response_url, links)
                .is_err()
            {
                // purposely not logging the error here, due to the frequency with which it gets hit
            }
        }

        log::trace!("exit: extract_all_links_from_javascript");
    }

    /// take a url fragment like homepage/assets/img/icons/handshake.svg and
    /// incrementally add
    ///   - homepage/assets/img/icons/
    ///   - homepage/assets/img/
    ///   - homepage/assets/
    ///   - homepage/
    fn add_all_sub_paths(&self, url_path: &str, links: &mut HashSet<String>) -> Result<()> {
        log::trace!("enter: add_all_sub_paths({}, {:?})", url_path, links);

        for sub_path in self.get_sub_paths_from_path(url_path) {
            self.add_link_to_set_of_links(&sub_path, links)?;
        }

        log::trace!("exit: add_all_sub_paths");
        Ok(())
    }

    /// given a url path, trim whitespace, remove slashes, and queries/fragments; return the
    /// normalized string
    pub(super) fn normalize_url_path(&self, path: &str) -> String {
        log::trace!("enter: normalize_url_path({})", path);

        // remove whitespace and leading '/'
        let path_str: String = path
            .trim()
            .trim_start_matches('/')
            .chars()
            .filter(|char| !char.is_whitespace())
            .collect();

        // snippets from rfc-3986:
        //
        //          foo://example.com:8042/over/there?name=ferret#nose
        //          \_/   \______________/\_________/ \_________/ \__/
        //           |           |            |            |        |
        //        scheme     authority       path        query   fragment
        //
        // The path component is terminated
        //    by the first question mark ("?") or number sign ("#") character, or
        //    by the end of the URI.
        //
        // The query component is indicated by the first question
        //    mark ("?") character and terminated by a number sign ("#") character
        //    or by the end of the URI.
        let (path_str, _discarded) = path_str
            .split_once('?')
            // if there isn't a '?', try to remove a fragment
            .unwrap_or_else(|| {
                // if there isn't a '#', return (original, empty)
                path_str.split_once('#').unwrap_or((&path_str, ""))
            });

        log::trace!("exit: normalize_url_path -> {}", path_str);
        path_str.into()
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
    pub(super) fn get_sub_paths_from_path(&self, path: &str) -> Vec<String> {
        log::trace!("enter: get_sub_paths_from_path({})", path);
        let mut paths = vec![];

        let normalized_path = self.normalize_url_path(path);

        // filter out any empty strings caused by .split
        let mut parts: Vec<Cow<_>> = normalized_path
            .split('/')
            .map(|s| self.url_regex.replace_all(s, ""))
            .filter(|s| !s.is_empty())
            .collect();

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
                possible_path = format!("{possible_path}/");
            }

            paths.push(possible_path); // good sub-path found
            parts.pop(); // use .pop() to remove the last part of the path and continue iteration
        }

        log::trace!("exit: get_sub_paths_from_path -> {:?}", paths);
        paths
    }

    /// simple helper to stay DRY, tries to join a url + fragment and add it to the `links` HashSet
    pub(super) fn add_link_to_set_of_links(
        &self,
        link: &str,
        links: &mut HashSet<String>,
    ) -> Result<()> {
        log::trace!("enter: add_link_to_set_of_links({}, {:?})", link, links);

        let old_url = match self.target {
            ExtractionTarget::ResponseBody | ExtractionTarget::DirectoryListing => {
                self.response.unwrap().url().clone()
            }
            ExtractionTarget::RobotsTxt => match parse_url_with_raw_path(&self.url) {
                Ok(u) => u,
                Err(e) => {
                    bail!("Could not parse {}: {}", self.url, e);
                }
            },
        };

        let new_url = old_url
            .join(link)
            .with_context(|| format!("Could not join {old_url} with {link}"))?;

        if old_url.domain() != new_url.domain() || old_url.host() != new_url.host() {
            // domains/ips are not the same, don't scan things that aren't part of the original
            // target url
            log::debug!(
                "Skipping {} because it's not part of the original target",
                new_url
            );
            log::trace!("exit: add_link_to_set_of_links");
            return Ok(());
        }

        links.insert(new_url.to_string());

        log::trace!("exit: add_link_to_set_of_links");

        Ok(())
    }

    /// Entry point to perform link extraction from robots.txt
    ///
    /// `base_url` can have paths and subpaths, however robots.txt will be requested from the
    /// root of the url
    /// given the url:
    ///     http://localhost/stuff/things
    /// this function requests:
    ///     http://localhost/robots.txt
    pub(super) async fn extract_from_robots(&self) -> Result<ExtractionResult> {
        log::trace!("enter: extract_robots_txt");

        let mut result: HashSet<_> = ExtractionResult::new();

        // request
        let response = self.make_extract_request("/robots.txt").await?;
        let body = response.text();

        for capture in self.robots_regex.captures_iter(body) {
            if let Some(new_path) = capture.name("url_path") {
                let mut new_url = parse_url_with_raw_path(&self.url)?;

                new_url.set_path(new_path.as_str());

                if self.add_all_sub_paths(new_url.path(), &mut result).is_err() {
                    log::warn!("could not add sub-paths from {} to {:?}", new_url, result);
                }
            }
        }

        log::trace!("exit: extract_robots_txt -> {:?}", result);
        Ok(result)
    }

    /// outer-most wrapper for parsing html response bodies in search of additional content.
    /// performs the following high-level steps:
    /// - requests the page, if necessary
    /// - checks the page to see if directory listing is enabled and sucks up all the links, if so
    /// - uses the linkfinder regex to grab links from embedded javascript/javascript files
    /// - extracts many different types of link sources from the html itself
    pub(super) async fn extract_from_body(&self) -> Result<ExtractionResult> {
        log::trace!("enter: extract_from_body");

        let mut result = ExtractionResult::new();

        let response = self.response.unwrap();
        let resp_url = response.url();
        let body = response.text();
        let html = Html::parse_document(body);

        // extract links from html tags/attributes and embedded javascript
        self.extract_all_links_from_html_tags(resp_url, &mut result, &html);
        self.extract_all_links_from_javascript(body, resp_url, &mut result);

        log::trace!("exit: extract_from_body -> {:?}", result);
        Ok(result)
    }

    /// parses html response bodies in search of <a> tags.
    ///
    /// the assumption is that directory listing is turned on and this extraction target simply
    /// scoops up all the links for the given directory. The test to detect a directory listing
    /// is located in `HeuristicTests`
    pub async fn extract_from_dir_listing(&self) -> Result<ExtractionResult> {
        log::trace!("enter: extract_from_dir_listing");

        let mut result = ExtractionResult::new();

        let response = self.response.unwrap();
        let html = Html::parse_document(response.text());

        self.extract_links_by_attr(response.url(), &mut result, &html, "a", "href");

        log::trace!("exit: extract_from_dir_listing -> {:?}", result);
        Ok(result)
    }

    /// simple helper to get html links by tag/attribute and add it to the `links` HashSet
    fn extract_links_by_attr(
        &self,
        resp_url: &Url,
        links: &mut HashSet<String>,
        html: &Html,
        html_tag: &str,
        html_attr: &str,
    ) {
        log::trace!("enter: extract_links_by_attr");

        let selector = Selector::parse(html_tag).unwrap();

        let tags = html
            .select(&selector)
            .filter(|a| a.value().attrs().any(|attr| attr.0 == html_attr));

        for tag in tags {
            if let Some(link) = tag.value().attr(html_attr) {
                log::debug!("Parsed link \"{}\" from {}", link, resp_url.as_str());

                if self
                    .parse_url_and_add_subpaths(link, resp_url, links)
                    .is_err()
                {
                    log::debug!("link didn't belong to the target domain/host: {}", link);
                }
            }
        }

        log::trace!("exit: extract_links_by_attr");
    }

    /// helper function that simply requests at <location> on the given url's base url
    ///
    /// example:
    ///     http://localhost/api/users -> http://localhost/<location>
    pub(super) async fn make_extract_request(&self, location: &str) -> Result<FeroxResponse> {
        log::trace!("enter: make_extract_request");

        // need late binding here to avoid 'creates a temporary which is freed...' in the
        // `let ... if` below to avoid cloning the client out of config
        let mut client = Client::new();

        if location == "/robots.txt" {
            // more often than not, domain/robots.txt will redirect to www.domain/robots.txt or something
            // similar; to account for that, create a client that will follow redirects, regardless of
            // what the user specified for the scanning client. Other than redirects, it will respect
            // all other user specified settings
            let follow_redirects = true;

            let proxy = if self.handles.config.proxy.is_empty() {
                None
            } else {
                Some(self.handles.config.proxy.as_str())
            };

            let server_certs = &self.handles.config.server_certs;

            let client_cert = if self.handles.config.client_cert.is_empty() {
                None
            } else {
                Some(self.handles.config.client_cert.as_str())
            };

            let client_key = if self.handles.config.client_key.is_empty() {
                None
            } else {
                Some(self.handles.config.client_key.as_str())
            };

            client = client::initialize(
                self.handles.config.timeout,
                &self.handles.config.user_agent,
                follow_redirects,
                self.handles.config.insecure,
                &self.handles.config.headers,
                proxy,
                server_certs,
                client_cert,
                client_key,
            )?;
        }

        let client = if location != "/robots.txt" {
            &self.handles.config.client
        } else {
            &client
        };

        let mut url = parse_url_with_raw_path(&self.url)?;
        url.set_path(location); // overwrite existing path

        // purposefully not using logged_request here due to using the special client
        let response = make_request(
            client,
            &url,
            DEFAULT_METHOD,
            None,
            self.handles.config.output_level,
            &self.handles.config,
            self.handles.stats.tx.clone(),
        )
        .await?;

        let ferox_response = FeroxResponse::from(
            response,
            &self.url,
            DEFAULT_METHOD,
            self.handles.config.output_level,
        )
        .await;
        // note: don't call parse_extension here. If we call it here, it gets called on robots.txt

        log::trace!("exit: make_extract_request -> {}", ferox_response);
        Ok(ferox_response)
    }

    /// update total number of links extracted and expected responses
    fn update_stats(&self, num_links: usize) -> Result<()> {
        let multiplier = self.handles.expected_num_requests_multiplier();

        self.handles
            .stats
            .send(AddToUsizeField(LinksExtracted, num_links))?;
        self.handles
            .stats
            .send(AddToUsizeField(TotalExpected, num_links * multiplier))?;

        Ok(())
    }
}
