use console::{measure_text_width, Emoji};
use std::fmt;

/// Initial visual indentation size used in formatting banner entries
const INDENT: usize = 3;

/// Column width used in formatting banner entries
const COL_WIDTH: usize = 22;

/// Represents a single line on the banner
#[derive(Default)]
pub(super) struct BannerEntry {
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
        let width = measure_text_width(&self.emoji);
        let pad_len = width * width;
        let pad = format!("{:<pad_len$}", "\u{0020}", pad_len = pad_len);
        Emoji(&self.emoji, &pad).to_string()
    }
}

/// Display implementation for a banner entry
impl fmt::Display for BannerEntry {
    /// Display formatter for the given banner entry
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
