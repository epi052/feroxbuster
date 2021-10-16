use super::*;
use crate::{
    config::{Configuration, OutputLevel},
    event_handlers::Handles,
    response::FeroxResponse,
    scanner::RESPONSES,
    statistics::Stats,
    traits::FeroxSerialize,
    SLEEP_DURATION, VERSION,
};
use indicatif::ProgressBar;
use predicates::prelude::*;
use std::sync::{atomic::Ordering, Arc};
use std::thread::sleep;
use std::time::Instant;
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
    assert!(result);
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
        OutputLevel::Default,
        Some(pb),
    );

    assert!(urls.insert(scan));

    let (result, _scan) = urls.add_scan(url, ScanType::Directory, ScanOrder::Latest);

    assert!(!result);
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
        OutputLevel::Default,
        Some(pb),
    );

    assert!(!scan
        .progress_bar
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .is_finished());

    scan.stop_progress_bar();

    assert!(scan
        .progress_bar
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .is_finished());
}

#[test]
/// add a known url to the hashset, without a trailing slash, expect false
fn add_url_to_list_of_scanned_urls_with_known_url_without_slash() {
    let urls = FeroxScans::default();
    let url = "http://unknown_url";

    let scan = FeroxScan::new(
        url,
        ScanType::File,
        ScanOrder::Latest,
        0,
        OutputLevel::Default,
        None,
    );

    assert!(urls.insert(scan));

    let (result, _scan) = urls.add_scan(url, ScanType::File, ScanOrder::Latest);

    assert!(!result);
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
        OutputLevel::Default,
        Some(pb),
    );
    let scan_two = FeroxScan::new(
        url_two,
        ScanType::Directory,
        ScanOrder::Latest,
        pb_two.length(),
        OutputLevel::Default,
        Some(pb_two),
    );

    scan_two.finish().unwrap(); // one complete, one incomplete
    scan_two
        .set_task(tokio::spawn(async move {
            sleep(Duration::from_millis(SLEEP_DURATION));
        }))
        .await
        .unwrap();

    assert!(urls.insert(scan));
    assert!(urls.insert(scan_two));

    urls.display_scans().await;
}

#[test]
/// ensure that PartialEq compares FeroxScan.id fields
fn partial_eq_compares_the_id_field() {
    let url = "http://unknown_url/";
    let scan = FeroxScan::new(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        0,
        OutputLevel::Default,
        None,
    );
    let scan_two = FeroxScan::new(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        0,
        OutputLevel::Default,
        None,
    );

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
        OutputLevel::Default,
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
        OutputLevel::Default,
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
    assert!(response.wildcard());
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
        OutputLevel::Default,
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
    for expected in [
        r#""scans""#,
        &format!(r#""id":"{}""#, saved_id),
        r#""url":"https://spiritanimal.com""#,
        r#""scan_type":"Directory""#,
        r#""status":"NotStarted""#,
        r#""num_requests":0"#,
        r#""config""#,
        r#""type":"configuration""#,
        r#""wordlist":"/usr/share/seclists/Discovery/Web-Content/raft-medium-directories.txt""#,
        r#""config""#,
        r#""proxy":"""#,
        r#""replay_proxy":"""#,
        r#""target_url":"""#,
        r#""status_codes":[200,204,301,302,307,308,401,403,405,500]"#,
        r#""replay_codes":[200,204,301,302,307,308,401,403,405,500]"#,
        r#""filter_status":[]"#,
        r#""threads":50"#,
        r#""timeout":7"#,
        r#""verbosity":0"#,
        r#""silent":false"#,
        r#""quiet":false"#,
        r#""auto_bail":false"#,
        r#""auto_tune":false"#,
        r#""json":false"#,
        r#""output":"""#,
        r#""debug_log":"""#,
        &format!(r#""user_agent":"feroxbuster/{}""#, VERSION),
        r#""random_agent":false"#,
        r#""redirects":false"#,
        r#""insecure":false"#,
        r#""extensions":[]"#,
        r#""headers""#,
        r#""queries":[]"#,
        r#""no_recursion":false"#,
        r#""extract_links":false"#,
        r#""add_slash":false"#,
        r#""stdin":false"#,
        r#""depth":4"#,
        r#""scan_limit":0"#,
        r#""parallel":0"#,
        r#""rate_limit":0"#,
        r#""filter_size":[]"#,
        r#""filter_line_count":[]"#,
        r#""filter_word_count":[]"#,
        r#""filter_regex":[]"#,
        r#""dont_filter":false"#,
        r#""resumed":false"#,
        r#""resume_from":"""#,
        r#""save_state":false"#,
        r#""time_limit":"""#,
        r#""filter_similar":[]"#,
        r#""url_denylist":[]"#,
        r#""responses""#,
        r#""type":"response""#,
        r#""url":"https://nerdcore.com/css""#,
        r#""path":"/css""#,
        r#""wildcard":true"#,
        r#""status":301"#,
        r#""content_length":173"#,
        r#""line_count":10"#,
        r#""word_count":16"#,
        r#""headers""#,
        r#""server":"nginx/1.16.1"#,
    ]
    .iter()
    {
        assert!(
            predicates::str::contains(*expected).eval(&json_state),
            "{}",
            expected
        )
    }
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
        start_time: Instant::now(),
        output_level: OutputLevel::Default,
        status_403s: Default::default(),
        status_429s: Default::default(),
        status: Default::default(),
        task: tokio::sync::Mutex::new(None),
        progress_bar: std::sync::Mutex::new(None),
        errors: Default::default(),
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
        start_time: Instant::now(),
        output_level: OutputLevel::Default,
        status_403s: Default::default(),
        status_429s: Default::default(),
        status: std::sync::Mutex::new(ScanStatus::Running),
        task: tokio::sync::Mutex::new(Some(tokio::spawn(async move {
            sleep(Duration::from_millis(SLEEP_DURATION * 2));
        }))),
        progress_bar: std::sync::Mutex::new(None),
        errors: Default::default(),
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

    let nums = menu.split_to_nums("1, 3,      4, 7 -     12, 10-10, 10-11, 9-12, 12-6, -1, 4-");

    assert_eq!(nums, vec![1, 3, 4, 7, 8, 9, 10, 11, 12]);
    assert_eq!(menu.split_to_nums("9-12"), vec![9, 10, 11, 12]);
    assert!(menu.split_to_nums("-12").is_empty());
    assert!(menu.split_to_nums("12-").is_empty());
    assert!(menu.split_to_nums("\n").is_empty());
}

#[test]
/// given a deep url, find the correct scan
fn get_base_scan_by_url_finds_correct_scan() {
    let urls = FeroxScans::default();
    let url = "http://localhost";
    let url1 = "http://localhost/stuff";
    let url2 = "http://shlocalhost/stuff/things";
    let url3 = "http://shlocalhost/stuff/things/mostuff";
    let (_, scan) = urls.add_scan(url, ScanType::Directory, ScanOrder::Latest);
    let (_, scan1) = urls.add_scan(url1, ScanType::Directory, ScanOrder::Latest);
    let (_, scan2) = urls.add_scan(url2, ScanType::Directory, ScanOrder::Latest);
    let (_, scan3) = urls.add_scan(url3, ScanType::Directory, ScanOrder::Latest);

    assert_eq!(
        urls.get_base_scan_by_url("http://localhost/things.php")
            .unwrap()
            .id,
        scan.id
    );
    assert_eq!(
        urls.get_base_scan_by_url("http://localhost/stuff/things.php")
            .unwrap()
            .id,
        scan1.id
    );
    assert_eq!(
        urls.get_base_scan_by_url("http://shlocalhost/stuff/things/mostuff.php")
            .unwrap()
            .id,
        scan2.id
    );
    assert_eq!(
        urls.get_base_scan_by_url("http://shlocalhost/stuff/things/mostuff/mothings.php")
            .unwrap()
            .id,
        scan3.id
    );
}

#[test]
/// given a shallow url without a trailing slash, find the correct scan
fn get_base_scan_by_url_finds_correct_scan_without_trailing_slash() {
    let urls = FeroxScans::default();
    let url = "http://localhost";
    let (_, scan) = urls.add_scan(url, ScanType::Directory, ScanOrder::Latest);
    assert_eq!(
        urls.get_base_scan_by_url("http://localhost/BKPMiherrortBPKcw")
            .unwrap()
            .id,
        scan.id
    );
}

#[test]
/// given a shallow url with a trailing slash, find the correct scan
fn get_base_scan_by_url_finds_correct_scan_with_trailing_slash() {
    let urls = FeroxScans::default();
    let url = "http://127.0.0.1:41971/";
    let (_, scan) = urls.add_scan(url, ScanType::Directory, ScanOrder::Latest);
    assert_eq!(
        urls.get_base_scan_by_url("http://127.0.0.1:41971/BKPMiherrortBPKcw")
            .unwrap()
            .id,
        scan.id
    );
}
