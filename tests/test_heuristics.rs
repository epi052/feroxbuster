mod utils;
use assert_cmd::prelude::*;
use assert_cmd::Command;
use httpmock::Method::GET;
use httpmock::{MockServer, Regex};
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

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

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
    assert_eq!(mock.hits(), 1);

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test passes one target with SSL issues via -u to the scanner, expected result is that the
/// scanner dies and prints an SSL specific error message
fn test_single_target_cannot_connect_due_to_ssl_errors() -> Result<(), Box<dyn std::error::Error>> {
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("https://expired.badssl.com")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Could not connect to https://expired.badssl.com due to SSL errors (run with -k to ignore), skipping...", )
        );

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test pipes two good targets to the scanner, expected result is that both targets
/// are scanned successfully and no error is reported (result of issue #169)
fn test_two_good_targets_scan_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let srv2 = MockServer::start();

    let urls = vec![srv.url("/"), srv2.url("/"), String::from("LICENSE")];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let mock2 = srv2.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(403).body("this also is a test");
    });

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
                .and(predicate::str::contains("403"))
                .and(predicate::str::contains("14c"))
                .and(predicate::str::contains("19c")),
        );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock2.hits(), 1);

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test finds a static wildcard and reports as much to stdout
fn test_static_wildcard_request_found() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap());
        then.status(200).body("this is a test");
    });

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

    assert_eq!(mock.hits(), 1);
    Ok(())
}

#[test]
/// test finds a dynamic wildcard and reports as much to stdout and a file
fn test_dynamic_wildcard_request_found() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();
    let outfile = tmp_dir.path().join("outfile");

    let mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap());
        then.status(200)
            .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    });

    let mock2 = srv.mock(|when, then| {
        when.method(GET).path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap());
        then.status(200).body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    });

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

    assert!(contents.contains("WLD"));
    assert!(contents.contains("Got"));
    assert!(contents.contains("200"));
    assert!(contents.contains("(url length: 32)"));
    assert!(contents.contains("(url length: 96)"));

    cmd.assert().success().stdout(
        predicate::str::contains("WLD")
            .and(predicate::str::contains("Got"))
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("(url length: 32)"))
            .and(predicate::str::contains("(url length: 96)")),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock2.hits(), 1);
}

#[test]
/// uses dont_filter, so the normal wildcard test should never happen
fn heuristics_static_wildcard_request_with_dont_filter() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap());
        then.status(200).body("this is a test");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--dont-filter")
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    assert_eq!(mock.hits(), 0);
    Ok(())
}

// #[test]
// /// test finds a static wildcard and reports as much to stdout
// fn heuristics_wildcard_test_with_two_static_wildcards() {
//     let srv = MockServer::start();
//     let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

//     let mock = srv.mock(|when, then| {
//         when.method(GET)
//             .path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap());
//         then.status(200)
//             .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
//     });

//     let mock2 = srv.mock(|when, then| {
//         when.method(GET)
//             .path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap());
//         then.status(200)
//             .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
//     });

//     let cmd = Command::cargo_bin("feroxbuster")
//         .unwrap()
//         .arg("--url")
//         .arg(srv.url("/"))
//         .arg("--wordlist")
//         .arg(file.as_os_str())
//         .arg("--add-slash")
//         .arg("--threads")
//         .arg("1")
//         .unwrap();

//     teardown_tmp_directory(tmp_dir);

//     cmd.assert().success().stdout(
//         predicate::str::contains("WLD")
//             .and(predicate::str::contains("Got"))
//             .and(predicate::str::contains("200"))
//             .and(predicate::str::contains("(url length: 32)"))
//             .and(predicate::str::contains("(url length: 96)"))
//             .and(predicate::str::contains(
//                 "Wildcard response is static; auto-filtering 46",
//             )),
//     );

//     assert_eq!(mock.hits(), 1);
//     assert_eq!(mock2.hits(), 1);
// }

#[test]
/// test finds a static wildcard and reports nothing to stdout
fn heuristics_wildcard_test_with_two_static_wildcards_with_silent_enabled(
) -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap());
        then.status(200)
            .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    });

    let mock2 = srv.mock(|when, then| {
        when.method(GET)
            .path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap());
        then.status(200)
            .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--add-slash")
        .arg("--silent")
        .arg("--threads")
        .arg("1")
        .unwrap();

    teardown_tmp_directory(tmp_dir);

    cmd.assert().success().stdout(predicate::str::is_empty());

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock2.hits(), 1);
    Ok(())
}

// #[test]
// /// test finds a static wildcard and reports as much to stdout and a file
// fn heuristics_wildcard_test_with_two_static_wildcards_and_output_to_file() {
//     let srv = MockServer::start();
//     let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();
//     let outfile = tmp_dir.path().join("outfile");

//     let mock = srv.mock(|when, then| {
//         when.method(GET)
//             .path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap());
//         then.status(200)
//             .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
//     });

//     let mock2 = srv.mock(|when, then| {
//         when.method(GET)
//             .path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap());
//         then.status(200)
//             .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
//     });

//     let cmd = Command::cargo_bin("feroxbuster")
//         .unwrap()
//         .arg("--url")
//         .arg(srv.url("/"))
//         .arg("--wordlist")
//         .arg(file.as_os_str())
//         .arg("--add-slash")
//         .arg("--output")
//         .arg(outfile.as_os_str())
//         .arg("--threads")
//         .arg("1")
//         .unwrap();

//     let contents = std::fs::read_to_string(outfile).unwrap();

//     teardown_tmp_directory(tmp_dir);

//     assert!(contents.contains("WLD"));
//     assert!(contents.contains("Got"));
//     assert!(contents.contains("200"));
//     assert!(contents.contains("(url length: 32)"));
//     assert!(contents.contains("(url length: 96)"));

//     cmd.assert().success().stdout(
//         predicate::str::contains("WLD")
//             .and(predicate::str::contains("Got"))
//             .and(predicate::str::contains("200"))
//             .and(predicate::str::contains("(url length: 32)"))
//             .and(predicate::str::contains("(url length: 96)"))
//             .and(predicate::str::contains(
//                 "Wildcard response is static; auto-filtering 46",
//             )),
//     );

//     assert_eq!(mock.hits(), 1);
//     assert_eq!(mock2.hits(), 1);
// }

// #[test]
// /// test finds a static wildcard that returns 3xx, expect redirects to => in response as well as
// /// in the output file
// fn heuristics_wildcard_test_with_redirect_as_response_code(
// ) -> Result<(), Box<dyn std::error::Error>> {
//     let srv = MockServer::start();

//     let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist")?;
//     let outfile = tmp_dir.path().join("outfile");

//     let mock = srv.mock(|when, then| {
//         when.method(GET)
//             .path_matches(Regex::new("/[a-zA-Z0-9]{32}/").unwrap());
//         then.status(301)
//             .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
//     });

//     let mock2 = srv.mock(|when, then| {
//         when.method(GET)
//             .path_matches(Regex::new("/[a-zA-Z0-9]{96}/").unwrap());
//         then.status(301)
//             .header("Location", &srv.url("/some-redirect"))
//             .body("this is a testAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
//     });

//     let cmd = Command::cargo_bin("feroxbuster")
//         .unwrap()
//         .arg("--url")
//         .arg(srv.url("/"))
//         .arg("--wordlist")
//         .arg(file.as_os_str())
//         .arg("--add-slash")
//         .arg("--output")
//         .arg(outfile.as_os_str())
//         .arg("--threads")
//         .arg("1")
//         .unwrap();

//     let contents = std::fs::read_to_string(outfile).unwrap();

//     teardown_tmp_directory(tmp_dir);

//     assert!(contents.contains("WLD"));
//     assert!(contents.contains("301"));
//     assert!(contents.contains("/some-redirect"));
//     assert!(contents.contains(" => "));
//     assert!(contents.contains(&srv.url("/")));
//     assert!(contents.contains("(url length: 32)"));

//     cmd.assert().success().stdout(
//         predicate::str::contains(" => ")
//             .and(predicate::str::contains("/some-redirect"))
//             .and(predicate::str::contains("301"))
//             .and(predicate::str::contains(srv.url("/")))
//             .and(predicate::str::contains("(url length: 32)"))
//             .and(predicate::str::contains("WLD")),
//     );

//     assert_eq!(mock.hits(), 1);
//     assert_eq!(mock2.hits(), 1);
//     Ok(())
// }

// todo figure out why ci hates these tests
