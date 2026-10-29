//! Opt-in text hints for controls whose full value must also be readable by keyboard.

use iced::advanced::Renderer as _;
use iced::advanced::{layout, mouse, overlay, renderer, widget, Layout, Shell, Widget};
use iced::time::Instant;
use iced::widget::{container, text};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use super::{position_layer, tooltip_element, PositioningOptions, TooltipOptions};
use crate::tokens::TOKENS;

/// Adds a full-value hint on keyboard focus while retaining the ordinary delayed hover tooltip.
pub fn focus_tooltip<'a, Message: 'a>(
    trigger: impl Widget<Message, Theme, iced::Renderer> + 'a,
    content: impl Into<String>,
    options: TooltipOptions,
) -> Element<'a, Message> {
    if !options.enabled {
        return trigger._boxed();
    }
    let content = content.into();
    FocusTooltip {
        trigger: tooltip_element(
            trigger._boxed(),
            text(content.clone())
                .size(TOKENS.font_sizes.s12)
                .wrapping(text::Wrapping::WordOrGlyph)
                ._boxed(),
            options,
        ),
        hint: container(
            text(content)
                .size(TOKENS.font_sizes.s12)
                .wrapping(text::Wrapping::WordOrGlyph),
        )
        .width(Length::Fit.max(options.max_width))
        .padding(TOKENS.spacing.s2)
        .style(super::style::tooltip_surface)
        ._boxed(),
        options,
        rich: false,
    }
    ._boxed()
}

/// Shows caller-styled, wrapping content above the trigger on hover or keyboard
/// focus. Rich hints start-align with an 8px gap and share one overlay in both
/// input modes. Disabling disclosure preserves the trigger's widget identity.
pub fn focus_tooltip_content<'a, Message: 'a>(
    trigger: impl Widget<Message, Theme, iced::Renderer> + 'a,
    hint: impl Widget<Message, Theme, iced::Renderer> + 'a,
    options: TooltipOptions,
) -> Element<'a, Message> {
    FocusTooltip {
        trigger: trigger._boxed(),
        hint: hint._boxed(),
        options,
        rich: true,
    }
    ._boxed()
}

struct FocusTooltip<'a, Message> {
    trigger: Element<'a, Message>,
    hint: Element<'a, Message>,
    options: TooltipOptions,
    rich: bool,
}

#[derive(Default)]
struct HoverState {
    since: Option<Instant>,
}

#[derive(Default)]
struct FocusProbe(bool);

impl widget::Operation for FocusProbe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn widget::Operation)) {
        operate(self);
    }

    fn focusable(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn widget::operation::Focusable,
    ) {
        self.0 |= state.is_focused();
    }

    fn custom(
        &mut self,
        _id: Option<&widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn std::any::Any,
    ) {
        if let Some(visible) = crate::widgets::control_button::visible_focus(state) {
            self.0 = visible;
        }
    }
}

impl<Message> widget::Meta for FocusTooltip<'_, Message> {}

impl<Message> Widget<Message, Theme, iced::Renderer> for FocusTooltip<'_, Message> {
    fn tag(&self) -> widget::tree::Tag {
        if self.rich {
            widget::tree::Tag::of::<HoverState>()
        } else {
            widget::tree::Tag::stateless()
        }
    }

    fn state(&self) -> widget::tree::State {
        if self.rich {
            widget::tree::State::new(HoverState::default())
        } else {
            widget::tree::State::None
        }
    }

    fn diff(&mut self, tree: &mut widget::Tree) {
        tree.diff_children(&mut [&mut self.trigger, &mut self.hint]);
        if self.rich && !self.options.enabled {
            tree.state.downcast_mut::<HoverState>().since = None;
        }
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
        if self.rich {
            let state = tree.state.downcast_mut::<HoverState>();
            let hovered = self.options.enabled
                && cursor.is_over(layout.bounds())
                && !matches!(
                    event,
                    Event::Window(iced::window::Event::Unfocused)
                        | Event::Mouse(mouse::Event::CursorLeft | mouse::Event::ButtonPressed(_))
                );
            if hovered {
                let since = *state.since.get_or_insert_with(Instant::now);
                if since.elapsed() < self.options.delay {
                    shell.request_redraw_at(since + self.options.delay);
                }
            } else if state.since.take().is_some() {
                shell.request_redraw();
            }
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

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        if !self.options.enabled {
            return Vec::new();
        }
        let mut focus = FocusProbe::default();
        self.trigger.operate(
            &mut tree.children[0],
            layout,
            viewport,
            renderer,
            &mut focus,
        );
        let hovered = self.rich
            && tree
                .state
                .downcast_ref::<HoverState>()
                .since
                .is_some_and(|since| since.elapsed() >= self.options.delay);
        if focus.0 || hovered {
            let hint_tree = &mut tree.children[1];
            let hint_viewport = Rectangle::with_size(window);
            self.hint.layout(
                hint_tree,
                renderer,
                &layout::Limits::new(Size::ZERO, window),
            );
            let position = position_layer(
                layout.bounds() + translation,
                hint_tree.size,
                hint_viewport,
                PositioningOptions {
                    preferred: self.options.placement,
                    gap: if self.rich {
                        TOKENS.spacing.s2
                    } else {
                        TOKENS.spacing.s1_5
                    },
                    alignment: if self.rich {
                        super::Alignment::Start
                    } else {
                        super::Alignment::Center
                    },
                    clamp_to_viewport: true,
                    flip_when_overflow: true,
                },
            );
            vec![overlay::Element::new(Box::new(FocusHint {
                content: &mut self.hint,
                layout: Layout::new(hint_tree.size).move_to(position.point),
                tree: hint_tree,
            }))]
        } else {
            self.trigger.overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
                window,
            )
        }
    }
}

struct FocusHint<'a, 'b, Message> {
    content: &'b mut Element<'a, Message>,
    tree: &'b mut widget::Tree,
    layout: Layout,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for FocusHint<'_, '_, Message> {
    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        cursor: mouse::Cursor,
    ) {
        renderer.with_layer(self.layout.bounds(), |renderer| {
            self.content.draw(
                self.tree,
                renderer,
                theme,
                style,
                self.layout,
                cursor,
                &self.layout.bounds(),
            );
        });
    }

    fn operate(&mut self, renderer: &iced::Renderer, operation: &mut dyn widget::Operation) {
        self.content.operate(
            self.tree,
            self.layout,
            &self.layout.bounds(),
            renderer,
            operation,
        );
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::renderer::{Headless, Settings as RendererSettings};
    use iced::advanced::widget::operation::focusable;
    use iced::Point;

    use super::*;
    use crate::{control_button, fonts::DEFAULT_LINE_HEIGHT, variants::ButtonVariant};

    #[test]
    fn rich_hover_and_focus_share_one_hint_and_disabling_keeps_focus() {
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            RendererSettings::default(),
            Some("tiny-skia"),
        ))
        .expect("software layout renderer");
        let build = |enabled| {
            focus_tooltip_content(
                control_button::<()>(None, Some("Account".into()), ButtonVariant::Text)
                    .id("avatar")
                    .width(Length::Fixed(40.0))
                    .min_height(40.0),
                container(text("Full account and server")).width(195),
                TooltipOptions {
                    enabled,
                    delay: std::time::Duration::ZERO,
                    ..TooltipOptions::default()
                },
            )
        };
        let mut control = build(true);
        let mut tree = widget::Tree::new(&control);
        tree.diff(&mut control);
        let viewport = Rectangle::with_size(Size::new(400.0, 400.0));
        control.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let layout = Layout::new(tree.size).move_to(Point::new(12.0, 300.0));
        let mut messages = iced::advanced::shell::Bus::new();
        let cursor = Point::new(30.0, 320.0);
        control.update(
            &mut tree,
            &Event::Mouse(mouse::Event::CursorMoved { position: cursor }),
            layout,
            mouse::Cursor::Available(cursor),
            &renderer,
            &mut Shell::new(
                &iced::window::Headless,
                iced::advanced::shell::Waker::noop(),
                &mut messages,
            ),
            &viewport,
        );
        assert_eq!(
            control
                .overlay(
                    &mut tree,
                    layout,
                    &renderer,
                    &viewport,
                    Vector::ZERO,
                    viewport.size()
                )
                .len(),
            1
        );
        control.operate(
            &mut tree,
            layout,
            &viewport,
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("avatar")),
        );
        assert_eq!(
            control
                .overlay(
                    &mut tree,
                    layout,
                    &renderer,
                    &viewport,
                    Vector::ZERO,
                    viewport.size()
                )
                .len(),
            1
        );
        control = build(false);
        tree.diff(&mut control);
        assert!(control
            .overlay(
                &mut tree,
                layout,
                &renderer,
                &viewport,
                Vector::ZERO,
                viewport.size()
            )
            .is_empty());
        control = build(true);
        tree.diff(&mut control);
        assert_eq!(
            control
                .overlay(
                    &mut tree,
                    layout,
                    &renderer,
                    &viewport,
                    Vector::ZERO,
                    viewport.size()
                )
                .len(),
            1,
            "disabling disclosure must not discard the avatar's keyboard focus"
        );
        assert!(
            messages.into_iter().next().is_none(),
            "disclosure is not an activation"
        );
    }

    #[test]
    fn nonclickable_selection_reveals_full_value_only_while_focused() {
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            RendererSettings {
                font: iced::Font::DEFAULT,
                text_size: 14.0.into(),
                line_height: DEFAULT_LINE_HEIGHT,
                metrics_hinting: false,
            },
            Some("tiny-skia"),
        ))
        .expect("software layout renderer");
        let mut control = focus_tooltip(
            control_button::<()>(
                None,
                Some("Selected profile".to_owned()),
                ButtonVariant::Secondary,
            )
            .id("selected-profile"),
            "UnbrokenUsername".repeat(12),
            TooltipOptions::default(),
        );
        let mut tree = widget::Tree::new(&control);
        tree.diff(&mut control);
        let viewport = Rectangle::with_size(Size::new(368.0, 300.0));
        control.layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let layout = Layout::new(tree.size).move_to(Point::new(0.0, 250.0));
        assert!(control
            .overlay(
                &mut tree,
                layout,
                &renderer,
                &viewport,
                Vector::ZERO,
                viewport.size()
            )
            .is_empty());
        control.operate(
            &mut tree,
            layout,
            &viewport,
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new("selected-profile")),
        );
        let mut hint = control
            .overlay(
                &mut tree,
                layout,
                &renderer,
                &viewport,
                Vector::ZERO,
                viewport.size(),
            )
            .into_iter()
            .next()
            .expect("keyboard focus reveals the selected account's full value");
        #[derive(Default)]
        struct HintBounds(Option<Rectangle>);
        impl widget::Operation for HintBounds {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
                visit(self);
            }
            fn container(
                &mut self,
                _id: Option<&widget::Id>,
                bounds: Rectangle,
                _viewport: &Rectangle,
            ) {
                self.0 = Some(bounds);
            }
        }
        let mut hint_bounds = HintBounds::default();
        hint.as_overlay_mut().operate(&renderer, &mut hint_bounds);
        let hint_bounds = hint_bounds.0.expect("hint exposes its container");
        assert!(viewport.contains(hint_bounds.position()));
        assert!(hint_bounds.x + hint_bounds.width <= viewport.width);
        assert!(hint_bounds.y + hint_bounds.height <= viewport.height);
        assert!(
            hint_bounds.height > 48.0,
            "the complete unbroken value must wrap within the hint"
        );
        drop(hint);
        let mut messages = iced::advanced::shell::Bus::new();
        control.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            mouse::Cursor::Unavailable,
            &renderer,
            &mut Shell::new(
                &iced::window::Headless,
                iced::advanced::shell::Waker::noop(),
                &mut messages,
            ),
            &viewport,
        );
        assert!(control
            .overlay(
                &mut tree,
                layout,
                &renderer,
                &viewport,
                Vector::ZERO,
                viewport.size()
            )
            .is_empty());
    }
}
