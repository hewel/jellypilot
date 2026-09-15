//! Account lifecycle: authentication, validated handoff, disconnect, sign-out.
//!
//! Ordering follows ADR 0033: a candidate is validated in isolation, the
//! platform teardown hook settles while the previous profile is still
//! active, and only then does the SDK swap the active client and advance the
//! scope epoch. A declined or failed teardown keeps the previous profile.
//!
//! Once irreversible work starts (credential deletion, session teardown) the
//! remainder of the transaction runs on an SDK-owned thread holding the
//! account permit, so a dropped caller future or a closed SDK cannot abandon
//! cleanup halfway. Committed outcomes are reported as structured results —
//! never as errors that imply the committed step was rolled back.

use std::sync::Arc;

use jellypilot_auth::login::{validate_saved_profile_in, ValidatedProfileCandidate};
use jellypilot_auth::{
    AuthStorageError, AuthStore, SavedProfileKey, SavedProfileSummary, SavedProfilesSnapshot,
    SensitiveSavedSession,
};
use jellypilot_media_server::{Credentials, JellyfinClient, MediaServerProvider};

use crate::{ActiveProfile, ProfileCandidate, Sdk, SdkError, SdkInner};

/// Committed result of [`Sdk::activate_candidate`].
///
/// The profile swap is committed before this value is produced: `profile`
/// is the live active profile even when `persistence_warning` is set. The
/// warning means the session or the startup-restore selection could not be
/// persisted; the activation itself is not undone.
#[derive(Debug)]
pub struct ActivationOutcome {
    /// The newly active profile.
    pub profile: ActiveProfile,
    /// Non-fatal persistence failure recorded after the committed swap.
    pub persistence_warning: Option<SdkError>,
}

/// Committed result of [`Sdk::sign_out`].
///
/// The credential deletion is committed before this value is produced.
/// `teardown_error` and `watchlist_error` report post-commit cleanup that
/// failed independently; neither implies the deletion was rolled back.
#[derive(Debug)]
pub struct SignOutOutcome {
    /// Saved profiles remaining after the deletion.
    pub remaining: Vec<SavedProfileSummary>,
    /// Startup-restore selection after the deletion, when one remains.
    pub last_activated_key: Option<SavedProfileKey>,
    /// Platform teardown failure recorded after the committed deletion.
    pub teardown_error: Option<SdkError>,
    /// Watchlist cleanup failure recorded after the committed deletion.
    pub watchlist_error: Option<SdkError>,
}

/// Committed result of [`Sdk::remove_saved_profile`].
///
/// Carries the storage mutation's own result so a later reload failure
/// cannot masquerade as a failed deletion.
#[derive(Debug)]
pub struct ProfileRemovalOutcome {
    /// Saved profiles remaining after the deletion.
    pub remaining: Vec<SavedProfileSummary>,
    /// Startup-restore selection after the deletion, when one remains.
    pub last_activated_key: Option<SavedProfileKey>,
}

impl Sdk {
    /// Saved profiles plus the only profile eligible for startup restore.
    pub async fn saved_profiles(&self) -> Result<SavedProfilesSnapshot, SdkError> {
        self.inner.check_open()?;
        self.inner
            .store
            .load_profiles_snapshot()
            .await
            .map_err(SdkError::from)
    }

    /// Authenticates with a password and returns a validated candidate.
    ///
    /// The active profile is untouched until [`Sdk::activate_candidate`].
    pub async fn password_login(
        &self,
        provider: MediaServerProvider,
        server_url: String,
        username: String,
        password: String,
    ) -> Result<ProfileCandidate, SdkError> {
        self.inner.check_open()?;
        let _permit = self
            .inner
            .account_op
            .clone()
            .try_lock_owned()
            .map_err(|_| SdkError::OperationInProgress)?;
        let server_url = jellypilot_auth::login::validate_server_url(&server_url, provider)
            .map_err(SdkError::InvalidInput)?;
        if username.trim().is_empty() {
            return Err(SdkError::InvalidInput("a username is required".to_owned()));
        }
        let credentials = AuthStore::protect_credentials(Credentials {
            provider,
            server_url,
            username,
            password,
        });
        let client = self.new_client();
        client
            .login()
            .authenticate(&credentials)
            .await
            .map_err(|error| SdkError::Authentication(error.to_string()))?;
        let candidate = ValidatedProfileCandidate::from_authenticated_client(Arc::new(client))
            .map_err(|error| SdkError::Authentication(error.to_string()))?;
        Ok(ProfileCandidate::new(candidate))
    }

    /// Validates a saved profile's stored session and returns a candidate.
    ///
    /// The stored credential is refreshed against the server on an isolated
    /// client; the active profile is untouched.
    pub async fn restore_saved_profile(&self, key: String) -> Result<ProfileCandidate, SdkError> {
        self.inner.check_open()?;
        let _permit = self
            .inner
            .account_op
            .clone()
            .try_lock_owned()
            .map_err(|_| SdkError::OperationInProgress)?;
        let candidate = validate_saved_profile_in(
            self.inner.store.clone(),
            SavedProfileKey::from_raw(key),
            self.inner.config.storage_dir.clone(),
        )
        .await
        .map_err(|error| match error {
            jellypilot_auth::login::LoginError::AuthStorage(storage) => SdkError::from(storage),
            jellypilot_auth::login::LoginError::Request(message) => {
                SdkError::Authentication(message)
            }
        })?;
        Ok(ProfileCandidate::new(candidate))
    }

    /// Activates a validated candidate as the single active profile.
    ///
    /// Runs the platform teardown hook while the previous profile is still
    /// active, then swaps the client and advances the scope epoch, cancelling
    /// every operation token minted under the previous scope. When
    /// `save_profile` is set the refreshed session is persisted for startup
    /// restore; a persistence failure is reported through
    /// [`ActivationOutcome::persistence_warning`] and does not undo the
    /// committed activation.
    ///
    /// Once the teardown hook starts, the transaction continues on an
    /// SDK-owned thread: dropping the returned future does not abandon the
    /// handoff. A [`SdkError::Closed`] result means the SDK was closed before
    /// the candidate could be adopted; the candidate is then discarded.
    pub async fn activate_candidate(
        &self,
        mut candidate: ProfileCandidate,
        save_profile: bool,
    ) -> Result<ActivationOutcome, SdkError> {
        self.inner.check_open()?;
        let permit = self
            .inner
            .account_op
            .clone()
            .try_lock_owned()
            .map_err(|_| SdkError::OperationInProgress)?;
        let candidate = candidate.take()?;
        let inner = Arc::clone(&self.inner);
        let receiver = self.inner.spawn_committed(move || {
            let inner = inner;
            async move {
                let _permit = permit;
                // A transaction beginning after close must not adopt.
                inner.check_open()?;
                inner.run_handoff_hook().await?;

                let (key, scope, client, session) = candidate.into_parts();
                let profile = {
                    // The closed check and the adoption commit share one lock
                    // acquisition: close() cannot slip between them and leave
                    // a live authenticated profile on a closed SDK.
                    let mut state = inner.state.lock().map_err(|_| SdkError::Closed)?;
                    if state.closed {
                        return Err(SdkError::Closed);
                    }
                    let old_epoch = state.epoch;
                    state.epoch = state.epoch.saturating_add(1);
                    if let Some(previous) = state.active.take() {
                        previous.client.login().disconnect();
                    }
                    let active = crate::ActiveSession {
                        key: key.clone(),
                        scope,
                        client: Arc::clone(&client),
                        _session: session,
                    };
                    // The committed outcome is captured at the swap: a close
                    // during the persistence waits below must not turn the
                    // committed activation into Err(Closed).
                    let profile = SdkInner::active_profile_for(&active);
                    state.active = Some(active);
                    if let Some(tokens) = state.tokens.remove(&old_epoch) {
                        for token in tokens {
                            if let Some(token) = token.upgrade() {
                                token.cancel();
                            }
                        }
                    }
                    profile
                };

                // The swap is committed. Persistence failures from here are
                // warnings on the committed outcome, not activation errors.
                let mut persistence_warning = None;
                if save_profile {
                    match SensitiveSavedSession::from_client(&client) {
                        Some(stored) => {
                            if let Err(error) = inner.store.save_session(stored).await {
                                persistence_warning = Some(SdkError::from(error));
                            }
                        }
                        None => {
                            persistence_warning = Some(SdkError::Storage(
                                "the activated session could not be captured".to_owned(),
                            ));
                        }
                    }
                }
                // Best-effort: records this profile as the startup-restore
                // candidate. A missing saved profile (save_profile = false)
                // is not a failure.
                match inner.store.record_successful_activation(key).await {
                    Ok(()) | Err(AuthStorageError::ProfileNotFound) => {}
                    Err(error) => {
                        if persistence_warning.is_none() {
                            persistence_warning = Some(SdkError::from(error));
                        }
                    }
                }

                Ok(ActivationOutcome {
                    profile,
                    persistence_warning,
                })
            }
        })?;
        receiver
            .await
            .map_err(|_| SdkError::Request("the activation task failed unexpectedly".to_owned()))?
    }

    /// Ends the active session without removing its saved credentials.
    ///
    /// Distinct from [`Sdk::sign_out`]: the saved profile remains restorable.
    /// Idempotent when no profile is active. Once the teardown hook starts,
    /// the transaction continues on an SDK-owned thread even if the returned
    /// future is dropped.
    pub async fn disconnect(&self) -> Result<(), SdkError> {
        self.inner.check_open()?;
        let permit = self
            .inner
            .account_op
            .clone()
            .try_lock_owned()
            .map_err(|_| SdkError::OperationInProgress)?;
        if self
            .inner
            .state
            .lock()
            .map_err(|_| SdkError::Closed)?
            .active
            .is_none()
        {
            return Ok(());
        }
        let inner = Arc::clone(&self.inner);
        let receiver = self.inner.spawn_committed(move || {
            let inner = inner;
            async move {
                let _permit = permit;
                inner.check_open()?;
                inner.run_handoff_hook().await?;
                inner.end_active_session();
                Ok(())
            }
        })?;
        receiver
            .await
            .map_err(|_| SdkError::Request("the disconnect task failed unexpectedly".to_owned()))?
    }

    /// Signs out: removes the saved credentials, ends the active session when
    /// it matches `key`, and optionally deletes that profile's device-local
    /// watchlist.
    ///
    /// Per ADR 0033 the protected credential deletion is performed and
    /// acknowledged first, while the in-memory session is still available;
    /// teardown and watchlist cleanup then run as committed follow-through.
    /// Once deletion starts the transaction cannot be cancelled as though it
    /// never ran: it continues on an SDK-owned thread even if the returned
    /// future is dropped or the SDK is closed, and post-commit failures are
    /// reported on [`SignOutOutcome`] rather than as rollback errors.
    pub async fn sign_out(
        &self,
        key: String,
        delete_watchlist: bool,
    ) -> Result<SignOutOutcome, SdkError> {
        self.inner.check_open()?;
        let permit = self
            .inner
            .account_op
            .clone()
            .try_lock_owned()
            .map_err(|_| SdkError::OperationInProgress)?;
        let key = SavedProfileKey::from_raw(key);

        // Resolve the scope before removal so watchlist cleanup can run even
        // for a profile that is not currently active.
        let snapshot = self
            .inner
            .store
            .load_profiles_snapshot()
            .await
            .map_err(SdkError::from)?;
        let scope = snapshot
            .profiles()
            .iter()
            .find(|profile| profile.key == key)
            .map(|profile| profile.scope().clone())
            .ok_or(SdkError::ProfileNotFound)?;
        let was_last_activated = snapshot.last_successfully_activated() == Some(&key);

        let inner = Arc::clone(&self.inner);
        let receiver = self.inner.spawn_committed(move || {
            let inner = inner;
            async move {
                let _permit = permit;
                // A transaction beginning after close must not delete
                // credentials.
                inner.check_open()?;
                let is_active = inner
                    .state
                    .lock()
                    .map_err(|_| SdkError::Closed)?
                    .active
                    .as_ref()
                    .is_some_and(|active| active.key == key);

                // Irreversible step: delete the protected credentials while
                // the in-memory session is still available for teardown.
                let remaining = inner
                    .store
                    .remove_profile(key)
                    .await
                    .map_err(SdkError::from)?;

                // Post-commit: platform teardown keeps the in-memory session
                // long enough to report and stop playback, then the session
                // ends. A declined or failed teardown is recorded, not
                // treated as a rollback of the deletion.
                let mut teardown_error = None;
                if is_active {
                    if let Err(error) = inner.run_handoff_hook().await {
                        teardown_error = Some(error);
                    }
                    inner.end_active_session();
                }

                let mut watchlist_error = None;
                if delete_watchlist {
                    if let Err(error) = inner.with_committed_watchlist(|store| {
                        store
                            .remove_scope(&scope)
                            .map_err(|error| SdkError::Storage(error.to_string()))?;
                        Ok(())
                    }) {
                        watchlist_error = Some(error);
                    }
                }

                Ok(SignOutOutcome {
                    remaining,
                    last_activated_key: if was_last_activated {
                        None
                    } else {
                        snapshot.last_successfully_activated().cloned()
                    },
                    teardown_error,
                    watchlist_error,
                })
            }
        })?;
        receiver
            .await
            .map_err(|_| SdkError::Request("the sign-out task failed unexpectedly".to_owned()))?
    }

    /// Removes a saved profile that is not currently active.
    ///
    /// Removing the active profile is a sign-out; callers must use
    /// [`Sdk::sign_out`] so teardown ordering is preserved. The returned
    /// outcome is the storage mutation's own result.
    pub async fn remove_saved_profile(
        &self,
        key: String,
    ) -> Result<ProfileRemovalOutcome, SdkError> {
        self.inner.check_open()?;
        let permit = self
            .inner
            .account_op
            .clone()
            .try_lock_owned()
            .map_err(|_| SdkError::OperationInProgress)?;
        let key = SavedProfileKey::from_raw(key);
        {
            let state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
            if state
                .active
                .as_ref()
                .is_some_and(|active| active.key == key)
            {
                return Err(SdkError::InvalidInput(
                    "the active profile must be signed out, not removed".to_owned(),
                ));
            }
        }
        let snapshot = self
            .inner
            .store
            .load_profiles_snapshot()
            .await
            .map_err(SdkError::from)?;
        let was_last_activated = snapshot.last_successfully_activated() == Some(&key);

        let inner = Arc::clone(&self.inner);
        let receiver = self.inner.spawn_committed(move || {
            let inner = inner;
            async move {
                let _permit = permit;
                inner.check_open()?;
                let remaining = inner
                    .store
                    .remove_profile(key)
                    .await
                    .map_err(SdkError::from)?;
                Ok(ProfileRemovalOutcome {
                    remaining,
                    last_activated_key: if was_last_activated {
                        None
                    } else {
                        snapshot.last_successfully_activated().cloned()
                    },
                })
            }
        })?;
        receiver
            .await
            .map_err(|_| SdkError::Request("the removal task failed unexpectedly".to_owned()))?
    }

    fn new_client(&self) -> JellyfinClient {
        let client = JellyfinClient::with_storage_dir(self.inner.config.storage_dir.clone());
        client.set_device_name(self.inner.config.device_name.clone());
        client
    }
}
