use super::*;
use crate::Joiner;

#[derive(Debug)]
/// Simple container for multiple JoinHandles
pub struct Tasks {
    /// JoinHandle for terminal handler
    pub terminal: Joiner,

    /// JoinHandle for statistics handler
    pub stats: Joiner,

    /// JoinHandle for filters handler
    pub filters: Joiner,
}

/// Tasks implementation
impl Tasks {
    /// Given JoinHandles for terminal, statistics, and filters create a new Tasks object
    pub fn new(terminal: Joiner, stats: Joiner, filters: Joiner) -> Self {
        Self {
            terminal,
            stats,
            filters,
        }
    }
}

#[derive(Clone, Debug)]
/// Container for the different *Handles that will be shared across modules
pub struct Handles {
    /// Handle for statistics
    pub stats: StatsHandle,

    /// Handle for filters
    pub filters: FiltersHandle,

    /// Handle for output (terminal/file)
    pub output: TermOutHandle,
}

/// implementation of Handles
impl Handles {
    /// Given a StatsHandle, FiltersHandle, and OutputHandle, create a Handles object
    pub fn new(stats: StatsHandle, filters: FiltersHandle, output: TermOutHandle) -> Self {
        Self {
            stats,
            filters,
            output,
        }
    }
}
