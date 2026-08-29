//! JellyPilot card surface styles.

use iced::widget::container;
use iced::{Background, Border, Color, Shadow, Theme};

use crate::tokens::TOKENS;
use crate::variants::SurfaceVariant;

/// Resolves a card surface variant to an iced container style.
pub fn style(_theme: &Theme, variant: SurfaceVariant) -> container::Style {
    let colors = TOKENS.colors;
    let (background, border, shadow) = match variant {
        SurfaceVariant::Elevated => (
            Some(with_alpha(colors.surfaceContainerLow, 0.45)),
            bordered(TOKENS.radii.xl, with_alpha(colors.primary, 0.2)),
            TOKENS.shadows.x2l.iced(),
        ),
        SurfaceVariant::Filled => (
            Some(with_alpha(colors.surface, 0.5)),
            bordered(TOKENS.radii.xl, with_alpha(colors.outlineVariant, 0.8)),
            TOKENS.shadows.xl.iced(),
        ),
    };

    container_style(background, colors.onSurface, border, shadow)
}

fn container_style(
    background: Option<Color>,
    text_color: Color,
    border: Border,
    shadow: Shadow,
) -> container::Style {
    container::Style {
        background: background.map(Background::Color),
        text_color: Some(text_color),
        border,
        shadow,
        ..container::Style::default()
    }
}

fn bordered(radius: f32, color: Color) -> Border {
    Border {
        radius: radius.into(),
        color,
        width: 1.0,
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::border::Radius;

    #[test]
    fn filled_surface_has_single_card_border() {
        let theme = crate::theme::theme();
        let style = style(&theme, SurfaceVariant::Filled);

        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.radius, Radius::from(TOKENS.radii.xl));
        assert_eq!(
            style.border.color,
            with_alpha(TOKENS.colors.outlineVariant, 0.8)
        );
    }

    #[test]
    fn elevated_surface_has_elevated_card_border() {
        let theme = crate::theme::theme();
        let style = style(&theme, SurfaceVariant::Elevated);

        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.radius, Radius::from(TOKENS.radii.xl));
        assert_eq!(style.border.color, with_alpha(TOKENS.colors.primary, 0.2));
    }
}
