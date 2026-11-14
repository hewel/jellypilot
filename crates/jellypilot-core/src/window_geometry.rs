//! Logical-pixel window sizing, independent of the native windowing toolkit.

use crate::config::AppMode;

pub const FULL_DEFAULT_SIZE: (f32, f32) = (1760.0, 900.0);
pub const CONTROL_ONLY_SIZE: (f32, f32) = (480.0, 760.0);
/// Reserve space on each edge for native decorations and desktop controls.
/// Monitor dimensions are not a platform work-area measurement.
pub const MONITOR_EDGE_FRACTION: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub size: (f32, f32),
    pub min_size: Option<(f32, f32)>,
    pub max_size: Option<(f32, f32)>,
    pub resizable: bool,
}

impl WindowGeometry {
    pub fn for_mode(mode: AppMode, restore_size: Option<(f32, f32)>) -> Self {
        match mode {
            AppMode::ControlOnly => Self {
                size: CONTROL_ONLY_SIZE,
                min_size: Some(CONTROL_ONLY_SIZE),
                max_size: Some(CONTROL_ONLY_SIZE),
                resizable: false,
            },
            AppMode::Full => Self {
                size: restore_size.unwrap_or(FULL_DEFAULT_SIZE),
                min_size: Some((1024.0, 640.0)),
                max_size: None,
                resizable: true,
            },
        }
    }

    /// Fits initial/restored geometry to the current monitor's logical dimensions.
    /// The caller must convert physical dimensions using the monitor's DPI first.
    /// Missing monitor information leaves sizing to the native compositor.
    pub fn fit_to_monitor(mut self, monitor: Option<(f32, f32)>) -> Self {
        let Some((width, height)) = monitor.filter(|(width, height)| {
            width.is_finite() && height.is_finite() && *width > 0.0 && *height > 0.0
        }) else {
            return self;
        };
        let fraction = 1.0 - 2.0 * MONITOR_EDGE_FRACTION;
        let available = (
            (width * fraction).floor().max(1.0),
            (height * fraction).floor().max(1.0),
        );
        let fit = |size: (f32, f32)| (size.0.min(available.0), size.1.min(available.1));
        self.min_size = self.min_size.map(fit);
        self.max_size = self.max_size.map(fit);
        let minimum = self.min_size.unwrap_or((1.0, 1.0));
        self.size = fit((self.size.0.max(minimum.0), self.size.1.max(minimum.1)));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_236_scaled_display_fits_both_modes_without_oversized_minimums() {
        // The reported 1920×1080 display at 150% is 1280×720 logical pixels.
        let monitor = (1920.0 / 1.5, 1080.0 / 1.5);
        for mode in [AppMode::Full, AppMode::ControlOnly] {
            let geometry = WindowGeometry::for_mode(mode, None).fit_to_monitor(Some(monitor));
            assert!(geometry.size.0 < monitor.0 && geometry.size.1 < monitor.1);
            let minimum = geometry.min_size.unwrap();
            assert!(minimum.0 <= geometry.size.0 && minimum.1 <= geometry.size.1);
            if mode == AppMode::ControlOnly {
                assert_eq!(geometry.min_size, Some(geometry.size));
                assert_eq!(geometry.max_size, Some(geometry.size));
                assert!(!geometry.resizable);
            } else {
                // Fitting startup must not cap future user resizing/maximization.
                assert_eq!(geometry.max_size, None);
                assert!(geometry.resizable);
            }
        }
    }

    #[test]
    fn restored_large_window_and_minimum_fit_a_smaller_high_dpi_monitor() {
        let geometry = WindowGeometry::for_mode(AppMode::Full, Some((2400.0, 1300.0)))
            .fit_to_monitor(Some((960.0, 540.0)));
        assert!(geometry.size.0 < 960.0 && geometry.size.1 < 540.0);
        let minimum = geometry.min_size.unwrap();
        assert!(minimum.0 <= geometry.size.0 && minimum.1 <= geometry.size.1);
    }

    #[test]
    fn fitting_preserves_sizes_that_already_fit_and_does_not_invent_monitor_data() {
        for mode in [AppMode::Full, AppMode::ControlOnly] {
            let geometry = WindowGeometry::for_mode(mode, Some((1400.0, 850.0)));
            assert_eq!(geometry.fit_to_monitor(Some((2560.0, 1440.0))), geometry);
            assert_eq!(geometry.fit_to_monitor(None), geometry);
            assert_eq!(geometry.fit_to_monitor(Some((0.0, f32::NAN))), geometry);
        }
    }
}
