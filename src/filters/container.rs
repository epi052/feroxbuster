use std::sync::Mutex;

use anyhow::Result;
use serde::{ser::SerializeSeq, Serialize, Serializer};

use crate::{
    event_handlers::Command::AddToUsizeField, response::FeroxResponse,
    statistics::StatField::WildcardsFiltered, CommandSender,
};

use super::{
    FeroxFilter, LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter,
    WildcardFilter, WordsFilter,
};

/// Container around a collection of `FeroxFilters`s
#[derive(Debug, Default)]
pub struct FeroxFilters {
    /// collection of `FeroxFilters`
    pub filters: Mutex<Vec<Box<dyn FeroxFilter>>>,
}

/// implementation of FeroxFilter collection
impl FeroxFilters {
    /// add a single FeroxFilter to the collection
    pub fn push(&self, filter: Box<dyn FeroxFilter>) -> Result<()> {
        if let Ok(mut guard) = self.filters.lock() {
            if guard.contains(&filter) {
                return Ok(());
            }

            guard.push(filter)
        }
        Ok(())
    }

    /// Simple helper to stay DRY; determines whether or not a given `FeroxResponse` should be reported
    /// to the user or not.
    pub fn should_filter_response(
        &self,
        response: &FeroxResponse,
        tx_stats: CommandSender,
    ) -> bool {
        if let Ok(filters) = self.filters.lock() {
            for filter in filters.iter() {
                // wildcard.should_filter goes here
                if filter.should_filter_response(response) {
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
        if let Ok(guard) = self.filters.lock() {
            let mut seq = serializer.serialize_seq(Some(guard.len()))?;

            for filter in &*guard {
                if let Some(line_filter) = filter.as_any().downcast_ref::<LinesFilter>() {
                    seq.serialize_element(line_filter).unwrap_or_default();
                } else if let Some(word_filter) = filter.as_any().downcast_ref::<WordsFilter>() {
                    seq.serialize_element(word_filter).unwrap_or_default();
                } else if let Some(size_filter) = filter.as_any().downcast_ref::<SizeFilter>() {
                    seq.serialize_element(size_filter).unwrap_or_default();
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
                } else if let Some(wildcard_filter) =
                    filter.as_any().downcast_ref::<WildcardFilter>()
                {
                    seq.serialize_element(wildcard_filter).unwrap_or_default();
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
