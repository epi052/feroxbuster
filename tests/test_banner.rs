use assert_cmd::Command;
use predicates::prelude::*;

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + proxy
fn banner_prints_proxy() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--proxy")
        .arg("http://127.0.0.1:8080")
        .assert()
        .failure()
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
                .and(predicate::str::contains("http://127.0.0.1:8080"))
                .and(predicate::str::contains("─┴─")),
        );
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + multiple headers
fn banner_prints_headers() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--headers")
        .arg("stuff:things")
        .arg("-H")
        .arg("mostuff:mothings")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + multiple size filters
fn banner_prints_size_filters() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-S")
        .arg("789456123")
        .arg("--sizefilter")
        .arg("44444444")
        .assert()
        .failure()
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
                .and(predicate::str::contains("789456123"))
                .and(predicate::str::contains("44444444"))
                .and(predicate::str::contains("─┴─")),
        );
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + queries
fn banner_prints_queries() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-Q")
        .arg("token=supersecret")
        .arg("--query")
        .arg("stuff=things")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + output file
fn banner_prints_output_file() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--output")
        .arg("/super/cool/path")
        .assert()
        .failure()
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
                .and(predicate::str::contains("─┴─")),
        );
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + insecure
fn banner_prints_insecure() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-k")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + follow redirects
fn banner_prints_redirects() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-r")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + extensions
fn banner_prints_extensions() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-x")
        .arg("js")
        .arg("--extensions")
        .arg("pdf")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + dontfilter
fn banner_prints_dontfilter() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--dontfilter")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=1
fn banner_prints_verbosity_one() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-v")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=2
fn banner_prints_verbosity_two() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-vv")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=3
fn banner_prints_verbosity_three() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-vvv")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + verbosity=4
fn banner_prints_verbosity_four() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-vvvv")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + add slash
fn banner_prints_add_slash() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-f")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + INFINITE recursion
fn banner_prints_infinite_depth() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--depth")
        .arg("0")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + recursion depth
fn banner_prints_recursion_depth() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("--depth")
        .arg("343214")
        .assert()
        .failure()
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
    Ok(())
}

#[test]
/// test allows non-existent wordlist to trigger the banner printing to stderr
/// expect to see all mandatory prints + no recursion
fn banner_prints_no_recursion() -> Result<(), Box<dyn std::error::Error>> {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://localhost")
        .arg("-n")
        .assert()
        .failure()
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
    Ok(())
}
