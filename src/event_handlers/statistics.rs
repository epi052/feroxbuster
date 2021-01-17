use super::*;
use crate::{
    config::CONFIGURATION,
    progress::{add_bar, BarType},
    statistics::{StatField, Stats},
    FeroxChannel,
};
use anyhow::Result;
use console::style;
use indicatif::ProgressBar;
use std::{sync::Arc, time::Instant};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::task::JoinHandle;

/// event handler struct for updating statistics
#[derive(Debug)]
pub struct StatsHandler {
    /// overall scan's progress bar
    bar: ProgressBar,

    /// Receiver half of mpsc from which `StatCommand`s are processed
    receiver: UnboundedReceiver<Command>,

    /// data class that stores all statistics updates
    stats: Arc<Stats>,
}

/// implementation of event handler for statistics
impl StatsHandler {
    /// create new event handler
    fn new(stats: Arc<Stats>, rx_stats: UnboundedReceiver<Command>) -> Self {
        // will be updated later via StatCommand; delay is for banner to print first
        let bar = ProgressBar::hidden();

        Self {
            bar,
            stats,
            receiver: rx_stats,
        }
    }

    /// Start a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives `StatCommands` and updates the given `Stats` object as appropriate
    async fn start(&mut self) -> Result<()> {
        log::trace!("enter: start({:?})", self);

        let start = Instant::now();

        while let Some(command) = self.receiver.recv().await {
            match command as Command {
                Command::AddError(err) => {
                    self.stats.add_error(err);
                    self.increment_bar();
                }
                Command::AddStatus(status) => {
                    self.stats.add_status_code(status);
                    self.increment_bar();
                }
                Command::AddRequest => {
                    self.stats.add_request();
                    self.increment_bar();
                }
                Command::Save => {
                    self.stats
                        .save(start.elapsed().as_secs_f64(), &CONFIGURATION.output)?;
                }
                Command::UpdateUsizeField(field, value) => {
                    let update_len = matches!(field, StatField::TotalScans);
                    self.stats.update_usize_field(field, value);

                    if update_len {
                        self.bar.set_length(self.stats.total_expected() as u64)
                    }
                }
                Command::UpdateF64Field(field, value) => self.stats.update_f64_field(field, value),
                Command::CreateBar => {
                    self.bar = add_bar("", self.stats.total_expected() as u64, BarType::Total);
                }
                Command::LoadStats(filename) => {
                    self.stats.merge_from(&filename)?;
                }
                Command::Exit => break,
                _ => {} // no more commands needed
            }
        }

        self.bar.finish();

        log::debug!("{:#?}", *self.stats);
        log::trace!("exit: start");
        Ok(())
    }

    /// Wrapper around incrementing the overall scan's progress bar
    fn increment_bar(&self) {
        let msg = format!(
            "{}:{:<7} {}:{:<7}",
            style("found").green(),
            self.stats.resources_discovered(),
            style("errors").red(),
            self.stats.errors(),
        );

        self.bar.set_message(&msg);
        self.bar.inc(1);
    }

    /// Initialize new `Stats` object and the sc side of an mpsc channel that is responsible for
    /// updates to the aforementioned object.
    pub fn initialize() -> (JoinHandle<Result<()>>, StatsHandle) {
        log::trace!("enter: initialize");

        let data = Arc::new(Stats::new());
        let (tx, rx): FeroxChannel<Command> = mpsc::unbounded_channel();

        let mut handler = StatsHandler::new(data.clone(), rx);

        let task = tokio::spawn(async move { handler.start().await });

        let event_handle = StatsHandle::new(data, tx);

        log::trace!("exit: initialize -> ({:?})", event_handle);

        (task, event_handle)
    }
}
