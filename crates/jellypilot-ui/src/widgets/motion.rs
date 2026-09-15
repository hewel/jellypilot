//! Shared motion primitives for the 2026-09-15 desktop motion contract.
//!
//! All primitives are widget-owned: animation state lives in the widget tree,
//! advances on `RedrawRequested` timestamps, and requests redraws only while a
//! transition is in flight. There is no app-wide ticker and no retained
//! snapshot layers — every primitive animates live layout, translation, or
//! clip so hit targets always match the drawn pixels.
//!
//! This fork's renderer has no group-opacity primitive, so the vocabulary is
//! translation, clip, and layout size rather than fades.
//!
//! [`scope`] publishes the motion policy to every descendant — including
//! overlay layers — through a dynamically-scoped thread-local. Animated
//! widgets read [`enabled`] inside their own calls, so call sites never thread
//! reduced-motion flags through the tree. The per-call `enabled` argument on
//! each primitive is an additional local gate, ANDed with the scope policy.

use std::any::Any;
use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, Widget};
use iced::advanced::{overlay, Shell};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, Vector};

use crate::tokens::TOKENS;

/// The eased translation applied by [`transition`] and [`reveal`], in logical
/// pixels. Kept at or below 12px per the restrained-motion contract.
const TRANSITION_OFFSET: f32 = 12.0;
thread_local! {
    /// Motion policy of the enclosing [`scope`]. `true` outside any scope so
    /// widgets used without a scope (tests, detached surfaces) still animate;
    /// the application root is expected to install the real policy once.
    static MOTION_ENABLED: Cell<bool> = const { Cell::new(true) };
}

/// Whether motion is currently permitted by the enclosing [`scope`].
///
/// Reads the dynamically-scoped policy: valid only inside widget `diff`,
/// `layout`, `update`, `draw`, `operate`, `mouse_interaction`, and `overlay`
/// calls running under a scope — which is exactly where animated widgets need
/// it. Outside any scope this returns `true`.
pub fn enabled() -> bool {
    MOTION_ENABLED.get()
}

/// Runs `f` with `enabled` as the motion policy, restoring the caller's policy
/// afterwards. Reentrant and panic-safe; the innermost scope wins.
pub(crate) fn with_policy<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
    struct Guard(bool);
    impl Drop for Guard {
        fn drop(&mut self) {
            MOTION_ENABLED.set(self.0);
        }
    }
    let _guard = Guard(MOTION_ENABLED.replace(enabled));
    f()
}

/// An eased scalar transition between `from` and `to`, driven by
/// `RedrawRequested` timestamps so tests can drive it deterministically.
///
/// `from`/`to` are eased-space endpoints: retargeting mid-flight captures the
/// currently displayed eased value as the new `from`, so interrupted
/// transitions continue from what the user sees.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Tween {
    pub(crate) fn new(from: f32, to: f32, start: Instant, duration: Duration) -> Self {
        Self {
            from,
            to,
            start,
            duration,
        }
    }

    /// The eased value at `now`. Non-finite or zero durations settle
    /// immediately at `to`.
    pub(crate) fn eased(&self, now: Instant) -> f32 {
        let duration = self.duration.as_secs_f32();
        if !duration.is_finite() || duration <= 0.0 {
            return self.to;
        }
        let progress = now.saturating_duration_since(self.start).as_secs_f32() / duration;
        self.from + (self.to - self.from) * TOKENS.easings.standard.sample(progress)
    }

    pub(crate) fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.duration
    }
}

/// Advances a layout-animated tween on `RedrawRequested`: stores the eased
/// progress in `shown`, invalidates layout only when the displayed value
/// actually changed (keeping the runaway-layout guard quiet), and requests the
/// next frame while unfinished. When the scope disables motion the widget
/// snaps to `target` and stops demanding frames.
pub(crate) fn tick_layout<Message>(
    tween: &mut Option<Tween>,
    shown: &mut f32,
    target: f32,
    event: &Event,
    shell: &mut Shell<'_, Message>,
) {
    if !enabled() {
        if tween.take().is_some() || *shown != target {
            *shown = target;
            shell.invalidate_layout();
        }
        return;
    }
    let Some(running) = *tween else {
        return;
    };
    let Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
        return;
    };
    let eased = running.eased(*now);
    if eased != *shown {
        *shown = eased;
        shell.invalidate_layout();
    }
    if running.finished(*now) {
        *shown = running.to;
        *tween = None;
    } else {
        shell.request_redraw();
    }
}

/// Advances a draw-only tween on `RedrawRequested`, requesting the next frame
/// while unfinished. Disabled motion settles immediately.
pub(crate) fn tick_draw<Message>(
    tween: &mut Option<Tween>,
    event: &Event,
    shell: &mut Shell<'_, Message>,
) {
    if !enabled() {
        tween.take();
        return;
    }
    let Some(running) = tween else {
        return;
    };
    let Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
        return;
    };
    if running.finished(*now) {
        tween.take();
    } else {
        shell.request_redraw();
    }
}

/// Linearly interpolates two colors in sRGBA space.
pub(crate) fn lerp_color(a: iced::Color, b: iced::Color, t: f32) -> iced::Color {
    let t = t.clamp(0.0, 1.0);
    iced::Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// Interpolates optional solid backgrounds; gradients and mixed kinds snap to
/// the target. `None` participates as transparent black so fills fade in.
pub(crate) fn lerp_background(
    a: Option<iced::Background>,
    b: Option<iced::Background>,
    t: f32,
) -> Option<iced::Background> {
    match (a, b) {
        (Some(iced::Background::Color(a)), Some(iced::Background::Color(b))) => {
            Some(iced::Background::Color(lerp_color(a, b, t)))
        }
        (None, Some(iced::Background::Color(b))) => {
            let mixed = lerp_color(iced::Color::TRANSPARENT, b, t);
            (mixed.a > 0.0).then_some(iced::Background::Color(mixed))
        }
        (None, None) => None,
        _ => b,
    }
}

/// Publishes the motion policy to every descendant of `content`, including
/// overlay layers. Wrap the application root once with
/// `enabled = !reduced_motion && images_visible`; descendants read the policy
/// through [`enabled`] without per-callsite plumbing.
pub fn scope<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    enabled: bool,
) -> Element<'a, Message> {
    Element::new(Scope {
        content: content.into(),
        enabled,
    })
}

struct Scope<'a, Message> {
    content: Element<'a, Message>,
    enabled: bool,
}

/// Forwards the child's own state like [`super::inert`], so wrapping or
/// unwrapping the root never discards descendant widget state.
impl<Message> Widget<Message, Theme, iced::Renderer> for Scope<'_, Message> {
    fn tag(&self) -> widget::tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> widget::tree::State {
        self.content.as_widget().state()
    }

    fn diff(&mut self, tree: &mut Tree) {
        with_policy(self.enabled, || self.content.as_widget_mut().diff(tree));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        with_policy(self.enabled, || {
            self.content.as_widget_mut().layout(tree, renderer, limits)
        })
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        with_policy(self.enabled, || {
            self.content
                .as_widget_mut()
                .operate(tree, layout, renderer, operation)
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        with_policy(self.enabled, || {
            self.content
                .as_widget_mut()
                .update(tree, event, layout, cursor, renderer, shell, viewport)
        });
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        with_policy(self.enabled, || {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport)
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        with_policy(self.enabled, || {
            self.content
                .as_widget()
                .mouse_interaction(tree, layout, cursor, viewport, renderer)
        })
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        let content = with_policy(self.enabled, || {
            self.content
                .as_widget_mut()
                .overlay(tree, layout, renderer, viewport, translation)
        });
        // Overlay layers live outside the scoped traversal, so the policy must
        // be re-established around every overlay call.
        content.map(|content| {
            overlay::Element::new(Box::new(PolicyOverlay {
                content,
                enabled: self.enabled,
            }))
        })
    }
}

/// Re-applies a scope's policy inside an overlay layer.
struct PolicyOverlay<'a, Message> {
    content: overlay::Element<'a, Message, Theme, iced::Renderer>,
    enabled: bool,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for PolicyOverlay<'_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        with_policy(self.enabled, || {
            self.content.as_overlay_mut().layout(renderer, bounds)
        })
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
    ) {
        with_policy(self.enabled, || {
            self.content
                .as_overlay_mut()
                .update(event, layout, cursor, renderer, shell)
        });
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        with_policy(self.enabled, || {
            self.content
                .as_overlay()
                .draw(renderer, theme, style, layout, cursor)
        });
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        with_policy(self.enabled, || {
            self.content
                .as_overlay_mut()
                .operate(layout, renderer, operation)
        });
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        with_policy(self.enabled, || {
            self.content
                .as_overlay()
                .mouse_interaction(layout, cursor, renderer)
        })
    }

    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        with_policy(self.enabled, || {
            self.content.as_overlay_mut().overlay(layout, renderer)
        })
        .map(|content| {
            overlay::Element::new(Box::new(PolicyOverlay {
                content,
                enabled: self.enabled,
            }))
        })
    }

    fn index(&self) -> f32 {
        self.content.as_overlay().index()
    }
}

/// Slides newly-keyed `content` up by [`TRANSITION_OFFSET`] over `duration`.
///
/// The first mount never animates, and a rebuild with the same `key` is a
/// plain diff — data-only updates and restored surfaces stay put. The child
/// tree is always preserved and diffed, so descendant state survives key
/// changes. Interrupting a transition retargets from the currently displayed
/// offset. Hit targets track the drawn position: events dispatch against the
/// translated layout.
pub fn transition<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    key: u64,
    enabled: bool,
    duration: Duration,
) -> Element<'a, Message> {
    Element::new(Transition {
        content: content.into(),
        key,
        enabled,
        duration,
    })
}

/// The animated axis for [`resize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
    Both,
}

struct Transition<'a, Message, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    key: u64,
    enabled: bool,
    duration: Duration,
}

#[derive(Debug)]
pub(crate) struct TransitionState {
    key: Option<u64>,
    tween: Option<Tween>,
    /// Eased progress the last layout pass presented.
    shown: f32,
}

impl TransitionState {
    pub(crate) fn motion_active(&self) -> bool {
        self.tween.is_some()
    }
}

impl<Message, Renderer> Transition<'_, Message, Renderer> {
    /// The entrance offset applied to the child at eased progress `shown`.
    fn offset(shown: f32) -> f32 {
        (1.0 - shown) * TRANSITION_OFFSET
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Transition<'_, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<TransitionState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(TransitionState {
            key: None,
            tween: None,
            shown: 1.0,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<TransitionState>();
        let animate = self.enabled && enabled();
        match state.key {
            // First mount presents the final state; restoration must not replay.
            None => {
                state.key = Some(self.key);
                state.shown = 1.0;
            }
            Some(previous) if previous != self.key => {
                state.key = Some(self.key);
                if animate {
                    let from = if state.tween.is_some() {
                        state.shown
                    } else {
                        0.0
                    };
                    state.tween =
                        (from != 1.0).then(|| Tween::new(from, 1.0, Instant::now(), self.duration));
                    state.shown = from;
                } else {
                    state.tween = None;
                    state.shown = 1.0;
                }
            }
            // Same key: data-only update; an in-flight transition continues.
            Some(_) => {
                if !animate {
                    state.tween = None;
                    state.shown = 1.0;
                }
            }
        }
        tree.diff_children(&mut [&mut self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let offset = Self::offset(tree.state.downcast_ref::<TransitionState>().shown);
        layout::Node::with_children(
            child.size(),
            vec![child.translate(Vector::new(0.0, offset))],
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.custom(
            None,
            layout.bounds(),
            tree.state.downcast_mut::<TransitionState>(),
        );
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout,
            renderer,
            operation,
        );
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
        let state = tree.state.downcast_mut::<TransitionState>();
        tick_layout(&mut state.tween, &mut state.shown, 1.0, event, shell);
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
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
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            child_layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(child_layout) = layout.children().next() else {
            return mouse::Interaction::None;
        };
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let child_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child_layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Collapses or expands `content` with a top-anchored height wipe over
/// `duration`. This is the docked-flow primitive — opt in only where the
/// surrounding layout should shrink with the content (e.g. a docked footer).
/// Floating layers must use [`reveal`] instead: a full-window wipe is not
/// restrained motion.
///
/// The child is always supplied by the caller: while `visible` is `false` the
/// exit keeps animating the currently-supplied content until it settles, then
/// the widget takes zero layout space. Logically-hidden content receives no
/// events, operations, or overlay passes. The caller must keep supplying the
/// content it wants animated out; query [`Activity`] to learn when the exit
/// has settled and the content can be dropped.
pub fn collapse<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    visible: bool,
    enabled: bool,
    duration: Duration,
) -> Element<'a, Message> {
    Element::new(Collapse {
        content: content.into(),
        visible,
        enabled,
        duration,
    })
}

struct Collapse<'a, Message, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    visible: bool,
    enabled: bool,
    duration: Duration,
}

#[derive(Debug)]
pub(crate) struct CollapseState {
    mounted: bool,
    visible: bool,
    tween: Option<Tween>,
    /// Fraction of the child height currently revealed (1 = fully shown).
    shown: f32,
}

impl CollapseState {
    pub(crate) fn motion_active(&self) -> bool {
        self.tween.is_some()
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Collapse<'_, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<CollapseState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(CollapseState {
            mounted: false,
            visible: true,
            tween: None,
            shown: 1.0,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<CollapseState>();
        let animate = self.enabled && enabled();
        if !state.mounted {
            // First mount presents the logical state directly.
            state.mounted = true;
            state.visible = self.visible;
            state.shown = if self.visible { 1.0 } else { 0.0 };
        } else if state.visible != self.visible {
            state.visible = self.visible;
            let target = if self.visible { 1.0 } else { 0.0 };
            if animate {
                let from = state.shown;
                state.tween = (from != target)
                    .then(|| Tween::new(from, target, Instant::now(), self.duration));
                state.shown = from;
            } else {
                state.tween = None;
                state.shown = target;
            }
        } else if !animate {
            state.tween = None;
            state.shown = if self.visible { 1.0 } else { 0.0 };
        }
        tree.diff_children(&mut [&mut self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_ref::<CollapseState>();
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        // The occupied height follows the revealed fraction so siblings
        // collapse smoothly instead of jumping when the exit settles.
        let size = Size::new(child.size().width, child.size().height * state.shown);
        layout::Node::with_children(limits.resolve(Length::Fit, Length::Fit, size), vec![child])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<CollapseState>();
        // Always report our own state so Activity can see a settling exit.
        operation.custom(None, layout.bounds(), state);
        if !state.visible {
            return;
        }
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout,
            renderer,
            operation,
        );
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
        let state = tree.state.downcast_mut::<CollapseState>();
        let target = if state.visible { 1.0 } else { 0.0 };
        tick_layout(&mut state.tween, &mut state.shown, target, event, shell);
        // Logically hidden content is inert even while its exit is drawing.
        if !state.visible {
            return;
        }
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        // The node bounds are the revealed region; outside it the child is
        // clipped away, so its hit targets must not answer there.
        let cursor = if cursor.is_over(layout.bounds()) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
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
        let state = tree.state.downcast_ref::<CollapseState>();
        if state.shown <= 0.0 {
            return;
        }
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        let child = &tree.children[0];
        let content = &self.content;
        if state.shown >= 1.0 {
            content.as_widget().draw(
                child,
                renderer,
                theme,
                style,
                child_layout,
                cursor,
                viewport,
            );
        } else {
            // The node bounds already equal the revealed region; the layer
            // clips the still-full-height child to it.
            renderer.with_layer(layout.bounds(), |renderer| {
                content.as_widget().draw(
                    child,
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<CollapseState>();
        if !state.visible {
            return mouse::Interaction::None;
        }
        let Some(child_layout) = layout.children().next() else {
            return mouse::Interaction::None;
        };
        let cursor = if cursor.is_over(layout.bounds()) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let state = tree.state.downcast_ref::<CollapseState>();
        if !state.visible {
            return None;
        }
        let child_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child_layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Reveals or hides floating `content` with a small eased translation over
/// `duration` — the restrained motion for overlays, popovers, and modal
/// layers. The child keeps its full layout while entering or exiting and
/// slides by [`TRANSITION_OFFSET`]; only a settled hidden widget takes zero
/// space. Logically-hidden content receives no events, operations, or overlay
/// passes, so an exiting layer is already inert while it slides out.
///
/// The child is always supplied by the caller: keep supplying the content you
/// want animated out and query [`Activity`] to learn when the exit has settled
/// and the layer can be dropped. For docked content whose surroundings should
/// shrink with it, use [`collapse`] instead.
pub fn reveal<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    visible: bool,
    enabled: bool,
    duration: Duration,
) -> Element<'a, Message> {
    Element::new(Reveal {
        content: content.into(),
        visible,
        enabled,
        duration,
    })
}

struct Reveal<'a, Message, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    visible: bool,
    enabled: bool,
    duration: Duration,
}

#[derive(Debug)]
pub(crate) struct RevealState {
    mounted: bool,
    visible: bool,
    tween: Option<Tween>,
    /// Eased progress the last update presented (1 = fully shown).
    shown: f32,
}

impl RevealState {
    pub(crate) fn motion_active(&self) -> bool {
        self.tween.is_some()
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Reveal<'_, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<RevealState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(RevealState {
            mounted: false,
            visible: true,
            tween: None,
            shown: 1.0,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<RevealState>();
        let animate = self.enabled && enabled();
        if !state.mounted {
            // First mount presents the logical state directly.
            state.mounted = true;
            state.visible = self.visible;
            state.shown = if self.visible { 1.0 } else { 0.0 };
        } else if state.visible != self.visible {
            state.visible = self.visible;
            let target = if self.visible { 1.0 } else { 0.0 };
            if animate {
                let from = state.shown;
                state.tween = (from != target)
                    .then(|| Tween::new(from, target, Instant::now(), self.duration));
                state.shown = from;
            } else {
                state.tween = None;
                state.shown = target;
            }
        } else if !animate {
            state.tween = None;
            state.shown = if self.visible { 1.0 } else { 0.0 };
        }
        tree.diff_children(&mut [&mut self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_ref::<RevealState>();
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        // Full child layout while entering or exiting; only a settled hidden
        // widget reports zero space.
        let size = if !state.visible && state.tween.is_none() {
            Size::ZERO
        } else {
            child.size()
        };
        let offset = (1.0 - state.shown) * TRANSITION_OFFSET;
        layout::Node::with_children(size, vec![child.translate(Vector::new(0.0, offset))])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<RevealState>();
        // Always report our own state so Activity can see a settling exit.
        operation.custom(None, layout.bounds(), state);
        if !state.visible {
            return;
        }
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout,
            renderer,
            operation,
        );
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
        let state = tree.state.downcast_mut::<RevealState>();
        let target = if state.visible { 1.0 } else { 0.0 };
        tick_layout(&mut state.tween, &mut state.shown, target, event, shell);
        // Logically hidden content is inert even while its exit is drawing.
        if !state.visible {
            return;
        }
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
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
        let state = tree.state.downcast_ref::<RevealState>();
        if state.shown <= 0.0 {
            return;
        }
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            child_layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<RevealState>();
        if !state.visible {
            return mouse::Interaction::None;
        }
        let Some(child_layout) = layout.children().next() else {
            return mouse::Interaction::None;
        };
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let state = tree.state.downcast_ref::<RevealState>();
        if !state.visible {
            return None;
        }
        let child_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child_layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Animates the widget's intrinsic size on `axis` when `key` changes.
///
/// Only key changes animate: continuous external sizing (window drags,
/// constraint changes) retargets the in-flight animation from the currently
/// displayed size without restarting it, and stays fully direct once settled.
/// The child is laid out under the animated size each frame, so hit targets
/// match the drawn pixels throughout.
pub fn resize<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    key: u64,
    enabled: bool,
    duration: Duration,
    axis: Axis,
) -> Element<'a, Message> {
    Element::new(Resize {
        content: content.into(),
        key,
        enabled,
        duration,
        axis,
    })
}

struct Resize<'a, Message, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    key: u64,
    enabled: bool,
    duration: Duration,
    axis: Axis,
}

#[derive(Debug)]
pub(crate) struct ResizeState {
    key: Option<u64>,
    tween: Option<Tween>,
    /// Eased progress the last layout pass presented.
    shown: f32,
    from: Size,
    to: Size,
    /// The size the last layout pass actually presented.
    shown_size: Size,
}

impl ResizeState {
    pub(crate) fn motion_active(&self) -> bool {
        self.tween.is_some()
    }

    fn displayed(&self) -> Size {
        if self.tween.is_some() {
            self.shown_size
        } else {
            self.to
        }
    }
}

fn lerp_size(from: Size, to: Size, t: f32, axis: Axis) -> Size {
    let t = t.clamp(0.0, 1.0);
    Size::new(
        if matches!(axis, Axis::Horizontal | Axis::Both) {
            from.width + (to.width - from.width) * t
        } else {
            to.width
        },
        if matches!(axis, Axis::Vertical | Axis::Both) {
            from.height + (to.height - from.height) * t
        } else {
            to.height
        },
    )
}

fn tighten(limits: &layout::Limits, size: Size, axis: Axis) -> layout::Limits {
    let mut max = limits.max();
    if matches!(axis, Axis::Horizontal | Axis::Both) {
        max.width = size.width;
    }
    if matches!(axis, Axis::Vertical | Axis::Both) {
        max.height = size.height;
    }
    layout::Limits::new(Size::ZERO, max)
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Resize<'_, Message, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<ResizeState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(ResizeState {
            key: None,
            tween: None,
            shown: 1.0,
            from: Size::ZERO,
            to: Size::ZERO,
            shown_size: Size::ZERO,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<ResizeState>();
        let animate = self.enabled && enabled();
        match state.key {
            None => {
                state.key = Some(self.key);
                state.shown = 1.0;
            }
            Some(previous) if previous != self.key => {
                state.key = Some(self.key);
                if animate {
                    // The new target is measured in the next layout; the
                    // animation starts from the currently displayed size.
                    state.from = state.displayed();
                    state.tween = Some(Tween::new(0.0, 1.0, Instant::now(), self.duration));
                    state.shown = 0.0;
                } else {
                    state.tween = None;
                    state.shown = 1.0;
                }
            }
            Some(_) => {
                if !animate {
                    state.tween = None;
                    state.shown = 1.0;
                }
            }
        }
        tree.diff_children(&mut [&mut self.content]);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<ResizeState>();
        // Measure the natural target under the real limits first.
        let natural = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
            .size();
        if state.tween.is_some() && natural != state.to {
            // Preserve the displayed size without restarting the easing clock.
            // Rebasing `from` must account for the progress already consumed.
            let remaining = 1.0 - state.shown;
            if remaining > f32::EPSILON {
                state.from = Size::new(
                    (state.shown_size.width - natural.width * state.shown) / remaining,
                    (state.shown_size.height - natural.height * state.shown) / remaining,
                );
            } else {
                state.tween = None;
                state.shown = 1.0;
            }
            state.to = natural;
        } else if state.tween.is_none() {
            state.to = natural;
        }

        let (child, size) = if state.tween.is_some() {
            let shown_size = lerp_size(state.from, state.to, state.shown, self.axis);
            state.shown_size = shown_size;
            let child = self.content.as_widget_mut().layout(
                &mut tree.children[0],
                renderer,
                &tighten(limits, shown_size, self.axis),
            );
            (child, shown_size)
        } else {
            state.shown_size = natural;
            (
                self.content
                    .as_widget_mut()
                    .layout(&mut tree.children[0], renderer, limits),
                natural,
            )
        };
        layout::Node::with_children(limits.resolve(Length::Fit, Length::Fit, size), vec![child])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.custom(
            None,
            layout.bounds(),
            tree.state.downcast_mut::<ResizeState>(),
        );
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout,
            renderer,
            operation,
        );
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
        let state = tree.state.downcast_mut::<ResizeState>();
        tick_layout(&mut state.tween, &mut state.shown, 1.0, event, shell);
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        // A fixed-size child can overflow the animated bounds; its hit targets
        // only answer inside the displayed region.
        let cursor = if state.tween.is_some() && !cursor.is_over(layout.bounds()) {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
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
        let state = tree.state.downcast_ref::<ResizeState>();
        let Some(child_layout) = layout.children().next() else {
            return;
        };
        let child = &tree.children[0];
        let content = &self.content;
        if state.tween.is_some() {
            renderer.with_layer(layout.bounds(), |renderer| {
                content.as_widget().draw(
                    child,
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    viewport,
                );
            });
        } else {
            content.as_widget().draw(
                child,
                renderer,
                theme,
                style,
                child_layout,
                cursor,
                viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<ResizeState>();
        let Some(child_layout) = layout.children().next() else {
            return mouse::Interaction::None;
        };
        let cursor = if state.tween.is_some() && !cursor.is_over(layout.bounds()) {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let child_layout = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child_layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Reports whether any motion primitive in a traversed subtree is animating.
///
/// Run it through `Widget::operate` (or `Overlay::operate`) over the content
/// that hosts the primitives. Callers that must keep content or an overlay
/// layer alive until an exit settles — popovers, modal hosts — can poll this
/// before dropping the layer.
///
#[derive(Default)]
pub struct Activity {
    active: bool,
}

impl Activity {
    /// Whether any visited primitive is mid-transition.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Operation for Activity {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(&mut self, _id: Option<&widget::Id>, _bounds: Rectangle, state: &mut dyn Any) {
        if let Some(state) = state.downcast_ref::<TransitionState>() {
            self.active |= state.motion_active();
        }
        if let Some(state) = state.downcast_ref::<CollapseState>() {
            self.active |= state.motion_active();
        }
        if let Some(state) = state.downcast_ref::<RevealState>() {
            self.active |= state.motion_active();
        }
        if let Some(state) = state.downcast_ref::<ResizeState>() {
            self.active |= state.motion_active();
        }
        if let Some(state) = state.downcast_ref::<super::artwork_grid::ArtworkGridState>() {
            self.active |= state.motion_active();
        }
        if let Some(state) = state.downcast_ref::<super::control_button::State>() {
            self.active |= state.motion_active();
        }
        if let Some(state) = state.downcast_ref::<super::switch::SwitchVisualState>() {
            self.active |= state.motion_active();
        }
    }
}

/// Final grid-local position of an item under the given metrics.
pub(crate) fn cell_position(
    item: usize,
    columns: usize,
    cell_width: f32,
    row_height: f32,
    column_gap: f32,
) -> Point {
    let row = item / columns;
    let column = item % columns;
    Point::new(
        column as f32 * (cell_width + column_gap),
        row as f32 * row_height,
    )
}

/// Shared per-item position maps for the animated artwork grid: `current`
/// holds the final positions from the last layout, `from` the positions the
/// in-flight reflow started from.
#[derive(Debug, Default)]
pub(crate) struct GridMotion {
    pub(crate) columns: usize,
    pub(crate) current: HashMap<usize, Point>,
    pub(crate) from: HashMap<usize, Point>,
    /// Breakpoint target only: later continuous sizing must move cells directly.
    pub(crate) reflow_target: Option<(super::artwork_grid::ArtworkGridMetrics, f32)>,
    pub(crate) tween: Option<Tween>,
    pub(crate) shown: f32,
}

impl GridMotion {
    /// Translation at the last animation frame, shared by layout and input.
    pub(crate) fn delta(&self, item: usize) -> Vector {
        let (Some(_), Some(from), Some((metrics, gap))) =
            (self.tween, self.from.get(&item), self.reflow_target)
        else {
            return Vector::ZERO;
        };
        let to = cell_position(
            item,
            metrics.columns,
            metrics.cell_width,
            metrics.row_height,
            gap,
        );
        let progress = self.shown;
        Vector::new(
            (from.x - to.x) * (1.0 - progress),
            (from.y - to.y) * (1.0 - progress),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use iced::advanced::layout::{self, Layout};
    use iced::advanced::mouse;
    use iced::advanced::widget::{Tree, Widget};
    use iced::advanced::Shell;
    use iced::widget::Space;
    use iced::{Element, Event, Point, Rectangle, Size, Theme};

    use super::{with_policy, Axis, Collapse, Resize, Reveal, Transition};
    use crate::tokens::TOKENS;

    const LIMITS: Size = Size::new(400.0, 400.0);

    type TestElement<'a> = Element<'a, (), Theme, ()>;

    fn redraw(at: Instant) -> Event {
        Event::Window(iced::window::Event::RedrawRequested(at))
    }

    fn make_shell<'a>(bus: &'a mut iced::advanced::shell::Bus<()>) -> Shell<'a, ()> {
        Shell::new(
            &iced::window::Headless,
            iced::advanced::shell::Waker::noop(),
            bus,
        )
    }

    fn limits() -> layout::Limits {
        layout::Limits::new(Size::ZERO, LIMITS)
    }

    fn space<'a>(width: f32, height: f32) -> TestElement<'a> {
        Space::new().width(width).height(height).into()
    }

    fn button() -> TestElement<'static> {
        iced::widget::button(Space::new().width(100).height(40))
            .padding(0)
            .on_press(())
            .into()
    }

    fn click(
        widget: &mut impl Widget<(), Theme, ()>,
        tree: &mut Tree,
        layout: Layout<'_>,
        point: Point,
    ) -> Vec<()> {
        let mut bus = iced::advanced::shell::Bus::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            widget.update(
                tree,
                &Event::Mouse(event),
                layout,
                mouse::Cursor::Available(point),
                &(),
                &mut make_shell(&mut bus),
                &Rectangle::with_size(LIMITS),
            );
        }
        bus.into_iter().collect()
    }

    fn transition_widget<'a>(content: TestElement<'a>, key: u64) -> Transition<'a, (), ()> {
        Transition {
            content,
            key,
            enabled: true,
            duration: TOKENS.durations.ms300,
        }
    }

    fn collapse_widget<'a>(content: TestElement<'a>, visible: bool) -> Collapse<'a, (), ()> {
        Collapse {
            content,
            visible,
            enabled: true,
            duration: TOKENS.durations.ms200,
        }
    }
    fn reveal_widget<'a>(content: TestElement<'a>, visible: bool) -> Reveal<'a, (), ()> {
        Reveal {
            content,
            visible,
            enabled: true,
            duration: TOKENS.durations.ms200,
        }
    }

    fn resize_widget<'a>(content: TestElement<'a>, key: u64) -> Resize<'a, (), ()> {
        Resize {
            content,
            key,
            enabled: true,
            duration: TOKENS.durations.ms300,
            axis: Axis::Both,
        }
    }

    #[test]
    fn transition_suppresses_initial_mount_and_same_key() {
        let mut widget = transition_widget(space(100.0, 100.0), 7);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        assert_eq!(
            widget.layout(&mut tree, &(), &limits()).children()[0]
                .bounds()
                .y,
            0.0
        );

        // Same key: a data-only rebuild keeps the tree settled.
        let mut widget = transition_widget(space(100.0, 100.0), 7);
        widget.diff(&mut tree);
        assert_eq!(
            widget.layout(&mut tree, &(), &limits()).children()[0]
                .bounds()
                .y,
            0.0
        );
    }

    #[test]
    fn transition_animates_key_change_and_settles() {
        let mut widget = transition_widget(space(100.0, 100.0), 1);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        let limits = limits();
        let node = widget.layout(&mut tree, &(), &limits);
        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(LIMITS);

        let mut widget = transition_widget(space(100.0, 100.0), 2);
        widget.diff(&mut tree);
        let initial = widget.layout(&mut tree, &(), &limits).children()[0]
            .bounds()
            .y;
        assert!(initial > 0.0);

        let start = Instant::now();
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_millis(75)),
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut shell,
            &viewport,
        );
        let offset = widget.layout(&mut tree, &(), &limits).children()[0]
            .bounds()
            .y;
        assert!(
            offset > 0.0 && offset < initial,
            "mid-flight offset, got {offset}"
        );
        assert_eq!(
            shell.redraw_request(),
            iced::window::RedrawRequest::NextFrame,
            "in-flight transition must keep requesting frames"
        );

        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_secs(5)),
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut shell,
            &viewport,
        );
        assert_eq!(
            widget.layout(&mut tree, &(), &limits).children()[0]
                .bounds()
                .y,
            0.0
        );
        assert_eq!(
            shell.redraw_request(),
            iced::window::RedrawRequest::Wait,
            "settled transition must not demand frames"
        );
    }

    #[test]
    fn transition_interrupt_retargets_from_displayed_progress() {
        let mut widget = transition_widget(space(100.0, 100.0), 1);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        let mut widget = transition_widget(space(100.0, 100.0), 2);
        widget.diff(&mut tree);
        let limits = limits();
        let node = widget.layout(&mut tree, &(), &limits);
        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(LIMITS);

        let start = Instant::now();
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_millis(150)),
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut shell,
            &viewport,
        );
        let mid = widget.layout(&mut tree, &(), &limits).children()[0]
            .bounds()
            .y;
        assert!(mid > 0.0 && mid < super::TRANSITION_OFFSET);

        // A second key change mid-flight continues from the displayed offset.
        let mut widget = transition_widget(space(100.0, 100.0), 3);
        widget.diff(&mut tree);
        assert_eq!(
            widget.layout(&mut tree, &(), &limits).children()[0]
                .bounds()
                .y,
            mid
        );
    }

    #[test]
    fn collapse_hides_events_and_collapses_when_settled() {
        let mut widget = collapse_widget(button(), true);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        let limits = limits();
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size().height, 40.0);
        assert_eq!(
            click(
                &mut widget,
                &mut tree,
                Layout::new(&node),
                Point::new(10.0, 10.0)
            ),
            vec![()]
        );

        // Flip to hidden: the exit animates while content is already inert.
        let mut widget = collapse_widget(button(), false);
        widget.diff(&mut tree);
        assert!(click(
            &mut widget,
            &mut tree,
            Layout::new(&node),
            Point::new(10.0, 10.0)
        )
        .is_empty());

        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(LIMITS);
        let inside = mouse::Cursor::Available(Point::new(10.0, 10.0));
        assert_eq!(
            widget.mouse_interaction(&tree, layout, inside, &viewport, &()),
            mouse::Interaction::None,
            "logically hidden content must not answer hit tests"
        );

        let start = Instant::now();
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_secs(5)),
            layout,
            inside,
            &(),
            &mut shell,
            &viewport,
        );

        // Settled hidden: the widget takes no space.
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size().height, 0.0, "settled hidden must be empty");
    }

    #[test]
    fn collapse_entrance_shrinks_bounds_progressively() {
        let mut widget = collapse_widget(space(100.0, 40.0), false);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        let limits = limits();
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size().height, 0.0, "mounted hidden stays empty");

        let mut widget = collapse_widget(space(100.0, 40.0), true);
        widget.diff(&mut tree);
        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(LIMITS);

        let start = Instant::now();
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_millis(100)),
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut shell,
            &viewport,
        );
        assert!(
            shell.is_layout_invalid().is_some(),
            "collapse animates through real layout"
        );

        let node = widget.layout(&mut tree, &(), &limits);
        let height = node.size().height;
        assert!(
            height > 0.0 && height < 40.0,
            "mid-collapse height {height} must interpolate"
        );
    }
    #[test]
    fn reveal_keeps_full_layout_while_animating_and_hides_events() {
        let mut widget = reveal_widget(button(), true);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        let limits = limits();
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size().height, 40.0);
        assert_eq!(
            click(
                &mut widget,
                &mut tree,
                Layout::new(&node),
                Point::new(10.0, 10.0)
            ),
            vec![()]
        );

        // Flip to hidden: the exit animates while content is already inert.
        let mut widget = reveal_widget(button(), false);
        widget.diff(&mut tree);
        assert!(click(
            &mut widget,
            &mut tree,
            Layout::new(&node),
            Point::new(10.0, 10.0)
        )
        .is_empty());

        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(LIMITS);
        let inside = mouse::Cursor::Available(Point::new(10.0, 10.0));
        assert_eq!(
            widget.mouse_interaction(&tree, layout, inside, &viewport, &()),
            mouse::Interaction::None,
            "logically hidden content must not answer hit tests"
        );

        // Mid-exit the widget still occupies its full space — the motion is a
        // small translation, not a height wipe.
        let start = Instant::now();
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_millis(50)),
            layout,
            inside,
            &(),
            &mut shell,
            &viewport,
        );
        let node = widget.layout(&mut tree, &(), &limits);
        let offset = node.children()[0].bounds().y;
        assert!(offset > 0.0 && offset < super::TRANSITION_OFFSET);
        assert_eq!(
            node.size().height,
            40.0,
            "exiting reveal keeps full layout while animating"
        );

        // Settled hidden: the widget takes no space and stops demanding frames.
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_secs(5)),
            layout,
            inside,
            &(),
            &mut shell,
            &viewport,
        );
        assert_eq!(
            shell.redraw_request(),
            iced::window::RedrawRequest::Wait,
            "settled hidden must not demand frames"
        );
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size(), Size::ZERO, "settled hidden must be empty");

        // Cancelling before the first entrance frame has no distance to animate.
        let mut widget = reveal_widget(button(), true);
        widget.diff(&mut tree);
        let mut widget = reveal_widget(button(), false);
        widget.diff(&mut tree);
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size(), Size::ZERO);
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(Instant::now()),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &(),
            &mut shell,
            &viewport,
        );
        assert_eq!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
    }

    #[test]
    fn resize_animates_key_change_and_stays_direct_otherwise() {
        let mut widget = resize_widget(space(100.0, 50.0), 1);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        let limits = limits();
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size(), Size::new(100.0, 50.0), "first mount is direct");

        // Same key, different content size: continuous sizing stays direct.
        let mut widget = resize_widget(space(200.0, 50.0), 1);
        widget.diff(&mut tree);
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(node.size(), Size::new(200.0, 50.0));

        // Key change animates through layout, starting at the displayed size.
        let mut widget = resize_widget(space(300.0, 50.0), 2);
        widget.diff(&mut tree);
        let node = widget.layout(&mut tree, &(), &limits);
        assert_eq!(
            node.size(),
            Size::new(200.0, 50.0),
            "animation starts at the displayed size"
        );

        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(LIMITS);
        let start = Instant::now();
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_millis(150)),
            layout,
            mouse::Cursor::Unavailable,
            &(),
            &mut shell,
            &viewport,
        );
        assert!(shell.is_layout_invalid().is_some());
        let node = widget.layout(&mut tree, &(), &limits);
        let width = node.size().width;
        assert!(
            width > 200.0 && width < 300.0,
            "mid-resize width {width} must interpolate"
        );

        // A new external target must not jump the currently displayed width.
        let mut widget = resize_widget(space(350.0, 50.0), 2);
        widget.diff(&mut tree);
        let node = widget.layout(&mut tree, &(), &limits);
        assert!((node.size().width - width).abs() < 0.001);
        let mut bus = iced::advanced::shell::Bus::new();
        let mut shell = make_shell(&mut bus);
        widget.update(
            &mut tree,
            &redraw(start + Duration::from_secs(5)),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &(),
            &mut shell,
            &viewport,
        );
        assert_eq!(
            widget.layout(&mut tree, &(), &limits).size(),
            Size::new(350.0, 50.0)
        );
        assert_eq!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
    }

    #[test]
    fn disabled_scope_snaps_everything() {
        let mut widget = transition_widget(space(100.0, 100.0), 1);
        let mut tree = Tree::new(&widget as &dyn Widget<(), Theme, ()>);
        widget.diff(&mut tree);
        let mut widget = transition_widget(space(100.0, 100.0), 2);
        widget.diff(&mut tree);
        assert!(
            widget.layout(&mut tree, &(), &limits()).children()[0]
                .bounds()
                .y
                > 0.0
        );

        // Diff under a disabled scope snaps to the final state.
        let mut widget = transition_widget(space(100.0, 100.0), 2);
        with_policy(false, || widget.diff(&mut tree));
        assert_eq!(
            widget.layout(&mut tree, &(), &limits()).children()[0]
                .bounds()
                .y,
            0.0
        );

        // Leaving the disabled scope restores animation for later intent.
        let mut widget = transition_widget(space(100.0, 100.0), 3);
        widget.diff(&mut tree);
        assert!(
            widget.layout(&mut tree, &(), &limits()).children()[0]
                .bounds()
                .y
                > 0.0
        );
    }
}
