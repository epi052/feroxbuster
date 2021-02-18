use super::container::UpdateStatus;
use super::*;
use crate::{config::Configuration, event_handlers::Handles, scan_manager::FeroxScans};
use httpmock::Method::GET;
use httpmock::MockServer;
use std::{io::stderr, sync::Arc, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test to hit no execution of targets for loop in banner
async fn banner_intialize_without_targets() {
    let config = Configuration::new().unwrap();
    let banner = Banner::new(&[], &config);
    banner.print_to(stderr(), Arc::new(config)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test to hit no execution of statuscode for loop in banner
async fn banner_intialize_without_status_codes() {
    let config = Configuration {
        status_codes: vec![],
        ..Default::default()
    };

    let banner = Banner::new(&[String::from("http://localhost")], &config);
    banner.print_to(stderr(), Arc::new(config)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test to hit an empty config file
async fn banner_intialize_without_config_file() {
    let config = Configuration {
        config: String::new(),
        ..Default::default()
    };

    let banner = Banner::new(&[String::from("http://localhost")], &config);
    banner.print_to(stderr(), Arc::new(config)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test to hit an empty queries
async fn banner_intialize_without_queries() {
    let config = Configuration {
        queries: vec![(String::new(), String::new())],
        ..Default::default()
    };

    let banner = Banner::new(&[String::from("http://localhost")], &config);
    banner.print_to(stderr(), Arc::new(config)).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test that
async fn banner_needs_update_returns_unknown_with_bad_url() {
    let handles = Arc::new(Handles::for_testing(None, None).0);

    let mut banner = Banner::new(
        &[String::from("http://localhost")],
        &Configuration::new().unwrap(),
    );

    let _ = banner.check_for_updates("", handles).await;

    assert!(matches!(banner.update_status, UpdateStatus::Unknown));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test return value of good url to needs_update
async fn banner_needs_update_returns_up_to_date() {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/latest");
        then.status(200).body("{\"tag_name\":\"v1.1.0\"}");
    });
    let scans = Arc::new(FeroxScans::default());

    let handles = Arc::new(Handles::for_testing(Some(scans), None).0);

    let mut banner = Banner::new(&[srv.url("")], &Configuration::new().unwrap());
    banner.version = String::from("1.1.0");

    let _ = banner.check_for_updates(&srv.url("/latest"), handles).await;

    assert_eq!(mock.hits(), 1);
    assert!(matches!(banner.update_status, UpdateStatus::UpToDate));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test return value of good url to needs_update that returns a newer version than current
async fn banner_needs_update_returns_out_of_date() {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/latest");
        then.status(200).body("{\"tag_name\":\"v1.1.0\"}");
    });

    let scans = Arc::new(FeroxScans::default());

    let handles = Arc::new(Handles::for_testing(Some(scans), None).0);

    let mut banner = Banner::new(&[srv.url("")], &Configuration::new().unwrap());
    banner.version = String::from("1.0.1");

    let _ = banner.check_for_updates(&srv.url("/latest"), handles).await;

    assert_eq!(mock.hits(), 1);
    assert!(matches!(banner.update_status, UpdateStatus::OutOfDate));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test return value of good url that times out
async fn banner_needs_update_returns_unknown_on_timeout() {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/latest");
        then.status(200)
            .body("{\"tag_name\":\"v1.1.0\"}")
            .delay(Duration::from_secs(8));
    });

    let handles = Arc::new(Handles::for_testing(None, None).0);

    let mut banner = Banner::new(&[srv.url("")], &Configuration::new().unwrap());

    let _ = banner.check_for_updates(&srv.url("/latest"), handles).await;

    assert_eq!(mock.hits(), 1);
    assert!(matches!(banner.update_status, UpdateStatus::Unknown));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test return value of good url with bad json response
async fn banner_needs_update_returns_unknown_on_bad_json_response() {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/latest");
        then.status(200).body("not json");
    });

    let handles = Arc::new(Handles::for_testing(None, None).0);

    let mut banner = Banner::new(&[srv.url("")], &Configuration::new().unwrap());

    let _ = banner.check_for_updates(&srv.url("/latest"), handles).await;

    assert_eq!(mock.hits(), 1);
    assert!(matches!(banner.update_status, UpdateStatus::Unknown));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// test return value of good url with json response that lacks the tag_name field
async fn banner_needs_update_returns_unknown_on_json_without_correct_tag() {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/latest");
        then.status(200)
            .body("{\"no tag_name\": \"doesn't exist\"}");
    });

    let handles = Arc::new(Handles::for_testing(None, None).0);

    let mut banner = Banner::new(&[srv.url("")], &Configuration::new().unwrap());
    banner.version = String::from("1.0.1");

    let _ = banner.check_for_updates(&srv.url("/latest"), handles).await;

    assert_eq!(mock.hits(), 1);
    assert!(matches!(banner.update_status, UpdateStatus::Unknown));
}
