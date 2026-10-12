//! Text-input wrapper that lets a parent own Escape semantics.
//!
//! iced's stock text input consumes Escape after clearing its own focus. This
//! wrapper observes the child focus before forwarding the event, so a parent
//! can clear a search draft (or close an anchored search layer) as one action.

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::keyboard::{key, Key};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

/// Wraps a text input so focused Escape publishes `on_escape` before iced can
/// consume the key and discard the child's focus state.
pub fn clear_on_escape<'a, Message>(
    input: impl Widget<Message, Theme, iced::Renderer> + 'a,
    on_escape: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    (EscapeInput {
        input: input._boxed(),
        on_escape,
    })
    ._boxed()
}

struct EscapeInput<'a, Message, Renderer = iced::Renderer> {
    input: Element<'a, Message, Theme, Renderer>,
    on_escape: Message,
}

impl<Message> iced::advanced::widget::Meta for EscapeInput<'_, Message> where Message: Clone {}

impl<Message> Widget<Message, Theme, iced::Renderer> for EscapeInput<'_, Message>
where
    Message: Clone,
{
    fn diff(&mut self, tree: &mut widget::Tree) {
        tree.diff_children(&mut [&mut self.input]);
    }

    fn size(&self) -> Size<Length> {
        self.input.size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) {
        self.input.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
        tree.children[0].translation = iced::Vector::ZERO;
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.input
            .operate(&mut tree.children[0], layout, viewport, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if is_escape(event)
            && child_is_focused(
                &mut self.input,
                &mut tree.children[0],
                layout,
                viewport,
                renderer,
            )
        {
            shell.publish(self.on_escape.clone());
            shell.capture_event();
            return;
        }
        self.input.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
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
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.input.draw(
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
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.input
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.input.overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}

pub(crate) fn child_is_focused<Message, Renderer: iced::advanced::Renderer>(
    input: &mut dyn Widget<Message, Theme, Renderer>,
    tree: &mut widget::Tree,
    layout: Layout,
    viewport: &Rectangle,
    renderer: &Renderer,
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
    input.operate(tree, layout, viewport, renderer, &mut probe);
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
