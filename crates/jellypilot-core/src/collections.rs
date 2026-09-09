//! Movie/series targets shared by featured-media and Now Playing collection actions.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollectionTarget<'a> {
    pub item_id: &'a str,
    pub is_series: bool,
}

/// Collection controls act on the movie or whole series, never on a fallback episode.
#[must_use]
pub fn collection_target<'a>(
    item_id: &'a str,
    item_type: &str,
    series_id: Option<&'a str>,
) -> Option<CollectionTarget<'a>> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return None;
    }
    if item_type.eq_ignore_ascii_case("episode") {
        let parent = series_id?.trim();
        return (!parent.is_empty() && parent != item_id).then_some(CollectionTarget {
            item_id: parent,
            is_series: true,
        });
    }
    let is_series = item_type.eq_ignore_ascii_case("series");
    (is_series || item_type.eq_ignore_ascii_case("movie"))
        .then_some(CollectionTarget { item_id, is_series })
}

#[cfg(test)]
mod tests {
    use super::collection_target;

    #[test]
    fn missing_parent_does_not_turn_series_favorite_into_episode_favorite() {
        assert!(collection_target("episode", "Episode", None).is_none());
    }

    #[test]
    fn self_parent_does_not_authorize_an_episode_collection_action() {
        assert!(collection_target("episode", "Episode", Some("episode")).is_none());
    }
}
