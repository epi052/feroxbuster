use anyhow::{bail, Context, Result};
use console::{strip_ansi_codes, style, user_attended};
use indicatif::ProgressBar;
use regex::Regex;
use reqwest::{Client, Method, Response, StatusCode, Url};
#[cfg(not(target_os = "windows"))]
use rlimit::{getrlimit, setrlimit, Resource};
use std::{
    fs,
    io::{self, BufWriter, Write},
    sync::Arc,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc::UnboundedSender, oneshot};

use crate::{
    config::Configuration,
    config::OutputLevel,
    event_handlers::{
        Command::{self, AddError, AddStatus},
        Handles,
    },
    progress::PROGRESS_PRINTER,
    response::FeroxResponse,
    send_command,
    statistics::StatError::{Connection, Other, Redirection, Request, Timeout},
    traits::FeroxSerialize,
    USER_AGENTS,
};

/// simple counter for grabbing 'random' user agents
static mut USER_AGENT_CTR: usize = 0;

/// Given the path to a file, open the file in append mode (create it if it doesn't exist) and
/// return a reference to the buffered file
pub fn open_file(filename: &str) -> Result<BufWriter<fs::File>> {
    log::trace!("enter: open_file({})", filename);

    let file = fs::OpenOptions::new() // std fs
        .create(true)
        .append(true)
        .open(filename)
        .with_context(|| fmt_err(&format!("Could not open {filename}")))?;

    let writer = BufWriter::new(file); // std io

    log::trace!("exit: open_file -> {:?}", writer);
    Ok(writer)
}

/// Takes in a string and examines the first character to return a color version of the same string
pub fn status_colorizer(status: &str) -> String {
    match status.chars().next() {
        Some('1') => style(status).blue().to_string(), // informational
        Some('2') => style(status).green().to_string(), // success
        Some('3') => style(status).yellow().to_string(), // redirects
        Some('4') => style(status).red().to_string(),  // client error
        Some('5') => style(status).red().to_string(),  // server error
        Some('W') => style(status).cyan().to_string(), // wildcard
        Some('E') => style(status).red().to_string(),  // error
        _ => status.to_string(),                       // ¯\_(ツ)_/¯
    }
}

/// simple wrapper to stay DRY
pub fn fmt_err(msg: &str) -> String {
    format!("{}: {}", status_colorizer("ERROR"), msg)
}

/// simple wrapper to get the current system time as
/// time elapsed from unix epoch
pub fn timestamp() -> f64 {
    let since_the_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));

    let secs = since_the_epoch.as_secs() as f64;
    let nanos = since_the_epoch.subsec_nanos() as f64;

    // Convert nanoseconds to fractional seconds and add to secs
    secs + (nanos / 1_000_000_000.0)
}

/// given a FeroxResponse, send a TryRecursion command
///
/// moved to utils to allow for calls from extractor and scanner
pub(crate) async fn send_try_recursion_command(
    handles: Arc<Handles>,
    response: FeroxResponse,
) -> Result<()> {
    // make the response mutable so we can drop the body before
    // sending it over the mpsc
    let mut response = response;
    response.drop_text();

    handles.send_scan_command(Command::TryRecursion(Box::new(response)))?;
    let (tx, rx) = oneshot::channel::<bool>();
    handles.send_scan_command(Command::Sync(tx))?;
    rx.await?;
    Ok(())
}

/// Takes in a string and colors it using console::style
///
/// mainly putting this here in case i want to change the color later, making any changes easy
pub fn module_colorizer(modname: &str) -> String {
    style(modname).cyan().to_string()
}

/// Simple helper to abstract away the check for an attached terminal.
///
/// If a terminal is attached, progress bars are visible and the progress bar is used to print
/// to stderr. The progress bar must be used when bars are visible in order to not jack up any
/// progress bar output (the bar knows how to print above itself)
///
/// If a terminal is not attached, `msg` is printed to stdout, with its ansi
/// color codes stripped.
///
/// additionally, provides a location for future printing options (no color, etc) to be handled
pub fn ferox_print(msg: &str, bar: &ProgressBar) {
    if user_attended() {
        bar.println(msg);
    } else {
        let stripped = strip_ansi_codes(msg);
        println!("{stripped}");
    }
}

/// wrapper for make_request used to pass error/response codes to FeroxScans for per-scan stats
/// tracking of information related to auto-tune/bail
pub async fn logged_request(
    url: &Url,
    method: &str,
    data: Option<&[u8]>,
    handles: Arc<Handles>,
) -> Result<Response> {
    let client = &handles.config.client;
    let level = handles.config.output_level;
    let tx_stats = handles.stats.tx.clone();

    let response = make_request(client, url, method, data, level, &handles.config, tx_stats).await;

    let scans = handles.ferox_scans()?;
    match response {
        Ok(resp) => {
            match resp.status() {
                StatusCode::TOO_MANY_REQUESTS | StatusCode::FORBIDDEN => {
                    scans.increment_status_code(url.as_str(), resp.status());
                }
                _ => {}
            }
            Ok(resp)
        }
        Err(e) => {
            log::warn!("err: {:?}", e);
            scans.increment_error(url.as_str());
            bail!(e)
        }
    }
}

/// Initiate request to the given `Url` using `Client`
pub async fn make_request(
    client: &Client,
    url: &Url,
    method: &str,
    mut data: Option<&[u8]>,
    output_level: OutputLevel,
    config: &Configuration,
    tx_stats: UnboundedSender<Command>,
) -> Result<Response> {
    log::trace!(
        "enter: make_request(Configuration::Client, {}, {:?}, {:?})",
        url,
        output_level,
        tx_stats
    );
    let tmp_workaround: Option<&[u8]> = Some(&[0xd_u8, 0xa]); // \r\n

    let mut request = client.request(Method::from_bytes(method.as_bytes())?, url.to_owned());

    if (!config.proxy.is_empty() || !config.replay_proxy.is_empty())
        && data.is_none()
        && ["post", "put", "patch"].contains(&method.to_ascii_lowercase().as_str())
    {
        // either --proxy or --replay-proxy was specified
        // AND
        // --data wasn't used
        // AND
        // the method is either post/put/patch (case insensitive)
        //
        // this combination of factors results in requests that are delayed for 10 seconds before
        // being issued. The tracking issues are
        //   https://github.com/epi052/feroxbuster/issues/501
        //   https://github.com/seanmonstar/reqwest/issues/1474
        //
        // as a (hopefully temporary) workaround, we'll add \r\n to the body so that there's no
        // delay
        data = tmp_workaround;
    }

    if let Some(body_data) = data {
        request = request.body(body_data.to_vec());
    }

    if config.random_agent {
        let index = unsafe {
            USER_AGENT_CTR += 1;
            USER_AGENT_CTR % USER_AGENTS.len()
        };

        let user_agent = USER_AGENTS[index];

        request = request.header("User-Agent", user_agent);
    }

    match request.send().await {
        Err(e) => {
            log::trace!("exit: make_request -> {}", e);

            if e.is_timeout() {
                send_command!(tx_stats, AddError(Timeout));
            } else if e.is_redirect() {
                if let Some(last_redirect) = e.url() {
                    // get where we were headed (last_redirect) and where we came from (url)
                    let fancy_message = format!(
                        "{} !=> {} ({})",
                        url,
                        last_redirect,
                        style("too many redirects").red(),
                    );

                    let msg_status = match e.status() {
                        Some(status) => status.to_string(),
                        None => "ERR".to_string(),
                    };

                    let report = create_report_string(
                        &msg_status,
                        method,
                        "-1",
                        "-1",
                        "-1",
                        &fancy_message,
                        output_level,
                    );

                    send_command!(tx_stats, AddError(Redirection));

                    ferox_print(&report, &PROGRESS_PRINTER)
                };
            } else if e.is_connect() {
                send_command!(tx_stats, AddError(Connection));
            } else if e.is_request() {
                send_command!(tx_stats, AddError(Request));
            } else {
                send_command!(tx_stats, AddError(Other));
            }

            log::warn!("Error while making request: {}", e);
            bail!("{}", e)
        }
        Ok(resp) => {
            log::trace!("exit: make_request -> {:?}", resp);
            send_command!(tx_stats, AddStatus(resp.status()));
            Ok(resp)
        }
    }
}

/// Helper to create the standard line for output to file/terminal
///
/// example output:
/// 200      127l      283w     4134c http://localhost/faq
pub fn create_report_string(
    status: &str,
    method: &str,
    line_count: &str,
    word_count: &str,
    content_length: &str,
    url: &str,
    output_level: OutputLevel,
) -> String {
    if matches!(output_level, OutputLevel::Silent) {
        // --silent used, just need the url
        format!("{url}\n")
    } else {
        // normal printing with status and sizes
        let color_status = status_colorizer(status);
        if status.contains("MSG") {
            format!(
                "{color_status} {method:>8} {line_count:>9} {word_count:>9} {content_length:>9} {url}\n"
            )
        } else {
            format!(
                "{color_status} {method:>8} {line_count:>8}l {word_count:>8}w {content_length:>8}c {url}\n"
            )
        }
    }
}

/// Attempts to set the soft limit for the RLIMIT_NOFILE resource
///
/// RLIMIT_NOFILE is the maximum number of file descriptors that can be opened by this process
///
/// The soft limit is the value that the kernel enforces for the corresponding resource.
/// The hard limit acts as a ceiling for the soft limit: an unprivileged process may set only its
/// soft limit to a value in the range from 0 up to the hard limit, and (irreversibly) lower its
/// hard limit.
///
/// A child process created via fork(2) inherits its parent's resource limits. Resource limits are
/// per-process attributes that are shared by all of the threads in a process.
///
/// Based on the above information, no attempt is made to restore the limit to its pre-scan value
/// as the adjustment made here is only valid for the scan itself (and any child processes, of which
/// there are none).
#[cfg(not(target_os = "windows"))]
pub fn set_open_file_limit(limit: u64) -> bool {
    log::trace!("enter: set_open_file_limit");

    if let Ok((soft, hard)) = getrlimit(Resource::NOFILE) {
        if hard > limit {
            // our default open file limit is less than the current hard limit, this means we can
            // set the soft limit to our default

            if setrlimit(Resource::NOFILE, limit, hard).is_ok() {
                log::debug!("set open file descriptor limit to {}", limit);

                log::trace!("exit: set_open_file_limit -> {}", true);
                return true;
            }
        } else if soft != hard {
            // hard limit is lower than our default, the next best option is to set the soft limit as
            // high as the hard limit will allow
            if setrlimit(Resource::NOFILE, hard, hard).is_ok() {
                log::debug!("set open file descriptor limit to {}", limit);

                log::trace!("exit: set_open_file_limit -> {}", true);
                return true;
            }
        }
    }

    // failed to set a new limit, as limit adjustments are a 'nice to have', we'll just log
    // and move along
    log::warn!("could not set open file descriptor limit to {}", limit);

    log::trace!("exit: set_open_file_limit -> {}", false);
    false
}

/// Given a string and a reference to a locked buffered file, write the contents and flush
/// the buffer to disk.
pub fn write_to<T>(
    value: &T,
    file: &mut io::BufWriter<fs::File>,
    convert_to_json: bool,
) -> Result<()>
where
    T: FeroxSerialize,
{
    // note to future self: adding logging of anything other than error to this function
    // is a bad idea. we call this function while processing records generated by the logger.
    // If we then call log::... while already processing some logging output, it results in
    // the second log entry being injected into the first.

    let contents = if convert_to_json {
        value.as_json()?
    } else {
        value.as_str()
    };

    let contents = strip_ansi_codes(&contents);

    let written = file.write(contents.as_bytes())?;

    if written > 0 {
        // this function is used within async functions/loops, so i'm flushing so that in
        // the event of a ctrl+c or w/e results seen so far are saved instead of left lying
        // around in the buffer
        file.flush()?;
    }

    Ok(())
}

/// determine if a url should be denied based on the given absolute url
fn should_deny_absolute(url_to_test: &Url, denier: &Url, handles: Arc<Handles>) -> Result<bool> {
    log::trace!(
        "enter: should_deny_absolute({}, {:?})",
        url_to_test.as_str(),
        denier.as_str(),
    );

    // simplest case is an exact match, check for it first
    if url_to_test == denier {
        log::trace!("exit: should_deny_absolute -> true");
        return Ok(true);
    }

    match (url_to_test.host(), denier.host()) {
        // .host() will return an enum with ipv4|6 or domain and is comparable
        // whereas .domain() returns None for ip addresses
        (Some(normed_host), Some(denier_host)) => {
            if normed_host != denier_host {
                // domains don't even match
                return Ok(false);
            }
        }
        _ => {
            // one or the other couldn't determine the host value, which probably means
            // it's not suitable for further comparison
            return Ok(false);
        }
    }

    let tested_host = url_to_test.host().unwrap(); // match above will catch errors

    // at this point, we have a matching set of ips or domain names. now we can process the
    // url path. The goal is to determine whether the given url's path is a subpath of any
    // url in the deny list, for example
    //    GIVEN URL                        URL DENY LIST               USER-SPECIFIED URLS TO SCAN
    //    http://some.domain/stuff/things, [http://some.domain/stuff], [http://some.domain] => true
    //    http://some.domain/stuff/things, [http://some.domain/stuff/things], [http://some.domain] => true
    //    http://some.domain/stuff/things, [http://some.domain/api], [http://some.domain] => false
    // the examples above are all pretty obvious, the kicker comes when the blocking url's
    // path is a parent to a scanned url
    //    http://some.domain/stuff/things, [http://some.domain/], [http://some.domain/stuff] => false
    //    http://some.domain/api, [http://some.domain/], [http://some.domain/stuff] => true
    // we want to deny all children of the parent, unless that child is a child of a scan
    // we specified through -u(s) or --stdin

    let deny_path = denier.path();
    let tested_path = url_to_test.path();

    if tested_path.starts_with(deny_path) {
        // at this point, we know that the given normalized path is a sub-path of the
        // current deny-url, now we just need to check to see if this deny-url is a parent
        // to a scanned url that is also a parent of the given url
        for ferox_scan in handles.ferox_scans()?.get_active_scans() {
            let scanner = parse_url_with_raw_path(ferox_scan.url().trim_end_matches('/'))
                .with_context(|| format!("Could not parse {ferox_scan} as a url"))?;

            // by calling the new parse_url_with_raw_path, and reaching this point without an
            // error, we know we have an authority and therefore a host. leaving the code
            // below, but we should never hit the else condition. leaving it in so if we find
            // a case where i'm mistaken, we'll know about it and can address it

            if let Some(scan_host) = scanner.host() {
                // same domain/ip check we perform on the denier above
                if tested_host != scan_host {
                    // domains don't even match, keep on keepin' on...
                    continue;
                }
            } else {
                // couldn't process .host from scanner
                unreachable!("should_deny_absolute: scanner.host() returned None, which shouldn't be possible");
            };

            let scan_path = scanner.path();

            if scan_path.starts_with(deny_path) && tested_path.starts_with(scan_path) {
                // user-specified scan url is a sub-path of the deny-urls's path AND the
                // url to check is a sub-path of the user-specified scan url
                //
                // the assumption is the user knew what they wanted and we're going to give
                // the scanned url precedence, even though it's a sub-path
                log::trace!("exit: should_deny_absolute -> false");
                return Ok(false);
            }
        }
        log::trace!("exit: should_deny_absolute -> true");
        return Ok(true);
    }

    log::trace!("exit: should_deny_absolute -> false");
    Ok(false)
}

/// determine if a url should be denied based on the given regular expression
///
/// the regex ONLY matches against the PATH of the url (not the scheme, host, port, etc)
fn should_deny_regex(url_to_test: &Url, denier: &Regex) -> bool {
    log::trace!(
        "enter: should_deny_regex({}, {})",
        url_to_test.as_str(),
        denier,
    );

    let result = denier.is_match(url_to_test.as_str());

    log::trace!("exit: should_deny_regex -> {}", result);
    result
}

/// determines whether or not a given url should be denied based on the user-supplied --dont-scan
/// flag
pub fn should_deny_url(url: &Url, handles: Arc<Handles>) -> Result<bool> {
    log::trace!(
        "enter: should_deny_url({}, {:?}, {:?})",
        url.as_str(),
        handles.config.url_denylist,
        handles.ferox_scans()?
    );

    // normalization for comparison is to remove the trailing / if one exists, this is done for
    // the given url and any url to which it's compared
    let normed_url = parse_url_with_raw_path(url.to_string().trim_end_matches('/'))?;

    for denier in &handles.config.url_denylist {
        // note to self: it may seem as though we can use regex only for --dont-scan, however, in
        // doing so, we lose the ability to block a parent directory while scanning a child
        if let Ok(should_deny) = should_deny_absolute(&normed_url, denier, handles.clone()) {
            if should_deny {
                return Ok(true);
            }
        }
    }

    for denier in &handles.config.regex_denylist {
        if should_deny_regex(&normed_url, denier) {
            return Ok(true);
        }
    }

    // made it to the end of the deny lists unscathed, return false, indicating we should not deny
    // this particular url
    log::trace!("exit: should_deny_url -> false");
    Ok(false)
}

/// given a url and filename-suffix, return a unique filename comprised of the slugified url,
/// current unix timestamp and suffix
///
/// ex: ferox-http_telsa_com-1606947491.state
pub fn slugify_filename(url: &str, prefix: &str, suffix: &str) -> String {
    log::trace!("enter: slugify({:?}, {:?}, {:?})", url, prefix, suffix);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();

    let altered_prefix = if !prefix.is_empty() {
        format!("{prefix}-")
    } else {
        String::new()
    };

    let slug = url.replace("://", "_").replace(['/', '.'], "_");

    let filename = format!("{altered_prefix}{slug}-{ts}.{suffix}");

    log::trace!("exit: slugify -> {}", filename);
    filename
}

/// This function takes a url string and returns a `url::Url`
///
/// It is primarily used to detect url paths that `url::Url::parse` will
/// silently transform, such as /path/../file.html -> /file.html
///
/// # Warning
///
/// In the instance of a url with encoded path traversal strings, such as
/// /path/%2e%2e/file.html, the underlying `url::Url::parse` will
/// further encode the %-signs and return /path/%252e%252e/file.html
pub fn parse_url_with_raw_path(url: &str) -> Result<Url> {
    log::trace!("enter: parse_url_with_raw_path({})", url);

    let parsed = Url::parse(url)?;

    if !parsed.has_authority() {
        // parsed correctly, but no authority, meaning mailto: or tel: or
        // some other url that we don't care about
        bail!("url to parse has no authority and is therefore invalid");
    }

    // thanks to @devx00: the possibility exists for Url to return true for
    // has_authority, but not have a host/port, so we'll check for that
    // and bail if it's the case
    if parsed.host().is_none() {
        bail!("url to parse doesn't have a host");
    }

    // we have a valid url, the next step is to check the path and see if it's
    // something that url::Url::parse would silently transform
    //
    // i.e. if the path is /path/../file.html, url::Url::parse will transform it
    // to /file.html, which is not what we want

    let farthest_right_authority_part;

    // we want to find the farthest right authority component, which is the
    // component that is the furthest right in the url that is part of the
    // authority
    //
    // per RFC 3986, the authority is defined as:
    // - authority = [ userinfo "@" ] host [ ":" port ]
    //
    // so the farthest right authority component is either the port or the host
    //
    // i.e. in http://example.com:80/path/file.html, the farthest right authority
    // component is :80
    //
    // in http://example.com/path/file.html, the farthest right authority component
    // is example.com
    //
    // the farthest right authority component is used to split the url into two
    // parts: the part before the authority and the part after the authority
    if let Some(port) = parsed.port() {
        // if the url has a port, then the farthest right authority component is
        // the port
        farthest_right_authority_part = format!(":{}", port);
    } else if parsed.has_host() {
        // if the url has a host, then the farthest right authority component is
        // the host
        farthest_right_authority_part = parsed.host_str().unwrap().to_owned();
    } else {
        // if the url has neither a port nor a host, then the url is invalid
        // and we can't do anything with it, but i don't think this is possible
        unreachable!("url has an authority, but has neither a port nor a host");
    }

    // split the original url string into two parts: the part before the authority and the part
    // after the authority (i.e. the path + query + fragment)

    let Some((_, after_authority)) = url.split_once(&farthest_right_authority_part) else {
        // if we can't split the url string into two parts, then the url doesn't conform to our
        // expectations, and we can't continue processing it, so we'll return the parsed url
        return Ok(parsed);
    };

    // when there is a port, but it matches the default port for the scheme,
    // url::Url::parse will mark the port as None, giving us a
    // `after_authority` that looks something like this:
    // - :80/path/file.html
    let after_authority = after_authority
        .replacen(":80", "", 1)
        .replacen(":443", "", 1);

    // snippets from rfc-3986:
    //
    //          foo://example.com:8042/over/there?name=ferret#nose
    //          \_/   \______________/\_________/ \_________/ \__/
    //           |           |            |            |        |
    //        scheme     authority       path        query   fragment
    //
    // The path component is terminated
    //    by the first question mark ("?") or number sign ("#") character, or
    //    by the end of the URI.
    //
    // The query component is indicated by the first question
    //    mark ("?") character and terminated by a number sign ("#") character
    //    or by the end of the URI.
    let (path, _discarded) = after_authority
        .split_once('?')
        // if there isn't a '?', try to remove a fragment
        .unwrap_or_else(|| {
            // if there isn't a '#', return (original, empty)
            after_authority
                .split_once('#')
                .unwrap_or((&after_authority, ""))
        });

    // at this point, we have the path, all by itself

    // each of the following is a string that we can expect url::Url::parse to
    // transform. The variety is to ensure we cover most common path traversal
    // encodings
    let transformation_detectors = [
        // ascii
        "..",
        // single url encoded
        "%2e%2e",
        // double url encoded
        "%25%32%65%25%32%65",
        // utf-8 encoded
        "%c0%ae%c0%ae",
        "%e0%40%ae%e0%40%ae",
        "%c0ae%c0ae",
        // 16 bit shenanigans
        "%uff0e%uff0e",
        "%u002e%u002e",
    ];

    let parsing_will_transform_path = transformation_detectors
        .iter()
        .any(|detector| path.to_lowercase().contains(detector));

    if !parsing_will_transform_path {
        // there's no string in the path of the url that will trigger a transformation
        // so, we can return it as-is
        return Ok(parsed);
    }

    // if we reach this point, the path contains a string that will trigger a transformation
    // so we need to manually create a Url that doesn't have the transformation
    // and return that
    //
    // special thanks to github user @lavafroth for this workaround

    let mut hacked_url = if path.ends_with('/') {
        // from_file_path silently strips trailing slashes, and
        // from_directory_path adds them, so we'll choose the appropriate
        // constructor based on the presence of a path's trailing slash

        // according to from_file_path docs:
        //   from_file_path returns `Err` if the given path is not absolute or,
        //   on Windows, if the prefix is not a disk prefix (e.g. `C:`) or a UNC prefix (`\\`).
        //
        // since we parsed out a valid url path, we know it is absolute, so on non-windows
        // platforms, we can safely unwrap. On windows, we need to fix up the path
        #[cfg(target_os = "windows")]
        {
            let path = format!("\\/IGNOREME{path}");
            Url::from_directory_path(path).unwrap()
        }
        #[cfg(not(target_os = "windows"))]
        Url::from_directory_path(path).unwrap()
    } else {
        #[cfg(target_os = "windows")]
        {
            let path = format!("\\/IGNOREME{path}");
            Url::from_file_path(path).unwrap()
        }
        #[cfg(not(target_os = "windows"))]
        Url::from_file_path(path).unwrap()
    };

    // host must be set first, otherwise multiple components may return Err
    hacked_url.set_host(parsed.host_str())?;
    // scheme/port/username/password can fail, but in this instance, we know they won't
    hacked_url.set_scheme(parsed.scheme()).unwrap();
    hacked_url.set_port(parsed.port()).unwrap();
    hacked_url.set_username(parsed.username()).unwrap();
    hacked_url.set_password(parsed.password()).unwrap();
    // query/fragment can't fail
    hacked_url.set_query(parsed.query());
    hacked_url.set_fragment(parsed.fragment());

    log::trace!("exit: parse_url_with_raw_path -> {}", hacked_url);
    Ok(hacked_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Configuration;
    use crate::scan_manager::{FeroxScans, ScanOrder};

    #[test]
    /// parse_url_with_raw_path with javascript:// should not throw an unimplemented! error
    fn utils_parse_url_with_raw_path_javascript() {
        let url = "javascript://";
        let parsed = parse_url_with_raw_path(url);
        assert!(parsed.is_err());
        assert!(parsed
            .unwrap_err()
            .to_string()
            .contains("url to parse doesn't have a host"));
    }

    #[test]
    /// multiple tests for parse_url_with_raw_path
    fn utils_parse_url_with_raw_path() {
        // ../.. is preserved
        let url = "https://www.google.com/../../stuff";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.as_str(), url);

        // ../.. is preserved as well as the trailing slash
        let url = "https://www.google.com/../../stuff/";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.as_str(), url);

        // no trailing slash is preserved
        let url = "https://www.google.com/stuff";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.as_str(), url);

        // trailing slash is preserved
        let url = "https://www.google.com/stuff/";
        let parsed: Url = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.as_str(), url);

        // mailto is an error
        let url = "mailto:user@example.com";
        let parsed = parse_url_with_raw_path(url);
        assert!(parsed.is_err());

        // relative url is an error
        let url = "../../stuff";
        let parsed = parse_url_with_raw_path(url);
        assert!(parsed.is_err());

        // absolute without host is an error
        let url = "/../../stuff";
        let parsed = parse_url_with_raw_path(url);
        assert!(parsed.is_err());

        // default ports are parsed correctly
        for url in [
            "http://example.com:80/path/file.html",
            "https://example.com:443/path/file.html",
        ] {
            let parsed = parse_url_with_raw_path(url).unwrap();
            assert!(parsed.port().is_none());
            assert_eq!(parsed.host().unwrap().to_string().as_str(), "example.com");
        }

        // non-default ports are parsed correctly
        for url in [
            "http://example.com:8080/path/file.html",
            "https://example.com:4433/path/file.html",
        ] {
            let parsed = parse_url_with_raw_path(url).unwrap();
            assert!(parsed.port().is_some());
            assert_eq!(parsed.as_str(), url);
        }

        // different encodings are respected if found in doubles
        //
        // note that the % sign is encoded as %25...
        let url = "http://user:pass@example.com/%2e%2e/stuff.php";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(
            parsed.as_str(),
            "http://user:pass@example.com/%252e%252e/stuff.php"
        );

        let url = "http://user:pass@example.com/%25%32%65%25%32%65/stuff.php";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.username(), "user");
        assert_eq!(parsed.password().unwrap(), "pass");
        assert_eq!(
            parsed.as_str(),
            "http://user:pass@example.com/%2525%2532%2565%2525%2532%2565/stuff.php"
        );

        let url = "http://user:pass@example.com/%c0%ae%c0%ae/stuff.php";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.username(), "user");
        assert_eq!(parsed.password().unwrap(), "pass");
        assert_eq!(
            parsed.as_str(),
            "http://user:pass@example.com/%25c0%25ae%25c0%25ae/stuff.php"
        );

        let url = "http://user:pass@example.com/%e0%40%ae%e0%40%ae/stuff.php";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.username(), "user");
        assert_eq!(parsed.password().unwrap(), "pass");
        assert_eq!(
            parsed.as_str(),
            "http://user:pass@example.com/%25e0%2540%25ae%25e0%2540%25ae/stuff.php"
        );

        let url = "http://user:pass@example.com/%c0ae%c0ae/stuff.php";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.username(), "user");
        assert_eq!(parsed.password().unwrap(), "pass");
        assert_eq!(
            parsed.as_str(),
            "http://user:pass@example.com/%25c0ae%25c0ae/stuff.php"
        );

        let url = "http://user:pass@example.com/%uff0e%uff0e/stuff.php";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.username(), "user");
        assert_eq!(parsed.password().unwrap(), "pass");
        assert_eq!(
            parsed.as_str(),
            "http://user:pass@example.com/%25uff0e%25uff0e/stuff.php"
        );

        let url = "http://user:pass@example.com/%u002e%u002e/stuff.php";
        let parsed = parse_url_with_raw_path(url).unwrap();
        assert_eq!(parsed.username(), "user");
        assert_eq!(parsed.password().unwrap(), "pass");
        assert_eq!(
            parsed.as_str(),
            "http://user:pass@example.com/%25u002e%25u002e/stuff.php"
        );
    }

    #[cfg(not(target_os = "windows"))]
    mod nix_only_tests {
        use super::*;

        #[test]
        /// set_open_file_limit with a low requested limit succeeds
        fn utils_set_open_file_limit_with_low_requested_limit() {
            let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
            let lower_limit = hard - 1;
            assert!(set_open_file_limit(lower_limit));
        }

        #[test]
        /// set_open_file_limit with a high requested limit succeeds
        fn utils_set_open_file_limit_with_high_requested_limit() {
            let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
            let higher_limit = hard + 1;
            // calculate a new soft to ensure soft != hard and hit that logic branch
            let new_soft = hard - 1;
            setrlimit(Resource::NOFILE, new_soft, hard).unwrap();
            assert!(set_open_file_limit(higher_limit));
        }

        #[test]
        /// set_open_file_limit should fail when hard == soft
        fn utils_set_open_file_limit_with_fails_when_both_limits_are_equal() {
            let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
            // calculate a new soft to ensure soft == hard and hit the failure logic branch
            setrlimit(Resource::NOFILE, hard, hard).unwrap();
            assert!(!set_open_file_limit(hard)); // returns false
        }
    }

    #[test]
    /// status colorizer uses red for 500s
    fn status_colorizer_uses_red_for_500s() {
        assert_eq!(status_colorizer("500"), style("500").red().to_string());
    }

    #[test]
    /// status colorizer uses red for 400s
    fn status_colorizer_uses_red_for_400s() {
        assert_eq!(status_colorizer("400"), style("400").red().to_string());
    }

    #[test]
    /// status colorizer uses red for errors
    fn status_colorizer_uses_red_for_errors() {
        assert_eq!(status_colorizer("ERROR"), style("ERROR").red().to_string());
    }

    #[test]
    /// status colorizer uses cyan for wildcards
    fn status_colorizer_uses_cyan_for_wildcards() {
        assert_eq!(status_colorizer("WLD"), style("WLD").cyan().to_string());
    }

    #[test]
    /// status colorizer uses blue for 100s
    fn status_colorizer_uses_blue_for_100s() {
        assert_eq!(status_colorizer("100"), style("100").blue().to_string());
    }

    #[test]
    /// status colorizer uses green for 200s
    fn status_colorizer_uses_green_for_200s() {
        assert_eq!(status_colorizer("200"), style("200").green().to_string());
    }

    #[test]
    /// status colorizer uses yellow for 300s
    fn status_colorizer_uses_yellow_for_300s() {
        assert_eq!(status_colorizer("300"), style("300").yellow().to_string());
    }

    #[test]
    /// status colorizer doesnt color anything else
    fn status_colorizer_returns_as_is() {
        assert_eq!(status_colorizer("farfignewton"), "farfignewton".to_string());
    }

    #[test]
    /// provide a url that should be blocked where the denier is an exact match for the tested url
    /// expect true
    fn should_deny_url_blocks_when_denier_is_exact_match() {
        let scan_url = "https://testdomain.com/";
        let deny_url = "https://testdomain.com/denied";
        let tested_url = Url::parse("https://testdomain.com/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a url that has a different host than the denier but the same path, expect false
    fn should_deny_url_doesnt_compare_mismatched_domains() {
        let scan_url = "https://testdomain.com/";
        let deny_url = "https://dev.testdomain.com/denied";
        let tested_url = Url::parse("https://testdomain.com/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(!should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier from which we can't check a host, which results in no comparison, expect false
    fn should_deny_url_doesnt_compare_non_domains() {
        let scan_url = "https://testdomain.com/";
        let deny_url = "unix:/run/foo.socket";
        let tested_url = Url::parse("https://testdomain.com/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(!should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a url that has a different host than the denier but the same path, expect false
    /// because the denier is a parent to the tested, even tho the scanned doesn't compare, it
    /// still returns true
    fn should_deny_url_doesnt_compare_mismatched_domains_in_scanned() {
        let deny_url = "https://testdomain.com/";
        let scan_url = "https://dev.testdomain.com/denied";
        let tested_url = Url::parse("https://testdomain.com/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier from which we can't check a host, which results in no comparison, expect false
    /// because the denier is a parent to the tested, even tho the scanned doesn't compare, it
    /// still returns true
    ///
    /// note: adding parse_url_with_raw_path changed the behavior of this test, it used to return
    /// true, now it returns false. see my note in should_deny_absolute and the unreachable!
    /// call block to see why
    ///
    /// leaving this test here to document the behavior change and to catch regressions in the
    /// new expected behavior
    fn should_deny_url_doesnt_compare_non_domains_in_scanned() {
        let deny_url = "https://testdomain.com/";
        let scan_url = "unix:/run/foo.socket";
        let tested_url = Url::parse("https://testdomain.com/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);
        assert!(!should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier where the tested url is a sub-path and the scanned url is not, expect true
    fn should_deny_url_blocks_child() {
        let scan_url = "https://testdomain.com/";
        let deny_url = "https://testdomain.com/api";
        let tested_url = Url::parse("https://testdomain.com/api/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier where the tested url is not a sub-path and the scanned url is not, expect false
    fn should_deny_url_doesnt_block_non_child() {
        let scan_url = "https://testdomain.com/";
        let deny_url = "https://testdomain.com/api";
        let tested_url = Url::parse("https://testdomain.com/not-denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(!should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier where the tested url is a sub-path and the scanned url is not, expect true
    fn should_deny_url_blocks_child_when_scan_url_isnt_parent() {
        let scan_url = "https://testdomain.com/api";
        let deny_url = "https://testdomain.com/";
        let tested_url = Url::parse("https://testdomain.com/stuff/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier where the tested url is not a sub-path and the scanned url is not, expect false
    fn should_deny_url_doesnt_block_child_when_scan_url_is_parent() {
        let scan_url = "https://testdomain.com/api";
        let deny_url = "https://testdomain.com/";
        let tested_url = Url::parse("https://testdomain.com/api/not-denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.url_denylist = vec![Url::parse(deny_url).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(!should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier where the tested url is matched against a regular expression in the path
    /// of the url
    fn should_deny_url_blocks_urls_based_on_regex_in_path() {
        let scan_url = "https://testdomain.com/";
        let deny_pattern = "/deni.*";
        let tested_url = Url::parse("https://testdomain.com/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.regex_denylist = vec![Regex::new(deny_pattern).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(should_deny_url(&tested_url, handles).unwrap());
    }

    #[test]
    /// provide a denier where the tested url is matched against a regular expression in the scheme
    /// of the url
    fn should_deny_url_blocks_urls_based_on_regex_in_scheme() {
        let scan_url = "https://testdomain.com/";
        let deny_pattern = "http:";
        let tested_http_url = Url::parse("http://testdomain.com/denied/").unwrap();
        let tested_https_url = Url::parse("https://testdomain.com/denied/").unwrap();

        let scans = Arc::new(FeroxScans::default());
        scans.add_directory_scan(
            scan_url,
            ScanOrder::Initial,
            Arc::new(Handles::for_testing(None, None).0),
        );

        let mut config = Configuration::new().unwrap();
        config.regex_denylist = vec![Regex::new(deny_pattern).unwrap()];
        let config = Arc::new(config);

        let handles = Arc::new(Handles::for_testing(Some(scans), Some(config)).0);

        assert!(!should_deny_url(&tested_https_url, handles.clone()).unwrap());
        assert!(should_deny_url(&tested_http_url, handles).unwrap());
    }
}
