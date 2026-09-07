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
use iced::{Background, Border, Color, Theme};

use crate::tokens::{palette, TOKENS};
use crate::variants::SurfaceVariant;

/// Shared contour choice for ordinary rounded rectangle surfaces.
pub const SURFACE_SMOOTHING: f32 = 0.6;

/// Resolves a surface role to an iced container style.
pub fn style(theme: &Theme, variant: SurfaceVariant) -> container::Style {
    let palette = palette(theme);
    let colors = palette.colors;
    let (background, shadow, border) = match variant {
        SurfaceVariant::Canvas => (
            colors.background,
            iced::Shadow::default(),
            Border {
                smoothing: 0.0,
                radius: TOKENS.radii.none.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
        ),
        SurfaceVariant::Block => (
            colors.surfaceContainerLowest,
            iced::Shadow::default(),
            Border {
                smoothing: 0.0,
                radius: TOKENS.radii.none.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
        ),
        SurfaceVariant::Raised => (
            colors.surfaceContainerHigh,
            palette.shadows.raised_high.iced(),
            Border {
                smoothing: SURFACE_SMOOTHING,
                radius: TOKENS.radii.lg.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
        ),
        SurfaceVariant::Floating => (
            colors.surfaceContainerHigh,
            palette.shadows.raised_high.iced(),
            Border {
                smoothing: SURFACE_SMOOTHING,
                radius: TOKENS.radii.lg.into(),
                color: colors.outlineVariant,
                width: 1.0,
            },
        ),
    };

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(colors.onSurface),
        border,
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
    use iced::Shadow;

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
    fn floating_matches_raised_plus_outline() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        let floating = style(&theme, SurfaceVariant::Floating);
        let raised = style(&theme, SurfaceVariant::Raised);

        assert_eq!(floating.background, raised.background);
        assert_eq!(floating.shadow, raised.shadow);
        assert_eq!(floating.border.radius, raised.border.radius);
        assert_eq!(floating.border.width, 1.0);
        assert_eq!(floating.border.color, DARK_PALETTE.colors.outlineVariant);
        assert_eq!(raised.border.width, 0.0);
    }

    #[test]
    fn all_roles_use_fully_opaque_backgrounds() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        for variant in [
            SurfaceVariant::Canvas,
            SurfaceVariant::Block,
            SurfaceVariant::Raised,
            SurfaceVariant::Floating,
        ] {
            let style = style(&theme, variant);
            let Some(Background::Color(color)) = style.background else {
                panic!("role {variant:?} must have a color background");
            };
            assert_eq!(color.a, 1.0, "role {variant:?} must be fully opaque");
        }
    }
}
