use std::sync::{Arc, LazyLock, Mutex};

use dolby_vision::rpu::{dovi_rpu::DoviRpu, rpu_data_mapping::RpuDataMapping};
use gst::{glib, prelude::*};
use gstreamer as gst;
use gstreamer_app as gst_app;

use crate::PlaybackError;

const META_NAME: &str = "JellypilotDoviFrameMeta";
static REGISTER_META: LazyLock<()> =
  LazyLock::new(|| gst::meta::CustomMeta::register(META_NAME, &["video"]));

pub(crate) struct DoviFrame {
  pub rpu: DoviRpu,
  pub mapping: Arc<RpuDataMapping>,
}

#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "JellypilotDoviFrameCarrier")]
struct Carrier(Arc<CarriedFrame>);
struct CarriedFrame {
  pts: Option<gst::ClockTime>,
  duration: Option<gst::ClockTime>,
  frame: Arc<DoviFrame>,
}

fn error(reason: impl std::fmt::Display) -> PlaybackError {
  PlaybackError::Frame(format!("Dolby Vision: {reason}"))
}

pub(crate) fn frame(sample: &gst::Sample) -> Result<Option<Arc<DoviFrame>>, PlaybackError> {
  let buffer = sample
    .buffer()
    .ok_or_else(|| error("missing decoded buffer"))?;
  let Ok(meta) = gst::meta::CustomMeta::from_buffer(buffer, META_NAME) else {
    return Ok(None);
  };
  let carrier = meta.structure().get::<Carrier>("frame").map_err(error)?;
  let matches = carrier.0.pts.is_some_and(|pts| {
    if buffer.pts() == Some(pts) {
      return true;
    }
    let Some(duration) = carrier.0.duration else {
      return false;
    };
    let Some(end) = pts.checked_add(duration) else {
      return false;
    };
    let Some(segment) = sample
      .segment()
      .and_then(|s| s.downcast_ref::<gst::ClockTime>())
    else {
      return false;
    };
    // Accurate seek clips the first decoded PTS after native metadata transfer.
    // Only the original AU's intersection with that segment is valid.
    segment
      .clip(pts, end)
      .is_some_and(|(start, _)| start == buffer.pts())
  });
  if !matches {
    return Err(error(format!(
      "decoded frame timestamp {:?} differs from encoded RPU {:?}",
      buffer.pts(),
      carrier.0.pts,
    )));
  }
  Ok(Some(carrier.0.frame.clone()))
}

#[derive(Default)]
struct Mappings([Option<Arc<RpuDataMapping>>; 16]);
impl Mappings {
  fn resolve(&mut self, rpu: &mut DoviRpu) -> Result<Arc<RpuDataMapping>, PlaybackError> {
    if rpu.header.use_prev_vdr_rpu_flag {
      let id = usize::try_from(rpu.header.prev_vdr_rpu_id).map_err(error)?;
      return self.0.get(id).and_then(Clone::clone).ok_or_else(|| {
        error(format!(
          "unavailable previous mapping ID {id} in this decode epoch"
        ))
      });
    }
    let mapping = rpu
      .rpu_data_mapping
      .take()
      .ok_or_else(|| error("missing full mapping"))?;
    let id = usize::try_from(mapping.vdr_rpu_id).map_err(error)?;
    let slot = self
      .0
      .get_mut(id)
      .ok_or_else(|| error("mapping ID exceeds 15"))?;
    let mapping = Arc::new(mapping);
    *slot = Some(mapping.clone());
    Ok(mapping)
  }
}

#[derive(Clone, Copy)]
enum Framing {
  AnnexB,
  Length(usize),
}
impl Framing {
  fn from_caps(caps: &gst::CapsRef) -> Result<Option<Self>, PlaybackError> {
    let s = caps
      .structure(0)
      .ok_or_else(|| error("missing decoder caps"))?;
    if s.name() != "video/x-h265" {
      return Ok(None);
    }
    if s.get::<&str>("alignment").map_err(error)? != "au" {
      return Err(error("HEVC decoder input must be access-unit aligned"));
    }
    match s.get::<&str>("stream-format").map_err(error)? {
      "byte-stream" => Ok(Some(Self::AnnexB)),
      "hvc1" | "hev1" => {
        let config = s.get::<gst::Buffer>("codec_data").map_err(error)?;
        let bytes = config.map_readable().map_err(error)?;
        if bytes.len() < 23 || bytes[0] != 1 {
          return Err(error("invalid HEVCDecoderConfigurationRecord"));
        }
        Ok(Some(Self::Length(usize::from(bytes[21] & 3) + 1)))
      }
      _ => Err(error("unsupported HEVC stream framing")),
    }
  }

  fn rpu<'a>(self, data: &'a [u8]) -> Result<Option<&'a [u8]>, PlaybackError> {
    let mut rpu = None;
    let mut inspect = |nal: &'a [u8]| -> Result<(), PlaybackError> {
      if nal.len() < 2 {
        return Err(error("truncated HEVC NAL header"));
      }
      if (nal[0] >> 1) & 63 == 62 {
        if nal[0] != 0x7c || nal[1] != 1 {
          return Err(error("unsupported RPU layer or temporal ID"));
        }
        if rpu.replace(nal).is_some() {
          return Err(error("multiple RPUs in one access unit"));
        }
      }
      Ok(())
    };
    match self {
      Self::Length(width) => {
        let mut offset = 0;
        while offset < data.len() {
          let prefix = data
            .get(offset..offset + width)
            .ok_or_else(|| error("truncated NAL length"))?;
          let size = prefix
            .iter()
            .fold(0usize, |n, b| (n << 8) | usize::from(*b));
          offset += width;
          let end = offset
            .checked_add(size)
            .ok_or_else(|| error("NAL length overflow"))?;
          inspect(
            data
              .get(offset..end)
              .ok_or_else(|| error("truncated length-prefixed NAL"))?,
          )?;
          offset = end;
        }
      }
      Self::AnnexB => {
        let start_code = |from: usize| {
          data
            .get(from..)?
            .windows(3)
            .position(|w| w == [0, 0, 1])
            .map(|i| from + i)
        };
        let mut start = start_code(0).ok_or_else(|| error("missing Annex B start code"))?;
        if data[..start].iter().any(|b| *b != 0) {
          return Err(error("invalid Annex B prefix"));
        }
        loop {
          let next = start_code(start + 3);
          let mut end = next.unwrap_or(data.len());
          while end > start + 3 && data[end - 1] == 0 {
            end -= 1;
          }
          inspect(&data[start + 3..end])?;
          match next {
            Some(next) => start = next,
            None => break,
          }
        }
      }
    }
    Ok(rpu)
  }
}

// A bounded syntax pass precedes dolby_vision 3.4.0: that parser allocates pivot
// and extension vectors before validation, and panics on polynomial interpolation.
// This cursor never allocates from bitstream counts and rejects lossy mixed curves.
struct Bits<'a> {
  data: &'a [u8],
  position: usize,
}
impl Bits<'_> {
  fn read(&mut self, count: usize) -> Result<u64, PlaybackError> {
    if count > 64 || count > self.remaining() {
      return Err(error("truncated RPU syntax"));
    }
    let mut value = 0;
    for _ in 0..count {
      value =
        (value << 1) | u64::from((self.data[self.position / 8] >> (7 - self.position % 8)) & 1);
      self.position += 1;
    }
    Ok(value)
  }
  fn remaining(&self) -> usize {
    self.data.len() * 8 - self.position
  }
  fn skip(&mut self, count: usize) -> Result<(), PlaybackError> {
    if count > self.remaining() {
      return Err(error("truncated RPU payload"));
    }
    self.position += count;
    Ok(())
  }
  fn ue(&mut self, max: u64) -> Result<u64, PlaybackError> {
    let mut zeros = 0;
    while self.read(1)? == 0 {
      zeros += 1;
      if zeros > 31 {
        return Err(error("oversized RPU Exp-Golomb value"));
      }
    }
    let value = ((1u64 << zeros) - 1) + self.read(zeros)?;
    if value > max {
      return Err(error("RPU syntax value exceeds supported bound"));
    }
    Ok(value)
  }
  fn require(
    &mut self,
    count: usize,
    value: u64,
    reason: &'static str,
  ) -> Result<(), PlaybackError> {
    if self.read(count)? != value {
      return Err(error(reason));
    }
    Ok(())
  }
  fn align(&mut self) -> Result<(), PlaybackError> {
    while !self.position.is_multiple_of(8) {
      self.require(1, 0, "nonzero RPU alignment bit")?;
    }
    Ok(())
  }
  fn extensions(&mut self) -> Result<(), PlaybackError> {
    let count = self.ue(255)?;
    self.align()?;
    for _ in 0..count {
      let length = self.ue(65536)? as usize;
      let level = self.read(8)?;
      let valid_length = match level {
        1 | 3 => length == 5,
        2 => length == 11,
        4 => length == 3,
        5 => length == 7,
        6 => length == 8,
        8 => matches!(length, 10 | 12 | 13 | 19 | 25),
        9 => matches!(length, 1 | 17),
        10 => matches!(length, 5 | 21),
        11 => length == 4,
        254 => length == 2,
        255 => length == 6,
        _ => false,
      };
      if !valid_length {
        return Err(error("unsupported DM extension level or length"));
      }
      self.skip(length * 8)?;
    }
    Ok(())
  }
}

fn preflight(data: &[u8]) -> Result<(), PlaybackError> {
  let mut b = Bits { data, position: 0 };
  b.require(8, 25, "invalid RPU prefix")?;
  b.require(6, 2, "unsupported RPU type")?;
  b.require(11, 18, "unsupported RPU format")?;
  b.require(4, 0, "only Dolby Vision Profile 5 is supported")?;
  b.require(4, 0, "unsupported RPU level")?;
  b.require(1, 1, "RPU sequence information is required")?;
  b.require(1, 0, "explicit chroma resampling filters are unsupported")?;
  b.require(2, 0, "floating-point RPU coefficients are unsupported")?;
  let denom = b.ue(23)? as usize;
  b.require(2, 1, "unsupported RPU normalization")?;
  b.require(1, 1, "Profile 5 requires full-range base layer")?;
  if b.ue(2)? != 2 || b.ue(2)? != 2 {
    return Err(error(
      "only 10-bit base layer without extended mapping is supported",
    ));
  }
  b.ue(6)?;
  b.require(1, 0, "spatial resampling is unsupported")?;
  b.require(
    3,
    0,
    "compressed DM or reserved RPU features are unsupported",
  )?;
  b.require(1, 0, "enhancement layer resampling is unsupported")?;
  b.require(1, 1, "enhancement layer residuals are unsupported")?;
  b.require(
    1,
    1,
    "current-frame display-management metadata is required",
  )?;
  let previous = b.read(1)? != 0;
  if previous {
    b.ue(15)?;
  } else {
    b.ue(15)?;
    b.ue(0)?;
    b.ue(0)?;
    let mut pieces = [0; 3];
    for count in &mut pieces {
      *count = b.ue(7)? as usize + 1;
      let mut pivot = 0;
      for index in 0..=*count {
        let delta = b.read(10)?;
        if index > 0 && delta == 0 {
          return Err(error("non-increasing reshape pivots"));
        }
        pivot += delta;
        if pivot > 1023 {
          return Err(error("reshape pivot exceeds 10-bit range"));
        }
      }
    }
    b.ue(0)?;
    b.ue(0)?;
    for count in pieces {
      let mut method = None;
      for _ in 0..count {
        let current = b.ue(1)?;
        if method.replace(current).is_some_and(|old| old != current) {
          return Err(error(
            "mixed polynomial/MMR pieces cannot be represented by this parser",
          ));
        }
        let coefficients = if current == 0 {
          let order = b.ue(1)?;
          if order == 0 {
            b.require(1, 0, "polynomial interpolation is unsupported")?;
          }
          order as usize + 2
        } else {
          let order = b.read(2)?;
          if order > 2 {
            return Err(error("unsupported MMR order"));
          }
          1 + (order as usize + 1) * 7
        };
        for _ in 0..coefficients {
          b.ue(u32::MAX.into())?;
          b.read(denom)?;
        }
      }
    }
  }
  let affected = b.ue(15)?;
  let current = b.ue(15)?;
  if affected != current {
    return Err(error("cross-frame DM ID updates are unsupported"));
  }
  b.ue(u32::MAX.into())?;
  b.skip(9 * 16 + 3 * 32 + 9 * 16 + 3 * 16 + 32 + 5 + 2 + 2 + 2 + 12 + 12 + 10)?;
  b.extensions()?;
  // This is the exact second-payload condition in dolby_vision 3.4.0.
  if b.remaining() >= 56 {
    b.extensions()?;
  }
  b.align()?;
  if b.remaining() != 40 {
    return Err(error("unsupported remaining RPU payload"));
  }
  Ok(())
}

fn parse(nal: &[u8]) -> Result<DoviRpu, PlaybackError> {
  if nal.len() > 65536 {
    return Err(error("RPU exceeds 64 KiB safety limit"));
  }
  let trimmed = DoviRpu::validated_trimmed_data(nal).map_err(error)?;
  let mut bytes = Vec::with_capacity(trimmed.len());
  let mut zeros = 0;
  for (index, &byte) in trimmed.iter().enumerate() {
    if zeros == 2 && byte == 3 {
      if trimmed.get(index + 1).is_none_or(|next| *next > 3) {
        return Err(error("invalid RPU emulation-prevention byte"));
      }
      zeros = 0;
      continue;
    }
    zeros = if byte == 0 { zeros + 1 } else { 0 };
    bytes.push(byte);
  }
  while bytes.last() == Some(&0) {
    bytes.pop();
  }
  preflight(&bytes)?;
  DoviRpu::parse_rpu(&bytes).map_err(error)
}

#[derive(Default)]
struct DecoderState {
  framing: Option<Framing>,
  caps: Option<gst::Caps>,
  mappings: Mappings,
  selected: bool,
}
impl DecoderState {
  fn buffer(&mut self, buffer: &gst::BufferRef) -> Result<Option<Arc<DoviFrame>>, PlaybackError> {
    let Some(framing) = self.framing else {
      return Ok(None);
    };
    let map = buffer.map_readable().map_err(error)?;
    let Some(nal) = framing.rpu(&map)? else {
      return if self.selected {
        Err(error("Dolby Vision access unit has no RPU"))
      } else {
        Ok(None)
      };
    };
    self.selected = true;
    if buffer.pts().is_none() {
      return Err(error("RPU access unit has no presentation timestamp"));
    }
    let mut rpu = parse(nal)?;
    let mapping = self.mappings.resolve(&mut rpu)?;
    Ok(Some(Arc::new(DoviFrame { rpu, mapping })))
  }
}

#[derive(Default)]
struct CaptureShared {
  probes: Vec<(gst::Pad, gst::PadProbeId)>,
  error: Option<PlaybackError>,
}
pub(crate) struct Capture {
  pipeline: glib::WeakRef<gst::Pipeline>,
  signal: Option<glib::SignalHandlerId>,
  shared: Arc<Mutex<CaptureShared>>,
}
impl Capture {
  pub(crate) fn error(&self) -> Option<PlaybackError> {
    match self.shared.lock() {
      Ok(shared) => shared.error.clone(),
      Err(_) => Some(error("capture state lock poisoned")),
    }
  }
}
impl Drop for Capture {
  fn drop(&mut self) {
    // The worker first sets Null; no streaming callback is active at teardown.
    if let (Some(pipeline), Some(signal)) = (self.pipeline.upgrade(), self.signal.take()) {
      pipeline.disconnect(signal);
    }
    if let Ok(mut shared) = self.shared.lock() {
      for (pad, id) in shared.probes.drain(..) {
        pad.remove_probe(id);
      }
    }
  }
}

pub(crate) fn install(pipeline: &gst::Pipeline, sink: &gst_app::AppSink) -> Capture {
  LazyLock::force(&REGISTER_META);
  let shared = Arc::new(Mutex::new(CaptureShared::default()));
  let callback_shared = shared.clone();
  let sink = sink.downgrade();
  let signal = pipeline.connect_deep_element_added(move |_, _, element| {
    if !element.factory().is_some_and(|factory| {
      factory.klass().split('/').any(|s| s == "Decoder")
        && factory.klass().split('/').any(|s| s == "Video")
    }) {
      return;
    }
    let Some(pad) = element.static_pad("sink") else {
      return;
    };
    let state = Mutex::new(DecoderState::default());
    let shared = callback_shared.clone();
    let sink = sink.clone();
    let id = pad.add_probe(
      gst::PadProbeType::BUFFER
        | gst::PadProbeType::EVENT_DOWNSTREAM
        | gst::PadProbeType::EVENT_FLUSH,
      move |pad, info| {
        let result = (|| -> Result<(), PlaybackError> {
          let mut state = state
            .lock()
            .map_err(|_| error("decoder state lock poisoned"))?;
          if let Some(event) = info.event() {
            match event.view() {
              gst::EventView::FlushStart(_) | gst::EventView::FlushStop(_) => {
                state.mappings = Mappings::default();
              }
              gst::EventView::StreamStart(_) => {
                *state = DecoderState::default();
                if let Some(sink) = sink.upgrade() {
                  sink.set_caps(Some(
                    &gst::Caps::builder("video/x-raw")
                      .field("format", "RGBA")
                      .build(),
                  ));
                }
              }
              gst::EventView::Caps(event) if state.caps.as_deref() != Some(event.caps()) => {
                state.mappings = Mappings::default();
                state.framing = Framing::from_caps(event.caps())?;
                state.caps = Some(event.caps().to_owned());
              }
              _ => {}
            }
          }
          if let Some(buffer) = info.buffer_mut() {
            if state.caps.is_none() {
              if let Some(caps) = pad.current_caps() {
                state.framing = Framing::from_caps(&caps)?;
                state.caps = Some(caps);
              }
            }
            let selected_before = state.selected;
            let parsed = state.buffer(buffer)?;
            if state.selected && !selected_before {
              sink
                .upgrade()
                .ok_or_else(|| error("video sink disappeared"))?
                .set_caps(Some(
                  &gst::Caps::builder("video/x-raw")
                    .field("format", "P010_10LE")
                    .build(),
                ));
            }
            let Some(frame) = parsed else {
              return Ok(());
            };
            let carrier = Carrier(Arc::new(CarriedFrame {
              pts: buffer.pts(),
              duration: buffer.duration(),
              frame,
            }));
            let mut meta =
              gst::meta::CustomMeta::add(buffer.make_mut(), META_NAME).map_err(error)?;
            meta.mut_structure().set("frame", carrier);
          }
          Ok(())
        })();
        if let Err(error) = result {
          if let Ok(mut shared) = shared.lock() {
            shared.error.get_or_insert(error);
          }
        }
        gst::PadProbeReturn::Ok
      },
    );
    if let Ok(mut shared) = callback_shared.lock() {
      match id {
        Some(id) => shared.probes.push((pad, id)),
        None => {
          shared
            .error
            .get_or_insert_with(|| error("cannot install decoder RPU probe"));
        }
      }
    }
  });
  Capture {
    pipeline: pipeline.downgrade(),
    signal: Some(signal),
    shared,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use dolby_vision::rpu::generate::GenerateConfig;

  fn full(id: u64) -> DoviRpu {
    let mut rpu = DoviRpu::profile5_config(&GenerateConfig::default()).unwrap();
    rpu.rpu_data_mapping.as_mut().unwrap().vdr_rpu_id = id;
    rpu
  }

  #[test]
  fn segment_clipping_preserves_only_the_intersecting_frames_rpu() {
    gst::init().unwrap();
    LazyLock::force(&REGISTER_META);
    let mut rpu = full(0);
    let mapping = Arc::new(rpu.rpu_data_mapping.take().unwrap());
    let expected = Arc::new(DoviFrame { rpu, mapping });
    let sample = |start_ms| {
      let mut buffer = gst::Buffer::new();
      buffer
        .get_mut()
        .unwrap()
        .set_pts(gst::ClockTime::from_mseconds(start_ms));
      let carrier = Carrier(Arc::new(CarriedFrame {
        pts: Some(gst::ClockTime::from_seconds(30)),
        duration: Some(gst::ClockTime::from_mseconds(41)),
        frame: expected.clone(),
      }));
      gst::meta::CustomMeta::add(buffer.get_mut().unwrap(), META_NAME)
        .unwrap()
        .mut_structure()
        .set("frame", carrier);
      let mut segment = gst::FormattedSegment::<gst::ClockTime>::new();
      segment.set_start(gst::ClockTime::from_mseconds(start_ms));
      gst::Sample::builder()
        .buffer(&buffer)
        .segment(segment.upcast_ref())
        .build()
    };
    assert!(Arc::ptr_eq(
      &frame(&sample(30_013)).unwrap().unwrap(),
      &expected
    ));
    assert!(
      frame(&sample(30_041)).is_err(),
      "an RPU ending at the segment boundary is stale"
    );
    assert!(
      frame(&sample(29_999)).is_err(),
      "clipping cannot move a frame backward"
    );
  }

  #[test]
  fn rejected_rpu_fails_capture_without_a_decoded_or_rendered_frame() {
    gst::init().expect("GStreamer initialization");
    let pipeline = gst::Pipeline::new();
    let sink = gst_app::AppSink::builder().build();
    let capture = install(&pipeline, &sink);
    let source = gst_app::AppSrc::builder()
      .caps(
        &gst::Caps::builder("video/x-h265")
          .field("stream-format", "byte-stream")
          .field("alignment", "au")
          .build(),
      )
      .format(gst::Format::Time)
      .build();
    let decoder = gst::ElementFactory::make("avdec_h265")
      .build()
      .expect("HEVC decoder");
    pipeline
      .add_many([source.upcast_ref(), &decoder, sink.upcast_ref()])
      .unwrap();
    gst::Element::link_many([source.upcast_ref(), &decoder, sink.upcast_ref()]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut malformed = gst::Buffer::from_slice(vec![0, 0, 0, 1, 124, 1, 25, 0]);
    malformed.get_mut().unwrap().set_pts(gst::ClockTime::ZERO);
    source.push_buffer(malformed).unwrap();
    source.end_of_stream().unwrap();
    let terminated = pipeline.bus().unwrap().timed_pop_filtered(
      gst::ClockTime::from_seconds(5),
      &[gst::MessageType::Error, gst::MessageType::Eos],
    );
    pipeline.set_state(gst::State::Null).unwrap();
    assert!(
      terminated.is_some(),
      "native decoder did not finish the malformed access unit"
    );
    assert!(matches!(capture.error(), Some(PlaybackError::Frame(_))));
  }

  #[test]
  fn mapping_replacement_keeps_in_flight_frames_and_flush_forbids_references() {
    let mut mappings = Mappings::default();
    let first = mappings.resolve(&mut full(7)).unwrap();
    let mut previous = full(7);
    previous.header.use_prev_vdr_rpu_flag = true;
    previous.header.prev_vdr_rpu_id = 7;
    assert_eq!(
      first.curves[0].pivots,
      mappings.resolve(&mut previous).unwrap().curves[0].pivots
    );
    let mut replacement = full(7);
    replacement.rpu_data_mapping.as_mut().unwrap().curves[0].pivots[0] = 1;
    let second = mappings.resolve(&mut replacement).unwrap();
    assert_ne!(first.curves[0].pivots[0], second.curves[0].pivots[0]);
    assert_eq!(
      second.curves[0].pivots,
      mappings.resolve(&mut previous).unwrap().curves[0].pivots
    );
    mappings = Mappings::default();
    assert!(mappings.resolve(&mut previous).is_err());
    assert!(mappings.resolve(&mut full(16)).is_err());
  }

  #[test]
  fn genuine_profile5_syntax_passes_all_framings_and_rejects_duplicate_rpus() {
    let encoded = full(0).write_hevc_unspec62_nalu().unwrap();
    assert_eq!(parse(&encoded).unwrap().dovi_profile, 5);
    let mut annex_b = vec![0, 0, 0, 1];
    annex_b.extend_from_slice(&encoded);
    assert_eq!(
      Framing::AnnexB.rpu(&annex_b).unwrap(),
      Some(encoded.as_slice())
    );
    for width in 1..=4 {
      if encoded.len() >= (1u64 << (width * 8)) as usize {
        continue;
      }
      let length = (encoded.len() as u32).to_be_bytes();
      let mut au = length[4 - width..].to_vec();
      au.extend_from_slice(&encoded);
      assert_eq!(
        Framing::Length(width).rpu(&au).unwrap(),
        Some(encoded.as_slice())
      );
      au.pop();
      assert!(Framing::Length(width).rpu(&au).is_err());
    }
    annex_b.extend_from_slice(&[0, 0, 1]);
    annex_b.extend_from_slice(&encoded);
    assert!(Framing::AnnexB.rpu(&annex_b).is_err());
  }

  #[test]
  fn missing_current_dm_is_not_replaced_by_previous_frame_metadata() {
    let mut rpu = full(0);
    rpu.header.vdr_dm_metadata_present_flag = false;
    rpu.vdr_dm_data = None;
    assert!(parse(&rpu.write_hevc_unspec62_nalu().unwrap()).is_err());
  }

  #[test]
  fn oversized_extension_count_is_rejected_before_parser_allocation() {
    // Exp-Golomb 65535: sixteen zero bits, a one, sixteen zero bits.
    let bytes = [0, 0, 0x80, 0, 0];
    let mut bits = Bits {
      data: &bytes,
      position: 0,
    };
    assert!(bits.extensions().is_err());
  }
}
