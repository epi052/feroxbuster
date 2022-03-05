use super::*;
use crate::{config::Configuration, statistics::Stats, traits::FeroxSerialize, utils::fmt_err};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashSet;
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

    /// collected extensions
    collected_extensions: HashSet<String>,
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
        let collected_extensions = match scans.collected_extensions.read() {
            Ok(extensions) => extensions.clone(),
            Err(_) => HashSet::new(),
        };

        Self {
            scans,
            config,
            responses,
            statistics,
            collected_extensions,
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
        serde_json::to_string(&self)
            .with_context(|| fmt_err("Could not convert scan's running state to JSON"))
    }
}
