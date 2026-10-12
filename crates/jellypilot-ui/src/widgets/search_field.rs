//! Search-field pill frame.
//!
//! The pill owns the visual border and fill; the inner text input is styled
//! with a transparent background and no border so the frame shows through.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::Shell;
use iced::widget::container;
use iced::{
    Alignment, Background, Border, Element, Length, Padding, Rectangle, Shadow, Size, Theme, Vector,
};

use crate::tokens::{palette, ThemePalette, SIDEBAR_CONTROL_RADIUS};

/// A pill-shaped frame that owns the fill and border for a search field.
///
/// The three children are laid out left-to-right: a leading control, the
/// borderless text input, and a trailing control. Focus on the input is read
/// through public focus operations, so the frame can switch to the focused
/// appearance without exposing any messages.
///
/// The single text input is located recursively in the widget tree, so it may
/// be wrapped by arbitrary intermediate widgets (e.g. an escape handler).
pub struct SearchField<'a, Message, Renderer = iced::Renderer> {
    children: [Element<'a, Message, Theme, Renderer>; 3],
    input_focused: bool,
}

impl<'a, Message, Renderer> SearchField<'a, Message, Renderer>
where
    Message: 'a,
    Renderer: 'a + iced::advanced::Renderer + iced::advanced::text::Renderer,
{
    fn refresh_focus(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) {
        if let Some(input_layout) = super::child_layout(layout, tree, 1) {
            self.input_focused = super::escape_input::child_is_focused(
                &mut self.children[1],
                &mut tree.children[1],
                input_layout,
                viewport,
                renderer,
            );
        }
    }

    /// Creates a search-field pill from its three children.
    ///
    /// `input` must contain exactly one text input somewhere in its widget
    /// subtree; its focus state is located recursively and used to drive the
    /// frame style.
    pub fn new(
        leading: impl Widget<Message, Theme, Renderer> + 'a,
        input: impl Widget<Message, Theme, Renderer> + 'a,
        trailing: impl Widget<Message, Theme, Renderer> + 'a,
    ) -> Self {
        Self {
            children: [leading._boxed(), input._boxed(), trailing._boxed()],
            input_focused: false,
        }
    }
}

/// Creates a [`SearchField`] pill with the default renderer.
pub fn search_field<'a, Message>(
    leading: impl Widget<Message, Theme, iced::Renderer> + 'a,
    input: impl Widget<Message, Theme, iced::Renderer> + 'a,
    trailing: impl Widget<Message, Theme, iced::Renderer> + 'a,
) -> SearchField<'a, Message>
where
    Message: 'a,
{
    SearchField::new(leading, input, trailing)
}

/// Pure style mapping for the pill frame.
///
/// The rest state uses a subtle container surface and a 1 px `borderSubtle`
/// structural edge; the focused state swaps to `controlHover` and a functional
/// primary border. Both keep the sidebar's 12 px radius.
#[must_use]
pub fn frame_style(palette: &ThemePalette, focused: bool) -> container::Style {
    let colors = palette.colors;
    let (background, border_color) = if focused {
        (colors.controlHover, colors.primary)
    } else {
        (colors.surfaceContainerHigh, colors.borderSubtle)
    };

    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            smoothing: crate::widgets::container::SURFACE_SMOOTHING,
            radius: SIDEBAR_CONTROL_RADIUS.into(),
            color: border_color,
            width: 1.0,
        },
        ..container::Style::default()
    }
}

impl<'a, Message, Renderer> iced::advanced::widget::Meta for SearchField<'a, Message, Renderer>
where
    Message: 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
}

impl<'a, Message, Renderer> Widget<Message, Theme, Renderer> for SearchField<'a, Message, Renderer>
where
    Message: 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<layout::flex::Cache>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(layout::flex::Cache::default())
    }
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fit,
        }
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        tree.size = layout::flex::resolve(
            layout::flex::Axis::Horizontal,
            renderer,
            limits,
            Length::Fill,
            Length::Fit,
            Padding::new(3.0),
            0.0,
            Alignment::Center,
            &mut tree.children,
            &mut self.children,
            tree.state.downcast_mut::<layout::flex::Cache>(),
        );
        let geometry = Layout::new(tree.size);
        self.refresh_focus(tree, geometry, &Rectangle::with_size(tree.size), renderer);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        for (child, (layout, tree)) in self
            .children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            child.operate(tree, layout, viewport, renderer, operation);
        }
        self.refresh_focus(tree, layout, viewport, renderer);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for (child, (layout, tree)) in self
            .children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            child.update(tree, event, layout, cursor, renderer, shell, viewport);
        }
        self.refresh_focus(tree, layout, viewport, renderer);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let Some(bounds) = layout.bounds().intersection(viewport) else {
            return;
        };

        let frame = frame_style(palette(theme), self.input_focused);

        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: frame.border,
                shadow: Shadow::default(),
                snap: true,
            },
            frame
                .background
                .expect("search field frame always has a background"),
        );

        for (child, (layout, tree)) in self.children.iter().zip(layout.iter(&tree.children)) {
            child.draw(tree, renderer, theme, style, layout, cursor, &bounds);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> iced::advanced::mouse::Interaction {
        for (child, (layout, tree)) in self.children.iter().zip(layout.iter(&tree.children)) {
            if cursor.is_over(layout.bounds()) {
                return child.mouse_interaction(tree, layout, cursor, viewport, renderer);
            }
        }
        iced::advanced::mouse::Interaction::default()
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}

impl<'a, Message, Renderer> From<SearchField<'a, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    fn from(field: SearchField<'a, Message, Renderer>) -> Self {
        field._boxed()
    }
}

#[cfg(test)]
mod tests {
    use iced::Background;

    use super::frame_style;
    use crate::tokens::{DARK_PALETTE, SIDEBAR_CONTROL_RADIUS};

    #[test]
    fn rest_frame_uses_surface_container_high_and_border_subtle() {
        let style = frame_style(&DARK_PALETTE, false);

        assert_eq!(
            style.background,
            Some(Background::Color(DARK_PALETTE.colors.surfaceContainerHigh))
        );
        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.color, DARK_PALETTE.colors.borderSubtle);
        assert_eq!(
            style.border.radius,
            iced::border::Radius::from(SIDEBAR_CONTROL_RADIUS)
        );
    }

    #[test]
    fn focused_frame_uses_hover_fill_and_primary_border() {
        let style = frame_style(&DARK_PALETTE, true);

        assert_eq!(
            style.background,
            Some(Background::Color(DARK_PALETTE.colors.controlHover))
        );
        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.color, DARK_PALETTE.colors.primary);
        assert_eq!(
            style.border.radius,
            iced::border::Radius::from(SIDEBAR_CONTROL_RADIUS)
        );
    }
}
