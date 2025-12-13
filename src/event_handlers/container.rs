use super::*;
use crate::config::Configuration;
use crate::event_handlers::scans::ScanHandle;
use crate::scan_manager::FeroxScans;
use crate::Joiner;
#[cfg(test)]
use crate::{filters::FeroxFilters, statistics::Stats, Command};
use anyhow::{bail, Result};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
#[cfg(test)]
use tokio::sync::mpsc::{self, UnboundedReceiver};

#[derive(Debug)]
/// Simple container for multiple JoinHandles
pub struct Tasks {
    /// JoinHandle for terminal handler
    pub terminal: Joiner,

    /// JoinHandle for statistics handler
    pub stats: Joiner,

    /// JoinHandle for filters handler
    pub filters: Joiner,

    /// JoinHandle for scans handler
    pub scans: Joiner,
}

/// Tasks implementation
impl Tasks {
    /// Given JoinHandles for terminal, statistics, and filters create a new Tasks object
    pub fn new(terminal: Joiner, stats: Joiner, filters: Joiner, scans: Joiner) -> Self {
        Self {
            terminal,
            stats,
            filters,
            scans,
        }
    }
}

#[derive(Debug)]
/// Container for the different *Handles that will be shared across modules
pub struct Handles {
    /// Handle for statistics
    pub stats: StatsHandle,

    /// Handle for filters
    pub filters: FiltersHandle,

    /// Handle for output (terminal/file)
    pub output: TermOutHandle,

    /// Handle for Configuration
    pub config: Arc<Configuration>,

    /// Handle for recursion
    pub scans: RwLock<Option<ScanHandle>>,

    /// Pointer to the list of words generated from reading in the wordlist
    pub wordlist: Arc<Vec<String>>,
}

/// implementation of Handles
impl Handles {
    /// Given a StatsHandle, FiltersHandle, and OutputHandle, create a Handles object
    pub fn new(
        stats: StatsHandle,
        filters: FiltersHandle,
        output: TermOutHandle,
        config: Arc<Configuration>,
        wordlist: Arc<Vec<String>>,
    ) -> Self {
        Self {
            stats,
            filters,
            output,
            config,
            scans: RwLock::new(None),
            wordlist,
        }
    }

    /// create a Handles object suitable for unit testing (non-functional)
    #[cfg(test)]
    pub fn for_testing(
        scanned_urls: Option<Arc<FeroxScans>>,
        config: Option<Arc<Configuration>>,
    ) -> (Self, UnboundedReceiver<Command>) {
        let configuration = config.unwrap_or_else(|| Arc::new(Configuration::new().unwrap()));
        let (tx, rx) = mpsc::unbounded_channel::<Command>();
        let terminal_handle = TermOutHandle::new(tx.clone(), tx.clone());
        let stats_handle = StatsHandle::new(Arc::new(Stats::new(configuration.json)), tx.clone());
        let filters_handle = FiltersHandle::new(Arc::new(FeroxFilters::default()), tx.clone());
        let wordlist = Arc::new(vec![String::from("this_is_a_test")]);
        let handles = Self::new(
            stats_handle,
            filters_handle,
            terminal_handle,
            configuration,
            wordlist,
        );
        if let Some(sh) = scanned_urls {
            let scan_handle = ScanHandle::new(sh, tx);
            handles.set_scan_handle(scan_handle);
        }
        (handles, rx)
    }

    /// Set the ScanHandle object
    pub fn set_scan_handle(&self, handle: ScanHandle) {
        if let Ok(mut guard) = self.scans.write() {
            if guard.is_none() {
                guard.replace(handle);
            }
        }
    }

    /// Helper to easily send a Command over the (locked) underlying CommandSender object
    pub fn send_scan_command(&self, command: Command) -> Result<()> {
        if let Ok(guard) = self.scans.read().as_ref() {
            if let Some(handle) = guard.as_ref() {
                handle.send(command)?;
                return Ok(());
            }
        }

        bail!("Could not get underlying CommandSender object")
    }

    /// wrapper to reach into `FeroxScans` and yank out the length of `collected_extensions`
    pub fn num_collected_extensions(&self) -> usize {
        if !self.config.collect_extensions {
            // if --collect-extensions wasn't used, simply return 0 and forego unlocking
            return 0;
        }

        self.collected_extensions().len()
    }

    /// wrapper to reach into `FeroxScans` and yank out the length of `collected_extensions`
    pub fn collected_extensions(&self) -> HashSet<String> {
        if let Ok(scans) = self.ferox_scans() {
            if let Ok(extensions) = scans.collected_extensions.read() {
                return extensions.clone();
            }
        }

        HashSet::new()
    }

    /// number of words in the wordlist, multiplied by `expected_num_requests_multiplier`
    pub fn expected_num_requests_per_dir(&self) -> usize {
        let num_words = self.wordlist.len();
        let multiplier = self.expected_num_requests_multiplier();
        multiplier * num_words
    }

    /// estimate of HTTP requests per word = (base + static extensions + collected extensions)
    /// multiplied by the number of request methods
    pub fn expected_num_requests_multiplier(&self) -> usize {
        let methods = self.config.methods.len().max(1);
        let base_requests = 1; // the bare word (with optional slash)
        let static_extensions = self.config.extensions.len();
        let dynamic_extensions = self.num_collected_extensions();

        let total_paths = base_requests + static_extensions + dynamic_extensions;

        total_paths * methods
    }

    /// Helper to easily get the (locked) underlying FeroxScans object
    pub fn ferox_scans(&self) -> Result<Arc<FeroxScans>> {
        if let Ok(guard) = self.scans.read().as_ref() {
            if let Some(handle) = guard.as_ref() {
                return Ok(handle.data.clone());
            }
        }

        bail!("Could not get underlying FeroxScans")
    }
}
