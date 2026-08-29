//! Skeleton loading placeholder with an optional breathing pulse animation.
//!
//! The pulse animation is implemented as a style-closure background alpha modulation
//! rather than a custom widget: swapping `container::Style::background` is a
//! draw-only change, so the animation never invalidates layout. This matters
//! because iced_winit treats repeated layout invalidation as a runaway-layout
//! signal and breaks the event loop after more than three consecutive invalidating
//! frames. The phase driver (see `State::skeleton_phase` in src-iced) therefore
//! only needs to request a redraw; the geometry of the block is identical at
//! every phase and under reduced motion.
//!
//! Visual feedback drove two revisions: the initial travelling white sweep was
//! too flashy, and the follow-up alpha-breathing pulse was imperceptible —
//! modulating the alpha of near-black surface colors against a near-black
//! window changes almost nothing. The pulse therefore interpolates between two
//! opaque surface tones (Low ↔ High), which reads clearly on the dark theme
//! while staying strictly tone-on-tone.

use iced::border::Radius;
use iced::widget::{container, space, Container};
use iced::{Background, Border, Color, Length, Shadow, Theme};

use crate::theme;
use crate::tokens::TOKENS;
use crate::variants::SurfaceVariant;

/// Interpolates between two surface tones for the breathing pulse at a
/// normalized `phase` in `[0.0, 1.0]`.
///
/// Smooth cosine curve: phase 0.0 -> `dim`, 0.5 -> `bright`, 1.0 -> `dim`.
/// The function is total: any non-finite `phase` (e.g. `NaN`, `INFINITY`)
/// returns `dim`.
#[must_use]
pub fn pulse_color(phase: f32, dim: Color, bright: Color) -> Color {
    let factor = if phase.is_finite() {
        0.5 - 0.5 * (phase * std::f32::consts::TAU).cos()
    } else {
        0.0
    };
    let lerp = |a: f32, b: f32| a + (b - a) * factor;
    Color {
        r: lerp(dim.r, bright.r),
        g: lerp(dim.g, bright.g),
        b: lerp(dim.b, bright.b),
        a: lerp(dim.a, bright.a),
    }
}

/// Builds a breathing placeholder block sized to `width` × `height`.
///
/// When `reduced_motion` is set or `phase` is non-finite this renders the same
/// static Elevated surface as the pre-animation skeleton boxes. Otherwise the
/// Elevated background color alpha is modulated by [`pulse_scale`].
pub fn skeleton_block<'a, Message: 'a>(
    width: impl Into<Length>,
    height: impl Into<Length>,
    phase: f32,
    reduced_motion: bool,
) -> Container<'a, Message> {
    container(space::horizontal())
        .width(width)
        .height(height)
        .style(move |theme: &Theme| skeleton_style(theme, phase, reduced_motion))
}

/// Builds a pulsing placeholder panel sized to `width` × `height` with a custom `base` color and `radius`.
///
/// When `reduced_motion` is set or `phase` is non-finite this renders a static flat panel with the
/// unmodified `base` color. Otherwise the background alpha is modulated by [`pulse_scale`].
pub fn skeleton_panel<'a, Message: 'a>(
    width: impl Into<Length>,
    height: impl Into<Length>,
    base: Color,
    radius: iced::border::Radius,
    phase: f32,
    reduced_motion: bool,
) -> Container<'a, Message> {
    container(space::horizontal())
        .width(width)
        .height(height)
        .style(move |_theme: &Theme| skeleton_panel_style(base, radius, phase, reduced_motion))
}

/// Resolves the skeleton block container style: static Elevated surface under reduced
/// motion or non-finite phase, tone-breathing Elevated background otherwise.
fn skeleton_style(theme: &Theme, phase: f32, reduced_motion: bool) -> container::Style {
    let mut style = theme::surface_variant(theme, SurfaceVariant::Elevated);
    if reduced_motion || !phase.is_finite() {
        return style;
    }

    style.background = Some(Background::Color(pulse_color(
        phase,
        TOKENS.colors.surfaceContainerLow,
        TOKENS.colors.surfaceContainerHigh,
    )));
    style
}

/// Resolves the skeleton panel container style: static base color under reduced
/// motion or non-finite phase, breathing between `base` and `surfaceContainerHigh` otherwise.
fn skeleton_panel_style(
    base: Color,
    radius: Radius,
    phase: f32,
    reduced_motion: bool,
) -> container::Style {
    let background_color = if reduced_motion || !phase.is_finite() {
        base
    } else {
        pulse_color(phase, base, TOKENS.colors.surfaceContainerHigh)
    };

    container::Style {
        background: Some(Background::Color(background_color)),
        border: Border {
            radius,
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        shadow: Shadow::default(),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-6;

    const DIM: Color = Color::from_rgb(0.1, 0.2, 0.3);
    const BRIGHT: Color = Color::from_rgb(0.4, 0.5, 0.6);
    fn assert_color_near(actual: Color, expected: Color) {
        assert!(
            (actual.r - expected.r).abs() < EPSILON
                && (actual.g - expected.g).abs() < EPSILON
                && (actual.b - expected.b).abs() < EPSILON
                && (actual.a - expected.a).abs() < EPSILON,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn pulse_color_endpoints_match_contract() {
        assert_eq!(pulse_color(0.0, DIM, BRIGHT), DIM);
        assert_eq!(pulse_color(0.5, DIM, BRIGHT), BRIGHT);
        assert_eq!(pulse_color(1.0, DIM, BRIGHT), DIM);
    }

    #[test]
    fn pulse_color_mid_quarter_is_channelwise_halfway() {
        // cos(π/2) = 0 → factor 0.5.
        let mid = pulse_color(0.25, DIM, BRIGHT);
        assert!((mid.r - 0.25).abs() < EPSILON);
        assert!((mid.g - 0.35).abs() < EPSILON);
        assert!((mid.b - 0.45).abs() < EPSILON);
    }

    #[test]
    fn pulse_color_is_monotonic_rise_then_fall() {
        let mut prev = pulse_color(0.0, DIM, BRIGHT).r;
        for step in 1..=500 {
            let phase = step as f32 / 1000.0;
            let current = pulse_color(phase, DIM, BRIGHT).r;
            assert!(
                current > prev,
                "expected rising pulse at phase {phase}: {current} <= {prev}"
            );
            prev = current;
        }
        for step in 501..=1000 {
            let phase = step as f32 / 1000.0;
            let current = pulse_color(phase, DIM, BRIGHT).r;
            assert!(
                current < prev,
                "expected falling pulse at phase {phase}: {current} >= {prev}"
            );
            prev = current;
        }
    }

    #[test]
    fn pulse_color_is_total_on_non_finite_inputs() {
        for non_finite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(pulse_color(non_finite, DIM, BRIGHT), DIM);
        }
    }

    #[test]
    fn reduced_motion_renders_the_static_elevated_surface() {
        let theme = crate::theme::theme();
        let elevated = theme::surface_variant(&theme, SurfaceVariant::Elevated);

        for phase in [0.0, 0.25, 0.5, 0.75, 1.0, f32::NAN, f32::INFINITY] {
            let style = skeleton_style(&theme, phase, true);
            assert_eq!(style.background, elevated.background);
            assert_eq!(style.border, elevated.border);
            assert_eq!(style.shadow, elevated.shadow);
        }
    }

    #[test]
    fn non_finite_phase_falls_back_to_the_static_surface() {
        let theme = crate::theme::theme();
        let elevated = theme::surface_variant(&theme, SurfaceVariant::Elevated);

        for non_finite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let style = skeleton_style(&theme, non_finite, false);
            assert_eq!(style.background, elevated.background);
            assert_eq!(style.border, elevated.border);
            assert_eq!(style.shadow, elevated.shadow);
        }
    }

    #[test]
    fn active_skeleton_block_breathes_between_surface_tones() {
        let theme = crate::theme::theme();
        let elevated = theme::surface_variant(&theme, SurfaceVariant::Elevated);

        let style_mid = skeleton_style(&theme, 0.5, false);
        let Some(Background::Color(mid_color)) = style_mid.background else {
            panic!("expected background color");
        };
        assert_color_near(mid_color, TOKENS.colors.surfaceContainerHigh);
        assert_eq!(style_mid.border, elevated.border);
        assert_eq!(style_mid.shadow, elevated.shadow);

        let style_start = skeleton_style(&theme, 0.0, false);
        assert_eq!(
            style_start.background,
            Some(Background::Color(TOKENS.colors.surfaceContainerLow))
        );
        assert_eq!(style_start.border, elevated.border);
        assert_eq!(style_start.shadow, elevated.shadow);
    }
    #[test]
    fn reduced_motion_panel_renders_static_base() {
        let base = Color::from_rgb(0.2, 0.3, 0.4);
        let radius = Radius::from(8.0);

        for phase in [0.0, 0.25, 0.5, 0.75, 1.0, f32::NAN, f32::INFINITY] {
            let style = skeleton_panel_style(base, radius, phase, true);
            assert_eq!(style.background, Some(Background::Color(base)));
            assert_eq!(style.border.radius, radius);
            assert_eq!(style.border.color, Color::TRANSPARENT);
            assert_eq!(style.border.width, 0.0);
            assert_eq!(style.shadow, Shadow::default());
        }
    }

    #[test]
    fn active_panel_breathes_from_base_to_surface_high() {
        let base = Color::from_rgba(0.2, 0.3, 0.4, 0.8);
        let radius = Radius::from(12.0);

        let style_start = skeleton_panel_style(base, radius, 0.0, false);
        assert_eq!(style_start.background, Some(Background::Color(base)));
        assert_eq!(style_start.border.radius, radius);

        let style_mid = skeleton_panel_style(base, radius, 0.5, false);
        let Some(Background::Color(mid_color)) = style_mid.background else {
            panic!("expected background color");
        };
        assert_color_near(mid_color, TOKENS.colors.surfaceContainerHigh);
        assert_eq!(style_mid.border.radius, radius);
    }
    #[test]
    fn non_finite_phase_panel_falls_back_to_static_base() {
        let base = Color::from_rgb(0.5, 0.5, 0.5);
        let radius = Radius::from(4.0);

        for non_finite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let style = skeleton_panel_style(base, radius, non_finite, false);
            assert_eq!(style.background, Some(Background::Color(base)));
            assert_eq!(style.border.radius, radius);
        }
    }
}
