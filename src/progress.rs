use crate::config::{CONFIGURATION, PROGRESS_BAR};
use indicatif::{ProgressBar, ProgressStyle};

pub fn add_bar(prefix: &str, length: u64, hidden: bool) -> ProgressBar {
    let style = if hidden || CONFIGURATION.quiet {
        ProgressStyle::default_bar().template("")
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
