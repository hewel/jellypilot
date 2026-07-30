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
    .activate_avif(
      &cache_key,
      &avif_name,
      15,
      "deadbeef",
      "image/avif",
      cache.current_epoch(),
    )
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

fn make_jpeg(width: u32, height: u32) -> Vec<u8> {
  let mut rgb = vec![0u8; (width * height * 3) as usize];
  for (i, chunk) in rgb.chunks_exact_mut(3).enumerate() {
    let x = (i as u32 % width) as u8;
    let y = (i as u32 / width) as u8;
    chunk[0] = x.wrapping_mul(8);
    chunk[1] = y.wrapping_mul(8);
    chunk[2] = 128;
  }
  let mut jpeg = Vec::new();
  image::write_buffer_with_format(
    &mut std::io::Cursor::new(&mut jpeg),
    &rgb,
    width,
    height,
    image::ColorType::Rgb8,
    image::ImageFormat::Jpeg,
  )
  .expect("encode jpeg");
  jpeg
}

fn make_avif() -> Vec<u8> {
  crate::avif_encode::encode_image_to_avif(&make_jpeg(32, 32))
    .expect("opaque jpeg must encode")
    .bytes
}

async fn set_conv(cache: &Arc<ImageCache>, cache_key: &str, state: &str, attempts: i64) {
  sqlx::query("UPDATE entries SET conv_state = ?, conv_attempts = ? WHERE cache_key = ?")
    .bind(state)
    .bind(attempts)
    .bind(cache_key)
    .execute(&cache.pool)
    .await
    .unwrap();
}

async fn get_row(cache: &Arc<ImageCache>, cache_key: &str) -> (String, i64, i64, i64) {
  let row: (String, i64, i64, i64) = sqlx::query_as(
    "SELECT conv_state, conv_attempts, conv_next_at, conv_policy_version FROM entries WHERE cache_key = ?",
  )
  .bind(cache_key)
  .fetch_one(&cache.pool)
  .await
  .unwrap();
  row
}

#[tokio::test]
async fn recover_resets_abandoned_encoding_claims() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/abandoned/Images/Primary";
  write_entry(&cache, url, b"jpeg").await;
  let cache_key = ImageCache::cache_key(&partition(), url);

  // Simulate a crashed owner leaving a row mid-claim.
  set_conv(&cache, &cache_key, "encoding", 0).await;

  cache.recover_on_adopt().await.expect("recover");

  let (state, _attempts, next_at, _ver) = get_row(&cache, &cache_key).await;
  assert_eq!(state, "pending", "abandoned claim must reset to pending");
  assert_eq!(next_at, 0, "reset claim must be immediately due");
}

#[tokio::test]
async fn recover_removes_stale_temp_but_keeps_locked_temp() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let images = dir.path().join("images");

  // An abandoned temp with no live writer lock must be removed.
  let abandoned = images.join(".tmp-abandoned-1");
  tokio::fs::write(&abandoned, b"partial").await.unwrap();

  // A temp whose exclusive lock is held (live writer) must be preserved.
  let owned = images.join(".tmp-owned-2");
  let owned_file = std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .truncate(true)
    .open(&owned)
    .unwrap();
  fs2::FileExt::lock_exclusive(&owned_file).unwrap();

  cache.recover_on_adopt().await.expect("recover");

  assert!(
    tokio::fs::metadata(&abandoned).await.is_err(),
    "unlocked stale temp must be removed"
  );
  assert!(
    tokio::fs::metadata(&owned).await.is_ok(),
    "locked temp owned by a live writer must be preserved"
  );

  fs2::FileExt::unlock(&owned_file).unwrap();
}

#[tokio::test]
async fn recover_requeues_old_policy_terminal_but_keeps_active_avif() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/policy/Images/Primary";
  write_entry(&cache, url, b"jpeg").await;
  let cache_key = ImageCache::cache_key(&partition(), url);

  // Terminal failure under an older policy version.
  sqlx::query("UPDATE entries SET conv_state='failed', conv_policy_version=0 WHERE cache_key=?")
    .bind(&cache_key)
    .execute(&cache.pool)
    .await
    .unwrap();

  // A second entry that is AVIF-active under the old policy must be untouched.
  let url2 = "https://media.example.com/Items/policy-avif/Images/Primary";
  write_entry(&cache, url2, b"jpeg").await;
  let cache_key2 = ImageCache::cache_key(&partition(), url2);
  let reader = cache.open_reader(&partition(), url2).await.unwrap();
  let orig2 = reader
    .path()
    .file_name()
    .unwrap()
    .to_str()
    .unwrap()
    .to_string();
  drop(reader);
  let avif2 = ImageCache::avif_file_name_for(&orig2);
  let avif_bytes = make_avif();
  tokio::fs::write(cache.path_for(&avif2), &avif_bytes)
    .await
    .unwrap();
  cache
    .activate_avif(
      &cache_key2,
      &avif2,
      avif_bytes.len() as u64,
      "d",
      "image/avif",
      cache.current_epoch(),
    )
    .await
    .unwrap();
  sqlx::query("UPDATE entries SET conv_policy_version=0 WHERE cache_key=?")
    .bind(&cache_key2)
    .execute(&cache.pool)
    .await
    .unwrap();

  cache.recover_on_adopt().await.expect("recover");

  // Terminal old-policy origin-active row is requeued with the current version.
  let (state, attempts, _next, ver) = get_row(&cache, &cache_key).await;
  assert_eq!(state, "pending", "old-policy terminal row must be requeued");
  assert_eq!(attempts, 0);
  assert_eq!(
    ver, CONVERSION_POLICY_VERSION,
    "requeued row must take the current policy version"
  );

  // AVIF-active row is left unchanged even though its version is old.
  let reader = cache.open_reader(&partition(), url2).await.unwrap();
  assert_eq!(
    reader.content_type(),
    Some("image/avif"),
    "active AVIF must remain active across policy requeue"
  );
}

#[tokio::test]
async fn recover_adopts_valid_orphan_avif_and_deletes_invalid() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");

  // Valid orphan: a real AVIF renamed into place but never activated.
  let url = "https://media.example.com/Items/orphan-good/Images/Primary";
  write_entry(&cache, url, &make_jpeg(32, 32)).await;
  let cache_key = ImageCache::cache_key(&partition(), url);
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  let orig = reader
    .path()
    .file_name()
    .unwrap()
    .to_str()
    .unwrap()
    .to_string();
  drop(reader);
  let avif_name = ImageCache::avif_file_name_for(&orig);
  let avif_bytes = make_avif();
  // Ensure the economic gate passes regardless of tiny synthetic sizes.
  sqlx::query("UPDATE entries SET original_size_bytes = ? WHERE cache_key = ?")
    .bind((avif_bytes.len() as i64) * 3)
    .bind(&cache_key)
    .execute(&cache.pool)
    .await
    .unwrap();
  tokio::fs::write(cache.path_for(&avif_name), &avif_bytes)
    .await
    .unwrap();

  // Invalid orphan: garbage at the AVIF path.
  let url_bad = "https://media.example.com/Items/orphan-bad/Images/Primary";
  write_entry(&cache, url_bad, b"jpeg").await;
  let cache_key_bad = ImageCache::cache_key(&partition(), url_bad);
  let reader = cache.open_reader(&partition(), url_bad).await.unwrap();
  let orig_bad = reader
    .path()
    .file_name()
    .unwrap()
    .to_str()
    .unwrap()
    .to_string();
  drop(reader);
  let avif_bad = ImageCache::avif_file_name_for(&orig_bad);
  let avif_bad_path = cache.path_for(&avif_bad);
  tokio::fs::write(&avif_bad_path, b"not-an-avif")
    .await
    .unwrap();

  cache.recover_on_adopt().await.expect("recover");

  // Valid orphan is adopted: it becomes the active representation.
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  assert_eq!(
    reader.content_type(),
    Some("image/avif"),
    "valid orphan AVIF must be adopted"
  );
  drop(reader);

  // Invalid orphan is deleted and the row requeued for a fresh encode.
  assert!(
    tokio::fs::metadata(&avif_bad_path).await.is_err(),
    "invalid orphan AVIF must be deleted"
  );
  let (state, _a, _n, _v) = get_row(&cache, &cache_key_bad).await;
  assert_eq!(state, "pending", "invalid orphan row must be requeued");
}

#[tokio::test]
async fn recover_finishes_deferred_origin_cleanup() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/deferred/Images/Primary";
  write_entry(&cache, url, &make_jpeg(32, 32)).await;
  let cache_key = ImageCache::cache_key(&partition(), url);
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  let orig_path = reader.path().to_path_buf();
  let orig = orig_path.file_name().unwrap().to_str().unwrap().to_string();
  drop(reader);
  let avif_name = ImageCache::avif_file_name_for(&orig);
  let avif_bytes = make_avif();
  tokio::fs::write(cache.path_for(&avif_name), &avif_bytes)
    .await
    .unwrap();
  cache
    .activate_avif(
      &cache_key,
      &avif_name,
      avif_bytes.len() as u64,
      "d",
      "image/avif",
      cache.current_epoch(),
    )
    .await
    .unwrap();

  // Simulate a crash between activation and origin deletion.
  assert!(tokio::fs::metadata(&orig_path).await.is_ok());

  cache.recover_on_adopt().await.expect("recover");

  // Original is removed; AVIF stays active.
  assert!(
    tokio::fs::metadata(&orig_path).await.is_err(),
    "deferred origin must be removed on recovery"
  );
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  assert_eq!(reader.content_type(), Some("image/avif"));
}

#[tokio::test]
async fn retry_schedule_is_10s_1m_10m_and_terminal_after_four() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/retry/Images/Primary";
  write_entry(&cache, url, b"jpeg").await;
  let cache_key = ImageCache::cache_key(&partition(), url);

  cache
    .record_conversion_failure(&cache_key, "t", 1_000)
    .await
    .unwrap();
  let (s, a, n, _v) = get_row(&cache, &cache_key).await;
  assert_eq!((s.as_str(), a, n), ("failed", 1, 11_000));

  cache
    .record_conversion_failure(&cache_key, "t", 11_000)
    .await
    .unwrap();
  let (s, a, n, _v) = get_row(&cache, &cache_key).await;
  assert_eq!((s.as_str(), a, n), ("failed", 2, 71_000));

  cache
    .record_conversion_failure(&cache_key, "t", 71_000)
    .await
    .unwrap();
  let (s, a, n, _v) = get_row(&cache, &cache_key).await;
  assert_eq!((s.as_str(), a, n), ("failed", 3, 671_000));

  cache
    .record_conversion_failure(&cache_key, "t", 671_000)
    .await
    .unwrap();
  let (s, a, n, _v) = get_row(&cache, &cache_key).await;
  assert_eq!(
    a, MAX_CONVERSION_ATTEMPTS as i64,
    "must use all four attempts"
  );
  assert_eq!((s.as_str(), n), ("failed", 0), "terminal after exhaustion");
}

#[tokio::test]
async fn claim_is_due_only_and_oldest_first() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url_a = "https://media.example.com/Items/a/Images/Primary";
  let url_b = "https://media.example.com/Items/b/Images/Primary";
  write_entry(&cache, url_a, b"a").await;
  write_entry(&cache, url_b, b"b").await;
  let key_a = ImageCache::cache_key(&partition(), url_a);
  let key_b = ImageCache::cache_key(&partition(), url_b);

  // Force deterministic enqueue order: A older than B.
  sqlx::query("UPDATE entries SET created_at = 1 WHERE cache_key = ?")
    .bind(&key_a)
    .execute(&cache.pool)
    .await
    .unwrap();
  sqlx::query("UPDATE entries SET created_at = 2 WHERE cache_key = ?")
    .bind(&key_b)
    .execute(&cache.pool)
    .await
    .unwrap();

  // Make A not yet due; only B is due.
  sqlx::query("UPDATE entries SET conv_next_at = 1_000 WHERE cache_key = ?")
    .bind(&key_a)
    .execute(&cache.pool)
    .await
    .unwrap();

  let claim = cache.claim_work(500).await.unwrap().expect("due claim");
  assert_eq!(claim.cache_key, key_b, "not-yet-due A must not be claimed");

  // Once A is due, it is claimed first because it was enqueued first.
  set_conv(&cache, &key_b, "pending", 0).await;
  sqlx::query("UPDATE entries SET conv_next_at = 0 WHERE cache_key = ?")
    .bind(&key_b)
    .execute(&cache.pool)
    .await
    .unwrap();
  sqlx::query("UPDATE entries SET conv_next_at = 0 WHERE cache_key = ?")
    .bind(&key_a)
    .execute(&cache.pool)
    .await
    .unwrap();
  let claim = cache.claim_work(1_000).await.unwrap().expect("claim");
  assert_eq!(
    claim.cache_key, key_a,
    "oldest-enqueued row must be claimed"
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
async fn epoch_guard_blocks_stale_activate_but_allows_fresh() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");
  let url = "https://media.example.com/Items/epoch-a/Images/Primary";
  write_entry(&cache, url, &make_jpeg(32, 32)).await;
  let cache_key = ImageCache::cache_key(&partition(), url);
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  let orig = reader
    .path()
    .file_name()
    .unwrap()
    .to_str()
    .unwrap()
    .to_string();
  drop(reader);
  let avif_name = ImageCache::avif_file_name_for(&orig);
  let avif_bytes = make_avif();
  tokio::fs::write(cache.path_for(&avif_name), &avif_bytes)
    .await
    .unwrap();

  let stale_epoch = cache.current_epoch();
  cache.bump_epoch();

  // Stale-epoch activation is rejected.
  let err = cache
    .activate_avif(
      &cache_key,
      &avif_name,
      avif_bytes.len() as u64,
      "d",
      "image/avif",
      stale_epoch,
    )
    .await;
  assert!(err.is_err(), "stale-epoch activation must fail");
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  assert_eq!(reader.content_type(), Some("image/jpeg"), "still origin");
  drop(reader);

  // A fresh-epoch activation succeeds.
  cache
    .activate_avif(
      &cache_key,
      &avif_name,
      avif_bytes.len() as u64,
      "d",
      "image/avif",
      cache.current_epoch(),
    )
    .await
    .expect("fresh activation");
  let reader = cache.open_reader(&partition(), url).await.unwrap();
  assert_eq!(reader.content_type(), Some("image/avif"));
}

async fn set_conv_full(
  cache: &Arc<ImageCache>,
  cache_key: &str,
  state: &str,
  attempts: i64,
  next_at: i64,
) {
  sqlx::query(
    "UPDATE entries SET conv_state = ?, conv_attempts = ?, conv_next_at = ? WHERE cache_key = ?",
  )
  .bind(state)
  .bind(attempts)
  .bind(next_at)
  .bind(cache_key)
  .execute(&cache.pool)
  .await
  .unwrap();
}

#[tokio::test]
async fn status_reports_committed_pending_savings_and_failures() {
  let dir = TempDirGuard::new();
  let cache = ImageCache::init(dir.path(), IMAGE_CACHE_MAX_BYTES)
    .await
    .expect("init");

  // A: pending origin of 500 bytes.
  let url_a = "https://media.example.com/Items/a/Images/Primary";
  write_entry(&cache, url_a, &[0u8; 500]).await;

  // B: accepted AVIF; original 1000 -> avif 100, so 900 saved.
  let url_b = "https://media.example.com/Items/b/Images/Primary";
  write_entry(&cache, url_b, &[0u8; 1000]).await;
  let key_b = ImageCache::cache_key(&partition(), url_b);
  let reader = cache.open_reader(&partition(), url_b).await.unwrap();
  let orig_b = reader
    .path()
    .file_name()
    .unwrap()
    .to_str()
    .unwrap()
    .to_string();
  drop(reader);
  let avif_b = ImageCache::avif_file_name_for(&orig_b);
  tokio::fs::write(cache.path_for(&avif_b), &[0u8; 100])
    .await
    .unwrap();
  cache
    .activate_avif(
      &key_b,
      &avif_b,
      100,
      "d",
      "image/avif",
      cache.current_epoch(),
    )
    .await
    .unwrap();

  // C: terminal failure (attempts exhausted) of 200 bytes.
  let url_c = "https://media.example.com/Items/c/Images/Primary";
  write_entry(&cache, url_c, &[0u8; 200]).await;
  let key_c = ImageCache::cache_key(&partition(), url_c);
  set_conv_full(&cache, &key_c, "failed", MAX_CONVERSION_ATTEMPTS as i64, 0).await;

  // D: delayed retry (failed, attempts left, future next_at) of 50 bytes.
  let url_d = "https://media.example.com/Items/d/Images/Primary";
  write_entry(&cache, url_d, &[0u8; 50]).await;
  let key_d = ImageCache::cache_key(&partition(), url_d);
  set_conv_full(&cache, &key_d, "failed", 2, 9_999_999_999).await;

  let status = cache.status(true).await.expect("status");
  assert_eq!(status.committed_bytes, 500 + 100 + 200 + 50);
  assert_eq!(
    status.pending_count, 2,
    "pending must include A and the delayed retry D"
  );
  assert_eq!(
    status.estimated_savings, 900,
    "current-only accepted savings"
  );
  assert_eq!(status.terminal_failures, 1);
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
