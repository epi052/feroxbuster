use super::*;

/// Dummy filter for internal shenanigans
#[derive(Default, Debug, PartialEq, Eq)]
pub struct EmptyFilter {}

impl FeroxFilter for EmptyFilter {
    /// `EmptyFilter` always returns false
    fn should_filter_response(&self, _response: &FeroxResponse) -> bool {
        false
    }

    /// Compare one EmptyFilter to another
    fn box_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Self>().map_or(false, |a| self == a)
    }

    /// Return self as Any for dynamic dispatch purposes
    fn as_any(&self) -> &dyn Any {
        self
    }
}
