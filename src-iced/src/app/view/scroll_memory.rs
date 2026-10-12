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
  iced::advanced::Widget::_boxed(Remember {
    content,
    memory: memory.clone(),
  })
}

struct Remember<'a> {
  content: Element<'a, Message>,
  memory: ScrollMemory,
}

impl widget::Meta for Remember<'_> {}

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
    self.content.size()
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) {
    self.content.layout(&mut tree.children[0], renderer, limits);
    let child_size = tree.children[0].size;
    self.content.operate(
      &mut tree.children[0],
      Layout::new(child_size),
      &Rectangle::with_size(child_size),
      renderer,
      &mut Positions {
        positions: &mut self.memory.0.borrow_mut(),
        mode: Mode::Restore,
      },
    );
    tree.size = tree.children[0].size;
    tree.children[0].translation = iced::Vector::ZERO;
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
    self.content.operate(
      &mut tree.children[0],
      layout,
      viewport,
      renderer,
      &mut Positions {
        positions: &mut self.memory.0.borrow_mut(),
        mode: Mode::Observe,
      },
    );
    self.content.update(
      &mut tree.children[0],
      event,
      layout,
      cursor,
      renderer,
      shell,
      viewport,
    );
    self.content.operate(
      &mut tree.children[0],
      layout,
      viewport,
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
    layout: Layout,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self.content.operate(
      &mut tree.children[0],
      layout,
      viewport,
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
    bounds: Rectangle,
    content: iced::Size,
    translation: Vector,
    state: &mut dyn widget::operation::Scrollable,
  ) {
    let Some(id) = id else { return };
    // Operations expose the last presented translation for coordinate consistency.
    // Navigation memory instead samples logical offsets before/after input so an
    // on_scroll-driven rebuild cannot restore over an unpresented wheel delta.
    // Third-party scrollables without this query retain the presented fallback.
    let offset = state
      .absolute_offset(bounds, content)
      .map(|offset| Vector::new(offset.x, offset.y))
      .unwrap_or(translation);
    let position = self.positions.entry(id.clone()).or_default();
    match self.mode {
      Mode::Restore => state.scroll_to(
        position.offset.into(),
        widget::operation::Animation::Instant,
        bounds,
        content,
      ),
      Mode::Observe => position.observed = offset,
      Mode::Capture => {
        // The logical query is clamped to the current viewport, just like the
        // presented fallback. A layout change alone is not an input change.
        // Only an actual input change replaces an axis; idle/loading frames cannot
        // erase a saved offset while the content is shorter than its destination.
        if offset.x != position.observed.x {
          position.offset.x = offset.x;
        }
        if offset.y != position.observed.y {
          position.offset.y = offset.y;
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
    content: iced::Size,
    translation: Vector,
    state: &mut dyn widget::operation::Scrollable,
  ) {
    if let Some(id) = id {
      let position = self.positions.entry(id.clone()).or_default();
      let current_translation = state
        .absolute_offset(bounds, content)
        .map(|offset| Vector::new(offset.x, offset.y))
        .unwrap_or(translation);
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
          translation: current_translation,
        },
      );
    } else {
      self
        .operation
        .scrollable(id, bounds, content, translation, state);
    }
  }

  fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle, _viewport: &iced::Rectangle) {
    self.operation.container(id, bounds, _viewport);
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
  fn text(
    &mut self,
    id: Option<&widget::Id>,
    bounds: Rectangle,
    state: &mut dyn iced::advanced::widget::operation::Text,
  ) {
    self.operation.text(id, bounds, state);
  }
  fn custom(&mut self, id: Option<&widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
    self.operation.custom(id, bounds, state);
  }
}

struct RememberScrollable<'a> {
  state: &'a mut dyn widget::operation::Scrollable,
  position: &'a mut Position,
  bounds: Rectangle,
  content: Size,
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
  fn absolute_offset(&self, bounds: Rectangle, content: Size) -> Option<AbsoluteOffset> {
    self.state.absolute_offset(bounds, content)
  }

  fn snap_to(
    &mut self,
    offset: RelativeOffset<Option<f32>>,
    animation: widget::operation::Animation,
    bounds: Rectangle,
    content: Size,
  ) {
    self.state.snap_to(offset, animation, bounds, content);
    self.save(AbsoluteOffset {
      x: offset
        .x
        .map(|x| x.clamp(0.0, 1.0) * (self.content.width - self.bounds.width).max(0.0)),
      y: offset
        .y
        .map(|y| y.clamp(0.0, 1.0) * (self.content.height - self.bounds.height).max(0.0)),
    });
  }

  fn scroll_to(
    &mut self,
    offset: AbsoluteOffset<Option<f32>>,
    animation: widget::operation::Animation,
    bounds: Rectangle,
    content: Size,
  ) {
    self.state.scroll_to(offset, animation, bounds, content);
    self.save(offset);
  }

  fn scroll_by(
    &mut self,
    offset: AbsoluteOffset,
    animation: widget::operation::Animation,
    bounds: Rectangle,
    content: Size,
  ) {
    self.state.scroll_by(offset, animation, bounds, content);
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
  use iced::widget::Widget as _;
  use iced::widget::{column, responsive, scrollable, space};
  use iced::{Fill, Point};

  struct Page {
    renderer: iced::Renderer,
    tree: widget::Tree,
    content: Element<'static, Message>,
    node: crate::app::test_support::LayoutGeometry,
  }

  fn headless_renderer() -> iced::Renderer {
    iced::futures::executor::block_on(iced::Renderer::new(
      renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 16.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    ))
    .expect("headless renderer")
  }

  impl Page {
    fn new(memory: &ScrollMemory) -> Self {
      let renderer = headless_renderer();
      let mut page = Self {
        renderer,
        tree: widget::Tree::empty(),
        content: space().boxed(),
        node: crate::app::test_support::LayoutGeometry::from_tree(&widget::Tree::empty()),
      };
      page.rebuild(memory, 1800.0, true);
      page
    }

    fn rebuild(&mut self, memory: &ScrollMemory, height: f32, ready: bool) {
      self.content = remember(
        responsive(move |_| {
          let horizontal = scrollable(space().width(1000).height(80))
            .smooth_scroll(false)
            .id(widget::Id::new("row"))
            .direction(scrollable::Direction::Horizontal(
              scrollable::Scrollbar::new(),
            ))
            .width(Fill)
            .height(100);
          scrollable(column![horizontal, space().height(height)])
            .smooth_scroll(false)
            .id(widget::Id::new(if ready { "page" } else { "loading" }))
            .width(Fill)
            .height(Fill)
        })
        .boxed(),
        memory,
      );
      self.tree.diff(&mut self.content);
      self.node = crate::app::test_support::layout_widget(
        &mut self.content,
        &mut self.tree,
        &self.renderer,
        &layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0)),
      );
    }

    fn operate(&mut self, operation: &mut dyn widget::Operation) {
      self.content.operate(
        &mut self.tree,
        self.node.layout(),
        &Rectangle::with_size(self.node.size()),
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
        iced::advanced::widget::operation::Animation::Instant,
      ));
    }

    fn offsets(&mut self) -> HashMap<widget::Id, Vector> {
      self.input(Event::Window(iced::window::Event::RedrawRequested(
        iced::time::Instant::now(),
      )));
      #[derive(Default)]
      struct Probe(HashMap<widget::Id, Vector>);
      impl widget::Operation for Probe {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
          visit(self);
        }
        fn scrollable(
          &mut self,
          id: Option<&widget::Id>,
          _: iced::Rectangle,
          _: iced::Size,
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
      self.content.update(
        &mut self.tree,
        &event,
        self.node.layout(),
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
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 450.0);
    // A ready viewport can also become temporarily shorter without changing
    // identity. Committing the clamped frame must not consume its saved offset.
    page.rebuild(&memory, 0.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 0.0);
    page.rebuild(&memory, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 450.0);
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
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 450.0);
    page.input(Event::Mouse(mouse::Event::WheelScrolled {
      delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -60.0 },
    }));
    // An on_scroll message can rebuild the view before the next presentation
    // frame. The wheel delta must already belong to this navigation entry.
    page.tree = widget::Tree::empty();
    page.rebuild(&memory, 1800.0, true);
    assert_eq!(page.offsets()[&widget::Id::new("page")].y, 510.0);
  }

  fn ui_page(memory: &ScrollMemory, height: f32) -> Element<'static, Message> {
    remember(
      scrollable(space().width(Fill).height(height))
        .smooth_scroll(false)
        .id(widget::Id::new("page"))
        .on_scroll(|scroll| {
          Message::Browse(crate::app::message::BrowseMessage::Scrolled(
            scroll.viewport,
          ))
        })
        .width(Fill)
        .height(Fill)
        .boxed(),
      memory,
    )
  }

  fn presented_ui_offset(
    ui: &mut iced_runtime::user_interface::UserInterface<'_, Message, Theme, iced::Renderer>,
    renderer: &iced::Renderer,
  ) -> f32 {
    #[derive(Default)]
    struct Probe(Option<f32>);
    impl widget::Operation for Probe {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn scrollable(
        &mut self,
        id: Option<&widget::Id>,
        _bounds: Rectangle,
        _content: Size,
        translation: Vector,
        _state: &mut dyn widget::operation::Scrollable,
      ) {
        if id == Some(&widget::Id::new("page")) {
          self.0 = Some(translation.y);
        }
      }
    }
    let mut probe = Probe::default();
    ui.operate(renderer, &mut probe);
    probe.0.expect("identified page")
  }

  #[test]
  fn scroll_message_rebuild_before_next_frame_preserves_the_wheel_offset() {
    use iced_runtime::user_interface::{Cache, UserInterface};
    let memory = ScrollMemory::default();
    let mut renderer = headless_renderer();
    let bounds = Size::new(200.0, 200.0);
    let mut ui = UserInterface::build(
      ui_page(&memory, 1800.0),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    ui.operate(
      &renderer,
      &mut widget::operation::scrollable::scroll_to::<()>(
        widget::Id::new("page"),
        AbsoluteOffset {
          x: None,
          y: Some(450.0),
        },
        widget::operation::Animation::Instant,
      ),
    );
    drop(crate::app::test_support::redraw_ui(&mut ui, &mut renderer));
    assert_eq!(presented_ui_offset(&mut ui, &renderer), 450.0);

    let mut bus = iced::advanced::shell::Bus::new();
    ui.update(
      &iced::window::Headless,
      &iced::advanced::shell::Waker::noop(),
      &[Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -60.0 },
      })],
      mouse::Cursor::Available(Point::new(80.0, 150.0)),
      &mut renderer,
      &mut bus,
    );
    let requested_offset = bus.drain().find_map(|(message, _)| match message {
      Message::Browse(crate::app::message::BrowseMessage::Scrolled(viewport)) => {
        Some(viewport.absolute_offset().y)
      }
      _ => None,
    });
    assert_eq!(
      requested_offset,
      Some(510.0),
      "wheel emitted the real application viewport message"
    );
    // The runtime may handle on_scroll and rebuild the view before another
    // RedrawRequested. Reuse its cache, rather than committing a synthetic frame.
    let mut ui = UserInterface::build(
      ui_page(&memory, 1800.0),
      bounds,
      ui.into_cache(),
      &mut renderer,
    );
    drop(crate::app::test_support::redraw_ui(&mut ui, &mut renderer));
    assert_eq!(presented_ui_offset(&mut ui, &renderer), 510.0);
  }

  #[test]
  fn committed_clamped_ui_frame_preserves_the_ready_pages_pending_offset() {
    use iced_runtime::user_interface::{Cache, UserInterface};
    let memory = ScrollMemory::default();
    let mut renderer = headless_renderer();
    let bounds = Size::new(200.0, 200.0);
    let mut ui = UserInterface::build(
      ui_page(&memory, 1800.0),
      bounds,
      Cache::new(),
      &mut renderer,
    );
    ui.operate(
      &renderer,
      &mut widget::operation::scrollable::scroll_to::<()>(
        widget::Id::new("page"),
        AbsoluteOffset {
          x: None,
          y: Some(450.0),
        },
        widget::operation::Animation::Instant,
      ),
    );
    drop(crate::app::test_support::redraw_ui(&mut ui, &mut renderer));
    assert_eq!(presented_ui_offset(&mut ui, &renderer), 450.0);
    let mut ui = UserInterface::build(
      ui_page(&memory, 0.0),
      bounds,
      ui.into_cache(),
      &mut renderer,
    );
    drop(crate::app::test_support::redraw_ui(&mut ui, &mut renderer));
    assert_eq!(presented_ui_offset(&mut ui, &renderer), 0.0);
    let mut ui = UserInterface::build(
      ui_page(&memory, 1800.0),
      bounds,
      ui.into_cache(),
      &mut renderer,
    );
    drop(crate::app::test_support::redraw_ui(&mut ui, &mut renderer));
    assert_eq!(presented_ui_offset(&mut ui, &renderer), 450.0);
  }
}
