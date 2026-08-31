//! Framework-independent external MPV playback for the native GTK shell.

use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{
  collect_player_state_sample, find_mpv, has_mpv_option, MpvClient, MpvEvent, PlayerState,
  PropertyValue,
};
use jellypilot_media_server::{
  ticks_to_seconds, JellyfinClient, MediaItem, MediaServerProvider, MediaSource, MediaStream,
  PlaybackProgressInfo, PlaybackStartInfo, PlaybackStopInfo, VideoItemDetail, VideoLibraryItem,
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

/// Provider selections attached to a remote playback request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaybackSelection {
  pub media_source_id: Option<String>,
  pub audio_stream_index: Option<i32>,
  pub subtitle_stream_index: Option<i32>,
}
/// An item accepted by the playback controller.
#[derive(Debug, Clone)]
pub enum Playable {
  Library(VideoLibraryItem),
  Detail(VideoItemDetail),
  Media(MediaItem),
}

impl Playable {
  #[must_use]
  pub fn item_id(&self) -> &str {
    match self {
      Self::Library(item) => &item.id,
      Self::Detail(item) => &item.id,
      Self::Media(item) => &item.id,
    }
  }

  /// Artwork image for the player bar: the series poster when present, the
  /// item's own artwork otherwise. Bare media items carry no image reference.
  #[must_use]
  pub fn image_id(&self) -> Option<&str> {
    match self {
      Self::Library(item) => item
        .series_poster_image_id
        .as_deref()
        .or(item.artwork_image_id.as_deref()),
      Self::Detail(item) => item
        .series_poster_image_id
        .as_deref()
        .or(item.artwork_image_id.as_deref()),
      Self::Media(_) => None,
    }
  }
}

/// Enriches a bare media-item playable with the fully resolved adjacent
/// playable when the same item was already looked up in either direction.
#[must_use]
pub fn rich_playable(adjacent: &[Option<Playable>; 2], item: &Playable) -> Playable {
  let Playable::Media(media) = item else {
    return item.clone();
  };
  adjacent
    .iter()
    .flatten()
    .find(|playable| playable.item_id() == media.id)
    .cloned()
    .unwrap_or_else(|| item.clone())
}

impl From<VideoLibraryItem> for Playable {
  fn from(item: VideoLibraryItem) -> Self {
    Self::Library(item)
  }
}

impl From<VideoItemDetail> for Playable {
  fn from(item: VideoItemDetail) -> Self {
    Self::Detail(item)
  }
}

impl From<MediaItem> for Playable {
  fn from(item: MediaItem) -> Self {
    Self::Media(item)
  }
}

/// Reconstructs the media-server item needed for adjacent episode lookup.
#[must_use]
pub fn media_item_from_playable(item: &Playable) -> MediaItem {
  match item {
    Playable::Library(item) => MediaItem {
      id: item.id.clone(),
      name: item.name.clone(),
      item_type: item.item_type.clone(),
      series_id: item.series_id.clone(),
      series_name: item.series_name.clone(),
      season_name: None,
      index_number: item.episode_number,
      parent_index_number: item.season_number,
      run_time_ticks: crate::player::runtime_seconds_to_ticks(item.runtime_seconds),
      overview: item.overview.clone(),
      series_primary_image_tag: None,
    },
    Playable::Detail(item) => MediaItem {
      id: item.id.clone(),
      name: item.name.clone(),
      item_type: item.item_type.clone(),
      series_id: item.series_id.clone(),
      series_name: item.series_name.clone(),
      season_name: None,
      index_number: item.episode_number,
      parent_index_number: item.season_number,
      run_time_ticks: crate::player::runtime_seconds_to_ticks(item.runtime_seconds),
      overview: item.overview.clone(),
      series_primary_image_tag: None,
    },
    Playable::Media(item) => item.clone(),
  }
}

/// Track metadata read from MPV's authoritative track-list property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
  pub id: i64,
  pub track_type: String,
  pub title: Option<String>,
  pub language: Option<String>,
  pub selected: bool,
  /// Provider media-stream index corresponding to this MPV track, when known.
  pub provider_index: Option<i32>,
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

/// Result returned after MPV accepted a new item or a transport control.
#[must_use = "playback warnings must be surfaced to the user"]
#[derive(Debug, Clone)]
pub struct PlaybackOutcome {
  pub snapshot: PlaybackSnapshot,
  pub warnings: Vec<PlaybackWarning>,
}

/// Result returned after selecting a media track.
#[must_use = "playback warnings must be surfaced to the user"]
#[derive(Debug, Clone)]
pub struct TrackSelectionOutcome {
  pub tracks: Vec<TrackInfo>,
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
/// Boxed asynchronous operation exposed by [`PlaybackServer`].
pub type PlaybackServerFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Token-free request for resolving an item into playable media.
#[derive(Debug, Clone)]
pub struct PlaybackResolutionRequest {
  pub item_id: String,
  pub start_time_ticks: Option<i64>,
  pub selection: PlaybackSelection,
}

/// Media-server data required to load a resolved item.
#[derive(Clone)]
pub struct PlaybackResolution {
  pub media_source: MediaSource,
  pub play_session_id: Option<String>,
  pub stream_url: AuthenticatedUrl,
  pub external_subtitle_url: Option<AuthenticatedUrl>,
}
// MediaSource embeds tokenized stream URLs; keep them out of Debug output.
impl fmt::Debug for PlaybackResolution {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("PlaybackResolution")
      .field("media_source_id", &self.media_source.id)
      .field("play_session_id", &self.play_session_id)
      .field("stream_url", &self.stream_url)
      .finish()
  }
}

/// Token-free state sent for playback start and progress reports.
#[derive(Debug, Clone)]
pub struct PlaybackReport {
  pub item_id: String,
  pub media_source_id: String,
  pub play_session_id: Option<String>,
  pub position_ticks: Option<i64>,
  pub is_paused: bool,
  pub is_muted: bool,
  pub volume_level: i32,
  pub audio_stream_index: Option<i32>,
  pub subtitle_stream_index: Option<i32>,
  pub play_method: String,
}

/// Token-free state sent when playback stops.
#[derive(Debug, Clone)]
pub struct PlaybackStopReport {
  pub item_id: String,
  pub media_source_id: String,
  pub play_session_id: Option<String>,
  pub position_ticks: Option<i64>,
}

/// Media-server operations required by the playback controller.
pub trait PlaybackServer: Send + Sync {
  fn provider(&self) -> MediaServerProvider;

  fn resolve(
    &self,
    request: PlaybackResolutionRequest,
  ) -> PlaybackServerFuture<'_, Result<PlaybackResolution, PlaybackError>>;

  fn report_playback_start(
    &self,
    report: PlaybackReport,
  ) -> PlaybackServerFuture<'_, Result<(), ()>>;

  fn report_playback_progress(
    &self,
    report: PlaybackReport,
  ) -> PlaybackServerFuture<'_, Result<(), ()>>;

  fn report_playback_stop(
    &self,
    report: PlaybackStopReport,
  ) -> PlaybackServerFuture<'_, Result<(), ()>>;
}

/// Production playback-server adapter backed by an authenticated Jellyfin client.
pub struct JellyfinPlaybackServer(Arc<JellyfinClient>);

impl From<Arc<JellyfinClient>> for JellyfinPlaybackServer {
  fn from(server: Arc<JellyfinClient>) -> Self {
    Self(server)
  }
}

impl PlaybackServer for JellyfinPlaybackServer {
  fn provider(&self) -> MediaServerProvider {
    self.0.provider()
  }

  fn resolve(
    &self,
    request: PlaybackResolutionRequest,
  ) -> PlaybackServerFuture<'_, Result<PlaybackResolution, PlaybackError>> {
    Box::pin(async move {
      let playback = self
        .0
        .playback()
        .get_playback_info(
          &request.item_id,
          request.start_time_ticks,
          request.selection.audio_stream_index,
          request.selection.subtitle_stream_index,
        )
        .await
        .map_err(|error| {
          log::warn!("playback info request failed: {error}");
          PlaybackError::PlaybackInfoUnavailable
        })?;
      let media_source = select_media_source(
        &playback.media_sources,
        request.selection.media_source_id.as_deref(),
      )?
      .clone();
      let stream_url = self
        .0
        .playback()
        .build_stream_url(&request.item_id, &media_source)
        .map(AuthenticatedUrl::new)
        .ok_or(PlaybackError::StreamUrlUnavailable)?;
      let external_subtitle_url = request
        .selection
        .subtitle_stream_index
        .filter(|index| *index >= 0)
        .and_then(|index| find_stream(&media_source.media_streams, "Subtitle", index).ok())
        .filter(|stream| stream.is_external)
        .and_then(|stream| {
          self
            .0
            .playback()
            .build_subtitle_url(&request.item_id, &media_source.id, stream)
        })
        .map(AuthenticatedUrl::new);

      Ok(PlaybackResolution {
        media_source,
        play_session_id: playback.play_session_id,
        stream_url,
        external_subtitle_url,
      })
    })
  }

  fn report_playback_start(
    &self,
    report: PlaybackReport,
  ) -> PlaybackServerFuture<'_, Result<(), ()>> {
    Box::pin(async move {
      self
        .0
        .playback()
        .report_playback_start(&PlaybackStartInfo::from(report))
        .await
        .map_err(|_| ())
    })
  }

  fn report_playback_progress(
    &self,
    report: PlaybackReport,
  ) -> PlaybackServerFuture<'_, Result<(), ()>> {
    Box::pin(async move {
      self
        .0
        .playback()
        .report_playback_progress(&PlaybackProgressInfo::from(report))
        .await
        .map_err(|_| ())
    })
  }

  fn report_playback_stop(
    &self,
    report: PlaybackStopReport,
  ) -> PlaybackServerFuture<'_, Result<(), ()>> {
    Box::pin(async move {
      self
        .0
        .playback()
        .report_playback_stop(&PlaybackStopInfo::from(report))
        .await
        .map_err(|_| ())
    })
  }
}

impl From<PlaybackReport> for PlaybackStartInfo {
  fn from(report: PlaybackReport) -> Self {
    Self {
      item_id: report.item_id,
      media_source_id: Some(report.media_source_id),
      play_session_id: report.play_session_id,
      position_ticks: report.position_ticks,
      is_paused: report.is_paused,
      is_muted: report.is_muted,
      volume_level: report.volume_level,
      audio_stream_index: report.audio_stream_index,
      subtitle_stream_index: report.subtitle_stream_index,
      play_method: report.play_method,
      can_seek: true,
    }
  }
}

impl From<PlaybackReport> for PlaybackProgressInfo {
  fn from(report: PlaybackReport) -> Self {
    Self {
      item_id: report.item_id,
      media_source_id: Some(report.media_source_id),
      play_session_id: report.play_session_id,
      position_ticks: report.position_ticks,
      is_paused: report.is_paused,
      is_muted: report.is_muted,
      volume_level: report.volume_level,
      audio_stream_index: report.audio_stream_index,
      subtitle_stream_index: report.subtitle_stream_index,
      play_method: report.play_method,
      can_seek: true,
    }
  }
}

impl From<PlaybackStopReport> for PlaybackStopInfo {
  fn from(report: PlaybackStopReport) -> Self {
    Self {
      item_id: report.item_id,
      media_source_id: Some(report.media_source_id),
      play_session_id: report.play_session_id,
      position_ticks: report.position_ticks,
    }
  }
}

/// Single-current-item playback adapter shared by GTK application code.
pub struct PlaybackController {
  server: Arc<dyn PlaybackServer>,
  mpv: MpvClient,
  configured_mpv_args: Vec<String>,
  active: Option<ActivePlayback>,
  active_transport_matches_mpv: bool,
  last_transport: PlayerState,
  last_progress_report_at: Option<Instant>,
  load_event_boundary: LoadEventBoundary,
  pending_client_messages: Vec<String>,
  /// Fullscreen flag captured when a playback-owned process ended, applied
  /// once to the next process a play starts, then cleared. Manual stops keep
  /// no carry-over: an explicit stop resets the window state.
  pending_fullscreen: Option<bool>,
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
    Self::from_server(
      Arc::new(JellyfinPlaybackServer::from(server)),
      mpv,
      configured_mpv_args,
    )
  }

  fn from_server(
    server: Arc<dyn PlaybackServer>,
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
      pending_fullscreen: None,
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

  /// Resolve and play an item.
  ///
  /// # Errors
  ///
  /// Returns a sanitized [`PlaybackError`] when the item, server response, MPV
  /// startup, or MPV load command cannot be completed.
  pub async fn play(
    &mut self,
    playable: Playable,
    position: PlaybackStartPosition,
  ) -> Result<PlaybackOutcome, PlaybackError> {
    self
      .play_selected(playable, position, PlaybackSelection::default())
      .await
  }

  /// Resolve and play an item with provider media-source and track selections.
  ///
  /// # Errors
  ///
  /// Returns a sanitized [`PlaybackError`] when the selected source or track is
  /// unavailable, or when MPV cannot start or load it.
  pub async fn play_selected(
    &mut self,
    playable: Playable,
    position: PlaybackStartPosition,
    selection: PlaybackSelection,
  ) -> Result<PlaybackOutcome, PlaybackError> {
    let request = PlayableRequest::from_playable(playable, position, selection)?;
    self.play_request(request).await
  }

  /// Read the active MPV track list without changing playback state.
  pub async fn tracks(&self) -> Result<Vec<TrackInfo>, PlaybackError> {
    let value = self
      .mpv
      .get_property("track-list")
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)?;
    let crate::PropertyValue::Json(json) = value else {
      return Err(PlaybackError::TrackUnavailable);
    };
    let mut tracks = parse_track_list(&json)?;
    if let Some(active) = &self.active {
      assign_provider_indexes(
        &mut tracks,
        &active.media_streams,
        active.subtitle_stream_index,
      );
    }
    Ok(tracks)
  }

  /// Select an audio track by MPV track id and return the refreshed track list.
  pub async fn select_audio_track(
    &mut self,
    id: i64,
  ) -> Result<TrackSelectionOutcome, PlaybackError> {
    self
      .mpv
      .set_audio_track(id)
      .await
      .map_err(|_| PlaybackError::MpvControlFailed)?;
    Ok(TrackSelectionOutcome {
      tracks: self.tracks().await?,
      warnings: Vec::new(),
    })
  }

  /// Select or disable a subtitle track and return the refreshed track list.
  pub async fn select_subtitle_track(
    &mut self,
    id: Option<i64>,
  ) -> Result<TrackSelectionOutcome, PlaybackError> {
    match mpv_subtitle_selection(id) {
      MpvSubtitleSelection::Track(id) => self.mpv.set_subtitle_track(id).await,
      MpvSubtitleSelection::Value(value) => self.mpv.set_property_string("sid", value).await,
    }
    .map_err(|_| PlaybackError::MpvControlFailed)?;
    Ok(TrackSelectionOutcome {
      tracks: self.tracks().await?,
      warnings: Vec::new(),
    })
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
    let warnings = match active {
      Some(active) => warning_for_reporting(
        self.report_stop(&active).await,
        PlaybackWarning::PlaybackStopNotReported,
      ),
      None => Vec::new(),
    };
    self.active = None;

    PlaybackShutdownOutcome { warnings }
  }

  /// Pause or resume the current item and report the resulting state.
  ///
  /// # Errors
  ///
  /// Returns [`PlaybackError::NoActivePlayback`] without an item, or
  /// [`PlaybackError::MpvControlFailed`] when MPV rejects the command.
  pub async fn set_paused(&mut self, paused: bool) -> Result<PlaybackOutcome, PlaybackError> {
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
  pub async fn seek(&mut self, position_seconds: f64) -> Result<PlaybackOutcome, PlaybackError> {
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
  pub async fn set_volume(&mut self, volume: f64) -> Result<PlaybackOutcome, PlaybackError> {
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
  pub async fn set_muted(&mut self, muted: bool) -> Result<PlaybackOutcome, PlaybackError> {
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

  async fn play_request(
    &mut self,
    request: PlayableRequest,
  ) -> Result<PlaybackOutcome, PlaybackError> {
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

    // A playback-owned process takes its window state with it on teardown;
    // restore the captured fullscreen flag before loading so the replacement
    // window opens in the same state the user left.
    if let Some(fullscreen) = self.pending_fullscreen.take() {
      if self.mpv.set_fullscreen(fullscreen).await.is_err() {
        log::warn!("could not restore MPV fullscreen state");
      }
    }

    let file_options = direct_playback_file_options(
      &resolved.active.now_playing.play_method,
      &self.configured_mpv_args,
    );
    // MPV's HTTP fetch defaults to ffmpeg's Lavf agent, which media-fronting
    // proxies commonly block; presenting the player's identity passes
    // player-allowlisted servers. The user's own MPV arguments win.
    if !has_mpv_option(&self.configured_mpv_args, "user-agent")
      && self
        .mpv
        .set_property_string("user-agent", "mpv")
        .await
        .is_err()
    {
      log::warn!("could not pass the player user agent to MPV");
    }
    if !self.load_resolved(&resolved, file_options).await {
      self.cleanup_failed_load(previous.as_ref()).await;
      return Err(PlaybackError::MpvLoadFailed);
    }

    if let Some(previous) = previous.as_ref() {
      if !self.report_stop(previous).await {
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
    if !self.report_start(active, &transport).await {
      warnings.push(PlaybackWarning::PlaybackStartNotReported);
    }
    self.last_progress_report_at = Some(Instant::now());

    Ok(PlaybackOutcome {
      snapshot: self.snapshot_with_transport(transport),
      warnings,
    })
  }

  async fn resolve(&self, request: PlayableRequest) -> Result<ResolvedPlayback, PlaybackError> {
    let start_position_seconds = request.start_position_seconds()?;
    let start_position_ticks = checked_seconds_to_ticks(start_position_seconds)?;
    let server_start_ticks =
      matches!(self.server.provider(), MediaServerProvider::Emby).then_some(start_position_ticks);
    let selection = request.selection;
    let PlaybackResolution {
      media_source,
      play_session_id,
      stream_url,
      external_subtitle_url,
    } = self
      .server
      .resolve(PlaybackResolutionRequest {
        item_id: request.item_id.clone(),
        start_time_ticks: server_start_ticks,
        selection: selection.clone(),
      })
      .await?;
    let runtime_seconds = request.runtime_seconds.or_else(|| {
      media_source
        .run_time_ticks
        .filter(|ticks| *ticks >= 0)
        .map(ticks_to_seconds)
    });
    let mpv_audio_index = resolve_mpv_track(
      &media_source.media_streams,
      "Audio",
      selection.audio_stream_index,
    )?;
    let mpv_subtitle_index = if external_subtitle_url.is_some() {
      None
    } else {
      resolve_mpv_track(
        &media_source.media_streams,
        "Subtitle",
        selection.subtitle_stream_index,
      )?
    };

    Ok(ResolvedPlayback {
      active: ActivePlayback {
        now_playing: NowPlayingItem {
          item_id: request.item_id,
          title: request.title,
          item_type: request.item_type,
          runtime_seconds,
          start_position_seconds,
          play_method: play_method(&media_source).to_owned(),
        },
        media_source_id: media_source.id,
        play_session_id,
        audio_stream_index: selection.audio_stream_index,
        subtitle_stream_index: selection.subtitle_stream_index,
        media_streams: media_source.media_streams,
        last_known_position_seconds: start_position_seconds,
      },
      stream_url,
      external_subtitle_url,
      mpv_audio_index,
      mpv_subtitle_index,
    })
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
      log::warn!(
        "MPV rejected the loadfile command for {}",
        stream_url_head(resolved.stream_url.as_str())
      );
      return false;
    }

    load_completed_with_timeout(MPV_FILE_LOAD_TIMEOUT, async {
      loop {
        let Ok(event) = events.recv().await else {
          return false;
        };
        if let Some(reason) = self.load_event_boundary.observe(&event) {
          log::warn!(
            "MPV could not load {}: {reason:?}",
            stream_url_head(resolved.stream_url.as_str())
          );
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

  /// Snapshot the outgoing process's fullscreen flag so the next process a
  /// play starts can restore the window state. A dead or disconnected
  /// process yields `None`, leaving the next start at MPV's own default.
  async fn capture_fullscreen(&self) -> Option<bool> {
    match self.mpv.get_property("fullscreen").await {
      Ok(PropertyValue::Bool(fullscreen)) => Some(fullscreen),
      _ => None,
    }
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
    self.pending_fullscreen = self.capture_fullscreen().await;
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
    self.pending_fullscreen = self.capture_fullscreen().await;
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

  fn control_outcome(&self, transport: PlayerState, reported: bool) -> PlaybackOutcome {
    PlaybackOutcome {
      snapshot: self.snapshot_with_transport(transport),
      warnings: warning_for_reporting(reported, PlaybackWarning::PlaybackProgressNotReported),
    }
  }

  async fn report_start(&self, active: &ActivePlayback, transport: &PlayerState) -> bool {
    reporting_succeeded_with_timeout(
      PLAYBACK_REPORT_TIMEOUT,
      self
        .server
        .report_playback_start(playback_report(active, transport)),
    )
    .await
  }

  async fn report_progress_for_transport(&self, transport: &PlayerState) -> bool {
    let Some(active) = self.active.as_ref() else {
      return false;
    };
    reporting_succeeded_with_timeout(
      PLAYBACK_REPORT_TIMEOUT,
      self
        .server
        .report_playback_progress(playback_report(active, transport)),
    )
    .await
  }

  async fn report_progress_now(&mut self, transport: &PlayerState) -> bool {
    let reported = self.report_progress_for_transport(transport).await;
    self.last_progress_report_at = Some(Instant::now());
    reported
  }

  async fn report_stop(&self, active: &ActivePlayback) -> bool {
    reporting_succeeded_with_timeout(
      PLAYBACK_REPORT_TIMEOUT,
      self.server.report_playback_stop(PlaybackStopReport {
        item_id: active.now_playing.item_id.clone(),
        media_source_id: active.media_source_id.clone(),
        play_session_id: active.play_session_id.clone(),
        position_ticks: checked_seconds_to_ticks(active.last_known_position_seconds).ok(),
      }),
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
  media_streams: Vec<MediaStream>,
  last_known_position_seconds: f64,
}
fn playback_report(active: &ActivePlayback, transport: &PlayerState) -> PlaybackReport {
  PlaybackReport {
    item_id: active.now_playing.item_id.clone(),
    media_source_id: active.media_source_id.clone(),
    play_session_id: active.play_session_id.clone(),
    position_ticks: checked_seconds_to_ticks(transport.time_pos).ok(),
    is_paused: transport.paused,
    is_muted: transport.muted,
    volume_level: volume_level(transport.volume),
    audio_stream_index: active.audio_stream_index,
    subtitle_stream_index: active.subtitle_stream_index,
    play_method: active.now_playing.play_method.clone(),
  }
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

/// Authenticated playback URL whose debug output never exposes credentials.
#[derive(Clone)]
pub struct AuthenticatedUrl(String);

impl AuthenticatedUrl {
  /// Wrap an authenticated playback URL.
  #[must_use]
  pub fn new(url: String) -> Self {
    Self(url)
  }

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
  position: PlaybackStartPosition,
  selection: PlaybackSelection,
}

impl PlayableRequest {
  fn from_playable(
    playable: Playable,
    position: PlaybackStartPosition,
    selection: PlaybackSelection,
  ) -> Result<Self, PlaybackError> {
    match playable {
      Playable::Library(item) => {
        validate_item_type(&item.item_type)?;
        Ok(Self {
          item_id: item.id,
          title: item_title(
            &item.name,
            &item.item_type,
            item.series_name.as_deref(),
            item.season_number,
            item.episode_number,
          ),
          item_type: item.item_type,
          runtime_seconds: item.runtime_seconds,
          resume_position_seconds: item.resume_position_seconds,
          position,
          selection,
        })
      }
      Playable::Detail(item) => {
        validate_item_type(&item.item_type)?;
        if !item.can_play {
          return Err(PlaybackError::ItemNotPlayable);
        }
        Ok(Self {
          item_id: item.id,
          title: item_title(
            &item.name,
            &item.item_type,
            item.series_name.as_deref(),
            item.season_number,
            item.episode_number,
          ),
          item_type: item.item_type,
          runtime_seconds: item.runtime_seconds,
          resume_position_seconds: item.resume_position_seconds,
          position,
          selection,
        })
      }
      Playable::Media(item) => {
        validate_item_type(&item.item_type)?;
        Ok(Self {
          item_id: item.id,
          title: item_title(
            &item.name,
            &item.item_type,
            item.series_name.as_deref(),
            item.parent_index_number,
            item.index_number,
          ),
          item_type: item.item_type,
          runtime_seconds: item
            .run_time_ticks
            .filter(|ticks| *ticks >= 0)
            .map(ticks_to_seconds),
          resume_position_seconds: None,
          position,
          selection,
        })
      }
    }
  }

  fn start_position_seconds(&self) -> Result<f64, PlaybackError> {
    let seconds = match self.position {
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

async fn reporting_succeeded_with_timeout<E>(
  timeout: Duration,
  report: impl Future<Output = Result<(), E>>,
) -> bool {
  tokio::time::timeout(timeout, report)
    .await
    .is_ok_and(|result| result.is_ok())
}

async fn load_completed_with_timeout(
  timeout: Duration,
  wait_for_load: impl Future<Output = bool>,
) -> bool {
  tokio::time::timeout(timeout, wait_for_load)
    .await
    .unwrap_or(false)
}

fn warning_for_reporting(reported: bool, warning: PlaybackWarning) -> Vec<PlaybackWarning> {
  if reported {
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
      provider_index: None,
    });
  }
  Ok(tracks)
}

fn assign_provider_indexes(
  tracks: &mut [TrackInfo],
  streams: &[MediaStream],
  selected_subtitle_index: Option<i32>,
) {
  for track_type in ["audio", "sub"] {
    for (position, track) in tracks
      .iter_mut()
      .filter(|track| track.track_type == track_type)
      .enumerate()
    {
      track.provider_index =
        provider_stream_for_mpv_track(streams, track_type, position, selected_subtitle_index)
          .map(|stream| stream.index);
    }
  }
}

fn provider_stream_for_mpv_track<'a>(
  streams: &'a [MediaStream],
  track_type: &str,
  position: usize,
  selected_subtitle_index: Option<i32>,
) -> Option<&'a MediaStream> {
  let provider_type = if track_type == "audio" {
    "Audio"
  } else {
    "Subtitle"
  };
  let mut internal = streams
    .iter()
    .filter(|stream| stream.stream_type == provider_type && !stream.is_external);
  let internal_count = internal.clone().count();
  if position < internal_count {
    return internal.nth(position);
  }
  if track_type != "sub" || position != internal_count {
    return None;
  }
  streams.iter().find(|stream| {
    stream.stream_type == provider_type
      && stream.is_external
      && Some(stream.index) == selected_subtitle_index
  })
}

/// Stream URL without its query string, where the access token rides. Safe to
/// log; identifies the server route MPV was asked to open.
fn stream_url_head(url: &str) -> &str {
  url.split('?').next().unwrap_or(url)
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
  use std::sync::atomic::{AtomicBool, Ordering};
  use std::sync::Mutex;

  use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, WriteHalf};

  use super::*;

  fn run_async<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .expect("test runtime should build")
      .block_on(future)
  }

  #[derive(Default)]
  struct MockReports {
    starts: Vec<PlaybackReport>,
    progress: Vec<PlaybackReport>,
    stops: Vec<PlaybackStopReport>,
    resolutions: Vec<PlaybackResolutionRequest>,
  }

  struct MockPlaybackServer {
    provider: MediaServerProvider,
    resolution: PlaybackResolution,
    reports: Mutex<MockReports>,
    fail_start: AtomicBool,
    fail_progress: AtomicBool,
    fail_stop: AtomicBool,
  }

  impl MockPlaybackServer {
    fn new() -> Self {
      Self {
        provider: MediaServerProvider::Jellyfin,
        resolution: PlaybackResolution {
          media_source: MediaSource {
            id: "source-1".to_owned(),
            path: None,
            protocol: "Http".to_owned(),
            container: Some("mkv".to_owned()),
            run_time_ticks: Some(15_000_000_000),
            media_streams: Vec::new(),
            supports_direct_play: true,
            supports_direct_stream: true,
            supports_transcoding: true,
            direct_stream_url: None,
            add_api_key_to_direct_stream_url: None,
            transcoding_url: None,
          },
          play_session_id: Some("play-1".to_owned()),
          stream_url: AuthenticatedUrl::new(
            "https://media.example/video?api_key=secret".to_owned(),
          ),
          external_subtitle_url: None,
        },
        reports: Mutex::new(MockReports::default()),
        fail_start: AtomicBool::new(false),
        fail_progress: AtomicBool::new(false),
        fail_stop: AtomicBool::new(false),
      }
    }

    fn with_provider(mut self, provider: MediaServerProvider) -> Self {
      self.provider = provider;
      self
    }

    fn stop_item_ids(&self) -> Vec<String> {
      self
        .reports
        .lock()
        .expect("mock reports should not be poisoned")
        .stops
        .iter()
        .map(|report| report.item_id.clone())
        .collect()
    }

    fn start_track_indices(&self) -> Vec<(Option<i32>, Option<i32>)> {
      self
        .reports
        .lock()
        .expect("mock reports should not be poisoned")
        .starts
        .iter()
        .map(|report| (report.audio_stream_index, report.subtitle_stream_index))
        .collect()
    }

    fn resolution_start_ticks(&self) -> Vec<Option<i64>> {
      self
        .reports
        .lock()
        .expect("mock reports should not be poisoned")
        .resolutions
        .iter()
        .map(|request| request.start_time_ticks)
        .collect()
    }
  }

  impl PlaybackServer for MockPlaybackServer {
    fn provider(&self) -> MediaServerProvider {
      self.provider
    }

    fn resolve(
      &self,
      request: PlaybackResolutionRequest,
    ) -> PlaybackServerFuture<'_, Result<PlaybackResolution, PlaybackError>> {
      self
        .reports
        .lock()
        .expect("mock reports should not be poisoned")
        .resolutions
        .push(request);
      let resolution = self.resolution.clone();
      Box::pin(async move { Ok(resolution) })
    }

    fn report_playback_start(
      &self,
      report: PlaybackReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>> {
      self
        .reports
        .lock()
        .expect("mock reports should not be poisoned")
        .starts
        .push(report);
      let fails = self.fail_start.load(Ordering::Relaxed);
      Box::pin(async move {
        if fails {
          Err(())
        } else {
          Ok(())
        }
      })
    }

    fn report_playback_progress(
      &self,
      report: PlaybackReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>> {
      self
        .reports
        .lock()
        .expect("mock reports should not be poisoned")
        .progress
        .push(report);
      let fails = self.fail_progress.load(Ordering::Relaxed);
      Box::pin(async move {
        if fails {
          Err(())
        } else {
          Ok(())
        }
      })
    }

    fn report_playback_stop(
      &self,
      report: PlaybackStopReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>> {
      self
        .reports
        .lock()
        .expect("mock reports should not be poisoned")
        .stops
        .push(report);
      let fails = self.fail_stop.load(Ordering::Relaxed);
      Box::pin(async move {
        if fails {
          Err(())
        } else {
          Ok(())
        }
      })
    }
  }

  struct MpvPeerState {
    paused: bool,
    time_pos: f64,
    duration: f64,
    volume: f64,
    muted: bool,
    fullscreen: bool,
    audio_track: i64,
    subtitle_track: Option<i64>,
  }

  impl Default for MpvPeerState {
    fn default() -> Self {
      Self {
        paused: false,
        time_pos: 0.0,
        duration: 1_500.0,
        volume: 100.0,
        muted: false,
        fullscreen: false,
        audio_track: 1,
        subtitle_track: None,
      }
    }
  }

  struct InMemoryMpv {
    client: MpvClient,
    writer: Arc<tokio::sync::Mutex<WriteHalf<DuplexStream>>>,
    peer: tokio::task::JoinHandle<()>,
    received: Arc<Mutex<Vec<Vec<serde_json::Value>>>>,
  }

  impl InMemoryMpv {
    async fn new() -> Self {
      Self::connect(MpvClient::new(None)).await
    }

    /// Install a fresh in-memory IPC connection on the client, standing in for
    /// an MPV process listening on that connection.
    async fn connect(client: MpvClient) -> Self {
      let (client_stream, peer_stream) = duplex(128 * 1024);
      let (reader, writer) = tokio::io::split(client_stream);
      let transport = MpvClient::from_io_for_test(reader, writer)
        .await
        .expect("test MPV transport should be constructed");
      client.install_ipc_for_test(transport);

      let (peer_reader, peer_writer) = tokio::io::split(peer_stream);
      let writer = Arc::new(tokio::sync::Mutex::new(peer_writer));
      let task_writer = Arc::clone(&writer);
      let received = Arc::new(Mutex::new(Vec::new()));
      let task_received = Arc::clone(&received);
      let peer = tokio::spawn(async move {
        let mut lines = BufReader::new(peer_reader).lines();
        let mut state = MpvPeerState::default();
        while let Ok(Some(line)) = lines.next_line().await {
          let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
          };
          let Some(request_id) = message.get("request_id").and_then(|value| value.as_i64()) else {
            continue;
          };
          let command = message
            .get("command")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
          task_received
            .lock()
            .expect("received commands should not be poisoned")
            .push(command.clone());
          let data = apply_mpv_command(&mut state, &command);
          let mut writer = task_writer.lock().await;
          write_mpv_message(
            &mut writer,
            &serde_json::json!({
              "request_id": request_id,
              "error": "success",
              "data": data,
            }),
          )
          .await;
          if command.first().and_then(serde_json::Value::as_str) == Some("loadfile") {
            write_mpv_message(&mut writer, &serde_json::json!({"event": "start-file"})).await;
            write_mpv_message(&mut writer, &serde_json::json!({"event": "file-loaded"})).await;
          }
        }
      });

      Self {
        client,
        writer,
        peer,
        received,
      }
    }

    /// Simulate the controlled process being replaced: the controller closed
    /// the old connection on teardown, so install a fresh one.
    async fn respawn(&self) -> Self {
      Self::connect(self.client.clone()).await
    }

    fn received_commands(&self) -> Vec<Vec<serde_json::Value>> {
      self
        .received
        .lock()
        .expect("received commands should not be poisoned")
        .clone()
    }

    async fn emit_eof(&self) {
      {
        let mut writer = self.writer.lock().await;
        write_mpv_message(
          &mut writer,
          &serde_json::json!({"event": "end-file", "reason": "eof"}),
        )
        .await;
      }
      self
        .client
        .get_property("pause")
        .await
        .expect("barrier property should be readable");
    }
  }

  impl Drop for InMemoryMpv {
    fn drop(&mut self) {
      self.peer.abort();
    }
  }

  async fn write_mpv_message(writer: &mut WriteHalf<DuplexStream>, message: &serde_json::Value) {
    writer
      .write_all(format!("{message}\n").as_bytes())
      .await
      .expect("test MPV peer should stay writable");
  }

  fn apply_mpv_command(
    state: &mut MpvPeerState,
    command: &[serde_json::Value],
  ) -> serde_json::Value {
    let name = command.first().and_then(serde_json::Value::as_str);
    match name {
      Some("set_property") => {
        let property = command.get(1).and_then(serde_json::Value::as_str);
        let value = command.get(2).cloned().unwrap_or(serde_json::Value::Null);
        match property {
          Some("pause") => state.paused = value.as_bool().unwrap_or(state.paused),
          Some("volume") => state.volume = value.as_f64().unwrap_or(state.volume),
          Some("mute") => state.muted = value.as_bool().unwrap_or(state.muted),
          Some("fullscreen") => state.fullscreen = value.as_bool().unwrap_or(state.fullscreen),
          Some("aid") => state.audio_track = value.as_i64().unwrap_or(state.audio_track),
          Some("sid") => state.subtitle_track = value.as_i64(),
          _ => {}
        }
        serde_json::Value::Null
      }
      Some("seek") => {
        state.time_pos = command
          .get(1)
          .and_then(serde_json::Value::as_f64)
          .unwrap_or(state.time_pos);
        serde_json::Value::Null
      }
      Some("get_property") => match command.get(1).and_then(serde_json::Value::as_str) {
        Some("pause") => serde_json::json!(state.paused),
        Some("time-pos") => serde_json::json!(state.time_pos),
        Some("duration") => serde_json::json!(state.duration),
        Some("volume") => serde_json::json!(state.volume),
        Some("mute") => serde_json::json!(state.muted),
        Some("fullscreen") => serde_json::json!(state.fullscreen),
        Some("track-list") => serde_json::json!([
          {"id": 1, "type": "audio", "title": "English", "selected": state.audio_track == 1},
          {"id": 2, "type": "audio", "title": "Commentary", "selected": state.audio_track == 2},
          {"id": 3, "type": "sub", "title": "English", "selected": state.subtitle_track == Some(3)},
        ]),
        _ => serde_json::Value::Null,
      },
      _ => serde_json::Value::Null,
    }
  }

  async fn controller_harness(
    server: Arc<MockPlaybackServer>,
  ) -> (PlaybackController, InMemoryMpv) {
    let mpv = InMemoryMpv::new().await;
    let controller = PlaybackController::from_server(server, mpv.client.clone(), Vec::new());
    (controller, mpv)
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
      series_poster_image_id: None,
      season_number: Some(1),
      episode_number: Some(2),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series".to_owned()),
      resume_position_seconds: Some(42.5),
      played_percentage: None,
      overview: None,
    }
  }

  fn item_detail(can_play: bool) -> VideoItemDetail {
    VideoItemDetail {
      id: "item-1".to_owned(),
      name: "Pilot".to_owned(),
      item_type: "Episode".to_owned(),
      overview: None,
      production_year: None,
      runtime_seconds: Some(1_500.0),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series".to_owned()),
      season_number: Some(1),
      episode_number: Some(2),
      genres: Vec::new(),
      played: false,
      favorite: false,
      played_percentage: None,
      resume_position_seconds: Some(42.5),
      can_resume: true,
      can_play,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      metadata: jellypilot_media_server::VideoDetailMetadata::default(),
    }
  }

  fn media_item(id: &str) -> MediaItem {
    MediaItem {
      id: id.to_owned(),
      name: "Pilot".to_owned(),
      item_type: "Episode".to_owned(),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series".to_owned()),
      season_name: Some("Season 1".to_owned()),
      index_number: Some(2),
      parent_index_number: Some(1),
      run_time_ticks: Some(15_000_000_000),
      overview: None,
      series_primary_image_tag: None,
    }
  }

  #[test]
  fn playable_image_id_prefers_the_series_poster() {
    let mut item = library_item("Episode");
    item.artwork_image_id = Some("item-art".to_owned());
    assert_eq!(Playable::Library(item.clone()).image_id(), Some("item-art"));
    item.series_poster_image_id = Some("series-poster".to_owned());
    assert_eq!(Playable::Library(item).image_id(), Some("series-poster"));

    let mut detail = item_detail(true);
    detail.artwork_image_id = Some("detail-art".to_owned());
    assert_eq!(Playable::Detail(detail).image_id(), Some("detail-art"));

    assert_eq!(Playable::Media(media_item("item-1")).image_id(), None);
  }

  #[test]
  fn rich_playable_substitutes_a_matching_adjacent_entry() {
    let bare = Playable::Media(media_item("item-1"));
    let mut detail = item_detail(true);
    detail.artwork_image_id = Some("detail-art".to_owned());

    assert!(matches!(
      rich_playable(&[None, None], &bare),
      Playable::Media(_)
    ));
    assert!(matches!(
      rich_playable(&[None, Some(Playable::Detail(detail))], &bare),
      Playable::Detail(_)
    ));
    // Non-media playables and mismatched ids pass through unchanged.
    let library = Playable::Library(library_item("Episode"));
    assert!(matches!(
      rich_playable(&[Some(library.clone()), None], &library),
      Playable::Library(_)
    ));
    assert!(matches!(
      rich_playable(
        &[Some(library), None],
        &Playable::Media(media_item("other"))
      ),
      Playable::Media(_)
    ));
  }

  #[test]
  fn library_playable_reconstructs_adjacent_lookup_metadata() {
    let mut item = library_item("Episode");
    item.overview = Some("Episode overview".to_owned());

    let converted = media_item_from_playable(&Playable::Library(item));

    assert_eq!(
      (
        converted.id.as_str(),
        converted.name.as_str(),
        converted.item_type.as_str(),
        converted.series_id.as_deref(),
        converted.series_name.as_deref(),
        converted.season_name.as_deref(),
        converted.index_number,
        converted.parent_index_number,
        converted.run_time_ticks,
        converted.overview.as_deref(),
        converted.series_primary_image_tag.as_deref(),
      ),
      (
        "item-1",
        "Pilot",
        "Episode",
        Some("series-1"),
        Some("Series"),
        None,
        Some(2),
        Some(1),
        Some(15_000_000_000),
        Some("Episode overview"),
        None,
      )
    );
  }

  #[test]
  fn detail_playable_reconstructs_adjacent_lookup_metadata() {
    let mut item = item_detail(true);
    item.overview = Some("Detail overview".to_owned());

    let converted = media_item_from_playable(&Playable::Detail(item));

    assert_eq!(
      (
        converted.id.as_str(),
        converted.series_id.as_deref(),
        converted.series_name.as_deref(),
        converted.index_number,
        converted.parent_index_number,
        converted.run_time_ticks,
        converted.overview.as_deref(),
      ),
      (
        "item-1",
        Some("series-1"),
        Some("Series"),
        Some(2),
        Some(1),
        Some(15_000_000_000),
        Some("Detail overview"),
      )
    );
  }

  #[test]
  fn media_playable_preserves_the_lookup_item() {
    let item = media_item("media-item");
    let expected = serde_json::to_value(&item).expect("media item should serialize");

    let converted = media_item_from_playable(&Playable::Media(item));

    assert_eq!(
      serde_json::to_value(converted).expect("converted item should serialize"),
      expected
    );
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
      media_streams: Vec::new(),
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
    let request = PlayableRequest::from_playable(
      item.into(),
      PlaybackStartPosition::Resume,
      PlaybackSelection::default(),
    )
    .expect("episode should be playable");

    assert_eq!(request.start_position_seconds(), Ok(42.5));
  }

  #[test]
  fn library_item_rejects_non_playable_show_summary() {
    let item = library_item("Series");

    let result = PlayableRequest::from_playable(
      item.into(),
      PlaybackStartPosition::Beginning,
      PlaybackSelection::default(),
    );

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
  fn remote_selection_reaches_resolution_and_initial_mpv_tracks() {
    run_async(async {
      let mut mock = MockPlaybackServer::new();
      mock.resolution.media_source.media_streams = vec![
        stream(0, "Video"),
        stream(3, "Audio"),
        stream(5, "Audio"),
        stream(7, "Subtitle"),
      ];
      let server = Arc::new(mock);
      let controller =
        PlaybackController::from_server(server.clone(), MpvClient::new(None), Vec::new());
      let selection = PlaybackSelection {
        media_source_id: Some("source-1".to_owned()),
        audio_stream_index: Some(5),
        subtitle_stream_index: Some(7),
      };
      let request = PlayableRequest::from_playable(
        Playable::Media(media_item("episode-1")),
        PlaybackStartPosition::Beginning,
        selection.clone(),
      )
      .expect("remote episode should be playable");

      let resolved = controller
        .resolve(request)
        .await
        .expect("remote selection should resolve");

      assert_eq!(resolved.mpv_audio_index, Some(2));
      assert_eq!(resolved.mpv_subtitle_index, Some(1));
      assert_eq!(resolved.active.audio_stream_index, Some(5));
      assert_eq!(resolved.active.subtitle_stream_index, Some(7));
      let reports = server
        .reports
        .lock()
        .expect("mock reports should not be poisoned");
      assert_eq!(reports.resolutions[0].selection, selection);
    });
  }

  #[test]
  fn current_mpv_tracks_receive_provider_stream_indexes() {
    let mut tracks = vec![
      TrackInfo {
        id: 2,
        track_type: "audio".to_owned(),
        title: None,
        language: None,
        selected: true,
        provider_index: None,
      },
      TrackInfo {
        id: 6,
        track_type: "sub".to_owned(),
        title: None,
        language: None,
        selected: false,
        provider_index: None,
      },
    ];

    assign_provider_indexes(
      &mut tracks,
      &[
        stream(0, "Video"),
        stream(4, "Audio"),
        stream(7, "Subtitle"),
      ],
      None,
    );

    assert_eq!(tracks[0].provider_index, Some(4));
    assert_eq!(tracks[1].provider_index, Some(7));
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
  fn playback_resolution_debug_output_omits_tokenized_media_source_urls() {
    let resolution = PlaybackResolution {
      media_source: MediaSource {
        id: "source-1".to_owned(),
        path: None,
        protocol: "Http".to_owned(),
        container: Some("mkv".to_owned()),
        run_time_ticks: Some(15_000_000_000),
        media_streams: Vec::new(),
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: true,
        direct_stream_url: Some(
          "https://media.example/video?api_key=do-not-print-this-token".to_owned(),
        ),
        add_api_key_to_direct_stream_url: Some(true),
        transcoding_url: None,
      },
      play_session_id: Some("play-1".to_owned()),
      stream_url: AuthenticatedUrl(
        "https://media.example/video?api_key=do-not-print-this-token".to_owned(),
      ),
      external_subtitle_url: None,
    };

    let debug = format!("{resolution:?}");

    assert!(!debug.contains("do-not-print-this-token"));
    assert!(debug.contains("source-1"));
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
      warning_for_reporting(false, PlaybackWarning::PlaybackProgressNotReported),
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
    let reported = run_async(reporting_succeeded_with_timeout(
      Duration::ZERO,
      std::future::pending::<Result<(), ()>>(),
    ));

    assert!(!reported);
  }

  #[test]
  fn reporting_timeout_preserves_an_immediate_success() {
    let reported = run_async(reporting_succeeded_with_timeout(
      Duration::from_secs(1),
      std::future::ready(Ok::<(), ()>(())),
    ));

    assert!(reported);
  }

  #[test]
  fn play_refreshes_through_natural_end_of_file() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, mpv) = controller_harness(Arc::clone(&server)).await;

      let started = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Resume,
        )
        .await
        .expect("playback should start");
      mpv.emit_eof().await;
      let refreshed = controller.refresh().await;

      assert_eq!(
        (
          started.snapshot.now_playing.is_some(),
          refreshed.state,
          refreshed.snapshot.now_playing.is_none(),
          server.stop_item_ids(),
        ),
        (
          true,
          PlaybackRefreshState::Ended(PlaybackEndReason::EndOfFile),
          true,
          vec!["item-1".to_owned()],
        )
      );
    });
  }

  #[test]
  fn ended_playback_restores_fullscreen_on_the_next_process() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, mpv) = controller_harness(server).await;

      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");
      mpv
        .client
        .set_fullscreen(true)
        .await
        .expect("fullscreen should be settable");
      mpv.emit_eof().await;
      let refreshed = controller.refresh().await;
      assert_eq!(
        refreshed.state,
        PlaybackRefreshState::Ended(PlaybackEndReason::EndOfFile)
      );

      let next_process = mpv.respawn().await;
      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("the next episode should start");

      let commands = next_process.received_commands();
      let fullscreen_at = commands.iter().position(|command| {
        command.first().and_then(serde_json::Value::as_str) == Some("set_property")
          && command.get(1).and_then(serde_json::Value::as_str) == Some("fullscreen")
          && command.get(2).and_then(serde_json::Value::as_bool) == Some(true)
      });
      let loadfile_at = commands.iter().position(|command| {
        command.first().and_then(serde_json::Value::as_str) == Some("loadfile")
      });
      assert!(
        matches!((fullscreen_at, loadfile_at), (Some(fullscreen), Some(load)) if fullscreen < load),
        "fullscreen restore must precede loadfile, got {commands:?}"
      );
    });
  }

  #[test]
  fn play_without_a_previous_end_leaves_fullscreen_untouched() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, mpv) = controller_harness(server).await;

      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");

      let touched_fullscreen = mpv.received_commands().iter().any(|command| {
        command.first().and_then(serde_json::Value::as_str) == Some("set_property")
          && command.get(1).and_then(serde_json::Value::as_str) == Some("fullscreen")
      });
      assert!(
        !touched_fullscreen,
        "a fresh play must not override MPV's own fullscreen default"
      );
    });
  }

  #[test]
  fn transport_controls_return_authoritative_outcomes() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, _mpv) = controller_harness(server).await;
      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");

      let paused = controller
        .set_paused(true)
        .await
        .expect("pause should work");
      let sought = controller.seek(60.0).await.expect("seek should work");
      let volume = controller
        .set_volume(37.0)
        .await
        .expect("volume should work");
      let muted = controller.set_muted(true).await.expect("mute should work");

      assert_eq!(
        (
          paused.snapshot.transport.paused,
          sought.snapshot.transport.time_pos,
          volume.snapshot.transport.volume,
          muted.snapshot.transport.muted,
        ),
        (true, 60.0, 37.0, true)
      );
    });
  }

  #[test]
  fn failed_start_report_becomes_playback_start_warning() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      server.fail_start.store(true, Ordering::Relaxed);
      let (mut controller, _mpv) = controller_harness(server).await;

      let outcome = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("reporting failure must not fail playback");

      assert_eq!(
        outcome.warnings,
        vec![PlaybackWarning::PlaybackStartNotReported]
      );
    });
  }

  #[test]
  fn failed_progress_report_becomes_playback_progress_warning() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, _mpv) = controller_harness(Arc::clone(&server)).await;
      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");
      server.fail_progress.store(true, Ordering::Relaxed);

      let outcome = controller.seek(60.0).await.expect("seek should work");

      assert_eq!(
        outcome.warnings,
        vec![PlaybackWarning::PlaybackProgressNotReported]
      );
    });
  }

  #[test]
  fn failed_stop_report_becomes_playback_stop_warning() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, _mpv) = controller_harness(Arc::clone(&server)).await;
      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");
      server.fail_stop.store(true, Ordering::Relaxed);

      let outcome = controller.stop().await.expect("stop should complete");

      assert_eq!(
        outcome.warnings,
        vec![PlaybackWarning::PlaybackStopNotReported]
      );
    });
  }

  #[test]
  fn track_selection_returns_refreshed_tracks() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, _mpv) = controller_harness(server).await;
      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");

      let audio = controller
        .select_audio_track(2)
        .await
        .expect("audio track should be selected");
      let subtitle = controller
        .select_subtitle_track(Some(3))
        .await
        .expect("subtitle track should be selected");

      assert_eq!(
        (
          audio
            .tracks
            .iter()
            .find(|track| track.id == 2)
            .map(|track| track.selected),
          subtitle
            .tracks
            .iter()
            .find(|track| track.id == 3)
            .map(|track| track.selected),
          audio.warnings,
          subtitle.warnings,
        ),
        (Some(true), Some(true), Vec::new(), Vec::new())
      );
    });
  }

  #[test]
  fn replacement_start_reports_previous_stop_and_maps_failure() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, _mpv) = controller_harness(Arc::clone(&server)).await;
      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("first playback should start");
      server.fail_stop.store(true, Ordering::Relaxed);
      let mut replacement = library_item("Episode");
      replacement.id = "item-2".to_owned();

      let outcome = controller
        .play(replacement.into(), PlaybackStartPosition::Beginning)
        .await
        .expect("replacement playback should start");

      assert_eq!(
        (server.stop_item_ids(), outcome.warnings),
        (
          vec!["item-1".to_owned()],
          vec![PlaybackWarning::PreviousPlaybackStopNotReported],
        )
      );
    });
  }

  #[test]
  fn play_presents_the_player_user_agent_before_loading() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, mpv) = controller_harness(server).await;

      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");

      let commands = mpv.received_commands();
      let user_agent_at = commands.iter().position(|command| {
        command.first().and_then(serde_json::Value::as_str) == Some("set_property")
          && command.get(1).and_then(serde_json::Value::as_str) == Some("user-agent")
          && command.get(2) == Some(&serde_json::json!("mpv"))
      });
      let loadfile_at = commands.iter().position(|command| {
        command.first().and_then(serde_json::Value::as_str) == Some("loadfile")
      });
      assert!(
        matches!((user_agent_at, loadfile_at), (Some(agent), Some(load)) if agent < load),
        "user-agent must be set before loadfile, got {commands:?}"
      );
    });
  }

  #[test]
  fn play_respects_a_user_configured_mpv_user_agent() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let mpv = InMemoryMpv::new().await;
      let mut controller = PlaybackController::from_server(
        server,
        mpv.client.clone(),
        vec!["--user-agent=Custom/1.0".to_owned()],
      );

      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");

      assert!(!mpv.received_commands().iter().any(|command| {
        command.get(1).and_then(serde_json::Value::as_str) == Some("user-agent")
      }));
    });
  }

  #[test]
  fn direct_play_start_carries_no_track_indices() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, _mpv) = controller_harness(Arc::clone(&server)).await;

      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::Beginning,
        )
        .await
        .expect("playback should start");

      assert_eq!(server.start_track_indices(), vec![(None, None)]);
    });
  }

  #[test]
  fn emby_resolution_receives_start_position_ticks() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new().with_provider(MediaServerProvider::Emby));
      let (mut controller, _mpv) = controller_harness(Arc::clone(&server)).await;

      let _ = controller
        .play(
          library_item("Episode").into(),
          PlaybackStartPosition::At(12.5),
        )
        .await
        .expect("playback should start");

      assert_eq!(server.resolution_start_ticks(), vec![Some(125_000_000)]);
    });
  }

  #[test]
  fn item_detail_can_play_gate_prevents_resolution() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let mut controller =
        PlaybackController::from_server(server, MpvClient::new(None), Vec::new());

      let result = controller
        .play(item_detail(false).into(), PlaybackStartPosition::Beginning)
        .await;

      assert!(matches!(result, Err(PlaybackError::ItemNotPlayable)));
    });
  }

  #[test]
  fn media_item_has_no_resume_position() {
    run_async(async {
      let server = Arc::new(MockPlaybackServer::new());
      let (mut controller, _mpv) = controller_harness(server).await;

      let outcome = controller
        .play(media_item("item-1").into(), PlaybackStartPosition::Resume)
        .await
        .expect("media item should start");

      assert_eq!(
        outcome
          .snapshot
          .now_playing
          .map(|item| item.start_position_seconds),
        Some(0.0)
      );
    });
  }
}
