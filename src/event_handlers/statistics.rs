use crate::{
    config::CONFIGURATION,
    progress::{add_bar, BarType},
    statistics::{StatCommand, StatField, Stats},
};
use anyhow::Result;
use console::style;
use indicatif::ProgressBar;
use std::{sync::Arc, time::Instant};
use tokio::sync::mpsc::UnboundedReceiver;

/// event handler struct for updating statistics
#[derive(Debug)]
pub struct StatsHandler {
    /// overall scan's progress bar
    bar: ProgressBar,

    /// Receiver half of mpsc from which `StatCommand`s are processed
    receiver: UnboundedReceiver<StatCommand>,

    /// data class that stores all statistics updates
    stats: Arc<Stats>,
}

/// implementation of event handler for statistics
impl StatsHandler {
    /// create new event handler builder
    pub fn new(stats: Arc<Stats>, rx_stats: UnboundedReceiver<StatCommand>) -> Self {
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
    pub async fn start(&mut self) -> Result<()> {
        log::trace!("enter: start({:?})", self);

        let start = Instant::now();

        while let Some(command) = self.receiver.recv().await {
            match command as StatCommand {
                StatCommand::AddError(err) => {
                    self.stats.add_error(err);
                    self.increment_bar();
                }
                StatCommand::AddStatus(status) => {
                    self.stats.add_status_code(status);
                    self.increment_bar();
                }
                StatCommand::AddRequest => {
                    self.stats.add_request();
                    self.increment_bar();
                }
                StatCommand::Save => {
                    self.stats
                        .save(start.elapsed().as_secs_f64(), &CONFIGURATION.output)?;
                }
                StatCommand::UpdateUsizeField(field, value) => {
                    let update_len = matches!(field, StatField::TotalScans);
                    self.stats.update_usize_field(field, value);

                    if update_len {
                        self.bar.set_length(self.stats.total_expected() as u64)
                    }
                }
                StatCommand::UpdateF64Field(field, value) => {
                    self.stats.update_f64_field(field, value)
                }
                StatCommand::CreateBar => {
                    self.bar = add_bar("", self.stats.total_expected() as u64, BarType::Total);
                }
                StatCommand::LoadStats(filename) => {
                    self.stats.merge_from(&filename);
                }
                StatCommand::Exit => break,
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
}
