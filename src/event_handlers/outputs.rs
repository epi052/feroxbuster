use anyhow::{Context, Result};
use tokio::sync::mpsc;

use crate::{
    config::{CONFIGURATION, PROGRESS_PRINTER},
    scanner::RESPONSES,
    send_command,
    statistics::StatField::ResourcesDiscovered,
    utils::{ferox_print, fmt_err, make_request, open_file, write_to},
    CommandReceiver, CommandSender, FeroxChannel, FeroxSerialize, Joiner,
};

use super::Command::UpdateUsizeField;
use super::*;

#[derive(Debug)]
/// Container for terminal output transmitter
pub struct TermOutHandle {
    pub tx: CommandSender,
}

/// implementation of OutputHandle
impl TermOutHandle {
    /// Given a CommandSender, create a new OutputHandle
    pub fn new(tx: CommandSender) -> Self {
        Self { tx }
    }

    /// Send the given Command over `tx`
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command)?;
        Ok(())
    }
}

#[derive(Debug)]
/// Event handler for files
pub struct FileOutHandler {
    /// file output handler's receiver
    receiver: CommandReceiver,

    /// Path to file used for writing to disk
    output: String,
}

impl FileOutHandler {
    /// Given a file tx/rx pair along with a filename and awaitable task, create
    /// a FileOutHandler
    fn new(output: &str, rx: CommandReceiver) -> Self {
        Self {
            receiver: rx,
            output: output.to_string(),
        }
    }

    /// Spawn a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives responses from the terminal handler and writes them to disk
    async fn start(&mut self, tx_stats: CommandSender) -> Result<()> {
        let mut file = open_file(&self.output)?;

        log::trace!("enter: start_file_handler({:?})", tx_stats);
        log::info!("Writing scan results to {}", self.output);

        while let Some(command) = self.receiver.recv().await {
            match command {
                Command::Report(response) => {
                    write_to(&*response, &mut file, CONFIGURATION.json)?;
                }
                Command::Exit => break,
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

    /// whether or not user specified an output file
    save_output: bool,
}

/// implementation of TermOutHandler
impl TermOutHandler {
    /// Given a terminal receiver along with a file transmitter and filename, create
    /// an OutputHandler
    fn new(receiver: CommandReceiver, tx_file: CommandSender, file_task: Option<Joiner>) -> Self {
        Self {
            receiver,
            tx_file,
            save_output: file_task.is_some(),
            file_task,
        }
    }

    /// Creates all required output handlers (terminal, file) and updates the given Handles/Tasks
    pub fn initialize(output_file: &str, tx_stats: CommandSender) -> (Joiner, TermOutHandle) {
        log::trace!("enter: initialize({}, {:?})", output_file, tx_stats);

        let (tx_term, rx_term): FeroxChannel<Command> = mpsc::unbounded_channel();
        let (tx_file, rx_file): FeroxChannel<Command> = mpsc::unbounded_channel();

        let mut file_handler = FileOutHandler::new(output_file, rx_file);

        let tx_stats_clone = tx_stats.clone();

        let file_task = if !output_file.is_empty() {
            // -o used, need to spawn the thread for writing to disk
            Some(tokio::spawn(async move {
                file_handler.start(tx_stats_clone).await
            }))
        } else {
            None
        };

        let mut term_handler = Self::new(rx_term, tx_file, file_task);
        let term_task = tokio::spawn(async move { term_handler.start(tx_stats).await });

        let event_handle = TermOutHandle::new(tx_term);

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
                        CONFIGURATION.status_codes.contains(&resp.status().as_u16());
                    let unknown_sentry = !RESPONSES.contains(&resp); // !contains == unknown
                    let should_process_response = contains_sentry && unknown_sentry;

                    if should_process_response {
                        // print to stdout
                        ferox_print(&resp.as_str(), &PROGRESS_PRINTER);

                        send_command!(tx_stats, UpdateUsizeField(ResourcesDiscovered, 1));

                        if self.save_output {
                            // -o used, need to send the report to be written out to disk
                            self.tx_file
                                .send(Command::Report(resp.clone()))
                                .with_context(|| {
                                    fmt_err(&format!("Could not send {} to file handler", resp))
                                })?;
                        }
                    }
                    log::trace!("report complete: {}", resp.url());

                    if CONFIGURATION.replay_client.is_some() && should_process_response {
                        // replay proxy specified/client created and this response's status code is one that
                        // should be replayed
                        make_request(
                            CONFIGURATION.replay_client.as_ref().unwrap(),
                            &resp.url(),
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
                        resp.text = String::new();

                        RESPONSES.insert(*resp);
                    }
                }
                Command::Exit => {
                    if self.save_output {
                        self.tx_file.send(Command::Exit)?; // kill file handler
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
