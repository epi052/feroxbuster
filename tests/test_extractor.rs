mod utils;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::MockServer;
use predicates::prelude::*;
use std::process::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// send a request to a page that contains a relative link, --extract-links should find the link
/// and make a request to the new link
fn extractor_finds_absolute_url() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

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
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/handshake.svg",
            )),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a request to a page that contains an absolute link to another domain, scanner should not
/// follow
fn extractor_finds_absolute_url_to_different_domain() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body("\"http://localhost/homepage/assets/img/icons/handshake.svg\"");
    });

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

    assert_eq!(mock.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a request to a page that contains a relative link, should follow
fn extractor_finds_relative_url() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body("\"/homepage/assets/img/icons/handshake.svg\"");
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
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/handshake.svg",
            )),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
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

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body(&srv.url("\"/homepage/assets/img/icons/handshake.svg\""));
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/README");
        then.status(200)
            .body(&srv.url("\"/homepage/assets/img/icons/handshake.svg\""));
    });

    let mock_three = srv.mock(|when, then| {
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
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            // .count(1) asserts that we only see the endpoint reported once, even though there
            // is the potential to request the same url twice
            .and(predicate::str::contains("/homepage/assets/img/icons/handshake.svg").count(1)),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    assert!(mock_three.hits() <= 2);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// send a request to a page that contains an absolute link that leads to a page with a filter_size
/// that should filter it out, expect not to see the second response reported
fn extractor_finds_filtered_content() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "README".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body(&srv.url("\"/homepage/assets/img/icons/handshake.svg\""));
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET)
            .path("/homepage/assets/img/icons/handshake.svg");
        then.status(200).body("im a little teapot");
    });

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

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// serve a robots.txt with a file and and a folder link contained within it. ferox should
/// find both links and request each one. Additionally, a scan should start with the directory
/// link found, meaning the wordlist will be thrown at the sub directory
fn extractor_finds_robots_txt_links_and_displays_files_or_scans_directories() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("im a little teapot"); // 18
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/robots.txt");
        then.status(200).body(
            r#"
            User-agent: *
            Crawl-delay: 10
            # CSS, JS, Images
            Allow: /misc/*.css$
            Disallow: /misc/stupidfile.php
               Disallow: /disallowed-subdir/
            "#,
        );
    });

    let mock_file = srv.mock(|when, then| {
        when.method(GET).path("/misc/stupidfile.php");
        then.status(200).body("im a little teapot too"); // 22
    });

    let mock_scanned_file = srv.mock(|when, then| {
        when.method(GET).path("/misc/LICENSE");
        then.status(200).body("i too, am a container for tea"); // 29
    });

    let mock_dir = srv.mock(|when, _| {
        when.method(GET).path("/misc/");
    });

    let mock_disallowed = srv.mock(|when, then| {
        when.method(GET).path("/disallowed-subdir");
        then.status(404);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .arg("-vvvv")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE") // 2 directories contain LICENSE
            .count(2)
            .and(predicate::str::contains("18c"))
            .and(predicate::str::contains("/misc/stupidfile.php"))
            .and(predicate::str::contains("22c"))
            .and(predicate::str::contains("/misc/LICENSE"))
            .and(predicate::str::contains("29c"))
            .and(predicate::str::contains("200").count(3)),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_dir.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    assert_eq!(mock_file.hits(), 1);
    assert_eq!(mock_disallowed.hits(), 1);
    assert_eq!(mock_scanned_file.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// send a request to a page that contains a link that contains a directory that returns a 403
/// --extract-links should find the link and make recurse into the 403 directory, finding LICENSE
fn extractor_recurses_into_403_directories() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body(&srv.url("'/homepage/assets/img/icons/handshake.svg'"));
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/homepage/assets/img/icons/LICENSE");
        then.status(200).body("that's just like, your opinion man");
    });

    let forbidden_dir = srv.mock(|when, then| {
        when.method(GET).path("/homepage/assets/img/icons/");
        then.status(403);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--extract-links")
        .arg("--depth") // need to go past default 4 directories
        .arg("0")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .count(2)
            .and(predicate::str::contains("1w")) // link in /LICENSE
            .and(predicate::str::contains("34c")) // recursed LICENSE
            .and(predicate::str::contains(
                "/homepage/assets/img/icons/LICENSE",
            )),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    assert_eq!(forbidden_dir.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}
