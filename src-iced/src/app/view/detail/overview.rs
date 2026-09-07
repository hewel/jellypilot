//! Retains full overview measurements while the visible copy may be clipped.
//!
//! Keeping the paragraph in the widget tree avoids reshaping every overview on
//! unrelated playback updates. The child is still rebuilt so its controls and
//! artwork reflect the current application state.

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self, paragraph::Plain, Renderer as _};
use iced::advanced::{layout, mouse, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::{alignment, Element, Event, Length, Pixels, Rectangle, Size, Theme, Vector};

use crate::app::message::Message;

type View<'a> = dyn Fn(f32, f32) -> Element<'a, Message> + 'a;

/// Builds the child with its available width and the full overview height.
/// `copy_inset` is the horizontal space occupied by padding and sibling controls.
pub(super) fn overview_layout<'a>(
  overview: Option<&'a str>,
  copy_inset: f32,
  text_size: f32,
  view: impl Fn(f32, f32) -> Element<'a, Message> + 'a,
) -> Element<'a, Message> {
  Element::new(OverviewLayout {
    overview: overview.filter(|value| !value.trim().is_empty()),
    copy_inset,
    text_size: Pixels(text_size),
    view: Box::new(view),
    content: iced::widget::space().into(),
  })
}

struct OverviewLayout<'a> {
  overview: Option<&'a str>,
  copy_inset: f32,
  text_size: Pixels,
  view: Box<View<'a>>,
  content: Element<'a, Message>,
}

impl Widget<Message, Theme, iced::Renderer> for OverviewLayout<'_> {
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<Plain<Paragraph>>()
  }

  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(Plain::<Paragraph>::default())
  }

  fn diff(&self, _tree: &mut widget::Tree) {
    // The child depends on the available width; diff it during layout.
  }

  fn size(&self) -> Size<Length> {
    Size::new(Length::Fill, Length::Shrink)
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    let limits = limits.width(Length::Fill).height(Length::Shrink);
    let width = limits.max().width;
    let paragraph = tree.state.downcast_mut::<Plain<Paragraph>>();
    let height = if let Some(overview) = self.overview {
      paragraph.update(text::Text {
        content: overview,
        bounds: Size::new((width - self.copy_inset).max(1.0), f32::INFINITY),
        size: self.text_size,
        line_height: text::LineHeight::default(),
        font: renderer.default_font(),
        align_x: text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::Word,
      });
      paragraph.min_height()
    } else {
      if !paragraph.content().is_empty() {
        *paragraph = Plain::default();
      }
      0.0
    };
    self.content = (self.view)(width, height);
    tree.diff_children(std::slice::from_ref(&self.content));
    let node =
      self
        .content
        .as_widget_mut()
        .layout(&mut tree.children[0], renderer, &limits.loose());
    let size = limits.resolve(Length::Fill, Length::Shrink, node.size());
    layout::Node::with_children(size, vec![node])
  }

  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &Event,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    renderer: &iced::Renderer,
    clipboard: &mut dyn Clipboard,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
  ) {
    self.content.as_widget_mut().update(
      &mut tree.children[0],
      event,
      layout.children().next().expect("overview child"),
      cursor,
      renderer,
      clipboard,
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
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    viewport: &Rectangle,
  ) {
    self.content.as_widget().draw(
      &tree.children[0],
      renderer,
      theme,
      style,
      layout.children().next().expect("overview child"),
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
      layout.children().next().expect("overview child"),
      cursor,
      viewport,
      renderer,
    )
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
      layout.children().next().expect("overview child"),
      renderer,
      operation,
    );
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
      layout.children().next().expect("overview child"),
      renderer,
      viewport,
      translation,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::advanced::renderer::Headless;
  use iced::Font;

  #[test]
  fn retained_measurement_matches_visible_text_after_layout_inputs_change() {
    let overview = "A detailed overview with enough words to wrap across several lines when \
      the available width is narrow, while fitting into fewer lines when more width is available.";
    let mut tree = widget::Tree::empty();
    let mut heights = Vec::new();
    // Reuse the same widget tree as playback updates, resizing, and navigation do.
    for (content, width, inset, size, font) in [
      (Some(overview), 180.0, 40.0, 13.0, Font::DEFAULT),
      (Some(overview), 800.0, 40.0, 13.0, Font::DEFAULT),
      (
        Some("A shorter replacement."),
        800.0,
        40.0,
        13.0,
        Font::DEFAULT,
      ),
      (Some(overview), 800.0, 500.0, 15.0, Font::MONOSPACE),
      (None, 800.0, 40.0, 13.0, Font::DEFAULT),
      (Some(" \n "), 800.0, 40.0, 13.0, Font::DEFAULT),
      (Some(overview), 180.0, 40.0, 13.0, Font::DEFAULT),
    ] {
      let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        font,
        16.0.into(),
        Some("tiny-skia"),
      ))
      .expect("headless renderer");
      let mut element = overview_layout(content, inset, size, |_, height| {
        iced::widget::space().height(height).into()
      });
      tree.diff(element.as_widget());
      let limits = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
      let height = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &limits)
        .size()
        .height;
      let expected = content
        .filter(|text| !text.trim().is_empty())
        .map_or(0.0, |content| {
          let mut text: Element<'_, Message> = iced::widget::text(content).size(size).into();
          let mut text_tree = widget::Tree::new(&text);
          text
            .as_widget_mut()
            .layout(
              &mut text_tree,
              &renderer,
              &layout::Limits::new(
                Size::ZERO,
                Size::new((width - inset).max(1.0), f32::INFINITY),
              ),
            )
            .size()
            .height
        });
      assert_eq!(height, expected, "measurement must match rendered copy");
      heights.push(height);
    }
    assert!(
      heights[0] > heights[1],
      "a narrower overview must wrap onto more lines"
    );
    assert_eq!(
      heights[0], heights[6],
      "returning to an overview restores its height"
    );
  }
}
