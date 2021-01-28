mod error;
mod macros;
mod container;
mod field;
#[cfg(test)]
mod tests;

pub use self::container::Stats;
pub use self::error::StatError;
pub use self::field::StatField;

#[cfg(test)]
use self::tests::{setup_stats_test, teardown_stats_test};
