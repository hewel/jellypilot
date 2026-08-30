//! Playback surface (ADR 0029): the MPV playback session and its projected
//! player-bar view, seek/volume previews, track popovers, player artwork, and
//! the folded-in remote Playback Target cluster (WebSocket session, command
//! translation, and capability registration).
//!
//! Two entry points share the private helpers below: [`update`] reduces
//! [`PlaybackMessage`] and [`update_remote`] reduces [`RemoteMessage`]; each
//! also records the diagnostics/toast follow-ups its cluster produces, so the
//! top-level router is a plain delegation. `quit_requested` is the shell's
//! quit-handshake flag, computed by the router so this module never reads
//! window/shell state.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use iced::widget::image;
use iced::Task;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::config::Settings;
use jellypilot_core::diagnostics::{coalescing_key, DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::request_gate::{RemotePlayToken, RemoteToken, RequestGate};
use jellypilot_media_server::artwork::{ArtworkSizeClass, LoadLane};
use jellypilot_media_server::ticks_to_seconds;
use jellypilot_mpv::configured_mpv_args;
use jellypilot_mpv::playback::{
  media_item_from_playable, rich_playable, Playable, PlaybackController, PlaybackControllerConfig,
  PlaybackStartPosition,
};
use jellypilot_mpv::playback_session::{
  seek_intent, volume_intent, AdjacentDirection, ControllerCommand, ControllerSettlement, EffectId,
  PlaybackEffect, PlaybackEvent, PlaybackInput, PlaybackIntent, PlaybackNotice, PlaybackSession,
  SessionView,
};
use jellypilot_mpv::remote_commands::{remote_command_action, RemoteCommandAction};
use jellypilot_session::{
  finalize_remote_target, JellyfinWebSocket, JellyfinWebSocketEvent, RemoteControlState,
};

use crate::tray::TrayAction;

use super::kernel::Kernel;
use super::message::{
  LoginMessage, Message, PlaybackMessage, RemoteMessage, RemoteSessionStart, RemoteStartError,
  SettingsMessage,
};
use super::state::{
  ArtworkCell, ArtworkCellState, NoticeLevel, PlaybackControllerHandle, RemoteEventChannel,
  RemoteSessionHandle,
};

/// Playback surface slice: the playback session machine and its projected
/// view, the resolved current/adjacent playables, the MPV controller handle,
/// in-flight effect markers, seek/volume previews, track popover flags,
/// player-bar artwork, and the remote Playback Target session state.
pub struct Surface {
  pub notice: Option<String>,
  pub artwork: Option<ArtworkCell>,
  pub controller: Option<PlaybackControllerHandle>,
  pub session: PlaybackSession,
  pub view: SessionView,
  pub playable: Option<Playable>,
  pub in_flight_refresh: Option<EffectId>,
  pub in_flight_command: Option<EffectId>,
  pub adjacent_playables: [Option<Playable>; 2],
  /// Token identifying the current remote registration epoch; stale remote
  /// completions carry an older token and are ignored.
  pub remote: RemoteToken,
  pub remote_session: Option<RemoteSessionHandle>,
  pub remote_events: Option<RemoteEventChannel>,
  pub remote_control_state: RemoteControlState,
  pub remote_stopping: bool,
  pub seek_preview: Option<f64>,
  pub volume_preview: Option<f64>,
  pub audio_menu_open: bool,
  pub subtitle_menu_open: bool,
}

impl Surface {
  pub fn new(request_gate: &mut RequestGate) -> Self {
    let session = PlaybackSession::default();
    Self {
      notice: None,
      artwork: None,
      view: session.view(),
      session,
      playable: None,
      in_flight_refresh: None,
      in_flight_command: None,
      adjacent_playables: [None, None],
      remote: request_gate.begin_remote(),
      remote_session: None,
      remote_events: None,
      remote_control_state: RemoteControlState::Unavailable,
      remote_stopping: false,
      seek_preview: None,
      volume_preview: None,
      audio_menu_open: false,
      subtitle_menu_open: false,
      controller: None,
    }
  }
}

/// Playback surface entry point: reduces a [`PlaybackMessage`] and records
/// the diagnostics/toast follow-up for a changed playback notice.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  message: PlaybackMessage,
) -> Task<Message> {
  tracing::debug!(
    message = playback_message_name(&message),
    "playback message"
  );
  let previous_notice = surface.notice.clone();
  let task = update_playback(surface, kernel, quit_requested, message);
  let toast_task = record_playback_notice(surface, kernel, previous_notice.as_deref());
  Task::batch([task, toast_task])
}

/// Remote Playback Target entry point: reduces a [`RemoteMessage`] and
/// records the diagnostics/toast follow-ups for remote-state and notice
/// changes.
pub fn update_remote(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  message: RemoteMessage,
) -> Task<Message> {
  let previous_state = surface.remote_control_state;
  let previous_notice = kernel.notice.clone();
  let task = handle_remote(surface, kernel, quit_requested, message);
  let toast_task =
    record_remote_change(surface, kernel, previous_state, previous_notice.as_deref());
  Task::batch([task, toast_task])
}

fn record_playback_notice(
  surface: &mut Surface,
  kernel: &mut Kernel,
  previous: Option<&str>,
) -> Task<Message> {
  let Some(notice) = surface.notice.clone() else {
    if previous.is_some() {
      kernel.diagnostics.reset_coalescing();
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
  let key = coalescing_key("playback", &notice);
  kernel
    .diagnostics
    .record_coalesced(&key, level, DiagnosticCategory::Playback, &notice);

  let toast_level = match level {
    DiagnosticLevel::Error => NoticeLevel::Error,
    _ => NoticeLevel::Warning,
  };
  kernel.show_toast(toast_level, notice)
}

fn record_remote_change(
  surface: &mut Surface,
  kernel: &mut Kernel,
  previous_state: RemoteControlState,
  previous_notice: Option<&str>,
) -> Task<Message> {
  if surface.remote_control_state != previous_state {
    let (level, message) = match surface.remote_control_state {
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
    kernel
      .diagnostics
      .record(level, DiagnosticCategory::RemoteControl, message);
  }
  if let Some(notice) = kernel
    .notice
    .clone()
    .filter(|notice| Some(notice.as_str()) != previous_notice)
  {
    kernel.diagnostics.record(
      DiagnosticLevel::Warning,
      DiagnosticCategory::RemoteControl,
      &notice,
    );
    return kernel.show_toast(NoticeLevel::Warning, notice);
  }
  Task::none()
}

/// Re-applies the configured MPV path/arguments: reconfigures the live
/// controller when one exists, otherwise retries discovery. Called by the
/// router after playback-relevant settings mutate (ADR 0029).
pub(crate) fn apply_playback_configuration(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
) -> Task<Message> {
  let config = playback_controller_config(kernel.settings.snapshot());
  if let Some(controller) = surface.controller.as_ref().map(Arc::clone) {
    return Task::perform(
      async move { controller.lock().await.configure_for_next_start(config) },
      |result| Message::Settings(SettingsMessage::PlaybackConfigApplied(result)),
    );
  }
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  match PlaybackController::discover(client, config) {
    Ok(controller) => {
      surface.controller = Some(Arc::new(tokio::sync::Mutex::new(controller)));
      let _ = surface.session.handle(
        PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
        Instant::now(),
      );
      sync_playback_projection(surface, kernel, quit_requested);
      surface.notice = None;
      Task::none()
    }
    Err(error) => {
      let _ = surface.session.handle(
        PlaybackInput::Event(PlaybackEvent::EngineAvailability(false)),
        Instant::now(),
      );
      sync_playback_projection(surface, kernel, quit_requested);
      surface.notice =
        Some("External playback is unavailable because MPV could not be found.".into());
      let toast_task = kernel.show_toast(
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

/// Re-registers the playback target after the configured name changes.
/// Called by the router after settings mutate (ADR 0029).
pub(crate) fn refinalize_playback_target(
  surface: &mut Surface,
  kernel: &mut Kernel,
) -> Task<Message> {
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let name = kernel
    .settings
    .snapshot()
    .playback_target_name()
    .unwrap_or("JellyPilot")
    .to_owned();
  client.set_device_name(name);
  if !should_refinalize_playback_target(surface, kernel) {
    return Task::none();
  }
  kernel.diagnostics.record(
    DiagnosticLevel::Info,
    DiagnosticCategory::RemoteControl,
    "Playback target name changed; remote registration requested.",
  );
  let remote = surface.remote;
  Task::perform(
    async move { finalize_remote_target(&client).await },
    move |result| Message::Remote(RemoteMessage::Finalized { remote, result }),
  )
}

fn should_refinalize_playback_target(surface: &Surface, kernel: &Kernel) -> bool {
  kernel.connection == ConnectionPhase::Connected
    && surface.remote_session.is_some()
    && surface.remote_control_state == RemoteControlState::Available
}

/// Opens the remote Playback Target WebSocket session and registers
/// capabilities. Called by the router when the login surface connects.
pub(crate) fn start_remote_session(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  if let Some(name) = kernel.settings.snapshot().playback_target_name() {
    client.set_device_name(name.to_owned());
  }

  let remote = surface.remote;
  let websocket = Arc::new(JellyfinWebSocket::new());
  let Some(mut websocket_events) = websocket.take_event_receiver() else {
    return Task::done(Message::Remote(RemoteMessage::Started {
      remote,
      result: Err(RemoteStartError::SessionUnavailable),
    }));
  };
  let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();
  surface.remote_events = Some(RemoteEventChannel {
    remote,
    receiver: Arc::new(tokio::sync::Mutex::new(event_receiver)),
  });
  let session = RemoteSessionHandle {
    websocket: Arc::clone(&websocket),
    lifecycle: Arc::new(tokio::sync::Mutex::new(())),
  };
  surface.remote_session = Some(session.clone());
  surface.remote_control_state = RemoteControlState::Connecting;

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

fn handle_remote(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  message: RemoteMessage,
) -> Task<Message> {
  match message {
    RemoteMessage::Started { remote, result } => {
      if !kernel.request_gate.is_current_remote(remote)
        || kernel.connection != ConnectionPhase::Connected
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
          surface.remote_session = Some(session);
          surface.remote_control_state = RemoteControlState::Available;
          if !validated {
            kernel.notice = Some(
              "Remote playback target connected, but server session validation is still pending."
                .to_owned(),
            );
          }
        }
        Err(error) => {
          surface.remote = kernel.request_gate.begin_remote();
          surface.remote_session = None;
          surface.remote_events = None;
          surface.remote_control_state = RemoteControlState::Unavailable;
          kernel.notice = Some(error.diagnostic().to_owned());
        }
      }
      Task::none()
    }
    RemoteMessage::Event { remote, event } => {
      if !kernel.request_gate.is_current_remote(remote) {
        return Task::none();
      }
      match event {
        JellyfinWebSocketEvent::Command(command)
          if surface.remote_control_state == RemoteControlState::Available =>
        {
          handle_remote_command(surface, kernel, quit_requested, remote, command)
        }
        JellyfinWebSocketEvent::Command(_) => Task::none(),
        JellyfinWebSocketEvent::Reconnected => {
          let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
            return Task::none();
          };
          surface.remote_control_state = RemoteControlState::Connecting;
          Task::perform(
            async move { finalize_remote_target(&client).await },
            move |result| Message::Remote(RemoteMessage::Finalized { remote, result }),
          )
        }
        JellyfinWebSocketEvent::ConnectionLost => {
          surface.remote_control_state = RemoteControlState::Lost;
          kernel.notice = Some(REMOTE_CONNECTION_LOST_NOTICE.to_owned());
          Task::none()
        }
        JellyfinWebSocketEvent::Connected => Task::none(),
      }
    }
    RemoteMessage::Finalized { remote, result } => {
      if !kernel.request_gate.is_current_remote(remote) {
        return Task::none();
      }
      match result {
        Ok(true) => {
          surface.remote_control_state = RemoteControlState::Available;
          if kernel.notice.as_deref() == Some(REMOTE_CONNECTION_LOST_NOTICE) {
            kernel.notice = None;
          }
          Task::none()
        }
        Ok(false) => {
          surface.remote_control_state = RemoteControlState::Available;
          kernel.notice = Some(
            "Remote playback target reconnected, but server session validation is still pending."
              .to_owned(),
          );
          Task::none()
        }
        Err(()) => fail_remote_finalization(surface, kernel),
      }
    }
    RemoteMessage::PlayResolved {
      remote,
      play,
      result,
      start_position_ticks,
      selection,
    } => {
      if !kernel.request_gate.is_current_remote(remote)
        || !kernel.request_gate.is_current_remote_play(play)
      {
        return Task::none();
      }
      let Ok(item) = *result else {
        kernel.notice = Some("Remote playback item could not be loaded.".to_owned());
        return Task::none();
      };
      let position = start_position_ticks.map_or(PlaybackStartPosition::Beginning, |ticks| {
        PlaybackStartPosition::At(ticks_to_seconds(ticks))
      });
      let intro = kernel.intro_availability();
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(PlaybackIntent::Start {
          item,
          position,
          intro,
          selection: Box::new(selection),
        }),
      )
    }
    RemoteMessage::RemoteDisconnected => Task::none(),
    RemoteMessage::QuitStopped => {
      surface.remote_stopping = false;
      if quit_may_exit(surface, quit_requested) {
        iced::exit()
      } else {
        Task::none()
      }
    }
  }
}

fn handle_remote_command(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  remote: RemoteToken,
  command: jellypilot_session::JellyfinCommand,
) -> Task<Message> {
  match remote_command_action(command, &surface.view) {
    Some(RemoteCommandAction::Intent(intent)) => {
      if intent.invalidates_remote_play() {
        kernel.request_gate.begin_remote_play();
      }
      let Some(intent) = intent.into_playback_intent(&surface.view) else {
        kernel.notice = Some(REMOTE_TRACKS_UNAVAILABLE_NOTICE.to_owned());
        return Task::none();
      };
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(intent),
      )
    }
    Some(RemoteCommandAction::Play {
      item_id,
      start_position_ticks,
      selection,
    }) => {
      let play = kernel.request_gate.begin_remote_play();
      let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
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

fn fail_remote_finalization(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  surface.remote = kernel.request_gate.begin_remote();
  surface.remote_events = None;
  surface.remote_control_state = RemoteControlState::Unavailable;
  kernel.notice = Some("Remote playback target capabilities could not be registered.".to_owned());
  let Some(session) = surface.remote_session.take() else {
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

/// Tears the remote session down for the shell's quit handshake; the
/// completion arrives as [`RemoteMessage::QuitStopped`]. Called by the
/// router's window/tray quit arms.
pub(crate) fn stop_remote_session_for_quit(
  surface: &mut Surface,
  kernel: &mut Kernel,
) -> Task<Message> {
  surface.remote = kernel.request_gate.begin_remote();
  surface.remote_events = None;
  surface.remote_control_state = RemoteControlState::Unavailable;
  let Some(session) = surface.remote_session.take() else {
    return Task::none();
  };
  surface.remote_stopping = true;
  Task::perform(
    async move { disconnect_remote_session(session).await },
    |()| Message::Remote(RemoteMessage::QuitStopped),
  )
}

/// Tears the remote session down before a sign-out; the completion arrives as
/// [`LoginMessage::RemoteDisconnected`] so the login surface finishes the
/// transition. Called by the router's login/settings arms.
pub(crate) fn stop_remote_session_for_login(
  surface: &mut Surface,
  kernel: &mut Kernel,
) -> Task<LoginMessage> {
  surface.remote = kernel.request_gate.begin_remote();
  surface.remote_events = None;
  surface.remote_control_state = RemoteControlState::Unavailable;
  let Some(session) = surface.remote_session.take() else {
    return Task::done(LoginMessage::RemoteDisconnected);
  };
  surface.remote_stopping = true;
  Task::perform(
    async move { disconnect_remote_session(session).await },
    |()| LoginMessage::RemoteDisconnected,
  )
}

/// Tray transport actions map onto playback intents. `Show`/`Quit` stay at
/// the top-level router: showing the window routes through the window
/// surface, and Quit owns the shell's quit handshake (ADR 0029).
pub(crate) fn update_tray(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  action: TrayAction,
) -> Task<Message> {
  match action {
    TrayAction::PlayPause => apply_playback_input(
      surface,
      kernel,
      quit_requested,
      PlaybackInput::Intent(PlaybackIntent::TogglePaused),
    ),
    TrayAction::Next => apply_local_playback_intent(
      surface,
      kernel,
      quit_requested,
      PlaybackIntent::PlayAdjacent(AdjacentDirection::Next),
    ),
    TrayAction::Previous => apply_local_playback_intent(
      surface,
      kernel,
      quit_requested,
      PlaybackIntent::PlayAdjacent(AdjacentDirection::Previous),
    ),
    TrayAction::Mute => {
      let Some(muted) = surface
        .view
        .now_playing
        .as_ref()
        .map(|playing| playing.muted)
      else {
        return Task::none();
      };
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(PlaybackIntent::SetMuted(!muted)),
      )
    }
    TrayAction::Show | TrayAction::Quit => Task::none(),
  }
}

/// Resets the playback session and discovers the MPV controller after the
/// login surface connects. Called by the router's login arm.
pub(crate) fn initialize_playback(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
) {
  surface.session = PlaybackSession::default();
  surface.view = surface.session.view();
  surface.notice = None;
  surface.playable = None;
  surface.adjacent_playables = [None, None];
  clear_player_artwork(surface, kernel);
  surface.seek_preview = None;
  surface.volume_preview = None;
  surface.remote = kernel.request_gate.begin_remote();
  surface.in_flight_refresh = None;
  surface.in_flight_command = None;

  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.controller = None;
    return;
  };
  match PlaybackController::discover(
    client,
    playback_controller_config(kernel.settings.snapshot()),
  ) {
    Ok(controller) => {
      surface.controller = Some(Arc::new(tokio::sync::Mutex::new(controller)));
      let _ = surface.session.handle(
        PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
        Instant::now(),
      );
      surface.view = surface.session.view();
    }
    Err(_) => {
      surface.controller = None;
      surface.notice =
        Some("External playback is unavailable because MPV could not be found.".into());
    }
  }
  sync_tray(surface, kernel, quit_requested);
}

fn playback_controller_config(settings: &Settings) -> PlaybackControllerConfig {
  let config = PlaybackControllerConfig::default().with_extra_args(configured_mpv_args(settings));
  match settings.mpv_path() {
    Some(path) => config.with_mpv_path(PathBuf::from(path)),
    None => config,
  }
}

fn apply_local_playback_intent(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  intent: PlaybackIntent,
) -> Task<Message> {
  if matches!(
    &intent,
    PlaybackIntent::Start { .. } | PlaybackIntent::Stop | PlaybackIntent::PlayAdjacent(_)
  ) {
    kernel.request_gate.begin_remote_play();
  }
  apply_playback_input(
    surface,
    kernel,
    quit_requested,
    PlaybackInput::Intent(intent),
  )
}

fn update_playback(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  message: PlaybackMessage,
) -> Task<Message> {
  match message {
    PlaybackMessage::Intent(intent) => {
      apply_local_playback_intent(surface, kernel, quit_requested, intent)
    }
    PlaybackMessage::Event(event) => apply_playback_input(
      surface,
      kernel,
      quit_requested,
      PlaybackInput::Event(*event),
    ),
    PlaybackMessage::SeekChanged(position) => {
      surface.seek_preview = seek_intent(
        position,
        surface
          .view
          .now_playing
          .as_ref()
          .and_then(|view| view.duration_seconds),
        surface.view.now_playing.is_some(),
      )
      .and_then(|intent| match intent {
        PlaybackIntent::Seek(position) => Some(position),
        _ => None,
      });
      Task::none()
    }
    PlaybackMessage::SeekReleased => {
      let Some(position) = surface.seek_preview else {
        return Task::none();
      };
      let Some(intent) = seek_intent(
        position,
        surface
          .view
          .now_playing
          .as_ref()
          .and_then(|view| view.duration_seconds),
        surface.view.now_playing.is_some(),
      ) else {
        return Task::none();
      };
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(intent),
      )
    }
    PlaybackMessage::VolumeChanged(volume) => {
      surface.volume_preview =
        volume_intent(volume, surface.view.now_playing.is_some()).and_then(|intent| match intent {
          PlaybackIntent::SetVolume(volume) => Some(volume),
          _ => None,
        });
      Task::none()
    }
    PlaybackMessage::VolumeReleased => {
      let Some(volume) = surface.volume_preview else {
        return Task::none();
      };
      let Some(intent) = volume_intent(volume, surface.view.now_playing.is_some()) else {
        return Task::none();
      };
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(intent),
      )
    }
    PlaybackMessage::AudioMenuToggled => {
      surface.audio_menu_open = !surface.audio_menu_open;
      surface.subtitle_menu_open = false;
      Task::none()
    }
    PlaybackMessage::AudioMenuDismissed => {
      surface.audio_menu_open = false;
      Task::none()
    }
    PlaybackMessage::AudioTrackSelected(id) => {
      surface.audio_menu_open = false;
      apply_local_playback_intent(
        surface,
        kernel,
        quit_requested,
        PlaybackIntent::SelectAudioTrack(id),
      )
    }
    PlaybackMessage::SubtitleMenuToggled => {
      surface.subtitle_menu_open = !surface.subtitle_menu_open;
      surface.audio_menu_open = false;
      Task::none()
    }
    PlaybackMessage::SubtitleMenuDismissed => {
      surface.subtitle_menu_open = false;
      Task::none()
    }
    PlaybackMessage::SubtitleTrackSelected(id) => {
      surface.subtitle_menu_open = false;
      apply_local_playback_intent(
        surface,
        kernel,
        quit_requested,
        PlaybackIntent::SelectSubtitleTrack(id),
      )
    }
    PlaybackMessage::ControllerSettled {
      id,
      settlement,
      started,
      tracks,
    } => {
      if surface.in_flight_refresh == Some(id) {
        surface.in_flight_refresh = None;
      }
      if surface.in_flight_command == Some(id) {
        surface.in_flight_command = None;
      }
      let started = if matches!(settlement.as_ref(), ControllerSettlement::Started(Ok(_))) {
        started
      } else {
        None
      };
      let shutdown = matches!(settlement.as_ref(), ControllerSettlement::Shutdown(_));
      if let Some(playable) = started.as_deref() {
        surface.playable = Some(playable.clone());
        surface.adjacent_playables = [None, None];
      }
      let mut tasks = vec![apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Event(PlaybackEvent::ControllerSettled {
          id,
          settlement: *settlement,
        }),
      )];
      tracing::debug!(
        started = ?started.as_deref().map(|playable| (playable_kind(playable), playable.image_id().map(str::to_owned))),
        now_playing = ?surface.view.now_playing.as_ref().map(|view| view.item.item_id.clone()),
        playable = ?surface.playable.as_ref().map(|playable| (playable_kind(playable), playable.image_id().map(str::to_owned))),
        "controller settled"
      );
      if let Some(result) = tracks {
        tasks.push(apply_playback_input(
          surface,
          kernel,
          quit_requested,
          PlaybackInput::Event(PlaybackEvent::TracksSettled { id, result }),
        ));
      }
      if shutdown {
        surface.controller = None;
        let _ = surface.session.handle(
          PlaybackInput::Event(PlaybackEvent::EngineAvailability(false)),
          Instant::now(),
        );
        sync_playback_projection(surface, kernel, quit_requested);
      }
      if !surface.view.busy {
        surface.seek_preview = None;
        surface.volume_preview = None;
      }
      tasks.push(clear_inactive_playback(surface, kernel));
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
      if remote != surface.remote
        || !kernel.request_gate.is_current_remote(remote)
        || !kernel.request_gate.is_current_remote_play(play)
      {
        return Task::none();
      }
      surface.adjacent_playables[direction.index()] =
        result.as_ref().ok().and_then(Option::as_ref).map(|item| {
          detail
            .map(|detail| Playable::Detail(*detail))
            .unwrap_or_else(|| Playable::Media(item.clone()))
        });
      // When the current item started from a bare Media playable (cast, or an
      // adjacent start that beat its own prefetch), the settled enrichment is
      // the first chance to resolve its artwork.
      let mut tasks = Vec::new();
      if let Some(rich) = surface.adjacent_playables[direction.index()].as_ref() {
        if surface.playable.as_ref().is_some_and(|current| {
          matches!(current, Playable::Media(_)) && current.item_id() == rich.item_id()
        }) {
          surface.playable = Some(rich.clone());
          tasks.push(prepare_player_artwork(surface, kernel));
        }
      }
      tasks.push(apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
          id,
          direction,
          result,
        }),
      ));
      Task::batch(tasks)
    }
    PlaybackMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = kernel.request_gate.is_current_session(session);
      if kernel
        .artwork_binder
        .settle(slot, ArtworkSurface::PlayerBar, session_ok)
        != ArtworkSettlement::Apply
      {
        return Task::none();
      }
      let Some(cell) = surface
        .artwork
        .as_mut()
        .filter(|cell| cell.slot == slot && cell.image_id == image_id)
      else {
        return Task::none();
      };
      match result {
        Ok(raster) => {
          cell.state = ArtworkCellState::Ready;
          kernel.artwork_handles.insert(
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

/// Feeds one input into the playback session, executes the resulting effects,
/// re-projects the view, and completes the shell's quit handshake when the
/// session is done. `pub(crate)` for the router's window/tray/settings quit
/// and configuration arms.
pub(crate) fn apply_playback_input(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  input: PlaybackInput,
) -> Task<Message> {
  let effects = surface.session.handle(input, Instant::now());
  let task = execute_playback_effects(surface, kernel, effects);
  sync_playback_projection(surface, kernel, quit_requested);
  let artwork_task = ensure_player_artwork(surface, kernel);
  if quit_may_exit(surface, quit_requested) {
    Task::batch([task, artwork_task, iced::exit()])
  } else {
    Task::batch([task, artwork_task])
  }
}
/// Re-prepares the player-bar artwork whenever the projected Now Playing item
/// matches the resolved playable but the artwork cell does not cover it.
/// Settlements and adjacent enrichments race with projection updates; this
/// keeps the poster eventually consistent after either ordering.
fn ensure_player_artwork(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let covered = surface.view.now_playing.as_ref().is_some_and(|view| {
    surface
      .playable
      .as_ref()
      .is_some_and(|playable| playable.item_id() == view.item.item_id)
  });
  tracing::debug!(
    covered,
    now_playing = ?surface.view.now_playing.as_ref().map(|view| view.item.item_id.clone()),
    playable = ?surface.playable.as_ref().map(|playable| (playable_kind(playable), playable.image_id().map(str::to_owned))),
    "ensure player artwork"
  );
  if covered {
    prepare_player_artwork(surface, kernel)
  } else {
    Task::none()
  }
}

fn playback_message_name(message: &PlaybackMessage) -> &'static str {
  match message {
    PlaybackMessage::Intent(_) => "intent",
    PlaybackMessage::Event(_) => "event",
    PlaybackMessage::SeekChanged(_) => "seek-changed",
    PlaybackMessage::SeekReleased => "seek-released",
    PlaybackMessage::VolumeChanged(_) => "volume-changed",
    PlaybackMessage::VolumeReleased => "volume-released",
    PlaybackMessage::AudioMenuToggled => "audio-menu-toggled",
    PlaybackMessage::AudioMenuDismissed => "audio-menu-dismissed",
    PlaybackMessage::AudioTrackSelected(_) => "audio-track-selected",
    PlaybackMessage::SubtitleMenuToggled => "subtitle-menu-toggled",
    PlaybackMessage::SubtitleMenuDismissed => "subtitle-menu-dismissed",
    PlaybackMessage::SubtitleTrackSelected(_) => "subtitle-track-selected",
    PlaybackMessage::ControllerSettled { .. } => "controller-settled",
    PlaybackMessage::AdjacentSettled { .. } => "adjacent-settled",
    PlaybackMessage::ArtworkLoaded { .. } => "artwork-loaded",
  }
}
fn playable_kind(playable: &Playable) -> &'static str {
  match playable {
    Playable::Library(_) => "library",
    Playable::Detail(_) => "detail",
    Playable::Media(_) => "media",
  }
}
/// The shell's quit handshake may exit once the playback session finished
/// cleaning up and no remote teardown is in flight.
pub(crate) fn quit_may_exit(surface: &Surface, quit_requested: bool) -> bool {
  quit_requested && surface.view.quit_may_proceed && !surface.remote_stopping
}

fn sync_playback_projection(surface: &mut Surface, kernel: &Kernel, quit_requested: bool) {
  let mut view = surface.session.view();
  if view.busy && surface.in_flight_refresh.is_some() && surface.in_flight_command.is_none() {
    view.busy = false;
  }
  surface.view = view;
  surface.notice = surface.view.notice.as_ref().map(|notice| match notice {
    PlaybackNotice::Failed(error) => error.to_string(),
    PlaybackNotice::Warnings(_) => {
      "Playback is active, but setup or reporting could not be completed.".to_owned()
    }
  });
  sync_tray(surface, kernel, quit_requested);
}

/// Mirrors the projected playback view into the tray menu. Writes only
/// kernel tray state; `pub(crate)` for the router's tray Quit arm.
pub(crate) fn sync_tray(surface: &Surface, kernel: &Kernel, quit_requested: bool) {
  if let Some(tray) = &kernel.tray {
    tray.sync(&surface.view, quit_requested);
  }
}

fn clear_player_artwork(surface: &mut Surface, kernel: &mut Kernel) {
  if let Some(cell) = surface.artwork.take() {
    kernel.artwork_handles.remove(cell.slot);
  }
}

fn clear_inactive_playback(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  // A start or refresh in flight can transiently project no Now Playing
  // between files; clearing here would wipe the incoming item's artwork.
  if surface.view.now_playing.is_some()
    || surface.in_flight_command.is_some()
    || surface.in_flight_refresh.is_some()
  {
    return Task::none();
  }
  tracing::debug!(
    in_flight_command = surface.in_flight_command.is_some(),
    in_flight_refresh = surface.in_flight_refresh.is_some(),
    playable = ?surface.playable.as_ref().map(playable_kind),
    "clearing inactive playback"
  );
  surface.playable = None;
  surface.adjacent_playables = [None, None];
  clear_player_artwork(surface, kernel);
  surface.seek_preview = None;
  surface.volume_preview = None;
  surface.audio_menu_open = false;
  surface.subtitle_menu_open = false;
  Task::none()
}

fn execute_playback_effects(
  surface: &mut Surface,
  kernel: &mut Kernel,
  effects: Vec<PlaybackEffect>,
) -> Task<Message> {
  let adjacent_play = effects
    .iter()
    .any(|effect| matches!(effect, PlaybackEffect::LookupAdjacent(_, _)))
    .then(|| kernel.request_gate.begin_remote_play());
  Task::batch(
    effects
      .into_iter()
      .map(|effect| execute_playback_effect(surface, kernel, effect, adjacent_play)),
  )
}

fn execute_playback_effect(
  surface: &mut Surface,
  kernel: &mut Kernel,
  effect: PlaybackEffect,
  adjacent_play: Option<RemotePlayToken>,
) -> Task<Message> {
  match effect {
    PlaybackEffect::Controller(id, command) => {
      match &command {
        ControllerCommand::Refresh => {
          surface.in_flight_refresh = Some(id);
        }
        ControllerCommand::ShowText { .. } => {}
        _ => {
          surface.in_flight_command = Some(id);
        }
      }
      execute_controller_command(surface, id, command)
    }
    PlaybackEffect::LookupAdjacent(id, direction) => {
      let Some(play) = adjacent_play else {
        return Task::none();
      };
      let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
        return Task::done(Message::Playback(PlaybackMessage::AdjacentSettled {
          remote: surface.remote,
          play,
          id,
          direction,
          result: Err(()),
          detail: None,
        }));
      };
      let Some(playable) = surface.playable.as_ref() else {
        return Task::none();
      };
      let current = media_item_from_playable(playable);
      let remote = surface.remote;
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
      let Some(client) = kernel
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
  surface: &Surface,
  id: EffectId,
  command: ControllerCommand,
) -> Task<Message> {
  let started = match &command {
    ControllerCommand::Start { item, .. } => Some(rich_playable(&surface.adjacent_playables, item)),
    _ => None,
  };
  let Some(controller) = surface.controller.as_ref().map(Arc::clone) else {
    let settlement = command.missing_controller_settlement();
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

fn prepare_player_artwork(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let image_id = surface
    .playable
    .as_ref()
    .and_then(Playable::image_id)
    .map(str::to_owned);
  let Some(image_id) = image_id else {
    tracing::debug!(
      playable = ?surface.playable.as_ref().map(playable_kind),
      "player artwork cleared: playable has no image"
    );
    clear_player_artwork(surface, kernel);
    return Task::none();
  };
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  if let Some(cell) = &surface.artwork {
    if cell.image_id == image_id {
      if cell.state == ArtworkCellState::Loading {
        return Task::none();
      }
      if cell.state == ArtworkCellState::Ready
        && kernel
          .artwork_handles
          .get(cell.slot, &cell.image_id)
          .is_some()
      {
        return Task::none();
      }
    }
  }
  clear_player_artwork(surface, kernel);
  if let Some(raster) = kernel
    .artwork_adapter
    .cached(&image_id, ArtworkSizeClass::Card)
  {
    let slot = kernel.artwork_binder.bind_settled();
    let handle = image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels());
    kernel
      .artwork_handles
      .insert(slot, image_id.clone(), handle);
    surface.artwork = Some(ArtworkCell {
      slot,
      image_id,
      state: ArtworkCellState::Ready,
    });
    return Task::none();
  }
  let slot = kernel.artwork_binder.bind_player_bar();
  surface.artwork = Some(ArtworkCell {
    slot,
    image_id: image_id.clone(),
    state: ArtworkCellState::Loading,
  });
  let adapter = Arc::clone(&kernel.artwork_adapter);
  let session = kernel.request_gate.current_session();
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

/// Tears down playback and the remote session on sign-out/disconnect. The
/// router performs the other surfaces' resets around this call (ADR 0029).
pub(crate) fn disconnect(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
) -> Task<Message> {
  let task = apply_playback_input(
    surface,
    kernel,
    quit_requested,
    PlaybackInput::Intent(PlaybackIntent::Disconnect),
  );
  surface.remote = kernel.request_gate.begin_remote();
  surface.remote_session = None;
  surface.remote_events = None;
  surface.remote_control_state = RemoteControlState::Unavailable;
  surface.remote_stopping = false;
  surface.in_flight_refresh = None;
  surface.in_flight_command = None;
  task
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Instant;

  use jellypilot_auth::AuthStore;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::{JellyfinClient, MediaItem, VideoItemDetail, VideoLibraryItem};
  use jellypilot_mpv::playback::{
    NowPlayingItem, PlaybackEndReason, PlaybackOutcome, PlaybackRefreshOutcome,
    PlaybackRefreshState, PlaybackSelection, PlaybackSnapshot,
  };
  use jellypilot_mpv::playback_session::{IntroAvailability, NowPlayingView, TracksView};
  use jellypilot_session::{GeneralCommand, IntroSkipMode, JellyfinCommand, PlayRequest};

  use super::*;
  use crate::app::state::ArtworkHandleRetention;

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let mut request_gate = RequestGate::default();
    let surface = Surface::new(&mut request_gate);
    let kernel = Kernel {
      settings,
      diagnostics: Diagnostics::default(),
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
      artwork_handles: ArtworkHandleRetention::default(),
    };
    (surface, kernel)
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

  fn media_item(id: &str) -> MediaItem {
    MediaItem {
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

  fn playback_snapshot(position: f64) -> PlaybackSnapshot {
    PlaybackSnapshot {
      now_playing: Some(NowPlayingItem {
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

  fn controller_effect(effects: Vec<PlaybackEffect>) -> (EffectId, ControllerCommand) {
    let [PlaybackEffect::Controller(id, command)] = effects.as_slice() else {
      panic!("expected one controller effect");
    };
    (*id, command.clone())
  }

  fn active_playback_fixture() -> (Surface, Kernel) {
    let (mut surface, kernel) = test_fixture();
    let now = Instant::now();
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = surface.session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (id, _) = controller_effect(effects);
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(10.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    surface.view = surface.session.view();
    (surface, kernel)
  }

  #[test]
  fn seek_release_keeps_committed_preview_while_queued_behind_refresh() {
    let (mut surface, mut kernel) = active_playback_fixture();
    let now = Instant::now();
    let (refresh_id, command) = controller_effect(
      surface
        .session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    surface.view = surface.session.view();
    surface.seek_preview = Some(120.0);

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::SeekReleased,
    ));
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
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

    assert_eq!(surface.seek_preview, Some(120.0));
    assert!(surface.view.busy);
  }

  #[test]
  fn volume_release_keeps_committed_preview_while_queued_behind_refresh() {
    let (mut surface, mut kernel) = active_playback_fixture();
    let now = Instant::now();
    let (refresh_id, _) = controller_effect(
      surface
        .session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    surface.view = surface.session.view();
    surface.volume_preview = Some(42.0);

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::VolumeReleased,
    ));
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
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

    assert_eq!(surface.volume_preview, Some(42.0));
    assert!(surface.view.busy);
  }

  #[test]
  fn seek_change_during_refresh_keeps_the_draft_and_the_release_commits() {
    let (mut surface, mut kernel) = active_playback_fixture();
    let now = Instant::now();
    let (_refresh_id, command) = controller_effect(
      surface
        .session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    surface.view = surface.session.view();
    assert!(surface.view.busy);

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::SeekChanged(5.0),
    ));
    assert_eq!(surface.seek_preview, Some(5.0));

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::SeekReleased,
    ));
    assert_eq!(surface.seek_preview, Some(5.0));
    assert!(surface.view.busy);
  }

  #[test]
  fn volume_change_during_refresh_keeps_the_draft_and_the_release_commits() {
    let (mut surface, mut kernel) = active_playback_fixture();
    let now = Instant::now();
    let (_refresh_id, command) = controller_effect(
      surface
        .session
        .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now),
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    surface.view = surface.session.view();
    assert!(surface.view.busy);

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::VolumeChanged(42.0),
    ));
    assert_eq!(surface.volume_preview, Some(42.0));

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::VolumeReleased,
    ));
    assert_eq!(surface.volume_preview, Some(42.0));
    assert!(surface.view.busy);
  }

  #[test]
  fn inactive_playback_clears_artwork_previews_and_popover_state() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.audio_menu_open = true;
    surface.subtitle_menu_open = true;
    surface.seek_preview = Some(42.0);
    surface.volume_preview = Some(80.0);

    drop(clear_inactive_playback(&mut surface, &mut kernel));

    assert!(!surface.audio_menu_open);
    assert!(!surface.subtitle_menu_open);
    assert_eq!(surface.seek_preview, None);
    assert_eq!(surface.volume_preview, None);
  }

  #[test]
  fn player_artwork_rebind_releases_the_previous_decoded_handle() {
    let (mut surface, mut kernel) = test_fixture();
    let old_slot = kernel.artwork_binder.bind_player_bar();
    surface.artwork = Some(ArtworkCell {
      slot: old_slot,
      image_id: "old-image".to_owned(),
      state: ArtworkCellState::Ready,
    });
    kernel.artwork_handles.insert(
      old_slot,
      "old-image".to_owned(),
      image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
    );
    let mut playable = episode("episode-1", 1);
    playable.artwork_image_id = Some("new-image".to_owned());
    surface.playable = Some(Playable::Library(playable));
    kernel.client = Some(Arc::new(JellyfinClient::new()));

    drop(prepare_player_artwork(&mut surface, &mut kernel));

    assert!(kernel.artwork_handles.get(old_slot, "old-image").is_none());
    assert_ne!(
      surface.artwork.as_ref().map(|cell| cell.slot),
      Some(old_slot)
    );
  }

  #[test]
  fn clearing_playback_releases_the_current_decoded_player_handle() {
    let (mut surface, mut kernel) = test_fixture();
    let slot = kernel.artwork_binder.bind_player_bar();
    surface.artwork = Some(ArtworkCell {
      slot,
      image_id: "player-image".to_owned(),
      state: ArtworkCellState::Ready,
    });
    kernel.artwork_handles.insert(
      slot,
      "player-image".to_owned(),
      image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
    );

    drop(clear_inactive_playback(&mut surface, &mut kernel));

    assert!(kernel.artwork_handles.get(slot, "player-image").is_none());
    assert!(surface.artwork.is_none());
  }
  fn detail_with_series_poster(id: &str, image_id: &str) -> VideoItemDetail {
    VideoItemDetail {
      id: id.to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      overview: None,
      production_year: None,
      runtime_seconds: Some(1_800.0),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      season_number: Some(3),
      episode_number: Some(17),
      genres: Vec::new(),
      played: false,
      favorite: false,
      played_percentage: None,
      resume_position_seconds: None,
      can_resume: false,
      can_play: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: Some(image_id.to_owned()),
      metadata: Default::default(),
    }
  }

  #[test]
  fn clear_inactive_playback_preserves_artwork_while_a_start_is_in_flight() {
    let (mut surface, mut kernel) = test_fixture();
    let now = Instant::now();
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = surface.session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-2", 3)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (id, _) = controller_effect(effects);
    surface.in_flight_command = Some(id);
    let slot = kernel.artwork_binder.bind_player_bar();
    surface.artwork = Some(ArtworkCell {
      slot,
      image_id: "series-poster".to_owned(),
      state: ArtworkCellState::Ready,
    });
    kernel.artwork_handles.insert(
      slot,
      "series-poster".to_owned(),
      image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
    );

    drop(clear_inactive_playback(&mut surface, &mut kernel));

    assert!(surface.artwork.is_some());
    assert!(kernel.artwork_handles.get(slot, "series-poster").is_some());
  }

  #[test]
  fn start_settlement_keeps_the_new_playable_when_now_playing_has_not_caught_up() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.playable = Some(Playable::Library(episode("episode-1", 1)));
    let now = Instant::now();
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = surface.session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Media(media_item("episode-2")),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (id, _) = controller_effect(effects);

    // The settle arrives while the projection still reports the previous item
    // (playback_snapshot pins episode-1), so the old revert-on-mismatch would
    // have discarded the new playable.
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::ControllerSettled {
        id,
        settlement: Box::new(ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(10.0),
          warnings: Vec::new(),
        }))),
        started: Some(Box::new(Playable::Detail(detail_with_series_poster(
          "episode-2",
          "series-poster",
        )))),
        tracks: None,
      },
    ));

    assert_eq!(
      surface.playable.as_ref().map(Playable::item_id),
      Some("episode-2")
    );

    // Once the projection catches up to the new item, the artwork cell covers
    // its image again through the eager ensure pass.
    surface.view.now_playing = Some(NowPlayingView {
      item: NowPlayingItem {
        item_id: "episode-2".to_owned(),
        title: "Second".to_owned(),
        item_type: "Episode".to_owned(),
        runtime_seconds: Some(1_800.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".to_owned(),
      },
      paused: false,
      position_seconds: 0.0,
      duration_seconds: Some(1_800.0),
      volume: 75.0,
      muted: false,
    });
    drop(ensure_player_artwork(&mut surface, &mut kernel));
    assert_eq!(
      surface.artwork.as_ref().map(|cell| cell.image_id.as_str()),
      Some("series-poster")
    );
  }

  #[test]
  fn adjacent_settlement_upgrades_a_bare_media_playable_and_restores_artwork() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.playable = Some(Playable::Media(media_item("episode-2")));
    let remote = surface.remote;
    let play = kernel.request_gate.begin_remote_play();
    let now = Instant::now();
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = surface.session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-2", 3)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (id, _) = controller_effect(effects);

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::AdjacentSettled {
        remote,
        play,
        id,
        direction: AdjacentDirection::Next,
        result: Ok(Some(media_item("episode-2"))),
        detail: Some(Box::new(detail_with_series_poster(
          "episode-2",
          "series-poster",
        ))),
      },
    ));

    assert!(matches!(surface.playable, Some(Playable::Detail(_))));
    assert_eq!(
      surface.artwork.as_ref().map(|cell| cell.image_id.as_str()),
      Some("series-poster")
    );
  }

  #[test]
  fn remote_track_selection_without_loaded_mapping_is_ignored_with_diagnostic() {
    let (mut surface, mut kernel) = test_fixture();
    surface.view.tracks = TracksView::Unavailable;
    let remote = surface.remote;

    drop(handle_remote_command(
      &mut surface,
      &mut kernel,
      false,
      remote,
      JellyfinCommand::GeneralCommand(GeneralCommand {
        name: "SetAudioStreamIndex".to_owned(),
        arguments: Some(serde_json::json!({ "Index": 4 })),
      }),
    ));

    assert_eq!(
      kernel.notice.as_deref(),
      Some(REMOTE_TRACKS_UNAVAILABLE_NOTICE)
    );
  }

  #[test]
  fn local_stop_invalidates_an_in_flight_remote_play_resolution() {
    let (mut surface, mut kernel) = test_fixture();
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      Instant::now(),
    );
    sync_playback_projection(&mut surface, &kernel, false);
    let stale_play = kernel.request_gate.begin_remote_play();
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::Intent(PlaybackIntent::Stop),
    ));
    assert!(!kernel.request_gate.is_current_remote_play(stale_play));
    let remote = surface.remote;

    drop(handle_remote(
      &mut surface,
      &mut kernel,
      false,
      RemoteMessage::PlayResolved {
        remote,
        play: stale_play,
        result: Box::new(Ok(Playable::Media(MediaItem {
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

    assert!(surface.view.busy);
    assert!(surface.view.now_playing.is_none());
  }

  #[test]
  fn local_adjacent_starts_invalidate_an_in_flight_remote_play_resolution() {
    for direction in [AdjacentDirection::Previous, AdjacentDirection::Next] {
      let (mut surface, mut kernel) = test_fixture();
      let stale_play = kernel.request_gate.begin_remote_play();

      drop(update_playback(
        &mut surface,
        &mut kernel,
        false,
        PlaybackMessage::Intent(PlaybackIntent::PlayAdjacent(direction)),
      ));

      assert!(!kernel.request_gate.is_current_remote_play(stale_play));
    }
  }

  #[test]
  fn double_adjacent_press_dispatches_single_start() {
    let (mut surface, kernel) = test_fixture();
    let now = Instant::now();
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::EngineAvailability(true)),
      now,
    );
    let effects = surface.session.handle(
      PlaybackInput::Intent(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      }),
      now,
    );
    let (id, _) = controller_effect(effects);
    let aux = surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id,
        settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(10.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    surface.view = surface.session.view();
    let next_id = aux
      .iter()
      .find_map(|effect| match effect {
        PlaybackEffect::LookupAdjacent(id, AdjacentDirection::Next) => Some(*id),
        _ => None,
      })
      .expect("expected next lookup effect");

    // Settle next adjacent item
    surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::AdjacentSettled {
        id: next_id,
        direction: AdjacentDirection::Next,
        result: Ok(Some(media_item("episode-2"))),
      }),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);

    // First adjacent press
    let first_effects = surface.session.handle(
      PlaybackInput::Intent(PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)),
      now,
    );
    let (start_id, _) = controller_effect(first_effects);
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(surface.view.busy);

    // Second adjacent press while first is in flight (suppressed)
    let second_effects = surface.session.handle(
      PlaybackInput::Intent(PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)),
      now,
    );
    assert!(second_effects.is_empty());

    // Settle the start
    let settle_effects = surface.session.handle(
      PlaybackInput::Event(PlaybackEvent::ControllerSettled {
        id: start_id,
        settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(0.0),
          warnings: Vec::new(),
        })),
      }),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);

    // No second start effect dispatched
    assert!(!settle_effects
      .iter()
      .any(|e| matches!(e, PlaybackEffect::Controller(_, _))));
    assert!(!surface.view.busy);
    assert!(surface.view.now_playing.is_some());
  }

  #[test]
  fn double_stop_dispatches_single_stop_and_produces_no_notice() {
    let (mut surface, kernel) = active_playback_fixture();
    let now = Instant::now();

    // First stop
    let first_effects = surface
      .session
      .handle(PlaybackInput::Intent(PlaybackIntent::Stop), now);
    let (stop_id, _) = controller_effect(first_effects);
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(surface.view.busy);

    // Second stop while first is in flight
    let second_effects = surface
      .session
      .handle(PlaybackInput::Intent(PlaybackIntent::Stop), now);
    assert!(second_effects.is_empty());

    // Settle the stop
    let settle_effects = surface.session.handle(
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
    sync_playback_projection(&mut surface, &kernel, false);

    // Stop settled with no notice
    assert!(settle_effects.is_empty());
    assert!(!surface.view.busy);
    assert!(surface.view.now_playing.is_none());
    assert!(surface.view.notice.is_none());
    assert!(surface.notice.is_none());
    assert!(kernel.active_toast.is_none());
  }

  #[test]
  fn stop_and_eof_produce_no_visible_notice_state() {
    let (mut surface, kernel) = active_playback_fixture();
    let now = Instant::now();

    let refresh_effects = surface
      .session
      .handle(PlaybackInput::Intent(PlaybackIntent::Tick), now);
    let (refresh_id, _) = controller_effect(refresh_effects);

    // Simulate EOF refresh settlement
    let settle_effects = surface.session.handle(
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
    sync_playback_projection(&mut surface, &kernel, false);

    assert!(settle_effects.is_empty());
    assert!(surface.view.now_playing.is_none());
    assert!(surface.view.notice.is_none());
    assert!(surface.notice.is_none());
    assert!(kernel.active_toast.is_none());
  }

  #[test]
  fn unavailable_remote_target_does_not_dispatch_commands() {
    let (mut surface, mut kernel) = test_fixture();
    surface.remote_control_state = RemoteControlState::Unavailable;
    let pending = kernel.request_gate.begin_remote_play();
    let remote = surface.remote;
    drop(handle_remote(
      &mut surface,
      &mut kernel,
      false,
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

    assert!(kernel.request_gate.is_current_remote_play(pending));
  }

  #[test]
  fn successful_reconnect_clears_only_the_connection_lost_notice() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.notice = Some(REMOTE_CONNECTION_LOST_NOTICE.to_owned());
    let remote = surface.remote;

    drop(handle_remote(
      &mut surface,
      &mut kernel,
      false,
      RemoteMessage::Finalized {
        remote,
        result: Ok(true),
      },
    ));

    assert!(kernel.notice.is_none());
    kernel.notice = Some("Unrelated notice".to_owned());
    drop(handle_remote(
      &mut surface,
      &mut kernel,
      false,
      RemoteMessage::Finalized {
        remote,
        result: Ok(true),
      },
    ));
    assert_eq!(kernel.notice.as_deref(), Some("Unrelated notice"));
  }

  #[test]
  fn reconnect_stays_connecting_until_capability_registration_finishes() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.remote_control_state = RemoteControlState::Lost;
    let remote = surface.remote;

    let task = handle_remote(
      &mut surface,
      &mut kernel,
      false,
      RemoteMessage::Event {
        remote,
        event: JellyfinWebSocketEvent::Reconnected,
      },
    );

    assert_eq!(task.units(), 1);
    assert_eq!(surface.remote_control_state, RemoteControlState::Connecting);
    drop(handle_remote(
      &mut surface,
      &mut kernel,
      false,
      RemoteMessage::Finalized {
        remote,
        result: Ok(false),
      },
    ));
    assert_eq!(surface.remote_control_state, RemoteControlState::Available);
  }

  #[test]
  fn initial_setup_failure_invalidates_a_later_finalization_success() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.connection = ConnectionPhase::Connected;
    let stale_remote = surface.remote;

    drop(handle_remote(
      &mut surface,
      &mut kernel,
      false,
      RemoteMessage::Started {
        remote: stale_remote,
        result: Err(RemoteStartError::CapabilityRegistrationFailed),
      },
    ));
    drop(handle_remote(
      &mut surface,
      &mut kernel,
      false,
      RemoteMessage::Finalized {
        remote: stale_remote,
        result: Ok(true),
      },
    ));

    assert_eq!(
      surface.remote_control_state,
      RemoteControlState::Unavailable
    );
    assert!(!kernel.request_gate.is_current_remote(stale_remote));
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
    let (mut surface, _kernel) = test_fixture();

    assert!(!quit_may_exit(&surface, true));
    surface.view.quit_may_proceed = true;
    assert!(quit_may_exit(&surface, true));
    surface.remote_stopping = true;
    assert!(!quit_may_exit(&surface, true));
  }

  #[test]
  fn playback_tick_and_settlement_do_not_project_busy_to_ui() {
    let (mut surface, mut kernel) = active_playback_fixture();
    assert!(!surface.view.busy);

    // Tick intent executes Refresh but must NOT project busy to UI
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::Intent(PlaybackIntent::Tick),
    ));
    assert!(surface.in_flight_refresh.is_some());
    assert_eq!(surface.in_flight_command, None);
    assert!(
      !surface.view.busy,
      "periodic refresh tick must not mark playback_view busy (prevents button flickering)"
    );

    // Refresh settlement clears in-flight refresh and keeps busy false
    let refresh_id = surface.in_flight_refresh.unwrap();
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
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
    assert_eq!(surface.in_flight_refresh, None);
    assert!(
      !surface.view.busy,
      "refresh settlement must keep playback_view busy as false"
    );
  }

  #[test]
  fn playback_refresh_transition_to_queued_command_preserves_busy_state() {
    let (mut surface, mut kernel) = active_playback_fixture();
    assert!(!surface.view.busy);

    // 1. Tick intent starts a Refresh
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::Intent(PlaybackIntent::Tick),
    ));
    let refresh_id = surface
      .in_flight_refresh
      .expect("tick must initiate an in-flight refresh");
    assert_eq!(surface.in_flight_command, None);
    assert!(
      !surface.view.busy,
      "periodic refresh tick alone must not mark playback_view busy"
    );

    // 2. Queue a seek command while refresh is in flight
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::Intent(PlaybackIntent::Seek(50.0)),
    ));

    // 3. Settle the in-flight refresh
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
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
    assert_eq!(surface.in_flight_refresh, None);
    let command_id = surface
      .in_flight_command
      .expect("settling refresh must dispatch the queued command and set in_flight_command");
    assert!(
      surface.view.busy,
      "playback_view.busy must remain true while queued command is in flight"
    );

    // 4. Settle the command
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::ControllerSettled {
        id: command_id,
        settlement: Box::new(ControllerSettlement::Controlled(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(50.0),
          warnings: Vec::new(),
        }))),
        started: None,
        tracks: None,
      },
    ));
    // Command marker is cleared and busy is false
    assert_eq!(surface.in_flight_command, None);
    assert!(
      !surface.view.busy,
      "playback_view.busy must be false after command settles"
    );
  }

  #[test]
  fn tray_action_executes_in_update_tray() {
    let (mut surface, mut kernel) = active_playback_fixture();
    assert_eq!(
      surface.view.now_playing.as_ref().map(|np| np.paused),
      Some(false)
    );

    drop(update_tray(
      &mut surface,
      &mut kernel,
      false,
      crate::tray::TrayAction::PlayPause,
    ));

    assert_eq!(
      surface.view.now_playing.as_ref().map(|np| np.paused),
      Some(true)
    );
  }
}
