use super::*;
use crate::utils::should_deny_url;
use crate::{
    client,
    event_handlers::{
        Command,
        Command::{AddError, AddToUsizeField},
        Handles,
    },
    progress::PROGRESS_PRINTER,
    scan_manager::ScanOrder,
    statistics::{
        StatError::Other,
        StatField::{LinksExtracted, TotalExpected},
    },
    url::FeroxUrl,
    utils::{ferox_print, logged_request, make_request, status_colorizer},
    DEFAULT_METHOD,
};
use anyhow::{bail, Context, Result};
use reqwest::{StatusCode, Url};
use scraper::{Html, Selector};
use std::collections::HashSet;
use tokio::sync::oneshot;

/// Whether an active scan is recursive or not
#[derive(Debug)]
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
    pub async fn extract(&self) -> Result<(HashSet<String>, bool)> {
        log::trace!("enter: extract (this fn has associated trace exit msg)");
        match self.target {
            ExtractionTarget::ResponseBody => Ok(self.extract_from_body().await?),
            ExtractionTarget::RobotsTxt => Ok(self.extract_from_robots().await?),
            ExtractionTarget::ParseHtml => Ok(self.parse_html().await?),
        }
    }

    /// given a set of links from a normal http body response, task the request handler to make
    /// the requests
    pub async fn request_links(&self, links: HashSet<String>) -> Result<()> {
        log::trace!("enter: request_links({:?})", links);
        let recursive = if self.handles.config.no_recursion {
            RecursionStatus::NotRecursive
        } else {
            RecursionStatus::Recursive
        };

        let scanned_urls = self.handles.ferox_scans()?;

        for link in links {
            let mut resp = match self.request_link(&link).await {
                Ok(resp) => resp,
                Err(_) => continue,
            };

            // filter if necessary
            if self
                .handles
                .filters
                .data
                .should_filter_response(&resp, self.handles.stats.tx.clone())
            {
                continue;
            }

            // request and report assumed file
            if resp.is_file() || !resp.is_directory() {
                log::debug!("Extracted File: {}", resp);

                scanned_urls.add_file_scan(&resp.url().to_string(), ScanOrder::Latest);

                if let Err(e) = resp.clone().send_report(self.handles.output.tx.clone()) {
                    log::warn!("Could not send FeroxResponse to output handler: {}", e);
                }

                continue;
            }

            if matches!(recursive, RecursionStatus::Recursive) {
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

                self.handles
                    .send_scan_command(Command::TryRecursion(Box::new(resp)))?;
                let (tx, rx) = oneshot::channel::<bool>();
                self.handles.send_scan_command(Command::Sync(tx))?;
                rx.await?;
            }
        }
        log::trace!("exit: request_links");
        Ok(())
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
    pub(super) async fn extract_from_body(&self) -> Result<(HashSet<String>, bool)> {
        log::trace!("enter: extract_from_body");

        let mut links = HashSet::<String>::new();

        // Response
        let response = self.response.unwrap();
        let resp_url = response.url();
        let body = response.text();
        let html = Html::parse_document(body);

        // Extract Links
        self.extract_links_by_attr(resp_url, &mut links, &html, "a", "href");
        self.extract_links_by_attr(resp_url, &mut links, &html, "img", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "form", "action");
        self.extract_links_by_attr(resp_url, &mut links, &html, "script", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "iframe", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "div", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "frame", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "embed", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "script", "src");

        for capture in self.links_regex.captures_iter(body) {
            // remove single & double quotes from both ends of the capture
            // capture[0] is the entire match, additional capture groups start at [1]
            let link = capture[0].trim_matches(|c| c == '\'' || c == '"');

            match Url::parse(link) {
                Ok(absolute) => {
                    if absolute.domain() != self.response.unwrap().url().domain()
                        || absolute.host() != self.response.unwrap().url().host()
                    {
                        // domains/ips are not the same, don't scan things that aren't part of the original
                        // target url
                        continue;
                    }

                    if self.add_all_sub_paths(absolute.path(), &mut links).is_err() {
                        log::warn!("could not add sub-paths from {} to {:?}", absolute, links);
                    }
                }
                Err(e) => {
                    // this is the expected error that happens when we try to parse a url fragment
                    //     ex: Url::parse("/login") -> Err("relative URL without a base")
                    // while this is technically an error, these are good results for us
                    if e.to_string().contains("relative URL without a base") {
                        if self.add_all_sub_paths(link, &mut links).is_err() {
                            log::warn!("could not add sub-paths from {} to {:?}", link, links);
                        }
                    } else {
                        // unexpected error has occurred
                        log::warn!("Could not parse given url: {}", e);
                        self.handles.stats.send(AddError(Other)).unwrap_or_default();
                    }
                }
            }
        }

        self.update_stats(links.len())?;

        log::trace!("exit: extract_from_body -> {:?}", links);

        Ok((links, false))
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
    pub(super) fn add_link_to_set_of_links(
        &self,
        link: &str,
        links: &mut HashSet<String>,
    ) -> Result<()> {
        log::trace!("enter: add_link_to_set_of_links({}, {:?})", link, links);

        let old_url = match self.target {
            ExtractionTarget::ResponseBody => self.response.unwrap().url().clone(),
            ExtractionTarget::RobotsTxt => match Url::parse(&self.url) {
                Ok(u) => u,
                Err(e) => {
                    bail!("Could not parse {}: {}", self.url, e);
                }
            },
            ExtractionTarget::ParseHtml => match Url::parse(&self.url) {
                Ok(u) => u,
                Err(e) => {
                    bail!("Could not parse {}: {}", self.url, e);
                }
            },
        };

        let new_url = old_url
            .join(link)
            .with_context(|| format!("Could not join {} with {}", old_url, link))?;

        links.insert(new_url.to_string());

        log::trace!("exit: add_link_to_set_of_links");

        Ok(())
    }

    /// Wrapper around link extraction logic
    ///   - create a new Url object based on cli options/args
    ///   - check if the new Url has already been seen/scanned -> None
    ///   - make a request to the new Url ? -> Some(response) : None
    pub(super) async fn request_link(&self, url: &str) -> Result<FeroxResponse> {
        log::trace!("enter: request_link({})", url);

        let ferox_url = FeroxUrl::from_string(url, self.handles.clone());

        // create a url based on the given command line options
        let new_url = ferox_url.format("", None)?;

        let scanned_urls = self.handles.ferox_scans()?;

        if scanned_urls.get_scan_by_url(&new_url.to_string()).is_some() {
            //we've seen the url before and don't need to scan again
            log::trace!("exit: request_link -> None");
            bail!("previously seen url");
        }

        if (!self.handles.config.url_denylist.is_empty()
            || !self.handles.config.regex_denylist.is_empty())
            && should_deny_url(&new_url, self.handles.clone())?
        {
            // can't allow a denied url to be requested
            bail!(
                "prevented request to {} due to {:?} || {:?}",
                url,
                self.handles.config.url_denylist,
                self.handles.config.regex_denylist,
            );
        }

        // make the request and store the response
        let new_response =
            logged_request(&new_url, DEFAULT_METHOD, None, self.handles.clone()).await?;

        let new_ferox_response = FeroxResponse::from(
            new_response,
            url,
            DEFAULT_METHOD,
            true,
            self.handles.config.output_level,
        )
        .await;

        log::trace!("exit: request_link -> {:?}", new_ferox_response);

        Ok(new_ferox_response)
    }

    /// Entry point to perform link extraction from robots.txt
    ///
    /// `base_url` can have paths and subpaths, however robots.txt will be requested from the
    /// root of the url
    /// given the url:
    ///     http://localhost/stuff/things
    /// this function requests:
    ///     http://localhost/robots.txt
    pub(super) async fn extract_from_robots(&self) -> Result<(HashSet<String>, bool)> {
        log::trace!("enter: extract_robots_txt");

        let mut links: HashSet<String> = HashSet::new();

        // request
        let response = self.make_extract_request("/robots.txt").await?;
        let body = response.text();

        for capture in self.robots_regex.captures_iter(body) {
            if let Some(new_path) = capture.name("url_path") {
                let mut new_url = Url::parse(&self.url)?;
                new_url.set_path(new_path.as_str());
                if self.add_all_sub_paths(new_url.path(), &mut links).is_err() {
                    log::warn!("could not add sub-paths from {} to {:?}", new_url, links);
                }
            }
        }

        self.update_stats(links.len())?;

        log::trace!("exit: extract_robots_txt -> {:?}", links);
        Ok((links, false))
    }

    /// Entry point to parse html for links (i.e. webscraping, directory listings)
    /// this function requests:
    ///     http://localhost/<location>
    pub(super) async fn parse_html(&self) -> Result<(HashSet<String>, bool)> {
        log::trace!("enter: parse_html");

        let mut links: HashSet<String> = HashSet::new();

        // Response
        let url = Url::parse(&self.url)?;
        let response = self.make_extract_request(url.path()).await?;
        let resp_url = response.url();
        let body = response.text();
        let html = Html::parse_document(body);

        // Directory listing heuristic detection to not continue scanning
        // Index of /: apache
        // Directory Listing for /: tomcat,
        // Directory Listing -- /: ASP.NET
        // <host> - /: iis, azure, skipping due to loose heuristic
        let title_selector = Selector::parse("title").unwrap();
        for t in html.select(&title_selector) {
            let title = t.inner_html().to_lowercase();
            if title.contains("directory listing for /")
                || title.contains("index of /")
                || title.contains("directory listing -- /")
            {
                log::debug!("Directory listing heuristic detection from \"{}\"", title);
                let msg = format!(
                    "{} {:>8} {:>8}l {:>8}w {:>8}c {} => Directory listing\n",
                    status_colorizer(response.status().as_str()),
                    response.method().as_str(),
                    response.line_count().to_string(),
                    response.word_count().to_string(),
                    response.content_length().to_string(),
                    response.url(),
                );
                ferox_print(&msg, &PROGRESS_PRINTER);

                self.extract_links_by_attr(resp_url, &mut links, &html, "a", "href");
                self.update_stats(links.len())?;

                log::trace!("exit: parse_html -> {:?}", links);
                return Ok((links, true));
            }
        }

        // Extract Links
        self.extract_links_by_attr(resp_url, &mut links, &html, "a", "href");
        self.extract_links_by_attr(resp_url, &mut links, &html, "img", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "form", "action");
        self.extract_links_by_attr(resp_url, &mut links, &html, "script", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "iframe", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "div", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "frame", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "embed", "src");
        self.extract_links_by_attr(resp_url, &mut links, &html, "script", "src");

        self.update_stats(links.len())?;

        log::trace!("exit: parse_html -> {:?}", links);
        Ok((links, false))
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
        for t in tags {
            if let Some(link) = t.value().attr(html_attr) {
                log::debug!("Parsed link \"{}\" from {}", link, resp_url.as_str());

                match Url::parse(link) {
                    Ok(absolute) => {
                        if absolute.domain() != resp_url.domain()
                            || absolute.host() != resp_url.host()
                        {
                            // domains/ips are not the same, don't scan things that aren't part of the original
                            // target url
                            continue;
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
                            if self.add_all_sub_paths(link, links).is_err() {
                                log::warn!("could not add sub-paths from {} to {:?}", link, links);
                            }
                        } else {
                            // unexpected error has occurred
                            log::warn!("Could not parse given url: {}", e);
                            self.handles.stats.send(AddError(Other)).unwrap_or_default();
                        }
                    }
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

        let client = client::initialize(
            self.handles.config.timeout,
            &self.handles.config.user_agent,
            follow_redirects,
            self.handles.config.insecure,
            &self.handles.config.headers,
            proxy,
        )?;

        let mut url = Url::parse(&self.url)?;
        url.set_path(location); // overwrite existing path

        // purposefully not using logged_request here due to using the special client
        let response = make_request(
            &client,
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
            true,
            self.handles.config.output_level,
        )
        .await;

        log::trace!("exit: make_extract_request -> {}", ferox_response);
        Ok(ferox_response)
    }

    /// update total number of links extracted and expected responses
    fn update_stats(&self, num_links: usize) -> Result<()> {
        let multiplier = self.handles.config.extensions.len().max(1);

        self.handles
            .stats
            .send(AddToUsizeField(LinksExtracted, num_links))?;
        self.handles
            .stats
            .send(AddToUsizeField(TotalExpected, num_links * multiplier))?;

        Ok(())
    }
}
