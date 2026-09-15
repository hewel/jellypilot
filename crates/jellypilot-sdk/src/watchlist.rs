//! Device-local Watchlist operations partitioned by profile scope.
//!
//! The watchlist is device-local membership, not server Favorites and not
//! Watch History. Records are owned by the active profile scope; sign-out
//! removes them only when the caller opts in. Every operation still honors
//! the caller's [`OperationToken`]: token epoch validation, scope capture,
//! and the store read or mutation run under one state lock, so a token
//! minted under an ended scope cannot touch the new scope's records.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use jellypilot_core::watchlist::WatchlistRecord;
use jellypilot_media_server::VideoLibraryItem;

use crate::{OperationToken, Sdk, SdkError};

impl Sdk {
    /// Watchlist records for the active profile, most recently added first.
    pub async fn watchlist_items(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Vec<WatchlistRecord>, SdkError> {
        self.run_watchlist(token, |store, scope| Ok(store.records_for(scope)))
            .await
    }

    /// Adds the item to the active profile's device watchlist.
    ///
    /// `item` supplies the fallback presentation data stored with the
    /// membership so an unavailable item stays identifiable.
    pub async fn watchlist_add(
        &self,
        token: Arc<OperationToken>,
        item: VideoLibraryItem,
    ) -> Result<bool, SdkError> {
        let added_at_unix_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        self.run_watchlist(token, move |store, scope| {
            let record = WatchlistRecord::from_item(scope.clone(), &item, added_at_unix_millis)
                .map_err(|error| SdkError::InvalidInput(error.to_string()))?;
            store
                .add(record)
                .map_err(|error| SdkError::Storage(error.to_string()))
        })
        .await
    }

    /// Removes the item from the active profile's device watchlist.
    pub async fn watchlist_remove(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<bool, SdkError> {
        self.run_watchlist(token, move |store, scope| {
            store
                .remove(scope, &item_id)
                .map_err(|error| SdkError::Storage(error.to_string()))
        })
        .await
    }

    /// Whether the item is in the active profile's device watchlist.
    pub async fn watchlist_contains(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<bool, SdkError> {
        self.run_watchlist(token, move |store, scope| {
            Ok(store.contains(scope, &item_id))
        })
        .await
    }
}
