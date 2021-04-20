use crate::progress::PROGRESS_BAR;
use console::{measure_text_width, pad_str, style, Alignment, Term};
use indicatif::ProgressDrawTarget;

/// Interactive scan cancellation menu
#[derive(Debug)]
pub(super) struct Menu {
    /// character to use as visual separator of lines
    separator: String,

    /// name of menu
    name: String,

    /// header: name surrounded by separators
    header: String,

    /// instructions
    instructions: String,

    /// footer: instructions surrounded by separators
    footer: String,

    /// target for output
    term: Term,
}

/// Implementation of Menu
impl Menu {
    /// Creates new Menu
    pub(super) fn new() -> Self {
        let separator = "─".to_string();

        let instructions = format!(
            "Enter a {} list of indexes to {} (ex: 2,3)",
            style("comma-separated").yellow(),
            style("cancel").red(),
        );

        let name = format!(
            "{} {} {}",
            "💀",
            style("Scan Cancel Menu").bright().yellow(),
            "💀"
        );

        let longest = measure_text_width(&instructions).max(measure_text_width(&name));

        let border = separator.repeat(longest);

        let padded_name = pad_str(&name, longest, Alignment::Center, None);

        let header = format!("{}\n{}\n{}", border, padded_name, border);
        let footer = format!("{}\n{}\n{}", border, instructions, border);

        Self {
            separator,
            name,
            header,
            instructions,
            footer,
            term: Term::stderr(),
        }
    }

    /// print menu header
    pub(super) fn print_header(&self) {
        self.println(&self.header);
    }

    /// print menu footer
    pub(super) fn print_footer(&self) {
        self.println(&self.footer);
    }

    /// set PROGRESS_BAR bar target to hidden
    pub(super) fn hide_progress_bars(&self) {
        PROGRESS_BAR.set_draw_target(ProgressDrawTarget::hidden());
    }

    /// set PROGRESS_BAR bar target to hidden
    pub(super) fn show_progress_bars(&self) {
        PROGRESS_BAR.set_draw_target(ProgressDrawTarget::stdout());
    }

    /// Wrapper around console's Term::clear_screen and flush
    pub(super) fn clear_screen(&self) {
        self.term.clear_screen().unwrap_or_default();
        self.term.flush().unwrap_or_default();
    }

    /// Wrapper around console's Term::write_line
    pub(super) fn println(&self, msg: &str) {
        self.term.write_line(msg).unwrap_or_default();
    }

    /// Helper for parsing a usize from a str
    fn str_to_usize(&self, value: &str) -> usize {
        if value.is_empty() {
            return 0;
        }

        value
            .trim()
            .to_string()
            .parse::<usize>()
            .unwrap_or_else(|e| {
                self.println(&format!("Found non-numeric input: {}: {:?}", e, value));
                0
            })
    }

    /// split a comma delimited string into vec of usizes
    pub(super) fn split_to_nums(&self, line: &str) -> Vec<usize> {
        let mut nums = Vec::new();
        let values = line.split(',');

        for mut value in values {
            value = value.trim();

            if value.contains('-') {
                // range of two values, needs further processing

                let range: Vec<usize> = value
                    .split('-')
                    .map(|s| self.str_to_usize(s))
                    .filter(|m| *m != 0)
                    .collect();

                if range.len() != 2 {
                    // expecting [1, 4] or similar, if a 0 was used, we'd be left with a vec of size 1
                    self.println(&format!("Found invalid range of scans: {}", value));
                    continue;
                }

                (range[0]..=range[1]).for_each(|n| {
                    // iterate from lower to upper bound and add all interim values, skipping
                    // any already known
                    if !nums.contains(&n) {
                        nums.push(n)
                    }
                });
            } else {
                let value = self.str_to_usize(value);

                if value != 0 && !nums.contains(&value) {
                    // the zeroth scan is always skipped, skip already known values
                    nums.push(value);
                }
            }
        }

        nums
    }

    /// get comma-separated list of scan indexes from the user
    pub(super) fn get_scans_from_user(&self) -> Option<(Vec<usize>, bool)> {
        if let Ok(line) = self.term.read_line() {
            let force = line.contains("-f");
            let line = line.replace("-f", "");
            Some((self.split_to_nums(&line), force))
        } else {
            None
        }
    }

    /// Given a url, confirm with user that we should cancel
    pub(super) fn confirm_cancellation(&self, url: &str) -> char {
        self.println(&format!(
            "You sure you wanna cancel this scan: {}? [Y/n]",
            url
        ));

        self.term.read_char().unwrap_or('n')
    }
}

/// Default implementation for Menu
impl Default for Menu {
    /// return Menu::new as default
    fn default() -> Menu {
        Menu::new()
    }
}
