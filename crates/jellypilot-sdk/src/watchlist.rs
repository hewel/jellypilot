//! Device-local Watchlist operations partitioned by profile scope.
//!
//! The watchlist is device-local membership, not server Favorites and not
//! Watch History. Records are owned by the active profile scope; sign-out
//! removes them only when the caller opts in. Every operation still honors
//! the caller's [`OperationToken`]: token epoch validation, scope capture,
//! and the store read or mutation run under one state lock, so a token
//! minted under an ended scope cannot touch the new scope's records.
//!
//! Writes are admitted through [`crate::item_actions::ItemActions`], the same
//! per-item admission authority the desktop frontend uses, so a Watchlist
//! write and a server write for the same item can never run concurrently.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jellypilot_core::watchlist::WatchlistRecord;
use jellypilot_media_server::VideoLibraryItem;

use crate::item_actions::{
    Admission, ItemActionError, WatchlistSnapshot, WatchlistStorage, WatchlistWrite,
    WatchlistWriteAction,
};
use crate::{OperationToken, Sdk, SdkError, SdkInner};

/// Scope-bound undo for an atomic Watchlist removal. The original records are
/// retained inside Rust, so callers cannot forge another profile's membership.
pub struct WatchlistRemoval {
    inner: Arc<SdkInner>,
    token: Arc<OperationToken>,
    records: Vec<WatchlistRecord>,
    undo_gate: Arc<tokio::sync::Mutex<bool>>,
}

impl WatchlistRemoval {
    pub fn removed_entries(&self) -> Vec<WatchlistRecord> {
        self.records.clone()
    }

    /// Restores the exact removed records. Repeating a successful undo is harmless.
    pub async fn undo(&self) -> Result<bool, SdkError> {
        let mut undone = Arc::clone(&self.undo_gate).lock_owned().await;
        if *undone {
            return Ok(false);
        }
        let admissions = self
            .records
            .iter()
            .map(|record| {
                self.inner
                    .admit_item(
                        &self.token,
                        record.item_id(),
                        jellypilot_core::item_actions::Action::Watchlist(true),
                    )
                    .map(Admission::immediate)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let inner = Arc::clone(&self.inner);
        let token = Arc::clone(&self.token);
        let records = self.records.clone();
        self.inner
            .handle
            .spawn_blocking(move || {
                let result = inner
                    .with_scoped_watchlist_write(&token, move |store, scope| {
                        if records.iter().any(|record| record.scope() != scope) {
                            return Err(SdkError::Stale);
                        }
                        let changed = store
                            .restore_items(&records)
                            .map_err(|error| SdkError::Storage(error.to_string()))?;
                        Ok((changed, (changed, store.records_for(scope))))
                    })
                    .map(|(revision, (changed, records))| WatchlistSnapshot {
                        revision,
                        records,
                        changed,
                    });
                let changed = result
                    .as_ref()
                    .map(|snapshot| snapshot.changed)
                    .map_err(Clone::clone);
                for admission in admissions {
                    admission.finish_watchlist(result.clone())?;
                }
                if changed.is_ok() {
                    *undone = true;
                }
                changed
            })
            .await
            .map_err(|_| SdkError::Storage("the Watchlist undo worker failed".to_owned()))?
    }
}

/// Watchlist storage adapter over the SDK-owned store.
///
/// The store mutation runs under the SDK state lock through
/// [`SdkInner::with_scoped_watchlist_write`], so token epoch validation, live-scope
/// capture, and the write itself stay serialized against account
/// transitions; the shared revision advances only when membership changed.
struct ScopedWatchlist {
    inner: Arc<SdkInner>,
    token: Arc<OperationToken>,
}

impl WatchlistStorage for ScopedWatchlist {
    async fn apply(
        &self,
        admission: Admission,
        write: WatchlistWrite,
    ) -> Result<WatchlistSnapshot, ItemActionError> {
        let inner = Arc::clone(&self.inner);
        let token = Arc::clone(&self.token);
        self.inner
            .handle
            .spawn_blocking(move || {
                let result = inner
                    .with_scoped_watchlist_write(&token, move |store, scope| {
                        let changed = match write.action {
                            WatchlistWriteAction::Add(item) => {
                                let added_at_unix_millis = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .map(|duration| duration.as_millis() as u64)
                                    .unwrap_or(0);
                                let record = WatchlistRecord::from_item(
                                    scope.clone(),
                                    &item,
                                    added_at_unix_millis,
                                )
                                .map_err(|error| SdkError::InvalidInput(error.to_string()))?;
                                store
                                    .add(record)
                                    .map_err(|error| SdkError::Storage(error.to_string()))?
                            }
                            WatchlistWriteAction::Remove => store
                                .remove(scope, &write.item_id)
                                .map_err(|error| SdkError::Storage(error.to_string()))?,
                        };
                        Ok((changed, (changed, store.records_for(scope))))
                    })
                    .map(|(revision, (changed, records))| WatchlistSnapshot {
                        revision,
                        changed,
                        records,
                    });
                admission.finish_watchlist(result)
            })
            .await
            .map_err(|error| {
                ItemActionError::Failed(if error.is_cancelled() {
                    SdkError::Closed
                } else {
                    SdkError::Storage("the watchlist task failed unexpectedly".to_owned())
                })
            })?
    }
}

impl Sdk {
    /// Removes the selected Watchlist entries in one atomic storage transaction.
    /// Failure admits no partial selection; successful undo preserves original order.
    pub async fn remove_watchlist_items(
        &self,
        token: Arc<OperationToken>,
        mut item_ids: Vec<String>,
    ) -> Result<Arc<WatchlistRemoval>, SdkError> {
        item_ids.sort();
        item_ids.dedup();
        let admissions = item_ids
            .iter()
            .map(|id| {
                self.inner
                    .admit_item(
                        &token,
                        id,
                        jellypilot_core::item_actions::Action::Watchlist(false),
                    )
                    .map(Admission::immediate)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let undo_token = self.new_operation_token()?;
        if undo_token.scope_ref()? != token.scope_ref()? {
            return Err(SdkError::Stale);
        }
        let inner = Arc::clone(&self.inner);
        let records = self
            .inner
            .handle
            .spawn_blocking(move || {
                let result = inner.with_scoped_watchlist_write(&token, move |store, scope| {
                    let removed = store
                        .remove_items(scope, &item_ids)
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                    Ok((!removed.is_empty(), (removed, store.records_for(scope))))
                });
                let snapshot = result
                    .as_ref()
                    .map(|(revision, (removed, records))| WatchlistSnapshot {
                        revision: *revision,
                        changed: !removed.is_empty(),
                        records: records.clone(),
                    })
                    .map_err(Clone::clone);
                for admission in admissions {
                    admission.finish_watchlist(snapshot.clone())?;
                }
                result.map(|(_, (removed, _))| removed)
            })
            .await
            .map_err(|_| SdkError::Storage("the Watchlist removal worker failed".to_owned()))??;
        Ok(Arc::new(WatchlistRemoval {
            inner: Arc::clone(&self.inner),
            token: undo_token,
            records,
            undo_gate: Arc::new(tokio::sync::Mutex::new(false)),
        }))
    }

    /// Watchlist records for the active profile, most recently added first.
    pub async fn watchlist_items(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Vec<WatchlistRecord>, SdkError> {
        self.inner
            .run_watchlist(token, |store, scope| Ok(store.records_for(scope)))
            .await
    }

    /// Adds the item to the active profile's device watchlist.
    ///
    /// `item` supplies the fallback presentation data stored with the
    /// membership so an unavailable item stays identifiable. A second write
    /// for the same item — Watchlist or server — fails with
    /// [`SdkError::OperationInProgress`] while one is in flight.
    pub async fn watchlist_add(
        &self,
        token: Arc<OperationToken>,
        item: VideoLibraryItem,
    ) -> Result<bool, SdkError> {
        let admission = self.inner.admit_item(
            &token,
            &item.id,
            jellypilot_core::item_actions::Action::Watchlist(true),
        )?;
        let storage = ScopedWatchlist {
            inner: Arc::clone(&self.inner),
            token,
        };
        let snapshot = self
            .inner
            .item_actions
            .run_watchlist(
                admission.immediate(),
                storage,
                WatchlistWrite {
                    item_id: item.id.clone(),
                    action: WatchlistWriteAction::Add(Box::new(item)),
                },
            )
            .await?;
        Ok(snapshot.changed)
    }

    /// Removes the item from the active profile's device watchlist.
    pub async fn watchlist_remove(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<bool, SdkError> {
        let admission = self.inner.admit_item(
            &token,
            &item_id,
            jellypilot_core::item_actions::Action::Watchlist(false),
        )?;
        let storage = ScopedWatchlist {
            inner: Arc::clone(&self.inner),
            token,
        };
        let snapshot = self
            .inner
            .item_actions
            .run_watchlist(
                admission.immediate(),
                storage,
                WatchlistWrite {
                    item_id,
                    action: WatchlistWriteAction::Remove,
                },
            )
            .await?;
        Ok(snapshot.changed)
    }

    /// Whether the item is in the active profile's device watchlist.
    pub async fn watchlist_contains(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<bool, SdkError> {
        self.inner
            .run_watchlist(token, move |store, scope| {
                Ok(store.contains(scope, &item_id))
            })
            .await
    }
}
