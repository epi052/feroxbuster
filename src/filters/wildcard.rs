use super::*;

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
