use std::sync::Arc;

use jellypilot_media_server::{
    JellyfinClient, VideoLibraryPageRequest, VideoLibraryPlayedFilter, VideoLibrarySort,
    VideoLibrarySortDirection, VideoSearchRequest,
};

use crate::browse_model::{
    BrowsePagePayload, BrowsePageRequest, BrowsePageSettlement, BrowsePreferences, BrowseSource,
};
use crate::cards::library_kind;

#[derive(Clone, Copy, Debug, Default)]
pub enum BrowsePresentation {
    #[default]
    Grid,
    List,
}

pub async fn fetch_browse_page(
    client: Arc<JellyfinClient>,
    request: BrowsePageRequest,
) -> BrowsePageSettlement {
    let BrowsePageRequest {
        source_id,
        source,
        token,
        start_index,
        limit,
        preferences,
    } = request;
    let result = async {
        let start_index = i32::try_from(start_index)
            .map_err(|_| "Library page start index is too large.".to_owned())?;
        let limit =
            i32::try_from(limit).map_err(|_| "Library page size is too large.".to_owned())?;
        match source {
            BrowseSource::Library { shortcut, .. } => {
                let collection_type = library_kind(&shortcut.collection_type);
                client
                    .library()
                    .browse_video(VideoLibraryPageRequest {
                        library_id: shortcut.id,
                        collection_type,
                        start_index,
                        limit,
                        sort: preferences.sort,
                        sort_direction: preferences.sort_direction,
                        played_filter: preferences.played_filter,
                        favorites_only: preferences.favorites_only,
                    })
                    .await
                    .map_err(|error| error.to_string())?
                    .try_into()
            }
            BrowseSource::Search { query, .. } => {
                let page = client
                    .library()
                    .search_video(VideoSearchRequest {
                        query: query.clone(),
                        start_index,
                        limit,
                    })
                    .await
                    .map_err(|error| error.to_string())?;
                if page.query != query {
                    return Err("Media server returned results for a different search.".to_owned());
                }
                BrowsePagePayload::try_from(page)
            }
        }
    }
    .await;
    BrowsePageSettlement {
        source_id,
        token,
        result,
    }
}

#[must_use]
pub fn browse_preferences(
    sort_selection: u32,
    played_selection: u32,
    favorites_only: bool,
) -> BrowsePreferences {
    let (sort, sort_direction) = match sort_selection {
        1 => (
            VideoLibrarySort::Title,
            VideoLibrarySortDirection::Descending,
        ),
        2 => (
            VideoLibrarySort::RecentlyAdded,
            VideoLibrarySortDirection::Descending,
        ),
        3 => (
            VideoLibrarySort::ReleaseDate,
            VideoLibrarySortDirection::Descending,
        ),
        _ => (
            VideoLibrarySort::Title,
            VideoLibrarySortDirection::Ascending,
        ),
    };
    let played_filter = match played_selection {
        1 => VideoLibraryPlayedFilter::Unplayed,
        2 => VideoLibraryPlayedFilter::Played,
        _ => VideoLibraryPlayedFilter::All,
    };
    BrowsePreferences {
        sort,
        sort_direction,
        played_filter,
        favorites_only,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_controls_map_to_provider_neutral_preferences() {
        let preferences = browse_preferences(2, 1, true);

        assert!(matches!(preferences.sort, VideoLibrarySort::RecentlyAdded));
        assert!(matches!(
            preferences.sort_direction,
            VideoLibrarySortDirection::Descending
        ));
        assert!(matches!(
            preferences.played_filter,
            VideoLibraryPlayedFilter::Unplayed
        ));
        assert!(preferences.favorites_only);
    }
}
