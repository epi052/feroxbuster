/// Generic Result type to ease error handling in async contexts
pub type FeroxResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub mod config;
pub mod logger;
