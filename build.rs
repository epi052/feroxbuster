use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
extern crate clap;

use clap::Shell;

include!("src/parser.rs");

fn main() {
    println!("cargo:rerun-if-env-changed=src/parser.rs");

    if std::env::var("DOCS_RS").is_ok() {
        return; // only build when we're not generating docs
    }

    let outdir = "shell_completions";

    let mut app = initialize();

    let shells: [Shell; 4] = [Shell::Bash, Shell::Fish, Shell::Zsh, Shell::PowerShell];

    for shell in &shells {
        app.gen_completions("feroxbuster", *shell, outdir);
    }

    // 0xdf pointed out an oddity when tab-completing options that expect file paths, the fix we
    // landed on was to add -o plusdirs to the bash completion script. The following code aims to
    // automate that fix and have it present in all future builds
    let mut contents = String::new();

    let mut bash_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("{}/feroxbuster.bash", outdir))
        .expect("Couldn't open bash completion script");

    bash_file
        .read_to_string(&mut contents)
        .expect("Couldn't read bash completion script");

    contents = contents.replace("default feroxbuster", "default -o plusdirs feroxbuster");

    bash_file
        .seek(SeekFrom::Start(0))
        .expect("Couldn't seek to position 0 in bash completion script");

    bash_file
        .write_all(contents.as_bytes())
        .expect("Couldn't write updated bash completion script to disk");
}
