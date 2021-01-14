use crate::{
    config::{Configuration, CONFIGURATION},
    statistics::StatCommand,
    utils::{make_request, status_colorizer},
};
use anyhow::Result;
use console::{style, Emoji};
use reqwest::{Client, Url};
use serde::export::Formatter;
use serde_json::Value;
use std::fmt::{self, Display};
use std::io::Write;
use tokio::sync::mpsc::UnboundedSender;

/// Initial visual indentation size used in formatting banner entries
const INDENT: usize = 3;

/// Column width used in formatting banner entries
const COL_WIDTH: usize = 22;

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

/// Represents a single line on the banner
#[derive(Default)]
struct BannerEntry {
    /// emoji used in the banner entry
    emoji: String,

    /// title used in the banner entry
    title: String,

    /// value passed in via config/cli/defaults
    value: String,
}

/// implementation of a banner entry
impl BannerEntry {
    /// Create a new banner entry from given fields
    pub fn new(emoji: &str, title: &str, value: &str) -> Self {
        BannerEntry {
            emoji: emoji.to_string(),
            title: title.to_string(),
            value: value.to_string(),
        }
    }

    /// Simple wrapper for emoji or fallback when terminal doesn't support emoji
    fn format_emoji(&self) -> String {
        let width = console::measure_text_width(&self.emoji);
        let pad_len = width * width;
        let pad = format!("{:<pad_len$}", "\u{0020}", pad_len = pad_len);
        Emoji(&self.emoji, &pad).to_string()
    }
}

/// Display implementation for a banner entry
impl Display for BannerEntry {
    /// Display formatter for the given banner entry
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\u{0020}{:\u{0020}<indent$}{:\u{0020}<width$}\u{2502}\u{0020}{}",
            self.format_emoji(),
            self.title,
            self.value,
            indent = INDENT,
            width = COL_WIDTH
        )
    }
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

/// Prints the banner to stdout.
///
/// Only prints those settings which are either always present, or passed in by the user.
pub async fn initialize<W>(
    targets: &[String],
    config: &Configuration,
    version: &str,
    mut writer: W,
    tx_stats: UnboundedSender<StatCommand>,
) -> Result<()>
where
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

    writeln!(&mut writer, "{}", artwork)?;
    writeln!(&mut writer, "{}", top)?;

    // begin with always printed items
    for target in targets {
        let tgt = BannerEntry::new("🎯", "Target Url", target);
        writeln!(&mut writer, "{}", tgt)?;
    }

    let mut codes = vec![];

    for code in &config.status_codes {
        codes.push(status_colorizer(&code.to_string()))
    }

    let threads = BannerEntry::new("🚀", "Threads", &config.threads.to_string());
    writeln!(&mut writer, "{}", threads)?;

    let words = BannerEntry::new("📖", "Wordlist", &config.wordlist);
    writeln!(&mut writer, "{}", words)?;

    let status_codes = BannerEntry::new("👌", "Status Codes", &format!("[{}]", codes.join(", ")));
    writeln!(&mut writer, "{}", status_codes)?;

    if !config.filter_status.is_empty() {
        // exception here for optional print due to me wanting the allows and denys to be printed
        // one after the other
        let mut code_filters = vec![];

        for code in &config.filter_status {
            code_filters.push(status_colorizer(&code.to_string()))
        }

        let banner_cfs = BannerEntry::new(
            "🗑",
            "Status Code Filters",
            &format!("[{}]", code_filters.join(", ")),
        );

        writeln!(&mut writer, "{}", banner_cfs)?;
    }

    let timeout = BannerEntry::new("💥", "Timeout (secs)", &config.timeout.to_string());
    writeln!(&mut writer, "{}", timeout)?;

    let user_agent = BannerEntry::new("🦡", "User-Agent", &config.user_agent);
    writeln!(&mut writer, "{}", user_agent)?;

    // followed by the maybe printed or variably displayed values
    if !config.config.is_empty() {
        let banner_cfg = BannerEntry::new("💉", "Config File", &config.config);
        writeln!(&mut writer, "{}", banner_cfg)?;
    }

    if !config.proxy.is_empty() {
        let proxy = BannerEntry::new("💎", "Proxy", &config.proxy);
        writeln!(&mut writer, "{}", proxy)?;
    }

    if !config.replay_proxy.is_empty() {
        // i include replay codes logic here because in config.rs, replay codes are set to the
        // value in status codes, meaning it's never empty

        let mut replay_codes = vec![];

        for code in &config.replay_codes {
            replay_codes.push(status_colorizer(&code.to_string()))
        }

        let banner_rcs = BannerEntry::new(
            "📼",
            "Replay Proxy Codes",
            &format!("[{}]", replay_codes.join(", ")),
        );

        let rproxy = BannerEntry::new("🎥", "Replay Proxy", &config.replay_proxy);

        writeln!(&mut writer, "{}", rproxy)?;
        writeln!(&mut writer, "{}", banner_rcs)?;
    }

    for (name, value) in &config.headers {
        let header = BannerEntry::new("🤯", "Header", &format!("{}: {}", name, value));
        writeln!(&mut writer, "{}", header)?;
    }

    for filter in &config.filter_size {
        let sz_filter = BannerEntry::new("💢", "Size Filter", &filter.to_string());
        writeln!(&mut writer, "{}", sz_filter)?;
    }

    for filter in &config.filter_similar {
        let sim_filter = BannerEntry::new("💢", "Similarity Filter", filter);
        writeln!(&mut writer, "{}", sim_filter)?;
    }

    for filter in &config.filter_word_count {
        let wc_filter = BannerEntry::new("💢", "Word Count Filter", &filter.to_string());
        writeln!(&mut writer, "{}", wc_filter)?;
    }

    for filter in &config.filter_line_count {
        let lc_filter = BannerEntry::new("💢", "Line Count Filter", &filter.to_string());
        writeln!(&mut writer, "{}", lc_filter)?;
    }

    for filter in &config.filter_regex {
        let reg_filter = BannerEntry::new("💢", "Regex Filter", filter);
        writeln!(&mut writer, "{}", reg_filter)?;
    }

    if config.extract_links {
        let ext_links = BannerEntry::new("🔎", "Extract Links", &config.extract_links.to_string());
        writeln!(&mut writer, "{}", ext_links)?;
    }

    if config.json {
        let json = BannerEntry::new("🧔", "JSON Output", &config.json.to_string());
        writeln!(&mut writer, "{}", json)?;
    }

    for query in &config.queries {
        let query = BannerEntry::new("🤔", "Query Parameter", &format!("{}={}", query.0, query.1));
        writeln!(&mut writer, "{}", query)?;
    }

    if !config.output.is_empty() {
        let out = BannerEntry::new("💾", "Output File", &config.output);
        writeln!(&mut writer, "{}", out)?;
    }

    if !config.debug_log.is_empty() {
        let debug_log = BannerEntry::new("🪲", "Debugging Log", &config.debug_log);
        writeln!(&mut writer, "{}", debug_log)?;
    }

    if !config.extensions.is_empty() {
        let b_exts = BannerEntry::new(
            "💲",
            "Extensions",
            &format!("[{}]", config.extensions.join(", ")),
        );
        writeln!(&mut writer, "{}", b_exts)?;
    }

    if config.insecure {
        let b_insec = BannerEntry::new("🔓", "Insecure", &config.insecure.to_string());
        writeln!(&mut writer, "{}", b_insec)?;
    }

    if config.redirects {
        let b_follow = BannerEntry::new("📍", "Follow Redirects", &config.redirects.to_string());
        writeln!(&mut writer, "{}", b_follow)?;
    }

    if config.dont_filter {
        let b_wild = BannerEntry::new("🤪", "Filter Wildcards", &(!config.dont_filter).to_string());
        writeln!(&mut writer, "{}", b_wild)?;
    }

    let volume = ["🔈", "🔉", "🔊", "📢"];
    if let 1..=4 = config.verbosity {
        //speaker medium volume (increasing with verbosity to loudspeaker)
        let vol = BannerEntry::new(
            volume[config.verbosity as usize - 1],
            "Verbosity",
            &config.verbosity.to_string(),
        );
        writeln!(&mut writer, "{}", vol)?;
    }

    if config.add_slash {
        let add = BannerEntry::new("🪓", "Add Slash", &config.add_slash.to_string());
        writeln!(&mut writer, "{}", add)?;
    }

    let b_recurse = if !config.no_recursion {
        let depth = if config.depth == 0 {
            "INFINITE".to_string()
        } else {
            config.depth.to_string()
        };

        BannerEntry::new("🔃", "Recursion Depth", &depth)
    } else {
        BannerEntry::new("🚫", "Do Not Recurse", &config.no_recursion.to_string())
    };

    writeln!(&mut writer, "{}", b_recurse)?;

    if config.scan_limit > 0 {
        let s_lim = BannerEntry::new(
            "🦥",
            "Concurrent Scan Limit",
            &config.scan_limit.to_string(),
        );
        writeln!(&mut writer, "{}", s_lim)?;
    }

    if !config.time_limit.is_empty() {
        let t_lim = BannerEntry::new("🕖", "Time Limit", &config.time_limit);
        writeln!(&mut writer, "{}", t_lim)?;
    }

    if matches!(status, UpdateStatus::OutOfDate) {
        let update = BannerEntry::new(
            "🎉",
            "New Version Available",
            "https://github.com/epi052/feroxbuster/releases/latest",
        );
        writeln!(&mut writer, "{}", update)?;
    }

    writeln!(&mut writer, "{}", bottom)?;

    writeln!(
        &mut writer,
        " 🏁  Press [{}] to use the {}™",
        style("ENTER").yellow(),
        style("Scan Cancel Menu").bright().yellow(),
    )?;

    writeln!(&mut writer, "{}", addl_section)?;

    Ok(())
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

        initialize(&[], &config, VERSION, stderr(), tx)
            .await
            .unwrap();
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
        .await
        .unwrap();
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
        .await
        .unwrap();
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
        .await
        .unwrap();
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
        .await
        .unwrap();
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
