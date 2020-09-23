use feroxbuster::config::CONFIGURATION;
use feroxbuster::scanner::scan_url;
use feroxbuster::utils::get_current_depth;
use feroxbuster::{banner, heuristics, logger, FeroxResult};
use futures::StreamExt;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use tokio::io;
use tokio_util::codec::{FramedRead, LinesCodec};

/// Create a HashSet of Strings from the given wordlist then stores it inside an Arc
fn get_unique_words_from_wordlist(path: &str) -> FeroxResult<Arc<HashSet<String>>> {
    log::trace!("enter: get_unique_words_from_wordlist({})", path);

    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Could not open wordlist: {}", e);
            log::trace!("exit: get_unique_words_from_wordlist -> {}", e);

            return Err(Box::new(e));
        }
    };

    let reader = BufReader::new(file);

    let mut words = HashSet::new();

    for line in reader.lines() {
        match line {
            Ok(word) => {
                words.insert(word);
            }
            Err(e) => {
                log::warn!("Could not parse current line from wordlist : {}", e);
            }
        }
    }

    log::trace!(
        "exit: get_unique_words_from_wordlist -> Arc<wordlist[{} words...]>",
        words.len()
    );
    Ok(Arc::new(words))
}

/// Determine whether it's a single url scan or urls are coming from stdin, then scan as needed
async fn scan(targets: Vec<String>) -> FeroxResult<()> {
    log::trace!("enter: scan");
    // cloning an Arc is cheap (it's basically a pointer into the heap)
    // so that will allow for cheap/safe sharing of a single wordlist across multi-target scans
    // as well as additional directories found as part of recursion
    let words =
        tokio::spawn(async move { get_unique_words_from_wordlist(&CONFIGURATION.wordlist) })
            .await??;

    let mut tasks = vec![];

    for target in targets {
        let wordclone = words.clone();

        let task = tokio::spawn(async move {
            let base_depth = get_current_depth(&target);
            scan_url(&target, wordclone, base_depth).await;
        });

        tasks.push(task);
    }

    // drive execution of all accumulated futures
    futures::future::join_all(tasks).await;
    log::trace!("exit: scan");

    Ok(())
}

async fn get_targets() -> Vec<String> {
    log::trace!("enter: get_targets");

    let mut targets = vec![];

    if CONFIGURATION.stdin {
        // got targets from stdin, i.e. cat sites | ./feroxbuster ...
        // just need to read the targets from stdin and spawn a future for each target found
        let stdin = io::stdin(); // tokio's stdin, not std
        let mut reader = FramedRead::new(stdin, LinesCodec::new());

        while let Some(line) = reader.next().await {
            match line {
                Ok(target) => {
                    targets.push(target);
                }
                Err(e) => {
                    println!("[ERROR] - {}", e);
                }
            }
        }
    } else {
        targets.push(CONFIGURATION.target_url.clone());
    }

    log::trace!("exit: get_targets -> {:?}", targets);

    targets
}

#[tokio::main]
async fn main() {
    logger::initialize(CONFIGURATION.verbosity);

    log::trace!("enter: main");
    log::debug!("{:#?}", *CONFIGURATION);

    // get targets from command line or stdin
    let targets = get_targets().await;

    if !CONFIGURATION.quiet {
        // only print banner if -q isn't used
        banner::initialize(&targets);
        // progress::initialize();
    }

    // discard non-responsive targets
    let live_targets = heuristics::connectivity_test(&targets).await;

    match scan(live_targets).await {
        Ok(_) => {
            log::info!("Done");
        }
        Err(e) => log::error!("An error occurred: {}", e),
    };

    log::trace!("exit: main");
}
