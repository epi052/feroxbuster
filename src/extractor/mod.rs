//! extract links from html source and robots.txt
mod builder;
mod container;
#[cfg(test)]
mod tests;

pub use self::builder::ExtractionTarget;
pub use self::builder::ExtractorBuilder;
pub use self::container::Extractor;

use crate::response::FeroxResponse;
use regex::Regex;
use std::sync::Arc;
