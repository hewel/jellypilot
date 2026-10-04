//! TV presentation for the shared saved-query workflow; persistence and Apply stay in the SDK route.

use iced::widget::{
  button, column, container, mouse_area, opaque, responsive, row, scrollable, space, text, Column,
};
use iced::{Alignment, Element, Fill, Length, Size, Task};
use jellypilot_core::tv_navigation::{reveal_offset, Input};
use jellypilot_sdk::saved_browse::{SavedBrowseFilter, SavedBrowseId};
use jellypilot_ui::fonts::HEADING_FONT;
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::tv_focus::focus;

use super::{text_entry, AppMessage, Focus as TvFocus, State};
use crate::app::{saved_browse as shared, state::Destination};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordAction {
  Apply,
  Rename,
  Delete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
  #[default]
  Back,
  Reload,
  Record(SavedBrowseId, RecordAction),
  Summary(SavedBrowseId, usize),
  Identity(SavedBrowseId),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FormFocus {
  #[default]
  Name,
  Summary(usize),
  Cancel,
  Submit,
  Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorIdentity {
  Save,
  Rename(SavedBrowseId),
}

#[derive(Default)]
pub struct Surface {
  serial: u64,
  records: Vec<SavedBrowseFilter>,
  presentation: Option<u64>,
  editor: Option<EditorIdentity>,
  keyboard: Option<text_entry::Surface>,
  form_focus: FormFocus,
  origin: Option<TvFocus>,
  conditions: bool,
  condition_focus: usize,
  index: usize,
  last_apply: Option<SavedBrowseId>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Scope {
  target: shared::Target,
  serial: u64,
  pending: bool,
}

#[derive(Clone)]
pub enum Message {
  Activate(Scope, Focus),
  Focus(Scope, Focus),
  Form(Scope, FormAction),
  Entry(Scope, text_entry::Message),
  Condition(Scope, usize),
  CloseConditions(Scope),
  Revealed(Scope, iced::widget::Id, iced::widget::Id, f32),
}

#[derive(Clone, Copy)]
pub enum FormAction {
  Name,
  Summary(usize),
  Cancel,
  Submit,
  Close,
}

fn message(message: Message) -> AppMessage {
  AppMessage::Tv(super::Message::SavedBrowse(message))
}
fn scope(state: &State) -> Scope {
  Scope {
    target: shared::target(state),
    serial: state.tv.saved_browse.serial,
    pending: state.saved_browse.pending.is_some(),
  }
}
fn admitted(state: &State) -> bool {
  state.tv_mode()
    && state.kernel.connection == jellypilot_auth::login::ConnectionPhase::Connected
    && state.shell.window_id.is_some()
    && !state.shell.quit_requested
    && state.shell.pending_close.is_none()
    && !crate::app::accounts::content_mutations_blocked(&state.kernel)
    && !crate::app::accounts::blocking_modal(&state.accounts)
    && !super::account::modal_open(state)
    && !state.tv.settings.open
    && !state.tv.search.open
    && !super::player::active(state)
    && !state.shell.settings_open
    && !state.shell.account_popover_open
    && !super::filters::is_open(state)
    && !super::player::upcoming::is_open(state)
}
fn route(state: &mut State, action: shared::Action) -> Task<AppMessage> {
  let message = shared::message(state, action);
  crate::app::update::route_message(state, message)
}
fn record(state: &State, id: SavedBrowseId) -> Option<&SavedBrowseFilter> {
  state
    .saved_browse
    .records
    .iter()
    .find(|record| record.id == id)
}
fn body_focus(state: &State) -> Focus {
  state
    .saved_browse
    .records
    .first()
    .map_or(Focus::Back, |record| Focus::Identity(record.id))
}
fn focused(state: &State) -> Focus {
  match state.tv.focus {
    TvFocus::SavedBrowse(focus) => focus,
    _ => body_focus(state),
  }
}
fn set_focus(state: &mut State, focus: Focus) {
  state.tv.focus = TvFocus::SavedBrowse(focus);
  state.tv.content_focus = state.tv.focus;
  if let Focus::Record(id, _) | Focus::Summary(id, _) | Focus::Identity(id) = focus {
    if let Some(index) = state
      .saved_browse
      .records
      .iter()
      .position(|record| record.id == id)
    {
      state.tv.saved_browse.index = index;
    }
  }
}

pub fn overlay_open(state: &State) -> bool {
  admitted(state) && (state.saved_browse.editor.is_some() || state.tv.saved_browse.conditions)
}

pub fn leave(state: &mut State) {
  let serial = state.tv.saved_browse.serial.wrapping_add(1);
  state.tv.saved_browse = Surface {
    serial,
    ..Surface::default()
  };
}

pub fn reconcile(state: &mut State) -> Task<AppMessage> {
  if !admitted(state) {
    if state.tv.saved_browse.conditions
      || state.tv.saved_browse.keyboard.is_some()
      || state.saved_browse.editor.is_some()
    {
      state.tv.saved_browse.serial = state.tv.saved_browse.serial.wrapping_add(1);
    }
    state.tv.saved_browse.conditions = false;
    state.tv.saved_browse.keyboard = None;
    return Task::none();
  }
  let editor = state
    .saved_browse
    .editor
    .as_ref()
    .map(|editor| match &editor.kind {
      shared::EditorKind::Save(_) => EditorIdentity::Save,
      shared::EditorKind::Rename(record) => EditorIdentity::Rename(record.id),
    });
  let records_changed = state.tv.saved_browse.records != state.saved_browse.records;
  let presentation_changed =
    state.tv.saved_browse.presentation != Some(state.saved_browse.presentation);
  let editor_changed = editor != state.tv.saved_browse.editor;
  if records_changed || presentation_changed || editor_changed {
    let surface = &mut state.tv.saved_browse;
    surface.serial = surface.serial.wrapping_add(1);
    surface.records.clone_from(&state.saved_browse.records);
    surface.presentation = Some(state.saved_browse.presentation);
    if editor_changed {
      surface.keyboard = None;
      surface.form_focus = FormFocus::Name;
    }
    if editor.is_none() && surface.editor.is_some() {
      if let Some(origin) = surface.origin.take() {
        state.tv.focus = origin;
      }
    }
    surface.editor = editor;
  }
  if !matches!(state.shell.destination, Destination::Library { .. }) {
    state.tv.saved_browse.conditions = false;
  }
  if matches!(state.shell.destination, Destination::SavedBrowse) {
    if let TvFocus::SavedBrowse(
      old @ (Focus::Record(id, _) | Focus::Summary(id, _) | Focus::Identity(id)),
    ) = state.tv.focus
    {
      if record(state, id).is_none() {
        let action = match old {
          Focus::Record(_, action) => action,
          _ => RecordAction::Apply,
        };
        let next = state
          .saved_browse
          .records
          .get(
            state
              .tv
              .saved_browse
              .index
              .min(state.saved_browse.records.len().saturating_sub(1)),
          )
          .map_or(Focus::Back, |record| Focus::Record(record.id, action));
        set_focus(state, next);
        return reveal(state);
      } else if let Some(index) = state
        .saved_browse
        .records
        .iter()
        .position(|record| record.id == id)
      {
        state.tv.saved_browse.index = index;
      }
    }
  }
  Task::none()
}

pub fn save_current(state: &mut State) -> Task<AppMessage> {
  if !admitted(state) || shared::current_preferences(state).is_none() {
    return Task::none();
  }
  state.tv.saved_browse.origin = Some(state.tv.focus);
  state.tv.saved_browse.conditions = false;
  state.tv.press = None;
  route(state, shared::Action::SaveCurrent)
}

pub fn open_conditions(state: &mut State) -> Task<AppMessage> {
  if !admitted(state) || shared::current_preferences(state).is_none() {
    return Task::none();
  }
  let surface = &mut state.tv.saved_browse;
  surface.conditions = true;
  surface.serial = surface.serial.wrapping_add(1);
  surface.condition_focus = 0;
  surface.origin = Some(state.tv.focus);
  state.tv.press = None;
  Task::none()
}

fn close_conditions(state: &mut State) {
  let surface = &mut state.tv.saved_browse;
  surface.conditions = false;
  surface.serial = surface.serial.wrapping_add(1);
  if let Some(origin) = surface.origin.take() {
    state.tv.focus = origin;
  }
}

pub fn conditions_label(state: &State) -> String {
  let mut label = state.t("saved-filters-conditions");
  if let Some(saved) = state
    .full
    .as_ref()
    .and_then(|full| full.browse.saved.as_ref())
  {
    label.push_str(" · ");
    if saved.deleted {
      label.push_str(&state.format(
        "saved-filters-detached",
        &[("name", saved.filter.name.clone().into())],
      ));
    } else {
      label.push_str(&saved.filter.name);
    }
    if shared::modified(state) {
      label.push_str(" · ");
      label.push_str(&state.t("saved-filters-modified"));
    }
  }
  label
}

pub fn activate(state: &mut State, focus: Focus) -> Task<AppMessage> {
  if !admitted(state) || !matches!(state.shell.destination, Destination::SavedBrowse) {
    return Task::none();
  }
  set_focus(state, focus);
  match focus {
    Focus::Back => route(state, shared::Action::Back),
    Focus::Reload if state.saved_browse.pending.is_none() => route(state, shared::Action::Reload),
    Focus::Record(id, action)
      if state.saved_browse.pending.is_none() && record(state, id).is_some() =>
    {
      let action = match action {
        RecordAction::Apply => {
          state.tv.saved_browse.last_apply = Some(id);
          shared::RecordAction::Apply
        }
        RecordAction::Rename => {
          state.tv.saved_browse.origin = Some(TvFocus::SavedBrowse(focus));
          shared::RecordAction::Rename
        }
        RecordAction::Delete => shared::RecordAction::Delete,
      };
      route(state, shared::Action::Record { id, action })
    }
    _ => Task::none(),
  }
}

fn form(state: &mut State, action: FormAction) -> Task<AppMessage> {
  if state.saved_browse.editor.is_none() {
    return Task::none();
  }
  match action {
    FormAction::Cancel | FormAction::Close => route(state, shared::Action::CloseEditor),
    FormAction::Submit if state.saved_browse.pending.is_none() => {
      state.tv.saved_browse.form_focus = FormFocus::Submit;
      route(state, shared::Action::Submit)
    }
    FormAction::Name if state.saved_browse.pending.is_none() => {
      state.tv.saved_browse.keyboard = Some(text_entry::Surface::default());
      Task::batch([text_entry::unfocus(), reveal(state)])
    }
    FormAction::Summary(index) => {
      state.tv.saved_browse.form_focus = FormFocus::Summary(index);
      reveal(state)
    }
    _ => Task::none(),
  }
}

fn apply_entry(
  state: &mut State,
  action: Option<text_entry::Action>,
  task: Task<text_entry::Message>,
) -> Task<AppMessage> {
  let receipt = scope(state);
  let operation = task.map(move |entry| message(Message::Entry(receipt.clone(), entry)));
  let effect = match action {
    Some(text_entry::Action::Changed(value)) => route(state, shared::Action::NameChanged(value)),
    Some(text_entry::Action::Done) => {
      state.tv.saved_browse.keyboard = None;
      state.tv.saved_browse.form_focus = FormFocus::Submit;
      text_entry::unfocus()
    }
    Some(text_entry::Action::Cancel) => {
      state.tv.saved_browse.keyboard = None;
      state.tv.saved_browse.form_focus = FormFocus::Name;
      text_entry::unfocus()
    }
    None => Task::none(),
  };
  Task::batch([operation, effect, reveal(state)])
}

pub fn update(state: &mut State, event: Message) -> Task<AppMessage> {
  let receipt = match &event {
    Message::Activate(scope, _)
    | Message::Focus(scope, _)
    | Message::Form(scope, _)
    | Message::Entry(scope, _)
    | Message::Condition(scope, _)
    | Message::CloseConditions(scope)
    | Message::Revealed(scope, _, _, _) => scope,
  };
  if !admitted(state) || *receipt != scope(state) {
    return Task::none();
  }
  match event {
    Message::Activate(_, focus) if !overlay_open(state) => activate(state, focus),
    Message::Focus(_, focus) if !overlay_open(state) => {
      set_focus(state, focus);
      Task::none()
    }
    Message::Form(_, action) => form(state, action),
    Message::Entry(_, entry) if state.saved_browse.pending.is_none() => {
      let Some(name) = state
        .saved_browse
        .editor
        .as_ref()
        .map(|editor| editor.name.clone())
      else {
        return Task::none();
      };
      let Some(keyboard) = state.tv.saved_browse.keyboard.as_mut() else {
        return Task::none();
      };
      let (action, task) = text_entry::update(keyboard, &name, entry);
      apply_entry(state, action, task)
    }
    Message::CloseConditions(_) => {
      close_conditions(state);
      Task::none()
    }
    Message::Condition(_, index)
      if state.tv.saved_browse.conditions && state.saved_browse.pending.is_none() =>
    {
      state.tv.saved_browse.condition_focus = index;
      let Some(preferences) = shared::current_preferences(state) else {
        return Task::none();
      };
      let conditions = shared::conditions(state, &preferences);
      if index == conditions.len() {
        return route(state, shared::Action::ClearConditions);
      }
      if let Some((condition, _, true)) = conditions.get(index) {
        return route(state, shared::Action::RemoveCondition(*condition));
      }
      Task::none()
    }
    Message::Revealed(_, id, target, y) if y.is_finite() && target == reveal_target(state).0 => {
      iced::widget::operation::scroll_to(id, iced::widget::operation::AbsoluteOffset { x: 0.0, y })
    }
    _ => Task::none(),
  }
}

pub fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  if !admitted(state) {
    return Task::none();
  }
  if state.saved_browse.editor.is_some() {
    if state.tv.saved_browse.keyboard.is_some() {
      let name = state.saved_browse.editor.as_ref().unwrap().name.clone();
      let keyboard = state.tv.saved_browse.keyboard.as_mut().unwrap();
      let (action, task) = text_entry::input(keyboard, &name, input);
      return apply_entry(state, action, task);
    }
    let focus = state.tv.saved_browse.form_focus;
    if input == Input::Back {
      return form(state, FormAction::Cancel);
    }
    if input == Input::Confirm {
      return form(
        state,
        match focus {
          FormFocus::Name => FormAction::Name,
          FormFocus::Summary(index) => FormAction::Summary(index),
          FormFocus::Cancel => FormAction::Cancel,
          FormFocus::Submit => FormAction::Submit,
          FormFocus::Close => FormAction::Close,
        },
      );
    }
    state.tv.saved_browse.form_focus = match (focus, input) {
      (FormFocus::Close, Input::Down) => FormFocus::Name,
      (FormFocus::Name, Input::Up) => FormFocus::Close,
      (FormFocus::Name, Input::Down) => FormFocus::Summary(0),
      (FormFocus::Summary(0), Input::Up) => FormFocus::Name,
      (FormFocus::Summary(index), Input::Up) => FormFocus::Summary(index - 1),
      (FormFocus::Summary(index), Input::Down) if index < 6 => FormFocus::Summary(index + 1),
      (FormFocus::Summary(_), Input::Down) => FormFocus::Submit,
      (FormFocus::Submit | FormFocus::Cancel, Input::Up) => FormFocus::Summary(6),
      (FormFocus::Submit, Input::Left) => FormFocus::Cancel,
      (FormFocus::Cancel, Input::Right) => FormFocus::Submit,
      _ => focus,
    };
    return reveal(state);
  }
  if state.tv.saved_browse.conditions {
    let count = shared::current_preferences(state).map_or(0, |preferences| {
      shared::conditions(state, &preferences).len()
    });
    let focused = state.tv.saved_browse.condition_focus;
    match input {
      Input::Back => close_conditions(state),
      Input::Confirm if focused > count => close_conditions(state),
      Input::Confirm => return update(state, Message::Condition(scope(state), focused)),
      Input::Up => state.tv.saved_browse.condition_focus = focused.saturating_sub(1),
      Input::Down => state.tv.saved_browse.condition_focus = (focused + 1).min(count + 1),
      _ => {}
    }
    return reveal(state);
  }
  if !matches!(state.shell.destination, Destination::SavedBrowse) {
    return Task::none();
  }
  let old = focused(state);
  if input == Input::Back {
    return activate(state, Focus::Back);
  }
  if input == Input::Confirm {
    return activate(state, old);
  }
  let records = &state.saved_browse.records;
  let next = match (old, input) {
    (Focus::Back, Input::Right) => Focus::Reload,
    (Focus::Reload, Input::Left) => Focus::Back,
    (Focus::Back | Focus::Reload, Input::Down) => body_focus(state),
    (Focus::Record(id, _), Input::Up) => Focus::Summary(id, 6),
    (Focus::Summary(id, index), Input::Down) if index < 6 => Focus::Summary(id, index + 1),
    (Focus::Summary(id, _), Input::Down) => Focus::Record(id, RecordAction::Apply),
    (Focus::Summary(id, index), Input::Up) if index > 0 => Focus::Summary(id, index - 1),
    (Focus::Summary(id, _), Input::Up) => Focus::Identity(id),
    (Focus::Identity(id), Input::Down) => Focus::Summary(id, 0),
    (Focus::Identity(id), Input::Up) => records
      .iter()
      .position(|record| record.id == id)
      .and_then(|index| index.checked_sub(1))
      .and_then(|index| records.get(index))
      .map_or(Focus::Back, |record| {
        Focus::Record(record.id, RecordAction::Apply)
      }),
    (Focus::Record(id, action), Input::Down) => {
      let index = records
        .iter()
        .position(|record| record.id == id)
        .unwrap_or(0);
      records
        .get(index + 1)
        .map_or(Focus::Record(id, action), |record| {
          Focus::Identity(record.id)
        })
    }
    (Focus::Record(id, RecordAction::Apply), Input::Right) => {
      Focus::Record(id, RecordAction::Rename)
    }
    (Focus::Record(id, RecordAction::Rename), Input::Right) => {
      Focus::Record(id, RecordAction::Delete)
    }
    (Focus::Record(id, RecordAction::Delete), Input::Left) => {
      Focus::Record(id, RecordAction::Rename)
    }
    (Focus::Record(id, RecordAction::Rename), Input::Left) => {
      Focus::Record(id, RecordAction::Apply)
    }
    (
      Focus::Record(_, RecordAction::Apply)
      | Focus::Summary(_, _)
      | Focus::Identity(_)
      | Focus::Back,
      Input::Left,
    ) => {
      state.tv.content_focus = state.tv.focus;
      state.tv.focus = TvFocus::Rail(
        super::navigation::rail_actions(state)
          .iter()
          .position(|action| *action == super::navigation::RailAction::SavedBrowse)
          .unwrap_or(0),
      );
      return Task::none();
    }
    _ => old,
  };
  set_focus(state, next);
  reveal(state)
}

fn control<'a>(
  label: String,
  focused: bool,
  enabled: bool,
  event: Message,
  id: iced::widget::Id,
  scale: f32,
) -> Element<'a, AppMessage> {
  container(focus(focused, move |progress| {
    button(
      text(label.clone())
        .size(style::BODY * scale)
        .color(if enabled || focused {
          style::foreground(style::PALETTE, progress, false)
        } else {
          style::PALETTE.text.muted
        }),
    )
    .padding([12.0 * scale, 20.0 * scale])
    .width(Fill)
    .height(Length::Fit.min(style::CONTROL * scale))
    .style(move |theme, status| {
      style::button_progress(style::PALETTE, progress, false)(
        theme,
        if status == button::Status::Disabled && progress > 0.0 {
          button::Status::Active
        } else {
          status
        },
      )
    })
    .on_press_maybe(enabled.then(|| message(event.clone())))
    .into()
  }))
  .width(Fill)
  .id(id)
  .into()
}
fn focus_id(focus: Focus) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-saved-{focus:?}"))
}
fn form_id(focus: FormFocus) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-saved-form-{focus:?}"))
}
fn condition_id(index: usize) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-saved-condition-{index}"))
}
fn hint<'a>(state: &State, value: &'a crate::i18n::UiText, scale: f32) -> Element<'a, AppMessage> {
  text(state.kernel.locale.message(value))
    .size(style::META * scale)
    .color(style::PALETTE.colors.error)
    .into()
}
fn library(state: &State, name: &str) -> String {
  state.format("saved-filters-library", &[("name", name.to_owned().into())])
}

pub fn view(state: &State) -> Element<'_, AppMessage> {
  let scale = style::scale(state.shell.window_size.width);
  let receipt = scope(state);
  let pending = state.saved_browse.pending.is_some();
  let active = !matches!(state.tv.focus, TvFocus::Rail(_)) && !overlay_open(state);
  let header = row![
    text(state.t("saved-filters-title"))
      .font(HEADING_FONT)
      .size(style::TITLE * scale),
    space().width(Fill),
    control(
      state.t("common-back"),
      active && focused(state) == Focus::Back,
      true,
      Message::Activate(receipt.clone(), Focus::Back),
      focus_id(Focus::Back),
      scale
    ),
    control(
      state.t("common-retry"),
      active && focused(state) == Focus::Reload,
      !pending,
      Message::Activate(receipt.clone(), Focus::Reload),
      focus_id(Focus::Reload),
      scale
    ),
  ]
  .spacing(16.0 * scale)
  .align_y(Alignment::Center);
  let mut body = column![text(state.t("saved-filters-description")).size(style::META * scale)]
    .spacing(24.0 * scale);
  if let Some(error) = &state.saved_browse.error {
    body = body.push(hint(state, error, scale));
  }
  if pending {
    body = body.push(text(state.t("common-loading")).size(style::META * scale));
  }
  if state.saved_browse.loaded && state.saved_browse.records.is_empty() {
    body = body.push(text(state.t("saved-filters-empty")).size(style::BODY * scale));
  }
  for record in &state.saved_browse.records {
    let identity = Focus::Identity(record.id);
    let mut item = column![control(
      format!("{}\n{}", record.name, library(state, &record.library_name)),
      active && focused(state) == identity,
      true,
      Message::Focus(receipt.clone(), identity),
      focus_id(identity),
      scale
    )]
    .spacing(12.0 * scale);
    for (index, (_, label, _)) in shared::conditions(state, &record.preferences)
      .into_iter()
      .enumerate()
    {
      let focus = Focus::Summary(record.id, index);
      item = item.push(control(
        label,
        active && focused(state) == focus,
        true,
        Message::Focus(receipt.clone(), focus),
        focus_id(focus),
        scale,
      ));
    }
    let mut actions = row![].spacing(16.0 * scale);
    for action in [
      RecordAction::Apply,
      RecordAction::Rename,
      RecordAction::Delete,
    ] {
      let focus = Focus::Record(record.id, action);
      let retry = action == RecordAction::Apply
        && state.tv.saved_browse.last_apply == Some(record.id)
        && state
          .saved_browse
          .error
          .as_ref()
          .is_some_and(|error| error.id() == "saved-filters-apply-failed");
      let key = if retry {
        "common-retry"
      } else {
        match action {
          RecordAction::Apply => "saved-filters-apply",
          RecordAction::Rename => "saved-filters-rename",
          RecordAction::Delete => "saved-filters-delete",
        }
      };
      actions = actions.push(
        mouse_area(control(
          state.t(key),
          active && focused(state) == focus,
          !pending,
          Message::Activate(receipt.clone(), focus),
          focus_id(focus),
          scale,
        ))
        .on_enter(message(Message::Focus(receipt.clone(), focus))),
      );
    }
    body = body.push(item.push(actions));
  }
  let body = crate::app::view::playback_tools::guard_scope(receipt, body);
  column![header, scrollable(body).id("tv-saved-manager").height(Fill)]
    .spacing(24.0 * scale)
    .height(Fill)
    .into()
}

fn editor_preferences(
  editor: &shared::Editor,
) -> (&str, &jellypilot_core::browse_model::BrowsePreferences) {
  match &editor.kind {
    shared::EditorKind::Save(draft) => (&draft.library_name, &draft.preferences),
    shared::EditorKind::Rename(record) => (&record.library_name, &record.preferences),
  }
}

pub fn overlay(state: &State) -> Option<Element<'_, AppMessage>> {
  if !overlay_open(state) {
    return None;
  }
  Some(opaque(responsive(move |bounds| {
    let scale = style::scale(bounds.width);
    if state.saved_browse.editor.is_some() {
      editor_view(state, bounds, scale)
    } else {
      conditions_view(state, bounds, scale)
    }
  })))
}

fn editor_view(state: &State, bounds: Size, scale: f32) -> Element<'_, AppMessage> {
  let editor = state.saved_browse.editor.as_ref().expect("editor overlay");
  let receipt = scope(state);
  let pending = state.saved_browse.pending.is_some();
  let keyboard = state.tv.saved_browse.keyboard.as_ref();
  let focus = state.tv.saved_browse.form_focus;
  let header = row![
    text(
      state.t(if matches!(editor.kind, shared::EditorKind::Save(_)) {
        "saved-filters-save-title"
      } else {
        "saved-filters-rename-title"
      })
    )
    .font(HEADING_FONT)
    .size(style::TITLE * scale),
    space().width(Fill),
    container(control(
      state.t("common-close"),
      keyboard.is_none() && focus == FormFocus::Close,
      true,
      Message::Form(receipt.clone(), FormAction::Close),
      form_id(FormFocus::Close),
      scale
    ))
    .width(200.0 * scale)
  ]
  .spacing(16.0 * scale)
  .align_y(Alignment::Center);
  let mut body = Column::new().spacing(20.0 * scale);
  if let Some(keyboard) = keyboard {
    let capture = receipt.clone();
    let keyboard = text_entry::view(
      keyboard,
      state.kernel.locale,
      state.t("saved-filters-name-placeholder"),
      &editor.name,
      false,
      !pending,
      scale,
    )
    .map(move |entry| message(Message::Entry(capture.clone(), entry)));
    body = body.push(if pending {
      jellypilot_ui::widgets::inert::inert(keyboard)
    } else {
      keyboard
    });
  } else {
    body = body.push(control(
      format!(
        "{}: {}",
        state.t("saved-filters-name"),
        if editor.name.is_empty() {
          state.t("saved-filters-name-placeholder")
        } else {
          editor.name.clone()
        }
      ),
      focus == FormFocus::Name,
      !pending,
      Message::Form(receipt.clone(), FormAction::Name),
      form_id(FormFocus::Name),
      scale,
    ));
    let (name, preferences) = editor_preferences(editor);
    body = body.push(text(library(state, name)).size(style::BODY * scale));
    for (index, (_, label, _)) in shared::conditions(state, preferences)
      .into_iter()
      .enumerate()
    {
      body = body.push(control(
        label,
        focus == FormFocus::Summary(index),
        true,
        Message::Form(receipt.clone(), FormAction::Summary(index)),
        form_id(FormFocus::Summary(index)),
        scale,
      ));
    }
  }
  if let Some(error) = &editor.error {
    body = body.push(hint(state, error, scale));
  }
  if pending {
    body = body.push(text(state.t("common-loading")).size(style::META * scale));
  }
  let body = crate::app::view::playback_tools::guard_scope(receipt.clone(), body);
  let footer = row![
    control(
      state.t("common-cancel"),
      keyboard.is_none() && focus == FormFocus::Cancel,
      true,
      Message::Form(receipt.clone(), FormAction::Cancel),
      form_id(FormFocus::Cancel),
      scale
    ),
    control(
      state.t(if matches!(editor.kind, shared::EditorKind::Save(_)) {
        "saved-filters-save"
      } else {
        "saved-filters-rename"
      }),
      keyboard.is_none() && focus == FormFocus::Submit,
      !pending,
      Message::Form(receipt.clone(), FormAction::Submit),
      form_id(FormFocus::Submit),
      scale
    ),
  ]
  .spacing(20.0 * scale);
  let footer = crate::app::view::playback_tools::guard_scope(receipt, footer);
  modal_frame(
    header.into(),
    body,
    Some(footer),
    bounds,
    scale,
    1280.0,
    "tv-saved-editor",
  )
}

fn conditions_view(state: &State, bounds: Size, scale: f32) -> Element<'_, AppMessage> {
  let receipt = scope(state);
  let preferences = shared::current_preferences(state).unwrap_or_default();
  let conditions = shared::conditions(state, &preferences);
  let focused = state.tv.saved_browse.condition_focus;
  let mut body = Column::new().spacing(24.0 * scale);
  if state
    .full
    .as_ref()
    .is_some_and(|full| full.browse.saved.is_some())
  {
    body = body.push(text(conditions_label(state)).size(style::BODY * scale));
  }
  for (index, (condition, label, removable)) in conditions.iter().enumerate() {
    let label = if *removable {
      format!(
        "{label}\n{}",
        state.format(
          "saved-filters-remove-condition",
          &[("name", state.t(condition_name(*condition)).into())]
        )
      )
    } else {
      label.clone()
    };
    body = body.push(control(
      label,
      focused == index,
      *removable && state.saved_browse.pending.is_none(),
      Message::Condition(receipt.clone(), index),
      condition_id(index),
      scale,
    ));
  }
  let body = crate::app::view::playback_tools::guard_scope(receipt.clone(), body);
  let header = row![
    text(state.t("saved-filters-conditions"))
      .font(HEADING_FONT)
      .size(style::SECTION * scale),
    space().width(Fill),
    container(control(
      state.t("common-close"),
      focused == conditions.len() + 1,
      true,
      Message::CloseConditions(receipt.clone()),
      condition_id(conditions.len() + 1),
      scale
    ))
    .width(180.0 * scale)
  ]
  .spacing(16.0 * scale)
  .align_y(Alignment::Center);
  let footer = control(
    state.t("saved-filters-clear"),
    focused == conditions.len(),
    conditions.iter().any(|(_, _, removable)| *removable),
    Message::Condition(receipt.clone(), conditions.len()),
    condition_id(conditions.len()),
    scale,
  );
  let footer = crate::app::view::playback_tools::guard_scope(receipt, footer);
  modal_frame(
    header.into(),
    body,
    Some(footer),
    bounds,
    scale,
    960.0,
    "tv-saved-conditions",
  )
}

fn condition_name(condition: shared::Condition) -> &'static str {
  match condition {
    shared::Condition::Sort => "saved-filters-sort",
    shared::Condition::Direction => "saved-filters-direction",
    shared::Condition::Played => "saved-filters-played",
    shared::Condition::Favorites => "saved-filters-favorites",
    shared::Condition::Quality => "saved-filters-quality",
    shared::Condition::Country => "saved-filters-country",
    shared::Condition::Genre => "saved-filters-genre",
  }
}

fn modal_frame<'a>(
  header: Element<'a, AppMessage>,
  body: Element<'a, AppMessage>,
  footer: Option<Element<'a, AppMessage>>,
  bounds: Size,
  scale: f32,
  width: f32,
  id: &'static str,
) -> Element<'a, AppMessage> {
  let gap = if bounds.height < 384.0 * scale {
    4.0
  } else if bounds.height < 480.0 * scale {
    12.0
  } else {
    24.0
  } * scale;
  let mut content = column![
    header,
    container(scrollable(body).id("tv-saved-modal-body").height(Fill))
      .id("tv-saved-modal-viewport")
      .height(Fill)
  ];
  if let Some(footer) = footer {
    content = content.push(footer);
  }
  let content = content.spacing(gap).width(Fill).height(Fill);
  container(
    container(content)
      .width(Length::Fill.max(width * scale))
      .height(Fill)
      .id(id),
  )
  .width(Fill)
  .height(Fill)
  .center_x(Fill)
  .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
  .style(style::canvas)
  .into()
}

fn reveal_target(state: &State) -> (iced::widget::Id, &'static str) {
  if state.saved_browse.editor.is_some() {
    let target = state
      .tv
      .saved_browse
      .keyboard
      .as_ref()
      .map(text_entry::focused_id)
      .unwrap_or_else(|| form_id(state.tv.saved_browse.form_focus));
    (target, "tv-saved-modal-body")
  } else if state.tv.saved_browse.conditions {
    (
      condition_id(state.tv.saved_browse.condition_focus),
      "tv-saved-modal-body",
    )
  } else {
    (focus_id(focused(state)), "tv-saved-manager")
  }
}

fn reveal(state: &State) -> Task<AppMessage> {
  use iced::advanced::widget;
  let (target, scroll) = reveal_target(state);
  struct Reveal {
    scope: Scope,
    target_id: widget::Id,
    scroll_id: widget::Id,
    target: Option<iced::Rectangle>,
    viewport: Option<(iced::Rectangle, iced::Rectangle, iced::Vector)>,
  }
  impl widget::Operation<AppMessage> for Reveal {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation<AppMessage>)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
      if id == Some(&self.target_id) {
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
      if id == Some(&self.scroll_id) {
        self.viewport = Some((bounds, content, translation));
      }
    }
    fn finish(&self) -> widget::operation::Outcome<AppMessage> {
      let (Some(target), Some((viewport, content, translation))) = (self.target, self.viewport)
      else {
        return widget::operation::Outcome::None;
      };
      let top = target.y - content.y;
      widget::operation::Outcome::Some(message(Message::Revealed(
        self.scope.clone(),
        self.scroll_id.clone(),
        self.target_id.clone(),
        reveal_offset(top, top + target.height, translation.y, viewport.height),
      )))
    }
  }
  widget::operate(Reveal {
    scope: scope(state),
    target_id: target,
    scroll_id: iced::widget::Id::new(scroll),
    target: None,
    viewport: None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::{
    browse_model::BrowsePreferences, config::BrowseFilterSettings, watchlist::ProfileScope,
    LoadState,
  };
  use jellypilot_media_server::{
    MediaServerProvider, VideoLibraryFilters, VideoLibraryKind, VideoLibraryPlayedFilter,
    VideoLibraryQuality, VideoLibraryShortcut, VideoLibrarySort, VideoLibrarySortDirection,
  };

  fn preferences() -> BrowsePreferences {
    BrowsePreferences {
      sort: VideoLibrarySort::ReleaseDate,
      sort_direction: VideoLibrarySortDirection::Descending,
      played_filter: VideoLibraryPlayedFilter::Played,
      favorites_only: true,
      filters: VideoLibraryFilters {
        quality: Some(VideoLibraryQuality::Uhd),
        country: Some("A country absent from current facets".into()),
        genre: Some("A genre absent from current facets".into()),
      },
    }
  }
  fn saved(id: u64) -> SavedBrowseFilter {
    SavedBrowseFilter {
      id: SavedBrowseId(id),
      name: format!("Saved {id}"),
      scope: ProfileScope::new(
        MediaServerProvider::Jellyfin,
        "https://media.example",
        "alice",
      )
      .unwrap(),
      library_id: "library".into(),
      library_name: "Library".into(),
      collection_type: VideoLibraryKind::Movies,
      preferences: preferences(),
    }
  }
  fn state() -> State {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    state.shell.window_id = Some(iced::window::Id::unique());
    state.shell.window_size = Size::new(1920.0, 1080.0);
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.shell.destination = Destination::Library {
      library_id: "library".into(),
      collection_type: "movies".into(),
    };
    state.full = Some(crate::app::state::FullUi::default());
    let full = state.full.as_mut().unwrap();
    full.home.data.shortcuts = LoadState::Ready(vec![VideoLibraryShortcut {
      id: "library".into(),
      name: "Library".into(),
      collection_type: "movies".into(),
      item_count: None,
      artwork_image_id: None,
    }]);
    full.browse.filters = Some(
      BrowseFilterSettings::default()
        .with_sort(VideoLibrarySort::ReleaseDate)
        .with_sort_direction(VideoLibrarySortDirection::Descending)
        .with_played_filter(VideoLibraryPlayedFilter::Played)
        .with_favorites_only(true),
    );
    full.browse.advanced_filters = preferences().filters;
    state.tv.destination = Some(state.shell.destination.clone());
    state.tv.focus = TvFocus::Header(4);
    state
  }

  #[test]
  fn editor_done_is_not_save_and_failure_and_nested_back_keep_the_draft() {
    let mut state = state();
    drop(save_current(&mut state));
    drop(reconcile(&mut state));
    let name = state.saved_browse.editor.as_ref().unwrap();
    assert_eq!(editor_preferences(name).1, &preferences());
    drop(form(&mut state, FormAction::Name));
    let receipt = scope(&state);
    drop(update(
      &mut state,
      Message::Entry(receipt, text_entry::Message::Changed("我的筛选".into())),
    ));
    let receipt = scope(&state);
    drop(update(
      &mut state,
      Message::Entry(receipt, text_entry::Message::Done),
    ));
    assert!(state.tv.saved_browse.keyboard.is_none());
    assert_eq!(state.tv.saved_browse.form_focus, FormFocus::Submit);
    assert!(state.saved_browse.pending.is_none());
    assert!(state.saved_browse.records.is_empty());
    state.saved_browse.pending = Some(shared::PendingAction::Save);
    state.saved_browse.presentation += 1;
    drop(reconcile(&mut state));
    assert_eq!(state.tv.saved_browse.form_focus, FormFocus::Submit);
    state.saved_browse.pending = None;
    state.saved_browse.presentation += 1;
    state.saved_browse.editor.as_mut().unwrap().error =
      Some(crate::i18n::UiText::new("saved-filters-save-failed"));
    drop(reconcile(&mut state));
    assert_eq!(state.saved_browse.editor.as_ref().unwrap().name, "我的筛选");
    assert_eq!(state.tv.saved_browse.form_focus, FormFocus::Submit);
    drop(form(&mut state, FormAction::Name));
    let receipt = scope(&state);
    drop(update(
      &mut state,
      Message::Entry(receipt, text_entry::Message::Native),
    ));
    drop(input(&mut state, Input::Back));
    assert!(state.tv.saved_browse.keyboard.is_some());
    drop(input(&mut state, Input::Back));
    assert!(state.tv.saved_browse.keyboard.is_none());
    assert!(state.saved_browse.editor.is_some());
    drop(input(&mut state, Input::Back));
    assert!(state.saved_browse.editor.is_none());
    assert_eq!(
      state.shell.destination,
      Destination::Library {
        library_id: "library".into(),
        collection_type: "movies".into()
      }
    );
  }

  #[test]
  fn list_refresh_keeps_record_identity_and_delete_chooses_a_valid_neighbor() {
    let mut state = state();
    state.shell.destination = Destination::SavedBrowse;
    state.full.as_mut().unwrap().home.data.shortcuts = LoadState::Ready(vec![]);
    state.saved_browse.records = vec![saved(1), saved(2)];
    state.saved_browse.loaded = true;
    drop(reconcile(&mut state));
    set_focus(
      &mut state,
      Focus::Record(SavedBrowseId(2), RecordAction::Delete),
    );
    let old = scope(&state);
    state.saved_browse.records.swap(0, 1);
    drop(reconcile(&mut state));
    assert_eq!(
      focused(&state),
      Focus::Record(SavedBrowseId(2), RecordAction::Delete)
    );
    drop(update(
      &mut state,
      Message::Activate(old, Focus::Record(SavedBrowseId(1), RecordAction::Rename)),
    ));
    assert!(state.saved_browse.editor.is_none());
    state
      .saved_browse
      .records
      .retain(|record| record.id != SavedBrowseId(2));
    drop(reconcile(&mut state));
    assert_eq!(
      focused(&state),
      Focus::Record(SavedBrowseId(1), RecordAction::Delete)
    );
    state.saved_browse.records.clear();
    drop(reconcile(&mut state));
    assert_eq!(focused(&state), Focus::Back);
    assert!(super::super::navigation::rail_actions(&state)
      .contains(&super::super::navigation::RailAction::SavedBrowse));
  }

  #[test]
  fn retired_editor_events_cannot_write_a_reopened_name_or_cross_an_account() {
    let mut state = state();
    drop(save_current(&mut state));
    drop(reconcile(&mut state));
    drop(form(&mut state, FormAction::Name));
    let old = scope(&state);
    drop(form(&mut state, FormAction::Cancel));
    drop(reconcile(&mut state));
    drop(save_current(&mut state));
    drop(reconcile(&mut state));
    drop(form(&mut state, FormAction::Name));
    drop(update(
      &mut state,
      Message::Entry(old, text_entry::Message::Changed("stale".into())),
    ));
    assert_eq!(state.saved_browse.editor.as_ref().unwrap().name, "");
    let old = scope(&state);
    state.kernel.request_gate.disconnect();
    drop(update(
      &mut state,
      Message::Entry(old, text_entry::Message::Changed("other profile".into())),
    ));
    assert_eq!(state.saved_browse.editor.as_ref().unwrap().name, "");
    state.shell.destination = Destination::NowPlaying;
    state.playback.view.lifecycle.playback_active = true;
    assert!(
      !overlay_open(&state),
      "a retired form must not obscure incoming playback"
    );
    drop(reconcile(&mut state));
    assert!(!overlay_open(&state));
  }

  #[test]
  fn conditions_remain_complete_and_tv_header_uses_one_geometry_source() {
    let mut state = state();
    state.full.as_mut().unwrap().browse.saved = Some(shared::AppliedFilter {
      filter: saved(1),
      deleted: false,
    });
    drop(open_conditions(&mut state));
    assert_eq!(
      shared::conditions(&state, &shared::current_preferences(&state).unwrap()).len(),
      7
    );
    assert!(!shared::modified(&state));
    state.full.as_mut().unwrap().browse.advanced_filters.genre = None;
    assert!(conditions_label(&state).contains(&state.t("saved-filters-modified")));
    state
      .full
      .as_mut()
      .unwrap()
      .browse
      .saved
      .as_mut()
      .unwrap()
      .deleted = true;
    assert!(conditions_label(&state)
      .contains(&state.format("saved-filters-detached", &[("name", "Saved 1".into())])));
    close_conditions(&mut state);
    for width in [1920.0, 1280.0] {
      state.shell.window_size.width = width;
      let expected = 272.0 * style::scale(width);
      assert_eq!(
        super::super::navigation::browse_header_height(&state),
        expected
      );
      drop(super::super::navigation::sync_browse(&mut state));
      assert_eq!(
        state
          .full
          .as_ref()
          .unwrap()
          .browse
          .grid_viewport
          .unwrap()
          .offset_y,
        -expected
      );
    }
    state.tv.focus = TvFocus::Header(3);
    drop(super::super::navigation::input(&mut state, Input::Down));
    assert_eq!(state.tv.focus, TvFocus::Header(5));
    drop(super::super::navigation::input(&mut state, Input::Up));
    assert_eq!(state.tv.focus, TvFocus::Header(1));
    assert_eq!(
      shared::current_preferences(&state).unwrap().played_filter,
      VideoLibraryPlayedFilter::Played
    );
    assert!(shared::current_preferences(&state).unwrap().favorites_only);
  }

  #[tokio::test]
  async fn long_names_and_osk_keep_close_and_submit_inside_short_viewports() {
    use iced::advanced::{layout, renderer, renderer::Headless, widget, Layout};
    use jellypilot_core::locale::UiLanguage;
    let renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .unwrap();
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      for size in [
        Size::new(1920.0, 1080.0),
        Size::new(1280.0, 720.0),
        Size::new(1920.0, 320.0),
      ] {
        for keyboard in [false, true] {
          let mut state = state();
          state.shell.window_size = size;
          state.kernel.locale = crate::i18n::Localizer::new(language);
          drop(save_current(&mut state));
          drop(reconcile(&mut state));
          state.saved_browse.editor.as_mut().unwrap().name = "长名称Long name ".repeat(6);
          if keyboard {
            drop(form(&mut state, FormAction::Name));
          }
          let scale = style::scale(size.width);
          let mut page = editor_view(&state, size, scale);
          let mut tree = widget::Tree::new(&page);
          tree.diff(page.as_widget_mut());
          let node = page.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
          );
          let mut bounds = crate::app::view::viewing_queue::tests::Bounds::default();
          page
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
          let close = bounds.get(form_id(FormFocus::Close));
          let save = bounds.get(form_id(FormFocus::Submit));
          let body = bounds.get("tv-saved-modal-viewport");
          for rect in [close, save] {
            assert!(rect.height >= style::CONTROL * scale - 0.1);
            assert!(
              rect.y >= style::SAFE_Y * scale - 0.1
                && rect.y + rect.height <= size.height - style::SAFE_Y * scale + 0.1,
              "{size:?} {rect:?}"
            );
          }
          assert!(
            body.height >= style::CONTROL * scale - 0.1,
            "{size:?} {body:?}"
          );
        }
      }
    }
  }

  #[tokio::test]
  async fn old_pointer_press_cannot_activate_a_refreshed_record_or_reopened_editor() {
    use iced::advanced::{renderer, renderer::Headless, shell};
    use iced::mouse::{Button, Cursor, Event as MouseEvent};
    use iced_runtime::user_interface::{Cache, UserInterface};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .unwrap();
    let size = Size::new(1920.0, 1080.0);
    for editor in [false, true] {
      let mut state = state();
      if editor {
        drop(save_current(&mut state));
        drop(reconcile(&mut state));
      } else {
        state.shell.destination = Destination::SavedBrowse;
        state.saved_browse.records = vec![saved(1), saved(2)];
        drop(reconcile(&mut state));
      }
      let page = if editor {
        editor_view(&state, size, 1.0)
      } else {
        view(&state)
      };
      let mut ui = UserInterface::build(page, size, Cache::new(), &mut renderer);
      let mut bounds = crate::app::view::viewing_queue::tests::Bounds::default();
      ui.operate(&renderer, &mut bounds);
      let point = if editor {
        bounds.get(form_id(FormFocus::Submit)).center()
      } else {
        bounds
          .get(focus_id(Focus::Record(
            SavedBrowseId(1),
            RecordAction::Apply,
          )))
          .center()
      };
      let mut bus = shell::Bus::new();
      let _ = ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[iced::Event::Mouse(MouseEvent::ButtonPressed(Button::Left))],
        Cursor::Available(point),
        &mut renderer,
        &mut bus,
      );
      let cache = ui.into_cache();
      if editor {
        drop(form(&mut state, FormAction::Cancel));
        drop(reconcile(&mut state));
        drop(save_current(&mut state));
        drop(reconcile(&mut state));
      } else {
        state.saved_browse.records.swap(0, 1);
        drop(reconcile(&mut state));
      }
      let page = if editor {
        editor_view(&state, size, 1.0)
      } else {
        view(&state)
      };
      let mut ui = UserInterface::build(page, size, cache, &mut renderer);
      let _ = ui.update(
        &iced::window::Headless,
        &shell::Waker::noop(),
        &[iced::Event::Mouse(MouseEvent::ButtonReleased(Button::Left))],
        Cursor::Available(point),
        &mut renderer,
        &mut bus,
      );
      assert!(bus.into_iter().all(|event| !matches!(
        event,
        AppMessage::Tv(super::super::Message::SavedBrowse(
          Message::Activate(..) | Message::Form(_, FormAction::Submit)
        ))
      )));
    }
  }
}
