use super::builder::{LINKFINDER_REGEX, ROBOTS_TXT_REGEX, URL_CHARS_REGEX};
use super::container::request_link;
use super::*;
use crate::config::{Configuration, OutputLevel};
use crate::scan_manager::ScanOrder;
use crate::{
    event_handlers::Handles, scan_manager::FeroxScans, utils::make_request, Command, FeroxChannel,
    DEFAULT_METHOD,
};
use anyhow::Result;
use httpmock::{Method::GET, MockServer};
use lazy_static::lazy_static;
use reqwest::{Client, StatusCode, Url};
use std::collections::HashSet;
use tokio::sync::mpsc;

lazy_static! {
    /// Extractor for testing robots.txt
    static ref ROBOTS_EXT: Extractor<'static> = setup_extractor(ExtractionTarget::RobotsTxt, Arc::new(FeroxScans::default()));

    /// Extractor for testing response bodies
    static ref BODY_EXT: Extractor<'static> = setup_extractor(ExtractionTarget::ResponseBody, Arc::new(FeroxScans::default()));

    /// Extractor for testing paring html
    static ref PARSEHTML_EXT: Extractor<'static> = setup_extractor(ExtractionTarget::DirectoryListing, Arc::new(FeroxScans::default()));

    /// FeroxResponse for Extractor
    static ref RESPONSE: FeroxResponse = get_test_response();
}

/// constructor for the default FeroxResponse used during testing
fn get_test_response() -> FeroxResponse {
    let mut resp = FeroxResponse::default();
    resp.set_text("nulla pharetra diam sit amet nisl suscipit adipiscing bibendum est");
    resp
}

/// creates a single extractor that can be used to test standalone functions
fn setup_extractor(target: ExtractionTarget, scanned_urls: Arc<FeroxScans>) -> Extractor<'static> {
    let mut builder = ExtractorBuilder::default();

    let builder = match target {
        ExtractionTarget::ResponseBody => builder
            .target(ExtractionTarget::ResponseBody)
            .response(&RESPONSE),
        ExtractionTarget::RobotsTxt => builder
            .url("http://localhost")
            .target(ExtractionTarget::RobotsTxt),
        ExtractionTarget::DirectoryListing => builder
            .url("http://localhost")
            .target(ExtractionTarget::DirectoryListing),
    };

    let config = Arc::new(Configuration::new().unwrap());
    let handles = Arc::new(Handles::for_testing(Some(scanned_urls), Some(config)).0);

    builder.handles(handles).build().unwrap()
}

#[test]
/// extract sub paths from the given url fragment; expect 4 sub paths and that all are
/// in the expected array
fn extractor_get_sub_paths_from_path_with_multiple_paths() {
    let path = "homepage/assets/img/icons/handshake.svg";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(path);
    let expected = vec![
        "homepage/",
        "homepage/assets/",
        "homepage/assets/img/",
        "homepage/assets/img/icons/",
        "homepage/assets/img/icons/handshake.svg",
    ];

    assert_eq!(r_paths.len(), expected.len());
    assert_eq!(b_paths.len(), expected.len());
    for expected_path in expected {
        assert!(r_paths.contains(&expected_path.to_string()));
        assert!(b_paths.contains(&expected_path.to_string()));
    }
}

#[test]
/// extract sub paths from the given url fragment; expect 2 sub paths and that all are
/// in the expected array. the fragment is wrapped in slashes to ensure no empty strings are
/// returned
fn extractor_get_sub_paths_from_path_with_enclosing_slashes() {
    let path = "/homepage/assets/";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(path);
    let expected = vec!["homepage/", "homepage/assets"];

    assert_eq!(r_paths.len(), expected.len());
    assert_eq!(b_paths.len(), expected.len());
    for expected_path in expected {
        assert!(r_paths.contains(&expected_path.to_string()));
        assert!(b_paths.contains(&expected_path.to_string()));
    }
}

#[test]
/// extract sub paths from the given url fragment; expect 1 sub path, no forward slashes are
/// included
fn extractor_get_sub_paths_from_path_with_only_a_word() {
    let path = "homepage";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(path);
    let expected = vec!["homepage"];

    assert_eq!(r_paths.len(), expected.len());
    assert_eq!(b_paths.len(), expected.len());
    for expected_path in expected {
        assert!(r_paths.contains(&expected_path.to_string()));
        assert!(b_paths.contains(&expected_path.to_string()));
    }
}

#[test]
/// extract sub paths from the given url fragment; expect 1 sub path, forward slash removed
fn extractor_get_sub_paths_from_path_with_an_absolute_word() {
    let path = "/homepage";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(path);
    let expected = vec!["homepage"];

    assert_eq!(r_paths.len(), expected.len());
    assert_eq!(b_paths.len(), expected.len());
    for expected_path in expected {
        assert!(r_paths.contains(&expected_path.to_string()));
        assert!(b_paths.contains(&expected_path.to_string()));
    }
}

#[test]
/// test that an ExtractorBuilder without a FeroxResponse and without a URL bails
fn extractor_builder_bails_when_neither_required_field_is_set() {
    let handles = Arc::new(Handles::for_testing(None, None).0);

    let extractor = ExtractorBuilder::default()
        .url("")
        .target(ExtractionTarget::RobotsTxt)
        .handles(handles)
        .build();

    assert!(extractor.is_err());
}

#[test]
/// Extractor with a non-base url bails
fn extractor_with_non_base_url_bails() -> Result<()> {
    let mut links = HashSet::<String>::new();
    let link = "admin";
    let handles = Arc::new(Handles::for_testing(None, None).0);

    let extractor = ExtractorBuilder::default()
        .url("\\\\\\")
        .handles(handles)
        .target(ExtractionTarget::RobotsTxt)
        .build()?;

    let result = extractor.add_link_to_set_of_links(link, &mut links);

    assert!(result.is_err());
    Ok(())
}

#[test]
/// test that a full url and fragment are joined correctly, then added to the given list
/// i.e. the happy path
fn extractor_add_link_to_set_of_links_happy_path() {
    let mut r_links = HashSet::<String>::new();
    let r_link = "admin";
    let mut b_links = HashSet::<String>::new();
    let b_link = "shmadmin";

    assert_eq!(r_links.len(), 0);
    ROBOTS_EXT
        .add_link_to_set_of_links(r_link, &mut r_links)
        .unwrap();

    assert_eq!(r_links.len(), 1);
    assert!(r_links.contains("http://localhost/admin"));

    assert_eq!(b_links.len(), 0);

    BODY_EXT
        .add_link_to_set_of_links(b_link, &mut b_links)
        .unwrap();

    assert_eq!(b_links.len(), 1);
    assert!(b_links.contains("http://localhost/shmadmin"));
}

#[test]
/// test that an invalid path fragment doesn't add anything to the set of links
fn extractor_add_link_to_set_of_links_with_non_base_url() {
    let mut links = HashSet::<String>::new();
    let link = "\\\\\\\\";
    assert_eq!(links.len(), 0);
    assert!(ROBOTS_EXT
        .add_link_to_set_of_links(link, &mut links)
        .is_err());
    assert!(BODY_EXT.add_link_to_set_of_links(link, &mut links).is_err());

    assert_eq!(links.len(), 0);
    assert!(links.is_empty());
}

#[test]
/// test for filtering queries and fragments
fn normalize_url_path_filters_queries_and_fragments() {
    let handles = Arc::new(Handles::for_testing(None, None).0);
    let extractor = ExtractorBuilder::default()
        .url("doesnt matter")
        .target(ExtractionTarget::RobotsTxt)
        .handles(handles)
        .build()
        .unwrap();

    let test_strings = [
        "over/there?name=ferret#nose",
        "over/there?name=ferret",
        "over/there#nose",
        "over/there",
        "over/there?name#nose",
        "over/there?name",
        "   over/there?name=ferret#nose  ",
        "over/there?name=ferret   ",
        "   over/there#nose",
    ];
    test_strings.iter().for_each(|&ts| {
        let normed = extractor.normalize_url_path(ts);
        assert_eq!(normed, "over/there");
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// use make_request to generate a Response, and use the Response to test get_links;
/// the response will contain an absolute path to a domain that is not part of the scanned
/// domain; expect an empty set returned
async fn extractor_get_links_with_absolute_url_that_differs_from_target_domain() -> Result<()> {
    let (tx_stats, _): FeroxChannel<Command> = mpsc::unbounded_channel();

    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/some-path");
        then.status(200).body(
            "\"http://definitely.not.a.thing.probably.com/homepage/assets/img/icons/handshake.svg\"",
        );
    });

    let client = Client::new();
    let url = Url::parse(&srv.url("/some-path")).unwrap();
    let config = Configuration::new().unwrap();

    let response = make_request(
        &client,
        &url,
        DEFAULT_METHOD,
        None,
        OutputLevel::Default,
        &config,
        tx_stats.clone(),
    )
    .await
    .unwrap();
    let (handles, _rx) = Handles::for_testing(None, None);

    let handles = Arc::new(handles);
    let ferox_response =
        FeroxResponse::from(response, &srv.url(""), DEFAULT_METHOD, OutputLevel::Default).await;

    let extractor = Extractor {
        links_regex: Regex::new(LINKFINDER_REGEX).unwrap(),
        robots_regex: Regex::new(ROBOTS_TXT_REGEX).unwrap(),
        url_regex: Regex::new(URL_CHARS_REGEX).unwrap(),
        response: Some(&ferox_response),
        url: String::new(),
        target: ExtractionTarget::ResponseBody,
        handles: handles.clone(),
    };

    let links = extractor.extract_from_body().await?;

    assert!(links.is_empty());
    assert_eq!(mock.hits(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test that /robots.txt is correctly requested given a base url (happy path)
async fn request_robots_txt_without_proxy() -> Result<()> {
    let handles = Arc::new(Handles::for_testing(None, None).0);

    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200).body("this is a test");
    });

    let extractor = Extractor {
        links_regex: Regex::new(LINKFINDER_REGEX).unwrap(),
        robots_regex: Regex::new(ROBOTS_TXT_REGEX).unwrap(),
        url_regex: Regex::new(URL_CHARS_REGEX).unwrap(),
        response: None,
        url: srv.url("/api/users/stuff/things"),
        target: ExtractionTarget::RobotsTxt,
        handles,
    };

    let resp = extractor.make_extract_request("/robots.txt").await?;

    assert!(matches!(resp.status(), &StatusCode::OK));
    println!("{resp}");
    assert_eq!(resp.content_length(), 14);
    assert_eq!(mock.hits(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test that /robots.txt is correctly requested given a base url (happy path) when a proxy is used
async fn request_robots_txt_with_proxy() -> Result<()> {
    let handles = Arc::new(Handles::for_testing(None, None).0);
    let mut config = Configuration::new()?;

    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200).body("this is also a test");
    });

    // note: the proxy doesn't actually do anything other than hit a different code branch
    // in this unit test; it would however have an effect on an integration test
    config.proxy = srv.url("/ima-proxy");
    config.no_recursion = true;

    let extractor = ExtractorBuilder::default()
        .url(&srv.url("/api/different/path"))
        .target(ExtractionTarget::RobotsTxt)
        .handles(handles)
        .build()?;

    let resp = extractor.make_extract_request("/robots.txt").await?;

    assert!(matches!(resp.status(), &StatusCode::OK));
    assert_eq!(resp.content_length(), 19);
    assert_eq!(mock.hits(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// request_link's happy path, expect back a FeroxResponse
async fn request_link_happy_path() -> Result<()> {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/login.php");
        then.status(200).body("this is a test");
    });

    let r_resp = request_link(&srv.url("/login.php"), ROBOTS_EXT.handles.clone()).await?;
    let b_resp = request_link(&srv.url("/login.php"), BODY_EXT.handles.clone()).await?;

    assert!(matches!(r_resp.status(), StatusCode::OK));
    assert!(matches!(b_resp.status(), StatusCode::OK));
    assert_eq!(r_resp.content_length().unwrap(), 14);
    assert_eq!(b_resp.content_length().unwrap(), 14);
    assert_eq!(mock.hits(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// request_link should bail in the event that the url is already in scanned_urls
async fn request_link_bails_on_seen_url() -> Result<()> {
    let url = "/unique-for-this-test.php";
    let srv = MockServer::start();
    let served = srv.url(url);

    let mock = srv.mock(|when, then| {
        when.method(GET).path(url);
        then.status(200)
            .body("this is a unique test, don't reuse the endpoint");
    });

    let scans = Arc::new(FeroxScans::default());
    scans.add_file_scan(
        &served,
        ScanOrder::Latest,
        Arc::new(Handles::for_testing(None, None).0),
    );

    let robots = setup_extractor(ExtractionTarget::RobotsTxt, scans.clone());
    let body = setup_extractor(ExtractionTarget::ResponseBody, scans);

    let r_resp = request_link(&served, robots.handles.clone()).await;
    let b_resp = request_link(&served, body.handles.clone()).await;

    assert!(r_resp.is_err());
    assert!(b_resp.is_err());
    assert_eq!(mock.hits(), 0); // function exits before requests can happen
    Ok(())
}
