use super::*;

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
