use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::image;
use iced::Task;
use jellypilot_auth::login::{should_disconnect_after_forget, ConnectionPhase};
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::browse_model::BrowseSource;
use jellypilot_core::config::{IntroMode, Settings};
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::request_gate::RemotePlayToken;
use jellypilot_media_server::artwork::{ArtworkSizeClass, LoadLane};
use jellypilot_media_server::VideoLibraryItem;
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

use crate::tray::TrayAction;

use super::browse;
use super::detail;
use super::home;
use super::login;
use super::message::{
  BrowseMessage, DetailMessage, HomeMessage, LoginMessage, Message, PlaybackMessage, RemoteMessage,
  RemoteSessionStart, RemoteStartError, SettingsMessage, WindowMessage,
};
use super::settings;
use super::state::{
  ArtworkCell, ArtworkCellState, Destination, NoticeLevel, RemoteEventChannel, RemoteSessionHandle,
  State,
};

/// Settings fields whose mutation triggers cross-surface follow-ups (playback
/// reconfiguration, intro-mode input, remote refinalization). The top-level
/// router snapshots them around the settings surface update so it can hoist
/// those follow-up writes (ADR 0029).
struct SettingsPlaybackSnapshot {
  mpv_path: Option<String>,
  mpv_args: Vec<String>,
  subtitle_languages: Vec<String>,
  intro_mode: IntroMode,
  playback_target_name: Option<String>,
}

impl SettingsPlaybackSnapshot {
  fn capture(settings: &Settings) -> Self {
    Self {
      mpv_path: settings.mpv_path().map(str::to_owned),
      mpv_args: settings.mpv_args().to_vec(),
      subtitle_languages: settings.subtitle_languages().to_vec(),
      intro_mode: settings.intro_mode(),
      playback_target_name: settings.playback_target_name().map(str::to_owned),
    }
  }
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  match message {
    Message::Window(message) => update_window(state, message),
    Message::Login(message) => {
      let was_connected = state.kernel.connection == ConnectionPhase::Connected;
      let previous_error = state.login.flow.error.clone();
      // Cross-surface writes hoisted out of the login surface (ADR 0029): a
      // forget that signs out tears the remote session down, and the teardown
      // completion clears the remote stopping flag.
      let forget_disconnect = match &message {
        LoginMessage::ForgetFinished {
          session,
          key,
          sign_out,
          result,
        } => {
          result.is_ok()
            && should_disconnect_after_forget(
              *sign_out,
              *session,
              state.kernel.request_gate.current_session(),
              state.kernel.connection,
              state.kernel.active_profile.as_ref() == Some(key),
            )
        }
        _ => false,
      };
      let remote_disconnected = matches!(message, LoginMessage::RemoteDisconnected);
      let login_task = login::update(
        &mut state.login,
        &mut state.kernel,
        state.playback_view.can_start_login,
        message,
      );
      if remote_disconnected {
        state.remote_stopping = false;
      }
      let login_task = if forget_disconnect {
        Task::batch([
          login_task,
          stop_remote_session_for_login(state).map(Message::Login),
        ])
      } else {
        login_task
      };
      let is_connected = state.kernel.connection == ConnectionPhase::Connected;
      if state.login.flow.error != previous_error {
        if let Some(error) = &state.login.flow.error {
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
          home::start_load(
            &mut state.home,
            &mut state.kernel,
            state.playback_view.now_playing.is_none(),
          ),
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
    Message::Home(HomeMessage::Navigate(destination)) => navigate(state, destination),
    Message::Home(message) => {
      // Cross-surface follow-ups hoisted out of the home surface (ADR 0029): a
      // Loaded settlement re-prepares the artwork pipeline, after which
      // handles are retained here because retention reads every surface's
      // slot set.
      let reprepared_artwork = matches!(message, HomeMessage::Loaded { .. });
      let task = home::update(
        &mut state.home,
        &mut state.kernel,
        state.playback_view.now_playing.is_none(),
        state.window_size.width,
        message,
      );
      if reprepared_artwork {
        state.retain_artwork_handles();
      }
      task
    }
    Message::Browse(BrowseMessage::SearchSubmitted) => {
      // Navigation stays at the top-level router (ADR 0029): the browse
      // surface owns the input text, but submitting it writes the shared
      // destination stack and drives the other surfaces' leave/enter hooks.
      let query = state.browse.search_input.trim().to_owned();
      if query.is_empty() {
        Task::none()
      } else {
        navigate(state, Destination::Search(query))
      }
    }
    Message::Browse(message) => {
      // Cross-surface follow-ups hoisted out of the browse surface (ADR
      // 0029): the router resolves the browse source, library-route,
      // playback-idle, and window-size reads the surface may not perform,
      // and a page-settlement or scroll-window sync re-prepares the artwork
      // pipeline, after which handles are retained here because retention
      // reads every surface's slot set.
      let previous_notice = state.kernel.notice.clone();
      let reprepared_artwork = matches!(
        message,
        BrowseMessage::PageSettled(_) | BrowseMessage::Scrolled(_)
      );
      let source = match &message {
        BrowseMessage::SortChanged(_)
        | BrowseMessage::SortDirectionToggled
        | BrowseMessage::PlayedFilterChanged(_)
        | BrowseMessage::FavoritesToggled => browse_source(state),
        _ => None,
      };
      let task = browse::update(
        &mut state.browse,
        &mut state.kernel,
        source,
        matches!(state.destination, Destination::Library { .. }),
        state.playback_view.now_playing.is_none(),
        state.window_size,
        message,
      );
      if reprepared_artwork {
        state.retain_artwork_handles();
      }
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
    Message::Detail(DetailMessage::Back) => navigate_back(state),
    Message::Detail(message) => {
      // Cross-surface follow-ups hoisted out of the detail surface (ADR
      // 0029): a content/season/neighbors settlement re-prepares the artwork
      // pipeline, after which handles are retained here because retention
      // reads every surface's slot set.
      let reprepared_artwork = matches!(
        message,
        DetailMessage::Loaded { .. }
          | DetailMessage::SeasonLoaded { .. }
          | DetailMessage::NeighborsLoaded { .. }
      );
      let detail_item_id = match &state.destination {
        Destination::Detail(item_id) => Some(item_id.as_str()),
        _ => None,
      };
      let task = detail::update(
        &mut state.detail,
        &mut state.kernel,
        detail_item_id,
        message,
      );
      if reprepared_artwork {
        state.retain_artwork_handles();
      }
      task
    }
    Message::Settings(message) => {
      // Cross-surface writes hoisted out of the settings surface (ADR 0029):
      // mutations that change playback-relevant settings reconfigure playback,
      // re-feed the intro mode, or refinalize the remote target here, and
      // Disconnect/SignOut tear the remote session down or start the login
      // surface's profile forget.
      let settings_before = SettingsPlaybackSnapshot::capture(state.kernel.settings.snapshot());
      let disconnect = matches!(message, SettingsMessage::Disconnect);
      let sign_out = matches!(message, SettingsMessage::SignOut);
      let settings_task = settings::update(&mut state.settings, &mut state.kernel, message);
      let settings_after = SettingsPlaybackSnapshot::capture(state.kernel.settings.snapshot());
      let mut tasks = vec![settings_task];
      if settings_after.mpv_path != settings_before.mpv_path
        || settings_after.mpv_args != settings_before.mpv_args
        || settings_after.subtitle_languages != settings_before.subtitle_languages
      {
        tasks.push(apply_playback_configuration(state));
      }
      if settings_after.intro_mode != settings_before.intro_mode {
        let mode = state.intro_availability().mode;
        tasks.push(apply_playback_input(
          state,
          PlaybackInput::Intent(PlaybackIntent::SetIntroMode(mode)),
        ));
      }
      if settings_after.playback_target_name != settings_before.playback_target_name {
        tasks.push(refinalize_playback_target(state));
      }
      if disconnect {
        tasks.push(stop_remote_session_for_login(state).map(Message::Login));
      } else if sign_out {
        let sign_out_task = match state.kernel.active_profile.clone() {
          Some(key) => login::start_forget(&mut state.login, &mut state.kernel, key)
            .map(|task| task.map(Message::Login))
            .unwrap_or_else(Task::none),
          None => stop_remote_session_for_login(state).map(Message::Login),
        };
        tasks.push(sign_out_task);
      }
      Task::batch(tasks)
    }
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
      let task = browse::sync_scroll_window(&mut state.browse, &mut state.kernel, size);
      // The sync may have re-prepared the artwork pipeline; retention reads
      // every surface's slot set, so it is hoisted to the router (ADR 0029).
      state.retain_artwork_handles();
      task
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

fn apply_playback_configuration(state: &mut State) -> Task<Message> {
  let config = playback_controller_config(state.kernel.settings.snapshot());
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
    .kernel
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
  if let Some(name) = state.kernel.settings.snapshot().playback_target_name() {
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
    playback_controller_config(state.kernel.settings.snapshot()),
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

fn navigate(state: &mut State, destination: Destination) -> Task<Message> {
  let previous = state.destination.clone();
  if !state.navigate_to(destination) {
    return Task::none();
  }
  activate_destination(state, previous)
}

fn open_detail(state: &mut State, item: VideoLibraryItem) -> Task<Message> {
  let item_id = item.id.clone();
  state.detail.items.insert(item_id.clone(), item);
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
    state.settings.view.shortcut_capture = None;
  }
  if previous == Destination::Home && state.destination != Destination::Home {
    home::leave_view(
      &mut state.home,
      &mut state.kernel,
      state.playback_view.now_playing.is_none(),
    );
  } else if matches!(
    previous,
    Destination::Library { .. } | Destination::Search(_)
  ) && !matches!(
    state.destination,
    Destination::Library { .. } | Destination::Search(_)
  ) {
    browse::leave_view(
      &mut state.browse,
      &mut state.kernel,
      state.playback_view.now_playing.is_none(),
    );
  } else if matches!(previous, Destination::Detail(_)) && previous != state.destination {
    detail::leave_view(
      &mut state.detail,
      &mut state.kernel,
      state.playback_view.now_playing.is_none(),
    );
  }

  match &state.destination {
    Destination::Home => home::start_load(
      &mut state.home,
      &mut state.kernel,
      state.playback_view.now_playing.is_none(),
    ),
    Destination::Library { .. } => {
      state.browse.search_input.clear();
      let source = browse_source(state);
      browse::start(
        &mut state.browse,
        &mut state.kernel,
        source,
        state.playback_view.now_playing.is_none(),
      )
    }
    Destination::Search(_) => {
      let source = browse_source(state);
      browse::start(
        &mut state.browse,
        &mut state.kernel,
        source,
        state.playback_view.now_playing.is_none(),
      )
    }
    Destination::Detail(item_id) => {
      detail::start_load(&mut state.detail, &mut state.kernel, Some(item_id))
    }
    Destination::Settings => Task::none(),
  }
}

fn browse_source(state: &State) -> Option<BrowseSource> {
  let session = state.kernel.request_gate.current_session();
  match &state.destination {
    Destination::Library {
      library_id,
      collection_type,
    } => {
      let jellypilot_core::LoadState::Ready(shortcuts) = &state.home.data.shortcuts else {
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

fn reset_connected_surface(state: &mut State) -> Task<Message> {
  let playback_task =
    apply_playback_input(state, PlaybackInput::Intent(PlaybackIntent::Disconnect));
  state.playback_remote = state.kernel.request_gate.begin_remote();
  state.remote_session = None;
  state.remote_events = None;
  state.remote_control_state = RemoteControlState::Unavailable;
  state.remote_stopping = false;
  browse::reset(&mut state.browse, &mut state.kernel);
  state.kernel.artwork_adapter.reset_session();
  state.kernel.artwork_binder.reset();
  state.in_flight_refresh = None;
  state.in_flight_command = None;
  state.home = home::Surface::default();
  state.detail = detail::Surface::default();
  state.kernel.artwork_handles.clear();
  state.navigation_stack.clear();
  state.destination = Destination::Home;
  playback_task
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

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;

  use super::*;
  use crate::app::kernel::Kernel;
  use crate::app::state::LoginState;
  use jellypilot_auth::{AuthStorageError, AuthStore, SavedProfileKey};
  use jellypilot_core::browse_model::LibraryBrowseView;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_media_server::artwork::ArtworkLoadSummary;
  use jellypilot_media_server::{JellyfinClient, MediaServerProvider};
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
    state.home.data.begin_load();

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
    state.home.data.settle_video_home(Err("settled".to_owned()));
    state.home.data.settle_shortcuts(Err("settled".to_owned()));
    drop(update_window(
      &mut state,
      WindowMessage::FrameTick(start + Duration::from_millis(1200)),
    ));
    assert_eq!(state.skeleton_phase, 0.0);
    assert_eq!(state.skeleton_animation_start, None);
  }

  fn test_state() -> State {
    let settings = SettingsStore::default();
    let mut request_gate = jellypilot_core::request_gate::RequestGate::default();
    let playback_remote = request_gate.begin_remote();
    let playback_session = jellypilot_mpv::playback_session::PlaybackSession::default();
    let playback_view = playback_session.view();
    let settings_view = crate::app::state::SettingsState::from_settings(settings.snapshot());
    State {
      smoke: false,
      window_size: iced::Size::new(1600.0, 900.0),
      skeleton_phase: 0.0,
      skeleton_animation_start: None,
      login: crate::app::login::Surface {
        flow: LoginState::from_settings(settings.snapshot()),
        quick_connect_task: None,
      },
      settings: crate::app::settings::Surface {
        view: settings_view,
      },
      kernel: Kernel {
        settings,
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
      playback_notice: None,
      quit_requested: false,
      destination: Destination::Home,
      navigation_stack: Vec::new(),
      detail: detail::Surface::default(),
      home: home::Surface::default(),
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
      browse: browse::Surface::default(),
      audio_menu_open: false,
      subtitle_menu_open: false,
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
    state.kernel.settings = settings;
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
        .kernel
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
    state.kernel.settings = settings;
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());

    drop(update(
      &mut state,
      Message::Settings(SettingsMessage::IntroModeSelected(
        jellypilot_core::config::IntroMode::Off,
      )),
    ));

    assert_eq!(
      state.kernel.settings.snapshot().intro_mode(),
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
    drop(update(
      &mut state,
      Message::Login(LoginMessage::RemoteDisconnected),
    ));
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
  fn target_name_mutation_requests_live_remote_refinalization() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-target-name-{}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    let mut state = test_state();
    state.kernel.settings = SettingsStore::for_test(path.clone());
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.remote_session = Some(RemoteSessionHandle {
      websocket: Arc::new(JellyfinWebSocket::new()),
      lifecycle: Arc::new(tokio::sync::Mutex::new(())),
    });
    state.settings.view.playback_target_name_input = "Bedroom".to_owned();
    state.remote_control_state = RemoteControlState::Available;

    let task = update(
      &mut state,
      Message::Settings(SettingsMessage::SavePlaybackTargetName),
    );

    assert_eq!(
      state.kernel.settings.snapshot().playback_target_name(),
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
    state.kernel.settings = settings;
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());
    state.kernel.connection = ConnectionPhase::Connected;
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.remote_session = Some(RemoteSessionHandle {
      websocket: Arc::new(JellyfinWebSocket::new()),
      lifecycle: Arc::new(tokio::sync::Mutex::new(())),
    });
    state.remote_control_state = RemoteControlState::Connecting;
    state.settings.view.playback_target_name_input = "Bedroom".to_owned();

    let task = update(
      &mut state,
      Message::Settings(SettingsMessage::SavePlaybackTargetName),
    );

    assert_eq!(task.units(), 0);
    assert!(!state.kernel.diagnostics.rows().any(|event| {
      event.message == "Playback target name changed; remote registration requested."
    }));
  }

  #[test]
  fn saving_mpv_path_discovers_a_missing_playback_controller() {
    let (settings, _file) = isolated_settings("discover-mpv-path");
    let mut state = test_state();
    state.kernel.settings = settings;
    state.settings.view =
      crate::app::state::SettingsState::from_settings(state.kernel.settings.snapshot());
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    state.settings.view.mpv_path_input = std::env::current_exe()
      .expect("test executable path should resolve")
      .to_string_lossy()
      .into_owned();

    let task = update(&mut state, Message::Settings(SettingsMessage::SaveMpvPath));

    assert_eq!(task.units(), 0);
    assert!(state.playback_controller.is_some());
    assert!(state.playback_view.engine_available);
    assert!(state.playback_notice.is_none());
  }

  #[test]
  fn escape_and_leaving_settings_both_clear_shortcut_capture() {
    let mut state = test_state();
    state.destination = Destination::Settings;
    state.settings.view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Next);

    drop(update(
      &mut state,
      Message::Settings(SettingsMessage::CancelShortcutCapture),
    ));
    assert!(state.settings.view.shortcut_capture.is_none());

    state.settings.view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Previous);
    drop(navigate(&mut state, Destination::Home));
    assert_eq!(state.destination, Destination::Home);
    assert!(state.settings.view.shortcut_capture.is_none());
  }

  #[test]
  fn sign_out_starts_secure_profile_removal_while_disconnect_does_not() {
    let key = profile_key("active");
    let mut disconnect = test_state();
    disconnect.kernel.connection = ConnectionPhase::Connected;
    disconnect.kernel.active_profile = Some(key.clone());

    drop(update(
      &mut disconnect,
      Message::Settings(SettingsMessage::Disconnect),
    ));

    assert!(disconnect.login.flow.busy_profile.is_none());

    let mut sign_out = test_state();
    sign_out.kernel.connection = ConnectionPhase::Connected;
    sign_out.kernel.active_profile = Some(key.clone());
    drop(update(
      &mut sign_out,
      Message::Settings(SettingsMessage::SignOut),
    ));

    assert_eq!(sign_out.login.flow.busy_profile.as_ref(), Some(&key));
  }

  #[test]
  fn login_failures_feed_the_sanitized_diagnostics_buffer() {
    let mut state = test_state();
    let revision = state.login.flow.profiles_revision;

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
  fn browse_re_navigation_rebuilds_from_the_raster_cache() {
    let mut state = test_state();
    state.kernel.client = Some(Arc::new(JellyfinClient::new()));
    let library = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };
    state.home.data.shortcuts =
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
    state.browse.view = LibraryBrowseView::Ready {
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

    // Mirrors the router: a re-prepared pipeline is followed by cross-surface
    // handle retention (ADR 0029).
    drop(browse::prepare_artwork(
      &mut state.browse,
      &mut state.kernel,
      state.window_size.width,
    ));
    state.retain_artwork_handles();
    let slot = state.browse.artwork.get("browse-item-1").unwrap().slot;
    let session = state.kernel.request_gate.current_session();
    // A real load stores the raster in the adapter cache; seed it beside the
    // synthesized completion so re-navigation can rebuild from it.
    state.kernel.artwork_adapter.seed_raster_for_test(
      "browse-art-1",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 4]),
    );
    drop(browse::update(
      &mut state.browse,
      &mut state.kernel,
      None,
      false,
      state.playback_view.now_playing.is_none(),
      state.window_size,
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
        .browse
        .artwork
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
    state.browse.view = LibraryBrowseView::Ready {
      visible_items: vec![jellypilot_core::browse_model::LibraryItemSlot { item: Some(item) }],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 1,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };
    drop(browse::prepare_artwork(
      &mut state.browse,
      &mut state.kernel,
      state.window_size.width,
    ));
    state.retain_artwork_handles();

    let browse_cell = state
      .browse
      .artwork
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
}
