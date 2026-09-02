//! JellyPilot container surface roles.
//!
//! Every surface is exactly one role:
//! - `Canvas`: flush with the window background — opaque, square, flat.
//! - `Block`: a docked block (sidebar, player bar) — opaque
//!   `surfaceContainerLowest`, square, flat; separation comes from the two
//!   shell hairlines, not from borders or shadows.
//! - `Raised`: a floating layer (cards, toasts, popovers) — opaque
//!   `surfaceContainerHigh`, `lg` radius, `raised_high` shadow.

use iced::widget::container;
use iced::{Background, Border, Color, Shadow, Theme};

use crate::tokens::{palette, TOKENS};
use crate::variants::SurfaceVariant;

/// Resolves a surface role to an iced container style.
pub fn style(theme: &Theme, variant: SurfaceVariant) -> container::Style {
    let palette = palette(theme);
    let colors = palette.colors;
    let (background, radius, shadow) = match variant {
        SurfaceVariant::Canvas => (colors.background, TOKENS.radii.none, Shadow::default()),
        SurfaceVariant::Block => (
            colors.surfaceContainerLowest,
            TOKENS.radii.none,
            Shadow::default(),
        ),
        SurfaceVariant::Raised => (
            colors.surfaceContainerHigh,
            TOKENS.radii.lg,
            palette.shadows.raised_high.iced(),
        ),
    };

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(colors.onSurface),
        border: Border {
            radius: radius.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        shadow,
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;
    use crate::tokens::DARK_PALETTE;
    use iced::border::Radius;

    #[test]
    fn canvas_is_flush_opaque_and_flat() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        let style = style(&theme, SurfaceVariant::Canvas);

        assert_eq!(
            style.background,
            Some(Background::Color(DARK_PALETTE.colors.background))
        );
        assert_eq!(style.border.width, 0.0);
        assert_eq!(style.border.radius, Radius::from(TOKENS.radii.none));
        assert_eq!(style.shadow, Shadow::default());
    }

    #[test]
    fn block_is_opaque_docked_and_flat() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        let style = style(&theme, SurfaceVariant::Block);

        assert_eq!(
            style.background,
            Some(Background::Color(
                DARK_PALETTE.colors.surfaceContainerLowest
            ))
        );
        assert_eq!(style.border.width, 0.0);
        assert_eq!(style.border.radius, Radius::from(TOKENS.radii.none));
        assert_eq!(style.shadow, Shadow::default());
    }

    #[test]
    fn raised_is_opaque_rounded_and_carries_the_high_shadow() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        let style = style(&theme, SurfaceVariant::Raised);

        assert_eq!(
            style.background,
            Some(Background::Color(DARK_PALETTE.colors.surfaceContainerHigh))
        );
        assert_eq!(style.border.width, 0.0);
        assert_eq!(style.border.radius, Radius::from(TOKENS.radii.lg));
        assert_eq!(style.shadow, DARK_PALETTE.shadows.raised_high.iced());
    }

    #[test]
    fn all_roles_use_fully_opaque_backgrounds() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        for variant in [
            SurfaceVariant::Canvas,
            SurfaceVariant::Block,
            SurfaceVariant::Raised,
        ] {
            let style = style(&theme, variant);
            let Some(Background::Color(color)) = style.background else {
                panic!("role {variant:?} must have a color background");
            };
            assert_eq!(color.a, 1.0, "role {variant:?} must be fully opaque");
        }
    }
}
