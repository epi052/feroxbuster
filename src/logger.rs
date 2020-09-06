use env_logger::Builder;
use std::env;
use std::io::Write;
use std::time::Instant;

/// Create an instance of an `env_logger` with an added time offset
pub fn initialize(verbosity: u8) {
    // use occurrences of -v on commandline to or verbosity = N in feroxconfig.toml to set
    // log level for the application; respects already specified RUST_LOG environment variable
    match verbosity {
        0 => (),
        1 => env::set_var("RUST_LOG", "warn"),
        2 => env::set_var("RUST_LOG", "info"),
        _ => env::set_var("RUST_LOG", "debug,hyper=info,reqwest=info"),
    }

    let start = Instant::now();
    let mut builder = Builder::from_default_env();

    builder
        .format(move |buf, rec| {
            let t = start.elapsed().as_secs_f32();
            writeln!(buf, "{:.03} [{}] - {}", t, rec.level(), rec.args())
        })
        .init();
}
