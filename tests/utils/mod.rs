use std::fs::{remove_dir_all, write};
use std::path::PathBuf;
use tempfile::TempDir;

pub fn setup_tmp_directory(
    words: &[String],
) -> Result<(TempDir, PathBuf), Box<dyn std::error::Error>> {
    let tmp_dir = TempDir::new()?;
    let file = tmp_dir.path().join("wordlist");
    write(&file, words.join("\n"))?;
    Ok((tmp_dir, file))
}

pub fn teardown_tmp_directory(directory: TempDir) {
    remove_dir_all(directory).unwrap();
}
