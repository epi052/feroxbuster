use crate::{config::Configuration, utils::status_colorizer, VERSION};

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

/// Prints the banner to stdout.
///
/// Only prints those settings which are either always present, or passed in by the user.
pub fn initialize(targets: &[String], config: &Configuration) {
    let artwork = format!(
        r#"
 ___  ___  __   __     __      __         __   ___
|__  |__  |__) |__) | /  `    /  \ \_/ | |  \ |__
|    |___ |  \ |  \ | \__,    \__/ / \ | |__/ |___
by Ben "epi" Risher                    ver: {}"#,
        VERSION
    );

    let top = "───────────────────────────┬──────────────────────";
    let bottom = "───────────────────────────┴──────────────────────";

    eprintln!("{}", artwork);
    eprintln!("{}", top);

    // begin with always printed items
    for target in targets {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Target Url", target)
        ); // 🎯
    }

    let mut codes = vec![];

    for code in &config.statuscodes {
        codes.push(status_colorizer(&code.to_string()))
    }

    eprintln!(
        "{}",
        format_banner_entry!("\u", "Threads", config.threads)
    ); // 🚀
    eprintln!(
        "{}",
        format_banner_entry!("\u", "Wordlist", config.wordlist)
    ); // 📖
    eprintln!(
        "{}",
        format_banner_entry!(
            "\u{1F197}",
            "Status Codes",
            format!("[{}]", codes.join(", "))
        )
    ); // 🆗
    eprintln!(
        "{}",
        format_banner_entry!("\u", "Timeout (secs)", config.timeout)
    ); // 💥
    eprintln!(
        "{}",
        format_banner_entry!("\u", "User-Agent", config.useragent)
    ); // 🦡

    // followed by the maybe printed or variably displayed values
    if !config.config.is_empty() {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Config File", config.config)
        ); // 💉
    }

    if !config.proxy.is_empty() {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Proxy", config.proxy)
        ); // 💎
    }

    if !config.headers.is_empty() {
        for (name, value) in &config.headers {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Header", name, value)
            ); // 🤯
        }
    }

    if !config.sizefilters.is_empty() {
        for filter in &config.sizefilters {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Size Filter", filter)
            ); // 💢
        }
    }

    if !config.queries.is_empty() {
        for query in &config.queries {
            eprintln!(
                "{}",
                format_banner_entry!(
                    "\u",
                    "Query Parameter",
                    format!("{}={}", query.0, query.1)
                )
            ); // 🤔
        }
    }

    if !config.output.is_empty() {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Output File", config.output)
        ); // 💾
    }

    if !config.extensions.is_empty() {
        eprintln!(
            "{}",
            format_banner_entry!(
                "\u",
                "Extensions",
                format!("[{}]", config.extensions.join(", "))
            )
        ); // 💲
    }

    if config.insecure {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Insecure", config.insecure)
        ); // 🔓
    }

    if config.redirects {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Follow Redirects", config.redirects)
        ); // 📍
    }

    if config.dontfilter {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Filter Wildcards", !config.dontfilter)
        ); // 🤪
    }

    match config.verbosity {
        //speaker medium volume (increasing with verbosity to loudspeaker)
        1 => {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Verbosity", config.verbosity)
            ); // 🔈
        }
        2 => {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Verbosity", config.verbosity)
            ); // 🔉
        }
        3 => {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Verbosity", config.verbosity)
            ); // 🔊
        }
        4 => {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Verbosity", config.verbosity)
            ); // 📢
        }
        _ => {}
    }

    if config.addslash {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Add Slash", config.addslash)
        ); // 🪓
    }

    if !config.norecursion {
        if config.depth == 0 {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Recursion Depth", "INFINITE")
            ); // 🔃
        } else {
            eprintln!(
                "{}",
                format_banner_entry!("\u", "Recursion Depth", config.depth)
            ); // 🔃
        }
    } else {
        eprintln!(
            "{}",
            format_banner_entry!("\u", "Do Not Recurse", config.norecursion)
        ); // 🚫
    }

    eprintln!("{}", bottom);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// test to hit no execution of targets for loop in banner
    fn banner_without_targets() {
        let config = Configuration::default();
        initialize(&[], &config);
    }

    #[test]
    /// test to hit no execution of statuscode for loop in banner
    fn banner_without_status_codes() {
        let mut config = Configuration::default();
        config.statuscodes = vec![];
        initialize(&[String::from("http://localhost")], &config);
    }

    #[test]
    /// test to hit an empty config file
    fn banner_without_config_file() {
        let mut config = Configuration::default();
        config.config = String::new();
        initialize(&[String::from("http://localhost")], &config);
    }

    #[test]
    /// test to hit an empty config file
    fn banner_without_queries() {
        let mut config = Configuration::default();
        config.queries = vec![(String::new(), String::new())];
        initialize(&[String::from("http://localhost")], &config);
    }
}
