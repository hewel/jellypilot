pub use jellypilot_media_server::{
    evaluate_manual_skip, IntroSkipDecision, IntroSkipKind, IntroSkipRange,
};
use jellypilot_media_server::{evaluate_skip, evaluate_skip_prompt};

/// Intro Skipper behavior for time-position updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroSkipMode {
    Automatic,
    Manual,
    Off,
}

/// Frontend-neutral action produced by an Intro Skipper time-position update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntroSkipAction {
    Seek(f64),
    ShowPrompt(IntroSkipKind),
}

/// Evaluate one time-position update against the active Intro Skipper ranges.
pub fn evaluate_intro_skip(
    position_seconds: f64,
    ranges: &mut [IntroSkipRange],
    mode: IntroSkipMode,
) -> Option<IntroSkipAction> {
    match mode {
        IntroSkipMode::Automatic => {
            evaluate_skip(position_seconds, ranges).map(IntroSkipAction::Seek)
        }
        IntroSkipMode::Manual => {
            evaluate_skip_prompt(position_seconds, ranges).map(IntroSkipAction::ShowPrompt)
        }
        IntroSkipMode::Off => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intro_range() -> IntroSkipRange {
        IntroSkipRange {
            kind: IntroSkipKind::Introduction,
            start_seconds: 10.0,
            end_seconds: 30.0,
            notified: false,
            skipped: false,
        }
    }

    #[test]
    fn automatic_mode_returns_seek_target() {
        let mut ranges = [intro_range()];

        let action = evaluate_intro_skip(10.0, &mut ranges, IntroSkipMode::Automatic);

        assert_eq!(action, Some(IntroSkipAction::Seek(30.0)));
    }

    #[test]
    fn manual_mode_returns_prompt_kind() {
        let mut ranges = [intro_range()];

        let action = evaluate_intro_skip(10.0, &mut ranges, IntroSkipMode::Manual);

        assert_eq!(
            action,
            Some(IntroSkipAction::ShowPrompt(IntroSkipKind::Introduction))
        );
    }

    #[test]
    fn off_mode_leaves_ranges_unchanged() {
        let mut ranges = [intro_range()];

        let action = evaluate_intro_skip(10.0, &mut ranges, IntroSkipMode::Off);

        assert_eq!(action, None);
        assert_eq!(ranges, [intro_range()]);
    }
}
