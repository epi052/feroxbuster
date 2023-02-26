//! small stand-alone tf-idf library, specifically designed for use in feroxbuster

mod constants;
mod document;
mod model;
mod term;
mod utils;

pub(crate) use self::document::Document;
pub(crate) use self::model::TfIdf;
pub(crate) use self::utils::preprocess;
