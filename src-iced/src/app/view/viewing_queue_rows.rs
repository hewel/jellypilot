//! Stable queue row state and cancellation of pointer gestures across queue edits.

use iced::widget::Widget as _;
use std::collections::HashMap;

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};
use jellypilot_core::{request_gate::SessionToken, viewing_queue::QueueEntryId};

use crate::app::{message::Message, state::State};

pub(crate) fn guard<'a>(
  state: &State,
  content: impl iced::advanced::Widget<Message, iced::Theme, iced::Renderer> + 'a,
) -> Element<'a, Message> {
  iced::advanced::Widget::_boxed(Rows {
    keys: Vec::new(),
    scope: (
      state.kernel.request_gate.current_session(),
      state.playback.view.upcoming.revision,
    ),
    content: content.boxed(),
  })
}

pub(crate) fn rows<'a>(
  state: &State,
  entries: Vec<(QueueEntryId, Element<'a, Message>)>,
  spacing: f32,
) -> Element<'a, Message> {
  let (keys, children): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
  iced::advanced::Widget::_boxed(Rows {
    keys,
    scope: (
      state.kernel.request_gate.current_session(),
      state.playback.view.upcoming.revision,
    ),
    content: iced::widget::Column::with_children(children)
      .spacing(spacing)
      .width(Length::Fill)
      .boxed(),
  })
}

struct Rows<'a> {
  keys: Vec<QueueEntryId>,
  scope: (SessionToken, u64),
  content: Element<'a, Message>,
}
struct RowState {
  keys: Vec<QueueEntryId>,
  pressed: Option<(SessionToken, u64)>,
}
impl widget::Meta for Rows<'_> {}

impl Widget<Message, Theme, iced::Renderer> for Rows<'_> {
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<RowState>()
  }
  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(RowState {
      keys: self.keys.clone(),
      pressed: None,
    })
  }
  fn diff(&mut self, tree: &mut widget::Tree) {
    if tree.children.is_empty() {
      tree.children.push(widget::Tree::new(&self.content));
    }
    let state = tree.state.downcast_mut::<RowState>();
    if state.keys != self.keys {
      // iced's keyed Column only searches insertion/removal boundaries; equal-length
      // reorders require explicitly moving each row's state with its stable identity.
      let mut previous: HashMap<_, _> = state
        .keys
        .iter()
        .copied()
        .zip(std::mem::take(&mut tree.children[0].children))
        .collect();
      tree.children[0].children = self
        .keys
        .iter()
        .map(|key| previous.remove(key).unwrap_or_else(widget::Tree::empty))
        .collect();
      state.keys.clone_from(&self.keys);
    }
    self.content.diff(&mut tree.children[0]);
  }
  fn size(&self) -> Size<Length> {
    self.content.size()
  }
  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) {
    self.content.layout(&mut tree.children[0], renderer, limits);
    tree.size = tree.children[0].size;
    tree.children[0].translation = iced::Vector::ZERO;
  }
  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self
      .content
      .operate(&mut tree.children[0], layout, viewport, renderer, operation);
  }
  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &Event,
    layout: Layout,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
  ) {
    let state = tree.state.downcast_mut::<RowState>();
    if matches!(
      event,
      Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        | Event::Touch(iced::touch::Event::FingerPressed { .. })
    ) {
      state.pressed = Some(self.scope);
    }
    let retired_release = matches!(
      event,
      Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        | Event::Touch(iced::touch::Event::FingerLifted { .. })
    ) && state
      .pressed
      .take()
      .is_some_and(|scope| scope != self.scope);
    // Deliver release without a hit so controls clear their pressed state, without
    // applying a newly rendered revision to an older pointer gesture.
    self.content.update(
      &mut tree.children[0],
      event,
      layout,
      if retired_release {
        mouse::Cursor::Unavailable
      } else {
        cursor
      },
      renderer,
      shell,
      viewport,
    );
  }
  fn draw(
    &self,
    tree: &widget::Tree,
    renderer: &mut iced::Renderer,
    theme: &Theme,
    style: &renderer::Style,
    layout: Layout,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
  ) {
    self.content.draw(
      &tree.children[0],
      renderer,
      theme,
      style,
      layout,
      cursor,
      viewport,
    );
  }
  fn mouse_interaction(
    &self,
    tree: &widget::Tree,
    layout: Layout,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
  ) -> mouse::Interaction {
    self
      .content
      .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
  }
  fn overlay<'a>(
    &'a mut self,
    tree: &'a mut widget::Tree,
    layout: Layout,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    translation: Vector,
    window: iced::Size,
  ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    self.content.overlay(
      &mut tree.children[0],
      layout,
      renderer,
      viewport,
      translation,
      window,
    )
  }
}
