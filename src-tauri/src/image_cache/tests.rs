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
async fn version_three_catalog_is_reset_to_origin_only_schema() {
  let dir = TempDirGuard::new();
  let images = dir.path().join("images");
  tokio::fs::create_dir_all(&images).await.unwrap();
  let db_path = images.join(CATALOG_FILE);
  let options = SqliteConnectOptions::new()
    .filename(&db_path)
    .create_if_missing(true);
  let pool = SqlitePoolOptions::new()
    .max_connections(1)
    .connect_with(options)
    .await
    .unwrap();
  sqlx::query(
    r#"
    CREATE TABLE entries (
      cache_key TEXT PRIMARY KEY,
      scope TEXT NOT NULL,
      file_name TEXT NOT NULL,
      size_bytes INTEGER NOT NULL,
      content_type TEXT,
      content_digest TEXT,
      accessed_at INTEGER NOT NULL,
      created_at INTEGER NOT NULL,
      active_kind TEXT NOT NULL,
      original_file_name TEXT,
      avif_file_name TEXT
    )
    "#,
  )
  .execute(&pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO entries VALUES \
     ('origin', 'scope', 'origin-active.jpg', 6, 'image/jpeg', 'a', 1, 1, 'origin', 'origin-active.jpg', NULL), \
     ('converted', 'scope', 'converted.avif', 4, 'image/avif', 'b', 1, 1, 'avif', 'original-for-avif.jpg', 'converted.avif')",
  )
  .execute(&pool)
  .await
  .unwrap();
  sqlx::query("PRAGMA user_version = 3")
    .execute(&pool)
    .await
    .unwrap();
  pool.close().await;

  let legacy_files = [
    "origin-active.jpg",
    "original-for-avif.jpg",
    "converted.avif",
    "orphan.avif",
    ".tmp-old",
    ".worker.lock",
  ];
  for name in legacy_files {
    tokio::fs::write(images.join(name), b"legacy")
      .await
      .unwrap();
  }

  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("migrate v3 catalog");
  let version: i64 = sqlx::query_scalar("PRAGMA user_version")
    .fetch_one(&cache.pool)
    .await
    .unwrap();
  assert_eq!(version, CATALOG_SCHEMA_VERSION);
  let columns: Vec<String> =
    sqlx::query_scalar("SELECT name FROM pragma_table_info('entries') ORDER BY cid")
      .fetch_all(&cache.pool)
      .await
      .unwrap();
  assert_eq!(
    columns,
    [
      "cache_key",
      "scope",
      "file_name",
      "size_bytes",
      "content_type",
      "content_digest",
      "accessed_at",
      "created_at",
    ]
  );
  let status = cache.status(true).await.unwrap();
  assert_eq!(status.committed_bytes, 0);
  assert_eq!(status.entry_count, 0);
  for name in legacy_files {
    assert!(
      tokio::fs::metadata(images.join(name)).await.is_err(),
      "legacy cache file {name} must be removed"
    );
  }

  let url = "https://media.example.com/Items/after-v4/Images/Primary";
  write_entry(&cache, url, b"fresh-origin").await;
  drop(cache);
  let restarted = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("restart v4 cache");
  let reader = restarted
    .open_reader(&partition(), url)
    .await
    .expect("new origin must survive restart");
  assert_eq!(
    tokio::fs::read(reader.path()).await.unwrap(),
    b"fresh-origin"
  );
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
async fn epoch_guard_blocks_stale_writer_commit() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/epoch-w/Images/Primary";

  // Begin a writer (captures epoch), then bump the epoch before finishing.
  let mut writer = cache
    .try_begin_writer(&partition(), url, Some("image/jpeg"), Some(4))
    .await
    .expect("writer");
  assert!(writer.try_push(bytes::Bytes::from_static(b"jpeg")));

  cache.bump_epoch();
  writer.finish();

  // Wait for the background commit attempt to settle.
  tokio::time::sleep(Duration::from_millis(200)).await;

  assert!(
    cache.open_reader(&partition(), url).await.is_none(),
    "a writer started before the epoch change must not republish"
  );
}

#[tokio::test]
async fn status_reports_committed_bytes_and_entry_count() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");

  write_entry(
    &cache,
    "https://media.example.com/Items/a/Images/Primary",
    &[0u8; 500],
  )
  .await;
  write_entry(
    &cache,
    "https://media.example.com/Items/b/Images/Primary",
    &[0u8; 100],
  )
  .await;

  let status = cache.status(true).await.expect("status");
  assert_eq!(status.committed_bytes, 600);
  assert_eq!(status.entry_count, 2);
  assert!(status.enabled);
  assert!(!status.clearing);
}

#[tokio::test]
async fn clear_removes_unpinned_and_defers_pinned() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");

  let url_unpinned = "https://media.example.com/Items/unpinned/Images/Primary";
  write_entry(&cache, url_unpinned, &[0u8; 400]).await;
  let unpinned_path = cache
    .open_reader(&partition(), url_unpinned)
    .await
    .unwrap()
    .path()
    .to_path_buf();

  let url_pinned = "https://media.example.com/Items/pinned/Images/Primary";
  write_entry(&cache, url_pinned, &[0u8; 300]).await;
  // Hold a reader on the pinned entry so Clear must defer it.
  let pinned_reader = cache.open_reader(&partition(), url_pinned).await.unwrap();
  let pinned_path = pinned_reader.path().to_path_buf();

  cache.clear().await.expect("clear");

  // Unpinned entry is fully removed (row and file).
  assert!(
    cache
      .open_reader(&partition(), url_unpinned)
      .await
      .is_none(),
    "unpinned entry must be removed"
  );
  assert!(tokio::fs::metadata(&unpinned_path).await.is_err());

  // Pinned entry is deferred: row and file remain and the read is not broken.
  assert!(tokio::fs::metadata(&pinned_path).await.is_ok());
  assert!(
    cache.open_reader(&partition(), url_pinned).await.is_some(),
    "pinned entry must remain available"
  );

  // Metrics reach the remaining locked state (only the pinned entry counts).
  let status = cache.status(true).await.expect("status");
  assert_eq!(status.committed_bytes, 300);
  assert!(!status.clearing, "clearing must clear after completion");

  drop(pinned_reader);
}

#[tokio::test]
async fn clear_blocks_stale_writer_and_resumes_caching() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/stale/Images/Primary";

  // A writer started before Clear must not republish across the epoch.
  let mut writer = cache
    .try_begin_writer(&partition(), url, Some("image/jpeg"), Some(4))
    .await
    .expect("writer");
  assert!(writer.try_push(bytes::Bytes::from_static(b"jpeg")));
  cache.clear().await.expect("clear");
  writer.finish();
  tokio::time::sleep(Duration::from_millis(200)).await;
  assert!(
    cache.open_reader(&partition(), url).await.is_none(),
    "pre-clear writer must not republish"
  );

  // Normal caching resumes after Clear when still enabled.
  write_entry(&cache, url, b"fresh").await;
  assert!(
    cache.open_reader(&partition(), url).await.is_some(),
    "caching must resume after clear"
  );
}
