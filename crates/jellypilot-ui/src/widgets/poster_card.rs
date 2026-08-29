//! Poster card widget with hover overlay lift on artwork and content-hugging bounds.
//!
//! Visual contract:
//! - Hover applies a subtle white overlay lift (~8% alpha) clipped to its rounded bounds.
//! - Press applies a slightly stronger overlay lift (~12% alpha).
//! - The copy region (title/caption) remains fixed below the artwork without overlay.
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
use iced::advanced::{Clipboard, Shell};
use iced::border::Radius;
use iced::touch;
use iced::{
    Background, Border, Color, Element, Event, Length, Point, Rectangle, Shadow, Size, Vector,
};

/// Default overlay alpha for unhovered / idle poster card artwork.
pub const ACTIVE_OVERLAY_ALPHA: f32 = 0.0;
/// Overlay alpha applied to the poster artwork when hovered (~8% white lift).
pub const HOVER_OVERLAY_ALPHA: f32 = 0.08;
/// Overlay alpha applied to the poster artwork when pressed (~12% white lift).
pub const PRESSED_OVERLAY_ALPHA: f32 = 0.12;
/// Overlay alpha applied when the card is disabled.
pub const DISABLED_OVERLAY_ALPHA: f32 = 0.0;

/// Default corner radius for the poster artwork overlay.
pub const POSTER_RADIUS: f32 = 8.0;

/// The interaction status of a [`PosterCard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// The card is interactive and not hovered.
    #[default]
    Active,
    /// The card is hovered by the cursor.
    Hovered,
    /// The card is currently pressed.
    Pressed,
    /// The card cannot be interacted with.
    Disabled,
}

/// Computes the poster artwork overlay alpha for a given card [`Status`].
#[must_use]
pub const fn overlay_alpha(status: Status) -> f32 {
    match status {
        Status::Active => ACTIVE_OVERLAY_ALPHA,
        Status::Hovered => HOVER_OVERLAY_ALPHA,
        Status::Pressed => PRESSED_OVERLAY_ALPHA,
        Status::Disabled => DISABLED_OVERLAY_ALPHA,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct State {
    is_pressed: bool,
}

/// A card widget displaying a poster artwork with copy beneath it.
///
/// Interaction state (hover/press) applies a rounded brightness overlay over only the
/// poster artwork without altering card layout geometry, distorting corner clipping, or shifting copy text.
pub struct PosterCard<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    poster: Element<'a, Message, Theme, Renderer>,
    copy: Element<'a, Message, Theme, Renderer>,
    on_press: Option<Message>,
    width: Length,
    height: Length,
    radius: Radius,
    status: Option<Status>,
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
            width: Length::Shrink,
            height: Length::Shrink,
            radius: Radius::from(TOKENS.radii.lg),
            status: None,
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

    /// Sets the corner [`Radius`] for the poster artwork overlay.
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

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.poster), Tree::new(&self.copy)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.poster.as_widget(), self.copy.as_widget()]);
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
        clipboard: &mut dyn Clipboard,
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
                clipboard,
                shell,
                viewport,
            );
            self.copy.as_widget_mut().update(
                &mut tree.children[1],
                event,
                copy_layout,
                cursor,
                renderer,
                clipboard,
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

        let current_status = if self.on_press.is_none() {
            Status::Disabled
        } else if cursor.is_over(bounds) {
            let state = tree.state.downcast_ref::<State>();
            if state.is_pressed {
                Status::Pressed
            } else {
                Status::Hovered
            }
        } else {
            Status::Active
        };

        if let Event::Window(iced::window::Event::RedrawRequested(_now)) = event {
            self.status = Some(current_status);
        } else if self.status.is_some_and(|status| status != current_status) {
            shell.request_redraw();
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

        let state = tree.state.downcast_ref::<State>();
        let is_mouse_over = cursor.is_over(layout.bounds());
        let status = if self.on_press.is_none() {
            Status::Disabled
        } else if is_mouse_over {
            if state.is_pressed {
                Status::Pressed
            } else {
                Status::Hovered
            }
        } else {
            Status::Active
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

        let alpha = overlay_alpha(status);
        if alpha > 0.0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: poster_layout.bounds(),
                    border: Border {
                        radius: self.radius,
                        color: Color::TRANSPARENT,
                        width: 0.0,
                    },
                    shadow: Shadow::default(),
                    snap: false,
                },
                Background::Color(Color::from_rgba(1.0, 1.0, 1.0, alpha)),
            );
        }

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

    use super::{
        overlay_alpha, poster_card, Status, ACTIVE_OVERLAY_ALPHA, DISABLED_OVERLAY_ALPHA,
        HOVER_OVERLAY_ALPHA, POSTER_RADIUS, PRESSED_OVERLAY_ALPHA,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestMessage {
        Clicked,
    }

    #[test]
    fn overlay_alpha_matches_design_spec() {
        assert_eq!(overlay_alpha(Status::Active), ACTIVE_OVERLAY_ALPHA);
        assert_eq!(overlay_alpha(Status::Active), 0.0);

        assert_eq!(overlay_alpha(Status::Hovered), HOVER_OVERLAY_ALPHA);
        assert_eq!(overlay_alpha(Status::Hovered), 0.08);

        assert_eq!(overlay_alpha(Status::Pressed), PRESSED_OVERLAY_ALPHA);
        assert_eq!(overlay_alpha(Status::Pressed), 0.12);

        assert_eq!(overlay_alpha(Status::Disabled), DISABLED_OVERLAY_ALPHA);
        assert_eq!(overlay_alpha(Status::Disabled), 0.0);
    }

    #[test]
    fn poster_radius_matches_token_lg() {
        assert_eq!(POSTER_RADIUS, crate::tokens::TOKENS.radii.lg);
    }

    #[test]
    fn poster_card_radius_builder_updates_radius() {
        let poster: Element<'_, TestMessage, iced::Theme, ()> =
            container(space::horizontal()).into();
        let copy: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal()).into();
        let custom_radius = iced::border::Radius::from(16.0);
        let card = poster_card(poster, copy).radius(custom_radius);
        assert_eq!(card.radius, custom_radius);
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
    fn no_layout_shift_between_card_states() {
        let card_width = 160.0;
        let poster_height = 240.0;
        let copy_height = 46.0;

        let make_card = || {
            let poster: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
                .width(card_width)
                .height(poster_height)
                .into();
            let copy: Element<'_, TestMessage, iced::Theme, ()> = container(space::horizontal())
                .width(card_width)
                .height(copy_height)
                .into();
            poster_card(poster, copy)
                .width(card_width)
                .on_press(TestMessage::Clicked)
        };

        let mut card = make_card();
        let mut tree = Tree::new(&card as &dyn Widget<TestMessage, iced::Theme, ()>);
        let renderer = ();
        let limits = layout::Limits::new(Size::ZERO, Size::new(card_width, 1000.0));

        let idle_node = card.layout(&mut tree, &renderer, &limits);

        // Hover or press do not affect layout calculation
        let hovered_node = card.layout(&mut tree, &renderer, &limits);
        assert_eq!(idle_node.size(), hovered_node.size());
        assert_eq!(idle_node.bounds(), hovered_node.bounds());
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
        let renderer = ();
        let limits = layout::Limits::new(Size::ZERO, Size::new(card_width, 1000.0));
        let node = card.layout(&mut tree, &renderer, &limits);
        let layout = Layout::new(&node);
        let mut clipboard = iced::advanced::clipboard::Null;
        let viewport = Rectangle::with_size(Size::new(1000.0, 1000.0));

        let cursor_inside = mouse::Cursor::Available(Point::new(50.0, 50.0));
        let cursor_outside = mouse::Cursor::Available(Point::new(500.0, 500.0));

        // 1. Mouse press inside -> captured, state.is_pressed becomes true
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            cursor_inside,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        assert!(shell.is_event_captured());
        let state = tree.state.downcast_ref::<super::State>();
        assert!(state.is_pressed);

        // 2. Mouse release inside -> published clicked message
        let mut published = Vec::new();
        let mut shell = Shell::new(&mut published);
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            cursor_inside,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        assert!(shell.is_event_captured());
        assert_eq!(published, vec![TestMessage::Clicked]);
        let state = tree.state.downcast_ref::<super::State>();
        assert!(!state.is_pressed);

        // 3. Mouse press inside then release outside -> not published
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            cursor_inside,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        let mut published = Vec::new();
        let mut shell = Shell::new(&mut published);
        card.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            cursor_outside,
            &renderer,
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        assert!(published.is_empty());
        let state = tree.state.downcast_ref::<super::State>();
        assert!(!state.is_pressed);
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
