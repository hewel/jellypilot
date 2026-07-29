use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

#[derive(Clone, Debug)]
pub struct HlsProxyConfig {
  pub cache_max_bytes: u64,
  pub cache_min_free_bytes: u64,
  pub prefetch_ahead: usize,
  #[allow(dead_code)]
  pub prefetch_concurrency: usize,
  pub origin_retries: u32,
  pub deactivate_grace: std::time::Duration,
}

impl Default for HlsProxyConfig {
  fn default() -> Self {
    Self {
      cache_max_bytes: 10 * 1024 * 1024 * 1024,
      cache_min_free_bytes: 2 * 1024 * 1024 * 1024,
      prefetch_ahead: 3,
      prefetch_concurrency: 1,
      origin_retries: 2,
      deactivate_grace: std::time::Duration::from_secs(2),
    }
  }
}

#[cfg(unix)]
fn set_owner_only_dir(path: &Path) {
  use std::os::unix::fs::PermissionsExt;
  let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(unix)]
fn set_owner_only_file(path: &Path) {
  use std::os::unix::fs::PermissionsExt;
  let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_owner_only_dir(_path: &Path) {}

#[cfg(not(unix))]
fn set_owner_only_file(_path: &Path) {}

pub struct StoreManager {
  cache_root: Option<PathBuf>,
  process_nonce: String,
  config: HlsProxyConfig,
  _lock_file: Option<Arc<fs::File>>,
  process_dir: Option<PathBuf>,
  active_readers: Arc<Mutex<HashMap<PathBuf, usize>>>,
  lru: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

pub struct ReaderGuard {
  path: PathBuf,
  active_readers: Arc<Mutex<HashMap<PathBuf, usize>>>,
}

impl Drop for ReaderGuard {
  fn drop(&mut self) {
    let mut guard = self.active_readers.lock();
    if let Some(count) = guard.get_mut(&self.path) {
      if *count <= 1 {
        guard.remove(&self.path);
      } else {
        *count -= 1;
      }
    }
  }
}

impl StoreManager {
  pub fn new(cache_root: Option<PathBuf>, config: HlsProxyConfig) -> Result<Arc<Self>, io::Error> {
    let process_nonce = uuid::Uuid::new_v4().to_string();

    let mut lock_file_opt = None;
    let mut process_dir_opt = None;

    if let Some(root) = &cache_root {
      fs::create_dir_all(root)?;
      set_owner_only_dir(root);

      let process_dir = root.join(&process_nonce);
      fs::create_dir_all(&process_dir)?;
      set_owner_only_dir(&process_dir);

      let lock_path = process_dir.join(".lock");
      let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&lock_path)?;
      set_owner_only_file(&lock_path);

      fs2::FileExt::lock_exclusive(&file)?;
      lock_file_opt = Some(Arc::new(file));
      process_dir_opt = Some(process_dir.clone());

      // Scan sibling process directories for stale locks
      if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
          let path = entry.path();
          if path.is_dir() && path != process_dir {
            let sibling_lock = path.join(".lock");
            // Only directories whose lock can be acquired are stale; a
            // lock-less directory may belong to a live starting process.
            if sibling_lock.exists() {
              if let Ok(file) = fs::OpenOptions::new().write(true).open(&sibling_lock) {
                if fs2::FileExt::try_lock_exclusive(&file).is_ok() {
                  // Stale process directory! Delete session contents first
                  Self::clean_stale_process_dir(&path, &sibling_lock, file);
                }
              }
            }
          }
        }
      }
    }

    Ok(Arc::new(Self {
      cache_root,
      process_nonce,
      config,
      _lock_file: lock_file_opt,
      process_dir: process_dir_opt,
      active_readers: Arc::new(Mutex::new(HashMap::new())),
      lru: Arc::new(Mutex::new(HashMap::new())),
    }))
  }

  fn clean_stale_process_dir(process_dir: &Path, lock_path: &Path, file: fs::File) {
    if let Ok(entries) = fs::read_dir(process_dir) {
      for entry in entries.flatten() {
        let p = entry.path();
        if p != lock_path {
          let result = if p.is_dir() {
            fs::remove_dir_all(&p)
          } else {
            fs::remove_file(&p)
          };
          // NotFound is expected; anything else retries on the next startup
          if let Err(e) = result {
            if e.kind() != io::ErrorKind::NotFound {
              log::warn!("Failed to remove stale HLS cache entry: {}", e);
            }
          }
        }
      }
    }
    let _ = fs2::FileExt::unlock(&file);
    drop(file);
    if let Err(e) = fs::remove_file(lock_path) {
      if e.kind() != io::ErrorKind::NotFound {
        log::warn!("Failed to remove stale HLS lock file: {}", e);
      }
    }
    if let Err(e) = fs::remove_dir(process_dir) {
      if e.kind() != io::ErrorKind::NotFound {
        log::warn!("Failed to remove stale HLS process directory: {}", e);
      }
    }
  }

  pub fn cache_root(&self) -> Option<&Path> {
    self.cache_root.as_deref()
  }

  #[allow(dead_code)]
  pub fn process_nonce(&self) -> &str {
    &self.process_nonce
  }

  pub fn config(&self) -> &HlsProxyConfig {
    &self.config
  }

  pub fn create_session_dir(&self, session_nonce: &str) -> Option<PathBuf> {
    if let Some(proc_dir) = &self.process_dir {
      let session_dir = proc_dir.join(session_nonce);
      if fs::create_dir_all(&session_dir).is_ok() {
        set_owner_only_dir(&session_dir);
        return Some(session_dir);
      }
    }
    None
  }

  pub fn session_dir(&self, session_nonce: &str) -> Option<PathBuf> {
    self.process_dir.as_ref().map(|p| p.join(session_nonce))
  }

  pub fn part_path(&self, session_nonce: &str, resource_id: &str) -> Option<PathBuf> {
    self
      .session_dir(session_nonce)
      .map(|p| p.join(format!("{}.part", resource_id)))
  }

  pub fn bin_path(&self, session_nonce: &str, resource_id: &str) -> Option<PathBuf> {
    self
      .session_dir(session_nonce)
      .map(|p| p.join(format!("{}.bin", resource_id)))
  }

  pub fn record_read(&self, path: &Path) {
    self.lru.lock().insert(path.to_path_buf(), Instant::now());
  }

  pub fn register_reader(&self, path: &Path) -> ReaderGuard {
    *self
      .active_readers
      .lock()
      .entry(path.to_path_buf())
      .or_insert(0) += 1;
    ReaderGuard {
      path: path.to_path_buf(),
      active_readers: self.active_readers.clone(),
    }
  }

  pub fn check_space_and_admit(&self, content_length: Option<u64>, pinned: &[PathBuf]) -> bool {
    let root = match &self.cache_root {
      Some(r) => r,
      None => return false,
    };

    let len = match content_length {
      Some(l) => l,
      None => return false,
    };

    let available = fs2::available_space(root).unwrap_or(0);
    let current_cached = self.calculate_total_cached_bytes();

    if current_cached + len > self.config.cache_max_bytes
      || available < self.config.cache_min_free_bytes + len
    {
      self.evict_lru(len, pinned);
    }

    let available_after = fs2::available_space(root).unwrap_or(0);
    let current_after = self.calculate_total_cached_bytes();

    current_after + len <= self.config.cache_max_bytes
      && available_after >= self.config.cache_min_free_bytes + len
  }

  fn calculate_total_cached_bytes(&self) -> u64 {
    let mut total = 0;
    if let Some(proc_dir) = &self.process_dir {
      if let Ok(sessions) = fs::read_dir(proc_dir) {
        for s in sessions.flatten() {
          if s.path().is_dir() {
            if let Ok(files) = fs::read_dir(s.path()) {
              for f in files.flatten() {
                if let Ok(meta) = f.metadata() {
                  total += meta.len();
                }
              }
            }
          }
        }
      }
    }
    total
  }

  fn evict_lru(&self, needed_bytes: u64, pinned: &[PathBuf]) {
    let proc_dir = match &self.process_dir {
      Some(p) => p,
      None => return,
    };

    let active_readers_guard = self.active_readers.lock();
    let lru_guard = self.lru.lock();

    let mut candidates: Vec<(PathBuf, Instant, u64)> = Vec::new();

    if let Ok(sessions) = fs::read_dir(proc_dir) {
      for s in sessions.flatten() {
        if s.path().is_dir() {
          if let Ok(files) = fs::read_dir(s.path()) {
            for f in files.flatten() {
              let path = f.path();
              if path.extension().and_then(|e| e.to_str()) == Some("bin") {
                if pinned.iter().any(|p| p == &path) {
                  continue;
                }
                if active_readers_guard.get(&path).copied().unwrap_or(0) > 0 {
                  continue;
                }
                let last_read = lru_guard.get(&path).copied().unwrap_or_else(Instant::now);
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                candidates.push((path, last_read, len));
              }
            }
          }
        }
      }
    }

    drop(lru_guard);
    drop(active_readers_guard);

    candidates.sort_by_key(|c| c.1); // Oldest first

    let mut freed = 0;
    for (path, _, len) in candidates {
      if freed >= needed_bytes {
        break;
      }
      if fs::remove_file(&path).is_ok() {
        self.lru.lock().remove(&path);
        freed += len;
      }
    }
  }

  pub fn cleanup_session(&self, session_nonce: &str) {
    if let Some(proc_dir) = &self.process_dir {
      let session_dir = proc_dir.join(session_nonce);
      let _ = fs::remove_dir_all(&session_dir);
    }
  }
}

impl Drop for StoreManager {
  fn drop(&mut self) {
    if let Some(proc_dir) = &self.process_dir {
      let lock_path = proc_dir.join(".lock");
      if let Ok(entries) = fs::read_dir(proc_dir) {
        for entry in entries.flatten() {
          let p = entry.path();
          if p != lock_path {
            let _ = if p.is_dir() {
              fs::remove_dir_all(&p)
            } else {
              fs::remove_file(&p)
            };
          }
        }
      }
      let _ = fs::remove_file(&lock_path);
      let _ = fs::remove_dir(proc_dir);
    }
  }
}
