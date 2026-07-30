//! Disk-backed write-through cache for media server artwork.
//!
//! One global SQLite catalog (via SQLx) is the single durable source of truth
//! for cached Library Image entries, replacing the former per-partition JSON
//! index. The catalog is an optional optimization: initialization, migration,
//! and reconciliation run asynchronously and every failure mode fails open to
//! streaming the image from its origin.
//!
//! Cache identity is `provider + normalized server URL + full origin URL`,
//! hashed into a collision-resistant relative filename so entries never
//! collide across servers. Committed files live within one global byte budget;
//! eviction is LRU over unlocked entries and is safe against active readers
//! through in-process reference counts plus cross-process file locks.

use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
  },
  time::{SystemTime, UNIX_EPOCH},
};

use fs2::FileExt;
use parking_lot::{Mutex as StdMutex, RwLock};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{FromRow, SqlitePool};
use tokio::sync::{mpsc, Mutex as TokioMutex, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::image_ref::normalize_server_url;
use crate::jellyfin::MediaServerProvider;

/// Global committed-byte budget shared by every cached server.
pub const IMAGE_CACHE_MAX_BYTES: u64 = 1024 * 1024 * 1024;
/// Origins larger than this stream to the client without being cached.
pub const IMAGE_CACHE_MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum in-flight tee buffer per elected writer.
pub const WRITER_BUFFER_BYTES: usize = 1024 * 1024;
/// Maximum number of concurrent disk writers process-wide.
pub const MAX_CONCURRENT_WRITERS: usize = 8;
/// Current conversion-policy version. Bumping this requeues terminal entries
/// whose originals still exist so they are re-evaluated under the new policy.
pub const CONVERSION_POLICY_VERSION: i64 = 1;
/// Total conversion attempts for transient failures (initial + 3 retries).
pub const MAX_CONVERSION_ATTEMPTS: u32 = 4;
/// Retry delays after attempt 1, 2, 3: 10s, 1m, 10m.
pub const RETRY_DELAYS_MS: [i64; 3] = [10_000, 60_000, 600_000];

const CATALOG_FILE: &str = "catalog.sqlite3";

#[derive(Debug, thiserror::Error)]
pub enum ImageCacheError {
  #[error("I/O error: {0}")]
  Io(#[from] std::io::Error),
  #[error("catalog error: {0}")]
  Catalog(#[from] sqlx::Error),
  #[error("system clock is before unix epoch")]
  Clock,
  #[error("cache writer already committed or aborted")]
  WriterClosed,
}

/// One cached representation on disk, guarded against eviction for its lifetime.
pub struct CacheReaderGuard {
  path: PathBuf,
  content_type: Option<String>,
  content_digest: Option<String>,
  size_bytes: u64,
  _guard: ReaderGuard,
  _file_lock: std::fs::File,
}

impl CacheReaderGuard {
  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn content_type(&self) -> Option<&str> {
    self.content_type.as_deref()
  }

  /// SHA-256 digest of the active representation's bytes, when recorded.
  pub fn content_digest(&self) -> Option<&str> {
    self.content_digest.as_deref()
  }

  pub fn size_bytes(&self) -> u64 {
    self.size_bytes
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageCachePartition {
  provider_slug: &'static str,
  server_hash: String,
}

impl ImageCachePartition {
  /// Stable identity prefix used to scope filenames and catalog rows.
  fn scope(&self) -> String {
    format!("{}-{}", self.provider_slug, self.server_hash)
  }
}

#[derive(Clone, Default)]
struct ActiveReaders {
  counts: Arc<StdMutex<HashMap<PathBuf, usize>>>,
}

impl ActiveReaders {
  fn acquire(&self, path: PathBuf) -> ReaderGuard {
    let mut counts = self.counts.lock();
    *counts.entry(path.clone()).or_insert(0) += 1;
    ReaderGuard {
      path,
      active_readers: self.clone(),
    }
  }

  fn release(&self, path: &Path) {
    let mut counts = self.counts.lock();
    if let std::collections::hash_map::Entry::Occupied(mut entry) = counts.entry(path.to_path_buf())
    {
      *entry.get_mut() -= 1;
      if *entry.get() == 0 {
        entry.remove();
      }
    }
  }

  fn is_active(&self, path: &Path) -> bool {
    let counts = self.counts.lock();
    counts.get(path).copied().unwrap_or(0) > 0
  }
}

pub struct ReaderGuard {
  path: PathBuf,
  active_readers: ActiveReaders,
}

impl Drop for ReaderGuard {
  fn drop(&mut self) {
    self.active_readers.release(&self.path);
  }
}

async fn acquire_reader_guard(
  active_readers: &ActiveReaders,
  path: PathBuf,
  content_type: Option<String>,
  content_digest: Option<String>,
  size_bytes: u64,
) -> Result<CacheReaderGuard, ImageCacheError> {
  let path_for_file = path.clone();
  let file_lock = tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
      .read(true)
      .open(&path_for_file)?;
    FileExt::lock_shared(&file)?;
    Ok(file)
  })
  .await
  .map_err(|e| ImageCacheError::Io(std::io::Error::other(e)))??;

  let guard = active_readers.acquire(path.clone());
  Ok(CacheReaderGuard {
    path,
    content_type,
    content_digest,
    size_bytes,
    _guard: guard,
    _file_lock: file_lock,
  })
}

#[derive(FromRow)]
struct EntryRow {
  file_name: String,
  size_bytes: i64,
  content_type: Option<String>,
  content_digest: Option<String>,
}

/// SQLite-backed global Library Image cache.
pub struct ImageCache {
  root: PathBuf,
  pool: SqlitePool,
  max_bytes: u64,
  active_readers: ActiveReaders,
  writer_permits: Arc<Semaphore>,
  /// Serializes admission/eviction so budget accounting stays consistent.
  commit_lock: TokioMutex<()>,
  /// Wakes the background conversion worker when new work is committed.
  work_notify: StdMutex<Option<Arc<tokio::sync::Notify>>>,
  /// Destructive-maintenance epoch. Bumped by Clear so writers/encoders that
  /// started before it cannot publish across the reset.
  epoch: AtomicU64,
}

#[derive(Clone)]
pub struct ImageCacheState(pub Arc<RwLock<Option<Arc<ImageCache>>>>);

impl ImageCacheState {
  pub fn empty() -> Self {
    Self(Arc::new(RwLock::new(None)))
  }
}

impl ImageCache {
  /// Open (or create) the global catalog, migrating and reconciling it.
  ///
  /// A corrupt or unmigratable catalog is quarantined and rebuilt once. Any
  /// unrecoverable failure is reported so the caller can fail open.
  pub async fn init(root: PathBuf, max_bytes: u64) -> Result<Arc<Self>, ImageCacheError> {
    match Self::open_pool(&root).await {
      Ok(pool) => Ok(Self::from_pool(root, pool, max_bytes)),
      Err(err) => {
        log::warn!(
          "Image cache catalog unusable ({}); quarantining and rebuilding",
          err
        );
        Self::quarantine(&root).await;
        let pool = Self::open_pool(&root).await?;
        Ok(Self::from_pool(root, pool, max_bytes))
      }
    }
  }

  async fn open_pool(root: &Path) -> Result<SqlitePool, ImageCacheError> {
    let images_dir = root.join("images");
    tokio::fs::create_dir_all(&images_dir).await?;
    let db_path = images_dir.join(CATALOG_FILE);

    let options = SqliteConnectOptions::new()
      .filename(&db_path)
      .create_if_missing(true)
      .journal_mode(SqliteJournalMode::Wal)
      .synchronous(SqliteSynchronous::Normal)
      .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
      .max_connections(4)
      .connect_with(options)
      .await?;

    Self::migrate(&pool).await?;
    Ok(pool)
  }

  async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
      r#"
      CREATE TABLE IF NOT EXISTS entries (
        cache_key      TEXT PRIMARY KEY,
        scope          TEXT NOT NULL,
        file_name      TEXT NOT NULL,
        size_bytes     INTEGER NOT NULL,
        content_type   TEXT,
        content_digest TEXT,
        accessed_at    INTEGER NOT NULL,
        created_at     INTEGER NOT NULL,
        active_kind    TEXT NOT NULL DEFAULT 'origin',
        original_file_name TEXT,
        original_size_bytes INTEGER,
        original_content_type TEXT,
        original_content_digest TEXT,
        avif_file_name TEXT,
        avif_size_bytes INTEGER,
        avif_content_digest TEXT,
        conv_state     TEXT NOT NULL DEFAULT 'pending',
        conv_attempts  INTEGER NOT NULL DEFAULT 0,
        conv_next_at   INTEGER NOT NULL DEFAULT 0,
        conv_policy_version INTEGER NOT NULL DEFAULT 0,
        conv_error     TEXT
      )
      "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_accessed ON entries (accessed_at)")
      .execute(pool)
      .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_scope ON entries (scope)")
      .execute(pool)
      .await?;

    // Versioned migrations for catalogs created by earlier builds.
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
      .fetch_one(pool)
      .await?;
    if version < 1 {
      // v1 adds the active-representation content digest used for ETags.
      Self::add_column_if_missing(pool, "entries", "content_digest", "TEXT").await?;
      sqlx::query("PRAGMA user_version = 1").execute(pool).await?;
    }
    if version < 2 {
      // v2 adds serving state (active representation) and durable conversion
      // state for the background AVIF worker.
      Self::add_column_if_missing(
        pool,
        "entries",
        "active_kind",
        "TEXT NOT NULL DEFAULT 'origin'",
      )
      .await?;
      Self::add_column_if_missing(pool, "entries", "original_file_name", "TEXT").await?;
      Self::add_column_if_missing(pool, "entries", "original_size_bytes", "INTEGER").await?;
      Self::add_column_if_missing(pool, "entries", "avif_file_name", "TEXT").await?;
      Self::add_column_if_missing(pool, "entries", "avif_size_bytes", "INTEGER").await?;
      Self::add_column_if_missing(pool, "entries", "avif_content_digest", "TEXT").await?;
      Self::add_column_if_missing(
        pool,
        "entries",
        "conv_state",
        "TEXT NOT NULL DEFAULT 'pending'",
      )
      .await?;
      Self::add_column_if_missing(
        pool,
        "entries",
        "conv_attempts",
        "INTEGER NOT NULL DEFAULT 0",
      )
      .await?;
      Self::add_column_if_missing(
        pool,
        "entries",
        "conv_next_at",
        "INTEGER NOT NULL DEFAULT 0",
      )
      .await?;
      Self::add_column_if_missing(
        pool,
        "entries",
        "conv_policy_version",
        "INTEGER NOT NULL DEFAULT 0",
      )
      .await?;
      Self::add_column_if_missing(pool, "entries", "conv_error", "TEXT").await?;
      sqlx::query("PRAGMA user_version = 2").execute(pool).await?;
    }
    if version < 3 {
      // v3 adds original content metadata for AVIF rejection recovery.
      Self::add_column_if_missing(pool, "entries", "original_content_type", "TEXT").await?;
      Self::add_column_if_missing(pool, "entries", "original_content_digest", "TEXT").await?;
      sqlx::query("PRAGMA user_version = 3").execute(pool).await?;
    }
    Ok(())
  }

  async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
  ) -> Result<(), sqlx::Error> {
    let exists: bool = sqlx::query_scalar(&format!(
      "SELECT EXISTS (SELECT 1 FROM pragma_table_info('{table}') WHERE name = '{column}')"
    ))
    .fetch_one(pool)
    .await?;
    if !exists {
      sqlx::query(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition}"
      ))
      .execute(pool)
      .await?;
    }
    Ok(())
  }

  fn from_pool(root: PathBuf, pool: SqlitePool, max_bytes: u64) -> Arc<Self> {
    Arc::new(Self {
      root,
      pool,
      max_bytes,
      active_readers: ActiveReaders::default(),
      writer_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_WRITERS)),
      commit_lock: TokioMutex::new(()),
      work_notify: StdMutex::new(None),
      epoch: AtomicU64::new(0),
    })
  }

  /// Register the notifier the conversion worker waits on for same-process work.
  pub fn set_work_notify(&self, notify: Arc<tokio::sync::Notify>) {
    *self.work_notify.lock() = Some(notify);
  }

  /// Current destructive-maintenance epoch.
  pub fn current_epoch(&self) -> u64 {
    self.epoch.load(Ordering::SeqCst)
  }

  /// Test-only access to the catalog pool for assertions.
  #[cfg(test)]
  pub fn pool_for_test(&self) -> &SqlitePool {
    &self.pool
  }

  /// Begin a destructive epoch: invalidates the right of any writer or encoder
  /// that started earlier to publish across it. Triggered by the Clear action
  /// (#194); exercised today by the epoch-guard tests.
  #[allow(dead_code)]
  pub fn bump_epoch(&self) -> u64 {
    self.epoch.fetch_add(1, Ordering::SeqCst) + 1
  }

  fn notify_work(&self) {
    if let Some(notify) = self.work_notify.lock().as_ref() {
      notify.notify_one();
    }
  }

  /// Move a corrupt catalog aside without touching image files.
  async fn quarantine(root: &Path) {
    let images_dir = root.join("images");
    for name in [
      CATALOG_FILE.to_string(),
      format!("{CATALOG_FILE}-wal"),
      format!("{CATALOG_FILE}-shm"),
    ] {
      let src = images_dir.join(&name);
      let dst = images_dir.join(format!(".corrupt-{}-{name}", Uuid::new_v4()));
      let _ = tokio::fs::rename(&src, &dst).await;
    }
  }

  pub fn partition(provider: MediaServerProvider, server_url: &str) -> ImageCachePartition {
    let provider_slug = provider_slug(provider);
    let normalized = normalize_server_url(server_url);
    ImageCachePartition {
      provider_slug,
      server_hash: short_hash(normalized.as_bytes()),
    }
  }

  pub fn cache_key(partition: &ImageCachePartition, remote_url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(partition.scope().as_bytes());
    hasher.update([0u8]);
    hasher.update(remote_url.as_bytes());
    format!("{:x}", hasher.finalize())
  }

  fn images_dir(&self) -> PathBuf {
    self.root.join("images")
  }

  fn entry_path(&self, file_name: &str) -> PathBuf {
    self.images_dir().join(file_name)
  }

  /// Look up a cached entry, refreshing its LRU recency. Returns `None` on a
  /// miss, a missing file (which is reconciled away), or any catalog error.
  pub async fn open_reader(
    &self,
    partition: &ImageCachePartition,
    remote_url: &str,
  ) -> Option<CacheReaderGuard> {
    let cache_key = Self::cache_key(partition, remote_url);
    let row: Option<EntryRow> = sqlx::query_as(
      "SELECT file_name, size_bytes, content_type, content_digest FROM entries WHERE cache_key = ?",
    )
    .bind(&cache_key)
    .fetch_optional(&self.pool)
    .await
    .ok()?;

    let row = row?;
    let path = self.entry_path(&row.file_name);

    if tokio::fs::metadata(&path).await.is_err() {
      // Dangling row: reconcile it away and treat as a miss.
      let _ = sqlx::query("DELETE FROM entries WHERE cache_key = ?")
        .bind(&cache_key)
        .execute(&self.pool)
        .await;
      return None;
    }

    // Best-effort LRU touch; never delay the response on it.
    if let Ok(now) = now_ms() {
      let _ = sqlx::query("UPDATE entries SET accessed_at = ? WHERE cache_key = ?")
        .bind(now as i64)
        .bind(&cache_key)
        .execute(&self.pool)
        .await;
    }

    acquire_reader_guard(
      &self.active_readers,
      path,
      row.content_type,
      row.content_digest,
      row.size_bytes.max(0) as u64,
    )
    .await
    .ok()
  }

  /// Try to elect a streaming writer for a miss. Returns `None` when caching is
  /// not admitted (writer slots full, oversized origin, or setup failure); the
  /// caller then streams from origin without caching.
  pub async fn try_begin_writer(
    self: &Arc<Self>,
    partition: &ImageCachePartition,
    remote_url: &str,
    content_type: Option<&str>,
    declared_len: Option<u64>,
  ) -> Option<StreamWriter> {
    if declared_len.is_some_and(|len| len > IMAGE_CACHE_MAX_ENTRY_BYTES) {
      return None;
    }

    let permit = self.writer_permits.clone().try_acquire_owned().ok()?;

    let cache_key = Self::cache_key(partition, remote_url);
    let extension = cache_extension(remote_url, content_type);
    let file_name = format!("{}.{}", short_hash(cache_key.as_bytes()), extension);
    let temp_name = format!(".tmp-{}-{}", file_name, Uuid::new_v4());
    let temp_path = self.entry_path(&temp_name);

    if tokio::fs::create_dir_all(self.images_dir()).await.is_err() {
      return None;
    }
    let file = match tokio::fs::File::create(&temp_path).await {
      Ok(file) => file,
      Err(_) => return None,
    };

    let (tx, rx) = mpsc::channel::<WriterMsg>(32);
    let cache = Arc::clone(self);
    let partition = partition.clone();
    let content_type = content_type.map(String::from);
    let cache_key_clone = cache_key.clone();
    let file_name_clone = file_name.clone();
    let expected_epoch = self.current_epoch();

    let done = tokio::spawn(async move {
      cache
        .run_writer(
          rx,
          file,
          temp_path,
          file_name_clone,
          cache_key_clone,
          partition,
          content_type,
          expected_epoch,
        )
        .await;
    });

    Some(StreamWriter {
      tx: Some(tx),
      buffer_permits: Arc::new(Semaphore::new(WRITER_BUFFER_BYTES)),
      oversized: false,
      _permit: permit,
      _done: done,
    })
  }

  #[allow(clippy::too_many_arguments)]
  async fn run_writer(
    &self,
    mut rx: mpsc::Receiver<WriterMsg>,
    mut file: tokio::fs::File,
    temp_path: PathBuf,
    file_name: String,
    cache_key: String,
    partition: ImageCachePartition,
    content_type: Option<String>,
    expected_epoch: u64,
  ) {
    use tokio::io::AsyncWriteExt;

    let mut written: u64 = 0;
    let mut completed = false;
    let mut hasher = Sha256::new();

    while let Some(msg) = rx.recv().await {
      match msg {
        WriterMsg::Done => {
          completed = true;
          break;
        }
        WriterMsg::Chunk(chunk, permit) => {
          if written.saturating_add(chunk.len() as u64) > IMAGE_CACHE_MAX_ENTRY_BYTES {
            break;
          }
          if let Err(err) = file.write_all(&chunk).await {
            log::debug!("Image cache write failed: {err}");
            break;
          }
          hasher.update(&chunk);
          written += chunk.len() as u64;
          drop(permit);
        }
      }
    }
    drop(rx);

    // Only a clean end-of-stream marker commits. A dropped sender (abandonment,
    // client disconnect, oversized, buffer overflow) leaves `completed` false.
    if !completed {
      drop(file);
      let _ = tokio::fs::remove_file(&temp_path).await;
      return;
    }

    if let Err(err) = file.flush().await {
      log::debug!("Image cache flush failed: {err}");
      drop(file);
      let _ = tokio::fs::remove_file(&temp_path).await;
      return;
    }
    if let Err(err) = file.sync_all().await {
      log::debug!("Image cache sync failed: {err}");
      drop(file);
      let _ = tokio::fs::remove_file(&temp_path).await;
      return;
    }
    drop(file);

    let content_digest = format!("{:x}", hasher.finalize());
    if let Err(err) = self
      .commit(
        &temp_path,
        &file_name,
        &cache_key,
        &partition,
        content_type,
        content_digest,
        written,
        expected_epoch,
      )
      .await
    {
      log::debug!("Image cache commit abandoned: {err}");
      let _ = tokio::fs::remove_file(&temp_path).await;
    }
  }

  /// Atomically publish a completed temp file if the global budget can hold it.
  #[allow(clippy::too_many_arguments)]
  async fn commit(
    &self,
    temp_path: &Path,
    file_name: &str,
    cache_key: &str,
    partition: &ImageCachePartition,
    content_type: Option<String>,
    content_digest: String,
    size_bytes: u64,
    expected_epoch: u64,
  ) -> Result<(), ImageCacheError> {
    if size_bytes > IMAGE_CACHE_MAX_ENTRY_BYTES {
      return Err(ImageCacheError::WriterClosed);
    }
    if size_bytes > self.max_bytes {
      return Err(ImageCacheError::WriterClosed);
    }

    let _commit = self.commit_lock.lock().await;

    // Epoch guard, taken under the same lock that destructive maintenance uses:
    // a writer that started before the epoch changed must not republish across
    // it.
    if self.current_epoch() != expected_epoch {
      return Err(ImageCacheError::WriterClosed);
    }

    self.reconcile_missing().await?;
    if !self.make_room(size_bytes).await? {
      // Budget cannot accommodate even after evicting every unlocked entry.
      return Err(ImageCacheError::WriterClosed);
    }

    let final_path = self.entry_path(file_name);
    tokio::fs::rename(temp_path, &final_path).await?;
    sync_parent_dir(&final_path).await;

    let now = now_ms()? as i64;
    sqlx::query(
      r#"
      INSERT INTO entries (
        cache_key, scope, file_name, size_bytes, content_type, content_digest,
        accessed_at, created_at, active_kind, original_file_name, original_size_bytes,
        original_content_type, original_content_digest,
        conv_state, conv_attempts, conv_next_at, conv_policy_version
      )
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'origin', ?, ?, ?, ?, 'pending', 0, 0, ?)
      ON CONFLICT(cache_key) DO UPDATE SET
        file_name = excluded.file_name,
        size_bytes = excluded.size_bytes,
        content_type = excluded.content_type,
        content_digest = excluded.content_digest,
        accessed_at = excluded.accessed_at,
        active_kind = 'origin',
        original_file_name = excluded.original_file_name,
        original_size_bytes = excluded.original_size_bytes,
        original_content_type = excluded.original_content_type,
        original_content_digest = excluded.original_content_digest,
        conv_state = 'pending',
        conv_attempts = 0,
        conv_next_at = 0,
        conv_policy_version = excluded.conv_policy_version
      "#,
    )
    .bind(cache_key)
    .bind(partition.scope())
    .bind(file_name)
    .bind(size_bytes as i64)
    .bind(&content_type)
    .bind(&content_digest)
    .bind(now)
    .bind(now)
    .bind(file_name)
    .bind(size_bytes as i64)
    .bind(&content_type)
    .bind(&content_digest)
    .bind(CONVERSION_POLICY_VERSION)
    .execute(&self.pool)
    .await?;

    self.notify_work();
    Ok(())
  }

  /// Sum committed bytes currently recorded in the catalog.
  async fn total_bytes(&self) -> Result<u64, ImageCacheError> {
    let total: Option<i64> = sqlx::query_scalar("SELECT COALESCE(SUM(size_bytes), 0) FROM entries")
      .fetch_one(&self.pool)
      .await?;
    Ok(total.unwrap_or(0).max(0) as u64)
  }

  /// Drop catalog rows whose files no longer exist, returning freed bytes.
  async fn reconcile_missing(&self) -> Result<(), ImageCacheError> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT cache_key, file_name FROM entries")
      .fetch_all(&self.pool)
      .await?;
    let mut missing = Vec::new();
    for (cache_key, file_name) in rows {
      if tokio::fs::metadata(self.entry_path(&file_name))
        .await
        .is_err()
      {
        missing.push(cache_key);
      }
    }
    for cache_key in missing {
      let _ = sqlx::query("DELETE FROM entries WHERE cache_key = ?")
        .bind(cache_key)
        .execute(&self.pool)
        .await;
    }
    Ok(())
  }

  /// Evict unlocked LRU entries until `incoming` bytes fit under the budget.
  /// Returns `false` when locked readers prevent making enough room.
  async fn make_room(&self, incoming: u64) -> Result<bool, ImageCacheError> {
    let mut total = self.total_bytes().await?;
    if total.saturating_add(incoming) <= self.max_bytes {
      return Ok(true);
    }

    let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
      "SELECT cache_key, file_name, size_bytes, accessed_at FROM entries ORDER BY accessed_at ASC",
    )
    .fetch_all(&self.pool)
    .await?;

    for (cache_key, file_name, size_bytes, _) in rows {
      if total.saturating_add(incoming) <= self.max_bytes {
        break;
      }
      let path = self.entry_path(&file_name);
      if self.active_readers.is_active(&path) {
        continue;
      }
      if is_file_locked(&path).await {
        continue;
      }
      match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
      }
      let _ = sqlx::query("DELETE FROM entries WHERE cache_key = ?")
        .bind(&cache_key)
        .execute(&self.pool)
        .await;
      total = total.saturating_sub(size_bytes.max(0) as u64);
    }

    Ok(total.saturating_add(incoming) <= self.max_bytes)
  }

  // ---- Background AVIF conversion worker catalog API ----

  /// Claim the oldest due pending/failed-retryable row for conversion.
  ///
  /// The claim is atomic (a conditional UPDATE) so only one process owns a row.
  /// Returns `None` when no work is due.
  pub async fn claim_work(&self, now_ms: i64) -> Result<Option<WorkClaim>, ImageCacheError> {
    let claimed: Option<(String,)> = sqlx::query_as(
      r#"
      UPDATE entries
      SET conv_state = 'encoding'
      WHERE cache_key = (
        SELECT cache_key FROM entries
        WHERE conv_state IN ('pending', 'failed')
          AND conv_attempts < ?
          AND conv_next_at <= ?
        ORDER BY created_at ASC
        LIMIT 1
      )
      RETURNING cache_key
      "#,
    )
    .bind(MAX_CONVERSION_ATTEMPTS as i64)
    .bind(now_ms)
    .fetch_optional(&self.pool)
    .await?;

    let Some((cache_key,)) = claimed else {
      return Ok(None);
    };

    let row: Option<WorkRow> = sqlx::query_as(
      r#"
      SELECT cache_key, original_file_name, original_size_bytes
      FROM entries WHERE cache_key = ?
      "#,
    )
    .bind(&cache_key)
    .fetch_optional(&self.pool)
    .await?;

    Ok(row.map(|r| WorkClaim {
      cache_key: r.cache_key,
      original_file_name: r.original_file_name.unwrap_or_default(),
      original_size_bytes: r.original_size_bytes.unwrap_or(0).max(0) as u64,
    }))
  }

  /// Record a terminal, non-retrying conversion outcome that keeps the origin
  /// active (`not_eligible` or `evaluated` insufficient savings).
  pub async fn record_conversion_skipped(
    &self,
    cache_key: &str,
    state: &str,
  ) -> Result<(), ImageCacheError> {
    sqlx::query(
      r#"
      UPDATE entries
      SET conv_state = ?, conv_error = NULL
      WHERE cache_key = ?
      "#,
    )
    .bind(state)
    .bind(cache_key)
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  /// Record a transient failure, scheduling the next retry attempt (or marking
  /// terminal `failed` once attempts are exhausted).
  pub async fn record_conversion_failure(
    &self,
    cache_key: &str,
    error: &str,
    now_ms: i64,
  ) -> Result<(), ImageCacheError> {
    let attempts: Option<(i64,)> =
      sqlx::query_as("SELECT conv_attempts FROM entries WHERE cache_key = ?")
        .bind(cache_key)
        .fetch_optional(&self.pool)
        .await?;
    let Some((attempts,)) = attempts else {
      return Ok(());
    };
    let attempts = attempts + 1;
    let (state, next_at) = if attempts >= MAX_CONVERSION_ATTEMPTS as i64 {
      ("failed", 0)
    } else {
      let delay = RETRY_DELAYS_MS
        .get(attempts as usize - 1)
        .copied()
        .unwrap_or(RETRY_DELAYS_MS[RETRY_DELAYS_MS.len() - 1]);
      ("failed", now_ms + delay)
    };
    sqlx::query(
      "UPDATE entries SET conv_state = ?, conv_attempts = ?, conv_next_at = ?, conv_error = ? WHERE cache_key = ?",
    )
    .bind(state)
    .bind(attempts)
    .bind(next_at)
    .bind(error)
    .bind(cache_key)
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  /// Publish a generated AVIF as the active representation. The AVIF file must
  /// already be durably renamed to its final path by the caller.
  #[allow(clippy::too_many_arguments)]
  pub async fn activate_avif(
    &self,
    cache_key: &str,
    avif_file_name: &str,
    avif_size_bytes: u64,
    avif_digest: &str,
    avif_content_type: &str,
    expected_epoch: u64,
  ) -> Result<(), ImageCacheError> {
    let _commit = self.commit_lock.lock().await;
    // Epoch guard: an encode started before a destructive epoch must not
    // publish across it.
    if self.current_epoch() != expected_epoch {
      return Err(ImageCacheError::WriterClosed);
    }
    sqlx::query(
      r#"
      UPDATE entries
      SET active_kind = 'avif',
          file_name = ?,
          size_bytes = ?,
          content_type = ?,
          content_digest = ?,
          avif_file_name = ?,
          avif_size_bytes = ?,
          avif_content_digest = ?,
          conv_state = 'accepted',
          conv_error = NULL
      WHERE cache_key = ?
      "#,
    )
    .bind(avif_file_name)
    .bind(avif_size_bytes as i64)
    .bind(avif_content_type)
    .bind(avif_digest)
    .bind(avif_file_name)
    .bind(avif_size_bytes as i64)
    .bind(avif_digest)
    .bind(cache_key)
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  /// Remove the original file after AVIF activation, deferring while readers
  /// hold it. Returns `true` if the original was removed (or already gone).
  pub async fn remove_original_if_idle(&self, cache_key: &str) -> Result<bool, ImageCacheError> {
    let row: Option<(Option<String>,)> =
      sqlx::query_as("SELECT original_file_name FROM entries WHERE cache_key = ?")
        .bind(cache_key)
        .fetch_optional(&self.pool)
        .await?;
    let Some((Some(original_file_name),)) = row else {
      return Ok(true);
    };
    let path = self.entry_path(&original_file_name);
    if self.active_readers.is_active(&path) || is_file_locked(&path).await {
      return Ok(false);
    }
    match tokio::fs::remove_file(&path).await {
      Ok(()) => Ok(true),
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(true),
      Err(err) => Err(err.into()),
    }
  }

  /// Reject an AVIF that the WebView cannot display. Restores the original as
  /// active, deletes the AVIF file, and marks conversion as failed.
  pub async fn reject_avif(&self, cache_key: &str) -> Result<(), ImageCacheError> {
    let row: Option<RejectRow> = sqlx::query_as(
      r#"
        SELECT active_kind, original_file_name, original_size_bytes,
               original_content_type, original_content_digest
        FROM entries WHERE cache_key = ?
        "#,
    )
    .bind(cache_key)
    .fetch_optional(&self.pool)
    .await?;

    let Some(row) = row else {
      return Ok(());
    };
    let RejectRow {
      active_kind,
      original_file_name,
      original_size_bytes,
      original_content_type,
      original_content_digest,
    } = row;

    // Only act if AVIF is currently active.
    if active_kind != "avif" {
      return Ok(());
    }

    let Some(original_file_name) = original_file_name else {
      // No original to restore; mark failed.
      sqlx::query("UPDATE entries SET conv_state = 'failed', conv_error = 'rejected_no_original' WHERE cache_key = ?")
        .bind(cache_key)
        .execute(&self.pool)
        .await?;
      return Ok(());
    };

    let _commit = self.commit_lock.lock().await;

    // Restore original as active.
    sqlx::query(
      r#"
      UPDATE entries
      SET active_kind = 'origin',
          file_name = ?,
          size_bytes = ?,
          content_type = ?,
          content_digest = ?,
          conv_state = 'failed',
          conv_error = 'rejected_by_webview'
      WHERE cache_key = ?
      "#,
    )
    .bind(&original_file_name)
    .bind(original_size_bytes)
    .bind(original_content_type)
    .bind(original_content_digest)
    .bind(cache_key)
    .execute(&self.pool)
    .await?;

    // Delete the AVIF file.
    let avif_file_name = Self::avif_file_name_for(&original_file_name);
    let avif_path = self.entry_path(&avif_file_name);
    match tokio::fs::remove_file(&avif_path).await {
      Ok(()) => {}
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
      Err(err) => return Err(err.into()),
    }

    Ok(())
  }

  /// Recovery-on-adopt: reconcile durable state left by a crashed or upgraded
  /// prior owner. Runs once by the worker immediately after it acquires the
  /// cache-directory lock, so no live process can be mid-claim on any row it
  /// touches.
  pub async fn recover_on_adopt(&self) -> Result<(), ImageCacheError> {
    self.reset_abandoned_claims().await?;
    self.cleanup_stale_temps().await?;
    self.requeue_policy_changed().await?;
    self.recover_orphan_avifs().await?;
    self.finish_deferred_origin_cleanup().await?;
    Ok(())
  }

  /// Rows left in `encoding` by a crashed owner are reset to `pending` so the
  /// new owner retries them. Safe because the exclusive worker lock proves no
  /// live process can be encoding them.
  async fn reset_abandoned_claims(&self) -> Result<(), ImageCacheError> {
    sqlx::query(
      "UPDATE entries SET conv_state='pending', conv_next_at=0 WHERE conv_state='encoding'",
    )
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  /// Remove abandoned `.tmp-*` writer files, but only when a cross-process
  /// exclusive lock proves no live writer still owns them.
  async fn cleanup_stale_temps(&self) -> Result<(), ImageCacheError> {
    let mut entries = match tokio::fs::read_dir(self.images_dir()).await {
      Ok(entries) => entries,
      Err(_) => return Ok(()),
    };
    while let Some(entry) = entries.next_entry().await? {
      let name = entry.file_name();
      let Some(name) = name.to_str() else { continue };
      if !name.starts_with(".tmp-") {
        continue;
      }
      let path = entry.path();
      let lock_path = path.clone();
      let removable = tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new()
          .read(true)
          .write(true)
          .open(&lock_path)
          .ok()?;
        // Non-blocking: acquiring the exclusive lock proves no writer owns it.
        FileExt::try_lock_exclusive(&file).ok()?;
        Some(())
      })
      .await
      .map(|v| v.is_some())
      .unwrap_or(false);
      if removable {
        let _ = tokio::fs::remove_file(&path).await;
      }
    }
    Ok(())
  }

  /// Requeue terminal entries recorded under an older conversion policy whose
  /// originals still exist. Active AVIF representations are left unchanged.
  async fn requeue_policy_changed(&self) -> Result<(), ImageCacheError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
      "SELECT cache_key, original_file_name FROM entries \
       WHERE conv_policy_version != ? AND active_kind = 'origin' \
         AND conv_state IN ('failed','not_eligible','evaluated') AND original_file_name IS NOT NULL",
    )
    .bind(CONVERSION_POLICY_VERSION)
    .fetch_all(&self.pool)
    .await?;
    for (cache_key, original_file_name) in rows {
      // Only requeue when the original bytes are still available to re-encode.
      if tokio::fs::metadata(self.entry_path(&original_file_name))
        .await
        .is_err()
      {
        continue;
      }
      sqlx::query(
        "UPDATE entries SET conv_state='pending', conv_attempts=0, conv_next_at=0, \
         conv_error=NULL, conv_policy_version=? WHERE cache_key=?",
      )
      .bind(CONVERSION_POLICY_VERSION)
      .bind(&cache_key)
      .execute(&self.pool)
      .await?;
    }
    Ok(())
  }

  /// Adopt final AVIF files that were renamed into place before a crash but
  /// never activated in SQLite. Each candidate is structurally and economically
  /// revalidated; valid files are activated, invalid files are deleted and the
  /// row is requeued for a fresh encode.
  async fn recover_orphan_avifs(&self) -> Result<(), ImageCacheError> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
      "SELECT cache_key, original_file_name, original_size_bytes FROM entries \
       WHERE active_kind = 'origin' AND original_file_name IS NOT NULL AND original_size_bytes IS NOT NULL",
    )
    .fetch_all(&self.pool)
    .await?;
    for (cache_key, original_file_name, original_size_bytes) in rows {
      let avif_name = Self::avif_file_name_for(&original_file_name);
      let avif_path = self.entry_path(&avif_name);
      let Ok(avif_bytes) = tokio::fs::read(&avif_path).await else {
        continue;
      };
      let avif_size = avif_bytes.len() as u64;
      let structurally_valid = crate::avif_encode::parse_avif_dimensions(&avif_bytes).is_some();
      let economically_valid =
        crate::avif_encode::has_sufficient_saving(original_size_bytes.max(0) as u64, avif_size);
      if structurally_valid && economically_valid {
        let digest = format!("{:x}", Sha256::digest(&avif_bytes));
        self
          .activate_avif(
            &cache_key,
            &avif_name,
            avif_size,
            &digest,
            "image/avif",
            self.current_epoch(),
          )
          .await?;
      } else {
        // Invalid orphan: delete it and requeue the row for a fresh encode.
        let _ = tokio::fs::remove_file(&avif_path).await;
        sqlx::query("UPDATE entries SET conv_state='pending', conv_next_at=0 WHERE cache_key=?")
          .bind(&cache_key)
          .execute(&self.pool)
          .await?;
      }
    }
    Ok(())
  }

  /// Finish removing originals for rows that activated an AVIF but crashed
  /// before the original was deleted. Retains the active AVIF and defers to
  /// any active readers via the shared removal path.
  async fn finish_deferred_origin_cleanup(&self) -> Result<(), ImageCacheError> {
    let rows: Vec<(String,)> = sqlx::query_as(
      "SELECT cache_key FROM entries WHERE active_kind = 'avif' AND original_file_name IS NOT NULL",
    )
    .fetch_all(&self.pool)
    .await?;
    for (cache_key,) in rows {
      let _ = self.remove_original_if_idle(&cache_key).await?;
    }
    Ok(())
  }

  /// Path to a cache entry file by name.
  pub fn path_for(&self, file_name: &str) -> PathBuf {
    self.entry_path(file_name)
  }

  /// Derive the AVIF filename for an original entry file.
  pub fn avif_file_name_for(original_file_name: &str) -> String {
    let stem = original_file_name
      .rsplit('.')
      .nth(1)
      .unwrap_or(original_file_name);
    format!("{stem}.avif")
  }
}

/// A row claimed by the conversion worker.
#[derive(Debug, Clone)]
pub struct WorkClaim {
  pub cache_key: String,
  pub original_file_name: String,
  pub original_size_bytes: u64,
}

#[derive(FromRow)]
struct WorkRow {
  cache_key: String,
  original_file_name: Option<String>,
  original_size_bytes: Option<i64>,
}

#[derive(FromRow)]
struct RejectRow {
  active_kind: String,
  original_file_name: Option<String>,
  original_size_bytes: Option<i64>,
  original_content_type: Option<String>,
  original_content_digest: Option<String>,
}

/// Message sent from the serving path to an elected disk writer.
enum WriterMsg {
  /// A body chunk plus the buffer permit that bounds it; the writer releases
  /// the permit once the chunk is on disk.
  Chunk(bytes::Bytes, OwnedSemaphorePermit),
  /// Clean end-of-stream: the transfer completed and may be committed.
  Done,
}

/// Handle returned by an elected writer. Feed origin chunks with `try_push`;
/// a `false` return means the cache branch was abandoned and the caller should
/// stop pushing. Only `finish` commits; dropping without it aborts and removes
/// the temp file.
pub struct StreamWriter {
  tx: Option<mpsc::Sender<WriterMsg>>,
  buffer_permits: Arc<Semaphore>,
  oversized: bool,
  _permit: OwnedSemaphorePermit,
  _done: tokio::task::JoinHandle<()>,
}

impl StreamWriter {
  /// Offer a chunk to the disk writer without blocking the client stream.
  /// Returns `false` when the buffer is full or the entry grew oversized; the
  /// cache branch is abandoned and the client stream is unaffected.
  pub fn try_push(&mut self, chunk: bytes::Bytes) -> bool {
    if self.oversized || self.tx.is_none() {
      return false;
    }
    let permit = match self
      .buffer_permits
      .clone()
      .try_acquire_many_owned(chunk.len() as u32)
    {
      Ok(permit) => permit,
      Err(_) => {
        self.abandon();
        return false;
      }
    };
    let Some(tx) = self.tx.as_ref() else {
      return false;
    };
    match tx.try_send(WriterMsg::Chunk(chunk, permit)) {
      Ok(()) => true,
      Err(_) => {
        self.abandon();
        false
      }
    }
  }

  fn abandon(&mut self) {
    self.oversized = true;
    self.tx.take();
  }

  /// Signal a completed transfer and let the background task commit. Consumes
  /// self so the sender cannot be reused afterward.
  pub fn finish(mut self) {
    if let Some(tx) = self.tx.take() {
      let _ = tx.try_send(WriterMsg::Done);
    }
  }
}

impl Drop for StreamWriter {
  fn drop(&mut self) {
    // Dropping without `finish` ends the writer loop without a `Done` marker,
    // so the background task treats the transfer as incomplete and removes its
    // temp file.
    self.tx.take();
  }
}

fn provider_slug(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "jellyfin",
    MediaServerProvider::Emby => "emby",
  }
}

fn short_hash(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  let digest = hasher.finalize();
  let mut out = String::with_capacity(32);
  for byte in &digest[..16] {
    use std::fmt::Write;
    let _ = write!(&mut out, "{byte:02x}");
  }
  out
}

fn cache_extension(remote_url: &str, content_type: Option<&str>) -> &'static str {
  match content_type.and_then(extension_from_content_type) {
    Some(extension) => extension,
    None => extension_from_url(remote_url).unwrap_or("img"),
  }
}

fn extension_from_content_type(content_type: &str) -> Option<&'static str> {
  let media_type = content_type.split(';').next()?.trim().to_ascii_lowercase();
  match media_type.as_str() {
    "image/jpeg" | "image/jpg" => Some("jpg"),
    "image/png" => Some("png"),
    "image/webp" => Some("webp"),
    "image/gif" => Some("gif"),
    "image/avif" => Some("avif"),
    _ => None,
  }
}

fn extension_from_url(remote_url: &str) -> Option<&'static str> {
  let path = remote_url.split('?').next()?.rsplit('/').next()?;
  let extension = path.rsplit('.').next()?.to_ascii_lowercase();
  match extension.as_str() {
    "jpg" | "jpeg" => Some("jpg"),
    "png" => Some("png"),
    "webp" => Some("webp"),
    "gif" => Some("gif"),
    "avif" => Some("avif"),
    _ => None,
  }
}

async fn is_file_locked(path: &Path) -> bool {
  let path = path.to_path_buf();
  tokio::task::spawn_blocking(move || {
    if let Ok(file) = std::fs::OpenOptions::new()
      .read(true)
      .write(true)
      .open(&path)
    {
      FileExt::try_lock_exclusive(&file).is_err()
    } else {
      // Missing/unopenable files are not "locked"; callers handle absence.
      false
    }
  })
  .await
  .unwrap_or(false)
}

async fn sync_parent_dir(path: &Path) {
  if let Some(parent) = path.parent() {
    let parent = parent.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
      if let Ok(dir) = std::fs::File::open(&parent) {
        let _ = dir.sync_all();
      }
    })
    .await;
  }
}

fn now_ms() -> Result<u128, ImageCacheError> {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| duration.as_millis())
    .map_err(|_| ImageCacheError::Clock)
}

#[cfg(test)]
mod tests;
