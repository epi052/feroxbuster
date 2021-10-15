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
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--proxy")
        .arg("http://127.0.0.1:8080")
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
        .arg(file.as_os_str())
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
                    "ERROR: Couldn't start /super/cool/path file handler",
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
/// expect to see all mandatory prints + no recursion
fn banner_prints_no_recursion() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-n")
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
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Could not connect to any target provided",
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
        .arg("--parallel")
        .arg("4316")
        .assert()
        .success()
        .stderr(
            predicate::str::contains("─┬─")
                .not()
                .and(predicate::str::contains("Target Url").not())
                .and(predicate::str::contains("Parallel Scans").not())
                .and(predicate::str::contains("Threads").not())
                .and(predicate::str::contains("Wordlist").not())
                .and(predicate::str::contains("Status Codes").not())
                .and(predicate::str::contains("Timeout (secs)").not())
                .and(predicate::str::contains("User-Agent").not()),
        );
}
