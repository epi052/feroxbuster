use super::FeroxFilter;
use super::SimilarityFilter;
use crate::event_handlers::Handles;
use crate::filters::similarity::SIM_HASHER;
use crate::nlp::preprocess;
use crate::response::FeroxResponse;
use crate::utils::{logged_request, parse_url_with_raw_path};
use crate::DEFAULT_METHOD;
use anyhow::Result;
use regex::Regex;
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
    let url = parse_url_with_raw_path(similarity_filter)?;

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

    let hash = SIM_HASHER.create_signature(preprocess(fr.text()).iter());

    Ok(SimilarityFilter {
        hash,
        original_url: similarity_filter.to_string(),
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
                hash: 0,
                original_url: filter_value.to_string(),
            }));
        }
        _ => (),
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Configuration;
    use crate::filters::{LinesFilter, RegexFilter, SizeFilter, StatusCodeFilter, WordsFilter};
    use crate::scan_manager::FeroxScans;
    use httpmock::Method::GET;
    use httpmock::MockServer;

    #[test]
    /// filter_lookup returns correct filters
    fn filter_lookup_returns_correct_filters() {
        let filter = filter_lookup("status", "200").unwrap();
        assert_eq!(
            filter.as_any().downcast_ref::<StatusCodeFilter>().unwrap(),
            &StatusCodeFilter { filter_code: 200 }
        );

        let filter = filter_lookup("lines", "10").unwrap();
        assert_eq!(
            filter.as_any().downcast_ref::<LinesFilter>().unwrap(),
            &LinesFilter { line_count: 10 }
        );

        let filter = filter_lookup("size", "20").unwrap();
        assert_eq!(
            filter.as_any().downcast_ref::<SizeFilter>().unwrap(),
            &SizeFilter { content_length: 20 }
        );

        let filter = filter_lookup("words", "30").unwrap();
        assert_eq!(
            filter.as_any().downcast_ref::<WordsFilter>().unwrap(),
            &WordsFilter { word_count: 30 }
        );

        let filter = filter_lookup("regex", "stuff.*").unwrap();
        let compiled = Regex::new("stuff.*").unwrap();
        let raw_string = String::from("stuff.*");
        assert_eq!(
            filter.as_any().downcast_ref::<RegexFilter>().unwrap(),
            &RegexFilter {
                compiled,
                raw_string
            }
        );

        let filter = filter_lookup("similarity", "http://localhost").unwrap();
        assert_eq!(
            filter.as_any().downcast_ref::<SimilarityFilter>().unwrap(),
            &SimilarityFilter {
                hash: 0,
                original_url: "http://localhost".to_string()
            }
        );

        assert!(filter_lookup("non-existent", "").is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// ensure create_similarity_filter correctness of return value and side-effects
    async fn create_similarity_filter_is_correct() {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200).body("this is a test");
        });

        let scans = FeroxScans::default();
        let config = Configuration {
            collect_extensions: true,
            ..Default::default()
        };

        let (test_handles, _) = Handles::for_testing(Some(Arc::new(scans)), Some(Arc::new(config)));

        let handles = Arc::new(test_handles);

        let filter = create_similarity_filter(&srv.url("/"), handles.clone())
            .await
            .unwrap();

        assert_eq!(mock.hits(), 1);

        assert_eq!(
            filter,
            SimilarityFilter {
                hash: 14897447612059286329,
                original_url: srv.url("/")
            }
        );
    }
}
