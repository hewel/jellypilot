use jellypilot_media_server::{VideoLibraryItem, VideoLibraryKind, VideoLibraryShortcut};

pub const HOME_HERO_HEIGHT: i32 = 340;
pub const POSTER_FRAME_WIDTH: i32 = 160;
pub const POSTER_FRAME_HEIGHT: i32 = 240;
pub const THUMB_FRAME_WIDTH: i32 = 240;
pub const THUMB_FRAME_HEIGHT: i32 = 135;

#[must_use]
pub fn card_frame_size(item: &VideoLibraryItem) -> (i32, i32) {
    if is_episode_item(item) {
        (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT)
    } else {
        (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT)
    }
}

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
pub fn hero_metadata(item: &VideoLibraryItem) -> String {
    if is_episode_item(item) {
        match (item.season_number, item.episode_number) {
            (Some(season), Some(number)) => format!("S{season} E{number} · {}", item.name),
            _ => format!("Episode · {}", item.name),
        }
    } else {
        item_caption(item)
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
