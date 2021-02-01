use super::*;
use crate::{config::Configuration, statistics::Stats, utils::fmt_err, FeroxSerialize};
use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::Arc;

/// Data container for (de)?serialization of multiple items
#[derive(Serialize, Debug)]
pub struct FeroxState {
    /// Known scans
    scans: Arc<FeroxScans>,

    /// Current running config
    config: Arc<Configuration>,

    /// Known responses
    responses: &'static FeroxResponses,

    /// Gathered statistics
    statistics: Arc<Stats>,
}

/// implementation of FeroxState
impl FeroxState {
    /// create new FeroxState object
    pub fn new(
        scans: Arc<FeroxScans>,
        config: Arc<Configuration>,
        responses: &'static FeroxResponses,
        statistics: Arc<Stats>,
    ) -> Self {
        Self {
            scans,
            config,
            responses,
            statistics,
        }
    }
}

/// FeroxSerialize implementation for FeroxState
impl FeroxSerialize for FeroxState {
    /// Simply return debug format of FeroxState to satisfy as_str
    fn as_str(&self) -> String {
        format!("{:?}", self)
    }

    /// Simple call to produce a JSON string using the given FeroxState
    fn as_json(&self) -> Result<String> {
        Ok(serde_json::to_string(&self)
            .with_context(|| fmt_err("Could not convert scan's running state to JSON"))?)
    }
}
