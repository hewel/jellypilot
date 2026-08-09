//! Native adapter for the UI-agnostic embedded playback core.

mod proxy;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use jellypilot_playback_core::{
  ffmpeg_argv, AudioChannelLayout, BrowserObservation, BrowserPlaybackCapabilities,
  BrowserPlaybackState, EmbeddedPlaybackCore, FfmpegCliRequest, FfmpegEncoderAvailability,
  FfmpegPlanRequest, FfmpegPlatform, PlaybackAction, PlaybackCommand, PlaybackGeneration,
  PlaybackObservationDisposition, PlaybackObservationToken, PlaybackPhase, PlaybackReport,
  PlaybackSession, PlaybackSnapshot, SourceVideoProfile,
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

use self::proxy::LoopbackMediaServer;

const MAX_FFMPEG_STARTUP_DIAGNOSTIC_BYTES: usize = 4096;

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

fn startup_error(
  kind: FfmpegStartupFailureKind,
  diagnostics: &FfmpegStartupDiagnostics,
  proxy_summary: &str,
) -> EmbeddedPlayerError {
  let detail = diagnostics.detail(kind, proxy_summary);
  match kind {
    FfmpegStartupFailureKind::Exited => EmbeddedPlayerError::SidecarStartupExit { detail },
    FfmpegStartupFailureKind::Deadline => EmbeddedPlayerError::SidecarStartupTimeout { detail },
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
  #[error("FFmpeg exited before publishing an HLS playlist: {detail}")]
  SidecarStartupExit { detail: String },
  #[error("FFmpeg did not publish an HLS playlist within 15 seconds: {detail}")]
  SidecarStartupTimeout { detail: String },
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
  pub playlist_url: Option<String>,
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
      playlist_url: None,
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
  timeline_offset_seconds: f64,
  seekable_start_seconds: Option<f64>,
  seekable_end_seconds: Option<f64>,
  last_progress_report: Instant,
  last_reported_paused: bool,
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
    let plan_request = FfmpegPlanRequest {
      platform: current_platform(),
      encoders: verified_encoder_availability(),
      browser: browser_capabilities(*self.capabilities.read()),
      video: source.source_video_profile,
      audio: source.audio_layout,
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

  async fn execute_commands(
    self: &Arc<Self>,
    commands: Vec<PlaybackCommand>,
  ) -> Result<(), EmbeddedPlayerError> {
    let mut commands = VecDeque::from(commands);
    while let Some(command) = commands.pop_front() {
      match command {
        PlaybackCommand::StartEmbedded { attempt } => {
          if let Err(error) = self.start_attempt(&attempt).await {
            let update = self.core.lock().dispatch(PlaybackAction::StartupFailed {
              generation: attempt.generation,
              message: error.to_string(),
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
  }

  async fn start_attempt(
    self: &Arc<Self>,
    attempt: &jellypilot_playback_core::PlaybackAttempt,
  ) -> Result<(), EmbeddedPlayerError> {
    self.stop_pipeline().await?;
    let proxy = self.ensure_proxy().await?;
    let source_nonce = uuid::Uuid::new_v4().to_string();
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
    let (upstream_url, audio_stream_index) = {
      let active = self.active.lock();
      let active = active.as_ref().ok_or(EmbeddedPlayerError::NoActiveSource)?;
      (
        active.source.upstream_url.clone(),
        active.source.audio_stream_index,
      )
    };
    {
      let mut active = self.active.lock();
      let active = active.as_mut().ok_or(EmbeddedPlayerError::NoActiveSource)?;
      active.output_dir = Some(output_dir.clone());
      active.timeline_offset_seconds = attempt.start_position_ticks as f64 / 10_000_000.0;
    }
    proxy.activate(
      source_nonce.clone(),
      hls_nonce.clone(),
      upstream_url,
      output_dir.clone(),
    );
    let source_url = proxy.source_url(&source_nonce);
    let args = ffmpeg_argv(&FfmpegCliRequest {
      source_url: &source_url,
      output_dir: &output_dir,
      start_position_seconds: attempt.start_position_ticks as f64 / 10_000_000.0,
      audio_stream_index,
      candidate: attempt.candidate,
      plan: &attempt.plan,
    });
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
          &proxy.source_diagnostic_summary(),
        );
        log::warn!("Embedded FFmpeg startup failed: {error}");
        return Err(error);
      }
      tokio::time::sleep(Duration::from_millis(100)).await;
    }

    self.update_transport(|state| {
      state.generation = Some(saturating_u32(attempt.generation.0));
      state.phase = EmbeddedPlayerPhase::Loading;
      state.playlist_url = Some(proxy.playlist_url(&hls_nonce));
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
    let update = {
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
    };
    if let Ok(update) = update {
      self.apply_snapshot(&update.snapshot, None);
      if let Err(error) = self.execute_terminal_commands(update.commands).await {
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
      state.video_codec = Some(match active.source.source_video_profile {
        SourceVideoProfile::H264Sdr => "h264".to_string(),
        SourceVideoProfile::HevcMain10Hdr => "hevc-main10".to_string(),
      });
      state.dynamic_range = Some(match active.source.source_video_profile {
        SourceVideoProfile::H264Sdr => "sdr".to_string(),
        SourceVideoProfile::HevcMain10Hdr => "hdr".to_string(),
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

fn verified_encoder_availability() -> FfmpegEncoderAvailability {
  // The bundled binary is guaranteed to provide the software encoders. Hardware
  // candidates are enabled only after a future runtime probe proves both codec
  // support and usable host devices.
  FfmpegEncoderAvailability::default()
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
      "source proxy returned HTTP 206 after 12 ms",
    );

    assert_eq!(
      error.to_string(),
      "FFmpeg did not publish an HLS playlist within 15 seconds: source proxy returned HTTP 206 after 12 ms; FFmpeg was still running at the startup deadline; FFmpeg: decoder initialization failed"
    );
  }
}
