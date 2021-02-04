#[derive(Debug, Copy, Clone)]
/// Simple enum to designate whether a URL was passed in by the user (Initial) or found during
/// scanning (Latest)
pub enum ScanOrder {
    /// Url was passed in by the user
    Initial,

    /// Url was found during scanning
    Latest,
}
