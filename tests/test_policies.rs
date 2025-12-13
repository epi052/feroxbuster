mod utils;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::MockServer;
use regex::Regex;
use std::fs::{read_to_string, write};
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use tokio::time::Duration;
use utils::{setup_tmp_directory, teardown_tmp_directory};

// tests/policy-test-error-words is a wordlist with the following attributes:
// - 60 errors per error category (error, 403, 429)
// - 1000 words tagged as normal for noise/padding
// - each error string is 6_RANDOM_ASCII{error,status403,status429,normal}6_RANDOM_ASCII
// examples:
// - BKPMiherrortBPKcw
// - lTjbLpstatus403fZQaFD
// - ZhGBHGstatus429SIUZvI
// - ufzEXWnormalOLhbLM
// these words will be used along with pattern matching to trigger different policies

#[test]
/// --auto-bail should cancel a scan with spurious 403s
fn auto_bail_cancels_scan_with_403s() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["ignored".to_string()], "wordlist").unwrap();
    let (log_dir, logfile) = setup_tmp_directory(&[], "debug-log").unwrap();

    let policy_words = read_to_string(Path::new("tests/policy-test-words.shuffled")).unwrap();

    write(&file, policy_words).unwrap();

    assert_eq!(file.metadata().unwrap().len(), 117720); // sanity check on wordlist size

    let error_mock = srv.mock(|when, then| {
        when.method(GET).path_matches(
            Regex::new("/[a-zA-Z]{6}(error|status429|status403)[a-zA-Z]{6}").unwrap(),
        );
        then.status(200).body("other errors are still a 200");
    });

    let normal_reqs_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z]{6}normal[a-zA-Z]{6}").unwrap());
        then.status(403)
            .body("these guys need to be 403 in order to trigger 90% threshold");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-bail")
        .arg("--dont-filter")
        .arg("--threads")
        .arg("4")
        .arg("--debug-log")
        .arg(logfile.as_os_str())
        .arg("-vv")
        .arg("--json")
        .assert()
        .success();

    println!("log filesize: {}", logfile.metadata().unwrap().len());
    let debug_log = read_to_string(logfile).unwrap();
    let re = Regex::new("total_expected: ([0-9]+),").unwrap();

    // read debug log to get the number of errors enforced
    for line in debug_log.lines() {
        let log: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        if let Some(message) = log.get("message") {
            let str_msg = message.as_str().unwrap_or_default().to_string();

            if str_msg.starts_with("Stats") {
                println!("{str_msg}");
                assert!(re.is_match(&str_msg));
                let total_expected = re
                    .captures(&str_msg)
                    .unwrap()
                    .get(1)
                    .map_or("", |m| m.as_str())
                    .parse::<usize>()
                    .unwrap();
                println!("total_expected: {total_expected}");
                assert!(total_expected < 5000);
            }
        }
    }

    teardown_tmp_directory(tmp_dir);
    teardown_tmp_directory(log_dir);

    assert!(normal_reqs_mock.hits() + error_mock.hits() > 25); // must have at least 50 reqs fly

    // expect much less in the way of requests for this one, 90% is measured against requests made,
    // not requests expected, so 90% can be reached very quickly. for the same reason, the
    // num_enforced can be less than 50
    assert!(normal_reqs_mock.hits() < 500);
    assert!(error_mock.hits() <= 180); // may or may not see all other error requests
}

#[test]
/// --auto-bail should cancel a scan with spurious 429s
fn auto_bail_cancels_scan_with_429s() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["ignored".to_string()], "wordlist").unwrap();
    let (log_dir, logfile) = setup_tmp_directory(&[], "debug-log").unwrap();

    let policy_words = read_to_string(Path::new("tests/policy-test-words.shuffled")).unwrap();

    write(&file, policy_words).unwrap();

    assert_eq!(file.metadata().unwrap().len(), 117720); // sanity check on wordlist size

    let error_mock = srv.mock(|when, then| {
        when.method(GET).path_matches(
            Regex::new("/[a-zA-Z]{6}(error|status429|status403)[a-zA-Z]{6}").unwrap(),
        );
        then.status(200).body("other errors are still a 200");
    });

    let normal_reqs_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z]{6}normal[a-zA-Z]{6}").unwrap());
        then.status(429)
            .body("these guys need to be 403 in order to trigger 90% threshold");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-bail")
        .arg("--dont-filter")
        .arg("--threads")
        .arg("4")
        .arg("--debug-log")
        .arg(logfile.as_os_str())
        .arg("-vvv")
        .arg("--json")
        .assert()
        .success();

    println!("log filesize: {}", logfile.metadata().unwrap().len());
    let debug_log = read_to_string(logfile).unwrap();
    let re = Regex::new("total_expected: ([0-9]+),").unwrap();

    // read debug log to get the number of errors enforced
    for line in debug_log.lines() {
        let log: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        if let Some(message) = log.get("message") {
            let str_msg = message.as_str().unwrap_or_default().to_string();

            if str_msg.starts_with("Stats") {
                println!("{str_msg}");
                assert!(re.is_match(&str_msg));
                let total_expected = re
                    .captures(&str_msg)
                    .unwrap()
                    .get(1)
                    .map_or("", |m| m.as_str())
                    .parse::<usize>()
                    .unwrap();
                println!("total_expected: {total_expected}");
                assert!(total_expected < 5000);
            }
        }
    }

    teardown_tmp_directory(tmp_dir);
    teardown_tmp_directory(log_dir);

    assert!(normal_reqs_mock.hits() + error_mock.hits() > 25); // must have at least 50 reqs fly

    // expect much less in the way of requests for this one, 90% is measured against requests made,
    // not requests expected, so 90% can be reached very quickly. for the same reason, the
    // num_enforced can be less than 50
    assert!(normal_reqs_mock.hits() < 500);
    assert!(error_mock.hits() <= 180); // may or may not see all other error requests
}

#[test]
/// --auto-tune should slow a scan with spurious 429s
fn auto_tune_slows_scan_with_429s() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["ignored".to_string()], "wordlist").unwrap();

    let policy_words = read_to_string(Path::new("tests/policy-test-words.shuffled")).unwrap();

    write(&file, policy_words).unwrap();

    assert_eq!(file.metadata().unwrap().len(), 117720); // sanity check on wordlist size

    let error_mock = srv.mock(|when, then| {
        when.method(GET).path_matches(
            Regex::new("/[a-zA-Z]{6}(error|status429|status403)[a-zA-Z]{6}").unwrap(),
        );
        then.status(200).body("other errors are still a 200");
    });

    let normal_reqs_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z]{6}normal[a-zA-Z]{6}").unwrap());
        then.status(429)
            .body("these guys need to be 429 in order to trigger 30% threshold");
    });

    let start = Instant::now();

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--dont-filter")
        .arg("--time-limit")
        .arg("7s")
        .arg("--threads")
        .arg("4")
        .assert()
        .failure();

    teardown_tmp_directory(tmp_dir);

    let normal_hits = normal_reqs_mock.hits();
    let error_hits = error_mock.hits();

    println!("normal_reqs_mock.hits(): {}", normal_hits);
    println!("error_mock.hits(): {}", error_hits);

    assert!(normal_hits + error_hits > 25); // must have at least 50 reqs fly

    println!("elapsed: {}", start.elapsed().as_millis());
    // With auto-tune and 429s, the scan should be slowed down but may still process
    // ~1800-2000 requests in 7 seconds. The key is that it hits the time limit.
    assert!(
        normal_hits < 3000,
        "Should process fewer than 3000 requests due to rate limiting"
    );
    assert!(error_hits <= 180); // may or may not see all other error requests
    assert!(start.elapsed().as_millis() >= 7000); // scan should hit time limit due to limiting
}

#[test]
/// --auto-tune should slow a scan with spurious 403s
fn auto_tune_slows_scan_with_403s() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["ignored".to_string()], "wordlist").unwrap();

    let policy_words = read_to_string(Path::new("tests/policy-test-words.shuffled")).unwrap();

    write(&file, policy_words).unwrap();

    assert_eq!(file.metadata().unwrap().len(), 117720); // sanity check on wordlist size

    let error_mock = srv.mock(|when, then| {
        when.method(GET).path_matches(
            Regex::new("/[a-zA-Z]{6}(error|status429|status403)[a-zA-Z]{6}").unwrap(),
        );
        then.status(200).body("other errors are still a 200");
    });

    let normal_reqs_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z]{6}normal[a-zA-Z]{6}").unwrap());
        then.status(403)
            .body("these guys need to be 403 in order to trigger 90% threshold");
    });

    let start = Instant::now();

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--dont-filter")
        .arg("--time-limit")
        .arg("7s")
        .arg("--threads")
        .arg("4")
        .assert()
        .failure();

    teardown_tmp_directory(tmp_dir);

    let normal_hits = normal_reqs_mock.hits();
    let error_hits = error_mock.hits();

    println!("normal_reqs_mock.hits(): {}", normal_hits);
    println!("error_mock.hits(): {}", error_hits);

    assert!(normal_hits + error_hits > 25); // must have at least 50 reqs fly

    println!("elapsed: {}", start.elapsed().as_millis());
    // With auto-tune and 403s, the scan should be slowed down but may still process
    // ~1800-2000 requests in 7 seconds. The key is that it hits the time limit.
    assert!(
        normal_hits < 3000,
        "Should process fewer than 3000 requests due to rate limiting"
    );
    assert!(error_hits <= 180); // may or may not see all other error requests
    assert!(start.elapsed().as_millis() >= 7000); // scan should hit time limit due to limiting
}

#[test]
/// --auto-tune should slow a scan with spurious errors
fn auto_tune_slows_scan_with_general_errors() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["ignored".to_string()], "wordlist").unwrap();

    let policy_words = read_to_string(Path::new("tests/policy-test-words.shuffled")).unwrap();

    write(&file, policy_words).unwrap();

    assert_eq!(file.metadata().unwrap().len(), 117720); // sanity check on wordlist size

    let error_mock = srv.mock(|when, then| {
        when.method(GET).path_matches(
            Regex::new("/[a-zA-Z]{6}(error|status429|status403)[a-zA-Z]{6}").unwrap(),
        );
        then.status(200).body("other errors are still a 200");
    });

    let normal_reqs_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z]{6}normal[a-zA-Z]{6}").unwrap());
        then.status(200)
            .body("these guys need to be 429 in order to trigger 30% threshold")
            .delay(Duration::new(3, 0));
    });

    let start = Instant::now();

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--dont-filter")
        .arg("--time-limit")
        .arg("7s")
        .arg("--threads")
        .arg("4")
        .arg("--timeout")
        .arg("2")
        .assert()
        .failure();

    teardown_tmp_directory(tmp_dir);

    let normal_hits = normal_reqs_mock.hits();
    let error_hits = error_mock.hits();

    println!("normal_reqs_mock.hits(): {}", normal_hits);
    println!("error_mock.hits(): {}", error_hits);
    println!("elapsed: {}", start.elapsed().as_millis());

    // Normal requests timeout (3s delay with 2s timeout), triggering error policy
    // The scan should be rate-limited and hit the time limit
    assert!(
        normal_hits < 3000,
        "Should process fewer requests due to rate limiting and timeouts"
    );
    assert!(error_hits <= 180); // may or may not see all other error requests
    assert!(start.elapsed().as_millis() >= 7000); // scan should hit time limit due to limiting
}
