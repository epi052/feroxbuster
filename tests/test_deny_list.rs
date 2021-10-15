mod utils;
use assert_cmd::prelude::*;
use assert_cmd::Command;
use httpmock::Method::GET;
use httpmock::MockServer;
use predicates::prelude::*;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// test that the deny list prevents a request if the requested url is a match
fn deny_list_works_during_with_a_normal_scan() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

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
        .arg("--dont-scan")
        .arg(srv.url("/LICENSE"))
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(srv.url("/LICENSE")).not());

    assert_eq!(mock.hits(), 0);
}

#[test]
/// test that the deny list prevents requests of urls found during extraction
fn deny_list_works_during_extraction() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body(&srv.url("'/homepage/assets/img/icons/handshake.svg'"));
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET)
            .path("/homepage/assets/img/icons/handshake.svg");
        then.status(200);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .arg("--dont-scan")
        .arg(srv.url("/homepage/"))
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("/homepage/assets/img/icons/handshake.svg").not()),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 0);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// test that the deny list prevents requests of urls found during recursion
fn deny_list_works_during_recursion() {
    let srv = MockServer::start();
    let urls = [
        "js".to_string(),
        "prod".to_string(),
        "dev".to_string(),
        "file.js".to_string(),
    ];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist").unwrap();

    let js_mock = srv.mock(|when, then| {
        when.method(GET).path("/js");
        then.status(301).header("Location", &srv.url("/js/"));
    });

    let js_prod_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/prod");
        then.status(301).header("Location", &srv.url("/js/prod/"));
    });

    let js_dev_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/dev");
        then.status(301).header("Location", &srv.url("/js/dev/"));
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
        .arg("-t")
        .arg("1")
        .arg("--dont-scan")
        .arg(srv.url("/js/dev"))
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::is_match("301.*js")
            .unwrap()
            .and(predicate::str::is_match("301.*js/prod").unwrap())
            .and(predicate::str::is_match("301.*js/dev").unwrap())
            .not()
            .and(predicate::str::is_match("200.*js/dev/file.js").unwrap())
            .not(),
    );

    assert_eq!(js_mock.hits(), 1);
    assert_eq!(js_prod_mock.hits(), 1);
    assert_eq!(js_dev_mock.hits(), 0);
    assert_eq!(js_dev_file_mock.hits(), 0);

    teardown_tmp_directory(tmp_dir);
}

#[test]
/// test that the deny list prevents requests of urls found during recursion when the denier is a
/// parent of a user-specified scan
fn deny_list_works_during_recursion_with_inverted_parents() {
    let srv = MockServer::start();
    let urls = [
        "js".to_string(),
        "prod".to_string(),
        "dev".to_string(),
        "api".to_string(),
        "file.js".to_string(),
    ];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist").unwrap();

    let js_mock = srv.mock(|when, then| {
        when.method(GET).path("/js");
        then.status(301).header("Location", &srv.url("/js/"));
    });

    let api_mock = srv.mock(|when, then| {
        when.method(GET).path("/api");
        then.status(200);
    });

    let js_prod_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/prod");
        then.status(301).header("Location", &srv.url("/js/prod/"));
    });

    let js_dev_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/dev");
        then.status(301).header("Location", &srv.url("/js/dev/"));
    });

    let js_dev_file_mock = srv.mock(|when, then| {
        when.method(GET).path("/js/dev/file.js");
        then.status(200)
            .body("this is a test and is more bytes than other ones");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/js"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-t")
        .arg("1")
        .arg("-vvvv")
        .arg("--dont-scan")
        .arg(srv.url("/"))
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::is_match("301.*js")
            .unwrap()
            .and(predicate::str::is_match("301.*js/prod").unwrap())
            .and(predicate::str::is_match("301.*js/dev").unwrap())
            .and(predicate::str::is_match("200.*js/dev/file.js").unwrap())
            .and(predicate::str::is_match("200.*api").unwrap())
            .not(),
    );

    assert_eq!(js_mock.hits(), 1);
    assert_eq!(js_prod_mock.hits(), 1);
    assert_eq!(js_dev_mock.hits(), 1);
    assert_eq!(js_dev_file_mock.hits(), 1);
    assert_eq!(api_mock.hits(), 0);

    teardown_tmp_directory(tmp_dir);
}

#[test]
/// test that a regex that prevents the base url from being scanned results in an early exit
fn deny_list_prevents_regex_that_denies_base_url() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

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
        .arg("--dont-scan")
        .arg("/")
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    let err_msg = format!(
        "Could not determine initial targets: The regex '/' matches {}/; the scan will never start",
        srv.base_url()
    );
    cmd.assert()
        .success()
        .stderr(predicate::str::contains(err_msg));

    assert_eq!(mock.hits(), 0);
}

#[test]
/// test that a url that prevents the base url from being scanned results in an early exit
fn deny_list_prevents_url_that_denies_base_url() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

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
        .arg("--dont-scan")
        .arg(srv.base_url())
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    let err_msg = format!(
        "Could not determine initial targets: The url '{}/' matches {}/; the scan will never start",
        srv.base_url(),
        srv.base_url()
    );

    cmd.assert()
        .success()
        .stderr(predicate::str::contains(err_msg));

    assert_eq!(mock.hits(), 0);
}
