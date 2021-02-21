use super::*;

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
