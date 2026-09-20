//! Authenticated media queries bound to the active profile scope.
//!
//! Every operation runs under an [`OperationToken`]: cancellation drops the
//! in-flight wait, and a scope change between issue and settlement makes the
//! result [`SdkError::Stale`] instead of committing cross-profile data.

use std::sync::Arc;

use jellypilot_media_server::{
    FavoritesPage, FavoritesPageRequest, VideoItemDetail, VideoItemStreams, VideoLibraryItem,
    VideoLibraryShortcut, VideoPlaybackTarget, VideoSeasonEpisodesPage,
    VideoSeasonEpisodesPageRequest, VideoShowDetail, VideoUserDataAction, VideoUserDataUpdate,
};

use crate::{OperationToken, Sdk, SdkError};

impl Sdk {
    /// Selects Home hero candidates using the same resume, source-priority and
    /// episode/series identity rules as desktop. Inputs are already fetched rows.
    pub fn home_featured_items(
        &self,
        home: jellypilot_media_server::VideoHome,
        latest_rows: Vec<Vec<VideoLibraryItem>>,
    ) -> Vec<VideoLibraryItem> {
        use jellypilot_core::home_hero::{candidates, HeroSource};
        use jellypilot_core::LoadState;
        let continue_watching: LoadState<_, ()> = LoadState::Ready(home.continue_watching);
        let next_up = LoadState::Ready(home.next_up);
        let latest: Vec<_> = latest_rows.into_iter().map(LoadState::Ready).collect();
        candidates(&continue_watching, &next_up, latest.iter())
            .into_iter()
            .filter_map(|candidate| {
                let row = match candidate.source {
                    HeroSource::ContinueWatching => &continue_watching,
                    HeroSource::NextUp => &next_up,
                    HeroSource::Latest(index) => latest.get(index)?,
                };
                let LoadState::Ready(items) = row else {
                    return None;
                };
                items.get(candidate.item_index).cloned()
            })
            .collect()
    }

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

    /// The provider's Next Up item and progress, without a client-side fallback.
    pub async fn next_episode_item(
        &self,
        token: Arc<OperationToken>,
        series_id: String,
    ) -> Result<Option<VideoLibraryItem>, SdkError> {
        self.inner
            .scoped(&token, |client| async move {
                client
                    .library()
                    .next_episode_item(series_id)
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

    /// Favorite/played mutation admitted through the shared item-action
    /// executor.
    ///
    /// A second write for the same item — server or Watchlist — fails with
    /// [`SdkError::OperationInProgress`] while one is in flight. The returned
    /// state is authoritative only after server acceptance; a response that
    /// does not confirm the requested flag fails, and a cancelled or stale
    /// result must not be applied to visible content.
    pub async fn update_user_data(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
        action: VideoUserDataAction,
    ) -> Result<VideoUserDataUpdate, SdkError> {
        let admission =
            self.inner
                .admit_item(&token, &item_id, crate::item_actions::server_action(action))?;
        let inner = Arc::clone(&self.inner);
        let worker_token = Arc::clone(&token);
        let receipt = admission.receipt().clone();
        self.inner
            .scoped(&token, move |client| async move {
                let result = inner
                    .item_actions
                    .run_server(admission, client)
                    .await
                    .map_err(SdkError::from)
                    .and_then(|update| {
                        if worker_token.is_cancelled() {
                            return Err(SdkError::Cancelled);
                        }
                        inner.publish_user_data(&worker_token.scope, &update)?;
                        Ok(update)
                    });
                // Publication is part of delivery, not the outer caller's
                // scheduling. A newer same-item write cannot overtake it.
                let _ = inner.item_actions.acknowledge(&receipt);
                result
            })
            .await
    }
}
