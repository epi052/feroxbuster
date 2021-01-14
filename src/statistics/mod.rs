mod error;
mod macros;
mod container;
mod command;
mod field;
mod init;
#[cfg(test)]
mod tests;

pub use self::command::StatCommand;
pub use self::container::Stats;
pub use self::error::StatError;
pub use self::field::StatField;
pub use self::init::initialize;

#[cfg(test)]
use self::tests::{setup_stats_test, teardown_stats_test};
