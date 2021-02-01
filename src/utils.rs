use anyhow::{bail, Context, Result};
use console::{strip_ansi_codes, style, user_attended};
use indicatif::ProgressBar;
use reqwest::{Client, Response, Url};
#[cfg(not(target_os = "windows"))]
use rlimit::{getrlimit, setrlimit, Resource, Rlim};
use std::{
    fs,
    io::{self, BufWriter, Write},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    config::PROGRESS_PRINTER,
    event_handlers::Command::{self, AddError, AddStatus},
    send_command,
    statistics::StatError::{Connection, Other, Redirection, Request, Timeout},
    traits::FeroxSerialize,
};

/// Given the path to a file, open the file in append mode (create it if it doesn't exist) and
/// return a reference to the buffered file
pub fn open_file(filename: &str) -> Result<BufWriter<fs::File>> {
    log::trace!("enter: open_file({})", filename);

    let file = fs::OpenOptions::new() // std fs
        .create(true)
        .append(true)
        .open(filename)
        .with_context(|| fmt_err(&format!("Could not open {}", filename)))?;

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
        println!("{}", stripped);
    }
}

/// Initiate request to the given `Url` using `Client`
pub async fn make_request(
    client: &Client,
    url: &Url,
    quiet: bool,
    tx_stats: UnboundedSender<Command>,
) -> Result<Response> {
    log::trace!(
        "enter: make_request(CONFIGURATION.Client, {}, {}, {:?})",
        url,
        quiet,
        tx_stats
    );

    match client.get(url.to_owned()).send().await {
        Err(e) => {
            let mut log_level = log::Level::Error;

            log::trace!("exit: make_request -> {}", e);
            if e.is_timeout() {
                // only warn for timeouts, while actual errors are still left as errors
                log_level = log::Level::Warn;
                send_command!(tx_stats, AddError(Timeout));
            } else if e.is_redirect() {
                if let Some(last_redirect) = e.url() {
                    // get where we were headed (last_redirect) and where we came from (url)
                    let fancy_message = format!("{} !=> {}", url, last_redirect);

                    let report = if let Some(msg_status) = e.status() {
                        send_command!(tx_stats, AddStatus(msg_status));
                        create_report_string(
                            msg_status.as_str(),
                            "-1",
                            "-1",
                            "-1",
                            &fancy_message,
                            quiet,
                        )
                    } else {
                        create_report_string("UNK", "-1", "-1", "-1", &fancy_message, quiet)
                    };

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

            if matches!(log_level, log::Level::Error) {
                log::error!("Error while making request: {}", e);
            } else {
                log::warn!("Error while making request: {}", e);
            }

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
    line_count: &str,
    word_count: &str,
    content_length: &str,
    url: &str,
    quiet: bool,
) -> String {
    if quiet {
        // -q used, just need the url
        format!("{}\n", url)
    } else {
        // normal printing with status and sizes
        let color_status = status_colorizer(status);
        format!(
            "{} {:>8}l {:>8}w {:>8}c {}\n",
            color_status, line_count, word_count, content_length, url
        )
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
pub fn set_open_file_limit(limit: usize) -> bool {
    log::trace!("enter: set_open_file_limit");

    if let Ok((soft, hard)) = getrlimit(Resource::NOFILE) {
        if hard.as_usize() > limit {
            // our default open file limit is less than the current hard limit, this means we can
            // set the soft limit to our default
            let new_soft_limit = Rlim::from_usize(limit);

            if setrlimit(Resource::NOFILE, new_soft_limit, hard).is_ok() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// set_open_file_limit with a low requested limit succeeds
    fn utils_set_open_file_limit_with_low_requested_limit() {
        let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
        let lower_limit = hard.as_usize() - 1;
        assert!(set_open_file_limit(lower_limit));
    }

    #[test]
    /// set_open_file_limit with a high requested limit succeeds
    fn utils_set_open_file_limit_with_high_requested_limit() {
        let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
        let higher_limit = hard.as_usize() + 1;
        // calculate a new soft to ensure soft != hard and hit that logic branch
        let new_soft = Rlim::from_usize(hard.as_usize() - 1);
        setrlimit(Resource::NOFILE, new_soft, hard).unwrap();
        assert!(set_open_file_limit(higher_limit));
    }

    #[test]
    /// set_open_file_limit should fail when hard == soft
    fn utils_set_open_file_limit_with_fails_when_both_limits_are_equal() {
        let (_, hard) = getrlimit(Resource::NOFILE).unwrap();
        // calculate a new soft to ensure soft == hard and hit the failure logic branch
        setrlimit(Resource::NOFILE, hard, hard).unwrap();
        assert!(!set_open_file_limit(hard.as_usize())); // returns false
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
}
