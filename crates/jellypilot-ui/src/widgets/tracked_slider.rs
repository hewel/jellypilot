//! Slider events that distinguish pointer previews from discrete adjustments.

use iced::advanced::{layout, mouse, renderer, shell::Bus, widget, Layout, Shell, Widget};
use iced::{keyboard, touch, Element, Length, Rectangle, Size, Theme};

/// The interaction lifecycle of a tracked slider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A normal mouse or touch press began a drag, even if its value did not change.
    DragStarted,
    /// The value changed during a pointer drag.
    Changed(f64),
    /// The pointer drag finished.
    DragEnded,
    /// The value changed without a pointer drag (keyboard, wheel, or default reset).
    Adjusted(f64),
}

/// Wraps a slider built with `Event::Changed` and `.on_release(Event::DragEnded)`.
///
/// Feed the application's drag flag back through `dragging`. Rebuilding with
/// `false` cancels any unfinished pointer interaction without committing it.
pub fn tracked_slider<'a, Message: 'a>(
    slider: iced::widget::Slider<'a, f64, Event>,
    dragging: bool,
    on_event: impl Fn(Event) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(TrackedSlider {
        slider: slider.into(),
        dragging,
        on_event,
    })
}

struct TrackedSlider<'a, F> {
    slider: Element<'a, Event>,
    dragging: bool,
    on_event: F,
}

#[derive(Default)]
struct State {
    dragging: bool,
    modifiers: keyboard::Modifiers,
    restore_modifiers: bool,
    messages: Bus<Event>,
}

impl<Message, F: Fn(Event) -> Message> Widget<Message, Theme, iced::Renderer>
    for TrackedSlider<'_, F>
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut widget::Tree) {
        tree.diff_children(&mut [self.slider.as_widget_mut()]);
        let state = tree.state.downcast_mut::<State>();
        if state.dragging && !self.dragging {
            // The pinned slider's interaction state is private. Reinitialize it rather
            // than synthesizing a release, which would commit the cancelled preview.
            tree.children[0].state = self.slider.as_widget().state();
            state.dragging = false;
            state.restore_modifiers = true;
        }
    }

    fn size(&self) -> Size<Length> {
        self.slider.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.slider
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
        self.slider
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if let iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }

        let press = matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | iced::Event::Touch(touch::Event::FingerPressed { .. })
        ) && cursor.is_over(layout.bounds());
        if press {
            // Match the pinned slider's command-click branch: reset, never drag.
            let start = !state.modifiers.command();
            if start && !state.dragging {
                shell.publish((self.on_event)(Event::DragStarted));
            }
            state.dragging = start;
        }

        let dragging = state.dragging;
        let mut local = shell.local(&mut state.messages);
        let child = self.slider.as_widget_mut();
        if state.restore_modifiers {
            child.update(
                &mut tree.children[0],
                &iced::Event::Keyboard(keyboard::Event::ModifiersChanged(state.modifiers)),
                layout,
                cursor,
                renderer,
                &mut local,
                viewport,
            );
            state.restore_modifiers = false;
        }
        child.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            &mut local,
            viewport,
        );
        shell.merge(local, |event| {
            (self.on_event)(match event {
                Event::Changed(value) if !dragging => Event::Adjusted(value),
                event => event,
            })
        });

        // Keep this local until release: multiple input events can arrive before
        // the application processes DragStarted and rebuilds its view.
        if matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | iced::Event::Touch(
                    touch::Event::FingerLifted { .. } | touch::Event::FingerLost { .. }
                )
        ) {
            state.dragging = false;
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
        self.slider.as_widget().draw(
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
        self.slider.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::{renderer::Headless, shell::Waker};
    use iced::keyboard::key::Named;
    use iced::{Font, Point};

    use super::*;

    struct Harness {
        slider: Element<'static, Event>,
        tree: widget::Tree,
        renderer: iced::Renderer,
        node: layout::Node,
        messages: Bus<Event>,
    }

    impl Harness {
        fn new() -> Self {
            let renderer = iced::futures::executor::block_on(iced::Renderer::new(
                renderer::Settings {
                    font: Font::DEFAULT,
                    text_size: 14.0.into(),
                    line_height: crate::fonts::DEFAULT_LINE_HEIGHT,
                    metrics_hinting: false,
                },
                Some("tiny-skia"),
            ))
            .expect("software renderer");
            let mut slider = Self::view(50.0, false);
            let mut tree = widget::Tree::new(slider.as_widget());
            tree.diff(slider.as_widget_mut());
            let node = slider.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(100.0, 16.0)),
            );
            Self {
                slider,
                tree,
                renderer,
                node,
                messages: Bus::new(),
            }
        }

        fn view(value: f64, dragging: bool) -> Element<'static, Event> {
            tracked_slider(
                iced::widget::slider(0.0..=100.0, value, Event::Changed)
                    .width(100)
                    .default(25.0)
                    .shift_step(10.0)
                    .on_release(Event::DragEnded),
                dragging,
                |event| event,
            )
        }

        fn rebuild(&mut self, value: f64, dragging: bool) {
            self.slider = Self::view(value, dragging);
            self.tree.diff(self.slider.as_widget_mut());
            self.node = self.slider.as_widget_mut().layout(
                &mut self.tree,
                &self.renderer,
                &layout::Limits::new(Size::ZERO, Size::new(100.0, 16.0)),
            );
        }

        fn send(&mut self, event: iced::Event, x: f32) {
            self.slider.as_widget_mut().update(
                &mut self.tree,
                &event,
                Layout::new(&self.node),
                mouse::Cursor::Available(Point::new(x, 8.0)),
                &self.renderer,
                &mut Shell::new(&iced::window::Headless, Waker::noop(), &mut self.messages),
                &Rectangle::with_size(Size::new(100.0, 16.0)),
            );
        }

        fn take(&mut self) -> Vec<Event> {
            self.messages.drain().map(|(event, _)| event).collect()
        }
    }

    fn press() -> iced::Event {
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
    }

    fn release() -> iced::Event {
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
    }

    fn move_to(x: f32) -> iced::Event {
        iced::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x, 8.0),
        })
    }

    fn key(key: Named) -> iced::Event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key),
            modified_key: keyboard::Key::Named(key),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::ArrowUp),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::NONE,
            text: None,
            repeat: false,
        })
    }

    #[test]
    fn unchanged_press_starts_drag_and_busy_application_still_receives_release() {
        let mut harness = Harness::new();
        harness.send(press(), 50.0);
        harness.send(move_to(70.0), 70.0);
        harness.send(release(), 150.0);
        harness.send(release(), 150.0);
        assert_eq!(
            harness.take(),
            vec![Event::DragStarted, Event::Changed(70.0), Event::DragEnded]
        );
    }

    #[test]
    fn changed_press_publishes_start_before_preview_and_survives_rebuild() {
        let mut harness = Harness::new();
        harness.send(press(), 80.0);
        harness.rebuild(80.0, true);
        harness.send(move_to(90.0), 90.0);
        harness.send(release(), 90.0);
        assert_eq!(
            harness.take(),
            vec![
                Event::DragStarted,
                Event::Changed(80.0),
                Event::Changed(90.0),
                Event::DragEnded
            ]
        );
    }

    #[test]
    fn keyboard_and_control_wheel_adjust_without_a_release_commit() {
        let mut harness = Harness::new();
        harness.send(key(Named::ArrowUp), 50.0);
        harness.send(key(Named::ArrowDown), 50.0);
        harness.send(
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)),
            50.0,
        );
        harness.send(
            iced::Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
            }),
            50.0,
        );
        harness.send(move_to(80.0), 80.0);
        harness.send(release(), 80.0);
        assert_eq!(
            harness.take(),
            vec![
                Event::Adjusted(51.0),
                Event::Adjusted(50.0),
                Event::Adjusted(51.0)
            ]
        );
    }

    #[test]
    fn command_click_resets_without_starting_drag() {
        let mut harness = Harness::new();
        let command = if cfg!(target_os = "macos") {
            keyboard::Modifiers::LOGO
        } else {
            keyboard::Modifiers::CTRL
        };
        harness.send(
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(command)),
            50.0,
        );
        harness.send(press(), 80.0);
        harness.send(move_to(90.0), 90.0);
        harness.send(release(), 90.0);
        assert_eq!(harness.take(), vec![Event::Adjusted(25.0)]);
    }

    #[test]
    fn cancellation_discards_residual_drag_and_preserves_held_shift_step() {
        let mut harness = Harness::new();
        harness.send(press(), 80.0);
        harness.rebuild(80.0, true);
        harness.send(
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::SHIFT,
            )),
            80.0,
        );
        harness.take();
        harness.rebuild(50.0, false);
        harness.send(move_to(90.0), 90.0);
        harness.send(release(), 90.0);
        assert_eq!(harness.take(), Vec::<Event>::new());

        harness.send(key(Named::ArrowUp), 50.0);
        assert_eq!(harness.take(), vec![Event::Adjusted(60.0)]);
        harness.send(press(), 70.0);
        harness.send(release(), 70.0);
        assert_eq!(
            harness.take(),
            vec![Event::DragStarted, Event::Changed(70.0), Event::DragEnded]
        );
    }

    #[test]
    fn touch_drag_ends_once_on_lift_or_loss() {
        let mut harness = Harness::new();
        for lost in [false, true] {
            harness.rebuild(50.0, false);
            let id = touch::Finger(1);
            let position = Point::new(50.0, 8.0);
            harness.send(
                iced::Event::Touch(touch::Event::FingerPressed { id, position }),
                50.0,
            );
            harness.send(
                iced::Event::Touch(touch::Event::FingerMoved {
                    id,
                    position: Point::new(70.0, 8.0),
                }),
                70.0,
            );
            let end = if lost {
                touch::Event::FingerLost { id, position }
            } else {
                touch::Event::FingerLifted { id, position }
            };
            harness.send(iced::Event::Touch(end), 150.0);
            harness.send(release(), 150.0);
            assert_eq!(
                harness.take(),
                vec![Event::DragStarted, Event::Changed(70.0), Event::DragEnded]
            );
        }
    }
}
