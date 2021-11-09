//! collection of all traits used
use crate::response::FeroxResponse;
use anyhow::Result;
use serde::Serialize;
use std::any::Any;
use std::fmt::Debug;

// references:
//   https://dev.to/magnusstrale/rust-trait-objects-in-a-vector-non-trivial-4co5
//   https://stackoverflow.com/questions/25339603/how-to-test-for-equality-between-trait-objects

/// FeroxFilter trait; represents different types of possible filters that can be applied to
/// responses
pub trait FeroxFilter: Debug + Send + Sync {
    /// Determine whether or not this particular filter should be applied or not
    fn should_filter_response(&self, response: &FeroxResponse) -> bool;

    /// delegates to the FeroxFilter-implementing type which gives us the actual type of self
    fn box_eq(&self, other: &dyn Any) -> bool;

    /// gives us `other` as Any in box_eq
    fn as_any(&self) -> &dyn Any;
}

/// implementation of PartialEq, necessary long-form due to "trait cannot be made into an object"
/// error when attempting to derive PartialEq on the trait itself
impl PartialEq for Box<dyn FeroxFilter> {
    /// Perform a comparison of two implementors of the FeroxFilter trait
    fn eq(&self, other: &Box<dyn FeroxFilter>) -> bool {
        self.box_eq(other.as_any())
    }
}

/// FeroxSerialize trait; represents different types that are Serialize and also implement
/// as_str / as_json methods
pub trait FeroxSerialize: Serialize {
    /// Return a String representation of the object, generally the human readable version of the
    /// implementor
    fn as_str(&self) -> String;

    /// Return an NDJSON representation of the object
    fn as_json(&self) -> Result<String>;

    /// Return a CSV representation of the object
    fn as_csv(&self) -> String {
        String::new()
    }
}
