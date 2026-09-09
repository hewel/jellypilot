//! Ordered, identity-stable candidates for the Video Home Hero.

use std::collections::HashSet;

use jellypilot_media_server::VideoLibraryItem;

use crate::cards::is_episode_item;
use crate::LoadState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeroSource {
    ContinueWatching,
    NextUp,
    Latest(usize),
}

/// Coordinates in the original source, before resume filtering or grouping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeroCandidate {
    pub source: HeroSource,
    pub item_index: usize,
}

/// Preserves server order, preferring usable resumes over Next Up and keeping
/// one candidate per media item and known series. Latest rows participate only
/// when both continuation requests have successfully returned empty lists.
#[must_use]
pub fn candidates<'a, E: 'a>(
    continue_watching: &'a LoadState<Vec<VideoLibraryItem>, E>,
    next_up: &'a LoadState<Vec<VideoLibraryItem>, E>,
    latest: impl IntoIterator<Item = &'a LoadState<Vec<VideoLibraryItem>, E>>,
) -> Vec<HeroCandidate> {
    let mut result = Vec::new();
    let mut item_ids = HashSet::new();
    let mut series_ids = HashSet::new();
    let mut append = |source, state: &'a LoadState<Vec<VideoLibraryItem>, E>| {
        let LoadState::Ready(items) = state else {
            return;
        };
        for (item_index, item) in items.iter().enumerate() {
            if source == HeroSource::ContinueWatching && !has_resume_position(item) {
                continue;
            }
            let item_id = nonblank(&item.id);
            let series_id = series_identity(item);
            if item_id.is_some_and(|id| item_ids.contains(id))
                || series_id.is_some_and(|id| series_ids.contains(id))
            {
                continue;
            }
            if let Some(id) = item_id {
                item_ids.insert(id);
            }
            if let Some(id) = series_id {
                series_ids.insert(id);
            }
            result.push(HeroCandidate { source, item_index });
        }
    };

    if matches!(continue_watching, LoadState::Ready(items) if items.is_empty())
        && matches!(next_up, LoadState::Ready(items) if items.is_empty())
    {
        for (index, state) in latest.into_iter().enumerate() {
            append(HeroSource::Latest(index), state);
        }
    } else {
        append(HeroSource::ContinueWatching, continue_watching);
        append(HeroSource::NextUp, next_up);
    }
    result
}

fn nonblank(id: &str) -> Option<&str> {
    (!id.trim().is_empty()).then_some(id)
}

fn series_identity(item: &VideoLibraryItem) -> Option<&str> {
    if is_episode_item(item) {
        item.series_id.as_deref().and_then(nonblank)
    } else if item.item_type.eq_ignore_ascii_case("Series") {
        nonblank(&item.id)
    } else {
        None
    }
}

/// Returns the selected eligible identity, or the first identity when selection
/// is absent or removed. The returned identity is borrowed from the new items.
#[must_use]
pub fn retained_selection<'a>(
    selected: Option<&str>,
    items: impl IntoIterator<Item = &'a VideoLibraryItem>,
) -> Option<&'a str> {
    let mut items = items.into_iter();
    let first = items.next()?.id.as_str();
    let Some(selected) = selected else {
        return Some(first);
    };
    if selected == first {
        return Some(first);
    }
    Some(
        items
            .find(|item| item.id == selected)
            .map_or(first, |item| item.id.as_str()),
    )
}

/// Whether an unplayed item has a finite, positive position before its known
/// valid runtime. Missing or invalid duration does not invalidate the position.
#[must_use]
pub fn has_resume_position(item: &VideoLibraryItem) -> bool {
    !item.played
        && item.resume_position_seconds.is_some_and(|position| {
            position.is_finite()
                && position > 0.0
                && item.runtime_seconds.is_none_or(|runtime| {
                    !runtime.is_finite() || runtime <= 0.0 || position < runtime
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, item_type: &str, series_id: Option<&str>) -> VideoLibraryItem {
        VideoLibraryItem {
            community_rating: None,
            episode_count: None,
            last_played_date: None,
            premiere_date: None,
            id: id.to_owned(),
            name: "Pilot".to_owned(),
            item_type: item_type.to_owned(),
            production_year: None,
            runtime_seconds: Some(2700.0),
            played: false,
            favorite: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            logo_image_id: None,
            series_poster_image_id: None,
            episode_thumb_image_id: None,
            series_thumb_image_id: None,
            series_backdrop_image_id: None,
            season_poster_image_id: None,
            season_number: Some(1),
            episode_number: Some(1),
            index_number_end: None,
            series_id: series_id.map(str::to_owned),
            series_name: Some("Shared display title".to_owned()),
            end_year: None,
            series_continuing: false,
            unplayed_item_count: None,
            resume_position_seconds: None,
            played_percentage: None,
            overview: None,
        }
    }

    fn resume(id: &str, series_id: Option<&str>) -> VideoLibraryItem {
        let mut item = item(id, "Episode", series_id);
        item.resume_position_seconds = Some(600.0);
        item
    }

    fn candidate(source: HeroSource, item_index: usize) -> HeroCandidate {
        HeroCandidate { source, item_index }
    }

    #[test]
    fn resumes_win_series_and_item_duplicates_without_reordering_sources() {
        let watching: LoadState<_> = LoadState::Ready(vec![
            item("not-started", "Episode", Some("series-a")),
            resume("a1", Some("series-a")),
            resume("a2", Some("series-a")),
            resume("b1", Some("series-b")),
        ]);
        let next = LoadState::Ready(vec![
            item("a3", "Episode", Some("series-a")),
            item("b1", "Episode", None),
            item("c1", "Episode", Some("series-c")),
            item("c2", "Episode", Some("series-c")),
            item("d1", "Episode", Some("series-d")),
        ]);
        assert_eq!(
            candidates(&watching, &next, []),
            vec![
                candidate(HeroSource::ContinueWatching, 1),
                candidate(HeroSource::ContinueWatching, 3),
                candidate(HeroSource::NextUp, 2),
                candidate(HeroSource::NextUp, 4),
            ]
        );
    }

    #[test]
    fn missing_series_identities_and_movie_metadata_do_not_merge_unrelated_items() {
        let next: LoadState<_> = LoadState::Ready(vec![
            item("a", "Episode", None),
            item("b", "Episode", None),
            item("c", "Episode", Some("")),
            item("d", "Episode", Some(" \t")),
            item("movie-a", "Movie", Some("stray-series")),
            item("movie-b", "Movie", Some("stray-series")),
            item("e", "Episode", Some("stray-series")),
        ]);
        assert_eq!(
            candidates(&LoadState::Ready(vec![]), &next, []),
            (0..7)
                .map(|index| candidate(HeroSource::NextUp, index))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn latest_groups_across_rows_and_preserves_original_row_positions() {
        let empty: LoadState<_> = LoadState::Ready(vec![]);
        let latest = [
            LoadState::Loading,
            LoadState::Ready(vec![
                item("series-a", "Series", None),
                item("movie", "Movie", None),
            ]),
            LoadState::Ready(vec![
                item("a1", "Episode", Some("series-a")),
                item("movie", "Movie", None),
                item("b1", "Episode", Some("series-b")),
                item("b2", "Episode", Some("series-b")),
            ]),
        ];
        assert_eq!(
            candidates(&empty, &empty, &latest),
            vec![
                candidate(HeroSource::Latest(1), 0),
                candidate(HeroSource::Latest(1), 1),
                candidate(HeroSource::Latest(2), 2),
            ]
        );
    }

    #[test]
    fn unresolved_or_failed_continuation_is_not_latest_fallback() {
        let empty = LoadState::Ready(vec![]);
        let latest = LoadState::Ready(vec![item("movie", "Movie", None)]);
        for unsettled in [
            LoadState::Idle,
            LoadState::Loading,
            LoadState::Failed("server unavailable".to_owned()),
        ] {
            assert!(candidates(&unsettled, &empty, [&latest]).is_empty());
            assert!(candidates(&empty, &unsettled, [&latest]).is_empty());
        }
    }

    #[test]
    fn unusable_resume_is_not_a_successfully_empty_response() {
        let watching: LoadState<_> = LoadState::Ready(vec![item("a", "Episode", Some("series-a"))]);
        let latest = LoadState::Ready(vec![item("movie", "Movie", None)]);
        assert!(candidates(&watching, &LoadState::Ready(vec![]), [&latest]).is_empty());
    }

    #[test]
    fn selection_survives_reordering_and_falls_back_when_removed() {
        let a = item("a", "Movie", None);
        let b = item("b", "Movie", None);
        let c = item("c", "Movie", None);
        let selected = retained_selection(Some("b"), [&a, &b]);
        let reordered = retained_selection(selected, [&c, &b, &a]);
        assert_eq!(reordered, Some("b"));
        let removed = retained_selection(reordered, [&c, &a]);
        assert_eq!(removed, Some("c"));
        assert_eq!(retained_selection(removed, []), None);
    }

    #[test]
    fn resume_requires_unplayed_finite_progress_before_valid_runtime() {
        let mut item = resume("a", None);
        assert!(has_resume_position(&item));
        item.resume_position_seconds = Some(2700.0);
        assert!(!has_resume_position(&item));
        item.runtime_seconds = None;
        assert!(has_resume_position(&item));
        item.resume_position_seconds = Some(f64::NAN);
        assert!(!has_resume_position(&item));
        item.resume_position_seconds = Some(0.0);
        assert!(!has_resume_position(&item));
        item.resume_position_seconds = Some(600.0);
        item.played = true;
        assert!(!has_resume_position(&item));
    }
}
