use super::*;
use crate::event_handlers::command::Command::Exit;
use crate::{
    config::CONFIGURATION,
    progress::{add_bar, BarType},
    statistics::{StatField, Stats},
    CommandSender, FeroxChannel, Joiner,
};
use anyhow::Result;
use console::style;
use indicatif::ProgressBar;
use std::{sync::Arc, time::Instant};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::oneshot::{self, Receiver, Sender};

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

    /// scan complete notifier
    tx_complete: Sender<Command>,
}

/// implementation of event handler for statistics
impl StatsHandler {
    /// create new event handler
    fn new(
        stats: Arc<Stats>,
        rx_stats: UnboundedReceiver<Command>,
        tx_complete: Sender<Command>,
    ) -> Self {
        // will be updated later via StatCommand; delay is for banner to print first
        let bar = ProgressBar::hidden();

        Self {
            bar,
            stats,
            tx_complete,
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
                        self.stats.increment_active_scans();
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
                Command::DecrementActiveScans => {
                    self.stats.decrement_active_scans();
                    log::error!("active scans: {}", self.stats.active_scans());
                    if self.stats.active_scans() == 1 {
                        // todo this is pretty lame, consider awaiting the rx oneshot after first scan incrememnts instead
                        // requires awaiting to actually work (join_all)
                        let (dummy, _) = oneshot::channel();
                        let tx = std::mem::replace(&mut self.tx_complete, dummy);
                        tx.send(Exit).unwrap_or_default();
                    }
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
    pub fn initialize() -> (Receiver<Command>, Joiner, StatsHandle) {
        log::trace!("enter: initialize");

        let data = Arc::new(Stats::new());
        let (tx, rx): FeroxChannel<Command> = mpsc::unbounded_channel();
        let (tx_complete, rx_complete): (Sender<Command>, Receiver<Command>) = oneshot::channel();

        let mut handler = StatsHandler::new(data.clone(), rx, tx_complete);

        let task = tokio::spawn(async move { handler.start().await });

        let event_handle = StatsHandle::new(data, tx);

        log::trace!("exit: initialize -> ({:?}, {:?})", task, event_handle);

        (rx_complete, task, event_handle)
    }
}
