//! UniFFI bindings for the JellyPilot shared SDK.
//!
//! This crate is a thin conversion layer: typed DTOs, typed errors, and
//! object/operation lifetime. All business behavior lives in
//! `jellypilot-sdk`; cancelling a consumer-side coroutine is not business
//! cancellation — use [`OperationToken`].

mod callbacks;
mod dto;
mod error;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use jellypilot_media_server as ms;

pub use callbacks::{QuickConnectListener, QuickConnectOutcome, SdkHooks, SecureCredentialStore};
pub use dto::*;
pub use error::{CredentialStoreError, SdkError};

uniffi::setup_scaffolding!();

/// Cancellation and staleness handle for one scoped operation.
///
/// Mint one per screen/operation via [`JellypilotSdk::new_operation_token`]
/// and cancel it on dispose. Tokens are bound to the profile scope that
/// created them; a scope change makes their results `SdkError.Stale`.
#[derive(uniffi::Object)]
pub struct OperationToken {
    inner: Arc<jellypilot_sdk::OperationToken>,
}

#[uniffi::export]
impl OperationToken {
    /// Cancels this operation. Idempotent.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Whether the token was cancelled or its scope ended.
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }

    /// The profile scope this token was minted under.
    ///
    /// Fails with `SdkError.NoActiveProfile` when the token was minted while
    /// no profile was active. Pass the reference to `image_target` or check
    /// it with `is_scope_active` before publishing scope-bound work.
    pub fn scope_ref(&self) -> Result<ProfileScopeRef, SdkError> {
        self.inner
            .scope_ref()
            .map(ProfileScopeRef::from)
            .map_err(SdkError::from)
    }
}

/// A validated authentication that is not yet the active profile.
///
/// Single-use: `activate_candidate` consumes it; `discard` releases it
/// without side effects.
#[derive(uniffi::Object)]
pub struct ProfileCandidate {
    inner: Mutex<Option<jellypilot_sdk::ProfileCandidate>>,
    key: String,
    provider: Provider,
    account_title: String,
}

impl ProfileCandidate {
    pub(crate) fn from_sdk(candidate: jellypilot_sdk::ProfileCandidate) -> Self {
        Self {
            key: candidate.key().to_owned(),
            provider: candidate.provider().into(),
            account_title: candidate.account_title().to_owned(),
            inner: Mutex::new(Some(candidate)),
        }
    }

    fn take(&self) -> Result<jellypilot_sdk::ProfileCandidate, SdkError> {
        self.inner
            .lock()
            .map_err(|_| SdkError::Closed)?
            .take()
            .ok_or(SdkError::InvalidInput {
                reason: "this profile candidate was already consumed".to_owned(),
            })
    }
}

#[uniffi::export]
impl ProfileCandidate {
    /// Stable saved-profile key for this candidate.
    pub fn profile_key(&self) -> String {
        self.key.clone()
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// Redacted `user@server` label suitable for confirmation UI.
    pub fn account_title(&self) -> String {
        self.account_title.clone()
    }

    /// Discards the candidate without activating it.
    pub fn discard(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(mut candidate) = inner.take() {
                candidate.discard();
            }
        }
    }
}

/// A running Quick Connect flow. Dropping it cancels the flow.
#[derive(uniffi::Object)]
pub struct QuickConnectSession {
    inner: Arc<jellypilot_sdk::QuickConnectSession>,
}

#[uniffi::export]
impl QuickConnectSession {
    /// Cancels the flow; the listener still receives `Cancelled` unless a
    /// terminal outcome already fired.
    pub fn cancel(&self) {
        self.inner.cancel();
    }
}

/// Shared business-operation owner for the Android frontend.
///
/// Owns the Tokio runtime and the single active profile scope. Create once
/// per process (application scope, not per composition) and call
/// [`JellypilotSdk::shutdown`] before disposing the generated object handle.
#[derive(uniffi::Object)]
pub struct JellypilotSdk {
    sdk: jellypilot_sdk::Sdk,
}

#[uniffi::export(async_runtime = "tokio")]
impl JellypilotSdk {
    /// Creates the SDK with its own runtime.
    ///
    /// `credential_store` is the platform protected-credential adapter
    /// (Keystore-backed on Android). `hooks` may be `null`; when present its
    /// `before_profile_handoff` runs while the previous profile is still
    /// active during activation, disconnect, and sign-out.
    #[uniffi::constructor]
    pub fn new(
        config: SdkConfig,
        credential_store: Arc<dyn SecureCredentialStore>,
        hooks: Option<Arc<dyn SdkHooks>>,
    ) -> Result<Arc<Self>, SdkError> {
        let sdk_config = jellypilot_sdk::SdkConfig {
            storage_dir: PathBuf::from(config.storage_dir),
            device_name: config.device_name,
        };
        let sdk = jellypilot_sdk::Sdk::new(
            sdk_config,
            Arc::new(callbacks::CredentialStoreAdapter::new(credential_store)),
            hooks.map(|hooks| {
                Arc::new(callbacks::HooksAdapter::new(hooks)) as Arc<dyn jellypilot_sdk::SdkHooks>
            }),
        )
        .map_err(SdkError::from)?;
        Ok(Arc::new(Self { sdk }))
    }

    /// Saved profiles plus the only profile eligible for startup restore.
    pub async fn saved_profiles(&self) -> Result<SavedProfilesSnapshot, SdkError> {
        self.sdk
            .saved_profiles()
            .await
            .map(SavedProfilesSnapshot::from)
            .map_err(SdkError::from)
    }

    /// Authenticates with a password and returns a validated candidate.
    /// The active profile is untouched until `activate_candidate`.
    pub async fn password_login(
        &self,
        provider: Provider,
        server_url: String,
        username: String,
        password: String,
    ) -> Result<Arc<ProfileCandidate>, SdkError> {
        self.sdk
            .password_login(provider.into(), server_url, username, password)
            .await
            .map(|candidate| Arc::new(ProfileCandidate::from_sdk(candidate)))
            .map_err(SdkError::from)
    }

    /// Starts a Jellyfin Quick Connect flow. Progress arrives on `listener`;
    /// the returned session cancels the flow when dropped or cancelled.
    pub fn start_quick_connect(
        &self,
        server_url: String,
        listener: Arc<dyn QuickConnectListener>,
    ) -> Result<Arc<QuickConnectSession>, SdkError> {
        self.sdk
            .start_quick_connect(
                ms::MediaServerProvider::Jellyfin,
                server_url,
                Arc::new(callbacks::QuickConnectListenerAdapter::new(listener)),
            )
            .map(|inner| Arc::new(QuickConnectSession { inner }))
            .map_err(SdkError::from)
    }

    /// Validates a saved profile's stored session and returns a candidate.
    pub async fn restore_saved_profile(
        &self,
        key: String,
    ) -> Result<Arc<ProfileCandidate>, SdkError> {
        self.sdk
            .restore_saved_profile(key)
            .await
            .map(|candidate| Arc::new(ProfileCandidate::from_sdk(candidate)))
            .map_err(SdkError::from)
    }

    /// Activates a validated candidate as the single active profile.
    ///
    /// Platform teardown (`SdkHooks.before_profile_handoff`) settles first;
    /// a declined teardown keeps the previous profile. The returned outcome
    /// is committed: `persistence_warning` reports a session or
    /// startup-restore persistence failure after the swap, not a rollback.
    pub async fn activate_candidate(
        &self,
        candidate: Arc<ProfileCandidate>,
        save_profile: bool,
    ) -> Result<ActivationOutcome, SdkError> {
        let candidate = candidate.take()?;
        self.sdk
            .activate_candidate(candidate, save_profile)
            .await
            .map(ActivationOutcome::from)
            .map_err(SdkError::from)
    }

    /// Ends the active session without removing saved credentials.
    /// Distinct from `sign_out`; idempotent when signed out.
    pub async fn disconnect(&self) -> Result<(), SdkError> {
        self.sdk.disconnect().await.map_err(SdkError::from)
    }

    /// Signs out: removes the saved credentials first, then ends the session
    /// when it matches `key` and optionally deletes that profile's
    /// device-local watchlist. The deletion is committed before the outcome
    /// is produced; `teardown_error`/`watchlist_error` report post-commit
    /// cleanup failures, not a rollback.
    pub async fn sign_out(
        &self,
        key: String,
        delete_watchlist: bool,
    ) -> Result<SignOutOutcome, SdkError> {
        self.sdk
            .sign_out(key, delete_watchlist)
            .await
            .map(SignOutOutcome::from)
            .map_err(SdkError::from)
    }

    /// Removes a saved profile that is not currently active.
    pub async fn remove_saved_profile(
        &self,
        key: String,
    ) -> Result<ProfileRemovalOutcome, SdkError> {
        self.sdk
            .remove_saved_profile(key)
            .await
            .map(ProfileRemovalOutcome::from)
            .map_err(SdkError::from)
    }

    /// The currently active profile, if any.
    pub fn active_profile(&self) -> Option<ActiveProfile> {
        self.sdk.active_profile().map(ActiveProfile::from)
    }

    /// Mints an operation token bound to the current profile scope.
    pub fn new_operation_token(&self) -> Result<Arc<OperationToken>, SdkError> {
        self.sdk
            .new_operation_token()
            .map(|inner| Arc::new(OperationToken { inner }))
            .map_err(SdkError::from)
    }

    /// Cancels every live operation token without ending the profile scope.
    pub fn cancel_scope_operations(&self) {
        self.sdk.cancel_scope_operations();
    }

    /// Whether `scope` still identifies the live profile scope. Use it to
    /// reject queued work whose issuing account is no longer active.
    pub fn is_scope_active(&self, scope: ProfileScopeRef) -> bool {
        self.sdk.is_scope_active(&scope.into())
    }

    /// Video Home landing rows (Continue Watching, Next Up).
    pub async fn video_home(&self, token: Arc<OperationToken>) -> Result<VideoHome, SdkError> {
        self.sdk
            .video_home(token.inner.clone())
            .await
            .map(VideoHome::from)
            .map_err(SdkError::from)
    }

    /// Movies/Shows library shortcuts for browse navigation.
    pub async fn library_shortcuts(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Vec<VideoLibraryShortcut>, SdkError> {
        self.sdk
            .library_shortcuts(token.inner.clone())
            .await
            .map(|shortcuts| {
                shortcuts
                    .into_iter()
                    .map(VideoLibraryShortcut::from)
                    .collect()
            })
            .map_err(SdkError::from)
    }

    /// Latest items for one library row.
    pub async fn library_latest(
        &self,
        token: Arc<OperationToken>,
        library_id: String,
    ) -> Result<Vec<VideoLibraryItem>, SdkError> {
        self.sdk
            .library_latest(token.inner.clone(), library_id)
            .await
            .map(|items| items.into_iter().map(VideoLibraryItem::from).collect())
            .map_err(SdkError::from)
    }

    /// Paged Library Browser listing.
    pub async fn browse_video(
        &self,
        token: Arc<OperationToken>,
        request: VideoLibraryPageRequest,
    ) -> Result<VideoLibraryPage, SdkError> {
        self.sdk
            .browse_video(token.inner.clone(), request.into())
            .await
            .map(VideoLibraryPage::from)
            .map_err(SdkError::from)
    }

    /// Paged video-only library search.
    pub async fn search_video(
        &self,
        token: Arc<OperationToken>,
        request: VideoSearchRequest,
    ) -> Result<VideoSearchPage, SdkError> {
        self.sdk
            .search_video(token.inner.clone(), request.into())
            .await
            .map(VideoSearchPage::from)
            .map_err(SdkError::from)
    }

    /// Root-level page of the user's video Favorites.
    pub async fn favorites(
        &self,
        token: Arc<OperationToken>,
        start_index: i32,
        limit: i32,
    ) -> Result<FavoritesPage, SdkError> {
        self.sdk
            .favorites(token.inner.clone(), start_index, limit)
            .await
            .map(FavoritesPage::from)
            .map_err(SdkError::from)
    }

    /// Server Watch History page (played and resumable items).
    pub async fn watch_history(
        &self,
        token: Arc<OperationToken>,
        start_index: i32,
        limit: i32,
    ) -> Result<WatchHistoryPage, SdkError> {
        self.sdk
            .watch_history(token.inner.clone(), start_index, limit)
            .await
            .map(WatchHistoryPage::from)
            .map_err(SdkError::from)
    }

    /// Playable Movie or Episode detail.
    pub async fn item_detail(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<VideoItemDetail, SdkError> {
        self.sdk
            .item_detail(token.inner.clone(), item_id)
            .await
            .map(VideoItemDetail::from)
            .map_err(SdkError::from)
    }

    /// Audio/subtitle stream metadata for a detail item.
    pub async fn item_streams(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<VideoItemStreams, SdkError> {
        self.sdk
            .item_streams(token.inner.clone(), item_id)
            .await
            .map(VideoItemStreams::from)
            .map_err(SdkError::from)
    }

    /// Show detail with seasons and next playable episode.
    pub async fn show_detail(
        &self,
        token: Arc<OperationToken>,
        series_id: String,
    ) -> Result<VideoShowDetail, SdkError> {
        self.sdk
            .show_detail(token.inner.clone(), series_id)
            .await
            .map(VideoShowDetail::from)
            .map_err(SdkError::from)
    }

    /// Bounded page of episodes inside a show season.
    pub async fn season_episodes_page(
        &self,
        token: Arc<OperationToken>,
        request: VideoSeasonEpisodesPageRequest,
    ) -> Result<VideoSeasonEpisodesPage, SdkError> {
        self.sdk
            .season_episodes_page(token.inner.clone(), request.into())
            .await
            .map(VideoSeasonEpisodesPage::from)
            .map_err(SdkError::from)
    }

    /// Related items for a detail view.
    pub async fn similar_video(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<Vec<VideoLibraryItem>, SdkError> {
        self.sdk
            .similar_video(token.inner.clone(), item_id)
            .await
            .map(|items| items.into_iter().map(VideoLibraryItem::from).collect())
            .map_err(SdkError::from)
    }

    /// Next playable episode for a series, when the provider exposes one.
    pub async fn next_playable_episode(
        &self,
        token: Arc<OperationToken>,
        series_id: String,
    ) -> Result<Option<VideoPlaybackTarget>, SdkError> {
        self.sdk
            .next_playable_episode(token.inner.clone(), series_id)
            .await
            .map(|target| target.map(VideoPlaybackTarget::from))
            .map_err(SdkError::from)
    }

    /// Batch item lookup used by watchlist and retained-content enrichment.
    pub async fn video_items_by_ids(
        &self,
        token: Arc<OperationToken>,
        item_ids: Vec<String>,
    ) -> Result<Vec<VideoLibraryItem>, SdkError> {
        self.sdk
            .video_items_by_ids(token.inner.clone(), item_ids)
            .await
            .map(|items| items.into_iter().map(VideoLibraryItem::from).collect())
            .map_err(SdkError::from)
    }

    /// Favorite/played mutation. Authoritative only after server acceptance;
    /// a cancelled or stale result must not be applied to visible content.
    pub async fn update_user_data(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
        action: VideoUserDataAction,
    ) -> Result<VideoUserDataUpdate, SdkError> {
        self.sdk
            .update_user_data(token.inner.clone(), item_id, action.into())
            .await
            .map(VideoUserDataUpdate::from)
            .map_err(SdkError::from)
    }

    /// Watchlist records for the active profile, most recently added first.
    pub async fn watchlist_items(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Vec<WatchlistEntry>, SdkError> {
        self.sdk
            .watchlist_items(token.inner.clone())
            .await
            .map(|records| records.into_iter().map(WatchlistEntry::from).collect())
            .map_err(SdkError::from)
    }

    /// Adds the item to the active profile's device watchlist.
    pub async fn watchlist_add(
        &self,
        token: Arc<OperationToken>,
        item: VideoLibraryItem,
    ) -> Result<bool, SdkError> {
        self.sdk
            .watchlist_add(token.inner.clone(), item.into())
            .await
            .map_err(SdkError::from)
    }

    /// Removes the item from the active profile's device watchlist.
    pub async fn watchlist_remove(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<bool, SdkError> {
        self.sdk
            .watchlist_remove(token.inner.clone(), item_id)
            .await
            .map_err(SdkError::from)
    }

    /// Whether the item is in the active profile's device watchlist.
    pub async fn watchlist_contains(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<bool, SdkError> {
        self.sdk
            .watchlist_contains(token.inner.clone(), item_id)
            .await
            .map_err(SdkError::from)
    }

    /// Resolves a signed image id into a fetch target for `scope`'s session.
    ///
    /// `scope` must come from `OperationToken.scope_ref()` captured when the
    /// image work was queued; the SDK validates it against the live scope
    /// and clones the matching client atomically, so artwork queued under
    /// one account cannot resolve against a later account's credentials.
    /// The authorization value is secret-bearing.
    pub fn image_target(
        &self,
        scope: ProfileScopeRef,
        image_id: String,
        max_width: Option<u32>,
    ) -> Result<LibraryImageTarget, SdkError> {
        self.sdk
            .image_target(&scope.into(), image_id, max_width)
            .map(LibraryImageTarget::from)
            .map_err(SdkError::from)
    }

    /// Ends the SDK: cancels live operations, disconnects the active client,
    /// and shuts down the runtime. Idempotent. Call this before the generated
    /// Kotlin `close()`, which only disposes its UniFFI object handle.
    pub fn shutdown(&self) {
        self.sdk.close();
    }
}
