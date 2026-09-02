use std::sync::Arc;

use crate::{JellyfinClient, LibraryLatestRow, VideoHome, VideoLibraryShortcut};

pub type HomeDataResult = (
  Result<VideoHome, String>,
  Result<Vec<VideoLibraryShortcut>, String>,
  Result<Vec<LibraryLatestRow>, String>,
);

pub async fn load_home_data(client: Arc<JellyfinClient>) -> HomeDataResult {
  let shortcuts = client
    .library()
    .library_shortcuts()
    .await
    .map_err(|error| error.to_string());

  let (video_home, latest_rows) = tokio::join!(
    async {
      client
        .library()
        .video_home()
        .await
        .map_err(|error| error.to_string())
    },
    async {
      match &shortcuts {
        Ok(shortcuts) => load_latest_rows(Arc::clone(&client), shortcuts).await,
        Err(error) => Err(format!(
          "Library latest media requires library shortcuts: {error}"
        )),
      }
    },
  );

  (video_home, shortcuts, latest_rows)
}

async fn load_latest_rows(
  client: Arc<JellyfinClient>,
  shortcuts: &[VideoLibraryShortcut],
) -> Result<Vec<LibraryLatestRow>, String> {
  let mut requests = tokio::task::JoinSet::new();

  for (index, shortcut) in shortcuts.iter().enumerate() {
    let client = Arc::clone(&client);
    let library_id = shortcut.id.clone();
    let library_name = shortcut.name.clone();
    requests.spawn(async move {
      let result = client
        .library()
        .library_latest(library_id.clone())
        .await
        .map_err(|error| error.to_string());
      (index, library_latest_row(library_id, library_name, result))
    });
  }

  let mut ordered_rows = vec![None; shortcuts.len()];
  while let Some(result) = requests.join_next().await {
    let (index, row) =
      result.map_err(|error| format!("Library latest media task failed: {error}"))?;
    ordered_rows[index] = Some(row);
  }

  ordered_rows
    .into_iter()
    .collect::<Option<Vec<_>>>()
    .ok_or_else(|| "Library latest media returned an incomplete result set".to_string())
}

fn library_latest_row(
  library_id: String,
  library_name: String,
  result: Result<Vec<crate::VideoLibraryItem>, String>,
) -> LibraryLatestRow {
  LibraryLatestRow {
    library_id,
    library_name,
    result,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn latest_row_preserves_library_identity_when_its_request_fails() {
    let row = library_latest_row(
      "shows".to_owned(),
      "TV Shows".to_owned(),
      Err("latest request failed".to_owned()),
    );

    assert!(matches!(
      row,
      LibraryLatestRow {
        library_id,
        library_name,
        result: Err(error),
      } if library_id == "shows"
        && library_name == "TV Shows"
        && error == "latest request failed"
    ));
  }
}
