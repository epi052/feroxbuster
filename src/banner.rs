use crate::{VERSION, config::CONFIGURATION, utils::status_colorizer};

macro_rules! format_banner_entry_helper {
    // \u{0020} -> unicode space
    // \u{2502} -> vertical box drawing character, i.e. │
    ($rune:expr, $name:expr, $value:expr, $indent:expr, $col_width:expr) => {
        format!("\u{0020}{:\u{0020}<indent$}{:\u{0020}<col_w$}\u{2502}\u{0020}{}", $rune, $name, $value, indent=$indent, col_w=$col_width)
    };
    ($rune:expr, $name:expr, $value:expr, $value2:expr, $indent:expr, $col_width:expr) => {
        format!("\u{0020}{:\u{0020}<indent$}{:\u{0020}<col_w$}\u{2502}\u{0020}{}:\u{0020}{}", $rune, $name, $value, $value2, indent=$indent, col_w=$col_width)
    }
}

macro_rules! format_banner_entry {
    // 4 -> unicode emoji padding width
    // 22 -> column width (when unicode rune is 4 bytes wide, 23 when it's 3)
    // hardcoded since macros don't allow let statements
    ($rune:expr, $name:expr, $value:expr) => {
        format_banner_entry_helper!($rune, $name, $value, 3, 22)
    };
    ($rune:expr, $name:expr, $value1:expr, $value2:expr) => {
        format_banner_entry_helper!($rune, $name, $value1, $value2, 3, 22)
    }

}


pub fn initialize(targets: &[String]) {
    let artwork = format!(r#"
 ___  ___  __   __     __      __         __   ___
|__  |__  |__) |__) | /  `    /  \ \_/ | |  \ |__
|    |___ |  \ |  \ | \__,    \__/ / \ | |__/ |___
by Ben "epi" Risher {}                  ver: {}"#, '\u{1F913}', VERSION);

    let top = "───────────────────────────┬──────────────────────";
    let bottom = "───────────────────────────┴──────────────────────";

    println!("{}", artwork);
    println!("{}", top);

    // begin with always printed items
    for target in targets {
        println!("{}", format_banner_entry!("\u{1F3af}", "Target Url", target));  // 🎯
    }

    let mut codes = vec![];

    for code in &CONFIGURATION.statuscodes {
        codes.push(status_colorizer(&code.to_string()))
    }

    println!("{}", format_banner_entry!("\u{1F680}", "Threads", CONFIGURATION.threads));  // 🚀
    println!("{}", format_banner_entry!("\u{1f4d6}", "Wordlist", CONFIGURATION.wordlist));  // 📖
    println!("{}", format_banner_entry!("\u{1F197}", "Status Codes", format!("[{}]", codes.join(", "))));  // 🆗
    println!("{}", format_banner_entry!("\u{1f4a5}", "Timeout (secs)", CONFIGURATION.timeout));  // 💥
    println!("{}", format_banner_entry!("\u{1F9a1}", "User-Agent", CONFIGURATION.useragent));  // 🦡

    // followed by the maybe printed or variably displayed values
    if !CONFIGURATION.proxy.is_empty() {
        println!("{}", format_banner_entry!("\u{1f48e}", "Proxy", CONFIGURATION.proxy));  // 💎
    }

    if !CONFIGURATION.headers.is_empty() {
        for (name, value) in &CONFIGURATION.headers {
            println!("{}", format_banner_entry!("\u{1f92f}", "Header", name, value));  // 🤯
        }
    }

    if !CONFIGURATION.sizefilters.is_empty() {
        for filter in &CONFIGURATION.sizefilters {
            println!("{}", format_banner_entry!("\u{1f4a2}", "Size Filters", filter));  // 💢
        }
    }

    if !CONFIGURATION.output.is_empty() {
        println!("{}", format_banner_entry!("\u{1f4be}", "Output File", CONFIGURATION.output));  // 💾
    }

    if !CONFIGURATION.extensions.is_empty() {
        println!("{}", format_banner_entry!("\u{1f4b2}", "Extensions", format!("[{}]", CONFIGURATION.extensions.join(", "))));  // 💲
    }

    if CONFIGURATION.insecure {
        println!("{}", format_banner_entry!("\u{1f513}", "Insecure", CONFIGURATION.insecure));  // 🔓
    }

    if CONFIGURATION.redirects {
        println!("{}", format_banner_entry!("\u{1f4cd}", "Follow Redirects", CONFIGURATION.redirects));  // 📍
    }

    match CONFIGURATION.verbosity {
        //speaker medium volume (increasing with verbosity to loudspeaker)
        1 => {
            println!("{}", format_banner_entry!("\u{1f508}", "Verbosity", CONFIGURATION.verbosity));  // 🔈
        },
        2 => {
            println!("{}", format_banner_entry!("\u{1f509}", "Verbosity", CONFIGURATION.verbosity));  // 🔉
        },
        3 => {
            println!("{}", format_banner_entry!("\u{1f50a}", "Verbosity", CONFIGURATION.verbosity));  // 🔊
        },
        4 => {
            println!("{}", format_banner_entry!("\u{1f4e2}", "Verbosity", CONFIGURATION.verbosity));  // 📢
        },
        _ => {}
    }

    if CONFIGURATION.addslash {
        println!("{}", format_banner_entry!("\u{1fa93}", "Add Slash", CONFIGURATION.addslash));  // 🪓
    }

    if !CONFIGURATION.norecursion {
        if CONFIGURATION.depth == 0 {
            println!("{}", format_banner_entry!("\u{1f503}", "Recursion Depth", "INFINITE"));  // 🔃
        } else {
            println!("{}", format_banner_entry!("\u{1f503}", "Recursion Depth", CONFIGURATION.depth));  // 🔃
        }
    } else {
        println!("{}", format_banner_entry!("\u{1f6ab}", "Do Not Recurse", CONFIGURATION.norecursion));  // 🚫
    }

    println!("{}", bottom);
}
