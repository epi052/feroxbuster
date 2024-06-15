use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use tokio::sync::oneshot::Sender;

use crate::response::FeroxResponse;
use crate::{
    event_handlers::Handles,
    message::FeroxMessage,
    statistics::{StatError, StatField},
    traits::FeroxFilter,
};

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
    ///
    /// the u64 value is the offset at which to start the progress bar (can be 0)
    CreateBar(u64),

    /// Add to a `Stats` field that corresponds to the given `StatField` by the given `usize` value
    AddToUsizeField(StatField, usize),

    /// Subtract from a `Stats` field that corresponds to the given `StatField` by the given `usize` value
    SubtractFromUsizeField(StatField, usize),

    /// Update a `Stats` field that corresponds to the given `StatField` by the given `f64` value
    AddToF64Field(StatField, f64),

    /// Save a `Stats` object to disk using `reporter::get_cached_file_handle`
    Save,

    /// Load a `Stats` object from disk
    LoadStats(String),

    /// Add a `FeroxFilter` implementor to `FilterHandler`'s instance of `FeroxFilters`
    AddFilter(Box<dyn FeroxFilter>),

    /// Remove a set of `FeroxFilter` implementors from `FeroxFilters` by index
    RemoveFilters(Vec<usize>),

    /// Send a `FeroxResponse` to the output handler for reporting
    Report(Box<FeroxResponse>),

    /// Send a group of urls to be scanned (only used for the urls passed in explicitly by the user)
    ScanInitialUrls(Vec<String>),

    /// Send a single url to be scanned (presumably added from the interactive menu)
    ScanNewUrl(String),

    /// Determine whether or not recursion is appropriate, given a FeroxResponse, if so start a scan
    TryRecursion(Box<FeroxResponse>),

    /// Send a pointer to the wordlist to the recursion handler
    UpdateWordlist(Arc<Vec<String>>),

    /// Instruct the ScanHandler to join on all known scans, use sender to notify main when done
    JoinTasks(Sender<bool>),

    /// Command used to test that a spawned task succeeded in initialization
    Ping,

    /// Just receive a sender and reply, used for slowing down the main thread
    Sync(Sender<bool>),

    /// Notify event handler that a new extension has been seen
    AddDiscoveredExtension(String),

    /// Write an arbitrary string to disk
    WriteToDisk(Box<FeroxMessage>),

    /// Break out of the (infinite) mpsc receive loop
    Exit,

    /// Give a handler access to an Arc<Handles> instance after the handler has
    /// already been initialized
    AddHandles(Arc<Handles>),

    /// inform the Stats object about which targets are being scanned
    UpdateTargets(Vec<String>),

    /// query the Stats handler about the position of the overall progress bar
    QueryOverallBarEta(Sender<Duration>),
}
