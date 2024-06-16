use super::*;
use crate::{
    config::Configuration,
    progress::{add_bar, BarType},
    statistics::{StatField, Stats},
    CommandSender, FeroxChannel, Joiner,
};
use anyhow::Result;
use console::style;
use indicatif::ProgressBar;
use std::{sync::Arc, time::Instant};
use tokio::sync::{
    mpsc::{self, UnboundedReceiver},
    oneshot,
};

#[derive(Debug)]
/// Container for statistics transmitter and Stats object
pub struct StatsHandle {
    /// Stats object used across modules to track statistics
    pub data: Arc<Stats>,

    /// transmitter used to update `data`
    pub tx: CommandSender,
}

/// implementation of StatsHandle
impl StatsHandle {
    /// Given an Arc-wrapped Stats and CommandSender, create a new StatsHandle
    pub fn new(data: Arc<Stats>, tx: CommandSender) -> Self {
        Self { data, tx }
    }

    /// Send the given Command over `tx`
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command)?;
        Ok(())
    }

    /// Sync the handle with the handler
    pub async fn sync(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel::<bool>();
        self.send(Command::Sync(tx))?;
        rx.await?;
        Ok(())
    }
}

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
    async fn start(&mut self, output_file: &str) -> Result<()> {
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
                        .save(start.elapsed().as_secs_f64(), output_file)?;
                }
                Command::AddToUsizeField(field, value) => {
                    self.stats.update_usize_field(field, value);

                    if matches!(field, StatField::TotalScans | StatField::TotalExpected) {
                        self.bar.set_length(self.stats.total_expected() as u64);
                    }
                }
                Command::SubtractFromUsizeField(field, value) => {
                    self.stats.subtract_from_usize_field(field, value);

                    if matches!(field, StatField::TotalExpected) {
                        self.bar.set_length(self.stats.total_expected() as u64);
                    }
                }
                Command::AddToF64Field(field, value) => self.stats.update_f64_field(field, value),
                Command::CreateBar(offset) => {
                    self.bar = add_bar("", self.stats.total_expected() as u64, BarType::Total);
                    self.bar.set_position(offset);
                }
                Command::LoadStats(filename) => {
                    self.stats.merge_from(&filename)?;
                }
                Command::Sync(sender) => {
                    sender.send(true).unwrap_or_default();
                }
                Command::QueryOverallBarEta(sender) => {
                    sender.send(self.bar.eta()).unwrap_or_default();
                }
                Command::UpdateTargets(targets) => {
                    self.stats.update_targets(targets);
                }
                Command::Exit => break,
                _ => {} // no more commands needed
            }
        }

        self.bar.finish();

        log::info!("{:#?}", *self.stats);
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

        self.bar.set_message(msg);

        if self.bar.position() < self.stats.total_expected() as u64 {
            // don't run off the end when we're a few requests over the expected total
            // due to the heuristics tests
            self.bar.inc(1);
        }
    }

    /// Initialize new `Stats` object and the sc side of an mpsc channel that is responsible for
    /// updates to the aforementioned object.
    pub fn initialize(config: Arc<Configuration>) -> (Joiner, StatsHandle) {
        log::trace!("enter: initialize");

        let data = Arc::new(Stats::new(config.json));
        let (tx, rx): FeroxChannel<Command> = mpsc::unbounded_channel();

        let mut handler = StatsHandler::new(data.clone(), rx);

        let task = tokio::spawn(async move { handler.start(&config.output).await });

        let event_handle = StatsHandle::new(data, tx);

        log::trace!("exit: initialize -> ({:?}, {:?})", task, event_handle);

        (task, event_handle)
    }
}
