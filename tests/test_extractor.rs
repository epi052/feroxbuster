mod utils;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::{Mock, MockServer};
use predicates::prelude::*;
use std::process::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// send a request to a page that contains a relative link, --extract-links should find the link
/// and make a request to the new link
fn extractor_finds_absolute_url() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(200)
        .return_body(&srv.url("'/homepage/assets/img/icons/handshake.svg'"))
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/homepage/assets/img/icons/handshake.svg")
        .return_status(200)
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/handshake.svg",
            )),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a request to a page that contains an absolute link to another domain, scanner should not
/// follow
fn extractor_finds_absolute_url_to_different_domain() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(200)
        .return_body("\"http://localhost/homepage/assets/img/icons/handshake.svg\"")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/handshake.svg",
            ))
            .not(),
    );

    assert_eq!(mock.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a request to a page that contains a relative link, should follow
fn extractor_finds_relative_url() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(200)
        .return_body("\"/homepage/assets/img/icons/handshake.svg\"")
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/homepage/assets/img/icons/handshake.svg")
        .return_status(200)
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/handshake.svg",
            )),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a request to a page that contains an relative link, follow it, and find the same link again
/// should follow then filter
fn extractor_finds_same_relative_url_twice() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "README".to_string()], "wordlist").unwrap();

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(200)
        .return_body(&srv.url("\"/homepage/assets/img/icons/handshake.svg\""))
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/README")
        .return_body(&srv.url("\"/homepage/assets/img/icons/handshake.svg\""))
        .return_status(200)
        .create_on(&srv);

    let mock_three = Mock::new()
        .expect_method(GET)
        .expect_path("/homepage/assets/img/icons/handshake.svg")
        .return_status(200)
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/handshake.svg",
            )),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    assert_eq!(mock_three.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// send a request to a page that contains an absolute link that leads to a page with a filter_size
/// that should filter it out, expect not to see the second response reported
fn extractor_finds_filtered_content() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "README".to_string()], "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(200)
        .return_body(&srv.url("\"/homepage/assets/img/icons/handshake.svg\""))
        .create_on(&srv);

    let mock_two = Mock::new()
        .expect_method(GET)
        .expect_path("/homepage/assets/img/icons/handshake.svg")
        .return_body("im a little teapot")
        .return_status(200)
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .arg("--filter-size")
        .arg("18")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/handshake.svg",
            ))
            .not(),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock_two.times_called(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}
