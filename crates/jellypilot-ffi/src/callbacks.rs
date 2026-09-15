//! Callback interfaces implemented by the Kotlin platform layer and the
//! adapters that feed them into the SDK's Rust traits.

use std::sync::Arc;

use crate::error::CredentialStoreError;
use crate::ProfileCandidate;

/// Platform protected-credential storage.
///
/// The adapter receives and returns opaque secret bytes only; encryption and
/// key custody belong to the platform (Android Keystore on Android). All
/// methods are invoked on the SDK's credential worker thread, never on the
/// UI thread or an async executor.
#[uniffi::export(foreign)]
pub trait SecureCredentialStore: Send + Sync {
    /// Returns the stored blob, or `null` when none exists.
    fn read(&self) -> Result<Option<Vec<u8>>, CredentialStoreError>;
    /// Persists the blob, replacing any previous value.
    fn write(&self, secret: Vec<u8>) -> Result<(), CredentialStoreError>;
    /// Deletes the stored blob. Deleting an absent blob succeeds.
    fn delete(&self) -> Result<(), CredentialStoreError>;
}

/// Platform teardown seam for profile transitions.
///
/// Invoked while the previous profile is still active, before the SDK
/// commits a new scope. Returning `false` aborts the transition and keeps
/// the previous profile active (ADR 0033 ordering). This is where the app
/// finishes playback and remote teardown.
#[uniffi::export(foreign, async_runtime = "tokio")]
#[async_trait::async_trait]
pub trait SdkHooks: Send + Sync {
    async fn before_profile_handoff(&self) -> bool;
}

/// Receives Quick Connect progress and its terminal outcome.
///
/// Callbacks arrive on SDK runtime threads. Exactly one `on_completed` is
/// delivered per session.
#[uniffi::export(foreign)]
pub trait QuickConnectListener: Send + Sync {
    /// The server issued a pairing code to display.
    fn on_code(&self, code: String);
    /// The user approved the code; final authentication is in flight.
    fn on_approving(&self);
    /// Terminal outcome; no further callbacks follow.
    fn on_completed(&self, outcome: QuickConnectOutcome);
}

/// Terminal result of a Quick Connect session.
#[derive(uniffi::Enum)]
pub enum QuickConnectOutcome {
    /// Authentication succeeded; the candidate is ready for activation.
    Success { candidate: Arc<ProfileCandidate> },
    /// The flow failed with a typed error.
    Failed { error: crate::SdkError },
    /// The session was cancelled before completing.
    Cancelled,
}

/// Adapts the Kotlin credential store to the auth crate's trait.
pub(crate) struct CredentialStoreAdapter {
    store: Arc<dyn SecureCredentialStore>,
}

impl CredentialStoreAdapter {
    pub(crate) fn new(store: Arc<dyn SecureCredentialStore>) -> Self {
        Self { store }
    }
}

impl jellypilot_auth::SecureCredential for CredentialStoreAdapter {
    fn read(&self) -> Result<Vec<u8>, jellypilot_auth::CredentialError> {
        match self.store.read() {
            Ok(Some(secret)) => Ok(secret),
            Ok(None) => Err(jellypilot_auth::CredentialError::Missing),
            Err(_) => Err(jellypilot_auth::CredentialError::Unavailable),
        }
    }

    fn write(&self, secret: &[u8]) -> Result<(), jellypilot_auth::CredentialError> {
        self.store
            .write(secret.to_vec())
            .map_err(|_| jellypilot_auth::CredentialError::WriteFailed)
    }

    fn delete(&self) -> Result<(), jellypilot_auth::CredentialError> {
        self.store
            .delete()
            .map_err(|_| jellypilot_auth::CredentialError::WriteFailed)
    }
}

/// Adapts the Kotlin hooks object to the SDK's async trait.
pub(crate) struct HooksAdapter {
    hooks: Arc<dyn SdkHooks>,
}

impl HooksAdapter {
    pub(crate) fn new(hooks: Arc<dyn SdkHooks>) -> Self {
        Self { hooks }
    }
}

#[async_trait::async_trait]
impl jellypilot_sdk::SdkHooks for HooksAdapter {
    async fn before_profile_handoff(&self) -> bool {
        self.hooks.before_profile_handoff().await
    }
}

/// Adapts the Kotlin listener to the SDK's listener trait.
pub(crate) struct QuickConnectListenerAdapter {
    listener: Arc<dyn QuickConnectListener>,
}

impl QuickConnectListenerAdapter {
    pub(crate) fn new(listener: Arc<dyn QuickConnectListener>) -> Self {
        Self { listener }
    }
}

impl jellypilot_sdk::QuickConnectListener for QuickConnectListenerAdapter {
    fn on_code(&self, code: String) {
        self.listener.on_code(code);
    }

    fn on_approving(&self) {
        self.listener.on_approving();
    }

    fn on_completed(&self, outcome: jellypilot_sdk::QuickConnectOutcome) {
        let outcome = match outcome {
            jellypilot_sdk::QuickConnectOutcome::Success(candidate) => {
                QuickConnectOutcome::Success {
                    candidate: Arc::new(ProfileCandidate::from_sdk(*candidate)),
                }
            }
            jellypilot_sdk::QuickConnectOutcome::Failed(error) => QuickConnectOutcome::Failed {
                error: error.into(),
            },
            jellypilot_sdk::QuickConnectOutcome::Cancelled => QuickConnectOutcome::Cancelled,
        };
        self.listener.on_completed(outcome);
    }
}
