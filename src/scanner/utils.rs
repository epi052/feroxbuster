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

impl PolicyTrigger {
    /// get the index into the `PolicyData.errors` array for this trigger
    pub fn as_index(&self) -> usize {
        match self {
            PolicyTrigger::Status403 => 0,
            PolicyTrigger::Status429 => 1,
            PolicyTrigger::Errors => 2,
            PolicyTrigger::TryAdjustUp => {
                unreachable!("TryAdjustUp should never be used to access the errors array");
            }
        }
    }
}
