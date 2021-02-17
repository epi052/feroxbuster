mod utils;
use assert_cmd::Command;
use httpmock::Method::GET;
use httpmock::MockServer;
use predicates::prelude::*;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// send the function a file to which we dont have permission in order to execute error branch
fn main_use_root_owned_file_as_wordlist() {
    let srv = MockServer::start();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200).body("this is a test");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg("/etc/shadow")
        .arg("-vvvv")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Failed while scanning: Could not open /etc/shadow",
        ));

    // connectivity test hits it once
    assert_eq!(mock.hits(), 1);
}

#[test]
/// send the function an empty file
fn main_use_empty_wordlist() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&[], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200).body("this is a test");
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Failed while scanning: Did not find any words in",
        ));

    assert_eq!(mock.hits(), 1);

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send nothing over stdin, expect heuristics to be upset during connectivity test
fn main_use_empty_stdin_targets() -> Result<(), Box<dyn std::error::Error>> {
    let (tmp_dir, file) = setup_tmp_directory(&[], "wordlist")?;

    // get_targets is called before scan, so the empty wordlist shouldn't trigger
    // the 'Did not find any words' error
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--stdin")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvv")
        .pipe_stdin(file)
        .unwrap()
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Could not connect to any target provided")
                .and(predicate::str::contains("Target Url"))
                .not(), // no target url found
        );

    teardown_tmp_directory(tmp_dir);

    Ok(())
}
