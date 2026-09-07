use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use super::Message;

/// Wrap the complete dialog panel, including padding, before centering it in
/// the scrim. Dismissal uses panel bounds, not whether a child consumed input.
pub(super) fn dismissible<'a>(
  panel: impl Into<Element<'a, Message>>,
  dismiss: Message,
) -> Element<'a, Message> {
  Element::new(Dismissible {
    panel: panel.into(),
    dismiss,
  })
}

struct Dismissible<'a> {
  panel: Element<'a, Message>,
  dismiss: Message,
}

impl Widget<Message, Theme, iced::Renderer> for Dismissible<'_> {
  fn tag(&self) -> widget::tree::Tag {
    self.panel.as_widget().tag()
  }
  fn state(&self) -> widget::tree::State {
    self.panel.as_widget().state()
  }
  fn diff(&mut self, tree: &mut widget::Tree) {
    self.panel.as_widget_mut().diff(tree);
  }
  fn size(&self) -> Size<Length> {
    self.panel.as_widget().size()
  }
  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    self.panel.as_widget_mut().layout(tree, renderer, limits)
  }
  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    self
      .panel
      .as_widget_mut()
      .operate(tree, layout, renderer, operation);
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
    self
      .panel
      .as_widget_mut()
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
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
  ) {
    self
      .panel
      .as_widget()
      .draw(tree, renderer, theme, style, layout, cursor, viewport);
  }
  fn mouse_interaction(
    &self,
    tree: &widget::Tree,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
  ) -> mouse::Interaction {
    self
      .panel
      .as_widget()
      .mouse_interaction(tree, layout, cursor, viewport, renderer)
  }
  fn overlay<'a>(
    &'a mut self,
    tree: &'a mut widget::Tree,
    layout: Layout<'a>,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    translation: Vector,
  ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    self
      .panel
      .as_widget_mut()
      .overlay(tree, layout, renderer, viewport, translation)
  }
}
