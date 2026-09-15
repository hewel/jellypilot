//! Typed SDK failure surface shared by Rust and FFI consumers.

use std::fmt;

use jellypilot_auth::AuthStorageError;
use jellypilot_media_server::JellyfinError;

/// Every failure an SDK operation can report.
///
/// `Cancelled` means the caller's [`crate::OperationToken`] was cancelled
/// before the operation settled. `Stale` means the profile scope that issued
/// the operation ended before the result was committed; the server-side
/// effect of a completed mutation is not rolled back. `HandoffAborted` means
/// platform teardown declined the transition and the previous profile is
/// still active.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SdkError {
    /// Input failed validation before any request was issued.
    InvalidInput(String),
    /// Authentication or saved-session validation failed.
    Authentication(String),
    /// Protected credential or local persistence failed.
    Storage(String),
    /// An authenticated server request failed.
    Request(String),
    /// The operation's token was cancelled.
    Cancelled,
    /// The issuing profile scope ended before the result was committed.
    Stale,
    /// No profile is active.
    NoActiveProfile,
    /// The referenced saved profile does not exist.
    ProfileNotFound,
    /// Another account operation is already running.
    OperationInProgress,
    /// Platform teardown declined the profile transition.
    HandoffAborted,
    /// The SDK was closed.
    Closed,
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message)
            | Self::Authentication(message)
            | Self::Storage(message)
            | Self::Request(message) => formatter.write_str(message),
            Self::Cancelled => formatter.write_str("the operation was cancelled"),
            Self::Stale => {
                formatter.write_str("the profile scope changed before the result was committed")
            }
            Self::NoActiveProfile => formatter.write_str("no profile is active"),
            Self::ProfileNotFound => formatter.write_str("the saved profile no longer exists"),
            Self::OperationInProgress => {
                formatter.write_str("another account operation is already running")
            }
            Self::HandoffAborted => {
                formatter.write_str("platform teardown declined the profile transition")
            }
            Self::Closed => formatter.write_str("the SDK is closed"),
        }
    }
}

impl std::error::Error for SdkError {}

impl From<AuthStorageError> for SdkError {
    fn from(error: AuthStorageError) -> Self {
        match error {
            AuthStorageError::ProfileNotFound => Self::ProfileNotFound,
            other => Self::Storage(other.to_string()),
        }
    }
}

impl From<JellyfinError> for SdkError {
    fn from(error: JellyfinError) -> Self {
        Self::Request(error.to_string())
    }
}
