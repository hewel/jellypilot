//! JellyPilot text-input catalog styles.

use iced::widget::text_input;
use iced::{Background, Border, Color, Theme};

use crate::tokens::TOKENS;
use crate::variants::FieldVariant;

/// Resolves a normal text-input variant and interaction status.
pub fn style(
    theme: &Theme,
    variant: FieldVariant,
    status: text_input::Status,
) -> text_input::Style {
    resolve(theme, variant, status, false)
}

/// Resolves an invalid text-input variant and interaction status.
pub fn error_style(
    theme: &Theme,
    variant: FieldVariant,
    status: text_input::Status,
) -> text_input::Style {
    resolve(theme, variant, status, true)
}

fn resolve(
    _theme: &Theme,
    variant: FieldVariant,
    status: text_input::Status,
    is_error: bool,
) -> text_input::Style {
    let colors = TOKENS.colors;
    let disabled = matches!(status, text_input::Status::Disabled);
    let (background, mut border_color) = match (variant, status) {
        (FieldVariant::Filled, text_input::Status::Hovered) => (
            with_alpha(colors.surfaceContainerHighest, 0.4),
            with_alpha(colors.secondary, 0.4),
        ),
        (FieldVariant::Filled, text_input::Status::Focused { .. }) => (
            with_alpha(colors.surfaceContainerHighest, 0.6),
            colors.secondary,
        ),
        (FieldVariant::Filled, _) => (
            with_alpha(colors.surfaceContainerHighest, 0.3),
            with_alpha(colors.outlineVariant, 0.8),
        ),
    };

    if is_error && !disabled {
        border_color = colors.error;
    }

    text_input::Style {
        background: Background::Color(if disabled {
            scale_alpha(background, 0.5)
        } else {
            background
        }),
        border: Border {
            radius: TOKENS.radii.lg.into(),
            color: if disabled {
                scale_alpha(border_color, 0.5)
            } else {
                border_color
            },
            width: 1.0,
        },
        icon: colors.onSurfaceVariant,
        placeholder: with_alpha(colors.onSurfaceVariant, 0.5),
        value: if disabled {
            scale_alpha(colors.onSurface, 0.5)
        } else {
            colors.onSurface
        },
        selection: with_alpha(colors.secondary, 0.3),
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
    use iced::border::Radius;
    use iced::widget::text_input::Status;

    use super::*;

    #[test]
    fn field_style_uses_lg_radius_token() {
        let theme = crate::theme::theme();
        let style = style(&theme, FieldVariant::Filled, Status::Active);
        assert_eq!(style.border.radius, Radius::from(TOKENS.radii.lg));
        assert_eq!(style.border.width, 1.0);

        let err_style = error_style(&theme, FieldVariant::Filled, Status::Active);
        assert_eq!(err_style.border.radius, Radius::from(TOKENS.radii.lg));
        assert_eq!(err_style.border.width, 1.0);
    }
}
