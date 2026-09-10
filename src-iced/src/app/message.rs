use std::sync::{Arc, Mutex};

use iced::widget::scrollable;
use iced::window;
use jellypilot_auth::login::{LoginError, LoginEvent};
use jellypilot_auth::{
  AuthStorageError, SavedProfileKey, SavedProfileSummary, SavedProfilesSnapshot,
  SensitiveSavedSession,
};
use jellypilot_core::browse_model::BrowsePageSettlement;
use jellypilot_core::config::{AppMode, IntroMode, LoginPrefill, ShortcutKind, ThemeMode};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::locale::LanguagePreference;
use jellypilot_core::request_gate::{
  DetailAuxToken, DetailToken, HomeToken, RemotePlayToken, RemoteToken, SessionToken,
};
use jellypilot_media_server::artwork::{ArtworkError, ArtworkRaster};
use jellypilot_media_server::home::HomeDataResult;
use jellypilot_media_server::{
  JellyfinClient, MediaItem, MediaServerProvider, VideoItemDetail, VideoLibraryItem,
  VideoLibraryPlayedFilter, VideoLibrarySort, VideoSeasonEpisodes, VideoSeasonEpisodesPage,
  VideoUserDataUpdate,
};
use jellypilot_mpv::playback::{Playable, PlaybackError, PlaybackSelection, TrackInfo};
use jellypilot_mpv::playback_session::{
  AdjacentDirection, ControllerSettlement, EffectId, PlaybackEvent, PlaybackIntent,
};
use jellypilot_session::{CapabilityRegistrationError, JellyfinWebSocketEvent};

use super::state::RemoteSessionHandle;

use zeroize::Zeroize;
impl std::fmt::Debug for Message {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Window(message) => formatter.debug_tuple("Window").field(message).finish(),
      Self::Shell(_) => formatter.write_str("Shell"),
      Self::PersonalLists(_) => formatter.write_str("PersonalLists"),
      Self::Collections(_) => formatter.write_str("Collections"),
      Self::Account(_) => formatter.write_str("Account([redacted])"),
      Self::Login(_) => formatter.write_str("Login([redacted])"),
      Self::Home(_) => formatter.write_str("Home"),
      Self::Browse(_) => formatter.write_str("Browse"),
      Self::OpenDetail(_) => formatter.write_str("OpenDetail"),
      Self::Detail(_) => formatter.write_str("Detail"),
      Self::Playback(_) => formatter.write_str("Playback"),
      Self::EmbeddedPlayer(message) => formatter
        .debug_tuple("EmbeddedPlayer")
        .field(message)
        .finish(),
      Self::Settings(_) => formatter.write_str("Settings"),
      Self::Remote(_) => formatter.write_str("Remote"),
      Self::Tray(action) => formatter.debug_tuple("Tray").field(action).finish(),
      Self::UiLanguageSelected(preference) => formatter
        .debug_tuple("UiLanguageSelected")
        .field(preference)
        .finish(),
      Self::SystemThemeDiscovered(mode) => formatter
        .debug_tuple("SystemThemeDiscovered")
        .field(mode)
        .finish(),
      Self::SystemThemeChanged(mode) => formatter
        .debug_tuple("SystemThemeChanged")
        .field(mode)
        .finish(),
      Self::DismissNotice(id) => formatter.debug_tuple("DismissNotice").field(id).finish(),
      Self::ArtworkSummaryReady => formatter.write_str("ArtworkSummaryReady"),
      Self::ImageObserved { .. } => formatter.write_str("ImageObserved"),
      Self::ProfileAvatarLoaded { .. } => formatter.write_str("ProfileAvatarLoaded([redacted])"),
    }
  }
}

#[derive(Clone)]
pub enum Message {
  Window(WindowMessage),
  Shell(ShellMessage),
  PersonalLists(super::personal_lists::PersonalListsMessage),
  Collections(super::collections::CollectionMessage),
  Account(super::accounts::Message),
  Login(LoginMessage),
  Home(HomeMessage),
  Browse(BrowseMessage),
  OpenDetail(Box<VideoLibraryItem>),
  Detail(DetailMessage),
  Playback(PlaybackMessage),
  EmbeddedPlayer(super::embedded_player::Message),
  Settings(SettingsMessage),
  Remote(RemoteMessage),
  Tray(crate::tray::TrayAction),
  UiLanguageSelected(LanguagePreference),
  /// One-shot OS light/dark mode discovered at boot.
  SystemThemeDiscovered(iced::theme::Mode),
  /// OS light/dark mode changed while the theme mode is `System`.
  SystemThemeChanged(iced::theme::Mode),
  DismissNotice(u64),
  /// Flushes the current burst of sanitized Library Image counters.
  ArtworkSummaryReady,
  ImageObserved {
    surface: super::artwork::ArtworkSurface,
    epoch: u64,
    spec: super::artwork::ImageSpec,
    priority: Option<super::artwork::ImagePriority>,
  },
  /// One saved profile's user image settled through the artwork pipeline;
  /// `None` when the profile's session could not be loaded at all.
  ProfileAvatarLoaded {
    key: SavedProfileKey,
    outcome: Option<Result<ArtworkRaster, ArtworkError>>,
  },
}

#[derive(Clone, Copy, Debug)]
pub enum WindowMessage {
  ShowRequested(Option<window::Id>),
  CloseRequested(window::Id),
  Resized(iced::Size),
  /// One rendered frame; carries the compositor timestamp so animation
  /// phases derive from frame cadence instead of wall-clock polling.
  FrameTick(std::time::Instant),
}

#[derive(Clone, Debug)]
pub enum ShellMessage {
  ToggleCompactSearch,
  DismissCompactSearch,
  FocusSearch,
  ClearSearch,
  RefreshCurrent,
  ExitPlayerFullscreen,
  ToggleAccountPopover,
  DismissAccountPopover,
  DismissAccountLayer,
  SearchEscape,
  SearchFocusChecked(bool),
  FocusNext,
  FocusPrevious,
  DirectoryLoaded {
    session: jellypilot_core::request_gate::SessionToken,
    generation: u64,
    result: Result<Vec<jellypilot_media_server::VideoLibraryShortcut>, String>,
  },
  RefreshFinished {
    session: jellypilot_core::request_gate::SessionToken,
    generation: u64,
  },
}

#[derive(Clone)]
pub enum HomeMessage {
  Navigate(super::state::Destination),
  Retry,
  HeroSelected(String),
  CardHoverEnter(String),
  CardHoverExit(String),
  Loaded {
    token: HomeToken,
    result: HomeDataResult,
  },
  ArtworkLoaded(super::artwork::ImageCompletion),
}

#[derive(Clone)]
pub enum BrowseMessage {
  SearchInputChanged(String),
  SearchSubmitted,
  SortMenuToggled,
  SortMenuDismissed,
  SortChanged(VideoLibrarySort),
  SortDirectionToggled,
  PlayedFilterChanged(VideoLibraryPlayedFilter),
  FavoritesToggled,
  ViewModeSelected(super::browse::ViewMode),
  Scrolled(scrollable::Viewport),
  GridViewportMeasured {
    epoch: u64,
    offset_y: f32,
    height: f32,
  },
  Retry,
  PageSettled(BrowsePageSettlement),
  ArtworkLoaded(super::artwork::ImageCompletion),
}

#[derive(Clone)]
pub enum DetailMessage {
  Back,
  Retry,
  RetryNeighbors,
  RetrySeason,
  OverviewToggled,
  EpisodeOverviewToggled(String),
  SeasonMenuToggled,
  SeasonMenuDismissed,
  SeasonSelected(String),
  FavoriteToggled,
  WatchlistToggled,
  PlayedToggled,
  Loaded {
    token: DetailToken,
    result: Box<Result<jellypilot_core::detail::DetailContent, String>>,
  },
  SeasonLoaded {
    token: DetailToken,
    result: Result<VideoSeasonEpisodesPage, String>,
  },
  NeighborsLoaded {
    token: DetailAuxToken,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  SimilarLoaded {
    token: DetailAuxToken,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  UserDataUpdated {
    token: DetailAuxToken,
    result: Result<VideoUserDataUpdate, String>,
  },
  ArtworkLoaded(super::artwork::ImageCompletion),
}
#[derive(Clone)]
pub enum SettingsMessage {
  Open,
  OpenAccounts,
  Close,
  SectionSelected(super::state::SettingsSection),
  PlaybackBackendSelected(jellypilot_core::config::PlaybackBackend),
  MpvPathChanged(String),
  SaveMpvPath,
  MpvArgsChanged(String),
  SaveMpvArgs,
  PlaybackTargetNameChanged(String),
  SavePlaybackTargetName,
  TmdbApiKeyChanged(String),
  SaveTmdbApiKey,
  IntroMenuToggled,
  IntroMenuDismissed,
  IntroModeSelected(IntroMode),
  RememberSeasonVolumeChanged(bool),
  PreferOriginalAudioChanged(bool),
  ThemeModeSelected(ThemeMode),
  AppModeSelected(AppMode),
  FontLicensesToggled,
  SubtitleMenuToggled,
  SubtitleMenuDismissed,
  SubtitleLanguageAdded(String),
  SubtitleLanguageMoved { index: usize, offset: i32 },
  SubtitleLanguageRemoved(usize),
  BeginShortcutCapture(ShortcutKind),
  ShortcutCaptured(String),
  CancelShortcutCapture,
  ImageCacheToggled,
  AutoLoginToggled,
  StartMinimizedToggled,
  ReducedMotionToggled,
  DiagnosticLevelMenuToggled,
  DiagnosticLevelMenuDismissed,
  DiagnosticLevelSelected(Option<DiagnosticLevel>),
  DiagnosticCategoryMenuToggled,
  DiagnosticCategoryMenuDismissed,
  DiagnosticCategorySelected(Option<DiagnosticCategory>),
  PlayerLogCaptureChanged(bool),
  ExportLogs,
  LogsExported(Result<String, String>),
  PlaybackConfigApplied(Result<(), PlaybackError>),
}

#[derive(Clone)]
pub enum PlaybackMessage {
  Intent(Box<PlaybackIntent>),
  Event(Box<PlaybackEvent>),
  SeekDragStarted,
  SeekChanged(f64),
  SeekReleased,
  SeekAdjusted(f64),
  VolumeDragStarted,
  VolumeChanged(f64),
  VolumeReleased,
  VolumeAdjusted(f64),
  AudioMenuToggled,
  AudioMenuDismissed,
  AudioTrackSelected(i64),
  SubtitleMenuToggled,
  SubtitleMenuDismissed,
  SubtitleTrackSelected(Option<i64>),
  QueueMenuToggled,
  QueueMenuDismissed,
  QueueItemSelected(Box<VideoLibraryItem>),
  QueueLoaded {
    session: SessionToken,
    generation: u64,
    series_id: String,
    season_number: i32,
    result: Result<VideoSeasonEpisodes, String>,
  },
  ControllerSettled {
    id: EffectId,
    settlement: Box<ControllerSettlement>,
    started: Option<Box<Playable>>,
    tracks: Option<Result<Vec<TrackInfo>, PlaybackError>>,
  },
  AdjacentSettled {
    remote: RemoteToken,
    play: RemotePlayToken,
    id: EffectId,
    direction: AdjacentDirection,
    result: Result<Option<MediaItem>, ()>,
    detail: Option<Box<VideoItemDetail>>,
  },
  ArtworkLoaded(super::artwork::ImageCompletion),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteStartError {
  SessionUnavailable,
  ConnectionFailed,
  CapabilityRegistrationFailed,
}

impl RemoteStartError {
  pub const fn diagnostic(self) -> &'static str {
    match self {
      Self::SessionUnavailable => "Remote playback target session is unavailable.",
      Self::ConnectionFailed => "Remote playback target could not connect.",
      Self::CapabilityRegistrationFailed => {
        "Remote playback target capabilities could not be registered."
      }
    }
  }
}

#[derive(Clone)]
pub struct RemoteSessionStart {
  pub session: RemoteSessionHandle,
  pub validated: bool,
}

#[derive(Clone)]
pub enum RemoteMessage {
  Started {
    remote: RemoteToken,
    result: Result<RemoteSessionStart, RemoteStartError>,
  },
  Event {
    remote: RemoteToken,
    event: JellyfinWebSocketEvent,
  },
  Finalized {
    remote: RemoteToken,
    result: Result<bool, CapabilityRegistrationError>,
  },
  PlayResolved {
    remote: RemoteToken,
    play: RemotePlayToken,
    result: Box<Result<Playable, ()>>,
    start_position_ticks: Option<i64>,
    selection: PlaybackSelection,
  },
  RemoteDisconnected,
  QuitStopped,
}

pub type SensitiveSessionPayload = SensitiveSavedSession;

struct ProtectedPayloadOwner<T: Zeroize>(Option<T>);

impl<T: Zeroize> Drop for ProtectedPayloadOwner<T> {
  fn drop(&mut self) {
    if let Some(payload) = &mut self.0 {
      payload.zeroize();
    }
  }
}

struct ProtectedPayload<T: Zeroize>(Arc<Mutex<ProtectedPayloadOwner<T>>>);

impl<T: Zeroize> Clone for ProtectedPayload<T> {
  fn clone(&self) -> Self {
    Self(Arc::clone(&self.0))
  }
}

impl<T: Zeroize> ProtectedPayload<T> {
  fn new(payload: T) -> Self {
    Self(Arc::new(Mutex::new(ProtectedPayloadOwner(Some(payload)))))
  }

  fn take(&self) -> Option<T> {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .0
      .take()
  }
}

#[derive(Clone)]
pub struct ProtectedSavedSession(ProtectedPayload<SensitiveSessionPayload>);

impl ProtectedSavedSession {
  pub fn new(session: SensitiveSessionPayload) -> Self {
    Self(ProtectedPayload::new(session))
  }

  pub fn take(&self) -> Option<SensitiveSessionPayload> {
    self.0.take()
  }
}

#[derive(Clone)]
pub struct PasswordSubmission {
  pub remember: bool,
  pub prefill: LoginPrefill,
  pub provider: MediaServerProvider,
}

#[derive(Clone)]
pub enum LoginMessage {
  ProviderSelected(MediaServerProvider),
  MethodSelected(super::state::LoginMethod),
  ServerUrlChanged(String),
  UsernameChanged(String),
  PasswordChanged(String),
  RememberToggled,
  QuickConnectSubmitted,
  QuickConnectCancelled,
  PasswordSubmitted,
  ProfilesLoaded {
    revision: u64,
    result: Result<SavedProfilesSnapshot, AuthStorageError>,
  },
  ActivationRecorded {
    session: SessionToken,
    key: SavedProfileKey,
    result: Result<(), AuthStorageError>,
  },
  WorkflowEvent(LoginEvent),
  PasswordFinished {
    session: SessionToken,
    client: Arc<JellyfinClient>,
    result: Result<ProtectedSavedSession, LoginError>,
    submission: PasswordSubmission,
  },
  SavedSessionStored {
    session: SessionToken,
    result: Result<(SavedProfileKey, Vec<SavedProfileSummary>), AuthStorageError>,
  },
  RestoreProfile(SavedProfileKey),
  RestoreFinished {
    session: SessionToken,
    key: SavedProfileKey,
    result: Result<ProtectedSavedSession, LoginError>,
  },
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicBool, Ordering};

  use super::*;

  struct ZeroizeWitness(Arc<AtomicBool>);

  impl Zeroize for ZeroizeWitness {
    fn zeroize(&mut self) {
      self.0.store(true, Ordering::Relaxed);
    }
  }

  #[test]
  fn protected_payload_zeroizes_when_the_last_message_copy_is_dropped() {
    let zeroized = Arc::new(AtomicBool::new(false));
    let payload = ProtectedPayload::new(ZeroizeWitness(Arc::clone(&zeroized)));
    let cloned = payload.clone();

    drop(payload);
    assert!(!zeroized.load(Ordering::Relaxed));
    drop(cloned);

    assert!(zeroized.load(Ordering::Relaxed));
  }
}
