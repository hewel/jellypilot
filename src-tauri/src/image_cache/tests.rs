use super::*;
use std::time::Duration;

struct TempDirGuard(PathBuf);
impl TempDirGuard {
  fn new() -> Self {
    let dir = std::env::temp_dir().join(format!("image_cache_test_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    Self(dir)
  }
  fn path(&self) -> PathBuf {
    self.0.clone()
  }
}
impl Drop for TempDirGuard {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

fn partition() -> ImageCachePartition {
  ImageCache::partition(MediaServerProvider::Jellyfin, "https://media.example.com/")
}

async fn write_entry(cache: &Arc<ImageCache>, url: &str, body: &[u8]) {
  let mut writer = cache
    .try_begin_writer(
      &partition(),
      url,
      Some("image/jpeg"),
      Some(body.len() as u64),
    )
    .await
    .expect("writer should be admitted");
  assert!(writer.try_push(bytes::Bytes::copy_from_slice(body)));
  writer.finish();
  // Wait for the background commit to land.
  for _ in 0..200 {
    if cache.open_reader(&partition(), url).await.is_some() {
      return;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
  }
  panic!("entry was not committed in time");
}

#[tokio::test]
async fn first_miss_commits_and_restart_hits() {
  let dir = TempDirGuard::new();
  let url = "https://media.example.com/Items/1/Images/Primary?tag=abc";
  let body = b"jpeg-bytes-here".to_vec();

  {
    let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
      .await
      .expect("init");
    assert!(cache.open_reader(&partition(), url).await.is_none());
    write_entry(&cache, url, &body).await;

    let reader = cache.open_reader(&partition(), url).await.expect("hit");
    assert_eq!(tokio::fs::read(reader.path()).await.unwrap(), body);
    assert_eq!(reader.content_type(), Some("image/jpeg"));
    assert_eq!(reader.size_bytes(), body.len() as u64);
  }

  // A fresh catalog over the same root (simulating a restart) must hit.
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("re-init");
  let reader = cache
    .open_reader(&partition(), url)
    .await
    .expect("restart hit");
  assert_eq!(tokio::fs::read(reader.path()).await.unwrap(), body);
}

#[tokio::test]
async fn cache_identity_is_scoped_per_server() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://a.example.com/Items/1/Images/Primary";

  write_entry(&cache, url, b"from-server-a").await;

  // Same relative URL under a different server must not collide.
  let other = ImageCache::partition(MediaServerProvider::Jellyfin, "https://b.example.com");
  assert!(cache.open_reader(&other, url).await.is_none());
}

#[tokio::test]
async fn abandoned_writer_leaves_no_entry_or_temp_file() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/2/Images/Primary";

  let mut writer = cache
    .try_begin_writer(&partition(), url, Some("image/jpeg"), None)
    .await
    .expect("writer");
  assert!(writer.try_push(bytes::Bytes::from_static(b"partial")));
  drop(writer); // abandonment: no finish()

  // Give the background task a chance to clean up.
  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(cache.open_reader(&partition(), url).await.is_none());

  let mut temps = 0usize;
  let mut rd = tokio::fs::read_dir(dir.path().join("images"))
    .await
    .unwrap();
  while let Some(entry) = rd.next_entry().await.unwrap() {
    if entry.file_name().to_string_lossy().starts_with(".tmp-") {
      temps += 1;
    }
  }
  assert_eq!(temps, 0, "no temp files should survive abandonment");
}

#[tokio::test]
async fn oversized_origin_is_not_admitted() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/3/Images/Primary";

  let declared = IMAGE_CACHE_MAX_ENTRY_BYTES + 1;
  assert!(
    cache
      .try_begin_writer(&partition(), url, Some("image/jpeg"), Some(declared))
      .await
      .is_none(),
    "declared-oversized origins must not be admitted"
  );
}

#[tokio::test]
async fn global_budget_evicts_lru_unlocked_entries() {
  let dir = TempDirGuard::new();
  // Budget fits two 100-byte entries but not three.
  let cache = ImageCache::init(dir.path(), 200).await.expect("init");
  let body = vec![0u8; 100];

  write_entry(&cache, "https://media.example.com/a", &body).await;
  tokio::time::sleep(Duration::from_millis(20)).await;
  write_entry(&cache, "https://media.example.com/b", &body).await;
  tokio::time::sleep(Duration::from_millis(20)).await;
  // Touch `a` so `b` is the LRU victim.
  assert!(cache
    .open_reader(&partition(), "https://media.example.com/a")
    .await
    .is_some());
  tokio::time::sleep(Duration::from_millis(20)).await;
  write_entry(&cache, "https://media.example.com/c", &body).await;

  assert!(cache
    .open_reader(&partition(), "https://media.example.com/a")
    .await
    .is_some());
  assert!(cache
    .open_reader(&partition(), "https://media.example.com/c")
    .await
    .is_some());
  assert!(
    cache
      .open_reader(&partition(), "https://media.example.com/b")
      .await
      .is_none(),
    "LRU entry should have been evicted to make room"
  );
}

#[tokio::test]
async fn active_reader_is_not_evicted() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), 200).await.expect("init");
  let body = vec![0u8; 100];

  write_entry(&cache, "https://media.example.com/keep", &body).await;
  let _reader = cache
    .open_reader(&partition(), "https://media.example.com/keep")
    .await
    .expect("hit");

  // Two more entries overflow the budget, but the held reader must survive.
  write_entry(&cache, "https://media.example.com/x", &body).await;
  write_entry(&cache, "https://media.example.com/y", &body).await;

  assert!(
    cache
      .open_reader(&partition(), "https://media.example.com/keep")
      .await
      .is_some(),
    "an actively-read entry must not be evicted"
  );
}

#[tokio::test]
async fn entry_larger_than_budget_is_not_cached() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), 50).await.expect("init");
  let url = "https://media.example.com/big";

  let mut writer = cache
    .try_begin_writer(&partition(), url, Some("image/jpeg"), Some(100))
    .await
    .expect("admitted (under per-entry cap)");
  assert!(writer.try_push(bytes::Bytes::from(vec![0u8; 100])));
  writer.finish();

  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(
    cache.open_reader(&partition(), url).await.is_none(),
    "entry exceeding the global budget must not be committed"
  );
}

#[tokio::test]
async fn corrupt_catalog_is_quarantined_and_rebuilt() {
  let dir = TempDirGuard::new();
  let images = dir.path().join("images");
  tokio::fs::create_dir_all(&images).await.unwrap();
  tokio::fs::write(
    images.join("catalog.sqlite3"),
    b"this is not a sqlite database",
  )
  .await
  .unwrap();

  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("corrupt catalog must be rebuilt, not fatal");
  write_entry(&cache, "https://media.example.com/after-corruption", b"ok").await;
  assert!(cache
    .open_reader(&partition(), "https://media.example.com/after-corruption")
    .await
    .is_some());

  // A quarantined copy of the corrupt file should exist.
  let mut quarantined = false;
  let mut rd = tokio::fs::read_dir(&images).await.unwrap();
  while let Some(entry) = rd.next_entry().await.unwrap() {
    if entry.file_name().to_string_lossy().starts_with(".corrupt-") {
      quarantined = true;
    }
  }
  assert!(quarantined, "corrupt catalog should be quarantined aside");
}

#[tokio::test]
async fn dangling_row_is_reconciled_to_miss() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/dangling";
  write_entry(&cache, url, b"bytes").await;

  // Delete the file out from under the catalog.
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  let path = reader.path().to_path_buf();
  drop(reader);
  tokio::fs::remove_file(&path).await.unwrap();

  assert!(
    cache.open_reader(&partition(), url).await.is_none(),
    "missing file must reconcile to a miss"
  );
}

#[tokio::test]
async fn reject_avif_restores_origin_and_marks_failed() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/reject/Images/Primary";
  let body = b"original-jpeg-bytes".to_vec();
  write_entry(&cache, url, &body).await;

  let cache_key = ImageCache::cache_key(&partition(), url);

  // Simulate a successful AVIF activation.
  let avif_name = ImageCache::avif_file_name_for(
    cache
      .open_reader(&partition(), url)
      .await
      .unwrap()
      .path()
      .file_name()
      .unwrap()
      .to_str()
      .unwrap(),
  );
  let avif_path = cache.path_for(&avif_name);
  tokio::fs::write(&avif_path, b"fake-avif-bytes")
    .await
    .unwrap();
  cache
    .activate_avif(&cache_key, &avif_name, 15, "deadbeef", "image/avif")
    .await
    .unwrap();

  // Confirm AVIF is now active.
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  assert_eq!(reader.content_type(), Some("image/avif"));
  drop(reader);

  // Reject: must restore origin and mark failed.
  cache.reject_avif(&cache_key).await.unwrap();

  let reader = cache.open_reader(&partition(), url).await.unwrap();
  assert_eq!(
    reader.content_type(),
    Some("image/jpeg"),
    "reject must restore origin content type"
  );
  assert_eq!(
    tokio::fs::read(reader.path()).await.unwrap(),
    body,
    "reject must restore original bytes"
  );
  drop(reader);

  // AVIF file must be deleted.
  assert!(
    tokio::fs::metadata(&avif_path).await.is_err(),
    "rejected AVIF file must be removed"
  );

  // conv_state must be 'failed'.
  let state: Option<(String,)> =
    sqlx::query_as("SELECT conv_state FROM entries WHERE cache_key = ?")
      .bind(&cache_key)
      .fetch_optional(&cache.pool)
      .await
      .unwrap();
  assert_eq!(
    state.unwrap().0,
    "failed",
    "reject must mark conversion as failed"
  );
}

#[tokio::test]
async fn reject_avif_noop_when_origin_active() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/noop/Images/Primary";
  write_entry(&cache, url, b"jpeg").await;

  let cache_key = ImageCache::cache_key(&partition(), url);
  // Rejecting when origin is already active must be a no-op.
  cache.reject_avif(&cache_key).await.unwrap();

  let reader = cache.open_reader(&partition(), url).await.unwrap();
  assert_eq!(reader.content_type(), Some("image/jpeg"));
}
