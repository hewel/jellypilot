//! Authentication workflows and secure profile storage for JellyPilot frontends.

pub mod login;

use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use jellypilot_media_server::{Credentials, MediaServerProvider, SavedSession};
use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use zeroize::{Zeroize, Zeroizing};

const KEYRING_SERVICE: &str = "io.github.hewel.JellyPilot.GtkPreview";
const KEYRING_ACCOUNT: &str = "saved-media-server-profiles-v1";
const STORAGE_VERSION: u32 = 1;
const WORKER_THREAD_NAME: &str = "jellypilot-secret-service";

#[derive(Serialize, Deserialize)]
struct StoredProfiles {
    version: u32,
    profiles: Vec<SavedSession>,
}

#[derive(Serialize)]
struct StoredProfilesRef<'a> {
    version: u32,
    profiles: &'a [SavedSession],
}

struct SensitiveSessions(Vec<SavedSession>);

pub struct SensitiveSavedSession(SavedSession);

impl Deref for SensitiveSavedSession {
    type Target = SavedSession;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for SensitiveSavedSession {
    fn drop(&mut self) {
        self.0.access_token.zeroize();
    }
}

impl fmt::Debug for SensitiveSavedSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveSavedSession([redacted])")
    }
}

pub struct AuthCredentials(Credentials);

impl Deref for AuthCredentials {
    type Target = Credentials;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for AuthCredentials {
    fn drop(&mut self) {
        self.0.password.zeroize();
    }
}

impl fmt::Debug for AuthCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthCredentials([redacted])")
    }
}

impl Deref for SensitiveSessions {
    type Target = Vec<SavedSession>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SensitiveSessions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for SensitiveSessions {
    fn drop(&mut self) {
        for session in &mut self.0 {
            session.access_token.zeroize();
        }
    }
}

#[derive(Clone)]
pub struct AuthStore {
    credential: Arc<dyn SecureCredential>,
    operation: Arc<Mutex<()>>,
}

struct SecretServiceCredential {
    access: Mutex<()>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CredentialError {
    Missing,
    Unavailable,
    WriteFailed,
}

trait SecureCredential: Send + Sync {
    fn read(&self) -> Result<Vec<u8>, CredentialError>;
    fn write(&self, secret: &[u8]) -> Result<(), CredentialError>;
    fn delete(&self) -> Result<(), CredentialError>;
}

#[derive(Clone, Eq, PartialEq)]
pub struct SavedProfileKey(String);

impl fmt::Debug for SavedProfileKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SavedProfileKey([redacted])")
    }
}

#[derive(Clone)]
pub struct SavedProfileSummary {
    pub key: SavedProfileKey,
    provider: MediaServerProvider,
    pub server_url: String,
    pub server_name: Option<String>,
    pub user_name: String,
}

impl SavedProfileSummary {
    pub const fn key(&self) -> &SavedProfileKey {
        &self.key
    }

    pub const fn provider(&self) -> MediaServerProvider {
        self.provider
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    pub fn user_name(&self) -> &str {
        &self.user_name
    }

    pub fn title(&self) -> String {
        format!("{}@{}", self.user_name, self.server_display_name())
    }

    pub const fn subtitle(&self) -> &'static str {
        match self.provider {
            MediaServerProvider::Jellyfin => "Jellyfin",
            MediaServerProvider::Emby => "Emby",
        }
    }

    pub fn restore_accessibility_label(&self) -> String {
        format!(
            "Restore saved sign-in for {} on {}",
            self.user_name,
            self.server_display_name()
        )
    }

    pub fn forget_accessibility_label(&self) -> String {
        format!(
            "Forget saved sign-in for {} on {}",
            self.user_name,
            self.server_display_name()
        )
    }

    pub fn forget_confirmation(&self) -> String {
        format!(
            "{} on {} will need a password to sign in again.",
            self.user_name,
            self.server_display_name()
        )
    }

    fn server_display_name(&self) -> &str {
        self.server_name.as_deref().unwrap_or(&self.server_url)
    }
}

impl fmt::Debug for SavedProfileSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedProfileSummary")
            .field("key", &self.key)
            .field("provider", &self.provider)
            .field("server_url", &"[redacted]")
            .field("server_name", &self.server_name)
            .field("user_name", &self.user_name)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthStorageError {
    Unavailable,
    Corrupt,
    ProfileNotFound,
    WriteFailed,
}

impl fmt::Display for AuthStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "Linux Secret Service is unavailable or locked",
            Self::Corrupt => "saved authentication data is invalid",
            Self::ProfileNotFound => "the saved profile no longer exists",
            Self::WriteFailed => "saved authentication could not be updated",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AuthStorageError {}

impl Default for AuthStore {
    fn default() -> Self {
        Self {
            credential: Arc::new(SecretServiceCredential {
                access: Mutex::new(()),
            }),
            operation: Arc::new(Mutex::new(())),
        }
    }
}

impl AuthStore {
    pub const fn protect_credentials(credentials: Credentials) -> AuthCredentials {
        AuthCredentials(credentials)
    }

    pub async fn load_profiles(&self) -> Result<Vec<SavedProfileSummary>, AuthStorageError> {
        self.run(|store| store.load_profiles_blocking()).await
    }

    pub async fn load_session(
        &self,
        key: SavedProfileKey,
    ) -> Result<SensitiveSavedSession, AuthStorageError> {
        self.run(move |store| store.load_session_blocking(&key))
            .await
    }

    pub async fn save_session(
        &self,
        session: SavedSession,
    ) -> Result<(SavedProfileKey, Vec<SavedProfileSummary>), AuthStorageError> {
        self.run(move |store| store.save_session_blocking(session))
            .await
    }

    pub async fn remove_profile(
        &self,
        key: SavedProfileKey,
    ) -> Result<Vec<SavedProfileSummary>, AuthStorageError> {
        self.run(move |store| store.remove_profile_blocking(&key))
            .await
    }

    async fn run<T, F>(&self, operation: F) -> Result<T, AuthStorageError>
    where
        T: Send + 'static,
        F: FnOnce(Self) -> Result<T, AuthStorageError> + Send + 'static,
    {
        let store = self.clone();
        let (sender, receiver) = oneshot::channel();
        std::thread::Builder::new()
            .name(WORKER_THREAD_NAME.to_owned())
            .spawn(move || {
                let _ = sender.send(operation(store));
            })
            .map_err(|_| AuthStorageError::Unavailable)?;
        receiver.await.map_err(|_| AuthStorageError::Unavailable)?
    }

    fn load_profiles_blocking(&self) -> Result<Vec<SavedProfileSummary>, AuthStorageError> {
        let _operation = self.lock_operation()?;
        self.load_sessions()
            .map(|sessions| profile_summaries(&sessions))
    }

    fn load_session_blocking(
        &self,
        key: &SavedProfileKey,
    ) -> Result<SensitiveSavedSession, AuthStorageError> {
        let _operation = self.lock_operation()?;
        let mut sessions = self.load_sessions()?;
        let position = sessions
            .iter()
            .position(|session| profile_key(session) == *key)
            .ok_or(AuthStorageError::ProfileNotFound)?;
        Ok(SensitiveSavedSession(sessions.remove(position)))
    }

    fn save_session_blocking(
        &self,
        mut session: SavedSession,
    ) -> Result<(SavedProfileKey, Vec<SavedProfileSummary>), AuthStorageError> {
        let _operation = match self.lock_operation() {
            Ok(operation) => operation,
            Err(error) => {
                session.access_token.zeroize();
                return Err(error);
            }
        };
        let mut sessions = match self.load_sessions() {
            Ok(sessions) => sessions,
            Err(error) => {
                session.access_token.zeroize();
                return Err(error);
            }
        };
        let key = upsert_session(&mut sessions, session);
        self.write_sessions(&sessions)?;
        Ok((key, profile_summaries(&sessions)))
    }

    fn remove_profile_blocking(
        &self,
        key: &SavedProfileKey,
    ) -> Result<Vec<SavedProfileSummary>, AuthStorageError> {
        let _operation = self.lock_operation()?;
        let mut sessions = self.load_sessions()?;
        if !remove_session(&mut sessions, key) {
            return Err(AuthStorageError::ProfileNotFound);
        }
        if sessions.is_empty() {
            self.delete_all()?;
        } else {
            self.write_sessions(&sessions)?;
        }
        Ok(profile_summaries(&sessions))
    }

    fn load_sessions(&self) -> Result<SensitiveSessions, AuthStorageError> {
        let secret = match self.credential.read() {
            Ok(secret) => Zeroizing::new(secret),
            Err(CredentialError::Missing) => return Ok(SensitiveSessions(Vec::new())),
            Err(CredentialError::Unavailable | CredentialError::WriteFailed) => {
                return Err(AuthStorageError::Unavailable);
            }
        };
        let stored: StoredProfiles =
            serde_json::from_slice(secret.as_slice()).map_err(|_| AuthStorageError::Corrupt)?;
        let profiles = SensitiveSessions(stored.profiles);
        if stored.version == STORAGE_VERSION && profiles.iter().all(valid_session) {
            Ok(profiles)
        } else {
            Err(AuthStorageError::Corrupt)
        }
    }

    fn write_sessions(&self, sessions: &[SavedSession]) -> Result<(), AuthStorageError> {
        let encoded = Zeroizing::new(
            serde_json::to_vec(&StoredProfilesRef {
                version: STORAGE_VERSION,
                profiles: sessions,
            })
            .map_err(|_| AuthStorageError::WriteFailed)?,
        );
        self.credential
            .write(encoded.as_slice())
            .map_err(|_| AuthStorageError::WriteFailed)
    }

    fn delete_all(&self) -> Result<(), AuthStorageError> {
        match self.credential.delete() {
            Ok(()) | Err(CredentialError::Missing) => Ok(()),
            Err(CredentialError::Unavailable | CredentialError::WriteFailed) => {
                Err(AuthStorageError::WriteFailed)
            }
        }
    }

    fn lock_operation(&self) -> Result<std::sync::MutexGuard<'_, ()>, AuthStorageError> {
        self.operation
            .lock()
            .map_err(|_| AuthStorageError::Unavailable)
    }
}

impl SecureCredential for SecretServiceCredential {
    fn read(&self) -> Result<Vec<u8>, CredentialError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        match keyring_entry()?.get_secret() {
            Ok(secret) => Ok(secret),
            Err(KeyringError::NoEntry) => Err(CredentialError::Missing),
            Err(_) => Err(CredentialError::Unavailable),
        }
    }

    fn write(&self, secret: &[u8]) -> Result<(), CredentialError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        keyring_entry()?
            .set_secret(secret)
            .map_err(|_| CredentialError::WriteFailed)
    }

    fn delete(&self) -> Result<(), CredentialError> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        match keyring_entry()?.delete_credential() {
            Ok(()) => Ok(()),
            Err(KeyringError::NoEntry) => Err(CredentialError::Missing),
            Err(_) => Err(CredentialError::WriteFailed),
        }
    }
}

fn keyring_entry() -> Result<Entry, CredentialError> {
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|_| CredentialError::Unavailable)
}

fn upsert_session(sessions: &mut Vec<SavedSession>, session: SavedSession) -> SavedProfileKey {
    let key = profile_key(&session);
    let _ = remove_session(sessions, &key);
    sessions.insert(0, session);
    key
}

fn remove_session(sessions: &mut Vec<SavedSession>, key: &SavedProfileKey) -> bool {
    if let Some(position) = sessions.iter().position(|saved| profile_key(saved) == *key) {
        let mut removed = sessions.remove(position);
        removed.access_token.zeroize();
        true
    } else {
        false
    }
}

fn profile_summaries(sessions: &[SavedSession]) -> Vec<SavedProfileSummary> {
    sessions
        .iter()
        .map(|session| SavedProfileSummary {
            key: profile_key(session),
            provider: session.provider,
            server_url: session.server_url.clone(),
            server_name: session.server_name.clone(),
            user_name: session.user_name.clone(),
        })
        .collect()
}

fn profile_key(session: &SavedSession) -> SavedProfileKey {
    let provider = match session.provider {
        MediaServerProvider::Jellyfin => "jellyfin",
        MediaServerProvider::Emby => "emby",
    };
    SavedProfileKey(format!(
        "{provider}|{}|{}",
        session.server_url.trim_end_matches('/'),
        session.user_id
    ))
}

fn valid_session(session: &SavedSession) -> bool {
    !session.server_url.trim().is_empty()
        && !session.access_token.is_empty()
        && !session.user_id.is_empty()
        && !session.user_name.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::ThreadId;

    use super::*;

    #[derive(Default)]
    struct MemoryCredential {
        secret: Mutex<Option<Vec<u8>>>,
        fail_delete: AtomicBool,
        operation_thread: Mutex<Option<(ThreadId, String)>>,
    }

    impl MemoryCredential {
        fn record_operation_thread(&self) -> Result<(), CredentialError> {
            let thread = std::thread::current();
            *self
                .operation_thread
                .lock()
                .map_err(|_| CredentialError::Unavailable)? =
                Some((thread.id(), thread.name().unwrap_or_default().to_owned()));
            Ok(())
        }
    }

    impl SecureCredential for MemoryCredential {
        fn read(&self) -> Result<Vec<u8>, CredentialError> {
            self.record_operation_thread()?;
            self.secret
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .clone()
                .ok_or(CredentialError::Missing)
        }

        fn write(&self, secret: &[u8]) -> Result<(), CredentialError> {
            self.record_operation_thread()?;
            *self
                .secret
                .lock()
                .map_err(|_| CredentialError::Unavailable)? = Some(secret.to_vec());
            Ok(())
        }

        fn delete(&self) -> Result<(), CredentialError> {
            self.record_operation_thread()?;
            if self.fail_delete.load(Ordering::Relaxed) {
                return Err(CredentialError::WriteFailed);
            }
            self.secret
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .take()
                .map(|_| ())
                .ok_or(CredentialError::Missing)
        }
    }

    fn memory_store() -> AuthStore {
        AuthStore {
            credential: Arc::new(MemoryCredential::default()),
            operation: Arc::new(Mutex::new(())),
        }
    }

    fn store_with_credential(credential: Arc<dyn SecureCredential>) -> AuthStore {
        AuthStore {
            credential,
            operation: Arc::new(Mutex::new(())),
        }
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(future)
    }

    fn session(access_token: &str) -> SavedSession {
        SavedSession {
            provider: MediaServerProvider::Jellyfin,
            server_url: "https://media.example.com".to_string(),
            access_token: access_token.to_string(),
            user_id: "user-1".to_string(),
            user_name: "Ada".to_string(),
            server_name: Some("Media Room".to_string()),
            device_id: Some("jellypilot-device".to_string()),
        }
    }

    #[test]
    fn upsert_session_replaces_the_same_server_user_profile() {
        let mut sessions = vec![session("old-token")];

        upsert_session(&mut sessions, session("new-token"));

        assert_eq!(sessions[0].access_token, "new-token");
    }

    #[test]
    fn saved_profile_debug_output_redacts_server_and_key() {
        let summary = profile_summaries(&[session("secret-token")]).remove(0);

        let debug = format!("{summary:?}");

        assert!(!debug.contains("media.example.com") && !debug.contains("user-1"));
    }

    #[test]
    fn saved_session_debug_output_does_not_expose_access_token() {
        let debug = format!("{:?}", session("secret-token"));

        assert!(!debug.contains("secret-token"));
    }

    #[test]
    fn invalid_saved_session_is_rejected() {
        let mut invalid = session("secret-token");
        invalid.server_url.clear();

        assert!(!valid_session(&invalid));
    }

    #[test]
    fn auth_store_round_trips_a_saved_session_through_its_interface() {
        let store = memory_store();
        let expected = session("secret-token");
        let (key, _) =
            block_on(store.save_session(expected)).expect("memory adapter should save session");

        let restored =
            block_on(store.load_session(key)).expect("memory adapter should load session");

        assert_eq!(restored.access_token, "secret-token");
    }

    #[test]
    fn remove_profile_deletes_the_last_secure_credential() {
        let store = memory_store();
        let (key, _) = block_on(store.save_session(session("secret-token")))
            .expect("memory adapter should save session");

        block_on(store.remove_profile(key)).expect("memory adapter should remove session");

        assert!(block_on(store.load_profiles())
            .expect("empty store should load")
            .is_empty());
    }

    #[test]
    fn remove_profile_reports_a_stale_profile_key() {
        let store = memory_store();
        let missing = profile_key(&session("secret-token"));

        let error =
            block_on(store.remove_profile(missing)).expect_err("missing profile should fail");

        assert_eq!(error, AuthStorageError::ProfileNotFound);
    }

    #[test]
    fn remove_profile_reports_secure_deletion_failure() {
        let credential = Arc::new(MemoryCredential::default());
        let store = store_with_credential(credential.clone());
        let (key, _) = block_on(store.save_session(session("secret-token")))
            .expect("memory adapter should save session");
        credential.fail_delete.store(true, Ordering::Relaxed);

        let error =
            block_on(store.remove_profile(key)).expect_err("failed secure deletion should fail");

        assert_eq!(error, AuthStorageError::WriteFailed);
    }

    #[test]
    fn public_interface_runs_credential_work_on_the_named_worker_thread() {
        let caller_thread = std::thread::current().id();
        let credential = Arc::new(MemoryCredential::default());
        let store = store_with_credential(credential.clone());

        block_on(store.load_profiles()).expect("empty store should load");

        let (worker_thread, worker_name) = credential
            .operation_thread
            .lock()
            .expect("operation thread should be recorded")
            .clone()
            .expect("credential should be accessed");
        assert_ne!(worker_thread, caller_thread);
        assert_eq!(worker_name, WORKER_THREAD_NAME);
    }

    #[test]
    fn typed_storage_error_round_trips_through_the_async_interface() {
        let credential = Arc::new(MemoryCredential::default());
        credential
            .write(br#"{"version":2,"profiles":[]}"#)
            .expect("fixture should be stored");
        let store = store_with_credential(credential);

        let error = block_on(store.load_profiles()).expect_err("unsupported version should fail");

        assert_eq!(error, AuthStorageError::Corrupt);
    }
}
