//! Authenticated media queries bound to the active profile scope.
//!
//! Every operation runs under an [`OperationToken`]: cancellation drops the
//! in-flight wait, and a scope change between issue and settlement makes the
//! result [`SdkError::Stale`] instead of committing cross-profile data.

use std::sync::Arc;

use jellypilot_media_server::{
    FavoritesPage, FavoritesPageRequest, VideoItemDetail, VideoItemStreams, VideoLibraryItem,
    VideoLibraryPage, VideoLibraryPageRequest, VideoLibraryShortcut, VideoPlaybackTarget,
    VideoSearchPage, VideoSearchRequest, VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest,
    VideoShowDetail, VideoUserDataAction, VideoUserDataUpdate, VideoUserDataUpdateRequest,
    WatchHistoryPage, WatchHistoryPageRequest,
};

use crate::{OperationToken, Sdk, SdkError};

impl Sdk {
    /// Video Home landing rows (Continue Watching, Next Up).
    pub async fn video_home(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<jellypilot_media_server::VideoHome, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client.library().video_home().await.map_err(SdkError::from)
            })
            .await
    }

    /// Movies/Shows library shortcuts for browse navigation.
    pub async fn library_shortcuts(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Vec<VideoLibraryShortcut>, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .library_shortcuts()
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Latest items for one library row.
    pub async fn library_latest(
        &self,
        token: Arc<OperationToken>,
        library_id: String,
    ) -> Result<Vec<VideoLibraryItem>, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .library_latest(library_id)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Paged Library Browser listing.
    pub async fn browse_video(
        &self,
        token: Arc<OperationToken>,
        request: VideoLibraryPageRequest,
    ) -> Result<VideoLibraryPage, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .browse_video(request)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Paged video-only library search.
    pub async fn search_video(
        &self,
        token: Arc<OperationToken>,
        request: VideoSearchRequest,
    ) -> Result<VideoSearchPage, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .search_video(request)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Root-level page of the user's video Favorites.
    pub async fn favorites(
        &self,
        token: Arc<OperationToken>,
        start_index: i32,
        limit: i32,
    ) -> Result<FavoritesPage, SdkError> {
        self.inner
            .scoped(&token, move |client| async move {
                client
                    .library()
                    .favorites(FavoritesPageRequest { start_index, limit })
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Server Watch History page (played and resumable items).
    pub async fn watch_history(
        &self,
        token: Arc<OperationToken>,
        start_index: i32,
        limit: i32,
    ) -> Result<WatchHistoryPage, SdkError> {
        self.inner
            .scoped(&token, move |client| async move {
                client
                    .library()
                    .history(WatchHistoryPageRequest { start_index, limit })
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Playable Movie or Episode detail.
    pub async fn item_detail(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<VideoItemDetail, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .item_detail(item_id)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Audio/subtitle stream metadata for a detail item.
    pub async fn item_streams(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<VideoItemStreams, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .item_streams(item_id)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Show detail with seasons and next playable episode.
    pub async fn show_detail(
        &self,
        token: Arc<OperationToken>,
        series_id: String,
    ) -> Result<VideoShowDetail, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .show_detail(series_id)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Bounded page of episodes inside a show season.
    pub async fn season_episodes_page(
        &self,
        token: Arc<OperationToken>,
        request: VideoSeasonEpisodesPageRequest,
    ) -> Result<VideoSeasonEpisodesPage, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .season_episodes_page(request)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Related items for a detail view.
    pub async fn similar_video(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
    ) -> Result<Vec<VideoLibraryItem>, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .similar_video(item_id)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Next playable episode for a series, when the provider exposes one.
    pub async fn next_playable_episode(
        &self,
        token: Arc<OperationToken>,
        series_id: String,
    ) -> Result<Option<VideoPlaybackTarget>, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .next_playable_episode(series_id)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Batch item lookup used by watchlist and retained-content enrichment.
    pub async fn video_items_by_ids(
        &self,
        token: Arc<OperationToken>,
        item_ids: Vec<String>,
    ) -> Result<Vec<VideoLibraryItem>, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .video_items_by_ids(item_ids)
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }

    /// Favorite/played mutation. The returned state is authoritative only
    /// after server acceptance; a cancelled or stale result must not be
    /// applied to visible content.
    pub async fn update_user_data(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
        action: VideoUserDataAction,
    ) -> Result<VideoUserDataUpdate, SdkError> {
        self.inner
            .scoped(&token, move |client| async move {
                client
                    .library()
                    .update_user_data(VideoUserDataUpdateRequest { item_id, action })
                    .await
                    .map_err(SdkError::from)
            })
            .await
    }
}
