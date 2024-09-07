#[cfg(not(test))]
use crate::event_handlers::TermInputHandler;
use crate::{
    config::{Configuration, OutputLevel},
    event_handlers::Handles,
    parser::TIMESPEC_REGEX,
    progress::BarType,
    scan_manager::scan::Visibility,
    scanner::RESPONSES,
};

use std::{fs::File, io::BufReader, sync::Arc};
use tokio::time;

/// Given a string representing some number of seconds, minutes, hours, or days, convert
/// that representation to seconds and then wait for those seconds to elapse.  Once that period
/// of time has elapsed, kill all currently running scans and dump a state file to disk that can
/// be used to resume any unfinished scan.
pub async fn start_max_time_thread(handles: Arc<Handles>) {
    log::trace!("enter: start_max_time_thread({:?})", handles);

    // as this function has already made it through the parser, which calls is_match on
    // the value passed to --time-limit using TIMESPEC_REGEX; we can safely assume that
    // the capture groups are populated; can expect something like 10m, 30s, 1h, etc...
    let captures = TIMESPEC_REGEX.captures(&handles.config.time_limit).unwrap();
    let length_match = captures.get(1).unwrap();
    let measurement_match = captures.get(2).unwrap();

    if let Ok(length) = length_match.as_str().parse::<u64>() {
        let length_in_secs = match measurement_match.as_str().to_ascii_lowercase().as_str() {
            "s" => length,
            "m" => length * 60,           // minutes
            "h" => length * 60 * 60,      // hours
            "d" => length * 60 * 60 * 24, // days
            _ => length,
        };

        log::debug!(
            "max time limit as string: {} and as seconds: {}",
            handles.config.time_limit,
            length_in_secs
        );

        time::sleep(time::Duration::new(length_in_secs, 0)).await;

        log::trace!("exit: start_max_time_thread");

        #[cfg(test)]
        panic!("{handles:?}");
        #[cfg(not(test))]
        let _ = TermInputHandler::sigint_handler(handles.clone());
    }

    log::warn!(
        "Could not parse the value provided ({}), can't enforce time limit",
        handles.config.time_limit
    );
}

/// Primary logic used to load a Configuration from disk and populate the appropriate data
/// structures
pub fn resume_scan(filename: &str) -> Configuration {
    log::trace!("enter: resume_scan({})", filename);

    let file = File::open(filename).unwrap_or_else(|e| {
        log::error!("{}", e);
        log::error!("Could not open state file, exiting");
        std::process::exit(1);
    });

    let reader = BufReader::new(file);
    let state: serde_json::Value = serde_json::from_reader(reader).unwrap();

    let conf = state.get("config").unwrap_or_else(|| {
        log::error!("Could not load configuration from state file, exiting");
        std::process::exit(1);
    });

    let config = serde_json::from_value(conf.clone()).unwrap_or_else(|e| {
        log::error!("{}", e);
        log::error!("Could not deserialize configuration found in state file, exiting");
        std::process::exit(1);
    });

    if let Some(responses) = state.get("responses") {
        if let Some(arr_responses) = responses.as_array() {
            for response in arr_responses {
                if let Ok(deser_resp) = serde_json::from_value(response.clone()) {
                    RESPONSES.insert(deser_resp);
                }
            }
        }
    }

    log::trace!("exit: resume_scan -> {:?}", config);
    config
}

/// determine the type of progress bar to display
/// takes both --limit-bars and output-level (--quiet|--silent|etc)
/// into account to arrive at a `BarType`
pub fn determine_bar_type(
    bar_limit: usize,
    number_of_bars: usize,
    output_level: OutputLevel,
) -> BarType {
    let visibility = if bar_limit == 0 {
        // no limit from cli, just set the value to visible
        // this protects us from a mutex unlock in number_of_bars
        // in the normal case
        Visibility::Visible
    } else if bar_limit < number_of_bars {
        // active bars exceed limit; hidden
        Visibility::Hidden
    } else {
        Visibility::Visible
    };

    match (output_level, visibility) {
        (OutputLevel::Default, Visibility::Visible) => BarType::Default,
        (OutputLevel::Quiet, Visibility::Visible) => BarType::Quiet,
        (OutputLevel::Default, Visibility::Hidden) => BarType::Hidden,
        (OutputLevel::Quiet, Visibility::Hidden) => BarType::Hidden,
        (OutputLevel::Silent | OutputLevel::SilentJSON, _) => BarType::Hidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_limit_visible() {
        let bar_type = determine_bar_type(0, 1, OutputLevel::Default);
        assert!(matches!(bar_type, BarType::Default));
    }

    #[test]
    fn test_limit_exceeded_hidden() {
        let bar_type = determine_bar_type(1, 2, OutputLevel::Default);
        assert!(matches!(bar_type, BarType::Hidden));
    }

    #[test]
    fn test_limit_not_exceeded_visible() {
        let bar_type = determine_bar_type(2, 1, OutputLevel::Default);
        assert!(matches!(bar_type, BarType::Default));
    }

    #[test]
    fn test_quiet_visible() {
        let bar_type = determine_bar_type(0, 1, OutputLevel::Quiet);
        assert!(matches!(bar_type, BarType::Quiet));
    }

    #[test]
    fn test_quiet_hidden() {
        let bar_type = determine_bar_type(1, 2, OutputLevel::Quiet);
        assert!(matches!(bar_type, BarType::Hidden));
    }

    #[test]
    fn test_silent_hidden() {
        let bar_type = determine_bar_type(0, 1, OutputLevel::Silent);
        assert!(matches!(bar_type, BarType::Hidden));
    }

    #[test]
    fn test_silent_json_hidden() {
        let bar_type = determine_bar_type(0, 1, OutputLevel::SilentJSON);
        assert!(matches!(bar_type, BarType::Hidden));
    }
}
