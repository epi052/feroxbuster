use std::sync::Mutex;

use anyhow::Result;

use crate::response::FeroxResponse;
use crate::{
    event_handlers::Command::UpdateUsizeField, statistics::StatField::WildcardsFiltered,
    CommandSender,
};

use super::{FeroxFilter, WildcardFilter};

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
                if filter.should_filter_response(&response) {
                    if filter.as_any().downcast_ref::<WildcardFilter>().is_some() {
                        tx_stats
                            .send(UpdateUsizeField(WildcardsFiltered, 1))
                            .unwrap_or_default();
                    }
                    return true;
                }
            }
        }
        false
    }
}
