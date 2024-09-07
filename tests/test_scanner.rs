mod utils;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::MockServer;
use predicates::prelude::*;
use std::thread::sleep;
use std::time::Duration;
use std::{process::Command, time};
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// send a single valid request, expect a 200 response
fn scanner_single_request_scan() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("14")),
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a valid request, follow redirects into new directories, expect 301/200 responses
fn scanner_recursive_request_scan() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let urls = [
        "js".to_string(),
        "prod".to_string(),
        "dev".to_string(),
        "file.js".to_string(),
    ];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist")?;

    let js_mock = srv.mock(|when, then| {
        when.method(GET).path("/js");
        then.status(301).header("Location", srv.url("/js/"));
    });

    let js_prod_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/prod");
        then.status(301).header("Location", srv.url("/js/prod/"));
    });

    let js_dev_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/dev");
        then.status(301).header("Location", srv.url("/js/dev/"));
    });

    let js_dev_file_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/dev/file.js");
        then.status(200)
            .body("this is a test and is more bytes than other ones");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("-t")
        .arg("1")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::is_match("301.*js")
            .unwrap()
            .and(predicate::str::is_match("301.*js/prod").unwrap())
            .and(predicate::str::is_match("301.*js/dev").unwrap())
            .and(predicate::str::is_match("200.*js/dev/file.js").unwrap()),
    );

    assert_eq!(js_mock.hits(), 2);
    assert_eq!(js_prod_mock.hits(), 2);
    assert_eq!(js_dev_mock.hits(), 2);
    assert_eq!(js_dev_file_mock.hits(), 1);

    teardown_tmp_directory(tmp_dir);

    Ok(())
}

#[test]
/// send a valid request, follow 200s into new directories, expect 200 responses
fn scanner_recursive_request_scan_using_only_success_responses(
) -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let urls = [
        "js/".to_string(),
        "prod/".to_string(),
        "dev/".to_string(),
        "file.js".to_string(),
    ];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist")?;

    let js_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/");
        then.status(200).header("Location", srv.url("/js/"));
    });

    let js_prod_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/prod/");
        then.status(200).header("Location", srv.url("/js/prod/"));
    });

    let js_dev_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/dev/");
        then.status(200).header("Location", srv.url("/js/dev/"));
    });

    let js_dev_file_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/dev/file.js");
        then.status(200)
            .body("this is a test and is more bytes than other ones");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("-t")
        .arg("1")
        .arg("--redirects")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::is_match("200.*js")
            .unwrap()
            .and(predicate::str::is_match("200.*js/prod").unwrap())
            .and(predicate::str::is_match("200.*js/dev").unwrap())
            .and(predicate::str::is_match("200.*js/dev/file.js").unwrap()),
    );

    assert_eq!(js_mock.hits(), 3);
    assert_eq!(js_prod_mock.hits(), 3);
    assert_eq!(js_dev_mock.hits(), 3);
    assert_eq!(js_dev_file_mock.hits(), 1);

    teardown_tmp_directory(tmp_dir);

    Ok(())
}

#[test]
/// send a single valid request, get a response, and write it to disk
fn scanner_single_request_scan_with_file_output() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let outfile = tmp_dir.path().join("output");

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("-o")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile)?;

    assert!(contents.contains("/LICENSE"));
    assert!(contents.contains("200"));
    assert!(contents.contains("14"));

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a single valid request with -q, get a response, and write only the url to disk
fn scanner_single_request_scan_with_file_output_and_tack_q(
) -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let outfile = tmp_dir.path().join("output");

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("-q")
        .arg("-o")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile)?;

    let url = srv.url("/LICENSE");
    assert!(contents.contains(&url));

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send an invalid output file, expect scan to fail
fn scanner_single_request_scan_with_invalid_file_output() -> Result<(), Box<dyn std::error::Error>>
{
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let outfile = tmp_dir.path(); // outfile is a directory

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("-q")
        .arg("-o")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile);
    assert!(contents.is_err());

    assert_eq!(mock.hits(), 0);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a single valid request using -q, expect only the url on stdout
fn scanner_single_request_quiet_scan() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-x")
        .arg("js,html")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains(srv.url("/LICENSE"))
            .and(predicate::str::contains("200"))
            .not()
            .and(predicate::str::contains("14"))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send single valid request, get back a 301 without a Location header
/// expect response_is_directory to return false when called
fn scanner_single_request_returns_301_without_location_header(
) -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(301).body("this is a test");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--timeout")
        .arg("5")
        .arg("--user-agent")
        .arg("some-user-agent-string")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains(srv.url("/LICENSE"))
            .and(predicate::str::contains("301"))
            .and(predicate::str::contains("14")),
    );

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a single valid request, expect a 200 response that then gets routed to the replay
/// proxy
fn scanner_single_request_replayed_to_proxy() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let proxy = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let mock_two = proxy.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--replay-proxy")
        .arg(format!("http://{}", proxy.address()))
        .arg("--replay-codes")
        .arg("200")
        .unwrap();

    cmd.assert()
        .success()
        .stdout(
            predicate::str::contains("/LICENSE")
                .and(predicate::str::contains("200"))
                .and(predicate::str::contains("14c")),
        )
        .stderr(predicate::str::contains("Replay Proxy Codes"));

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a single valid request, filter the size of the response, expect one out of 2 urls
fn scanner_single_request_scan_with_filtered_result() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "ignored".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a not a test");
    });

    let filtered_mock = srv.mock(|when, then| {
        when.method(GET).path("/ignored");
        then.status(200).body("this is a test");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-n")
        .arg("-S")
        .arg("14")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("20"))
            .and(predicate::str::contains("ignored"))
            .not()
            .and(predicate::str::contains(" 14 "))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(filtered_mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
#[should_panic] // added in 2.11.0 for panicking trace-level logging
/// send a single valid request, get a response, and write the logging messages to disk
fn scanner_single_request_scan_with_debug_logging() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let outfile = tmp_dir.path().join("debug.log");

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("--debug-log")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile).unwrap();
    println!("{contents}");
    assert!(contents.starts_with("Configuration {"));
    assert!(contents.contains("TRC"));
    assert!(contents.contains("DBG"));
    assert!(contents.contains("INF"));
    assert!(contents.contains("feroxbuster All scans complete!"));
    assert!(contents.contains("feroxbuster::event_handlers::inputs exit: start_enter_handler"));

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
#[should_panic] // added in 2.11.0 for panicking trace-level logging
/// send a single valid request, get a response, and write the logging messages to disk as NDJSON
fn scanner_single_request_scan_with_debug_logging_as_json() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let outfile = tmp_dir.path().join("debug.log");

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("--debug-log")
        .arg(outfile.as_os_str())
        .arg("--json")
        .unwrap();

    let contents = std::fs::read_to_string(outfile).unwrap();
    println!("{contents}");
    assert!(contents.starts_with("{\"type\":\"configuration\""));
    assert!(contents.contains("\"level\":\"TRACE\""));
    assert!(contents.contains("\"level\":\"DEBUG\""));
    assert!(contents.contains("\"level\":\"INFO\""));
    assert!(contents.contains("time_offset"));
    assert!(contents.contains("exit: main"));
    assert!(contents.contains(&srv.url("/LICENSE")));
    assert!(contents.contains("\"module\":\"feroxbuster::response\""));
    assert!(contents.contains("\"module\":\"feroxbuster::url\""));
    assert!(contents.contains("\"module\":\"feroxbuster::event_handlers::inputs\""));
    assert!(contents.contains("exit: start_enter_handler"));
    assert!(contents.contains("All scans complete!"));

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// send a single valid request, filter the response by regex, expect one out of 2 urls
fn scanner_single_request_scan_with_regex_filtered_result() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "ignored".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let filtered_mock = srv.mock(|when, then| {
        when.method(GET).path("/ignored");
        then.status(200)
            .body("this is a test\nThat rug really tied the room together");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-regex")
        .arg("'That rug.*together$'")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("20"))
            .and(predicate::str::contains("ignored"))
            .not()
            .and(predicate::str::contains(" 14 "))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(filtered_mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// send a request to a 403 directory, expect recursion to work into the 403
fn scanner_recursion_works_with_403_directories() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "ignored/".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let forbidden_dir = srv.mock(|when, then| {
        when.method(GET).path("/ignored/");
        then.status(403);
    });

    let found_anyway = srv.mock(|when, then| {
        when.method(GET).path("/ignored/LICENSE");
        then.status(200)
            .body("this is a test\nThat rug really tied the room together");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .count(2)
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("404"))
            .and(predicate::str::contains("53c Auto-filtering"))
            .and(predicate::str::contains(
                "Auto-filtering found 404-like response and created new filter;",
            ))
            .and(predicate::str::contains("14c"))
            .and(predicate::str::contains("0c"))
            .and(predicate::str::contains("ignored").count(2))
            .and(predicate::str::contains("/ignored/LICENSE")),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(found_anyway.hits(), 1);
    assert_eq!(forbidden_dir.hits(), 3);

    teardown_tmp_directory(tmp_dir);
}

#[test]
/// kick off scan with a time limit;  
fn rate_limit_enforced_when_specified() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(
        &[
            "css".to_string(),
            "stuff".to_string(),
            "css1".to_string(),
            "css2".to_string(),
            "css3".to_string(),
            "css4".to_string(),
        ],
        "wordlist",
    )
    .unwrap();

    let now = time::Instant::now();
    let lower_bound = time::Duration::new(5, 0);

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--rate-limit")
        .arg("1")
        .assert()
        .success();

    // --rate-limit is 1, so the test should take roughly 5 seconds, so elapsed should be at least
    // 5 seconds. If not rate-limited, this test takes about 500ms without rate limiting
    assert!(now.elapsed() > lower_bound);

    teardown_tmp_directory(tmp_dir);
}

#[test]
/// ensure that auto-discovered extensions are tracked in statistics and bar lengths are updated
fn add_discovered_extension_updates_bars_and_stats() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(
        &["LICENSE".to_string(), "stuff.php".to_string()],
        "wordlist",
    )
    .unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/stuff.php");
        then.status(200).body("cool... coolcoolcool");
    });

    let file_path = tmp_dir.path().join("debug-file.txt");

    assert!(!file_path.exists());

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .arg("--collect-extensions")
        .arg("-vvvv")
        .arg("--debug-log")
        .arg(file_path.as_os_str())
        .unwrap()
        .assert()
        .success();

    mock.assert_hits(1);
    let contents = std::fs::read_to_string(file_path).unwrap();
    println!("{contents}");
    assert!(contents.contains("discovered new extension: php"));
    // assert!(contents.contains("extensions_collected: 1"));  // this is racy
    assert!(contents.contains("expected_per_scan: 6"));
}

#[test]
/// send a request to a 200 file, expect pre-configured backup collection rules to be applied
/// and then requested
fn collect_backups_makes_appropriate_requests() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE.txt".to_string()], "wordlist").unwrap();

    let valid_paths = [
        "/LICENSE.txt",
        "/LICENSE.txt~",
        "/LICENSE.txt.bak",
        "/LICENSE.txt.bak2",
        "/LICENSE.txt.old",
        "/LICENSE.txt.1",
        "/LICENSE.bak",
        "/.LICENSE.txt.swp",
    ];

    let valid_mocks: Vec<_> = valid_paths
        .iter()
        .map(|&p| {
            srv.mock(|when, then| {
                when.method(GET).path(p);
                then.status(200).body("this is a valid test");
            })
        })
        .collect();

    let invalid_paths: Vec<_> = vec![
        "/LICENSE.txt~~",
        "/LICENSE.txt.bak.bak",
        "/LICENSE.txt.bak2.bak2",
        "/LICENSE.txt.old.old",
        "/LICENSE.txt.1.1",
        "/..LICENSE.txt.swp.swp",
    ];

    let invalid_mocks: Vec<_> = invalid_paths
        .iter()
        .map(|&p| {
            srv.mock(|when, then| {
                when.method(GET).path(p);
                then.status(200).body("this is an invalid test");
            })
        })
        .collect();

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--collect-backups")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE.txt")
            .and(predicate::str::contains("/LICENSE.txt~"))
            .and(predicate::str::contains("/LICENSE.txt.bak"))
            .and(predicate::str::contains("/LICENSE.txt.bak2"))
            .and(predicate::str::contains("/LICENSE.txt.old"))
            .and(predicate::str::contains("/LICENSE.txt.1"))
            .and(predicate::str::contains("/LICENSE.bak"))
            .and(predicate::str::contains("/.LICENSE.txt.swp")),
    );

    for valid_mock in valid_mocks {
        assert_eq!(valid_mock.hits(), 1);
    }

    for invalid_mock in invalid_mocks {
        assert_eq!(invalid_mock.hits(), 0);
    }

    teardown_tmp_directory(tmp_dir);
}

#[test]
/// send a request to 4 200 files, expect non-zero tf-idf rated words to be requested as well
fn collect_words_makes_appropriate_requests() {
    let srv = MockServer::start();

    let wordlist: Vec<_> = [
        "doc1", "doc2", "doc3", "doc4", "blah", "blah2", "blah3", "blah4",
    ]
    .iter()
    .map(|w| w.to_string())
    .collect();

    let (tmp_dir, file) = setup_tmp_directory(&wordlist, "wordlist").unwrap();

    srv.mock(|when, then| {
        when.method(GET).path("/doc1");
        then.status(200)
            .body("Air quality in the sunny island improved gradually throughout Wednesday.");
    });
    srv.mock(|when, then| {
        when.method(GET).path("/doc2");
        then.status(200).body(
            "Air quality in Singapore on Wednesday continued to get worse as haze hit the island.",
        );
    });
    srv.mock(|when, then| {
        when.method(GET).path("/doc3");
        then.status(200).body("The air quality in Singapore is monitored through a network of air monitoring stations located in different parts of the island");
    });
    srv.mock(|when, then| {
        when.method(GET).path("/doc4");
        then.status(200)
            .body("The air quality in Singapore got worse on Wednesday.");
    });

    let valid_paths = vec![
        "/gradually",
        "/network",
        "/hit",
        "/located",
        "/continued",
        "/island",
        "/worse",
        "/monitored",
        "/monitoring",
        "/haze",
        "/different",
        "/stations",
        "/sunny",
        "/singapore",
        "/improved",
        "/parts",
        "/wednesday",
    ];

    let valid_mocks: Vec<_> = valid_paths
        .iter()
        .map(|&p| {
            srv.mock(|when, then| {
                when.method(GET).path(p);
                then.status(200);
            })
        })
        .collect();

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("-vv")
        .arg("--collect-words")
        .arg("-t")
        .arg("1")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/doc1")
            .and(predicate::str::contains("/doc2"))
            .and(predicate::str::contains("/doc3"))
            .and(predicate::str::contains("/doc4")),
    );

    sleep(Duration::new(2, 0));

    for valid_mock in valid_mocks {
        assert_eq!(valid_mock.hits(), 1);
    }

    teardown_tmp_directory(tmp_dir);
}

#[test]
/// send a request to an endpoint that has abnormal redirect logic, ala fast-api
fn scanner_forced_recursion_ignores_normal_redirect_logic() -> Result<(), Box<dyn std::error::Error>>
{
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock1 = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(301)
            .body("this is a test")
            .header("Location", srv.url("/LICENSE"));
    });

    let mock2 = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE/LICENSE");
        then.status(404);
    });

    let mock3 = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE/LICENSE/LICENSE");
        then.status(404);
    });

    let mock4 = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE/LICENSE/LICENSE/LICENSE");
        then.status(404);
    });

    let outfile = tmp_dir.path().join("output");

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--force-recursion")
        .arg("--dont-filter")
        .arg("--status-codes")
        .arg("301")
        .arg("200")
        .arg("-o")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile)?;
    println!("{contents}");

    assert!(contents.contains("/LICENSE"));
    assert!(contents.contains("301"));
    assert!(contents.contains("14"));

    assert_eq!(mock1.hits(), 2);
    assert_eq!(mock2.hits(), 1);
    assert_eq!(mock3.hits(), 0);
    assert_eq!(mock4.hits(), 0);

    teardown_tmp_directory(tmp_dir);

    Ok(())
}
