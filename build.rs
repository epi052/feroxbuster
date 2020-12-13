extern crate clap;

use clap::Shell;

include!("src/parser.rs");

fn main() {
    println!("cargo:rerun-if-env-changed=src/parser.rs");

    if let Ok(_) = std::env::var("DOCS_RS") {
        return; // only build when we're not generating docs
    }

    let outdir = "shell_completions";

    let mut app = initialize();

    let shells: [Shell; 4] = [Shell::Bash, Shell::Fish, Shell::Zsh, Shell::PowerShell];

    for shell in &shells {
        app.gen_completions("feroxbuster", *shell, outdir);
    }
}
