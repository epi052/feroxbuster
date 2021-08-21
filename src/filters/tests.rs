use super::*;
use ::fuzzyhash::FuzzyHash;
use ::regex::Regex;

#[test]
/// simply test the default values for wildcardfilter, expect 0, 0
fn wildcard_filter_default() {
    let wcf = WildcardFilter::default();
    assert_eq!(wcf.size, u64::MAX);
    assert_eq!(wcf.dynamic, u64::MAX);
}

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn wildcard_filter_as_any() {
    let filter = WildcardFilter::default();
    let filter2 = WildcardFilter::default();

    assert!(filter.box_eq(filter2.as_any()));

    assert_eq!(
        *filter.as_any().downcast_ref::<WildcardFilter>().unwrap(),
        filter
    );
}

#[test]
/// just a simple test to increase code coverage by hitting as_any and the inner value
fn lines_filter_as_any() {
    let filter = LinesFilter { line_count: 1 };
    let filter2 = LinesFilter { line_count: 1 };

    assert!(filter.box_eq(filter2.as_any()));

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
    let filter2 = WordsFilter { word_count: 1 };

    assert!(filter.box_eq(filter2.as_any()));

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
    let filter2 = SizeFilter { content_length: 1 };

    assert!(filter.box_eq(filter2.as_any()));

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
    let filter2 = StatusCodeFilter { filter_code: 200 };

    assert!(filter.box_eq(filter2.as_any()));

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
    let compiled2 = Regex::new(raw).unwrap();
    let filter = RegexFilter {
        compiled,
        raw_string: raw.to_string(),
    };
    let filter2 = RegexFilter {
        compiled: compiled2,
        raw_string: raw.to_string(),
    };

    assert!(filter.box_eq(filter2.as_any()));

    assert_eq!(filter.raw_string, r".*\.txt$");
    assert_eq!(
        *filter.as_any().downcast_ref::<RegexFilter>().unwrap(),
        filter
    );
}

#[test]
/// test should_filter on WilcardFilter where static logic matches
fn wildcard_should_filter_when_static_wildcard_found() {
    let mut resp = FeroxResponse::default();
    resp.set_wildcard(true);
    resp.set_url("http://localhost");
    resp.set_text(
        "pellentesque diam volutpat commodo sed egestas egestas fringilla phasellus faucibus",
    );

    let filter = WildcardFilter {
        size: 83,
        dynamic: 0,
        dont_filter: false,
    };

    assert!(filter.should_filter_response(&resp));
}

#[test]
/// test should_filter on WilcardFilter where static logic matches but response length is 0
fn wildcard_should_filter_when_static_wildcard_len_is_zero() {
    let mut resp = FeroxResponse::default();
    resp.set_wildcard(true);
    resp.set_url("http://localhost");

    // default WildcardFilter is used in the code that executes when response.content_length() == 0
    let filter = WildcardFilter::new(false);

    assert!(filter.should_filter_response(&resp));
}

#[test]
/// test should_filter on WilcardFilter where dynamic logic matches
fn wildcard_should_filter_when_dynamic_wildcard_found() {
    let mut resp = FeroxResponse::default();
    resp.set_wildcard(true);
    resp.set_url("http://localhost/stuff");
    resp.set_text("pellentesque diam volutpat commodo sed egestas egestas fringilla");

    let filter = WildcardFilter {
        size: 0,
        dynamic: 59, // content-length - 5 (len('stuff'))
        dont_filter: false,
    };

    println!("resp: {:?}: filter: {:?}", resp, filter);

    assert!(filter.should_filter_response(&resp));
}

#[test]
/// test should_filter on RegexFilter where regex matches body
fn regexfilter_should_filter_when_regex_matches_on_response_body() {
    let mut resp = FeroxResponse::default();
    resp.set_url("http://localhost/stuff");
    resp.set_text("im a body response hurr durr!");

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
    let mut resp = FeroxResponse::default();
    resp.set_url("http://localhost/stuff");
    resp.set_text("sitting");

    let mut filter = SimilarityFilter {
        text: FuzzyHash::new("kitten").to_string(),
        threshold: 95,
    };

    // kitten/sitting is 57% similar, so a threshold of 95 should not be filtered
    assert!(!filter.should_filter_response(&resp));

    resp.set_text("");
    filter.text = String::new();
    filter.threshold = 100;

    // two empty strings are the same, however ssdeep doesn't accept empty strings, expect false
    assert!(!filter.should_filter_response(&resp));

    resp.set_text("some data to hash for the purposes of running a test");
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

    let filter2 = SimilarityFilter {
        text: String::from("stuff"),
        threshold: 95,
    };

    assert!(filter.box_eq(filter2.as_any()));

    assert_eq!(filter.text, "stuff");
    assert_eq!(
        *filter.as_any().downcast_ref::<SimilarityFilter>().unwrap(),
        filter
    );
}
