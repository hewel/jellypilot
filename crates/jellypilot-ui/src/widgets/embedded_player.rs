//! Cinema chrome, scoped to the embedded video surface.

use iced::widget::{button, container, slider};
use iced::{gradient, Background, Border, Color, Degrees, Theme};

use crate::tokens::{DARK_PALETTE, TOKENS};
use crate::variants::ButtonVariant;

const CARD_FILL: Color = Color::from_rgba(24.0 / 255.0, 24.0 / 255.0, 30.0 / 255.0, 0.82);
// Preblend white/12% against the reference card, per the temporary border policy.
const CARD_EDGE: Color = Color::from_rgb(
    0.12 + 0.88 * 24.0 / 255.0,
    0.12 + 0.88 * 24.0 / 255.0,
    0.12 + 0.88 * 30.0 / 255.0,
);
const RAIL: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.18);
const RAIL_WIDTH: f32 = 4.0;
const TIMELINE_KNOB_RADIUS: f32 = 7.0;
const TIMELINE_KNOB_BORDER: f32 = 2.0;
const VOLUME_KNOB_RADIUS: f32 = 4.0;
const FRAMED_FILL: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.6);
const FRAMED_EDGE: Color = Color::from_rgb(
    0.15 + 0.85 * 0.4 * 24.0 / 255.0,
    0.15 + 0.85 * 0.4 * 24.0 / 255.0,
    0.15 + 0.85 * 0.4 * 30.0 / 255.0,
);
/// Reuses the app's Charcoal theme for cinema chrome in either browser theme.
pub fn theme() -> Theme {
    static THEME: std::sync::LazyLock<Theme> =
        std::sync::LazyLock::new(|| crate::theme::theme(crate::theme::ThemeMode::Dark));
    THEME.clone()
}

/// Floating transport card. Translucency without blur is scoped to this card.
pub fn surface(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(CARD_FILL)),
        text_color: Some(DARK_PALETTE.text.body),
        border: Border {
            radius: TOKENS.radii.x2l.into(),
            smoothing: super::container::SURFACE_SMOOTHING,
            color: CARD_EDGE,
            width: 1.0,
        },
        shadow: DARK_PALETTE.shadows.raised_high.iced(),
        ..container::Style::default()
    }
}

/// Feedback bubbles and player menus retain their opaque cinema surface.
pub fn popover(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(DARK_PALETTE.colors.surfaceContainer)),
        border: Border {
            radius: TOKENS.radii.xl.into(),
            color: DARK_PALETTE.colors.outlineVariant,
            ..surface(theme).border
        },
        ..surface(theme)
    }
}

/// Quiet white transport actions; the primary play/pause action stays solid.
pub fn control(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    if variant != ButtonVariant::Primary {
        style.border.width = 0.0;
        style.border.color = Color::TRANSPARENT;
        style.text_color = if status == button::Status::Disabled {
            DARK_PALETTE.text.muted
        } else {
            Color::WHITE
        };
        if matches!(status, button::Status::Active | button::Status::Disabled)
            && variant != ButtonVariant::TonalActive
        {
            style.background = None;
        }
    }
    style
}

/// Volume feedback changes only the glyph tint, never its hit-area background.
pub fn volume_control(
    theme: &Theme,
    variant: ButtonVariant,
    status: button::Status,
) -> button::Style {
    let mut style = control(theme, variant, status);
    style.background = None;
    style.text_color = match status {
        button::Status::Hovered | button::Status::Pressed => Color::WHITE,
        button::Status::Disabled => DARK_PALETTE.text.muted,
        button::Status::Active => Color::WHITE.scale_alpha(0.85),
    };
    style
}

/// Framed dark output and navigation controls, independent of quiet transport.
pub fn framed_control(
    theme: &Theme,
    variant: ButtonVariant,
    status: button::Status,
) -> button::Style {
    let mut style = super::button::style(theme, variant, status);
    if variant != ButtonVariant::Primary && status != button::Status::Disabled {
        style.text_color = Color::WHITE;
        if status == button::Status::Active && variant != ButtonVariant::TonalActive {
            style.background = Some(Background::Color(FRAMED_FILL));
            style.border.color = FRAMED_EDGE;
        }
    }
    style
}

/// Four-pixel timeline with a persistent accent knob and white outline.
pub fn timeline(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, status);
    style.rail.width = RAIL_WIDTH;
    style.rail.backgrounds = (DARK_PALETTE.colors.primary.into(), RAIL.into());
    style.handle.shape = slider::HandleShape::Circle {
        radius: TIMELINE_KNOB_RADIUS,
    };
    style.handle.background = DARK_PALETTE.colors.primary.into();
    style.handle.border_color = Color::WHITE;
    style.handle.border_width = TIMELINE_KNOB_BORDER;
    style
}

/// White volume rail with a small knob revealed only on hover or drag.
pub fn volume(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, status);
    style.rail.width = RAIL_WIDTH;
    style.rail.backgrounds = (Color::WHITE.into(), RAIL.into());
    style.handle.shape = slider::HandleShape::Circle {
        radius: if status == slider::Status::Active {
            0.0
        } else {
            VOLUME_KNOB_RADIUS
        },
    };
    style.handle.background = Color::WHITE.into();
    style
}

/// Quiet divider between volume and fullscreen controls.
pub fn separator(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.scale_alpha(0.15).into()),
        ..container::Style::default()
    }
}

/// Video-edge legibility gradients; the view owns their height and visibility.
pub fn scrim(top: bool) -> container::Style {
    let gradient = if top {
        gradient::Linear::new(Degrees(180.0))
            .add_stop(0.0, Color::BLACK.scale_alpha(0.55))
            .add_stop(0.55, Color::BLACK.scale_alpha(0.25))
            .add_stop(1.0, Color::TRANSPARENT)
    } else {
        gradient::Linear::new(Degrees(180.0))
            .add_stop(0.0, Color::TRANSPARENT)
            .add_stop(0.25, Color::BLACK.scale_alpha(0.12))
            .add_stop(0.5, Color::BLACK.scale_alpha(0.32))
            .add_stop(0.75, Color::BLACK.scale_alpha(0.58))
            .add_stop(1.0, Color::BLACK.scale_alpha(0.8))
    };
    container::Style {
        background: Some(Background::Gradient(gradient.into())),
        ..container::Style::default()
    }
}

/// Non-interactive slider appearance while a playback command is settling.
pub fn unavailable_slider(theme: &Theme, _status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, slider::Status::Active);
    style.rail.backgrounds = (
        DARK_PALETTE.colors.control.into(),
        DARK_PALETTE.colors.control.into(),
    );
    style.handle.background = DARK_PALETTE.text.muted.into();
    style
}
