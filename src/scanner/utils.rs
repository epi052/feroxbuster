#[derive(Copy, Clone, PartialEq, Eq, Debug)]
/// represents different situations where different criteria can trigger auto-tune/bail behavior
pub enum PolicyTrigger {
    /// excessive 403 trigger
    Status403,

    /// excessive 429 trigger
    Status429,

    /// excessive general errors
    Errors,

    /// dummy error for upward rate adjustment
    TryAdjustUp,
}
