use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

use tokio::sync::Notify;

pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CACHED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_CACHED_ENTRIES: usize = 256;
pub const MAX_ACTIVE_LOADS: usize = 4;
pub const MAX_ACTIVE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_QUEUED_LOADS: usize = 64;
pub const DECODE_PIXEL_BUFFER_RESERVATIONS: usize = 2;

#[derive(Clone, Copy)]
pub struct ArtworkLimits {
  pub max_response_bytes: usize,
  pub max_decoded_bytes: usize,
  pub max_cached_bytes: usize,
  pub max_cached_entries: usize,
  pub max_active_loads: usize,
  pub max_active_bytes: usize,
  pub max_queued_loads: usize,
}

impl Default for ArtworkLimits {
  fn default() -> Self {
    Self {
      max_response_bytes: MAX_RESPONSE_BYTES,
      max_decoded_bytes: MAX_DECODED_BYTES,
      max_cached_bytes: MAX_CACHED_BYTES,
      max_cached_entries: MAX_CACHED_ENTRIES,
      max_active_loads: MAX_ACTIVE_LOADS,
      max_active_bytes: MAX_ACTIVE_BYTES,
      max_queued_loads: MAX_QUEUED_LOADS,
    }
  }
}

impl ArtworkLimits {
  #[must_use]
  pub fn load_reservation_bytes(self) -> usize {
    self.max_response_bytes.saturating_add(
      self
        .max_decoded_bytes
        .saturating_mul(DECODE_PIXEL_BUFFER_RESERVATIONS),
    )
  }

  #[must_use]
  pub fn normalized(mut self) -> Self {
    self.max_active_loads = self.max_active_loads.max(1);
    self.max_active_bytes = self.max_active_bytes.max(self.load_reservation_bytes());
    self
  }
}

#[derive(Clone, Copy)]
pub struct ArtworkLoadTicket(u64);

impl ArtworkLoadTicket {
  #[must_use]
  pub const fn new(generation: u64) -> Self {
    Self(generation)
  }

  #[must_use]
  pub const fn generation(self) -> u64 {
    self.0
  }
}

pub enum LoadAdmission<T, R> {
  Cached(T),
  Follower(R),
  Leader(u64),
  Cancelled,
}

#[derive(Default)]
pub struct LoadScheduler {
  next_queue_id: u64,
  active_loads: usize,
  active_bytes: usize,
  queue: VecDeque<QueuedEntry>,
}

struct QueuedEntry {
  id: u64,
  notify: Arc<Notify>,
}

impl LoadScheduler {
  #[must_use]
  pub fn enqueue(&mut self) -> (u64, Arc<Notify>) {
    let id = self.next_queue_id;
    self.next_queue_id = self.next_queue_id.wrapping_add(1);
    let notify = Arc::new(Notify::new());
    self.queue.push_back(QueuedEntry {
      id,
      notify: Arc::clone(&notify),
    });
    (id, notify)
  }

  #[must_use]
  pub fn queued_loads(&self) -> usize {
    self.queue.len()
  }
  #[must_use]
  pub const fn active_loads(&self) -> usize {
    self.active_loads
  }

  #[must_use]
  pub const fn active_bytes(&self) -> usize {
    self.active_bytes
  }

  pub fn try_activate(
    &mut self,
    queue_id: u64,
    max_active_loads: usize,
    max_active_bytes: usize,
    reserved_bytes: usize,
  ) -> bool {
    let is_next = self.queue.front().is_some_and(|entry| entry.id == queue_id);
    let bytes_fit = reserved_bytes <= max_active_bytes.saturating_sub(self.active_bytes);
    if !is_next || self.active_loads >= max_active_loads || !bytes_fit {
      return false;
    }

    self.queue.pop_front();
    self.active_loads += 1;
    self.active_bytes += reserved_bytes;
    true
  }

  pub fn release(&mut self, reserved_bytes: usize) {
    if self.active_loads > 0 {
      self.active_loads -= 1;
    }
    self.active_bytes = self.active_bytes.saturating_sub(reserved_bytes);
    self.notify_front();
  }

  pub fn cancel(&mut self, queue_id: u64) {
    let was_front = self.queue.front().is_some_and(|entry| entry.id == queue_id);
    if let Some(index) = self.queue.iter().position(|entry| entry.id == queue_id) {
      self.queue.remove(index);
    }
    if was_front {
      self.notify_front();
    }
  }

  fn notify_front(&self) {
    if let Some(entry) = self.queue.front() {
      entry.notify.notify_one();
    }
  }

  pub fn cancel_queued(&mut self) {
    self.queue.clear();
  }
}

/// Redacted artwork failures suitable for UI state and logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkError {
  RequestRejected,
  FetchFailed,
  OriginRejected,
  UnsupportedContentType,
  AnimatedImageUnsupported,
  ResponseTooLarge,
  EmptyResponse,
  DecodeFailed,
  CacheUnavailable,
  DecodedImageTooLarge,
  UiThreadRequired,
  Cancelled,
  Overloaded,
}

impl fmt::Display for ArtworkError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let message = match self {
      Self::RequestRejected => "artwork reference was rejected",
      Self::FetchFailed => "artwork could not be fetched",
      Self::OriginRejected => "artwork server returned an unusable status",
      Self::UnsupportedContentType => "artwork response was not an image",
      Self::AnimatedImageUnsupported => "animated artwork is not supported",
      Self::ResponseTooLarge => "artwork response exceeded the memory limit",
      Self::EmptyResponse => "artwork response was empty",
      Self::DecodeFailed => "artwork data could not be decoded",
      Self::CacheUnavailable => "artwork disk cache is unavailable",
      Self::DecodedImageTooLarge => "decoded artwork exceeded the memory limit",
      Self::UiThreadRequired => "artwork texture must be created on the GTK thread",
      Self::Cancelled => "artwork loading was cancelled",
      Self::Overloaded => "artwork loader is at capacity",
    };
    formatter.write_str(message)
  }
}

impl std::error::Error for ArtworkError {}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn aggregate_reservation_covers_encoded_and_two_pixel_buffers() {
    let limits = ArtworkLimits::default();

    assert_eq!(
      limits.load_reservation_bytes(),
      limits.max_response_bytes + limits.max_decoded_bytes * DECODE_PIXEL_BUFFER_RESERVATIONS
    );
  }

  #[test]
  fn aggregate_reservation_saturates_on_overflow() {
    let limits = ArtworkLimits {
      max_response_bytes: usize::MAX,
      max_decoded_bytes: usize::MAX,
      ..ArtworkLimits::default()
    };

    assert_eq!(limits.load_reservation_bytes(), usize::MAX);
  }

  #[test]
  fn scheduler_bounds_active_loads_and_aggregate_bytes() {
    let mut scheduler = LoadScheduler::default();
    let (first, _) = scheduler.enqueue();
    let (second, _) = scheduler.enqueue();
    let (third, _) = scheduler.enqueue();

    assert!(scheduler.try_activate(first, 2, 100, 40));
    assert!(scheduler.try_activate(second, 2, 100, 40));
    assert!(!scheduler.try_activate(third, 2, 100, 40));
    scheduler.release(40);
    assert!(scheduler.try_activate(third, 2, 100, 40));
  }

  #[test]
  fn ordinary_home_batch_queues_and_drains_in_fifo_order() {
    let mut scheduler = LoadScheduler::default();
    let queued = (0..48).map(|_| scheduler.enqueue().0).collect::<Vec<_>>();
    let reservation = 40;

    for (index, id) in queued.into_iter().enumerate() {
      assert!(scheduler.try_activate(id, 1, reservation, reservation));
      assert_eq!(scheduler.active_loads, 1, "load {index} should be active");
      assert_eq!(scheduler.active_bytes, reservation);
      scheduler.release(reservation);
    }
    assert_eq!(scheduler.active_loads, 0);
    assert_eq!(scheduler.active_bytes, 0);
    assert_eq!(scheduler.queued_loads(), 0);
  }

  #[test]
  fn errors_are_redacted() {
    let secret = "https://server.invalid/image?api_key=secret";
    let errors = [
      ArtworkError::RequestRejected,
      ArtworkError::FetchFailed,
      ArtworkError::OriginRejected,
      ArtworkError::UnsupportedContentType,
      ArtworkError::AnimatedImageUnsupported,
      ArtworkError::ResponseTooLarge,
      ArtworkError::EmptyResponse,
      ArtworkError::DecodeFailed,
      ArtworkError::CacheUnavailable,
      ArtworkError::DecodedImageTooLarge,
      ArtworkError::UiThreadRequired,
      ArtworkError::Cancelled,
      ArtworkError::Overloaded,
    ];
    for error in errors {
      assert!(!error.to_string().contains(secret));
    }
  }
}
