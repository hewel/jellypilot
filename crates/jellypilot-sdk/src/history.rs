//! Local History hiding with a scope-bound undo and server-offset pagination.

use std::fmt;
use std::sync::Arc;

use jellypilot_core::history::HistoryVisibilityStore;
use jellypilot_core::watchlist::ProfileScope;
use jellypilot_media_server::{VideoLibraryItem, WatchHistoryPageRequest};
use tokio::sync::Mutex;

use crate::{OperationToken, Sdk, SdkError, SdkInner};

/// A visible History page. `next_start_index` addresses the merged server stream;
/// deriving it from the visible item count would repeat/skip locally hidden rows.
#[derive(Clone, Debug)]
pub struct HistoryPage {
    pub start_index: i32,
    pub limit: i32,
    /// Server count before local visibility filtering; not a visible-list count.
    pub total_record_count: i32,
    pub next_start_index: i32,
    pub has_more: bool,
    pub items: Vec<VideoLibraryItem>,
}

/// A desktop History page with an exact visible count only when established by
/// the requested data. Unknown counts do not prevent server-offset pagination.
#[derive(Clone, Debug)]
pub struct DesktopHistoryPage {
    pub start_index: i32,
    pub limit: i32,
    pub total_record_count: Option<i32>,
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
    generation: Option<u64>,
    undone: Arc<Mutex<bool>>,
}

impl fmt::Debug for HistoryRemoval {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryRemoval")
            .field("item_id", &self.item_id)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl HistoryRemoval {
    pub fn item_id(&self) -> String {
        self.item_id.clone()
    }

    /// Whether this operation created a hide and therefore owns an Undo.
    pub const fn changed(&self) -> bool {
        self.generation.is_some()
    }

    pub async fn undo(&self) -> Result<bool, SdkError> {
        let mut undone = Arc::clone(&self.undone).lock_owned().await;
        let Some(generation) = self.generation.filter(|_| !*undone) else {
            return Ok(false);
        };
        let inner = Arc::clone(&self.inner);
        let token = Arc::clone(&self.token);
        let item_id = self.item_id.clone();
        self.inner
            .handle
            .spawn_blocking(move || {
                let (_, changed) = inner.with_history(&token, true, None, |store, scope| {
                    let changed = store
                        .restore_removal(scope, &item_id, generation)
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
        expected_revision: Option<u64>,
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
        if expected_revision.is_some_and(|expected| expected != *revision) {
            return Err(SdkError::Stale);
        }
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
        self.hide_history_observation(token, item_id, None).await
    }

    /// Hides the observed desktop state. A newer server observation can restore
    /// it; Android's item-only read API never evaluates these observations.
    pub async fn hide_desktop_history_item(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
        last_played_date: Option<String>,
    ) -> Result<Arc<HistoryRemoval>, SdkError> {
        self.hide_history_observation(token, item_id, last_played_date)
            .await
    }

    async fn hide_history_observation(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
        last_played_date: Option<String>,
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
        let (_, generation) = self
            .inner
            .handle
            .spawn_blocking(move || {
                inner.with_history(&token, true, None, |store, scope| {
                    let generation = store
                        .hide_with_observation(scope, &hidden_id, last_played_date.as_deref())
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                    Ok((generation.is_some(), generation))
                })
            })
            .await
            .map_err(|_| SdkError::Storage("history hide worker failed".to_owned()))??;
        Ok(Arc::new(HistoryRemoval {
            inner: Arc::clone(&self.inner),
            token: undo_token,
            item_id,
            generation,
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
        self.history_page(token, start_index, limit, false)
            .await
            .map(|(page, _)| page)
    }

    /// Reads desktop History and observes newer server last-played values for
    /// hidden items. Missing/equal/invalid timestamps do not restore visibility.
    /// Counting never issues requests beyond those needed to fill this page.
    pub async fn desktop_watch_history(
        &self,
        token: Arc<OperationToken>,
        start_index: i32,
        limit: i32,
    ) -> Result<DesktopHistoryPage, SdkError> {
        let (page, total_record_count) = self.history_page(token, start_index, limit, true).await?;
        Ok(DesktopHistoryPage {
            start_index: page.start_index,
            limit: page.limit,
            total_record_count,
            next_start_index: page.next_start_index,
            has_more: page.has_more,
            items: page.items,
        })
    }

    /// Restores desktop visibility after a successful *new* local Playback
    /// Session. The host calls this once on successful admission, never for a
    /// failed start, a progress report, or pause/resume in the current session.
    pub async fn restore_desktop_history_for_playback(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<bool, SdkError> {
        let inner = Arc::clone(&self.inner);
        self.inner
            .handle
            .spawn_blocking(move || {
                inner
                    .with_history(&token, true, None, |store, scope| {
                        let changed = store
                            .set_hidden(scope, &item_id, false)
                            .map_err(|error| SdkError::Storage(error.to_string()))?;
                        Ok((changed, changed))
                    })
                    .map(|(_, changed)| changed)
            })
            .await
            .map_err(|_| SdkError::Storage("history playback worker failed".to_owned()))?
    }

    async fn history_page(
        &self,
        token: Arc<OperationToken>,
        start_index: i32,
        limit: i32,
        desktop: bool,
    ) -> Result<(HistoryPage, Option<i32>), SdkError> {
        let inner = Arc::clone(&self.inner);
        let snapshot_inner = Arc::clone(&inner);
        let snapshot_token = Arc::clone(&token);
        let (mut revision, mut hidden) = self
            .inner
            .handle
            .spawn_blocking(move || {
                snapshot_inner.with_history(&snapshot_token, false, None, |store, scope| {
                    Ok((false, store.hidden_item_ids(scope)))
                })
            })
            .await
            .map_err(|_| SdkError::Storage("history visibility worker failed".to_owned()))??;
        let observation_token = Arc::clone(&token);
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
                    let page = if desktop {
                        let observation_inner = Arc::clone(&inner);
                        let observation_token = Arc::clone(&observation_token);
                        let (next_revision, (page, current_hidden)) = inner
                            .handle
                            .spawn_blocking(move || {
                                observation_inner.with_history(
                                    &observation_token,
                                    true,
                                    Some(revision),
                                    |store, scope| {
                                        let changed = store
                                            .observe_last_played(
                                                scope,
                                                page.items.iter().map(|item| {
                                                    (
                                                        item.id.as_str(),
                                                        item.last_played_date.as_deref(),
                                                    )
                                                }),
                                            )
                                            .map_err(|error| {
                                                SdkError::Storage(error.to_string())
                                            })?;
                                        Ok((changed, (page, store.hidden_item_ids(scope))))
                                    },
                                )
                            })
                            .await
                            .map_err(|_| {
                                SdkError::Storage("history observation worker failed".to_owned())
                            })??;
                        revision = next_revision;
                        hidden = current_hidden;
                        page
                    } else {
                        page
                    };
                    cursor = cursor.checked_add(returned).ok_or_else(|| {
                        SdkError::Request("history page offset overflow".to_owned())
                    })?;
                    items.extend(
                        page.items
                            .into_iter()
                            .filter(|item| !hidden.contains(&item.id)),
                    );
                    if !page.has_more || items.len() >= limit as usize {
                        let visible_total = if hidden.is_empty() {
                            Some(page.total_record_count)
                        } else if start_index == 0 && !page.has_more {
                            Some(i32::try_from(items.len()).map_err(|_| {
                                SdkError::Request("history count overflow".to_owned())
                            })?)
                        } else {
                            None
                        };
                        return Ok((
                            HistoryPage {
                                start_index,
                                limit,
                                total_record_count: page.total_record_count,
                                next_start_index: cursor,
                                has_more: page.has_more,
                                items,
                            },
                            visible_total,
                        ));
                    }
                }
            })
            .await
    }
}
