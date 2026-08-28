use std::sync::Arc;

use jellypilot_media_server::{
    JellyfinClient, VideoItemDetail, VideoLibraryItem, VideoSeason, VideoSeasonEpisodesPageRequest,
    VideoShowDetail, VideoUserDataUpdate,
};

use crate::LoadState;

pub const SEASON_EPISODE_PAGE_SIZE: i32 = 30;

#[derive(Clone)]
pub enum DetailContent {
    Item(VideoItemDetail),
    Show(VideoShowDetail),
}

pub async fn load_detail_content(
    client: Arc<JellyfinClient>,
    item: VideoLibraryItem,
) -> Result<DetailContent, String> {
    if item.item_type.eq_ignore_ascii_case("series") {
        client
            .library()
            .show_detail(item.id)
            .await
            .map(DetailContent::Show)
            .map_err(|error| error.to_string())
    } else {
        client
            .library()
            .item_detail(item.id)
            .await
            .map(DetailContent::Item)
            .map_err(|error| error.to_string())
    }
}

pub async fn load_season_neighbors(
    client: Arc<JellyfinClient>,
    item_id: String,
    series_id: String,
    season_number: i32,
) -> Result<Vec<VideoLibraryItem>, String> {
    client
        .library()
        .season_episodes_page(VideoSeasonEpisodesPageRequest {
            series_id,
            season_id: None,
            season_number: Some(season_number),
            start_index: 0,
            limit: SEASON_EPISODE_PAGE_SIZE,
        })
        .await
        .map(|page| {
            page.episodes
                .into_iter()
                .filter(|episode| episode.id != item_id)
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[must_use]
pub fn detail_metadata(detail: &VideoItemDetail) -> String {
    let mut details = Vec::new();
    if let Some(year) = detail.production_year {
        details.push(year.to_string());
    }
    details.push(detail.item_type.clone());
    if !detail.genres.is_empty() {
        details.push(detail.genres.join(", "));
    }
    if detail.favorite {
        details.push("Favorite".to_owned());
    }
    details.join(" · ")
}

#[must_use]
pub fn show_detail_metadata(detail: &VideoShowDetail) -> String {
    let mut details = Vec::new();
    if let Some(year) = detail.production_year {
        details.push(year.to_string());
    }
    details.push("Series".to_owned());
    if !detail.genres.is_empty() {
        details.push(detail.genres.join(", "));
    }
    if detail.favorite {
        details.push("Favorite".to_owned());
    }
    details.join(" · ")
}

#[must_use]
pub fn season_page_request(
    series_id: &str,
    season: &VideoSeason,
    start_index: i32,
) -> VideoSeasonEpisodesPageRequest {
    VideoSeasonEpisodesPageRequest {
        series_id: series_id.to_owned(),
        season_id: Some(season.id.clone()),
        season_number: season.season_number,
        start_index: start_index.max(0),
        limit: SEASON_EPISODE_PAGE_SIZE,
    }
}

pub fn apply_user_data_update(
    detail: &mut LoadState<DetailContent>,
    update: &VideoUserDataUpdate,
) -> bool {
    match detail {
        LoadState::Ready(DetailContent::Item(item)) if item.id == update.item_id => {
            item.played = update.played;
            item.favorite = update.favorite;
            true
        }
        LoadState::Ready(DetailContent::Show(show)) if show.id == update.item_id => {
            show.played = update.played;
            show.favorite = update.favorite;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn season_page_request_uses_exact_identity_and_a_bounded_window() {
        let season = VideoSeason {
            id: "season-2".to_owned(),
            name: "Season 2".to_owned(),
            season_number: Some(2),
            played: false,
            favorite: false,
            artwork_image_id: None,
        };

        let request = season_page_request("show-1", &season, 60);

        assert_eq!(request.series_id, "show-1");
        assert_eq!(request.season_id.as_deref(), Some("season-2"));
        assert_eq!(request.season_number, Some(2));
        assert_eq!(request.start_index, 60);
        assert_eq!(request.limit, 30);
    }

    #[test]
    fn user_data_completion_updates_only_the_matching_detail() {
        let mut detail = LoadState::Ready(DetailContent::Show(VideoShowDetail {
            id: "show-1".to_owned(),
            name: "Show".to_owned(),
            overview: None,
            production_year: None,
            genres: Vec::new(),
            played: false,
            favorite: false,
            can_play: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            next_episode: None,
            seasons: Vec::new(),
            metadata: Default::default(),
        }));
        let stale = VideoUserDataUpdate {
            item_id: "show-2".to_owned(),
            played: true,
            favorite: true,
        };
        assert!(!apply_user_data_update(&mut detail, &stale));
        let current = VideoUserDataUpdate {
            item_id: "show-1".to_owned(),
            played: true,
            favorite: true,
        };
        assert!(apply_user_data_update(&mut detail, &current));
        assert!(matches!(
            detail,
            LoadState::Ready(DetailContent::Show(VideoShowDetail {
                played: true,
                favorite: true,
                ..
            }))
        ));
    }
}
