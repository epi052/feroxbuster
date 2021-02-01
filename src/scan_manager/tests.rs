use super::*;
use crate::{
    config::Configuration, event_handlers::Handles, response::FeroxResponse, scanner::RESPONSES,
    statistics::Stats, FeroxSerialize, SLEEP_DURATION, VERSION,
};
use indicatif::ProgressBar;
use predicates::prelude::*;
use std::sync::{atomic::Ordering, Arc};
use std::thread::sleep;
use tokio::time::{self, Duration};

#[test]
/// test that ScanType's default is File
fn default_scantype_is_file() {
    match ScanType::default() {
        ScanType::File => {}
        ScanType::Directory => panic!(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// tests that pause_scan pauses execution and releases execution when PAUSE_SCAN is toggled
/// the spinner used during the test has had .finish_and_clear called on it, meaning that
/// a new one will be created, taking the if branch within the function
async fn scanner_pause_scan_with_finished_spinner() {
    let now = time::Instant::now();
    let urls = FeroxScans::default();

    PAUSE_SCAN.store(true, Ordering::Relaxed);

    let expected = time::Duration::from_secs(2);

    tokio::spawn(async move {
        time::sleep(expected).await;
        PAUSE_SCAN.store(false, Ordering::Relaxed);
    });

    urls.pause(false).await;

    assert!(now.elapsed() > expected);
}

#[test]
/// add an unknown url to the hashset, expect true
fn add_url_to_list_of_scanned_urls_with_unknown_url() {
    let urls = FeroxScans::default();
    let url = "http://unknown_url";
    let (result, _scan) = urls.add_scan(url, ScanType::Directory, ScanOrder::Latest);
    assert_eq!(result, true);
}

#[test]
/// add a known url to the hashset, with a trailing slash, expect false
fn add_url_to_list_of_scanned_urls_with_known_url() {
    let urls = FeroxScans::default();
    let pb = ProgressBar::new(1);
    let url = "http://unknown_url/";

    let scan = FeroxScan::new(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        pb.length(),
        false,
        Some(pb),
    );

    assert_eq!(urls.insert(scan), true);

    let (result, _scan) = urls.add_scan(url, ScanType::Directory, ScanOrder::Latest);

    assert_eq!(result, false);
}

#[test]
/// stop_progress_bar should stop the progress bar
fn stop_progress_bar_stops_bar() {
    let pb = ProgressBar::new(1);
    let url = "http://unknown_url/";

    let scan = FeroxScan::new(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        pb.length(),
        false,
        Some(pb),
    );

    assert_eq!(
        scan.progress_bar
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .is_finished(),
        false
    );

    scan.stop_progress_bar();

    assert_eq!(
        scan.progress_bar
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .is_finished(),
        true
    );
}

#[test]
/// add a known url to the hashset, without a trailing slash, expect false
fn add_url_to_list_of_scanned_urls_with_known_url_without_slash() {
    let urls = FeroxScans::default();
    let url = "http://unknown_url";

    let scan = FeroxScan::new(url, ScanType::File, ScanOrder::Latest, 0, false, None);

    assert_eq!(urls.insert(scan), true);

    let (result, _scan) = urls.add_scan(url, ScanType::File, ScanOrder::Latest);

    assert_eq!(result, false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// just increasing coverage, no real expectations
async fn call_display_scans() {
    let urls = FeroxScans::default();
    let pb = ProgressBar::new(1);
    let pb_two = ProgressBar::new(2);
    let url = "http://unknown_url/";
    let url_two = "http://unknown_url/fa";
    let scan = FeroxScan::new(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        pb.length(),
        false,
        Some(pb),
    );
    let scan_two = FeroxScan::new(
        url_two,
        ScanType::Directory,
        ScanOrder::Latest,
        pb_two.length(),
        false,
        Some(pb_two),
    );

    scan_two.finish().unwrap(); // one complete, one incomplete
    scan_two
        .set_task(tokio::spawn(async move {
            sleep(Duration::from_millis(SLEEP_DURATION));
        }))
        .await
        .unwrap();

    assert_eq!(urls.insert(scan), true);
    assert_eq!(urls.insert(scan_two), true);

    urls.display_scans().await;
}

#[test]
/// ensure that PartialEq compares FeroxScan.id fields
fn partial_eq_compares_the_id_field() {
    let url = "http://unknown_url/";
    let scan = FeroxScan::new(url, ScanType::Directory, ScanOrder::Latest, 0, false, None);
    let scan_two = FeroxScan::new(url, ScanType::Directory, ScanOrder::Latest, 0, false, None);

    assert!(!scan.eq(&scan_two));

    let scan_two = scan.clone();

    assert!(scan.eq(&scan_two));
}

#[test]
/// show that a new progress bar is created if one doesn't exist
fn ferox_scan_get_progress_bar_when_none_is_set() {
    let scan = FeroxScan::default();

    assert!(scan.progress_bar.lock().unwrap().is_none()); // no pb exists

    let pb = scan.progress_bar();

    assert!(scan.progress_bar.lock().unwrap().is_some()); // new pb created
    assert!(!pb.is_finished()) // not finished
}

#[test]
/// given a JSON entry representing a FeroxScan, test that it deserializes into the proper type
/// with the right attributes
fn ferox_scan_deserialize() {
    let fs_json = r#"{"id":"057016a14769414aac9a7a62707598cb","url":"https://spiritanimal.com","scan_type":"Directory","status":"Complete"}"#;
    let fs_json_two = r#"{"id":"057016a14769414aac9a7a62707598cb","url":"https://spiritanimal.com","scan_type":"Not Correct","status":"Cancelled"}"#;
    let fs_json_three = r#"{"id":"057016a14769414aac9a7a62707598cb","url":"https://spiritanimal.com","scan_type":"Not Correct","status":"","num_requests":42}"#;

    let fs: FeroxScan = serde_json::from_str(fs_json).unwrap();
    let fs_two: FeroxScan = serde_json::from_str(fs_json_two).unwrap();
    let fs_three: FeroxScan = serde_json::from_str(fs_json_three).unwrap();
    assert_eq!(fs.url, "https://spiritanimal.com");

    match fs.scan_type {
        ScanType::Directory => {}
        ScanType::File => {
            panic!();
        }
    }
    match fs_two.scan_type {
        ScanType::Directory => {
            panic!();
        }
        ScanType::File => {}
    }

    match *fs.progress_bar.lock().unwrap() {
        None => {}
        Some(_) => {
            panic!();
        }
    }
    assert!(matches!(*fs.status.lock().unwrap(), ScanStatus::Complete));
    assert!(matches!(
        *fs_two.status.lock().unwrap(),
        ScanStatus::Cancelled
    ));
    assert!(matches!(
        *fs_three.status.lock().unwrap(),
        ScanStatus::NotStarted
    ));
    assert_eq!(fs_three.num_requests, 42);
    assert_eq!(fs.id, "057016a14769414aac9a7a62707598cb");
}

#[test]
/// given a FeroxScan, test that it serializes into the proper JSON entry
fn ferox_scan_serialize() {
    let fs = FeroxScan::new(
        "https://spiritanimal.com",
        ScanType::Directory,
        ScanOrder::Latest,
        0,
        false,
        None,
    );
    let fs_json = format!(
        r#"{{"id":"{}","url":"https://spiritanimal.com","scan_type":"Directory","status":"NotStarted","num_requests":0}}"#,
        fs.id
    );
    assert_eq!(fs_json, serde_json::to_string(&*fs).unwrap());
}

#[test]
/// given a FeroxScans, test that it serializes into the proper JSON entry
fn ferox_scans_serialize() {
    let ferox_scan = FeroxScan::new(
        "https://spiritanimal.com",
        ScanType::Directory,
        ScanOrder::Latest,
        0,
        false,
        None,
    );
    let ferox_scans = FeroxScans::default();
    let ferox_scans_json = format!(
        r#"[{{"id":"{}","url":"https://spiritanimal.com","scan_type":"Directory","status":"NotStarted","num_requests":0}}]"#,
        ferox_scan.id
    );
    ferox_scans.scans.write().unwrap().push(ferox_scan);
    assert_eq!(
        ferox_scans_json,
        serde_json::to_string(&ferox_scans).unwrap()
    );
}

#[test]
/// given a FeroxResponses, test that it serializes into the proper JSON entry
fn ferox_responses_serialize() {
    let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"}}"#;
    let response: FeroxResponse = serde_json::from_str(json_response).unwrap();

    let responses = FeroxResponses::default();
    responses.insert(response);
    // responses has a response now

    // serialized should be a list of responses
    let expected = format!("[{}]", json_response);

    let serialized = serde_json::to_string(&responses).unwrap();
    assert_eq!(expected, serialized);
}

#[test]
/// given a FeroxResponse, test that it serializes into the proper JSON entry
fn ferox_response_serialize_and_deserialize() {
    // deserialize
    let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"}}"#;
    let response: FeroxResponse = serde_json::from_str(json_response).unwrap();

    assert_eq!(response.url().as_str(), "https://nerdcore.com/css");
    assert_eq!(response.url().path(), "/css");
    assert_eq!(response.wildcard(), true);
    assert_eq!(response.status().as_u16(), 301);
    assert_eq!(response.content_length(), 173);
    assert_eq!(response.line_count(), 10);
    assert_eq!(response.word_count(), 16);
    assert_eq!(response.headers().get("server").unwrap(), "nginx/1.16.1");

    // serialize, however, this can fail when headers are out of order
    let new_json = serde_json::to_string(&response).unwrap();
    assert_eq!(json_response, new_json);
}

#[test]
/// test FeroxSerialize implementation of FeroxState
fn feroxstates_feroxserialize_implementation() {
    let ferox_scan = FeroxScan::new(
        "https://spiritanimal.com",
        ScanType::Directory,
        ScanOrder::Latest,
        0,
        false,
        None,
    );
    let ferox_scans = FeroxScans::default();
    let saved_id = ferox_scan.id.clone();
    ferox_scans.insert(ferox_scan);

    let config = Configuration::new().unwrap();
    let stats = Arc::new(Stats::new(config.extensions.len(), config.json));

    let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"}}"#;
    let response: FeroxResponse = serde_json::from_str(json_response).unwrap();
    RESPONSES.insert(response);

    let ferox_state = FeroxState::new(
        Arc::new(ferox_scans),
        Arc::new(Configuration::new().unwrap()),
        &RESPONSES,
        stats,
    );

    let expected_strs = predicates::str::contains("scans: FeroxScans").and(
        predicate::str::contains("config: Configuration")
            .and(predicate::str::contains("responses: FeroxResponses"))
            .and(predicate::str::contains("nerdcore.com"))
            .and(predicate::str::contains("/css"))
            .and(predicate::str::contains("https://spiritanimal.com")),
    );

    assert!(expected_strs.eval(&ferox_state.as_str()));

    let json_state = ferox_state.as_json().unwrap();
    let expected = format!(
        r#"{{"scans":[{{"id":"{}","url":"https://spiritanimal.com","scan_type":"Directory","status":"NotStarted","num_requests":0}}],"config":{{"type":"configuration","wordlist":"/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt","config":"","proxy":"","replay_proxy":"","target_url":"","status_codes":[200,204,301,302,307,308,401,403,405],"replay_codes":[200,204,301,302,307,308,401,403,405],"filter_status":[],"threads":50,"timeout":7,"verbosity":0,"quiet":false,"json":false,"output":"","debug_log":"","user_agent":"feroxbuster/{}","redirects":false,"insecure":false,"extensions":[],"headers":{{}},"queries":[],"no_recursion":false,"extract_links":false,"add_slash":false,"stdin":false,"depth":4,"scan_limit":0,"filter_size":[],"filter_line_count":[],"filter_word_count":[],"filter_regex":[],"dont_filter":false,"resumed":false,"resume_from":"","save_state":false,"time_limit":"","filter_similar":[]}},"responses":[{{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{{"server":"nginx/1.16.1"}}}}]"#,
        saved_id, VERSION
    );
    println!("{}\n{}", expected, json_state);
    assert!(predicates::str::contains(expected).eval(&json_state));
}

#[should_panic]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// call start_max_time_thread with a valid timespec, expect a panic, but only after a certain
/// number of seconds
async fn start_max_time_thread_panics_after_delay() {
    let now = time::Instant::now();
    let delay = time::Duration::new(3, 0);

    let config = Configuration {
        time_limit: String::from("3s"),
        ..Default::default()
    };

    let handles = Arc::new(Handles::for_testing(None, Some(Arc::new(config))).0);

    start_max_time_thread(handles).await;

    assert!(now.elapsed() > delay);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// call start_max_time_thread with a timespec that's too large to be parsed correctly, expect
/// immediate return and no panic, as the sigint handler is never called
async fn start_max_time_thread_returns_immediately_with_too_large_input() {
    let now = time::Instant::now();
    let delay = time::Duration::new(1, 0);
    let config = Configuration {
        time_limit: String::from("18446744073709551616m"),
        ..Default::default()
    };

    let handles = Arc::new(Handles::for_testing(None, Some(Arc::new(config))).0);

    // pub const MAX: usize = usize::MAX; // 18_446_744_073_709_551_615usize
    start_max_time_thread(handles).await; // can't fit in dest u64

    assert!(now.elapsed() < delay); // assuming function call will take less than 1second
}

#[test]
/// coverage for FeroxScan's Display implementation
fn feroxscan_display() {
    let scan = FeroxScan {
        id: "".to_string(),
        url: String::from("http://localhost"),
        scan_order: ScanOrder::Latest,
        scan_type: Default::default(),
        num_requests: 0,
        quiet: false,
        status: Default::default(),
        task: tokio::sync::Mutex::new(None),
        progress_bar: std::sync::Mutex::new(None),
    };

    let not_started = format!("{}", scan);

    assert!(predicate::str::contains("not started")
        .and(predicate::str::contains("localhost"))
        .eval(&not_started));

    scan.set_status(ScanStatus::Complete).unwrap();
    let complete = format!("{}", scan);
    assert!(predicate::str::contains("complete")
        .and(predicate::str::contains("localhost"))
        .eval(&complete));

    scan.set_status(ScanStatus::Cancelled).unwrap();
    let cancelled = format!("{}", scan);
    assert!(predicate::str::contains("cancelled")
        .and(predicate::str::contains("localhost"))
        .eval(&cancelled));

    scan.set_status(ScanStatus::Running).unwrap();
    let running = format!("{}", scan);
    assert!(predicate::str::contains("running")
        .and(predicate::str::contains("localhost"))
        .eval(&running));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// call FeroxScan::abort, ensure status becomes cancelled
async fn ferox_scan_abort() {
    let scan = FeroxScan {
        id: "".to_string(),
        url: String::from("http://localhost"),
        scan_order: ScanOrder::Latest,
        scan_type: Default::default(),
        num_requests: 0,
        quiet: false,
        status: std::sync::Mutex::new(ScanStatus::Running),
        task: tokio::sync::Mutex::new(Some(tokio::spawn(async move {
            sleep(Duration::from_millis(SLEEP_DURATION * 2));
        }))),
        progress_bar: std::sync::Mutex::new(None),
    };

    scan.abort().await.unwrap();

    assert!(matches!(
        *scan.status.lock().unwrap(),
        ScanStatus::Cancelled
    ));
}

#[test]
/// call a few menu functions for coverage's sake
///
/// there's not a trivial way to test these programmatically (at least i'm too lazy rn to do it)
/// and their correctness can be verified easily manually; just calling for now
fn menu_print_header_and_footer() {
    let menu = Menu::new();
    menu.clear_screen();
    menu.print_header();
    menu.print_footer();
    menu.hide_progress_bars();
    menu.show_progress_bars();
}

#[test]
/// ensure spaces are trimmed and numbers are returned from split_to_nums
fn split_to_nums_is_correct() {
    let menu = Menu::new();

    let nums = menu.split_to_nums("1, 3,      4");

    assert_eq!(nums, vec![1, 3, 4]);
}
