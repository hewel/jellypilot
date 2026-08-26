use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jellypilot_media_server::{
  JellyfinClient, MediaItem, PlaybackEngineKind, SavedSession, VideoDetailMetadata, VideoHome,
  VideoItemDetail, VideoItemStreams, VideoLibraryItem, VideoLibraryKind, VideoLibraryPageRequest,
  VideoLibraryPlayedFilter, VideoLibraryShortcut, VideoLibrarySort, VideoLibrarySortDirection,
  VideoSearchRequest, VideoSeason, VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest,
  VideoShowDetail, VideoUserDataAction, VideoUserDataUpdate, VideoUserDataUpdateRequest,
};
use jellypilot_mpv::{has_mpv_option, write_input_conf, PlayerState};
use jellypilot_session::{IntroSkipKind, IntroSkipMode, IntroSkipRange};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, Component, ComponentParts, ComponentSender, RelmApp};

use crate::pages::diagnostics::{self, DiagnosticsContext, DiagnosticsPage};
use crate::pages::login::{
  self, run_auth_operation, LoginContext, LoginEffect, LoginEvent, LoginPage,
};
use crate::pages::settings::{
  self, ConnectionView, SettingsContext, SettingsEffect, SettingsEvent, SettingsPage,
};

use crate::artwork::{ArtworkAdapter, DecodedArtwork, FALLBACK_ARTWORK_ICON};
use crate::artwork_cache::ArtworkCacheStats;
use crate::auth_storage::{AuthStore, SavedProfileKey, SavedProfileSummary};
use crate::browse_model::{
  BrowseEffect, BrowseModel, BrowsePagePayload, BrowsePageRequest, BrowsePageSettlement,
  BrowsePreferences, BrowseSource,
};
use crate::config::{self, LoginPrefill};
use crate::diagnostics::{DiagnosticCategory, DiagnosticLevel, Diagnostics};
use crate::library_browse::LibraryBrowseView;
use crate::playback::{
  Playable, PlaybackController, PlaybackControllerConfig, PlaybackError, PlaybackRefreshOutcome,
  PlaybackRefreshState, PlaybackSnapshot, PlaybackStartPosition, TrackInfo,
};
use crate::playback_session::{
  AdjacentAvailability, AdjacentDirection, ControllerCommand, ControllerSettlement, EffectId,
  IntroAvailability, PlaybackEffect, PlaybackEvent, PlaybackInput, PlaybackIntent, PlaybackNotice,
  PlaybackSession, SessionView, TracksView,
};
use crate::request_gate::{
  DetailAuxKind, DetailAuxToken, DetailToken, HomeToken, ImageCacheToken, RemotePlayToken,
  RemoteToken, RequestGate, SessionToken,
};

const APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview";
const SMOKE_APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview.Smoke";
const SEASON_EPISODE_PAGE_SIZE: i32 = 30;
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
  login: LoginPage,
  settings: SettingsPage,
  diagnostics_page: DiagnosticsPage,
  intro_mode: config::IntroMode,
  diagnostics: Diagnostics,
  saved_profiles: LoadState<Vec<SavedProfileSummary>>,
  active_saved_profile: Option<SavedProfileKey>,
  artwork: Arc<ArtworkAdapter>,
  artwork_view: u64,
  playback_artwork_view: u64,
  artwork_slot: u64,
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
  streams: LoadState<VideoItemStreams>,
  season_neighbors: LoadState<Vec<VideoLibraryItem>>,
  season: Option<SeasonSelection>,
  recommendations: LoadState<Vec<VideoLibraryItem>>,
  user_data_busy: bool,
  user_data_error: Option<String>,
  remote_state: RemoteControlState,
  remote_socket: Option<Arc<jellypilot_session::JellyfinWebSocket>>,
  playback_session: PlaybackSession,
  playback_controller: Option<PlaybackController>,
  playback_item: Option<MediaItem>,
  playback_artwork_image_id: Option<String>,
  playback_reconfigure_pending: bool,
  playback_engine_error: Option<String>,
  remote_disconnect_pending: bool,
  quitting: bool,
  ui: Ui,
}

struct ArtworkTarget {
  picture: gtk::Picture,
  fallback: gtk::Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackKind {
  Audio,
  Subtitle,
}

#[derive(Clone, Copy)]
enum ArtworkPresentation {
  Backdrop,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum ConnectionPhase {
  #[default]
  SignedOut,
  Connecting,
  Connected,
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

#[derive(Clone, Default)]
pub(crate) enum LoadState<T> {
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
pub(crate) enum BrowsePresentation {
  #[default]
  Grid,
  List,
}

#[derive(Debug)]
pub(crate) enum AppMessage {
  Login(login::Message),
  Settings(settings::Message),
  Diagnostics(diagnostics::Message),
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
  StopPlayback,
  RefreshPlayback,
  QuitRequested,
  RemoteDisconnectSettled(RemoteToken),
}

enum AppCommand {
  LoginEvent(LoginEvent),
  RemoteReady {
    token: RemoteToken,
    socket: Arc<jellypilot_session::JellyfinWebSocket>,
    receiver: relm4::tokio::sync::mpsc::Receiver<jellypilot_session::JellyfinWebSocketEvent>,
    validated: bool,
  },
  RemoteEvent {
    token: RemoteToken,
    event: jellypilot_session::JellyfinWebSocketEvent,
  },
  RemoteFailed {
    token: RemoteToken,
  },
  RemotePlay {
    token: RemotePlayToken,
    start_position: PlaybackStartPosition,
    result: Result<VideoItemDetail, String>,
  },
  ConnectionStatus {
    session: SessionToken,
    result: Result<(), ()>,
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
    token: DetailAuxToken,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  Streams {
    token: DetailAuxToken,
    result: Result<VideoItemStreams, String>,
  },
  SeasonNeighbors {
    token: DetailAuxToken,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  SeasonEpisodes {
    token: DetailToken,
    season_id: String,
    result: Result<VideoSeasonEpisodesPage, String>,
  },
  UserData {
    token: DetailAuxToken,
    result: Result<VideoUserDataUpdate, String>,
  },
  Artwork {
    session: SessionToken,
    view: u64,
    slot: u64,
    result: Result<DecodedArtwork, ()>,
  },
  ImageCacheStats {
    token: ImageCacheToken,
    result: Result<ArtworkCacheStats, ()>,
  },
  ImageCacheCleared {
    token: ImageCacheToken,
    result: Result<ArtworkCacheStats, ()>,
  },
  PlaybackSettled {
    id: EffectId,
    controller: Option<Box<PlaybackController>>,
    settlement: ControllerSettlement,
    tracks: Option<Result<Vec<TrackInfo>, PlaybackError>>,
  },
  IntroRangesSettled {
    id: EffectId,
    result: Result<Vec<IntroSkipRange>, ()>,
  },
  AdjacentSettled {
    id: EffectId,
    direction: AdjacentDirection,
    result: Result<Option<MediaItem>, ()>,
  },
}

impl std::fmt::Debug for AppCommand {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::LoginEvent(event) => formatter.debug_tuple("LoginEvent").field(event).finish(),
      Self::RemoteFailed { token } => formatter
        .debug_struct("RemoteFailed")
        .field("token", token)
        .finish(),
      Self::RemotePlay { token, result, .. } => formatter
        .debug_struct("RemotePlay")
        .field("token", token)
        .field("successful", &result.is_ok())
        .finish(),
      Self::ConnectionStatus { session, result } => formatter
        .debug_struct("ConnectionStatus")
        .field("session", session)
        .field("successful", &result.is_ok())
        .finish(),
      Self::RemoteReady { token, .. } => formatter
        .debug_struct("RemoteReady")
        .field("token", token)
        .finish(),
      Self::RemoteEvent { token, event } => formatter
        .debug_struct("RemoteEvent")
        .field("token", token)
        .field("event", event)
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
      Self::Recommendations { token, result } => formatter
        .debug_struct("Recommendations")
        .field("token", token)
        .field("successful", &result.is_ok())
        .finish(),
      Self::Streams { token, result } => formatter
        .debug_struct("Streams")
        .field("token", token)
        .field("successful", &result.is_ok())
        .finish(),
      Self::SeasonNeighbors { token, result } => formatter
        .debug_struct("SeasonNeighbors")
        .field("token", token)
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
      Self::UserData { token, result } => formatter
        .debug_struct("UserData")
        .field("token", token)
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
      Self::ImageCacheStats { token, result } => formatter
        .debug_struct("ImageCacheStats")
        .field("token", token)
        .field("successful", &result.is_ok())
        .finish(),
      Self::ImageCacheCleared { token, result } => formatter
        .debug_struct("ImageCacheCleared")
        .field("token", token)
        .field("successful", &result.is_ok())
        .finish(),
      Self::PlaybackSettled { settlement, .. } => formatter
        .debug_struct("PlaybackSettled")
        .field(
          "kind",
          &match settlement {
            ControllerSettlement::Started(_) => "started",
            ControllerSettlement::Controlled(_) => "controlled",
            ControllerSettlement::Stopped(_) => "stopped",
            ControllerSettlement::Refreshed { .. } => "refreshed",
            ControllerSettlement::TrackSelected(_) => "track-selected",
            ControllerSettlement::OsdShown(_) => "osd",
            ControllerSettlement::Shutdown(_) => "shutdown",
          },
        )
        .finish(),
      Self::IntroRangesSettled { result, .. } => formatter
        .debug_struct("IntroRangesSettled")
        .field("successful", &result.is_ok())
        .finish(),
      Self::AdjacentSettled {
        direction, result, ..
      } => formatter
        .debug_struct("AdjacentSettled")
        .field("direction", direction)
        .field("successful", &result.is_ok())
        .finish(),
    }
  }
}

struct Ui {
  toast_overlay: adw::ToastOverlay,
  root: adw::ToolbarView,
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
    let login = LoginPage::build(sender.input_sender());
    let diagnostics_page = DiagnosticsPage::build(sender.input_sender());
    let settings = SettingsPage::build(sender.input_sender(), diagnostics_page.root());
    let ui = Ui::new(&sender, login.root());
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
    let _playback_tick = gtk::glib::timeout_add_seconds_local(1, {
      let sender = sender.clone();
      move || {
        sender.input(AppMessage::RefreshPlayback);
        gtk::glib::ControlFlow::Continue
      }
    });
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
      login,
      settings,
      diagnostics_page,
      intro_mode,
      diagnostics,
      saved_profiles: LoadState::Loading,
      active_saved_profile: None,
      artwork,
      artwork_view: 0,
      playback_artwork_view: 0,
      artwork_slot: 0,
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
      streams: LoadState::Idle,
      season_neighbors: LoadState::Idle,
      detail_origin: None,
      detail_parent: None,
      recommendations: LoadState::Idle,
      season: None,
      user_data_busy: false,
      user_data_error: None,
      remote_state: RemoteControlState::Unavailable,
      remote_socket: None,
      playback_session: PlaybackSession::default(),
      playback_controller: None,
      playback_item: None,
      playback_artwork_image_id: None,
      playback_reconfigure_pending: false,
      playback_engine_error: None,
      remote_disconnect_pending: false,
      quitting: false,
      ui,
    };
    let widgets = view_output!();
    model.render_connection_settings();
    if !smoke_test {
      sender.input(AppMessage::Login(login::Message::LoadSavedProfiles));
    }

    ComponentParts { model, widgets }
  }

  fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
    match message {
      AppMessage::Login(message) => self.dispatch_login(message, &sender),
      AppMessage::Settings(message) => self.dispatch_settings(message, &sender),
      AppMessage::Diagnostics(message) => self.dispatch_diagnostics(message),
      AppMessage::Disconnect => {
        if !self.login.is_profile_busy() {
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
        self.requests.cancel_detail_loads();
        self.season = None;
        self.render_detail(&sender);
      }
      AppMessage::UpdateUserData { item_id, action } => {
        self.start_user_data_update(item_id, action, &sender)
      }
      AppMessage::PlayLibrary(item, start_position) => {
        self.start_playback(Playable::from(item), start_position, &sender)
      }
      AppMessage::PlayDetail(item, start_position) => {
        self.start_playback(Playable::from(item), start_position, &sender)
      }
      AppMessage::TogglePaused => self.dispatch_playback(PlaybackIntent::TogglePaused, &sender),
      AppMessage::SetPaused(paused) => {
        self.dispatch_playback(PlaybackIntent::SetPaused(paused), &sender)
      }
      AppMessage::Seek(position) => self.dispatch_playback(PlaybackIntent::Seek(position), &sender),
      AppMessage::SetVolume(volume) => {
        self.dispatch_playback(PlaybackIntent::SetVolume(volume), &sender)
      }
      AppMessage::SetMuted(muted) => {
        self.dispatch_playback(PlaybackIntent::SetMuted(muted), &sender)
      }
      AppMessage::SelectAudioTrack(id) => {
        self.dispatch_playback(PlaybackIntent::SelectAudioTrack(id), &sender)
      }
      AppMessage::SelectSubtitleTrack(id) => {
        self.dispatch_playback(PlaybackIntent::SelectSubtitleTrack(id), &sender)
      }
      AppMessage::PlayAdjacent(direction) => {
        self.dispatch_playback(PlaybackIntent::PlayAdjacent(direction), &sender)
      }
      AppMessage::StopPlayback => self.dispatch_playback(PlaybackIntent::Stop, &sender),
      AppMessage::RefreshPlayback => self.dispatch_playback(PlaybackIntent::Tick, &sender),
      AppMessage::QuitRequested => self.request_quit(&sender),
      AppMessage::RemoteDisconnectSettled(token) => {
        if self.requests.is_current_remote(token) && self.remote_disconnect_pending {
          self.remote_disconnect_pending = false;
          if self.quitting && self.playback_session.view().quit_may_proceed {
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
      AppCommand::LoginEvent(event) => self.dispatch_login_event(event, &sender),
      AppCommand::RemoteReady {
        token,
        socket,
        receiver,
        validated,
      } => {
        if !self.requests.is_current_remote(token) {
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
        if !validated {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::RemoteControl,
            "The server has not listed this device as a remote-control target yet.",
          );
        }
        sender.command(move |output, shutdown| {
          shutdown
            .register(async move {
              let mut receiver = receiver;
              while let Some(event) = receiver.recv().await {
                if output
                  .send(AppCommand::RemoteEvent { token, event })
                  .is_err()
                {
                  break;
                }
              }
            })
            .drop_on_shutdown()
        });
      }
      AppCommand::RemoteFailed { token } => {
        if self.requests.is_current_remote(token) {
          self.remote_state = RemoteControlState::Lost;
          self.update_connection_status();
          self.record_diagnostic(
            DiagnosticLevel::Error,
            DiagnosticCategory::RemoteControl,
            "Remote-control capability or socket setup failed.",
          );
        }
      }
      AppCommand::RemoteEvent { token, event } => {
        if !self.requests.is_current_remote(token) {
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
        token,
        start_position,
        result,
      } => {
        if !self.requests.is_current_remote_play(token) {
          return;
        }
        if let Ok(item) = result {
          self.start_playback(Playable::from(item), start_position, &sender);
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
        if !self.requests.is_current_session(session)
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
          }
          Err(()) => {
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Connection,
              "Authenticated connection status refresh failed.",
            );
          }
        }
        let cx = self.settings_context();
        self
          .settings
          .handle_event(SettingsEvent::ConnectionStatus(result), &cx);
        self.render_connection_settings();
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
      AppCommand::Recommendations { token, result } => {
        if !self.requests.finish_detail_aux(token) {
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
      AppCommand::Streams { token, result } => {
        if !self.requests.finish_detail_aux(token) {
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
      AppCommand::SeasonNeighbors { token, result } => {
        if !self.requests.finish_detail_aux(token) {
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
      AppCommand::UserData { token, result } => {
        self.finish_user_data_update(token, result, &sender)
      }
      AppCommand::Artwork {
        session,
        view,
        slot,
        result,
      } => {
        if !self.requests.is_current_session(session) {
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
      AppCommand::ImageCacheStats { token, result } => {
        if !self.requests.finish_image_cache(token) || self.image_cache_clearing {
          return;
        }
        let cx = self.settings_context();
        self
          .settings
          .handle_event(SettingsEvent::ImageCacheStats(result), &cx);
      }
      AppCommand::ImageCacheCleared { token, result } => {
        if !self.requests.finish_image_cache(token) {
          return;
        }
        self.image_cache_clearing = false;
        match &result {
          Ok(_) => {
            self.record_diagnostic(
              DiagnosticLevel::Info,
              DiagnosticCategory::Artwork,
              "Library Image Cache cleared.",
            );
          }
          Err(()) => {
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Artwork,
              "Library Image Cache clear failed.",
            );
          }
        }
        let cx = self.settings_context();
        self
          .settings
          .handle_event(SettingsEvent::ImageCacheCleared(result), &cx);
      }
      AppCommand::PlaybackSettled {
        id,
        controller,
        settlement,
        tracks,
      } => self.finish_controller_settlement(id, controller, settlement, tracks, &sender),
      AppCommand::IntroRangesSettled { id, result } => {
        self.apply_playback_event(PlaybackEvent::IntroRangesSettled { id, result }, &sender);
      }
      AppCommand::AdjacentSettled {
        id,
        direction,
        result,
      } => {
        if result.is_err() {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::Playback,
            "The server could not resolve one or more adjacent episodes.",
          );
        }
        self.apply_playback_event(
          PlaybackEvent::AdjacentSettled {
            id,
            direction,
            result,
          },
          &sender,
        );
      }
    }
  }

  fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
    self.artwork.reset_session();
    self.stop_remote_session(None);
    self.login.reset_flow();
    if let Some(mut controller) = self.playback_controller.take() {
      relm4::spawn(async move {
        let _ = controller.shutdown().await;
      });
    }
  }
}

impl AppModel {
  fn dispatch_login(&mut self, message: login::Message, sender: &ComponentSender<Self>) {
    let effects = self.with_login_context(|login, cx| login.handle(message, cx));
    self.execute_login_effects(effects, sender);
  }

  fn settings_context(&self) -> SettingsContext {
    SettingsContext {
      intro_mode: self.intro_mode,
      image_cache_clearing: self.image_cache_clearing,
      connected: matches!(self.connection, ConnectionPhase::Connected),
    }
  }

  fn dispatch_settings(&mut self, message: settings::Message, sender: &ComponentSender<Self>) {
    let cx = self.settings_context();
    let effects = self.settings.handle(message, &cx);
    self.execute_settings_effects(effects, sender);
  }

  fn dispatch_diagnostics(&mut self, message: diagnostics::Message) {
    let mut cx = DiagnosticsContext {
      diagnostics: &mut self.diagnostics,
    };
    self.diagnostics_page.handle(message, &mut cx);
  }

  fn execute_settings_effects(
    &mut self,
    effects: Vec<SettingsEffect>,
    sender: &ComponentSender<Self>,
  ) {
    for effect in effects {
      match effect {
        SettingsEffect::ReconfigurePlayback => self.reconfigure_playback_controller(),
        SettingsEffect::IntroModeChanged(mode) => {
          self.intro_mode = mode;
          self.dispatch_playback(
            PlaybackIntent::SetIntroMode(session_intro_mode(mode)),
            sender,
          );
        }
        SettingsEffect::ReconnectRemoteControl => self.reconnect_remote_control(sender),
        SettingsEffect::RefreshConnectionStatus => self.refresh_connection_status(sender),
        SettingsEffect::SetImageCacheEnabled(enabled) => {
          self.artwork.set_disk_cache_enabled(enabled);
        }
        SettingsEffect::RefreshImageCacheStats => {
          if self.image_cache_clearing {
            continue;
          }
          let token = self.requests.begin_image_cache();
          let artwork = Arc::clone(&self.artwork);
          sender.oneshot_command(async move {
            AppCommand::ImageCacheStats {
              token,
              result: artwork.disk_cache_stats().await.map_err(|_| ()),
            }
          });
        }
        SettingsEffect::ClearImageCache => {
          if self.image_cache_clearing {
            continue;
          }
          self.image_cache_clearing = true;
          let token = self.requests.begin_image_cache();
          let artwork = Arc::clone(&self.artwork);
          sender.oneshot_command(async move {
            let result = async {
              artwork.clear_disk_cache().await.map_err(|_| ())?;
              artwork.disk_cache_stats().await.map_err(|_| ())
            }
            .await;
            AppCommand::ImageCacheCleared { token, result }
          });
        }
        SettingsEffect::Diagnostic(level, category, message) => {
          self.record_diagnostic(level, category, message);
        }
      }
    }
  }

  fn dispatch_login_event(&mut self, event: LoginEvent, sender: &ComponentSender<Self>) {
    if let LoginEvent::SavedSessionStored { session, result } = &event {
      self.apply_saved_session_storage_status(*session, result);
    }
    let effects = self.with_login_context(|login, cx| login.handle_event(event, cx));
    self.execute_login_effects(effects, sender);
    self.login.render_saved_profiles(&self.saved_profiles);
    self.render_saved_profile_settings();
  }

  fn with_login_context<R>(
    &mut self,
    f: impl FnOnce(&mut LoginPage, &mut LoginContext<'_>) -> R,
  ) -> R {
    let can_start_login = self.playback_can_start_login();
    let connection = self.connection;
    let mut cx = LoginContext {
      gate: &mut self.requests,
      saved_profiles: &mut self.saved_profiles,
      active_saved_profile: &mut self.active_saved_profile,
      connection,
      can_start_login,
    };
    f(&mut self.login, &mut cx)
  }

  fn execute_login_effects(&mut self, effects: Vec<LoginEffect>, sender: &ComponentSender<Self>) {
    for effect in effects {
      match effect {
        LoginEffect::AuthStarted => {
          self.browse.model.reset();
          self.connection = ConnectionPhase::Connecting;
          self.home = LoadState::Loading;
        }
        LoginEffect::Authenticated {
          client,
          stored_session,
        } => self.finish_login(client, stored_session, sender),
        LoginEffect::AuthFailed { message } => {
          self.connection = ConnectionPhase::Failed;
          self.home = LoadState::Failed(message);
        }
        LoginEffect::InvalidInput => {
          self.connection = ConnectionPhase::Failed;
        }
        LoginEffect::PersistPrefill(prefill) => {
          let remember = prefill.remember;
          let warning = if remember {
            config::save(&prefill)
          } else {
            config::clear()
          }
          .err()
          .map(|_| {
            if remember {
              "Signed in, but sign-in details could not be saved on this device."
            } else {
              "Signed in, but remembered sign-in details could not be cleared on this device."
            }
          });
          self.login.apply_prefill_warning(warning);
          if warning.is_some() {
            self.record_diagnostic(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Config,
              "Sign-in succeeded, but the local sign-in configuration update failed.",
            );
          }
        }
        LoginEffect::Diagnostic(level, category, message) => {
          self.record_diagnostic(level, category, message);
        }
        LoginEffect::Cancelled => {
          self.connection = ConnectionPhase::SignedOut;
          self.home = LoadState::Idle;
        }
        LoginEffect::ProfileBusyChanged => self.sync_profile_busy_widgets(),
        LoginEffect::Disconnect => self.disconnect(sender),
        LoginEffect::LoadSavedProfiles => self.load_saved_profiles(sender),
        LoginEffect::RunPasswordAuth {
          session,
          credentials,
        } => {
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
            AppCommand::LoginEvent(LoginEvent::Login {
              session,
              client,
              result,
            })
          });
        }
        LoginEffect::RunQuickConnect {
          session,
          server_url,
          mut cancellation,
        } => {
          let client = configured_client(&config::load());
          let command_client = Arc::clone(&client);
          sender.command(move |output, shutdown| {
            shutdown
              .register(async move {
                let emit = {
                  let output = output.clone();
                  move |event| output.send(AppCommand::LoginEvent(event)).is_ok()
                };
                let operation = login::quick_connect_workflow(
                  command_client,
                  server_url,
                  session,
                  emit,
                  login::QUICK_CONNECT_POLL_INTERVAL,
                  login::QUICK_CONNECT_TIMEOUT,
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
        LoginEffect::RunRestore { session, key } => {
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
            AppCommand::LoginEvent(LoginEvent::Login {
              session,
              client,
              result,
            })
          });
        }
        LoginEffect::RunForget {
          session,
          key,
          sign_out,
        } => {
          let store = self.auth_store.clone();
          let command_key = key.clone();
          sender.oneshot_command(async move {
            let result = run_auth_operation(move || store.remove_profile(&command_key))
              .await
              .map_err(|_| "The saved sign-in could not be forgotten.".to_string())
              .and_then(|result| {
                result.map_err(|error| format!("Saved sign-in could not be forgotten: {error}."))
              });
            AppCommand::LoginEvent(LoginEvent::ForgotProfile {
              session,
              key,
              sign_out,
              result,
            })
          });
        }
      }
    }
  }

  fn apply_saved_session_storage_status(
    &mut self,
    session: SessionToken,
    result: &Result<(SavedProfileKey, Vec<SavedProfileSummary>), String>,
  ) {
    let is_current = self.requests.is_current_session(session)
      && matches!(self.connection, ConnectionPhase::Connected);
    match result {
      Ok(_) => {
        if is_current {
          self.settings.set_storage_status(
            "This session is stored securely in Linux Secret Service.",
            true,
          );
        }
      }
      Err(message) => {
        if is_current {
          self.settings.set_storage_status(message, true);
        }
      }
    }
    if !is_current {
      self.settings.set_storage_status("", false);
    }
  }

  fn load_saved_profiles(&mut self, sender: &ComponentSender<Self>) {
    let store = self.auth_store.clone();
    sender.oneshot_command(async move {
      let result = run_auth_operation(move || store.load_profiles())
        .await
        .map_err(|_| "Secure saved sign-ins could not be loaded.".to_string())
        .and_then(|result| result.map_err(|error| format!("Saved sign-ins unavailable: {error}.")));
      AppCommand::LoginEvent(LoginEvent::SavedProfiles(result))
    });
  }

  fn sync_profile_busy_widgets(&self) {
    let busy = self.login.is_profile_busy();
    let connected = matches!(self.connection, ConnectionPhase::Connected);
    self.ui.disconnect_button.set_sensitive(connected && !busy);
    self.settings.set_disconnect_sensitive(connected && !busy);
    self
      .settings
      .set_forget_sensitive(self.active_saved_profile.is_some() && !busy);
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
            .playback_session
            .view()
            .now_playing
            .map(|view| view.muted)
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
        let token = self.requests.begin_remote_play();
        let start_position = request
          .start_position_ticks
          .filter(|ticks| *ticks > 0)
          .map(|ticks| PlaybackStartPosition::At(ticks as f64 / 10_000_000.0))
          .unwrap_or(PlaybackStartPosition::Beginning);
        let client = Arc::clone(&self.client);
        sender.oneshot_command(async move {
          let result = client
            .library()
            .item_detail(item_id)
            .await
            .map_err(|error| error.to_string());
          AppCommand::RemotePlay {
            token,
            start_position,
            result,
          }
        });
      }
    }
  }

  fn start_remote_session(&mut self, sender: &ComponentSender<Self>) {
    let token = self.requests.begin_remote();
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
        let url = client.playback().websocket_url().map_err(|_| ())?;
        let user_agent = client.playback().websocket_user_agent();
        socket
          .connect_with_user_agent(&url, Some(&user_agent))
          .await
          .map_err(|_| ())?;
        let validated = finalize_remote_target(&client).await?;
        let receiver = socket.take_event_receiver().ok_or(())?;
        Ok::<_, ()>((receiver, validated))
      }
      .await;
      match result {
        Ok((receiver, validated)) => AppCommand::RemoteReady {
          token,
          socket,
          receiver,
          validated,
        },
        Err(()) => AppCommand::RemoteFailed { token },
      }
    });
  }

  fn stop_remote_session(&mut self, quit_gate: Option<&ComponentSender<Self>>) {
    let token = self.requests.begin_remote();
    self.remote_state = RemoteControlState::Unavailable;
    if let Some(socket) = self.remote_socket.take() {
      if let Some(sender) = quit_gate {
        self.remote_disconnect_pending = true;
        let settle_sender = sender.clone();
        relm4::spawn(async move {
          socket.disconnect().await;
          settle_sender.input(AppMessage::RemoteDisconnectSettled(token));
        });
        let timeout_sender = sender.clone();
        gtk::glib::timeout_add_local_once(Duration::from_secs(2), move || {
          timeout_sender.input(AppMessage::RemoteDisconnectSettled(token));
        });
      } else {
        relm4::spawn(async move {
          socket.disconnect().await;
        });
      }
    }
  }

  fn finish_login(
    &mut self,
    client: Arc<JellyfinClient>,
    stored_session: Option<SavedSession>,
    sender: &ComponentSender<Self>,
  ) {
    self.client = client;
    self
      .settings
      .set_intro_skip_visible(self.client.supports_intro_skipper());
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
    match PlaybackController::discover(
      Arc::clone(&self.client),
      playback_controller_config(&settings),
    ) {
      Ok(controller) => {
        self.playback_controller = Some(controller);
        self.playback_engine_error = None;
        self.apply_playback_event(PlaybackEvent::EngineAvailability(true), sender);
      }
      Err(error) => {
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Playback,
          format!("External MPV playback is unavailable: {error}."),
        );
        self.playback_controller = None;
        self.playback_engine_error = Some(format!(
          "Playback is unavailable: {error}. Install MPV and try again."
        ));
        self.apply_playback_event(PlaybackEvent::EngineAvailability(false), sender);
      }
    }
    self.connection = ConnectionPhase::Connected;
    self.start_remote_session(sender);
    self.home = LoadState::Loading;
    self.shortcuts.clear();
    self.shortcuts_error = None;
    if let Some(session_to_save) = stored_session {
      self.start_persist_session(session_to_save, sender);
    } else {
      self
        .settings
        .set_storage_status("The connected session could not be saved securely.", true);
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

  fn start_persist_session(&mut self, session: SavedSession, sender: &ComponentSender<Self>) {
    // Preserve secure-storage intent ordering: Forget cannot overtake an older pending save and
    // then be undone when that save eventually acquires the keyring lock.
    self.login.set_profile_busy(true);
    self.sync_profile_busy_widgets();
    self
      .settings
      .set_storage_status("Saving this session securely…", true);
    let store = self.auth_store.clone();
    let token = self.requests.current_session();
    sender.oneshot_command(async move {
      let result = run_auth_operation(move || store.save_session(session))
        .await
        .map_err(|_| "The session could not be saved securely.".to_string())
        .and_then(|result| {
          result.map_err(|error| format!("The session could not be saved securely: {error}."))
        });
      AppCommand::LoginEvent(LoginEvent::SavedSessionStored {
        session: token,
        result,
      })
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
    self.login.reset_flow();
    self.artwork.reset_session();
    self.diagnostics.reset_coalescing();
    self.artwork_view = self.artwork_view.saturating_add(1);
    self.artwork_targets.clear();
    let _client = std::mem::replace(&mut self.client, Arc::new(JellyfinClient::new()));
    self.dispatch_playback(PlaybackIntent::Disconnect, sender);
    self.apply_playback_event(PlaybackEvent::EngineAvailability(false), sender);
    self.playback_item = None;
    self.playback_artwork_image_id = None;
    self.playback_engine_error = None;
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
    self.requests.set_detail_item(None);
    self.streams = LoadState::Idle;
    self.season_neighbors = LoadState::Idle;
    self.season = None;
    self.invalidate_user_data_update();
    self.ui.search.set_text("");
    self.ui.playback_bar.set_visible(false);
    self.ui.search.set_sensitive(false);
    // Activating the static group anchor first clears any dynamic library shortcut,
    // then deactivating it leaves the signed-out shell with no selected destination.
    self.ui.nav_home.set_active(true);
    self.ui.nav_home.set_active(false);
    self.ui.disconnect_button.set_sensitive(false);
    self.settings.set_disconnect_sensitive(false);
    self.ui.connection_status.set_label("Not connected");
    self.render_connection_settings();
    self.settings.set_intro_skip_visible(false);
    self.settings.close();
    self.ui.root.set_content(Some(self.login.root()));
    let can_login = self.playback_can_start_login();
    self.login.set_controls_sensitive(can_login);
    self.login.render_saved_profiles(&self.saved_profiles);
    self.render_saved_profile_settings();
    self.login.set_status(if can_login {
      "Disconnected."
    } else {
      "Stopping native playback before another connection can start…"
    });
  }

  fn request_quit(&mut self, sender: &ComponentSender<Self>) {
    if self.quitting {
      return;
    }
    self.quitting = true;
    self.requests.disconnect();
    self.login.reset_flow();
    self.artwork.reset_session();
    self.invalidate_user_data_update();
    self.stop_remote_session(Some(sender));
    self.dispatch_playback(PlaybackIntent::Quit, sender);
    if self.playback_session.view().quit_may_proceed && !self.remote_disconnect_pending {
      relm4::main_adw_application().quit();
    }
  }

  fn render_authenticated(&mut self, sender: &ComponentSender<Self>) {
    self.ui.root.set_content(Some(&self.ui.authenticated));
    self.ui.search.set_sensitive(true);
    self
      .ui
      .disconnect_button
      .set_sensitive(!self.login.is_profile_busy());
    self
      .settings
      .set_disconnect_sensitive(!self.login.is_profile_busy());
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

  fn render_saved_profile_settings(&self) {
    self.settings.render_profiles(
      &self.saved_profiles,
      &self.active_saved_profile,
      self.connection,
      self.login.is_profile_busy(),
    );
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
        session: self.requests.current_session(),
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
      session: self.requests.current_session(),
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
        session: self.requests.current_session(),
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
    self.requests.set_detail_item(Some(item.id.clone()));
    self.recommendations = LoadState::Loading;
    self.streams = LoadState::Loading;
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
    let recommendation_token = self
      .requests
      .begin_detail_aux(DetailAuxKind::Recommendations);
    let stream_token = self.requests.begin_detail_aux(DetailAuxKind::Streams);
    let season_neighbor_token = self
      .requests
      .begin_detail_aux(DetailAuxKind::SeasonNeighbors);
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
    if let Some(token) = recommendation_token {
      let client = Arc::clone(&self.client);
      let item_id = item.id.clone();
      sender.oneshot_command(async move {
        let result = client
          .library()
          .similar_video(item_id)
          .await
          .map_err(|error| error.to_string());
        AppCommand::Recommendations { token, result }
      });
    }
    if let Some(token) = stream_token {
      let client = Arc::clone(&self.client);
      let stream_item_id = item.id.clone();
      sender.oneshot_command(async move {
        let result = client
          .library()
          .item_streams(stream_item_id)
          .await
          .map_err(|error| error.to_string());
        AppCommand::Streams { token, result }
      });
    }
    if let (Some((series_id, season_number)), Some(token)) =
      (season_neighbor_request, season_neighbor_token)
    {
      let client = Arc::clone(&self.client);
      let item_id = item.id.clone();
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
        AppCommand::SeasonNeighbors { token, result }
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
      self.requests.cancel_detail_loads();
      self.detail = LoadState::Ready(parent.content);
      self.season = parent.season;
      self
        .requests
        .set_detail_item(self.current_detail_identity().map(str::to_owned));
      self.recommendations = LoadState::Loading;
      self.streams = LoadState::Idle;
      self.season_neighbors = LoadState::Idle;
      let _ = self.requests.begin_detail_aux(DetailAuxKind::Streams);
      let _ = self
        .requests
        .begin_detail_aux(DetailAuxKind::SeasonNeighbors);
      if let Some(token) = self
        .requests
        .begin_detail_aux(DetailAuxKind::Recommendations)
      {
        let client = Arc::clone(&self.client);
        let item_id = self
          .current_detail_identity()
          .expect("parent detail item was just recorded")
          .to_owned();
        sender.oneshot_command(async move {
          let result = client
            .library()
            .similar_video(item_id)
            .await
            .map_err(|error| error.to_string());
          AppCommand::Recommendations { token, result }
        });
      }
      self.render_detail(sender);
      return;
    }
    if self.season.is_some() {
      self.requests.cancel_detail_loads();
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
    self.requests.invalidate_detail_aux(DetailAuxKind::UserData);
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
    let Some(token) = self.requests.begin_detail_aux(DetailAuxKind::UserData) else {
      return;
    };
    self.user_data_busy = true;
    self.user_data_error = None;
    self.render_detail(sender);
    let client = Arc::clone(&self.client);
    sender.oneshot_command(async move {
      let result = client
        .library()
        .update_user_data(VideoUserDataUpdateRequest { item_id, action })
        .await
        .map_err(|_| "Could not update this item's library state.".to_owned());
      AppCommand::UserData { token, result }
    });
  }

  fn finish_user_data_update(
    &mut self,
    token: DetailAuxToken,
    result: Result<VideoUserDataUpdate, String>,
    sender: &ComponentSender<Self>,
  ) {
    if !self.requests.finish_detail_aux(token) {
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

  fn playback_can_start_login(&self) -> bool {
    login::can_start_login(self.connection) && self.playback_session.view().can_start_login
  }

  fn playback_controls_enabled(&self) -> bool {
    let view = self.playback_session.view();
    view.engine_available && !view.busy
  }

  fn intro_availability(&self) -> IntroAvailability {
    IntroAvailability {
      mode: session_intro_mode(self.intro_mode),
      skipper_available: self.client.supports_intro_skipper(),
    }
  }

  fn start_playback(
    &mut self,
    item: Playable,
    position: PlaybackStartPosition,
    sender: &ComponentSender<Self>,
  ) {
    self.dispatch_playback(
      PlaybackIntent::Start {
        item,
        position,
        intro: self.intro_availability(),
      },
      sender,
    );
  }

  fn dispatch_playback(&mut self, intent: PlaybackIntent, sender: &ComponentSender<Self>) {
    self.apply_playback_input(PlaybackInput::Intent(intent), sender);
  }

  fn apply_playback_event(&mut self, event: PlaybackEvent, sender: &ComponentSender<Self>) {
    self.apply_playback_input(PlaybackInput::Event(event), sender);
  }

  fn apply_playback_input(&mut self, input: PlaybackInput, sender: &ComponentSender<Self>) {
    let effects = self.playback_session.handle(input, Instant::now());
    self.execute_playback_effects(effects, sender);
    self.render_playback_bar();
  }

  fn execute_playback_effects(
    &mut self,
    effects: Vec<PlaybackEffect>,
    sender: &ComponentSender<Self>,
  ) {
    for effect in effects {
      match effect {
        PlaybackEffect::Controller(id, command) => {
          self.execute_controller_command(id, command, sender);
        }
        PlaybackEffect::FetchIntroRanges(id, item_id) => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            let result = client
              .playback()
              .get_intro_skipper_ranges(&item_id)
              .await
              .map_err(|_| ());
            AppCommand::IntroRangesSettled { id, result }
          });
        }
        PlaybackEffect::LookupAdjacent(id, direction) => {
          let Some(item) = self.playback_item.clone() else {
            self.apply_playback_event(
              PlaybackEvent::AdjacentSettled {
                id,
                direction,
                result: Ok(None),
              },
              sender,
            );
            continue;
          };
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            let result = match direction {
              AdjacentDirection::Previous => client.playback().get_previous_episode(&item).await,
              AdjacentDirection::Next => client.playback().get_next_episode(&item).await,
            }
            .map_err(|_| ());
            AppCommand::AdjacentSettled {
              id,
              direction,
              result,
            }
          });
        }
      }
    }
  }

  fn execute_controller_command(
    &mut self,
    id: EffectId,
    command: ControllerCommand,
    sender: &ComponentSender<Self>,
  ) {
    if let ControllerCommand::Start { item, .. } = &command {
      self.playback_item = Some(media_item_from_playable(item));
      if let Some(image_id) = playable_artwork_image_id(item) {
        self.playback_artwork_image_id = Some(image_id);
        self.queue_playback_artwork(sender);
      }
    }
    let Some(mut controller) = self.playback_controller.take() else {
      let settlement = match command {
        ControllerCommand::Shutdown => ControllerSettlement::Shutdown(Vec::new()),
        ControllerCommand::Start { .. } => {
          ControllerSettlement::Started(Err(PlaybackError::MpvNotFound))
        }
        ControllerCommand::Stop => {
          ControllerSettlement::Stopped(Err(PlaybackError::NoActivePlayback))
        }
        ControllerCommand::Refresh => ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: PlaybackSnapshot {
              now_playing: None,
              transport: PlayerState::default(),
            },
            state: PlaybackRefreshState::Idle,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
        ControllerCommand::SelectAudioTrack(_) | ControllerCommand::SelectSubtitleTrack(_) => {
          ControllerSettlement::TrackSelected(Err(PlaybackError::NoActivePlayback))
        }
        ControllerCommand::ShowText { .. } => {
          ControllerSettlement::OsdShown(Err(PlaybackError::NoActivePlayback))
        }
        ControllerCommand::SetPaused(_)
        | ControllerCommand::Seek(_)
        | ControllerCommand::SetVolume(_)
        | ControllerCommand::SetMuted(_) => {
          ControllerSettlement::Controlled(Err(PlaybackError::NoActivePlayback))
        }
      };
      self.finish_controller_settlement(id, None, settlement, None, sender);
      return;
    };
    sender.oneshot_command(async move {
      let mut tracks = None;
      let (settlement, controller) = match command {
        ControllerCommand::Start { item, position } => {
          let result = controller.play(item, position).await;
          if result.is_ok() {
            tracks = Some(controller.tracks().await);
          }
          (ControllerSettlement::Started(result), Some(controller))
        }
        ControllerCommand::SetPaused(paused) => (
          ControllerSettlement::Controlled(controller.set_paused(paused).await),
          Some(controller),
        ),
        ControllerCommand::Seek(position) => (
          ControllerSettlement::Controlled(controller.seek(position).await),
          Some(controller),
        ),
        ControllerCommand::SetVolume(volume) => (
          ControllerSettlement::Controlled(controller.set_volume(volume).await),
          Some(controller),
        ),
        ControllerCommand::SetMuted(muted) => (
          ControllerSettlement::Controlled(controller.set_muted(muted).await),
          Some(controller),
        ),
        ControllerCommand::SelectAudioTrack(id) => (
          ControllerSettlement::TrackSelected(controller.select_audio_track(id).await),
          Some(controller),
        ),
        ControllerCommand::SelectSubtitleTrack(id) => (
          ControllerSettlement::TrackSelected(controller.select_subtitle_track(id).await),
          Some(controller),
        ),
        ControllerCommand::ShowText { text, duration_ms } => (
          ControllerSettlement::OsdShown(controller.show_text(&text, duration_ms).await),
          Some(controller),
        ),
        ControllerCommand::Stop => (
          ControllerSettlement::Stopped(controller.stop().await),
          Some(controller),
        ),
        ControllerCommand::Refresh => {
          let outcome = controller.refresh().await;
          let client_messages = controller.take_client_messages();
          (
            ControllerSettlement::Refreshed {
              outcome,
              client_messages,
            },
            Some(controller),
          )
        }
        ControllerCommand::Shutdown => {
          let outcome = controller.shutdown().await;
          (ControllerSettlement::Shutdown(outcome.warnings), None)
        }
      };
      AppCommand::PlaybackSettled {
        id,
        controller: controller.map(Box::new),
        settlement,
        tracks,
      }
    });
  }

  fn finish_controller_settlement(
    &mut self,
    id: EffectId,
    controller: Option<Box<PlaybackController>>,
    settlement: ControllerSettlement,
    tracks: Option<Result<Vec<TrackInfo>, PlaybackError>>,
    sender: &ComponentSender<Self>,
  ) {
    if let Some(mut controller) = controller {
      if self.playback_reconfigure_pending {
        self.playback_reconfigure_pending = false;
        if controller
          .configure_for_next_start(playback_controller_config(&config::load()))
          .is_err()
        {
          self.show_settings_failure(
            "Settings were saved, but no MPV executable is available for the next start.",
          );
        }
      }
      self.playback_controller = Some(*controller);
    }
    self.record_playback_settlement(&settlement);
    self.apply_playback_event(PlaybackEvent::ControllerSettled { id, settlement }, sender);
    if self.playback_session.view().now_playing.is_none() {
      self.playback_item = None;
      self.playback_artwork_image_id = None;
      self.queue_playback_artwork(sender);
    }
    if let Some(tracks) = tracks {
      self.apply_playback_event(PlaybackEvent::TracksSettled { id, result: tracks }, sender);
    }
    let view = self.playback_session.view();
    if view.quit_may_proceed {
      if self.remote_disconnect_pending {
        return;
      }
      relm4::main_adw_application().quit();
      return;
    }
    if matches!(self.connection, ConnectionPhase::SignedOut) {
      let can_login = self.playback_can_start_login();
      self.login.set_controls_sensitive(can_login);
      if can_login {
        self.login.set_status("Disconnected.");
      }
    }
  }

  fn record_playback_settlement(&mut self, settlement: &ControllerSettlement) {
    match settlement {
      ControllerSettlement::Started(Ok(outcome)) => {
        if !outcome.warnings.is_empty() {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::Playback,
            "Playback completed with one or more non-fatal reporting warnings.",
          );
        }
        if let Some(item) = &self.playback_item {
          self.record_diagnostic(
            DiagnosticLevel::Info,
            DiagnosticCategory::Playback,
            format!("Playback started for “{}”.", item.name),
          );
        }
      }
      ControllerSettlement::Started(Err(error)) => {
        let message = format!("Could not start playback: {error}.");
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Playback,
          &message,
        );
        self.add_toast(&message);
      }
      ControllerSettlement::Controlled(Ok(outcome)) => {
        if !outcome.warnings.is_empty() {
          self.record_diagnostic(
            DiagnosticLevel::Warning,
            DiagnosticCategory::Playback,
            "Playback completed with one or more non-fatal reporting warnings.",
          );
        }
      }
      ControllerSettlement::Controlled(Err(error)) => {
        let message = format!("{error}.");
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Playback,
          &message,
        );
        self.add_toast(&message);
      }
      ControllerSettlement::Stopped(Ok(_)) => {
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Playback,
          "Playback stopped.",
        );
      }
      ControllerSettlement::Stopped(Err(error)) => {
        let message = format!("Could not stop playback: {error}.");
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Playback,
          &message,
        );
        self.add_toast(&message);
      }
      ControllerSettlement::TrackSelected(Ok(_)) => {
        self.record_diagnostic(
          DiagnosticLevel::Info,
          DiagnosticCategory::Playback,
          "MPV track selection completed.",
        );
        self.add_toast("Track switched.");
      }
      ControllerSettlement::TrackSelected(Err(_)) => {
        self.record_diagnostic(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Playback,
          "MPV track selection failed.",
        );
        self.add_toast("Could not switch that track.");
      }
      ControllerSettlement::Refreshed { outcome, .. } => {
        if outcome.snapshot.now_playing.is_none() {
          self.record_diagnostic(
            DiagnosticLevel::Info,
            DiagnosticCategory::Playback,
            "Playback session ended.",
          );
        }
      }
      ControllerSettlement::OsdShown(Err(error)) => {
        let message = format!("Could not show the Intro Skipper prompt: {error}.");
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Playback,
          &message,
        );
      }
      ControllerSettlement::Shutdown(warnings) => {
        if matches!(self.connection, ConnectionPhase::SignedOut) && !warnings.is_empty() {
          self.login.set_status(
            "Disconnected. Playback stopped, but its final server progress could not be updated.",
          );
        }
      }
      ControllerSettlement::OsdShown(Ok(())) => {}
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
    let client = Arc::clone(&self.client);
    let session = self.requests.current_session();
    sender.oneshot_command(async move {
      AppCommand::ConnectionStatus {
        session,
        result: client.playback().validate_session().await.map_err(|_| ()),
      }
    });
  }

  fn render_connection_settings(&self) {
    let connected = matches!(self.connection, ConnectionPhase::Connected);
    let remote_status = match self.remote_state {
      RemoteControlState::Unavailable => "Remote Control unavailable",
      RemoteControlState::Connecting => "Remote Control connecting",
      RemoteControlState::Available => "Remote Control available",
      RemoteControlState::Lost => "Remote Control connection lost",
    };
    self.settings.render_connection(&ConnectionView {
      connected,
      server_url: &self.login.server_url_text(),
      user: &self.login.username_text(),
      remote_status,
      reconnect_sensitive: connected && self.client.supports_remote_control(),
      refresh_sensitive: connected,
    });
  }
  fn reconfigure_playback_controller(&mut self) {
    let settings = config::load();
    let playback_config = playback_controller_config(&settings);
    if self.playback_controller.is_none() {
      if self.playback_session.view().busy {
        self.playback_reconfigure_pending = true;
        return;
      }
      match PlaybackController::discover(Arc::clone(&self.client), playback_config) {
        Ok(controller) => {
          self.playback_controller = Some(controller);
          self.playback_engine_error = None;
          self.playback_reconfigure_pending = false;
          self.playback_session.handle(
            PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
            Instant::now(),
          );
          self
            .settings
            .set_config_status("Saved. MPV is available for the next playback start.");
          self.render_playback_bar();
        }
        Err(_) => {
          self.playback_reconfigure_pending = false;
          self.show_settings_failure(
            "Settings were saved, but no MPV executable is available for the next start.",
          );
        }
      }
      return;
    }

    let result = self
      .playback_controller
      .as_mut()
      .map(|controller| controller.configure_for_next_start(playback_config));
    match result {
      Some(Ok(())) => {
        self.playback_reconfigure_pending = false;
        self
          .settings
          .set_config_status("Saved. Player changes apply on the next MPV start.");
      }
      Some(Err(_)) | None => {
        self.playback_reconfigure_pending = false;
        self.show_settings_failure(
          "Settings were saved, but no MPV executable is available for the next start.",
        );
      }
    }
  }

  fn show_settings_failure(&mut self, message: &str) {
    self.settings.set_config_status(message);
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
    let Some(image_id) = self.playback_artwork_image_id.clone() else {
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
    let session = self.requests.current_session();
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
    if self.playback_artwork_image_id.is_some() {
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
    let session = self.requests.current_session();
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
      action.set_sensitive(self.playback_controls_enabled());
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
    primary.set_sensitive(self.playback_controls_enabled());
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
    let session = self.requests.current_session();
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
    play.set_sensitive(self.playback_controls_enabled() && detail.can_play);
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
      resume.set_sensitive(self.playback_controls_enabled());
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
    self
      .diagnostics_page
      .apply_change(change, &self.diagnostics);
  }

  fn record_artwork_failure(&mut self) {
    let change = self.diagnostics.record_coalesced(
      "artwork-load-failure",
      DiagnosticLevel::Warning,
      DiagnosticCategory::Artwork,
      "Artwork could not be loaded or decoded; a fallback is shown.",
    );
    self
      .diagnostics_page
      .apply_change(change, &self.diagnostics);
  }

  fn add_toast(&self, title: impl AsRef<str>) {
    self
      .ui
      .toast_overlay
      .add_toast(adw::Toast::new(title.as_ref()));
  }

  fn render_playback_bar(&self) {
    let view = self.playback_session.view();
    let now_playing = view.now_playing.as_ref();
    self.ui.playback_bar.set_visible(now_playing.is_some());
    let title = now_playing
      .map(|playing| playing.item.title.as_str())
      .unwrap_or("");
    self.ui.playback_title.set_label(title);
    if let Some(prompt) = view.intro_prompt {
      self.ui.playback_title.set_tooltip_text(Some(&format!(
        "{} skip available",
        intro_skip_label(prompt.kind)
      )));
    } else {
      self.ui.playback_title.set_tooltip_text(None::<&str>);
    }
    let subtitle = playback_meta_subtitle(self.playback_item.as_ref());
    self.ui.playback_subtitle.set_label(&subtitle);
    self.ui.playback_subtitle.set_visible(!subtitle.is_empty());
    let error = view.notice.as_ref().and_then(|notice| match notice {
      PlaybackNotice::Failed(_) => playback_notice(notice),
      _ => None,
    });
    let status = playback_bar_status(
      error.as_deref(),
      self.playback_engine_error.as_deref(),
      view.busy,
    );
    match status {
      Some((icon, message)) => {
        self.ui.playback_status_icon.set_icon_name(Some(icon));
        self.ui.playback_status_label.set_label(message);
        self.ui.playback_info.set_visible_child_name("status");
      }
      None => self.ui.playback_info.set_visible_child_name("meta"),
    }
    let active = now_playing.is_some() && view.engine_available && !view.busy;
    self.ui.pause_button.set_sensitive(active);
    self.ui.stop_button.set_sensitive(active);
    self.ui.seek.set_sensitive(active);
    self.ui.volume.set_sensitive(active);
    self.ui.mute_button.set_sensitive(active);
    if let Some(playing) = now_playing {
      let duration = playing.duration_seconds.unwrap_or(playing.position_seconds);
      self
        .ui
        .position_label
        .set_label(&format_duration(playing.position_seconds));
      self.ui.duration_label.set_label(&format_duration(duration));
      self.ui.pause_button.set_icon_name(if playing.paused {
        "media-playback-start-symbolic"
      } else {
        "media-playback-pause-symbolic"
      });
      self
        .ui
        .pause_button
        .set_tooltip_text(Some(if playing.paused {
          "Resume playback"
        } else {
          "Pause playback"
        }));
      self.ui.mute_button.set_icon_name(if playing.muted {
        "audio-volume-muted-symbolic"
      } else {
        "audio-volume-high-symbolic"
      });
      self
        .ui
        .mute_button
        .set_tooltip_text(Some(if playing.muted { "Unmute" } else { "Mute" }));
      self.ui.playback_controls_syncing.set(true);
      self.ui.seek.set_range(0.0, duration.max(1.0));
      let position = playing.position_seconds.clamp(0.0, duration.max(1.0));
      if (self.ui.seek.value() - position).abs() > f64::EPSILON {
        self.ui.seek.set_value(position);
      }
      let volume = playing.volume.clamp(0.0, 100.0);
      if (self.ui.volume.value() - volume).abs() > f64::EPSILON {
        self.ui.volume.set_value(volume);
      }
      if self.ui.mute_button.is_active() != playing.muted {
        self.ui.mute_button.set_active(playing.muted);
      }
      self.ui.playback_controls_syncing.set(false);
    } else {
      self.ui.position_label.set_label("00:00");
      self.ui.duration_label.set_label("00:00");
    }
    self.render_track_controls(active, &view);
    self.render_adjacent_controls(active, &view);
  }

  fn render_track_controls(&self, active: bool, view: &SessionView) {
    self.ui.playback_controls_syncing.set(true);
    match &view.tracks {
      TracksView::Ready { tracks, .. } => {
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
      TracksView::Loading => {
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
      TracksView::Unavailable => {
        self.clear_track_lists();
        let reason = if !view.engine_available {
          self
            .playback_engine_error
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

  fn render_adjacent_controls(&self, active: bool, view: &SessionView) {
    let previous = &view.adjacent.previous;
    let next = &view.adjacent.next;
    let previous_available = matches!(previous, AdjacentAvailability::Available { .. });
    let next_available = matches!(next, AdjacentAvailability::Available { .. });
    self
      .ui
      .previous_button
      .set_sensitive(active && previous_available);
    self.ui.next_button.set_sensitive(active && next_available);
    let busy_reason = view
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
  fn new(sender: &ComponentSender<AppModel>, login: &gtk::ScrolledWindow) -> Self {
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
    root.set_content(Some(login));

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
    let about = adw::AboutDialog::new();
    about.set_application_name("JellyPilot");
    about.set_application_icon("video-x-generic");
    about.set_version(env!("CARGO_PKG_VERSION"));
    about.set_comments("A native media client for Jellyfin and Emby.");
    about.set_website("https://github.com/hewel/jellypilot");
    let application = relm4::main_adw_application();
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
    }
  }
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

fn playback_notice(notice: &PlaybackNotice) -> Option<String> {
  Some(match notice {
    PlaybackNotice::Finished => "Playback finished.".to_owned(),
    PlaybackNotice::Stopped => "Playback stopped.".to_owned(),
    PlaybackNotice::Failed(error) => format!("{error}."),
    PlaybackNotice::Warnings(warnings) => {
      let details = warnings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
      format!("Playback is active, but {details}.")
    }
  })
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

const fn session_intro_mode(mode: config::IntroMode) -> IntroSkipMode {
  match mode {
    config::IntroMode::Automatic => IntroSkipMode::Automatic,
    config::IntroMode::Manual => IntroSkipMode::Manual,
    config::IntroMode::Off => IntroSkipMode::Off,
  }
}

const fn intro_skip_label(kind: IntroSkipKind) -> &'static str {
  match kind {
    IntroSkipKind::Introduction => "Intro",
    IntroSkipKind::Credits => "Credits",
  }
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
) -> Option<(&'static str, &'a str)> {
  if let Some(error) = error {
    return Some(("dialog-error-symbolic", error));
  }
  if let Some(unavailable) = unavailable {
    return Some(("dialog-warning-symbolic", unavailable));
  }
  if busy {
    return Some(("content-loading-symbolic", "Buffering…"));
  }
  None
}

fn adjacent_control_reason(
  availability: &AdjacentAvailability,
  direction: AdjacentDirection,
) -> &str {
  match availability {
    AdjacentAvailability::Loading => "Checking adjacent episodes…",
    AdjacentAvailability::Available { .. } => match direction {
      AdjacentDirection::Previous => "Play previous episode",
      AdjacentDirection::Next => "Play next episode",
    },
    AdjacentAvailability::Unavailable => match direction {
      AdjacentDirection::Previous => "No previous episode is available.",
      AdjacentDirection::Next => "No next episode is available.",
    },
    AdjacentAvailability::Idle => "Episode navigation requires active episode playback.",
  }
}

fn playable_artwork_image_id(item: &Playable) -> Option<String> {
  match item {
    Playable::Library(item) => item
      .series_poster_image_id
      .clone()
      .or_else(|| item.artwork_image_id.clone()),
    Playable::Detail(item) => item
      .series_poster_image_id
      .clone()
      .or_else(|| item.artwork_image_id.clone()),
    Playable::Media(_) => None,
  }
}

fn media_item_from_playable(item: &Playable) -> MediaItem {
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
      run_time_ticks: runtime_seconds_to_ticks(item.runtime_seconds),
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
      run_time_ticks: runtime_seconds_to_ticks(item.runtime_seconds),
      overview: item.overview.clone(),
      series_primary_image_tag: None,
    },
    Playable::Media(item) => item.clone(),
  }
}

fn runtime_seconds_to_ticks(seconds: Option<f64>) -> Option<i64> {
  seconds
    .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
    .map(|seconds| (seconds * 10_000_000.0).round() as i64)
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

/// Reports this device as a controllable Playback Target after the command socket
/// connected, then checks session visibility. The server registers the session from
/// the socket and learns media control from this report, so both must happen before
/// any validation. Validation is informational: a fresh session may not be listed
/// yet, so it never fails setup. Returns whether validation succeeded.
async fn finalize_remote_target(client: &JellyfinClient) -> Result<bool, ()> {
  client
    .playback()
    .report_capabilities_for_checked(PlaybackEngineKind::ExternalMpv)
    .await
    .map_err(|_| ())?;
  Ok(client.playback().validate_session().await.is_ok())
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
  use jellypilot_media_server::{Credentials, MediaServerProvider};
  use std::future::Future;

  fn run_async<T>(future: impl Future<Output = T>) -> T {
    relm4::tokio::runtime::Builder::new_current_thread()
      .enable_io()
      .enable_time()
      .build()
      .expect("test runtime should build")
      .block_on(future)
  }

  /// Minimal canned-response HTTP server: one connection per response, request line
  /// captured per connection, `connection: close` so the client opens a fresh one.
  fn serve_http_responses(
    responses: Vec<(&'static str, &'static str)>,
  ) -> (String, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("fake server should bind");
    let addr = listener
      .local_addr()
      .expect("fake server should have an address");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
      for (status, body) in responses {
        let (mut stream, _) = listener.accept().expect("fake server should accept");
        let mut buffer = [0_u8; 8192];
        let read = stream
          .read(&mut buffer)
          .expect("fake server should read the request");
        tx.send(String::from_utf8_lossy(&buffer[..read]).into_owned())
          .expect("request log should send");
        let response = format!(
          "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
          body.len()
        );
        stream
          .write_all(response.as_bytes())
          .expect("fake server should write the response");
      }
    });
    (format!("http://{addr}"), rx)
  }

  async fn authenticated_client(server_url: String) -> JellyfinClient {
    let client = JellyfinClient::new();
    client
      .login()
      .authenticate(&Credentials {
        provider: MediaServerProvider::Jellyfin,
        server_url,
        username: "Ada".to_owned(),
        password: "correct horse battery staple".to_owned(),
      })
      .await
      .expect("authentication against the fake server should succeed");
    client
  }

  #[test]
  fn finalize_remote_target_reports_capabilities_before_informational_validation() {
    let (server_url, requests) = serve_http_responses(vec![
      (
        "200 OK",
        r#"{"User":{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#,
      ),
      (
        "200 OK",
        r#"{"ServerName":"Fake","Version":"10.10.0","Id":"server-1"}"#,
      ),
      ("200 OK", ""),
      ("500 Internal Server Error", r#"{"Message":"boom"}"#),
    ]);
    let client = run_async(async {
      let client = authenticated_client(server_url).await;
      let validated = finalize_remote_target(&client)
        .await
        .expect("a validation failure must not fail remote-target setup");
      assert!(!validated, "validation failed softly");
      client
    });
    drop(client);

    let _auth = requests.recv().expect("auth request captured");
    let _info = requests.recv().expect("info request captured");
    let capabilities = requests.recv().expect("capabilities request captured");
    let validation = requests.recv().expect("validation request captured");
    assert!(capabilities.starts_with("POST /Sessions/Capabilities"));
    assert!(validation.starts_with("GET /Sessions"));
  }

  #[test]
  fn finalize_remote_target_fails_when_capability_report_is_rejected() {
    let (server_url, _requests) = serve_http_responses(vec![
      (
        "200 OK",
        r#"{"User":{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#,
      ),
      (
        "200 OK",
        r#"{"ServerName":"Fake","Version":"10.10.0","Id":"server-1"}"#,
      ),
      ("500 Internal Server Error", r#"{"Message":"boom"}"#),
    ]);
    run_async(async {
      let client = authenticated_client(server_url).await;
      assert!(finalize_remote_target(&client).await.is_err());
    });
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
}
