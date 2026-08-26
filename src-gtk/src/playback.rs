//! Framework-independent external MPV playback for the native GTK shell.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jellypilot_media_server::{
  ticks_to_seconds, JellyfinClient, MediaItem, MediaServerProvider, MediaSource, MediaStream,
  PlaybackProgressInfo, PlaybackStartInfo, PlaybackStopInfo, VideoItemDetail, VideoLibraryItem,
};
use jellypilot_mpv::{
  collect_player_state_sample, find_mpv, has_mpv_option, MpvClient, MpvEvent, PlayerState,
};

const DIRECT_PLAYBACK_CACHE_OPTIONS: [(&str, &str); 8] = [
  ("cache", "cache=yes"),
  ("cache-on-disk", "cache-on-disk=yes"),
  ("demuxer-max-bytes", "demuxer-max-bytes=256MiB"),
  ("demuxer-max-back-bytes", "demuxer-max-back-bytes=128MiB"),
  ("demuxer-seekable-cache", "demuxer-seekable-cache=yes"),
  ("cache-pause", "cache-pause=yes"),
  ("cache-pause-initial", "cache-pause-initial=yes"),
  ("cache-pause-wait", "cache-pause-wait=3"),
];
const MEDIA_TICKS_PER_SECOND: i64 = 10_000_000;
const MPV_FILE_LOAD_TIMEOUT: Duration = Duration::from_secs(15);
const PASSIVE_PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(10);
const PLAYBACK_REPORT_TIMEOUT: Duration = Duration::from_secs(2);

/// MPV process settings used when constructing a playback controller.
#[derive(Default)]
pub struct PlaybackControllerConfig {
  mpv_path: Option<PathBuf>,
  extra_args: Vec<String>,
  demuxer_cache_dir: Option<PathBuf>,
}

impl PlaybackControllerConfig {
  /// Use an explicit MPV executable instead of PATH discovery.
  #[must_use]
  pub fn with_mpv_path(mut self, mpv_path: PathBuf) -> Self {
    self.mpv_path = Some(mpv_path);
    self
  }

  /// Pass additional process arguments to MPV.
  #[must_use]
  pub fn with_extra_args(mut self, extra_args: Vec<String>) -> Self {
    self.extra_args = extra_args;
    self
  }

  /// Put MPV's temporary demuxer cache under the application cache directory.
  #[must_use]
  pub fn with_demuxer_cache_dir(mut self, demuxer_cache_dir: PathBuf) -> Self {
    self.demuxer_cache_dir = Some(demuxer_cache_dir);
    self
  }
}

/// How a new item chooses its initial position.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PlaybackStartPosition {
  /// Start at the beginning.
  #[default]
  Beginning,
  /// Use the resume position from the Library item or detail.
  Resume,
  /// Start at an explicit number of seconds.
  At(f64),
}

/// Optional choices applied atomically with MPV's `loadfile` command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaybackOptions {
  pub start_position: PlaybackStartPosition,
  /// Provider stream index, not MPV's type-local track number.
  pub audio_stream_index: Option<i32>,
  /// Provider stream index, or `-1` to disable subtitles.
  pub subtitle_stream_index: Option<i32>,
  /// Prefer a specific source from the playback-info response.
  pub media_source_id: Option<String>,
}
/// Track metadata read from MPV's authoritative track-list property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
  pub id: i64,
  pub track_type: String,
  pub title: Option<String>,
  pub language: Option<String>,
  pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MpvSubtitleSelection {
  Track(i64),
  Value(&'static str),
}

fn mpv_subtitle_selection(id: Option<i64>) -> MpvSubtitleSelection {
  match id {
    Some(id) if id >= 0 => MpvSubtitleSelection::Track(id),
    Some(_) | None => MpvSubtitleSelection::Value("no"),
  }
}

/// Token-free metadata suitable for a Now Playing view.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlayingItem {
  pub item_id: String,
  pub title: String,
  pub item_type: String,
  pub runtime_seconds: Option<f64>,
  pub start_position_seconds: f64,
  pub play_method: String,
}

/// Current item metadata plus the authoritative MPV transport state.
#[derive(Debug, Clone)]
pub struct PlaybackSnapshot {
  pub now_playing: Option<NowPlayingItem>,
  pub transport: PlayerState,
}

/// Whether a best-effort media-server playback report succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportingStatus {
  Reported,
  Failed,
}

/// Non-fatal work that could not be completed after media started playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackWarning {
  PreviousPlaybackStopNotReported,
  PlaybackStartNotReported,
  PlaybackProgressNotReported,
  PlaybackStopNotReported,
  MediaTitleUnavailable,
  ExternalSubtitleUnavailable,
}

impl fmt::Display for PlaybackWarning {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      Self::PreviousPlaybackStopNotReported => "the previous playback stop could not be reported",
      Self::PlaybackStartNotReported => "playback start could not be reported",
      Self::PlaybackProgressNotReported => "playback progress could not be reported",
      Self::PlaybackStopNotReported => "playback stop could not be reported",
      Self::MediaTitleUnavailable => "the external player title could not be updated",
      Self::ExternalSubtitleUnavailable => "the external subtitle could not be loaded",
    })
  }
}

/// Why a refresh cleared the current playback session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackEndReason {
  /// MPV emitted an end-of-file event for the current item.
  EndOfFile,
  /// MPV could not continue playing the current item or was stopped externally.
  Error,
  /// The MPV IPC connection disappeared while an item was active.
  Disconnected,
}

/// Playback lifecycle state observed by [`PlaybackController::refresh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackRefreshState {
  Idle,
  Active,
  Ended(PlaybackEndReason),
}

/// Result returned after MPV accepted a new item.
#[must_use = "playback warnings must be surfaced to the user"]
#[derive(Debug, Clone)]
pub struct PlaybackStartOutcome {
  pub snapshot: PlaybackSnapshot,
  pub warnings: Vec<PlaybackWarning>,
}

/// Result returned by a transport control.
#[must_use = "playback warnings must be surfaced to the user"]
#[derive(Debug, Clone)]
pub struct PlaybackControlOutcome {
  pub snapshot: PlaybackSnapshot,
  pub warnings: Vec<PlaybackWarning>,
}

/// Result returned after the current item is stopped.
#[must_use = "playback warnings must be surfaced to the user"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackStopOutcome {
  pub warnings: Vec<PlaybackWarning>,
}

/// Result of reconciling MPV with the controller's active item.
#[must_use = "refresh state and playback warnings must be handled"]
#[derive(Debug, Clone)]
pub struct PlaybackRefreshOutcome {
  pub snapshot: PlaybackSnapshot,
  pub state: PlaybackRefreshState,
  pub warnings: Vec<PlaybackWarning>,
}

/// Result of gracefully disposing the playback controller's runtime state.
#[must_use = "shutdown reporting warnings must be surfaced to the user"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackShutdownOutcome {
  pub stopped_active_playback: bool,
  pub warnings: Vec<PlaybackWarning>,
}

/// Sanitized playback failure. Authenticated URLs and dependency error payloads
/// deliberately never cross this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackError {
  MpvNotFound,
  UnsupportedItemType,
  ItemNotPlayable,
  InvalidStartPosition,
  InvalidVolume,
  PlaybackInfoUnavailable,
  MediaSourceUnavailable,
  StreamUrlUnavailable,
  SubtitleUrlUnavailable,
  TrackUnavailable,
  MpvStartFailed,
  MpvLoadFailed,
  MpvControlFailed,
  NoActivePlayback,
}

impl fmt::Display for PlaybackError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      Self::MpvNotFound => "MPV executable not found",
      Self::UnsupportedItemType => "only movies and episodes can be played",
      Self::ItemNotPlayable => "the selected item is not playable",
      Self::InvalidStartPosition => "playback start position is invalid",
      Self::InvalidVolume => "volume must be a finite value from 0 to 100",
      Self::PlaybackInfoUnavailable => "playback information is unavailable",
      Self::MediaSourceUnavailable => "no playable media source is available",
      Self::StreamUrlUnavailable => "the authenticated media stream is unavailable",
      Self::SubtitleUrlUnavailable => "the authenticated subtitle stream is unavailable",
      Self::TrackUnavailable => "the selected media track is unavailable",
      Self::MpvStartFailed => "MPV could not be started",
      Self::MpvLoadFailed => "MPV could not load the selected item",
      Self::MpvControlFailed => "MPV could not apply the transport command",
      Self::NoActivePlayback => "there is no active playback",
    })
  }
}

impl std::error::Error for PlaybackError {}

/// Single-current-item playback adapter shared by GTK application code.
pub struct PlaybackController {
  server: Arc<JellyfinClient>,
  mpv: MpvClient,
  configured_mpv_args: Vec<String>,
  active: Option<ActivePlayback>,
  active_transport_matches_mpv: bool,
  last_transport: PlayerState,
  last_progress_report_at: Option<Instant>,
  load_event_boundary: LoadEventBoundary,
  pending_client_messages: Vec<String>,
}

impl PlaybackController {
  /// Discover MPV and create a controller without starting the process.
  ///
  /// # Errors
  ///
  /// Returns [`PlaybackError::MpvNotFound`] when no configured or discoverable
  /// executable is available.
  pub fn discover(
    server: Arc<JellyfinClient>,
    config: PlaybackControllerConfig,
  ) -> Result<Self, PlaybackError> {
    let mpv_path = match config.mpv_path {
      Some(path) => path,
      None => find_mpv().ok_or(PlaybackError::MpvNotFound)?,
    };
    let mpv = MpvClient::new(Some(mpv_path));
    mpv.set_extra_args(config.extra_args.clone());
    if let Some(cache_dir) = config.demuxer_cache_dir {
      mpv.set_demuxer_cache_dir(cache_dir);
    }

    Ok(Self::from_mpv(server, mpv, config.extra_args))
  }

  /// Create a controller around an existing MPV client.
  ///
  /// This seam lets callers share process configuration and lets tests install
  /// the MPV crate's feature-gated in-memory IPC connection.
  #[must_use]
  pub fn from_mpv(
    server: Arc<JellyfinClient>,
    mpv: MpvClient,
    configured_mpv_args: Vec<String>,
  ) -> Self {
    Self {
      server,
      mpv,
      configured_mpv_args,
      active: None,
      active_transport_matches_mpv: false,
      last_transport: PlayerState::default(),
      last_progress_report_at: None,
      load_event_boundary: LoadEventBoundary::Settled,
      pending_client_messages: Vec::new(),
    }
  }

  /// Update process settings used the next time MPV starts.
  ///
  /// # Errors
  ///
  /// Returns [`PlaybackError::MpvNotFound`] when no explicit or discoverable
  /// executable is available.
  pub fn configure_for_next_start(
    &mut self,
    config: PlaybackControllerConfig,
  ) -> Result<(), PlaybackError> {
    let mpv_path = match config.mpv_path {
      Some(path) => path,
      None => find_mpv().ok_or(PlaybackError::MpvNotFound)?,
    };
    self.mpv.set_mpv_path(Some(mpv_path));
    self.mpv.set_extra_args(config.extra_args.clone());
    if let Some(cache_dir) = config.demuxer_cache_dir {
      self.mpv.set_demuxer_cache_dir(cache_dir);
    }
    self.configured_mpv_args = config.extra_args;
    Ok(())
  }

  /// Resolve and play a Library summary item.
  ///
  /// # Errors
  ///
  /// Returns a sanitized [`PlaybackError`] when the item, server response, MPV
  /// startup, or MPV load command cannot be completed.
  pub async fn play_library_item(
    &mut self,
    item: &VideoLibraryItem,
    options: PlaybackOptions,
  ) -> Result<PlaybackStartOutcome, PlaybackError> {
    let request = PlayableRequest::from_library_item(item, options)?;
    self.play(request).await
  }

  /// Resolve and play a fully loaded Library item detail.
  ///
  /// # Errors
  ///
  /// Returns a sanitized [`PlaybackError`] when the item, server response, MPV
  /// startup, or MPV load command cannot be completed.
  pub async fn play_item_detail(
    &mut self,
    item: &VideoItemDetail,
    options: PlaybackOptions,
  ) -> Result<PlaybackStartOutcome, PlaybackError> {
    let request = PlayableRequest::from_item_detail(item, options)?;
    self.play(request).await
  }

  /// Resolve and play a media item returned by the adjacent-episode API.
  ///
  /// # Errors
  ///
  /// Returns a sanitized [`PlaybackError`] when the item, server response, MPV
  /// startup, or MPV load command cannot be completed.
  pub async fn play_media_item(
    &mut self,
    item: &MediaItem,
    options: PlaybackOptions,
  ) -> Result<PlaybackStartOutcome, PlaybackError> {
    let request = PlayableRequest::from_media_item(item, options)?;
    self.play(request).await
  }

  /// Read MPV transport without changing lifecycle or reporting state.
  ///
  /// Shell refresh loops should call [`Self::refresh`] so natural completion,
  /// disconnection, progress reporting, and warnings are reconciled.
  pub async fn snapshot(&self) -> PlaybackSnapshot {
    self.snapshot_with_transport(self.collect_transport().await.unwrap_or_default())
  }

  /// Read the active MPV track list without changing playback state.
  pub async fn tracks(&self) -> Result<Vec<TrackInfo>, PlaybackError> {
    let value = self
      .mpv
      .get_property("track-list")
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)?;
    let jellypilot_mpv::PropertyValue::Json(json) = value else {
      return Err(PlaybackError::TrackUnavailable);
    };
    parse_track_list(&json)
  }

  /// Select an audio track by MPV track id.
  pub async fn select_audio_track(&self, id: i64) -> Result<(), PlaybackError> {
    self
      .mpv
      .set_audio_track(id)
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)
  }

  /// Select or disable a subtitle track by MPV track id.
  pub async fn select_subtitle_track(&self, id: Option<i64>) -> Result<(), PlaybackError> {
    let result = match mpv_subtitle_selection(id) {
      MpvSubtitleSelection::Track(id) => self.mpv.set_subtitle_track(id).await,
      MpvSubtitleSelection::Value(value) => self.mpv.set_property_string("sid", value).await,
    };
    result.map_err(|_| PlaybackError::MpvControlFailed)
  }

  /// Show a transient message in MPV's on-screen display.
  pub async fn show_text(&self, text: &str, duration_ms: i64) -> Result<(), PlaybackError> {
    self
      .mpv
      .show_text(text, duration_ms)
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)
  }

  /// Drain script-message names observed since the last shell refresh.
  pub fn take_client_messages(&mut self) -> Vec<String> {
    std::mem::take(&mut self.pending_client_messages)
  }
  /// Reconcile the active item with MPV and report periodic progress.
  ///
  /// A disconnected process or MPV end-of-file event ends the active session,
  /// reports stop from its last known position, and cleans the MPV runtime.
  /// Reporting failures are returned as sanitized warnings for the shell.
  pub async fn refresh(&mut self) -> PlaybackRefreshOutcome {
    if self.active.is_none() {
      self.last_progress_report_at = None;
      self.load_event_boundary = LoadEventBoundary::Settled;
      self.active_transport_matches_mpv = false;
      self.pending_client_messages.clear();
      return PlaybackRefreshOutcome {
        snapshot: self.snapshot_with_transport(PlayerState::default()),
        state: PlaybackRefreshState::Idle,
        warnings: Vec::new(),
      };
    }

    if let Some(reason) = self.take_terminal_end_reason() {
      return self.finish_ended_playback(reason).await;
    }

    let Some(transport) = self.collect_transport().await else {
      return self
        .finish_ended_playback(PlaybackEndReason::Disconnected)
        .await;
    };

    self.record_transport(&transport);
    let warnings = if passive_progress_report_due(
      self.last_progress_report_at,
      Instant::now(),
      PASSIVE_PROGRESS_REPORT_INTERVAL,
    ) {
      warning_for_reporting(
        self.report_progress_now(&transport).await,
        PlaybackWarning::PlaybackProgressNotReported,
      )
    } else {
      Vec::new()
    };
    PlaybackRefreshOutcome {
      snapshot: self.snapshot_with_transport(transport),
      state: PlaybackRefreshState::Active,
      warnings,
    }
  }

  /// Gracefully stop/report any active item and always clean the MPV runtime.
  pub async fn shutdown(&mut self) -> PlaybackShutdownOutcome {
    if self.active.is_some() && self.active_transport_matches_mpv {
      let transport = self
        .collect_transport()
        .await
        .unwrap_or_else(|| self.last_transport.clone());
      self.record_transport(&transport);
    }
    let active = self.active.clone();
    self.last_progress_report_at = None;
    self.load_event_boundary = LoadEventBoundary::Settled;
    self.active_transport_matches_mpv = false;
    self.last_transport = PlayerState::default();
    self.pending_client_messages.clear();
    let _ = self.mpv.quit().await;
    let (stopped_active_playback, warnings) = match active {
      Some(active) => (
        true,
        warning_for_reporting(
          self.report_stop(&active).await,
          PlaybackWarning::PlaybackStopNotReported,
        ),
      ),
      None => (false, Vec::new()),
    };
    self.active = None;

    PlaybackShutdownOutcome {
      stopped_active_playback,
      warnings,
    }
  }

  /// Pause or resume the current item and report the resulting state.
  ///
  /// # Errors
  ///
  /// Returns [`PlaybackError::NoActivePlayback`] without an item, or
  /// [`PlaybackError::MpvControlFailed`] when MPV rejects the command.
  pub async fn set_paused(
    &mut self,
    paused: bool,
  ) -> Result<PlaybackControlOutcome, PlaybackError> {
    self.require_active()?;
    self
      .mpv
      .set_pause(paused)
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)?;

    let mut transport = self
      .collect_transport()
      .await
      .unwrap_or_else(|| self.last_transport.clone());
    transport.paused = paused;
    self.record_transport(&transport);
    let reporting = self.report_progress_now(&transport).await;
    Ok(self.control_outcome(transport, reporting))
  }

  /// Seek the current item to an absolute position in seconds.
  ///
  /// # Errors
  ///
  /// Returns an error for missing playback, a negative/non-finite position, or
  /// a rejected MPV command.
  pub async fn seek(
    &mut self,
    position_seconds: f64,
  ) -> Result<PlaybackControlOutcome, PlaybackError> {
    self.require_active()?;
    checked_seconds_to_ticks(position_seconds)?;
    self
      .mpv
      .seek(position_seconds)
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)?;

    let mut transport = self
      .collect_transport()
      .await
      .unwrap_or_else(|| self.last_transport.clone());
    transport.time_pos = position_seconds;
    self.record_transport(&transport);
    let reporting = self.report_progress_now(&transport).await;
    Ok(self.control_outcome(transport, reporting))
  }

  /// Set MPV volume on its 0–100 scale.
  ///
  /// # Errors
  ///
  /// Returns an error for missing playback, out-of-range volume, or a rejected
  /// MPV command.
  pub async fn set_volume(&mut self, volume: f64) -> Result<PlaybackControlOutcome, PlaybackError> {
    self.require_active()?;
    validate_volume(volume)?;
    self
      .mpv
      .set_volume(volume)
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)?;

    let mut transport = self
      .collect_transport()
      .await
      .unwrap_or_else(|| self.last_transport.clone());
    transport.volume = volume;
    self.record_transport(&transport);
    let reporting = self.report_progress_now(&transport).await;
    Ok(self.control_outcome(transport, reporting))
  }

  /// Set MPV mute state idempotently.
  ///
  /// # Errors
  ///
  /// Returns an error for missing playback or a rejected MPV command.
  pub async fn set_muted(&mut self, muted: bool) -> Result<PlaybackControlOutcome, PlaybackError> {
    self.require_active()?;
    self
      .mpv
      .set_mute(muted)
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)?;

    let mut transport = self
      .collect_transport()
      .await
      .unwrap_or_else(|| self.last_transport.clone());
    transport.muted = muted;
    self.record_transport(&transport);
    let reporting = self.report_progress_now(&transport).await;
    Ok(self.control_outcome(transport, reporting))
  }

  /// Stop the current item, terminate the owned MPV process, and report stop.
  ///
  /// # Errors
  ///
  /// Returns [`PlaybackError::NoActivePlayback`] when no item is current.
  pub async fn stop(&mut self) -> Result<PlaybackStopOutcome, PlaybackError> {
    self.require_active()?;
    let transport = self
      .collect_transport()
      .await
      .unwrap_or_else(|| self.last_transport.clone());
    self.record_transport(&transport);
    let active = self.active.clone().ok_or(PlaybackError::NoActivePlayback)?;
    self.last_progress_report_at = None;
    self.load_event_boundary = LoadEventBoundary::Settled;
    self.active_transport_matches_mpv = false;
    self.last_transport = PlayerState::default();
    self.pending_client_messages.clear();
    let _ = self.mpv.quit().await;

    let warnings = warning_for_reporting(
      self.report_stop(&active).await,
      PlaybackWarning::PlaybackStopNotReported,
    );
    self.active = None;
    Ok(PlaybackStopOutcome { warnings })
  }

  async fn play(
    &mut self,
    request: PlayableRequest,
  ) -> Result<PlaybackStartOutcome, PlaybackError> {
    let resolved = self.resolve(request).await?;
    let mut warnings = self.settle_disconnected_previous().await;
    // Keep the previous item owned by the controller until the replacement is
    // fully loaded. This makes cancellation safe: shutdown can still report and
    // clean the old item if the shell drops an in-flight start future.
    let previous = self.active.clone();
    self.last_progress_report_at = None;
    self.pending_client_messages.clear();
    if !self.mpv.is_connected() && self.mpv.start().await.is_err() {
      self.cleanup_failed_load(previous.as_ref()).await;
      return Err(PlaybackError::MpvStartFailed);
    }

    let file_options = direct_playback_file_options(
      &resolved.active.now_playing.play_method,
      &self.configured_mpv_args,
    );
    if !self.load_resolved(&resolved, file_options).await {
      self.cleanup_failed_load(previous.as_ref()).await;
      return Err(PlaybackError::MpvLoadFailed);
    }

    if let Some(previous) = previous.as_ref() {
      if self.report_stop(previous).await == ReportingStatus::Failed {
        warnings.push(PlaybackWarning::PreviousPlaybackStopNotReported);
      }
    }
    self.active = Some(resolved.active);
    self.active_transport_matches_mpv = true;

    let active = self
      .active
      .as_ref()
      .ok_or(PlaybackError::NoActivePlayback)?;
    if self
      .mpv
      .set_property_string("force-media-title", &active.now_playing.title)
      .await
      .is_err()
    {
      warnings.push(PlaybackWarning::MediaTitleUnavailable);
    }
    if let Some(subtitle_url) = resolved.external_subtitle_url {
      if self.mpv.sub_add(subtitle_url.as_str(), true).await.is_err() {
        warnings.push(PlaybackWarning::ExternalSubtitleUnavailable);
      }
    }

    let mut baseline = self.last_transport.clone();
    baseline.connected = true;
    baseline.paused = false;
    baseline.time_pos = active.now_playing.start_position_seconds;
    baseline.duration = active.now_playing.runtime_seconds.unwrap_or_default();
    let sample = collect_player_state_sample(&self.mpv).await;
    let mut transport = if sample.is_connected() {
      sample.merge(&baseline)
    } else {
      baseline
    };
    // The load boundary is authoritative for the new item's initial transport.
    // A late property response can still describe the replaced file.
    transport.connected = true;
    transport.paused = false;
    transport.time_pos = active.now_playing.start_position_seconds;
    self.record_transport(&transport);
    let active = self
      .active
      .as_ref()
      .ok_or(PlaybackError::NoActivePlayback)?;
    if self.report_start(active, &transport).await == ReportingStatus::Failed {
      warnings.push(PlaybackWarning::PlaybackStartNotReported);
    }
    self.last_progress_report_at = Some(Instant::now());

    Ok(PlaybackStartOutcome {
      snapshot: self.snapshot_with_transport(transport),
      warnings,
    })
  }

  async fn resolve(&self, request: PlayableRequest) -> Result<ResolvedPlayback, PlaybackError> {
    let start_position_seconds = request.start_position_seconds()?;
    let start_position_ticks = checked_seconds_to_ticks(start_position_seconds)?;
    let server_start_ticks =
      matches!(self.server.provider(), MediaServerProvider::Emby).then_some(start_position_ticks);
    let playback = self
      .server
      .playback()
      .get_playback_info(
        &request.item_id,
        server_start_ticks,
        request.options.audio_stream_index,
        request.options.subtitle_stream_index,
      )
      .await
      .map_err(|_| PlaybackError::PlaybackInfoUnavailable)?;
    let media_source = select_media_source(
      &playback.media_sources,
      request.options.media_source_id.as_deref(),
    )?;
    let stream_url = self
      .server
      .playback()
      .build_stream_url(&request.item_id, media_source)
      .map(AuthenticatedUrl)
      .ok_or(PlaybackError::StreamUrlUnavailable)?;

    let mpv_audio_index = resolve_mpv_track(
      &media_source.media_streams,
      "Audio",
      request.options.audio_stream_index,
    )?;
    let (mpv_subtitle_index, external_subtitle_url) = self.resolve_subtitle(
      &request.item_id,
      media_source,
      request.options.subtitle_stream_index,
    )?;
    let runtime_seconds = request.runtime_seconds.or_else(|| {
      media_source
        .run_time_ticks
        .filter(|ticks| *ticks >= 0)
        .map(ticks_to_seconds)
    });

    Ok(ResolvedPlayback {
      active: ActivePlayback {
        now_playing: NowPlayingItem {
          item_id: request.item_id,
          title: request.title,
          item_type: request.item_type,
          runtime_seconds,
          start_position_seconds,
          play_method: play_method(media_source).to_owned(),
        },
        media_source_id: media_source.id.clone(),
        play_session_id: playback.play_session_id,
        audio_stream_index: request.options.audio_stream_index,
        subtitle_stream_index: request.options.subtitle_stream_index,
        last_known_position_seconds: start_position_seconds,
      },
      stream_url,
      external_subtitle_url,
      mpv_audio_index,
      mpv_subtitle_index,
    })
  }

  fn resolve_subtitle(
    &self,
    item_id: &str,
    media_source: &MediaSource,
    selected_index: Option<i32>,
  ) -> Result<(Option<i64>, Option<AuthenticatedUrl>), PlaybackError> {
    let Some(selected_index) = selected_index else {
      return Ok((None, None));
    };
    if selected_index < 0 {
      return Ok((Some(i64::from(selected_index)), None));
    }
    let stream = find_stream(&media_source.media_streams, "Subtitle", selected_index)?;
    if !stream.is_external {
      return Ok((
        Some(type_local_track_index(
          &media_source.media_streams,
          "Subtitle",
          selected_index,
        )?),
        None,
      ));
    }

    let subtitle_url = self
      .server
      .playback()
      .build_subtitle_url(item_id, &media_source.id, stream)
      .map(AuthenticatedUrl)
      .ok_or(PlaybackError::SubtitleUrlUnavailable)?;
    Ok((None, Some(subtitle_url)))
  }

  fn require_active(&self) -> Result<&ActivePlayback, PlaybackError> {
    self.active.as_ref().ok_or(PlaybackError::NoActivePlayback)
  }

  fn record_transport(&mut self, transport: &PlayerState) {
    let Some(active) = self.active.as_mut() else {
      return;
    };
    if transport.connected && checked_seconds_to_ticks(transport.time_pos).is_ok() {
      active.last_known_position_seconds = transport.time_pos;
      self.last_transport = transport.clone();
    }
  }

  async fn collect_transport(&self) -> Option<PlayerState> {
    let sample = collect_player_state_sample(&self.mpv).await;
    if !sample.is_connected() {
      return None;
    }
    Some(sample.merge(&self.last_transport))
  }

  async fn load_resolved(
    &mut self,
    resolved: &ResolvedPlayback,
    file_options: Vec<String>,
  ) -> bool {
    let Some(events) = self.mpv.events() else {
      self.load_event_boundary = LoadEventBoundary::Settled;
      return false;
    };
    while events.try_recv().is_ok() {}
    self.load_event_boundary = LoadEventBoundary::AwaitingStart;
    // From this point MPV may already have accepted the replacement even if
    // the awaiting Rust future is cancelled. Keep the previous server item for
    // stop attribution, but never sample the replacement transport into it.
    self.active_transport_matches_mpv = false;

    if self
      .mpv
      .loadfile_with_options(
        resolved.stream_url.as_str(),
        Some(resolved.active.now_playing.start_position_seconds),
        resolved.mpv_audio_index,
        resolved.mpv_subtitle_index,
        file_options,
      )
      .await
      .is_err()
    {
      return false;
    }

    load_completed_with_timeout(MPV_FILE_LOAD_TIMEOUT, async {
      loop {
        let Ok(event) = events.recv().await else {
          return false;
        };
        if self.load_event_boundary.observe(&event).is_some() {
          return false;
        }
        if self.load_event_boundary == LoadEventBoundary::Settled {
          return true;
        }
      }
    })
    .await
  }

  async fn cleanup_failed_load(&mut self, previous: Option<&ActivePlayback>) {
    self.last_progress_report_at = None;
    self.load_event_boundary = LoadEventBoundary::Settled;
    self.active_transport_matches_mpv = false;
    self.last_transport = PlayerState::default();
    self.mpv.stop().await;
    if let Some(previous) = previous {
      let _ = self.report_stop(previous).await;
    }
    self.active = None;
  }

  fn take_terminal_end_reason(&mut self) -> Option<PlaybackEndReason> {
    let events = self.mpv.events()?;
    let mut reason = None;
    while let Ok(event) = events.try_recv() {
      if event.event == "client-message" {
        if let Some(message) = event.args.as_ref().and_then(|args| args.first()) {
          self.pending_client_messages.push(message.clone());
        }
      }
      reason = self.load_event_boundary.observe(&event).or(reason);
    }
    reason
  }

  async fn settle_disconnected_previous(&mut self) -> Vec<PlaybackWarning> {
    if self.active.is_none() {
      return Vec::new();
    }

    if self.take_terminal_end_reason().is_none() {
      if let Some(transport) = self.collect_transport().await {
        self.record_transport(&transport);
        return Vec::new();
      }
    }

    let active = self.active.clone();
    self.last_progress_report_at = None;
    self.load_event_boundary = LoadEventBoundary::Settled;
    self.active_transport_matches_mpv = false;
    self.last_transport = PlayerState::default();
    let _ = self.mpv.quit().await;
    let warnings = match active {
      Some(active) => warning_for_reporting(
        self.report_stop(&active).await,
        PlaybackWarning::PreviousPlaybackStopNotReported,
      ),
      None => Vec::new(),
    };
    self.active = None;
    warnings
  }

  async fn finish_ended_playback(&mut self, reason: PlaybackEndReason) -> PlaybackRefreshOutcome {
    let active = self.active.clone();
    self.last_progress_report_at = None;
    self.load_event_boundary = LoadEventBoundary::Settled;
    self.active_transport_matches_mpv = false;
    self.last_transport = PlayerState::default();
    let _ = self.mpv.quit().await;
    let warnings = match active {
      Some(active) => warning_for_reporting(
        self.report_stop(&active).await,
        PlaybackWarning::PlaybackStopNotReported,
      ),
      None => Vec::new(),
    };
    self.active = None;

    PlaybackRefreshOutcome {
      snapshot: self.snapshot_with_transport(PlayerState::default()),
      state: PlaybackRefreshState::Ended(reason),
      warnings,
    }
  }

  fn snapshot_with_transport(&self, transport: PlayerState) -> PlaybackSnapshot {
    PlaybackSnapshot {
      now_playing: self
        .active
        .as_ref()
        .map(|active| active.now_playing.clone()),
      transport,
    }
  }

  fn control_outcome(
    &self,
    transport: PlayerState,
    reporting: ReportingStatus,
  ) -> PlaybackControlOutcome {
    PlaybackControlOutcome {
      snapshot: self.snapshot_with_transport(transport),
      warnings: warning_for_reporting(reporting, PlaybackWarning::PlaybackProgressNotReported),
    }
  }

  async fn report_start(
    &self,
    active: &ActivePlayback,
    transport: &PlayerState,
  ) -> ReportingStatus {
    let info = PlaybackStartInfo {
      item_id: active.now_playing.item_id.clone(),
      media_source_id: Some(active.media_source_id.clone()),
      play_session_id: active.play_session_id.clone(),
      position_ticks: checked_seconds_to_ticks(transport.time_pos).ok(),
      is_paused: transport.paused,
      is_muted: transport.muted,
      volume_level: volume_level(transport.volume),
      audio_stream_index: active.audio_stream_index,
      subtitle_stream_index: active.subtitle_stream_index,
      play_method: active.now_playing.play_method.clone(),
      can_seek: true,
    };
    let playback = self.server.playback();
    reporting_status_with_timeout(
      PLAYBACK_REPORT_TIMEOUT,
      playback.report_playback_start(&info),
    )
    .await
  }

  async fn report_progress_for_transport(&self, transport: &PlayerState) -> ReportingStatus {
    let Some(active) = self.active.as_ref() else {
      return ReportingStatus::Failed;
    };
    let info = PlaybackProgressInfo {
      item_id: active.now_playing.item_id.clone(),
      media_source_id: Some(active.media_source_id.clone()),
      play_session_id: active.play_session_id.clone(),
      position_ticks: checked_seconds_to_ticks(transport.time_pos).ok(),
      is_paused: transport.paused,
      is_muted: transport.muted,
      volume_level: volume_level(transport.volume),
      audio_stream_index: active.audio_stream_index,
      subtitle_stream_index: active.subtitle_stream_index,
      play_method: active.now_playing.play_method.clone(),
      can_seek: true,
    };
    let playback = self.server.playback();
    reporting_status_with_timeout(
      PLAYBACK_REPORT_TIMEOUT,
      playback.report_playback_progress(&info),
    )
    .await
  }

  async fn report_progress_now(&mut self, transport: &PlayerState) -> ReportingStatus {
    let reporting = self.report_progress_for_transport(transport).await;
    self.last_progress_report_at = Some(Instant::now());
    reporting
  }

  async fn report_stop(&self, active: &ActivePlayback) -> ReportingStatus {
    let info = PlaybackStopInfo {
      item_id: active.now_playing.item_id.clone(),
      media_source_id: Some(active.media_source_id.clone()),
      play_session_id: active.play_session_id.clone(),
      position_ticks: checked_seconds_to_ticks(active.last_known_position_seconds).ok(),
    };
    let playback = self.server.playback();
    reporting_status_with_timeout(
      PLAYBACK_REPORT_TIMEOUT,
      playback.report_playback_stop(&info),
    )
    .await
  }
}

#[derive(Clone)]
struct ActivePlayback {
  now_playing: NowPlayingItem,
  media_source_id: String,
  play_session_id: Option<String>,
  audio_stream_index: Option<i32>,
  subtitle_stream_index: Option<i32>,
  last_known_position_seconds: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadEventBoundary {
  Settled,
  AwaitingStart,
  Loading,
}

impl LoadEventBoundary {
  fn observe(&mut self, event: &MpvEvent) -> Option<PlaybackEndReason> {
    match *self {
      Self::Settled => playback_end_reason(event),
      Self::AwaitingStart => match event.event.as_str() {
        "start-file" => {
          *self = Self::Loading;
          None
        }
        "file-loaded" => {
          *self = Self::Settled;
          None
        }
        _ => None,
      },
      Self::Loading => match event.event.as_str() {
        "file-loaded" => {
          *self = Self::Settled;
          None
        }
        "end-file" if event.reason.as_deref() == Some("redirect") => {
          *self = Self::AwaitingStart;
          None
        }
        "end-file" => {
          *self = Self::Settled;
          playback_end_reason(event)
        }
        _ => None,
      },
    }
  }
}

struct ResolvedPlayback {
  active: ActivePlayback,
  stream_url: AuthenticatedUrl,
  external_subtitle_url: Option<AuthenticatedUrl>,
  mpv_audio_index: Option<i64>,
  mpv_subtitle_index: Option<i64>,
}

struct AuthenticatedUrl(String);

impl AuthenticatedUrl {
  fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Debug for AuthenticatedUrl {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("AuthenticatedUrl([redacted])")
  }
}

struct PlayableRequest {
  item_id: String,
  title: String,
  item_type: String,
  runtime_seconds: Option<f64>,
  resume_position_seconds: Option<f64>,
  options: PlaybackOptions,
}

impl PlayableRequest {
  fn from_library_item(
    item: &VideoLibraryItem,
    options: PlaybackOptions,
  ) -> Result<Self, PlaybackError> {
    validate_item_type(&item.item_type)?;
    Ok(Self {
      item_id: item.id.clone(),
      title: item_title(
        &item.name,
        &item.item_type,
        item.series_name.as_deref(),
        item.season_number,
        item.episode_number,
      ),
      item_type: item.item_type.clone(),
      runtime_seconds: item.runtime_seconds,
      resume_position_seconds: item.resume_position_seconds,
      options,
    })
  }

  fn from_item_detail(
    item: &VideoItemDetail,
    options: PlaybackOptions,
  ) -> Result<Self, PlaybackError> {
    validate_item_type(&item.item_type)?;
    if !item.can_play {
      return Err(PlaybackError::ItemNotPlayable);
    }
    Ok(Self {
      item_id: item.id.clone(),
      title: item_title(
        &item.name,
        &item.item_type,
        item.series_name.as_deref(),
        item.season_number,
        item.episode_number,
      ),
      item_type: item.item_type.clone(),
      runtime_seconds: item.runtime_seconds,
      resume_position_seconds: item.resume_position_seconds,
      options,
    })
  }

  fn from_media_item(item: &MediaItem, options: PlaybackOptions) -> Result<Self, PlaybackError> {
    validate_item_type(&item.item_type)?;
    Ok(Self {
      item_id: item.id.clone(),
      title: item_title(
        &item.name,
        &item.item_type,
        item.series_name.as_deref(),
        item.parent_index_number,
        item.index_number,
      ),
      item_type: item.item_type.clone(),
      runtime_seconds: item
        .run_time_ticks
        .filter(|ticks| *ticks >= 0)
        .map(ticks_to_seconds),
      resume_position_seconds: None,
      options,
    })
  }

  fn start_position_seconds(&self) -> Result<f64, PlaybackError> {
    let seconds = match self.options.start_position {
      PlaybackStartPosition::Beginning => 0.0,
      PlaybackStartPosition::Resume => self.resume_position_seconds.unwrap_or(0.0),
      PlaybackStartPosition::At(seconds) => seconds,
    };
    checked_seconds_to_ticks(seconds)?;
    Ok(seconds)
  }
}

fn validate_item_type(item_type: &str) -> Result<(), PlaybackError> {
  if matches!(item_type, "Movie" | "Episode") {
    Ok(())
  } else {
    Err(PlaybackError::UnsupportedItemType)
  }
}

fn checked_seconds_to_ticks(seconds: f64) -> Result<i64, PlaybackError> {
  if !seconds.is_finite() || seconds < 0.0 {
    return Err(PlaybackError::InvalidStartPosition);
  }
  let ticks = seconds * MEDIA_TICKS_PER_SECOND as f64;
  if ticks > i64::MAX as f64 {
    return Err(PlaybackError::InvalidStartPosition);
  }
  Ok(ticks.round() as i64)
}

fn validate_volume(volume: f64) -> Result<(), PlaybackError> {
  if volume.is_finite() && (0.0..=100.0).contains(&volume) {
    Ok(())
  } else {
    Err(PlaybackError::InvalidVolume)
  }
}

fn volume_level(volume: f64) -> i32 {
  if volume.is_finite() {
    volume.clamp(0.0, 100.0).round() as i32
  } else {
    100
  }
}

fn reporting_status(success: bool) -> ReportingStatus {
  if success {
    ReportingStatus::Reported
  } else {
    ReportingStatus::Failed
  }
}

async fn reporting_status_with_timeout<E>(
  timeout: Duration,
  report: impl std::future::Future<Output = Result<(), E>>,
) -> ReportingStatus {
  reporting_status(
    relm4::tokio::time::timeout(timeout, report)
      .await
      .is_ok_and(|result| result.is_ok()),
  )
}

async fn load_completed_with_timeout(
  timeout: Duration,
  wait_for_load: impl std::future::Future<Output = bool>,
) -> bool {
  relm4::tokio::time::timeout(timeout, wait_for_load)
    .await
    .unwrap_or(false)
}

fn warning_for_reporting(
  status: ReportingStatus,
  warning: PlaybackWarning,
) -> Vec<PlaybackWarning> {
  if status == ReportingStatus::Reported {
    Vec::new()
  } else {
    vec![warning]
  }
}

fn passive_progress_report_due(
  last_report_at: Option<Instant>,
  now: Instant,
  interval: Duration,
) -> bool {
  last_report_at
    .is_some_and(|last_report_at| now.saturating_duration_since(last_report_at) >= interval)
}

fn playback_end_reason(event: &MpvEvent) -> Option<PlaybackEndReason> {
  if event.event != "end-file" {
    return None;
  }
  match event.reason.as_deref() {
    Some("eof") => Some(PlaybackEndReason::EndOfFile),
    Some("redirect") => None,
    _ => Some(PlaybackEndReason::Error),
  }
}

fn select_media_source<'a>(
  media_sources: &'a [MediaSource],
  selected_id: Option<&str>,
) -> Result<&'a MediaSource, PlaybackError> {
  match selected_id {
    Some(id) => media_sources.iter().find(|source| source.id == id),
    None => media_sources.first(),
  }
  .ok_or(PlaybackError::MediaSourceUnavailable)
}

fn find_stream<'a>(
  streams: &'a [MediaStream],
  stream_type: &str,
  provider_index: i32,
) -> Result<&'a MediaStream, PlaybackError> {
  streams
    .iter()
    .find(|stream| stream.stream_type == stream_type && stream.index == provider_index)
    .ok_or(PlaybackError::TrackUnavailable)
}

fn resolve_mpv_track(
  streams: &[MediaStream],
  stream_type: &str,
  selected_index: Option<i32>,
) -> Result<Option<i64>, PlaybackError> {
  selected_index
    .map(|index| {
      if index < 0 {
        Ok(i64::from(index))
      } else {
        type_local_track_index(streams, stream_type, index)
      }
    })
    .transpose()
}

fn type_local_track_index(
  streams: &[MediaStream],
  stream_type: &str,
  provider_index: i32,
) -> Result<i64, PlaybackError> {
  find_stream(streams, stream_type, provider_index)?;
  streams
    .iter()
    .filter(|stream| stream.stream_type == stream_type)
    .position(|stream| stream.index == provider_index)
    .and_then(|position| i64::try_from(position).ok())
    .and_then(|position| position.checked_add(1))
    .ok_or(PlaybackError::TrackUnavailable)
}

fn play_method(media_source: &MediaSource) -> &'static str {
  if media_source.supports_direct_play {
    "DirectPlay"
  } else if media_source.supports_direct_stream {
    "DirectStream"
  } else {
    "Transcode"
  }
}
fn parse_track_list(json: &str) -> Result<Vec<TrackInfo>, PlaybackError> {
  let values: Vec<serde_json::Value> =
    serde_json::from_str(json).map_err(|_| PlaybackError::TrackUnavailable)?;
  let mut tracks = Vec::new();
  for value in values {
    let Some(id) = value.get("id").and_then(serde_json::Value::as_i64) else {
      continue;
    };
    let Some(track_type) = value.get("type").and_then(serde_json::Value::as_str) else {
      continue;
    };
    if !matches!(track_type, "audio" | "sub") {
      continue;
    }
    tracks.push(TrackInfo {
      id,
      track_type: track_type.to_owned(),
      title: value
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::to_owned),
      language: value
        .get("lang")
        .and_then(|value| value.as_str())
        .map(str::to_owned),
      selected: value
        .get("selected")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false),
    });
  }
  Ok(tracks)
}

fn direct_playback_file_options(play_method: &str, configured_args: &[String]) -> Vec<String> {
  if !matches!(play_method, "DirectPlay" | "DirectStream") {
    return Vec::new();
  }
  DIRECT_PLAYBACK_CACHE_OPTIONS
    .iter()
    .filter(|(name, _)| !has_mpv_option(configured_args, name))
    .map(|(_, option)| (*option).to_owned())
    .collect()
}

fn item_title(
  name: &str,
  item_type: &str,
  series_name: Option<&str>,
  season_number: Option<i32>,
  episode_number: Option<i32>,
) -> String {
  if item_type != "Episode" {
    return name.to_owned();
  }
  let Some(series_name) = series_name else {
    return name.to_owned();
  };
  match (season_number, episode_number) {
    (Some(season), Some(episode)) => {
      format!("{series_name} - S{season:02}E{episode:02} - {name}")
    }
    _ => format!("{series_name} - {name}"),
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;

  use super::*;

  fn run_async<T>(future: impl Future<Output = T>) -> T {
    relm4::tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .expect("test runtime should build")
      .block_on(future)
  }
  #[test]
  fn track_list_parser_filters_and_maps_tracks() {
    let tracks = parse_track_list(
      r#"[{"id":1,"type":"audio","title":"English","lang":"eng","selected":true},{"id":2,"type":"sub","title":"Spanish","lang":"spa","selected":false},{"id":3,"type":"video"}]"#,
    )
    .unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].id, 1);
    assert_eq!(tracks[0].track_type, "audio");
    assert!(tracks[0].selected);
    assert_eq!(tracks[1].language.as_deref(), Some("spa"));
  }

  #[test]
  fn track_list_parser_rejects_invalid_json() {
    assert!(parse_track_list("not-json").is_err());
  }

  #[test]
  fn subtitle_off_and_negative_ids_map_to_mpv_no() {
    assert_eq!(
      mpv_subtitle_selection(None),
      MpvSubtitleSelection::Value("no")
    );
    assert_eq!(
      mpv_subtitle_selection(Some(-1)),
      MpvSubtitleSelection::Value("no")
    );
    assert_eq!(
      mpv_subtitle_selection(Some(2)),
      MpvSubtitleSelection::Track(2)
    );
  }

  fn library_item(item_type: &str) -> VideoLibraryItem {
    VideoLibraryItem {
      id: "item-1".to_owned(),
      name: "Pilot".to_owned(),
      item_type: item_type.to_owned(),
      production_year: None,
      runtime_seconds: Some(1_500.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      season_number: Some(1),
      episode_number: Some(2),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series".to_owned()),
      resume_position_seconds: Some(42.5),
      played_percentage: None,
      overview: None,
    }
  }

  fn stream(index: i32, stream_type: &str) -> MediaStream {
    MediaStream {
      index,
      stream_type: stream_type.to_owned(),
      codec: None,
      video_range: None,
      video_range_type: None,
      color_transfer: None,
      bit_depth: None,
      channels: None,
      language: None,
      display_title: None,
      is_default: false,
      is_external: false,
    }
  }

  fn active_playback(position_seconds: f64) -> ActivePlayback {
    ActivePlayback {
      now_playing: NowPlayingItem {
        item_id: "item-1".to_owned(),
        title: "Pilot".to_owned(),
        item_type: "Episode".to_owned(),
        runtime_seconds: Some(1_500.0),
        start_position_seconds: 42.5,
        play_method: "DirectPlay".to_owned(),
      },
      media_source_id: "source-1".to_owned(),
      play_session_id: Some("play-1".to_owned()),
      audio_stream_index: None,
      subtitle_stream_index: None,
      last_known_position_seconds: position_seconds,
    }
  }

  fn controller_with_active(position_seconds: f64) -> PlaybackController {
    let mut controller = PlaybackController::from_mpv(
      Arc::new(JellyfinClient::new()),
      MpvClient::new(None),
      Vec::new(),
    );
    controller.active = Some(active_playback(position_seconds));
    controller.active_transport_matches_mpv = true;
    controller
  }

  #[test]
  fn library_item_resume_uses_provider_position_without_exposing_transport_data() {
    let item = library_item("Episode");
    let request = PlayableRequest::from_library_item(
      &item,
      PlaybackOptions {
        start_position: PlaybackStartPosition::Resume,
        ..PlaybackOptions::default()
      },
    )
    .expect("episode should be playable");

    assert_eq!(request.start_position_seconds(), Ok(42.5));
  }

  #[test]
  fn library_item_rejects_non_playable_show_summary() {
    let item = library_item("Series");

    let result = PlayableRequest::from_library_item(&item, PlaybackOptions::default());

    assert!(matches!(result, Err(PlaybackError::UnsupportedItemType)));
  }

  #[test]
  fn provider_track_index_maps_to_mpv_type_local_track_number() {
    let streams = vec![
      stream(0, "Video"),
      stream(3, "Audio"),
      stream(5, "Audio"),
      stream(7, "Subtitle"),
    ];

    assert_eq!(resolve_mpv_track(&streams, "Audio", Some(5)), Ok(Some(2)));
  }

  #[test]
  fn provider_track_index_rejects_missing_selected_track() {
    let streams = vec![stream(0, "Video"), stream(3, "Audio")];

    assert_eq!(
      resolve_mpv_track(&streams, "Audio", Some(5)),
      Err(PlaybackError::TrackUnavailable)
    );
  }

  #[test]
  fn direct_playback_options_preserve_explicit_mpv_override() {
    let options = direct_playback_file_options(
      "DirectPlay",
      &["--cache=no".to_owned(), "--fullscreen".to_owned()],
    );

    assert!(!options.iter().any(|option| option.starts_with("cache=")));
  }

  #[test]
  fn authenticated_url_debug_output_is_always_redacted() {
    let url =
      AuthenticatedUrl("https://media.example/video?api_key=do-not-print-this-token".to_owned());

    assert_eq!(format!("{url:?}"), "AuthenticatedUrl([redacted])");
  }

  #[test]
  fn checked_seconds_to_ticks_rejects_non_finite_position() {
    assert_eq!(
      checked_seconds_to_ticks(f64::NAN),
      Err(PlaybackError::InvalidStartPosition)
    );
  }

  #[test]
  fn episode_title_includes_series_and_episode_coordinates() {
    assert_eq!(
      item_title("Pilot", "Episode", Some("Series"), Some(1), Some(2)),
      "Series - S01E02 - Pilot"
    );
  }

  #[test]
  fn connected_transport_updates_last_known_stop_position() {
    let mut controller = controller_with_active(42.5);
    controller.record_transport(&PlayerState {
      connected: true,
      time_pos: 117.25,
      ..PlayerState::default()
    });

    assert_eq!(
      controller
        .active
        .as_ref()
        .map(|active| active.last_known_position_seconds),
      Some(117.25)
    );
  }

  #[test]
  fn disconnected_transport_preserves_last_known_stop_position() {
    let mut controller = controller_with_active(117.25);
    controller.record_transport(&PlayerState::default());

    assert_eq!(
      controller
        .active
        .as_ref()
        .map(|active| active.last_known_position_seconds),
      Some(117.25)
    );
  }

  #[test]
  fn progress_reporting_failure_becomes_a_sanitized_warning() {
    assert_eq!(
      warning_for_reporting(
        ReportingStatus::Failed,
        PlaybackWarning::PlaybackProgressNotReported,
      ),
      vec![PlaybackWarning::PlaybackProgressNotReported]
    );
  }

  #[test]
  fn passive_progress_report_waits_until_interval_elapses() {
    let last_report_at = Instant::now();
    let before_interval =
      last_report_at + PASSIVE_PROGRESS_REPORT_INTERVAL - Duration::from_nanos(1);

    assert!(!passive_progress_report_due(
      Some(last_report_at),
      before_interval,
      PASSIVE_PROGRESS_REPORT_INTERVAL,
    ));
  }

  #[test]
  fn passive_progress_report_is_due_at_interval_boundary() {
    let last_report_at = Instant::now();

    assert!(passive_progress_report_due(
      Some(last_report_at),
      last_report_at + PASSIVE_PROGRESS_REPORT_INTERVAL,
      PASSIVE_PROGRESS_REPORT_INTERVAL,
    ));
  }

  #[test]
  fn passive_progress_report_is_disabled_without_an_active_schedule() {
    assert!(!passive_progress_report_due(
      None,
      Instant::now(),
      PASSIVE_PROGRESS_REPORT_INTERVAL,
    ));
  }

  fn lifecycle_event(event: &str, reason: Option<&str>) -> MpvEvent {
    MpvEvent {
      event: event.to_owned(),
      id: None,
      name: None,
      data: None,
      reason: reason.map(str::to_owned),
      args: None,
    }
  }

  fn mpv_event(reason: &str) -> MpvEvent {
    lifecycle_event("end-file", Some(reason))
  }

  #[test]
  fn eof_event_maps_to_natural_end() {
    assert_eq!(
      playback_end_reason(&mpv_event("eof")),
      Some(PlaybackEndReason::EndOfFile)
    );
  }

  #[test]
  fn error_event_maps_to_terminal_error() {
    assert_eq!(
      playback_end_reason(&mpv_event("error")),
      Some(PlaybackEndReason::Error)
    );
  }

  #[test]
  fn external_stop_event_maps_to_terminal_error() {
    assert_eq!(
      playback_end_reason(&mpv_event("stop")),
      Some(PlaybackEndReason::Error)
    );
  }

  #[test]
  fn replacement_boundary_ignores_old_stop_until_new_file_is_loaded() {
    let mut boundary = LoadEventBoundary::AwaitingStart;
    let old_stop = boundary.observe(&mpv_event("stop"));
    let new_start = boundary.observe(&lifecycle_event("start-file", None));
    let new_loaded = boundary.observe(&lifecycle_event("file-loaded", None));

    assert_eq!(
      (old_stop, new_start, new_loaded, boundary),
      (None, None, None, LoadEventBoundary::Settled)
    );
  }

  #[test]
  fn replacement_boundary_ignores_late_old_eof() {
    let mut boundary = LoadEventBoundary::AwaitingStart;
    let old_eof = boundary.observe(&mpv_event("eof"));
    let new_start = boundary.observe(&lifecycle_event("start-file", None));
    let new_loaded = boundary.observe(&lifecycle_event("file-loaded", None));

    assert_eq!(
      (old_eof, new_start, new_loaded, boundary),
      (None, None, None, LoadEventBoundary::Settled)
    );
  }

  #[test]
  fn replacement_boundary_ignores_late_old_error_but_ends_loaded_new_item() {
    let mut boundary = LoadEventBoundary::AwaitingStart;
    let old_error = boundary.observe(&mpv_event("error"));
    let new_start = boundary.observe(&lifecycle_event("start-file", None));
    let new_loaded = boundary.observe(&lifecycle_event("file-loaded", None));
    let new_error = boundary.observe(&mpv_event("error"));

    assert_eq!(
      (old_error, new_start, new_loaded, new_error, boundary),
      (
        None,
        None,
        None,
        Some(PlaybackEndReason::Error),
        LoadEventBoundary::Settled,
      )
    );
  }

  #[test]
  fn replacement_boundary_treats_new_load_error_as_terminal() {
    let mut boundary = LoadEventBoundary::AwaitingStart;
    let new_start = boundary.observe(&lifecycle_event("start-file", None));
    let new_error = boundary.observe(&mpv_event("error"));

    assert_eq!(
      (new_start, new_error, boundary),
      (
        None,
        Some(PlaybackEndReason::Error),
        LoadEventBoundary::Settled,
      )
    );
  }

  #[test]
  fn replacement_boundary_ignores_redirect_before_followup_file_load() {
    let mut boundary = LoadEventBoundary::AwaitingStart;
    let first_start = boundary.observe(&lifecycle_event("start-file", None));
    let redirect = boundary.observe(&mpv_event("redirect"));
    let redirected_start = boundary.observe(&lifecycle_event("start-file", None));
    let redirected_loaded = boundary.observe(&lifecycle_event("file-loaded", None));

    assert_eq!(
      (
        first_start,
        redirect,
        redirected_start,
        redirected_loaded,
        boundary,
      ),
      (None, None, None, None, LoadEventBoundary::Settled)
    );
  }

  #[test]
  fn replacement_boundary_treats_new_stop_as_terminal_after_start_file() {
    let mut boundary = LoadEventBoundary::AwaitingStart;
    let new_start = boundary.observe(&lifecycle_event("start-file", None));
    let new_stop = boundary.observe(&mpv_event("stop"));

    assert_eq!(
      (new_start, new_stop, boundary),
      (
        None,
        Some(PlaybackEndReason::Error),
        LoadEventBoundary::Settled,
      )
    );
  }

  #[test]
  fn load_boundary_timeout_rejects_a_pending_load() {
    let loaded = run_async(load_completed_with_timeout(
      Duration::ZERO,
      std::future::pending::<bool>(),
    ));

    assert!(!loaded);
  }

  #[test]
  fn reporting_timeout_returns_failed_for_a_pending_request() {
    let status = run_async(reporting_status_with_timeout(
      Duration::ZERO,
      std::future::pending::<Result<(), ()>>(),
    ));

    assert_eq!(status, ReportingStatus::Failed);
  }

  #[test]
  fn reporting_timeout_preserves_an_immediate_success() {
    let status = run_async(reporting_status_with_timeout(
      Duration::from_secs(1),
      std::future::ready(Ok::<(), ()>(())),
    ));

    assert_eq!(status, ReportingStatus::Reported);
  }

  #[test]
  fn refresh_reconciles_disconnected_active_playback_without_a_process() {
    let mut controller = controller_with_active(117.25);
    controller.last_progress_report_at = Some(Instant::now());

    let outcome = run_async(controller.refresh());

    assert_eq!(
      (
        outcome.state,
        outcome.snapshot.now_playing.is_none(),
        controller.last_progress_report_at.is_none(),
        outcome.warnings,
      ),
      (
        PlaybackRefreshState::Ended(PlaybackEndReason::Disconnected),
        true,
        true,
        vec![PlaybackWarning::PlaybackStopNotReported],
      )
    );
  }

  #[test]
  fn failed_load_cleanup_clears_attribution_and_boundary_without_a_process() {
    let mut controller = controller_with_active(117.25);
    let previous = active_playback(117.25);
    controller.last_progress_report_at = Some(Instant::now());
    controller.load_event_boundary = LoadEventBoundary::Loading;

    run_async(controller.cleanup_failed_load(Some(&previous)));

    assert_eq!(
      (
        controller.active.is_none(),
        controller.last_progress_report_at.is_none(),
        controller.load_event_boundary,
        controller.mpv.is_connected(),
      ),
      (true, true, LoadEventBoundary::Settled, false)
    );
  }

  #[test]
  fn shutdown_clears_active_playback_without_a_process() {
    let mut controller = controller_with_active(117.25);
    controller.last_progress_report_at = Some(Instant::now());

    let outcome = run_async(controller.shutdown());

    assert_eq!(
      (
        outcome.stopped_active_playback,
        controller.active.is_none(),
        controller.last_progress_report_at.is_none(),
        outcome.warnings,
      ),
      (
        true,
        true,
        true,
        vec![PlaybackWarning::PlaybackStopNotReported],
      )
    );
  }

  #[test]
  fn stop_clears_passive_progress_schedule() {
    let mut controller = controller_with_active(117.25);
    controller.last_progress_report_at = Some(Instant::now());

    let result = run_async(controller.stop());

    assert!(
      result.is_ok() && controller.last_progress_report_at.is_none(),
      "stop result was {result:?}"
    );
  }
}
