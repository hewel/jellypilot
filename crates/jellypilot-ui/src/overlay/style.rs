use iced::widget::container;
use iced::{Background, Border, Color, Theme};

use crate::tokens::{palette, TOKENS};

pub(super) fn popover_surface(theme: &Theme) -> container::Style {
    let palette = palette(theme);
    container::Style {
        background: Some(Background::Color(palette.colors.surfaceContainerHigh)),
        text_color: Some(palette.colors.onSurface),
        border: Border {
            radius: TOKENS.radii.lg.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        shadow: palette.shadows.raised_high.iced(),
        ..container::Style::default()
    }
}

pub(super) fn tooltip_surface(theme: &Theme) -> container::Style {
    let palette = palette(theme);
    container::Style {
        background: Some(Background::Color(palette.colors.surfaceContainerHighest)),
        text_color: Some(palette.colors.onSurface),
        border: Border {
            radius: TOKENS.radii.md.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        shadow: palette.shadows.raised.iced(),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use iced::{Background, Color};

    use super::{popover_surface, tooltip_surface};
    use crate::tokens::{DARK_PALETTE, TOKENS};

    #[test]
    fn popover_surface_is_an_opaque_borderless_raised_layer() {
        let style = popover_surface(&crate::theme::theme(crate::theme::ThemeMode::Dark));
        match style.background {
            Some(Background::Color(color)) => {
                assert_eq!(color.a, 1.0, "popover background must be fully opaque");
                assert_eq!(color, DARK_PALETTE.colors.surfaceContainerHigh);
            }
            other => panic!("expected Color background, got {other:?}"),
        }
        assert_eq!(style.border.color, Color::TRANSPARENT);
        assert_eq!(style.border.width, 0.0);
        assert_eq!(
            style.border.radius,
            iced::border::Radius::from(TOKENS.radii.lg)
        );
        assert_eq!(style.shadow, DARK_PALETTE.shadows.raised_high.iced());
    }

    #[test]
    fn tooltip_uses_highest_surface_tokens() {
        let style = tooltip_surface(&crate::theme::theme(crate::theme::ThemeMode::Dark));

        assert_eq!(
            style.background,
            Some(Background::Color(
                DARK_PALETTE.colors.surfaceContainerHighest
            ))
        );
        assert_eq!(style.text_color, Some(DARK_PALETTE.colors.onSurface));
        assert_eq!(style.border.width, 0.0);
        assert_eq!(
            style.border.radius,
            iced::border::Radius::from(TOKENS.radii.md)
        );
        assert_eq!(style.shadow, DARK_PALETTE.shadows.raised.iced());
    }
}
