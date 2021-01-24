use super::*;
use crate::event_handlers::scans::ScanHandle;
use crate::scan_manager::FeroxScans;
use crate::{CommandSender, Joiner};
use anyhow::{bail, Result};
use std::sync::{Arc, RwLock};

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

    /// Handle for recursion
    pub scans: RwLock<Option<ScanHandle>>,
}

/// implementation of Handles
impl Handles {
    /// Given a StatsHandle, FiltersHandle, and OutputHandle, create a Handles object
    pub fn new(stats: StatsHandle, filters: FiltersHandle, output: TermOutHandle) -> Self {
        Self {
            stats,
            filters,
            output,
            scans: RwLock::new(None),
        }
    }

    /// Set the ScanHandle object
    pub fn scan_handle(&self, handle: ScanHandle) {
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
                handle.tx.send(command)?;
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

    /// Helper to easily get the (locked) underlying transmitter
    pub fn sender(&self) -> Result<CommandSender> {
        if let Ok(guard) = self.scans.read().as_ref() {
            if let Some(handle) = guard.as_ref() {
                return Ok(handle.tx.clone());
            }
        }

        bail!("Could not get underlying transmitter")
    }
}
