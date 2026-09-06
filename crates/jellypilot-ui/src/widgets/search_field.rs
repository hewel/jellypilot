//! Search-field pill frame.
//!
//! The pill owns the visual border and fill; the inner text input is styled
//! with a transparent background and no border so the frame shows through.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::widget::{container, text_input};
use iced::{
    Alignment, Background, Border, Element, Length, Padding, Rectangle, Shadow, Size, Theme, Vector,
};

use crate::tokens::{palette, ThemePalette, SIDEBAR_CONTROL_RADIUS};

/// A pill-shaped frame that owns the fill and border for a search field.
///
/// The three children are laid out left-to-right: a leading control, the
/// borderless text input, and a trailing control. Focus on the input is read
/// from its widget state at draw time, so the frame can switch to the focused
/// appearance without exposing any messages.
///
/// The single text input is located recursively in the widget tree, so it may
/// be wrapped by arbitrary intermediate widgets (e.g. an escape handler).
pub struct SearchField<'a, Message, Renderer = iced::Renderer> {
    children: [Element<'a, Message, Theme, Renderer>; 3],
}

impl<'a, Message, Renderer> SearchField<'a, Message, Renderer>
where
    Message: 'a,
    Renderer: 'a + iced::advanced::Renderer + iced::advanced::text::Renderer,
{
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
/// The rest state uses the control fill and a structural `outlineVariant`
/// border; the focused state swaps to `controlHover` and a functional primary
/// border. Both keep the sidebar's 12 px radius.
#[must_use]
pub fn frame_style(palette: &ThemePalette, focused: bool) -> container::Style {
    let colors = palette.colors;
    let (background, border_color) = if focused {
        (colors.controlHover, colors.primary)
    } else {
        (colors.control, colors.outlineVariant)
    };

    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            radius: SIDEBAR_CONTROL_RADIUS.into(),
            color: border_color,
            width: 1.0,
        },
        ..container::Style::default()
    }
}

fn is_input_focused<Renderer>(tree: &widget::Tree) -> bool
where
    Renderer: iced::advanced::text::Renderer,
{
    if let widget::tree::State::Some(state) = &tree.state {
        if state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_some_and(text_input::State::is_focused)
        {
            return true;
        }
    }

    tree.children.iter().any(is_input_focused::<Renderer>)
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
            height: Length::Shrink,
        }
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::flex::resolve(
            layout::flex::Axis::Horizontal,
            renderer,
            limits,
            Length::Fill,
            Length::Shrink,
            Padding::new(3.0),
            0.0,
            Alignment::Center,
            &mut self.children,
            &mut tree.children,
        )
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
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, tree), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
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

        let frame = frame_style(palette(theme), is_input_focused::<Renderer>(tree));

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
    use iced::advanced::widget;
    use iced::widget::text_input;
    use iced::Background;

    use super::{frame_style, is_input_focused};
    use crate::tokens::{DARK_PALETTE, SIDEBAR_CONTROL_RADIUS};

    #[test]
    fn rest_frame_uses_control_fill_and_outline_variant_border() {
        let style = frame_style(&DARK_PALETTE, false);

        assert_eq!(
            style.background,
            Some(Background::Color(DARK_PALETTE.colors.control))
        );
        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.color, DARK_PALETTE.colors.outlineVariant);
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

    fn focused_input_tree() -> widget::Tree {
        let mut state = text_input::State::<
            <iced::Renderer as iced::advanced::text::Renderer>::Paragraph,
        >::new();
        state.focus();
        widget::Tree {
            tag: widget::tree::Tag::of::<
                text_input::State<<iced::Renderer as iced::advanced::text::Renderer>::Paragraph>,
            >(),
            state: widget::tree::State::new(state),
            children: Vec::new(),
        }
    }

    fn wrapper_tree(child: widget::Tree) -> widget::Tree {
        widget::Tree {
            tag: widget::tree::Tag::stateless(),
            state: widget::tree::State::None,
            children: vec![child],
        }
    }

    #[test]
    fn recursive_focus_detects_input_under_wrapper() {
        let tree = wrapper_tree(focused_input_tree());

        assert!(is_input_focused::<iced::Renderer>(&tree));
    }

    #[test]
    fn recursive_focus_ignores_unfocused_input_and_other_state() {
        let unfocused = {
            let state = text_input::State::<
                <iced::Renderer as iced::advanced::text::Renderer>::Paragraph,
            >::new();
            widget::Tree {
                tag: widget::tree::Tag::of::<
                    text_input::State<
                        <iced::Renderer as iced::advanced::text::Renderer>::Paragraph,
                    >,
                >(),
                state: widget::tree::State::new(state),
                children: Vec::new(),
            }
        };

        assert!(!is_input_focused::<iced::Renderer>(&wrapper_tree(
            unfocused
        )));
        assert!(!is_input_focused::<iced::Renderer>(&widget::Tree::empty()));
    }
}
