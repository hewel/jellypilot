use std::sync::Arc;

use jellypilot_media_server::{
    JellyfinClient, JellyfinError, VideoItemDetail, VideoLibraryItem, VideoSeason,
    VideoSeasonEpisodesPageRequest, VideoShowDetail, VideoUserDataUpdate,
};

use crate::LoadState;

pub const SEASON_EPISODE_PAGE_SIZE: i32 = 30;

#[derive(Clone)]
pub enum DetailContent {
    Item(Box<VideoItemDetail>),
    Show(Box<VideoShowDetail>),
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
            .map(|detail| DetailContent::Show(Box::new(detail)))
            .map_err(|error| error.to_string())
    } else {
        client
            .library()
            .item_detail(item.id)
            .await
            .map(|detail| DetailContent::Item(Box::new(detail)))
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

/// Loads provider-neutral similar video cards for a detail shelf.
pub async fn load_similar_items(
    client: &JellyfinClient,
    item_id: String,
) -> Result<Vec<VideoLibraryItem>, JellyfinError> {
    client.library().similar_video(item_id).await
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

/// Picks the season a show detail opens on: the season of the next-up
/// episode, falling back to the first listed season.
#[must_use]
pub fn initial_season(show: &VideoShowDetail) -> Option<&VideoSeason> {
    show.next_episode
        .as_ref()
        .and_then(|episode| episode.season_number)
        .and_then(|season_number| {
            show.seasons
                .iter()
                .find(|season| season.season_number == Some(season_number))
        })
        .or_else(|| show.seasons.first())
}

/// Builds the first-page episodes request for the show's selected season, or
/// `None` when the selection does not resolve to a season of the loaded show.
#[must_use]
pub fn selected_season_request(
    detail: &LoadState<DetailContent>,
    selected_season_id: Option<&str>,
) -> Option<VideoSeasonEpisodesPageRequest> {
    let LoadState::Ready(DetailContent::Show(show)) = detail else {
        return None;
    };
    let selected_season_id = selected_season_id?;
    let season = show
        .seasons
        .iter()
        .find(|season| season.id == selected_season_id)?;
    Some(season_page_request(&show.id, season, 0))
}

/// Artwork cell key shared by the detail update and view for an episode card.
#[must_use]
pub fn detail_episode_key(item_id: &str) -> String {
    format!("detail-episode:{item_id}")
}

/// Artwork cell key shared by the detail update and view for a similar-item card.
#[must_use]
pub fn detail_similar_key(item_id: &str) -> String {
    format!("detail-similar:{item_id}")
}

/// Reads the current (item id, played, favorite) user-data flags of ready
/// detail content; `None` while no content is ready.
#[must_use]
pub fn detail_user_data(detail: &LoadState<DetailContent>) -> Option<(String, bool, bool)> {
    match detail {
        LoadState::Ready(DetailContent::Item(item)) => {
            Some((item.id.clone(), item.played, item.favorite))
        }
        LoadState::Ready(DetailContent::Show(show)) => {
            Some((show.id.clone(), show.played, show.favorite))
        }
        LoadState::Idle | LoadState::Loading | LoadState::Failed(_) => None,
    }
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
        let mut detail = LoadState::Ready(DetailContent::Show(Box::new(VideoShowDetail {
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
            logo_image_id: None,
            next_episode: None,
            seasons: Vec::new(),
            metadata: Default::default(),
        })));
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
            &detail,
            LoadState::Ready(DetailContent::Show(show)) if show.played && show.favorite
        ));
    }

    fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
        VideoLibraryItem {
            id: id.to_owned(),
            name: "Episode".to_owned(),
            item_type: "Episode".to_owned(),
            production_year: None,
            runtime_seconds: Some(1_800.0),
            played: false,
            favorite: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            logo_image_id: None,
            series_poster_image_id: None,
            season_number: Some(season_number),
            episode_thumb_image_id: None,
            series_thumb_image_id: None,
            series_backdrop_image_id: None,
            episode_number: Some(1),
            series_id: Some("show-1".to_owned()),
            series_name: Some("Show".to_owned()),
            resume_position_seconds: None,
            played_percentage: None,
            overview: None,
            index_number_end: None,
            season_poster_image_id: None,
            end_year: None,
            series_continuing: false,
            unplayed_item_count: None,
        }
    }

    fn season(id: &str, number: i32) -> VideoSeason {
        VideoSeason {
            id: id.to_owned(),
            name: format!("Season {number}"),
            season_number: Some(number),
            played: false,
            favorite: false,
            artwork_image_id: None,
        }
    }

    fn show_detail(next_episode: Option<VideoLibraryItem>) -> VideoShowDetail {
        VideoShowDetail {
            id: "show-1".to_owned(),
            name: "Show".to_owned(),
            overview: None,
            production_year: None,
            genres: Vec::new(),
            played: false,
            favorite: true,
            can_play: true,
            artwork_image_id: None,
            backdrop_image_id: None,
            logo_image_id: None,
            next_episode,
            seasons: vec![season("season-1", 1), season("season-2", 2)],
            metadata: Default::default(),
        }
    }

    #[test]
    fn initial_season_prefers_the_next_up_episodes_season_then_the_first_season() {
        let show = show_detail(Some(episode("episode-2", 2)));
        assert_eq!(
            initial_season(&show).map(|season| season.id.as_str()),
            Some("season-2")
        );

        let show = show_detail(None);
        assert_eq!(
            initial_season(&show).map(|season| season.id.as_str()),
            Some("season-1")
        );
    }

    #[test]
    fn selected_season_request_resolves_only_a_season_of_the_loaded_show() {
        let detail = LoadState::Ready(DetailContent::Show(Box::new(show_detail(None))));

        let request = selected_season_request(&detail, Some("season-2"))
            .expect("selected season should produce a page");
        assert_eq!(request.series_id, "show-1");
        assert_eq!(request.season_id.as_deref(), Some("season-2"));
        assert_eq!(request.season_number, Some(2));
        assert_eq!(request.start_index, 0);
        assert_eq!(request.limit, SEASON_EPISODE_PAGE_SIZE);

        assert!(selected_season_request(&detail, Some("missing-season")).is_none());
        assert!(selected_season_request(&detail, None).is_none());
        assert!(selected_season_request(&LoadState::Loading, Some("season-2")).is_none());
    }

    #[test]
    fn detail_episode_key_scopes_episode_cells_under_the_detail_prefix() {
        assert_eq!(detail_episode_key("ep-1"), "detail-episode:ep-1");
    }

    #[test]
    fn detail_similar_key_scopes_cells_under_the_detail_similar_prefix() {
        assert_eq!(detail_similar_key("movie-1"), "detail-similar:movie-1");
    }

    #[test]
    fn detail_user_data_reads_flags_only_from_ready_content() {
        assert!(detail_user_data(&LoadState::Loading).is_none());

        let detail = LoadState::Ready(DetailContent::Show(Box::new(show_detail(None))));
        let (item_id, played, favorite) =
            detail_user_data(&detail).expect("ready content exposes user data");
        assert_eq!(item_id, "show-1");
        assert!(!played);
        assert!(favorite);
    }
}
