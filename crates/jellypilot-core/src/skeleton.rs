//! Display-free skeleton shimmer phase math shared by the frontend shells.

use std::time::Duration;

/// Breathing pulse phase in [0, 1): one full pulse per 1600ms, matching
/// `tokens.durations.ms1600` in jellypilot-ui.
#[must_use]
pub fn skeleton_phase_at(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / Duration::from_millis(1600).as_secs_f32()).fract()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_phase_wraps_once_per_1600ms() {
        assert_eq!(skeleton_phase_at(Duration::from_millis(0)), 0.0);
        assert_eq!(skeleton_phase_at(Duration::from_millis(800)), 0.5);
        assert_eq!(skeleton_phase_at(Duration::from_millis(1600)), 0.0);
        assert_eq!(skeleton_phase_at(Duration::from_millis(2000)), 0.25);
    }
}
