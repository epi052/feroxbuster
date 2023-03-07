use std::sync::RwLock;

use anyhow::Result;
use serde::{ser::SerializeSeq, Serialize, Serializer};

use crate::response::FeroxResponse;

use super::{
    FeroxFilter, LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter,
    WildcardFilter, WordsFilter,
};
use crate::{
    event_handlers::Command::AddToUsizeField, statistics::StatField::WildcardsFiltered,
    CommandSender,
};
/// Container around a collection of `FeroxFilters`s
#[derive(Debug, Default)]
pub struct FeroxFilters {
    /// collection of `FeroxFilters`
    pub filters: RwLock<Vec<Box<dyn FeroxFilter>>>,
}

/// implementation of FeroxFilter collection
impl FeroxFilters {
    /// add a single FeroxFilter to the collection
    pub fn push(&self, filter: Box<dyn FeroxFilter>) -> Result<()> {
        if let Ok(mut guard) = self.filters.write() {
            if guard.contains(&filter) {
                return Ok(());
            }

            guard.push(filter)
        }
        Ok(())
    }

    /// remove items from the underlying collection by their index
    ///
    /// note: indexes passed in should be index-to-remove+1. This is built for the scan mgt menu
    ///       so indexes aren't 0-based whehn the user enters them.
    ///       
    pub fn remove(&self, indices: &mut [usize]) {
        // since we're removing by index, indices must be sorted and then reversed.
        // this allows us to iterate over the collection from the rear, allowing any shifting
        // of the vector to happen on sections that we no longer care about, as we're moving
        // in the opposite direction
        indices.sort_unstable();
        indices.reverse();

        if let Ok(mut guard) = self.filters.write() {
            for index in indices {
                // numbering of the menu starts at 1, so we'll need to reduce the index by 1
                // to account for that. if they've provided 0 as an offset, we'll set the
                // result to a gigantic number and skip it in the loop with a bounds check
                let reduced_idx = index.checked_sub(1).unwrap_or(usize::MAX);

                // check if number provided is out of range
                if reduced_idx >= guard.len() {
                    // usize can't be negative, just need to handle exceeding bounds
                    continue;
                }

                guard.remove(reduced_idx);
            }
        }
    }

    /// Simple helper to stay DRY; determines whether or not a given `FeroxResponse` should be reported
    /// to the user or not.
    pub fn should_filter_response(
        &self,
        response: &FeroxResponse,
        tx_stats: CommandSender,
    ) -> bool {
        if let Ok(filters) = self.filters.read() {
            for filter in filters.iter() {
                // wildcard.should_filter goes here
                if filter.should_filter_response(response) {
                    log::debug!("filtering response due to: {:?}", filter);
                    if filter.as_any().downcast_ref::<WildcardFilter>().is_some() {
                        tx_stats
                            .send(AddToUsizeField(WildcardsFiltered, 1))
                            .unwrap_or_default();
                    }
                    return true;
                }
            }
        }
        false
    }
}

impl Serialize for FeroxFilters {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Ok(guard) = self.filters.read() {
            let mut seq = serializer.serialize_seq(Some(guard.len()))?;

            for filter in &*guard {
                if let Some(line_filter) = filter.as_any().downcast_ref::<LinesFilter>() {
                    seq.serialize_element(line_filter).unwrap_or_default();
                } else if let Some(word_filter) = filter.as_any().downcast_ref::<WordsFilter>() {
                    seq.serialize_element(word_filter).unwrap_or_default();
                } else if let Some(size_filter) = filter.as_any().downcast_ref::<SizeFilter>() {
                    seq.serialize_element(size_filter).unwrap_or_default();
                } else if let Some(wildcard_filter) =
                    filter.as_any().downcast_ref::<WildcardFilter>()
                {
                    seq.serialize_element(wildcard_filter).unwrap_or_default();
                } else if let Some(status_filter) =
                    filter.as_any().downcast_ref::<StatusCodeFilter>()
                {
                    seq.serialize_element(status_filter).unwrap_or_default();
                } else if let Some(regex_filter) = filter.as_any().downcast_ref::<RegexFilter>() {
                    seq.serialize_element(regex_filter).unwrap_or_default();
                } else if let Some(similarity_filter) =
                    filter.as_any().downcast_ref::<SimilarityFilter>()
                {
                    seq.serialize_element(similarity_filter).unwrap_or_default();
                }
            }
            seq.end()
        } else {
            // if for some reason we can't unlock the mutex, just write an empty list
            let seq = serializer.serialize_seq(Some(0))?;
            seq.end()
        }
    }
}
