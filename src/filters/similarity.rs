use super::*;
use fuzzyhash::FuzzyHash;

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

    /// Compare one SimilarityFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}
