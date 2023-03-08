//! contains all of feroxbuster's filters
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt::Debug;

use crate::response::FeroxResponse;
use crate::traits::FeroxFilter;

pub use self::container::FeroxFilters;
pub(crate) use self::empty::EmptyFilter;
pub use self::init::initialize;
pub use self::lines::LinesFilter;
pub use self::regex::RegexFilter;
pub use self::similarity::{SimilarityFilter, SIM_HASHER};
pub use self::size::SizeFilter;
pub use self::status_code::StatusCodeFilter;
pub(crate) use self::utils::{create_similarity_filter, filter_lookup};
pub use self::wildcard::WildcardFilter;
pub use self::words::WordsFilter;

mod status_code;
mod words;
mod lines;
mod size;
mod regex;
mod similarity;
mod container;
#[cfg(test)]
mod tests;
mod init;
mod utils;
mod wildcard;
mod empty;
