use std::io::stdin;
use std::{
    env::{
        args,
        consts::{ARCH, OS},
    },
    fs::{create_dir, remove_file, File},
    io::{stderr, BufRead, BufReader},
    ops::Index,
    path::Path,
    process::{exit, Command, Stdio},
    sync::{atomic::Ordering, Arc},
};

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use tokio::{
    io,
    sync::{oneshot, Semaphore},
};
use tokio_util::codec::{FramedRead, LinesCodec};

use feroxbuster::{
    banner::{Banner, UPDATE_URL},
    config::{Configuration, OutputLevel},
    event_handlers::{
        Command::{
            AddHandles, CreateBar, Exit, JoinTasks, LoadStats, ScanInitialUrls, UpdateTargets,
            UpdateWordlist,
        },
        FiltersHandler, Handles, ScanHandler, StatsHandler, Tasks, TermInputHandler,
        TermOutHandler, SCAN_COMPLETE,
    },
    filters, heuristics, logger,
    progress::PROGRESS_PRINTER,
    scan_manager::{self, ScanType},
    scanner,
    utils::{fmt_err, slugify_filename},
    SECONDARY_WORDLIST,
};
#[cfg(not(target_os = "windows"))]
use feroxbuster::{utils::set_open_file_limit, DEFAULT_OPEN_FILE_LIMIT};
use lazy_static::lazy_static;
use regex::Regex;
use self_update::cargo_crate_version;

lazy_static! {
    /// Limits the number of parallel scans active at any given time when using --parallel
    static ref PARALLEL_LIMITER: Semaphore = Semaphore::new(0);
}

/// Create a Vec of Strings from the given wordlist then stores it inside an Arc
fn get_unique_words_from_wordlist(path: &str) -> Result<Arc<Vec<String>>> {
    log::trace!("enter: get_unique_words_from_wordlist({})", path);
    let mut trimmed_word = false;

    let file = File::open(path).with_context(|| format!("Could not open {path}"))?;

    let reader = BufReader::new(file);

    // this empty string ensures that we call Requester::request with the base url, i.e.
    // `http://localhost/` instead of going straight into `http://localhost/WORD.EXT`.
    // for vanilla scans, it doesn't matter all that much, but it can be a significant difference
    // when `-e` is used, depending on the content at the base url.
    let mut words = vec![String::from("")];

    for line in reader.lines() {
        line.map(|result| {
            if !result.starts_with('#') && !result.is_empty() {
                if result.starts_with('/') {
                    words.push(result.trim_start_matches('/').to_string());
                    trimmed_word = true;
                } else {
                    words.push(result);
                }
            }
        })
        .ok();
    }

    if trimmed_word {
        log::warn!("Some words in the wordlist started with a leading forward-slash; those words were trimmed (i.e. /word -> word)");
    }

    log::trace!(
        "exit: get_unique_words_from_wordlist -> Arc<wordlist[{} words...]>",
        words.len()
    );

    Ok(Arc::new(words))
}

/// Determine whether it's a single url scan or urls are coming from stdin, then scan as needed
async fn scan(targets: Vec<String>, handles: Arc<Handles>) -> Result<()> {
    log::trace!("enter: scan({:?}, {:?})", targets, handles);

    let scanned_urls = handles.ferox_scans()?;

    handles.send_scan_command(UpdateWordlist(handles.wordlist.clone()))?;

    scanner::initialize(handles.wordlist.len(), handles.clone()).await?;

    // at this point, the stat thread's progress bar can be created; things that needed to happen
    // first:
    // - banner gets printed
    // - scanner initialized (this sent expected requests per directory to the stats thread, which
    //   having been set, makes it so the progress bar doesn't flash as full before anything has
    //   even happened
    if matches!(handles.config.output_level, OutputLevel::Default) {
        let mut total_offset = 0;

        if let Ok(guard) = handles.scans.read() {
            if let Some(handle) = &*guard {
                if let Ok(scans) = handle.data.scans.read() {
                    for scan in scans.iter() {
                        total_offset += scan.requests_made_so_far();
                    }
                }
            }
        }

        // only create the bar if no --silent|--quiet
        handles.stats.send(CreateBar(total_offset))?;

        // blocks until the bar is created / avoids race condition in first two bars
        handles.stats.sync().await?;
    }

    if handles.config.resumed {
        // display what has already been completed
        scanned_urls.print_known_responses();
        scanned_urls.print_completed_bars(handles.wordlist.len())?;
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
                if scan.is_complete() || matches!(scan.scan_type, ScanType::File) {
                    // this one's already done, or it's not a directory, ignore it
                    continue;
                }

                targets.push(scan.url().to_owned());
            }
        };
    } else {
        targets.push(handles.config.target_url.clone());
    }

    // remove footgun that arises if a --dont-scan value matches on a base url
    for target in targets.iter_mut() {
        for denier in &handles.config.regex_denylist {
            if denier.is_match(target) {
                bail!(
                    "The regex '{}' matches {}; the scan will never start",
                    denier,
                    target
                );
            }
        }
        for denier in &handles.config.url_denylist {
            if denier.as_str().trim_end_matches('/') == target.trim_end_matches('/') {
                bail!(
                    "The url '{}' matches {}; the scan will never start",
                    denier,
                    target
                );
            }
        }

        if !target.starts_with("http") {
            // --url hackerone.com
            *target = format!("{}://{target}", handles.config.protocol);
        }
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
    });

    // check if update_app is true
    if config.update_app {
        match update_app().await {
            Err(e) => eprintln!("\n[ERROR] {}", e),
            Ok(self_update::Status::UpToDate(version)) => {
                eprintln!("\nFeroxbuster {} is up to date", version)
            }
            Ok(self_update::Status::Updated(version)) => {
                eprintln!("\nFeroxbuster updated to {} version", version)
            }
        }
        exit(0);
    }

    let words = if config.wordlist.starts_with("http") {
        // found a url scheme, attempt to download the wordlist
        let response = config
            .client
            .get(&config.wordlist)
            .send()
            .await
            .context(format!(
                "Unable to download wordlist from remote url: {}",
                config.wordlist
            ))?;

        if !response.status().is_success() {
            // status code isn't a 200, bail
            bail!(
                "[{}] Unable to download wordlist from url: {}",
                response.status().as_str(),
                config.wordlist
            );
        }

        // attempt to get the filename from the url's path
        let Some(path_segments) = response.url().path_segments() else {
            bail!("Unable to parse path from url: {}", response.url());
        };

        let Some(filename) = path_segments.last() else {
            bail!(
                "Unable to parse filename from url's path: {}",
                response.url().path()
            );
        };

        let filename = filename.to_string();

        // read the body and write it to disk, then use existing code to read the wordlist
        let body = response.text().await?;

        std::fs::write(&filename, body)?;

        get_unique_words_from_wordlist(&filename)?
    } else {
        match get_unique_words_from_wordlist(&config.wordlist) {
            Ok(w) => w,
            Err(err) => {
                let secondary = Path::new(SECONDARY_WORDLIST);

                if secondary.exists() {
                    eprintln!("Found wordlist in secondary location");
                    get_unique_words_from_wordlist(SECONDARY_WORDLIST)?
                } else {
                    return Err(err);
                }
            }
        }
    };

    if words.len() <= 1 {
        // the check is now <= 1 due to the initial empty string added in 2.6.0
        // 1 -> empty wordlist
        // 0 -> error
        bail!("Did not find any words in {}", config.wordlist);
    }

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
        words,
    ));

    let (scan_task, scan_handle) = ScanHandler::initialize(handles.clone());

    handles.set_scan_handle(scan_handle); // must be done after Handles initialization
    handles.output.send(AddHandles(handles.clone()))?;

    filters::initialize(handles.clone()).await?; // send user-supplied filters to the handler

    // create new Tasks object, each of these handles is one that will be joined on later
    let tasks = Tasks::new(out_task, stats_task, filters_task, scan_task);

    if !config.time_limit.is_empty() && config.parallel == 0 {
        // --time-limit value not an empty string, need to kick off the thread that enforces
        // the limit
        //
        // if --parallel is used, this branch won't execute in the main process, but will in the
        // children. This is because --parallel is stripped from the children's command line
        // arguments, so, when spawned, they won't have --parallel, the parallel value will be set
        // to the default of 0, and will hit this branch. This makes it so that the time limit
        // is enforced on each individual child process, instead of the main process
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
        scanned_urls.add_serialized_scans(&from_here, handles.clone())?;

        // populate Stats object with previously known statistics
        handles.stats.send(LoadStats(from_here))?;
    }

    // get targets from command line or stdin
    let targets = match get_targets(handles.clone()).await {
        Ok(t) => t,
        Err(e) => {
            // should only happen in the event that there was an error reading from stdin
            clean_up(handles, tasks).await?;
            bail!("Could not determine initial targets: {}", e);
        }
    };

    // --parallel branch
    if config.parallel > 0 {
        log::trace!("enter: parallel branch");

        PARALLEL_LIMITER.add_permits(config.parallel);

        let invocation = args();

        let para_regex = Regex::new("--stdin").unwrap();

        // remove stdin since only the original process will process targets
        // remove quiet and silent so we can force silent later to normalize output
        let mut original = invocation
            .filter(|s| !para_regex.is_match(s))
            .collect::<Vec<String>>();

        // we need remove --parallel from command line so we don't hit this branch over and over
        // but we must remove --parallel N manually; the filter above never sees --parallel and the
        // value passed to it at the same time, so can't filter them out in one pass

        // unwrap is fine, as it has to be in the args for us to be in this code branch
        let parallel_index = original.iter().position(|s| *s == "--parallel").unwrap();

        // remove --parallel
        original.remove(parallel_index);

        // remove N passed to --parallel (it's the same index again since everything shifts
        // from removing --parallel)
        original.remove(parallel_index);

        // to log unique files to a shared folder, we need to first check for the presence
        // of -o|--output.
        let out_dir = if !config.output.is_empty() {
            // -o|--output was used, so we'll attempt to create a directory to store the files
            let output_path = Path::new(&handles.config.output);

            // this only returns None if the path terminates in `..`. Since I don't want to
            // hand-hold to that degree, we'll unwrap and fail if the output path ends in `..`
            let base_name = output_path.file_name().unwrap();

            let new_folder = slugify_filename(&base_name.to_string_lossy(), "", "logs");

            let final_path = output_path.with_file_name(new_folder);

            // create the directory or fail silently, assuming the reason for failure is that
            // the path exists already
            create_dir(&final_path).unwrap_or(());

            final_path.to_string_lossy().to_string()
        } else {
            String::new()
        };

        // unvalidated targets fresh from stdin, just spawn children and let them do all checks
        for target in targets {
            // add the current target to the provided command
            let mut cloned = original.clone();

            if !out_dir.is_empty() {
                // output directory value is not empty, need to join output directory with
                // unique scan filename

                // unwrap is ok, we already know -o was used
                let out_idx = original
                    .iter()
                    .position(|s| *s == "--output" || *s == "-o")
                    .unwrap();

                let filename = slugify_filename(&target, "ferox", "log");

                let full_path = Path::new(&out_dir)
                    .join(filename)
                    .to_string_lossy()
                    .to_string();

                // a +1 to the index is fine here, as clap has already validated that
                // -o|--output has a value associated with it
                cloned[out_idx + 1] = full_path;
            }

            cloned.push("-u".to_string());
            cloned.push(target);

            let bin = cloned.index(0).to_owned(); // user's path to feroxbuster
            let args = cloned.index(1..).to_vec(); // and args

            let permit = PARALLEL_LIMITER.acquire().await?;

            log::debug!("parallel exec: {} {}", bin, args.join(" "));

            tokio::task::spawn(async move {
                let mut output = Command::new(bin)
                    .args(&args)
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("failed to spawn a child process");

                let stdout = output.stdout.take().unwrap();

                let mut bufread = BufReader::new(stdout);
                // output for a single line is a minimum of 51 bytes, so we'll start with that
                // + a little wiggle room, and grow as needed
                let mut buf: String = String::with_capacity(128);

                while let Ok(n) = bufread.read_line(&mut buf) {
                    if n > 0 {
                        let trimmed = buf.trim();
                        if !trimmed.is_empty() {
                            println!("{}", trimmed);
                        }
                        buf.clear();
                    } else {
                        break;
                    }
                }
                drop(permit);
            });
        }

        // the output handler creates an empty file to which it will try to write, because
        // this happens before we enter the --parallel branch, we need to remove that file
        // if it's empty
        let output = handles.config.output.to_owned();

        clean_up(handles, tasks).await?;

        let file = Path::new(&output);
        if file.exists() {
            // expectation is that this is always true for the first ferox process
            if file.metadata()?.len() == 0 {
                // empty file, attempt to remove it
                remove_file(file)?;
            }
        }

        log::trace!("exit: parallel branch && wrapped main");
        return Ok(());
    }

    // in order for the Stats object to know about which targets are being scanned, we need to
    // wait until the parallel branch has been handled before sending the UpdateTargets command
    // this ensures that only the targets being scanned are sent to the Stats object
    //
    // if sent before the parallel branch is handled, the Stats object will have duplicate
    // targets
    handles.stats.send(UpdateTargets(targets.clone()))?;

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
        // done later here in main). sync checks that the tx/rx connection to the file handler works
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
            bail!(fmt_err(&format!("Failed while scanning: {e}")));
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

async fn update_app() -> Result<self_update::Status, Box<dyn ::std::error::Error>> {
    let target_os = format!("{}-{}", ARCH, OS);
    let status = tokio::task::spawn_blocking(move || {
        self_update::backends::github::Update::configure()
            .repo_owner("epi052")
            .repo_name("feroxbuster")
            .bin_name("feroxbuster")
            .target(target_os.as_str())
            .show_download_progress(true)
            .current_version(cargo_crate_version!())
            .build()?
            .update()
    })
    .await??;

    Ok(status)
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
        let future = wrapped_main(config.clone());
        if let Err(e) = runtime.block_on(future) {
            eprintln!("{e}");

            // the code below is to facilitate testing tests/test_banner entries. Since it's an
            // integration test, normal test detection (cfg!(test), etc...) won't work. So, in
            // the tests themselves, we pass
            // `--wordlist /definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676`
            // and look for that here to print the banner.
            //
            // this change became a necessity once we moved wordlist parsing out of `scan` and into
            // `wrapped_main`.
            if e.to_string()
                .contains("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
            {
                // support the handful of tests that use `--stdin`
                let targets: Vec<_> = if config.stdin {
                    stdin().lock().lines().map(|tgt| tgt.unwrap()).collect()
                } else {
                    vec!["http://localhost".to_string()]
                };

                // print the banner to stderr
                let std_stderr = stderr(); // std::io::stderr
                let banner = Banner::new(&targets, &config);
                if (!config.quiet && !config.silent) || config.parallel != 0 {
                    banner.print_to(std_stderr, config).unwrap();
                }
            }

            // if we've encountered an error before clean_up can be called (i.e. a wordlist error)
            // we need to at least spin-down the progress bar
            PROGRESS_PRINTER.finish();
        };
    }

    log::trace!("exit: main");

    Ok(())
}
