//! A visual-only wrapper for temporarily obscured application surfaces.

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

/// Draws `content` while excluding it from input, overlays, and focus traversal.
/// Preserves the content's widget state when the wrapper is added or removed.
pub fn inert<'a, Message>(
    content: impl Widget<Message, Theme, iced::Renderer> + 'a,
) -> Element<'a, Message>
where
    Message: 'a + 'static,
{
    (Inert {
        content: content._boxed(),
        visible: true,
    })
    ._boxed()
}

/// Keeps layout and widget state, but suppresses drawing and interaction.
/// Used when an opaque replacement must not render a second native video surface.
pub fn concealed<'a, Message>(
    content: impl Widget<Message, Theme, iced::Renderer> + 'a,
) -> Element<'a, Message>
where
    Message: 'a + 'static,
{
    (Inert {
        content: content._boxed(),
        visible: false,
    })
    ._boxed()
}

struct Inert<'a, Message> {
    content: Element<'a, Message>,
    visible: bool,
}

impl<Message> iced::advanced::widget::Meta for Inert<'_, Message> where Message: 'static {}

impl<Message> Widget<Message, Theme, iced::Renderer> for Inert<'_, Message>
where
    Message: 'static,
{
    fn tag(&self) -> widget::tree::Tag {
        self.content.tag()
    }

    fn state(&self) -> widget::tree::State {
        self.content.state()
    }

    fn diff(&mut self, tree: &mut widget::Tree) {
        self.content.diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) {
        self.content.layout(tree, renderer, limits)
    }

    fn update(
        &mut self,
        _tree: &mut widget::Tree,
        _event: &Event,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
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
        if !self.visible {
            return;
        }
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }

    fn overlay<'a>(
        &'a mut self,
        _tree: &'a mut widget::Tree,
        _layout: Layout,
        _renderer: &iced::Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
        _window: Size,
    ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::renderer::{Headless, Settings as RendererSettings};
    use iced::advanced::widget::operation::focusable;

    use super::*;
    use crate::overlay::{focus_tooltip, TooltipOptions};
    use crate::{control_button, fonts::DEFAULT_LINE_HEIGHT, variants::ButtonVariant};

    #[test]
    fn obscured_control_retains_focus_without_exposing_focus_or_overlays() {
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            RendererSettings {
                font: iced::Font::DEFAULT,
                text_size: 14.0.into(),
                line_height: DEFAULT_LINE_HEIGHT,
                metrics_hinting: false,
            },
            Some("tiny-skia"),
        ))
        .expect("software layout renderer");
        let control = || {
            focus_tooltip(
                control_button::<()>(
                    None,
                    Some("Background control".to_owned()),
                    ButtonVariant::Secondary,
                )
                .id("background-control"),
                "Keyboard focus hint",
                TooltipOptions::default(),
            )
        };
        let mut visible = control();
        let mut tree = widget::Tree::new(&visible);
        tree.diff(&mut visible);
        let viewport = Rectangle::with_size(Size::new(368.0, 300.0));
        let limits = layout::Limits::new(Size::ZERO, viewport.size());
        visible.layout(&mut tree, &renderer, &limits);
        let node = Layout::new(tree.size);
        visible.operate(
            &mut tree,
            node,
            &viewport,
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("background-control")),
        );

        let mut obscured = inert(control());
        tree.diff(&mut obscured);
        obscured.layout(&mut tree, &renderer, &limits);
        let node = Layout::new(tree.size);
        assert!(obscured
            .overlay(
                &mut tree,
                node,
                &renderer,
                &viewport,
                Vector::ZERO,
                viewport.size()
            )
            .is_empty());
        // A focus operation must not traverse the obscured background.
        obscured.operate(
            &mut tree,
            node,
            &viewport,
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("another-control")),
        );

        let mut restored = control();
        tree.diff(&mut restored);
        restored.layout(&mut tree, &renderer, &limits);
        let node = Layout::new(tree.size);
        assert!(
            !restored
                .overlay(
                    &mut tree,
                    node,
                    &renderer,
                    &viewport,
                    Vector::ZERO,
                    viewport.size()
                )
                .is_empty(),
            "the original keyboard focus hint returns after the modal closes"
        );
    }
}
