//! Deterministic regressions for scope cancellation, stale rejection, and
//! cross-profile state partitioning. No network: sessions are adopted
//! through the test hook and credentials live in memory.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jellypilot_auth::login::ValidatedProfileCandidate;
use jellypilot_auth::{AuthStore, CredentialError, SavedProfileKey, SecureCredential};
use jellypilot_core::watchlist::{ProfileScope, WatchlistStore};
use jellypilot_media_server::{
    JellyfinClient, MediaServerProvider, SavedSession, VideoLibraryItem,
};

use crate::{
    ActivationOutcome, ProfileCandidate, QuickConnectListener, QuickConnectOutcome, Sdk, SdkConfig,
    SdkError, SdkHooks,
};

#[derive(Default)]
struct MemoryCredential {
    secret: Mutex<Option<Vec<u8>>>,
    fail_write: AtomicBool,
    fail_delete: AtomicBool,
    read_entered: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    read_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    write_entered: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    write_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    delete_entered: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    delete_gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl SecureCredential for MemoryCredential {
    fn read(&self) -> Result<Vec<u8>, CredentialError> {
        if let Some(entered) = self.read_entered.lock().expect("gate lock").take() {
            let _ = entered.send(());
        }
        if let Some(gate) = self.read_gate.lock().expect("gate lock").take() {
            let _ = gate.recv();
        }
        self.secret
            .lock()
            .expect("secret lock")
            .clone()
            .ok_or(CredentialError::Missing)
    }

    fn write(&self, secret: &[u8]) -> Result<(), CredentialError> {
        if let Some(entered) = self.write_entered.lock().expect("gate lock").take() {
            let _ = entered.send(());
        }
        if let Some(gate) = self.write_gate.lock().expect("gate lock").take() {
            let _ = gate.recv();
        }
        if self.fail_write.load(Ordering::SeqCst) {
            return Err(CredentialError::WriteFailed);
        }
        *self.secret.lock().expect("secret lock") = Some(secret.to_vec());
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        if let Some(entered) = self.delete_entered.lock().expect("gate lock").take() {
            let _ = entered.send(());
        }
        if let Some(gate) = self.delete_gate.lock().expect("gate lock").take() {
            let _ = gate.recv();
        }
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(CredentialError::WriteFailed);
        }
        *self.secret.lock().expect("secret lock") = None;
        Ok(())
    }
}

fn test_session(user_id: &str, server_url: &str) -> SavedSession {
    SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: server_url.to_owned(),
        access_token: format!("token-{user_id}"),
        user_id: user_id.to_owned(),
        user_name: user_id.to_owned(),
        server_name: Some("Test Server".to_owned()),
        device_id: None,
    }
}

fn test_item(id: &str) -> VideoLibraryItem {
    VideoLibraryItem {
        id: id.to_owned(),
        name: format!("Item {id}"),
        item_type: "Movie".to_owned(),
        production_year: None,
        premiere_date: None,
        community_rating: None,
        episode_count: None,
        last_played_date: None,
        runtime_seconds: None,
        played: false,
        favorite: false,
        artwork_image_id: None,
        backdrop_image_id: None,
        logo_image_id: None,
        series_poster_image_id: None,
        episode_thumb_image_id: None,
        series_thumb_image_id: None,
        series_backdrop_image_id: None,
        season_poster_image_id: None,
        season_number: None,
        episode_number: None,
        index_number_end: None,
        series_id: None,
        series_name: None,
        end_year: None,
        series_continuing: false,
        unplayed_item_count: None,
        resume_position_seconds: None,
        played_percentage: None,
        overview: None,
    }
}

fn test_sdk() -> (Sdk, tempfile::TempDir) {
    test_sdk_with(None, Arc::new(MemoryCredential::default()))
}

fn test_sdk_with_hooks(hooks: Option<Arc<dyn SdkHooks>>) -> (Sdk, tempfile::TempDir) {
    test_sdk_with(hooks, Arc::new(MemoryCredential::default()))
}

fn test_sdk_with(
    hooks: Option<Arc<dyn SdkHooks>>,
    credential: Arc<MemoryCredential>,
) -> (Sdk, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let sdk = Sdk::new(
        SdkConfig {
            storage_dir: dir.path().to_path_buf(),
            device_name: "sdk-test".to_owned(),
        },
        credential,
        hooks,
    )
    .expect("sdk should build");
    (sdk, dir)
}

#[tokio::test]
async fn token_minted_before_disconnect_is_stale_not_cancelled() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");

    sdk.disconnect().await.expect("disconnect");

    assert!(token.is_cancelled(), "scope end must cancel live tokens");
    let error = sdk
        .video_home(token)
        .await
        .expect_err("old-scope token must not run");
    assert_eq!(error, SdkError::Stale);
}

#[tokio::test]
async fn cancelled_token_rejects_before_dispatch() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");
    token.cancel();

    let error = sdk
        .video_home(token)
        .await
        .expect_err("cancelled token must not run");
    assert_eq!(error, SdkError::Cancelled);
}

#[tokio::test]
async fn queries_without_active_profile_fail_typed() {
    let (sdk, _dir) = test_sdk();
    let token = sdk.new_operation_token().expect("token");
    let error = sdk.video_home(token).await.expect_err("no active profile");
    assert_eq!(error, SdkError::NoActiveProfile);
}

#[tokio::test]
async fn profile_switch_invalidates_previous_scope_tokens() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let old_token = sdk.new_operation_token().expect("old token");

    sdk.adopt_test_session(test_session("grace", "https://other.example.test"));

    assert!(old_token.is_cancelled());
    let error = sdk
        .watchlist_items(old_token.clone())
        .await
        .expect_err("old-scope token must be stale");
    assert_eq!(error, SdkError::Stale);
    let error = sdk
        .watchlist_add(old_token, test_item("movie-1"))
        .await
        .expect_err("old-scope token must not mutate the new scope");
    assert_eq!(error, SdkError::Stale);

    let new_token = sdk.new_operation_token().expect("new token");
    sdk.watchlist_items(new_token)
        .await
        .expect("new-scope token must run");
}

#[tokio::test]
async fn watchlist_is_partitioned_per_profile_scope() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));

    let token = sdk.new_operation_token().expect("token");
    assert!(sdk
        .watchlist_add(token.clone(), test_item("movie-1"))
        .await
        .expect("add"));
    assert!(sdk
        .watchlist_contains(token, "movie-1".to_owned())
        .await
        .expect("contains"));

    // A different user on the same server must not see the record.
    sdk.adopt_test_session(test_session("grace", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");
    assert!(!sdk
        .watchlist_contains(token.clone(), "movie-1".to_owned())
        .await
        .expect("contains"));
    assert!(sdk.watchlist_items(token).await.expect("items").is_empty());

    // Switching back sees the original record again.
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");
    assert!(sdk
        .watchlist_contains(token, "movie-1".to_owned())
        .await
        .expect("contains"));
}

#[tokio::test]
async fn declined_handoff_hook_keeps_previous_profile() {
    struct Decline;
    #[async_trait::async_trait]
    impl SdkHooks for Decline {
        async fn before_profile_handoff(&self) -> bool {
            false
        }
    }

    let (sdk, _dir) = test_sdk_with_hooks(Some(Arc::new(Decline)));
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));

    let error = sdk.disconnect().await.expect_err("hook must decline");
    assert_eq!(error, SdkError::HandoffAborted);
    assert!(
        sdk.active_profile().is_some(),
        "declined teardown must keep the previous profile"
    );
}

#[tokio::test]
async fn handoff_hook_runs_once_per_transition() {
    struct Count(AtomicUsize);
    #[async_trait::async_trait]
    impl SdkHooks for Count {
        async fn before_profile_handoff(&self) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst);
            true
        }
    }

    let hooks = Arc::new(Count(AtomicUsize::new(0)));
    let counter = Arc::clone(&hooks);
    let (sdk, _dir) = test_sdk_with_hooks(Some(hooks));
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));

    sdk.disconnect().await.expect("disconnect");
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);

    // No active profile: disconnect must not invoke the hook again.
    sdk.disconnect().await.expect("idempotent disconnect");
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn sign_out_removes_credentials_and_scope_watchlist() {
    // The SDK's store and this handle share the same in-memory credential
    // blob, so a session saved here is visible to the SDK.
    let credential = Arc::new(MemoryCredential::default());
    let store = AuthStore::with_credential(credential.clone());
    let session = test_session("ada", "https://media.example.test");
    let key = SavedProfileKey::for_session(&session);
    store
        .save_session(jellypilot_auth::SensitiveSavedSession::from_saved_session(
            session.clone(),
        ))
        .await
        .expect("save session");

    let (sdk, _dir) = test_sdk_with(None, credential);
    sdk.adopt_test_session(session);

    let token = sdk.new_operation_token().expect("token");
    sdk.watchlist_add(token, test_item("movie-1"))
        .await
        .expect("add");

    let outcome = sdk
        .sign_out(key.as_str().to_owned(), true)
        .await
        .expect("sign out");
    assert!(outcome.remaining.is_empty());
    assert!(outcome.teardown_error.is_none());
    assert!(outcome.watchlist_error.is_none());
    assert!(sdk.active_profile().is_none());

    // The scope's watchlist records were deleted with the opt-in flag.
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");
    assert!(sdk.watchlist_items(token).await.expect("items").is_empty());
}

#[tokio::test]
async fn sign_out_without_watchlist_opt_in_keeps_records() {
    let credential = Arc::new(MemoryCredential::default());
    let store = AuthStore::with_credential(credential.clone());
    let session = test_session("ada", "https://media.example.test");
    let key = SavedProfileKey::for_session(&session);
    store
        .save_session(jellypilot_auth::SensitiveSavedSession::from_saved_session(
            session.clone(),
        ))
        .await
        .expect("save session");

    let (sdk, _dir) = test_sdk_with(None, credential);
    sdk.adopt_test_session(session);

    let token = sdk.new_operation_token().expect("token");
    sdk.watchlist_add(token, test_item("movie-1"))
        .await
        .expect("add");

    sdk.sign_out(key.as_str().to_owned(), false)
        .await
        .expect("sign out");

    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");
    assert!(sdk
        .watchlist_contains(token, "movie-1".to_owned())
        .await
        .expect("contains"));
}

#[tokio::test]
async fn close_cancels_tokens_and_rejects_new_operations() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");

    sdk.close();

    assert!(token.is_cancelled());
    assert_eq!(
        sdk.new_operation_token().expect_err("closed"),
        SdkError::Closed
    );
    let error = sdk.video_home(token).await.expect_err("closed");
    assert_eq!(error, SdkError::Closed);
}

#[tokio::test]
async fn concurrent_account_operations_fail_fast() {
    // The hook blocks inside the first disconnect so the second account
    // operation must fail fast instead of queueing behind the handoff.
    struct Gate {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
        armed: AtomicBool,
    }
    #[async_trait::async_trait]
    impl SdkHooks for Gate {
        async fn before_profile_handoff(&self) -> bool {
            self.entered.notify_one();
            if self.armed.swap(false, Ordering::SeqCst) {
                self.release.notified().await;
            }
            true
        }
    }

    let gate = Arc::new(Gate {
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
        armed: AtomicBool::new(true),
    });
    let (sdk, _dir) = test_sdk_with_hooks(Some(gate.clone()));
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));

    let mut first = Box::pin(sdk.disconnect());
    tokio::select! {
        _ = &mut first => panic!("disconnect should wait in the hook"),
        _ = gate.entered.notified() => {}
    }

    let error = sdk
        .disconnect()
        .await
        .expect_err("second account op must fail fast");
    assert_eq!(error, SdkError::OperationInProgress);

    gate.release.notify_one();
    first.await.expect("first disconnect completes");
}

fn test_candidate(
    user_id: &str,
    server_url: &str,
    storage_dir: &std::path::Path,
) -> ProfileCandidate {
    let client = Arc::new(JellyfinClient::with_storage_dir(storage_dir.to_path_buf()));
    client
        .login()
        .adopt_validated_session(&test_session(user_id, server_url));
    let candidate = ValidatedProfileCandidate::from_authenticated_client(client)
        .unwrap_or_else(|_| panic!("test session must validate"));
    ProfileCandidate::new(candidate)
}

#[tokio::test]
async fn close_during_activation_never_adopts_candidate() {
    // The hook blocks mid-activation; close() must win the race so the
    // candidate is never installed on a closed SDK.
    struct Gate {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    #[async_trait::async_trait]
    impl SdkHooks for Gate {
        async fn before_profile_handoff(&self) -> bool {
            self.entered.notify_one();
            self.release.notified().await;
            true
        }
    }

    let gate = Arc::new(Gate {
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let (sdk, dir) = test_sdk_with_hooks(Some(gate.clone()));
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let candidate = test_candidate("grace", "https://media.example.test", dir.path());

    let mut activation = Box::pin(sdk.activate_candidate(candidate, false));
    tokio::select! {
        _ = &mut activation => panic!("activation should wait in the hook"),
        _ = gate.entered.notified() => {}
    }

    sdk.close();
    gate.release.notify_one();

    let error = activation.await.expect_err("closed SDK must not adopt");
    assert_eq!(error, SdkError::Closed);
    assert!(
        sdk.active_profile().is_none(),
        "a closed SDK must not expose an active profile"
    );
}

#[tokio::test]
async fn failed_secure_deletion_preserves_active_scope() {
    // ADR 0033: protected deletion must succeed before disconnection. When
    // it fails, the active session and its watchlist stay untouched.
    let credential = Arc::new(MemoryCredential::default());
    let store = AuthStore::with_credential(credential.clone());
    let session = test_session("ada", "https://media.example.test");
    let key = SavedProfileKey::for_session(&session);
    store
        .save_session(jellypilot_auth::SensitiveSavedSession::from_saved_session(
            session.clone(),
        ))
        .await
        .expect("save session");

    credential.fail_delete.store(true, Ordering::SeqCst);
    let (sdk, _dir) = test_sdk_with(None, credential);
    sdk.adopt_test_session(session);

    let token = sdk.new_operation_token().expect("token");
    sdk.watchlist_add(token.clone(), test_item("movie-1"))
        .await
        .expect("add");

    let error = sdk
        .sign_out(key.as_str().to_owned(), true)
        .await
        .expect_err("deletion failure must surface");
    assert!(
        matches!(error, SdkError::Storage(_)),
        "expected Storage, got {error:?}"
    );
    assert!(
        sdk.active_profile().is_some(),
        "failed deletion must keep the active profile"
    );
    assert!(sdk
        .watchlist_contains(token, "movie-1".to_owned())
        .await
        .expect("watchlist stays readable"));
}

#[tokio::test]
async fn panicking_sign_out_hook_still_completes_committed_cleanup() {
    struct PanickingHook;
    #[async_trait::async_trait]
    impl SdkHooks for PanickingHook {
        async fn before_profile_handoff(&self) -> bool {
            panic!("platform teardown failed");
        }
    }

    let (sdk, dir) = test_sdk_with_hooks(Some(Arc::new(PanickingHook)));
    let candidate = test_candidate("ada", "https://media.example.test", dir.path());
    let activation = sdk
        .activate_candidate(candidate, true)
        .await
        .expect("activate");
    let token = sdk.new_operation_token().expect("token");
    sdk.watchlist_add(token, test_item("movie-1"))
        .await
        .expect("add");

    let outcome = sdk
        .sign_out(activation.profile.key, true)
        .await
        .expect("committed sign-out");
    assert_eq!(outcome.teardown_error, Some(SdkError::HandoffAborted));
    assert!(outcome.watchlist_error.is_none());
    assert!(outcome.remaining.is_empty());
    assert!(sdk.active_profile().is_none());
    let scope = ProfileScope::new(
        MediaServerProvider::Jellyfin,
        "https://media.example.test",
        "ada",
    )
    .expect("scope");
    let watchlist = WatchlistStore::load_in_dir(dir.path().to_path_buf()).expect("watchlist");
    assert!(watchlist.records_for(&scope).is_empty());
}

#[tokio::test]
async fn dropped_sign_out_future_completes_committed_cleanup() {
    // Once credential deletion starts, dropping the caller's future must not
    // abandon the session teardown or the opted-in watchlist cleanup.
    let credential = Arc::new(MemoryCredential::default());
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    *credential.delete_entered.lock().expect("gate lock") = Some(entered_tx);
    *credential.delete_gate.lock().expect("gate lock") = Some(release_rx);

    let store = AuthStore::with_credential(credential.clone());
    let session = test_session("ada", "https://media.example.test");
    let key = SavedProfileKey::for_session(&session);
    store
        .save_session(jellypilot_auth::SensitiveSavedSession::from_saved_session(
            session.clone(),
        ))
        .await
        .expect("save session");

    let (sdk, dir) = test_sdk_with(None, credential);
    sdk.adopt_test_session(session);
    let token = sdk.new_operation_token().expect("token");
    sdk.watchlist_add(token, test_item("movie-1"))
        .await
        .expect("add");

    let mut sign_out = Box::pin(sdk.sign_out(key.as_str().to_owned(), true));
    tokio::select! {
        _ = &mut sign_out => panic!("sign-out should wait in credential deletion"),
        result = entered_rx => result.expect("deletion must start"),
    }
    drop(sign_out);
    release_tx.send(()).expect("release deletion");

    // The committed transaction finishes on its own thread; poll the
    // observable end states instead of joining it.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let session_ended = sdk.active_profile().is_none();
        let watchlist_cleared = WatchlistStore::load_in_dir(dir.path().to_path_buf())
            .expect("watchlist store")
            .records_for(
                &ProfileScope::new(
                    MediaServerProvider::Jellyfin,
                    "https://media.example.test",
                    "ada",
                )
                .expect("scope"),
            )
            .is_empty();
        if session_ended && watchlist_cleared {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "committed cleanup did not settle"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn committed_activation_reports_persistence_failure() {
    // A save failure after the committed swap is a warning on the outcome,
    // not an activation error: the new profile stays active.
    let credential = Arc::new(MemoryCredential::default());
    credential.fail_write.store(true, Ordering::SeqCst);
    let (sdk, dir) = test_sdk_with(None, credential);
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let candidate = test_candidate("grace", "https://media.example.test", dir.path());

    let outcome = sdk
        .activate_candidate(candidate, true)
        .await
        .expect("persistence failure must not fail the committed activation");
    assert_eq!(outcome.profile.user_id, "grace");
    assert!(
        matches!(outcome.persistence_warning, Some(SdkError::Storage(_))),
        "expected a storage warning, got {:?}",
        outcome.persistence_warning
    );
    assert_eq!(
        sdk.active_profile().map(|profile| profile.user_id),
        Some("grace".to_owned())
    );
}

#[tokio::test]
async fn sign_out_reports_watchlist_failure_after_commit() {
    // The credential deletion commits; a failing watchlist cleanup is
    // reported on the outcome instead of failing the sign-out.
    let credential = Arc::new(MemoryCredential::default());
    let store = AuthStore::with_credential(credential.clone());
    let session = test_session("ada", "https://media.example.test");
    let key = SavedProfileKey::for_session(&session);
    store
        .save_session(jellypilot_auth::SensitiveSavedSession::from_saved_session(
            session.clone(),
        ))
        .await
        .expect("save session");

    let (sdk, dir) = test_sdk_with(None, credential);
    sdk.adopt_test_session(session);
    let token = sdk.new_operation_token().expect("token");
    sdk.watchlist_add(token, test_item("movie-1"))
        .await
        .expect("add");

    // Obstruct the file destination instead of relying on OS permissions,
    // which differ for Windows directories and privileged test runners.
    let watchlist_path = dir.path().join("watchlist.json");
    std::fs::remove_file(&watchlist_path).expect("remove fixture watchlist");
    std::fs::create_dir(&watchlist_path).expect("obstruct watchlist destination");

    let outcome = sdk
        .sign_out(key.as_str().to_owned(), true)
        .await
        .expect("watchlist failure must not fail the committed sign-out");
    assert!(outcome.remaining.is_empty());
    assert!(
        matches!(outcome.watchlist_error, Some(SdkError::Storage(_))),
        "expected a watchlist storage error, got {:?}",
        outcome.watchlist_error
    );
    assert!(sdk.active_profile().is_none());
}

#[tokio::test]
async fn image_target_rejects_replaced_scope() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().expect("token");
    let scope = token.scope_ref().expect("scope");
    assert!(sdk.is_scope_active(&scope));

    sdk.adopt_test_session(test_session("grace", "https://media.example.test"));

    assert!(!sdk.is_scope_active(&scope));
    let error = sdk
        .image_target(&scope, "image-1".to_owned(), None)
        .expect_err("a replaced scope must not resolve images");
    assert_eq!(error, SdkError::Stale);
}

#[tokio::test]
async fn close_during_committed_deletion_preserves_teardown() {
    // Once credential deletion starts, close() must not disconnect the
    // session or skip platform teardown: the committed transaction owns
    // cleanup until it settles.
    struct Count(AtomicUsize);
    #[async_trait::async_trait]
    impl SdkHooks for Count {
        async fn before_profile_handoff(&self) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst);
            true
        }
    }

    let credential = Arc::new(MemoryCredential::default());
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    *credential.delete_entered.lock().expect("gate lock") = Some(entered_tx);
    *credential.delete_gate.lock().expect("gate lock") = Some(release_rx);

    let store = AuthStore::with_credential(credential.clone());
    let session = test_session("ada", "https://media.example.test");
    let key = SavedProfileKey::for_session(&session);
    store
        .save_session(jellypilot_auth::SensitiveSavedSession::from_saved_session(
            session.clone(),
        ))
        .await
        .expect("save session");

    let hooks = Arc::new(Count(AtomicUsize::new(0)));
    let (sdk, _dir) = test_sdk_with(Some(hooks.clone()), credential.clone());
    sdk.adopt_test_session(session);

    let mut sign_out = Box::pin(sdk.sign_out(key.as_str().to_owned(), false));
    tokio::select! {
        _ = &mut sign_out => panic!("sign-out should wait in credential deletion"),
        result = entered_rx => result.expect("deletion must start"),
    }

    sdk.close();
    release_tx.send(()).expect("release deletion");

    let outcome = sign_out.await.expect("committed sign-out completes");
    assert!(outcome.remaining.is_empty());
    assert!(
        outcome.teardown_error.is_none(),
        "close must not skip the teardown hook: {:?}",
        outcome.teardown_error
    );
    assert_eq!(
        hooks.0.load(Ordering::SeqCst),
        1,
        "platform teardown must run once despite close"
    );
    assert!(
        credential.secret.lock().expect("secret lock").is_none(),
        "the committed deletion must have run"
    );
    assert!(sdk.active_profile().is_none());
}

#[tokio::test]
async fn close_during_handoff_hook_defers_session_teardown() {
    // A hook running when close() lands still observes the active profile:
    // close defers the disconnect to the transaction's cleanup guard.
    struct Gate {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
        sdk: Mutex<Option<Arc<Sdk>>>,
        observed_active: AtomicBool,
    }
    #[async_trait::async_trait]
    impl SdkHooks for Gate {
        async fn before_profile_handoff(&self) -> bool {
            self.entered.notify_one();
            self.release.notified().await;
            if let Some(sdk) = self.sdk.lock().expect("sdk lock").as_ref() {
                self.observed_active
                    .store(sdk.active_profile().is_some(), Ordering::SeqCst);
            }
            true
        }
    }

    let gate = Arc::new(Gate {
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
        sdk: Mutex::new(None),
        observed_active: AtomicBool::new(false),
    });
    let (sdk, _dir) = test_sdk_with_hooks(Some(gate.clone()));
    let sdk = Arc::new(sdk);
    *gate.sdk.lock().expect("sdk lock") = Some(Arc::clone(&sdk));
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));

    let mut disconnect = Box::pin(sdk.disconnect());
    tokio::select! {
        _ = &mut disconnect => panic!("disconnect should wait in the hook"),
        _ = gate.entered.notified() => {}
    }

    sdk.close();
    gate.release.notify_one();

    disconnect.await.expect("disconnect completes");
    assert!(
        gate.observed_active.load(Ordering::SeqCst),
        "the hook must still see the active profile after close"
    );
    assert!(sdk.active_profile().is_none());
}

#[tokio::test]
async fn close_after_adoption_reports_committed_activation() {
    // The swap commits before persistence is awaited; a close during the
    // credential write must not turn the committed activation into
    // Err(Closed).
    let credential = Arc::new(MemoryCredential::default());
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    *credential.write_entered.lock().expect("gate lock") = Some(entered_tx);
    *credential.write_gate.lock().expect("gate lock") = Some(release_rx);

    let (sdk, dir) = test_sdk_with(None, credential);
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let candidate = test_candidate("grace", "https://media.example.test", dir.path());

    let mut activation = Box::pin(sdk.activate_candidate(candidate, true));
    tokio::select! {
        _ = &mut activation => panic!("activation should wait in the credential write"),
        result = entered_rx => result.expect("persistence must start"),
    }

    sdk.close();
    release_tx.send(()).expect("release write");

    let outcome = activation
        .await
        .expect("a committed activation reports its outcome");
    assert_eq!(outcome.profile.user_id, "grace");
    assert!(outcome.persistence_warning.is_none());
    assert!(
        sdk.active_profile().is_none(),
        "the closed session is cleared once the transaction settles"
    );
}

#[tokio::test]
async fn sign_out_beginning_after_close_skips_deletion() {
    // close() lands after sign_out is admitted but before the transaction
    // begins: it must not delete credentials.
    let credential = Arc::new(MemoryCredential::default());
    let store = AuthStore::with_credential(credential.clone());
    let session = test_session("ada", "https://media.example.test");
    let key = SavedProfileKey::for_session(&session);
    store
        .save_session(jellypilot_auth::SensitiveSavedSession::from_saved_session(
            session.clone(),
        ))
        .await
        .expect("save session");

    // Arm the read gate after setup so the first gated read is sign_out's
    // saved-profile snapshot.
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    *credential.read_entered.lock().expect("gate lock") = Some(entered_tx);
    *credential.read_gate.lock().expect("gate lock") = Some(release_rx);

    let (sdk, _dir) = test_sdk_with(None, credential.clone());
    sdk.adopt_test_session(session);

    let mut sign_out = Box::pin(sdk.sign_out(key.as_str().to_owned(), false));
    tokio::select! {
        _ = &mut sign_out => panic!("sign-out should wait in the snapshot read"),
        result = entered_rx => result.expect("snapshot read must start"),
    }

    sdk.close();
    release_tx.send(()).expect("release read");

    let error = sign_out
        .await
        .expect_err("closed SDK rejects the transaction");
    assert_eq!(error, SdkError::Closed);
    assert!(
        credential.secret.lock().expect("secret lock").is_some(),
        "a transaction beginning after close must not delete credentials"
    );
}

/// Terminal-outcome recorder for Quick Connect tests.
#[derive(Default)]
struct QuickConnectProbe {
    outcomes: Mutex<Vec<String>>,
    completed: tokio::sync::Notify,
}

impl QuickConnectProbe {
    fn outcomes(&self) -> Vec<String> {
        self.outcomes.lock().expect("outcomes lock").clone()
    }
}

impl QuickConnectListener for QuickConnectProbe {
    fn on_code(&self, _code: String) {}

    fn on_approving(&self) {}

    fn on_completed(&self, outcome: QuickConnectOutcome) {
        self.outcomes
            .lock()
            .expect("outcomes lock")
            .push(format!("{outcome:?}"));
        self.completed.notify_one();
    }
}

/// A local server that accepts connections but never responds, so the flow
/// stays in its first request until cancelled. The returned listener must
/// be held for the flow's lifetime.
fn hanging_server() -> (String, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind hanging server");
    let port = listener.local_addr().expect("local addr").port();
    (format!("http://127.0.0.1:{port}"), listener)
}

async fn await_terminal(probe: &QuickConnectProbe) {
    tokio::time::timeout(Duration::from_secs(10), probe.completed.notified())
        .await
        .expect("terminal outcome must be delivered");
}

#[tokio::test]
async fn quick_connect_cancel_and_drop_delivers_terminal_once() {
    // cancel() followed by an immediate drop aborts the task; the terminal
    // owner still delivers exactly one outcome.
    let (sdk, _dir) = test_sdk();
    let probe = Arc::new(QuickConnectProbe::default());
    let (server_url, _listener) = hanging_server();
    let session = sdk
        .start_quick_connect(MediaServerProvider::Jellyfin, server_url, probe.clone())
        .expect("start quick connect");

    session.cancel();
    drop(session);

    await_terminal(&probe).await;
    assert_eq!(
        probe.outcomes(),
        vec!["QuickConnectOutcome::Cancelled".to_owned()]
    );
}

#[tokio::test]
async fn quick_connect_close_delivers_terminal_once() {
    // Closing the SDK cancels the flow and shuts down its runtime; the
    // listener still receives exactly one terminal outcome.
    let (sdk, _dir) = test_sdk();
    let probe = Arc::new(QuickConnectProbe::default());
    let (server_url, _listener) = hanging_server();
    let session = sdk
        .start_quick_connect(MediaServerProvider::Jellyfin, server_url, probe.clone())
        .expect("start quick connect");

    sdk.close();

    await_terminal(&probe).await;
    assert_eq!(
        probe.outcomes(),
        vec!["QuickConnectOutcome::Cancelled".to_owned()]
    );
    drop(session);
}

#[tokio::test]
async fn quick_connect_with_handle_close_cancels_flow() {
    // An embedder-owned runtime survives close(); the flow is cancelled
    // through the SDK's shutdown token instead of task destruction.
    let dir = tempfile::tempdir().expect("temp dir");
    let sdk = Sdk::with_handle(
        SdkConfig {
            storage_dir: dir.path().to_path_buf(),
            device_name: "sdk-test".to_owned(),
        },
        AuthStore::with_credential(Arc::new(MemoryCredential::default())),
        None,
        tokio::runtime::Handle::current(),
    );
    let probe = Arc::new(QuickConnectProbe::default());
    let (server_url, _listener) = hanging_server();
    let session = sdk
        .start_quick_connect(MediaServerProvider::Jellyfin, server_url, probe.clone())
        .expect("start quick connect");

    sdk.close();

    await_terminal(&probe).await;
    assert_eq!(
        probe.outcomes(),
        vec!["QuickConnectOutcome::Cancelled".to_owned()]
    );
    drop(session);
}

#[tokio::test]
async fn quick_connect_terminal_releases_permit_for_reentry() {
    // The account permit is released before on_completed, so the listener
    // can begin the next account operation from inside the callback.
    struct Reentrant {
        sdk: Arc<Sdk>,
        candidate: Mutex<Option<ProfileCandidate>>,
        handle: tokio::runtime::Handle,
        result: Mutex<Option<tokio::sync::oneshot::Sender<Result<ActivationOutcome, SdkError>>>>,
    }
    impl QuickConnectListener for Reentrant {
        fn on_code(&self, _code: String) {}

        fn on_approving(&self) {}

        fn on_completed(&self, _outcome: QuickConnectOutcome) {
            let sdk = Arc::clone(&self.sdk);
            let candidate = self
                .candidate
                .lock()
                .expect("candidate lock")
                .take()
                .expect("candidate installed");
            let result = self.result.lock().expect("result lock").take();
            let _task = self.handle.spawn(async move {
                let outcome = sdk.activate_candidate(candidate, false).await;
                if let Some(result) = result {
                    let _ = result.send(outcome);
                }
            });
        }
    }

    let (sdk, dir) = test_sdk();
    let sdk = Arc::new(sdk);
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let listener = Arc::new(Reentrant {
        sdk: Arc::clone(&sdk),
        candidate: Mutex::new(Some(test_candidate(
            "grace",
            "https://media.example.test",
            dir.path(),
        ))),
        handle: tokio::runtime::Handle::current(),
        result: Mutex::new(Some(result_tx)),
    });
    let (server_url, _listener) = hanging_server();
    let session = sdk
        .start_quick_connect(MediaServerProvider::Jellyfin, server_url, listener)
        .expect("start quick connect");

    session.cancel();

    let outcome = tokio::time::timeout(Duration::from_secs(10), result_rx)
        .await
        .expect("reentrant activation must run")
        .expect("reentrant activation reports")
        .expect("the permit must be free inside on_completed");
    assert_eq!(outcome.profile.user_id, "grace");
    assert_eq!(
        sdk.active_profile().map(|profile| profile.user_id),
        Some("grace".to_owned())
    );
    drop(session);
}

#[tokio::test]
async fn scope_ref_without_active_profile_is_typed() {
    let (sdk, _dir) = test_sdk();
    let token = sdk.new_operation_token().expect("token");
    let error = token.scope_ref().expect_err("no active profile");
    assert_eq!(error, SdkError::NoActiveProfile);
}
