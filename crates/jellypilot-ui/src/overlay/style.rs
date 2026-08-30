use iced::widget::container;
use iced::{Background, Border, Color, Theme};

use crate::tokens::TOKENS;

pub(super) fn popover_surface(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(TOKENS.colors.surfaceContainerHigh)),
        text_color: Some(TOKENS.colors.onSurface),
        border: Border {
            radius: TOKENS.radii.lg.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        shadow: TOKENS.shadows.raised_high.iced(),
        ..container::Style::default()
    }
}

pub(super) fn tooltip_surface(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(TOKENS.colors.surfaceContainerHighest)),
        text_color: Some(TOKENS.colors.onSurface),
        border: Border {
            radius: TOKENS.radii.md.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        shadow: TOKENS.shadows.raised.iced(),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use iced::{Background, Color};

    use super::{popover_surface, tooltip_surface};
    use crate::tokens::TOKENS;

    #[test]
    fn popover_surface_is_an_opaque_borderless_raised_layer() {
        let style = popover_surface(&crate::theme::theme());
        match style.background {
            Some(Background::Color(color)) => {
                assert_eq!(color.a, 1.0, "popover background must be fully opaque");
                assert_eq!(color, TOKENS.colors.surfaceContainerHigh);
            }
            other => panic!("expected Color background, got {other:?}"),
        }
        assert_eq!(style.border.color, Color::TRANSPARENT);
        assert_eq!(style.border.width, 0.0);
        assert_eq!(
            style.border.radius,
            iced::border::Radius::from(TOKENS.radii.lg)
        );
        assert_eq!(style.shadow, TOKENS.shadows.raised_high.iced());
    }

    #[test]
    fn tooltip_uses_highest_surface_tokens() {
        let style = tooltip_surface(&crate::theme::theme());

        assert_eq!(
            style.background,
            Some(Background::Color(TOKENS.colors.surfaceContainerHighest))
        );
        assert_eq!(style.text_color, Some(TOKENS.colors.onSurface));
        assert_eq!(style.border.width, 0.0);
        assert_eq!(
            style.border.radius,
            iced::border::Radius::from(TOKENS.radii.md)
        );
        assert_eq!(style.shadow, TOKENS.shadows.raised.iced());
    }
}
