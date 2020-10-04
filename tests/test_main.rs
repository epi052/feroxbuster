mod utils;
use httpmock::Method::GET;
use httpmock::{MockServer, Mock};
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
/// send the function a directory to execute error branch
fn main_use_directory_as_wordlist() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();

    let mock = Mock::new()
        .expect_method(GET)
        .expect_path("/")
        .return_status(200)
        .return_body("this is a test")
        .create_on(&srv);

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg("/etc/shadow")
        .arg("-vvvv")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("ERROR main::get_unique_words_from_wordlist Permission denied (os error 13)")
        );

    // connectivity test hits it once
    assert_eq!(mock.times_called(), 1);
    Ok(())
}