mod utils;
use assert_cmd::Command;
use predicates::prelude::*;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + proxy
fn banner_prints_proxy() -> Result<(), Box<dyn std::error::Error>> {
    let urls = vec![
        String::from("http://localhost"),
        String::from("http://schmocalhost"),
    ];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--stdin")
        .arg("--proxy")
        .arg("http://127.0.0.1:8080")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .pipe_stdin(file)
        .unwrap()
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("http://schmocalhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Proxy"))
                .and(predicate::str::contains("http://127.0.0.1:8080"))
                .and(predicate::str::contains("─┴─")),
        );

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + replay proxy
fn banner_prints_replay_proxy() -> Result<(), Box<dyn std::error::Error>> {
    let urls = vec![
        String::from("http://localhost"),
        String::from("http://schmocalhost"),
    ];
    let (tmp_dir, file) = setup_tmp_directory(&urls, "wordlist")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--stdin")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .arg("--replay-proxy")
        .arg("http://127.0.0.1:8081")
        .pipe_stdin(file)
        .unwrap()
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("http://schmocalhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Replay Proxy"))
                .and(predicate::str::contains("http://127.0.0.1:8081"))
                .and(predicate::str::contains("─┴─")),
        );

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + multiple headers
fn banner_prints_headers() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--headers")
        .arg("stuff:things")
        .arg("-H")
        .arg("mostuff:mothings")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Header"))
                .and(predicate::str::contains("stuff: things"))
                .and(predicate::str::contains("mostuff: mothings"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + multiple dont scan url & regex entries
fn banner_prints_denied_urls() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--dont-scan")
        .arg("http://dont-scan.me")
        .arg("https://also-not.me")
        .arg("https:")
        .arg("/deny.*")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Don't Scan Url"))
                .and(predicate::str::contains("Don't Scan Regex"))
                .and(predicate::str::contains("http://dont-scan.me"))
                .and(predicate::str::contains("https://also-not.me"))
                .and(predicate::str::contains("https:"))
                .and(predicate::str::contains("/deny.*"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + multiple headers
fn banner_prints_random_agent() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--random-agent")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Random"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + multiple size filters
fn banner_prints_filter_sizes() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-S")
        .arg("789456123")
        .arg("--filter-size")
        .arg("44444444")
        .arg("-N")
        .arg("678")
        .arg("--filter-lines")
        .arg("679")
        .arg("-W")
        .arg("93")
        .arg("--filter-words")
        .arg("94")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Size Filter"))
                .and(predicate::str::contains("Word Count Filter"))
                .and(predicate::str::contains("Line Count Filter"))
                .and(predicate::str::contains("789456123"))
                .and(predicate::str::contains("44444444"))
                .and(predicate::str::contains("93"))
                .and(predicate::str::contains("94"))
                .and(predicate::str::contains("678"))
                .and(predicate::str::contains("679"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + queries
fn banner_prints_queries() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-Q")
        .arg("token=supersecret")
        .arg("--query")
        .arg("stuff=things")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Query Parameter"))
                .and(predicate::str::contains("token=supersecret"))
                .and(predicate::str::contains("stuff=things"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + status codes
fn banner_prints_status_codes() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-s")
        .arg("201,301,401")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("[201, 301, 401]"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + replay codes
fn banner_prints_replay_codes() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--replay-codes")
        .arg("200,302")
        .arg("--replay-proxy")
        .arg("http://localhost:8081")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Replay Proxy"))
                .and(predicate::str::contains("http://localhost:8081"))
                .and(predicate::str::contains("Replay Proxy Codes"))
                .and(predicate::str::contains("[200, 302]"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + output file
fn banner_prints_output_file() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--output")
        .arg("/super/cool/path")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Output File"))
                .and(predicate::str::contains("/super/cool/path"))
                .and(predicate::str::contains(
                    "Could not open /definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676",
                ))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + insecure
fn banner_prints_insecure() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-k")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Insecure"))
                .and(predicate::str::contains("true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + follow redirects
fn banner_prints_redirects() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-r")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Follow Redirects"))
                .and(predicate::str::contains("true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + extensions
fn banner_prints_extensions() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-x")
        .arg("js")
        .arg("--extensions")
        .arg("pdf")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Extensions"))
                .and(predicate::str::contains("[js, pdf]"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + dont_filter
fn banner_prints_dont_filter() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--dont-filter")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Filter Wildcards"))
                .and(predicate::str::contains("false"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=1
fn banner_prints_verbosity_one() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-v")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Verbosity"))
                .and(predicate::str::contains("│ 1"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=2
fn banner_prints_verbosity_two() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-vv")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Verbosity"))
                .and(predicate::str::contains("│ 2"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=3
fn banner_prints_verbosity_three() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-vvv")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Verbosity"))
                .and(predicate::str::contains("│ 3"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=4
fn banner_prints_verbosity_four() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-vvvv")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Verbosity"))
                .and(predicate::str::contains("│ 4"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + add slash
fn banner_prints_add_slash() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-f")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Add Slash"))
                .and(predicate::str::contains("true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + INFINITE recursion
fn banner_prints_infinite_depth() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--depth")
        .arg("0")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Recursion Depth"))
                .and(predicate::str::contains("INFINITE"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + recursion depth
fn banner_prints_recursion_depth() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--depth")
        .arg("343214")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Recursion Depth"))
                .and(predicate::str::contains("343214"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + server certs
fn banner_prints_server_certs() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--server-certs")
        .arg("tests/mutual-auth/certs/server/server.crt.1")
        .arg("tests/mutual-auth/certs/server/server.crt.2")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Server Certificates"))
                .and(predicate::str::contains("server.crt.1"))
                .and(predicate::str::contains("server.crt.2"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + server certs
fn banner_prints_client_cert_and_key() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--client-cert")
        .arg("tests/mutual-auth/certs/client/client.crt")
        .arg("--client-key")
        .arg("tests/mutual-auth/certs/client/client.key")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Client Certificate"))
                .and(predicate::str::contains("Client Key"))
                .and(predicate::str::contains("certs/client/client.crt"))
                .and(predicate::str::contains("certs/client/client.key"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + no recursion
fn banner_prints_no_recursion() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-n")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Do Not Recurse"))
                .and(predicate::str::contains("true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see nothing
fn banner_doesnt_print() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-q")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Could not open /definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676",
        ));
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + extract-links
fn banner_prints_extract_links() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-e")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Extract Links"))
                .and(predicate::str::contains("true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + scan-limit
fn banner_prints_scan_limit() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-L")
        .arg("4")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Concurrent Scan Limit"))
                .and(predicate::str::contains("│ 4"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + filter-status
fn banner_prints_filter_status() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-C")
        .arg("200")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Status Code Filters"))
                .and(predicate::str::contains("│ [200]"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + json
fn banner_prints_json() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--json")
        .arg("--output")
        .arg("/dev/null")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("JSON Output"))
                .and(predicate::str::contains("│ true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + json
fn banner_prints_debug_log() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--debug-log")
        .arg("/dev/null")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Debugging Log"))
                .and(predicate::str::contains("│ /dev/null"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + regex filters
fn banner_prints_filter_regex() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--filter-regex")
        .arg("^ignore me$")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Regex Filter"))
                .and(predicate::str::contains("│ ^ignore me$"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + time limit
fn banner_prints_time_limit() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--time-limit")
        .arg("10m")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Time Limit"))
                .and(predicate::str::contains("│ 10m"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + similarity filter
fn banner_prints_similarity_filter() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--filter-similar-to")
        .arg("https://somesite.com")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Similarity Filter"))
                .and(predicate::str::contains("│ https://somesite.com"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + rate limit
fn banner_prints_rate_limit() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--rate-limit")
        .arg("6735")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Requests per Second"))
                .and(predicate::str::contains("│ 6735"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + auto tune
fn banner_prints_auto_tune() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--auto-tune")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Auto Tune"))
                .and(predicate::str::contains("│ true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + auto bail
fn banner_prints_auto_bail() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--auto-bail")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Auto Bail"))
                .and(predicate::str::contains("│ true"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see no banner output
fn banner_doesnt_print_when_silent() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--silent")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .not()
                .and(predicate::str::contains("Target Url").not())
                .and(predicate::str::contains("http://localhost").not())
                .and(predicate::str::contains("Threads").not())
                .and(predicate::str::contains("Wordlist").not())
                .and(predicate::str::contains("Status Codes").not())
                .and(predicate::str::contains("Timeout (secs)").not())
                .and(predicate::str::contains("User-Agent").not()),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see no banner output
fn banner_doesnt_print_when_quiet() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--quiet")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .not()
                .and(predicate::str::contains("Target Url").not())
                .and(predicate::str::contains("http://localhost").not())
                .and(predicate::str::contains("Threads").not())
                .and(predicate::str::contains("Wordlist").not())
                .and(predicate::str::contains("Status Codes").not())
                .and(predicate::str::contains("Timeout (secs)").not())
                .and(predicate::str::contains("User-Agent").not()),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see nothing as --parallel forces --silent to be true
fn banner_prints_parallel() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--stdin")
        .arg("--quiet")
        .arg("--parallel")
        .arg("4316")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Parallel Scans"))
                .and(predicate::str::contains("4316"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + methods
fn banner_prints_methods() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-m")
        .arg("PUT")
        .arg("--methods")
        .arg("OPTIONS")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("HTTP methods"))
                .and(predicate::str::contains("[PUT, OPTIONS]"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + data body
fn banner_prints_data() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-m")
        .arg("PUT")
        .arg("--methods")
        .arg("POST")
        .arg("--data")
        .arg("some_data")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("HTTP Body"))
                .and(predicate::str::contains("some_data"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + ignored extensions
fn banner_prints_collect_extensions_and_dont_collect_default() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--collect-extensions")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Collect Extensions"))
                .and(predicate::str::contains("Ignored Extensions"))
                .and(predicate::str::contains("Images, Movies, Audio, etc..."))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + collect extensions
fn banner_prints_collect_extensions_and_dont_collect_with_input() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--collect-extensions")
        .arg("--dont-collect")
        .arg("pdf")
        .arg("xps")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Collect Extensions"))
                .and(predicate::str::contains("Ignored Extensions"))
                .and(predicate::str::contains("[pdf, xps]"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + collect backups
fn banner_prints_collect_backups() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--collect-backups")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Collect Backups"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + collect words
fn banner_prints_collect_words() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--collect-words")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Collect Words"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + collect words
fn banner_prints_all_composite_settings_smart() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--smart")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Collect Words"))
                .and(predicate::str::contains("Collect Backups"))
                .and(predicate::str::contains("Extract Links"))
                .and(predicate::str::contains("Auto Tune"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + collect words
fn banner_prints_all_composite_settings_thorough() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--thorough")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Collect Words"))
                .and(predicate::str::contains("Collect Extensions"))
                .and(predicate::str::contains("Collect Backups"))
                .and(predicate::str::contains("Extract Links"))
                .and(predicate::str::contains("Auto Tune"))
                .and(predicate::str::contains("─┴─")),
        );
}
#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + collect words
fn banner_prints_all_composite_settings_burp() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--burp")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Proxy"))
                .and(predicate::str::contains("Insecure"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + collect words
fn banner_prints_all_composite_settings_burp_replay() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--burp-replay")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Replay Proxy"))
                .and(predicate::str::contains("Insecure"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + force recursion
fn banner_prints_force_recursion() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--force-recursion")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Force Recursion"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + scan-dir-listings
fn banner_prints_scan_dir_listings() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--scan-dir-listings")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Scan Dir Listings"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + protocol
fn banner_prints_protocol() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("localhost")
        .arg("--protocol")
        .arg("http")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Default Protocol"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + protocol
fn banner_prints_limit_dirs() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("localhost")
        .arg("--limit-bars")
        .arg("3")
        .arg("--wordlist")
        .arg("/definitely/doesnt/exist/0cd7fed0-47f4-4b18-a1b0-ac39708c1676")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .and(predicate::str::contains("Target Url"))
                .and(predicate::str::contains("http://localhost"))
                .and(predicate::str::contains("Threads"))
                .and(predicate::str::contains("Wordlist"))
                .and(predicate::str::contains("Status Codes"))
                .and(predicate::str::contains("Timeout (secs)"))
                .and(predicate::str::contains("User-Agent"))
                .and(predicate::str::contains("Limit Dir Scan Bars"))
                .and(predicate::str::contains("─┴─")),
        );
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + force recursion
fn banner_prints_update_app() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--update")
        .assert()
        .success()
        .stdout(predicate::str::contains("Checking target-arch..."));
}
