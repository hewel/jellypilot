//! Local History hiding with a scope-bound undo and server-offset pagination.

use std::sync::Arc;

use jellypilot_core::history::HistoryVisibilityStore;
use jellypilot_core::watchlist::ProfileScope;
use jellypilot_media_server::{VideoLibraryItem, WatchHistoryPageRequest};
use tokio::sync::Mutex;

use crate::{OperationToken, Sdk, SdkError, SdkInner};

/// A visible History page. `next_start_index` addresses the merged server stream;
/// deriving it from the visible item count would repeat/skip locally hidden rows.
#[derive(Debug)]
pub struct HistoryPage {
    pub start_index: i32,
    pub limit: i32,
    /// Server count before local visibility filtering; not a visible-list count.
    pub total_record_count: i32,
    pub next_start_index: i32,
    pub has_more: bool,
    pub items: Vec<VideoLibraryItem>,
}

/// Single-use undo for a local History hide. It carries no authenticated client
/// operation, and therefore cannot erase or reset server playback progress.
pub struct HistoryRemoval {
    inner: Arc<SdkInner>,
    token: Arc<OperationToken>,
    item_id: String,
    changed: bool,
    undone: Arc<Mutex<bool>>,
}

impl HistoryRemoval {
    pub fn item_id(&self) -> String {
        self.item_id.clone()
    }

    pub async fn undo(&self) -> Result<bool, SdkError> {
        let mut undone = Arc::clone(&self.undone).lock_owned().await;
        if *undone || !self.changed {
            return Ok(false);
        }
        let inner = Arc::clone(&self.inner);
        let token = Arc::clone(&self.token);
        let item_id = self.item_id.clone();
        self.inner
            .handle
            .spawn_blocking(move || {
                let (_, changed) = inner.with_history(&token, true, |store, scope| {
                    let changed = store
                        .set_hidden(scope, &item_id, false)
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                    Ok((changed, changed))
                })?;
                *undone = true;
                Ok(changed)
            })
            .await
            .map_err(|_| SdkError::Storage("history undo worker failed".to_owned()))?
    }
}

impl SdkInner {
    fn with_history<T>(
        &self,
        token: &OperationToken,
        write: bool,
        operation: impl FnOnce(
            &mut HistoryVisibilityStore,
            &ProfileScope,
        ) -> Result<(bool, T), SdkError>,
    ) -> Result<(u64, T), SdkError> {
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        if state.epoch != token.epoch {
            return Err(SdkError::Stale);
        }
        if token.is_cancelled() {
            return Err(SdkError::Cancelled);
        }
        if write && (state.handoff_in_progress || state.sign_out_cleanup_pending) {
            return Err(SdkError::OperationInProgress);
        }
        let scope = &state
            .active
            .as_ref()
            .ok_or(SdkError::NoActiveProfile)?
            .scope;
        let mut revision = self.history_revision.lock().map_err(|_| SdkError::Closed)?;
        let mut store = HistoryVisibilityStore::load_in_dir(self.config.storage_dir.clone())
            .map_err(|error| SdkError::Storage(error.to_string()))?;
        let (changed, result) = operation(&mut store, scope)?;
        if changed {
            *revision = revision.wrapping_add(1);
        }
        Ok((*revision, result))
    }
}

impl Sdk {
    /// Hides one server History item on this device without any server mutation.
    pub async fn hide_history_item(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<Arc<HistoryRemoval>, SdkError> {
        let item_id = item_id.trim().to_owned();
        if item_id.is_empty() {
            return Err(SdkError::InvalidInput(
                "a history item identity is required".to_owned(),
            ));
        }
        let undo_token = self.new_operation_token()?;
        if undo_token.scope_ref()? != token.scope_ref()? {
            return Err(SdkError::Stale);
        }
        let inner = Arc::clone(&self.inner);
        let hidden_id = item_id.clone();
        let (_, changed) = self
            .inner
            .handle
            .spawn_blocking(move || {
                inner.with_history(&token, true, |store, scope| {
                    let changed = store
                        .set_hidden(scope, &hidden_id, true)
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                    Ok((changed, changed))
                })
            })
            .await
            .map_err(|_| SdkError::Storage("history hide worker failed".to_owned()))??;
        Ok(Arc::new(HistoryRemoval {
            inner: Arc::clone(&self.inner),
            token: undo_token,
            item_id,
            changed,
            undone: Arc::new(Mutex::new(false)),
        }))
    }

    /// Reads server History while skipping local hidden rows. A fully hidden
    /// source page is advanced internally until visible data or the true end.
    pub async fn watch_history(
        &self,
        token: Arc<OperationToken>,
        start_index: i32,
        limit: i32,
    ) -> Result<HistoryPage, SdkError> {
        let inner = Arc::clone(&self.inner);
        let snapshot_inner = Arc::clone(&inner);
        let snapshot_token = Arc::clone(&token);
        let (revision, hidden) = self
            .inner
            .handle
            .spawn_blocking(move || {
                snapshot_inner.with_history(&snapshot_token, false, |store, scope| {
                    Ok((false, store.hidden_item_ids(scope)))
                })
            })
            .await
            .map_err(|_| SdkError::Storage("history visibility worker failed".to_owned()))??;
        self.inner
            .scoped(&token, move |client| async move {
                let start_index = start_index.max(0);
                let limit = limit.clamp(1, 100);
                let mut cursor = start_index;
                let mut items = Vec::new();
                let mut total = None;
                loop {
                    let remaining = limit
                        - i32::try_from(items.len()).map_err(|_| {
                            SdkError::Request("history page size overflow".to_owned())
                        })?;
                    let page = client
                        .library()
                        .history(WatchHistoryPageRequest {
                            start_index: cursor,
                            limit: remaining,
                        })
                        .await
                        .map_err(SdkError::from)?;
                    if *inner
                        .history_revision
                        .lock()
                        .map_err(|_| SdkError::Closed)?
                        != revision
                    {
                        return Err(SdkError::Stale);
                    }
                    if total.is_some_and(|total| total != page.total_record_count) {
                        return Err(SdkError::Request(
                            "history changed while paging; refresh the list".to_owned(),
                        ));
                    }
                    total = Some(page.total_record_count);
                    let returned = i32::try_from(page.items.len())
                        .map_err(|_| SdkError::Request("history page size overflow".to_owned()))?;
                    if page.start_index != cursor
                        || returned > remaining
                        || (returned == 0 && page.has_more)
                    {
                        return Err(SdkError::Request(
                            "history returned an incomplete page".to_owned(),
                        ));
                    }
                    cursor = cursor.checked_add(returned).ok_or_else(|| {
                        SdkError::Request("history page offset overflow".to_owned())
                    })?;
                    items.extend(
                        page.items
                            .into_iter()
                            .filter(|item| !hidden.contains(&item.id)),
                    );
                    if !page.has_more || items.len() >= limit as usize {
                        return Ok(HistoryPage {
                            start_index,
                            limit,
                            total_record_count: page.total_record_count,
                            next_start_index: cursor,
                            has_more: page.has_more,
                            items,
                        });
                    }
                }
            })
            .await
    }
}
