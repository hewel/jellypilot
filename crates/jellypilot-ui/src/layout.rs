//! Window-width size classes that drive adaptive layout.
//!
//! Size classes describe geometry only: they select how much content fits and
//! which layout variant a screen uses at a given window width. Typography
//! stays fixed across classes so readability never depends on window size.

/// Widest width (exclusive) classified as [`SizeClass::Compact`].
///
/// Aligned with the design tokens' `breakpoints.xl` ("1280px"). Breakpoints are stored as
/// `&'static str`, so the value is duplicated here as a plain `f32`.
pub const COMPACT_MAX_WIDTH: f32 = 1280.0;

/// Narrowest width (inclusive) classified as [`SizeClass::Wide`].
pub const WIDE_MIN_WIDTH: f32 = 1920.0;

/// Window-width size class driving adaptive layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizeClass {
    /// Width below [`COMPACT_MAX_WIDTH`].
    Compact,
    /// Width between [`COMPACT_MAX_WIDTH`] and [`WIDE_MIN_WIDTH`].
    Standard,
    /// Width at or above [`WIDE_MIN_WIDTH`].
    Wide,
}

impl SizeClass {
    /// Classify a window width into a size class.
    ///
    /// Total and defensive: non-finite or negative widths fall back to
    /// `Compact` so degenerate geometry never panics downstream layout code.
    #[must_use]
    pub fn from_width(width: f32) -> Self {
        if !width.is_finite() || width < 0.0 {
            return Self::Compact;
        }
        if width < COMPACT_MAX_WIDTH {
            Self::Compact
        } else if width < WIDE_MIN_WIDTH {
            Self::Standard
        } else {
            Self::Wide
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_width_boundary_table() {
        let cases: &[(f32, SizeClass)] = &[
            (0.0, SizeClass::Compact),
            (1024.0, SizeClass::Compact),
            (1279.99, SizeClass::Compact),
            (1280.0, SizeClass::Standard),
            (1280.01, SizeClass::Standard),
            (1600.0, SizeClass::Standard),
            (1919.99, SizeClass::Standard),
            (1920.0, SizeClass::Wide),
            (2560.0, SizeClass::Wide),
        ];
        for &(width, expected) in cases {
            assert_eq!(
                SizeClass::from_width(width),
                expected,
                "width {width} should be {expected:?}"
            );
        }
    }

    #[test]
    fn from_width_degenerate_values_fall_back_to_compact() {
        assert_eq!(SizeClass::from_width(f32::NAN), SizeClass::Compact);
        assert_eq!(SizeClass::from_width(f32::INFINITY), SizeClass::Compact);
        assert_eq!(SizeClass::from_width(f32::NEG_INFINITY), SizeClass::Compact);
        assert_eq!(SizeClass::from_width(-1.0), SizeClass::Compact);
    }
}
