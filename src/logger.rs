use crate::{
    config::{CONFIGURATION, PROGRESS_PRINTER},
    reporter::safe_file_write,
    utils::open_file,
    FeroxMessage, FeroxSerialize,
};
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

    let debug_file = open_file(&CONFIGURATION.debug_log);

    if let Some(buffered_file) = debug_file.clone() {
        // write out the configuration to the debug file if it exists
        safe_file_write(&*CONFIGURATION, buffered_file, CONFIGURATION.json);
    }

    builder
        .format(move |_, record| {
            let log_entry = FeroxMessage {
                message: record.args().to_string(),
                level: record.level().to_string(),
                time_offset: start.elapsed().as_secs_f32(),
                module: record.target().to_string(),
                kind: "log".to_string(),
            };

            PROGRESS_PRINTER.println(&log_entry.as_str());

            if let Some(buffered_file) = debug_file.clone() {
                safe_file_write(&log_entry, buffered_file, CONFIGURATION.json);
            }

            Ok(())
        })
        .init();
}
