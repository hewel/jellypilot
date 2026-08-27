use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jellypilot_media_server::{
  JellyfinClient, MediaItem, PlaybackEngineKind, SavedSession, VideoItemDetail,
  VideoUserDataUpdateRequest,
};

use jellypilot_mpv::{has_mpv_option, write_input_conf, PlayerState};
use jellypilot_session::{IntroSkipMode, IntroSkipRange};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, Component, ComponentParts, ComponentSender, RelmApp};

use crate::artwork::{ArtworkAdapter, DecodedArtwork};
use crate::artwork_binder::{ArtworkBinder, ArtworkSettlement, ArtworkSlot, ArtworkSurface};
use crate::artwork_cache::ArtworkCacheStats;
use crate::auth_storage::{AuthStore, SavedProfileKey, SavedProfileSummary};

use crate::config::{self, LoginPrefill};
use crate::diagnostics::{DiagnosticCategory, DiagnosticLevel, Diagnostics};
use crate::pages::browse::{self, BrowseContext, BrowseEffect, BrowseEvent, BrowsePage};
use crate::pages::cards::{clear_box, dim_label};
use crate::pages::detail::{self, DetailContext, DetailEffect, DetailEvent, DetailPage};
use crate::pages::diagnostics::{self, DiagnosticsContext, DiagnosticsPage};
use crate::pages::home::{self, HomeContext, HomeEffect, HomeEvent, HomePage};
use crate::pages::login::{
  self, run_auth_operation, LoginContext, LoginEffect, LoginEvent, LoginPage,
};
use crate::pages::player::{self, PlayerContext, PlayerEffect, PlayerEvent, PlayerPage};
use crate::pages::settings::{
  self, ConnectionView, SettingsContext, SettingsEffect, SettingsEvent, SettingsPage,
};

use crate::playback::{
  Playable, PlaybackController, PlaybackControllerConfig, PlaybackError, PlaybackRefreshOutcome,
  PlaybackRefreshState, PlaybackSnapshot, PlaybackStartPosition, TrackInfo,
};
use crate::playback_session::{
  AdjacentDirection, ControllerCommand, ControllerSettlement, EffectId, IntroAvailability,
  PlaybackEffect, PlaybackEvent, PlaybackInput, PlaybackIntent, PlaybackSession,
};
use crate::request_gate::{
  ImageCacheToken, RemotePlayToken, RemoteToken, RequestGate, SessionToken,
};

pub(crate) use crate::pages::LoadState;

const APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview";
const SMOKE_APP_ID: &str = "io.github.hewel.JellyPilot.GtkPreview.Smoke";
struct AppModel {
  client: Arc<JellyfinClient>,
  auth_store: AuthStore,
  login: LoginPage,
  settings: SettingsPage,
  diagnostics_page: DiagnosticsPage,
  home_page: HomePage,
  browse_page: BrowsePage,
  detail_page: DetailPage,
  player: PlayerPage,
  intro_mode: config::IntroMode,
  diagnostics: Diagnostics,
  saved_profiles: LoadState<Vec<SavedProfileSummary>>,
  active_saved_profile: Option<SavedProfileKey>,
  artwork: Arc<ArtworkAdapter>,
  artwork_binder: ArtworkBinder,
  image_cache_clearing: bool,
  requests: RequestGate,
  connection: ConnectionPhase,
  remote_state: RemoteControlState,
  remote_socket: Option<Arc<jellypilot_session::JellyfinWebSocket>>,
  playback_session: PlaybackSession,
  playback_controller: Option<PlaybackController>,
  playback_reconfigure_pending: bool,
  remote_disconnect_pending: bool,
  quitting: bool,
  ui: Ui,
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

#[derive(Debug)]
pub(crate) enum AppMessage {
  Login(login::Message),
  Settings(settings::Message),
  Diagnostics(diagnostics::Message),
  Home(home::Message),
  Browse(browse::Message),
  Detail(detail::Message),
  Player(player::Message),
  Disconnect,
  ShowHome,
  SearchRequested,
  SetPaused(bool),
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
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
  HomeEvent(HomeEvent),
  BrowseEvent(BrowseEvent),
  DetailEvent(DetailEvent),
  Artwork {
    session: SessionToken,
    surface: ArtworkSurface,
    slot: ArtworkSlot,
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
      Self::HomeEvent(_) => formatter.debug_tuple("HomeEvent").finish(),
      Self::BrowseEvent(_) => formatter.debug_tuple("BrowseEvent").finish(),
      Self::DetailEvent(_) => formatter.debug_tuple("DetailEvent").finish(),
      Self::Artwork {
        session,
        surface,
        slot,
        result,
      } => formatter
        .debug_struct("Artwork")
        .field("session", session)
        .field("surface", surface)
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
  disconnect_button: gtk::Button,
  content: gtk::Stack,
  nav_home: gtk::ToggleButton,
  shortcuts: gtk::Box,
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
    let home_page = HomePage::build(sender.input_sender());
    let browse_page = BrowsePage::build(sender.input_sender());
    let detail_page = DetailPage::build(sender.input_sender());
    let player = PlayerPage::build(sender.input_sender());
    let ui = Ui::new(
      &sender,
      login.root(),
      home_page.root(),
      browse_page.root(),
      detail_page.root(),
      player.root(),
    );

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
      home_page,
      browse_page,
      detail_page,
      player,
      intro_mode,
      diagnostics,
      saved_profiles: LoadState::Loading,
      active_saved_profile: None,
      artwork,
      artwork_binder: ArtworkBinder::default(),
      image_cache_clearing: false,
      requests: RequestGate::default(),
      connection: ConnectionPhase::SignedOut,
      remote_state: RemoteControlState::Unavailable,
      remote_socket: None,
      playback_session: PlaybackSession::default(),
      playback_controller: None,
      playback_reconfigure_pending: false,
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
      AppMessage::Home(message) => self.dispatch_home(message, &sender),
      AppMessage::Browse(message) => self.dispatch_browse(message, &sender),
      AppMessage::Detail(message) => self.dispatch_detail(message, &sender),
      AppMessage::Player(message) => self.dispatch_player(message, &sender),
      AppMessage::Disconnect => {
        if !self.login.is_profile_busy() {
          self.disconnect(&sender);
        }
      }
      AppMessage::ShowHome => self.show_home(&sender),
      AppMessage::SearchRequested => self.start_search(&sender),
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
      AppCommand::HomeEvent(event) => self.dispatch_home_event(event, &sender),
      AppCommand::BrowseEvent(event) => self.dispatch_browse_event(event, &sender),
      AppCommand::DetailEvent(event) => self.dispatch_detail_event(event, &sender),
      AppCommand::Artwork {
        session,
        surface,
        slot,
        result,
      } => self.settle_artwork(session, surface, slot, result),
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

  fn dispatch_home(&mut self, message: home::Message, sender: &ComponentSender<Self>) {
    let effects = self.with_home_context(|page, cx| page.handle(message, cx));
    self.execute_home_effects(effects, sender);
  }

  fn dispatch_home_event(&mut self, event: HomeEvent, sender: &ComponentSender<Self>) {
    let effects = self.with_home_context(|page, cx| page.handle_event(event, cx));
    self.render_shortcuts(sender);
    self.execute_home_effects(effects, sender);
  }

  fn with_home_context<R>(
    &mut self,
    f: impl FnOnce(&mut HomePage, &mut HomeContext<'_>) -> R,
  ) -> R {
    let connection = self.connection;
    let playback_enabled = self.playback_controls_enabled();
    let mut cx = HomeContext {
      gate: &mut self.requests,
      binder: &mut self.artwork_binder,
      connection,
      playback_enabled,
    };
    f(&mut self.home_page, &mut cx)
  }

  fn execute_home_effects(&mut self, effects: Vec<HomeEffect>, sender: &ComponentSender<Self>) {
    for effect in effects {
      match effect {
        HomeEffect::BeginArtworkView => self.begin_page_artwork_view(sender),
        HomeEffect::ArtworkLoad {
          surface,
          slot,
          image_id,
        } => self.spawn_artwork_load(surface, slot, image_id, sender),
        HomeEffect::HomeLoad { token } => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            AppCommand::HomeEvent(home::load_home_data(client, token).await)
          });
        }
        HomeEffect::OpenDetail(item) => self.dispatch_detail(detail::Message::Open(item), sender),
        HomeEffect::OpenLibrary(shortcut) => {
          self.select_library_shortcut(&shortcut.id);
          self.navigate_to("browse");
          self.dispatch_browse(browse::Message::OpenLibrary(shortcut), sender);
        }
        HomeEffect::PlayItem(item, position) => self.start_playback(item, position, sender),
        HomeEffect::RenderIfVisible => {
          if self.visible_page().as_deref() == Some("home") {
            let render = self.with_home_context(|page, cx| page.render(cx));
            self.execute_home_effects(render, sender);
          }
        }
      }
    }
  }
  fn dispatch_browse(&mut self, message: browse::Message, sender: &ComponentSender<Self>) {
    match &message {
      browse::Message::OpenLibrary(shortcut) => {
        self.select_library_shortcut(&shortcut.id);
        self.navigate_to("browse");
      }
      browse::Message::Search(_) => {
        self.ui.nav_home.set_active(true);
        self.ui.nav_home.set_active(false);
        self.navigate_to("browse");
      }
      _ => {}
    }
    let effects = self.with_browse_context(|page, cx| page.handle(message, cx));
    self.execute_browse_page_effects(effects, sender);
  }

  fn dispatch_browse_event(&mut self, event: BrowseEvent, sender: &ComponentSender<Self>) {
    let effects = self.with_browse_context(|page, cx| page.handle_event(event, cx));
    let render = effects
      .iter()
      .any(|effect| matches!(effect, BrowseEffect::Render));
    let effects = effects
      .into_iter()
      .filter(|effect| !matches!(effect, BrowseEffect::Render))
      .collect();
    self.execute_browse_page_effects(effects, sender);
    if render && self.visible_page().as_deref() == Some("browse") {
      let render = self.with_browse_context(|page, cx| page.render(cx));
      self.execute_browse_page_effects(render, sender);
    }
  }

  fn with_browse_context<R>(
    &mut self,
    f: impl FnOnce(&mut BrowsePage, &mut BrowseContext<'_>) -> R,
  ) -> R {
    let playback_enabled = self.playback_controls_enabled();
    let mut cx = BrowseContext {
      gate: &mut self.requests,
      binder: &mut self.artwork_binder,
      playback_enabled,
    };
    f(&mut self.browse_page, &mut cx)
  }

  fn execute_browse_page_effects(
    &mut self,
    effects: Vec<BrowseEffect>,
    sender: &ComponentSender<Self>,
  ) {
    for effect in effects {
      match effect {
        BrowseEffect::BeginArtworkView => self.begin_page_artwork_view(sender),
        BrowseEffect::ArtworkLoad {
          surface,
          slot,
          image_id,
        } => self.spawn_artwork_load(surface, slot, image_id, sender),
        BrowseEffect::BrowsePage(request) => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            AppCommand::BrowseEvent(BrowseEvent::Page(
              browse::fetch_browse_page(client, request).await,
            ))
          });
        }
        BrowseEffect::OpenDetail(item) => self.dispatch_detail(detail::Message::Open(item), sender),
        BrowseEffect::PlayItem(item, position) => self.start_playback(item, position, sender),
        BrowseEffect::Render => {
          let render = self.with_browse_context(|page, cx| page.render(cx));
          self.execute_browse_page_effects(render, sender);
        }
      }
    }
  }

  fn dispatch_detail(&mut self, message: detail::Message, sender: &ComponentSender<Self>) {
    let effects = self.with_detail_context(|page, cx| page.handle(message, cx));
    self.execute_detail_effects(effects, sender);
  }

  fn dispatch_detail_event(&mut self, event: DetailEvent, sender: &ComponentSender<Self>) {
    let effects = self.with_detail_context(|page, cx| page.handle_event(event, cx));
    let render = effects
      .iter()
      .any(|effect| matches!(effect, DetailEffect::Render));
    let effects = effects
      .into_iter()
      .filter(|effect| !matches!(effect, DetailEffect::Render))
      .collect();
    self.execute_detail_effects(effects, sender);
    if render && self.visible_page().as_deref() == Some("detail") {
      let render = self.with_detail_context(|page, cx| page.render(cx));
      self.execute_detail_effects(render, sender);
    }
  }

  fn with_detail_context<R>(
    &mut self,
    f: impl FnOnce(&mut DetailPage, &mut DetailContext<'_>) -> R,
  ) -> R {
    let playback_enabled = self.playback_controls_enabled();
    let current_page = self.visible_page();
    let mut cx = DetailContext {
      gate: &mut self.requests,
      binder: &mut self.artwork_binder,
      playback_enabled,
      current_page,
    };
    f(&mut self.detail_page, &mut cx)
  }

  fn execute_detail_effects(&mut self, effects: Vec<DetailEffect>, sender: &ComponentSender<Self>) {
    for effect in effects {
      match effect {
        DetailEffect::BeginArtworkView => self.begin_page_artwork_view(sender),
        DetailEffect::ArtworkLoad {
          surface,
          slot,
          image_id,
        } => self.spawn_artwork_load(surface, slot, image_id, sender),
        DetailEffect::DetailLoad { token, item } => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            AppCommand::DetailEvent(DetailEvent::Loaded {
              token,
              result: Box::new(detail::load_detail_content(client, item).await),
            })
          });
        }
        DetailEffect::Recommendations { token, item_id } => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            let result = client
              .library()
              .similar_video(item_id)
              .await
              .map_err(|error| error.to_string());
            AppCommand::DetailEvent(DetailEvent::Recommendations { token, result })
          });
        }
        DetailEffect::Streams { token, item_id } => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            let result = client
              .library()
              .item_streams(item_id)
              .await
              .map_err(|error| error.to_string());
            AppCommand::DetailEvent(DetailEvent::Streams { token, result })
          });
        }
        DetailEffect::SeasonNeighbors {
          token,
          item_id,
          series_id,
          season_number,
        } => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            AppCommand::DetailEvent(DetailEvent::SeasonNeighbors {
              token,
              result: detail::load_season_neighbors(client, item_id, series_id, season_number)
                .await,
            })
          });
        }
        DetailEffect::SeasonPage {
          token,
          season_id,
          request,
        } => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            let result = client
              .library()
              .season_episodes_page(request)
              .await
              .map_err(|error| error.to_string());
            AppCommand::DetailEvent(DetailEvent::SeasonEpisodes {
              token,
              season_id,
              result,
            })
          });
        }
        DetailEffect::UserDataAction {
          token,
          item_id,
          action,
        } => {
          let client = Arc::clone(&self.client);
          sender.oneshot_command(async move {
            let result = client
              .library()
              .update_user_data(VideoUserDataUpdateRequest { item_id, action })
              .await
              .map_err(|_| "Could not update this item's library state.".to_owned());
            AppCommand::DetailEvent(DetailEvent::UserData { token, result })
          });
        }
        DetailEffect::PlayItem(item, position) => self.start_playback(item, position, sender),
        DetailEffect::ShowDetail => self.show_page("detail"),
        DetailEffect::Back { origin } => {
          self.navigate_to(&origin);
          match origin.as_str() {
            "home" => {
              let render = self.with_home_context(|page, cx| page.render(cx));
              self.execute_home_effects(render, sender);
            }
            "browse" => {
              let render = self.with_browse_context(|page, cx| page.render(cx));
              self.execute_browse_page_effects(render, sender);
            }
            _ => {}
          }
        }
        DetailEffect::Render => {
          let render = self.with_detail_context(|page, cx| page.render(cx));
          self.execute_detail_effects(render, sender);
        }
      }
    }
  }

  fn dispatch_player(&mut self, message: player::Message, sender: &ComponentSender<Self>) {
    let effects = self.with_player_context(|page, cx| page.handle(message, cx));
    self.execute_player_effects(effects, sender);
  }

  fn dispatch_player_event(&mut self, event: PlayerEvent<'_>, sender: &ComponentSender<Self>) {
    let effects = self.with_player_context(|page, cx| page.handle_event(event, cx));
    self.execute_player_effects(effects, sender);
  }

  fn with_player_context<R>(
    &mut self,
    f: impl FnOnce(&mut PlayerPage, &mut PlayerContext<'_>) -> R,
  ) -> R {
    let mut cx = PlayerContext {
      artwork: self.artwork.as_ref(),
      binder: &mut self.artwork_binder,
    };
    f(&mut self.player, &mut cx)
  }

  fn execute_player_effects(&mut self, effects: Vec<PlayerEffect>, sender: &ComponentSender<Self>) {
    for effect in effects {
      match effect {
        PlayerEffect::TogglePaused => self.dispatch_playback(PlaybackIntent::TogglePaused, sender),
        PlayerEffect::Seek(position) => {
          self.dispatch_playback(PlaybackIntent::Seek(position), sender)
        }
        PlayerEffect::SetVolume(volume) => {
          self.dispatch_playback(PlaybackIntent::SetVolume(volume), sender)
        }
        PlayerEffect::SetMuted(muted) => {
          self.dispatch_playback(PlaybackIntent::SetMuted(muted), sender)
        }
        PlayerEffect::SelectAudioTrack(id) => {
          self.dispatch_playback(PlaybackIntent::SelectAudioTrack(id), sender)
        }
        PlayerEffect::SelectSubtitleTrack(id) => {
          self.dispatch_playback(PlaybackIntent::SelectSubtitleTrack(id), sender)
        }
        PlayerEffect::PlayAdjacent(direction) => {
          self.dispatch_playback(PlaybackIntent::PlayAdjacent(direction), sender)
        }
        PlayerEffect::Stop => self.dispatch_playback(PlaybackIntent::Stop, sender),
        PlayerEffect::ArtworkLoad {
          surface,
          slot,
          image_id,
        } => self.spawn_artwork_load(surface, slot, image_id, sender),
      }
    }
  }

  fn render_player(&self) {
    let view = self.playback_session.view();
    self.player.render(&view);
  }

  fn visible_page(&self) -> Option<String> {
    self
      .ui
      .content
      .visible_child_name()
      .map(|name| name.to_string())
  }

  fn begin_page_artwork_view(&mut self, sender: &ComponentSender<Self>) {
    self.artwork.cancel_pending();
    self.dispatch_player_event(PlayerEvent::RefreshArtwork, sender);
  }

  fn spawn_artwork_load(
    &self,
    surface: ArtworkSurface,
    slot: ArtworkSlot,
    image_id: String,
    sender: &ComponentSender<Self>,
  ) {
    let artwork = Arc::clone(&self.artwork);
    let artwork_ticket = artwork.ticket();
    let client = Arc::clone(&self.client);
    let session = self.requests.current_session();
    sender.oneshot_command(async move {
      let result = artwork
        .load_with_ticket(&client, &image_id, artwork_ticket)
        .await
        .map_err(|_| ());
      AppCommand::Artwork {
        session,
        surface,
        slot,
        result,
      }
    });
  }

  fn settle_artwork(
    &mut self,
    session: SessionToken,
    surface: ArtworkSurface,
    slot: ArtworkSlot,
    result: Result<DecodedArtwork, ()>,
  ) {
    let session_ok = self.requests.is_current_session(session);
    if self.artwork_binder.settle(slot, surface, session_ok) != ArtworkSettlement::Apply {
      return;
    }
    let Ok(decoded) = result else {
      self.record_artwork_failure();
      return;
    };
    let applied = match surface {
      ArtworkSurface::Home => self.home_page.apply_artwork(slot, decoded),
      ArtworkSurface::Browse => self.browse_page.apply_artwork(slot, decoded),
      ArtworkSurface::Detail => self.detail_page.apply_artwork(slot, decoded),
      ArtworkSurface::PlayerBar => self.player.apply_artwork(slot, decoded),
    };
    if applied {
      self.diagnostics.reset_coalescing();
    } else {
      self.record_artwork_failure();
    }
  }

  fn execute_settings_effects(
    &mut self,
    effects: Vec<SettingsEffect>,
    sender: &ComponentSender<Self>,
  ) {
    for effect in effects {
      match effect {
        SettingsEffect::ReconfigurePlayback => self.reconfigure_playback_controller(sender),
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
          self.browse_page.reset();
          self.connection = ConnectionPhase::Connecting;
          self.home_page.prepare_connected_session();
        }

        LoginEffect::Authenticated {
          client,
          stored_session,
        } => self.finish_login(client, stored_session, sender),
        LoginEffect::AuthFailed { message } => {
          self.connection = ConnectionPhase::Failed;
          self.home_page.show_failure(&message);
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
          self.home_page.reset();
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
        self.dispatch_player_event(PlayerEvent::EngineAvailable, sender);
        self.apply_playback_event(PlaybackEvent::EngineAvailability(true), sender);
      }
      Err(error) => {
        self.record_diagnostic(
          DiagnosticLevel::Error,
          DiagnosticCategory::Playback,
          format!("External MPV playback is unavailable: {error}."),
        );
        self.playback_controller = None;
        self.dispatch_player_event(
          PlayerEvent::EngineUnavailable(format!(
            "Playback is unavailable: {error}. Install MPV and try again."
          )),
          sender,
        );
        self.apply_playback_event(PlaybackEvent::EngineAvailability(false), sender);
      }
    }
    self.connection = ConnectionPhase::Connected;
    self.start_remote_session(sender);
    self.home_page.prepare_connected_session();

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
    self.dispatch_home(home::Message::Load, sender);
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
    self.artwork_binder.reset();
    self.home_page.reset();
    self.browse_page.reset();
    self.detail_page.reset();
    let _client = std::mem::replace(&mut self.client, Arc::new(JellyfinClient::new()));
    self.dispatch_playback(PlaybackIntent::Disconnect, sender);
    self.apply_playback_event(PlaybackEvent::EngineAvailability(false), sender);
    self.dispatch_player_event(PlayerEvent::EngineAvailable, sender);
    self.dispatch_player_event(PlayerEvent::Stopped, sender);
    self.connection = ConnectionPhase::SignedOut;
    self.active_saved_profile = None;
    self.requests.set_detail_item(None);
    self.invalidate_user_data_update();

    self.ui.search.set_text("");
    self.render_player();
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
    let render = self.with_home_context(|page, cx| page.render(cx));
    self.execute_home_effects(render, sender);
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
    if let Some(message) = self.home_page.shortcuts_error() {
      let retry = gtk::Button::with_label("Retry libraries");
      retry.set_tooltip_text(Some(message));
      let sender = sender.clone();
      retry.connect_clicked(move |_| sender.input(AppMessage::Home(home::Message::Retry)));
      self.ui.shortcuts.append(&retry);
      return;
    }
    if self.home_page.shortcuts().is_empty() {
      self
        .ui
        .shortcuts
        .append(&dim_label("No video libraries available."));
      return;
    }
    for shortcut in self.home_page.shortcuts() {
      let button = navigation_button(&shortcut.name, "folder-videos-symbolic");
      button.set_group(Some(&self.ui.nav_home));
      let shortcut = shortcut.clone();
      let sender = sender.clone();
      button.connect_clicked(move |_| {
        sender.input(AppMessage::Browse(browse::Message::OpenLibrary(
          shortcut.clone(),
        )))
      });
      self.ui.shortcuts.append(&button);
    }
  }

  fn select_library_shortcut(&self, shortcut_id: &str) {
    let Some(index) = self
      .home_page
      .shortcuts()
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
    self.dispatch_browse(browse::Message::Search(query), sender);
  }

  fn invalidate_user_data_update(&mut self) {
    self
      .requests
      .invalidate_detail_aux(crate::request_gate::DetailAuxKind::UserData);
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
    self.render_player();
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
          let Some(item) = self.player.item().cloned() else {
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
      self.dispatch_player_event(PlayerEvent::Started(item), sender);
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
      self.dispatch_player_event(PlayerEvent::Stopped, sender);
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
        if let Some(item) = self.player.item() {
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

  fn reconfigure_playback_controller(&mut self, sender: &ComponentSender<Self>) {
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
          self.dispatch_player_event(PlayerEvent::EngineAvailable, sender);
          self.playback_reconfigure_pending = false;
          self.playback_session.handle(
            PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
            Instant::now(),
          );
          self
            .settings
            .set_config_status("Saved. MPV is available for the next playback start.");
          self.render_player();
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
}

impl Ui {
  fn new(
    sender: &ComponentSender<AppModel>,
    login: &gtk::ScrolledWindow,
    home: &gtk::Widget,
    browse: &gtk::ScrolledWindow,
    detail: &gtk::Widget,
    player: &gtk::Box,
  ) -> Self {
    install_media_css();
    let toast_overlay = adw::ToastOverlay::new();
    let root = adw::ToolbarView::new();
    root.add_bottom_bar(player);
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
    content.add_named(home, Some("home"));
    content.add_named(browse, Some("browse"));
    content.add_named(detail, Some("detail"));

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
      disconnect_button,
      content,
      nav_home,
      shortcuts,
    }
  }
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

fn connection_label(client: &JellyfinClient) -> String {
  let state = client.login().connection_state();
  match (state.server_name, state.user_name) {
    (Some(server), Some(user)) => format!("Connected to {server} as {user}"),
    (Some(server), None) => format!("Connected to {server}"),
    _ => "Connected".to_owned(),
  }
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
