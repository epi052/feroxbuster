use super::*;

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
