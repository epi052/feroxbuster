use std::sync::Arc;

use anyhow::{bail, Result};
use console::style;
use scraper::{Html, Selector};
use uuid::Uuid;

use crate::filters::{SimilarityFilter, SIM_HASHER};
use crate::message::FeroxMessage;
use crate::nlp::preprocess;
use crate::{
    config::OutputLevel,
    event_handlers::{Command, Handles},
    filters::{LinesFilter, SizeFilter, WordsFilter},
    progress::PROGRESS_PRINTER,
    response::FeroxResponse,
    skip_fail,
    url::FeroxUrl,
    utils::{ferox_print, fmt_err, logged_request, status_colorizer},
    DEFAULT_METHOD,
};

/// length of a standard UUID, used when determining wildcard responses
const UUID_LENGTH: u64 = 32;

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
            ids.push(Uuid::new_v4().as_simple().to_string());
        }

        let unique_id = ids.join("");

        log::trace!("exit: unique_string -> {}", unique_id);
        unique_id
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
                                &format!("Could not connect to {target_url} due to SSL errors (run with -k to ignore), skipping..."),
                                &PROGRESS_PRINTER,
                            );
                        } else {
                            ferox_print(
                                &format!("Could not connect to {target_url}, skipping..."),
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
            format!("{target_url}/")
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

    /// given a target's base url, attempt to automatically detect its 404 response
    /// pattern(s), and then set filters that will exclude those patterns from future
    /// responses
    pub async fn detect_404_like_responses(&self, target_url: &str) -> Result<u64> {
        log::trace!("enter: detect_404_like_responses({:?})", target_url);

        if self.handles.config.dont_filter {
            // early return, dont_filter scans don't need tested
            log::trace!("exit: detect_404_like_responses -> dont_filter is true");
            return Ok(0);
        }

        let mut req_counter = 0;

        let data = if self.handles.config.data.is_empty() {
            None
        } else {
            Some(self.handles.config.data.as_slice())
        };

        // To take care of slash when needed
        let slash = if self.handles.config.add_slash {
            Some("/")
        } else {
            None
        };

        // 4 is due to the array in the nested for loop below
        let mut responses = Vec::with_capacity(4);

        // for every method, attempt to id its 404 response
        //
        // a good example of one where the GET/POST differ is on hackthebox:
        // - http://prd.m.rendering-api.interface.htb/api
        for method in self.handles.config.methods.iter() {
            for (prefix, length) in [("", 1), ("", 3), (".htaccess", 1), ("admin", 1)] {
                let path = format!("{prefix}{}", self.unique_string(length));

                let ferox_url = FeroxUrl::from_string(target_url, self.handles.clone());

                let nonexistent_url = ferox_url.format(&path, slash)?;

                // example requests:
                // - http://localhost/2fc1077836ad43ab98b7a31c2ca28fea
                // - http://localhost/92969beae6bf4beb855d1622406d87e395c87387a9ad432e8a11245002b709b03cf609d471004154b83bcc1c6ec49f6f
                // - http://localhost/.htaccessa005a2131e68449aa26e99029c914c09
                // - http://localhost/adminf1d2541e73c44dcb9d1fb7d93334b280
                let response =
                    logged_request(&nonexistent_url, &method, data, self.handles.clone()).await;

                req_counter += 1;

                // continue to next on error
                let response = skip_fail!(response);

                if !self
                    .handles
                    .config
                    .status_codes
                    .contains(&response.status().as_u16())
                {
                    // if the response code isn't one that's accepted via -s values, then skip to the next
                    //
                    // the default value for -s is all status codes, so unless the user says otherwise
                    // this won't fire
                    continue;
                }

                let ferox_response = FeroxResponse::from(
                    response,
                    &ferox_url.target,
                    &method,
                    self.handles.config.output_level,
                )
                .await;

                responses.push(ferox_response);
            }

            if responses.len() < 2 {
                // don't have enough responses to make a determination, continue to next method
                responses.clear();
                continue;
            }

            // Command::AddFilter, &str (bytes/words/lines), usize (i.e. length associated with the type)
            let Some((command, filter_type, filter_length)) = self.examine_404_like_responses(&responses) else {
                // no match was found during analysis of responses
                responses.clear();
                continue;
            };

            // check whether we already know about this filter
            match command {
                Command::AddFilter(ref filter) => {
                    if let Ok(guard) = self.handles.filters.data.filters.read() {
                        if guard.contains(filter) {
                            // match was found, but already known; clear the vec and continue to the next
                            responses.clear();
                            continue;
                        }
                    }
                }
                _ => unreachable!(),
            }

            // create the new filter
            self.handles.filters.send(command)?;

            // if we're here, we've detected a 404-like response pattern, and we're already filtering for size/word/line
            //
            // in addition, we'll create a similarity filter as a fallback
            let hash = SIM_HASHER.create_signature(preprocess(responses[0].text()).iter());

            let sim_filter = SimilarityFilter {
                hash,
                original_url: responses[0].url().to_string(),
            };

            self.handles
                .filters
                .send(Command::AddFilter(Box::new(sim_filter)))?;

            // reset the responses for the next method, if it exists
            responses.clear();

            // report to the user, if appropriate
            if matches!(
                self.handles.config.output_level,
                OutputLevel::Default | OutputLevel::Quiet
            ) {
                let msg = format!("{} {:>8} {:>9} {:>9} {:>9} {} => {} {}-like response ({} {}); toggle this behavior by using {}\n", status_colorizer("WLD"), "-", "-", "-", "-", style(target_url).cyan(), style("auto-filtering").bright().green(), style("404").red(), style(filter_length).cyan(), filter_type, style("--dont-filter").yellow());
                ferox_print(&msg, &PROGRESS_PRINTER);
            }
        }

        log::trace!("exit: detect_404_like_responses");

        return Ok(req_counter);
    }

    /// for all responses, examine chars/words/lines
    /// if all responses respective lengths match each other, we can assume
    /// that will remain true for subsequent non-existent urls
    ///
    /// values are examined from most to least specific (content length, word count, line count)
    fn examine_404_like_responses(
        &self,
        responses: &[FeroxResponse],
    ) -> Option<(Command, &'static str, usize)> {
        let mut size_sentry = true;
        let mut word_sentry = true;
        let mut line_sentry = true;

        let content_length = responses[0].content_length();
        let word_count = responses[0].word_count();
        let line_count = responses[0].line_count();

        for response in &responses[1..] {
            // if any of the responses differ in length, that particular
            // response length type is no longer a candidate for filtering
            if response.content_length() != content_length {
                size_sentry = false;
            }

            if response.word_count() != word_count {
                word_sentry = false;
            }

            if response.line_count() != line_count {
                line_sentry = false;
            }
        }

        // the if/else-if/else nature of the block means that we'll get the most
        // specific match, if one is to be had
        //
        // each block returns the information needed to send the filter away and
        // display a message to the user
        if size_sentry {
            // - command to send to the filters handler
            // - the unit-type we're filtering on (bytes/words/lines)
            // - the value associated with the unit-type on which we're filtering
            Some((
                Command::AddFilter(Box::new(SizeFilter { content_length })),
                "bytes",
                content_length as usize,
            ))
        } else if word_sentry {
            Some((
                Command::AddFilter(Box::new(WordsFilter { word_count })),
                "words",
                word_count,
            ))
        } else if line_sentry {
            Some((
                Command::AddFilter(Box::new(LinesFilter { line_count })),
                "lines",
                line_count,
            ))
        } else {
            // no match was found; clear the vec and continue to the next
            None
        }
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
