use super::{
    utils::create_similarity_filter, LinesFilter, RegexFilter, SizeFilter, StatusCodeFilter,
    WordsFilter,
};
use crate::{event_handlers::Handles, skip_fail, utils::fmt_err, Command::AddFilter};
use anyhow::Result;
use regex::Regex;
use std::sync::Arc;

/// add all user-supplied filters to the (already started) filters handler
pub async fn initialize(handles: Arc<Handles>) -> Result<()> {
    // add any status code filters to filters handler's FeroxFilters  (-C|--filter-status)
    for code_filter in &handles.config.filter_status {
        let filter = StatusCodeFilter {
            filter_code: *code_filter,
        };
        let boxed_filter = Box::new(filter);
        skip_fail!(handles.filters.send(AddFilter(boxed_filter)));
    }

    // add any line count filters to filters handler's FeroxFilters  (-N|--filter-lines)
    for lines_filter in &handles.config.filter_line_count {
        let filter = LinesFilter {
            line_count: *lines_filter,
        };
        let boxed_filter = Box::new(filter);
        skip_fail!(handles.filters.send(AddFilter(boxed_filter)));
    }

    // add any line count filters to filters handler's FeroxFilters  (-W|--filter-words)
    for words_filter in &handles.config.filter_word_count {
        let filter = WordsFilter {
            word_count: *words_filter,
        };
        let boxed_filter = Box::new(filter);
        skip_fail!(handles.filters.send(AddFilter(boxed_filter)));
    }

    // add any line count filters to filters handler's FeroxFilters  (-S|--filter-size)
    for size_filter in &handles.config.filter_size {
        let filter = SizeFilter {
            content_length: *size_filter,
        };
        let boxed_filter = Box::new(filter);
        skip_fail!(handles.filters.send(AddFilter(boxed_filter)));
    }

    // add any regex filters to filters handler's FeroxFilters  (-X|--filter-regex)
    for regex_filter in &handles.config.filter_regex {
        let raw = regex_filter;
        let compiled = skip_fail!(Regex::new(raw));

        let filter = RegexFilter {
            raw_string: raw.to_owned(),
            compiled,
        };
        let boxed_filter = Box::new(filter);
        skip_fail!(handles.filters.send(AddFilter(boxed_filter)));
    }

    // add any similarity filters to filters handler's FeroxFilters  (--filter-similar-to)
    for similarity_filter in &handles.config.filter_similar {
        let filter = skip_fail!(create_similarity_filter(similarity_filter, handles.clone()).await);

        let boxed_filter = Box::new(filter);
        skip_fail!(handles.filters.send(AddFilter(boxed_filter)));
    }

    handles.filters.sync().await?;
    Ok(())
}
