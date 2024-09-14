use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use lazy_static::lazy_static;

lazy_static! {
    /// Global progress bar that houses other progress bars
    pub static ref PROGRESS_BAR: MultiProgress = MultiProgress::with_draw_target(ProgressDrawTarget::stdout());

    /// Global progress bar that is only used for printing messages that don't jack up other bars
    pub static ref PROGRESS_PRINTER: ProgressBar = add_bar("", 0, BarType::Hidden);
}

/// Types of ProgressBars that can be added to `PROGRESS_BAR`
#[derive(Copy, Clone)]
pub enum BarType {
    /// no template used / not visible
    Hidden,

    /// normal directory status bar (reqs/sec shown)
    Default,

    /// similar to `Default`, except `-` is used in place of line/word/char count
    Message,

    /// bar used to show overall scan metrics
    Total,

    /// simpler output bar that shows only the directory being scanned (no updating info)
    Quiet,
}

/// Add an [indicatif::ProgressBar](https://docs.rs/indicatif/latest/indicatif/struct.ProgressBar.html)
/// to the global [PROGRESS_BAR](../config/struct.PROGRESS_BAR.html)
pub fn add_bar(prefix: &str, length: u64, bar_type: BarType) -> ProgressBar {
    let pb = ProgressBar::new(length).with_prefix(prefix.to_string());

    update_style(&pb, bar_type);

    PROGRESS_BAR.add(pb)
}

/// Update the style of a progress bar based on the `BarType`
pub fn update_style(bar: &ProgressBar, bar_type: BarType) {
    let mut style = ProgressStyle::default_bar().progress_chars("#>-").with_key(
        "smoothed_per_sec",
        |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| match (
            state.pos(),
            state.elapsed().as_millis(),
        ) {
            // https://github.com/console-rs/indicatif/issues/394#issuecomment-1309971049
            //
            // indicatif released a change to how they reported eta/per_sec
            // and the results looked really weird based on how we use the progress
            // bars. this fixes that
            (pos, elapsed_ms) if elapsed_ms > 0 => {
                write!(w, "{:.0}/s", pos as f64 * 1000_f64 / elapsed_ms as f64).unwrap()
            }
            _ => write!(w, "-").unwrap(),
        },
    );

    style = match bar_type {
        BarType::Hidden => style.template("").unwrap(),
        BarType::Default => style
            .template("[{bar:.cyan/blue}] - {elapsed:<4} {pos:>7}/{len:7} {smoothed_per_sec:7} {prefix} {msg}")
            .unwrap(),
        BarType::Message => style
            .template(&format!(
            "[{{bar:.cyan/blue}}] - {{elapsed:<4}} {{pos:>7}}/{{len:7}} {:7} {{prefix}} {{msg}}",
            "-"
        ))
            .unwrap(),
        BarType::Total => style
            .template("[{bar:.yellow/blue}] - {elapsed:<4} {pos:>7}/{len:7} {eta:7} {msg}")
            .unwrap(),
        BarType::Quiet => style.template("Scanning: {prefix}").unwrap(),
    };

    bar.set_style(style);
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
