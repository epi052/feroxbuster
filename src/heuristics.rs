use std::sync::Arc;

use anyhow::{bail, Result};
use console::style;
use scraper::{Html, Selector};
use uuid::Uuid;

use crate::message::FeroxMessage;
use crate::{
    config::OutputLevel,
    event_handlers::{Command, Handles},
    filters::WildcardFilter,
    progress::PROGRESS_PRINTER,
    response::FeroxResponse,
    skip_fail,
    url::FeroxUrl,
    utils::{ferox_print, fmt_err, logged_request, status_colorizer},
    DEFAULT_METHOD,
};

/// length of a standard UUID, used when determining wildcard responses
const UUID_LENGTH: u64 = 32;

/// wrapper around ugly string formatting
macro_rules! format_template {
    ($template:expr, $method:expr, $length:expr) => {
        format!(
            $template,
            status_colorizer("WLD"),
            $method,
            "-",
            "-",
            "-",
            style("auto-filtering").yellow(),
            style($length).cyan(),
            style("--dont-filter").yellow()
        )
    };
}

/// enum representing the different servers that `parse_html` can detect when directory listing is
/// enabled
#[derive(Copy, Debug, Clone)]
pub enum DirListingType {
    /// apache server, detected by `Index of /`
    Apache,

    /// tomcat/python server, detected by `Directory Listing for /`
    TomCatOrPython,

    /// ASP.NET server, detected by `Directory Listing -- /`
    AspDotNet,

    // /// IIS/Azure server, detected by `HOST_NAME - /` (not currently used)
    // IIS_AZURE,
    /// variant that represents the absence of directory listing
    None,
}

/// Wrapper around the results of running a directory listing detection against a target web page
#[derive(Debug, Clone)]
pub struct DirListingResult {
    /// type of server where directory listing was detected
    /// i.e. https://portswigger.net/kb/issues/00600100_directory-listing
    pub dir_list_type: Option<DirListingType>,

    /// the `FeroxResponse` generated during detection
    pub response: FeroxResponse,
}

/// container for heuristics related info
pub struct HeuristicTests {
    /// Handles object for event handler interaction
    handles: Arc<Handles>,
}

/// HeuristicTests implementation
impl HeuristicTests {
    /// create a new HeuristicTests struct
    pub fn new(handles: Arc<Handles>) -> Self {
        Self { handles }
    }

    /// Simple helper to return a uuid, formatted as lowercase without hyphens
    ///
    /// `length` determines the number of uuids to string together. Each uuid
    /// is 32 characters long. So, a length of 1 return a 32 character string,
    /// a length of 2 returns a 64 character string, and so on...
    fn unique_string(&self, length: usize) -> String {
        log::trace!("enter: unique_string({})", length);
        let mut ids = vec![];

        for _ in 0..length {
            ids.push(Uuid::new_v4().to_simple().to_string());
        }

        let unique_id = ids.join("");

        log::trace!("exit: unique_string -> {}", unique_id);
        unique_id
    }

    /// wrapper for sending a filter to the filters event handler
    fn send_filter(&self, filter: WildcardFilter) -> Result<()> {
        self.handles
            .filters
            .send(Command::AddFilter(Box::new(filter)))
    }

    /// Tests the given url to see if it issues a wildcard response
    ///
    /// In the event that url returns a wildcard response, a
    /// [WildcardFilter](struct.WildcardFilter.html) is created and sent to the filters event
    /// handler.
    ///
    /// Returns the number of times to increment the caller's progress bar
    pub async fn wildcard(&self, target_url: &str) -> Result<u64> {
        log::trace!("enter: wildcard_test({:?})", target_url);

        if self.handles.config.dont_filter {
            // early return, dont_filter scans don't need tested
            log::trace!("exit: wildcard_test -> 0");
            return Ok(0);
        }

        let data = match self.handles.config.data.is_empty() {
            true => None,
            false => Some(self.handles.config.data.as_slice()),
        };

        let ferox_url = FeroxUrl::from_string(target_url, self.handles.clone());

        for method in self.handles.config.methods.iter() {
            let ferox_response = self
                .make_wildcard_request(&ferox_url, method.as_str(), data, 1)
                .await?;

            // found a wildcard response
            let mut wildcard = WildcardFilter::new(self.handles.config.dont_filter);

            let wc_length = ferox_response.content_length();

            if wc_length == 0 {
                log::trace!("exit: wildcard_test -> 1");
                self.send_filter(wildcard)?;
                return Ok(1);
            }

            // content length of wildcard is non-zero, perform additional tests:
            //   make a second request, with a known-sized (64) longer request
            let resp_two = self
                .make_wildcard_request(&ferox_url, method.as_str(), data, 3)
                .await?;

            let wc2_length = resp_two.content_length();

            wildcard.method = resp_two.method().as_str().to_owned();

            if wc2_length == wc_length + (UUID_LENGTH * 2) {
                // second length is what we'd expect to see if the requested url is
                // reflected in the response along with some static content; aka custom 404
                let url_len = ferox_url.path_length()?;

                wildcard.dynamic = wc_length - url_len;

                if matches!(
                    self.handles.config.output_level,
                    OutputLevel::Default | OutputLevel::Quiet
                ) {
                    let msg = format_template!("{} {:>8} {:>9} {:>9} {:>9} Wildcard response is dynamic; {} ({} + url length) responses; toggle this behavior by using {}\n", method, wildcard.dynamic);
                    ferox_print(&msg, &PROGRESS_PRINTER);
                }
            } else if wc_length == wc2_length {
                wildcard.size = wc_length;

                if matches!(
                    self.handles.config.output_level,
                    OutputLevel::Default | OutputLevel::Quiet
                ) {
                    let msg = format_template!("{} {:>8} {:>9} {:>9} {:>9} Wildcard response is static; {} {} responses; toggle this behavior by using {}\n", method, wildcard.size);
                    ferox_print(&msg, &PROGRESS_PRINTER);
                }
            }

            self.send_filter(wildcard)?;
        }

        log::trace!("exit: wildcard_test");
        Ok(2)
    }

    /// Generates a uuid and appends it to the given target url. The reasoning is that the randomly
    /// generated unique string should not exist on and be served by the target web server.
    ///
    /// Once the unique url is created, the request is sent to the server. If the server responds
    /// back with a valid status code, the response is considered to be a wildcard response. If that
    /// wildcard response has a 3xx status code, that redirection location is displayed to the user.
    async fn make_wildcard_request(
        &self,
        target: &FeroxUrl,
        method: &str,
        data: Option<&[u8]>,
        length: usize,
    ) -> Result<FeroxResponse> {
        log::trace!("enter: make_wildcard_request({}, {})", target, length);

        let unique_str = self.unique_string(length);

        // To take care of slash when needed
        let slash = if self.handles.config.add_slash {
            Some("/")
        } else {
            None
        };

        let nonexistent_url = target.format(&unique_str, slash)?;

        let response = logged_request(
            &nonexistent_url.to_owned(),
            method,
            data,
            self.handles.clone(),
        )
        .await?;

        if self
            .handles
            .config
            .status_codes
            .contains(&response.status().as_u16())
        {
            // found a wildcard response

            let mut ferox_response = FeroxResponse::from(
                response,
                &target.target,
                method,
                self.handles.config.output_level,
            )
            .await;
            ferox_response.set_wildcard(true);

            if self
                .handles
                .filters
                .data
                .should_filter_response(&ferox_response, self.handles.stats.tx.clone())
            {
                bail!("filtered response")
            }

            if matches!(
                self.handles.config.output_level,
                OutputLevel::Default | OutputLevel::Quiet
            ) {
                let boxed = Box::new(ferox_response.clone());
                self.handles.output.send(Command::Report(boxed))?;
            }

            log::trace!("exit: make_wildcard_request -> {}", ferox_response);
            return Ok(ferox_response);
        }

        log::trace!("exit: make_wildcard_request -> Err");
        bail!("uninteresting status code")
    }

    /// Simply tries to connect to all given sites before starting to scan
    ///
    /// In the event that no sites can be reached, the program will exit.
    ///
    /// Any urls that are found to be alive are returned to the caller.
    pub async fn connectivity(&self, target_urls: &[String]) -> Result<Vec<String>> {
        log::trace!("enter: connectivity_test({:?})", target_urls);

        let mut good_urls = vec![];

        for target_url in target_urls {
            let url = FeroxUrl::from_string(target_url, self.handles.clone());
            let request = skip_fail!(url.format("", None));

            let result = logged_request(&request, DEFAULT_METHOD, None, self.handles.clone()).await;

            match result {
                Ok(_) => {
                    good_urls.push(target_url.to_owned());
                }
                Err(e) => {
                    if matches!(
                        self.handles.config.output_level,
                        OutputLevel::Default | OutputLevel::Quiet
                    ) {
                        if e.to_string().contains(":SSL") {
                            ferox_print(
                                &format!("Could not connect to {} due to SSL errors (run with -k to ignore), skipping...", target_url),
                                &PROGRESS_PRINTER,
                            );
                        } else {
                            ferox_print(
                                &format!("Could not connect to {}, skipping...", target_url),
                                &PROGRESS_PRINTER,
                            );
                        }
                    }
                    log::warn!("{}", e);
                }
            }
        }

        if good_urls.is_empty() {
            bail!("Could not connect to any target provided");
        }

        log::trace!("exit: connectivity_test -> {:?}", good_urls);
        Ok(good_urls)
    }

    /// heuristic designed to detect when a server has directory listing enabled
    pub async fn directory_listing(&self, target_url: &str) -> Result<Option<DirListingResult>> {
        log::trace!("enter: directory_listing({})", target_url);

        let tgt = if !target_url.ends_with('/') {
            // if left unchanged, this function would be called against redirects that point to
            // valid directories for most, if not all, directories beyond the initial urls.
            // so, instead of `directory_listing("http://localhost") -> None` we get
            // `directory_listing("http://localhost/") -> Some(DirListingResult)` if there is
            // directory listing beyond the redirect
            format!("{}/", target_url)
        } else {
            target_url.to_string()
        };

        let url = FeroxUrl::from_string(&tgt, self.handles.clone());
        let request = url.format("", None)?;

        let result = logged_request(&request, DEFAULT_METHOD, None, self.handles.clone()).await?;

        let ferox_response = FeroxResponse::from(
            result,
            &url.target,
            DEFAULT_METHOD,
            self.handles.config.output_level,
        )
        .await;

        let body = ferox_response.text();
        let html = Html::parse_document(body);

        let dirlist_type = self.detect_directory_listing(&html);

        if dirlist_type.is_some() {
            // folks that run things and step away/rely on logs need to be notified of directory
            // listing, since they won't see the message on the bar; bastardizing FeroxMessage
            // for ease of implementation. This could use a bit of polish at some point.
            let msg = format!(
                "detected directory listing: {} ({:?})",
                target_url,
                dirlist_type.unwrap()
            );
            let ferox_msg = FeroxMessage {
                kind: "log".to_string(),
                message: msg.clone(),
                level: "MSG".to_string(),
                time_offset: 0.0,
                module: "feroxbuster::heuristics".to_string(),
            };
            self.handles
                .output
                .tx_file
                .send(Command::WriteToDisk(Box::new(ferox_msg)))
                .unwrap_or_default();

            log::info!("{}", msg);

            let result = DirListingResult {
                dir_list_type: dirlist_type,
                response: ferox_response,
            };

            log::trace!("exit: directory_listing -> {:?}", result);
            return Ok(Some(result));
        }

        log::trace!("exit: directory_listing -> None");
        Ok(None)
    }

    /// Directory listing heuristic detection, uses <title> tag to make its determination. When
    /// the inner html of <title> matches one of the following, a `DirListingType` is returned.
    /// - apache: `Index of /`
    /// - tomcat/python: `Directory Listing for /`
    /// - ASP.NET: `Directory Listing -- /`
    /// - <host> - /: iis, azure, skipping due to loose heuristic
    fn detect_directory_listing(&self, html: &Html) -> Option<DirListingType> {
        log::trace!("enter: detect_directory_listing(html body...)");

        let title_selector = Selector::parse("title").expect("couldn't parse title selector");

        for t in html.select(&title_selector) {
            let title = t.inner_html().to_lowercase();

            let dirlist_type = if title.contains("directory listing for /") {
                Some(DirListingType::TomCatOrPython)
            } else if title.contains("index of /") {
                Some(DirListingType::Apache)
            } else if title.contains("directory listing -- /") {
                Some(DirListingType::AspDotNet)
            } else {
                // IIS_AZURE purposely skipped for now
                None
            };

            if dirlist_type.is_some() {
                log::trace!("exit: detect_directory_listing -> {:?}", dirlist_type);
                return dirlist_type;
            }
        }

        log::trace!("exit: detect_directory_listing -> None");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// request a unique string of 32bytes * a value returns correct result
    fn heuristics_unique_string_returns_correct_length() {
        let (handles, _) = Handles::for_testing(None, None);
        let tester = HeuristicTests::new(Arc::new(handles));
        for i in 0..10 {
            assert_eq!(tester.unique_string(i).len(), i * 32);
        }
    }

    #[test]
    /// `detect_directory_listing` correctly identifies tomcat/python instances
    fn detect_directory_listing_finds_tomcat_python() {
        let html = "<title>directory listing for /</title>";
        let parsed = Html::parse_document(html);
        let handles = Handles::for_testing(None, None);
        let heuristics = HeuristicTests::new(Arc::new(handles.0));
        let dirlist_type = heuristics.detect_directory_listing(&parsed);
        assert!(matches!(
            dirlist_type.unwrap(),
            DirListingType::TomCatOrPython
        ));
    }

    #[test]
    /// `detect_directory_listing` correctly identifies apache instances
    fn detect_directory_listing_finds_apache() {
        let html = "<title>index of /</title>";
        let parsed = Html::parse_document(html);
        let handles = Handles::for_testing(None, None);
        let heuristics = HeuristicTests::new(Arc::new(handles.0));
        let dirlist_type = heuristics.detect_directory_listing(&parsed);
        assert!(matches!(dirlist_type.unwrap(), DirListingType::Apache));
    }

    #[test]
    /// `detect_directory_listing` correctly identifies ASP.NET instances
    fn detect_directory_listing_finds_asp_dot_net() {
        let html = "<title>directory listing -- /</title>";
        let parsed = Html::parse_document(html);
        let handles = Handles::for_testing(None, None);
        let heuristics = HeuristicTests::new(Arc::new(handles.0));
        let dirlist_type = heuristics.detect_directory_listing(&parsed);
        assert!(matches!(dirlist_type.unwrap(), DirListingType::AspDotNet));
    }

    #[test]
    /// `detect_directory_listing` returns None when heuristic doesn't match
    fn detect_directory_listing_returns_none_as_default() {
        let html = "<title>derp listing -- /</title>";
        let parsed = Html::parse_document(html);
        let handles = Handles::for_testing(None, None);
        let heuristics = HeuristicTests::new(Arc::new(handles.0));
        let dirlist_type = heuristics.detect_directory_listing(&parsed);
        assert!(dirlist_type.is_none());
    }
}
