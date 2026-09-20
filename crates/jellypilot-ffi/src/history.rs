use std::sync::Arc;

use crate::{JellypilotSdk, OperationToken, SdkError};

#[derive(uniffi::Object)]
pub struct HistoryRemoval {
    inner: Arc<jellypilot_sdk::HistoryRemoval>,
}

#[uniffi::export(async_runtime = "tokio")]
impl HistoryRemoval {
    pub fn item_id(&self) -> String {
        self.inner.item_id()
    }

    /// Restores local visibility once. False means no hidden membership changed;
    /// callers must not insert a duplicate row or increment a fabricated count.
    pub async fn undo(&self) -> Result<bool, SdkError> {
        self.inner.undo().await.map_err(Into::into)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl JellypilotSdk {
    pub async fn hide_history_item(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<Arc<HistoryRemoval>, SdkError> {
        self.sdk
            .hide_history_item(Arc::clone(&token.inner), item_id)
            .await
            .map(|inner| Arc::new(HistoryRemoval { inner }))
            .map_err(Into::into)
    }
}
