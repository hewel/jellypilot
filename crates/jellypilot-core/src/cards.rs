use jellypilot_media_server::{VideoLibraryItem, VideoLibraryKind};

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

/// Wide logos fit `ref_height` as-is (their width follows the aspect); logos
/// with a narrower aspect are boosted toward a shared visual-area target.
const WIDE_ASPECT: f32 = 1.6;
const AREA_FACTOR: f32 = 2.0;
const MAX_HEIGHT_FACTOR: f32 = 1.5;

/// Display size for a Title Logo of intrinsic `width` x `height` pixels.
/// Wide logos fit `ref_height` unchanged; narrower logos are scaled up toward
/// an area of `AREA_FACTOR` x `ref_height`^2 so small marks keep visual weight
/// next to wide ones, with height capped at `MAX_HEIGHT_FACTOR` x ref.
/// A zero dimension means "unknown" and yields `(0.0, ref_height)` so callers
/// can fall back to height-only fitting.
#[must_use]
pub fn logo_display_size(width: u32, height: u32, ref_height: f32) -> (f32, f32) {
    if width == 0 || height == 0 {
        return (0.0, ref_height);
    }
    let aspect = width as f32 / height as f32;
    if aspect >= WIDE_ASPECT {
        return (ref_height * aspect, ref_height);
    }
    let boost = (AREA_FACTOR / aspect).sqrt().min(MAX_HEIGHT_FACTOR);
    (ref_height * boost * aspect, ref_height * boost)
}

/// Stable episode code for cards; missing ordinary episode indexes yield no code.
#[must_use]
pub fn episode_card_code(item: &VideoLibraryItem) -> Option<String> {
    if !is_episode_item(item) {
        return None;
    }
    match (item.season_number, item.episode_number) {
        (Some(0), _) => Some("SP".to_owned()),
        (Some(season), Some(episode)) => Some(match item.index_number_end {
            Some(end) => format!("S{season}:E{episode}-{end}"),
            None => format!("S{season}:E{episode}"),
        }),
        _ => None,
    }
}

/// Stable compact episode identity for hero metadata.
#[must_use]
pub fn episode_hero_code(item: &VideoLibraryItem) -> Option<String> {
    if !is_episode_item(item) {
        return None;
    }
    Some(format!(
        "S{} E{}",
        item.season_number?, item.episode_number?
    ))
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
pub fn is_episode_item(item: &VideoLibraryItem) -> bool {
    item.item_type.eq_ignore_ascii_case("Episode")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_logo_fits_the_reference_height_unchanged() {
        let (width, height) = logo_display_size(600, 150, 96.0);
        assert_eq!((width, height), (384.0, 96.0));
    }

    #[test]
    fn narrow_logo_boosts_toward_the_area_target() {
        let (width, height) = logo_display_size(90, 100, 96.0);
        let boost = (2.0_f32 / 0.9).sqrt();
        assert!((height - 96.0 * boost).abs() < 0.01);
        assert!((width - 96.0 * boost * 0.9).abs() < 0.01);
    }

    #[test]
    fn extremely_tall_logo_is_capped_at_one_and_a_half_heights() {
        let (width, height) = logo_display_size(33, 100, 96.0);
        assert_eq!(height, 144.0);
        assert!((width - 144.0 * 0.33).abs() < 0.01);
    }

    #[test]
    fn zero_intrinsic_size_signals_fallback() {
        assert_eq!(logo_display_size(0, 100, 96.0), (0.0, 96.0));
    }

    fn video_item(item_type: &str) -> VideoLibraryItem {
        VideoLibraryItem {
            premiere_date: None,
            id: "item-1".to_owned(),
            name: "Item".to_owned(),
            item_type: item_type.to_owned(),
            production_year: None,
            runtime_seconds: None,
            played: false,
            favorite: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            logo_image_id: None,
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
    fn card_title_selects_the_parent_series_for_episodes() {
        let mut item = video_item("Episode");
        item.name = "Pilot".to_owned();
        item.series_name = Some("Series".to_owned());

        assert_eq!(card_title(&item), "Series");
    }

    #[test]
    fn card_title_falls_back_to_episode_name_when_series_name_is_missing() {
        let mut item = video_item("Episode");
        item.name = "Pilot".to_owned();

        assert_eq!(card_title(&item), "Pilot");
    }

    #[test]
    fn episode_codes_do_not_manufacture_missing_indexes() {
        for (season_number, episode_number) in [(None, Some(2)), (Some(1), None), (None, None)] {
            let mut item = video_item("Episode");
            item.season_number = season_number;
            item.episode_number = episode_number;

            assert_eq!(
                (episode_card_code(&item), episode_hero_code(&item)),
                (None, None)
            );
        }
    }

    #[test]
    fn special_episode_code_does_not_require_an_episode_index() {
        let mut item = video_item("Episode");
        item.season_number = Some(0);

        assert_eq!(episode_card_code(&item).as_deref(), Some("SP"));
    }

    #[test]
    fn episode_code_preserves_the_server_range() {
        let mut item = video_item("Episode");
        item.season_number = Some(6);
        item.episode_number = Some(1);
        item.index_number_end = Some(2);

        assert_eq!(episode_card_code(&item).as_deref(), Some("S6:E1-2"));
    }

    #[test]
    fn card_title_keeps_the_movie_name_even_with_series_metadata() {
        let mut item = video_item("Movie");
        item.name = "Movie".to_owned();
        item.series_name = Some("Unrelated series".to_owned());

        assert_eq!(card_title(&item), "Movie");
    }
}
