//! Retains full overview measurements while the visible copy may be clipped.
//!
//! Keeping the paragraph in the widget tree avoids reshaping every overview on
//! unrelated playback updates. The child is still rebuilt so its controls and
//! artwork reflect the current application state.

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self, paragraph::Plain, Renderer as _};
use iced::advanced::{
  layout, mouse, overlay, renderer, widget, Layout, Renderer as _, Shell, Widget,
};
use iced::{alignment, Element, Event, Length, Pixels, Rectangle, Size, Theme, Vector};

use crate::app::message::Message;

type View<'a> = dyn Fn(f32, f32) -> Element<'a, Message> + 'a;

/// Measures both blocks once before allocating spare backdrop height to the
/// gap. A Fill child in a normal column cannot do this under the scrollable's
/// unbounded height: a fixed height compresses copy, while Fill is unbounded.
pub(super) fn hero_foreground<'a>(
  back: Element<'a, Message>,
  copy: Element<'a, Message>,
  min_height: f32,
) -> Element<'a, Message> {
  Element::new(HeroForeground {
    content: [back, copy],
    min_height,
  })
}

struct HeroForeground<'a> {
  content: [Element<'a, Message>; 2],
  min_height: f32,
}

impl Widget<Message, Theme, iced::Renderer> for HeroForeground<'_> {
  fn diff(&mut self, tree: &mut widget::Tree) {
    tree.diff_children(&mut self.content);
  }

  fn size(&self) -> Size<Length> {
    Size::new(Length::Fill, Length::Fit)
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    let padding = jellypilot_ui::tokens::TOKENS.spacing.s6;
    let spacing = jellypilot_ui::tokens::TOKENS.spacing.s5;
    let width = limits.max().width;
    let child_limits = layout::Limits::new(
      Size::ZERO,
      Size::new((width - 2.0 * padding).max(0.0), f32::INFINITY),
    );
    let back =
      self.content[0]
        .as_widget_mut()
        .layout(&mut tree.children[0], renderer, &child_limits);
    let copy =
      self.content[1]
        .as_widget_mut()
        .layout(&mut tree.children[1], renderer, &child_limits);
    let height =
      (back.size().height + spacing + copy.size().height + 2.0 * padding).max(self.min_height);
    let copy_y = height - padding - copy.size().height;
    layout::Node::with_children(
      Size::new(width, height),
      vec![
        back.move_to((padding, padding)),
        copy.move_to((padding, copy_y)),
      ],
    )
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
    for ((child, tree), layout) in self
      .content
      .iter_mut()
      .zip(&mut tree.children)
      .zip(layout.children())
    {
      child
        .as_widget_mut()
        .update(tree, event, layout, cursor, renderer, shell, viewport);
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
    for ((child, tree), layout) in self
      .content
      .iter()
      .zip(&tree.children)
      .zip(layout.children())
    {
      child
        .as_widget()
        .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
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
      .content
      .iter()
      .zip(&tree.children)
      .zip(layout.children())
      .map(|((child, tree), layout)| {
        child
          .as_widget()
          .mouse_interaction(tree, layout, cursor, viewport, renderer)
      })
      .max()
      .unwrap_or_default()
  }

  fn operate(
    &mut self,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    operation: &mut dyn widget::Operation,
  ) {
    operation.container(None, layout.bounds());
    operation.traverse(&mut |operation| {
      for ((child, tree), layout) in self
        .content
        .iter_mut()
        .zip(&mut tree.children)
        .zip(layout.children())
      {
        child
          .as_widget_mut()
          .operate(tree, layout, renderer, operation);
      }
    });
  }

  fn overlay<'a>(
    &'a mut self,
    tree: &'a mut widget::Tree,
    layout: Layout<'a>,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    translation: Vector,
  ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
    overlay::from_children(
      &mut self.content,
      tree,
      layout,
      renderer,
      viewport,
      translation,
    )
  }
}

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

  fn diff(&mut self, _tree: &mut widget::Tree) {
    // The child depends on the available width; diff it during layout.
  }

  fn size(&self) -> Size<Length> {
    Size::new(Length::Fill, Length::Fit)
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    let limits = limits.width(Length::Fill).height(Length::Fit);
    let width = limits.max().width;
    let paragraph = tree.state.downcast_mut::<Plain<Paragraph>>();
    let height = if let Some(overview) = self.overview {
      paragraph.update(text::Text {
        content: overview,
        bounds: Size::new((width - self.copy_inset).max(1.0), f32::INFINITY),
        size: self.text_size,
        line_height: renderer.line_height(),
        font: renderer.font(),
        align_x: text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::Word,
        ellipsis: text::Ellipsis::default(),
        hint_factor: renderer.hint_factor(),
      });
      paragraph.min_height()
    } else {
      if !paragraph.content().is_empty() {
        *paragraph = Plain::default();
      }
      0.0
    };
    self.content = (self.view)(width, height);
    tree.diff_children(std::slice::from_mut(&mut self.content));
    let node =
      self
        .content
        .as_widget_mut()
        .layout(&mut tree.children[0], renderer, &limits.loose());
    let size = limits.resolve(Length::Fill, Length::Fit, node.size());
    layout::Node::with_children(size, vec![node])
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
    self.content.as_widget_mut().update(
      &mut tree.children[0],
      event,
      layout.children().next().expect("overview child"),
      cursor,
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
  use iced::advanced::{renderer, renderer::Headless};
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
        renderer::Settings {
          font,
          text_size: 16.0.into(),
          line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
          metrics_hinting: false,
        },
        Some("tiny-skia"),
      ))
      .expect("headless renderer");
      let mut element = overview_layout(content, inset, size, |_, height| {
        iced::widget::space().height(height).into()
      });
      tree.diff(element.as_widget_mut());
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
          text_tree.diff(text.as_widget_mut());
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
