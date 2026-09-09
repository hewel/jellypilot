//! A visual-only wrapper for temporarily obscured application surfaces.

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

/// Draws `content` while excluding it from input, overlays, and focus traversal.
/// Preserves the content's widget state when the wrapper is added or removed.
pub fn inert<'a, Message>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message>
where
    Message: 'a + 'static,
{
    Element::new(Inert {
        content: content.into(),
        visible: true,
    })
}

/// Keeps layout and widget state, but suppresses drawing and interaction.
/// Used when an opaque replacement must not render a second native video surface.
pub fn concealed<'a, Message>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message>
where
    Message: 'a + 'static,
{
    Element::new(Inert {
        content: content.into(),
        visible: false,
    })
}

struct Inert<'a, Message> {
    content: Element<'a, Message>,
    visible: bool,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Inert<'_, Message>
where
    Message: 'static,
{
    fn tag(&self) -> widget::tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> widget::tree::State {
        self.content.as_widget().state()
    }

    fn diff(&mut self, tree: &mut widget::Tree) {
        self.content.as_widget_mut().diff(tree);
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
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn update(
        &mut self,
        _tree: &mut widget::Tree,
        _event: &Event,
        _layout: Layout<'_>,
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
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !self.visible {
            return;
        }
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }

    fn overlay<'a>(
        &'a mut self,
        _tree: &'a mut widget::Tree,
        _layout: Layout<'a>,
        _renderer: &iced::Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        None
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
        let node = visible
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        visible.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("background-control")),
        );

        let mut obscured = inert(control());
        tree.diff(&mut obscured);
        let node = obscured
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert!(obscured
            .as_widget_mut()
            .overlay(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &viewport,
                Vector::ZERO
            )
            .is_none());
        // A focus operation must not traverse the obscured background.
        obscured.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("another-control")),
        );

        let mut restored = control();
        tree.diff(&mut restored);
        let node = restored
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert!(
            restored
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &viewport,
                    Vector::ZERO
                )
                .is_some(),
            "the original keyboard focus hint returns after the modal closes"
        );
    }
}
