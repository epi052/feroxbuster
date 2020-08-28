use env_logger::Builder;
use std::io::Write;
use std::time::Instant;

/// Create an instance of an `env_logger` with an added time offset
pub fn init_logger() {
    let start = Instant::now();
    let mut builder = Builder::from_default_env();

    builder
        .format(move |buf, rec| {
            let t = start.elapsed().as_secs_f32();
            writeln!(buf, "{:.03} [{}] - {}", t, rec.level(), rec.args())
        })
        .init();
}
