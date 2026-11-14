//! One auxiliary presentation window for the existing embedded playback session.

use std::time::{Duration, Instant};

use iced::{event, keyboard, window, Event, Subscription, Task};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_mpv::playback::PlaybackError;
use jellypilot_mpv::playback_session::PlaybackIntent;

use super::message::{Message as AppMessage, PlaybackMessage, WindowMessage};
use super::state::{Destination, State};
use super::{playback, shell};
use crate::embedded::{PresentationStatus, PresentationWindow};
use crate::i18n::UiText;

#[derive(Clone)]
pub enum Message {
  Enter,
  Return,
  Close,
  PointerMoved,
  PointerLeft,
  FocusChanged(bool),
  Drag,
  Resize(window::Direction),
  TogglePaused,
  FocusNext(bool),
  Bound {
    id: window::Id,
    generation: u64,
    message: Box<Message>,
  },
  Opened {
    id: window::Id,
    generation: u64,
  },
  Poll {
    id: window::Id,
    generation: u64,
  },
  Ready {
    id: window::Id,
    generation: u64,
    window: Option<PresentationWindow>,
  },
  SourceReady {
    id: window::Id,
    generation: u64,
    target: PresentationWindow,
    source: Option<PresentationWindow>,
  },
  Presented {
    id: window::Id,
    generation: u64,
    window: PresentationWindow,
  },
  TimedOut {
    generation: u64,
  },
  PauseSettled {
    id: window::Id,
    generation: u64,
    result: Result<(), PlaybackError>,
  },
  HideControls {
    generation: u64,
    deadline: Instant,
  },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
  Opening,
  Active,
  Returning,
  Closing,
}

#[derive(Default)]
pub struct Surface {
  id: Option<window::Id>,
  phase: Option<Phase>,
  generation: u64,
  session: Option<SessionToken>,
  media_generation: u64,
  selected: bool,
  main_presenting: bool,
  source: Option<PresentationWindow>,
  native: Option<PresentationWindow>,
  tv_return: Option<bool>,
  restore_fullscreen: bool,
  target: Option<window::Id>,
  return_pending: Option<window::Id>,
  querying: bool,
  controls_deadline: Option<Instant>,
  focus_visibility: jellypilot_ui::widgets::control_button::FocusVisibility,
  controls_focus: bool,
  notice: Option<UiText>,
}

impl Surface {
  pub(super) fn keeps_alive(&self) -> bool {
    self.id.is_some() && self.selected
  }
}

pub fn window_id(state: &State) -> Option<window::Id> {
  state.shell.pip.id
}

pub fn active(state: &State) -> bool {
  state.shell.pip.selected && !state.shell.pip.main_presenting
}

pub fn available(state: &State) -> bool {
  cfg!(any(target_os = "linux", target_os = "windows"))
    && crate::embedded::enabled()
    && !state.tv_mode()
    && !state.shell.quit_requested
    && state.shell.pending_close.is_none()
    && state.shell.window_id.is_some()
    && state.shell.pip.id.is_none()
    && state.playback.view.now_playing.is_some()
    && !super::accounts::content_mutations_blocked(&state.kernel)
}

pub fn controls_enabled(state: &State) -> bool {
  state.shell.pip.phase == Some(Phase::Active)
    && !state.shell.quit_requested
    && !super::accounts::content_mutations_blocked(&state.kernel)
}

pub fn controls_visible(state: &State) -> bool {
  let surface = &state.shell.pip;
  !controls_enabled(state)
    || surface.controls_focus
    || surface.controls_deadline.is_some()
    || surface.notice.is_some()
    || state
      .playback
      .view
      .now_playing
      .as_ref()
      .is_some_and(|playing| playing.paused)
}

pub fn notice(state: &State) -> Option<&UiText> {
  state.shell.pip.notice.as_ref()
}

pub fn focus_visibility(state: &State) -> jellypilot_ui::widgets::control_button::FocusVisibility {
  state.shell.pip.focus_visibility.clone()
}

pub fn bind_current(state: &State, action: Message) -> AppMessage {
  match window_id(state) {
    Some(id) => bind(id, generation(state), message(action)),
    None => message(action),
  }
}

pub(super) fn transitioning(state: &State) -> bool {
  matches!(
    state.shell.pip.phase,
    Some(Phase::Closing | Phase::Returning)
  )
}

pub(super) fn admit_playback(state: &mut State) -> Task<AppMessage> {
  if state.shell.pip.phase == Some(Phase::Closing) {
    state.shell.pip.generation = state.shell.pip.generation.wrapping_add(1);
    state.shell.pip.phase = Some(Phase::Active);
    state.shell.pip.notice = None;
  }
  playback::window_opened(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
  )
  .unwrap_or_else(Task::none)
}

/// A TV request becomes effective only after the main window has the video.
pub(super) fn return_for_tv(state: &mut State, persist: bool) -> Task<AppMessage> {
  state.shell.pip.tv_return = Some(persist);
  if state.shell.pip.phase == Some(Phase::Opening) && !state.shell.pip.selected {
    if let Some(id) = state.shell.window_id {
      return finish_return(state, id);
    }
  }
  update(state, Message::Return)
}

pub(super) fn generation(state: &State) -> u64 {
  state.shell.pip.generation
}

pub(super) fn bind(id: window::Id, generation: u64, message: AppMessage) -> AppMessage {
  match message {
    AppMessage::Pip(message) => AppMessage::Pip(Message::Bound {
      id,
      generation,
      message: Box::new(message),
    }),
    message => message,
  }
}

fn current(state: &State, id: window::Id, generation: u64) -> bool {
  let surface = &state.shell.pip;
  surface.id == Some(id)
    && surface.generation == generation
    && surface.session == Some(state.kernel.request_gate.current_session())
}

fn message(message: Message) -> AppMessage {
  AppMessage::Pip(message)
}

fn timeout(generation: u64) -> Task<AppMessage> {
  Task::perform(
    async move {
      tokio::time::sleep(shell::WINDOW_OPEN_TIMEOUT).await;
      generation
    },
    |generation| message(Message::TimedOut { generation }),
  )
}

fn query(state: &mut State, id: window::Id) -> Task<AppMessage> {
  let surface = &mut state.shell.pip;
  surface.querying = true;
  surface.target = Some(id);
  let generation = surface.generation;
  crate::embedded::presentation_window(id).map(move |window| {
    message(Message::Ready {
      id,
      generation,
      window,
    })
  })
}

fn reveal(surface: &mut Surface) -> Task<AppMessage> {
  surface.controls_deadline = Some(Instant::now() + Duration::from_secs(3));
  Task::none()
}

fn poll_presentation(
  id: window::Id,
  generation: u64,
  window: PresentationWindow,
) -> Task<AppMessage> {
  Task::perform(
    async move {
      tokio::time::sleep(Duration::from_millis(40)).await;
      (id, generation, window)
    },
    |(id, generation, window)| {
      message(Message::Presented {
        id,
        generation,
        window,
      })
    },
  )
}

fn cancel_return_open(state: &mut State) -> Task<AppMessage> {
  state
    .shell
    .pip
    .return_pending
    .take()
    .map_or_else(Task::none, |id| {
      shell::cancel_pending_open(&mut state.shell, id)
    })
}

/// Retiring presentation never stops or reloads the media session.
pub(super) fn retire(state: &mut State) -> Task<AppMessage> {
  let cancel = cancel_return_open(state);
  let surface = &mut state.shell.pip;
  let id = surface.id.take();
  surface.generation = surface.generation.wrapping_add(1);
  surface.phase = None;
  surface.selected = false;
  surface.main_presenting = false;
  surface.native = None;
  surface.querying = false;
  surface.target = None;
  surface.controls_focus = false;
  surface.controls_deadline = None;
  surface.tv_return = None;
  if !crate::embedded::set_presentation_window(surface.source) {
    crate::embedded::set_presentation_window(None);
  }
  Task::batch([cancel, id.map_or_else(Task::none, window::close)])
}

fn last_window(state: &State) -> Task<AppMessage> {
  if state.shell.window_id.is_none()
    && state.shell.pending_window_id.is_none()
    && window_id(state).is_none()
    && state.kernel.tray.is_none()
    && !state.shell.quit_requested
  {
    Task::done(AppMessage::Tray(crate::tray::TrayAction::Quit))
  } else {
    Task::none()
  }
}

fn fail_transition(state: &mut State) -> Task<AppMessage> {
  match state.shell.pip.phase {
    Some(Phase::Opening) => {
      if state.shell.pip.selected {
        // The main window may have closed after native selection but before
        // the first successful presentation. Never retire the only playing
        // surface without either a ready receiver or a pause acknowledgement.
        let recovery = if state.shell.window_id.is_some() {
          update(state, Message::Return)
        } else {
          update(state, Message::Close)
        };
        state.shell.pip.notice = Some(UiText::new("player-pip-open-failed"));
        return recovery;
      }
      let close = retire(state);
      state.shell.pip.notice = Some(UiText::new("player-pip-open-failed"));
      Task::batch([close, last_window(state)])
    }
    Some(Phase::Returning) => {
      let cancel = cancel_return_open(state);
      let surface = &mut state.shell.pip;
      surface.tv_return = None;
      crate::embedded::set_presentation_window(surface.native);
      surface.main_presenting = false;
      surface.generation = surface.generation.wrapping_add(1);
      surface.phase = Some(Phase::Active);
      surface.querying = false;
      surface.target = None;
      surface.notice = Some(UiText::new("player-pip-return-failed"));
      let reveal = reveal(surface);
      let play = playback::window_opened(
        &mut state.playback,
        &mut state.kernel,
        state.shell.quit_requested,
      )
      .unwrap_or_else(Task::none);
      Task::batch([cancel, reveal, play])
    }
    _ => Task::none(),
  }
}

pub(super) fn reconcile(state: &mut State) -> Task<AppMessage> {
  if window_id(state).is_none() {
    return Task::none();
  }
  let media_generation = state.playback.view.lifecycle.replacement_generation;
  let lifecycle = &state.playback.view.lifecycle;
  if state.shell.quit_requested
    || state.shell.pip.session != Some(state.kernel.request_gate.current_session())
    || super::accounts::content_mutations_blocked(&state.kernel)
    || (!lifecycle.playback_active
      && !lifecycle.retain_presentation
      && !lifecycle.replacing
      && lifecycle.settled
      && !state.playback.view.busy
      && !playback::play_pending(&state.playback))
  {
    let close = retire(state);
    return Task::batch([close, last_window(state)]);
  }
  if state.shell.pip.media_generation != media_generation {
    state.shell.pip.media_generation = media_generation;
    if state.shell.pip.phase == Some(Phase::Closing) {
      return admit_playback(state);
    }
    if state.shell.pip.phase == Some(Phase::Active) {
      state.shell.pip.generation = state.shell.pip.generation.wrapping_add(1);
    }
  }
  if state.tv_mode() && state.shell.pip.phase != Some(Phase::Returning) {
    return update(state, Message::Return);
  }
  if state.shell.pip.phase == Some(Phase::Returning) && !state.shell.pip.querying {
    if let Some(id) = state.shell.window_id {
      return query(state, id);
    }
  }
  Task::none()
}

/// Intercepts only auxiliary-window events; main lifecycle retains its own state.
pub(super) fn window_event(state: &mut State, event: WindowMessage) -> Option<Task<AppMessage>> {
  match event {
    WindowMessage::CloseRequested(id) if window_id(state) == Some(id) => {
      Some(update(state, Message::Close))
    }
    WindowMessage::Closed(id) if window_id(state) == Some(id) => {
      let was_selected = state.shell.pip.keeps_alive();
      let close = retire(state);
      let mut tasks = vec![close];
      if was_selected {
        // There is no visible source to keep on a compositor-forced close.
        // Pause and revoke queued commands before choosing another surface.
        let pause = playback::window_closed(
          &mut state.playback,
          &mut state.kernel,
          state.shell.quit_requested,
        );
        tasks.push(pause.task);
        tasks.push(
          pause
            .acknowledgement
            .map(|result| AppMessage::Playback(PlaybackMessage::PresentationPaused(result))),
        );
        if state.shell.window_id.is_some() {
          tasks.push(shell::navigate(state, Destination::NowPlaying));
          tasks.extend(playback::window_opened(
            &mut state.playback,
            &mut state.kernel,
            state.shell.quit_requested,
          ));
        }
      }
      tasks.push(last_window(state));
      Some(Task::batch(tasks))
    }
    WindowMessage::Resized(id, _) if window_id(state) == Some(id) => Some(Task::none()),
    WindowMessage::OpenTimedOut(id) | WindowMessage::Closed(id)
      if state.shell.pip.phase == Some(Phase::Returning)
        && (state.shell.pip.target == Some(id) || state.shell.pending_window_id == Some(id)) =>
    {
      // Main shell still consumes the event; reconciliation observes failure.
      state.shell.pip.querying = false;
      None
    }
    _ => None,
  }
}

pub(super) fn update(state: &mut State, action: Message) -> Task<AppMessage> {
  match action {
    Message::Bound {
      id,
      generation,
      message,
    } => {
      if current(state, id, generation) {
        update(state, *message)
      } else {
        Task::none()
      }
    }
    Message::Enter => {
      if !available(state) {
        return Task::none();
      }
      let (id, open) = window::open(window::Settings {
        size: iced::Size::new(480.0, 270.0),
        min_size: Some(iced::Size::new(280.0, 158.0)),
        decorations: false,
        resizable: true,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        icon: crate::window_icon(),
        ..window::Settings::default()
      });
      let surface = &mut state.shell.pip;
      surface.generation = surface.generation.wrapping_add(1);
      surface.id = Some(id);
      surface.phase = Some(Phase::Opening);
      surface.session = Some(state.kernel.request_gate.current_session());
      surface.restore_fullscreen = state.shell.player_fullscreen;
      surface.notice = None;
      surface.media_generation = state.playback.view.lifecycle.replacement_generation;
      surface.source = None;
      surface.native = None;
      surface.main_presenting = false;
      let generation = surface.generation;
      Task::batch([
        open.map(move |id| message(Message::Opened { id, generation })),
        timeout(generation),
      ])
    }
    Message::Opened { id, generation } => {
      if !current(state, id, generation) {
        return if window_id(state) == Some(id) {
          Task::none()
        } else {
          window::close(id)
        };
      }
      if state.shell.pip.phase == Some(Phase::Opening) {
        query(state, id)
      } else {
        Task::none()
      }
    }
    Message::Poll { id, generation } => {
      if state.shell.pip.generation == generation
        && state.shell.pip.target == Some(id)
        && matches!(
          state.shell.pip.phase,
          Some(Phase::Opening | Phase::Returning)
        )
      {
        query(state, id)
      } else {
        Task::none()
      }
    }
    Message::Ready {
      id,
      generation,
      window: ready,
    } => {
      if state.shell.pip.generation != generation
        || state.shell.pip.target != Some(id)
        || state.shell.pip.session != Some(state.kernel.request_gate.current_session())
        || !matches!(
          state.shell.pip.phase,
          Some(Phase::Opening | Phase::Returning)
        )
      {
        return Task::none();
      }
      let Some(ready) = ready else {
        return Task::perform(
          async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            (id, generation)
          },
          |(id, generation)| message(Message::Poll { id, generation }),
        );
      };
      if state.shell.pip.phase == Some(Phase::Opening) {
        if window_id(state) != Some(id) {
          return Task::none();
        }
        if state.shell.pip.source.is_none() {
          let Some(main) = state.shell.window_id else {
            return fail_transition(state);
          };
          return crate::embedded::presentation_window(main).map(move |source| {
            message(Message::SourceReady {
              id,
              generation,
              target: ready,
              source,
            })
          });
        }
        if !crate::embedded::set_presentation_window(Some(ready)) {
          return fail_transition(state);
        }
        state.shell.pip.native = Some(ready);
        state.shell.pip.selected = true;
        playback::tools::close(&mut state.playback);
        playback::cancel_slider_drags(&mut state.playback);
        poll_presentation(id, generation, ready)
      } else {
        if state.shell.window_id != Some(id) {
          return Task::none();
        }
        let navigate = shell::navigate(state, Destination::NowPlaying);
        if !crate::embedded::set_presentation_window(Some(ready)) {
          return fail_transition(state);
        }
        state.shell.pip.main_presenting = true;
        Task::batch([navigate, poll_presentation(id, generation, ready)])
      }
    }
    Message::SourceReady {
      id,
      generation,
      target,
      source,
    } => {
      if !current(state, id, generation) || state.shell.pip.phase != Some(Phase::Opening) {
        return Task::none();
      }
      let Some(source) = source else {
        return fail_transition(state);
      };
      state.shell.pip.source = Some(source);
      update(
        state,
        Message::Ready {
          id,
          generation,
          window: Some(target),
        },
      )
    }
    Message::Presented {
      id,
      generation,
      window: native,
    } => {
      if state.shell.pip.generation != generation
        || state.shell.pip.target != Some(id)
        || state.shell.pip.session != Some(state.kernel.request_gate.current_session())
        || !matches!(
          state.shell.pip.phase,
          Some(Phase::Opening | Phase::Returning)
        )
      {
        return Task::none();
      }
      match crate::embedded::presentation_status(native) {
        PresentationStatus::Preparing => poll_presentation(id, generation, native),
        PresentationStatus::Failed | PresentationStatus::Unavailable => fail_transition(state),
        PresentationStatus::Ready if state.shell.pip.phase == Some(Phase::Opening) => {
          state.shell.pip.phase = Some(Phase::Active);
          state.shell.pip.querying = false;
          state.shell.pip.target = None;
          Task::batch([
            shell::exit_player_fullscreen(state),
            reveal(&mut state.shell.pip),
          ])
        }
        PresentationStatus::Ready => {
          if state.shell.window_id != Some(id) {
            return fail_transition(state);
          }
          state.shell.pip.source = Some(native);
          finish_return(state, id)
        }
      }
    }
    Message::TimedOut { generation } if state.shell.pip.generation == generation => {
      fail_transition(state)
    }
    Message::Return => {
      if window_id(state).is_none() || state.shell.pip.phase == Some(Phase::Returning) {
        return Task::none();
      }
      if state.shell.pip.phase == Some(Phase::Opening) && !state.shell.pip.selected {
        return retire(state);
      }
      let surface = &mut state.shell.pip;
      surface.generation = surface.generation.wrapping_add(1);
      surface.phase = Some(Phase::Returning);
      surface.querying = false;
      surface.target = None;
      surface.notice = None;
      let generation = surface.generation;
      let needs_main = state.shell.window_id.is_none() && state.shell.pending_window_id.is_none();
      let show = super::update::route_message(
        state,
        AppMessage::Window(WindowMessage::ShowRequested(None)),
      );
      if needs_main {
        state.shell.pip.return_pending = state.shell.pending_window_id;
      }
      Task::batch([show, timeout(generation), reconcile(state)])
    }
    Message::Close => {
      let Some(id) = window_id(state) else {
        return Task::none();
      };
      if state.shell.pip.phase == Some(Phase::Opening) && !state.shell.pip.selected {
        return retire(state);
      }
      if state.shell.pip.phase == Some(Phase::Closing) {
        return Task::none();
      }
      let cancel = cancel_return_open(state);
      crate::embedded::set_presentation_window(state.shell.pip.native);
      state.shell.pip.main_presenting = false;
      let surface = &mut state.shell.pip;
      surface.generation = surface.generation.wrapping_add(1);
      surface.phase = Some(Phase::Closing);
      surface.notice = None;
      surface.querying = false;
      surface.target = None;
      let generation = surface.generation;
      let pause = playback::window_closed(
        &mut state.playback,
        &mut state.kernel,
        state.shell.quit_requested,
      );
      Task::batch([
        cancel,
        pause.task,
        pause.acknowledgement.map(move |result| {
          message(Message::PauseSettled {
            id,
            generation,
            result,
          })
        }),
      ])
    }
    Message::PauseSettled {
      id,
      generation,
      result,
    } => {
      if !current(state, id, generation) || state.shell.pip.phase != Some(Phase::Closing) {
        return Task::none();
      }
      if result.is_err() {
        state.shell.pip.phase = Some(Phase::Active);
        state.shell.pip.notice = Some(UiText::new("player-pip-pause-failed"));
        let play = playback::window_opened(
          &mut state.playback,
          &mut state.kernel,
          state.shell.quit_requested,
        )
        .unwrap_or_else(Task::none);
        return Task::batch([play, reveal(&mut state.shell.pip)]);
      }
      let close = retire(state);
      let play = if state.shell.window_id.is_some() {
        playback::window_opened(
          &mut state.playback,
          &mut state.kernel,
          state.shell.quit_requested,
        )
        .unwrap_or_else(Task::none)
      } else {
        Task::none()
      };
      Task::batch([close, play, last_window(state)])
    }
    Message::PointerMoved => reveal(&mut state.shell.pip),
    Message::PointerLeft => {
      state.shell.pip.controls_deadline = None;
      Task::none()
    }
    Message::FocusChanged(focused) => {
      state.shell.pip.controls_focus = focused;
      reveal(&mut state.shell.pip)
    }
    Message::HideControls {
      generation,
      deadline,
    } => {
      if state.shell.pip.generation == generation
        && state.shell.pip.controls_deadline == Some(deadline)
      {
        state.shell.pip.controls_deadline = None;
      }
      Task::none()
    }
    Message::Drag if controls_enabled(state) => {
      window_id(state).map_or_else(Task::none, window::drag)
    }
    Message::Resize(direction) if controls_enabled(state) => {
      window_id(state).map_or_else(Task::none, |id| window::drag_resize(id, direction))
    }
    Message::FocusNext(previous) if controls_enabled(state) => {
      state.shell.pip.controls_focus = true;
      let focus = shell::focus_step("pip-root", previous);
      Task::batch([focus, reveal(&mut state.shell.pip)])
    }
    Message::TogglePaused
      if controls_enabled(state)
        && !state.playback.view.busy
        && !state.playback.view.lifecycle.replacing =>
    {
      playback::update(
        &mut state.playback,
        &mut state.kernel,
        state.shell.quit_requested,
        PlaybackMessage::Intent(Box::new(PlaybackIntent::TogglePaused)),
      )
      .task
    }
    _ => Task::none(),
  }
}

pub(super) fn subscription(state: &State) -> Subscription<AppMessage> {
  let Some(id) = window_id(state) else {
    return Subscription::none();
  };
  let events = iced::event::listen_with(|event, status, id| Some((event, status, id)))
    .with((id, generation(state)))
    .filter_map(pip_event);
  let timer = state
    .shell
    .pip
    .controls_deadline
    .map_or_else(Subscription::none, |deadline| {
      Subscription::run_with((generation(state), deadline), controls_timer)
    });
  Subscription::batch([events, timer])
}

type WindowInput = ((window::Id, u64), (Event, event::Status, window::Id));

fn pip_event(((id, generation), (event, status, source)): WindowInput) -> Option<AppMessage> {
  if source != id {
    return None;
  }
  let action = match event {
    Event::Window(window::Event::Focused) => Message::PointerMoved,
    Event::Window(window::Event::Unfocused) => Message::FocusChanged(false),
    Event::Keyboard(keyboard::Event::KeyPressed {
      modified_key,
      modifiers,
      repeat: false,
      ..
    }) if status == event::Status::Ignored => match (modified_key.as_ref(), modifiers) {
      (keyboard::Key::Named(keyboard::key::Named::Space), keyboard::Modifiers::NONE) => {
        Message::TogglePaused
      }
      (keyboard::Key::Named(keyboard::key::Named::Escape), keyboard::Modifiers::NONE) => {
        Message::Return
      }
      (keyboard::Key::Named(keyboard::key::Named::Tab), keyboard::Modifiers::NONE) => {
        Message::FocusNext(false)
      }
      (keyboard::Key::Named(keyboard::key::Named::Tab), keyboard::Modifiers::SHIFT) => {
        Message::FocusNext(true)
      }
      _ => return None,
    },
    _ => return None,
  };
  Some(bind(id, generation, message(action)))
}

fn controls_timer(
  &(generation, deadline): &(u64, Instant),
) -> impl iced::futures::Stream<Item = AppMessage> {
  iced::futures::stream::once(async move {
    tokio::time::sleep_until(deadline.into()).await;
    message(Message::HideControls {
      generation,
      deadline,
    })
  })
}

fn finish_return(state: &mut State, id: window::Id) -> Task<AppMessage> {
  let navigate = shell::navigate(state, Destination::NowPlaying);
  let restore =
    state.shell.pip.restore_fullscreen && !state.tv_mode() && !state.shell.player_fullscreen;
  let tv = state.shell.pip.tv_return;
  let close = retire(state);
  let fullscreen = if restore && tv.is_none() {
    shell::toggle_player_fullscreen(state)
  } else {
    Task::none()
  };
  let play = playback::window_opened(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
  )
  .unwrap_or_else(Task::none);
  let mode = match tv {
    Some(true) => Task::done(AppMessage::UiModeSelected(
      jellypilot_core::config::UiMode::Tv,
    )),
    Some(false) => Task::done(AppMessage::TvLaunch),
    None => Task::none(),
  };
  Task::batch([
    navigate,
    fullscreen,
    play,
    window::gain_focus(id),
    close,
    mode,
  ])
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::futures::StreamExt;
  use iced_runtime::{window::Action as WindowAction, Action};

  fn playing(main: bool) -> State {
    let mut state = super::super::update::tests::test_state();
    state.shell.window_id = main.then(window::Id::unique);
    state.shell.images_visible = main;
    state.shell.destination = Destination::Search("preserved browsing".into());
    state.playback.view.lifecycle.playback_active = true;
    state.playback.view.lifecycle.retain_presentation = true;
    state.shell.pip = Surface {
      id: Some(window::Id::unique()),
      phase: Some(Phase::Active),
      generation: 1,
      session: Some(state.kernel.request_gate.current_session()),
      selected: true,
      ..Surface::default()
    };
    state
  }

  fn app_update(state: &mut State, event: WindowMessage) -> Task<AppMessage> {
    super::super::update::route_message(state, AppMessage::Window(event))
  }

  #[test]
  fn closing_main_without_tray_leaves_pip_and_playback_resident() {
    let mut state = playing(true);
    let main = state.shell.window_id.unwrap();
    let pip = window_id(&state);
    let lease = std::sync::Arc::clone(&state.playback.presentation_hold);
    drop(app_update(&mut state, WindowMessage::CloseRequested(main)));
    assert_eq!(state.shell.window_id, None);
    assert_eq!(window_id(&state), pip);
    assert!(!state.shell.quit_requested);
    assert!(std::sync::Arc::ptr_eq(
      &lease,
      &state.playback.presentation_hold
    ));
    assert!(state.shell.pending_close.is_none());
  }

  #[test]
  fn compositor_destroying_main_does_not_quit_or_retire_pip() {
    let mut state = playing(true);
    let main = state.shell.window_id.unwrap();
    drop(app_update(&mut state, WindowMessage::Closed(main)));
    assert!(active(&state));
    assert!(!state.shell.quit_requested);
    assert_eq!(state.shell.window_id, None);
  }

  #[test]
  fn auxiliary_and_foreign_resize_do_not_change_main_layout() {
    let mut state = playing(true);
    let main = state.shell.window_id.unwrap();
    let pip = window_id(&state).unwrap();
    let original = state.shell.window_size;
    drop(app_update(
      &mut state,
      WindowMessage::Resized(pip, iced::Size::new(480.0, 270.0)),
    ));
    drop(app_update(
      &mut state,
      WindowMessage::Resized(window::Id::unique(), iced::Size::new(1.0, 1.0)),
    ));
    assert_eq!(state.shell.window_size, original);
    let size = iced::Size::new(1100.0, 700.0);
    drop(app_update(&mut state, WindowMessage::Resized(main, size)));
    assert_eq!(state.shell.window_size, size);
  }

  #[test]
  fn return_opens_only_one_main_and_keeps_source_until_presentation() {
    let mut state = playing(false);
    drop(update(&mut state, Message::Return));
    let pending = state
      .shell
      .pending_window_id
      .expect("main open is admitted synchronously");
    let generation = generation(&state);
    drop(update(&mut state, Message::Return));
    assert_eq!(state.shell.pending_window_id, Some(pending));
    assert_eq!(generation, super::generation(&state));
    assert!(active(&state));
    assert!(!controls_enabled(&state));
  }

  #[test]
  fn timed_out_return_keeps_source_and_suppresses_late_main_admission() {
    let mut state = playing(false);
    drop(update(&mut state, Message::Return));
    let pending = state.shell.pending_window_id.unwrap();
    let generation = generation(&state);
    drop(update(&mut state, Message::TimedOut { generation }));
    assert!(active(&state));
    assert!(controls_enabled(&state));
    assert_eq!(
      notice(&state).map(UiText::id),
      Some("player-pip-return-failed")
    );
    drop(app_update(
      &mut state,
      WindowMessage::ShowRequested(Some(pending)),
    ));
    assert!(state.shell.window_id.is_none());
    assert!(state.shell.pending_window_id.is_none());
  }

  #[test]
  fn opening_timeout_restores_main_without_changing_browsing_or_fullscreen() {
    let mut state = playing(true);
    state.shell.pip.phase = Some(Phase::Opening);
    state.shell.pip.selected = false;
    state.shell.player_fullscreen = true;
    let destination = state.shell.destination.clone();
    let generation = generation(&state);
    drop(update(&mut state, Message::TimedOut { generation }));
    assert!(window_id(&state).is_none());
    assert_eq!(state.shell.destination, destination);
    assert!(state.shell.player_fullscreen);
    assert_eq!(
      notice(&state).map(UiText::id),
      Some("player-pip-open-failed")
    );
  }

  #[test]
  fn selected_opening_failure_after_main_close_waits_for_pause_acknowledgement() {
    let mut state = playing(false);
    let id = window_id(&state).unwrap();
    state.shell.pip.phase = Some(Phase::Opening);
    let opening = generation(&state);
    drop(update(
      &mut state,
      Message::TimedOut {
        generation: opening,
      },
    ));
    assert_eq!(window_id(&state), Some(id));
    assert_eq!(state.shell.pip.phase, Some(Phase::Closing));
    let closing = generation(&state);
    drop(update(
      &mut state,
      Message::PauseSettled {
        id,
        generation: closing,
        result: Err(PlaybackError::MpvControlFailed),
      },
    ));
    assert_eq!(window_id(&state), Some(id));
    assert!(controls_enabled(&state));
    assert_eq!(
      notice(&state).map(UiText::id),
      Some("player-pip-pause-failed")
    );
  }

  #[test]
  fn selected_opening_failure_with_main_waits_for_receiver_before_retiring() {
    let mut state = playing(true);
    state.shell.pip.phase = Some(Phase::Opening);
    let id = window_id(&state);
    let opening = generation(&state);
    drop(update(
      &mut state,
      Message::TimedOut {
        generation: opening,
      },
    ));
    assert_eq!(window_id(&state), id);
    assert_eq!(state.shell.pip.phase, Some(Phase::Returning));
  }

  #[test]
  fn escape_during_selected_opening_reopens_main_instead_of_hiding_video() {
    let mut state = playing(false);
    state.shell.pip.phase = Some(Phase::Opening);
    let id = window_id(&state);
    drop(update(&mut state, Message::Return));
    assert_eq!(window_id(&state), id);
    assert!(state.shell.pending_window_id.is_some());
    assert_eq!(state.shell.pip.phase, Some(Phase::Returning));
  }

  #[test]
  fn return_after_close_ignores_old_pause_acknowledgement() {
    let mut state = playing(true);
    let id = window_id(&state).unwrap();
    drop(update(&mut state, Message::Close));
    let closing = generation(&state);
    drop(update(&mut state, Message::Return));
    drop(update(
      &mut state,
      Message::PauseSettled {
        id,
        generation: closing,
        result: Ok(()),
      },
    ));
    assert_eq!(window_id(&state), Some(id));
    assert_eq!(state.shell.pip.phase, Some(Phase::Returning));
  }

  #[test]
  fn close_after_return_cancels_pending_main_and_ignores_transfer_callback() {
    let mut state = playing(false);
    drop(update(&mut state, Message::Return));
    let pending = state.shell.pending_window_id.unwrap();
    let returning = generation(&state);
    drop(update(&mut state, Message::Close));
    drop(update(
      &mut state,
      Message::Ready {
        id: pending,
        generation: returning,
        window: None,
      },
    ));
    assert_eq!(state.shell.pip.phase, Some(Phase::Closing));
    assert!(state.shell.pending_window_id.is_none());
    assert!(active(&state));
  }

  #[test]
  fn close_requires_matching_success_and_preserves_browsed_page() {
    let mut state = playing(true);
    let destination = state.shell.destination.clone();
    let id = window_id(&state).unwrap();
    drop(update(&mut state, Message::Close));
    assert_eq!(window_id(&state), Some(id));
    assert!(!controls_enabled(&state));
    let generation = generation(&state);
    drop(update(
      &mut state,
      Message::PauseSettled {
        id,
        generation,
        result: Ok(()),
      },
    ));
    assert!(window_id(&state).is_none());
    assert_eq!(state.shell.destination, destination);
  }

  #[test]
  fn failed_pause_keeps_visible_pip_and_reports_inline_failure() {
    let mut state = playing(true);
    let id = window_id(&state).unwrap();
    drop(update(&mut state, Message::Close));
    let generation = generation(&state);
    drop(update(
      &mut state,
      Message::PauseSettled {
        id,
        generation,
        result: Err(PlaybackError::MpvControlFailed),
      },
    ));
    assert_eq!(window_id(&state), Some(id));
    assert!(controls_enabled(&state));
    assert_eq!(
      notice(&state).map(UiText::id),
      Some("player-pip-pause-failed")
    );
  }

  #[test]
  fn explicit_resume_revokes_pending_close_before_reopening_admission() {
    let mut state = playing(true);
    let id = window_id(&state).unwrap();
    drop(update(&mut state, Message::Close));
    let closing = generation(&state);
    drop(admit_playback(&mut state));
    drop(update(
      &mut state,
      Message::PauseSettled {
        id,
        generation: closing,
        result: Ok(()),
      },
    ));
    assert_eq!(window_id(&state), Some(id));
    assert!(controls_enabled(&state));
  }

  #[test]
  fn replacement_media_cannot_inherit_old_close_acknowledgement() {
    let mut state = playing(true);
    let id = window_id(&state).unwrap();
    drop(update(&mut state, Message::Close));
    let closing = generation(&state);
    state.playback.view.lifecycle.replacement_generation += 1;
    drop(reconcile(&mut state));
    drop(update(
      &mut state,
      Message::PauseSettled {
        id,
        generation: closing,
        result: Ok(()),
      },
    ));
    assert!(controls_enabled(&state));
    assert_eq!(window_id(&state), Some(id));
  }

  #[test]
  fn old_window_controls_cannot_close_a_new_pip() {
    let mut state = playing(true);
    let stale = bind_current(&state, Message::Close);
    drop(retire(&mut state));
    let id = window::Id::unique();
    state.shell.pip.id = Some(id);
    state.shell.pip.selected = true;
    state.shell.pip.phase = Some(Phase::Active);
    drop(super::super::update::route_message(&mut state, stale));
    assert_eq!(window_id(&state), Some(id));
    assert_eq!(state.shell.pip.phase, Some(Phase::Active));
  }

  #[test]
  fn returned_surface_retires_pip_only_after_main_admission() {
    for main_open in [true, false] {
      let mut state = playing(main_open);
      drop(update(&mut state, Message::Return));
      if let Some(id) = state.shell.pending_window_id {
        drop(app_update(
          &mut state,
          WindowMessage::ShowRequested(Some(id)),
        ));
      }
      let main = state
        .shell
        .window_id
        .expect("main admitted before presentation completion");
      drop(finish_return(&mut state, main));
      assert_eq!(state.shell.window_id, Some(main));
      assert!(window_id(&state).is_none());
      assert_eq!(state.shell.destination, Destination::NowPlaying);
    }
  }

  #[tokio::test]
  async fn unexpected_last_window_close_requests_orderly_quit_without_tray() {
    let mut state = playing(false);
    let id = window_id(&state).unwrap();
    let task = app_update(&mut state, WindowMessage::Closed(id));
    let mut actions = iced_runtime::task::into_stream(task).unwrap();
    let mut quit = false;
    while let Some(action) = actions.next().await {
      quit |= matches!(
        action,
        Action::Output(AppMessage::Tray(crate::tray::TrayAction::Quit))
      );
    }
    assert!(quit);
    assert!(window_id(&state).is_none());
  }

  #[tokio::test]
  async fn late_open_callback_is_closed_without_displacing_a_new_window() {
    let mut state = playing(true);
    let old = window::Id::unique();
    let current = window_id(&state);
    let task = update(
      &mut state,
      Message::Opened {
        id: old,
        generation: 0,
      },
    );
    let mut actions = iced_runtime::task::into_stream(task).unwrap();
    assert!(
      matches!(actions.next().await, Some(Action::Window(WindowAction::Close(id))) if id == old)
    );
    assert_eq!(window_id(&state), current);
  }

  #[test]
  fn main_search_space_never_becomes_a_pip_transport_shortcut() {
    let main = window::Id::unique();
    let pip = window::Id::unique();
    let key = || {
      Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(keyboard::key::Named::Space),
        modified_key: keyboard::Key::Named(keyboard::key::Named::Space),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Space),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::NONE,
        text: Some(" ".into()),
        repeat: false,
      })
    };
    assert!(pip_event(((pip, 1), (key(), event::Status::Ignored, main))).is_none());
    assert!(pip_event(((pip, 1), (key(), event::Status::Captured, pip))).is_none());
    assert!(matches!(
      pip_event(((pip, 1), (key(), event::Status::Ignored, pip))),
      Some(AppMessage::Pip(Message::Bound { id, message, .. }))
        if id == pip && matches!(*message, Message::TogglePaused)
    ));
  }

  #[test]
  fn ended_playback_retires_pip_without_leaving_the_browsed_page() {
    let mut state = playing(true);
    let destination = state.shell.destination.clone();
    state.playback.view.lifecycle.playback_active = false;
    state.playback.view.lifecycle.retain_presentation = false;
    state.playback.view.lifecycle.settled = true;
    drop(reconcile(&mut state));
    assert!(window_id(&state).is_none());
    assert_eq!(state.shell.destination, destination);
  }

  #[test]
  fn tv_request_waits_for_main_and_failed_return_preserves_desktop_pip() {
    let mut state = playing(false);
    let app_mode = state.app_mode();
    drop(return_for_tv(&mut state, true));
    assert!(!state.tv_mode());
    let generation = generation(&state);
    drop(update(&mut state, Message::TimedOut { generation }));
    assert!(!state.tv_mode());
    assert_eq!(state.app_mode(), app_mode);
    assert!(active(&state));
    assert!(state.shell.pip.tv_return.is_none());
  }
}
