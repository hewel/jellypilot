//! Device-local named browse queries, bound to the operation's actual profile.

use std::fmt;
use std::sync::Arc;

use jellypilot_core::browse_model::{BrowseQueryPolicy, BrowseSource};
use jellypilot_core::request_gate::RequestGate;
pub use jellypilot_core::saved_browse::{SavedBrowseDraft, SavedBrowseFilter, SavedBrowseId};
use jellypilot_core::saved_browse::{SavedBrowseStore, SavedBrowseStoreError};
use jellypilot_core::watchlist::ProfileScope;
use jellypilot_media_server::VideoLibraryKind;

use crate::browse::{BrowsePresentation, BrowseSession};

use crate::{OperationToken, Sdk, SdkError, SdkInner, SdkState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SavedBrowseError {
    Sdk(SdkError),
    InvalidName,
    DuplicateName,
    LimitReached,
    Missing,
    LibraryUnavailable,
}

impl fmt::Display for SavedBrowseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sdk(error) => error.fmt(formatter),
            Self::InvalidName => formatter.write_str("saved filter name is invalid"),
            Self::DuplicateName => formatter.write_str("a saved filter already uses this name"),
            Self::LimitReached => formatter.write_str("the saved filter limit has been reached"),
            Self::Missing => formatter.write_str("the saved filter no longer exists"),
            Self::LibraryUnavailable => formatter.write_str("the saved library is unavailable"),
        }
    }
}

impl std::error::Error for SavedBrowseError {}

impl From<SdkError> for SavedBrowseError {
    fn from(error: SdkError) -> Self {
        Self::Sdk(error)
    }
}

impl From<SavedBrowseStoreError> for SavedBrowseError {
    fn from(error: SavedBrowseStoreError) -> Self {
        match error {
            SavedBrowseStoreError::InvalidName => Self::InvalidName,
            SavedBrowseStoreError::DuplicateName => Self::DuplicateName,
            SavedBrowseStoreError::LimitReached => Self::LimitReached,
            SavedBrowseStoreError::Missing => Self::Missing,
            SavedBrowseStoreError::InvalidLibrary => Self::Sdk(SdkError::InvalidInput(
                "a library identity is required".to_owned(),
            )),
            other => Self::Sdk(SdkError::Storage(other.to_string())),
        }
    }
}

impl SdkInner {
    fn saved_browse_scope<'a>(
        &self,
        state: &'a SdkState,
        token: &OperationToken,
        require_admission: bool,
    ) -> Result<&'a ProfileScope, SavedBrowseError> {
        if state.closed {
            return Err(SdkError::Closed.into());
        }
        if state.epoch != token.epoch || !std::ptr::eq(self, token.inner.as_ptr()) {
            return Err(SdkError::Stale.into());
        }
        if token.is_cancelled() {
            return Err(SdkError::Cancelled.into());
        }
        if require_admission && (state.handoff_in_progress || state.sign_out_cleanup_pending) {
            return Err(SdkError::OperationInProgress.into());
        }
        let active = state.active.as_ref().ok_or(SdkError::NoActiveProfile)?;
        if active.key.as_str() != token.scope.profile_key || token.scope.generation != state.epoch {
            return Err(SdkError::Stale.into());
        }
        Ok(&active.scope)
    }

    fn with_saved_browse<T>(
        &self,
        token: &OperationToken,
        require_admission: bool,
        operation: impl FnOnce(&mut SavedBrowseStore, &ProfileScope) -> Result<T, SavedBrowseError>,
    ) -> Result<T, SavedBrowseError> {
        // The same lock guards account activation and remains held through the
        // physical rename. A queued worker cannot write into a later account.
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        let scope = self.saved_browse_scope(&state, token, require_admission)?;
        let mut store = SavedBrowseStore::load_in_dir(self.config.storage_dir.clone())?;
        self.saved_browse_scope(&state, token, require_admission)?;
        let result = operation(&mut store, scope);
        // Committed store truth survives cancellation of result delivery.
        let notifications = if result.is_ok() {
            let records = store.records_for(scope);
            state
                .browsers
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .map(|browser| {
                    let changed =
                        browser.token_scope() == &token.scope && browser.reconcile_saved(&records);
                    (browser, changed)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        drop(state);
        for (browser, changed) in notifications {
            if changed {
                browser.notify_saved_changed();
            }
        }
        result
    }

    async fn run_saved_browse<T: Send + 'static>(
        self: &Arc<Self>,
        token: Arc<OperationToken>,
        require_admission: bool,
        operation: impl FnOnce(&mut SavedBrowseStore, &ProfileScope) -> Result<T, SavedBrowseError>
            + Send
            + 'static,
    ) -> Result<T, SavedBrowseError> {
        let inner = Arc::clone(self);
        let work_token = Arc::clone(&token);
        let result = self
            .handle
            .spawn_blocking(move || {
                inner.with_saved_browse(&work_token, require_admission, operation)
            })
            .await
            .map_err(|_| SdkError::Storage("saved filter worker failed".to_owned()))?;
        // A committed write remains in its original scope, but delivery also
        // retires when that scope or the caller's operation ended meanwhile.
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        self.saved_browse_scope(&state, &token, require_admission)?;
        result
    }
}

impl Sdk {
    pub async fn saved_browse_list(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Vec<SavedBrowseFilter>, SavedBrowseError> {
        self.inner
            .run_saved_browse(token, false, |store, scope| Ok(store.records_for(scope)))
            .await
    }

    pub async fn saved_browse_save(
        &self,
        token: Arc<OperationToken>,
        draft: SavedBrowseDraft,
    ) -> Result<SavedBrowseFilter, SavedBrowseError> {
        self.inner
            .run_saved_browse(token, true, move |store, scope| {
                store.save(scope, draft).map_err(Into::into)
            })
            .await
    }

    pub async fn saved_browse_rename(
        &self,
        token: Arc<OperationToken>,
        id: SavedBrowseId,
        name: String,
    ) -> Result<SavedBrowseFilter, SavedBrowseError> {
        self.inner
            .run_saved_browse(token, true, move |store, scope| {
                store.rename(scope, id, &name).map_err(Into::into)
            })
            .await
    }

    pub async fn saved_browse_delete(
        &self,
        token: Arc<OperationToken>,
        id: SavedBrowseId,
    ) -> Result<bool, SavedBrowseError> {
        self.inner
            .run_saved_browse(token, true, move |store, scope| {
                store.remove(scope, id).map_err(Into::into)
            })
            .await
    }

    /// Resolves a fresh directory and adopts the complete saved query under
    /// an independent lifetime. Cancelling the manager afterwards cannot close it.
    pub async fn open_saved_browser(
        &self,
        token: Arc<OperationToken>,
        id: SavedBrowseId,
        presentation: BrowsePresentation,
    ) -> Result<Arc<BrowseSession>, SavedBrowseError> {
        let filter = self
            .inner
            .run_saved_browse(Arc::clone(&token), true, move |store, scope| {
                store
                    .get(scope, id)
                    .cloned()
                    .ok_or(SavedBrowseError::Missing)
            })
            .await?;
        let libraries = self.library_shortcuts(Arc::clone(&token)).await?;
        let adopted_token = self.new_operation_token()?;
        let collection_type = match filter.collection_type {
            VideoLibraryKind::Movies => "movies",
            VideoLibraryKind::TvShows => "tvshows",
        };
        let library = libraries
            .into_iter()
            .find(|library| {
                library.id == filter.library_id && library.collection_type == collection_type
            })
            .ok_or(SavedBrowseError::LibraryUnavailable)?;
        let inner = Arc::clone(&self.inner);
        let worker_token = Arc::clone(&token);
        let (session, work) = self
            .inner
            .handle
            .spawn_blocking(move || {
                let mut state = inner.state.lock().map_err(|_| SdkError::Closed)?;
                let scope = inner.saved_browse_scope(&state, &worker_token, true)?;
                inner.check_browse_scope(&state, &adopted_token)?;
                let store = SavedBrowseStore::load_in_dir(inner.config.storage_dir.clone())?;
                let current = store.get(scope, id).ok_or(SavedBrowseError::Missing)?;
                if current != &filter {
                    return Err(SdkError::Stale.into());
                }
                inner.saved_browse_scope(&state, &worker_token, true)?;
                let source = BrowseSource::Library {
                    session: RequestGate::default().begin_login(),
                    shortcut: library,
                };
                let policy = BrowseQueryPolicy::saved(filter, presentation);
                inner
                    .create_browser(&mut state, &adopted_token, source, policy)
                    .map_err(SavedBrowseError::from)
            })
            .await
            .map_err(|_| SdkError::Storage("saved filter worker failed".to_owned()))??;
        let admission = {
            let state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
            self.inner
                .saved_browse_scope(&state, &token, true)
                .map(|_| ())
        };
        // A completed worker is not delivery: its caller may have retired
        // while this future was unpolled. Drop rejected owners only unlocked.
        admission?;
        session.dispatch(work);
        Ok(session)
    }
}
