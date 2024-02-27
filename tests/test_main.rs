mod utils;
use assert_cmd::Command;
use httpmock::Method::GET;
use httpmock::{MockServer, Regex};
use predicates::prelude::*;
use std::fs::{read_dir, read_to_string};
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
        .stderr(predicate::str::contains("Could not open /etc/shadow"));

    assert_eq!(mock.hits(), 0);
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
        .stderr(predicate::str::contains("Did not find any words in"));

    assert_eq!(mock.hits(), 0);

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

#[test]
/// send three targets over stdin, expect parallel to spawn children and each child config to show
/// up in the output file
fn main_parallel_spawns_children() -> Result<(), Box<dyn std::error::Error>> {
    let t1 = MockServer::start();
    let t2 = MockServer::start();
    let t3 = MockServer::start();

    let words = [
        String::from("LICENSE"),
        String::from("stuff"),
        String::from("things"),
        String::from("mostuff"),
        String::from("mothings"),
    ];
    let (word_tmp_dir, wordlist) = setup_tmp_directory(&words, "wordlist")?;
    let (output_dir, outfile) = setup_tmp_directory(&[], "output-file")?;
    let (tgt_tmp_dir, targets) =
        setup_tmp_directory(&[t1.url("/"), t2.url("/"), t3.url("/")], "targets")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .env("RUST_LOG", "trace")
        .arg("--stdin")
        .arg("--parallel")
        .arg("2")
        .arg("--quiet")
        .arg("--debug-log")
        .arg(outfile.as_os_str())
        .arg("--wordlist")
        .arg(wordlist.as_os_str())
        .pipe_stdin(targets)
        .unwrap()
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Could not connect to any target provided")
                .and(predicate::str::contains("Target Url"))
                .not(), // no target url found
        );

    let contents = read_to_string(outfile).unwrap();
    println!("contents: {contents}");

    assert!(contents.contains("parallel branch && wrapped main")); // exits parallel branch

    // DBG      0.007 feroxbuster parallel exec: target/debug/feroxbuster
    //   --debug-log /tmp/.tmpAjRts6/output-file --wordlist /tmp/.tmpS4CKKq/wordlist
    //   --silent -u http://127.0.0.1:41979/
    let r1 = Regex::new(&format!("parallel exec:.*-u {}", t1.url("/"))).unwrap();
    let r2 = Regex::new(&format!("parallel exec:.*-u {}", t2.url("/"))).unwrap();
    let r3 = Regex::new(&format!("parallel exec:.*-u {}", t3.url("/"))).unwrap();

    assert!(r1.is_match(&contents)); // all 3 were spawned
    assert!(r2.is_match(&contents));
    assert!(r3.is_match(&contents));

    teardown_tmp_directory(word_tmp_dir);
    teardown_tmp_directory(tgt_tmp_dir);
    teardown_tmp_directory(output_dir);

    Ok(())
}

#[test]
/// send three targets over stdin with --output enabled, expect parallel to create a new directory
/// and the log files therein
fn main_parallel_creates_output_directory() -> Result<(), Box<dyn std::error::Error>> {
    let t1 = MockServer::start();
    let t2 = MockServer::start();
    let t3 = MockServer::start();

    let words = [
        String::from("LICENSE"),
        String::from("stuff"),
        String::from("things"),
        String::from("mostuff"),
        String::from("mothings"),
    ];
    let (word_tmp_dir, wordlist) = setup_tmp_directory(&words, "wordlist")?;
    let (output_dir, outfile) = setup_tmp_directory(&[], "output-file")?;
    let (tgt_tmp_dir, targets) =
        setup_tmp_directory(&[t1.url("/"), t2.url("/"), t3.url("/")], "targets")?;

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--stdin")
        .arg("--quiet")
        .arg("--parallel")
        .arg("2")
        .arg("--output")
        .arg(outfile.as_os_str())
        .arg("--wordlist")
        .arg(wordlist.as_os_str())
        .pipe_stdin(targets)
        .unwrap()
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Could not connect to any target provided")
                .and(predicate::str::contains("Target Url"))
                .not(), // no target url found
        );

    // output_dir should return something similar to output-file-1627845244.logs with the
    // line below. if it ever fails, can use the regex below to filter out the right directory
    let sub_dir = read_dir(&output_dir)?.next().unwrap()?.file_name();

    let mut num_logs = 0;
    let file_regex = Regex::new("ferox-[a-zA-Z_:0-9]+-[0-9]+.log").unwrap();
    let dir_regex = Regex::new("output-file-[0-9]+.logs").unwrap();

    let sub_dir = output_dir.as_ref().join(sub_dir);

    // created directory like output-file-1627845741.logs/
    assert!(dir_regex.is_match(&sub_dir.to_string_lossy()));

    for entry in sub_dir.read_dir()? {
        let entry = entry?;
        // created each file like ferox-https_localhost-1627845741.log
        println!("name: {:?}", entry.file_name().to_string_lossy());
        assert!(file_regex.is_match(&entry.file_name().to_string_lossy()));
        num_logs += 1;
    }

    // should be 3 log files total
    assert_eq!(num_logs, 3);

    teardown_tmp_directory(word_tmp_dir);
    teardown_tmp_directory(tgt_tmp_dir);
    teardown_tmp_directory(output_dir);

    Ok(())
}

#[test]
/// download a wordlist from a url
fn main_download_wordlist_from_url() -> Result<(), Box<dyn std::error::Error>> {
    let srv = MockServer::start();

    let (tmp_dir, _) = setup_tmp_directory(&["a".to_string()], "wordlist")?;

    let mock1 = srv.mock(|when, then| {
        when.method(GET).path("/derp");
        then.status(200).body("stuff\nthings");
    });

    // serve endpoints stuff and things
    let mock2 = srv.mock(|when, then| {
        when.method(GET).path("/stuff");
        then.status(200);
    });

    let mock3 = srv.mock(|when, then| {
        when.method(GET).path("/things");
        then.status(200);
    });

    Command::cargo_bin("feroxbuster")
        .unwrap()
        .current_dir(&tmp_dir)
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(srv.url("/derp"))
        .assert()
        .success()
        .stderr(predicate::str::contains(srv.url("/derp")));

    teardown_tmp_directory(tmp_dir);

    assert_eq!(mock1.hits(), 1); // downloaded wordlist
    assert_eq!(mock2.hits(), 1); // found stuff from wordlist
    assert_eq!(mock3.hits(), 1); // found things from wordlist

    Ok(())
}
