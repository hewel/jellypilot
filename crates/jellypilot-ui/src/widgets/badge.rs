//! JellyPilot status badge styles.

use iced::widget::container;
use iced::{Background, Border, Color, Theme};

use crate::tokens::TOKENS;
use crate::variants::BadgeVariant;

/// Resolves a status badge variant to an iced container style.
pub fn style(_theme: &Theme, variant: BadgeVariant) -> container::Style {
    let colors = TOKENS.colors;
    let (background, text_color) = match variant {
        BadgeVariant::Success => (colors.tertiaryContainer, colors.tertiary),
        BadgeVariant::Warning => (colors.warningContainer, colors.warning),
        BadgeVariant::Neutral => (colors.surfaceContainerHigh, colors.onSurfaceVariant),
    };

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(text_color),
        border: Border {
            radius: TOKENS.radii.md.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use iced::border::Radius;

    use super::*;

    #[test]
    fn badge_variants_use_md_radius_and_no_border() {
        let theme = crate::theme::theme();
        for variant in [
            BadgeVariant::Success,
            BadgeVariant::Warning,
            BadgeVariant::Neutral,
        ] {
            let style = style(&theme, variant);
            assert_eq!(
                style.border.radius,
                Radius::from(TOKENS.radii.md),
                "Badge variant {variant:?} must use md (6px) radius token"
            );
            assert_eq!(style.border.width, 0.0);
        }
    }

    #[test]
    fn badge_fills_are_opaque_container_tones() {
        let theme = crate::theme::theme();
        let expected = [
            (BadgeVariant::Success, TOKENS.colors.tertiaryContainer),
            (BadgeVariant::Warning, TOKENS.colors.warningContainer),
            (BadgeVariant::Neutral, TOKENS.colors.surfaceContainerHigh),
        ];
        for (variant, fill) in expected {
            let style = style(&theme, variant);
            assert_eq!(
                style.background,
                Some(Background::Color(fill)),
                "Badge variant {variant:?} must use its opaque container fill"
            );
        }
    }
}
