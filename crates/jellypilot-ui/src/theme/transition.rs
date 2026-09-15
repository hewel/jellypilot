use iced::Color;

use crate::tokens::{
    SemanticColors, ShadowToken, Shadows, TextColors, ThemePalette, DARK_PALETTE, LIGHT_PALETTE,
};

// Stable borrowed palettes let view closures and Catalog styles share exactly
// the same colors without allocating or rebuilding all semantic roles per widget.
// The 256 shared steps avoid per-frame palette allocation.
static PALETTES: [ThemePalette; 256] = palettes();

pub(super) fn at(progress: f32) -> &'static ThemePalette {
    &PALETTES[index(progress)]
}

pub(super) fn for_background(background: Color) -> Option<&'static ThemePalette> {
    let progress = (background.r - DARK_PALETTE.colors.background.r)
        / (LIGHT_PALETTE.colors.background.r - DARK_PALETTE.colors.background.r);
    let palette = at(progress);
    (palette.colors.background == background).then_some(palette)
}

fn index(progress: f32) -> usize {
    if progress.is_finite() {
        (progress.clamp(0.0, 1.0) * 255.0).round() as usize
    } else {
        0
    }
}

const fn scalar(a: f32, b: f32, progress: f32) -> f32 {
    a + (b - a) * progress
}

const fn color(a: Color, b: Color, progress: f32) -> Color {
    Color {
        r: scalar(a.r, b.r, progress),
        g: scalar(a.g, b.g, progress),
        b: scalar(a.b, b.b, progress),
        a: scalar(a.a, b.a, progress),
    }
}

const fn shadow(a: ShadowToken, b: ShadowToken, progress: f32) -> ShadowToken {
    ShadowToken {
        color: color(a.color, b.color, progress),
        offset: iced::Vector {
            x: scalar(a.offset.x, b.offset.x, progress),
            y: scalar(a.offset.y, b.offset.y, progress),
        },
        blur_radius: scalar(a.blur_radius, b.blur_radius, progress),
        spread_radius: scalar(a.spread_radius, b.spread_radius, progress),
        inset: a.inset,
    }
}

macro_rules! colors {
    ($kind:ident, $a:expr, $b:expr, $progress:expr; $($field:ident),+ $(,)?) => {
        $kind { $($field: color($a.$field, $b.$field, $progress)),+ }
    };
}

const fn blend(progress: f32) -> ThemePalette {
    let a = DARK_PALETTE;
    let b = LIGHT_PALETTE;
    ThemePalette {
        colors: colors!(SemanticColors, a.colors, b.colors, progress;
            background, borderSubtle, control, controlHover, error, errorContainer,
            favorite, imageOutline, onBackground, onControl, onControlHover,
            onError, onErrorContainer, onPrimary, onPrimaryContainer, onSecondary,
            onSecondaryContainer, onSurface, onSurfaceVariant, onTertiary,
            onTertiaryContainer, onWarning, onWarningContainer, outline,
            outlineVariant, primary, primaryContainer, primaryHover, primaryPressed,
            secondary, secondaryContainer, sidebarBg, surface, surfaceContainer,
            surfaceContainerHigh, surfaceContainerHighest, surfaceContainerLow,
            surfaceContainerLowest, surfaceTint, surfaceVariant, tertiary,
            tertiaryContainer, warning, warningContainer,
        ),
        text: colors!(TextColors, a.text, b.text, progress;
            heading, secondary, body, metadata, muted,
        ),
        shadows: Shadows {
            none: shadow(a.shadows.none, b.shadows.none, progress),
            raised: shadow(a.shadows.raised, b.shadows.raised, progress),
            raised_high: shadow(a.shadows.raised_high, b.shadows.raised_high, progress),
        },
    }
}

const fn palettes() -> [ThemePalette; 256] {
    let mut result = [DARK_PALETTE; 256];
    let mut i = 1;
    while i < 255 {
        result[i] = blend(i as f32 / 255.0);
        i += 1;
    }
    result[255] = LIGHT_PALETTE;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_and_explicit_view_colors_agree_during_a_theme_transition() {
        for progress in [0.0, 0.17, 0.51, 0.84, 1.0] {
            let theme = crate::theme::theme_at(progress);
            assert_eq!(crate::tokens::palette(&theme), at(progress));
        }
        assert_eq!(at(0.0), &DARK_PALETTE);
        assert_eq!(at(1.0), &LIGHT_PALETTE);
    }
}
