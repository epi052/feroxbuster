use std::fs::{copy, create_dir_all, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
extern crate clap;
extern crate dirs;

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

    // hunter0x8 let me know that when installing via cargo, it would be nice if we dropped a
    // config file during the build process. The following code will place an example config in
    // the user's configuration directory
    //   - linux: $XDG_CONFIG_HOME or $HOME/.config
    //   - macOS: $HOME/Library/Application Support
    //   - windows: {FOLDERID_RoamingAppData}
    println!(
        "did we find it? {}",
        std::env::var("IN_PIPELINE").unwrap_or(String::from("no"))
    );
    if std::env::var("IN_PIPELINE").is_ok() {
        return; // only copy the config file when we're not running in the CI/CD pipeline
    }

    let mut config_dir = dirs::config_dir().expect("Couldn't resolve user's config directory");
    config_dir = config_dir.join("feroxbuster"); // $HOME/.config/feroxbuster

    if !config_dir.exists() {
        // recursively create the feroxbuster directory and all of its parent components if
        // they are missing
        create_dir_all(&config_dir)
            .expect("Couldn't create one or more directories needed to copy the config file");
    }

    // hard-coding config name here to not rely on the crate we're building, if DEFAULT_CONFIG_NAME
    // ever changes, this will need to be updated
    let config_file = config_dir.join("ferox-config.toml");

    if !config_file.exists() {
        // config file doesn't exist, add it to the config directory
        copy("ferox-config.toml.example", config_file)
            .expect("Couldn't copy example config into config directory");
    }
}
