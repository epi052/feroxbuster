use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use futures::future;
use scraper::{Html, Selector};
use uuid::Uuid;

use crate::filters::{SimilarityFilter, WildcardFilter, SIM_HASHER};
use crate::message::FeroxMessage;
use crate::nlp::preprocess;
use crate::scanner::RESPONSES;
use crate::{
    config::OutputLevel,
    event_handlers::{Command, Handles},
    progress::PROGRESS_PRINTER,
    response::FeroxResponse,
    skip_fail,
    url::FeroxUrl,
    utils::{ferox_print, fmt_err, logged_request},
    DEFAULT_METHOD,
};

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

/// wrapper around the results of running a wildcard detection against a target web page
#[derive(Copy, Debug, Clone)]
pub enum WildcardResult {
    /// variant that represents a wildcard directory
    WildcardDirectory(usize),

    /// variant that represents the presence of a 404-like response
    FourOhFourLike(usize),
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
                                &format!("Could not connect to {target_url} due to SSL errors (run with -k to ignore), skipping...\n  => {}\n", e.root_cause()),
                                &PROGRESS_PRINTER,
                            );
                        } else {
                            ferox_print(
                                &format!(
                                    "Could not connect to {target_url}, skipping...\n  => {}\n",
                                    e.root_cause()
                                ),
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
    pub async fn detect_404_like_responses(
        &self,
        target_url: &str,
    ) -> Result<Option<WildcardResult>> {
        log::trace!("enter: detect_404_like_responses({:?})", target_url);

        if self.handles.config.dont_filter {
            // early return, dont_filter scans don't need tested
            log::trace!("exit: detect_404_like_responses -> dont_filter is true");
            return Ok(None);
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

        // no matter what, we want an empty extension for the base case
        let mut extensions = vec!["".to_string()];

        // and then we want to add any extensions that was specified
        // or has since been added to the running config
        for ext in &self.handles.config.extensions {
            extensions.push(format!(".{}", ext));
        }

        // for every method, attempt to id its 404 response
        //
        // a good example of one where the GET/POST differ is on hackthebox:
        // - http://prd.m.rendering-api.interface.htb/api
        //
        // a good example of one where the heuristics return a 403 and a 404 (apache)
        // as well as return two different types of 404s based on the file extension
        // - http://10.10.11.198 (Encoding box in normal labs)
        //
        // both methods and extensions can elicit different responses from a given
        // server, so both are considered when building auto-filter rules
        for method in self.handles.config.methods.iter() {
            for extension in extensions.iter() {
                // build out the 6 paths we'll use
                let paths = [
                    ("", 1),
                    ("", 3),
                    (".htaccess", 1),
                    (".htaccess", 3),
                    ("admin", 1),
                    ("admin", 3),
                ]
                .map(|(prefix, length)| {
                    format!("{prefix}{}{extension}", self.unique_string(length))
                });

                // allow all 6 requests to fly asynchronously
                let responses = future::join_all(paths.into_iter().map(|path| async move {
                    let ferox_url = FeroxUrl::from_string(target_url, self.handles.clone());

                    let Ok(nonexistent_url) = ferox_url.format(&path, slash) else {
                        return None;
                    };

                    // example requests:
                    // - http://localhost/2fc1077836ad43ab98b7a31c2ca28fea
                    // - http://localhost/92969beae6bf4beb855d1622406d87e395c87387a9ad432e8a11245002b709b03cf609d471004154b83bcc1c6ec49f6f
                    // - http://localhost/.htaccessa005a2131e68449aa26e99029c914c09
                    // - http://localhost/.htaccess92969beae6bf4beb855d1622406d87e395c87387a9ad432e8a11245002b709b03cf609d471004154b83bcc1c6ec49f6f
                    // - http://localhost/adminf1d2541e73c44dcb9d1fb7d93334b280
                    // - http://localhost/admin92969beae6bf4beb855d1622406d87e395c87387a9ad432e8a11245002b709b03cf609d471004154b83bcc1c6ec49f6f
                    let Ok(response) =
                        logged_request(&nonexistent_url, method, data, self.handles.clone()).await
                    else {
                        return None;
                    };

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
                        return None;
                    }

                    Some(
                        FeroxResponse::from(
                            response,
                            &ferox_url.target,
                            method,
                            self.handles.config.output_level,
                        )
                        .await,
                    )
                }))
                .await // await gives vector of options containing feroxresponses
                .into_iter()
                .flatten() // strip out the none values
                .collect::<Vec<_>>();

                if responses.len() < 2 {
                    // don't have enough responses to make a determination, continue to next method
                    log::debug!("not enough responses to make a determination");
                    continue;
                }

                // check the responses for similarities on which we can filter, multiple may be returned
                let Some((wildcard_filters, wildcard_responses)) =
                    self.examine_404_like_responses(&responses)
                else {
                    // no match was found during analysis of responses
                    log::warn!("no match found for 404 responses");
                    continue;
                };

                // report to the user, if appropriate
                if matches!(
                    self.handles.config.output_level,
                    OutputLevel::Default | OutputLevel::Quiet
                ) {
                    // sentry value to control whether or not to print the filter
                    // used because we only want to print the same filter once
                    let mut print_sentry;

                    if let Ok(filters) = self.handles.filters.data.filters.read() {
                        for new_wildcard in &wildcard_filters {
                            // reset the sentry for every new wildcard produced by examine_404_like_responses
                            print_sentry = true;

                            for other in filters.iter() {
                                if let Some(other_wildcard) =
                                    other.as_any().downcast_ref::<WildcardFilter>()
                                {
                                    // check the new wildcard against all existing wildcards, if it was added
                                    // on the cli or by a previous directory, don't print it
                                    if new_wildcard.as_ref() == other_wildcard {
                                        print_sentry = false;
                                        break;
                                    }
                                }
                            }

                            // if we're here, we've found a new wildcard that we didn't previously display, print it
                            if print_sentry {
                                ferox_print(&format!("{}", new_wildcard), &PROGRESS_PRINTER);
                            }
                        }
                    }
                }

                // create the new filter
                for wildcard in wildcard_filters {
                    self.handles.filters.send(Command::AddFilter(wildcard))?;
                }

                // if we're here, we've detected a 404-like response pattern, and we're already filtering for size/word/line
                //
                // in addition, we'll create a similarity filter as a fallback
                for resp in wildcard_responses {
                    let hash = SIM_HASHER.create_signature(preprocess(resp.text()).iter());

                    let sim_filter = SimilarityFilter {
                        hash,
                        original_url: resp.url().to_string(),
                    };

                    self.handles
                        .filters
                        .send(Command::AddFilter(Box::new(sim_filter)))?;

                    if resp.is_directory() {
                        // response is either a 3XX with a Location header that matches url + '/'
                        // or it's a 2XX that ends with a '/'
                        // or it's a 403 that ends with a '/'

                        // set the wildcard flag to true, so we can check it when preventing
                        // recursion in event_handlers/scans.rs

                        // we'd need to clone the response to give ownership to the global list anyway
                        // so we'll also use that clone to set the wildcard flag
                        let mut cloned_resp = resp.clone();

                        cloned_resp.set_wildcard(true);

                        // add the response to the global list of responses
                        RESPONSES.insert(cloned_resp);

                        // function-internal magic number, indicates that we've detected a wildcard directory
                        req_counter += 100;
                    }
                }
            }
        }

        log::trace!("exit: detect_404_like_responses");

        let retval = if req_counter >= 100 {
            WildcardResult::WildcardDirectory(req_counter)
        } else {
            WildcardResult::FourOhFourLike(req_counter)
        };

        Ok(Some(retval))
    }

    /// for all responses, group them by status code, then examine chars/words/lines.
    /// if all responses' respective lengths within a status code grouping match
    /// each other, we can assume that will remain true for subsequent non-existent urls
    ///
    /// within a status code grouping, values are examined from most to
    /// least specific (content length, word count, line count)
    #[allow(clippy::vec_box)] // the box is needed in the caller and i dont feel like changing it
    fn examine_404_like_responses<'a>(
        &self,
        responses: &'a [FeroxResponse],
    ) -> Option<(Vec<Box<WildcardFilter>>, Vec<&'a FeroxResponse>)> {
        // aside from word/line/byte counts, additional discriminators are status code
        // extension, and request method. The request method and extension are handled by
        // the caller, since they're part of the request and make up the nested for loops
        // in detect_404_like_responses.
        //
        // The status code is handled here, since it's part of the response to catch cases
        // where we have something like a 403 and a 404

        let mut size_sentry = true;
        let mut word_sentry = true;
        let mut line_sentry = true;

        // returned vec of boxed wildcard filters
        let mut wildcards = Vec::new();

        // returned vec of ferox responses that are needed for additional
        // analysis
        let mut wild_responses = Vec::new();

        // mapping of grouped responses to status code
        let mut grouped_responses = HashMap::new();

        // iterate over all responses and add each response to its
        // corresponding status code group
        for response in responses {
            grouped_responses
                .entry(response.status())
                .or_insert_with(Vec::new)
                .push(response);
        }

        // iterate over each grouped response and determine the most specific
        // filter that can be applied to all responses in the group, i.e.
        // start from byte count and work 'out' to line count
        for response_group in grouped_responses.values() {
            if response_group.len() < 2 {
                // not enough responses to make a determination
                continue;
            }

            let method = response_group[0].method();
            let status_code = response_group[0].status();
            let content_length = response_group[0].content_length();
            let word_count = response_group[0].word_count();
            let line_count = response_group[0].line_count();

            for response in &response_group[1..] {
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

            if !size_sentry && !word_sentry && !line_sentry {
                // none of the response lengths match, so we can't filter on any of them
                continue;
            }

            let mut wildcard = WildcardFilter {
                content_length: None,
                line_count: None,
                word_count: None,
                method: method.to_string(),
                status_code: status_code.as_u16(),
                dont_filter: self.handles.config.dont_filter,
            };

            match (size_sentry, word_sentry, line_sentry) {
                (true, true, true) => {
                    // all three types of length match, so we can't filter on any of them
                    wildcard.content_length = Some(content_length);
                    wildcard.word_count = Some(word_count);
                    wildcard.line_count = Some(line_count);
                }
                (true, true, false) => {
                    // content length and word count match, so we can filter on either
                    wildcard.content_length = Some(content_length);
                    wildcard.word_count = Some(word_count);
                }
                (true, false, true) => {
                    // content length and line count match, so we can filter on either
                    wildcard.content_length = Some(content_length);
                    wildcard.line_count = Some(line_count);
                }
                (false, true, true) => {
                    // word count and line count match, so we can filter on either
                    wildcard.word_count = Some(word_count);
                    wildcard.line_count = Some(line_count);
                }
                (true, false, false) => {
                    // content length matches, so we can filter on that
                    wildcard.content_length = Some(content_length);
                }
                (false, true, false) => {
                    // word count matches, so we can filter on that
                    wildcard.word_count = Some(word_count);
                }
                (false, false, true) => {
                    // line count matches, so we can filter on that
                    wildcard.line_count = Some(line_count);
                }
                (false, false, false) => {
                    // none of the length types match, so we can't filter on any of them
                    unreachable!("no wildcard size matches; handled by the if statement above");
                }
            };

            wild_responses.push(response_group[0]);
            wildcards.push(Box::new(wildcard));
        }

        Some((wildcards, wild_responses))
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
