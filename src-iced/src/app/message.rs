use std::sync::{Arc, Mutex};

use iced::widget::scrollable;
use iced::window;
use jellypilot_auth::{SavedProfileKey, SavedProfilesSnapshot};
use jellypilot_sdk::{ActivationOutcome, ProfileCandidate, QuickConnectOutcome, SdkError};

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
  MediaItem, MediaServerProvider, VideoItemDetail, VideoLibraryItem, VideoLibraryPlayedFilter,
  VideoLibrarySort, VideoSeasonEpisodes, VideoSeasonEpisodesPage,
};
use jellypilot_mpv::playback::{Playable, PlaybackError, PlaybackSelection};
use jellypilot_mpv::playback_session::{
  AdjacentDirection, ControllerSettlement, EffectId, PlaybackEvent, PlaybackIntent,
};
use jellypilot_session::JellyfinWebSocketEvent;

impl std::fmt::Debug for Message {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Window(message) => formatter.debug_tuple("Window").field(message).finish(),
      Self::Shell(_) => formatter.write_str("Shell"),
      Self::PersonalLists(_) => formatter.write_str("PersonalLists"),
      Self::Collections(_) => formatter.write_str("Collections"),
      Self::ItemActions(_) => formatter.write_str("ItemActions"),
      Self::Undo(_) => formatter.write_str("Undo"),
      Self::ListPlayback(_) => formatter.write_str("ListPlayback"),
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
  ItemActions(super::item_actions::Message),
  Undo(super::undo::Message),
  ListPlayback(super::list_playback::Message),
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
  /// An explicit play command needs a visible player (ADR 0043). The router
  /// normalizes this into a real show only while a deferred play is still
  /// pending; the shell itself treats it as a no-op.
  ShowForPlayback,
  CloseRequested(window::Id),
  /// The compositor destroyed the window without a close request (kill,
  /// session end); the shell treats it as a close that already happened.
  Closed(window::Id),
  /// A window opened for an explicit play command did not arrive in time.
  OpenTimedOut(window::Id),
  /// The close-path engine pause resolved: the window stays closed only
  /// after the acknowledgement lands (ADR 0043).
  BackgroundPauseSettled {
    id: window::Id,
    generation: u64,
    result: Result<(), PlaybackError>,
  },
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
  /// Toggles the read-only track list anchored to a Media Specifications chip.
  TrackMenuToggled(super::state::TrackMenu),
  TrackMenuDismissed,
  /// Opens the loaded episode's parent series through normal navigation.
  OpenSeries,
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
  ArtworkLoaded(super::artwork::ImageCompletion),
}
#[derive(Clone)]
pub enum SettingsMessage {
  Open,
  OpenAccounts,
  Close,
  SectionSelected(super::state::SettingsSection),
  LanguageMenuToggled,
  LanguageMenuDismissed,
  UiLanguageSelected(LanguagePreference),
  PlaybackBackendSelected(jellypilot_core::config::PlaybackBackend),
  HdrOutputSelected(jellypilot_core::config::HdrOutput),
  HdrStatusChanged(crate::embedded::HdrState),
  HdrContentChanged(bool),
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
  /// The player skip toggle: `true` selects Automatic, `false` Manual, for the
  /// current episode's series within the active profile (ADR 0045).
  IntroModeChanged(bool),
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
  /// The close-path engine pause resolved for a window that was already
  /// destroyed (compositor kill, session end): errors surface here because
  /// no pending close is waiting on the acknowledgement.
  PresentationPaused(Result<(), PlaybackError>),
}
#[derive(Clone)]
pub enum RemoteMessage {
  Completed(super::playback::remote::Completion),
  Event {
    remote: RemoteToken,
    event: JellyfinWebSocketEvent,
  },
  PlayResolved {
    remote: RemoteToken,
    play: RemotePlayToken,
    result: Box<Result<Playable, ()>>,
    start_position_ticks: Option<i64>,
    selection: PlaybackSelection,
  },
}

/// A validated profile candidate protected inside a cloneable message.
///
/// The candidate is single-use: whichever reducer turn takes it owns the
/// validated session; clones that arrive later observe `None`.
#[derive(Clone)]
pub struct ProtectedCandidate(Arc<Mutex<Option<ProfileCandidate>>>);

impl ProtectedCandidate {
  pub fn new(candidate: ProfileCandidate) -> Self {
    Self(Arc::new(Mutex::new(Some(candidate))))
  }

  pub fn take(&self) -> Option<ProfileCandidate> {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .take()
  }
}

impl std::fmt::Debug for ProtectedCandidate {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str("ProtectedCandidate([redacted])")
  }
}

/// A terminal Quick Connect outcome protected inside a cloneable message.
#[derive(Clone)]
pub struct ProtectedOutcome(Arc<Mutex<Option<QuickConnectOutcome>>>);

impl ProtectedOutcome {
  pub fn new(outcome: QuickConnectOutcome) -> Self {
    Self(Arc::new(Mutex::new(Some(outcome))))
  }

  pub fn take(&self) -> Option<QuickConnectOutcome> {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .take()
  }
}

impl std::fmt::Debug for ProtectedOutcome {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str("ProtectedOutcome([redacted])")
  }
}

/// Quick Connect progress forwarded from the SDK listener to a login surface.
#[derive(Clone)]
pub enum QuickConnectEvent {
  /// The server issued a pairing code to display.
  Code(String),
  /// The user approved the code; final authentication is in flight.
  Approving,
  /// Terminal outcome; no further events follow for this session.
  Completed(ProtectedOutcome),
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
    result: Result<SavedProfilesSnapshot, SdkError>,
  },
  QuickConnectEvent {
    session: u64,
    event: QuickConnectEvent,
  },
  PasswordFinished {
    request: u64,
    result: Result<ProtectedCandidate, SdkError>,
    submission: PasswordSubmission,
  },
  RestoreProfile(SavedProfileKey),
  /// SDK saved-profile validation finished; `Ok` carries the candidate.
  RestoreFinished {
    request: u64,
    key: SavedProfileKey,
    result: Result<ProtectedCandidate, SdkError>,
  },
  /// One SDK activation finished: password, Quick Connect, or saved restore.
  ActivationFinished {
    result: Result<ActivationOutcome, SdkError>,
  },
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_media_server::JellyfinClient;

  #[test]
  fn protected_candidate_is_taken_exactly_once() {
    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&jellypilot_media_server::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: "https://example.test".to_owned(),
        access_token: "token".to_owned(),
        user_id: "user".to_owned(),
        user_name: "User".to_owned(),
        server_name: None,
        device_id: None,
      });
    let candidate = ProfileCandidate::new(
      jellypilot_auth::login::ValidatedProfileCandidate::from_authenticated_client(client)
        .unwrap_or_else(|_| panic!("authenticated client becomes a candidate")),
    );
    let protected = ProtectedCandidate::new(candidate);
    let cloned = protected.clone();

    assert!(protected.take().is_some());
    assert!(cloned.take().is_none());
  }
}
