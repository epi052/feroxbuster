use super::*;
use crate::nlp::preprocess;
use crate::NEAR_DUPLICATE_DISTANCE;
use gaoya::simhash::{SimHash, SimHashBits, SimSipHasher64};
use lazy_static::lazy_static;

lazy_static! {
    /// single instance of the sip hasher used in similarity filtering
    pub static ref SIM_HASHER: SimHash<SimSipHasher64, u64, 64> =
        SimHash::<SimSipHasher64, u64, 64>::new(SimSipHasher64::new(1, 2));
}

/// Simple implementor of FeroxFilter; used to filter out responses based on the similarity of a
/// Response body with a known response; specified using --filter-similar-to
#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarityFilter {
    /// Hash of Response's body to be used during similarity comparison
    pub hash: u64,

    /// Url originally requested for the similarity filter
    pub original_url: String,

    /// Maximum hamming distance allowed between two signatures
    pub cutoff: usize,
}

impl SimilarityFilter {
    /// Create a new SimilarityFilter
    pub fn new(hash: u64, original_url: String, cutoff: usize) -> Self {
        Self {
            hash,
            original_url,
            cutoff,
        }
    }
}

impl From<&FeroxResponse> for SimilarityFilter {
    fn from(response: &FeroxResponse) -> Self {
        Self::new(
            SIM_HASHER.create_signature(preprocess(response.text()).iter()),
            response.url().to_string(),
            NEAR_DUPLICATE_DISTANCE,
        )
    }
}

/// implementation of FeroxFilter for SimilarityFilter
impl FeroxFilter for SimilarityFilter {
    /// Check `FeroxResponse::text` against what was requested from the site passed in via
    /// --filter-similar-to
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        let other = SIM_HASHER.create_signature(preprocess(response.text()).iter());
        self.hash.hamming_distance(&other) <= NEAR_DUPLICATE_DISTANCE
    }

    /// Compare one SimilarityFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other
            .downcast_ref::<Self>()
            .is_some_and(|a| self.hash == a.hash)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}
