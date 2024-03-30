use super::Command::AddToUsizeField;
use super::*;

use anyhow::{Context, Result};
use futures::future::{BoxFuture, FutureExt};
use tokio::sync::{mpsc, oneshot};

use crate::{
    config::Configuration,
    progress::PROGRESS_PRINTER,
    response::FeroxResponse,
    scanner::RESPONSES,
    send_command, skip_fail,
    statistics::StatField::{ResourcesDiscovered, TotalExpected},
    traits::FeroxSerialize,
    utils::{ferox_print, fmt_err, make_request, open_file, write_to},
    CommandReceiver, CommandSender, Joiner,
};
use std::sync::Arc;
use url::Url;

#[derive(Debug, Copy, Clone)]
/// Simple enum for semantic clarity around calling expectations for `process_response`
enum ProcessResponseCall {
    /// call should allow recursion
    Recursive,

    /// call should not allow recursion
    NotRecursive,
}

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

        write_to(&*self.config, &mut file, self.config.json)?;

        while let Some(command) = self.receiver.recv().await {
            match command {
                Command::Report(response) => {
                    skip_fail!(write_to(&*response, &mut file, self.config.json));
                }
                Command::WriteToDisk(message) => {
                    // todo consider making report accept dyn FeroxSerialize; would mean adding
                    //  as_any/box_eq/PartialEq to the trait and then adding them to the
                    //  implementing structs
                    skip_fail!(write_to(&*message, &mut file, self.config.json));
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

    /// handles instance
    handles: Option<Arc<Handles>>,
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
            handles: None,
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
                Command::Report(resp) => {
                    if let Err(err) = self
                        .process_response(tx_stats.clone(), resp, ProcessResponseCall::Recursive)
                        .await
                    {
                        log::warn!("{}", err);
                    }
                }
                Command::Sync(sender) => {
                    sender.send(true).unwrap_or_default();
                }
                Command::AddHandles(handles) => {
                    self.handles = Some(handles);
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

    /// upon receiving a `FeroxResponse` from the mpsc, handle printing, sending to the replay
    /// proxy, checking for backups of the `FeroxResponse`'s url, and tracking the response.
    fn process_response(
        &self,
        tx_stats: CommandSender,
        mut resp: Box<FeroxResponse>,
        call_type: ProcessResponseCall,
    ) -> BoxFuture<'_, Result<()>> {
        log::trace!("enter: process_response({:?}, {:?})", resp, call_type);

        async move {
            let contains_sentry = if !self.config.filter_status.is_empty() {
                // -C was used, meaning -s was not and we should ignore the defaults
                // https://github.com/epi052/feroxbuster/issues/535
                // -C indicates that we should filter that status code, but allow all others
                !self.config.filter_status.contains(&resp.status().as_u16())
            } else {
                // -C wasn't used, so, we defer to checking the -s values
                self.config.status_codes.contains(&resp.status().as_u16())
            };

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
                            fmt_err(&format!("Could not send {resp} to file handler"))
                        })?;
                }
            }
            log::trace!("report complete: {}", resp.url());

            if self.config.replay_client.is_some() && should_process_response {
                // replay proxy specified/client created and this response's status code is one that
                // should be replayed; not using logged_request due to replay proxy client
                let data = if self.config.data.is_empty() {
                    None
                } else {
                    Some(self.config.data.as_slice())
                };

                make_request(
                    self.config.replay_client.as_ref().unwrap(),
                    resp.url(),
                    resp.method().as_str(),
                    data,
                    self.config.output_level,
                    &self.config,
                    tx_stats.clone(),
                )
                .await
                .with_context(|| "Could not replay request through replay proxy")?;
            }

            if self.config.collect_backups
                && should_process_response
                && matches!(call_type, ProcessResponseCall::Recursive)
            {
                // --collect-backups was used; the response is one we care about, and the function
                // call came from the loop in `.start` (i.e. recursive was specified)
                let backup_urls = self.generate_backup_urls(&resp).await;

                // need to manually adjust stats
                send_command!(tx_stats, AddToUsizeField(TotalExpected, backup_urls.len()));

                for backup_url in &backup_urls {
                    let backup_response = make_request(
                        &self.config.client,
                        backup_url,
                        resp.method().as_str(),
                        None,
                        self.config.output_level,
                        &self.config,
                        tx_stats.clone(),
                    )
                    .await
                    .with_context(|| {
                        format!("Could not request backup of {}", resp.url().as_str())
                    })?;

                    let ferox_response = FeroxResponse::from(
                        backup_response,
                        resp.url().as_str(),
                        resp.method().as_str(),
                        resp.output_level,
                    )
                    .await;

                    let Some(handles) = self.handles.as_ref() else {
                        // shouldn't ever happen, but we'll log and return early if it does
                        log::error!("handles were unexpectedly None, this shouldn't happen");
                        return Ok(());
                    };

                    if handles
                        .filters
                        .data
                        .should_filter_response(&ferox_response, tx_stats.clone())
                    {
                        // response was filtered for one reason or another, don't process it
                        continue;
                    }

                    self.process_response(
                        tx_stats.clone(),
                        Box::new(ferox_response),
                        ProcessResponseCall::NotRecursive,
                    )
                    .await?;
                }
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
            log::trace!("exit: process_response");
            Ok(())
        }
        .boxed()
    }

    /// internal helper to stay DRY
    fn add_new_url_to_vec(&self, url: &Url, new_name: &str, urls: &mut Vec<Url>) {
        if let Ok(joined) = url.join(new_name) {
            urls.push(joined);
        }
    }

    /// given a `FeroxResponse`, generate either 6 or 7 urls that are likely backups of the
    /// original.
    ///
    /// example:
    ///     original: LICENSE.txt
    ///     backups:    
    ///         - LICENSE.txt~
    ///         - LICENSE.txt.bak
    ///         - LICENSE.txt.bak2
    ///         - LICENSE.txt.old
    ///         - LICENSE.txt.1
    ///         - LICENSE.bak
    ///         - .LICENSE.txt.swp
    async fn generate_backup_urls(&self, response: &FeroxResponse) -> Vec<Url> {
        log::trace!("enter: generate_backup_urls({:?})", response);

        let mut urls = vec![];
        let url = response.url();

        // confirmed safe: see src/response.rs for comments
        let filename = url.path_segments().unwrap().last().unwrap();

        if !filename.is_empty() {
            // append rules
            for suffix in &self.config.backup_extensions {
                self.add_new_url_to_vec(url, &format!("{filename}{suffix}"), &mut urls);
            }

            // vim swap rule
            self.add_new_url_to_vec(url, &format!(".{filename}.swp"), &mut urls);

            // replace original extension rule
            let parts: Vec<_> = filename
                .split('.')
                // keep things like /.bash_history out of results
                .filter(|part| !part.is_empty())
                .collect();

            if parts.len() > 1 {
                // filename + at least one extension, i.e. whatever.js becomes ["whatever", "js"]
                self.add_new_url_to_vec(url, &format!("{}.bak", parts.first().unwrap()), &mut urls);
            }
        }

        log::trace!("exit: generate_backup_urls -> {:?}", urls);
        urls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_handlers::Command;

    #[test]
    /// try to hit struct field coverage of FileOutHandler
    fn struct_fields_of_file_out_handler() {
        let (_, rx) = mpsc::unbounded_channel::<Command>();
        let config = Arc::new(Configuration::new().unwrap());
        let foh = FileOutHandler {
            config,
            receiver: rx,
        };
        println!("{foh:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// try to hit struct field coverage of TermOutHandler
    async fn struct_fields_of_term_out_handler() {
        let (tx, rx) = mpsc::unbounded_channel::<Command>();
        let (tx_file, _) = mpsc::unbounded_channel::<Command>();
        let config = Arc::new(Configuration::new().unwrap());
        let handles = Arc::new(Handles::for_testing(None, None).0);

        let toh = TermOutHandler {
            config,
            file_task: None,
            receiver: rx,
            tx_file,
            handles: Some(handles),
        };

        println!("{toh:?}");
        tx.send(Command::Exit).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// when the feroxresponse's url contains an extension, there should be 7 urls returned
    async fn generate_backup_urls_creates_correct_urls_when_extension_present() {
        let (tx, rx) = mpsc::unbounded_channel::<Command>();
        let (tx_file, _) = mpsc::unbounded_channel::<Command>();
        let config = Arc::new(Configuration::new().unwrap());
        let handles = Arc::new(Handles::for_testing(None, None).0);

        let toh = TermOutHandler {
            config,
            file_task: None,
            receiver: rx,
            tx_file,
            handles: Some(handles),
        };

        let expected: Vec<_> = vec![
            "derp.php~",
            "derp.php.bak",
            "derp.php.bak2",
            "derp.php.old",
            "derp.php.1",
            ".derp.php.swp",
            "derp.bak",
        ];

        let mut fr = FeroxResponse::default();
        fr.set_url("http://localhost/derp.php");

        let urls = toh.generate_backup_urls(&fr).await;

        let paths: Vec<_> = urls
            .iter()
            .map(|url| url.path_segments().unwrap().last().unwrap())
            .collect();

        assert_eq!(urls.len(), 7);

        for path in paths {
            assert!(expected.contains(&path));
        }

        tx.send(Command::Exit).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// when the feroxresponse's url doesn't contain an extension, there should be 6 urls returned
    async fn generate_backup_urls_creates_correct_urls_when_extension_not_present() {
        let (tx, rx) = mpsc::unbounded_channel::<Command>();
        let (tx_file, _) = mpsc::unbounded_channel::<Command>();
        let config = Arc::new(Configuration::new().unwrap());
        let handles = Arc::new(Handles::for_testing(None, None).0);

        let toh = TermOutHandler {
            config,
            file_task: None,
            receiver: rx,
            tx_file,
            handles: Some(handles),
        };

        let expected: Vec<_> = vec![
            "derp~",
            "derp.bak",
            "derp.bak2",
            "derp.old",
            "derp.1",
            ".derp.swp",
        ];

        let mut fr = FeroxResponse::default();
        fr.set_url("http://localhost/derp");

        let urls = toh.generate_backup_urls(&fr).await;

        let paths: Vec<_> = urls
            .iter()
            .map(|url| url.path_segments().unwrap().last().unwrap())
            .collect();

        assert_eq!(urls.len(), 6);

        for path in paths {
            assert!(expected.contains(&path));
        }

        tx.send(Command::Exit).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test to ensure that backups are requested from the directory in which they were found
    /// re: issue #513
    async fn generate_backup_urls_creates_correct_urls_when_not_at_root() {
        let (tx, rx) = mpsc::unbounded_channel::<Command>();
        let (tx_file, _) = mpsc::unbounded_channel::<Command>();
        let config = Arc::new(Configuration::new().unwrap());
        let handles = Arc::new(Handles::for_testing(None, None).0);

        let toh = TermOutHandler {
            config,
            file_task: None,
            receiver: rx,
            tx_file,
            handles: Some(handles),
        };

        let expected: Vec<_> = vec![
            "http://localhost/wordpress/derp.php~",
            "http://localhost/wordpress/derp.php.bak",
            "http://localhost/wordpress/derp.php.bak2",
            "http://localhost/wordpress/derp.php.old",
            "http://localhost/wordpress/derp.php.1",
            "http://localhost/wordpress/.derp.php.swp",
            "http://localhost/wordpress/derp.bak",
        ];

        let mut fr = FeroxResponse::default();
        fr.set_url("http://localhost/wordpress/derp.php");

        let urls = toh.generate_backup_urls(&fr).await;

        let url_strs: Vec<_> = urls.iter().map(|url| url.as_str()).collect();

        assert_eq!(urls.len(), 7);

        for url_str in url_strs {
            assert!(expected.contains(&url_str));
        }

        tx.send(Command::Exit).unwrap();
    }
}
