//! Text-input wrapper that lets a parent own Escape semantics.
//!
//! iced's stock text input consumes Escape after clearing its own focus. This
//! wrapper observes the child focus before forwarding the event, so a parent
//! can clear a search draft (or close an anchored search layer) as one action.

use iced::advanced::{layout, mouse, overlay, renderer, widget, Clipboard, Layout, Shell, Widget};
use iced::keyboard::{key, Key};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

/// Wraps a text input so focused Escape publishes `on_escape` before iced can
/// consume the key and discard the child's focus state.
pub fn clear_on_escape<'a, Message>(
    input: impl Into<Element<'a, Message>>,
    on_escape: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    Element::new(EscapeInput {
        input: input.into(),
        on_escape,
    })
}

struct EscapeInput<'a, Message, Renderer = iced::Renderer> {
    input: Element<'a, Message, Theme, Renderer>,
    on_escape: Message,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for EscapeInput<'_, Message>
where
    Message: Clone,
{
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.input)]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(&[self.input.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        self.input.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.input.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.input
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
        self.input
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
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if is_escape(event)
            && child_is_focused(
                self.input.as_widget_mut(),
                &mut tree.children[0],
                layout,
                renderer,
            )
        {
            shell.publish(self.on_escape.clone());
            shell.capture_event();
            return;
        }
        self.input.as_widget_mut().update(
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
        self.input.as_widget().draw(
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
        self.input.as_widget().mouse_interaction(
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
        self.input.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

fn child_is_focused<Message>(
    input: &mut dyn Widget<Message, Theme, iced::Renderer>,
    tree: &mut widget::Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
) -> bool {
    struct FocusProbe {
        focused: bool,
    }

    impl widget::Operation for FocusProbe {
        fn traverse(&mut self, operation: &mut dyn FnMut(&mut dyn widget::Operation)) {
            operation(self);
        }

        fn focusable(
            &mut self,
            _id: Option<&widget::Id>,
            _bounds: Rectangle,
            state: &mut dyn widget::operation::Focusable,
        ) {
            self.focused |= state.is_focused();
        }
    }

    let mut probe = FocusProbe { focused: false };
    input.operate(tree, layout, renderer, &mut probe);
    probe.focused
}

fn is_escape(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(key::Named::Escape),
            ..
        })
    )
}
