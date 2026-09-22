//! TV presentation for the existing playback session. Engine ownership stays in playback.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::{
  button, column, container, mouse_area, opaque, responsive, row, scrollable, space, stack, text,
  Column,
};
use iced::{Alignment, Element, Fill, Length, Size, Subscription, Task};
use jellypilot_core::intro_skipper::IntroSkipMode;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::tv_navigation::Input;
use jellypilot_core::tv_player::{Action, Control, Observation, Panel, Player};
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_mpv::playback::TrackInfo;
use jellypilot_mpv::playback_session::{
  AdjacentAvailability, AdjacentDirection, PlaybackIntent, TracksView,
};
use jellypilot_mpv::player::format_duration;
use jellypilot_mpv::statistics::PlaybackControls;
use jellypilot_ui::fonts::{BODY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::inert::inert;
use jellypilot_ui::widgets::tv_focus::focus;
use jellypilot_ui::widgets::{embedded_player as cinema, tv_player as chrome};

use crate::app::embedded_player;
use crate::app::message::{Message as AppMessage, PlaybackMessage};
use crate::app::playback::{self, QueueState};
use crate::app::state::{Destination, State};

#[derive(Clone, Debug)]
pub enum Message {
  Activate(Control),
  Choice(usize),
  ClosePanel,
  Seek,
  ConfirmSeek,
  CancelSeek,
  PointerMoved(iced::Point),
  PicturePressed,
  Wake(Instant),
  Poll,
  ControlsSampled {
    generation: u64,
    token: Instant,
    sample: Option<Box<PlaybackControls>>,
  },
  Dispatch {
    session: SessionToken,
    generation: u64,
    presentation: Option<Instant>,
    command: Box<AppMessage>,
  },
}

#[derive(Default)]
pub struct Surface {
  pub remote: Player,
  generation: Option<u64>,
  presentation: Option<Instant>,
  controls: Option<Box<PlaybackControls>>,
  sampling: Option<Instant>,
  choices: Vec<Choice>,
  choices_revision: u64,
  pointer: Option<iced::Point>,
  notice: Option<crate::i18n::UiText>,
}

#[derive(Clone)]
enum ChoiceAction {
  Audio(i64),
  Subtitle(Option<i64>),
  Queue(Box<VideoLibraryItem>),
  Speed(f64),
  SessionSkip,
  Close,
}

struct Choice {
  label: String,
  action: ChoiceAction,
}

fn message(message: Message) -> AppMessage {
  AppMessage::Tv(super::Message::Player(message))
}

fn command(state: &State, command: AppMessage) -> Task<AppMessage> {
  Task::done(message(Message::Dispatch {
    session: state.kernel.request_gate.current_session(),
    generation: state.playback.view.lifecycle.replacement_generation,
    presentation: state.tv.player.presentation,
    command: Box::new(command),
  }))
}

fn dispatch(state: &State, intent: PlaybackIntent) -> Task<AppMessage> {
  command(
    state,
    AppMessage::Playback(PlaybackMessage::Intent(Box::new(intent))),
  )
}

pub fn active(state: &State) -> bool {
  state.tv_mode()
    && state.shell.images_visible
    && state.shell.window_id.is_some()
    && state.playback.view.lifecycle.playback_active
    && (embedded_player::active(state) || state.shell.destination == Destination::NowPlaying)
}

fn input_admitted(state: &State) -> bool {
  active(state)
    && !state.shell.quit_requested
    && state.shell.pending_close.is_none()
    && state.kernel.connection == jellypilot_auth::login::ConnectionPhase::Connected
    && !crate::app::accounts::content_mutations_blocked(&state.kernel)
    && !crate::app::accounts::blocking_modal(&state.accounts)
    && !state.shell.settings_open
    && !state.shell.account_popover_open
    && !state.tv.settings.open
    && !state.tv.search.open
}

/// Retires reads and panel state when the TV presentation releases the session.
/// A late read from that presentation cannot block the next entry or supply its facts.
pub fn leave(state: &mut State) -> Task<AppMessage> {
  let information_open = state
    .tv
    .player
    .remote
    .panel()
    .is_some_and(|panel| panel.kind == Panel::Information);
  state.tv.player = Surface::default();
  if information_open {
    embedded_player::update(state, embedded_player::Message::InformationDismissed)
  } else {
    Task::none()
  }
}

fn observation(state: &State) -> Observation {
  let now_playing = state.playback.view.now_playing.as_ref();
  Observation {
    generation: state.playback.view.lifecycle.replacement_generation,
    paused: now_playing.is_some_and(|playing| playing.paused),
    position: now_playing.map_or(0.0, |playing| playing.position_seconds),
    seek_range: seek_range(state),
    busy: state.playback.view.busy || state.playback.view.lifecycle.replacing,
  }
}

fn seek_range(state: &State) -> Option<(f64, f64)> {
  let sample = state.tv.player.controls.as_ref()?;
  let playing = state.playback.view.now_playing.as_ref()?;
  if sample.seekable != Some(true) {
    return None;
  }
  if sample.partially_seekable == Some(false) {
    return sample
      .duration_seconds
      .filter(|duration| duration.is_finite() && *duration > 0.0)
      .map(|duration| (0.0, duration));
  }
  // A partial stream may have disjoint cache intervals. Staying in the
  // current interval never offers an uncached gap as a valid target.
  sample.buffered_ranges.iter().copied().find(|(start, end)| {
    start.is_finite()
      && end.is_finite()
      && *start >= 0.0
      && *end > *start
      && playing.position_seconds >= *start
      && playing.position_seconds <= *end
  })
}

fn available_controls(state: &State) -> Vec<Control> {
  let mut controls = vec![Control::Queue, Control::Information];
  if matches!(
    state.playback.view.adjacent.previous,
    AdjacentAvailability::Available { .. }
  ) {
    controls.push(Control::Previous);
  }
  if seek_range(state).is_some() {
    controls.push(Control::Backward);
  }
  controls.push(Control::PlayPause);
  if seek_range(state).is_some() {
    controls.push(Control::Forward);
  }
  if matches!(
    state.playback.view.adjacent.next,
    AdjacentAvailability::Available { .. }
  ) {
    controls.push(Control::Next);
  }
  controls.extend([Control::Audio, Control::Subtitles, Control::Settings]);
  if state.playback.view.intro_prompt.is_some() {
    controls.push(Control::Skip);
  }
  controls
}

pub fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  if !input_admitted(state) {
    return Task::none();
  }
  if input == Input::Confirm
    && state
      .tv
      .player
      .remote
      .panel()
      .and_then(|panel| state.tv.player.choices.get(panel.focused))
      .is_some_and(|choice| matches!(choice.action, ChoiceAction::Close))
  {
    return update(state, Message::ClosePanel);
  }
  let observation = observation(state);
  let controls = available_controls(state);
  let before = state.tv.player.remote.panel();
  let action = state
    .tv
    .player
    .remote
    .input(input, observation, &controls, Instant::now());
  let task = apply_action(state, action);
  Task::batch([
    task,
    synchronize_panel(state, before.map(|panel| panel.kind)),
  ])
}

pub fn update(state: &mut State, event: Message) -> Task<AppMessage> {
  if !active(state) {
    return Task::none();
  }
  if !matches!(
    event,
    Message::ControlsSampled { .. } | Message::Wake(_) | Message::Poll
  ) && !input_admitted(state)
  {
    return Task::none();
  }
  let now = Instant::now();
  match event {
    Message::Activate(control) => {
      if control != Control::Back && !available_controls(state).contains(&control) {
        return Task::none();
      }
      let observation = observation(state);
      let action = state.tv.player.remote.activate(control, observation, now);
      apply_action(state, action)
    }
    Message::Choice(index) => {
      if state.tv.player.remote.panel().is_none() {
        return Task::none();
      }
      state.tv.player.remote.focus_choice(index);
      apply_choice(state, index)
    }
    Message::ClosePanel => {
      let panel = state.tv.player.remote.panel().map(|panel| panel.kind);
      state.tv.player.remote.close_panel(now);
      synchronize_panel(state, panel)
    }
    Message::Seek => {
      let observation = observation(state);
      state.tv.player.remote.begin_seek(observation, now);
      Task::none()
    }
    Message::ConfirmSeek => input(state, Input::Confirm),
    Message::CancelSeek => input(state, Input::Down),
    Message::PicturePressed => {
      if state.tv.player.remote.panel().is_some() {
        return update(state, Message::ClosePanel);
      }
      if state.tv.player.remote.full() {
        input(state, Input::Back)
      } else {
        state.tv.player.remote.reveal(now);
        Task::none()
      }
    }
    Message::PointerMoved(position) => {
      if state.tv.player.pointer != Some(position) {
        state.tv.player.pointer = Some(position);
        state.tv.player.remote.reveal(now);
      }
      Task::none()
    }
    Message::Wake(now) => {
      state.tv.player.remote.expire(now);
      Task::none()
    }
    Message::Poll => sample_controls(state),
    Message::ControlsSampled {
      generation,
      token,
      sample,
    } => {
      if state.tv.player.generation == Some(generation) && state.tv.player.sampling == Some(token) {
        state.tv.player.sampling = None;
        state.tv.player.controls = sample;
      }
      Task::none()
    }
    Message::Dispatch {
      session,
      generation,
      presentation,
      command,
    } => {
      if state.kernel.request_gate.is_current_session(session)
        && generation == state.playback.view.lifecycle.replacement_generation
        && presentation == state.tv.player.presentation
        && input_admitted(state)
      {
        crate::app::update::route_message(state, *command)
      } else {
        Task::none()
      }
    }
  }
}

fn sample_controls(state: &mut State) -> Task<AppMessage> {
  if state.tv.player.sampling.is_some() {
    return Task::none();
  }
  let Some(controller) = state.playback.controller.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let generation = state.playback.view.lifecycle.replacement_generation;
  let token = Instant::now();
  state.tv.player.sampling = Some(token);
  Task::perform(
    async move {
      let reader = controller.lock().await.statistics_reader();
      match reader {
        Some(reader) => reader.controls().await.ok().map(Box::new),
        None => None,
      }
    },
    move |sample| {
      message(Message::ControlsSampled {
        generation,
        token,
        sample,
      })
    },
  )
}

pub fn reconcile(state: &mut State) -> Task<AppMessage> {
  if !active(state) {
    if state.tv.player.generation.is_some() {
      state.tv.player = Surface::default();
    }
    return Task::none();
  }
  let generation = state.playback.view.lifecycle.replacement_generation;
  if state.tv.player.generation != Some(generation) {
    state.tv.player = Surface {
      generation: Some(generation),
      presentation: Some(Instant::now()),
      ..Surface::default()
    };
  }
  let observation = observation(state);
  let replaced = state.tv.player.remote.observe(observation, Instant::now());
  refresh_panel_choices(state);
  if state.playback.notice != state.tv.player.notice {
    state.tv.player.notice = state.playback.notice.clone();
    if state.tv.player.notice.is_some() {
      state.tv.player.remote.reveal(Instant::now());
    }
  }
  if replaced {
    sample_controls(state)
  } else {
    Task::none()
  }
}

pub fn subscription(state: &State) -> Subscription<AppMessage> {
  if !active(state) {
    return Subscription::none();
  }
  let idle = if state.tv.player.remote.deadline().is_some() {
    iced::time::every(Duration::from_millis(250)).map(|now| message(Message::Wake(now)))
  } else {
    Subscription::none()
  };
  Subscription::batch([
    idle,
    iced::time::every(Duration::from_secs(1)).map(|_| message(Message::Poll)),
  ])
}

fn apply_action(state: &mut State, action: Option<Action>) -> Task<AppMessage> {
  match action {
    None => Task::none(),
    Some(Action::Exit) => command(
      state,
      AppMessage::EmbeddedPlayer(embedded_player::Message::Back),
    ),
    Some(Action::TogglePaused) => dispatch(state, PlaybackIntent::TogglePaused),
    Some(Action::Seek(position)) => dispatch(state, PlaybackIntent::Seek(position)),
    Some(Action::Previous) => dispatch(
      state,
      PlaybackIntent::PlayAdjacent(AdjacentDirection::Previous),
    ),
    Some(Action::Next) => dispatch(state, PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)),
    Some(Action::Skip) => dispatch(state, PlaybackIntent::SkipIntro),
    Some(Action::OpenPanel(panel)) => open_panel(state, panel),
    Some(Action::ApplyChoice(index)) => apply_choice(state, index),
  }
}

fn open_panel(state: &mut State, panel: Panel) -> Task<AppMessage> {
  let choices = panel_choices(state, panel);
  let selected = if panel == Panel::Settings {
    5
  } else {
    choices
      .iter()
      .position(|choice| choice_selected(state, &choice.action))
      .unwrap_or(0)
  };
  state
    .tv
    .player
    .remote
    .open_panel(panel, selected, choices.len(), Instant::now());
  state.tv.player.choices_revision = choices_revision(state, panel);
  state.tv.player.choices = choices;
  if panel == Panel::Information {
    command(
      state,
      AppMessage::EmbeddedPlayer(embedded_player::Message::InformationToggled),
    )
  } else {
    Task::none()
  }
}

fn choices_revision(state: &State, panel: Panel) -> u64 {
  let mut hash = std::collections::hash_map::DefaultHasher::new();
  state.kernel.locale.hash(&mut hash);
  match panel {
    Panel::Audio | Panel::Subtitles => {
      std::mem::discriminant(&state.playback.view.tracks).hash(&mut hash);
      if let TracksView::Ready { tracks, .. } = &state.playback.view.tracks {
        for track in tracks {
          (&track.id, &track.track_type, &track.title, &track.language).hash(&mut hash);
        }
      }
    }
    Panel::Queue => {
      std::mem::discriminant(&state.playback.queue).hash(&mut hash);
      if let QueueState::Ready(items) = &state.playback.queue {
        for item in items {
          (&item.id, &item.name).hash(&mut hash);
        }
      }
    }
    _ => {}
  }
  hash.finish()
}

fn same_choice(left: &ChoiceAction, right: &ChoiceAction) -> bool {
  match (left, right) {
    (ChoiceAction::Audio(a), ChoiceAction::Audio(b)) => a == b,
    (ChoiceAction::Subtitle(a), ChoiceAction::Subtitle(b)) => a == b,
    (ChoiceAction::Queue(a), ChoiceAction::Queue(b)) => a.id == b.id,
    (ChoiceAction::Speed(a), ChoiceAction::Speed(b)) => a == b,
    (ChoiceAction::SessionSkip, ChoiceAction::SessionSkip)
    | (ChoiceAction::Close, ChoiceAction::Close) => true,
    _ => false,
  }
}

fn refresh_panel_choices(state: &mut State) {
  let Some(panel) = state.tv.player.remote.panel() else {
    return;
  };
  let revision = choices_revision(state, panel.kind);
  if revision == state.tv.player.choices_revision {
    return;
  }
  let choices = panel_choices(state, panel.kind);
  let focused = state
    .tv
    .player
    .choices
    .get(panel.focused)
    .and_then(|previous| {
      choices
        .iter()
        .position(|choice| same_choice(&previous.action, &choice.action))
    })
    .unwrap_or(panel.focused.min(choices.len().saturating_sub(1)));
  state
    .tv
    .player
    .remote
    .replace_choices(focused, choices.len());
  state.tv.player.choices_revision = revision;
  state.tv.player.choices = choices;
}

fn panel_choices(state: &State, panel: Panel) -> Vec<Choice> {
  let mut choices = Vec::new();
  match panel {
    Panel::Audio | Panel::Subtitles => {
      if panel == Panel::Subtitles {
        choices.push(Choice {
          label: state.t("tv-player-subtitles-off"),
          action: ChoiceAction::Subtitle(None),
        });
      }
      if let TracksView::Ready { tracks, .. } = &state.playback.view.tracks {
        for track in tracks.iter().filter(|track| {
          track.track_type
            == if panel == Panel::Audio {
              "audio"
            } else {
              "sub"
            }
        }) {
          choices.push(Choice {
            label: track_label(state, track),
            action: if panel == Panel::Audio {
              ChoiceAction::Audio(track.id)
            } else {
              ChoiceAction::Subtitle(Some(track.id))
            },
          });
        }
      }
    }
    Panel::Queue => {
      if let QueueState::Ready(items) = &state.playback.queue {
        choices.extend(items.iter().map(|item| Choice {
          label: item.name.clone(),
          action: ChoiceAction::Queue(Box::new(item.clone())),
        }));
      }
    }
    Panel::Settings => {
      choices.extend([0.75, 1.0, 1.25, 1.5, 2.0].into_iter().map(|speed| Choice {
        label: format!("{speed}×"),
        action: ChoiceAction::Speed(speed),
      }));
      choices.push(Choice {
        label: state.t("player-auto-skip-intro-credits"),
        action: ChoiceAction::SessionSkip,
      });
    }
    Panel::Information => {}
  }
  choices.push(Choice {
    label: state.t("common-close"),
    action: ChoiceAction::Close,
  });
  choices
}

fn synchronize_panel(state: &mut State, previous: Option<Panel>) -> Task<AppMessage> {
  if state.tv.player.remote.panel().is_none() {
    state.tv.player.choices.clear();
    if previous == Some(Panel::Information) {
      return embedded_player::update(state, embedded_player::Message::InformationDismissed);
    }
  }
  Task::none()
}

fn apply_choice(state: &mut State, index: usize) -> Task<AppMessage> {
  let Some(action) = state
    .tv
    .player
    .choices
    .get(index)
    .map(|choice| choice.action.clone())
  else {
    return Task::none();
  };
  if (!matches!(action, ChoiceAction::Close) && observation(state).busy)
    || !choice_enabled(state, &action)
  {
    return Task::none();
  }
  let task = match action {
    ChoiceAction::Audio(id) => command(
      state,
      AppMessage::Playback(PlaybackMessage::AudioTrackSelected(id)),
    ),
    ChoiceAction::Subtitle(id) => command(
      state,
      AppMessage::Playback(PlaybackMessage::SubtitleTrackSelected(id)),
    ),
    ChoiceAction::Queue(item) => command(
      state,
      AppMessage::Playback(PlaybackMessage::QueueItemSelected(item)),
    ),
    ChoiceAction::Speed(speed) => return dispatch(state, PlaybackIntent::SetSpeed(speed)),
    ChoiceAction::SessionSkip => {
      let automatic = state.playback.session.intro_mode() == IntroSkipMode::Automatic;
      return dispatch(
        state,
        PlaybackIntent::SetIntroMode(if automatic {
          IntroSkipMode::Manual
        } else {
          IntroSkipMode::Automatic
        }),
      );
    }
    ChoiceAction::Close => Task::none(),
  };
  let previous = state.tv.player.remote.panel().map(|panel| panel.kind);
  state.tv.player.remote.close_panel(Instant::now());
  Task::batch([task, synchronize_panel(state, previous)])
}

fn choice_enabled(state: &State, action: &ChoiceAction) -> bool {
  match action {
    ChoiceAction::Speed(_) => state
      .tv
      .player
      .controls
      .as_ref()
      .is_some_and(|controls| controls.speed.is_some()),
    ChoiceAction::SessionSkip => {
      !state.playback.view.intro_ranges.is_empty()
        && playback::series_intro_mode(&state.playback, &state.kernel).is_some()
    }
    ChoiceAction::Audio(id) | ChoiceAction::Subtitle(Some(id)) => {
      matches!(&state.playback.view.tracks, TracksView::Ready { tracks, .. } if tracks.iter().any(|track| track.id == *id))
    }
    ChoiceAction::Subtitle(None) => matches!(state.playback.view.tracks, TracksView::Ready { .. }),
    ChoiceAction::Queue(item) => {
      matches!(&state.playback.queue, QueueState::Ready(items) if items.iter().any(|candidate| candidate.id == item.id))
    }
    ChoiceAction::Close => true,
  }
}

fn choice_selected(state: &State, action: &ChoiceAction) -> bool {
  match action {
    ChoiceAction::Audio(id) => {
      matches!(&state.playback.view.tracks, TracksView::Ready { audio, .. } if *audio == Some(*id))
    }
    ChoiceAction::Subtitle(id) => {
      matches!(&state.playback.view.tracks, TracksView::Ready { subtitle, .. } if subtitle == id)
    }
    ChoiceAction::Queue(item) => state
      .playback
      .view
      .now_playing
      .as_ref()
      .is_some_and(|playing| playing.item.item_id == item.id),
    ChoiceAction::Speed(speed) => state
      .tv
      .player
      .controls
      .as_ref()
      .and_then(|controls| controls.speed)
      .is_some_and(|current| (current - speed).abs() < 0.001),
    ChoiceAction::SessionSkip => state.playback.session.intro_mode() == IntroSkipMode::Automatic,
    ChoiceAction::Close => false,
  }
}

fn track_label(state: &State, track: &TrackInfo) -> String {
  match (track.title.as_deref(), track.language.as_deref()) {
    (Some(title), Some(language)) if title != language => format!("{title} · {language}"),
    (Some(title), _) => title.to_owned(),
    (_, Some(language)) => language.to_owned(),
    _ => state.format("player-track", &[("number", track.id.into())]),
  }
}

pub fn view(state: &State) -> Element<'_, AppMessage> {
  responsive(move |bounds| -> Element<'_, AppMessage> {
    let scale = style::scale(bounds.width);
    let video: Element<'_, AppMessage> = if crate::embedded::enabled() {
      crate::embedded::view()
    } else {
      container(text(state.t("tv-player-external")).size(style::BODY * scale))
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .style(style::canvas)
        .into()
    };
    let picture = mouse_area(video)
      .on_move(|point| message(Message::PointerMoved(point)))
      .on_press(message(Message::PicturePressed))
      .interaction(
        if state.tv.player.remote.full() || state.tv.player.remote.panel().is_some() {
          iced::mouse::Interaction::Idle
        } else {
          iced::mouse::Interaction::Hidden
        },
      );
    let mut layers = stack![picture].width(Fill).height(Fill);
    if state.tv.player.remote.full() {
      layers = layers.push(
        container(space())
          .width(Fill)
          .height(220.0 * scale)
          .style(|_| cinema::scrim(true)),
      );
      layers = layers.push(
        container(
          container(space())
            .width(Fill)
            .height(420.0 * scale)
            .style(|_| cinema::scrim(false)),
        )
        .width(Fill)
        .height(Fill)
        .align_y(Alignment::End),
      );
      layers = layers.push(full_chrome(state, scale));
    }
    if state.playback.view.intro_prompt.is_some() && state.tv.player.remote.panel().is_none() {
      layers = layers.push(
        container(action_button(
          state,
          Control::Skip,
          Icon::IntroSkip,
          Some(state.t("player-skip")),
          scale,
        ))
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .padding(iced::Padding {
          top: 0.0,
          right: style::SAFE_X * scale,
          bottom: if state.tv.player.remote.full() {
            320.0 * scale
          } else {
            style::SAFE_Y * scale
          },
          left: 0.0,
        }),
      );
    }
    if let Some(panel) = state.tv.player.remote.panel() {
      return stack![
        inert(layers),
        opaque(panel_view(state, panel.kind, bounds, scale))
      ]
      .into();
    }
    layers.into()
  })
  .into()
}

fn full_chrome(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let title = state
    .playback
    .view
    .now_playing
    .as_ref()
    .map(|playing| playing.item.title.clone())
    .unwrap_or_else(|| state.t("player-waiting"));
  let top = row![
    action_button(
      state,
      Control::Back,
      Icon::ChevronLeft,
      Some(state.t("common-back")),
      scale
    ),
    ellipsis_text(title)
      .font(HEADING_FONT)
      .size(style::SECTION * scale)
      .color(style::PALETTE.text.heading)
  ]
  .spacing(style::GAP * scale)
  .align_y(Alignment::Center);
  let mut bottom = column![timeline(state, scale)].spacing(20.0 * scale);
  if let Some(notice) = state.playback.notice.as_ref() {
    bottom = bottom.push(
      text(state.kernel.locale.message(notice))
        .size(style::META * scale)
        .color(style::PALETTE.colors.error),
    );
  }
  let left = row![
    action_button(
      state,
      Control::Queue,
      Icon::Playlist,
      Some(state.t("player-queue")),
      scale
    ),
    action_button(
      state,
      Control::Information,
      Icon::Info,
      Some(state.t("player-information")),
      scale
    )
  ]
  .spacing(16.0 * scale);
  let paused = state
    .playback
    .view
    .now_playing
    .as_ref()
    .is_some_and(|playing| playing.paused);
  let center = row![
    action_button(state, Control::Previous, Icon::Previous, None, scale),
    action_button(
      state,
      Control::Backward,
      Icon::ChevronLeft,
      Some("10".into()),
      scale
    ),
    action_button(
      state,
      Control::PlayPause,
      if paused { Icon::Play } else { Icon::Pause },
      None,
      scale
    ),
    action_button(
      state,
      Control::Forward,
      Icon::ChevronRight,
      Some("10".into()),
      scale
    ),
    action_button(state, Control::Next, Icon::Next, None, scale)
  ]
  .spacing(24.0 * scale)
  .align_y(Alignment::Center);
  let right = row![
    action_button(
      state,
      Control::Audio,
      Icon::AudioTrack,
      Some(state.t("player-audio")),
      scale
    ),
    action_button(
      state,
      Control::Subtitles,
      Icon::Subtitles,
      Some(state.t("player-subtitles")),
      scale
    ),
    action_button(
      state,
      Control::Settings,
      Icon::Settings,
      Some(state.t("tv-player-settings")),
      scale
    )
  ]
  .spacing(16.0 * scale);
  let side_width = 552.0 * scale;
  bottom = bottom.push(
    row![
      container(left).width(side_width),
      space().width(Fill),
      center,
      space().width(Fill),
      container(right).width(side_width).align_x(Alignment::End)
    ]
    .align_y(Alignment::Center),
  );
  bottom = bottom.push(
    container(
      ellipsis_text(controls_hint(state).unwrap_or_default())
        .size(style::META * scale)
        .line_height(iced::Pixels(28.0 * scale))
        .color(style::PALETTE.text.body),
    )
    .width(Fill)
    .height(28.0 * scale)
    .center_x(Fill),
  );
  container(column![top, space().height(Fill), bottom])
    .width(Fill)
    .height(Fill)
    .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
    .into()
}

fn controls_hint(state: &State) -> Option<String> {
  if !state.tv.player.remote.full() || state.tv.player.remote.panel().is_some() {
    return None;
  }
  if state.tv.player.remote.seek().is_some() {
    return Some(state.t("tv-player-seek-hint"));
  }
  if observation(state).busy {
    return Some(state.t("tv-player-busy-hint"));
  }
  let focused = state.tv.player.remote.focused()?;
  let action = state.t(match focused {
    Control::Back => "tv-player-exit",
    Control::PlayPause if observation(state).paused => "tv-player-resume",
    Control::PlayPause => "tv-player-pause",
    Control::Previous => "tv-player-previous",
    Control::Next => "tv-player-next",
    Control::Backward => "tv-player-backward",
    Control::Forward => "tv-player-forward",
    Control::Queue => "player-queue",
    Control::Information => "player-information",
    Control::Audio => "player-audio",
    Control::Subtitles => "player-subtitles",
    Control::Settings => "tv-player-settings",
    Control::Skip => "player-skip",
  });
  Some(state.format(
    if focused == Control::PlayPause && seek_range(state).is_some() {
      "tv-player-primary-hint"
    } else {
      "tv-player-controls-hint"
    },
    &[("action", action.into())],
  ))
}

fn panel_hint(state: &State) -> String {
  let Some(choice) = state
    .tv
    .player
    .remote
    .panel()
    .and_then(|panel| state.tv.player.choices.get(panel.focused))
  else {
    return state.t("tv-player-panel-back-hint");
  };
  if !matches!(choice.action, ChoiceAction::Close)
    && (observation(state).busy || !choice_enabled(state, &choice.action))
  {
    return state.t("tv-player-panel-back-hint");
  }
  let action = match choice.action {
    ChoiceAction::Audio(_) => state.t("tv-player-apply-audio"),
    ChoiceAction::Subtitle(_) => state.t("tv-player-apply-subtitles"),
    ChoiceAction::Queue(_) => state.t("tv-player-play-item"),
    ChoiceAction::Speed(_) => state.format(
      "tv-player-use-speed",
      &[("speed", choice.label.clone().into())],
    ),
    ChoiceAction::SessionSkip => state.t(if choice_selected(state, &choice.action) {
      "tv-player-disable-session-skip"
    } else {
      "tv-player-enable-session-skip"
    }),
    ChoiceAction::Close => return state.t("tv-player-panel-close-hint"),
  };
  state.format(
    if matches!(
      choice.action,
      ChoiceAction::Audio(_) | ChoiceAction::Subtitle(_)
    ) {
      "tv-player-panel-apply-hint"
    } else {
      "tv-player-panel-change-hint"
    },
    &[("action", action.into())],
  )
}

fn action_button(
  state: &State,
  control: Control,
  icon: Icon,
  label: Option<String>,
  scale: f32,
) -> Element<'_, AppMessage> {
  let focused = state.tv.player.remote.focused() == Some(control);
  let enabled = !observation(state).busy
    && (control == Control::Back || available_controls(state).contains(&control));
  let primary = control == Control::PlayPause;
  let round = matches!(
    control,
    Control::Previous | Control::Backward | Control::PlayPause | Control::Forward | Control::Next
  );
  let fixed = round || label.is_none();
  let view = focus(focused && enabled, move |progress| {
    let dimension = if primary {
      80.0
        + if jellypilot_ui::widgets::motion::enabled() {
          4.0 * progress
        } else {
          0.0
        }
    } else {
      style::CONTROL
    } * scale;
    let color = chrome::foreground(progress, primary, enabled);
    let content: Element<'_, AppMessage> =
      if matches!(control, Control::Backward | Control::Forward) {
        column![
          icon_with_color(icon, IconSize::Custom(24.0 * scale), color),
          text("10").font(BODY_FONT).size(18.0 * scale)
        ]
        .align_x(Alignment::Center)
        .into()
      } else if let Some(label) = label.as_ref() {
        row![
          icon_with_color(icon, IconSize::Custom(28.0 * scale), color),
          text(label.clone())
            .font(BODY_FONT)
            .size(style::BODY * scale)
        ]
        .spacing(12.0 * scale)
        .align_y(Alignment::Center)
        .into()
      } else {
        icon_with_color(icon, IconSize::Custom(32.0 * scale), color).into()
      };
    button(
      container(content)
        .width(if fixed { Fill } else { Length::Shrink })
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center),
    )
    .padding(if round { 0.0 } else { 16.0 * scale })
    .height(dimension)
    .width(if fixed {
      Length::Fixed(dimension)
    } else {
      Length::Shrink
    })
    .style(chrome::control(progress, primary, round))
    .on_press_maybe(enabled.then(|| message(Message::Activate(control))))
    .into()
  });
  if primary {
    view
      .frame(
        Size::new(80.0 * scale, 80.0 * scale),
        Alignment::Center,
        Alignment::Center,
      )
      .into()
  } else {
    view.into()
  }
}

fn timeline(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let playing = state.playback.view.now_playing.as_ref();
  let duration = state
    .tv
    .player
    .controls
    .as_ref()
    .and_then(|controls| controls.duration_seconds)
    .or_else(|| playing.and_then(|playing| playing.duration_seconds))
    .filter(|value| value.is_finite() && *value > 0.0);
  let current = playing.map_or(0.0, |playing| playing.position_seconds);
  let candidate = state.tv.player.remote.seek();
  let position = candidate.map_or(current, |seek| seek.candidate);
  let time = format!(
    "{} / {}",
    format_duration(position),
    duration
      .map(format_duration)
      .unwrap_or_else(|| state.t("common-unavailable"))
  );
  let buffered = state
    .tv
    .player
    .controls
    .as_ref()
    .map_or(&[][..], |controls| controls.buffered_ranges.as_slice());
  let bar: Element<'_, AppMessage> = focus(candidate.is_some(), move |progress| {
    chrome::timeline(position, duration.unwrap_or(0.0), buffered, progress, scale)
  })
  .into();
  let preview = responsive(move |bounds| -> Element<'_, AppMessage> {
    if candidate.is_none() {
      return space().width(Fill).height(48.0 * scale).into();
    }
    let width = 152.0 * scale;
    let fraction = duration.map_or(0.0, |duration| (position / duration).clamp(0.0, 1.0)) as f32;
    let offset =
      (fraction * bounds.width - width / 2.0).clamp(0.0, (bounds.width - width).max(0.0));
    row![
      space().width(offset),
      container(
        text(format_duration(position))
          .font(HEADING_FONT)
          .size(style::BODY * scale)
      )
      .width(width)
      .center_x(width)
    ]
    .height(48.0 * scale)
    .into()
  });
  let mut actions = row![];
  if candidate.is_some() {
    actions = actions
      .push(simple_button(
        state.t("tv-player-seek-confirm"),
        Message::ConfirmSeek,
        false,
        scale,
      ))
      .push(simple_button(
        state.t("common-cancel"),
        Message::CancelSeek,
        false,
        scale,
      ));
  } else if seek_range(state).is_none() {
    actions = actions.push(text(state.t("tv-player-seek-unavailable")).size(style::META * scale));
  }
  let content = column![
    container(preview).height(48.0 * scale),
    bar,
    row![
      text(time).size(style::BODY * scale),
      space().width(Fill),
      actions.spacing(16.0 * scale).align_y(Alignment::Center)
    ]
    .height(style::CONTROL * scale)
    .align_y(Alignment::Center)
  ];
  let track: Element<'_, AppMessage> = content.into();
  if candidate.is_none() && seek_range(state).is_some() {
    mouse_area(track).on_press(message(Message::Seek)).into()
  } else {
    track
  }
}

fn simple_button(
  label: String,
  event: Message,
  focused: bool,
  scale: f32,
) -> Element<'static, AppMessage> {
  focus(focused, move |progress| {
    button(text(label.clone()).size(style::BODY * scale))
      .padding([12.0 * scale, 20.0 * scale])
      .height(style::CONTROL * scale)
      .style(style::button_progress(style::PALETTE, progress, false))
      .on_press(message(event.clone()))
      .into()
  })
  .into()
}

fn panel_view(
  state: &State,
  panel: Panel,
  bounds: iced::Size,
  scale: f32,
) -> Element<'_, AppMessage> {
  let title = match panel {
    Panel::Queue => "player-queue",
    Panel::Audio => "player-audio",
    Panel::Subtitles => "player-subtitles",
    Panel::Information => "player-information",
    Panel::Settings => "tv-player-settings",
  };
  let focused = state
    .tv
    .player
    .remote
    .panel()
    .map_or(0, |panel| panel.focused);
  let title = text(state.t(title))
    .font(HEADING_FONT)
    .size(style::TITLE * scale)
    .line_height(iced::Pixels(48.0 * scale));
  let close = state
    .tv
    .player
    .choices
    .iter()
    .enumerate()
    .find(|(_, choice)| matches!(choice.action, ChoiceAction::Close));
  let heading: Element<'_, AppMessage> = if panel == Panel::Settings {
    let mut heading = row![title, space().width(Fill)].align_y(Alignment::Center);
    if let Some((index, _)) = close {
      heading = heading.push(panel_close_button(index, focused, scale));
    }
    heading.into()
  } else {
    title.into()
  };
  let mut content = column![heading].spacing(24.0 * scale);
  if panel == Panel::Information {
    content = content.push(information(state, scale));
  }
  if panel == Panel::Settings {
    content = content.push(
      text(state.t("tv-player-speed"))
        .size(style::BODY * scale)
        .line_height(iced::Pixels(32.0 * scale)),
    );
    let speed = state
      .tv
      .player
      .choices
      .iter()
      .take(5)
      .enumerate()
      .fold(row![], |row, (index, choice)| {
        row.push(choice_button(state, index, choice, focused, scale))
      });
    content = content.push(speed.spacing(12.0 * scale));
    for (index, choice) in state
      .tv
      .player
      .choices
      .iter()
      .enumerate()
      .skip(5)
      .filter(|(_, choice)| !matches!(choice.action, ChoiceAction::Close))
    {
      content = content.push(choice_button(state, index, choice, focused, scale));
    }
    content = content.push(
      text(
        state.t(if choice_enabled(state, &ChoiceAction::SessionSkip) {
          "tv-player-session-skip-hint"
        } else {
          "tv-player-session-skip-unavailable"
        }),
      )
      .size(style::META * scale)
      .line_height(iced::Pixels(28.0 * scale))
      .color(style::PALETTE.text.body),
    );
  } else {
    let room = ((bounds.height - (2.0 * style::SAFE_Y + 360.0) * scale) / (104.0 * scale))
      .floor()
      .max(1.0) as usize;
    let start = focused.saturating_sub(room.saturating_sub(1)).min(
      state
        .tv
        .player
        .choices
        .len()
        .saturating_sub(1)
        .saturating_sub(room),
    );
    for (index, choice) in state
      .tv
      .player
      .choices
      .iter()
      .enumerate()
      .filter(|(_, choice)| !matches!(choice.action, ChoiceAction::Close))
      .skip(start)
      .take(room)
    {
      content = content.push(choice_button(state, index, choice, focused, scale));
    }
    let status = match panel {
      Panel::Queue => match &state.playback.queue {
        QueueState::Loading => Some("player-loading-episodes"),
        QueueState::Unavailable | QueueState::Failed => Some("player-unavailable-queue"),
        QueueState::Ready(items) if items.is_empty() => Some("player-no-episodes"),
        _ => None,
      },
      Panel::Audio | Panel::Subtitles => match &state.playback.view.tracks {
        TracksView::Loading => Some(if panel == Panel::Audio {
          "player-loading-audio"
        } else {
          "player-loading-subtitles"
        }),
        TracksView::Unavailable => Some("tv-player-tracks-unavailable"),
        _ => None,
      },
      _ => None,
    };
    if let Some(status) = status {
      content = content.push(text(state.t(status)).size(style::META * scale));
    }
  }
  if panel != Panel::Settings {
    content = content.push(space().height(Fill));
    if let Some((index, choice)) = close {
      content = content.push(choice_button(state, index, choice, focused, scale));
    }
  }
  content = content.push(
    ellipsis_text(panel_hint(state))
      .size(style::META * scale)
      .line_height(iced::Pixels(28.0 * scale))
      .color(style::PALETTE.text.body),
  );
  let max_height = (bounds.height - 2.0 * style::SAFE_Y * scale).max(1.0);
  let height = if panel == Panel::Settings {
    Length::Fit.max(max_height)
  } else {
    Length::Fixed(max_height)
  };
  let content: Element<'_, AppMessage> = if panel == Panel::Settings {
    scrollable(content)
      .height(Length::Fit.max((max_height - 80.0 * scale).max(1.0)))
      .into()
  } else {
    content.into()
  };
  let panel = container(content)
    .id("tv-player-panel")
    .padding(40.0 * scale)
    .width(744.0 * scale)
    .height(height)
    .style(chrome::panel);
  container(panel)
    .width(Fill)
    .height(Fill)
    .align_x(Alignment::End)
    .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
    .into()
}

fn panel_close_button(index: usize, focused: usize, scale: f32) -> Element<'static, AppMessage> {
  container(focus(index == focused, move |progress| {
    button(
      container(icon_with_color(
        Icon::Close,
        IconSize::Custom(28.0 * scale),
        style::foreground(style::PALETTE, progress, false),
      ))
      .center_x(Fill)
      .center_y(Fill),
    )
    .padding(0)
    .width(style::CONTROL * scale)
    .height(style::CONTROL * scale)
    .style(chrome::choice(progress, false, false))
    .on_press(message(Message::Choice(index)))
    .into()
  }))
  .id("tv-player-panel-close")
  .into()
}

fn choice_button<'a>(
  state: &'a State,
  index: usize,
  choice: &'a Choice,
  focused: usize,
  scale: f32,
) -> Element<'a, AppMessage> {
  let selected = choice_selected(state, &choice.action);
  let enabled = matches!(choice.action, ChoiceAction::Close)
    || (choice_enabled(state, &choice.action) && !observation(state).busy);
  let compact = matches!(choice.action, ChoiceAction::Speed(_));
  let close = matches!(choice.action, ChoiceAction::Close);
  let toggle = matches!(choice.action, ChoiceAction::SessionSkip);
  focus(index == focused && enabled, move |progress| {
    let color = if enabled {
      style::foreground(style::PALETTE, progress, selected)
    } else {
      style::PALETTE.text.muted
    };
    let label = ellipsis_text(choice.label.clone())
      .size(style::BODY * scale)
      .color(color);
    let content: Element<'_, AppMessage> = if toggle {
      row![
        column![
          label,
          text(state.t("tv-player-session-only"))
            .size(style::META * scale)
            .color(chrome::supporting(progress))
        ]
        .spacing(4.0 * scale)
        .width(Fill),
        text(state.t(if selected {
          "tv-player-on"
        } else {
          "common-off"
        }))
        .size(style::BODY * scale)
        .color(color),
        chrome::session_switch(selected, scale)
      ]
      .spacing(16.0 * scale)
      .align_y(Alignment::Center)
      .into()
    } else if compact || close {
      container(label)
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .into()
    } else {
      row![
        icon_with_color(
          if selected {
            Icon::CircleDot
          } else {
            Icon::Circle
          },
          IconSize::Custom(28.0 * scale),
          color
        ),
        label
      ]
      .align_y(Alignment::Center)
      .spacing(20.0 * scale)
      .into()
    };
    button(content)
      .padding([12.0 * scale, 20.0 * scale])
      .height(
        if compact || close {
          64.0
        } else if toggle {
          88.0
        } else {
          80.0
        } * scale,
      )
      .width(if close {
        Length::Fixed(152.0 * scale)
      } else {
        Fill
      })
      .style(chrome::choice(progress, selected, compact))
      .on_press_maybe(enabled.then(|| message(Message::Choice(index))))
      .into()
  })
  .into()
}

fn information(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let Some(info) = embedded_player::information(state) else {
    return text(state.t(if embedded_player::information_failed(state) {
      "player-information-unavailable"
    } else {
      "player-information-loading"
    }))
    .size(style::META * scale)
    .into();
  };
  let mut rows = Column::new().spacing(0);
  if let Some(filename) = info.filename.as_ref() {
    rows = rows.push(
      container(ellipsis_text(filename.clone()).size(style::BODY * scale))
        .width(Fill)
        .padding(iced::Padding {
          bottom: 24.0 * scale,
          ..iced::Padding::ZERO
        }),
    );
  }
  let video = info.video.as_ref();
  let codec = video.and_then(|video| video.codec.clone());
  let resolution = video
    .and_then(|video| video.width.zip(video.height))
    .map(|(width, height)| format!("{width} × {height}"));
  let fps = video
    .and_then(|video| video.container_fps)
    .map(|fps| format!("{fps:.2} fps"));
  let bitrate = video
    .and_then(|video| video.bitrate)
    .map(|bitrate| format!("{:.1} Mbps", bitrate as f64 / 1_000_000.0));
  let audio = info
    .audio
    .as_ref()
    .map(|audio| {
      [
        audio.language.as_deref(),
        audio.codec.as_deref(),
        audio.channels.as_deref(),
      ]
      .into_iter()
      .flatten()
      .collect::<Vec<_>>()
      .join(" · ")
    })
    .filter(|value| !value.is_empty());
  let subtitles = match &state.playback.view.tracks {
    TracksView::Ready {
      tracks,
      subtitle: Some(id),
      ..
    } => tracks
      .iter()
      .find(|track| track.id == *id)
      .map(|track| track_label(state, track)),
    TracksView::Ready { subtitle: None, .. } => Some(state.t("tv-player-subtitles-off")),
    _ => None,
  };
  for (label, value) in [
    ("player-video", codec),
    ("tv-player-resolution", resolution),
    ("tv-player-frame-rate", fps),
    ("player-information-bitrate", bitrate),
    ("player-audio", audio),
    ("player-subtitles", subtitles),
  ] {
    rows = rows.push(info_row(
      state.t(label),
      value.unwrap_or_else(|| state.t("common-unavailable")),
      scale,
    ));
  }
  rows.into()
}

fn info_row(label: String, value: String, scale: f32) -> Element<'static, AppMessage> {
  row![
    text(label)
      .size(style::META * scale)
      .color(style::PALETTE.text.body),
    container(
      ellipsis_text(value)
        .size(style::BODY * scale)
        .color(style::PALETTE.text.heading)
    )
    .width(Fill)
    .align_x(Alignment::End)
  ]
  .height(64.0 * scale)
  .spacing(24.0 * scale)
  .align_y(Alignment::Center)
  .into()
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::futures::StreamExt;
  use jellypilot_core::config::UiMode;
  use jellypilot_mpv::playback::NowPlayingItem;
  use jellypilot_mpv::playback_session::NowPlayingView;

  async fn dispatched(task: Task<AppMessage>) -> Message {
    let mut stream = iced_runtime::task::into_stream(task).expect("deferred TV command");
    let Some(iced_runtime::Action::Output(AppMessage::Tv(super::super::Message::Player(message)))) =
      stream.next().await
    else {
      panic!("TV command envelope")
    };
    message
  }

  fn playing_state() -> State {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = UiMode::Tv;
    state.shell.window_id = Some(iced::window::Id::unique());
    state.shell.destination = Destination::NowPlaying;
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.playback.view.lifecycle.playback_active = true;
    state.playback.view.now_playing = Some(NowPlayingView {
      item: NowPlayingItem {
        item_id: "movie".into(),
        title: "Movie".into(),
        item_type: "Movie".into(),
        series_id: None,
        runtime_seconds: Some(600.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".into(),
        original_language: None,
      },
      paused: true,
      position_seconds: 15.0,
      duration_seconds: Some(600.0),
      volume: 80.0,
      muted: false,
    });
    drop(reconcile(&mut state));
    state
  }

  #[tokio::test]
  async fn settings_panel_fits_content_and_header_close_remains_reachable() {
    use iced::advanced::{layout, renderer, renderer::Headless, shell, widget, Layout, Shell};
    use jellypilot_core::locale::UiLanguage;

    #[derive(Default)]
    struct Bounds {
      panel: Option<iced::Rectangle>,
      close: Option<iced::Rectangle>,
    }

    impl widget::Operation for Bounds {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
        if id == Some(&widget::Id::new("tv-player-panel")) {
          self.panel = Some(bounds);
        }
        if id == Some(&widget::Id::new("tv-player-panel-close")) {
          self.close = Some(bounds);
        }
      }
    }

    let renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless layout renderer");
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      for viewport in [
        Size::new(1920.0, 1080.0),
        Size::new(1280.0, 720.0),
        Size::new(1920.0, 420.0),
      ] {
        let mut state = playing_state();
        state.kernel.locale = crate::i18n::Localizer::new(language);
        drop(open_panel(&mut state, Panel::Settings));
        state.playback.view.busy = true;
        let scale = style::scale(viewport.width);
        let mut page = panel_view(&state, Panel::Settings, viewport, scale);
        let mut tree = widget::Tree::new(&page);
        tree.diff(page.as_widget_mut());
        let node = page.as_widget_mut().layout(
          &mut tree,
          &renderer,
          &layout::Limits::new(Size::ZERO, viewport),
        );
        let mut bounds = Bounds::default();
        page
          .as_widget_mut()
          .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        let panel = bounds.panel.expect("settings panel bounds");
        let close = bounds.close.expect("header close bounds");
        let available = viewport.height - 2.0 * style::SAFE_Y * scale;
        assert!(
          panel.height <= available + 0.1,
          "localized settings exceeded the safe viewport: {panel:?}"
        );
        if viewport.height >= 720.0 {
          assert!(
            panel.height < available * 0.7,
            "settings stretched instead of fitting its content: {panel:?}"
          );
        }
        assert!((panel.y - style::SAFE_Y * scale).abs() < 0.1);
        assert!((panel.x + panel.width - (viewport.width - style::SAFE_X * scale)).abs() < 0.1);
        assert!(
          close.height >= style::CONTROL * scale - 0.1
            && close.width >= style::CONTROL * scale - 0.1
        );
        assert!(
          panel.contains(close.center()),
          "close must remain visible when the panel is capped"
        );
        let mut bus = shell::Bus::new();
        for event in [
          iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
          iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
        ] {
          page.as_widget_mut().update(
            &mut tree,
            &iced::Event::Mouse(event),
            Layout::new(&node),
            iced::mouse::Cursor::Available(close.center()),
            &renderer,
            &mut Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus),
            &iced::Rectangle::with_size(viewport),
          );
        }
        let mut messages = bus.into_iter();
        let Some(AppMessage::Tv(super::super::Message::Player(close))) = messages.next() else {
          panic!("header close publishes its action");
        };
        assert!(messages.next().is_none(), "closing must publish only once");
        drop(page);
        drop(update(&mut state, close));
        assert!(state.tv.player.remote.panel().is_none());
        assert_eq!(state.tv.player.remote.focused(), Some(Control::PlayPause));
        assert!(state.playback.view.now_playing.as_ref().unwrap().paused);
      }
    }
  }

  #[test]
  fn contextual_hints_follow_active_layer_and_paused_seek_cancel() {
    let mut state = playing_state();
    state.tv.player.controls = Some(Box::new(PlaybackControls {
      seekable: Some(true),
      partially_seekable: Some(false),
      duration_seconds: Some(600.0),
      speed: Some(1.0),
      ..PlaybackControls::default()
    }));
    let paused = controls_hint(&state).expect("paused main action");
    drop(update(&mut state, Message::Seek));
    let seeking = controls_hint(&state).expect("seek confirm and cancel guidance");
    assert_ne!(seeking, paused);
    drop(input(&mut state, Input::Right));
    drop(input(&mut state, Input::Back));
    assert_eq!(controls_hint(&state), Some(paused.clone()));
    assert!(state.playback.view.now_playing.as_ref().unwrap().paused);

    drop(update(&mut state, Message::Activate(Control::Settings)));
    assert!(
      controls_hint(&state).is_none(),
      "background transport must not compete with panel guidance"
    );
    state.tv.player.remote.focus_choice(2);
    let speed = panel_hint(&state);
    let close = state
      .tv
      .player
      .choices
      .iter()
      .position(|choice| matches!(choice.action, ChoiceAction::Close))
      .unwrap();
    state.tv.player.remote.focus_choice(close);
    assert_ne!(panel_hint(&state), speed);
    drop(input(&mut state, Input::Confirm));
    assert_eq!(state.tv.player.remote.focused(), Some(Control::Settings));
    assert_ne!(controls_hint(&state), Some(paused.clone()));
    drop(input(&mut state, Input::Back));
    assert!(
      controls_hint(&state).is_none(),
      "pure playback has no permanent hint"
    );
    drop(input(&mut state, Input::Confirm));
    assert_eq!(controls_hint(&state), Some(paused));
    assert!(state.playback.view.now_playing.as_ref().unwrap().paused);
  }

  #[test]
  fn leaving_tv_retires_in_flight_reads_and_reentry_accepts_only_its_new_sample() {
    let mut state = playing_state();
    let generation = state.playback.view.lifecycle.replacement_generation;
    let old = Instant::now();
    state.tv.player.sampling = Some(old);
    state
      .tv
      .player
      .remote
      .open_panel(Panel::Subtitles, 1, 2, old);
    drop(crate::app::shell::apply_ui_mode(
      &mut state,
      UiMode::Desktop,
    ));
    assert!(state.tv.player.sampling.is_none());
    assert!(state.tv.player.remote.panel().is_none());
    drop(update(
      &mut state,
      Message::ControlsSampled {
        generation,
        token: old,
        sample: Some(Box::new(PlaybackControls {
          speed: Some(2.0),
          ..PlaybackControls::default()
        })),
      },
    ));
    drop(crate::app::shell::apply_ui_mode(&mut state, UiMode::Tv));
    drop(reconcile(&mut state));
    assert!(state.tv.player.sampling.is_none());
    let fresh = old + Duration::from_secs(1);
    state.tv.player.sampling = Some(fresh);
    drop(update(
      &mut state,
      Message::ControlsSampled {
        generation,
        token: old,
        sample: Some(Box::new(PlaybackControls {
          speed: Some(2.0),
          ..PlaybackControls::default()
        })),
      },
    ));
    assert_eq!(state.tv.player.sampling, Some(fresh));
    assert!(state.tv.player.controls.is_none());
    drop(update(
      &mut state,
      Message::ControlsSampled {
        generation,
        token: fresh,
        sample: Some(Box::new(PlaybackControls {
          speed: Some(1.0),
          duration_seconds: Some(90.0),
          seekable: Some(true),
          partially_seekable: Some(false),
          ..PlaybackControls::default()
        })),
      },
    ));
    assert!(state.tv.player.sampling.is_none());
    assert_eq!(seek_range(&state), Some((0.0, 90.0)));
  }

  #[test]
  fn preview_uses_confirmed_seekability_and_never_server_runtime_or_cache_gaps() {
    let mut state = playing_state();
    assert_eq!(seek_range(&state), None);
    state.tv.player.controls = Some(Box::new(PlaybackControls {
      seekable: Some(true),
      partially_seekable: Some(false),
      duration_seconds: Some(60.0),
      ..PlaybackControls::default()
    }));
    assert_eq!(seek_range(&state), Some((0.0, 60.0)));
    state
      .tv
      .player
      .controls
      .as_mut()
      .unwrap()
      .partially_seekable = Some(true);
    state.tv.player.controls.as_mut().unwrap().buffered_ranges = vec![(10.0, 20.0), (40.0, 50.0)];
    assert_eq!(seek_range(&state), Some((10.0, 20.0)));
    state
      .playback
      .view
      .now_playing
      .as_mut()
      .unwrap()
      .position_seconds = 30.0;
    assert_eq!(seek_range(&state), None);
    state.tv.player.controls.as_mut().unwrap().seekable = Some(false);
    state
      .playback
      .view
      .now_playing
      .as_mut()
      .unwrap()
      .position_seconds = 15.0;
    assert_eq!(seek_range(&state), None);
  }

  #[tokio::test]
  async fn deferred_commands_cannot_cross_media_or_account_replacement() {
    let mut state = playing_state();
    let old = dispatched(dispatch(
      &state,
      PlaybackIntent::SetIntroMode(IntroSkipMode::Automatic),
    ))
    .await;
    state.playback.view.lifecycle.replacement_generation += 1;
    drop(update(&mut state, old));
    assert_eq!(state.playback.session.intro_mode(), IntroSkipMode::Off);

    let fresh = dispatched(dispatch(
      &state,
      PlaybackIntent::SetIntroMode(IntroSkipMode::Automatic),
    ))
    .await;
    drop(update(&mut state, fresh));
    assert_eq!(
      state.playback.session.intro_mode(),
      IntroSkipMode::Automatic
    );

    // The account token also fences a command when two account sessions happen
    // to project the same controller generation.
    state.playback.view.lifecycle.playback_active = true;
    let old_account = dispatched(dispatch(
      &state,
      PlaybackIntent::SetIntroMode(IntroSkipMode::Manual),
    ))
    .await;
    state.kernel.request_gate.disconnect();
    drop(update(&mut state, old_account));
    assert_eq!(
      state.playback.session.intro_mode(),
      IntroSkipMode::Automatic
    );
  }

  #[test]
  fn session_skip_choice_survives_presentation_recreation_without_saving_a_preference() {
    let mut state = playing_state();
    let saved = state.kernel.settings.snapshot().intro_mode();
    state.playback.session.handle(
      jellypilot_mpv::playback_session::PlaybackInput::Intent(Box::new(
        PlaybackIntent::SetIntroMode(IntroSkipMode::Manual),
      )),
      Instant::now(),
    );
    drop(crate::app::shell::apply_ui_mode(
      &mut state,
      UiMode::Desktop,
    ));
    drop(crate::app::shell::apply_ui_mode(&mut state, UiMode::Tv));
    drop(reconcile(&mut state));
    assert!(!choice_selected(&state, &ChoiceAction::SessionSkip));
    assert_eq!(state.playback.session.intro_mode(), IntroSkipMode::Manual);
    assert_eq!(state.kernel.settings.snapshot().intro_mode(), saved);
  }

  #[tokio::test]
  async fn deferred_player_command_is_retired_when_a_tv_page_takes_input() {
    for settings in [false, true] {
      let mut state = playing_state();
      let pending = dispatched(dispatch(
        &state,
        PlaybackIntent::SetIntroMode(IntroSkipMode::Automatic),
      ))
      .await;
      if settings {
        state.tv.settings.open = true;
      } else {
        state.tv.search.open = true;
      }
      drop(update(&mut state, pending));
      assert_eq!(state.playback.session.intro_mode(), IntroSkipMode::Off);
    }
  }

  #[test]
  fn arriving_tracks_preserve_choice_identity_and_busy_close_restores_the_source() {
    let mut state = playing_state();
    state.playback.view.tracks = TracksView::Loading;
    drop(update(&mut state, Message::Activate(Control::Audio)));
    assert!(matches!(
      state.tv.player.choices[0].action,
      ChoiceAction::Close
    ));
    let track = |id| TrackInfo {
      id,
      track_type: "audio".into(),
      title: Some(format!("Audio {id}")),
      language: None,
      selected: id == 1,
      provider_index: None,
    };
    state.playback.view.tracks = TracksView::Ready {
      tracks: vec![track(1), track(2)],
      audio: Some(1),
      subtitle: None,
    };
    drop(reconcile(&mut state));
    let focused = state.tv.player.remote.panel().unwrap().focused;
    assert!(matches!(
      state.tv.player.choices[focused].action,
      ChoiceAction::Close
    ));
    assert_eq!(state.tv.player.choices.len(), 3);
    state.tv.player.remote.focus_choice(1);
    if let TracksView::Ready { tracks, .. } = &mut state.playback.view.tracks {
      tracks.insert(0, track(3));
    }
    drop(reconcile(&mut state));
    let focused = state.tv.player.remote.panel().unwrap().focused;
    assert!(matches!(
      state.tv.player.choices[focused].action,
      ChoiceAction::Audio(2)
    ));
    assert!(matches!(
      state.playback.view.tracks,
      TracksView::Ready { audio: Some(1), .. }
    ));
    state.playback.view.busy = true;
    let close = state.tv.player.choices.len() - 1;
    drop(update(&mut state, Message::Choice(close)));
    assert!(state.tv.player.remote.panel().is_none());
    assert_eq!(state.tv.player.remote.focused(), Some(Control::Audio));
    assert!(state.playback.view.now_playing.as_ref().unwrap().paused);
  }

  #[test]
  fn queue_arrival_keeps_minimal_origin_and_can_close_during_transport_work() {
    let mut state = playing_state();
    state.playback.queue = QueueState::Loading;
    let playing = observation(&state);
    let controls = available_controls(&state);
    state
      .tv
      .player
      .remote
      .input(Input::Back, playing, &controls, Instant::now());
    drop(open_panel(&mut state, Panel::Queue));
    state.playback.queue = QueueState::Ready(vec![super::super::view::tests::item("episode")]);
    drop(reconcile(&mut state));
    assert_eq!(state.tv.player.choices.len(), 2);
    let focused = state.tv.player.remote.panel().unwrap().focused;
    assert!(matches!(
      state.tv.player.choices[focused].action,
      ChoiceAction::Close
    ));
    state.playback.view.busy = true;
    drop(input(&mut state, Input::Confirm));
    assert!(state.tv.player.remote.panel().is_none());
    assert!(!state.tv.player.remote.full());
  }
}
