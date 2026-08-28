use std::sync::Arc;

use crate::{JellyfinClient, VideoHome, VideoLibraryShortcut};

pub type HomeDataResult = (
  Result<VideoHome, String>,
  Result<Vec<VideoLibraryShortcut>, String>,
);

pub async fn load_home_data(client: Arc<JellyfinClient>) -> HomeDataResult {
  tokio::join!(
    async {
      client
        .library()
        .video_home()
        .await
        .map_err(|error| error.to_string())
    },
    async {
      client
        .library()
        .library_shortcuts()
        .await
        .map_err(|error| error.to_string())
    },
  )
}
