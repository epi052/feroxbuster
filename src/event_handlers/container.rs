use super::*;
use crate::config::Configuration;
use crate::event_handlers::scans::ScanHandle;
use crate::scan_manager::FeroxScans;
use crate::Joiner;
#[cfg(test)]
use crate::{filters::FeroxFilters, statistics::Stats, Command};
use anyhow::{bail, Result};
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
}

/// implementation of Handles
impl Handles {
    /// Given a StatsHandle, FiltersHandle, and OutputHandle, create a Handles object
    pub fn new(
        stats: StatsHandle,
        filters: FiltersHandle,
        output: TermOutHandle,
        config: Arc<Configuration>,
    ) -> Self {
        Self {
            stats,
            filters,
            output,
            config,
            scans: RwLock::new(None),
        }
    }

    /// create a Handles object suitable for unit testing (non-functional)
    #[cfg(test)]
    pub fn for_testing(
        scanned_urls: Option<Arc<FeroxScans>>,
        config: Option<Arc<Configuration>>,
    ) -> (Self, UnboundedReceiver<Command>) {
        let (tx, rx) = mpsc::unbounded_channel::<Command>();
        let terminal_handle = TermOutHandle::new(tx.clone(), tx.clone());
        let stats_handle = StatsHandle::new(Arc::new(Stats::new()), tx.clone());
        let filters_handle = FiltersHandle::new(Arc::new(FeroxFilters::default()), tx.clone());
        let configuration = config.unwrap_or_else(|| Arc::new(Configuration::new().unwrap()));
        let handles = Self::new(stats_handle, filters_handle, terminal_handle, configuration);
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
                let _ = std::mem::replace(&mut *guard, Some(handle));
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
