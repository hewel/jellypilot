//! JellyPilot design tokens: the locked Neon Indigo accent over Charcoal
//! (dark) and Light Clean (light) surface systems, each with a five-step
//! high-contrast text hierarchy.

use std::time::Duration;

use iced::{Color, Shadow, Theme, Vector};

/// Complete JellyPilot design-token set.
#[derive(Debug, Clone, Copy)]
pub struct DesignTokens {
    pub fonts: Fonts,
    pub spacing: Spacing,
    pub font_sizes: FontSizes,
    pub line_heights: LineHeights,
    pub font_weights: FontWeights,
    pub radii: Radii,
    pub z_index: ZIndex,
    pub letter_spacings: LetterSpacings,
    pub durations: Durations,
    pub easings: Easings,
    pub breakpoints: Breakpoints,
}

/// Semantic color roles.
///
/// Field spelling follows the Material-3 role names the design contract was
/// written in; `favorite` is the JellyPilot-specific rose accent for the
/// favorited-heart state.
#[expect(
    non_snake_case,
    reason = "semantic role names follow the Material-3 spelling of the design contract"
)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SemanticColors {
    pub background: Color,
    pub error: Color,
    pub errorContainer: Color,
    pub favorite: Color,
    pub onBackground: Color,
    pub onError: Color,
    pub onErrorContainer: Color,
    pub onPrimary: Color,
    pub onPrimaryContainer: Color,
    pub onSecondary: Color,
    pub onSecondaryContainer: Color,
    pub onSurface: Color,
    pub onSurfaceVariant: Color,
    pub onTertiary: Color,
    pub onTertiaryContainer: Color,
    pub onWarning: Color,
    pub onWarningContainer: Color,
    pub outline: Color,
    pub outlineVariant: Color,
    pub primary: Color,
    pub primaryContainer: Color,
    pub secondary: Color,
    pub secondaryContainer: Color,
    pub surface: Color,
    pub surfaceContainer: Color,
    pub surfaceContainerHigh: Color,
    pub surfaceContainerHighest: Color,
    pub surfaceContainerLow: Color,
    pub surfaceContainerLowest: Color,
    pub surfaceTint: Color,
    pub surfaceVariant: Color,
    pub tertiary: Color,
    pub tertiaryContainer: Color,
    pub warning: Color,
    pub warningContainer: Color,
}

/// The five-step text hierarchy.
///
/// `heading` through `metadata` meet the 4.5:1 normal-text contrast floor on
/// their mode's canvas. `muted` is exempt from the floor and is reserved for
/// auxiliary, non-essential text (device IDs, placeholders, loading hints);
/// anything the user must read must use `metadata` or brighter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextColors {
    /// Level 1: page titles, item names, primary values.
    pub heading: Color,
    /// Level 2: cast and genre values, important subtitles.
    pub secondary: Color,
    /// Level 3: overviews and long-form reading text.
    pub body: Color,
    /// Level 4: labels, years, timestamps, captions.
    pub metadata: Color,
    /// Level 5: auxiliary hints and placeholders only; exempt from the 4.5:1 floor.
    pub muted: Color,
}

/// Canonical Panda font-family stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fonts {
    pub display: &'static str,
    pub mono: &'static str,
    pub sans: &'static str,
}

/// Panda spacing tokens and component-specific spatial constraints in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    pub s0: f32,
    pub px: f32,
    pub s0_5: f32,
    pub s1: f32,
    pub s1_5: f32,
    pub s2: f32,
    pub s2_5: f32,
    pub s3: f32,
    pub s3_5: f32,
    pub s4: f32,
    pub s5: f32,
    pub s6: f32,
    pub s7: f32,
    pub s8: f32,
    pub s9: f32,
    pub s10: f32,
    pub s11: f32,
    pub s12: f32,
    pub s14: f32,
    pub s16: f32,
    pub s20: f32,
    pub s24: f32,
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub x2l: f32,
    pub x3l: f32,
    pub x4l: f32,
    pub x5l: f32,
    pub x6l: f32,
    pub x7l: f32,
    pub x8l: f32,
    pub x9l: f32,
    pub tooltip_max_width: f32,
}

/// Panda font-size tokens in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct FontSizes {
    pub s10: f32,
    pub s11: f32,
    pub s12: f32,
    pub s13: f32,
    pub s14: f32,
    pub s15: f32,
    pub s16: f32,
    pub s18: f32,
    pub s20: f32,
    pub s22: f32,
    pub s24: f32,
    pub s28: f32,
    pub s32: f32,
    pub s36: f32,
    pub s45: f32,
}

/// Panda line-height tokens. Named values are multipliers; numbered values are pixels.
#[derive(Debug, Clone, Copy)]
pub struct LineHeights {
    pub none: f32,
    pub tight: f32,
    pub snug: f32,
    pub normal: f32,
    pub relaxed: f32,
    pub loose: f32,
    pub s14: f32,
    pub s16: f32,
    pub s20: f32,
    pub s22: f32,
    pub s24: f32,
    pub s28: f32,
    pub s32: f32,
    pub s40: f32,
    pub s44: f32,
    pub s52: f32,
}

/// Panda numeric font-weight tokens.
#[derive(Debug, Clone, Copy)]
pub struct FontWeights {
    pub normal: u16,
    pub medium: u16,
    pub semibold: u16,
    pub bold: u16,
    pub extrabold: u16,
    pub black: u16,
}

/// Panda radius tokens in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Radii {
    pub none: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub full: f32,
}

/// A shadow literal that preserves the CSS spread and inset data iced cannot render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowToken {
    pub color: Color,
    pub offset: Vector,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub inset: bool,
}

impl ShadowToken {
    /// Converts the representable offset, blur, and color to an iced shadow.
    pub const fn iced(self) -> Shadow {
        Shadow {
            color: self.color,
            offset: self.offset,
            blur_radius: self.blur_radius,
        }
    }
}

/// Panda shadow tokens: a two-tier semantic scale. `none` for flush
/// surfaces, `raised` for small floating chrome, `raised_high` for floating
/// layers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadows {
    pub none: ShadowToken,
    /// Low lift for small floating chrome (tooltips, scroll indicators).
    pub raised: ShadowToken,
    /// High lift for floating layers (popovers, toasts, raised cards).
    pub raised_high: ShadowToken,
}
/// Mode-variant tokens: the semantic colors, text hierarchy, and shadows for
/// one theme mode. Everything structural (spacing, radii, fonts, …) is
/// mode-independent and stays on [`TOKENS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePalette {
    pub colors: SemanticColors,
    pub text: TextColors,
    pub shadows: Shadows,
}

/// A Panda z-index literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZIndexValue {
    Auto,
    Value(i32),
}

/// Panda z-index tokens.
#[derive(Debug, Clone, Copy)]
pub struct ZIndex {
    pub auto: ZIndexValue,
    pub z0: ZIndexValue,
    pub z10: ZIndexValue,
    pub z20: ZIndexValue,
    pub z40: ZIndexValue,
    pub z50: ZIndexValue,
    pub z60: ZIndexValue,
    pub z100: ZIndexValue,
    pub behind: ZIndexValue,
}

/// Panda letter-spacing tokens in em units.
#[derive(Debug, Clone, Copy)]
pub struct LetterSpacings {
    pub s0: f32,
    pub s5: f32,
    pub s8: f32,
    pub s18: f32,
    pub s20: f32,
    pub s25: f32,
}

/// Panda transition-duration tokens.
#[derive(Debug, Clone, Copy)]
pub struct Durations {
    pub none: Duration,
    pub ms75: Duration,
    pub ms100: Duration,
    pub ms150: Duration,
    pub ms200: Duration,
    pub ms300: Duration,
    pub ms500: Duration,
    pub ms700: Duration,
    pub ms1000: Duration,
}

/// A Panda easing literal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    CubicBezier([f32; 4]),
    Linear,
}

/// Panda easing tokens.
#[derive(Debug, Clone, Copy)]
pub struct Easings {
    pub standard: Easing,
    pub emphasized: Easing,
    pub in_out: Easing,
    pub linear: Easing,
}

/// Compile-time Panda responsive breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Breakpoints {
    pub sm: &'static str,
    pub md: &'static str,
    pub lg: &'static str,
    pub xl: &'static str,
    pub x2l: &'static str,
}

/// Mode-independent JellyPilot tokens.
pub const TOKENS: DesignTokens = DesignTokens {
    fonts: Fonts {
        display: "'Space Grotesk Variable', 'Inter Variable', ui-sans-serif, system-ui, sans-serif",
        mono: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
        sans: "'Inter Variable', ui-sans-serif, system-ui, sans-serif",
    },
    spacing: Spacing {
        s0: 0.0,
        px: 1.0,
        s0_5: 2.0,
        s1: 4.0,
        s1_5: 6.0,
        s2: 8.0,
        s2_5: 10.0,
        s3: 12.0,
        s3_5: 14.0,
        s4: 16.0,
        s5: 20.0,
        s6: 24.0,
        s7: 28.0,
        s8: 32.0,
        s9: 36.0,
        s10: 40.0,
        s11: 44.0,
        s12: 48.0,
        s14: 56.0,
        s16: 64.0,
        s20: 80.0,
        s24: 96.0,
        xs: 12.0,
        sm: 14.0,
        md: 16.0,
        lg: 18.0,
        xl: 20.0,
        x2l: 24.0,
        x3l: 30.0,
        x4l: 36.0,
        x5l: 48.0,
        x6l: 60.0,
        x7l: 72.0,
        x8l: 96.0,
        x9l: 128.0,
        tooltip_max_width: 300.0,
    },
    font_sizes: FontSizes {
        s10: 10.0,
        s11: 11.0,
        s12: 12.0,
        s13: 13.0,
        s14: 14.0,
        s15: 15.0,
        s16: 16.0,
        s18: 18.0,
        s20: 20.0,
        s22: 22.0,
        s24: 24.0,
        s28: 28.0,
        s32: 32.0,
        s36: 36.0,
        s45: 45.0,
    },
    line_heights: LineHeights {
        none: 1.0,
        tight: 1.25,
        snug: 1.375,
        normal: 1.5,
        relaxed: 1.625,
        loose: 2.0,
        s14: 14.0,
        s16: 16.0,
        s20: 20.0,
        s22: 22.0,
        s24: 24.0,
        s28: 28.0,
        s32: 32.0,
        s40: 40.0,
        s44: 44.0,
        s52: 52.0,
    },
    font_weights: FontWeights {
        normal: 400,
        medium: 500,
        semibold: 600,
        bold: 700,
        extrabold: 800,
        black: 900,
    },
    radii: Radii {
        none: 0.0,
        sm: 2.0,
        md: 6.0,
        lg: 8.0,
        full: 9999.0,
    },
    z_index: ZIndex {
        auto: ZIndexValue::Auto,
        z0: ZIndexValue::Value(0),
        z10: ZIndexValue::Value(10),
        z20: ZIndexValue::Value(20),
        z40: ZIndexValue::Value(40),
        z50: ZIndexValue::Value(50),
        z60: ZIndexValue::Value(60),
        z100: ZIndexValue::Value(100),
        behind: ZIndexValue::Value(-1),
    },
    letter_spacings: LetterSpacings {
        s0: 0.0,
        s5: 0.05,
        s8: 0.08,
        s18: 0.18,
        s20: 0.2,
        s25: 0.25,
    },
    durations: Durations {
        none: Duration::from_millis(0),
        ms75: Duration::from_millis(75),
        ms100: Duration::from_millis(100),
        ms150: Duration::from_millis(150),
        ms200: Duration::from_millis(200),
        ms300: Duration::from_millis(300),
        ms500: Duration::from_millis(500),
        ms700: Duration::from_millis(700),
        ms1000: Duration::from_millis(1_000),
    },
    easings: Easings {
        standard: Easing::CubicBezier([0.2, 0.0, 0.0, 1.0]),
        emphasized: Easing::CubicBezier([0.16, 1.0, 0.3, 1.0]),
        in_out: Easing::CubicBezier([0.4, 0.0, 0.6, 1.0]),
        linear: Easing::Linear,
    },
    breakpoints: Breakpoints {
        sm: "640px",
        md: "768px",
        lg: "1024px",
        xl: "1280px",
        x2l: "1536px",
    },
};

/// Dark JellyPilot palette: Charcoal. Near-zero-chroma deep-charcoal surfaces
/// (4–7% lightness, never OLED pure black) under the Neon Indigo accent
/// (`#6366f1`); the text hierarchy runs a cool-neutral ladder from white.
pub const DARK_PALETTE: ThemePalette = ThemePalette {
    colors: SemanticColors {
        background: Color::from_rgb8(0x0a, 0x0b, 0x0e),
        error: Color::from_rgb8(0xff, 0x6b, 0x7a),
        errorContainer: Color::from_rgb8(0x4b, 0x11, 0x19),
        favorite: Color::from_rgb8(0xf8, 0x71, 0x71),
        onBackground: Color::from_rgb8(0xff, 0xff, 0xff),
        onError: Color::from_rgb8(0x33, 0x00, 0x06),
        onErrorContainer: Color::from_rgb8(0xff, 0xd9, 0xde),
        onPrimary: Color::from_rgb8(0xff, 0xff, 0xff),
        onPrimaryContainer: Color::from_rgb8(0xe0, 0xe2, 0xff),
        onSecondary: Color::from_rgb8(0x0b, 0x0a, 0x24),
        onSecondaryContainer: Color::from_rgb8(0x81, 0x8c, 0xf8),
        onSurface: Color::from_rgb8(0xff, 0xff, 0xff),
        onSurfaceVariant: Color::from_rgb8(0xa1, 0xa1, 0xaa),
        onTertiary: Color::from_rgb8(0x00, 0x1f, 0x16),
        onTertiaryContainer: Color::from_rgb8(0xa7, 0xf3, 0xd0),
        onWarning: Color::from_rgb8(0x2a, 0x1a, 0x00),
        onWarningContainer: Color::from_rgb8(0xfd, 0xe6, 0x8a),
        outline: Color::from_rgb8(0x52, 0x52, 0x5b),
        outlineVariant: Color::from_rgb8(0x2a, 0x2a, 0x30),
        primary: Color::from_rgb8(0x63, 0x66, 0xf1),
        primaryContainer: Color::from_rgb8(0x1a, 0x1b, 0x37),
        secondary: Color::from_rgb8(0x81, 0x8c, 0xf8),
        secondaryContainer: Color::from_rgb8(0x18, 0x19, 0x2f),
        surface: Color::from_rgb8(0x15, 0x16, 0x1c),
        surfaceContainer: Color::from_rgb8(0x19, 0x1a, 0x21),
        surfaceContainerHigh: Color::from_rgb8(0x20, 0x22, 0x2b),
        surfaceContainerHighest: Color::from_rgb8(0x2a, 0x2d, 0x38),
        surfaceContainerLow: Color::from_rgb8(0x12, 0x13, 0x18),
        surfaceContainerLowest: Color::from_rgb8(0x0e, 0x0f, 0x14),
        surfaceTint: Color::from_rgb8(0x63, 0x66, 0xf1),
        surfaceVariant: Color::from_rgb8(0x1b, 0x1d, 0x26),
        tertiary: Color::from_rgb8(0x34, 0xd3, 0x99),
        tertiaryContainer: Color::from_rgb8(0x06, 0x38, 0x2a),
        warning: Color::from_rgb8(0xfb, 0xbf, 0x24),
        warningContainer: Color::from_rgb8(0x3f, 0x2e, 0x08),
    },
    text: TextColors {
        heading: Color::from_rgb8(0xff, 0xff, 0xff),
        secondary: Color::from_rgb8(0xf4, 0xf4, 0xf5),
        body: Color::from_rgb8(0xd4, 0xd4, 0xd8),
        metadata: Color::from_rgb8(0xa1, 0xa1, 0xaa),
        muted: Color::from_rgb8(0x71, 0x71, 0x7a),
    },
    shadows: Shadows {
        none: shadow(0.0, 0.0, 0.0, 0.0, 0.0, false),
        raised: shadow(0.0, 2.0, 8.0, 0.0, 0.45, false),
        raised_high: shadow(0.0, 8.0, 24.0, 0.0, 0.65, false),
    },
};

/// Light JellyPilot palette: Light Clean. Cold-white canvas with pure-white
/// surfaces and a Slate text ladder. The Neon Indigo accent stays `#6366f1`;
/// accent text drops to the deeper `#4f46e5`, and status roles drop to their
/// 700-series steps so text and icons hold the 4.5:1 floor on the light
/// canvas.
pub const LIGHT_PALETTE: ThemePalette = ThemePalette {
    colors: SemanticColors {
        background: Color::from_rgb8(0xfb, 0xfc, 0xfd),
        error: Color::from_rgb8(0x4b, 0x11, 0x19),
        errorContainer: Color::from_rgb8(0xff, 0xd9, 0xde),
        favorite: Color::from_rgb8(0xe1, 0x1d, 0x48),
        onBackground: Color::from_rgb8(0x0f, 0x17, 0x2a),
        onError: Color::from_rgb8(0xff, 0xd9, 0xde),
        onErrorContainer: Color::from_rgb8(0x4b, 0x11, 0x19),
        onPrimary: Color::from_rgb8(0xff, 0xff, 0xff),
        onPrimaryContainer: Color::from_rgb8(0x1f, 0x21, 0x52),
        onSecondary: Color::from_rgb8(0xff, 0xff, 0xff),
        onSecondaryContainer: Color::from_rgb8(0x4f, 0x46, 0xe5),
        onSurface: Color::from_rgb8(0x0f, 0x17, 0x2a),
        onSurfaceVariant: Color::from_rgb8(0x64, 0x74, 0x8b),
        onTertiary: Color::from_rgb8(0xff, 0xff, 0xff),
        onTertiaryContainer: Color::from_rgb8(0x06, 0x5f, 0x46),
        onWarning: Color::from_rgb8(0xff, 0xff, 0xff),
        onWarningContainer: Color::from_rgb8(0x92, 0x40, 0x0e),
        outline: Color::from_rgb8(0x94, 0xa3, 0xb8),
        outlineVariant: Color::from_rgb8(0xcb, 0xd5, 0xe1),
        primary: Color::from_rgb8(0x63, 0x66, 0xf1),
        primaryContainer: Color::from_rgb8(0xe0, 0xe2, 0xff),
        secondary: Color::from_rgb8(0x4f, 0x46, 0xe5),
        secondaryContainer: Color::from_rgb8(0xe8, 0xe8, 0xf9),
        surface: Color::from_rgb8(0xff, 0xff, 0xff),
        surfaceContainer: Color::from_rgb8(0xe9, 0xed, 0xf2),
        surfaceContainerHigh: Color::from_rgb8(0xe2, 0xe8, 0xf0),
        surfaceContainerHighest: Color::from_rgb8(0xd3, 0xda, 0xe4),
        surfaceContainerLow: Color::from_rgb8(0xf1, 0xf5, 0xf9),
        surfaceContainerLowest: Color::from_rgb8(0xfa, 0xfa, 0xfa),
        surfaceTint: Color::from_rgb8(0x63, 0x66, 0xf1),
        surfaceVariant: Color::from_rgb8(0xe5, 0xea, 0xf1),
        tertiary: Color::from_rgb8(0x04, 0x78, 0x57),
        tertiaryContainer: Color::from_rgb8(0xd1, 0xfa, 0xe5),
        warning: Color::from_rgb8(0xb4, 0x53, 0x09),
        warningContainer: Color::from_rgb8(0xfe, 0xf3, 0xc7),
    },
    text: TextColors {
        heading: Color::from_rgb8(0x0f, 0x17, 0x2a),
        secondary: Color::from_rgb8(0x1e, 0x29, 0x3b),
        body: Color::from_rgb8(0x47, 0x55, 0x69),
        metadata: Color::from_rgb8(0x64, 0x74, 0x8b),
        muted: Color::from_rgb8(0x94, 0xa3, 0xb8),
    },
    shadows: Shadows {
        none: shadow(0.0, 0.0, 0.0, 0.0, 0.0, false),
        raised: shadow(0.0, 2.0, 8.0, 0.0, 0.06, false),
        raised_high: shadow(0.0, 8.0, 24.0, 0.0, 0.10, false),
    },
};

/// Resolves the palette for an iced theme by matching its background color
/// against the two palettes' `background` values. Unknown themes fall back
/// to the dark palette.
pub fn palette(theme: &Theme) -> &'static ThemePalette {
    if theme.palette().background == LIGHT_PALETTE.colors.background {
        &LIGHT_PALETTE
    } else {
        &DARK_PALETTE
    }
}

const fn shadow(
    x: f32,
    y: f32,
    blur_radius: f32,
    spread_radius: f32,
    alpha: f32,
    inset: bool,
) -> ShadowToken {
    ShadowToken {
        color: Color::from_rgba8(0, 0, 0, alpha),
        offset: Vector { x, y },
        blur_radius,
        spread_radius,
        inset,
    }
}

#[cfg(test)]
mod tests {
    use iced::{Color, Theme};

    use super::{
        palette, Breakpoints, Fonts, SemanticColors, TextColors, ThemePalette, DARK_PALETTE,
        LIGHT_PALETTE, TOKENS,
    };

    fn luminance(color: Color) -> f32 {
        0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
    }

    fn linearized(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    /// WCAG contrast ratio between two opaque colors.
    fn contrast(a: Color, b: Color) -> f32 {
        let lum = |c: Color| {
            0.2126 * linearized(c.r) + 0.7152 * linearized(c.g) + 0.0722 * linearized(c.b)
        };
        let (hi, lo) = (lum(a).max(lum(b)), lum(a).min(lum(b)));
        (hi + 0.05) / (lo + 0.05)
    }

    fn semantic_color_fields(colors: &SemanticColors) -> [(&'static str, Color); 35] {
        [
            ("background", colors.background),
            ("error", colors.error),
            ("errorContainer", colors.errorContainer),
            ("favorite", colors.favorite),
            ("onBackground", colors.onBackground),
            ("onError", colors.onError),
            ("onErrorContainer", colors.onErrorContainer),
            ("onPrimary", colors.onPrimary),
            ("onPrimaryContainer", colors.onPrimaryContainer),
            ("onSecondary", colors.onSecondary),
            ("onSecondaryContainer", colors.onSecondaryContainer),
            ("onSurface", colors.onSurface),
            ("onSurfaceVariant", colors.onSurfaceVariant),
            ("onTertiary", colors.onTertiary),
            ("onTertiaryContainer", colors.onTertiaryContainer),
            ("onWarning", colors.onWarning),
            ("onWarningContainer", colors.onWarningContainer),
            ("outline", colors.outline),
            ("outlineVariant", colors.outlineVariant),
            ("primary", colors.primary),
            ("primaryContainer", colors.primaryContainer),
            ("secondary", colors.secondary),
            ("secondaryContainer", colors.secondaryContainer),
            ("surface", colors.surface),
            ("surfaceContainer", colors.surfaceContainer),
            ("surfaceContainerHigh", colors.surfaceContainerHigh),
            ("surfaceContainerHighest", colors.surfaceContainerHighest),
            ("surfaceContainerLow", colors.surfaceContainerLow),
            ("surfaceContainerLowest", colors.surfaceContainerLowest),
            ("surfaceTint", colors.surfaceTint),
            ("surfaceVariant", colors.surfaceVariant),
            ("tertiary", colors.tertiary),
            ("tertiaryContainer", colors.tertiaryContainer),
            ("warning", colors.warning),
            ("warningContainer", colors.warningContainer),
        ]
    }

    #[test]
    fn dark_semantic_colors_match_the_locked_charcoal_spec() {
        assert_eq!(
            DARK_PALETTE.colors,
            SemanticColors {
                background: Color::from_rgb8(0x0a, 0x0b, 0x0e),
                error: Color::from_rgb8(0xff, 0x6b, 0x7a),
                errorContainer: Color::from_rgb8(0x4b, 0x11, 0x19),
                favorite: Color::from_rgb8(0xf8, 0x71, 0x71),
                onBackground: Color::from_rgb8(0xff, 0xff, 0xff),
                onError: Color::from_rgb8(0x33, 0x00, 0x06),
                onErrorContainer: Color::from_rgb8(0xff, 0xd9, 0xde),
                onPrimary: Color::from_rgb8(0xff, 0xff, 0xff),
                onPrimaryContainer: Color::from_rgb8(0xe0, 0xe2, 0xff),
                onSecondary: Color::from_rgb8(0x0b, 0x0a, 0x24),
                onSecondaryContainer: Color::from_rgb8(0x81, 0x8c, 0xf8),
                onSurface: Color::from_rgb8(0xff, 0xff, 0xff),
                onSurfaceVariant: Color::from_rgb8(0xa1, 0xa1, 0xaa),
                onTertiary: Color::from_rgb8(0x00, 0x1f, 0x16),
                onTertiaryContainer: Color::from_rgb8(0xa7, 0xf3, 0xd0),
                onWarning: Color::from_rgb8(0x2a, 0x1a, 0x00),
                onWarningContainer: Color::from_rgb8(0xfd, 0xe6, 0x8a),
                outline: Color::from_rgb8(0x52, 0x52, 0x5b),
                outlineVariant: Color::from_rgb8(0x2a, 0x2a, 0x30),
                primary: Color::from_rgb8(0x63, 0x66, 0xf1),
                primaryContainer: Color::from_rgb8(0x1a, 0x1b, 0x37),
                secondary: Color::from_rgb8(0x81, 0x8c, 0xf8),
                secondaryContainer: Color::from_rgb8(0x18, 0x19, 0x2f),
                surface: Color::from_rgb8(0x15, 0x16, 0x1c),
                surfaceContainer: Color::from_rgb8(0x19, 0x1a, 0x21),
                surfaceContainerHigh: Color::from_rgb8(0x20, 0x22, 0x2b),
                surfaceContainerHighest: Color::from_rgb8(0x2a, 0x2d, 0x38),
                surfaceContainerLow: Color::from_rgb8(0x12, 0x13, 0x18),
                surfaceContainerLowest: Color::from_rgb8(0x0e, 0x0f, 0x14),
                surfaceTint: Color::from_rgb8(0x63, 0x66, 0xf1),
                surfaceVariant: Color::from_rgb8(0x1b, 0x1d, 0x26),
                tertiary: Color::from_rgb8(0x34, 0xd3, 0x99),
                tertiaryContainer: Color::from_rgb8(0x06, 0x38, 0x2a),
                warning: Color::from_rgb8(0xfb, 0xbf, 0x24),
                warningContainer: Color::from_rgb8(0x3f, 0x2e, 0x08),
            }
        );
    }

    #[test]
    fn light_semantic_colors_match_the_locked_light_clean_spec() {
        assert_eq!(
            LIGHT_PALETTE.colors,
            SemanticColors {
                background: Color::from_rgb8(0xfb, 0xfc, 0xfd),
                error: Color::from_rgb8(0x4b, 0x11, 0x19),
                errorContainer: Color::from_rgb8(0xff, 0xd9, 0xde),
                favorite: Color::from_rgb8(0xe1, 0x1d, 0x48),
                onBackground: Color::from_rgb8(0x0f, 0x17, 0x2a),
                onError: Color::from_rgb8(0xff, 0xd9, 0xde),
                onErrorContainer: Color::from_rgb8(0x4b, 0x11, 0x19),
                onPrimary: Color::from_rgb8(0xff, 0xff, 0xff),
                onPrimaryContainer: Color::from_rgb8(0x1f, 0x21, 0x52),
                onSecondary: Color::from_rgb8(0xff, 0xff, 0xff),
                onSecondaryContainer: Color::from_rgb8(0x4f, 0x46, 0xe5),
                onSurface: Color::from_rgb8(0x0f, 0x17, 0x2a),
                onSurfaceVariant: Color::from_rgb8(0x64, 0x74, 0x8b),
                onTertiary: Color::from_rgb8(0xff, 0xff, 0xff),
                onTertiaryContainer: Color::from_rgb8(0x06, 0x5f, 0x46),
                onWarning: Color::from_rgb8(0xff, 0xff, 0xff),
                onWarningContainer: Color::from_rgb8(0x92, 0x40, 0x0e),
                outline: Color::from_rgb8(0x94, 0xa3, 0xb8),
                outlineVariant: Color::from_rgb8(0xcb, 0xd5, 0xe1),
                primary: Color::from_rgb8(0x63, 0x66, 0xf1),
                primaryContainer: Color::from_rgb8(0xe0, 0xe2, 0xff),
                secondary: Color::from_rgb8(0x4f, 0x46, 0xe5),
                secondaryContainer: Color::from_rgb8(0xe8, 0xe8, 0xf9),
                surface: Color::from_rgb8(0xff, 0xff, 0xff),
                surfaceContainer: Color::from_rgb8(0xe9, 0xed, 0xf2),
                surfaceContainerHigh: Color::from_rgb8(0xe2, 0xe8, 0xf0),
                surfaceContainerHighest: Color::from_rgb8(0xd3, 0xda, 0xe4),
                surfaceContainerLow: Color::from_rgb8(0xf1, 0xf5, 0xf9),
                surfaceContainerLowest: Color::from_rgb8(0xfa, 0xfa, 0xfa),
                surfaceTint: Color::from_rgb8(0x63, 0x66, 0xf1),
                surfaceVariant: Color::from_rgb8(0xe5, 0xea, 0xf1),
                tertiary: Color::from_rgb8(0x04, 0x78, 0x57),
                tertiaryContainer: Color::from_rgb8(0xd1, 0xfa, 0xe5),
                warning: Color::from_rgb8(0xb4, 0x53, 0x09),
                warningContainer: Color::from_rgb8(0xfe, 0xf3, 0xc7),
            }
        );
    }

    #[test]
    fn text_ladders_match_the_locked_spec() {
        assert_eq!(
            DARK_PALETTE.text,
            TextColors {
                heading: Color::from_rgb8(0xff, 0xff, 0xff),
                secondary: Color::from_rgb8(0xf4, 0xf4, 0xf5),
                body: Color::from_rgb8(0xd4, 0xd4, 0xd8),
                metadata: Color::from_rgb8(0xa1, 0xa1, 0xaa),
                muted: Color::from_rgb8(0x71, 0x71, 0x7a),
            }
        );
        assert_eq!(
            LIGHT_PALETTE.text,
            TextColors {
                heading: Color::from_rgb8(0x0f, 0x17, 0x2a),
                secondary: Color::from_rgb8(0x1e, 0x29, 0x3b),
                body: Color::from_rgb8(0x47, 0x55, 0x69),
                metadata: Color::from_rgb8(0x64, 0x74, 0x8b),
                muted: Color::from_rgb8(0x94, 0xa3, 0xb8),
            }
        );
    }

    #[test]
    fn text_ladder_descends_and_holds_the_contrast_floor() {
        for (name, palette) in [("dark", &DARK_PALETTE), ("light", &LIGHT_PALETTE)] {
            let text = palette.text;
            let canvas = palette.colors.background;
            let rungs = [text.heading, text.secondary, text.body, text.metadata];
            // Each step down the hierarchy reads quieter than the one above.
            for pair in rungs.windows(2) {
                assert!(
                    contrast(pair[0], canvas) > contrast(pair[1], canvas),
                    "{name} text ladder must descend: {:?} vs {:?}",
                    pair[0],
                    pair[1]
                );
            }
            // Heading through metadata are normal text and hold the 4.5:1 floor.
            for rung in rungs {
                assert!(
                    contrast(rung, canvas) >= 4.5,
                    "{name} text rung {rung:?} must reach 4.5:1 on the canvas"
                );
            }
            // `muted` is exempt from the floor: auxiliary hints and placeholders only.
        }
    }

    #[test]
    fn badge_and_chip_text_holds_the_contrast_floor() {
        for (name, palette) in [("dark", &DARK_PALETTE), ("light", &LIGHT_PALETTE)] {
            let colors = &palette.colors;
            let pairs = [
                ("warning badge", colors.warning, colors.warningContainer),
                ("success badge", colors.tertiary, colors.tertiaryContainer),
                (
                    "neutral badge",
                    palette.text.secondary,
                    colors.surfaceContainerHigh,
                ),
                (
                    "active switch chip",
                    colors.onSecondaryContainer,
                    colors.secondaryContainer,
                ),
            ];
            for (label, fg, bg) in pairs {
                assert!(
                    contrast(fg, bg) >= 4.5,
                    "{name} {label} text must reach 4.5:1 ({fg:?} on {bg:?})"
                );
            }
        }
    }

    #[test]
    fn font_stacks_match_canonical_panda_literals() {
        assert_eq!(
            TOKENS.fonts,
            Fonts {
                display:
                    "'Space Grotesk Variable', 'Inter Variable', ui-sans-serif, system-ui, sans-serif",
                mono: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
                sans: "'Inter Variable', ui-sans-serif, system-ui, sans-serif",
            }
        );
    }

    #[test]
    fn breakpoints_match_canonical_panda_literals() {
        assert_eq!(
            TOKENS.breakpoints,
            Breakpoints {
                sm: "640px",
                md: "768px",
                lg: "1024px",
                xl: "1280px",
                x2l: "1536px",
            }
        );
    }

    #[test]
    fn radii_scale_matches_the_retuned_tokens() {
        assert_eq!(TOKENS.radii.none, 0.0);
        assert_eq!(TOKENS.radii.sm, 2.0);
        assert_eq!(TOKENS.radii.md, 6.0);
        assert_eq!(TOKENS.radii.lg, 8.0);
        assert_eq!(TOKENS.radii.full, 9999.0);
    }

    #[test]
    fn shadow_tokens_match_the_two_tier_semantic_scale() {
        use super::ShadowToken;
        use iced::Vector;

        assert_eq!(
            DARK_PALETTE.shadows.none,
            ShadowToken {
                color: Color::from_rgba8(0, 0, 0, 0.0),
                offset: Vector { x: 0.0, y: 0.0 },
                blur_radius: 0.0,
                spread_radius: 0.0,
                inset: false,
            }
        );
        assert_eq!(
            DARK_PALETTE.shadows.raised,
            ShadowToken {
                color: Color::from_rgba8(0, 0, 0, 0.45),
                offset: Vector { x: 0.0, y: 2.0 },
                blur_radius: 8.0,
                spread_radius: 0.0,
                inset: false,
            }
        );
        assert_eq!(
            DARK_PALETTE.shadows.raised_high,
            ShadowToken {
                color: Color::from_rgba8(0, 0, 0, 0.65),
                offset: Vector { x: 0.0, y: 8.0 },
                blur_radius: 24.0,
                spread_radius: 0.0,
                inset: false,
            }
        );
    }

    #[test]
    fn palettes_have_fully_opaque_colors() {
        for (name, palette) in [("dark", &DARK_PALETTE), ("light", &LIGHT_PALETTE)] {
            let text_fields = [
                ("text.heading", palette.text.heading),
                ("text.secondary", palette.text.secondary),
                ("text.body", palette.text.body),
                ("text.metadata", palette.text.metadata),
                ("text.muted", palette.text.muted),
            ];
            for (field, color) in semantic_color_fields(&palette.colors)
                .into_iter()
                .chain(text_fields)
            {
                assert_eq!(
                    color.a, 1.0,
                    "{name} palette color {field} must be fully opaque"
                );
            }
        }
    }

    #[test]
    fn light_container_ladder_is_a_distinct_darkening_step_series() {
        let colors = LIGHT_PALETTE.colors;
        let ladder = [
            ("surface", colors.surface),
            ("background", colors.background),
            ("surfaceContainerLowest", colors.surfaceContainerLowest),
            ("surfaceContainerLow", colors.surfaceContainerLow),
            ("surfaceContainer", colors.surfaceContainer),
            ("surfaceVariant", colors.surfaceVariant),
            ("surfaceContainerHigh", colors.surfaceContainerHigh),
            ("surfaceContainerHighest", colors.surfaceContainerHighest),
        ];

        for pair in ladder.windows(2) {
            let (lighter_name, lighter) = pair[0];
            let (darker_name, darker) = pair[1];
            assert!(
                luminance(lighter) > luminance(darker) + 0.005,
                "light ladder must darken perceptibly: {lighter_name} ({lighter:?}) vs {darker_name} ({darker:?})"
            );
        }
    }

    #[test]
    fn light_on_colors_order_against_their_surfaces() {
        let colors = LIGHT_PALETTE.colors;
        // Primary text darker than secondary text, both darker than the canvas.
        assert!(luminance(colors.onSurface) < luminance(colors.onSurfaceVariant));
        assert!(luminance(colors.onSurfaceVariant) < luminance(colors.background));
        // Dark status text on light status containers.
        assert!(luminance(colors.onTertiaryContainer) < luminance(colors.tertiaryContainer));
        assert!(luminance(colors.onWarningContainer) < luminance(colors.warningContainer));
        assert!(luminance(colors.onErrorContainer) < luminance(colors.errorContainer));
        assert!(luminance(colors.onPrimaryContainer) < luminance(colors.primaryContainer));
        assert!(luminance(colors.onSecondaryContainer) < luminance(colors.secondaryContainer));
        // Light text on the filled brand/status accents.
        assert!(luminance(colors.onPrimary) > luminance(colors.primary));
        assert!(luminance(colors.onSecondary) > luminance(colors.secondary));
        assert!(luminance(colors.onTertiary) > luminance(colors.tertiary));
        assert!(luminance(colors.onWarning) > luminance(colors.warning));
        assert!(luminance(colors.onError) > luminance(colors.error));
        // Outlines read against the canvas.
        assert!(luminance(colors.outline) < luminance(colors.background));
        assert!(luminance(colors.outlineVariant) < luminance(colors.background));
    }

    #[test]
    fn light_shadows_keep_the_two_tier_scale_with_lower_alphas() {
        assert_eq!(LIGHT_PALETTE.shadows.none.color.a, 0.0);
        assert_eq!(LIGHT_PALETTE.shadows.raised.color.a, 0.06);
        assert_eq!(LIGHT_PALETTE.shadows.raised_high.color.a, 0.10);
        assert_eq!(
            LIGHT_PALETTE.shadows.raised.offset,
            DARK_PALETTE.shadows.raised.offset
        );
        assert_eq!(
            LIGHT_PALETTE.shadows.raised.blur_radius,
            DARK_PALETTE.shadows.raised.blur_radius
        );
        assert_eq!(
            LIGHT_PALETTE.shadows.raised_high.offset,
            DARK_PALETTE.shadows.raised_high.offset
        );
        assert_eq!(
            LIGHT_PALETTE.shadows.raised_high.blur_radius,
            DARK_PALETTE.shadows.raised_high.blur_radius
        );
    }

    #[test]
    fn palette_resolves_by_theme_background_identity() {
        let dark = crate::theme::theme(crate::theme::ThemeMode::Dark);
        let light = crate::theme::theme(crate::theme::ThemeMode::Light);

        // Value equality: `&CONST` expressions have no stable address, so
        // pointer identity is not a reliable assertion for const palettes.
        assert_eq!(palette(&dark), &DARK_PALETTE);
        assert_eq!(palette(&light), &LIGHT_PALETTE);
        // Any other theme (built-in or custom) falls back to the dark palette.
        assert_eq!(palette(&Theme::Dracula), &DARK_PALETTE);
        // Even iced's built-in light theme falls back: identity is by exact
        // JellyPilot background color, not by brightness.
        assert_eq!(palette(&Theme::Light), &DARK_PALETTE);
    }

    #[test]
    fn theme_palette_keeps_colors_and_shadows_together() {
        let palette: ThemePalette = LIGHT_PALETTE;
        assert_eq!(palette.colors.background.a, 1.0);
        assert!(palette.shadows.raised_high.color.a > 0.0);
    }
}
