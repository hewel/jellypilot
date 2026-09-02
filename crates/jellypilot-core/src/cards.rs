use jellypilot_media_server::{VideoLibraryItem, VideoLibraryKind, VideoLibraryShortcut};

#[must_use]
pub fn item_caption(item: &VideoLibraryItem) -> String {
    match item.production_year {
        Some(year) => format!("{year} · {}", item.item_type),
        None => item.item_type.clone(),
    }
}

#[must_use]
pub fn hero_headline(item: &VideoLibraryItem) -> String {
    if is_episode_item(item) {
        item.series_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| item.name.clone())
    } else {
        item.name.clone()
    }
}

#[must_use]
pub fn card_title(item: &VideoLibraryItem) -> &str {
    if is_episode_item(item) {
        item.series_name.as_deref().unwrap_or(&item.name)
    } else {
        &item.name
    }
}

#[must_use]
pub fn series_year_range(
    production_year: Option<i32>,
    end_year: Option<i32>,
    continuing: bool,
) -> String {
    let Some(production_year) = production_year else {
        return String::new();
    };

    if continuing {
        format!("{production_year} - Present")
    } else {
        match end_year {
            Some(end_year) if end_year != production_year => {
                format!("{production_year} - {end_year}")
            }
            _ => production_year.to_string(),
        }
    }
}

#[must_use]
pub fn card_subtitle(item: &VideoLibraryItem) -> String {
    if item.item_type.eq_ignore_ascii_case("Series") {
        series_year_range(item.production_year, item.end_year, item.series_continuing)
    } else if is_episode_item(item) {
        match (item.season_number, item.episode_number) {
            (Some(0), _) => format!("SP - {}", item.name),
            (Some(season), Some(episode)) => match item.index_number_end {
                Some(end) => format!("S{season}:E{episode}-{end} - {}", item.name),
                None => format!("S{season}:E{episode} - {}", item.name),
            },
            _ => item.name.clone(),
        }
    } else {
        item.production_year
            .map_or_else(String::new, |year| year.to_string())
    }
}

#[must_use]
pub fn runtime_caption(runtime_seconds: f64) -> Option<String> {
    if !runtime_seconds.is_finite() || runtime_seconds <= 0.0 {
        return None;
    }
    let total_minutes = (runtime_seconds / 60.0).round() as u64;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    Some(match (hours, minutes) {
        (0, minutes) => format!("{minutes}m"),
        (hours, 0) => format!("{hours}h"),
        (hours, minutes) => format!("{hours}h {minutes}m"),
    })
}

#[must_use]
pub fn hero_metadata(item: &VideoLibraryItem) -> String {
    let mut parts = Vec::with_capacity(3);
    if let Some(year) = item.production_year {
        parts.push(year.to_string());
    }
    if let Some(runtime) = item.runtime_seconds.and_then(runtime_caption) {
        parts.push(runtime);
    }
    if is_episode_item(item) {
        if let (Some(season), Some(episode)) = (item.season_number, item.episode_number) {
            parts.push(format!("S{season} E{episode}"));
        }
    }
    if parts.is_empty() {
        item_caption(item)
    } else {
        parts.join(" · ")
    }
}

#[must_use]
pub fn library_kind(collection_type: &str) -> VideoLibraryKind {
    if collection_type.eq_ignore_ascii_case("tvshows") || collection_type.eq_ignore_ascii_case("tv")
    {
        VideoLibraryKind::TvShows
    } else {
        VideoLibraryKind::Movies
    }
}

#[must_use]
pub fn library_shortcut_caption(shortcut: &VideoLibraryShortcut) -> String {
    let kind = match library_kind(&shortcut.collection_type) {
        VideoLibraryKind::TvShows => "TV Shows",
        VideoLibraryKind::Movies => "Movies",
    };
    match shortcut.item_count {
        Some(count) => format!("{kind} · {count}"),
        None => kind.to_owned(),
    }
}

#[must_use]
pub fn is_episode_item(item: &VideoLibraryItem) -> bool {
    item.item_type.eq_ignore_ascii_case("Episode")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_item(item_type: &str) -> VideoLibraryItem {
        VideoLibraryItem {
            id: "item-1".to_owned(),
            name: "Item".to_owned(),
            item_type: item_type.to_owned(),
            production_year: None,
            runtime_seconds: None,
            played: false,
            favorite: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            series_poster_image_id: None,
            season_number: None,
            episode_thumb_image_id: None,
            series_thumb_image_id: None,
            series_backdrop_image_id: None,
            episode_number: None,
            series_id: None,
            series_name: None,
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

    #[test]
    fn card_helpers_format_episode_with_full_data() {
        let mut item = video_item("Episode");
        item.name = "Pilot".to_owned();
        item.series_name = Some("Series".to_owned());
        item.season_number = Some(1);
        item.episode_number = Some(2);

        assert_eq!(
            (card_title(&item), card_subtitle(&item)),
            ("Series", "S1:E2 - Pilot".to_owned())
        );
    }

    #[test]
    fn card_title_falls_back_to_episode_name_when_series_name_is_missing() {
        let mut item = video_item("Episode");
        item.name = "Pilot".to_owned();

        assert_eq!(card_title(&item), "Pilot");
    }

    #[test]
    fn card_subtitle_omits_number_prefix_when_any_episode_index_is_missing() {
        for (season_number, episode_number) in [(None, Some(2)), (Some(1), None), (None, None)] {
            let mut item = video_item("Episode");
            item.name = "Pilot".to_owned();
            item.season_number = season_number;
            item.episode_number = episode_number;

            assert_eq!(card_subtitle(&item), "Pilot");
        }
    }

    #[test]
    fn card_subtitle_formats_special_episode() {
        let mut item = video_item("Episode");
        item.name = "Name".to_owned();
        item.season_number = Some(0);
        item.episode_number = Some(1);

        assert_eq!(card_subtitle(&item), "SP - Name");
    }

    #[test]
    fn card_subtitle_formats_multi_episode_range() {
        let mut item = video_item("Episode");
        item.name = "Name".to_owned();
        item.season_number = Some(6);
        item.episode_number = Some(1);
        item.index_number_end = Some(2);

        assert_eq!(card_subtitle(&item), "S6:E1-2 - Name");
    }

    #[test]
    fn card_subtitle_formats_continuing_series_year_range() {
        let mut item = video_item("Series");
        item.production_year = Some(2023);
        item.series_continuing = true;

        assert_eq!(card_subtitle(&item), "2023 - Present");
    }

    #[test]
    fn card_subtitle_formats_ended_series_year_range() {
        let mut item = video_item("Series");
        item.production_year = Some(2011);
        item.end_year = Some(2017);

        assert_eq!(card_subtitle(&item), "2011 - 2017");
    }

    #[test]
    fn card_subtitle_collapses_same_year_series_range() {
        let mut item = video_item("Series");
        item.production_year = Some(2021);
        item.end_year = Some(2021);

        assert_eq!(card_subtitle(&item), "2021");
    }

    #[test]
    fn card_subtitle_omits_series_year_range_without_production_year() {
        let mut item = video_item("Series");
        item.end_year = Some(2021);
        item.series_continuing = true;

        assert_eq!(card_subtitle(&item), "");
    }

    #[test]
    fn card_helpers_format_movie_with_year() {
        let mut item = video_item("Movie");
        item.name = "Movie".to_owned();
        item.production_year = Some(2024);

        assert_eq!(
            (card_title(&item), card_subtitle(&item)),
            ("Movie", "2024".to_owned())
        );
    }

    #[test]
    fn card_helpers_format_movie_without_year() {
        let mut item = video_item("Movie");
        item.name = "Movie".to_owned();

        assert_eq!(
            (card_title(&item), card_subtitle(&item)),
            ("Movie", String::new())
        );
    }

    #[test]
    fn runtime_caption_formats_hours_and_minutes() {
        assert_eq!(runtime_caption(5_400.0).as_deref(), Some("1h 30m"));
    }

    #[test]
    fn hero_metadata_combines_available_episode_facts() {
        let item = VideoLibraryItem {
            id: "episode-1".to_owned(),
            name: "Pilot".to_owned(),
            item_type: "Episode".to_owned(),
            production_year: Some(2024),
            runtime_seconds: Some(2_700.0),
            played: false,
            favorite: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            series_poster_image_id: None,
            season_number: Some(1),
            episode_thumb_image_id: None,
            series_thumb_image_id: None,
            series_backdrop_image_id: None,
            episode_number: Some(2),
            series_id: None,
            series_name: Some("Series".to_owned()),
            resume_position_seconds: None,
            played_percentage: None,
            overview: None,
            index_number_end: None,
            season_poster_image_id: None,
            end_year: None,
            series_continuing: false,
            unplayed_item_count: None,
        };

        assert_eq!(hero_metadata(&item), "2024 · 45m · S1 E2");
    }
}
