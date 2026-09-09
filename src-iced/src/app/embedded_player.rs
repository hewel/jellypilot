//! Embedded-only presentation lifecycle and keyboard intent orchestration.

use std::time::{Duration, Instant};

use iced::{event, keyboard, Event, Subscription, Task};
use jellypilot_core::config::AppMode;
use jellypilot_mpv::playback_session::{
  seek_intent, volume_intent, ControllerSettlement, PlaybackIntent,
};

use super::message::{Message as AppMessage, PlaybackMessage, ShellMessage};
use super::state::{Destination, State};

const IDLE: Duration = Duration::from_secs(3);
const FEEDBACK: Duration = Duration::from_millis(1200);

#[derive(Clone, Debug)]
pub enum Message {
  PointerMoved {
    position: iced::Point,
    bounds: iced::Size,
    controls_height: f32,
  },
  Back,
  SeekBy(f64),
  VolumeBy(f64),
  SeekHovered(Option<f64>),
  Wake(Instant),
}

#[derive(Default)]
pub struct Surface {
  visible: bool,
  idle_deadline: Option<Instant>,
  back_visible: bool,
  back_deadline: Option<Instant>,
  cursor_visible: bool,
  cursor_deadline: Option<Instant>,
  feedback: Option<(String, Instant)>,
  seek_hover: Option<f64>,
  desired_seek: Option<f64>,
  desired_volume: Option<f64>,
  returning: bool,
  replacement_generation: u64,
  was_active: bool,
}

pub(super) fn active(state: &State) -> bool {
  crate::embedded::enabled()
    && state.shell.images_visible
    && state.shell.window_id.is_some()
    && state.playback.view.now_playing.is_some()
    && (state.shell.destination == Destination::NowPlaying
      || state.shell.player_fullscreen
      || state.app_mode() == AppMode::ControlOnly)
}

fn menu_open(state: &State) -> bool {
  state.playback.audio_menu_open
    || state.playback.subtitle_menu_open
    || state.playback.queue_menu_open
}

pub(super) fn input_blocked(state: &State) -> bool {
  state.shell.settings_open
    || state.shell.account_popover_open
    || state.shell.compact_search_open
    || state.settings.view.shortcut_capture.is_some()
    || state.playback.replacing
    || state.shell.embedded_player.returning
    || super::accounts::blocking_modal(&state.accounts)
    || super::accounts::handoff_generation(&state.accounts).is_some()
    || menu_open(state)
}

pub(super) fn controls_visible(state: &State) -> bool {
  !active(state) || state.shell.embedded_player.visible || held(state)
}

pub(super) fn back_visible(state: &State) -> bool {
  !active(state) || state.shell.embedded_player.back_visible || held(state)
}

pub(super) fn cursor_visible(state: &State) -> bool {
  !active(state) || state.shell.embedded_player.cursor_visible || held(state)
}

pub(super) fn feedback(state: &State) -> Option<String> {
  active(state)
    .then(|| {
      state
        .shell
        .embedded_player
        .feedback
        .as_ref()
        .map(|(text, _)| text.clone())
    })
    .flatten()
}

pub(super) fn seek_hover(state: &State) -> Option<f64> {
  state.shell.embedded_player.seek_hover
}

pub(super) fn is_seek_dragging(state: &State) -> bool {
  state.playback.seek_dragging
}

pub(super) fn is_volume_dragging(state: &State) -> bool {
  state.playback.volume_dragging
}

fn held(state: &State) -> bool {
  state
    .playback
    .view
    .now_playing
    .as_ref()
    .is_some_and(|playing| playing.paused)
    || state.playback.seek_dragging
    || state.playback.volume_dragging
    || state.playback.view.intro_prompt.is_some()
    || input_blocked(state)
    || state.shell.embedded_player.returning
}

pub(super) fn reconcile(state: &mut State) {
  if state.shell.embedded_player.replacement_generation != state.playback.replacement_generation {
    let surface = &mut state.shell.embedded_player;
    surface.replacement_generation = state.playback.replacement_generation;
    surface.desired_seek = None;
    surface.desired_volume = None;
    surface.returning = false;
    surface.feedback = None;
  }
  let active = active(state);
  if !active {
    state.playback.seek_dragging = false;
    state.playback.volume_dragging = false;
  }
  let held = held(state);
  let settled = !state.playback.view.busy
    && state.playback.in_flight_command.is_none()
    && state.playback.in_flight_refresh.is_none();
  reconcile_surface(
    &mut state.shell.embedded_player,
    active,
    held,
    settled,
    Instant::now(),
  );
}

fn reconcile_surface(surface: &mut Surface, active: bool, held: bool, settled: bool, now: Instant) {
  if !active {
    // Drop all transient presentation on exit; the view owns the scoped cursor.
    *surface = Surface::default();
    return;
  }
  if !surface.was_active {
    surface.visible = true;
    surface.back_visible = true;
    surface.cursor_visible = true;
  }
  surface.was_active = true;
  if held {
    surface.visible = true;
    surface.idle_deadline = None;
    surface.back_visible = true;
    surface.back_deadline = None;
    surface.cursor_visible = true;
    surface.cursor_deadline = None;
  } else if surface.visible && surface.idle_deadline.is_none() {
    surface.idle_deadline = Some(now + IDLE);
  }
  if !held && surface.back_visible && surface.back_deadline.is_none() {
    surface.back_deadline = Some(now + IDLE);
  }
  if !held && surface.cursor_visible && surface.cursor_deadline.is_none() {
    surface.cursor_deadline = Some(now + IDLE);
  }
  if settled {
    surface.desired_seek = None;
    surface.desired_volume = None;
  }
}

pub(super) fn subscription(state: &State) -> Subscription<AppMessage> {
  if !active(state) {
    return Subscription::none();
  }
  let surface = &state.shell.embedded_player;
  let deadline = surface
    .idle_deadline
    .into_iter()
    .chain(surface.back_deadline)
    .chain(surface.cursor_deadline)
    .chain(surface.feedback.as_ref().map(|(_, deadline)| *deadline))
    .min();
  deadline.map_or_else(Subscription::none, |deadline| {
    Subscription::run_with(deadline, wake_stream)
  })
}

pub(super) fn reserved_key(event: &Event) -> bool {
  let Event::Keyboard(keyboard::Event::KeyPressed {
    modified_key,
    modifiers,
    ..
  }) = event
  else {
    return false;
  };
  modifiers.is_empty()
    && matches!(
      modified_key.as_ref(),
      keyboard::Key::Named(
        keyboard::key::Named::ArrowLeft
          | keyboard::key::Named::ArrowRight
          | keyboard::key::Named::ArrowUp
          | keyboard::key::Named::ArrowDown
          | keyboard::key::Named::Space
          | keyboard::key::Named::Escape
      ) | keyboard::Key::Character("f" | "F")
    )
}

fn wake_stream(deadline: &Instant) -> impl iced::futures::Stream<Item = AppMessage> {
  let deadline = *deadline;
  iced::futures::stream::once(async move {
    tokio::time::sleep_until(deadline.into()).await;
    AppMessage::EmbeddedPlayer(Message::Wake(deadline))
  })
}

pub(super) fn keyboard(event: Event, status: event::Status) -> Option<AppMessage> {
  if status == event::Status::Captured {
    return None;
  }
  let Event::Keyboard(keyboard::Event::KeyPressed {
    modified_key,
    modifiers,
    repeat,
    ..
  }) = event
  else {
    return None;
  };
  if !modifiers.is_empty() {
    return None;
  }
  use keyboard::key::Named;
  let message = match modified_key.as_ref() {
    keyboard::Key::Named(Named::ArrowLeft) => Message::SeekBy(-5.0),
    keyboard::Key::Named(Named::ArrowRight) => Message::SeekBy(5.0),
    keyboard::Key::Named(Named::ArrowUp) => Message::VolumeBy(5.0),
    keyboard::Key::Named(Named::ArrowDown) => Message::VolumeBy(-5.0),
    keyboard::Key::Character("f" | "F") if !repeat => {
      return Some(AppMessage::Playback(PlaybackMessage::Intent(Box::new(
        PlaybackIntent::ToggleFullscreen,
      ))))
    }
    keyboard::Key::Named(Named::Space) if !repeat => {
      return Some(AppMessage::Playback(PlaybackMessage::Intent(Box::new(
        PlaybackIntent::TogglePaused,
      ))))
    }
    keyboard::Key::Named(Named::Escape) if !repeat => {
      return Some(AppMessage::Shell(ShellMessage::ExitPlayerFullscreen))
    }
    _ => return None,
  };
  Some(AppMessage::EmbeddedPlayer(message))
}

pub(super) fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  if !active(state) {
    return Task::none();
  }
  let now = Instant::now();
  match message {
    Message::PointerMoved {
      position,
      bounds,
      controls_height,
    } => {
      pointer_moved(
        &mut state.shell.embedded_player,
        position,
        bounds,
        controls_height,
        now,
      );
    }
    Message::SeekHovered(position) => {
      state.shell.embedded_player.seek_hover = position.filter(|v| v.is_finite());
      if position.is_some() {
        state.shell.embedded_player.visible = true;
        state.shell.embedded_player.idle_deadline = Some(now + IDLE);
      }
    }
    Message::Wake(deadline) => {
      let held = held(state);
      expire(&mut state.shell.embedded_player, deadline, now, held);
    }
    Message::Back => {
      if input_blocked(state) || state.shell.embedded_player.returning {
        return Task::none();
      }
      state.shell.embedded_player.returning = true;
      state.shell.embedded_player.desired_seek = None;
      state.shell.embedded_player.desired_volume = None;
      let task = dispatch(state, PlaybackIntent::Stop);
      state.shell.embedded_player.replacement_generation = state.playback.replacement_generation;
      return task;
    }
    Message::SeekBy(delta) | Message::VolumeBy(delta) => {
      if input_blocked(state) {
        return Task::none();
      }
      let Some(playing) = state.playback.view.now_playing.as_ref() else {
        return Task::none();
      };
      let surface = &mut state.shell.embedded_player;
      let Some(intent) = adjustment(
        surface,
        playing,
        matches!(message, Message::SeekBy(_)),
        delta,
        now,
      ) else {
        return Task::none();
      };
      return dispatch(state, intent);
    }
  }
  Task::none()
}

fn pointer_moved(
  surface: &mut Surface,
  position: iced::Point,
  bounds: iced::Size,
  controls_height: f32,
  now: Instant,
) {
  if !iced::Rectangle::with_size(bounds).contains(position) {
    return;
  }
  surface.cursor_visible = true;
  surface.cursor_deadline = Some(now + IDLE);
  if position.y >= (bounds.height - controls_height).max(0.0) {
    surface.visible = true;
    surface.idle_deadline = Some(now + IDLE);
  }
  // The return target is independent of the bottom transport reveal zone.
  if position.x <= 112.0 && position.y <= 100.0 {
    surface.back_visible = true;
    surface.back_deadline = Some(now + IDLE);
  }
}

fn dispatch(state: &mut State, intent: PlaybackIntent) -> Task<AppMessage> {
  super::playback::update(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
    PlaybackMessage::Intent(Box::new(intent)),
  )
}

/// Observe the actual accepted controller settlement before the router consumes it.
pub(super) fn before_playback(state: &mut State, message: &PlaybackMessage) -> bool {
  if !crate::embedded::enabled() {
    return false;
  }
  let surface = &mut state.shell.embedded_player;
  match message {
    PlaybackMessage::SeekReleased => {
      if let Some(position) = state.playback.seek_preview {
        surface.desired_seek = Some(position);
      }
    }
    PlaybackMessage::VolumeReleased => {
      if let Some(volume) = state.playback.volume_preview {
        surface.desired_volume = Some(volume);
      }
    }
    PlaybackMessage::SeekAdjusted(position) => {
      if let Some(PlaybackIntent::Seek(position)) = seek_intent(
        *position,
        state
          .playback
          .view
          .now_playing
          .as_ref()
          .and_then(|view| view.duration_seconds),
        state.playback.view.now_playing.is_some(),
      ) {
        surface.desired_seek = Some(position);
      }
    }
    PlaybackMessage::VolumeAdjusted(volume) => {
      if let Some(PlaybackIntent::SetVolume(volume)) =
        volume_intent(*volume, state.playback.view.now_playing.is_some())
      {
        surface.desired_volume = Some(volume);
      }
    }
    PlaybackMessage::ControllerSettled { id, settlement, .. } => {
      return stop_return(
        surface,
        state.playback.in_flight_command == Some(*id),
        settlement,
      );
    }
    _ => {}
  }
  false
}

fn stop_return(surface: &mut Surface, accepted: bool, settlement: &ControllerSettlement) -> bool {
  if accepted {
    if let ControllerSettlement::Stopped(result) = settlement {
      return std::mem::take(&mut surface.returning) && result.is_ok();
    }
  }
  false
}

pub(super) fn return_to_source(state: &mut State) -> Task<AppMessage> {
  let fullscreen = super::shell::exit_player_fullscreen(state);
  if state.full.is_some() && state.shell.destination == Destination::NowPlaying {
    Task::batch([fullscreen, super::shell::navigate_back(state)])
  } else {
    fullscreen
  }
}

fn expire(surface: &mut Surface, deadline: Instant, now: Instant, held: bool) {
  if surface.idle_deadline == Some(deadline) && now >= deadline && !held {
    surface.visible = false;
    surface.idle_deadline = None;
    surface.seek_hover = None;
  }
  if surface.back_deadline == Some(deadline) && now >= deadline && !held {
    surface.back_visible = false;
    surface.back_deadline = None;
  }
  if surface.cursor_deadline == Some(deadline) && now >= deadline && !held {
    surface.cursor_visible = false;
    surface.cursor_deadline = None;
  }
  if surface
    .feedback
    .as_ref()
    .is_some_and(|(_, expires)| *expires <= now)
  {
    surface.feedback = None;
  }
}

fn adjustment(
  surface: &mut Surface,
  playing: &jellypilot_mpv::playback_session::NowPlayingView,
  seek: bool,
  delta: f64,
  now: Instant,
) -> Option<PlaybackIntent> {
  let intent = if seek {
    seek_intent(
      surface.desired_seek.unwrap_or(playing.position_seconds) + delta,
      playing.duration_seconds,
      true,
    )
  } else {
    volume_intent(
      surface.desired_volume.unwrap_or(playing.volume) + delta,
      true,
    )
  }?;
  let text = match intent {
    PlaybackIntent::Seek(position) => {
      surface.desired_seek = Some(position);
      format!(
        "{:+.0} s · {}:{:02}",
        delta,
        position as u64 / 60,
        position as u64 % 60
      )
    }
    PlaybackIntent::SetVolume(volume) => {
      surface.desired_volume = Some(volume);
      format!("{volume:.0}%")
    }
    _ => return None,
  };
  surface.feedback = Some((text, now + FEEDBACK));
  Some(intent)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn key(key: keyboard::Key, repeat: bool) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
      modified_key: key.clone(),
      key,
      physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyA),
      location: keyboard::Location::Standard,
      modifiers: keyboard::Modifiers::NONE,
      text: None,
      repeat,
    })
  }

  #[test]
  fn captured_arrows_do_not_seek_but_uncaptured_repeats_do() {
    let right = key(keyboard::Key::Named(keyboard::key::Named::ArrowRight), true);
    assert!(keyboard(right.clone(), event::Status::Captured).is_none());
    assert!(matches!(
      keyboard(right, event::Status::Ignored),
      Some(AppMessage::EmbeddedPlayer(Message::SeekBy(5.0)))
    ));
    let fullscreen = key(keyboard::Key::Character("f".into()), false);
    assert!(matches!(keyboard(fullscreen, event::Status::Ignored),
      Some(AppMessage::Playback(PlaybackMessage::Intent(intent))) if matches!(*intent, PlaybackIntent::ToggleFullscreen)));
    assert!(keyboard(
      key(keyboard::Key::Character("f".into()), true),
      event::Status::Ignored
    )
    .is_none());
    assert!(keyboard(
      key(keyboard::Key::Named(keyboard::key::Named::Space), true),
      event::Status::Ignored
    )
    .is_none());
  }

  #[test]
  fn pointer_reveals_only_the_nearby_control_region() {
    let now = Instant::now();
    let bounds = iced::Size::new(1100.0, 900.0);
    let mut surface = Surface::default();
    pointer_moved(
      &mut surface,
      iced::Point::new(550.0, 450.0),
      bounds,
      202.0,
      now,
    );
    assert!(!surface.visible && !surface.back_visible);
    pointer_moved(
      &mut surface,
      iced::Point::new(550.0, 710.0),
      bounds,
      202.0,
      now,
    );
    assert!(surface.visible && !surface.back_visible);
    let deadline = surface.idle_deadline.unwrap();
    // Moving over the picture does not keep the transport alive.
    pointer_moved(
      &mut surface,
      iced::Point::new(550.0, 450.0),
      bounds,
      202.0,
      now + IDLE,
    );
    expire(&mut surface, deadline, now + IDLE, false);
    assert!(!surface.visible);
    pointer_moved(
      &mut surface,
      iced::Point::new(60.0, 40.0),
      bounds,
      202.0,
      now + IDLE,
    );
    assert!(surface.back_visible && !surface.visible);
    expire(&mut surface, now + IDLE + IDLE, now + IDLE + IDLE, false);
    assert!(!surface.back_visible);
    // Responsive controls and fullscreen use their actual measured surface, not the saved window.
    pointer_moved(
      &mut surface,
      iced::Point::new(200.0, 510.0),
      iced::Size::new(400.0, 800.0),
      320.0,
      now,
    );
    assert!(surface.visible);
  }

  #[test]
  fn picture_motion_reveals_cursor_without_revealing_controls() {
    let now = Instant::now();
    let mut surface = Surface::default();
    let bounds = iced::Size::new(1920.0, 1080.0);
    let center = iced::Point::new(960.0, 540.0);
    pointer_moved(&mut surface, center, bounds, 202.0, now);
    assert!(surface.cursor_visible);
    assert!(!surface.visible && !surface.back_visible);
    let old_deadline = surface.cursor_deadline.unwrap();
    pointer_moved(
      &mut surface,
      center,
      bounds,
      202.0,
      now + Duration::from_secs(2),
    );
    expire(&mut surface, old_deadline, old_deadline, false);
    assert!(
      surface.cursor_visible,
      "an old wake must not hide a recently moved cursor"
    );
    let deadline = surface.cursor_deadline.unwrap();
    expire(&mut surface, deadline, deadline, false);
    assert!(!surface.cursor_visible);
    assert!(!surface.visible && !surface.back_visible);
    pointer_moved(&mut surface, center, bounds, 202.0, deadline);
    assert!(
      surface.cursor_visible,
      "motion restores the cursor after timeout"
    );
    reconcile_surface(&mut surface, false, false, true, deadline);
    assert!(!surface.cursor_visible);
    assert!(surface.cursor_deadline.is_none());
  }

  #[test]
  fn idle_holds_cancel_old_deadlines_and_exit_clears_feedback() {
    let now = Instant::now();
    let mut surface = Surface::default();
    reconcile_surface(&mut surface, true, false, true, now);
    let old = surface.idle_deadline.unwrap();
    // A menu, pause or drag arriving before the wake must keep controls visible.
    reconcile_surface(&mut surface, true, true, true, now + IDLE);
    expire(&mut surface, old, now + IDLE, true);
    assert!(surface.visible);
    assert!(surface.idle_deadline.is_none());
    reconcile_surface(&mut surface, true, false, true, now + IDLE);
    expire(&mut surface, old, now + IDLE, false);
    assert!(surface.visible);
    expire(&mut surface, now + IDLE + IDLE, now + IDLE + IDLE, false);
    assert!(!surface.visible);
    surface.feedback = Some(("75%".into(), now + IDLE + IDLE + FEEDBACK));
    reconcile_surface(&mut surface, true, false, false, now + IDLE + IDLE);
    assert!(
      !surface.visible,
      "keyboard feedback must not reveal hidden controls"
    );
    reconcile_surface(&mut surface, false, false, false, now + IDLE + IDLE);
    assert!(surface.feedback.is_none());
    assert!(surface.idle_deadline.is_none());
  }

  #[test]
  fn repeated_adjustments_accumulate_until_all_async_snapshots_settle() {
    let playing = jellypilot_mpv::playback_session::NowPlayingView {
      item: jellypilot_mpv::playback::NowPlayingItem {
        item_id: "movie".into(),
        title: "Movie".into(),
        item_type: "Movie".into(),
        runtime_seconds: Some(60.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".into(),
      },
      paused: false,
      position_seconds: 10.0,
      duration_seconds: Some(60.0),
      volume: 90.0,
      muted: false,
    };
    let now = Instant::now();
    let mut surface = Surface::default();
    assert!(matches!(
      adjustment(&mut surface, &playing, true, 5.0, now),
      Some(PlaybackIntent::Seek(15.0))
    ));
    // A refresh or earlier control result still contains the old transport.
    reconcile_surface(&mut surface, true, false, false, now);
    assert!(matches!(
      adjustment(&mut surface, &playing, true, 5.0, now),
      Some(PlaybackIntent::Seek(20.0))
    ));
    for _ in 0..20 {
      adjustment(&mut surface, &playing, true, 5.0, now);
    }
    assert_eq!(surface.desired_seek, Some(60.0));
    adjustment(&mut surface, &playing, false, 5.0, now);
    assert!(matches!(
      adjustment(&mut surface, &playing, false, 5.0, now),
      Some(PlaybackIntent::SetVolume(100.0))
    ));
    assert!(matches!(
      adjustment(&mut surface, &playing, false, 5.0, now),
      Some(PlaybackIntent::SetVolume(100.0))
    ));
    reconcile_surface(&mut surface, true, false, true, now);
    assert!(matches!(
      adjustment(&mut surface, &playing, true, -5.0, now),
      Some(PlaybackIntent::Seek(5.0))
    ));
    let unknown = jellypilot_mpv::playback_session::NowPlayingView {
      duration_seconds: None,
      ..playing
    };
    assert!(adjustment(&mut surface, &unknown, true, 5.0, now).is_none());
  }
}

#[cfg(test)]
mod return_tests {
  use super::*;

  #[test]
  fn back_waits_for_accepted_stop_success_and_failed_stop_can_be_retried() {
    let success =
      ControllerSettlement::Stopped(Ok(jellypilot_mpv::playback::PlaybackStopOutcome {
        warnings: Vec::new(),
      }));
    let failure = ControllerSettlement::Stopped(Err(
      jellypilot_mpv::playback::PlaybackError::MpvControlFailed,
    ));
    let mut surface = Surface {
      returning: true,
      ..Surface::default()
    };
    assert!(
      !stop_return(&mut surface, false, &success),
      "stale stop must not navigate"
    );
    assert!(surface.returning);
    assert!(
      !stop_return(&mut surface, true, &failure),
      "failed stop must retain the player"
    );
    assert!(!surface.returning, "failure permits an explicit retry");
    surface.returning = true;
    assert!(stop_return(&mut surface, true, &success));
    assert!(
      !stop_return(&mut surface, true, &success),
      "duplicate completion must not pop another page"
    );
  }
}
