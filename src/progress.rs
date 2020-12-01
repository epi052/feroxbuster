use crate::config::{CONFIGURATION, PROGRESS_BAR};
use indicatif::{ProgressBar, ProgressStyle};

/// Add an [indicatif::ProgressBar](https://docs.rs/indicatif/latest/indicatif/struct.ProgressBar.html)
/// to the global [PROGRESS_BAR](../config/struct.PROGRESS_BAR.html)
pub fn add_bar(prefix: &str, length: u64, hidden: bool, hide_per_sec: bool) -> ProgressBar {
    let style = if hidden || CONFIGURATION.quiet {
        ProgressStyle::default_bar().template("")
    } else if hide_per_sec {
        ProgressStyle::default_bar()
            .template(&format!(
                "[{{bar:.cyan/blue}}] - {{elapsed:<4}} {{pos:>7}}/{{len:7}} {:7} {{prefix}}",
                "-"
            ))
            .progress_chars("#>-")
    } else {
        ProgressStyle::default_bar()
            .template("[{bar:.cyan/blue}] - {elapsed:<4} {pos:>7}/{len:7} {per_sec:7} {prefix}")
            .progress_chars("#>-")
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
        let p1 = add_bar("prefix", 2, true, false); // hidden
        let p2 = add_bar("prefix", 2, false, true); // no per second field
        let p3 = add_bar("prefix", 2, false, false); // normal bar

        p1.finish();
        p2.finish();
        p3.finish();

        assert!(p1.is_finished());
        assert!(p2.is_finished());
        assert!(p3.is_finished());
    }
}
