use std::fs::{copy, create_dir_all, OpenOptions};
use std::io::{Read, Seek, Write};

use clap_complete::{generate_to, shells};

include!("src/parser.rs");

fn main() {
    println!("cargo:rerun-if-env-changed=src/parser.rs");

    if std::env::var("DOCS_RS").is_ok() {
        return; // only build when we're not generating docs
    }

    let outdir = "shell_completions";

    let mut app = initialize();

    generate_to(shells::Bash, &mut app, "feroxbuster", outdir).unwrap();
    generate_to(shells::Zsh, &mut app, "feroxbuster", outdir).unwrap();
    generate_to(shells::Zsh, &mut app, "feroxbuster", outdir).unwrap();
    generate_to(shells::PowerShell, &mut app, "feroxbuster", outdir).unwrap();
    generate_to(shells::Elvish, &mut app, "feroxbuster", outdir).unwrap();

    // 0xdf pointed out an oddity when tab-completing options that expect file paths, the fix we
    // landed on was to add -o plusdirs to the bash completion script. The following code aims to
    // automate that fix and have it present in all future builds
    let mut contents = String::new();

    let mut bash_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!("{outdir}/feroxbuster.bash"))
        .expect("Couldn't open bash completion script");

    bash_file
        .read_to_string(&mut contents)
        .expect("Couldn't read bash completion script");

    contents = contents.replace("default feroxbuster", "default -o plusdirs feroxbuster");

    bash_file
        .rewind()
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
    let mut config_dir = dirs::config_dir().expect("Couldn't resolve user's config directory");
    config_dir = config_dir.join("feroxbuster"); // $HOME/.config/feroxbuster

    if !config_dir.exists() {
        // recursively create the feroxbuster directory and all of its parent components if
        // they are missing
        if create_dir_all(&config_dir).is_err() {
            // only copy the config file when we're not running in the CI/CD pipeline
            // which fails with permission denied
            eprintln!("Couldn't create one or more directories needed to copy the config file");
            return;
        }
    }

    // hard-coding config name here to not rely on the crate we're building, if DEFAULT_CONFIG_NAME
    // ever changes, this will need to be updated
    let config_file = config_dir.join("ferox-config.toml");

    if !config_file.exists() {
        // config file doesn't exist, add it to the config directory
        if copy("ferox-config.toml.example", config_file).is_err() {
            eprintln!("Couldn't copy example config into config directory");
        }
    }
}
