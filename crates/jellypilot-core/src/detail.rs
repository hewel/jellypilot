use std::collections::HashSet;
use std::sync::Arc;

use jellypilot_media_server::{
    JellyfinClient, JellyfinError, VideoItemDetail, VideoLibraryItem, VideoSeason,
    VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest, VideoShowDetail, VideoUserDataUpdate,
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
    request: VideoSeasonEpisodesPageRequest,
) -> Result<VideoSeasonEpisodesPage, String> {
    client
        .library()
        .season_episodes_page(request)
        .await
        .map(|mut page| {
            // The server cursor includes the current episode even though its
            // neighbor shelf does not, so keep all paging metadata unchanged.
            page.episodes.retain(|episode| episode.id != item_id);
            page
        })
        .map_err(|error| error.to_string())
}

/// Continues from the server cursor, including records omitted by card mapping
/// or by the current episode's neighbor shelf.
#[must_use]
pub fn next_season_page_request(
    page: &VideoSeasonEpisodesPage,
) -> Option<VideoSeasonEpisodesPageRequest> {
    (page.has_more && page.next_start_index > page.start_index).then(|| {
        VideoSeasonEpisodesPageRequest {
            series_id: page.series_id.clone(),
            season_id: page.season_id.clone(),
            season_number: page.season_number,
            start_index: page.next_start_index,
            limit: SEASON_EPISODE_PAGE_SIZE,
        }
    })
}

/// Appends only the next page of the same season, retaining loaded episode
/// order and ignoring overlapping records if the server's library changed.
/// Returns `false` without changing content for a mismatched page.
#[must_use]
pub fn append_season_page(
    loaded: &mut VideoSeasonEpisodesPage,
    mut next: VideoSeasonEpisodesPage,
) -> bool {
    if !loaded.has_more
        || next.series_id != loaded.series_id
        || next.season_id != loaded.season_id
        || next.season_number != loaded.season_number
        || next.start_index != loaded.next_start_index
    {
        return false;
    }
    let mut ids: HashSet<_> = loaded.episodes.iter().map(|item| item.id.clone()).collect();
    next.episodes.retain(|item| ids.insert(item.id.clone()));
    loaded.episodes.extend(next.episodes);
    loaded.total_record_count = next.total_record_count;
    loaded.has_more = next.has_more && next.next_start_index > next.start_index;
    loaded.next_start_index = next.next_start_index;
    true
}

/// Loads provider-neutral similar video cards for a detail shelf.
pub async fn load_similar_items(
    client: &JellyfinClient,
    item_id: String,
) -> Result<Vec<VideoLibraryItem>, JellyfinError> {
    client.library().similar_video(item_id).await
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

/// Resolves a season of the loaded show by its server season number, e.g. the
/// season an episode detail was opened from. `None` when the number is absent
/// or matches no listed season; callers then keep the normal default season.
#[must_use]
pub fn season_for_number(show: &VideoShowDetail, season_number: i32) -> Option<&VideoSeason> {
    show.seasons
        .iter()
        .find(|season| season.season_number == Some(season_number))
}

/// Builds the first-page episodes request for the show's selected season, or
/// `None` when the selection does not resolve to a season of the loaded show.
#[must_use]
pub fn selected_season_request<E>(
    detail: &LoadState<DetailContent, E>,
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
pub fn detail_user_data<E>(detail: &LoadState<DetailContent, E>) -> Option<(String, bool, bool)> {
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

pub fn apply_user_data_update<E>(
    detail: &mut LoadState<DetailContent, E>,
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

    fn episode_page(start: i32, end: i32, total: i32) -> VideoSeasonEpisodesPage {
        VideoSeasonEpisodesPage {
            series_id: "show-1".to_owned(),
            season_id: Some("season-1".to_owned()),
            season_number: Some(1),
            start_index: start,
            limit: SEASON_EPISODE_PAGE_SIZE,
            total_record_count: total,
            next_start_index: end,
            has_more: end < total,
            episodes: (start..end)
                .map(|index| episode(&format!("episode-{index}"), 1))
                .collect(),
        }
    }

    #[test]
    fn paging_reaches_episodes_beyond_thirty_without_counting_filtered_cards() {
        let mut loaded = episode_page(0, 30, 35);
        loaded.episodes.retain(|episode| episode.id != "episode-15");
        let request = next_season_page_request(&loaded).expect("next page available");
        assert_eq!(request.start_index, 30);
        assert_eq!(request.season_id.as_deref(), Some("season-1"));

        assert!(append_season_page(&mut loaded, episode_page(30, 35, 35)));
        assert_eq!(loaded.episodes.len(), 34);
        assert_eq!(
            loaded.episodes.last().expect("last episode").id,
            "episode-34"
        );
        assert!(!loaded.episodes.iter().any(|item| item.id == "episode-15"));
        assert!(next_season_page_request(&loaded).is_none());
    }

    #[test]
    fn appending_rejects_another_season_or_cursor_without_losing_loaded_content() {
        let mut loaded = episode_page(0, 30, 40);
        let mut wrong_season = episode_page(30, 40, 40);
        wrong_season.season_id = Some("season-2".to_owned());
        assert!(!append_season_page(&mut loaded, wrong_season));
        assert!(!append_season_page(&mut loaded, episode_page(0, 30, 40)));
        assert_eq!(loaded.episodes.len(), 30);
        assert_eq!(
            next_season_page_request(&loaded)
                .expect("retry cursor")
                .start_index,
            30
        );
    }

    #[test]
    fn overlapping_server_pages_do_not_duplicate_loaded_episodes() {
        let mut loaded = episode_page(0, 30, 35);
        let mut next = episode_page(30, 35, 35);
        next.episodes.insert(0, episode("episode-29", 1));
        assert!(append_season_page(&mut loaded, next));
        assert_eq!(loaded.episodes.len(), 35);
        assert!(next_season_page_request(&loaded).is_none());
    }

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
        let mut detail: LoadState<_> =
            LoadState::Ready(DetailContent::Show(Box::new(VideoShowDetail {
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
                original_language: None,
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
            community_rating: None,
            episode_count: None,
            last_played_date: None,
            premiere_date: None,
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
            original_language: None,
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
    fn season_for_number_matches_only_a_listed_season_number() {
        let show = show_detail(None);

        assert_eq!(
            season_for_number(&show, 2).map(|season| season.id.as_str()),
            Some("season-2")
        );
        assert!(season_for_number(&show, 9).is_none());

        let mut unnumbered = show_detail(None);
        unnumbered.seasons[0].season_number = None;
        assert!(season_for_number(&unnumbered, 1).is_none());
    }

    #[test]
    fn selected_season_request_resolves_only_a_season_of_the_loaded_show() {
        let detail: LoadState<_> =
            LoadState::Ready(DetailContent::Show(Box::new(show_detail(None))));

        let request = selected_season_request(&detail, Some("season-2"))
            .expect("selected season should produce a page");
        assert_eq!(request.series_id, "show-1");
        assert_eq!(request.season_id.as_deref(), Some("season-2"));
        assert_eq!(request.season_number, Some(2));
        assert_eq!(request.start_index, 0);
        assert_eq!(request.limit, SEASON_EPISODE_PAGE_SIZE);

        assert!(selected_season_request(&detail, Some("missing-season")).is_none());
        assert!(selected_season_request(&detail, None).is_none());
        assert!(
            selected_season_request(&LoadState::<DetailContent>::Loading, Some("season-2"))
                .is_none()
        );
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
        assert!(detail_user_data(&LoadState::<DetailContent>::Loading).is_none());

        let detail: LoadState<_> =
            LoadState::Ready(DetailContent::Show(Box::new(show_detail(None))));
        let (item_id, played, favorite) =
            detail_user_data(&detail).expect("ready content exposes user data");
        assert_eq!(item_id, "show-1");
        assert!(!played);
        assert!(favorite);
    }
}
