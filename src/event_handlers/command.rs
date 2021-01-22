use crate::{
    statistics::{StatError, StatField},
    traits::FeroxFilter,
    FeroxResponse,
};
use reqwest::StatusCode;
use std::collections::HashSet;
use std::sync::Arc;

/// Protocol definition for updating an event handler via mpsc
#[derive(Debug)]
pub enum Command {
    /// Add one to the total number of requests
    AddRequest,

    /// Add one to the proper field(s) based on the given `StatError`
    AddError(StatError),

    /// Add one to the proper field(s) based on the given `StatusCode`
    AddStatus(StatusCode),

    /// Create the progress bar (`BarType::Total`) that is updated from the stats thread
    CreateBar,

    /// Update a `Stats` field that corresponds to the given `StatField` by the given `usize` value
    UpdateUsizeField(StatField, usize),

    /// Update a `Stats` field that corresponds to the given `StatField` by the given `f64` value
    UpdateF64Field(StatField, f64),

    /// Save a `Stats` object to disk using `reporter::get_cached_file_handle`
    Save,

    /// Load a `Stats` object from disk
    LoadStats(String),

    /// Subtract one from the current number of active scans
    DecrementActiveScans,

    /// Add a `FeroxFilter` implementor to `FilterHandler`'s instance of `FeroxFilters`
    AddFilter(Box<dyn FeroxFilter>),

    /// Send a `FeroxResponse` to the output handler for reporting
    Report(Box<FeroxResponse>),

    /// Send a url to be scanned (in the context of recursion)
    ScanUrl(String),

    /// Send a group of urls to be scanned (only used for the urls passed in explicitly by the user)
    ScanInitialUrls(Vec<String>),

    /// Send a pointer to the wordlist to the recursion handler
    UpdateWordlist(Arc<HashSet<String>>),

    /// Send a pointer to the wordlist to the recursion handler
    AddDirectoryScan(Arc<HashSet<String>>), // todo never used?

    /// Break out of the (infinite) mpsc receive loop
    Exit,
}
