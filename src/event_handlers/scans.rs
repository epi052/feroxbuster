use super::command::Command::UpdateUsizeField;
use super::*;
use crate::utils::get_url_depth;
use crate::{
    config::CONFIGURATION,
    scan_manager::{FeroxScan, FeroxScans, ScanOrder},
    scanner::scan_url,
    statistics::StatField::TotalScans,
    CommandReceiver, CommandSender, FeroxChannel, FeroxResponse, Joiner,
};
use anyhow::{bail, Result};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug)]
/// Container for recursion transmitter and FeroxScans object
pub struct ScanHandle {
    /// FeroxScans object used across modules to track scans
    pub data: Arc<FeroxScans>,

    /// transmitter used to update `data`
    pub tx: CommandSender,
}

/// implementation of RecursionHandle
impl ScanHandle {
    /// Given an Arc-wrapped FeroxScans and CommandSender, create a new RecursionHandle
    pub fn new(data: Arc<FeroxScans>, tx: CommandSender) -> Self {
        Self { data, tx }
    }

    /// Send the given Command over `tx`
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command)?;
        Ok(())
    }
}

/// event handler for updating a single data structure of all FeroxScans
#[derive(Debug)]
pub struct ScanHandler {
    /// collection of FeroxScans
    data: Arc<FeroxScans>,

    /// handles to other handlers needed to kick off a scan while already past main
    handles: Arc<Handles>,

    /// Receiver half of mpsc from which `Command`s are processed
    receiver: CommandReceiver,

    /// wordlist (re)used for each scan
    wordlist: std::sync::Mutex<Option<Arc<HashSet<String>>>>,

    /// group of scans that need to be joined
    tasks: Vec<Arc<FeroxScan>>,

    /// depths associated with the initial targets provided by the user
    depths: Vec<(String, usize)>,
}

/// implementation of event handler for filters
impl ScanHandler {
    /// create new event handler
    pub fn new(data: Arc<FeroxScans>, handles: Arc<Handles>, receiver: CommandReceiver) -> Self {
        Self {
            data,
            handles,
            receiver,
            tasks: Vec::new(),
            depths: Vec::new(),
            wordlist: std::sync::Mutex::new(None),
        }
    }

    /// Set the wordlist
    fn wordlist(&self, wordlist: Arc<HashSet<String>>) {
        if let Ok(mut guard) = self.wordlist.lock() {
            if guard.is_none() {
                let _ = std::mem::replace(&mut *guard, Some(wordlist));
            }
        }
    }

    /// Initialize new `FeroxScans` and the sc side of an mpsc channel that is responsible for
    /// updates to the aforementioned object.
    pub fn initialize(handles: Arc<Handles>) -> (Joiner, ScanHandle) {
        log::trace!("enter: initialize");

        let data = Arc::new(FeroxScans::default());
        let (tx, rx): FeroxChannel<Command> = mpsc::unbounded_channel();

        let mut handler = Self::new(data.clone(), handles, rx);

        let task = tokio::spawn(async move { handler.start().await });

        let event_handle = ScanHandle::new(data, tx);

        log::trace!("exit: initialize -> ({:?}, {:?})", task, event_handle);

        (task, event_handle)
    }

    /// Start a single consumer task (sc side of mpsc)
    ///
    /// The consumer simply receives `Command` and acts accordingly
    pub async fn start(&mut self) -> Result<()> {
        log::trace!("enter: start({:?})", self);

        while let Some(command) = self.receiver.recv().await {
            match command {
                Command::ScanUrl(url, sender) => {
                    self.ordered_scan_url(vec![url], ScanOrder::Latest).await?;
                    sender.send(true).expect("oneshot channel failed");
                }
                Command::ScanInitialUrls(targets) => {
                    self.ordered_scan_url(targets, ScanOrder::Initial).await?;
                }
                Command::UpdateWordlist(wordlist) => {
                    self.wordlist(wordlist);
                }
                Command::JoinTasks(sender) => {
                    let ferox_scans = self.handles.ferox_scans().unwrap_or_default();

                    tokio::spawn(async move {
                        while ferox_scans.has_active_scans() {
                            for scan in ferox_scans.get_active_scans() {
                                scan.join().await;
                            }
                        }
                        sender.send(true).expect("oneshot channel failed");
                    });
                }
                Command::TryRecursion(response) => {
                    self.try_recursion(response).await?;
                }
                _ => {} // no other commands needed for RecursionHandler
            }
        }

        log::trace!("exit: start");
        Ok(())
    }

    /// Helper to easily get the (locked) underlying wordlist
    pub fn get_wordlist(&self) -> Result<Arc<HashSet<String>>> {
        if let Ok(guard) = self.wordlist.lock().as_ref() {
            if let Some(list) = guard.as_ref() {
                return Ok(list.clone());
            }
        }

        bail!("Could not get underlying wordlist")
    }

    /// wrapper around scanning a url to stay DRY
    async fn ordered_scan_url(&mut self, targets: Vec<String>, order: ScanOrder) -> Result<()> {
        for target in targets {
            let (unknown, scan) = self
                .data
                .add_directory_scan(&target, self.handles.stats.data.clone());

            if !unknown {
                // not unknown, i.e. we've seen the url before and don't need to scan again
                continue;
            }

            let list = self.get_wordlist()?;

            log::info!("scan handler received {} - beginning scan", target);

            if matches!(order, ScanOrder::Initial) {
                self.depths.push((target.clone(), get_url_depth(&target)));
            }

            let handles_clone = self.handles.clone();

            let task = tokio::spawn(async move {
                if let Err(e) = scan_url(&target, order, list, handles_clone).await {
                    log::warn!("{}", e);
                }
            });

            self.handles.stats.send(UpdateUsizeField(TotalScans, 1))?;

            scan.set_task(task).await?;

            self.tasks.push(scan.clone());
        }
        Ok(())
    }

    async fn try_recursion(&mut self, response: FeroxResponse) -> Result<()> {
        log::trace!("enter: try_recursion({:?})", response,);

        // todo get depth from self.depths
        let mut base_depth = 1_usize;

        for (base_url, base_url_depth) in &self.depths {
            if response.url().as_str().starts_with(base_url) {
                base_depth = *base_url_depth;
            }
        }

        // todo remove CONFIG dependence, maybe in init
        if response.reached_max_depth(base_depth, CONFIGURATION.depth) {
            // at or past recursion depth
            return Ok(());
        }

        if !response.is_directory() {
            // not a directory
            return Ok(());
        }

        let targets = vec![response.url().to_string()];
        self.ordered_scan_url(targets, ScanOrder::Latest).await?;

        log::info!("Added new directory to recursive scan: {}", response.url());

        log::trace!("exit: try_recursion");
        Ok(())
    }
}
