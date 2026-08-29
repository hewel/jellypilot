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

/// Resolves the borderless poster-stream card button style for grid and row poster cards.
///
/// In the default/active state, there is no card chrome (transparent background, zero border,
/// no shadow) so the poster and metadata sit directly on the page surface. On hover or press,
/// a subtle brightness lift appears without an outline ring or shadow elevation.
pub fn poster_card_style(_theme: &Theme, status: button::Status) -> button::Style {
    let colors = TOKENS.colors;
    let (background, border_color, border_width, shadow) = match status {
        button::Status::Active => (None, Color::TRANSPARENT, 0.0, TOKENS.shadows.none.iced()),
        button::Status::Hovered => (
            Some(with_alpha(Color::WHITE, 0.05)),
            Color::TRANSPARENT,
            0.0,
            TOKENS.shadows.none.iced(),
        ),
        button::Status::Pressed => (
            Some(with_alpha(Color::WHITE, 0.08)),
            Color::TRANSPARENT,
            0.0,
            TOKENS.shadows.none.iced(),
        ),
        button::Status::Disabled => (None, Color::TRANSPARENT, 0.0, TOKENS.shadows.none.iced()),
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: colors.onSurface,
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

    #[test]
    fn text_button_variant_never_draws_border_in_any_status() {
        use crate::variants::ButtonVariant;
        use iced::widget::button::Status;

        let theme = crate::theme::theme();
        for status in [
            Status::Active,
            Status::Hovered,
            Status::Pressed,
            Status::Disabled,
        ] {
            let style = super::style(&theme, ButtonVariant::Text, status);
            assert_eq!(
                style.border.width, 0.0,
                "Text button variant must have zero border width in status {status:?}"
            );
            assert_eq!(
                style.border.color,
                Color::TRANSPARENT,
                "Text button variant must have transparent border color in status {status:?}"
            );
        }
    }

    #[test]
    fn poster_card_style_active_has_no_border_or_background_or_shadow() {
        let theme = crate::theme::theme();
        let style = super::poster_card_style(&theme, iced::widget::button::Status::Active);
        assert_eq!(style.border.width, 0.0);
        assert_eq!(style.border.color, Color::TRANSPARENT);
        assert!(style.background.is_none());
        assert_eq!(style.shadow, crate::tokens::TOKENS.shadows.none.iced());
    }
    #[test]
    fn poster_card_style_hovered_has_brightness_lift_and_no_ring_or_shadow() {
        let theme = crate::theme::theme();
        let style = super::poster_card_style(&theme, iced::widget::button::Status::Hovered);
        assert_eq!(style.border.width, 0.0);
        assert_eq!(style.border.color, Color::TRANSPARENT);
        assert_eq!(
            style.background,
            Some(iced::Background::Color(super::with_alpha(
                Color::WHITE,
                0.05
            )))
        );
        assert_eq!(style.shadow, crate::tokens::TOKENS.shadows.none.iced());
    }

    #[test]
    fn poster_card_style_pressed_has_stronger_brightness_lift_and_no_ring_or_shadow() {
        let theme = crate::theme::theme();
        let style = super::poster_card_style(&theme, iced::widget::button::Status::Pressed);
        assert_eq!(style.border.width, 0.0);
        assert_eq!(style.border.color, Color::TRANSPARENT);
        assert_eq!(
            style.background,
            Some(iced::Background::Color(super::with_alpha(
                Color::WHITE,
                0.08
            )))
        );
        assert_eq!(style.shadow, crate::tokens::TOKENS.shadows.none.iced());
    }
}
