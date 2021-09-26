use std::fs::{copy, create_dir_all, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
extern crate clap;
extern crate dirs;

use clap::Shell;

include!("src/parser.rs");

/// this code is taken from
/// https://play.rust-lang.org/?version=stable&mode=debug&edition=2018&gist=4734c208edf7da625588fa16b5e2ed93
/// which was linked in the random user agent issue discussion
///
/// ** I'm not advocating for the use of this code, just using it for demonstration purposes **
use rand::prelude::*;
use std::fmt::Write as fmt_Write;
use std::fs::write;
const WINDOWS_VERSION: [&str; 11] = [
    "3.1", "3.5", "4.0", "5.0", "5.1", "5.2", "6.0", "6.1", "6.2", "6.3", "10.0",
];
fn generate_randomua() -> String {
    let mut rng = thread_rng();
    let gecko_version = format!("{:.1}", rng.gen_range(40.0..60.0));
    let platform = if (rng.gen::<u8>() % 2) == 0 {
        let version = WINDOWS_VERSION.choose(&mut rng).unwrap();
        format!(
            "(Windows NT {}; rv:{}) Gecko/20100101 Firefox/{}",
            version, gecko_version, gecko_version
        )
    } else {
        let version = format!("10.{}", rng.gen_range(0..19));
        format!(
            "(Macintosh; Intel Mac OS X {}; rv:{}) Gecko/20100101 Firefox/{}",
            version, gecko_version, gecko_version
        )
    };
    format!("Mozilla/5.0 {}", platform)
}

fn main() {
    println!("cargo:rerun-if-env-changed=src/parser.rs");

    if env::var("DOCS_RS").is_ok() {
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
    let mut config_dir = dirs::config_dir().expect("Couldn't resolve user's config directory");
    config_dir = config_dir.join("feroxbuster"); // $HOME/.config/feroxbuster

    if !config_dir.exists() {
        // recursively create the feroxbuster directory and all of its parent components if
        // they are missing
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

    let user_agents = (0..20)
        .map(|_| generate_randomua())
        .collect::<Vec<String>>();

    let mut definition = String::from("pub static USER_AGENTS:[&'static str; ");

    definition.push_str(user_agents.len().to_string().as_str());
    definition.push_str("] = [");
    for user_agent in user_agents {
        definition.push_str(&format!("\"{}\",", user_agent));
    }
    definition.push_str("];");

    let out_file = format!("{}/user-agents.rs", env::var("OUT_DIR").unwrap());
    write(out_file, definition);
}
