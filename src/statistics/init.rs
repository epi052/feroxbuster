use super::{command::StatCommand, container::Stats};
use crate::{event_handlers::StatsHandler, FeroxChannel};
use anyhow::Result;
use std::sync::Arc;
use tokio::{
    sync::mpsc::{self, UnboundedSender},
    task::JoinHandle,
};

/// Initialize new `Stats` object and the sc side of an mpsc channel that is responsible for
/// updates to the aforementioned object.
pub fn initialize() -> (
    Arc<Stats>,
    UnboundedSender<StatCommand>,
    JoinHandle<Result<()>>,
) {
    log::trace!("enter: initialize");

    let stats_tracker = Arc::new(Stats::new());
    let (tx_stats, rx_stats): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

    let mut handler = StatsHandler::new(stats_tracker.clone(), rx_stats);

    let stats_thread = tokio::spawn(async move { handler.start().await });

    log::trace!(
        "exit: initialize -> ({:?}, {:?}, {:?})",
        stats_tracker,
        tx_stats,
        stats_thread
    );

    (stats_tracker, tx_stats, stats_thread)
}
