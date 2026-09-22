//! TV search draft, using the shared browse route and remote text editor.

use iced::widget::{button, column, container, opaque, responsive, row, text};
use iced::{Element, Fill, Task};
use jellypilot_core::tv_navigation::Input;
use jellypilot_ui::fonts::DISPLAY_FONT;
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::tv_focus::focus;

use crate::app::message::{HomeMessage, Message as AppMessage};
use crate::app::state::{Destination, State};

use super::text_entry;

#[derive(Default)]
pub struct Surface {
  pub open: bool,
  draft: String,
  keyboard: text_entry::Surface,
  actions: bool,
  confirm: bool,
}

#[derive(Clone)]
pub enum Message {
  Entry(text_entry::Message),
  Edit,
  Submit,
  Close,
}

fn message(message: Message) -> AppMessage {
  AppMessage::Tv(super::Message::Search(message))
}

pub fn open(state: &mut State) -> Task<AppMessage> {
  state.tv.search.open = true;
  state.tv.search.draft = match &state.shell.destination {
    Destination::Search(query) => query.clone(),
    _ => String::new(),
  };
  state.tv.search.keyboard = text_entry::Surface::default();
  state.tv.search.actions = false;
  state.tv.search.confirm = false;
  text_entry::unfocus()
}

pub fn close(state: &mut State) -> Task<AppMessage> {
  state.tv.search.open = false;
  state.tv.search.draft.clear();
  text_entry::unfocus()
}

fn apply_entry(
  state: &mut State,
  action: Option<text_entry::Action>,
  task: Task<text_entry::Message>,
) -> Task<AppMessage> {
  let effect = match action {
    Some(text_entry::Action::Changed(value)) => {
      state.tv.search.draft = value;
      Task::none()
    }
    Some(text_entry::Action::Done) => {
      state.tv.search.actions = true;
      state.tv.search.confirm = !state.tv.search.draft.trim().is_empty();
      Task::none()
    }
    Some(text_entry::Action::Cancel) => close(state),
    None => Task::none(),
  };
  Task::batch([task.map(|entry| message(Message::Entry(entry))), effect])
}

pub fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  if !state.tv.search.open {
    return Task::none();
  }
  if state.tv.search.actions {
    match input {
      Input::Back => return close(state),
      Input::Up => return update(state, Message::Edit),
      Input::Left => state.tv.search.confirm = false,
      Input::Right => state.tv.search.confirm = !state.tv.search.draft.trim().is_empty(),
      Input::Confirm => {
        return update(
          state,
          if state.tv.search.confirm {
            Message::Submit
          } else {
            Message::Close
          },
        )
      }
      _ => {}
    }
    return Task::none();
  }
  let surface = &mut state.tv.search;
  let (action, task) = text_entry::input(&mut surface.keyboard, &surface.draft, input);
  apply_entry(state, action, task)
}

pub fn update(state: &mut State, event: Message) -> Task<AppMessage> {
  if !state.tv_mode() || !state.tv.search.open {
    return Task::none();
  }
  match event {
    Message::Close => close(state),
    Message::Edit => {
      state.tv.search.actions = false;
      text_entry::unfocus()
    }
    Message::Submit => {
      let query = state.tv.search.draft.trim().to_owned();
      if query.is_empty() {
        return Task::none();
      }
      let close = close(state);
      let navigate = crate::app::update::route_message(
        state,
        AppMessage::Home(HomeMessage::Navigate(Destination::Search(query))),
      );
      Task::batch([close, navigate])
    }
    Message::Entry(entry) => {
      let surface = &mut state.tv.search;
      surface.actions = false;
      let (action, task) = text_entry::update(&mut surface.keyboard, &surface.draft, entry);
      apply_entry(state, action, task)
    }
  }
}

pub fn view(state: &State) -> Element<'_, AppMessage> {
  opaque(responsive(move |bounds| -> Element<'_, AppMessage> {
    let scale = style::scale(bounds.width);
    let surface = &state.tv.search;
    let controls = row![
      action(
        state.t("common-cancel"),
        surface.actions && !surface.confirm,
        true,
        Message::Close,
        scale
      ),
      action(
        state.t("tv-search-submit"),
        surface.actions && surface.confirm,
        !surface.draft.trim().is_empty(),
        Message::Submit,
        scale
      ),
    ]
    .spacing(20.0 * scale);
    let body = column![
      text(state.t("tv-search-title"))
        .font(DISPLAY_FONT)
        .size(style::TITLE * scale)
        .color(style::PALETTE.text.heading),
      text_entry::view(
        &surface.keyboard,
        state.kernel.locale,
        state.t("tv-search-placeholder"),
        &surface.draft,
        false,
        !surface.actions,
        scale
      )
      .map(|entry| message(Message::Entry(entry))),
      controls,
    ]
    .spacing(style::GAP * scale);
    container(container(body).width(iced::Length::Fill.max(1280.0 * scale)))
      .width(Fill)
      .height(Fill)
      .center_x(Fill)
      .center_y(Fill)
      .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
      .style(style::canvas)
      .into()
  }))
}

fn action<'a>(
  label: String,
  focused: bool,
  enabled: bool,
  event: Message,
  scale: f32,
) -> Element<'a, AppMessage> {
  container(focus(focused, move |progress| {
    button(
      text(label.clone())
        .size(style::BODY * scale)
        .color(style::foreground(style::PALETTE, progress, false)),
    )
    .width(Fill)
    .height(style::CONTROL * scale)
    .style(style::button_progress(style::PALETTE, progress, false))
    .on_press_maybe(enabled.then(|| message(event.clone())))
    .into()
  }))
  .width(Fill)
  .into()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cancelling_search_preserves_the_source_focus_and_destination() {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    state.shell.destination = Destination::Search("original".to_owned());
    state.tv.focus = super::super::Focus::Grid(7);
    drop(open(&mut state));
    drop(update(
      &mut state,
      Message::Entry(text_entry::Message::Changed("changed".to_owned())),
    ));
    drop(input(&mut state, Input::Back));
    assert!(!state.tv.search.open);
    assert_eq!(
      state.shell.destination,
      Destination::Search("original".to_owned())
    );
    assert_eq!(state.tv.focus, super::super::Focus::Grid(7));
  }

  #[test]
  fn blank_submit_keeps_the_editor_open_and_does_not_navigate() {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    let destination = state.shell.destination.clone();
    drop(open(&mut state));
    drop(update(
      &mut state,
      Message::Entry(text_entry::Message::Changed("  ".to_owned())),
    ));
    drop(update(&mut state, Message::Submit));
    assert!(state.tv.search.open);
    assert_eq!(state.shell.destination, destination);
  }
}
