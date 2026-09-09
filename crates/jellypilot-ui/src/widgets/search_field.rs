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
    fn refresh_focus(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer) {
        if let Some(input_layout) = layout.children().nth(1) {
            self.input_focused = super::escape_input::child_is_focused(
                self.children[1].as_widget_mut(),
                &mut tree.children[1],
                input_layout,
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
        leading: impl Into<Element<'a, Message, Theme, Renderer>>,
        input: impl Into<Element<'a, Message, Theme, Renderer>>,
        trailing: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            children: [leading.into(), input.into(), trailing.into()],
            input_focused: false,
        }
    }
}

/// Creates a [`SearchField`] pill with the default renderer.
pub fn search_field<'a, Message>(
    leading: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    input: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
    trailing: impl Into<Element<'a, Message, Theme, iced::Renderer>>,
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

impl<'a, Message, Renderer> Widget<Message, Theme, Renderer> for SearchField<'a, Message, Renderer>
where
    Message: 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fit,
        }
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = layout::flex::resolve(
            layout::flex::Axis::Horizontal,
            renderer,
            limits,
            Length::Fill,
            Length::Fit,
            Padding::new(3.0),
            0.0,
            Alignment::Center,
            &mut self.children,
            &mut tree.children,
        );
        self.refresh_focus(tree, Layout::new(&node), renderer);
        node
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child
                .as_widget_mut()
                .operate(tree, layout, renderer, operation);
        }
        self.refresh_focus(tree, layout, renderer);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child
                .as_widget_mut()
                .update(tree, event, layout, cursor, renderer, shell, viewport);
        }
        self.refresh_focus(tree, layout, renderer);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
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

        for ((child, tree), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, &bounds);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> iced::advanced::mouse::Interaction {
        for ((child, tree), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            if cursor.is_over(layout.bounds()) {
                return child
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer);
            }
        }
        iced::advanced::mouse::Interaction::default()
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
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
        Element::new(field)
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
