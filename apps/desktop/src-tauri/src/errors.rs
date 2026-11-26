use er_optimizer_core::SearchError;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
pub struct AppError {
    pub message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// A rejection inside a job's work fails the job; it is never a cancellation.
impl From<AppError> for SearchError {
    fn from(error: AppError) -> Self {
        Self::Failed(error.message)
    }
}

/// How a job's work ended. Every finished payload is built from one, so a payload is
/// never both cancelled and failed.
#[derive(Debug)]
pub enum JobOutcome<T> {
    Completed(T),
    Cancelled,
    Failed(String),
}

impl<T> JobOutcome<T> {
    /// The outcome of a worker's result. A result completed after cancellation was
    /// requested is discarded.
    pub fn new(result: Result<T, SearchError>, cancel_requested: bool) -> Self {
        match result {
            Ok(_) if cancel_requested => Self::Cancelled,
            Ok(value) => Self::Completed(value),
            Err(SearchError::Cancelled) => Self::Cancelled,
            Err(SearchError::Failed(message)) => Self::Failed(message),
        }
    }

    /// The payload, the cancellation flag and the error the wire carries.
    pub(crate) fn into_parts(self) -> (Option<T>, bool, Option<String>) {
        match self {
            Self::Completed(value) => (Some(value), false, None),
            Self::Cancelled => (None, true, None),
            Self::Failed(message) => (None, false, Some(message)),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}
