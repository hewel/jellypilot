//! Opt-in text hints for controls whose full value must also be readable by keyboard.

use iced::advanced::{layout, mouse, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::widget::{container, text};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use super::{position_layer, tooltip_element, PositioningOptions, TooltipOptions};
use crate::tokens::TOKENS;

/// Adds a full-value hint on keyboard focus while retaining the ordinary delayed hover tooltip.
pub fn focus_tooltip<'a, Message: 'a>(
    trigger: impl Into<Element<'a, Message>>,
    content: impl Into<String>,
    options: TooltipOptions,
) -> Element<'a, Message> {
    if !options.enabled {
        return trigger.into();
    }
    let content = content.into();
    Element::new(FocusTooltip {
        trigger: tooltip_element(
            trigger,
            text(content.clone())
                .size(TOKENS.font_sizes.s12)
                .wrapping(text::Wrapping::WordOrGlyph),
            options,
        ),
        hint: container(
            text(content)
                .size(TOKENS.font_sizes.s12)
                .wrapping(text::Wrapping::WordOrGlyph),
        )
        .max_width(options.max_width)
        .padding(TOKENS.spacing.s2)
        .style(super::style::tooltip_surface)
        .into(),
        options,
    })
}

struct FocusTooltip<'a, Message> {
    trigger: Element<'a, Message>,
    hint: Element<'a, Message>,
    options: TooltipOptions,
}

#[derive(Default)]
struct FocusProbe(bool);

impl widget::Operation for FocusProbe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
        operate(self);
    }

    fn focusable(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn widget::operation::Focusable,
    ) {
        self.0 |= state.is_focused();
    }

    fn custom(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn std::any::Any,
    ) {
        if let Some(visible) = crate::widgets::control_button::visible_focus(state) {
            self.0 = visible;
        }
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for FocusTooltip<'_, Message> {
    fn children(&self) -> Vec<widget::Tree> {
        vec![
            widget::Tree::new(&self.trigger),
            widget::Tree::new(&self.hint),
        ]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(&[&self.trigger, &self.hint]);
    }

    fn size(&self) -> Size<Length> {
        self.trigger.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.trigger.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.trigger
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        self.trigger.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
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
        self.trigger.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
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
        self.trigger.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        self.trigger
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        let mut focus = FocusProbe::default();
        self.trigger
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut focus);
        if focus.0 {
            Some(overlay::Element::new(Box::new(FocusHint {
                content: &mut self.hint,
                tree: &mut tree.children[1],
                anchor: layout.bounds() + translation,
                options: self.options,
            })))
        } else {
            self.trigger.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            )
        }
    }
}

struct FocusHint<'a, 'b, Message> {
    content: &'b mut Element<'a, Message>,
    tree: &'b mut widget::Tree,
    anchor: Rectangle,
    options: TooltipOptions,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for FocusHint<'_, '_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let viewport = Rectangle::with_size(bounds);
        let node = self.content.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let position = position_layer(
            self.anchor,
            node.size(),
            viewport,
            PositioningOptions {
                preferred: self.options.placement,
                gap: TOKENS.spacing.s1_5,
                alignment: super::Alignment::Center,
                clamp_to_viewport: true,
                flip_when_overflow: true,
            },
        );
        node.move_to(position.point)
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::operation::focusable;
    use iced::{Font, Point};

    use super::*;
    use crate::{control_button, variants::ButtonVariant};

    #[test]
    fn nonclickable_selection_reveals_full_value_only_while_focused() {
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            Font::DEFAULT,
            14.0.into(),
            Some("tiny-skia"),
        ))
        .expect("software layout renderer");
        let mut control = focus_tooltip(
            control_button::<()>(
                None,
                Some("Selected profile".to_owned()),
                ButtonVariant::Secondary,
            )
            .id("selected-profile"),
            "UnbrokenUsername".repeat(12),
            TooltipOptions::default(),
        );
        let mut tree = widget::Tree::new(&control);
        let viewport = Rectangle::with_size(Size::new(368.0, 300.0));
        let node = control
            .as_widget_mut()
            .layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, viewport.size()),
            )
            .move_to(Point::new(0.0, 250.0));
        let layout = Layout::new(&node);
        assert!(control
            .as_widget_mut()
            .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO,)
            .is_none());
        control.as_widget_mut().operate(
            &mut tree,
            layout,
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("selected-profile")),
        );
        let mut hint = control
            .as_widget_mut()
            .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO)
            .expect("keyboard focus reveals the selected account's full value");
        let hint_bounds = hint
            .as_overlay_mut()
            .layout(&renderer, viewport.size())
            .bounds();
        assert!(viewport.contains(hint_bounds.position()));
        assert!(hint_bounds.x + hint_bounds.width <= viewport.width);
        assert!(hint_bounds.y + hint_bounds.height <= viewport.height);
        assert!(
            hint_bounds.height > 48.0,
            "the complete unbroken value must wrap within the hint"
        );
        drop(hint);
        let mut messages = Vec::new();
        control.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        assert!(control
            .as_widget_mut()
            .overlay(&mut tree, layout, &renderer, &viewport, Vector::ZERO,)
            .is_none());
    }
}
