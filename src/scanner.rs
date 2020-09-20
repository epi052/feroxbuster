use crate::config::CONFIGURATION;
use crate::heuristics::WildcardFilter;
use crate::utils::{get_current_depth, get_url_path_length, status_colorizer};
use crate::{heuristics, FeroxResult};
use futures::future::{BoxFuture, FutureExt};
use futures::{stream, StreamExt};
use reqwest::{Client, Response, Url};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

/// Simple helper to generate a `Url`
///
/// Errors during parsing `url` or joining `word` are propagated up the call stack
pub fn format_url(
    url: &str,
    word: &str,
    addslash: bool,
    extension: Option<&str>,
) -> FeroxResult<Url> {
    log::trace!(
        "enter: format_url({}, {}, {}, {:?})",
        url,
        word,
        addslash,
        extension
    );

    // from reqwest::Url::join
    //   Note: a trailing slash is significant. Without it, the last path component
    //   is considered to be a “file” name to be removed to get at the “directory”
    //   that is used as the base
    //
    // the transforms that occur here will need to keep this in mind, i.e. add a slash to preserve
    // the current directory sent as part of the url
    let url = if !url.ends_with('/') {
        format!("{}/", url)
    } else {
        url.to_string()
    };

    let base_url = reqwest::Url::parse(&url)?;

    // extensions and slashes are mutually exclusive cases
    let word = if extension.is_some() {
        format!("{}.{}", word, extension.unwrap())
    } else if addslash && !word.ends_with('/') {
        // -f used, and word doesn't already end with a /
        format!("{}/", word)
    } else {
        String::from(word)
    };

    match base_url.join(&word) {
        Ok(request) => {
            log::trace!("exit: format_url -> {}", request);
            Ok(request)
        }
        Err(e) => {
            log::trace!("exit: format_url -> {}", e);
            log::error!("Could not join {} with {}", word, base_url);
            Err(Box::new(e))
        }
    }
}

/// Initiate request to the given `Url` using the pre-configured `Client`
pub async fn make_request(client: &Client, url: &Url) -> FeroxResult<Response> {
    log::trace!("enter: make_request(CONFIGURATION.Client, {})", url);

    match client.get(url.to_owned()).send().await {
        Ok(resp) => {
            log::debug!("requested Url: {}", resp.url());
            log::trace!("exit: make_request -> {:?}", resp);
            Ok(resp)
        }
        Err(e) => {
            log::trace!("exit: make_request -> {}", e);
            log::error!("Error while making request: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives responses and writes them to the given output file if they meet
/// the given reporting criteria
async fn spawn_file_reporter(mut report_channel: UnboundedReceiver<Response>) {
    log::trace!("enter: spawn_file_reporter({:?}", report_channel);

    log::info!("Writing scan results to {}", CONFIGURATION.output);

    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&CONFIGURATION.output)
    {
        Ok(outfile) => {
            log::debug!("{:?} opened in append mode", outfile);

            let mut writer = BufWriter::new(outfile);

            while let Some(resp) = report_channel.recv().await {
                log::debug!("received {} on reporting channel", resp.url());

                if CONFIGURATION.statuscodes.contains(&resp.status().as_u16()) {
                    let report = if CONFIGURATION.quiet {
                        format!("{}\n", resp.url())
                    } else {
                        format!(
                            "[{}] - {} - [{} bytes]\n",
                            resp.status(),
                            resp.url(),
                            resp.content_length().unwrap_or(0)
                        )
                    };

                    match write!(writer, "{}", report) {
                        Ok(_) => (),
                        Err(e) => {
                            log::error!("could not write report to disk: {}", e);
                        }
                    }
                }

                log::debug!("report complete: {}", resp.url());
            }
        }
        Err(e) => {
            log::error!("error opening file: {}", e);
        }
    }
    log::trace!("exit: spawn_file_reporter");
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives responses and prints them if they meet the given
/// reporting criteria
async fn spawn_terminal_reporter(mut report_channel: UnboundedReceiver<Response>) {
    log::trace!("enter: spawn_terminal_reporter({:?})", report_channel);

    while let Some(resp) = report_channel.recv().await {
        log::debug!("received {} on reporting channel", resp.url());

        if CONFIGURATION.statuscodes.contains(&resp.status().as_u16()) {
            if CONFIGURATION.quiet {
                println!("{}", resp.url());
            } else {
                let status = status_colorizer(&resp.status().to_string());
                println!(
                    "[{}] - {} - [{} bytes]",
                    status,
                    resp.url(),
                    resp.content_length().unwrap_or(0)
                );
            }
        }
        log::debug!("report complete: {}", resp.url());
    }
    log::trace!("exit: spawn_terminal_reporter");
}

/// Spawn a single consumer task (sc side of mpsc)
///
/// The consumer simply receives Urls and scans them
fn spawn_recursion_handler(
    mut recursion_channel: UnboundedReceiver<String>,
    wordlist: Arc<HashSet<String>>,
    base_depth: usize,
) -> BoxFuture<'static, Vec<JoinHandle<()>>> {
    log::trace!(
        "enter: spawn_recursion_handler({:?}, wordlist[{} words...], {})",
        recursion_channel,
        wordlist.len(),
        base_depth
    );

    let boxed_future = async move {
        let mut scans = vec![];
        while let Some(resp) = recursion_channel.recv().await {
            log::info!("received {} on recursion channel", resp);
            let clonedresp = resp.clone();
            let clonedlist = wordlist.clone();
            scans.push(tokio::spawn(async move {
                scan_url(clonedresp.to_owned().as_str(), clonedlist, base_depth).await
            }));
        }
        scans
    }
    .boxed();

    log::trace!("exit: spawn_recursion_handler -> BoxFuture<'static, Vec<JoinHandle<()>>>");
    boxed_future
}

/// Creates a vector of formatted Urls
///
/// At least one value will be returned (base_url + word)
///
/// If any extensions were passed to the program, each extension will add a
/// (base_url + word + ext) Url to the vector
fn create_urls(target_url: &str, word: &str, extensions: &[String]) -> Vec<Url> {
    log::trace!(
        "enter: create_urls({}, {}, {:?})",
        target_url,
        word,
        extensions
    );

    let mut urls = vec![];

    if let Ok(url) = format_url(&target_url, &word, CONFIGURATION.addslash, None) {
        urls.push(url); // default request, i.e. no extension
    }

    for ext in extensions.iter() {
        if let Ok(url) = format_url(&target_url, &word, CONFIGURATION.addslash, Some(ext)) {
            urls.push(url); // any extensions passed in
        }
    }

    log::trace!("exit: create_urls -> {:?}", urls);
    urls
}

/// Helper function to determine suitability for recursion
///
/// handles 2xx and 3xx responses by either checking if the url ends with a / (2xx)
/// or if the Location header is present and matches the base url + / (3xx)
fn response_is_directory(response: &Response) -> bool {
    log::trace!("enter: is_directory({:?})", response);

    if response.status().is_redirection() {
        // status code is 3xx
        match response.headers().get("Location") {
            // and has a Location header
            Some(loc) => {
                // get absolute redirect Url based on the already known base url
                log::debug!("Location header: {:?}", loc);

                if let Ok(loc_str) = loc.to_str() {
                    if let Ok(abs_url) = response.url().join(loc_str) {
                        if format!("{}/", response.url()) == abs_url.as_str() {
                            // if current response's Url + / == the absolute redirection
                            // location, we've found a directory suitable for recursion
                            log::debug!(
                                "found directory suitable for recursion: {}",
                                response.url()
                            );
                            log::trace!("exit: is_directory -> true");
                            return true;
                        }
                    }
                }
            }
            None => {
                log::debug!(
                    "expected Location header, but none was found: {:?}",
                    response
                );
                log::trace!("exit: is_directory -> false");
                return false;
            }
        }
    } else if response.status().is_success() {
        // status code is 2xx, need to check if it ends in /
        if response.url().as_str().ends_with('/') {
            log::debug!("{} is directory suitable for recursion", response.url());
            log::trace!("exit: is_directory -> true");
            return true;
        }
    }

    log::trace!("exit: is_directory -> false");
    false
}

/// Helper function that determines if the configured maximum recursion depth has been reached
///
/// Essentially looks at the Url path and determines how many directories are present in the
/// given Url
fn reached_max_depth(url: &Url, base_depth: usize) -> bool {
    log::trace!("enter: reached_max_depth({}, {})", url, base_depth);

    if CONFIGURATION.depth == 0 {
        // early return, as 0 means recurse forever; no additional processing needed
        log::trace!("exit: reached_max_depth -> false");
        return false;
    }

    let depth = get_current_depth(url.as_str());

    if depth - base_depth >= CONFIGURATION.depth {
        return true;
    }

    log::trace!("exit: reached_max_depth -> false");
    false
}

/// Helper function that wraps logic to check for recursion opportunities
///
/// When a recursion opportunity is found, the new url is sent across the recursion channel
async fn try_recursion(
    response: &Response,
    base_depth: usize,
    transmitter: UnboundedSender<String>,
) {
    log::trace!(
        "enter: try_recursion({:?}, {}, {:?})",
        response,
        base_depth,
        transmitter
    );

    if !reached_max_depth(response.url(), base_depth) && response_is_directory(&response) {
        if CONFIGURATION.redirects {
            // response is 2xx can simply send it because we're following redirects
            log::info!("Added new directory to recursive scan: {}", response.url());

            match transmitter.send(String::from(response.url().as_str())) {
                Ok(_) => {
                    log::debug!("sent {} across channel to begin a new scan", response.url());
                }
                Err(e) => {
                    log::error!(
                        "could not send {} across {:?}: {}",
                        response.url(),
                        transmitter,
                        e
                    );
                }
            }
        } else {
            let new_url = String::from(response.url().as_str());

            log::info!("Added new directory to recursive scan: {}", new_url);

            match transmitter.send(new_url) {
                Ok(_) => {}
                Err(e) => {
                    log::error!(
                        "could not send {}/ across {:?}: {}",
                        response.url(),
                        transmitter,
                        e
                    );
                }
            }
        }
    }
    log::trace!("exit: try_recursion");
}

/// Wrapper for [make_request](fn.make_request.html)
///
/// Handles making multiple requests based on the presence of extensions
///
/// Attempts recursion when appropriate and sends Responses to the report handler for processing
async fn make_requests(
    target_url: &str,
    word: &str,
    base_depth: usize,
    filter: Arc<WildcardFilter>,
    dir_chan: UnboundedSender<String>,
    report_chan: UnboundedSender<Response>,
) {
    log::trace!(
        "enter: make_requests({}, {}, {}, {:?}, {:?})",
        target_url,
        word,
        base_depth,
        dir_chan,
        report_chan
    );

    let urls = create_urls(&target_url, &word, &CONFIGURATION.extensions);

    for url in urls {
        if let Ok(response) = make_request(&CONFIGURATION.client, &url).await {
            // response came back without error

            // do recursion if appropriate
            if !CONFIGURATION.norecursion && response_is_directory(&response) {
                try_recursion(&response, base_depth, dir_chan.clone()).await;
            }

            // purposefully doing recursion before filtering. the thought process is that
            // even though this particular url is filtered, subsequent urls may not

            let content_len = &response.content_length().unwrap_or(0);

            if CONFIGURATION.sizefilters.contains(content_len) {
                // filtered value from --sizefilters, move on to the next url
                continue;
            }

            if filter.size > 0 && filter.size == *content_len && true {
                // todo replace with --dumb logic
                // static wildcard size found during testing
                // size isn't default, size equals response length, and it's not a 'dumb' scan
                continue;
            }

            if filter.dynamic > 0 && true {
                // todo replace with --dumb logic
                // dynamic wildcard offset found during testing

                // I'm about to manually split this url path instead of using reqwest::Url's
                // builtin parsing. The reason is that they call .split() on the url path
                // except that I don't want an empty string taking up the last index in the
                // event that the url ends with a forward slash.  It's ugly enough to be split
                // into its own function for readability.
                let url_len = get_url_path_length(&response.url());

                if url_len + filter.dynamic == *content_len {
                    continue;
                }
            }

            // everything else should be reported
            match report_chan.send(response) {
                Ok(_) => {
                    log::debug!("sent {}/{} over reporting channel", &target_url, &word);
                }
                Err(e) => {
                    log::error!("wtf: {}", e);
                }
            }
        }
    }
    log::trace!("exit: make_requests");
}

/// Scan a given url using a given wordlist
///
/// This is the primary entrypoint for the scanner
pub async fn scan_url(target_url: &str, wordlist: Arc<HashSet<String>>, base_depth: usize) {
    log::trace!(
        "enter: scan_url({:?}, wordlist[{} words...], {})",
        target_url,
        wordlist.len(),
        base_depth
    );

    log::info!("Starting scan against: {}", target_url);

    let (tx_rpt, rx_rpt): (UnboundedSender<Response>, UnboundedReceiver<Response>) =
        mpsc::unbounded_channel();

    let (tx_dir, rx_dir): (UnboundedSender<String>, UnboundedReceiver<String>) =
        mpsc::unbounded_channel();

    let reporter = if !CONFIGURATION.output.is_empty() {
        // output file defined
        tokio::spawn(async move { spawn_file_reporter(rx_rpt).await })
    } else {
        tokio::spawn(async move { spawn_terminal_reporter(rx_rpt).await })
    };

    // lifetime satisfiers, as it's an Arc, clones are cheap anyway
    let looping_words = wordlist.clone();
    let recurser_words = wordlist.clone();

    let recurser =
        tokio::spawn(
            async move { spawn_recursion_handler(rx_dir, recurser_words, base_depth).await },
        );

    let filter = if let Some(f) = heuristics::wildcard_test(&target_url).await {
        Arc::new(f)
    } else {
        Arc::new(WildcardFilter::default())
    };

    // producer tasks (mp of mpsc); responsible for making requests
    let producers = stream::iter(looping_words.deref().to_owned())
        .map(|word| {
            let wc_filter = filter.clone();
            let txd = tx_dir.clone();
            let txr = tx_rpt.clone();
            let tgt = target_url.to_string(); // done to satisfy 'static lifetime below
            tokio::spawn(async move {
                make_requests(&tgt, &word, base_depth, wc_filter, txd, txr).await
            })
        })
        .for_each_concurrent(CONFIGURATION.threads, |resp| async move {
            match resp.await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("error awaiting a response: {}", e);
                }
            }
        });

    // await tx tasks
    log::trace!("awaiting scan producers");
    producers.await;
    log::trace!("done awaiting scan producers");

    // manually drop tx in order for the rx task's while loops to eval to false
    log::trace!("dropped recursion handler's transmitter");
    drop(tx_dir);

    // await rx tasks
    log::trace!("awaiting recursive scan receiver/scans");
    futures::future::join_all(recurser.await.unwrap()).await;
    log::trace!("done awaiting recursive scan receiver/scans");

    // same thing here, drop report tx so the rx can finish up
    log::trace!("dropped report handler's transmitter");
    drop(tx_rpt);

    log::trace!("awaiting report receiver");
    match reporter.await {
        Ok(_) => {}
        Err(e) => {
            log::error!("error awaiting report receiver: {}", e);
        }
    }
    log::trace!("done awaiting report receiver");

    log::trace!("exit: scan_url");
}

#[cfg(test)]
mod tests {
    use super::*;
    //
    #[test]
    fn test_format_url_normal() {
        assert_eq!(
            format_url("http://localhost", "stuff", false, None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_no_word() {
        assert_eq!(
            format_url("http://localhost", "", false, None).unwrap(),
            reqwest::Url::parse("http://localhost").unwrap()
        );
    }

    #[test]
    #[should_panic]
    fn test_format_url_no_url() {
        format_url("", "stuff", false, None).unwrap();
    }

    #[test]
    fn test_format_url_word_with_preslash() {
        assert_eq!(
            format_url("http://localhost", "/stuff", false, None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff").unwrap()
        );
    }

    #[test]
    fn test_format_url_word_with_postslash() {
        assert_eq!(
            format_url("http://localhost", "stuff/", false, None).unwrap(),
            reqwest::Url::parse("http://localhost/stuff/").unwrap()
        );
    }
}
