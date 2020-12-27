use crate::config::CONFIGURATION;
use crate::utils::get_url_path_length;
use crate::{FeroxResponse, FeroxSerialize};
use fuzzyhash::FuzzyHash;
use regex::Regex;
use std::any::Any;
use std::fmt::Debug;

// references:
//   https://dev.to/magnusstrale/rust-trait-objects-in-a-vector-non-trivial-4co5
//   https://stackoverflow.com/questions/25339603/how-to-test-for-equality-between-trait-objects

/// FeroxFilter trait; represents different types of possible filters that can be applied to
/// responses
pub trait FeroxFilter: Debug + Send + Sync {
    /// Determine whether or not this particular filter should be applied or not
    fn should_filter_response(&self, response: &FeroxResponse) -> bool;

    /// delegates to the FeroxFilter-implementing type which gives us the actual type of self
    fn box_eq(&self, other: &dyn Any) -> bool;

    /// gives us `other` as Any in box_eq
    fn as_any(&self) -> &dyn Any;
}

/// implementation of PartialEq, necessary long-form due to "trait cannot be made into an object"
/// error when attempting to derive PartialEq on the trait itself
impl PartialEq for Box<dyn FeroxFilter> {
    /// Perform a comparison of two implementors of the FeroxFilter trait
    fn eq(&self, other: &Box<dyn FeroxFilter>) -> bool {
        self.box_eq(other.as_any())
    }
}

/// Data holder for two pieces of data needed when auto-filtering out wildcard responses
///
/// `dynamic` is the size of the response that will later be combined with the length
/// of the path of the url requested and used to determine interesting pages from custom
/// 404s where the requested url is reflected back in the response
///
/// `size` is size of the response that should be included with filters passed via runtime
/// configuration and any static wildcard lengths.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct WildcardFilter {
    /// size of the response that will later be combined with the length of the path of the url
    /// requested
    pub dynamic: u64,

    /// size of the response that should be included with filters passed via runtime configuration
    pub size: u64,
}

/// implementation of FeroxFilter for WildcardFilter
impl FeroxFilter for WildcardFilter {
    /// Examine size, dynamic, and content_len to determine whether or not the response received
    /// is a wildcard response and therefore should be filtered out
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        // quick return if dont_filter is set
        if CONFIGURATION.dont_filter {
            // --dont-filter applies specifically to wildcard filters, it is not a 100% catch all
            // for not filtering anything.  As such, it should live in the implementation of
            // a wildcard filter
            return false;
        }

        if self.size > 0 && self.size == response.content_length() {
            // static wildcard size found during testing
            // size isn't default, size equals response length, and auto-filter is on
            log::debug!("static wildcard: filtered out {}", response.url());
            log::trace!("exit: should_filter_response -> true");
            return true;
        }

        if self.dynamic > 0 {
            // dynamic wildcard offset found during testing

            // I'm about to manually split this url path instead of using reqwest::Url's
            // builtin parsing. The reason is that they call .split() on the url path
            // except that I don't want an empty string taking up the last index in the
            // event that the url ends with a forward slash.  It's ugly enough to be split
            // into its own function for readability.
            let url_len = get_url_path_length(&response.url());

            if url_len + self.dynamic == response.content_length() {
                log::debug!("dynamic wildcard: filtered out {}", response.url());
                log::trace!("exit: should_filter_response -> true");
                return true;
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

/// Simple implementor of FeroxFilter; used to filter out status codes specified using
/// -C|--filter-status
#[derive(Default, Debug, PartialEq)]
pub struct StatusCodeFilter {
    /// Status code that should not be displayed to the user
    pub filter_code: u16,
}

/// implementation of FeroxFilter for StatusCodeFilter
impl FeroxFilter for StatusCodeFilter {
    /// Check `filter_code` against what was passed in via -C|--filter-status
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        if response.status().as_u16() == self.filter_code {
            log::debug!(
                "filtered out {} based on --filter-status of {}",
                response.url(),
                self.filter_code
            );
            log::trace!("exit: should_filter_response -> true");
            return true;
        }

        log::trace!("exit: should_filter_response -> false");
        false
    }

    /// Compare one StatusCodeFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Simple implementor of FeroxFilter; used to filter out responses based on the number of lines
/// in a Response body; specified using -N|--filter-lines
#[derive(Default, Debug, PartialEq)]
pub struct LinesFilter {
    /// Number of lines in a Response's body that should be filtered
    pub line_count: usize,
}

/// implementation of FeroxFilter for LinesFilter
impl FeroxFilter for LinesFilter {
    /// Check `line_count` against what was passed in via -N|--filter-lines
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        let result = response.line_count() == self.line_count;

        log::trace!("exit: should_filter_response -> {}", result);

        result
    }

    /// Compare one LinesFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Simple implementor of FeroxFilter; used to filter out responses based on the number of words
/// in a Response body; specified using -W|--filter-words
#[derive(Default, Debug, PartialEq)]
pub struct WordsFilter {
    /// Number of words in a Response's body that should be filtered
    pub word_count: usize,
}

/// implementation of FeroxFilter for WordsFilter
impl FeroxFilter for WordsFilter {
    /// Check `word_count` against what was passed in via -W|--filter-words
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        let result = response.word_count() == self.word_count;

        log::trace!("exit: should_filter_response -> {}", result);

        result
    }

    /// Compare one WordsFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Simple implementor of FeroxFilter; used to filter out responses based on the length of a
/// Response body; specified using -S|--filter-size
#[derive(Default, Debug, PartialEq)]
pub struct SizeFilter {
    /// Overall length of a Response's body that should be filtered
    pub content_length: u64,
}

/// implementation of FeroxFilter for SizeFilter
impl FeroxFilter for SizeFilter {
    /// Check `content_length` against what was passed in via -S|--filter-size
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        let result = response.content_length() == self.content_length;

        log::trace!("exit: should_filter_response -> {}", result);

        result
    }

    /// Compare one SizeFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Simple implementor of FeroxFilter; used to filter out responses based on a given regular
/// expression; specified using -X|--filter-regex
#[derive(Debug)]
pub struct RegexFilter {
    /// Regular expression to be applied to the response body for filtering, compiled
    pub compiled: Regex,

    /// Regular expression as passed in on the command line, not compiled
    pub raw_string: String,
}

/// implementation of FeroxFilter for RegexFilter
impl FeroxFilter for RegexFilter {
    /// Check `expression` against the response body, if the expression matches, the response
    /// should be filtered out
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        let result = self.compiled.is_match(response.text());

        log::trace!("exit: should_filter_response -> {}", result);

        result
    }

    /// Compare one SizeFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// PartialEq implementation for RegexFilter
impl PartialEq for RegexFilter {
    /// Simple comparison of the raw string passed in via the command line
    fn eq(&self, other: &RegexFilter) -> bool {
        self.raw_string == other.raw_string
    }
}

/// Simple implementor of FeroxFilter; used to filter out responses based on the similarity of a
/// Response body with a known response; specified using --filter-similar-to
#[derive(Default, Debug, PartialEq)]
pub struct SimilarityFilter {
    /// Response's body to be used for comparison for similarity
    pub text: String,

    /// Percentage of similarity at which a page is determined to be a near-duplicate of another
    pub threshold: u32,
}

/// implementation of FeroxFilter for SimilarityFilter
impl FeroxFilter for SimilarityFilter {
    /// Check `FeroxResponse::text` against what was requested from the site passed in via
    /// --filter-similar-to
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        let other = FuzzyHash::new(&response.text);

        if let Ok(result) = FuzzyHash::compare(&self.text, &other.to_string()) {
            return result >= self.threshold;
        }

        // couldn't hash the response, don't filter
        log::warn!("Could not hash body from {}", response.as_str());
        false
    }

    /// Compare one SizeFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Url;

    #[test]
    /// just a simple test to increase code coverage by hitting as_any and the inner value
    fn lines_filter_as_any() {
        let filter = LinesFilter { line_count: 1 };

        assert_eq!(filter.line_count, 1);
        assert_eq!(
            *filter.as_any().downcast_ref::<LinesFilter>().unwrap(),
            filter
        );
    }

    #[test]
    /// just a simple test to increase code coverage by hitting as_any and the inner value
    fn words_filter_as_any() {
        let filter = WordsFilter { word_count: 1 };

        assert_eq!(filter.word_count, 1);
        assert_eq!(
            *filter.as_any().downcast_ref::<WordsFilter>().unwrap(),
            filter
        );
    }

    #[test]
    /// just a simple test to increase code coverage by hitting as_any and the inner value
    fn size_filter_as_any() {
        let filter = SizeFilter { content_length: 1 };

        assert_eq!(filter.content_length, 1);
        assert_eq!(
            *filter.as_any().downcast_ref::<SizeFilter>().unwrap(),
            filter
        );
    }

    #[test]
    /// just a simple test to increase code coverage by hitting as_any and the inner value
    fn status_code_filter_as_any() {
        let filter = StatusCodeFilter { filter_code: 200 };

        assert_eq!(filter.filter_code, 200);
        assert_eq!(
            *filter.as_any().downcast_ref::<StatusCodeFilter>().unwrap(),
            filter
        );
    }

    #[test]
    /// just a simple test to increase code coverage by hitting as_any and the inner value
    fn regex_filter_as_any() {
        let raw = r".*\.txt$";
        let compiled = Regex::new(raw).unwrap();
        let filter = RegexFilter {
            compiled,
            raw_string: raw.to_string(),
        };

        assert_eq!(filter.raw_string, r".*\.txt$");
        assert_eq!(
            *filter.as_any().downcast_ref::<RegexFilter>().unwrap(),
            filter
        );
    }

    #[test]
    /// test should_filter on WilcardFilter where static logic matches
    fn wildcard_should_filter_when_static_wildcard_found() {
        let resp = FeroxResponse {
            text: String::new(),
            wildcard: true,
            url: Url::parse("http://localhost").unwrap(),
            content_length: 100,
            word_count: 50,
            line_count: 25,
            headers: reqwest::header::HeaderMap::new(),
            status: reqwest::StatusCode::OK,
        };

        let filter = WildcardFilter {
            size: 100,
            dynamic: 0,
        };

        assert!(filter.should_filter_response(&resp));
    }

    #[test]
    /// test should_filter on WilcardFilter where dynamic logic matches
    fn wildcard_should_filter_when_dynamic_wildcard_found() {
        let resp = FeroxResponse {
            text: String::new(),
            wildcard: true,
            url: Url::parse("http://localhost/stuff").unwrap(),
            content_length: 100,
            word_count: 50,
            line_count: 25,
            headers: reqwest::header::HeaderMap::new(),
            status: reqwest::StatusCode::OK,
        };

        let filter = WildcardFilter {
            size: 0,
            dynamic: 95,
        };

        assert!(filter.should_filter_response(&resp));
    }

    #[test]
    /// test should_filter on RegexFilter where regex matches body
    fn regexfilter_should_filter_when_regex_matches_on_response_body() {
        let resp = FeroxResponse {
            text: String::from("im a body response hurr durr!"),
            wildcard: false,
            url: Url::parse("http://localhost/stuff").unwrap(),
            content_length: 100,
            word_count: 50,
            line_count: 25,
            headers: reqwest::header::HeaderMap::new(),
            status: reqwest::StatusCode::OK,
        };

        let raw = r"response...rr";

        let filter = RegexFilter {
            raw_string: raw.to_string(),
            compiled: Regex::new(raw).unwrap(),
        };

        assert!(filter.should_filter_response(&resp));
    }

    #[test]
    /// simple test for similarity filter, taken from strsim docs
    fn similarity_filter_is_accurate() {
        let mut resp = FeroxResponse {
            text: String::from("sitting"),
            wildcard: false,
            url: Url::parse("http://localhost/stuff").unwrap(),
            content_length: 100,
            word_count: 50,
            line_count: 25,
            headers: reqwest::header::HeaderMap::new(),
            status: reqwest::StatusCode::OK,
        };

        let mut filter = SimilarityFilter {
            text: String::from("kitten"),
            threshold: 95,
        };

        // assert!((normalized_levenshtein("kitten", "sitting") - 0.57142).abs() < 0.00001)
        // kitten/sitting is 57% similar, so a threshold of 95 should not be filtered
        assert!(!filter.should_filter_response(&resp));

        resp.text = String::new();
        filter.text = String::new();
        filter.threshold = 100;

        // assert!((normalized_levenshtein("", "") - 1.0).abs() < 0.00001)
        // two empty strings are the same, however ssdeep doesn't accept empty strings, expect false
        assert!(!filter.should_filter_response(&resp));
    }

    #[test]
    /// just a simple test to increase code coverage by hitting as_any and the inner value
    fn similarity_filter_as_any() {
        let filter = SimilarityFilter {
            text: String::from("stuff"),
            threshold: 95,
        };

        assert_eq!(filter.text, "stuff");
        assert_eq!(
            *filter.as_any().downcast_ref::<SimilarityFilter>().unwrap(),
            filter
        );
    }
}
