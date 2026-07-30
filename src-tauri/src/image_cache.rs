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
  sync::Arc,
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
        cache_key     TEXT PRIMARY KEY,
        scope         TEXT NOT NULL,
        file_name     TEXT NOT NULL,
        size_bytes    INTEGER NOT NULL,
        content_type  TEXT,
        content_digest TEXT,
        accessed_at   INTEGER NOT NULL,
        created_at    INTEGER NOT NULL
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
      let has_digest: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info('entries') WHERE name = 'content_digest')",
      )
      .fetch_one(pool)
      .await?;
      if !has_digest {
        sqlx::query("ALTER TABLE entries ADD COLUMN content_digest TEXT")
          .execute(pool)
          .await?;
      }
      sqlx::query("PRAGMA user_version = 1").execute(pool).await?;
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
    })
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

  fn cache_key(partition: &ImageCachePartition, remote_url: &str) -> String {
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
  ) -> Result<(), ImageCacheError> {
    if size_bytes > IMAGE_CACHE_MAX_ENTRY_BYTES {
      return Err(ImageCacheError::WriterClosed);
    }
    if size_bytes > self.max_bytes {
      return Err(ImageCacheError::WriterClosed);
    }

    let _commit = self.commit_lock.lock().await;

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
      INSERT INTO entries (cache_key, scope, file_name, size_bytes, content_type, content_digest, accessed_at, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(cache_key) DO UPDATE SET
        file_name = excluded.file_name,
        size_bytes = excluded.size_bytes,
        content_type = excluded.content_type,
        content_digest = excluded.content_digest,
        accessed_at = excluded.accessed_at
      "#,
    )
    .bind(cache_key)
    .bind(partition.scope())
    .bind(file_name)
    .bind(size_bytes as i64)
    .bind(content_type)
    .bind(content_digest)
    .bind(now)
    .bind(now)
    .execute(&self.pool)
    .await?;

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
