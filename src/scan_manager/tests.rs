use super::*;
use crate::filters::{
    FeroxFilters, LinesFilter, RegexFilter, SimilarityFilter, SizeFilter, StatusCodeFilter,
    WordsFilter,
};
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
use regex::Regex;
use std::sync::atomic::AtomicBool;
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
    let handles = Arc::new(Handles::for_testing(None, None).0);

    PAUSE_SCAN.store(true, Ordering::Relaxed);

    let expected = time::Duration::from_secs(2);

    tokio::spawn(async move {
        time::sleep(expected).await;
        PAUSE_SCAN.store(false, Ordering::Relaxed);
    });

    urls.pause(false, handles).await;

    assert!(now.elapsed() > expected);
}

#[test]
/// add an unknown url to the hashset, expect true
fn add_url_to_list_of_scanned_urls_with_unknown_url() {
    let urls = FeroxScans::default();
    let url = "http://unknown_url";
    let (result, _scan) = urls.add_scan(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        Arc::new(Handles::for_testing(None, None).0),
    );
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
        pb.length().unwrap(),
        OutputLevel::Default,
        Some(pb),
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );

    assert!(urls.insert(scan));

    let (result, _scan) = urls.add_scan(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        Arc::new(Handles::for_testing(None, None).0),
    );

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
        pb.length().unwrap(),
        OutputLevel::Default,
        Some(pb),
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );

    assert!(!scan
        .progress_bar
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .is_finished());

    scan.stop_progress_bar(0);

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

    let scan: Arc<FeroxScan> = FeroxScan::new(
        url,
        ScanType::File,
        ScanOrder::Latest,
        0,
        OutputLevel::Default,
        None,
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );

    assert!(urls.insert(scan));

    let (result, _scan) = urls.add_scan(
        url,
        ScanType::File,
        ScanOrder::Latest,
        Arc::new(Handles::for_testing(None, None).0),
    );

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
        pb.length().unwrap(),
        OutputLevel::Default,
        Some(pb),
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );
    let scan_two = FeroxScan::new(
        url_two,
        ScanType::Directory,
        ScanOrder::Latest,
        pb_two.length().unwrap(),
        OutputLevel::Default,
        Some(pb_two),
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );

    scan_two.finish(0).unwrap(); // one complete, one incomplete
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
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );
    let scan_two = FeroxScan::new(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        0,
        OutputLevel::Default,
        None,
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );

    assert!(!scan.eq(&scan_two));

    #[allow(clippy::redundant_clone)]
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
    let fs_json = r#"{"id":"057016a14769414aac9a7a62707598cb","url":"https://spiritanimal.com","scan_type":"Directory","status":"Complete","requests_made_so_far":500}"#;
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

    match fs.progress_bar.lock() {
        Ok(guard) => {
            if guard.is_some() {
                panic!();
            }
        }
        Err(_) => {
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
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );
    let fs_json = format!(
        r#"{{"id":"{}","url":"https://spiritanimal.com","normalized_url":"https://spiritanimal.com/","scan_type":"Directory","status":"NotStarted","num_requests":0,"requests_made_so_far":0}}"#,
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
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );
    let ferox_scans = FeroxScans::default();
    let ferox_scans_json = format!(
        r#"[{{"id":"{}","url":"https://spiritanimal.com","normalized_url":"https://spiritanimal.com/","scan_type":"Directory","status":"NotStarted","num_requests":0,"requests_made_so_far":0}}]"#,
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
    let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","original_url":"https://nerdcore.com","path":"/css","wildcard":true,"status":301,"method":"GET","content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"},"extension":"","timestamp":1711796681.3455093}"#;
    let response: FeroxResponse = serde_json::from_str(json_response).unwrap();

    let responses = FeroxResponses::default();
    responses.insert(response);
    // responses has a response now

    // serialized should be a list of responses
    let expected = format!("[{json_response}]");

    let serialized = serde_json::to_string(&responses).unwrap();
    assert_eq!(expected, serialized);
}

#[test]
/// given a FeroxResponse, test that it serializes into the proper JSON entry
fn ferox_response_serialize_and_deserialize() {
    // deserialize
    let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","original_url":"https://nerdcore.com","path":"/css","wildcard":true,"status":301,"method":"GET","content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"},"extension":"","timestamp":1711796681.3455093}"#;
    let response: FeroxResponse = serde_json::from_str(json_response).unwrap();

    assert_eq!(response.url().as_str(), "https://nerdcore.com/css");
    assert_eq!(response.url().path(), "/css");
    assert!(response.wildcard());
    assert_eq!(response.status().as_u16(), 301);
    assert_eq!(response.content_length(), 173);
    assert_eq!(response.line_count(), 10);
    assert_eq!(response.word_count(), 16);
    assert_eq!(response.headers().get("server").unwrap(), "nginx/1.16.1");
    assert_eq!(response.timestamp(), 1711796681.3455093);

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
        true,
        Arc::new(Handles::for_testing(None, None).0),
    );
    let ferox_scans = FeroxScans::default();
    let saved_id = ferox_scan.id.clone();

    ferox_scans.insert(ferox_scan);

    ferox_scans
        .collected_extensions
        .write()
        .unwrap()
        .insert(String::from("php"));

    let mut config = Configuration::new().unwrap();

    config.collect_extensions = true;

    let stats = Arc::new(Stats::new(config.json));

    let json_response = r#"{"type":"response","url":"https://nerdcore.com/css","path":"/css","wildcard":true,"status":301,"content_length":173,"line_count":10,"word_count":16,"headers":{"server":"nginx/1.16.1"},"extension":""}"#;
    let response: FeroxResponse = serde_json::from_str(json_response).unwrap();
    RESPONSES.insert(response);

    let filters = FeroxFilters::default();
    filters
        .push(Box::new(StatusCodeFilter { filter_code: 100 }))
        .unwrap();
    filters
        .push(Box::new(WordsFilter { word_count: 200 }))
        .unwrap();
    filters
        .push(Box::new(SizeFilter {
            content_length: 300,
        }))
        .unwrap();
    filters
        .push(Box::new(LinesFilter { line_count: 400 }))
        .unwrap();
    filters
        .push(Box::new(RegexFilter {
            raw_string: ".*".to_string(),
            compiled: Regex::new(".*").unwrap(),
        }))
        .unwrap();
    filters
        .push(Box::new(SimilarityFilter {
            hash: 1,
            original_url: "http://localhost:12345/".to_string(),
        }))
        .unwrap();

    let ferox_state = FeroxState::new(
        Arc::new(ferox_scans),
        Arc::new(config),
        &RESPONSES,
        stats,
        Arc::new(filters),
    );

    let expected_strs = predicates::str::contains("scans: FeroxScans").and(
        predicate::str::contains("config: Configuration")
            .and(predicate::str::contains("responses: FeroxResponses"))
            .and(predicate::str::contains("nerdcore.com"))
            .and(predicate::str::contains("/css"))
            .and(predicate::str::contains("https://spiritanimal.com"))
            .and(predicate::str::contains("php")),
    );

    assert!(expected_strs.eval(&ferox_state.as_str()));

    let json_state = ferox_state.as_json().unwrap();

    println!("echo '{json_state}'|jq"); // for debugging, if the test fails, can see what's going on

    for expected in [
        r#""scans""#,
        &format!(r#""id":"{saved_id}""#),
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
        r#""status_codes":[100,101,102,200,201,202,203,204,205,206,207,208,226,300,301,302,303,304,305,307,308,400,401,402,403,404,405,406,407,408,409,410,411,412,413,414,415,416,417,418,421,422,423,424,426,428,429,431,451,500,501,502,503,504,505,506,507,508,510,511,103,425]"#,
        r#""replay_codes":[100,101,102,200,201,202,203,204,205,206,207,208,226,300,301,302,303,304,305,307,308,400,401,402,403,404,405,406,407,408,409,410,411,412,413,414,415,416,417,418,421,422,423,424,426,428,429,431,451,500,501,502,503,504,505,506,507,508,510,511,103,425]"#,
        r#""filter_status":[]"#,
        r#""threads":50"#,
        r#""timeout":7"#,
        r#""verbosity":0"#,
        r#""silent":false"#,
        r#""quiet":false"#,
        r#""auto_bail":false"#,
        r#""auto_tune":false"#,
        r#""force_recursion":false"#,
        r#""json":false"#,
        r#""output":"""#,
        r#""debug_log":"""#,
        &format!(r#""user_agent":"feroxbuster/{VERSION}""#),
        r#""random_agent":false"#,
        r#""redirects":false"#,
        r#""insecure":false"#,
        r#""extensions":[]"#,
        r#""methods":["GET"],"#,
        r#""data":[]"#,
        r#""headers""#,
        r#""queries":[]"#,
        r#""no_recursion":false"#,
        r#""extract_links":true"#,
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
        r#""client_cert":"""#,
        r#""client_key":"""#,
        r#""server_certs":[]"#,
        r#""url":"https://nerdcore.com/css""#,
        r#""path":"/css""#,
        r#""wildcard":true"#,
        r#""status":301"#,
        r#""method":"GET""#,
        r#""content_length":173"#,
        r#""line_count":10"#,
        r#""limit_bars":0"#,
        r#""word_count":16"#,
        r#""headers""#,
        r#""server":"nginx/1.16.1"#,
        r#""collect_extensions":true"#,
        r#""collect_backups":false"#,
        r#""collect_words":false"#,
        r#""scan_dir_listings":false"#,
        r#""protocol":"https""#,
        r#""filters":[{"filter_code":100},{"word_count":200},{"content_length":300},{"line_count":400},{"compiled":".*","raw_string":".*"},{"hash":1,"original_url":"http://localhost:12345/"}]"#,
        r#""collected_extensions":["php"]"#,
        r#""dont_collect":["tif","tiff","ico","cur","bmp","webp","svg","png","jpg","jpeg","jfif","gif","avif","apng","pjpeg","pjp","mov","wav","mpg","mpeg","mp3","mp4","m4a","m4p","m4v","ogg","webm","ogv","oga","flac","aac","3gp","css","zip","xls","xml","gz","tgz"]"#,
    ]
    .iter()
    {
        assert!(
            predicates::str::contains(*expected).eval(&json_state)
        );
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
        normalized_url: String::from("http://localhost/"),
        scan_order: ScanOrder::Latest,
        scan_type: Default::default(),
        handles: Some(Arc::new(Handles::for_testing(None, None).0)),
        num_requests: 0,
        requests_made_so_far: 0,
        visible: AtomicBool::new(true),
        start_time: Instant::now(),
        output_level: OutputLevel::Default,
        status_403s: Default::default(),
        status_429s: Default::default(),
        status: Default::default(),
        task: tokio::sync::Mutex::new(None),
        progress_bar: std::sync::Mutex::new(None),
        errors: Default::default(),
    };

    let not_started = format!("{scan}");

    assert!(predicate::str::contains("not started")
        .and(predicate::str::contains("localhost"))
        .eval(&not_started));

    scan.set_status(ScanStatus::Complete).unwrap();
    let complete = format!("{scan}");
    assert!(predicate::str::contains("complete")
        .and(predicate::str::contains("localhost"))
        .eval(&complete));

    scan.set_status(ScanStatus::Cancelled).unwrap();
    let cancelled = format!("{scan}");
    assert!(predicate::str::contains("cancelled")
        .and(predicate::str::contains("localhost"))
        .eval(&cancelled));

    scan.set_status(ScanStatus::Running).unwrap();
    let running = format!("{scan}");
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
        normalized_url: String::from("http://localhost/"),
        scan_order: ScanOrder::Latest,
        scan_type: Default::default(),
        num_requests: 0,
        requests_made_so_far: 0,
        start_time: Instant::now(),
        output_level: OutputLevel::Default,
        visible: AtomicBool::new(true),
        status_403s: Default::default(),
        status_429s: Default::default(),
        status: std::sync::Mutex::new(ScanStatus::Running),
        task: tokio::sync::Mutex::new(Some(tokio::spawn(async move {
            sleep(Duration::from_millis(SLEEP_DURATION * 2));
        }))),
        progress_bar: std::sync::Mutex::new(None),
        errors: Default::default(),
        handles: Some(Arc::new(Handles::for_testing(None, None).0)),
    };

    scan.abort(0).await.unwrap();

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
    let menu_cmd_1 = MenuCmd::AddUrl(String::from("http://localhost"));
    let menu_cmd_2 = MenuCmd::Cancel(vec![0], false);
    let menu_cmd_res_1 = MenuCmdResult::Url(String::from("http://localhost"));
    let menu_cmd_res_2 = MenuCmdResult::NumCancelled(2);
    println!("{menu_cmd_1:?}{menu_cmd_2:?}{menu_cmd_res_1:?}{menu_cmd_res_2:?}");
    menu.clear_screen();
    menu.print_header();
    menu.print_footer();
    menu.hide_progress_bars();
    menu.show_progress_bars();
}

/// ensure command parsing from user input results int he correct MenuCmd returned
#[test]
fn menu_get_command_input_from_user_returns_cancel() {
    let menu = Menu::new();

    for (idx, cmd) in ["cancel", "Cancel", "c", "C"].iter().enumerate() {
        let force = idx % 2 == 0;

        let full_cmd = if force {
            format!("{cmd} -f {idx}\n")
        } else {
            format!("{cmd} {idx}\n")
        };

        let result = menu.get_command_input_from_user(&full_cmd).unwrap();

        assert!(matches!(result, MenuCmd::Cancel(_, _)));

        if let MenuCmd::Cancel(canx_list, ret_force) = result {
            assert_eq!(canx_list, vec![idx]);
            assert_eq!(force, ret_force);
        }
    }
}

/// ensure command parsing from user input results int he correct MenuCmd returned
#[test]
fn menu_get_command_input_from_user_returns_add() {
    let menu = Menu::new();

    for cmd in ["add", "Addd", "a", "A", "None"] {
        let test_url = "http://happyfuntimes.commmm";
        let full_cmd = format!("{cmd} {test_url}\n");

        if cmd != "None" {
            let result = menu.get_command_input_from_user(&full_cmd).unwrap();
            assert!(matches!(result, MenuCmd::AddUrl(_)));

            if let MenuCmd::AddUrl(url) = result {
                assert_eq!(url, test_url);
            }
        } else {
            assert!(menu.get_command_input_from_user(&full_cmd).is_none());
        };
    }
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
    let handles = Arc::new(Handles::for_testing(None, None).0);
    let urls = FeroxScans::default();
    let url = "http://localhost";
    let url1 = "http://localhost/stuff";
    let url2 = "http://shlocalhost/stuff/things";
    let url3 = "http://shlocalhost/stuff/things/mostuff";
    let (_, scan) = urls.add_scan(url, ScanType::Directory, ScanOrder::Latest, handles.clone());
    let (_, scan1) = urls.add_scan(
        url1,
        ScanType::Directory,
        ScanOrder::Latest,
        handles.clone(),
    );
    let (_, scan2) = urls.add_scan(
        url2,
        ScanType::Directory,
        ScanOrder::Latest,
        handles.clone(),
    );
    let (_, scan3) = urls.add_scan(url3, ScanType::Directory, ScanOrder::Latest, handles);

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
    let (_, scan) = urls.add_scan(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        Arc::new(Handles::for_testing(None, None).0),
    );
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
    let (_, scan) = urls.add_scan(
        url,
        ScanType::Directory,
        ScanOrder::Latest,
        Arc::new(Handles::for_testing(None, None).0),
    );
    assert_eq!(
        urls.get_base_scan_by_url("http://127.0.0.1:41971/BKPMiherrortBPKcw")
            .unwrap()
            .id,
        scan.id
    );
}
