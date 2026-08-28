//! JellyPilot status badge styles.

use iced::widget::container;
use iced::{Background, Border, Color, Theme};

use crate::tokens::TOKENS;
use crate::variants::BadgeVariant;

/// Resolves a status badge variant to an iced container style.
pub fn style(_theme: &Theme, variant: BadgeVariant) -> container::Style {
    let colors = TOKENS.colors;
    let (background, border_color, text_color) = match variant {
        BadgeVariant::Success => (
            with_alpha(colors.tertiaryContainer, 0.2),
            with_alpha(colors.tertiary, 0.3),
            colors.tertiary,
        ),
        BadgeVariant::Warning => (
            with_alpha(colors.warningContainer, 0.2),
            with_alpha(colors.warning, 0.3),
            colors.warning,
        ),
        BadgeVariant::Neutral => (
            with_alpha(colors.surfaceContainerHighest, 0.3),
            with_alpha(colors.outlineVariant, 0.6),
            colors.onSurfaceVariant,
        ),
    };

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(text_color),
        border: Border {
            radius: TOKENS.radii.full.into(),
            color: border_color,
            width: 1.0,
        },
        ..container::Style::default()
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}
