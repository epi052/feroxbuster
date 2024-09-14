use super::entry::BannerEntry;
use crate::{
    client,
    config::Configuration,
    event_handlers::Handles,
    utils::{make_request, parse_url_with_raw_path, status_colorizer},
    DEFAULT_IGNORED_EXTENSIONS, DEFAULT_METHOD, DEFAULT_STATUS_CODES, VERSION,
};
use anyhow::{bail, Result};
use console::{style, Emoji};
use serde_json::Value;
use std::collections::HashMap;
use std::{io::Write, sync::Arc};

/// Url used to query github's api; specifically used to look for the latest tagged release name
pub const UPDATE_URL: &str = "https://api.github.com/repos/epi052/feroxbuster/releases/latest";

/// Simple enum to hold three different update states
#[derive(Debug)]
pub(super) enum UpdateStatus {
    /// this version and latest release are the same
    UpToDate,

    /// this version and latest release are not the same
    OutOfDate,

    /// some error occurred during version check
    Unknown,
}

/// Banner object, contains multiple BannerEntry's and knows how to display itself
pub struct Banner {
    /// all live targets
    targets: Vec<BannerEntry>,

    /// represents Configuration.status_codes
    status_codes: BannerEntry,

    /// represents Configuration.filter_status
    filter_status: BannerEntry,

    /// represents Configuration.threads
    threads: BannerEntry,

    /// represents Configuration.wordlist
    wordlist: BannerEntry,

    /// represents Configuration.timeout
    timeout: BannerEntry,

    /// represents Configuration.user_agent
    user_agent: BannerEntry,

    /// represents Configuration.random_agent
    random_agent: BannerEntry,

    /// represents Configuration.config
    config: BannerEntry,

    /// represents Configuration.proxy
    proxy: BannerEntry,

    /// represents Configuration.client_key
    client_key: BannerEntry,

    /// represents Configuration.client_cert
    client_cert: BannerEntry,

    /// represents Configuration.server_certs
    server_certs: BannerEntry,

    /// represents Configuration.replay_proxy
    replay_proxy: BannerEntry,

    /// represents Configuration.replay_codes
    replay_codes: BannerEntry,

    /// represents Configuration.headers
    headers: Vec<BannerEntry>,

    /// represents Configuration.filter_size
    filter_size: Vec<BannerEntry>,

    /// represents Configuration.filter_similar
    filter_similar: Vec<BannerEntry>,

    /// represents Configuration.filter_word_count
    filter_word_count: Vec<BannerEntry>,

    /// represents Configuration.filter_line_count
    filter_line_count: Vec<BannerEntry>,

    /// represents Configuration.filter_regex
    filter_regex: Vec<BannerEntry>,

    /// represents Configuration.extract_links
    extract_links: BannerEntry,

    /// represents Configuration.json
    json: BannerEntry,

    /// represents Configuration.output
    output: BannerEntry,

    /// represents Configuration.debug_log
    debug_log: BannerEntry,

    /// represents Configuration.extensions
    extensions: BannerEntry,

    /// represents Configuration.methods
    methods: BannerEntry,

    /// represents Configuration.data
    data: BannerEntry,

    /// represents Configuration.insecure
    insecure: BannerEntry,

    /// represents Configuration.redirects
    redirects: BannerEntry,

    /// represents Configuration.dont_filter
    dont_filter: BannerEntry,

    /// represents Configuration.queries
    queries: Vec<BannerEntry>,

    /// represents Configuration.verbosity
    verbosity: BannerEntry,

    /// represents Configuration.add_slash
    add_slash: BannerEntry,

    /// represents Configuration.no_recursion
    no_recursion: BannerEntry,

    /// represents Configuration.scan_limit
    scan_limit: BannerEntry,

    /// represents Configuration.time_limit
    time_limit: BannerEntry,

    /// represents Configuration.rate_limit
    rate_limit: BannerEntry,

    /// represents Configuration.parallel
    parallel: BannerEntry,

    /// represents Configuration.auto_tune
    auto_tune: BannerEntry,

    /// represents Configuration.auto_bail
    auto_bail: BannerEntry,

    /// represents Configuration.url_denylist
    url_denylist: Vec<BannerEntry>,

    /// current version of feroxbuster
    pub(super) version: String,

    /// whether or not there is a known new version
    pub(super) update_status: UpdateStatus,

    /// represents Configuration.collect_extensions
    collect_extensions: BannerEntry,

    /// represents Configuration.dont_collect
    dont_collect: BannerEntry,

    /// represents Configuration.collect_backups
    collect_backups: BannerEntry,

    /// represents Configuration.collect_words
    collect_words: BannerEntry,

    /// represents Configuration.collect_words
    force_recursion: BannerEntry,

    /// represents Configuration.protocol
    protocol: BannerEntry,

    /// represents Configuration.scan_dir_listings
    scan_dir_listings: BannerEntry,

    /// represents Configuration.limit_bars
    limit_bars: BannerEntry,
}

/// implementation of Banner
impl Banner {
    /// Create a new Banner from a Configuration and live targets
    pub fn new(tgts: &[String], config: &Configuration) -> Self {
        let mut targets = Vec::new();
        let mut url_denylist = Vec::new();
        let mut code_filters = Vec::new();
        let mut replay_codes = Vec::new();
        let mut headers = Vec::new();
        let mut filter_size = Vec::new();
        let mut filter_similar = Vec::new();
        let mut filter_word_count = Vec::new();
        let mut filter_line_count = Vec::new();
        let mut filter_regex = Vec::new();
        let mut queries = Vec::new();

        for target in tgts {
            targets.push(BannerEntry::new("🎯", "Target Url", target));
        }

        for denied_url in &config.url_denylist {
            url_denylist.push(BannerEntry::new(
                "🚫",
                "Don't Scan Url",
                denied_url.as_str(),
            ));
        }

        for denied_regex in &config.regex_denylist {
            url_denylist.push(BannerEntry::new(
                "🚫",
                "Don't Scan Regex",
                denied_regex.as_str(),
            ));
        }

        // the +2 is for the 2 experimental status codes we add to the default list manually
        let status_codes = if config.status_codes.len() == DEFAULT_STATUS_CODES.len() + 2 {
            let all_str = format!(
                "{} {} {}{}",
                style("All").cyan(),
                style("Status").green(),
                style("Codes").yellow(),
                style("!").red()
            );
            BannerEntry::new("👌", "Status Codes", &all_str)
        } else {
            let mut codes = vec![];

            for code in &config.status_codes {
                codes.push(status_colorizer(&code.to_string()))
            }

            BannerEntry::new("👌", "Status Codes", &format!("[{}]", codes.join(", ")))
        };

        for code in &config.filter_status {
            code_filters.push(status_colorizer(&code.to_string()))
        }
        let filter_status = BannerEntry::new(
            "💢",
            "Status Code Filters",
            &format!("[{}]", code_filters.join(", ")),
        );

        for code in &config.replay_codes {
            replay_codes.push(status_colorizer(&code.to_string()))
        }
        let replay_codes = BannerEntry::new(
            "📼",
            "Replay Proxy Codes",
            &format!("[{}]", replay_codes.join(", ")),
        );

        for (name, value) in &config.headers {
            headers.push(BannerEntry::new(
                "🤯",
                "Header",
                &format!("{name}: {value}"),
            ));
        }

        for filter in &config.filter_size {
            filter_size.push(BannerEntry::new("💢", "Size Filter", &filter.to_string()));
        }

        for filter in &config.filter_similar {
            filter_similar.push(BannerEntry::new("💢", "Similarity Filter", filter));
        }

        for filter in &config.filter_word_count {
            filter_word_count.push(BannerEntry::new(
                "💢",
                "Word Count Filter",
                &filter.to_string(),
            ));
        }

        for filter in &config.filter_line_count {
            filter_line_count.push(BannerEntry::new(
                "💢",
                "Line Count Filter",
                &filter.to_string(),
            ));
        }

        for filter in &config.filter_regex {
            filter_regex.push(BannerEntry::new("💢", "Regex Filter", filter));
        }

        for query in &config.queries {
            queries.push(BannerEntry::new(
                "🤔",
                "Query Parameter",
                &format!("{}={}", query.0, query.1),
            ));
        }

        let volume = ["🔈", "🔉", "🔊", "📢"];
        let verbosity = if let 1..=4 = config.verbosity {
            //speaker medium volume (increasing with verbosity to loudspeaker)
            BannerEntry::new(
                volume[config.verbosity as usize - 1],
                "Verbosity",
                &config.verbosity.to_string(),
            )
        } else {
            BannerEntry::default()
        };

        let no_recursion = if !config.no_recursion {
            let depth = if config.depth == 0 {
                "INFINITE".to_string()
            } else {
                config.depth.to_string()
            };

            BannerEntry::new("🔃", "Recursion Depth", &depth)
        } else {
            BannerEntry::new("🚫", "Do Not Recurse", &config.no_recursion.to_string())
        };

        let protocol = if config.protocol.to_lowercase() == "http" {
            BannerEntry::new("🔓", "Default Protocol", &config.protocol)
        } else {
            BannerEntry::new("🔒", "Default Protocol", &config.protocol)
        };

        let scan_limit = BannerEntry::new(
            "🦥",
            "Concurrent Scan Limit",
            &config.scan_limit.to_string(),
        );

        let force_recursion =
            BannerEntry::new("🤘", "Force Recursion", &config.force_recursion.to_string());
        let replay_proxy = BannerEntry::new("🎥", "Replay Proxy", &config.replay_proxy);
        let auto_tune = BannerEntry::new("🎶", "Auto Tune", &config.auto_tune.to_string());
        let auto_bail = BannerEntry::new("🙅", "Auto Bail", &config.auto_bail.to_string());
        let scan_dir_listings = BannerEntry::new(
            "📂",
            "Scan Dir Listings",
            &config.scan_dir_listings.to_string(),
        );
        let cfg = BannerEntry::new("💉", "Config File", &config.config);
        let proxy = BannerEntry::new("💎", "Proxy", &config.proxy);
        let server_certs = BannerEntry::new(
            "🏅",
            "Server Certificates",
            &format!("[{}]", config.server_certs.join(", ")),
        );
        let client_cert = BannerEntry::new("🏅", "Client Certificate", &config.client_cert);
        let client_key = BannerEntry::new("🔑", "Client Key", &config.client_key);
        let threads = BannerEntry::new("🚀", "Threads", &config.threads.to_string());
        let limit_bars =
            BannerEntry::new("📊", "Limit Dir Scan Bars", &config.limit_bars.to_string());
        let wordlist = BannerEntry::new("📖", "Wordlist", &config.wordlist);
        let timeout = BannerEntry::new("💥", "Timeout (secs)", &config.timeout.to_string());
        let user_agent = BannerEntry::new("🦡", "User-Agent", &config.user_agent);
        let random_agent = BannerEntry::new("🦡", "User-Agent", "Random");
        let extract_links =
            BannerEntry::new("🔎", "Extract Links", &config.extract_links.to_string());
        let json = BannerEntry::new("🧔", "JSON Output", &config.json.to_string());
        let output = BannerEntry::new("💾", "Output File", &config.output);
        let debug_log = BannerEntry::new("🪲", "Debugging Log", &config.debug_log);
        let extensions = BannerEntry::new(
            "💲",
            "Extensions",
            &format!("[{}]", config.extensions.join(", ")),
        );
        let methods = BannerEntry::new(
            "🏁",
            "HTTP methods",
            &format!("[{}]", config.methods.join(", ")),
        );

        let dont_collect = if config.dont_collect == DEFAULT_IGNORED_EXTENSIONS {
            // default has 30+ extensions, just trim it up
            BannerEntry::new(
                "💸",
                "Ignored Extensions",
                "[Images, Movies, Audio, etc...]",
            )
        } else {
            BannerEntry::new(
                "💸",
                "Ignored Extensions",
                &format!("[{}]", config.dont_collect.join(", ")),
            )
        };

        let offset = std::cmp::min(config.data.len(), 30);
        let data = String::from_utf8(config.data[..offset].to_vec())
            .unwrap_or_else(|_err| {
                format!(
                    "{:x?} ...",
                    &config.data[..std::cmp::min(config.data.len(), 13)]
                )
            })
            .replace('\n', " ")
            .replace('\r', "");
        let data = BannerEntry::new("💣", "HTTP Body", &data);
        let insecure = BannerEntry::new("🔓", "Insecure", &config.insecure.to_string());
        let redirects = BannerEntry::new("📍", "Follow Redirects", &config.redirects.to_string());
        let dont_filter =
            BannerEntry::new("🤪", "Filter Wildcards", &(!config.dont_filter).to_string());
        let add_slash = BannerEntry::new("🪓", "Add Slash", &config.add_slash.to_string());
        let time_limit = BannerEntry::new("🕖", "Time Limit", &config.time_limit);
        let parallel = BannerEntry::new("🛤", "Parallel Scans", &config.parallel.to_string());
        let rate_limit =
            BannerEntry::new("🚧", "Requests per Second", &config.rate_limit.to_string());
        let collect_extensions = BannerEntry::new(
            "💰",
            "Collect Extensions",
            &config.collect_extensions.to_string(),
        );
        let collect_backups =
            BannerEntry::new("🏦", "Collect Backups", &config.collect_backups.to_string());

        let collect_words =
            BannerEntry::new("🤑", "Collect Words", &config.collect_words.to_string());

        Self {
            targets,
            status_codes,
            threads,
            wordlist,
            filter_status,
            timeout,
            user_agent,
            random_agent,
            auto_bail,
            auto_tune,
            proxy,
            client_cert,
            client_key,
            server_certs,
            replay_codes,
            replay_proxy,
            headers,
            filter_size,
            filter_similar,
            filter_word_count,
            filter_line_count,
            filter_regex,
            extract_links,
            parallel,
            json,
            queries,
            output,
            debug_log,
            extensions,
            methods,
            data,
            insecure,
            dont_filter,
            redirects,
            verbosity,
            add_slash,
            no_recursion,
            rate_limit,
            scan_limit,
            force_recursion,
            time_limit,
            url_denylist,
            collect_extensions,
            collect_backups,
            collect_words,
            dont_collect,
            config: cfg,
            scan_dir_listings,
            protocol,
            limit_bars,
            version: VERSION.to_string(),
            update_status: UpdateStatus::Unknown,
        }
    }

    /// get a fancy header for the banner
    fn header(&self) -> String {
        let artwork = format!(
            r#"
 ___  ___  __   __     __      __         __   ___
|__  |__  |__) |__) | /  `    /  \ \_/ | |  \ |__
|    |___ |  \ |  \ | \__,    \__/ / \ | |__/ |___
by Ben "epi" Risher {}                 ver: {}"#,
            Emoji("🤓", &format!("{:<2}", "\u{0020}")),
            self.version
        );

        let top = "───────────────────────────┬──────────────────────";

        format!("{artwork}\n{top}")
    }

    /// get a fancy footer for the banner
    fn footer(&self) -> String {
        let addl_section = "──────────────────────────────────────────────────";
        let bottom = "───────────────────────────┴──────────────────────";

        let instructions = format!(
            " 🏁  Press [{}] to use the {}™",
            style("ENTER").yellow(),
            style("Scan Management Menu").bright().yellow(),
        );

        format!("{bottom}\n{instructions}\n{addl_section}")
    }

    /// Makes a request to the given url, expecting to receive a JSON response that contains a field
    /// named `tag_name` that holds a value representing the latest tagged release of this tool.
    ///
    /// ex: v1.1.0
    pub async fn check_for_updates(&mut self, url: &str, handles: Arc<Handles>) -> Result<()> {
        log::trace!("enter: needs_update({}, {:?})", url, handles);

        let api_url = parse_url_with_raw_path(url)?;

        // we don't want to leak sensitive header info / include auth headers
        // with the github api request, so we'll build a client specifically
        // for this task. thanks to @stuhlmann for the suggestion!
        let client = client::initialize(
            handles.config.timeout,
            "feroxbuster-update-check",
            handles.config.redirects,
            handles.config.insecure,
            &HashMap::new(),
            Some(&handles.config.proxy),
            &handles.config.server_certs,
            Some(&handles.config.client_cert),
            Some(&handles.config.client_key),
        )?;
        let level = handles.config.output_level;
        let tx_stats = handles.stats.tx.clone();

        let result = make_request(
            &client,
            &api_url,
            DEFAULT_METHOD,
            None,
            level,
            &handles.config,
            tx_stats,
        )
        .await?;

        let body = result.text().await?;

        let json_response: Value = serde_json::from_str(&body)?;

        let latest_version = match json_response["tag_name"].as_str() {
            Some(tag) => tag.trim_start_matches('v'),
            None => {
                bail!("JSON has no tag_name: {}", json_response);
            }
        };

        // if we've gotten this far, we have a string in the form of X.X.X where X is a number
        // all that's left is to compare the current version with the version found above

        if latest_version == self.version {
            // there's really only two possible outcomes if we accept that the tag conforms to
            // the X.X.X pattern:
            //   1. the version strings match, meaning we're up to date
            //   2. the version strings do not match, meaning we're out of date
            //
            // except for developers working on this code, nobody should ever be in a situation
            // where they have a version greater than the latest tagged release
            self.update_status = UpdateStatus::UpToDate;
        } else {
            self.update_status = UpdateStatus::OutOfDate;
        }

        log::trace!("exit: check_for_updates -> {:?}", self.update_status);
        Ok(())
    }

    /// display the banner on Write writer
    pub fn print_to<W>(&self, mut writer: W, config: Arc<Configuration>) -> Result<()>
    where
        W: Write,
    {
        writeln!(&mut writer, "{}", self.header())?;

        // begin with always printed items
        for target in &self.targets {
            writeln!(&mut writer, "{target}")?;
        }

        for denied_url in &self.url_denylist {
            writeln!(&mut writer, "{denied_url}")?;
        }

        writeln!(&mut writer, "{}", self.threads)?;
        writeln!(&mut writer, "{}", self.wordlist)?;

        if config.filter_status.is_empty() {
            // -C and -s are mutually exclusive, and -s meaning changes when -C is used
            // so only print one or the other
            writeln!(&mut writer, "{}", self.status_codes)?;
        } else {
            writeln!(&mut writer, "{}", self.filter_status)?;
        }

        writeln!(&mut writer, "{}", self.timeout)?;

        if config.random_agent {
            writeln!(&mut writer, "{}", self.random_agent)?;
        } else {
            writeln!(&mut writer, "{}", self.user_agent)?;
        }

        // followed by the maybe printed or variably displayed values
        if !config.request_file.is_empty() || !config.target_url.starts_with("http") {
            writeln!(&mut writer, "{}", self.protocol)?;
        }

        if config.limit_bars > 0 {
            writeln!(&mut writer, "{}", self.limit_bars)?;
        }

        if !config.config.is_empty() {
            writeln!(&mut writer, "{}", self.config)?;
        }

        if !config.proxy.is_empty() {
            writeln!(&mut writer, "{}", self.proxy)?;
        }

        if !config.client_cert.is_empty() {
            writeln!(&mut writer, "{}", self.client_cert)?;
        }

        if !config.client_key.is_empty() {
            writeln!(&mut writer, "{}", self.client_key)?;
        }

        if !config.server_certs.is_empty() {
            writeln!(&mut writer, "{}", self.server_certs)?;
        }

        if !config.replay_proxy.is_empty() {
            // i include replay codes logic here because in config.rs, replay codes are set to the
            // value in status codes, meaning it's never empty
            writeln!(&mut writer, "{}", self.replay_proxy)?;
            writeln!(&mut writer, "{}", self.replay_codes)?;
        }

        for header in &self.headers {
            writeln!(&mut writer, "{header}")?;
        }

        for filter in &self.filter_size {
            writeln!(&mut writer, "{filter}")?;
        }

        for filter in &self.filter_similar {
            writeln!(&mut writer, "{filter}")?;
        }

        for filter in &self.filter_word_count {
            writeln!(&mut writer, "{filter}")?;
        }

        for filter in &self.filter_line_count {
            writeln!(&mut writer, "{filter}")?;
        }

        for filter in &self.filter_regex {
            writeln!(&mut writer, "{filter}")?;
        }

        if config.extract_links {
            writeln!(&mut writer, "{}", self.extract_links)?;
        }

        if config.json {
            writeln!(&mut writer, "{}", self.json)?;
        }

        for query in &self.queries {
            writeln!(&mut writer, "{query}")?;
        }

        if !config.output.is_empty() {
            writeln!(&mut writer, "{}", self.output)?;
        }

        if config.scan_dir_listings {
            writeln!(&mut writer, "{}", self.scan_dir_listings)?;
        }

        if !config.debug_log.is_empty() {
            writeln!(&mut writer, "{}", self.debug_log)?;
        }

        if !config.extensions.is_empty() {
            writeln!(&mut writer, "{}", self.extensions)?;
        }

        if config.collect_extensions {
            // dont-collect is active only when collect-extensions is used
            writeln!(&mut writer, "{}", self.collect_extensions)?;
            writeln!(&mut writer, "{}", self.dont_collect)?;
        }

        if config.collect_backups {
            writeln!(&mut writer, "{}", self.collect_backups)?;
        }

        if config.collect_words {
            writeln!(&mut writer, "{}", self.collect_words)?;
        }

        if !config.methods.is_empty() {
            writeln!(&mut writer, "{}", self.methods)?;
        }

        if !config.data.is_empty() {
            writeln!(&mut writer, "{}", self.data)?;
        }

        if config.insecure {
            writeln!(&mut writer, "{}", self.insecure)?;
        }

        if config.auto_bail {
            writeln!(&mut writer, "{}", self.auto_bail)?;
        }
        if config.auto_tune {
            writeln!(&mut writer, "{}", self.auto_tune)?;
        }

        if config.redirects {
            writeln!(&mut writer, "{}", self.redirects)?;
        }

        if config.dont_filter {
            writeln!(&mut writer, "{}", self.dont_filter)?;
        }

        if let 1..=4 = config.verbosity {
            writeln!(&mut writer, "{}", self.verbosity)?;
        }

        if config.add_slash {
            writeln!(&mut writer, "{}", self.add_slash)?;
        }

        writeln!(&mut writer, "{}", self.no_recursion)?;

        if config.force_recursion {
            writeln!(&mut writer, "{}", self.force_recursion)?;
        }

        if config.scan_limit > 0 {
            writeln!(&mut writer, "{}", self.scan_limit)?;
        }

        if config.parallel > 0 {
            writeln!(&mut writer, "{}", self.parallel)?;
        }

        if config.rate_limit > 0 {
            writeln!(&mut writer, "{}", self.rate_limit)?;
        }

        if !config.time_limit.is_empty() {
            writeln!(&mut writer, "{}", self.time_limit)?;
        }

        if matches!(self.update_status, UpdateStatus::OutOfDate) {
            let update = BannerEntry::new(
                "🎉",
                "New Version Available",
                "https://github.com/epi052/feroxbuster/releases/latest",
            );
            writeln!(&mut writer, "{update}")?;
        }

        writeln!(&mut writer, "{}", self.footer())?;

        Ok(())
    }
}
