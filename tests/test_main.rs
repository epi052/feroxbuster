pub mod utils;
use assert_cmd::Command;
use httpmock::Method::GET;
use httpmock::{Mock, MockServer};
use predicates::prelude::*;

#[test]
/// send the function a file to which we dont have permission in order to execute error branch
fn main_use_root_owned_file_as_wordlist() -> Result<(), Box<dyn std::error::Error>> {
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
        .stderr(predicate::str::contains(
            "ERROR main::get_unique_words_from_wordlist Permission denied (os error 13)",
        ));

    // connectivity test hits it once
    assert_eq!(mock.times_called(), 1);
    Ok(())
}
