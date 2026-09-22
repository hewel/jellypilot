use std::sync::Arc;

use jellypilot_core::browse_model::{BrowsePreferences as CorePreferences, LibraryBrowseView};
use jellypilot_core::LibraryBrowseMode;
use jellypilot_sdk::browse as sdk;

use crate::{
    ProfileScopeRef, SdkError, VideoLibraryItem, VideoLibraryPlayedFilter, VideoLibraryShortcut,
    VideoLibrarySort, VideoLibrarySortDirection,
};

#[derive(Clone, Debug, uniffi::Record)]
pub struct BrowsePreferences {
    pub sort: VideoLibrarySort,
    pub sort_direction: VideoLibrarySortDirection,
    pub played_filter: VideoLibraryPlayedFilter,
    pub favorites_only: bool,
}

impl From<BrowsePreferences> for CorePreferences {
    fn from(value: BrowsePreferences) -> Self {
        Self {
            sort: value.sort.into(),
            sort_direction: value.sort_direction.into(),
            played_filter: value.played_filter.into(),
            favorites_only: value.favorites_only,
            filters: Default::default(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum BrowseQuery {
    Library {
        library: VideoLibraryShortcut,
        preferences: BrowsePreferences,
    },
    Search {
        query: String,
    },
}

impl From<BrowseQuery> for sdk::BrowseQuery {
    fn from(value: BrowseQuery) -> Self {
        match value {
            BrowseQuery::Library {
                library,
                preferences,
            } => Self::Library {
                library: library.into(),
                preferences: preferences.into(),
            },
            BrowseQuery::Search { query } => Self::Search { query },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum BrowseStatus {
    Inactive,
    Loading,
    Empty,
    Ready,
    Failed,
}

/// Sparse slots retain their absolute index; clients must not compact them.
#[derive(Clone, Debug, uniffi::Record)]
pub struct BrowseSnapshot {
    pub revision: u64,
    pub scope: ProfileScopeRef,
    pub identity: String,
    pub status: BrowseStatus,
    pub items: Vec<Option<VideoLibraryItem>>,
    pub visible_start: u32,
    pub total_count: u32,
    pub is_virtual: bool,
    pub loading_more: bool,
    pub error: Option<String>,
    pub retryable: bool,
    pub retry_busy: bool,
    pub refreshing: bool,
    pub refresh_error: Option<String>,
}

impl From<sdk::BrowseSnapshot> for BrowseSnapshot {
    fn from(value: sdk::BrowseSnapshot) -> Self {
        let mut result = Self {
            revision: value.revision,
            scope: value.scope.into(),
            identity: value.identity,
            status: BrowseStatus::Inactive,
            items: Vec::new(),
            visible_start: 0,
            total_count: 0,
            is_virtual: false,
            loading_more: false,
            error: None,
            retryable: value
                .refresh_failure
                .as_ref()
                .is_some_and(|failure| failure.retryable),
            retry_busy: false,
            refreshing: value.refreshing,
            refresh_error: value.refresh_failure.map(|failure| failure.message),
        };
        match value.view {
            LibraryBrowseView::Inactive => {}
            LibraryBrowseView::Loading => result.status = BrowseStatus::Loading,
            LibraryBrowseView::Empty => result.status = BrowseStatus::Empty,
            LibraryBrowseView::Failed {
                message,
                retryable,
                retry_busy,
            } => {
                result.status = BrowseStatus::Failed;
                result.error = Some(message);
                result.retryable = retryable;
                result.retry_busy = retry_busy;
            }
            LibraryBrowseView::Ready {
                mode,
                total_record_count,
                visible_items,
                visible_start,
                is_fetching_more,
                load_more_failure,
                retry_busy,
            } => {
                result.status = BrowseStatus::Ready;
                result.items = visible_items
                    .into_iter()
                    .map(|slot| slot.item.map(Into::into))
                    .collect();
                result.visible_start = visible_start;
                result.total_count = total_record_count;
                result.is_virtual = mode == LibraryBrowseMode::Virtual;
                result.loading_more = is_fetching_more;
                if let Some(failure) = load_more_failure {
                    result.retryable |= failure.retryable;
                    result.error = Some(failure.message);
                }
                result.retry_busy = retry_busy;
            }
        }
        result
    }
}

/// Immutable query/scope binding; paging, refresh and cancellation live in Rust.
#[derive(uniffi::Object)]
pub struct BrowseSession {
    pub(crate) inner: Arc<sdk::BrowseSession>,
}

#[uniffi::export(async_runtime = "tokio")]
impl BrowseSession {
    pub fn snapshot(&self) -> Result<BrowseSnapshot, SdkError> {
        self.inner.snapshot().map(Into::into).map_err(Into::into)
    }

    pub async fn next_snapshot(&self, after_revision: u64) -> Result<BrowseSnapshot, SdkError> {
        self.inner
            .next_snapshot(after_revision)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }

    pub fn set_display_range(&self, start: u32, end: u32) -> Result<(), SdkError> {
        self.inner.set_display_range(start, end).map_err(Into::into)
    }

    pub fn refresh(&self) -> Result<(), SdkError> {
        self.inner.refresh().map_err(Into::into)
    }

    pub fn retry(&self) -> Result<(), SdkError> {
        self.inner.retry().map_err(Into::into)
    }

    pub fn suspend(&self) -> Result<(), SdkError> {
        self.inner.suspend().map_err(Into::into)
    }

    pub fn resume(&self) -> Result<(), SdkError> {
        self.inner.resume().map_err(Into::into)
    }

    /// Cancels work and wakes snapshot waiters before disposing the FFI handle.
    /// `close` is reserved by UniFFI's generated Kotlin AutoCloseable adapter.
    pub fn shutdown(&self) {
        self.inner.close();
    }
}
