//! Navigation drawn over its scroll content, with a live backdrop on wgpu.
//!
//! Keep this wrapper inside the page's `Scrollable`: the scrollable paints its
//! native scrollbar afterwards, outside the backdrop's scene capture.

use std::cell::Cell;

use iced::advanced::{
    layout, mouse, overlay, renderer, renderer::Headless, widget, Layout, Renderer as _, Shell,
    Widget,
};
use iced::{
    gradient, Background, Blur, Degrees, Element, Event, Length, Point, Rectangle, Size, Theme,
    Vector,
};

use crate::tokens::{palette, TOKENS};

/// Pins navigation to the scrolling viewport's top, bounded by its page.
///
/// `reserve_space` puts the body after the navigation's natural height. Use
/// `false` for a detail hero that intentionally starts behind its Back button.
/// The navigation's child background should be transparent: this wrapper owns
/// the shared legibility surface and keeps all navigation contents sharp.
pub fn header<'a, Message: 'static>(
    content: impl Widget<Message, Theme, iced::Renderer> + 'a,
    navigation: impl Widget<Message, Theme, iced::Renderer> + 'a,
    reserve_space: bool,
) -> Element<'a, Message> {
    (Header {
        content: content._boxed(),
        navigation: navigation._boxed(),
        reserve_space,
    })
    ._boxed()
}

struct Header<'a, Message> {
    content: Element<'a, Message>,
    navigation: Element<'a, Message>,
    reserve_space: bool,
}

#[derive(Default)]
struct State {
    window_viewport: Cell<Option<Rectangle>>,
}

fn navigation_layout(tree: &widget::Tree, bounds: Rectangle, viewport: Rectangle) -> Layout {
    let height = tree.children[1].size.height;
    let maximum = (bounds.height - height).max(0.0);
    Layout::new(tree.children[1].size).move_to(Point::new(
        bounds.x,
        bounds.y + (viewport.y - bounds.y).clamp(0.0, maximum),
    ))
}

fn body_cursor(cursor: mouse::Cursor, navigation: Rectangle, viewport: Rectangle) -> mouse::Cursor {
    if navigation
        .intersection(&viewport)
        .is_some_and(|visible| cursor.is_over(visible))
    {
        cursor.levitate()
    } else {
        cursor
    }
}

impl<Message> iced::advanced::widget::Meta for Header<'_, Message> {}

impl<Message> Widget<Message, Theme, iced::Renderer> for Header<'_, Message> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut widget::Tree) {
        tree.diff_children(&mut [&mut self.content, &mut self.navigation]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) {
        self.navigation.layout(
            &mut tree.children[1],
            renderer,
            &limits.loose().height(Length::Shrink),
        );
        tree.children[1].translation = Vector::ZERO;
        let navigation = tree.children[1].size;
        let reserved = if self.reserve_space {
            navigation.height
        } else {
            0.0
        };
        self.content.layout(
            &mut tree.children[0],
            renderer,
            &limits.shrink(Size::new(0.0, reserved)),
        );
        tree.children[0].translation = Vector::new(0.0, reserved);
        let content = tree.children[0].size;
        tree.size = limits.resolve(
            Length::Fill,
            Length::Shrink,
            Size::new(
                content.width.max(navigation.width),
                (reserved + content.height).max(navigation.height),
            ),
        );
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let navigation = navigation_layout(tree, layout.bounds(), *viewport);
        let content_layout = super::child_layout(layout, tree, 0).expect("laid out content");
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            self.navigation.operate(
                &mut tree.children[1],
                navigation,
                viewport,
                renderer,
                operation,
            );
            self.content.operate(
                &mut tree.children[0],
                content_layout,
                viewport,
                renderer,
                operation,
            );
        });
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
        let navigation = navigation_layout(tree, layout.bounds(), *viewport);
        let content_layout = super::child_layout(layout, tree, 0).expect("laid out content");
        let state = tree.state.downcast_ref::<State>();
        // Touch events carry window-space positions even if no mouse cursor is
        // available. Preserve levitation: a scrollbar/overlay already owns it.
        let cursor = if cursor == mouse::Cursor::Unavailable {
            match event {
                Event::Touch(
                    iced::touch::Event::FingerPressed { position, .. }
                    | iced::touch::Event::FingerMoved { position, .. }
                    | iced::touch::Event::FingerLifted { position, .. }
                    | iced::touch::Event::FingerLost { position, .. },
                ) => {
                    let window = state.window_viewport.get().unwrap_or(*viewport);
                    mouse::Cursor::Available(
                        *position + Vector::new(viewport.x - window.x, viewport.y - window.y),
                    )
                }
                _ => cursor,
            }
        } else {
            cursor
        };
        let over_navigation = navigation
            .bounds()
            .intersection(viewport)
            .is_some_and(|visible| cursor.is_over(visible));
        self.navigation.update(
            &mut tree.children[1],
            event,
            navigation,
            cursor,
            renderer,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            // A body control may have been pressed before dragging into navigation.
            // It still needs the release/cancel, but must not activate under chrome.
            if matches!(
                event,
                Event::Mouse(mouse::Event::ButtonReleased(_))
                    | Event::Touch(
                        iced::touch::Event::FingerLifted { .. }
                            | iced::touch::Event::FingerLost { .. }
                    )
            ) {
                self.content.update(
                    &mut tree.children[0],
                    event,
                    content_layout,
                    mouse::Cursor::Unavailable,
                    renderer,
                    shell,
                    viewport,
                );
            }
            return;
        }
        // Even empty navigation chrome must not activate an obscured body control.
        // Leave wheel events uncaptured so the enclosing scrollable keeps scrolling.
        if over_navigation
            && matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(_))
                    | Event::Touch(iced::touch::Event::FingerPressed { .. })
            )
        {
            shell.capture_event();
            return;
        }
        self.content.update(
            &mut tree.children[0],
            event,
            content_layout,
            body_cursor(cursor, navigation.bounds(), *viewport),
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
        let navigation = navigation_layout(tree, layout.bounds(), *viewport);
        self.content.draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            super::child_layout(layout, tree, 0).expect("laid out child"),
            body_cursor(cursor, navigation.bounds(), *viewport),
            viewport,
        );
        let Some(page_clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        let navigation_bounds = navigation.bounds();
        let surface = palette(theme).colors.background;
        let live_blur = renderer.name() == "wgpu";
        // The falloff belongs to navigation, not the readable page below it.
        // Keep capture/tint clipped to the same bounds that own navigation input.
        let bounds = navigation_bounds;
        let Some(visible) = bounds.intersection(&page_clip) else {
            return;
        };
        renderer.with_layer(visible, |renderer| {
            let tint = if live_blur {
                let plateau = (1.0 - TOKENS.navigation_blur.fade_height / bounds.height).max(0.0);
                renderer.draw_backdrop(renderer::Backdrop {
                    bounds,
                    blur: Blur::vertical_gradient(TOKENS.navigation_blur.sigma, 0.0)
                        .range(plateau, 1.0),
                    border_radius: 0.0.into(),
                    border_smoothing: 0.0,
                    optics: None,
                    quality: Default::default(),
                });
                Background::Gradient(
                    gradient::Linear::new(Degrees(180.0))
                        .add_stop(0.0, surface.scale_alpha(TOKENS.navigation_blur.tint_alpha))
                        .add_stop(
                            plateau,
                            surface.scale_alpha(TOKENS.navigation_blur.tint_alpha),
                        )
                        .add_stop(1.0, surface.scale_alpha(0.0))
                        .into(),
                )
            } else {
                Background::Color(surface)
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..renderer::Quad::default()
                },
                tint,
            );
            self.navigation.draw(
                &tree.children[1],
                renderer,
                theme,
                style,
                navigation,
                cursor,
                &page_clip,
            );
        });
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let navigation = navigation_layout(tree, layout.bounds(), *viewport);
        if navigation
            .bounds()
            .intersection(viewport)
            .is_some_and(|visible| cursor.is_over(visible))
        {
            return self.navigation.mouse_interaction(
                &tree.children[1],
                navigation,
                cursor,
                viewport,
                renderer,
            );
        }
        self.content.mouse_interaction(
            &tree.children[0],
            super::child_layout(layout, tree, 0).expect("laid out child"),
            cursor,
            viewport,
            renderer,
        )
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
        let state = tree.state.downcast_mut::<State>();
        // All widget traversals receive content-space viewports. Overlay translation
        // maps that viewport back to window coordinates for raw touch positions.
        let content_viewport = *viewport;
        state.window_viewport.set(Some(*viewport + translation));
        let navigation = navigation_layout(tree, layout.bounds(), content_viewport);
        let content_layout = super::child_layout(layout, tree, 0).expect("laid out content");
        let (content, header) = tree.children.split_at_mut(1);
        let mut overlays = self.content.overlay(
            &mut content[0],
            content_layout,
            renderer,
            viewport,
            translation,
            window,
        );
        overlays.extend(self.navigation.overlay(
            &mut header[0],
            navigation,
            renderer,
            viewport,
            translation,
            window,
        ));
        overlays
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::shell::{Bus, Waker};
    use iced::advanced::widget::operation::{focusable, scrollable as scroll_operation};
    use iced::widget::{column, container, row, scrollable, space};
    use iced::{Color, Font};
    use iced_runtime::user_interface::{Cache, UserInterface};

    use super::*;
    use crate::control_button;
    use crate::overlay::{popover, PopoverOptions};
    use crate::theme::{theme, ThemeMode};
    use crate::variants::ButtonVariant;

    #[derive(Clone, Debug, PartialEq)]
    enum Message {
        Navigation,
        Body,
        Panel,
        Dismiss,
    }

    /// A deliberately sharp scene and overlay fixture; rendering goes through
    /// the real Widget/Scrollable/Popover paths, not a renderer mock.
    struct Stripes {
        size: Size,
        colors: [Color; 2],
    }

    impl iced::advanced::widget::Meta for Stripes {}

    impl Widget<Message, Theme, iced::Renderer> for Stripes {
        fn size(&self) -> Size<Length> {
            Size::new(self.size.width.into(), self.size.height.into())
        }

        fn layout(
            &mut self,
            tree: &mut widget::Tree,
            _renderer: &iced::Renderer,
            limits: &layout::Limits,
        ) {
            tree.size = layout::atomic(limits, self.size.width, self.size.height);
        }

        fn draw(
            &self,
            _tree: &widget::Tree,
            renderer: &mut iced::Renderer,
            _theme: &Theme,
            _style: &renderer::Style,
            layout: Layout,
            _cursor: mouse::Cursor,
            viewport: &Rectangle,
        ) {
            let bounds = layout.bounds();
            let Some(visible) = bounds.intersection(viewport) else {
                return;
            };
            renderer.with_layer(visible, |renderer| {
                for stripe in 0..(bounds.width / 8.0).ceil() as usize {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: bounds.x + stripe as f32 * 8.0,
                                width: 8.0,
                                ..bounds
                            },
                            ..renderer::Quad::default()
                        },
                        self.colors[stripe % 2],
                    );
                }
            });
        }
    }

    fn striped_page(with_header: bool) -> Element<'static, Message> {
        let body: Element<'_, Message> = (Stripes {
            size: Size::new(320.0, 400.0),
            colors: [Color::BLACK, Color::WHITE],
        })
        ._boxed();
        let body = if with_header {
            let navigation = popover(
                container(space::horizontal()).width(32).height(32),
                (Stripes {
                    size: Size::new(64.0, 32.0),
                    colors: [
                        Color::from_rgb(1.0, 0.0, 1.0),
                        Color::from_rgb(0.0, 1.0, 0.0),
                    ],
                })
                ._boxed(),
                true,
                PopoverOptions {
                    width: Some(96.0),
                    ..PopoverOptions::default()
                },
                Message::Dismiss,
            );
            header(
                body,
                container(navigation).width(Length::Fill).height(56),
                false,
            )
        } else {
            body
        };
        scrollable(body)
            .id("page")
            .height(Length::Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(12)
                    .scroller_width(12)
                    .margin(0),
            ))
            .style(|theme, status| {
                let mut style = scrollable::default(theme, status);
                style.vertical_rail.background = Some(Color::from_rgb(0.0, 1.0, 1.0).into());
                style.vertical_rail.border = Default::default();
                style.vertical_rail.scroller.background = Color::from_rgb(0.0, 1.0, 1.0).into();
                style.vertical_rail.scroller.border = Default::default();
                style
            })
            ._boxed()
    }

    #[test]
    fn wgpu_blurs_live_scroll_content_but_leaves_native_scrollbar_and_popover_sharp() {
        let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
            renderer::Settings::default(),
            Some("wgpu"),
        ))
        .expect("wgpu headless renderer required for numeric backdrop coverage");
        assert_eq!(renderer.name(), "wgpu");
        let current_theme = theme(ThemeMode::Dark);
        let size = Size::new(320.0, 180.0);
        let pixel = |pixels: &[u8], x: usize, y: usize| -> [u8; 4] {
            pixels[(y * 320 + x) * 4..(y * 320 + x + 1) * 4]
                .try_into()
                .expect("RGBA pixel")
        };
        let mut baseline =
            UserInterface::build(striped_page(false), size, Cache::new(), &mut renderer);
        scroll(&mut baseline, &mut renderer);
        baseline.draw(
            &mut renderer,
            &current_theme,
            &renderer::Style::default(),
            mouse::Cursor::Unavailable,
        );
        let sharp = renderer.screenshot(Size::new(320, 180), 1.0, Color::BLACK);
        let mut ui = UserInterface::build(striped_page(true), size, Cache::new(), &mut renderer);
        scroll(&mut ui, &mut renderer);
        ui.draw(
            &mut renderer,
            &current_theme,
            &renderer::Style::default(),
            mouse::Cursor::Unavailable,
        );
        let blurred = renderer.screenshot(Size::new(320, 180), 1.0, Color::BLACK);
        assert!(
            renderer.blur_statistics().scene_misses > 0,
            "real scene blur must execute"
        );

        // Blur lives within the 56px navigation. Its contrast loss exceeds tint
        // alone; every pixel below navigation must retain the sharp page scene.
        let contrast =
            |pixels: &[u8], y| pixel(pixels, 200, y)[0].abs_diff(pixel(pixels, 208, y)[0]);
        assert!(contrast(&sharp, 28) > 240);
        assert!(
            contrast(&blurred, 28) < 60,
            "live stripes must be blurred, rather than only tinted"
        );
        for y in [56, 57, 72, 100, 164] {
            assert_eq!(pixel(&blurred, 200, y), pixel(&sharp, 200, y));
            assert_eq!(pixel(&blurred, 208, y), pixel(&sharp, 208, y));
        }

        // This rail crosses the header and the clear page. Its exact cyan
        // pixels must match the unfiltered native Scrollable baseline.
        for y in [20, 100, 164] {
            assert_eq!(pixel(&sharp, 314, y), [0, 255, 255, 255]);
            assert_eq!(pixel(&blurred, 314, y), pixel(&sharp, 314, y));
        }

        // Only the independent popover has magenta/green stripes. Assert intact
        // one-pixel edges in the actual overlay render, without inspecting images.
        let magenta = [255, 0, 255, 255];
        let green = [0, 255, 0, 255];
        let mut intact_edges = 0;
        for y in 0..180 {
            for x in 1..100 {
                let previous = pixel(&blurred, x - 1, y);
                let current = pixel(&blurred, x, y);
                if (previous == magenta && current == green)
                    || (previous == green && current == magenta)
                {
                    intact_edges += 1;
                }
            }
        }
        assert!(
            intact_edges >= 100,
            "popover stripes must retain sharp, fully saturated edges: {intact_edges}"
        );
    }

    fn software_renderer() -> iced::Renderer {
        iced::futures::executor::block_on(iced::Renderer::new(
            renderer::Settings {
                font: Font::DEFAULT,
                text_size: 14.0.into(),
                line_height: crate::fonts::DEFAULT_LINE_HEIGHT,
                metrics_hinting: false,
            },
            Some("tiny-skia"),
        ))
        .expect("software renderer")
    }

    fn page(open: bool) -> Element<'static, Message> {
        let trigger = control_button(None, Some("Navigate".to_owned()), ButtonVariant::Text)
            .id("navigation")
            .on_press(Message::Navigation);
        let navigation: Element<'_, Message> = if open {
            popover(
                trigger,
                control_button(None, Some("Action".to_owned()), ButtonVariant::Text)
                    .id("panel")
                    .on_press(Message::Panel),
                true,
                PopoverOptions {
                    width: Some(160.0),
                    ..PopoverOptions::default()
                },
                Message::Dismiss,
            )
        } else {
            trigger._boxed()
        };
        let content = column![
            space::vertical().height(100),
            control_button(None, Some("Body".to_owned()), ButtonVariant::Text)
                .id("body")
                .width(Length::Fill)
                .on_press(Message::Body),
            space::vertical().height(400),
        ];
        scrollable(header(
            content,
            container(row![navigation]).width(Length::Fill).height(56),
            true,
        ))
        .id("page")
        .height(Length::Fill)
        ._boxed()
    }

    fn scroll(
        ui: &mut UserInterface<'_, Message, Theme, iced::Renderer>,
        renderer: &mut iced::Renderer,
    ) {
        ui.operate(
            renderer,
            &mut scroll_operation::scroll_to::<()>(
                widget::Id::new("page"),
                scroll_operation::AbsoluteOffset {
                    x: None,
                    y: Some(120.0),
                },
                widget::operation::Animation::Instant,
            ),
        );
        frame(ui, renderer);
    }

    fn frame(
        ui: &mut UserInterface<'_, Message, Theme, iced::Renderer>,
        renderer: &mut iced::Renderer,
    ) {
        ui.update(
            &iced::window::Headless,
            &Waker::noop(),
            &[Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            renderer,
            &mut Bus::new(),
        );
    }

    #[derive(Default)]
    struct FocusBounds {
        navigation: Option<Rectangle>,
        body: Option<Rectangle>,
        panel: Option<Rectangle>,
        offset: f32,
    }
    impl widget::Operation for FocusBounds {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
            visit(self);
        }
        fn scrollable(
            &mut self,
            _: Option<&widget::Id>,
            _: Rectangle,
            _: Size,
            translation: Vector,
            _: &mut dyn scroll_operation::Scrollable,
        ) {
            self.offset = translation.y;
        }
        fn focusable(
            &mut self,
            id: Option<&widget::Id>,
            bounds: Rectangle,
            _: &mut dyn focusable::Focusable,
        ) {
            if id == Some(&widget::Id::new("navigation")) {
                self.navigation = Some(bounds);
            }
            if id == Some(&widget::Id::new("body")) {
                self.body = Some(bounds);
            }
            if id == Some(&widget::Id::new("panel")) {
                self.panel = Some(bounds);
            }
        }
    }

    #[test]
    fn scrolled_navigation_hit_and_focus_bounds_match_the_visible_header() {
        let mut renderer = software_renderer();
        let mut ui = UserInterface::build(
            column![page(false)]
                .width(Length::Fill)
                .height(Length::Fill),
            Size::new(320.0, 180.0),
            Cache::new(),
            &mut renderer,
        );
        scroll(&mut ui, &mut renderer);
        let mut bounds = FocusBounds::default();
        ui.operate(&renderer, &mut bounds);
        let navigation = bounds.navigation.expect("navigation focus target");
        assert_eq!(bounds.offset, 120.0);
        assert_eq!(navigation.y - bounds.offset, 0.0);
        assert_eq!(bounds.body.expect("body focus target").y, 156.0);
        ui.draw(
            &mut renderer,
            &theme(ThemeMode::Dark),
            &renderer::Style::default(),
            mouse::Cursor::Unavailable,
        );
        ui.operate(
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("navigation")),
        );
        let mut messages = Bus::new();
        let cursor =
            mouse::Cursor::Available(Point::new(navigation.x + 8.0, navigation.height / 2.0));
        ui.update(
            &iced::window::Headless,
            &Waker::noop(),
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
            vec![Message::Navigation]
        );

        // At this offset the Body button is physically behind the blank part of
        // the sticky surface. A primary press there must not leak through.
        let (_, statuses) = ui.update(
            &iced::window::Headless,
            &Waker::noop(),
            &[
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(Point::new(230.0, 42.0)),
            &mut renderer,
            &mut messages,
        );
        assert_eq!(statuses[0], iced::event::Status::Captured);
        assert!(messages.is_empty());

        // Wheel scrolling continues even when the pointer is over navigation.
        ui.update(
            &iced::window::Headless,
            &Waker::noop(),
            &[Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -24.0 },
            })],
            cursor,
            &mut renderer,
            &mut messages,
        );
        ui.update(
            &iced::window::Headless,
            &Waker::noop(),
            &[Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now() + std::time::Duration::from_millis(300),
            ))],
            cursor,
            &mut renderer,
            &mut messages,
        );
        let mut after = FocusBounds::default();
        ui.operate(&renderer, &mut after);
        assert!(after.offset > bounds.offset);
    }

    #[test]
    fn header_popover_anchor_remains_in_window_coordinates_after_scroll() {
        let mut renderer = software_renderer();
        let mut ui = UserInterface::build(
            page(true),
            Size::new(320.0, 180.0),
            Cache::new(),
            &mut renderer,
        );
        scroll(&mut ui, &mut renderer);
        let mut bounds = FocusBounds::default();
        ui.operate(&renderer, &mut bounds);
        let panel = bounds
            .panel
            .expect("independent header popover focus target");
        assert!(
            panel.y > 20.0 && panel.y < 100.0,
            "popover must remain below its pinned trigger: {panel:?}"
        );
        let mut messages = Bus::new();
        ui.update(
            &iced::window::Headless,
            &Waker::noop(),
            &[
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(Point::new(panel.x + 8.0, panel.y + panel.height / 2.0)),
            &mut renderer,
            &mut messages,
        );
        assert_eq!(
            messages
                .drain()
                .map(|(message, _)| message)
                .collect::<Vec<_>>(),
            vec![Message::Panel]
        );
    }

    #[test]
    fn touch_without_mouse_cursor_is_blocked_and_body_drag_release_is_cancelled() {
        let renderer = software_renderer();
        let mut widget = Header {
            content: column![
                space::vertical().height(100),
                control_button(None, Some("Body".to_owned()), ButtonVariant::Text)
                    .width(Length::Fill)
                    .on_press(Message::Body),
                space::vertical().height(400)
            ]
            ._boxed(),
            navigation: container(
                control_button(None, Some("Navigate".to_owned()), ButtonVariant::Text)
                    .width(Length::Fixed(100.0))
                    .on_press(Message::Navigation),
            )
            .width(320)
            .height(56)
            ._boxed(),
            reserve_space: false,
        };
        let mut tree = widget::Tree::new(&widget);
        widget.diff(&mut tree);
        widget.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(320.0, f32::INFINITY)),
        );
        let node = Layout::new(tree.size);
        let window = Rectangle::with_size(Size::new(320.0, 180.0));
        let mut bus = Bus::new();
        let touch = |position: Point, phase| {
            let id = iced::touch::Finger(7);
            Event::Touch(match phase {
                0 => iced::touch::Event::FingerPressed { id, position },
                1 => iced::touch::Event::FingerMoved { id, position },
                _ => iced::touch::Event::FingerLifted { id, position },
            })
        };
        {
            let mut send = |offset: f32, event: Event| {
                let viewport = window + Vector::new(0.0, offset);
                drop(widget.overlay(
                    &mut tree,
                    node,
                    &renderer,
                    &viewport,
                    Vector::new(0.0, -offset),
                    window.size(),
                ));
                let mut shell = Shell::new(&iced::window::Headless, Waker::noop(), &mut bus);
                widget.update(
                    &mut tree,
                    &event,
                    node,
                    mouse::Cursor::Unavailable,
                    &renderer,
                    &mut shell,
                    &viewport,
                );
                shell.event_status()
            };
            // The body starts behind the pinned navigation after scrolling to 100.
            assert_eq!(
                send(100.0, touch(Point::new(230.0, 20.0), 0)),
                iced::event::Status::Captured
            );
            send(100.0, touch(Point::new(230.0, 20.0), 2));
            // Start a real body press below navigation, then scroll it beneath chrome
            // before releasing. A later release over the body must not resurrect it.
            send(40.0, touch(Point::new(230.0, 80.0), 0));
            send(100.0, touch(Point::new(230.0, 20.0), 1));
            send(100.0, touch(Point::new(230.0, 20.0), 2));
            send(40.0, touch(Point::new(230.0, 80.0), 2));
        }
        assert!(
            bus.is_empty(),
            "touch on obscured body controls must not activate them"
        );

        // Touch remains usable for the visible navigation, with no mouse cursor.
        let viewport = window + Vector::new(0.0, 100.0);
        drop(widget.overlay(
            &mut tree,
            node,
            &renderer,
            &viewport,
            Vector::new(0.0, -100.0),
            window.size(),
        ));
        for phase in [0, 2] {
            let mut shell = Shell::new(&iced::window::Headless, Waker::noop(), &mut bus);
            widget.update(
                &mut tree,
                &touch(Point::new(10.0, 16.0), phase),
                node,
                mouse::Cursor::Unavailable,
                &renderer,
                &mut shell,
                &viewport,
            );
        }
        assert_eq!(
            bus.drain().map(|(message, _)| message).collect::<Vec<_>>(),
            vec![Message::Navigation]
        );
    }

    #[test]
    fn real_scrollable_touch_does_not_activate_obscured_or_cancelled_controls() {
        let mut renderer = software_renderer();
        let mut ui = UserInterface::build(
            page(false),
            Size::new(320.0, 180.0),
            Cache::new(),
            &mut renderer,
        );
        let mut bus = Bus::new();
        let id = iced::touch::Finger(7);
        let touch = |position: Point, phase| {
            Event::Touch(match phase {
                0 => iced::touch::Event::FingerPressed { id, position },
                1 => iced::touch::Event::FingerMoved { id, position },
                2 => iced::touch::Event::FingerLifted { id, position },
                _ => iced::touch::Event::FingerLost { id, position },
            })
        };
        // Native winit supplies the current touch position as the cursor. Exercise
        // the full Scrollable transform, including its own release/cancel state.
        let mut send = |ui: &mut UserInterface<'_, Message, Theme, iced::Renderer>,
                        renderer: &mut iced::Renderer,
                        position,
                        phase| {
            ui.update(
                &iced::window::Headless,
                &Waker::noop(),
                &[touch(position, phase)],
                mouse::Cursor::Available(position),
                renderer,
                &mut bus,
            );
        };
        scroll(&mut ui, &mut renderer);
        send(&mut ui, &mut renderer, Point::new(230.0, 42.0), 0);
        send(&mut ui, &mut renderer, Point::new(230.0, 42.0), 2);
        ui.operate(
            &renderer,
            &mut scroll_operation::scroll_to::<()>(
                widget::Id::new("page"),
                scroll_operation::AbsoluteOffset {
                    x: None,
                    y: Some(80.0),
                },
                widget::operation::Animation::Instant,
            ),
        );
        frame(&mut ui, &mut renderer);
        send(&mut ui, &mut renderer, Point::new(230.0, 90.0), 0);
        send(&mut ui, &mut renderer, Point::new(230.0, 20.0), 1);
        send(&mut ui, &mut renderer, Point::new(230.0, 20.0), 3);
        send(&mut ui, &mut renderer, Point::new(230.0, 90.0), 2);
        send(&mut ui, &mut renderer, Point::new(10.0, 16.0), 0);
        send(&mut ui, &mut renderer, Point::new(10.0, 16.0), 3);
        send(&mut ui, &mut renderer, Point::new(10.0, 16.0), 2);
        send(&mut ui, &mut renderer, Point::new(10.0, 16.0), 0);
        send(&mut ui, &mut renderer, Point::new(10.0, 16.0), 2);
        assert_eq!(
            bus.drain().map(|(message, _)| message).collect::<Vec<_>>(),
            vec![Message::Navigation],
            "only the final uncancelled navigation tap may activate"
        );

        // A touch-only cursor follows the same real Scrollable overlay translation.
        for (position, phase) in [
            (Point::new(230.0, 42.0), 0),
            (Point::new(230.0, 42.0), 2),
            (Point::new(10.0, 16.0), 0),
            (Point::new(10.0, 16.0), 2),
        ] {
            ui.update(
                &iced::window::Headless,
                &Waker::noop(),
                &[touch(position, phase)],
                mouse::Cursor::Unavailable,
                &mut renderer,
                &mut bus,
            );
        }
        assert_eq!(
            bus.drain().map(|(message, _)| message).collect::<Vec<_>>(),
            vec![Message::Navigation],
            "window touch positions must hit the visible navigation and block obscured body"
        );
    }

    #[test]
    fn fallback_is_opaque_and_offscreen_navigation_adds_no_pixels_or_idle_redraw() {
        let mut renderer = software_renderer();
        let mut widget = Header::<Message> {
            content: container(space::horizontal())
                .width(320)
                .height(400)
                .style(|_| iced::widget::container::Style {
                    background: Some(Color::from_rgb(0.8, 0.0, 0.0).into()),
                    ..Default::default()
                })
                ._boxed(),
            navigation: container(space::horizontal())
                .width(320)
                .height(56)
                ._boxed(),
            reserve_space: false,
        };
        let mut tree = widget::Tree::new(&widget);
        widget.diff(&mut tree);
        widget.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(320.0, 400.0)),
        );
        let node = Layout::new(tree.size);
        let viewport = Rectangle::new(Point::new(0.0, 120.0), Size::new(320.0, 180.0));
        let current_theme = theme(ThemeMode::Dark);
        renderer.reset(Rectangle::with_size(Size::new(320.0, 180.0)));
        renderer.with_translation(Vector::new(0.0, -120.0), |renderer| {
            widget.draw(
                &tree,
                renderer,
                &current_theme,
                &renderer::Style::default(),
                node,
                mouse::Cursor::Unavailable,
                &viewport,
            );
        });
        let pixels = renderer.screenshot(Size::new(320, 180), 1.0, Color::BLACK);
        let at = |x: usize, y: usize| &pixels[(y * 320 + x) * 4..(y * 320 + x + 1) * 4];
        let background = palette(&current_theme).colors.background;
        let expected = [
            (background.r * 255.0).round() as u8,
            (background.g * 255.0).round() as u8,
            (background.b * 255.0).round() as u8,
            255,
        ];
        assert_eq!(
            at(160, 20),
            expected,
            "software header uses the semantic opaque surface"
        );
        assert!(
            at(160, 80)[0] > 150,
            "software fallback does not tint or blur the body below navigation"
        );

        let outside = Rectangle::new(Point::new(0.0, 500.0), Size::new(320.0, 180.0));
        renderer.reset(Rectangle::with_size(Size::new(320.0, 180.0)));
        renderer.with_translation(Vector::new(0.0, -500.0), |renderer| {
            widget.draw(
                &tree,
                renderer,
                &current_theme,
                &renderer::Style::default(),
                node,
                mouse::Cursor::Unavailable,
                &outside,
            );
        });
        assert!(renderer
            .screenshot(Size::new(320, 180), 1.0, Color::BLACK)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 0, 255]));
        let mut bus = Bus::new();
        let mut shell = Shell::new(&iced::window::Headless, Waker::noop(), &mut bus);
        widget.update(
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            )),
            node,
            mouse::Cursor::Unavailable,
            &renderer,
            &mut shell,
            &outside,
        );
        assert_eq!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
        assert!(shell.is_empty());
    }
}
