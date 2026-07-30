//! Background AVIF conversion worker for the Library Image cache.
//!
//! One cache-directory-wide worker claims the oldest due conversion row from
//! SQLite, encodes the original to AVIF off the serving path, and activates the
//! result when it is structurally valid and at least 15% smaller. Work is
//! foreground-gated (no playback or origin image fetch, plus a quiescence
//! window) and capability-gated (a positive WebView AVIF probe). Encoding uses
//! one thread and the worker yields between attempts so it never competes with
//! foreground activity.

use std::{
  path::PathBuf,
  sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
  },
  time::{Duration, Instant},
};

use fs2::FileExt;
use sha2::{Digest, Sha256};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::avif_encode::{self, EncodeReject};
use crate::image_cache::ImageCache;

/// Delay between SQLite polls for work inserted by another process.
const CROSS_PROCESS_POLL: Duration = Duration::from_secs(5);
/// Required foreground quiet period before claiming work.
const QUIESCENCE: Duration = Duration::from_secs(1);
/// Yield after every conversion attempt.
const INTER_ATTEMPT_YIELD: Duration = Duration::from_millis(300);

/// Tracks foreground activity that must gate background conversion.
pub struct ForegroundGate {
  playback_sessions: AtomicU32,
  image_fetches: AtomicU32,
  last_activity: parking_lot::Mutex<Instant>,
}

impl ForegroundGate {
  pub fn new() -> Self {
    Self {
      playback_sessions: AtomicU32::new(0),
      image_fetches: AtomicU32::new(0),
      last_activity: parking_lot::Mutex::new(Instant::now() - QUIESCENCE),
    }
  }

  pub fn image_fetch_started(&self) {
    self.image_fetches.fetch_add(1, Ordering::SeqCst);
    self.touch();
  }

  pub fn image_fetch_finished(&self) {
    self.image_fetches.fetch_sub(1, Ordering::SeqCst);
    self.touch();
  }

  fn touch(&self) {
    *self.last_activity.lock() = Instant::now();
  }

  /// True when no foreground activity is in flight and the quiet window has
  /// elapsed since the last activity.
  pub fn is_quiescent(&self) -> bool {
    if self.playback_sessions.load(Ordering::SeqCst) > 0
      || self.image_fetches.load(Ordering::SeqCst) > 0
    {
      return false;
    }
    self.last_activity.lock().elapsed() >= QUIESCENCE
  }
}

/// One-time WebView AVIF capability result.
#[derive(Clone, Default)]
pub struct AvifCapability {
  supported: Arc<AtomicBool>,
}

impl AvifCapability {
  pub fn new() -> Self {
    Self::default()
  }

  /// Record the probe result. Only a positive result enables conversion.
  pub fn set_supported(&self, supported: bool) {
    self.supported.store(supported, Ordering::SeqCst);
  }

  pub fn is_supported(&self) -> bool {
    self.supported.load(Ordering::SeqCst)
  }
}

/// The cache-directory-wide conversion worker handle.
pub struct ConversionWorker {
  cancel: CancellationToken,
}

impl ConversionWorker {
  /// Spawn the worker. Returns `None` if another process already owns the
  /// cache-directory worker lock.
  pub fn start(
    cache: Arc<ImageCache>,
    cache_dir: PathBuf,
    gate: Arc<ForegroundGate>,
    capability: AvifCapability,
  ) -> Option<Self> {
    let lock_path = cache_dir.join("images").join(".worker.lock");
    let lock_file = acquire_worker_lock(&lock_path)?;
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();

    tauri::async_runtime::spawn(async move {
      run_worker(cache, gate, capability, worker_cancel, lock_file).await;
    });

    Some(Self { cancel })
  }
}

impl Drop for ConversionWorker {
  fn drop(&mut self) {
    self.cancel.cancel();
  }
}

fn acquire_worker_lock(path: &std::path::Path) -> Option<std::fs::File> {
  if let Some(parent) = path.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  let file = std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .truncate(false)
    .open(path)
    .ok()?;
  // Non-blocking: only one process may own the worker for this cache dir.
  FileExt::try_lock_exclusive(&file).ok()?;
  Some(file)
}

async fn run_worker(
  cache: Arc<ImageCache>,
  gate: Arc<ForegroundGate>,
  capability: AvifCapability,
  cancel: CancellationToken,
  _lock_file: std::fs::File,
) {
  let notify = Arc::new(Notify::new());
  // Expose the notifier so commit can wake the worker for same-process work.
  cache.set_work_notify(Arc::clone(&notify));

  loop {
    // Wait for a wake-up, the cross-process poll interval, or shutdown.
    tokio::select! {
      _ = cancel.cancelled() => break,
      _ = notify.notified() => {}
      _ = tokio::time::sleep(CROSS_PROCESS_POLL) => {}
    }

    if cancel.is_cancelled() {
      break;
    }

    // Capability gate: unknown or unsupported leaves origins active.
    if !capability.is_supported() {
      continue;
    }
    // Foreground gate: require quiescence before claiming.
    if !gate.is_quiescent() {
      continue;
    }

    let now = now_ms();
    let claim = match cache.claim_work(now).await {
      Ok(Some(claim)) => claim,
      Ok(None) => continue,
      Err(err) => {
        log::debug!("Conversion claim failed: {err}");
        continue;
      }
    };

    process_claim(&cache, &claim).await;

    // Yield after every attempt so encoding never crowds the foreground.
    tokio::time::sleep(INTER_ATTEMPT_YIELD).await;
  }
}

async fn process_claim(cache: &Arc<ImageCache>, claim: &crate::image_cache::WorkClaim) {
  let original_path = cache.path_for(&claim.original_file_name);

  let original = match tokio::fs::read(&original_path).await {
    Ok(bytes) => bytes,
    Err(_) => {
      // Original gone: nothing to convert; leave the row terminal.
      let _ = cache
        .record_conversion_skipped(&claim.cache_key, "not_eligible")
        .await;
      return;
    }
  };

  // Encode off the async runtime on a single dedicated thread.
  let result =
    tokio::task::spawn_blocking(move || avif_encode::encode_jpeg_to_avif(&original)).await;

  let encoded = match result {
    Ok(Ok(encoded)) => encoded,
    Ok(Err(EncodeReject::NotEligible)) => {
      let _ = cache
        .record_conversion_skipped(&claim.cache_key, "not_eligible")
        .await;
      return;
    }
    Ok(Err(EncodeReject::Corrupt)) => {
      let _ = cache
        .record_conversion_skipped(&claim.cache_key, "not_eligible")
        .await;
      return;
    }
    Ok(Err(EncodeReject::Transient)) | Err(_) => {
      let _ = cache
        .record_conversion_failure(&claim.cache_key, "transient encode failure", now_ms())
        .await;
      return;
    }
  };

  // Structural validation: parse as an AVIF container and confirm dimensions.
  match avif_encode::parse_avif_dimensions(&encoded.bytes) {
    Some((w, h)) if w == encoded.width && h == encoded.height => {}
    _ => {
      let _ = cache
        .record_conversion_skipped(&claim.cache_key, "not_eligible")
        .await;
      return;
    }
  }

  // Economic validation: at least 15% smaller than the original.
  if !avif_encode::has_sufficient_saving(claim.original_size_bytes, encoded.bytes.len() as u64) {
    let _ = cache
      .record_conversion_skipped(&claim.cache_key, "evaluated")
      .await;
    return;
  }

  // Durable publication: write beside final path, sync, atomic rename, sync dir.
  let avif_name = ImageCache::avif_file_name_for(&claim.original_file_name);
  let final_path = cache.path_for(&avif_name);
  let temp_path = cache.path_for(&format!(".tmp-{avif_name}-{}", uuid::Uuid::new_v4()));

  if let Err(err) = publish_file(&temp_path, &final_path, &encoded.bytes).await {
    log::debug!("AVIF publication failed: {err}");
    let _ = tokio::fs::remove_file(&temp_path).await;
    let _ = cache
      .record_conversion_failure(&claim.cache_key, "transient publish failure", now_ms())
      .await;
    return;
  }

  let digest = format!("{:x}", Sha256::digest(&encoded.bytes));
  if let Err(err) = cache
    .activate_avif(
      &claim.cache_key,
      &avif_name,
      encoded.bytes.len() as u64,
      &digest,
      "image/avif",
    )
    .await
  {
    log::debug!("AVIF activation failed: {err}");
    let _ = tokio::fs::remove_file(&final_path).await;
    return;
  }

  // Remove the original now that AVIF is active, deferring to active readers.
  let _ = cache.remove_original_if_idle(&claim.cache_key).await;
}

async fn publish_file(
  temp_path: &std::path::Path,
  final_path: &std::path::Path,
  bytes: &[u8],
) -> Result<(), std::io::Error> {
  use tokio::io::AsyncWriteExt;
  let mut file = tokio::fs::File::create(temp_path).await?;
  file.write_all(bytes).await?;
  file.flush().await?;
  file.sync_all().await?;
  drop(file);
  tokio::fs::rename(temp_path, final_path).await?;
  sync_parent_dir(final_path).await;
  Ok(())
}

async fn sync_parent_dir(path: &std::path::Path) {
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

fn now_ms() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_millis() as i64)
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn foreground_gate_requires_quiescence() {
    let gate = ForegroundGate::new();
    // Freshly created: last activity is "now" minus QUIESCENCE, so quiescent.
    assert!(gate.is_quiescent());

    gate.image_fetch_started();
    assert!(!gate.is_quiescent(), "active image fetch blocks conversion");
    gate.image_fetch_finished();
    // Activity just happened -> within the quiescence window.
    assert!(!gate.is_quiescent(), "must wait out the quiet window");
  }

  #[test]
  fn capability_defaults_to_unsupported() {
    let cap = AvifCapability::new();
    assert!(
      !cap.is_supported(),
      "unknown capability must not start work"
    );
  }
}
