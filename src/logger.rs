use std::env;
use std::fs::OpenOptions;
use std::time::Instant;

use env_logger::Builder;

use crate::utils::write_to;
use crate::{
    config::{CONFIGURATION, PROGRESS_PRINTER},
    utils::fmt_err,
    FeroxMessage, FeroxSerialize,
};
use anyhow::{Context, Result};
use std::io::BufWriter;
use std::sync::{Arc, RwLock};

/// Create a customized instance of
/// [env_logger::Logger](https://docs.rs/env_logger/latest/env_logger/struct.Logger.html)
/// with timer offset/color and set the log level based on `verbosity`
pub fn initialize(verbosity: u8) -> Result<()> {
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

    let file = if !CONFIGURATION.debug_log.is_empty() {
        let f = OpenOptions::new() // std fs
            .create(true)
            .append(true)
            .open(&CONFIGURATION.debug_log)
            .with_context(|| fmt_err(&format!("Could not open {}", &CONFIGURATION.debug_log)))?;

        let mut writer = BufWriter::new(f);

        // write out the configuration to the debug file if it exists
        write_to(&*CONFIGURATION, &mut writer, CONFIGURATION.json)?;

        Some(Arc::new(RwLock::new(writer)))
    } else {
        None
    };

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

            if let Some(buffered_file) = file.clone() {
                if let Ok(mut unlocked) = buffered_file.write() {
                    let _ = write_to(&log_entry, &mut unlocked, CONFIGURATION.json);
                }
            }

            Ok(())
        })
        .init();

    Ok(())
}
