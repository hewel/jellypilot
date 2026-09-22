//! Embedded-only presentation lifecycle and keyboard intent orchestration.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::futures::SinkExt;
use iced::{event, keyboard, Event, Subscription, Task};
use jellypilot_core::config::AppMode;
use jellypilot_core::now_playing_adjustments::{Control, Input as AdjustmentInput};
use jellypilot_mpv::playback_session::{ControllerAcceptance, PlaybackIntent, StopCompletion};
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
  PointerLeft,
  Back,
  SeekBy(f64),
  VolumeBy(f64),
  SeekHovered(Option<f64>),
  Wake(Instant),
  OptionsToggled,
  OptionsDismissed,
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
  ChaptersLoaded {
    generation: u64,
    chapters: Option<Arc<Vec<jellypilot_mpv::statistics::PlaybackChapter>>>,
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
      Self::PointerLeft => "PointerLeft",
      Self::Back => "Back",
      Self::SeekBy(_) => "SeekBy",
      Self::VolumeBy(_) => "VolumeBy",
      Self::SeekHovered(_) => "SeekHovered",
      Self::Wake(_) => "Wake",
      Self::OptionsToggled => "OptionsToggled",
      Self::OptionsDismissed => "OptionsDismissed",
      Self::InformationToggled => "InformationToggled",
      Self::InformationDismissed => "InformationDismissed",
      Self::InformationSampled { .. } => "InformationSampled([redacted])",
      Self::BufferSampled { .. } => "BufferSampled",
      Self::ChaptersLoaded { .. } => "ChaptersLoaded",
      Self::QueueArtworkLoaded(_) => "QueueArtworkLoaded([redacted])",
      Self::QueueScrolled { .. } => "QueueScrolled",
    })
  }
}

#[derive(Default)]
pub struct Surface {
  visible: bool,
  idle_deadline: Option<Instant>,
  minimal_deadline: Option<Instant>,
  paused: bool,
  pointer_regions: Option<(bool, bool)>,
  back_visible: bool,
  back_deadline: Option<Instant>,
  cursor_visible: bool,
  cursor_deadline: Option<Instant>,
  feedback: Option<(String, Instant)>,
  seek_hover: Option<f64>,
  returning: bool,
  replacement_generation: u64,
  was_active: bool,
  options_open: bool,
  information_open: bool,
  information: Option<Box<PlaybackStatistics>>,
  information_failed: bool,
  buffered_ranges: Vec<(f64, f64)>,
  observation: Option<Observation>,
  queue_artwork: super::artwork::ImageCollection,
  queue_observed: bool,
  queue_scroll_edges: (bool, bool),
  /// Chapter metadata fetched once per media generation; `None` while the
  /// request is pending or was never issued, `Some` (possibly empty) once
  /// the reader answered for the current generation.
  chapters: Option<Arc<Vec<jellypilot_mpv::statistics::PlaybackChapter>>>,
  /// Chapter start times mirrored for the marker widget, which cannot see
  /// the MPV chapter type.
  chapter_times: Vec<f64>,
  /// Generation the outstanding chapter fetch belongs to.
  chapter_demand: Option<u64>,
  /// Real intro/credit intervals converted for the marker widget.
  intro_ranges: Vec<(f64, f64)>,
  queue_item_count: usize,
}

impl Surface {
  fn minimal_visible(&self) -> bool {
    !self.visible && (self.paused || self.minimal_deadline.is_some())
  }
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

#[derive(Clone)]
struct ChapterSubscription {
  generation: u64,
  controller: super::state::PlaybackControllerHandle,
}

impl Hash for ChapterSubscription {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.generation.hash(state);
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

pub(super) fn options_open(state: &State) -> bool {
  state.shell.embedded_player.options_open
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

fn presentation_active(state: &State) -> bool {
  active(state) || (state.tv_mode() && super::tv::player::active(state))
}

fn menu_open(state: &State) -> bool {
  state.playback.audio_menu_open
    || state.playback.subtitle_menu_open
    || state.playback.queue_menu_open
}

pub(super) fn input_blocked(state: &State) -> bool {
  controls_blocked(state) || options_open(state)
}

/// Options may operate their own controls while blocking picture shortcuts.
pub(super) fn controls_blocked(state: &State) -> bool {
  state.shell.settings_open
    || state.shell.account_popover_open
    || state.shell.compact_search_open
    || state.settings.view.shortcut_capture.is_some()
    || state.playback.view.lifecycle.replacing
    || state.shell.embedded_player.returning
    || super::accounts::blocking_modal(&state.accounts)
    || state.kernel.sdk.content_mutations_blocked()
    || menu_open(state)
}

pub(super) fn controls_visible(state: &State) -> bool {
  !active(state) || state.shell.embedded_player.visible || held(state)
}

pub(super) fn minimal_visible(state: &State) -> bool {
  !controls_visible(state) && state.shell.embedded_player.minimal_visible()
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
  state.playback.adjustments.view().seek_dragging
}

pub(super) fn is_volume_dragging(state: &State) -> bool {
  state.playback.adjustments.view().volume_dragging
}

fn held(state: &State) -> bool {
  // Only interactions that genuinely need the controls keep them open: an
  // in-flight drag, an open track/queue menu, or blocked input. Pausing, a
  // live skip prompt, and the Information panel stay visible on their own
  // without forcing the complete presentation.
  let adjustments = state.playback.adjustments.view();
  adjustments.seek_dragging
    || adjustments.volume_dragging
    || input_blocked(state)
    || state.shell.embedded_player.returning
}

pub(super) fn reconcile(state: &mut State) {
  observe_replacement(
    &mut state.shell.embedded_player,
    state.playback.view.lifecycle.replacement_generation,
  );
  let active = presentation_active(state);
  if !active
    || controls_blocked(state)
    || !super::view::player::embedded_options_available(state, state.shell.window_size.width)
  {
    state.shell.embedded_player.options_open = false;
  }
  let settled = state.playback.view.lifecycle.settled;
  // The core adjustment state owns drag flags and desired targets; embedded
  // only reports whether its presentation is active and settled.
  let _ = super::playback::adjust(
    &mut state.playback,
    AdjustmentInput::Presentation { active, settled },
  );
  let held = held(state);
  let paused = state
    .playback
    .view
    .now_playing
    .as_ref()
    .is_some_and(|playing| playing.paused);
  reconcile_surface(
    &mut state.shell.embedded_player,
    active,
    held,
    paused,
    Instant::now(),
  );
  let blocked = state.shell.settings_open
    || state.shell.account_popover_open
    || super::accounts::blocking_modal(&state.accounts)
    || state.shell.quit_requested;
  if blocked || menu_open(state) || options_open(state) {
    state.shell.embedded_player.information_open = false;
    state.shell.embedded_player.information = None;
  }
  // Information keeps its statistics demand while the rest of the chrome is
  // allowed to auto-hide; buffer-only sampling still follows the controls.
  let visible = active
    && ((!state.tv_mode() && controls_visible(state))
      || state.shell.embedded_player.information_open)
    && !blocked;
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
  reconcile_chapters(
    surface,
    active && !replacing && state.playback.view.now_playing.is_some(),
    generation,
  );
  sync_intro_ranges(surface, &state.playback.view.intro_ranges);
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
    surface.returning = false;
    surface.feedback = None;
    surface.options_open = false;
    surface.information = None;
    surface.information_failed = false;
    surface.buffered_ranges.clear();
    surface.observation = None;
    surface.chapters = None;
    surface.chapter_times.clear();
    surface.chapter_demand = None;
    surface.intro_ranges.clear();
    surface.queue_artwork.clear();
  }
}

/// Chapter metadata is queried once per media generation, only while a real
/// item is playing and not mid-replacement. The demand survives until the
/// reader answers; a failed read still settles it so the timeline never
fn reconcile_chapters(surface: &mut Surface, needed: bool, generation: u64) {
  if !needed {
    surface.chapter_demand = None;
    return;
  }
  if surface.chapters.is_none() && surface.chapter_demand.is_none() {
    surface.chapter_demand = Some(generation);
  }
}

/// Mirrors the session's fetched intro/credit intervals into the marker
/// widget's plain pair form; empty while inactive, unfetched, or retired.
fn sync_intro_ranges(surface: &mut Surface, ranges: &[jellypilot_media_server::IntroSkipRange]) {
  let converted = ranges
    .iter()
    .map(|range| (range.start_seconds, range.end_seconds));
  if !surface.intro_ranges.iter().copied().eq(converted.clone()) {
    surface.intro_ranges.clear();
    surface.intro_ranges.extend(converted);
  }
}

pub(super) fn chapters(state: &State) -> Option<&[jellypilot_mpv::statistics::PlaybackChapter]> {
  state
    .shell
    .embedded_player
    .chapters
    .as_deref()
    .map(Vec::as_slice)
}

pub(super) fn chapter_times(state: &State) -> &[f64] {
  &state.shell.embedded_player.chapter_times
}

pub(super) fn intro_ranges(state: &State) -> &[(f64, f64)] {
  &state.shell.embedded_player.intro_ranges
}

fn reconcile_surface(surface: &mut Surface, active: bool, held: bool, paused: bool, now: Instant) {
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
  if !held {
    if let Some((bottom, top)) = surface.pointer_regions {
      if !bottom {
        hide_controls(surface, now);
      }
      if !top {
        surface.back_visible = false;
        surface.back_deadline = None;
      }
    }
  }
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
  if surface.paused && !paused && !surface.visible {
    surface.minimal_deadline = Some(now + IDLE);
  }
  surface.paused = paused;
  if surface.visible || paused {
    surface.minimal_deadline = None;
  }
}

pub(super) fn subscription(state: &State) -> Subscription<AppMessage> {
  if !presentation_active(state) {
    return Subscription::none();
  }
  let surface = &state.shell.embedded_player;
  let deadline = surface
    .idle_deadline
    .into_iter()
    .chain(surface.back_deadline)
    .chain(surface.cursor_deadline)
    .chain(surface.minimal_deadline)
    .chain(surface.feedback.as_ref().map(|(_, deadline)| *deadline))
    .min();
  let wake = deadline
    .filter(|_| !state.tv_mode())
    .map_or_else(Subscription::none, |deadline| {
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
  let chapters = match (surface.chapter_demand, state.playback.controller.as_ref()) {
    (Some(generation), Some(controller)) => Subscription::run_with(
      ChapterSubscription {
        generation,
        controller: Arc::clone(controller),
      },
      chapter_stream,
    ),
    _ => Subscription::none(),
  };
  Subscription::batch([wake, observation, chapters])
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

/// One chapter metadata read per demand generation. The stream stays alive
/// after answering so the subscription is not respawned into a second query;
/// `ChaptersLoaded` settles the demand and drops the stream.
fn chapter_stream(
  subscription: &ChapterSubscription,
) -> impl iced::futures::Stream<Item = AppMessage> {
  let subscription = subscription.clone();
  iced::stream::channel(1, async move |mut output| {
    let reader = subscription.controller.lock().await.statistics_reader();
    let chapters = match reader {
      Some(reader) => reader.chapters().await.ok().map(Arc::new),
      None => None,
    };
    let _ = output
      .send(AppMessage::EmbeddedPlayer(Message::ChaptersLoaded {
        generation: subscription.generation,
        chapters,
      }))
      .await;
    std::future::pending::<()>().await;
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
  use keyboard::key::Named;

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
  if !presentation_active(state) && !matches!(message, Message::InformationDismissed) {
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
    Message::OptionsToggled => {
      if state.playback.view.busy
        || controls_blocked(state)
        || state.shell.quit_requested
        || !super::view::player::embedded_options_available(state, state.shell.window_size.width)
      {
        return Task::none();
      }
      state.shell.embedded_player.options_open = !options_open(state);
    }
    Message::OptionsDismissed => {
      state.shell.embedded_player.options_open = false;
    }
    Message::InformationToggled => {
      if state.playback.view.lifecycle.replacing || state.shell.quit_requested {
        return Task::none();
      }
      let surface = &mut state.shell.embedded_player;
      surface.options_open = false;
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
    Message::ChaptersLoaded {
      generation,
      chapters,
    } => {
      let surface = &mut state.shell.embedded_player;
      if surface.chapter_demand == Some(generation) {
        surface.chapter_demand = None;
      }
      // Late replies for a replaced media generation are dropped, never
      // stored against the new item.
      if surface.replacement_generation == generation {
        if let Some(chapters) = chapters {
          surface.chapter_times = chapters
            .iter()
            .map(|chapter| chapter.time_seconds)
            .collect();
          surface.chapters = Some(chapters);
        } else {
          surface.chapters = Some(Arc::new(Vec::new()));
        }
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
    Message::PointerLeft => pointer_left(&mut state.shell.embedded_player, now),
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
      if input_blocked(state) || state.playback.view.now_playing.is_none() {
        return Task::none();
      }
      let control = if matches!(message, Message::SeekBy(_)) {
        Control::Seek
      } else {
        Control::Volume
      };
      let Some(intent) =
        super::playback::adjust(&mut state.playback, AdjustmentInput::Step(control, delta))
      else {
        return Task::none();
      };
      let text = match intent {
        PlaybackIntent::Seek(position) => format!(
          "{:+.0} s · {}:{:02}",
          delta,
          position as u64 / 60,
          position as u64 % 60
        ),
        PlaybackIntent::SetVolume(volume) => format!("{volume:.0}%"),
        _ => return Task::none(),
      };
      state.shell.embedded_player.feedback = Some((text, now + FEEDBACK));
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
    pointer_left(surface, now);
    return;
  }
  surface.cursor_visible = true;
  surface.cursor_deadline = Some(now + IDLE);
  let bottom = position.y >= (bounds.height - controls_height).max(0.0);
  let top = position.y <= 100.0 && (position.x <= 112.0 || position.x >= bounds.width - 144.0);
  if !bottom
    && !top
    && surface
      .pointer_regions
      .is_some_and(|(bottom, top)| bottom || top)
  {
    surface.minimal_deadline = Some(now + IDLE);
  }
  surface.pointer_regions = Some((bottom, top));
  if bottom {
    surface.visible = true;
    surface.idle_deadline = Some(now + IDLE);
    surface.minimal_deadline = None;
  } else {
    hide_controls(surface, now);
  }
  surface.back_visible = top;
  surface.back_deadline = top.then_some(now + IDLE);
}

fn pointer_left(surface: &mut Surface, now: Instant) {
  if surface
    .pointer_regions
    .is_some_and(|(bottom, top)| bottom || top)
  {
    surface.minimal_deadline = Some(now + IDLE);
  }
  surface.pointer_regions = Some((false, false));
  hide_controls(surface, now);
  surface.back_visible = false;
  surface.back_deadline = None;
  surface.cursor_visible = false;
  surface.cursor_deadline = None;
}

fn hide_controls(surface: &mut Surface, now: Instant) {
  if surface.visible {
    surface.minimal_deadline = Some(now + IDLE);
  }
  surface.visible = false;
  surface.idle_deadline = None;
  surface.seek_hover = None;
}

fn dispatch(state: &mut State, intent: PlaybackIntent) -> super::playback::PlaybackUpdate {
  super::playback::update(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
    PlaybackMessage::Intent(Box::new(intent)),
  )
}

/// Navigation consumes the session's accepted outcome, never a raw controller message.
pub(super) fn after_playback(state: &mut State, acceptance: ControllerAcceptance) -> bool {
  if !crate::embedded::enabled() && !state.tv_mode() {
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
    hide_controls(surface, now);
  }
  if surface.back_deadline == Some(deadline) && now >= deadline && !held {
    surface.back_visible = false;
    surface.back_deadline = None;
  }
  if surface.minimal_deadline == Some(deadline) && now >= deadline && !held {
    surface.minimal_deadline = None;
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

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn narrow_options_keep_skip_accessible_and_capture_dismissal_input() {
    use iced::advanced::{renderer::Headless, widget};
    use iced::{mouse, Point, Rectangle, Size};
    use iced_runtime::user_interface::{Cache, UserInterface};

    #[derive(Default)]
    struct Targets(Vec<Rectangle>);
    impl widget::Operation for Targets {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn focusable(
        &mut self,
        _: Option<&widget::Id>,
        bounds: Rectangle,
        _: &mut dyn widget::operation::Focusable,
      ) {
        self.0.push(bounds);
      }
    }

    let mut state = State::boot(false);
    let client = jellypilot_media_server::JellyfinClient::new();
    client
      .login()
      .adopt_validated_session(&jellypilot_media_server::SavedSession {
        provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
        server_url: "https://media.example.com".to_owned(),
        user_id: "user-1".to_owned(),
        user_name: "User".to_owned(),
        access_token: "token".to_owned(),
        server_name: None,
        device_id: None,
      });
    state.kernel.client = Some(Arc::new(client));
    state.playback.view.now_playing = Some(jellypilot_mpv::playback_session::NowPlayingView {
      item: jellypilot_mpv::playback::NowPlayingItem {
        item_id: "episode".into(),
        title: "Episode".into(),
        item_type: "Episode".into(),
        series_id: Some("series".into()),
        runtime_seconds: Some(60.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".into(),
        original_language: None,
      },
      paused: false,
      position_seconds: 10.0,
      duration_seconds: Some(60.0),
      volume: 75.0,
      muted: false,
    });
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let bounds = Size::new(768.0, 576.0);
    state.shell.window_size = bounds;
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));

    let mut ui = UserInterface::build(
      crate::app::view::player::embedded(&state),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    let mut targets = Targets::default();
    ui.operate(&renderer, &mut targets);
    let trigger = targets
      .0
      .iter()
      .find(|target| target.x > 600.0 && target.x < 680.0 && target.y == 24.0)
      .expect("options trigger beside Information");
    let mut bus = iced::advanced::shell::Bus::new();
    let _ = ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[press.clone(), release.clone()],
      mouse::Cursor::Available(trigger.center()),
      &mut renderer,
      &mut bus,
    );
    assert!(bus
      .drain()
      .any(|(message, _)| matches!(message, AppMessage::EmbeddedPlayer(Message::OptionsToggled))));
    let cache = ui.into_cache();

    state.shell.embedded_player.options_open = true;
    assert!(
      input_blocked(&state),
      "the open menu blocks picture shortcuts"
    );
    assert!(
      !controls_blocked(&state),
      "the menu's own toggle remains usable"
    );
    assert!(
      held(&state),
      "menu interaction holds chrome and cursor visible"
    );
    let mut ui = UserInterface::build(
      crate::app::view::player::embedded(&state),
      bounds,
      cache,
      &mut renderer,
    );
    let mut targets = Targets::default();
    ui.operate(&renderer, &mut targets);
    let toggle = targets
      .0
      .iter()
      .find(|target| target.width > 100.0 && target.y >= 72.0 && target.y < 150.0)
      .expect("skip toggle inside the options menu");
    let mut bus = iced::advanced::shell::Bus::new();
    let _ = ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[press.clone(), release],
      mouse::Cursor::Available(toggle.center()),
      &mut renderer,
      &mut bus,
    );
    assert!(bus.drain().any(|(message, _)| matches!(
      message,
      AppMessage::Playback(PlaybackMessage::IntroModeChanged(false))
    )));

    for event in [
      key(keyboard::Key::Named(keyboard::key::Named::Escape), false),
      press,
    ] {
      let mut bus = iced::advanced::shell::Bus::new();
      let (_, statuses) = ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[event],
        mouse::Cursor::Available(Point::new(384.0, 288.0)),
        &mut renderer,
        &mut bus,
      );
      assert_eq!(statuses, [event::Status::Captured]);
      let messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
      assert!(messages.iter().any(|message| matches!(
        message,
        AppMessage::EmbeddedPlayer(Message::OptionsDismissed)
      )));
      assert!(!messages
        .iter()
        .any(|message| matches!(message, AppMessage::Playback(PlaybackMessage::Intent(_)))));
    }
  }

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
    // Leaving the transport switches immediately, without waiting for idle.
    pointer_moved(
      &mut surface,
      iced::Point::new(550.0, 450.0),
      bounds,
      202.0,
      now + Duration::from_millis(50),
    );
    assert!(!surface.visible && surface.minimal_visible());
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
    pointer_moved(
      &mut surface,
      iced::Point::new(977.0, 40.0),
      bounds,
      202.0,
      now + IDLE + IDLE,
    );
    assert!(
      surface.back_visible,
      "the options target stays inside the top reveal region"
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
    reconcile_surface(&mut surface, true, false, false, now);
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
    reconcile_surface(&mut surface, false, false, false, deadline);
    assert!(!surface.cursor_visible);
    assert!(surface.cursor_deadline.is_none());
  }

  #[test]
  fn idle_holds_cancel_old_deadlines_and_exit_clears_feedback() {
    let now = Instant::now();
    let mut surface = Surface::default();
    reconcile_surface(&mut surface, true, false, false, now);
    let old = surface.idle_deadline.unwrap();
    // A menu or drag arriving before the wake must keep controls visible.
    reconcile_surface(&mut surface, true, true, false, now + IDLE);
    expire(&mut surface, old, now + IDLE, true);
    assert!(surface.visible);
    assert!(surface.idle_deadline.is_none());
    reconcile_surface(&mut surface, true, false, false, now + IDLE);
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
  fn paused_playback_with_information_and_manual_prompt_can_auto_hide() {
    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(jellypilot_mpv::playback_session::NowPlayingView {
      item: jellypilot_mpv::playback::NowPlayingItem {
        item_id: "episode".into(),
        title: "Episode".into(),
        item_type: "Episode".into(),
        series_id: Some("series".into()),
        runtime_seconds: Some(60.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".into(),
        original_language: None,
      },
      paused: true,
      position_seconds: 10.0,
      duration_seconds: Some(60.0),
      volume: 75.0,
      muted: false,
    });
    state.playback.view.intro_prompt = Some(jellypilot_mpv::playback_session::IntroPromptView {
      kind: jellypilot_media_server::IntroSkipKind::Introduction,
    });
    state.shell.embedded_player.information_open = true;
    let now = Instant::now();
    let hold = held(&state);
    let surface = &mut state.shell.embedded_player;
    reconcile_surface(surface, true, hold, true, now);
    expire(surface, now + IDLE, now + IDLE, hold);
    assert!(!surface.visible && !surface.back_visible);
    reconcile_surface(surface, true, hold, true, now + IDLE);
    expire(surface, now + IDLE + IDLE, now + IDLE + IDLE, hold);
    assert!(surface.minimal_visible());
    assert!(surface.information_open);
    assert!(state.playback.view.intro_prompt.is_some());
  }

  #[test]
  fn leaving_controls_shows_minimal_then_hides_it_without_picture_motion_extending_it() {
    let now = Instant::now();
    let bounds = iced::Size::new(1100.0, 900.0);
    let mut surface = Surface::default();
    reconcile_surface(&mut surface, true, false, false, now);
    pointer_moved(
      &mut surface,
      iced::Point::new(550.0, 800.0),
      bounds,
      202.0,
      now,
    );
    let left_at = now + Duration::from_millis(50);
    pointer_moved(
      &mut surface,
      iced::Point::new(550.0, 450.0),
      bounds,
      202.0,
      left_at,
    );
    assert!(!surface.visible && !surface.back_visible && surface.minimal_visible());

    pointer_moved(
      &mut surface,
      iced::Point::new(600.0, 450.0),
      bounds,
      202.0,
      left_at + IDLE,
    );
    expire(&mut surface, left_at + IDLE, left_at + IDLE, false);
    reconcile_surface(&mut surface, true, false, false, left_at + IDLE);
    assert!(!surface.minimal_visible());
    assert!(
      surface.cursor_visible,
      "picture motion only renews the cursor"
    );
  }

  #[test]
  fn pause_restores_minimal_and_resume_gives_it_a_fresh_timeout() {
    let now = Instant::now();
    let mut surface = Surface::default();
    reconcile_surface(&mut surface, true, false, false, now);
    pointer_left(&mut surface, now);
    expire(&mut surface, now + IDLE, now + IDLE, false);
    assert!(!surface.minimal_visible());

    reconcile_surface(&mut surface, true, false, true, now + IDLE);
    expire(&mut surface, now + IDLE + IDLE, now + IDLE + IDLE, false);
    assert!(surface.minimal_visible(), "pause has no minimal timeout");

    let resumed = now + IDLE + IDLE;
    reconcile_surface(&mut surface, true, false, false, resumed);
    expire(&mut surface, now + IDLE, resumed, false);
    assert!(
      surface.minimal_visible(),
      "an old wake cannot hide the resumed presentation"
    );
    expire(&mut surface, resumed + IDLE, resumed + IDLE, false);
    reconcile_surface(&mut surface, true, false, false, resumed + IDLE);
    assert!(!surface.minimal_visible());
  }

  #[test]
  fn leaving_during_a_hold_switches_to_minimal_when_the_hold_ends() {
    let now = Instant::now();
    let mut surface = Surface::default();
    reconcile_surface(&mut surface, true, true, false, now);
    pointer_left(&mut surface, now);
    reconcile_surface(&mut surface, true, true, false, now);
    assert!(surface.visible && !surface.minimal_visible());
    reconcile_surface(&mut surface, true, false, false, now + IDLE);
    assert!(!surface.visible && !surface.back_visible && surface.minimal_visible());
    expire(&mut surface, now + IDLE + IDLE, now + IDLE + IDLE, false);
    assert!(!surface.minimal_visible());
  }

  #[test]
  fn information_demand_rejects_late_samples_after_close_replacement_and_window_exit() {
    let now = Instant::now();
    let mut surface = Surface {
      information_open: true,
      ..Surface::default()
    };
    reconcile_surface(&mut surface, true, true, false, now);
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

#[cfg(test)]
mod marker_tests {
  use super::*;

  #[test]
  fn chapter_demand_is_issued_once_per_generation_and_settles_on_reply() {
    let mut surface = Surface::default();
    reconcile_chapters(&mut surface, true, 7);
    assert_eq!(surface.chapter_demand, Some(7));
    // A second reconcile while the read is in flight must not reissue.
    reconcile_chapters(&mut surface, true, 7);
    assert_eq!(surface.chapter_demand, Some(7));

    surface.chapter_demand = None;
    surface.chapters = Some(Arc::new(Vec::new()));
    reconcile_chapters(&mut surface, true, 7);
    assert!(
      surface.chapter_demand.is_none(),
      "an answered generation must never be queried again"
    );

    // A replacement clears the cache and re-arms the demand for the new media.
    observe_replacement(&mut surface, 8);
    assert!(surface.chapters.is_none() && surface.chapter_times.is_empty());
    reconcile_chapters(&mut surface, true, 8);
    assert_eq!(surface.chapter_demand, Some(8));

    // Hidden or replaced media drops the demand without fetching.
    reconcile_chapters(&mut surface, false, 8);
    assert!(surface.chapter_demand.is_none());
  }
}
