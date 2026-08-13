use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use jellypilot_media_server::{
  Credentials, JellyfinClient, JellyfinError, MediaServerProvider, QuickConnectStatus,
  SavedSession, VideoHome, VideoItemDetail, VideoLibraryItem, VideoLibraryKind,
  VideoLibraryPageRequest, VideoLibraryPlayedFilter, VideoLibraryShortcut, VideoLibrarySort,
  VideoLibrarySortDirection, VideoSearchRequest, VideoSeason, VideoSeasonEpisodesPage,
  VideoSeasonEpisodesPageRequest, VideoShowDetail, VideoUserDataAction, VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
};
use relm4::gtk::prelude::*;
use relm4::tokio::sync::{oneshot, watch};
use relm4::{gtk, Component, ComponentParts, ComponentSender, RelmApp};
use zeroize::{Zeroize, Zeroizing};

use crate::artwork::{ArtworkAdapter, DecodedArtwork, FALLBACK_ARTWORK_ICON};
use crate::auth_storage::{AuthStore, SavedProfileKey, SavedProfileSummary};
use crate::browse_model::{
  BrowseEffect, BrowseModel, BrowsePagePayload, BrowsePageRequest, BrowsePageSettlement,
  BrowsePreferences, BrowseSource,
};
use crate::library_browse::LibraryBrowseView;
use crate::playback::{
  PlaybackController, PlaybackControllerConfig, PlaybackEndReason, PlaybackError, PlaybackOptions,
  PlaybackRefreshState, PlaybackSnapshot, PlaybackStartPosition, PlaybackWarning,
};
use crate::request_gate::{DetailToken, HomeToken, RequestGate, SessionToken};

const APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview";
const SMOKE_APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview.Smoke";
const SEASON_EPISODE_PAGE_SIZE: i32 = 30;
const QUICK_CONNECT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const QUICK_CONNECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

struct AppModel {
  client: Arc<JellyfinClient>,
  auth_store: AuthStore,
  saved_profiles: LoadState<Vec<SavedProfileSummary>>,
  active_saved_profile: Option<SavedProfileKey>,
  profile_operation_busy: bool,
  quick_connect_phase: QuickConnectPhase,
  quick_connect_cancellation: watch::Sender<u64>,
  artwork: Arc<ArtworkAdapter>,
  artwork_view: u64,
  artwork_slot: u64,
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
  season: Option<SeasonSelection>,
  user_data_busy: bool,
  user_data_sequence: u64,
  user_data_error: Option<String>,
  playback: PlaybackState,
  playback_cancellation: watch::Sender<u64>,
  playback_refresh_source: Option<gtk::glib::SourceId>,
  playback_cleanup_pending: bool,
  quitting: bool,
  ui: Ui,
}

struct ArtworkTarget {
  picture: gtk::Picture,
  fallback: gtk::Image,
}

#[derive(Default)]
struct PlaybackState {
  controller: Option<PlaybackController>,
  snapshot: Option<PlaybackSnapshot>,
  unavailable: Option<String>,
  error: Option<String>,
  notice: Option<String>,
  busy: bool,
  sequence: u64,
  pending: VecDeque<PlaybackRequest>,
}

enum PlaybackRequest {
  Library(VideoLibraryItem, PlaybackStartPosition),
  Detail(VideoItemDetail, PlaybackStartPosition),
  Paused(bool),
  Seek(f64),
  Volume(f64),
  Muted(bool),
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
  Stop,
  Refresh,
}

impl PlaybackRequest {
  const fn kind(&self) -> PlaybackRequestKind {
    match self {
      Self::Library(..) | Self::Detail(..) => PlaybackRequestKind::Start,
      Self::Paused(_) => PlaybackRequestKind::Paused,
      Self::Seek(_) => PlaybackRequestKind::Seek,
      Self::Volume(_) => PlaybackRequestKind::Volume,
      Self::Muted(_) => PlaybackRequestKind::Muted,
      Self::Stop => PlaybackRequestKind::Stop,
      Self::Refresh => PlaybackRequestKind::Refresh,
    }
  }
}

struct PlaybackCommandSuccess {
  snapshot: Option<PlaybackSnapshot>,
  warnings: Vec<PlaybackWarning>,
  notice: Option<String>,
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
  ShowNowPlaying,
  ShowSettings,
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
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  StopPlayback,
  RefreshPlayback,
  QuitRequested,
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
  Playback {
    session: u64,
    sequence: u64,
    controller: Box<PlaybackController>,
    result: Result<PlaybackCommandSuccess, PlaybackCommandFailure>,
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
  root: gtk::Box,
  login: gtk::ScrolledWindow,
  provider: gtk::DropDown,
  server_url: gtk::Entry,
  username: gtk::Entry,
  password: gtk::PasswordEntry,
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
  authenticated: gtk::Box,
  header: gtk::HeaderBar,
  sidebar_toggle: gtk::ToggleButton,
  connection_status: gtk::Label,
  search: gtk::SearchEntry,
  disconnect_button: gtk::Button,
  content: gtk::Stack,
  nav_home: gtk::ToggleButton,
  nav_now_playing: gtk::ToggleButton,
  nav_settings: gtk::ToggleButton,
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
  now_playing_status: gtk::Label,
  now_playing_notice: gtk::Label,
  position_label: gtk::Label,
  duration_label: gtk::Label,
  pause_button: gtk::Button,
  stop_button: gtk::Button,
  seek: gtk::Scale,
  volume: gtk::Scale,
  mute_button: gtk::ToggleButton,
  playback_controls_syncing: Rc<Cell<bool>>,
  settings_saved_profile: gtk::Label,
  settings_storage_status: gtk::Label,
  settings_disconnect_button: gtk::Button,
  forget_current_profile: gtk::Button,
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
    main_window = gtk::ApplicationWindow {
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
    root.set_titlebar(Some(&ui.header));
    root.set_child(Some(&ui.root));
    if smoke_test {
      let application = relm4::main_application();
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
    let model = Self {
      client: Arc::new(JellyfinClient::new()),
      auth_store: AuthStore::default(),
      saved_profiles: LoadState::Loading,
      active_saved_profile: None,
      profile_operation_busy: false,
      quick_connect_phase: QuickConnectPhase::Idle,
      quick_connect_cancellation,
      artwork: Arc::new(ArtworkAdapter::default()),
      artwork_view: 0,
      artwork_slot: 0,
      artwork_targets: HashMap::new(),
      requests: RequestGate::default(),
      connection: ConnectionPhase::SignedOut,
      home: LoadState::Idle,
      shortcuts: Vec::new(),
      shortcuts_error: None,
      browse: BrowseState::default(),
      detail: LoadState::Idle,
      detail_selection: None,
      detail_origin: None,
      detail_parent: None,
      season: None,
      user_data_busy: false,
      user_data_sequence: 0,
      user_data_error: None,
      playback: PlaybackState::default(),
      playback_cancellation,
      playback_refresh_source: Some(playback_refresh_source),
      playback_cleanup_pending: false,
      quitting: false,
      ui,
    };
    let widgets = view_output!();
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
      AppMessage::ShowNowPlaying => {
        self.navigate_to("now-playing");
        self.start_playback(PlaybackRequest::Refresh, &sender);
      }
      AppMessage::ShowSettings => self.navigate_to("settings"),
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
      AppMessage::TogglePaused => self.start_playback(
        PlaybackRequest::Paused(
          !self
            .playback
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.transport.paused),
        ),
        &sender,
      ),
      AppMessage::Seek(position) => self.start_playback(PlaybackRequest::Seek(position), &sender),
      AppMessage::SetVolume(volume) => {
        self.start_playback(PlaybackRequest::Volume(volume), &sender)
      }
      AppMessage::SetMuted(muted) => self.start_playback(PlaybackRequest::Muted(muted), &sender),
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
          Err(message) => LoadState::Failed(message),
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
          }
          Err(message) => {
            if is_current {
              self.active_saved_profile = None;
              self.ui.settings_storage_status.set_label(&message);
            }
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
          }
          Err(message) => {
            self.ui.saved_profiles_status.set_label(&message);
            self.ui.saved_profiles_status.set_visible(true);
            self.render_saved_profile_settings();
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
        if session != self.requests.session_generation() || view != self.artwork_view {
          return;
        }
        let Some(target) = self.artwork_targets.remove(&slot) else {
          return;
        };
        if let Ok(decoded) = result.and_then(|decoded| decoded.texture().map_err(|_| ())) {
          target.picture.set_paintable(Some(&decoded));
          target.fallback.set_visible(false);
        }
      }
      AppCommand::Playback {
        session,
        sequence,
        controller,
        result,
      } => {
        let controller = *controller;
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
        self.playback.controller = Some(controller);
        self.playback.busy = false;
        match result {
          Ok(success) => {
            self.playback.snapshot = success.snapshot;
            self.playback.error = None;
            self.playback.notice = playback_notice(success.notice, &success.warnings);
            if self.playback.snapshot.is_some() {
              self.show_page("now-playing");
            }
            self.render_now_playing();
          }
          Err(failure) => {
            if failure.clear_snapshot {
              self.playback.snapshot = None;
            }
            self.playback.error = Some(failure.message);
            self.show_page("now-playing");
            self.render_now_playing();
          }
        }
        if let Some(request) = self.playback.pending.pop_front() {
          self.start_playback(request, &sender);
        }
      }
      AppCommand::PlaybackShutdown {
        disposition,
        warnings,
      } => {
        if matches!(disposition, PlaybackShutdownDisposition::Disconnect) {
          self.playback_cleanup_pending = false;
        }
        if shutdown_completion_quits(self.quitting, disposition) {
          relm4::main_application().quit();
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
      return;
    }

    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.ui.quick_connect_code.set_label("");
    self.ui.quick_connect_code.set_visible(false);
    self.ui.quick_connect_status.set_label("");
    self.ui.quick_connect_spinner.stop();
    self.ui.quick_connect_spinner.set_visible(false);
    self.ui.password.set_text("");
    let session = self.prepare_login("Connecting and loading your libraries…");
    let credentials = SensitiveCredentials(Credentials {
      provider: provider_for(self.ui.provider.selected()),
      server_url,
      username,
      password,
    });
    // Authenticate an isolated candidate so a superseded login cannot mutate the active session.
    let client = Arc::new(JellyfinClient::new());
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
      return;
    }

    self.cancel_inflight_quick_connect();
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

    let client = Arc::new(JellyfinClient::new());
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

    let session = self.prepare_login("Restoring the saved sign-in…");
    let store = self.auth_store.clone();
    let client = Arc::new(JellyfinClient::new());
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
    if let Some(window) = relm4::main_application().active_window() {
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
    match result {
      Ok(()) => {
        let session_to_save = client.login().get_saved_session();
        self.active_saved_profile = None;
        self.client = client;
        self.artwork.reset_session();
        self.artwork = Arc::new(ArtworkAdapter::default());
        self.playback = match PlaybackController::discover(
          Arc::clone(&self.client),
          PlaybackControllerConfig::default(),
        ) {
          Ok(controller) => PlaybackState {
            controller: Some(controller),
            ..PlaybackState::default()
          },
          Err(error) => PlaybackState {
            unavailable: Some(format!(
              "Playback is unavailable: {error}. Install MPV and try again."
            )),
            ..PlaybackState::default()
          },
        };
        self.connection = ConnectionPhase::Connected;
        self.home = LoadState::Loading;
        self.shortcuts.clear();
        self.shortcuts_error = None;
        self.ui.login_status.set_label("");
        self.ui.login_status.set_visible(false);
        if let Some(session_to_save) = session_to_save {
          self.start_persist_session(session_to_save, sender);
        } else {
          self
            .ui
            .settings_storage_status
            .set_label("The connected session could not be saved securely.");
          self.ui.settings_storage_status.set_visible(true);
        }
        self.render_authenticated(sender);
        self.show_home(sender);
        self.load_home(sender);
      }
      Err(message) => {
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
    self.requests.disconnect();
    self.cancel_inflight_quick_connect();
    self.quick_connect_phase = QuickConnectPhase::Idle;
    self.artwork.reset_session();
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
    self.season = None;
    self.invalidate_user_data_update();
    self.playback = PlaybackState::default();
    self.ui.search.set_text("");
    self.ui.search.set_sensitive(false);
    self.ui.sidebar_toggle.set_visible(false);
    // Activating the static group anchor first clears any dynamic library shortcut,
    // then deactivating it leaves the signed-out shell with no selected destination.
    self.ui.nav_home.set_active(true);
    self.ui.nav_home.set_active(false);
    self.ui.disconnect_button.set_sensitive(false);
    self.ui.settings_disconnect_button.set_sensitive(false);
    self.ui.connection_status.set_label("Not connected");
    self.ui.quick_connect_code.set_label("");
    self.ui.quick_connect_code.set_visible(false);
    self.ui.quick_connect_status.set_label("");
    self.ui.quick_connect_spinner.stop();
    self.ui.quick_connect_spinner.set_visible(false);
    self.render_quick_connect_controls();
    if self.ui.authenticated.parent().is_some() {
      self.ui.root.remove(&self.ui.authenticated);
    }
    if self.ui.login.parent().is_none() {
      self.ui.root.append(&self.ui.login);
    }
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
    if let Some(controller) = self.playback.controller.take() {
      self.shutdown_playback(controller, PlaybackShutdownDisposition::Quit, sender);
    } else if quit_can_finish_without_controller(self.playback.busy, self.playback_cleanup_pending)
    {
      relm4::main_application().quit();
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
    if self.ui.login.parent().is_some() {
      self.ui.root.remove(&self.ui.login);
    }
    if self.ui.authenticated.parent().is_none() {
      self.ui.root.append(&self.ui.authenticated);
    }
    self.ui.search.set_sensitive(true);
    self.ui.sidebar_toggle.set_visible(true);
    self
      .ui
      .disconnect_button
      .set_sensitive(!self.profile_operation_busy);
    self
      .ui
      .settings_disconnect_button
      .set_sensitive(!self.profile_operation_busy);
    self
      .ui
      .connection_status
      .set_label(&connection_label(&self.client));
    self.render_shortcuts(sender);
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
    match page {
      "home" => self.ui.nav_home.set_active(true),
      "now-playing" => self.ui.nav_now_playing.set_active(true),
      "settings" => self.ui.nav_settings.set_active(true),
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
    self.begin_artwork_view();
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
          .or(latest_movies.first())
          .or(next_up.first())
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
      }
    }
  }

  fn open_library(&mut self, shortcut: VideoLibraryShortcut, sender: &ComponentSender<Self>) {
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
    let token = self.requests.begin_detail();
    self.detail = LoadState::Loading;
    self.show_page("detail");
    self.render_detail(sender);
    let client = Arc::clone(&self.client);
    sender.oneshot_command(async move {
      let result = if item.item_type.eq_ignore_ascii_case("series") {
        client
          .library()
          .show_detail(item.id)
          .await
          .map(DetailContent::Show)
          .map_err(|error| error.to_string())
      } else {
        client
          .library()
          .item_detail(item.id)
          .await
          .map(DetailContent::Item)
          .map_err(|error| error.to_string())
      };
      AppCommand::Detail {
        token,
        result: Box::new(result),
      }
    });
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
    self.begin_artwork_view();
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
    self.begin_artwork_view();
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
    if self.playback.busy {
      if request_kind != PlaybackRequestKind::Refresh {
        self.queue_playback_request(request);
      }
      return;
    }
    let Some(mut controller) = self.playback.controller.take() else {
      if request_kind == PlaybackRequestKind::Refresh {
        return;
      }
      self.playback.error = self
        .playback
        .unavailable
        .clone()
        .or_else(|| Some("Playback controller is unavailable.".to_owned()));
      self.show_page("now-playing");
      self.render_now_playing();
      return;
    };
    self.playback.busy = true;
    self.playback.sequence = self.playback.sequence.saturating_add(1);
    let session = self.requests.session_generation();
    let sequence = self.playback.sequence;
    let mut cancellation = self.playback_cancellation.subscribe();
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
              .map(|outcome| PlaybackCommandSuccess {
                snapshot: Some(outcome.snapshot),
                warnings: outcome.warnings,
                notice: None,
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
              .map(|outcome| PlaybackCommandSuccess {
                snapshot: Some(outcome.snapshot),
                warnings: outcome.warnings,
                notice: None,
              })
              .map_err(playback_start_failure),
            PlaybackRequest::Paused(paused) => controller
              .set_paused(paused)
              .await
              .map(|outcome| PlaybackCommandSuccess {
                snapshot: Some(outcome.snapshot),
                warnings: outcome.warnings,
                notice: None,
              })
              .map_err(|error| playback_failure("Could not update playback", error)),
            PlaybackRequest::Seek(position) => controller
              .seek(position)
              .await
              .map(|outcome| PlaybackCommandSuccess {
                snapshot: Some(outcome.snapshot),
                warnings: outcome.warnings,
                notice: None,
              })
              .map_err(|error| playback_failure("Could not seek", error)),
            PlaybackRequest::Volume(volume) => controller
              .set_volume(volume)
              .await
              .map(|outcome| PlaybackCommandSuccess {
                snapshot: Some(outcome.snapshot),
                warnings: outcome.warnings,
                notice: None,
              })
              .map_err(|error| playback_failure("Could not set volume", error)),
            PlaybackRequest::Muted(muted) => controller
              .set_muted(muted)
              .await
              .map(|outcome| PlaybackCommandSuccess {
                snapshot: Some(outcome.snapshot),
                warnings: outcome.warnings,
                notice: None,
              })
              .map_err(|error| playback_failure("Could not update mute", error)),
            PlaybackRequest::Stop => controller
              .stop()
              .await
              .map(|outcome| PlaybackCommandSuccess {
                snapshot: None,
                warnings: outcome.warnings,
                notice: Some("Playback stopped.".to_owned()),
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
              Ok(PlaybackCommandSuccess {
                snapshot,
                warnings: outcome.warnings,
                notice,
              })
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

  fn begin_artwork_view(&mut self) {
    self.artwork.cancel_pending();
    self.artwork_view = self.artwork_view.saturating_add(1);
    self.artwork_targets.clear();
  }

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
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("flat");
    let button = gtk::Button::new();
    button.set_has_frame(false);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let artwork_overlay = gtk::Overlay::new();
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_keep_aspect_ratio(true);
    picture.set_size_request(164, 220);
    let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    fallback.set_pixel_size(48);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    artwork_overlay.set_child(Some(&picture));
    artwork_overlay.add_overlay(&fallback);
    if let Some(image_id) = item.artwork_image_id.as_deref() {
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
    if item
      .played_percentage
      .is_some_and(|value| value > 0.0 && value < 100.0)
    {
      let progress = gtk::ProgressBar::new();
      progress.set_fraction(item.played_percentage.unwrap_or_default() / 100.0);
      progress.set_show_text(false);
      progress.set_valign(gtk::Align::End);
      progress.set_hexpand(true);
      progress.add_css_class("osd");
      artwork_overlay.add_overlay(&progress);
    }
    column.append(&artwork_overlay);
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
    let text = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(2)
      .margin_start(4)
      .margin_end(4)
      .margin_bottom(4)
      .build();
    let title = gtk::Label::new(Some(&item.name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(20);
    text.append(&title);
    let details = dim_label(&item_metadata(item));
    details.set_ellipsize(gtk::pango::EllipsizeMode::End);
    details.set_max_width_chars(20);
    text.append(&details);
    card.append(&text);
    if matches!(item.item_type.as_str(), "Movie" | "Episode") {
      let has_resume = item.resume_position_seconds.unwrap_or_default() > 0.0;
      let action = gtk::Button::with_label(if has_resume { "▶ Resume" } else { "▶ Play" });
      action.add_css_class("flat");
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
      card.append(&action);
    }
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
    let artwork_overlay = gtk::Overlay::new();
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_keep_aspect_ratio(true);
    picture.set_size_request(96, 72);
    let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    fallback.set_pixel_size(32);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    artwork_overlay.set_child(Some(&picture));
    artwork_overlay.add_overlay(&fallback);
    if let Some(image_id) = item.artwork_image_id.as_deref() {
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
    row.append(&artwork_overlay);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(&item.name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(64);
    text.append(&title);
    let details = dim_label(&item_metadata(item));
    details.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&details);
    if item
      .played_percentage
      .is_some_and(|value| value > 0.0 && value < 100.0)
    {
      let progress = gtk::ProgressBar::new();
      progress.set_fraction(item.played_percentage.unwrap_or_default() / 100.0);
      progress.set_show_text(false);
      text.append(&progress);
    }
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
    let backdrop = gtk::Picture::new();
    backdrop.set_can_shrink(true);
    backdrop.set_keep_aspect_ratio(true);
    backdrop.set_size_request(-1, 280);
    let fallback = gtk::Image::from_icon_name("image-missing-symbolic");
    fallback.set_pixel_size(64);
    fallback.set_halign(gtk::Align::Center);
    fallback.set_valign(gtk::Align::Center);
    let backdrop_overlay = gtk::Overlay::new();
    backdrop_overlay.set_child(Some(&backdrop));
    backdrop_overlay.add_overlay(&fallback);
    container.set_child(Some(&backdrop_overlay));
    if let Some(image_id) = item.artwork_image_id.as_deref() {
      self.artwork_slot = self.artwork_slot.saturating_add(1);
      let slot = self.artwork_slot;
      self.artwork_targets.insert(
        slot,
        ArtworkTarget {
          picture: backdrop,
          fallback,
        },
      );
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
    let gradient = gtk::Box::new(gtk::Orientation::Vertical, 0);
    gradient.add_css_class("osd");
    gradient.set_hexpand(true);
    gradient.set_vexpand(true);
    gradient.set_valign(gtk::Align::End);
    let hero_text = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(8)
      .margin_top(24)
      .margin_bottom(24)
      .margin_start(28)
      .margin_end(28)
      .build();
    let title = gtk::Label::new(Some(&item.name));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(60);
    hero_text.append(&title);
    let metadata = dim_label(&item_metadata(item));
    metadata.set_xalign(0.0);
    metadata.set_ellipsize(gtk::pango::EllipsizeMode::End);
    hero_text.append(&metadata);
    if let Some(overview) = &item.overview {
      let synopsis = gtk::Label::new(Some(overview));
      synopsis.set_xalign(0.0);
      synopsis.set_ellipsize(gtk::pango::EllipsizeMode::End);
      synopsis.set_max_width_chars(80);
      synopsis.set_lines(3);
      synopsis.set_wrap(false);
      synopsis.add_css_class("dim-label");
      hero_text.append(&synopsis);
    }
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let has_resume = item.resume_position_seconds.unwrap_or_default() > 0.0;
    let primary_label = if has_resume { "Resume" } else { "Play" };
    let primary = gtk::Button::with_label(primary_label);
    primary.add_css_class("suggested-action");
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
    let detail_item = item.clone();
    let detail_sender = sender.clone();
    details
      .connect_clicked(move |_| detail_sender.input(AppMessage::SelectItem(detail_item.clone())));
    actions.append(&details);
    if item
      .played_percentage
      .is_some_and(|value| value > 0.0 && value < 100.0)
    {
      let progress = gtk::ProgressBar::new();
      progress.set_fraction(item.played_percentage.unwrap_or_default() / 100.0);
      progress.set_show_text(false);
      progress.set_hexpand(true);
      progress.set_valign(gtk::Align::Center);
      hero_text.append(&progress);
    }
    hero_text.append(&actions);
    gradient.append(&hero_text);
    container.add_overlay(&gradient);
    container.upcast()
  }

  fn media_shelf(
    &mut self,
    title: &str,
    items: &[VideoLibraryItem],
    sender: &ComponentSender<Self>,
  ) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 12);
    section.set_margin_top(8);
    section.set_margin_bottom(8);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("title-3");
    title_label.set_xalign(0.0);
    section.append(&title_label);
    if items.is_empty() {
      section.append(&dim_label("Nothing available."));
      return section.upcast();
    }
    let flow = gtk::FlowBox::builder()
      .selection_mode(gtk::SelectionMode::None)
      .max_children_per_line(6)
      .min_children_per_line(1)
      .row_spacing(12)
      .column_spacing(12)
      .homogeneous(true)
      .build();
    for item in items {
      let child = gtk::FlowBoxChild::new();
      child.set_child(Some(&self.media_button(item, true, sender)));
      flow.insert(&child, -1);
    }
    section.append(&flow);
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
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let name = gtk::Label::new(Some(&season.name));
        name.set_hexpand(true);
        name.set_xalign(0.0);
        row.append(&name);
        let list_row = gtk::ListBoxRow::new();
        list_row.set_child(Some(&row));
        list_row.set_tooltip_text(Some(&format!("Browse episodes in {}", season.name)));
        seasons.append(&list_row);
        if selected_season_id == Some(season.id.as_str()) {
          seasons.select_row(Some(&list_row));
        }
      }
      body.append(&seasons);
    }
    if let Some(selection) = self.season.clone() {
      let section = self.season_episodes_view(&selection, sender);
      body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
      body.append(&section);
    }
    column.append(&body);
    column.upcast()
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

  fn render_now_playing(&self) {
    let snapshot = self.playback.snapshot.as_ref();
    let now_playing = snapshot.and_then(|snapshot| snapshot.now_playing.as_ref());
    let label = match (snapshot, now_playing) {
      (Some(snapshot), Some(item)) => format!("{} · {}", item.title, playback_position(snapshot)),
      _ => "No active native playback session.".to_owned(),
    };
    self.ui.now_playing_status.set_label(&label);
    let notice = self
      .playback
      .error
      .as_deref()
      .or(self.playback.notice.as_deref())
      .or(self.playback.unavailable.as_deref());
    self.ui.now_playing_notice.set_label(notice.unwrap_or(""));
    self.ui.now_playing_notice.set_visible(notice.is_some());
    let active = now_playing.is_some() && !self.playback.busy;
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
          "Resume"
        } else {
          "Pause"
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
  }
}

impl Ui {
  fn new(sender: &ComponentSender<AppModel>) -> Self {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let provider = gtk::DropDown::from_strings(&["Jellyfin", "Emby"]);
    provider.set_selected(0);
    let server_url = form_entry("https://media.example.com", gtk::InputPurpose::Url);
    let username = form_entry("Username", gtk::InputPurpose::Name);
    let password = gtk::PasswordEntry::new();
    password.set_placeholder_text(Some("Password"));
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
    password.connect_activate({
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
    root.append(&login);

    let sidebar_revealer = gtk::Revealer::builder()
      .transition_type(gtk::RevealerTransitionType::SlideRight)
      .reveal_child(true)
      .build();
    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(true);
    let sidebar_toggle = gtk::ToggleButton::new();
    sidebar_toggle.set_child(Some(&gtk::Image::from_icon_name("sidebar-show-symbolic")));
    sidebar_toggle.set_active(true);
    sidebar_toggle.set_visible(false);
    sidebar_toggle.set_tooltip_text(Some("Show or hide navigation"));
    sidebar_toggle.update_property(&[gtk::accessible::Property::Label("Show or hide navigation")]);
    sidebar_toggle.connect_toggled({
      let sidebar_revealer = sidebar_revealer.clone();
      move |button| sidebar_revealer.set_reveal_child(button.is_active())
    });
    header.pack_start(&sidebar_toggle);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search your media"));
    search.set_hexpand(true);
    search.set_width_chars(12);
    search.set_sensitive(false);
    search.update_property(&[gtk::accessible::Property::Label("Search your media")]);
    search.connect_activate({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::SearchRequested)
    });
    header.set_title_widget(Some(&search));
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

    let authenticated = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    authenticated.append(&body);
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
    let nav_now_playing = navigation_button("Now Playing", "media-playback-start-symbolic");
    nav_now_playing.set_group(Some(&nav_home));
    nav_now_playing.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::ShowNowPlaying)
    });
    sidebar.append(&nav_now_playing);
    let nav_settings = navigation_button("Settings", "emblem-system-symbolic");
    nav_settings.set_group(Some(&nav_home));
    nav_settings.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::ShowSettings)
    });
    sidebar.append(&nav_settings);
    let sidebar_panel = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sidebar_panel.append(&sidebar);
    sidebar_panel.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    sidebar_revealer.set_child(Some(&sidebar_panel));
    body.append(&sidebar_revealer);

    let content = gtk::Stack::new();
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_transition_type(gtk::StackTransitionType::Crossfade);
    body.append(&content);
    let home_content = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(18)
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
    let list_button = gtk::ToggleButton::new();
    list_button.set_child(Some(&gtk::Image::from_icon_name("view-list-symbolic")));
    list_button.set_tooltip_text(Some("List view"));
    list_button.update_property(&[gtk::accessible::Property::Label("List view")]);
    list_button.set_group(Some(&grid_button));
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
    let toolbar = gtk::Box::new(gtk::Orientation::Vertical, 6);
    toolbar.append(&browse_title);
    let browse_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    browse_actions.append(&browse_status);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    browse_actions.append(&spacer);
    browse_actions.append(&grid_button);
    browse_actions.append(&list_button);
    toolbar.append(&browse_actions);
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
    toolbar.append(&browse_filter_bar);
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
    let pagination = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    pagination.append(&load_previous_button);
    pagination.append(&load_next_button);
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

    let now_playing_status = dim_label("No active native playback session.");
    now_playing_status.set_wrap(true);
    let now_playing_notice = dim_label("");
    now_playing_notice.set_wrap(true);
    now_playing_notice.set_visible(false);
    now_playing_notice.set_accessible_role(gtk::AccessibleRole::Status);
    let position_label = playback_time_label();
    let duration_label = playback_time_label();
    let pause_button = gtk::Button::from_icon_name("media-playback-start-symbolic");
    pause_button.set_tooltip_text(Some("Pause or resume playback"));
    pause_button.update_property(&[gtk::accessible::Property::Label("Pause or resume playback")]);
    pause_button.set_sensitive(false);
    pause_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::TogglePaused)
    });
    let stop_button = gtk::Button::from_icon_name("media-playback-stop-symbolic");
    stop_button.set_tooltip_text(Some("Stop playback"));
    stop_button.update_property(&[gtk::accessible::Property::Label("Stop playback")]);
    stop_button.set_sensitive(false);
    stop_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.input(AppMessage::StopPlayback)
    });
    let playback_controls_syncing = Rc::new(Cell::new(false));
    let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    seek.set_hexpand(true);
    seek.set_draw_value(false);
    seek.set_sensitive(false);
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
    volume.set_hexpand(true);
    volume.set_draw_value(false);
    volume.set_sensitive(false);
    volume.update_property(&[gtk::accessible::Property::Label("Volume")]);
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
    mute_button.set_child(Some(&gtk::Image::from_icon_name(
      "audio-volume-muted-symbolic",
    )));
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
    let now_playing = now_playing_page(NowPlayingPageWidgets {
      status: &now_playing_status,
      notice: &now_playing_notice,
      position_label: &position_label,
      duration_label: &duration_label,
      pause: &pause_button,
      stop: &stop_button,
      seek: &seek,
      volume: &volume,
      mute: &mute_button,
    });
    content.add_named(&now_playing, Some("now-playing"));
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
    let settings = settings_page(
      &settings_saved_profile,
      &settings_storage_status,
      &settings_disconnect_button,
      &forget_current_profile,
    );
    content.add_named(&settings, Some("settings"));

    Self {
      root,
      login,
      provider,
      server_url,
      username,
      password,
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
      header,
      sidebar_toggle,
      connection_status,
      search,
      disconnect_button,
      content,
      nav_home,
      nav_now_playing,
      nav_settings,
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
      now_playing_status,
      now_playing_notice,
      position_label,
      duration_label,
      pause_button,
      stop_button,
      seek,
      volume,
      mute_button,
      playback_controls_syncing,
      settings_saved_profile,
      settings_storage_status,
      settings_disconnect_button,
      forget_current_profile,
    }
  }
}

struct LoginPageWidgets<'a> {
  provider: &'a gtk::DropDown,
  server_url: &'a gtk::Entry,
  username: &'a gtk::Entry,
  password: &'a gtk::PasswordEntry,
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
  let page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .valign(gtk::Align::Center)
    .halign(gtk::Align::Center)
    .spacing(0)
    .build();
  let card = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(18)
    .margin_top(32)
    .margin_bottom(32)
    .margin_start(36)
    .margin_end(36)
    .build();
  card.add_css_class("card");
  let header = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(8)
    .build();
  let icon = gtk::Image::from_icon_name("video-x-generic-symbolic");
  icon.set_pixel_size(48);
  icon.set_halign(gtk::Align::Center);
  header.append(&icon);
  let title = gtk::Label::new(Some("JellyPilot"));
  title.add_css_class("title-1");
  title.set_halign(gtk::Align::Center);
  header.append(&title);
  let copy = dim_label("Connect to your Jellyfin or Emby server.");
  copy.set_wrap(true);
  copy.set_halign(gtk::Align::Center);
  copy.set_justify(gtk::Justification::Center);
  header.append(&copy);
  card.append(&header);
  card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
  let saved_heading = gtk::Label::new(Some("Saved sign-ins"));
  saved_heading.add_css_class("heading");
  saved_heading.set_xalign(0.0);
  card.append(&saved_heading);
  let saved_profiles_scroll = gtk::ScrolledWindow::builder()
    .child(saved_profiles)
    .max_content_height(240)
    .propagate_natural_height(true)
    .hscrollbar_policy(gtk::PolicyType::Never)
    .build();
  card.append(&saved_profiles_scroll);
  card.append(saved_profiles_status);
  card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
  let server_form = gtk::Grid::builder()
    .row_spacing(10)
    .column_spacing(12)
    .build();
  add_form_row(&server_form, 0, "Server type", provider);
  add_form_row(&server_form, 1, "Server URL", server_url);
  card.append(&server_form);
  card.append(method_switcher);

  let quick_connect_page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .margin_top(6)
    .build();
  let quick_connect_copy = dim_label(
    "Request a code, then approve it from another client already signed in to this Jellyfin server.",
  );
  quick_connect_copy.set_wrap(true);
  quick_connect_copy.set_justify(gtk::Justification::Center);
  quick_connect_page.append(&quick_connect_copy);
  quick_connect_code.set_halign(gtk::Align::Center);
  quick_connect_page.append(quick_connect_code);
  let quick_connect_progress = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(8)
    .halign(gtk::Align::Center)
    .hexpand(true)
    .build();
  quick_connect_spinner.set_halign(gtk::Align::Center);
  quick_connect_status.set_hexpand(true);
  quick_connect_status.set_halign(gtk::Align::Fill);
  quick_connect_progress.append(quick_connect_spinner);
  quick_connect_progress.append(quick_connect_status);
  quick_connect_page.append(&quick_connect_progress);
  quick_connect.set_hexpand(true);
  quick_connect_page.append(quick_connect);
  cancel_quick_connect.set_halign(gtk::Align::Center);
  quick_connect_page.append(cancel_quick_connect);
  method_stack.add_titled(&quick_connect_page, Some("quick-connect"), "Quick Connect");

  let password_page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .margin_top(6)
    .build();
  let password_form = gtk::Grid::builder()
    .row_spacing(10)
    .column_spacing(12)
    .build();
  add_form_row(&password_form, 0, "Username", username);
  add_form_row(&password_form, 1, "Password", password);
  password_page.append(&password_form);
  let storage_copy = dim_label(
    "Successful sign-ins are saved in Linux Secret Service. JellyPilot never stores your password.",
  );
  storage_copy.set_wrap(true);
  password_page.append(&storage_copy);
  sign_in.set_hexpand(true);
  password_page.append(sign_in);
  method_stack.add_titled(&password_page, Some("password"), "Password");
  method_stack.set_visible_child_name("quick-connect");
  card.append(method_stack);
  status.set_halign(gtk::Align::Center);
  card.append(status);
  page.append(&card);
  gtk::ScrolledWindow::builder()
    .child(&page)
    .hscrollbar_policy(gtk::PolicyType::Never)
    .vexpand(true)
    .build()
}

fn saved_profile_row(
  profile: &SavedProfileSummary,
  sender: &ComponentSender<AppModel>,
) -> gtk::ListBoxRow {
  let row = gtk::ListBoxRow::new();
  row.set_activatable(false);
  row.set_selectable(false);
  let content = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .margin_top(10)
    .margin_bottom(10)
    .margin_start(12)
    .margin_end(12)
    .build();
  let identity = gtk::Box::new(gtk::Orientation::Vertical, 3);
  identity.set_hexpand(true);
  let title = gtk::Label::new(Some(
    profile
      .server_name
      .as_deref()
      .unwrap_or(profile.server_url.as_str()),
  ));
  title.add_css_class("heading");
  title.set_xalign(0.0);
  title.set_ellipsize(gtk::pango::EllipsizeMode::End);
  let provider = match profile.provider {
    MediaServerProvider::Jellyfin => "Jellyfin",
    MediaServerProvider::Emby => "Emby",
  };
  let account = dim_label(&format!("{provider} · {}", profile.user_name));
  account.set_ellipsize(gtk::pango::EllipsizeMode::End);
  let server = dim_label(&profile.server_url);
  server.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
  identity.append(&title);
  identity.append(&account);
  identity.append(&server);

  let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
  actions.set_valign(gtk::Align::Center);
  actions.set_halign(gtk::Align::End);
  let continue_button = gtk::Button::with_label("Continue");
  continue_button.add_css_class("suggested-action");
  continue_button.update_property(&[gtk::accessible::Property::Label(&format!(
    "Continue as {} on {}",
    profile.user_name,
    profile
      .server_name
      .as_deref()
      .unwrap_or(profile.server_url.as_str())
  ))]);
  continue_button.connect_clicked({
    let sender = sender.clone();
    let key = profile.key.clone();
    move |_| sender.input(AppMessage::RestoreSavedProfile(key.clone()))
  });
  let forget_button = gtk::Button::with_label("Forget");
  forget_button.add_css_class("destructive-action");
  forget_button.update_property(&[gtk::accessible::Property::Label(&format!(
    "Forget saved sign-in for {} on {}",
    profile.user_name,
    profile
      .server_name
      .as_deref()
      .unwrap_or(profile.server_url.as_str())
  ))]);
  forget_button.connect_clicked({
    let sender = sender.clone();
    let key = profile.key.clone();
    move |_| sender.input(AppMessage::ForgetSavedProfile(key.clone()))
  });
  actions.append(&continue_button);
  actions.append(&forget_button);
  content.append(&identity);
  content.append(&actions);
  row.set_child(Some(&content));
  row
}

struct NowPlayingPageWidgets<'a> {
  status: &'a gtk::Label,
  notice: &'a gtk::Label,
  position_label: &'a gtk::Label,
  duration_label: &'a gtk::Label,
  pause: &'a gtk::Button,
  stop: &'a gtk::Button,
  seek: &'a gtk::Scale,
  volume: &'a gtk::Scale,
  mute: &'a gtk::ToggleButton,
}

fn now_playing_page(widgets: NowPlayingPageWidgets<'_>) -> gtk::Widget {
  let NowPlayingPageWidgets {
    status,
    notice,
    position_label,
    duration_label,
    pause,
    stop,
    seek,
    volume,
    mute,
  } = widgets;
  let page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(20)
    .margin_top(24)
    .margin_bottom(24)
    .margin_start(24)
    .margin_end(24)
    .build();
  let title = gtk::Label::new(Some("Now Playing"));
  title.add_css_class("title-1");
  title.set_xalign(0.0);
  let panel = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(16)
    .build();
  panel.add_css_class("card");
  panel.set_margin_top(8);
  panel.set_margin_bottom(8);
  panel.set_margin_start(8);
  panel.set_margin_end(8);
  let inner = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(16)
    .margin_top(20)
    .margin_bottom(20)
    .margin_start(24)
    .margin_end(24)
    .build();
  status.set_xalign(0.0);
  status.set_wrap(true);
  inner.append(status);
  notice.set_xalign(0.0);
  notice.set_wrap(true);
  inner.append(notice);
  let timeline = gtk::Box::new(gtk::Orientation::Horizontal, 12);
  timeline.append(position_label);
  seek.set_hexpand(true);
  seek.set_draw_value(false);
  timeline.append(seek);
  timeline.append(duration_label);
  inner.append(&timeline);
  let transport = gtk::Box::new(gtk::Orientation::Horizontal, 12);
  transport.set_halign(gtk::Align::Center);
  pause.add_css_class("suggested-action");
  stop.add_css_class("destructive-action");
  transport.append(pause);
  transport.append(stop);
  inner.append(&transport);
  let audio_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
  audio_row.set_halign(gtk::Align::Center);
  let volume_icon = gtk::Image::from_icon_name("audio-volume-medium-symbolic");
  audio_row.append(&volume_icon);
  volume.set_size_request(200, -1);
  volume.set_draw_value(false);
  audio_row.append(volume);
  audio_row.append(mute);
  inner.append(&audio_row);
  panel.append(&inner);
  page.append(&title);
  page.append(&panel);
  page.upcast()
}

fn settings_page(
  saved_profile: &gtk::Label,
  storage_status: &gtk::Label,
  disconnect: &gtk::Button,
  forget_saved_profile: &gtk::Button,
) -> gtk::Widget {
  let page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(20)
    .margin_top(24)
    .margin_bottom(24)
    .margin_start(24)
    .margin_end(24)
    .build();
  let title = gtk::Label::new(Some("Settings"));
  title.add_css_class("title-1");
  title.set_xalign(0.0);
  let session_group = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .build();
  session_group.add_css_class("card");
  let group_inner = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .margin_top(16)
    .margin_bottom(16)
    .margin_start(20)
    .margin_end(20)
    .build();
  let session_heading = gtk::Label::new(Some("Session"));
  session_heading.add_css_class("heading");
  session_heading.set_xalign(0.0);
  group_inner.append(&session_heading);
  let copy = dim_label(
    "Disconnect this session here or from the header bar. Saved sign-ins remain available until you forget them.",
  );
  copy.set_wrap(true);
  group_inner.append(&copy);
  group_inner.append(disconnect);
  session_group.append(&group_inner);
  page.append(&title);
  page.append(&session_group);
  let saved_group = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .build();
  saved_group.add_css_class("card");
  let saved_inner = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .margin_top(16)
    .margin_bottom(16)
    .margin_start(20)
    .margin_end(20)
    .build();
  let saved_heading = gtk::Label::new(Some("Saved sign-in"));
  saved_heading.add_css_class("heading");
  saved_heading.set_xalign(0.0);
  saved_inner.append(&saved_heading);
  saved_inner.append(saved_profile);
  saved_inner.append(storage_status);
  forget_saved_profile.set_halign(gtk::Align::Start);
  saved_inner.append(forget_saved_profile);
  saved_group.append(&saved_inner);
  page.append(&saved_group);
  let migration_group = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(12)
    .build();
  migration_group.add_css_class("card");
  let migration_inner = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(8)
    .margin_top(16)
    .margin_bottom(16)
    .margin_start(20)
    .margin_end(20)
    .build();
  let migration_heading = gtk::Label::new(Some("Migration Status"));
  migration_heading.add_css_class("heading");
  migration_heading.set_xalign(0.0);
  migration_inner.append(&migration_heading);
  let features = [
    ("Password sign-in", true),
    ("Video Home and library browsing", true),
    ("Search", true),
    ("Item details and seasons", true),
    ("External MPV playback", true),
    ("Quick Connect", true),
    ("Saved profiles", true),
    ("Embedded web playback", false),
  ];
  for (feature, available) in features {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = gtk::Image::from_icon_name(if available {
      "emblem-ok-symbolic"
    } else {
      "action-unavailable-symbolic"
    });
    icon.set_pixel_size(16);
    if !available {
      icon.add_css_class("dim-label");
    }
    row.append(&icon);
    let label = gtk::Label::new(Some(feature));
    label.set_xalign(0.0);
    if !available {
      label.add_css_class("dim-label");
    }
    row.append(&label);
    migration_inner.append(&row);
  }
  migration_group.append(&migration_inner);
  page.append(&migration_group);
  gtk::ScrolledWindow::builder()
    .child(&page)
    .vexpand(true)
    .build()
    .upcast()
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
  let scroll = gtk::ScrolledWindow::builder()
    .child(&page)
    .vexpand(true)
    .build();
  scroll.upcast()
}

fn state_view(title: &str, copy: &str, icon_name: &str) -> gtk::Widget {
  let state = gtk::Box::builder()
    .orientation(gtk::Orientation::Horizontal)
    .spacing(12)
    .build();
  state.set_accessible_role(gtk::AccessibleRole::Status);
  let icon = gtk::Image::from_icon_name(icon_name);
  icon.set_pixel_size(24);
  let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
  let title = gtk::Label::new(Some(title));
  title.add_css_class("heading");
  title.set_xalign(0.0);
  let copy = dim_label(copy);
  copy.set_xalign(0.0);
  copy.set_wrap(true);
  text.append(&title);
  text.append(&copy);
  state.append(&icon);
  state.append(&text);
  state.upcast()
}

fn loading_view(copy: &str) -> gtk::Widget {
  let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  let spinner = gtk::Spinner::new();
  spinner.start();
  let label = dim_label(copy);
  row.set_accessible_role(gtk::AccessibleRole::Status);
  row.append(&spinner);
  row.append(&label);
  row.upcast()
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

fn form_entry(placeholder: &str, purpose: gtk::InputPurpose) -> gtk::Entry {
  let entry = gtk::Entry::new();
  entry.set_placeholder_text(Some(placeholder));
  entry.set_input_purpose(purpose);
  entry.set_hexpand(true);
  entry.set_width_chars(1);
  entry
}

fn add_form_row<W: IsA<gtk::Widget>>(grid: &gtk::Grid, row: i32, label: &str, widget: &W) {
  let label = gtk::Label::with_mnemonic(&format!("_{label}"));
  label.set_xalign(1.0);
  label.set_mnemonic_widget(Some(widget));
  grid.attach(&label, 0, row, 1, 1);
  grid.attach(widget, 1, row, 1, 1);
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

fn item_metadata(item: &VideoLibraryItem) -> String {
  let mut details = Vec::new();
  if let Some(year) = item.production_year {
    details.push(year.to_string());
  }
  details.push(item.item_type.clone());
  if item.played {
    details.push("Played".to_owned());
  }
  details.join(" · ")
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

fn playback_position(snapshot: &PlaybackSnapshot) -> String {
  let transport = &snapshot.transport;
  format!(
    "{} / {}",
    format_duration(transport.time_pos),
    format_duration(transport.duration)
  )
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
}
