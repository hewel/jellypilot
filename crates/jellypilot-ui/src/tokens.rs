//! Dark-only design tokens ported from JellyPilot's canonical Panda theme.

use std::time::Duration;

use iced::{Color, Shadow, Vector};

/// Complete JellyPilot design-token set.
#[derive(Debug, Clone, Copy)]
pub struct DesignTokens {
    pub raw_colors: RawColors,
    pub colors: SemanticColors,
    pub fonts: Fonts,
    pub spacing: Spacing,
    pub font_sizes: FontSizes,
    pub line_heights: LineHeights,
    pub font_weights: FontWeights,
    pub radii: Radii,
    pub shadows: Shadows,
    pub z_index: ZIndex,
    pub letter_spacings: LetterSpacings,
    pub durations: Durations,
    pub easings: Easings,
    pub breakpoints: Breakpoints,
}

/// Raw Panda color palettes.
#[derive(Debug, Clone, Copy)]
pub struct RawColors {
    pub neutral: Neutral,
    pub indigo: Indigo,
    pub teal: Teal,
    pub amber: Amber,
    pub red: Red,
}

/// Neutral palette steps.
#[derive(Debug, Clone, Copy)]
pub struct Neutral {
    pub n0: Color,
    pub n50: Color,
    pub n300: Color,
    pub n500: Color,
    pub n700: Color,
    pub n750: Color,
    pub n800: Color,
    pub n850: Color,
    pub n900: Color,
    pub n925: Color,
    pub n950: Color,
    pub n975: Color,
    pub n1000: Color,
}

/// Indigo palette steps.
#[derive(Debug, Clone, Copy)]
pub struct Indigo {
    pub n50: Color,
    pub n300: Color,
    pub n600: Color,
    pub n900: Color,
    pub n950: Color,
    pub n1000: Color,
}

/// Teal palette steps.
#[derive(Debug, Clone, Copy)]
pub struct Teal {
    pub n50: Color,
    pub n400: Color,
    pub n900: Color,
    pub n1000: Color,
}

/// Amber palette steps.
#[derive(Debug, Clone, Copy)]
pub struct Amber {
    pub n50: Color,
    pub n400: Color,
    pub n900: Color,
    pub n1000: Color,
}

/// Red palette steps.
#[derive(Debug, Clone, Copy)]
pub struct Red {
    pub n50: Color,
    pub n400: Color,
    pub n900: Color,
    pub n1000: Color,
}

/// Panda semantic color roles.
///
/// Field spelling deliberately matches `semanticColorHex` so a role has the
/// same name in web and iced design discussions.
#[expect(
    non_snake_case,
    reason = "Panda semantic token names are the cross-frontend public contract"
)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SemanticColors {
    pub background: Color,
    pub error: Color,
    pub errorContainer: Color,
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

/// Canonical Panda font-family stacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fonts {
    pub display: &'static str,
    pub mono: &'static str,
    pub sans: &'static str,
}

/// Panda spacing tokens, converted from rem to logical pixels at the 16px root.
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
    pub xl: f32,
    pub x2l: f32,
    pub x3l: f32,
    pub x4l: f32,
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

/// Panda shadow tokens.
#[derive(Debug, Clone, Copy)]
pub struct Shadows {
    pub none: ShadowToken,
    pub sm: ShadowToken,
    pub md: ShadowToken,
    pub lg: ShadowToken,
    pub xl: ShadowToken,
    pub x2l: ShadowToken,
    pub inner: ShadowToken,
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

/// Fixed dark JellyPilot tokens.
pub const TOKENS: DesignTokens = DesignTokens {
    raw_colors: RawColors {
        neutral: Neutral {
            n0: Color::from_rgb8(255, 255, 255),
            n50: Color::from_rgb8(243, 246, 255),
            n300: Color::from_rgb8(174, 184, 204),
            n500: Color::from_rgb8(92, 108, 140),
            n700: Color::from_rgb8(38, 46, 66),
            n750: Color::from_rgb8(34, 41, 62),
            n800: Color::from_rgb8(30, 37, 56),
            n850: Color::from_rgb8(22, 27, 42),
            n900: Color::from_rgb8(17, 20, 32),
            n925: Color::from_rgb8(11, 13, 20),
            n950: Color::from_rgb8(10, 12, 18),
            n975: Color::from_rgb8(5, 6, 10),
            n1000: Color::from_rgb8(4, 5, 8),
        },
        indigo: Indigo {
            n50: Color::from_rgb8(224, 226, 255),
            n300: Color::from_rgb8(129, 140, 248),
            n600: Color::from_rgb8(79, 70, 229),
            n900: Color::from_rgb8(31, 33, 82),
            n950: Color::from_rgb8(27, 28, 59),
            n1000: Color::from_rgb8(11, 10, 36),
        },
        teal: Teal {
            n50: Color::from_rgb8(191, 255, 232),
            n400: Color::from_rgb8(79, 227, 177),
            n900: Color::from_rgb8(6, 56, 42),
            n1000: Color::from_rgb8(0, 31, 22),
        },
        amber: Amber {
            n50: Color::from_rgb8(255, 231, 168),
            n400: Color::from_rgb8(246, 199, 104),
            n900: Color::from_rgb8(63, 46, 8),
            n1000: Color::from_rgb8(42, 26, 0),
        },
        red: Red {
            n50: Color::from_rgb8(255, 217, 222),
            n400: Color::from_rgb8(255, 107, 122),
            n900: Color::from_rgb8(75, 17, 25),
            n1000: Color::from_rgb8(51, 0, 6),
        },
    },
    colors: SemanticColors {
        background: Color::from_rgb8(5, 6, 10),
        error: Color::from_rgb8(255, 107, 122),
        errorContainer: Color::from_rgb8(75, 17, 25),
        onBackground: Color::from_rgb8(243, 246, 255),
        onError: Color::from_rgb8(51, 0, 6),
        onErrorContainer: Color::from_rgb8(255, 217, 222),
        onPrimary: Color::from_rgb8(255, 255, 255),
        onPrimaryContainer: Color::from_rgb8(224, 226, 255),
        onSecondary: Color::from_rgb8(11, 10, 36),
        onSecondaryContainer: Color::from_rgb8(224, 226, 255),
        onSurface: Color::from_rgb8(243, 246, 255),
        onSurfaceVariant: Color::from_rgb8(174, 184, 204),
        onTertiary: Color::from_rgb8(0, 31, 22),
        onTertiaryContainer: Color::from_rgb8(191, 255, 232),
        onWarning: Color::from_rgb8(42, 26, 0),
        onWarningContainer: Color::from_rgb8(255, 231, 168),
        outline: Color::from_rgb8(92, 108, 140),
        outlineVariant: Color::from_rgb8(38, 46, 66),
        primary: Color::from_rgb8(79, 70, 229),
        primaryContainer: Color::from_rgb8(27, 28, 59),
        secondary: Color::from_rgb8(129, 140, 248),
        secondaryContainer: Color::from_rgb8(31, 33, 82),
        surface: Color::from_rgb8(11, 13, 20),
        surfaceContainer: Color::from_rgb8(17, 20, 32),
        surfaceContainerHigh: Color::from_rgb8(22, 27, 42),
        surfaceContainerHighest: Color::from_rgb8(34, 41, 62),
        surfaceContainerLow: Color::from_rgb8(10, 12, 18),
        surfaceContainerLowest: Color::from_rgb8(4, 5, 8),
        surfaceTint: Color::from_rgb8(79, 70, 229),
        surfaceVariant: Color::from_rgb8(30, 37, 56),
        tertiary: Color::from_rgb8(79, 227, 177),
        tertiaryContainer: Color::from_rgb8(6, 56, 42),
        warning: Color::from_rgb8(246, 199, 104),
        warningContainer: Color::from_rgb8(63, 46, 8),
    },
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
        xl: 12.0,
        x2l: 16.0,
        x3l: 24.0,
        x4l: 32.0,
        full: 9999.0,
    },
    shadows: Shadows {
        none: shadow(0.0, 0.0, 0.0, 0.0, 0.0, false),
        sm: shadow(0.0, 1.0, 2.0, 0.0, 0.2, false),
        md: shadow(0.0, 4.0, 8.0, -2.0, 0.35, false),
        lg: shadow(0.0, 10.0, 18.0, -6.0, 0.45, false),
        xl: shadow(0.0, 18.0, 30.0, -10.0, 0.55, false),
        x2l: shadow(0.0, 25.0, 50.0, -12.0, 0.65, false),
        inner: shadow(0.0, 2.0, 4.0, 0.0, 0.28, true),
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
    use iced::Color;

    use super::{Breakpoints, Fonts, SemanticColors, TOKENS};

    #[test]
    fn semantic_colors_match_canonical_panda_hex_literals() {
        assert_eq!(
            TOKENS.colors,
            SemanticColors {
                background: Color::from_rgb8(0x05, 0x06, 0x0a),
                error: Color::from_rgb8(0xff, 0x6b, 0x7a),
                errorContainer: Color::from_rgb8(0x4b, 0x11, 0x19),
                onBackground: Color::from_rgb8(0xf3, 0xf6, 0xff),
                onError: Color::from_rgb8(0x33, 0x00, 0x06),
                onErrorContainer: Color::from_rgb8(0xff, 0xd9, 0xde),
                onPrimary: Color::from_rgb8(0xff, 0xff, 0xff),
                onPrimaryContainer: Color::from_rgb8(0xe0, 0xe2, 0xff),
                onSecondary: Color::from_rgb8(0x0b, 0x0a, 0x24),
                onSecondaryContainer: Color::from_rgb8(0xe0, 0xe2, 0xff),
                onSurface: Color::from_rgb8(0xf3, 0xf6, 0xff),
                onSurfaceVariant: Color::from_rgb8(0xae, 0xb8, 0xcc),
                onTertiary: Color::from_rgb8(0x00, 0x1f, 0x16),
                onTertiaryContainer: Color::from_rgb8(0xbf, 0xff, 0xe8),
                onWarning: Color::from_rgb8(0x2a, 0x1a, 0x00),
                onWarningContainer: Color::from_rgb8(0xff, 0xe7, 0xa8),
                outline: Color::from_rgb8(0x5c, 0x6c, 0x8c),
                outlineVariant: Color::from_rgb8(0x26, 0x2e, 0x42),
                primary: Color::from_rgb8(0x4f, 0x46, 0xe5),
                primaryContainer: Color::from_rgb8(0x1b, 0x1c, 0x3b),
                secondary: Color::from_rgb8(0x81, 0x8c, 0xf8),
                secondaryContainer: Color::from_rgb8(0x1f, 0x21, 0x52),
                surface: Color::from_rgb8(0x0b, 0x0d, 0x14),
                surfaceContainer: Color::from_rgb8(0x11, 0x14, 0x20),
                surfaceContainerHigh: Color::from_rgb8(0x16, 0x1b, 0x2a),
                surfaceContainerHighest: Color::from_rgb8(0x22, 0x29, 0x3e),
                surfaceContainerLow: Color::from_rgb8(0x0a, 0x0c, 0x12),
                surfaceContainerLowest: Color::from_rgb8(0x04, 0x05, 0x08),
                surfaceTint: Color::from_rgb8(0x4f, 0x46, 0xe5),
                surfaceVariant: Color::from_rgb8(0x1e, 0x25, 0x38),
                tertiary: Color::from_rgb8(0x4f, 0xe3, 0xb1),
                tertiaryContainer: Color::from_rgb8(0x06, 0x38, 0x2a),
                warning: Color::from_rgb8(0xf6, 0xc7, 0x68),
                warningContainer: Color::from_rgb8(0x3f, 0x2e, 0x08),
            }
        );
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
}
