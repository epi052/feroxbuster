mod utils;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::{Mock, MockServer};
use predicates::prelude::*;
use std::process::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// create a FeroxResponse that should elicit a true from
/// StatusCodeFilter::should_filter_response
fn filters_status_code_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(302)
        .return_body("this is a test")
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/file.js")
        .return_status(200)
        .return_body("this is also a test of some import")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("--filter-status")
        .arg("302")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .not()
            .and(predicate::str::contains("302"))
            .not()
            .and(predicate::str::contains("14c"))
            .not()
            .and(predicate::str::contains("/file.js"))
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("34c")),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// LinesFilter::should_filter_response
fn filters_lines_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(302)
        .return_body("this is a test")
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/file.js")
        .return_status(200)
        .return_body("this is also a test of some import\nwith 2 lines, no less")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-lines")
        .arg("2")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("302"))
            .and(predicate::str::contains("14"))
            .and(predicate::str::contains("/file.js"))
            .not()
            .and(predicate::str::contains("200"))
            .not()
            .and(predicate::str::contains("2l"))
            .not(),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// WordsFilter::should_filter_response
fn filters_words_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(302)
        .return_body("this is a test")
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/file.js")
        .return_status(200)
        .return_body("this is also a test of some import\nwith 2 lines, no less")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-words")
        .arg("13")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("302"))
            .and(predicate::str::contains("14"))
            .and(predicate::str::contains("/file.js"))
            .not()
            .and(predicate::str::contains("200"))
            .not()
            .and(predicate::str::contains("13w"))
            .not(),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// SizeFilter::should_filter_response
fn filters_size_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(302)
        .return_body("this is a test")
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/file.js")
        .return_status(200)
        .return_body("this is also a test of some import\nwith 2 lines, no less")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-size")
        .arg("56")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("302"))
            .and(predicate::str::contains("14"))
            .and(predicate::str::contains("/file.js"))
            .not()
            .and(predicate::str::contains("200"))
            .not()
            .and(predicate::str::contains("56c"))
            .not(),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
}
