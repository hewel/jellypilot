//! Interruptible TV focus presentation with an optional fixed layout footprint.

use std::time::Instant;

use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::{Alignment, Element, Event, Length, Rectangle, Size, Theme, Vector};

use super::motion::{self, Tween};
use crate::tv::FOCUS_DURATION;

/// Builds the current focus appearance from eased progress in `0..=1`.
/// First mount presents its final value; subsequent changes are interruptible.
pub fn focus<'a, Message: 'a>(
    focused: bool,
    build: impl Fn(f32) -> Element<'a, Message> + 'a,
) -> Focus<'a, Message> {
    let target = if focused { 1.0 } else { 0.0 };
    Focus {
        content: build(target),
        build: Box::new(build),
        target,
        frame: None,
    }
}

pub struct Focus<'a, Message> {
    content: Element<'a, Message>,
    build: Box<dyn Fn(f32) -> Element<'a, Message> + 'a>,
    target: f32,
    frame: Option<(Size, Alignment, Alignment)>,
}

impl<Message> Focus<'_, Message> {
    /// Keeps the original slot while expanded artwork lays out, draws and receives input
    /// at its real enlarged bounds. The caller reserves clearance around the slot.
    #[must_use]
    pub fn frame(mut self, size: Size, horizontal: Alignment, vertical: Alignment) -> Self {
        self.frame = Some((size, horizontal, vertical));
        self
    }
}

#[derive(Default)]
struct State {
    target: Option<f32>,
    shown: f32,
    tween: Option<Tween>,
}

impl State {
    fn retarget(&mut self, target: f32, enabled: bool, now: Instant) {
        if !enabled || self.target.is_none() {
            self.shown = target;
            self.tween = None;
        } else if self.target != Some(target) {
            let from = self.tween.map_or(self.shown, |tween| tween.eased(now));
            self.shown = from;
            self.tween = (from != target).then(|| Tween::new(from, target, now, FOCUS_DURATION));
        }
        self.target = Some(target);
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Focus<'_, Message> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
    fn diff(&mut self, tree: &mut widget::Tree) {
        let state = tree.state.downcast_mut::<State>();
        state.retarget(self.target, motion::enabled(), Instant::now());
        self.content = (self.build)(state.shown);
        tree.diff_children(&mut [&mut self.content]);
    }
    fn size(&self) -> Size<Length> {
        self.frame.map_or_else(
            || self.content.as_widget().size(),
            |(size, _, _)| Size::new(Length::Fixed(size.width), Length::Fixed(size.height)),
        )
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        state.retarget(self.target, motion::enabled(), Instant::now());
        self.content = (self.build)(state.shown);
        tree.diff_children(&mut [&mut self.content]);
        let Some((size, horizontal, vertical)) = self.frame else {
            let child =
                self.content
                    .as_widget_mut()
                    .layout(&mut tree.children[0], renderer, limits);
            return layout::Node::with_children(child.size(), vec![child]);
        };
        let child = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::INFINITE),
        );
        let slot = limits.resolve(Length::Fixed(size.width), Length::Fixed(size.height), size);
        layout::Node::with_children(slot, vec![child.align(horizontal, vertical, slot)])
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if let Some(child) = layout.children().next() {
            self.content
                .as_widget_mut()
                .operate(&mut tree.children[0], child, renderer, operation);
        }
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
        let state = tree.state.downcast_mut::<State>();
        motion::tick_layout(
            &mut state.tween,
            &mut state.shown,
            self.target,
            event,
            shell,
        );
        if let Some(child) = layout.children().next() {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                child,
                cursor,
                renderer,
                shell,
                viewport,
            );
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
        if let Some(child) = layout.children().next() {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                child,
                cursor,
                viewport,
            );
        }
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        layout
            .children()
            .next()
            .map_or(mouse::Interaction::None, |child| {
                self.content.as_widget().mouse_interaction(
                    &tree.children[0],
                    child,
                    cursor,
                    viewport,
                    renderer,
                )
            })
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Vec<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        layout.children().next().map_or_else(Vec::new, |child| {
            self.content.as_widget_mut().overlay(
                &mut tree.children[0],
                child,
                renderer,
                viewport,
                translation,
            )
        })
    }
}

impl<'a, Message: 'a> From<Focus<'a, Message>> for Element<'a, Message> {
    fn from(focus: Focus<'a, Message>) -> Self {
        Element::new(focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn focus_reversal_continues_from_the_displayed_value_and_reduced_motion_settles() {
        let now = Instant::now();
        let mut state = State::default();
        state.retarget(0.0, true, now);
        state.retarget(1.0, true, now);
        let interrupted = now + Duration::from_millis(70);
        let shown = state.tween.expect("focus tween").eased(interrupted);
        state.retarget(0.0, true, interrupted);
        assert_eq!(state.shown, shown);
        assert!(state.shown > 0.0 && state.shown < 1.0);
        state.retarget(1.0, false, interrupted);
        assert_eq!(state.shown, 1.0);
        assert!(state.tween.is_none());
    }

    #[test]
    fn expanded_focus_keeps_its_slot_and_accepts_input_at_the_painted_edge() {
        use iced::advanced::{renderer::Headless, shell};
        use iced::widget::{button, space};
        use iced::Point;

        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            renderer::Settings::default(),
            Some("tiny-skia"),
        ))
        .expect("software layout renderer");
        for slot in [Size::new(80.0, 80.0), Size::new(278.0, 417.0)] {
            let factor = if slot.width == 80.0 { 1.05 } else { 1.03 };
            let mut control = focus(true, move |_| {
                button(space())
                    .padding(0)
                    .width(slot.width * factor)
                    .height(slot.height * factor)
                    .on_press(())
                    .into()
            })
            .frame(slot, Alignment::Center, Alignment::End);
            let mut tree = widget::Tree::new(&control as &dyn Widget<(), Theme, iced::Renderer>);
            control.diff(&mut tree);
            let node = control
                .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, slot))
                .move_to(Point::new(20.0, 20.0));
            let layout = Layout::new(&node);
            let child = layout.child(0).bounds();
            assert_eq!(node.size(), slot);
            assert!((child.center_x() - node.bounds().center_x()).abs() < 0.001);
            assert!((child.y + child.height - (20.0 + slot.height)).abs() < 0.001);
            let point = Point::new(child.x + 0.5, child.y + 0.5);
            assert!(!node.bounds().contains(point));
            let mut bus = shell::Bus::new();
            for event in [
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                control.update(
                    &mut tree,
                    &Event::Mouse(event),
                    layout,
                    mouse::Cursor::Available(point),
                    &renderer,
                    &mut Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus),
                    &Rectangle::with_size(Size::new(600.0, 600.0)),
                );
            }
            assert_eq!(bus.into_iter().count(), 1);
        }
    }
}
