use super::{
    LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter, WordsFilter,
};
use crate::{
    event_handlers::Handles,
    response::FeroxResponse,
    skip_fail,
    utils::{fmt_err, logged_request},
    Command::AddFilter,
    DEFAULT_METHOD, SIMILARITY_THRESHOLD,
};
use anyhow::Result;
use fuzzyhash::FuzzyHash;
use regex::Regex;
use reqwest::Url;
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
        // url as-is based on input, ignores user-specified url manipulation options (add-slash etc)
        let url = skip_fail!(Url::parse(similarity_filter));

        // attempt to request the given url
        let resp = skip_fail!(logged_request(&url, DEFAULT_METHOD, None, handles.clone()).await);

        // if successful, create a filter based on the response's body
        let mut fr = FeroxResponse::from(
            resp,
            similarity_filter,
            DEFAULT_METHOD,
            handles.config.output_level,
        )
        .await;

        if handles.config.collect_extensions {
            fr.parse_extension(handles.clone())?;
        }

        // hash the response body and store the resulting hash in the filter object
        let hash = FuzzyHash::new(&fr.text()).to_string();

        let filter = SimilarityFilter {
            text: hash,
            threshold: SIMILARITY_THRESHOLD,
        };

        let boxed_filter = Box::new(filter);
        skip_fail!(handles.filters.send(AddFilter(boxed_filter)));
    }

    handles.filters.sync().await?;
    Ok(())
}
