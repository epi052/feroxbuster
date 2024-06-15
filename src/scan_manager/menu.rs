use std::time::Duration;

use crate::filters::filter_lookup;
use crate::progress::PROGRESS_BAR;
use crate::traits::FeroxFilter;
use console::{measure_text_width, pad_str, style, Alignment, Term};
use indicatif::{HumanDuration, ProgressDrawTarget};
use regex::Regex;

/// Data container for a command entered by the user interactively
#[derive(Debug)]
pub enum MenuCmd {
    /// user wants to add a url to be scanned
    AddUrl(String),

    /// user wants to cancel one or more active scans
    Cancel(Vec<usize>, bool),

    /// user wants to create a new filter
    AddFilter(Box<dyn FeroxFilter>),

    /// user wants to remove one or more active filters
    RemoveFilter(Vec<usize>),
}

/// Data container for a command result to be used internally by the ferox_scanner
#[derive(Debug)]
pub enum MenuCmdResult {
    /// Url to be added to the scan queue
    Url(String),

    /// Number of scans that were actually cancelled, can be 0
    NumCancelled(usize),

    /// Filter to be added to current list of `FeroxFilters`
    Filter(Box<dyn FeroxFilter>),
}

/// Interactive scan cancellation menu
#[derive(Debug)]
pub(super) struct Menu {
    /// header: name surrounded by separators
    header: String,

    /// footer: instructions surrounded by separators
    footer: String,

    /// length of longest displayed line (suitable for ascii/unicode)
    longest: usize,

    /// unicode line border, matched to longest displayed line
    border: String,

    /// target for output
    pub(super) term: Term,
}

/// Implementation of Menu
impl Menu {
    /// Creates new Menu
    pub(super) fn new() -> Self {
        let separator = "─".to_string();

        let name = format!(
            "{} {} {}",
            "💀",
            style("Scan Management Menu").bright().yellow(),
            "💀"
        );

        let add_cmd = format!(
            "  {}[{}] NEW_URL (ex: {} http://localhost)\n",
            style("a").green(),
            style("dd").green(),
            style("add").green()
        );

        let canx_cmd = format!(
            "  {}[{}] [-f] SCAN_ID[-SCAN_ID[,...]] (ex: {} 1-4,8,9-13 or {} -f 3)\n",
            style("c").red(),
            style("ancel").red(),
            style("cancel").red(),
            style("c").red(),
        );

        let new_filter_cmd = format!(
            "  {}[{}] FILTER_TYPE FILTER_VALUE (ex: {} lines 40)\n",
            style("n").green(),
            style("ew-filter").green(),
            style("n").green(),
        );

        let valid_filters = format!(
            "    FILTER_TYPEs: {}, {}, {}, {}, {}, {}\n",
            style("status").yellow(),
            style("lines").yellow(),
            style("size").yellow(),
            style("words").yellow(),
            style("regex").yellow(),
            style("similarity").yellow()
        );

        let rm_filter_cmd = format!(
            "  {}[{}] FILTER_ID[-FILTER_ID[,...]] (ex: {} 1-4,8,9-13 or {} 3)",
            style("r").red(),
            style("m-filter").red(),
            style("rm-filter").red(),
            style("r").red(),
        );

        let mut commands = format!("{}:\n", style("Commands").bright().blue());
        commands.push_str(&add_cmd);
        commands.push_str(&canx_cmd);
        commands.push_str(&new_filter_cmd);
        commands.push_str(&valid_filters);
        commands.push_str(&rm_filter_cmd);

        let longest = measure_text_width(&canx_cmd).max(measure_text_width(&name)) + 1;

        let border = separator.repeat(longest);

        let padded_name = pad_str(&name, longest, Alignment::Center, None);

        let header = format!("{border}\n{padded_name}\n{border}");
        let footer = format!("{commands}\n{border}");

        Self {
            header,
            footer,
            border,
            longest,
            term: Term::stderr(),
        }
    }

    /// print menu header
    pub(super) fn print_header(&self) {
        self.println(&self.header);
    }

    /// print menu unicode border line
    pub(super) fn print_border(&self) {
        self.println(&self.border);
    }

    /// print menu footer
    pub(super) fn print_footer(&self) {
        self.println(&self.footer);
    }

    /// print menu footer
    pub(super) fn print_eta(&self, eta: Duration) {
        let inner = format!("⏳ {} remaining ⏳", HumanDuration(eta));
        let padded_eta = pad_str(&inner, self.longest, Alignment::Center, None);
        self.println(&format!("{padded_eta}\n{}", self.border));
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
                self.println(&format!("Found non-numeric input: {e}: {value:?}"));
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
                    self.println(&format!("Found invalid range of scans: {value}"));
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
                if value.is_empty() {
                    continue;
                }

                let value = self.str_to_usize(value);

                if !nums.contains(&value) {
                    // skip already known values
                    nums.push(value);
                }
            }
        }

        nums
    }

    /// get input from the user and translate it to a `MenuCmd`
    pub(super) fn get_command_input_from_user(&self, line: &str) -> Option<MenuCmd> {
        let line = line.trim(); // normalize input if there are leading spaces

        match line.chars().next().unwrap_or('_').to_ascii_lowercase() {
            'c' => {
                // cancel command; start by determining if -f was used
                let force = line.contains("-f");

                // then remove c[ancel] from the command so it can be passed to the number
                // splitter
                let re = Regex::new(r"^[cC][ancelANCEL]*").unwrap();
                let line = line.replace("-f", "");
                let line = re.replace(&line, "").to_string();

                Some(MenuCmd::Cancel(self.split_to_nums(&line), force))
            }
            'a' => {
                // add command
                // similar to cancel, we need to remove the a[dd] substring, the rest should be
                // a url
                let re = Regex::new(r"^[aA][dD]*").unwrap();
                let line = re.replace(line, "").to_string().trim().to_string();

                Some(MenuCmd::AddUrl(line))
            }
            'n' => {
                // new filter command
                let mut line = line.split_whitespace();
                line.next(); // 'n' or 'new-filter'

                if let Some(filter_type) = line.next() {
                    // have a string in the filter_type position
                    if let Some(filter_value) = line.next() {
                        // have a string in the filter_value position
                        if let Some(result) = filter_lookup(filter_type, filter_value) {
                            // lookup was successful, return the new filter
                            return Some(MenuCmd::AddFilter(result));
                        }
                    }
                }
                None
            }
            'r' => {
                // remove filter command

                // remove r[m-filter] from the command so it can be passed to the number
                // splitter
                let re = Regex::new(r"^[rR][mfilterMFILTER-]*").unwrap();
                // we don't respect a -f or lack thereof in this command, but in case the user
                // doesn't realize / thinks its the same as cancel -f, just remove it
                let line = line.replace("-f", "");
                let line = re.replace(&line, "").to_string();

                let indices = self.split_to_nums(&line);

                Some(MenuCmd::RemoveFilter(indices))
            }
            _ => {
                // invalid input
                None
            }
        }
    }

    /// Given a url, confirm with user that we should cancel
    pub(super) fn confirm_cancellation(&self, url: &str) -> char {
        self.println(&format!(
            "You sure you wanna cancel this scan: {url}? [Y/n]"
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
