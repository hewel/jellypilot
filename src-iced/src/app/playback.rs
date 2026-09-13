//! Playback surface (ADR 0029): the MPV playback session and its projected
//! player-bar view, seek/volume previews, track popovers, player artwork, and
//! the folded-in remote Playback Target cluster bridged to the resource-owning
//! [`remote`] runtime (lifecycle, command translation, and registration).
//!
//! Two entry points share the private helpers below: [`update`] reduces
//! [`PlaybackMessage`] and [`update_remote`] reduces [`RemoteMessage`]; each
//! also records the diagnostics/toast follow-ups its cluster produces, so the
//! top-level router is a plain delegation. `quit_requested` is the shell's
//! quit-handshake flag, computed by the router so this module never reads
//! window/shell state.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use iced::Task;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_core::audio_tracks::AudioTrackStore;
use jellypilot_core::config::Settings;
use jellypilot_core::diagnostics::{coalescing_key, DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::request_gate::{RemotePlayToken, RemoteToken, RequestGate, SessionToken};
use jellypilot_core::volume_memory::SeasonVolumeStore;
use jellypilot_core::watchlist::ProfileScope;
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::{
  ticks_to_seconds, VideoLibraryItem, VideoSeasonEpisodes, VideoSeasonEpisodesRequest,
};
use jellypilot_mpv::configured_mpv_args;
use jellypilot_mpv::playback::{
  media_item_from_playable, rich_playable, Playable, PlaybackController, PlaybackControllerConfig,
  PlaybackSelection, PlaybackStartPosition, PlaybackWarning, VolumeMemoryPreference,
};
use jellypilot_mpv::playback_session::{
  seek_intent, volume_intent, AdjacentDirection, ControllerAcceptance, ControllerCommand,
  ControllerSettlement, EffectId, PlaybackEffect, PlaybackEvent, PlaybackInput, PlaybackIntent,
  PlaybackNotice, PlaybackSession, PlaybackStep, PlaybackTransition, SessionView,
};
use jellypilot_mpv::remote_commands::{remote_command_action, RemoteCommandAction};
use jellypilot_session::RemoteControlState;

use crate::i18n::UiText;
use crate::tray::TrayAction;
use jellypilot_mpv::playback::PlaybackError;

use super::accounts;
use super::artwork::{ImageCollection, ImagePriority, ImageSpec};
use super::kernel::Kernel;
use super::message::{Message, PlaybackMessage, RemoteMessage, SettingsMessage};
use super::state::{NoticeLevel, PlaybackControllerHandle};

pub(crate) mod remote;

pub(crate) const PLAYER_IMAGE_KEY: &str = "now-playing";
pub(crate) const PLAYER_THUMBNAIL_KEY: &str = "now-playing-thumbnail";

#[derive(Clone)]
pub enum QueueState {
  Unavailable,
  Loading,
  Ready(Vec<VideoLibraryItem>),
  Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueueSeasonKey {
  series_id: String,
  season_number: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveQueueLoad {
  session: SessionToken,
  generation: u64,
  season: QueueSeasonKey,
}

struct AccountPlaybackHandoff {
  generation: u64,
  settlement: Option<Result<(), String>>,
}

pub(crate) struct AccountHandoffStart {
  pub task: Task<Message>,
  pub playback_cleanup: Option<Result<(), String>>,
}

/// One playback reduction: the task tree to run plus the session's lifecycle
/// transition, which the router consumes for embedded Back navigation.
pub struct PlaybackUpdate {
  pub task: Task<Message>,
  pub transition: PlaybackTransition,
}

impl PlaybackUpdate {
  fn without_transition(task: Task<Message>) -> Self {
    Self {
      task,
      transition: PlaybackTransition::default(),
    }
  }
}

/// Shared only with tasks for one controller. Configuration snapshots and the
/// live memory preference have separate revisions because MPV arguments may
/// change without a volume-memory transition.
struct ControllerConfiguration {
  revision: AtomicU64,
  volume_memory: VolumeMemoryPreference,
}

impl Default for ControllerConfiguration {
  fn default() -> Self {
    Self {
      revision: AtomicU64::new(0),
      volume_memory: VolumeMemoryPreference::new(true),
    }
  }
}

impl ControllerConfiguration {
  fn publish(&self, enabled: bool) -> u64 {
    // Publish without waiting for the controller lock: an in-flight start may
    // still be resolving media when the user disables restoration/persistence.
    self.volume_memory.set_enabled(enabled);
    self.revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
  }

  fn invalidate_pending(&self) {
    // Shutdown still needs the last desired gate to capture the old account's
    // final accepted volume. Retire config snapshots without disabling memory.
    self.revision.fetch_add(1, Ordering::AcqRel);
  }
}

/// Playback surface slice: the playback session machine and its projected
/// view, the resolved current/adjacent playables, the MPV controller handle,
/// seek/volume previews, track popover flags, player-bar artwork, and the
/// remote Playback Target runtime.
pub struct Surface {
  pub notice: Option<UiText>,
  pub artwork: ImageCollection,
  pub artwork_enabled: bool,
  pub controller: Option<PlaybackControllerHandle>,
  controller_configuration: Arc<ControllerConfiguration>,
  pub session: PlaybackSession,
  pub view: SessionView,
  pub playable: Option<Playable>,
  pub adjacent_playables: [Option<Playable>; 2],
  pub queue: QueueState,
  queue_session: Option<SessionToken>,
  queue_season: Option<QueueSeasonKey>,
  queue_generation: u64,
  active_queue_load: Option<ActiveQueueLoad>,
  pub queue_menu_open: bool,
  /// Resource-owning remote target runtime; its view/token gate all remote
  /// readiness, event, and teardown decisions.
  pub remote: remote::Runtime,
  account_playback_handoff: Option<AccountPlaybackHandoff>,
  pub seek_dragging: bool,
  pub volume_dragging: bool,
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
      artwork: ImageCollection::default(),
      artwork_enabled: true,
      view: session.view(),
      session,
      playable: None,
      adjacent_playables: [None, None],
      queue: QueueState::Unavailable,
      queue_session: None,
      queue_season: None,
      queue_generation: 0,
      active_queue_load: None,
      queue_menu_open: false,
      remote: remote::Runtime::new(request_gate),
      account_playback_handoff: None,
      seek_preview: None,
      seek_dragging: false,
      volume_dragging: false,
      volume_preview: None,
      audio_menu_open: false,
      subtitle_menu_open: false,
      controller: None,
      controller_configuration: Arc::default(),
    }
  }
}

/// Playback surface entry point: reduces a [`PlaybackMessage`] and records
/// the diagnostics/toast follow-up for a changed playback notice. The returned
/// transition carries the session's controller acceptance for the router.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  message: PlaybackMessage,
) -> PlaybackUpdate {
  tracing::debug!(
    message = playback_message_name(&message),
    "playback message"
  );
  let previous_notice = surface.notice.as_ref().map(UiText::id);
  let previous_view_notice = surface.view.notice.clone();
  let PlaybackUpdate { task, transition } =
    update_playback(surface, kernel, quit_requested, message);
  let toast_task = record_playback_notice(
    surface,
    kernel,
    previous_notice,
    previous_view_notice.as_ref(),
  );
  PlaybackUpdate {
    task: Task::batch([task, toast_task]),
    transition,
  }
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
  let previous_state = surface.remote.view().state;
  let previous_notice = kernel.notice.as_ref().map(UiText::id);
  let task = handle_remote(surface, kernel, quit_requested, message);
  let toast_task = record_remote_change(surface, kernel, previous_state, previous_notice);
  Task::batch([task, toast_task])
}

fn record_playback_notice(
  surface: &mut Surface,
  kernel: &mut Kernel,
  previous: Option<&str>,
  previous_view_notice: Option<&PlaybackNotice>,
) -> Task<Message> {
  let Some(notice) = surface.notice.clone() else {
    if previous.is_some() {
      kernel.diagnostics.reset_coalescing();
    }
    return Task::none();
  };
  if Some(notice.id()) == previous && surface.view.notice.as_ref() == previous_view_notice {
    return Task::none();
  }
  let level = match &surface.view.notice {
    Some(PlaybackNotice::Warnings(_)) => DiagnosticLevel::Warning,
    _ => DiagnosticLevel::Error,
  };
  let diagnostic = match &surface.view.notice {
    Some(PlaybackNotice::Failed(error)) => error.to_string(),
    Some(PlaybackNotice::CleanupFailed(error)) => error.to_string(),
    Some(PlaybackNotice::Warnings(warnings)) => warnings
      .iter()
      .map(ToString::to_string)
      .collect::<Vec<_>>()
      .join("; "),
    None => "External playback is unavailable because MPV could not be found.".to_owned(),
  };
  let key = coalescing_key("playback", &diagnostic);
  kernel
    .diagnostics
    .record_coalesced(&key, level, DiagnosticCategory::Playback, &diagnostic);

  let toast_level = match level {
    DiagnosticLevel::Error => NoticeLevel::Error,
    _ => NoticeLevel::Warning,
  };
  kernel.show_toast(toast_level, notice)
}

fn record_failed_shutdown_warnings(
  kernel: &mut Kernel,
  warnings: &[PlaybackWarning],
) -> Task<Message> {
  if warnings.is_empty() {
    return Task::none();
  }
  for warning in warnings {
    kernel.diagnostics.record(
      DiagnosticLevel::Warning,
      DiagnosticCategory::Playback,
      format!("Playback cleanup warning: {warning}."),
    );
  }
  kernel.show_toast(
    NoticeLevel::Warning,
    UiText::new("player-reporting-incomplete"),
  )
}

fn record_remote_change(
  surface: &mut Surface,
  kernel: &mut Kernel,
  previous_state: RemoteControlState,
  previous_notice: Option<&str>,
) -> Task<Message> {
  let state = surface.remote.view().state;
  if state != previous_state {
    let (level, message) = match state {
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
    .filter(|notice| Some(notice.id()) != previous_notice)
  {
    return kernel.show_toast(NoticeLevel::Warning, notice);
  }
  Task::none()
}

fn remote_notice(kernel: &mut Kernel, id: &'static str, diagnostic: &str) {
  kernel.diagnostics.record(
    DiagnosticLevel::Warning,
    DiagnosticCategory::RemoteControl,
    diagnostic,
  );
  kernel.notice = Some(UiText::new(id));
}

fn playback_error_text(error: PlaybackError) -> UiText {
  UiText::new(match error {
    PlaybackError::MpvNotFound => "player-mpv-not-found",
    PlaybackError::UnsupportedItemType => "player-unsupported-item",
    PlaybackError::ItemNotPlayable => "player-item-not-playable",
    PlaybackError::InvalidStartPosition => "player-invalid-position",
    PlaybackError::InvalidVolume => "player-invalid-volume",
    PlaybackError::PlaybackInfoUnavailable => "player-playback-info-unavailable",
    PlaybackError::MediaSourceUnavailable => "player-source-unavailable",
    PlaybackError::StreamUrlUnavailable => "player-stream-unavailable",
    PlaybackError::SubtitleUrlUnavailable => "player-subtitle-stream-unavailable",
    PlaybackError::TrackUnavailable => "player-track-unavailable",
    PlaybackError::MpvStartFailed => "player-mpv-start-failed",
    PlaybackError::MpvLoadFailed => "player-mpv-load-failed",
    PlaybackError::MpvControlFailed => "player-mpv-control-failed",
    PlaybackError::NoActivePlayback => "player-no-active-playback",
  })
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
  let revision = surface
    .controller_configuration
    .publish(kernel.settings.snapshot().remember_season_volume());
  if let Some(controller) = surface.controller.as_ref().map(Arc::clone) {
    let desired = Arc::clone(&surface.controller_configuration);
    return Task::perform(
      async move {
        let mut controller = controller.lock().await;
        controller.synchronize_volume_memory_preference().await;
        if desired.revision.load(Ordering::Acquire) != revision {
          return Ok(());
        }
        let result = controller.configure_for_next_start(config).await;
        controller.synchronize_volume_memory_preference().await;
        result
      },
      |result| Message::Settings(SettingsMessage::PlaybackConfigApplied(result)),
    );
  }
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  match discover_playback_controller(kernel, client, config) {
    Ok(mut controller) => {
      surface.controller_configuration = Arc::new(ControllerConfiguration::default());
      surface
        .controller_configuration
        .publish(kernel.settings.snapshot().remember_season_volume());
      controller
        .set_volume_memory_preference(surface.controller_configuration.volume_memory.clone());
      surface.controller = Some(Arc::new(tokio::sync::Mutex::new(controller)));
      let _ = surface.session.handle(
        PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
        Instant::now(),
      );
      sync_playback_projection(surface, kernel, quit_requested);
      surface.notice = None;
      Task::none()
    }
    Err(error) => {
      let _ = surface.session.handle(
        PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(false))),
        Instant::now(),
      );
      sync_playback_projection(surface, kernel, quit_requested);
      surface.notice = Some(playback_error_text(PlaybackError::MpvNotFound));
      let toast_task = kernel.show_toast(
        NoticeLevel::Error,
        playback_error_text(PlaybackError::MpvNotFound),
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
/// Called by the router after settings mutate (ADR 0029). The runtime owns
/// whether the new desired name re-registers; the client write keeps the
/// shared auth header's device name current either way.
pub(crate) fn refinalize_playback_target(
  surface: &mut Surface,
  kernel: &mut Kernel,
) -> Task<Message> {
  let name = kernel
    .settings
    .snapshot()
    .playback_target_name()
    .unwrap_or("JellyPilot")
    .to_owned();
  if let Some(client) = kernel.client.as_ref() {
    client.set_device_name(name.clone());
  }
  let update = surface
    .remote
    .update(remote::Input::Rename(name), &mut kernel.request_gate);
  if kernel.connection == ConnectionPhase::Connected
    && surface.remote.view().state == RemoteControlState::Available
  {
    kernel.diagnostics.record(
      DiagnosticLevel::Info,
      DiagnosticCategory::RemoteControl,
      "Playback target name changed; remote registration requested.",
    );
  }
  apply_remote_update(surface, kernel, false, update)
}

/// Starts the remote Playback Target runtime for the connected account.
/// Called by the router when the login surface connects.
pub(crate) fn start_remote_session(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let name = kernel
    .settings
    .snapshot()
    .playback_target_name()
    .unwrap_or("JellyPilot")
    .to_owned();
  let update = surface.remote.update(
    remote::Input::Start { client, name },
    &mut kernel.request_gate,
  );
  apply_remote_update(surface, kernel, false, update)
}

const REMOTE_CONNECTION_LOST_NOTICE: &str = "player-remote-connection-lost";
const REMOTE_TRACKS_UNAVAILABLE_NOTICE: &str = "player-remote-tracks-unavailable";

fn handle_remote(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  message: RemoteMessage,
) -> Task<Message> {
  match message {
    RemoteMessage::Completed(completion) => {
      let update = surface.remote.update(
        remote::Input::Completed(completion),
        &mut kernel.request_gate,
      );
      apply_remote_update(surface, kernel, quit_requested, update)
    }
    RemoteMessage::Event { remote, event } => {
      let update = surface.remote.update(
        remote::Input::Event { remote, event },
        &mut kernel.request_gate,
      );
      apply_remote_update(surface, kernel, quit_requested, update)
    }
    RemoteMessage::PlayResolved {
      remote,
      play,
      result,
      start_position_ticks,
      selection,
    } => {
      if remote != surface.remote.token()
        || !kernel.request_gate.is_current_remote(remote)
        || !kernel.request_gate.is_current_remote_play(play)
      {
        return Task::none();
      }
      let Ok(item) = *result else {
        remote_notice(
          kernel,
          "player-remote-item-unavailable",
          "Remote playback item could not be loaded.",
        );
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
        PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
          item,
          position,
          intro,
          selection: Box::new(selection),
        })),
      )
      .task
    }
  }
}

/// Applies one runtime update: surfaces its notices, translates accepted
/// commands into playback work, schedules owned futures, and resolves the
/// waiters whose teardown just settled.
fn apply_remote_update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  update: remote::Update,
) -> Task<Message> {
  let remote::Update {
    work,
    notices,
    commands,
    settled,
  } = update;
  for notice in notices {
    apply_remote_notice(kernel, notice);
  }
  let mut tasks: Vec<Task<Message>> = work
    .into_iter()
    .map(|work| {
      Task::perform(work.run(), |completion| {
        Message::Remote(RemoteMessage::Completed(completion))
      })
    })
    .collect();
  for (remote, command) in commands {
    tasks.push(handle_remote_command(
      surface,
      kernel,
      quit_requested,
      remote,
      command,
    ));
  }
  for waiter in settled {
    match waiter {
      remote::Waiter::Quit => {
        if quit_may_exit(surface, quit_requested) {
          tasks.push(iced::exit());
        }
      }
      remote::Waiter::Account(generation) => {
        tasks.push(Task::done(Message::Account(
          accounts::Message::RemoteHandoffSettled { generation },
        )));
      }
      remote::Waiter::Disconnect => {}
    }
  }
  Task::batch(tasks)
}

fn apply_remote_notice(kernel: &mut Kernel, notice: remote::Notice) {
  match notice {
    remote::Notice::StartFailed(error) => remote_notice(
      kernel,
      match error {
        remote::StartError::SessionUnavailable => "player-remote-session-unavailable",
        remote::StartError::ConnectionFailed => "player-remote-connect-failed",
        remote::StartError::CapabilityRegistrationFailed => "player-remote-registration-failed",
      },
      error.diagnostic(),
    ),
    remote::Notice::ValidationPending { reconnected } => remote_notice(
      kernel,
      if reconnected {
        "player-remote-revalidation-pending"
      } else {
        "player-remote-validation-pending"
      },
      if reconnected {
        "Remote playback target reconnected, but server session validation is still pending."
      } else {
        "Remote playback target connected, but server session validation is still pending."
      },
    ),
    remote::Notice::ConnectionLost => remote_notice(
      kernel,
      REMOTE_CONNECTION_LOST_NOTICE,
      "Remote playback connection lost; reconnecting…",
    ),
    remote::Notice::ConnectionRestored => {
      if kernel.notice.as_ref().map(UiText::id) == Some(REMOTE_CONNECTION_LOST_NOTICE) {
        kernel.notice = None;
      }
    }
    remote::Notice::RegistrationFailed => remote_notice(
      kernel,
      "player-remote-registration-failed",
      "Remote playback target capabilities could not be registered.",
    ),
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
        remote_notice(
          kernel,
          REMOTE_TRACKS_UNAVAILABLE_NOTICE,
          "Remote track selection ignored because playback tracks are not loaded.",
        );
        return Task::none();
      };
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(Box::new(intent)),
      )
      .task
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

/// Retires the remote target for the shell's quit handshake; the runtime
/// settles [`remote::Waiter::Quit`] once teardown completes. Called by the
/// router's window/tray quit arms.
pub(crate) fn stop_remote_session_for_quit(
  surface: &mut Surface,
  kernel: &mut Kernel,
) -> Task<Message> {
  let update = surface.remote.update(
    remote::Input::Retire(remote::Waiter::Quit),
    &mut kernel.request_gate,
  );
  apply_remote_update(surface, kernel, true, update)
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
    TrayAction::PlayPause => {
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(Box::new(PlaybackIntent::TogglePaused)),
      )
      .task
    }
    TrayAction::Next => {
      apply_local_playback_intent(
        surface,
        kernel,
        quit_requested,
        PlaybackIntent::PlayAdjacent(AdjacentDirection::Next),
      )
      .task
    }
    TrayAction::Previous => {
      apply_local_playback_intent(
        surface,
        kernel,
        quit_requested,
        PlaybackIntent::PlayAdjacent(AdjacentDirection::Previous),
      )
      .task
    }
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
        PlaybackInput::Intent(Box::new(PlaybackIntent::SetMuted(!muted))),
      )
      .task
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
  clear_queue(surface);
  surface.artwork.clear();
  surface.seek_preview = None;
  surface.volume_preview = None;
  // The remote runtime survives surface initialization and profile handoff;
  // it owns its own lifecycle across account transitions.
  // Retire the old controller's settings tasks before creating an account-scoped
  // replacement. Old futures retain only the retired controller and gate.
  surface.controller_configuration.invalidate_pending();
  surface.controller_configuration = Arc::new(ControllerConfiguration::default());
  surface
    .controller_configuration
    .publish(kernel.settings.snapshot().remember_season_volume());

  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.controller = None;
    return;
  };
  client.set_tmdb_api_key(kernel.settings.snapshot().tmdb_api_key().map(str::to_owned));
  let config = playback_controller_config(kernel.settings.snapshot());
  match discover_playback_controller(kernel, client, config) {
    Ok(mut controller) => {
      controller
        .set_volume_memory_preference(surface.controller_configuration.volume_memory.clone());
      surface.controller = Some(Arc::new(tokio::sync::Mutex::new(controller)));
      let _ = surface.session.handle(
        PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
        Instant::now(),
      );
      surface.view = surface.session.view();
    }
    Err(_) => {
      surface.controller = None;
      surface.notice = Some(playback_error_text(PlaybackError::MpvNotFound));
    }
  }
  sync_tray(surface, kernel, quit_requested);
}

fn playback_controller_config(settings: &Settings) -> PlaybackControllerConfig {
  if let Some(options) = crate::embedded::options() {
    return PlaybackControllerConfig::default()
      .with_subtitle_languages(settings.subtitle_languages().to_vec())
      .with_embedded_ipc(options.ipc.clone())
      .with_volume_memory_enabled(settings.remember_season_volume())
      .with_subtitle_languages(settings.subtitle_languages().to_vec())
      .with_original_audio_enabled(settings.prefer_original_audio());
  }
  let config = PlaybackControllerConfig::default()
    .with_extra_args(configured_mpv_args(settings))
    .with_volume_memory_enabled(settings.remember_season_volume())
    .with_subtitle_languages(settings.subtitle_languages().to_vec())
    .with_original_audio_enabled(settings.prefer_original_audio());
  match settings.mpv_path() {
    Some(path) => config.with_mpv_path(PathBuf::from(path)),
    None => config,
  }
}

fn discover_playback_controller(
  kernel: &mut Kernel,
  client: Arc<jellypilot_media_server::JellyfinClient>,
  config: PlaybackControllerConfig,
) -> Result<PlaybackController, PlaybackError> {
  let connection = client.login().connection_state();
  let mut controller = PlaybackController::discover(client, config)?;
  if let (true, Some(server_url), Some(user_id)) = (
    connection.connected,
    connection.server_url,
    connection.user_id,
  ) {
    match ProfileScope::new(connection.provider, server_url, user_id) {
      Ok(scope) => {
        match AudioTrackStore::load(scope.clone()) {
          Ok(store) => controller.set_audio_track_memory(store),
          Err(error) => {
            kernel.diagnostics.record(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Config,
              format!("Could not load audio track memory: {error}"),
            );
          }
        }
        match SeasonVolumeStore::load(scope) {
          Ok(store) => controller.set_volume_memory(store),
          Err(error) => {
            kernel.diagnostics.record(
              DiagnosticLevel::Warning,
              DiagnosticCategory::Config,
              format!("Could not load season volume memory: {error}"),
            );
          }
        }
      }
      Err(error) => {
        kernel.diagnostics.record(
          DiagnosticLevel::Warning,
          DiagnosticCategory::Config,
          format!("Could not identify playback memory account: {error}"),
        );
      }
    }
  }
  Ok(controller)
}

fn apply_local_playback_intent(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  intent: PlaybackIntent,
) -> PlaybackUpdate {
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
    PlaybackInput::Intent(Box::new(intent)),
  )
}

fn episode_queue_request(
  playable: &Playable,
) -> Option<(QueueSeasonKey, VideoSeasonEpisodesRequest)> {
  let (item_type, series_id, season_number) = match playable {
    Playable::Library(item) => (
      item.item_type.as_str(),
      item.series_id.as_deref(),
      item.season_number,
    ),
    Playable::Detail(item) => (
      item.item_type.as_str(),
      item.series_id.as_deref(),
      item.season_number,
    ),
    Playable::Media(item) => (
      item.item_type.as_str(),
      item.series_id.as_deref(),
      item.parent_index_number,
    ),
  };
  if item_type != "Episode" {
    return None;
  }
  let series_id = series_id?.trim();
  if series_id.is_empty() {
    return None;
  }
  let season = QueueSeasonKey {
    series_id: series_id.to_owned(),
    season_number: season_number?,
  };
  let request = VideoSeasonEpisodesRequest {
    series_id: season.series_id.clone(),
    season_id: None,
    season_number: Some(season.season_number),
  };
  Some((season, request))
}

fn queue_item_start_intent(
  item: VideoLibraryItem,
  intro: jellypilot_mpv::playback_session::IntroAvailability,
) -> PlaybackIntent {
  PlaybackIntent::Start {
    item: Playable::Library(item),
    position: PlaybackStartPosition::Resume,
    intro,
    selection: Box::new(PlaybackSelection::default()),
  }
}

fn load_queue_after_start(
  surface: &mut Surface,
  kernel: &Kernel,
  playable: &Playable,
) -> Task<Message> {
  let Some((season, request)) = episode_queue_request(playable) else {
    clear_queue(surface);
    return Task::none();
  };
  let session = kernel.request_gate.current_session();
  let same_queue =
    surface.queue_session == Some(session) && surface.queue_season.as_ref() == Some(&season);
  if same_queue
    && (matches!(surface.queue, QueueState::Ready(_))
      || (matches!(surface.queue, QueueState::Loading)
        && surface.active_queue_load.as_ref().is_some_and(|active| {
          active.session == session
            && active.generation == surface.queue_generation
            && active.season == season
        })))
  {
    return Task::none();
  }
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.active_queue_load = None;
    surface.queue_session = Some(session);
    surface.queue_season = Some(season);
    surface.queue = QueueState::Failed;
    surface.queue_menu_open = false;
    return Task::none();
  };
  surface.queue_generation = surface.queue_generation.wrapping_add(1);
  let generation = surface.queue_generation;
  surface.queue_session = Some(session);
  surface.queue_season = Some(season.clone());
  surface.active_queue_load = Some(ActiveQueueLoad {
    session,
    generation,
    season: season.clone(),
  });
  surface.queue = QueueState::Loading;
  let series_id = season.series_id;
  let season_number = season.season_number;
  Task::perform(
    async move {
      client
        .library()
        .season_episodes(request)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| {
      Message::Playback(PlaybackMessage::QueueLoaded {
        session,
        generation,
        series_id,
        season_number,
        result,
      })
    },
  )
}

fn apply_queue_loaded(
  surface: &mut Surface,
  kernel: &Kernel,
  session: SessionToken,
  generation: u64,
  series_id: String,
  season_number: i32,
  result: Result<VideoSeasonEpisodes, String>,
) {
  let season = QueueSeasonKey {
    series_id,
    season_number,
  };
  let completion = ActiveQueueLoad {
    session,
    generation,
    season: season.clone(),
  };
  if !kernel.request_gate.is_current_session(session)
    || surface.active_queue_load.as_ref() != Some(&completion)
    || surface.queue_generation != generation
    || surface.queue_session != Some(session)
    || surface.queue_season.as_ref() != Some(&season)
    || surface
      .playable
      .as_ref()
      .and_then(episode_queue_request)
      .is_none_or(|(current, _)| current != season)
  {
    return;
  }
  surface.active_queue_load = None;
  surface.queue = match result {
    Ok(season) => QueueState::Ready(season.episodes),
    Err(_) => QueueState::Failed,
  };
}

fn clear_queue(surface: &mut Surface) {
  surface.queue_generation = surface.queue_generation.wrapping_add(1);
  surface.active_queue_load = None;
  surface.queue_session = None;
  surface.queue_season = None;
  surface.queue = QueueState::Unavailable;
  surface.queue_menu_open = false;
}

fn update_playback(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  message: PlaybackMessage,
) -> PlaybackUpdate {
  match message {
    PlaybackMessage::Intent(intent) => {
      apply_local_playback_intent(surface, kernel, quit_requested, *intent)
    }
    PlaybackMessage::Event(event) => apply_playback_input(
      surface,
      kernel,
      quit_requested,
      PlaybackInput::Event(Box::new(*event)),
    ),
    PlaybackMessage::SeekDragStarted => {
      surface.seek_dragging = true;
      surface.seek_preview = None;
      PlaybackUpdate::without_transition(Task::none())
    }
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
      PlaybackUpdate::without_transition(Task::none())
    }
    PlaybackMessage::SeekReleased => {
      surface.seek_dragging = false;
      let Some(position) = surface.seek_preview else {
        return PlaybackUpdate::without_transition(Task::none());
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
        return PlaybackUpdate::without_transition(Task::none());
      };
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(Box::new(intent)),
      )
    }
    PlaybackMessage::SeekAdjusted(position) => {
      let Some(intent) = seek_intent(
        position,
        surface
          .view
          .now_playing
          .as_ref()
          .and_then(|view| view.duration_seconds),
        surface.view.now_playing.is_some(),
      ) else {
        return PlaybackUpdate::without_transition(Task::none());
      };
      if let PlaybackIntent::Seek(position) = intent {
        surface.seek_preview = Some(position);
      }
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(Box::new(intent)),
      )
    }
    PlaybackMessage::VolumeDragStarted => {
      surface.volume_dragging = true;
      surface.volume_preview = None;
      PlaybackUpdate::without_transition(Task::none())
    }
    PlaybackMessage::VolumeChanged(volume) => {
      surface.volume_preview =
        volume_intent(volume, surface.view.now_playing.is_some()).and_then(|intent| match intent {
          PlaybackIntent::SetVolume(volume) => Some(volume),
          _ => None,
        });
      PlaybackUpdate::without_transition(Task::none())
    }
    PlaybackMessage::VolumeReleased => {
      surface.volume_dragging = false;
      let Some(volume) = surface.volume_preview else {
        return PlaybackUpdate::without_transition(Task::none());
      };
      let Some(intent) = volume_intent(volume, surface.view.now_playing.is_some()) else {
        return PlaybackUpdate::without_transition(Task::none());
      };
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(Box::new(intent)),
      )
    }
    PlaybackMessage::VolumeAdjusted(volume) => {
      let Some(intent) = volume_intent(volume, surface.view.now_playing.is_some()) else {
        return PlaybackUpdate::without_transition(Task::none());
      };
      if let PlaybackIntent::SetVolume(volume) = intent {
        surface.volume_preview = Some(volume);
      }
      apply_playback_input(
        surface,
        kernel,
        quit_requested,
        PlaybackInput::Intent(Box::new(intent)),
      )
    }
    PlaybackMessage::AudioMenuToggled => {
      surface.audio_menu_open = !surface.audio_menu_open;
      surface.subtitle_menu_open = false;
      surface.queue_menu_open = false;
      PlaybackUpdate::without_transition(Task::none())
    }
    PlaybackMessage::AudioMenuDismissed => {
      surface.audio_menu_open = false;
      PlaybackUpdate::without_transition(Task::none())
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
      surface.queue_menu_open = false;
      PlaybackUpdate::without_transition(Task::none())
    }
    PlaybackMessage::SubtitleMenuDismissed => {
      surface.subtitle_menu_open = false;
      PlaybackUpdate::without_transition(Task::none())
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
    PlaybackMessage::QueueMenuToggled => {
      surface.queue_menu_open = !surface.queue_menu_open;
      surface.audio_menu_open = false;
      surface.subtitle_menu_open = false;
      PlaybackUpdate::without_transition(if surface.queue_menu_open {
        super::view::player::reveal_current_queue_item()
      } else {
        Task::none()
      })
    }
    PlaybackMessage::QueueMenuDismissed => {
      surface.queue_menu_open = false;
      PlaybackUpdate::without_transition(Task::none())
    }
    PlaybackMessage::QueueItemSelected(item) => {
      surface.queue_menu_open = false;
      let intro = kernel.intro_availability();
      apply_local_playback_intent(
        surface,
        kernel,
        quit_requested,
        queue_item_start_intent(*item, intro),
      )
    }
    PlaybackMessage::QueueLoaded {
      session,
      generation,
      series_id,
      season_number,
      result,
    } => {
      let was_loading = matches!(surface.queue, QueueState::Loading);
      apply_queue_loaded(
        surface,
        kernel,
        session,
        generation,
        series_id,
        season_number,
        result,
      );
      PlaybackUpdate::without_transition(
        if was_loading && surface.queue_menu_open && matches!(surface.queue, QueueState::Ready(_)) {
          super::view::player::reveal_current_queue_item()
        } else {
          Task::none()
        },
      )
    }
    PlaybackMessage::ControllerSettled {
      id,
      settlement,
      started,
      tracks,
    } => {
      // The session decides whether this settlement is a normal acceptance,
      // detached cleanup, or stale; sidecars and shutdown effects follow its
      // verdict, never the raw message.
      let started_ok = matches!(settlement.as_ref(), ControllerSettlement::Started(Ok(_)));
      let shutdown_cleanup = match settlement.as_ref() {
        ControllerSettlement::Shutdown(outcome) => {
          Some(outcome.cleanup.map_err(|error| error.to_string()))
        }
        _ => None,
      };
      let failed_shutdown_warnings = match settlement.as_ref() {
        ControllerSettlement::Shutdown(outcome) if outcome.cleanup.is_err() => {
          outcome.warnings.clone()
        }
        _ => Vec::new(),
      };
      let settlement_error = match settlement.as_ref() {
        ControllerSettlement::Started(Err(error)) => Some(error.to_string()),
        _ => None,
      };
      let PlaybackStep {
        effects,
        transition,
      } = surface.session.handle(
        PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
          id,
          settlement: *settlement,
        })),
        Instant::now(),
      );
      let accepted = matches!(transition.controller, ControllerAcceptance::Applied { .. });
      let cleanup_accepted = matches!(
        transition.controller,
        ControllerAcceptance::Applied { .. } | ControllerAcceptance::DetachedCleanup
      );
      if transition.replacement_accepted {
        // An accepted replacement invalidates transient UI immediately, even
        // when the new controller effect only just dispatched.
        if crate::embedded::enabled() {
          cancel_slider_drags(surface);
        }
      }
      let started = if accepted && started_ok {
        started
      } else {
        None
      };
      if let Some(playable) = started.as_deref() {
        surface.playable = Some(playable.clone());
        surface.adjacent_playables = [None, None];
      }
      let mut tasks = vec![finish_playback_step(
        surface,
        kernel,
        quit_requested,
        effects,
      )];
      if cleanup_accepted && !failed_shutdown_warnings.is_empty() {
        tasks.push(record_failed_shutdown_warnings(
          kernel,
          &failed_shutdown_warnings,
        ));
      }
      tracing::debug!(
        started = ?started.as_deref().map(|playable| (playable_kind(playable), playable.image_id().map(str::to_owned))),
        now_playing = ?surface.view.now_playing.as_ref().map(|view| view.item.item_id.clone()),
        playable = ?surface.playable.as_ref().map(|playable| (playable_kind(playable), playable.image_id().map(str::to_owned))),
        error = ?settlement_error,
        "controller settled"
      );
      if accepted {
        if let Some(result) = tracks {
          tasks.push(
            apply_playback_input(
              surface,
              kernel,
              quit_requested,
              PlaybackInput::Event(Box::new(PlaybackEvent::TracksSettled { id, result })),
            )
            .task,
          );
        }
        if let Some(playable) = started.as_deref() {
          tasks.push(load_queue_after_start(surface, kernel, playable));
        }
      }
      if cleanup_accepted && matches!(shutdown_cleanup, Some(Ok(()))) {
        surface.controller_configuration.invalidate_pending();
        surface.controller = None;
        let _ = surface.session.handle(
          PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(false))),
          Instant::now(),
        );
        sync_playback_projection(surface, kernel, quit_requested);
      }
      if cleanup_accepted {
        if let (Some(result), Some(handoff)) =
          (shutdown_cleanup, surface.account_playback_handoff.as_mut())
        {
          handoff.settlement = Some(result);
        }
      }
      if !surface.view.busy {
        if !surface.seek_dragging {
          surface.seek_preview = None;
        }
        if !surface.volume_dragging {
          surface.volume_preview = None;
        }
      }
      tasks.push(clear_inactive_playback(surface));
      if quit_may_exit(surface, quit_requested) {
        tasks.push(iced::exit());
      }
      PlaybackUpdate {
        task: Task::batch(tasks),
        transition,
      }
    }
    PlaybackMessage::AdjacentSettled {
      remote,
      play,
      id,
      direction,
      result,
      detail,
    } => {
      if remote != surface.remote.token()
        || !kernel.request_gate.is_current_remote(remote)
        || !kernel.request_gate.is_current_remote_play(play)
      {
        return PlaybackUpdate::without_transition(Task::none());
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
      tasks.push(
        apply_playback_input(
          surface,
          kernel,
          quit_requested,
          PlaybackInput::Event(Box::new(PlaybackEvent::AdjacentSettled {
            id,
            direction,
            result,
          })),
        )
        .task,
      );
      PlaybackUpdate::without_transition(Task::batch(tasks))
    }
    PlaybackMessage::ArtworkLoaded(completion) => {
      surface
        .artwork
        .settle(kernel.request_gate.current_session(), completion);
      PlaybackUpdate::without_transition(Task::none())
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
) -> PlaybackUpdate {
  let PlaybackStep {
    effects,
    transition,
  } = surface.session.handle(input, Instant::now());
  if transition.replacement_accepted && crate::embedded::enabled() {
    // An accepted replacement invalidates transient UI immediately, even when
    // its controller effect is still queued behind an in-flight command.
    cancel_slider_drags(surface);
  }
  PlaybackUpdate {
    task: finish_playback_step(surface, kernel, quit_requested, effects),
    transition,
  }
}

/// Executes the effects of one session step, re-projects the view, and
/// completes the shell's quit handshake when the session is done.
fn finish_playback_step(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  effects: Vec<PlaybackEffect>,
) -> Task<Message> {
  let task = execute_playback_effects(surface, kernel, effects);
  sync_playback_projection(surface, kernel, quit_requested);
  let artwork_task = ensure_player_artwork(surface, kernel);
  if quit_may_exit(surface, quit_requested) {
    Task::batch([task, artwork_task, iced::exit()])
  } else {
    Task::batch([task, artwork_task])
  }
}

pub(crate) fn cancel_slider_drags(surface: &mut Surface) {
  surface.seek_dragging = false;
  surface.volume_dragging = false;
  surface.seek_preview = None;
  surface.volume_preview = None;
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
    PlaybackMessage::SeekDragStarted => "seek-drag-started",
    PlaybackMessage::SeekAdjusted(_) => "seek-adjusted",
    PlaybackMessage::VolumeDragStarted => "volume-drag-started",
    PlaybackMessage::VolumeAdjusted(_) => "volume-adjusted",
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
    PlaybackMessage::QueueMenuToggled => "queue-menu-toggled",
    PlaybackMessage::QueueMenuDismissed => "queue-menu-dismissed",
    PlaybackMessage::QueueItemSelected(_) => "queue-item-selected",
    PlaybackMessage::QueueLoaded { .. } => "queue-loaded",
    PlaybackMessage::ControllerSettled { .. } => "controller-settled",
    PlaybackMessage::AdjacentSettled { .. } => "adjacent-settled",
    PlaybackMessage::ArtworkLoaded(_) => "artwork-loaded",
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
/// cleaning up and the remote runtime is quiescent.
pub(crate) fn quit_may_exit(surface: &Surface, quit_requested: bool) -> bool {
  quit_requested && surface.view.quit_may_proceed && surface.remote.view().quiescent
}

pub(crate) fn sync_playback_projection(
  surface: &mut Surface,
  kernel: &Kernel,
  quit_requested: bool,
) {
  surface.view = surface.session.view();
  surface.notice = surface.view.notice.as_ref().map(|notice| match notice {
    PlaybackNotice::Failed(error) => playback_error_text(*error),
    PlaybackNotice::Warnings(_) => UiText::new("player-setup-incomplete"),
    PlaybackNotice::CleanupFailed(_) => UiText::new("player-cleanup-failed"),
  });
  // Embedded playback owns session idle inhibition; the call is a no-op
  // unless the desired state changed or a surface is (un)available.
  crate::embedded::idle::set_desired(crate::embedded::idle::wants_inhibit(
    surface.view.now_playing.as_ref(),
  ));
  sync_tray(surface, kernel, quit_requested);
}

/// Mirrors the projected playback view into the tray menu. Writes only
/// kernel tray state; `pub(crate)` for the router's tray Quit arm.
pub(crate) fn sync_tray(surface: &Surface, kernel: &Kernel, quit_requested: bool) {
  if let Some(tray) = &kernel.tray {
    tray.sync(&surface.view, quit_requested, kernel.locale);
  }
}

/// Releases only Now Playing's image demand while its window is hidden.
pub(crate) fn suspend_artwork(surface: &mut Surface) {
  surface.artwork_enabled = false;
  surface.artwork.clear();
}

/// Restores the selected image after the shell makes playback visible.
pub(crate) fn resume_artwork(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  surface.artwork_enabled = true;
  ensure_player_artwork(surface, kernel)
}

fn clear_inactive_playback(surface: &mut Surface) -> Task<Message> {
  // Controller occupancy can transiently project no Now Playing between
  // files; clearing here would wipe the incoming item's artwork.
  if surface.view.lifecycle.retain_presentation {
    return Task::none();
  }
  tracing::debug!(
    settled = surface.view.lifecycle.settled,
    playable = ?surface.playable.as_ref().map(playable_kind),
    "clearing inactive playback"
  );
  surface.playable = None;
  surface.adjacent_playables = [None, None];
  clear_queue(surface);
  surface.artwork.clear();
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
    PlaybackEffect::Controller(id, command) => execute_controller_command(surface, id, command),
    PlaybackEffect::LookupAdjacent(id, direction) => {
      let Some(play) = adjacent_play else {
        return Task::none();
      };
      let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
        return Task::done(Message::Playback(PlaybackMessage::AdjacentSettled {
          remote: surface.remote.token(),
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
      let remote = surface.remote.token();
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
      controller.synchronize_volume_memory_preference().await;
      match command {
        ControllerCommand::Start {
          item,
          position,
          selection,
          continue_playback,
        } => {
          if !continue_playback {
            controller.discard_continuation();
          }
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
        ControllerCommand::ToggleFullscreen => (
          ControllerSettlement::Controlled(controller.toggle_fullscreen().await),
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
          (ControllerSettlement::Shutdown(outcome), None)
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
  if !surface.artwork_enabled {
    return Task::none();
  }
  let mut specs = Vec::with_capacity(2);
  if let Some(playable) = surface.playable.as_ref() {
    if let Some(image_id) = playable.image_id() {
      specs.push(ImageSpec {
        key: PLAYER_IMAGE_KEY.to_owned(),
        image_id: image_id.to_owned(),
        size_class: ArtworkSizeClass::Card,
        derived: DerivedArtwork::default(),
      });
    }
    if crate::embedded::enabled() {
      if let Some(image_id) = player_thumbnail_id(playable) {
        specs.push(ImageSpec {
          key: PLAYER_THUMBNAIL_KEY.to_owned(),
          image_id: image_id.to_owned(),
          size_class: ArtworkSizeClass::Card,
          derived: DerivedArtwork::default(),
        });
      }
    }
  }
  surface.artwork.retain(&specs);
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  Task::batch(specs.into_iter().map(|spec| {
    surface.artwork.observe(
      kernel.request_gate.current_session(),
      spec,
      Some(ImagePriority::Visible),
      Arc::clone(&client),
      Arc::clone(&kernel.artwork_adapter),
      |completion| Message::Playback(PlaybackMessage::ArtworkLoaded(completion)),
    )
  }))
}

fn player_thumbnail_id(playable: &Playable) -> Option<&str> {
  match playable {
    Playable::Library(item) => super::home::landscape_image_id(item),
    Playable::Detail(item) if item.item_type.eq_ignore_ascii_case("Episode") => item
      .artwork_image_id
      .as_deref()
      .or(item.backdrop_image_id.as_deref()),
    Playable::Detail(item) => item.backdrop_image_id.as_deref(),
    Playable::Media(_) => None,
  }
}

/// Starts the teardown barrier used by profile switch, Disconnect, and active
/// Sign Out. The caller keeps the old [`Kernel`] client alive until both the
/// remote runtime's account waiter and playback cleanup settlement complete.
pub(crate) fn begin_account_handoff(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
  generation: u64,
) -> AccountHandoffStart {
  surface.controller_configuration.invalidate_pending();
  let playback = apply_playback_input(
    surface,
    kernel,
    quit_requested,
    PlaybackInput::Intent(Box::new(PlaybackIntent::Disconnect)),
  );
  clear_queue(surface);
  let remote_update = surface.remote.update(
    remote::Input::Retire(remote::Waiter::Account(generation)),
    &mut kernel.request_gate,
  );
  let remote = apply_remote_update(surface, kernel, quit_requested, remote_update);

  let playback_cleanup = if surface.view.can_start_login {
    surface.account_playback_handoff = None;
    Some(Ok(()))
  } else {
    surface.account_playback_handoff = Some(AccountPlaybackHandoff {
      generation,
      settlement: None,
    });
    None
  };

  AccountHandoffStart {
    task: Task::batch([playback.task, remote]),
    playback_cleanup,
  }
}

/// Takes one sanitized playback-cleanup settlement for the account reducer.
pub(crate) fn take_account_handoff_settlement(
  surface: &mut Surface,
) -> Option<(u64, Result<(), String>)> {
  let handoff = surface.account_playback_handoff.as_mut()?;
  let settlement = handoff.settlement.take()?;
  let generation = handoff.generation;
  surface.account_playback_handoff = None;
  Some((generation, settlement))
}

/// Tears down playback and retires the remote target on sign-out/disconnect.
/// The router performs the other surfaces' resets around this call (ADR 0029).
pub(crate) fn disconnect(
  surface: &mut Surface,
  kernel: &mut Kernel,
  quit_requested: bool,
) -> Task<Message> {
  surface.controller_configuration.invalidate_pending();
  let playback = apply_playback_input(
    surface,
    kernel,
    quit_requested,
    PlaybackInput::Intent(Box::new(PlaybackIntent::Disconnect)),
  );
  clear_queue(surface);
  let remote_update = surface.remote.update(
    remote::Input::Retire(remote::Waiter::Disconnect),
    &mut kernel.request_gate,
  );
  let remote = apply_remote_update(surface, kernel, quit_requested, remote_update);
  Task::batch([playback.task, remote])
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Instant;

  use jellypilot_auth::AuthStore;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::intro_skipper::IntroSkipMode;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::{
    JellyfinClient, MediaItem, VideoItemDetail, VideoLibraryItem, VideoSeasonEpisodes,
  };
  use jellypilot_mpv::playback::{
    NowPlayingItem, PlaybackEndReason, PlaybackOutcome, PlaybackRefreshOutcome,
    PlaybackRefreshState, PlaybackSelection, PlaybackSnapshot,
  };
  use jellypilot_mpv::playback_session::{IntroAvailability, NowPlayingView, TracksView};
  use jellypilot_session::{GeneralCommand, JellyfinCommand, JellyfinWebSocketEvent, PlayRequest};

  use super::*;

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let mut request_gate = RequestGate::default();
    let surface = Surface::new(&mut request_gate);
    let kernel = Kernel {
      locale: crate::i18n::Localizer::default(),
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
      avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      profile_avatars: Default::default(),
    };
    (surface, kernel)
  }

  #[test]
  fn different_warnings_with_the_same_ui_summary_remain_in_diagnostics() {
    let (mut surface, mut kernel) = test_fixture();
    let first = PlaybackNotice::Warnings(vec![PlaybackWarning::PlaybackStartNotReported]);
    surface.notice = Some(UiText::new("player-setup-incomplete"));
    surface.view.notice = Some(first.clone());
    drop(record_playback_notice(
      &mut surface,
      &mut kernel,
      None,
      None,
    ));

    let next = PlaybackWarning::PlaybackProgressNotReported;
    surface.view.notice = Some(PlaybackNotice::Warnings(vec![next]));
    drop(record_playback_notice(
      &mut surface,
      &mut kernel,
      Some("player-setup-incomplete"),
      Some(&first),
    ));
    assert!(
      kernel
        .diagnostics
        .rows()
        .any(|event| event.message == next.to_string()),
      "a shared localized summary must not suppress a distinct technical warning"
    );
  }

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      community_rating: None,
      episode_count: None,
      last_played_date: None,
      premiere_date: None,
      logo_image_id: None,
      id: id.to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      runtime_seconds: Some(1_800.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: Some(season_number),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
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
        original_language: None,
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
    let (surface, kernel, _) = active_playback_fixture_with_auxiliary();
    (surface, kernel)
  }

  fn active_playback_fixture_with_auxiliary() -> (Surface, Kernel, Vec<PlaybackEffect>) {
    let (mut surface, kernel) = test_fixture();
    let now = Instant::now();
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let start = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      })),
      now,
    );
    let (id, _) = controller_effect(start.effects);
    let auxiliary = surface
      .session
      .handle(
        PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
          id,
          settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
            snapshot: playback_snapshot(10.0),
            warnings: Vec::new(),
          })),
        })),
        now,
      )
      .effects;
    surface.view = surface.session.view();
    (surface, kernel, auxiliary)
  }

  #[test]
  fn queued_tray_and_remote_replacements_block_adjustments_before_start_dispatches() {
    for remote_start in [false, true] {
      let (mut surface, mut kernel, auxiliary) = active_playback_fixture_with_auxiliary();
      for effect in auxiliary {
        if let PlaybackEffect::LookupAdjacent(id, AdjacentDirection::Next) = effect {
          drop(apply_playback_input(
            &mut surface,
            &mut kernel,
            false,
            PlaybackInput::Event(Box::new(PlaybackEvent::AdjacentSettled {
              id,
              direction: AdjacentDirection::Next,
              result: Ok(Some(media_item("episode-2"))),
            })),
          ));
        }
      }
      let seek = surface.session.handle(
        PlaybackInput::Intent(Box::new(PlaybackIntent::Seek(15.0))),
        Instant::now(),
      );
      let (seek_id, _) = controller_effect(seek.effects);
      surface.view = surface.session.view();
      let before = surface.view.lifecycle.replacement_generation;
      if remote_start {
        let remote = surface.remote.token();
        let play = kernel.request_gate.begin_remote_play();
        drop(update_remote(
          &mut surface,
          &mut kernel,
          false,
          RemoteMessage::PlayResolved {
            remote,
            play,
            result: Box::new(Ok(Playable::Library(episode("episode-2", 1)))),
            start_position_ticks: None,
            selection: PlaybackSelection::default(),
          },
        ));
      } else {
        drop(update_tray(
          &mut surface,
          &mut kernel,
          false,
          TrayAction::Next,
        ));
      }
      assert!(
        surface.view.lifecycle.replacing,
        "embedded keyboard must be blocked before the queued Start effect runs"
      );
      assert_ne!(
        surface.view.lifecycle.replacement_generation, before,
        "old seek targets and Back intent must be invalidated"
      );
      drop(update(
        &mut surface,
        &mut kernel,
        false,
        PlaybackMessage::ControllerSettled {
          id: seek_id,
          settlement: Box::new(ControllerSettlement::Controlled(Ok(PlaybackOutcome {
            snapshot: playback_snapshot(15.0),
            warnings: Vec::new(),
          }))),
          started: None,
          tracks: None,
        },
      ));
      assert!(
        surface.view.lifecycle.replacing,
        "the barrier remains during replacement startup"
      );
      assert!(
        !surface.view.lifecycle.settled,
        "the queued replacement start is now in flight"
      );
    }
  }

  #[test]
  fn discrete_slider_adjustments_dispatch_without_starting_a_drag() {
    for (message, seek) in [
      (PlaybackMessage::SeekAdjusted(120.0), true),
      (PlaybackMessage::VolumeAdjusted(42.0), false),
    ] {
      let (mut surface, mut kernel) = active_playback_fixture();
      let now = Instant::now();
      let (refresh_id, _) = controller_effect(
        surface
          .session
          .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now)
          .effects,
      );
      surface.view = surface.session.view();
      drop(update_playback(&mut surface, &mut kernel, false, message));
      assert!(!surface.seek_dragging && !surface.volume_dragging);
      let (_, command) = controller_effect(
        surface
          .session
          .handle(
            PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
              id: refresh_id,
              settlement: ControllerSettlement::Refreshed {
                outcome: PlaybackRefreshOutcome {
                  snapshot: playback_snapshot(10.0),
                  state: PlaybackRefreshState::Active,
                  warnings: Vec::new(),
                },
                client_messages: Vec::new(),
              },
            })),
            now,
          )
          .effects,
      );
      if seek {
        assert!(matches!(command, ControllerCommand::Seek(120.0)));
      } else {
        assert!(matches!(command, ControllerCommand::SetVolume(42.0)));
      }
    }
  }

  #[test]
  fn unchanged_slider_press_holds_controls_without_committing_a_stale_preview() {
    let (mut surface, mut kernel) = active_playback_fixture();
    surface.seek_preview = Some(90.0);
    surface.volume_preview = Some(35.0);
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::SeekDragStarted,
    ));
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::VolumeDragStarted,
    ));
    assert!(surface.seek_dragging && surface.volume_dragging);
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
      PlaybackMessage::VolumeReleased,
    ));
    assert!(!surface.seek_dragging && !surface.volume_dragging);
    assert!(surface.view.lifecycle.settled);
    assert!(!surface.view.busy);
  }

  #[test]
  fn fullscreen_switches_cancel_previews_and_late_releases_do_not_commit() {
    let (playback, kernel) = active_playback_fixture();
    let mut state = super::super::state::State::boot(false);
    state.playback = playback;
    state.kernel = kernel;
    state.shell.window_id = Some(iced::window::Id::unique());
    for fullscreen in [true, false] {
      state.playback.seek_dragging = true;
      state.playback.volume_dragging = true;
      state.playback.seek_preview = Some(90.0);
      state.playback.volume_preview = Some(35.0);
      drop(super::super::shell::toggle_player_fullscreen(&mut state));
      assert_eq!(state.shell.player_fullscreen, fullscreen);
      assert!(!state.playback.seek_dragging && !state.playback.volume_dragging);
      assert_eq!(state.playback.seek_preview, None);
      assert_eq!(state.playback.volume_preview, None);
      drop(update_playback(
        &mut state.playback,
        &mut state.kernel,
        false,
        PlaybackMessage::SeekReleased,
      ));
      drop(update_playback(
        &mut state.playback,
        &mut state.kernel,
        false,
        PlaybackMessage::VolumeReleased,
      ));
      assert!(state.playback.view.lifecycle.settled);
    }
  }

  #[test]
  fn fullscreen_no_op_does_not_cancel_a_drag() {
    let (playback, kernel) = active_playback_fixture();
    let mut state = super::super::state::State::boot(false);
    state.playback = playback;
    state.kernel = kernel;
    state.shell.window_id = None;
    state.playback.seek_dragging = true;
    state.playback.seek_preview = Some(90.0);
    drop(super::super::shell::toggle_player_fullscreen(&mut state));
    drop(super::super::shell::exit_player_fullscreen(&mut state));
    assert!(state.playback.seek_dragging);
    assert_eq!(state.playback.seek_preview, Some(90.0));
  }

  #[test]
  fn transport_refresh_preserves_active_drag_target_until_release() {
    let (mut surface, mut kernel) = active_playback_fixture();
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::SeekDragStarted,
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::SeekChanged(90.0),
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::VolumeDragStarted,
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::VolumeChanged(35.0),
    ));
    let refresh = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)),
      Instant::now(),
    );
    let (id, _) = controller_effect(refresh.effects);
    surface.view = surface.session.view();
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::ControllerSettled {
        id,
        settlement: Box::new(ControllerSettlement::Refreshed {
          outcome: jellypilot_mpv::playback::PlaybackRefreshOutcome {
            snapshot: playback_snapshot(11.0),
            state: jellypilot_mpv::playback::PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        }),
        started: None,
        tracks: None,
      },
    ));
    assert_eq!(surface.seek_preview, Some(90.0));
    assert_eq!(surface.volume_preview, Some(35.0));
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::SeekReleased,
    ));
    assert!(!surface.seek_dragging);
    assert_eq!(
      surface.seek_preview,
      Some(90.0),
      "release retains the chosen target while its command settles"
    );
    assert!(!surface.view.lifecycle.settled);
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::VolumeReleased,
    ));
    assert!(!surface.volume_dragging);
    assert_eq!(surface.volume_preview, Some(35.0));
  }

  #[test]
  fn library_episode_derives_its_season_queue_request() {
    let (season, request) = episode_queue_request(&Playable::Library(episode("episode-1", 2)))
      .expect("library episode should identify its season");

    assert_eq!(
      (season.series_id.as_str(), season.season_number),
      ("show-1", 2)
    );
    assert_eq!(
      (request.series_id.as_str(), request.season_number),
      ("show-1", Some(2))
    );
    assert!(request.season_id.is_none());
  }

  #[test]
  fn detail_episode_derives_its_season_queue_request() {
    let (season, request) = episode_queue_request(&Playable::Detail(detail_with_series_poster(
      "episode-1",
      "poster",
    )))
    .expect("detail episode should identify its season");

    assert_eq!(
      (season.series_id.as_str(), season.season_number),
      ("show-1", 3)
    );
    assert_eq!(
      (request.series_id.as_str(), request.season_number),
      ("show-1", Some(3))
    );
    assert!(request.season_id.is_none());
  }

  #[test]
  fn media_episode_derives_its_season_queue_request() {
    let (season, request) = episode_queue_request(&Playable::Media(media_item("episode-1")))
      .expect("media episode should identify its season");

    assert_eq!(
      (season.series_id.as_str(), season.season_number),
      ("series-1", 1)
    );
    assert_eq!(
      (request.series_id.as_str(), request.season_number),
      ("series-1", Some(1))
    );
    assert!(request.season_id.is_none());
  }

  #[test]
  fn successful_episode_start_enters_queue_loading_state() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let now = Instant::now();
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let started = Playable::Library(episode("episode-1", 1));
    let start = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: started.clone(),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      })),
      now,
    );
    let (id, _) = controller_effect(start.effects);

    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::ControllerSettled {
        id,
        settlement: Box::new(ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(0.0),
          warnings: Vec::new(),
        }))),
        started: Some(Box::new(started)),
        tracks: None,
      },
    ));

    assert!(matches!(surface.queue, QueueState::Loading));
  }

  #[test]
  fn same_season_start_reuses_the_active_queue_load() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let first = Playable::Library(episode("episode-1", 1));
    surface.playable = Some(first.clone());

    let first_task = load_queue_after_start(&mut surface, &kernel, &first);
    let generation = surface.queue_generation;
    let active = surface.active_queue_load.clone();
    let second_task = load_queue_after_start(
      &mut surface,
      &kernel,
      &Playable::Library(episode("episode-2", 1)),
    );

    assert_eq!(first_task.units(), 1);
    assert_eq!(second_task.units(), 0);
    assert_eq!(surface.queue_generation, generation);
    assert_eq!(surface.active_queue_load, active);
  }

  #[test]
  fn ready_queue_is_reused_by_exact_season_even_when_started_item_is_absent() {
    let (mut surface, kernel) = test_fixture();
    let session = kernel.request_gate.current_session();
    surface.queue = QueueState::Ready(vec![episode("episode-1", 1)]);
    surface.queue_session = Some(session);
    surface.queue_season = Some(QueueSeasonKey {
      series_id: "show-1".to_owned(),
      season_number: 1,
    });
    surface.queue_menu_open = true;

    let task = load_queue_after_start(
      &mut surface,
      &kernel,
      &Playable::Library(episode("episode-not-in-result", 1)),
    );

    assert_eq!(task.units(), 0);
    assert!(matches!(
      &surface.queue,
      QueueState::Ready(items) if items.len() == 1
    ));
    assert!(surface.queue_menu_open);
  }

  #[test]
  fn ready_queue_is_not_reused_for_another_exact_season_key() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let session = kernel.request_gate.current_session();
    surface.queue = QueueState::Ready(vec![episode("shared-id", 1)]);
    surface.queue_session = Some(session);
    surface.queue_season = Some(QueueSeasonKey {
      series_id: "another-show".to_owned(),
      season_number: 1,
    });
    let playable = Playable::Library(episode("shared-id", 1));
    surface.playable = Some(playable.clone());

    let task = load_queue_after_start(&mut surface, &kernel, &playable);

    assert_eq!(task.units(), 1);
    assert!(matches!(surface.queue, QueueState::Loading));
    assert_eq!(
      surface
        .queue_season
        .as_ref()
        .map(|season| season.series_id.as_str()),
      Some("show-1")
    );
  }

  #[test]
  fn failed_queue_retries_on_a_later_successful_episode_start() {
    let (mut surface, mut kernel) = test_fixture();
    let playable = Playable::Library(episode("episode-1", 1));
    surface.playable = Some(playable.clone());

    let unavailable_task = load_queue_after_start(&mut surface, &kernel, &playable);
    assert_eq!(unavailable_task.units(), 0);
    assert!(matches!(surface.queue, QueueState::Failed));

    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let retry_task = load_queue_after_start(&mut surface, &kernel, &playable);

    assert_eq!(retry_task.units(), 1);
    assert!(matches!(surface.queue, QueueState::Loading));
    assert!(surface.active_queue_load.is_some());
  }

  #[test]
  fn same_season_completion_applies_after_current_item_changes() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let first = Playable::Library(episode("episode-1", 1));
    surface.playable = Some(first.clone());
    drop(load_queue_after_start(&mut surface, &kernel, &first));
    let active = surface
      .active_queue_load
      .clone()
      .expect("queue request should be active");
    surface.playable = Some(Playable::Library(episode("episode-2", 1)));

    apply_queue_loaded(
      &mut surface,
      &kernel,
      active.session,
      active.generation,
      active.season.series_id,
      active.season.season_number,
      Ok(VideoSeasonEpisodes {
        series_id: "show-1".to_owned(),
        season_id: None,
        season_number: Some(1),
        episodes: vec![
          episode("episode-1", 1),
          episode("episode-2", 1),
          episode("episode-3", 1),
        ],
      }),
    );

    assert!(matches!(
      &surface.queue,
      QueueState::Ready(items)
        if items.iter().map(|item| item.id.as_str()).eq([
          "episode-1",
          "episode-2",
          "episode-3",
        ])
    ));
    assert!(surface.active_queue_load.is_none());
  }

  #[test]
  fn completion_from_prior_session_is_rejected_after_clear_and_relogin() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let playable = Playable::Library(episode("episode-1", 1));
    surface.playable = Some(playable.clone());
    drop(load_queue_after_start(&mut surface, &kernel, &playable));
    let stale = surface
      .active_queue_load
      .clone()
      .expect("queue request should be active");

    clear_queue(&mut surface);
    assert_ne!(surface.queue_generation, stale.generation);
    let new_session = kernel.request_gate.begin_login();
    assert_ne!(stale.session, new_session);
    apply_queue_loaded(
      &mut surface,
      &kernel,
      stale.session,
      stale.generation,
      stale.season.series_id,
      stale.season.season_number,
      Ok(VideoSeasonEpisodes {
        series_id: "show-1".to_owned(),
        season_id: None,
        season_number: Some(1),
        episodes: vec![episode("stale", 1)],
      }),
    );

    assert!(matches!(surface.queue, QueueState::Unavailable));
    assert!(surface.active_queue_load.is_none());
  }

  #[test]
  fn superseded_queue_completion_cannot_replace_new_season_load() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let season_one = Playable::Library(episode("season-1-episode", 1));
    surface.playable = Some(season_one.clone());
    drop(load_queue_after_start(&mut surface, &kernel, &season_one));
    let stale = surface
      .active_queue_load
      .clone()
      .expect("first queue request should be active");
    let season_two = Playable::Library(episode("season-2-episode", 2));
    surface.playable = Some(season_two.clone());
    drop(load_queue_after_start(&mut surface, &kernel, &season_two));
    let current = surface
      .active_queue_load
      .clone()
      .expect("second queue request should be active");

    apply_queue_loaded(
      &mut surface,
      &kernel,
      stale.session,
      stale.generation,
      stale.season.series_id,
      stale.season.season_number,
      Ok(VideoSeasonEpisodes {
        series_id: "show-1".to_owned(),
        season_id: None,
        season_number: Some(1),
        episodes: vec![episode("stale", 1)],
      }),
    );

    assert!(matches!(surface.queue, QueueState::Loading));
    assert_eq!(surface.active_queue_load, Some(current));
  }

  #[test]
  fn completion_is_rejected_when_current_playable_has_another_season() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let requested = Playable::Library(episode("episode-1", 1));
    surface.playable = Some(requested.clone());
    drop(load_queue_after_start(&mut surface, &kernel, &requested));
    let active = surface
      .active_queue_load
      .clone()
      .expect("queue request should be active");
    surface.playable = Some(Playable::Library(episode("episode-2", 2)));

    apply_queue_loaded(
      &mut surface,
      &kernel,
      active.session,
      active.generation,
      active.season.series_id,
      active.season.season_number,
      Ok(VideoSeasonEpisodes {
        series_id: "show-1".to_owned(),
        season_id: None,
        season_number: Some(1),
        episodes: vec![episode("wrong-season", 1)],
      }),
    );

    assert!(matches!(surface.queue, QueueState::Loading));
    assert!(surface.active_queue_load.is_some());
  }

  #[test]
  fn non_episode_start_clears_queue_and_menu() {
    let (mut surface, kernel) = test_fixture();
    surface.queue = QueueState::Ready(vec![episode("episode-1", 1)]);
    surface.queue_menu_open = true;
    let mut movie = episode("movie-1", 1);
    movie.item_type = "Movie".to_owned();
    movie.series_id = None;
    movie.season_number = None;

    drop(load_queue_after_start(
      &mut surface,
      &kernel,
      &Playable::Library(movie),
    ));

    assert!(matches!(surface.queue, QueueState::Unavailable));
    assert!(!surface.queue_menu_open);
  }

  #[test]
  fn queue_item_selection_builds_resume_start_with_default_tracks() {
    let item = episode("episode-2", 1);
    let intro = IntroAvailability {
      mode: IntroSkipMode::Manual,
      skipper_available: true,
    };

    let PlaybackIntent::Start {
      item: Playable::Library(selected),
      position,
      intro: selected_intro,
      selection,
    } = queue_item_start_intent(item, intro)
    else {
      panic!("queue row should build a library start intent");
    };

    assert_eq!(selected.id, "episode-2");
    assert_eq!(position, PlaybackStartPosition::Resume);
    assert_eq!(*selection, PlaybackSelection::default());
    assert_eq!(selected_intro.mode, IntroSkipMode::Manual);
    assert!(selected_intro.skipper_available);
  }

  #[test]
  fn seek_release_keeps_committed_preview_while_queued_behind_refresh() {
    let (mut surface, mut kernel) = active_playback_fixture();
    let now = Instant::now();
    let (refresh_id, command) = controller_effect(
      surface
        .session
        .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now)
        .effects,
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
        .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now)
        .effects,
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
        .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now)
        .effects,
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    surface.view = surface.session.view();
    assert!(!surface.view.busy);

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
    assert!(!surface.view.busy);
  }

  #[test]
  fn volume_change_during_refresh_keeps_the_draft_and_the_release_commits() {
    let (mut surface, mut kernel) = active_playback_fixture();
    let now = Instant::now();
    let (_refresh_id, command) = controller_effect(
      surface
        .session
        .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now)
        .effects,
    );
    assert!(matches!(command, ControllerCommand::Refresh));
    surface.view = surface.session.view();
    assert!(!surface.view.busy);

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
    assert!(!surface.view.busy);
  }

  #[test]
  fn inactive_playback_clears_artwork_previews_and_popover_state() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.audio_menu_open = true;
    surface.subtitle_menu_open = true;
    surface.seek_preview = Some(42.0);
    surface.volume_preview = Some(80.0);

    drop(clear_inactive_playback(&mut surface));

    assert!(!surface.audio_menu_open);
    assert!(!surface.subtitle_menu_open);
    assert_eq!(surface.seek_preview, None);
    assert_eq!(surface.volume_preview, None);
  }

  fn cached_player_fixture() -> (Surface, Kernel, String) {
    use jellypilot_media_server::{
      artwork::ArtworkRaster, image_id_for_url, ImageRefKind, MediaServerProvider, SavedSession,
    };

    let (mut surface, mut kernel) = test_fixture();
    let server_url = "https://images.example.com";
    let client = Arc::new(JellyfinClient::new());
    client.login().adopt_validated_session(&SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: server_url.to_owned(),
      access_token: "token".to_owned(),
      user_id: "user".to_owned(),
      user_name: "user".to_owned(),
      server_name: None,
      device_id: None,
    });
    let image_id = image_id_for_url(
      MediaServerProvider::Jellyfin,
      server_url,
      format!("{server_url}/Items/episode-1/Images/Primary"),
      ImageRefKind::Artwork,
    )
    .expect("valid image reference");
    kernel.artwork_adapter.seed_raster_for_test(
      &image_id,
      ArtworkSizeClass::Card,
      ArtworkRaster::from_raw_for_test(1, 1, vec![1, 2, 3, 255]),
    );
    kernel.client = Some(client);
    let mut item = episode("episode-1", 1);
    item.artwork_image_id = Some(image_id.clone());
    surface.playable = Some(Playable::Library(item));
    surface.view.now_playing = Some(NowPlayingView {
      item: playback_snapshot(10.0).now_playing.expect("active item"),
      paused: false,
      position_seconds: 10.0,
      duration_seconds: Some(1_800.0),
      volume: 75.0,
      muted: false,
    });
    drop(ensure_player_artwork(&mut surface, &mut kernel));
    (surface, kernel, image_id)
  }

  #[test]
  fn progress_updates_preserve_the_displayed_player_image() {
    let (mut surface, mut kernel, _) = cached_player_fixture();
    let handle = surface
      .artwork
      .get(PLAYER_IMAGE_KEY)
      .and_then(|cell| cell.handle())
      .expect("cached player image")
      .id();

    surface
      .view
      .now_playing
      .as_mut()
      .expect("active playback")
      .position_seconds = 11.0;
    drop(ensure_player_artwork(&mut surface, &mut kernel));

    assert_eq!(
      surface
        .artwork
        .get(PLAYER_IMAGE_KEY)
        .and_then(|cell| cell.handle())
        .map(|handle| handle.id()),
      Some(handle),
    );
  }

  #[test]
  fn suspending_playback_releases_its_image_without_clearing_browsing() {
    let (mut surface, mut kernel, image_id) = cached_player_fixture();
    let mut browsing = ImageCollection::default();
    drop(browsing.observe(
      kernel.request_gate.current_session(),
      ImageSpec {
        key: "browse-item".to_owned(),
        image_id,
        size_class: ArtworkSizeClass::Card,
        derived: DerivedArtwork::default(),
      },
      Some(ImagePriority::Visible),
      Arc::clone(kernel.client.as_ref().expect("authenticated client")),
      Arc::clone(&kernel.artwork_adapter),
      |completion| Message::Playback(PlaybackMessage::ArtworkLoaded(completion)),
    ));
    let handle = browsing
      .get("browse-item")
      .and_then(|cell| cell.handle())
      .expect("cached browsing image")
      .id();

    suspend_artwork(&mut surface);
    drop(ensure_player_artwork(&mut surface, &mut kernel));

    assert!(surface.artwork.is_empty());
    assert_eq!(
      browsing
        .get("browse-item")
        .and_then(|cell| cell.handle())
        .map(|handle| handle.id()),
      Some(handle),
    );

    drop(resume_artwork(&mut surface, &mut kernel));
    assert!(surface
      .artwork
      .get(PLAYER_IMAGE_KEY)
      .and_then(|cell| cell.handle())
      .is_some());
  }

  fn detail_with_series_poster(id: &str, image_id: &str) -> VideoItemDetail {
    VideoItemDetail {
      logo_image_id: None,
      media_info: None,
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
      original_language: None,
    }
  }

  #[test]
  fn clear_inactive_playback_preserves_artwork_while_a_start_is_in_flight() {
    let (mut surface, mut kernel) = test_fixture();
    let now = Instant::now();
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let start = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-2", 3)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      })),
      now,
    );
    let (_id, _) = controller_effect(start.effects);
    // The dispatched start keeps controller occupancy unsettled, which is
    // what retains presentation while Now Playing is transiently absent.
    surface.view = surface.session.view();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.playable = Some(Playable::Detail(detail_with_series_poster(
      "episode-1",
      "series-poster",
    )));
    drop(prepare_player_artwork(&mut surface, &mut kernel));

    drop(clear_inactive_playback(&mut surface));

    assert_eq!(
      surface
        .artwork
        .get(PLAYER_IMAGE_KEY)
        .map(|cell| cell.image_id.as_str()),
      Some("series-poster"),
    );
  }

  #[test]
  fn start_settlement_keeps_the_new_playable_when_now_playing_has_not_caught_up() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.playable = Some(Playable::Library(episode("episode-1", 1)));
    let now = Instant::now();
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let start = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: Playable::Media(media_item("episode-2")),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      })),
      now,
    );
    let (id, _) = controller_effect(start.effects);

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
        original_language: None,
      },
      paused: false,
      position_seconds: 0.0,
      duration_seconds: Some(1_800.0),
      volume: 75.0,
      muted: false,
    });
    drop(ensure_player_artwork(&mut surface, &mut kernel));
    assert_eq!(
      surface
        .artwork
        .get(PLAYER_IMAGE_KEY)
        .map(|cell| cell.image_id.as_str()),
      Some("series-poster")
    );
  }

  #[test]
  fn adjacent_settlement_upgrades_a_bare_media_playable_and_restores_artwork() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.playable = Some(Playable::Media(media_item("episode-2")));
    let remote = surface.remote.token();
    let play = kernel.request_gate.begin_remote_play();
    let now = Instant::now();
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let start = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-2", 3)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      })),
      now,
    );
    let (id, _) = controller_effect(start.effects);

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
      surface
        .artwork
        .get(PLAYER_IMAGE_KEY)
        .map(|cell| cell.image_id.as_str()),
      Some("series-poster")
    );
  }

  #[test]
  fn remote_track_selection_without_loaded_mapping_is_ignored_with_diagnostic() {
    let (mut surface, mut kernel) = test_fixture();
    surface.view.tracks = TracksView::Unavailable;
    let remote = surface.remote.token();

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
      kernel.notice.as_ref().map(UiText::id),
      Some(REMOTE_TRACKS_UNAVAILABLE_NOTICE)
    );
  }

  #[test]
  fn local_stop_invalidates_an_in_flight_remote_play_resolution() {
    let (mut surface, mut kernel) = test_fixture();
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      Instant::now(),
    );
    sync_playback_projection(&mut surface, &kernel, false);
    let stale_play = kernel.request_gate.begin_remote_play();
    drop(update_playback(
      &mut surface,
      &mut kernel,
      false,
      PlaybackMessage::Intent(Box::new(PlaybackIntent::Stop)),
    ));
    assert!(!kernel.request_gate.is_current_remote_play(stale_play));
    let remote = surface.remote.token();

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
        PlaybackMessage::Intent(Box::new(PlaybackIntent::PlayAdjacent(direction))),
      ));

      assert!(!kernel.request_gate.is_current_remote_play(stale_play));
    }
  }

  #[test]
  fn double_adjacent_press_dispatches_single_start() {
    let (mut surface, kernel) = test_fixture();
    let now = Instant::now();
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
      now,
    );
    let start = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
        item: Playable::Library(episode("episode-1", 1)),
        position: PlaybackStartPosition::Beginning,
        intro: IntroAvailability {
          mode: IntroSkipMode::Off,
          skipper_available: false,
        },
        selection: Box::default(),
      })),
      now,
    );
    let (id, _) = controller_effect(start.effects);
    let aux = surface
      .session
      .handle(
        PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
          id,
          settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
            snapshot: playback_snapshot(10.0),
            warnings: Vec::new(),
          })),
        })),
        now,
      )
      .effects;
    surface.view = surface.session.view();
    let next_id = aux
      .iter()
      .find_map(|effect| match effect {
        PlaybackEffect::LookupAdjacent(id, AdjacentDirection::Next) => Some(*id),
        _ => None,
      })
      .expect("expected next lookup effect");

    // Settle next adjacent item
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::AdjacentSettled {
        id: next_id,
        direction: AdjacentDirection::Next,
        result: Ok(Some(media_item("episode-2"))),
      })),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);

    // First adjacent press
    let first = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::PlayAdjacent(
        AdjacentDirection::Next,
      ))),
      now,
    );
    let (start_id, _) = controller_effect(first.effects);
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(surface.view.busy);

    // Second adjacent press while first is in flight (suppressed)
    let second = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::PlayAdjacent(
        AdjacentDirection::Next,
      ))),
      now,
    );
    assert!(second.effects.is_empty());

    // Settle the start
    let settle = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: start_id,
        settlement: ControllerSettlement::Started(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(0.0),
          warnings: Vec::new(),
        })),
      })),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);

    // No second start effect dispatched
    assert!(!settle
      .effects
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
    let first = surface
      .session
      .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Stop)), now);
    let (stop_id, _) = controller_effect(first.effects);
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(surface.view.busy);

    // Second stop while first is in flight
    let second = surface
      .session
      .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Stop)), now);
    assert!(second.effects.is_empty());

    // Settle the stop
    let settle = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: stop_id,
        settlement: ControllerSettlement::Stopped(Ok(
          jellypilot_mpv::playback::PlaybackStopOutcome {
            warnings: Vec::new(),
          },
        )),
      })),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);

    // Stop settled with no notice
    assert!(settle.effects.is_empty());
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

    let refresh = surface
      .session
      .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now);
    let (refresh_id, _) = controller_effect(refresh.effects);

    // Simulate EOF refresh settlement
    let settle = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: refresh_id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(10.0),
            state: PlaybackRefreshState::Ended(PlaybackEndReason::EndOfFile),
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      })),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);

    assert!(settle.effects.is_empty());
    assert!(surface.view.now_playing.is_none());
    assert!(surface.view.notice.is_none());
    assert!(surface.notice.is_none());
    assert!(kernel.active_toast.is_none());
  }

  #[test]
  fn unavailable_remote_target_does_not_dispatch_commands() {
    let (mut surface, mut kernel) = test_fixture();
    let pending = kernel.request_gate.begin_remote_play();
    let remote = surface.remote.token();
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
  fn connection_restored_clears_only_the_connection_lost_notice() {
    let (_surface, mut kernel) = test_fixture();
    kernel.notice = Some(UiText::new(REMOTE_CONNECTION_LOST_NOTICE));

    apply_remote_notice(&mut kernel, remote::Notice::ConnectionRestored);
    assert!(kernel.notice.is_none());

    kernel.notice = Some(UiText::new(REMOTE_TRACKS_UNAVAILABLE_NOTICE));
    apply_remote_notice(&mut kernel, remote::Notice::ConnectionRestored);
    assert_eq!(
      kernel.notice.as_ref().map(UiText::id),
      Some(REMOTE_TRACKS_UNAVAILABLE_NOTICE)
    );
  }

  #[test]
  fn quit_exit_stays_blocked_until_the_session_cleanup_handshake_settles() {
    let (mut surface, mut kernel) = test_fixture();

    assert!(!quit_may_exit(&surface, true));
    surface.view.quit_may_proceed = true;
    assert!(
      quit_may_exit(&surface, true),
      "a quiescent remote runtime does not block quit"
    );

    // A live remote target keeps the runtime non-quiescent until its retire
    // cleanup settles, so the quit handshake stays blocked.
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    drop(start_remote_session(&mut surface, &mut kernel));
    assert!(!surface.remote.view().quiescent);
    assert!(!quit_may_exit(&surface, true));
  }

  #[test]
  fn account_handoff_does_not_clear_a_quit_owned_remote_teardown() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    drop(start_remote_session(&mut surface, &mut kernel));
    drop(stop_remote_session_for_quit(&mut surface, &mut kernel));

    let start = begin_account_handoff(&mut surface, &mut kernel, true, 12);

    // The account waiter joins the quit-owned teardown inside the runtime;
    // it must not mark the runtime quiescent or release the quit gate early.
    assert!(!surface.remote.view().quiescent);
    assert!(!quit_may_exit(&surface, true));
    assert!(matches!(start.playback_cleanup, Some(Ok(()))));
  }

  #[test]
  fn playback_tick_and_settlement_do_not_project_busy_to_ui() {
    let (mut surface, mut kernel) = active_playback_fixture();
    assert!(!surface.view.busy);

    // Tick intent executes Refresh but must NOT project busy to UI
    let tick = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)),
      Instant::now(),
    );
    let (refresh_id, _) = controller_effect(tick.effects);
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(
      !surface.view.busy,
      "periodic refresh tick must not mark playback_view busy (prevents button flickering)"
    );

    // Refresh settlement keeps busy false
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
    assert!(
      !surface.view.busy,
      "refresh settlement must keep playback_view busy as false"
    );
  }

  #[test]
  fn playback_refresh_transition_to_queued_command_preserves_busy_state() {
    let (mut surface, kernel) = active_playback_fixture();
    let now = Instant::now();
    assert!(!surface.view.busy);

    // 1. Tick intent starts a Refresh
    let tick = surface
      .session
      .handle(PlaybackInput::Intent(Box::new(PlaybackIntent::Tick)), now);
    let (refresh_id, _) = controller_effect(tick.effects);
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(
      !surface.view.busy,
      "periodic refresh tick alone must not mark playback_view busy"
    );

    // 2. Queue a seek command while refresh is in flight; it is not busy yet
    let _ = surface.session.handle(
      PlaybackInput::Intent(Box::new(PlaybackIntent::Seek(50.0))),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(
      !surface.view.busy,
      "a control queued behind refresh must not mark playback_view busy"
    );

    // 3. Settle the in-flight refresh; the queued seek dispatches
    let settled = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: refresh_id,
        settlement: ControllerSettlement::Refreshed {
          outcome: PlaybackRefreshOutcome {
            snapshot: playback_snapshot(11.0),
            state: PlaybackRefreshState::Active,
            warnings: Vec::new(),
          },
          client_messages: Vec::new(),
        },
      })),
      now,
    );
    let (command_id, _) = controller_effect(settled.effects);
    sync_playback_projection(&mut surface, &kernel, false);
    assert!(
      surface.view.busy,
      "playback_view.busy must remain true while queued command is in flight"
    );

    // 4. Settle the command
    let _ = surface.session.handle(
      PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
        id: command_id,
        settlement: ControllerSettlement::Controlled(Ok(PlaybackOutcome {
          snapshot: playback_snapshot(50.0),
          warnings: Vec::new(),
        })),
      })),
      now,
    );
    sync_playback_projection(&mut surface, &kernel, false);
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
