use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::widget::Widget as _;
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use super::Message;

/// Wrap the complete dialog panel, including padding, before centering it in
/// the scrim. Dismissal uses panel bounds, not whether a child consumed input.
pub(super) fn dismissible<'a>(
  panel: impl iced::advanced::Widget<Message, iced::Theme, iced::Renderer> + 'a,
  dismiss: Message,
) -> Element<'a, Message> {
  iced::advanced::Widget::_boxed(Dismissible {
    panel: panel.boxed(),
    dismiss,
  })
}

struct Dismissible<'a> {
  panel: Element<'a, Message>,
  dismiss: Message,
}

impl widget::Meta for Dismissible<'_> {}

impl Widget<Message, Theme, iced::Renderer> for Dismissible<'_> {
  fn tag(&self) -> widget::tree::Tag {
    self.panel.tag()
  }
  fn state(&self) -> widget::tree::State {
    self.panel.state()
  }
  fn diff(&mut self, tree: &mut widget::Tree) {
    self.panel.diff(tree);
  }
  fn size(&self) -> Size<Length> {
    self.panel.size()
  }
  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) {
    self.panel.layout(tree, renderer, limits)
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
      .panel
      .operate(tree, layout, viewport, renderer, operation);
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
    self
      .panel
      .update(tree, event, layout, cursor, renderer, shell, viewport);
    if shell.is_event_captured() {
      return;
    }
    let position = match event {
      Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => cursor.position(),
      Event::Touch(iced::touch::Event::FingerPressed { position, .. }) => Some(*position),
      _ => None,
    };
    if let Some(position) = position.filter(|position| viewport.contains(*position)) {
      if !layout.bounds().contains(position) {
        shell.publish(self.dismiss.clone());
      }
      // A passive panel area must not pass a press to a lower layer either.
      shell.capture_event();
    }
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
    self
      .panel
      .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
      .panel
      .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
    self
      .panel
      .overlay(tree, layout, renderer, viewport, translation, window)
  }
}

#[cfg(test)]
mod tests {
  use iced::advanced::{renderer, renderer::Headless, shell};
  use iced::widget::{container, Space};
  use iced::{mouse, touch, Event, Length, Point, Size};
  use iced_runtime::user_interface::{Cache, UserInterface};
  use jellypilot_ui::{fonts::DEFAULT_LINE_HEIGHT, tokens::TOKENS, widgets::motion};

  use super::{dismissible, Message};

  #[test]
  fn entering_modal_touch_uses_presented_bounds_and_exiting_modal_is_inert() {
    let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
      renderer::Settings {
        font: iced::Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    ))
    .expect("software renderer");
    let view = |visible| {
      motion::scope(
        motion::reveal(
          container(dismissible(
            Space::new().width(100).height(40),
            Message::DismissNotice(7),
          ))
          .center_x(Length::Fill)
          .center_y(Length::Fill),
          visible,
          true,
          TOKENS.durations.ms200,
        ),
        true,
      )
    };
    let bounds = Size::new(400.0, 300.0);
    let ui = UserInterface::build(view(false), bounds, Cache::new(), &mut renderer);
    let mut ui = UserInterface::build(view(true), bounds, ui.into_cache(), &mut renderer);
    let mut messages = shell::Bus::new();
    let waker = shell::Waker::noop();
    // The natural panel ends at y=170; its entering position extends below it.
    ui.update(
      &iced::window::Headless,
      &waker,
      &[Event::Touch(touch::Event::FingerPressed {
        id: touch::Finger(1),
        position: Point::new(200.0, 175.0),
      })],
      mouse::Cursor::Unavailable,
      &mut renderer,
      &mut messages,
    );
    assert!(
      messages.is_empty(),
      "touching the displayed panel must not dismiss it"
    );

    let outside = Event::Touch(touch::Event::FingerPressed {
      id: touch::Finger(2),
      position: Point::new(200.0, 120.0),
    });
    ui.update(
      &iced::window::Headless,
      &waker,
      std::slice::from_ref(&outside),
      mouse::Cursor::Unavailable,
      &mut renderer,
      &mut messages,
    );
    let emitted = messages
      .drain()
      .map(|(message, _)| message)
      .collect::<Vec<_>>();
    assert!(matches!(emitted.as_slice(), [Message::DismissNotice(7)]));

    let mut ui = UserInterface::build(view(false), bounds, ui.into_cache(), &mut renderer);
    ui.update(
      &iced::window::Headless,
      &waker,
      &[outside],
      mouse::Cursor::Unavailable,
      &mut renderer,
      &mut messages,
    );
    assert!(
      messages.is_empty(),
      "an exiting modal must already be inert"
    );
  }
}
