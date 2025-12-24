mod utils;
use assert_cmd::cargo_bin;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::MockServer;
use predicates::prelude::*;
use std::process::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// send a request to two different URLs, where both have the same word count and status code
/// the response should be unique, and not seen twice
fn word_and_status_makes_a_response_unique_and_isnt_seen() -> Result<(), Box<dyn std::error::Error>>
{
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".into(), "Other".into()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body(srv.url("this is a word count supplier"));
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/Other");
        then.status(200)
            .body(srv.url("this is a word count supplier"));
    });

    let cmd = Command::new(cargo_bin!("feroxbuster"))
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--unique")
        .arg("--threads")
        .arg("1") // to ensure sequential processing
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("/Other").not()),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// send a request to two different URLs, where both have the same content length and status code
/// is a redirection the response should be unique, and not seen twice
fn bytes_and_status_makes_a_redirect_response_unique_and_isnt_seen(
) -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".into(), "Other".into()], "wordlist")?;

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(200)
            .body(srv.url("this is a word count supplier"));
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/Other");
        then.status(301)
            .body(srv.url("this is a word count supplier")); // redirect + same body
    });

    let cmd = Command::new(cargo_bin!("feroxbuster"))
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--unique")
        .arg("--threads")
        .arg("1") // to ensure sequential processing
        .unwrap();

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("/LICENSE").and(predicate::str::contains("/Other").not()));

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
    Ok(())
}
