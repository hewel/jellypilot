//! Opacity for an explicitly selected Hero image; image arrival alone is static.

use std::time::Instant;

use iced::advanced::{layout, mouse, renderer, widget, Layout, Shell, Widget};
use iced::widget::Image;
use iced::{Element, Event, Rectangle, Size, Theme};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::widgets::motion;

use super::super::message::Message;

pub(super) fn fade(image: Image, key: u64) -> Element<'static, Message> {
  Element::new(Fade { image, key })
}

struct Fade {
  image: Image,
  key: u64,
}

struct State {
  key: u64,
  opacity: f32,
  from: f32,
  started: Option<Instant>,
}

impl Widget<Message, Theme, iced::Renderer> for Fade {
  fn tag(&self) -> widget::tree::Tag {
    widget::tree::Tag::of::<State>()
  }

  fn state(&self) -> widget::tree::State {
    widget::tree::State::new(State {
      key: self.key,
      opacity: 1.0,
      from: 1.0,
      started: None,
    })
  }

  fn diff(&mut self, tree: &mut widget::Tree) {
    let state = tree.state.downcast_mut::<State>();
    if !motion::enabled() {
      state.opacity = 1.0;
      state.started = None;
    } else if state.key != self.key {
      // Reversals keep the currently displayed alpha rather than restarting
      // a queued entrance. No previous image allocation is retained.
      state.from = if state.started.is_some() {
        state.opacity
      } else {
        0.0
      };
      state.opacity = state.from;
      state.started = Some(Instant::now());
    }
    state.key = self.key;
  }

  fn size(&self) -> Size<iced::Length> {
    <Image as Widget<Message, Theme, iced::Renderer>>::size(&self.image)
  }

  fn layout(
    &mut self,
    tree: &mut widget::Tree,
    renderer: &iced::Renderer,
    limits: &layout::Limits,
  ) -> layout::Node {
    <Image as Widget<Message, Theme, iced::Renderer>>::layout(
      &mut self.image,
      tree,
      renderer,
      limits,
    )
  }

  fn update(
    &mut self,
    tree: &mut widget::Tree,
    event: &Event,
    _layout: Layout<'_>,
    _cursor: mouse::Cursor,
    _renderer: &iced::Renderer,
    shell: &mut Shell<'_, Message>,
    _viewport: &Rectangle,
  ) {
    let state = tree.state.downcast_mut::<State>();
    if !motion::enabled() {
      state.opacity = 1.0;
      state.started = None;
      return;
    }
    if let (Some(started), Event::Window(iced::window::Event::RedrawRequested(now))) =
      (state.started, event)
    {
      let progress =
        now.saturating_duration_since(started).as_secs_f32() / TOKENS.durations.ms300.as_secs_f32();
      state.opacity = state.from + (1.0 - state.from) * TOKENS.easings.standard.sample(progress);
      if progress >= 1.0 {
        state.opacity = 1.0;
        state.started = None;
      }
    }
    if state.started.is_some() {
      shell.request_redraw();
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
    let opacity = if motion::enabled() {
      tree.state.downcast_ref::<State>().opacity
    } else {
      1.0
    };
    let translucent;
    let image = if opacity >= 1.0 {
      &self.image
    } else {
      translucent = self.image.clone().opacity(opacity);
      &translucent
    };
    <Image as Widget<Message, Theme, iced::Renderer>>::draw(
      image, tree, renderer, theme, style, layout, cursor, viewport,
    );
  }
}
