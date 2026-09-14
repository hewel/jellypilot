//! Focus bookkeeping for the embedded player's background-click action.
//!
//! iced has no activation-origin metadata. A focus transition protects the next
//! press until deliberate input intervenes; redraws and modifier reconciliation
//! do not disarm it. Keyboard/taskbar activation without intervening input is
//! indistinguishable and therefore also protects one background click.

use std::collections::VecDeque;

use iced::advanced::{layout, mouse, overlay, renderer, shell::Bus, widget, Layout, Shell, Widget};
use iced::{mouse::Button, window, Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

/// Observes the whole player so a click on an explicit control also consumes
/// activation. Only messages emitted by the activating press are mapped;
/// children still receive the press and retain their normal release behavior.
pub fn activation_guard<'a, Message: 'static>(
  content: impl Into<Element<'a, Message>>,
  on_activation: impl Fn(Message, Point) -> Message + 'a,
) -> Element<'a, Message> {
  Element::new(ActivationGuard {
    content: content.into(),
    on_activation,
  })
}

struct ActivationGuard<'a, Message, F> {
  content: Element<'a, Message>,
  on_activation: F,
}

struct State<Message> {
  focused: bool,
  armed: bool,
  activation_press_seen: bool,
  last_cursor: Option<Point>,
  activation_point: Option<Point>,
  // iced dispatches the whole overlay batch before replaying ignored events
  // to the root. Retain classifications, not another copy of the event stream.
  pending: VecDeque<Option<Point>>,
  messages: Bus<Message>,
}

impl<Message> Default for State<Message> {
  fn default() -> Self {
    Self {
      focused: true,
      armed: false,
      activation_press_seen: false,
      last_cursor: None,
      activation_point: None,
      pending: VecDeque::new(),
      messages: Bus::new(),
    }
  }
}

impl<Message> State<Message> {
  fn observe(&mut self, event: &Event, cursor: mouse::Cursor) -> Option<Point> {
    let previous_cursor = self.last_cursor;
    self.last_cursor = cursor.position().or(previous_cursor);
    let mut activation = None;
    match event {
      Event::Window(window::Event::Focused) => {
        if !self.focused {
          self.armed = !self.activation_press_seen;
          self.focused = true;
          self.activation_press_seen = false;
        }
      }
      Event::Window(window::Event::Unfocused) => {
        self.focused = false;
        self.armed = false;
        self.activation_press_seen = false;
        self.activation_point = None;
      }
      Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
        // A nested overlay can bubble the same press through several wrappers.
        if self.activation_point.is_some() {
          return self.activation_point;
        }
        if self.armed || !self.focused {
          activation = cursor.position();
        }
        if !self.focused {
          self.activation_press_seen = true;
        }
        self.armed = false;
        self.activation_point = activation;
      }
      Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
        self.activation_point = None;
        self.armed = false;
      }
      Event::Mouse(mouse::Event::CursorMoved { position }) => {
        if previous_cursor != Some(*position) {
          self.armed = false;
        }
      }
      Event::Mouse(
        mouse::Event::ButtonPressed(_)
        | mouse::Event::ButtonReleased(_)
        | mouse::Event::WheelScrolled { .. },
      )
      | Event::Keyboard(
        iced::keyboard::Event::KeyPressed { .. } | iced::keyboard::Event::KeyReleased { .. },
      )
      | Event::Touch(_) => self.armed = false,
      _ => {}
    }
    activation
  }
}

impl<Message: 'static, F: Fn(Message, Point) -> Message> Widget<Message, Theme, iced::Renderer>
  for ActivationGuard<'_, Message, F>
{
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<State<Message>>()
  }

  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(State::<Message>::default())
  }

  fn diff(&mut self, tree: &mut widget::Tree) {
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
    self
      .content
      .as_widget_mut()
      .layout(&mut tree.children[0], renderer, limits)
  }

  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self
      .content
      .as_widget_mut()
      .operate(&mut tree.children[0], layout, renderer, operation);
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
    let state = tree.state.downcast_mut::<State<Message>>();
    let activation = state
      .pending
      .pop_front()
      .unwrap_or_else(|| state.observe(event, cursor));

    let mut local = shell.local(&mut state.messages);
    self.content.as_widget_mut().update(
      &mut tree.children[0],
      event,
      layout,
      cursor,
      renderer,
      &mut local,
      viewport,
    );
    shell.merge(local, |message| match activation {
      Some(position) => (self.on_activation)(message, position),
      None => message,
    });
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

  fn overlay<'b>(
    &'b mut self,
    tree: &'b mut widget::Tree,
    layout: Layout<'b>,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    translation: Vector,
  ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
    self
      .content
      .as_widget_mut()
      .overlay(
        &mut tree.children[0],
        layout,
        renderer,
        viewport,
        translation,
      )
      .map(|content| guarded_overlay(content, tree.state.downcast_mut::<State<Message>>(), true))
  }
}

fn guarded_overlay<'a, Message: 'a>(
  content: overlay::Element<'a, Message, Theme, iced::Renderer>,
  state: &'a mut State<Message>,
  root: bool,
) -> overlay::Element<'a, Message, Theme, iced::Renderer> {
  overlay::Element::new(Box::new(ActivationOverlay {
    content,
    state,
    root,
  }))
}

struct ActivationOverlay<'a, Message> {
  content: overlay::Element<'a, Message, Theme, iced::Renderer>,
  state: &'a mut State<Message>,
  root: bool,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for ActivationOverlay<'_, Message> {
  fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
    self.content.as_overlay_mut().layout(renderer, bounds)
  }

  fn update(
    &mut self,
    event: &Event,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut Shell<'_, Message>,
  ) {
    let activation = self.state.observe(event, cursor);
    self
      .content
      .as_overlay_mut()
      .update(event, layout, cursor, renderer, shell);
    if self.root && shell.event_status() == iced::event::Status::Ignored {
      self.state.pending.push_back(activation);
    }
  }

  fn draw(
    &self,
    renderer: &mut iced::Renderer,
    theme: &Theme,
    style: &renderer::Style,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
  ) {
    self
      .content
      .as_overlay()
      .draw(renderer, theme, style, layout, cursor);
  }

  fn operate(
    &mut self,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self
      .content
      .as_overlay_mut()
      .operate(layout, renderer, operation);
  }

  fn mouse_interaction(
    &self,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
  ) -> mouse::Interaction {
    self
      .content
      .as_overlay()
      .mouse_interaction(layout, cursor, renderer)
  }

  fn overlay<'a>(
    &'a mut self,
    layout: Layout<'a>,
    renderer: &iced::Renderer,
  ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    self
      .content
      .as_overlay_mut()
      .overlay(layout, renderer)
      .map(|content| guarded_overlay(content, self.state, false))
  }

  fn index(&self) -> f32 {
    self.content.as_overlay().index()
  }
}
