//! Boolean switch using the shared keyboard-focusable control interaction.
//!
//! The interactive shell is a [`ControlButton`]; the 40×22 track and 18px knob
//! are drawn by [`SwitchVisual`], a non-interactive leaf that animates knob
//! travel and track/knob colors over `durations.ms150` under the enclosing
//! [`motion::scope`] policy.

use std::cell::Cell;
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::Shell;
use iced::widget::button;
use iced::{Border, Element, Event, Length, Rectangle, Size, Theme};

use crate::tokens::{palette, LIGHT_PALETTE, TOKENS};
use crate::variants::ButtonVariant;

use super::control_button::{control_button_content, ControlButton, ControlStatus};
use super::motion::{self, Tween};

/// Creates a 40 × 22 switch with an 18px knob and a 40px-high hit target.
/// The caller supplies the toggle message and an adjacent descriptive label.
pub fn switch<'a, Message: Clone + 'a>(enabled: bool) -> ControlButton<'a, Message> {
    let status = ControlStatus::default();
    control_button_content(
        {
            let status = status.clone();
            move |_| SwitchVisual::new(enabled, status.clone()).into()
        },
        ButtonVariant::Text,
    )
    .status_channel(status)
    .padding([9, 0])
    .width(Length::Fixed(40.0))
    .min_height(40.0)
    .style(style)
}

fn style(_theme: &Theme, _variant: ButtonVariant, _status: button::Status) -> button::Style {
    button::Style {
        border: Border::default().rounded(TOKENS.radii.md),
        ..button::Style::default()
    }
}

/// The switch's resolved track and knob colors for one frame.
#[derive(Debug, Clone, Copy)]
struct SwitchColors {
    track: iced::Color,
    knob: iced::Color,
}

impl SwitchColors {
    fn lerp(from: &Self, to: &Self, progress: f32) -> Self {
        Self {
            track: motion::lerp_color(from.track, to.track, progress),
            knob: motion::lerp_color(from.knob, to.knob, progress),
        }
    }
}

/// Resolves track and knob colors for a status and on-fraction in `[0, 1]`.
fn switch_colors(theme: &Theme, status: button::Status, on: f32) -> SwitchColors {
    let palette = palette(theme);
    let colors = palette.colors;
    let dark = colors.background != LIGHT_PALETTE.colors.background;
    let track = if status == button::Status::Disabled {
        colors.control
    } else {
        let off = if status == button::Status::Hovered {
            colors.controlHover
        } else {
            colors.surfaceContainerHighest
        };
        motion::lerp_color(off, colors.primary, on)
    };
    let knob = if status == button::Status::Disabled {
        palette.text.muted
    } else {
        // Off keeps the dark-mode onPrimary knob; light mode uses the readable
        // onControl adaptation. On is always onPrimary.
        motion::lerp_color(
            if dark {
                colors.onPrimary
            } else {
                colors.onControl
            },
            colors.onPrimary,
            on,
        )
    };
    SwitchColors { track, knob }
}

/// The non-interactive 40×22 switch visual: track fill plus the 18px knob.
/// Interaction status arrives through the [`ControlStatus`] channel the
/// enclosing control publishes; `on` arrives through `diff`.
pub(crate) struct SwitchVisual {
    on: bool,
    status: ControlStatus,
}

impl SwitchVisual {
    pub(crate) fn new(on: bool, status: ControlStatus) -> Self {
        Self { on, status }
    }
}

#[derive(Debug)]
pub(crate) struct SwitchVisualState {
    mounted: bool,
    on: bool,
    status: button::Status,
    on_tween: Option<Tween>,
    status_tween: Option<Tween>,
    status_from: Option<SwitchColors>,
    /// The colors the last draw presented; retargets start here.
    displayed: Cell<Option<SwitchColors>>,
}

impl Default for SwitchVisualState {
    fn default() -> Self {
        Self {
            mounted: false,
            on: false,
            status: button::Status::Active,
            on_tween: None,
            status_tween: None,
            status_from: None,
            displayed: Cell::new(None),
        }
    }
}

impl SwitchVisualState {
    pub(crate) fn motion_active(&self) -> bool {
        self.on_tween.is_some() || self.status_tween.is_some()
    }

    /// The eased on-fraction at `now`; settled at the logical state.
    fn on_fraction(&self, now: Instant) -> f32 {
        self.on_tween
            .map_or_else(|| self.on as u8 as f32, |tween| tween.eased(now))
    }
}

const TRACK: Size = Size::new(40.0, 22.0);
const KNOB: f32 = 18.0;
const KNOB_INSET: f32 = 2.0;

impl<Message, Renderer> Widget<Message, Theme, Renderer> for SwitchVisual
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<SwitchVisualState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(SwitchVisualState::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        if !motion::enabled() {
            let state = tree.state.downcast_mut::<SwitchVisualState>();
            state.on_tween = None;
            state.status_tween = None;
            state.status_from = None;
        }
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(TRACK.width), Length::Fixed(TRACK.height))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, TRACK.width, TRACK.height)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.custom(
            None,
            layout.bounds(),
            tree.state.downcast_mut::<SwitchVisualState>(),
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SwitchVisualState>();
        let status = self.status.get();
        if !state.mounted {
            // First mount presents the logical state directly.
            state.mounted = true;
            state.on = self.on;
            state.status = status;
        }
        if !motion::enabled() {
            state.on = self.on;
            state.status = status;
            state.on_tween = None;
            state.status_tween = None;
            state.status_from = None;
            return;
        }
        if self.on != state.on {
            // Knob travel retargets from the currently displayed fraction.
            let from = state.on_fraction(Instant::now());
            state.on = self.on;
            state.on_tween = Some(Tween::new(
                from,
                self.on as u8 as f32,
                Instant::now(),
                TOKENS.durations.ms150,
            ));
            shell.request_redraw();
        }
        if status != state.status {
            state.status = status;
            state.status_from = state.displayed.get();
            state.status_tween = state
                .status_from
                .map(|_| Tween::new(0.0, 1.0, Instant::now(), TOKENS.durations.ms150));
            if state.status_tween.is_some() {
                shell.request_redraw();
            }
        }
        motion::tick_draw(&mut state.on_tween, event, shell);
        motion::tick_draw(&mut state.status_tween, event, shell);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<SwitchVisualState>();
        let now = Instant::now();
        let on = state.on_fraction(now);
        let mut colors = switch_colors(theme, state.status, on);
        if let (Some(tween), Some(from)) = (state.status_tween, state.status_from) {
            colors = SwitchColors::lerp(&from, &colors, tween.eased(now));
        }
        state.displayed.set(Some(colors));

        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border::default().rounded(TOKENS.radii.full),
                ..renderer::Quad::default()
            },
            iced::Background::Color(colors.track),
        );
        let travel = (TRACK.width - KNOB - 2.0 * KNOB_INSET) * on;
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: bounds.x + KNOB_INSET + travel,
                    y: bounds.y + KNOB_INSET,
                    width: KNOB,
                    height: KNOB,
                },
                border: Border::default().rounded(TOKENS.radii.full),
                ..renderer::Quad::default()
            },
            iced::Background::Color(colors.knob),
        );
    }
}

impl<'a, Message, Renderer> From<SwitchVisual> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: 'a + iced::advanced::Renderer,
{
    fn from(visual: SwitchVisual) -> Self {
        Element::new(visual)
    }
}
