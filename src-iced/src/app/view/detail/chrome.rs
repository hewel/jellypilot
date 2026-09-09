use iced::advanced::{layout, renderer, widget, Layout, Renderer as _, Widget};
use iced::widget::{container, image, Image};
use iced::{gradient, Background, ContentFit, Degrees, Element, Fill, Length, Rectangle, Size};
use jellypilot_ui::icons::{Icon, IconSize};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::ButtonVariant;
use jellypilot_ui::widgets::control_button::control_button;

use crate::app::detail::DETAIL_BACKDROP_KEY;
use crate::app::message::{DetailMessage, Message};
use crate::app::state::State;

pub(super) fn detail_back(state: &State, width: f32, height: f32) -> Element<'_, Message> {
  let button = control_button(
    Some(Icon::ChevronLeft),
    Some(state.t("detail-back")),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Sm)
  .label_size(12.0)
  .spacing(6.0)
  .padding([8, 14])
  .style(jellypilot_ui::widgets::button::detail_back)
  .on_press_maybe(
    (!state.shell.navigation_stack.is_empty()).then_some(Message::Detail(DetailMessage::Back)),
  );
  let backdrop = state
    .full
    .as_ref()
    .expect("FullUi required")
    .detail
    .artwork
    .get(DETAIL_BACKDROP_KEY)
    .and_then(|cell| cell.handle())
    .cloned();
  container(Element::new(BackGlass {
    content: button.into(),
    backdrop,
    frame: Size::new(width, height),
  }))
  .padding(18)
  .width(Fill)
  .into()
}

/// The mask follows the localized button's layout; its sampling frame remains
/// aligned with the full hero even when the page scrolls.
struct BackGlass<'a> {
  content: Element<'a, Message>,
  backdrop: Option<image::Handle>,
  frame: Size,
}

impl Widget<Message, iced::Theme, iced::Renderer> for BackGlass<'_> {
  fn diff(&mut self, tree: &mut widget::Tree) {
    tree.diff_children(&mut [self.content.as_widget_mut()]);
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
    event: &iced::Event,
    layout: Layout<'_>,
    cursor: iced::mouse::Cursor,
    renderer: &iced::Renderer,
    shell: &mut iced::advanced::Shell<'_, Message>,
    viewport: &Rectangle,
  ) {
    self.content.as_widget_mut().update(
      &mut tree.children[0],
      event,
      layout,
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
    theme: &iced::Theme,
    style: &renderer::Style,
    layout: Layout<'_>,
    cursor: iced::mouse::Cursor,
    viewport: &Rectangle,
  ) {
    let Some(visible) = layout.bounds().intersection(viewport) else {
      return;
    };
    if let Some(handle) = &self.backdrop {
      let background = jellypilot_ui::tokens::palette(theme).colors.background;
      let fade = gradient::Linear::new(Degrees(180.0))
        .add_stop(0.0, background.scale_alpha(0.30))
        .add_stop(0.45, background.scale_alpha(0.28))
        .add_stop(0.78, background.scale_alpha(0.72))
        .add_stop(1.0, background);
      let image = Image::new(handle.clone())
        .content_fit(ContentFit::Cover)
        .display_frame(Rectangle::new(iced::Point::new(-18.0, -18.0), self.frame))
        .mask_frame(Rectangle::new(iced::Point::ORIGIN, layout.bounds().size()))
        .border_radius(TOKENS.radii.xl)
        .border_smoothing(jellypilot_ui::widgets::container::SURFACE_SMOOTHING)
        .blur(10.0)
        .tint(Background::Gradient(fade.into()));
      <Image as Widget<Message, iced::Theme, iced::Renderer>>::draw(
        &image, tree, renderer, theme, style, layout, cursor, &visible,
      );
      let left_alpha = |x: f32| 0.55 * (1.0 - x / (self.frame.width * 0.55).max(1.0)).max(0.0);
      let left_fade = gradient::Linear::new(Degrees(90.0))
        .add_stop(0.0, background.scale_alpha(left_alpha(18.0)))
        .add_stop(
          1.0,
          background.scale_alpha(left_alpha(18.0 + layout.bounds().width)),
        );
      renderer.with_layer(visible, |renderer| {
        renderer.fill_quad(
          renderer::Quad {
            bounds: layout.bounds(),
            border: iced::Border {
              radius: TOKENS.radii.xl.into(),
              ..iced::Border::default()
            },
            ..renderer::Quad::default()
          },
          Background::Gradient(left_fade.into()),
        );
      });
    }
    // Native image batches must settle before the Catalog fill and label.
    renderer.with_layer(visible, |renderer| {
      self.content.as_widget().draw(
        &tree.children[0],
        renderer,
        theme,
        style,
        layout,
        cursor,
        &visible,
      );
    });
  }

  fn mouse_interaction(
    &self,
    tree: &widget::Tree,
    layout: Layout<'_>,
    cursor: iced::mouse::Cursor,
    viewport: &Rectangle,
    renderer: &iced::Renderer,
  ) -> iced::mouse::Interaction {
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
    translation: iced::Vector,
  ) -> Option<iced::advanced::overlay::Element<'a, Message, iced::Theme, iced::Renderer>> {
    self.content.as_widget_mut().overlay(
      &mut tree.children[0],
      layout,
      renderer,
      viewport,
      translation,
    )
  }
}
