use std::{
    io::stderr,
    sync::{atomic::Ordering, Arc},
};

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use tokio::{io, sync::oneshot};
use tokio_util::codec::{FramedRead, LinesCodec};

use feroxbuster::{
    banner::{Banner, UPDATE_URL},
    config::{Configuration, OutputLevel},
    event_handlers::{
        Command::{CreateBar, Exit, JoinTasks, LoadStats, ScanInitialUrls},
        FiltersHandler, Handles, ScanHandler, StatsHandler, Tasks, TermInputHandler,
        TermOutHandler, SCAN_COMPLETE,
    },
    filters, heuristics, logger,
    progress::{PROGRESS_BAR, PROGRESS_PRINTER},
    scan_manager::{self},
    scanner,
    utils::fmt_err,
};
#[cfg(not(target_os = "windows"))]
use feroxbuster::{utils::set_open_file_limit, DEFAULT_OPEN_FILE_LIMIT};

/// Determine whether it's a single url scan or urls are coming from stdin, then scan as needed
async fn scan(targets: Vec<String>, handles: Arc<Handles>) -> Result<()> {
    log::trace!("enter: scan({:?}, {:?})", targets, handles);

    let wordlist_len = handles.stats.data.expected_per_scan();

    if wordlist_len == 0 {
        bail!("Did not find any words in {}", handles.config.wordlist);
    }

    let scanned_urls = handles.ferox_scans()?;

    // todo may need to sync here or in scans.rs

    // at this point, the stat thread's progress bar can be created; things that needed to happen
    // first:
    // - banner gets printed
    // - scanner initialized (this sent expected requests per directory to the stats thread, which
    //   having been set, makes it so the progress bar doesn't flash as full before anything has
    //   even happened
    if matches!(handles.config.output_level, OutputLevel::Default) {
        // only create the bar if no --silent|--quiet
        handles.stats.send(CreateBar)?;

        // blocks until the bar is created / avoids race condition in first two bars
        handles.stats.sync().await?;
    }

    if handles.config.resumed {
        // display what has already been completed
        scanned_urls.print_known_responses();
        scanned_urls.print_completed_bars(wordlist_len)?;
    }

    log::debug!("sending {:?} to be scanned as initial targets", targets);
    handles.send_scan_command(ScanInitialUrls(targets))?;

    log::trace!("exit: scan");

    Ok(())
}

/// Get targets from either commandline or stdin, pass them back to the caller as a Result<Vec>
async fn get_targets(handles: Arc<Handles>) -> Result<Vec<String>> {
    log::trace!("enter: get_targets({:?})", handles);

    let mut targets = vec![];

    if handles.config.stdin {
        // got targets from stdin, i.e. cat sites | ./feroxbuster ...
        // just need to read the targets from stdin and spawn a future for each target found
        let stdin = io::stdin(); // tokio's stdin, not std
        let mut reader = FramedRead::new(stdin, LinesCodec::new());

        while let Some(line) = reader.next().await {
            targets.push(line?);
        }
    } else if handles.config.resumed {
        // resume-from can't be used with --url, and --stdin is marked false for every resumed
        // scan, making it mutually exclusive from either of the other two options
        let ferox_scans = handles.ferox_scans()?;

        if let Ok(scans) = ferox_scans.scans.read() {
            for scan in scans.iter() {
                // ferox_scans gets deserialized scans added to it at program start if --resume-from
                // is used, so scans that aren't marked complete still need to be scanned
                if scan.is_complete() {
                    // this one's already done, ignore it
                    continue;
                }

                targets.push(scan.url().to_owned());
            }
        };
    } else {
        targets.push(handles.config.target_url.clone());
    }

    log::trace!("exit: get_targets -> {:?}", targets);

    Ok(targets)
}

/// async main called from real main, broken out in this way to allow for some synchronous code
/// to be executed before bringing the tokio runtime online
async fn wrapped_main(config: Arc<Configuration>) -> Result<()> {
    // join can only be called once, otherwise it causes the thread to panic
    tokio::task::spawn_blocking(move || {
        // ok, lazy_static! uses (unsurprisingly in retrospect) a lazy loading model where the
        // thing obtained through deref isn't actually created until it's used. This created a
        // problem when initializing the logger as it relied on PROGRESS_PRINTER which may or may
        // not have been created by the time it was needed for logging (really only occurred in
        // heuristics / banner / main). In order to initialize logging properly, we need to ensure
        // PROGRESS_PRINTER and PROGRESS_BAR have been used at least once.  This call satisfies
        // that constraint
        PROGRESS_PRINTER.println("");
        PROGRESS_BAR.join().unwrap();
    });

    // spawn all event handlers, expect back a JoinHandle and a *Handle to the specific event
    let (stats_task, stats_handle) = StatsHandler::initialize(config.clone());
    let (filters_task, filters_handle) = FiltersHandler::initialize();
    let (out_task, out_handle) =
        TermOutHandler::initialize(config.clone(), stats_handle.tx.clone());

    // bundle up all the disparate handles and JoinHandles (tasks)
    let handles = Arc::new(Handles::new(
        stats_handle,
        filters_handle,
        out_handle,
        config.clone(),
    ));

    let (scan_task, scan_handle) = ScanHandler::initialize(handles.clone());

    handles.set_scan_handle(scan_handle); // must be done after Handles initialization

    filters::initialize(handles.clone()).await?; // send user-supplied filters to the handler

    // create new Tasks object, each of these handles is one that will be joined on later
    let tasks = Tasks::new(out_task, stats_task, filters_task, scan_task);

    if !config.time_limit.is_empty() {
        // --time-limit value not an empty string, need to kick off the thread that enforces
        // the limit
        let time_handles = handles.clone();
        tokio::spawn(async move { scan_manager::start_max_time_thread(time_handles).await });
    }

    // can't trace main until after logger is initialized and the above task is started
    log::trace!("enter: main");

    // spawn a thread that listens for keyboard input on stdin, when a user presses enter
    // the input handler will toggle PAUSE_SCAN, which in turn is used to pause and resume
    // scans that are already running
    // also starts ctrl+c handler
    TermInputHandler::initialize(handles.clone());

    if config.resumed {
        let scanned_urls = handles.ferox_scans()?;
        let from_here = config.resume_from.clone();

        // populate FeroxScans object with previously seen scans
        scanned_urls.add_serialized_scans(&from_here)?;

        // populate Stats object with previously known statistics
        handles.stats.send(LoadStats(from_here))?;
    }

    // get targets from command line or stdin
    let targets = match get_targets(handles.clone()).await {
        Ok(t) => t,
        Err(e) => {
            // should only happen in the event that there was an error reading from stdin
            clean_up(handles, tasks).await?;
            bail!("Could not get determine initial targets: {}", e);
        }
    };

    if matches!(config.output_level, OutputLevel::Default) {
        // only print banner if output level is default (no banner on --quiet|--silent)
        let std_stderr = stderr(); // std::io::stderr

        let mut banner = Banner::new(&targets, &config);

        // only interested in the side-effect that sets banner.update_status
        let _ = banner.check_for_updates(UPDATE_URL, handles.clone()).await;

        if banner.print_to(std_stderr, config.clone()).is_err() {
            clean_up(handles, tasks).await?;
            bail!(fmt_err("Could not print banner"));
        }
    }

    {
        let send_to_file = !config.output.is_empty();

        // The TermOutHandler spawns a FileOutHandler, so errors in the FileOutHandler never bubble
        // up due to the TermOutHandler never awaiting the result of FileOutHandler::start (that's
        // done later here in main). Ping checks that the tx/rx connection to the file handler works
        if send_to_file && handles.output.sync(send_to_file).await.is_err() {
            // output file specified and file handler could not initialize
            clean_up(handles, tasks).await?;
            let msg = format!("Couldn't start {} file handler", config.output);
            bail!(fmt_err(&msg));
        }
    }

    // discard non-responsive targets
    let live_targets = {
        let test = heuristics::HeuristicTests::new(handles.clone());
        let result = test.connectivity(&targets).await;
        if result.is_err() {
            clean_up(handles, tasks).await?;
            bail!(fmt_err(&result.unwrap_err().to_string()));
        }
        result?
    };

    if live_targets.is_empty() {
        clean_up(handles, tasks).await?;
        bail!(fmt_err("Could not find any live targets to scan"));
    }

    // kick off a scan against any targets determined to be responsive
    match scan(live_targets, handles.clone()).await {
        Ok(_) => {}
        Err(e) => {
            clean_up(handles, tasks).await?;
            bail!(fmt_err(&format!("Failed while scanning: {}", e)));
        }
    }

    clean_up(handles, tasks).await?;

    log::trace!("exit: wrapped_main");
    Ok(())
}

/// Single cleanup function that handles all the necessary drops/finishes etc required to gracefully
/// shutdown the program
async fn clean_up(handles: Arc<Handles>, tasks: Tasks) -> Result<()> {
    log::trace!("enter: clean_up({:?}, {:?})", handles, tasks);

    let (tx, rx) = oneshot::channel::<bool>();
    handles.send_scan_command(JoinTasks(tx))?;
    rx.await?;

    log::info!("All scans complete!");

    // terminal handler closes file handler if one is in use
    handles.output.send(Exit)?;
    tasks.terminal.await??;
    log::trace!("terminal handler closed");

    handles.filters.send(Exit)?;
    tasks.filters.await??;
    log::trace!("filters handler closed");

    handles.stats.send(Exit)?;
    tasks.stats.await??;
    log::trace!("stats handler closed");

    // mark all scans complete so the terminal input handler will exit cleanly
    SCAN_COMPLETE.store(true, Ordering::Relaxed);

    // clean-up function for the MultiProgress bar; must be called last in order to still see
    // the final trace messages above
    PROGRESS_PRINTER.finish();

    log::trace!("exit: clean_up");
    Ok(())
}

fn main() -> Result<()> {
    let config = Arc::new(Configuration::new().with_context(|| "Could not create Configuration")?);

    // setup logging based on the number of -v's used
    if matches!(
        config.output_level,
        OutputLevel::Default | OutputLevel::Quiet
    ) {
        // don't log on --silent
        logger::initialize(config.clone())?;
    }

    // this function uses rlimit, which is not supported on windows
    #[cfg(not(target_os = "windows"))]
    set_open_file_limit(DEFAULT_OPEN_FILE_LIMIT);

    if let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        let future = wrapped_main(config);
        if let Err(e) = runtime.block_on(future) {
            eprintln!("{}", e);
        };
    }

    log::trace!("exit: main");

    Ok(())
}
