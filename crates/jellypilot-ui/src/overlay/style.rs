use iced::widget::container;
use iced::{Background, Border, Color, Theme};

use crate::tokens::TOKENS;
use crate::variants::SurfaceVariant;

pub(super) fn popover_surface(theme: &Theme) -> container::Style {
    crate::widgets::container::style(theme, SurfaceVariant::Elevated)
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
        shadow: TOKENS.shadows.lg.iced(),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use iced::Background;

    use super::{popover_surface, tooltip_surface};
    use crate::tokens::TOKENS;

    #[test]
    fn popover_uses_the_elevated_surface_catalog_style() {
        let theme = crate::theme::theme();

        assert_eq!(
            popover_surface(&theme),
            crate::widgets::container::style(&theme, crate::variants::SurfaceVariant::Elevated)
        );
    }

    #[test]
    fn tooltip_uses_highest_surface_tokens() {
        let style = tooltip_surface(&crate::theme::theme());

        assert_eq!(
            style.background,
            Some(Background::Color(TOKENS.colors.surfaceContainerHighest))
        );
        assert_eq!(style.text_color, Some(TOKENS.colors.onSurface));
    }
}
