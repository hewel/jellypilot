//! JellyPilot button catalog styles.

use iced::widget::button;
use iced::{Background, Border, Color, Theme};

use crate::tokens::{palette, TOKENS};
use crate::variants::ButtonVariant;

/// Resolves a button variant and interaction status to an iced style.
pub fn style(theme: &Theme, variant: ButtonVariant, status: button::Status) -> button::Style {
    let palette = palette(theme);
    let colors = palette.colors;
    let shadows = palette.shadows;
    let (mut background, mut text_color, mut border_color, border_width, mut shadow) = match variant
    {
        ButtonVariant::Primary => (
            Some(colors.primary),
            colors.onPrimary,
            Color::TRANSPARENT,
            0.0,
            shadows.none.iced(),
        ),
        ButtonVariant::Secondary => (
            Some(colors.secondaryContainer),
            colors.onSecondaryContainer,
            colors.outlineVariant,
            1.0,
            shadows.none.iced(),
        ),
        ButtonVariant::Tonal => (
            Some(colors.surfaceContainerLow),
            colors.onSurface,
            Color::TRANSPARENT,
            0.0,
            shadows.none.iced(),
        ),
        ButtonVariant::TonalActive => (
            Some(colors.surfaceContainerHigh),
            colors.onSurface,
            Color::TRANSPARENT,
            0.0,
            shadows.none.iced(),
        ),
        ButtonVariant::Text => (
            None,
            colors.secondary,
            Color::TRANSPARENT,
            0.0,
            shadows.none.iced(),
        ),
        ButtonVariant::Icon => (
            None,
            colors.onSurfaceVariant,
            Color::TRANSPARENT,
            0.0,
            shadows.none.iced(),
        ),
    };

    match status {
        button::Status::Active | button::Status::Pressed => {}
        button::Status::Hovered => match variant {
            ButtonVariant::Primary => {
                background = background.map(|color| brightness(color, 1.1));
            }
            ButtonVariant::Secondary => border_color = colors.outline,
            ButtonVariant::Tonal => background = Some(colors.surfaceContainerHigh),
            ButtonVariant::TonalActive => {}
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
            radius: TOKENS.radii.md.into(),
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

        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
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
    fn button_variants_use_md_radius_token() {
        use crate::variants::ButtonVariant;
        use iced::border::Radius;
        use iced::widget::button::Status;

        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Tonal,
            ButtonVariant::TonalActive,
            ButtonVariant::Text,
            ButtonVariant::Icon,
        ] {
            let style = super::style(&theme, variant, Status::Active);
            assert_eq!(
                style.border.radius,
                Radius::from(crate::tokens::TOKENS.radii.md),
                "Button variant {variant:?} must use md (6px) radius token"
            );
        }
    }

    #[test]
    fn buttons_cast_no_shadow_in_any_status() {
        use crate::variants::ButtonVariant;
        use iced::widget::button::Status;
        use iced::Shadow;

        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Tonal,
            ButtonVariant::TonalActive,
            ButtonVariant::Text,
            ButtonVariant::Icon,
        ] {
            for status in [
                Status::Active,
                Status::Hovered,
                Status::Pressed,
                Status::Disabled,
            ] {
                let style = super::style(&theme, variant, status);
                assert_eq!(
                    style.shadow,
                    Shadow::default(),
                    "Button variant {variant:?} must cast no shadow in status {status:?}"
                );
            }
        }
    }

    #[test]
    fn tonal_rests_on_container_low_and_lifts_to_high_on_hover() {
        use crate::variants::ButtonVariant;
        use iced::widget::button::Status;
        use iced::Background;

        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let idle = super::style(&theme, ButtonVariant::Tonal, Status::Active);
        assert_eq!(
            idle.background,
            Some(Background::Color(
                crate::tokens::DARK_PALETTE.colors.surfaceContainerLow
            ))
        );
        assert_eq!(idle.border.width, 0.0);

        let hovered = super::style(&theme, ButtonVariant::Tonal, Status::Hovered);
        assert_eq!(
            hovered.background,
            Some(Background::Color(
                crate::tokens::DARK_PALETTE.colors.surfaceContainerHigh
            ))
        );
        assert_eq!(hovered.border.width, 0.0);
    }

    #[test]
    fn tonal_active_is_always_filled_with_surface_container_high() {
        use crate::variants::ButtonVariant;
        use iced::widget::button::Status;
        use iced::Background;

        let theme = crate::theme::theme(crate::theme::ThemeMode::Dark);
        for status in [Status::Active, Status::Hovered, Status::Pressed] {
            let style = super::style(&theme, ButtonVariant::TonalActive, status);
            assert_eq!(
                style.background,
                Some(Background::Color(
                    crate::tokens::DARK_PALETTE.colors.surfaceContainerHigh
                )),
                "TonalActive must stay filled in status {status:?}"
            );
            assert_eq!(style.border.width, 0.0);
        }
    }
}
