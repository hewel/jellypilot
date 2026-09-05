//! Input-method tracking before widgets or overlays can capture an event.

use std::collections::VecDeque;

use iced::advanced::{layout, mouse, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use super::control_button::{FocusSnapshot, FocusVisibility};

/// Gives custom controls keyboard-only focus, including inside nested overlays.
/// Keep `visibility` for the lifetime of the window and wrap its root once.
pub fn focus_scope<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    visibility: FocusVisibility,
) -> Element<'a, Message> {
    Element::new(FocusScope {
        content: content.into(),
        visibility,
    })
}

struct FocusScope<'a, Message> {
    content: Element<'a, Message>,
    visibility: FocusVisibility,
}

#[derive(Default)]
struct State {
    // iced forwards exactly the root overlay's ignored events to the base, in order.
    // Retain their input state until that second pass; captured events never queue.
    pending: VecDeque<FocusSnapshot>,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for FocusScope<'_, Message> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(&[&self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self.content.as_widget_mut();
        let node = child.layout(&mut tree.children[0], renderer, limits);
        child.operate(
            &mut tree.children[0],
            Layout::new(&node),
            renderer,
            &mut self.visibility,
        );
        node
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
        let snapshot = input_method(event)
            .and_then(|_| tree.state.downcast_mut::<State>().pending.pop_front());
        let final_state = snapshot.map(|snapshot| {
            let final_state = self.visibility.snapshot();
            self.visibility.restore(snapshot);
            final_state
        });
        if final_state.is_none() {
            observe(&self.visibility, event, shell);
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if let Some(final_state) = final_state {
            self.visibility.restore(final_state);
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
        self.content.as_widget().draw(
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
        self.content.as_widget().mouse_interaction(
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
        self.content
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
        let content = self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        );
        let pending = &mut tree.state.downcast_mut::<State>().pending;
        content.map(|content| scoped_overlay(content, &mut self.visibility, Some(pending)))
    }
}

fn input_method(event: &Event) -> Option<bool> {
    match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed { .. }) => Some(true),
        Event::Mouse(mouse::Event::ButtonPressed(_))
        | Event::Touch(iced::touch::Event::FingerPressed { .. }) => Some(false),
        _ => None,
    }
}

fn observe<Message>(visibility: &FocusVisibility, event: &Event, shell: &mut Shell<'_, Message>) {
    let Some(keyboard) = input_method(event) else {
        return;
    };
    let changed = visibility.is_keyboard() != keyboard;
    // Do this before forwarding: captured events never reach the underlying root.
    // Nested overlays may observe a bubbling press again before the next event.
    visibility.set_keyboard(keyboard);
    if changed {
        shell.request_redraw();
    }
}

fn scoped_overlay<'a, Message: 'a>(
    content: overlay::Element<'a, Message, Theme, iced::Renderer>,
    visibility: &'a mut FocusVisibility,
    pending: Option<&'a mut VecDeque<FocusSnapshot>>,
) -> overlay::Element<'a, Message, Theme, iced::Renderer> {
    overlay::Element::new(Box::new(FocusOverlay {
        content,
        visibility,
        pending,
    }))
}

struct FocusOverlay<'a, Message> {
    content: overlay::Element<'a, Message, Theme, iced::Renderer>,
    visibility: &'a mut FocusVisibility,
    pending: Option<&'a mut VecDeque<FocusSnapshot>>,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for FocusOverlay<'_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let content = self.content.as_overlay_mut();
        let node = content.layout(renderer, bounds);
        content.operate(Layout::new(&node), renderer, self.visibility);
        node
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
        observe(self.visibility, event, shell);
        self.content
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
        if input_method(event).is_some() && shell.event_status() == iced::event::Status::Ignored {
            if let Some(pending) = self.pending.as_mut() {
                pending.push_back(self.visibility.snapshot());
            }
        }
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.content
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|content| scoped_overlay(content, self.visibility, None))
    }

    fn index(&self) -> f32 {
        self.content.as_overlay().index()
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::{clipboard, renderer::Headless, widget::operation::focusable};
    use iced::keyboard::{self, key::Named};
    use iced::widget::{column, text};
    use iced::{Font, Point};
    use iced_runtime::user_interface::{Cache, UserInterface};

    use super::*;
    use crate::overlay::{popover, PopoverOptions};
    use crate::{control_button, variants::ButtonVariant};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Message {
        Activate,
        Dismiss,
    }

    fn key(key: Named) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            modified_key: keyboard::Key::Named(key),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::NONE,
            text: None,
            repeat: false,
        })
    }

    fn view(visibility: FocusVisibility, open: bool, nested: bool) -> Element<'static, Message> {
        let trigger = control_button(None, Some("Account".to_owned()), ButtonVariant::Text)
            .id("trigger")
            .on_press(Message::Activate);
        let action = control_button(None, Some("Menu action".to_owned()), ButtonVariant::Text)
            .id("action")
            .on_press(Message::Activate);
        let content: Element<'_, Message> =
            column![text("Non-interactive header").height(40), action].into();
        let content = if nested {
            popover(
                text("Inner menu"),
                content,
                true,
                PopoverOptions::default(),
                Message::Dismiss,
            )
        } else {
            content
        };
        focus_scope(
            popover(
                trigger,
                content,
                open,
                PopoverOptions::default(),
                Message::Dismiss,
            ),
            visibility,
        )
    }

    fn focus_info(
        ui: &mut UserInterface<'_, Message, Theme, iced::Renderer>,
        renderer: &iced::Renderer,
        id: &'static str,
    ) -> (bool, Rectangle) {
        struct Query {
            id: widget::Id,
            result: Option<(bool, Rectangle)>,
        }
        impl widget::Operation for Query {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
                visit(self);
            }
            fn focusable(
                &mut self,
                id: Option<&widget::Id>,
                bounds: Rectangle,
                state: &mut dyn widget::operation::Focusable,
            ) {
                if id == Some(&self.id) {
                    self.result = Some((state.is_focused(), bounds));
                }
            }
        }
        let mut query = Query {
            id: widget::Id::new(id),
            result: None,
        };
        ui.operate(renderer, &mut query);
        query.result.expect("focusable control exists")
    }

    #[test]
    fn captured_panel_press_cancels_background_and_nested_control_activation() {
        let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
            Font::DEFAULT,
            14.0.into(),
            Some("tiny-skia"),
        ))
        .expect("software renderer");
        for nested in [false, true] {
            let mut ui = UserInterface::build(
                view(FocusVisibility::default(), true, nested),
                Size::new(400.0, 400.0),
                Cache::new(),
                &mut renderer,
            );
            let mut messages = Vec::new();
            for target in ["trigger", "action"] {
                ui.update(
                    &[key(Named::Tab)],
                    mouse::Cursor::Unavailable,
                    &mut renderer,
                    &mut clipboard::Null,
                    &mut messages,
                );
                ui.operate(
                    &renderer,
                    &mut focusable::focus::<()>(widget::Id::new(target)),
                );
                assert!(focus_info(&mut ui, &renderer, target).0);
                let action = focus_info(&mut ui, &renderer, "action").1;
                let header = mouse::Cursor::Available(Point::new(action.x + 8.0, action.y - 8.0));
                let (_, statuses) = ui.update(
                    &[
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        key(Named::Enter),
                    ],
                    header,
                    &mut renderer,
                    &mut clipboard::Null,
                    &mut messages,
                );
                assert_eq!(
                    statuses[0],
                    iced::event::Status::Captured,
                    "opaque panel consumes the press"
                );
                assert!(
                    messages.is_empty(),
                    "Enter must not activate focus hidden by a captured press"
                );
                assert!(
                    !focus_info(&mut ui, &renderer, target).0,
                    "Tab traversal must not count stale focus"
                );

                ui.operate(
                    &renderer,
                    &mut focusable::focus::<()>(widget::Id::new(target)),
                );
                ui.update(
                    &[
                        key(Named::Enter),
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    ],
                    header,
                    &mut renderer,
                    &mut clipboard::Null,
                    &mut messages,
                );
                assert_eq!(
                    messages,
                    vec![Message::Activate],
                    "a later press must not swallow an earlier key activation"
                );
                assert!(!focus_info(&mut ui, &renderer, target).0);
                messages.clear();
            }
        }
    }

    #[test]
    fn captured_escape_restores_keyboard_focus_before_any_subscription_message() {
        let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
            Font::DEFAULT,
            14.0.into(),
            Some("tiny-skia"),
        ))
        .expect("software renderer");
        let visibility = FocusVisibility::default();
        let bounds = Size::new(400.0, 400.0);
        let mut ui = UserInterface::build(
            view(visibility.clone(), true, false),
            bounds,
            Cache::new(),
            &mut renderer,
        );
        let mut messages = Vec::new();
        // iced processes the overlay's whole event batch before the base widget.
        let (_, statuses) = ui.update(
            &[
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                key(Named::Escape),
            ],
            mouse::Cursor::Available(Point::new(350.0, 350.0)),
            &mut renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        assert_eq!(
            statuses,
            vec![iced::event::Status::Ignored, iced::event::Status::Captured]
        );
        assert_eq!(messages.last(), Some(&Message::Dismiss));
        let mut ui = UserInterface::build(
            view(visibility, false, false),
            bounds,
            ui.into_cache(),
            &mut renderer,
        );
        ui.operate(
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("trigger")),
        );
        assert!(
            focus_info(&mut ui, &renderer, "trigger").0,
            "Esc restores the trigger synchronously"
        );
        messages.clear();
        ui.update(
            &[key(Named::Enter)],
            mouse::Cursor::Unavailable,
            &mut renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        assert_eq!(messages, vec![Message::Activate]);

        messages.clear();
        ui.update(
            &[Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))],
            mouse::Cursor::Unavailable,
            &mut renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        ui.operate(
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("trigger")),
        );
        ui.update(
            &[key(Named::Enter)],
            mouse::Cursor::Unavailable,
            &mut renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        assert!(
            messages.is_empty(),
            "pointer-origin restoration must not retain hidden activation"
        );
        assert!(!focus_info(&mut ui, &renderer, "trigger").0);
    }
}
