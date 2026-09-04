//! JellyPilot text-input catalog styles.

use iced::widget::text_input;
use iced::{Background, Border, Color, Theme};

use crate::tokens::{palette, TOKENS};
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
    theme: &Theme,
    variant: FieldVariant,
    status: text_input::Status,
    is_error: bool,
) -> text_input::Style {
    let palette = palette(theme);
    let colors = palette.colors;
    let disabled = matches!(status, text_input::Status::Disabled);
    // Opaque fill with no idle border. The 1px border appears only as a
    // functional signal: primary while focused (accessibility exemption), or
    // error while the field is invalid.
    let FieldVariant::Filled = variant;
    let (background, mut border_color, mut border_width) = match status {
        text_input::Status::Focused { .. } => (colors.controlHover, colors.primary, 1.0),
        _ => (colors.control, Color::TRANSPARENT, 0.0),
    };

    if is_error && !disabled {
        border_color = colors.error;
        border_width = 1.0;
    }

    text_input::Style {
        background: Background::Color(if disabled {
            scale_alpha(background, 0.5)
        } else {
            background
        }),
        border: Border {
            radius: TOKENS.radii.md.into(),
            color: if disabled {
                scale_alpha(border_color, 0.5)
            } else {
                border_color
            },
            width: border_width,
        },
        icon: colors.onControl,
        placeholder: palette.text.muted,
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
    use crate::tokens::DARK_PALETTE;

    #[test]
    fn field_style_is_opaque_borderless_and_uses_md_radius() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for status in [Status::Active, Status::Hovered] {
            let style = style(&theme, FieldVariant::Filled, status);
            assert_eq!(
                style.background,
                Background::Color(DARK_PALETTE.colors.control)
            );
            assert_eq!(style.icon, DARK_PALETTE.colors.onControl);
            assert_eq!(style.border.radius, Radius::from(TOKENS.radii.md));
            assert_eq!(style.border.width, 0.0);
        }
    }

    #[test]
    fn focused_field_draws_the_primary_focus_border() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let focused = style(
            &theme,
            FieldVariant::Filled,
            Status::Focused { is_hovered: false },
        );
        assert_eq!(
            focused.background,
            Background::Color(DARK_PALETTE.colors.controlHover)
        );
        assert_eq!(focused.border.width, 1.0);
        assert_eq!(focused.border.color, DARK_PALETTE.colors.primary);
        assert_eq!(focused.border.radius, Radius::from(TOKENS.radii.md));
    }

    #[test]
    fn error_field_draws_the_error_border() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let err_style = error_style(&theme, FieldVariant::Filled, Status::Active);
        assert_eq!(err_style.border.width, 1.0);
        assert_eq!(err_style.border.color, DARK_PALETTE.colors.error);
        assert_eq!(err_style.border.radius, Radius::from(TOKENS.radii.md));

        let disabled = error_style(&theme, FieldVariant::Filled, Status::Disabled);
        assert_eq!(disabled.border.width, 0.0);
    }
}
