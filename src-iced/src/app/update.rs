use std::collections::{hash_map::DefaultHasher, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::futures::stream::{self, StreamExt};
use iced::widget::{image, operation};
use iced::Task;
use jellypilot_auth::login::{
  can_start_login, quick_connect_workflow, should_disconnect_after_forget, ConnectionPhase,
  LoginError, LoginEvent, QUICK_CONNECT_POLL_INTERVAL, QUICK_CONNECT_TIMEOUT,
};
use jellypilot_auth::AuthStore;
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::artwork_loader::{
  grid_cell_visible, plan_artwork_loads, visible_row_cards, PlannedArtworkLoad,
};
use jellypilot_core::browse::fetch_browse_page;
use jellypilot_core::browse_model::{
  BrowseEffect, BrowsePageRequest, BrowsePageSettlement, BrowsePreferences, BrowseSource,
  LibraryBrowseView,
};
use jellypilot_core::config::{
  BrowseFilterSettings, LoginPrefill, Settings, SettingsMutationError,
};
use jellypilot_core::detail::{
  apply_user_data_update, load_detail_content, load_season_neighbors, season_page_request,
  DetailContent,
};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::request_gate::{
  DetailAuxKind, DetailToken, HomeToken, RemotePlayToken, RequestGate,
};
use jellypilot_media_server::artwork::{
  ArtworkLoadObservation, ArtworkLoadSummary, ArtworkSizeClass, LoadLane,
};
use jellypilot_media_server::home::{load_home_data, HomeDataResult};
use jellypilot_media_server::{
  Credentials, JellyfinClient, MediaServerProvider, VideoLibraryItem, VideoLibrarySortDirection,
  VideoSeason, VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest, VideoUserDataAction,
  VideoUserDataUpdate, VideoUserDataUpdateRequest,
};
use jellypilot_mpv::configured_mpv_args;
use jellypilot_mpv::playback::{
  media_item_from_playable, Playable, PlaybackController, PlaybackControllerConfig,
  PlaybackEndReason, PlaybackError, PlaybackRefreshOutcome, PlaybackRefreshState,
  PlaybackSelection, PlaybackSnapshot,
};
use jellypilot_mpv::playback_session::{
  AdjacentDirection, ControllerCommand, ControllerSettlement, PlaybackEffect, PlaybackEvent,
  PlaybackInput, PlaybackIntent, PlaybackNotice, SessionView, TracksView,
};
use jellypilot_session::{
  finalize_remote_target, remote_index_value, remote_volume_value, GeneralCommand, JellyfinCommand,
  JellyfinWebSocket, JellyfinWebSocketEvent, PlayRequest, PlaystateRequest, RemoteControlState,
};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::widgets::artwork_grid::{ArtworkGridMetrics, ArtworkGridViewport};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use crate::tray::TrayAction;

use super::message::{
  ArtworkLoadCompletion, BrowseMessage, DetailMessage, HomeMessage, LoginMessage, Message,
  PasswordSubmission, PlaybackMessage, ProtectedSavedSession, RemoteMessage, RemoteSessionStart,
  RemoteStartError, SensitiveSessionPayload, SettingsMessage, WindowMessage,
};
use super::state::{
  ArtworkCell, ArtworkCellState, BrowseArtwork, BrowseViewport, ConnectedIdentity, Destination,
  DetailArtwork, DetailState, HomeArtwork, HomeSection, HomeState, LoginMethod, NoticeLevel,
  QuickConnectState, RemoteEventChannel, RemoteSessionHandle, State, UserDataActionKind,
};
use super::view::browse::{grid_available_width, CARD_COPY_HEIGHT};
use super::view::home::{content_width, section_frame_size};

const SETTINGS_SAVE_ERROR: &str = "Could not save settings.";
const INVALID_LOGIN_PREFILL_ERROR: &str = "Server and username are required.";
const INVALID_PROVIDER_ERROR: &str = "The selected provider is invalid.";
const INVALID_SUBTITLE_LANGUAGE_ERROR: &str = "Choose a valid subtitle language.";
const DUPLICATE_SUBTITLE_LANGUAGE_ERROR: &str = "That subtitle language is already in the list.";
const EMPTY_SHORTCUT_ERROR: &str = "Press a non-modifier key for this shortcut.";
const SHORTCUT_COLLISION_ERROR: &str = "That shortcut is already assigned.";
const PLAYBACK_CONFIG_APPLY_ERROR: &str = "Settings were saved, but MPV could not be reconfigured.";

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  match message {
    Message::Window(message) => update_window(state, message),
    Message::Login(message) => {
      let was_connected = state.kernel.connection == ConnectionPhase::Connected;
      let previous_error = state.login.error.clone();
      let login_task = update_login(state, message).map(Message::Login);
      let is_connected = state.kernel.connection == ConnectionPhase::Connected;
      if state.login.error != previous_error {
        if let Some(error) = &state.login.error {
          state
            .kernel
            .diagnostics
            .record(DiagnosticLevel::Error, DiagnosticCategory::Auth, error);
        }
      }
      if !was_connected && is_connected {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Connection,
          "Connected to media server.",
        );
        state.destination = Destination::Home;
        initialize_playback(state);
        Task::batch([
          login_task,
          start_home_load(state),
          start_remote_session(state),
        ])
      } else if was_connected && !is_connected {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Connection,
          "Disconnected from media server.",
        );
        Task::batch([login_task, reset_connected_surface(state)])
      } else {
        login_task
      }
    }
    Message::Home(message) => update_home(state, message),
    Message::Browse(message) => {
      let previous_notice = state.kernel.notice.clone();
      let task = update_browse(state, message);
      if let Some(notice) = state
        .kernel
        .notice
        .clone()
        .filter(|notice| Some(notice.as_str()) != previous_notice.as_deref())
      {
        let toast_task = state.show_toast(NoticeLevel::Error, notice);
        Task::batch([task, toast_task])
      } else {
        task
      }
    }
    Message::OpenDetail(item) => open_detail(state, item),
    Message::Detail(message) => update_detail(state, message),
    Message::Settings(message) => update_settings(state, message),
    Message::Playback(message) => {
      let previous_notice = state.playback_notice.clone();
      let task = update_playback(state, message);
      let toast_task = record_playback_notice(state, previous_notice.as_deref());
      Task::batch([task, toast_task])
    }
    Message::Remote(message) => {
      let previous_state = state.remote_control_state;
      let previous_notice = state.kernel.notice.clone();
      let task = update_remote(state, message);
      let toast_task = record_remote_change(state, previous_state, previous_notice.as_deref());
      Task::batch([task, toast_task])
    }
    Message::Tray(action) => update_tray(state, action),
    Message::DismissNotice(id) => {
      state.dismiss_toast(id);
      Task::none()
    }
    Message::ArtworkStreamCompleted(summary) => {
      if let Some(message) = summary.diagnostic_message() {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Artwork,
          message,
        );
      }
      Task::none()
    }
  }
}

fn record_playback_notice(state: &mut State, previous: Option<&str>) -> Task<Message> {
  let Some(notice) = state.playback_notice.clone() else {
    if previous.is_some() {
      state.kernel.diagnostics.reset_coalescing();
    }
    return Task::none();
  };
  if Some(notice.as_str()) == previous {
    return Task::none();
  }
  let level = if notice.contains("failed")
    || notice.contains("Failed")
    || notice.contains("unavailable")
    || notice.contains("Unavailable")
  {
    DiagnosticLevel::Error
  } else {
    DiagnosticLevel::Warning
  };
  let key = diagnostic_coalescing_key("playback", &notice);
  state
    .kernel
    .diagnostics
    .record_coalesced(&key, level, DiagnosticCategory::Playback, &notice);

  let toast_level = match level {
    DiagnosticLevel::Error => NoticeLevel::Error,
    _ => NoticeLevel::Warning,
  };
  state.show_toast(toast_level, notice)
}

fn diagnostic_coalescing_key(prefix: &str, message: &str) -> String {
  let mut hasher = DefaultHasher::new();
  message.hash(&mut hasher);
  format!("{prefix}-{:x}", hasher.finish())
}

fn record_remote_change(
  state: &mut State,
  previous_state: RemoteControlState,
  previous_notice: Option<&str>,
) -> Task<Message> {
  if state.remote_control_state != previous_state {
    let (level, message) = match state.remote_control_state {
      RemoteControlState::Connecting => {
        (DiagnosticLevel::Info, "Remote playback target connecting.")
      }
      RemoteControlState::Available => (DiagnosticLevel::Info, "Remote playback target available."),
      RemoteControlState::Lost => (
        DiagnosticLevel::Warning,
        "Remote playback target connection lost.",
      ),
      RemoteControlState::Unavailable => (
        DiagnosticLevel::Warning,
        "Remote playback target unavailable.",
      ),
    };
    state
      .kernel
      .diagnostics
      .record(level, DiagnosticCategory::RemoteControl, message);
  }
  if let Some(notice) = state
    .kernel
    .notice
    .clone()
    .filter(|notice| Some(notice.as_str()) != previous_notice)
  {
    state.kernel.diagnostics.record(
      DiagnosticLevel::Warning,
      DiagnosticCategory::RemoteControl,
      &notice,
    );
    return state.show_toast(NoticeLevel::Warning, notice);
  }
  Task::none()
}

fn update_window(state: &mut State, message: WindowMessage) -> Task<Message> {
  match message {
    WindowMessage::CloseRequested(id) if state.kernel.tray.is_some() => {
      iced::window::set_mode(id, iced::window::Mode::Hidden)
    }
    WindowMessage::CloseRequested(_) => {
      state.quit_requested = true;
      Task::batch([
        apply_playback_input(state, PlaybackInput::Intent(PlaybackIntent::Quit)),
        stop_remote_session_for_quit(state),
      ])
    }
    WindowMessage::ShowRequested(id) => id.map_or_else(Task::none, |id| {
      iced::window::set_mode(id, iced::window::Mode::Windowed).chain(iced::window::gain_focus(id))
    }),
    WindowMessage::Resized(size) => {
      state.window_size = size;
      sync_browse_scroll_window(state)
    }
    WindowMessage::FrameTick(now) => {
      // Smoke runs only need proof that the first frame rendered.
      if state.smoke {
        state.smoke = false;
        return iced::exit();
      }
      if state.skeletons_active() {
        let start = state.skeleton_animation_start.get_or_insert(now);
        state.skeleton_phase = skeleton_phase_at(now.duration_since(*start));
      } else {
        // Restart the sweep from phase 0 on the next loading burst.
        state.skeleton_animation_start = None;
        state.skeleton_phase = 0.0;
      }
      Task::none()
    }
  }
}

/// Breathing pulse phase in [0, 1): one full pulse per 1600ms, matching
/// `tokens.durations.ms1600` in jellypilot-ui.
fn skeleton_phase_at(elapsed: Duration) -> f32 {
  (elapsed.as_secs_f32() / Duration::from_millis(1600).as_secs_f32()).fract()
}
fn update_settings(state: &mut State, message: SettingsMessage) -> Task<Message> {
  match message {
    SettingsMessage::MpvPathChanged(value) => {
      state.settings_view.mpv_path_input = value;
      clear_settings_feedback(state);
      Task::none()
    }
    SettingsMessage::SaveMpvPath => {
      let value = state.settings_view.mpv_path_input.clone();
      let result = state.settings.set_mpv_path(value);
      if finish_settings_mutation(state, result) {
        apply_playback_configuration(state)
      } else {
        Task::none()
      }
    }
    SettingsMessage::MpvArgsChanged(value) => {
      state.settings_view.mpv_args_input = value;
      clear_settings_feedback(state);
      Task::none()
    }
    SettingsMessage::SaveMpvArgs => {
      let value = state.settings_view.mpv_args_input.clone();
      let result = state.settings.set_mpv_args(&value);
      if finish_settings_mutation(state, result) {
        state.settings_view.mpv_args_input = state.settings.snapshot().mpv_args().join(" ");
        apply_playback_configuration(state)
      } else {
        Task::none()
      }
    }
    SettingsMessage::PlaybackTargetNameChanged(value) => {
      state.settings_view.playback_target_name_input = value;
      clear_settings_feedback(state);
      Task::none()
    }
    SettingsMessage::SavePlaybackTargetName => {
      let value = state.settings_view.playback_target_name_input.clone();
      let result = state.settings.set_playback_target_name(value);
      if finish_settings_mutation(state, result) {
        refinalize_playback_target(state)
      } else {
        Task::none()
      }
    }
    SettingsMessage::IntroMenuToggled => {
      state.settings_view.intro_menu_open = !state.settings_view.intro_menu_open;
      Task::none()
    }
    SettingsMessage::IntroMenuDismissed => {
      state.settings_view.intro_menu_open = false;
      Task::none()
    }
    SettingsMessage::IntroModeSelected(mode) => {
      state.settings_view.intro_menu_open = false;
      let result = state.settings.set_intro_mode(mode);
      if finish_settings_mutation(state, result) {
        let mode = state.intro_availability().mode;
        apply_playback_input(
          state,
          PlaybackInput::Intent(PlaybackIntent::SetIntroMode(mode)),
        )
      } else {
        Task::none()
      }
    }
    SettingsMessage::SubtitleMenuToggled => {
      state.settings_view.subtitle_menu_open = !state.settings_view.subtitle_menu_open;
      Task::none()
    }
    SettingsMessage::SubtitleMenuDismissed => {
      state.settings_view.subtitle_menu_open = false;
      Task::none()
    }
    SettingsMessage::SubtitleLanguageAdded(language) => {
      state.settings_view.subtitle_menu_open = false;
      let result = state.settings.add_subtitle_language(language);
      if finish_settings_mutation(state, result) {
        apply_playback_configuration(state)
      } else {
        Task::none()
      }
    }
    SettingsMessage::SubtitleLanguageMoved { index, offset } => {
      let result = state.settings.move_subtitle_language(index, offset);
      if finish_settings_mutation(state, result) {
        apply_playback_configuration(state)
      } else {
        Task::none()
      }
    }
    SettingsMessage::SubtitleLanguageRemoved(index) => {
      let result = state.settings.remove_subtitle_language(index);
      if finish_settings_mutation(state, result) {
        apply_playback_configuration(state)
      } else {
        Task::none()
      }
    }
    SettingsMessage::BeginShortcutCapture(kind) => {
      state.settings_view.shortcut_capture = Some(kind);
      clear_settings_feedback(state);
      Task::none()
    }
    SettingsMessage::ShortcutCaptured(binding) => {
      let Some(kind) = state.settings_view.shortcut_capture.take() else {
        return Task::none();
      };
      let result = state.settings.set_shortcut(kind, binding);
      finish_settings_mutation(state, result);
      Task::none()
    }
    SettingsMessage::CancelShortcutCapture => {
      state.settings_view.shortcut_capture = None;
      Task::none()
    }
    SettingsMessage::ImageCacheToggled => {
      let enabled = !state.settings.snapshot().image_cache_enabled();
      let result = state.settings.set_image_cache_enabled(enabled);
      if finish_settings_mutation(state, result) {
        state.kernel.artwork_adapter.set_disk_cache_enabled(enabled);
      }
      Task::none()
    }
    SettingsMessage::StartMinimizedToggled => {
      let enabled = !state.settings.snapshot().start_minimized();
      let result = state.settings.set_start_minimized(enabled);
      finish_settings_mutation(state, result);
      Task::none()
    }
    SettingsMessage::ReducedMotionToggled => {
      let enabled = !state.settings.snapshot().reduced_motion();
      let result = state.settings.set_reduced_motion(enabled);
      finish_settings_mutation(state, result);
      Task::none()
    }
    SettingsMessage::DiagnosticLevelMenuToggled => {
      state.settings_view.diagnostic_level_menu_open =
        !state.settings_view.diagnostic_level_menu_open;
      Task::none()
    }
    SettingsMessage::DiagnosticLevelMenuDismissed => {
      state.settings_view.diagnostic_level_menu_open = false;
      Task::none()
    }
    SettingsMessage::DiagnosticLevelSelected(level) => {
      state.settings_view.diagnostic_level = level;
      state.settings_view.diagnostic_level_menu_open = false;
      Task::none()
    }
    SettingsMessage::DiagnosticCategoryMenuToggled => {
      state.settings_view.diagnostic_category_menu_open =
        !state.settings_view.diagnostic_category_menu_open;
      Task::none()
    }
    SettingsMessage::DiagnosticCategoryMenuDismissed => {
      state.settings_view.diagnostic_category_menu_open = false;
      Task::none()
    }
    SettingsMessage::DiagnosticCategorySelected(category) => {
      state.settings_view.diagnostic_category = category;
      state.settings_view.diagnostic_category_menu_open = false;
      Task::none()
    }
    SettingsMessage::Disconnect => stop_remote_session_for_login(state).map(Message::Login),
    SettingsMessage::SignOut => {
      if let Some(key) = state.kernel.active_profile.clone() {
        start_forget(state, key)
          .map(|task| task.map(Message::Login))
          .unwrap_or_else(Task::none)
      } else {
        stop_remote_session_for_login(state).map(Message::Login)
      }
    }
    SettingsMessage::PlaybackConfigApplied(result) => {
      if result.is_err() {
        state.settings_view.error = Some(PLAYBACK_CONFIG_APPLY_ERROR);
        state.kernel.diagnostics.record(
          DiagnosticLevel::Error,
          DiagnosticCategory::Config,
          PLAYBACK_CONFIG_APPLY_ERROR,
        );
      }
      Task::none()
    }
  }
}

fn clear_settings_feedback(state: &mut State) {
  state.settings_view.error = None;
  state.settings_view.saved = None;
}

fn finish_settings_mutation(
  state: &mut State,
  result: Result<bool, SettingsMutationError>,
) -> bool {
  match result {
    Ok(changed) => {
      state.settings_view.error = None;
      state.settings_view.saved = Some("Saved");
      if changed {
        state.kernel.diagnostics.record(
          DiagnosticLevel::Info,
          DiagnosticCategory::Config,
          "Settings updated.",
        );
      }
      changed
    }
    Err(error) => {
      state.settings_view.saved = None;
      state.settings_view.error = Some(settings_mutation_error(&error));
      state.kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Config,
        error.to_string(),
      );
      false
    }
  }
}

fn settings_mutation_error(error: &SettingsMutationError) -> &'static str {
  match error {
    SettingsMutationError::Config(_) => SETTINGS_SAVE_ERROR,
    SettingsMutationError::InvalidLoginPrefill => INVALID_LOGIN_PREFILL_ERROR,
    SettingsMutationError::InvalidProvider => INVALID_PROVIDER_ERROR,
    SettingsMutationError::InvalidSubtitleLanguage => INVALID_SUBTITLE_LANGUAGE_ERROR,
    SettingsMutationError::DuplicateSubtitleLanguage => DUPLICATE_SUBTITLE_LANGUAGE_ERROR,
    SettingsMutationError::EmptyShortcut => EMPTY_SHORTCUT_ERROR,
    SettingsMutationError::ShortcutCollision => SHORTCUT_COLLISION_ERROR,
  }
}

fn apply_playback_configuration(state: &mut State) -> Task<Message> {
  let config = playback_controller_config(state.settings.snapshot());
  if let Some(controller) = state.playback_controller.as_ref().map(Arc::clone) {
    return Task::perform(
      async move { controller.lock().await.configure_for_next_start(config) },
      |result| Message::Settings(SettingsMessage::PlaybackConfigApplied(result)),
    );
  }
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  match PlaybackController::discover(client, config) {
    Ok(controller) => {
      state.playback_controller = Some(Arc::new(tokio::sync::Mutex::new(controller)));
      let _ = state.playback_session.handle(
        PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
        Instant::now(),
      );
      sync_playback_projection(state);
      state.playback_notice = None;
      Task::none()
    }
    Err(error) => {
      let _ = state.playback_session.handle(
        PlaybackInput::Event(PlaybackEvent::EngineAvailability(false)),
        Instant::now(),
      );
      sync_playback_projection(state);
      state.playback_notice =
        Some("External playback is unavailable because MPV could not be found.".into());
      let toast_task = state.show_toast(
        NoticeLevel::Error,
        "External playback is unavailable because MPV could not be found.",
      );
      Task::batch([
        Task::done(Message::Settings(SettingsMessage::PlaybackConfigApplied(
          Err(error),
        ))),
        toast_task,
      ])
    }
  }
}

fn refinalize_playback_target(state: &mut State) -> Task<Message> {
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let name = state
    .settings
    .snapshot()
    .playback_target_name()
    .unwrap_or("JellyPilot")
    .to_owned();
  client.set_device_name(name);
  if !should_refinalize_playback_target(state) {
    return Task::none();
  }
  state.kernel.diagnostics.record(
    DiagnosticLevel::Info,
    DiagnosticCategory::RemoteControl,
    "Playback target name changed; remote registration requested.",
  );
  let remote = state.playback_remote;
  Task::perform(
    async move { finalize_remote_target(&client).await },
    move |result| Message::Remote(RemoteMessage::Finalized { remote, result }),
  )
}

fn should_refinalize_playback_target(state: &State) -> bool {
  state.kernel.connection == ConnectionPhase::Connected
    && state.remote_session.is_some()
    && state.remote_control_state == RemoteControlState::Available
}

fn start_remote_session(state: &mut State) -> Task<Message> {
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  if let Some(name) = state.settings.snapshot().playback_target_name() {
    client.set_device_name(name.to_owned());
  }

  let remote = state.playback_remote;
  let websocket = Arc::new(JellyfinWebSocket::new());
  let Some(mut websocket_events) = websocket.take_event_receiver() else {
    return Task::done(Message::Remote(RemoteMessage::Started {
      remote,
      result: Err(RemoteStartError::SessionUnavailable),
    }));
  };
  let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();
  state.remote_events = Some(RemoteEventChannel {
    remote,
    receiver: Arc::new(tokio::sync::Mutex::new(event_receiver)),
  });
  let session = RemoteSessionHandle {
    websocket: Arc::clone(&websocket),
    lifecycle: Arc::new(tokio::sync::Mutex::new(())),
  };
  state.remote_session = Some(session.clone());
  state.remote_control_state = RemoteControlState::Connecting;

  Task::perform(
    async move {
      let lifecycle = Arc::clone(&session.lifecycle);
      let _lifecycle = lifecycle.lock().await;
      let websocket_url = client
        .playback()
        .websocket_url()
        .map_err(|_| RemoteStartError::SessionUnavailable)?;
      let user_agent = client.playback().websocket_user_agent();
      let forwarder = tokio::spawn(async move {
        while let Some(event) = websocket_events.recv().await {
          if event_sender.send(event).is_err() {
            break;
          }
        }
      });
      if websocket
        .connect_with_user_agent(&websocket_url, Some(&user_agent))
        .await
        .is_err()
      {
        websocket.disconnect().await;
        let _ = forwarder.await;
        return Err(RemoteStartError::ConnectionFailed);
      }
      let validated = match finalize_remote_target(&client).await {
        Ok(validated) => validated,
        Err(()) => {
          websocket.disconnect().await;
          let _ = forwarder.await;
          return Err(RemoteStartError::CapabilityRegistrationFailed);
        }
      };
      Ok(RemoteSessionStart { session, validated })
    },
    move |result| Message::Remote(RemoteMessage::Started { remote, result }),
  )
}

const REMOTE_CONNECTION_LOST_NOTICE: &str = "Remote playback connection lost; reconnecting…";
const REMOTE_TRACKS_UNAVAILABLE_NOTICE: &str =
  "Remote track selection ignored because playback tracks are not loaded.";

fn update_remote(state: &mut State, message: RemoteMessage) -> Task<Message> {
  match message {
    RemoteMessage::Started { remote, result } => {
      if !state.kernel.request_gate.is_current_remote(remote)
        || state.kernel.connection != ConnectionPhase::Connected
      {
        return match result {
          Ok(started) => {
            let session = started.session;
            Task::perform(
              async move { disconnect_remote_session(session).await },
              |()| Message::Remote(RemoteMessage::RemoteDisconnected),
            )
          }
          Err(_) => Task::none(),
        };
      }
      match result {
        Ok(started) => {
          let RemoteSessionStart { session, validated } = started;
          state.remote_session = Some(session);
          state.remote_control_state = RemoteControlState::Available;
          if !validated {
            state.kernel.notice = Some(
              "Remote playback target connected, but server session validation is still pending."
                .to_owned(),
            );
          }
        }
        Err(error) => {
          state.playback_remote = state.kernel.request_gate.begin_remote();
          state.remote_session = None;
          state.remote_events = None;
          state.remote_control_state = RemoteControlState::Unavailable;
          state.kernel.notice = Some(error.diagnostic().to_owned());
        }
      }
      Task::none()
    }
    RemoteMessage::Event { remote, event } => {
      if !state.kernel.request_gate.is_current_remote(remote) {
        return Task::none();
      }
      match event {
        JellyfinWebSocketEvent::Command(command)
          if state.remote_control_state == RemoteControlState::Available =>
        {
          handle_remote_command(state, remote, command)
        }
        JellyfinWebSocketEvent::Command(_) => Task::none(),
        JellyfinWebSocketEvent::Reconnected => {
          let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
            return Task::none();
          };
          state.remote_control_state = RemoteControlState::Connecting;
          Task::perform(
            async move { finalize_remote_target(&client).await },
            move |result| Message::Remote(RemoteMessage::Finalized { remote, result }),
          )
        }
        JellyfinWebSocketEvent::ConnectionLost => {
          state.remote_control_state = RemoteControlState::Lost;
          state.kernel.notice = Some(REMOTE_CONNECTION_LOST_NOTICE.to_owned());
          Task::none()
        }
        JellyfinWebSocketEvent::Connected => Task::none(),
      }
    }
    RemoteMessage::Finalized { remote, result } => {
      if !state.kernel.request_gate.is_current_remote(remote) {
        return Task::none();
      }
      match result {
        Ok(true) => {
          state.remote_control_state = RemoteControlState::Available;
          if state.kernel.notice.as_deref() == Some(REMOTE_CONNECTION_LOST_NOTICE) {
            state.kernel.notice = None;
          }
          Task::none()
        }
        Ok(false) => {
          state.remote_control_state = RemoteControlState::Available;
          state.kernel.notice = Some(
            "Remote playback target reconnected, but server session validation is still pending."
              .to_owned(),
          );
          Task::none()
        }
        Err(()) => fail_remote_finalization(state),
      }
    }
    RemoteMessage::PlayResolved {
      remote,
      play,
      result,
      start_position_ticks,
      selection,
    } => {
      if !state.kernel.request_gate.is_current_remote(remote)
        || !state.kernel.request_gate.is_current_remote_play(play)
      {
        return Task::none();
      }
      let Ok(item) = *result else {
        state.kernel.notice = Some("Remote playback item could not be loaded.".to_owned());
        return Task::none();
      };
      let position = start_position_ticks.map_or(
        jellypilot_mpv::playback::PlaybackStartPosition::Beginning,
        |ticks| {
          jellypilot_mpv::playback::PlaybackStartPosition::At(
            jellypilot_media_server::ticks_to_seconds(ticks),
          )
        },
      );
      apply_playback_input(
        state,
        PlaybackInput::Intent(PlaybackIntent::Start {
          item,
          position,
          intro: state.intro_availability(),
          selection: Box::new(selection),
        }),
      )
    }
    RemoteMessage::RemoteDisconnected => Task::none(),
    RemoteMessage::QuitStopped => {
      state.remote_stopping = false;
      if quit_may_exit(state) {
        iced::exit()
      } else {
        Task::none()
      }
    }
  }
}

enum RemoteCommandAction {
  Intent(RemotePlaybackIntent),
  Play {
    item_id: String,
    start_position_ticks: Option<i64>,
    selection: PlaybackSelection,
  },
}

#[derive(Clone, Copy)]
enum RemotePlaybackIntent {
  SetPaused(bool),
  TogglePaused,
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  SelectAudioStream(i64),
  SelectSubtitleStream(Option<i64>),
  Stop,
  PlayAdjacent(AdjacentDirection),
}

impl RemotePlaybackIntent {
  fn into_playback_intent(self, playback: &SessionView) -> Option<PlaybackIntent> {
    match self {
      Self::SetPaused(paused) => Some(PlaybackIntent::SetPaused(paused)),
      Self::TogglePaused => Some(PlaybackIntent::TogglePaused),
      Self::Seek(position) => Some(PlaybackIntent::Seek(position)),
      Self::SetVolume(volume) => Some(PlaybackIntent::SetVolume(volume)),
      Self::SetMuted(muted) => Some(PlaybackIntent::SetMuted(muted)),
      Self::SelectAudioStream(index) => {
        provider_track_id(playback, "audio", index).map(PlaybackIntent::SelectAudioTrack)
      }
      Self::SelectSubtitleStream(None) => Some(PlaybackIntent::SelectSubtitleTrack(None)),
      Self::SelectSubtitleStream(Some(index)) => provider_track_id(playback, "sub", index)
        .map(|id| PlaybackIntent::SelectSubtitleTrack(Some(id))),
      Self::Stop => Some(PlaybackIntent::Stop),
      Self::PlayAdjacent(direction) => Some(PlaybackIntent::PlayAdjacent(direction)),
    }
  }

  const fn invalidates_remote_play(self) -> bool {
    matches!(self, Self::Stop | Self::PlayAdjacent(_))
  }
}

fn remote_command_action(
  command: JellyfinCommand,
  playback: &SessionView,
) -> Option<RemoteCommandAction> {
  match command {
    JellyfinCommand::Play(request) => remote_play_action(request),
    JellyfinCommand::Playstate(request) => remote_playstate_action(request),
    JellyfinCommand::GeneralCommand(request) => remote_general_action(
      request,
      playback.now_playing.as_ref().map(|playing| playing.muted),
    ),
  }
}

fn remote_play_action(request: PlayRequest) -> Option<RemoteCommandAction> {
  Some(RemoteCommandAction::Play {
    item_id: request.item_ids.first()?.clone(),
    start_position_ticks: request.start_position_ticks,
    selection: PlaybackSelection {
      media_source_id: request.media_source_id,
      audio_stream_index: request.audio_stream_index,
      subtitle_stream_index: request.subtitle_stream_index,
    },
  })
}

fn remote_playstate_action(request: PlaystateRequest) -> Option<RemoteCommandAction> {
  let intent = match request.command.as_str() {
    "Pause" => RemotePlaybackIntent::SetPaused(true),
    "Unpause" => RemotePlaybackIntent::SetPaused(false),
    "PlayPause" => RemotePlaybackIntent::TogglePaused,
    "Seek" => RemotePlaybackIntent::Seek(jellypilot_media_server::ticks_to_seconds(
      request.seek_position_ticks?,
    )),
    "Stop" => RemotePlaybackIntent::Stop,
    "NextTrack" => RemotePlaybackIntent::PlayAdjacent(AdjacentDirection::Next),
    "PreviousTrack" => RemotePlaybackIntent::PlayAdjacent(AdjacentDirection::Previous),
    _ => return None,
  };
  Some(RemoteCommandAction::Intent(intent))
}

fn remote_general_action(
  request: GeneralCommand,
  muted: Option<bool>,
) -> Option<RemoteCommandAction> {
  let arguments = request.arguments.as_ref();
  let intent = match request.name.as_str() {
    "SetVolume" => RemotePlaybackIntent::SetVolume(remote_volume_value(
      arguments.and_then(|arguments| arguments.get("Volume")),
    )?),
    "ToggleMute" => RemotePlaybackIntent::SetMuted(!muted?),
    "SetAudioStreamIndex" => RemotePlaybackIntent::SelectAudioStream(remote_index_value(
      arguments.and_then(|arguments| arguments.get("Index")),
    )?),
    "SetSubtitleStreamIndex" => {
      let index = remote_index_value(arguments.and_then(|arguments| arguments.get("Index")))?;
      RemotePlaybackIntent::SelectSubtitleStream((index >= 0).then_some(index))
    }
    _ => return None,
  };
  Some(RemoteCommandAction::Intent(intent))
}

fn handle_remote_command(
  state: &mut State,
  remote: jellypilot_core::request_gate::RemoteToken,
  command: JellyfinCommand,
) -> Task<Message> {
  match remote_command_action(command, &state.playback_view) {
    Some(RemoteCommandAction::Intent(intent)) => {
      if intent.invalidates_remote_play() {
        state.kernel.request_gate.begin_remote_play();
      }
      let Some(intent) = intent.into_playback_intent(&state.playback_view) else {
        state.kernel.notice = Some(REMOTE_TRACKS_UNAVAILABLE_NOTICE.to_owned());
        return Task::none();
      };
      apply_playback_input(state, PlaybackInput::Intent(intent))
    }
    Some(RemoteCommandAction::Play {
      item_id,
      start_position_ticks,
      selection,
    }) => {
      let play = state.kernel.request_gate.begin_remote_play();
      let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
        return Task::none();
      };
      Task::perform(
        async move {
          let item = client.playback().get_item(&item_id).await.map_err(|_| ())?;
          Ok(
            client
              .library()
              .item_detail(item_id)
              .await
              .map(Playable::Detail)
              .unwrap_or_else(|_| Playable::Media(item)),
          )
        },
        move |result| {
          Message::Remote(RemoteMessage::PlayResolved {
            remote,
            play,
            result: Box::new(result),
            start_position_ticks,
            selection,
          })
        },
      )
    }
    None => Task::none(),
  }
}
fn provider_track_id(playback: &SessionView, track_type: &str, provider_index: i64) -> Option<i64> {
  let TracksView::Ready { tracks, .. } = &playback.tracks else {
    return None;
  };
  tracks
    .iter()
    .find(|track| {
      track.track_type == track_type && track.provider_index.map(i64::from) == Some(provider_index)
    })
    .map(|track| track.id)
}

fn fail_remote_finalization(state: &mut State) -> Task<Message> {
  state.playback_remote = state.kernel.request_gate.begin_remote();
  state.remote_events = None;
  state.remote_control_state = RemoteControlState::Unavailable;
  state.kernel.notice =
    Some("Remote playback target capabilities could not be registered.".to_owned());
  let Some(session) = state.remote_session.take() else {
    return Task::none();
  };
  Task::perform(
    async move { disconnect_remote_session(session).await },
    |()| Message::Remote(RemoteMessage::RemoteDisconnected),
  )
}

async fn disconnect_remote_session(session: RemoteSessionHandle) {
  let lifecycle = Arc::clone(&session.lifecycle);
  let _lifecycle = lifecycle.lock().await;
  session.websocket.disconnect().await;
}

fn stop_remote_session_for_quit(state: &mut State) -> Task<Message> {
  state.playback_remote = state.kernel.request_gate.begin_remote();
  state.remote_events = None;
  state.remote_control_state = RemoteControlState::Unavailable;
  let Some(session) = state.remote_session.take() else {
    return Task::none();
  };
  state.remote_stopping = true;
  Task::perform(
    async move { disconnect_remote_session(session).await },
    |()| Message::Remote(RemoteMessage::QuitStopped),
  )
}

fn update_tray(state: &mut State, action: TrayAction) -> Task<Message> {
  match action {
    TrayAction::PlayPause => {
      apply_playback_input(state, PlaybackInput::Intent(PlaybackIntent::TogglePaused))
    }
    TrayAction::Next => {
      apply_local_playback_intent(state, PlaybackIntent::PlayAdjacent(AdjacentDirection::Next))
    }
    TrayAction::Previous => apply_local_playback_intent(
      state,
      PlaybackIntent::PlayAdjacent(AdjacentDirection::Previous),
    ),
    TrayAction::Mute => {
      let Some(muted) = state
        .playback_view
        .now_playing
        .as_ref()
        .map(|playing| playing.muted)
      else {
        return Task::none();
      };
      apply_playback_input(
        state,
        PlaybackInput::Intent(PlaybackIntent::SetMuted(!muted)),
      )
    }
    TrayAction::Show => {
      iced::window::oldest().map(|id| Message::Window(WindowMessage::ShowRequested(id)))
    }
    TrayAction::Quit => {
      if state.quit_requested {
        return Task::none();
      }
      state.quit_requested = true;
      sync_tray(state);
      Task::batch([
        apply_playback_input(state, PlaybackInput::Intent(PlaybackIntent::Quit)),
        stop_remote_session_for_quit(state),
      ])
    }
  }
}

fn initialize_playback(state: &mut State) {
  state.playback_session = Default::default();
  state.playback_view = state.playback_session.view();
  state.playback_notice = None;
  state.playback_playable = None;
  state.adjacent_playables = [None, None];
  clear_player_artwork(state);
  state.seek_preview = None;
  state.volume_preview = None;
  state.playback_remote = state.kernel.request_gate.begin_remote();
  state.in_flight_refresh = None;
  state.in_flight_command = None;

  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    state.playback_controller = None;
    return;
  };
  match PlaybackController::discover(
    client,
    playback_controller_config(state.settings.snapshot()),
  ) {
    Ok(controller) => {
      state.playback_controller = Some(Arc::new(tokio::sync::Mutex::new(controller)));
      let _ = state.playback_session.handle(
        PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
        Instant::now(),
      );
      state.playback_view = state.playback_session.view();
    }
    Err(_) => {
      state.playback_controller = None;
      state.playback_notice =
        Some("External playback is unavailable because MPV could not be found.".into());
    }
  }
  sync_tray(state);
}

fn playback_controller_config(settings: &Settings) -> PlaybackControllerConfig {
  let config = PlaybackControllerConfig::default().with_extra_args(configured_mpv_args(settings));
  match settings.mpv_path() {
    Some(path) => config.with_mpv_path(PathBuf::from(path)),
    None => config,
  }
}

fn apply_local_playback_intent(state: &mut State, intent: PlaybackIntent) -> Task<Message> {
  if matches!(
    &intent,
    PlaybackIntent::Start { .. } | PlaybackIntent::Stop | PlaybackIntent::PlayAdjacent(_)
  ) {
    state.kernel.request_gate.begin_remote_play();
  }
  apply_playback_input(state, PlaybackInput::Intent(intent))
}

fn update_playback(state: &mut State, message: PlaybackMessage) -> Task<Message> {
  match message {
    PlaybackMessage::Intent(intent) => apply_local_playback_intent(state, intent),
    PlaybackMessage::Event(event) => apply_playback_input(state, PlaybackInput::Event(*event)),
    PlaybackMessage::SeekChanged(position) => {
      state.seek_preview = seek_intent(
        position,
        state
          .playback_view
          .now_playing
          .as_ref()
          .and_then(|view| view.duration_seconds),
        state.playback_view.now_playing.is_some(),
      )
      .and_then(|intent| match intent {
        PlaybackIntent::Seek(position) => Some(position),
        _ => None,
      });
      Task::none()
    }
    PlaybackMessage::SeekReleased => {
      let Some(position) = state.seek_preview else {
        return Task::none();
      };
      let Some(intent) = seek_intent(
        position,
        state
          .playback_view
          .now_playing
          .as_ref()
          .and_then(|view| view.duration_seconds),
        state.playback_view.now_playing.is_some(),
      ) else {
        return Task::none();
      };
      apply_playback_input(state, PlaybackInput::Intent(intent))
    }
    PlaybackMessage::VolumeChanged(volume) => {
      state.volume_preview = volume_intent(volume, state.playback_view.now_playing.is_some())
        .and_then(|intent| match intent {
          PlaybackIntent::SetVolume(volume) => Some(volume),
          _ => None,
        });
      Task::none()
    }
    PlaybackMessage::VolumeReleased => {
      let Some(volume) = state.volume_preview else {
        return Task::none();
      };
      let Some(intent) = volume_intent(volume, state.playback_view.now_playing.is_some()) else {
        return Task::none();
      };
      apply_playback_input(state, PlaybackInput::Intent(intent))
    }
    PlaybackMessage::AudioMenuToggled => {
      state.audio_menu_open = !state.audio_menu_open;
      state.subtitle_menu_open = false;
      Task::none()
    }
    PlaybackMessage::AudioMenuDismissed => {
      state.audio_menu_open = false;
      Task::none()
    }
    PlaybackMessage::AudioTrackSelected(id) => {
      state.audio_menu_open = false;
      apply_local_playback_intent(state, PlaybackIntent::SelectAudioTrack(id))
    }
    PlaybackMessage::SubtitleMenuToggled => {
      state.subtitle_menu_open = !state.subtitle_menu_open;
      state.audio_menu_open = false;
      Task::none()
    }
    PlaybackMessage::SubtitleMenuDismissed => {
      state.subtitle_menu_open = false;
      Task::none()
    }
    PlaybackMessage::SubtitleTrackSelected(id) => {
      state.subtitle_menu_open = false;
      apply_local_playback_intent(state, PlaybackIntent::SelectSubtitleTrack(id))
    }
    PlaybackMessage::ControllerSettled {
      id,
      settlement,
      started,
      tracks,
    } => {
      if state.in_flight_refresh == Some(id) {
        state.in_flight_refresh = None;
      }
      if state.in_flight_command == Some(id) {
        state.in_flight_command = None;
      }
      let started = if matches!(settlement.as_ref(), ControllerSettlement::Started(Ok(_))) {
        started
      } else {
        None
      };
      let shutdown = matches!(settlement.as_ref(), ControllerSettlement::Shutdown(_));
      let previous_playable = state.playback_playable.clone();
      if let Some(playable) = started.as_deref() {
        state.playback_playable = Some(playable.clone());
        state.adjacent_playables = [None, None];
      }
      let mut tasks = vec![apply_playback_input(
        state,
        PlaybackInput::Event(PlaybackEvent::ControllerSettled {
          id,
          settlement: *settlement,
        }),
      )];
      let start_accepted = started.as_deref().is_some_and(|playable| {
        state
          .playback_view
          .now_playing
          .as_ref()
          .is_some_and(|view| view.item.item_id == playable_item_id(playable))
      });
      if started.is_some() && !start_accepted {
        state.playback_playable = previous_playable;
      }
      if let Some(result) = tracks {
        tasks.push(apply_playback_input(
          state,
          PlaybackInput::Event(PlaybackEvent::TracksSettled { id, result }),
        ));
      }
      if start_accepted {
        tasks.push(prepare_player_artwork(state));
      }
      if shutdown {
        state.playback_controller = None;
        let _ = state.playback_session.handle(
          PlaybackInput::Event(PlaybackEvent::EngineAvailability(false)),
          Instant::now(),
        );
        sync_playback_projection(state);
      }
      if !state.playback_view.busy {
        state.seek_preview = None;
        state.volume_preview = None;
      }
      tasks.push(clear_inactive_playback(state));
      Task::batch(tasks)
    }
    PlaybackMessage::AdjacentSettled {
      remote,
      play,
      id,
      direction,
      result,
      detail,
    } => {
      if remote != state.playback_remote
        || !state.kernel.request_gate.is_current_remote(remote)
        || !state.kernel.request_gate.is_current_remote_play(play)
      {
        return Task::none();
      }
      state.adjacent_playables[adjacent_index(direction)] =
        result.as_ref().ok().and_then(Option::as_ref).map(|item| {
          detail
            .map(|detail| Playable::Detail(*detail))
            .unwrap_or_else(|| Playable::Media(item.clone()))
        });
      apply_playback_input(
        state,
        PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
          id,
          direction,
          result,
        }),
      )
    }
    PlaybackMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = state.kernel.request_gate.is_current_session(session);
      if state
        .kernel
        .artwork_binder
        .settle(slot, ArtworkSurface::PlayerBar, session_ok)
        != ArtworkSettlement::Apply
      {
        return Task::none();
      }
      let Some(cell) = state
        .playback_artwork
        .as_mut()
        .filter(|cell| cell.slot == slot && cell.image_id == image_id)
      else {
        return Task::none();
      };
      match result {
        Ok(raster) => {
          cell.state = ArtworkCellState::Ready;
          state.kernel.artwork_handles.insert(
            slot,
            image_id,
            image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels()),
          );
        }
        Err(jellypilot_media_server::artwork::ArtworkError::Cancelled) => {}
        Err(_) => cell.state = ArtworkCellState::Failed,
      }
      Task::none()
    }
  }
}

fn seek_intent(position: f64, duration: Option<f64>, active: bool) -> Option<PlaybackIntent> {
  let duration = duration.filter(|duration| duration.is_finite() && *duration > 0.0)?;
  (active && position.is_finite()).then(|| PlaybackIntent::Seek(position.clamp(0.0, duration)))
}

fn volume_intent(volume: f64, active: bool) -> Option<PlaybackIntent> {
  (active && volume.is_finite()).then(|| PlaybackIntent::SetVolume(volume.clamp(0.0, 100.0)))
}

fn apply_playback_input(state: &mut State, input: PlaybackInput) -> Task<Message> {
  let effects = state.playback_session.handle(input, Instant::now());
  let task = execute_playback_effects(state, effects);
  sync_playback_projection(state);
  if quit_may_exit(state) {
    Task::batch([task, iced::exit()])
  } else {
    task
  }
}

fn quit_may_exit(state: &State) -> bool {
  state.quit_requested && state.playback_view.quit_may_proceed && !state.remote_stopping
}

fn sync_playback_projection(state: &mut State) {
  let mut view = state.playback_session.view();
  if view.busy && state.in_flight_refresh.is_some() && state.in_flight_command.is_none() {
    view.busy = false;
  }
  state.playback_view = view;
  state.playback_notice = state
    .playback_view
    .notice
    .as_ref()
    .map(|notice| match notice {
      PlaybackNotice::Failed(error) => error.to_string(),
      PlaybackNotice::Warnings(_) => {
        "Playback is active, but setup or reporting could not be completed.".to_owned()
      }
    });
  sync_tray(state);
}

fn sync_tray(state: &State) {
  if let Some(tray) = &state.kernel.tray {
    tray.sync(&state.playback_view, state.quit_requested);
  }
}

fn clear_player_artwork(state: &mut State) {
  if let Some(cell) = state.playback_artwork.take() {
    state.kernel.artwork_handles.remove(cell.slot);
  }
}

fn clear_inactive_playback(state: &mut State) -> Task<Message> {
  if state.playback_view.now_playing.is_some() {
    return Task::none();
  }
  state.playback_playable = None;
  state.adjacent_playables = [None, None];
  clear_player_artwork(state);
  state.seek_preview = None;
  state.volume_preview = None;
  state.audio_menu_open = false;
  state.subtitle_menu_open = false;
  Task::none()
}

fn execute_playback_effects(state: &mut State, effects: Vec<PlaybackEffect>) -> Task<Message> {
  let adjacent_play = effects
    .iter()
    .any(|effect| matches!(effect, PlaybackEffect::LookupAdjacent(_, _)))
    .then(|| state.kernel.request_gate.begin_remote_play());
  Task::batch(
    effects
      .into_iter()
      .map(|effect| execute_playback_effect(state, effect, adjacent_play)),
  )
}

fn execute_playback_effect(
  state: &mut State,
  effect: PlaybackEffect,
  adjacent_play: Option<RemotePlayToken>,
) -> Task<Message> {
  match effect {
    PlaybackEffect::Controller(id, command) => {
      match &command {
        ControllerCommand::Refresh => {
          state.in_flight_refresh = Some(id);
        }
        ControllerCommand::ShowText { .. } => {}
        _ => {
          state.in_flight_command = Some(id);
        }
      }
      execute_controller_command(state, id, command)
    }
    PlaybackEffect::LookupAdjacent(id, direction) => {
      let Some(play) = adjacent_play else {
        return Task::none();
      };
      let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
        return Task::done(Message::Playback(PlaybackMessage::AdjacentSettled {
          remote: state.playback_remote,
          play,
          id,
          direction,
          result: Err(()),
          detail: None,
        }));
      };
      let Some(playable) = state.playback_playable.as_ref() else {
        return Task::none();
      };
      let current = media_item_from_playable(playable);
      let remote = state.playback_remote;
      Task::perform(
        async move {
          let result = match direction {
            AdjacentDirection::Previous => client.playback().get_previous_episode(&current).await,
            AdjacentDirection::Next => client.playback().get_next_episode(&current).await,
          }
          .map_err(|_| ());
          let detail = match &result {
            Ok(Some(item)) => client.library().item_detail(item.id.clone()).await.ok(),
            Ok(None) | Err(()) => None,
          };
          (result, detail)
        },
        move |(result, detail)| {
          Message::Playback(PlaybackMessage::AdjacentSettled {
            remote,
            play,
            id,
            direction,
            result,
            detail: detail.map(Box::new),
          })
        },
      )
    }
    PlaybackEffect::FetchIntroRanges(id, item_id) => {
      let Some(client) = state
        .kernel
        .client
        .as_ref()
        .filter(|client| client.supports_intro_skipper())
        .map(Arc::clone)
      else {
        return Task::done(Message::Playback(PlaybackMessage::Event(Box::new(
          PlaybackEvent::IntroRangesSettled {
            id,
            result: Err(()),
          },
        ))));
      };
      Task::perform(
        async move {
          client
            .playback()
            .get_intro_skipper_ranges(&item_id)
            .await
            .map_err(|_| ())
        },
        move |result| {
          Message::Playback(PlaybackMessage::Event(Box::new(
            PlaybackEvent::IntroRangesSettled { id, result },
          )))
        },
      )
    }
  }
}

fn execute_controller_command(
  state: &State,
  id: jellypilot_mpv::playback_session::EffectId,
  command: ControllerCommand,
) -> Task<Message> {
  let started = match &command {
    ControllerCommand::Start { item, .. } => Some(rich_playable(state, item)),
    _ => None,
  };
  let Some(controller) = state.playback_controller.as_ref().map(Arc::clone) else {
    let settlement = missing_controller_settlement(&command);
    return Task::done(Message::Playback(PlaybackMessage::ControllerSettled {
      id,
      settlement: Box::new(settlement),
      started: started.map(Box::new),
      tracks: None,
    }));
  };
  Task::perform(
    async move {
      let mut controller = controller.lock().await;
      match command {
        ControllerCommand::Start {
          item,
          position,
          selection,
        } => {
          let result = controller.play_selected(item, position, selection).await;
          let tracks = if result.is_ok() {
            Some(controller.tracks().await)
          } else {
            None
          };
          (ControllerSettlement::Started(result), tracks)
        }
        ControllerCommand::SetPaused(paused) => (
          ControllerSettlement::Controlled(controller.set_paused(paused).await),
          None,
        ),
        ControllerCommand::Seek(position) => (
          ControllerSettlement::Controlled(controller.seek(position).await),
          None,
        ),
        ControllerCommand::SetVolume(volume) => (
          ControllerSettlement::Controlled(controller.set_volume(volume).await),
          None,
        ),
        ControllerCommand::SetMuted(muted) => (
          ControllerSettlement::Controlled(controller.set_muted(muted).await),
          None,
        ),
        ControllerCommand::SelectAudioTrack(id) => (
          ControllerSettlement::TrackSelected(controller.select_audio_track(id).await),
          None,
        ),
        ControllerCommand::SelectSubtitleTrack(id) => (
          ControllerSettlement::TrackSelected(controller.select_subtitle_track(id).await),
          None,
        ),
        ControllerCommand::ShowText { text, duration_ms } => (
          ControllerSettlement::OsdShown(controller.show_text(&text, duration_ms).await),
          None,
        ),
        ControllerCommand::Stop => (ControllerSettlement::Stopped(controller.stop().await), None),
        ControllerCommand::Refresh => {
          let outcome = controller.refresh().await;
          let client_messages = controller.take_client_messages();
          (
            ControllerSettlement::Refreshed {
              outcome,
              client_messages,
            },
            None,
          )
        }
        ControllerCommand::Shutdown => {
          let outcome = controller.shutdown().await;
          (ControllerSettlement::Shutdown(outcome.warnings), None)
        }
      }
    },
    move |(settlement, tracks)| {
      Message::Playback(PlaybackMessage::ControllerSettled {
        id,
        settlement: Box::new(settlement),
        started: started.map(Box::new),
        tracks,
      })
    },
  )
}

fn missing_controller_settlement(command: &ControllerCommand) -> ControllerSettlement {
  match command {
    ControllerCommand::Start { .. } => {
      ControllerSettlement::Started(Err(PlaybackError::MpvNotFound))
    }
    ControllerCommand::Stop => ControllerSettlement::Stopped(Err(PlaybackError::NoActivePlayback)),
    ControllerCommand::SelectAudioTrack(_) | ControllerCommand::SelectSubtitleTrack(_) => {
      ControllerSettlement::TrackSelected(Err(PlaybackError::NoActivePlayback))
    }
    ControllerCommand::ShowText { .. } => {
      ControllerSettlement::OsdShown(Err(PlaybackError::NoActivePlayback))
    }
    ControllerCommand::Shutdown => ControllerSettlement::Shutdown(Vec::new()),
    ControllerCommand::Refresh => ControllerSettlement::Refreshed {
      outcome: PlaybackRefreshOutcome {
        snapshot: PlaybackSnapshot {
          now_playing: None,
          transport: Default::default(),
        },
        state: PlaybackRefreshState::Ended(PlaybackEndReason::Disconnected),
        warnings: Vec::new(),
      },
      client_messages: Vec::new(),
    },
    ControllerCommand::SetPaused(_)
    | ControllerCommand::Seek(_)
    | ControllerCommand::SetVolume(_)
    | ControllerCommand::SetMuted(_) => {
      ControllerSettlement::Controlled(Err(PlaybackError::NoActivePlayback))
    }
  }
}

fn rich_playable(state: &State, item: &Playable) -> Playable {
  let Playable::Media(media) = item else {
    return item.clone();
  };
  state
    .adjacent_playables
    .iter()
    .flatten()
    .find(|playable| playable_item_id(playable) == media.id)
    .cloned()
    .unwrap_or_else(|| item.clone())
}

fn playable_item_id(playable: &Playable) -> &str {
  match playable {
    Playable::Library(item) => &item.id,
    Playable::Detail(item) => &item.id,
    Playable::Media(item) => &item.id,
  }
}

fn adjacent_index(direction: AdjacentDirection) -> usize {
  match direction {
    AdjacentDirection::Previous => 0,
    AdjacentDirection::Next => 1,
  }
}

fn prepare_player_artwork(state: &mut State) -> Task<Message> {
  let image_id = state
    .playback_playable
    .as_ref()
    .and_then(playback_image_id)
    .map(str::to_owned);
  let Some(image_id) = image_id else {
    clear_player_artwork(state);
    return Task::none();
  };
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  if let Some(cell) = &state.playback_artwork {
    if cell.image_id == image_id {
      if cell.state == ArtworkCellState::Loading {
        return Task::none();
      }
      if cell.state == ArtworkCellState::Ready
        && state
          .kernel
          .artwork_handles
          .get(cell.slot, &cell.image_id)
          .is_some()
      {
        return Task::none();
      }
    }
  }
  clear_player_artwork(state);
  if let Some(raster) = state
    .kernel
    .artwork_adapter
    .cached(&image_id, ArtworkSizeClass::Card)
  {
    let slot = state.kernel.artwork_binder.bind_settled();
    let handle = image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels());
    state
      .kernel
      .artwork_handles
      .insert(slot, image_id.clone(), handle);
    state.playback_artwork = Some(ArtworkCell {
      slot,
      image_id,
      state: ArtworkCellState::Ready,
    });
    return Task::none();
  }
  let slot = state.kernel.artwork_binder.bind_player_bar();
  state.playback_artwork = Some(ArtworkCell {
    slot,
    image_id: image_id.clone(),
    state: ArtworkCellState::Loading,
  });
  let adapter = Arc::clone(&state.kernel.artwork_adapter);
  let session = state.kernel.request_gate.current_session();
  let completion_image_id = image_id.clone();
  Task::perform(
    async move {
      adapter
        .load(
          &client,
          &image_id,
          ArtworkSizeClass::Card,
          LoadLane::Visible,
        )
        .await
        .0
    },
    move |result| {
      Message::Playback(PlaybackMessage::ArtworkLoaded {
        session,
        slot,
        image_id: completion_image_id,
        result,
      })
    },
  )
}

fn playback_image_id(playable: &Playable) -> Option<&str> {
  match playable {
    Playable::Library(item) => item
      .series_poster_image_id
      .as_deref()
      .or(item.artwork_image_id.as_deref()),
    Playable::Detail(item) => item
      .series_poster_image_id
      .as_deref()
      .or(item.artwork_image_id.as_deref()),
    Playable::Media(_) => None,
  }
}

fn update_home(state: &mut State, message: HomeMessage) -> Task<Message> {
  match message {
    HomeMessage::Navigate(destination) => navigate(state, destination),
    HomeMessage::Retry => start_home_load(state),
    HomeMessage::Loaded { token, result } => {
      if !settle_home(
        &mut state.home,
        &mut state.kernel.request_gate,
        token,
        result,
      ) {
        return Task::none();
      }
      prepare_home_artwork(state)
    }
    HomeMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = state.kernel.request_gate.is_current_session(session);
      apply_home_artwork_completion(
        state,
        session_ok,
        ArtworkLoadCompletion {
          slot,
          image_id,
          result,
        },
      );
      Task::none()
    }
  }
}

fn apply_home_artwork_completion(
  state: &mut State,
  session_ok: bool,
  completion: ArtworkLoadCompletion,
) {
  if state
    .kernel
    .artwork_binder
    .settle(completion.slot, ArtworkSurface::Home, session_ok)
    != ArtworkSettlement::Apply
  {
    return;
  }
  let Some(cell) = state
    .home_artwork
    .cell_mut(completion.slot, &completion.image_id)
  else {
    return;
  };
  match completion.result {
    Ok(raster) => {
      cell.state = ArtworkCellState::Ready;
      state.kernel.artwork_handles.insert(
        completion.slot,
        completion.image_id,
        image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels()),
      );
    }
    Err(jellypilot_media_server::artwork::ArtworkError::Cancelled) => {}
    Err(_) => cell.state = ArtworkCellState::Failed,
  }
}

fn start_home_load(state: &mut State) -> Task<Message> {
  if !state.home.has_ready_content() {
    state.home.begin_load();
  }
  if state.playback_view.now_playing.is_none() {
    state.kernel.artwork_adapter.cancel_pending();
    state.home_artwork.prune_unready();
  }
  let token = state.kernel.request_gate.begin_home();
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    let error = "The connected media server session is unavailable.".to_owned();
    state.home.settle_video_home(Err(error.clone()));
    state.home.settle_shortcuts(Err(error));
    return Task::none();
  };

  Task::perform(load_home_data(client), move |result| {
    Message::Home(HomeMessage::Loaded { token, result })
  })
}

fn settle_home(
  home: &mut HomeState,
  request_gate: &mut RequestGate,
  token: HomeToken,
  result: HomeDataResult,
) -> bool {
  if !request_gate.finish_home(token) {
    return false;
  }
  let (video_home, shortcuts) = result;
  home.settle_video_home(video_home);
  home.settle_shortcuts(shortcuts);
  true
}

#[derive(Clone, Copy)]
enum ArtworkPlacement {
  Hero,
  Card(HomeSection),
}

struct ArtworkLoadSpec {
  placement: ArtworkPlacement,
  item_id: String,
  image_id: String,
  visible: bool,
}

impl ArtworkLoadSpec {
  fn size_class(&self) -> ArtworkSizeClass {
    match self.placement {
      ArtworkPlacement::Hero => ArtworkSizeClass::Hero,
      ArtworkPlacement::Card(_) => ArtworkSizeClass::Card,
    }
  }
}

enum ArtworkStreamEvent {
  Loaded(ArtworkLoadCompletion),
  Completed(ArtworkLoadSummary),
}

/// Streams a surface's Library Image loads visible-first, emitting one message
/// per image as it settles and `Message::ArtworkStreamCompleted` with this
/// stream's own sanitized aggregate at the end. `summary` seeds the aggregate
/// with synchronous cache hits from the prepare pass.
fn stream_artwork_loads<F>(
  adapter: Arc<jellypilot_media_server::artwork::ArtworkAdapter>,
  client: Arc<JellyfinClient>,
  session: jellypilot_core::request_gate::SessionToken,
  loads: Vec<PlannedArtworkLoad>,
  summary: ArtworkLoadSummary,
  make_message: F,
) -> Task<Message>
where
  F: Fn(jellypilot_core::request_gate::SessionToken, ArtworkLoadCompletion) -> Message
    + Send
    + Sync
    + 'static,
{
  if loads.is_empty() && summary == ArtworkLoadSummary::default() {
    return Task::none();
  }
  let summary = Arc::new(Mutex::new(summary));
  let planned = plan_artwork_loads(loads);
  let concurrency = planned.len();
  let completions = stream::iter(planned).map({
    let summary = Arc::clone(&summary);
    move |load| {
      let adapter = Arc::clone(&adapter);
      let client = Arc::clone(&client);
      let summary = Arc::clone(&summary);
      async move {
        let lane = if load.visible {
          LoadLane::Visible
        } else {
          LoadLane::Offscreen
        };
        let image_id = load.image_id;
        let (result, observation) = adapter
          .load(&client, &image_id, load.size_class, lane)
          .await;
        if let Ok(mut summary) = summary.lock() {
          summary.record(&observation);
        }
        ArtworkLoadCompletion {
          slot: load.slot,
          image_id,
          result,
        }
      }
    }
  });
  let events = completions
    .buffer_unordered(concurrency)
    .map(ArtworkStreamEvent::Loaded)
    .chain(stream::once(async move {
      let summary = summary.lock().map(|summary| *summary).unwrap_or_default();
      ArtworkStreamEvent::Completed(summary)
    }));
  Task::run(events, move |event| match event {
    ArtworkStreamEvent::Loaded(completion) => make_message(session, completion),
    ArtworkStreamEvent::Completed(summary) => Message::ArtworkStreamCompleted(summary),
  })
}

fn prepare_home_artwork(state: &mut State) -> Task<Message> {
  if !state.home.has_ready_content() {
    return Task::none();
  }
  let specs = home_artwork_specs(state);
  let hero_item_id = specs
    .iter()
    .find(|spec| matches!(spec.placement, ArtworkPlacement::Hero))
    .map(|spec| spec.item_id.as_str());
  let mut section_item_ids: [HashSet<&str>; 4] = Default::default();
  for spec in &specs {
    if let ArtworkPlacement::Card(section) = spec.placement {
      section_item_ids[section.index()].insert(spec.item_id.as_str());
    }
  }
  state
    .home_artwork
    .retain_items(hero_item_id, &section_item_ids);

  let session = state.kernel.request_gate.current_session();
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    state.retain_artwork_handles();
    return Task::none();
  };
  let adapter = Arc::clone(&state.kernel.artwork_adapter);
  let mut summary = ArtworkLoadSummary::default();
  let mut load_specs = Vec::new();

  for spec in specs {
    let existing_cell = match spec.placement {
      ArtworkPlacement::Hero => state.home_artwork.hero(&spec.item_id),
      ArtworkPlacement::Card(section) => state.home_artwork.card(section, &spec.item_id),
    };
    if let Some(cell) = existing_cell {
      if cell.image_id == spec.image_id {
        if cell.state == ArtworkCellState::Loading {
          continue;
        }
        if cell.state == ArtworkCellState::Ready
          && state
            .kernel
            .artwork_handles
            .get(cell.slot, &cell.image_id)
            .is_some()
        {
          continue;
        }
      }
    }

    if let Some(raster) = adapter.cached(&spec.image_id, spec.size_class()) {
      summary.record(&ArtworkLoadObservation::raster_hit(raster.byte_len() as u64));
      let slot = state.kernel.artwork_binder.bind_settled();
      let handle = image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels());
      state
        .kernel
        .artwork_handles
        .insert(slot, spec.image_id.clone(), handle);
      let cell = ArtworkCell {
        slot,
        image_id: spec.image_id,
        state: ArtworkCellState::Ready,
      };
      match spec.placement {
        ArtworkPlacement::Hero => state.home_artwork.insert_hero(spec.item_id, cell),
        ArtworkPlacement::Card(section) => {
          state.home_artwork.insert_card(section, spec.item_id, cell);
        }
      }
      continue;
    }

    let slot = state.kernel.artwork_binder.bind(ArtworkSurface::Home);
    let size_class = spec.size_class();
    let cell = ArtworkCell {
      slot,
      image_id: spec.image_id.clone(),
      state: ArtworkCellState::Loading,
    };
    match spec.placement {
      ArtworkPlacement::Hero => state.home_artwork.insert_hero(spec.item_id, cell),
      ArtworkPlacement::Card(section) => {
        state.home_artwork.insert_card(section, spec.item_id, cell);
      }
    }
    load_specs.push(PlannedArtworkLoad {
      slot,
      image_id: spec.image_id,
      size_class,
      visible: spec.visible,
    });
  }

  state.retain_artwork_handles();
  stream_artwork_loads(
    adapter,
    client,
    session,
    load_specs,
    summary,
    |session, completion| {
      Message::Home(HomeMessage::ArtworkLoaded {
        session,
        slot: completion.slot,
        image_id: completion.image_id,
        result: completion.result,
      })
    },
  )
}

fn home_artwork_specs(state: &State) -> Vec<ArtworkLoadSpec> {
  let mut specs = Vec::new();
  if let Some(item) = state.home.featured_item() {
    push_artwork_spec(&mut specs, ArtworkPlacement::Hero, item, true);
  }
  let class = SizeClass::from_width(state.window_size.width);
  let content_width = content_width(state.window_size.width, class);
  for section in HomeSection::ALL {
    if let jellypilot_core::LoadState::Ready(items) = state.home.section(section) {
      let (card_width, _) = section_frame_size(section);
      let visible_cards = visible_row_cards(content_width, card_width, TOKENS.spacing.s4);
      for (index, item) in items.iter().enumerate() {
        push_artwork_spec(
          &mut specs,
          ArtworkPlacement::Card(section),
          item,
          index < visible_cards,
        );
      }
    }
  }
  specs
}

fn push_artwork_spec(
  specs: &mut Vec<ArtworkLoadSpec>,
  placement: ArtworkPlacement,
  item: &VideoLibraryItem,
  visible: bool,
) {
  if let Some(image_id) = &item.artwork_image_id {
    specs.push(ArtworkLoadSpec {
      placement,
      item_id: item.id.clone(),
      image_id: image_id.clone(),
      visible,
    });
  }
}

fn leave_home_view(state: &mut State) {
  state.kernel.request_gate.begin_home();
  if state.playback_view.now_playing.is_none() {
    state.kernel.artwork_adapter.cancel_pending();
    state.home_artwork.prune_unready();
  }
}

fn navigate(state: &mut State, destination: Destination) -> Task<Message> {
  let previous = state.destination.clone();
  if !state.navigate_to(destination) {
    return Task::none();
  }
  activate_destination(state, previous)
}

fn open_detail(state: &mut State, item: VideoLibraryItem) -> Task<Message> {
  let item_id = item.id.clone();
  state.detail_items.insert(item_id.clone(), item);
  navigate(state, Destination::Detail(item_id))
}

fn navigate_back(state: &mut State) -> Task<Message> {
  let previous = state.destination.clone();
  if !state.navigate_back() {
    return Task::none();
  }
  activate_destination(state, previous)
}

fn activate_destination(state: &mut State, previous: Destination) -> Task<Message> {
  if previous == Destination::Settings && state.destination != Destination::Settings {
    state.settings_view.shortcut_capture = None;
  }
  if previous == Destination::Home && state.destination != Destination::Home {
    leave_home_view(state);
  } else if matches!(
    previous,
    Destination::Library { .. } | Destination::Search(_)
  ) && !matches!(
    state.destination,
    Destination::Library { .. } | Destination::Search(_)
  ) {
    leave_browse_view(state);
  } else if matches!(previous, Destination::Detail(_)) && previous != state.destination {
    leave_detail_view(state);
  }

  match &state.destination {
    Destination::Home => start_home_load(state),
    Destination::Library { .. } => {
      state.search_input.clear();
      start_browse(state)
    }
    Destination::Search(_) => start_browse(state),
    Destination::Detail(_) => start_detail_load(state),
    Destination::Settings => Task::none(),
  }
}

const DETAIL_FAILURE: &str = "Could not load this item. Try again.";
const SEASON_FAILURE: &str = "Could not load this season. Try again.";
const USER_DATA_FAILURE: &str = "Could not update user data. Try again.";

fn update_detail(state: &mut State, message: DetailMessage) -> Task<Message> {
  match message {
    DetailMessage::Back => navigate_back(state),
    DetailMessage::Retry => start_detail_load(state),
    DetailMessage::RetryNeighbors => start_detail_followup(state),
    DetailMessage::RetrySeason => start_selected_season_load(state),
    DetailMessage::OverviewToggled => {
      state.detail.overview_expanded = !state.detail.overview_expanded;
      Task::none()
    }
    DetailMessage::SeasonSelected(season_id) => {
      if !select_season(&mut state.detail, &season_id) {
        return Task::none();
      }
      start_selected_season_load(state)
    }
    DetailMessage::FavoriteToggled => start_user_data_update(state, UserDataActionKind::Favorite),
    DetailMessage::PlayedToggled => start_user_data_update(state, UserDataActionKind::Played),
    DetailMessage::Loaded { token, result } => {
      if !settle_detail_load(
        &mut state.detail,
        &mut state.kernel.request_gate,
        token,
        *result,
      ) {
        return Task::none();
      }
      let followup = start_detail_followup(state);
      Task::batch([followup, prepare_detail_artwork(state)])
    }
    DetailMessage::SeasonLoaded { token, result } => {
      if !settle_season_load(
        &mut state.detail,
        &mut state.kernel.request_gate,
        token,
        result,
      ) {
        return Task::none();
      }
      prepare_detail_artwork(state)
    }
    DetailMessage::NeighborsLoaded { token, result } => {
      if !state.kernel.request_gate.finish_detail_aux(token) {
        return Task::none();
      }
      state.detail.season_neighbors = match result {
        Ok(items) => jellypilot_core::LoadState::Ready(items),
        Err(_) => jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned()),
      };
      prepare_detail_artwork(state)
    }
    DetailMessage::UserDataUpdated { token, result } => {
      let Some(update) = settle_user_data_update(
        &mut state.detail,
        &mut state.kernel.request_gate,
        token,
        result,
      ) else {
        return Task::none();
      };
      if let Some(update) = update {
        if let Some(item) = state.detail_items.get_mut(&update.item_id) {
          item.played = update.played;
          item.favorite = update.favorite;
        }
      }
      Task::none()
    }
    DetailMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = state.kernel.request_gate.is_current_session(session);
      apply_detail_artwork_completion(
        state,
        session_ok,
        ArtworkLoadCompletion {
          slot,
          image_id,
          result,
        },
      );
      Task::none()
    }
  }
}

fn apply_detail_artwork_completion(
  state: &mut State,
  session_ok: bool,
  completion: ArtworkLoadCompletion,
) {
  if state
    .kernel
    .artwork_binder
    .settle(completion.slot, ArtworkSurface::Detail, session_ok)
    != ArtworkSettlement::Apply
  {
    return;
  }
  let Some(cell) = state
    .detail_artwork
    .cell_mut(completion.slot, &completion.image_id)
  else {
    return;
  };
  match completion.result {
    Ok(raster) => {
      cell.state = ArtworkCellState::Ready;
      state.kernel.artwork_handles.insert(
        completion.slot,
        completion.image_id,
        image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels()),
      );
    }
    Err(jellypilot_media_server::artwork::ArtworkError::Cancelled) => {}
    Err(_) => cell.state = ArtworkCellState::Failed,
  }
}

fn start_detail_load(state: &mut State) -> Task<Message> {
  let Destination::Detail(item_id) = &state.destination else {
    return Task::none();
  };
  let item_id = item_id.clone();
  let Some(item) = state.detail_items.get(&item_id).cloned() else {
    state.detail.content = jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned());
    return Task::none();
  };
  state.detail.clear();
  begin_detail_artwork_view(state);
  state.kernel.request_gate.set_detail_item(Some(item_id));
  let token = state.kernel.request_gate.begin_detail();
  state.detail.content = jellypilot_core::LoadState::Loading;
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    state.detail.content = jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned());
    return Task::none();
  };

  Task::perform(
    async move {
      load_detail_content(client, item)
        .await
        .map_err(|_| DETAIL_FAILURE.to_owned())
    },
    move |result| {
      Message::Detail(DetailMessage::Loaded {
        token,
        result: Box::new(result),
      })
    },
  )
}

fn settle_detail_load(
  detail: &mut DetailState,
  gate: &mut RequestGate,
  token: DetailToken,
  result: Result<DetailContent, String>,
) -> bool {
  if !gate.finish_detail(token) {
    return false;
  }
  detail.content = match result {
    Ok(content) => jellypilot_core::LoadState::Ready(content),
    Err(_) => jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned()),
  };
  true
}

fn start_detail_followup(state: &mut State) -> Task<Message> {
  match &state.detail.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      let request = match (
        item.series_id.as_ref(),
        item.season_number,
        item.item_type.eq_ignore_ascii_case("episode"),
      ) {
        (Some(series_id), Some(season_number), true) => {
          Some((item.id.clone(), series_id.clone(), season_number))
        }
        _ => None,
      };
      let Some((item_id, series_id, season_number)) = request else {
        state.detail.season_neighbors = jellypilot_core::LoadState::Idle;
        return Task::none();
      };
      let Some(token) = state
        .kernel
        .request_gate
        .begin_detail_aux(DetailAuxKind::SeasonNeighbors)
      else {
        return Task::none();
      };
      state.detail.season_neighbors = jellypilot_core::LoadState::Loading;
      let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
        state.detail.season_neighbors =
          jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned());
        return Task::none();
      };
      Task::perform(
        async move {
          load_season_neighbors(client, item_id, series_id, season_number)
            .await
            .map_err(|_| SEASON_FAILURE.to_owned())
        },
        move |result| Message::Detail(DetailMessage::NeighborsLoaded { token, result }),
      )
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      state.detail.selected_season_id = initial_season(show).map(|season| season.id.clone());
      start_selected_season_load(state)
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => Task::none(),
  }
}

fn initial_season(show: &jellypilot_media_server::VideoShowDetail) -> Option<&VideoSeason> {
  show
    .next_episode
    .as_ref()
    .and_then(|episode| episode.season_number)
    .and_then(|season_number| {
      show
        .seasons
        .iter()
        .find(|season| season.season_number == Some(season_number))
    })
    .or_else(|| show.seasons.first())
}

fn select_season(detail: &mut DetailState, season_id: &str) -> bool {
  let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &detail.content else {
    return false;
  };
  if detail.selected_season_id.as_deref() == Some(season_id)
    || !show.seasons.iter().any(|season| season.id == season_id)
  {
    return false;
  }
  detail.selected_season_id = Some(season_id.to_owned());
  true
}

fn selected_season_request(detail: &DetailState) -> Option<VideoSeasonEpisodesPageRequest> {
  let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &detail.content else {
    return None;
  };
  let selected_id = detail.selected_season_id.as_deref()?;
  let season = show
    .seasons
    .iter()
    .find(|season| season.id == selected_id)?;
  Some(season_page_request(&show.id, season, 0))
}

fn start_selected_season_load(state: &mut State) -> Task<Message> {
  let Some(request) = selected_season_request(&state.detail) else {
    state.detail.season_episodes = jellypilot_core::LoadState::Idle;
    return Task::none();
  };
  let token = state.kernel.request_gate.begin_detail();
  state.detail.season_episodes = jellypilot_core::LoadState::Loading;
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    state.detail.season_episodes = jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned());
    return Task::none();
  };
  Task::perform(
    async move {
      client
        .library()
        .season_episodes_page(request)
        .await
        .map_err(|_| SEASON_FAILURE.to_owned())
    },
    move |result| Message::Detail(DetailMessage::SeasonLoaded { token, result }),
  )
}

fn settle_season_load(
  detail: &mut DetailState,
  gate: &mut RequestGate,
  token: DetailToken,
  result: Result<VideoSeasonEpisodesPage, String>,
) -> bool {
  if !gate.finish_detail(token) {
    return false;
  }
  detail.season_episodes = match result {
    Ok(page) => jellypilot_core::LoadState::Ready(page),
    Err(_) => jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned()),
  };
  true
}

fn start_user_data_update(state: &mut State, kind: UserDataActionKind) -> Task<Message> {
  if state.detail.user_data_busy.is_some() {
    return Task::none();
  }
  let Some((item_id, played, favorite)) = detail_user_data(&state.detail.content) else {
    return Task::none();
  };
  let action = match kind {
    UserDataActionKind::Favorite if favorite => VideoUserDataAction::Unfavorite,
    UserDataActionKind::Favorite => VideoUserDataAction::Favorite,
    UserDataActionKind::Played if played => VideoUserDataAction::MarkUnplayed,
    UserDataActionKind::Played => VideoUserDataAction::MarkPlayed,
  };
  let Some(token) = state
    .kernel
    .request_gate
    .begin_detail_aux(DetailAuxKind::UserData)
  else {
    return Task::none();
  };
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    state.detail.user_data_error = Some(USER_DATA_FAILURE.to_owned());
    return Task::none();
  };
  state.detail.user_data_busy = Some(kind);
  state.detail.user_data_error = None;
  let request = VideoUserDataUpdateRequest { item_id, action };
  Task::perform(
    async move {
      client
        .library()
        .update_user_data(request)
        .await
        .map_err(|_| USER_DATA_FAILURE.to_owned())
    },
    move |result| Message::Detail(DetailMessage::UserDataUpdated { token, result }),
  )
}

fn detail_user_data(
  detail: &jellypilot_core::LoadState<DetailContent>,
) -> Option<(String, bool, bool)> {
  match detail {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      Some((item.id.clone(), item.played, item.favorite))
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      Some((show.id.clone(), show.played, show.favorite))
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => None,
  }
}

fn settle_user_data_update(
  detail: &mut DetailState,
  gate: &mut RequestGate,
  token: jellypilot_core::request_gate::DetailAuxToken,
  result: Result<VideoUserDataUpdate, String>,
) -> Option<Option<VideoUserDataUpdate>> {
  if !gate.finish_detail_aux(token) {
    return None;
  }
  detail.user_data_busy = None;
  match result {
    Ok(update) if apply_user_data_update(&mut detail.content, &update) => {
      detail.user_data_error = None;
      Some(Some(update))
    }
    Ok(_) | Err(_) => {
      detail.user_data_error = Some(USER_DATA_FAILURE.to_owned());
      Some(None)
    }
  }
}

const DETAIL_POSTER_KEY: &str = "detail-poster";
const DETAIL_BACKDROP_KEY: &str = "detail-backdrop";

struct DetailArtworkSpec {
  key: String,
  image_id: String,
  size_class: ArtworkSizeClass,
  visible: bool,
}

fn prepare_detail_artwork(state: &mut State) -> Task<Message> {
  let mut specs = Vec::new();
  match &state.detail.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      push_detail_artwork(
        &mut specs,
        DETAIL_POSTER_KEY.to_owned(),
        &item.artwork_image_id,
        ArtworkSizeClass::Hero,
        true,
      );
      push_detail_artwork(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &item.backdrop_image_id,
        ArtworkSizeClass::Backdrop,
        true,
      );
      if let jellypilot_core::LoadState::Ready(neighbors) = &state.detail.season_neighbors {
        for episode in neighbors {
          push_detail_artwork(
            &mut specs,
            detail_episode_key(&episode.id),
            &episode.artwork_image_id,
            ArtworkSizeClass::Card,
            false,
          );
        }
      }
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      push_detail_artwork(
        &mut specs,
        DETAIL_POSTER_KEY.to_owned(),
        &show.artwork_image_id,
        ArtworkSizeClass::Hero,
        true,
      );
      push_detail_artwork(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &show.backdrop_image_id,
        ArtworkSizeClass::Backdrop,
        true,
      );
      if let Some(next) = &show.next_episode {
        push_detail_artwork(
          &mut specs,
          detail_episode_key(&next.id),
          &next.artwork_image_id,
          ArtworkSizeClass::Card,
          false,
        );
      }
      if let jellypilot_core::LoadState::Ready(page) = &state.detail.season_episodes {
        for episode in &page.episodes {
          push_detail_artwork(
            &mut specs,
            detail_episode_key(&episode.id),
            &episode.artwork_image_id,
            ArtworkSizeClass::Card,
            false,
          );
        }
      }
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => return Task::none(),
  }

  let retained_keys = specs
    .iter()
    .map(|spec| spec.key.as_str())
    .collect::<HashSet<_>>();
  state.detail_artwork.retain_keys(&retained_keys);
  drop(retained_keys);
  let session = state.kernel.request_gate.current_session();
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    state.retain_artwork_handles();
    return Task::none();
  };
  let adapter = Arc::clone(&state.kernel.artwork_adapter);
  let mut summary = ArtworkLoadSummary::default();
  let mut load_specs = Vec::new();
  for spec in specs {
    if let Some(cell) = state.detail_artwork.get(&spec.key) {
      if cell.image_id == spec.image_id {
        if cell.state == ArtworkCellState::Loading {
          continue;
        }
        if cell.state == ArtworkCellState::Ready
          && state
            .kernel
            .artwork_handles
            .get(cell.slot, &cell.image_id)
            .is_some()
        {
          continue;
        }
      }
    }

    if let Some(raster) = adapter.cached(&spec.image_id, spec.size_class) {
      summary.record(&ArtworkLoadObservation::raster_hit(raster.byte_len() as u64));
      let slot = state.kernel.artwork_binder.bind_settled();
      let handle = image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels());
      state
        .kernel
        .artwork_handles
        .insert(slot, spec.image_id.clone(), handle);
      state.detail_artwork.insert(
        spec.key,
        ArtworkCell {
          slot,
          image_id: spec.image_id,
          state: ArtworkCellState::Ready,
        },
      );
      continue;
    }

    let slot = state.kernel.artwork_binder.bind(ArtworkSurface::Detail);
    state.detail_artwork.insert(
      spec.key,
      ArtworkCell {
        slot,
        image_id: spec.image_id.clone(),
        state: ArtworkCellState::Loading,
      },
    );
    load_specs.push(PlannedArtworkLoad {
      slot,
      image_id: spec.image_id,
      size_class: spec.size_class,
      visible: spec.visible,
    });
  }
  state.retain_artwork_handles();
  stream_artwork_loads(
    adapter,
    client,
    session,
    load_specs,
    summary,
    |session, completion| {
      Message::Detail(DetailMessage::ArtworkLoaded {
        session,
        slot: completion.slot,
        image_id: completion.image_id,
        result: completion.result,
      })
    },
  )
}

fn push_detail_artwork(
  specs: &mut Vec<DetailArtworkSpec>,
  key: String,
  image_id: &Option<String>,
  size_class: ArtworkSizeClass,
  visible: bool,
) {
  if let Some(image_id) = image_id {
    specs.push(DetailArtworkSpec {
      key,
      image_id: image_id.clone(),
      size_class,
      visible,
    });
  }
}

fn detail_episode_key(item_id: &str) -> String {
  format!("detail-episode:{item_id}")
}

fn begin_detail_artwork_view(state: &mut State) {
  state
    .kernel
    .artwork_binder
    .begin_view(ArtworkSurface::Detail);
  state.detail_artwork.clear();
}

fn leave_detail_view(state: &mut State) {
  state.kernel.request_gate.navigate();
  if state.playback_view.now_playing.is_none() {
    state.kernel.artwork_adapter.cancel_pending();
  }
  begin_detail_artwork_view(state);
  state.detail.clear();
}

fn update_browse(state: &mut State, message: BrowseMessage) -> Task<Message> {
  match message {
    BrowseMessage::SearchInputChanged(value) => {
      state.search_input = value;
      Task::none()
    }
    BrowseMessage::SearchSubmitted => {
      let query = state.search_input.trim();
      if query.is_empty() {
        return Task::none();
      }
      navigate(state, Destination::Search(query.to_owned()))
    }
    BrowseMessage::SortMenuToggled => {
      state.browse_sort_menu_open = !state.browse_sort_menu_open;
      Task::none()
    }
    BrowseMessage::SortMenuDismissed => {
      state.browse_sort_menu_open = false;
      Task::none()
    }
    BrowseMessage::SortChanged(sort) => {
      state.browse_sort_menu_open = false;
      persist_browse_filters(state, |filters| filters.with_sort(sort))
    }
    BrowseMessage::SortDirectionToggled => persist_browse_filters(state, |filters| {
      let direction = match filters.sort_direction() {
        VideoLibrarySortDirection::Ascending => VideoLibrarySortDirection::Descending,
        VideoLibrarySortDirection::Descending => VideoLibrarySortDirection::Ascending,
      };
      filters.with_sort_direction(direction)
    }),
    BrowseMessage::PlayedFilterChanged(played_filter) => {
      persist_browse_filters(state, |filters| filters.with_played_filter(played_filter))
    }
    BrowseMessage::FavoritesToggled => persist_browse_filters(state, |filters| {
      filters.with_favorites_only(!filters.favorites_only())
    }),
    BrowseMessage::Scrolled(viewport) => {
      let bounds = viewport.bounds();
      let offset = viewport.absolute_offset();
      state.browse_viewport = BrowseViewport {
        offset_y: offset.y,
        height: bounds.height,
      };
      sync_browse_scroll_window(state)
    }
    BrowseMessage::Retry => {
      let effects = match state.browse.retry() {
        Ok(effects) => effects,
        Err(error) => {
          state.kernel.notice = Some(format!("Could not retry library browsing: {error}"));
          return Task::none();
        }
      };
      sync_browse_view(state);
      apply_browse_effects(state, effects)
    }
    BrowseMessage::PageSettled(settlement) => {
      if state.browse.is_current_settlement(&settlement) {
        state.browse_page_tasks.remove(&settlement.token);
      }
      let effects = match state.browse.settle(settlement) {
        Ok(effects) => effects,
        Err(error) => {
          state.kernel.notice = Some(format!("Could not apply library results: {error}"));
          return Task::none();
        }
      };
      sync_browse_view(state);
      Task::batch([
        apply_browse_effects(state, effects),
        sync_browse_scroll_window(state),
        prepare_browse_artwork(state),
      ])
    }
    BrowseMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = state.kernel.request_gate.is_current_session(session);
      apply_browse_artwork_completion(
        state,
        session_ok,
        ArtworkLoadCompletion {
          slot,
          image_id,
          result,
        },
      );
      Task::none()
    }
  }
}

fn apply_browse_artwork_completion(
  state: &mut State,
  session_ok: bool,
  completion: ArtworkLoadCompletion,
) {
  if state
    .kernel
    .artwork_binder
    .settle(completion.slot, ArtworkSurface::Browse, session_ok)
    != ArtworkSettlement::Apply
  {
    return;
  }
  let Some(cell) = state
    .browse_artwork
    .cell_mut(completion.slot, &completion.image_id)
  else {
    return;
  };
  match completion.result {
    Ok(raster) => {
      cell.state = ArtworkCellState::Ready;
      state.kernel.artwork_handles.insert(
        completion.slot,
        completion.image_id,
        image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels()),
      );
    }
    Err(jellypilot_media_server::artwork::ArtworkError::Cancelled) => {}
    Err(_) => cell.state = ArtworkCellState::Failed,
  }
}

fn persist_browse_filters(
  state: &mut State,
  mutation: impl FnOnce(BrowseFilterSettings) -> BrowseFilterSettings,
) -> Task<Message> {
  if !matches!(state.destination, Destination::Library { .. }) {
    return Task::none();
  }
  let filters = mutation(state.settings.snapshot().browse_filters());
  if let Err(error) = state.settings.set_browse_filters(filters) {
    state.kernel.notice = Some(format!("Could not save library filters: {error}"));
    return Task::none();
  }
  start_browse(state)
}

fn start_browse(state: &mut State) -> Task<Message> {
  let Some(source) = browse_source(state) else {
    abort_browse_pages(state);
    if let Err(error) = state.browse.reset() {
      state.kernel.notice = Some(format!("Could not reset library browsing: {error}"));
      return Task::none();
    }
    sync_browse_view(state);
    state.kernel.notice = Some("The selected library is no longer available.".to_owned());
    return Task::none();
  };
  let preferences = BrowsePreferences::from(state.settings.snapshot().browse_filters());
  let effects = match state.browse.configure_with_preferences(source, preferences) {
    Ok(effects) => effects,
    Err(error) => {
      state.kernel.notice = Some(format!("Could not open library browsing: {error}"));
      sync_browse_view(state);
      return Task::none();
    }
  };
  if state.playback_view.now_playing.is_none() {
    state.kernel.artwork_adapter.cancel_pending();
  }
  begin_browse_artwork_view(state);
  sync_browse_view(state);
  apply_browse_effects(state, effects)
}

fn browse_source(state: &State) -> Option<BrowseSource> {
  let session = state.kernel.request_gate.current_session();
  match &state.destination {
    Destination::Library {
      library_id,
      collection_type,
    } => {
      let jellypilot_core::LoadState::Ready(shortcuts) = &state.home.shortcuts else {
        return None;
      };
      shortcuts
        .iter()
        .find(|shortcut| shortcut.id == *library_id && shortcut.collection_type == *collection_type)
        .cloned()
        .map(|shortcut| BrowseSource::Library { session, shortcut })
    }
    Destination::Search(query) => Some(BrowseSource::Search {
      session,
      query: query.clone(),
    }),
    Destination::Home | Destination::Detail(_) | Destination::Settings => None,
  }
}

/// Extra grid rows included in the loading window beyond each viewport edge.
const BROWSE_WINDOW_MARGIN_ROWS: u32 = 2;

/// Maps scroll geometry to the clamped display-index window the grid covers.
///
/// Rows come from dividing the viewport span by the grid row height; item
/// indexes are rows times the column count, expanded by
/// [`BROWSE_WINDOW_MARGIN_ROWS`] on each side and clamped to `total`.
fn visible_display_range(
  offset_y: f32,
  viewport_height: f32,
  metrics: ArtworkGridMetrics,
  total: u32,
) -> Range<u32> {
  if total == 0
    || metrics.columns == 0
    || !metrics.row_height.is_finite()
    || metrics.row_height <= 0.0
  {
    return 0..0;
  }
  let offset_y = finite_non_negative(offset_y);
  let viewport_height = finite_non_negative(viewport_height);
  let columns = u32::try_from(metrics.columns).unwrap_or(u32::MAX);
  let first_row = (offset_y / metrics.row_height).floor() as u32;
  let end_row = ((offset_y + viewport_height) / metrics.row_height).ceil() as u32;
  let start = first_row
    .saturating_sub(BROWSE_WINDOW_MARGIN_ROWS)
    .saturating_mul(columns)
    .min(total);
  let end = end_row
    .saturating_add(BROWSE_WINDOW_MARGIN_ROWS)
    .saturating_mul(columns)
    .min(total)
    .max(start);
  start..end
}

const fn finite_non_negative(value: f32) -> f32 {
  if value.is_finite() && value > 0.0 {
    value
  } else {
    0.0
  }
}

/// Recomputes the scroll-driven display window and loads newly visible pages.
///
/// The model no-ops an unchanged range, so callers may invoke this freely
/// after scroll, resize, and page-settlement events.
fn sync_browse_scroll_window(state: &mut State) -> Task<Message> {
  let LibraryBrowseView::Ready {
    total_record_count, ..
  } = &state.browse_view
  else {
    return Task::none();
  };
  let total = *total_record_count;
  let class = SizeClass::from_width(state.window_size.width);
  let metrics = ArtworkGridMetrics::for_cards(
    grid_available_width(state.window_size.width, class),
    CARD_COPY_HEIGHT,
  );
  // iced only publishes scroll viewport geometry for overflowing content, so
  // short libraries would never report a height; the window height is a safe
  // upper bound that keeps the auto-fill trigger alive for them.
  let viewport_height = state.browse_viewport.height.max(state.window_size.height);
  let range = visible_display_range(
    state.browse_viewport.offset_y,
    viewport_height,
    metrics,
    total,
  );
  // Metadata-only peek: the hot scroll path must not clone the window's
  // items via `display_range()` just to compare the range.
  if state.browse.peek_display_range().as_ref() == Some(&range) {
    return Task::none();
  }
  let effects = match state.browse.set_display_range(range, total) {
    Ok(effects) => effects,
    Err(error) => {
      state.kernel.notice = Some(format!("Could not load more library items: {error}"));
      return Task::none();
    }
  };
  sync_browse_view(state);
  Task::batch([
    apply_browse_effects(state, effects),
    prepare_browse_artwork(state),
  ])
}

fn sync_browse_view(state: &mut State) {
  state.browse_view = state.browse.view();
}

fn apply_browse_effects(state: &mut State, effects: Vec<BrowseEffect>) -> Task<Message> {
  // Viewport resets must land before page requests: Task::batch runs in
  // parallel, so a fast settlement could evaluate the stale near-tail offset
  // and advance another window before scroll-to-zero is applied.
  let mut resets = Vec::new();
  let mut tasks = Vec::with_capacity(effects.len());
  for effect in effects {
    match effect {
      BrowseEffect::ResetViewport => {
        state.browse_viewport.offset_y = 0.0;
        resets.push(operation::scroll_to(
          state.browse_scroll_id.clone(),
          operation::AbsoluteOffset { x: 0.0, y: 0.0 },
        ));
      }
      BrowseEffect::RequestPage(request) => {
        tasks.push(start_browse_page_request(state, request));
      }
      BrowseEffect::CancelPage { token } => {
        if let Some(handle) = state.browse_page_tasks.remove(&token) {
          handle.abort();
        }
      }
    }
  }
  let tasks = Task::batch(tasks);
  if resets.is_empty() {
    tasks
  } else {
    Task::batch(resets).chain(tasks)
  }
}

fn start_browse_page_request(state: &mut State, request: BrowsePageRequest) -> Task<Message> {
  let token = request.token;
  let failure_message = browse_failure_message(&request.source);
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    return Task::done(Message::Browse(BrowseMessage::PageSettled(
      BrowsePageSettlement {
        source_id: request.source_id,
        token,
        result: Err(failure_message.to_owned()),
      },
    )));
  };
  let (task, handle) = Task::perform(
    async move { fixed_browse_failure(fetch_browse_page(client, request).await, failure_message) },
    |settlement| Message::Browse(BrowseMessage::PageSettled(settlement)),
  )
  .abortable();
  state.browse_page_tasks.insert(token, handle);
  task
}

const fn browse_failure_message(source: &BrowseSource) -> &'static str {
  match source {
    BrowseSource::Library { .. } => "Could not load this library. Try again.",
    BrowseSource::Search { .. } => "Could not load these search results. Try again.",
  }
}

fn fixed_browse_failure(
  mut settlement: BrowsePageSettlement,
  failure_message: &'static str,
) -> BrowsePageSettlement {
  if settlement.result.is_err() {
    settlement.result = Err(failure_message.to_owned());
  }
  settlement
}

fn prepare_browse_artwork(state: &mut State) -> Task<Message> {
  let LibraryBrowseView::Ready {
    visible_items,
    visible_start,
    ..
  } = &state.browse_view
  else {
    return Task::none();
  };
  let visible_start = usize::try_from(*visible_start).unwrap_or(usize::MAX);
  let specs = visible_items
    .iter()
    .enumerate()
    .filter_map(|(index, slot)| {
      slot
        .item
        .as_ref()
        .map(|item| (index, item.id.clone(), item.artwork_image_id.clone()))
    })
    .collect::<Vec<_>>();
  let visible_ids = specs
    .iter()
    .map(|(_, item_id, _)| item_id.as_str())
    .collect::<HashSet<_>>();
  state.browse_artwork.retain_items(&visible_ids);
  drop(visible_ids);
  let session = state.kernel.request_gate.current_session();
  let Some(client) = state.kernel.client.as_ref().map(Arc::clone) else {
    state.retain_artwork_handles();
    return Task::none();
  };
  let adapter = Arc::clone(&state.kernel.artwork_adapter);
  let class = SizeClass::from_width(state.window_size.width);
  let available_width = grid_available_width(state.window_size.width, class);
  let metrics = ArtworkGridMetrics::for_cards(available_width, CARD_COPY_HEIGHT);
  // The grid is the first scrollable child, so scroll coordinates are already
  // grid-local; the window start shifts slot positions to global grid indexes.
  let grid_viewport = ArtworkGridViewport::from_scroll_geometry(
    state.browse_viewport.offset_y,
    state.browse_viewport.height,
    0.0,
  );
  let mut summary = ArtworkLoadSummary::default();
  let mut load_specs = Vec::new();

  for (index, item_id, image_id) in specs {
    let Some(image_id) = image_id else {
      continue;
    };
    if let Some(cell) = state.browse_artwork.get(&item_id) {
      if cell.image_id == image_id {
        if cell.state == ArtworkCellState::Loading {
          continue;
        }
        if cell.state == ArtworkCellState::Ready
          && state
            .kernel
            .artwork_handles
            .get(cell.slot, &cell.image_id)
            .is_some()
        {
          continue;
        }
      }
    }

    if let Some(raster) = adapter.cached(&image_id, ArtworkSizeClass::Card) {
      summary.record(&ArtworkLoadObservation::raster_hit(raster.byte_len() as u64));
      let slot = state.kernel.artwork_binder.bind_settled();
      let handle = image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels());
      state
        .kernel
        .artwork_handles
        .insert(slot, image_id.clone(), handle);
      state.browse_artwork.insert(
        item_id,
        ArtworkCell {
          slot,
          image_id,
          state: ArtworkCellState::Ready,
        },
      );
      continue;
    }

    let slot = state.kernel.artwork_binder.bind(ArtworkSurface::Browse);
    state.browse_artwork.insert(
      item_id,
      ArtworkCell {
        slot,
        image_id: image_id.clone(),
        state: ArtworkCellState::Loading,
      },
    );
    load_specs.push(PlannedArtworkLoad {
      slot,
      image_id,
      size_class: ArtworkSizeClass::Card,
      visible: grid_cell_visible(
        visible_start.saturating_add(index),
        metrics.columns,
        grid_viewport.offset_y,
        grid_viewport.height,
        metrics.row_height,
      ),
    });
  }

  state.retain_artwork_handles();
  stream_artwork_loads(
    adapter,
    client,
    session,
    load_specs,
    summary,
    |session, completion| {
      Message::Browse(BrowseMessage::ArtworkLoaded {
        session,
        slot: completion.slot,
        image_id: completion.image_id,
        result: completion.result,
      })
    },
  )
}

fn begin_browse_artwork_view(state: &mut State) {
  state
    .kernel
    .artwork_binder
    .begin_view(ArtworkSurface::Browse);
  state.browse_artwork.clear();
}

fn leave_browse_view(state: &mut State) {
  abort_browse_pages(state);
  if state.playback_view.now_playing.is_none() {
    state.kernel.artwork_adapter.cancel_pending();
  }
  begin_browse_artwork_view(state);
  if let Err(error) = state.browse.reset() {
    state.kernel.notice = Some(format!("Could not reset library browsing: {error}"));
  }
  sync_browse_view(state);
}

fn abort_browse_pages(state: &mut State) {
  for (_, handle) in state.browse_page_tasks.drain() {
    handle.abort();
  }
}

fn reset_connected_surface(state: &mut State) -> Task<Message> {
  let playback_task =
    apply_playback_input(state, PlaybackInput::Intent(PlaybackIntent::Disconnect));
  state.playback_remote = state.kernel.request_gate.begin_remote();
  state.remote_session = None;
  state.remote_events = None;
  state.remote_control_state = RemoteControlState::Unavailable;
  state.remote_stopping = false;
  abort_browse_pages(state);
  state.kernel.artwork_adapter.reset_session();
  state.kernel.artwork_binder.reset();
  state.in_flight_refresh = None;
  state.in_flight_command = None;
  state.home_artwork = HomeArtwork::default();
  state.browse_artwork = BrowseArtwork::default();
  state.detail_artwork = DetailArtwork::default();
  state.kernel.artwork_handles.clear();
  state.home = HomeState::default();
  state.detail.clear();
  state.detail_items.clear();
  state.navigation_stack.clear();
  if let Err(error) = state.browse.reset() {
    state.kernel.notice = Some(format!("Could not reset library browsing: {error}"));
  }
  state.browse_view = LibraryBrowseView::Inactive;
  state.destination = Destination::Home;
  playback_task
}

pub fn update_login(state: &mut State, message: LoginMessage) -> Task<LoginMessage> {
  match message {
    LoginMessage::ProviderSelected(provider) => {
      interrupt_quick_connect(state);
      state.login.select_provider(provider);
      state.login.error = None;
      Task::none()
    }
    LoginMessage::MethodSelected(method) => {
      if state.login.provider == MediaServerProvider::Jellyfin {
        if method == LoginMethod::Password {
          interrupt_quick_connect(state);
        }
        state.login.method = method;
        state.login.error = None;
      }
      Task::none()
    }
    LoginMessage::ServerUrlChanged(value) => {
      state.login.server_url = value;
      state.login.error = None;
      Task::none()
    }
    LoginMessage::UsernameChanged(value) => {
      state.login.username = value;
      state.login.error = None;
      Task::none()
    }
    LoginMessage::PasswordChanged(value) => {
      state.login.password = Zeroizing::new(value);
      state.login.error = None;
      Task::none()
    }
    LoginMessage::RememberToggled => {
      state.login.remember = !state.login.remember;
      Task::none()
    }
    LoginMessage::QuickConnectSubmitted => {
      if playback_allows_login(state) {
        start_quick_connect(state)
      } else {
        Task::none()
      }
    }
    LoginMessage::QuickConnectCancelled => {
      cancel_quick_connect(state);
      state.kernel.connection = ConnectionPhase::SignedOut;
      state.login.reset_quick_connect();
      state.login.error = None;
      state.kernel.request_gate.disconnect();
      Task::none()
    }
    LoginMessage::PasswordSubmitted => {
      if playback_allows_login(state) {
        start_password_login(state)
      } else {
        Task::none()
      }
    }
    LoginMessage::RemoteDisconnected => {
      state.remote_stopping = false;
      if let Some(client) = state.kernel.client.take() {
        client.login().disconnect();
      }
      state.kernel.request_gate.disconnect();
      state.kernel.connection = ConnectionPhase::SignedOut;
      state.kernel.connected_identity = None;
      state.kernel.active_profile = None;
      Task::none()
    }
    LoginMessage::ProfilesLoaded { revision, result } => {
      state.login.profiles_loading = false;
      if revision != state.login.profiles_revision {
        return Task::none();
      }
      match result {
        Ok(profiles) => state.login.profiles = profiles,
        Err(error) => {
          state.login.error = Some(LoginError::AuthStorage(error).to_string());
        }
      }
      Task::none()
    }
    LoginMessage::WorkflowEvent(event) => handle_workflow_event(state, event),
    LoginMessage::PasswordFinished {
      session,
      client,
      result,
      submission,
    } => {
      if !state.kernel.request_gate.finish_login(session) {
        return Task::none();
      }
      match result {
        Ok(saved_session) => {
          let Some(saved_session) = saved_session.take() else {
            return Task::none();
          };
          complete_authentication(state, session, client, saved_session, Some(submission))
        }
        Err(error) => {
          fail_password_login(state, &error);
          Task::none()
        }
      }
    }
    LoginMessage::SavedSessionStored { session, result } => {
      let current = state.kernel.request_gate.is_current_session(session);
      match result {
        Ok((key, profiles)) => {
          state.login.profiles_revision = state.login.profiles_revision.wrapping_add(1);
          state.login.profiles = profiles;
          if current {
            state.kernel.active_profile = Some(key);
          }
        }
        Err(error) if current => {
          state.kernel.notice = Some(LoginError::AuthStorage(error).to_string());
        }
        Err(_) => {}
      }
      Task::none()
    }
    LoginMessage::RestoreProfile(key) => {
      if playback_allows_login(state) {
        start_restore(state, key)
      } else {
        Task::none()
      }
    }
    LoginMessage::RestoreFinished {
      session,
      key,
      result,
    } => {
      if !state.kernel.request_gate.finish_login(session) {
        return Task::none();
      }
      if state.login.busy_profile.as_ref() == Some(&key) {
        state.login.busy_profile = None;
      }
      match result {
        Ok(saved_session) => {
          let Some(saved_session) = saved_session.take() else {
            return Task::none();
          };
          let client = Arc::new(JellyfinClient::new());
          client.login().adopt_validated_session(&saved_session);
          state.kernel.connection = ConnectionPhase::Connected;
          state.kernel.connected_identity = Some(ConnectedIdentity::from_session(&saved_session));
          state.kernel.client = Some(client);
          state.kernel.active_profile = Some(key);
          state.login.error = None;
        }
        Err(error) => fail_restore(state, &error),
      }
      Task::none()
    }
    LoginMessage::AskForgetProfile(key) => {
      if state.login.busy_profile.is_none() {
        state.login.forget_confirmation = Some(key);
      }
      Task::none()
    }
    LoginMessage::CancelForgetProfile => {
      state.login.forget_confirmation = None;
      Task::none()
    }
    LoginMessage::ConfirmForgetProfile(key) => start_forget(state, key).unwrap_or_else(Task::none),
    LoginMessage::ForgetFinished {
      session,
      key,
      sign_out,
      result,
    } => {
      if state.login.busy_profile.as_ref() == Some(&key) {
        state.login.busy_profile = None;
      }
      if state.login.forget_confirmation.as_ref() == Some(&key) {
        state.login.forget_confirmation = None;
      }
      let active_matches = state.kernel.active_profile.as_ref() == Some(&key);
      let disconnect = should_disconnect_after_forget(
        sign_out,
        session,
        state.kernel.request_gate.current_session(),
        state.kernel.connection,
        active_matches,
      );
      match result {
        Ok(profiles) => {
          state.login.profiles_revision = state.login.profiles_revision.wrapping_add(1);
          state.login.profiles = profiles;
          if disconnect {
            return stop_remote_session_for_login(state);
          }
        }
        Err(error) => state.login.error = Some(LoginError::AuthStorage(error).to_string()),
      }
      Task::none()
    }
  }
}

fn stop_remote_session_for_login(state: &mut State) -> Task<LoginMessage> {
  state.playback_remote = state.kernel.request_gate.begin_remote();
  state.remote_events = None;
  state.remote_control_state = RemoteControlState::Unavailable;
  let Some(session) = state.remote_session.take() else {
    return Task::done(LoginMessage::RemoteDisconnected);
  };
  state.remote_stopping = true;
  Task::perform(
    async move { disconnect_remote_session(session).await },
    |()| LoginMessage::RemoteDisconnected,
  )
}

fn playback_allows_login(state: &mut State) -> bool {
  if state.playback_view.can_start_login {
    true
  } else {
    state.login.error =
      Some("Finishing external playback shutdown. Try again in a moment.".to_owned());
    false
  }
}

pub fn load_saved_profiles(state: &State) -> Task<LoginMessage> {
  let store = state.kernel.auth_store.clone();
  let revision = state.login.profiles_revision;
  Task::perform(async move { store.load_profiles().await }, move |result| {
    LoginMessage::ProfilesLoaded { revision, result }
  })
}

fn start_quick_connect(state: &mut State) -> Task<LoginMessage> {
  if !can_start_login(state.kernel.connection) {
    return Task::none();
  }
  if state.login.provider != MediaServerProvider::Jellyfin {
    state.login.method = LoginMethod::Password;
    return Task::none();
  }
  let server_url = match validate_server_url(&state.login.server_url, state.login.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      state.login.error = Some(error);
      return Task::none();
    }
  };
  state.login.server_url = server_url.clone();

  cancel_quick_connect(state);
  let session = state.kernel.request_gate.begin_login();
  state.kernel.connection = ConnectionPhase::Connecting;
  state.login.quick_connect = QuickConnectState::Requesting;
  state.login.error = None;
  let client = Arc::new(JellyfinClient::new());
  let stream = iced::stream::channel(16, async move |sender| {
    let sender = Arc::new(Mutex::new(sender));
    quick_connect_workflow(
      client,
      server_url,
      session,
      move |event| {
        sender
          .lock()
          .is_ok_and(|mut sender| sender.try_send(event).is_ok())
      },
      QUICK_CONNECT_POLL_INTERVAL,
      QUICK_CONNECT_TIMEOUT,
    )
    .await;
  });
  let (task, handle) = Task::run(stream, LoginMessage::WorkflowEvent).abortable();
  state.quick_connect_task = Some(handle);
  task
}

fn start_password_login(state: &mut State) -> Task<LoginMessage> {
  if !can_start_login(state.kernel.connection) {
    return Task::none();
  }
  let server_url = match validate_server_url(&state.login.server_url, state.login.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      state.login.error = Some(error);
      return Task::none();
    }
  };
  state.login.server_url = server_url.clone();
  let username = state.login.username.trim().to_owned();
  if username.is_empty() {
    state.login.error = Some("Enter your username before signing in.".to_owned());
    return Task::none();
  }

  cancel_quick_connect(state);
  let session = state.kernel.request_gate.begin_login();
  state.kernel.connection = ConnectionPhase::Connecting;
  state.login.error = None;
  let client = Arc::new(JellyfinClient::new());
  let command_client = Arc::clone(&client);
  let submission = password_submission(state, server_url.clone(), username.clone());
  let credentials = AuthStore::protect_credentials(Credentials {
    provider: state.login.provider,
    server_url,
    username,
    password: std::mem::take(&mut *state.login.password),
  });
  Task::perform(
    async move {
      let result = async {
        let mut response = command_client
          .login()
          .authenticate(&credentials)
          .await
          .map_err(|_| LoginError::Request("Password authentication failed.".to_owned()))?;
        response.access_token.zeroize();
        jellypilot_auth::SensitiveSavedSession::from_client(&command_client)
          .map(ProtectedSavedSession::new)
          .ok_or_else(|| LoginError::Request("Password authentication failed.".to_owned()))
      }
      .await;
      (client, result)
    },
    move |(client, result)| LoginMessage::PasswordFinished {
      session,
      client,
      result,
      submission,
    },
  )
}

fn password_submission(state: &State, server_url: String, username: String) -> PasswordSubmission {
  PasswordSubmission {
    remember: state.login.remember,
    prefill: LoginPrefill::new(server_url, username),
    provider: state.login.provider,
  }
}

fn handle_workflow_event(state: &mut State, event: LoginEvent) -> Task<LoginMessage> {
  match event {
    LoginEvent::QuickConnectCode { session, code } => {
      if state.kernel.request_gate.is_current_login(session) {
        state.login.quick_connect = QuickConnectState::Waiting(code);
      }
      Task::none()
    }
    LoginEvent::QuickConnectApproving { session } => {
      if state.kernel.request_gate.is_current_login(session) {
        state.login.quick_connect = QuickConnectState::Approving;
      }
      Task::none()
    }
    LoginEvent::Login {
      session,
      client,
      result,
    } => {
      if !state.kernel.request_gate.finish_login(session) {
        return Task::none();
      }
      state.quick_connect_task = None;
      match result {
        Ok(()) => match jellypilot_auth::SensitiveSavedSession::from_client(&client) {
          Some(saved_session) => {
            complete_authentication(state, session, client, saved_session, None)
          }
          None => {
            fail_login(
              state,
              LoginError::Request("Quick Connect returned no session.".to_owned()),
            );
            Task::none()
          }
        },
        Err(error) => {
          fail_login(state, error);
          state.login.quick_connect = QuickConnectState::Failed;
          Task::none()
        }
      }
    }
    LoginEvent::SavedProfiles(result) => update_login(
      state,
      LoginMessage::ProfilesLoaded {
        revision: state.login.profiles_revision,
        result,
      },
    ),
    LoginEvent::SavedSessionStored { session, result } => {
      update_login(state, LoginMessage::SavedSessionStored { session, result })
    }
    LoginEvent::ForgotProfile {
      session,
      key,
      sign_out,
      result,
    } => update_login(
      state,
      LoginMessage::ForgetFinished {
        session,
        key,
        sign_out,
        result,
      },
    ),
  }
}

fn complete_authentication(
  state: &mut State,
  session: jellypilot_core::request_gate::SessionToken,
  client: Arc<JellyfinClient>,
  saved_session: SensitiveSessionPayload,
  submission: Option<PasswordSubmission>,
) -> Task<LoginMessage> {
  let identity = ConnectedIdentity::from_session(&saved_session);
  if let Some(submission) = submission {
    persist_password_submission(state, submission);
  }

  state.kernel.connection = ConnectionPhase::Connected;
  state.kernel.connected_identity = Some(identity);
  state.kernel.client = Some(client);
  state.login.password.clear();
  state.login.error = None;
  state.login.reset_quick_connect();
  let store = state.kernel.auth_store.clone();

  Task::perform(
    async move { store.save_session(saved_session).await },
    move |result| LoginMessage::SavedSessionStored { session, result },
  )
}

fn persist_password_submission(state: &mut State, submission: PasswordSubmission) {
  let settings_result = if submission.remember {
    state.settings.set_login_prefill(
      submission.prefill,
      provider_key(submission.provider).to_owned(),
    )
  } else {
    state.settings.clear_login_prefill()
  };
  if let Err(error) = settings_result {
    state.kernel.notice = Some(format!("Could not update remembered sign-in: {error}"));
  }
}

fn start_restore(state: &mut State, key: jellypilot_auth::SavedProfileKey) -> Task<LoginMessage> {
  interrupt_quick_connect(state);
  let session = state.kernel.request_gate.begin_login();
  state.kernel.connection = ConnectionPhase::Connecting;
  state.login.busy_profile = Some(key.clone());
  state.login.error = None;
  let store = state.kernel.auth_store.clone();
  Task::perform(
    async move {
      let result = async {
        let sensitive = store.load_session(key.clone()).await?;
        let candidate = JellyfinClient::for_saved_profile(&sensitive);
        candidate
          .login()
          .restore_session(&sensitive)
          .await
          .map_err(|_| LoginError::Request("Saved sign-in validation failed.".to_owned()))?;
        jellypilot_auth::SensitiveSavedSession::from_client(&candidate)
          .map(ProtectedSavedSession::new)
          .ok_or_else(|| LoginError::Request("Saved sign-in validation failed.".to_owned()))
      }
      .await;
      (key, result)
    },
    move |(key, result)| LoginMessage::RestoreFinished {
      session,
      key,
      result,
    },
  )
}

fn start_forget(
  state: &mut State,
  key: jellypilot_auth::SavedProfileKey,
) -> Option<Task<LoginMessage>> {
  if state.login.busy_profile.is_some() {
    return None;
  }
  state.login.forget_confirmation = None;
  state.login.busy_profile = Some(key.clone());
  let session = state.kernel.request_gate.current_session();
  let sign_out = state.kernel.active_profile.as_ref() == Some(&key);
  let store = state.kernel.auth_store.clone();
  Some(Task::perform(
    async move {
      let result = store.remove_profile(key.clone()).await;
      (key, result)
    },
    move |(key, result)| LoginMessage::ForgetFinished {
      session,
      key,
      sign_out,
      result,
    },
  ))
}

fn cancel_quick_connect(state: &mut State) {
  if let Some(handle) = state.quick_connect_task.take() {
    handle.abort();
  }
}

fn interrupt_quick_connect(state: &mut State) {
  if state.quick_connect_task.is_some()
    || !matches!(state.login.quick_connect, QuickConnectState::Idle)
  {
    cancel_quick_connect(state);
    state.kernel.request_gate.disconnect();
    state.kernel.connection = ConnectionPhase::SignedOut;
    state.login.reset_quick_connect();
  }
}

fn fail_login(state: &mut State, error: LoginError) {
  state.kernel.connection = ConnectionPhase::Failed;
  state.login.error = Some(error.to_string());
}

fn fail_password_login(state: &mut State, _error: &LoginError) {
  state.kernel.connection = ConnectionPhase::Failed;
  state.login.error =
    Some("Sign-in failed. Check your server, username, and password, then try again.".to_owned());
}

fn fail_restore(state: &mut State, _error: &LoginError) {
  state.kernel.connection = ConnectionPhase::Failed;
  state.login.error =
    Some("Could not restore this saved sign-in. Sign in again to refresh it.".to_owned());
}

fn provider_key(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "jellyfin",
    MediaServerProvider::Emby => "emby",
  }
}

fn validate_server_url(raw: &str, provider: MediaServerProvider) -> Result<String, String> {
  let server_url = raw.trim().trim_end_matches('/');
  let invalid = || format!("Enter a valid {} server URL.", provider_label(provider));
  if server_url.is_empty() || !raw_path_is_safe(server_url) {
    return Err(invalid());
  }
  let parsed = Url::parse(server_url).map_err(|_| invalid())?;
  if !matches!(parsed.scheme(), "http" | "https")
    || parsed.host_str().is_none()
    || !parsed.username().is_empty()
    || parsed.password().is_some()
    || parsed.query().is_some()
    || parsed.fragment().is_some()
    || !path_segments_are_safe(parsed.path())
  {
    return Err(invalid());
  }
  Ok(server_url.to_owned())
}

fn raw_path_is_safe(url: &str) -> bool {
  let without_fragment = url.split('#').next().unwrap_or_default();
  let without_query = without_fragment.split('?').next().unwrap_or_default();
  let path = without_query
    .split_once("://")
    .and_then(|(_, authority_and_path)| {
      authority_and_path
        .find('/')
        .map(|at| &authority_and_path[at..])
    })
    .unwrap_or(without_query);
  path_segments_are_safe(path)
}

fn path_segments_are_safe(path: &str) -> bool {
  !path.split('/').any(|segment| {
    let segment = segment.to_ascii_lowercase();
    segment.contains("%2f")
      || segment.contains("%5c")
      || matches!(segment.replace("%2e", ".").as_str(), "." | "..")
  })
}

fn provider_label(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "Jellyfin",
    MediaServerProvider::Emby => "Emby",
  }
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;

  use super::*;
  use crate::app::kernel::Kernel;
  use crate::app::state::LoginState;
  use jellypilot_auth::{AuthStorageError, SavedProfileKey};
  use jellypilot_core::config::SettingsStore;
  #[test]
  fn skeleton_phase_wraps_once_per_1600ms() {
    assert_eq!(skeleton_phase_at(Duration::from_millis(0)), 0.0);
    assert_eq!(skeleton_phase_at(Duration::from_millis(800)), 0.5);
    assert_eq!(skeleton_phase_at(Duration::from_millis(1600)), 0.0);
    assert_eq!(skeleton_phase_at(Duration::from_millis(2000)), 0.25);
  }

  #[test]
  fn frame_tick_advances_phase_while_skeletons_load_and_resets_after() {
    let mut state = test_state();
    state.home.begin_load();

    let start = Instant::now();
    drop(update_window(&mut state, WindowMessage::FrameTick(start)));
    assert_eq!(state.skeleton_phase, 0.0);
    assert_eq!(state.skeleton_animation_start, Some(start));

    drop(update_window(
      &mut state,
      WindowMessage::FrameTick(start + Duration::from_millis(800)),
    ));
    assert_eq!(state.skeleton_phase, 0.5);

    // Once nothing loads, the clock resets so the next burst starts at 0.
    state.home.settle_video_home(Err("settled".to_owned()));
    state.home.settle_shortcuts(Err("settled".to_owned()));
    drop(update_window(
      &mut state,
      WindowMessage::FrameTick(start + Duration::from_millis(1200)),
    ));
    assert_eq!(state.skeleton_phase, 0.0);
    assert_eq!(state.skeleton_animation_start, None);
  }

  fn test_state() -> State {
    let settings = SettingsStore::default();
    let mut request_gate = RequestGate::default();
    let playback_remote = request_gate.begin_remote();
    let playback_session = jellypilot_mpv::playback_session::PlaybackSession::default();
    let playback_view = playback_session.view();
    let settings_view = crate::app::state::SettingsState::from_settings(settings.snapshot());
    State {
      smoke: false,
      window_size: iced::Size::new(1600.0, 900.0),
      skeleton_phase: 0.0,
      skeleton_animation_start: None,
      login: LoginState::from_settings(settings.snapshot()),
      settings,
      settings_view,
      kernel: Kernel {
        diagnostics: jellypilot_core::diagnostics::Diagnostics::default(),
        auth_store: AuthStore::default(),
        request_gate,
        client: None,
        connection: ConnectionPhase::SignedOut,
        connected_identity: None,
        active_profile: None,
        notice: None,
        active_toast: None,
        next_toast_id: 0,
        tray: None,
        artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
        artwork_binder: Default::default(),
        artwork_handles: Default::default(),
      },
      quick_connect_task: None,
      playback_notice: None,
      quit_requested: false,
      destination: Destination::Home,
      navigation_stack: Vec::new(),
      detail_items: Default::default(),
      detail: DetailState::default(),
      detail_artwork: DetailArtwork::default(),
      home: HomeState::default(),
      home_artwork: HomeArtwork::default(),
      playback_artwork: None,
      playback_controller: None,
      playback_session,
      playback_view,
      playback_playable: None,
      adjacent_playables: [None, None],
      in_flight_refresh: None,
      in_flight_command: None,
      playback_remote,
      remote_session: None,
      remote_events: None,
      remote_control_state: RemoteControlState::Unavailable,
      remote_stopping: false,
      seek_preview: None,
      volume_preview: None,
      browse: Default::default(),
      browse_view: LibraryBrowseView::Inactive,
      browse_artwork: Default::default(),
      browse_page_tasks: Default::default(),
      browse_viewport: BrowseViewport::default(),
      browse_scroll_id: iced::widget::Id::unique(),
      browse_sort_menu_open: false,
      audio_menu_open: false,
      subtitle_menu_open: false,
      search_input: String::new(),
    }
  }

  fn browse_request(effects: Vec<BrowseEffect>) -> BrowsePageRequest {
    effects
      .into_iter()
      .find_map(|effect| match effect {
        BrowseEffect::RequestPage(request) => Some(request),
        BrowseEffect::ResetViewport | BrowseEffect::CancelPage { .. } => None,
      })
      .expect("browse request should be emitted")
  }

  fn search_source(state: &State, query: &str) -> BrowseSource {
    BrowseSource::Search {
      session: state.kernel.request_gate.current_session(),
      query: query.to_owned(),
    }
  }

  fn video_item(id: &str) -> jellypilot_media_server::VideoItemDetail {
    jellypilot_media_server::VideoItemDetail {
      id: id.to_owned(),
      name: "Arrival".to_owned(),
      item_type: "Movie".to_owned(),
      overview: None,
      production_year: Some(2016),
      runtime_seconds: Some(116.0 * 60.0),
      series_id: None,
      series_name: None,
      season_number: None,
      episode_number: None,
      genres: vec!["Science Fiction".to_owned()],
      played: false,
      favorite: false,
      played_percentage: None,
      resume_position_seconds: None,
      can_resume: false,
      can_play: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      metadata: Default::default(),
    }
  }

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      id: id.to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      runtime_seconds: Some(1_800.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      series_poster_image_id: None,
      season_number: Some(season_number),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
    }
  }

  fn media_item(id: &str) -> jellypilot_media_server::MediaItem {
    jellypilot_media_server::MediaItem {
      id: id.to_owned(),
      name: "Pilot".to_owned(),
      item_type: "Episode".to_owned(),
      series_id: Some("series-1".to_owned()),
      series_name: Some("Series".to_owned()),
      season_name: None,
      index_number: Some(1),
      parent_index_number: Some(1),
      run_time_ticks: Some(1_800_000_000),
      overview: None,
      series_primary_image_tag: None,
    }
  }

  fn season(id: &str, number: i32) -> VideoSeason {
    VideoSeason {
      id: id.to_owned(),
      name: format!("Season {number}"),
      season_number: Some(number),
      played: false,
      favorite: false,
      artwork_image_id: None,
    }
  }

  fn show_detail() -> jellypilot_media_server::VideoShowDetail {
    jellypilot_media_server::VideoShowDetail {
      id: "show-1".to_owned(),
      name: "Show".to_owned(),
      overview: None,
      production_year: None,
      genres: Vec::new(),
      played: false,
      favorite: false,
      can_play: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      next_episode: Some(episode("episode-2", 2)),
      seasons: vec![season("season-1", 1), season("season-2", 2)],
      metadata: Default::default(),
    }
  }

  #[test]
  fn stale_detail_settlement_cannot_replace_the_current_request() {
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Loading,
      ..DetailState::default()
    };
    let mut gate = RequestGate::default();
    let stale = gate.begin_detail();
    let current = gate.begin_detail();

    assert!(!settle_detail_load(
      &mut detail,
      &mut gate,
      stale,
      Ok(DetailContent::Item(video_item("stale"))),
    ));
    assert!(matches!(
      detail.content,
      jellypilot_core::LoadState::Loading
    ));
    assert!(settle_detail_load(
      &mut detail,
      &mut gate,
      current,
      Ok(DetailContent::Item(video_item("current"))),
    ));
    assert!(matches!(
      &detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.id == "current"
    ));
  }

  #[test]
  fn user_data_transition_waits_for_confirmation_and_preserves_data_on_failure() {
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Ready(DetailContent::Item(video_item("item-1"))),
      user_data_busy: Some(UserDataActionKind::Favorite),
      ..DetailState::default()
    };
    let mut gate = RequestGate::default();
    gate.set_detail_item(Some("item-1".to_owned()));
    let stale = gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("detail item should permit user-data update");
    let success = gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("detail item should permit user-data update");

    assert!(settle_user_data_update(
      &mut detail,
      &mut gate,
      stale,
      Ok(VideoUserDataUpdate {
        item_id: "item-1".to_owned(),
        played: true,
        favorite: true,
      }),
    )
    .is_none());
    assert_eq!(detail.user_data_busy, Some(UserDataActionKind::Favorite));

    let applied = settle_user_data_update(
      &mut detail,
      &mut gate,
      success,
      Ok(VideoUserDataUpdate {
        item_id: "item-1".to_owned(),
        played: false,
        favorite: true,
      }),
    );
    assert!(matches!(applied, Some(Some(_))));
    assert!(matches!(
      &detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.favorite && !item.played
    ));
    assert!(detail.user_data_busy.is_none());

    detail.user_data_busy = Some(UserDataActionKind::Played);
    let failure = gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("retry should mint a fresh token");
    assert!(matches!(
      settle_user_data_update(
        &mut detail,
        &mut gate,
        failure,
        Err("raw server response".to_owned()),
      ),
      Some(None)
    ));
    assert!(matches!(
      &detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.favorite && !item.played
    ));
    assert_eq!(detail.user_data_error.as_deref(), Some(USER_DATA_FAILURE));
  }

  #[test]
  fn leaving_detail_rejects_pending_user_data_after_reopening_same_item() {
    let mut state = test_state();
    state.destination = Destination::Detail("item-1".to_owned());
    state
      .kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    let stale = state
      .kernel
      .request_gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("detail item should permit user-data update");

    leave_detail_view(&mut state);
    state.destination = Destination::Detail("item-1".to_owned());
    state
      .kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    state.detail.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(video_item("item-1")));
    state.detail.user_data_busy = Some(UserDataActionKind::Favorite);

    let settlement = settle_user_data_update(
      &mut state.detail,
      &mut state.kernel.request_gate,
      stale,
      Ok(VideoUserDataUpdate {
        item_id: "item-1".to_owned(),
        played: true,
        favorite: true,
      }),
    );

    assert!(settlement.is_none());
    assert!(matches!(
      &state.detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if !item.played && !item.favorite
    ));
    assert_eq!(
      state.detail.user_data_busy,
      Some(UserDataActionKind::Favorite)
    );
  }

  #[test]
  fn season_switching_uses_the_selected_seasons_exact_identity() {
    let show = show_detail();
    assert_eq!(
      initial_season(&show).map(|season| season.id.as_str()),
      Some("season-2")
    );
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Ready(DetailContent::Show(show)),
      selected_season_id: Some("season-2".to_owned()),
      ..DetailState::default()
    };

    assert!(select_season(&mut detail, "season-1"));
    let request = selected_season_request(&detail).expect("selected season should produce a page");
    assert_eq!(request.series_id, "show-1");
    assert_eq!(request.season_id.as_deref(), Some("season-1"));
    assert_eq!(request.season_number, Some(1));
    assert_eq!(request.start_index, 0);
    assert_eq!(
      request.limit,
      jellypilot_core::detail::SEASON_EPISODE_PAGE_SIZE
    );
    assert!(!select_season(&mut detail, "season-1"));
    assert!(!select_season(&mut detail, "missing-season"));
  }

  #[test]
  fn identical_browse_resubmit_keeps_the_in_flight_request_handle() {
    let mut state = test_state();
    state.destination = Destination::Search("arrival".to_owned());
    let request = browse_request(
      state
        .browse
        .configure(search_source(&state, "arrival"))
        .expect("search should configure"),
    );
    let (_, handle) = Task::<Message>::none().abortable();
    state.browse_page_tasks.insert(request.token, handle);

    drop(start_browse(&mut state));

    assert!(state.browse_page_tasks.contains_key(&request.token));
    assert!(matches!(state.browse_view, LibraryBrowseView::Loading));
  }

  #[test]
  fn stale_same_session_settlement_keeps_the_reopened_request_handle() {
    let mut state = test_state();
    let source = search_source(&state, "arrival");
    let stale = browse_request(
      state
        .browse
        .configure(source.clone())
        .expect("first search should configure"),
    );
    state.browse.reset().expect("browse epoch should advance");
    let current = browse_request(
      state
        .browse
        .configure(source)
        .expect("search should reopen"),
    );
    let (_, handle) = Task::<Message>::none().abortable();
    state.browse_page_tasks.insert(current.token, handle);

    drop(update_browse(
      &mut state,
      BrowseMessage::PageSettled(BrowsePageSettlement {
        source_id: stale.source_id,
        token: stale.token,
        result: Err("stale server response".to_owned()),
      }),
    ));

    assert!(state.browse_page_tasks.contains_key(&current.token));
    assert!(matches!(state.browse_view, LibraryBrowseView::Loading));
  }

  #[test]
  fn browse_failure_messages_are_fixed_for_library_and_search_sources() {
    let state = test_state();
    let library = BrowseSource::Library {
      session: state.kernel.request_gate.current_session(),
      shortcut: jellypilot_media_server::VideoLibraryShortcut {
        id: "library-1".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: None,
        artwork_image_id: None,
      },
    };
    let search = search_source(&state, "arrival");

    assert_eq!(
      browse_failure_message(&library),
      "Could not load this library. Try again."
    );
    assert_eq!(
      browse_failure_message(&search),
      "Could not load these search results. Try again."
    );
    let settlement = fixed_browse_failure(
      BrowsePageSettlement {
        source_id: "source".to_owned(),
        token: jellypilot_core::LibraryBrowseLoadToken {
          generation: 1,
          sequence: 1,
        },
        result: Err("HTTP 500: raw server response body".to_owned()),
      },
      browse_failure_message(&search),
    );
    assert_eq!(
      settlement.result.as_ref().err().map(String::as_str),
      Some("Could not load these search results. Try again.")
    );
  }

  #[test]
  fn reset_viewport_effect_clears_the_recorded_scroll_offset() {
    let mut state = test_state();
    state.browse_viewport.offset_y = 640.0;

    drop(apply_browse_effects(
      &mut state,
      vec![BrowseEffect::ResetViewport],
    ));

    assert_eq!(state.browse_viewport.offset_y, 0.0);
  }

  #[test]
  fn stale_home_settlement_does_not_replace_the_current_loading_state() {
    let mut home = HomeState::default();
    let mut gate = RequestGate::default();
    let stale = gate.begin_home();
    let _current = gate.begin_home();
    home.begin_load();

    let applied = settle_home(
      &mut home,
      &mut gate,
      stale,
      (
        Err("stale home".to_owned()),
        Err("stale shortcuts".to_owned()),
      ),
    );

    assert!(matches!(
      (applied, &home.continue_watching, &home.shortcuts),
      (
        false,
        jellypilot_core::LoadState::Loading,
        jellypilot_core::LoadState::Loading
      )
    ));
  }

  #[test]
  fn browse_scroll_position_drives_the_display_window() {
    // 1600×900 window: 1248px grid, 8 columns, 275px rows; the 900px window
    // height covers 4 rows, so the settled bootstrap expands to 6 rows.
    let mut state = test_state();
    let library = BrowseSource::Library {
      session: state.kernel.request_gate.current_session(),
      shortcut: jellypilot_media_server::VideoLibraryShortcut {
        id: "library-1".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: Some(264),
        artwork_image_id: None,
      },
    };
    let initial_request = browse_request(
      state
        .browse
        .configure(library)
        .expect("library should configure"),
    );
    sync_browse_view(&mut state);

    let settlement = BrowsePageSettlement {
      source_id: initial_request.source_id.clone(),
      token: initial_request.token,
      result: Ok(jellypilot_core::browse_model::BrowsePagePayload {
        start_index: 0,
        limit: 24,
        total_record_count: 264,
        has_more: true,
        items: (0..24)
          .map(|index| VideoLibraryItem {
            id: format!("item-{index}"),
            name: format!("Item {index}"),
            item_type: "Movie".to_owned(),
            production_year: None,
            runtime_seconds: None,
            played: false,
            favorite: false,
            artwork_image_id: None,
            series_poster_image_id: None,
            season_number: None,
            episode_number: None,
            series_id: None,
            series_name: None,
            resume_position_seconds: None,
            played_percentage: None,
            overview: None,
          })
          .collect(),
      }),
    };
    drop(update_browse(
      &mut state,
      BrowseMessage::PageSettled(settlement),
    ));

    // Settlement triggers a scroll-window sync that fills the viewport.
    assert_eq!(state.browse.display_range(), Some(0..48));

    // Scrolling ten rows down shifts the window without resetting it.
    state.browse_viewport = BrowseViewport {
      offset_y: 2750.0,
      height: 800.0,
    };
    drop(sync_browse_scroll_window(&mut state));
    assert_eq!(state.browse.display_range(), Some(64..128));

    // An unchanged viewport keeps the window and emits no page requests.
    let pending_before = state.browse_page_tasks.len();
    drop(sync_browse_scroll_window(&mut state));
    assert_eq!(state.browse.display_range(), Some(64..128));
    assert_eq!(state.browse_page_tasks.len(), pending_before);

    // Scrolling back up restores the earlier window.
    state.browse_viewport = BrowseViewport {
      offset_y: 0.0,
      height: 800.0,
    };
    drop(sync_browse_scroll_window(&mut state));
    assert_eq!(state.browse.display_range(), Some(0..48));
  }

  #[test]
  fn visible_display_range_maps_scroll_geometry_to_item_indexes() {
    let metrics = ArtworkGridMetrics {
      columns: 8,
      cell_width: 142.0,
      cell_height: 259.0,
      row_height: 275.0,
    };

    // Top of the grid: four visible rows plus two margin rows below.
    assert_eq!(visible_display_range(0.0, 900.0, metrics, 264), 0..48);
    // Middle: rows 10..14 visible, expanded to rows 8..16.
    assert_eq!(visible_display_range(2750.0, 900.0, metrics, 264), 64..128);
    // Near the end the window clamps to the total.
    assert_eq!(visible_display_range(8800.0, 900.0, metrics, 264), 240..264);
    // A zero-height viewport still covers its margin rows.
    assert_eq!(visible_display_range(0.0, 0.0, metrics, 264), 0..16);
    // An empty library yields an empty window.
    assert_eq!(visible_display_range(0.0, 900.0, metrics, 0), 0..0);
  }

  #[test]
  fn visible_display_range_sanitizes_degenerate_inputs() {
    let metrics = ArtworkGridMetrics {
      columns: 8,
      cell_width: 142.0,
      cell_height: 259.0,
      row_height: 275.0,
    };

    // Non-finite or negative geometry falls back to the grid origin.
    assert_eq!(visible_display_range(f32::NAN, 900.0, metrics, 264), 0..48);
    assert_eq!(
      visible_display_range(2750.0, f32::INFINITY, metrics, 264),
      64..96
    );
    assert_eq!(visible_display_range(-50.0, 900.0, metrics, 264), 0..48);

    // Degenerate metrics cannot map rows, so the window is empty.
    let zero_row = ArtworkGridMetrics {
      row_height: 0.0,
      ..metrics
    };
    assert_eq!(visible_display_range(0.0, 900.0, zero_row, 264), 0..0);
    let no_columns = ArtworkGridMetrics {
      columns: 0,
      ..metrics
    };
    assert_eq!(visible_display_range(0.0, 900.0, no_columns, 264), 0..0);
  }

  struct TestSettingsFile(PathBuf);

  impl Drop for TestSettingsFile {
    fn drop(&mut self) {
      let _ = fs::remove_file(&self.0);
    }
  }

  fn isolated_settings(name: &str) -> (SettingsStore, TestSettingsFile) {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-settings-{}-{name}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    (
      SettingsStore::for_test(path.clone()),
      TestSettingsFile(path),
    )
  }

  fn profile_key(name: &str) -> SavedProfileKey {
    let server_url = format!("https://{name}.example.test");
    let user_id = format!("{name}-user-id");
    SavedProfileKey::for_identity(MediaServerProvider::Jellyfin, &server_url, &user_id)
  }

  #[test]
  fn invalid_server_url_is_rejected_before_a_login_token_is_created() {
    let mut state = test_state();
    state.login.server_url = "not a server".to_owned();
    let session_before = state.kernel.request_gate.current_session();

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));

    assert_eq!(state.kernel.request_gate.current_session(), session_before);
    assert_eq!(
      state.login.error.as_deref(),
      Some("Enter a valid Jellyfin server URL.")
    );
  }

  #[test]
  fn quick_connect_cancel_and_retry_reset_display_state_and_replace_request() {
    let mut state = test_state();
    state.login.server_url = "https://media.example.test".to_owned();
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    let first_session = state.kernel.request_gate.current_session();
    state.login.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectCancelled,
    ));
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    assert_eq!(state.login.quick_connect, QuickConnectState::Requesting);
    assert_ne!(state.kernel.request_gate.current_session(), first_session);
  }

  #[test]
  fn remembered_prefill_can_be_applied_and_cleared_without_display_state() {
    let mut state = test_state();
    state.login.apply_prefill(Some(LoginPrefill::new(
      "https://media.example.test".to_owned(),
      "ada".to_owned(),
    )));
    assert_eq!(state.login.username, "ada");

    state.login.apply_prefill(None);
    assert!(state.login.server_url.is_empty());
    assert!(state.login.username.is_empty());
    assert!(!state.login.remember);
  }

  #[test]
  fn selecting_emby_forces_password_and_hides_quick_connect_state() {
    let mut state = test_state();
    state.login.method = LoginMethod::QuickConnect;
    state.login.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());

    drop(update_login(
      &mut state,
      LoginMessage::ProviderSelected(MediaServerProvider::Emby),
    ));

    assert_eq!(state.login.method, LoginMethod::Password);
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);
  }

  #[test]
  fn stale_quick_connect_completion_does_not_clear_retry_abort_handle() {
    let mut state = test_state();
    state.login.server_url = "https://media.example.test".to_owned();
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    let stale_session = state.kernel.request_gate.current_session();
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectCancelled,
    ));
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));

    drop(handle_workflow_event(
      &mut state,
      LoginEvent::Login {
        session: stale_session,
        client: Arc::new(JellyfinClient::new()),
        result: Err(LoginError::Request("stale failure".to_owned())),
      },
    ));

    assert!(state.quick_connect_task.is_some());
    assert!(state.kernel.connection == ConnectionPhase::Connecting);
    assert_eq!(state.login.quick_connect, QuickConnectState::Requesting);
  }

  #[test]
  fn stale_profile_load_is_rejected_after_session_storage_completes() {
    let mut state = test_state();
    let session = state.kernel.request_gate.current_session();
    let key = profile_key("new");

    drop(update_login(
      &mut state,
      LoginMessage::SavedSessionStored {
        session,
        result: Ok((key.clone(), Vec::new())),
      },
    ));
    drop(update_login(
      &mut state,
      LoginMessage::ProfilesLoaded {
        revision: 0,
        result: Err(AuthStorageError::Corrupt),
      },
    ));

    assert_eq!(state.login.profiles_revision, 1);
    assert_eq!(state.kernel.active_profile.as_ref(), Some(&key));
    assert!(state.login.error.is_none());
    assert!(!state.login.profiles_loading);
  }

  #[test]
  fn forget_result_is_applied_after_a_new_login_session_starts() {
    let mut state = test_state();
    let key = profile_key("forgotten");
    let forget_session = state.kernel.request_gate.begin_login();
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.active_profile = Some(key.clone());
    state.login.busy_profile = Some(key.clone());
    state.login.forget_confirmation = Some(key.clone());
    let current_session = state.kernel.request_gate.begin_login();
    state.kernel.connection = ConnectionPhase::Connecting;

    drop(update_login(
      &mut state,
      LoginMessage::ForgetFinished {
        session: forget_session,
        key: key.clone(),
        sign_out: true,
        result: Ok(Vec::new()),
      },
    ));

    assert_eq!(state.kernel.request_gate.current_session(), current_session);
    assert_eq!(state.login.profiles_revision, 1);
    assert!(state.login.busy_profile.is_none());
    assert!(state.login.forget_confirmation.is_none());
    assert_eq!(state.kernel.active_profile.as_ref(), Some(&key));
    assert!(state.kernel.connection == ConnectionPhase::Connecting);
  }

  #[test]
  fn stale_restore_completion_does_not_clear_new_restore_busy_key() {
    let mut state = test_state();
    let first_key = profile_key("first");
    let second_key = profile_key("second");
    drop(start_restore(&mut state, first_key.clone()));
    let first_session = state.kernel.request_gate.current_session();
    drop(start_restore(&mut state, second_key.clone()));
    let second_session = state.kernel.request_gate.current_session();

    drop(update_login(
      &mut state,
      LoginMessage::RestoreFinished {
        session: first_session,
        key: first_key,
        result: Err(LoginError::Request("stale failure".to_owned())),
      },
    ));

    assert_eq!(state.kernel.request_gate.current_session(), second_session);
    assert_eq!(state.login.busy_profile.as_ref(), Some(&second_key));
    assert!(state.kernel.connection == ConnectionPhase::Connecting);
    assert!(state.login.error.is_none());
  }

  #[test]
  fn duplicate_forget_confirmation_returns_no_second_task_while_profile_is_busy() {
    let mut state = test_state();
    let key = profile_key("duplicate");
    state.login.forget_confirmation = Some(key.clone());

    let first_task = start_forget(&mut state, key.clone());
    assert!(first_task.is_some());
    drop(first_task);
    let second_task = start_forget(&mut state, key.clone());

    assert!(second_task.is_none());
    assert_eq!(state.login.busy_profile.as_ref(), Some(&key));
    assert!(state.login.forget_confirmation.is_none());
  }

  #[test]
  fn starting_restore_fully_interrupts_quick_connect_state() {
    let mut state = test_state();
    state.login.server_url = "https://media.example.test".to_owned();
    drop(start_quick_connect(&mut state));
    state.login.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());
    let quick_connect_session = state.kernel.request_gate.current_session();
    let key = profile_key("restore");

    drop(start_restore(&mut state, key.clone()));

    assert_ne!(
      state.kernel.request_gate.current_session(),
      quick_connect_session
    );
    assert!(state.quick_connect_task.is_none());
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);
    assert_eq!(state.login.busy_profile.as_ref(), Some(&key));
  }

  #[test]
  fn login_submit_handlers_reject_requests_while_connecting() {
    let mut state = test_state();
    state.kernel.connection = ConnectionPhase::Connecting;
    state.login.server_url = "https://media.example.test".to_owned();
    state.login.username = "ada".to_owned();
    state.login.password = Zeroizing::new("secret".to_owned());
    let session = state.kernel.request_gate.current_session();

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    drop(update_login(&mut state, LoginMessage::PasswordSubmitted));

    assert_eq!(state.kernel.request_gate.current_session(), session);
    assert_eq!(state.login.password.as_str(), "secret");
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);
  }

  #[test]
  fn password_completion_persists_submitted_snapshot_after_form_edits() {
    let mut state = test_state();
    let (settings, _settings_file) = isolated_settings("password-snapshot");
    state.settings = settings;
    state.login.remember = true;
    state.login.provider = MediaServerProvider::Jellyfin;
    let submission = password_submission(
      &state,
      "https://submitted.example.test".to_owned(),
      "submitted-user".to_owned(),
    );

    state.login.server_url = "https://edited.example.test".to_owned();
    state.login.username = "edited-user".to_owned();
    state.login.remember = false;
    state.login.provider = MediaServerProvider::Emby;

    persist_password_submission(&mut state, submission);

    let persisted = state.settings.snapshot();
    assert!(persisted.remembers_login_prefill());
    assert_eq!(
      persisted.login_prefill().server_url(),
      "https://submitted.example.test"
    );
    assert_eq!(persisted.login_prefill().username(), "submitted-user");
    assert_eq!(persisted.login_provider(), "jellyfin");
  }

  #[test]
  fn password_and_restore_failures_use_fixed_user_messages() {
    let mut password_state = test_state();
    let password_session = password_state.kernel.request_gate.begin_login();
    password_state.kernel.connection = ConnectionPhase::Connecting;
    let submission = password_submission(
      &password_state,
      "https://media.example.test".to_owned(),
      "ada".to_owned(),
    );
    drop(update_login(
      &mut password_state,
      LoginMessage::PasswordFinished {
        session: password_session,
        client: Arc::new(JellyfinClient::new()),
        result: Err(LoginError::Request(
          "response included password=secret".to_owned(),
        )),
        submission,
      },
    ));

    let mut restore_state = test_state();
    let key = profile_key("restore-error");
    let restore_session = restore_state.kernel.request_gate.begin_login();
    restore_state.kernel.connection = ConnectionPhase::Connecting;
    restore_state.login.busy_profile = Some(key.clone());
    drop(update_login(
      &mut restore_state,
      LoginMessage::RestoreFinished {
        session: restore_session,
        key,
        result: Err(LoginError::Request(
          "response included access_token=secret".to_owned(),
        )),
      },
    ));

    assert_eq!(
      password_state.login.error.as_deref(),
      Some("Sign-in failed. Check your server, username, and password, then try again.")
    );
    assert_eq!(
      restore_state.login.error.as_deref(),
      Some("Could not restore this saved sign-in. Sign in again to refresh it.")
    );
  }

  fn playback_snapshot(position: f64) -> PlaybackSnapshot {
    PlaybackSnapshot {
      now_playing: Some(jellypilot_mpv::playback::NowPlayingItem {
        item_id: "episode-1".to_owned(),
        title: "Pilot".to_owned(),
        item_type: "Episode".to_owned(),
        runtime_seconds: Some(1_800.0),
        start_position_seconds: 0.0,
        play_method: "Transcode".to_owned(),
      }),
      transport: jellypilot_mpv::PlayerState {
        connected: true,
        paused: false,
        muted: false,
        time_pos: position,
        duration: 1_800.0,
        volume: 75.0,
      },
    }
  }

  fn active_playback_state() -> State {
    let mut state = test_state();
    let now = Instant::now();
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = state.playback_session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: jellypilot_mpv::playback::PlaybackStartPosition::Beginning,
        intro: jellypilot_mpv::playback_session::IntroAvailability {
          mode: jellypilot_session::IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (id, _) = controller_effect(effects);
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Started(Ok(jellypilot_mpv::playback::PlaybackOutcome {
          snapshot: playback_snapshot(10.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    state.playback_view = state.playback_session.view();
    state
  }

  fn controller_effect(
    effects: Vec<PlaybackEffect>,
  ) -> (
    jellypilot_mpv::playback_session::EffectId,
    ControllerCommand,
  ) {
    let [PlaybackEffect::Controller(id, command)] = effects.as_slice() else {
      panic!("expected one controller effect");
    };
    (*id, command.clone())
  }

  fn active_intro_prompt_state() -> State {
    let mut state = test_state();
    let now = Instant::now();
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = state.playback_session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: jellypilot_mpv::playback::PlaybackStartPosition::Beginning,
        intro: jellypilot_mpv::playback_session::IntroAvailability {
          mode: jellypilot_session::IntroSkipMode::Manual,
          skipper_available: true,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (start_id, _) = controller_effect(effects);
    let auxiliary = state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: start_id,
        settlement: ControllerSettlement::Started(Ok(jellypilot_mpv::playback::PlaybackOutcome {
          snapshot: playback_snapshot(0.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    let intro_id = auxiliary
      .iter()
      .find_map(|effect| match effect {
        PlaybackEffect::FetchIntroRanges(id, _) => Some(*id),
        PlaybackEffect::Controller(_, _) | PlaybackEffect::LookupAdjacent(_, _) => None,
      })
      .expect("active intro playback should fetch ranges");
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::IntroRangesSettled {
        id: intro_id,
        result: Ok(vec![jellypilot_session::IntroSkipRange {
          kind: jellypilot_session::IntroSkipKind::Introduction,
          start_seconds: 10.0,
          end_seconds: 30.0,
          notified: false,
          skipped: false,
        }]),
      }),
      now,
    );
    let (refresh_id, _) = controller_effect(
      state
        .playback_session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    let effects = state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: refresh_id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(10.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      }),
      now,
    );
    let (prompt_id, command) = controller_effect(effects);
    assert!(matches!(command, ControllerCommand::ShowText { .. }));
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: prompt_id,
        settlement: ControllerSettlement::OsdShown(Ok(())),
      }),
      now,
    );
    state.playback_view = state.playback_session.view();
    assert!(state.playback_view.intro_prompt.is_some());
    state
  }

  #[test]
  fn seek_release_keeps_committed_preview_while_queued_behind_refresh() {
    let mut state = active_playback_state();
    let now = Instant::now();
    let (refresh_id, command) = controller_effect(
      state
        .playback_session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    state.playback_view = state.playback_session.view();
    state.seek_preview = Some(120.0);

    drop(update_playback(&mut state, PlaybackMessage::SeekReleased));
    drop(update_playback(
      &mut state,
      PlaybackMessage::ControllerSettled {
        id: refresh_id,
        settlement: Box::new(ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(10.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        }),
        started: None,
        tracks: None,
      },
    ));

    assert_eq!(state.seek_preview, Some(120.0));
    assert!(state.playback_view.busy);
  }

  #[test]
  fn volume_release_keeps_committed_preview_while_queued_behind_refresh() {
    let mut state = active_playback_state();
    let now = Instant::now();
    let (refresh_id, _) = controller_effect(
      state
        .playback_session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    state.playback_view = state.playback_session.view();
    state.volume_preview = Some(42.0);

    drop(update_playback(&mut state, PlaybackMessage::VolumeReleased));
    drop(update_playback(
      &mut state,
      PlaybackMessage::ControllerSettled {
        id: refresh_id,
        settlement: Box::new(ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(10.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        }),
        started: None,
        tracks: None,
      },
    ));

    assert_eq!(state.volume_preview, Some(42.0));
    assert!(state.playback_view.busy);
  }

  #[test]
  fn seek_change_during_refresh_keeps_the_draft_and_the_release_commits() {
    let mut state = active_playback_state();
    let now = Instant::now();
    let (_refresh_id, command) = controller_effect(
      state
        .playback_session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    state.playback_view = state.playback_session.view();
    assert!(state.playback_view.busy);

    drop(update_playback(
      &mut state,
      PlaybackMessage::SeekChanged(5.0),
    ));
    assert_eq!(state.seek_preview, Some(5.0));

    drop(update_playback(&mut state, PlaybackMessage::SeekReleased));
    assert_eq!(state.seek_preview, Some(5.0));
    assert!(state.playback_view.busy);
  }

  #[test]
  fn volume_change_during_refresh_keeps_the_draft_and_the_release_commits() {
    let mut state = active_playback_state();
    let now = Instant::now();
    let (_refresh_id, command) = controller_effect(
      state
        .playback_session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    state.playback_view = state.playback_session.view();
    assert!(state.playback_view.busy);

    drop(update_playback(
      &mut state,
      PlaybackMessage::VolumeChanged(42.0),
    ));
    assert_eq!(state.volume_preview, Some(42.0));

    drop(update_playback(&mut state, PlaybackMessage::VolumeReleased));
    assert_eq!(state.volume_preview, Some(42.0));
    assert!(state.playback_view.busy);
  }

  #[test]
  fn inactive_playback_clears_artwork_previews_and_popover_state() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.audio_menu_open = true;
    state.subtitle_menu_open = true;
    state.seek_preview = Some(42.0);
    state.volume_preview = Some(80.0);

    drop(clear_inactive_playback(&mut state));

    assert!(!state.audio_menu_open);
    assert!(!state.subtitle_menu_open);
    assert_eq!(state.seek_preview, None);
    assert_eq!(state.volume_preview, None);
  }

  #[test]
  fn player_artwork_rebind_releases_the_previous_decoded_handle() {
    let mut state = test_state();
    let old_slot = state.kernel.artwork_binder.bind_player_bar();
    state.playback_artwork = Some(ArtworkCell {
      slot: old_slot,
      image_id: "old-image".to_owned(),
      state: ArtworkCellState::Ready,
    });
    state.kernel.artwork_handles.insert(
      old_slot,
      "old-image".to_owned(),
      image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
    );
    let mut playable = episode("episode-1", 1);
    playable.artwork_image_id = Some("new-image".to_owned());
    state.playback_playable = Some(Playable::Library(playable));
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));

    drop(prepare_player_artwork(&mut state));

    assert!(state
      .kernel
      .artwork_handles
      .get(old_slot, "old-image")
      .is_none());
    assert_ne!(
      state.playback_artwork.as_ref().map(|cell| cell.slot),
      Some(old_slot)
    );
  }

  #[test]
  fn clearing_playback_releases_the_current_decoded_player_handle() {
    let mut state = test_state();
    let slot = state.kernel.artwork_binder.bind_player_bar();
    state.playback_artwork = Some(ArtworkCell {
      slot,
      image_id: "player-image".to_owned(),
      state: ArtworkCellState::Ready,
    });
    state.kernel.artwork_handles.insert(
      slot,
      "player-image".to_owned(),
      image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
    );

    drop(clear_inactive_playback(&mut state));

    assert!(state
      .kernel
      .artwork_handles
      .get(slot, "player-image")
      .is_none());
    assert!(state.playback_artwork.is_none());
  }

  #[test]
  fn seek_intent_is_emitted_for_an_active_timeline_regardless_of_busy() {
    assert!(matches!(
      seek_intent(125.0, Some(100.0), true),
      Some(PlaybackIntent::Seek(100.0))
    ));
    assert!(seek_intent(10.0, None, true).is_none());
    assert!(seek_intent(10.0, Some(100.0), false).is_none());
    assert!(seek_intent(f64::NAN, Some(100.0), true).is_none());
  }

  #[test]
  fn volume_intent_is_emitted_for_active_playback_regardless_of_busy() {
    assert!(matches!(
      volume_intent(125.0, true),
      Some(PlaybackIntent::SetVolume(100.0))
    ));
    assert!(volume_intent(50.0, false).is_none());
    assert!(volume_intent(f64::INFINITY, true).is_none());
  }

  fn playstate_action(command: &str, seek_position_ticks: Option<i64>) -> RemoteCommandAction {
    remote_playstate_action(PlaystateRequest {
      command: command.to_owned(),
      seek_position_ticks,
    })
    .expect("supported command should map")
  }

  fn general_action(
    name: &str,
    arguments: Option<serde_json::Value>,
    muted: Option<bool>,
  ) -> RemoteCommandAction {
    remote_general_action(
      GeneralCommand {
        name: name.to_owned(),
        arguments,
      },
      muted,
    )
    .expect("supported command should map")
  }

  #[test]
  fn remote_playstate_commands_map_to_session_intents() {
    assert!(matches!(
      playstate_action("Pause", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SetPaused(true))
    ));
    assert!(matches!(
      playstate_action("Unpause", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SetPaused(false))
    ));
    assert!(matches!(
      playstate_action("PlayPause", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::TogglePaused)
    ));
    assert!(matches!(
      playstate_action("Seek", Some(125_000_000)),
      RemoteCommandAction::Intent(RemotePlaybackIntent::Seek(12.5))
    ));
    assert!(matches!(
      playstate_action("Stop", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::Stop)
    ));
    assert!(matches!(
      playstate_action("NextTrack", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::PlayAdjacent(AdjacentDirection::Next))
    ));
    assert!(matches!(
      playstate_action("PreviousTrack", None),
      RemoteCommandAction::Intent(RemotePlaybackIntent::PlayAdjacent(
        AdjacentDirection::Previous
      ))
    ));
  }

  #[test]
  fn remote_general_commands_accept_wire_values_and_map_to_session_intents() {
    for (value, expected) in [
      (serde_json::json!("52.5"), 52.5),
      (serde_json::json!(125), 100.0),
      (serde_json::json!(-5), 0.0),
    ] {
      assert!(matches!(
        general_action("SetVolume", Some(serde_json::json!({ "Volume": value })), None),
        RemoteCommandAction::Intent(RemotePlaybackIntent::SetVolume(volume))
          if volume == expected
      ));
    }
    assert!(matches!(
      general_action("ToggleMute", None, Some(false)),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SetMuted(true))
    ));
    assert!(matches!(
      general_action(
        "SetAudioStreamIndex",
        Some(serde_json::json!({ "Index": "4" })),
        None,
      ),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SelectAudioStream(4))
    ));
    assert!(matches!(
      general_action(
        "SetSubtitleStreamIndex",
        Some(serde_json::json!({ "Index": -1 })),
        None,
      ),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SelectSubtitleStream(None))
    ));
    assert!(matches!(
      general_action(
        "SetSubtitleStreamIndex",
        Some(serde_json::json!({ "Index": 7 })),
        None,
      ),
      RemoteCommandAction::Intent(RemotePlaybackIntent::SelectSubtitleStream(Some(7)))
    ));
  }

  #[test]
  fn remote_play_carries_source_and_track_selection() {
    let action = remote_play_action(PlayRequest {
      item_ids: vec!["episode-1".to_owned(), "episode-2".to_owned()],
      start_position_ticks: Some(75_000_000),
      play_command: "PlayNow".to_owned(),
      media_source_id: Some("source-2".to_owned()),
      audio_stream_index: Some(4),
      subtitle_stream_index: Some(7),
    })
    .expect("play request should map");

    assert!(matches!(
      action,
      RemoteCommandAction::Play {
        item_id,
        start_position_ticks: Some(75_000_000),
        selection: PlaybackSelection {
          media_source_id: Some(source),
          audio_stream_index: Some(4),
          subtitle_stream_index: Some(7),
        },
      } if item_id == "episode-1" && source == "source-2"
    ));
  }

  #[test]
  fn provider_stream_index_maps_to_current_mpv_track_id() {
    let mut state = test_state();
    state.playback_view.tracks = TracksView::Ready {
      tracks: vec![
        jellypilot_mpv::playback::TrackInfo {
          id: 2,
          track_type: "audio".to_owned(),
          title: None,
          language: None,
          selected: false,
          provider_index: Some(4),
        },
        jellypilot_mpv::playback::TrackInfo {
          id: 6,
          track_type: "sub".to_owned(),
          title: None,
          language: None,
          selected: false,
          provider_index: Some(7),
        },
      ],
      audio: None,
      subtitle: None,
    };

    assert!(matches!(
      RemotePlaybackIntent::SelectAudioStream(4).into_playback_intent(&state.playback_view),
      Some(PlaybackIntent::SelectAudioTrack(2))
    ));
    assert!(matches!(
      RemotePlaybackIntent::SelectSubtitleStream(Some(7))
        .into_playback_intent(&state.playback_view),
      Some(PlaybackIntent::SelectSubtitleTrack(Some(6)))
    ));
  }

  #[test]
  fn remote_track_selection_without_loaded_mapping_is_ignored_with_diagnostic() {
    let mut state = test_state();
    state.playback_view.tracks = TracksView::Unavailable;
    let remote = state.playback_remote;

    drop(handle_remote_command(
      &mut state,
      remote,
      JellyfinCommand::GeneralCommand(GeneralCommand {
        name: "SetAudioStreamIndex".to_owned(),
        arguments: Some(serde_json::json!({ "Index": 4 })),
      }),
    ));

    assert_eq!(
      state.kernel.notice.as_deref(),
      Some(REMOTE_TRACKS_UNAVAILABLE_NOTICE)
    );
  }
  #[test]
  fn local_stop_invalidates_an_in_flight_remote_play_resolution() {
    let mut state = test_state();
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      Instant::now(),
    );
    sync_playback_projection(&mut state);
    let stale_play = state.kernel.request_gate.begin_remote_play();
    drop(update_playback(
      &mut state,
      PlaybackMessage::Intent(PlaybackIntent::Stop),
    ));
    assert!(!state.kernel.request_gate.is_current_remote_play(stale_play));
    let remote = state.playback_remote;

    drop(update_remote(
      &mut state,
      RemoteMessage::PlayResolved {
        remote,
        play: stale_play,
        result: Box::new(Ok(Playable::Media(jellypilot_media_server::MediaItem {
          id: "episode-1".to_owned(),
          name: "Pilot".to_owned(),
          item_type: "Episode".to_owned(),
          series_id: Some("series-1".to_owned()),
          series_name: Some("Series".to_owned()),
          season_name: None,
          index_number: Some(1),
          parent_index_number: Some(1),
          run_time_ticks: Some(1_800_000_000),
          overview: None,
          series_primary_image_tag: None,
        }))),
        start_position_ticks: None,
        selection: PlaybackSelection::default(),
      },
    ));

    assert!(state.playback_view.busy);
    assert!(state.playback_view.now_playing.is_none());
  }

  #[test]
  fn local_adjacent_starts_invalidate_an_in_flight_remote_play_resolution() {
    for direction in [AdjacentDirection::Previous, AdjacentDirection::Next] {
      let mut state = test_state();
      let stale_play = state.kernel.request_gate.begin_remote_play();

      drop(update_playback(
        &mut state,
        PlaybackMessage::Intent(PlaybackIntent::PlayAdjacent(direction)),
      ));

      assert!(!state.kernel.request_gate.is_current_remote_play(stale_play));
    }
  }

  #[test]
  fn double_adjacent_press_dispatches_single_start() {
    let mut state = test_state();
    let now = Instant::now();
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = state.playback_session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: jellypilot_mpv::playback::PlaybackStartPosition::Beginning,
        intro: jellypilot_mpv::playback_session::IntroAvailability {
          mode: jellypilot_session::IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (id, _) = controller_effect(effects);
    let aux = state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Started(Ok(jellypilot_mpv::playback::PlaybackOutcome {
          snapshot: playback_snapshot(10.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    state.playback_view = state.playback_session.view();
    let next_id = aux
      .iter()
      .find_map(|effect| match effect {
        PlaybackEffect::LookupAdjacent(id, AdjacentDirection::Next) => Some(*id),
        _ => None,
      })
      .expect("expected next lookup effect");

    // Settle next adjacent item
    state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
        id: next_id,
        direction: AdjacentDirection::Next,
        result: Ok(Some(media_item("episode-2"))),
      }),
      now,
    );
    sync_playback_projection(&mut state);

    // First adjacent press
    let first_effects = state.playback_session.handle(
      PlaybackInput::Intent(PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)),
      now,
    );
    let (start_id, _) = controller_effect(first_effects);
    sync_playback_projection(&mut state);
    assert!(state.playback_view.busy);

    // Second adjacent press while first is in flight (suppressed)
    let second_effects = state.playback_session.handle(
      PlaybackInput::Intent(PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)),
      now,
    );
    assert!(second_effects.is_empty());

    // Settle the start
    let settle_effects = state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: start_id,
        settlement: ControllerSettlement::Started(Ok(jellypilot_mpv::playback::PlaybackOutcome {
          snapshot: playback_snapshot(0.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    sync_playback_projection(&mut state);

    // No second start effect dispatched
    assert!(!settle_effects
      .iter()
      .any(|e| matches!(e, PlaybackEffect::Controller(_, _))));
    assert!(!state.playback_view.busy);
    assert!(state.playback_view.now_playing.is_some());
  }

  #[test]
  fn double_stop_dispatches_single_stop_and_produces_no_notice() {
    let mut state = active_playback_state();
    let now = Instant::now();

    // First stop
    let first_effects = state
      .playback_session
      .handle(PlaybackInput::Intent(PlaybackIntent::Stop), now);
    let (stop_id, _) = controller_effect(first_effects);
    sync_playback_projection(&mut state);
    assert!(state.playback_view.busy);

    // Second stop while first is in flight
    let second_effects = state
      .playback_session
      .handle(PlaybackInput::Intent(PlaybackIntent::Stop), now);
    assert!(second_effects.is_empty());

    // Settle the stop
    let settle_effects = state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: stop_id,
        settlement: ControllerSettlement::Stopped(Ok(
          jellypilot_mpv::playback::PlaybackStopOutcome {
            warnings: Vec::new(),
          },
        )),
      }),
      now,
    );
    sync_playback_projection(&mut state);

    // Stop settled with no notice
    assert!(settle_effects.is_empty());
    assert!(!state.playback_view.busy);
    assert!(state.playback_view.now_playing.is_none());
    assert!(state.playback_view.notice.is_none());
    assert!(state.playback_notice.is_none());
    assert!(state.kernel.active_toast.is_none());
  }

  #[test]
  fn stop_and_eof_produce_no_visible_notice_state() {
    let mut state = active_playback_state();
    let now = Instant::now();

    let refresh_effects = state
      .playback_session
      .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now);
    let (refresh_id, _) = controller_effect(refresh_effects);

    // Simulate EOF refresh settlement
    let settle_effects = state.playback_session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: refresh_id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(10.0),
            state: PlaybackRefreshState::Ended(PlaybackEndReason::EndOfFile),
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      }),
      now,
    );
    sync_playback_projection(&mut state);

    assert!(settle_effects.is_empty());
    assert!(state.playback_view.now_playing.is_none());
    assert!(state.playback_view.notice.is_none());
    assert!(state.playback_notice.is_none());
    assert!(state.kernel.active_toast.is_none());
  }

  #[test]
  fn failure_produces_toast_that_clears_on_timeout_message() {
    let mut state = test_state();
    assert!(state.kernel.active_toast.is_none());

    let task = state.show_toast(NoticeLevel::Error, "Playback failed: decoder error");
    assert!(task.units() > 0);
    assert!(state.kernel.active_toast.is_some());
    let toast = state.kernel.active_toast.as_ref().unwrap();
    assert_eq!(toast.id, 1);
    assert_eq!(toast.message, "Playback failed: decoder error");
    assert_eq!(toast.level, NoticeLevel::Error);

    // Dismissing with matching ID clears the toast
    drop(update(&mut state, Message::DismissNotice(1)));
    assert!(state.kernel.active_toast.is_none());
  }

  #[test]
  fn newer_notice_replaces_older_and_older_timeout_does_not_dismiss_newer() {
    let mut state = test_state();

    drop(state.show_toast(NoticeLevel::Warning, "Warning 1"));
    assert_eq!(state.kernel.active_toast.as_ref().unwrap().id, 1);
    assert_eq!(
      state.kernel.active_toast.as_ref().unwrap().message,
      "Warning 1"
    );

    drop(state.show_toast(NoticeLevel::Error, "Error 2"));
    assert_eq!(state.kernel.active_toast.as_ref().unwrap().id, 2);
    assert_eq!(
      state.kernel.active_toast.as_ref().unwrap().message,
      "Error 2"
    );

    // Timeout from older notice (id: 1) arrives -> does NOT clear newer notice (id: 2)
    drop(update(&mut state, Message::DismissNotice(1)));
    assert!(state.kernel.active_toast.is_some());
    assert_eq!(state.kernel.active_toast.as_ref().unwrap().id, 2);

    // Timeout or manual dismiss for newer notice (id: 2) -> clears it
    drop(update(&mut state, Message::DismissNotice(2)));
    assert!(state.kernel.active_toast.is_none());
  }

  #[test]
  fn unavailable_remote_target_does_not_dispatch_commands() {
    let mut state = test_state();
    state.remote_control_state = RemoteControlState::Unavailable;
    let pending = state.kernel.request_gate.begin_remote_play();
    let remote = state.playback_remote;
    drop(update_remote(
      &mut state,
      RemoteMessage::Event {
        remote,
        event: JellyfinWebSocketEvent::Command(JellyfinCommand::Play(PlayRequest {
          item_ids: vec!["episode-1".to_owned()],
          start_position_ticks: None,
          play_command: "PlayNow".to_owned(),
          media_source_id: None,
          audio_stream_index: None,
          subtitle_stream_index: None,
        })),
      },
    ));

    assert!(state.kernel.request_gate.is_current_remote_play(pending));
  }

  #[test]
  fn successful_reconnect_clears_only_the_connection_lost_notice() {
    let mut state = test_state();
    state.kernel.notice = Some(REMOTE_CONNECTION_LOST_NOTICE.to_owned());
    let remote = state.playback_remote;

    drop(update_remote(
      &mut state,
      RemoteMessage::Finalized {
        remote,
        result: Ok(true),
      },
    ));

    assert!(state.kernel.notice.is_none());
    state.kernel.notice = Some("Unrelated notice".to_owned());
    drop(update_remote(
      &mut state,
      RemoteMessage::Finalized {
        remote,
        result: Ok(true),
      },
    ));
    assert_eq!(state.kernel.notice.as_deref(), Some("Unrelated notice"));
  }

  #[test]
  fn reconnect_stays_connecting_until_capability_registration_finishes() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.remote_control_state = RemoteControlState::Lost;
    let remote = state.playback_remote;

    let task = update_remote(
      &mut state,
      RemoteMessage::Event {
        remote,
        event: JellyfinWebSocketEvent::Reconnected,
      },
    );

    assert_eq!(task.units(), 1);
    assert_eq!(state.remote_control_state, RemoteControlState::Connecting);
    drop(update_remote(
      &mut state,
      RemoteMessage::Finalized {
        remote,
        result: Ok(false),
      },
    ));
    assert_eq!(state.remote_control_state, RemoteControlState::Available);
  }

  #[test]
  fn initial_setup_failure_invalidates_a_later_finalization_success() {
    let mut state = test_state();
    state.kernel.connection = ConnectionPhase::Connected;
    let stale_remote = state.playback_remote;

    drop(update_remote(
      &mut state,
      RemoteMessage::Started {
        remote: stale_remote,
        result: Err(RemoteStartError::CapabilityRegistrationFailed),
      },
    ));
    drop(update_remote(
      &mut state,
      RemoteMessage::Finalized {
        remote: stale_remote,
        result: Ok(true),
      },
    ));

    assert_eq!(state.remote_control_state, RemoteControlState::Unavailable);
    assert!(!state.kernel.request_gate.is_current_remote(stale_remote));
  }

  #[test]
  fn settings_intro_mode_threads_into_playback_availability() {
    let (settings, _file) = isolated_settings("intro-availability");
    let mut state = test_state();
    state.settings = settings;
    for (configured, expected) in [
      (
        jellypilot_core::config::IntroMode::Automatic,
        jellypilot_session::IntroSkipMode::Automatic,
      ),
      (
        jellypilot_core::config::IntroMode::Manual,
        jellypilot_session::IntroSkipMode::Manual,
      ),
      (
        jellypilot_core::config::IntroMode::Off,
        jellypilot_session::IntroSkipMode::Off,
      ),
    ] {
      state
        .settings
        .set_intro_mode(configured)
        .expect("isolated settings should save");
      let availability = state.intro_availability();
      assert_eq!(availability.mode, expected);
      assert!(!availability.skipper_available);
    }
  }

  #[test]
  fn intro_mode_mutation_updates_the_active_playback_session() {
    let (settings, _file) = isolated_settings("live-intro-mode");
    let mut state = active_intro_prompt_state();
    state.settings = settings;
    state.settings_view =
      crate::app::state::SettingsState::from_settings(state.settings.snapshot());

    drop(update_settings(
      &mut state,
      SettingsMessage::IntroModeSelected(jellypilot_core::config::IntroMode::Off),
    ));

    assert_eq!(
      state.settings.snapshot().intro_mode(),
      jellypilot_core::config::IntroMode::Off
    );
    assert!(state.playback_view.intro_prompt.is_none());
  }

  #[test]
  fn login_state_changes_only_after_remote_teardown_completion() {
    let mut state = test_state();
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));

    drop(stop_remote_session_for_login(&mut state));

    assert!(state.kernel.connection == ConnectionPhase::Connected);
    assert!(state.kernel.client.is_some());
    drop(update_login(&mut state, LoginMessage::RemoteDisconnected));
    assert!(state.kernel.connection == ConnectionPhase::SignedOut);
    assert!(state.kernel.client.is_none());
  }

  #[tokio::test]
  async fn websocket_teardown_waits_for_inflight_setup_to_release_lifecycle() {
    let lifecycle = Arc::new(tokio::sync::Mutex::new(()));
    let setup = lifecycle.lock().await;
    let session = RemoteSessionHandle {
      websocket: Arc::new(JellyfinWebSocket::new()),
      lifecycle: Arc::clone(&lifecycle),
    };
    let teardown = tokio::spawn(disconnect_remote_session(session));
    tokio::task::yield_now().await;

    assert!(!teardown.is_finished());
    drop(setup);
    tokio::time::timeout(std::time::Duration::from_secs(1), teardown)
      .await
      .expect("teardown should finish after setup releases the lifecycle")
      .expect("teardown task should finish");
  }

  #[test]
  fn quit_exit_stays_blocked_until_the_session_cleanup_handshake_settles() {
    let mut state = test_state();
    state.quit_requested = true;

    assert!(!quit_may_exit(&state));
    state.playback_view.quit_may_proceed = true;
    assert!(quit_may_exit(&state));
    state.remote_stopping = true;
    assert!(!quit_may_exit(&state));
  }

  #[test]
  fn close_without_an_available_tray_uses_the_quit_cleanup_handshake() {
    let mut state = test_state();

    drop(update_window(
      &mut state,
      WindowMessage::CloseRequested(iced::window::Id::unique()),
    ));

    assert!(state.quit_requested);
    assert!(state.playback_view.quit_may_proceed);
  }
  #[test]
  fn window_resize_updates_the_tracked_window_size() {
    let mut state = test_state();
    let size = iced::Size::new(1024.0, 768.0);

    drop(update_window(&mut state, WindowMessage::Resized(size)));

    assert_eq!(state.window_size, size);
  }

  #[test]
  fn login_is_gated_by_playback_cleanup_with_fixed_copy() {
    let mut state = test_state();
    state.playback_view.can_start_login = false;

    assert!(!playback_allows_login(&mut state));
    assert_eq!(
      state.login.error.as_deref(),
      Some("Finishing external playback shutdown. Try again in a moment.")
    );
  }
  #[test]
  fn target_name_mutation_requests_live_remote_refinalization() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-target-name-{}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let mut state = test_state();
    state.settings = SettingsStore::for_test(path.clone());
    state.settings_view =
      crate::app::state::SettingsState::from_settings(state.settings.snapshot());
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.remote_session = Some(RemoteSessionHandle {
      websocket: Arc::new(JellyfinWebSocket::new()),
      lifecycle: Arc::new(tokio::sync::Mutex::new(())),
    });
    state.settings_view.playback_target_name_input = "Bedroom".to_owned();
    state.remote_control_state = RemoteControlState::Available;

    let task = update_settings(&mut state, SettingsMessage::SavePlaybackTargetName);

    assert_eq!(
      state.settings.snapshot().playback_target_name(),
      Some("Bedroom")
    );
    assert!(state.kernel.diagnostics.rows().any(|event| {
      event.message == "Playback target name changed; remote registration requested."
    }));
    assert_eq!(task.units(), 1);
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn connecting_target_name_mutation_schedules_no_duplicate_registration() {
    let (settings, _file) = isolated_settings("connecting-target-name");
    let mut state = test_state();
    state.settings = settings;
    state.settings_view =
      crate::app::state::SettingsState::from_settings(state.settings.snapshot());
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.remote_session = Some(RemoteSessionHandle {
      websocket: Arc::new(JellyfinWebSocket::new()),
      lifecycle: Arc::new(tokio::sync::Mutex::new(())),
    });
    state.remote_control_state = RemoteControlState::Connecting;
    state.settings_view.playback_target_name_input = "Bedroom".to_owned();

    let task = update_settings(&mut state, SettingsMessage::SavePlaybackTargetName);

    assert_eq!(task.units(), 0);
    assert!(!state.kernel.diagnostics.rows().any(|event| {
      event.message == "Playback target name changed; remote registration requested."
    }));
  }

  #[test]
  fn saving_mpv_path_discovers_a_missing_playback_controller() {
    let (settings, _file) = isolated_settings("discover-mpv-path");
    let mut state = test_state();
    state.settings = settings;
    state.settings_view =
      crate::app::state::SettingsState::from_settings(state.settings.snapshot());
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.settings_view.mpv_path_input = std::env::current_exe()
      .expect("test executable path should resolve")
      .to_string_lossy()
      .into_owned();

    let task = update_settings(&mut state, SettingsMessage::SaveMpvPath);

    assert_eq!(task.units(), 0);
    assert!(state.playback_controller.is_some());
    assert!(state.playback_view.engine_available);
    assert!(state.playback_notice.is_none());
  }

  #[test]
  fn escape_and_leaving_settings_both_clear_shortcut_capture() {
    let mut state = test_state();
    state.destination = Destination::Settings;
    state.settings_view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Next);

    drop(update_settings(
      &mut state,
      SettingsMessage::CancelShortcutCapture,
    ));
    assert!(state.settings_view.shortcut_capture.is_none());

    state.settings_view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Previous);
    drop(navigate(&mut state, Destination::Home));
    assert_eq!(state.destination, Destination::Home);
    assert!(state.settings_view.shortcut_capture.is_none());
  }

  #[test]
  fn sign_out_starts_secure_profile_removal_while_disconnect_does_not() {
    let key = profile_key("active");
    let mut disconnect = test_state();
    disconnect.kernel.connection = ConnectionPhase::Connected;
    disconnect.kernel.active_profile = Some(key.clone());

    drop(update_settings(
      &mut disconnect,
      SettingsMessage::Disconnect,
    ));

    assert!(disconnect.login.busy_profile.is_none());

    let mut sign_out = test_state();
    sign_out.kernel.connection = ConnectionPhase::Connected;
    sign_out.kernel.active_profile = Some(key.clone());
    drop(update_settings(&mut sign_out, SettingsMessage::SignOut));

    assert_eq!(sign_out.login.busy_profile.as_ref(), Some(&key));
  }

  #[test]
  fn settings_mutation_errors_use_fixed_inline_copy() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-settings-error-{}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let mut state = test_state();
    state.settings = SettingsStore::for_test(path);
    state.settings_view =
      crate::app::state::SettingsState::from_settings(state.settings.snapshot());
    state.settings_view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Next);

    drop(update_settings(
      &mut state,
      SettingsMessage::ShortcutCaptured("Shift+<".to_owned()),
    ));

    assert_eq!(
      state.settings_view.error,
      Some("That shortcut is already assigned.")
    );
  }

  #[test]
  fn login_failures_feed_the_sanitized_diagnostics_buffer() {
    let mut state = test_state();
    let revision = state.login.profiles_revision;

    drop(update(
      &mut state,
      Message::Login(LoginMessage::ProfilesLoaded {
        revision,
        result: Err(AuthStorageError::Corrupt),
      }),
    ));

    assert!(state.kernel.diagnostics.rows().any(|event| {
      event.level == DiagnosticLevel::Error && event.category == DiagnosticCategory::Auth
    }));
  }

  #[test]
  fn leave_and_return_with_unchanged_data_preserves_ready_artwork_and_avoids_loading_reset() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let item = episode("item-1", 1);
    let mut item_with_art = item.clone();
    item_with_art.artwork_image_id = Some("art-1".to_owned());
    state
      .home
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item_with_art.clone()],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    state.home.settle_shortcuts(Ok(Vec::new()));

    drop(prepare_home_artwork(&mut state));
    let slot = state
      .home_artwork
      .card(HomeSection::ContinueWatching, "item-1")
      .expect("card slot exists")
      .slot;

    let session = state.kernel.request_gate.current_session();
    drop(update_home(
      &mut state,
      HomeMessage::ArtworkLoaded {
        session,
        slot,
        image_id: "art-1".to_owned(),
        result: Ok(
          jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
            1,
            1,
            vec![1, 2, 3, 4],
          ),
        ),
      },
    ));
    let initial_handle_id = state
      .kernel
      .artwork_handles
      .get(slot, "art-1")
      .expect("initial handle exists")
      .id();

    // Navigate away to Settings
    drop(navigate(&mut state, Destination::Settings));
    assert!(state.home.has_ready_content());

    // Return to Home
    drop(navigate(&mut state, Destination::Home));
    assert!(state.home.has_ready_content());
    assert_eq!(
      state
        .home_artwork
        .card(HomeSection::ContinueWatching, "item-1")
        .map(|cell| cell.state),
      Some(ArtworkCellState::Ready)
    );
    let post_nav_handle_id = state
      .kernel
      .artwork_handles
      .get(slot, "art-1")
      .expect("post-nav handle exists")
      .id();
    assert_eq!(initial_handle_id, post_nav_handle_id);

    // Identical refetch settles without resetting cell to Loading
    let warm_task = prepare_home_artwork(&mut state);
    assert_eq!(warm_task.units(), 0);
    assert_eq!(
      state
        .home_artwork
        .card(HomeSection::ContinueWatching, "item-1")
        .map(|cell| cell.state),
      Some(ArtworkCellState::Ready)
    );
    let refetch_handle_id = state
      .kernel
      .artwork_handles
      .get(slot, "art-1")
      .expect("refetched handle exists")
      .id();
    assert_eq!(initial_handle_id, refetch_handle_id);
  }

  #[test]
  fn home_memory_cache_hit_synchronously_settles_without_retained_handle() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let item = episode("item-2", 1);
    let mut item_with_art = item.clone();
    item_with_art.artwork_image_id = Some("cached-art-2".to_owned());
    state
      .home
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item_with_art],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    state.home.settle_shortcuts(Ok(Vec::new()));

    // Seed the raster cache directly on a fresh state, so no handle is
    // retained for the image when the prepare pass runs.
    state.kernel.artwork_adapter.seed_raster_for_test(
      "cached-art-2",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
        1,
        1,
        vec![10, 20, 30, 40],
      ),
    );
    let warm_task = prepare_home_artwork(&mut state);
    // One sentinel unit reports the aggregate cache-hit telemetry event.
    assert_eq!(warm_task.units(), 1);

    let card_cell = state
      .home_artwork
      .card(HomeSection::ContinueWatching, "item-2")
      .expect("card cell exists");
    assert_eq!(card_cell.state, ArtworkCellState::Ready);
    assert!(state
      .kernel
      .artwork_handles
      .get(card_cell.slot, "cached-art-2")
      .is_some());
  }

  #[test]
  fn artwork_stream_completion_records_one_sanitized_aggregate_event() {
    let mut state = test_state();
    let summary = ArtworkLoadSummary {
      raster_loads: 1,
      memory_loads: 2,
      disk_loads: 1,
      network_loads: 3,
      failed_loads: 1,
      total_duration_millis: 120,
      total_bytes: 4096,
    };

    drop(update(&mut state, Message::ArtworkStreamCompleted(summary)));

    let artwork_events = state
      .kernel
      .diagnostics
      .rows()
      .filter(|row| row.category == DiagnosticCategory::Artwork)
      .count();
    assert_eq!(artwork_events, 1);

    // An empty summary records nothing.
    drop(update(
      &mut state,
      Message::ArtworkStreamCompleted(ArtworkLoadSummary::default()),
    ));
    let artwork_events = state
      .kernel
      .diagnostics
      .rows()
      .filter(|row| row.category == DiagnosticCategory::Artwork)
      .count();
    assert_eq!(artwork_events, 1);
  }

  #[test]
  fn artwork_stream_settlement_applies_each_completion_as_it_arrives() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut items = Vec::new();
    for index in 1..=3 {
      let mut item = episode(&format!("batch-item-{index}"), index);
      item.artwork_image_id = Some(format!("batch-art-{index}"));
      items.push(item);
    }
    state
      .home
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: items,
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    state.home.settle_shortcuts(Ok(Vec::new()));
    drop(prepare_home_artwork(&mut state));

    let image_ids = (1..=3)
      .map(|index| format!("batch-art-{index}"))
      .collect::<Vec<_>>();
    let completions = image_ids
      .iter()
      .enumerate()
      .map(|(index, image_id)| {
        let item_id = image_id.replacen("batch-art", "batch-item", 1);
        let slot = state
          .home_artwork
          .card(HomeSection::ContinueWatching, &item_id)
          .expect("batch card exists")
          .slot;
        let result = if index == 1 {
          Err(jellypilot_media_server::artwork::ArtworkError::FetchFailed)
        } else {
          Ok(
            jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
              1,
              1,
              vec![1, 2, 3, 4],
            ),
          )
        };
        ArtworkLoadCompletion {
          slot,
          image_id: image_id.clone(),
          result,
        }
      })
      .collect::<Vec<ArtworkLoadCompletion>>();
    let session = state.kernel.request_gate.current_session();
    for completion in completions {
      drop(update_home(
        &mut state,
        HomeMessage::ArtworkLoaded {
          session,
          slot: completion.slot,
          image_id: completion.image_id,
          result: completion.result,
        },
      ));
    }

    for index in 1..=3 {
      let item_id = format!("batch-item-{index}");
      let image_id = format!("batch-art-{index}");
      let cell = state
        .home_artwork
        .card(HomeSection::ContinueWatching, &item_id)
        .expect("settled batch card exists");
      if index == 2 {
        assert_eq!(cell.state, ArtworkCellState::Failed);
        assert!(state
          .kernel
          .artwork_handles
          .get(cell.slot, &image_id)
          .is_none());
      } else {
        assert_eq!(cell.state, ArtworkCellState::Ready);
        assert!(state
          .kernel
          .artwork_handles
          .get(cell.slot, &image_id)
          .is_some());
      }
    }
  }

  #[test]
  fn browse_re_navigation_rebuilds_from_the_raster_cache() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let library = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };
    state.home.shortcuts =
      jellypilot_core::LoadState::Ready(vec![jellypilot_media_server::VideoLibraryShortcut {
        id: "movies".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: Some(1),
        artwork_image_id: None,
      }]);

    drop(navigate(&mut state, library.clone()));

    let mut item = episode("browse-item-1", 1);
    item.artwork_image_id = Some("browse-art-1".to_owned());
    state.browse_view = LibraryBrowseView::Ready {
      visible_items: vec![jellypilot_core::browse_model::LibraryItemSlot {
        item: Some(item.clone()),
      }],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 1,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };

    drop(prepare_browse_artwork(&mut state));
    let slot = state.browse_artwork.get("browse-item-1").unwrap().slot;
    let session = state.kernel.request_gate.current_session();
    // A real load stores the raster in the adapter cache; seed it beside the
    // synthesized completion so re-navigation can rebuild from it.
    state.kernel.artwork_adapter.seed_raster_for_test(
      "browse-art-1",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 4]),
    );
    drop(update_browse(
      &mut state,
      BrowseMessage::ArtworkLoaded {
        session,
        slot,
        image_id: "browse-art-1".to_owned(),
        result: Ok(
          jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
            1,
            1,
            vec![1, 2, 3, 4],
          ),
        ),
      },
    ));
    assert_eq!(
      state
        .browse_artwork
        .get("browse-item-1")
        .map(|cell| cell.state),
      Some(ArtworkCellState::Ready)
    );
    assert!(state
      .kernel
      .artwork_handles
      .get(slot, "browse-art-1")
      .is_some());

    // Navigate away to Home
    drop(navigate(&mut state, Destination::Home));

    // Return to Browse
    drop(navigate(&mut state, library));
    state.browse_view = LibraryBrowseView::Ready {
      visible_items: vec![jellypilot_core::browse_model::LibraryItemSlot { item: Some(item) }],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 1,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };
    drop(prepare_browse_artwork(&mut state));

    let browse_cell = state
      .browse_artwork
      .get("browse-item-1")
      .expect("browse cell exists");
    assert_eq!(browse_cell.state, ArtworkCellState::Ready);
    // The handle is rebuilt synchronously from the raster cache; there is no
    // cross-navigation handle identity to preserve.
    assert!(state
      .kernel
      .artwork_handles
      .get(browse_cell.slot, "browse-art-1")
      .is_some());
  }

  #[test]
  fn inflight_loading_artwork_is_not_re_issued_on_followup_prepare() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-item-1");
    item.artwork_image_id = Some("detail-art-1".to_owned());
    state.detail.content = jellypilot_core::LoadState::Ready(DetailContent::Item(item));

    // First prepare starts the initial load
    drop(prepare_detail_artwork(&mut state));
    let cell = state
      .detail_artwork
      .get(DETAIL_POSTER_KEY)
      .expect("poster cell exists");
    let original_slot = cell.slot;
    assert_eq!(cell.state, ArtworkCellState::Loading);

    // Follow-up prepare (e.g. neighbors loaded) does not re-issue or replace the slot
    state.detail.season_neighbors = jellypilot_core::LoadState::Ready(Vec::new());
    let warm_task = prepare_detail_artwork(&mut state);
    assert_eq!(warm_task.units(), 0);
    let second_cell = state
      .detail_artwork
      .get(DETAIL_POSTER_KEY)
      .expect("poster cell exists");
    assert_eq!(second_cell.slot, original_slot);
    assert_eq!(second_cell.state, ArtworkCellState::Loading);
  }

  #[test]
  fn detail_memory_cache_hit_synchronously_settles_without_retained_handle() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-cache-item");
    item.artwork_image_id = Some("detail-cache-art".to_owned());
    state.detail.content = jellypilot_core::LoadState::Ready(DetailContent::Item(item));
    state.kernel.artwork_adapter.seed_raster_for_test(
      "detail-cache-art",
      jellypilot_media_server::artwork::ArtworkSizeClass::Hero,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
        1,
        1,
        vec![10, 20, 30, 40],
      ),
    );
    state.kernel.artwork_handles.clear();
    let warm_task = prepare_detail_artwork(&mut state);
    // One sentinel unit reports the aggregate cache-hit telemetry event.
    assert_eq!(warm_task.units(), 1);

    let poster = state
      .detail_artwork
      .get(DETAIL_POSTER_KEY)
      .expect("detail poster exists");
    assert_eq!(poster.state, ArtworkCellState::Ready);
    assert!(state
      .kernel
      .artwork_handles
      .get(poster.slot, "detail-cache-art")
      .is_some());
  }

  #[test]
  fn browse_memory_cache_hit_synchronously_settles_without_retained_handle() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = episode("browse-cache-item-1", 1);
    item.artwork_image_id = Some("browse-cache-art-1".to_owned());

    // Seed the raster cache directly
    state.kernel.artwork_adapter.seed_raster_for_test(
      "browse-cache-art-1",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
        1,
        1,
        vec![10, 20, 30, 40],
      ),
    );

    // Wipe artwork_handles completely so there is NO retained handle in artwork_handles
    state.kernel.artwork_handles.clear();

    state.browse_view = LibraryBrowseView::Ready {
      visible_items: vec![jellypilot_core::browse_model::LibraryItemSlot { item: Some(item) }],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 1,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };

    let warm_task = prepare_browse_artwork(&mut state);
    // One sentinel unit reports the aggregate cache-hit telemetry event.
    assert_eq!(warm_task.units(), 1);

    let browse_cell = state
      .browse_artwork
      .get("browse-cache-item-1")
      .expect("browse cell exists");
    assert_eq!(browse_cell.state, ArtworkCellState::Ready);
    assert!(state
      .kernel
      .artwork_handles
      .get(browse_cell.slot, "browse-cache-art-1")
      .is_some());
  }

  #[test]
  fn cancelled_artwork_load_does_not_mark_cell_failed_and_is_reloaded_on_revisit() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = episode("item-cancel", 1);
    item.artwork_image_id = Some("art-cancel".to_owned());
    state
      .home
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    state.home.settle_shortcuts(Ok(Vec::new()));

    drop(prepare_home_artwork(&mut state));
    let slot = state
      .home_artwork
      .card(HomeSection::ContinueWatching, "item-cancel")
      .expect("card slot exists")
      .slot;

    // Navigation away cancels pending loads and prunes unready cells
    drop(navigate(&mut state, Destination::Settings));
    assert!(state
      .home_artwork
      .card(HomeSection::ContinueWatching, "item-cancel")
      .is_none());

    // Late cancelled message arriving after leave does not mark failed
    let session = state.kernel.request_gate.current_session();
    drop(update_home(
      &mut state,
      HomeMessage::ArtworkLoaded {
        session,
        slot,
        image_id: "art-cancel".to_owned(),
        result: Err(jellypilot_media_server::artwork::ArtworkError::Cancelled),
      },
    ));

    // Returning to Home re-prepares and binds a fresh load
    drop(navigate(&mut state, Destination::Home));
    drop(prepare_home_artwork(&mut state));
    let new_cell = state
      .home_artwork
      .card(HomeSection::ContinueWatching, "item-cancel")
      .expect("card cell is recreated on revisit");
    assert_eq!(new_cell.state, ArtworkCellState::Loading);
  }

  #[test]
  fn repeated_warm_prepares_maintain_zero_tracked_live_slots() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let item = episode("item-warm", 1);
    let mut item_with_art = item.clone();
    item_with_art.artwork_image_id = Some("art-warm".to_owned());
    state
      .home
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item_with_art],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    state.home.settle_shortcuts(Ok(Vec::new()));

    // Cold prepare allocates 1 live in-flight slot
    drop(prepare_home_artwork(&mut state));
    let cold_slot = state
      .home_artwork
      .card(HomeSection::ContinueWatching, "item-warm")
      .unwrap()
      .slot;
    assert_eq!(state.kernel.artwork_binder.live_slots_count(), 1);

    // Settle the cold load -> live_slots_count becomes 0
    let session = state.kernel.request_gate.current_session();
    drop(update_home(
      &mut state,
      HomeMessage::ArtworkLoaded {
        session,
        slot: cold_slot,
        image_id: "art-warm".to_owned(),
        result: Ok(
          jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
            1,
            1,
            vec![1, 2, 3, 4],
          ),
        ),
      },
    ));
    assert_eq!(state.kernel.artwork_binder.live_slots_count(), 0);

    // Repeated warm prepares (handle reuse / cached hit) must NOT leak live slots in ArtworkBinder
    for _ in 0..10 {
      let warm_task = prepare_home_artwork(&mut state);
      assert_eq!(warm_task.units(), 0);
      assert_eq!(state.kernel.artwork_binder.live_slots_count(), 0);
    }
  }

  #[test]
  fn playback_tick_and_settlement_do_not_project_busy_to_ui() {
    let mut state = active_playback_state();
    assert!(!state.playback_view.busy);

    // Tick intent executes Refresh but must NOT project busy to UI
    drop(update_playback(
      &mut state,
      PlaybackMessage::Intent(PlaybackIntent::Tick),
    ));
    assert!(state.in_flight_refresh.is_some());
    assert_eq!(state.in_flight_command, None);
    assert!(
      !state.playback_view.busy,
      "periodic refresh tick must not mark playback_view busy (prevents button flickering)"
    );

    // Refresh settlement clears in-flight refresh and keeps busy false
    let refresh_id = state.in_flight_refresh.unwrap();
    drop(update_playback(
      &mut state,
      PlaybackMessage::ControllerSettled {
        id: refresh_id,
        settlement: Box::new(ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(11.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        }),
        started: None,
        tracks: None,
      },
    ));
    assert_eq!(state.in_flight_refresh, None);
    assert!(
      !state.playback_view.busy,
      "refresh settlement must keep playback_view busy as false"
    );
  }

  #[test]
  fn playback_refresh_transition_to_queued_command_preserves_busy_state() {
    let mut state = active_playback_state();
    assert!(!state.playback_view.busy);

    // 1. Tick intent starts a Refresh
    drop(update_playback(
      &mut state,
      PlaybackMessage::Intent(PlaybackIntent::Tick),
    ));
    let refresh_id = state
      .in_flight_refresh
      .expect("tick must initiate an in-flight refresh");
    assert_eq!(state.in_flight_command, None);
    assert!(
      !state.playback_view.busy,
      "periodic refresh tick alone must not mark playback_view busy"
    );

    // 2. Queue a seek command while refresh is in flight
    drop(update_playback(
      &mut state,
      PlaybackMessage::Intent(PlaybackIntent::Seek(50.0)),
    ));

    // 3. Settle the in-flight refresh
    drop(update_playback(
      &mut state,
      PlaybackMessage::ControllerSettled {
        id: refresh_id,
        settlement: Box::new(ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(11.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        }),
        started: None,
        tracks: None,
      },
    ));

    // Refresh marker is cleared, command marker is set to the newly dispatched seek command,
    // and playback_view.busy is true
    assert_eq!(state.in_flight_refresh, None);
    let command_id = state
      .in_flight_command
      .expect("settling refresh must dispatch the queued command and set in_flight_command");
    assert!(
      state.playback_view.busy,
      "playback_view.busy must remain true while queued command is in flight"
    );

    // 4. Settle the command
    drop(update_playback(
      &mut state,
      PlaybackMessage::ControllerSettled {
        id: command_id,
        settlement: Box::new(ControllerSettlement::Controlled(Ok(
          jellypilot_mpv::playback::PlaybackOutcome {
            snapshot: playback_snapshot(50.0),
            warnings: Vec::new(),
          },
        ))),
        started: None,
        tracks: None,
      },
    ));
    // Command marker is cleared and busy is false
    assert_eq!(state.in_flight_command, None);
    assert!(
      !state.playback_view.busy,
      "playback_view.busy must be false after command settles"
    );
  }

  #[test]
  fn tray_action_executes_in_update_tray() {
    let mut state = active_playback_state();
    assert_eq!(
      state.playback_view.now_playing.as_ref().map(|np| np.paused),
      Some(false)
    );

    drop(update_tray(&mut state, crate::tray::TrayAction::PlayPause));

    assert_eq!(
      state.playback_view.now_playing.as_ref().map(|np| np.paused),
      Some(true)
    );
  }

  #[test]
  fn page_settle_spawns_all_24_visible_browse_loads_at_once() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let items = (0..24)
      .map(|i| {
        let mut item = episode(&format!("item-{i}"), 1);
        item.artwork_image_id = Some(format!("art-{i}"));
        jellypilot_core::browse_model::LibraryItemSlot { item: Some(item) }
      })
      .collect::<Vec<_>>();

    state.browse_view = LibraryBrowseView::Ready {
      visible_items: items,
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 24,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };

    drop(prepare_browse_artwork(&mut state));

    for i in 0..24 {
      let cell = state
        .browse_artwork
        .get(&format!("item-{i}"))
        .expect("all 24 cells must be present in browse_artwork");
      assert_eq!(
        cell.state,
        ArtworkCellState::Loading,
        "all 24 cells must be admitted into Loading in the same pass"
      );
    }
  }
}
