use super::*;
use crate::nlp::preprocess;
use gaoya::simhash::{SimHash, SimHashBits, SimSipHasher64};
use lazy_static::lazy_static;

lazy_static! {
    /// single instance of the sip hasher used in similarity filtering
    pub static ref SIM_HASHER: SimHash<SimSipHasher64, u64, 64> =
        SimHash::<SimSipHasher64, u64, 64>::new(SimSipHasher64::new(1, 2));
}

/// maximum hamming distance allowed between two signatures
///
/// ref: https://static.googleusercontent.com/media/research.google.com/en//pubs/archive/33026.pdf
/// section: 4.1 Choice of Parameters
const MAX_HAMMING_DISTANCE: usize = 3;

/// Simple implementor of FeroxFilter; used to filter out responses based on the similarity of a
/// Response body with a known response; specified using --filter-similar-to
#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarityFilter {
    /// Hash of Response's body to be used during similarity comparison
    pub hash: u64,

    /// Url originally requested for the similarity filter
    pub original_url: String,
}

/// implementation of FeroxFilter for SimilarityFilter
impl FeroxFilter for SimilarityFilter {
    /// Check `FeroxResponse::text` against what was requested from the site passed in via
    /// --filter-similar-to
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        let other = SIM_HASHER.create_signature(preprocess(response.text()).iter());
        self.hash.hamming_distance(&other) <= MAX_HAMMING_DISTANCE
    }

    /// Compare one SimilarityFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other
            .downcast_ref::<Self>()
            .map_or(false, |a| self.hash == a.hash)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}
