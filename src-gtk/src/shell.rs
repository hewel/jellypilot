use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use jellypilot_media_server::{
  Credentials, JellyfinClient, MediaServerProvider, VideoHome, VideoItemDetail, VideoLibraryItem,
  VideoLibraryKind, VideoLibraryPageRequest, VideoLibraryPlayedFilter, VideoLibraryShortcut,
  VideoLibrarySort, VideoLibrarySortDirection, VideoSearchRequest, VideoSeason,
  VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest, VideoShowDetail,
};
use relm4::gtk::prelude::*;
use relm4::{gtk, Component, ComponentParts, ComponentSender, RelmApp};

use crate::artwork::{ArtworkAdapter, DecodedArtwork, FALLBACK_ARTWORK_ICON};
use crate::browse_model::{
  BrowseEffect, BrowseModel, BrowsePagePayload, BrowsePageRequest, BrowsePageSettlement,
  BrowseSource,
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

/// Native GTK stylesheet for JellyPilot.
const APP_CSS: &str = r#"
.jp-card {
  background: alpha(@theme_fg_color, 0.0);
  border-radius: 12px;
  transition: background 180ms ease;
}
.jp-card:hover {
  background: alpha(@theme_fg_color, 0.06);
}
.jp-card-artwork {
  border-radius: 10px;

}
.jp-hero {
  border-radius: 16px;

  margin-bottom: 12px;
}
.jp-hero-overlay {
  background: linear-gradient(
    to top,
    alpha(@theme_bg_color, 0.95) 0%,
    alpha(@theme_bg_color, 0.6) 40%,
    transparent 100%
  );
}
.jp-shelf-title {
  font-weight: 700;
  font-size: 1.15em;
}
.jp-detail-backdrop {
  border-radius: 14px;

}
.jp-detail-overlay {
  background: linear-gradient(
    to top,
    @theme_bg_color 0%,
    alpha(@theme_bg_color, 0.85) 35%,
    transparent 100%
  );
}
.jp-now-playing {
  border-radius: 16px;

}
.jp-action-button {
  padding: 8px 20px;
  font-weight: 600;
}
.jp-login-card {
  background: alpha(@theme_fg_color, 0.03);
  border-radius: 16px;
  border: 1px solid alpha(@theme_fg_color, 0.08);
  padding: 32px;
}
.jp-sidebar-button {
  border-radius: 8px;
  padding: 6px 10px;
}
.jp-progress-bar progress {
  min-height: 4px;
  border-radius: 2px;
}
"#;

fn load_app_css() {
  let provider = gtk::CssProvider::new();
  provider.load_from_data(APP_CSS);
  gtk::style_context_add_provider_for_display(
    &gtk::gdk::Display::default().expect("a display is required to load the application CSS"),
    &provider,
    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION as u32,
  );
}

struct AppModel {
  client: Arc<JellyfinClient>,
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
  playback: PlaybackState,
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
  LoginRequested,
  Disconnect,
  ShowHome,
  ShowNowPlaying,
  ShowSettings,
  OpenLibrary(VideoLibraryShortcut),
  SearchRequested,
  SelectItem(VideoLibraryItem),
  SetBrowsePresentation(BrowsePresentation),
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
  Login {
    session: SessionToken,
    client: Arc<JellyfinClient>,
    result: Result<(), String>,
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
      Self::Login {
        session, result, ..
      } => formatter
        .debug_struct("Login")
        .field("session", session)
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
  login: gtk::Box,
  provider: gtk::DropDown,
  server_url: gtk::Entry,
  username: gtk::Entry,
  password: gtk::PasswordEntry,
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
  grid_button: gtk::ToggleButton,
  list_button: gtk::ToggleButton,
  load_previous_button: gtk::Button,
  load_next_button: gtk::Button,
  browse_scroll: gtk::ScrolledWindow,
  detail_content: gtk::Box,
  now_playing_status: gtk::Label,
  now_playing_notice: gtk::Label,
  pause_button: gtk::Button,
  stop_button: gtk::Button,
  seek: gtk::Scale,
  volume: gtk::Scale,
  mute_button: gtk::ToggleButton,
  playback_controls_syncing: Rc<Cell<bool>>,
}

#[relm4::component]
impl Component for AppModel {
  type Init = ();
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
    _init: Self::Init,
    root: Self::Root,
    sender: ComponentSender<Self>,
  ) -> ComponentParts<Self> {
    let ui = Ui::new(&sender);
    root.set_titlebar(Some(&ui.header));
    root.set_child(Some(&ui.root));
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
    let model = Self {
      client: Arc::new(JellyfinClient::new()),
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
      playback: PlaybackState::default(),
      playback_refresh_source: Some(playback_refresh_source),
      playback_cleanup_pending: false,
      quitting: false,
      ui,
    };
    let widgets = view_output!();

    ComponentParts { model, widgets }
  }

  fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
    match message {
      AppMessage::LoginRequested => self.start_login(&sender),
      AppMessage::Disconnect => self.disconnect(&sender),
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
      AppCommand::Login {
        session,
        client,
        result,
      } => self.finish_login(session, client, result, &sender),
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
  fn start_login(&mut self, sender: &ComponentSender<Self>) {
    if !can_start_login(self.connection, self.playback_cleanup_pending) {
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

    self.ui.password.set_text("");
    let session = self.requests.begin_login();
    self.browse.model.reset();
    self.connection = ConnectionPhase::Connecting;
    self.home = LoadState::Loading;
    self.ui.login_button.set_sensitive(false);
    self
      .ui
      .login_status
      .set_label("Connecting and loading your libraries…");
    self.ui.login_status.set_visible(true);
    let credentials = Credentials {
      provider: provider_for(self.ui.provider.selected()),
      server_url,
      username,
      password,
    };
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

    self.ui.login_button.set_sensitive(true);
    match result {
      Ok(()) => {
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
        self.render_authenticated(sender);
        self.show_home(sender);
        self.load_home(sender);
      }
      Err(message) => {
        self.connection = ConnectionPhase::Failed;
        self.home = LoadState::Failed(message.clone());
        self.ui.login_status.set_label(&message);
        self.ui.login_status.set_visible(true);
      }
    }
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
    self.artwork.reset_session();
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
    self.detail = LoadState::Idle;
    self.detail_selection = None;
    self.detail_origin = None;
    self.detail_parent = None;
    self.season = None;
    self.playback = PlaybackState::default();
    self.ui.search.set_text("");
    self.ui.search.set_sensitive(false);
    self.ui.sidebar_toggle.set_visible(false);
    // Activating the static group anchor first clears any dynamic library shortcut,
    // then deactivating it leaves the signed-out shell with no selected destination.
    self.ui.nav_home.set_active(true);
    self.ui.nav_home.set_active(false);
    self.ui.disconnect_button.set_sensitive(false);
    self.ui.connection_status.set_label("Not connected");
    if self.ui.authenticated.parent().is_some() {
      self.ui.root.remove(&self.ui.authenticated);
    }
    if self.ui.login.parent().is_none() {
      self.ui.root.append(&self.ui.login);
    }
    self
      .ui
      .login_button
      .set_sensitive(!self.playback_cleanup_pending);
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
    self.artwork.reset_session();
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

  fn render_authenticated(&mut self, sender: &ComponentSender<Self>) {
    if self.ui.login.parent().is_some() {
      self.ui.root.remove(&self.ui.login);
    }
    if self.ui.authenticated.parent().is_none() {
      self.ui.root.append(&self.ui.authenticated);
    }
    self.ui.search.set_sensitive(true);
    self.ui.sidebar_toggle.set_visible(true);
    self.ui.disconnect_button.set_sensitive(true);
    self
      .ui
      .connection_status
      .set_label(&connection_label(&self.client));
    self.render_shortcuts(sender);
  }

  fn show_home(&mut self, sender: &ComponentSender<Self>) {
    self.navigate_to("home");
    self.render_home(sender);
  }

  fn navigate_to(&mut self, page: &str) {
    self.requests.navigate();
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
    let result = self.browse.model.configure(BrowseSource::Library {
      session: self.requests.session_generation(),
      shortcut,
    });
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

  fn load_detail(&mut self, item: VideoLibraryItem, sender: &ComponentSender<Self>) {
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
                sort: VideoLibrarySort::Title,
                sort_direction: VideoLibrarySortDirection::Ascending,
                played_filter: VideoLibraryPlayedFilter::All,
                favorites_only: false,
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
    sender.oneshot_command(async move {
      let result = match request {
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
    let client = Arc::clone(&self.client);
    let image_id = image_id.to_owned();
    let session = self.requests.session_generation();
    let view = self.artwork_view;
    sender.oneshot_command(async move {
      let result = artwork.load(&client, &image_id).await.map_err(|_| ());
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
      let client = Arc::clone(&self.client);
      let image_id = image_id.to_owned();
      let session = self.requests.session_generation();
      let view = self.artwork_view;
      sender.oneshot_command(async move {
        let result = artwork.load(&client, &image_id).await.map_err(|_| ());
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
      let client = Arc::clone(&self.client);
      let image_id = image_id.to_owned();
      let session = self.requests.session_generation();
      let view = self.artwork_view;
      sender.oneshot_command(async move {
        let result = artwork.load(&client, &image_id).await.map_err(|_| ());
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
      action.set_tooltip_text(Some(if has_resume { "Resume" } else { "Play" }));
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
      let client = Arc::clone(&self.client);
      let image_id = image_id.to_owned();
      let session = self.requests.session_generation();
      let view = self.artwork_view;
      sender.oneshot_command(async move {
        let result = artwork.load(&client, &image_id).await.map_err(|_| ());
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
      LoadState::Ready(page)
        if page.episodes.is_empty() && page.total_record_count == 0 && !page.has_more =>
      {
        section.append(&state_view(
          "No episodes available",
          "This season does not contain any visible episodes.",
          "folder-videos-symbolic",
        ));
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
        next.set_sensitive(page.has_more);
        let sender_clone = sender.clone();
        next.connect_clicked(move |_| sender_clone.input(AppMessage::NextSeasonEpisodePage));
        pagination.append(&previous);
        pagination.append(&page_status);
        pagination.append(&next);
        section.append(&pagination);
        if page.episodes.is_empty() {
          section.append(&dim_label(
            "No visible episodes are available on this page.",
          ));
        } else {
          section.append(&self.media_list(&page.episodes, sender));
        }
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
    let login = login_page(
      &provider,
      &server_url,
      &username,
      &password,
      &login_status,
      &login_button,
    );
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
    let now_playing = now_playing_page(
      &now_playing_status,
      &now_playing_notice,
      &pause_button,
      &stop_button,
      &seek,
      &volume,
      &mute_button,
    );
    content.add_named(&now_playing, Some("now-playing"));
    let settings = settings_page(sender);
    content.add_named(&settings, Some("settings"));

    Self {
      root,
      login,
      provider,
      server_url,
      username,
      password,
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
      grid_button,
      list_button,
      load_previous_button,
      load_next_button,
      browse_scroll,
      detail_content,
      now_playing_status,
      now_playing_notice,
      pause_button,
      stop_button,
      seek,
      volume,
      mute_button,
      playback_controls_syncing,
    }
  }
}

fn login_page(
  provider: &gtk::DropDown,
  server_url: &gtk::Entry,
  username: &gtk::Entry,
  password: &gtk::PasswordEntry,
  status: &gtk::Label,
  sign_in: &gtk::Button,
) -> gtk::Box {
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
  let copy = dim_label(
    "Connect to your Jellyfin or Emby server. Credentials are used only for this session.",
  );
  copy.set_wrap(true);
  copy.set_halign(gtk::Align::Center);
  copy.set_justify(gtk::Justification::Center);
  header.append(&copy);
  card.append(&header);
  card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
  let form = gtk::Grid::builder()
    .row_spacing(10)
    .column_spacing(12)
    .build();
  add_form_row(&form, 0, "Server type", provider);
  add_form_row(&form, 1, "Server URL", server_url);
  add_form_row(&form, 2, "Username", username);
  add_form_row(&form, 3, "Password", password);
  card.append(&form);
  status.set_halign(gtk::Align::Center);
  card.append(status);
  sign_in.set_hexpand(true);
  card.append(sign_in);
  page.append(&card);
  let footer =
    dim_label("Quick Connect and saved profiles are not yet available in the GTK preview.");
  footer.set_halign(gtk::Align::Center);
  footer.set_margin_top(12);
  footer.set_margin_bottom(24);
  page.append(&footer);
  page
}

fn now_playing_page(
  status: &gtk::Label,
  notice: &gtk::Label,
  pause: &gtk::Button,
  stop: &gtk::Button,
  seek: &gtk::Scale,
  volume: &gtk::Scale,
  mute: &gtk::ToggleButton,
) -> gtk::Widget {
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
  let position_label = gtk::Label::new(Some("00:00"));
  position_label.add_css_class("dim-label");
  position_label.add_css_class("monospace");
  timeline.append(&position_label);
  seek.set_hexpand(true);
  seek.set_draw_value(false);
  timeline.append(seek);
  let duration_label = gtk::Label::new(Some("00:00"));
  duration_label.add_css_class("dim-label");
  duration_label.add_css_class("monospace");
  timeline.append(&duration_label);
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

fn settings_page(sender: &ComponentSender<AppModel>) -> gtk::Widget {
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
  let copy = dim_label("Disconnect this session here or from the header bar. The server URL and credentials are not saved between sessions in the GTK preview.");
  copy.set_wrap(true);
  group_inner.append(&copy);
  let disconnect = gtk::Button::with_label("Disconnect");
  disconnect.add_css_class("destructive-action");
  disconnect.set_halign(gtk::Align::Start);
  disconnect.connect_clicked({
    let sender = sender.clone();
    move |_| sender.input(AppMessage::Disconnect)
  });
  group_inner.append(&disconnect);
  session_group.append(&group_inner);
  page.append(&title);
  page.append(&session_group);
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
    ("Quick Connect", false),
    ("Saved profiles", false),
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
  page.upcast()
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

const fn can_start_login(connection: ConnectionPhase, playback_cleanup_pending: bool) -> bool {
  matches!(
    connection,
    ConnectionPhase::SignedOut | ConnectionPhase::Failed
  ) && !playback_cleanup_pending
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
  load_app_css();
  if smoke_test {
    app.allow_multiple_instances(true);
    let application = relm4::main_application();
    application.connect_window_added(move |application, window| {
      let application = application.clone();
      window.connect_map(move |_| {
        let application = application.clone();
        gtk::glib::idle_add_local_once(move || application.quit());
      });
    });
  }
  if smoke_test {
    app
      .with_args(vec!["jellypilot-gtk-smoke".to_owned()])
      .run::<AppModel>(());
  } else {
    app.run::<AppModel>(());
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
