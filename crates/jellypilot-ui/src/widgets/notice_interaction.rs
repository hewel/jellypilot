//! Observe notice interaction without owning focus or intercepting child actions.

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

/// Whether the pointer or keyboard focus currently belongs to a notice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoticeInteraction {
    pub hovered: bool,
    pub focused: bool,
}

/// Observes the content and its focusable descendants without changing focus.
/// Keep each notice keyed by its operation ID when presenting a changing list.
pub fn notice_interaction<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    on_change: impl Fn(NoticeInteraction) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(NoticeObserver {
        content: content.into(),
        on_change,
    })
}

struct NoticeObserver<'a, Message, F> {
    content: Element<'a, Message>,
    on_change: F,
}

#[derive(Default)]
struct State {
    published: NoticeInteraction,
}

#[derive(Default)]
struct FocusProbe(bool);

impl widget::Operation for FocusProbe {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn widget::operation::Focusable,
    ) {
        self.0 |= state.is_focused();
    }
}

impl<Message, F: Fn(NoticeInteraction) -> Message> Widget<Message, Theme, iced::Renderer>
    for NoticeObserver<'_, Message, F>
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut widget::Tree) {
        tree.diff_children(&mut [&mut self.content]);
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
        self.content
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
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
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
        let mut focus = FocusProbe::default();
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut focus);
        let interaction = NoticeInteraction {
            hovered: cursor.is_over(layout.bounds()) && cursor.is_over(*viewport),
            focused: focus.0,
        };
        let state = tree.state.downcast_mut::<State>();
        if state.published != interaction {
            state.published = interaction;
            shell.publish((self.on_change)(interaction));
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

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use iced::advanced::renderer::Headless;
    use iced::advanced::shell::{Bus, Waker};
    use iced::advanced::widget::operation::focusable;
    use iced::keyboard::{self, key::Named};
    use iced::widget::row;
    use iced::{window, Point};
    use iced_runtime::user_interface::{Cache, UserInterface};

    use super::*;
    use crate::variants::ButtonVariant;
    use crate::widgets::control_button::control_button;
    use crate::widgets::control_button::FocusVisibility;
    use crate::widgets::focus_scope::focus_scope;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Message {
        Interaction(NoticeInteraction),
        Undo,
        Dismiss,
    }

    fn key(key: Named) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            modified_key: keyboard::Key::Named(key),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Tab),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::NONE,
            text: None,
            repeat: false,
        })
    }

    #[test]
    fn observes_focus_and_hover_without_stealing_focus_or_child_activation() {
        let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
            renderer::Settings::default(),
            Some("tiny-skia"),
        ))
        .expect("software renderer");
        let content = focus_scope(
            notice_interaction(
                row![
                    control_button(None, Some("Undo".into()), ButtonVariant::Text)
                        .id("undo")
                        .on_press(Message::Undo),
                    control_button(None, Some("Dismiss".into()), ButtonVariant::Text)
                        .id("dismiss")
                        .on_press(Message::Dismiss),
                ],
                Message::Interaction,
            ),
            FocusVisibility::default(),
        );
        let mut ui =
            UserInterface::build(content, Size::new(320.0, 80.0), Cache::new(), &mut renderer);
        let mut messages = Bus::new();
        let update = |ui: &mut UserInterface<'_, Message, Theme, iced::Renderer>,
                      renderer: &mut iced::Renderer,
                      messages: &mut Bus<Message>,
                      event,
                      cursor| {
            let _ = ui.update(
                &window::Headless,
                &Waker::noop(),
                &[event],
                cursor,
                renderer,
                messages,
            );
        };
        update(
            &mut ui,
            &mut renderer,
            &mut messages,
            key(Named::Tab),
            mouse::Cursor::Unavailable,
        );
        assert!(messages.drain().next().is_none());
        ui.operate(
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("undo")),
        );
        update(
            &mut ui,
            &mut renderer,
            &mut messages,
            Event::Window(window::Event::RedrawRequested(Instant::now())),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            messages
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            vec![Message::Interaction(NoticeInteraction {
                hovered: false,
                focused: true,
            })]
        );
        update(
            &mut ui,
            &mut renderer,
            &mut messages,
            key(Named::Enter),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            messages
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            vec![Message::Undo]
        );
        ui.operate(
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("dismiss")),
        );
        update(
            &mut ui,
            &mut renderer,
            &mut messages,
            Event::Window(window::Event::RedrawRequested(Instant::now())),
            mouse::Cursor::Available(Point::new(4.0, 4.0)),
        );
        assert_eq!(
            messages
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            vec![Message::Interaction(NoticeInteraction {
                hovered: true,
                focused: true,
            })]
        );
        ui.operate(
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("outside")),
        );
        update(
            &mut ui,
            &mut renderer,
            &mut messages,
            Event::Mouse(mouse::Event::CursorLeft),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(
            messages
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            vec![Message::Interaction(NoticeInteraction::default())]
        );
    }
}
