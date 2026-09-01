use std::collections::{HashMap, VecDeque};
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
pub const MAX_QUEUED_LOADS: usize = 128;
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
  pub max_queued_loads: usize,
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
      max_queued_loads: MAX_QUEUED_LOADS,
    }
  }
}

impl ArtworkLimits {
  /// Bytes one load admits against the aggregate budget: the encoded body,
  /// the bounded full-size decode buffer, and the resulting Library Image
  /// Raster. Both decoded sizes are class-bounded, so the reservation is per
  /// size class.
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

  #[must_use]
  pub fn normalized(mut self) -> Self {
    self.max_active_loads = self.max_active_loads.max(1);
    self.max_active_bytes = self
      .max_active_bytes
      .max(self.load_reservation_bytes(ArtworkSizeClass::Backdrop));
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
    }
  }

  /// Upper bound on one raster's RGBA bytes, implied by the target box.
  #[must_use]
  pub const fn max_raster_bytes(self) -> usize {
    let (width, height) = self.target_box();
    width as usize * height as usize * 4
  }

  /// Cap on the source-image area decoded before downsampling, in pixels.
  ///
  /// Sources are server-resized to maxWidth 600 (Card/Hero) or 1920
  /// (Backdrop); the caps admit generous aspect extremes while rejecting
  /// decompression-bomb-shaped sources before the full RGBA buffer is
  /// allocated.
  #[must_use]
  pub const fn max_decode_pixels(self) -> u64 {
    match self {
      Self::Card | Self::Hero => 600 * 2400,
      Self::Backdrop => 1920 * 4320,
    }
  }

  /// Upper bound on the full-size decoded RGBA buffer, implied by
  /// [`Self::max_decode_pixels`].
  #[must_use]
  pub const fn max_decode_bytes(self) -> usize {
    self.max_decode_pixels() as usize * 4
  }
}

/// Display geometry for a frosted progress strip derived during artwork decode.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FrostedStripSpec {
  pub frame_width: u32,
  pub frame_height: u32,
  pub bar_height: u32,
  pub corner_radius: u32,
}

/// A Library Image Raster: an in-memory, display-sized RGBA decode of a
/// Library Image, keyed by the image reference and an [`ArtworkSizeClass`].
/// Never persisted; renderers build their handle from it synchronously.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtworkRaster {
  width: u32,
  height: u32,
  pixels: Bytes,
  frosted_strip: Option<Box<Self>>,
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
      .frosted_strip
      .as_deref()
      .map_or(self.pixels.len(), |strip| {
        self.pixels.len().saturating_add(strip.byte_len())
      })
  }

  #[must_use]
  pub fn frosted_strip(&self) -> Option<&Self> {
    self.frosted_strip.as_deref()
  }

  #[must_use]
  pub fn into_parts(self) -> (u32, u32, Bytes, Option<Self>) {
    (
      self.width,
      self.height,
      self.pixels,
      self.frosted_strip.map(|strip| *strip),
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
      frosted_strip: None,
    }
  }
}

/// Cache and coalescing key for one decoded Library Image Raster.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RasterKey {
  image_id: Arc<str>,
  size_class: ArtworkSizeClass,
  frosted_strip: Option<FrostedStripSpec>,
}

/// Per-load identity and scheduling context carried through fetch and decode.
struct LoadContext {
  key: RasterKey,
  generation: u64,
  lane: LoadLane,
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
  decode_raster_with_frosted_strip(bytes, size_class, None)
}

fn decode_raster_with_frosted_strip(
  bytes: &ArtworkBytes,
  size_class: ArtworkSizeClass,
  frosted_strip: Option<FrostedStripSpec>,
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
  let mut frosted_strip = frosted_strip
    .and_then(|spec| generate_frosted_strip(&rgba, spec))
    .map(Box::new);
  let (width, height) = rgba.dimensions();
  let pixels = Bytes::from(rgba.into_raw());
  if pixels.len() > size_class.max_raster_bytes() {
    return Err(ArtworkError::DecodedImageTooLarge);
  }
  if frosted_strip.as_deref().is_some_and(|strip| {
    pixels.len().saturating_add(strip.byte_len()) > size_class.max_raster_bytes()
  }) {
    frosted_strip = None;
  }
  Ok(ArtworkRaster {
    width,
    height,
    pixels,
    frosted_strip,
  })
}

fn generate_frosted_strip(
  source: &image::RgbaImage,
  spec: FrostedStripSpec,
) -> Option<ArtworkRaster> {
  if source.width() == 0
    || source.height() == 0
    || spec.frame_width == 0
    || spec.frame_height == 0
    || spec.bar_height == 0
    || spec.bar_height > spec.frame_height
  {
    return None;
  }

  let mut blurred = source.clone();
  box_blur_three_passes(&mut blurred, (source.width() / 10).max(8));
  let source_rect = cover_strip_source_rect(source.dimensions(), spec);
  let mut strip = resize_fractional(&blurred, source_rect, spec.frame_width, spec.bar_height);
  mask_bottom_corners(&mut strip, spec.corner_radius as f32);
  let (width, height) = strip.dimensions();
  Some(ArtworkRaster {
    width,
    height,
    pixels: Bytes::from(strip.into_raw()),
    frosted_strip: None,
  })
}

fn cover_strip_source_rect(
  (source_width, source_height): (u32, u32),
  spec: FrostedStripSpec,
) -> (f32, f32, f32, f32) {
  let content_width = source_width as f32;
  let content_height = source_height as f32;
  let bounds_width = spec.frame_width as f32;
  let bounds_height = spec.frame_height as f32;
  let content_aspect = content_width / content_height;
  let bounds_aspect = bounds_width / bounds_height;
  let (drawn_width, drawn_height) = if bounds_aspect < content_aspect {
    (
      content_width * bounds_height / content_height,
      bounds_height,
    )
  } else {
    (bounds_width, content_height * bounds_width / content_width)
  };
  let offset_x = (bounds_width - drawn_width) / 2.0;
  let offset_y = (bounds_height - drawn_height) / 2.0;
  let scale_x = drawn_width / content_width;
  let scale_y = drawn_height / content_height;
  (
    (0.0 - offset_x) / scale_x,
    (bounds_height - spec.bar_height as f32 - offset_y) / scale_y,
    bounds_width / scale_x,
    spec.bar_height as f32 / scale_y,
  )
}

fn resize_fractional(
  source: &image::RgbaImage,
  (left, top, width, height): (f32, f32, f32, f32),
  output_width: u32,
  output_height: u32,
) -> image::RgbaImage {
  image::RgbaImage::from_fn(output_width, output_height, |x, y| {
    let source_x = left + (x as f32 + 0.5) * width / output_width as f32 - 0.5;
    let source_y = top + (y as f32 + 0.5) * height / output_height as f32 - 0.5;
    bilinear_pixel(source, source_x, source_y)
  })
}

fn bilinear_pixel(source: &image::RgbaImage, x: f32, y: f32) -> image::Rgba<u8> {
  let x = x.clamp(0.0, source.width().saturating_sub(1) as f32);
  let y = y.clamp(0.0, source.height().saturating_sub(1) as f32);
  let x0 = x.floor() as u32;
  let y0 = y.floor() as u32;
  let x1 = x0.saturating_add(1).min(source.width() - 1);
  let y1 = y0.saturating_add(1).min(source.height() - 1);
  let x_fraction = x - x0 as f32;
  let y_fraction = y - y0 as f32;
  let top_left = source.get_pixel(x0, y0).0;
  let top_right = source.get_pixel(x1, y0).0;
  let bottom_left = source.get_pixel(x0, y1).0;
  let bottom_right = source.get_pixel(x1, y1).0;
  image::Rgba(std::array::from_fn(|channel| {
    let top = f32::from(top_left[channel]) * (1.0 - x_fraction)
      + f32::from(top_right[channel]) * x_fraction;
    let bottom = f32::from(bottom_left[channel]) * (1.0 - x_fraction)
      + f32::from(bottom_right[channel]) * x_fraction;
    (top * (1.0 - y_fraction) + bottom * y_fraction).round() as u8
  }))
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

fn mask_bottom_corners(strip: &mut image::RgbaImage, radius: f32) {
  if radius <= 0.0 {
    return;
  }
  let width = strip.width() as f32;
  let height = strip.height() as f32;
  let center_y = height - radius;
  for y in 0..strip.height() {
    let pixel_y = y as f32 + 0.5;
    for x in 0..strip.width() {
      let pixel_x = x as f32 + 0.5;
      let center_x = if pixel_x < radius {
        radius
      } else if pixel_x > width - radius {
        width - radius
      } else {
        continue;
      };
      let distance = ((pixel_x - center_x).powi(2) + (pixel_y - center_y).powi(2)).sqrt();
      let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
      let coverage = coverage * coverage * (3.0 - 2.0 * coverage);
      let alpha = &mut strip.get_pixel_mut(x, y).0[3];
      *alpha = (f32::from(*alpha) * coverage).round() as u8;
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

type AdapterLoadResult = Result<ArtworkRaster, ArtworkError>;
type AdapterFetchResult = Result<(ArtworkBytes, ArtworkSource), ArtworkError>;
type AdapterLoadAdmission = LoadAdmission<ArtworkRaster, oneshot::Receiver<AdapterLoadResult>>;

/// Authenticated, bounded, coalescing artwork pipeline.
///
/// Loads decode to a Library Image Raster sized by an [`ArtworkSizeClass`]:
/// the raster cache (byte-budgeted) is consulted first, then the encoded
/// memory cache, coalesced in-flight work, the disk cache, and the network.
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
        scheduler: LoadScheduler::default(),
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
    self.cached_with_frosted_strip(image_id, size_class, None)
  }

  /// Synchronous fast path for a raster with an optional derived strip.
  pub fn cached_with_frosted_strip(
    &self,
    image_id: &str,
    size_class: ArtworkSizeClass,
    frosted_strip: Option<FrostedStripSpec>,
  ) -> Option<ArtworkRaster> {
    self.lock_state().raster_cache.get(&RasterKey {
      image_id: Arc::from(image_id),
      size_class,
      frosted_strip,
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
      .load_with_ticket(client, image_id, size_class, None, self.ticket(), lane)
      .await
  }

  /// Fetches and decodes artwork with an optional derived frosted strip.
  pub async fn load_with_frosted_strip(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    size_class: ArtworkSizeClass,
    frosted_strip: Option<FrostedStripSpec>,
    lane: LoadLane,
  ) -> (Result<ArtworkRaster, ArtworkError>, ArtworkLoadObservation) {
    self
      .load_with_ticket(
        client,
        image_id,
        size_class,
        frosted_strip,
        self.ticket(),
        lane,
      )
      .await
  }

  /// Fetches and decodes an image only while `ticket` belongs to the current generation.
  pub async fn load_with_ticket(
    &self,
    client: &JellyfinClient,
    image_id: &str,
    size_class: ArtworkSizeClass,
    frosted_strip: Option<FrostedStripSpec>,
    ticket: ArtworkLoadTicket,
    lane: LoadLane,
  ) -> (Result<ArtworkRaster, ArtworkError>, ArtworkLoadObservation) {
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
    let key = RasterKey {
      image_id: Arc::from(image_id),
      size_class,
      frosted_strip,
    };
    let mut generation = self.generation_sender.subscribe();
    let load_generation = ticket.generation();
    match self.admit(key.clone(), load_generation, lane) {
      LoadAdmission::Cached(raster) => {
        let bytes = raster.byte_len();
        let observation = finish_load(
          &span,
          started,
          ArtworkLoadSettlement::Loaded(ArtworkSource::Raster),
          bytes,
        );
        (Ok(raster), observation)
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
        let pending = PendingLoad::new(self, key.clone(), load_generation);
        let result = match self
          .acquire_load_permit(&key, load_generation, lane, &mut generation)
          .await
        {
          Ok(permit) => {
            self
              .fetch_and_decode(
                client,
                &request,
                LoadContext {
                  key: key.clone(),
                  generation: load_generation,
                  lane,
                },
                permit,
                &mut generation,
              )
              .await
          }
          Err(error) => Err(error),
        };
        // Non-raster sources report the encoded body length so the aggregate
        // keeps transfer bytes distinct from raster pixel bytes.
        let (settlement, bytes) = match &result {
          Ok((_, source, encoded_bytes)) => {
            (ArtworkLoadSettlement::Loaded(*source), *encoded_bytes)
          }
          Err(ArtworkError::Cancelled) => (ArtworkLoadSettlement::Cancelled, 0),
          Err(_) => (ArtworkLoadSettlement::Failed, 0),
        };
        let settled = result
          .as_ref()
          .map(|(raster, _, _)| raster.clone())
          .map_err(|error| *error);
        pending.complete(&settled);
        let observation = finish_load(&span, started, settlement, bytes);
        (result.map(|(raster, _, _)| raster), observation)
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

  fn admit(&self, key: RasterKey, generation: u64, lane: LoadLane) -> AdapterLoadAdmission {
    self.lock_state().admit(key, generation, lane)
  }

  async fn acquire_load_permit(
    &self,
    key: &RasterKey,
    load_generation: u64,
    lane: LoadLane,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<LoadPermit, ArtworkError> {
    let queued = QueuedLoad::new(self, load_generation, lane)?;
    // Publish the queue entry so a visible follower can promote this leader
    // while it is still queued.
    if let Some(load) = self.lock_state().in_flight.get_mut(key) {
      if load.generation == load_generation {
        load.queued_id = Some(queued.id());
      }
    }
    let reservation = self.limits.load_reservation_bytes(key.size_class);
    loop {
      if self.try_activate(queued.id(), reservation) {
        return Ok(queued.activate(reservation));
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

  fn try_activate(&self, queue_id: u64, reservation: usize) -> bool {
    self.lock_state().scheduler.try_activate(
      queue_id,
      self.limits.max_active_loads,
      self.limits.max_active_bytes,
      reservation,
    )
  }

  async fn fetch_and_decode(
    &self,
    client: &JellyfinClient,
    request: &LibraryImageRequest,
    context: LoadContext,
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
      .decode_tracked(
        bytes,
        size_class,
        context.key.frosted_strip,
        permit,
        generation,
      )
      .await
    {
      Ok(raster) => Ok((raster, source, encoded_bytes)),
      Err(ArtworkError::DecodedImageTooLarge) if source != ArtworkSource::Network => {
        self
          .retry_oversized_from_network(client, request, &context, generation)
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
    let bytes = tokio::select! {
      result = self.fetch_uncached(client, request) => result?,
      changed = generation.changed() => {
        let _ = changed;
        return Err(ArtworkError::Cancelled);
      }
    };
    let permit = self
      .acquire_load_permit(&context.key, context.generation, context.lane, generation)
      .await?;
    let encoded_bytes = bytes.byte_len();
    let stored = bytes.clone();
    let raster = self
      .decode_tracked(
        bytes,
        context.key.size_class,
        context.key.frosted_strip,
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
    frosted_strip: Option<FrostedStripSpec>,
    permit: LoadPermit,
    generation: &mut watch::Receiver<u64>,
  ) -> Result<ArtworkRaster, ArtworkError> {
    let decode = tokio::task::spawn_blocking(move || {
      // Cancellation may drop the join handle, so the blocking decode owns
      // aggregate admission until it actually stops.
      let _permit = permit;
      decode_raster_with_frosted_strip(&bytes, size_class, frosted_strip)
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

  fn finish_pending(&self, key: RasterKey, generation: u64, result: &AdapterLoadResult) {
    let waiters = {
      let mut state = self.lock_state();
      if let Ok(raster) = result {
        if generation >= state.cache_generation {
          state.raster_cache.insert(key.clone(), raster.clone());
        }
      }
      if state.generation == generation {
        state
          .in_flight
          .remove(&key)
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
    self.lock_state().raster_cache.insert(
      RasterKey {
        image_id: Arc::from(image_id),
        size_class,
        frosted_strip: None,
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
  scheduler: LoadScheduler,
}

impl AdapterState {
  fn admit(&mut self, key: RasterKey, generation: u64, lane: LoadLane) -> AdapterLoadAdmission {
    if generation != self.generation {
      return LoadAdmission::Cancelled;
    }
    if let Some(raster) = self.raster_cache.get(&key) {
      return LoadAdmission::Cached(raster);
    }
    if let Some(load) = self.in_flight.get_mut(&key) {
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

  fn cancel_stale(&mut self, clear_cache: bool) -> Vec<oneshot::Sender<AdapterLoadResult>> {
    if clear_cache {
      self.encoded_cache.clear();
      self.raster_cache.clear();
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
  waiters: Vec<oneshot::Sender<AdapterLoadResult>>,
  /// Scheduler entry while the leader waits for a permit; lets a visible
  /// follower promote a still-queued offscreen leader.
  queued_id: Option<u64>,
}

async fn wait_for_follower(
  receiver: oneshot::Receiver<AdapterLoadResult>,
  generation: &mut watch::Receiver<u64>,
) -> AdapterLoadResult {
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

fn notify_cancelled(waiters: Vec<oneshot::Sender<AdapterLoadResult>>) {
  for waiter in waiters {
    let _ = waiter.send(Err(ArtworkError::Cancelled));
  }
}

struct QueuedLoad<'a> {
  adapter: &'a ArtworkAdapter,
  id: Option<u64>,
  notify: Arc<Notify>,
}

impl<'a> QueuedLoad<'a> {
  fn new(
    adapter: &'a ArtworkAdapter,
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
  key: Option<RasterKey>,
  generation: u64,
}

impl<'a> PendingLoad<'a> {
  fn new(adapter: &'a ArtworkAdapter, key: RasterKey, generation: u64) -> Self {
    Self {
      adapter,
      key: Some(key),
      generation,
    }
  }

  fn complete(mut self, result: &AdapterLoadResult) {
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

  fn raster(width: u32, height: u32) -> ArtworkRaster {
    let pixels = vec![7u8; width as usize * height as usize * 4];
    ArtworkRaster::from_raw_for_test(width, height, Bytes::from(pixels))
  }

  const fn frosted_spec() -> FrostedStripSpec {
    FrostedStripSpec {
      frame_width: 240,
      frame_height: 135,
      bar_height: 8,
      corner_radius: 8,
    }
  }

  fn class_key(image_id: &str, size_class: ArtworkSizeClass) -> RasterKey {
    RasterKey {
      image_id: Arc::from(image_id),
      size_class,
      frosted_strip: None,
    }
  }

  fn begin_leader<'a>(adapter: &'a ArtworkAdapter, key: &str) -> PendingLoad<'a> {
    let generation = adapter.lock_state().generation;
    let LoadAdmission::Leader(generation) = adapter.admit(
      class_key(key, ArtworkSizeClass::Card),
      generation,
      LoadLane::Offscreen,
    ) else {
      panic!("expected a leader admission");
    };
    PendingLoad::new(adapter, class_key(key, ArtworkSizeClass::Card), generation)
  }

  fn activate_permit(adapter: &ArtworkAdapter) -> LoadPermit {
    let generation = adapter.lock_state().generation;
    let queued =
      QueuedLoad::new(adapter, generation, LoadLane::Visible).expect("queue has capacity");
    let reservation = adapter
      .limits
      .load_reservation_bytes(ArtworkSizeClass::Card);
    assert!(adapter.try_activate(queued.id(), reservation));
    queued.activate(reservation)
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

  /// Encodes a deterministic RGBA image of the given size as PNG.
  fn encode_test_png(width: u32, height: u32) -> Vec<u8> {
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
  fn aggregate_reservation_covers_encoded_and_two_pixel_buffers_per_class() {
    let limits = ArtworkLimits::default();

    for size_class in [
      ArtworkSizeClass::Card,
      ArtworkSizeClass::Hero,
      ArtworkSizeClass::Backdrop,
    ] {
      assert_eq!(
        limits.load_reservation_bytes(size_class),
        limits.max_response_bytes
          + size_class.max_decode_bytes()
          + size_class.max_raster_bytes() * DECODE_PIXEL_BUFFER_RESERVATIONS
      );
    }
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
    let reservation = adapter
      .limits
      .load_reservation_bytes(ArtworkSizeClass::Card);
    let loads = (0..MAX_ACTIVE_LOADS)
      .map(|_| {
        QueuedLoad::new(&adapter, generation, LoadLane::Visible).expect("load should be queued")
      })
      .collect::<Vec<_>>();
    for load in &loads {
      assert!(
        adapter.try_activate(load.id(), reservation),
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
      .get_mut(&class_key("shared", ArtworkSizeClass::Card))
      .expect("leader is in flight")
      .queued_id = Some(leader.id());

    let LoadAdmission::Follower(_receiver) = adapter.admit(
      class_key("shared", ArtworkSizeClass::Card),
      generation,
      LoadLane::Visible,
    ) else {
      panic!("expected a follower admission");
    };

    // When one permit frees, the promoted leader drains before the earlier
    // offscreen load.
    drop(permits.pop());
    let reservation = adapter
      .limits
      .load_reservation_bytes(ArtworkSizeClass::Card);
    assert!(adapter.try_activate(leader.id(), reservation));
    assert!(!adapter.try_activate(earlier.id(), reservation));
  }

  #[test]
  fn load_summary_message_is_sanitized_and_empty_summary_records_nothing() {
    assert_eq!(ArtworkLoadSummary::default().diagnostic_message(), None);

    let summary = ArtworkLoadSummary {
      raster_loads: 4,
      memory_loads: 8,
      disk_loads: 3,
      network_loads: 1,
      failed_loads: 2,
      total_duration_millis: 640,
      total_bytes: 1_234_567,
    };
    let message = summary
      .diagnostic_message()
      .expect("settled loads describe");

    assert!(message.contains("18"));
    assert!(message.contains("4 raster"));
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
    let reservation = adapter
      .limits
      .load_reservation_bytes(ArtworkSizeClass::Card);
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
    assert!(adapter.try_activate(current.id(), reservation));
    let current = current.activate(reservation);
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
      adapter.admit(
        class_key("current", ArtworkSizeClass::Card),
        generation,
        LoadLane::Offscreen
      ),
      LoadAdmission::Leader(1)
    ));
  }

  #[test]
  fn cancelling_a_queued_load_allows_the_next_unique_load_to_run() {
    let adapter = ArtworkAdapter::default();
    let reservation = adapter
      .limits
      .load_reservation_bytes(ArtworkSizeClass::Card);
    let first = activate_permit(&adapter);
    let second = activate_permit(&adapter);
    let generation = adapter.lock_state().generation;
    let cancelled =
      QueuedLoad::new(&adapter, generation, LoadLane::Visible).expect("queue has capacity");
    let next =
      QueuedLoad::new(&adapter, generation, LoadLane::Visible).expect("queue has capacity");

    assert!(!adapter.try_activate(next.id(), reservation));
    drop(cancelled);
    drop(first);
    assert!(adapter.try_activate(next.id(), reservation));

    let next = next.activate(reservation);
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
    let LoadAdmission::Follower(receiver) = adapter.admit(
      class_key("same", ArtworkSizeClass::Card),
      0,
      LoadLane::Offscreen,
    ) else {
      panic!("expected a follower admission");
    };
    let decoded = raster(1, 1);
    let pixels = decoded.pixels.as_ptr();

    pending.complete(&Ok(decoded));
    let received = receiver
      .blocking_recv()
      .expect("leader sends a result")
      .expect("leader succeeds");

    assert_eq!(received.pixels.as_ptr(), pixels);
    let LoadAdmission::Cached(cached) = adapter.admit(
      class_key("same", ArtworkSizeClass::Card),
      0,
      LoadLane::Offscreen,
    ) else {
      panic!("expected a raster cache hit");
    };
    assert_eq!(cached.pixels.as_ptr(), pixels);
  }

  #[test]
  fn coalescing_distinguishes_size_classes() {
    let adapter = ArtworkAdapter::default();
    let generation = adapter.lock_state().generation;

    assert!(matches!(
      adapter.admit(
        class_key("shared", ArtworkSizeClass::Card),
        generation,
        LoadLane::Offscreen
      ),
      LoadAdmission::Leader(_)
    ));
    assert!(matches!(
      adapter.admit(
        class_key("shared", ArtworkSizeClass::Card),
        generation,
        LoadLane::Offscreen
      ),
      LoadAdmission::Follower(_)
    ));
    // A different size class of the same reference coalesces separately.
    assert!(matches!(
      adapter.admit(
        class_key("shared", ArtworkSizeClass::Hero),
        generation,
        LoadLane::Offscreen
      ),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn cancelled_follower_does_not_cancel_the_shared_leader() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) = adapter.admit(
      class_key("same", ArtworkSizeClass::Card),
      0,
      LoadLane::Offscreen,
    ) else {
      panic!("expected a follower admission");
    };
    drop(receiver);
    pending.complete(&Ok(raster(1, 1)));

    assert!(matches!(
      adapter.admit(
        class_key("same", ArtworkSizeClass::Card),
        0,
        LoadLane::Offscreen
      ),
      LoadAdmission::Cached(_)
    ));
  }

  #[test]
  fn cancelled_leader_notifies_followers_and_releases_the_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) = adapter.admit(
      class_key("same", ArtworkSizeClass::Card),
      0,
      LoadLane::Offscreen,
    ) else {
      panic!("expected a follower admission");
    };

    drop(pending);

    assert!(matches!(
      receiver.blocking_recv().expect("leader sends cancellation"),
      Err(ArtworkError::Cancelled)
    ));
    assert!(matches!(
      adapter.admit(
        class_key("same", ArtworkSizeClass::Card),
        0,
        LoadLane::Offscreen
      ),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn completed_error_releases_the_coalescing_key() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    pending.complete(&Err(ArtworkError::FetchFailed));

    assert!(matches!(
      adapter.admit(
        class_key("same", ArtworkSizeClass::Card),
        0,
        LoadLane::Offscreen
      ),
      LoadAdmission::Leader(_)
    ));
  }

  #[test]
  fn generation_cancellation_notifies_followers_and_rejects_stale_admission() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "same");
    let LoadAdmission::Follower(receiver) = adapter.admit(
      class_key("same", ArtworkSizeClass::Card),
      0,
      LoadLane::Offscreen,
    ) else {
      panic!("expected a follower admission");
    };

    adapter.cancel_pending();

    assert!(matches!(
      receiver.blocking_recv().expect("follower is notified"),
      Err(ArtworkError::Cancelled)
    ));
    assert!(matches!(
      adapter.admit(
        class_key("stale", ArtworkSizeClass::Card),
        0,
        LoadLane::Offscreen
      ),
      LoadAdmission::Cancelled
    ));
    assert!(matches!(
      adapter.admit(
        class_key("current", ArtworkSizeClass::Card),
        1,
        LoadLane::Offscreen
      ),
      LoadAdmission::Leader(1)
    ));
    drop(pending);
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
  fn clear_caches_drops_cached_data_without_cancelling_in_flight_loads() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "in-flight");
    {
      let mut state = adapter.lock_state();
      state
        .raster_cache
        .insert(class_key("cached", ArtworkSizeClass::Card), raster(1, 1));
      state
        .encoded_cache
        .insert(Arc::from("encoded"), artwork(&[1, 2, 3, 4]));
    }

    adapter.clear_caches();

    let mut state = adapter.lock_state();
    assert!(state
      .raster_cache
      .get(&class_key("cached", ArtworkSizeClass::Card))
      .is_none());
    assert!(state.encoded_cache.get(&Arc::from("encoded")).is_none());
    drop(state);

    // The in-flight load still finishes and re-enters the raster cache.
    pending.complete(&Ok(raster(1, 1)));
    assert!(adapter
      .lock_state()
      .raster_cache
      .get(&class_key("in-flight", ArtworkSizeClass::Card))
      .is_some());
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
  fn stale_success_after_reset_session_does_not_repopulate_raster_cache() {
    let adapter = ArtworkAdapter::default();
    let pending = begin_leader(&adapter, "stale");

    adapter.reset_session();
    pending.complete(&Ok(raster(1, 1)));

    assert!(adapter
      .lock_state()
      .raster_cache
      .get(&class_key("stale", ArtworkSizeClass::Card))
      .is_none());
  }

  #[test]
  fn ticket_captured_before_reset_cannot_adopt_the_new_generation() {
    let adapter = ArtworkAdapter::default();
    let stale = adapter.ticket();

    adapter.reset_session();

    assert!(matches!(
      adapter.admit(
        class_key("stale", ArtworkSizeClass::Card),
        stale.generation(),
        LoadLane::Offscreen
      ),
      LoadAdmission::Cancelled
    ));
    assert!(matches!(
      adapter.admit(
        class_key("current", ArtworkSizeClass::Card),
        adapter.ticket().generation(),
        LoadLane::Offscreen
      ),
      LoadAdmission::Leader(_)
    ));
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
  fn frosted_request_does_not_reuse_a_stripless_raster_cache_entry() {
    let adapter = ArtworkAdapter::default();
    adapter.seed_raster_for_test("shared", ArtworkSizeClass::Card, raster(2, 2));

    assert!(adapter.cached("shared", ArtworkSizeClass::Card).is_some());
    assert!(adapter
      .cached_with_frosted_strip("shared", ArtworkSizeClass::Card, Some(frosted_spec()))
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
  fn decode_drops_the_optional_strip_when_combined_raster_bytes_exceed_the_class_cap() {
    let png = encode_test_png(600, 900);

    let raster = decode_raster_with_frosted_strip(
      &artwork(&png),
      ArtworkSizeClass::Card,
      Some(frosted_spec()),
    )
    .expect("main raster still decodes");

    assert_eq!(raster.byte_len(), ArtworkSizeClass::Card.max_raster_bytes());
    assert!(raster.frosted_strip().is_none());
  }

  #[test]
  fn frosted_strip_has_exact_frame_width_and_bar_height() {
    let source = image::RgbaImage::from_pixel(240, 135, image::Rgba([30, 60, 90, 255]));

    let strip = generate_frosted_strip(&source, frosted_spec()).expect("strip is generated");

    assert_eq!((strip.width(), strip.height()), (240, 8));
  }

  #[test]
  fn frosted_strip_masks_only_the_bottom_corners() {
    let source = image::RgbaImage::from_pixel(240, 135, image::Rgba([30, 60, 90, 255]));
    let strip = generate_frosted_strip(&source, frosted_spec()).expect("strip is generated");
    let (_, _, pixels, _) = strip.into_parts();
    let alpha_at = |x: usize, y: usize| pixels[(y * 240 + x) * 4 + 3];

    assert_eq!(alpha_at(0, 7), 0);
    assert_eq!(alpha_at(239, 7), 0);
    assert_eq!(alpha_at(120, 4), 255);
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
  fn frosted_strip_samples_the_fractional_cover_transform() {
    let source =
      image::RgbaImage::from_fn(120, 300, |_, y| image::Rgba([y.min(255) as u8, 0, 0, 255]));

    let strip = generate_frosted_strip(&source, frosted_spec()).expect("strip is generated");
    let (_, _, pixels, _) = strip.into_parts();
    let red_at = |y: usize| pixels[(y * 240 + 120) * 4];

    assert_eq!(red_at(0), 180);
    assert_eq!(red_at(7), 183);
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
