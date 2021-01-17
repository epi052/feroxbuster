use super::FeroxFilter;
use anyhow::Result;
use std::sync::Mutex;

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
        if let Ok(mut unlocked) = self.filters.lock() {
            unlocked.push(filter)
        }
        Ok(())
    }
}
