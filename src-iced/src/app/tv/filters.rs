//! Candidate library filters. Only Apply crosses into the shared browse query.

use iced::widget::{
  button, column, container, mouse_area, opaque, row, scrollable, space, stack, text, Column, Row,
};
use iced::{Alignment, Element, Fill, Task};
use jellypilot_core::diagnostics::sanitize_message;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::tv_navigation::Input;
use jellypilot_media_server::{
  VideoLibraryFilterOptions, VideoLibraryFilters, VideoLibraryKind, VideoLibraryQuality,
};
use jellypilot_ui::fonts::HEADING_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::tv_focus;

use super::{AppMessage, Focus as TvFocus, State};
use crate::app::state::Destination;

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
  serial: u64,
  session: SessionToken,
  destination: Destination,
}

#[derive(Clone)]
pub enum Message {
  Loaded(Token, Result<VideoLibraryFilterOptions, String>),
  Focus(Token, usize, usize),
  Select(Token, usize, usize),
  Apply(Token),
  Cancel(Token),
  Retry(Token),
  Scrolled(Token, usize, f32),
  Reveal(Token, usize, usize, f32),
}

#[derive(Default)]
pub struct Surface {
  panel: Option<Panel>,
  serial: u64,
}

struct Panel {
  token: Token,
  applied: VideoLibraryFilters,
  draft: VideoLibraryFilters,
  options: Options,
  row: usize,
  cursors: [usize; 4],
  offsets: [f32; 3],
  pending: Option<iced::task::Handle>,
}

impl Drop for Panel {
  fn drop(&mut self) {
    if let Some(pending) = self.pending.take() {
      pending.abort();
    }
  }
}

enum Options {
  Loading,
  Ready(VideoLibraryFilterOptions),
  Failed(String),
}

fn dispatch(message: Message) -> AppMessage {
  AppMessage::Tv(super::Message::Filters(message))
}

pub fn is_open(state: &State) -> bool {
  state.tv.filters.panel.is_some()
}

fn available(state: &State) -> bool {
  state.tv_mode()
    && state.kernel.connection == jellypilot_auth::login::ConnectionPhase::Connected
    && matches!(state.shell.destination, Destination::Library { .. })
    && !state.tv.settings.open
    && !state.tv.search.open
    && !super::saved_browse::overlay_open(state)
    && !super::account::modal_open(state)
    && !super::lists::menu_open(state)
    && !super::player::active(state)
    && !state.shell.quit_requested
    && state.shell.pending_close.is_none()
    && !crate::app::accounts::content_mutations_blocked(&state.kernel)
    && !crate::app::accounts::blocking_modal(&state.accounts)
    && !state.shell.settings_open
    && !state.shell.account_popover_open
}

fn current(state: &State, token: &Token) -> bool {
  available(state)
    && token.session == state.kernel.request_gate.current_session()
    && token.destination == state.shell.destination
    && state
      .tv
      .filters
      .panel
      .as_ref()
      .is_some_and(|panel| &panel.token == token)
}

pub fn open(state: &mut State) -> Task<AppMessage> {
  if !available(state) || is_open(state) {
    return Task::none();
  }
  let Some(full) = state.full.as_ref() else {
    return Task::none();
  };
  let applied = full.browse.advanced_filters.clone();
  state.tv.press = None;
  state.tv.filters.panel = Some(Panel {
    token: Token {
      serial: 0,
      session: state.kernel.request_gate.current_session(),
      destination: state.shell.destination.clone(),
    },
    draft: applied.clone(),
    applied,
    options: Options::Loading,
    row: 3,
    cursors: [0, 0, 0, 1],
    offsets: [0.0; 3],
    pending: None,
  });
  load(state)
}

fn load(state: &mut State) -> Task<AppMessage> {
  let Destination::Library {
    library_id,
    collection_type,
  } = &state.shell.destination
  else {
    return Task::none();
  };
  let library_id = library_id.clone();
  let kind = if collection_type == "tvshows" {
    VideoLibraryKind::TvShows
  } else {
    VideoLibraryKind::Movies
  };
  let unavailable = state.t("tv-filter-failed");
  let Some(panel) = state.tv.filters.panel.as_mut() else {
    return Task::none();
  };
  if let Some(pending) = panel.pending.take() {
    pending.abort();
  }
  state.tv.filters.serial = state.tv.filters.serial.wrapping_add(1);
  panel.token.serial = state.tv.filters.serial;
  panel.options = Options::Loading;
  panel.row = 3;
  panel.cursors[3] = 1;
  let token = panel.token.clone();
  let Some(client) = state.kernel.client.clone() else {
    return Task::done(dispatch(Message::Loaded(token, Err(unavailable))));
  };
  let (task, handle) = Task::perform(
    async move {
      client
        .library()
        .filter_options(library_id, kind)
        .await
        .map_err(|error| sanitize_message(&error.to_string()))
    },
    move |result| dispatch(Message::Loaded(token.clone(), result)),
  )
  .abortable();
  panel.pending = Some(handle);
  task
}

/// Cancels pending metadata demand without changing the applied browse query.
pub fn close(state: &mut State) {
  let Some(panel) = state.tv.filters.panel.take() else {
    return;
  };
  if panel.token.destination == state.shell.destination
    && panel.token.session == state.kernel.request_gate.current_session()
  {
    state.tv.focus = TvFocus::Header(2);
  }
  state.tv.press = None;
}

pub fn reconcile(state: &mut State) {
  if state
    .tv
    .filters
    .panel
    .as_ref()
    .is_some_and(|panel| !current(state, &panel.token))
  {
    close(state);
  }
}

pub fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  let token = match &message {
    Message::Loaded(token, _)
    | Message::Focus(token, _, _)
    | Message::Select(token, _, _)
    | Message::Apply(token)
    | Message::Cancel(token)
    | Message::Retry(token)
    | Message::Scrolled(token, _, _)
    | Message::Reveal(token, _, _, _) => token,
  };
  if !current(state, token) {
    return Task::none();
  }
  match message {
    Message::Loaded(_, result) => {
      let panel = state.tv.filters.panel.as_mut().expect("current panel");
      panel.pending = None;
      panel.options = match result {
        Ok(options) => Options::Ready(options),
        Err(error) => Options::Failed(error),
      };
      if let Options::Ready(_) = &panel.options {
        panel.cursors = [
          selected_index(panel, 0),
          selected_index(panel, 1),
          selected_index(panel, 2),
          0,
        ];
        panel.row = 0;
        for row in 0..3 {
          panel.offsets[row] = ((panel.cursors[row] + 1) as f32 * 212.0 - 12.0 - 688.0).max(0.0);
        }
      } else {
        panel.row = 3;
        panel.cursors[3] = 0;
      }
      reveal(state)
    }
    Message::Scrolled(_, row, offset) => {
      if row < 3 && offset.is_finite() {
        let scale = style::scale(state.shell.window_size.width);
        state
          .tv
          .filters
          .panel
          .as_mut()
          .expect("current panel")
          .offsets[row] = (offset / scale).max(0.0);
      }
      Task::none()
    }
    Message::Reveal(_, row, index, y) => {
      let panel = state.tv.filters.panel.as_ref().expect("current panel");
      if panel.row != row || panel.cursors[row] != index || !y.is_finite() {
        return Task::none();
      }
      iced::widget::operation::scroll_to(
        iced::widget::Id::new("tv-filter-panel"),
        iced::widget::operation::AbsoluteOffset { x: 0.0, y },
      )
    }
    Message::Cancel(_) => {
      close(state);
      Task::none()
    }
    Message::Retry(_) => load(state),
    Message::Apply(_) => {
      let Some(panel) = state
        .tv
        .filters
        .panel
        .as_ref()
        .filter(|panel| matches!(panel.options, Options::Ready(_)))
      else {
        return Task::none();
      };
      let draft = panel.draft.clone();
      let changed = state
        .full
        .as_ref()
        .is_some_and(|full| full.browse.advanced_filters != draft);
      let source = crate::app::shell::browse_source(state);
      if source.is_none() {
        return Task::none();
      }
      close(state);
      if !changed {
        return Task::none();
      }
      state.tv.offset = 0.0;
      let Some(full) = state.full.as_mut() else {
        return Task::none();
      };
      let task = crate::app::browse::apply_advanced_filters(
        &mut full.browse,
        &mut state.kernel,
        source,
        draft,
      );
      Task::batch([task, super::navigation::restore_scroll(state)])
    }
    Message::Focus(_, row, index) => {
      let panel = state.tv.filters.panel.as_mut().expect("current panel");
      if row < 4 && index < row_len(panel, row) {
        panel.row = row;
        panel.cursors[row] = index;
      }
      Task::none()
    }
    Message::Select(_, row, index) => {
      let panel = state.tv.filters.panel.as_mut().expect("current panel");
      if row >= 3 || index >= row_len(panel, row) {
        return Task::none();
      }
      let values = values(panel, row);
      match row {
        0 => {
          panel.draft.quality = values.get(index).and_then(|value| match value {
            Value::Quality(quality) => Some(*quality),
            _ => None,
          })
        }
        1 => {
          panel.draft.country = values.get(index).and_then(|value| match value {
            Value::Text(value) => Some(value.clone()),
            _ => None,
          })
        }
        _ => {
          panel.draft.genre = values.get(index).and_then(|value| match value {
            Value::Text(value) => Some(value.clone()),
            _ => None,
          })
        }
      }
      panel.row = row;
      panel.cursors[row] = index;
      Task::none()
    }
  }
}

pub fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  let Some(panel) = state.tv.filters.panel.as_ref() else {
    return Task::none();
  };
  if !current(state, &panel.token) {
    close(state);
    return Task::none();
  }
  let token = panel.token.clone();
  let row = panel.row;
  let index = panel.cursors[row];
  match input {
    Input::Back => return update(state, Message::Cancel(token)),
    Input::Confirm => {
      return update(
        state,
        if row < 3 {
          Message::Select(token, row, index)
        } else if index == 1 {
          Message::Cancel(token)
        } else if matches!(panel.options, Options::Failed(_)) {
          Message::Retry(token)
        } else {
          Message::Apply(token)
        },
      )
    }
    _ => {}
  }
  let panel = state.tv.filters.panel.as_mut().expect("open panel");
  let ready = matches!(panel.options, Options::Ready(_));
  match input {
    Input::Up if ready => panel.row = row.saturating_sub(1),
    Input::Down if ready => panel.row = (row + 1).min(3),
    Input::Left => {
      panel.cursors[row] = if matches!(panel.options, Options::Loading) {
        1
      } else {
        index.saturating_sub(1)
      }
    }
    Input::Right => panel.cursors[row] = (index + 1).min(row_len(panel, row).saturating_sub(1)),
    _ => {}
  }
  reveal(state)
}

#[derive(Clone, PartialEq)]
enum Value {
  All,
  Quality(VideoLibraryQuality),
  Text(String),
}

fn values(panel: &Panel, row: usize) -> Vec<Value> {
  let Options::Ready(options) = &panel.options else {
    return Vec::new();
  };
  let mut values = vec![Value::All];
  match row {
    0 => values.extend(options.qualities.iter().copied().map(Value::Quality)),
    1 => values.extend(options.countries.iter().cloned().map(Value::Text)),
    _ => values.extend(options.genres.iter().cloned().map(Value::Text)),
  }
  // A saved value may disappear from newly refreshed metadata. Keep that actual
  // previous choice visible until the user resets or replaces it.
  let selected = selected_value(&panel.applied, row);
  if selected != Value::All && !values.contains(&selected) {
    values.push(selected);
  }
  values
}

fn selected_value(filters: &VideoLibraryFilters, row: usize) -> Value {
  match row {
    0 => filters.quality.map_or(Value::All, Value::Quality),
    1 => filters.country.clone().map_or(Value::All, Value::Text),
    _ => filters.genre.clone().map_or(Value::All, Value::Text),
  }
}

fn selected_index(panel: &Panel, row: usize) -> usize {
  values(panel, row)
    .iter()
    .position(|value| value == &selected_value(&panel.draft, row))
    .unwrap_or(0)
}
fn row_len(panel: &Panel, row: usize) -> usize {
  if row == 3 {
    2
  } else {
    values(panel, row).len()
  }
}
fn scroll_id(row: usize) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-filter-options-{row}"))
}

fn option_id(row: usize, index: usize) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-filter-option-{row}-{index}"))
}

fn reveal(state: &mut State) -> Task<AppMessage> {
  use iced::advanced::widget;
  let Some(panel) = state.tv.filters.panel.as_mut() else {
    return Task::none();
  };
  if panel.row < 3 {
    let index = panel.cursors[panel.row];
    panel.offsets[panel.row] = jellypilot_core::tv_navigation::reveal_offset(
      index as f32 * 212.0,
      index as f32 * 212.0 + 200.0,
      panel.offsets[panel.row],
      688.0,
    );
  }
  let scale = style::scale(state.shell.window_size.width);
  let mut tasks: Vec<Task<AppMessage>> = panel
    .offsets
    .iter()
    .enumerate()
    .map(|(row, offset)| {
      iced::widget::operation::scroll_to(
        scroll_id(row),
        iced::widget::operation::AbsoluteOffset {
          x: offset * scale,
          y: 0.0,
        },
      )
    })
    .collect();
  struct Reveal {
    token: Token,
    row: usize,
    index: usize,
    id: widget::Id,
    target: Option<iced::Rectangle>,
    viewport: Option<(iced::Rectangle, iced::Rectangle, iced::Vector)>,
  }
  impl widget::Operation<AppMessage> for Reveal {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation<AppMessage>)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
      if id == Some(&self.id) {
        self.target = Some(bounds);
      }
    }
    fn scrollable(
      &mut self,
      id: Option<&widget::Id>,
      bounds: iced::Rectangle,
      content: iced::Rectangle,
      translation: iced::Vector,
      _state: &mut dyn widget::operation::Scrollable,
    ) {
      if id == Some(&widget::Id::new("tv-filter-panel")) {
        self.viewport = Some((bounds, content, translation));
      }
    }
    fn finish(&self) -> widget::operation::Outcome<AppMessage> {
      let (Some(target), Some((viewport, content, translation))) = (self.target, self.viewport)
      else {
        return widget::operation::Outcome::None;
      };
      let top = target.y - content.y;
      let y = jellypilot_core::tv_navigation::reveal_offset(
        top,
        top + target.height,
        translation.y,
        viewport.height,
      );
      widget::operation::Outcome::Some(dispatch(Message::Reveal(
        self.token.clone(),
        self.row,
        self.index,
        y,
      )))
    }
  }
  tasks.push(widget::operate(Reveal {
    token: panel.token.clone(),
    row: panel.row,
    index: panel.cursors[panel.row],
    id: option_id(panel.row, panel.cursors[panel.row]),
    target: None,
    viewport: None,
  }));
  Task::batch(tasks)
}

pub fn active_count(filters: &VideoLibraryFilters) -> usize {
  usize::from(filters.quality.is_some())
    + usize::from(filters.country.is_some())
    + usize::from(filters.genre.is_some())
}

fn label(state: &State, value: &Value) -> String {
  match value {
    Value::All => state.t("browse-all"),
    Value::Text(value) => value.clone(),
    Value::Quality(quality) => state.t(match quality {
      VideoLibraryQuality::Hd => "tv-filter-hd",
      VideoLibraryQuality::FullHd => "tv-filter-full-hd",
      VideoLibraryQuality::Uhd => "tv-filter-uhd",
      VideoLibraryQuality::DolbyVision => "tv-filter-dolby-vision",
    }),
  }
}

fn control<'a>(
  label: String,
  focused: bool,
  selected: bool,
  enabled: bool,
  message: Message,
  scale: f32,
  id: iced::widget::Id,
) -> Element<'a, AppMessage> {
  container(tv_focus::focus(focused && enabled, move |progress| {
    let color = style::foreground(style::PALETTE, progress, selected);
    let mut content = row![container(
      ellipsis_text(label.clone())
        .size(style::BODY * scale)
        .line_height(iced::Pixels(32.0 * scale))
        .font(HEADING_FONT)
        .color(color)
    )
    .width(Fill)]
    .spacing(8.0 * scale)
    .align_y(Alignment::Center);
    if selected {
      content = content.push(icon_with_color(
        Icon::Check,
        IconSize::Custom(20.0 * scale),
        color,
      ));
    }
    button(content)
      .width(Fill)
      .height(64.0 * scale)
      .padding([0.0, 20.0 * scale])
      .style(style::button_progress(style::PALETTE, progress, selected))
      .on_press_maybe(enabled.then(|| dispatch(message.clone())))
      .into()
  }))
  .id(id)
  .into()
}

pub fn view(state: &State) -> Element<'_, AppMessage> {
  let Some(panel) = state.tv.filters.panel.as_ref() else {
    return space().into();
  };
  let scale = style::scale(state.shell.window_size.width);
  let mut content = column![column![
    text(state.t("tv-filter-title"))
      .size(style::SECTION * scale)
      .line_height(iced::Pixels(40.0 * scale))
      .font(HEADING_FONT),
    text(state.t(if panel.draft != panel.applied {
      "tv-filter-pending"
    } else {
      "tv-filter-instructions"
    }))
    .size(style::META * scale)
    .line_height(iced::Pixels(28.0 * scale))
    .color(style::PALETTE.text.metadata)
  ]
  .spacing(8.0 * scale)]
  .spacing(24.0 * scale);
  match &panel.options {
    Options::Loading => {
      content = content.push(text(state.t("tv-loading")).size(style::BODY * scale));
    }
    Options::Failed(error) => {
      content = content.push(
        column![
          text(state.t("tv-filter-failed"))
            .size(style::BODY * scale)
            .color(style::PALETTE.colors.error),
          text(error)
            .size(style::META * scale)
            .color(style::PALETTE.text.metadata)
        ]
        .spacing(12.0 * scale),
      );
    }
    Options::Ready(options) => {
      let mut groups = Column::new().spacing(20.0 * scale);
      for (row_index, title) in ["tv-filter-quality", "tv-filter-country", "tv-filter-genre"]
        .into_iter()
        .enumerate()
      {
        let options_for_row = values(panel, row_index);
        let count = options_for_row.len();
        let start = ((panel.offsets[row_index] / 212.0).floor() as usize)
          .saturating_sub(2)
          .min(count);
        let end = (start + 9).min(count);
        let mut choices = Row::new().spacing(12.0 * scale);
        if start > 0 {
          choices = choices.push(space().width((start as f32 * 212.0 - 12.0) * scale));
        }
        for (index, value) in options_for_row.iter().enumerate().take(end).skip(start) {
          let selected = value == &selected_value(&panel.draft, row_index);
          let token = panel.token.clone();
          let button = control(
            label(state, value),
            panel.row == row_index && panel.cursors[row_index] == index,
            selected,
            true,
            Message::Select(token.clone(), row_index, index),
            scale,
            option_id(row_index, index),
          );
          choices = choices.push(
            mouse_area(container(button).width(200.0 * scale))
              .on_enter(dispatch(Message::Focus(token, row_index, index))),
          );
        }
        if count > end {
          choices = choices.push(space().width(((count - end) as f32 * 212.0 - 12.0) * scale));
        }
        let scroll_token = panel.token.clone();
        let mut group = column![
          text(state.t(title))
            .size(style::META * scale)
            .line_height(iced::Pixels(28.0 * scale))
            .color(style::PALETTE.text.metadata),
          scrollable(choices)
            .id(scroll_id(row_index))
            .on_scroll(move |viewport| dispatch(Message::Scrolled(
              scroll_token.clone(),
              row_index,
              viewport.absolute_offset().x
            )))
            .direction(scrollable::Direction::Horizontal(
              scrollable::Scrollbar::new()
            ))
            .height(64.0 * scale)
            .width(Fill)
        ]
        .spacing(8.0 * scale);
        if row_index == 0 && options.qualities.is_empty() {
          group = group.push(
            text(state.t("tv-filter-quality-unavailable"))
              .size(style::META * scale)
              .color(style::PALETTE.text.metadata),
          );
        }
        groups = groups.push(group);
      }
      content = content.push(groups);
    }
  }
  let ready = matches!(panel.options, Options::Ready(_));
  let failed = matches!(panel.options, Options::Failed(_));
  let token = panel.token.clone();
  let primary = control(
    state.t(if failed {
      "home-retry"
    } else {
      "tv-filter-apply"
    }),
    panel.row == 3 && panel.cursors[3] == 0,
    false,
    ready || failed,
    if failed {
      Message::Retry(token.clone())
    } else {
      Message::Apply(token.clone())
    },
    scale,
    option_id(3, 0),
  );
  let cancel = control(
    state.t("common-cancel"),
    panel.row == 3 && panel.cursors[3] == 1,
    false,
    true,
    Message::Cancel(token),
    scale,
    option_id(3, 1),
  );
  let hint = if panel.row < 3 {
    "tv-filter-choice-hint"
  } else if panel.cursors[3] == 1 {
    "tv-filter-cancel-hint"
  } else if failed {
    "tv-filter-retry-hint"
  } else {
    "tv-filter-apply-hint"
  };
  content = content.push(
    column![
      row![
        container(primary).width(216.0 * scale),
        container(cancel).width(160.0 * scale)
      ]
      .spacing(16.0 * scale),
      text(state.t(hint))
        .size(style::META * scale)
        .line_height(iced::Pixels(28.0 * scale))
        .color(style::PALETTE.text.metadata)
    ]
    .spacing(16.0 * scale),
  );
  let max_content_height =
    (state.shell.window_size.height - (2.0 * style::SAFE_Y + 80.0) * scale).max(1.0);
  let panel = container(
    scrollable(content)
      .id("tv-filter-panel")
      .height(iced::Length::Fit.max(max_content_height)),
  )
  .id("tv-filter-surface")
  .width(768.0 * scale)
  .height(iced::Length::Fit)
  .padding(40.0 * scale)
  .style(style::panel);
  stack![
    opaque(
      container(space())
        .width(Fill)
        .height(Fill)
        .style(style::scrim)
    ),
    container(panel)
      .width(Fill)
      .height(Fill)
      .align_x(Alignment::End)
      .padding(iced::Padding {
        top: style::SAFE_Y * scale,
        right: style::SAFE_X * scale,
        bottom: style::SAFE_Y * scale,
        left: style::SAFE_X * scale
      })
  ]
  .into()
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::LoadState;
  use jellypilot_media_server::VideoLibraryShortcut;

  fn state() -> State {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    state.shell.window_size = iced::Size::new(1920.0, 1080.0);
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.shell.destination = Destination::Library {
      library_id: "library".to_owned(),
      collection_type: "movies".to_owned(),
    };
    state.tv.destination = Some(state.shell.destination.clone());
    state.tv.focus = TvFocus::Header(2);
    state.full = Some(crate::app::state::FullUi::default());
    state.full.as_mut().expect("full").home.data.shortcuts =
      LoadState::Ready(vec![VideoLibraryShortcut {
        id: "library".to_owned(),
        name: "Library".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: None,
        artwork_image_id: None,
      }]);
    state
  }

  fn options() -> VideoLibraryFilterOptions {
    VideoLibraryFilterOptions {
      qualities: vec![VideoLibraryQuality::Hd, VideoLibraryQuality::Uhd],
      countries: vec!["Canada".to_owned(), "Japan".to_owned()],
      genres: vec!["Drama".to_owned(), "Comedy".to_owned()],
    }
  }

  fn ready(state: &mut State) -> Token {
    drop(open(state));
    let token = state
      .tv
      .filters
      .panel
      .as_ref()
      .expect("panel")
      .token
      .clone();
    drop(update(state, Message::Loaded(token.clone(), Ok(options()))));
    token
  }

  #[test]
  fn choices_are_a_draft_and_back_preserves_query_count_and_scroll() {
    let mut state = state();
    state
      .full
      .as_mut()
      .expect("full")
      .browse
      .advanced_filters
      .country = Some("Canada".to_owned());
    state.full.as_mut().expect("full").browse.view =
      jellypilot_core::browse_model::LibraryBrowseView::Empty;
    state.tv.offset = 680.0;
    let before = state
      .full
      .as_ref()
      .expect("full")
      .browse
      .advanced_filters
      .clone();
    let token = ready(&mut state);
    drop(update(&mut state, Message::Select(token.clone(), 0, 2)));
    drop(update(&mut state, Message::Select(token, 1, 2)));
    assert_ne!(
      state.tv.filters.panel.as_ref().expect("draft").draft,
      before
    );
    assert_eq!(
      state.full.as_ref().expect("full").browse.advanced_filters,
      before
    );
    assert!(matches!(
      state.full.as_ref().expect("full").browse.view,
      jellypilot_core::browse_model::LibraryBrowseView::Empty
    ));
    drop(input(&mut state, Input::Back));
    assert!(!is_open(&state));
    assert_eq!(state.tv.offset, 680.0);
    assert_eq!(state.tv.focus, TvFocus::Header(2));
    ready(&mut state);
    assert_eq!(
      state.tv.filters.panel.as_ref().expect("reopened").draft,
      before
    );
  }

  #[test]
  fn apply_commits_once_and_reset_all_only_changes_its_dimension() {
    let mut state = state();
    let token = ready(&mut state);
    drop(update(&mut state, Message::Select(token.clone(), 0, 2)));
    drop(update(&mut state, Message::Select(token.clone(), 1, 1)));
    drop(update(&mut state, Message::Select(token.clone(), 2, 2)));
    drop(update(&mut state, Message::Select(token.clone(), 1, 0)));
    assert_eq!(
      state.full.as_ref().expect("full").browse.advanced_filters,
      VideoLibraryFilters::default()
    );
    drop(update(&mut state, Message::Apply(token.clone())));
    let applied = &state.full.as_ref().expect("full").browse.advanced_filters;
    assert_eq!(applied.quality, Some(VideoLibraryQuality::Uhd));
    assert_eq!(applied.country, None);
    assert_eq!(applied.genre.as_deref(), Some("Comedy"));
    assert_eq!(state.tv.focus, TvFocus::Header(2));
    assert!(!is_open(&state));
    state.tv.offset = 240.0;
    drop(update(&mut state, Message::Apply(token)));
    assert_eq!(
      state.tv.offset, 240.0,
      "a duplicate Apply cannot refresh or reset again"
    );
  }

  #[test]
  fn late_metadata_cannot_cross_close_reopen_session_or_surface_transitions() {
    let mut state = state();
    drop(open(&mut state));
    let old = state
      .tv
      .filters
      .panel
      .as_ref()
      .expect("panel")
      .token
      .clone();
    close(&mut state);
    drop(open(&mut state));
    let current = state
      .tv
      .filters
      .panel
      .as_ref()
      .expect("panel")
      .token
      .clone();
    drop(update(&mut state, Message::Loaded(old, Ok(options()))));
    assert!(matches!(
      state.tv.filters.panel.as_ref().expect("new panel").options,
      Options::Loading
    ));
    state.tv.settings.open = true;
    reconcile(&mut state);
    assert!(!is_open(&state));
    drop(update(&mut state, Message::Loaded(current, Ok(options()))));
    assert!(!is_open(&state));
    state.tv.settings.open = false;
    drop(open(&mut state));
    let old_session = state
      .tv
      .filters
      .panel
      .as_ref()
      .expect("panel")
      .token
      .clone();
    state.kernel.request_gate.begin_login();
    drop(update(
      &mut state,
      Message::Loaded(old_session, Ok(options())),
    ));
    reconcile(&mut state);
    assert!(!is_open(&state));
    assert_eq!(
      state.full.as_ref().expect("full").browse.advanced_filters,
      VideoLibraryFilters::default()
    );
  }

  #[test]
  fn pointer_scroll_materializes_later_options_and_keyboard_returns_to_focus() {
    let mut state = state();
    let token = ready(&mut state);
    let many = VideoLibraryFilterOptions {
      qualities: vec![],
      countries: (0..100).map(|index| format!("Country {index}")).collect(),
      genres: vec![],
    };
    drop(update(&mut state, Message::Loaded(token.clone(), Ok(many))));
    drop(update(
      &mut state,
      Message::Scrolled(token.clone(), 1, 212.0 * 50.0),
    ));
    assert_eq!(
      state.tv.filters.panel.as_ref().expect("panel").offsets[1],
      212.0 * 50.0
    );
    drop(update(&mut state, Message::Focus(token, 1, 51)));
    drop(input(&mut state, Input::Right));
    let panel = state.tv.filters.panel.as_ref().expect("panel");
    assert_eq!(panel.cursors[1], 52);
    assert!(panel.offsets[1] <= 52.0 * 212.0 && panel.offsets[1] + 688.0 >= 52.0 * 212.0 + 200.0);
  }

  #[tokio::test]
  async fn candidate_panel_fits_content_and_keeps_actions_reachable_in_short_windows() {
    use iced::advanced::{renderer, renderer::Headless, widget};
    use iced::futures::StreamExt;
    use iced::{Rectangle, Size, Vector};
    use iced_runtime::user_interface::{Cache, UserInterface};

    struct Bounds {
      focus: widget::Id,
      panel: Option<Rectangle>,
      target: Option<Rectangle>,
      scroll: Option<(Rectangle, Rectangle, Vector)>,
    }
    impl widget::Operation for Bounds {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
        if id == Some(&widget::Id::new("tv-filter-surface")) {
          self.panel = Some(bounds);
        } else if id == Some(&self.focus) {
          self.target = Some(bounds);
        }
      }
      fn scrollable(
        &mut self,
        id: Option<&widget::Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        _state: &mut dyn widget::operation::Scrollable,
      ) {
        if id == Some(&widget::Id::new("tv-filter-panel")) {
          self.scroll = Some((bounds, content, translation));
        }
      }
    }

    async fn drive(
      state: &mut State,
      renderer: &mut iced::Renderer,
      mut cache: Cache,
      task: Task<AppMessage>,
    ) -> Cache {
      let mut pending = std::collections::VecDeque::from([task]);
      while let Some(task) = pending.pop_front() {
        let mut ui = UserInterface::build(view(state), state.shell.window_size, cache, renderer);
        let mut messages = Vec::new();
        if let Some(mut stream) = iced_runtime::task::into_stream(task) {
          while let Some(action) = stream.next().await {
            match action {
              iced_runtime::Action::Widget(mut operation) => {
                ui.operate(renderer, operation.as_mut());
                // The native runtime finishes the operation to deliver its measured reply.
                drop(operation.finish());
              }
              iced_runtime::Action::Output(AppMessage::Tv(super::super::Message::Filters(
                message,
              ))) => {
                messages.push(message);
              }
              _ => panic!("unexpected effect while revealing filter focus"),
            }
          }
        }
        cache = ui.into_cache();
        pending.extend(messages.into_iter().map(|message| update(state, message)));
      }
      cache
    }

    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless layout renderer");
    for size in [
      Size::new(1920.0, 1080.0),
      Size::new(1280.0, 720.0),
      Size::new(1920.0, 540.0),
      Size::new(1280.0, 360.0),
    ] {
      let mut state = state();
      state.shell.window_size = size;
      ready(&mut state);
      let scale = style::scale(size.width);
      let mut cache = Cache::default();
      // Down reaches Apply, Right reaches Cancel, and Up returns through every
      // metadata row after the outer viewport has actually scrolled.
      for key in [
        Input::Up,
        Input::Down,
        Input::Down,
        Input::Down,
        Input::Right,
        Input::Up,
        Input::Up,
        Input::Up,
      ] {
        let task = input(&mut state, key);
        cache = drive(&mut state, &mut renderer, cache, task).await;
        let panel = state.tv.filters.panel.as_ref().expect("open panel");
        let mut bounds = Bounds {
          focus: option_id(panel.row, panel.cursors[panel.row]),
          panel: None,
          target: None,
          scroll: None,
        };
        let mut ui = UserInterface::build(view(&state), size, cache, &mut renderer);
        ui.operate(&renderer, &mut bounds);
        let surface = bounds.panel.expect("filter panel");
        assert!((surface.width - 768.0 * scale).abs() < 0.1);
        assert!((surface.y - style::SAFE_Y * scale).abs() < 0.1);
        assert!(surface.y + surface.height <= size.height - style::SAFE_Y * scale + 0.1);
        let (viewport, content, translation) = bounds.scroll.expect("filter viewport");
        if size.height >= 720.0 {
          assert!((surface.height - 652.0 * scale).abs() < 0.5);
          assert!((viewport.height - content.height).abs() < 0.1);
        } else {
          assert!(content.height > viewport.height);
        }
        let target = bounds.target.expect("focused filter control");
        let top = target.y - translation.y;
        assert!(
          top >= viewport.y - 0.1,
          "focus clipped above at {size:?}: {key:?}"
        );
        assert!(
          top + target.height <= viewport.y + viewport.height + 0.1,
          "focus clipped below at {size:?}: {key:?}"
        );
        cache = ui.into_cache();
      }
    }
  }
}
