//! Typed errors crossing the FFI boundary.

use std::fmt;

/// Every failure an SDK operation can report to Kotlin.
///
/// `Cancelled` means the operation's token was cancelled before it settled.
/// `Stale` means the issuing profile scope ended before the result was
/// committed; a completed server mutation is not rolled back.
/// `HandoffAborted` means platform teardown declined the transition and the
/// previous profile is still active.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Error)]
pub enum SdkError {
    InvalidInput { reason: String },
    Authentication { reason: String },
    Storage { reason: String },
    Request { reason: String },
    Cancelled,
    Stale,
    NoActiveProfile,
    ProfileNotFound,
    OperationInProgress,
    HandoffAborted,
    Closed,
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { reason: message }
            | Self::Authentication { reason: message }
            | Self::Storage { reason: message }
            | Self::Request { reason: message } => formatter.write_str(message),
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

impl From<jellypilot_sdk::SdkError> for SdkError {
    fn from(error: jellypilot_sdk::SdkError) -> Self {
        match error {
            jellypilot_sdk::SdkError::InvalidInput(message) => {
                Self::InvalidInput { reason: message }
            }
            jellypilot_sdk::SdkError::Authentication(message) => {
                Self::Authentication { reason: message }
            }
            jellypilot_sdk::SdkError::Storage(message) => Self::Storage { reason: message },
            jellypilot_sdk::SdkError::Request(message) => Self::Request { reason: message },
            jellypilot_sdk::SdkError::Cancelled => Self::Cancelled,
            jellypilot_sdk::SdkError::Stale => Self::Stale,
            jellypilot_sdk::SdkError::NoActiveProfile => Self::NoActiveProfile,
            jellypilot_sdk::SdkError::ProfileNotFound => Self::ProfileNotFound,
            jellypilot_sdk::SdkError::OperationInProgress => Self::OperationInProgress,
            jellypilot_sdk::SdkError::HandoffAborted => Self::HandoffAborted,
            jellypilot_sdk::SdkError::Closed => Self::Closed,
        }
    }
}

/// Failure reported by the platform credential adapter.
///
/// `read` returning `null` means no credential blob exists; throwing
/// `Unavailable` means the store is locked or absent; `WriteFailed` covers
/// rejected writes and deletes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Error)]
pub enum CredentialStoreError {
    Unavailable,
    WriteFailed,
}

impl fmt::Display for CredentialStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("protected credential storage is unavailable"),
            Self::WriteFailed => formatter.write_str("protected credential write failed"),
        }
    }
}

impl std::error::Error for CredentialStoreError {}
