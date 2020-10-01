use crate::config::PROGRESS_PRINTER;
use console::{style, Color};
use env_logger::Builder;
use std::env;
use std::time::Instant;

/// Create a customized instance of
/// [env_logger::Logger](https://docs.rs/env_logger/latest/env_logger/struct.Logger.html)
/// with timer offset/color and set the log level based on `verbosity`
pub fn initialize(verbosity: u8) {
    // use occurrences of -v on commandline to or verbosity = N in feroxconfig.toml to set
    // log level for the application; respects already specified RUST_LOG environment variable
    match env::var("RUST_LOG") {
        Ok(_) => {} // RUST_LOG found, don't override
        Err(_) => {
            // only set log level based on verbosity when RUST_LOG variable doesn't exist
            match verbosity {
                0 => (),
                1 => env::set_var("RUST_LOG", "warn"),
                2 => env::set_var("RUST_LOG", "info"),
                3 => env::set_var("RUST_LOG", "debug,hyper=info,reqwest=info"),
                _ => env::set_var("RUST_LOG", "trace,hyper=info,reqwest=info"),
            }
        }
    }

    let start = Instant::now();
    let mut builder = Builder::from_default_env();

    builder
        .format(move |_, record| {
            let t = start.elapsed().as_secs_f32();
            let level = record.level();

            let (level_name, level_color) = match level {
                log::Level::Error => ("ERR", Color::Red),
                log::Level::Warn => ("WRN", Color::Red),
                log::Level::Info => ("INF", Color::Cyan),
                log::Level::Debug => ("DBG", Color::Yellow),
                log::Level::Trace => ("TRC", Color::Magenta),
            };

            let msg = format!(
                "{} {:10.03} {}",
                style(format!("{}", level_name)).bg(level_color).black(),
                style(t).dim(),
                style(record.args()).dim(),
            );

            PROGRESS_PRINTER.println(msg);
            Ok(())
        })
        .init();
}
