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
            episode_number: Some(2),
            series_id: None,
            series_name: Some("Series".to_owned()),
            resume_position_seconds: None,
            played_percentage: None,
            overview: None,
        };

        assert_eq!(hero_metadata(&item), "2024 · 45m · S1 E2");
    }
}
