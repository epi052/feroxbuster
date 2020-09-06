use feroxbuster::config::{Configuration, CONFIGURATION};
use feroxbuster::{logger, FeroxResult};
use std::fs::File;
use std::io::{BufRead, BufReader};
use tokio::io;
use tokio_util::codec::{FramedRead, LinesCodec};
use std::collections::HashSet;
use futures::StreamExt;
use feroxbuster::scanner::FeroxScan;


/// Create a Set of Strings from the given wordlist
fn get_unique_words_from_wordlist(config: &Configuration) -> FeroxResult<HashSet<String>> {
    let file = match File::open(&config.wordlist) {
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

    Ok(words)
}


async fn app() -> FeroxResult<()> {
    let words =
        tokio::spawn(async move { get_unique_words_from_wordlist(&CONFIGURATION) }).await??;

    if CONFIGURATION.stdin {
        let stdin = io::stdin();  // tokio's stdin, not std
        let mut reader = FramedRead::new(stdin, LinesCodec::new());
        let mut tasks = vec![];


        while let Some(item) = reader.next().await {
            match item {
                Ok(line) => {
                    let cloned = words.as_ref.clone();
                    let task = tokio::spawn(async move {
                        let scanner = FeroxScan::new(&cloned);
                        scanner.scan_directory(&line).await;
                    });
                    tasks.push(task);
                }
                Err(e) => {
                    println!("FOUND: ERROR: {}", e);
                }
            }
        }
        futures::future::join_all(tasks).await;
    } else {
        let scanner = FeroxScan::new(&words);
        scanner.scan_directory(&CONFIGURATION.target_url).await;
    }

    Ok(())
}

fn main() {
    logger::initialize(CONFIGURATION.verbosity);

    log::debug!("{:#?}", *CONFIGURATION);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    match rt.block_on(app()) {
        Ok(_) => log::info!("Done"),
        Err(e) => log::error!("An error occurred: {}", e),
    };
}


