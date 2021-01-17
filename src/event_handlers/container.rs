use super::Command;
use crate::{filters::FeroxFilters, statistics::Stats, CommandSender};
use anyhow::Result;
use std::sync::Arc;
use tokio::task::JoinHandle;

type Joiner = JoinHandle<Result<()>>;

pub struct Tasks {
    pub terminal: Joiner,
    pub file: Option<Joiner>,
    pub stats: Joiner,
    pub filters: Joiner,
}

impl Tasks {
    pub fn new(terminal: Joiner, file: Option<Joiner>, stats: Joiner, filters: Joiner) -> Self {
        Self {
            terminal,
            file,
            stats,
            filters,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatsHandle {
    pub data: Arc<Stats>,
    pub tx: CommandSender,
}

impl StatsHandle {
    pub fn new(data: Arc<Stats>, tx: CommandSender) -> Self {
        Self { data, tx }
    }
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FiltersHandle {
    pub data: Arc<FeroxFilters>,
    pub tx: CommandSender,
}

impl FiltersHandle {
    pub fn new(data: Arc<FeroxFilters>, tx: CommandSender) -> Self {
        Self { data, tx }
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command)?;
        Ok(())
    }
}

// #[derive(Clone, Debug)]
// pub struct ReporterTerminalHandle {
//     pub data: Arc<Stats>,
//     pub task: JoinHandle<Result<()>>,
//     pub transmitter: CommandSender,
// }
//
// impl StatsHandle {
//     pub fn new(data: Arc<Stats>, transmitter: CommandSender, task: JoinHandle<Result<()>>) -> Self {
//         Self {
//             data,
//             task,
//             transmitter,
//         }
//     }
// }

// #[derive(Clone, Debug)]
// pub enum EventHandle {
//     Stats(StatsHandle),
//     Filters(FiltersHandle),
// }
//
// impl EventHandle {
//     pub fn send(&self, cmd: Command) -> Result<()> {
//         if let EventHandle::Filters(handle) = self {
//             send_command!(handle.transmitter, cmd);
//         }
//         Ok(())
//     }
//
//
//     pub fn stats(&self) -> Result<&StatsHandle> {
//         match self {
//             EventHandle::Stats(handle) => Ok(handle),
//             _ => {
//                 bail!("no underlying StatsHandle found")
//             }
//         }
//     }
//
//     pub fn filters(&self) -> Result<&FiltersHandle> {
//         match self {
//             EventHandle::Filters(handle) => Ok(handle),
//             _ => {
//                 bail!("no underlying FiltersHandle found")
//             }
//         }
//     }
// }

// todo need to move these into their proper places

/// todo docs everywhere in this file

// /// todo docs if this pans out
#[derive(Clone, Debug)]
pub struct Handles {
    pub stats: StatsHandle,
    pub filters: FiltersHandle,
}

impl Handles {
    pub fn new(stats: StatsHandle, filters: FiltersHandle) -> Self {
        Self { stats, filters }
    }

    // /// todo
    // ///
    // /// expected order of task completion
    // /// - terminal
    // /// - file (if present)
    // /// - filters
    // /// - stats
    // pub async fn clean_up(self) -> Result<()> {
    //     // todo trace
    //
    //     send_command!(self.filters.tx, Command::Exit); // send exit command and await the end of the future
    //     self.filters
    //         .task
    //         .await
    //         .with_context(|| fmt_err("Could not await a filters handler's receiver"))?;
    //
    //     send_command!(self.stats.tx, Command::Exit);
    //     self.stats
    //         .task
    //         .await
    //         .with_context(|| fmt_err("Could not await a stats handler's receiver"))?;
    //
    //     Ok(())
    // }
}
