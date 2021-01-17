use super::*;
use crate::{filters::FeroxFilters, FeroxChannel};
use anyhow::Result;
use std::sync::Arc;
use tokio::{
    sync::mpsc::{self, UnboundedReceiver},
    task::JoinHandle,
};

/// event handler for updating a single data structure of all active filters
#[derive(Debug)]
pub struct FiltersHandler {
    /// collection of generic type `T` where `T` is some collection of data
    data: Arc<FeroxFilters>,

    /// Receiver half of mpsc from which `Command`s are processed
    receiver: UnboundedReceiver<Command>,
}

/// implementation of event handler for filters

impl FiltersHandler {
    /// create new event handler
    pub fn new(data: Arc<FeroxFilters>, receiver: UnboundedReceiver<Command>) -> Self {
        Self { data, receiver }
    }

    /// Initialize new `FeroxFilters` and the sc side of an mpsc channel that is responsible for
    /// updates to the aforementioned object.
    pub fn initialize() -> (JoinHandle<Result<()>>, FiltersHandle) {
        log::trace!("enter: initialize");

        let data = Arc::new(FeroxFilters::default());
        let (tx, rx): FeroxChannel<Command> = mpsc::unbounded_channel();

        let mut handler = Self::new(data.clone(), rx);

        let task = tokio::spawn(async move { handler.start().await });

        let event_handle = FiltersHandle::new(data, tx);

        log::trace!("exit: initialize -> ({:?})", event_handle);

        (task, event_handle)
    }

    /// Start a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives `Command` and acts accordingly
    pub async fn start(&mut self) -> Result<()> {
        log::trace!("enter: start({:?})", self);

        while let Some(command) = self.receiver.recv().await {
            match command {
                Command::AddFilter(filter) => {
                    self.data.push(filter)?;
                }
                Command::Exit => break,
                _ => {} // no other commands needed for FilterHandler
            }
        }

        log::trace!("exit: start");
        Ok(())
    }
}
