//! Parent-controlled floating popover widget.

use iced::advanced::Renderer as _;
use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::keyboard::{key, Key};
use iced::widget::{container, opaque};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use super::positioning::{position_layer, Alignment, Placement, PositioningOptions};
use super::style;
use crate::tokens::TOKENS;
use crate::widgets::motion;

/// Semantic appearance of the floating surface; ordinary popovers keep their default style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PopoverAppearance {
    #[default]
    Default,
    Account,
    EmbeddedPlayer,
    EmbeddedQueue,
    PlaybackInformation,
    /// Read-only track list anchored to a Media Specifications chip.
    TrackList,
}

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
    /// Consume the outside press that dismisses the panel so it cannot also
    /// activate whatever sits underneath (e.g. the video's pause toggle).
    pub consume_outside_press: bool,
    pub clamp_to_viewport: bool,
    pub flip_when_overflow: bool,
    pub appearance: PopoverAppearance,
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
            consume_outside_press: false,
            clamp_to_viewport: true,
            flip_when_overflow: true,
            appearance: PopoverAppearance::Default,
        }
    }
}

/// Displays caller-composed content in an overlay anchored to `trigger`.
///
/// The parent owns `is_open` and handles `on_dismiss`. When enabled in
/// [`PopoverOptions`], Escape and primary pointer presses outside both the
/// trigger and floating panel publish `on_dismiss`.
/// While open, trigger hints are suppressed; overlays inside the panel remain available.
pub fn popover<'a, Message>(
    trigger: impl Widget<Message, Theme, iced::Renderer> + 'a,
    content: impl Widget<Message, Theme, iced::Renderer> + 'a,
    is_open: bool,
    options: PopoverOptions,
    on_dismiss: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    // The reveal keeps the panel mounted through its exit; the overlay
    // below stays alive while the reveal reports activity.
    let content = motion::reveal(
        opaque(
            container(content)
                .padding(match options.appearance {
                    PopoverAppearance::Default => TOKENS.spacing.s3,
                    PopoverAppearance::Account => TOKENS.spacing.s2_5,
                    PopoverAppearance::EmbeddedPlayer => TOKENS.spacing.s3,
                    PopoverAppearance::EmbeddedQueue | PopoverAppearance::PlaybackInformation => {
                        TOKENS.spacing.s2
                    }
                    PopoverAppearance::TrackList => TOKENS.spacing.s2,
                })
                .width(options.width.map_or(Length::Fit, Length::Fixed))
                .style(match options.appearance {
                    PopoverAppearance::Default => style::popover_surface,
                    PopoverAppearance::Account => crate::widgets::sidebar::popover,
                    PopoverAppearance::EmbeddedPlayer => crate::widgets::embedded_player::popover,
                    PopoverAppearance::EmbeddedQueue => {
                        crate::widgets::embedded_player::queue_panel
                    }
                    PopoverAppearance::PlaybackInformation => {
                        crate::widgets::embedded_player::information_panel
                    }
                    PopoverAppearance::TrackList => style::track_list_surface,
                }),
        )
        ._boxed(),
        is_open,
        true,
        TOKENS.durations.ms200,
    );

    Popover {
        trigger: trigger._boxed(),
        content,
        is_open,
        options,
        on_dismiss,
    }
    ._boxed()
}

struct Popover<'a, Message> {
    trigger: Element<'a, Message>,
    content: Element<'a, Message>,
    is_open: bool,
    options: PopoverOptions,
    on_dismiss: Message,
}

impl<Message> widget::Meta for Popover<'_, Message> {}

impl<Message> Widget<Message, Theme, iced::Renderer> for Popover<'_, Message>
where
    Message: Clone,
{
    fn diff(&mut self, tree: &mut widget::Tree) {
        tree.diff_children(&mut [&mut self.trigger, &mut self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.trigger.size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) {
        self.trigger.layout(&mut tree.children[0], renderer, limits);
        tree.children[0].translation = Vector::ZERO;
        tree.size = tree.children[0].size;
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
        self.trigger.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        // While closed, the panel only receives redraw ticks so a pending or
        // in-flight exit reveal can advance; it never sees input.
        if !self.is_open
            && matches!(
                event,
                Event::Window(iced::window::Event::RedrawRequested(_))
            )
        {
            let content_layout = Layout::new(tree.children[1].size);
            self.content.update(
                &mut tree.children[1],
                event,
                content_layout,
                mouse::Cursor::Unavailable,
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
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.trigger.draw(
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
        self.trigger
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
        let mut children = tree.children.iter_mut();
        let trigger_tree = children.next().expect("popover trigger tree");
        let content_tree = children.next().expect("popover content tree");
        let exiting = !self.is_open && {
            let mut activity = motion::Activity::default();
            let content_layout = Layout::new(content_tree.size);
            self.content.operate(
                content_tree,
                content_layout,
                viewport,
                renderer,
                &mut activity,
            );
            activity.is_active()
        };
        if self.is_open || exiting {
            // Scrollable supplies its viewport in content coordinates; the anchor
            // and floating panel are positioned in window coordinates.
            let viewport = viewport_for_layout(*viewport + translation, window);
            let anchor_bounds = layout.bounds() + translation;
            let mut limits = layout::Limits::new(
                Size::ZERO,
                if self.options.clamp_to_viewport {
                    viewport.size()
                } else {
                    Size::INFINITE
                },
            );
            limits = limits.width(if let Some(width) = self.options.width {
                Length::Fixed(width)
            } else if self.options.match_trigger_width {
                Length::Fixed(anchor_bounds.width)
            } else {
                Length::Fit
            });
            self.content.layout(content_tree, renderer, &limits);
            let position = position_layer(
                anchor_bounds,
                content_tree.size,
                viewport,
                PositioningOptions {
                    preferred: self.options.placement,
                    alignment: self.options.alignment,
                    gap: self.options.gap,
                    clamp_to_viewport: self.options.clamp_to_viewport,
                    flip_when_overflow: self.options.flip_when_overflow,
                },
            );
            let content_layout = Layout::new(content_tree.size).move_to(position.point);
            // Include the reveal's presented translation in the clipping region.
            let panel = content_layout
                .iter(&content_tree.children)
                .next()
                .map_or(content_layout.bounds(), |(layout, _)| layout.bounds());
            let clip_size = Size::new(
                content_tree
                    .size
                    .width
                    .max(panel.x + panel.width - position.point.x),
                content_tree
                    .size
                    .height
                    .max(panel.y + panel.height - position.point.y),
            );
            vec![overlay::Element::new(Box::new(PopoverOverlay {
                content: &mut self.content,
                tree: content_tree,
                layout: content_layout,
                clip_bounds: Rectangle::new(position.point, clip_size),
                window,
                anchor_bounds,
                viewport_bounds: viewport,
                options: self.options,
                on_dismiss: self.on_dismiss.clone(),
                is_open: self.is_open,
            }))]
        } else {
            self.trigger.overlay(
                trigger_tree,
                layout,
                renderer,
                viewport,
                translation,
                window,
            )
        }
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.trigger
            .operate(&mut tree.children[0], layout, viewport, renderer, operation);
    }
}

struct PopoverOverlay<'a, 'b, Message> {
    content: &'b mut Element<'a, Message>,
    tree: &'b mut widget::Tree,
    layout: Layout,
    clip_bounds: Rectangle,
    window: Size,
    anchor_bounds: Rectangle,
    viewport_bounds: Rectangle,
    options: PopoverOptions,
    on_dismiss: Message,
    /// While exiting, the panel still draws but captures no input.
    is_open: bool,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for PopoverOverlay<'_, '_, Message>
where
    Message: Clone,
{
    fn update(
        &mut self,
        event: &Event,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
    ) {
        if self.is_open && self.options.close_on_escape && is_escape_press(event) {
            shell.publish(self.on_dismiss.clone());
            shell.capture_event();
            return;
        }

        let content_layout = self.layout;
        let overlay_bounds = content_layout
            .iter(&self.tree.children)
            .next()
            .map_or(content_layout.bounds(), |(layout, _)| layout.bounds());
        if self.is_open
            && outside_press_action(
                self.options.close_on_outside_press,
                event,
                cursor,
                overlay_bounds,
                self.anchor_bounds,
            ) == OutsidePressAction::PublishDismissal
        {
            shell.publish(self.on_dismiss.clone());
            if self.options.consume_outside_press {
                shell.capture_event();
            }
            return;
        }

        // A logically hidden panel animating out only receives redraw ticks.
        if !self.is_open
            && !matches!(
                event,
                Event::Window(iced::window::Event::RedrawRequested(_))
            )
        {
            return;
        }

        self.content.update(
            self.tree,
            event,
            content_layout,
            cursor,
            renderer,
            shell,
            &self.viewport_bounds,
        );
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        cursor: mouse::Cursor,
    ) {
        renderer.with_layer(self.clip_bounds, |renderer| {
            self.content.draw(
                self.tree,
                renderer,
                theme,
                style,
                self.layout,
                cursor,
                &self.clip_bounds,
            );
        });
    }

    fn operate(&mut self, renderer: &iced::Renderer, operation: &mut dyn widget::Operation) {
        operation.container(None, self.clip_bounds, &self.viewport_bounds);
        operation.traverse(&mut |operation| {
            self.content.operate(
                self.tree,
                self.layout,
                &self.viewport_bounds,
                renderer,
                operation,
            );
        });
    }

    fn mouse_interaction(
        &self,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if !self.is_open {
            return mouse::Interaction::None;
        }
        self.content
            .mouse_interaction(self.tree, self.layout, cursor, &self.clip_bounds, renderer)
    }

    fn overlay<'a>(
        &'a mut self,
        renderer: &iced::Renderer,
    ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        self.content.overlay(
            self.tree,
            self.layout,
            renderer,
            &self.viewport_bounds,
            Vector::ZERO,
            self.window,
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
    use iced::advanced::Widget as _;
    use iced::{Event, Point, Rectangle, Size};

    use super::{outside_press_action, primary_press_position, OutsidePressAction, PopoverOptions};

    #[test]
    fn consuming_outside_dismissal_does_not_activate_the_underlying_surface() {
        use iced::advanced::renderer::{Headless, Settings};
        use iced::widget::{container, mouse_area, space, stack, text};
        use iced::Fill;
        use iced_runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
            Settings::default(),
            Some("tiny-skia"),
        ))
        .expect("software renderer");
        let bounds = Size::new(400.0, 300.0);
        for consume in [false, true] {
            let view = stack![
                mouse_area(container(space::horizontal()).width(Fill).height(Fill))
                    .on_press("underlying"),
                super::popover(
                    container(text("Information")).width(100).height(40),
                    container(text("Media properties")).width(140).height(40),
                    true,
                    PopoverOptions {
                        consume_outside_press: consume,
                        ..PopoverOptions::default()
                    },
                    "dismiss",
                ),
            ]
            .width(Fill)
            .height(Fill);
            let mut ui = UserInterface::build(view, bounds, Cache::new(), &mut renderer);
            let mut messages = iced::advanced::shell::Bus::new();
            ui.update(
                &iced::window::Headless,
                &iced::advanced::shell::Waker::noop(),
                &[Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left,
                ))],
                mouse::Cursor::Available(Point::new(300.0, 200.0)),
                &mut renderer,
                &mut messages,
            );
            let messages: Vec<_> = messages.drain().map(|(message, _)| message).collect();
            if consume {
                assert_eq!(messages, ["dismiss"]);
            } else {
                assert_eq!(messages, ["dismiss", "underlying"]);
            }
        }
    }

    #[test]
    fn scrolled_popover_clamps_and_receives_input_in_window_coordinates() {
        use iced::advanced::{renderer, renderer::Headless, shell, widget};
        use iced::widget::{column, container, scrollable, space, text};
        use iced_runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
            renderer::Settings::default(),
            Some("tiny-skia"),
        ))
        .expect("software renderer");
        let action = crate::control_button(
            None,
            Some("Menu action".to_owned()),
            crate::variants::ButtonVariant::Text,
        )
        .id("scrolled-panel-action")
        .min_height(32.0)
        .on_press("activate");
        let view = scrollable(column![
            space().height(200),
            super::popover(
                container(text("Menu")).width(100).height(40),
                container(action).id("scrolled-panel").width(120).height(64),
                true,
                PopoverOptions {
                    consume_outside_press: true,
                    ..PopoverOptions::default()
                },
                "dismiss",
            ),
            space().height(500),
        ])
        .smooth_scroll(false)
        .id("scrolled-page")
        .width(200)
        .height(180);
        let window = Size::new(200.0, 180.0);
        let mut ui = UserInterface::build(view, window, Cache::new(), &mut renderer);
        ui.operate(
            &renderer,
            &mut widget::operation::scrollable::scroll_to::<()>(
                widget::Id::new("scrolled-page"),
                iced::widget::scrollable::AbsoluteOffset {
                    x: Some(0.0),
                    y: Some(180.0),
                },
                widget::operation::Animation::Instant,
            ),
        );
        // Instant scroll operations become the input/overlay frame on redraw.
        let mut frame_messages = shell::Bus::new();
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            &mut renderer,
            &mut frame_messages,
        );
        #[derive(Default)]
        struct Bounds {
            panel: Option<Rectangle>,
            action: Option<Rectangle>,
        }
        impl widget::Operation for Bounds {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
                visit(self);
            }
            fn container(
                &mut self,
                id: Option<&widget::Id>,
                bounds: Rectangle,
                _viewport: &Rectangle,
            ) {
                if id == Some(&widget::Id::new("scrolled-panel")) {
                    self.panel = Some(bounds);
                }
            }
            fn focusable(
                &mut self,
                id: Option<&widget::Id>,
                bounds: Rectangle,
                _state: &mut dyn widget::operation::Focusable,
            ) {
                if id == Some(&widget::Id::new("scrolled-panel-action")) {
                    self.action = Some(bounds);
                }
            }
        }
        let mut bounds = Bounds::default();
        ui.operate(&renderer, &mut bounds);
        let panel = bounds.panel.expect("open panel exposes its bounds");
        assert!(
            (panel.y - 80.0).abs() < 0.5 && panel.y + panel.height <= window.height,
            "the menu follows the trigger at window y=20, including its gap and surface padding: {panel:?}"
        );
        let cursor = mouse::Cursor::Available(bounds.action.expect("panel action").center());
        let mut messages = shell::Bus::new();
        ui.update(
            &iced::window::Headless,
            &shell::Waker::noop(),
            &[
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            cursor,
            &mut renderer,
            &mut messages,
        );
        assert_eq!(
            messages
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            ["activate"],
            "window-space input must activate the drawn menu rather than dismiss it"
        );
    }

    #[test]
    fn open_popover_hides_trigger_hint_but_preserves_content_hint() {
        use std::cell::Cell;
        use std::time::Duration;

        use iced::advanced::{renderer, renderer::Headless, widget};
        use iced::widget::{container, text};
        use iced::{Element, Theme};
        use iced_runtime::user_interface::{Cache, UserInterface};

        use super::popover;
        use crate::{
            fonts::DEFAULT_LINE_HEIGHT,
            overlay::{tooltip_element, TooltipOptions},
        };

        let trigger_painted = Cell::new(false);
        let content_painted = Cell::new(false);
        fn hint<'a>(label: &'static str, painted: &'a Cell<bool>) -> Element<'a, ()> {
            text(label)
                .style(move |_| {
                    painted.set(true);
                    text::Style::default()
                })
                ._boxed()
        }
        #[derive(Default)]
        struct ContentTarget(Option<Rectangle>);
        impl widget::Operation for ContentTarget {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
                operate(self);
            }

            fn text(
                &mut self,
                _id: Option<&widget::Id>,
                bounds: Rectangle,
                text: &mut dyn widget::operation::Text,
            ) {
                if text.text().as_ref() == "Content target" {
                    self.0 = Some(bounds);
                }
            }
        }
        let view = |open| {
            let options = TooltipOptions {
                delay: Duration::ZERO,
                ..TooltipOptions::default()
            };
            popover(
                tooltip_element(
                    container(text("Account")).width(100).height(40),
                    hint("Account identity", &trigger_painted),
                    options,
                ),
                tooltip_element(
                    container(text("Content target")).width(140).height(40),
                    hint("Content detail", &content_painted),
                    options,
                ),
                open,
                PopoverOptions::default(),
                (),
            )
        };
        let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
            renderer::Settings {
                font: iced::Font::DEFAULT,
                text_size: 14.0.into(),
                line_height: DEFAULT_LINE_HEIGHT,
                metrics_hinting: false,
            },
            Some("tiny-skia"),
        ))
        .expect("software renderer");
        let bounds = Size::new(400.0, 300.0);
        let mut ui = UserInterface::build(view(false), bounds, Cache::new(), &mut renderer);
        let mut messages = iced::advanced::shell::Bus::new();
        let trigger_cursor = mouse::Cursor::Available(Point::new(20.0, 20.0));
        ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            &[Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(20.0, 20.0),
            })],
            trigger_cursor,
            &mut renderer,
            &mut messages,
        );
        ui.draw(
            &mut renderer,
            &Theme::Light,
            &renderer::Style::default(),
            trigger_cursor,
        );
        assert!(
            trigger_painted.replace(false),
            "closed trigger reveals its hover hint"
        );

        let mut ui = UserInterface::build(view(true), bounds, ui.into_cache(), &mut renderer);
        ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            &[],
            trigger_cursor,
            &mut renderer,
            &mut messages,
        );
        ui.draw(
            &mut renderer,
            &Theme::Light,
            &renderer::Style::default(),
            trigger_cursor,
        );
        assert!(
            !trigger_painted.replace(false),
            "open popover must suppress an already-visible trigger hint"
        );

        let mut target = ContentTarget::default();
        ui.operate(&renderer, &mut target);
        let position = target
            .0
            .expect("the open panel exposes its content")
            .center();
        let content_cursor = mouse::Cursor::Available(position);
        ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            &[Event::Mouse(mouse::Event::CursorMoved { position })],
            content_cursor,
            &mut renderer,
            &mut messages,
        );
        ui.draw(
            &mut renderer,
            &Theme::Light,
            &renderer::Style::default(),
            content_cursor,
        );
        assert!(
            content_painted.get(),
            "nested content hints remain readable"
        );
        assert!(!trigger_painted.get());

        let mut ui = UserInterface::build(view(false), bounds, ui.into_cache(), &mut renderer);
        ui.update(
            &iced::window::Headless,
            &iced::advanced::shell::Waker::noop(),
            &[Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(20.0, 20.0),
            })],
            trigger_cursor,
            &mut renderer,
            &mut messages,
        );
        ui.draw(
            &mut renderer,
            &Theme::Light,
            &renderer::Style::default(),
            trigger_cursor,
        );
        assert!(
            trigger_painted.get(),
            "closing the popover restores ordinary hover hints"
        );
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

    #[test]
    fn outside_touch_press_publishes_dismissal_without_capture() {
        let event = Event::Touch(iced::touch::Event::FingerPressed {
            id: iced::touch::Finger(2),
            position: Point::new(20.0, 20.0),
        });

        assert_eq!(
            outside_press_action(
                true,
                &event,
                mouse::Cursor::Unavailable,
                Rectangle::new(Point::new(100.0, 100.0), Size::new(80.0, 60.0)),
                Rectangle::new(Point::new(100.0, 60.0), Size::new(40.0, 24.0)),
            ),
            OutsidePressAction::PublishDismissal
        );
    }

    #[test]
    fn entering_panel_fits_overlay_clip_and_dismissal_tracks_its_position() {
        use crate::{tokens::TOKENS, widgets::motion};
        use iced::advanced::{renderer, renderer::Headless, shell, widget, Layout, Shell};
        use iced::widget::{container, opaque, Space};

        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            renderer::Settings {
                font: iced::Font::DEFAULT,
                text_size: 14.0.into(),
                line_height: crate::fonts::DEFAULT_LINE_HEIGHT,
                metrics_hinting: false,
            },
            Some("tiny-skia"),
        ))
        .expect("software renderer");
        let panel = |visible| {
            motion::reveal(
                opaque(container(Space::new().width(100).height(40)))._boxed(),
                visible,
                true,
                TOKENS.durations.ms200,
            )
        };
        let view = |visible| {
            super::Popover {
                trigger: container(Space::new().width(100).height(40))._boxed(),
                content: panel(visible),
                is_open: visible,
                options: PopoverOptions::default(),
                on_dismiss: 1u8,
            }
            ._boxed()
        };
        let mut closed = view(false);
        let mut tree = widget::Tree::new(&closed);
        tree.diff(&mut closed);
        let mut content = view(true);
        tree.diff(&mut content);
        let anchor = Rectangle::new(Point::new(20.0, 20.0), Size::new(100.0, 40.0));
        let options = PopoverOptions::default();
        let natural_top = anchor.y + anchor.height + options.gap;
        let window = Size::new(400.0, 300.0);
        let viewport = Rectangle::with_size(window);
        content.layout(
            &mut tree,
            &renderer,
            &iced::advanced::layout::Limits::new(Size::ZERO, window),
        );
        let mut overlay = content
            .overlay(
                &mut tree,
                Layout::new(anchor.size()).move_to(anchor.position()),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
                window,
            )
            .pop()
            .expect("open panel overlay");
        #[derive(Default)]
        struct Clip(Option<Rectangle>);
        impl widget::Operation for Clip {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
                visit(self);
            }
            fn container(
                &mut self,
                _id: Option<&widget::Id>,
                bounds: Rectangle,
                _viewport: &Rectangle,
            ) {
                self.0.get_or_insert(bounds);
            }
        }
        let mut clip = Clip::default();
        overlay.as_overlay_mut().operate(&renderer, &mut clip);
        let displayed_bottom = Point::new(30.0, natural_top + 45.0);
        assert!(
            clip.0
                .expect("overlay clip bounds")
                .contains(displayed_bottom),
            "native overlay clipping must contain the translated panel"
        );

        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let mut messages = shell::Bus::new();
        overlay.as_overlay_mut().update(
            &press,
            mouse::Cursor::Available(displayed_bottom),
            &renderer,
            &mut Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut messages),
        );
        assert!(
            messages.is_empty(),
            "the translated bottom strip belongs to the panel"
        );
        overlay.as_overlay_mut().update(
            &press,
            mouse::Cursor::Available(Point::new(30.0, natural_top + 2.0)),
            &renderer,
            &mut Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut messages),
        );
        assert_eq!(
            messages
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            vec![1],
            "the vacated top strip is outside the panel"
        );
    }
}
