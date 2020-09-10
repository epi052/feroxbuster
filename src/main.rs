use feroxbuster::config::CONFIGURATION;
use feroxbuster::scanner::FeroxScan;
use feroxbuster::{logger, FeroxResult};
use futures::StreamExt;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use tokio::io;
use tokio_util::codec::{FramedRead, LinesCodec};


/// Create a HashSet of Strings from the given wordlist then stores it inside an Arc
fn get_unique_words_from_wordlist(path: &str) -> FeroxResult<Arc<HashSet<String>>> {
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Could not open wordlist: {}", e);
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

    Ok(Arc::new(words))
}

async fn scan() -> FeroxResult<()> {
    // cloning an Arc is cheap (it's basically a pointer into the heap)
    // so that will allow for cheap/safe sharing of a single wordlist across multi-target scans
    // as well as additional directories found as part of recursion
    let words =
        tokio::spawn(async move { get_unique_words_from_wordlist(&CONFIGURATION.wordlist) })
            .await??;

    if CONFIGURATION.stdin {
        // got targets from stdin, i.e. cat sites | ./feroxbuster ...
        // just need to read the targets from stdin and spawn a future for each target found
        let stdin = io::stdin(); // tokio's stdin, not std
        let mut reader = FramedRead::new(stdin, LinesCodec::new());
        let mut tasks = vec![];
        let scanner = FeroxScan::new(words.clone());

        while let Some(line) = reader.next().await {
            match line {
                Ok(target) => {
                    let scanner_ptr = scanner.clone();
                    let task = tokio::spawn(async move {
                        scanner_ptr.scan_directory(&target).await;
                    });
                    tasks.push(task);
                }
                Err(e) => {
                    println!("FOUND: ERROR: {}", e);
                }
            }
        }
        // drive execution of all accumulated futures
        futures::future::join_all(tasks).await;
    } else {
        let scanner = FeroxScan::new(words.clone());
        scanner.scan_directory(&CONFIGURATION.target_url).await;
    }

    Ok(())
}

fn main() {
    logger::initialize(CONFIGURATION.verbosity);

    log::debug!("{:#?}", *CONFIGURATION);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    match rt.block_on(scan()) {
        Ok(_) => log::info!("Done"),
        Err(e) => log::error!("An error occurred: {}", e),
    };
}
