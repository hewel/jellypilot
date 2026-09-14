//! Typed playback statistics sampled over MPV JSON IPC.
//!
//! Property semantics follow the pinned MPV manual (`DOCS/man/input.rst`) and
//! the builtin `player/lua/stats.lua` script. Every field is optional: MPV
//! reports many properties only while the matching decoder, output driver, or
//! demuxer feature is active, and unavailable properties stay `None` instead
//! of being replaced with invented values. Transport failures (disconnect,
//! timeout) surface as [`MpvError`].

use crate::{MpvClient, MpvError, PropertyValue};

/// Cloneable handle that samples playback statistics from a live MPV IPC
/// connection.
///
/// Obtain one from `PlaybackController::statistics_reader`. The reader shares
/// the controller's IPC connection but holds no controller state, so sampling
/// never blocks controller work. Each call issues fresh property reads; the
/// reader itself never polls.
#[derive(Clone)]
pub struct StatisticsReader {
  mpv: MpvClient,
}

/// A coherent statistics sample for one playing file.
///
/// `playlist_entry_id` identifies the MPV playlist entry the sample describes.
/// [`StatisticsReader::sample`] verifies the entry identity before and after
/// sampling and fails with [`MpvError::MediaChanged`] instead of returning a
/// mixture of two files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaybackStatistics {
  /// `playlist/N/id` of the entry marked `playing`. Unique for the lifetime of
  /// the MPV core; populated only by a successfully identity-checked sample.
  pub playlist_entry_id: Option<i64>,
  /// Basename derived from `path`, parsing URL components before decoding.
  /// Never includes URL credentials, query parameters, or fragments.
  pub filename: Option<String>,
  /// `file-format`: symbolic container name (may be a comma-separated list).
  pub container_format: Option<String>,
  /// `file-size`: source length in bytes.
  pub file_size_bytes: Option<u64>,
  /// Demuxer cache counters from `demuxer-cache-state`.
  pub cache: CacheStatistics,
  /// `demuxer-cache-state/seekable-ranges`: buffered regions that can actually
  /// be seeked to, sorted by start. MPV reports them in arbitrary order and
  /// they may overlap; they are returned as reported, never merged into fake
  /// contiguous progress.
  pub buffered_ranges: Vec<(f64, f64)>,
  /// Video output and render timing facts.
  pub output: OutputStatistics,
  /// Decoded video track facts; `None` when no video track or parameters exist.
  pub video: Option<VideoStatistics>,
  /// Audio track and output facts; `None` when no audio track or params exist.
  pub audio: Option<AudioStatistics>,
}

/// Demuxer cache counters from `demuxer-cache-state`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CacheStatistics {
  /// `fw-bytes`: packet bytes buffered from the current decoding position.
  pub forward_bytes: Option<u64>,
  /// `total-bytes`: packet bytes of the entire queue including cached ranges.
  pub total_bytes: Option<u64>,
  /// `cache-duration`: approximate buffered duration in seconds. MPV calls
  /// this guess unreliable and often unavailable.
  pub duration_seconds: Option<f64>,
}

/// Video output driver and render timing facts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputStatistics {
  /// `current-vo`: active video output driver name.
  pub vo: Option<String>,
  /// `current-gpu-context`: GPU context of the VO (`gpu`/`gpu-next` only).
  pub gpu_context: Option<String>,
  /// `display-fps`: refresh rate reported by system APIs, in Hz. This is the
  /// lowest rate of the covered displays, not a measurement.
  pub display_fps: Option<f64>,
  /// `avsync`: last A/V synchronization difference in seconds.
  pub av_sync_seconds: Option<f64>,
  /// `decoder-frame-drop-count`: frames dropped by the decoder.
  pub decoder_dropped_frames: Option<u64>,
  /// `frame-drop-count`: frames dropped by the VO.
  pub output_dropped_frames: Option<u64>,
  /// `vo-passes` summed per frame type, matching the stats script's condensed
  /// view. Empty when the VO does not implement pass introspection; only
  /// implemented by MPV's own VOs.
  pub pass_timings: Vec<PassTiming>,
}

/// Summed execution time of one `vo-passes` frame type (`fresh`, `redraw`).
///
/// All values are nanoseconds, as documented for `vo-passes/TYPE/N/last|avg|
/// peak`. `avg` covers a handful of seconds; `peak` is the maximum inside that
/// averaging window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassTiming {
  /// Frame type key reported by MPV, e.g. `fresh` or `redraw`.
  pub frame_type: String,
  /// Sum of `last` across the type's passes, in nanoseconds.
  pub last_ns: u64,
  /// Sum of `avg` across the type's passes, in nanoseconds.
  pub avg_ns: u64,
  /// Sum of `peak` across the type's passes, in nanoseconds.
  pub peak_ns: u64,
}

/// Decoded video track facts from `current-tracks/video` and `video-params`.
///
/// Color and HDR fields describe the source stream as decoded, not the output
/// chain (`video-out-params`/`video-target-params` are deliberately unused).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoStatistics {
  /// `track-list` codec name, e.g. `h264`, `hevc`, `av1`.
  pub codec: Option<String>,
  /// `codec-profile`: codec profile when the track carries one.
  pub codec_profile: Option<String>,
  /// `hwdec-current`: active hardware decoder; `"no"` means software
  /// decoding. `None` while no decoder is loaded.
  pub hwdec: Option<String>,
  /// `video-params/w`: decoded width without aspect correction.
  pub width: Option<u32>,
  /// `video-params/h`: decoded height without aspect correction.
  pub height: Option<u32>,
  /// `container-fps`: container-declared frame rate; MPV warns this can be
  /// bogus for modern containers.
  pub container_fps: Option<f64>,
  /// `video-params/pixelformat`.
  pub pixel_format: Option<String>,
  /// `video-params/hw-pixelformat`: underlying format for some hardware
  /// decoding paths.
  pub hw_pixel_format: Option<String>,
  /// `video-params/primaries`.
  pub primaries: Option<String>,
  /// `video-params/gamma`: transfer function in use.
  pub gamma: Option<String>,
  /// `track-list` Dolby Vision profile when the container reports it.
  pub dolby_vision_profile: Option<u64>,
  /// `video-bitrate`: packet-level bitrate in bits per second.
  pub bitrate: Option<u64>,
}

/// Audio track and output facts from `current-tracks/audio`, `audio-params`,
/// and `audio-out-params`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioStatistics {
  /// `track-list` codec name, e.g. `aac`, `truehd`.
  pub codec: Option<String>,
  /// `track-list` language tag.
  pub language: Option<String>,
  /// `audio-params/channels`: channel layout from the decoder.
  pub channels: Option<String>,
  /// `audio-out-params/samplerate` in Hz.
  pub output_sample_rate: Option<u32>,
  /// `audio-bitrate`: packet-level bitrate in bits per second.
  pub bitrate: Option<u64>,
  /// `current-ao`: active audio output driver name.
  pub ao: Option<String>,
  /// `volume`: MPV mixer volume, may exceed 100 with amplification.
  pub volume: Option<f64>,
  /// `mute`: MPV mixer mute flag.
  pub muted: Option<bool>,
}

impl StatisticsReader {
  pub(crate) fn new(mpv: MpvClient) -> Self {
    Self { mpv }
  }

  /// Sample a coherent statistics snapshot for the currently playing file.
  ///
  /// Issues one batch of property reads. Properties MPV reports as unavailable
  /// become `None`; transport failures return `Err`. When the playing playlist
  /// entry changes mid-sample the result is discarded with
  /// [`MpvError::MediaChanged`] so a sample never mixes two files.
  ///
  /// # Errors
  ///
  /// Returns [`MpvError`] on IPC disconnect, timeout, or when MPV is not
  /// connected, and [`MpvError::MediaChanged`] on a mid-sample file switch.
  pub async fn sample(&self) -> Result<PlaybackStatistics, MpvError> {
    let before = self.playing_entry_id().await?;
    let statistics = self.sample_properties(before).await?;
    let after = self.playing_entry_id().await?;
    if before != after {
      return Err(MpvError::MediaChanged);
    }
    Ok(statistics)
  }

  /// Read the currently buffered seekable ranges from `demuxer-cache-state`.
  ///
  /// Intended for cheap timeline updates. Ranges are sorted by start and
  /// returned as MPV reports them (possibly overlapping); malformed or
  /// non-finite entries are dropped. An unavailable `demuxer-cache-state`
  /// property is a real error here, unlike inside [`Self::sample`] where it
  /// degrades to empty data.
  ///
  /// The playing playlist entry is verified before and after the read so a
  /// mid-query file replacement fails with [`MpvError::MediaChanged`] instead
  /// of returning ranges for the wrong file.
  ///
  /// # Errors
  ///
  /// Returns [`MpvError`] when the property query fails or the transport
  /// fails, and [`MpvError::MediaChanged`] on a mid-query file switch.
  pub async fn buffered_ranges(&self) -> Result<Vec<(f64, f64)>, MpvError> {
    let before = self.playing_entry_id().await?;
    let value = self.mpv.get_property("demuxer-cache-state").await?;
    let after = self.playing_entry_id().await?;
    if before != after {
      return Err(MpvError::MediaChanged);
    }
    Ok(seekable_ranges(&decode_json(Some(value))))
  }

  /// Identity of the playlist entry MPV marks `playing`, from one `playlist`
  /// read so the position and id cannot race each other.
  async fn playing_entry_id(&self) -> Result<Option<i64>, MpvError> {
    let value =
      decode_json(property(&self.mpv, "playlist").await?).ok_or(MpvError::CommandFailed)?;
    let entries = value.as_array().ok_or(MpvError::CommandFailed)?;
    let entry = entries
      .iter()
      .find(|entry| json_bool(entry, "playing") == Some(true))
      .ok_or(MpvError::MediaChanged)?;
    entry
      .get("id")
      .and_then(serde_json::Value::as_i64)
      .map(Some)
      .ok_or(MpvError::MediaChanged)
  }

  async fn sample_properties(
    &self,
    playlist_entry_id: Option<i64>,
  ) -> Result<PlaybackStatistics, MpvError> {
    let mpv = &self.mpv;
    let (file, output, video, audio) = tokio::join!(
      async {
        tokio::join!(
          property(mpv, "path"),
          property(mpv, "file-format"),
          property(mpv, "file-size"),
          property(mpv, "demuxer-cache-state"),
        )
      },
      async {
        tokio::join!(
          property(mpv, "current-vo"),
          property(mpv, "current-gpu-context"),
          property(mpv, "display-fps"),
          property(mpv, "avsync"),
          property(mpv, "decoder-frame-drop-count"),
          property(mpv, "frame-drop-count"),
          property(mpv, "vo-passes"),
        )
      },
      async {
        tokio::join!(
          property(mpv, "current-tracks/video"),
          property(mpv, "video-params"),
          property(mpv, "hwdec-current"),
          property(mpv, "container-fps"),
          property(mpv, "video-bitrate"),
        )
      },
      async {
        tokio::join!(
          property(mpv, "current-tracks/audio"),
          property(mpv, "audio-params"),
          property(mpv, "audio-out-params"),
          property(mpv, "audio-bitrate"),
          property(mpv, "current-ao"),
          property(mpv, "volume"),
          property(mpv, "mute"),
        )
      },
    );

    let (path, file_format, file_size, demuxer_cache) = file;
    let (current_vo, gpu_context, display_fps, avsync, decoder_drops, output_drops, vo_passes) =
      output;
    let (video_track, video_params, hwdec, container_fps, video_bitrate) = video;
    let (audio_track, audio_params, audio_out_params, audio_bitrate, current_ao, volume, mute) =
      audio;

    // Remaining errors are transport failures; unavailable properties already
    // arrived as Ok(None). Propagate the first transport error.
    let path = path?;
    let file_format = file_format?;
    let file_size = file_size?;
    let demuxer_cache = demuxer_cache?;
    let current_vo = current_vo?;
    let gpu_context = gpu_context?;
    let display_fps = display_fps?;
    let avsync = avsync?;
    let decoder_drops = decoder_drops?;
    let output_drops = output_drops?;
    let vo_passes = vo_passes?;
    let video_track = video_track?;
    let video_params = video_params?;
    let hwdec = hwdec?;
    let container_fps = container_fps?;
    let video_bitrate = video_bitrate?;
    let audio_track = audio_track?;
    let audio_params = audio_params?;
    let audio_out_params = audio_out_params?;
    let audio_bitrate = audio_bitrate?;
    let current_ao = current_ao?;
    let volume = volume?;
    let mute = mute?;

    let demuxer_cache = decode_json(demuxer_cache);
    let cache = demuxer_cache
      .as_ref()
      .map_or_else(CacheStatistics::default, |state| CacheStatistics {
        forward_bytes: json_u64(state, "fw-bytes"),
        total_bytes: json_u64(state, "total-bytes"),
        duration_seconds: json_f64(state, "cache-duration").filter(|seconds| *seconds >= 0.0),
      });
    let buffered_ranges = seekable_ranges(&demuxer_cache);

    let output = OutputStatistics {
      vo: as_string(current_vo),
      gpu_context: as_string(gpu_context),
      display_fps: as_f64(display_fps).filter(|fps| *fps > 0.0),
      av_sync_seconds: as_f64(avsync),
      decoder_dropped_frames: as_u64(decoder_drops),
      output_dropped_frames: as_u64(output_drops),
      pass_timings: pass_timings(decode_json(vo_passes)),
    };

    let video_track = decode_json(video_track);
    let video_params = decode_json(video_params);
    let video = if video_track.is_some() || video_params.is_some() {
      let track = video_track.as_ref();
      let params = video_params.as_ref();
      Some(VideoStatistics {
        codec: track.and_then(|t| json_string(t, "codec")),
        codec_profile: track.and_then(|t| json_string(t, "codec-profile")),
        hwdec: as_string(hwdec),
        width: params
          .and_then(|p| json_u64(p, "w"))
          .and_then(|v| u32::try_from(v).ok()),
        height: params
          .and_then(|p| json_u64(p, "h"))
          .and_then(|v| u32::try_from(v).ok()),
        container_fps: as_f64(container_fps).filter(|fps| *fps > 0.0),
        pixel_format: params.and_then(|p| json_string(p, "pixelformat")),
        hw_pixel_format: params.and_then(|p| json_string(p, "hw-pixelformat")),
        primaries: params.and_then(|p| json_string(p, "primaries")),
        gamma: params.and_then(|p| json_string(p, "gamma")),
        dolby_vision_profile: track.and_then(|t| json_u64(t, "dolby-vision-profile")),
        bitrate: as_u64(video_bitrate),
      })
    } else {
      None
    };

    let audio_track = decode_json(audio_track);
    let audio_params = decode_json(audio_params);
    let audio_out_params = decode_json(audio_out_params);
    let ao = as_string(current_ao);
    let audio = if audio_track.is_some()
      || audio_params.is_some()
      || audio_out_params.is_some()
      || ao.is_some()
    {
      let track = audio_track.as_ref();
      let params = audio_params.as_ref();
      let out_params = audio_out_params.as_ref();
      Some(AudioStatistics {
        codec: track.and_then(|t| json_string(t, "codec")),
        language: track.and_then(|t| json_string(t, "lang")),
        channels: params.and_then(|p| json_string(p, "channels")),
        output_sample_rate: out_params
          .and_then(|p| json_u64(p, "samplerate"))
          .and_then(|v| u32::try_from(v).ok()),
        bitrate: as_u64(audio_bitrate),
        ao,
        volume: as_f64(volume),
        muted: as_bool(mute),
      })
    } else {
      None
    };

    Ok(PlaybackStatistics {
      playlist_entry_id,
      filename: as_string(path).and_then(|path| display_filename(&path)),
      container_format: as_string(file_format),
      file_size_bytes: as_u64(file_size),
      cache,
      buffered_ranges,
      output,
      video,
      audio,
    })
  }
}

/// Read one property, mapping MPV's "unavailable" outcomes to `None` while
/// preserving real transport failures.
async fn property(mpv: &MpvClient, name: &str) -> Result<Option<PropertyValue>, MpvError> {
  match mpv.get_property(name).await {
    Ok(PropertyValue::Null) => Ok(None),
    Ok(value) => Ok(Some(value)),
    Err(MpvError::CommandFailed) => Ok(None),
    Err(error) => Err(error),
  }
}

// MPV's `filename` decodes the entire URL before taking its basename. A slash
// inside a query can erase `?` and expose the remaining authentication fields.
fn display_filename(path: &str) -> Option<String> {
  if path.contains("://") {
    let url = url::Url::parse(path).ok()?;
    let basename = url.path_segments()?.next_back()?;
    if basename.is_empty() {
      return None;
    }
    Some(
      percent_encoding::percent_decode_str(basename)
        .decode_utf8_lossy()
        .into_owned(),
    )
  } else {
    path
      .rsplit(['/', '\\'])
      .next()
      .filter(|name| !name.is_empty())
      .map(str::to_owned)
  }
}

fn as_string(value: Option<PropertyValue>) -> Option<String> {
  match value {
    Some(PropertyValue::String(text)) if !text.is_empty() => Some(text),
    _ => None,
  }
}

fn as_f64(value: Option<PropertyValue>) -> Option<f64> {
  match value {
    Some(PropertyValue::Number(number)) if number.is_finite() => Some(number),
    _ => None,
  }
}

fn as_u64(value: Option<PropertyValue>) -> Option<u64> {
  match value {
    Some(PropertyValue::Number(number))
      if number.is_finite()
        && number >= 0.0
        && number < u64::MAX as f64
        && number.fract() == 0.0 =>
    {
      Some(number as u64)
    }
    _ => None,
  }
}

fn as_bool(value: Option<PropertyValue>) -> Option<bool> {
  match value {
    Some(PropertyValue::Bool(flag)) => Some(flag),
    _ => None,
  }
}

/// Node properties arrive as serialized JSON inside [`PropertyValue::Json`].
fn decode_json(value: Option<PropertyValue>) -> Option<serde_json::Value> {
  match value {
    Some(PropertyValue::Json(text)) => serde_json::from_str(&text).ok(),
    _ => None,
  }
}

fn json_string(map: &serde_json::Value, key: &str) -> Option<String> {
  map
    .get(key)
    .and_then(serde_json::Value::as_str)
    .filter(|text| !text.is_empty())
    .map(str::to_owned)
}

fn json_f64(map: &serde_json::Value, key: &str) -> Option<f64> {
  map
    .get(key)
    .and_then(serde_json::Value::as_f64)
    .filter(|n| n.is_finite())
}

fn json_u64(map: &serde_json::Value, key: &str) -> Option<u64> {
  map.get(key).and_then(serde_json::Value::as_u64)
}

fn json_bool(map: &serde_json::Value, key: &str) -> Option<bool> {
  map.get(key).and_then(serde_json::Value::as_bool)
}

/// `seekable-ranges` entries with finite `start`/`end`, sorted by start.
/// Overlapping ranges are kept as reported; merging them would invent
/// contiguous progress MPV did not confirm.
fn seekable_ranges(cache_state: &Option<serde_json::Value>) -> Vec<(f64, f64)> {
  let Some(ranges) = cache_state
    .as_ref()
    .and_then(|state| state.get("seekable-ranges"))
    .and_then(serde_json::Value::as_array)
  else {
    return Vec::new();
  };
  let mut ranges: Vec<(f64, f64)> = ranges
    .iter()
    .filter_map(|range| {
      let start = json_f64(range, "start")?;
      let end = json_f64(range, "end")?;
      (end > start).then_some((start, end))
    })
    .collect();
  ranges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
  ranges
}

/// Sum `vo-passes` per frame type like the stats script's condensed view.
/// Only the documented `last`/`avg`/`peak` aggregates are read; the raw
/// `samples` arrays are ignored. Passes missing any aggregate are skipped.
fn pass_timings(vo_passes: Option<serde_json::Value>) -> Vec<PassTiming> {
  let Some(value) = vo_passes else {
    return Vec::new();
  };
  let Some(types) = value.as_object() else {
    return Vec::new();
  };
  let mut timings: Vec<PassTiming> = types
    .iter()
    .filter_map(|(frame_type, passes)| {
      let passes = passes.as_array()?;
      let mut timing = PassTiming {
        frame_type: frame_type.clone(),
        last_ns: 0,
        avg_ns: 0,
        peak_ns: 0,
      };
      let mut counted = 0_u32;
      for pass in passes {
        let (Some(last), Some(avg), Some(peak)) = (
          json_u64(pass, "last"),
          json_u64(pass, "avg"),
          json_u64(pass, "peak"),
        ) else {
          continue;
        };
        timing.last_ns = timing.last_ns.saturating_add(last);
        timing.avg_ns = timing.avg_ns.saturating_add(avg);
        timing.peak_ns = timing.peak_ns.saturating_add(peak);
        counted += 1;
      }
      (counted > 0).then_some(timing)
    })
    .collect();
  timings.sort_by(|a, b| a.frame_type.cmp(&b.frame_type));
  timings
}

#[cfg(test)]
mod tests {
  use std::collections::{HashMap, VecDeque};

  use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

  use super::*;

  /// Fake MPV peer answering `get_property` commands from canned responses.
  /// `playlist_responses` are consumed in order; the last one repeats, which
  /// lets a test switch the playing entry between the pre- and post-sample
  /// identity reads.
  async fn spawn_peer(
    properties: HashMap<String, serde_json::Value>,
    playlist_responses: VecDeque<serde_json::Value>,
  ) -> (StatisticsReader, tokio::task::JoinHandle<()>) {
    let (client_stream, peer_stream) = duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(client_stream);
    let client = MpvClient::from_io_for_test(reader, writer)
      .await
      .expect("test client should be constructed");
    let (peer_reader, mut peer_writer) = tokio::io::split(peer_stream);
    let peer = tokio::spawn(async move {
      let mut lines = BufReader::new(peer_reader).lines();
      let mut playlist_responses = playlist_responses;
      while let Ok(Some(line)) = lines.next_line().await {
        let Ok(command) = serde_json::from_str::<serde_json::Value>(&line) else {
          continue;
        };
        let Some(request_id) = command.get("request_id").and_then(|v| v.as_i64()) else {
          continue;
        };
        let property = command
          .get("command")
          .and_then(|c| c.get(1))
          .and_then(|v| v.as_str())
          .unwrap_or_default()
          .to_owned();
        let response = if property == "playlist" && !playlist_responses.is_empty() {
          let data = if playlist_responses.len() > 1 {
            playlist_responses.pop_front().unwrap()
          } else {
            playlist_responses.front().unwrap().clone()
          };
          serde_json::json!({"error": "success", "data": data, "request_id": request_id})
        } else if let Some(data) = properties.get(&property) {
          serde_json::json!({"error": "success", "data": data, "request_id": request_id})
        } else {
          serde_json::json!({"error": "property unavailable", "request_id": request_id})
        };
        if peer_writer
          .write_all(format!("{response}\n").as_bytes())
          .await
          .is_err()
        {
          break;
        }
      }
    });
    (StatisticsReader::new(client), peer)
  }

  fn playing_playlist(id: i64) -> serde_json::Value {
    serde_json::json!([{"filename": "movie.mkv", "playing": true, "current": true, "id": id}])
  }

  fn populated_properties() -> HashMap<String, serde_json::Value> {
    HashMap::from([
      (
        "path".to_owned(),
        serde_json::json!(
          "https://user:password@server/%E7%89%87.mkv?path=%2Fmovie.mkv&api_key=secret#fragment"
        ),
      ),
      (
        "filename".to_owned(),
        serde_json::json!("movie.mkv&api_key=secret"),
      ),
      ("file-size".to_owned(), serde_json::json!(-1)),
      (
        "video-params".to_owned(),
        serde_json::json!({"w": 320.5, "h": 180}),
      ),
      (
        "demuxer-cache-state".to_owned(),
        serde_json::json!({
          "seekable-ranges": [
            {"start": 120.0, "end": 240.0},
            {"start": 0.0, "end": 60.0},
            {"start": 45.0, "end": 90.0},
            {"start": "bad", "end": 70.0},
            {"start": 300.0, "end": 300.0}
          ]
        }),
      ),
      (
        "vo-passes".to_owned(),
        serde_json::json!({
          "fresh": [
            {"last": 400_000, "avg": 380_000, "peak": 900_000, "samples": [1, 2, 3]},
            {"last": 600_000, "avg": 620_000, "peak": 1_100_000, "samples": [4, 5]}
          ],
          "redraw": []
        }),
      ),
    ])
  }

  #[tokio::test]
  async fn sample_preserves_unavailable_values_and_sanitizes_untrusted_metadata() {
    let (reader, peer) = spawn_peer(
      populated_properties(),
      VecDeque::from([playing_playlist(42)]),
    )
    .await;
    let stats = reader.sample().await.expect("sample");
    assert_eq!(stats.filename.as_deref(), Some("片.mkv"));
    assert_eq!(
      stats.file_size_bytes, None,
      "invalid sizes must not become zero"
    );
    assert_eq!(
      stats.video.unwrap().width,
      None,
      "fractional dimensions are not pixels"
    );
    assert_eq!(
      stats.output.display_fps, None,
      "unavailable refresh must not become zero Hz"
    );
    assert_eq!(
      stats.buffered_ranges,
      vec![(0.0, 60.0), (45.0, 90.0), (120.0, 240.0)]
    );
    assert_eq!(
      stats.output.pass_timings,
      vec![PassTiming {
        frame_type: "fresh".into(),
        last_ns: 1_000_000,
        avg_ns: 1_000_000,
        peak_ns: 2_000_000,
      }],
      "aggregate GPU passes, not the raw samples; empty frame types are unavailable"
    );
    drop(reader);
    peer.await.unwrap();
  }

  #[tokio::test]
  async fn sample_requires_a_readable_playing_identity() {
    let (reader, peer) = spawn_peer(populated_properties(), VecDeque::new()).await;
    assert!(matches!(
      reader.sample().await,
      Err(MpvError::CommandFailed)
    ));
    drop(reader);
    peer.await.unwrap();
  }

  #[tokio::test]
  async fn sample_rejects_media_replaced_mid_sample() {
    let (reader, peer) = spawn_peer(
      populated_properties(),
      VecDeque::from([playing_playlist(42), playing_playlist(43)]),
    )
    .await;

    let result = reader.sample().await;

    assert!(
      matches!(result, Err(MpvError::MediaChanged)),
      "expected MediaChanged, got {result:?}"
    );

    drop(reader);
    peer.await.expect("peer task should finish");
  }

  #[tokio::test]
  async fn buffered_ranges_rejects_media_replaced_mid_query() {
    let (reader, peer) = spawn_peer(
      populated_properties(),
      VecDeque::from([playing_playlist(42), playing_playlist(43)]),
    )
    .await;

    let result = reader.buffered_ranges().await;

    assert!(
      matches!(result, Err(MpvError::MediaChanged)),
      "expected MediaChanged, got {result:?}"
    );

    drop(reader);
    peer.await.expect("peer task should finish");
  }

  #[tokio::test]
  async fn sample_surfaces_transport_failure() {
    let (client_stream, peer_stream) = duplex(1024);
    let (reader, writer) = tokio::io::split(client_stream);
    let client = MpvClient::from_io_for_test(reader, writer)
      .await
      .expect("test client should be constructed");
    drop(peer_stream);
    let reader = StatisticsReader::new(client);

    let result = reader.sample().await;

    assert!(
      matches!(
        result,
        Err(MpvError::IpcDisconnected) | Err(MpvError::NotConnected)
      ),
      "expected transport error, got {result:?}"
    );
  }
}
