use crate::{
    config::{Configuration, CONFIGURATION},
    statistics::StatCommand,
    utils::{make_request, status_colorizer},
};
use console::{style, Emoji};
use reqwest::{Client, Url};
use serde_json::Value;
use std::io::Write;
use tokio::sync::mpsc::UnboundedSender;

/// macro helper to abstract away repetitive string formatting
macro_rules! format_banner_entry_helper {
    // \u{0020} -> unicode space
    // \u{2502} -> vertical box drawing character, i.e. │
    ($rune:expr, $name:expr, $value:expr, $indent:expr, $col_width:expr) => {
        format!(
            "\u{0020}{:\u{0020}<indent$}{:\u{0020}<col_w$}\u{2502}\u{0020}{}",
            $rune,
            $name,
            $value,
            indent = $indent,
            col_w = $col_width
        )
    };
    ($rune:expr, $name:expr, $value:expr, $value2:expr, $indent:expr, $col_width:expr) => {
        format!(
            "\u{0020}{:\u{0020}<indent$}{:\u{0020}<col_w$}\u{2502}\u{0020}{}:\u{0020}{}",
            $rune,
            $name,
            $value,
            $value2,
            indent = $indent,
            col_w = $col_width
        )
    };
}

/// macro that wraps another macro helper to abstract away repetitive string formatting
macro_rules! format_banner_entry {
    // 4 -> unicode emoji padding width
    // 22 -> column width (when unicode rune is 4 bytes wide, 23 when it's 3)
    // hardcoded since macros don't allow let statements
    ($rune:expr, $name:expr, $value:expr) => {
        format_banner_entry_helper!($rune, $name, $value, 3, 22)
    };
    ($rune:expr, $name:expr, $value1:expr, $value2:expr) => {
        format_banner_entry_helper!($rune, $name, $value1, $value2, 3, 22)
    };
}

/// Url used to query github's api; specifically used to look for the latest tagged release name
const UPDATE_URL: &str = "https://api.github.com/repos/epi052/feroxbuster/releases/latest";

/// Simple enum to hold three different update states
#[derive(Debug)]
enum UpdateStatus {
    /// this version and latest release are the same
    UpToDate,

    /// this version and latest release are not the same
    OutOfDate,

    /// some error occurred during version check
    Unknown,
}

/// Makes a request to the given url, expecting to receive a JSON response that contains a field
/// named `tag_name` that holds a value representing the latest tagged release of this tool.
///
/// ex: v1.1.0
///
/// Returns `UpdateStatus`
async fn needs_update(
    client: &Client,
    url: &str,
    bin_version: &str,
    tx_stats: UnboundedSender<StatCommand>,
) -> UpdateStatus {
    log::trace!("enter: needs_update({:?}, {}, {:?})", client, url, tx_stats);

    let unknown = UpdateStatus::Unknown;

    let api_url = match Url::parse(url) {
        Ok(url) => url,
        Err(e) => {
            log::error!("{}", e);
            log::trace!("exit: needs_update -> {:?}", unknown);
            return unknown;
        }
    };

    if let Ok(response) = make_request(&client, &api_url, tx_stats.clone()).await {
        let body = response.text().await.unwrap_or_default();

        let json_response: Value = serde_json::from_str(&body).unwrap_or_default();

        if json_response.is_null() {
            // unwrap_or_default above should result in a null value for the json_response variable
            log::error!("Could not parse JSON from response body");
            log::trace!("exit: needs_update -> {:?}", unknown);
            return unknown;
        }

        let latest_version = match json_response["tag_name"].as_str() {
            Some(tag) => tag.trim_start_matches('v'),
            None => {
                log::error!("Could not get version field from JSON response");
                log::debug!("{}", json_response);
                log::trace!("exit: needs_update -> {:?}", unknown);
                return unknown;
            }
        };

        // if we've gotten this far, we have a string in the form of X.X.X where X is a number
        // all that's left is to compare the current version with the version found above

        return if latest_version == bin_version {
            // there's really only two possible outcomes if we accept that the tag conforms to
            // the X.X.X pattern:
            //   1. the version strings match, meaning we're up to date
            //   2. the version strings do not match, meaning we're out of date
            //
            // except for developers working on this code, nobody should ever be in a situation
            // where they have a version greater than the latest tagged release
            log::trace!("exit: needs_update -> UpdateStatus::UpToDate");
            UpdateStatus::UpToDate
        } else {
            log::trace!("exit: needs_update -> UpdateStatus::OutOfDate");
            UpdateStatus::OutOfDate
        };
    }

    log::trace!("exit: needs_update -> {:?}", unknown);
    unknown
}

/// Simple wrapper for emoji or fallback when terminal doesn't support emoji
fn format_emoji(emoji: &str) -> String {
    let width = console::measure_text_width(emoji);
    let pad_len = width * width;
    let pad = format!("{:<pad_len$}", "\u{0020}", pad_len = pad_len);
    Emoji(emoji, &pad).to_string()
}

/// Prints the banner to stdout.
///
/// Only prints those settings which are either always present, or passed in by the user.
pub async fn initialize<W>(
    targets: &[String],
    config: &Configuration,
    version: &str,
    mut writer: W,
    tx_stats: UnboundedSender<StatCommand>,
) where
    W: Write,
{
    let artwork = format!(
        r#"
 ___  ___  __   __     __      __         __   ___
|__  |__  |__) |__) | /  `    /  \ \_/ | |  \ |__
|    |___ |  \ |  \ | \__,    \__/ / \ | |__/ |___
by Ben "epi" Risher {}                 ver: {}"#,
        Emoji("🤓", &format!("{:<2}", "\u{0020}")),
        version
    );
    let status = needs_update(&CONFIGURATION.client, UPDATE_URL, version, tx_stats).await;

    let top = "───────────────────────────┬──────────────────────";
    let addl_section = "──────────────────────────────────────────────────";
    let bottom = "───────────────────────────┴──────────────────────";

    writeln!(&mut writer, "{}", artwork).unwrap_or_default();
    writeln!(&mut writer, "{}", top).unwrap_or_default();

    // begin with always printed items
    for target in targets {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🎯"), "Target Url", target)
        )
        .unwrap_or_default(); // 🎯
    }

    let mut codes = vec![];

    for code in &config.status_codes {
        codes.push(status_colorizer(&code.to_string()))
    }

    writeln!(
        &mut writer,
        "{}",
        format_banner_entry!(format_emoji("🚀"), "Threads", config.threads)
    )
    .unwrap_or_default(); // 🚀

    writeln!(
        &mut writer,
        "{}",
        format_banner_entry!(format_emoji("📖"), "Wordlist", config.wordlist)
    )
    .unwrap_or_default(); // 📖

    writeln!(
        &mut writer,
        "{}",
        format_banner_entry!(
            format_emoji("🆗"),
            "Status Codes",
            format!("[{}]", codes.join(", "))
        )
    )
    .unwrap_or_default(); // 🆗

    if !config.filter_status.is_empty() {
        // exception here for optional print due to me wanting the allows and denys to be printed
        // one after the other
        let mut code_filters = vec![];

        for code in &config.filter_status {
            code_filters.push(status_colorizer(&code.to_string()))
        }

        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(
                format_emoji("💢"),
                "Status Code Filters",
                format!("[{}]", code_filters.join(", "))
            )
        )
        .unwrap_or_default(); // 💢
    }

    writeln!(
        &mut writer,
        "{}",
        format_banner_entry!(format_emoji("💥"), "Timeout (secs)", config.timeout)
    )
    .unwrap_or_default(); // 💥

    writeln!(
        &mut writer,
        "{}",
        format_banner_entry!(format_emoji("🦡"), "User-Agent", config.user_agent)
    )
    .unwrap_or_default(); // 🦡

    // followed by the maybe printed or variably displayed values
    if !config.config.is_empty() {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("💉"), "Config File", config.config)
        )
        .unwrap_or_default(); // 💉
    }

    if !config.proxy.is_empty() {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("💎"), "Proxy", config.proxy)
        )
        .unwrap_or_default(); // 💎
    }

    if !config.replay_proxy.is_empty() {
        // i include replay codes logic here because in config.rs, replay codes are set to the
        // value in status codes, meaning it's never empty

        let mut replay_codes = vec![];

        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🎥"), "Replay Proxy", config.replay_proxy)
        )
        .unwrap_or_default(); // 🎥

        for code in &config.replay_codes {
            replay_codes.push(status_colorizer(&code.to_string()))
        }

        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(
                format_emoji("📼"),
                "Replay Proxy Codes",
                format!("[{}]", replay_codes.join(", "))
            )
        )
        .unwrap_or_default(); // 📼
    }

    if !config.headers.is_empty() {
        for (name, value) in &config.headers {
            writeln!(
                &mut writer,
                "{}",
                format_banner_entry!(format_emoji("🤯"), "Header", name, value)
            )
            .unwrap_or_default(); // 🤯
        }
    }

    if !config.filter_size.is_empty() {
        for filter in &config.filter_size {
            writeln!(
                &mut writer,
                "{}",
                format_banner_entry!(format_emoji("💢"), "Size Filter", filter)
            )
            .unwrap_or_default(); // 💢
        }
    }

    if !config.filter_similar.is_empty() {
        for filter in &config.filter_similar {
            writeln!(
                &mut writer,
                "{}",
                format_banner_entry!(format_emoji("💢"), "Similarity Filter", filter)
            )
            .unwrap_or_default(); // 💢
        }
    }

    for filter in &config.filter_word_count {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("💢"), "Word Count Filter", filter)
        )
        .unwrap_or_default(); // 💢
    }

    for filter in &config.filter_line_count {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("💢"), "Line Count Filter", filter)
        )
        .unwrap_or_default(); // 💢
    }

    for filter in &config.filter_regex {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("💢"), "Regex Filter", filter)
        )
        .unwrap_or_default(); // 💢
    }

    if config.extract_links {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🔎"), "Extract Links", config.extract_links)
        )
        .unwrap_or_default(); // 🔎
    }

    if config.json {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🧔"), "JSON Output", config.json)
        )
        .unwrap_or_default(); // 🧔
    }

    if !config.queries.is_empty() {
        for query in &config.queries {
            writeln!(
                &mut writer,
                "{}",
                format_banner_entry!(
                    format_emoji("🤔"),
                    "Query Parameter",
                    format!("{}={}", query.0, query.1)
                )
            )
            .unwrap_or_default(); // 🤔
        }
    }

    if !config.output.is_empty() {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("💾"), "Output File", config.output)
        )
        .unwrap_or_default(); // 💾
    }

    if !config.debug_log.is_empty() {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🪲"), "Debugging Log", config.debug_log)
        )
        .unwrap_or_default(); // 🪲
    }

    if !config.extensions.is_empty() {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(
                format_emoji("💲"),
                "Extensions",
                format!("[{}]", config.extensions.join(", "))
            )
        )
        .unwrap_or_default(); // 💲
    }

    if config.insecure {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🔓"), "Insecure", config.insecure)
        )
        .unwrap_or_default(); // 🔓
    }

    if config.redirects {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("📍"), "Follow Redirects", config.redirects)
        )
        .unwrap_or_default(); // 📍
    }

    if config.dont_filter {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🤪"), "Filter Wildcards", !config.dont_filter)
        )
        .unwrap_or_default(); // 🤪
    }

    let volume = ["🔈", "🔉", "🔊", "📢"];
    if let 1..=4 = config.verbosity {
        //speaker medium volume (increasing with verbosity to loudspeaker)
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(
                format_emoji(volume[config.verbosity as usize - 1]),
                "Verbosity",
                config.verbosity
            )
        )
        .unwrap_or_default();
    }

    if config.add_slash {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🪓"), "Add Slash", config.add_slash)
        )
        .unwrap_or_default(); // 🪓
    }

    if !config.no_recursion {
        if config.depth == 0 {
            writeln!(
                &mut writer,
                "{}",
                format_banner_entry!(format_emoji("🔃"), "Recursion Depth", "INFINITE")
            )
            .unwrap_or_default(); // 🔃
        } else {
            writeln!(
                &mut writer,
                "{}",
                format_banner_entry!(format_emoji("🔃"), "Recursion Depth", config.depth)
            )
            .unwrap_or_default(); // 🔃
        }
    } else {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🚫"), "Do Not Recurse", config.no_recursion)
        )
        .unwrap_or_default(); // 🚫
    }

    if CONFIGURATION.scan_limit > 0 {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(
                format_emoji("🦥"),
                "Concurrent Scan Limit",
                config.scan_limit
            )
        )
        .unwrap_or_default(); // 🦥
    }

    if !CONFIGURATION.time_limit.is_empty() {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(format_emoji("🕖"), "Time Limit", config.time_limit)
        )
        .unwrap_or_default(); // 🕖
    }

    if matches!(status, UpdateStatus::OutOfDate) {
        writeln!(
            &mut writer,
            "{}",
            format_banner_entry!(
                format_emoji("🎉"),
                "New Version Available",
                "https://github.com/epi052/feroxbuster/releases/latest"
            )
        )
        .unwrap_or_default(); // 🎉
    }

    writeln!(&mut writer, "{}", bottom).unwrap_or_default();
    // ⏯
    writeln!(
        &mut writer,
        " {}  Press [{}] to use the {}™",
        format_emoji("🏁"),
        style("ENTER").yellow(),
        style("Scan Cancel Menu").bright().yellow(),
    )
    .unwrap_or_default();

    writeln!(&mut writer, "{}", addl_section).unwrap_or_default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeroxChannel, VERSION};
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use std::fs::read_to_string;
    use std::io::stderr;
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test to hit no execution of targets for loop in banner
    async fn banner_intialize_without_targets() {
        let config = Configuration::default();
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        initialize(&[], &config, VERSION, stderr(), tx).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test to hit no execution of statuscode for loop in banner
    async fn banner_intialize_without_status_codes() {
        let config = Configuration {
            status_codes: vec![],
            ..Default::default()
        };

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        initialize(
            &[String::from("http://localhost")],
            &config,
            VERSION,
            stderr(),
            tx,
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test to hit an empty config file
    async fn banner_intialize_without_config_file() {
        let config = Configuration {
            config: String::new(),
            ..Default::default()
        };

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        initialize(
            &[String::from("http://localhost")],
            &config,
            VERSION,
            stderr(),
            tx,
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test to hit an empty config file
    async fn banner_intialize_without_queries() {
        let config = Configuration {
            queries: vec![(String::new(), String::new())],
            ..Default::default()
        };

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        initialize(
            &[String::from("http://localhost")],
            &config,
            VERSION,
            stderr(),
            tx,
        )
        .await;
    }

    #[ignore]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test to show that a new version is available for download
    async fn banner_intialize_with_mismatched_version() {
        let config = Configuration::default();
        let file = NamedTempFile::new().unwrap();
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        initialize(
            &[String::from("http://localhost")],
            &config,
            "mismatched-version",
            &file,
            tx,
        )
        .await;
        let contents = read_to_string(file.path()).unwrap();
        println!("contents: {}", contents);
        assert!(contents.contains("New Version Available"));
        assert!(contents.contains("https://github.com/epi052/feroxbuster/releases/latest"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test that
    async fn banner_needs_update_returns_unknown_with_bad_url() {
        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let result = needs_update(&CONFIGURATION.client, &"", VERSION, tx).await;
        assert!(matches!(result, UpdateStatus::Unknown));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test return value of good url to needs_update
    async fn banner_needs_update_returns_up_to_date() {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then| {
            when.method(GET).path("/latest");
            then.status(200).body("{\"tag_name\":\"v1.1.0\"}");
        });

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let result = needs_update(&CONFIGURATION.client, &srv.url("/latest"), "1.1.0", tx).await;

        assert_eq!(mock.hits(), 1);
        assert!(matches!(result, UpdateStatus::UpToDate));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test return value of good url to needs_update that returns a newer version than current
    async fn banner_needs_update_returns_out_of_date() {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then| {
            when.method(GET).path("/latest");
            then.status(200).body("{\"tag_name\":\"v1.1.0\"}");
        });

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let result = needs_update(&CONFIGURATION.client, &srv.url("/latest"), "1.0.1", tx).await;

        assert_eq!(mock.hits(), 1);
        assert!(matches!(result, UpdateStatus::OutOfDate));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test return value of good url that times out
    async fn banner_needs_update_returns_unknown_on_timeout() {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then| {
            when.method(GET).path("/latest");
            then.status(200)
                .body("{\"tag_name\":\"v1.1.0\"}")
                .delay(Duration::from_secs(8));
        });

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let result = needs_update(&CONFIGURATION.client, &srv.url("/latest"), "1.0.1", tx).await;

        assert_eq!(mock.hits(), 1);
        assert!(matches!(result, UpdateStatus::Unknown));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test return value of good url with bad json response
    async fn banner_needs_update_returns_unknown_on_bad_json_response() {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then| {
            when.method(GET).path("/latest");
            then.status(200).body("not json");
        });

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let result = needs_update(&CONFIGURATION.client, &srv.url("/latest"), "1.0.1", tx).await;

        assert_eq!(mock.hits(), 1);
        assert!(matches!(result, UpdateStatus::Unknown));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    /// test return value of good url with json response that lacks the tag_name field
    async fn banner_needs_update_returns_unknown_on_json_without_correct_tag() {
        let srv = MockServer::start();

        let mock = srv.mock(|when, then| {
            when.method(GET).path("/latest");
            then.status(200)
                .body("{\"no tag_name\": \"doesn't exist\"}");
        });

        let (tx, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();

        let result = needs_update(&CONFIGURATION.client, &srv.url("/latest"), "1.0.1", tx).await;

        assert_eq!(mock.hits(), 1);
        assert!(matches!(result, UpdateStatus::Unknown));
    }
}
