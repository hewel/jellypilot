//! Scroll offsets belong to navigation entries, not iced's transient widget tree.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};
use widget::operation::scrollable::{AbsoluteOffset, RelativeOffset};

use crate::app::message::Message;

#[derive(Clone, Debug, Default)]
pub(crate) struct ScrollMemory(Rc<RefCell<HashMap<widget::Id, Position>>>);

#[derive(Debug, Default)]
struct Position {
  offset: AbsoluteOffset,
  observed: Vector,
}

/// Identified scrollables are restored after layout, including responsive children.
/// Loading placeholders must not carry the corresponding ready-content IDs.
pub(crate) fn remember<'a>(
  content: Element<'a, Message>,
  memory: &ScrollMemory,
) -> Element<'a, Message> {
  Element::new(Remember {
    content,
    memory: memory.clone(),
  })
}

struct Remember<'a> {
  content: Element<'a, Message>,
  memory: ScrollMemory,
}

impl Widget<Message, Theme, iced::Renderer> for Remember<'_> {
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<ScrollMemory>()
  }

  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(self.memory.clone())
  }

  fn diff(&mut self, tree: &mut widget::Tree) {
    let previous = tree.state.downcast_mut::<ScrollMemory>();
    if !Rc::ptr_eq(&previous.0, &self.memory.0) {
      *previous = self.memory.clone();
      // Identical route shapes otherwise inherit focus, drag and scroll state.
      tree.children.clear();
    }
    tree.diff_children(std::slice::from_mut(&mut self.content));
  }

  fn size(&self) -> Size<Length> {
    self.content.as_widget().size()
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    let node = self
      .content
      .as_widget_mut()
      .layout(&mut tree.children[0], renderer, limits);
    self.content.as_widget_mut().operate(
      &mut tree.children[0],
      Layout::new(&node),
      renderer,
      &mut Positions {
        positions: &mut self.memory.0.borrow_mut(),
        mode: Mode::Restore,
      },
    );
    node
  }

  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &Event,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
  ) {
    self.content.as_widget_mut().operate(
      &mut tree.children[0],
      layout,
      renderer,
      &mut Positions {
        positions: &mut self.memory.0.borrow_mut(),
        mode: Mode::Observe,
      },
    );
    self.content.as_widget_mut().update(
      &mut tree.children[0],
      event,
      layout,
      cursor,
      renderer,
      shell,
      viewport,
    );
    self.content.as_widget_mut().operate(
      &mut tree.children[0],
      layout,
      renderer,
      &mut Positions {
        positions: &mut self.memory.0.borrow_mut(),
        mode: Mode::Capture,
      },
    );
  }

  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self.content.as_widget_mut().operate(
      &mut tree.children[0],
      layout,
      renderer,
      &mut RememberOperation {
        operation,
        positions: &mut self.memory.0.borrow_mut(),
      },
    );
  }

  fn draw(
    &self,
    tree: &widget::Tree,
    renderer: &mut iced::Renderer,
    theme: &Theme,
    style: &renderer::Style,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
  ) {
    self.content.as_widget().draw(
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
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
  ) -> mouse::Interaction {
    self.content.as_widget().mouse_interaction(
      &tree.children[0],
      layout,
      cursor,
      viewport,
      renderer,
    )
  }

  fn overlay<'a>(
    &'a mut self,
    tree: &'a mut widget::Tree,
    layout: Layout<'a>,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    translation: Vector,
  ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    self.content.as_widget_mut().overlay(
      &mut tree.children[0],
      layout,
      renderer,
      viewport,
      translation,
    )
  }
}

enum Mode {
  Restore,
  Observe,
  Capture,
}

struct Positions<'a> {
  positions: &'a mut HashMap<widget::Id, Position>,
  mode: Mode,
}

impl widget::Operation for Positions<'_> {
  fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
    visit(self);
  }

  fn scrollable(
    &mut self,
    id: Option<&widget::Id>,
    _bounds: Rectangle,
    _content: Rectangle,
    translation: Vector,
    state: &mut dyn widget::operation::Scrollable,
  ) {
    let Some(id) = id else { return };
    let position = self.positions.entry(id.clone()).or_default();
    match self.mode {
      Mode::Restore => state.scroll_to(position.offset.into()),
      Mode::Observe => position.observed = translation,
      Mode::Capture => {
        // Iced reports clamped translations, not the underlying absolute offsets.
        // Only an actual input change replaces an axis; idle/loading frames cannot
        // erase a saved offset while the content is shorter than its destination.
        if translation.x != position.observed.x {
          position.offset.x = translation.x;
        }
        if translation.y != position.observed.y {
          position.offset.y = translation.y;
        }
      }
    }
  }
}

/// Observe explicit mutations, including resetting an already-clamped viewport to
/// zero. Sampling translations alone cannot distinguish that from a read-only query.
struct RememberOperation<'a> {
  operation: &'a mut dyn widget::Operation,
  positions: &'a mut HashMap<widget::Id, Position>,
}

impl widget::Operation for RememberOperation<'_> {
  fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
    let positions = &mut *self.positions;
    self.operation.traverse(&mut |operation| {
      visit(&mut RememberOperation {
        operation,
        positions,
      })
    });
  }

  fn scrollable(
    &mut self,
    id: Option<&widget::Id>,
    bounds: Rectangle,
    content: Rectangle,
    translation: Vector,
    state: &mut dyn widget::operation::Scrollable,
  ) {
    if let Some(id) = id {
      let position = self.positions.entry(id.clone()).or_default();
      self.operation.scrollable(
        id.into(),
        bounds,
        content,
        translation,
        &mut RememberScrollable {
          state,
          position,
          bounds,
          content,
          translation,
        },
      );
    } else {
      self
        .operation
        .scrollable(id, bounds, content, translation, state);
    }
  }

  fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle) {
    self.operation.container(id, bounds);
  }
  fn focusable(
    &mut self,
    id: Option<&widget::Id>,
    bounds: Rectangle,
    state: &mut dyn widget::operation::Focusable,
  ) {
    self.operation.focusable(id, bounds, state);
  }
  fn text_input(
    &mut self,
    id: Option<&widget::Id>,
    bounds: Rectangle,
    state: &mut dyn widget::operation::TextInput,
  ) {
    self.operation.text_input(id, bounds, state);
  }
  fn text(&mut self, id: Option<&widget::Id>, bounds: Rectangle, text: &str) {
    self.operation.text(id, bounds, text);
  }
  fn custom(&mut self, id: Option<&widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
    self.operation.custom(id, bounds, state);
  }
}

struct RememberScrollable<'a> {
  state: &'a mut dyn widget::operation::Scrollable,
  position: &'a mut Position,
  bounds: Rectangle,
  content: Rectangle,
  translation: Vector,
}

impl RememberScrollable<'_> {
  fn save(&mut self, offset: AbsoluteOffset<Option<f32>>) {
    if let Some(x) = offset.x {
      self.position.offset.x = x.max(0.0);
    }
    if let Some(y) = offset.y {
      self.position.offset.y = y.max(0.0);
    }
    self.translation = Vector::new(
      self
        .position
        .offset
        .x
        .min((self.content.width - self.bounds.width).max(0.0)),
      self
        .position
        .offset
        .y
        .min((self.content.height - self.bounds.height).max(0.0)),
    );
  }
}

impl widget::operation::Scrollable for RememberScrollable<'_> {
  fn snap_to(&mut self, offset: RelativeOffset<Option<f32>>) {
    self.state.snap_to(offset);
    self.save(AbsoluteOffset {
      x: offset
        .x
        .map(|x| x.clamp(0.0, 1.0) * (self.content.width - self.bounds.width).max(0.0)),
      y: offset
        .y
        .map(|y| y.clamp(0.0, 1.0) * (self.content.height - self.bounds.height).max(0.0)),
    });
  }

  fn scroll_to(&mut self, offset: AbsoluteOffset<Option<f32>>) {
    self.state.scroll_to(offset);
    self.save(offset);
  }

  fn scroll_by(&mut self, offset: AbsoluteOffset, bounds: Rectangle, content: Rectangle) {
    self.state.scroll_by(offset, bounds, content);
    self.save(AbsoluteOffset {
      x: (content.width > bounds.width)
        .then(|| (self.translation.x + offset.x).clamp(0.0, content.width - bounds.width)),
      y: (content.height > bounds.height)
        .then(|| (self.translation.y + offset.y).clamp(0.0, content.height - bounds.height)),
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::advanced::renderer::Headless;
  use iced::widget::{column, responsive, scrollable, space};
  use iced::{Fill, Point};

  struct Page {
    renderer: iced::Renderer,
    tree: widget::Tree,
    content: Element<'static, Message>,
    node: layout::Node,
  }

  impl Page {
    fn new(memory: &ScrollMemory) -> Self {
      let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        renderer::Settings {
          font: iced::Font::DEFAULT,
          text_size: 16.0.into(),
          line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
          metrics_hinting: false,
        },
        Some("tiny-skia"),
      ))
      .expect("headless renderer");
      let mut page = Self {
        renderer,
        tree: widget::Tree::empty(),
        content: space().into(),
        node: layout::Node::new(Size::ZERO),
      };
      page.rebuild(memory, 1800.0, true);
      page
    }

    fn rebuild(&mut self, memory: &ScrollMemory, height: f32, ready: bool) {
      self.content = remember(
        responsive(move |_| {
          let horizontal = scrollable(space().width(1000).height(80))
            .id(widget::Id::new("row"))
            .direction(scrollable::Direction::Horizontal(
              scrollable::Scrollbar::new(),
            ))
            .width(Fill)
            .height(100);
          scrollable(column![horizontal, space().height(height)])
            .id(widget::Id::new(if ready { "page" } else { "loading" }))
            .width(Fill)
            .height(Fill)
        })
        .into(),
        memory,
      );
      self.tree.diff(self.content.as_widget_mut());
      self.node = self.content.as_widget_mut().layout(
        &mut self.tree,
        &self.renderer,
        &layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0)),
      );
    }

    fn operate(&mut self, operation: &mut dyn widget::Operation) {
      self.content.as_widget_mut().operate(
        &mut self.tree,
        Layout::new(&self.node),
        &self.renderer,
        operation,
      );
    }

    fn scroll_to(&mut self, id: &'static str, x: f32, y: f32) {
      self.operate(&mut widget::operation::scrollable::scroll_to(
        widget::Id::new(id),
        AbsoluteOffset {
          x: Some(x),
          y: Some(y),
        },
      ));
    }

    fn offsets(&mut self) -> HashMap<widget::Id, Vector> {
      #[derive(Default)]
      struct Probe(HashMap<widget::Id, Vector>);
      impl widget::Operation for Probe {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
          visit(self);
        }
        fn scrollable(
          &mut self,
          id: Option<&widget::Id>,
          _: Rectangle,
          _: Rectangle,
          translation: Vector,
          _: &mut dyn widget::operation::Scrollable,
        ) {
          if let Some(id) = id {
            self.0.insert(id.clone(), translation);
          }
        }
      }
      let mut probe = Probe::default();
      self.operate(&mut probe);
      probe.0
    }

    fn input(&mut self, event: Event) {
      let mut messages = iced::advanced::shell::Bus::new();
      self.content.as_widget_mut().update(
        &mut self.tree,
        &event,
        Layout::new(&self.node),
        mouse::Cursor::Available(Point::new(80.0, 150.0)),
        &self.renderer,
        &mut Shell::new(
          &iced::window::Headless,
          iced::advanced::shell::Waker::noop(),
          &mut messages,
        ),
        &Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
      );
    }
  }

  #[test]
  fn reconstructed_responsive_page_restores_vertical_and_nested_horizontal_offsets() {
    let memory = ScrollMemory::default();
    let mut page = Page::new(&memory);
    page.scroll_to("page", 0.0, 450.0);
    page.scroll_to("row", 350.0, 0.0);
    let before = page.offsets();
    page.tree = widget::Tree::empty();
    page.rebuild(&memory.clone(), 1800.0, true);
    assert_eq!(page.offsets(), before);
  }

  #[test]
  fn independent_entries_reset_reconciled_shapes_without_losing_previous_entry() {
    let first = ScrollMemory::default();
    let second = ScrollMemory::default();
    let mut page = Page::new(&first);
    page.scroll_to("page", 0.0, 450.0);
    page.rebuild(&second, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 0.0);
    page.scroll_to("page", 0.0, 120.0);
    page.rebuild(&first, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 450.0);
    page.rebuild(&second, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 120.0);
  }

  #[test]
  fn loading_and_clamped_layouts_do_not_consume_pending_restoration() {
    let memory = ScrollMemory::default();
    let mut page = Page::new(&memory);
    page.scroll_to("page", 0.0, 450.0);
    page.tree = widget::Tree::empty();
    page.rebuild(&memory, 1800.0, false);
    page.input(Event::Mouse(mouse::Event::CursorLeft));
    page.rebuild(&memory, 0.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 0.0);
    page.input(Event::Mouse(mouse::Event::CursorLeft));
    page.rebuild(&memory, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 450.0);
  }

  #[test]
  fn explicit_reset_replaces_pending_offset_even_when_viewport_is_clamped_to_zero() {
    let memory = ScrollMemory::default();
    let mut page = Page::new(&memory);
    page.scroll_to("page", 0.0, 450.0);
    page.rebuild(&memory, 0.0, true);
    page.scroll_to("page", 0.0, 0.0);
    page.tree = widget::Tree::empty();
    page.rebuild(&memory, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 0.0);
  }

  #[test]
  fn wheel_input_after_restoration_survives_reconstruction() {
    let memory = ScrollMemory::default();
    let mut page = Page::new(&memory);
    page.scroll_to("page", 0.0, 450.0);
    page.tree = widget::Tree::empty();
    page.rebuild(&memory, 1800.0, true);
    page.input(Event::Mouse(mouse::Event::WheelScrolled {
      delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -60.0 },
    }));
    page.tree = widget::Tree::empty();
    page.rebuild(&memory, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 510.0);
  }
}
