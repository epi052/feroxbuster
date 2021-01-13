use super::*;

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
