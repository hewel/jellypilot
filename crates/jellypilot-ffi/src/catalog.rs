use crate::{JellypilotSdk, VideoHome, VideoLibraryItem};

#[derive(Clone, Debug, uniffi::Record)]
pub struct CollectionTarget {
    pub item_id: String,
    pub is_series: bool,
}

#[uniffi::export]
impl JellypilotSdk {
    pub fn home_featured_items(
        &self,
        home: VideoHome,
        latest_rows: Vec<Vec<VideoLibraryItem>>,
    ) -> Vec<VideoLibraryItem> {
        self.sdk
            .home_featured_items(
                jellypilot_media_server::VideoHome {
                    continue_watching: home.continue_watching.into_iter().map(Into::into).collect(),
                    next_up: home.next_up.into_iter().map(Into::into).collect(),
                },
                latest_rows
                    .into_iter()
                    .map(|row| row.into_iter().map(Into::into).collect())
                    .collect(),
            )
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Featured and Now Playing collection actions address a whole movie/series.
    /// An episode without a distinct parent returns no target.
    pub fn collection_target(
        &self,
        item_id: String,
        item_type: String,
        series_id: Option<String>,
    ) -> Option<CollectionTarget> {
        jellypilot_core::collections::collection_target(&item_id, &item_type, series_id.as_deref())
            .map(|target| CollectionTarget {
                item_id: target.item_id.to_owned(),
                is_series: target.is_series,
            })
    }
}
