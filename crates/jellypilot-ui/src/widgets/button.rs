//! JellyPilot button catalog styles.

use iced::widget::button;
use iced::{Background, Border, Color, Theme};

use crate::tokens::TOKENS;
use crate::variants::ButtonVariant;

/// Resolves a button variant and interaction status to an iced style.
pub fn style(_theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let colors = TOKENS.colors;
    let (mut background, mut text_color, mut border_color, border_width, mut shadow) = match variant
    {
        ButtonVariant::Primary => (
            Some(colors.primary),
            colors.onPrimary,
            Color::TRANSPARENT,
            0.0,
            TOKENS.shadows.none.iced(),
        ),
        ButtonVariant::Secondary => (
            Some(colors.secondaryContainer),
            colors.onSecondaryContainer,
            colors.outlineVariant,
            1.0,
            TOKENS.shadows.md.iced(),
        ),
        ButtonVariant::Outlined => (
            None,
            colors.onSurface,
            colors.outline,
            1.0,
            TOKENS.shadows.none.iced(),
        ),
        ButtonVariant::Text => (
            None,
            colors.secondary,
            Color::TRANSPARENT,
            0.0,
            TOKENS.shadows.none.iced(),
        ),
        ButtonVariant::Icon => (
            None,
            colors.onSurfaceVariant,
            Color::TRANSPARENT,
            0.0,
            TOKENS.shadows.none.iced(),
        ),
    };

    match status {
        button::Status::Active | button::Status::Pressed => {}
        button::Status::Hovered => match variant {
            ButtonVariant::Primary => {
                background = background.map(|color| brightness(color, 1.1));
            }
            ButtonVariant::Secondary => border_color = colors.outline,
            ButtonVariant::Outlined => {
                background = Some(with_alpha(colors.primary, 0.05));
                border_color = colors.primary;
            }
            ButtonVariant::Text => background = Some(with_alpha(colors.secondary, 0.1)),
            ButtonVariant::Icon => {
                background = Some(with_alpha(colors.primary, 0.1));
                text_color = colors.onSurface;
            }
        },
        button::Status::Disabled => {
            background = background.map(|color| scale_alpha(color, 0.5));
            text_color = scale_alpha(text_color, 0.5);
            border_color = scale_alpha(border_color, 0.5);
            shadow.color = scale_alpha(shadow.color, 0.5);
        }
    }

    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border {
            radius: TOKENS.radii.x2l.into(),
            color: border_color,
            width: border_width,
        },
        shadow,
        ..button::Style::default()
    }
}

fn brightness(color: Color, factor: f32) -> Color {
    Color {
        r: (color.r * factor).min(1.0),
        g: (color.g * factor).min(1.0),
        b: (color.b * factor).min(1.0),
        ..color
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

fn scale_alpha(color: Color, factor: f32) -> Color {
    Color {
        a: color.a * factor,
        ..color
    }
}

#[cfg(test)]
mod tests {
    use iced::Color;

    use super::brightness;

    #[test]
    fn brightness_matches_primary_css_hover_filter() {
        let hovered = brightness(Color::from_rgb8(0x4f, 0x46, 0xe5), 1.1);

        assert_eq!(
            hovered,
            Color::from_rgb(
                (0x4f as f32 / 255.0 * 1.1).min(1.0),
                (0x46 as f32 / 255.0 * 1.1).min(1.0),
                (0xe5 as f32 / 255.0 * 1.1).min(1.0),
            )
        );
    }

    #[test]
    fn brightness_clamps_channels_to_one() {
        assert_eq!(brightness(Color::WHITE, 1.1), Color::WHITE);
    }
}
