use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::sync::{oneshot, watch, Notify};

use crate::{
  artwork_cache_key, ArtworkCacheStats, ArtworkDiskCache, JellyfinClient, LibraryImageRequest,
};

pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CACHED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_CACHED_ENTRIES: usize = 256;
pub const MAX_RASTER_CACHED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RASTER_CACHED_ENTRIES: usize = 512;
pub const MAX_ACTIVE_LOADS: usize = 24;
pub const MAX_ACTIVE_BYTES: usize = 384 * 1024 * 1024;
pub const DECODE_PIXEL_BUFFER_RESERVATIONS: usize = 2;

const MAX_IMAGE_REFERENCE_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy)]
pub struct ArtworkLimits {
  pub max_response_bytes: usize,
  pub max_cached_bytes: usize,
  pub max_cached_entries: usize,
  pub max_raster_cached_bytes: usize,
  pub max_raster_cached_entries: usize,
  pub max_active_loads: usize,
  pub max_active_bytes: usize,
}

impl Default for ArtworkLimits {
  fn default() -> Self {
    Self {
      max_response_bytes: MAX_RESPONSE_BYTES,
      max_cached_bytes: MAX_CACHED_BYTES,
      max_cached_entries: MAX_CACHED_ENTRIES,
      max_raster_cached_bytes: MAX_RASTER_CACHED_BYTES,
      max_raster_cached_entries: MAX_RASTER_CACHED_ENTRIES,
      max_active_loads: MAX_ACTIVE_LOADS,
      max_active_bytes: MAX_ACTIVE_BYTES,
    }
  }
}

impl ArtworkLimits {
  /// Bytes one plain load admits against the aggregate budget: the
  /// encoded body, bounded full-size decode buffer, and working raster
  /// buffers.
  #[must_use]
  pub fn load_reservation_bytes(self, size_class: ArtworkSizeClass) -> usize {
    self
      .max_response_bytes
      .saturating_add(size_class.max_decode_bytes())
      .saturating_add(
        size_class
          .max_raster_bytes()
          .saturating_mul(DECODE_PIXEL_BUFFER_RESERVATIONS),
      )
  }

  fn load_reservation_bytes_with_derived(
    self,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
  ) -> usize {
    let shadow_bytes = if derived.logo_shadow {
      size_class.max_logo_shadow_bytes()
    } else {
      0
    };
    self
      .load_reservation_bytes(size_class)
      .saturating_add(shadow_bytes)
  }

  #[must_use]
  pub fn normalized(mut self) -> Self {
    self.max_active_loads = self.max_active_loads.max(1);
    // A normalized custom budget must admit one maximum-sized Backdrop.
    let max_backdrop_load = self.load_reservation_bytes(ArtworkSizeClass::Backdrop);
    self.max_active_bytes = self.max_active_bytes.max(max_backdrop_load);
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

/// Admission priority for live Library Image demand awaiting a resource permit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadLane {
  Visible,
  Offscreen,
}

#[derive(Default)]
struct LoadBudget {
  active_loads: usize,
  active_bytes: usize,
}

impl LoadBudget {
  #[cfg(test)]
  const fn active_loads(&self) -> usize {
    self.active_loads
  }

  #[cfg(test)]
  const fn active_bytes(&self) -> usize {
    self.active_bytes
  }

  fn try_reserve(
    &mut self,
    max_active_loads: usize,
    max_active_bytes: usize,
    bytes: usize,
  ) -> bool {
    if self.active_loads >= max_active_loads
      || bytes > max_active_bytes.saturating_sub(self.active_bytes)
    {
      return false;
    }
    self.active_loads += 1;
    self.active_bytes += bytes;
    true
  }

  fn release(&mut self, reserved_bytes: usize) {
    self.active_loads = self.active_loads.saturating_sub(1);
    self.active_bytes = self.active_bytes.saturating_sub(reserved_bytes);
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
  pub fn byte_len(&self) -> usize {
    self.0.len()
  }
}

/// Render-side decode bucket for a Library Image Raster.
///
/// Classes are derived from view constants at roughly twice the logical
/// display size; they do not change what is requested from the server.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtworkSizeClass {
  /// Browse posters (160×240), home thumbs (240×135), player bar (56×56).
  Card,
  /// Home hero and detail poster (220×330).
  Hero,
  /// Detail backdrop; the server already caps these requests at 1920px.
  Backdrop,
  /// Account avatars. Jellyfin serves user photos only at original
  /// resolution (10.11's legacy route discards sizing parameters; the
  /// current route has none), so the decode budget matches Backdrop while
  /// the raster box stays thumbnail-small.
  Avatar,
}

impl ArtworkSizeClass {
  /// Decode target box: the raster is shrunk to fit, aspect preserved, and
  /// never upscaled.
  #[must_use]
  pub const fn target_box(self) -> (u32, u32) {
    match self {
      Self::Card => (400, 600),
      Self::Hero => (440, 660),
      Self::Backdrop => (1920, 1920),
      Self::Avatar => (128, 128),
    }
  }

  /// Upper bound on one raster's RGBA bytes, implied by the target box.
  #[must_use]
  pub const fn max_raster_bytes(self) -> usize {
    let (width, height) = self.target_box();
    width as usize * height as usize * 4
  }

  /// Shadow rasters carry a transparent margin of height/4 per side, bounding
  /// them at (width + height/2) x (height * 3/2) pixels.
  const fn max_logo_shadow_bytes(self) -> usize {
    let (width, height) = self.target_box();
    (width as usize + height as usize / 2) * (height as usize * 3 / 2) * 4
  }

  /// Cap on the source-image area decoded before downsampling, in pixels.
  ///
  /// Sources are server-resized to maxWidth 600 (Card/Hero) or 1920
  /// (Backdrop); the caps admit generous aspect extremes while rejecting
  /// decompression-bomb-shaped sources before the full RGBA buffer is
  /// allocated.
  pub const fn max_decode_pixels(self) -> u64 {
    match self {
      Self::Card | Self::Hero => 600 * 2400,
      Self::Backdrop | Self::Avatar => 1920 * 4320,
    }
  }

  /// Upper bound on the full-size decoded RGBA buffer, implied by
  /// [`Self::max_decode_pixels`].
  #[must_use]
  pub const fn max_decode_bytes(self) -> usize {
    self.max_decode_pixels() as usize * 4
  }

  /// Server-side resize width requested for sources decoded into this class.
  ///
  /// Wide-reference kinds (Backdrop at 1920) are clamped down to this width
  /// when the load targets a smaller class, so the fetched source cannot
  /// exceed [`Self::max_decode_pixels`].
  pub(crate) const fn source_max_width(self) -> u32 {
    match self {
      Self::Card | Self::Hero => 600,
      Self::Backdrop => 1920,
      Self::Avatar => 128,
    }
  }
}

/// A Library Image Raster: an in-memory, display-sized RGBA decode of a
/// Library Image, keyed by the image reference and an [`ArtworkSizeClass`].
/// Never persisted; renderers build their handle from it synchronously.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtworkRaster {
  width: u32,
  height: u32,
  pixels: Bytes,
  logo_shadow: Option<Box<Self>>,
}

impl ArtworkRaster {
  #[must_use]
  pub const fn width(&self) -> u32 {
    self.width
  }

  #[must_use]
  pub const fn height(&self) -> u32 {
    self.height
  }

  #[must_use]
  pub fn byte_len(&self) -> usize {
    self
      .pixels
      .len()
      .saturating_add(self.logo_shadow.as_deref().map_or(0, Self::byte_len))
  }

  #[must_use]
  pub fn logo_shadow(&self) -> Option<&Self> {
    self.logo_shadow.as_deref()
  }

  #[must_use]
  pub fn into_parts(self) -> (u32, u32, Bytes, Option<Self>) {
    (
      self.width,
      self.height,
      self.pixels,
      self.logo_shadow.map(|shadow| *shadow),
    )
  }

  #[must_use]
  pub fn into_pixels(self) -> Bytes {
    self.pixels
  }
}

#[cfg(any(test, feature = "test-utils"))]
impl ArtworkRaster {
  #[must_use]
  pub fn from_raw_for_test(width: u32, height: u32, pixels: impl Into<Bytes>) -> Self {
    Self {
      width,
      height,
      pixels: pixels.into(),
      logo_shadow: None,
    }
  }
}

/// Derived raster variants baked alongside the main decode. Part of the cache
/// and coalescing identity: plain and shadow-bearing variants of the same image
/// coexist independently.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DerivedArtwork {
  /// Soft drop shadow baked from the source alpha for Title Logos.
  pub logo_shadow: bool,
}

/// Cache and coalescing key for one decoded Library Image Raster.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RasterKey {
  image_id: Arc<str>,
  size_class: ArtworkSizeClass,
  derived: DerivedArtwork,
}

/// Per-load identity and scheduling context carried through fetch and decode.
struct LoadContext {
  key: RasterKey,
  generation: u64,
  attempt: u64,
}

/// Decodes encoded bytes into a display-sized Library Image Raster.
///
/// Applies EXIF orientation as iced's image loader does, then downsamples to
/// the size class target box (shrink-only, aspect preserved). Animated
/// containers are rejected before decode.
#[cfg(test)]
fn decode_raster(
  bytes: &ArtworkBytes,
  size_class: ArtworkSizeClass,
) -> Result<ArtworkRaster, ArtworkError> {
  decode_raster_with_derived(bytes, size_class, DerivedArtwork::default())
}

fn decode_raster_with_derived(
  bytes: &ArtworkBytes,
  size_class: ArtworkSizeClass,
  derived: DerivedArtwork,
) -> Result<ArtworkRaster, ArtworkError> {
  use image::ImageDecoder as _;

  validate_static_image_container(bytes.as_slice())?;
  let reader = image::ImageReader::new(Cursor::new(bytes.as_slice()))
    .with_guessed_format()
    .map_err(|_| ArtworkError::DecodeFailed)?;
  let mut decoder = reader
    .into_decoder()
    .map_err(|_| ArtworkError::DecodeFailed)?;
  let orientation = decoder
    .orientation()
    .unwrap_or(image::metadata::Orientation::NoTransforms);
  // Reject oversized sources before the full-size buffer is allocated;
  // max_alloc backstops wider intermediates (e.g. 16-bit PNG decode buffers).
  let (source_width, source_height) = decoder.dimensions();
  if u64::from(source_width) * u64::from(source_height) > size_class.max_decode_pixels() {
    return Err(ArtworkError::DecodedImageTooLarge);
  }
  let mut limits = image::Limits::default();
  limits.max_alloc = Some(2 * size_class.max_decode_bytes() as u64);
  let _ = decoder.set_limits(limits);
  let mut decoded =
    image::DynamicImage::from_decoder(decoder).map_err(|_| ArtworkError::DecodeFailed)?;
  decoded.apply_orientation(orientation);
  let (target_width, target_height) = size_class.target_box();
  // `thumbnail` scales to fill the box even for smaller images; skip it to
  // keep the downsample shrink-only.
  let sized = if decoded.width() > target_width || decoded.height() > target_height {
    decoded.thumbnail(target_width, target_height)
  } else {
    decoded
  };
  let rgba = sized.to_rgba8();
  let logo_shadow = derived
    .logo_shadow
    .then(|| Box::new(generate_logo_shadow(&rgba)));
  let (width, height) = rgba.dimensions();
  let pixels = Bytes::from(rgba.into_raw());
  if pixels.len() > size_class.max_raster_bytes() {
    return Err(ArtworkError::DecodedImageTooLarge);
  }
  Ok(ArtworkRaster {
    width,
    height,
    pixels,
    logo_shadow,
  })
}

/// Bakes a soft drop shadow from the source alpha in two stacked falloffs — a
/// tight core that keeps the glyph contour readable and a wide diffuse tail so
/// no hard boundary shows. The canvas grows a transparent margin of height/4
/// on top, bottom, and right so the halo never clips there; the left side
/// stays flush (glyphs carry their own left margins and the logo must keep
/// left alignment). The glyph sits at (0, pad) and renderers indent the logo
/// on top only by pad x scale — a constant 3/2 render ratio.
fn generate_logo_shadow(source: &image::RgbaImage) -> ArtworkRaster {
  let pad = (source.height() / 4).max(1);
  let padded_width = source.width() + 2 * pad;
  let padded_height = source.height() + 2 * pad;
  let mut padded = image::RgbaImage::new(padded_width, padded_height);
  image::imageops::overlay(&mut padded, source, 0, i64::from(pad));
  let mut tight = shadow_layer(&padded, 28);
  let mut diffuse = shadow_layer(&padded, 12);
  box_blur_three_passes(&mut tight, logo_shadow_tight_radius(padded_width));
  box_blur_three_passes(&mut diffuse, logo_shadow_diffuse_radius(padded_width));
  // Source-over the diffuse tail under the tight core; RGB stays zeroed.
  for (core, tail) in tight.pixels_mut().zip(diffuse.pixels()) {
    let a1 = u16::from(core.0[3]);
    let a2 = u16::from(tail.0[3]);
    core.0[3] = (a1 + a2 * (255 - a1) / 255) as u8;
  }
  ArtworkRaster {
    width: padded_width,
    height: padded_height,
    pixels: Bytes::from(tight.into_raw()),
    logo_shadow: None,
  }
}

fn shadow_layer(source: &image::RgbaImage, alpha_percent: u16) -> image::RgbaImage {
  let mut layer = source.clone();
  for pixel in layer.pixels_mut() {
    pixel.0[0] = 0;
    pixel.0[1] = 0;
    pixel.0[2] = 0;
    pixel.0[3] = (u16::from(pixel.0[3]) * alpha_percent / 100) as u8;
  }
  layer
}

fn logo_shadow_tight_radius(source_width: u32) -> u32 {
  (source_width / 240).clamp(2, 3)
}

fn logo_shadow_diffuse_radius(source_width: u32) -> u32 {
  (source_width / 53).clamp(8, 14)
}

fn box_blur_three_passes(image: &mut image::RgbaImage, radius: u32) {
  if radius == 0 || image.width() == 0 || image.height() == 0 {
    return;
  }
  let mut scratch = image::RgbaImage::new(image.width(), image.height());
  for _ in 0..3 {
    box_blur_horizontal(image, &mut scratch, radius);
    box_blur_vertical(&scratch, image, radius);
  }
}

fn box_blur_horizontal(source: &image::RgbaImage, target: &mut image::RgbaImage, radius: u32) {
  let width = source.width();
  let kernel_width = radius.saturating_mul(2).saturating_add(1);
  // Seed once per row, then slide by one leaving and one entering pixel so
  // every output pixel stays O(1), including Backdrop-scale blur radii.
  for y in 0..source.height() {
    let mut sums = [0_u64; 4];
    for offset in 0..kernel_width {
      let x = offset.saturating_sub(radius).min(width - 1);
      for (sum, channel) in sums.iter_mut().zip(source.get_pixel(x, y).0) {
        *sum += u64::from(channel);
      }
    }
    for x in 0..width {
      target.put_pixel(
        x,
        y,
        image::Rgba(sums.map(|sum| (sum / u64::from(kernel_width)) as u8)),
      );
      let leaving = x.saturating_sub(radius);
      let entering = x.saturating_add(radius).saturating_add(1).min(width - 1);
      for ((sum, left), right) in sums
        .iter_mut()
        .zip(source.get_pixel(leaving, y).0)
        .zip(source.get_pixel(entering, y).0)
      {
        *sum = sum.saturating_sub(u64::from(left)) + u64::from(right);
      }
    }
  }
}

fn box_blur_vertical(source: &image::RgbaImage, target: &mut image::RgbaImage, radius: u32) {
  let height = source.height();
  let kernel_height = radius.saturating_mul(2).saturating_add(1);
  // As above, the radius affects only the initial column sum, not per-pixel
  // kernel work.
  for x in 0..source.width() {
    let mut sums = [0_u64; 4];
    for offset in 0..kernel_height {
      let y = offset.saturating_sub(radius).min(height - 1);
      for (sum, channel) in sums.iter_mut().zip(source.get_pixel(x, y).0) {
        *sum += u64::from(channel);
      }
    }
    for y in 0..height {
      target.put_pixel(
        x,
        y,
        image::Rgba(sums.map(|sum| (sum / u64::from(kernel_height)) as u8)),
      );
      let leaving = y.saturating_sub(radius);
      let entering = y.saturating_add(radius).saturating_add(1).min(height - 1);
      for ((sum, top), bottom) in sums
        .iter_mut()
        .zip(source.get_pixel(x, leaving).0)
        .zip(source.get_pixel(x, entering).0)
      {
        *sum = sum.saturating_sub(u64::from(top)) + u64::from(bottom);
      }
    }
  }
}

/// Where a Library Image load obtained its result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkSource {
  /// Synchronous hit on the Library Image Raster cache.
  Raster,
  /// Encoded bytes came from the in-memory Library Image cache.
  Memory,
  /// Encoded bytes came from the on-disk Library Image Cache.
  Disk,
  /// Encoded bytes came from the media server.
  Network,
}

impl ArtworkSource {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Raster => "raster",
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
  pub raster_loads: u64,
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
/// and the byte size of what it served (raster pixels for raster hits,
/// encoded bytes otherwise). Never carries URLs or image references, so it
/// can feed the user-facing Diagnostics view aggregates directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtworkLoadObservation {
  pub settlement: ArtworkLoadSettlement,
  pub duration: Duration,
  pub bytes: u64,
}

impl ArtworkLoadObservation {
  /// Observation for a synchronous Library Image Raster cache hit on a caller
  /// fast path.
  #[must_use]
  pub const fn raster_hit(bytes: u64) -> Self {
    Self {
      settlement: ArtworkLoadSettlement::Loaded(ArtworkSource::Raster),
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
      ArtworkSource::Raster => self.raster_loads = self.raster_loads.saturating_add(1),
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
      .raster_loads
      .saturating_add(self.memory_loads)
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
      "Library images settled: {settled} ({} raster, {} memory, {} disk, {} network, \
      {} failed); {} bytes loaded; average {} ms.",
      self.raster_loads,
      self.memory_loads,
      self.disk_loads,
      self.network_loads,
      self.failed_loads,
      self.total_bytes,
      average_millis,
    ))
  }
}

type AdapterFetchResult = Result<(ArtworkBytes, ArtworkSource), ArtworkError>;

/// Authenticated, bounded, coalescing artwork pipeline.
///
/// Loads decode to a Library Image Raster sized by an [`ArtworkSizeClass`]:
/// the raster cache (byte-budgeted) is consulted first, then the encoded
/// memory cache, coalesced in-flight work, the disk cache, and the network.
#[derive(Clone)]
pub struct ArtworkAdapter {
  state: Arc<Mutex<AdapterState>>,
  generation_sender: watch::Sender<u64>,
  limits: ArtworkLimits,
  disk_cache: ArtworkDiskCache,
}

impl Default for ArtworkAdapter {
  fn default() -> Self {
    Self::with_limits(ArtworkLimits::default())
  }
}

impl ArtworkAdapter {
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  #[must_use]
  pub fn with_limits(limits: ArtworkLimits) -> Self {
    Self::with_limits_and_disk_cache(limits, ArtworkDiskCache::default())
  }

  #[must_use]
  pub fn with_limits_and_disk_cache(limits: ArtworkLimits, disk_cache: ArtworkDiskCache) -> Self {
    let limits = limits.normalized();
    let (generation_sender, _) = watch::channel(0);
    Self {
      state: Arc::new(Mutex::new(AdapterState {
        generation: 0,
        cache_generation: 0,
        encoded_cache: ArtworkCache::new(limits.max_cached_bytes, limits.max_cached_entries),
        raster_cache: ArtworkCache::new(
          limits.max_raster_cached_bytes,
          limits.max_raster_cached_entries,
        ),
        in_flight: HashMap::new(),
        next_identity: 0,
        admission_changed: Arc::new(Notify::new()),
        scheduler: LoadBudget::default(),
      })),
      generation_sender,
      limits,
      disk_cache,
    }
  }

  #[must_use]
  pub fn ticket(&self) -> ArtworkLoadTicket {
    ArtworkLoadTicket::new(self.lock_state().generation)
  }

  /// Synchronous fast path: returns the cached Library Image Raster for this
  /// reference and size class, when present.
  pub fn cached(&self, image_id: &str, size_class: ArtworkSizeClass) -> Option<ArtworkRaster> {
    self.cached_with_derived(image_id, size_class, DerivedArtwork::default())
  }

  /// Synchronous fast path for a raster with derived variants.
  pub fn cached_with_derived(
    &self,
    image_id: &str,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
  ) -> Option<ArtworkRaster> {
    self.lock_state().raster_cache.get(&RasterKey {
      image_id: Arc::from(image_id),
      size_class,
      derived,
    })
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

  /// Fetches and decodes an opaque signed image reference into a Library
  /// Image Raster sized by `size_class`.
  ///
  /// `lane` classifies the load's scheduling priority: [`LoadLane::Visible`]
  /// loads drain before queued [`LoadLane::Offscreen`] work. The returned
  /// observation carries the load's sanitized telemetry so callers can
  /// aggregate per surface instead of sharing process-wide state.
  pub async fn load(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    size_class: ArtworkSizeClass,
    lane: LoadLane,
  ) -> (Result<ArtworkRaster, ArtworkError>, ArtworkLoadObservation) {
    self
      .load_with_ticket(
        client,
        image_id,
        size_class,
        DerivedArtwork::default(),
        self.ticket(),
        lane,
      )
      .await
  }

  /// Fetches and decodes artwork with derived variants.
  pub async fn load_with_derived(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
    lane: LoadLane,
  ) -> (Result<ArtworkRaster, ArtworkError>, ArtworkLoadObservation) {
    self
      .load_with_ticket(client, image_id, size_class, derived, self.ticket(), lane)
      .await
  }

  /// Fetches and decodes an image only while `ticket` belongs to the current generation.
  pub async fn load_with_ticket(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
    ticket: ArtworkLoadTicket,
    lane: LoadLane,
  ) -> (Result<ArtworkRaster, ArtworkError>, ArtworkLoadObservation) {
    match self.demand_inner(client, image_id, size_class, derived, ticket, lane) {
      ArtworkDemand::Ready(settlement) => settlement,
      ArtworkDemand::Pending { control, receiver } => {
        let settlement = receiver.await.unwrap_or_else(|_| cancelled_settlement());
        drop(control);
        settlement
      }
    }
  }

  /// Registers independent live demand; cache hits and rejected requests settle synchronously.
  pub fn demand(
    self: &Arc<Self>,
    client: Arc<JellyfinClient>,
    image_id: String,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
    ticket: ArtworkLoadTicket,
    lane: LoadLane,
  ) -> ArtworkDemand {
    self.demand_inner(&client, &image_id, size_class, derived, ticket, lane)
  }

  fn demand_inner(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
    ticket: ArtworkLoadTicket,
    lane: LoadLane,
  ) -> ArtworkDemand {
    let rejected = |error| {
      ArtworkDemand::Ready((
        Err(error),
        ArtworkLoadObservation {
          settlement: if error == ArtworkError::Cancelled {
            ArtworkLoadSettlement::Cancelled
          } else {
            ArtworkLoadSettlement::Failed
          },
          duration: Duration::ZERO,
          bytes: 0,
        },
      ))
    };
    if let Err(error) = validate_image_reference(image_id) {
      return rejected(error);
    }
    // Authorization precedes even decoded-cache access.
    let Ok(request) = client
      .library()
      .image_request_with_max_width(image_id, size_class.source_max_width())
    else {
      return rejected(ArtworkError::RequestRejected);
    };
    let key = RasterKey {
      image_id: Arc::from(image_id),
      size_class,
      derived,
    };
    let mut state = self.lock_state();
    if ticket.generation() != state.generation {
      return rejected(ArtworkError::Cancelled);
    }
    if let Some(raster) = state.raster_cache.get(&key) {
      let observation = ArtworkLoadObservation::raster_hit(raster.byte_len() as u64);
      return ArtworkDemand::Ready((Ok(raster), observation));
    }
    let Some(id) = state.next_identity.checked_add(1) else {
      return rejected(ArtworkError::Overloaded);
    };
    state.next_identity = id;
    let (sender, receiver) = oneshot::channel();
    let consumer = DemandConsumer {
      lane,
      sender,
      started: Instant::now(),
    };
    let attempt = if let Some(load) = state.in_flight.get_mut(&key) {
      let attempt = load.attempt;
      load.demands.insert(id, consumer);
      state.admission_changed.notify_waiters();
      attempt
    } else {
      let generation = state.generation;
      let adapter = self.clone();
      let client = client.clone();
      let context = LoadContext {
        key: key.clone(),
        generation,
        attempt: id,
      };
      // Spawn while holding the registry lock so a fast completion cannot
      // race installation. The worker, never the initiating consumer, owns work.
      let worker = tokio::spawn(async move {
        adapter.run_load(client, request, context).await;
      });
      state.in_flight.insert(
        key.clone(),
        InFlightLoad {
          attempt: id,
          demands: HashMap::from([(id, consumer)]),
          waiting: true,
          worker: worker.abort_handle(),
        },
      );
      state.admission_changed.notify_waiters();
      id
    };
    ArtworkDemand::Pending {
      control: ArtworkDemandControl {
        state: Arc::clone(&self.state),
        key,
        attempt,
        id,
      },
      receiver,
    }
  }

  async fn run_load(
    &self,
    client: JellyfinClient,
    request: LibraryImageRequest,
    context: LoadContext,
  ) {
    let started = Instant::now();
    let span = tracing::info_span!(
      "library_image_load",
      source = tracing::field::Empty,
      encoded_bytes = tracing::field::Empty,
      duration_ms = tracing::field::Empty
    );
    let mut generation = self.generation_sender.subscribe();
    let result = match self.acquire_load_permit(&context, &mut generation).await {
      Ok(permit) => {
        self
          .fetch_and_decode(&client, &request, &context, permit, &mut generation)
          .await
      }
      Err(error) => Err(error),
    };
    let (settlement, bytes) = match &result {
      Ok((_, source, bytes)) => (ArtworkLoadSettlement::Loaded(*source), *bytes),
      Err(ArtworkError::Cancelled) => (ArtworkLoadSettlement::Cancelled, 0),
      Err(_) => (ArtworkLoadSettlement::Failed, 0),
    };
    let observation = finish_load(&span, started, settlement, bytes);
    self.finish_pending(&context, (result.map(|(raster, _, _)| raster), observation));
  }

  /// Cancels queued and network work from the previous consumer generation.
  pub fn cancel_pending(&self) {
    self.advance_generation(false);
  }

  /// Cancels pending work and clears decoded data from the previous session.
  pub fn reset_session(&self) {
    self.advance_generation(true);
  }

  /// Clears the encoded and raster caches without cancelling in-flight loads.
  ///
  /// Unlike [`Self::reset_session`], in-flight loads still finish and their
  /// results re-enter the caches. Used when the shell drops browse surfaces
  /// while playback artwork may still be loading.
  pub fn clear_caches(&self) {
    let mut state = self.lock_state();
    state.encoded_cache.clear();
    state.raster_cache.clear();
  }

  fn advance_generation(&self, clear_cache: bool) {
    let waiters = {
      let mut state = self.lock_state();
      state.generation = state.generation.wrapping_add(1);
      let generation = state.generation;
      if clear_cache {
        state.cache_generation = generation;
      }
      let waiters = state.cancel_stale(clear_cache);
      let _ = self.generation_sender.send_replace(generation);
      waiters
    };
    notify_cancelled(waiters);
  }

  async fn acquire_load_permit(
    &self,
    context: &LoadContext,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<LoadPermit, ArtworkError> {
    let changed = Arc::clone(&self.lock_state().admission_changed);
    let reservation = self
      .limits
      .load_reservation_bytes_with_derived(context.key.size_class, context.key.derived);
    {
      let mut state = self.lock_state();
      let Some(load) = state
        .in_flight
        .get_mut(&context.key)
        .filter(|load| load.attempt == context.attempt)
      else {
        return Err(ArtworkError::Cancelled);
      };
      load.waiting = true;
    }
    loop {
      // Register before checking admission so release cannot be lost between
      // the check and suspension. Waiting work holds metadata, never permits.
      let notified = changed.notified();
      tokio::pin!(notified);
      notified.as_mut().enable();
      {
        let mut state = self.lock_state();
        if state.generation != context.generation
          || !state
            .in_flight
            .get(&context.key)
            .is_some_and(|load| load.attempt == context.attempt)
        {
          return Err(ArtworkError::Cancelled);
        }
        let next = state
          .in_flight
          .values()
          .filter(|load| load.waiting)
          .min_by_key(|load| (load.lane() != LoadLane::Visible, load.attempt))
          .map(|load| load.attempt);
        // Demand is admitted directly when physical capacity is available;
        // it need not occupy a bounded physical scheduler queue beforehand.
        if next == Some(context.attempt)
          && state.scheduler.try_reserve(
            self.limits.max_active_loads,
            self.limits.max_active_bytes,
            reservation,
          )
        {
          if let Some(load) = state.in_flight.get_mut(&context.key) {
            load.waiting = false;
          }
          changed.notify_waiters();
          return Ok(LoadPermit {
            state: Arc::clone(&self.state),
            reserved_bytes: reservation,
          });
        }
      }
      tokio::select! {
        () = notified => {}
        changed = generation.changed() => {
          let _ = changed;
          return Err(ArtworkError::Cancelled);
        }
      }
    }
  }

  async fn fetch_and_decode(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
    context: &LoadContext,
    permit: LoadPermit,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<(ArtworkRaster, ArtworkSource, usize), ArtworkError> {
    let size_class = context.key.size_class;
    let (bytes, source) = tokio::select! {
      result = self.original_bytes(client, request, context.key.image_id.clone(), context.generation) => result?,
      changed = generation.changed() => {
        let _ = changed;
        return Err(ArtworkError::Cancelled);
      }
    };
    let encoded_bytes = bytes.byte_len();
    match self
      .decode_tracked(bytes, size_class, context.key.derived, permit, generation)
      .await
    {
      Ok(raster) => Ok((raster, source, encoded_bytes)),
      Err(ArtworkError::DecodedImageTooLarge) if source != ArtworkSource::Network => {
        self
          .retry_oversized_from_network(client, request, context, generation)
          .await
      }
      Err(error) => Err(error),
    }
  }

  /// Retries a load whose cached origin bytes exceed the decode budget. The
  /// cached copies predate the budget or were poisoned by an origin that once
  /// served the full-size image, so every cached copy is dropped and the
  /// origin is asked once for a fresh one. An oversized origin response is
  /// not persisted, so it cannot poison the caches again.
  async fn retry_oversized_from_network(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
    context: &LoadContext,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<(ArtworkRaster, ArtworkSource, usize), ArtworkError> {
    self
      .lock_state()
      .encoded_cache
      .remove(&context.key.image_id);
    self
      .disk_cache
      .remove(artwork_cache_key(
        request.server_url(),
        request.origin_url(),
      ))
      .await;
    let permit = self.acquire_load_permit(context, generation).await?;
    let bytes = tokio::select! {
      result = self.fetch_uncached(client, request) => result?,
      changed = generation.changed() => {
        let _ = changed;
        return Err(ArtworkError::Cancelled);
      }
    };
    let encoded_bytes = bytes.byte_len();
    let stored = bytes.clone();
    let raster = self
      .decode_tracked(
        bytes,
        context.key.size_class,
        context.key.derived,
        permit,
        generation,
      )
      .await?;
    self.store_network_bytes(
      request,
      context.key.image_id.clone(),
      &stored,
      context.generation,
    );
    Ok((raster, ArtworkSource::Network, encoded_bytes))
  }

  /// Runs one blocking decode under the load's byte reservation.
  async fn decode_tracked(
    &self,
    bytes: ArtworkBytes,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
    permit: LoadPermit,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<ArtworkRaster, ArtworkError> {
    let decode = tokio::task::spawn_blocking(move || {
      // Cancellation may drop the join handle, so the blocking decode owns
      // aggregate admission until it actually stops.
      let _permit = permit;
      decode_raster_with_derived(&bytes, size_class, derived)
    });
    tokio::select! {
      result = decode => result.map_err(|_| ArtworkError::DecodeFailed)?,
      changed = generation.changed() => {
        let _ = changed;
        Err(ArtworkError::Cancelled)
      }
    }
  }

  /// Resolves the encoded Library Image bytes: encoded memory cache first,
  /// then the disk cache, then the network.
  async fn original_bytes(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
    image_key: Arc<str>,
    load_generation: u64,
  ) -> AdapterFetchResult {
    if let Some(bytes) = self.lock_state().encoded_cache.get(&image_key) {
      return Ok((bytes, ArtworkSource::Memory));
    }
    let disk_key = artwork_cache_key(request.server_url(), request.origin_url());
    if let Some(bytes) = self
      .disk_cache
      .load(
        disk_key.clone(),
        self.limits.max_response_bytes,
        validate_disk_artwork,
      )
      .await
    {
      let bytes = ArtworkBytes(bytes);
      self.cache_encoded(image_key, &bytes, load_generation);
      return Ok((bytes, ArtworkSource::Disk));
    }
    let bytes = self.fetch_uncached(client, request).await?;
    self.store_network_bytes(request, image_key, &bytes, load_generation);
    Ok((bytes, ArtworkSource::Network))
  }

  /// Stores freshly fetched origin bytes in the encoded and disk caches when
  /// the container is a supported still image.
  fn store_network_bytes(
    &self,
    request: &LibraryImageRequest,
    image_key: Arc<str>,
    bytes: &ArtworkBytes,
    load_generation: u64,
  ) {
    if validate_static_image_container(bytes.0.as_ref()).is_err() {
      return;
    }
    self.cache_encoded(image_key, bytes, load_generation);
    let disk_cache = self.disk_cache.clone();
    let disk_key = artwork_cache_key(request.server_url(), request.origin_url());
    let disk_bytes = Arc::clone(&bytes.0);
    tokio::spawn(async move {
      disk_cache.store(disk_key, disk_bytes).await;
    });
  }

  /// Stores encoded bytes fetched by a load, unless the load's session has
  /// been reset since admission (same gate as raster insertion).
  fn cache_encoded(&self, image_key: Arc<str>, bytes: &ArtworkBytes, load_generation: u64) {
    let mut state = self.lock_state();
    if load_generation >= state.cache_generation {
      state.encoded_cache.insert(image_key, bytes.clone());
    }
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

  fn finish_pending(&self, context: &LoadContext, settlement: ArtworkDemandSettlement) {
    let consumers = {
      let mut state = self.lock_state();
      if state.generation != context.generation
        || !state
          .in_flight
          .get(&context.key)
          .is_some_and(|load| load.attempt == context.attempt)
      {
        return;
      }
      if let Ok(raster) = &settlement.0 {
        state
          .raster_cache
          .insert(context.key.clone(), raster.clone());
      }
      state
        .in_flight
        .remove(&context.key)
        .map(|load| load.demands)
        .unwrap_or_default()
    };
    // Attribute the actual physical source to one surviving consumer, even
    // when the initiating consumer left; other consumers report sharing.
    let reporter = consumers.keys().min().copied();
    for (id, consumer) in consumers {
      let mut observation = settlement.1;
      observation.duration = consumer.started.elapsed();
      if Some(id) != reporter && settlement.0.is_ok() {
        observation.settlement = ArtworkLoadSettlement::Shared;
        observation.bytes = settlement
          .0
          .as_ref()
          .map_or(0, |raster| raster.byte_len() as u64);
      }
      let _ = consumer.sender.send((settlement.0.clone(), observation));
    }
  }

  fn lock_state(&self) -> MutexGuard<'_, AdapterState> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }
}

#[cfg(any(test, feature = "test-utils"))]
impl ArtworkAdapter {
  pub fn seed_raster_for_test(
    &self,
    image_id: &str,
    size_class: ArtworkSizeClass,
    raster: ArtworkRaster,
  ) {
    self.seed_raster_with_derived_for_test(image_id, size_class, DerivedArtwork::default(), raster);
  }

  pub fn seed_raster_with_derived_for_test(
    &self,
    image_id: &str,
    size_class: ArtworkSizeClass,
    derived: DerivedArtwork,
    raster: ArtworkRaster,
  ) {
    self.lock_state().raster_cache.insert(
      RasterKey {
        image_id: Arc::from(image_id),
        size_class,
        derived,
      },
      raster,
    );
  }
}

struct AdapterState {
  generation: u64,
  cache_generation: u64,
  encoded_cache: ArtworkCache<Arc<str>, ArtworkBytes>,
  raster_cache: ArtworkCache<RasterKey, ArtworkRaster>,
  in_flight: HashMap<RasterKey, InFlightLoad>,
  next_identity: u64,
  admission_changed: Arc<Notify>,
  scheduler: LoadBudget,
}

impl AdapterState {
  fn cancel_stale(&mut self, clear_cache: bool) -> Vec<oneshot::Sender<ArtworkDemandSettlement>> {
    if clear_cache {
      self.encoded_cache.clear();
      self.raster_cache.clear();
    }
    self.admission_changed.notify_waiters();
    self
      .in_flight
      .drain()
      .flat_map(|(_, load)| {
        load.worker.abort();
        load.demands.into_values().map(|consumer| consumer.sender)
      })
      .collect()
  }
}

struct InFlightLoad {
  attempt: u64,
  demands: HashMap<u64, DemandConsumer>,
  waiting: bool,
  worker: tokio::task::AbortHandle,
}

impl InFlightLoad {
  fn lane(&self) -> LoadLane {
    if self
      .demands
      .values()
      .any(|consumer| consumer.lane == LoadLane::Visible)
    {
      LoadLane::Visible
    } else {
      LoadLane::Offscreen
    }
  }
}

struct DemandConsumer {
  lane: LoadLane,
  sender: oneshot::Sender<ArtworkDemandSettlement>,
  started: Instant,
}

/// A decoded result and its truthful per-consumer load observation.
pub type ArtworkDemandSettlement = (Result<ArtworkRaster, ArtworkError>, ArtworkLoadObservation);

/// Synchronous admission outcome or independently controlled asynchronous demand.
pub enum ArtworkDemand {
  Ready(ArtworkDemandSettlement),
  Pending {
    control: ArtworkDemandControl,
    receiver: oneshot::Receiver<ArtworkDemandSettlement>,
  },
}

/// Owns one demand. Dropping the last demand cancels its physical work.
pub struct ArtworkDemandControl {
  state: Arc<Mutex<AdapterState>>,
  key: RasterKey,
  attempt: u64,
  id: u64,
}

impl ArtworkDemandControl {
  /// Updates this consumer's live scheduling priority without restarting work.
  pub fn set_lane(&self, lane: LoadLane) {
    let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(load) = state
      .in_flight
      .get_mut(&self.key)
      .filter(|load| load.attempt == self.attempt)
    {
      if let Some(consumer) = load.demands.get_mut(&self.id) {
        consumer.lane = lane;
      }
      state.admission_changed.notify_waiters();
    }
  }
}

impl Drop for ArtworkDemandControl {
  fn drop(&mut self) {
    let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(load) = state
      .in_flight
      .get_mut(&self.key)
      .filter(|load| load.attempt == self.attempt)
    else {
      return;
    };
    load.demands.remove(&self.id);
    if load.demands.is_empty() {
      if let Some(load) = state.in_flight.remove(&self.key) {
        load.worker.abort();
      }
    }
    state.admission_changed.notify_waiters();
  }
}

fn cancelled_settlement() -> ArtworkDemandSettlement {
  (
    Err(ArtworkError::Cancelled),
    ArtworkLoadObservation {
      settlement: ArtworkLoadSettlement::Cancelled,
      duration: Duration::ZERO,
      bytes: 0,
    },
  )
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

fn notify_cancelled(waiters: Vec<oneshot::Sender<ArtworkDemandSettlement>>) {
  for waiter in waiters {
    let _ = waiter.send(cancelled_settlement());
  }
}

struct LoadPermit {
  state: Arc<Mutex<AdapterState>>,
  reserved_bytes: usize,
}

impl Drop for LoadPermit {
  fn drop(&mut self) {
    let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
    state.scheduler.release(self.reserved_bytes);
    state.admission_changed.notify_waiters();
  }
}

#[cfg(test)]
#[path = "artwork_demand_tests.rs"]
mod demand_tests;

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

/// A value stored in a byte-budgeted artwork cache.
trait CacheValue: Clone + Send + Sync + 'static {
  fn byte_len(&self) -> usize;
}

impl CacheValue for ArtworkBytes {
  fn byte_len(&self) -> usize {
    self.0.len()
  }
}

impl CacheValue for ArtworkRaster {
  fn byte_len(&self) -> usize {
    self.byte_len()
  }
}

/// Clock-LRU cache bounded by total bytes and entry count; shared by the
/// encoded Library Image memory cache and the Library Image Raster cache.
struct ArtworkCache<K, T>
where
  K: Eq + std::hash::Hash + Ord + Clone,
  T: CacheValue,
{
  entries: HashMap<K, CacheEntry<T>>,
  total_bytes: usize,
  clock: u64,
  max_bytes: usize,
  max_entries: usize,
}

struct CacheEntry<T> {
  artwork: T,
  last_used: u64,
}

impl<K, T> ArtworkCache<K, T>
where
  K: Eq + std::hash::Hash + Ord + Clone,
  T: CacheValue,
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

  fn get(&mut self, key: &K) -> Option<T> {
    let entry = self.entries.get_mut(key)?;
    self.clock = self.clock.saturating_add(1);
    entry.last_used = self.clock;
    Some(entry.artwork.clone())
  }

  fn clear(&mut self) {
    self.entries.clear();
    self.total_bytes = 0;
  }

  fn remove(&mut self, key: &K) {
    if let Some(previous) = self.entries.remove(key) {
      self.total_bytes = self.total_bytes.saturating_sub(previous.artwork.byte_len());
    }
  }

  fn insert(&mut self, key: K, artwork: T) {
    let artwork_bytes = artwork.byte_len();
    if self.max_bytes == 0 || self.max_entries == 0 || artwork_bytes > self.max_bytes {
      return;
    }
    if let Some(previous) = self.entries.remove(&key) {
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
        .map(|(key, _)| key.clone())
      else {
        break;
      };
      if let Some(removed) = self.entries.remove(&oldest) {
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

  pub(super) fn raster(width: u32, height: u32) -> ArtworkRaster {
    let pixels = vec![7u8; width as usize * height as usize * 4];
    ArtworkRaster::from_raw_for_test(width, height, Bytes::from(pixels))
  }

  fn class_key(image_id: &str, size_class: ArtworkSizeClass) -> RasterKey {
    RasterKey {
      image_id: Arc::from(image_id),
      size_class,
      derived: DerivedArtwork::default(),
    }
  }

  fn activate_permit(adapter: &ArtworkAdapter) -> LoadPermit {
    let reservation = adapter
      .limits
      .load_reservation_bytes(ArtworkSizeClass::Card);
    assert!(adapter.lock_state().scheduler.try_reserve(
      adapter.limits.max_active_loads,
      adapter.limits.max_active_bytes,
      reservation,
    ));
    LoadPermit {
      state: Arc::clone(&adapter.state),
      reserved_bytes: reservation,
    }
  }

  pub(super) fn adopt_session(client: &JellyfinClient, server_url: &str, user_id: &str) {
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

  pub(super) fn image_id(server_url: &str) -> String {
    image_id_for_url(
      MediaServerProvider::Jellyfin,
      server_url,
      format!("{server_url}/Items/1/Images/Primary"),
      ImageRefKind::Artwork,
    )
    .expect("image reference is valid")
  }

  /// Encodes a deterministic RGBA image of the given size as PNG.
  pub(super) fn encode_test_png(width: u32, height: u32) -> Vec<u8> {
    let buffer = image::RgbaImage::from_pixel(width, height, image::Rgba([3, 90, 200, 255]));
    let mut encoded = Cursor::new(Vec::new());
    buffer
      .write_to(&mut encoded, image::ImageFormat::Png)
      .expect("test image encodes");
    encoded.into_inner()
  }

  /// Encodes a JPEG carrying an EXIF orientation tag.
  fn encode_oriented_jpeg(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    let buffer = image::RgbImage::from_pixel(width, height, image::Rgb([12, 34, 56]));
    let mut encoded = Cursor::new(Vec::new());
    buffer
      .write_to(&mut encoded, image::ImageFormat::Jpeg)
      .expect("test image encodes");
    let jpeg = encoded.into_inner();
    assert!(jpeg.starts_with(&[0xff, 0xd8]));

    // Minimal EXIF APP1 segment: "Exif\0\0" + little-endian TIFF header + one
    // IFD entry for tag 0x0112 (Orientation).
    let mut exif = Vec::new();
    exif.extend_from_slice(&[0xff, 0xe1, 0x00, 0x22]); // APP1, length 34
    exif.extend_from_slice(b"Exif\0\0");
    exif.extend_from_slice(b"II\x2a\x00"); // little-endian TIFF magic
    exif.extend_from_slice(&8u32.to_le_bytes()); // IFD offset
    exif.extend_from_slice(&1u16.to_le_bytes()); // one entry
    exif.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation tag
    exif.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    exif.extend_from_slice(&1u32.to_le_bytes()); // count
    exif.extend_from_slice(&u32::from(orientation).to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes()); // no next IFD

    let mut oriented = jpeg[..2].to_vec();
    oriented.extend_from_slice(&exif);
    oriented.extend_from_slice(&jpeg[2..]);
    oriented
  }

  #[test]
  fn aggregate_reservation_matches_plain_and_shadowed_load_shapes() {
    let limits = ArtworkLimits::default();

    for size_class in [
      ArtworkSizeClass::Card,
      ArtworkSizeClass::Hero,
      ArtworkSizeClass::Backdrop,
    ] {
      let base = limits.max_response_bytes
        + size_class.max_decode_bytes()
        + size_class.max_raster_bytes() * DECODE_PIXEL_BUFFER_RESERVATIONS;
      assert_eq!(limits.load_reservation_bytes(size_class), base);
      assert_eq!(
        limits.load_reservation_bytes_with_derived(size_class, DerivedArtwork::default()),
        base
      );
      assert_eq!(
        limits
          .load_reservation_bytes_with_derived(size_class, DerivedArtwork { logo_shadow: true },),
        base + size_class.max_logo_shadow_bytes()
      );
    }
  }

  #[test]
  fn normalized_budget_admits_a_maximum_backdrop() {
    let limits = ArtworkLimits {
      max_active_bytes: 0,
      ..ArtworkLimits::default()
    }
    .normalized();
    let mut scheduler = LoadBudget::default();

    assert!(scheduler.try_reserve(
      limits.max_active_loads,
      limits.max_active_bytes,
      limits.load_reservation_bytes(ArtworkSizeClass::Backdrop),
    ));
  }

  #[test]
  fn aggregate_reservation_saturates_on_overflow() {
    let limits = ArtworkLimits {
      max_response_bytes: usize::MAX,
      ..ArtworkLimits::default()
    };

    assert_eq!(
      limits.load_reservation_bytes(ArtworkSizeClass::Backdrop),
      usize::MAX
    );
  }

  #[test]
  fn scheduler_bounds_active_loads_and_aggregate_bytes() {
    let mut scheduler = LoadBudget::default();
    assert!(scheduler.try_reserve(2, 100, 40));
    assert!(scheduler.try_reserve(2, 100, 40));
    assert!(!scheduler.try_reserve(2, 100, 40));
    scheduler.release(40);
    assert!(!scheduler.try_reserve(2, 100, 61));
    assert!(scheduler.try_reserve(2, 100, 60));
  }

  #[test]
  fn cached_admission_reports_a_raster_observation() {
    let adapter = ArtworkAdapter::default();
    let client = JellyfinClient::new();
    let server_url = "https://server.example.com";
    let reference = image_id(server_url);
    adopt_session(&client, server_url, "user");
    adapter.seed_raster_for_test(&reference, ArtworkSizeClass::Card, raster(1, 1));
    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .expect("runtime builds");

    let (result, observation) = runtime.block_on(adapter.load(
      &client,
      &reference,
      ArtworkSizeClass::Card,
      LoadLane::Visible,
    ));

    assert!(result.is_ok());
    assert_eq!(
      observation.settlement,
      ArtworkLoadSettlement::Loaded(ArtworkSource::Raster)
    );
    assert_eq!(observation.bytes, 4);
  }

  #[test]
  fn summary_folds_observations_and_skips_shared_and_cancelled_loads() {
    let mut summary = ArtworkLoadSummary::default();
    summary.record(&ArtworkLoadObservation::raster_hit(10));
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

    assert_eq!(summary.raster_loads, 1);
    assert_eq!(summary.network_loads, 1);
    assert_eq!(summary.failed_loads, 1);
    assert_eq!(summary.total_bytes, 100);
    assert_eq!(summary.total_duration_millis, 40);
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
  fn reset_session_clears_caches() {
    let adapter = ArtworkAdapter::default();
    {
      let mut state = adapter.lock_state();
      state
        .raster_cache
        .insert(class_key("cached", ArtworkSizeClass::Card), raster(1, 1));
      state
        .encoded_cache
        .insert(Arc::from("encoded"), artwork(&[1, 2, 3, 4]));
    }

    adapter.reset_session();

    let mut state = adapter.lock_state();
    assert!(state
      .raster_cache
      .get(&class_key("cached", ArtworkSizeClass::Card))
      .is_none());
    assert!(state.encoded_cache.get(&Arc::from("encoded")).is_none());
  }

  #[test]
  fn oversized_cached_bytes_are_dropped_and_refetched_from_the_network() {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .expect("runtime builds");
    runtime.block_on(async {
      use tokio::io::{AsyncReadExt, AsyncWriteExt};

      let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener binds");
      let port = listener.local_addr().expect("listener address").port();
      let small = encode_test_png(100, 150);
      let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accepts one load");
        let mut head = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !head.windows(4).any(|window| window == b"\r\n\r\n") {
          let read = socket.read(&mut buffer).await.expect("request reads");
          if read == 0 {
            break;
          }
          head.extend_from_slice(&buffer[..read]);
        }
        let response = format!(
          "HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
          small.len()
        );
        socket
          .write_all(response.as_bytes())
          .await
          .expect("response head writes");
        socket.write_all(&small).await.expect("response body writes");
      });

      let server_url = format!("http://127.0.0.1:{port}");
      let client = JellyfinClient::new();
      adopt_session(&client, &server_url, "user");
      let reference = image_id(&server_url);
      let cache_root = std::env::temp_dir().join(format!(
        "jellypilot-artwork-test-{}-oversized",
        std::process::id()
      ));
      let adapter = ArtworkAdapter::with_limits_and_disk_cache(
        ArtworkLimits::default(),
        crate::ArtworkDiskCache::new(cache_root.clone(), 1 << 20, true),
      );
      // 1300×1300 exceeds the Card/Hero source cap, as a stale origin-sized
      // cache entry from before the decode budget does.
      let oversized = artwork(&encode_test_png(1300, 1300));
      let generation = adapter.ticket().generation();
      adapter.cache_encoded(Arc::from(reference.as_str()), &oversized, generation);

      let (result, observation) = adapter
        .load(&client, &reference, ArtworkSizeClass::Card, LoadLane::Visible)
        .await;

      let raster = result.expect("oversized cached bytes are refetched from the origin");
      assert_eq!((raster.width(), raster.height()), (100, 150));
      assert!(matches!(
        observation.settlement,
        ArtworkLoadSettlement::Loaded(ArtworkSource::Network)
      ));
      server.await.expect("server serves the refetch");
      let _ = std::fs::remove_dir_all(cache_root);
    });
  }

  #[test]
  fn user_image_loads_through_adapter_with_avatar_kind() {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .expect("runtime builds");
    runtime.block_on(async {
      use tokio::io::{AsyncReadExt, AsyncWriteExt};

      let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener binds");
      let port = listener.local_addr().expect("listener address").port();
      let png = encode_test_png(96, 96);
      let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accepts one load");
        let mut head = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !head.windows(4).any(|window| window == b"\r\n\r\n") {
          let read = socket.read(&mut buffer).await.expect("request reads");
          if read == 0 {
            break;
          }
          head.extend_from_slice(&buffer[..read]);
        }
        let response = format!(
          "HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
          png.len()
        );
        socket
          .write_all(response.as_bytes())
          .await
          .expect("response head writes");
        socket.write_all(&png).await.expect("response body writes");
        String::from_utf8_lossy(&head).into_owned()
      });

      let server_url = format!("http://127.0.0.1:{port}");
      let client = JellyfinClient::new();
      adopt_session(&client, &server_url, "user-1");
      let reference = crate::user_image_id(MediaServerProvider::Jellyfin, &server_url, "user-1")
        .expect("user image ref is valid");
      let cache_root = std::env::temp_dir().join(format!(
        "jellypilot-artwork-test-{}-avatar",
        std::process::id()
      ));
      let adapter = ArtworkAdapter::with_limits_and_disk_cache(
        ArtworkLimits::default(),
        crate::ArtworkDiskCache::new(cache_root.clone(), 1 << 20, true),
      );

      let (result, _observation) = adapter
        .load(&client, &reference, ArtworkSizeClass::Avatar, LoadLane::Offscreen)
        .await;

      let raster = result.expect("user image should load through the adapter");
      assert_eq!((raster.width(), raster.height()), (96, 96));
      let head = server.await.expect("server serves the request");
      assert!(
        head.contains("/Users/user-1/Images/Primary?maxWidth=128"),
        "avatar origin should hit the user image route at 128px: {head}"
      );
      let _ = std::fs::remove_dir_all(cache_root);
    });
  }

  #[test]
  fn cached_raster_is_revalidated_against_the_current_client_session() {
    let adapter = ArtworkAdapter::default();
    let client = JellyfinClient::new();
    let first_server = "https://first.example.com";
    let reference = image_id(first_server);
    let cached = raster(1, 1);
    let cached_pixels = cached.pixels.as_ptr();
    adopt_session(&client, first_server, "first-user");
    adapter.seed_raster_for_test(&reference, ArtworkSizeClass::Card, cached);
    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .expect("runtime builds");

    let (accepted, _) = runtime.block_on(adapter.load(
      &client,
      &reference,
      ArtworkSizeClass::Card,
      LoadLane::Visible,
    ));
    let accepted = accepted.expect("current session accepts cache hit");
    assert_eq!(accepted.pixels.as_ptr(), cached_pixels);

    adopt_session(&client, "https://second.example.com", "second-user");
    assert!(matches!(
      runtime
        .block_on(adapter.load(
          &client,
          &reference,
          ArtworkSizeClass::Card,
          LoadLane::Visible
        ))
        .0,
      Err(ArtworkError::RequestRejected)
    ));
  }

  #[test]
  fn raster_cache_hit_skips_fetch_and_decode() {
    let adapter = ArtworkAdapter::default();
    let client = JellyfinClient::new();
    let server_url = "https://server.example.com";
    let reference = image_id(server_url);
    adopt_session(&client, server_url, "user");
    let seeded = raster(2, 1);
    let seeded_pixels = seeded.pixels.as_ptr();
    adapter.seed_raster_for_test(&reference, ArtworkSizeClass::Card, seeded);
    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .expect("runtime builds");

    // The seeded pixels are not a decodable image and no server answers the
    // reference, so any fetch or decode would fail instead of succeeding.
    let (result, observation) = runtime.block_on(adapter.load(
      &client,
      &reference,
      ArtworkSizeClass::Card,
      LoadLane::Visible,
    ));

    let raster = result.expect("raster cache hit succeeds without decoding");
    assert_eq!(raster.pixels.as_ptr(), seeded_pixels);
    assert_eq!(
      observation.settlement,
      ArtworkLoadSettlement::Loaded(ArtworkSource::Raster)
    );
    assert_eq!(observation.bytes, 8);
  }

  #[test]
  fn encoded_memory_hit_decodes_to_a_class_sized_raster() {
    let adapter = ArtworkAdapter::default();
    let client = JellyfinClient::new();
    let server_url = "https://server.example.com";
    let reference = image_id(server_url);
    adopt_session(&client, server_url, "user");
    let png = encode_test_png(600, 900);
    adapter
      .lock_state()
      .encoded_cache
      .insert(Arc::from(reference.as_str()), artwork(&png));
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .expect("runtime builds");

    let (result, observation) = runtime.block_on(adapter.load(
      &client,
      &reference,
      ArtworkSizeClass::Card,
      LoadLane::Visible,
    ));

    let raster = result.expect("encoded cache bytes decode");
    assert_eq!((raster.width(), raster.height()), (400, 600));
    assert_eq!(
      observation.settlement,
      ArtworkLoadSettlement::Loaded(ArtworkSource::Memory)
    );
    // The raster is now cached for the Card class only.
    assert!(adapter.cached(&reference, ArtworkSizeClass::Card).is_some());
    assert!(adapter.cached(&reference, ArtworkSizeClass::Hero).is_none());
  }

  #[test]
  fn logo_shadow_matches_source_geometry_with_darkened_soft_alpha() {
    let source = image::RgbaImage::from_pixel(240, 135, image::Rgba([200, 40, 40, 255]));

    let shadow = generate_logo_shadow(&source);

    // pad = 135 / 4 = 33 per side, so the canvas grows to 306x201.
    assert_eq!((shadow.width(), shadow.height()), (306, 201));
    let (_, _, pixels, ..) = shadow.into_parts();
    let glyph_pixel = (33 * 306) * 4;
    assert_eq!(
      (
        pixels[glyph_pixel],
        pixels[glyph_pixel + 1],
        pixels[glyph_pixel + 2]
      ),
      (0, 0, 0),
      "RGB zeroed"
    );
    let alpha = pixels[glyph_pixel + 3];
    assert!(alpha > 0 && alpha < 255, "alpha softened");
  }

  #[test]
  fn logo_shadow_falls_off_without_a_hard_boundary() {
    // Dense glyph bars; the shadow must soften their edges instead of tracing
    // them: per-pixel alpha steps stay far below a visible cliff.
    let source = image::RgbaImage::from_fn(240, 135, |x, y| {
      let glyph = ((20..220).contains(&x) && (30..60).contains(&y))
        || ((40..200).contains(&x) && (80..100).contains(&y));
      if glyph {
        image::Rgba([255, 255, 255, 255])
      } else {
        image::Rgba([0, 0, 0, 0])
      }
    });

    let shadow = generate_logo_shadow(&source);
    let (_, _, pixels, ..) = shadow.into_parts();
    let alpha_at = |x: usize, y: usize| pixels[(y * 306 + x) * 4 + 3];

    // The vertical pad shifts the glyph 33px down; horizontally it stays put.
    // Scan across the first bar's right edge (x = 220) at mid height (y = 78).
    let mut max_step = 0i16;
    let mut previous = i16::from(alpha_at(210, 78));
    for x in 211..240 {
      let current = i16::from(alpha_at(x, 78));
      max_step = max_step.max((previous - current).abs());
      previous = current;
    }
    assert!(alpha_at(226, 78) > 0, "halo extends past the glyph edge");
    // Vertical: the top pad gives the halo room above the glyph (edge y = 63).
    let mut max_step = 0i16;
    let mut previous = i16::from(alpha_at(120, 50));
    for y in 51..78 {
      let current = i16::from(alpha_at(120, y));
      max_step = max_step.max((previous - current).abs());
      previous = current;
    }
    assert!(alpha_at(120, 57) > 0, "halo extends above the glyph edge");
    assert!(
      max_step <= 12,
      "no visible vertical alpha cliff, got {max_step}"
    );
  }

  #[test]
  fn logo_shadow_request_does_not_reuse_a_plain_raster() {
    let adapter = ArtworkAdapter::default();
    adapter.seed_raster_for_test("logo", ArtworkSizeClass::Hero, raster(2, 2));

    assert!(adapter.cached("logo", ArtworkSizeClass::Hero).is_some());
    assert!(adapter
      .cached_with_derived(
        "logo",
        ArtworkSizeClass::Hero,
        DerivedArtwork { logo_shadow: true },
      )
      .is_none());
  }

  #[test]
  fn decode_bounds_dimensions_to_the_class_box() {
    // Sources for Card/Hero loads are server-resized to maxWidth=600.
    let png = encode_test_png(600, 900);

    let card = decode_raster(&artwork(&png), ArtworkSizeClass::Card).expect("card decodes");
    assert_eq!((card.width(), card.height()), (400, 600));
    let hero = decode_raster(&artwork(&png), ArtworkSizeClass::Hero).expect("hero decodes");
    assert_eq!((hero.width(), hero.height()), (440, 660));
    let backdrop =
      decode_raster(&artwork(&png), ArtworkSizeClass::Backdrop).expect("backdrop decodes");
    assert_eq!((backdrop.width(), backdrop.height()), (600, 900));
  }

  #[test]
  fn three_pass_box_blur_reduces_hard_edge_variance() {
    let mut source = image::RgbaImage::from_fn(64, 16, |x, _| {
      let value = if x < 32 { 0 } else { 255 };
      image::Rgba([value, value, value, 255])
    });
    let variance = |image: &image::RgbaImage| {
      let values = image.pixels().map(|pixel| f64::from(pixel.0[0]));
      let count = f64::from(image.width() * image.height());
      let mean = values.clone().sum::<f64>() / count;
      let variance = values
        .map(|value| {
          let difference = value - mean;
          difference * difference
        })
        .sum::<f64>()
        / count;
      (mean, variance)
    };
    let (_, original_variance) = variance(&source);

    box_blur_three_passes(&mut source, 8);

    let (blurred_mean, blurred_variance) = variance(&source);
    assert!((blurred_mean - 127.5).abs() < 2.0);
    assert!(blurred_variance < original_variance);
    assert!((0..64).any(|x| {
      let value = source.get_pixel(x, 8).0[0];
      value > 0 && value < 255
    }));
  }

  #[test]
  fn decode_never_upscales_smaller_artwork() {
    let png = encode_test_png(100, 150);

    let card = decode_raster(&artwork(&png), ArtworkSizeClass::Card).expect("card decodes");
    assert_eq!((card.width(), card.height()), (100, 150));
  }

  #[test]
  fn decode_applies_exif_orientation() {
    let jpeg = encode_oriented_jpeg(120, 40, 6);

    let raster = decode_raster(&artwork(&jpeg), ArtworkSizeClass::Card).expect("jpeg decodes");

    assert_eq!((raster.width(), raster.height()), (40, 120));
  }

  #[test]
  fn decode_rejects_animated_containers_before_decoding() {
    assert_eq!(
      decode_raster(&artwork(b"GIF89a"), ArtworkSizeClass::Card),
      Err(ArtworkError::AnimatedImageUnsupported)
    );
    assert_eq!(
      decode_raster(
        &artwork(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00acTL\x00\x00\x00\x00"),
        ArtworkSizeClass::Card
      ),
      Err(ArtworkError::AnimatedImageUnsupported)
    );
  }

  #[test]
  fn decode_rejects_oversized_sources_before_allocating_the_full_buffer() {
    // 1300×1300 exceeds the Card/Hero source cap (600×2400) but fits the
    // Backdrop cap, so the rejection is the class bound, not the container.
    let png = encode_test_png(1300, 1300);

    assert_eq!(
      decode_raster(&artwork(&png), ArtworkSizeClass::Card),
      Err(ArtworkError::DecodedImageTooLarge)
    );
    assert_eq!(
      decode_raster(&artwork(&png), ArtworkSizeClass::Hero),
      Err(ArtworkError::DecodedImageTooLarge)
    );
    let backdrop =
      decode_raster(&artwork(&png), ArtworkSizeClass::Backdrop).expect("backdrop decodes");
    assert_eq!((backdrop.width(), backdrop.height()), (1300, 1300));
  }

  #[test]
  fn avatar_class_decodes_full_size_photos_but_rejects_extreme_sources() {
    // Jellyfin serves user photos at original resolution regardless of the
    // requested size, so a straight from-the-camera shot must decode (and
    // shrink into the 128px box) while genuinely extreme sources stay
    // rejected before the full buffer is allocated.
    let photo = decode_raster(
      &artwork(&encode_test_png(2000, 2000)),
      ArtworkSizeClass::Avatar,
    )
    .expect("original-resolution user photo decodes");
    assert!(
      photo.width() <= 128 && photo.height() <= 128,
      "avatar raster stays thumbnail-sized"
    );

    assert_eq!(
      decode_raster(
        &artwork(&encode_test_png(4000, 3000)),
        ArtworkSizeClass::Avatar
      ),
      Err(ArtworkError::DecodedImageTooLarge)
    );
  }

  #[test]
  fn encoded_cache_insertion_is_gated_on_the_load_generation() {
    let adapter = ArtworkAdapter::default();
    adapter.cache_encoded(Arc::from("fresh"), &artwork(&[1, 2]), 0);
    assert!(adapter
      .lock_state()
      .encoded_cache
      .get(&Arc::from("fresh"))
      .is_some());

    adapter.reset_session();
    // A load admitted before the reset must not repopulate the encoded cache.
    adapter.cache_encoded(Arc::from("stale"), &artwork(&[3, 4]), 0);
    assert!(adapter
      .lock_state()
      .encoded_cache
      .get(&Arc::from("stale"))
      .is_none());
  }

  #[test]
  fn decode_rejects_garbage_bytes() {
    let garbage = [0xff, 0xd8, 0xff, 0x00, 0x10, 0x00];

    assert_eq!(
      decode_raster(&artwork(&garbage), ArtworkSizeClass::Card),
      Err(ArtworkError::DecodeFailed)
    );
  }

  #[test]
  fn cache_evicts_least_recently_used_entry_when_entry_limit_is_reached() {
    let mut cache = ArtworkCache::<Arc<str>, ArtworkBytes>::new(16, 2);
    cache.insert(Arc::from("a"), artwork(&[1]));
    cache.insert(Arc::from("b"), artwork(&[2]));
    let _ = cache.get(&Arc::from("a"));
    cache.insert(Arc::from("c"), artwork(&[3]));

    assert!(cache.get(&Arc::from("b")).is_none());
  }

  #[test]
  fn cache_evicts_oldest_entry_when_byte_limit_is_reached() {
    let mut cache = ArtworkCache::<Arc<str>, ArtworkBytes>::new(3, 3);
    cache.insert(Arc::from("a"), artwork(&[1, 2]));
    cache.insert(Arc::from("b"), artwork(&[3, 4]));

    assert!(cache.get(&Arc::from("a")).is_none());
  }

  #[test]
  fn cache_does_not_store_an_entry_larger_than_its_total_limit() {
    let mut cache = ArtworkCache::<Arc<str>, ArtworkBytes>::new(2, 2);
    cache.insert(Arc::from("large"), artwork(&[1, 2, 3]));

    assert!(cache.get(&Arc::from("large")).is_none());
  }

  #[test]
  fn raster_cache_evicts_by_byte_budget_across_size_classes() {
    let mut cache = ArtworkCache::<RasterKey, ArtworkRaster>::new(64, usize::MAX);
    cache.insert(class_key("a", ArtworkSizeClass::Card), raster(2, 2)); // 16 bytes
    cache.insert(class_key("a", ArtworkSizeClass::Hero), raster(2, 2)); // 16 bytes
    let _ = cache.get(&class_key("a", ArtworkSizeClass::Card));
    cache.insert(class_key("b", ArtworkSizeClass::Card), raster(2, 2)); // 48 total
    cache.insert(class_key("c", ArtworkSizeClass::Card), raster(3, 2)); // 72 > 64: evicts the Hero raster

    assert!(cache.get(&class_key("a", ArtworkSizeClass::Hero)).is_none());
    assert!(cache.get(&class_key("a", ArtworkSizeClass::Card)).is_some());
    assert!(cache.get(&class_key("c", ArtworkSizeClass::Card)).is_some());
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
