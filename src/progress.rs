use crate::config::{CONFIGURATION, PROGRESS_BAR};
use indicatif::{ProgressBar, ProgressStyle};

/// Types of ProgressBars that can be added to `PROGRESS_BAR`
pub enum BarType {
    /// no template used / not visible
    Hidden,

    /// normal directory status bar (reqs/sec shown)
    Default,

    /// similar to `Default`, except `-` is used in place of line/word/char count
    Message,

    /// bar used to show overall scan metrics
    Total,
}

/// Add an [indicatif::ProgressBar](https://docs.rs/indicatif/latest/indicatif/struct.ProgressBar.html)
/// to the global [PROGRESS_BAR](../config/struct.PROGRESS_BAR.html)
pub fn add_bar(prefix: &str, length: u64, bar_type: BarType) -> ProgressBar {
    let mut style = ProgressStyle::default_bar().progress_chars("#>-");

    style = if CONFIGURATION.quiet {
        style.template("")
    } else {
        match bar_type {
            BarType::Hidden => style.template(""),
            BarType::Default => style.template(
                "[{bar:.cyan/blue}] - {elapsed:<4} {pos:>7}/{len:7} {per_sec:7} {prefix}",
            ),
            BarType::Message => style.template(&format!(
                "[{{bar:.cyan/blue}}] - {{elapsed:<4}} {{pos:>7}}/{{len:7}} {:7} {{prefix}}",
                "-"
            )),
            BarType::Total => {
                style.template("[{bar:.yellow/blue}] - {elapsed:<4} {pos:>7}/{len:7} {eta:7} {msg}")
            }
        }
    };

    let progress_bar = PROGRESS_BAR.add(ProgressBar::new(length));

    progress_bar.set_style(style);

    progress_bar.set_prefix(&prefix);

    progress_bar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// hit all code branches for add_bar
    fn add_bar_with_all_configurations() {
        let p1 = add_bar("prefix", 2, BarType::Hidden); // hidden
        let p2 = add_bar("prefix", 2, BarType::Message); // no per second field
        let p3 = add_bar("prefix", 2, BarType::Default); // normal bar
        let p4 = add_bar("prefix", 2, BarType::Total); // totals bar

        p1.finish();
        p2.finish();
        p3.finish();
        p4.finish();

        assert!(p1.is_finished());
        assert!(p2.is_finished());
        assert!(p3.is_finished());
        assert!(p4.is_finished());
    }
}
