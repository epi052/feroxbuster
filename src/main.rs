use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyCode};
use feroxbuster::{
    banner::{Banner, UPDATE_URL},
    config::{CONFIGURATION, PROGRESS_BAR, PROGRESS_PRINTER},
    event_handlers::{
        Command::{self, CreateBar, Exit, LoadStats, UpdateUsizeField},
        FiltersHandler, Handles, StatsHandler, Tasks,
    },
    heuristics, logger,
    progress::{add_bar, BarType},
    reporter,
    scan_manager::{self, ScanStatus, PAUSE_SCAN},
    scanner::{self, scan_url, SCANNED_URLS},
    send_command,
    statistics::{StatField::InitialTargets, Stats},
    utils::{ferox_print, fmt_err, get_current_depth, status_colorizer},
    FeroxError, FeroxResponse, FeroxResult, SLEEP_DURATION,
};
#[cfg(not(target_os = "windows"))]
use feroxbuster::{utils::set_open_file_limit, DEFAULT_OPEN_FILE_LIMIT};
use futures::StreamExt;
use std::{
    collections::HashSet,
    convert::TryInto,
    fs::File,
    io::{stderr, BufRead, BufReader},
    process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};
use tokio::{io, sync::mpsc::UnboundedSender};
use tokio_util::codec::{FramedRead, LinesCodec};

/// Atomic boolean flag, used to determine whether or not the terminal input handler should exit
pub static SCAN_COMPLETE: AtomicBool = AtomicBool::new(false);

/// Handles specific key events triggered by the user over stdin
fn terminal_input_handler() {
    log::trace!("enter: terminal_input_handler");

    loop {
        if PAUSE_SCAN.load(Ordering::Relaxed) {
            // if the scan is already paused, we don't want this event poller fighting the user
            // over stdin
            sleep(Duration::from_millis(SLEEP_DURATION));
        } else if event::poll(Duration::from_millis(SLEEP_DURATION)).unwrap_or(false) {
            // It's guaranteed that the `read()` won't block when the `poll()`
            // function returns `true`

            if let Ok(key_pressed) = event::read() {
                // ignore any other keys
                if key_pressed == Event::Key(KeyCode::Enter.into()) {
                    // if the user presses Enter, set PAUSE_SCAN to true. The interactive menu
                    // will be triggered and will handle setting PAUSE_SCAN to false
                    PAUSE_SCAN.store(true, Ordering::Release);
                }
            }
        } else {
            // Timeout expired and no `Event` is available; use the timeout to check SCAN_COMPLETE
            if SCAN_COMPLETE.load(Ordering::Relaxed) {
                // scan has been marked complete by main, time to exit the loop
                break;
            }
        }
    }
    log::trace!("exit: terminal_input_handler");
}

/// Create a HashSet of Strings from the given wordlist then stores it inside an Arc
fn get_unique_words_from_wordlist(path: &str) -> Result<Arc<HashSet<String>>> {
    log::trace!("enter: get_unique_words_from_wordlist({})", path);

    let file = File::open(&path)?;

    let reader = BufReader::new(file);

    let mut words = HashSet::new();

    for line in reader.lines() {
        let result = line?;

        if result.starts_with('#') || result.is_empty() {
            continue;
        }

        words.insert(result);
    }

    log::trace!(
        "exit: get_unique_words_from_wordlist -> Arc<wordlist[{} words...]>",
        words.len()
    );

    Ok(Arc::new(words))
}

/// Determine whether it's a single url scan or urls are coming from stdin, then scan as needed
async fn scan(
    targets: Vec<String>,
    tx_term: UnboundedSender<FeroxResponse>,
    tx_file: UnboundedSender<FeroxResponse>,
    handles: Handles,
) -> Result<()> {
    log::trace!(
        "enter: scan({:?}, {:?}, {:?}, {:?})",
        targets,
        tx_term,
        tx_file,
        handles
    );
    // cloning an Arc is cheap (it's basically a pointer into the heap)
    // so that will allow for cheap/safe sharing of a single wordlist across multi-target scans
    // as well as additional directories found as part of recursion
    let words =
        tokio::spawn(async move { get_unique_words_from_wordlist(&CONFIGURATION.wordlist) })
            .await??;

    if words.len() == 0 {
        bail!("Did not find any words in {}", CONFIGURATION.wordlist);
    }

    scanner::initialize(words.len(), &CONFIGURATION, handles.stats.tx.clone()).await;

    // at this point, the stat thread's progress bar can be created; things that needed to happen
    // first:
    // - banner gets printed
    // - scanner initialized (this sent expected requests per directory to the stats thread, which
    //   having been set, makes it so the progress bar doesn't flash as full before anything has
    //   even happened
    handles.stats.send(CreateBar)?;

    if CONFIGURATION.resumed {
        handles
            .stats
            .send(LoadStats(CONFIGURATION.resume_from.clone()))?;

        SCANNED_URLS.print_known_responses();

        if let Ok(scans) = SCANNED_URLS.scans.lock() {
            for scan in scans.iter() {
                if let Ok(locked_scan) = scan.lock() {
                    if matches!(locked_scan.status, ScanStatus::Complete) {
                        // these scans are complete, and just need to be shown to the user
                        let pb = add_bar(
                            &locked_scan.url,
                            words.len().try_into().unwrap_or_default(),
                            BarType::Message,
                        );
                        pb.finish();
                    }
                }
            }
        }
    }

    let mut tasks = vec![];

    for target in targets {
        let word_clone = words.clone();
        let term_clone = tx_term.clone();
        let file_clone = tx_file.clone();
        let tx_stats_clone = handles.stats.tx.clone();
        let stats_clone = handles.stats.data.clone();

        let task = tokio::spawn(async move {
            let base_depth = get_current_depth(&target);
            scan_url(
                &target,
                word_clone,
                base_depth,
                stats_clone,
                term_clone,
                file_clone,
                tx_stats_clone,
            )
            .await;
        });

        tasks.push(task);
    }

    // drive execution of all accumulated futures
    futures::future::join_all(tasks).await;
    log::trace!("exit: scan");

    Ok(())
}

/// Get targets from either commandline or stdin, pass them back to the caller as a Result<Vec>
async fn get_targets() -> FeroxResult<Vec<String>> {
    log::trace!("enter: get_targets");

    let mut targets = vec![];

    if CONFIGURATION.stdin {
        // got targets from stdin, i.e. cat sites | ./feroxbuster ...
        // just need to read the targets from stdin and spawn a future for each target found
        let stdin = io::stdin(); // tokio's stdin, not std
        let mut reader = FramedRead::new(stdin, LinesCodec::new());

        while let Some(line) = reader.next().await {
            targets.push(line?);
        }
    } else if CONFIGURATION.resumed {
        // resume-from can't be used with --url, and --stdin is marked false for every resumed
        // scan, making it mutually exclusive from either of the other two options
        if let Ok(scans) = SCANNED_URLS.scans.lock() {
            for scan in scans.iter() {
                // SCANNED_URLS gets deserialized scans added to it at program start if --resume-from
                // is used, so scans that aren't marked complete still need to be scanned
                if let Ok(locked_scan) = scan.lock() {
                    if matches!(locked_scan.status, ScanStatus::Complete) {
                        // this one's already done, ignore it
                        continue;
                    }
                    targets.push(locked_scan.url.to_owned());
                }
            }
        }
    } else {
        targets.push(CONFIGURATION.target_url.clone());
    }

    log::trace!("exit: get_targets -> {:?}", targets);

    Ok(targets)
}

/// async main called from real main, broken out in this way to allow for some synchronous code
/// to be executed before bringing the tokio runtime online
async fn wrapped_main() -> Result<()> {
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

    let (stats_task, stats_handle) = StatsHandler::initialize();
    let (filters_task, filters_handle) = FiltersHandler::initialize();
    let handles = Handles::new(stats_handle, filters_handle);

    if !CONFIGURATION.time_limit.is_empty() {
        // --time-limit value not an empty string, need to kick off the thread that enforces
        // the limit

        let max_time_stats = handles.stats.data.clone();

        tokio::spawn(async move {
            scan_manager::start_max_time_thread(&CONFIGURATION.time_limit, max_time_stats).await
        });
    }

    // can't trace main until after logger is initialized and the above task is started
    log::trace!("enter: main");

    // spawn a thread that listens for keyboard input on stdin, when a user presses enter
    // the input handler will toggle PAUSE_SCAN, which in turn is used to pause and resume
    // scans that are already running
    tokio::task::spawn_blocking(terminal_input_handler);

    let save_output = !CONFIGURATION.output.is_empty(); // was -o used?

    if CONFIGURATION.save_state {
        // start the ctrl+c handler
        scan_manager::initialize(handles.stats.data.clone());
    }

    // todo transmitters here added to handles and made event handlers
    let (tx_term, tx_file, term_handle, file_handle) =
        reporter::initialize(&CONFIGURATION.output, save_output, handles.stats.tx.clone());

    let tasks = Tasks::new(term_handle, file_handle, stats_task, filters_task);

    // get targets from command line or stdin
    let targets = match get_targets().await {
        Ok(t) => t,
        Err(e) => {
            // should only happen in the event that there was an error reading from stdin
            clean_up(tx_term, tx_file, handles, tasks).await?;
            bail!("Could not get determine initial targets: {}", e);
        }
    };

    send_command!(
        handles.stats.tx,
        UpdateUsizeField(InitialTargets, targets.len())
    );

    if !CONFIGURATION.quiet {
        // only print banner if -q isn't used
        let std_stderr = stderr(); // std::io::stderr

        let mut banner = Banner::new(&targets, &CONFIGURATION);

        // only interested in the side-effect that sets banner.update_status
        let _ = banner
            .check_for_updates(&CONFIGURATION.client, UPDATE_URL, handles.stats.tx.clone())
            .await;

        banner.print_to(std_stderr, &CONFIGURATION)?;
    }

    // discard non-responsive targets
    let live_targets = heuristics::connectivity_test(&targets, handles.stats.tx.clone()).await;

    if live_targets.is_empty() {
        clean_up(tx_term, tx_file, handles, tasks).await?;
        bail!(fmt_err("Could not find any live targets to scan"));
    }

    // kick off a scan against any targets determined to be responsive
    scan(
        live_targets,
        tx_term.clone(),
        tx_file.clone(),
        handles.clone(),
    )
    .await?;

    log::info!("All scans complete!");
    clean_up(tx_term, tx_file, handles, tasks).await?;

    log::trace!("exit: wrapped_main");
    Ok(())
}

/// Single cleanup function that handles all the necessary drops/finishes etc required to gracefully
/// shutdown the program
async fn clean_up(
    tx_term: UnboundedSender<FeroxResponse>, // todo replace all of these with Handles obj
    tx_file: UnboundedSender<FeroxResponse>,
    handles: Handles,
    tasks: Tasks,
) -> Result<()> {
    log::trace!(
        "enter: clean_up({:?}, {:?}, {:?})", // todo missing tasks
        tx_term,
        tx_file,
        handles,
    );
    drop(tx_term);
    tasks.terminal.await??;

    // the same drop/await process used on the terminal handler is repeated for the file handler
    // we drop the file transmitter every time, because it's created no matter what
    drop(tx_file);

    if let Some(file_task) = tasks.file {
        file_task.await??;
    }

    send_command!(handles.filters.tx, Exit);
    tasks.filters.await??;

    send_command!(handles.stats.tx, Exit);
    tasks.stats.await??;

    // mark all scans complete so the terminal input handler will exit cleanly
    SCAN_COMPLETE.store(true, Ordering::Relaxed);

    // clean-up function for the MultiProgress bar; must be called last in order to still see
    // the final trace messages above
    PROGRESS_PRINTER.finish();

    // drop(tx_stats);

    log::trace!("exit: clean_up");
    Ok(())
}

fn main() {
    // setup logging based on the number of -v's used
    logger::initialize(CONFIGURATION.verbosity);

    // this function uses rlimit, which is not supported on windows
    #[cfg(not(target_os = "windows"))]
    set_open_file_limit(DEFAULT_OPEN_FILE_LIMIT);

    if let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        let future = wrapped_main();
        if let Err(e) = runtime.block_on(future) {
            eprintln!("{}", e);
        };
    }

    log::trace!("exit: main");
}
