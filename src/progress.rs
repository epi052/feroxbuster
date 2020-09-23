use crate::config::PROGRESS_BAR;
use indicatif::{ProgressBar, ProgressStyle};

pub fn add_bar(prefix: &str, length: u64, hidden: bool) -> ProgressBar {
    let style = ProgressStyle::default_bar()
        .template("[{bar:.cyan/blue}] - {elapsed:<4} {pos:>7}/{len:7} {per_sec:7} {prefix}")
        .progress_chars("#>-");

    let progress_bar = if hidden {
        // PROGRESS_BAR.add(ProgressBar::hidden())
        ProgressBar::hidden()
    } else {
        PROGRESS_BAR.add(ProgressBar::new(length))
    };

    progress_bar.set_style(style);

    progress_bar.set_prefix(&prefix);

    progress_bar
}
