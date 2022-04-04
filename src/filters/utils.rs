use super::FeroxFilter;
use super::SimilarityFilter;
use crate::event_handlers::Handles;
use crate::response::FeroxResponse;
use crate::utils::logged_request;
use crate::{DEFAULT_METHOD, SIMILARITY_THRESHOLD};
use anyhow::Result;
use fuzzyhash::FuzzyHash;
use regex::Regex;
use reqwest::Url;
use std::sync::Arc;

/// wrapper around logic necessary to create a SimilarityFilter
///
/// - parses given url
/// - makes request to the parsed url
/// - gathers extensions from the url, if configured to do so
/// - computes hash of response body
/// - creates filter with hash
pub(crate) async fn create_similarity_filter(
    similarity_filter: &str,
    handles: Arc<Handles>,
) -> Result<SimilarityFilter> {
    // url as-is based on input, ignores user-specified url manipulation options (add-slash etc)
    let url = Url::parse(similarity_filter)?;

    // attempt to request the given url
    let resp = logged_request(&url, DEFAULT_METHOD, None, handles.clone()).await?;

    // if successful, create a filter based on the response's body
    let mut fr = FeroxResponse::from(
        resp,
        similarity_filter,
        DEFAULT_METHOD,
        handles.config.output_level,
    )
    .await;

    if handles.config.collect_extensions {
        fr.parse_extension(handles.clone())?;
    }

    // hash the response body and store the resulting hash in the filter object
    let hash = FuzzyHash::new(&fr.text()).to_string();

    Ok(SimilarityFilter {
        hash,
        threshold: SIMILARITY_THRESHOLD,
    })
}

/// used in conjunction with the Scan Management Menu
///
/// when a user uses the n[ew-filter] command in the menu, the two params are passed here for
/// processing.
///
/// an example command may be `new-filter lines 40`. `lines` and `40` are passed here as &str's
///
/// once here, the type and value are used to create an appropriate FeroxFilter. If anything
/// goes wrong during creation, None is returned.
pub(crate) fn filter_lookup(filter_type: &str, filter_value: &str) -> Option<Box<dyn FeroxFilter>> {
    match filter_type {
        "status" => {
            if let Ok(parsed) = filter_value.parse() {
                return Some(Box::new(super::StatusCodeFilter {
                    filter_code: parsed,
                }));
            }
        }
        "lines" => {
            if let Ok(parsed) = filter_value.parse() {
                return Some(Box::new(super::LinesFilter { line_count: parsed }));
            }
        }
        "size" => {
            if let Ok(parsed) = filter_value.parse() {
                return Some(Box::new(super::SizeFilter {
                    content_length: parsed,
                }));
            }
        }
        "words" => {
            if let Ok(parsed) = filter_value.parse() {
                return Some(Box::new(super::WordsFilter { word_count: parsed }));
            }
        }
        "regex" => {
            if let Ok(parsed) = Regex::new(filter_value) {
                return Some(Box::new(super::RegexFilter {
                    compiled: parsed,
                    raw_string: filter_value.to_string(),
                }));
            }
        }
        "similarity" => {
            return Some(Box::new(SimilarityFilter {
                // bastardizing the hash field to pass back a url, this means that a caller
                // of this function needs to turn the url into a hash. This is a workaround for
                // wanting to call this this function from menu.rs but not having access to
                // a Handles instance. So, we pass things back up into ferox_scanner.rs and use the
                // Handles there to make the actual request.
                hash: filter_value.to_string(),
                threshold: SIMILARITY_THRESHOLD,
            }));
        }
        _ => (),
    }

    None
}
