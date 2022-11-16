use std::fs::{remove_dir_all, write};
use std::path::PathBuf;
use tempfile::TempDir;

/// integration test helper: creates a temp directory, and writes `words` to
/// a file named `filename` in the temp directory
pub fn setup_tmp_directory(
    words: &[String],
    filename: &str,
) -> Result<(TempDir, PathBuf), Box<dyn std::error::Error>> {
    let tmp_dir = TempDir::new()?;
    let file = tmp_dir.path().join(filename);
    write(&file, words.join("\n"))?;
    Ok((tmp_dir, file))
}

/// integration test helper: removes a temporary directory, presumably created with
/// [setup_tmp_directory](fn.setup_tmp_directory.html)
pub fn teardown_tmp_directory(directory: TempDir) {
    remove_dir_all(directory).unwrap();
}
