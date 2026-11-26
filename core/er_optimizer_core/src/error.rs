use std::fmt;

/// Why a cancellable search or evaluation returned no result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchError {
    /// The caller's `should_continue` check returned false.
    Cancelled,
    /// The work failed; the message names the cause.
    Failed(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<String> for SearchError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for SearchError {
    fn from(message: &str) -> Self {
        Self::Failed(message.to_string())
    }
}
