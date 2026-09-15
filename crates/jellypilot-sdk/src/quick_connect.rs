//! Jellyfin Quick Connect authentication flow.
//!
//! The workflow runs on the SDK runtime and reports through a listener:
//! the pairing code, the approval transition, and exactly one terminal
//! outcome. Cancelling the session aborts polling promptly; a terminal
//! outcome is delivered exactly once.

use std::sync::{Arc, Mutex};

use jellypilot_auth::login::{
    quick_connect_available, quick_connect_workflow, LoginEvent, QUICK_CONNECT_POLL_INTERVAL,
    QUICK_CONNECT_TIMEOUT,
};
use jellypilot_core::request_gate::RequestGate;
use jellypilot_media_server::MediaServerProvider;
use tokio::sync::OwnedMutexGuard;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{ProfileCandidate, Sdk, SdkError};

/// Terminal result of a Quick Connect session.
pub enum QuickConnectOutcome {
    /// Authentication succeeded; the candidate is ready for activation.
    Success(Box<ProfileCandidate>),
    /// The flow failed with a typed error.
    Failed(SdkError),
    /// The session was cancelled before completing.
    Cancelled,
}

impl std::fmt::Debug for QuickConnectOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success(_) => formatter.write_str("QuickConnectOutcome::Success"),
            Self::Failed(error) => formatter
                .debug_tuple("QuickConnectOutcome::Failed")
                .field(error)
                .finish(),
            Self::Cancelled => formatter.write_str("QuickConnectOutcome::Cancelled"),
        }
    }
}

/// Receives Quick Connect progress and its terminal outcome.
///
/// Callbacks arrive on SDK runtime threads, never on a UI thread. Exactly
/// one `on_completed` is delivered per session.
pub trait QuickConnectListener: Send + Sync {
    /// The server issued a pairing code to display.
    fn on_code(&self, code: String);
    /// The user approved the code; final authentication is in flight.
    fn on_approving(&self);
    /// Terminal outcome; no further callbacks follow.
    fn on_completed(&self, outcome: QuickConnectOutcome);
}

/// A running Quick Connect flow. Dropping it cancels the flow; the listener
/// still receives the terminal outcome.
pub struct QuickConnectSession {
    cancel: CancellationToken,
    task: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl QuickConnectSession {
    /// Cancels the flow. The listener still receives
    /// [`QuickConnectOutcome::Cancelled`] unless a terminal outcome already
    /// fired.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl std::fmt::Debug for QuickConnectSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QuickConnectSession")
            .field("cancelled", &self.cancel.is_cancelled())
            .finish()
    }
}

impl Drop for QuickConnectSession {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.get_mut().take() {
            task.abort();
        }
    }
}

/// Sole owner of a session's terminal delivery.
///
/// Created before the workflow task is spawned and moved into it, so every
/// exit path — completion, cancellation, abort before the first poll, or
/// runtime shutdown dropping the task — funnels through `Drop` and delivers
/// exactly one `on_completed`. The account permit is released before the
/// callback so the listener may begin the next account operation
/// immediately.
struct QuickConnectTerminal {
    listener: Arc<dyn QuickConnectListener>,
    recorded: Mutex<Option<QuickConnectOutcome>>,
    permit: Mutex<Option<OwnedMutexGuard<()>>>,
}

impl QuickConnectTerminal {
    fn record(&self, outcome: QuickConnectOutcome) {
        *self
            .recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(outcome);
    }

    fn emit_terminal(&self) {
        let outcome = self
            .recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .unwrap_or(QuickConnectOutcome::Cancelled);
        // Release account admission before the terminal callback so the
        // listener can immediately begin the next account operation.
        drop(
            self.permit
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take(),
        );
        // A panicking listener must not abort the process when this drop
        // already runs inside a task unwind.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.listener.on_completed(outcome);
        }));
    }
}

impl Drop for QuickConnectTerminal {
    fn drop(&mut self) {
        self.emit_terminal();
    }
}

impl Sdk {
    /// Starts a Jellyfin Quick Connect flow against `server_url`.
    ///
    /// Returns the session immediately; progress arrives on `listener`.
    /// Quick Connect is a Jellyfin capability — other providers fail with
    /// [`SdkError::InvalidInput`].
    pub fn start_quick_connect(
        &self,
        provider: MediaServerProvider,
        server_url: String,
        listener: Arc<dyn QuickConnectListener>,
    ) -> Result<Arc<QuickConnectSession>, SdkError> {
        self.inner.check_open()?;
        if !quick_connect_available(provider) {
            return Err(SdkError::InvalidInput(
                "Quick Connect is only available for Jellyfin servers".to_owned(),
            ));
        }
        let server_url = jellypilot_auth::login::validate_server_url(&server_url, provider)
            .map_err(SdkError::InvalidInput)?;

        let cancel = self.inner.shutdown.child_token();
        let inner = Arc::clone(&self.inner);
        let session_cancel = cancel.clone();
        let task = self.inner.handle.spawn({
            // The terminal owner is captured by the task future before
            // spawn: an abort before the first poll or a spawn panic still
            // drops it and delivers exactly one outcome.
            let terminal = Arc::new(QuickConnectTerminal {
                listener,
                recorded: Mutex::new(None),
                permit: Mutex::new(None),
            });
            async move {
                match inner.account_op.clone().try_lock_owned() {
                    Ok(permit) => {
                        *terminal
                            .permit
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(permit);
                    }
                    Err(_) => {
                        terminal.record(QuickConnectOutcome::Failed(
                            SdkError::OperationInProgress,
                        ));
                        return;
                    }
                }
                let client = Arc::new(jellypilot_media_server::JellyfinClient::with_storage_dir(
                    inner.config.storage_dir.clone(),
                ));
                client.set_device_name(inner.config.device_name.clone());

                let mut gate = RequestGate::default();
                let session_token = gate.begin_login();
                let emit_terminal = Arc::clone(&terminal);
                let emit_cancel = session_cancel.clone();
                let emit = move |event: LoginEvent| -> bool {
                    if emit_cancel.is_cancelled() {
                        return false;
                    }
                    match event {
                        LoginEvent::QuickConnectCode { code, .. } => {
                            emit_terminal.listener.on_code(code);
                        }
                        LoginEvent::QuickConnectApproving { .. } => {
                            emit_terminal.listener.on_approving();
                        }
                        LoginEvent::Login { client, result, .. } => {
                            let outcome = match result {
                                Ok(()) => {
                                    match jellypilot_auth::login::ValidatedProfileCandidate::from_authenticated_client(client) {
                                        Ok(candidate) => QuickConnectOutcome::Success(Box::new(
                                            ProfileCandidate::new(candidate),
                                        )),
                                        Err(error) => QuickConnectOutcome::Failed(
                                            SdkError::Authentication(error.to_string()),
                                        ),
                                    }
                                }
                                Err(error) => QuickConnectOutcome::Failed(match error {
                                    jellypilot_auth::login::LoginError::AuthStorage(storage) => {
                                        SdkError::from(storage)
                                    }
                                    jellypilot_auth::login::LoginError::Request(message) => {
                                        SdkError::Authentication(message)
                                    }
                                }),
                            };
                            emit_terminal.record(outcome);
                        }
                        // Saved-profile events cannot occur in this flow.
                        LoginEvent::SavedProfiles(_) | LoginEvent::SavedSessionStored { .. } => {}
                    }
                    !emit_cancel.is_cancelled()
                };

                tokio::select! {
                    biased;
                    () = session_cancel.cancelled() => {}
                    () = quick_connect_workflow(
                        client,
                        server_url,
                        session_token,
                        emit,
                        QUICK_CONNECT_POLL_INTERVAL,
                        QUICK_CONNECT_TIMEOUT,
                    ) => {}
                }
                // Dropping the terminal releases the account permit and
                // delivers the recorded outcome — or `Cancelled` when the
                // flow ended before one was recorded.
                drop(terminal);
            }
        });

        Ok(Arc::new(QuickConnectSession {
            cancel,
            task: tokio::sync::Mutex::new(Some(task)),
        }))
    }
}
