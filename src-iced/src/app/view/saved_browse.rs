//! Saved query management and the current library's complete condition summary.

use iced::widget::{
  column, container, modal, row, scrollable, space, text, text_input, Column, Id,
};
use iced::{Alignment, Element, Fill, Pixels};
use jellypilot_core::browse_model::BrowsePreferences;
use jellypilot_sdk::saved_browse::{SavedBrowseFilter, SavedBrowseId};
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{focus_tooltip, TooltipOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, FieldVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};

use super::playback_tools::{guard_scope, guarded_rows};
use crate::app::message::Message;
use crate::app::saved_browse::{self, Action, EditorKind, PendingAction, RecordAction};
use crate::app::state::State;

pub(crate) const SAVE_CURRENT_ID: &str = "saved-filters-save-current";
pub(crate) const EDITOR_INPUT_ID: &str = "saved-filters-name-input";
pub(crate) const BACK_ID: &str = "saved-filters-back";

pub(crate) fn record_id(id: SavedBrowseId, action: RecordAction) -> Id {
  let action = match action {
    RecordAction::Apply => "apply",
    RecordAction::Rename => "rename",
    RecordAction::Delete => "delete",
  };
  Id::from(format!("saved-filters-{}-{action}", id.0))
}

fn interactive(state: &State) -> bool {
  state.saved_browse.pending.is_none() && !state.kernel.sdk.content_mutations_blocked()
}

fn copy<'a>(state: &State, value: String) -> Element<'a, Message> {
  text(value)
    .size(TOKENS.font_sizes.s13)
    .color(state.palette().text.secondary)
    .wrapping(text::Wrapping::WordOrGlyph)
    .width(Fill)
    .into()
}

fn summary<'a>(state: &State, preferences: &BrowsePreferences) -> Element<'a, Message> {
  Column::with_children(
    saved_browse::conditions(state, preferences)
      .into_iter()
      .map(|(_, label, _)| copy(state, label)),
  )
  .spacing(TOKENS.spacing.s1)
  .width(Fill)
  .into()
}

pub(super) fn page(state: &State) -> Element<'_, Message> {
  let surface = &state.saved_browse;
  let back = control_button(
    Some(Icon::ChevronLeft),
    Some(state.t("common-back")),
    ButtonVariant::Text,
  )
  .id(BACK_ID)
  .min_height(40.0)
  .on_press(saved_browse::message(state, Action::Back));
  let mut body = column![
    back,
    text(state.t("saved-filters-title"))
      .font(DISPLAY_FONT)
      .size(36)
      .line_height(Pixels(40.0))
      .color(state.palette().text.heading)
      .wrapping(text::Wrapping::WordOrGlyph)
      .width(Fill),
    copy(state, state.t("saved-filters-description")),
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill);
  if let Some(error) = &surface.error {
    body = body.push(
      text(state.kernel.locale.message(error))
        .size(TOKENS.font_sizes.s13)
        .color(state.palette().colors.error)
        .wrapping(text::Wrapping::WordOrGlyph)
        .width(Fill),
    );
    if !surface.loaded
      || matches!(
        error.id(),
        "saved-filters-load-failed" | "saved-filters-stale"
      )
    {
      body = body.push(
        control_button(None, Some(state.t("common-retry")), ButtonVariant::Tonal)
          .id("saved-filters-retry")
          .min_height(40.0)
          .on_press_maybe(interactive(state).then(|| saved_browse::message(state, Action::Reload))),
      );
    }
  }
  if surface.pending.is_some() {
    body = body.push(copy(
      state,
      state.t(
        if matches!(surface.pending, Some(PendingAction::Apply(_))) {
          "saved-filters-applying"
        } else {
          "common-loading"
        },
      ),
    ));
  } else if surface.loaded && surface.records.is_empty() {
    body = body.push(copy(state, state.t("saved-filters-empty")));
  }
  let records = guarded_rows(
    saved_browse::target(state),
    surface
      .records
      .iter()
      .map(|record| (record.id.0.to_string(), record_row(state, record)))
      .collect(),
    TOKENS.spacing.s7,
  );
  body = body.push(records);
  let inset = super::browse::page_padding(SizeClass::from_width(state.shell.window_size.width));
  guard_scope(
    (
      saved_browse::target(state),
      surface
        .records
        .iter()
        .map(|record| record.id)
        .collect::<Vec<_>>(),
    ),
    scrollable(body.padding([TOKENS.spacing.s10, inset]))
      .id("saved-filters-list")
      .height(Fill)
      .width(Fill)
      .style(jellypilot_ui::theme::scrollable),
  )
}

fn record_row<'a>(state: &'a State, record: &'a SavedBrowseFilter) -> Element<'a, Message> {
  let action = |action, key, variant| {
    guard_scope(
      (saved_browse::target(state), record.id, key),
      control_button(None, Some(state.t(key)), variant)
        .id(record_id(record.id, action))
        .min_height(40.0)
        .on_press_maybe(interactive(state).then(|| {
          saved_browse::message(
            state,
            Action::Record {
              id: record.id,
              action,
            },
          )
        })),
    )
  };
  column![
    text(&record.name)
      .font(HEADING_FONT)
      .size(TOKENS.font_sizes.s20)
      .color(state.palette().text.heading)
      .wrapping(text::Wrapping::WordOrGlyph)
      .width(Fill),
    copy(
      state,
      state.format(
        "saved-filters-library",
        &[("name", record.library_name.as_str().into())]
      )
    ),
    summary(state, &record.preferences),
    row![
      action(
        RecordAction::Apply,
        "saved-filters-apply",
        ButtonVariant::Tonal
      ),
      action(
        RecordAction::Rename,
        "saved-filters-rename",
        ButtonVariant::Text
      ),
      action(
        RecordAction::Delete,
        "saved-filters-delete",
        ButtonVariant::Text
      ),
    ]
    .spacing(TOKENS.spacing.s2)
    .wrap(),
  ]
  .spacing(TOKENS.spacing.s2)
  .width(Fill)
  .into()
}

pub(super) fn save_button(state: &State) -> Element<'_, Message> {
  guard_scope(
    saved_browse::target(state),
    control_button(
      Some(Icon::Bookmark),
      Some(state.t("saved-filters-save-current")),
      ButtonVariant::Text,
    )
    .id(SAVE_CURRENT_ID)
    .min_height(40.0)
    .on_press_maybe(
      (interactive(state) && saved_browse::current_preferences(state).is_some())
        .then(|| saved_browse::message(state, Action::SaveCurrent)),
    ),
  )
}

pub(super) fn active_summary(state: &State) -> Element<'_, Message> {
  let Some(applied) = state
    .full
    .as_ref()
    .and_then(|full| full.browse.saved.as_ref())
  else {
    return space().into();
  };
  let Some(preferences) = saved_browse::current_preferences(state) else {
    return space().into();
  };
  let mut heading = column![copy(
    state,
    state.format(
      if applied.deleted {
        "saved-filters-detached"
      } else {
        "saved-filters-applied"
      },
      &[("name", applied.filter.name.as_str().into())],
    ),
  )]
  .spacing(TOKENS.spacing.s1)
  .width(Fill);
  if saved_browse::modified(state) {
    heading = heading.push(copy(state, state.t("saved-filters-modified")));
  }
  let mut any_removable = false;
  let mut conditions = Column::new().spacing(TOKENS.spacing.s1).width(Fill);
  for (condition, label, removable) in saved_browse::conditions(state, &preferences) {
    any_removable |= removable;
    if removable {
      let tooltip = state.format(
        "saved-filters-remove-condition",
        &[("name", label.as_str().into())],
      );
      let control = control_button_content(
        move |control| {
          let color = match control {
            jellypilot_ui::icons::IconControlState::Disabled => state.palette().text.muted,
            jellypilot_ui::icons::IconControlState::Hovered => state.palette().text.heading,
            jellypilot_ui::icons::IconControlState::Rest => state.palette().text.body,
          };
          row![
            text(label.clone())
              .size(TOKENS.font_sizes.s13)
              .color(color)
              .wrapping(text::Wrapping::WordOrGlyph)
              .width(Fill),
            icon_with_color(Icon::Close, IconSize::Xs, color),
          ]
          .spacing(TOKENS.spacing.s2)
          .align_y(Alignment::Center)
          .into()
        },
        ButtonVariant::Text,
      )
      .id(Id::from(format!("saved-filters-condition-{condition:?}")))
      .width(Fill)
      .min_height(40.0)
      .padding(TOKENS.spacing.s2)
      .on_press_maybe(
        interactive(state)
          .then(|| saved_browse::message(state, Action::RemoveCondition(condition))),
      );
      conditions = conditions.push(guard_scope(
        saved_browse::target(state),
        focus_tooltip(control, tooltip, TooltipOptions::default()),
      ));
    } else {
      conditions = conditions.push(container(copy(state, label)).padding(TOKENS.spacing.s2));
    }
  }
  let clear = control_button(
    None,
    Some(state.t("saved-filters-clear")),
    ButtonVariant::Text,
  )
  .id("saved-filters-clear")
  .min_height(40.0)
  .on_press_maybe(
    (interactive(state) && any_removable)
      .then(|| saved_browse::message(state, Action::ClearConditions)),
  );
  guard_scope(
    saved_browse::target(state),
    column![
      heading,
      scrollable(conditions.push(clear))
        .id("saved-filters-active-conditions")
        .height(Fill)
        .style(jellypilot_ui::theme::scrollable),
    ]
    .height(180)
    .width(Fill)
    .spacing(TOKENS.spacing.s1),
  )
}

pub(super) fn layer(state: &State) -> Element<'_, Message> {
  let Some(editor) = state
    .saved_browse
    .editor
    .as_ref()
    .filter(|_| !state.tv_mode())
  else {
    return space().into();
  };
  let (title, library, preferences) = match &editor.kind {
    EditorKind::Save(draft) => (
      "saved-filters-save-title",
      &draft.library_name,
      &draft.preferences,
    ),
    EditorKind::Rename(record) => (
      "saved-filters-rename-title",
      &record.library_name,
      &record.preferences,
    ),
  };
  let target = saved_browse::target(state);
  let enabled = interactive(state);
  let dismiss = saved_browse::message(state, Action::CloseEditor);
  let field = text_input(state.t("saved-filters-name-placeholder"), &editor.name)
    .id(EDITOR_INPUT_ID)
    .size(TOKENS.font_sizes.s14)
    .padding([TOKENS.spacing.s2_5, TOKENS.spacing.s3])
    .on_input_maybe(enabled.then_some(move |name| {
      Message::SavedBrowse(saved_browse::Message::Action {
        target: target.clone(),
        action: Action::NameChanged(name),
      })
    }))
    .on_submit_maybe(enabled.then(|| saved_browse::message(state, Action::Submit)))
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let close = focus_tooltip(
    control_button(Some(Icon::Close), None, ButtonVariant::Text)
      .id("saved-filters-editor-close")
      .width(iced::Length::Fixed(40.0))
      .min_height(40.0)
      .on_press(dismiss.clone()),
    state.t("common-close"),
    TooltipOptions::default(),
  );
  let heading = row![
    text(state.t(title))
      .font(HEADING_FONT)
      .size(TOKENS.font_sizes.s20)
      .wrapping(text::Wrapping::WordOrGlyph)
      .width(Fill),
    close,
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center);
  let mut body = column![
    copy(state, state.t("saved-filters-name")),
    field,
    copy(
      state,
      state.format(
        "saved-filters-library",
        &[("name", library.as_str().into())]
      )
    ),
    summary(state, preferences),
  ]
  .spacing(TOKENS.spacing.s2)
  .width(Fill);
  if let Some(error) = &editor.error {
    body = body.push(
      text(state.kernel.locale.message(error))
        .size(TOKENS.font_sizes.s13)
        .color(state.palette().colors.error)
        .wrapping(text::Wrapping::WordOrGlyph)
        .width(Fill),
    );
  }
  if state.saved_browse.pending.is_some() {
    body = body.push(copy(state, state.t("common-loading")));
  }
  let footer = row![
    control_button(None, Some(state.t("common-cancel")), ButtonVariant::Text)
      .id("saved-filters-editor-cancel")
      .min_height(40.0)
      .width(Fill)
      .on_press(dismiss.clone()),
    control_button(None, Some(state.t("common-save")), ButtonVariant::Primary)
      .id("saved-filters-editor-save")
      .min_height(40.0)
      .width(Fill)
      .on_press_maybe(enabled.then(|| saved_browse::message(state, Action::Submit))),
  ]
  .spacing(TOKENS.spacing.s2);
  let panel = guard_scope(
    saved_browse::target(state),
    container(
      column![
        heading,
        scrollable(body)
          .id("saved-filters-editor-body")
          .height(Fill)
          .style(jellypilot_ui::theme::scrollable),
        footer,
      ]
      .height(Fill)
      .width(Fill)
      .spacing(TOKENS.spacing.s3),
    )
    .id("saved-filters-editor-panel")
    .width((state.shell.window_size.width - 32.0).clamp(0.0, 520.0))
    .height((state.shell.window_size.height - 64.0).clamp(0.0, 560.0))
    .padding(TOKENS.spacing.s4)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Floating)),
  );
  modal(
    TOKENS.modal.backdrop_blur_sigma,
    container(super::modal_dismiss::dismissible(panel, dismiss))
      .center_x(Fill)
      .center_y(Fill)
      .style(move |_theme| container::Style {
        background: Some(state.palette().colors.surface.scale_alpha(0.6).into()),
        ..container::Style::default()
      }),
  )
}

#[cfg(test)]
mod tests {
  use super::super::viewing_queue::tests::click;
  use super::*;
  use iced::advanced::{renderer::Headless, widget};
  use iced::{mouse, Event, Size, Theme};
  use iced_runtime::user_interface::{Cache, UserInterface};
  use jellypilot_core::locale::UiLanguage;
  use jellypilot_core::watchlist::ProfileScope;
  use jellypilot_media_server::{MediaServerProvider, VideoLibraryKind, VideoLibraryQuality};

  #[derive(Default)]
  struct Bounds(Vec<(Id, iced::Rectangle)>, Vec<(Id, iced::Vector)>);
  impl widget::Operation for Bounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: iced::Rectangle) {
      if let Some(id) = id {
        self.0.push((id.clone(), bounds));
      }
    }
    fn scrollable(
      &mut self,
      id: Option<&Id>,
      bounds: iced::Rectangle,
      _: iced::Rectangle,
      translation: iced::Vector,
      _: &mut dyn widget::operation::Scrollable,
    ) {
      self.container(id, bounds);
      if let Some(id) = id {
        self.1.push((id.clone(), translation));
      }
    }
  }
  impl Bounds {
    fn get(&self, id: impl Into<Id>) -> iced::Rectangle {
      let id = id.into();
      self
        .0
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .unwrap_or_else(|| panic!("missing measured widget {id:?}"))
        .1
    }
  }

  fn record(id: u64) -> SavedBrowseFilter {
    let mut preferences = BrowsePreferences::default();
    preferences.filters.quality = Some(VideoLibraryQuality::Uhd);
    preferences.filters.country = Some("A country value outside the current facet list".repeat(4));
    preferences.filters.genre = Some("ExtremelyLongUnbrokenGenreValue".repeat(8));
    SavedBrowseFilter {
      id: SavedBrowseId(id),
      name: format!("{id}: {}", "Long saved filter name ".repeat(3)),
      scope: ProfileScope::new(
        MediaServerProvider::Jellyfin,
        "https://media.example.com",
        "user",
      )
      .unwrap(),
      library_id: "library".into(),
      collection_type: VideoLibraryKind::Movies,
      library_name: "A library with a complete name".into(),
      preferences,
    }
  }

  fn state() -> State {
    let mut state = crate::app::update::tests::test_state();
    state.shell.destination = crate::app::state::Destination::SavedBrowse;
    state.shell.window_size = Size::new(600.0, 900.0);
    state.saved_browse.loaded = true;
    state.saved_browse.records = vec![record(1), record(2)];
    state
  }

  fn open_editor(state: &mut State, id: SavedBrowseId) {
    let target = saved_browse::target(state);
    drop(saved_browse::update(
      state,
      saved_browse::Message::Action {
        target,
        action: Action::Record {
          id,
          action: RecordAction::Rename,
        },
      },
    ));
  }

  async fn renderer() -> iced::Renderer {
    iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .unwrap()
  }

  fn events(
    ui: &mut UserInterface<'_, Message, Theme, iced::Renderer>,
    renderer: &mut iced::Renderer,
    events: &[Event],
    cursor: mouse::Cursor,
  ) -> Vec<Message> {
    let mut bus = iced::advanced::shell::Bus::new();
    ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      events,
      cursor,
      renderer,
      &mut bus,
    );
    bus.drain().map(|(message, _)| message).collect()
  }

  fn pointer_event(pressed: bool, touch: bool, point: iced::Point) -> Event {
    if touch {
      Event::Touch(if pressed {
        iced::touch::Event::FingerPressed {
          id: iced::touch::Finger(1),
          position: point,
        }
      } else {
        iced::touch::Event::FingerLifted {
          id: iced::touch::Finger(1),
          position: point,
        }
      })
    } else {
      Event::Mouse(if pressed {
        mouse::Event::ButtonPressed(mouse::Button::Left)
      } else {
        mouse::Event::ButtonReleased(mouse::Button::Left)
      })
    }
  }

  #[tokio::test]
  async fn naming_form_keeps_actions_visible_with_long_values_in_short_windows() {
    let mut renderer = renderer().await;
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      for viewport in [
        Size::new(320.0, 280.0),
        Size::new(400.0, 480.0),
        Size::new(1280.0, 720.0),
      ] {
        let mut state = state();
        state.shell.window_size = viewport;
        state.kernel.locale = crate::i18n::Localizer::new(language);
        open_editor(&mut state, SavedBrowseId(1));
        let mut ui = UserInterface::build(layer(&state), viewport, Cache::new(), &mut renderer);
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let panel = bounds.get("saved-filters-editor-panel");
        assert!(panel.width <= (viewport.width - 32.0).min(520.0) + 0.1);
        assert!(panel.height <= (viewport.height - 64.0).min(560.0) + 0.1);
        let body = bounds.get("saved-filters-editor-body");
        let close = bounds.get("saved-filters-editor-close");
        let save = bounds.get("saved-filters-editor-save");
        assert!(body.height > 0.0 && body.y >= close.y + close.height - 0.1);
        assert!(body.y + body.height <= save.y + 0.1);
        for id in [
          "saved-filters-editor-close",
          "saved-filters-editor-cancel",
          "saved-filters-editor-save",
        ] {
          let control = bounds.get(id);
          assert!(
            control.width >= 40.0 && control.height >= 40.0,
            "small control {id}: {control:?}"
          );
          assert!(control.x >= panel.x && control.x + control.width <= panel.x + panel.width + 0.1);
          assert!(
            control.y >= panel.y && control.y + control.height <= panel.y + panel.height + 0.1
          );
        }
        assert!(matches!(
          click(&mut ui, &mut renderer, save).as_slice(),
          [Message::SavedBrowse(saved_browse::Message::Action {
            action: Action::Submit,
            ..
          })]
        ));
      }
    }
  }

  #[tokio::test]
  async fn list_replacement_or_refresh_cannot_retarget_a_pending_pointer_release() {
    let mut renderer = renderer().await;
    for touch in [false, true] {
      for replacement in [false, true] {
        let mut state = state();
        for record in &mut state.saved_browse.records {
          record.preferences = BrowsePreferences::default();
        }
        let mut ui = UserInterface::build(
          page(&state),
          state.shell.window_size,
          Cache::new(),
          &mut renderer,
        );
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let point = bounds
          .get(record_id(SavedBrowseId(1), RecordAction::Delete))
          .center();
        assert!(events(
          &mut ui,
          &mut renderer,
          &[pointer_event(true, touch, point)],
          mouse::Cursor::Available(point)
        )
        .is_empty());
        let cache = ui.into_cache();
        if replacement {
          state.saved_browse.records.swap(0, 1);
        } else {
          state.saved_browse.presentation += 1;
        }
        let mut ui =
          UserInterface::build(page(&state), state.shell.window_size, cache, &mut renderer);
        assert!(events(
          &mut ui,
          &mut renderer,
          &[pointer_event(false, touch, point)],
          mouse::Cursor::Available(point)
        )
        .is_empty());
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let id = state.saved_browse.records[0].id;
        assert!(matches!(
          click(&mut ui, &mut renderer, bounds.get(record_id(id, RecordAction::Delete))).as_slice(),
          [Message::SavedBrowse(saved_browse::Message::Action { target, action: Action::Record { id: clicked, action: RecordAction::Delete } })]
            if *clicked == id && *target == saved_browse::target(&state)
        ));
      }
    }
  }

  #[tokio::test]
  async fn record_focus_follows_identity_across_reorder_and_neighbor_removal() {
    use iced::keyboard::{key, Event as KeyEvent, Key, Location, Modifiers};
    let mut renderer = renderer().await;
    for remove_neighbor in [false, true] {
      let mut state = state();
      state.shell.window_size = Size::new(900.0, 1800.0);
      for record in &mut state.saved_browse.records {
        record.preferences = BrowsePreferences::default();
      }
      let mut ui = UserInterface::build(
        page(&state),
        state.shell.window_size,
        Cache::new(),
        &mut renderer,
      );
      ui.operate(
        &renderer,
        &mut widget::operation::focusable::focus::<()>(record_id(
          SavedBrowseId(2),
          RecordAction::Delete,
        )),
      );
      let cache = ui.into_cache();
      if remove_neighbor {
        state.saved_browse.records.remove(0);
      } else {
        state.saved_browse.records.swap(0, 1);
      }
      state.saved_browse.presentation += 1;
      let mut ui =
        UserInterface::build(page(&state), state.shell.window_size, cache, &mut renderer);
      let messages = events(
        &mut ui,
        &mut renderer,
        &[Event::Keyboard(KeyEvent::KeyPressed {
          key: Key::Named(key::Named::Enter),
          modified_key: Key::Named(key::Named::Enter),
          physical_key: key::Physical::Code(key::Code::Enter),
          location: Location::Standard,
          modifiers: Modifiers::NONE,
          text: None,
          repeat: false,
        })],
        mouse::Cursor::Unavailable,
      );
      assert!(matches!(messages.as_slice(),
        [Message::SavedBrowse(saved_browse::Message::Action { target, action: Action::Record { id: SavedBrowseId(2), action: RecordAction::Delete } })]
          if *target == saved_browse::target(&state)));
    }
  }

  #[tokio::test]
  async fn closing_and_reopening_an_editor_retires_mouse_and_touch_submission() {
    let mut renderer = renderer().await;
    for touch in [false, true] {
      let mut state = state();
      open_editor(&mut state, SavedBrowseId(1));
      let mut ui = UserInterface::build(
        layer(&state),
        state.shell.window_size,
        Cache::new(),
        &mut renderer,
      );
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      let point = bounds.get("saved-filters-editor-save").center();
      assert!(events(
        &mut ui,
        &mut renderer,
        &[pointer_event(true, touch, point)],
        mouse::Cursor::Available(point)
      )
      .is_empty());
      let cache = ui.into_cache();
      let target = saved_browse::target(&state);
      drop(saved_browse::update(
        &mut state,
        saved_browse::Message::Action {
          target,
          action: Action::CloseEditor,
        },
      ));
      open_editor(&mut state, SavedBrowseId(2));
      let mut ui =
        UserInterface::build(layer(&state), state.shell.window_size, cache, &mut renderer);
      assert!(events(
        &mut ui,
        &mut renderer,
        &[pointer_event(false, touch, point)],
        mouse::Cursor::Available(point)
      )
      .is_empty());
      assert!(
        matches!(click(&mut ui, &mut renderer, bounds.get("saved-filters-editor-save")).as_slice(),
        [Message::SavedBrowse(saved_browse::Message::Action { target, action: Action::Submit })]
          if *target == saved_browse::target(&state))
      );
    }
  }

  #[tokio::test]
  async fn pending_forms_and_records_disable_writes_while_cancel_remains_reachable() {
    let mut renderer = renderer().await;
    let mut state = state();
    open_editor(&mut state, SavedBrowseId(1));
    state.saved_browse.pending = Some(PendingAction::Rename(SavedBrowseId(1)));
    let mut ui = UserInterface::build(
      layer(&state),
      state.shell.window_size,
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert!(click(
      &mut ui,
      &mut renderer,
      bounds.get("saved-filters-editor-save")
    )
    .is_empty());
    assert!(matches!(
      click(
        &mut ui,
        &mut renderer,
        bounds.get("saved-filters-editor-cancel")
      )
      .as_slice(),
      [Message::SavedBrowse(saved_browse::Message::Action {
        action: Action::CloseEditor,
        ..
      })]
    ));
    drop(ui);
    let mut ui = UserInterface::build(
      record_row(&state, &state.saved_browse.records[0]),
      Size::new(280.0, 1200.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    for action in [
      RecordAction::Apply,
      RecordAction::Rename,
      RecordAction::Delete,
    ] {
      let control = bounds.get(record_id(SavedBrowseId(1), action));
      assert!(control.x >= 0.0 && control.x + control.width <= 280.1);
      assert!(control.height >= 40.0);
      assert!(click(&mut ui, &mut renderer, control).is_empty());
    }
  }

  #[tokio::test]
  async fn retained_advanced_conditions_remain_removable_in_the_bounded_summary() {
    let mut renderer = renderer().await;
    let mut state = state();
    let record = record(1);
    state.shell.destination = crate::app::state::Destination::Library {
      library_id: record.library_id.clone(),
      collection_type: "movies".into(),
    };
    let browse = &mut state.full.as_mut().unwrap().browse;
    crate::app::browse::install_preferences(browse, record.preferences.clone());
    browse.saved = Some(saved_browse::AppliedFilter {
      filter: record,
      deleted: true,
    });
    let mut ui = UserInterface::build(
      active_summary(&state),
      Size::new(240.0, 180.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    let scroll = bounds.get("saved-filters-active-conditions");
    assert!(scroll.height > 0.0 && scroll.y + scroll.height <= 180.1);
    ui.operate(
      &renderer,
      &mut widget::operation::scrollable::scroll_to::<()>(
        Id::new("saved-filters-active-conditions"),
        widget::operation::scrollable::AbsoluteOffset {
          x: None,
          y: Some(10_000.0),
        },
      ),
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    let translation = bounds
      .1
      .iter()
      .find(|(id, _)| *id == Id::new("saved-filters-active-conditions"))
      .unwrap()
      .1;
    let clear = bounds.get("saved-filters-clear") - translation;
    assert!(
      scroll.contains(clear.center()),
      "scrolling exposes Clear within the summary viewport"
    );
    assert!(matches!(
      click(&mut ui, &mut renderer, clear).as_slice(),
      [Message::SavedBrowse(saved_browse::Message::Action {
        action: Action::ClearConditions,
        ..
      })]
    ));
  }
}
