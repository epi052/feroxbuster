mod utils;
use assert_cmd::prelude::*;
use httpmock::MockServer;
use predicates::prelude::*;
use std::process::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// send a single valid request, expect a 200 response
fn read_in_config_file_for_settings() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();

    let (tmp_dir, file) = setup_tmp_directory(&["threads = 37".to_string()], "ferox-config.toml")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .current_dir(&tmp_dir)
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .assert()
        .success()
        .stderr(predicate::str::contains("│ 37"));

    teardown_tmp_directory(tmp_dir);

    Ok(())
}
