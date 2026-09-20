use crate::{JellypilotSdk, OperationToken, SdkError, WatchlistEntry};
use std::sync::Arc;

#[derive(uniffi::Object)]
pub struct WatchlistRemoval {
    inner: Arc<jellypilot_sdk::WatchlistRemoval>,
}

#[uniffi::export(async_runtime = "tokio")]
impl WatchlistRemoval {
    pub fn removed_entries(&self) -> Vec<WatchlistEntry> {
        self.inner
            .removed_entries()
            .into_iter()
            .map(Into::into)
            .collect()
    }
    pub async fn undo(&self) -> Result<bool, SdkError> {
        self.inner.undo().await.map_err(Into::into)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl JellypilotSdk {
    pub async fn remove_watchlist_items(
        &self,
        token: Arc<OperationToken>,
        item_ids: Vec<String>,
    ) -> Result<Arc<WatchlistRemoval>, SdkError> {
        self.sdk
            .remove_watchlist_items(Arc::clone(&token.inner), item_ids)
            .await
            .map(|inner| Arc::new(WatchlistRemoval { inner }))
            .map_err(Into::into)
    }
}
