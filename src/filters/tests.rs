use super::*;
use ::fuzzyhash::FuzzyHash;
use ::regex::Regex;
use reqwest::Url;

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn lines_filter_as_any() {
    let filter = LinesFilter { line_count: 1 };

    assert_eq!(filter.line_count, 1);
    assert_eq!(
        *filter.as_any().downcast_ref::<LinesFilter>().unwrap(),
        filter
    );
}

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn words_filter_as_any() {
    let filter = WordsFilter { word_count: 1 };

    assert_eq!(filter.word_count, 1);
    assert_eq!(
        *filter.as_any().downcast_ref::<WordsFilter>().unwrap(),
        filter
    );
}

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn size_filter_as_any() {
    let filter = SizeFilter { content_length: 1 };

    assert_eq!(filter.content_length, 1);
    assert_eq!(
        *filter.as_any().downcast_ref::<SizeFilter>().unwrap(),
        filter
    );
}

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn status_code_filter_as_any() {
    let filter = StatusCodeFilter { filter_code: 200 };

    assert_eq!(filter.filter_code, 200);
    assert_eq!(
        *filter.as_any().downcast_ref::<StatusCodeFilter>().unwrap(),
        filter
    );
}

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn regex_filter_as_any() {
    let raw = r".*\.txt$";
    let compiled = Regex::new(raw).unwrap();
    let filter = RegexFilter {
        compiled,
        raw_string: raw.to_string(),
    };

    assert_eq!(filter.raw_string, r".*\.txt$");
    assert_eq!(
        *filter.as_any().downcast_ref::<RegexFilter>().unwrap(),
        filter
    );
}

#[test]
/// test should_filter on WilcardFilter where static logic matches
fn wildcard_should_filter_when_static_wildcard_found() {
    let resp = FeroxResponse {
        text: String::new(),
        wildcard: true,
        url: Url::parse("http://localhost").unwrap(),
        content_length: 100,
        word_count: 50,
        line_count: 25,
        headers: reqwest::header::HeaderMap::new(),
        status: reqwest::StatusCode::OK,
    };

    let filter = WildcardFilter {
        size: 100,
        dynamic: 0,
    };

    assert!(filter.should_filter_response(&resp));
}

#[test]
/// test should_filter on WilcardFilter where dynamic logic matches
fn wildcard_should_filter_when_dynamic_wildcard_found() {
    let resp = FeroxResponse {
        text: String::new(),
        wildcard: true,
        url: Url::parse("http://localhost/stuff").unwrap(),
        content_length: 100,
        word_count: 50,
        line_count: 25,
        headers: reqwest::header::HeaderMap::new(),
        status: reqwest::StatusCode::OK,
    };

    let filter = WildcardFilter {
        size: 0,
        dynamic: 95,
    };

    assert!(filter.should_filter_response(&resp));
}

#[test]
/// test should_filter on RegexFilter where regex matches body
fn regexfilter_should_filter_when_regex_matches_on_response_body() {
    let resp = FeroxResponse {
        text: String::from("im a body response hurr durr!"),
        wildcard: false,
        url: Url::parse("http://localhost/stuff").unwrap(),
        content_length: 100,
        word_count: 50,
        line_count: 25,
        headers: reqwest::header::HeaderMap::new(),
        status: reqwest::StatusCode::OK,
    };

    let raw = r"response...rr";

    let filter = RegexFilter {
        raw_string: raw.to_string(),
        compiled: Regex::new(raw).unwrap(),
    };

    assert!(filter.should_filter_response(&resp));
}

#[test]
/// a few simple tests for similarity filter
fn similarity_filter_is_accurate() {
    let mut resp = FeroxResponse {
        text: String::from("sitting"),
        wildcard: false,
        url: Url::parse("http://localhost/stuff").unwrap(),
        content_length: 100,
        word_count: 50,
        line_count: 25,
        headers: reqwest::header::HeaderMap::new(),
        status: reqwest::StatusCode::OK,
    };

    let mut filter = SimilarityFilter {
        text: FuzzyHash::new("kitten").to_string(),
        threshold: 95,
    };

    // kitten/sitting is 57% similar, so a threshold of 95 should not be filtered
    assert!(!filter.should_filter_response(&resp));

    resp.text = String::new();
    filter.text = String::new();
    filter.threshold = 100;

    // two empty strings are the same, however ssdeep doesn't accept empty strings, expect false
    assert!(!filter.should_filter_response(&resp));

    resp.text = String::from("some data to hash for the purposes of running a test");
    filter.text = FuzzyHash::new("some data to hash for the purposes of running a te").to_string();
    filter.threshold = 17;

    assert!(filter.should_filter_response(&resp));
}

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn similarity_filter_as_any() {
    let filter = SimilarityFilter {
        text: String::from("stuff"),
        threshold: 95,
    };

    assert_eq!(filter.text, "stuff");
    assert_eq!(
        *filter.as_any().downcast_ref::<SimilarityFilter>().unwrap(),
        filter
    );
}
