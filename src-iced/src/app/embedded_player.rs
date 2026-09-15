//! Embedded-only presentation lifecycle and keyboard intent orchestration.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::futures::SinkExt;
use iced::{event, keyboard, Event, Subscription, Task};
use jellypilot_core::config::AppMode;
use jellypilot_mpv::playback_session::{
  seek_intent, volume_intent, ControllerAcceptance, PlaybackIntent, StopCompletion,
};
use jellypilot_mpv::statistics::PlaybackStatistics;

use super::message::{Message as AppMessage, PlaybackMessage, ShellMessage};
use super::state::{Destination, State};

const IDLE: Duration = Duration::from_secs(3);
const FEEDBACK: Duration = Duration::from_millis(1200);

#[derive(Clone)]
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
  InformationToggled,
  InformationDismissed,
  InformationSampled {
    token: Instant,
    sample: Option<Box<PlaybackStatistics>>,
  },
  BufferSampled {
    token: Instant,
    ranges: Vec<(f64, f64)>,
  },
  QueueArtworkLoaded(super::artwork::ImageCompletion),
  QueueScrolled {
    epoch: u64,
    item_count: usize,
    top: bool,
    bottom: bool,
  },
}

impl std::fmt::Debug for Message {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(match self {
      Self::PointerMoved { .. } => "PointerMoved",
      Self::Back => "Back",
      Self::SeekBy(_) => "SeekBy",
      Self::VolumeBy(_) => "VolumeBy",
      Self::SeekHovered(_) => "SeekHovered",
      Self::Wake(_) => "Wake",
      Self::InformationToggled => "InformationToggled",
      Self::InformationDismissed => "InformationDismissed",
      Self::InformationSampled { .. } => "InformationSampled([redacted])",
      Self::BufferSampled { .. } => "BufferSampled",
      Self::QueueArtworkLoaded(_) => "QueueArtworkLoaded([redacted])",
      Self::QueueScrolled { .. } => "QueueScrolled",
    })
  }
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
  information_open: bool,
  information: Option<Box<PlaybackStatistics>>,
  information_failed: bool,
  buffered_ranges: Vec<(f64, f64)>,
  observation: Option<Observation>,
  queue_artwork: super::artwork::ImageCollection,
  queue_observed: bool,
  queue_scroll_edges: (bool, bool),
  queue_item_count: usize,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Observation {
  token: Instant,
  generation: u64,
  information: bool,
}

#[derive(Clone)]
struct ObservationSubscription {
  demand: Observation,
  controller: super::state::PlaybackControllerHandle,
}

impl Hash for ObservationSubscription {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.demand.hash(state);
    Arc::as_ptr(&self.controller).hash(state);
  }
}

pub(super) fn queue_artwork(state: &State) -> &super::artwork::ImageCollection {
  &state.shell.embedded_player.queue_artwork
}

pub(super) fn queue_scroll_edges(state: &State) -> (bool, bool) {
  state.shell.embedded_player.queue_scroll_edges
}

pub(super) fn observe_queue_image(
  state: &mut State,
  epoch: u64,
  spec: super::artwork::ImageSpec,
  priority: Option<super::artwork::ImagePriority>,
) -> Task<AppMessage> {
  if !active(state) || !state.shell.embedded_player.queue_observed {
    return Task::none();
  }
  let collection = &mut state.shell.embedded_player.queue_artwork;
  if epoch != collection.epoch() {
    return Task::none();
  }
  let Some(client) = state.kernel.client.as_ref() else {
    return Task::none();
  };
  collection.observe(
    state.kernel.request_gate.current_session(),
    spec,
    priority,
    Arc::clone(client),
    Arc::clone(&state.kernel.artwork_adapter),
    |completion| AppMessage::EmbeddedPlayer(Message::QueueArtworkLoaded(completion)),
  )
}

fn settle_information(
  surface: &mut Surface,
  token: Instant,
  sample: Option<Box<PlaybackStatistics>>,
) {
  if surface.information_open
    && surface
      .observation
      .is_some_and(|demand| demand.token == token && demand.information)
  {
    surface.buffered_ranges.clear();
    surface.information_failed = sample.is_none();
    surface.information = sample;
  }
}

pub(super) fn information_open(state: &State) -> bool {
  state.shell.embedded_player.information_open
}

pub(super) fn information(state: &State) -> Option<&PlaybackStatistics> {
  state.shell.embedded_player.information.as_deref()
}

pub(super) fn information_failed(state: &State) -> bool {
  state.shell.embedded_player.information_failed
}

pub(super) fn buffered_ranges(state: &State) -> &[(f64, f64)] {
  let surface = &state.shell.embedded_player;
  surface
    .information
    .as_ref()
    .map_or(surface.buffered_ranges.as_slice(), |information| {
      information.buffered_ranges.as_slice()
    })
}

pub(super) fn active(state: &State) -> bool {
  crate::embedded::enabled()
    && state.shell.images_visible
    && state.shell.window_id.is_some()
    && state.playback.view.lifecycle.playback_active
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
    || state.playback.view.lifecycle.replacing
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
    || information_open(state)
}

pub(super) fn reconcile(state: &mut State) {
  observe_replacement(
    &mut state.shell.embedded_player,
    state.playback.view.lifecycle.replacement_generation,
  );
  let active = active(state);
  if !active {
    state.playback.seek_dragging = false;
    state.playback.volume_dragging = false;
  }
  let held = held(state);
  let settled = state.playback.view.lifecycle.settled;
  reconcile_surface(
    &mut state.shell.embedded_player,
    active,
    held,
    settled,
    Instant::now(),
  );
  let blocked = state.shell.settings_open
    || state.shell.account_popover_open
    || super::accounts::blocking_modal(&state.accounts)
    || state.shell.quit_requested;
  if blocked || menu_open(state) {
    state.shell.embedded_player.information_open = false;
    state.shell.embedded_player.information = None;
  }
  let visible = active && controls_visible(state) && !blocked;
  let generation = state.playback.view.lifecycle.replacement_generation;
  let replacing = state.playback.view.lifecycle.replacing;
  reconcile_observation(
    &mut state.shell.embedded_player,
    visible && !replacing,
    generation,
    Instant::now(),
  );
  let surface = &mut state.shell.embedded_player;
  let queue_open = active && state.playback.queue_menu_open;
  let item_count = match &state.playback.queue {
    super::playback::QueueState::Ready(items) => items.len(),
    _ => 0,
  };
  reconcile_queue(surface, queue_open, queue_open && !blocked, item_count);
  state
    .image_diagnostics
    .record(surface.queue_artwork.take_summary());
}

fn reconcile_queue(surface: &mut Surface, open: bool, observed: bool, item_count: usize) {
  if surface.queue_observed && !observed {
    surface.queue_artwork.clear();
  }
  if !open || surface.queue_item_count != item_count {
    // Fitting lists emit no on_scroll. Retain geometry across artwork refreshes,
    // but never carry overflow indicators into a new list or a reopened popup.
    surface.queue_scroll_edges = (false, false);
  }
  surface.queue_observed = observed;
  surface.queue_item_count = item_count;
}

fn observe_replacement(surface: &mut Surface, generation: u64) {
  if surface.replacement_generation != generation {
    surface.replacement_generation = generation;
    surface.desired_seek = None;
    surface.desired_volume = None;
    surface.returning = false;
    surface.feedback = None;
    surface.information = None;
    surface.information_failed = false;
    surface.buffered_ranges.clear();
    surface.observation = None;
    surface.queue_artwork.clear();
  }
}

fn reconcile_surface(surface: &mut Surface, active: bool, held: bool, settled: bool, now: Instant) {
  if !active {
    // Drop all transient presentation on exit; the view owns the scoped cursor.
    if surface.was_active {
      *surface = Surface::default();
    }
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
  let wake = deadline.map_or_else(Subscription::none, |deadline| {
    Subscription::run_with(deadline, wake_stream)
  });
  let observation = match (surface.observation, state.playback.controller.as_ref()) {
    (Some(demand), Some(controller)) => Subscription::run_with(
      ObservationSubscription {
        demand,
        controller: Arc::clone(controller),
      },
      observation_stream,
    ),
    _ => Subscription::none(),
  };
  Subscription::batch([wake, observation])
}

fn reconcile_observation(surface: &mut Surface, visible: bool, generation: u64, now: Instant) {
  if !visible {
    surface.observation = None;
    surface.buffered_ranges.clear();
    return;
  }
  if !surface.observation.is_some_and(|demand| {
    demand.generation == generation && demand.information == surface.information_open
  }) {
    surface.observation = Some(Observation {
      token: now,
      generation,
      information: surface.information_open,
    });
  }
}

fn observation_stream(
  subscription: &ObservationSubscription,
) -> impl iced::futures::Stream<Item = AppMessage> {
  let subscription = subscription.clone();
  iced::stream::channel(1, async move |mut output| {
    loop {
      // Only copy the read handle under the controller lock. Property sampling
      // must never hold up a seek, track change, stop, or account handoff.
      let reader = subscription.controller.lock().await.statistics_reader();
      let token = subscription.demand.token;
      let message = if subscription.demand.information {
        let sample = match reader {
          Some(reader) => reader.sample().await.ok().map(Box::new),
          None => None,
        };
        Message::InformationSampled { token, sample }
      } else {
        let ranges = match reader {
          Some(reader) => reader.buffered_ranges().await.unwrap_or_default(),
          None => Vec::new(),
        };
        Message::BufferSampled { token, ranges }
      };
      if output
        .send(AppMessage::EmbeddedPlayer(message))
        .await
        .is_err()
      {
        break;
      }
      tokio::time::sleep(Duration::from_secs(1)).await;
    }
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
    Message::QueueScrolled {
      epoch,
      item_count,
      top,
      bottom,
    } => {
      let surface = &mut state.shell.embedded_player;
      if surface.queue_observed
        && surface.queue_artwork.epoch() == epoch
        && surface.queue_item_count == item_count
      {
        surface.queue_scroll_edges = (top, bottom);
      }
    }
    Message::QueueArtworkLoaded(completion) => {
      state
        .shell
        .embedded_player
        .queue_artwork
        .settle(state.kernel.request_gate.current_session(), completion);
    }
    Message::InformationToggled => {
      if state.playback.view.lifecycle.replacing || state.shell.quit_requested {
        return Task::none();
      }
      let surface = &mut state.shell.embedded_player;
      surface.information_open = !surface.information_open;
      surface.information = None;
      surface.information_failed = false;
      surface.observation = None;
      state.playback.audio_menu_open = false;
      state.playback.subtitle_menu_open = false;
      state.playback.queue_menu_open = false;
    }
    Message::InformationDismissed => {
      let surface = &mut state.shell.embedded_player;
      surface.information_open = false;
      surface.information = None;
      surface.information_failed = false;
      surface.observation = None;
    }
    Message::InformationSampled { token, sample } => {
      let surface = &mut state.shell.embedded_player;
      settle_information(surface, token, sample);
    }
    Message::BufferSampled { token, ranges } => {
      let surface = &mut state.shell.embedded_player;
      if surface
        .observation
        .is_some_and(|demand| demand.token == token && !demand.information)
      {
        surface.buffered_ranges = ranges;
      }
    }
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
      let update = dispatch(state, PlaybackIntent::Stop);
      observe_replacement(
        &mut state.shell.embedded_player,
        state.playback.view.lifecycle.replacement_generation,
      );
      state.shell.embedded_player.returning = update.transition.replacement_accepted;
      return update.task;
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
      return dispatch(state, intent).task;
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
  // Back and information share the top chrome, independently of the transport.
  if position.y <= 100.0 && (position.x <= 112.0 || position.x >= bounds.width - 112.0) {
    surface.back_visible = true;
    surface.back_deadline = Some(now + IDLE);
  }
}

fn dispatch(state: &mut State, intent: PlaybackIntent) -> super::playback::PlaybackUpdate {
  super::playback::update(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
    PlaybackMessage::Intent(Box::new(intent)),
  )
}

/// Capture local adjustment previews before playback consumes the input.
pub(super) fn before_playback(state: &mut State, message: &PlaybackMessage) {
  if !crate::embedded::enabled() {
    return;
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
    _ => {}
  }
}

/// Navigation consumes the session's accepted outcome, never a raw controller message.
pub(super) fn after_playback(state: &mut State, acceptance: ControllerAcceptance) -> bool {
  if !crate::embedded::enabled() {
    return false;
  }
  let surface = &mut state.shell.embedded_player;
  observe_replacement(
    surface,
    state.playback.view.lifecycle.replacement_generation,
  );
  let stop = match acceptance {
    ControllerAcceptance::Applied { stop } => stop,
    _ => None,
  };
  stop_return(surface, stop)
}

fn stop_return(surface: &mut Surface, stop: Option<StopCompletion>) -> bool {
  stop.is_some_and(|outcome| {
    std::mem::take(&mut surface.returning) && outcome == StopCompletion::Succeeded
  })
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
  fn queue_fades_follow_list_geometry_not_media_artwork_refreshes() {
    let mut surface = Surface::default();
    reconcile_queue(&mut surface, true, true, 8);
    surface.queue_scroll_edges = (true, true);

    observe_replacement(&mut surface, 1);
    reconcile_queue(&mut surface, true, true, 8);
    assert_eq!(
      surface.queue_scroll_edges,
      (true, true),
      "refreshing artwork must preserve an open queue's measured scroll indicators"
    );

    reconcile_queue(&mut surface, true, false, 8);
    reconcile_queue(&mut surface, true, true, 8);
    assert_eq!(
      surface.queue_scroll_edges,
      (true, true),
      "temporarily covering the queue with Settings must preserve its native scroll state"
    );

    reconcile_queue(&mut surface, true, true, 2);
    assert_eq!(
      surface.queue_scroll_edges,
      (false, false),
      "a short replacement list will not emit an on_scroll callback"
    );

    surface.queue_scroll_edges = (true, false);
    reconcile_queue(&mut surface, false, false, 2);
    reconcile_queue(&mut surface, true, true, 2);
    assert_eq!(
      surface.queue_scroll_edges,
      (false, false),
      "a reopened popup starts with fresh native scroll state"
    );
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
    pointer_moved(
      &mut surface,
      iced::Point::new(1040.0, 40.0),
      bounds,
      202.0,
      now + IDLE + IDLE,
    );
    assert!(
      surface.back_visible && !surface.visible,
      "the information corner reveals the top chrome"
    );
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
    reconcile_surface(&mut surface, true, false, true, now);
    let now = now + IDLE;
    expire(&mut surface, now, now, false);
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
        original_language: None,
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

  #[test]
  fn information_demand_rejects_late_samples_after_close_replacement_and_window_exit() {
    let now = Instant::now();
    let mut surface = Surface {
      information_open: true,
      ..Surface::default()
    };
    reconcile_surface(&mut surface, true, true, true, now);
    reconcile_observation(&mut surface, true, 0, now);
    let old = surface.observation.unwrap().token;
    let sample = || {
      Some(Box::new(PlaybackStatistics {
        filename: Some("old-file.mkv".into()),
        ..PlaybackStatistics::default()
      }))
    };
    settle_information(&mut surface, old, sample());
    assert!(surface.information.is_some());

    surface.information_open = false;
    surface.information = None;
    reconcile_observation(&mut surface, true, 0, now + Duration::from_millis(1));
    settle_information(&mut surface, old, sample());
    assert!(surface.information.is_none());
    surface.information_open = true;
    reconcile_observation(&mut surface, true, 0, now + Duration::from_millis(2));
    settle_information(&mut surface, old, sample());
    assert!(
      surface.information.is_none(),
      "reopening must not revive an old request"
    );

    let current = surface.observation.unwrap().token;
    settle_information(&mut surface, current, sample());
    observe_replacement(&mut surface, 1);
    settle_information(&mut surface, current, sample());
    assert!(
      surface.information.is_none(),
      "replacing media clears and rejects old metadata"
    );
    reconcile_observation(&mut surface, true, 1, now + Duration::from_millis(3));
    let current = surface.observation.unwrap().token;
    settle_information(&mut surface, current, None);
    assert!(
      surface.information_failed,
      "failed reads are not zero-valued successful samples"
    );
    settle_information(&mut surface, current, sample());
    assert!(!surface.information_failed);
    reconcile_surface(&mut surface, false, false, false, now + IDLE);
    settle_information(&mut surface, current, sample());
    assert!(surface.information.is_none());
    assert!(
      surface.observation.is_none(),
      "hidden windows own no statistics stream"
    );
  }
}

#[cfg(test)]
mod return_tests {
  use super::*;

  #[test]
  fn back_waits_for_accepted_stop_success_and_failed_stop_can_be_retried() {
    let mut surface = Surface {
      returning: true,
      ..Surface::default()
    };
    assert!(!stop_return(&mut surface, None));
    assert!(surface.returning);
    assert!(!stop_return(&mut surface, Some(StopCompletion::Failed)));
    assert!(!surface.returning);
    surface.returning = true;
    assert!(stop_return(&mut surface, Some(StopCompletion::Succeeded)));
    assert!(!stop_return(&mut surface, Some(StopCompletion::Succeeded)));
  }

  #[test]
  fn a_later_replacement_cancels_back_but_observing_its_own_stop_does_not() {
    let mut surface = Surface::default();
    observe_replacement(&mut surface, 1);
    surface.returning = true;
    observe_replacement(&mut surface, 1);
    assert!(stop_return(&mut surface, Some(StopCompletion::Succeeded)));

    surface.returning = true;
    observe_replacement(&mut surface, 2);
    assert!(!stop_return(&mut surface, Some(StopCompletion::Succeeded)));
  }
}
