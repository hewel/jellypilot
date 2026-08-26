//! Authenticated, bounded artwork loading for the GTK frontend.
//!
//! Network work and image decoding return `Send`-safe data. GTK objects are
//! created only when [`DecodedArtwork::texture`] is called on the GTK thread.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jellypilot_media_server::{JellyfinClient, LibraryImageRequest};
use relm4::gtk;
use relm4::gtk::gdk_pixbuf::prelude::{PixbufAnimationExt, PixbufLoaderExt};
use relm4::tokio::sync::{oneshot, watch, Notify};

use crate::artwork_cache::{artwork_cache_key, ArtworkCacheStats, ArtworkDiskCache};

pub(crate) const FALLBACK_ARTWORK_ICON: &str = "image-missing-symbolic";

const MAX_IMAGE_REFERENCE_BYTES: usize = 32 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
// Artwork is displayed at at most 220 logical pixels high today. Four MiB of
// pixels still permits about 1365x768 RGBA, while bounding 48 retained Home
// textures to about 192 MiB. Larger originals are downscaled by PixbufLoader
// at `size-prepared`, before its destination pixel buffer is allocated.
const MAX_DECODED_BYTES: usize = 4 * 1024 * 1024;
const MAX_CACHED_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHED_ENTRIES: usize = 256;
const MAX_ACTIVE_LOADS: usize = 4;
const MAX_ACTIVE_BYTES: usize = 64 * 1024 * 1024;
// One Home render currently schedules at most 48 unique images. Keeping a
// full extra active-load margin avoids dropping ordinary views while bounding
// queued reference metadata under pathological callers.
const MAX_QUEUED_LOADS: usize = 64;
const DECODE_INPUT_CHUNK_BYTES: usize = 16 * 1024;
// At peak, a decoder may retain both its mutable pixbuf and the immutable
// GBytes snapshot returned to the UI. The encoded response is reserved too.
const DECODE_PIXEL_BUFFER_RESERVATIONS: usize = 2;
// GdkPixbuf emits 8-bit RGB or RGBA; four bytes per pixel also upper-bounds
// the padding in an RGB rowstride.
const PIXBUF_ALLOCATION_BYTES_PER_PIXEL: usize = 4;

type LoadResult = Result<DecodedArtwork, ArtworkError>;
type FetchResult = Result<ArtworkBytes, ArtworkError>;

/// Loads signed media-server image references without exposing their origin.
///
/// Clone this adapter behind an [`Arc`] when several Relm4 commands need it.
/// Concurrent requests for the same opaque reference share one origin fetch.
pub(crate) struct ArtworkAdapter {
  state: Arc<Mutex<AdapterState>>,
  generation_sender: watch::Sender<u64>,
  limits: ArtworkLimits,
  disk_cache: ArtworkDiskCache,
}

#[derive(Clone, Copy)]
struct ArtworkLimits {
  max_response_bytes: usize,
  max_decoded_bytes: usize,
  max_cached_bytes: usize,
  max_cached_entries: usize,
  max_active_loads: usize,
  max_active_bytes: usize,
  max_queued_loads: usize,
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
  fn load_reservation_bytes(self) -> usize {
    self.max_response_bytes.saturating_add(
      self
        .max_decoded_bytes
        .saturating_mul(DECODE_PIXEL_BUFFER_RESERVATIONS),
    )
  }

  fn normalized(mut self) -> Self {
    self.max_active_loads = self.max_active_loads.max(1);
    self.max_active_bytes = self.max_active_bytes.max(self.load_reservation_bytes());
    self
  }
}

impl Default for ArtworkAdapter {
  fn default() -> Self {
    Self::with_limits(ArtworkLimits::default())
  }
}

impl ArtworkAdapter {
  fn with_limits(limits: ArtworkLimits) -> Self {
    Self::with_limits_and_disk_cache(limits, ArtworkDiskCache::default())
  }

  fn with_limits_and_disk_cache(limits: ArtworkLimits, disk_cache: ArtworkDiskCache) -> Self {
    let limits = limits.normalized();
    let (generation_sender, _) = watch::channel(0);
    Self {
      state: Arc::new(Mutex::new(AdapterState {
        generation: 0,
        cache: ArtworkCache::new(limits.max_cached_bytes, limits.max_cached_entries),
        in_flight: HashMap::new(),
        scheduler: LoadScheduler::default(),
      })),
      generation_sender,
      limits,
      disk_cache,
    }
  }

  /// Fetches and decodes an opaque, signed image reference.
  ///
  /// The returned value contains only raw pixels and can cross Relm4's command
  /// boundary. Call [`DecodedArtwork::texture`] from `update_cmd` before
  /// assigning it to a `gtk::Picture`.
  ///
  /// # Errors
  ///
  /// Returns a redacted [`ArtworkError`] when authorization, transport,
  /// response bounds, or decoding fails. Error values never contain the image
  /// reference, origin URL, response body, or authentication details.
  #[cfg(test)]
  async fn load(
    &self,
    client: &JellyfinClient,
    image_id: &str,
  ) -> Result<DecodedArtwork, ArtworkError> {
    let ticket = self.ticket();
    self.load_with_ticket(client, image_id, ticket).await
  }

  pub(crate) fn ticket(&self) -> ArtworkLoadTicket {
    ArtworkLoadTicket(self.lock_state().generation)
  }

  pub(crate) fn cached(&self, image_id: &str) -> Option<DecodedArtwork> {
    self.lock_state().cache.get(image_id)
  }

  pub(crate) fn set_disk_cache_enabled(&self, enabled: bool) {
    self.disk_cache.set_enabled(enabled);
  }

  pub(crate) async fn disk_cache_stats(&self) -> Result<ArtworkCacheStats, ArtworkError> {
    self
      .disk_cache
      .stats()
      .await
      .map_err(|_| ArtworkError::CacheUnavailable)
  }

  pub(crate) async fn clear_disk_cache(&self) -> Result<(), ArtworkError> {
    self
      .disk_cache
      .clear()
      .await
      .map_err(|_| ArtworkError::CacheUnavailable)
  }

  pub(crate) async fn load_with_ticket(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    ticket: ArtworkLoadTicket,
  ) -> Result<DecodedArtwork, ArtworkError> {
    validate_image_reference(image_id)?;
    // This validation deliberately precedes cache admission: signed opaque
    // references are authorized against the current client session on every
    // call, including decoded cache hits.
    let request = client
      .library()
      .image_request(image_id)
      .map_err(|_| ArtworkError::RequestRejected)?;
    let key = Arc::<str>::from(image_id);
    let disk_key = artwork_cache_key(request.server_url(), request.origin_url());
    let mut generation = self.generation_sender.subscribe();
    let load_generation = ticket.0;
    match self.admit(Arc::clone(&key), load_generation) {
      LoadAdmission::Cached(artwork) => Ok(artwork),
      LoadAdmission::Follower(receiver) => wait_for_follower(receiver, &mut generation).await,
      LoadAdmission::Leader(load_generation) => {
        let pending = PendingLoad::new(self, Arc::clone(&key), load_generation);
        let permit = self
          .acquire_load_permit(load_generation, &mut generation)
          .await?;
        let result = self
          .fetch_and_decode(client, &request, disk_key, permit, &mut generation)
          .await;
        pending.complete(&result);
        result
      }
      LoadAdmission::Cancelled => Err(ArtworkError::Cancelled),
    }
  }

  /// Cancels queued and network artwork work from the previous UI view.
  ///
  /// Decodes already running on the blocking pool finish in the background and
  /// retain their aggregate-memory permit until they actually stop.
  pub(crate) fn cancel_pending(&self) {
    self.advance_generation(false);
  }

  /// Cancels pending artwork work and clears decoded data from the old session.
  pub(crate) fn reset_session(&self) {
    self.advance_generation(true);
  }

  fn advance_generation(&self, clear_cache: bool) {
    let (generation, waiters) = {
      let mut state = self.lock_state();
      state.generation = state.generation.wrapping_add(1);
      let generation = state.generation;
      let waiters = state.cancel_stale(clear_cache);
      (generation, waiters)
    };
    let _ = self.generation_sender.send_replace(generation);
    notify_cancelled(waiters);
  }

  fn admit(&self, key: Arc<str>, generation: u64) -> LoadAdmission {
    self.lock_state().admit(key, generation)
  }

  async fn acquire_load_permit(
    &self,
    load_generation: u64,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<LoadPermit, ArtworkError> {
    let queued = QueuedLoad::new(self, load_generation)?;
    loop {
      if self.try_activate(queued.id()) {
        return Ok(queued.activate(self.limits.load_reservation_bytes()));
      }
      relm4::tokio::select! {
        () = queued.wait() => {}
        changed = generation.changed() => {
          let _ = changed;
          return Err(ArtworkError::Cancelled);
        }
      }
    }
  }

  fn try_activate(&self, queue_id: u64) -> bool {
    self.lock_state().scheduler.try_activate(
      queue_id,
      self.limits.max_active_loads,
      self.limits.max_active_bytes,
      self.limits.load_reservation_bytes(),
    )
  }

  async fn fetch_and_decode(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
    disk_key: String,
    permit: LoadPermit,
    generation: &mut watch::Receiver<u64>,
  ) -> LoadResult {
    let bytes = relm4::tokio::select! {
      result = self.original_bytes(client, request, disk_key) => result?,
      changed = generation.changed() => {
        let _ = changed;
        return Err(ArtworkError::Cancelled);
      }
    };
    let decoded_limit = self.limits.max_decoded_bytes;
    let decode = relm4::spawn_blocking(move || {
      // Keep aggregate admission until the blocking decode really stops. A
      // cancelled caller may drop its join handle while this closure runs.
      let _permit = permit;
      decode_pixels(bytes, decoded_limit)
    });
    relm4::tokio::select! {
      result = decode => result.map_err(|_| ArtworkError::DecodeFailed)?,
      changed = generation.changed() => {
        let _ = changed;
        Err(ArtworkError::Cancelled)
      }
    }
  }

  async fn original_bytes(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
    disk_key: String,
  ) -> FetchResult {
    if let Some(bytes) = self
      .disk_cache
      .load(
        disk_key.clone(),
        self.limits.max_response_bytes,
        validate_disk_artwork,
      )
      .await
    {
      return Ok(ArtworkBytes(bytes));
    }
    let bytes = self.fetch_uncached(client, request).await?;
    // Undecodable content (e.g. animated images) must not enter the disk
    // cache: it can never be served and would churn store → validate-delete →
    // refetch on every view.
    if validate_static_image_container(bytes.0.as_ref()).is_ok() {
      let disk_cache = self.disk_cache.clone();
      let disk_bytes = Arc::clone(&bytes.0);
      relm4::spawn(async move {
        disk_cache.store(disk_key, disk_bytes).await;
      });
    }
    Ok(bytes)
  }

  async fn fetch_uncached(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
  ) -> FetchResult {
    let mut response = client
      .library()
      .fetch_image(request)
      .await
      .map_err(|_| ArtworkError::FetchFailed)?;

    let content_type = match response.headers().get("content-type") {
      Some(value) => Some(
        value
          .to_str()
          .map_err(|_| ArtworkError::UnsupportedContentType)?,
      ),
      None => None,
    };
    validate_response_metadata(
      response.status().is_success(),
      response.content_length(),
      content_type,
      self.limits.max_response_bytes,
    )?;

    let capacity = response
      .content_length()
      .and_then(|length| usize::try_from(length).ok())
      .unwrap_or(0);
    let mut body = Vec::with_capacity(capacity);
    while let Some(chunk) = response
      .chunk()
      .await
      .map_err(|_| ArtworkError::FetchFailed)?
    {
      append_body_chunk(&mut body, &chunk, self.limits.max_response_bytes)?;
    }
    if body.is_empty() {
      return Err(ArtworkError::EmptyResponse);
    }

    Ok(ArtworkBytes(Arc::from(body)))
  }

  fn finish_pending(&self, key: Arc<str>, generation: u64, result: &LoadResult) {
    let waiters = {
      let mut state = self.lock_state();
      if state.generation == generation {
        let waiters = state
          .in_flight
          .remove(key.as_ref())
          .map(|load| load.waiters)
          .unwrap_or_default();
        if let Ok(artwork) = result {
          state.cache.insert(Arc::clone(&key), artwork.clone());
        }
        waiters
      } else {
        Vec::new()
      }
    };

    for waiter in waiters {
      let _ = waiter.send(result.clone());
    }
  }

  fn cancel_queued_load(&self, queue_id: u64) {
    self.lock_state().scheduler.cancel(queue_id);
  }

  fn lock_state(&self) -> MutexGuard<'_, AdapterState> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }
}

#[derive(Clone, Copy)]
pub(crate) struct ArtworkLoadTicket(u64);

struct AdapterState {
  generation: u64,
  cache: ArtworkCache,
  in_flight: HashMap<Arc<str>, InFlightLoad>,
  scheduler: LoadScheduler,
}

impl AdapterState {
  fn admit(&mut self, key: Arc<str>, generation: u64) -> LoadAdmission {
    if generation != self.generation {
      return LoadAdmission::Cancelled;
    }
    if let Some(artwork) = self.cache.get(key.as_ref()) {
      return LoadAdmission::Cached(artwork);
    }

    if let Some(load) = self.in_flight.get_mut(key.as_ref()) {
      if load.generation != generation {
        return LoadAdmission::Cancelled;
      }
      let (sender, receiver) = oneshot::channel();
      load.waiters.push(sender);
      return LoadAdmission::Follower(receiver);
    }

    self.in_flight.insert(
      key,
      InFlightLoad {
        generation,
        waiters: Vec::new(),
      },
    );
    LoadAdmission::Leader(generation)
  }

  fn cancel_stale(&mut self, clear_cache: bool) -> Vec<oneshot::Sender<LoadResult>> {
    if clear_cache {
      self.cache.clear();
    }
    self.scheduler.cancel_queued();
    self
      .in_flight
      .drain()
      .flat_map(|(_, load)| load.waiters)
      .collect()
  }
}

struct InFlightLoad {
  generation: u64,
  waiters: Vec<oneshot::Sender<LoadResult>>,
}

enum LoadAdmission {
  Cached(DecodedArtwork),
  Follower(oneshot::Receiver<LoadResult>),
  Leader(u64),
  Cancelled,
}

async fn wait_for_follower(
  receiver: oneshot::Receiver<LoadResult>,
  generation: &mut watch::Receiver<u64>,
) -> LoadResult {
  relm4::tokio::select! {
    result = receiver => result.unwrap_or(Err(ArtworkError::Cancelled)),
    changed = generation.changed() => {
      let _ = changed;
      Err(ArtworkError::Cancelled)
    }
  }
}

fn notify_cancelled(waiters: Vec<oneshot::Sender<LoadResult>>) {
  for waiter in waiters {
    let _ = waiter.send(Err(ArtworkError::Cancelled));
  }
}

#[derive(Default)]
struct LoadScheduler {
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
  fn enqueue(&mut self) -> (u64, Arc<Notify>) {
    let id = self.next_queue_id;
    self.next_queue_id = self.next_queue_id.wrapping_add(1);
    let notify = Arc::new(Notify::new());
    self.queue.push_back(QueuedEntry {
      id,
      notify: Arc::clone(&notify),
    });
    (id, notify)
  }

  fn try_activate(
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

  fn release(&mut self, reserved_bytes: usize) {
    if self.active_loads > 0 {
      self.active_loads -= 1;
    }
    self.active_bytes = self.active_bytes.saturating_sub(reserved_bytes);
    self.notify_front();
  }

  fn cancel(&mut self, queue_id: u64) {
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

  fn cancel_queued(&mut self) {
    self.queue.clear();
  }
}

struct QueuedLoad<'a> {
  adapter: &'a ArtworkAdapter,
  id: Option<u64>,
  notify: Arc<Notify>,
}

impl<'a> QueuedLoad<'a> {
  fn new(adapter: &'a ArtworkAdapter, generation: u64) -> Result<Self, ArtworkError> {
    let (id, notify) = {
      let mut state = adapter.lock_state();
      if state.generation != generation {
        return Err(ArtworkError::Cancelled);
      }
      if state.scheduler.queue.len() >= adapter.limits.max_queued_loads {
        return Err(ArtworkError::Overloaded);
      }
      state.scheduler.enqueue()
    };
    Ok(Self {
      adapter,
      id: Some(id),
      notify,
    })
  }

  fn id(&self) -> u64 {
    self.id.unwrap_or_default()
  }

  async fn wait(&self) {
    self.notify.notified().await;
  }

  fn activate(mut self, reserved_bytes: usize) -> LoadPermit {
    self.id.take();
    LoadPermit {
      state: Arc::clone(&self.adapter.state),
      reserved_bytes,
    }
  }
}

impl Drop for QueuedLoad<'_> {
  fn drop(&mut self) {
    if let Some(id) = self.id.take() {
      self.adapter.cancel_queued_load(id);
    }
  }
}

struct LoadPermit {
  state: Arc<Mutex<AdapterState>>,
  reserved_bytes: usize,
}

impl Drop for LoadPermit {
  fn drop(&mut self) {
    self
      .state
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .scheduler
      .release(self.reserved_bytes);
  }
}

struct PendingLoad<'a> {
  adapter: &'a ArtworkAdapter,
  key: Option<Arc<str>>,
  generation: u64,
}

impl<'a> PendingLoad<'a> {
  fn new(adapter: &'a ArtworkAdapter, key: Arc<str>, generation: u64) -> Self {
    Self {
      adapter,
      key: Some(key),
      generation,
    }
  }

  fn complete(mut self, result: &LoadResult) {
    if let Some(key) = self.key.take() {
      self.adapter.finish_pending(key, self.generation, result);
    }
  }
}

impl Drop for PendingLoad<'_> {
  fn drop(&mut self) {
    if let Some(key) = self.key.take() {
      self
        .adapter
        .finish_pending(key, self.generation, &Err(ArtworkError::Cancelled));
    }
  }
}

#[derive(Clone)]
struct ArtworkBytes(Arc<[u8]>);

/// Raw decoded pixels that are safe to return from a Relm4 command.
#[derive(Clone)]
pub(crate) struct DecodedArtwork {
  width: i32,
  height: i32,
  stride: usize,
  format: PixelFormat,
  pixels: gtk::glib::Bytes,
}

impl DecodedArtwork {
  fn len(&self) -> usize {
    self.pixels.len()
  }

  /// Creates a GDK texture from decoded pixels on the GTK main thread.
  ///
  /// # Errors
  ///
  /// Returns [`ArtworkError::UiThreadRequired`] if GTK is uninitialized or
  /// this method is called from another thread.
  pub(crate) fn texture(&self) -> Result<gtk::gdk::Texture, ArtworkError> {
    if !gtk::is_initialized_main_thread() {
      return Err(ArtworkError::UiThreadRequired);
    }

    let format = match self.format {
      PixelFormat::Rgb => gtk::gdk::MemoryFormat::R8g8b8,
      PixelFormat::Rgba => gtk::gdk::MemoryFormat::R8g8b8a8,
    };
    let texture =
      gtk::gdk::MemoryTexture::new(self.width, self.height, format, &self.pixels, self.stride);
    Ok(gtk::gdk::Texture::from(texture))
  }
}

impl fmt::Debug for DecodedArtwork {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("DecodedArtwork")
      .field("width", &self.width)
      .field("height", &self.height)
      .field("stride", &self.stride)
      .field("format", &self.format)
      .field("pixel_bytes", &self.pixels.len())
      .finish()
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PixelFormat {
  Rgb,
  Rgba,
}

/// Redacted artwork failures suitable for UI state and logs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArtworkError {
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

fn validate_disk_artwork(bytes: &[u8]) -> bool {
  validate_static_image_container(bytes).is_ok()
}

fn validate_image_reference(image_id: &str) -> Result<(), ArtworkError> {
  if image_id.is_empty() || image_id.len() > MAX_IMAGE_REFERENCE_BYTES {
    return Err(ArtworkError::RequestRejected);
  }
  Ok(())
}

fn validate_response_metadata(
  success: bool,
  content_length: Option<u64>,
  content_type: Option<&str>,
  max_response_bytes: usize,
) -> Result<(), ArtworkError> {
  if !success {
    return Err(ArtworkError::OriginRejected);
  }
  if content_length.is_some_and(|length| {
    usize::try_from(length).map_or(true, |length| length > max_response_bytes)
  }) {
    return Err(ArtworkError::ResponseTooLarge);
  }
  if content_type.is_some_and(|value| !is_image_content_type(value)) {
    return Err(ArtworkError::UnsupportedContentType);
  }
  Ok(())
}

fn is_image_content_type(value: &str) -> bool {
  let media_type = value.split(';').next().unwrap_or_default().trim();
  media_type
    .get(..6)
    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("image/"))
    && media_type.len() > 6
}

fn validate_static_image_container(bytes: &[u8]) -> Result<(), ArtworkError> {
  if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
    return Ok(());
  }
  if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
    return if png_contains_animation_control(bytes) {
      Err(ArtworkError::AnimatedImageUnsupported)
    } else {
      Ok(())
    };
  }
  if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
    return if webp_contains_animation(bytes) {
      Err(ArtworkError::AnimatedImageUnsupported)
    } else {
      Ok(())
    };
  }
  if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
    return Err(ArtworkError::AnimatedImageUnsupported);
  }
  Err(ArtworkError::UnsupportedContentType)
}

fn png_contains_animation_control(bytes: &[u8]) -> bool {
  let mut offset = 8usize;
  while let Some(header) = bytes.get(offset..offset.saturating_add(8)) {
    let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let chunk_type = &header[4..8];
    if chunk_type == b"acTL" {
      return true;
    }
    if chunk_type == b"IDAT" || chunk_type == b"IEND" {
      return false;
    }
    let Some(next) = offset
      .checked_add(12)
      .and_then(|base| base.checked_add(length))
    else {
      return false;
    };
    if next > bytes.len() {
      return false;
    }
    offset = next;
  }
  false
}

fn webp_contains_animation(bytes: &[u8]) -> bool {
  let mut offset = 12usize;
  while let Some(header) = bytes.get(offset..offset.saturating_add(8)) {
    let chunk_type = &header[..4];
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if chunk_type == b"ANIM" || chunk_type == b"ANMF" {
      return true;
    }
    let payload_start = offset.saturating_add(8);
    if chunk_type == b"VP8X"
      && bytes
        .get(payload_start)
        .is_some_and(|flags| flags & 0x02 != 0)
    {
      return true;
    }
    let Some(padded_length) = length.checked_add(length % 2) else {
      return false;
    };
    let Some(next) = payload_start.checked_add(padded_length) else {
      return false;
    };
    if next > bytes.len() {
      return false;
    }
    offset = next;
  }
  false
}

fn append_body_chunk(
  body: &mut Vec<u8>,
  chunk: &[u8],
  max_response_bytes: usize,
) -> Result<(), ArtworkError> {
  if body.len() > max_response_bytes || chunk.len() > max_response_bytes.saturating_sub(body.len())
  {
    return Err(ArtworkError::ResponseTooLarge);
  }
  body.extend_from_slice(chunk);
  Ok(())
}

fn decode_pixels(
  bytes: ArtworkBytes,
  max_decoded_bytes: usize,
) -> Result<DecodedArtwork, ArtworkError> {
  validate_static_image_container(bytes.0.as_ref())?;
  let loader = gtk::gdk_pixbuf::PixbufLoader::new();
  let prepared_error = Rc::new(Cell::new(None));
  let signal_error = Rc::clone(&prepared_error);
  let _size_prepared_handler = loader.connect_size_prepared(move |loader, width, height| {
    if let Err(error) = validate_prepared_dimensions(width, height, max_decoded_bytes) {
      signal_error.set(Some(error));
      // `size-prepared` runs before the pixel buffer is allocated. Keep the
      // rejected decode at the loader's smallest valid target size.
      let safe_width = i32::from(width > 0);
      let safe_height = i32::from(height > 0);
      if safe_width > 0 && safe_height > 0 {
        loader.set_size(safe_width, safe_height);
      }
    }
  });

  for chunk in bytes.0.chunks(DECODE_INPUT_CHUNK_BYTES) {
    let write_result = loader.write(chunk);
    if let Some(error) = prepared_error.get() {
      let _ = loader.close();
      return Err(error);
    }
    write_result.map_err(|_| ArtworkError::DecodeFailed)?;
  }

  let close_result = loader.close();
  if let Some(error) = prepared_error.get() {
    return Err(error);
  }
  close_result.map_err(|_| ArtworkError::DecodeFailed)?;
  if loader
    .animation()
    .is_some_and(|animation| !animation.is_static_image())
  {
    return Err(ArtworkError::AnimatedImageUnsupported);
  }
  let pixbuf = loader.pixbuf().ok_or(ArtworkError::DecodeFailed)?;
  if pixbuf.colorspace() != gtk::gdk_pixbuf::Colorspace::Rgb || pixbuf.bits_per_sample() != 8 {
    return Err(ArtworkError::DecodeFailed);
  }

  let format = match (pixbuf.n_channels(), pixbuf.has_alpha()) {
    (3, false) => PixelFormat::Rgb,
    (4, true) => PixelFormat::Rgba,
    _ => return Err(ArtworkError::DecodeFailed),
  };
  let width = usize::try_from(pixbuf.width()).map_err(|_| ArtworkError::DecodeFailed)?;
  let height = usize::try_from(pixbuf.height()).map_err(|_| ArtworkError::DecodeFailed)?;
  let source_stride =
    usize::try_from(pixbuf.rowstride()).map_err(|_| ArtworkError::DecodeFailed)?;
  let channels = match format {
    PixelFormat::Rgb => 3,
    PixelFormat::Rgba => 4,
  };
  let tight_stride = width
    .checked_mul(channels)
    .ok_or(ArtworkError::DecodedImageTooLarge)?;
  let minimum_source_len = source_stride
    .checked_mul(height.saturating_sub(1))
    .and_then(|len| len.checked_add(tight_stride))
    .ok_or(ArtworkError::DecodedImageTooLarge)?;
  if width == 0
    || height == 0
    || source_stride < tight_stride
    || minimum_source_len > max_decoded_bytes
  {
    return Err(ArtworkError::DecodedImageTooLarge);
  }

  let pixels = pixbuf.read_pixel_bytes();
  if pixels.len() < minimum_source_len || pixels.len() > max_decoded_bytes {
    return Err(ArtworkError::DecodeFailed);
  }

  Ok(DecodedArtwork {
    width: pixbuf.width(),
    height: pixbuf.height(),
    stride: source_stride,
    format,
    pixels,
  })
}

fn validate_prepared_dimensions(
  width: i32,
  height: i32,
  max_decoded_bytes: usize,
) -> Result<(), ArtworkError> {
  let width = usize::try_from(width).map_err(|_| ArtworkError::DecodeFailed)?;
  let height = usize::try_from(height).map_err(|_| ArtworkError::DecodeFailed)?;
  if width == 0 || height == 0 {
    return Err(ArtworkError::DecodeFailed);
  }

  let allocation_bytes = pixbuf_allocation_bytes(width, height)?;
  if allocation_bytes > max_decoded_bytes {
    return Err(ArtworkError::DecodedImageTooLarge);
  }
  Ok(())
}

fn pixbuf_allocation_bytes(width: usize, height: usize) -> Result<usize, ArtworkError> {
  width
    .checked_mul(height)
    .and_then(|pixels| pixels.checked_mul(PIXBUF_ALLOCATION_BYTES_PER_PIXEL))
    .ok_or(ArtworkError::DecodedImageTooLarge)
}

struct ArtworkCache {
  entries: HashMap<Arc<str>, CacheEntry>,
  total_bytes: usize,
  clock: u64,
  max_bytes: usize,
  max_entries: usize,
}

struct CacheEntry {
  artwork: DecodedArtwork,
  last_used: u64,
}

impl ArtworkCache {
  fn new(max_bytes: usize, max_entries: usize) -> Self {
    Self {
      entries: HashMap::new(),
      total_bytes: 0,
      clock: 0,
      max_bytes,
      max_entries,
    }
  }

  fn get(&mut self, key: &str) -> Option<DecodedArtwork> {
    let entry = self.entries.get_mut(key)?;
    self.clock = self.clock.saturating_add(1);
    entry.last_used = self.clock;
    Some(entry.artwork.clone())
  }

  fn clear(&mut self) {
    self.entries.clear();
    self.total_bytes = 0;
  }

  fn insert(&mut self, key: Arc<str>, artwork: DecodedArtwork) {
    if self.max_bytes == 0 || self.max_entries == 0 || artwork.len() > self.max_bytes {
      return;
    }

    if let Some(previous) = self.entries.remove(key.as_ref()) {
      self.total_bytes = self.total_bytes.saturating_sub(previous.artwork.len());
    }
    self.clock = self.clock.saturating_add(1);
    self.total_bytes = self.total_bytes.saturating_add(artwork.len());
    self.entries.insert(
      key,
      CacheEntry {
        artwork,
        last_used: self.clock,
      },
    );

    while self.total_bytes > self.max_bytes || self.entries.len() > self.max_entries {
      let Some(oldest) = self
        .entries
        .iter()
        .min_by(|left, right| {
          left
            .1
            .last_used
            .cmp(&right.1.last_used)
            .then_with(|| left.0.cmp(right.0))
        })
        .map(|(key, _)| Arc::clone(key))
      else {
        break;
      };
      if let Some(removed) = self.entries.remove(oldest.as_ref()) {
        self.total_bytes = self.total_bytes.saturating_sub(removed.artwork.len());
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_media_server::{
    image_id_for_url, ImageRefKind, MediaServerProvider, SavedSession,
  };

  fn artwork(data: &[u8]) -> DecodedArtwork {
    DecodedArtwork {
      width: 1,
      height: 1,
      stride: data.len(),
      format: PixelFormat::Rgba,
      pixels: gtk::glib::Bytes::from(data),
    }
  }

  fn begin_leader<'a>(adapter: &'a ArtworkAdapter, key: &str) -> PendingLoad<'a> {
    let key = Arc::<str>::from(key);
    let generation = adapter.lock_state().generation;
    let LoadAdmission::Leader(generation) = adapter.admit(Arc::clone(&key), generation) else {
      panic!("expected a leader admission");
    };
    PendingLoad::new(adapter, key, generation)
  }

  fn activate_permit(adapter: &ArtworkAdapter) -> LoadPermit {
    let generation = adapter.lock_state().generation;
    let queued = QueuedLoad::new(adapter, generation).expect("queue has capacity");
    assert!(adapter.try_activate(queued.id()));
    queued.activate(adapter.limits.load_reservation_bytes())
  }

  fn adopt_session(client: &JellyfinClient, server_url: &str, user_id: &str) {
    client.login().adopt_validated_session(&SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: server_url.to_owned(),
      access_token: format!("token-{user_id}"),
      user_id: user_id.to_owned(),
      user_name: user_id.to_owned(),
      server_name: None,
      device_id: None,
    });
  }

  fn image_id(server_url: &str) -> String {
    image_id_for_url(
      MediaServerProvider::Jellyfin,
      server_url,
      format!("{server_url}/Items/1/Images/Primary"),
      ImageRefKind::Artwork,
    )
    .expect("image reference is valid")
  }

  #[test]
  fn prepared_dimensions_accept_an_image_at_the_exact_allocation_limit() {
    let result = validate_prepared_dimensions(8, 2, 64);

    assert_eq!(result, Ok(()));
  }

  #[test]
  fn prepared_dimensions_reject_an_image_above_the_allocation_limit() {
    let result = validate_prepared_dimensions(9, 2, 64);

    assert_eq!(result, Err(ArtworkError::DecodedImageTooLarge));
  }

  #[test]
  fn pixbuf_allocation_size_rejects_dimension_overflow() {
    let result = pixbuf_allocation_bytes(usize::MAX, 2);

    assert_eq!(result, Err(ArtworkError::DecodedImageTooLarge));
  }

  #[test]
  fn aggregate_reservation_covers_encoded_and_two_pixel_buffers() {
    let limits = ArtworkLimits::default();

    assert_eq!(
      limits.load_reservation_bytes(),
      MAX_RESPONSE_BYTES + (2 * MAX_DECODED_BYTES)
    );
    assert_eq!(
      MAX_ACTIVE_BYTES / limits.load_reservation_bytes(),
      MAX_ACTIVE_LOADS
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
    let reservation = 40;

    assert!(scheduler.try_activate(first, 3, 80, reservation));
    assert!(scheduler.try_activate(second, 3, 80, reservation));
    assert!(!scheduler.try_activate(third, 3, 80, reservation));
    assert_eq!(scheduler.active_loads, 2);
    assert_eq!(scheduler.active_bytes, 80);

    scheduler.release(reservation);
    assert!(scheduler.try_activate(third, 3, 80, reservation));
    assert_eq!(scheduler.active_loads, 2);
    assert_eq!(scheduler.active_bytes, 80);
  }

  #[test]
  fn ordinary_home_batch_queues_and_drains_in_fifo_order() {
    let mut scheduler = LoadScheduler::default();
    let queued = (0..48).map(|_| scheduler.enqueue().0).collect::<Vec<_>>();
    let reservation = 40;

    assert!(scheduler.try_activate(queued[0], 2, 80, reservation));
    assert!(scheduler.try_activate(queued[1], 2, 80, reservation));
    for id in &queued[2..] {
      scheduler.release(reservation);
      assert!(scheduler.try_activate(*id, 2, 80, reservation));
    }
    scheduler.release(reservation);
    scheduler.release(reservation);

    assert_eq!(scheduler.active_loads, 0);
    assert_eq!(scheduler.active_bytes, 0);
    assert!(scheduler.queue.is_empty());
  }

  #[test]
  fn queue_accepts_an_ordinary_48_image_home_and_bounds_pathological_backlog() {
    let adapter = ArtworkAdapter::default();
    let generation = adapter.lock_state().generation;
    let home = (0..48)
      .map(|_| QueuedLoad::new(&adapter, generation).expect("Home queue has capacity"))
      .collect::<Vec<_>>();
    let margin = (48..MAX_QUEUED_LOADS)
      .map(|_| QueuedLoad::new(&adapter, generation).expect("bounded margin has capacity"))
      .collect::<Vec<_>>();

    assert!(matches!(
      QueuedLoad::new(&adapter, generation),
      Err(ArtworkError::Overloaded)
    ));

    drop(home);
    drop(margin);
  }

  #[test]
  fn cancelling_a_generation_removes_stale_backlog_before_current_work() {
    let adapter = ArtworkAdapter::default();
    let stale_generation = adapter.lock_state().generation;
    let stale = (0..MAX_QUEUED_LOADS)
      .map(|_| QueuedLoad::new(&adapter, stale_generation).expect("stale queue has capacity"))
      .collect::<Vec<_>>();

    adapter.cancel_pending();

    let current_generation = adapter.lock_state().generation;
    let current = QueuedLoad::new(&adapter, current_generation).expect("current load is admitted");
    assert!(adapter.try_activate(current.id()));
    let current = current.activate(adapter.limits.load_reservation_bytes());
    drop(stale);
    drop(current);

    let state = adapter.lock_state();
    assert_eq!(state.scheduler.active_loads, 0);
    assert_eq!(state.scheduler.active_bytes, 0);
    assert!(state.scheduler.queue.is_empty());
  }

  #[test]
  fn generation_advance_without_receivers_is_retained_for_later_loads() {
    let adapter = ArtworkAdapter::default();

    adapter.cancel_pending();

    let generation = *adapter.generation_sender.subscribe().borrow();
    assert_eq!(generation, 1);
    assert!(matches!(
      adapter.admit(Arc::from("current"), generation),
      LoadAdmission::Leader(1)
    ));
  }

  #[test]
  fn cancelling_a_queued_load_allows_the_next_unique_load_to_run() {
    let adapter = ArtworkAdapter::default();
    let first = activate_permit(&adapter);
    let second = activate_permit(&adapter);
    let generation = adapter.lock_state().generation;
    let cancelled = QueuedLoad::new(&adapter, generation).expect("queue has capacity");
    let next = QueuedLoad::new(&adapter, generation).expect("queue has capacity");

    assert!(!adapter.try_activate(next.id()));
    drop(cancelled);
    drop(first);
    assert!(adapter.try_activate(next.id()));

    let next = next.activate(adapter.limits.load_reservation_bytes());
    drop(second);
    drop(next);
    let state = adapter.lock_state();
    assert_eq!(state.scheduler.active_loads, 0);
    assert_eq!(state.scheduler.active_bytes, 0);
  }

  #[test]
  fn owned_permit_stays_admitted_until_background_work_drops_it() {
    let adapter = ArtworkAdapter::default();
    let permit = activate_permit(&adapter);
    let (started_sender, started_receiver) = std::sync::mpsc::channel();
    let (finish_sender, finish_receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
      started_sender.send(()).expect("started receiver remains");
      finish_receiver.recv().expect("finish sender remains");
      drop(permit);
    });

    started_receiver.recv().expect("worker starts");
    assert_eq!(adapter.lock_state().scheduler.active_loads, 1);
    finish_sender.send(()).expect("worker remains");
    worker.join().expect("worker does not panic");

    let state = adapter.lock_state();
    assert_eq!(state.scheduler.active_loads, 0);
    assert_eq!(state.scheduler.active_bytes, 0);
  }

  #[test]
  fn in_flight_admission_coalesces_one_decoded_result_and_pixel_storage() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let key = Arc::<str>::from("same");
    let LoadAdmission::Follower(receiver) = adapter.admit(key, 0) else {
      panic!("expected a follower admission");
    };
    let decoded = artwork(&[1, 2, 3, 4]);
    let pixels = decoded.pixels.as_ptr();

    pending.complete(&Ok(decoded));
    let received = receiver
      .blocking_recv()
      .expect("leader sends a result")
      .expect("leader succeeds");

    assert_eq!(received.pixels.as_ptr(), pixels);
    let LoadAdmission::Cached(cached) = adapter.admit(Arc::from("same"), 0) else {
      panic!("expected a decoded cache hit");
    };
    assert_eq!(cached.pixels.as_ptr(), pixels);
  }

  #[test]
  fn cancelled_follower_does_not_cancel_the_shared_leader() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let key = Arc::<str>::from("same");
    let LoadAdmission::Follower(receiver) = adapter.admit(key, 0) else {
      panic!("expected a follower admission");
    };
    drop(receiver);
    pending.complete(&Ok(artwork(&[1, 2, 3, 4])));

    assert!(matches!(
      adapter.admit(Arc::from("same"), 0),
      LoadAdmission::Cached(_)
    ));
  }

  #[test]
  fn cancelled_leader_notifies_followers_and_releases_the_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) = adapter.admit(Arc::from("same"), 0) else {
      panic!("expected a follower admission");
    };

    drop(pending);

    assert!(matches!(
      receiver.blocking_recv().expect("leader sends cancellation"),
      Err(ArtworkError::Cancelled)
    ));
    assert!(matches!(
      adapter.admit(Arc::from("same"), 0),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn completed_error_releases_the_coalescing_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    pending.complete(&Err(ArtworkError::FetchFailed));

    assert!(matches!(
      adapter.admit(Arc::from("same"), 0),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn generation_cancellation_notifies_followers_and_rejects_stale_admission() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) = adapter.admit(Arc::from("same"), 0) else {
      panic!("expected a follower admission");
    };

    adapter.cancel_pending();

    assert!(matches!(
      receiver.blocking_recv().expect("follower is notified"),
      Err(ArtworkError::Cancelled)
    ));
    assert!(matches!(
      adapter.admit(Arc::from("stale"), 0),
      LoadAdmission::Cancelled
    ));
    assert!(matches!(
      adapter.admit(Arc::from("current"), 1),
      LoadAdmission::Leader(1)
    ));
    drop(pending);
  }

  #[test]
  fn reset_session_clears_decoded_cache() {
    let adapter = ArtworkAdapter::default();
    adapter
      .lock_state()
      .cache
      .insert(Arc::from("cached"), artwork(&[1, 2, 3, 4]));

    adapter.reset_session();

    assert!(adapter.lock_state().cache.get("cached").is_none());
  }

  #[test]
  fn ticket_captured_before_reset_cannot_adopt_the_new_generation() {
    let adapter = ArtworkAdapter::default();
    let stale = adapter.ticket();

    adapter.reset_session();

    assert!(matches!(
      adapter.admit(Arc::from("stale"), stale.0),
      LoadAdmission::Cancelled
    ));
    assert!(matches!(
      adapter.admit(Arc::from("current"), adapter.ticket().0),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn cached_artwork_is_revalidated_against_the_current_client_session() {
    let adapter = ArtworkAdapter::default();
    let client = JellyfinClient::new();
    let first_server = "https://first.example.com";
    let reference = image_id(first_server);
    let cached = artwork(&[1, 2, 3, 4]);
    let cached_pixels = cached.pixels.as_ptr();
    adopt_session(&client, first_server, "first-user");
    adapter
      .lock_state()
      .cache
      .insert(Arc::from(reference.as_str()), cached);
    let runtime = relm4::tokio::runtime::Builder::new_current_thread()
      .build()
      .expect("runtime builds");

    let accepted = runtime
      .block_on(adapter.load(&client, &reference))
      .expect("current session accepts cache hit");
    assert_eq!(accepted.pixels.as_ptr(), cached_pixels);

    adopt_session(&client, "https://second.example.com", "second-user");
    assert!(matches!(
      runtime.block_on(adapter.load(&client, &reference)),
      Err(ArtworkError::RequestRejected)
    ));
  }

  #[test]
  fn cache_evicts_least_recently_used_entry_when_entry_limit_is_reached() {
    let mut cache = ArtworkCache::new(16, 2);
    cache.insert(Arc::from("a"), artwork(&[1]));
    cache.insert(Arc::from("b"), artwork(&[2]));
    let _ = cache.get("a");
    cache.insert(Arc::from("c"), artwork(&[3]));

    assert!(cache.get("b").is_none());
  }

  #[test]
  fn cache_evicts_oldest_entry_when_byte_limit_is_reached() {
    let mut cache = ArtworkCache::new(3, 3);
    cache.insert(Arc::from("a"), artwork(&[1, 2]));
    cache.insert(Arc::from("b"), artwork(&[3, 4]));

    assert!(cache.get("a").is_none());
  }

  #[test]
  fn cache_does_not_store_an_entry_larger_than_its_total_limit() {
    let mut cache = ArtworkCache::new(2, 2);
    cache.insert(Arc::from("large"), artwork(&[1, 2, 3]));

    assert!(cache.get("large").is_none());
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
      ArtworkError::DecodedImageTooLarge,
      ArtworkError::UiThreadRequired,
      ArtworkError::Cancelled,
      ArtworkError::Overloaded,
    ];

    for error in errors {
      assert!(!error.to_string().contains(secret));
    }
  }

  #[test]
  fn response_metadata_rejects_non_success_status() {
    let result = validate_response_metadata(false, Some(1), Some("image/png"), 10);

    assert_eq!(result, Err(ArtworkError::OriginRejected));
  }

  #[test]
  fn response_metadata_rejects_oversized_declared_body() {
    let result = validate_response_metadata(true, Some(11), Some("image/png"), 10);

    assert_eq!(result, Err(ArtworkError::ResponseTooLarge));
  }

  #[test]
  fn response_metadata_accepts_image_type_with_parameters() {
    let result = validate_response_metadata(true, Some(10), Some("Image/WebP; charset=binary"), 10);

    assert_eq!(result, Ok(()));
  }

  #[test]
  fn response_metadata_rejects_explicit_non_image_type() {
    let result = validate_response_metadata(true, Some(10), Some("text/html"), 10);

    assert_eq!(result, Err(ArtworkError::UnsupportedContentType));
  }

  #[test]
  fn static_artwork_container_preflight_accepts_jpeg_png_and_webp() {
    assert_eq!(validate_static_image_container(&[0xff, 0xd8, 0xff]), Ok(()));
    assert_eq!(
      validate_static_image_container(b"\x89PNG\r\n\x1a\n"),
      Ok(())
    );
    assert_eq!(
      validate_static_image_container(b"RIFF\x00\x00\x00\x00WEBPVP8 \x00\x00\x00\x00"),
      Ok(())
    );
  }

  #[test]
  fn artwork_container_preflight_rejects_animated_formats_before_decode() {
    assert_eq!(
      validate_static_image_container(b"GIF89a"),
      Err(ArtworkError::AnimatedImageUnsupported)
    );
    assert_eq!(
      validate_static_image_container(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00acTL\x00\x00\x00\x00"),
      Err(ArtworkError::AnimatedImageUnsupported)
    );
    assert_eq!(
      validate_static_image_container(b"RIFF\x00\x00\x00\x00WEBPANIM\x00\x00\x00\x00"),
      Err(ArtworkError::AnimatedImageUnsupported)
    );
  }

  #[test]
  fn body_collector_accepts_a_body_at_the_exact_limit() {
    let mut body = vec![1, 2];

    let result = append_body_chunk(&mut body, &[3, 4], 4);

    assert_eq!(result, Ok(()));
  }

  #[test]
  fn body_collector_rejects_a_chunk_crossing_the_limit() {
    let mut body = vec![1, 2];

    let result = append_body_chunk(&mut body, &[3, 4, 5], 4);

    assert_eq!(result, Err(ArtworkError::ResponseTooLarge));
  }
}
