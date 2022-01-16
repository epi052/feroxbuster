use assert_cmd::Command;
use predicates::prelude::*;

#[test]
/// specify an incorrect param (-fc) with --help after it on the command line
/// old behavior printed
/// error: Found argument '-c' which wasn't expected, or isn't valid in this context
///
/// USAGE:
///     feroxbuster --add-slash --url <URL>...
///
/// For more information try --help
///
/// the new behavior we expect to see is to print the long form help message, of which
/// Ludicrous speed... go! is near the bottom of that output, so we can test for that
fn parser_incorrect_param_with_tack_tack_help() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("-fc")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Ludicrous speed... go!"));
}

#[test]
/// specify an incorrect param (-fc) with --help after it on the command line
/// old behavior printed
/// error: Found argument '-c' which wasn't expected, or isn't valid in this context
///
/// USAGE:
///     feroxbuster --add-slash --url <URL>...
///
/// For more information try --help
///
/// the new behavior we expect to see is to print the short form help message, of which
/// "[CAUTION] 4 -v's is probably too much" is near the bottom of that output, so we can test for that
fn parser_incorrect_param_with_tack_h() {
    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("-fc")
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[CAUTION] 4 -v's is probably too much",
        ));
}
