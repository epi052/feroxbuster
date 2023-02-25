use super::*;
use fuzzyhash::FuzzyHash;
use gaoya::minhash::{MinHash, MinHasher, MinHasher16};
use gaoya::text::whitespace_split;

/// enum wrapper for two distinct hashing signature types
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum HashValueType {
    /// String value for FuzzyHash
    String(String),

    /// Vec<u16> value for minhash
    Vec(Vec<u16>),
}

impl Default for HashValueType {
    fn default() -> Self {
        Self::String(String::new())
    }
}

/// Simple implementor of FeroxFilter; used to filter out responses based on the similarity of a
/// Response body with a known response; specified using --filter-similar-to
#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarityFilter {
    /// Hash of Response's body to be used during similarity comparison
    pub hash: HashValueType,

    /// Percentage of similarity at which a page is determined to be a near-duplicate of another
    pub threshold: u32,

    /// Url originally requested for the similarity filter
    pub original_url: String,
}

/// implementation of FeroxFilter for SimilarityFilter
impl FeroxFilter for SimilarityFilter {
    /// Check `FeroxResponse::text` against what was requested from the site passed in via
    /// --filter-similar-to
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        match self.hash {
            HashValueType::String(ref hash) => {
                // original response size was over the minimum required to effectively use ssdeep
                let other = FuzzyHash::new(response.text());

                if let Ok(result) = FuzzyHash::compare(hash, other.to_string()) {
                    return result >= self.threshold;
                }
            }
            HashValueType::Vec(ref hash) => {
                // original response was too small for ssdeep, so minhash was used as an alternative
                let hasher = MinHasher16::new(256);
                let other = hasher.create_signature(whitespace_split(response.text()));
                let result = hasher.compute_similarity(hash.iter(), other.iter());
                return (result * 100.0) as u32 >= self.threshold;
            }
        }

        // couldn't hash the response, don't filter
        log::warn!(
            "Could not compare similarity of body from {}; returning not-similar",
            response.url().as_str()
        );
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
