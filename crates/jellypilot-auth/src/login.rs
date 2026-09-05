use std::sync::Arc;
use std::time::Duration;

use crate::{
    AuthCredentials, AuthStorageError, AuthStore, SavedProfileKey, SavedProfileSummary,
    SavedProfilesSnapshot, SensitiveSavedSession,
};
use jellypilot_core::config::LoginPrefill;
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::watchlist::ProfileScope;
use jellypilot_media_server::{
    JellyfinClient, JellyfinError, MediaServerProvider, QuickConnectStatus, SavedSession,
};
use tokio::sync::watch;
use url::Url;
use zeroize::{Zeroize, Zeroizing};

pub const QUICK_CONNECT_POLL_INTERVAL: Duration = Duration::from_secs(5);
pub const QUICK_CONNECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionPhase {
    #[default]
    SignedOut,
    Connecting,
    Connected,
    Failed,
}

#[derive(Clone)]
pub enum LoginEvent {
    SavedProfiles(Result<SavedProfilesSnapshot, AuthStorageError>),
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

/// A saved profile validated on an isolated client and ready for app handoff.
///
/// Constructing this value never changes the current live client. Dropping a
/// cancelled candidate clears both token-owning fields through their owners.
pub struct ValidatedProfileCandidate {
    key: SavedProfileKey,
    scope: ProfileScope,
    client: Arc<JellyfinClient>,
    refreshed_session: SensitiveSavedSession,
}

impl ValidatedProfileCandidate {
    pub const fn key(&self) -> &SavedProfileKey {
        &self.key
    }

    pub const fn scope(&self) -> &ProfileScope {
        &self.scope
    }

    pub const fn client(&self) -> &Arc<JellyfinClient> {
        &self.client
    }

    /// Redacted account label suitable for confirmation UI.
    pub fn account_title(&self) -> String {
        format!(
            "{}@{}",
            self.refreshed_session.user_name,
            self.refreshed_session
                .server_name
                .as_deref()
                .unwrap_or(&self.refreshed_session.server_url)
        )
    }

    /// Transfers the validated connection and refreshed session to the app's
    /// serialized handoff coordinator.
    pub fn into_parts(
        self,
    ) -> (
        SavedProfileKey,
        ProfileScope,
        Arc<JellyfinClient>,
        SensitiveSavedSession,
    ) {
        (self.key, self.scope, self.client, self.refreshed_session)
    }

    /// Captures a client that has just completed password or Quick Connect
    /// authentication as an isolated handoff candidate.
    ///
    /// The client must already hold a complete authenticated session. This
    /// constructor performs no network requests and never changes the app's
    /// active client.
    pub fn from_authenticated_client(client: Arc<JellyfinClient>) -> Result<Self, LoginError> {
        let refreshed_session = SensitiveSavedSession::from_client(&client).ok_or_else(|| {
            LoginError::Request("The authenticated session could not be prepared.".to_owned())
        })?;
        if !crate::valid_session(&refreshed_session) {
            return Err(LoginError::Request(
                "The authenticated session could not be prepared.".to_owned(),
            ));
        }
        let scope = ProfileScope::new(
            refreshed_session.provider,
            refreshed_session.server_url.clone(),
            refreshed_session.user_id.clone(),
        )
        .map_err(|_| {
            LoginError::Request("The authenticated session could not be prepared.".to_owned())
        })?;
        let key = SavedProfileKey::for_scope(&scope);

        Ok(Self {
            key,
            scope,
            client,
            refreshed_session,
        })
    }
}

impl std::fmt::Debug for ValidatedProfileCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedProfileCandidate")
            .field("key", &self.key)
            .field("scope", &"[redacted]")
            .field("client", &"[redacted]")
            .field("refreshed_session", &"[redacted]")
            .finish()
    }
}

/// Loads and validates a saved login without mutating the active connection.
pub async fn validate_saved_profile(
    store: AuthStore,
    key: SavedProfileKey,
) -> Result<ValidatedProfileCandidate, LoginError> {
    let stored_session = store.load_session(key.clone()).await?;
    let candidate = Arc::new(JellyfinClient::for_saved_profile(&stored_session));
    candidate
        .login()
        .restore_session(&stored_session)
        .await
        .map_err(|_| LoginError::Request("Saved sign-in validation failed.".to_owned()))?;
    let validated = ValidatedProfileCandidate::from_authenticated_client(candidate)
        .map_err(|_| LoginError::Request("Saved sign-in validation failed.".to_owned()))?;
    if validated.key() != &key {
        return Err(LoginError::Request(
            "Saved sign-in validation failed.".to_owned(),
        ));
    }
    Ok(validated)
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
}

impl std::fmt::Debug for LoginEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SavedProfiles(result) => formatter
                .debug_tuple("SavedProfiles")
                .field(&result.as_ref().map(|snapshot| snapshot.profiles().len()))
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
pub const fn provider_for(selected: u32) -> MediaServerProvider {
    if selected == 1 {
        MediaServerProvider::Emby
    } else {
        MediaServerProvider::Jellyfin
    }
}

#[must_use]
pub const fn provider_key(provider: MediaServerProvider) -> &'static str {
    match provider {
        MediaServerProvider::Jellyfin => "jellyfin",
        MediaServerProvider::Emby => "emby",
    }
}

#[must_use]
pub const fn provider_label(provider: MediaServerProvider) -> &'static str {
    match provider {
        MediaServerProvider::Jellyfin => "Jellyfin",
        MediaServerProvider::Emby => "Emby",
    }
}

/// Normalizes a user-entered server URL, rejecting anything that is not a
/// plain http(s) host with a safe path (no credentials, query, fragment, or
/// encoded traversal separators).
pub fn validate_server_url(raw: &str, provider: MediaServerProvider) -> Result<String, String> {
    let server_url = raw.trim().trim_end_matches('/');
    let invalid = || format!("Enter a valid {} server URL.", provider_label(provider));
    if server_url.is_empty() || !raw_path_is_safe(server_url) {
        return Err(invalid());
    }
    let parsed = Url::parse(server_url).map_err(|_| invalid())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !path_segments_are_safe(parsed.path())
    {
        return Err(invalid());
    }
    Ok(server_url.to_owned())
}

#[must_use]
pub fn raw_path_is_safe(url: &str) -> bool {
    let without_fragment = url.split('#').next().unwrap_or_default();
    let without_query = without_fragment.split('?').next().unwrap_or_default();
    let path = without_query
        .split_once("://")
        .and_then(|(_, authority_and_path)| {
            authority_and_path
                .find('/')
                .map(|at| &authority_and_path[at..])
        })
        .unwrap_or(without_query);
    path_segments_are_safe(path)
}

#[must_use]
pub fn path_segments_are_safe(path: &str) -> bool {
    !path.split('/').any(|segment| {
        let segment = segment.to_ascii_lowercase();
        segment.contains("%2f")
            || segment.contains("%5c")
            || matches!(segment.replace("%2e", ".").as_str(), "." | "..")
    })
}

#[cfg(test)]
mod tests {
    use jellypilot_core::request_gate::RequestGate;
    use jellypilot_media_server::SavedSession;

    use super::*;

    fn saved_session() -> SavedSession {
        SavedSession {
            provider: MediaServerProvider::Jellyfin,
            server_url: "https://media.example.test".to_owned(),
            access_token: "secret-token".to_owned(),
            user_id: "user-1".to_owned(),
            user_name: "Ada".to_owned(),
            server_name: Some("Media Room".to_owned()),
            device_id: Some("saved-device".to_owned()),
        }
    }

    #[test]
    fn quick_connect_is_available_only_for_jellyfin() {
        assert!(quick_connect_available(MediaServerProvider::Jellyfin));
        assert!(!quick_connect_available(MediaServerProvider::Emby));
    }

    #[test]
    fn validated_candidate_debug_output_redacts_identity_and_token() {
        let session = saved_session();
        let scope = ProfileScope::new(
            session.provider,
            session.server_url.clone(),
            session.user_id.clone(),
        )
        .expect("session should provide a profile scope");
        let candidate = ValidatedProfileCandidate {
            key: SavedProfileKey::for_scope(&scope),
            scope,
            client: Arc::new(JellyfinClient::for_saved_profile(&session)),
            refreshed_session: SensitiveSavedSession::new(session),
        };

        let debug = format!("{candidate:?}");

        assert!(!debug.contains("secret-token"));
        assert!(!debug.contains("media.example.test"));
        assert!(!debug.contains("user-1"));
    }

    #[test]
    fn authenticated_candidate_rejects_incomplete_session_fields() {
        for malformed in [
            SavedSession {
                access_token: String::new(),
                ..saved_session()
            },
            SavedSession {
                user_name: "   ".to_owned(),
                ..saved_session()
            },
        ] {
            let client = Arc::new(JellyfinClient::new());
            client.login().adopt_validated_session(&malformed);

            assert!(ValidatedProfileCandidate::from_authenticated_client(client).is_err());
        }
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
    fn login_is_blocked_while_connecting_or_connected() {
        assert!(!can_start_login(ConnectionPhase::Connecting));
        assert!(!can_start_login(ConnectionPhase::Connected));
        assert!(can_start_login(ConnectionPhase::SignedOut));
        assert!(can_start_login(ConnectionPhase::Failed));
    }

    #[test]
    fn provider_key_and_label_cover_each_provider() {
        assert_eq!(provider_key(MediaServerProvider::Jellyfin), "jellyfin");
        assert_eq!(provider_key(MediaServerProvider::Emby), "emby");
        assert_eq!(provider_label(MediaServerProvider::Jellyfin), "Jellyfin");
        assert_eq!(provider_label(MediaServerProvider::Emby), "Emby");
    }

    #[test]
    fn validate_server_url_trims_and_accepts_plain_http_hosts() {
        assert_eq!(
            validate_server_url(
                "  https://media.example.test/  ",
                MediaServerProvider::Jellyfin,
            )
            .as_deref(),
            Ok("https://media.example.test")
        );
        assert_eq!(
            validate_server_url("http://192.168.1.10:8096", MediaServerProvider::Emby).as_deref(),
            Ok("http://192.168.1.10:8096")
        );
        assert_eq!(
            validate_server_url(
                "https://media.example.test/jellyfin",
                MediaServerProvider::Jellyfin,
            )
            .as_deref(),
            Ok("https://media.example.test/jellyfin")
        );
    }

    #[test]
    fn validate_server_url_rejects_unparseable_or_non_http_urls() {
        let jellyfin_error = "Enter a valid Jellyfin server URL.".to_owned();
        for raw in ["", "   ", "not a server", "ftp://media.example.test"] {
            assert_eq!(
                validate_server_url(raw, MediaServerProvider::Jellyfin),
                Err(jellyfin_error.clone()),
                "input: {raw}"
            );
        }
        assert_eq!(
            validate_server_url("", MediaServerProvider::Emby),
            Err("Enter a valid Emby server URL.".to_owned())
        );
    }

    #[test]
    fn validate_server_url_rejects_credentials_query_and_fragment() {
        for raw in [
            "https://user@media.example.test",
            "https://user:pass@media.example.test",
            "https://media.example.test?api_key=secret",
            "https://media.example.test/#fragment",
        ] {
            assert!(
                validate_server_url(raw, MediaServerProvider::Jellyfin).is_err(),
                "input: {raw}"
            );
        }
    }

    #[test]
    fn validate_server_url_rejects_unsafe_path_segments() {
        for raw in [
            "https://media.example.test/..",
            "https://media.example.test/%2e%2e",
            "https://media.example.test/a%2Fb",
            "https://media.example.test/a%5Cb",
        ] {
            assert!(
                validate_server_url(raw, MediaServerProvider::Jellyfin).is_err(),
                "input: {raw}"
            );
        }
    }

    #[test]
    fn raw_path_is_safe_strips_query_and_fragment_before_checking() {
        assert!(raw_path_is_safe("https://media.example.test/library"));
        assert!(raw_path_is_safe(
            "https://media.example.test/library?x=%2f#y=%5c"
        ));
        assert!(!raw_path_is_safe(
            "https://media.example.test/unsafe%2fpath"
        ));
        assert!(!raw_path_is_safe("media.example.test/.."));
    }

    #[test]
    fn path_segments_are_safe_rejects_encoded_separators_and_traversal() {
        assert!(path_segments_are_safe("/library/movies"));
        assert!(path_segments_are_safe("/"));
        for path in ["..", ".", "%2e%2e", "%2E.", "a%2fb", "a%5cb", "%2F", "%5C"] {
            assert!(!path_segments_are_safe(path), "input: {path}");
        }
    }
}
