mod utils;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::MockServer;
use predicates::prelude::*;
use std::process::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// Test that small responses under the limit are not truncated
fn response_size_limit_small_response_not_truncated() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["test".to_string()], "wordlist").unwrap();

    let small_body = "Small response that should not be truncated";

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/test");
        then.status(200).body(small_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("1024") // 1KB limit
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/test")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("43c")) // content length (was 44c but actual is 43c)
            .and(predicate::str::contains("truncated to size limit").not()), // should not be truncated
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test that large responses over the limit are truncated and marked appropriately
fn response_size_limit_large_response_truncated() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["large".to_string()], "wordlist").unwrap();

    // Create a response larger than our limit
    let large_body = "A".repeat(2048); // 2KB of 'A' characters

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/large");
        then.status(200).body(&large_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("1024") // 1KB limit, smaller than response
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/large")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("truncated to size limit")), // should be truncated
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test that multiple responses are handled correctly with size limits
fn response_size_limit_mixed_response_sizes() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(
        &[
            "small".to_string(),
            "large".to_string(),
            "medium".to_string(),
        ],
        "wordlist",
    )
    .unwrap();

    let small_body = "Small";
    let medium_body = "B".repeat(512); // 512 bytes
    let large_body = "C".repeat(2048); // 2KB

    let mock_small = srv.mock(|when, then| {
        when.method(GET).path("/small");
        then.status(200).body(small_body);
    });

    let mock_medium = srv.mock(|when, then| {
        when.method(GET).path("/medium");
        then.status(200).body(&medium_body);
    });

    let mock_large = srv.mock(|when, then| {
        when.method(GET).path("/large");
        then.status(200).body(&large_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("1024") // 1KB limit
        .arg("-vvvv")
        .unwrap();

    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Small response should not be truncated
    assert!(stdout.contains("/small"));
    assert!(
        !stdout.contains("/small")
            || !stdout.contains("(truncated to size limit)")
            || !stdout
                .lines()
                .find(|line| line.contains("/small"))
                .unwrap_or("")
                .contains("(truncated to size limit)")
    );

    // Medium response should not be truncated (512 < 1024)
    assert!(stdout.contains("/medium"));
    assert!(
        !stdout.contains("/medium")
            || !stdout.contains("(truncated to size limit)")
            || !stdout
                .lines()
                .find(|line| line.contains("/medium"))
                .unwrap_or("")
                .contains("(truncated to size limit)")
    );

    // Large response should be truncated (2048 > 1024)
    assert!(stdout.contains("/large"));
    assert!(stdout
        .lines()
        .any(|line| line.contains("/large") && line.contains("truncated to size limit")));

    assert_eq!(mock_small.hits(), 1);
    assert_eq!(mock_medium.hits(), 1);
    assert_eq!(mock_large.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test the default response size limit (4MB)
fn response_size_limit_default_4mb() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["test".to_string()], "wordlist").unwrap();

    // Create a response smaller than 4MB default limit
    let body = "D".repeat(1024 * 1024); // 1MB

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/test");
        then.status(200).body(&body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        // No --response-size-limit specified, should use 4MB default
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/test")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("truncated to size limit").not()), // 1MB < 4MB default
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test very small response size limit (smaller than typical HTTP headers/metadata)
fn response_size_limit_very_small_limit() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["tiny".to_string()], "wordlist").unwrap();

    let body = "This is a response that will definitely be truncated";

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/tiny");
        then.status(200).body(body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("10") // Very small 10 byte limit
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/tiny")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("truncated to size limit")), // Should be truncated
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test response size limit with redirects (3xx responses)
fn response_size_limit_with_redirects() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["redirect".to_string()], "wordlist").unwrap();

    let large_redirect_body = "E".repeat(2048); // 2KB redirect response

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/redirect");
        then.status(301)
            .header("Location", "/redirected")
            .body(&large_redirect_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("1024") // 1KB limit
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/redirect")
            .and(predicate::str::contains("301"))
            .and(predicate::str::contains("1024c")), // Should show 1024c (truncated size)
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test response size limit with error responses (4xx/5xx)
fn response_size_limit_with_error_responses() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["error".to_string()], "wordlist").unwrap();

    let large_error_body = format!(
        "{}{}{}",
        "<html><head><title>Error</title></head><body>",
        "F".repeat(2048), // 2KB of error content
        "</body></html>"
    );

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/error");
        then.status(500).body(&large_error_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("1024") // 1KB limit
        .arg("--status-codes")
        .arg("500") // Include 500 responses
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/error")
            .and(predicate::str::contains("500"))
            .and(predicate::str::contains("truncated to size limit")), // Should be truncated
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test JSON output includes truncated field
fn response_size_limit_json_output_includes_truncated_field() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["jsontest".to_string()], "wordlist").unwrap();
    let output_file = tmp_dir.path().join("output.json");

    let large_body = "G".repeat(2048); // 2KB

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/jsontest");
        then.status(200).body(&large_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("1024") // 1KB limit
        .arg("--json")
        .arg("--output")
        .arg(output_file.as_os_str())
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success();

    // Read the JSON output file
    let json_content = std::fs::read_to_string(&output_file).unwrap();

    // Should contain truncated: true for the large response
    assert!(json_content.contains("\"truncated\":true"));
    assert!(json_content.contains("/jsontest"));

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test that banner shows response size limit when non-default value is used
fn response_size_limit_shows_in_banner() {
    let (tmp_dir, file) = setup_tmp_directory(&["test".to_string()], "wordlist").unwrap();

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://127.0.0.1:1") // Non-existent server to trigger quick exit
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("2097152") // 2MB
        .arg("--timeout")
        .arg("1") // Quick timeout
        .unwrap();

    cmd.assert()
        .success() // It actually succeeds with graceful error handling
        .stderr(
            predicate::str::contains("Response Size Limit")
                .and(predicate::str::contains("2097152 bytes")),
        );

    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test edge case: response exactly at the limit
fn response_size_limit_exact_limit() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["exact".to_string()], "wordlist").unwrap();

    let exact_body = "H".repeat(1024); // Exactly 1KB

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/exact");
        then.status(200).body(&exact_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--response-size-limit")
        .arg("1024") // Exactly the limit
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/exact")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("truncated to size limit").not()), // Should not be truncated (exact match)
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// Test response size limit with configuration file
fn response_size_limit_from_config_file() {
    let srv = MockServer::start();
    let (tmp_dir, wordlist_file) =
        setup_tmp_directory(&["configtest".to_string()], "wordlist").unwrap();

    // Create ferox-config.toml in the same temp directory
    let config_content = "response_size_limit = 512";
    let config_file = tmp_dir.path().join("ferox-config.toml");
    std::fs::write(&config_file, config_content).unwrap();

    let large_body = "I".repeat(1024); // 1KB, larger than config limit

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/configtest");
        then.status(200).body(&large_body);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .current_dir(tmp_dir.path())
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(wordlist_file.as_os_str())
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/configtest")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("truncated to size limit")), // Should be truncated due to config
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}
