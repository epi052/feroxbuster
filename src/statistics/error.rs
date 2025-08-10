#[derive(Debug, Copy, Clone)]
/// Enum variants used to inform the `StatCommand` protocol what `Stats` fields should be updated
pub enum StatError {
    /// Represents a timeout error
    Timeout,

    /// Represents a URL formatting error
    UrlFormat,

    /// Represents an error encountered during redirection
    Redirection,

    /// Represents an error encountered during connection
    Connection,

    /// Represents an error resulting from the client's request
    Request,

    /// Represents certificate-related errors (TLS/SSL)
    Certificate,

    /// Represents any other error not explicitly defined above
    Other,
}
