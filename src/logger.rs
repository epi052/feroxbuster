use crate::config::{CONFIGURATION, PROGRESS_PRINTER};
use crate::reporter::{get_cached_file_handle, safe_file_write};
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
                3 => env::set_var("RUST_LOG", "feroxbuster=debug,info"),
                _ => env::set_var("RUST_LOG", "feroxbuster=trace,info"),
            }
        }
    }

    let start = Instant::now();
    let mut builder = Builder::from_default_env();

    // I REALLY wanted the logger to also use the reporting channels found in the `reporter`
    // module. However, in order to properly clean up the channels, all references to the
    // transmitter side of a channel need to go out of scope, then you can await the future into
    // which the receiver was moved.
    //
    // The problem was that putting a transmitter reference in this closure, which gets initialized
    // as part of the global logger, made it so that I couldn't destroy/leak/take/swap the last
    // reference to allow the channels to gracefully close.
    //
    // The workaround was to have a RwLock around the file and allow both the logger and the
    // file handler to both write independent of each other.
    let locked_file = get_cached_file_handle(&CONFIGURATION.output);

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
                "{} {:10.03} {} {}\n",
                style(level_name).bg(level_color).black(),
                style(t).dim(),
                record.target(),
                style(record.args()).dim(),
            );
            PROGRESS_PRINTER.println(&msg);

            if let Some(buffered_file) = locked_file.clone() {
                safe_file_write(&msg, buffered_file);
            }

            Ok(())
        })
        .init();
}
