mod utils;

use assert_cmd::Command;
use httpmock::prelude::*;
use httpmock::MockServer;
use regex::Regex;
use std::fs::{read_to_string, write};
use utils::{setup_tmp_directory, teardown_tmp_directory};

/// Helper to create a test wordlist with controllable patterns
fn create_test_wordlist(
    normal: usize,
    errors: usize,
    status403: usize,
    status429: usize,
) -> String {
    let mut words = Vec::new();

    // Normal responses
    for i in 0..normal {
        words.push(format!("normal_{:06}", i));
    }

    // Timeout errors
    for i in 0..errors {
        words.push(format!("error_{:06}", i));
    }

    // 403 responses
    for i in 0..status403 {
        words.push(format!("s403_{:06}", i));
    }

    // 429 responses
    for i in 0..status429 {
        words.push(format!("s429_{:06}", i));
    }

    words.join("\n")
}

/// Scenario 1: High 403 rate - tests policy enforcement
#[test]
fn scenario_high_403_rate() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&[], "wordlist").unwrap();
    let (log_dir, logfile) = setup_tmp_directory(&[], "debug-log").unwrap();

    // Create wordlist with high 403 rate
    // Need 90%+ ratio and enough requests to trigger policy: 900/(900+100) = 90%
    let wordlist = create_test_wordlist(100, 0, 900, 0);
    write(&file, wordlist).unwrap();

    let _normal_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/normal_.*").unwrap());
        then.status(200).body("OK");
    });

    let _forbidden_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/s403_.*").unwrap());
        then.status(403).body("Forbidden");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--dont-filter")
        .arg("--threads")
        .arg("10")
        .arg("--debug-log")
        .arg(logfile.as_os_str())
        .arg("--json")
        .arg("-vv")
        .assert()
        .success();

    let debug_log = read_to_string(&logfile).unwrap();

    let mut found_403_policy = false;

    for line in debug_log.lines() {
        if let Ok(log) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(msg) = log.get("message").and_then(|m| m.as_str()) {
                if msg.contains("auto-tune:") && msg.contains("enforcing limit") {
                    found_403_policy = true;
                }
            }
        }
    }

    teardown_tmp_directory(tmp_dir);
    teardown_tmp_directory(log_dir);

    assert!(found_403_policy, "High 403 rate should trigger policy");
}

/// Scenario 2: High 429 rate - tests aggressive rate limiting
#[test]
fn scenario_high_429_rate() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&[], "wordlist").unwrap();
    let (log_dir, logfile) = setup_tmp_directory(&[], "debug-log").unwrap();

    // High 429 rate should trigger more aggressive limiting
    // Need 30%+ ratio and enough requests: 450/(450+150) = 75%
    let wordlist = create_test_wordlist(150, 0, 0, 450);
    write(&file, wordlist).unwrap();

    let _normal_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/normal_.*").unwrap());
        then.status(200).body("OK");
    });

    let _rate_limit_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/s429_.*").unwrap());
        then.status(429).body("Too Many Requests");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--dont-filter")
        .arg("--threads")
        .arg("10")
        .arg("--debug-log")
        .arg(logfile.as_os_str())
        .arg("--json")
        .arg("-vv")
        .assert()
        .success();

    let debug_log = read_to_string(&logfile).unwrap();

    let mut found_429_policy = false;

    for line in debug_log.lines() {
        if let Ok(log) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(msg) = log.get("message").and_then(|m| m.as_str()) {
                if msg.contains("auto-tune:") && msg.contains("enforcing limit") {
                    found_429_policy = true;
                }
            }
        }
    }

    teardown_tmp_directory(tmp_dir);
    teardown_tmp_directory(log_dir);

    assert!(found_429_policy, "High 429 rate should trigger policy");
}

/// Scenario 3: Recovery pattern - errors then normal
#[test]
fn scenario_recovery_pattern() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&[], "wordlist").unwrap();
    let (log_dir, logfile) = setup_tmp_directory(&[], "debug-log").unwrap();

    // Pattern: errors first, then normal - should slow down then speed up
    let mut wordlist = Vec::new();
    for i in 0..100 {
        wordlist.push(format!("s403_{:04}", i));
    }
    for i in 0..300 {
        wordlist.push(format!("normal_{:04}", i));
    }

    write(&file, wordlist.join("\n")).unwrap();

    let _normal_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/normal_.*").unwrap());
        then.status(200).body("OK");
    });

    let _error_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/s403_.*").unwrap());
        then.status(403).body("Forbidden");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--dont-filter")
        .arg("--threads")
        .arg("10")
        .arg("--debug-log")
        .arg(logfile.as_os_str())
        .arg("--json")
        .arg("-vv")
        .assert()
        .success();

    let debug_log = read_to_string(&logfile).unwrap();

    let mut auto_tune_triggered = false;

    for line in debug_log.lines() {
        if let Ok(log) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(msg) = log.get("message").and_then(|m| m.as_str()) {
                if msg.contains("auto-tune:") && msg.contains("enforcing limit") {
                    auto_tune_triggered = true;
                }
            }
        }
    }

    teardown_tmp_directory(tmp_dir);
    teardown_tmp_directory(log_dir);

    assert!(
        auto_tune_triggered,
        "Should trigger auto-tune due to errors"
    );
}

/// Scenario 4: Mixed steady state - balanced errors and normal
#[test]
fn scenario_mixed_steady_state() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&[], "wordlist").unwrap();
    let (log_dir, logfile) = setup_tmp_directory(&[], "debug-log").unwrap();

    // Evenly mixed - not enough to trigger bail, but enough for tuning
    // Need 25+ general errors to trigger: 30 >= 25
    let wordlist = create_test_wordlist(150, 30, 10, 10);
    write(&file, wordlist).unwrap();

    let normal_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/normal_.*").unwrap());
        then.status(200).body("OK");
    });

    let error_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/error_.*").unwrap());
        then.status(504).body("Gateway Timeout");
    });

    let forbidden_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/s403_.*").unwrap());
        then.status(403).body("Forbidden");
    });

    let rate_limit_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/s429_.*").unwrap());
        then.status(429).body("Too Many Requests");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--threads")
        .arg("10")
        .arg("--debug-log")
        .arg(logfile.as_os_str())
        .arg("-vv")
        .assert()
        .success();

    let debug_log = read_to_string(&logfile).unwrap();
    let mut _policy_adjustments = 0;

    for line in debug_log.lines() {
        if let Ok(log) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(msg) = log.get("message").and_then(|m| m.as_str()) {
                if msg.contains("scan speed") || msg.contains("set rate limit") {
                    _policy_adjustments += 1;
                }
            }
        }
    }

    let total =
        normal_mock.hits() + error_mock.hits() + forbidden_mock.hits() + rate_limit_mock.hits();

    teardown_tmp_directory(tmp_dir);
    teardown_tmp_directory(log_dir);

    // With mixed but not extreme errors, should see some adjustments
    assert!(total > 100, "Should complete significant portion of scan");
}

/// Scenario 5: Capped auto-tune - --rate-limit caps --auto-tune adjustments
#[test]
fn scenario_capped_auto_tune() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&[], "wordlist").unwrap();
    let (log_dir, logfile) = setup_tmp_directory(&[], "debug-log").unwrap();

    // Pattern: errors first to trigger rate limiting, then normal responses to allow upward adjustment
    // The rate limit cap should prevent exceeding the specified limit
    let mut wordlist = Vec::new();

    // Start with many errors to trigger auto-tune
    for i in 0..200 {
        wordlist.push(format!("s403_{:04}", i));
    }

    // Then many normal responses to allow upward adjustment
    for i in 0..400 {
        wordlist.push(format!("normal_{:04}", i));
    }

    write(&file, wordlist.join("\n")).unwrap();

    let _normal_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/normal_.*").unwrap());
        then.status(200).body("OK");
    });

    let _error_mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/s403_.*").unwrap());
        then.status(403).body("Forbidden");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--auto-tune")
        .arg("--rate-limit")
        .arg("50") // Cap at 50 req/s
        .arg("--dont-filter")
        .arg("--threads")
        .arg("10")
        .arg("--debug-log")
        .arg(logfile.as_os_str())
        .arg("--json")
        .arg("-vv")
        .assert()
        .success();

    let debug_log = read_to_string(&logfile).unwrap();

    let mut auto_tune_triggered = false;
    let mut max_rate_seen = 0;

    for line in debug_log.lines() {
        if let Ok(log) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(msg) = log.get("message").and_then(|m| m.as_str()) {
                // Check for auto-tune activation
                if msg.contains("auto-tune:") && msg.contains("enforcing limit") {
                    auto_tune_triggered = true;
                }

                // Extract rate values from messages like "set rate limit (25/s)" or "scan speed (30/s)"
                if msg.contains("/s)") {
                    if let Some(start) = msg.rfind('(') {
                        if let Some(end) = msg.rfind("/s)") {
                            if let Ok(rate) = msg[start + 1..end].parse::<usize>() {
                                max_rate_seen = max_rate_seen.max(rate);
                            }
                        }
                    }
                }
            }
        }
    }

    teardown_tmp_directory(tmp_dir);
    teardown_tmp_directory(log_dir);

    assert!(
        auto_tune_triggered,
        "Auto-tune should be triggered by errors"
    );

    assert!(
        max_rate_seen <= 50,
        "Auto-tune should never exceed rate-limit cap of 50, but saw {}",
        max_rate_seen
    );
}
