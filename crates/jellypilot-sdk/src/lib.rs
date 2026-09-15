//! Shared business-operation SDK for JellyPilot frontends.
//!
//! The SDK orchestrates the existing auth, core, and media-server domain
//! crates instead of reimplementing them. It owns the single active profile
//! scope, account-operation ordering, per-operation cancellation, and
//! stale-result rejection. Desktop consumes this API directly in Rust;
//! Android reaches it through `jellypilot-ffi`.
//!
//! Lifetime rules:
//! - One active profile scope at a time. Activating a candidate, disconnect,
//!   and sign-out advance the scope epoch, which cancels every live
//!   [`OperationToken`] from the previous scope and makes their late results
//!   stale.
//! - Account operations are serialized; a second concurrent account
//!   operation fails with [`SdkError::OperationInProgress`].
//! - Cancelling an operation never claims a server mutation was rolled back;
//!   it only stops the result from being committed to the caller.
//! - Once irreversible account work starts (credential deletion, session
//!   teardown), the transaction finishes on an SDK-owned thread even when the
//!   caller drops its future or the SDK is closed.

mod account;
mod error;
mod hooks;
mod image;
mod query;
mod quick_connect;
#[cfg(test)]
mod tests;
mod watchlist;

use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

use futures_util::FutureExt;
use jellypilot_auth::{AuthStore, SecureCredential};
use jellypilot_core::watchlist::{ProfileScope, WatchlistStore};
use jellypilot_media_server::JellyfinClient;
use tokio::runtime::Handle;
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

pub use account::{ActivationOutcome, ProfileRemovalOutcome, SignOutOutcome};
pub use error::SdkError;
pub use hooks::SdkHooks;
pub use image::LibraryImageTarget;
pub use quick_connect::{QuickConnectListener, QuickConnectOutcome, QuickConnectSession};

/// Host-provided configuration for one SDK instance.
#[derive(Clone, Debug)]
pub struct SdkConfig {
    /// App-private directory for SDK-owned persistence (watchlist, client
    /// caches). Android passes its files directory; desktop passes its
    /// configuration directory.
    pub storage_dir: PathBuf,
    /// Device name reported to the media server.
    pub device_name: String,
}

/// Identity of the currently active profile scope.
#[derive(Clone, Debug)]
pub struct ActiveProfile {
    pub key: String,
    pub provider: jellypilot_media_server::MediaServerProvider,
    pub server_url: String,
    pub server_name: Option<String>,
    pub user_id: String,
    pub user_name: String,
    pub capabilities: jellypilot_media_server::ProviderCapabilities,
}

/// A validated authentication that is not yet the active profile.
///
/// Constructing a candidate never mutates the active connection. It is
/// single-use: [`Sdk::activate_candidate`] consumes it, and dropping it
/// discards the validated session without side effects.
pub struct ProfileCandidate {
    inner: Option<jellypilot_auth::login::ValidatedProfileCandidate>,
    key: String,
    provider: jellypilot_media_server::MediaServerProvider,
    account_title: String,
}

impl ProfileCandidate {
    fn new(candidate: jellypilot_auth::login::ValidatedProfileCandidate) -> Self {
        Self {
            key: candidate.key().as_str().to_owned(),
            provider: candidate.scope().provider(),
            account_title: candidate.account_title(),
            inner: Some(candidate),
        }
    }

    /// Stable saved-profile key for this candidate.
    pub fn key(&self) -> &str {
        &self.key
    }

    pub const fn provider(&self) -> jellypilot_media_server::MediaServerProvider {
        self.provider
    }

    /// Redacted `user@server` label suitable for confirmation UI.
    pub fn account_title(&self) -> &str {
        &self.account_title
    }

    /// Discards the candidate without activating it.
    pub fn discard(&mut self) {
        self.inner.take();
    }

    fn take(&mut self) -> Result<jellypilot_auth::login::ValidatedProfileCandidate, SdkError> {
        self.inner.take().ok_or(SdkError::InvalidInput(
            "this profile candidate was already consumed".to_owned(),
        ))
    }
}

impl std::fmt::Debug for ProfileCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProfileCandidate")
            .field("key", &"[redacted]")
            .field("provider", &self.provider)
            .finish()
    }
}

/// Identity of the profile scope an [`OperationToken`] was minted under.
///
/// `generation` is the scope epoch at mint time. Passing the reference back
/// to [`Sdk::is_scope_active`] or [`Sdk::image_target`] lets the SDK bind a
/// follow-up operation to the exact scope that issued it, so work queued
/// under one account cannot resolve against a later account's credentials.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileScopeRef {
    /// Stable saved-profile key of the scope's profile.
    pub profile_key: String,
    /// Scope epoch captured when the token was minted.
    pub generation: u64,
}

/// Cancellation and staleness handle for one scoped operation.
///
/// Tokens are bound to the profile scope that created them. Cancelling the
/// token aborts the operation's wait; advancing the scope (activation,
/// disconnect, sign-out, close) cancels every live token and makes their
/// results [`SdkError::Stale`].
pub struct OperationToken {
    epoch: u64,
    scope: ProfileScopeRef,
    cancel: Arc<CancellationToken>,
    inner: Weak<SdkInner>,
}

impl OperationToken {
    /// Cancels this operation. Idempotent.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Whether the token was cancelled or its scope ended.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// The profile scope this token was minted under.
    ///
    /// Fails with [`SdkError::NoActiveProfile`] when the token was minted
    /// while no profile was active.
    pub fn scope_ref(&self) -> Result<ProfileScopeRef, SdkError> {
        if self.scope.profile_key.is_empty() {
            return Err(SdkError::NoActiveProfile);
        }
        Ok(self.scope.clone())
    }
}

impl std::fmt::Debug for OperationToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationToken")
            .field("epoch", &self.epoch)
            .field("cancelled", &self.cancel.is_cancelled())
            .finish()
    }
}

impl Drop for OperationToken {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            if let Ok(mut state) = inner.state.lock() {
                if let Some(live) = state.tokens.get_mut(&self.epoch) {
                    live.retain(|weak| match weak.upgrade() {
                        Some(token) => !Arc::ptr_eq(&token, &self.cancel),
                        None => false,
                    });
                    if live.is_empty() {
                        state.tokens.remove(&self.epoch);
                    }
                }
            }
        }
    }
}

struct ActiveSession {
    key: jellypilot_auth::SavedProfileKey,
    scope: ProfileScope,
    client: Arc<JellyfinClient>,
    /// Held for RAII: dropping the session zeroizes its access token.
    _session: jellypilot_auth::SensitiveSavedSession,
}

struct SdkState {
    epoch: u64,
    active: Option<ActiveSession>,
    tokens: HashMap<u64, Vec<Weak<CancellationToken>>>,
    watchlist: Option<WatchlistStore>,
    closed: bool,
    /// Number of committed account transactions currently owning session
    /// teardown. While nonzero, [`Sdk::close`] leaves the active session in
    /// place so platform teardown and post-deletion cleanup keep their
    /// authentication; the last guard to settle performs the disconnect.
    committed_cleanup: u32,
}

/// RAII owner of the committed-transaction cleanup boundary.
///
/// Claimed synchronously when a committed transaction is admitted and held
/// by its SDK-owned thread until the transaction settles — including panic
/// unwind and thread-spawn failure, where the dropped guard still performs
/// the deferred teardown. While a guard is live, [`Sdk::close`] marks the
/// SDK closed and cancels cancellable work but leaves the active session
/// for the transaction; dropping the last guard after close disconnects it.
struct CommittedCleanup {
    inner: Arc<SdkInner>,
}

impl CommittedCleanup {
    /// Claims cleanup ownership for a transaction admitted before close.
    ///
    /// Fails with [`SdkError::Closed`] when the SDK is already closed: a
    /// transaction beginning after close must not start irreversible work
    /// such as credential deletion.
    fn begin(inner: &Arc<SdkInner>) -> Result<Self, SdkError> {
        let mut state = inner.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        state.committed_cleanup = state.committed_cleanup.saturating_add(1);
        Ok(Self {
            inner: Arc::clone(inner),
        })
    }
}

impl Drop for CommittedCleanup {
    fn drop(&mut self) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        state.committed_cleanup = state.committed_cleanup.saturating_sub(1);
        if state.committed_cleanup == 0 && state.closed {
            if let Some(active) = state.active.take() {
                active.client.login().disconnect();
            }
        }
    }
}

pub(crate) struct SdkInner {
    handle: Handle,
    store: AuthStore,
    config: SdkConfig,
    hooks: Option<Arc<dyn SdkHooks>>,
    state: Mutex<SdkState>,
    /// Serializes account operations (login, activation, disconnect,
    /// sign-out, credential removal) so a second one fails fast instead of
    /// interleaving with an in-flight handoff. Owned guards let committed
    /// transactions carry the permit into SDK-owned execution.
    account_op: Arc<AsyncMutex<()>>,
    /// Cancelled by [`Sdk::close`]. Flows that outlive their spawn site —
    /// Quick Connect sessions — derive child tokens from it so close
    /// terminates them even when the runtime is embedder-owned.
    shutdown: CancellationToken,
}

impl SdkInner {
    fn check_open(&self) -> Result<(), SdkError> {
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        Ok(())
    }

    /// Cancels every token minted for `epoch` and forgets them.
    #[cfg(any(test, feature = "test-utils"))]
    fn cancel_epoch(&self, epoch: u64) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(tokens) = state.tokens.remove(&epoch) {
                for token in tokens {
                    if let Some(token) = token.upgrade() {
                        token.cancel();
                    }
                }
            }
        }
    }

    /// Runs `operation` against the watchlist store under the state lock.
    ///
    /// Token epoch validation, scope capture, and the store read or mutation
    /// are serialized by the single lock: a concurrent activation cannot
    /// redirect the operation into the next scope's records. The store does
    /// synchronous filesystem work, so callers must run this off the
    /// consumer's thread (see [`Sdk::run_watchlist`]).
    fn with_scoped_watchlist<T>(
        &self,
        token: &OperationToken,
        operation: impl FnOnce(&mut WatchlistStore, &ProfileScope) -> Result<T, SdkError>,
    ) -> Result<T, SdkError> {
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        if state.epoch != token.epoch {
            return Err(SdkError::Stale);
        }
        if token.is_cancelled() {
            return Err(SdkError::Cancelled);
        }
        let scope = state
            .active
            .as_ref()
            .map(|active| active.scope.clone())
            .ok_or(SdkError::NoActiveProfile)?;
        if state.watchlist.is_none() {
            state.watchlist = Some(
                WatchlistStore::load_in_dir(self.config.storage_dir.clone())
                    .map_err(|error| SdkError::Storage(error.to_string()))?,
            );
        }
        let store = state
            .watchlist
            .as_mut()
            .expect("watchlist store was just initialized");
        operation(store, &scope)
    }

    /// Runs committed cleanup against the watchlist store.
    ///
    /// Unlike [`Self::with_scoped_watchlist`] this is not bound to a live
    /// scope: sign-out cleanup must still run after the scope ended or the
    /// SDK closed, because the credential deletion it follows is
    /// irreversible.
    fn with_committed_watchlist<T>(
        &self,
        operation: impl FnOnce(&mut WatchlistStore) -> Result<T, SdkError>,
    ) -> Result<T, SdkError> {
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.watchlist.is_none() {
            state.watchlist = Some(
                WatchlistStore::load_in_dir(self.config.storage_dir.clone())
                    .map_err(|error| SdkError::Storage(error.to_string()))?,
            );
        }
        let store = state
            .watchlist
            .as_mut()
            .expect("watchlist store was just initialized");
        operation(store)
    }

    /// Runs a committed account transaction on an SDK-owned thread.
    ///
    /// Once irreversible work starts (credential deletion, session teardown)
    /// the transaction and its account permit move to a dedicated thread so
    /// dropping the caller's future — or closing the SDK — cannot abandon
    /// cleanup halfway. The [`CommittedCleanup`] guard is claimed here,
    /// before the thread exists, so a spawn or runtime-build failure still
    /// runs the deferred teardown, and it is dropped before the outcome is
    /// sent so a settled result never precedes the cleanup it owns. The
    /// returned receiver reports the recorded outcome; a dropped receiver
    /// does not stop the work.
    fn spawn_committed<T, F, Fut>(
        self: &Arc<Self>,
        work: F,
    ) -> Result<oneshot::Receiver<Result<T, SdkError>>, SdkError>
    where
        T: Send + 'static,
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, SdkError>> + Send + 'static,
    {
        let committed = CommittedCleanup::begin(self)?;
        let (sender, receiver) = oneshot::channel();
        // If the thread cannot be spawned the sender drops with the closure
        // and the receiver resolves to an error the caller maps to a failure.
        let _ = std::thread::Builder::new()
            .name("jellypilot-sdk-account".to_owned())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let result = runtime.block_on(work());
                drop(committed);
                let _ = sender.send(result);
            });
        Ok(receiver)
    }

    /// Whether `scope` still identifies the live profile scope.
    pub(crate) fn scope_is_active(&self, scope: &ProfileScopeRef) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        if state.closed {
            return false;
        }
        state.active.as_ref().is_some_and(|active| {
            state.epoch == scope.generation && active.key.as_str() == scope.profile_key
        })
    }

    /// Runs the platform teardown hook when a profile is active.
    ///
    /// A declined hook aborts the transition with [`SdkError::HandoffAborted`]
    /// and leaves the previous profile active.
    async fn run_handoff_hook(&self) -> Result<(), SdkError> {
        let has_active = self
            .state
            .lock()
            .map_err(|_| SdkError::Closed)?
            .active
            .is_some();
        if !has_active {
            return Ok(());
        }
        let Some(hooks) = self.hooks.clone() else {
            return Ok(());
        };
        let allowed = AssertUnwindSafe(async move { hooks.before_profile_handoff().await })
            .catch_unwind()
            .await
            .unwrap_or(false);
        if allowed {
            Ok(())
        } else {
            Err(SdkError::HandoffAborted)
        }
    }

    /// Disconnects the active client, clears the session, advances the epoch,
    /// and cancels every token minted under the ended scope.
    fn end_active_session(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let old_epoch = state.epoch;
        state.epoch = state.epoch.saturating_add(1);
        if let Some(active) = state.active.take() {
            active.client.login().disconnect();
        }
        if let Some(tokens) = state.tokens.remove(&old_epoch) {
            for token in tokens {
                if let Some(token) = token.upgrade() {
                    token.cancel();
                }
            }
        }
    }

    /// Builds the public profile view of an installed session.
    fn active_profile_for(active: &ActiveSession) -> ActiveProfile {
        let connection = active.client.login().connection_state();
        ActiveProfile {
            key: active.key.as_str().to_owned(),
            provider: connection.provider,
            server_url: connection.server_url.unwrap_or_default(),
            server_name: connection.server_name,
            user_id: connection.user_id.unwrap_or_default(),
            user_name: connection.user_name.unwrap_or_default(),
            capabilities: connection.capabilities,
        }
    }

    /// The currently active profile, if any.
    fn active_profile(&self) -> Option<ActiveProfile> {
        let state = self.state.lock().ok()?;
        state.active.as_ref().map(Self::active_profile_for)
    }

    /// Runs `operation` under the token's cancellation and the scope's
    /// staleness rules on the SDK runtime.
    ///
    /// The work is spawned on the owned Tokio handle so it runs regardless of
    /// which thread or executor polls the returned future. Cancellation drops
    /// the in-flight wait; a result is only committed when the token is still
    /// live and the scope epoch is unchanged.
    pub(crate) async fn scoped<T, F, Fut>(
        &self,
        token: &OperationToken,
        operation: F,
    ) -> Result<T, SdkError>
    where
        T: Send + 'static,
        F: FnOnce(Arc<JellyfinClient>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, SdkError>> + Send + 'static,
    {
        self.check_open()?;
        let client = {
            let state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if state.epoch != token.epoch {
                return Err(SdkError::Stale);
            }
            if token.is_cancelled() {
                return Err(SdkError::Cancelled);
            }
            state
                .active
                .as_ref()
                .map(|active| Arc::clone(&active.client))
                .ok_or(SdkError::NoActiveProfile)?
        };

        let work = AbortOnDropHandle::new(self.handle.spawn(operation(client)));
        let result = tokio::select! {
            biased;
            () = token.cancel.cancelled() => return Err(SdkError::Cancelled),
            result = work => result.map_err(|join| {
                if join.is_cancelled() {
                    SdkError::Closed
                } else {
                    SdkError::Request("the operation task failed unexpectedly".to_owned())
                }
            })?,
        };
        if token.is_cancelled() {
            return Err(SdkError::Cancelled);
        }
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        if state.epoch != token.epoch {
            return Err(SdkError::Stale);
        }
        result
    }
}

/// Shared business-operation owner. One instance per application process.
pub struct Sdk {
    inner: Arc<SdkInner>,
    /// Present only when the SDK owns the runtime (created via [`Sdk::new`]
    /// family). Embedders supplying their own runtime leave this empty.
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
}

impl Sdk {
    /// Creates an SDK with its own Tokio runtime and an [`AuthStore`] backed
    /// by the injected protected-credential adapter.
    pub fn new(
        config: SdkConfig,
        credential: Arc<dyn SecureCredential>,
        hooks: Option<Arc<dyn SdkHooks>>,
    ) -> Result<Self, SdkError> {
        Self::with_auth_store(config, AuthStore::with_credential(credential), hooks)
    }

    /// [`Sdk::new`] with a pre-built [`AuthStore`].
    pub fn with_auth_store(
        config: SdkConfig,
        store: AuthStore,
        hooks: Option<Arc<dyn SdkHooks>>,
    ) -> Result<Self, SdkError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("jellypilot-sdk")
            .build()
            .map_err(|error| SdkError::Storage(error.to_string()))?;
        let inner = Self::inner(config, store, hooks, runtime.handle().clone());
        Ok(Self {
            inner,
            runtime: Mutex::new(Some(runtime)),
        })
    }

    /// Creates an SDK that runs its work on an existing Tokio runtime.
    ///
    /// Desktop embedders with an established runtime use this so SDK work
    /// shares their executor; `close` then only ends SDK state, not the
    /// runtime.
    pub fn with_handle(
        config: SdkConfig,
        store: AuthStore,
        hooks: Option<Arc<dyn SdkHooks>>,
        handle: Handle,
    ) -> Self {
        Self {
            inner: Self::inner(config, store, hooks, handle),
            runtime: Mutex::new(None),
        }
    }

    fn inner(
        config: SdkConfig,
        store: AuthStore,
        hooks: Option<Arc<dyn SdkHooks>>,
        handle: Handle,
    ) -> Arc<SdkInner> {
        Arc::new(SdkInner {
            handle,
            store,
            config,
            hooks,
            state: Mutex::new(SdkState {
                epoch: 0,
                active: None,
                tokens: HashMap::new(),
                watchlist: None,
                closed: false,
                committed_cleanup: 0,
            }),
            account_op: Arc::new(AsyncMutex::new(())),
            shutdown: CancellationToken::new(),
        })
    }

    /// Mints an operation token bound to the current profile scope.
    ///
    /// The token captures the active scope's identity and generation so
    /// follow-up calls can prove they still belong to the same account.
    pub fn new_operation_token(&self) -> Result<Arc<OperationToken>, SdkError> {
        self.inner.check_open()?;
        let mut state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        let cancel = Arc::new(CancellationToken::new());
        let epoch = state.epoch;
        let scope = ProfileScopeRef {
            profile_key: state
                .active
                .as_ref()
                .map(|active| active.key.as_str().to_owned())
                .unwrap_or_default(),
            generation: epoch,
        };
        state
            .tokens
            .entry(epoch)
            .or_default()
            .push(Arc::downgrade(&cancel));
        Ok(Arc::new(OperationToken {
            epoch,
            scope,
            cancel,
            inner: Arc::downgrade(&self.inner),
        }))
    }

    /// Cancels every live operation token without ending the profile scope.
    pub fn cancel_scope_operations(&self) {
        if let Ok(mut state) = self.inner.state.lock() {
            for (_, tokens) in state.tokens.drain() {
                for token in tokens {
                    if let Some(token) = token.upgrade() {
                        token.cancel();
                    }
                }
            }
        }
    }

    /// The currently active profile, if any.
    pub fn active_profile(&self) -> Option<ActiveProfile> {
        self.inner.active_profile()
    }

    /// Whether `scope` still identifies the live profile scope.
    ///
    /// Consumers holding a [`ProfileScopeRef`] from
    /// [`OperationToken::scope_ref`] use this to reject queued work whose
    /// issuing account is no longer active.
    pub fn is_scope_active(&self, scope: &ProfileScopeRef) -> bool {
        self.inner.scope_is_active(scope)
    }

    /// Runs a watchlist operation on the SDK runtime.
    ///
    /// The store performs synchronous filesystem work under the state lock;
    /// dispatching through `spawn_blocking` keeps that I/O off the consumer's
    /// thread while the lock still serializes token validation, scope
    /// capture, and the mutation itself.
    pub(crate) async fn run_watchlist<T>(
        &self,
        token: Arc<OperationToken>,
        operation: impl FnOnce(&mut WatchlistStore, &ProfileScope) -> Result<T, SdkError>
            + Send
            + 'static,
    ) -> Result<T, SdkError>
    where
        T: Send + 'static,
    {
        self.inner.check_open()?;
        let inner = Arc::clone(&self.inner);
        self.inner
            .handle
            .spawn_blocking(move || inner.with_scoped_watchlist(&token, operation))
            .await
            .map_err(|join| {
                if join.is_cancelled() {
                    SdkError::Closed
                } else {
                    SdkError::Request("the watchlist task failed unexpectedly".to_owned())
                }
            })?
    }

    /// Ends the SDK: rejects new work, cancels live operations and flows,
    /// disconnects the active client, and shuts down the owned runtime when
    /// present. Idempotent.
    ///
    /// A committed account transaction in flight (credential deletion,
    /// session teardown) keeps the active session and its cleanup
    /// obligation until it settles; its [`CommittedCleanup`] guard performs
    /// the disconnect afterward, so closing never abandons teardown halfway
    /// and never blocks waiting on a hook that may itself call `close`.
    pub fn close(&self) {
        let tokens = {
            let Ok(mut state) = self.inner.state.lock() else {
                return;
            };
            if state.closed {
                return;
            }
            state.closed = true;
            if state.committed_cleanup == 0 {
                if let Some(active) = state.active.take() {
                    active.client.login().disconnect();
                }
            }
            std::mem::take(&mut state.tokens)
        };
        for (_, epoch_tokens) in tokens {
            for token in epoch_tokens {
                if let Some(token) = token.upgrade() {
                    token.cancel();
                }
            }
        }
        self.inner.shutdown.cancel();
        if let Ok(mut runtime) = self.runtime.lock() {
            if let Some(runtime) = runtime.take() {
                runtime.shutdown_background();
            }
        }
    }

    /// Test-only: installs an already-authenticated client as the active
    /// session without network validation.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn adopt_test_session(&self, session: jellypilot_media_server::SavedSession) {
        let key = jellypilot_auth::SavedProfileKey::for_session(&session);
        let scope = ProfileScope::new(
            session.provider,
            session.server_url.clone(),
            session.user_id.clone(),
        )
        .expect("test session must form a valid scope");
        let client = Arc::new(JellyfinClient::with_storage_dir(
            self.inner.config.storage_dir.clone(),
        ));
        client.login().adopt_validated_session(&session);
        let old_epoch = {
            let mut state = self.inner.state.lock().expect("state lock");
            if state.closed {
                return;
            }
            let old_epoch = state.epoch;
            state.epoch = state.epoch.saturating_add(1);
            if let Some(previous) = state.active.take() {
                previous.client.login().disconnect();
            }
            state.active = Some(ActiveSession {
                key,
                scope,
                _session: jellypilot_auth::SensitiveSavedSession::from_client(&client)
                    .expect("adopted session must be capturable"),
                client,
            });
            old_epoch
        };
        self.inner.cancel_epoch(old_epoch);
    }
}

impl Drop for Sdk {
    fn drop(&mut self) {
        self.close();
    }
}
