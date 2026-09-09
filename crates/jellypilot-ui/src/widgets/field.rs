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
    // Filled fields rest on a subtle container surface with a quiet structural
    // edge. The 1px border is functional: primary while focused, error while
    // invalid. Disabled fields keep an opaque control fill and muted content.
    let FieldVariant::Filled = variant;
    let (background, mut border_color, mut border_width) = match status {
        text_input::Status::Focused { .. } => (colors.controlHover, colors.primary, 1.0),
        _ => (colors.surfaceContainerHigh, colors.borderSubtle, 1.0),
    };

    if is_error && !disabled {
        border_color = colors.error;
        border_width = 1.0;
    }

    text_input::Style {
        background: Background::Color(if disabled { colors.control } else { background }),
        border: Border {
            smoothing: super::container::SURFACE_SMOOTHING,
            radius: TOKENS.radii.xl.into(),
            color: if disabled {
                Color::TRANSPARENT
            } else {
                border_color
            },
            width: if disabled { 0.0 } else { border_width },
        },
        placeholder: palette.text.muted,
        value: if disabled {
            palette.text.muted
        } else {
            colors.onSurface
        },
        selection: with_alpha(colors.secondary, 0.3),
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

#[cfg(test)]
mod tests {
    use iced::border::Radius;
    use iced::widget::text_input::Status;

    use super::*;
    use crate::tokens::DARK_PALETTE;

    #[test]
    fn filled_field_rest_uses_surface_container_high_and_border_subtle() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for status in [Status::Active, Status::Hovered] {
            let style = style(&theme, FieldVariant::Filled, status);
            assert_eq!(
                style.background,
                Background::Color(DARK_PALETTE.colors.surfaceContainerHigh)
            );
            assert_eq!(style.border.radius, Radius::from(TOKENS.radii.xl));
            assert_eq!(style.border.width, 1.0);
            assert_eq!(style.border.color, DARK_PALETTE.colors.borderSubtle);
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
        assert_eq!(focused.border.radius, Radius::from(TOKENS.radii.xl));
    }

    #[test]
    fn error_field_draws_the_error_border() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let err_style = error_style(&theme, FieldVariant::Filled, Status::Active);
        assert_eq!(err_style.border.width, 1.0);
        assert_eq!(err_style.border.color, DARK_PALETTE.colors.error);
        assert_eq!(err_style.border.radius, Radius::from(TOKENS.radii.xl));

        let disabled = error_style(&theme, FieldVariant::Filled, Status::Disabled);
        assert_eq!(disabled.border.width, 0.0);
        assert_eq!(
            disabled.background,
            Background::Color(DARK_PALETTE.colors.control)
        );
        assert_eq!(disabled.value, DARK_PALETTE.text.muted);
    }

    #[test]
    fn disabled_field_uses_control_fill_and_muted_value() {
        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let disabled = style(&theme, FieldVariant::Filled, Status::Disabled);
        assert_eq!(
            disabled.background,
            Background::Color(DARK_PALETTE.colors.control)
        );
        assert_eq!(disabled.border.width, 0.0);
        assert_eq!(disabled.value, DARK_PALETTE.text.muted);
    }
}
