//! The temporary Up next list; the current-season browser remains in `player`.

use iced::widget::{column, container, row, scrollable, space, text, Column};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_core::viewing_queue::{
  QueueEntryId, QueueMove, QueuePlacement, VIEWING_QUEUE_CAPACITY,
};
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};
use jellypilot_ui::icons::{Icon, IconSize};
use jellypilot_ui::overlay::{popover, tooltip, Placement, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::control_button::control_button;

pub(crate) use super::viewing_queue_rows::guard as queue_guard;
pub(crate) use super::viewing_queue_rows::rows as queue_rows;
use crate::app::message::{Message, PlaybackMessage};
use crate::app::playback::viewing_queue::{Action, Message as QueueMessage};
use crate::app::state::State;

fn entry_id(id: QueueEntryId, action: &str) -> iced::widget::Id {
  iced::widget::Id::from(format!("viewing-queue-{}-{action}", id.0))
}

pub(crate) fn focus_after_edit(
  state: &State,
  message: &PlaybackMessage,
) -> Option<iced::widget::Id> {
  if state.tv_mode() {
    return None;
  }
  let PlaybackMessage::ViewingQueue(QueueMessage::Edit { action, .. }) = message else {
    return None;
  };
  let entries = &state.playback.view.upcoming.entries;
  let target = match action {
    Action::Move(id, direction) => {
      let index = entries.iter().position(|entry| entry.id == *id)?;
      // Repeated keyboard activation must continue editing, never become Play now.
      let key = match direction {
        QueueMove::Up if index > 1 => "viewing-queue-up",
        QueueMove::Down if index + 2 < entries.len() => "viewing-queue-down",
        _ => return Some(iced::widget::Id::new("viewing-queue-close")),
      };
      return Some(entry_id(*id, key));
    }
    Action::Remove(id) => entries
      .iter()
      .position(|entry| entry.id == *id)
      .and_then(|index| {
        entries
          .get(index + 1)
          .or_else(|| index.checked_sub(1).and_then(|index| entries.get(index)))
          .map(|entry| entry.id)
      }),
    Action::Clear => None,
    _ => return None,
  };
  Some(target.map_or_else(
    || iced::widget::Id::new("viewing-queue-close"),
    |id| entry_id(id, "viewing-queue-remove"),
  ))
}

pub(super) fn edit(state: &State, action: Action) -> Message {
  Message::Playback(PlaybackMessage::ViewingQueue(QueueMessage::Edit {
    session: state.kernel.request_gate.current_session(),
    revision: state.playback.view.upcoming.revision,
    action,
  }))
}

pub(super) fn add(
  state: &State,
  item: Playable,
  position: PlaybackStartPosition,
  placement: QueuePlacement,
) -> Message {
  edit(
    state,
    Action::Insert {
      item: Box::new(item),
      position,
      placement,
    },
  )
}

pub(super) fn trigger(state: &State, icon_only: bool) -> Element<'_, Message> {
  let button = control_button(
    Some(Icon::Playlist),
    (!icon_only).then(|| state.t("viewing-queue-open")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .min_height(40.0)
  .padding([8, 10])
  .on_press(Message::Playback(PlaybackMessage::ViewingQueue(
    QueueMessage::Open,
  )));
  if icon_only {
    tooltip(
      button.width(Length::Fixed(40.0)),
      state.t("viewing-queue-open"),
      TooltipOptions::default(),
    )
  } else {
    button.into()
  }
}

/// Kept above video and other content so the opaque floating surface remains sharp.
pub(super) fn layer(state: &State) -> Element<'_, Message> {
  if !state.playback.viewing_queue.open || state.tv_mode() {
    return space().into();
  }
  let width = (state.shell.window_size.width - 32.0).clamp(0.0, 520.0);
  container(popover(
    space().width(0).height(0),
    content(state),
    true,
    PopoverOptions {
      placement: Placement::Below,
      alignment: jellypilot_ui::overlay::Alignment::End,
      width: Some(width),
      consume_outside_press: true,
      ..PopoverOptions::default()
    },
    Message::Playback(PlaybackMessage::ViewingQueue(QueueMessage::Close)),
  ))
  .width(Fill)
  .height(Fill)
  .align_x(Alignment::End)
  .padding(16)
  .into()
}

fn content(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let view = &state.playback.view;
  let upcoming = &view.upcoming;
  let busy = upcoming.pending.is_some() || view.lifecycle.replacing;
  let controls_enabled = !busy && !state.kernel.sdk.content_mutations_blocked();
  let close = control_button(
    Some(Icon::Close),
    Some(state.t("viewing-queue-close")),
    ButtonVariant::Text,
  )
  .id("viewing-queue-close")
  .min_height(40.0)
  .on_press(Message::Playback(PlaybackMessage::ViewingQueue(
    QueueMessage::Close,
  )));
  let heading = row![
    text(state.t("viewing-queue-title")).size(20).width(Fill),
    close
  ]
  .align_y(Alignment::Center)
  .spacing(8);
  let mut rows = Column::new().spacing(12).width(Fill);
  if let Some(current) = &view.now_playing {
    rows = rows.push(
      column![
        text(state.t("viewing-queue-current"))
          .size(12)
          .color(palette.text.metadata),
        text(current.item.title.clone()).size(15)
      ]
      .spacing(4),
    );
  }
  if let Some(error) = state
    .playback
    .viewing_queue
    .error
    .as_ref()
    .or(state.playback.notice.as_ref())
  {
    rows = rows.push(
      text(state.kernel.locale.message(error))
        .size(13)
        .color(palette.colors.error),
    );
  }
  if busy {
    rows = rows.push(
      text(state.t("viewing-queue-starting"))
        .size(13)
        .color(palette.text.metadata),
    );
  }
  if upcoming.entries.is_empty() {
    rows = rows.push(
      text(state.t("viewing-queue-empty"))
        .size(14)
        .color(palette.text.metadata),
    );
  }
  let mut entries = Vec::new();
  for (index, entry) in upcoming.entries.iter().enumerate() {
    let enabled = controls_enabled;
    let position = match entry.position {
      PlaybackStartPosition::Resume => state.t("viewing-queue-resume"),
      PlaybackStartPosition::Beginning => state.t("viewing-queue-beginning"),
      PlaybackStartPosition::At(seconds) => format!(
        "{} · {}",
        state.t("viewing-queue-resume"),
        state.kernel.locale.duration(seconds)
      ),
    };
    let action = |icon, key, action, enabled: bool| {
      control_button(Some(icon), Some(state.t(key)), ButtonVariant::Text)
        .id(entry_id(entry.id, key))
        .min_height(40.0)
        .padding([8, 10])
        .on_press_maybe(enabled.then(|| edit(state, action)))
    };
    let buttons = row![
      action(
        Icon::Play,
        "viewing-queue-play",
        Action::PlayNow(entry.id),
        enabled && view.engine_available
      ),
      action(
        Icon::ArrowUp,
        "viewing-queue-up",
        Action::Move(entry.id, QueueMove::Up),
        enabled && index > 0
      ),
      action(
        Icon::ArrowDown,
        "viewing-queue-down",
        Action::Move(entry.id, QueueMove::Down),
        enabled && index + 1 < upcoming.entries.len()
      ),
      action(
        Icon::Trash,
        "viewing-queue-remove",
        Action::Remove(entry.id),
        enabled
      ),
    ]
    .spacing(4)
    .wrap();
    entries.push((
      entry.id,
      container(
        column![
          text(entry.title.clone()).size(15),
          text(position).size(12).color(palette.text.metadata),
          buttons
        ]
        .spacing(4),
      )
      .width(Fill)
      .padding([8, 0])
      .into(),
    ));
  }
  rows = rows.push(queue_rows(state, entries, 0.0));
  if upcoming.entries.len() >= VIEWING_QUEUE_CAPACITY {
    rows = rows.push(
      text(state.t("viewing-queue-full"))
        .size(13)
        .color(palette.text.metadata),
    );
  }
  let clear = queue_guard(
    state,
    control_button(
      Some(Icon::Trash),
      Some(state.t("viewing-queue-clear")),
      ButtonVariant::Text,
    )
    .id("viewing-queue-clear")
    .min_height(40.0)
    .on_press_maybe(
      (controls_enabled && !upcoming.entries.is_empty()).then(|| edit(state, Action::Clear)),
    ),
  );
  // Popover supplies its own padding; include that in the panel height budget.
  let height =
    ((state.shell.window_size.height - 64.0).clamp(0.0, 640.0) - 2.0 * TOKENS.spacing.s3).max(0.0);
  container(
    column![
      heading,
      scrollable(rows)
        .id("viewing-queue-list")
        .height(Length::Fill)
        .direction(iced::widget::scrollable::Direction::Vertical(
          iced::widget::scrollable::Scrollbar::new()
        )),
      clear
    ]
    .spacing(TOKENS.spacing.s2)
    .height(height)
    .width(Fill),
  )
  .id("viewing-queue-content")
  .into()
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  use iced::advanced::{renderer::Headless, widget, widget::operation::focusable};
  use iced::keyboard::{key, Event as KeyEvent, Key, Location, Modifiers};
  use iced::{Event, Rectangle, Size};
  use iced_runtime::user_interface::{Cache, UserInterface};
  use jellypilot_mpv::playback_session::UpcomingQueueEntry;

  #[derive(Default)]
  pub(crate) struct Bounds(pub Vec<(widget::Id, Rectangle)>, pub Option<widget::Id>);
  impl widget::Operation for Bounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
      if let Some(id) = id {
        self.0.push((id.clone(), bounds));
      }
    }
    fn focusable(
      &mut self,
      id: Option<&widget::Id>,
      _: Rectangle,
      state: &mut dyn focusable::Focusable,
    ) {
      if state.is_focused() {
        self.1 = id.cloned();
      }
    }
  }
  impl Bounds {
    pub(crate) fn get(&self, id: impl Into<widget::Id>) -> Rectangle {
      let id = id.into();
      self
        .0
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .expect("control in widget tree")
        .1
    }
  }
  pub(crate) fn click(
    ui: &mut UserInterface<'_, Message, iced::Theme, iced::Renderer>,
    renderer: &mut iced::Renderer,
    bounds: Rectangle,
  ) -> Vec<Message> {
    let mut bus = iced::advanced::shell::Bus::new();
    ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[
        Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        Event::Mouse(iced::mouse::Event::ButtonReleased(
          iced::mouse::Button::Left,
        )),
      ],
      iced::mouse::Cursor::Available(bounds.center()),
      renderer,
      &mut bus,
    );
    bus.drain().map(|(message, _)| message).collect()
  }
  fn state() -> State {
    let mut state = State::boot(false);
    state.playback.view.engine_available = true;
    state.playback.viewing_queue.open = true;
    state.playback.view.upcoming.revision = 31;
    state.playback.view.upcoming.entries = [1, 2, 3]
      .map(|id| UpcomingQueueEntry {
        id: QueueEntryId(id),
        item_id: format!("movie-{id}"),
        title: format!("A very long movie title with enough words to wrap onto a second line {id}"),
        item_type: "Movie".into(),
        position: PlaybackStartPosition::Resume,
        artwork_image_id: None,
      })
      .into();
    state
  }
  async fn renderer() -> iced::Renderer {
    iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .unwrap()
  }

  #[tokio::test]
  async fn short_narrow_popover_preserves_targets_close_and_scroll_containment() {
    let mut renderer = renderer().await;
    for language in [
      jellypilot_core::locale::UiLanguage::English,
      jellypilot_core::locale::UiLanguage::SimplifiedChinese,
    ] {
      for viewport in [
        Size::new(360.0, 320.0),
        Size::new(400.0, 480.0),
        Size::new(1280.0, 720.0),
      ] {
        let mut state = state();
        state.shell.window_size = viewport;
        state.kernel.locale = crate::i18n::Localizer::new(language);
        let mut ui = UserInterface::build(layer(&state), viewport, Cache::new(), &mut renderer);
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let close = bounds.get("viewing-queue-close");
        let clear = bounds.get("viewing-queue-clear");
        assert!(
          bounds.get("viewing-queue-content").height + 2.0 * TOKENS.spacing.s3
            <= (viewport.height - 64.0).min(640.0) + 0.1
        );
        for (_, control) in bounds
          .0
          .iter()
          .filter(|(id, _)| *id != widget::Id::new("viewing-queue-list"))
        {
          assert!(
            control.x >= 16.0 && control.x + control.width <= viewport.width - 15.9,
            "horizontal overflow: {control:?} in {viewport:?}"
          );
          assert!(
            control.width >= 40.0 && control.height >= 40.0,
            "target shrunk: {control:?}"
          );
        }
        assert!(close.y >= 16.0 && clear.y + clear.height <= viewport.height - 16.0);
        assert!(clear.y >= close.y + close.height);
        assert!(matches!(
          click(&mut ui, &mut renderer, close).as_slice(),
          [Message::Playback(PlaybackMessage::ViewingQueue(
            QueueMessage::Close
          ))]
        ));
      }
    }
  }

  #[tokio::test]
  async fn row_controls_capture_identity_and_keep_disabled_actions_inert() {
    let mut renderer = renderer().await;
    let mut state = state();
    state.shell.window_size = Size::new(600.0, 1000.0);
    for (id, action, enabled) in [
      (1, "viewing-queue-up", false),
      (3, "viewing-queue-down", false),
      (2, "viewing-queue-remove", true),
      (1, "viewing-queue-play", true),
    ] {
      let mut ui = UserInterface::build(
        content(&state),
        Size::new(520.0, 1000.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      let messages = click(
        &mut ui,
        &mut renderer,
        bounds.get(entry_id(QueueEntryId(id), action)),
      );
      if enabled {
        assert!(
          matches!(messages.as_slice(), [Message::Playback(PlaybackMessage::ViewingQueue(QueueMessage::Edit { session, revision: 31, action: Action::Remove(QueueEntryId(2)) | Action::PlayNow(QueueEntryId(1)) }))] if *session == state.kernel.request_gate.current_session())
        );
      } else {
        assert!(messages.is_empty());
      }
    }
    state.playback.view.engine_available = false;
    let mut ui = UserInterface::build(
      content(&state),
      Size::new(520.0, 1000.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert!(click(
      &mut ui,
      &mut renderer,
      bounds.get(entry_id(QueueEntryId(1), "viewing-queue-play"))
    )
    .is_empty());
    assert!(matches!(
      click(
        &mut ui,
        &mut renderer,
        bounds.get(entry_id(QueueEntryId(1), "viewing-queue-remove"))
      )
      .as_slice(),
      [Message::Playback(PlaybackMessage::ViewingQueue(
        QueueMessage::Edit {
          action: Action::Remove(QueueEntryId(1)),
          ..
        }
      ))]
    ));
  }

  #[tokio::test]
  async fn overlay_focus_traversal_and_reorder_follow_stable_entry_identity() {
    let mut renderer = renderer().await;
    let mut state = state();
    state.shell.window_size = Size::new(600.0, 900.0);
    let mut cache = Cache::new();
    for expected in [
      widget::Id::new("viewing-queue-close"),
      entry_id(QueueEntryId(1), "viewing-queue-play"),
    ] {
      let mut ui =
        UserInterface::build(layer(&state), state.shell.window_size, cache, &mut renderer);
      let mut operation: Box<dyn widget::Operation> = Box::new(focusable::focus_next());
      loop {
        ui.operate(&renderer, operation.as_mut());
        if let widget::operation::Outcome::Chain(next) = operation.finish() {
          operation = next;
        } else {
          break;
        }
      }
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      assert_eq!(bounds.1, Some(expected));
      cache = ui.into_cache();
    }
    state.playback.view.upcoming.entries = state
      .playback
      .view
      .upcoming
      .entries
      .iter()
      .cloned()
      .rev()
      .collect();
    let mut ui = UserInterface::build(layer(&state), state.shell.window_size, cache, &mut renderer);
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert_eq!(
      bounds.1,
      Some(entry_id(QueueEntryId(1), "viewing-queue-play"))
    );
    let mut bus = iced::advanced::shell::Bus::new();
    ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[Event::Keyboard(KeyEvent::KeyPressed {
        key: Key::Named(key::Named::Enter),
        modified_key: Key::Named(key::Named::Enter),
        physical_key: key::Physical::Code(key::Code::Enter),
        location: Location::Standard,
        modifiers: Modifiers::NONE,
        text: None,
        repeat: false,
      })],
      iced::mouse::Cursor::Unavailable,
      &mut renderer,
      &mut bus,
    );
    let messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
    assert!(matches!(
      messages.as_slice(),
      [Message::Playback(PlaybackMessage::ViewingQueue(
        QueueMessage::Edit {
          action: Action::PlayNow(QueueEntryId(1)),
          ..
        }
      ))]
    ));
    drop(ui);
    let action = PlaybackMessage::ViewingQueue(QueueMessage::Edit {
      session: state.kernel.request_gate.current_session(),
      revision: 31,
      action: Action::Remove(QueueEntryId(1)),
    });
    assert_eq!(
      focus_after_edit(&state, &action),
      Some(entry_id(QueueEntryId(2), "viewing-queue-remove"))
    );
    state.playback.view.upcoming.entries = state
      .playback
      .view
      .upcoming
      .entries
      .iter()
      .filter(|entry| entry.id == QueueEntryId(1))
      .cloned()
      .collect();
    assert_eq!(
      focus_after_edit(&state, &action),
      Some(widget::Id::new("viewing-queue-close"))
    );
  }
  #[tokio::test]
  async fn management_focus_after_reorder_or_remove_never_turns_enter_into_play() {
    let mut renderer = renderer().await;
    for (action, surviving, expected_action) in [
      (
        Action::Move(QueueEntryId(3), QueueMove::Up),
        vec![1, 3, 2],
        "up",
      ),
      (
        Action::Move(QueueEntryId(2), QueueMove::Up),
        vec![2, 1, 3],
        "close",
      ),
      (Action::Remove(QueueEntryId(2)), vec![1, 3], "remove-three"),
    ] {
      let mut state = state();
      let message = PlaybackMessage::ViewingQueue(QueueMessage::Edit {
        session: state.kernel.request_gate.current_session(),
        revision: 31,
        action,
      });
      let target = focus_after_edit(&state, &message).unwrap();
      state.playback.view.upcoming.entries = surviving
        .into_iter()
        .map(|id| {
          state
            .playback
            .view
            .upcoming
            .entries
            .iter()
            .find(|entry| entry.id == QueueEntryId(id))
            .unwrap()
            .clone()
        })
        .collect();
      state.playback.view.upcoming.revision += 1;
      let mut ui = UserInterface::build(
        content(&state),
        Size::new(520.0, 1000.0),
        Cache::new(),
        &mut renderer,
      );
      ui.operate(&renderer, &mut focusable::focus::<()>(target));
      let mut bus = iced::advanced::shell::Bus::new();
      ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[Event::Keyboard(KeyEvent::KeyPressed {
          key: Key::Named(key::Named::Enter),
          modified_key: Key::Named(key::Named::Enter),
          physical_key: key::Physical::Code(key::Code::Enter),
          location: Location::Standard,
          modifiers: Modifiers::NONE,
          text: None,
          repeat: false,
        })],
        iced::mouse::Cursor::Unavailable,
        &mut renderer,
        &mut bus,
      );
      let messages: Vec<_> = bus.drain().map(|(message, _)| message).collect();
      if expected_action == "close" {
        assert!(matches!(
          messages.as_slice(),
          [Message::Playback(PlaybackMessage::ViewingQueue(
            QueueMessage::Close
          ))]
        ));
        continue;
      }
      let [Message::Playback(PlaybackMessage::ViewingQueue(QueueMessage::Edit {
        action,
        revision: 32,
        ..
      }))] = messages.as_slice()
      else {
        panic!("focused control must emit exactly one current edit");
      };
      match expected_action {
        "up" => assert!(matches!(
          action,
          Action::Move(QueueEntryId(3), QueueMove::Up)
        )),
        _ => assert!(matches!(action, Action::Remove(QueueEntryId(3)))),
      }
    }
  }

  #[tokio::test]
  async fn clear_press_cannot_adopt_a_new_queue_revision_or_profile_on_release() {
    let mut renderer = renderer().await;
    for retire_profile in [false, true] {
      let mut state = state();
      state.shell.window_size = Size::new(600.0, 1000.0);
      let mut ui = UserInterface::build(
        content(&state),
        Size::new(520.0, 1000.0),
        Cache::new(),
        &mut renderer,
      );
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      let cursor = iced::mouse::Cursor::Available(bounds.get("viewing-queue-clear").center());
      let mut bus = iced::advanced::shell::Bus::new();
      ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[Event::Mouse(iced::mouse::Event::ButtonPressed(
          iced::mouse::Button::Left,
        ))],
        cursor,
        &mut renderer,
        &mut bus,
      );
      assert_eq!(bus.drain().count(), 0);
      let cache = ui.into_cache();
      if retire_profile {
        state.kernel.request_gate.disconnect();
      } else {
        state.playback.view.upcoming.revision += 1;
      }
      let mut ui = UserInterface::build(
        content(&state),
        Size::new(520.0, 1000.0),
        cache,
        &mut renderer,
      );
      ui.update(
        &iced::window::Headless,
        &iced::advanced::shell::Waker::noop(),
        &[Event::Mouse(iced::mouse::Event::ButtonReleased(
          iced::mouse::Button::Left,
        ))],
        cursor,
        &mut renderer,
        &mut bus,
      );
      assert_eq!(
        bus.drain().count(),
        0,
        "old press cannot clear the newly rendered queue"
      );
      let mut bounds = Bounds::default();
      ui.operate(&renderer, &mut bounds);
      assert!(matches!(
        click(&mut ui, &mut renderer, bounds.get("viewing-queue-clear")).as_slice(),
        [Message::Playback(PlaybackMessage::ViewingQueue(
          QueueMessage::Edit {
            action: Action::Clear,
            ..
          }
        ))]
      ));
    }
  }
}
