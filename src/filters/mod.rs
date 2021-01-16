//! contains all of feroxbuster's filters
mod wildcard;
mod status_code;
mod words;
mod lines;
mod size;
mod regex;
mod similarity;
#[cfg(test)]
mod tests;
mod helpers;

pub use self::lines::LinesFilter;
pub use self::regex::RegexFilter;
pub use self::similarity::SimilarityFilter;
pub use self::size::SizeFilter;
pub use self::status_code::StatusCodeFilter;
pub use self::wildcard::WildcardFilter;
pub use self::words::WordsFilter;

use crate::{
    config::CONFIGURATION,
    traits::{FeroxFilter, FeroxSerialize},
    utils::get_url_path_length,
    FeroxResponse,
};
use std::any::Any;
use std::fmt::Debug;
