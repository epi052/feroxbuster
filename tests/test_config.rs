mod utils;
use assert_cmd::cargo_bin;
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

    Command::new(cargo_bin!("feroxbuster"))
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

/// runs feroxbuster with the given args and a wordlist that doesn't exist, so only the banner is
/// printed; `user_config` and `cwd_config` are the contents of a ferox-config.toml in the user's
/// config directory (lower priority) and in the current directory (higher priority)
fn run_with_scope_configs(
    user_config: Option<&str>,
    cwd_config: Option<&str>,
    args: &[&str],
) -> Result<assert_cmd::assert::Assert, Box<dyn std::error::Error>> {
    let config_home = tempfile::TempDir::new()?;
    let cwd = tempfile::TempDir::new()?;

    if let Some(contents) = user_config {
        let dir = config_home.path().join("feroxbuster");
        std::fs::create_dir(&dir)?;
        std::fs::write(dir.join("ferox-config.toml"), contents)?;
    }

    if let Some(contents) = cwd_config {
        std::fs::write(cwd.path().join("ferox-config.toml"), contents)?;
    }

    let assert = Command::new(cargo_bin!("feroxbuster"))
        .current_dir(cwd.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(args)
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success();

    Ok(assert)
}

/// matches the banner entry for the given in-scope host
fn in_scope(host: &str) -> predicates::str::RegexPredicate {
    predicate::str::is_match(format!(r"(?m)In-Scope Url\s+│ {}$", regex::escape(host))).unwrap()
}

#[test]
/// config FOO + -u BAR => FOO, BAR
fn config_scope_is_kept_when_url_is_passed() -> Result<(), Box<dyn std::error::Error>> {
    run_with_scope_configs(
        None,
        Some(r#"scope = ["http://foo.com"]"#),
        &["--url", "http://localhost"],
    )?
    .stderr(in_scope("foo.com").and(in_scope("localhost")));

    Ok(())
}

#[test]
/// config FOO + -u BAR + --scope BAZ => BAR, BAZ
fn cli_scope_replaces_config_scope() -> Result<(), Box<dyn std::error::Error>> {
    run_with_scope_configs(
        None,
        Some(r#"scope = ["http://foo.com"]"#),
        &["--url", "http://localhost", "--scope", "baz.com"],
    )?
    .stderr(
        in_scope("localhost")
            .and(in_scope("baz.com"))
            .and(in_scope("foo.com").not()),
    );

    Ok(())
}

#[test]
/// user config FOO + cwd config BAR + -u BAZ => BAR, BAZ
fn higher_priority_config_scope_replaces_lower_priority_one(
) -> Result<(), Box<dyn std::error::Error>> {
    run_with_scope_configs(
        Some(r#"scope = ["http://foo.com"]"#),
        Some(r#"scope = ["http://bar.com"]"#),
        &["--url", "http://localhost"],
    )?
    .stderr(
        in_scope("bar.com")
            .and(in_scope("localhost"))
            .and(in_scope("foo.com").not()),
    );

    Ok(())
}

#[test]
/// (no config) + -u FOO + --scope BAR => FOO, BAR
fn url_and_cli_scope_without_config() -> Result<(), Box<dyn std::error::Error>> {
    run_with_scope_configs(
        None,
        None,
        &["--url", "http://localhost", "--scope", "bar.com"],
    )?
    .stderr(in_scope("localhost").and(in_scope("bar.com")));

    Ok(())
}

#[test]
/// scope entries without a scheme in a config file are accepted, like --scope on the cli
fn config_scope_accepts_domains_without_scheme() -> Result<(), Box<dyn std::error::Error>> {
    run_with_scope_configs(
        None,
        Some(r#"scope = ["foo.com", "bar.com"]"#),
        &["--url", "http://localhost"],
    )?
    .stderr(
        in_scope("foo.com")
            .and(in_scope("bar.com"))
            .and(in_scope("localhost")),
    );

    Ok(())
}

#[test]
/// -u http://foo.com + --scope foo.com => foo.com, once
fn scope_is_deduplicated_by_host() -> Result<(), Box<dyn std::error::Error>> {
    run_with_scope_configs(
        None,
        None,
        &["--url", "http://foo.com", "--scope", "foo.com"],
    )?
    .stderr(in_scope("foo.com").and(predicate::function(|out: &str| {
        out.matches("In-Scope Url").count() == 1
    })));

    Ok(())
}

#[test]
/// scan started with --stdin, resumed with --scope => original targets and new scope
fn resumed_scope_keeps_saved_scope_and_adds_cli_scope() -> Result<(), Box<dyn std::error::Error>> {
    let state_dir = tempfile::TempDir::new()?;
    let state_file = state_dir.path().join("ferox.state");
    std::fs::write(
        &state_file,
        r#"{"scans":[],"responses":[],"config":{"type":"configuration","stdin":true,"target_url":"","scope":["http://foo.com/","http://bar.com/"]}}"#,
    )?;

    run_with_scope_configs(
        None,
        None,
        &[
            "--resume-from",
            state_file.to_str().unwrap(),
            "--scope",
            "baz.com",
        ],
    )?
    .stderr(
        in_scope("foo.com")
            .and(in_scope("bar.com"))
            .and(in_scope("baz.com")),
    );

    Ok(())
}
