use crate::config::CONFIGURATION;
use crate::utils::get_url_path_length;
use crate::FeroxResponse;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
