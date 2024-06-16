use super::*;
use ::regex::Regex;

/// Simple implementor of FeroxFilter; used to filter out responses based on a given regular
/// expression; specified using -X|--filter-regex
#[derive(Debug, Serialize, Deserialize)]
pub struct RegexFilter {
    /// Regular expression to be applied to the response body for filtering, compiled
    #[serde(with = "serde_regex")]
    pub compiled: Regex,

    /// Regular expression as passed in on the command line, not compiled
    pub raw_string: String,
}

impl Default for RegexFilter {
    fn default() -> Self {
        Self {
            compiled: Regex::new("").unwrap(),
            raw_string: String::new(),
        }
    }
}

/// implementation of FeroxFilter for RegexFilter
impl FeroxFilter for RegexFilter {
    /// Check `expression` against the response body, if the expression matches, the response
    /// should be filtered out
    fn should_filter_response(&self, response: &FeroxResponse) -> bool {
        log::trace!("enter: should_filter_response({:?} {})", self, response);

        let result = self.compiled.is_match(response.text());
        let other = response.headers().iter().any(|(k, v)| {
            self.compiled.is_match(k.as_str()) || self.compiled.is_match(v.to_str().unwrap_or(""))
        });

        log::trace!("exit: should_filter_response -> {}", result || other);

        result || other
    }

    /// Compare one SizeFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// PartialEq implementation for RegexFilter
impl PartialEq for RegexFilter {
    /// Simple comparison of the raw string passed in via the command line
    fn eq(&self, other: &RegexFilter) -> bool {
        self.raw_string == other.raw_string
    }
}
