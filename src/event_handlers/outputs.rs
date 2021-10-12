use super::Command::AddToUsizeField;
use super::*;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, oneshot};

use crate::{
    config::Configuration,
    progress::PROGRESS_PRINTER,
    scanner::RESPONSES,
    send_command, skip_fail,
    statistics::StatField::ResourcesDiscovered,
    traits::FeroxSerialize,
    utils::{ferox_print, fmt_err, make_request, open_file, write_to},
    CommandReceiver, CommandSender, Joiner,
};
use std::sync::Arc;

#[derive(Debug)]
/// Container for terminal output transmitter
pub struct TermOutHandle {
    /// Transmitter that sends to the TermOutHandler handler
    pub tx: CommandSender,

    /// Transmitter that sends to the FileOutHandler handler
    pub tx_file: CommandSender,
}

/// implementation of OutputHandle
impl TermOutHandle {
    /// Given a CommandSender, create a new OutputHandle
    pub fn new(tx: CommandSender, tx_file: CommandSender) -> Self {
        Self { tx, tx_file }
    }

    /// Send the given Command over `tx`
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command)?;
        Ok(())
    }

    /// Sync the handle with the handler
    pub async fn sync(&self, send_to_file: bool) -> Result<()> {
        let (tx, rx) = oneshot::channel::<bool>();
        self.send(Command::Sync(tx))?;

        if send_to_file {
            let (tx, rx) = oneshot::channel::<bool>();
            self.tx_file.send(Command::Sync(tx))?;
            rx.await?;
        }

        rx.await?;
        Ok(())
    }
}

#[derive(Debug)]
/// Event handler for files
pub struct FileOutHandler {
    /// file output handler's receiver
    receiver: CommandReceiver,

    /// pointer to "global" configuration struct
    config: Arc<Configuration>,
}

impl FileOutHandler {
    /// Given a file tx/rx pair along with a filename and awaitable task, create
    /// a FileOutHandler
    fn new(rx: CommandReceiver, config: Arc<Configuration>) -> Self {
        Self {
            receiver: rx,
            config,
        }
    }

    /// Spawn a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives responses from the terminal handler and writes them to disk
    async fn start(&mut self, tx_stats: CommandSender) -> Result<()> {
        log::trace!("enter: start_file_handler({:?})", tx_stats);

        let mut file = open_file(&self.config.output)?;

        log::info!("Writing scan results to {}", self.config.output);

        while let Some(command) = self.receiver.recv().await {
            match command {
                Command::Report(response) => {
                    skip_fail!(write_to(&*response, &mut file, self.config.json));
                }
                Command::Exit => {
                    break;
                }
                Command::Sync(sender) => {
                    skip_fail!(sender.send(true));
                }
                _ => {} // no more needed
            }
        }

        // close the file before we tell statistics to save current data to the same file
        drop(file);

        send_command!(tx_stats, Command::Save);

        log::trace!("exit: start_file_handler");
        Ok(())
    }
}

#[derive(Debug)]
/// Event handler for terminal
pub struct TermOutHandler {
    /// terminal output handler's receiver
    receiver: CommandReceiver,

    /// file handler
    tx_file: CommandSender,

    /// optional file handler task
    file_task: Option<Joiner>,

    /// pointer to "global" configuration struct
    config: Arc<Configuration>,
}

/// implementation of TermOutHandler
impl TermOutHandler {
    /// Given a terminal receiver along with a file transmitter and filename, create
    /// an OutputHandler
    fn new(
        receiver: CommandReceiver,
        tx_file: CommandSender,
        file_task: Option<Joiner>,
        config: Arc<Configuration>,
    ) -> Self {
        Self {
            receiver,
            tx_file,
            file_task,
            config,
        }
    }

    /// Creates all required output handlers (terminal, file) and updates the given Handles/Tasks
    pub fn initialize(
        config: Arc<Configuration>,
        tx_stats: CommandSender,
    ) -> (Joiner, TermOutHandle) {
        log::trace!("enter: initialize({:?}, {:?})", config, tx_stats);

        let (tx_term, rx_term) = mpsc::unbounded_channel::<Command>();
        let (tx_file, rx_file) = mpsc::unbounded_channel::<Command>();

        let mut file_handler = FileOutHandler::new(rx_file, config.clone());

        let tx_stats_clone = tx_stats.clone();

        let file_task = if !config.output.is_empty() {
            // -o used, need to spawn the thread for writing to disk
            Some(tokio::spawn(async move {
                file_handler.start(tx_stats_clone).await
            }))
        } else {
            None
        };

        let mut term_handler = Self::new(rx_term, tx_file.clone(), file_task, config);
        let term_task = tokio::spawn(async move { term_handler.start(tx_stats).await });

        let event_handle = TermOutHandle::new(tx_term, tx_file);

        log::trace!("exit: initialize -> ({:?}, {:?})", term_task, event_handle);

        (term_task, event_handle)
    }

    /// Start a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives `Command` and acts accordingly
    async fn start(&mut self, tx_stats: CommandSender) -> Result<()> {
        log::trace!("enter: start({:?})", tx_stats);

        while let Some(command) = self.receiver.recv().await {
            match command {
                Command::Report(mut resp) => {
                    let contains_sentry =
                        self.config.status_codes.contains(&resp.status().as_u16());
                    let unknown_sentry = !RESPONSES.contains(&resp); // !contains == unknown
                    let should_process_response = contains_sentry && unknown_sentry;

                    if should_process_response {
                        // print to stdout
                        ferox_print(&resp.as_str(), &PROGRESS_PRINTER);

                        send_command!(tx_stats, AddToUsizeField(ResourcesDiscovered, 1));

                        if self.file_task.is_some() {
                            // -o used, need to send the report to be written out to disk
                            self.tx_file
                                .send(Command::Report(resp.clone()))
                                .with_context(|| {
                                    fmt_err(&format!("Could not send {} to file handler", resp))
                                })?;
                        }
                    }
                    log::trace!("report complete: {}", resp.url());

                    if self.config.replay_client.is_some() && should_process_response {
                        // replay proxy specified/client created and this response's status code is one that
                        // should be replayed; not using logged_request due to replay proxy client
                        make_request(
                            self.config.replay_client.as_ref().unwrap(),
                            resp.url(),
                            self.config.output_level,
                            &self.config,
                            tx_stats.clone(),
                        )
                        .await
                        .with_context(|| "Could not replay request through replay proxy")?;
                    }

                    if should_process_response {
                        // add response to RESPONSES for serialization in case of ctrl+c
                        // placed all by its lonesome like this so that RESPONSES can take ownership
                        // of the FeroxResponse

                        // before ownership is transferred, there's no real reason to keep the body anymore
                        // so we can free that piece of data, reducing memory usage
                        resp.drop_text();

                        RESPONSES.insert(*resp);
                    }
                }
                Command::Sync(sender) => {
                    sender.send(true).unwrap_or_default();
                }
                Command::Exit => {
                    if self.file_task.is_some() && self.tx_file.send(Command::Exit).is_ok() {
                        self.file_task.as_mut().unwrap().await??; // wait for death
                    }
                    break;
                }
                _ => {} // no more commands needed
            }
        }
        log::trace!("exit: start");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// try to hit struct field coverage of FileOutHandler
    fn struct_fields_of_file_out_handler() {
        let (_, rx) = mpsc::unbounded_channel::<Command>();
        let config = Arc::new(Configuration::new().unwrap());
        let foh = FileOutHandler {
            config,
            receiver: rx,
        };
        println!("{:?}", foh);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// try to hit struct field coverage of TermOutHandler
    async fn struct_fields_of_term_out_handler() {
        let (tx, rx) = mpsc::unbounded_channel::<Command>();
        let (tx_file, _) = mpsc::unbounded_channel::<Command>();
        let config = Arc::new(Configuration::new().unwrap());

        let toh = TermOutHandler {
            config,
            file_task: None,
            receiver: rx,
            tx_file,
        };

        println!("{:?}", toh);
        tx.send(Command::Exit).unwrap();
    }
}
