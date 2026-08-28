use std::sync::Arc;
use std::time::Duration;

use crate::{AuthCredentials, AuthStorageError, SavedProfileKey, SavedProfileSummary};
use jellypilot_core::config::LoginPrefill;
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_media_server::{
    JellyfinClient, JellyfinError, MediaServerProvider, QuickConnectStatus, SavedSession,
};
use tokio::sync::watch;
use zeroize::{Zeroize, Zeroizing};

pub const QUICK_CONNECT_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const QUICK_CONNECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub enum ConnectionPhase {
    #[default]
    SignedOut,
    Connecting,
    Connected,
    Failed,
}

#[derive(Clone)]
pub enum LoginEvent {
    SavedProfiles(Result<Vec<SavedProfileSummary>, AuthStorageError>),
    SavedSessionStored {
        session: SessionToken,
        result: Result<(SavedProfileKey, Vec<SavedProfileSummary>), AuthStorageError>,
    },
    Login {
        session: SessionToken,
        client: Arc<JellyfinClient>,
        result: Result<(), LoginError>,
    },
    QuickConnectCode {
        session: SessionToken,
        code: String,
    },
    QuickConnectApproving {
        session: SessionToken,
    },
    ForgotProfile {
        session: SessionToken,
        key: SavedProfileKey,
        sign_out: bool,
        result: Result<Vec<SavedProfileSummary>, AuthStorageError>,
    },
}

#[derive(Clone)]
pub enum LoginError {
    AuthStorage(AuthStorageError),
    Request(String),
}

impl From<AuthStorageError> for LoginError {
    fn from(error: AuthStorageError) -> Self {
        Self::AuthStorage(error)
    }
}

impl std::fmt::Display for LoginError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthStorage(error) => write!(formatter, "Saved sign-in unavailable: {error}."),
            Self::Request(message) => formatter.write_str(message),
        }
    }
}

pub enum LoginEffect {
    AuthStarted,
    Authenticated {
        client: Arc<JellyfinClient>,
        stored_session: Option<SavedSession>,
    },
    AuthFailed {
        message: String,
    },
    /// Form input failed validation before any request; no Home impact.
    InvalidInput,
    PersistPrefill {
        prefill: LoginPrefill,
        provider: String,
        remember: bool,
    },
    Diagnostic(DiagnosticLevel, DiagnosticCategory, String),
    Cancelled,
    ProfileBusyChanged,
    Disconnect,
    LoadSavedProfiles,
    RunPasswordAuth {
        session: SessionToken,
        credentials: AuthCredentials,
    },
    RunQuickConnect {
        session: SessionToken,
        server_url: String,
        cancellation: watch::Receiver<u64>,
    },
    RunRestore {
        session: SessionToken,
        key: SavedProfileKey,
    },
    RunForget {
        session: SessionToken,
        key: SavedProfileKey,
        sign_out: bool,
    },
}

impl std::fmt::Debug for LoginEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SavedProfiles(result) => formatter
                .debug_tuple("SavedProfiles")
                .field(&result.as_ref().map(Vec::len))
                .finish(),
            Self::SavedSessionStored { session, result } => formatter
                .debug_struct("SavedSessionStored")
                .field("session", session)
                .field(
                    "profile_count",
                    &result.as_ref().map(|(_, profiles)| profiles.len()),
                )
                .finish(),
            Self::Login {
                session, result, ..
            } => formatter
                .debug_struct("Login")
                .field("session", session)
                .field("successful", &result.is_ok())
                .finish(),
            Self::QuickConnectCode { session, .. } => formatter
                .debug_struct("QuickConnectCode")
                .field("session", session)
                .field("code", &"[redacted]")
                .finish(),
            Self::QuickConnectApproving { session } => formatter
                .debug_struct("QuickConnectApproving")
                .field("session", session)
                .finish(),
            Self::ForgotProfile {
                session,
                key,
                sign_out,
                result,
            } => formatter
                .debug_struct("ForgotProfile")
                .field("session", session)
                .field("key", key)
                .field("sign_out", sign_out)
                .field("successful", &result.is_ok())
                .finish(),
        }
    }
}

pub async fn quick_connect_workflow(
    client: Arc<JellyfinClient>,
    server_url: String,
    session: SessionToken,
    emit: impl Fn(LoginEvent) -> bool + Send,
    poll_interval: Duration,
    workflow_timeout: Duration,
) {
    let command_client = Arc::clone(&client);
    let result: Result<(), LoginError> = async {
        let request = command_client
            .login()
            .quick_connect_start(&server_url)
            .await
            .map_err(|error| LoginError::Request(quick_connect_start_message(&error).to_owned()))?;
        let (code, secret) = request.into_parts();
        let secret = Zeroizing::new(secret);
        if !emit(LoginEvent::QuickConnectCode { session, code }) {
            return Err(LoginError::Request(
                "Quick Connect was cancelled.".to_owned(),
            ));
        }

        tokio::time::timeout(workflow_timeout, async {
            loop {
                tokio::time::sleep(poll_interval).await;
                match command_client
                    .login()
                    .quick_connect_check(&server_url, secret.as_str())
                    .await
                    .map_err(|_| LoginError::Request(quick_connect_check_message().to_owned()))?
                {
                    QuickConnectStatus::Waiting => {}
                    QuickConnectStatus::Approved => break Ok(()),
                }
            }
        })
        .await
        .unwrap_or_else(|_| {
            Err(LoginError::Request(
                quick_connect_timeout_message().to_owned(),
            ))
        })?;

        if !emit(LoginEvent::QuickConnectApproving { session }) {
            return Err(LoginError::Request(
                "Quick Connect was cancelled.".to_owned(),
            ));
        }
        let mut response = command_client
            .login()
            .quick_connect_authenticate(&server_url, secret.as_str())
            .await
            .map_err(|_| LoginError::Request(quick_connect_authentication_message().to_owned()))?;
        response.access_token.zeroize();
        Ok(())
    }
    .await;

    let _ = emit(LoginEvent::Login {
        session,
        client,
        result,
    });
}

#[must_use]
pub const fn quick_connect_available(provider: MediaServerProvider) -> bool {
    matches!(provider, MediaServerProvider::Jellyfin)
}

const fn quick_connect_start_message(error: &JellyfinError) -> &'static str {
    match error {
        JellyfinError::QuickConnectUnavailable => {
            "Quick Connect is not enabled on this Jellyfin server. Use password sign-in instead."
        }
        JellyfinError::InvalidUrl(_) => "Enter a valid Jellyfin server URL to request a code.",
        _ => "Quick Connect could not be started. Check the server address and try again.",
    }
}

const fn quick_connect_check_message() -> &'static str {
    "JellyPilot could not check this Quick Connect code. Request a new code and try again."
}

const fn quick_connect_authentication_message() -> &'static str {
    "JellyPilot could not finish the approved Quick Connect sign-in. Request a new code and try again."
}

const fn quick_connect_timeout_message() -> &'static str {
    "Quick Connect code expired. Request a new code to try again."
}

#[must_use]
pub const fn can_start_login(connection: ConnectionPhase) -> bool {
    matches!(
        connection,
        ConnectionPhase::SignedOut | ConnectionPhase::Failed
    )
}

#[must_use]
pub fn should_disconnect_after_forget(
    sign_out: bool,
    operation_session: SessionToken,
    current_session: SessionToken,
    connection: ConnectionPhase,
    active_profile_matches: bool,
) -> bool {
    sign_out
        && operation_session == current_session
        && matches!(connection, ConnectionPhase::Connected)
        && active_profile_matches
}

#[must_use]
pub const fn provider_for(selected: u32) -> MediaServerProvider {
    if selected == 1 {
        MediaServerProvider::Emby
    } else {
        MediaServerProvider::Jellyfin
    }
}

#[cfg(test)]
mod tests {
    use jellypilot_core::request_gate::RequestGate;

    use super::*;

    #[test]
    fn quick_connect_is_available_only_for_jellyfin() {
        assert!(quick_connect_available(MediaServerProvider::Jellyfin));
        assert!(!quick_connect_available(MediaServerProvider::Emby));
    }

    #[test]
    fn quick_connect_command_debug_redacts_the_public_code() {
        let mut gate = RequestGate::default();
        let session = gate.begin_login();
        let event = LoginEvent::QuickConnectCode {
            session,
            code: "ABCD12".to_owned(),
        };

        let debug = format!("{event:?}");
        assert!(!debug.contains("ABCD12"));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn quick_connect_user_messages_do_not_include_transport_details() {
        let error = JellyfinError::HttpError(
            "https://media.example/QuickConnect/Connect?secret=private-secret".to_owned(),
        );

        let messages = [
            quick_connect_start_message(&error),
            quick_connect_check_message(),
            quick_connect_authentication_message(),
            quick_connect_timeout_message(),
        ];
        assert!(messages
            .iter()
            .all(|message| !message.contains("private-secret") && !message.contains("https://")));
        assert_eq!(
            quick_connect_timeout_message(),
            "Quick Connect code expired. Request a new code to try again."
        );
    }

    #[test]
    fn saved_profile_deletion_only_signs_out_the_originating_live_session() {
        let mut gate = RequestGate::default();
        for _ in 0..4 {
            gate.disconnect();
        }
        let current = gate.current_session();
        gate.disconnect();
        let other = gate.current_session();
        assert!(should_disconnect_after_forget(
            true,
            current,
            current,
            ConnectionPhase::Connected,
            true,
        ));
        assert!(!should_disconnect_after_forget(
            true,
            current,
            other,
            ConnectionPhase::Connected,
            true,
        ));
        assert!(!should_disconnect_after_forget(
            true,
            current,
            current,
            ConnectionPhase::Connected,
            false,
        ));
    }

    #[test]
    fn login_is_blocked_while_connecting_or_connected() {
        assert!(!can_start_login(ConnectionPhase::Connecting));
        assert!(!can_start_login(ConnectionPhase::Connected));
        assert!(can_start_login(ConnectionPhase::SignedOut));
        assert!(can_start_login(ConnectionPhase::Failed));
    }
}
