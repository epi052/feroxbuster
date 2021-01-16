use super::*;
use crate::utils::make_request;
use crate::FeroxChannel;
use anyhow::Result;
use httpmock::Method::GET;
use httpmock::MockServer;
use lazy_static::lazy_static;
use reqwest::{header::HeaderMap, Client, StatusCode, Url};
use std::collections::HashSet;
use tokio::sync::mpsc;

lazy_static! {
    /// Extractor for testing robots.txt
    static ref ROBOTS_EXT: Extractor<'static> = setup_extractor(ExtractionTarget::RobotsTxt);

    /// Extractor for testing response bodies
    static ref BODY_EXT: Extractor<'static> = setup_extractor(ExtractionTarget::ResponseBody);

    /// Configuration for Extractor
    static ref CONFIG: Configuration = Configuration::new();

    /// FeroxScans for Extractor
    static ref SCANS: FeroxScans = FeroxScans::default();

    /// FeroxResponse for Extractor
    static ref RESPONSE: FeroxResponse = get_test_response();
}

fn get_test_response() -> FeroxResponse {
    FeroxResponse {
        text: String::new(),
        wildcard: true,
        url: Url::parse("https://localhost").unwrap(),
        content_length: 125,
        word_count: 10,
        line_count: 14,
        headers: HeaderMap::new(),
        status: StatusCode::OK,
    }
}

/// creates a single extractor that can be used to test standalone functions
fn setup_extractor(target: ExtractionTarget) -> Extractor<'static> {
    let (tx_dir, _): FeroxChannel<String> = mpsc::unbounded_channel();
    let (tx_stats, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
    let (tx_term, _): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();
    let stats = Arc::new(Stats::new());

    let mut builder = match target {
        ExtractionTarget::ResponseBody => ExtractorBuilder::with_response(&RESPONSE),
        ExtractionTarget::RobotsTxt => ExtractorBuilder::with_url("https://localhost"),
    };

    builder
        .target(target)
        .depth(4)
        .config(&CONFIG)
        .recursion_transmitter(tx_dir.clone())
        .stats_transmitter(tx_stats.clone())
        .reporter_transmitter(tx_term.clone())
        .scanned_urls(&SCANS)
        .stats(stats.clone())
        .build()
        .unwrap()
}

#[test]
/// extract sub paths from the given url fragment; expect 4 sub paths and that all are
/// in the expected array
fn extractor_get_sub_paths_from_path_with_multiple_paths() {
    let path = "homepage/assets/img/icons/handshake.svg";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(&path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(&path);
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
        assert_eq!(r_paths.contains(&expected_path.to_string()), true);
        assert_eq!(b_paths.contains(&expected_path.to_string()), true);
    }
}

#[test]
/// extract sub paths from the given url fragment; expect 2 sub paths and that all are
/// in the expected array. the fragment is wrapped in slashes to ensure no empty strings are
/// returned
fn extractor_get_sub_paths_from_path_with_enclosing_slashes() {
    let path = "/homepage/assets/";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(&path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(&path);
    let expected = vec!["homepage/", "homepage/assets"];

    assert_eq!(r_paths.len(), expected.len());
    assert_eq!(b_paths.len(), expected.len());
    for expected_path in expected {
        assert_eq!(r_paths.contains(&expected_path.to_string()), true);
        assert_eq!(b_paths.contains(&expected_path.to_string()), true);
    }
}

#[test]
/// extract sub paths from the given url fragment; expect 1 sub path, no forward slashes are
/// included
fn extractor_get_sub_paths_from_path_with_only_a_word() {
    let path = "homepage";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(&path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(&path);
    let expected = vec!["homepage"];

    assert_eq!(r_paths.len(), expected.len());
    assert_eq!(b_paths.len(), expected.len());
    for expected_path in expected {
        assert_eq!(r_paths.contains(&expected_path.to_string()), true);
        assert_eq!(b_paths.contains(&expected_path.to_string()), true);
    }
}

#[test]
/// extract sub paths from the given url fragment; expect 1 sub path, forward slash removed
fn extractor_get_sub_paths_from_path_with_an_absolute_word() {
    let path = "/homepage";
    let r_paths = ROBOTS_EXT.get_sub_paths_from_path(&path);
    let b_paths = BODY_EXT.get_sub_paths_from_path(&path);
    let expected = vec!["homepage"];

    assert_eq!(r_paths.len(), expected.len());
    assert_eq!(b_paths.len(), expected.len());
    for expected_path in expected {
        assert_eq!(r_paths.contains(&expected_path.to_string()), true);
        assert_eq!(b_paths.contains(&expected_path.to_string()), true);
    }
}
#[test]
/// test that an ExtractorBuilder without a FeroxResponse and without a URL bails
fn extractor_builder_bails_when_neither_required_field_is_set() {
    let (tx_dir, _): FeroxChannel<String> = mpsc::unbounded_channel();
    let (tx_stats, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
    let (tx_term, _): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();
    let stats = Arc::new(Stats::new());

    let extractor = ExtractorBuilder::with_url("")
        .target(ExtractionTarget::ResponseBody)
        .depth(4)
        .config(&CONFIG)
        .recursion_transmitter(tx_dir.clone())
        .stats_transmitter(tx_stats.clone())
        .reporter_transmitter(tx_term.clone())
        .scanned_urls(&SCANS)
        .stats(stats.clone())
        .build();

    assert!(extractor.is_err());
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
    assert!(r_links.contains("https://localhost/admin"));

    assert_eq!(b_links.len(), 0);

    BODY_EXT
        .add_link_to_set_of_links(b_link, &mut b_links)
        .unwrap();

    assert_eq!(b_links.len(), 1);
    assert!(b_links.contains("https://localhost/shmadmin"));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// use make_request to generate a Response, and use the Response to test get_links;
/// the response will contain an absolute path to a domain that is not part of the scanned
/// domain; expect an empty set returned
async fn extractor_get_links_with_absolute_url_that_differs_from_target_domain() -> Result<()> {
    let (tx_dir, _): FeroxChannel<String> = mpsc::unbounded_channel();
    let (tx_stats, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
    let (tx_term, _): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();
    let stats = Arc::new(Stats::new());

    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/some-path");
        then.status(200).body(
            "\"http://defintely.not.a.thing.probably.com/homepage/assets/img/icons/handshake.svg\"",
        );
    });

    let client = Client::new();
    let url = Url::parse(&srv.url("/some-path")).unwrap();

    let response = make_request(&client, &url, tx_stats.clone()).await.unwrap();

    let ferox_response = FeroxResponse::from(response, true).await;

    let extractor = ExtractorBuilder::with_response(&ferox_response)
        .target(ExtractionTarget::ResponseBody)
        .depth(4)
        .config(&CONFIG)
        .recursion_transmitter(tx_dir.clone())
        .stats_transmitter(tx_stats.clone())
        .reporter_transmitter(tx_term.clone())
        .scanned_urls(&SCANS)
        .stats(stats.clone())
        .build()?;

    let links = extractor.extract_from_body().await?;

    assert!(links.is_empty());

    assert_eq!(mock.hits(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test that /robots.txt is correctly requested given a base url (happy path)
async fn request_robots_txt_without_proxy() -> Result<()> {
    let (tx_dir, _): FeroxChannel<String> = mpsc::unbounded_channel();
    let (tx_stats, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
    let (tx_term, _): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();
    let stats = Arc::new(Stats::new());
    let config = Configuration::new();

    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200).body("this is a test");
    });

    let extractor = ExtractorBuilder::with_url(&srv.url("/api/users/stuff/things"))
        .target(ExtractionTarget::RobotsTxt)
        .depth(4)
        .config(&config)
        .recursion_transmitter(tx_dir.clone())
        .stats_transmitter(tx_stats.clone())
        .reporter_transmitter(tx_term.clone())
        .scanned_urls(&SCANS)
        .stats(stats.clone())
        .build()?;

    let resp = extractor.request_robots_txt().await?;

    assert!(matches!(resp.status(), &StatusCode::OK));
    println!("{}", resp);
    assert_eq!(resp.content_length(), 14);
    assert_eq!(mock.hits(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test that /robots.txt is correctly requested given a base url (happy path) when a proxy is used
async fn request_robots_txt_with_proxy() -> Result<()> {
    let (tx_dir, _): FeroxChannel<String> = mpsc::unbounded_channel();
    let (tx_stats, _): FeroxChannel<StatCommand> = mpsc::unbounded_channel();
    let (tx_term, _): FeroxChannel<FeroxResponse> = mpsc::unbounded_channel();
    let stats = Arc::new(Stats::new());
    let mut config = Configuration::new();

    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200).body("this is also a test");
    });

    // note: the proxy doesn't actually do anything other than hit a different code branch
    // in this unit test; it would however have an effect on an integration test
    config.proxy = srv.url("/ima-proxy");

    let extractor = ExtractorBuilder::with_url(&srv.url("/api/different/path"))
        .target(ExtractionTarget::RobotsTxt)
        .depth(4)
        .config(&config)
        .recursion_transmitter(tx_dir.clone())
        .stats_transmitter(tx_stats.clone())
        .reporter_transmitter(tx_term.clone())
        .scanned_urls(&SCANS)
        .stats(stats.clone())
        .build()?;

    let resp = extractor.request_robots_txt().await?;

    assert!(matches!(resp.status(), &StatusCode::OK));
    assert_eq!(resp.content_length(), 19);
    assert_eq!(mock.hits(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// get_feroxresponse_from_link's happy path, expect back a FeroxResponse
async fn get_feroxresponse_from_link_happy_path() -> Result<()> {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/login.php");
        then.status(200).body("this is a test");
    });

    let r_resp = ROBOTS_EXT
        .get_feroxresponse_from_link(&srv.url("/login.php"))
        .await?;
    let b_resp = BODY_EXT
        .get_feroxresponse_from_link(&srv.url("/login.php"))
        .await?;

    assert!(matches!(r_resp.status(), &StatusCode::OK));
    assert!(matches!(b_resp.status(), &StatusCode::OK));
    assert_eq!(r_resp.content_length(), 14);
    assert_eq!(b_resp.content_length(), 14);
    assert_eq!(mock.hits(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// get_feroxresponse_from_link should bail in the event that the url is already in scanned_urls
async fn get_feroxresponse_from_link_bails_on_seen_url() -> Result<()> {
    let url = "/unique-for-this-test.php";
    let srv = MockServer::start();
    let served = srv.url(url);

    let mock = srv.mock(|when, then| {
        when.method(GET).path(url);
        then.status(200)
            .body("this is a unique test, don't reuse the endpoint");
    });

    SCANS.add_file_scan(&served, ROBOTS_EXT.stats.clone());

    let r_resp = ROBOTS_EXT.get_feroxresponse_from_link(&served).await;
    let b_resp = BODY_EXT.get_feroxresponse_from_link(&served).await;

    assert!(r_resp.is_err());
    assert!(b_resp.is_err());
    assert_eq!(mock.hits(), 0); // function exits before requests can happen
    Ok(())
}
