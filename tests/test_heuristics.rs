mod utils;
use httpmock::Method::GET;
use httpmock::{Mock, MockServer};
use assert_cmd::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
fn test_single_target_cannot_connect() -> Result<(), Box<dyn std::error::Error>> {
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()])?;

    let cmd = std::panic::catch_unwind(|| Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg("http://fjdksafjkdsajfkdsajkfdsajkfsdjkdsfdsafdsafdsajkr3l2ajfdskafdsjk")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .unwrap()
    );

    assert!(cmd.is_err());

    teardown_tmp_directory(tmp_dir);
    Ok(())
}

#[test]
fn test_two_targets_cannot_connect() -> Result<(), Box<dyn std::error::Error>> {
    let not_real = String::from("http://fjdksafjkdsajfkdsajkfdsajkfsdjkdsfdsafdsafdsajkr3l2ajfdskafdsjk");
    let urls = vec![not_real.clone(), not_real];
    let (tmp_dir, file) = setup_tmp_directory(&urls)?;

    let cmd = std::panic::catch_unwind(|| Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--stdin")
        .arg("--wordlist")
        .arg(file.as_os_str())
        .pipe_stdin(file)
        .unwrap().unwrap()
    );

    assert!(cmd.is_err());

    teardown_tmp_directory(tmp_dir);
    Ok(())
}
