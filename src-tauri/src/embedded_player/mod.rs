//! Native adapter for the UI-agnostic embedded playback core.

mod proxy;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use jellypilot_playback_core::{
  ffmpeg_argv, AudioChannelLayout, BrowserObservation, BrowserPlaybackCapabilities,
  BrowserPlaybackState, EmbeddedPlaybackCore, FfmpegCliRequest, FfmpegEncoderAvailability,
  FfmpegPlanRequest, FfmpegPlatform, MediaDelivery, MediaProbeFacts, MediaProbeFailure,
  MediaProbeResult, PlaybackAction, PlaybackCommand, PlaybackGeneration,
  PlaybackObservationDisposition, PlaybackObservationToken, PlaybackPhase, PlaybackReport,
  PlaybackSession, PlaybackSnapshot, ProbedAudioCodec, ProbedAudioStream, ProbedContainer,
  ProbedDynamicRange, ProbedPixelFormat, ProbedVideoCodec, ProbedVideoSampleEntry,
  ProbedVideoStream, SourceVideoProfile,
};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tauri_specta::Event;

use crate::config::AppConfig;
use crate::jellyfin::{
  seconds_to_ticks, JellyfinClient, PlaybackProgressInfo, PlaybackStartInfo, PlaybackStopInfo,
};

use self::proxy::{LoopbackMediaServer, SourceProxySnapshot};

const MAX_FFMPEG_STARTUP_DIAGNOSTIC_BYTES: usize = 4096;
const MAX_FFPROBE_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_PROBE_DIAGNOSTIC_BYTES: usize = 4096;
const FFPROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
enum FfmpegStartupFailureKind {
  Exited,
  Deadline,
}

#[derive(Default)]
struct FfmpegStartupDiagnostics {
  output: String,
  exit_code: Option<Option<i32>>,
}

impl FfmpegStartupDiagnostics {
  fn push(&mut self, value: &str) {
    self.output.push_str(&sanitize_ffmpeg_diagnostic(value));
    if self.output.len() > MAX_FFMPEG_STARTUP_DIAGNOSTIC_BYTES {
      let mut start = self.output.len() - MAX_FFMPEG_STARTUP_DIAGNOSTIC_BYTES;
      while !self.output.is_char_boundary(start) {
        start += 1;
      }
      self.output.drain(..start);
    }
  }

  fn record_exit(&mut self, code: Option<i32>) {
    self.exit_code = Some(code);
  }

  fn detail(&self, kind: FfmpegStartupFailureKind, proxy_summary: &str) -> String {
    let status = match (kind, self.exit_code) {
      (FfmpegStartupFailureKind::Exited, Some(Some(code))) => {
        format!("FFmpeg exited with status {code}")
      }
      (FfmpegStartupFailureKind::Exited, _) => "FFmpeg exited without a status code".to_string(),
      (FfmpegStartupFailureKind::Deadline, _) => {
        "FFmpeg was still running at the startup deadline".to_string()
      }
    };
    let output = self.output.trim();
    if output.is_empty() {
      format!("{proxy_summary}; {status}; FFmpeg produced no diagnostic output")
    } else {
      format!("{proxy_summary}; {status}; FFmpeg: {output}")
    }
  }
}

fn sanitize_ffmpeg_diagnostic(value: &str) -> String {
  let mut sanitized = String::with_capacity(value.len());
  let mut remaining = value;
  while let Some(start) = ["http://", "https://"]
    .into_iter()
    .filter_map(|marker| remaining.find(marker))
    .min()
  {
    sanitized.push_str(&remaining[..start]);
    sanitized.push_str("[REDACTED_URL]");
    let url = &remaining[start..];
    let end = url
      .find(|character: char| {
        character.is_whitespace() || matches!(character, '\"' | '\'' | ']' | '>' | ')')
      })
      .unwrap_or(url.len());
    remaining = &url[end..];
  }
  sanitized.push_str(remaining);
  sanitized
    .chars()
    .map(|character| {
      if character.is_control() && !matches!(character, '\n' | '\t') {
        ' '
      } else {
        character
      }
    })
    .collect()
}

#[derive(Debug)]
struct BoundedSidecarOutput {
  stdout: Vec<u8>,
  stderr: String,
  status: Option<i32>,
}

#[derive(Deserialize)]
struct FfprobeDocument {
  #[serde(default)]
  streams: Vec<FfprobeStream>,
  format: Option<FfprobeFormat>,
}

#[derive(Deserialize)]
struct FfprobeFormat {
  #[serde(default)]
  format_name: String,
  tags: Option<FfprobeFormatTags>,
}

#[derive(Deserialize)]
struct FfprobeFormatTags {
  major_brand: Option<String>,
}

#[derive(Deserialize)]
struct FfprobeStream {
  index: Option<u32>,
  codec_type: Option<String>,
  codec_name: Option<String>,
  profile: Option<String>,
  pix_fmt: Option<String>,
  codec_tag_string: Option<String>,
  channels: Option<u32>,
  color_transfer: Option<String>,
  disposition: Option<FfprobeDisposition>,
  #[serde(default)]
  side_data_list: Vec<FfprobeSideData>,
}

#[derive(Deserialize)]
struct FfprobeDisposition {
  #[serde(default)]
  attached_pic: u8,
}

#[derive(Deserialize)]
struct FfprobeSideData {
  side_data_type: Option<String>,
}

async fn run_bounded_sidecar(
  app: &AppHandle,
  sidecar: &str,
  args: Vec<String>,
  timeout: Duration,
  stdout_limit: usize,
) -> Result<BoundedSidecarOutput, MediaProbeFailure> {
  let (mut events, child) = app
    .shell()
    .sidecar(sidecar)
    .map_err(|_| MediaProbeFailure::SidecarUnavailable)?
    .args(args)
    .spawn()
    .map_err(|_| MediaProbeFailure::SidecarUnavailable)?;
  let deadline = tokio::time::Instant::now() + timeout;
  let mut stdout = Vec::new();
  let mut stderr = String::new();
  let mut status = None;

  loop {
    let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
      let _ = child.kill();
      return Err(MediaProbeFailure::Timeout);
    };
    let event = match tokio::time::timeout(remaining, events.recv()).await {
      Ok(event) => event,
      Err(_) => {
        let _ = child.kill();
        return Err(MediaProbeFailure::Timeout);
      }
    };
    let Some(event) = event else {
      break;
    };
    match event {
      CommandEvent::Stdout(bytes) => {
        if stdout.len().saturating_add(bytes.len()) > stdout_limit {
          let _ = child.kill();
          return Err(MediaProbeFailure::OutputTooLarge);
        }
        stdout.extend_from_slice(&bytes);
        stdout.push(b'\n');
      }
      CommandEvent::Stderr(bytes) => append_bounded_diagnostic(
        &mut stderr,
        &String::from_utf8_lossy(&bytes),
        MAX_PROBE_DIAGNOSTIC_BYTES,
      ),
      CommandEvent::Error(message) => {
        append_bounded_diagnostic(&mut stderr, &message, MAX_PROBE_DIAGNOSTIC_BYTES);
      }
      CommandEvent::Terminated(payload) => status = payload.code,
      _ => {}
    }
  }

  Ok(BoundedSidecarOutput {
    stdout,
    stderr,
    status,
  })
}

fn append_bounded_diagnostic(target: &mut String, value: &str, limit: usize) {
  target.push_str(&sanitize_ffmpeg_diagnostic(value));
  target.push('\n');
  if target.len() <= limit {
    return;
  }
  let mut start = target.len() - limit;
  while !target.is_char_boundary(start) {
    start += 1;
  }
  target.drain(..start);
}

fn normalize_ffprobe(
  document: &FfprobeDocument,
  selected_audio_stream_index: Option<i32>,
  source_profile: SourceVideoProfile,
) -> Result<MediaProbeFacts, MediaProbeFailure> {
  let video_streams = document
    .streams
    .iter()
    .filter(|stream| {
      stream.codec_type.as_deref() == Some("video")
        && stream
          .disposition
          .as_ref()
          .is_none_or(|disposition| disposition.attached_pic == 0)
    })
    .collect::<Vec<_>>();
  let audio_streams = document
    .streams
    .iter()
    .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
    .collect::<Vec<_>>();
  let video_stream = video_streams
    .first()
    .copied()
    .ok_or(MediaProbeFailure::MissingVideoStream)?;
  let audio_stream = match selected_audio_stream_index {
    Some(selected) => {
      let selected =
        u32::try_from(selected).map_err(|_| MediaProbeFailure::SelectedAudioStreamMissing)?;
      Some(
        audio_streams
          .iter()
          .copied()
          .find(|stream| stream.index == Some(selected))
          .ok_or(MediaProbeFailure::SelectedAudioStreamMissing)?,
      )
    }
    None => audio_streams.first().copied(),
  };
  let video_stream_count =
    u16::try_from(video_streams.len()).map_err(|_| MediaProbeFailure::InvalidOutput)?;
  let audio_stream_count =
    u16::try_from(audio_streams.len()).map_err(|_| MediaProbeFailure::InvalidOutput)?;

  Ok(MediaProbeFacts {
    container: normalize_container(document.format.as_ref()),
    video: normalize_video_stream(video_stream, source_profile)?,
    audio: audio_stream.map(normalize_audio_stream).transpose()?,
    video_stream_count,
    audio_stream_count,
  })
}

fn normalize_container(format: Option<&FfprobeFormat>) -> ProbedContainer {
  let Some(format) = format else {
    return ProbedContainer::Other;
  };
  let mp4_family = format
    .format_name
    .split(',')
    .any(|name| matches!(name.trim(), "mov" | "mp4"));
  let major_brand = format
    .tags
    .as_ref()
    .and_then(|tags| tags.major_brand.as_deref())
    .map(str::trim);
  let strict_mp4_brand = major_brand.is_some_and(|brand| {
    matches!(
      brand.to_ascii_lowercase().as_str(),
      "isom"
        | "iso2"
        | "iso3"
        | "iso4"
        | "iso5"
        | "iso6"
        | "mp41"
        | "mp42"
        | "m4v"
        | "avc1"
        | "dash"
    )
  });
  if mp4_family && strict_mp4_brand {
    ProbedContainer::Mp4
  } else {
    ProbedContainer::Other
  }
}

fn normalize_video_stream(
  stream: &FfprobeStream,
  source_profile: SourceVideoProfile,
) -> Result<ProbedVideoStream, MediaProbeFailure> {
  let codec = match stream.codec_name.as_deref() {
    Some("h264") => ProbedVideoCodec::H264,
    Some("hevc") => ProbedVideoCodec::Hevc,
    _ => ProbedVideoCodec::Other,
  };
  let pixel_format = match stream.pix_fmt.as_deref() {
    Some("yuv420p") | Some("yuvj420p") => ProbedPixelFormat::Yuv420p,
    Some("p010le") | Some("p010be") | Some("yuv420p10le") | Some("yuv420p10be") => {
      ProbedPixelFormat::TenBit420
    }
    _ => ProbedPixelFormat::Other,
  };
  let sample_entry = match stream.codec_tag_string.as_deref() {
    Some(value) if value.eq_ignore_ascii_case("avc1") => ProbedVideoSampleEntry::Avc1,
    Some(value) if value.eq_ignore_ascii_case("hvc1") => ProbedVideoSampleEntry::Hvc1,
    _ => ProbedVideoSampleEntry::Other,
  };
  let reported_hdr = stream
    .color_transfer
    .as_deref()
    .is_some_and(|transfer| matches!(transfer, "smpte2084" | "arib-std-b67"))
    || stream.side_data_list.iter().any(|side_data| {
      side_data.side_data_type.as_deref().is_some_and(|kind| {
        let kind = kind.to_ascii_lowercase();
        kind.contains("mastering display")
          || kind.contains("content light")
          || kind.contains("dovi")
          || kind.contains("dolby vision")
      })
    });
  let dynamic_range = if reported_hdr || source_profile == SourceVideoProfile::HevcMain10Hdr {
    ProbedDynamicRange::Hdr
  } else {
    ProbedDynamicRange::Sdr
  };
  let hevc_main10 = stream
    .profile
    .as_deref()
    .is_some_and(|profile| profile.eq_ignore_ascii_case("main 10"));

  Ok(ProbedVideoStream {
    stream_index: stream.index.ok_or(MediaProbeFailure::InvalidOutput)?,
    codec,
    pixel_format,
    sample_entry,
    dynamic_range,
    hevc_main10,
  })
}

fn normalize_audio_stream(stream: &FfprobeStream) -> Result<ProbedAudioStream, MediaProbeFailure> {
  let channels = u8::try_from(
    stream
      .channels
      .ok_or(MediaProbeFailure::InvalidAudioChannelCount)?,
  )
  .map_err(|_| MediaProbeFailure::InvalidAudioChannelCount)?;
  if channels == 0 {
    return Err(MediaProbeFailure::InvalidAudioChannelCount);
  }
  Ok(ProbedAudioStream {
    stream_index: stream.index.ok_or(MediaProbeFailure::InvalidOutput)?,
    codec: if stream.codec_name.as_deref() == Some("aac")
      && stream
        .profile
        .as_deref()
        .is_some_and(|profile| profile.eq_ignore_ascii_case("LC"))
    {
      ProbedAudioCodec::Aac
    } else {
      ProbedAudioCodec::Other
    },
    channels,
  })
}

fn startup_error(
  kind: FfmpegStartupFailureKind,
  diagnostics: &FfmpegStartupDiagnostics,
  proxy: &SourceProxySnapshot,
) -> EmbeddedPlayerError {
  let detail = diagnostics.detail(kind, &proxy.summary);
  let retryable = !proxy.terminal_failure;
  match kind {
    FfmpegStartupFailureKind::Exited => {
      EmbeddedPlayerError::SidecarStartupExit { detail, retryable }
    }
    FfmpegStartupFailureKind::Deadline => {
      EmbeddedPlayerError::SidecarStartupTimeout { detail, retryable }
    }
  }
}

/// Error returned by the native embedded-player adapter.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddedPlayerError {
  #[error("failed to bind the embedded media loopback service: {0}")]
  LoopbackBind(#[source] std::io::Error),
  #[error("failed to resolve the application cache directory: {0}")]
  CacheDirectory(#[source] tauri::Error),
  #[error("failed to prepare the embedded HLS output directory: {0}")]
  OutputDirectory(#[source] std::io::Error),
  #[error("failed to start the FFmpeg sidecar: {0}")]
  Sidecar(#[source] tauri_plugin_shell::Error),
  #[error("failed to build the FFmpeg command: {0}")]
  FfmpegCli(#[from] jellypilot_playback_core::FfmpegCliError),
  #[error("FFmpeg exited before publishing an HLS playlist: {detail}")]
  SidecarStartupExit { detail: String, retryable: bool },
  #[error("FFmpeg did not publish an HLS playlist within 15 seconds: {detail}")]
  SidecarStartupTimeout { detail: String, retryable: bool },
  #[error("failed to force-stop FFmpeg sidecar process {pid}: {message}")]
  SidecarForceStop { pid: u32, message: String },
  #[error("FFmpeg sidecar process {pid} did not terminate within five seconds")]
  SidecarTerminationTimeout { pid: u32 },
  #[error("lost the termination observer for FFmpeg sidecar process {pid}")]
  SidecarTerminationObserver { pid: u32 },
  #[error("embedded playback transition failed: {0}")]
  Core(#[from] jellypilot_playback_core::PlaybackCoreError),
  #[error("no embedded playback source is active")]
  NoActiveSource,
  #[error("embedded playback manager is not initialized")]
  ManagerUnavailable,
  #[error("embedded playback session is stale")]
  StaleSession,
}

impl EmbeddedPlayerError {
  fn startup_retryable(&self) -> bool {
    match self {
      Self::SidecarStartupExit { retryable, .. }
      | Self::SidecarStartupTimeout { retryable, .. } => *retryable,
      Self::LoopbackBind(_)
      | Self::CacheDirectory(_)
      | Self::OutputDirectory(_)
      | Self::Sidecar(_)
      | Self::FfmpegCli(_)
      | Self::SidecarForceStop { .. }
      | Self::SidecarTerminationTimeout { .. }
      | Self::SidecarTerminationObserver { .. }
      | Self::Core(_)
      | Self::NoActiveSource
      | Self::ManagerUnavailable
      | Self::StaleSession => false,
    }
  }
}

/// Browser-visible phase of the current embedded session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EmbeddedPlayerPhase {
  Idle,
  Preparing,
  Loading,
  Playing,
  Paused,
  Buffering,
  Stopping,
  Stopped,
  Ended,
  Failed,
}

/// User-facing embedded failure and explicit fallback availability.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedPlayerFailure {
  pub message: String,
  pub retryable: bool,
  pub can_play_in_mpv: bool,
}

/// Browser-authorized media exposed for the active delivery candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EmbeddedPlayerMedia {
  /// Strict MP4 source forwarded without FFmpeg.
  DirectSource {
    url: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
  },
  /// Fragmented-MP4 HLS playlist produced by FFmpeg.
  Hls { url: String },
}

/// Complete browser read model for one embedded playback generation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedPlayerState {
  pub session_id: Option<String>,
  pub revision: u32,
  pub generation: Option<u32>,
  pub phase: EmbeddedPlayerPhase,
  pub item_id: Option<String>,
  pub title: Option<String>,
  pub subtitle: Option<String>,
  pub media: Option<EmbeddedPlayerMedia>,
  pub timeline_offset_seconds: f64,
  pub position_seconds: f64,
  pub duration_seconds: Option<f64>,
  pub desired_paused: bool,
  pub desired_muted: bool,
  pub desired_volume: u8,
  pub desired_seek_position_seconds: Option<f64>,
  pub video_codec: Option<String>,
  pub dynamic_range: Option<String>,
  pub can_play_in_mpv: bool,
  pub failure: Option<EmbeddedPlayerFailure>,
}

impl Default for EmbeddedPlayerState {
  fn default() -> Self {
    Self {
      session_id: None,
      revision: 0,
      generation: None,
      phase: EmbeddedPlayerPhase::Idle,
      item_id: None,
      title: None,
      subtitle: None,
      media: None,
      timeline_offset_seconds: 0.0,
      position_seconds: 0.0,
      duration_seconds: None,
      desired_paused: true,
      desired_muted: false,
      desired_volume: 100,
      desired_seek_position_seconds: None,
      video_codec: None,
      dynamic_range: None,
      can_play_in_mpv: false,
      failure: None,
    }
  }
}

/// Embedded-player state change pushed to the Solid owner.
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedPlayerChanged {
  pub state: EmbeddedPlayerState,
}

/// Capabilities detected in the current system WebView.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WebPlaybackCapabilities {
  pub fragmented_mp4_hls: bool,
  pub h264_sdr: bool,
  pub hevc_main10_hdr: bool,
  pub aac: bool,
  pub max_audio_channels: u8,
}

impl Default for WebPlaybackCapabilities {
  fn default() -> Self {
    Self {
      fragmented_mp4_hls: false,
      h264_sdr: false,
      hevc_main10_hdr: false,
      aac: false,
      max_audio_channels: 2,
    }
  }
}

/// Control command shared by route controls, remote-cast commands, and fallback UI.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlaybackControlCommand {
  Pause,
  Resume,
  Seek { position_seconds: f64 },
  SetVolume { volume: u8 },
  ToggleMute,
  Stop,
  Restart,
  Replay,
}

/// Media event observed by the HTML video element.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EmbeddedPlayerObservationKind {
  Ready,
  Playing,
  Paused,
  Buffering,
  Ended,
  Failed { message: String },
}

/// Session-scoped browser observation with monotonic ordering.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedPlayerObservation {
  pub session_id: String,
  pub generation: u32,
  pub sequence: u32,
  pub kind: EmbeddedPlayerObservationKind,
  pub media_time_seconds: f64,
  pub duration_seconds: Option<f64>,
  pub seekable_start_seconds: Option<f64>,
  pub seekable_end_seconds: Option<f64>,
  pub muted: bool,
  pub volume: u8,
}

/// Rust-only provider source and presentation metadata for one session.
pub(crate) struct EmbeddedPlaybackSource {
  pub item_id: String,
  pub media_source_id: Option<String>,
  pub play_session_id: Option<String>,
  pub title: String,
  pub subtitle: Option<String>,
  pub upstream_url: String,
  pub start_position_ticks: u64,
  pub duration_ticks: Option<u64>,
  pub audio_stream_index: Option<i32>,
  pub source_video_profile: SourceVideoProfile,
  // Kept in the provider handoff until its caller can be migrated; FFprobe is
  // authoritative because provider metadata can describe a different stream.
  #[allow(dead_code)]
  pub audio_layout: Option<AudioChannelLayout>,
  pub mpv_fallback_available: bool,
}

struct ActiveAdapterSession {
  session_id: String,
  source: EmbeddedPlaybackSource,
  child: Option<CommandChild>,
  child_pid: Option<u32>,
  termination: Option<tokio::sync::watch::Receiver<bool>>,
  stop_requested: bool,
  output_dir: Option<PathBuf>,
  vaapi_device: Option<PathBuf>,
  probe: Option<MediaProbeFacts>,
  timeline_offset_seconds: f64,
  seekable_start_seconds: Option<f64>,
  seekable_end_seconds: Option<f64>,
  last_progress_report: Instant,
  last_reported_paused: bool,
}

#[derive(Default)]
struct VerifiedEncoderAvailability {
  encoders: FfmpegEncoderAvailability,
  vaapi_device: Option<PathBuf>,
}

/// I/O interpreter for [`EmbeddedPlaybackCore`].
pub struct EmbeddedPlayerManager {
  app: AppHandle,
  client: Arc<JellyfinClient>,
  config: Arc<RwLock<AppConfig>>,
  core: Mutex<EmbeddedPlaybackCore>,
  capabilities: RwLock<WebPlaybackCapabilities>,
  proxy: Mutex<Option<Arc<LoopbackMediaServer>>>,
  active: Mutex<Option<ActiveAdapterSession>>,
  state: RwLock<EmbeddedPlayerState>,
  transition: tokio::sync::Mutex<()>,
}

/// Lazily installed Tauri state for the setup-dependent player manager.
#[derive(Clone, Default)]
pub struct EmbeddedPlayerManagerState(Arc<RwLock<Option<Arc<EmbeddedPlayerManager>>>>);

impl EmbeddedPlayerManagerState {
  pub fn install(&self, manager: Arc<EmbeddedPlayerManager>) {
    *self.0.write() = Some(manager);
  }

  pub fn current(&self) -> Result<Arc<EmbeddedPlayerManager>, EmbeddedPlayerError> {
    self
      .0
      .read()
      .clone()
      .ok_or(EmbeddedPlayerError::ManagerUnavailable)
  }
}

impl EmbeddedPlayerManager {
  /// Create the native adapter. Loopback and FFmpeg resources remain lazy.
  pub fn new(
    app: AppHandle,
    client: Arc<JellyfinClient>,
    config: Arc<RwLock<AppConfig>>,
  ) -> Arc<Self> {
    Arc::new(Self {
      app,
      client,
      config,
      core: Mutex::new(EmbeddedPlaybackCore::new()),
      capabilities: RwLock::new(WebPlaybackCapabilities::default()),
      proxy: Mutex::new(None),
      active: Mutex::new(None),
      state: RwLock::new(EmbeddedPlayerState::default()),
      transition: tokio::sync::Mutex::new(()),
    })
  }

  /// Return the current immutable browser read model.
  pub fn state(&self) -> EmbeddedPlayerState {
    self.state.read().clone()
  }

  /// Whether an embedded session currently owns playback commands.
  pub fn is_active(&self) -> bool {
    self.active.lock().is_some()
  }

  /// Store current WebView capabilities for the next playback plan.
  pub fn register_capabilities(&self, capabilities: WebPlaybackCapabilities) {
    *self.capabilities.write() = capabilities;
  }

  /// Start a provider source through the pure core and execute its commands.
  pub async fn play(
    self: &Arc<Self>,
    source: EmbeddedPlaybackSource,
  ) -> Result<EmbeddedPlayerState, EmbeddedPlayerError> {
    let _transition = self.transition.lock().await;
    self.stop_adapter_resources().await?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let proxy = self.ensure_proxy().await?;
    let probe_nonce = uuid::Uuid::new_v4().to_string();
    proxy.activate(
      probe_nonce.clone(),
      None,
      None,
      source.upstream_url.clone(),
      None,
    );
    let probe_source_url = proxy.source_url(&probe_nonce);
    let probe = self
      .probe_source(
        &probe_source_url,
        source.audio_stream_index,
        source.source_video_profile,
      )
      .await;
    proxy.revoke();
    let verified_encoders = verified_encoder_availability(&self.app, probe).await;
    let plan_request = FfmpegPlanRequest {
      platform: current_platform(),
      encoders: verified_encoders.encoders,
      browser: browser_capabilities(*self.capabilities.read()),
      probe,
    };
    let playback = PlaybackSession {
      item_id: source.item_id.clone(),
      media_source_id: source.media_source_id.clone(),
      play_session_id: source.play_session_id.clone(),
      start_position_ticks: source.start_position_ticks,
      duration_ticks: source.duration_ticks,
      plan_request,
      mpv_fallback_available: source.mpv_fallback_available,
    };
    *self.active.lock() = Some(ActiveAdapterSession {
      session_id,
      source,
      child: None,
      child_pid: None,
      termination: None,
      stop_requested: false,
      output_dir: None,
      vaapi_device: verified_encoders.vaapi_device,
      probe: match probe {
        MediaProbeResult::Facts(facts) => Some(facts),
        MediaProbeResult::Failed(_) => None,
      },
      timeline_offset_seconds: 0.0,
      seekable_start_seconds: None,
      seekable_end_seconds: None,
      last_progress_report: Instant::now(),
      last_reported_paused: false,
    });

    if let Some(window) = self.app.get_webview_window("main") {
      let _ = window.show();
      let _ = window.unminimize();
      let _ = window.set_focus();
    }

    let update = self.core.lock().dispatch(PlaybackAction::Play(playback))?;
    self.apply_snapshot(&update.snapshot, None);
    self.execute_commands(update.commands).await?;
    Ok(self.state())
  }

  async fn probe_source(
    &self,
    source_url: &str,
    selected_audio_stream_index: Option<i32>,
    source_profile: SourceVideoProfile,
  ) -> MediaProbeResult {
    let args = vec![
      "-v".to_string(),
      "error".to_string(),
      "-print_format".to_string(),
      "json".to_string(),
      "-show_entries".to_string(),
      "format=format_name:format_tags=major_brand:stream=index,codec_type,codec_name,profile,pix_fmt,codec_tag_string,channels,color_transfer:stream_disposition=attached_pic:stream_side_data=side_data_type".to_string(),
      source_url.to_string(),
    ];
    let output = match run_bounded_sidecar(
      &self.app,
      "ffprobe",
      args,
      FFPROBE_TIMEOUT,
      MAX_FFPROBE_OUTPUT_BYTES,
    )
    .await
    {
      Ok(output) => output,
      Err(failure) => {
        log::warn!("Embedded media probe failed: {failure:?}");
        return MediaProbeResult::Failed(failure);
      }
    };
    if output.status != Some(0) {
      let diagnostic = output.stderr.trim();
      if diagnostic.is_empty() {
        log::warn!("Embedded FFprobe exited unsuccessfully without diagnostic output");
      } else {
        log::warn!("Embedded FFprobe exited unsuccessfully: {diagnostic}");
      }
      return MediaProbeResult::Failed(MediaProbeFailure::ProcessFailed);
    }
    let document = match serde_json::from_slice::<FfprobeDocument>(&output.stdout) {
      Ok(document) => document,
      Err(_) => {
        log::warn!("Embedded FFprobe returned malformed JSON");
        return MediaProbeResult::Failed(MediaProbeFailure::InvalidOutput);
      }
    };
    match normalize_ffprobe(&document, selected_audio_stream_index, source_profile) {
      Ok(facts) => MediaProbeResult::Facts(facts),
      Err(failure) => {
        log::warn!("Embedded FFprobe facts were unusable: {failure:?}");
        MediaProbeResult::Failed(failure)
      }
    }
  }

  /// Apply a user or remote playback control.
  pub async fn control(
    self: &Arc<Self>,
    command: PlaybackControlCommand,
  ) -> Result<EmbeddedPlayerState, EmbeddedPlayerError> {
    let _transition = self.transition.lock().await;
    if matches!(&command, PlaybackControlCommand::Stop) {
      return self.stop_session().await;
    }
    match command {
      PlaybackControlCommand::SetVolume { volume } => {
        self.update_transport(|state| state.desired_volume = volume.min(100));
        return Ok(self.state());
      }
      PlaybackControlCommand::ToggleMute => {
        self.update_transport(|state| state.desired_muted = !state.desired_muted);
        return Ok(self.state());
      }
      PlaybackControlCommand::Seek { position_seconds }
        if self.seek_is_inside_window(position_seconds) =>
      {
        self.update_transport(|state| {
          state.desired_seek_position_seconds = Some(position_seconds.max(0.0));
          state.position_seconds = position_seconds.max(0.0);
        });
        return Ok(self.state());
      }
      _ => {}
    }

    let action = match command {
      PlaybackControlCommand::Pause => PlaybackAction::Pause,
      PlaybackControlCommand::Resume => PlaybackAction::Resume,
      PlaybackControlCommand::Seek { position_seconds } => PlaybackAction::Seek {
        position_ticks: seconds_to_ticks(position_seconds.max(0.0)) as u64,
      },
      PlaybackControlCommand::Stop => unreachable!("stop is handled before generic controls"),
      PlaybackControlCommand::Restart => PlaybackAction::Restart,
      PlaybackControlCommand::Replay => PlaybackAction::Replay,
      PlaybackControlCommand::SetVolume { .. } | PlaybackControlCommand::ToggleMute => {
        return Ok(self.state());
      }
    };
    let update = self.core.lock().dispatch(action)?;
    self.apply_snapshot(&update.snapshot, None);
    self.execute_commands(update.commands).await?;
    Ok(self.state())
  }

  async fn stop_session(self: &Arc<Self>) -> Result<EmbeddedPlayerState, EmbeddedPlayerError> {
    let snapshot = self.core.lock().snapshot();
    if matches!(
      snapshot.phase,
      PlaybackPhase::Idle | PlaybackPhase::Stopped | PlaybackPhase::Ended | PlaybackPhase::Failed
    ) {
      self.stop_adapter_resources().await?;
      let mut state = EmbeddedPlayerState::default();
      state.phase = EmbeddedPlayerPhase::Stopped;
      state.revision = self.state.read().revision.wrapping_add(1);
      *self.state.write() = state;
      self.emit();
      return Ok(self.state());
    }

    let update = self.core.lock().dispatch(PlaybackAction::Stop)?;
    self.apply_snapshot(&update.snapshot, None);
    self.execute_commands(update.commands).await?;

    let snapshot = self.core.lock().snapshot();
    if let Some(generation) = snapshot.generation {
      let sequence = snapshot
        .last_observation_sequence
        .unwrap_or(0)
        .saturating_add(1);
      let update =
        self
          .core
          .lock()
          .dispatch(PlaybackAction::BrowserObserved(BrowserObservation {
            token: PlaybackObservationToken {
              generation,
              sequence,
            },
            state: BrowserPlaybackState::Stopped,
            position_ticks: snapshot.position_ticks,
          }))?;
      self.apply_snapshot(&update.snapshot, None);
      self.execute_terminal_commands(update.commands).await?;
    }
    self.stop_adapter_resources().await?;
    Ok(self.state())
  }

  /// Apply one monotonic media observation and any resulting report commands.
  pub async fn observe(
    self: &Arc<Self>,
    observation: EmbeddedPlayerObservation,
  ) -> Result<EmbeddedPlayerState, EmbeddedPlayerError> {
    let _transition = self.transition.lock().await;
    let ended = matches!(&observation.kind, EmbeddedPlayerObservationKind::Ended);
    let timeline_offset = {
      let active = self.active.lock();
      let session = active.as_ref().ok_or(EmbeddedPlayerError::NoActiveSource)?;
      if session.session_id != observation.session_id {
        return Err(EmbeddedPlayerError::StaleSession);
      }
      if self.state.read().generation != Some(observation.generation) {
        return Ok(self.state());
      }
      session.timeline_offset_seconds
    };
    if matches!(observation.kind, EmbeddedPlayerObservationKind::Ready) {
      self.apply_observed_transport(&observation, timeline_offset);
      return Ok(self.state());
    }

    let browser_state = match &observation.kind {
      EmbeddedPlayerObservationKind::Playing => BrowserPlaybackState::Playing,
      EmbeddedPlayerObservationKind::Paused => BrowserPlaybackState::Paused,
      EmbeddedPlayerObservationKind::Buffering => BrowserPlaybackState::Buffering,
      EmbeddedPlayerObservationKind::Ended => BrowserPlaybackState::Ended,
      EmbeddedPlayerObservationKind::Failed { message } => BrowserPlaybackState::Failed {
        message: message.clone(),
      },
      EmbeddedPlayerObservationKind::Ready => return Ok(self.state()),
    };
    let absolute_position = timeline_offset + observation.media_time_seconds.max(0.0);
    let update =
      self
        .core
        .lock()
        .dispatch(PlaybackAction::BrowserObserved(BrowserObservation {
          token: PlaybackObservationToken {
            generation: PlaybackGeneration(u64::from(observation.generation)),
            sequence: u64::from(observation.sequence),
          },
          state: browser_state,
          position_ticks: seconds_to_ticks(absolute_position) as u64,
        }))?;
    if update.observation != PlaybackObservationDisposition::Applied {
      return Ok(self.state());
    }
    self.apply_observed_transport(&observation, timeline_offset);
    self.apply_snapshot(&update.snapshot, Some(absolute_position));
    self.execute_commands(update.commands).await?;
    if ended {
      self.stop_pipeline().await?;
    }
    Ok(self.state())
  }

  fn apply_observed_transport(
    &self,
    observation: &EmbeddedPlayerObservation,
    timeline_offset: f64,
  ) {
    {
      let mut active = self.active.lock();
      if let Some(active) = active.as_mut() {
        active.seekable_start_seconds = observation
          .seekable_start_seconds
          .map(|value| value + timeline_offset);
        active.seekable_end_seconds = observation
          .seekable_end_seconds
          .map(|value| value + timeline_offset);
      }
    }
    self.update_transport(|state| {
      state.desired_muted = observation.muted;
      state.desired_volume = observation.volume.min(100);
      if let Some(duration) = observation
        .duration_seconds
        .filter(|value| value.is_finite())
      {
        state.duration_seconds = Some(duration.max(0.0));
      }
    });
  }

  fn execute_commands(
    self: &Arc<Self>,
    commands: Vec<PlaybackCommand>,
  ) -> BoxFuture<'_, Result<(), EmbeddedPlayerError>> {
    Box::pin(async move {
      let mut commands = VecDeque::from(commands);
      while let Some(command) = commands.pop_front() {
        match command {
          PlaybackCommand::StartEmbedded { attempt } => {
            if let Err(error) = self.start_attempt(&attempt).await {
              let retryable = error.startup_retryable();
              let update = self.core.lock().dispatch(PlaybackAction::StartupFailed {
                generation: attempt.generation,
                message: error.to_string(),
                retryable,
              })?;
              self.apply_snapshot(&update.snapshot, None);
              for command in update.commands.into_iter().rev() {
                commands.push_front(command);
              }
            }
          }
          PlaybackCommand::SetPaused { paused, .. } => {
            self.update_transport(|state| state.desired_paused = paused);
          }
          PlaybackCommand::StopEmbedded { .. } => self.stop_pipeline().await?,
          PlaybackCommand::ReportStarted { report } => self.report_started(&report).await,
          PlaybackCommand::ReportProgress { report } => self.report_progress(&report).await,
          PlaybackCommand::ReportStopped { report, .. } => self.report_stopped(&report).await,
        }
      }
      Ok(())
    })
  }

  async fn start_attempt(
    self: &Arc<Self>,
    attempt: &jellypilot_playback_core::PlaybackAttempt,
  ) -> Result<(), EmbeddedPlayerError> {
    self.stop_pipeline().await?;
    let proxy = self.ensure_proxy().await?;
    let source_nonce = uuid::Uuid::new_v4().to_string();
    let (upstream_url, vaapi_device) = {
      let active = self.active.lock();
      let active = active.as_ref().ok_or(EmbeddedPlayerError::NoActiveSource)?;
      (
        active.source.upstream_url.clone(),
        active.vaapi_device.clone(),
      )
    };

    if attempt.candidate.delivery == MediaDelivery::DirectSource {
      let direct_media_nonce = uuid::Uuid::new_v4().to_string();
      proxy.activate(
        source_nonce,
        Some(direct_media_nonce.clone()),
        None,
        upstream_url,
        None,
      );
      {
        let mut active = self.active.lock();
        let active = active.as_mut().ok_or(EmbeddedPlayerError::NoActiveSource)?;
        active.output_dir = None;
        active.timeline_offset_seconds = 0.0;
      }
      let start_position_seconds = attempt.start_position_ticks as f64 / 10_000_000.0;
      self.update_transport(|state| {
        state.generation = Some(saturating_u32(attempt.generation.0));
        state.phase = EmbeddedPlayerPhase::Loading;
        state.media = Some(EmbeddedPlayerMedia::DirectSource {
          url: proxy.direct_media_url(&direct_media_nonce),
          mime_type: "video/mp4".to_string(),
        });
        state.timeline_offset_seconds = 0.0;
        state.position_seconds = start_position_seconds;
        state.desired_paused = attempt.paused;
        state.desired_seek_position_seconds = Some(start_position_seconds);
      });
      return Ok(());
    }

    let hls_nonce = uuid::Uuid::new_v4().to_string();
    let output_dir = self
      .app
      .path()
      .app_cache_dir()
      .map_err(EmbeddedPlayerError::CacheDirectory)?
      .join("embedded-playback")
      .join(format!("{}-{}", uuid::Uuid::new_v4(), attempt.generation.0));
    tokio::fs::create_dir_all(&output_dir)
      .await
      .map_err(EmbeddedPlayerError::OutputDirectory)?;
    {
      let mut active = self.active.lock();
      let active = active.as_mut().ok_or(EmbeddedPlayerError::NoActiveSource)?;
      active.output_dir = Some(output_dir.clone());
      active.timeline_offset_seconds = attempt.start_position_ticks as f64 / 10_000_000.0;
    }
    proxy.activate(
      source_nonce.clone(),
      None,
      Some(hls_nonce.clone()),
      upstream_url,
      Some(output_dir.clone()),
    );
    let source_url = proxy.source_url(&source_nonce);
    let args = ffmpeg_argv(&FfmpegCliRequest {
      source_url: &source_url,
      output_dir: &output_dir,
      start_position_seconds: attempt.start_position_ticks as f64 / 10_000_000.0,
      vaapi_device: vaapi_device.as_deref(),
      candidate: attempt.candidate,
      plan: &attempt.plan,
    })?;
    let (mut events, child) = self
      .app
      .shell()
      .sidecar("ffmpeg")
      .map_err(EmbeddedPlayerError::Sidecar)?
      .args(args)
      .spawn()
      .map_err(EmbeddedPlayerError::Sidecar)?;
    let pid = child.pid();
    let (termination_tx, termination_rx) = tokio::sync::watch::channel(false);
    {
      let mut active = self.active.lock();
      let active = active.as_mut().ok_or(EmbeddedPlayerError::NoActiveSource)?;
      active.child = Some(child);
      active.child_pid = Some(pid);
      active.termination = Some(termination_rx);
      active.stop_requested = false;
    }
    let terminated = Arc::new(AtomicBool::new(false));
    let terminated_for_task = Arc::clone(&terminated);
    let diagnostics = Arc::new(Mutex::new(FfmpegStartupDiagnostics::default()));
    let diagnostics_for_task = Arc::clone(&diagnostics);
    let manager = Arc::downgrade(self);
    let generation = attempt.generation;
    tauri::async_runtime::spawn(async move {
      while let Some(event) = events.recv().await {
        match event {
          CommandEvent::Stderr(bytes) => {
            let line = String::from_utf8_lossy(&bytes);
            diagnostics_for_task.lock().push(&line);
          }
          CommandEvent::Error(message) => diagnostics_for_task.lock().push(&message),
          CommandEvent::Terminated(payload) => {
            diagnostics_for_task.lock().record_exit(payload.code);
            terminated_for_task.store(true, Ordering::Release);
            let _ = termination_tx.send(true);
            if let Some(manager) = manager.upgrade() {
              let failure = diagnostics_for_task.lock().output.clone();
              manager
                .handle_process_terminated(generation, pid, payload.code, failure)
                .await;
            }
            break;
          }
          _ => {}
        }
      }
    });

    let playlist_path = output_dir.join("master.m3u8");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
      if tokio::fs::metadata(&playlist_path)
        .await
        .is_ok_and(|metadata| metadata.len() > 0)
      {
        break;
      }
      let exited = terminated.load(Ordering::Acquire);
      let deadline_reached = tokio::time::Instant::now() >= deadline;
      if exited || deadline_reached {
        self.stop_pipeline().await?;
        let diagnostics = diagnostics.lock();
        let error = startup_error(
          if exited {
            FfmpegStartupFailureKind::Exited
          } else {
            FfmpegStartupFailureKind::Deadline
          },
          &diagnostics,
          &proxy.source_diagnostic(),
        );
        log::warn!("Embedded FFmpeg startup failed: {error}");
        return Err(error);
      }
      tokio::time::sleep(Duration::from_millis(100)).await;
    }

    self.update_transport(|state| {
      state.generation = Some(saturating_u32(attempt.generation.0));
      state.phase = EmbeddedPlayerPhase::Loading;
      state.media = Some(EmbeddedPlayerMedia::Hls {
        url: proxy.playlist_url(&hls_nonce),
      });
      state.timeline_offset_seconds = attempt.start_position_ticks as f64 / 10_000_000.0;
      state.position_seconds = state.timeline_offset_seconds;
      state.desired_paused = attempt.paused;
      state.desired_seek_position_seconds = None;
    });
    Ok(())
  }

  async fn handle_process_terminated(
    self: Arc<Self>,
    generation: PlaybackGeneration,
    pid: u32,
    code: Option<i32>,
    failure: String,
  ) {
    let _transition = self.transition.lock().await;
    let stop_requested = {
      let active = self.active.lock();
      let Some(active) = active
        .as_ref()
        .filter(|active| active.child_pid == Some(pid))
      else {
        return;
      };
      active.stop_requested
    };
    if stop_requested {
      let output_dir = self.finish_observed_pipeline(pid, true);
      self.cleanup_pipeline_output(output_dir).await;
      return;
    }
    if !ffmpeg_exit_failed(code) {
      self.finish_observed_pipeline(pid, false);
      return;
    }
    let output_dir = self.finish_observed_pipeline(pid, true);
    self.cleanup_pipeline_output(output_dir).await;
    let snapshot = self.core.lock().snapshot();
    let sequence = snapshot
      .last_observation_sequence
      .unwrap_or(0)
      .saturating_add(1);
    if !failure.trim().is_empty() {
      log::warn!(
        "FFmpeg exited unexpectedly with status {code:?} and {} bytes of diagnostic output",
        failure.len()
      );
    }
    let message = format!("Local FFmpeg transcode exited unexpectedly with status {code:?}");
    let update = if snapshot.generation_has_played {
      self
        .core
        .lock()
        .dispatch(PlaybackAction::BrowserObserved(BrowserObservation {
          token: PlaybackObservationToken {
            generation,
            sequence,
          },
          state: BrowserPlaybackState::Failed { message },
          position_ticks: snapshot.position_ticks,
        }))
    } else {
      let source_diagnostic = self.proxy.lock().as_ref().map_or(
        SourceProxySnapshot {
          summary: String::new(),
          terminal_failure: false,
        },
        |proxy| proxy.source_diagnostic(),
      );
      self.core.lock().dispatch(PlaybackAction::StartupFailed {
        generation,
        message,
        retryable: !source_diagnostic.terminal_failure,
      })
    };
    if let Ok(update) = update {
      self.apply_snapshot(&update.snapshot, None);
      if let Err(error) = self.execute_commands(update.commands).await {
        log::warn!("Failed to clean up terminated FFmpeg pipeline: {error}");
      }
    }
  }

  async fn execute_terminal_commands(
    &self,
    commands: Vec<PlaybackCommand>,
  ) -> Result<(), EmbeddedPlayerError> {
    for command in commands {
      match command {
        PlaybackCommand::StartEmbedded { .. } => {
          log::error!("playback core requested an unexpected restart after FFmpeg terminated");
        }
        PlaybackCommand::SetPaused { paused, .. } => {
          self.update_transport(|state| state.desired_paused = paused);
        }
        PlaybackCommand::StopEmbedded { .. } => self.stop_pipeline().await?,
        PlaybackCommand::ReportStarted { report } => self.report_started(&report).await,
        PlaybackCommand::ReportProgress { report } => self.report_progress(&report).await,
        PlaybackCommand::ReportStopped { report, .. } => self.report_stopped(&report).await,
      }
    }
    Ok(())
  }

  fn apply_snapshot(&self, snapshot: &PlaybackSnapshot, position_seconds: Option<f64>) {
    let active = self.active.lock();
    let mut state = self.state.write();
    state.revision = state.revision.wrapping_add(1);
    state.generation = snapshot
      .generation
      .map(|generation| saturating_u32(generation.0));
    state.phase = phase(snapshot.phase);
    if matches!(
      state.phase,
      EmbeddedPlayerPhase::Idle
        | EmbeddedPlayerPhase::Preparing
        | EmbeddedPlayerPhase::Stopping
        | EmbeddedPlayerPhase::Stopped
        | EmbeddedPlayerPhase::Ended
        | EmbeddedPlayerPhase::Failed
    ) {
      state.media = None;
    }
    state.item_id = snapshot
      .session
      .as_ref()
      .map(|session| session.item_id.clone());
    state.position_seconds = position_seconds
      .unwrap_or(snapshot.position_ticks as f64 / 10_000_000.0)
      .max(0.0);
    state.duration_seconds = snapshot
      .duration_ticks
      .map(|ticks| ticks as f64 / 10_000_000.0);
    state.desired_paused = snapshot.paused;
    if let Some(active) = active.as_ref() {
      state.session_id = Some(active.session_id.clone());
      state.title = Some(active.source.title.clone());
      state.subtitle = active.source.subtitle.clone();
      state.timeline_offset_seconds = active.timeline_offset_seconds;
      state.video_codec = active.probe.map(|probe| match probe.video.codec {
        ProbedVideoCodec::H264 => "h264".to_string(),
        ProbedVideoCodec::Hevc if probe.video.hevc_main10 => "hevc-main10".to_string(),
        ProbedVideoCodec::Hevc => "hevc".to_string(),
        ProbedVideoCodec::Other => "other".to_string(),
      });
      state.dynamic_range = active.probe.map(|probe| match probe.video.dynamic_range {
        ProbedDynamicRange::Sdr => "sdr".to_string(),
        ProbedDynamicRange::Hdr => "hdr".to_string(),
      });
    }
    state.failure = snapshot
      .failure
      .as_ref()
      .map(|failure| EmbeddedPlayerFailure {
        message: failure.message.clone(),
        retryable: failure.retryable,
        can_play_in_mpv: snapshot
          .mpv_fallback
          .as_ref()
          .is_some_and(|fallback| fallback.available),
      });
    state.can_play_in_mpv = snapshot
      .mpv_fallback
      .as_ref()
      .is_some_and(|fallback| fallback.available)
      || active
        .as_ref()
        .is_some_and(|active| active.source.mpv_fallback_available);
    drop(state);
    drop(active);
    self.emit();
  }

  fn update_transport(&self, update: impl FnOnce(&mut EmbeddedPlayerState)) {
    {
      let mut state = self.state.write();
      update(&mut state);
      state.revision = state.revision.wrapping_add(1);
    }
    self.emit();
  }

  fn emit(&self) {
    if let Err(error) = (EmbeddedPlayerChanged {
      state: self.state(),
    })
    .emit(&self.app)
    {
      log::error!("Failed to emit embedded player state: {error}");
    }
  }

  fn seek_is_inside_window(&self, position_seconds: f64) -> bool {
    self.active.lock().as_ref().is_some_and(|active| {
      matches!(
        (active.seekable_start_seconds, active.seekable_end_seconds),
        (Some(start), Some(end)) if position_seconds >= start && position_seconds <= end
      )
    })
  }

  async fn ensure_proxy(&self) -> Result<Arc<LoopbackMediaServer>, EmbeddedPlayerError> {
    if let Some(proxy) = self.proxy.lock().clone() {
      return Ok(proxy);
    }
    let proxy = Arc::new(LoopbackMediaServer::start().await?);
    *self.proxy.lock() = Some(Arc::clone(&proxy));
    Ok(proxy)
  }

  async fn stop_pipeline(&self) -> Result<(), EmbeddedPlayerError> {
    let (child, pid, mut termination, output_without_process) = {
      let mut active = self.active.lock();
      let Some(active) = active.as_mut() else {
        return Ok(());
      };
      if active.child_pid.is_none() {
        (None, None, None, active.output_dir.take())
      } else {
        active.stop_requested = true;
        (
          active.child.take(),
          active.child_pid,
          active.termination.clone(),
          None,
        )
      }
    };
    if let Some(proxy) = self.proxy.lock().as_ref() {
      proxy.revoke();
    }
    if let Some(output_dir) = output_without_process {
      self.cleanup_pipeline_output(Some(output_dir)).await;
      return Ok(());
    }
    let Some(pid) = pid else {
      return Ok(());
    };
    let primary_kill_failed = child.and_then(|child| match child.kill() {
      Ok(()) => None,
      Err(source) => {
        log::warn!("Primary FFmpeg stop failed for process {pid}: {source}");
        Some(source)
      }
    });
    let Some(termination) = termination.as_mut() else {
      if let Err(message) = force_terminate_process(pid).await {
        return Err(EmbeddedPlayerError::SidecarForceStop { pid, message });
      }
      return Err(EmbeddedPlayerError::SidecarTerminationObserver { pid });
    };
    let mut force_error = None;
    if primary_kill_failed.is_some() {
      force_error = force_terminate_process(pid).await.err();
    }
    if !wait_for_termination(termination, pid).await? {
      force_error = force_terminate_process(pid).await.err().or(force_error);
      if !wait_for_termination(termination, pid).await? {
        if let Some(message) = force_error {
          return Err(EmbeddedPlayerError::SidecarForceStop { pid, message });
        }
        return Err(EmbeddedPlayerError::SidecarTerminationTimeout { pid });
      }
    }
    let output_dir = self.finish_observed_pipeline(pid, true);
    self.cleanup_pipeline_output(output_dir).await;
    Ok(())
  }

  fn finish_observed_pipeline(&self, pid: u32, remove_output: bool) -> Option<PathBuf> {
    let mut active = self.active.lock();
    let active = active
      .as_mut()
      .filter(|active| active.child_pid == Some(pid))?;
    active.child = None;
    active.child_pid = None;
    active.termination = None;
    active.stop_requested = false;
    remove_output.then(|| active.output_dir.take()).flatten()
  }

  async fn cleanup_pipeline_output(&self, output_dir: Option<PathBuf>) {
    let Some(output_dir) = output_dir else {
      return;
    };
    if let Some(proxy) = self.proxy.lock().as_ref() {
      proxy.revoke();
    }
    if let Err(error) = tokio::fs::remove_dir_all(&output_dir).await {
      if error.kind() != std::io::ErrorKind::NotFound {
        log::warn!(
          "Failed to remove embedded HLS output {}: {error}",
          output_dir.display()
        );
      }
    }
  }

  async fn stop_adapter_resources(&self) -> Result<(), EmbeddedPlayerError> {
    self.stop_pipeline().await?;
    self.active.lock().take();
    Ok(())
  }

  async fn report_started(&self, report: &PlaybackReport) {
    let Some(info) = self.report_start_info(report) else {
      return;
    };
    if let Err(error) = self.client.playback().report_playback_start(&info).await {
      log::warn!("Failed to report embedded playback start: {error}");
    }
  }

  async fn report_progress(&self, report: &PlaybackReport) {
    let (muted, volume, audio_stream_index) = {
      let mut active = self.active.lock();
      let Some(active) = active.as_mut() else {
        return;
      };
      let interval = Duration::from_secs(self.config.read().progress_interval.into());
      let changed = active.last_reported_paused != report.paused;
      if !changed && active.last_progress_report.elapsed() < interval {
        return;
      }
      active.last_progress_report = Instant::now();
      active.last_reported_paused = report.paused;
      let state = self.state.read();
      (
        state.desired_muted,
        i32::from(state.desired_volume),
        active.source.audio_stream_index,
      )
    };
    let info = PlaybackProgressInfo {
      item_id: report.session.item_id.clone(),
      media_source_id: report.session.media_source_id.clone(),
      play_session_id: report.session.play_session_id.clone(),
      position_ticks: Some(report.position_ticks as i64),
      is_paused: report.paused,
      is_muted: muted,
      volume_level: volume,
      audio_stream_index,
      subtitle_stream_index: Some(-1),
      play_method: "DirectPlay".to_string(),
      can_seek: true,
    };
    if let Err(error) = self.client.playback().report_playback_progress(&info).await {
      log::warn!("Failed to report embedded playback progress: {error}");
    }
  }

  async fn report_stopped(&self, report: &PlaybackReport) {
    let info = PlaybackStopInfo {
      item_id: report.session.item_id.clone(),
      media_source_id: report.session.media_source_id.clone(),
      play_session_id: report.session.play_session_id.clone(),
      position_ticks: Some(report.position_ticks as i64),
    };
    if let Err(error) = self.client.playback().report_playback_stop(&info).await {
      log::warn!("Failed to report embedded playback stop: {error}");
    }
  }

  fn report_start_info(&self, report: &PlaybackReport) -> Option<PlaybackStartInfo> {
    let active = self.active.lock();
    let active = active.as_ref()?;
    let state = self.state.read();
    Some(PlaybackStartInfo {
      item_id: report.session.item_id.clone(),
      media_source_id: report.session.media_source_id.clone(),
      play_session_id: report.session.play_session_id.clone(),
      position_ticks: Some(report.position_ticks as i64),
      is_paused: report.paused,
      is_muted: state.desired_muted,
      volume_level: i32::from(state.desired_volume),
      audio_stream_index: active.source.audio_stream_index,
      subtitle_stream_index: Some(-1),
      play_method: "DirectPlay".to_string(),
      can_seek: true,
    })
  }

  /// Resume metadata for an explicit MPV fallback action.
  pub fn mpv_fallback_request(&self) -> Option<(String, f64)> {
    let snapshot = self.core.lock().snapshot();
    snapshot
      .mpv_fallback
      .filter(|fallback| fallback.available)
      .map(|fallback| {
        (
          fallback.item_id,
          fallback.resume_position_ticks as f64 / 10_000_000.0,
        )
      })
      .or_else(|| {
        let active = self.active.lock();
        let active = active.as_ref()?;
        active.source.mpv_fallback_available.then(|| {
          (
            active.source.item_id.clone(),
            self.state.read().position_seconds,
          )
        })
      })
  }
}

fn phase(phase: PlaybackPhase) -> EmbeddedPlayerPhase {
  match phase {
    PlaybackPhase::Idle => EmbeddedPlayerPhase::Idle,
    PlaybackPhase::Starting | PlaybackPhase::Seeking | PlaybackPhase::Restarting => {
      EmbeddedPlayerPhase::Preparing
    }
    PlaybackPhase::Playing => EmbeddedPlayerPhase::Playing,
    PlaybackPhase::Paused => EmbeddedPlayerPhase::Paused,
    PlaybackPhase::Buffering => EmbeddedPlayerPhase::Buffering,
    PlaybackPhase::Stopping => EmbeddedPlayerPhase::Stopping,
    PlaybackPhase::Stopped => EmbeddedPlayerPhase::Stopped,
    PlaybackPhase::Ended => EmbeddedPlayerPhase::Ended,
    PlaybackPhase::Failed => EmbeddedPlayerPhase::Failed,
  }
}

fn current_platform() -> FfmpegPlatform {
  #[cfg(target_os = "macos")]
  return FfmpegPlatform::MacOs;
  #[cfg(target_os = "windows")]
  return FfmpegPlatform::Windows;
  #[cfg(target_os = "linux")]
  return FfmpegPlatform::Linux;
}

async fn verified_encoder_availability(
  app: &AppHandle,
  probe: MediaProbeResult,
) -> VerifiedEncoderAvailability {
  #[cfg(target_os = "linux")]
  {
    let MediaProbeResult::Facts(facts) = probe else {
      return VerifiedEncoderAvailability::default();
    };
    if facts.video.dynamic_range == ProbedDynamicRange::Hdr {
      return VerifiedEncoderAvailability::default();
    }
    for render_node in discover_amd_render_nodes().await {
      if smoke_h264_vaapi(app, &render_node).await {
        return VerifiedEncoderAvailability {
          encoders: FfmpegEncoderAvailability {
            vaapi: true,
            ..FfmpegEncoderAvailability::default()
          },
          vaapi_device: Some(render_node),
        };
      }
    }
    VerifiedEncoderAvailability::default()
  }
  #[cfg(not(target_os = "linux"))]
  {
    let _ = (app, probe);
    VerifiedEncoderAvailability::default()
  }
}

#[cfg(target_os = "linux")]
async fn discover_amd_render_nodes() -> Vec<PathBuf> {
  let mut nodes = Vec::new();
  let Ok(mut entries) = tokio::fs::read_dir("/sys/class/drm").await else {
    return nodes;
  };
  while let Ok(Some(entry)) = entries.next_entry().await {
    let name = entry.file_name();
    let Some(name) = name.to_str() else {
      continue;
    };
    if !name
      .strip_prefix("renderD")
      .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
    {
      continue;
    }
    let Ok(vendor) = tokio::fs::read_to_string(entry.path().join("device/vendor")).await else {
      continue;
    };
    if !vendor.trim().eq_ignore_ascii_case("0x1002") {
      continue;
    }
    let device = Path::new("/dev/dri").join(name);
    if tokio::fs::metadata(&device).await.is_ok() {
      nodes.push(device);
    }
  }
  nodes.sort();
  nodes
}

#[cfg(target_os = "linux")]
async fn smoke_h264_vaapi(app: &AppHandle, render_node: &Path) -> bool {
  let args = vec![
    "-nostdin".to_string(),
    "-hide_banner".to_string(),
    "-loglevel".to_string(),
    "error".to_string(),
    "-vaapi_device".to_string(),
    render_node.to_string_lossy().into_owned(),
    "-f".to_string(),
    "lavfi".to_string(),
    "-i".to_string(),
    "color=c=black:s=64x64:r=1:d=0.1".to_string(),
    "-vf".to_string(),
    "format=nv12,hwupload".to_string(),
    "-frames:v".to_string(),
    "1".to_string(),
    "-c:v".to_string(),
    "h264_vaapi".to_string(),
    "-f".to_string(),
    "null".to_string(),
    "-".to_string(),
  ];
  match run_bounded_sidecar(app, "ffmpeg", args, Duration::from_secs(5), 1024).await {
    Ok(output) if output.status == Some(0) => true,
    Ok(output) => {
      if !output.stderr.trim().is_empty() {
        log::debug!(
          "VAAPI smoke failed for {}: {}",
          render_node.display(),
          output.stderr.trim()
        );
      }
      false
    }
    Err(failure) => {
      log::debug!(
        "VAAPI smoke unavailable for {}: {failure:?}",
        render_node.display()
      );
      false
    }
  }
}

fn browser_capabilities(capabilities: WebPlaybackCapabilities) -> BrowserPlaybackCapabilities {
  BrowserPlaybackCapabilities {
    fmp4_hls: capabilities.fragmented_mp4_hls,
    h264_sdr: capabilities.h264_sdr,
    hevc_main10_hdr: capabilities.hevc_main10_hdr,
    aac: capabilities.aac,
    max_audio_channels: capabilities.max_audio_channels,
  }
}

fn saturating_u32(value: u64) -> u32 {
  u32::try_from(value).unwrap_or(u32::MAX)
}

fn ffmpeg_exit_failed(code: Option<i32>) -> bool {
  code != Some(0)
}

async fn wait_for_termination(
  termination: &mut tokio::sync::watch::Receiver<bool>,
  pid: u32,
) -> Result<bool, EmbeddedPlayerError> {
  if *termination.borrow() {
    return Ok(true);
  }
  match tokio::time::timeout(Duration::from_secs(5), termination.changed()).await {
    Ok(Ok(())) => Ok(*termination.borrow()),
    Ok(Err(_)) => Err(EmbeddedPlayerError::SidecarTerminationObserver { pid }),
    Err(_) => Ok(false),
  }
}

fn force_termination_command(pid: u32) -> (&'static str, Vec<String>) {
  #[cfg(unix)]
  {
    ("kill", vec!["-KILL".to_string(), pid.to_string()])
  }
  #[cfg(windows)]
  {
    (
      "taskkill",
      vec![
        "/PID".to_string(),
        pid.to_string(),
        "/T".to_string(),
        "/F".to_string(),
      ],
    )
  }
}

async fn force_terminate_process(pid: u32) -> Result<(), String> {
  let (program, args) = force_termination_command(pid);
  let status = tokio::process::Command::new(program)
    .args(args)
    .status()
    .await
    .map_err(|error| format!("could not run {program}: {error}"))?;
  status
    .success()
    .then_some(())
    .ok_or_else(|| format!("{program} exited with {status}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn ffprobe_stream(
    index: u32,
    codec_type: &str,
    codec_name: &str,
    channels: Option<u32>,
  ) -> FfprobeStream {
    FfprobeStream {
      index: Some(index),
      codec_type: Some(codec_type.to_string()),
      codec_name: Some(codec_name.to_string()),
      profile: Some(
        if codec_type == "audio" && codec_name == "aac" {
          "LC"
        } else {
          "High"
        }
        .to_string(),
      ),
      pix_fmt: Some("yuv420p".to_string()),
      codec_tag_string: Some("avc1".to_string()),
      channels,
      color_transfer: None,
      disposition: Some(FfprobeDisposition { attached_pic: 0 }),
      side_data_list: Vec::new(),
    }
  }

  #[test]
  fn browser_capability_mapping_preserves_detected_limits() {
    let mapped = browser_capabilities(WebPlaybackCapabilities {
      fragmented_mp4_hls: true,
      h264_sdr: true,
      hevc_main10_hdr: false,
      aac: true,
      max_audio_channels: 6,
    });

    assert!(mapped.fmp4_hls);
    assert!(!mapped.hevc_main10_hdr);
    assert_eq!(mapped.max_audio_channels, 6);
  }

  #[test]
  fn successful_ffmpeg_eof_is_not_a_runtime_failure() {
    assert!(!ffmpeg_exit_failed(Some(0)));
    assert!(ffmpeg_exit_failed(Some(1)));
    assert!(ffmpeg_exit_failed(None));
  }

  #[test]
  fn force_termination_command_targets_the_owned_pid() {
    let (program, args) = force_termination_command(4242);

    assert!(
      !program.is_empty() && args.iter().any(|argument| argument == "4242"),
      "force command must target PID 4242: {program} {args:?}"
    );
  }

  #[test]
  fn ffmpeg_startup_diagnostic_redacts_urls_and_keeps_the_bounded_tail() {
    let mut diagnostic = FfmpegStartupDiagnostics::default();
    diagnostic.push(&format!(
      "{} https://provider.invalid/video?api_key=secret\nfinal decoder error",
      "x".repeat(MAX_FFMPEG_STARTUP_DIAGNOSTIC_BYTES + 32)
    ));

    assert!(
      diagnostic.output.len() <= MAX_FFMPEG_STARTUP_DIAGNOSTIC_BYTES
        && diagnostic.output.contains("[REDACTED_URL]")
        && diagnostic.output.ends_with("final decoder error")
        && !diagnostic.output.contains("secret")
    );
  }

  #[test]
  fn startup_timeout_reports_proxy_stage_and_sanitized_ffmpeg_output() {
    let mut diagnostic = FfmpegStartupDiagnostics::default();
    diagnostic.push("decoder initialization failed");

    let error = startup_error(
      FfmpegStartupFailureKind::Deadline,
      &diagnostic,
      &SourceProxySnapshot {
        summary: "source proxy returned HTTP 206 after 12 ms".to_string(),
        terminal_failure: false,
      },
    );

    assert_eq!(
      error.to_string(),
      "FFmpeg did not publish an HLS playlist within 15 seconds: source proxy returned HTTP 206 after 12 ms; FFmpeg was still running at the startup deadline; FFmpeg: decoder initialization failed"
    );
    assert!(error.startup_retryable());
  }

  #[test]
  fn ffprobe_normalization_selects_the_requested_global_audio_stream() {
    let document = FfprobeDocument {
      streams: vec![
        ffprobe_stream(2, "video", "h264", None),
        ffprobe_stream(3, "audio", "aac", Some(2)),
        ffprobe_stream(7, "audio", "ac3", Some(6)),
      ],
      format: Some(FfprobeFormat {
        format_name: "mov,mp4,m4a,3gp,3g2,mj2".to_string(),
        tags: Some(FfprobeFormatTags {
          major_brand: Some("isom".to_string()),
        }),
      }),
    };

    let facts = normalize_ffprobe(&document, Some(7), SourceVideoProfile::H264Sdr)
      .expect("probe should normalize");

    assert_eq!(
      (
        facts.container,
        facts.video.stream_index,
        facts.audio.map(|audio| (audio.stream_index, audio.codec)),
        facts.audio_stream_count,
      ),
      (
        ProbedContainer::Mp4,
        2,
        Some((7, ProbedAudioCodec::Other)),
        2,
      )
    );
  }

  #[test]
  fn ffprobe_normalization_rejects_a_missing_selected_audio_stream() {
    let document = FfprobeDocument {
      streams: vec![ffprobe_stream(0, "video", "h264", None)],
      format: None,
    };

    let error = normalize_ffprobe(&document, Some(4), SourceVideoProfile::H264Sdr)
      .expect_err("selected audio must not silently change");

    assert_eq!(error, MediaProbeFailure::SelectedAudioStreamMissing);
  }

  #[test]
  fn ffprobe_normalization_requires_aac_lc_for_browser_compatibility() {
    let mut stream = ffprobe_stream(3, "audio", "aac", Some(2));
    stream.profile = Some("HE-AAC".to_string());

    let audio = normalize_audio_stream(&stream).expect("valid audio stream should normalize");

    assert_eq!(audio.codec, ProbedAudioCodec::Other);
  }

  #[test]
  fn quicktime_brand_is_not_strict_mp4_direct_source() {
    let format = FfprobeFormat {
      format_name: "mov,mp4,m4a,3gp,3g2,mj2".to_string(),
      tags: Some(FfprobeFormatTags {
        major_brand: Some("qt  ".to_string()),
      }),
    };

    assert_eq!(normalize_container(Some(&format)), ProbedContainer::Other);
  }

  #[test]
  fn direct_source_media_serializes_to_the_locked_public_tag() {
    let media = EmbeddedPlayerMedia::DirectSource {
      url: "http://127.0.0.1:1234/media/nonce".to_string(),
      mime_type: "video/mp4".to_string(),
    };

    assert_eq!(
      serde_json::to_value(media).expect("media should serialize"),
      serde_json::json!({
        "kind": "directSource",
        "url": "http://127.0.0.1:1234/media/nonce",
        "mimeType": "video/mp4",
      })
    );
  }

  #[test]
  fn source_auth_failure_is_not_a_retryable_candidate_failure() {
    let error = EmbeddedPlayerError::SidecarStartupExit {
      detail: "source proxy returned HTTP 401 after 4 ms (range: no); FFmpeg exited".to_string(),
      retryable: false,
    };

    assert!(!error.startup_retryable());
  }

  #[test]
  fn encoder_failure_before_any_source_request_is_retryable() {
    let error = EmbeddedPlayerError::SidecarStartupExit {
      detail: "source proxy received no request; FFmpeg: device initialization failed".to_string(),
      retryable: true,
    };

    assert!(error.startup_retryable());
  }
}
