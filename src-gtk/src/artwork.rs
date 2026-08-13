//! Authenticated, bounded artwork loading for the GTK frontend.
//!
//! Network work and image decoding return `Send`-safe data. GTK objects are
//! created only when [`DecodedArtwork::texture`] is called on the GTK thread.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use jellypilot_media_server::JellyfinClient;
use relm4::gtk;
use relm4::gtk::gdk_pixbuf::prelude::PixbufLoaderExt;
use relm4::tokio::sync::{oneshot, Notify};

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
  limits: ArtworkLimits,
}

#[derive(Clone, Copy)]
struct ArtworkLimits {
  max_response_bytes: usize,
  max_decoded_bytes: usize,
  max_cached_bytes: usize,
  max_cached_entries: usize,
  max_active_loads: usize,
  max_active_bytes: usize,
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
    let limits = limits.normalized();
    Self {
      state: Arc::new(Mutex::new(AdapterState {
        cache: ArtworkCache::new(limits.max_cached_bytes, limits.max_cached_entries),
        in_flight: HashMap::new(),
        scheduler: LoadScheduler::default(),
      })),
      limits,
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
  pub(crate) async fn load(
    &self,
    client: &JellyfinClient,
    image_id: &str,
  ) -> Result<DecodedArtwork, ArtworkError> {
    validate_image_reference(image_id)?;
    let key = Arc::<str>::from(image_id);

    match self.admit(Arc::clone(&key)) {
      LoadAdmission::Cached(artwork) => Ok(artwork),
      LoadAdmission::Follower(receiver) => match receiver.await {
        Ok(result) => result,
        Err(_) => Err(ArtworkError::Cancelled),
      },
      LoadAdmission::Leader => {
        let pending = PendingLoad::new(self, Arc::clone(&key));
        let permit = self.acquire_load_permit().await;
        let result = self.fetch_and_decode(client, &key, permit).await;
        pending.complete(&result);
        result
      }
    }
  }

  fn admit(&self, key: Arc<str>) -> LoadAdmission {
    self.lock_state().admit(key)
  }

  async fn acquire_load_permit(&self) -> LoadPermit {
    let queued = QueuedLoad::new(self);
    loop {
      if self.try_activate(queued.id()) {
        return queued.activate(self.limits.load_reservation_bytes());
      }
      queued.wait().await;
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
    image_id: &str,
    permit: LoadPermit,
  ) -> LoadResult {
    let bytes = self.fetch_uncached(client, image_id).await?;
    let decoded_limit = self.limits.max_decoded_bytes;
    relm4::spawn_blocking(move || {
      // Keep aggregate admission until the blocking decode really stops. A
      // cancelled caller may drop its join handle while this closure runs.
      let _permit = permit;
      decode_pixels(bytes, decoded_limit)
    })
    .await
    .map_err(|_| ArtworkError::DecodeFailed)?
  }

  async fn fetch_uncached(&self, client: &JellyfinClient, image_id: &str) -> FetchResult {
    let request = client
      .library()
      .image_request(image_id)
      .map_err(|_| ArtworkError::RequestRejected)?;
    let mut response = client
      .library()
      .fetch_image(&request)
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

  fn finish_pending(&self, key: Arc<str>, result: &LoadResult) {
    let waiters = {
      let mut state = self.lock_state();
      if let Ok(artwork) = result {
        state.cache.insert(Arc::clone(&key), artwork.clone());
      }
      state.in_flight.remove(key.as_ref()).unwrap_or_default()
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

struct AdapterState {
  cache: ArtworkCache,
  in_flight: HashMap<Arc<str>, Vec<oneshot::Sender<LoadResult>>>,
  scheduler: LoadScheduler,
}

impl AdapterState {
  fn admit(&mut self, key: Arc<str>) -> LoadAdmission {
    if let Some(artwork) = self.cache.get(key.as_ref()) {
      return LoadAdmission::Cached(artwork);
    }

    if let Some(waiters) = self.in_flight.get_mut(key.as_ref()) {
      let (sender, receiver) = oneshot::channel();
      waiters.push(sender);
      return LoadAdmission::Follower(receiver);
    }

    self.in_flight.insert(key, Vec::new());
    LoadAdmission::Leader
  }
}

enum LoadAdmission {
  Cached(DecodedArtwork),
  Follower(oneshot::Receiver<LoadResult>),
  Leader,
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
}

struct QueuedLoad<'a> {
  adapter: &'a ArtworkAdapter,
  id: Option<u64>,
  notify: Arc<Notify>,
}

impl<'a> QueuedLoad<'a> {
  fn new(adapter: &'a ArtworkAdapter) -> Self {
    let (id, notify) = adapter.lock_state().scheduler.enqueue();
    Self {
      adapter,
      id: Some(id),
      notify,
    }
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
}

impl<'a> PendingLoad<'a> {
  fn new(adapter: &'a ArtworkAdapter, key: Arc<str>) -> Self {
    Self {
      adapter,
      key: Some(key),
    }
  }

  fn complete(mut self, result: &LoadResult) {
    if let Some(key) = self.key.take() {
      self.adapter.finish_pending(key, result);
    }
  }
}

impl Drop for PendingLoad<'_> {
  fn drop(&mut self) {
    if let Some(key) = self.key.take() {
      self
        .adapter
        .finish_pending(key, &Err(ArtworkError::Cancelled));
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
  ResponseTooLarge,
  EmptyResponse,
  DecodeFailed,
  DecodedImageTooLarge,
  UiThreadRequired,
  Cancelled,
}

impl fmt::Display for ArtworkError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let message = match self {
      Self::RequestRejected => "artwork reference was rejected",
      Self::FetchFailed => "artwork could not be fetched",
      Self::OriginRejected => "artwork server returned an unusable status",
      Self::UnsupportedContentType => "artwork response was not an image",
      Self::ResponseTooLarge => "artwork response exceeded the memory limit",
      Self::EmptyResponse => "artwork response was empty",
      Self::DecodeFailed => "artwork data could not be decoded",
      Self::DecodedImageTooLarge => "decoded artwork exceeded the memory limit",
      Self::UiThreadRequired => "artwork texture must be created on the GTK thread",
      Self::Cancelled => "artwork loading was cancelled",
    };
    formatter.write_str(message)
  }
}

impl std::error::Error for ArtworkError {}

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
    let LoadAdmission::Leader = adapter.admit(Arc::clone(&key)) else {
      panic!("expected a leader admission");
    };
    PendingLoad::new(adapter, key)
  }

  fn activate_permit(adapter: &ArtworkAdapter) -> LoadPermit {
    let queued = QueuedLoad::new(adapter);
    assert!(adapter.try_activate(queued.id()));
    queued.activate(adapter.limits.load_reservation_bytes())
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
  fn cancelling_a_queued_load_allows_the_next_unique_load_to_run() {
    let adapter = ArtworkAdapter::default();
    let first = activate_permit(&adapter);
    let second = activate_permit(&adapter);
    let cancelled = QueuedLoad::new(&adapter);
    let next = QueuedLoad::new(&adapter);

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
    let LoadAdmission::Follower(receiver) = adapter.admit(key) else {
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
    let LoadAdmission::Cached(cached) = adapter.admit(Arc::from("same")) else {
      panic!("expected a decoded cache hit");
    };
    assert_eq!(cached.pixels.as_ptr(), pixels);
  }

  #[test]
  fn cancelled_follower_does_not_cancel_the_shared_leader() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let key = Arc::<str>::from("same");
    let LoadAdmission::Follower(receiver) = adapter.admit(key) else {
      panic!("expected a follower admission");
    };
    drop(receiver);
    pending.complete(&Ok(artwork(&[1, 2, 3, 4])));

    assert!(matches!(
      adapter.admit(Arc::from("same")),
      LoadAdmission::Cached(_)
    ));
  }

  #[test]
  fn cancelled_leader_notifies_followers_and_releases_the_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) = adapter.admit(Arc::from("same")) else {
      panic!("expected a follower admission");
    };

    drop(pending);

    assert!(matches!(
      receiver.blocking_recv().expect("leader sends cancellation"),
      Err(ArtworkError::Cancelled)
    ));
    assert!(matches!(
      adapter.admit(Arc::from("same")),
      LoadAdmission::Leader
    ));
  }

  #[test]
  fn completed_error_releases_the_coalescing_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    pending.complete(&Err(ArtworkError::FetchFailed));

    assert!(matches!(
      adapter.admit(Arc::from("same")),
      LoadAdmission::Leader
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
      ArtworkError::ResponseTooLarge,
      ArtworkError::EmptyResponse,
      ArtworkError::DecodeFailed,
      ArtworkError::DecodedImageTooLarge,
      ArtworkError::UiThreadRequired,
      ArtworkError::Cancelled,
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
