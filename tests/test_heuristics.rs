mod utils;
use assert_cmd::prelude::*;
use assert_cmd::Command;
use httpmock::Method::GET;
use httpmock::{Mock, MockServer, Regex};
use predicates::prelude::*;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// test passes one bad target via -u to the scanner, expected result is that the
/// scanner dies
fn test_single_target_cannot_connect() -> Result<(), Box<dyn std::error::Error>> {
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://fjdksafjkdsajfkdsajkfdsajkfsdjkdsfdsafdsafdsajkr3l2ajfdskafdsjk")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Could not connect to http://fjdksafjkdsajfkdsajkfdsajkfsdjkdsfdsafdsafdsajkr3l2ajfdskafdsjk, skipping...", )
        );

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test pipes two bad targets to the scanner, expected result is that the
/// scanner dies
fn test_two_targets_cannot_connect() -> Result<(), Box<dyn std::error::Error>> {
    let not_real =
        String::from("http://fjdksafjkdsajfkdsajkfdsajkfsdjkdsfdsafdsafdsajkr3l2ajfdskafdsjk");
    let urls = vec![not_real.clone(), not_real];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--stdin")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .pipe_stdin(file)
        .unwrap()
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Could not connect to http://fjdksafjkdsajfkdsajkfdsajkfsdjkdsfdsafdsafdsajkr3l2ajfdskafdsjk, skipping...", )
        );

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test pipes one good target and one bad to the scanner, expected result is that the
/// good target is scanned successfully while the bad target is ignored and handled properly
fn test_one_good_and_one_bad_target_scan_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();

    let not_real =
        String::from("http://fjdksafjkdsajfkdsajkfdsajkfsdjkdsfdsafdsafdsajkr3l2ajfdskafdsjk");
    let urls = vec![not_real, srv.url("/"), String::from("LICENSE")];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/LICENSE")
        .return_status(200)
        .return_body("this is a test")
        .create_on(&srv);

    let mut cmd = Command::cargo_bin("feroxbuster").unwrap();

    cmd.arg("--stdin")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .pipe_stdin(file)
        .unwrap()
        .assert()
        .success()
        .stdout(
            predicate::str::contains("/LICENSE")
                .and(predicate::str::contains("200"))
                .and(predicate::str::contains("14")),
        );
    assert_eq!(mock.times_called(), 1);

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test finds a static wildcard and reports as much to stdout
fn test_static_wildcard_request_found() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap())
        .return_status(200)
        .return_body("this is a test")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--add-slash")
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    cmd.assert().success().stdout(
        predicate::str::contains("WLD")
            .and(predicate::str::contains("Got"))
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("(url length: 32)")),
    );

    assert_eq!(mock.times_called(), 1);
    Ok(())
}

#[test]
/// test finds a dynamic wildcard and reports as much to stdout and a file
fn test_dynamic_wildcard_request_found() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();
    let outfile = tmp_dir.path().join("outfile");

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let mock2 = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--add-slash")
        .arg("--output")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile).unwrap();

    teardown_tmp_directory(tmp_dir);

    assert_eq!(contents.contains("WLD"), true);
    assert_eq!(contents.contains("Got"), true);
    assert_eq!(contents.contains("200"), true);
    assert_eq!(contents.contains("(url length: 32)"), true);
    assert_eq!(contents.contains("(url length: 96)"), true);

    cmd.assert().success().stdout(
        predicate::str::contains("WLD")
            .and(predicate::str::contains("Got"))
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("(url length: 32)"))
            .and(predicate::str::contains("(url length: 96)")),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock2.times_called(), 1);
}

#[test]
/// uses dont_filter, so the normal wildcard test should never happen
fn heuristics_static_wildcard_request_with_dont_filter() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap())
        .return_status(200)
        .return_body("this is a test")
        .create_on(&srv);

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--dont-filter")
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    assert_eq!(mock.times_called(), 0);
    Ok(())
}

#[test]
/// test finds a static wildcard and reports as much to stdout
fn heuristics_wildcard_test_with_two_static_wildcards() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let mock2 = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--add-slash")
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    cmd.assert().success().stdout(
        predicate::str::contains("WLD")
            .and(predicate::str::contains("Got"))
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("(url length: 32)"))
            .and(predicate::str::contains("(url length: 96)"))
            .and(predicate::str::contains(
                "Wildcard response is static; auto-filtering 46",
            )),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock2.times_called(), 1);
}

#[test]
/// test finds a static wildcard and reports nothing to stdout
fn heuristics_wildcard_test_with_two_static_wildcards_with_quiet_enabled(
) -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let mock2 = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--add-slash")
        .arg("-q")
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    cmd.assert().success().stdout(predicate::str::is_empty());

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock2.times_called(), 1);
    Ok(())
}

#[test]
/// test finds a static wildcard and reports as much to stdout and a file
fn heuristics_wildcard_test_with_two_static_wildcards_and_output_to_file() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();
    let outfile = tmp_dir.path().join("outfile");

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let mock2 = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap())
        .return_status(200)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--add-slash")
        .arg("--output")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile).unwrap();

    teardown_tmp_directory(tmp_dir);

    assert_eq!(contents.contains("WLD"), true);
    assert_eq!(contents.contains("Got"), true);
    assert_eq!(contents.contains("200"), true);
    assert_eq!(contents.contains("(url length: 32)"), true);
    assert_eq!(contents.contains("(url length: 96)"), true);

    cmd.assert().success().stdout(
        predicate::str::contains("WLD")
            .and(predicate::str::contains("Got"))
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("(url length: 32)"))
            .and(predicate::str::contains("(url length: 96)"))
            .and(predicate::str::contains(
                "Wildcard response is static; auto-filtering 46",
            )),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock2.times_called(), 1);
}

#[test]
/// test finds a static wildcard that returns 3xx, expect redirects to => in response as well as
/// in the output file
fn heuristics_wildcard_test_with_redirect_as_response_code(
) -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;
    let outfile = tmp_dir.path().join("outfile");

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap())
        .return_status(301)
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let mock2 = Mock::new()
        .expect_method(GET)
        .expect_path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap())
        .return_status(301)
        .return_header("Location", &srv.url("/some-redirect"))
        .return_body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .create_on(&srv);

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--add-slash")
        .arg("--output")
        .arg(outfile.as_os_str())
        .unwrap();

    let contents = std::fs::read_to_string(outfile).unwrap();

    teardown_tmp_directory(tmp_dir);

    assert_eq!(contents.contains("WLD"), true);
    assert_eq!(contents.contains("301"), true);
    assert_eq!(contents.contains("/some-redirect"), true);
    assert_eq!(contents.contains("redirects to => "), true);
    assert_eq!(contents.contains(&srv.url("/")), true);
    assert_eq!(contents.contains("(url length: 32)"), true);

    cmd.assert().success().stdout(
        predicate::str::contains("redirects to => ")
            .and(predicate::str::contains("/some-redirect"))
            .and(predicate::str::contains("301"))
            .and(predicate::str::contains(srv.url("/")))
            .and(predicate::str::contains("(url length: 32)"))
            .and(predicate::str::contains("WLD")),
    );

    assert_eq!(mock.times_called(), 1);
    assert_eq!(mock2.times_called(), 1);
    Ok(())
}
