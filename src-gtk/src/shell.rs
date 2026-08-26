use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jellypilot_media_server::{
  Credentials, JellyfinClient, JellyfinError, MediaItem, MediaServerProvider, PlaybackEngineKind,
  QuickConnectStatus, SavedSession, VideoDetailMetadata, VideoHome, VideoItemDetail,
  VideoItemStreams, VideoLibraryItem, VideoLibraryKind, VideoLibraryPageRequest,
  VideoLibraryPlayedFilter, VideoLibraryShortcut, VideoLibrarySort, VideoLibrarySortDirection,
  VideoSearchRequest, VideoSeason, VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest,
  VideoShowDetail, VideoUserDataAction, VideoUserDataUpdate, VideoUserDataUpdateRequest,
};
use jellypilot_mpv::{find_mpv, has_mpv_option, write_input_conf};
use jellypilot_session::{
  evaluate_intro_skip, evaluate_manual_skip, IntroSkipAction, IntroSkipKind, IntroSkipMode,
  IntroSkipRange,
};
use relm4::adw::prelude::*;
use relm4::tokio::sync::{oneshot, watch};
use relm4::{adw, gtk, Component, ComponentParts, ComponentSender, RelmApp};
use zeroize::{Zeroize, Zeroizing};

use crate::artwork::{ArtworkAdapter, DecodedArtwork, FALLBACK_ARTWORK_ICON};
use crate::artwork_cache::ArtworkCacheStats;
use crate::auth_storage::{AuthStore, SavedProfileKey, SavedProfileSummary};
use crate::browse_model::{
  BrowseEffect, BrowseModel, BrowsePagePayload, BrowsePageRequest, BrowsePageSettlement,
  BrowsePreferences, BrowseSource,
};
use crate::config::{self, LoginPrefill};
use crate::diagnostics::{
  sanitize_message, DiagnosticCategory, DiagnosticChange, DiagnosticEvent, DiagnosticLevel,
  Diagnostics, DiagnosticsViewState,
};
use crate::library_browse::LibraryBrowseView;
use crate::playback::{
  PlaybackController, PlaybackControllerConfig, PlaybackEndReason, PlaybackError, PlaybackOptions,
  PlaybackRefreshState, PlaybackSnapshot, PlaybackStartPosition, PlaybackWarning, TrackInfo,
};
use crate::request_gate::{DetailToken, HomeToken, RequestGate, SessionToken};

const APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview";
const SUBTITLE_LANGUAGE_OPTIONS: [&str; 8] =
  ["eng", "spa", "fra", "deu", "ita", "por", "jpn", "zho"];
const SMOKE_APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview.Smoke";
const SEASON_EPISODE_PAGE_SIZE: i32 = 30;
const QUICK_CONNECT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const QUICK_CONNECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const HOME_HERO_HEIGHT: i32 = 340;
const POSTER_FRAME_WIDTH: i32 = 160;
const POSTER_FRAME_HEIGHT: i32 = 240;
const THUMB_FRAME_WIDTH: i32 = 240;
const THUMB_FRAME_HEIGHT: i32 = 135;
const PLAYER_THUMB_SIZE: i32 = 36;
const PLAYBACK_ARTWORK_SLOT: u64 = u64::MAX;

struct AppModel {
  client: Arc<JellyfinClient>,
  auth_store: AuthStore,
  pending_prefill: Option<LoginPrefill>,
  intro_mode: config::IntroMode,
  diagnostics: Diagnostics,
  saved_profiles: LoadState<Vec<SavedProfileSummary>>,
  active_saved_profile: Option<SavedProfileKey>,
  profile_operation_busy: bool,
  quick_connect_phase: QuickConnectPhase,
  quick_connect_cancellation: watch::Sender<u64>,
  artwork: Arc<ArtworkAdapter>,
  artwork_view: u64,
  playback_artwork_view: u64,
  artwork_slot: u64,
  image_cache_sequence: u64,
  image_cache_clearing: bool,
  artwork_targets: HashMap<u64, ArtworkTarget>,
  requests: RequestGate,
  connection: ConnectionPhase,
  home: LoadState<VideoHome>,
  shortcuts: Vec<VideoLibraryShortcut>,
  shortcuts_error: Option<String>,
  browse: BrowseState,
  detail: LoadState<DetailContent>,
  detail_selection: Option<VideoLibraryItem>,
  detail_origin: Option<String>,
  detail_parent: Option<DetailParent>,
  detail_identity: Option<String>,
  streams: LoadState<VideoItemStreams>,
  stream_sequence: u64,
  season_neighbors: LoadState<Vec<VideoLibraryItem>>,
  season_neighbor_sequence: u64,
  season: Option<SeasonSelection>,
  recommendations: LoadState<Vec<VideoLibraryItem>>,
  recommendation_sequence: u64,
  user_data_busy: bool,
  user_data_sequence: u64,
  user_data_error: Option<String>,
  remote_state: RemoteControlState,
  remote_generation: u64,
  remote_play_generation: u64,
  remote_socket: Option<Arc<jellypilot_session::JellyfinWebSocket>>,
  playback: PlaybackState,
  playback_cancellation: watch::Sender<u64>,
  playback_start_generation: u64,
  playback_refresh_source: Option<gtk::glib::SourceId>,
  playback_cleanup_pending: bool,
  remote_disconnect_pending: bool,
  quitting: bool,
  ui: Ui,
}

struct ArtworkTarget {
  picture: gtk::Picture,
  fallback: gtk::Image,
}

struct DiagnosticRowWidgets {
  row: gtk::ListBoxRow,
  message: gtk::Label,
}

#[derive(Default)]
struct PlaybackState {
  controller: Option<PlaybackController>,
  snapshot: Option<PlaybackSnapshot>,
  active_item: Option<MediaItem>,
  active_artwork_image_id: Option<String>,
  identity: Option<PlaybackIdentity>,
  tracks: PlaybackTrackState,
  adjacent: AdjacentState,
  intro_skip: IntroSkipState,
  unavailable: Option<String>,
  error: Option<String>,
  notice: Option<String>,
  busy: bool,
  desired_paused: Option<bool>,
  desired_muted: Option<bool>,
  sequence: u64,
  reconfigure_pending: bool,
  pending: VecDeque<PlaybackRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlaybackIdentity {
  session: u64,
  sequence: u64,
  item_id: String,
}

#[derive(Default)]
enum PlaybackTrackState {
  #[default]
  Unavailable,
  Loading {
    identity: PlaybackIdentity,
  },
  Ready {
    identity: PlaybackIdentity,
    tracks: Vec<TrackInfo>,
  },
  Failed {
    identity: PlaybackIdentity,
    message: String,
  },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdjacentDirection {
  Previous,
  Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackKind {
  Audio,
  Subtitle,
}

#[derive(Clone, Copy)]
enum ShortcutKind {
  Next,
  Previous,
  IntroSkip,
}

#[derive(Default)]
enum AdjacentAvailability {
  #[default]
  Idle,
  Loading,
  Available(MediaItem),
  Unavailable(String),
}

#[derive(Default)]
struct AdjacentState {
  identity: Option<PlaybackIdentity>,
  sequence: u64,
  previous: AdjacentAvailability,
  next: AdjacentAvailability,
}

impl AdjacentState {
  fn availability(&self, direction: AdjacentDirection) -> &AdjacentAvailability {
    match direction {
      AdjacentDirection::Previous => &self.previous,
      AdjacentDirection::Next => &self.next,
    }
  }
}

struct IntroSkipState {
  identity: Option<PlaybackIdentity>,
  sequence: u64,
  mode: IntroSkipMode,
  ranges: Vec<IntroSkipRange>,
  active_prompt: Option<ActiveIntroPrompt>,
}

impl Default for IntroSkipState {
  fn default() -> Self {
    Self {
      identity: None,
      sequence: 0,
      mode: IntroSkipMode::Off,
      ranges: Vec::new(),
      active_prompt: None,
    }
  }
}

struct ActiveIntroPrompt {
  range_index: usize,
  expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum IntroUiAction {
  Seek {
    range_index: usize,
    target: f64,
  },
  Prompt {
    range_index: usize,
    kind: IntroSkipKind,
  },
  ManualSkip {
    range_index: usize,
    kind: IntroSkipKind,
    seek_target: f64,
  },
}

enum PlaybackRequest {
  Library(VideoLibraryItem, PlaybackStartPosition),
  Detail(VideoItemDetail, PlaybackStartPosition),
  ReplaceMedia(MediaItem),
  Paused(bool),
  Seek(f64),
  Volume(f64),
  Muted(bool),
  AudioTrack {
    identity: PlaybackIdentity,
    id: i64,
  },
  SubtitleTrack {
    identity: PlaybackIdentity,
    id: Option<i64>,
  },
  RefreshTracks(PlaybackIdentity),
  ShowText {
    identity: PlaybackIdentity,
    text: String,
    duration_ms: i64,
    prompt_range: Option<usize>,
  },
  Stop,
  Refresh,
}

struct SensitiveCredentials(Credentials);

impl std::ops::Deref for SensitiveCredentials {
  type Target = Credentials;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl Drop for SensitiveCredentials {
  fn drop(&mut self) {
    self.0.password.zeroize();
  }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PlaybackRequestKind {
  Start,
  Paused,
  Seek,
  Volume,
  Muted,
  AudioTrack,
  SubtitleTrack,
  RefreshTracks,
  ShowText,
  Stop,
  Refresh,
}

impl PlaybackRequest {
  const fn kind(&self) -> PlaybackRequestKind {
    match self {
      Self::Library(..) | Self::Detail(..) | Self::ReplaceMedia(..) => PlaybackRequestKind::Start,
      Self::Paused(_) => PlaybackRequestKind::Paused,
      Self::Seek(_) => PlaybackRequestKind::Seek,
      Self::Volume(_) => PlaybackRequestKind::Volume,
      Self::Muted(_) => PlaybackRequestKind::Muted,
      Self::AudioTrack { .. } => PlaybackRequestKind::AudioTrack,
      Self::SubtitleTrack { .. } => PlaybackRequestKind::SubtitleTrack,
      Self::RefreshTracks(_) => PlaybackRequestKind::RefreshTracks,
      Self::ShowText { .. } => PlaybackRequestKind::ShowText,
      Self::Stop => PlaybackRequestKind::Stop,
      Self::Refresh => PlaybackRequestKind::Refresh,
    }
  }

  fn identity(&self) -> Option<&PlaybackIdentity> {
    match self {
      Self::AudioTrack { identity, .. }
      | Self::SubtitleTrack { identity, .. }
      | Self::RefreshTracks(identity)
      | Self::ShowText { identity, .. } => Some(identity),
      _ => None,
    }
  }

  fn started_item(&self) -> Option<MediaItem> {
    match self {
      Self::Library(item, _) => Some(media_item_from_library(item)),
      Self::Detail(item, _) => Some(media_item_from_detail(item)),
      Self::ReplaceMedia(item) => Some(item.clone()),
      _ => None,
    }
  }

  fn started_artwork_image_id(&self) -> Option<String> {
    match self {
      Self::Library(item, _) => item
        .series_poster_image_id
        .clone()
        .or_else(|| item.artwork_image_id.clone()),
      Self::Detail(item, _) => item
        .series_poster_image_id
        .clone()
        .or_else(|| item.artwork_image_id.clone()),
      Self::ReplaceMedia(_) => None,
      _ => None,
    }
  }
}

struct IntroPromptReceipt {
  identity: PlaybackIdentity,
  range_index: usize,
  duration: Duration,
}

struct PlaybackCommandSuccess {
  snapshot: Option<PlaybackSnapshot>,
  preserve_snapshot: bool,
  warnings: Vec<PlaybackWarning>,
  notice: Option<String>,
  tracks: Option<Result<Vec<TrackInfo>, String>>,
  client_messages: Vec<String>,
  prompt_displayed: Option<IntroPromptReceipt>,
}

impl PlaybackCommandSuccess {
  fn playback(
    snapshot: Option<PlaybackSnapshot>,
    warnings: Vec<PlaybackWarning>,
    notice: Option<String>,
  ) -> Self {
    Self {
      snapshot,
      preserve_snapshot: false,
      warnings,
      notice,
      tracks: None,
      client_messages: Vec::new(),
      prompt_displayed: None,
    }
  }

  fn tracks(result: Result<Vec<TrackInfo>, String>) -> Self {
    Self {
      snapshot: None,
      preserve_snapshot: true,
      warnings: Vec::new(),
      notice: None,
      tracks: Some(result),
      client_messages: Vec::new(),
      prompt_displayed: None,
    }
  }
  fn preserved() -> Self {
    Self {
      snapshot: None,
      preserve_snapshot: true,
      warnings: Vec::new(),
      notice: None,
      tracks: None,
      client_messages: Vec::new(),
      prompt_displayed: None,
    }
  }
}

struct PlaybackCommandFailure {
  message: String,
  clear_snapshot: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaybackShutdownDisposition {
  Detached,
  Disconnect,
  Quit,
}

#[derive(Clone, Copy)]
enum ArtworkPresentation {
  Backdrop,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum ConnectionPhase {
  #[default]
  SignedOut,
  Connecting,
  Connected,
  Failed,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum QuickConnectPhase {
  #[default]
  Idle,
  Requesting,
  Waiting,
  Approving,
  Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RemoteControlState {
  Unavailable,
  Connecting,
  Available,
  Lost,
}
fn remote_state_after_event(
  state: RemoteControlState,
  event: &jellypilot_session::JellyfinWebSocketEvent,
) -> RemoteControlState {
  match event {
    jellypilot_session::JellyfinWebSocketEvent::Connected
    | jellypilot_session::JellyfinWebSocketEvent::Reconnected => RemoteControlState::Available,
    jellypilot_session::JellyfinWebSocketEvent::ConnectionLost => RemoteControlState::Lost,
    jellypilot_session::JellyfinWebSocketEvent::Command(_) => state,
  }
}
fn remote_volume_value(value: Option<&serde_json::Value>) -> Option<f64> {
  let volume = match value? {
    serde_json::Value::Number(number) => number.as_f64()?,
    serde_json::Value::String(value) => value.trim().parse().ok()?,
    _ => return None,
  };
  volume.is_finite().then(|| volume.clamp(0.0, 100.0))
}
impl QuickConnectPhase {
  const fn is_active(self) -> bool {
    matches!(self, Self::Requesting | Self::Waiting | Self::Approving)
  }
}

#[derive(Clone, Default)]
enum LoadState<T> {
  #[default]
  Idle,
  Loading,
  Ready(T),
  Failed(String),
}

#[derive(Default)]
struct BrowseState {
  title: String,
  model: BrowseModel,
  error: Option<String>,
  presentation: BrowsePresentation,
  library_shortcut: Option<VideoLibraryShortcut>,
  sort_selection: u32,
  played_selection: u32,
  favorites_only: bool,
}

#[derive(Clone)]
enum DetailContent {
  Item(VideoItemDetail),
  Show(VideoShowDetail),
}

#[derive(Clone)]
struct SeasonSelection {
  season: VideoSeason,
  episodes: LoadState<VideoSeasonEpisodesPage>,
  requested_start_index: i32,
}

#[derive(Clone)]
struct DetailParent {
  content: DetailContent,
  season: Option<SeasonSelection>,
}

#[derive(Clone, Copy, Debug, Default)]
enum BrowsePresentation {
  #[default]
  Grid,
  List,
}

#[derive(Debug)]
enum AppMessage {
  LoadSavedProfiles,
  LoginRequested,
  QuickConnectRequested,
  CancelQuickConnect,
  RestoreSavedProfile(SavedProfileKey),
  ForgetSavedProfile(SavedProfileKey),
  ForgetCurrentProfile,
  ConfirmForgetSavedProfile {
    key: SavedProfileKey,
    sign_out: bool,
  },
  Disconnect,
  ShowHome,
  OpenLibrary(VideoLibraryShortcut),
  SearchRequested,
  SelectItem(VideoLibraryItem),
  SetBrowsePresentation(BrowsePresentation),
  SetBrowseSort(u32),
  SetBrowsePlayedFilter(u32),
  SetBrowseFavoritesOnly(bool),
  LoadPreviousPage,
  LoadNextPage,
  RetryBrowse,
  RetryHome,
  RetryDetail,
  BackFromDetail,
  SelectSeason(VideoSeason),
  PreviousSeasonEpisodePage,
  NextSeasonEpisodePage,
  RetrySeason,
  BackFromSeason,
  UpdateUserData {
    item_id: String,
    action: VideoUserDataAction,
  },
  PlayLibrary(VideoLibraryItem, PlaybackStartPosition),
  PlayDetail(VideoItemDetail, PlaybackStartPosition),
  TogglePaused,
  SetPaused(bool),
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  SelectAudioTrack(i64),
  SelectSubtitleTrack(Option<i64>),
  PlayAdjacent(AdjacentDirection),
  SetIntroMode(u32),
  ReconnectRemoteControl,
  RefreshConnectionStatus,
  DetectMpv,
  SetMpvPath(String),
  SetMpvArgs(String),
  SetPlaybackTargetName(String),
  AddSubtitlePreset,
  AddSubtitleCustom,
  MoveSubtitleLanguage {
    index: usize,
    offset: i32,
  },
  RemoveSubtitleLanguage(usize),
  ClearSubtitleLanguages,
  SetNextEpisodeKey(String),
  SetPreviousEpisodeKey(String),
  SetIntroSkipKey(String),
  SetImageCacheEnabled(bool),
  RefreshImageCacheStats,
  ConfirmClearImageCache,
  ClearImageCache,
  CopyDiagnostics,
  ClearDiagnostics,
  RefreshDiagnostics,
  StopPlayback,
  RefreshPlayback,
  QuitRequested,
  RemoteDisconnectSettled(u64),
}

enum AppCommand {
  SavedProfiles(Result<Vec<SavedProfileSummary>, String>),
  SavedSessionStored {
    session: u64,
    result: Result<(SavedProfileKey, Vec<SavedProfileSummary>), String>,
  },
  Login {
    session: SessionToken,
    client: Arc<JellyfinClient>,
    result: Result<(), String>,
  },
  RemoteReady {
    generation: u64,
    socket: Arc<jellypilot_session::JellyfinWebSocket>,
    receiver: relm4::tokio::sync::mpsc::Receiver<jellypilot_session::JellyfinWebSocketEvent>,
  },
  RemoteEvent {
    generation: u64,
    event: jellypilot_session::JellyfinWebSocketEvent,
  },
  RemoteFailed {
    generation: u64,
  },
  RemotePlay {
    generation: u64,
    playback_generation: u64,
    play_generation: u64,
    start_position: PlaybackStartPosition,
    result: Result<VideoItemDetail, String>,
  },
  ConnectionStatus {
    session: u64,
    result: Result<(), ()>,
  },
  QuickConnectCode {
    session: SessionToken,
    code: String,
  },
  QuickConnectApproving {
    session: SessionToken,
  },
  ForgotProfile {
    session: u64,
    key: SavedProfileKey,
    sign_out: bool,
    result: Result<Vec<SavedProfileSummary>, String>,
  },
  Home {
    token: HomeToken,
    result: (
      Result<VideoHome, String>,
      Result<Vec<VideoLibraryShortcut>, String>,
    ),
  },
  Browse(BrowsePageSettlement),
  Detail {
    token: DetailToken,
    result: Box<Result<DetailContent, String>>,
  },
  Recommendations {
    session: u64,
    sequence: u64,
    item_id: String,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  Streams {
    session: u64,
    sequence: u64,
    item_id: String,
    result: Result<VideoItemStreams, String>,
  },
  SeasonNeighbors {
    session: u64,
    sequence: u64,
    item_id: String,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  SeasonEpisodes {
    token: DetailToken,
    season_id: String,
    result: Result<VideoSeasonEpisodesPage, String>,
  },
  UserData {
    session: u64,
    sequence: u64,
    item_id: String,
    result: Result<VideoUserDataUpdate, String>,
  },
  Artwork {
    session: u64,
    view: u64,
    slot: u64,
    result: Result<DecodedArtwork, ()>,
  },
  ImageCacheStats {
    sequence: u64,
    result: Result<ArtworkCacheStats, ()>,
  },
  ImageCacheCleared {
    sequence: u64,
    result: Result<ArtworkCacheStats, ()>,
  },
  Playback {
    session: u64,
    sequence: u64,
    request_kind: PlaybackRequestKind,
    started_item: Option<MediaItem>,
    started_artwork_image_id: Option<String>,
    controller: Box<PlaybackController>,
    result: Result<PlaybackCommandSuccess, PlaybackCommandFailure>,
  },
  AdjacentEpisodes {
    session: u64,
    sequence: u64,
    identity: PlaybackIdentity,
    previous: Result<Option<MediaItem>, String>,
    next: Result<Option<MediaItem>, String>,
  },
  IntroRanges {
    session: u64,
    sequence: u64,
    identity: PlaybackIdentity,
    ranges: Vec<IntroSkipRange>,
  },
  PlaybackShutdown {
    disposition: PlaybackShutdownDisposition,
    warnings: Vec<PlaybackWarning>,
  },
}

impl std::fmt::Debug for AppCommand {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::SavedProfiles(result) => formatter
        .debug_tuple("SavedProfiles")
        .field(&result.as_ref().map(Vec::len))
        .finish(),
      Self::SavedSessionStored { session, result } => formatter
        .debug_struct("SavedSessionStored")
        .field("session", session)
        .field(
          "profile_count",
          &result.as_ref().map(|(_, profiles)| profiles.len()),
        )
        .finish(),
      Self::Login {
        session, result, ..
      } => formatter
        .debug_struct("Login")
        .field("session", session)
        .field("successful", &result.is_ok())
        .finish(),
      Self::RemoteFailed { generation } => formatter
        .debug_struct("RemoteFailed")
        .field("generation", generation)
        .finish(),
      Self::RemotePlay {
        generation, result, ..
      } => formatter
        .debug_struct("RemotePlay")
        .field("generation", generation)
        .field("successful", &result.is_ok())
        .finish(),
      Self::ConnectionStatus { session, result } => formatter
        .debug_struct("ConnectionStatus")
        .field("session", session)
        .field("successful", &result.is_ok())
        .finish(),
      Self::RemoteReady { generation, .. } => formatter
        .debug_struct("RemoteReady")
        .field("generation", generation)
        .finish(),
      Self::RemoteEvent { generation, event } => formatter
        .debug_struct("RemoteEvent")
        .field("generation", generation)
        .field("event", event)
        .finish(),
      Self::QuickConnectCode { session, .. } => formatter
        .debug_struct("QuickConnectCode")
        .field("session", session)
        .field("code", &"[redacted]")
        .finish(),
      Self::QuickConnectApproving { session } => formatter
        .debug_struct("QuickConnectApproving")
        .field("session", session)
        .finish(),
      Self::ForgotProfile {
        session,
        key,
        sign_out,
        result,
      } => formatter
        .debug_struct("ForgotProfile")
        .field("session", session)
        .field("key", key)
        .field("sign_out", sign_out)
        .field("successful", &result.is_ok())
        .finish(),
      Self::Browse(settlement) => formatter.debug_tuple("Browse").field(settlement).finish(),
      Self::Home { token, result } => formatter
        .debug_struct("Home")
        .field("token", token)
        .field("home_successful", &result.0.is_ok())
        .field("shortcuts_successful", &result.1.is_ok())
        .finish(),
      Self::Detail { token, result } => formatter
        .debug_struct("Detail")
        .field("token", token)
        .field("successful", &result.is_ok())
        .finish(),
      Self::Recommendations {
        session,
        sequence,
        item_id,
        result,
      } => formatter
        .debug_struct("Recommendations")
        .field("session", session)
        .field("sequence", sequence)
        .field("item_id", item_id)
        .field("successful", &result.is_ok())
        .finish(),
      Self::Streams {
        session,
        sequence,
        item_id,
        result,
      } => formatter
        .debug_struct("Streams")
        .field("session", session)
        .field("sequence", sequence)
        .field("item_id", item_id)
        .field("successful", &result.is_ok())
        .finish(),
      Self::SeasonNeighbors {
        session,
        sequence,
        item_id,
        result,
      } => formatter
        .debug_struct("SeasonNeighbors")
        .field("session", session)
        .field("sequence", sequence)
        .field("item_id", item_id)
        .field("successful", &result.is_ok())
        .finish(),
      Self::SeasonEpisodes {
        token,
        season_id,
        result,
      } => formatter
        .debug_struct("SeasonEpisodes")
        .field("token", token)
        .field("season_id", season_id)
        .field("successful", &result.is_ok())
        .finish(),
      Self::UserData {
        session,
        sequence,
        result,
        ..
      } => formatter
        .debug_struct("UserData")
        .field("session", session)
        .field("sequence", sequence)
        .field("successful", &result.is_ok())
        .finish(),
      Self::Artwork {
        session,
        view,
        slot,
        result,
      } => formatter
        .debug_struct("Artwork")
        .field("session", session)
        .field("view", view)
        .field("slot", slot)
        .field("successful", &result.is_ok())
        .finish(),
      Self::ImageCacheStats { sequence, result } => formatter
        .debug_struct("ImageCacheStats")
        .field("sequence", sequence)
        .field("successful", &result.is_ok())
        .finish(),
      Self::ImageCacheCleared { sequence, result } => formatter
        .debug_struct("ImageCacheCleared")
        .field("sequence", sequence)
        .field("successful", &result.is_ok())
        .finish(),
      Self::Playback {
        session,
        sequence,
        result,
        ..
      } => formatter
        .debug_struct("Playback")
        .field("session", session)
        .field("sequence", sequence)
        .field("successful", &result.is_ok())
        .finish(),
      Self::AdjacentEpisodes {
        session,
        sequence,
        previous,
        next,
        ..
      } => formatter
        .debug_struct("AdjacentEpisodes")
        .field("session", session)
        .field("sequence", sequence)
        .field("previous_successful", &previous.is_ok())
        .field("next_successful", &next.is_ok())
        .finish(),
      Self::IntroRanges {
        session,
        sequence,
        ranges,
        ..
      } => formatter
        .debug_struct("IntroRanges")
        .field("session", session)
        .field("sequence", sequence)
        .field("range_count", &ranges.len())
        .finish(),
      Self::PlaybackShutdown {
        disposition,
        warnings,
      } => formatter
        .debug_struct("PlaybackShutdown")
        .field("disposition", disposition)
        .field("warning_count", &warnings.len())
        .finish(),
    }
  }
}

struct Ui {
  toast_overlay: adw::ToastOverlay,
  root: adw::ToolbarView,
  login: gtk::ScrolledWindow,
  provider: adw::ComboRow,
  server_url: adw::EntryRow,
  username: adw::EntryRow,
  password: adw::PasswordEntryRow,
  remember_prefill: gtk::Switch,
  login_method_switcher: gtk::StackSwitcher,
  quick_connect_code: gtk::Label,
  quick_connect_status: gtk::Label,
  quick_connect_spinner: gtk::Spinner,
  quick_connect_button: gtk::Button,
  quick_connect_cancel_button: gtk::Button,
  saved_profiles: gtk::ListBox,
  saved_profiles_status: gtk::Label,
  login_status: gtk::Label,
  login_button: gtk::Button,
  authenticated: adw::NavigationSplitView,
  connection_status: gtk::Label,
  search: gtk::SearchEntry,
  playback_bar: gtk::Box,
  playback_artwork: gtk::Image,
  playback_artwork_fallback: gtk::Image,
  playback_title: gtk::Label,
  playback_subtitle: gtk::Label,
  playback_status_icon: gtk::Image,
  playback_status_label: gtk::Label,
  playback_info: gtk::Stack,
  disconnect_button: gtk::Button,
  content: gtk::Stack,
  nav_home: gtk::ToggleButton,
  shortcuts: gtk::Box,
  home_content: gtk::Box,
  browse_title: gtk::Label,
  browse_status: gtk::Label,
  browse_content: gtk::Box,
  browse_filter_bar: gtk::Box,
  sort_dropdown: gtk::DropDown,
  played_dropdown: gtk::DropDown,
  favorites_only: gtk::CheckButton,
  grid_button: gtk::ToggleButton,
  list_button: gtk::ToggleButton,
  load_previous_button: gtk::Button,
  load_next_button: gtk::Button,
  browse_scroll: gtk::ScrolledWindow,
  detail_content: gtk::Box,
  position_label: gtk::Label,
  duration_label: gtk::Label,
  previous_button: gtk::Button,
  pause_button: gtk::Button,
  next_button: gtk::Button,
  stop_button: gtk::Button,
  seek: gtk::Scale,
  volume: gtk::Scale,
  mute_button: gtk::ToggleButton,
  audio_button: gtk::MenuButton,
  subtitle_button: gtk::MenuButton,
  audio_track_list: gtk::Box,
  subtitle_track_list: gtk::Box,
  playback_controls_syncing: Rc<Cell<bool>>,
  sender: ComponentSender<AppModel>,
  settings_saved_profile: gtk::Label,
  settings_storage_status: gtk::Label,
  settings_disconnect_button: gtk::Button,
  intro_skip_group: adw::PreferencesGroup,
  intro_skip_mode: adw::ComboRow,
  intro_skip_status: gtk::Label,
  settings_config_status: gtk::Label,
  settings_server_url: gtk::Label,
  settings_user: gtk::Label,
  settings_remote_status: gtk::Label,
  settings_reconnect_button: gtk::Button,
  settings_refresh_status_button: gtk::Button,
  settings_mpv_path: adw::EntryRow,
  settings_mpv_status: gtk::Label,
  settings_subtitle_languages: gtk::Box,
  settings_subtitle_preset: gtk::DropDown,
  settings_subtitle_custom: adw::EntryRow,
  settings_image_cache: adw::SwitchRow,
  settings_image_cache_syncing: Rc<Cell<bool>>,
  settings_image_cache_stats: gtk::Label,
  settings_image_cache_clear: gtk::Button,
  diagnostics_list: gtk::ListBox,
  diagnostic_rows: Rc<RefCell<HashMap<u64, DiagnosticRowWidgets>>>,
  diagnostics_empty: gtk::Label,
  diagnostics_count: gtk::Label,
  diagnostics_scroll: gtk::ScrolledWindow,
  diagnostics_copy: gtk::Button,
  diagnostics_clear: gtk::Button,
  diagnostics_status: gtk::Label,
  forget_current_profile: gtk::Button,
  preferences: adw::PreferencesDialog,
}

#[relm4::component]
impl Component for AppModel {
  type Init = bool;
  type Input = AppMessage;
  type Output = ();
  type CommandOutput = AppCommand;
  type Widgets = AppModelWidgets;

  view! {
    #[root]
    main_window = adw::ApplicationWindow {
      set_title: Some("JellyPilot"),
      set_default_size: (1280, 720),
    }
  }
  fn init(
    smoke_test: Self::Init,
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    let ui = Ui::new(&sender);
    root.set_content(Some(&ui.toast_overlay));
    let narrow_breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
      adw::BreakpointConditionLengthType::MaxWidth,
      800.0,
      adw::LengthUnit::Sp,
    ));
    narrow_breakpoint.add_setters(&[(&ui.authenticated, "collapsed", true)]);
    root.add_breakpoint(narrow_breakpoint);
    if smoke_test {
      let application = relm4::main_adw_application();
      root.connect_map(move |_| {
        let application = application.clone();
        gtk::glib::idle_add_local_once(move || application.quit());
      });
    }
    root.connect_close_request({
      let sender = sender.clone();
      move |_| {
        sender.input(AppMessage::QuitRequested);
        gtk::glib::Propagation::Stop
      }
    });
    let playback_refresh_source = gtk::glib::timeout_add_seconds_local(1, {
      let sender = sender.clone();
      move || {
        sender.input(AppMessage::RefreshPlayback);
        gtk::glib::ControlFlow::Continue
      }
    });
    let (playback_cancellation, _) = watch::channel(0);
    let (quick_connect_cancellation, _) = watch::channel(0);
    let loaded_config = config::load_checked();
    let intro_mode = loaded_config
      .as_ref()
      .map_or_else(|_| config::IntroMode::default(), |config| config.intro_mode);
    let image_cache_enabled = loaded_config
      .as_ref()
      .map_or(true, |config| config.image_cache_enabled);
    let artwork = Arc::new(ArtworkAdapter::default());
    artwork.set_disk_cache_enabled(image_cache_enabled);
    let mut diagnostics = Diagnostics::default();
    if loaded_config.is_err() {
      diagnostics.record(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Config,
        "The GTK configuration could not be loaded; safe defaults are in use.",
      );
    }
    let model = Self {
      client: Arc::new(JellyfinClient::new()),
      auth_store: AuthStore::default(),
      pending_prefill: None,
      intro_mode,
      diagnostics,
      saved_profiles: LoadState::Loading,
      active_saved_profile: None,
      profile_operation_busy: false,
      quick_connect_phase: QuickConnectPhase::Idle,
      quick_connect_cancellation,
      artwork,
      artwork_view: 0,
      playback_artwork_view: 0,
      artwork_slot: 0,
      image_cache_sequence: 0,
      image_cache_clearing: false,
      artwork_targets: HashMap::new(),
      requests: RequestGate::default(),
      connection: ConnectionPhase::SignedOut,
      home: LoadState::Idle,
      shortcuts: Vec::new(),
      shortcuts_error: None,
      browse: BrowseState::default(),
      detail: LoadState::Idle,
      detail_selection: None,
      detail_identity: None,
      streams: LoadState::Idle,
      stream_sequence: 0,
      season_neighbors: LoadState::Idle,
      season_neighbor_sequence: 0,
      detail_origin: None,
      detail_parent: None,
      recommendations: LoadState::Idle,
      recommendation_sequence: 0,
      season: None,
      user_data_busy: false,
      user_data_sequence: 0,
      user_data_error: None,
      remote_state: RemoteControlState::Unavailable,
      remote_generation: 0,
      remote_play_generation: 0,
      remote_socket: None,
      playback: PlaybackState::default(),
      playback_cancellation,
      playback_start_generation: 0,
      playback_refresh_source: Some(playback_refresh_source),
      playback_cleanup_pending: false,
      remote_disconnect_pending: false,
      quitting: false,
      ui,
    };
    let widgets = view_output!();
    model.render_connection_settings();
    model.render_subtitle_settings(&sender);
    if !smoke_test {
      sender.input(AppMessage::LoadSavedProfiles);
    }

    ComponentParts { model, widgets }
  }

  fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
    match message {
      AppMessage::LoadSavedProfiles => self.load_saved_profiles(&sender),
      AppMessage::LoginRequested => self.start_login(&sender),
      AppMessage::QuickConnectRequested => self.start_quick_connect(&sender),
      AppMessage::CancelQuickConnect => self.cancel_quick_connect(),
      AppMessage::RestoreSavedProfile(key) => self.start_saved_login(key, &sender),
      AppMessage::ForgetSavedProfile(key) => self.confirm_forget_saved_profile(key, false, &sender),
      AppMessage::ForgetCurrentProfile => {
        if let Some(key) = self.active_saved_profile.clone() {
          self.confirm_forget_saved_profile(key, true, &sender);
        }
      }
      AppMessage::ConfirmForgetSavedProfile { key, sign_out } => {
        self.forget_saved_profile(key, sign_out, &sender)
      }
      AppMessage::Disconnect => {
        if !self.profile_operation_busy {
          self.disconnect(&sender);
        }
      }
      AppMessage::ShowHome => self.show_home(&sender),
      AppMessage::OpenLibrary(shortcut) => self.open_library(shortcut, &sender),
      AppMessage::SearchRequested => self.start_search(&sender),
      AppMessage::SelectItem(item) => self.load_detail(item, &sender),
      AppMessage::SetBrowsePresentation(presentation) => {
        self.browse.presentation = presentation;
        self.render_browse(&sender);
      }
      AppMessage::SetBrowseSort(selection) => {
        self.browse.sort_selection = selection;
        self.apply_browse_preferences(&sender);
      }
      AppMessage::SetBrowsePlayedFilter(selection) => {
        self.browse.played_selection = selection;
        self.apply_browse_preferences(&sender);
      }
      AppMessage::SetBrowseFavoritesOnly(favorites_only) => {
        self.browse.favorites_only = favorites_only;
        self.apply_browse_preferences(&sender);
      }
      AppMessage::LoadPreviousPage => self.load_previous_page(&sender),
      AppMessage::LoadNextPage => self.load_next_page(&sender),
      AppMessage::RetryBrowse => self.retry_browse(&sender),
      AppMessage::RetryHome => self.load_home(&sender),
      AppMessage::RetryDetail => self.retry_detail(&sender),
      AppMessage::BackFromDetail => self.back_from_detail(&sender),
      AppMessage::SelectSeason(season) => self.load_season(season, &sender),
      AppMessage::PreviousSeasonEpisodePage => self.change_season_episode_page(-1, &sender),
      AppMessage::NextSeasonEpisodePage => self.change_season_episode_page(1, &sender),
      AppMessage::RetrySeason => self.retry_season(&sender),
      AppMessage::BackFromSeason => {
        self.requests.navigate();
        self.season = None;
        self.render_detail(&sender);
      }
      AppMessage::UpdateUserData { item_id, action } => {
        self.start_user_data_update(item_id, action, &sender)
      }
      AppMessage::PlayLibrary(item, start_position) => {
        self.start_playback(PlaybackRequest::Library(item, start_position), &sender)
      }
      AppMessage::PlayDetail(item, start_position) => {
        self.start_playback(PlaybackRequest::Detail(item, start_position), &sender)
      }
      AppMessage::TogglePaused => {
        let paused = self
          .playback
          .desired_paused
          .or_else(|| {
            self
              .playback
              .snapshot
              .as_ref()
              .map(|snapshot| snapshot.transport.paused)
          })
          .unwrap_or(false);
        self.start_playback(PlaybackRequest::Paused(!paused), &sender);
      }
      AppMessage::SetPaused(paused) => {
        self.start_playback(PlaybackRequest::Paused(paused), &sender)
      }
      AppMessage::Seek(position) => self.start_playback(PlaybackRequest::Seek(position), &sender),
      AppMessage::SetVolume(volume) => {
        self.start_playback(PlaybackRequest::Volume(volume), &sender)
      }
      AppMessage::SetMuted(muted) => self.start_playback(PlaybackRequest::Muted(muted), &sender),
      AppMessage::SelectAudioTrack(id) => self.select_track(TrackKind::Audio, Some(id), &sender),
      AppMessage::SelectSubtitleTrack(id) => self.select_track(TrackKind::Subtitle, id, &sender),
      AppMessage::PlayAdjacent(direction) => self.play_adjacent(direction, &sender),
      AppMessage::CopyDiagnostics => self.copy_diagnostics(),
      AppMessage::ClearDiagnostics => {
        self.diagnostics.clear();
        self.ui.diagnostics_status.set_label("");
        self.ui.diagnostics_status.set_visible(false);
        self.render_diagnostics();
      }
      AppMessage::RefreshDiagnostics => self.render_diagnostics(),
      AppMessage::SetIntroMode(selected) => self.set_intro_mode(selected),
      AppMessage::ReconnectRemoteControl => self.reconnect_remote_control(&sender),
      AppMessage::RefreshConnectionStatus => self.refresh_connection_status(&sender),
      AppMessage::DetectMpv => self.detect_mpv(),
      AppMessage::SetMpvPath(path) => self.update_mpv_path(path),
      AppMessage::SetMpvArgs(args) => self.update_mpv_args(args),
      AppMessage::SetPlaybackTargetName(name) => self.update_playback_target_name(name),
      AppMessage::AddSubtitlePreset => self.add_subtitle_preset(&sender),
      AppMessage::AddSubtitleCustom => self.add_custom_subtitle(&sender),
      AppMessage::MoveSubtitleLanguage { index, offset } => {
        self.move_subtitle_language(index, offset, &sender)
      }
      AppMessage::RemoveSubtitleLanguage(index) => self.remove_subtitle_language(index, &sender),
      AppMessage::ClearSubtitleLanguages => self.clear_subtitle_languages(&sender),
      AppMessage::SetNextEpisodeKey(key) => self.update_shortcut(ShortcutKind::Next, key),
      AppMessage::SetPreviousEpisodeKey(key) => self.update_shortcut(ShortcutKind::Previous, key),
      AppMessage::SetIntroSkipKey(key) => self.update_shortcut(ShortcutKind::IntroSkip, key),
      AppMessage::SetImageCacheEnabled(enabled) => self.set_image_cache_enabled(enabled),
      AppMessage::RefreshImageCacheStats => self.refresh_image_cache_stats(&sender),
      AppMessage::ConfirmClearImageCache => self.confirm_clear_image_cache(&sender),
      AppMessage::ClearImageCache => self.clear_image_cache(&sender),
      AppMessage::StopPlayback => self.start_playback(PlaybackRequest::Stop, &sender),
      AppMessage::RefreshPlayback => {
        if self
          .playback
          .snapshot
          .as_ref()
          .and_then(|snapshot| snapshot.now_playing.as_ref())
          .is_some()
        {
          self.start_playback(PlaybackRequest::Refresh, &sender);
        }
      }
      AppMessage::QuitRequested => self.request_quit(&sender),
      AppMessage::RemoteDisconnectSettled(generation) => {
        if generation == self.remote_generation && self.remote_disconnect_pending {
          self.remote_disconnect_pending = false;
          if self.quitting
            && quit_can_finish_without_controller(self.playback.busy, self.playback_cleanup_pending)
          {
            relm4::main_adw_application().quit();
          }
        }
      }
    }
  }

  fn update_cmd(
    &mut self,
    command: Self::CommandOutput,
    sender: ComponentSender<Self>,
    _root: &Self::Root,
  ) {
    match command {
      AppCommand::SavedProfiles(result) => {
        self.saved_profiles = match result {
          Ok(profiles) => LoadState::Ready(profiles),
          Err(message) => {
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Auth,
              "Saved profiles could not be loaded from Secret Service.",
            );
            LoadState::Failed(message)
          }
        };
        self.render_saved_profiles(&sender);
        self.render_saved_profile_settings();
      }
      AppCommand::SavedSessionStored { session, result } => {
        self.set_profile_operation_busy(false);
        let is_current = session == self.requests.session_generation()
          && matches!(self.connection, ConnectionPhase::Connected);
        match result {
          Ok((key, profiles)) => {
            if is_current {
              self.active_saved_profile = Some(key);
            }
            self.saved_profiles = LoadState::Ready(profiles);
            if is_current {
              self
                .ui
                .settings_storage_status
                .set_label("This session is stored securely in Linux Secret Service.");
            }
            self.record_diagnostic(
              DiagnosticLevel::Info,
              DiagnosticCategory::Auth,
              "The connected session was stored in Secret Service.",
            );
          }
          Err(message) => {
            if is_current {
              self.active_saved_profile = None;
              self.ui.settings_storage_status.set_label(&message);
            }
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Auth,
              "The connected session could not be stored in Secret Service.",
            );
          }
        }
        self.ui.settings_storage_status.set_visible(is_current);
        self.render_saved_profiles(&sender);
        self.render_saved_profile_settings();
      }
      AppCommand::Login {
        session,
        client,
        result,
      } => self.finish_login(session, client, result, &sender),
      AppCommand::RemoteReady {
        generation,
        socket,
        receiver,
      } => {
        if generation != self.remote_generation {
          return;
        }
        self.remote_state = RemoteControlState::Connecting;
        self.remote_socket = Some(socket);
        self.update_connection_status();
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::RemoteControl,
          "Remote-control capability was granted; opening the command socket.",
        );
        sender.command(move |output, shutdown| {
          shutdown
            .register(async move {
              let mut receiver = receiver;
              while let Some(event) = receiver.recv().await {
                if output
                  .send(AppCommand::RemoteEvent { generation, event })
                  .is_err()
                {
                  break;
                }
              }
            })
            .drop_on_shutdown()
        });
      }
      AppCommand::RemoteFailed { generation } => {
        if generation == self.remote_generation {
          self.remote_state = RemoteControlState::Lost;
          self.update_connection_status();
          self.record_diagnostic(
            DiagnosticLevel::Error,
            DiagnosticCategory::RemoteControl,
            "Remote-control capability or socket setup failed.",
          );
        }
      }
      AppCommand::RemoteEvent { generation, event } => {
        if generation != self.remote_generation {
          return;
        }
        let message = match &event {
          jellypilot_session::JellyfinWebSocketEvent::Connected => {
            Some("Remote-control socket connected.")
          }
          jellypilot_session::JellyfinWebSocketEvent::ConnectionLost => {
            Some("Remote-control socket connection was lost.")
          }
          jellypilot_session::JellyfinWebSocketEvent::Reconnected => {
            Some("Remote-control socket reconnected.")
          }
          jellypilot_session::JellyfinWebSocketEvent::Command(_) => None,
        };
        if let Some(message) = message {
          self.record_diagnostic(
            if matches!(
              event,
              jellypilot_session::JellyfinWebSocketEvent::ConnectionLost
            ) {
              DiagnosticLevel::Warning
            } else {
              DiagnosticLevel::Info
            },
            DiagnosticCategory::RemoteControl,
            message,
          );
        }
        self.remote_state = remote_state_after_event(self.remote_state, &event);
        if let jellypilot_session::JellyfinWebSocketEvent::Command(command) = event {
          self.handle_remote_command(command, &sender);
        }
        self.update_connection_status();
      }
      AppCommand::RemotePlay {
        generation,
        playback_generation,
        play_generation,
        start_position,
        result,
      } => {
        if generation != self.remote_generation
          || playback_generation != self.playback_start_generation
          || play_generation != self.remote_play_generation
        {
          return;
        }
        if let Ok(item) = result {
          self.start_playback(PlaybackRequest::Detail(item, start_position), &sender);
        } else {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::RemoteControl,
            "A remote Play command was rejected because its item could not be loaded.",
          );
          self.update_connection_status();
        }
      }
      AppCommand::ConnectionStatus { session, result } => {
        if session != self.requests.session_generation()
          || !matches!(self.connection, ConnectionPhase::Connected)
        {
          return;
        }
        match result {
          Ok(()) => {
            self.record_diagnostic(
              DiagnosticLevel::Info,
              DiagnosticCategory::Connection,
              "Authenticated connection status refresh succeeded.",
            );
            self
              .ui
              .settings_config_status
              .set_label("Connection status refreshed.");
          }
          Err(()) => {
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Connection,
              "Authenticated connection status refresh failed.",
            );
            self
              .ui
              .settings_config_status
              .set_label("Connection status refresh failed.");
          }
        }
        self.ui.settings_config_status.set_visible(true);
        self.render_connection_settings();
      }
      AppCommand::QuickConnectCode { session, code } => {
        if !self.requests.is_current_login(session) {
          return;
        }
        self.quick_connect_phase = QuickConnectPhase::Waiting;
        self.ui.quick_connect_code.set_label(&code);
        self
          .ui
          .quick_connect_code
          .update_property(&[gtk::accessible::Property::Label(&format!(
            "Quick Connect code: {code}"
          ))]);
        self.ui.quick_connect_code.set_visible(true);
        self
          .ui
          .quick_connect_status
          .set_label("Waiting for approval in another signed-in Jellyfin client…");
        self.ui.quick_connect_spinner.start();
        self.ui.quick_connect_spinner.set_visible(true);
        self.render_quick_connect_controls();
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Auth,
          "Quick Connect code received; waiting for approval.",
        );
      }
      AppCommand::QuickConnectApproving { session } => {
        if !self.requests.is_current_login(session) {
          return;
        }
        self.quick_connect_phase = QuickConnectPhase::Approving;
        self
          .ui
          .quick_connect_status
          .set_label("Approved. Signing in…");
        self.render_quick_connect_controls();
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Auth,
          "Quick Connect was approved; authentication is finishing.",
        );
      }
      AppCommand::ForgotProfile {
        session,
        key,
        sign_out,
        result,
      } => {
        let disconnect_current_session = should_disconnect_after_forget(
          sign_out,
          session,
          self.requests.session_generation(),
          self.connection,
          self.active_saved_profile.as_ref() == Some(&key),
        );
        self.set_profile_operation_busy(false);
        match result {
          Ok(profiles) => {
            if self.active_saved_profile.as_ref() == Some(&key) {
              self.active_saved_profile = None;
            }
            self.saved_profiles = LoadState::Ready(profiles);
            self
              .ui
              .saved_profiles_status
              .set_label("Saved sign-in forgotten.");
            self.ui.saved_profiles_status.set_visible(true);
            self.render_saved_profiles(&sender);
            self.render_saved_profile_settings();
            if disconnect_current_session {
              self.disconnect(&sender);
            }
            self.record_diagnostic(
              DiagnosticLevel::Info,
              DiagnosticCategory::Auth,
              "Saved profile removal completed.",
            );
          }
          Err(message) => {
            self.ui.saved_profiles_status.set_label(&message);
            self.ui.saved_profiles_status.set_visible(true);
            self.render_saved_profile_settings();
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Auth,
              "Saved profile removal failed in Secret Service.",
            );
          }
        }
      }
      AppCommand::Home { token, result } => self.finish_home(token, result, &sender),
      AppCommand::Browse(settlement) => {
        match self.browse.model.settle(settlement) {
          Ok(effects) => {
            self.browse.error = None;
            self.execute_browse_effects(effects, &sender);
          }
          Err(error) => self.browse.error = Some(error.to_string()),
        }
        // Keep background page settlements from invalidating artwork targets owned by a
        // different visible page (for example, an item detail opened while browse was loading).
        if self.ui.content.visible_child_name().as_deref() == Some("browse") {
          self.render_browse(&sender);
        }
      }
      AppCommand::Detail { token, result } => {
        if !self.requests.finish_detail(token) {
          return;
        }
        self.detail = match *result {
          Ok(detail) => LoadState::Ready(detail),
          Err(message) => LoadState::Failed(message),
        };
        if self.ui.content.visible_child_name().as_deref() == Some("detail") {
          self.render_detail(&sender);
        }
      }
      AppCommand::Recommendations {
        session,
        sequence,
        item_id,
        result,
      } => {
        if session != self.requests.session_generation()
          || sequence != self.recommendation_sequence
          || self.detail_identity.as_deref() != Some(item_id.as_str())
        {
          return;
        }
        self.recommendations = match result {
          Ok(items) => LoadState::Ready(items),
          Err(message) => LoadState::Failed(message),
        };
        if self.ui.content.visible_child_name().as_deref() == Some("detail") {
          self.render_detail(&sender);
        }
      }
      AppCommand::Streams {
        session,
        sequence,
        item_id,
        result,
      } => {
        if session != self.requests.session_generation()
          || sequence != self.stream_sequence
          || self.detail_identity.as_deref() != Some(item_id.as_str())
        {
          return;
        }
        self.streams = match result {
          Ok(streams) => LoadState::Ready(streams),
          Err(message) => LoadState::Failed(message),
        };
        if self.ui.content.visible_child_name().as_deref() == Some("detail") {
          self.render_detail(&sender);
        }
      }
      AppCommand::SeasonNeighbors {
        session,
        sequence,
        item_id,
        result,
      } => {
        if session != self.requests.session_generation()
          || sequence != self.season_neighbor_sequence
          || self.detail_identity.as_deref() != Some(item_id.as_str())
        {
          return;
        }
        self.season_neighbors = match result {
          Ok(items) => LoadState::Ready(items),
          Err(message) => LoadState::Failed(message),
        };
        if self.ui.content.visible_child_name().as_deref() == Some("detail") {
          self.render_detail(&sender);
        }
      }
      AppCommand::SeasonEpisodes {
        token,
        season_id,
        result,
      } => {
        if !self.requests.finish_detail(token) {
          return;
        }
        let Some(selection) = self
          .season
          .as_mut()
          .filter(|selection| selection.season.id == season_id)
        else {
          return;
        };
        selection.episodes = match result {
          Ok(episodes) => LoadState::Ready(episodes),
          Err(message) => LoadState::Failed(message),
        };
        if self.ui.content.visible_child_name().as_deref() == Some("detail") {
          self.render_detail(&sender);
        }
      }
      AppCommand::UserData {
        session,
        sequence,
        item_id,
        result,
      } => self.finish_user_data_update(session, sequence, &item_id, result, &sender),
      AppCommand::Artwork {
        session,
        view,
        slot,
        result,
      } => {
        if session != self.requests.session_generation() {
          return;
        }
        let playback_thumb = slot == PLAYBACK_ARTWORK_SLOT;
        if playback_thumb {
          if view != self.playback_artwork_view {
            return;
          }
        } else if view != self.artwork_view {
          return;
        }
        match result.and_then(|decoded| decoded.texture().map_err(|_| ())) {
          Ok(decoded) => {
            if playback_thumb {
              self.ui.playback_artwork.set_paintable(Some(&decoded));
              self.ui.playback_artwork_fallback.set_visible(false);
            } else if let Some(target) = self.artwork_targets.remove(&slot) {
              target.picture.set_paintable(Some(&decoded));
              target.fallback.set_visible(false);
            }
            self.diagnostics.reset_coalescing();
          }
          Err(()) => self.record_artwork_failure(),
        }
      }
      AppCommand::ImageCacheStats { sequence, result } => {
        if sequence != self.image_cache_sequence || self.image_cache_clearing {
          return;
        }
        match result {
          Ok(stats) => self.render_image_cache_stats(stats),
          Err(()) => {
            self
              .ui
              .settings_image_cache_stats
              .set_label("Cache statistics are unavailable.");
            self.ui.settings_image_cache_clear.set_sensitive(false);
          }
        }
      }
      AppCommand::ImageCacheCleared { sequence, result } => {
        if sequence != self.image_cache_sequence {
          return;
        }
        self.image_cache_clearing = false;
        match result {
          Ok(stats) => {
            self.render_image_cache_stats(stats);
            self.record_diagnostic(
              DiagnosticLevel::Info,
              DiagnosticCategory::Artwork,
              "Library Image Cache cleared.",
            );
          }
          Err(()) => {
            self
              .ui
              .settings_image_cache_stats
              .set_label("Library Image Cache could not be cleared.");
            self.ui.settings_image_cache_clear.set_sensitive(true);
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Artwork,
              "Library Image Cache clear failed.",
            );
          }
        }
      }
      AppCommand::Playback {
        session,
        sequence,
        request_kind,
        started_item,
        started_artwork_image_id,
        controller,
        result,
      } => {
        let mut controller = *controller;
        if session != self.requests.session_generation()
          || sequence != self.playback.sequence
          || self.quitting
        {
          let disposition = stale_playback_disposition(
            self.quitting,
            self.connection,
            self.playback_cleanup_pending,
          );
          self.shutdown_playback(controller, disposition, &sender);
          return;
        }
        if self.playback.reconfigure_pending {
          self.playback.reconfigure_pending = false;
          if controller
            .configure_for_next_start(playback_controller_config(&config::load()))
            .is_err()
          {
            self.show_settings_failure(
              "Settings were saved, but no MPV executable is available for the next start.",
            );
          }
        }
        self.playback.controller = Some(controller);
        self.playback.busy = false;
        let mut refresh_auxiliary = None;
        let mut refresh_artwork = false;
        let mut intro_action = None;
        let mut shortcut_adjacent = None;
        let mut playback_started_title = None;
        match result {
          Ok(success) => {
            let PlaybackCommandSuccess {
              snapshot,
              preserve_snapshot,
              warnings,
              notice,
              tracks,
              client_messages,
              prompt_displayed,
            } = success;
            if !warnings.is_empty() && request_kind != PlaybackRequestKind::Refresh {
              self.record_diagnostic(
                DiagnosticLevel::Warning,
                DiagnosticCategory::Playback,
                "Playback completed with one or more non-fatal reporting warnings.",
              );
            }
            if !preserve_snapshot {
              self.playback.snapshot = snapshot;
            }
            if let Some(snapshot) = self.playback.snapshot.as_ref() {
              self.playback.desired_paused = Some(snapshot.transport.paused);
              self.playback.desired_muted = Some(snapshot.transport.muted);
            } else {
              self.playback.desired_paused = None;
              self.playback.desired_muted = None;
            }
            self.playback.error = None;
            self.playback.notice = playback_notice(notice, &warnings);
            if request_kind == PlaybackRequestKind::Start {
              if let (Some(item), Some(now_playing)) = (
                started_item,
                self
                  .playback
                  .snapshot
                  .as_ref()
                  .and_then(|snapshot| snapshot.now_playing.as_ref()),
              ) {
                if item.id == now_playing.item_id {
                  let identity = PlaybackIdentity {
                    session,
                    sequence,
                    item_id: item.id.clone(),
                  };
                  playback_started_title = Some(item.name.clone());
                  self.playback.active_item = Some(item);
                  if started_artwork_image_id.is_some() {
                    self.playback.active_artwork_image_id = started_artwork_image_id;
                    refresh_artwork = true;
                  }
                  self.playback.identity = Some(identity.clone());
                  self.playback.tracks = PlaybackTrackState::Loading {
                    identity: identity.clone(),
                  };
                  self.playback.intro_skip = IntroSkipState {
                    identity: Some(identity.clone()),
                    sequence: 0,
                    mode: session_intro_mode(self.intro_mode),
                    ranges: Vec::new(),
                    active_prompt: None,
                  };
                  refresh_auxiliary = Some(identity);
                }
              }
            } else if matches!(
              request_kind,
              PlaybackRequestKind::Stop | PlaybackRequestKind::Refresh
            ) && self.playback.snapshot.is_none()
            {
              self.clear_playback_context();
              refresh_artwork = true;
            }
            if let Some(result) = tracks {
              if matches!(
                request_kind,
                PlaybackRequestKind::AudioTrack | PlaybackRequestKind::SubtitleTrack
              ) {
                self.record_diagnostic(
                  if result.is_ok() {
                    DiagnosticLevel::Info
                  } else {
                    DiagnosticLevel::Warning
                  },
                  DiagnosticCategory::Playback,
                  match (request_kind, result.is_ok()) {
                    (PlaybackRequestKind::AudioTrack, true) => {
                      "MPV audio track selection completed."
                    }
                    (PlaybackRequestKind::AudioTrack, false) => "MPV audio track selection failed.",
                    (_, true) => "MPV subtitle track selection completed.",
                    (_, false) => "MPV subtitle track selection failed.",
                  },
                );
              }
              self.finish_track_refresh(result);
            }
            if let Some(prompt) = prompt_displayed {
              let current = auxiliary_settlement_is_current(
                prompt.identity.session,
                &prompt.identity,
                self.requests.session_generation(),
                self.playback.identity.as_ref(),
              );
              let prompt_range_valid = self
                .playback
                .intro_skip
                .ranges
                .get(prompt.range_index)
                .is_some_and(|range| range.notified && !range.skipped);
              if current
                && self.playback.intro_skip.mode == IntroSkipMode::Manual
                && prompt_range_valid
              {
                self.playback.intro_skip.active_prompt = Some(ActiveIntroPrompt {
                  range_index: prompt.range_index,
                  expires_at: Instant::now() + prompt.duration,
                });
              }
            }
            if request_kind == PlaybackRequestKind::Refresh {
              let position = self
                .playback
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.transport.time_pos);
              if let (Some(direction), Some(identity)) = (
                adjacent_direction_from_client_messages(&client_messages),
                self.playback.identity.clone(),
              ) {
                if auxiliary_settlement_is_current(
                  identity.session,
                  &identity,
                  self.requests.session_generation(),
                  self.playback.identity.as_ref(),
                ) {
                  shortcut_adjacent = Some((identity, direction));
                }
              }
              if shortcut_adjacent.is_none() {
                let manual_requested = manual_intro_skip_requested(&client_messages);
                let active_prompt_range = active_intro_prompt_range(
                  &mut self.playback.intro_skip.active_prompt,
                  Instant::now(),
                );
                if let (Some(position), Some(identity)) = (position, self.playback.identity.clone())
                {
                  if self.playback.intro_skip.identity.as_ref() == Some(&identity) {
                    intro_action = evaluate_intro_ui_action(
                      position,
                      &mut self.playback.intro_skip.ranges,
                      self.playback.intro_skip.mode,
                      manual_requested,
                      active_prompt_range,
                    )
                    .map(|action| (identity, action));
                  }
                }
              }
            }
            if let Some(title) = playback_started_title.take() {
              self.record_diagnostic(
                DiagnosticLevel::Info,
                DiagnosticCategory::Playback,
                format!("Playback started for “{title}”."),
              );
            } else if request_kind == PlaybackRequestKind::Stop {
              self.record_diagnostic(
                DiagnosticLevel::Info,
                DiagnosticCategory::Playback,
                "Playback stopped.",
              );
            } else if request_kind == PlaybackRequestKind::Refresh
              && self.playback.snapshot.is_none()
            {
              self.record_diagnostic(
                if self
                  .playback
                  .notice
                  .as_deref()
                  .is_some_and(|notice| notice.contains("disconnected"))
                {
                  DiagnosticLevel::Warning
                } else {
                  DiagnosticLevel::Info
                },
                DiagnosticCategory::Playback,
                "Playback session ended.",
              );
            }
            if refresh_artwork {
              self.queue_playback_artwork(&sender);
            }
            if matches!(
              request_kind,
              PlaybackRequestKind::AudioTrack | PlaybackRequestKind::SubtitleTrack
            ) {
              self.add_toast(match request_kind {
                PlaybackRequestKind::AudioTrack => "Audio track switched.",
                _ => "Subtitle track switched.",
              });
            }
            self.render_playback_bar();
          }
          Err(failure) => {
            self.record_diagnostic(
              DiagnosticLevel::Error,
              DiagnosticCategory::Playback,
              &failure.message,
            );
            if failure.clear_snapshot {
              self.playback.snapshot = None;
              self.clear_playback_context();
              self.queue_playback_artwork(&sender);
            }
            if let Some(snapshot) = self.playback.snapshot.as_ref() {
              self.playback.desired_paused = Some(snapshot.transport.paused);
              self.playback.desired_muted = Some(snapshot.transport.muted);
            } else {
              self.playback.desired_paused = None;
              self.playback.desired_muted = None;
            }
            self.add_toast(&failure.message);
            self.playback.error = Some(failure.message);
            self.render_playback_bar();
          }
        }
        if let Some(identity) = refresh_auxiliary {
          self.refresh_adjacent_episodes(identity.clone(), &sender);
          self.refresh_intro_ranges(identity.clone(), &sender);
          self.queue_playback_request(PlaybackRequest::RefreshTracks(identity));
        }
        if let Some((identity, direction)) = shortcut_adjacent {
          if auxiliary_settlement_is_current(
            identity.session,
            &identity,
            self.requests.session_generation(),
            self.playback.identity.as_ref(),
          ) {
            self.play_adjacent(direction, &sender);
          }
        } else if let Some((identity, action)) = intro_action {
          self.apply_intro_action(identity, action, &sender);
        }
        if let Some(request) = self.playback.pending.pop_front() {
          self.start_playback(request, &sender);
        }
      }
      AppCommand::AdjacentEpisodes {
        session,
        sequence,
        identity,
        previous,
        next,
      } => {
        if !auxiliary_settlement_is_current(
          session,
          &identity,
          self.requests.session_generation(),
          self.playback.identity.as_ref(),
        ) || sequence != self.playback.adjacent.sequence
        {
          return;
        }
        if previous.is_err() || next.is_err() {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::Playback,
            "The server could not resolve one or more adjacent episodes.",
          );
        }
        self.playback.adjacent.previous =
          adjacent_availability(AdjacentDirection::Previous, previous);
        self.playback.adjacent.next = adjacent_availability(AdjacentDirection::Next, next);
        self.render_playback_bar();
      }
      AppCommand::IntroRanges {
        session,
        sequence,
        identity,
        ranges,
      } => {
        if !auxiliary_settlement_is_current(
          session,
          &identity,
          self.requests.session_generation(),
          self.playback.identity.as_ref(),
        ) || sequence != self.playback.intro_skip.sequence
        {
          return;
        }
        self.playback.intro_skip.ranges = ranges;
      }
      AppCommand::PlaybackShutdown {
        disposition,
        warnings,
      } => {
        self.playback.busy = false;
        self.playback_cleanup_pending = false;
        if shutdown_completion_quits(self.quitting, disposition) {
          if self.remote_disconnect_pending {
            self.quitting = true;
            return;
          }
          relm4::main_adw_application().quit();
          return;
        }
        if matches!(disposition, PlaybackShutdownDisposition::Disconnect) && !warnings.is_empty() {
          self.ui.login_status.set_label(
            "Disconnected. Playback stopped, but its final server progress could not be updated.",
          );
          self.ui.login_status.set_visible(true);
        }
        if matches!(disposition, PlaybackShutdownDisposition::Disconnect) {
          self.playback.busy = false;
          self.ui.login_button.set_sensitive(true);
          if warnings.is_empty() {
            self.ui.login_status.set_label("Disconnected.");
            self.ui.login_status.set_visible(true);
          }
        }
      }
    }
  }

  fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
    self.artwork.reset_session();
    self.stop_remote_session(None);
    self.cancel_inflight_quick_connect();
    self.cancel_inflight_playback();
    if let Some(source) = self.playback_refresh_source.take() {
      source.remove();
    }
    if let Some(mut controller) = self.playback.controller.take() {
      relm4::spawn(async move {
        let _ = controller.shutdown().await;
      });
    }
  }
}

impl AppModel {
  fn load_saved_profiles(&mut self, sender: &ComponentSender<Self>) {
    self.saved_profiles = LoadState::Loading;
    self.render_saved_profiles(sender);
    let store = self.auth_store.clone();
    sender.oneshot_command(async move {
      let result = run_auth_operation(move || store.load_profiles())
        .await
        .map_err(|_| "Secure saved sign-ins could not be loaded.".to_string())
        .and_then(|result| result.map_err(|error| format!("Saved sign-ins unavailable: {error}.")));
      AppCommand::SavedProfiles(result)
    });
  }

  fn handle_remote_command(
    &mut self,
    command: jellypilot_session::JellyfinCommand,
    sender: &ComponentSender<Self>,
  ) {
    match command {
      jellypilot_session::JellyfinCommand::Playstate(request) => match request.command.as_str() {
        "Pause" => sender.input(AppMessage::SetPaused(true)),
        "Unpause" => sender.input(AppMessage::SetPaused(false)),
        "Seek" => {
          if let Some(ticks) = request.seek_position_ticks {
            sender.input(AppMessage::Seek(ticks as f64 / 10_000_000.0));
          } else {
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::RemoteControl,
              "A remote Seek command was rejected because it had no position.",
            );
          }
        }
        "Stop" => sender.input(AppMessage::StopPlayback),
        _ => self.record_diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::RemoteControl,
          "An unsupported remote playstate command was rejected.",
        ),
      },
      jellypilot_session::JellyfinCommand::GeneralCommand(request) => match request.name.as_str() {
        "SetVolume" => {
          if let Some(volume) = remote_volume_value(
            request
              .arguments
              .as_ref()
              .and_then(|args| args.get("Volume")),
          ) {
            sender.input(AppMessage::SetVolume(volume));
          } else {
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::RemoteControl,
              "A remote volume command was rejected because its value was invalid.",
            );
          }
        }
        "ToggleMute" => {
          let muted = self
            .playback
            .desired_muted
            .or_else(|| {
              self
                .playback
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.transport.muted)
            })
            .unwrap_or(false);
          sender.input(AppMessage::SetMuted(!muted));
        }
        _ => self.record_diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::RemoteControl,
          "An unsupported remote general command was rejected.",
        ),
      },
      jellypilot_session::JellyfinCommand::Play(request) => {
        if !matches!(
          request.play_command.as_str(),
          "PlayNow" | "PlayInstant" | ""
        ) {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::RemoteControl,
            "A remote Play command with an unsupported mode was rejected.",
          );
          return;
        }
        let Some(item_id) = request.item_ids.first().cloned() else {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::RemoteControl,
            "A remote Play command without an item was rejected.",
          );
          return;
        };
        self.remote_play_generation = self.remote_play_generation.wrapping_add(1);
        let play_generation = self.remote_play_generation;
        let playback_generation = self.playback_start_generation;
        let start_position = request
          .start_position_ticks
          .filter(|ticks| *ticks > 0)
          .map(|ticks| PlaybackStartPosition::At(ticks as f64 / 10_000_000.0))
          .unwrap_or(PlaybackStartPosition::Beginning);
        let client = Arc::clone(&self.client);
        let generation = self.remote_generation;
        sender.oneshot_command(async move {
          let result = client
            .library()
            .item_detail(item_id)
            .await
            .map_err(|error| error.to_string());
          AppCommand::RemotePlay {
            generation,
            playback_generation,
            play_generation,
            start_position,
            result,
          }
        });
      }
    }
  }

  fn start_remote_session(&mut self, sender: &ComponentSender<Self>) {
    self.remote_generation = self.remote_generation.wrapping_add(1);
    let generation = self.remote_generation;
    self.remote_state = if self.client.supports_remote_control() {
      RemoteControlState::Connecting
    } else {
      RemoteControlState::Unavailable
    };
    if !self.client.supports_remote_control() {
      self.record_diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::RemoteControl,
        "The server denied remote-control capability.",
      );
      return;
    }
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::RemoteControl,
      "Requesting remote-control capability.",
    );
    let client = Arc::clone(&self.client);
    let socket = Arc::new(jellypilot_session::JellyfinWebSocket::new());
    self.remote_socket = Some(Arc::clone(&socket));
    sender.oneshot_command(async move {
      let result = async {
        client.playback().validate_session().await.map_err(|_| ())?;
        client
          .playback()
          .report_capabilities_for_checked(PlaybackEngineKind::ExternalMpv)
          .await
          .map_err(|_| ())?;
        if !client.supports_remote_control() {
          return Err(());
        }
        let url = client.playback().websocket_url().map_err(|_| ())?;
        let user_agent = client.playback().websocket_user_agent();
        socket
          .connect_with_user_agent(&url, Some(&user_agent))
          .await
          .map_err(|_| ())?;
        let receiver = socket.take_event_receiver().ok_or(())?;
        Ok::<_, ()>(receiver)
      }
      .await;
      match result {
        Ok(receiver) => AppCommand::RemoteReady {
          generation,
          socket,
          receiver,
        },
        Err(()) => AppCommand::RemoteFailed { generation },
      }
    });
  }

  fn stop_remote_session(&mut self, quit_gate: Option<&ComponentSender<Self>>) {
    self.remote_generation = self.remote_generation.wrapping_add(1);
    self.remote_state = RemoteControlState::Unavailable;
    if let Some(socket) = self.remote_socket.take() {
      if let Some(sender) = quit_gate {
        self.remote_disconnect_pending = true;
        let generation = self.remote_generation;
        let settle_sender = sender.clone();
        relm4::spawn(async move {
          socket.disconnect().await;
          settle_sender.input(AppMessage::RemoteDisconnectSettled(generation));
        });
        let timeout_sender = sender.clone();
        gtk::glib::timeout_add_local_once(Duration::from_secs(2), move || {
          timeout_sender.input(AppMessage::RemoteDisconnectSettled(generation));
        });
      } else {
        relm4::spawn(async move {
          socket.disconnect().await;
        });
      }
    }
  }
  fn start_login(&mut self, sender: &ComponentSender<Self>) {
    if self.profile_operation_busy
      || !can_start_login(self.connection, self.playback_cleanup_pending)
    {
      return;
    }
    let server_url = self.ui.server_url.text().trim().to_owned();
    let username = self.ui.username.text().trim().to_owned();

    let password = self.ui.password.text().to_string();
    if server_url.is_empty() || username.is_empty() {
      self.connection = ConnectionPhase::Failed;
      self
        .ui
        .login_status
        .set_label("Enter a server URL and username to continue.");
      self.ui.login_status.set_visible(true);
      self.record_diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Auth,
        "Password sign-in was rejected because the server URL or username is empty.",
      );
      return;
    }
    let mut pending_prefill = config::load();
    pending_prefill.remember = self.ui.remember_prefill.is_active();
    pending_prefill.server_url = server_url.clone();
    pending_prefill.provider = if self.ui.provider.selected() == 1 {
      "emby".to_owned()
    } else {
      "jellyfin".to_owned()
    };
    pending_prefill.username = username.clone();
    self.pending_prefill = Some(pending_prefill);

    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.ui.quick_connect_code.set_label("");
    self.ui.quick_connect_code.set_visible(false);
    self.ui.quick_connect_status.set_label("");
    self.ui.quick_connect_spinner.stop();
    self.ui.quick_connect_spinner.set_visible(false);
    self.ui.password.set_text("");
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Connection,
      "Connecting to the selected media server.",
    );
    let session = self.prepare_login("Connecting and loading your libraries…");
    let credentials = SensitiveCredentials(Credentials {
      provider: provider_for(self.ui.provider.selected()),
      server_url,
      username,
      password,
    });
    // Authenticate an isolated candidate so a superseded login cannot mutate the active session.
    let client = configured_client(&config::load());
    let command_client = Arc::clone(&client);
    sender.oneshot_command(async move {
      let result = async {
        command_client
          .login()
          .authenticate(&credentials)
          .await
          .map_err(|error| error.to_string())?;
        Ok(())
      }
      .await;
      AppCommand::Login {
        session,
        client,
        result,
      }
    });
  }

  fn start_quick_connect(&mut self, sender: &ComponentSender<Self>) {
    if self.profile_operation_busy
      || !can_start_login(self.connection, self.playback_cleanup_pending)
    {
      return;
    }
    if !quick_connect_available(provider_for(self.ui.provider.selected())) {
      self.quick_connect_phase = QuickConnectPhase::Failed;
      self
        .ui
        .quick_connect_status
        .set_label("Quick Connect is available only for Jellyfin. Sign in with a password.");
      self.render_quick_connect_controls();
      self.record_diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Auth,
        "Quick Connect was rejected because the selected server type does not support it.",
      );
      return;
    }
    let server_url = self.ui.server_url.text().trim().to_owned();
    if server_url.is_empty() {
      self.quick_connect_phase = QuickConnectPhase::Failed;
      self
        .ui
        .quick_connect_status
        .set_label("Enter a Jellyfin server URL to request a code.");
      self.render_quick_connect_controls();
      self.record_diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Auth,
        "Quick Connect was rejected because the server URL is empty.",
      );
      return;
    }
    self.pending_prefill = None;

    self.cancel_inflight_quick_connect();
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Auth,
      "Quick Connect request started.",
    );
    let session = self.prepare_login("Requesting a Quick Connect code…");
    self.quick_connect_phase = QuickConnectPhase::Requesting;
    self.ui.login_status.set_label("");
    self.ui.login_status.set_visible(false);
    self.ui.quick_connect_code.set_label("");
    self.ui.quick_connect_code.set_visible(false);
    self.ui.quick_connect_status.set_label("Requesting a code…");
    self.ui.quick_connect_spinner.start();
    self.ui.quick_connect_spinner.set_visible(true);
    self.render_quick_connect_controls();

    let client = configured_client(&config::load());
    let command_client = Arc::clone(&client);
    let mut cancellation = self.quick_connect_cancellation.subscribe();
    sender.command(move |output, shutdown| {
      shutdown
        .register(async move {
          let operation = quick_connect_workflow(
            command_client,
            server_url,
            session,
            output,
            QUICK_CONNECT_POLL_INTERVAL,
            QUICK_CONNECT_TIMEOUT,
          );
          relm4::tokio::pin!(operation);
          relm4::tokio::select! {
            () = &mut operation => {}
            changed = cancellation.changed() => {
              let _ = changed;
            }
          }
        })
        .drop_on_shutdown()
    });
  }

  fn cancel_quick_connect(&mut self) {
    if !self.quick_connect_phase.is_active() {
      return;
    }
    self.cancel_inflight_quick_connect();
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Auth,
      "Quick Connect request was cancelled.",
    );
    self.requests.disconnect();
    self.connection = ConnectionPhase::SignedOut;
    self.home = LoadState::Idle;
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.set_login_controls_sensitive(true);
    self.ui.login_status.set_label("");
    self.ui.login_status.set_visible(false);
    self.ui.quick_connect_status.set_label("");
    self.ui.quick_connect_code.set_label("");
    self.ui.quick_connect_code.set_visible(false);
    self.ui.quick_connect_spinner.stop();
    self.ui.quick_connect_spinner.set_visible(false);
    self.render_quick_connect_controls();
  }

  fn start_saved_login(&mut self, key: SavedProfileKey, sender: &ComponentSender<Self>) {
    if self.profile_operation_busy
      || !can_start_login(self.connection, self.playback_cleanup_pending)
    {
      return;
    }
    let Some(profile) = self
      .saved_profile_summaries()
      .iter()
      .find(|profile| profile.key == key)
      .cloned()
    else {
      self
        .ui
        .login_status
        .set_label("That saved sign-in is no longer available.");
      self.ui.login_status.set_visible(true);
      self.record_diagnostic(
        DiagnosticLevel::Warning,
        DiagnosticCategory::Auth,
        "Saved profile restore was rejected because the profile is no longer available.",
      );
      return;
    };
    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.ui.quick_connect_code.set_label("");
    self.ui.quick_connect_code.set_visible(false);
    self.ui.quick_connect_status.set_label("");
    self.ui.quick_connect_spinner.stop();
    self.ui.quick_connect_spinner.set_visible(false);
    self.ui.provider.set_selected(match profile.provider {
      MediaServerProvider::Jellyfin => 0,
      MediaServerProvider::Emby => 1,
    });
    self.ui.server_url.set_text(&profile.server_url);
    self.ui.username.set_text(&profile.user_name);
    self.ui.password.set_text("");
    self.pending_prefill = None;

    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Auth,
      "Saved profile restore started.",
    );
    let session = self.prepare_login("Restoring the saved sign-in…");
    let store = self.auth_store.clone();
    let client = configured_client(&config::load());
    let command_client = Arc::clone(&client);
    let requested_key = key.clone();
    sender.oneshot_command(async move {
      let result = async {
        let stored_session = run_auth_operation(move || store.load_session(&requested_key))
          .await
          .map_err(|_| "The saved sign-in could not be read.".to_string())?
          .map_err(|error| format!("Saved sign-in unavailable: {error}."))?;
        command_client
          .login()
          .restore_session(&stored_session)
          .await
          .map_err(|error| format!("Saved sign-in could not be restored: {error}"))?;
        Ok(())
      }
      .await;
      AppCommand::Login {
        session,
        client,
        result,
      }
    });
  }

  fn forget_saved_profile(
    &mut self,
    key: SavedProfileKey,
    sign_out: bool,
    sender: &ComponentSender<Self>,
  ) {
    if self.profile_operation_busy {
      return;
    }
    self.set_profile_operation_busy(true);
    self
      .ui
      .saved_profiles_status
      .set_label("Forgetting saved sign-in…");
    self.ui.saved_profiles_status.set_visible(true);
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Auth,
      "Saved profile removal started.",
    );
    let store = self.auth_store.clone();
    let command_key = key.clone();
    let session = self.requests.session_generation();
    sender.oneshot_command(async move {
      let result = run_auth_operation(move || store.remove_profile(&command_key))
        .await
        .map_err(|_| "The saved sign-in could not be forgotten.".to_string())
        .and_then(|result| {
          result.map_err(|error| format!("Saved sign-in could not be forgotten: {error}."))
        });
      AppCommand::ForgotProfile {
        session,
        key,
        sign_out,
        result,
      }
    });
  }

  #[allow(deprecated)]
  fn confirm_forget_saved_profile(
    &self,
    key: SavedProfileKey,
    sign_out: bool,
    sender: &ComponentSender<Self>,
  ) {
    if self.profile_operation_busy {
      return;
    }
    let Some(profile) = self
      .saved_profile_summaries()
      .iter()
      .find(|profile| profile.key == key)
    else {
      return;
    };
    let title = if sign_out {
      "Sign out and forget this profile?"
    } else {
      "Forget this saved sign-in?"
    };
    let server = profile
      .server_name
      .as_deref()
      .unwrap_or(profile.server_url.as_str());
    let dialog = gtk::MessageDialog::builder()
      .modal(true)
      .message_type(gtk::MessageType::Question)
      .text(title)
      .secondary_text(format!(
        "{} on {} will need a password to sign in again.",
        profile.user_name, server
      ))
      .build();
    if let Some(window) = relm4::main_adw_application().active_window() {
      dialog.set_transient_for(Some(&window));
    }
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let forget_button = dialog.add_button("Forget", gtk::ResponseType::Accept);
    forget_button.add_css_class("destructive-action");
    dialog.set_default_response(gtk::ResponseType::Cancel);
    dialog.connect_response({
      let sender = sender.clone();
      move |dialog, response| {
        dialog.close();
        if response == gtk::ResponseType::Accept {
          sender.input(AppMessage::ConfirmForgetSavedProfile {
            key: key.clone(),
            sign_out,
          });
        }
      }
    });
    dialog.present();
  }

  fn prepare_login(&mut self, status: &str) -> SessionToken {
    let session = self.requests.begin_login();
    self.browse.model.reset();
    self.connection = ConnectionPhase::Connecting;
    self.home = LoadState::Loading;
    self.set_login_controls_sensitive(false);
    self.ui.login_status.set_label(status);
    self.ui.login_status.set_visible(true);
    session
  }

  fn finish_login(
    &mut self,
    session: SessionToken,
    client: Arc<JellyfinClient>,
    result: Result<(), String>,
    sender: &ComponentSender<Self>,
  ) {
    if !matches!(self.connection, ConnectionPhase::Connecting)
      || !self.requests.finish_login(session)
    {
      return;
    }

    let was_quick_connect = self.quick_connect_phase.is_active();
    self.quick_connect_phase = if result.is_ok() {
      QuickConnectPhase::Idle
    } else if self.quick_connect_phase.is_active() {
      QuickConnectPhase::Failed
    } else {
      self.quick_connect_phase
    };
    self.ui.quick_connect_spinner.stop();
    self.ui.quick_connect_spinner.set_visible(false);
    if matches!(self.quick_connect_phase, QuickConnectPhase::Idle) {
      self.ui.quick_connect_code.set_label("");
      self.ui.quick_connect_code.set_visible(false);
      self.ui.quick_connect_status.set_label("");
    }
    self.set_login_controls_sensitive(true);
    self.render_quick_connect_controls();
    let pending_prefill = self.pending_prefill.take();
    match result {
      Ok(()) => {
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Connection,
          "Media server connection established.",
        );
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Auth,
          if was_quick_connect {
            "Quick Connect approval completed successfully."
          } else {
            "Authentication completed successfully."
          },
        );
        let prefill_warning = pending_prefill.and_then(|prefill| {
          let remember = prefill.remember;
          let result = if remember {
            config::save(&prefill)
          } else {
            config::clear()
          };
          result.err().map(|_| {
            if remember {
              "Signed in, but sign-in details could not be saved on this device."
            } else {
              "Signed in, but remembered sign-in details could not be cleared on this device."
            }
          })
        });
        let session_to_save = client.login().get_saved_session();
        if let Some(session) = session_to_save.as_ref() {
          self.ui.server_url.set_text(&session.server_url);
          self.ui.username.set_text(&session.user_name);
        }
        self.active_saved_profile = None;
        self.client = client;
        self
          .ui
          .intro_skip_group
          .set_visible(self.client.supports_intro_skipper());
        self.artwork.reset_session();
        self.diagnostics.reset_coalescing();
        self.artwork = Arc::new(ArtworkAdapter::default());
        let settings = config::load();
        self
          .artwork
          .set_disk_cache_enabled(settings.image_cache_enabled);
        if write_input_conf(
          &settings.key_next_episode,
          &settings.key_previous_episode,
          &settings.key_intro_skip,
        )
        .is_none()
        {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::Config,
            "The MPV shortcut file could not be written for this session.",
          );
        }
        self.playback = match PlaybackController::discover(
          Arc::clone(&self.client),
          playback_controller_config(&settings),
        ) {
          Ok(controller) => PlaybackState {
            controller: Some(controller),
            ..PlaybackState::default()
          },
          Err(error) => {
            self.record_diagnostic(
              DiagnosticLevel::Error,
              DiagnosticCategory::Playback,
              format!("External MPV playback is unavailable: {error}."),
            );
            PlaybackState {
              unavailable: Some(format!(
                "Playback is unavailable: {error}. Install MPV and try again."
              )),
              ..PlaybackState::default()
            }
          }
        };
        self.connection = ConnectionPhase::Connected;
        self.start_remote_session(sender);
        self.home = LoadState::Loading;
        self.shortcuts.clear();
        self.shortcuts_error = None;
        if let Some(warning) = prefill_warning {
          self.ui.login_status.set_label(warning);
          self.ui.login_status.set_visible(true);
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::Config,
            "Sign-in succeeded, but the local sign-in configuration update failed.",
          );
        } else {
          self.ui.login_status.set_label("");
          self.ui.login_status.set_visible(false);
        }
        if let Some(session_to_save) = session_to_save {
          self.start_persist_session(session_to_save, sender);
        } else {
          self
            .ui
            .settings_storage_status
            .set_label("The connected session could not be saved securely.");
          self.ui.settings_storage_status.set_visible(true);
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::Auth,
            "The connected session could not be stored in Secret Service.",
          );
        }
        self.render_authenticated(sender);
        self.show_home(sender);
        self.load_home(sender);
      }
      Err(message) => {
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Auth,
          if was_quick_connect {
            "Quick Connect failed or expired before authentication completed."
          } else {
            "Authentication failed."
          },
        );
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Connection,
          "Media server connection failed.",
        );
        self.connection = ConnectionPhase::Failed;
        self.home = LoadState::Failed(message.clone());
        if was_quick_connect {
          self.ui.quick_connect_status.set_label(&message);
          self.ui.login_status.set_label("");
          self.ui.login_status.set_visible(false);
        } else {
          self.ui.login_status.set_label(&message);
          self.ui.login_status.set_visible(true);
        }
        self.render_saved_profiles(sender);
      }
    }
  }

  fn start_persist_session(&mut self, session: SavedSession, sender: &ComponentSender<Self>) {
    // Preserve secure-storage intent ordering: Forget cannot overtake an older pending save and
    // then be undone when that save eventually acquires the keyring lock.
    self.set_profile_operation_busy(true);
    self
      .ui
      .settings_storage_status
      .set_label("Saving this session securely…");
    self.ui.settings_storage_status.set_visible(true);
    let store = self.auth_store.clone();
    let session_generation = self.requests.session_generation();
    sender.oneshot_command(async move {
      let result = run_auth_operation(move || store.save_session(session))
        .await
        .map_err(|_| "The session could not be saved securely.".to_string())
        .and_then(|result| {
          result.map_err(|error| format!("The session could not be saved securely: {error}."))
        });
      AppCommand::SavedSessionStored {
        session: session_generation,
        result,
      }
    });
  }

  fn load_home(&mut self, sender: &ComponentSender<Self>) {
    let client = Arc::clone(&self.client);
    let token = self.requests.begin_home();
    self.home = LoadState::Loading;
    if self.ui.content.visible_child_name().as_deref() == Some("home") {
      self.render_home(sender);
    }
    sender.oneshot_command(async move {
      let (home, shortcuts) = relm4::tokio::join!(
        async {
          client
            .library()
            .video_home()
            .await
            .map_err(|error| error.to_string())
        },
        async {
          client
            .library()
            .library_shortcuts()
            .await
            .map_err(|error| error.to_string())
        },
      );
      AppCommand::Home {
        token,
        result: (home, shortcuts),
      }
    });
  }

  fn finish_home(
    &mut self,
    token: HomeToken,
    result: (
      Result<VideoHome, String>,
      Result<Vec<VideoLibraryShortcut>, String>,
    ),
    sender: &ComponentSender<Self>,
  ) {
    if !self.requests.finish_home(token) || !matches!(self.connection, ConnectionPhase::Connected) {
      return;
    }
    self.home = match result.0 {
      Ok(home) => LoadState::Ready(home),
      Err(message) => LoadState::Failed(message),
    };
    match result.1 {
      Ok(shortcuts) => {
        self.shortcuts = shortcuts;
        self.shortcuts_error = None;
      }
      Err(message) => {
        self.shortcuts.clear();
        self.shortcuts_error = Some(message);
      }
    }
    self.render_shortcuts(sender);
    if self.ui.content.visible_child_name().as_deref() == Some("home") {
      self.render_home(sender);
    }
  }

  fn disconnect(&mut self, sender: &ComponentSender<Self>) {
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Connection,
      "Disconnected from the media server.",
    );
    self.stop_remote_session(None);
    self.requests.disconnect();
    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.artwork.reset_session();
    self.diagnostics.reset_coalescing();
    self.cancel_inflight_playback();
    self.artwork_view = self.artwork_view.saturating_add(1);
    self.artwork_targets.clear();
    let _client = std::mem::replace(&mut self.client, Arc::new(JellyfinClient::new()));
    let controller = self.playback.controller.take();
    self.playback_cleanup_pending = playback_cleanup_required(
      self.playback_cleanup_pending,
      controller.is_some(),
      self.playback.busy,
    );
    self.playback.pending.clear();
    self.connection = ConnectionPhase::SignedOut;
    self.active_saved_profile = None;
    self.home = LoadState::Idle;
    self.shortcuts.clear();
    self.shortcuts_error = None;
    self.browse = BrowseState::default();
    self.ui.sort_dropdown.set_selected(0);
    self.ui.played_dropdown.set_selected(0);
    self.ui.favorites_only.set_active(false);
    self.detail = LoadState::Idle;
    self.detail_selection = None;
    self.detail_origin = None;
    self.detail_parent = None;
    self.recommendations = LoadState::Idle;
    self.recommendation_sequence = self.recommendation_sequence.saturating_add(1);
    self.detail_identity = None;
    self.streams = LoadState::Idle;
    self.stream_sequence = self.stream_sequence.saturating_add(1);
    self.season_neighbors = LoadState::Idle;
    self.season_neighbor_sequence = self.season_neighbor_sequence.saturating_add(1);
    self.season = None;
    self.invalidate_user_data_update();
    self.playback = PlaybackState::default();
    self.ui.search.set_text("");
    self.ui.playback_bar.set_visible(false);
    self.ui.search.set_sensitive(false);
    // Activating the static group anchor first clears any dynamic library shortcut,
    // then deactivating it leaves the signed-out shell with no selected destination.
    self.ui.nav_home.set_active(true);
    self.ui.nav_home.set_active(false);
    self.ui.disconnect_button.set_sensitive(false);
    self.ui.settings_disconnect_button.set_sensitive(false);
    self.ui.connection_status.set_label("Not connected");
    self.render_connection_settings();
    self.ui.quick_connect_code.set_label("");
    self.ui.quick_connect_code.set_visible(false);
    self.ui.quick_connect_status.set_label("");
    self.ui.quick_connect_spinner.stop();
    self.ui.quick_connect_spinner.set_visible(false);
    self.render_quick_connect_controls();
    self.ui.intro_skip_group.set_visible(false);
    self.ui.preferences.close();
    self.ui.root.set_content(Some(&self.ui.login));
    self.set_login_controls_sensitive(!self.playback_cleanup_pending);
    self.render_saved_profiles(sender);
    self.render_saved_profile_settings();
    self
      .ui
      .login_status
      .set_label(if self.playback_cleanup_pending {
        "Stopping native playback before another connection can start…"
      } else {
        "Disconnected."
      });
    self.ui.login_status.set_visible(true);
    if let Some(controller) = controller {
      self.shutdown_playback(controller, PlaybackShutdownDisposition::Disconnect, sender);
    }
  }

  fn request_quit(&mut self, sender: &ComponentSender<Self>) {
    if self.quitting {
      return;
    }
    self.quitting = true;
    self.requests.disconnect();
    self.cancel_inflight_quick_connect();
    self.artwork.reset_session();
    self.invalidate_user_data_update();
    self.cancel_inflight_playback();
    if let Some(source) = self.playback_refresh_source.take() {
      source.remove();
    }
    self.playback.pending.clear();
    self.stop_remote_session(Some(sender));
    if let Some(controller) = self.playback.controller.take() {
      self.shutdown_playback(controller, PlaybackShutdownDisposition::Quit, sender);
    } else if quit_can_finish_without_controller(self.playback.busy, self.playback_cleanup_pending)
      && !self.remote_disconnect_pending
    {
      relm4::main_adw_application().quit();
    }
  }

  fn shutdown_playback(
    &self,
    mut controller: PlaybackController,
    disposition: PlaybackShutdownDisposition,
    sender: &ComponentSender<Self>,
  ) {
    sender.oneshot_command(async move {
      let outcome = controller.shutdown().await;
      AppCommand::PlaybackShutdown {
        disposition,
        warnings: outcome.warnings,
      }
    });
  }

  fn cancel_inflight_playback(&self) {
    let next = (*self.playback_cancellation.borrow()).wrapping_add(1);
    let _ = self.playback_cancellation.send_replace(next);
  }

  fn cancel_inflight_quick_connect(&self) {
    let next = (*self.quick_connect_cancellation.borrow()).wrapping_add(1);
    let _ = self.quick_connect_cancellation.send_replace(next);
  }

  fn render_authenticated(&mut self, sender: &ComponentSender<Self>) {
    self.ui.root.set_content(Some(&self.ui.authenticated));
    self.ui.search.set_sensitive(true);
    self
      .ui
      .disconnect_button
      .set_sensitive(!self.profile_operation_busy);
    self
      .ui
      .settings_disconnect_button
      .set_sensitive(!self.profile_operation_busy);
    self.update_connection_status();
    self.render_shortcuts(sender);
  }
  fn update_connection_status(&self) {
    let remote = match self.remote_state {
      RemoteControlState::Unavailable => "Remote control unavailable",
      RemoteControlState::Connecting => "Remote control connecting",
      RemoteControlState::Available => "Remote control available",
      RemoteControlState::Lost => "Remote control connection lost",
    };
    self
      .ui
      .connection_status
      .set_label(&format!("{} · {remote}", connection_label(&self.client)));
    self.render_connection_settings();
  }

  fn saved_profile_summaries(&self) -> &[SavedProfileSummary] {
    match &self.saved_profiles {
      LoadState::Ready(profiles) => profiles,
      LoadState::Idle | LoadState::Loading | LoadState::Failed(_) => &[],
    }
  }

  fn set_login_controls_sensitive(&self, sensitive: bool) {
    let sensitive = sensitive && !self.profile_operation_busy;
    self.ui.provider.set_sensitive(sensitive);
    self.ui.server_url.set_sensitive(sensitive);
    self.ui.remember_prefill.set_sensitive(sensitive);
    self.ui.username.set_sensitive(sensitive);
    self.ui.password.set_sensitive(sensitive);
    self.ui.login_button.set_sensitive(sensitive);
    self.ui.saved_profiles.set_sensitive(sensitive);
    self.ui.login_method_switcher.set_sensitive(sensitive);
    self.ui.quick_connect_button.set_sensitive(
      sensitive && quick_connect_available(provider_for(self.ui.provider.selected())),
    );
  }

  fn render_quick_connect_controls(&self) {
    let provider_supported = quick_connect_available(provider_for(self.ui.provider.selected()));
    let failed = matches!(self.quick_connect_phase, QuickConnectPhase::Failed);
    self
      .ui
      .login_method_switcher
      .set_visible(provider_supported);
    self.ui.quick_connect_button.set_visible(matches!(
      self.quick_connect_phase,
      QuickConnectPhase::Idle | QuickConnectPhase::Failed
    ));
    self.ui.quick_connect_button.set_label(
      if matches!(self.quick_connect_phase, QuickConnectPhase::Failed) {
        "Request a new code"
      } else {
        "Request Quick Connect code"
      },
    );
    self
      .ui
      .quick_connect_cancel_button
      .set_visible(self.quick_connect_phase.is_active());
    self.ui.quick_connect_code.set_visible(matches!(
      self.quick_connect_phase,
      QuickConnectPhase::Waiting | QuickConnectPhase::Approving
    ));
    self.ui.quick_connect_status.set_accessible_role(if failed {
      gtk::AccessibleRole::Alert
    } else {
      gtk::AccessibleRole::Status
    });
    if failed {
      self.ui.quick_connect_status.add_css_class("error");
    } else {
      self.ui.quick_connect_status.remove_css_class("error");
    }
  }

  fn set_profile_operation_busy(&mut self, busy: bool) {
    self.profile_operation_busy = busy;
    let connected = matches!(self.connection, ConnectionPhase::Connected);
    self.ui.saved_profiles.set_sensitive(!busy);
    self.ui.disconnect_button.set_sensitive(connected && !busy);
    self
      .ui
      .settings_disconnect_button
      .set_sensitive(connected && !busy);
    self
      .ui
      .forget_current_profile
      .set_sensitive(self.active_saved_profile.is_some() && !busy);
  }

  fn render_saved_profiles(&self, sender: &ComponentSender<Self>) {
    clear_list_box(&self.ui.saved_profiles);
    match &self.saved_profiles {
      LoadState::Idle | LoadState::Loading => {
        self
          .ui
          .saved_profiles_status
          .set_label("Loading saved sign-ins…");
        self.ui.saved_profiles_status.set_visible(true);
      }
      LoadState::Failed(message) => {
        self.ui.saved_profiles_status.set_label(message);
        self.ui.saved_profiles_status.set_visible(true);
      }
      LoadState::Ready(profiles) if profiles.is_empty() => {
        self
          .ui
          .saved_profiles_status
          .set_label("No saved sign-ins yet.");
        self.ui.saved_profiles_status.set_visible(true);
      }
      LoadState::Ready(profiles) => {
        self.ui.saved_profiles_status.set_visible(false);
        for profile in profiles {
          self
            .ui
            .saved_profiles
            .append(&saved_profile_row(profile, sender));
        }
      }
    }
  }

  fn render_saved_profile_settings(&self) {
    self.ui.settings_storage_status.set_visible(false);
    if !matches!(self.connection, ConnectionPhase::Connected) {
      self
        .ui
        .settings_saved_profile
        .set_label("No active session. Sign in to manage this device's saved profile.");
      self.ui.settings_disconnect_button.set_sensitive(false);
      self.ui.forget_current_profile.set_sensitive(false);
      return;
    }

    let active = self.active_saved_profile.as_ref().and_then(|key| {
      self
        .saved_profile_summaries()
        .iter()
        .find(|profile| &profile.key == key)
    });
    if let Some(profile) = active {
      self.ui.settings_saved_profile.set_label(&format!(
        "Signed in as {} on {}. The session token is stored in Linux Secret Service; the password is not saved.",
        profile.user_name,
        profile
          .server_name
          .as_deref()
          .unwrap_or(profile.server_url.as_str())
      ));
      self
        .ui
        .forget_current_profile
        .set_sensitive(!self.profile_operation_busy);
      self
        .ui
        .forget_current_profile
        .update_property(&[gtk::accessible::Property::Label(&format!(
          "Sign out and forget saved sign-in for {} on {}",
          profile.user_name,
          profile
            .server_name
            .as_deref()
            .unwrap_or(profile.server_url.as_str())
        ))]);
      self.ui.settings_storage_status.set_visible(true);
    } else {
      self
        .ui
        .settings_saved_profile
        .set_label("This active session is not saved. Passwords are never stored by the GTK app.");
      self.ui.forget_current_profile.set_sensitive(false);
      if matches!(self.saved_profiles, LoadState::Failed(_)) {
        self
          .ui
          .settings_storage_status
          .set_label("Linux Secret Service is unavailable or locked.");
        self.ui.settings_storage_status.set_visible(true);
      }
    }
  }

  fn show_home(&mut self, sender: &ComponentSender<Self>) {
    self.navigate_to("home");
    self.render_home(sender);
  }

  fn navigate_to(&mut self, page: &str) {
    self.requests.navigate();
    self.invalidate_user_data_update();
    self.show_page(page);
  }

  fn show_page(&self, page: &str) {
    self.ui.content.set_visible_child_name(page);
    self.ui.authenticated.set_show_content(true);
    match page {
      "home" => self.ui.nav_home.set_active(true),
      "browse" | "detail" => {}
      _ => {}
    }
  }

  fn render_shortcuts(&self, sender: &ComponentSender<Self>) {
    clear_box(&self.ui.shortcuts);
    if let Some(message) = &self.shortcuts_error {
      let retry = gtk::Button::with_label("Retry libraries");
      retry.set_tooltip_text(Some(message));
      let sender = sender.clone();
      retry.connect_clicked(move |_| sender.input(AppMessage::RetryHome));
      self.ui.shortcuts.append(&retry);
      return;
    }
    if self.shortcuts.is_empty() {
      self
        .ui
        .shortcuts
        .append(&dim_label("No video libraries available."));
      return;
    }
    for shortcut in &self.shortcuts {
      let button = navigation_button(&shortcut.name, "folder-videos-symbolic");
      button.set_group(Some(&self.ui.nav_home));
      let shortcut = shortcut.clone();
      let sender = sender.clone();
      button.connect_clicked(move |_| sender.input(AppMessage::OpenLibrary(shortcut.clone())));
      self.ui.shortcuts.append(&button);
    }
  }

  fn render_home(&mut self, sender: &ComponentSender<Self>) {
    self.begin_artwork_view(sender);
    clear_box(&self.ui.home_content);
    match &self.home {
      LoadState::Idle => self.ui.home_content.append(&state_view(
        "Connect to browse your libraries",
        "Sign in to Jellyfin or Emby to load Video Home.",
        "network-offline-symbolic",
      )),
      LoadState::Loading => self
        .ui
        .home_content
        .append(&loading_view("Loading Video Home…")),
      LoadState::Failed(message) => {
        self.ui.home_content.append(&state_view(
          "Video Home could not load",
          message.as_str(),
          "dialog-error-symbolic",
        ));
        let retry = gtk::Button::with_label("Retry");
        let sender = sender.clone();
        retry.connect_clicked(move |_| sender.input(AppMessage::RetryHome));
        self.ui.home_content.append(&retry);
      }
      LoadState::Ready(home) => {
        let continue_watching = home.continue_watching.clone();
        let next_up = home.next_up.clone();
        let latest_movies = home.latest_movies.clone();
        let latest_episodes = home.latest_episodes.clone();
        let hero_item = continue_watching
          .first()
          .or(next_up.first())
          .or(latest_movies.first())
          .cloned();
        if let Some(item) = hero_item {
          let hero = self.featured_hero(&item, sender);
          self.ui.home_content.append(&hero);
        }
        let shelves = [
          ("Continue Watching", continue_watching),
          ("Next Up", next_up),
          ("Latest Movies", latest_movies),
          ("Latest Episodes", latest_episodes),
        ];
        for (title, items) in shelves {
          let shelf = self.media_shelf(title, &items, sender);
          self.ui.home_content.append(&shelf);
        }
        let libraries = self.library_shortcuts_section(sender);
        self.ui.home_content.append(&libraries);
      }
    }
  }

  fn library_shortcuts_section(&mut self, sender: &ComponentSender<Self>) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title = gtk::Label::new(Some("Libraries"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    section.append(&title);
    if let Some(message) = &self.shortcuts_error {
      section.append(&dim_label(message));
      return section.upcast();
    }
    if self.shortcuts.is_empty() {
      section.append(&dim_label("No video libraries available."));
      return section.upcast();
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let shortcuts = self.shortcuts.clone();
    for shortcut in &shortcuts {
      row.append(&self.library_shortcut_card(shortcut, sender));
    }
    let scroll = gtk::ScrolledWindow::builder()
      .child(&row)
      .hscrollbar_policy(gtk::PolicyType::Automatic)
      .vscrollbar_policy(gtk::PolicyType::Never)
      .propagate_natural_width(true)
      .build();
    section.append(&scroll);
    section.upcast()
  }

  fn library_shortcut_card(
    &mut self,
    shortcut: &VideoLibraryShortcut,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.set_width_request(POSTER_FRAME_WIDTH);
    let button = gtk::Button::new();
    button.set_has_frame(false);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let artwork_overlay = gtk::Overlay::new();
    artwork_overlay.add_css_class("jellypilot-poster");
    artwork_overlay.set_overflow(gtk::Overflow::Hidden);
    artwork_overlay.set_size_request(POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT);
    let picture = cover_picture(POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT);
    let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    fallback.set_pixel_size(48);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    artwork_overlay.set_child(Some(&picture));
    artwork_overlay.add_overlay(&fallback);
    self.queue_artwork(
      picture,
      fallback,
      shortcut.artwork_image_id.as_deref(),
      sender,
    );
    column.append(&artwork_overlay);
    let text = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(2)
      .build();
    let title = gtk::Label::new(Some(&shortcut.name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(18);
    text.append(&title);
    let details = dim_label(&library_shortcut_caption(shortcut));
    details.set_ellipsize(gtk::pango::EllipsizeMode::End);
    details.set_max_width_chars(18);
    text.append(&details);
    column.append(&text);
    button.set_child(Some(&column));
    let accessible_label = format!("Open library {}", shortcut.name);
    button.set_tooltip_text(Some(&accessible_label));
    button.update_property(&[gtk::accessible::Property::Label(&accessible_label)]);
    let shortcut = shortcut.clone();
    let sender = sender.clone();
    button.connect_clicked(move |_| {
      sender.input(AppMessage::OpenLibrary(shortcut.clone()));
    });
    card.append(&button);
    card.upcast()
  }

  fn select_library_shortcut(&self, shortcut_id: &str) {
    let Some(index) = self
      .shortcuts
      .iter()
      .position(|shortcut| shortcut.id == shortcut_id)
    else {
      return;
    };
    let Some(button) = self
      .ui
      .shortcuts
      .first_child()
      .and_then(|mut child| {
        for _ in 0..index {
          child = child.next_sibling()?;
        }
        Some(child)
      })
      .and_downcast::<gtk::ToggleButton>()
    else {
      return;
    };
    button.set_active(true);
  }

  fn open_library(&mut self, shortcut: VideoLibraryShortcut, sender: &ComponentSender<Self>) {
    self.select_library_shortcut(&shortcut.id);
    self.navigate_to("browse");
    self.browse.title = shortcut.name.clone();
    self.browse.error = None;
    self.browse.library_shortcut = Some(shortcut.clone());
    let result = self.browse.model.configure_with_preferences(
      BrowseSource::Library {
        session: self.requests.session_generation(),
        shortcut,
      },
      browse_preferences(
        self.browse.sort_selection,
        self.browse.played_selection,
        self.browse.favorites_only,
      ),
    );
    match result {
      Ok(effects) => self.execute_browse_effects(effects, sender),
      Err(error) => self.browse.error = Some(error.to_string()),
    }
    self.render_browse(sender);
  }

  fn start_search(&mut self, sender: &ComponentSender<Self>) {
    let query = self.ui.search.text().trim().to_owned();
    if query.is_empty() {
      self
        .ui
        .search
        .set_tooltip_text(Some("Enter a title to search your server."));
      self.ui.search.grab_focus();
      return;
    }
    // Activating the static group anchor first clears any dynamic library shortcut,
    // then deactivating it represents a global search with no sidebar destination.
    self.ui.nav_home.set_active(true);
    self.ui.nav_home.set_active(false);
    self.navigate_to("browse");
    self.browse.title = format!("Search results for \"{query}\"");
    self.browse.error = None;
    self.browse.library_shortcut = None;
    let result = self.browse.model.configure(BrowseSource::Search {
      session: self.requests.session_generation(),
      query,
    });
    match result {
      Ok(effects) => self.execute_browse_effects(effects, sender),
      Err(error) => self.browse.error = Some(error.to_string()),
    }
    self.render_browse(sender);
  }

  fn apply_browse_preferences(&mut self, sender: &ComponentSender<Self>) {
    let Some(shortcut) = self.browse.library_shortcut.clone() else {
      return;
    };
    self.browse.error = None;
    let result = self.browse.model.configure_with_preferences(
      BrowseSource::Library {
        session: self.requests.session_generation(),
        shortcut,
      },
      browse_preferences(
        self.browse.sort_selection,
        self.browse.played_selection,
        self.browse.favorites_only,
      ),
    );
    match result {
      Ok(effects) => self.execute_browse_effects(effects, sender),
      Err(error) => self.browse.error = Some(error.to_string()),
    }
    self.render_browse(sender);
  }

  fn load_detail(&mut self, item: VideoLibraryItem, sender: &ComponentSender<Self>) {
    self.invalidate_user_data_update();
    let origin = self.ui.content.visible_child_name();
    if origin.as_deref() != Some("detail") {
      self.detail_origin = origin.map(|page| page.to_string());
      self.detail_parent = None;
    } else if let LoadState::Ready(content @ DetailContent::Show(_)) = &self.detail {
      self.detail_parent = Some(DetailParent {
        content: content.clone(),
        season: self.season.clone(),
      });
    }
    self.detail_selection = Some(item.clone());
    self.season = None;
    self.recommendation_sequence = self.recommendation_sequence.saturating_add(1);
    let recommendation_sequence = self.recommendation_sequence;
    self.recommendations = LoadState::Loading;
    self.detail_identity = Some(item.id.clone());
    self.stream_sequence = self.stream_sequence.saturating_add(1);
    let stream_sequence = self.stream_sequence;
    self.streams = LoadState::Loading;
    self.season_neighbor_sequence = self.season_neighbor_sequence.saturating_add(1);
    let season_neighbor_sequence = self.season_neighbor_sequence;
    let season_neighbor_request = item
      .series_id
      .clone()
      .zip(item.season_number)
      .filter(|_| item.item_type.eq_ignore_ascii_case("episode"));
    self.season_neighbors = if season_neighbor_request.is_some() {
      LoadState::Loading
    } else {
      LoadState::Idle
    };
    let token = self.requests.begin_detail();
    let recommendation_item_id = item.id.clone();
    self.detail = LoadState::Loading;
    self.show_page("detail");
    self.render_detail(sender);
    let client = Arc::clone(&self.client);
    let detail_item_id = item.id.clone();
    let detail_item_type = item.item_type.clone();
    sender.oneshot_command(async move {
      let result = if detail_item_type.eq_ignore_ascii_case("series") {
        client
          .library()
          .show_detail(detail_item_id.clone())
          .await
          .map(DetailContent::Show)
          .map_err(|error| error.to_string())
      } else {
        client
          .library()
          .item_detail(detail_item_id)
          .await
          .map(DetailContent::Item)
          .map_err(|error| error.to_string())
      };
      AppCommand::Detail {
        token,
        result: Box::new(result),
      }
    });
    let client = Arc::clone(&self.client);
    let item_id = recommendation_item_id;
    let session = self.requests.session_generation();
    sender.oneshot_command(async move {
      let result = client
        .library()
        .similar_video(item_id.clone())
        .await
        .map_err(|error| error.to_string());
      AppCommand::Recommendations {
        session,
        sequence: recommendation_sequence,
        item_id,
        result,
      }
    });
    let client = Arc::clone(&self.client);
    let stream_item_id = item.id.clone();
    let session = self.requests.session_generation();
    sender.oneshot_command(async move {
      let result = client
        .library()
        .item_streams(stream_item_id.clone())
        .await
        .map_err(|error| error.to_string());
      AppCommand::Streams {
        session,
        sequence: stream_sequence,
        item_id: stream_item_id,
        result,
      }
    });
    if let Some((series_id, season_number)) = season_neighbor_request {
      let client = Arc::clone(&self.client);
      let item_id = item.id.clone();
      let session = self.requests.session_generation();
      sender.oneshot_command(async move {
        let result = client
          .library()
          .season_episodes_page(VideoSeasonEpisodesPageRequest {
            series_id,
            season_id: None,
            season_number: Some(season_number),
            start_index: 0,
            limit: SEASON_EPISODE_PAGE_SIZE,
          })
          .await
          .map(|page| {
            page
              .episodes
              .into_iter()
              .filter(|episode| episode.id != item_id)
              .collect()
          })
          .map_err(|error| error.to_string());
        AppCommand::SeasonNeighbors {
          session,
          sequence: season_neighbor_sequence,
          item_id,
          result,
        }
      });
    }
  }

  fn retry_detail(&mut self, sender: &ComponentSender<Self>) {
    let Some(item) = self.detail_selection.clone() else {
      return;
    };
    self.load_detail(item, sender);
  }

  fn back_from_detail(&mut self, sender: &ComponentSender<Self>) {
    self.invalidate_user_data_update();
    if let Some(parent) = self.detail_parent.take() {
      self.requests.navigate();
      self.detail = LoadState::Ready(parent.content);
      self.season = parent.season;
      self.detail_identity = self.current_detail_identity().map(str::to_owned);
      self.recommendation_sequence = self.recommendation_sequence.saturating_add(1);
      let recommendation_sequence = self.recommendation_sequence;
      self.recommendations = LoadState::Loading;
      self.streams = LoadState::Idle;
      self.stream_sequence = self.stream_sequence.saturating_add(1);
      self.season_neighbors = LoadState::Idle;
      self.season_neighbor_sequence = self.season_neighbor_sequence.saturating_add(1);
      if let Some(item_id) = self.detail_identity.clone() {
        let client = Arc::clone(&self.client);
        let session = self.requests.session_generation();
        sender.oneshot_command(async move {
          let result = client
            .library()
            .similar_video(item_id.clone())
            .await
            .map_err(|error| error.to_string());
          AppCommand::Recommendations {
            session,
            sequence: recommendation_sequence,
            item_id,
            result,
          }
        });
      }
      self.render_detail(sender);
      return;
    }
    if self.season.is_some() {
      self.requests.navigate();
      self.season = None;
      self.render_detail(sender);
      return;
    }
    let origin = self
      .detail_origin
      .clone()
      .unwrap_or_else(|| "home".to_owned());
    self.navigate_to(&origin);
    match origin.as_str() {
      "home" => self.render_home(sender),
      "browse" => self.render_browse(sender),
      _ => {}
    }
  }

  fn invalidate_user_data_update(&mut self) {
    self.user_data_sequence = self.user_data_sequence.saturating_add(1);
    self.user_data_busy = false;
    self.user_data_error = None;
  }

  fn start_user_data_update(
    &mut self,
    item_id: String,
    action: VideoUserDataAction,
    sender: &ComponentSender<Self>,
  ) {
    if self.user_data_busy || self.current_detail_item_id() != Some(item_id.as_str()) {
      return;
    }
    self.user_data_sequence = self.user_data_sequence.saturating_add(1);
    self.user_data_busy = true;
    self.user_data_error = None;
    let session = self.requests.session_generation();
    let sequence = self.user_data_sequence;
    self.render_detail(sender);
    let client = Arc::clone(&self.client);
    sender.oneshot_command(async move {
      let result = client
        .library()
        .update_user_data(VideoUserDataUpdateRequest {
          item_id: item_id.clone(),
          action,
        })
        .await
        .map_err(|_| "Could not update this item's library state.".to_owned());
      AppCommand::UserData {
        session,
        sequence,
        item_id,
        result,
      }
    });
  }

  fn finish_user_data_update(
    &mut self,
    session: u64,
    sequence: u64,
    item_id: &str,
    result: Result<VideoUserDataUpdate, String>,
    sender: &ComponentSender<Self>,
  ) {
    if session != self.requests.session_generation()
      || sequence != self.user_data_sequence
      || self.current_detail_item_id() != Some(item_id)
    {
      return;
    }
    self.user_data_busy = false;
    match result {
      Ok(update) => {
        let updated = apply_user_data_update(&mut self.detail, &update);
        debug_assert!(
          updated,
          "current detail identity was checked before applying user data"
        );
        if let Some(selection) = self
          .detail_selection
          .as_mut()
          .filter(|selection| selection.id == update.item_id)
        {
          selection.played = update.played;
          selection.favorite = update.favorite;
        }
        self.user_data_error = None;
      }
      Err(message) => self.user_data_error = Some(message),
    }
    self.render_detail(sender);
  }

  fn current_detail_item_id(&self) -> Option<&str> {
    match &self.detail {
      LoadState::Ready(DetailContent::Item(detail)) => Some(detail.id.as_str()),
      LoadState::Ready(DetailContent::Show(detail)) => Some(detail.id.as_str()),
      _ => None,
    }
  }
  fn current_detail_identity(&self) -> Option<&str> {
    match &self.detail {
      LoadState::Ready(DetailContent::Item(detail)) => Some(detail.id.as_str()),
      LoadState::Ready(DetailContent::Show(detail)) => Some(detail.id.as_str()),
      _ => None,
    }
  }

  fn load_season(&mut self, season: VideoSeason, sender: &ComponentSender<Self>) {
    self.load_season_page(season, 0, sender);
  }

  fn load_season_page(
    &mut self,
    season: VideoSeason,
    start_index: i32,
    sender: &ComponentSender<Self>,
  ) {
    let Some(series_id) = self.current_show().map(|detail| detail.id.clone()) else {
      return;
    };
    let start_index = start_index.max(0);
    let token = self.requests.begin_detail();
    let season_id = season.id.clone();
    let request = season_page_request(&series_id, &season, start_index);
    self.season = Some(SeasonSelection {
      season,
      episodes: LoadState::Loading,
      requested_start_index: start_index,
    });
    self.render_detail(sender);
    let client = Arc::clone(&self.client);
    sender.oneshot_command(async move {
      let result = client
        .library()
        .season_episodes_page(request)
        .await
        .map_err(|error| error.to_string());
      AppCommand::SeasonEpisodes {
        token,
        season_id,
        result,
      }
    });
  }

  fn retry_season(&mut self, sender: &ComponentSender<Self>) {
    let Some((season, start_index)) = self
      .season
      .as_ref()
      .map(|selection| (selection.season.clone(), selection.requested_start_index))
    else {
      return;
    };
    self.load_season_page(season, start_index, sender);
  }

  fn change_season_episode_page(&mut self, direction: i8, sender: &ComponentSender<Self>) {
    let Some(selection) = self.season.as_ref() else {
      return;
    };
    let LoadState::Ready(page) = &selection.episodes else {
      return;
    };
    let next_start_index = if direction < 0 {
      if page.start_index <= 0 {
        return;
      }
      page.start_index.saturating_sub(page.limit.max(1))
    } else {
      if !page.has_more || page.next_start_index <= page.start_index {
        return;
      }
      page.next_start_index
    };
    let season = selection.season.clone();
    self.load_season_page(season, next_start_index, sender);
  }

  fn current_show(&self) -> Option<&VideoShowDetail> {
    match &self.detail {
      LoadState::Ready(DetailContent::Show(detail)) => Some(detail),
      _ => None,
    }
  }

  fn load_next_page(&mut self, sender: &ComponentSender<Self>) {
    match self.browse.model.load_next() {
      Ok(effects) => {
        self.browse.error = None;
        self.execute_browse_effects(effects, sender);
      }
      Err(error) => self.browse.error = Some(error.to_string()),
    }
    self.render_browse(sender);
  }

  fn load_previous_page(&mut self, sender: &ComponentSender<Self>) {
    match self.browse.model.load_previous() {
      Ok(effects) => {
        self.browse.error = None;
        self.execute_browse_effects(effects, sender);
      }
      Err(error) => self.browse.error = Some(error.to_string()),
    }
    self.render_browse(sender);
  }

  fn retry_browse(&mut self, sender: &ComponentSender<Self>) {
    match self.browse.model.retry() {
      Ok(effects) => {
        self.browse.error = None;
        self.execute_browse_effects(effects, sender);
      }
      Err(error) => self.browse.error = Some(error.to_string()),
    }
    self.render_browse(sender);
  }

  fn execute_browse_effects(&self, effects: Vec<BrowseEffect>, sender: &ComponentSender<Self>) {
    for effect in effects {
      match effect {
        BrowseEffect::ResetViewport => {
          let adjustment = self.ui.browse_scroll.vadjustment();
          adjustment.set_value(adjustment.lower());
        }
        BrowseEffect::RequestPage(request) => self.request_browse_page(request, sender),
        // One-shot commands cannot be aborted individually. Removing the pending reducer token
        // still makes the eventual completion a deterministic no-op.
        BrowseEffect::CancelPage => {}
      }
    }
  }

  fn request_browse_page(&self, request: BrowsePageRequest, sender: &ComponentSender<Self>) {
    let client = Arc::clone(&self.client);
    sender.oneshot_command(async move {
      let BrowsePageRequest {
        source_id,
        source,
        token,
        start_index,
        limit,
        preferences,
      } = request;
      let result = async {
        let start_index = i32::try_from(start_index)
          .map_err(|_| "Library page start index is too large.".to_owned())?;
        let limit =
          i32::try_from(limit).map_err(|_| "Library page size is too large.".to_owned())?;
        match source {
          BrowseSource::Library { shortcut, .. } => {
            let collection_type = library_kind(&shortcut.collection_type);
            client
              .library()
              .browse_video(VideoLibraryPageRequest {
                library_id: shortcut.id,
                collection_type,
                start_index,
                limit,
                sort: preferences.sort,
                sort_direction: preferences.sort_direction,
                played_filter: preferences.played_filter,
                favorites_only: preferences.favorites_only,
              })
              .await
              .map_err(|error| error.to_string())?
              .try_into()
          }
          BrowseSource::Search { query, .. } => {
            let page = client
              .library()
              .search_video(VideoSearchRequest {
                query: query.clone(),
                start_index,
                limit,
              })
              .await
              .map_err(|error| error.to_string())?;
            if page.query != query {
              return Err("Media server returned results for a different search.".to_owned());
            }
            BrowsePagePayload::try_from(page)
          }
        }
      }
      .await;
      AppCommand::Browse(BrowsePageSettlement {
        source_id,
        token,
        result,
      })
    });
  }

  fn render_browse(&mut self, sender: &ComponentSender<Self>) {
    self.begin_artwork_view(sender);
    self.ui.browse_title.set_label(&self.browse.title);
    self
      .ui
      .browse_filter_bar
      .set_visible(self.browse.library_shortcut.is_some());
    self
      .ui
      .sort_dropdown
      .set_selected(self.browse.sort_selection);
    self
      .ui
      .played_dropdown
      .set_selected(self.browse.played_selection);
    self
      .ui
      .favorites_only
      .set_active(self.browse.favorites_only);
    self
      .ui
      .grid_button
      .set_active(matches!(self.browse.presentation, BrowsePresentation::Grid));
    self
      .ui
      .list_button
      .set_active(matches!(self.browse.presentation, BrowsePresentation::List));
    clear_box(&self.ui.browse_content);
    self.ui.browse_status.set_label("");
    self.ui.load_previous_button.set_visible(false);
    self.ui.load_previous_button.set_sensitive(true);
    self.ui.load_next_button.set_visible(false);
    self.ui.load_next_button.set_sensitive(true);
    if let Some(message) = &self.browse.error {
      self.ui.browse_content.append(&state_view(
        "Items could not load",
        message,
        "dialog-error-symbolic",
      ));
      return;
    }

    match self.browse.model.view() {
      LibraryBrowseView::Inactive => self.ui.browse_content.append(&state_view(
        "Choose a library",
        "Select Movies or Shows from the sidebar.",
        "folder-videos-symbolic",
      )),
      LibraryBrowseView::Loading => self
        .ui
        .browse_content
        .append(&loading_view("Loading items…")),
      LibraryBrowseView::Empty => self.ui.browse_content.append(&state_view(
        "No matching items",
        "Try a different library or search term.",
        "edit-find-symbolic",
      )),
      LibraryBrowseView::Failed {
        message,
        retryable,
        retry_busy,
      } => {
        self.ui.browse_status.set_label(&message);
        self.ui.browse_content.append(&state_view(
          "Items could not load",
          &message,
          "dialog-error-symbolic",
        ));
        if retryable {
          let retry = gtk::Button::with_label("Retry");
          retry.set_sensitive(!retry_busy);
          let sender = sender.clone();
          retry.connect_clicked(move |_| sender.input(AppMessage::RetryBrowse));
          self.ui.browse_content.append(&retry);
        }
      }
      LibraryBrowseView::Ready {
        visible_items,
        total_record_count,
        is_fetching_more,
        load_more_failure,
        retry_busy,
        ..
      } => {
        let display_range = self.browse.model.display_range();
        let items: Vec<_> = visible_items
          .into_iter()
          .filter_map(|slot| slot.item)
          .collect();
        self.render_media_results(&items, total_record_count, sender);
        if let Some(range) = display_range {
          self.ui.browse_status.set_label(&format!(
            "Items {}–{} of {total_record_count}",
            range.start.saturating_add(1),
            range.end
          ));
        }
        if is_fetching_more {
          self
            .ui
            .browse_content
            .append(&loading_view("Loading more items…"));
        }
        if let Some(message) = load_more_failure {
          self.ui.browse_content.append(&state_view(
            "More items could not load",
            &message,
            "dialog-warning-symbolic",
          ));
          let retry = gtk::Button::with_label("Retry loading more");
          retry.set_sensitive(!retry_busy);
          let sender = sender.clone();
          retry.connect_clicked(move |_| sender.input(AppMessage::RetryBrowse));
          self.ui.browse_content.append(&retry);
        } else {
          self
            .ui
            .load_previous_button
            .set_visible(self.browse.model.can_load_previous());
          self
            .ui
            .load_previous_button
            .set_sensitive(!is_fetching_more);
          self
            .ui
            .load_next_button
            .set_visible(self.browse.model.can_load_more());
          self.ui.load_next_button.set_sensitive(!is_fetching_more);
        }
      }
    }
  }

  fn render_media_results(
    &mut self,
    items: &[VideoLibraryItem],
    total: u32,
    sender: &ComponentSender<Self>,
  ) {
    self.ui.browse_status.set_label(&format!("{total} items"));
    if items.is_empty() {
      self.ui.browse_content.append(&state_view(
        "No matching items",
        "Try a different library or search term.",
        "edit-find-symbolic",
      ));
      return;
    }
    let content = match self.browse.presentation {
      BrowsePresentation::Grid => self.media_grid(items, sender),
      BrowsePresentation::List => self.media_list(items, sender),
    };
    self.ui.browse_content.append(&content);
  }

  fn render_detail(&mut self, sender: &ComponentSender<Self>) {
    self.begin_artwork_view(sender);
    clear_box(&self.ui.detail_content);
    let back = gtk::Button::with_label("Back");
    let sender_clone = sender.clone();
    back.connect_clicked(move |_| sender_clone.input(AppMessage::BackFromDetail));
    self.ui.detail_content.append(&back);
    if let Some(message) = &self.user_data_error {
      let status = dim_label(message);
      status.set_accessible_role(gtk::AccessibleRole::Status);
      status.set_wrap(true);
      self.ui.detail_content.append(&status);
    }
    match &self.detail {
      LoadState::Idle => self.ui.detail_content.append(&state_view(
        "Select an item",
        "Choose a movie or episode to inspect its details.",
        "view-more-symbolic",
      )),
      LoadState::Loading => self
        .ui
        .detail_content
        .append(&loading_view("Loading details…")),
      LoadState::Failed(message) => {
        self.ui.detail_content.append(&state_view(
          "Details could not load",
          message.as_str(),
          "dialog-error-symbolic",
        ));
        let retry = gtk::Button::with_label("Retry");
        retry.set_sensitive(self.detail_selection.is_some());
        let sender = sender.clone();
        retry.connect_clicked(move |_| sender.input(AppMessage::RetryDetail));
        self.ui.detail_content.append(&retry);
      }
      LoadState::Ready(DetailContent::Item(detail)) => {
        let detail = detail.clone();
        let view = self.detail_view(&detail, sender);
        self.ui.detail_content.append(&view);
      }
      LoadState::Ready(DetailContent::Show(detail)) => {
        let detail = detail.clone();
        let view = self.show_detail_view(&detail, sender);
        self.ui.detail_content.append(&view);
      }
    }
  }

  fn start_playback(&mut self, request: PlaybackRequest, sender: &ComponentSender<Self>) {
    let request_kind = request.kind();
    match &request {
      PlaybackRequest::Paused(value) => self.playback.desired_paused = Some(*value),
      PlaybackRequest::Muted(value) => self.playback.desired_muted = Some(*value),
      _ => {}
    }
    if request_kind == PlaybackRequestKind::Start {
      self.playback_start_generation = self.playback_start_generation.wrapping_add(1);
    }
    if let Some(identity) = request.identity() {
      if !auxiliary_settlement_is_current(
        identity.session,
        identity,
        self.requests.session_generation(),
        self.playback.identity.as_ref(),
      ) {
        return;
      }
    }
    if self.playback.busy {
      if request_kind != PlaybackRequestKind::Refresh {
        self.queue_playback_request(request);
      }
      return;
    }
    let started_artwork_image_id = request.started_artwork_image_id();
    if request_kind == PlaybackRequestKind::Start {
      if let Some(image_id) = started_artwork_image_id.clone() {
        self.playback.active_artwork_image_id = Some(image_id);
        self.queue_playback_artwork(sender);
      }
    }
    let started_item = request.started_item();
    let track_identity = request.identity().cloned();
    let Some(mut controller) = self.playback.controller.take() else {
      if matches!(
        request_kind,
        PlaybackRequestKind::Refresh | PlaybackRequestKind::RefreshTracks
      ) {
        return;
      }
      self.playback.error = self
        .playback
        .unavailable
        .clone()
        .or_else(|| Some("Playback controller is unavailable.".to_owned()));
      if let Some(message) = self.playback.error.as_deref() {
        self.add_toast(message);
      }
      self.render_playback_bar();
      return;
    };
    if let Some(identity) = track_identity {
      self.playback.tracks = PlaybackTrackState::Loading { identity };
    }
    self.playback.busy = true;
    self.playback.sequence = self.playback.sequence.saturating_add(1);
    let session = self.requests.session_generation();
    let sequence = self.playback.sequence;
    let mut cancellation = self.playback_cancellation.subscribe();
    self.render_playback_bar();
    sender.oneshot_command(async move {
      let result = {
        let operation = async {
          match request {
            PlaybackRequest::Library(item, start_position) => controller
              .play_library_item(
                &item,
                PlaybackOptions {
                  start_position,
                  ..PlaybackOptions::default()
                },
              )
              .await
              .map(|outcome| {
                PlaybackCommandSuccess::playback(Some(outcome.snapshot), outcome.warnings, None)
              })
              .map_err(playback_start_failure),
            PlaybackRequest::Detail(item, start_position) => controller
              .play_item_detail(
                &item,
                PlaybackOptions {
                  start_position,
                  ..PlaybackOptions::default()
                },
              )
              .await
              .map(|outcome| {
                PlaybackCommandSuccess::playback(Some(outcome.snapshot), outcome.warnings, None)
              })
              .map_err(playback_start_failure),
            PlaybackRequest::ReplaceMedia(item) => match controller.stop().await {
              Ok(stopped) => {
                match controller
                  .play_media_item(&item, PlaybackOptions::default())
                  .await
                {
                  Ok(outcome) => {
                    let mut warnings = stopped.warnings;
                    warnings.extend(outcome.warnings);
                    Ok(PlaybackCommandSuccess::playback(
                      Some(outcome.snapshot),
                      warnings,
                      None,
                    ))
                  }
                  Err(error) => Err(PlaybackCommandFailure {
                    message: format!("Could not start adjacent episode: {error}."),
                    clear_snapshot: true,
                  }),
                }
              }
              Err(error) => Err(playback_failure(
                "Could not stop the current episode",
                error,
              )),
            },
            PlaybackRequest::Paused(paused) => controller
              .set_paused(paused)
              .await
              .map(|outcome| {
                PlaybackCommandSuccess::playback(Some(outcome.snapshot), outcome.warnings, None)
              })
              .map_err(|error| playback_failure("Could not update playback", error)),
            PlaybackRequest::Seek(position) => controller
              .seek(position)
              .await
              .map(|outcome| {
                PlaybackCommandSuccess::playback(Some(outcome.snapshot), outcome.warnings, None)
              })
              .map_err(|error| playback_failure("Could not seek", error)),
            PlaybackRequest::Volume(volume) => controller
              .set_volume(volume)
              .await
              .map(|outcome| {
                PlaybackCommandSuccess::playback(Some(outcome.snapshot), outcome.warnings, None)
              })
              .map_err(|error| playback_failure("Could not set volume", error)),
            PlaybackRequest::Muted(muted) => controller
              .set_muted(muted)
              .await
              .map(|outcome| {
                PlaybackCommandSuccess::playback(Some(outcome.snapshot), outcome.warnings, None)
              })
              .map_err(|error| playback_failure("Could not update mute", error)),
            PlaybackRequest::AudioTrack { id, .. } => {
              let result = match controller.select_audio_track(id).await {
                Ok(()) => controller.tracks().await.map_err(|_| {
                  "MPV changed the audio track, but its track state could not be refreshed."
                    .to_owned()
                }),
                Err(_) => Err("MPV could not select that audio track.".to_owned()),
              };
              Ok(PlaybackCommandSuccess::tracks(result))
            }
            PlaybackRequest::SubtitleTrack { id, .. } => {
              let result = match controller.select_subtitle_track(id).await {
                Ok(()) => controller.tracks().await.map_err(|_| {
                  "MPV changed the subtitle track, but its track state could not be refreshed."
                    .to_owned()
                }),
                Err(_) => Err("MPV could not select that subtitle track.".to_owned()),
              };
              Ok(PlaybackCommandSuccess::tracks(result))
            }
            PlaybackRequest::RefreshTracks(_) => {
              let result = controller
                .tracks()
                .await
                .map_err(|_| "MPV track information is unavailable.".to_owned());
              Ok(PlaybackCommandSuccess::tracks(result))
            }
            PlaybackRequest::ShowText {
              identity,
              text,
              duration_ms,
              prompt_range,
            } => controller
              .show_text(&text, duration_ms)
              .await
              .map(|()| {
                let mut success = PlaybackCommandSuccess::preserved();
                success.prompt_displayed = prompt_range.map(|range_index| IntroPromptReceipt {
                  identity,
                  range_index,
                  duration: Duration::from_millis(duration_ms.max(0) as u64),
                });
                success
              })
              .map_err(|error| playback_failure("Could not show the Intro Skipper prompt", error)),
            PlaybackRequest::Stop => controller
              .stop()
              .await
              .map(|outcome| {
                PlaybackCommandSuccess::playback(
                  None,
                  outcome.warnings,
                  Some("Playback stopped.".to_owned()),
                )
              })
              .map_err(|error| playback_failure("Could not stop playback", error)),
            PlaybackRequest::Refresh => {
              let outcome = controller.refresh().await;
              let (snapshot, notice) = match outcome.state {
                PlaybackRefreshState::Active => (Some(outcome.snapshot), None),
                PlaybackRefreshState::Idle => (None, None),
                PlaybackRefreshState::Ended(PlaybackEndReason::EndOfFile) => {
                  (None, Some("Playback finished.".to_owned()))
                }
                PlaybackRefreshState::Ended(PlaybackEndReason::Disconnected) => (
                  None,
                  Some("The external player disconnected; playback was stopped.".to_owned()),
                ),
                PlaybackRefreshState::Ended(PlaybackEndReason::Error) => (
                  None,
                  Some(
                    "The external player could not continue this item; playback was stopped."
                      .to_owned(),
                  ),
                ),
              };
              let mut success =
                PlaybackCommandSuccess::playback(snapshot, outcome.warnings, notice);
              success.client_messages = controller.take_client_messages();
              Ok(success)
            }
          }
        };
        relm4::tokio::pin!(operation);
        relm4::tokio::select! {
          result = &mut operation => Some(result),
          changed = cancellation.changed() => {
            let _ = changed;
            None
          }
        }
      };
      let result = match result {
        Some(result) => result,
        None => {
          let _ = controller.shutdown().await;
          Err(PlaybackCommandFailure {
            message: "Playback operation was cancelled.".to_owned(),
            clear_snapshot: true,
          })
        }
      };
      AppCommand::Playback {
        session,
        sequence,
        request_kind,
        started_item,
        started_artwork_image_id,
        controller: Box::new(controller),
        result,
      }
    });
  }

  fn queue_playback_request(&mut self, request: PlaybackRequest) {
    let kind = request.kind();
    if matches!(kind, PlaybackRequestKind::Start | PlaybackRequestKind::Stop) {
      self.playback.pending.clear();
    } else {
      self
        .playback
        .pending
        .retain(|pending| pending.kind() != kind);
    }
    self.playback.pending.push_back(request);
  }

  fn clear_playback_context(&mut self) {
    self.playback.active_item = None;
    self.playback.active_artwork_image_id = None;
    self.playback.identity = None;
    self.playback.desired_paused = None;
    self.playback.desired_muted = None;
    self.playback.tracks = PlaybackTrackState::Unavailable;
    self.playback.adjacent = AdjacentState::default();
    self.playback.intro_skip = IntroSkipState::default();
  }

  fn finish_track_refresh(&mut self, result: Result<Vec<TrackInfo>, String>) {
    let PlaybackTrackState::Loading { identity } = &self.playback.tracks else {
      return;
    };
    let identity = identity.clone();
    if !auxiliary_settlement_is_current(
      identity.session,
      &identity,
      self.requests.session_generation(),
      self.playback.identity.as_ref(),
    ) {
      return;
    }
    self.playback.tracks = match result {
      Ok(tracks) => PlaybackTrackState::Ready { identity, tracks },
      Err(message) => PlaybackTrackState::Failed { identity, message },
    };
  }

  fn select_track(&mut self, kind: TrackKind, id: Option<i64>, sender: &ComponentSender<Self>) {
    let PlaybackTrackState::Ready { identity, tracks } = &self.playback.tracks else {
      return;
    };
    let expected_type = match kind {
      TrackKind::Audio => "audio",
      TrackKind::Subtitle => "sub",
    };
    if let Some(id) = id {
      if !tracks
        .iter()
        .any(|track| track.track_type == expected_type && track.id == id)
      {
        return;
      }
    } else if kind == TrackKind::Audio {
      return;
    }
    let identity = identity.clone();
    let request = match (kind, id) {
      (TrackKind::Audio, Some(id)) => PlaybackRequest::AudioTrack { identity, id },
      (TrackKind::Audio, None) => return,
      (TrackKind::Subtitle, id) => PlaybackRequest::SubtitleTrack { identity, id },
    };
    self.start_playback(request, sender);
  }

  fn play_adjacent(&mut self, direction: AdjacentDirection, sender: &ComponentSender<Self>) {
    if self.playback.busy {
      return;
    }
    let AdjacentAvailability::Available(item) = self.playback.adjacent.availability(direction)
    else {
      return;
    };
    let item = item.clone();
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::Playback,
      match direction {
        AdjacentDirection::Previous => "Starting the previous episode.",
        AdjacentDirection::Next => "Starting the next episode.",
      },
    );
    self.start_playback(PlaybackRequest::ReplaceMedia(item), sender);
  }

  fn refresh_adjacent_episodes(
    &mut self,
    identity: PlaybackIdentity,
    sender: &ComponentSender<Self>,
  ) {
    if !auxiliary_settlement_is_current(
      identity.session,
      &identity,
      self.requests.session_generation(),
      self.playback.identity.as_ref(),
    ) {
      return;
    }
    let Some(item) = self.playback.active_item.clone() else {
      return;
    };
    self.playback.adjacent.sequence = self.playback.adjacent.sequence.saturating_add(1);
    self.playback.adjacent.identity = Some(identity.clone());
    if item.item_type != "Episode" {
      let reason = "Previous and next controls are available only for episodes.".to_owned();
      self.playback.adjacent.previous = AdjacentAvailability::Unavailable(reason.clone());
      self.playback.adjacent.next = AdjacentAvailability::Unavailable(reason);
      self.render_playback_bar();
      return;
    }
    if item.series_id.is_none() {
      let reason = "The server did not provide the series identity for this episode.".to_owned();
      self.playback.adjacent.previous = AdjacentAvailability::Unavailable(reason.clone());
      self.playback.adjacent.next = AdjacentAvailability::Unavailable(reason);
      self.render_playback_bar();
      return;
    }

    let session = identity.session;
    let sequence = self.playback.adjacent.sequence;
    let client = Arc::clone(&self.client);
    self.playback.adjacent.previous = AdjacentAvailability::Loading;
    self.playback.adjacent.next = AdjacentAvailability::Loading;
    self.render_playback_bar();
    sender.oneshot_command(async move {
      let previous_client = Arc::clone(&client);
      let previous_item = item.clone();
      let previous = async move {
        previous_client
          .playback()
          .get_previous_episode(&previous_item)
          .await
          .map_err(|_| "Could not check for a previous episode.".to_owned())
      };
      let next = async move {
        client
          .playback()
          .get_next_episode(&item)
          .await
          .map_err(|_| "Could not check for a next episode.".to_owned())
      };
      let (previous, next) = relm4::tokio::join!(previous, next);
      AppCommand::AdjacentEpisodes {
        session,
        sequence,
        identity,
        previous,
        next,
      }
    });
  }

  fn refresh_intro_ranges(&mut self, identity: PlaybackIdentity, sender: &ComponentSender<Self>) {
    if !auxiliary_settlement_is_current(
      identity.session,
      &identity,
      self.requests.session_generation(),
      self.playback.identity.as_ref(),
    ) {
      return;
    }
    let Some(item) = self.playback.active_item.as_ref() else {
      return;
    };
    let mode = session_intro_mode(self.intro_mode);
    self.playback.intro_skip.identity = Some(identity.clone());
    self.playback.intro_skip.mode = mode;
    self.playback.intro_skip.ranges.clear();
    self.playback.intro_skip.sequence = self.playback.intro_skip.sequence.saturating_add(1);
    if !should_fetch_intro_ranges(
      self.intro_mode,
      self.client.supports_intro_skipper(),
      &item.item_type,
    ) {
      return;
    }

    let session = identity.session;
    let sequence = self.playback.intro_skip.sequence;
    let item_id = item.id.clone();
    let client = Arc::clone(&self.client);
    sender.oneshot_command(async move {
      let ranges = client
        .playback()
        .get_intro_skipper_ranges(&item_id)
        .await
        .unwrap_or_default();
      AppCommand::IntroRanges {
        session,
        sequence,
        identity,
        ranges,
      }
    });
  }

  fn apply_intro_action(
    &mut self,
    identity: PlaybackIdentity,
    action: IntroUiAction,
    sender: &ComponentSender<Self>,
  ) {
    if !auxiliary_settlement_is_current(
      identity.session,
      &identity,
      self.requests.session_generation(),
      self.playback.identity.as_ref(),
    ) {
      return;
    }
    match action {
      IntroUiAction::Seek { target, .. } => {
        self.start_playback(PlaybackRequest::Seek(target), sender);
      }
      IntroUiAction::Prompt { range_index, kind } => {
        self.start_playback(
          PlaybackRequest::ShowText {
            identity,
            text: format!(
              "{} available — use the JellyPilot skip-intro shortcut",
              intro_skip_label(kind)
            ),
            duration_ms: 3000,
            prompt_range: Some(range_index),
          },
          sender,
        );
      }
      IntroUiAction::ManualSkip {
        range_index,
        kind,
        seek_target,
      } => {
        if self
          .playback
          .intro_skip
          .active_prompt
          .as_ref()
          .is_some_and(|prompt| prompt.range_index == range_index)
        {
          self.playback.intro_skip.active_prompt = None;
        }
        self.start_playback(PlaybackRequest::Seek(seek_target), sender);
        self.start_playback(
          PlaybackRequest::ShowText {
            identity,
            text: format!("Skipped {}", intro_skip_label(kind).to_lowercase()),
            duration_ms: 1500,
            prompt_range: None,
          },
          sender,
        );
      }
    }
  }

  fn set_intro_mode(&mut self, selected: u32) {
    let mode = config_intro_mode(selected);
    let mut prefill = config::load();
    prefill.intro_mode = mode;
    match config::save(&prefill) {
      Ok(()) => {
        self.intro_mode = mode;
        if mode == config::IntroMode::Off {
          disable_intro_skip(&mut self.playback.intro_skip);
        }
        self.ui.intro_skip_status.set_label("");
        self.ui.intro_skip_status.set_visible(false);
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Config,
          "Intro Skip preference was saved.",
        );
      }
      Err(_) => {
        self.record_diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Config,
          "The Intro Skip preference could not be saved.",
        );
        self
          .ui
          .intro_skip_status
          .set_label("The Intro Skip preference could not be saved.");
        self.ui.intro_skip_status.set_visible(true);
        self
          .ui
          .intro_skip_mode
          .set_selected(intro_mode_selection(self.intro_mode));
      }
    }
  }

  fn reconnect_remote_control(&mut self, sender: &ComponentSender<Self>) {
    if !matches!(self.connection, ConnectionPhase::Connected) {
      return;
    }
    self.record_diagnostic(
      DiagnosticLevel::Info,
      DiagnosticCategory::RemoteControl,
      "Remote-control reconnection requested from Settings.",
    );
    self.stop_remote_session(None);
    self.start_remote_session(sender);
    self.render_connection_settings();
  }

  fn refresh_connection_status(&mut self, sender: &ComponentSender<Self>) {
    if !matches!(self.connection, ConnectionPhase::Connected) {
      return;
    }
    self
      .ui
      .settings_config_status
      .set_label("Refreshing connection status…");
    self.ui.settings_config_status.set_visible(true);
    let client = Arc::clone(&self.client);
    let session = self.requests.session_generation();
    sender.oneshot_command(async move {
      AppCommand::ConnectionStatus {
        session,
        result: client.playback().validate_session().await.map_err(|_| ()),
      }
    });
  }

  fn render_connection_settings(&self) {
    let connected = matches!(self.connection, ConnectionPhase::Connected);
    let server_url = if connected {
      non_empty_setting(self.ui.server_url.text().to_string())
        .unwrap_or_else(|| "Connected server URL unavailable".to_owned())
    } else {
      "Not connected".to_owned()
    };
    self
      .ui
      .settings_server_url
      .set_label(&format!("Server URL: {server_url}"));
    let user = if connected {
      non_empty_setting(self.ui.username.text().to_string())
        .unwrap_or_else(|| "Authenticated user unavailable".to_owned())
    } else {
      "No authenticated user".to_owned()
    };
    self.ui.settings_user.set_label(&format!("User: {user}"));
    self
      .ui
      .settings_remote_status
      .set_label(match self.remote_state {
        RemoteControlState::Unavailable => "Remote Control unavailable",
        RemoteControlState::Connecting => "Remote Control connecting",
        RemoteControlState::Available => "Remote Control available",
        RemoteControlState::Lost => "Remote Control connection lost",
      });
    self
      .ui
      .settings_reconnect_button
      .set_sensitive(connected && self.client.supports_remote_control());
    self
      .ui
      .settings_refresh_status_button
      .set_sensitive(connected);
  }
  fn detect_mpv(&mut self) {
    match find_mpv() {
      Some(path) => {
        self.ui.settings_mpv_path.set_text(&path.to_string_lossy());
        self
          .ui
          .settings_mpv_status
          .set_label("MPV detected. The path applies on the next MPV start.");
        self.ui.settings_mpv_status.set_visible(true);
      }
      None => {
        self
          .ui
          .settings_mpv_status
          .set_label("MPV was not found in PATH or a standard install location.");
        self.ui.settings_mpv_status.set_visible(true);
        self.record_diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Playback,
          "MPV detection from Settings did not find an executable.",
        );
      }
    }
  }

  fn update_mpv_path(&mut self, path: String) {
    let mut settings = config::load();
    settings.mpv_path = non_empty_setting(path);
    if self.save_application_config(&settings) {
      self.reconfigure_playback_controller();
    }
  }

  fn update_mpv_args(&mut self, args: String) {
    let mut settings = config::load();
    settings.mpv_args = parse_mpv_args(&args);
    if self.save_application_config(&settings) {
      self.reconfigure_playback_controller();
    }
  }

  fn update_playback_target_name(&mut self, name: String) {
    let mut settings = config::load();
    settings.playback_target_name = non_empty_setting(name);
    self.save_application_config(&settings);
  }

  fn reconfigure_playback_controller(&mut self) {
    let settings = config::load();
    let playback_config = playback_controller_config(&settings);
    if self.playback.controller.is_none() {
      if self.playback.busy {
        self.playback.reconfigure_pending = true;
        return;
      }
      match PlaybackController::discover(Arc::clone(&self.client), playback_config) {
        Ok(controller) => {
          self.playback.controller = Some(controller);
          self.playback.unavailable = None;
          self.playback.error = None;
          self.playback.reconfigure_pending = false;
          self
            .ui
            .settings_config_status
            .set_label("Saved. MPV is available for the next playback start.");
          self.ui.settings_config_status.set_visible(true);
        }
        Err(_) => {
          self.playback.reconfigure_pending = false;
          self.show_settings_failure(
            "Settings were saved, but no MPV executable is available for the next start.",
          );
        }
      }
      return;
    }

    let result = self
      .playback
      .controller
      .as_mut()
      .map(|controller| controller.configure_for_next_start(playback_config));
    match result {
      Some(Ok(())) => {
        self.playback.reconfigure_pending = false;
        self
          .ui
          .settings_config_status
          .set_label("Saved. Player changes apply on the next MPV start.");
        self.ui.settings_config_status.set_visible(true);
      }
      Some(Err(_)) | None => {
        self.playback.reconfigure_pending = false;
        self.show_settings_failure(
          "Settings were saved, but no MPV executable is available for the next start.",
        );
      }
    }
  }

  fn add_subtitle_preset(&mut self, sender: &ComponentSender<Self>) {
    let selected = self.ui.settings_subtitle_preset.selected() as usize;
    let Some(language) = SUBTITLE_LANGUAGE_OPTIONS.get(selected) else {
      return;
    };
    self.add_subtitle_language((*language).to_owned(), sender);
  }

  fn add_custom_subtitle(&mut self, sender: &ComponentSender<Self>) {
    let language = self.ui.settings_subtitle_custom.text().to_string();
    if self.add_subtitle_language(language, sender) {
      self.ui.settings_subtitle_custom.set_text("");
    }
  }

  fn add_subtitle_language(&mut self, language: String, sender: &ComponentSender<Self>) -> bool {
    let language = language.trim().to_ascii_lowercase();
    if !valid_subtitle_language(&language) {
      self.show_settings_failure("Enter a language code using letters, numbers, '-' or '_'.");
      return false;
    }
    let mut settings = config::load();
    if settings
      .subtitle_languages
      .iter()
      .any(|existing| existing.eq_ignore_ascii_case(&language))
    {
      self.show_settings_failure("That subtitle language is already in the priority list.");
      return false;
    }
    settings.subtitle_languages.push(language);
    if self.save_application_config(&settings) {
      self.reconfigure_playback_controller();
      self.render_subtitle_settings(sender);
      true
    } else {
      false
    }
  }

  fn move_subtitle_language(&mut self, index: usize, offset: i32, sender: &ComponentSender<Self>) {
    let mut settings = config::load();
    let Ok(index_i32) = i32::try_from(index) else {
      return;
    };
    let target = index_i32.saturating_add(offset);
    let Ok(target) = usize::try_from(target) else {
      return;
    };
    if index >= settings.subtitle_languages.len() || target >= settings.subtitle_languages.len() {
      return;
    }
    settings.subtitle_languages.swap(index, target);
    if self.save_application_config(&settings) {
      self.reconfigure_playback_controller();
      self.render_subtitle_settings(sender);
    }
  }

  fn remove_subtitle_language(&mut self, index: usize, sender: &ComponentSender<Self>) {
    let mut settings = config::load();
    if index >= settings.subtitle_languages.len() {
      return;
    }
    settings.subtitle_languages.remove(index);
    if self.save_application_config(&settings) {
      self.reconfigure_playback_controller();
      self.render_subtitle_settings(sender);
    }
  }

  fn clear_subtitle_languages(&mut self, sender: &ComponentSender<Self>) {
    let mut settings = config::load();
    settings.subtitle_languages.clear();
    if self.save_application_config(&settings) {
      self.reconfigure_playback_controller();
      self.render_subtitle_settings(sender);
    }
  }

  fn render_subtitle_settings(&self, sender: &ComponentSender<Self>) {
    clear_box(&self.ui.settings_subtitle_languages);
    let settings = config::load();
    if settings.subtitle_languages.is_empty() {
      self
        .ui
        .settings_subtitle_languages
        .append(&dim_label("No subtitle language priority configured."));
      return;
    }
    let last = settings.subtitle_languages.len().saturating_sub(1);
    for (index, language) in settings.subtitle_languages.iter().enumerate() {
      let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
      let label = gtk::Label::new(Some(language));
      label.add_css_class("monospace");
      label.set_hexpand(true);
      label.set_xalign(0.0);
      row.append(&label);
      let up = gtk::Button::from_icon_name("go-up-symbolic");
      up.set_tooltip_text(Some("Move up"));
      up.set_sensitive(index > 0);
      up.connect_clicked({
        let sender = sender.clone();
        move |_| sender.input(AppMessage::MoveSubtitleLanguage { index, offset: -1 })
      });
      row.append(&up);
      let down = gtk::Button::from_icon_name("go-down-symbolic");
      down.set_tooltip_text(Some("Move down"));
      down.set_sensitive(index < last);
      down.connect_clicked({
        let sender = sender.clone();
        move |_| sender.input(AppMessage::MoveSubtitleLanguage { index, offset: 1 })
      });
      row.append(&down);
      let remove = gtk::Button::from_icon_name("edit-delete-symbolic");
      remove.set_tooltip_text(Some("Remove"));
      remove.connect_clicked({
        let sender = sender.clone();
        move |_| sender.input(AppMessage::RemoveSubtitleLanguage(index))
      });
      row.append(&remove);
      self.ui.settings_subtitle_languages.append(&row);
    }
  }

  fn update_shortcut(&mut self, kind: ShortcutKind, key: String) {
    let key = key.trim().to_owned();
    if key.is_empty() {
      self.show_settings_failure("MPV shortcut keys cannot be empty.");
      return;
    }
    let mut settings = config::load();
    if shortcut_binding_collision(&settings, kind, &key) {
      self.show_settings_failure(
        "That MPV shortcut is already assigned to another JellyPilot action.",
      );
      return;
    }
    match kind {
      ShortcutKind::Next => settings.key_next_episode = key,
      ShortcutKind::Previous => settings.key_previous_episode = key,
      ShortcutKind::IntroSkip => settings.key_intro_skip = key,
    }
    if self.save_application_config(&settings) {
      self.write_shortcut_config(&settings);
    }
  }

  fn write_shortcut_config(&mut self, settings: &LoginPrefill) {
    if write_input_conf(
      &settings.key_next_episode,
      &settings.key_previous_episode,
      &settings.key_intro_skip,
    )
    .is_some()
    {
      self
        .ui
        .settings_config_status
        .set_label("Saved. Shortcut changes apply when MPV (re)starts.");
      self.ui.settings_config_status.set_visible(true);
    } else {
      self.show_settings_failure(
        "Settings were saved, but the MPV shortcut file could not be written.",
      );
    }
  }

  fn set_image_cache_enabled(&mut self, enabled: bool) {
    let mut settings = config::load();
    let previous = settings.image_cache_enabled;
    settings.image_cache_enabled = enabled;
    if self.save_application_config(&settings) {
      self.artwork.set_disk_cache_enabled(enabled);
      self.ui.settings_config_status.set_label(if enabled {
        "Saved. Disk Library Image Cache enabled."
      } else {
        "Saved. Disk Library Image Cache disabled; memory caching remains active."
      });
      self.ui.settings_config_status.set_visible(true);
    } else {
      self.ui.settings_image_cache_syncing.set(true);
      self.ui.settings_image_cache.set_active(previous);
      self.ui.settings_image_cache_syncing.set(false);
    }
  }

  fn refresh_image_cache_stats(&mut self, sender: &ComponentSender<Self>) {
    if self.image_cache_clearing {
      return;
    }
    self.image_cache_sequence = self.image_cache_sequence.wrapping_add(1);
    let sequence = self.image_cache_sequence;
    self
      .ui
      .settings_image_cache_stats
      .set_label("Computing cache statistics…");
    self.ui.settings_image_cache_clear.set_sensitive(false);
    let artwork = Arc::clone(&self.artwork);
    sender.oneshot_command(async move {
      AppCommand::ImageCacheStats {
        sequence,
        result: artwork.disk_cache_stats().await.map_err(|_| ()),
      }
    });
  }

  fn confirm_clear_image_cache(&mut self, sender: &ComponentSender<Self>) {
    if self.image_cache_clearing {
      return;
    }
    let Some(parent) = relm4::main_adw_application().active_window() else {
      self.show_settings_failure("Library Image Cache confirmation could not be shown.");
      return;
    };
    let dialog = adw::AlertDialog::new(
      Some("Clear Library Image Cache?"),
      Some(
        "This removes best-effort original image copies. Artwork will be fetched again as needed.",
      ),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("clear", "Clear cache")]);
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    let sender = sender.clone();
    gtk::glib::spawn_future_local(async move {
      if dialog.choose_future(&parent).await.as_str() == "clear" {
        sender.input(AppMessage::ClearImageCache);
      }
    });
  }

  fn clear_image_cache(&mut self, sender: &ComponentSender<Self>) {
    if self.image_cache_clearing {
      return;
    }
    self.image_cache_clearing = true;
    self.image_cache_sequence = self.image_cache_sequence.wrapping_add(1);
    let sequence = self.image_cache_sequence;
    self
      .ui
      .settings_image_cache_stats
      .set_label("Clearing Library Image Cache…");
    self.ui.settings_image_cache_clear.set_sensitive(false);
    let artwork = Arc::clone(&self.artwork);
    sender.oneshot_command(async move {
      let result = async {
        artwork.clear_disk_cache().await.map_err(|_| ())?;
        artwork.disk_cache_stats().await.map_err(|_| ())
      }
      .await;
      AppCommand::ImageCacheCleared { sequence, result }
    });
  }

  fn render_image_cache_stats(&self, stats: ArtworkCacheStats) {
    self.ui.settings_image_cache_stats.set_label(&format!(
      "{} across {} cached image{}",
      format_byte_count(stats.bytes),
      stats.entries,
      if stats.entries == 1 { "" } else { "s" }
    ));
    self
      .ui
      .settings_image_cache_clear
      .set_sensitive(!self.image_cache_clearing && stats.entries > 0);
  }

  fn save_application_config(&mut self, settings: &LoginPrefill) -> bool {
    match config::save(settings) {
      Ok(()) => {
        self.ui.settings_config_status.set_label("Saved");
        self.ui.settings_config_status.set_visible(true);
        true
      }
      Err(_) => {
        self.show_settings_failure("Settings could not be saved.");
        false
      }
    }
  }

  fn show_settings_failure(&mut self, message: &str) {
    self.ui.settings_config_status.set_label(message);
    self.ui.settings_config_status.set_visible(true);
    self.record_diagnostic(
      DiagnosticLevel::Warning,
      DiagnosticCategory::Config,
      message,
    );
  }

  fn queue_playback_artwork(&mut self, sender: &ComponentSender<Self>) {
    self.playback_artwork_view = self.playback_artwork_view.saturating_add(1);
    self
      .ui
      .playback_artwork
      .set_paintable(None::<&gtk::gdk::Paintable>);
    self.ui.playback_artwork_fallback.set_visible(true);
    let Some(image_id) = self.playback.active_artwork_image_id.clone() else {
      return;
    };
    if let Some(decoded) = self.artwork.cached(&image_id) {
      if let Ok(texture) = decoded.texture() {
        self.ui.playback_artwork.set_paintable(Some(&texture));
        self.ui.playback_artwork_fallback.set_visible(false);
        return;
      }
    }
    let artwork = Arc::clone(&self.artwork);
    let client = Arc::clone(&self.client);
    let session = self.requests.session_generation();
    let view = self.playback_artwork_view;
    sender.oneshot_command(async move {
      let result = artwork
        .load_with_ticket(&client, &image_id, artwork.ticket())
        .await
        .map_err(|_| ());
      AppCommand::Artwork {
        session,
        view,
        slot: PLAYBACK_ARTWORK_SLOT,
        result,
      }
    });
  }

  fn begin_artwork_view(&mut self, sender: &ComponentSender<Self>) {
    self.artwork.cancel_pending();
    self.artwork_view = self.artwork_view.saturating_add(1);
    self.artwork_targets.clear();
    if self.playback.active_artwork_image_id.is_some() {
      self.queue_playback_artwork(sender);
    }
  }
  #[allow(deprecated)]
  fn artwork(
    &mut self,
    image_id: Option<&str>,
    presentation: ArtworkPresentation,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let overlay = gtk::Overlay::new();
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_keep_aspect_ratio(true);
    match presentation {
      ArtworkPresentation::Backdrop => {
        picture.set_hexpand(true);
        picture.set_size_request(-1, 220);
      }
    }
    let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    fallback.set_pixel_size(32);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    overlay.set_child(Some(&picture));
    overlay.add_overlay(&fallback);
    let Some(image_id) = image_id else {
      return overlay.upcast();
    };
    self.artwork_slot = self.artwork_slot.saturating_add(1);
    let slot = self.artwork_slot;
    self
      .artwork_targets
      .insert(slot, ArtworkTarget { picture, fallback });
    let artwork = Arc::clone(&self.artwork);
    let artwork_ticket = artwork.ticket();
    let client = Arc::clone(&self.client);
    let image_id = image_id.to_owned();
    let session = self.requests.session_generation();
    let view = self.artwork_view;
    sender.oneshot_command(async move {
      let result = artwork
        .load_with_ticket(&client, &image_id, artwork_ticket)
        .await
        .map_err(|_| ());
      AppCommand::Artwork {
        session,
        view,
        slot,
        result,
      }
    });
    overlay.upcast()
  }

  fn media_button(
    &mut self,
    item: &VideoLibraryItem,
    compact: bool,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    if compact {
      return self.poster_card(item, sender);
    }
    self.row_card(item, sender)
  }

  fn poster_card(
    &mut self,
    item: &VideoLibraryItem,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let (width, height) = card_frame_size(item);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.set_width_request(width);
    let button = gtk::Button::new();
    button.set_has_frame(false);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let artwork_overlay = gtk::Overlay::new();
    artwork_overlay.add_css_class("jellypilot-poster");
    artwork_overlay.set_overflow(gtk::Overflow::Hidden);
    artwork_overlay.set_size_request(width, height);
    let picture = cover_picture(width, height);
    let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    fallback.set_pixel_size(48);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    artwork_overlay.set_child(Some(&picture));
    artwork_overlay.add_overlay(&fallback);
    self.queue_artwork(picture, fallback, item.artwork_image_id.as_deref(), sender);
    if let Some(badge) = status_badge(item) {
      artwork_overlay.add_overlay(&badge);
    }
    if let Some(progress) = resume_progress_bar(item) {
      artwork_overlay.add_overlay(&progress);
    }
    column.append(&artwork_overlay);
    let text = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(2)
      .build();
    let title = gtk::Label::new(Some(&item.name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(18);
    text.append(&title);
    let details = dim_label(&item_caption(item));
    details.set_ellipsize(gtk::pango::EllipsizeMode::End);
    details.set_max_width_chars(18);
    text.append(&details);
    column.append(&text);
    button.set_child(Some(&column));
    let accessible_label = format!("Open details for {}", item.name);
    button.set_tooltip_text(Some(&accessible_label));
    button.update_property(&[gtk::accessible::Property::Label(&accessible_label)]);
    let selection_item = item.clone();
    let selection_sender = sender.clone();
    button.connect_clicked(move |_| {
      selection_sender.input(AppMessage::SelectItem(selection_item.clone()))
    });
    card.append(&button);
    card.upcast()
  }

  fn row_card(&mut self, item: &VideoLibraryItem, sender: &ComponentSender<Self>) -> gtk::Widget {
    let button = gtk::Button::new();
    button.set_has_frame(false);
    let row = gtk::Box::builder()
      .orientation(gtk::Orientation::Horizontal)
      .spacing(12)
      .margin_top(6)
      .margin_bottom(6)
      .margin_start(8)
      .margin_end(8)
      .build();
    let (width, height) = if is_episode_item(item) {
      (128, 72)
    } else {
      (72, 108)
    };
    let artwork_overlay = gtk::Overlay::new();
    artwork_overlay.add_css_class("jellypilot-poster");
    artwork_overlay.set_overflow(gtk::Overflow::Hidden);
    artwork_overlay.set_size_request(width, height);
    let picture = cover_picture(width, height);
    let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    fallback.set_pixel_size(32);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    artwork_overlay.set_child(Some(&picture));
    artwork_overlay.add_overlay(&fallback);
    self.queue_artwork(picture, fallback, item.artwork_image_id.as_deref(), sender);
    if let Some(badge) = status_badge(item) {
      artwork_overlay.add_overlay(&badge);
    }
    if let Some(progress) = resume_progress_bar(item) {
      artwork_overlay.add_overlay(&progress);
    }
    row.append(&artwork_overlay);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(&item.name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(64);
    text.append(&title);
    let details = dim_label(&item_caption(item));
    details.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&details);
    row.append(&text);
    if matches!(item.item_type.as_str(), "Movie" | "Episode") {
      let has_resume = item.resume_position_seconds.unwrap_or_default() > 0.0;
      let action = gtk::Button::from_icon_name("media-playback-start-symbolic");
      action.add_css_class("flat");
      action.add_css_class("suggested-action");
      action.set_valign(gtk::Align::Center);
      let action_label = if has_resume { "Resume" } else { "Play" };
      action.set_tooltip_text(Some(action_label));
      action.update_property(&[gtk::accessible::Property::Label(action_label)]);
      action.set_sensitive(self.playback.controller.is_some() && !self.playback.busy);
      let item = item.clone();
      let sender = sender.clone();
      let position = if has_resume {
        PlaybackStartPosition::Resume
      } else {
        PlaybackStartPosition::Beginning
      };
      action
        .connect_clicked(move |_| sender.input(AppMessage::PlayLibrary(item.clone(), position)));
      row.append(&action);
    }
    button.set_child(Some(&row));
    let accessible_label = format!("Open details for {}", item.name);
    button.set_tooltip_text(Some(&accessible_label));
    button.update_property(&[gtk::accessible::Property::Label(&accessible_label)]);
    let selection_item = item.clone();
    let selection_sender = sender.clone();
    button.connect_clicked(move |_| {
      selection_sender.input(AppMessage::SelectItem(selection_item.clone()))
    });
    button.upcast()
  }

  fn featured_hero(
    &mut self,
    item: &VideoLibraryItem,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let container = gtk::Overlay::new();
    container.add_css_class("jellypilot-rounded");
    container.add_css_class("jellypilot-hero");
    container.set_overflow(gtk::Overflow::Hidden);
    container.set_hexpand(true);
    container.set_size_request(-1, HOME_HERO_HEIGHT);
    let backdrop = cover_picture(-1, HOME_HERO_HEIGHT);
    let fallback = gtk::Image::from_icon_name("image-missing-symbolic");
    fallback.set_pixel_size(64);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    let backdrop_overlay = gtk::Overlay::new();
    backdrop_overlay.set_hexpand(true);
    backdrop_overlay.set_vexpand(true);
    backdrop_overlay.set_child(Some(&backdrop));
    backdrop_overlay.add_overlay(&fallback);
    container.set_child(Some(&backdrop_overlay));
    self.queue_artwork(backdrop, fallback, item.artwork_image_id.as_deref(), sender);
    let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
    scrim.add_css_class("jellypilot-hero-scrim");
    scrim.set_hexpand(true);
    scrim.set_vexpand(true);
    scrim.set_valign(gtk::Align::Fill);
    let hero_text = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(8)
      .margin_top(48)
      .margin_bottom(24)
      .margin_start(28)
      .margin_end(28)
      .valign(gtk::Align::End)
      .vexpand(true)
      .build();
    let title = gtk::Label::new(Some(&hero_headline(item)));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(60);
    hero_text.append(&title);
    let metadata = gtk::Label::new(Some(&hero_metadata(item)));
    metadata.add_css_class("dim-label");
    metadata.set_xalign(0.0);
    metadata.set_ellipsize(gtk::pango::EllipsizeMode::End);
    hero_text.append(&metadata);
    if let Some(overview) = &item.overview {
      let synopsis = gtk::Label::new(Some(overview));
      synopsis.set_xalign(0.0);
      synopsis.set_wrap(true);
      synopsis.set_wrap_mode(gtk::pango::WrapMode::WordChar);
      synopsis.set_lines(3);
      synopsis.set_ellipsize(gtk::pango::EllipsizeMode::End);
      synopsis.set_max_width_chars(80);
      synopsis.add_css_class("dim-label");
      hero_text.append(&synopsis);
    }
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let has_resume = item.resume_position_seconds.unwrap_or_default() > 0.0;
    let primary_label = if has_resume { "Resume" } else { "Play" };
    let primary = gtk::Button::with_label(primary_label);
    primary.add_css_class("suggested-action");
    primary.add_css_class("pill");
    let primary_position = if has_resume {
      PlaybackStartPosition::Resume
    } else {
      PlaybackStartPosition::Beginning
    };
    let play_item = item.clone();
    let play_sender = sender.clone();
    primary.connect_clicked(move |_| {
      play_sender.input(AppMessage::PlayLibrary(play_item.clone(), primary_position))
    });
    primary.set_sensitive(self.playback.controller.is_some() && !self.playback.busy);
    actions.append(&primary);
    let details = gtk::Button::with_label("Details");
    details.add_css_class("pill");
    details.add_css_class("osd");
    let detail_item = item.clone();
    let detail_sender = sender.clone();
    details
      .connect_clicked(move |_| detail_sender.input(AppMessage::SelectItem(detail_item.clone())));
    actions.append(&details);
    hero_text.append(&actions);
    scrim.append(&hero_text);
    container.add_overlay(&scrim);
    container.upcast()
  }

  fn queue_artwork(
    &mut self,
    picture: gtk::Picture,
    fallback: gtk::Image,
    image_id: Option<&str>,
    sender: &ComponentSender<Self>,
  ) {
    let Some(image_id) = image_id else {
      return;
    };
    self.artwork_slot = self.artwork_slot.saturating_add(1);
    let slot = self.artwork_slot;
    self
      .artwork_targets
      .insert(slot, ArtworkTarget { picture, fallback });
    let artwork = Arc::clone(&self.artwork);
    let artwork_ticket = artwork.ticket();
    let client = Arc::clone(&self.client);
    let image_id = image_id.to_owned();
    let session = self.requests.session_generation();
    let view = self.artwork_view;
    sender.oneshot_command(async move {
      let result = artwork
        .load_with_ticket(&client, &image_id, artwork_ticket)
        .await
        .map_err(|_| ());
      AppCommand::Artwork {
        session,
        view,
        slot,
        result,
      }
    });
  }

  fn media_shelf(
    &mut self,
    title: &str,
    items: &[VideoLibraryItem],
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("title-2");
    title_label.set_xalign(0.0);
    section.append(&title_label);
    if items.is_empty() {
      section.append(&dim_label("Nothing available."));
      return section.upcast();
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    for item in items {
      row.append(&self.media_button(item, true, sender));
    }
    let scroll = gtk::ScrolledWindow::builder()
      .child(&row)
      .hscrollbar_policy(gtk::PolicyType::Automatic)
      .vscrollbar_policy(gtk::PolicyType::Never)
      .propagate_natural_width(true)
      .build();
    section.append(&scroll);
    section.upcast()
  }

  fn media_grid(
    &mut self,
    items: &[VideoLibraryItem],
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let flow = gtk::FlowBox::builder()
      .selection_mode(gtk::SelectionMode::None)
      .max_children_per_line(6)
      .min_children_per_line(1)
      .row_spacing(12)
      .column_spacing(12)
      .build();
    for item in items {
      let child = gtk::FlowBoxChild::new();
      child.set_child(Some(&self.media_button(item, true, sender)));
      flow.insert(&child, -1);
    }
    flow.upcast()
  }

  fn media_list(
    &mut self,
    items: &[VideoLibraryItem],
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    for item in items {
      list.append(&self.media_button(item, false, sender));
    }
    list.upcast()
  }

  fn detail_view(
    &mut self,
    detail: &VideoItemDetail,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let backdrop_container = gtk::Overlay::new();
    let backdrop_artwork = self.artwork(
      detail
        .backdrop_image_id
        .as_deref()
        .or(detail.artwork_image_id.as_deref()),
      ArtworkPresentation::Backdrop,
      sender,
    );
    backdrop_container.set_child(Some(&backdrop_artwork));
    let gradient = gtk::Box::new(gtk::Orientation::Vertical, 0);
    gradient.add_css_class("osd");
    gradient.set_hexpand(true);
    gradient.set_vexpand(true);
    gradient.set_valign(gtk::Align::End);
    let info = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(8)
      .margin_top(20)
      .margin_bottom(20)
      .margin_start(24)
      .margin_end(24)
      .build();
    let title = gtk::Label::new(Some(&detail.name));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_wrap(true);
    info.append(&title);
    let metadata = dim_label(&detail_metadata(detail));
    metadata.set_wrap(true);
    info.append(&metadata);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let play = gtk::Button::with_label("Play");
    play.add_css_class("suggested-action");
    let item = detail.clone();
    let sender_clone = sender.clone();
    play.connect_clicked(move |_| {
      sender_clone.input(AppMessage::PlayDetail(
        item.clone(),
        PlaybackStartPosition::Beginning,
      ))
    });
    play
      .set_sensitive(self.playback.controller.is_some() && !self.playback.busy && detail.can_play);
    actions.append(&play);
    if detail.can_resume {
      let resume = gtk::Button::with_label("Resume");
      let item = detail.clone();
      let sender_clone = sender.clone();
      resume.connect_clicked(move |_| {
        sender_clone.input(AppMessage::PlayDetail(
          item.clone(),
          PlaybackStartPosition::Resume,
        ))
      });
      resume.set_sensitive(self.playback.controller.is_some() && !self.playback.busy);
      actions.append(&resume);
    }
    actions.append(&self.user_data_controls(&detail.id, detail.played, detail.favorite, sender));
    info.append(&actions);
    gradient.append(&info);
    backdrop_container.add_overlay(&gradient);
    column.append(&backdrop_container);
    let body = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(12)
      .margin_top(16)
      .margin_start(24)
      .margin_end(24)
      .margin_bottom(24)
      .build();
    if let Some(overview) = &detail.overview {
      let overview_label = gtk::Label::new(Some("Synopsis"));
      overview_label.add_css_class("heading");
      overview_label.set_xalign(0.0);
      body.append(&overview_label);
      let overview = gtk::Label::new(Some(overview));
      overview.set_xalign(0.0);
      overview.set_wrap(true);
      overview.set_selectable(true);
      body.append(&overview);
    }
    if let Some(metadata) = detail_metadata_section(&detail.metadata, &detail.genres) {
      body.append(&metadata);
    }
    body.append(&self.stream_metadata_view());
    if let Some(neighbors) = self.season_neighbors_view(detail, sender) {
      body.append(&neighbors);
    }
    if let Some(recommendations) = self.recommendations_view(sender) {
      body.append(&recommendations);
    }
    column.append(&body);
    column.upcast()
  }

  fn show_detail_view(
    &mut self,
    detail: &VideoShowDetail,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let backdrop_container = gtk::Overlay::new();
    let backdrop_artwork = self.artwork(
      detail
        .backdrop_image_id
        .as_deref()
        .or(detail.artwork_image_id.as_deref()),
      ArtworkPresentation::Backdrop,
      sender,
    );
    backdrop_container.set_child(Some(&backdrop_artwork));
    let gradient = gtk::Box::new(gtk::Orientation::Vertical, 0);
    gradient.add_css_class("osd");
    gradient.set_hexpand(true);
    gradient.set_vexpand(true);
    gradient.set_valign(gtk::Align::End);
    let info = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(8)
      .margin_top(20)
      .margin_bottom(20)
      .margin_start(24)
      .margin_end(24)
      .build();
    let title = gtk::Label::new(Some(&detail.name));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_wrap(true);
    info.append(&title);
    let metadata = dim_label(&show_detail_metadata(detail));
    metadata.set_wrap(true);
    info.append(&metadata);
    info.append(&self.user_data_controls(&detail.id, detail.played, detail.favorite, sender));
    gradient.append(&info);
    backdrop_container.add_overlay(&gradient);
    column.append(&backdrop_container);
    let body = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(12)
      .margin_top(16)
      .margin_start(24)
      .margin_end(24)
      .margin_bottom(24)
      .build();
    if let Some(overview) = &detail.overview {
      let overview_label = gtk::Label::new(Some("Synopsis"));
      overview_label.add_css_class("heading");
      overview_label.set_xalign(0.0);
      body.append(&overview_label);
      let overview = gtk::Label::new(Some(overview));
      overview.set_xalign(0.0);
      overview.set_wrap(true);
      overview.set_selectable(true);
      body.append(&overview);
    }
    if let Some(episode) = &detail.next_episode {
      let heading = gtk::Label::new(Some("Next Episode"));
      heading.add_css_class("heading");
      heading.set_xalign(0.0);
      body.append(&heading);
      body.append(&self.media_button(episode, false, sender));
    }
    let seasons_heading = gtk::Label::new(Some("Seasons"));
    seasons_heading.add_css_class("heading");
    seasons_heading.set_xalign(0.0);
    body.append(&seasons_heading);
    if detail.seasons.is_empty() {
      body.append(&dim_label("No seasons are available."));
    } else {
      let seasons = gtk::ListBox::new();
      seasons.set_selection_mode(gtk::SelectionMode::Single);
      seasons.set_activate_on_single_click(true);
      let selected_season_id = self
        .season
        .as_ref()
        .map(|selection| selection.season.id.as_str());
      let available_seasons = detail.seasons.clone();
      seasons.connect_row_activated({
        let sender = sender.clone();
        move |_, row| {
          let Ok(index) = usize::try_from(row.index()) else {
            return;
          };
          if let Some(season) = available_seasons.get(index) {
            sender.input(AppMessage::SelectSeason(season.clone()));
          }
        }
      });
      for season in &detail.seasons {
        let row = adw::ActionRow::new();
        row.set_title(&season.name);
        row.set_subtitle(
          &season
            .season_number
            .map(|number| format!("Season {number}"))
            .unwrap_or_else(|| "Season".to_owned()),
        );
        row.set_activatable(true);
        row.set_tooltip_text(Some(&format!("Browse episodes in {}", season.name)));
        seasons.append(&row);
        if selected_season_id == Some(season.id.as_str()) {
          seasons.select_row(Some(&row));
        }
      }
      body.append(&seasons);
    }
    if let Some(metadata) = detail_metadata_section(&detail.metadata, &detail.genres) {
      body.append(&metadata);
    }
    if let Some(recommendations) = self.recommendations_view(sender) {
      body.append(&recommendations);
    }
    if let Some(selection) = self.season.clone() {
      let section = self.season_episodes_view(&selection, sender);
      body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
      body.append(&section);
    }
    column.append(&body);
    column.upcast()
  }
  fn stream_metadata_view(&self) -> gtk::Widget {
    match &self.streams {
      LoadState::Idle => stream_metadata_status(),
      LoadState::Loading => loading_view("Loading audio and subtitle metadata…"),
      LoadState::Failed(message) => state_view(
        "Stream metadata unavailable",
        message,
        "dialog-warning-symbolic",
      ),
      LoadState::Ready(streams) => {
        let audio = streams.audio_streams.len();
        let subtitles = streams.subtitle_streams.len();
        state_view(
          "Audio and subtitles",
          &format!("{audio} audio stream(s) · {subtitles} subtitle stream(s) available."),
          "audio-x-generic-symbolic",
        )
      }
    }
  }

  fn season_neighbors_view(
    &mut self,
    detail: &VideoItemDetail,
    sender: &ComponentSender<Self>,
  ) -> Option<gtk::Widget> {
    let season_number = detail.season_number?;
    match self.season_neighbors.clone() {
      LoadState::Idle => None,
      LoadState::Loading => Some(loading_view(&format!(
        "Loading more from Season {season_number}…"
      ))),
      LoadState::Failed(message) => Some(state_view(
        "Season episodes unavailable",
        &message,
        "dialog-warning-symbolic",
      )),
      LoadState::Ready(items) if items.is_empty() => None,
      LoadState::Ready(items) => {
        Some(self.media_shelf(&format!("More from Season {season_number}"), &items, sender))
      }
    }
  }

  fn recommendations_view(&mut self, sender: &ComponentSender<Self>) -> Option<gtk::Widget> {
    match self.recommendations.clone() {
      LoadState::Idle => None,
      LoadState::Loading => Some(loading_view("Loading recommendations…")),
      LoadState::Failed(message) => Some(state_view(
        "Recommendations unavailable",
        &message,
        "dialog-warning-symbolic",
      )),
      LoadState::Ready(items) if items.is_empty() => None,
      LoadState::Ready(items) => Some(self.media_shelf("More like this", &items, sender)),
    }
  }

  fn user_data_controls(
    &self,
    item_id: &str,
    played: bool,
    favorite: bool,
    sender: &ComponentSender<Self>,
  ) -> gtk::Box {
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let favorite_action = if favorite {
      VideoUserDataAction::Unfavorite
    } else {
      VideoUserDataAction::Favorite
    };
    let favorite_button = gtk::Button::with_label(if favorite {
      "Remove from Favorites"
    } else {
      "Add to Favorites"
    });
    favorite_button.set_sensitive(!self.user_data_busy);
    let favorite_id = item_id.to_owned();
    let favorite_sender = sender.clone();
    favorite_button.connect_clicked(move |_| {
      favorite_sender.input(AppMessage::UpdateUserData {
        item_id: favorite_id.clone(),
        action: favorite_action,
      })
    });
    controls.append(&favorite_button);

    let played_action = if played {
      VideoUserDataAction::MarkUnplayed
    } else {
      VideoUserDataAction::MarkPlayed
    };
    let played_button = gtk::Button::with_label(if played {
      "Mark Unwatched"
    } else {
      "Mark Watched"
    });
    played_button.set_sensitive(!self.user_data_busy);
    let played_id = item_id.to_owned();
    let played_sender = sender.clone();
    played_button.connect_clicked(move |_| {
      played_sender.input(AppMessage::UpdateUserData {
        item_id: played_id.clone(),
        action: played_action,
      })
    });
    controls.append(&played_button);
    controls
  }

  fn season_episodes_view(
    &mut self,
    selection: &SeasonSelection,
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let heading_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let heading = gtk::Label::new(Some(&selection.season.name));
    heading.add_css_class("title-3");
    heading.set_hexpand(true);
    heading.set_xalign(0.0);
    let all_seasons = gtk::Button::with_label("All seasons");
    let sender_clone = sender.clone();
    all_seasons.connect_clicked(move |_| sender_clone.input(AppMessage::BackFromSeason));
    heading_row.append(&heading);
    heading_row.append(&all_seasons);
    section.append(&heading_row);
    match &selection.episodes {
      LoadState::Idle | LoadState::Loading => {
        section.append(&loading_view("Loading episodes…"));
      }
      LoadState::Failed(message) => {
        section.append(&state_view(
          "Episodes could not load",
          message,
          "dialog-error-symbolic",
        ));
        let retry = gtk::Button::with_label("Retry");
        let sender = sender.clone();
        retry.connect_clicked(move |_| sender.input(AppMessage::RetrySeason));
        section.append(&retry);
      }
      LoadState::Ready(page) if page.episodes.is_empty() => {
        let (title, message) = if page.total_record_count == 0 {
          (
            "No episodes available",
            "This season does not contain any visible episodes.",
          )
        } else {
          (
            "No episodes on this page",
            "The server returned no visible episodes for this page.",
          )
        };
        section.append(&state_view(title, message, "folder-videos-symbolic"));
        let can_go_previous = page.start_index > 0;
        let can_go_next = page.has_more && page.next_start_index > page.start_index;
        if can_go_previous || can_go_next {
          let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 8);
          navigation.set_halign(gtk::Align::Center);
          let previous = gtk::Button::with_label("Previous episode page");
          previous.set_sensitive(can_go_previous);
          let previous_sender = sender.clone();
          previous.connect_clicked(move |_| {
            previous_sender.input(AppMessage::PreviousSeasonEpisodePage);
          });
          let next = gtk::Button::with_label("Next episode page");
          next.set_sensitive(can_go_next);
          let next_sender = sender.clone();
          next.connect_clicked(move |_| {
            next_sender.input(AppMessage::NextSeasonEpisodePage);
          });
          navigation.append(&previous);
          navigation.append(&next);
          section.append(&navigation);
        }
      }
      LoadState::Ready(page) => {
        let start = page.start_index.max(0);
        let end = page
          .next_start_index
          .max(start)
          .min(page.total_record_count);
        let pagination = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let previous = gtk::Button::from_icon_name("go-previous-symbolic");
        previous.set_tooltip_text(Some("Previous episode page"));
        previous.update_property(&[gtk::accessible::Property::Label("Previous episode page")]);
        previous.set_sensitive(start > 0);
        let sender_clone = sender.clone();
        previous
          .connect_clicked(move |_| sender_clone.input(AppMessage::PreviousSeasonEpisodePage));
        let page_status = gtk::Label::new(Some(&format!(
          "Episodes {}–{} of {}",
          start.saturating_add(1),
          end,
          page.total_record_count,
        )));
        page_status.set_hexpand(true);
        page_status.set_xalign(0.5);
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        next.set_tooltip_text(Some("Next episode page"));
        next.update_property(&[gtk::accessible::Property::Label("Next episode page")]);
        next.set_sensitive(page.has_more);
        let sender_clone = sender.clone();
        next.connect_clicked(move |_| sender_clone.input(AppMessage::NextSeasonEpisodePage));
        pagination.append(&previous);
        pagination.append(&page_status);
        pagination.append(&next);
        section.append(&pagination);
        section.append(&self.media_list(&page.episodes, sender));
      }
    }
    section.upcast()
  }

  fn record_diagnostic(
    &mut self,
    level: DiagnosticLevel,
    category: DiagnosticCategory,
    message: impl AsRef<str>,
  ) {
    let change = self.diagnostics.record(level, category, message);
    self.apply_diagnostic_change(change);
  }

  fn record_artwork_failure(&mut self) {
    let change = self.diagnostics.record_coalesced(
      "artwork-load-failure",
      DiagnosticLevel::Warning,
      DiagnosticCategory::Artwork,
      "Artwork could not be loaded or decoded; a fallback is shown.",
    );
    self.apply_diagnostic_change(change);
  }

  fn apply_diagnostic_change(&self, change: DiagnosticChange) {
    match change {
      DiagnosticChange::Added { event, dropped_id } => {
        if let Some(dropped_id) = dropped_id {
          if let Some(row) = self.ui.diagnostic_rows.borrow_mut().remove(&dropped_id) {
            self.ui.diagnostics_list.remove(&row.row);
          }
        }
        self.append_diagnostic_row(&event);
        self.update_diagnostics_summary();
      }
      DiagnosticChange::Updated(event) => {
        let message = self
          .ui
          .diagnostic_rows
          .borrow()
          .get(&event.id)
          .map(|row| row.message.clone());
        if let Some(message) = message {
          message.set_label(&event.message);
        } else {
          self.render_diagnostics();
        }
      }
    }
  }

  fn render_diagnostics(&self) {
    clear_list_box(&self.ui.diagnostics_list);
    self.ui.diagnostic_rows.borrow_mut().clear();
    for event in self.diagnostics.events() {
      self.append_diagnostic_row(event);
    }
    self.update_diagnostics_summary();
  }

  fn append_diagnostic_row(&self, event: &DiagnosticEvent) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(10);
    content.set_margin_end(10);
    let time = gtk::Label::new(Some(&format_diagnostic_time(event.timestamp_seconds)));
    time.add_css_class("dim-label");
    time.add_css_class("monospace");
    time.set_valign(gtk::Align::Start);
    content.append(&time);
    let level = gtk::Label::new(Some(event.level.label()));
    level.add_css_class("caption-heading");
    level.add_css_class(match event.level {
      DiagnosticLevel::Info => "accent",
      DiagnosticLevel::Warning => "warning",
      DiagnosticLevel::Error => "error",
    });
    level.set_valign(gtk::Align::Start);
    content.append(&level);
    let category = gtk::Label::new(Some(event.category.label()));
    category.add_css_class("dim-label");
    category.set_valign(gtk::Align::Start);
    content.append(&category);
    let message = gtk::Label::new(Some(&event.message));
    message.add_css_class("monospace");
    message.set_hexpand(true);
    message.set_wrap(true);
    message.set_xalign(0.0);
    message.set_selectable(true);
    content.append(&message);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&content));
    self.ui.diagnostics_list.append(&row);
    self
      .ui
      .diagnostic_rows
      .borrow_mut()
      .insert(event.id, DiagnosticRowWidgets { row, message });
  }

  fn update_diagnostics_summary(&self) {
    let state = self.diagnostics.view_state();
    let count = match state {
      DiagnosticsViewState::Empty => 0,
      DiagnosticsViewState::Events { count } => count,
    };
    self.ui.diagnostics_count.set_label(&format!(
      "{count} sanitized runtime event{}",
      if count == 1 { "" } else { "s" }
    ));
    let populated = count > 0;
    self.ui.diagnostics_empty.set_visible(!populated);
    self.ui.diagnostics_scroll.set_visible(populated);
    self.ui.diagnostics_copy.set_sensitive(populated);
    self.ui.diagnostics_clear.set_sensitive(populated);
    if populated {
      let adjustment = self.ui.diagnostics_scroll.vadjustment();
      gtk::glib::idle_add_local_once(move || {
        adjustment.set_value((adjustment.upper() - adjustment.page_size()).max(0.0));
      });
    }
  }

  fn copy_diagnostics(&self) {
    let text = self
      .diagnostics
      .events()
      .map(|event| {
        format!(
          "[{}] {} [{}] {}",
          format_diagnostic_time(event.timestamp_seconds),
          event.level.label(),
          event.category.label(),
          sanitize_message(&event.message)
        )
      })
      .collect::<Vec<_>>()
      .join("\n");
    let Some(display) = gtk::gdk::Display::default() else {
      self
        .ui
        .diagnostics_status
        .set_label("Copy failed: no display clipboard is available.");
      self.ui.diagnostics_status.set_visible(true);
      return;
    };
    display.clipboard().set_text(&text);
    self.ui.diagnostics_status.set_label("Copied");
    self.ui.diagnostics_status.set_visible(true);
  }

  fn add_toast(&self, title: impl AsRef<str>) {
    self
      .ui
      .toast_overlay
      .add_toast(adw::Toast::new(title.as_ref()));
  }

  fn render_playback_bar(&self) {
    let snapshot = self.playback.snapshot.as_ref();
    let now_playing = snapshot.and_then(|snapshot| snapshot.now_playing.as_ref());
    let controller_available = self.playback.controller.is_some();
    self.ui.playback_bar.set_visible(now_playing.is_some());
    let title = self
      .playback
      .active_item
      .as_ref()
      .map(|item| item.name.as_str())
      .or(now_playing.map(|item| item.title.as_str()))
      .unwrap_or("");
    self.ui.playback_title.set_label(title);
    let subtitle = playback_meta_subtitle(self.playback.active_item.as_ref());
    self.ui.playback_subtitle.set_label(&subtitle);
    self.ui.playback_subtitle.set_visible(!subtitle.is_empty());
    let status = playback_bar_status(
      self.playback.error.as_deref(),
      self.playback.unavailable.as_deref(),
      self.playback.busy,
      snapshot.map(|snapshot| snapshot.transport.connected),
    );
    match status {
      Some((icon, message)) => {
        self.ui.playback_status_icon.set_icon_name(Some(icon));
        self.ui.playback_status_label.set_label(message);
        self.ui.playback_info.set_visible_child_name("status");
      }
      None => self.ui.playback_info.set_visible_child_name("meta"),
    }
    let active = now_playing.is_some() && controller_available && !self.playback.busy;
    self.ui.pause_button.set_sensitive(active);
    self.ui.stop_button.set_sensitive(active);
    self.ui.seek.set_sensitive(active);
    self.ui.volume.set_sensitive(active);
    self.ui.mute_button.set_sensitive(active);
    if let Some(snapshot) = snapshot {
      self
        .ui
        .position_label
        .set_label(&format_duration(snapshot.transport.time_pos));
      self
        .ui
        .duration_label
        .set_label(&format_duration(snapshot.transport.duration));
      self
        .ui
        .pause_button
        .set_icon_name(if snapshot.transport.paused {
          "media-playback-start-symbolic"
        } else {
          "media-playback-pause-symbolic"
        });
      self
        .ui
        .pause_button
        .set_tooltip_text(Some(if snapshot.transport.paused {
          "Resume playback"
        } else {
          "Pause playback"
        }));
      self
        .ui
        .mute_button
        .set_icon_name(if snapshot.transport.muted {
          "audio-volume-muted-symbolic"
        } else {
          "audio-volume-high-symbolic"
        });
      self
        .ui
        .mute_button
        .set_tooltip_text(Some(if snapshot.transport.muted {
          "Unmute"
        } else {
          "Mute"
        }));
      self.ui.playback_controls_syncing.set(true);
      self
        .ui
        .seek
        .set_range(0.0, snapshot.transport.duration.max(1.0));
      let position = snapshot
        .transport
        .time_pos
        .clamp(0.0, snapshot.transport.duration.max(1.0));
      if (self.ui.seek.value() - position).abs() > f64::EPSILON {
        self.ui.seek.set_value(position);
      }
      let volume = snapshot.transport.volume.clamp(0.0, 100.0);
      if (self.ui.volume.value() - volume).abs() > f64::EPSILON {
        self.ui.volume.set_value(volume);
      }
      if self.ui.mute_button.is_active() != snapshot.transport.muted {
        self.ui.mute_button.set_active(snapshot.transport.muted);
      }
      self.ui.playback_controls_syncing.set(false);
    } else {
      self.ui.position_label.set_label("00:00");
      self.ui.duration_label.set_label("00:00");
    }
    self.render_track_controls(active);
    self.render_adjacent_controls(active);
  }

  fn render_track_controls(&self, active: bool) {
    self.ui.playback_controls_syncing.set(true);
    let current_identity = self.playback.identity.as_ref();
    match &self.playback.tracks {
      PlaybackTrackState::Ready { identity, tracks } if Some(identity) == current_identity => {
        let audio = tracks
          .iter()
          .filter(|track| track.track_type == "audio")
          .collect::<Vec<_>>();
        let subtitles = tracks
          .iter()
          .filter(|track| track.track_type == "sub")
          .collect::<Vec<_>>();
        populate_track_list(
          &self.ui.audio_track_list,
          audio.iter().copied(),
          None,
          TrackKind::Audio,
          &self.ui.playback_controls_syncing,
          &self.ui.sender,
        );
        populate_track_list(
          &self.ui.subtitle_track_list,
          subtitles.iter().copied(),
          Some("Off"),
          TrackKind::Subtitle,
          &self.ui.playback_controls_syncing,
          &self.ui.sender,
        );
        let audio_available = !audio.is_empty();
        let subtitle_available = !subtitles.is_empty();
        self
          .ui
          .audio_button
          .set_sensitive(active && audio_available);
        self
          .ui
          .subtitle_button
          .set_sensitive(active && subtitle_available);
        self
          .ui
          .audio_button
          .set_tooltip_text(Some(if audio_available {
            "Select the MPV audio track"
          } else {
            "MPV reported no audio tracks."
          }));
        self
          .ui
          .subtitle_button
          .set_tooltip_text(Some(if subtitle_available {
            "Select an MPV subtitle track or turn subtitles off"
          } else {
            "MPV reported no subtitle tracks."
          }));
      }
      PlaybackTrackState::Loading { identity } if Some(identity) == current_identity => {
        self.clear_track_lists();
        self
          .ui
          .audio_button
          .set_tooltip_text(Some("Audio tracks are loading."));
        self
          .ui
          .subtitle_button
          .set_tooltip_text(Some("Subtitle tracks are loading."));
      }
      PlaybackTrackState::Failed { identity, message } if Some(identity) == current_identity => {
        self.clear_track_lists();
        self.ui.audio_button.set_tooltip_text(Some(message));
        self.ui.subtitle_button.set_tooltip_text(Some(message));
      }
      _ => {
        self.clear_track_lists();
        let reason = if self.playback.controller.is_none() {
          self
            .playback
            .unavailable
            .as_deref()
            .unwrap_or("Playback controller is unavailable.")
        } else {
          "Track controls require active playback."
        };
        self.ui.audio_button.set_tooltip_text(Some(reason));
        self.ui.subtitle_button.set_tooltip_text(Some(reason));
      }
    }
    self.ui.playback_controls_syncing.set(false);
  }

  fn clear_track_lists(&self) {
    clear_box(&self.ui.audio_track_list);
    clear_box(&self.ui.subtitle_track_list);
    self.ui.audio_button.set_sensitive(false);
    self.ui.subtitle_button.set_sensitive(false);
  }

  fn render_adjacent_controls(&self, active: bool) {
    let current = self.playback.adjacent.identity.as_ref() == self.playback.identity.as_ref()
      && self.playback.identity.is_some();
    let previous = current.then_some(&self.playback.adjacent.previous);
    let next = current.then_some(&self.playback.adjacent.next);
    let previous_available = previous
      .is_some_and(|availability| matches!(availability, AdjacentAvailability::Available(_)));
    let next_available =
      next.is_some_and(|availability| matches!(availability, AdjacentAvailability::Available(_)));
    self
      .ui
      .previous_button
      .set_sensitive(active && previous_available);
    self.ui.next_button.set_sensitive(active && next_available);
    let busy_reason = self
      .playback
      .busy
      .then_some("Another playback operation is in progress.");
    let previous_reason =
      busy_reason.unwrap_or_else(|| adjacent_control_reason(previous, AdjacentDirection::Previous));
    let next_reason =
      busy_reason.unwrap_or_else(|| adjacent_control_reason(next, AdjacentDirection::Next));
    self
      .ui
      .previous_button
      .set_tooltip_text(Some(previous_reason));
    self.ui.next_button.set_tooltip_text(Some(next_reason));
  }
}

impl Ui {
  fn new(sender: &ComponentSender<AppModel>) -> Self {
    install_media_css();
    let toast_overlay = adw::ToastOverlay::new();
    let root = adw::ToolbarView::new();
    let playback_controls_syncing = Rc::new(Cell::new(false));
    let playback_artwork = gtk::Image::new();
    playback_artwork.set_pixel_size(PLAYER_THUMB_SIZE);
    let playback_artwork_fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    playback_artwork_fallback.set_pixel_size(16);
    playback_artwork_fallback.set_halign(gtk::Align::Center);
    playback_artwork_fallback.set_valign(gtk::Align::Center);
    let artwork_frame = gtk::Overlay::new();
    artwork_frame.add_css_class("jellypilot-rounded");
    artwork_frame.add_css_class("jellypilot-playerbar-thumb");
    artwork_frame.set_overflow(gtk::Overflow::Hidden);
    artwork_frame.set_size_request(PLAYER_THUMB_SIZE, PLAYER_THUMB_SIZE);
    artwork_frame.set_valign(gtk::Align::Center);
    artwork_frame.set_child(Some(&playback_artwork));
    artwork_frame.add_overlay(&playback_artwork_fallback);
    let playback_title = gtk::Label::new(None);
    playback_title.add_css_class("heading");
    playback_title.set_xalign(0.0);
    playback_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    playback_title.set_hexpand(true);
    let playback_subtitle = dim_label("");
    playback_subtitle.set_xalign(0.0);
    playback_subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let playback_meta = gtk::Box::new(gtk::Orientation::Vertical, 0);
    playback_meta.set_valign(gtk::Align::Center);
    playback_meta.set_hexpand(true);
    playback_meta.append(&playback_title);
    playback_meta.append(&playback_subtitle);
    let playback_status_icon = gtk::Image::from_icon_name("content-loading-symbolic");
    playback_status_icon.set_pixel_size(16);
    let playback_status_label = gtk::Label::new(None);
    playback_status_label.set_xalign(0.0);
    playback_status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    playback_status_label.set_hexpand(true);
    let playback_status = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    playback_status.set_valign(gtk::Align::Center);
    playback_status.append(&playback_status_icon);
    playback_status.append(&playback_status_label);
    let playback_info = gtk::Stack::new();
    playback_info.set_hexpand(true);
    playback_info.set_hhomogeneous(true);
    playback_info.add_named(&playback_meta, Some("meta"));
    playback_info.add_named(&playback_status, Some("status"));
    playback_info.set_visible_child_name("meta");
    let previous_button = gtk::Button::from_icon_name("media-skip-backward-symbolic");
    previous_button.add_css_class("flat");
    previous_button.add_css_class("circular");
    previous_button.set_tooltip_text(Some("Previous episode is unavailable."));
    previous_button.update_property(&[gtk::accessible::Property::Label("Previous episode")]);
    previous_button.set_sensitive(false);
    previous_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::PlayAdjacent(AdjacentDirection::Previous))
    });
    let pause_button = gtk::Button::from_icon_name("media-playback-start-symbolic");
    pause_button.add_css_class("flat");
    pause_button.add_css_class("circular");
    pause_button.set_tooltip_text(Some("Pause or resume playback"));
    pause_button.update_property(&[gtk::accessible::Property::Label("Pause or resume playback")]);
    pause_button.set_sensitive(false);
    pause_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::TogglePaused)
    });
    let next_button = gtk::Button::from_icon_name("media-skip-forward-symbolic");
    next_button.add_css_class("flat");
    next_button.add_css_class("circular");
    next_button.set_tooltip_text(Some("Next episode is unavailable."));
    next_button.update_property(&[gtk::accessible::Property::Label("Next episode")]);
    next_button.set_sensitive(false);
    next_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::PlayAdjacent(AdjacentDirection::Next))
    });
    let stop_button = gtk::Button::from_icon_name("media-playback-stop-symbolic");
    stop_button.add_css_class("flat");
    stop_button.add_css_class("circular");
    stop_button.set_tooltip_text(Some("Stop playback"));
    stop_button.update_property(&[gtk::accessible::Property::Label("Stop playback")]);
    stop_button.set_sensitive(false);
    stop_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::StopPlayback)
    });
    let transport = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    transport.set_valign(gtk::Align::Center);
    transport.append(&previous_button);
    transport.append(&pause_button);
    transport.append(&next_button);
    transport.append(&stop_button);
    let position_label = playback_time_label();
    let duration_label = playback_time_label();
    let time = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    time.set_valign(gtk::Align::Center);
    time.append(&position_label);
    time.append(&gtk::Label::new(Some("/")));
    time.append(&duration_label);
    let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    seek.add_css_class("jellypilot-bar-seek");
    seek.set_draw_value(false);
    seek.set_sensitive(false);
    seek.set_hexpand(true);
    seek.set_halign(gtk::Align::Fill);
    seek.set_valign(gtk::Align::Center);
    seek.update_property(&[gtk::accessible::Property::Label("Playback position")]);
    seek.connect_change_value({
      let sender = sender.clone();
      let playback_controls_syncing = Rc::clone(&playback_controls_syncing);
      move |_, _, value| {
        if !playback_controls_syncing.get() {
          sender.input(AppMessage::Seek(value));
        }
        gtk::glib::Propagation::Proceed
      }
    });
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume.add_css_class("jellypilot-bar-volume");
    volume.set_draw_value(false);
    volume.set_sensitive(false);
    volume.set_hexpand(false);
    volume.set_vexpand(false);
    volume.set_valign(gtk::Align::Center);
    volume.set_size_request(140, -1);
    volume.connect_change_value({
      let sender = sender.clone();
      let playback_controls_syncing = Rc::clone(&playback_controls_syncing);
      move |_, _, value| {
        if !playback_controls_syncing.get() {
          sender.input(AppMessage::SetVolume(value));
        }
        gtk::glib::Propagation::Proceed
      }
    });
    let mute_button = gtk::ToggleButton::new();
    mute_button.set_icon_name("audio-volume-high-symbolic");
    mute_button.add_css_class("flat");
    mute_button.set_tooltip_text(Some("Mute"));
    mute_button.set_sensitive(false);
    mute_button.update_property(&[gtk::accessible::Property::Label("Mute")]);
    mute_button.connect_toggled({
      let sender = sender.clone();
      let playback_controls_syncing = Rc::clone(&playback_controls_syncing);
      move |button| {
        if !playback_controls_syncing.get() {
          sender.input(AppMessage::SetMuted(button.is_active()));
        }
      }
    });
    let audio_track_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    audio_track_list.add_css_class("jellypilot-track-list");
    let audio_popover = gtk::Popover::new();
    audio_popover.set_child(Some(&audio_track_list));
    let audio_button = gtk::MenuButton::new();
    audio_button.add_css_class("flat");
    audio_button.add_css_class("circular");
    audio_button.set_icon_name("audio-x-generic-symbolic");
    audio_button.set_tooltip_text(Some("Audio track"));
    audio_button.set_sensitive(false);
    audio_button.set_popover(Some(&audio_popover));
    audio_button.update_property(&[gtk::accessible::Property::Label("Audio track")]);
    let subtitle_track_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    subtitle_track_list.add_css_class("jellypilot-track-list");
    let subtitle_popover = gtk::Popover::new();
    subtitle_popover.set_child(Some(&subtitle_track_list));
    let subtitle_button = gtk::MenuButton::new();
    subtitle_button.add_css_class("flat");
    subtitle_button.add_css_class("circular");
    subtitle_button.set_icon_name("media-view-subtitles-symbolic");
    subtitle_button.set_tooltip_text(Some("Subtitle track"));
    subtitle_button.set_sensitive(false);
    subtitle_button.set_popover(Some(&subtitle_popover));
    subtitle_button.update_property(&[gtk::accessible::Property::Label("Subtitle track")]);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_margin_top(4);
    row.set_margin_bottom(8);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_valign(gtk::Align::Center);
    row.append(&artwork_frame);
    row.append(&playback_info);
    row.append(&transport);
    row.append(&time);
    row.append(&volume);
    row.append(&mute_button);
    row.append(&audio_button);
    row.append(&subtitle_button);
    let playback_bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    playback_bar.add_css_class("jellypilot-playerbar");
    playback_bar.set_visible(false);
    playback_bar.append(&seek);
    playback_bar.append(&row);
    root.add_bottom_bar(&playback_bar);
    toast_overlay.set_child(Some(&root));
    let prefill = config::load();
    let provider = adw::ComboRow::new();
    provider.set_title("Server type");
    provider.set_model(Some(&gtk::StringList::new(&["Jellyfin", "Emby"])));
    provider.set_selected(if prefill.provider.eq_ignore_ascii_case("emby") {
      1
    } else {
      0
    });
    let server_url = adw::EntryRow::new();
    server_url.set_title("Server URL");
    server_url.set_input_purpose(gtk::InputPurpose::Url);
    server_url.set_text(&prefill.server_url);
    let username = adw::EntryRow::new();
    username.set_title("Username");
    username.set_input_purpose(gtk::InputPurpose::Name);
    username.set_text(&prefill.username);
    let password = adw::PasswordEntryRow::new();
    password.set_title("Password");
    let remember_prefill = gtk::Switch::new();
    remember_prefill.set_active(prefill.remember);
    let saved_profiles = gtk::ListBox::new();
    saved_profiles.set_selection_mode(gtk::SelectionMode::None);
    saved_profiles.add_css_class("boxed-list");
    let saved_profiles_status = dim_label("Loading saved sign-ins…");
    saved_profiles_status.set_wrap(true);
    saved_profiles_status.set_accessible_role(gtk::AccessibleRole::Status);
    let login_status = dim_label("");
    login_status.set_wrap(true);
    login_status.set_visible(false);
    login_status.set_accessible_role(gtk::AccessibleRole::Status);
    let login_button = gtk::Button::with_label("Sign in");
    login_button.add_css_class("suggested-action");
    login_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::LoginRequested)
    });
    password.connect_entry_activated({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::LoginRequested)
    });
    let login_method_stack = gtk::Stack::builder()
      .transition_type(gtk::StackTransitionType::SlideLeftRight)
      .build();
    let login_method_switcher = gtk::StackSwitcher::new();
    login_method_switcher.set_stack(Some(&login_method_stack));
    login_method_switcher.set_halign(gtk::Align::Center);
    let quick_connect_code = gtk::Label::new(None);
    quick_connect_code.add_css_class("title-1");
    quick_connect_code.add_css_class("monospace");
    quick_connect_code.set_selectable(true);
    quick_connect_code.set_visible(false);
    let quick_connect_status = dim_label("");
    quick_connect_status.set_wrap(true);
    quick_connect_status.set_justify(gtk::Justification::Center);
    quick_connect_status.set_accessible_role(gtk::AccessibleRole::Status);
    let quick_connect_spinner = gtk::Spinner::new();
    quick_connect_spinner.set_visible(false);
    let quick_connect_button = gtk::Button::with_label("Request Quick Connect code");
    quick_connect_button.add_css_class("suggested-action");
    quick_connect_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::QuickConnectRequested)
    });
    let quick_connect_cancel_button = gtk::Button::with_label("Cancel request");
    quick_connect_cancel_button.set_visible(false);
    quick_connect_cancel_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::CancelQuickConnect)
    });
    let login = login_page(LoginPageWidgets {
      remember_prefill: &remember_prefill,
      provider: &provider,
      server_url: &server_url,
      username: &username,
      password: &password,
      method_stack: &login_method_stack,
      method_switcher: &login_method_switcher,
      quick_connect_code: &quick_connect_code,
      quick_connect_status: &quick_connect_status,
      quick_connect_spinner: &quick_connect_spinner,
      quick_connect: &quick_connect_button,
      cancel_quick_connect: &quick_connect_cancel_button,
      saved_profiles: &saved_profiles,
      saved_profiles_status: &saved_profiles_status,
      status: &login_status,
      sign_in: &login_button,
    });
    provider.connect_selected_notify({
      let sender = sender.clone();
      let method_stack = login_method_stack.clone();
      let method_switcher = login_method_switcher.clone();
      let quick_connect_button = quick_connect_button.clone();
      move |provider| {
        let available = quick_connect_available(provider_for(provider.selected()));
        method_switcher.set_visible(available);
        quick_connect_button.set_sensitive(available);
        if !available {
          method_stack.set_visible_child_name("password");
          sender.input(AppMessage::CancelQuickConnect);
        }
      }
    });
    if !quick_connect_available(provider_for(provider.selected())) {
      login_method_switcher.set_visible(false);
      login_method_stack.set_visible_child_name("password");
      quick_connect_button.set_sensitive(false);
    }
    root.set_content(Some(&login));

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(true);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search your media"));
    search.set_hexpand(true);
    search.set_width_chars(12);
    search.set_sensitive(false);
    search.update_property(&[gtk::accessible::Property::Label("Search your media")]);
    root.add_top_bar(&header);
    search.connect_activate({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::SearchRequested)
    });
    let window_title = gtk::Label::new(Some("JellyPilot"));
    window_title.add_css_class("title");
    header.set_title_widget(Some(&window_title));
    header.pack_start(&search);
    let connection_status = dim_label("Not connected");
    connection_status.set_accessible_role(gtk::AccessibleRole::Status);
    header.pack_end(&connection_status);
    let disconnect_button = gtk::Button::from_icon_name("system-log-out-symbolic");
    disconnect_button.set_tooltip_text(Some("Disconnect"));
    disconnect_button.set_sensitive(false);
    disconnect_button.update_property(&[gtk::accessible::Property::Label("Disconnect")]);
    disconnect_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::Disconnect)
    });
    header.pack_end(&disconnect_button);

    let sidebar = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(4)
      .margin_top(12)
      .margin_bottom(12)
      .margin_start(12)
      .margin_end(12)
      .build();
    let nav_home = navigation_button("Video Home", "go-home-symbolic");
    nav_home.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::ShowHome)
    });
    sidebar.append(&nav_home);
    sidebar.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let libraries_label = gtk::Label::new(Some("LIBRARIES"));
    libraries_label.set_xalign(0.0);
    libraries_label.add_css_class("caption");
    sidebar.append(&libraries_label);
    let shortcuts = gtk::Box::new(gtk::Orientation::Vertical, 4);
    sidebar.append(&shortcuts);
    sidebar.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let content = gtk::Stack::new();
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_transition_type(gtk::StackTransitionType::Crossfade);
    let home_content = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(24)
      .margin_top(24)
      .margin_bottom(24)
      .margin_start(24)
      .margin_end(24)
      .build();
    let home_page = scrolled_page(
      "Video Home",
      "Recently added and in-progress video from this server.",
      &home_content,
    );
    content.add_named(&home_page, Some("home"));

    let browse_title = gtk::Label::new(Some("Library"));
    browse_title.add_css_class("title-2");
    browse_title.set_xalign(0.0);
    browse_title.set_hexpand(true);
    browse_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    browse_title.set_max_width_chars(48);
    let browse_status = dim_label("");
    browse_status.set_xalign(0.0);
    browse_status.set_wrap(true);
    let grid_button = gtk::ToggleButton::new();
    grid_button.set_child(Some(&gtk::Image::from_icon_name("view-grid-symbolic")));
    grid_button.set_tooltip_text(Some("Grid view"));
    grid_button.update_property(&[gtk::accessible::Property::Label("Grid view")]);
    grid_button.set_active(true);
    grid_button.set_valign(gtk::Align::Center);
    grid_button.set_size_request(36, 32);
    let list_button = gtk::ToggleButton::new();
    list_button.set_child(Some(&gtk::Image::from_icon_name("view-list-symbolic")));
    list_button.set_tooltip_text(Some("List view"));
    list_button.update_property(&[gtk::accessible::Property::Label("List view")]);
    list_button.set_group(Some(&grid_button));
    list_button.set_valign(gtk::Align::Center);
    list_button.set_size_request(36, 32);
    grid_button.connect_toggled({
      let sender = sender.clone();
      move |button| {
        if button.is_active() {
          sender.input(AppMessage::SetBrowsePresentation(BrowsePresentation::Grid));
        }
      }
    });
    list_button.connect_toggled({
      let sender = sender.clone();
      move |button| {
        if button.is_active() {
          sender.input(AppMessage::SetBrowsePresentation(BrowsePresentation::List));
        }
      }
    });
    let load_next_button = gtk::Button::with_label("Load more");
    load_next_button.set_visible(false);
    load_next_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::LoadNextPage)
    });
    let load_previous_button = gtk::Button::with_label("Previous page");
    load_previous_button.set_visible(false);
    load_previous_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::LoadPreviousPage)
    });
    let toolbar = adw::PreferencesGroup::new();
    toolbar.set_title("Browse");
    toolbar.add(&browse_title);
    toolbar.add(&browse_status);
    let browse_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    browse_actions.append(&grid_button);
    browse_actions.append(&list_button);
    toolbar.add(&browse_actions);
    let browse_filter_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let sort_label = gtk::Label::new(Some("Sort"));
    let sort_dropdown =
      gtk::DropDown::from_strings(&["Title A–Z", "Title Z–A", "Recently added", "Release date"]);
    sort_dropdown.update_property(&[gtk::accessible::Property::Label("Sort library")]);
    sort_dropdown.connect_selected_notify({
      let sender = sender.clone();
      move |dropdown| sender.input(AppMessage::SetBrowseSort(dropdown.selected()))
    });
    let played_label = gtk::Label::new(Some("Watched"));
    let played_dropdown = gtk::DropDown::from_strings(&["All", "Unwatched", "Watched"]);
    played_dropdown.update_property(&[gtk::accessible::Property::Label("Filter watched state")]);
    played_dropdown.connect_selected_notify({
      let sender = sender.clone();
      move |dropdown| sender.input(AppMessage::SetBrowsePlayedFilter(dropdown.selected()))
    });
    let favorites_only = gtk::CheckButton::with_label("Favorites only");
    favorites_only.connect_toggled({
      let sender = sender.clone();
      move |button| sender.input(AppMessage::SetBrowseFavoritesOnly(button.is_active()))
    });
    browse_filter_bar.append(&sort_label);
    browse_filter_bar.append(&sort_dropdown);
    browse_filter_bar.append(&played_label);
    browse_filter_bar.append(&played_dropdown);
    browse_filter_bar.append(&favorites_only);
    toolbar.add(&browse_filter_bar);
    let browse_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let browse_page = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(16)
      .margin_top(24)
      .margin_bottom(24)
      .margin_start(24)
      .margin_end(24)
      .build();
    browse_page.append(&toolbar);
    browse_page.append(&browse_content);
    let pagination = adw::ActionRow::new();
    pagination.set_title("Pages");
    pagination.add_suffix(&load_previous_button);
    pagination.add_suffix(&load_next_button);
    browse_page.append(&pagination);
    let browse_scroll = gtk::ScrolledWindow::builder()
      .child(&browse_page)
      .vexpand(true)
      .build();
    content.add_named(&browse_scroll, Some("browse"));

    let detail_content = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(12)
      .margin_top(24)
      .margin_bottom(24)
      .margin_start(24)
      .margin_end(24)
      .build();
    let detail_page = scrolled_page("Item Details", "", &detail_content);
    content.add_named(&detail_page, Some("detail"));

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_title_widget(Some(&gtk::Label::new(Some("Navigation"))));
    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.add_top_bar(&sidebar_header);
    sidebar_toolbar.set_content(Some(&sidebar));
    let sidebar_page = adw::NavigationPage::new(&sidebar_toolbar, "Navigation");
    let content_toolbar = adw::ToolbarView::new();
    content_toolbar.set_content(Some(&content));
    let content_page = adw::NavigationPage::new(&content_toolbar, "Content");
    let authenticated = adw::NavigationSplitView::new();
    authenticated.set_sidebar(Some(&sidebar_page));
    authenticated.set_content(Some(&content_page));
    authenticated.set_hexpand(true);
    authenticated.set_vexpand(true);
    let settings_saved_profile = dim_label("");
    settings_saved_profile.set_wrap(true);
    let settings_storage_status = dim_label("");
    settings_storage_status.set_wrap(true);
    settings_storage_status.set_visible(false);
    settings_storage_status.set_accessible_role(gtk::AccessibleRole::Status);
    let settings_disconnect_button = gtk::Button::with_label("Disconnect");
    settings_disconnect_button.add_css_class("destructive-action");
    settings_disconnect_button.set_halign(gtk::Align::Start);
    settings_disconnect_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::Disconnect)
    });
    let intro_skip_group = adw::PreferencesGroup::new();
    intro_skip_group.set_title("Intro Skip");
    intro_skip_group.set_description(Some(
      "Automatic skips detected ranges. Manual shows an MPV prompt. Off does not fetch or apply ranges.",
    ));
    intro_skip_group.set_visible(false);
    let intro_skip_mode = adw::ComboRow::new();
    intro_skip_mode.set_title("Mode");
    intro_skip_mode.set_subtitle("Changes apply when playback next (re)starts in MPV.");
    intro_skip_mode.set_model(Some(&gtk::StringList::new(&["Automatic", "Manual", "Off"])));
    intro_skip_mode.set_selected(intro_mode_selection(prefill.intro_mode));
    intro_skip_mode.connect_selected_notify({
      let sender = sender.clone();
      move |row| sender.input(AppMessage::SetIntroMode(row.selected()))
    });
    intro_skip_group.add(&intro_skip_mode);
    let intro_skip_status = dim_label("");
    intro_skip_status.set_wrap(true);
    intro_skip_status.set_visible(false);
    let settings_config_status = dim_label("");
    settings_config_status.set_wrap(true);
    settings_config_status.set_visible(false);
    settings_config_status.set_accessible_role(gtk::AccessibleRole::Status);
    let settings_server_url = dim_label("Not connected");
    settings_server_url.set_selectable(true);
    settings_server_url.set_wrap(true);
    let settings_user = dim_label("No authenticated user");
    settings_user.set_selectable(true);
    let settings_remote_status = dim_label("Remote Control unavailable");
    let settings_reconnect_button = gtk::Button::with_label("Reconnect remote control");
    settings_reconnect_button.set_sensitive(false);
    settings_reconnect_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::ReconnectRemoteControl)
    });
    let settings_refresh_status_button = gtk::Button::with_label("Refresh status");
    settings_refresh_status_button.set_sensitive(false);
    settings_refresh_status_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::RefreshConnectionStatus)
    });
    let settings_mpv_path = adw::EntryRow::new();
    settings_mpv_path.set_title("MPV path");
    settings_mpv_path.set_text(prefill.mpv_path.as_deref().unwrap_or(""));
    settings_mpv_path.connect_changed({
      let sender = sender.clone();
      move |entry| sender.input(AppMessage::SetMpvPath(entry.text().to_string()))
    });
    let settings_detect_mpv = gtk::Button::with_label("Detect");
    settings_detect_mpv.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::DetectMpv)
    });
    let settings_mpv_status = dim_label("");
    settings_mpv_status.set_wrap(true);
    settings_mpv_status.set_visible(false);
    settings_mpv_status.set_accessible_role(gtk::AccessibleRole::Status);
    let settings_mpv_args = adw::EntryRow::new();
    settings_mpv_args.set_title("Advanced MPV arguments");
    settings_mpv_args.set_text(&prefill.mpv_args.join(" "));
    settings_mpv_args.connect_changed({
      let sender = sender.clone();
      move |entry| sender.input(AppMessage::SetMpvArgs(entry.text().to_string()))
    });
    let settings_target_name = adw::EntryRow::new();
    settings_target_name.set_title("Playback Target name");
    settings_target_name.set_text(prefill.playback_target_name.as_deref().unwrap_or(""));
    settings_target_name.connect_changed({
      let sender = sender.clone();
      move |entry| sender.input(AppMessage::SetPlaybackTargetName(entry.text().to_string()))
    });
    let settings_subtitle_languages = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let settings_subtitle_preset = gtk::DropDown::from_strings(&SUBTITLE_LANGUAGE_OPTIONS);
    let subtitle_preset_add = gtk::Button::with_label("Add selected");
    subtitle_preset_add.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::AddSubtitlePreset)
    });
    let settings_subtitle_custom = adw::EntryRow::new();
    settings_subtitle_custom.set_title("Custom language code");
    settings_subtitle_custom.connect_entry_activated({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::AddSubtitleCustom)
    });
    let subtitle_custom_add = gtk::Button::with_label("Add custom");
    subtitle_custom_add.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::AddSubtitleCustom)
    });
    let subtitle_clear = gtk::Button::with_label("Clear all");
    subtitle_clear.add_css_class("destructive-action");
    subtitle_clear.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::ClearSubtitleLanguages)
    });
    let settings_key_next = adw::EntryRow::new();
    settings_key_next.set_title("Next episode");
    settings_key_next.set_text(&prefill.key_next_episode);
    settings_key_next.connect_changed({
      let sender = sender.clone();
      move |entry| sender.input(AppMessage::SetNextEpisodeKey(entry.text().to_string()))
    });
    let settings_key_previous = adw::EntryRow::new();
    settings_key_previous.set_title("Previous episode");
    settings_key_previous.set_text(&prefill.key_previous_episode);
    settings_key_previous.connect_changed({
      let sender = sender.clone();
      move |entry| sender.input(AppMessage::SetPreviousEpisodeKey(entry.text().to_string()))
    });
    let settings_key_intro = adw::EntryRow::new();
    settings_key_intro.set_title("Skip intro");
    settings_key_intro.set_text(&prefill.key_intro_skip);
    settings_key_intro.connect_changed({
      let sender = sender.clone();
      move |entry| sender.input(AppMessage::SetIntroSkipKey(entry.text().to_string()))
    });
    let settings_image_cache_syncing = Rc::new(Cell::new(false));
    let settings_image_cache = adw::SwitchRow::new();
    settings_image_cache.set_title("Disk Library Image Cache");
    settings_image_cache.set_subtitle(
      "Stores original server image bytes for faster repeat browsing; never used as offline truth.",
    );
    settings_image_cache.set_active(prefill.image_cache_enabled);
    settings_image_cache.connect_active_notify({
      let sender = sender.clone();
      let syncing = Rc::clone(&settings_image_cache_syncing);
      move |row| {
        if !syncing.get() {
          sender.input(AppMessage::SetImageCacheEnabled(row.is_active()));
        }
      }
    });
    let settings_image_cache_stats = dim_label("Cache statistics have not been computed.");
    settings_image_cache_stats.set_wrap(true);
    settings_image_cache_stats.set_accessible_role(gtk::AccessibleRole::Status);
    let settings_image_cache_clear = gtk::Button::with_label("Clear Library Image Cache");
    settings_image_cache_clear.add_css_class("destructive-action");
    settings_image_cache_clear.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::ConfirmClearImageCache)
    });
    let diagnostics_count = dim_label("0 sanitized runtime events");
    diagnostics_count.set_xalign(0.0);
    let diagnostics_list = gtk::ListBox::new();
    diagnostics_list.set_selection_mode(gtk::SelectionMode::None);
    diagnostics_list.add_css_class("boxed-list");
    let diagnostic_rows = Rc::new(RefCell::new(HashMap::new()));
    let diagnostics_scroll = gtk::ScrolledWindow::builder()
      .child(&diagnostics_list)
      .min_content_height(360)
      .vexpand(true)
      .build();
    diagnostics_scroll.set_visible(false);
    let diagnostics_empty = dim_label("No diagnostic events yet");
    diagnostics_empty.set_xalign(0.0);
    diagnostics_empty.set_wrap(true);
    let diagnostics_status = dim_label("");
    diagnostics_status.set_visible(false);
    diagnostics_status.set_accessible_role(gtk::AccessibleRole::Status);
    let diagnostics_copy = gtk::Button::with_label("Copy diagnostics");
    diagnostics_copy.set_sensitive(false);
    diagnostics_copy.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::CopyDiagnostics)
    });
    let diagnostics_clear = gtk::Button::with_label("Clear");
    diagnostics_clear.add_css_class("destructive-action");
    diagnostics_clear.set_sensitive(false);
    diagnostics_clear.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::ClearDiagnostics)
    });
    let diagnostics_page = diagnostics_page(
      &diagnostics_count,
      &diagnostics_scroll,
      &diagnostics_empty,
      &diagnostics_status,
      &diagnostics_copy,
      &diagnostics_clear,
    );
    intro_skip_status.set_accessible_role(gtk::AccessibleRole::Status);
    intro_skip_group.add(&intro_skip_status);
    let forget_current_profile = gtk::Button::with_label("Sign out and forget");
    forget_current_profile.add_css_class("destructive-action");
    forget_current_profile.set_sensitive(false);
    forget_current_profile.connect_clicked({
      let sender = sender.clone();
      move |_| {
        // The model resolves the current key at message time so stale widgets never retain tokens.
        sender.input(AppMessage::ForgetCurrentProfile)
      }
    });
    let preferences = settings_page(
      SettingsPageWidgets {
        config_status: &settings_config_status,
        server_url: &settings_server_url,
        user: &settings_user,
        remote_status: &settings_remote_status,
        disconnect: &settings_disconnect_button,
        reconnect: &settings_reconnect_button,
        refresh_status: &settings_refresh_status_button,
        saved_profile: &settings_saved_profile,
        storage_status: &settings_storage_status,
        forget_saved_profile: &forget_current_profile,
        mpv_path: &settings_mpv_path,
        detect_mpv: &settings_detect_mpv,
        mpv_status: &settings_mpv_status,
        mpv_args: &settings_mpv_args,
        target_name: &settings_target_name,
        subtitle_languages: &settings_subtitle_languages,
        subtitle_preset: &settings_subtitle_preset,
        subtitle_preset_add: &subtitle_preset_add,
        subtitle_custom: &settings_subtitle_custom,
        subtitle_custom_add: &subtitle_custom_add,
        subtitle_clear: &subtitle_clear,
        key_next: &settings_key_next,
        key_previous: &settings_key_previous,
        key_intro: &settings_key_intro,
        image_cache: &settings_image_cache,
        image_cache_stats: &settings_image_cache_stats,
        image_cache_clear: &settings_image_cache_clear,
        intro_skip: &intro_skip_group,
      },
      &diagnostics_page,
    );
    let about = adw::AboutDialog::new();
    about.set_application_name("JellyPilot");
    about.set_application_icon("video-x-generic");
    about.set_version(env!("CARGO_PKG_VERSION"));
    about.set_comments("A native media client for Jellyfin and Emby.");
    about.set_website("https://github.com/hewel/jellypilot");
    let application = relm4::main_adw_application();
    let preferences_action = gtk::gio::SimpleAction::new("preferences", None);
    preferences_action.connect_activate({
      let preferences = preferences.clone();
      let sender = sender.clone();
      move |_, _| {
        sender.input(AppMessage::RefreshDiagnostics);
        sender.input(AppMessage::RefreshImageCacheStats);
        let parent = relm4::main_adw_application().active_window();
        preferences.present(parent.as_ref());
      }
    });
    application.add_action(&preferences_action);
    let about_action = gtk::gio::SimpleAction::new("about", None);
    about_action.connect_activate({
      let about = about.clone();
      move |_, _| {
        let parent = relm4::main_adw_application().active_window();
        about.present(parent.as_ref());
      }
    });
    application.add_action(&about_action);
    let quit_action = gtk::gio::SimpleAction::new("quit", None);
    quit_action.connect_activate({
      let sender = sender.clone();
      move |_, _| sender.input(AppMessage::QuitRequested)
    });
    application.add_action(&quit_action);
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Preferences"), Some("app.preferences"));
    menu.append(Some("About JellyPilot"), Some("app.about"));
    menu.append(Some("Quit"), Some("app.quit"));
    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_tooltip_text(Some("Main menu"));
    menu_button.set_menu_model(Some(&menu));
    header.pack_end(&menu_button);

    Self {
      toast_overlay,
      root,
      login,
      provider,
      server_url,
      username,
      password,
      remember_prefill,
      login_method_switcher,
      quick_connect_code,
      quick_connect_status,
      quick_connect_spinner,
      quick_connect_button,
      quick_connect_cancel_button,
      saved_profiles,
      saved_profiles_status,
      login_status,
      login_button,
      authenticated,
      connection_status,
      search,
      playback_bar,
      playback_artwork,
      playback_artwork_fallback,
      playback_title,
      playback_subtitle,
      playback_status_icon,
      playback_status_label,
      playback_info,
      disconnect_button,
      content,
      nav_home,
      shortcuts,
      home_content,
      browse_title,
      browse_status,
      browse_content,
      browse_filter_bar,
      sort_dropdown,
      played_dropdown,
      favorites_only,
      grid_button,
      list_button,
      load_previous_button,
      load_next_button,
      browse_scroll,
      detail_content,
      position_label,
      duration_label,
      previous_button,
      pause_button,
      next_button,
      stop_button,
      seek,
      volume,
      mute_button,
      audio_button,
      subtitle_button,
      audio_track_list,
      subtitle_track_list,
      playback_controls_syncing,
      sender: sender.clone(),
      settings_saved_profile,
      settings_storage_status,
      settings_disconnect_button,
      intro_skip_group,
      intro_skip_mode,
      intro_skip_status,
      settings_config_status,
      settings_server_url,
      settings_user,
      settings_remote_status,
      settings_reconnect_button,
      settings_refresh_status_button,
      settings_mpv_path,
      settings_mpv_status,
      settings_subtitle_languages,
      settings_subtitle_preset,
      settings_subtitle_custom,
      settings_image_cache,
      settings_image_cache_syncing,
      settings_image_cache_stats,
      settings_image_cache_clear,
      diagnostics_list,
      diagnostic_rows,
      diagnostics_empty,
      diagnostics_count,
      diagnostics_scroll,
      diagnostics_copy,
      diagnostics_clear,
      diagnostics_status,
      forget_current_profile,
      preferences,
    }
  }
}

struct LoginPageWidgets<'a> {
  remember_prefill: &'a gtk::Switch,
  provider: &'a adw::ComboRow,
  server_url: &'a adw::EntryRow,
  username: &'a adw::EntryRow,
  password: &'a adw::PasswordEntryRow,
  method_stack: &'a gtk::Stack,
  method_switcher: &'a gtk::StackSwitcher,
  quick_connect_code: &'a gtk::Label,
  quick_connect_status: &'a gtk::Label,
  quick_connect_spinner: &'a gtk::Spinner,
  quick_connect: &'a gtk::Button,
  cancel_quick_connect: &'a gtk::Button,
  saved_profiles: &'a gtk::ListBox,
  saved_profiles_status: &'a gtk::Label,
  status: &'a gtk::Label,
  sign_in: &'a gtk::Button,
}

fn login_page(widgets: LoginPageWidgets<'_>) -> gtk::ScrolledWindow {
  let LoginPageWidgets {
    remember_prefill,
    provider,
    server_url,
    username,
    password,
    method_stack,
    method_switcher,
    quick_connect_code,
    quick_connect_status,
    quick_connect_spinner,
    quick_connect,
    cancel_quick_connect,
    saved_profiles,
    saved_profiles_status,
    status,
    sign_in,
  } = widgets;
  let page = gtk::Box::new(gtk::Orientation::Vertical, 24);
  page.set_halign(gtk::Align::Center);
  page.set_margin_top(32);
  page.set_margin_bottom(32);
  let branding = gtk::Box::new(gtk::Orientation::Vertical, 8);
  branding.set_halign(gtk::Align::Center);
  let icon = gtk::Image::from_icon_name("video-x-generic-symbolic");
  icon.set_pixel_size(48);
  branding.append(&icon);
  let title = gtk::Label::new(Some("JellyPilot"));
  title.add_css_class("title-1");
  branding.append(&title);
  let subtitle = dim_label("Connect to your Jellyfin or Emby server.");
  subtitle.set_justify(gtk::Justification::Center);
  subtitle.set_wrap(true);
  branding.append(&subtitle);
  page.append(&branding);

  let server_group = adw::PreferencesGroup::new();
  server_group.set_title("Server");
  server_group.add(provider);
  server_group.add(server_url);
  page.append(&server_group);
  method_switcher.set_halign(gtk::Align::Center);
  page.append(method_switcher);

  let quick_connect_page = gtk::Box::new(gtk::Orientation::Vertical, 14);
  let quick_copy = dim_label(
    "Request a code, then approve it from another client already signed in to this Jellyfin server.",
  );
  quick_copy.set_wrap(true);
  quick_copy.set_justify(gtk::Justification::Center);
  quick_connect_page.append(&quick_copy);
  quick_connect_code.set_halign(gtk::Align::Center);
  quick_connect_page.append(quick_connect_code);
  quick_connect_spinner.set_halign(gtk::Align::Center);
  quick_connect_page.append(quick_connect_spinner);
  quick_connect_status.set_justify(gtk::Justification::Center);
  quick_connect_status.set_wrap(true);
  quick_connect_page.append(quick_connect_status);
  quick_connect.set_halign(gtk::Align::Center);
  quick_connect_page.append(quick_connect);
  cancel_quick_connect.set_halign(gtk::Align::Center);
  quick_connect_page.append(cancel_quick_connect);
  method_stack.add_titled(&quick_connect_page, Some("quick-connect"), "Quick Connect");

  let password_page = gtk::Box::new(gtk::Orientation::Vertical, 14);
  let credentials_group = adw::PreferencesGroup::new();
  credentials_group.add(username);
  credentials_group.add(password);
  let remember_row = adw::ActionRow::new();
  remember_row.set_title("Remember sign-in details");
  remember_row.set_subtitle("Save server, provider, and username on this device.");
  remember_row.add_suffix(remember_prefill);
  remember_row.set_activatable_widget(Some(remember_prefill));
  credentials_group.add(&remember_row);
  password_page.append(&credentials_group);
  let storage_copy = dim_label(
    "Successful sign-ins are saved in Linux Secret Service. JellyPilot never stores your password.",
  );
  storage_copy.set_wrap(true);
  password_page.append(&storage_copy);
  sign_in.set_hexpand(true);
  password_page.append(sign_in);
  method_stack.add_titled(&password_page, Some("password"), "Password");
  method_stack.set_visible_child_name("quick-connect");
  page.append(method_stack);
  status.set_halign(gtk::Align::Center);
  status.set_wrap(true);
  page.append(status);

  let saved_group = adw::PreferencesGroup::new();
  saved_group.set_title("Saved sign-ins");
  let saved_profiles_scroll = gtk::ScrolledWindow::builder()
    .child(saved_profiles)
    .max_content_height(240)
    .propagate_natural_height(true)
    .hscrollbar_policy(gtk::PolicyType::Never)
    .build();
  saved_group.add(&saved_profiles_scroll);
  saved_group.add(saved_profiles_status);
  page.append(&saved_group);

  let clamp = adw::Clamp::new();
  clamp.set_maximum_size(620);
  clamp.set_child(Some(&page));
  gtk::ScrolledWindow::builder()
    .child(&clamp)
    .hscrollbar_policy(gtk::PolicyType::Never)
    .vexpand(true)
    .build()
}

fn saved_profile_row(
  profile: &SavedProfileSummary,
  sender: &ComponentSender<AppModel>,
) -> adw::ActionRow {
  let action = adw::ActionRow::new();
  let provider = match profile.provider {
    MediaServerProvider::Jellyfin => "Jellyfin",
    MediaServerProvider::Emby => "Emby",
  };
  let server = profile
    .server_name
    .as_deref()
    .unwrap_or(profile.server_url.as_str());
  action.set_title(&format!("{}@{}", profile.user_name, server));
  action.set_subtitle(provider);
  action.set_activatable(true);
  action.update_property(&[gtk::accessible::Property::Label(&format!(
    "Restore saved sign-in for {} on {}",
    profile.user_name, server
  ))]);
  let key = profile.key.clone();
  let sender_clone = sender.clone();
  action.connect_activated(move |_| {
    sender_clone.input(AppMessage::RestoreSavedProfile(key.clone()));
  });
  let forget = gtk::Button::with_label("Forget");
  forget.add_css_class("destructive-action");
  forget.update_property(&[gtk::accessible::Property::Label(&format!(
    "Forget saved sign-in for {} on {}",
    profile.user_name, server
  ))]);
  let key = profile.key.clone();
  let sender_clone = sender.clone();
  forget.connect_clicked(move |_| {
    sender_clone.input(AppMessage::ForgetSavedProfile(key.clone()));
  });
  action.add_suffix(&forget);
  action
}

fn diagnostics_page(
  count: &gtk::Label,
  scroll: &gtk::ScrolledWindow,
  empty: &gtk::Label,
  status: &gtk::Label,
  copy: &gtk::Button,
  clear: &gtk::Button,
) -> adw::PreferencesPage {
  let page = adw::PreferencesPage::new();
  page.set_title("Diagnostics");
  page.set_icon_name(Some("dialog-information-symbolic"));
  let group = adw::PreferencesGroup::new();
  group.set_title("Sanitized runtime events");
  group.set_description(Some(
    "Connection, authentication, playback, remote-control, artwork, and configuration events useful for support.",
  ));
  let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
  content.append(count);
  content.append(empty);
  content.append(scroll);
  content.append(status);
  let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  actions.set_halign(gtk::Align::End);
  actions.append(copy);
  actions.append(clear);
  content.append(&actions);
  group.add(&content);
  page.add(&group);
  page
}

struct SettingsPageWidgets<'a> {
  config_status: &'a gtk::Label,
  server_url: &'a gtk::Label,
  user: &'a gtk::Label,
  remote_status: &'a gtk::Label,
  disconnect: &'a gtk::Button,
  reconnect: &'a gtk::Button,
  refresh_status: &'a gtk::Button,
  saved_profile: &'a gtk::Label,
  storage_status: &'a gtk::Label,
  forget_saved_profile: &'a gtk::Button,
  mpv_path: &'a adw::EntryRow,
  detect_mpv: &'a gtk::Button,
  mpv_status: &'a gtk::Label,
  mpv_args: &'a adw::EntryRow,
  target_name: &'a adw::EntryRow,
  subtitle_languages: &'a gtk::Box,
  subtitle_preset: &'a gtk::DropDown,
  subtitle_preset_add: &'a gtk::Button,
  subtitle_custom: &'a adw::EntryRow,
  subtitle_custom_add: &'a gtk::Button,
  subtitle_clear: &'a gtk::Button,
  key_next: &'a adw::EntryRow,
  key_previous: &'a adw::EntryRow,
  key_intro: &'a adw::EntryRow,
  image_cache: &'a adw::SwitchRow,
  image_cache_stats: &'a gtk::Label,
  image_cache_clear: &'a gtk::Button,
  intro_skip: &'a adw::PreferencesGroup,
}

fn settings_page(
  widgets: SettingsPageWidgets<'_>,
  diagnostics: &adw::PreferencesPage,
) -> adw::PreferencesDialog {
  let dialog = adw::PreferencesDialog::new();
  dialog.set_title("Preferences");
  let page = adw::PreferencesPage::new();
  page.set_title("JellyPilot");
  let SettingsPageWidgets {
    config_status,
    server_url,
    user,
    remote_status,
    disconnect,
    reconnect,
    refresh_status,
    saved_profile,
    storage_status,
    forget_saved_profile,
    mpv_path,
    detect_mpv,
    mpv_status,
    mpv_args,
    target_name,
    subtitle_languages,
    subtitle_preset,
    subtitle_preset_add,
    subtitle_custom,
    subtitle_custom_add,
    subtitle_clear,
    key_next,
    key_previous,
    key_intro,
    image_cache,
    image_cache_stats,
    image_cache_clear,
    intro_skip,
  } = widgets;

  let status_group = adw::PreferencesGroup::new();
  status_group.add(config_status);
  page.add(&status_group);

  let connection_group = adw::PreferencesGroup::new();
  connection_group.set_title("Connection");
  connection_group.set_description(Some(
    "Live authenticated-session and Remote Control status.",
  ));
  connection_group.add(server_url);
  connection_group.add(user);
  connection_group.add(remote_status);
  let connection_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  connection_actions.append(disconnect);
  connection_actions.append(reconnect);
  connection_actions.append(refresh_status);
  connection_group.add(&connection_actions);
  page.add(&connection_group);

  let player_group = adw::PreferencesGroup::new();
  player_group.set_title("Player");
  player_group.set_description(Some(
    "MPV path, advanced arguments, and subtitle priorities apply on the next MPV start. Playback Target name applies to newly established sessions.",
  ));
  player_group.add(mpv_path);
  let detect_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  detect_row.append(detect_mpv);
  detect_row.append(mpv_status);
  player_group.add(&detect_row);
  player_group.add(mpv_args);
  player_group.add(target_name);
  page.add(&player_group);

  let subtitles_group = adw::PreferencesGroup::new();
  subtitles_group.set_title("Subtitles");
  subtitles_group.set_description(Some(
    "Ordered MPV subtitle-language priority for newly started playback.",
  ));
  subtitles_group.add(subtitle_languages);
  let preset_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  preset_row.append(subtitle_preset);
  preset_row.append(subtitle_preset_add);
  subtitles_group.add(&preset_row);
  subtitles_group.add(subtitle_custom);
  let subtitle_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  subtitle_actions.append(subtitle_custom_add);
  subtitle_actions.append(subtitle_clear);
  subtitles_group.add(&subtitle_actions);
  page.add(&subtitles_group);

  let shortcuts_group = adw::PreferencesGroup::new();
  shortcuts_group.set_title("Shortcuts");
  shortcuts_group.set_description(Some(
    "JellyPilot MPV bindings are saved immediately and apply when MPV (re)starts.",
  ));
  shortcuts_group.add(key_next);
  shortcuts_group.add(key_previous);
  shortcuts_group.add(key_intro);
  page.add(&shortcuts_group);

  let library_group = adw::PreferencesGroup::new();
  library_group.set_title("Library");
  library_group.set_description(Some(
    "The disk cache is best-effort acceleration for original Library Image bytes, not an offline artwork source. Capacity is bounded to 512 MiB.",
  ));
  library_group.add(image_cache);
  library_group.add(image_cache_stats);
  image_cache_clear.set_halign(gtk::Align::Start);
  library_group.add(image_cache_clear);
  page.add(&library_group);

  page.add(intro_skip);

  let session_group = adw::PreferencesGroup::new();
  session_group.set_title("Session");
  session_group.set_description(Some(
    "Saved sign-ins remain available until they are forgotten.",
  ));
  session_group.add(saved_profile);
  session_group.add(storage_status);
  forget_saved_profile.set_halign(gtk::Align::Start);
  session_group.add(forget_saved_profile);
  page.add(&session_group);

  dialog.add(&page);
  dialog.add(diagnostics);
  dialog
}

fn detail_metadata_section(
  metadata: &VideoDetailMetadata,
  genres: &[String],
) -> Option<gtk::Widget> {
  let rating = match (&metadata.community_rating, &metadata.official_rating) {
    (Some(community), Some(official)) => format!("Community rating {community:.1} · {official}"),
    (Some(community), None) => format!("Community rating {community:.1}"),
    (None, Some(official)) => official.clone(),
    (None, None) => String::new(),
  };
  if rating.is_empty()
    && genres.is_empty()
    && metadata.creators.is_empty()
    && metadata.cast.is_empty()
  {
    return None;
  }
  let group = adw::PreferencesGroup::new();
  group.set_title("Details");
  if !rating.is_empty() {
    group.add(&dim_label(&rating));
  }
  if !genres.is_empty() {
    group.add(&dim_label(&format!("Genres: {}", genres.join(", "))));
  }
  if !metadata.creators.is_empty() {
    group.add(&dim_label(&format!(
      "Creators: {}",
      metadata.creators.join(", ")
    )));
  }
  if !metadata.cast.is_empty() {
    let cast = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for name in metadata.cast.iter().take(12) {
      let label = gtk::Label::new(Some(name));
      label.add_css_class("caption");
      label.set_max_width_chars(18);
      label.set_ellipsize(gtk::pango::EllipsizeMode::End);
      cast.append(&label);
    }
    let cast_scroll = gtk::ScrolledWindow::builder()
      .child(&cast)
      .hscrollbar_policy(gtk::PolicyType::Automatic)
      .vscrollbar_policy(gtk::PolicyType::Never)
      .build();
    group.add(&cast_scroll);
  }
  Some(group.upcast())
}

fn stream_metadata_status() -> gtk::Widget {
  state_view(
    "Audio and subtitles",
    "Stream metadata is available when playback starts; no stream details were requested yet.",
    "audio-x-generic-symbolic",
  )
}

fn scrolled_page(title: &str, subtitle: &str, content: &gtk::Box) -> gtk::Widget {
  let page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(18)
    .margin_top(24)
    .margin_bottom(24)
    .margin_start(24)
    .margin_end(24)
    .build();
  let title = gtk::Label::new(Some(title));
  title.add_css_class("title-1");
  title.set_xalign(0.0);
  page.append(&title);
  if !subtitle.is_empty() {
    let subtitle = dim_label(subtitle);
    subtitle.set_wrap(true);
    page.append(&subtitle);
  }
  page.append(content);
  let clamp = adw::Clamp::new();
  clamp.set_maximum_size(960);
  clamp.set_child(Some(&page));
  let scroll = gtk::ScrolledWindow::builder()
    .child(&clamp)
    .vexpand(true)
    .build();
  scroll.upcast()
}

fn state_view(title: &str, copy: &str, icon_name: &str) -> gtk::Widget {
  let status = adw::StatusPage::new();
  status.set_title(title);
  status.set_description(Some(copy));
  status.set_icon_name(Some(icon_name));
  status.set_vexpand(true);
  status.upcast()
}

fn loading_view(copy: &str) -> gtk::Widget {
  let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
  column.set_halign(gtk::Align::Center);
  column.set_valign(gtk::Align::Center);
  column.set_accessible_role(gtk::AccessibleRole::Status);
  let spinner = gtk::Spinner::new();
  spinner.start();
  column.append(&spinner);
  column.append(&dim_label(copy));
  column.upcast()
}

fn navigation_button(label: &str, icon: &str) -> gtk::ToggleButton {
  let button = gtk::ToggleButton::new();
  button.set_halign(gtk::Align::Fill);
  button.add_css_class("flat");
  let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  let icon = gtk::Image::from_icon_name(icon);
  let label = gtk::Label::new(Some(label));
  label.set_xalign(0.0);
  row.append(&icon);
  row.append(&label);
  button.set_child(Some(&row));
  button
}

fn dim_label(text: &str) -> gtk::Label {
  let label = gtk::Label::new(Some(text));
  label.add_css_class("dim-label");
  label.set_xalign(0.0);
  label
}

fn playback_time_label() -> gtk::Label {
  let label = gtk::Label::new(Some("00:00"));
  label.add_css_class("dim-label");
  label.add_css_class("monospace");
  label
}

fn playback_meta_subtitle(item: Option<&MediaItem>) -> String {
  let Some(item) = item else {
    return String::new();
  };
  if !item.item_type.eq_ignore_ascii_case("episode") {
    return String::new();
  }
  let series = item
    .series_name
    .as_deref()
    .map(str::trim)
    .filter(|name| !name.is_empty());
  match (series, item.parent_index_number, item.index_number) {
    (Some(series), Some(season), Some(episode)) => {
      format!("{series} · S{season} E{episode}")
    }
    (Some(series), _, _) => series.to_owned(),
    (_, Some(season), Some(episode)) => format!("S{season} E{episode} · {}", item.name),
    _ => item.name.clone(),
  }
}

fn library_shortcut_caption(shortcut: &VideoLibraryShortcut) -> String {
  let kind = match library_kind(&shortcut.collection_type) {
    VideoLibraryKind::TvShows => "TV Shows",
    VideoLibraryKind::Movies => "Movies",
  };
  match shortcut.item_count {
    Some(count) => format!("{kind} · {count}"),
    None => kind.to_owned(),
  }
}

fn clear_box(container: &gtk::Box) {
  while let Some(child) = container.first_child() {
    container.remove(&child);
  }
}

fn clear_list_box(container: &gtk::ListBox) {
  while let Some(child) = container.first_child() {
    container.remove(&child);
  }
}

async fn run_auth_operation<T, F>(operation: F) -> Result<T, ()>
where
  T: Send + 'static,
  F: FnOnce() -> T + Send + 'static,
{
  let (sender, receiver) = oneshot::channel();
  std::thread::Builder::new()
    .name("jellypilot-secret-service".to_string())
    .spawn(move || {
      let _ = sender.send(operation());
    })
    .map_err(|_| ())?;
  receiver.await.map_err(|_| ())
}

fn install_media_css() {
  let Some(display) = gtk::gdk::Display::default() else {
    return;
  };
  let provider = gtk::CssProvider::new();
  provider.load_from_string(include_str!("../style.css"));
  gtk::style_context_add_provider_for_display(
    &display,
    &provider,
    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
  );
}
async fn quick_connect_workflow(
  client: Arc<JellyfinClient>,
  server_url: String,
  session: SessionToken,
  output: relm4::Sender<AppCommand>,
  poll_interval: Duration,
  workflow_timeout: Duration,
) {
  let command_client = Arc::clone(&client);
  let result = async {
    let request = command_client
      .login()
      .quick_connect_start(&server_url)
      .await
      .map_err(|error| quick_connect_start_message(&error).to_owned())?;
    let (code, secret) = request.into_parts();
    let secret = Zeroizing::new(secret);
    if output
      .send(AppCommand::QuickConnectCode { session, code })
      .is_err()
    {
      return Err("Quick Connect was cancelled.".to_owned());
    }

    relm4::tokio::time::timeout(workflow_timeout, async {
      loop {
        relm4::tokio::time::sleep(poll_interval).await;
        match command_client
          .login()
          .quick_connect_check(&server_url, secret.as_str())
          .await
          .map_err(|_| quick_connect_check_message().to_owned())?
        {
          QuickConnectStatus::Waiting => {}
          QuickConnectStatus::Approved => break Ok(()),
        }
      }
    })
    .await
    .unwrap_or_else(|_| Err(quick_connect_timeout_message().to_owned()))?;

    if output
      .send(AppCommand::QuickConnectApproving { session })
      .is_err()
    {
      return Err("Quick Connect was cancelled.".to_owned());
    }
    let mut response = command_client
      .login()
      .quick_connect_authenticate(&server_url, secret.as_str())
      .await
      .map_err(|_| quick_connect_authentication_message().to_owned())?;
    response.access_token.zeroize();
    Ok(())
  }
  .await;

  let _ = output.send(AppCommand::Login {
    session,
    client,
    result,
  });
}

const fn quick_connect_available(provider: MediaServerProvider) -> bool {
  matches!(provider, MediaServerProvider::Jellyfin)
}

const fn quick_connect_start_message(error: &JellyfinError) -> &'static str {
  match error {
    JellyfinError::QuickConnectUnavailable => {
      "Quick Connect is not enabled on this Jellyfin server. Use password sign-in instead."
    }
    JellyfinError::InvalidUrl(_) => "Enter a valid Jellyfin server URL to request a code.",
    _ => "Quick Connect could not be started. Check the server address and try again.",
  }
}

const fn quick_connect_check_message() -> &'static str {
  "JellyPilot could not check this Quick Connect code. Request a new code and try again."
}

const fn quick_connect_authentication_message() -> &'static str {
  "JellyPilot could not finish the approved Quick Connect sign-in. Request a new code and try again."
}

const fn quick_connect_timeout_message() -> &'static str {
  "Quick Connect code expired. Request a new code to try again."
}

const fn can_start_login(connection: ConnectionPhase, playback_cleanup_pending: bool) -> bool {
  matches!(
    connection,
    ConnectionPhase::SignedOut | ConnectionPhase::Failed
  ) && !playback_cleanup_pending
}

const fn should_disconnect_after_forget(
  sign_out: bool,
  operation_session: u64,
  current_session: u64,
  connection: ConnectionPhase,
  active_profile_matches: bool,
) -> bool {
  sign_out
    && operation_session == current_session
    && matches!(connection, ConnectionPhase::Connected)
    && active_profile_matches
}

const fn quit_can_finish_without_controller(
  playback_busy: bool,
  playback_cleanup_pending: bool,
) -> bool {
  !playback_busy && !playback_cleanup_pending
}

const fn shutdown_completion_quits(
  quitting: bool,
  disposition: PlaybackShutdownDisposition,
) -> bool {
  quitting || matches!(disposition, PlaybackShutdownDisposition::Quit)
}

const fn playback_cleanup_required(
  cleanup_pending: bool,
  controller_owned: bool,
  command_busy: bool,
) -> bool {
  cleanup_pending || controller_owned || command_busy
}

const fn stale_playback_disposition(
  quitting: bool,
  connection: ConnectionPhase,
  playback_cleanup_pending: bool,
) -> PlaybackShutdownDisposition {
  if quitting {
    PlaybackShutdownDisposition::Quit
  } else if matches!(connection, ConnectionPhase::SignedOut) || playback_cleanup_pending {
    PlaybackShutdownDisposition::Disconnect
  } else {
    PlaybackShutdownDisposition::Detached
  }
}

fn provider_for(selected: u32) -> MediaServerProvider {
  if selected == 1 {
    MediaServerProvider::Emby
  } else {
    MediaServerProvider::Jellyfin
  }
}

fn library_kind(collection_type: &str) -> VideoLibraryKind {
  if collection_type.eq_ignore_ascii_case("tvshows") || collection_type.eq_ignore_ascii_case("tv") {
    VideoLibraryKind::TvShows
  } else {
    VideoLibraryKind::Movies
  }
}

fn browse_preferences(
  sort_selection: u32,
  played_selection: u32,
  favorites_only: bool,
) -> BrowsePreferences {
  let (sort, sort_direction) = match sort_selection {
    1 => (
      VideoLibrarySort::Title,
      VideoLibrarySortDirection::Descending,
    ),
    2 => (
      VideoLibrarySort::RecentlyAdded,
      VideoLibrarySortDirection::Descending,
    ),
    3 => (
      VideoLibrarySort::ReleaseDate,
      VideoLibrarySortDirection::Descending,
    ),
    _ => (
      VideoLibrarySort::Title,
      VideoLibrarySortDirection::Ascending,
    ),
  };
  let played_filter = match played_selection {
    1 => VideoLibraryPlayedFilter::Unplayed,
    2 => VideoLibraryPlayedFilter::Played,
    _ => VideoLibraryPlayedFilter::All,
  };
  BrowsePreferences {
    sort,
    sort_direction,
    played_filter,
    favorites_only,
  }
}

fn apply_user_data_update(
  detail: &mut LoadState<DetailContent>,
  update: &VideoUserDataUpdate,
) -> bool {
  match detail {
    LoadState::Ready(DetailContent::Item(item)) if item.id == update.item_id => {
      item.played = update.played;
      item.favorite = update.favorite;
      true
    }
    LoadState::Ready(DetailContent::Show(show)) if show.id == update.item_id => {
      show.played = update.played;
      show.favorite = update.favorite;
      true
    }
    _ => false,
  }
}

fn connection_label(client: &JellyfinClient) -> String {
  let state = client.login().connection_state();
  match (state.server_name, state.user_name) {
    (Some(server), Some(user)) => format!("Connected to {server} as {user}"),
    (Some(server), None) => format!("Connected to {server}"),
    _ => "Connected".to_owned(),
  }
}

fn is_episode_item(item: &VideoLibraryItem) -> bool {
  item.item_type.eq_ignore_ascii_case("Episode")
}

fn card_frame_size(item: &VideoLibraryItem) -> (i32, i32) {
  if is_episode_item(item) {
    (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT)
  } else {
    (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT)
  }
}

fn cover_picture(width: i32, height: i32) -> gtk::Picture {
  let picture = gtk::Picture::new();
  picture.set_can_shrink(true);
  picture.set_content_fit(gtk::ContentFit::Cover);
  picture.set_hexpand(true);
  picture.set_vexpand(true);
  picture.set_halign(gtk::Align::Fill);
  picture.set_valign(gtk::Align::Fill);
  picture.set_size_request(width, height);
  picture
}

fn item_caption(item: &VideoLibraryItem) -> String {
  match item.production_year {
    Some(year) => format!("{year} · {}", item.item_type),
    None => item.item_type.clone(),
  }
}

fn hero_headline(item: &VideoLibraryItem) -> String {
  if is_episode_item(item) {
    item
      .series_name
      .as_deref()
      .map(str::trim)
      .filter(|name| !name.is_empty())
      .map(ToOwned::to_owned)
      .unwrap_or_else(|| item.name.clone())
  } else {
    item.name.clone()
  }
}

fn hero_metadata(item: &VideoLibraryItem) -> String {
  if is_episode_item(item) {
    match (item.season_number, item.episode_number) {
      (Some(season), Some(number)) => format!("S{season} E{number} · {}", item.name),
      _ => format!("Episode · {}", item.name),
    }
  } else {
    item_caption(item)
  }
}

fn status_badge(item: &VideoLibraryItem) -> Option<gtk::Label> {
  let text = if item.played {
    "Played"
  } else if item.favorite {
    "Favorite"
  } else {
    return None;
  };
  let badge = gtk::Label::new(Some(text));
  badge.add_css_class("jellypilot-badge");
  badge.set_halign(gtk::Align::End);
  badge.set_valign(gtk::Align::Start);
  Some(badge)
}

fn resume_progress_bar(item: &VideoLibraryItem) -> Option<gtk::ProgressBar> {
  let percentage = item
    .played_percentage
    .filter(|value| *value > 0.0 && *value < 100.0)?;
  let progress = gtk::ProgressBar::new();
  progress.set_fraction(percentage / 100.0);
  progress.set_show_text(false);
  progress.set_valign(gtk::Align::End);
  progress.set_hexpand(true);
  progress.add_css_class("jellypilot-progress-overlay");
  Some(progress)
}

fn detail_metadata(detail: &VideoItemDetail) -> String {
  let mut details = Vec::new();
  if let Some(year) = detail.production_year {
    details.push(year.to_string());
  }
  details.push(detail.item_type.clone());
  if !detail.genres.is_empty() {
    details.push(detail.genres.join(", "));
  }
  if detail.favorite {
    details.push("Favorite".to_owned());
  }
  details.join(" · ")
}

fn playback_notice(notice: Option<String>, warnings: &[PlaybackWarning]) -> Option<String> {
  let warning = (!warnings.is_empty()).then(|| {
    let details = warnings
      .iter()
      .map(ToString::to_string)
      .collect::<Vec<_>>()
      .join("; ");
    format!("Playback is active, but {details}.")
  });
  match (notice, warning) {
    (Some(notice), Some(warning)) => Some(format!("{notice}\n{warning}")),
    (Some(notice), None) => Some(notice),
    (None, warning) => warning,
  }
}

fn shortcut_binding_collision(
  settings: &LoginPrefill,
  kind: ShortcutKind,
  candidate: &str,
) -> bool {
  let other_bindings = match kind {
    ShortcutKind::Next => [
      settings.key_previous_episode.as_str(),
      settings.key_intro_skip.as_str(),
    ],
    ShortcutKind::Previous => [
      settings.key_next_episode.as_str(),
      settings.key_intro_skip.as_str(),
    ],
    ShortcutKind::IntroSkip => [
      settings.key_next_episode.as_str(),
      settings.key_previous_episode.as_str(),
    ],
  };
  other_bindings
    .iter()
    .any(|binding| binding.trim().eq_ignore_ascii_case(candidate.trim()))
}

fn non_empty_setting(value: String) -> Option<String> {
  let value = value.trim();
  (!value.is_empty()).then(|| value.to_owned())
}

fn parse_mpv_args(value: &str) -> Vec<String> {
  value.split_whitespace().map(str::to_owned).collect()
}

fn valid_subtitle_language(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 16
    && value
      .chars()
      .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn configured_client(settings: &LoginPrefill) -> Arc<JellyfinClient> {
  let client = Arc::new(JellyfinClient::new());
  if let Some(name) = settings
    .playback_target_name
    .as_deref()
    .map(str::trim)
    .filter(|name| !name.is_empty())
  {
    client.set_device_name(name.to_owned());
  }
  client
}

fn configured_mpv_args(settings: &LoginPrefill) -> Vec<String> {
  let mut args = settings.mpv_args.clone();
  if !settings.subtitle_languages.is_empty() && !has_mpv_option(&args, "slang") {
    args.push(format!("--slang={}", settings.subtitle_languages.join(",")));
  }
  args
}

fn playback_controller_config(settings: &LoginPrefill) -> PlaybackControllerConfig {
  let mut playback =
    PlaybackControllerConfig::default().with_extra_args(configured_mpv_args(settings));
  if let Some(path) = settings
    .mpv_path
    .as_deref()
    .map(str::trim)
    .filter(|path| !path.is_empty())
  {
    playback = playback.with_mpv_path(PathBuf::from(path));
  }
  playback
}

const fn config_intro_mode(selected: u32) -> config::IntroMode {
  match selected {
    1 => config::IntroMode::Manual,
    2 => config::IntroMode::Off,
    _ => config::IntroMode::Automatic,
  }
}

const fn intro_mode_selection(mode: config::IntroMode) -> u32 {
  match mode {
    config::IntroMode::Automatic => 0,
    config::IntroMode::Manual => 1,
    config::IntroMode::Off => 2,
  }
}

const fn session_intro_mode(mode: config::IntroMode) -> IntroSkipMode {
  match mode {
    config::IntroMode::Automatic => IntroSkipMode::Automatic,
    config::IntroMode::Manual => IntroSkipMode::Manual,
    config::IntroMode::Off => IntroSkipMode::Off,
  }
}

fn should_fetch_intro_ranges(
  mode: config::IntroMode,
  capability_available: bool,
  item_type: &str,
) -> bool {
  mode != config::IntroMode::Off && capability_available && item_type == "Episode"
}

fn evaluate_intro_ui_action(
  position_seconds: f64,
  ranges: &mut [IntroSkipRange],
  mode: IntroSkipMode,
  manual_requested: bool,
  active_prompt_range: Option<usize>,
) -> Option<IntroUiAction> {
  let range_index = ranges.iter().position(|range| {
    !range.skipped
      && position_seconds.is_finite()
      && position_seconds >= range.start_seconds
      && position_seconds < range.end_seconds
  })?;
  let range = std::slice::from_mut(&mut ranges[range_index]);
  if manual_requested {
    if mode != IntroSkipMode::Manual || active_prompt_range != Some(range_index) {
      return None;
    }
    return evaluate_manual_skip(position_seconds, range).map(|decision| {
      IntroUiAction::ManualSkip {
        range_index,
        kind: decision.kind,
        seek_target: decision.seek_target,
      }
    });
  }
  evaluate_intro_skip(position_seconds, range, mode).map(|action| match action {
    IntroSkipAction::Seek(target) => IntroUiAction::Seek {
      range_index,
      target,
    },
    IntroSkipAction::ShowPrompt(kind) => IntroUiAction::Prompt { range_index, kind },
  })
}

fn active_intro_prompt_range(
  prompt: &mut Option<ActiveIntroPrompt>,
  now: Instant,
) -> Option<usize> {
  if prompt
    .as_ref()
    .is_some_and(|prompt| now < prompt.expires_at)
  {
    prompt.as_ref().map(|prompt| prompt.range_index)
  } else {
    *prompt = None;
    None
  }
}

fn disable_intro_skip(state: &mut IntroSkipState) {
  state.sequence = state.sequence.wrapping_add(1);
  state.mode = IntroSkipMode::Off;
  state.ranges.clear();
  state.active_prompt = None;
}

const fn intro_skip_label(kind: IntroSkipKind) -> &'static str {
  match kind {
    IntroSkipKind::Introduction => "Intro",
    IntroSkipKind::Credits => "Credits",
  }
}

fn manual_intro_skip_requested(messages: &[String]) -> bool {
  messages
    .iter()
    .any(|message| message == "jellypilot-skip-intro")
}

fn adjacent_direction_from_client_messages(messages: &[String]) -> Option<AdjacentDirection> {
  messages.iter().find_map(|message| match message.as_str() {
    "jellypilot-next" => Some(AdjacentDirection::Next),
    "jellypilot-prev" => Some(AdjacentDirection::Previous),
    _ => None,
  })
}

fn auxiliary_settlement_is_current(
  operation_session: u64,
  identity: &PlaybackIdentity,
  current_session: u64,
  current_identity: Option<&PlaybackIdentity>,
) -> bool {
  operation_session == current_session
    && identity.session == operation_session
    && current_identity == Some(identity)
}

#[cfg_attr(not(test), allow(dead_code))]
fn selected_track_id(tracks: &[TrackInfo], kind: TrackKind, selected: u32) -> Option<Option<i64>> {
  let track_type = match kind {
    TrackKind::Audio => "audio",
    TrackKind::Subtitle => "sub",
  };
  if kind == TrackKind::Subtitle && selected == 0 {
    return Some(None);
  }
  let index = if kind == TrackKind::Subtitle {
    selected.checked_sub(1)?
  } else {
    selected
  };
  tracks
    .iter()
    .filter(|track| track.track_type == track_type)
    .nth(index as usize)
    .map(|track| Some(track.id))
}

fn track_label(track: &TrackInfo) -> String {
  match (track.title.as_deref(), track.language.as_deref()) {
    (Some(title), Some(language)) if !title.eq_ignore_ascii_case(language) => {
      format!("{title} · {language}")
    }
    (Some(title), _) => title.to_owned(),
    (None, Some(language)) => language.to_owned(),
    (None, None) => format!("Track {}", track.id),
  }
}

fn populate_track_list<'a>(
  list: &gtk::Box,
  tracks: impl Iterator<Item = &'a TrackInfo>,
  off_label: Option<&str>,
  kind: TrackKind,
  syncing: &Rc<Cell<bool>>,
  sender: &ComponentSender<AppModel>,
) {
  clear_box(list);
  let tracks = tracks.collect::<Vec<_>>();
  let off_selected = off_label.is_some() && tracks.iter().all(|track| !track.selected);
  let mut group = None;
  if let Some(label) = off_label {
    let off = gtk::CheckButton::with_label(label);
    off.set_active(off_selected);
    off.connect_toggled({
      let sender = sender.clone();
      let syncing = Rc::clone(syncing);
      move |button| {
        if !syncing.get() && button.is_active() {
          sender.input(AppMessage::SelectSubtitleTrack(None));
        }
      }
    });
    group = Some(off.clone());
    list.append(&off);
  }
  for track in tracks {
    let row = gtk::CheckButton::with_label(&track_label(track));
    if let Some(group) = &group {
      row.set_group(Some(group));
    } else {
      group = Some(row.clone());
    }
    row.set_active(track.selected);
    let id = track.id;
    row.connect_toggled({
      let sender = sender.clone();
      let syncing = Rc::clone(syncing);
      move |button| {
        if !syncing.get() && button.is_active() {
          match kind {
            TrackKind::Audio => sender.input(AppMessage::SelectAudioTrack(id)),
            TrackKind::Subtitle => sender.input(AppMessage::SelectSubtitleTrack(Some(id))),
          }
        }
      }
    });
    list.append(&row);
  }
}

fn playback_bar_status<'a>(
  error: Option<&'a str>,
  unavailable: Option<&'a str>,
  busy: bool,
  connected: Option<bool>,
) -> Option<(&'static str, &'a str)> {
  if let Some(error) = error {
    return Some(("dialog-error-symbolic", error));
  }
  if let Some(unavailable) = unavailable {
    return Some(("dialog-warning-symbolic", unavailable));
  }
  if connected == Some(false) {
    return Some(("network-offline-symbolic", "Connection lost"));
  }
  if busy {
    return Some(("content-loading-symbolic", "Buffering…"));
  }
  None
}

fn adjacent_availability(
  direction: AdjacentDirection,
  result: Result<Option<MediaItem>, String>,
) -> AdjacentAvailability {
  match result {
    Ok(Some(item)) => AdjacentAvailability::Available(item),
    Ok(None) => AdjacentAvailability::Unavailable(
      match direction {
        AdjacentDirection::Previous => "No previous episode is available.",
        AdjacentDirection::Next => "No next episode is available.",
      }
      .to_owned(),
    ),
    Err(message) => AdjacentAvailability::Unavailable(message),
  }
}

fn adjacent_control_reason(
  availability: Option<&AdjacentAvailability>,
  direction: AdjacentDirection,
) -> &str {
  match availability {
    Some(AdjacentAvailability::Loading) => "Checking adjacent episodes…",
    Some(AdjacentAvailability::Available(_)) => match direction {
      AdjacentDirection::Previous => "Play previous episode",
      AdjacentDirection::Next => "Play next episode",
    },
    Some(AdjacentAvailability::Unavailable(message)) => message,
    Some(AdjacentAvailability::Idle) | None => {
      "Episode navigation requires active episode playback."
    }
  }
}

fn media_item_from_library(item: &VideoLibraryItem) -> MediaItem {
  MediaItem {
    id: item.id.clone(),
    name: item.name.clone(),
    item_type: item.item_type.clone(),
    series_id: item.series_id.clone(),
    series_name: item.series_name.clone(),
    season_name: None,
    index_number: item.episode_number,
    parent_index_number: item.season_number,
    run_time_ticks: runtime_seconds_to_ticks(item.runtime_seconds),
    overview: item.overview.clone(),
    series_primary_image_tag: None,
  }
}

fn media_item_from_detail(item: &VideoItemDetail) -> MediaItem {
  MediaItem {
    id: item.id.clone(),
    name: item.name.clone(),
    item_type: item.item_type.clone(),
    series_id: item.series_id.clone(),
    series_name: item.series_name.clone(),
    season_name: None,
    index_number: item.episode_number,
    parent_index_number: item.season_number,
    run_time_ticks: runtime_seconds_to_ticks(item.runtime_seconds),
    overview: item.overview.clone(),
    series_primary_image_tag: None,
  }
}

fn runtime_seconds_to_ticks(seconds: Option<f64>) -> Option<i64> {
  seconds
    .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
    .map(|seconds| (seconds * 10_000_000.0).round() as i64)
}

fn playback_failure(context: &str, error: PlaybackError) -> PlaybackCommandFailure {
  PlaybackCommandFailure {
    message: format!("{context}: {error}."),
    clear_snapshot: false,
  }
}

fn playback_start_failure(error: PlaybackError) -> PlaybackCommandFailure {
  PlaybackCommandFailure {
    message: format!("Could not start playback: {error}."),
    clear_snapshot: matches!(
      error,
      PlaybackError::MpvStartFailed | PlaybackError::MpvLoadFailed
    ),
  }
}

fn format_byte_count(bytes: u64) -> String {
  const KIB: f64 = 1024.0;
  const MIB: f64 = KIB * 1024.0;
  const GIB: f64 = MIB * 1024.0;
  let bytes = bytes as f64;
  if bytes >= GIB {
    format!("{:.1} GiB", bytes / GIB)
  } else if bytes >= MIB {
    format!("{:.1} MiB", bytes / MIB)
  } else if bytes >= KIB {
    format!("{:.1} KiB", bytes / KIB)
  } else {
    format!("{bytes:.0} B")
  }
}

fn format_diagnostic_time(timestamp_seconds: u64) -> String {
  i64::try_from(timestamp_seconds)
    .ok()
    .and_then(|timestamp| gtk::glib::DateTime::from_unix_utc(timestamp).ok())
    .and_then(|timestamp| timestamp.format("%Y-%m-%d %H:%M:%S UTC").ok())
    .map(|timestamp| timestamp.to_string())
    .unwrap_or_else(|| format!("{timestamp_seconds} UTC"))
}

fn format_duration(seconds: f64) -> String {
  let seconds = seconds.max(0.0).round() as u64;
  format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn show_detail_metadata(detail: &VideoShowDetail) -> String {
  let mut details = Vec::new();
  if let Some(year) = detail.production_year {
    details.push(year.to_string());
  }
  details.push("Series".to_owned());
  if !detail.genres.is_empty() {
    details.push(detail.genres.join(", "));
  }
  if detail.favorite {
    details.push("Favorite".to_owned());
  }
  details.join(" · ")
}

fn season_page_request(
  series_id: &str,
  season: &VideoSeason,
  start_index: i32,
) -> VideoSeasonEpisodesPageRequest {
  VideoSeasonEpisodesPageRequest {
    series_id: series_id.to_owned(),
    season_id: Some(season.id.clone()),
    season_number: season.season_number,
    start_index: start_index.max(0),
    limit: SEASON_EPISODE_PAGE_SIZE,
  }
}

pub(crate) fn run(smoke_test: bool) {
  let app = RelmApp::new(if smoke_test { SMOKE_APP_ID } else { APP_ID });
  if smoke_test {
    app.allow_multiple_instances(true);
  }
  if smoke_test {
    app
      .with_args(vec!["jellypilot-gtk-smoke".to_owned()])
      .run::<AppModel>(true);
  } else {
    app.run::<AppModel>(false);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn quick_connect_is_available_only_for_jellyfin() {
    assert!(quick_connect_available(MediaServerProvider::Jellyfin));
    assert!(!quick_connect_available(MediaServerProvider::Emby));
  }

  #[test]
  fn quick_connect_command_debug_redacts_the_public_code() {
    let mut gate = RequestGate::default();
    let session = gate.begin_login();
    let command = AppCommand::QuickConnectCode {
      session,
      code: "ABCD12".to_owned(),
    };

    let debug = format!("{command:?}");
    assert!(!debug.contains("ABCD12"));
    assert!(debug.contains("[redacted]"));
  }

  #[test]
  fn quick_connect_user_messages_do_not_include_transport_details() {
    let error = JellyfinError::HttpError(
      "https://media.example/QuickConnect/Connect?secret=private-secret".to_owned(),
    );

    let messages = [
      quick_connect_start_message(&error),
      quick_connect_check_message(),
      quick_connect_authentication_message(),
      quick_connect_timeout_message(),
    ];
    assert!(messages
      .iter()
      .all(|message| !message.contains("private-secret") && !message.contains("https://")));
    assert_eq!(
      quick_connect_timeout_message(),
      "Quick Connect code expired. Request a new code to try again."
    );
  }

  #[test]
  fn season_page_request_uses_exact_identity_and_a_bounded_window() {
    let season = VideoSeason {
      id: "season-2".to_owned(),
      name: "Season 2".to_owned(),
      season_number: Some(2),
      played: false,
      favorite: false,
      artwork_image_id: None,
    };

    let request = season_page_request("show-1", &season, 60);

    assert_eq!(request.series_id, "show-1");
    assert_eq!(request.season_id.as_deref(), Some("season-2"));
    assert_eq!(request.season_number, Some(2));
    assert_eq!(request.start_index, 60);
    assert_eq!(request.limit, 30);
  }

  #[test]
  fn saved_profile_deletion_only_signs_out_the_originating_live_session() {
    assert!(should_disconnect_after_forget(
      true,
      4,
      4,
      ConnectionPhase::Connected,
      true,
    ));
    assert!(!should_disconnect_after_forget(
      true,
      4,
      5,
      ConnectionPhase::Connected,
      true,
    ));
    assert!(!should_disconnect_after_forget(
      true,
      4,
      4,
      ConnectionPhase::Connected,
      false,
    ));
  }

  #[test]
  fn browse_controls_map_to_provider_neutral_preferences() {
    let preferences = browse_preferences(2, 1, true);

    assert!(matches!(preferences.sort, VideoLibrarySort::RecentlyAdded));
    assert!(matches!(
      preferences.sort_direction,
      VideoLibrarySortDirection::Descending
    ));
    assert!(matches!(
      preferences.played_filter,
      VideoLibraryPlayedFilter::Unplayed
    ));
    assert!(preferences.favorites_only);
  }

  #[test]
  fn user_data_completion_updates_only_the_matching_detail() {
    let mut detail = LoadState::Ready(DetailContent::Show(VideoShowDetail {
      id: "show-1".to_owned(),
      name: "Show".to_owned(),
      overview: None,
      production_year: None,
      genres: Vec::new(),
      played: false,
      favorite: false,
      can_play: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      next_episode: None,
      seasons: Vec::new(),
      metadata: Default::default(),
    }));
    let stale = VideoUserDataUpdate {
      item_id: "show-2".to_owned(),
      played: true,
      favorite: true,
    };
    assert!(!apply_user_data_update(&mut detail, &stale));
    let current = VideoUserDataUpdate {
      item_id: "show-1".to_owned(),
      played: true,
      favorite: true,
    };
    assert!(apply_user_data_update(&mut detail, &current));
    assert!(matches!(
      detail,
      LoadState::Ready(DetailContent::Show(VideoShowDetail {
        played: true,
        favorite: true,
        ..
      }))
    ));
  }

  #[test]
  fn login_is_blocked_while_connecting_connected_or_cleaning_playback() {
    assert!(!can_start_login(ConnectionPhase::Connecting, false));
    assert!(!can_start_login(ConnectionPhase::Connected, false));
    assert!(!can_start_login(ConnectionPhase::SignedOut, true));
    assert!(can_start_login(ConnectionPhase::SignedOut, false));
    assert!(can_start_login(ConnectionPhase::Failed, false));
  }

  #[test]
  fn only_failed_mpv_start_or_load_clears_the_previous_snapshot() {
    assert!(playback_start_failure(PlaybackError::MpvStartFailed).clear_snapshot);
    assert!(playback_start_failure(PlaybackError::MpvLoadFailed).clear_snapshot);
    assert!(!playback_start_failure(PlaybackError::PlaybackInfoUnavailable).clear_snapshot);
  }

  #[test]
  fn quit_waits_for_disconnect_cleanup_and_finishes_on_its_completion() {
    assert!(!quit_can_finish_without_controller(false, true));
    assert!(!quit_can_finish_without_controller(true, false));
    assert!(quit_can_finish_without_controller(false, false));
    assert!(shutdown_completion_quits(
      true,
      PlaybackShutdownDisposition::Disconnect,
    ));
    assert!(shutdown_completion_quits(
      false,
      PlaybackShutdownDisposition::Quit,
    ));
    assert!(!shutdown_completion_quits(
      false,
      PlaybackShutdownDisposition::Detached,
    ));
  }
  #[test]
  fn remote_volume_accepts_wire_string_and_number_forms() {
    assert_eq!(
      remote_volume_value(Some(&serde_json::json!("50"))),
      Some(50.0)
    );
    assert_eq!(
      remote_volume_value(Some(&serde_json::json!(125))),
      Some(100.0)
    );
    assert_eq!(
      remote_volume_value(Some(&serde_json::json!("invalid"))),
      None
    );
  }

  #[test]
  fn disconnect_requires_cleanup_for_owned_or_in_flight_controller() {
    assert!(!playback_cleanup_required(false, false, false));
    assert!(playback_cleanup_required(false, true, false));
    assert!(playback_cleanup_required(false, false, true));
    assert!(playback_cleanup_required(true, false, false));
  }

  #[test]
  fn stale_playback_from_disconnected_session_is_shut_down_before_relogin() {
    assert_eq!(
      stale_playback_disposition(false, ConnectionPhase::SignedOut, true),
      PlaybackShutdownDisposition::Disconnect,
    );
    assert_eq!(
      stale_playback_disposition(false, ConnectionPhase::Connected, false),
      PlaybackShutdownDisposition::Detached,
    );
    assert_eq!(
      stale_playback_disposition(true, ConnectionPhase::Connected, false),
      PlaybackShutdownDisposition::Quit,
    );
  }
  #[test]
  fn track_selection_maps_filtered_rows_and_subtitle_off() {
    let tracks = vec![
      TrackInfo {
        id: 3,
        track_type: "audio".to_owned(),
        title: Some("English".to_owned()),
        language: Some("eng".to_owned()),
        selected: true,
      },
      TrackInfo {
        id: 8,
        track_type: "sub".to_owned(),
        title: Some("Spanish".to_owned()),
        language: Some("spa".to_owned()),
        selected: false,
      },
    ];

    assert_eq!(
      selected_track_id(&tracks, TrackKind::Audio, 0),
      Some(Some(3))
    );
    assert_eq!(
      selected_track_id(&tracks, TrackKind::Subtitle, 0),
      Some(None)
    );
    assert_eq!(
      selected_track_id(&tracks, TrackKind::Subtitle, 1),
      Some(Some(8))
    );
    assert_eq!(selected_track_id(&tracks, TrackKind::Audio, 1), None);
  }

  #[test]
  fn auxiliary_settlement_requires_the_current_session_and_playback_identity() {
    let current = PlaybackIdentity {
      session: 4,
      sequence: 9,
      item_id: "episode-3".to_owned(),
    };
    let replaced = PlaybackIdentity {
      session: 4,
      sequence: 10,
      item_id: "episode-3".to_owned(),
    };

    assert!(auxiliary_settlement_is_current(
      4,
      &current,
      4,
      Some(&current),
    ));
    assert!(!auxiliary_settlement_is_current(
      4,
      &current,
      5,
      Some(&current),
    ));
    assert!(!auxiliary_settlement_is_current(
      4,
      &current,
      4,
      Some(&replaced),
    ));
  }
  #[test]
  fn remote_lifecycle_transitions_remain_honest() {
    assert_eq!(
      remote_state_after_event(
        RemoteControlState::Connecting,
        &jellypilot_session::JellyfinWebSocketEvent::Connected,
      ),
      RemoteControlState::Available
    );
    assert_eq!(
      remote_state_after_event(
        RemoteControlState::Available,
        &jellypilot_session::JellyfinWebSocketEvent::ConnectionLost,
      ),
      RemoteControlState::Lost
    );
  }
  fn intro_range(kind: IntroSkipKind, start_seconds: f64, end_seconds: f64) -> IntroSkipRange {
    IntroSkipRange {
      kind,
      start_seconds,
      end_seconds,
      notified: false,
      skipped: false,
    }
  }

  #[test]
  fn intro_range_fetch_requires_enabled_mode_capability_and_episode() {
    assert!(should_fetch_intro_ranges(
      config::IntroMode::Automatic,
      true,
      "Episode",
    ));
    assert!(!should_fetch_intro_ranges(
      config::IntroMode::Off,
      true,
      "Episode",
    ));
    assert!(!should_fetch_intro_ranges(
      config::IntroMode::Manual,
      false,
      "Episode",
    ));
    assert!(!should_fetch_intro_ranges(
      config::IntroMode::Manual,
      true,
      "Movie",
    ));
  }

  #[test]
  fn disabling_intro_skip_purges_ranges_prompt_and_invalidates_fetch() {
    let mut state = IntroSkipState {
      identity: None,
      sequence: 7,
      mode: IntroSkipMode::Manual,
      ranges: vec![intro_range(IntroSkipKind::Introduction, 10.0, 30.0)],
      active_prompt: Some(ActiveIntroPrompt {
        range_index: 0,
        expires_at: Instant::now() + Duration::from_secs(3),
      }),
    };

    disable_intro_skip(&mut state);

    assert_eq!(state.mode, IntroSkipMode::Off);
    assert!(state.ranges.is_empty());
    assert!(state.active_prompt.is_none());
    assert_eq!(state.sequence, 8);
  }

  #[test]
  fn automatic_intro_skip_starts_at_the_exact_boundary_and_applies_each_range_once() {
    let mut ranges = [
      intro_range(IntroSkipKind::Introduction, 10.0, 30.0),
      intro_range(IntroSkipKind::Credits, 90.0, 120.0),
    ];

    assert_eq!(
      evaluate_intro_ui_action(9.5, &mut ranges, IntroSkipMode::Automatic, false, None),
      None
    );
    assert_eq!(
      evaluate_intro_ui_action(10.0, &mut ranges, IntroSkipMode::Automatic, false, None),
      Some(IntroUiAction::Seek {
        range_index: 0,
        target: 30.0,
      })
    );
    assert_eq!(
      evaluate_intro_ui_action(10.0, &mut ranges, IntroSkipMode::Automatic, false, None),
      None
    );
    assert_eq!(
      evaluate_intro_ui_action(90.0, &mut ranges, IntroSkipMode::Automatic, false, None),
      Some(IntroUiAction::Seek {
        range_index: 1,
        target: 120.0,
      })
    );
  }

  #[test]
  fn seeking_back_does_not_reskip_an_automatic_range() {
    let mut ranges = [intro_range(IntroSkipKind::Introduction, 10.0, 30.0)];

    assert_eq!(
      evaluate_intro_ui_action(10.0, &mut ranges, IntroSkipMode::Automatic, false, None),
      Some(IntroUiAction::Seek {
        range_index: 0,
        target: 30.0,
      })
    );
    assert_eq!(
      evaluate_intro_ui_action(5.0, &mut ranges, IntroSkipMode::Automatic, false, None),
      None
    );
    assert_eq!(
      evaluate_intro_ui_action(10.0, &mut ranges, IntroSkipMode::Automatic, false, None),
      None
    );
  }

  #[test]
  fn manual_intro_shortcut_requires_a_live_displayed_prompt_and_skips_once() {
    let mut ranges = [intro_range(IntroSkipKind::Introduction, 10.0, 30.0)];

    assert_eq!(
      evaluate_intro_ui_action(10.0, &mut ranges, IntroSkipMode::Manual, false, None),
      Some(IntroUiAction::Prompt {
        range_index: 0,
        kind: IntroSkipKind::Introduction,
      })
    );
    let messages = vec!["jellypilot-skip-intro".to_owned()];
    assert!(manual_intro_skip_requested(&messages));
    assert_eq!(
      evaluate_intro_ui_action(
        10.0,
        &mut ranges,
        IntroSkipMode::Manual,
        manual_intro_skip_requested(&messages),
        None,
      ),
      None
    );
    let now = Instant::now();
    let mut prompt = Some(ActiveIntroPrompt {
      range_index: 0,
      expires_at: now + Duration::from_secs(3),
    });
    let active_prompt = active_intro_prompt_range(&mut prompt, now);
    assert_eq!(
      evaluate_intro_ui_action(
        10.0,
        &mut ranges,
        IntroSkipMode::Manual,
        manual_intro_skip_requested(&messages),
        active_prompt,
      ),
      Some(IntroUiAction::ManualSkip {
        range_index: 0,
        kind: IntroSkipKind::Introduction,
        seek_target: 30.0,
      })
    );
    assert_eq!(
      evaluate_intro_ui_action(
        10.0,
        &mut ranges,
        IntroSkipMode::Manual,
        manual_intro_skip_requested(&messages),
        active_prompt,
      ),
      None
    );
    let mut expired_prompt = Some(ActiveIntroPrompt {
      range_index: 0,
      expires_at: now,
    });
    assert_eq!(active_intro_prompt_range(&mut expired_prompt, now), None);
    assert!(expired_prompt.is_none());
  }
  #[test]
  fn diagnostic_timestamp_includes_date_and_explicit_utc_zone() {
    assert_eq!(format_diagnostic_time(0), "1970-01-01 00:00:00 UTC");
  }
  #[test]
  fn advanced_mpv_arguments_parse_as_space_separated_values() {
    assert_eq!(
      parse_mpv_args(" --fullscreen   --profile=gpu-hq "),
      vec!["--fullscreen", "--profile=gpu-hq"]
    );
  }

  #[test]
  fn subtitle_preferences_reach_mpv_launch_without_overriding_user_slang() {
    let mut settings = LoginPrefill {
      mpv_args: vec!["--fullscreen".to_owned()],
      subtitle_languages: vec!["eng".to_owned(), "spa".to_owned()],
      ..LoginPrefill::default()
    };
    assert_eq!(
      configured_mpv_args(&settings),
      vec!["--fullscreen", "--slang=eng,spa"]
    );

    settings.mpv_args.push("--slang=jpn".to_owned());
    assert_eq!(
      configured_mpv_args(&settings),
      vec!["--fullscreen", "--slang=jpn"]
    );
  }

  #[test]
  fn custom_subtitle_language_validation_rejects_mpv_list_delimiters() {
    assert!(valid_subtitle_language("pt-br"));
    assert!(valid_subtitle_language("zho_hant"));
    assert!(!valid_subtitle_language(""));
    assert!(!valid_subtitle_language("eng,spa"));
    assert!(!valid_subtitle_language("english subtitles"));
  }
  #[test]
  fn client_messages_map_to_adjacent_episode_directions() {
    assert_eq!(
      adjacent_direction_from_client_messages(&[
        "unrelated".to_owned(),
        "jellypilot-next".to_owned(),
      ]),
      Some(AdjacentDirection::Next)
    );
    assert_eq!(
      adjacent_direction_from_client_messages(&["jellypilot-prev".to_owned()]),
      Some(AdjacentDirection::Previous)
    );
    assert_eq!(
      adjacent_direction_from_client_messages(&["jellypilot-skip-intro".to_owned()]),
      None
    );
  }

  #[test]
  fn shortcut_collisions_are_case_insensitive_trimmed_and_exclude_current_action() {
    let settings = LoginPrefill::default();
    assert!(shortcut_binding_collision(
      &settings,
      ShortcutKind::Next,
      " shift+< ",
    ));
    assert!(shortcut_binding_collision(
      &settings,
      ShortcutKind::IntroSkip,
      "SHIFT+>",
    ));
    assert!(!shortcut_binding_collision(
      &settings,
      ShortcutKind::Next,
      " shift+> ",
    ));
  }
}
