//! Parent-controlled floating popover widget.

use iced::advanced::{layout, mouse, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::keyboard::{key, Key};
use iced::widget::{container, opaque};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use super::positioning::{position_layer, Alignment, Placement, PositioningOptions};
use super::style;
use crate::tokens::TOKENS;

/// Placement and dismissal behavior for a [`popover`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopoverOptions {
    pub placement: Placement,
    pub alignment: Alignment,
    pub gap: f32,
    pub width: Option<f32>,
    pub match_trigger_width: bool,
    pub close_on_escape: bool,
    pub close_on_outside_press: bool,
    pub clamp_to_viewport: bool,
    pub flip_when_overflow: bool,
}

impl Default for PopoverOptions {
    fn default() -> Self {
        Self {
            placement: Placement::Below,
            alignment: Alignment::Start,
            gap: TOKENS.spacing.s2,
            width: None,
            match_trigger_width: false,
            close_on_escape: true,
            close_on_outside_press: true,
            clamp_to_viewport: true,
            flip_when_overflow: true,
        }
    }
}

/// Displays caller-composed content in an overlay anchored to `trigger`.
///
/// The parent owns `is_open` and handles `on_dismiss`. When enabled in
/// [`PopoverOptions`], Escape and primary pointer presses outside both the
/// trigger and floating panel publish `on_dismiss`.
pub fn popover<'a, Message>(
    trigger: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    is_open: bool,
    options: PopoverOptions,
    on_dismiss: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let content = opaque(
        container(content)
            .padding(TOKENS.spacing.s3)
            .width(options.width.map_or(Length::Shrink, Length::Fixed))
            .style(style::popover_surface),
    );

    Element::new(Popover {
        trigger: trigger.into(),
        content,
        is_open,
        options,
        on_dismiss,
    })
}

struct Popover<'a, Message> {
    trigger: Element<'a, Message>,
    content: Element<'a, Message>,
    is_open: bool,
    options: PopoverOptions,
    on_dismiss: Message,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Popover<'_, Message>
where
    Message: Clone,
{
    fn children(&self) -> Vec<widget::Tree> {
        vec![
            widget::Tree::new(&self.trigger),
            widget::Tree::new(&self.content),
        ]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(&[self.trigger.as_widget(), self.content.as_widget()]);
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

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        let mut children = tree.children.iter_mut();
        let trigger_tree = children.next().expect("popover trigger tree");
        let content_tree = children.next().expect("popover content tree");
        let trigger_overlay = self.trigger.as_widget_mut().overlay(
            trigger_tree,
            layout,
            renderer,
            viewport,
            translation,
        );
        let popover_overlay = self.is_open.then(|| {
            overlay::Element::new(Box::new(PopoverOverlay {
                content: &mut self.content,
                tree: content_tree,
                anchor_bounds: layout.bounds() + translation,
                viewport_bounds: *viewport,
                options: self.options,
                on_dismiss: self.on_dismiss.clone(),
            }))
        });

        if trigger_overlay.is_some() || popover_overlay.is_some() {
            Some(
                overlay::Group::with_children(
                    trigger_overlay.into_iter().chain(popover_overlay).collect(),
                )
                .overlay(),
            )
        } else {
            None
        }
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
}

struct PopoverOverlay<'a, 'b, Message> {
    content: &'b mut Element<'a, Message>,
    tree: &'b mut widget::Tree,
    anchor_bounds: Rectangle,
    viewport_bounds: Rectangle,
    options: PopoverOptions,
    on_dismiss: Message,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for PopoverOverlay<'_, '_, Message>
where
    Message: Clone,
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let viewport = viewport_for_layout(self.viewport_bounds, bounds);
        self.viewport_bounds = viewport;
        let mut limits = layout::Limits::new(
            Size::ZERO,
            if self.options.clamp_to_viewport {
                viewport.size()
            } else {
                Size::INFINITE
            },
        );

        if let Some(width) = self.options.width {
            limits = limits.width(Length::Fixed(width));
        } else if self.options.match_trigger_width {
            limits = limits.width(Length::Fixed(self.anchor_bounds.width));
        } else {
            limits = limits.width(Length::Shrink);
        }

        let node = self
            .content
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        let position = position_layer(
            self.anchor_bounds,
            node.size(),
            viewport,
            PositioningOptions {
                preferred: self.options.placement,
                alignment: self.options.alignment,
                gap: self.options.gap,
                clamp_to_viewport: self.options.clamp_to_viewport,
                flip_when_overflow: self.options.flip_when_overflow,
            },
        );

        node.move_to(position.point)
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        if self.options.close_on_escape && is_escape_press(event) {
            shell.publish(self.on_dismiss.clone());
            shell.capture_event();
            return;
        }

        let overlay_bounds = layout.bounds();
        if outside_press_action(
            self.options.close_on_outside_press,
            event,
            cursor,
            overlay_bounds,
            self.anchor_bounds,
        ) == OutsidePressAction::PublishDismissal
        {
            shell.publish(self.on_dismiss.clone());
            return;
        }

        self.content.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &overlay_bounds,
        );
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.content
            .as_widget()
            .draw(self.tree, renderer, theme, style, layout, cursor, &bounds);
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &self.viewport_bounds,
            Vector::ZERO,
        )
    }
}

fn viewport_for_layout(viewport: Rectangle, bounds: Size) -> Rectangle {
    if viewport.width > 0.0 && viewport.height > 0.0 {
        viewport
    } else {
        Rectangle::with_size(bounds)
    }
}

fn is_escape_press(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::Escape),
            ..
        })
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutsidePressAction {
    Ignore,
    PublishDismissal,
}

fn outside_press_action(
    enabled: bool,
    event: &Event,
    cursor: mouse::Cursor,
    overlay_bounds: Rectangle,
    anchor_bounds: Rectangle,
) -> OutsidePressAction {
    if enabled
        && primary_press_position(event, cursor).is_some_and(|position| {
            !overlay_bounds.contains(position) && !anchor_bounds.contains(position)
        })
    {
        OutsidePressAction::PublishDismissal
    } else {
        OutsidePressAction::Ignore
    }
}

fn primary_press_position(event: &Event, cursor: mouse::Cursor) -> Option<Point> {
    match event {
        Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
            cursor.position()
        }
        Event::Touch(iced::touch::Event::FingerPressed { position, .. }) => Some(*position),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::mouse;
    use iced::{Event, Point, Rectangle, Size};

    use super::{outside_press_action, primary_press_position, OutsidePressAction, PopoverOptions};
    use crate::tokens::TOKENS;

    #[test]
    fn defaults_enable_both_supported_dismissal_paths() {
        let options = PopoverOptions::default();

        assert!(options.close_on_escape);
        assert!(options.close_on_outside_press);
        assert_eq!(options.gap, TOKENS.spacing.s2);
    }

    #[test]
    fn touch_press_uses_event_position_without_a_mouse_cursor() {
        let point = Point::new(12.0, 34.0);
        let event = Event::Touch(iced::touch::Event::FingerPressed {
            id: iced::touch::Finger(1),
            position: point,
        });

        assert_eq!(
            primary_press_position(&event, mouse::Cursor::Unavailable),
            Some(point)
        );
    }

    #[test]
    fn outside_primary_press_publishes_dismissal_without_capture() {
        let event = Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left));

        assert_eq!(
            outside_press_action(
                true,
                &event,
                mouse::Cursor::Available(Point::new(20.0, 20.0)),
                Rectangle::new(Point::new(100.0, 100.0), Size::new(80.0, 60.0)),
                Rectangle::new(Point::new(100.0, 60.0), Size::new(40.0, 24.0)),
            ),
            OutsidePressAction::PublishDismissal
        );
    }
}
