//! extract links from html source and robots.txt
use std::sync::Arc;

use regex::Regex;

use crate::config::Configuration;
use crate::ferox_response::FeroxResponse;

pub use self::builder::ExtractionTarget;
pub use self::builder::ExtractorBuilder;
pub use self::container::Extractor;

mod builder;
mod container;
#[cfg(test)]
mod tests;
