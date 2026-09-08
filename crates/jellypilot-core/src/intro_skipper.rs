//! Stateful Intro Skipper decisions, independent of playback command execution.

use std::time::{Duration, Instant};

use jellypilot_media_server::{IntroSkipKind, IntroSkipRange};

const PROMPT_DURATION_MS: u32 = 3_000;

/// Intro Skipper behavior for playback observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroSkipMode {
    Automatic,
    Manual,
    Off,
}

/// Opaque correlation for a single prompt presentation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntroPromptToken(u64);

/// Playback input relevant to skip eligibility.
#[derive(Debug, Clone, Copy)]
pub enum IntroSkipInput {
    Position,
    ManualSkip,
}

/// An action whose eligibility has already been consumed by the policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntroSkipAction {
    Seek(f64),
    ShowPrompt {
        token: IntroPromptToken,
        duration_ms: u32,
    },
    ManualSkip(f64),
}

struct RangeState {
    range: IntroSkipRange,
    consumed: bool,
    notified: bool,
}

struct PendingPrompt {
    token: IntroPromptToken,
    range: usize,
}

struct ActivePrompt {
    range: usize,
    expires_at: Instant,
}

/// Owns range consumption and the pending/live manual prompt lifecycle.
/// Create a fresh policy for each playback session; controller epochs stay with the caller.
pub struct IntroSkipper {
    mode: IntroSkipMode,
    ranges: Vec<RangeState>,
    pending_prompt: Option<PendingPrompt>,
    active_prompt: Option<ActivePrompt>,
    prompt_sequence: u64,
}

impl Default for IntroSkipper {
    fn default() -> Self {
        Self::new(IntroSkipMode::Off)
    }
}

impl IntroSkipper {
    pub fn new(mode: IntroSkipMode) -> Self {
        Self {
            mode,
            ranges: Vec::new(),
            pending_prompt: None,
            active_prompt: None,
            prompt_sequence: 0,
        }
    }

    pub fn mode(&self) -> IntroSkipMode {
        self.mode
    }

    /// Off forgets fetched ranges. Re-enabling does not restore or refetch them.
    pub fn set_mode(&mut self, mode: IntroSkipMode) {
        self.mode = mode;
        if mode == IntroSkipMode::Off {
            self.ranges.clear();
            self.dismiss_prompt();
            self.pending_prompt = None;
        }
    }

    /// Accept a fetched range set after the caller has rejected stale fetch results.
    pub fn replace_ranges(&mut self, ranges: Vec<IntroSkipRange>) {
        self.dismiss_prompt();
        self.pending_prompt = None;
        self.ranges = if self.mode == IntroSkipMode::Off {
            Vec::new()
        } else {
            ranges
                .into_iter()
                .map(|range| RangeState {
                    range,
                    consumed: false,
                    notified: false,
                })
                .collect()
        };
    }

    /// Consume an eligible attempt at issuance, regardless of eventual seek success.
    /// Ranges are considered in supplied order, with exact-start inclusive/end exclusive bounds.
    pub fn observe(
        &mut self,
        position: f64,
        now: Instant,
        input: IntroSkipInput,
    ) -> Option<IntroSkipAction> {
        self.advance_time(now);
        if self.mode == IntroSkipMode::Off || !position.is_finite() {
            return None;
        }
        let index = self.ranges.iter().position(|state| {
            !state.consumed
                && position >= state.range.start_seconds
                && position < state.range.end_seconds
        })?;
        let state = &mut self.ranges[index];
        if matches!(input, IntroSkipInput::ManualSkip) {
            if self.mode != IntroSkipMode::Manual
                || self
                    .active_prompt
                    .as_ref()
                    .is_none_or(|prompt| prompt.range != index)
            {
                return None;
            }
            state.consumed = true;
            state.notified = true;
            self.active_prompt = None;
            return Some(IntroSkipAction::ManualSkip(state.range.end_seconds));
        }
        match self.mode {
            IntroSkipMode::Automatic => {
                state.consumed = true;
                state.notified = true;
                Some(IntroSkipAction::Seek(state.range.end_seconds))
            }
            IntroSkipMode::Manual if !state.notified => {
                state.notified = true;
                self.prompt_sequence = self.prompt_sequence.wrapping_add(1);
                let token = IntroPromptToken(self.prompt_sequence);
                self.pending_prompt = Some(PendingPrompt {
                    token,
                    range: index,
                });
                Some(IntroSkipAction::ShowPrompt {
                    token,
                    duration_ms: PROMPT_DURATION_MS,
                })
            }
            IntroSkipMode::Manual | IntroSkipMode::Off => None,
        }
    }

    /// Only successful presentation of the still-pending prompt makes it live.
    /// Its lifetime starts at settlement, not when the presentation was requested.
    pub fn prompt_settled(&mut self, token: IntroPromptToken, presented: bool, now: Instant) {
        if self
            .pending_prompt
            .as_ref()
            .is_none_or(|pending| pending.token != token)
        {
            return;
        }
        let Some(pending) = self.pending_prompt.take() else {
            return;
        };
        if presented && self.mode == IntroSkipMode::Manual && !self.ranges[pending.range].consumed {
            self.active_prompt = Some(ActivePrompt {
                range: pending.range,
                expires_at: now + Duration::from_millis(u64::from(PROMPT_DURATION_MS)),
            });
        }
    }

    pub fn advance_time(&mut self, now: Instant) {
        if self
            .active_prompt
            .as_ref()
            .is_some_and(|prompt| now >= prompt.expires_at)
        {
            self.active_prompt = None;
        }
    }

    /// Dismiss the presented prompt without rearming its range.
    pub fn dismiss_prompt(&mut self) {
        self.active_prompt = None;
    }

    /// Current presented prompt, after the most recent time observation.
    pub fn prompt_kind(&self) -> Option<IntroSkipKind> {
        self.active_prompt
            .as_ref()
            .map(|prompt| self.ranges[prompt.range].range.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(kind: IntroSkipKind, start_seconds: f64, end_seconds: f64) -> IntroSkipRange {
        IntroSkipRange {
            kind,
            start_seconds,
            end_seconds,
        }
    }

    fn policy(mode: IntroSkipMode) -> IntroSkipper {
        let mut policy = IntroSkipper::new(mode);
        policy.replace_ranges(vec![range(IntroSkipKind::Introduction, 10.0, 30.0)]);
        policy
    }

    fn prompt(policy: &mut IntroSkipper, now: Instant, position: f64) -> IntroPromptToken {
        let Some(IntroSkipAction::ShowPrompt { token, .. }) =
            policy.observe(position, now, IntroSkipInput::Position)
        else {
            panic!("expected prompt presentation");
        };
        token
    }

    #[test]
    fn eligibility_uses_exact_half_open_bounds_and_finite_positions() {
        for (position, expected) in [
            (9.0, None),
            (9.999, None),
            (10.0, Some(IntroSkipAction::Seek(30.0))),
            (29.999, Some(IntroSkipAction::Seek(30.0))),
            (30.0, None),
            (f64::NAN, None),
            (f64::INFINITY, None),
            (f64::NEG_INFINITY, None),
        ] {
            assert_eq!(
                policy(IntroSkipMode::Automatic).observe(
                    position,
                    Instant::now(),
                    IntroSkipInput::Position
                ),
                expected
            );
        }
    }

    #[test]
    fn automatic_attempt_is_consumed_without_success_and_seek_back_never_rearms_it() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Automatic);
        policy.replace_ranges(vec![
            range(IntroSkipKind::Introduction, 10.0, 30.0),
            range(IntroSkipKind::Introduction, 50.0, 60.0),
        ]);
        assert_eq!(
            policy.observe(20.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
        // No success is reported: even a failed or unexecuted seek remains consumed.
        assert_eq!(policy.observe(20.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(0.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
        assert_eq!(
            policy.observe(50.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(60.0))
        );
        assert_eq!(policy.observe(50.0, now, IntroSkipInput::Position), None);
    }

    #[test]
    fn first_matching_range_retains_priority_until_consumed() {
        let now = Instant::now();
        let mut policy = IntroSkipper::new(IntroSkipMode::Manual);
        policy.replace_ranges(vec![
            range(IntroSkipKind::Introduction, 10.0, 30.0),
            range(IntroSkipKind::Credits, 20.0, 40.0),
        ]);
        let token = prompt(&mut policy, now, 20.0);
        policy.prompt_settled(token, false, now);
        assert_eq!(policy.observe(20.0, now, IntroSkipInput::Position), None);
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(
            policy.observe(20.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
        assert_eq!(
            policy.observe(20.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(40.0))
        );
    }

    #[test]
    fn manual_prompts_and_consumption_are_independent_for_repeated_segment_types() {
        let now = Instant::now();
        let mut policy = IntroSkipper::new(IntroSkipMode::Manual);
        policy.replace_ranges(vec![
            range(IntroSkipKind::Introduction, 10.0, 30.0),
            range(IntroSkipKind::Introduction, 50.0, 60.0),
        ]);
        let first = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(first, true, now);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
        let second = prompt(&mut policy, now, 50.0);
        policy.prompt_settled(second, true, now);
        assert_eq!(
            policy.observe(50.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(60.0))
        );
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(50.0, now, IntroSkipInput::Position), None);
    }

    #[test]
    fn manual_skip_requires_successful_presentation_and_the_current_range() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::ManualSkip), None);
        policy.prompt_settled(token, true, now);
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Introduction));
        assert_eq!(policy.observe(30.0, now, IntroSkipInput::ManualSkip), None);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
        assert_eq!(policy.prompt_kind(), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::ManualSkip), None);
    }

    #[test]
    fn failed_prompt_does_not_enable_manual_skip_or_retry_presentation() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, false, now);
        policy.prompt_settled(token, true, now);
        assert_eq!(policy.prompt_kind(), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::ManualSkip), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
    }

    #[test]
    fn prompt_deadline_begins_at_presentation_and_expires_at_exact_deadline() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        let presented_at = now + Duration::from_secs(5);
        policy.prompt_settled(token, true, presented_at);
        policy.advance_time(presented_at + Duration::from_millis(2_999));
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Introduction));
        assert_eq!(
            policy.observe(
                10.0,
                presented_at + Duration::from_secs(3),
                IntroSkipInput::ManualSkip
            ),
            None
        );
        assert_eq!(policy.prompt_kind(), None);
    }

    #[test]
    fn dismissal_does_not_rearm_the_presented_range() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        policy.dismiss_prompt();
        assert_eq!(policy.prompt_kind(), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::ManualSkip), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
    }

    #[test]
    fn off_clears_ranges_and_rejects_delayed_presentation_after_reenabling() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.set_mode(IntroSkipMode::Off);
        policy.set_mode(IntroSkipMode::Manual);
        policy.prompt_settled(token, true, now);
        assert_eq!(policy.prompt_kind(), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
    }

    #[test]
    fn replacement_ranges_reject_old_prompt_without_discarding_new_pending_prompt() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Manual);
        let old = prompt(&mut policy, now, 10.0);
        policy.replace_ranges(vec![range(IntroSkipKind::Credits, 10.0, 50.0)]);
        let current = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(old, true, now);
        assert_eq!(policy.prompt_kind(), None);
        policy.prompt_settled(current, true, now);
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Credits));
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(50.0))
        );
    }

    #[test]
    fn non_off_mode_changes_preserve_consumption_and_live_prompt_lifetime() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Automatic);
        policy.set_mode(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::ManualSkip), None);
        policy.set_mode(IntroSkipMode::Manual);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
    }

    #[test]
    fn pending_presentation_uses_mode_at_settlement_without_rearming() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.set_mode(IntroSkipMode::Automatic);
        policy.prompt_settled(token, true, now);
        policy.set_mode(IntroSkipMode::Manual);
        assert_eq!(policy.prompt_kind(), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::ManualSkip), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
    }

    #[test]
    fn prompt_for_another_range_cannot_authorize_manual_skip() {
        let now = Instant::now();
        let mut policy = IntroSkipper::new(IntroSkipMode::Manual);
        policy.replace_ranges(vec![
            range(IntroSkipKind::Introduction, 10.0, 30.0),
            range(IntroSkipKind::Credits, 40.0, 60.0),
        ]);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        assert_eq!(policy.observe(40.0, now, IntroSkipInput::ManualSkip), None);
        let credits = prompt(&mut policy, now, 40.0);
        policy.prompt_settled(credits, true, now);
        assert_eq!(
            policy.observe(40.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(60.0))
        );
    }
}
