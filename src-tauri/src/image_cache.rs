//! Disk-backed cache for media server artwork.

use std::{
  collections::HashMap,
  future::Future,
  path::{Path, PathBuf},
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use fs2::FileExt;
use parking_lot::{Mutex as StdMutex, RwLock};
use serde::{Deserialize, Serialize};

use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::image_ref::normalize_server_url;
use crate::jellyfin::MediaServerProvider;

pub const IMAGE_CACHE_MAX_BYTES: u64 = 1024 * 1024 * 1024;
pub const IMAGE_CACHE_DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum ImageCacheError {
  #[error("I/O error: {0}")]
  Io(#[from] std::io::Error),
  #[error("JSON error: {0}")]
  Json(#[from] serde_json::Error),
  #[error("image download failed: {0}")]
  Download(String),
  #[error("system clock is before unix epoch")]
  Clock,
  #[error("cache writer already committed or aborted")]
  WriterClosed,
}

#[derive(Debug, Clone)]
pub struct ImageDownload {
  pub bytes: Vec<u8>,
  pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageCachePartition {
  id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
  file_name: String,
  size_bytes: u64,
  accessed_at_ms: u128,
  #[serde(default)]
  content_type: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheIndex {
  entries: HashMap<String, CacheEntry>,
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

pub struct CacheReaderGuard {
  path: PathBuf,
  content_type: Option<String>,
  size_bytes: u64,
  _guard: ReaderGuard,
  _file_lock: std::fs::File,
}

async fn acquire_reader_guard(
  active_readers: &ActiveReaders,
  path: PathBuf,
  content_type: Option<String>,
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
    size_bytes,
    _guard: guard,
    _file_lock: file_lock,
  })
}

impl CacheReaderGuard {
  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn content_type(&self) -> Option<&str> {
    self.content_type.as_deref()
  }

  pub fn size_bytes(&self) -> u64 {
    self.size_bytes
  }

  pub async fn read_to_vec(&self) -> Result<Vec<u8>, ImageCacheError> {
    Ok(tokio::fs::read(&self.path).await?)
  }
}

pub struct ImageCache {
  root: PathBuf,
  max_bytes: u64,
  index_lock: TokioMutex<()>,
  active_readers: ActiveReaders,
}

#[derive(Clone)]
pub struct ImageCacheState(pub Arc<RwLock<Option<Arc<ImageCache>>>>);

impl ImageCacheState {
  pub fn empty() -> Self {
    Self(Arc::new(RwLock::new(None)))
  }

  pub fn get(&self) -> Option<Arc<ImageCache>> {
    self.0.read().clone()
  }
}

struct PartitionLockGuard<'a> {
  _tokio_guard: tokio::sync::MutexGuard<'a, ()>,
  lock_file: std::fs::File,
}

impl<'a> Drop for PartitionLockGuard<'a> {
  fn drop(&mut self) {
    let _ = FileExt::unlock(&self.lock_file);
  }
}

pub struct CacheWriter<'a> {
  cache: &'a ImageCache,
  partition: ImageCachePartition,
  content_type: Option<String>,
  temp_path: PathBuf,
  file_name: String,
  key: String,
  file: Option<tokio::fs::File>,
  written_bytes: u64,
  committed: bool,
}

impl<'a> CacheWriter<'a> {
  pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), ImageCacheError> {
    let Some(file) = self.file.as_mut() else {
      return Err(ImageCacheError::WriterClosed);
    };
    file.write_all(buf).await?;
    self.written_bytes += buf.len() as u64;
    Ok(())
  }

  pub async fn commit(mut self) -> Result<CacheReaderGuard, ImageCacheError> {
    let Some(mut file) = self.file.take() else {
      return Err(ImageCacheError::WriterClosed);
    };
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    let partition_dir = self.cache.partition_dir(&self.partition);
    let final_path = partition_dir.join(&self.file_name);

    let _lock = self.cache.lock_partition(&self.partition).await?;

    tokio::fs::rename(&self.temp_path, &final_path).await?;
    self.committed = true;

    let reader = acquire_reader_guard(
      &self.cache.active_readers,
      final_path,
      self.content_type.clone(),
      self.written_bytes,
    )
    .await?;

    let mut index = self.cache.load_index(&self.partition).await?;
    index.entries.insert(
      self.key.clone(),
      CacheEntry {
        file_name: self.file_name.clone(),
        size_bytes: self.written_bytes,
        accessed_at_ms: now_ms()?,
        content_type: self.content_type.clone(),
      },
    );

    self
      .cache
      .prune_with_lock(&self.partition, &mut index)
      .await?;
    self.cache.save_index(&self.partition, &index).await?;

    Ok(reader)
  }

  pub async fn abort(mut self) -> Result<(), ImageCacheError> {
    if let Some(file) = self.file.take() {
      drop(file);
    }
    if !self.committed {
      let _ = tokio::fs::remove_file(&self.temp_path).await;
      self.committed = true;
    }
    Ok(())
  }

  pub fn temp_path(&self) -> &Path {
    &self.temp_path
  }
}

impl<'a> Drop for CacheWriter<'a> {
  fn drop(&mut self) {
    if !self.committed {
      let temp_path = self.temp_path.clone();
      self.file.take();
      let _ = std::fs::remove_file(temp_path);
    }
  }
}

impl<'a> tokio::io::AsyncWrite for CacheWriter<'a> {
  fn poll_write(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &[u8],
  ) -> std::task::Poll<Result<usize, std::io::Error>> {
    let Some(file) = self.file.as_mut() else {
      return std::task::Poll::Ready(Err(std::io::Error::other("writer closed or committed")));
    };
    let res = std::pin::Pin::new(file).poll_write(cx, buf);
    if let std::task::Poll::Ready(Ok(n)) = res {
      self.written_bytes += n as u64;
    }
    res
  }

  fn poll_flush(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Result<(), std::io::Error>> {
    let Some(file) = self.file.as_mut() else {
      return std::task::Poll::Ready(Ok(()));
    };
    std::pin::Pin::new(file).poll_flush(cx)
  }

  fn poll_shutdown(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Result<(), std::io::Error>> {
    let Some(file) = self.file.as_mut() else {
      return std::task::Poll::Ready(Ok(()));
    };
    std::pin::Pin::new(file).poll_shutdown(cx)
  }
}

impl ImageCache {
  pub fn new(root: PathBuf) -> Self {
    Self::with_max_bytes(root, IMAGE_CACHE_MAX_BYTES)
  }

  pub fn with_max_bytes(root: PathBuf, max_bytes: u64) -> Self {
    Self {
      root,
      max_bytes,
      index_lock: TokioMutex::new(()),
      active_readers: ActiveReaders::default(),
    }
  }

  pub fn partition(provider: MediaServerProvider, server_url: &str) -> ImageCachePartition {
    let provider = provider_slug(provider);
    let normalized_url = normalize_server_url(server_url);
    ImageCachePartition {
      id: format!("{provider}-{:016x}", stable_hash(normalized_url.as_bytes())),
    }
  }

  pub async fn open_reader(
    &self,
    partition: &ImageCachePartition,
    remote_url: &str,
  ) -> Result<Option<CacheReaderGuard>, ImageCacheError> {
    let _lock = self.lock_partition(partition).await?;
    let mut index = self.load_index(partition).await?;
    let key = cache_key(remote_url);
    let Some(entry) = index.entries.get_mut(&key) else {
      return Ok(None);
    };

    let path = self.partition_dir(partition).join(&entry.file_name);
    if tokio::fs::metadata(&path).await.is_err() {
      index.entries.remove(&key);
      self.save_index(partition, &index).await?;
      return Ok(None);
    }

    entry.accessed_at_ms = now_ms()?;
    let content_type = entry.content_type.clone();
    let size_bytes = entry.size_bytes;
    self.save_index(partition, &index).await?;

    Ok(Some(
      acquire_reader_guard(&self.active_readers, path, content_type, size_bytes).await?,
    ))
  }

  pub async fn begin_write<'a>(
    &'a self,
    partition: &ImageCachePartition,
    remote_url: &str,
    content_type: Option<&str>,
  ) -> Result<CacheWriter<'a>, ImageCacheError> {
    let partition_dir = self.partition_dir(partition);
    tokio::fs::create_dir_all(&partition_dir).await?;

    let key = cache_key(remote_url);
    let extension = cache_extension(remote_url, content_type);
    let file_name = format!("{key}.{extension}");
    let temp_name = format!(".tmp-{key}-{}", Uuid::new_v4());
    let temp_path = partition_dir.join(temp_name);

    let file = tokio::fs::File::create(&temp_path).await?;

    Ok(CacheWriter {
      cache: self,
      partition: partition.clone(),
      content_type: content_type.map(String::from),
      temp_path,
      file_name,
      key,
      file: Some(file),
      written_bytes: 0,
      committed: false,
    })
  }

  pub async fn resolve_image_download<Fut>(
    &self,
    partition: &ImageCachePartition,
    remote_url: &str,
    fetch: Fut,
  ) -> Result<ImageDownload, ImageCacheError>
  where
    Fut: Future<Output = Result<ImageDownload, ImageCacheError>>,
  {
    if let Some(reader) = self.open_reader(partition, remote_url).await? {
      let bytes = reader.read_to_vec().await?;
      return Ok(ImageDownload {
        bytes,
        content_type: reader.content_type().map(String::from),
      });
    }

    let download = tokio::time::timeout(IMAGE_CACHE_DOWNLOAD_TIMEOUT, fetch)
      .await
      .map_err(|_| ImageCacheError::Download("download timed out".to_string()))??;

    let mut writer = self
      .begin_write(partition, remote_url, download.content_type.as_deref())
      .await?;
    writer.write_all(&download.bytes).await?;
    let _reader = writer.commit().await?;

    Ok(download)
  }

  async fn lock_partition<'a>(
    &'a self,
    partition: &ImageCachePartition,
  ) -> Result<PartitionLockGuard<'a>, ImageCacheError> {
    let tokio_guard = self.index_lock.lock().await;
    let partition_dir = self.partition_dir(partition);
    tokio::fs::create_dir_all(&partition_dir).await?;
    let lock_path = partition_dir.join(".lock");

    let lock_file = tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
      let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
      FileExt::lock_exclusive(&file)?;
      Ok(file)
    })
    .await
    .map_err(|e| ImageCacheError::Io(std::io::Error::other(e)))??;

    Ok(PartitionLockGuard {
      _tokio_guard: tokio_guard,
      lock_file,
    })
  }

  async fn prune_with_lock(
    &self,
    partition: &ImageCachePartition,
    index: &mut CacheIndex,
  ) -> Result<(), ImageCacheError> {
    let partition_dir = self.partition_dir(partition);
    let mut total = 0_u64;
    let mut missing = Vec::new();

    for (key, entry) in &index.entries {
      let path = partition_dir.join(&entry.file_name);
      match tokio::fs::metadata(&path).await {
        Ok(metadata) => total = total.saturating_add(metadata.len()),
        Err(_) => missing.push(key.clone()),
      }
    }

    for key in missing {
      index.entries.remove(&key);
    }

    if total <= self.max_bytes {
      return Ok(());
    }

    let mut entries = index
      .entries
      .iter()
      .map(|(key, entry)| {
        (
          key.clone(),
          entry.file_name.clone(),
          entry.size_bytes,
          entry.accessed_at_ms,
        )
      })
      .collect::<Vec<_>>();
    entries.sort_by_key(|(_, _, _, accessed_at_ms)| *accessed_at_ms);

    for (key, file_name, size_bytes, _) in entries {
      if total <= self.max_bytes {
        break;
      }
      let path = partition_dir.join(file_name);
      if self.active_readers.is_active(&path) {
        continue;
      }
      let path_for_lock = path.clone();
      let is_locked = tokio::task::spawn_blocking(move || {
        if let Ok(file) = std::fs::OpenOptions::new()
          .read(true)
          .write(true)
          .open(&path_for_lock)
        {
          FileExt::try_lock_exclusive(&file).is_err()
        } else {
          false
        }
      })
      .await
      .unwrap_or(false);

      if is_locked {
        continue;
      }
      if let Err(err) = tokio::fs::remove_file(&path).await {
        if err.kind() != std::io::ErrorKind::NotFound {
          return Err(err.into());
        }
      }
      total = total.saturating_sub(size_bytes);
      index.entries.remove(&key);
    }

    Ok(())
  }

  async fn load_index(
    &self,
    partition: &ImageCachePartition,
  ) -> Result<CacheIndex, ImageCacheError> {
    let path = self.index_path(partition);
    match tokio::fs::read(&path).await {
      Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(CacheIndex::default()),
      Err(err) => Err(err.into()),
    }
  }

  async fn save_index(
    &self,
    partition: &ImageCachePartition,
    index: &CacheIndex,
  ) -> Result<(), ImageCacheError> {
    let partition_dir = self.partition_dir(partition);
    tokio::fs::create_dir_all(&partition_dir).await?;
    let index_path = self.index_path(partition);
    let temp_index_path = partition_dir.join(format!(".index.json.tmp.{}", Uuid::new_v4()));

    let bytes = serde_json::to_vec_pretty(index)?;
    tokio::fs::write(&temp_index_path, &bytes).await?;

    if let Ok(file) = tokio::fs::File::open(&temp_index_path).await {
      let _ = file.sync_all().await;
    }

    tokio::fs::rename(&temp_index_path, &index_path).await?;
    Ok(())
  }

  fn partition_dir(&self, partition: &ImageCachePartition) -> PathBuf {
    self.root.join("images").join(&partition.id)
  }

  fn index_path(&self, partition: &ImageCachePartition) -> PathBuf {
    self.partition_dir(partition).join("index.json")
  }
}

fn provider_slug(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "jellyfin",
    MediaServerProvider::Emby => "emby",
  }
}

fn cache_key(remote_url: &str) -> String {
  format!("{:016x}", stable_hash(remote_url.as_bytes()))
}

fn stable_hash(bytes: &[u8]) -> u64 {
  let mut hash = 0xcbf29ce484222325_u64;
  for byte in bytes {
    hash ^= u64::from(*byte);
    hash = hash.wrapping_mul(0x100000001b3);
  }
  hash
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

fn now_ms() -> Result<u128, ImageCacheError> {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|duration| duration.as_millis())
    .map_err(|_| ImageCacheError::Clock)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};

  fn temp_cache_dir() -> PathBuf {
    std::env::temp_dir().join(format!("jellypilot-image-cache-test-{}", Uuid::new_v4()))
  }

  fn partition() -> ImageCachePartition {
    ImageCache::partition(MediaServerProvider::Jellyfin, "https://media.example.com/")
  }

  #[tokio::test]
  async fn partition_ignores_trailing_server_slash() {
    let a = ImageCache::partition(MediaServerProvider::Jellyfin, "https://media.example.com");
    let b = ImageCache::partition(MediaServerProvider::Jellyfin, "https://media.example.com/");

    assert_eq!(a.id, b.id);
  }

  #[tokio::test]
  async fn resolve_image_download_reuses_cached_file_after_first_download() {
    let root = temp_cache_dir();
    let cache = ImageCache::with_max_bytes(root.clone(), 1024 * 1024);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_fetch = calls.clone();

    let first = cache
      .resolve_image_download(
        &partition(),
        "https://media.example.com/Items/1/Images/Primary?tag=a",
        async move {
          calls_for_fetch.fetch_add(1, Ordering::SeqCst);
          Ok(ImageDownload {
            bytes: b"image".to_vec(),
            content_type: Some("image/png".to_string()),
          })
        },
      )
      .await
      .expect("first image should cache");
    let second = cache
      .resolve_image_download(
        &partition(),
        "https://media.example.com/Items/1/Images/Primary?tag=a",
        async {
          Ok(ImageDownload {
            bytes: b"changed".to_vec(),
            content_type: Some("image/png".to_string()),
          })
        },
      )
      .await
      .expect("second image should hit cache");

    assert_eq!(first.bytes, b"image");
    assert_eq!(second.bytes, b"image");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_dir_all(root);
  }

  #[tokio::test]
  async fn resolve_image_download_evicts_least_recently_used_files_over_limit() {
    let root = temp_cache_dir();
    let cache = ImageCache::with_max_bytes(root.clone(), 7);
    let partition = partition();
    let first_url = "https://media.example.com/a.png?tag=1";
    let second_url = "https://media.example.com/b.png?tag=2";

    cache
      .resolve_image_download(&partition, first_url, async {
        Ok(ImageDownload {
          bytes: b"12345".to_vec(),
          content_type: Some("image/png".to_string()),
        })
      })
      .await
      .expect("first image should cache");
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    cache
      .resolve_image_download(&partition, second_url, async {
        Ok(ImageDownload {
          bytes: b"67890".to_vec(),
          content_type: Some("image/png".to_string()),
        })
      })
      .await
      .expect("second image should cache");
    let first = cache
      .partition_dir(&partition)
      .join(format!("{}.png", cache_key(first_url)));
    let second = cache
      .partition_dir(&partition)
      .join(format!("{}.png", cache_key(second_url)));

    assert!(!first.exists());
    assert!(second.exists());
    let _ = std::fs::remove_dir_all(root);
  }

  #[tokio::test]
  async fn resolve_image_download_returns_error_when_download_fails() {
    let root = temp_cache_dir();
    let cache = ImageCache::with_max_bytes(root.clone(), 1024 * 1024);
    let remote_url = "https://media.example.com/Items/1/Images/Primary?tag=a".to_string();

    let err = cache
      .resolve_image_download(&partition(), &remote_url, async {
        Err(ImageCacheError::Download("no route".to_string()))
      })
      .await
      .expect_err("failed download should propagate");

    assert_eq!(err.to_string(), "image download failed: no route");
    let _ = std::fs::remove_dir_all(root);
  }

  #[tokio::test]
  async fn stream_write_commit_atomically_creates_cache_hit() {
    let root = temp_cache_dir();
    let cache = ImageCache::with_max_bytes(root.clone(), 1024 * 1024);
    let partition = partition();
    let remote_url = "https://media.example.com/stream1.png";

    let mut writer = cache
      .begin_write(&partition, remote_url, Some("image/png"))
      .await
      .expect("should begin write");
    writer
      .write_all(b"streamed bytes")
      .await
      .expect("should write bytes");

    let temp_path = writer.temp_path().to_path_buf();
    assert!(temp_path.exists());
    assert!(cache
      .open_reader(&partition, remote_url)
      .await
      .expect("reader check")
      .is_none());

    let reader = writer.commit().await.expect("commit should succeed");
    assert!(!temp_path.exists());
    assert_eq!(reader.content_type(), Some("image/png"));

    let opened = cache
      .open_reader(&partition, remote_url)
      .await
      .expect("should open reader")
      .expect("cache hit expected");
    assert_eq!(
      opened.read_to_vec().await.expect("read bytes"),
      b"streamed bytes"
    );

    let _ = std::fs::remove_dir_all(root);
  }

  #[tokio::test]
  async fn stream_write_abort_or_drop_cleans_temp_file_and_no_cache_hit() {
    let root = temp_cache_dir();
    let cache = ImageCache::with_max_bytes(root.clone(), 1024 * 1024);
    let partition = partition();
    let remote_url = "https://media.example.com/stream_aborted.png";

    let temp_path = {
      let mut writer = cache
        .begin_write(&partition, remote_url, Some("image/png"))
        .await
        .expect("should begin write");
      writer
        .write_all(b"partial content")
        .await
        .expect("should write");
      let temp = writer.temp_path().to_path_buf();
      assert!(temp.exists());
      // writer drops here without committing
      temp
    };

    assert!(!temp_path.exists());
    assert!(cache
      .open_reader(&partition, remote_url)
      .await
      .expect("reader check")
      .is_none());

    let _ = std::fs::remove_dir_all(root);
  }

  #[tokio::test]
  async fn active_reader_prevents_lru_eviction() {
    let root = temp_cache_dir();
    let cache = ImageCache::with_max_bytes(root.clone(), 10);
    let partition = partition();

    let first_url = "https://media.example.com/first.png";
    let second_url = "https://media.example.com/second.png";

    let mut w1 = cache
      .begin_write(&partition, first_url, Some("image/png"))
      .await
      .unwrap();
    w1.write_all(b"1234567").await.unwrap(); // 7 bytes
    let r1 = w1.commit().await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    // Writing second entry pushes the cache over its limit. Both committed
    // readers remain protected until their guards are released.
    let mut w2 = cache
      .begin_write(&partition, second_url, Some("image/png"))
      .await
      .unwrap();
    w2.write_all(b"8901234").await.unwrap(); // 7 bytes
    let _r2 = w2.commit().await.unwrap();

    // r1 is still held!
    let first_reader = cache.open_reader(&partition, first_url).await.unwrap();
    assert!(
      first_reader.is_some(),
      "active reader path must survive eviction"
    );

    drop(r1);
    drop(first_reader);

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    // Now write third entry (7 bytes) when r1 is no longer held.
    let third_url = "https://media.example.com/third.png";
    let mut w3 = cache
      .begin_write(&partition, third_url, Some("image/png"))
      .await
      .unwrap();
    w3.write_all(b"5678901").await.unwrap();
    let _r3 = w3.commit().await.unwrap();

    // Now first entry can be evicted.
    let first_after_release = cache.open_reader(&partition, first_url).await.unwrap();
    assert!(
      first_after_release.is_none(),
      "first entry should be evicted after reader dropped"
    );

    let _ = std::fs::remove_dir_all(root);
  }

  #[tokio::test]
  async fn concurrent_instances_lock_guarded_index_updates() {
    let root = temp_cache_dir();
    let cache_a = Arc::new(ImageCache::with_max_bytes(root.clone(), 1024 * 1024));
    let cache_b = Arc::new(ImageCache::with_max_bytes(root.clone(), 1024 * 1024));
    let partition = partition();

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let ca = cache_a.clone();
    let part_a = partition.clone();
    let bar_a = barrier.clone();
    let task_a = tokio::spawn(async move {
      let url_a = "https://media.example.com/instance_a.png";
      let mut wa = ca
        .begin_write(&part_a, url_a, Some("image/png"))
        .await
        .unwrap();
      wa.write_all(b"data_a").await.unwrap();
      bar_a.wait().await;
      wa.commit().await.unwrap();
    });

    let cb = cache_b.clone();
    let part_b = partition.clone();
    let bar_b = barrier.clone();
    let task_b = tokio::spawn(async move {
      let url_b = "https://media.example.com/instance_b.png";
      let mut wb = cb
        .begin_write(&part_b, url_b, Some("image/png"))
        .await
        .unwrap();
      wb.write_all(b"data_b").await.unwrap();
      bar_b.wait().await;
      wb.commit().await.unwrap();
    });

    let (res_a, res_b) = tokio::join!(task_a, task_b);
    res_a.unwrap();
    res_b.unwrap();

    let url_a = "https://media.example.com/instance_a.png";
    let url_b = "https://media.example.com/instance_b.png";

    let read_b_from_a = cache_a.open_reader(&partition, url_b).await.unwrap();
    assert!(
      read_b_from_a.is_some(),
      "cache_a should see write from cache_b"
    );

    let read_a_from_b = cache_b.open_reader(&partition, url_a).await.unwrap();
    assert!(
      read_a_from_b.is_some(),
      "cache_b should see write from cache_a"
    );

    let _ = std::fs::remove_dir_all(root);
  }
  #[tokio::test]
  async fn cross_instance_active_reader_prevents_eviction() {
    let root = temp_cache_dir();
    let cache_a = ImageCache::with_max_bytes(root.clone(), 10);
    let cache_b = ImageCache::with_max_bytes(root.clone(), 10);
    let partition = partition();

    let url1 = "https://media.example.com/shared_1.png";
    let url2 = "https://media.example.com/shared_2.png";

    let mut w1 = cache_a
      .begin_write(&partition, url1, Some("image/png"))
      .await
      .unwrap();
    w1.write_all(b"1234567").await.unwrap(); // 7 bytes
    let r1 = w1.commit().await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    // cache_b writes url2 (7 bytes) -> total 14 > 10.
    // cache_b prune tries to evict url1, but cache_a holds a shared file lock (r1).
    // Therefore, cache_b must skip evicting url1.
    let mut w2 = cache_b
      .begin_write(&partition, url2, Some("image/png"))
      .await
      .unwrap();
    w2.write_all(b"8901234").await.unwrap(); // 7 bytes
    let _r2 = w2.commit().await.unwrap();

    let r1_check = cache_a.open_reader(&partition, url1).await.unwrap();
    assert!(
      r1_check.is_some(),
      "url1 must survive cross-instance eviction while r1 is active"
    );

    drop(r1);
    drop(r1_check);

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    let url3 = "https://media.example.com/shared_3.png";
    let mut w3 = cache_b
      .begin_write(&partition, url3, Some("image/png"))
      .await
      .unwrap();
    w3.write_all(b"5678901").await.unwrap();
    let _r3 = w3.commit().await.unwrap();

    let r1_after = cache_a.open_reader(&partition, url1).await.unwrap();
    assert!(
      r1_after.is_none(),
      "url1 should be evictable after reader guard is dropped across instances"
    );

    let _ = std::fs::remove_dir_all(root);
  }
}
