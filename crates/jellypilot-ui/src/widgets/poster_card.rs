//! Poster card widget with content-hugging bounds.
//!
//! Visual contract:
//! - The artwork and copy render exactly as provided; hover/press apply no
//!   overlay, lift, or tint — the card stays visually quiet.
//! - The copy region (title/caption) remains fixed below the artwork.
//! - No ghost background panel, border, or card-zone elevation shadow is drawn.
//! - Layout height hugs `poster_height + copy_height` with zero dead space.
//!
//! Note on keyboard focus:
//! `PosterCard` is currently mouse- and touch-interactive; keyboard focus
//! navigation across stream grids is reserved for a future navigation design pass.

use crate::tokens::TOKENS;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, Widget};
use iced::advanced::Shell;
use iced::border::Radius;
use iced::touch;
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// Default corner radius for the poster artwork.
pub const POSTER_RADIUS: f32 = TOKENS.radii.xl;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct State {
    is_pressed: bool,
}

/// A card widget displaying a poster artwork with copy beneath it.
///
/// Press handling is the only interaction: hover/press states change nothing
/// visually and never alter card layout geometry or shift copy text.
pub struct PosterCard<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    poster: Element<'a, Message, Theme, Renderer>,
    copy: Element<'a, Message, Theme, Renderer>,
    on_press: Option<Message>,
    width: Length,
    height: Length,
    radius: Radius,
}

impl<'a, Message, Theme, Renderer> PosterCard<'a, Message, Theme, Renderer> {
    /// Creates a new [`PosterCard`] with the given poster artwork and copy elements.
    pub fn new(
        poster: impl Into<Element<'a, Message, Theme, Renderer>>,
        copy: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self {
            poster: poster.into(),
            copy: copy.into(),
            on_press: None,
            width: Length::Fit,
            height: Length::Fit,
            radius: Radius::from(TOKENS.radii.xl),
        }
    }

    /// Sets the message to produce when the card is pressed.
    #[must_use]
    pub fn on_press(mut self, on_press: Message) -> Self {
        self.on_press = Some(on_press);
        self
    }

    /// Sets the optional message to produce when the card is pressed.
    #[must_use]
    pub fn on_press_maybe(mut self, on_press: Option<Message>) -> Self {
        self.on_press = on_press;
        self
    }

    /// Sets the width of the card.
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the card.
    #[must_use]
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the corner [`Radius`] for the poster artwork.
    #[must_use]
    pub fn radius(mut self, radius: impl Into<Radius>) -> Self {
        self.radius = radius.into();
        self
    }
}

/// Convenience constructor for a [`PosterCard`].
pub fn poster_card<'a, Message, Theme, Renderer>(
    poster: impl Into<Element<'a, Message, Theme, Renderer>>,
    copy: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> PosterCard<'a, Message, Theme, Renderer> {
    PosterCard::new(poster, copy)
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for PosterCard<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Renderer: 'a + iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [self.poster.as_widget_mut(), self.copy.as_widget_mut()]);
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(self.width).height(self.height);

        let poster_node =
            self.poster
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &limits.loose());
        let poster_size = poster_node.size();

        let copy_limits = limits.loose();
        let copy_node =
            self.copy
                .as_widget_mut()
                .layout(&mut tree.children[1], renderer, &copy_limits);
        let copy_size = copy_node.size();

        let content_width = poster_size.width.max(copy_size.width);
        let content_height = poster_size.height + copy_size.height;
        let final_size = limits.resolve(
            self.width,
            self.height,
            Size::new(content_width, content_height),
        );

        let mut poster_node = poster_node;
        let mut copy_node = copy_node;

        poster_node.move_to_mut(Point::ORIGIN);
        copy_node.move_to_mut(Point::new(0.0, poster_size.height));

        layout::Node::with_children(final_size, vec![poster_node, copy_node])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        let mut children = layout.children();
        if let (Some(poster_layout), Some(copy_layout)) = (children.next(), children.next()) {
            operation.traverse(&mut |operation| {
                self.poster.as_widget_mut().operate(
                    &mut tree.children[0],
                    poster_layout,
                    renderer,
                    operation,
                );
                self.copy.as_widget_mut().operate(
                    &mut tree.children[1],
                    copy_layout,
                    renderer,
                    operation,
                );
            });
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let mut children = layout.children();
        if let (Some(poster_layout), Some(copy_layout)) = (children.next(), children.next()) {
            self.poster.as_widget_mut().update(
                &mut tree.children[0],
                event,
                poster_layout,
                cursor,
                renderer,
                shell,
                viewport,
            );
            self.copy.as_widget_mut().update(
                &mut tree.children[1],
                event,
                copy_layout,
                cursor,
                renderer,
                shell,
                viewport,
            );
        }

        if shell.is_event_captured() {
            return;
        }

        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if self.on_press.is_some() && cursor.is_over(bounds) {
                    let state = tree.state.downcast_mut::<State>();
                    state.is_pressed = true;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                if let Some(on_press) = &self.on_press {
                    let state = tree.state.downcast_mut::<State>();
                    if state.is_pressed {
                        state.is_pressed = false;
                        if cursor.is_over(bounds) {
                            shell.publish(on_press.clone());
                        }
                        shell.capture_event();
                    }
                }
            }
            Event::Touch(touch::Event::FingerLost { .. }) => {
                let state = tree.state.downcast_mut::<State>();
                state.is_pressed = false;
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let mut children = layout.children();
        let Some(poster_layout) = children.next() else {
            return;
        };
        let Some(copy_layout) = children.next() else {
            return;
        };

        self.poster.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            poster_layout,
            cursor,
            viewport,
        );

        self.copy.as_widget().draw(
            &tree.children[1],
            renderer,
            theme,
            style,
            copy_layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let is_mouse_over = cursor.is_over(layout.bounds());

        if is_mouse_over && self.on_press.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = layout.children();
        let poster_layout = children.next()?;
        let copy_layout = children.next()?;

        let (poster_tree, copy_tree) = tree.children.split_at_mut(1);
        self.poster
            .as_widget_mut()
            .overlay(
                &mut poster_tree[0],
                poster_layout,
                renderer,
                viewport,
                translation,
            )
            .or_else(|| {
                self.copy.as_widget_mut().overlay(
                    &mut copy_tree[0],
                    copy_layout,
                    renderer,
                    viewport,
                    translation,
                )
            })
    }
}

impl<'a, Message, Theme, Renderer> From<PosterCard<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + iced::advanced::Renderer,
{
    fn from(card: PosterCard<'a, Message, Theme, Renderer>) -> Self {
        Element::new(card)
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::layout::{self, Layout};
    use iced::advanced::mouse;
    use iced::advanced::widget::{Tree, Widget};
    use iced::advanced::Shell;
    use iced::widget::{column, container, space, text};
    use iced::{Element, Event, Point, Rectangle, Size};

    use super::poster_card;
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestMessage {
        Clicked,
    }

    #[test]
    fn card_layout_height_hugs_poster_plus_copy_exactly() {
        let poster_height = 240.0;
        let copy_height = 46.0;
        let card_width = 160.0;

        let poster: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
            .width(card_width)
            .height(poster_height)
            .into();
        let copy: Element<'_, TestMessage, iced::Theme, ()> =
            container(column![text("Title").size(14), text("Caption").size(12),])
                .width(card_width)
                .height(copy_height)
                .into();

        let mut card = poster_card(poster, copy)
            .width(card_width)
            .on_press(TestMessage::Clicked);

        let mut tree = Tree::new(&card as &dyn Widget<TestMessage, iced::Theme, ()>);
        card.diff(&mut tree);
        let renderer = ();
        let limits = layout::Limits::new(Size::ZERO, Size::new(card_width, 1000.0));

        let node = card.layout(&mut tree, &renderer, &limits);
        assert_eq!(node.size().width, card_width);
        assert_eq!(node.size().height, poster_height + copy_height);

        let children: Vec<_> = node.children().to_vec();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size(), Size::new(card_width, poster_height));
        assert_eq!(children[0].bounds().position(), Point::ORIGIN);
        assert_eq!(children[1].size(), Size::new(card_width, copy_height));
        assert_eq!(
            children[1].bounds().position(),
            Point::new(0.0, poster_height)
        );
    }

    #[test]
    fn state_transitions_publish_on_press_and_manage_press_cycle() {
        let card_width = 160.0;
        let poster_height = 240.0;
        let copy_height = 46.0;

        let poster: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
            .width(card_width)
            .height(poster_height)
            .into();
        let copy: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
            .width(card_width)
            .height(copy_height)
            .into();

        let mut card = poster_card(poster, copy)
            .width(card_width)
            .on_press(TestMessage::Clicked);

        let mut tree = Tree::new(&card as &dyn Widget<TestMessage, iced::Theme, ()>);
        card.diff(&mut tree);
        let renderer = ();
        let limits = layout::Limits::new(Size::ZERO, Size::new(card_width, 1000.0));
        let node = card.layout(&mut tree, &renderer, &limits);
        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(Size::new(1000.0, 1000.0));

        let cursor_inside = mouse::Cursor::Available(Point::new(50.0, 50.0));
        let cursor_outside = mouse::Cursor::Available(Point::new(500.0, 500.0));

        // Pressing alone captures the event but must not activate the card.
        let mut messages = iced::advanced::shell::Bus::new();
        let mut shell = Shell::new(
            &iced::window::Headless,
            iced::advanced::shell::Waker::noop(),
            &mut messages,
        );
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            cursor_inside,
            &renderer,
            &mut shell,
            &viewport,
        );
        assert!(shell.is_event_captured());
        assert!(messages.is_empty());

        // Releasing inside activates the card once.
        let mut published = iced::advanced::shell::Bus::new();
        let mut shell = Shell::new(
            &iced::window::Headless,
            iced::advanced::shell::Waker::noop(),
            &mut published,
        );
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            cursor_inside,
            &renderer,
            &mut shell,
            &viewport,
        );
        assert!(shell.is_event_captured());
        assert_eq!(
            published
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            vec![TestMessage::Clicked]
        );

        // Releasing outside cancels the press.
        let mut messages = iced::advanced::shell::Bus::new();
        let mut shell = Shell::new(
            &iced::window::Headless,
            iced::advanced::shell::Waker::noop(),
            &mut messages,
        );
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            cursor_inside,
            &renderer,
            &mut shell,
            &viewport,
        );
        let mut published = iced::advanced::shell::Bus::new();
        let mut shell = Shell::new(
            &iced::window::Headless,
            iced::advanced::shell::Waker::noop(),
            &mut published,
        );
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            cursor_outside,
            &renderer,
            &mut shell,
            &viewport,
        );
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            cursor_inside,
            &renderer,
            &mut shell,
            &viewport,
        );
        assert!(
            published.is_empty(),
            "cancelled press must not activate on a later release"
        );
    }

    #[test]
    fn mouse_interaction_pointer_only_when_hovered_and_enabled() {
        let card_width = 160.0;
        let poster_height = 240.0;
        let copy_height = 46.0;

        let poster: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
            .width(card_width)
            .height(poster_height)
            .into();
        let copy: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
            .width(card_width)
            .height(copy_height)
            .into();

        let mut card_enabled = poster_card(poster, copy)
            .width(card_width)
            .on_press(TestMessage::Clicked);

        let mut tree = Tree::new(&card_enabled as &dyn Widget<TestMessage, iced::Theme, ()>);
        card_enabled.diff(&mut tree);
        let renderer = ();
        let limits = layout::Limits::new(Size::ZERO, Size::new(card_width, 1000.0));
        let node = card_enabled.layout(&mut tree, &renderer, &limits);
        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(Size::new(1000.0, 1000.0));

        let cursor_inside = mouse::Cursor::Available(Point::new(50.0, 50.0));
        let cursor_outside = mouse::Cursor::Available(Point::new(500.0, 500.0));

        assert_eq!(
            card_enabled.mouse_interaction(&tree, layout, cursor_inside, &viewport, &renderer),
            mouse::Interaction::Pointer
        );
        assert_eq!(
            card_enabled.mouse_interaction(&tree, layout, cursor_outside, &viewport, &renderer),
            mouse::Interaction::None
        );

        let poster2: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
            .width(card_width)
            .height(poster_height)
            .into();
        let copy2: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
            .width(card_width)
            .height(copy_height)
            .into();
        let card_disabled: Element<'_, TestMessage, iced::Theme, ()> =
            poster_card(poster2, copy2).width(card_width).into();

        assert_eq!(
            card_disabled.as_widget().mouse_interaction(
                &tree,
                layout,
                cursor_inside,
                &viewport,
                &renderer
            ),
            mouse::Interaction::None
        );
    }
}
