use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::{oneshot, watch, Notify};

use crate::{
  artwork_cache_key, ArtworkCacheStats, ArtworkDiskCache, JellyfinClient, LibraryImageRequest,
};

pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CACHED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_CACHED_ENTRIES: usize = 256;
pub const MAX_ACTIVE_LOADS: usize = 24;
pub const MAX_ACTIVE_BYTES: usize = 384 * 1024 * 1024;
pub const MAX_QUEUED_LOADS: usize = 128;
pub const DECODE_PIXEL_BUFFER_RESERVATIONS: usize = 2;

const MAX_IMAGE_REFERENCE_BYTES: usize = 32 * 1024;

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

/// Admission priority for a queued Library Image load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadLane {
  Visible,
  Offscreen,
}

#[derive(Default)]
pub struct LoadScheduler {
  next_queue_id: u64,
  active_loads: usize,
  active_bytes: usize,
  visible: VecDeque<QueuedEntry>,
  offscreen: VecDeque<QueuedEntry>,
}

struct QueuedEntry {
  id: u64,
  notify: Arc<Notify>,
}

impl LoadScheduler {
  #[must_use]
  pub fn enqueue(&mut self, lane: LoadLane) -> (u64, Arc<Notify>) {
    let id = self.next_queue_id;
    self.next_queue_id = self.next_queue_id.wrapping_add(1);
    let notify = Arc::new(Notify::new());
    self.lane_mut(lane).push_back(QueuedEntry {
      id,
      notify: Arc::clone(&notify),
    });
    (id, notify)
  }

  fn lane_mut(&mut self, lane: LoadLane) -> &mut VecDeque<QueuedEntry> {
    match lane {
      LoadLane::Visible => &mut self.visible,
      LoadLane::Offscreen => &mut self.offscreen,
    }
  }

  /// The entry admitted next: the visible lane always drains first.
  fn front(&self) -> Option<&QueuedEntry> {
    self.visible.front().or_else(|| self.offscreen.front())
  }

  #[must_use]
  pub fn queued_loads(&self) -> usize {
    self.visible.len() + self.offscreen.len()
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
    let is_next = self.front().is_some_and(|entry| entry.id == queue_id);
    let bytes_fit = reserved_bytes <= max_active_bytes.saturating_sub(self.active_bytes);
    if !is_next || self.active_loads >= max_active_loads || !bytes_fit {
      return false;
    }

    if self
      .visible
      .front()
      .is_some_and(|entry| entry.id == queue_id)
    {
      self.visible.pop_front();
    } else {
      self.offscreen.pop_front();
    }
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

  /// Moves a still-queued offscreen entry to the back of the visible lane.
  /// Returns `false` when the entry is no longer queued (already active,
  /// cancelled, or already visible).
  pub fn promote_to_visible(&mut self, queue_id: u64) -> bool {
    let Some(index) = self.offscreen.iter().position(|entry| entry.id == queue_id) else {
      return false;
    };
    let Some(entry) = self.offscreen.remove(index) else {
      return false;
    };
    self.visible.push_back(entry);
    true
  }

  pub fn cancel(&mut self, queue_id: u64) {
    let was_front = self.front().is_some_and(|entry| entry.id == queue_id);
    if let Some(index) = self.visible.iter().position(|entry| entry.id == queue_id) {
      self.visible.remove(index);
    } else if let Some(index) = self.offscreen.iter().position(|entry| entry.id == queue_id) {
      self.offscreen.remove(index);
    }
    if was_front {
      self.notify_front();
    }
  }

  fn notify_front(&self) {
    if let Some(entry) = self.front() {
      entry.notify.notify_one();
    }
  }

  pub fn cancel_queued(&mut self) {
    self.visible.clear();
    self.offscreen.clear();
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
      Self::Cancelled => "artwork loading was cancelled",
      Self::Overloaded => "artwork loader is at capacity",
    };
    formatter.write_str(message)
  }
}

impl std::error::Error for ArtworkError {}

/// Send-safe artwork bytes returned by the display-independent decoder.
#[derive(Clone)]
pub struct ArtworkBytes(Arc<[u8]>);

impl ArtworkBytes {
  #[must_use]
  pub fn as_slice(&self) -> &[u8] {
    &self.0
  }

  #[must_use]
  pub fn into_inner(self) -> Arc<[u8]> {
    self.0
  }
}

#[cfg(any(test, feature = "test-utils"))]
impl ArtworkBytes {
  #[must_use]
  pub fn from_raw_for_test(bytes: Arc<[u8]>) -> Self {
    Self(bytes)
  }
}

/// A cacheable result produced by an [`ArtworkDecoder`].
pub trait ArtworkOutput: Clone + Send + Sync + 'static {
  fn byte_len(&self) -> usize;
}

impl ArtworkOutput for ArtworkBytes {
  fn byte_len(&self) -> usize {
    self.0.len()
  }
}

/// Display-independent decoding boundary for the bounded artwork pipeline.
pub trait ArtworkDecoder: Clone + Send + Sync + 'static {
  type Output: ArtworkOutput;

  fn decode(
    &self,
    bytes: ArtworkBytes,
    max_decoded_bytes: usize,
  ) -> Result<Self::Output, ArtworkError>;
}

/// Decoder boundary that validates the image container and returns encoded bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawArtworkDecoder;

impl ArtworkDecoder for RawArtworkDecoder {
  type Output = ArtworkBytes;

  fn decode(
    &self,
    bytes: ArtworkBytes,
    _max_decoded_bytes: usize,
  ) -> Result<Self::Output, ArtworkError> {
    validate_static_image_container(bytes.as_slice())?;
    Ok(bytes)
  }
}

/// Where a Library Image load obtained its bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkSource {
  Memory,
  Disk,
  Network,
}

impl ArtworkSource {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Memory => "memory",
      Self::Disk => "disk",
      Self::Network => "network",
    }
  }
}

/// Sanitized aggregate of Library Image loads since the last drain.
///
/// Counts, durations, and byte totals only — never URLs or image references —
/// so it can feed the user-facing Diagnostics view directly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArtworkLoadSummary {
  pub memory_loads: u64,
  pub disk_loads: u64,
  pub network_loads: u64,
  pub failed_loads: u64,
  pub total_duration_millis: u64,
  pub total_bytes: u64,
}

/// How one Library Image load call settled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkLoadSettlement {
  /// Bytes came from the given source.
  Loaded(ArtworkSource),
  /// Served by a coalesced in-flight leader, which reports the source.
  Shared,
  /// Failed with a redacted error.
  Failed,
  /// Cancelled by a generation change.
  Cancelled,
}

/// Telemetry for one Library Image load call: how it settled, its duration,
/// and its encoded byte size. Never carries URLs or image references, so it
/// can feed the user-facing Diagnostics view aggregates directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtworkLoadObservation {
  pub settlement: ArtworkLoadSettlement,
  pub duration: Duration,
  pub bytes: u64,
}

impl ArtworkLoadObservation {
  /// Observation for a synchronous decoded-cache hit on a caller fast path.
  #[must_use]
  pub const fn memory_hit(bytes: u64) -> Self {
    Self {
      settlement: ArtworkLoadSettlement::Loaded(ArtworkSource::Memory),
      duration: Duration::ZERO,
      bytes,
    }
  }
}

impl ArtworkLoadSummary {
  /// Folds one load observation into the aggregate. Shared and cancelled
  /// loads are not counted: the coalescing leader reports shared loads, and
  /// cancellations are navigation churn.
  pub fn record(&mut self, observation: &ArtworkLoadObservation) {
    match observation.settlement {
      ArtworkLoadSettlement::Loaded(source) => {
        self.record_success(source, observation.duration, observation.bytes);
      }
      ArtworkLoadSettlement::Failed => self.record_failure(),
      ArtworkLoadSettlement::Shared | ArtworkLoadSettlement::Cancelled => {}
    }
  }

  fn record_success(&mut self, source: ArtworkSource, duration: Duration, bytes: u64) {
    match source {
      ArtworkSource::Memory => self.memory_loads = self.memory_loads.saturating_add(1),
      ArtworkSource::Disk => self.disk_loads = self.disk_loads.saturating_add(1),
      ArtworkSource::Network => self.network_loads = self.network_loads.saturating_add(1),
    }
    let millis = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    self.total_duration_millis = self.total_duration_millis.saturating_add(millis);
    self.total_bytes = self.total_bytes.saturating_add(bytes);
  }

  fn record_failure(&mut self) {
    self.failed_loads = self.failed_loads.saturating_add(1);
  }

  /// Formats the aggregate for the Diagnostics view, or `None` when no load
  /// settled since the last drain.
  #[must_use]
  pub fn diagnostic_message(&self) -> Option<String> {
    let succeeded = self
      .memory_loads
      .saturating_add(self.disk_loads)
      .saturating_add(self.network_loads);
    let settled = succeeded.saturating_add(self.failed_loads);
    if settled == 0 {
      return None;
    }
    let average_millis = self
      .total_duration_millis
      .checked_div(succeeded)
      .unwrap_or_default();
    Some(format!(
      "Library images settled: {settled} ({} memory, {} disk, {} network, {} failed); \
      {} bytes loaded; average {} ms.",
      self.memory_loads,
      self.disk_loads,
      self.network_loads,
      self.failed_loads,
      self.total_bytes,
      average_millis,
    ))
  }
}

type AdapterLoadResult<T> = Result<T, ArtworkError>;
type AdapterFetchResult = Result<(ArtworkBytes, ArtworkSource), ArtworkError>;
type AdapterLoadAdmission<T> = LoadAdmission<T, oneshot::Receiver<AdapterLoadResult<T>>>;

/// Authenticated, bounded, coalescing artwork pipeline.
pub struct ArtworkAdapter<D = RawArtworkDecoder>
where
  D: ArtworkDecoder,
{
  state: Arc<Mutex<AdapterState<D::Output>>>,
  generation_sender: watch::Sender<u64>,
  limits: ArtworkLimits,
  disk_cache: ArtworkDiskCache,
  decoder: D,
}

impl Default for ArtworkAdapter<RawArtworkDecoder> {
  fn default() -> Self {
    Self::with_limits(ArtworkLimits::default())
  }
}

impl ArtworkAdapter<RawArtworkDecoder> {
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  #[must_use]
  pub fn with_limits(limits: ArtworkLimits) -> Self {
    Self::with_decoder_limits_and_disk_cache(RawArtworkDecoder, limits, ArtworkDiskCache::default())
  }
}

impl<D> ArtworkAdapter<D>
where
  D: ArtworkDecoder,
{
  #[must_use]
  pub fn with_decoder(decoder: D) -> Self {
    Self::with_decoder_limits_and_disk_cache(
      decoder,
      ArtworkLimits::default(),
      ArtworkDiskCache::default(),
    )
  }

  #[must_use]
  pub fn with_decoder_limits_and_disk_cache(
    decoder: D,
    limits: ArtworkLimits,
    disk_cache: ArtworkDiskCache,
  ) -> Self {
    let limits = limits.normalized();
    let (generation_sender, _) = watch::channel(0);
    Self {
      state: Arc::new(Mutex::new(AdapterState {
        generation: 0,
        cache_generation: 0,
        cache: ArtworkCache::new(limits.max_cached_bytes, limits.max_cached_entries),
        in_flight: HashMap::new(),
        scheduler: LoadScheduler::default(),
      })),
      generation_sender,
      limits,
      disk_cache,
      decoder,
    }
  }

  #[must_use]
  pub fn ticket(&self) -> ArtworkLoadTicket {
    ArtworkLoadTicket::new(self.lock_state().generation)
  }

  pub fn cached(&self, image_id: &str) -> Option<D::Output> {
    self.lock_state().cache.get(image_id)
  }

  pub fn set_disk_cache_enabled(&self, enabled: bool) {
    self.disk_cache.set_enabled(enabled);
  }

  /// Returns disk-cache statistics without exposing cache paths.
  ///
  /// # Errors
  ///
  /// Returns [`ArtworkError::CacheUnavailable`] when cache inspection fails.
  pub async fn disk_cache_stats(&self) -> Result<ArtworkCacheStats, ArtworkError> {
    self
      .disk_cache
      .stats()
      .await
      .map_err(|_| ArtworkError::CacheUnavailable)
  }

  /// Clears encoded artwork cached on disk.
  ///
  /// # Errors
  ///
  /// Returns [`ArtworkError::CacheUnavailable`] when the cache cannot be cleared.
  pub async fn clear_disk_cache(&self) -> Result<(), ArtworkError> {
    self
      .disk_cache
      .clear()
      .await
      .map_err(|_| ArtworkError::CacheUnavailable)
  }

  /// Fetches and decodes an opaque signed image reference.
  ///
  /// `lane` classifies the load's scheduling priority: [`LoadLane::Visible`]
  /// loads drain before queued [`LoadLane::Offscreen`] work. The returned
  /// observation carries the load's sanitized telemetry so callers can
  /// aggregate per surface instead of sharing process-wide state.
  pub async fn load(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    lane: LoadLane,
  ) -> (Result<D::Output, ArtworkError>, ArtworkLoadObservation) {
    self
      .load_with_ticket(client, image_id, self.ticket(), lane)
      .await
  }

  /// Fetches and decodes an image only while `ticket` belongs to the current generation.
  pub async fn load_with_ticket(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    ticket: ArtworkLoadTicket,
    lane: LoadLane,
  ) -> (Result<D::Output, ArtworkError>, ArtworkLoadObservation) {
    let started = Instant::now();
    let span = tracing::info_span!(
      "library_image_load",
      source = tracing::field::Empty,
      encoded_bytes = tracing::field::Empty,
      duration_ms = tracing::field::Empty,
    );
    if let Err(error) = validate_image_reference(image_id) {
      let observation = finish_load(&span, started, ArtworkLoadSettlement::Failed, 0);
      return (Err(error), observation);
    }
    // Signed opaque references are authorized against the current session on
    // every call, including decoded cache hits.
    let request = match client.library().image_request(image_id) {
      Ok(request) => request,
      Err(_) => {
        let observation = finish_load(&span, started, ArtworkLoadSettlement::Failed, 0);
        return (Err(ArtworkError::RequestRejected), observation);
      }
    };
    let key = Arc::<str>::from(image_id);
    let disk_key = artwork_cache_key(request.server_url(), request.origin_url());
    let mut generation = self.generation_sender.subscribe();
    let load_generation = ticket.generation();
    match self.admit(Arc::clone(&key), load_generation, lane) {
      LoadAdmission::Cached(artwork) => {
        let bytes = artwork.byte_len();
        let observation = finish_load(
          &span,
          started,
          ArtworkLoadSettlement::Loaded(ArtworkSource::Memory),
          bytes,
        );
        (Ok(artwork), observation)
      }
      LoadAdmission::Follower(receiver) => {
        let result = wait_for_follower(receiver, &mut generation).await;
        let (settlement, bytes) = match &result {
          Ok(artwork) => (ArtworkLoadSettlement::Shared, artwork.byte_len()),
          Err(ArtworkError::Cancelled) => (ArtworkLoadSettlement::Cancelled, 0),
          Err(_) => (ArtworkLoadSettlement::Failed, 0),
        };
        let observation = finish_load(&span, started, settlement, bytes);
        (result, observation)
      }
      LoadAdmission::Leader(load_generation) => {
        let pending = PendingLoad::new(self, Arc::clone(&key), load_generation);
        let result = match self
          .acquire_load_permit(&key, load_generation, lane, &mut generation)
          .await
        {
          Ok(permit) => {
            self
              .fetch_and_decode(client, &request, disk_key, permit, &mut generation)
              .await
          }
          Err(error) => Err(error),
        };
        let (settlement, bytes) = match &result {
          Ok((artwork, source)) => (ArtworkLoadSettlement::Loaded(*source), artwork.byte_len()),
          Err(ArtworkError::Cancelled) => (ArtworkLoadSettlement::Cancelled, 0),
          Err(_) => (ArtworkLoadSettlement::Failed, 0),
        };
        let settled = result
          .as_ref()
          .map(|(artwork, _)| artwork.clone())
          .map_err(|error| *error);
        pending.complete(&settled);
        let observation = finish_load(&span, started, settlement, bytes);
        (result.map(|(artwork, _)| artwork), observation)
      }
      LoadAdmission::Cancelled => {
        let observation = finish_load(&span, started, ArtworkLoadSettlement::Cancelled, 0);
        (Err(ArtworkError::Cancelled), observation)
      }
    }
  }

  /// Cancels queued and network work from the previous consumer generation.
  pub fn cancel_pending(&self) {
    self.advance_generation(false);
  }

  /// Cancels pending work and clears decoded data from the previous session.
  pub fn reset_session(&self) {
    self.advance_generation(true);
  }

  fn advance_generation(&self, clear_cache: bool) {
    let (generation, waiters) = {
      let mut state = self.lock_state();
      state.generation = state.generation.wrapping_add(1);
      let generation = state.generation;
      if clear_cache {
        state.cache_generation = generation;
      }
      let waiters = state.cancel_stale(clear_cache);
      (generation, waiters)
    };
    let _ = self.generation_sender.send_replace(generation);
    notify_cancelled(waiters);
  }

  fn admit(
    &self,
    key: Arc<str>,
    generation: u64,
    lane: LoadLane,
  ) -> AdapterLoadAdmission<D::Output> {
    self.lock_state().admit(key, generation, lane)
  }

  async fn acquire_load_permit(
    &self,
    key: &Arc<str>,
    load_generation: u64,
    lane: LoadLane,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<LoadPermit<D::Output>, ArtworkError> {
    let queued = QueuedLoad::new(self, load_generation, lane)?;
    // Publish the queue entry so a visible follower can promote this leader
    // while it is still queued.
    if let Some(load) = self.lock_state().in_flight.get_mut(key.as_ref()) {
      if load.generation == load_generation {
        load.queued_id = Some(queued.id());
      }
    }
    loop {
      if self.try_activate(queued.id()) {
        return Ok(queued.activate(self.limits.load_reservation_bytes()));
      }
      tokio::select! {
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
    permit: LoadPermit<D::Output>,
    generation: &mut watch::Receiver<u64>,
  ) -> AdapterLoadResult<(D::Output, ArtworkSource)> {
    let (bytes, source) = tokio::select! {
      result = self.original_bytes(client, request, disk_key) => result?,
      changed = generation.changed() => {
        let _ = changed;
        return Err(ArtworkError::Cancelled);
      }
    };
    let decoder = self.decoder.clone();
    let decoded_limit = self.limits.max_decoded_bytes;
    let decode = tokio::task::spawn_blocking(move || {
      // Cancellation may drop the join handle, so the blocking decoder owns
      // aggregate admission until it actually stops.
      let _permit = permit;
      decoder.decode(bytes, decoded_limit)
    });
    tokio::select! {
      result = decode => result.map_err(|_| ArtworkError::DecodeFailed)?.map(|output| (output, source)),
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
  ) -> AdapterFetchResult {
    if let Some(bytes) = self
      .disk_cache
      .load(
        disk_key.clone(),
        self.limits.max_response_bytes,
        validate_disk_artwork,
      )
      .await
    {
      return Ok((ArtworkBytes(bytes), ArtworkSource::Disk));
    }
    let bytes = self.fetch_uncached(client, request).await?;
    if validate_static_image_container(bytes.0.as_ref()).is_ok() {
      let disk_cache = self.disk_cache.clone();
      let disk_bytes = Arc::clone(&bytes.0);
      tokio::spawn(async move {
        disk_cache.store(disk_key, disk_bytes).await;
      });
    }
    Ok((bytes, ArtworkSource::Network))
  }

  async fn fetch_uncached(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
  ) -> Result<ArtworkBytes, ArtworkError> {
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

  fn finish_pending(&self, key: Arc<str>, generation: u64, result: &AdapterLoadResult<D::Output>) {
    let waiters = {
      let mut state = self.lock_state();
      if let Ok(artwork) = result {
        if generation >= state.cache_generation {
          state.cache.insert(Arc::clone(&key), artwork.clone());
        }
      }
      if state.generation == generation {
        state
          .in_flight
          .remove(key.as_ref())
          .map(|load| load.waiters)
          .unwrap_or_default()
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

  fn lock_state(&self) -> MutexGuard<'_, AdapterState<D::Output>> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }
}

#[cfg(any(test, feature = "test-utils"))]
impl<D> ArtworkAdapter<D>
where
  D: ArtworkDecoder,
{
  pub fn seed_cached_for_test(&self, image_id: &str, output: D::Output) {
    self.lock_state().cache.insert(Arc::from(image_id), output);
  }
}

struct AdapterState<T>
where
  T: ArtworkOutput,
{
  generation: u64,
  cache_generation: u64,
  cache: ArtworkCache<T>,
  in_flight: HashMap<Arc<str>, InFlightLoad<T>>,
  scheduler: LoadScheduler,
}

impl<T> AdapterState<T>
where
  T: ArtworkOutput,
{
  fn admit(&mut self, key: Arc<str>, generation: u64, lane: LoadLane) -> AdapterLoadAdmission<T> {
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
      // A visible joiner promotes a still-queued offscreen leader so the
      // shared fetch drains with visible work.
      if lane == LoadLane::Visible {
        if let Some(queue_id) = load.queued_id {
          self.scheduler.promote_to_visible(queue_id);
        }
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
        queued_id: None,
      },
    );
    LoadAdmission::Leader(generation)
  }

  fn cancel_stale(&mut self, clear_cache: bool) -> Vec<oneshot::Sender<AdapterLoadResult<T>>> {
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

struct InFlightLoad<T>
where
  T: ArtworkOutput,
{
  generation: u64,
  waiters: Vec<oneshot::Sender<AdapterLoadResult<T>>>,
  /// Scheduler entry while the leader waits for a permit; lets a visible
  /// follower promote a still-queued offscreen leader.
  queued_id: Option<u64>,
}

async fn wait_for_follower<T>(
  receiver: oneshot::Receiver<AdapterLoadResult<T>>,
  generation: &mut watch::Receiver<u64>,
) -> AdapterLoadResult<T>
where
  T: ArtworkOutput,
{
  tokio::select! {
    result = receiver => result.unwrap_or(Err(ArtworkError::Cancelled)),
    changed = generation.changed() => {
      let _ = changed;
      Err(ArtworkError::Cancelled)
    }
  }
}

/// Builds the per-load observation and records the load's tracing span fields.
fn finish_load(
  span: &tracing::Span,
  started: Instant,
  settlement: ArtworkLoadSettlement,
  bytes: usize,
) -> ArtworkLoadObservation {
  let duration = started.elapsed();
  if let ArtworkLoadSettlement::Loaded(source) = settlement {
    span.record("source", source.as_str());
    span.record("encoded_bytes", bytes);
  }
  span.record("duration_ms", duration.as_millis() as u64);
  ArtworkLoadObservation {
    settlement,
    duration,
    bytes: bytes as u64,
  }
}

fn notify_cancelled<T>(waiters: Vec<oneshot::Sender<AdapterLoadResult<T>>>)
where
  T: ArtworkOutput,
{
  for waiter in waiters {
    let _ = waiter.send(Err(ArtworkError::Cancelled));
  }
}

struct QueuedLoad<'a, D>
where
  D: ArtworkDecoder,
{
  adapter: &'a ArtworkAdapter<D>,
  id: Option<u64>,
  notify: Arc<Notify>,
}

impl<'a, D> QueuedLoad<'a, D>
where
  D: ArtworkDecoder,
{
  fn new(
    adapter: &'a ArtworkAdapter<D>,
    generation: u64,
    lane: LoadLane,
  ) -> Result<Self, ArtworkError> {
    let (id, notify) = {
      let mut state = adapter.lock_state();
      if state.generation != generation {
        return Err(ArtworkError::Cancelled);
      }
      if state.scheduler.queued_loads() >= adapter.limits.max_queued_loads {
        return Err(ArtworkError::Overloaded);
      }
      state.scheduler.enqueue(lane)
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

  fn activate(mut self, reserved_bytes: usize) -> LoadPermit<D::Output> {
    self.id.take();
    LoadPermit {
      state: Arc::clone(&self.adapter.state),
      reserved_bytes,
    }
  }
}

impl<D> Drop for QueuedLoad<'_, D>
where
  D: ArtworkDecoder,
{
  fn drop(&mut self) {
    if let Some(id) = self.id.take() {
      self.adapter.cancel_queued_load(id);
    }
  }
}

struct LoadPermit<T>
where
  T: ArtworkOutput,
{
  state: Arc<Mutex<AdapterState<T>>>,
  reserved_bytes: usize,
}

impl<T> Drop for LoadPermit<T>
where
  T: ArtworkOutput,
{
  fn drop(&mut self) {
    self
      .state
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .scheduler
      .release(self.reserved_bytes);
  }
}

struct PendingLoad<'a, D>
where
  D: ArtworkDecoder,
{
  adapter: &'a ArtworkAdapter<D>,
  key: Option<Arc<str>>,
  generation: u64,
}

impl<'a, D> PendingLoad<'a, D>
where
  D: ArtworkDecoder,
{
  fn new(adapter: &'a ArtworkAdapter<D>, key: Arc<str>, generation: u64) -> Self {
    Self {
      adapter,
      key: Some(key),
      generation,
    }
  }

  fn complete(mut self, result: &AdapterLoadResult<D::Output>) {
    if let Some(key) = self.key.take() {
      self.adapter.finish_pending(key, self.generation, result);
    }
  }
}

impl<D> Drop for PendingLoad<'_, D>
where
  D: ArtworkDecoder,
{
  fn drop(&mut self) {
    if let Some(key) = self.key.take() {
      self
        .adapter
        .finish_pending(key, self.generation, &Err(ArtworkError::Cancelled));
    }
  }
}

fn validate_disk_artwork(bytes: &[u8]) -> bool {
  validate_static_image_container(bytes).is_ok()
}

fn validate_image_reference(image_id: &str) -> Result<(), ArtworkError> {
  if image_id.is_empty() || image_id.len() > MAX_IMAGE_REFERENCE_BYTES {
    return Err(ArtworkError::RequestRejected);
  }
  Ok(())
}

pub fn validate_response_metadata(
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

pub fn validate_static_image_container(bytes: &[u8]) -> Result<(), ArtworkError> {
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

pub fn append_body_chunk(
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

struct ArtworkCache<T>
where
  T: ArtworkOutput,
{
  entries: HashMap<Arc<str>, CacheEntry<T>>,
  total_bytes: usize,
  clock: u64,
  max_bytes: usize,
  max_entries: usize,
}

struct CacheEntry<T> {
  artwork: T,
  last_used: u64,
}

impl<T> ArtworkCache<T>
where
  T: ArtworkOutput,
{
  fn new(max_bytes: usize, max_entries: usize) -> Self {
    Self {
      entries: HashMap::new(),
      total_bytes: 0,
      clock: 0,
      max_bytes,
      max_entries,
    }
  }

  fn get(&mut self, key: &str) -> Option<T> {
    let entry = self.entries.get_mut(key)?;
    self.clock = self.clock.saturating_add(1);
    entry.last_used = self.clock;
    Some(entry.artwork.clone())
  }

  fn clear(&mut self) {
    self.entries.clear();
    self.total_bytes = 0;
  }

  fn insert(&mut self, key: Arc<str>, artwork: T) {
    let artwork_bytes = artwork.byte_len();
    if self.max_bytes == 0 || self.max_entries == 0 || artwork_bytes > self.max_bytes {
      return;
    }
    if let Some(previous) = self.entries.remove(key.as_ref()) {
      self.total_bytes = self.total_bytes.saturating_sub(previous.artwork.byte_len());
    }
    self.clock = self.clock.saturating_add(1);
    self.total_bytes = self.total_bytes.saturating_add(artwork_bytes);
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
        self.total_bytes = self.total_bytes.saturating_sub(removed.artwork.byte_len());
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{image_id_for_url, ImageRefKind, MediaServerProvider, SavedSession};

  fn artwork(data: &[u8]) -> ArtworkBytes {
    ArtworkBytes(Arc::from(data))
  }

  fn begin_leader<'a>(
    adapter: &'a ArtworkAdapter,
    key: &str,
  ) -> PendingLoad<'a, RawArtworkDecoder> {
    let key = Arc::<str>::from(key);
    let generation = adapter.lock_state().generation;
    let LoadAdmission::Leader(generation) =
      adapter.admit(Arc::clone(&key), generation, LoadLane::Offscreen)
    else {
      panic!("expected a leader admission");
    };
    PendingLoad::new(adapter, key, generation)
  }

  fn activate_permit(adapter: &ArtworkAdapter) -> LoadPermit<ArtworkBytes> {
    let generation = adapter.lock_state().generation;
    let queued =
      QueuedLoad::new(adapter, generation, LoadLane::Visible).expect("queue has capacity");
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
    let (first, _) = scheduler.enqueue(LoadLane::Visible);
    let (second, _) = scheduler.enqueue(LoadLane::Visible);
    let (third, _) = scheduler.enqueue(LoadLane::Visible);

    assert!(scheduler.try_activate(first, 2, 100, 40));
    assert!(scheduler.try_activate(second, 2, 100, 40));
    assert!(!scheduler.try_activate(third, 2, 100, 40));
    scheduler.release(40);
    assert!(scheduler.try_activate(third, 2, 100, 40));
  }
  #[test]
  fn full_24_poster_page_activates_concurrently_without_queueing() {
    let adapter = ArtworkAdapter::default();
    let generation = adapter.lock_state().generation;
    let loads = (0..MAX_ACTIVE_LOADS)
      .map(|_| {
        QueuedLoad::new(&adapter, generation, LoadLane::Visible).expect("load should be queued")
      })
      .collect::<Vec<_>>();
    for load in &loads {
      assert!(
        adapter.try_activate(load.id()),
        "all 24 visible page loads should activate in the same pass"
      );
    }
    assert_eq!(
      adapter.lock_state().scheduler.active_loads(),
      MAX_ACTIVE_LOADS
    );
    assert_eq!(adapter.lock_state().scheduler.queued_loads(), 0);
  }

  #[test]
  fn visible_lane_preempts_queued_offscreen_loads_when_a_permit_frees() {
    let mut scheduler = LoadScheduler::default();
    let (offscreen_first, _) = scheduler.enqueue(LoadLane::Offscreen);
    let (offscreen_second, _) = scheduler.enqueue(LoadLane::Offscreen);
    let reservation = 40;

    assert!(scheduler.try_activate(offscreen_first, 1, 100, reservation));
    assert!(!scheduler.try_activate(offscreen_second, 1, 100, reservation));

    // A visible load queued later still drains before the waiting offscreen load.
    let (visible, _) = scheduler.enqueue(LoadLane::Visible);
    assert!(!scheduler.try_activate(visible, 1, 100, reservation));
    scheduler.release(reservation);
    assert!(scheduler.try_activate(visible, 1, 100, reservation));
    assert!(!scheduler.try_activate(offscreen_second, 1, 100, reservation));
    scheduler.release(reservation);
    assert!(scheduler.try_activate(offscreen_second, 1, 100, reservation));

    assert_eq!(scheduler.queued_loads(), 0);
    scheduler.release(reservation);
    assert_eq!(scheduler.active_loads(), 0);
    assert_eq!(scheduler.active_bytes(), 0);
  }

  #[test]
  fn cancelling_a_visible_load_keeps_lane_ordering_intact() {
    let mut scheduler = LoadScheduler::default();
    let (offscreen, _) = scheduler.enqueue(LoadLane::Offscreen);
    let (visible_first, _) = scheduler.enqueue(LoadLane::Visible);
    let (visible_second, _) = scheduler.enqueue(LoadLane::Visible);

    scheduler.cancel(visible_first);

    // The remaining visible load drains before the earlier offscreen load.
    assert!(scheduler.try_activate(visible_second, 2, 100, 40));
    assert!(scheduler.try_activate(offscreen, 2, 100, 40));
    assert_eq!(scheduler.queued_loads(), 0);
  }

  #[test]
  fn cached_admission_reports_a_memory_observation() {
    let adapter = ArtworkAdapter::default();
    let client = JellyfinClient::new();
    let server_url = "https://server.example.com";
    let reference = image_id(server_url);
    adopt_session(&client, server_url, "user");
    adapter.seed_cached_for_test(&reference, artwork(&[1, 2, 3, 4]));
    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .expect("runtime builds");

    let (result, observation) =
      runtime.block_on(adapter.load(&client, &reference, LoadLane::Visible));

    assert!(result.is_ok());
    assert_eq!(
      observation.settlement,
      ArtworkLoadSettlement::Loaded(ArtworkSource::Memory)
    );
    assert_eq!(observation.bytes, 4);
  }

  #[test]
  fn summary_folds_observations_and_skips_shared_and_cancelled_loads() {
    let mut summary = ArtworkLoadSummary::default();
    summary.record(&ArtworkLoadObservation::memory_hit(10));
    summary.record(&ArtworkLoadObservation {
      settlement: ArtworkLoadSettlement::Loaded(ArtworkSource::Network),
      duration: Duration::from_millis(40),
      bytes: 90,
    });
    summary.record(&ArtworkLoadObservation {
      settlement: ArtworkLoadSettlement::Failed,
      duration: Duration::from_millis(5),
      bytes: 0,
    });
    summary.record(&ArtworkLoadObservation {
      settlement: ArtworkLoadSettlement::Shared,
      duration: Duration::from_millis(7),
      bytes: 10,
    });
    summary.record(&ArtworkLoadObservation {
      settlement: ArtworkLoadSettlement::Cancelled,
      duration: Duration::ZERO,
      bytes: 0,
    });

    assert_eq!(summary.memory_loads, 1);
    assert_eq!(summary.network_loads, 1);
    assert_eq!(summary.failed_loads, 1);
    assert_eq!(summary.total_bytes, 100);
    assert_eq!(summary.total_duration_millis, 40);
  }

  #[test]
  fn promotion_moves_a_queued_offscreen_entry_to_the_visible_lane() {
    let mut scheduler = LoadScheduler::default();
    let (promoted, _) = scheduler.enqueue(LoadLane::Offscreen);
    let (earlier_offscreen, _) = scheduler.enqueue(LoadLane::Offscreen);
    let (visible, _) = scheduler.enqueue(LoadLane::Visible);
    let reservation = 40;

    assert!(scheduler.promote_to_visible(promoted));

    assert!(scheduler.try_activate(visible, 3, 200, reservation));
    assert!(scheduler.try_activate(promoted, 3, 200, reservation));
    assert!(scheduler.try_activate(earlier_offscreen, 3, 200, reservation));
    // Already-activated entries cannot be promoted again.
    assert!(!scheduler.promote_to_visible(promoted));
  }

  #[test]
  fn visible_follower_promotes_a_queued_offscreen_leader() {
    let adapter = ArtworkAdapter::default();
    let mut permits = (0..MAX_ACTIVE_LOADS)
      .map(|_| activate_permit(&adapter))
      .collect::<Vec<_>>();
    let generation = adapter.lock_state().generation;
    // An unrelated offscreen load queues ahead of the leader.
    let earlier =
      QueuedLoad::new(&adapter, generation, LoadLane::Offscreen).expect("queue has capacity");
    let _pending = begin_leader(&adapter, "shared");
    let leader =
      QueuedLoad::new(&adapter, generation, LoadLane::Offscreen).expect("queue has capacity");
    adapter
      .lock_state()
      .in_flight
      .get_mut("shared")
      .expect("leader is in flight")
      .queued_id = Some(leader.id());

    let LoadAdmission::Follower(_receiver) =
      adapter.admit(Arc::from("shared"), generation, LoadLane::Visible)
    else {
      panic!("expected a follower admission");
    };

    // When one permit frees, the promoted leader drains before the earlier
    // offscreen load.
    drop(permits.pop());
    assert!(adapter.try_activate(leader.id()));
    assert!(!adapter.try_activate(earlier.id()));
  }

  #[test]
  fn load_summary_message_is_sanitized_and_empty_summary_records_nothing() {
    assert_eq!(ArtworkLoadSummary::default().diagnostic_message(), None);

    let summary = ArtworkLoadSummary {
      memory_loads: 8,
      disk_loads: 3,
      network_loads: 1,
      failed_loads: 2,
      total_duration_millis: 480,
      total_bytes: 1_234_567,
    };
    let message = summary
      .diagnostic_message()
      .expect("settled loads describe");

    assert!(message.contains("14"));
    assert!(message.contains("8 memory"));
    assert!(message.contains("3 disk"));
    assert!(message.contains("1 network"));
    assert!(message.contains("2 failed"));
    assert!(message.contains("1234567 bytes"));
    assert!(message.contains("average 40 ms"));
    assert!(!message.contains("https://"));
  }

  #[test]
  fn ordinary_home_batch_queues_and_drains_in_fifo_order() {
    let mut scheduler = LoadScheduler::default();
    let queued = (0..48)
      .map(|_| scheduler.enqueue(LoadLane::Visible).0)
      .collect::<Vec<_>>();
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
      ArtworkError::Cancelled,
      ArtworkError::Overloaded,
    ];
    for error in errors {
      assert!(!error.to_string().contains(secret));
    }
  }

  #[test]
  fn queue_accepts_an_ordinary_48_image_home_and_bounds_pathological_backlog() {
    let adapter = ArtworkAdapter::default();
    let generation = adapter.lock_state().generation;
    let home = (0..48)
      .map(|_| {
        QueuedLoad::new(&adapter, generation, LoadLane::Visible).expect("home queue has capacity")
      })
      .collect::<Vec<_>>();
    let margin = (48..MAX_QUEUED_LOADS)
      .map(|_| {
        QueuedLoad::new(&adapter, generation, LoadLane::Offscreen)
          .expect("bounded margin has capacity")
      })
      .collect::<Vec<_>>();

    assert!(matches!(
      QueuedLoad::new(&adapter, generation, LoadLane::Visible),
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
      .map(|_| {
        QueuedLoad::new(&adapter, stale_generation, LoadLane::Offscreen)
          .expect("stale queue has capacity")
      })
      .collect::<Vec<_>>();

    adapter.cancel_pending();

    let current_generation = adapter.lock_state().generation;
    let current = QueuedLoad::new(&adapter, current_generation, LoadLane::Visible)
      .expect("current load is admitted");
    assert!(adapter.try_activate(current.id()));
    let current = current.activate(adapter.limits.load_reservation_bytes());
    drop(stale);
    drop(current);

    let state = adapter.lock_state();
    assert_eq!(state.scheduler.active_loads(), 0);
    assert_eq!(state.scheduler.active_bytes(), 0);
    assert_eq!(state.scheduler.queued_loads(), 0);
  }

  #[test]
  fn generation_advance_without_receivers_is_retained_for_later_loads() {
    let adapter = ArtworkAdapter::default();

    adapter.cancel_pending();

    let generation = *adapter.generation_sender.subscribe().borrow();
    assert_eq!(generation, 1);
    assert!(matches!(
      adapter.admit(Arc::from("current"), generation, LoadLane::Offscreen),
      LoadAdmission::Leader(1)
    ));
  }

  #[test]
  fn cancelling_a_queued_load_allows_the_next_unique_load_to_run() {
    let adapter = ArtworkAdapter::default();
    let first = activate_permit(&adapter);
    let second = activate_permit(&adapter);
    let generation = adapter.lock_state().generation;
    let cancelled =
      QueuedLoad::new(&adapter, generation, LoadLane::Visible).expect("queue has capacity");
    let next =
      QueuedLoad::new(&adapter, generation, LoadLane::Visible).expect("queue has capacity");

    assert!(!adapter.try_activate(next.id()));
    drop(cancelled);
    drop(first);
    assert!(adapter.try_activate(next.id()));

    let next = next.activate(adapter.limits.load_reservation_bytes());
    drop(second);
    drop(next);
    let state = adapter.lock_state();
    assert_eq!(state.scheduler.active_loads(), 0);
    assert_eq!(state.scheduler.active_bytes(), 0);
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
    assert_eq!(adapter.lock_state().scheduler.active_loads(), 1);
    finish_sender.send(()).expect("worker remains");
    worker.join().expect("worker does not panic");

    let state = adapter.lock_state();
    assert_eq!(state.scheduler.active_loads(), 0);
    assert_eq!(state.scheduler.active_bytes(), 0);
  }

  #[test]
  fn in_flight_admission_coalesces_one_result_and_storage() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) =
      adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen)
    else {
      panic!("expected a follower admission");
    };
    let decoded = artwork(&[1, 2, 3, 4]);
    let bytes = decoded.0.as_ptr();

    pending.complete(&Ok(decoded));
    let received = receiver
      .blocking_recv()
      .expect("leader sends a result")
      .expect("leader succeeds");

    assert_eq!(received.0.as_ptr(), bytes);
    let LoadAdmission::Cached(cached) = adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen)
    else {
      panic!("expected a decoded cache hit");
    };
    assert_eq!(cached.0.as_ptr(), bytes);
  }

  #[test]
  fn cancelled_follower_does_not_cancel_the_shared_leader() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) =
      adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen)
    else {
      panic!("expected a follower admission");
    };
    drop(receiver);
    pending.complete(&Ok(artwork(&[1, 2, 3, 4])));

    assert!(matches!(
      adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen),
      LoadAdmission::Cached(_)
    ));
  }

  #[test]
  fn cancelled_leader_notifies_followers_and_releases_the_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) =
      adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen)
    else {
      panic!("expected a follower admission");
    };

    drop(pending);

    assert!(matches!(
      receiver.blocking_recv().expect("leader sends cancellation"),
      Err(ArtworkError::Cancelled)
    ));
    assert!(matches!(
      adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn completed_error_releases_the_coalescing_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    pending.complete(&Err(ArtworkError::FetchFailed));

    assert!(matches!(
      adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn generation_cancellation_notifies_followers_and_rejects_stale_admission() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) =
      adapter.admit(Arc::from("same"), 0, LoadLane::Offscreen)
    else {
      panic!("expected a follower admission");
    };

    adapter.cancel_pending();

    assert!(matches!(
      receiver.blocking_recv().expect("follower is notified"),
      Err(ArtworkError::Cancelled)
    ));
    assert!(matches!(
      adapter.admit(Arc::from("stale"), 0, LoadLane::Offscreen),
      LoadAdmission::Cancelled
    ));
    assert!(matches!(
      adapter.admit(Arc::from("current"), 1, LoadLane::Offscreen),
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
  fn stale_success_after_reset_session_does_not_repopulate_decoded_cache() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "stale");

    adapter.reset_session();
    pending.complete(&Ok(artwork(&[1, 2, 3, 4])));

    assert!(adapter.lock_state().cache.get("stale").is_none());
  }

  #[test]
  fn ticket_captured_before_reset_cannot_adopt_the_new_generation() {
    let adapter = ArtworkAdapter::default();
    let stale = adapter.ticket();

    adapter.reset_session();

    assert!(matches!(
      adapter.admit(Arc::from("stale"), stale.generation(), LoadLane::Offscreen),
      LoadAdmission::Cancelled
    ));
    assert!(matches!(
      adapter.admit(
        Arc::from("current"),
        adapter.ticket().generation(),
        LoadLane::Offscreen
      ),
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
    let cached_bytes = cached.0.as_ptr();
    adopt_session(&client, first_server, "first-user");
    adapter
      .lock_state()
      .cache
      .insert(Arc::from(reference.as_str()), cached);
    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .expect("runtime builds");

    let (accepted, _) = runtime.block_on(adapter.load(&client, &reference, LoadLane::Visible));
    let accepted = accepted.expect("current session accepts cache hit");
    assert_eq!(accepted.0.as_ptr(), cached_bytes);

    adopt_session(&client, "https://second.example.com", "second-user");
    assert!(matches!(
      runtime
        .block_on(adapter.load(&client, &reference, LoadLane::Visible))
        .0,
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
