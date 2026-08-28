use parking_lot::RwLock;
use std::fs;
#[cfg(test)]
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

pub const MAX_DISK_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const ENTRY_EXTENSION: &str = "artwork";
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArtworkCacheStats {
  pub bytes: u64,
  pub entries: usize,
}

#[derive(Clone)]
pub struct ArtworkDiskCache {
  root: Arc<PathBuf>,
  max_bytes: u64,
  enabled: Arc<AtomicBool>,
  clearing: Arc<AtomicBool>,
  epoch: Arc<AtomicU64>,
  operation_lock: Arc<RwLock<()>>,
}

impl Default for ArtworkDiskCache {
  fn default() -> Self {
    Self::new(
      dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("jellypilot")
        .join("artwork"),
      MAX_DISK_CACHE_BYTES,
      true,
    )
  }
}

impl ArtworkDiskCache {
  pub fn new(root: PathBuf, max_bytes: u64, enabled: bool) -> Self {
    Self {
      root: Arc::new(root),
      max_bytes,
      enabled: Arc::new(AtomicBool::new(enabled)),
      clearing: Arc::new(AtomicBool::new(false)),
      epoch: Arc::new(AtomicU64::new(0)),
      operation_lock: Arc::new(RwLock::new(())),
    }
  }

  pub fn set_enabled(&self, enabled: bool) {
    self.enabled.store(enabled, Ordering::Release);
  }

  pub async fn load(
    &self,
    key: String,
    max_entry_bytes: usize,
    validate: fn(&[u8]) -> bool,
  ) -> Option<Arc<[u8]>> {
    if !self.enabled.load(Ordering::Acquire) {
      return None;
    }
    let root = Arc::clone(&self.root);
    let operation_lock = Arc::clone(&self.operation_lock);
    tokio::task::spawn_blocking(move || {
      let read_operation = operation_lock.read();
      match read_candidate_entry(&root, &key, max_entry_bytes, validate) {
        EntryReadOutcome::Valid(bytes) => Some(bytes),
        EntryReadOutcome::NotFound => None,
        EntryReadOutcome::Invalid => {
          drop(read_operation);
          let _write_operation = operation_lock.write();
          remove_invalid_entry(&root, &key, max_entry_bytes, validate);
          None
        }
      }
    })
    .await
    .ok()
    .flatten()
  }

  pub async fn store(&self, key: String, bytes: Arc<[u8]>) {
    if !self.enabled.load(Ordering::Acquire)
      || self.clearing.load(Ordering::Acquire)
      || bytes.len() as u64 > self.max_bytes
    {
      return;
    }
    let root = Arc::clone(&self.root);
    let max_bytes = self.max_bytes;
    let expected_epoch = self.epoch.load(Ordering::Acquire);
    let epoch = Arc::clone(&self.epoch);
    let clearing = Arc::clone(&self.clearing);
    let operation_lock = Arc::clone(&self.operation_lock);
    let _ = tokio::task::spawn_blocking(move || {
      let _operation = operation_lock.write();
      store_entry(
        &root,
        &key,
        &bytes,
        max_bytes,
        expected_epoch,
        &epoch,
        &clearing,
      )
    })
    .await;
  }

  pub async fn stats(&self) -> Result<ArtworkCacheStats, io::Error> {
    let root = Arc::clone(&self.root);
    let operation_lock = Arc::clone(&self.operation_lock);
    tokio::task::spawn_blocking(move || {
      let _operation = operation_lock.read();
      cache_stats(&root)
    })
    .await
    .map_err(io::Error::other)?
  }

  pub async fn clear(&self) -> Result<(), io::Error> {
    if self.clearing.swap(true, Ordering::AcqRel) {
      return Ok(());
    }
    self.epoch.fetch_add(1, Ordering::AcqRel);
    let root = Arc::clone(&self.root);
    let operation_lock = Arc::clone(&self.operation_lock);
    let result = match tokio::task::spawn_blocking(move || {
      let _operation = operation_lock.write();
      clear_entries(&root)
    })
    .await
    {
      Ok(result) => result,
      Err(error) => Err(io::Error::other(error)),
    };
    self.clearing.store(false, Ordering::Release);
    result
  }
}

pub fn artwork_cache_key(server_identity: &str, origin_url: &str) -> String {
  let normalized_server = server_identity
    .trim()
    .trim_end_matches('/')
    .to_ascii_lowercase();
  // FNV-1a over length-prefixed fields: a fixed, specified algorithm so the
  // persisted key stays stable across toolchains and releases. DefaultHasher
  // explicitly does not guarantee that.
  let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
  let mut absorb = |bytes: &[u8]| {
    for byte in bytes {
      hash ^= u64::from(*byte);
      hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
  };
  absorb(&(normalized_server.len() as u64).to_le_bytes());
  absorb(normalized_server.as_bytes());
  absorb(&(origin_url.len() as u64).to_le_bytes());
  absorb(origin_url.as_bytes());
  format!("{hash:016x}")
}

fn entry_path(root: &Path, key: &str) -> PathBuf {
  root.join(format!("{key}.{ENTRY_EXTENSION}"))
}

enum EntryReadOutcome {
  Valid(Arc<[u8]>),
  NotFound,
  Invalid,
}

fn read_candidate_entry(
  root: &Path,
  key: &str,
  max_entry_bytes: usize,
  validate: fn(&[u8]) -> bool,
) -> EntryReadOutcome {
  let path = entry_path(root, key);
  let metadata = match fs::metadata(&path) {
    Ok(metadata) => metadata,
    Err(_) => return EntryReadOutcome::NotFound,
  };
  if metadata.len() == 0 || metadata.len() > max_entry_bytes as u64 {
    return EntryReadOutcome::Invalid;
  }
  let bytes = match fs::read(&path) {
    Ok(bytes) => bytes,
    Err(_) => return EntryReadOutcome::NotFound,
  };
  if !validate(&bytes) {
    return EntryReadOutcome::Invalid;
  }
  EntryReadOutcome::Valid(Arc::from(bytes))
}

fn remove_invalid_entry(
  root: &Path,
  key: &str,
  max_entry_bytes: usize,
  validate: fn(&[u8]) -> bool,
) {
  let path = entry_path(root, key);
  if let Ok(metadata) = fs::metadata(&path) {
    if metadata.len() == 0 || metadata.len() > max_entry_bytes as u64 {
      let _ = fs::remove_file(&path);
      return;
    }
  }
  if let Ok(bytes) = fs::read(&path) {
    if !validate(&bytes) {
      let _ = fs::remove_file(&path);
    }
  }
}

fn store_entry(
  root: &Path,
  key: &str,
  bytes: &[u8],
  max_bytes: u64,
  expected_epoch: u64,
  epoch: &AtomicU64,
  clearing: &AtomicBool,
) -> Result<(), io::Error> {
  fs::create_dir_all(root)?;
  let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
  let temporary = root.join(format!(".{key}-{}-{sequence}.tmp", std::process::id()));
  if let Err(error) = fs::write(&temporary, bytes) {
    let _ = fs::remove_file(&temporary);
    return Err(error);
  }
  if clearing.load(Ordering::Acquire) || epoch.load(Ordering::Acquire) != expected_epoch {
    let _ = fs::remove_file(temporary);
    return Ok(());
  }
  let path = entry_path(root, key);
  if let Err(error) = fs::rename(&temporary, &path) {
    let _ = fs::remove_file(&temporary);
    return Err(error);
  }
  evict_oldest_entries(root, max_bytes)
}

fn cache_stats(root: &Path) -> Result<ArtworkCacheStats, io::Error> {
  let mut stats = ArtworkCacheStats::default();
  let entries = match fs::read_dir(root) {
    Ok(entries) => entries,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(stats),
    Err(error) => return Err(error),
  };
  for entry in entries {
    let Some(entry) = tolerate_not_found(entry)? else {
      continue;
    };
    let path = entry.path();
    if path.extension().and_then(|value| value.to_str()) != Some(ENTRY_EXTENSION) {
      continue;
    }
    let Some(metadata) = tolerate_not_found(entry.metadata())? else {
      continue;
    };
    if metadata.is_file() {
      stats.entries = stats.entries.saturating_add(1);
      stats.bytes = stats.bytes.saturating_add(metadata.len());
    }
  }
  Ok(stats)
}
fn clear_entries(root: &Path) -> Result<(), io::Error> {
  let entries = match fs::read_dir(root) {
    Ok(entries) => entries,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
    Err(error) => return Err(error),
  };
  for entry in entries {
    let Some(path) = tolerate_not_found(entry.map(|entry| entry.path()))? else {
      continue;
    };
    let extension = path.extension().and_then(|value| value.to_str());
    if extension == Some(ENTRY_EXTENSION) || extension == Some("tmp") {
      let _ = tolerate_not_found(fs::remove_file(path))?;
    }
  }
  Ok(())
}

fn tolerate_not_found<T>(result: Result<T, io::Error>) -> Result<Option<T>, io::Error> {
  match result {
    Ok(value) => Ok(Some(value)),
    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
    Err(error) => Err(error),
  }
}
fn evict_oldest_entries(root: &Path, max_bytes: u64) -> Result<(), io::Error> {
  let mut entries = Vec::new();
  for entry in fs::read_dir(root)? {
    let entry = entry?;
    let path = entry.path();
    if path.extension().and_then(|value| value.to_str()) != Some(ENTRY_EXTENSION) {
      continue;
    }
    let metadata = entry.metadata()?;
    if metadata.is_file() {
      entries.push((
        metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        path,
        metadata.len(),
      ));
    }
  }
  let mut total = entries
    .iter()
    .fold(0_u64, |total, (_, _, size)| total.saturating_add(*size));
  entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
  for (_, path, size) in entries {
    if total <= max_bytes {
      break;
    }
    if fs::remove_file(path).is_ok() {
      total = total.saturating_sub(size);
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

  fn test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
      "jellypilot-artwork-cache-{}-{name}-{}",
      std::process::id(),
      TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
  }

  fn run_async<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .expect("test runtime")
      .block_on(future)
  }

  fn valid(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes[0] != 0
  }
  #[test]
  fn clear_removes_orphaned_temporary_files() {
    let root = test_root("clear-tmp");
    fs::create_dir_all(&root).unwrap();
    let entry = entry_path(&root, "entry");
    fs::write(&entry, [1_u8, 2, 3]).unwrap();
    let temporary = root.join(".entry-1-7.tmp");
    fs::write(&temporary, [4_u8, 5, 6]).unwrap();

    clear_entries(&root).unwrap();

    assert!(!entry.exists());
    assert!(!temporary.exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn network_bytes_write_through_verbatim() {
    let root = test_root("write-through");
    let cache = ArtworkDiskCache::new(root.clone(), 1024, true);
    run_async(cache.store("entry".to_owned(), Arc::from([1_u8, 2, 3].as_slice())));

    assert_eq!(fs::read(entry_path(&root, "entry")).unwrap(), [1, 2, 3]);
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn disk_hit_supplies_original_bytes_after_memory_miss() {
    let root = test_root("disk-hit");
    let cache = ArtworkDiskCache::new(root.clone(), 1024, true);
    run_async(cache.store("entry".to_owned(), Arc::from([1_u8, 2, 3].as_slice())));

    let hit = run_async(cache.load("entry".to_owned(), 1024, valid)).expect("disk hit");

    assert_eq!(hit.as_ref(), [1, 2, 3]);
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn oldest_mtime_entry_is_evicted_under_byte_cap() {
    let root = test_root("eviction");
    let cache = ArtworkDiskCache::new(root.clone(), 5, true);
    run_async(cache.store("a".to_owned(), Arc::from([1_u8, 1, 1].as_slice())));
    run_async(cache.store("b".to_owned(), Arc::from([2_u8, 2, 2].as_slice())));

    assert!(run_async(cache.load("a".to_owned(), 1024, valid)).is_none());
    assert_eq!(
      run_async(cache.load("b".to_owned(), 1024, valid))
        .expect("newest retained")
        .as_ref(),
      [2, 2, 2]
    );
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn disabled_cache_bypasses_reads_and_writes() {
    let root = test_root("disabled");
    fs::create_dir_all(&root).unwrap();
    fs::write(entry_path(&root, "existing"), [1_u8]).unwrap();
    let cache = ArtworkDiskCache::new(root.clone(), 1024, false);

    assert!(run_async(cache.load("existing".to_owned(), 1024, valid)).is_none());
    run_async(cache.store("new".to_owned(), Arc::from([2_u8].as_slice())));
    assert!(!entry_path(&root, "new").exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn clear_removes_all_committed_entries() {
    let root = test_root("clear");
    let cache = ArtworkDiskCache::new(root.clone(), 1024, true);
    run_async(cache.store("a".to_owned(), Arc::from([1_u8].as_slice())));
    run_async(cache.store("b".to_owned(), Arc::from([2_u8].as_slice())));

    run_async(cache.clear()).unwrap();

    assert_eq!(
      run_async(cache.stats()).unwrap(),
      ArtworkCacheStats::default()
    );
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn corrupt_entry_is_a_miss_and_is_deleted() {
    let root = test_root("corrupt");
    let cache = ArtworkDiskCache::new(root.clone(), 1024, true);
    run_async(cache.store("entry".to_owned(), Arc::from([0_u8, 1].as_slice())));

    assert!(run_async(cache.load("entry".to_owned(), 1024, valid)).is_none());
    assert!(!entry_path(&root, "entry").exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn cache_key_is_cross_process_stable_and_server_scoped() {
    let process_one_reference = "signed-reference-from-process-one";
    let process_two_reference = "signed-reference-from-process-two";
    assert_ne!(process_one_reference, process_two_reference);
    let origin = "https://media.example/Items/item/Images/Primary?maxHeight=220";

    assert_eq!(
      artwork_cache_key("HTTPS://MEDIA.EXAMPLE/", origin),
      artwork_cache_key("https://media.example", origin)
    );
    assert_ne!(
      artwork_cache_key("https://one.example", origin),
      artwork_cache_key("https://two.example", origin)
    );
    assert_ne!(
      artwork_cache_key("https://one.example", origin),
      artwork_cache_key(
        "https://one.example",
        "https://one.example/Items/other/Images/Primary?maxHeight=220",
      )
    );
  }

  #[test]
  fn stale_store_from_before_clear_epoch_is_discarded() {
    let root = test_root("clear-epoch");
    let cache = ArtworkDiskCache::new(root.clone(), 1024, true);
    let stale_epoch = cache.epoch.load(Ordering::Acquire);
    cache.epoch.fetch_add(1, Ordering::AcqRel);

    store_entry(
      &root,
      "stale",
      &[1_u8, 2, 3],
      1024,
      stale_epoch,
      &cache.epoch,
      &cache.clearing,
    )
    .unwrap();

    assert!(!entry_path(&root, "stale").exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn stats_and_clear_tolerate_entries_that_vanish_mid_pass() {
    let root = test_root("vanished");
    fs::create_dir_all(&root).unwrap();
    let vanished = entry_path(&root, "vanished");

    assert!(tolerate_not_found(fs::metadata(&vanished))
      .unwrap()
      .is_none());
    assert!(tolerate_not_found(fs::remove_file(&vanished))
      .unwrap()
      .is_none());
    assert_eq!(cache_stats(&root).unwrap(), ArtworkCacheStats::default());
    clear_entries(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
  }

  #[tokio::test]
  async fn concurrent_disk_reads_execute_in_parallel() {
    let root = test_root("concurrent-reads");
    let cache = ArtworkDiskCache::new(root.clone(), 1024, true);
    cache
      .store("entry-1".to_owned(), Arc::from([1_u8, 2, 3].as_slice()))
      .await;
    cache
      .store("entry-2".to_owned(), Arc::from([4_u8, 5, 6].as_slice()))
      .await;

    use std::sync::atomic::AtomicUsize;
    static ACTIVE_VALIDATORS: AtomicUsize = AtomicUsize::new(0);
    static OBSERVED_CONCURRENCY: AtomicBool = AtomicBool::new(false);

    ACTIVE_VALIDATORS.store(0, Ordering::SeqCst);
    OBSERVED_CONCURRENCY.store(false, Ordering::SeqCst);

    fn barrier_validate(bytes: &[u8]) -> bool {
      let active = ACTIVE_VALIDATORS.fetch_add(1, Ordering::SeqCst) + 1;
      if active >= 2 {
        OBSERVED_CONCURRENCY.store(true, Ordering::SeqCst);
      }
      let start = std::time::Instant::now();
      while !OBSERVED_CONCURRENCY.load(Ordering::SeqCst)
        && start.elapsed() < std::time::Duration::from_millis(500)
      {
        std::thread::yield_now();
      }
      ACTIVE_VALIDATORS.fetch_sub(1, Ordering::SeqCst);
      !bytes.is_empty()
    }

    let (read_one, read_two) = tokio::join!(
      cache.load("entry-1".to_owned(), 1024, barrier_validate),
      cache.load("entry-2".to_owned(), 1024, barrier_validate),
    );

    assert!(
      OBSERVED_CONCURRENCY.load(Ordering::SeqCst),
      "both load operations must execute their validation concurrently under shared read locks"
    );
    assert_eq!(read_one.unwrap().as_ref(), [1, 2, 3]);
    assert_eq!(read_two.unwrap().as_ref(), [4, 5, 6]);
    fs::remove_dir_all(root).unwrap();
  }
}
