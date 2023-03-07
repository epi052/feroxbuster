use console::style;

use super::*;
use crate::utils::create_report_string;
use crate::{config::OutputLevel, DEFAULT_METHOD};

/// Data holder for all relevant data needed when auto-filtering out wildcard responses
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WildcardFilter {
    /// The content-length of this response, if known
    pub content_length: Option<u64>,

    /// The number of lines contained in the body of this response, if known
    pub line_count: Option<usize>,

    /// The number of words contained in the body of this response, if known
    pub word_count: Option<usize>,

    /// method used in request that should be included with filters passed via runtime configuration
    pub method: String,

    /// the status code returned in the response
    pub status_code: u16,

    /// whether or not the user passed -D on the command line
    pub dont_filter: bool,
}

/// implementation of WildcardFilter
impl WildcardFilter {
    /// given a boolean representing whether -D was used or not, create a new WildcardFilter
    pub fn new(dont_filter: bool) -> Self {
        Self {
            dont_filter,
            ..Default::default()
        }
    }
}

/// implement default that populates `method` with its default value
impl Default for WildcardFilter {
    fn default() -> Self {
        Self {
            content_length: None,
            line_count: None,
            word_count: None,
            method: DEFAULT_METHOD.to_string(),
            status_code: 0,
            dont_filter: false,
        }
    }
}

/// implementation of FeroxFilter for WildcardFilter
impl FeroxFilter for WildcardFilter {
    /// Examine size/words/lines and method to determine whether or not the response received
    /// is a wildcard response and therefore should be filtered out
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        // quick return if dont_filter is set
        if self.dont_filter {
            // --dont-filter applies specifically to wildcard filters, it is not a 100% catch all
            // for not filtering anything.  As such, it should live in the implementation of
            // a wildcard filter
            return false;
        }

        if self.method != response.method().as_str() {
            // method's don't match, so this response should not be filtered out
            log::trace!("exit: should_filter_response -> false");
            return false;
        }

        if self.status_code != response.status().as_u16() {
            // status codes don't match, so this response should not be filtered out
            log::trace!("exit: should_filter_response -> false");
            return false;
        }

        // methods and status codes match at this point, just need to check the other fields

        match (self.content_length, self.word_count, self.line_count) {
            (Some(cl), Some(wc), Some(lc)) => {
                if cl == response.content_length()
                    && wc == response.word_count()
                    && lc == response.line_count()
                {
                    log::debug!("filtered out {}", response.url());
                    log::trace!("exit: should_filter_response -> true");
                    return true;
                }
            }
            (Some(cl), Some(wc), None) => {
                if cl == response.content_length() && wc == response.word_count() {
                    log::debug!("filtered out {}", response.url());
                    log::trace!("exit: should_filter_response -> true");
                    return true;
                }
            }
            (Some(cl), None, Some(lc)) => {
                if cl == response.content_length() && lc == response.line_count() {
                    log::debug!("filtered out {}", response.url());
                    log::trace!("exit: should_filter_response -> true");
                    return true;
                }
            }
            (None, Some(wc), Some(lc)) => {
                if wc == response.word_count() && lc == response.line_count() {
                    log::debug!("filtered out {}", response.url());
                    log::trace!("exit: should_filter_response -> true");
                    return true;
                }
            }
            (Some(cl), None, None) => {
                if cl == response.content_length() {
                    log::debug!("filtered out {}", response.url());
                    log::trace!("exit: should_filter_response -> true");
                    return true;
                }
            }
            (None, Some(wc), None) => {
                if wc == response.word_count() {
                    log::debug!("filtered out {}", response.url());
                    log::trace!("exit: should_filter_response -> true");
                    return true;
                }
            }
            (None, None, Some(lc)) => {
                if lc == response.line_count() {
                    log::debug!("filtered out {}", response.url());
                    log::trace!("exit: should_filter_response -> true");
                    return true;
                }
            }
            (None, None, None) => {
                unreachable!("wildcard filter without any filters set");
            }
        }

        log::trace!("exit: should_filter_response -> false");
        false
    }

    /// Compare one WildcardFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl std::fmt::Display for WildcardFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = create_report_string(
            self.status_code.to_string().as_str(),
            self.method.as_str(),
            &self
                .line_count
                .map_or_else(|| "-".to_string(), |x| x.to_string()),
            &self
                .word_count
                .map_or_else(|| "-".to_string(), |x| x.to_string()),
            &self
                .content_length
                .map_or_else(|| "-".to_string(), |x| x.to_string()),
            &format!(
                "{} found {}-like response and created new filter; toggle off with {}",
                style("Auto-filtering").bright().green(),
                style("404").red(),
                style("--dont-filter").yellow()
            ),
            OutputLevel::Default,
        );
        write!(f, "{}", msg)
    }
}
