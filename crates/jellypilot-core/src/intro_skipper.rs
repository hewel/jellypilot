//! Stateful Intro Skipper decisions, independent of playback command execution.

use std::sync::Arc;
use std::time::{Duration, Instant};

use jellypilot_media_server::{IntroSkipKind, IntroSkipRange};

const PROMPT_DURATION_MS: u32 = 3_000;

/// Whole-range prompts are retired by the policy, not the presenter. MPV's
/// show-text duration is a signed 32-bit millisecond value.
const PERSISTENT_PROMPT_DURATION_MS: u32 = i32::MAX as u32;

/// Intro Skipper behavior for playback observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroSkipMode {
    Automatic,
    Manual,
    Off,
}

/// How a presented manual prompt ages and rearms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualPromptPolicy {
    /// Legacy contract: a prompt expires three seconds after presentation and
    /// each range offers it at most once per session.
    Timed,
    /// Desktop contract (ADR 0045): a prompt stays live for the whole
    /// continuous stay inside its range. Dismissal and use suppress only the
    /// current stay; leaving the range rearms it.
    WholeRange,
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
    /// The automatic attempt was issued; seeking back never rearms it.
    consumed: bool,
    /// Timed policy: the one prompt offer per session was already made.
    notified: bool,
    /// WholeRange policy: dismissal or use suppresses the current stay only.
    suppressed: bool,
    /// The most recent observation was inside this range.
    inside: bool,
    user_seek: Option<UserSeekStay>,
}

#[derive(Clone, Copy)]
enum UserSeekStay {
    AwaitingEntry,
    Inside,
}

struct PendingPrompt {
    token: IntroPromptToken,
    range: usize,
}

struct ActivePrompt {
    range: usize,
    /// Timed policy deadline; `None` while a whole-range stay keeps the
    /// prompt alive.
    expires_at: Option<Instant>,
}

/// Owns range consumption and the pending/live manual prompt lifecycle.
/// Create a fresh policy for each playback session; controller epochs stay with the caller.
pub struct IntroSkipper {
    mode: IntroSkipMode,
    manual_policy: ManualPromptPolicy,
    ranges: Vec<RangeState>,
    /// The real fetched ranges behind `ranges`; never synthesized.
    intro_ranges: Arc<[IntroSkipRange]>,
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
    /// Legacy policy: timed prompts, preserved for callers that have not
    /// migrated to the desktop contract.
    pub fn new(mode: IntroSkipMode) -> Self {
        Self::with_manual_policy(mode, ManualPromptPolicy::Timed)
    }

    pub fn with_manual_policy(mode: IntroSkipMode, manual_policy: ManualPromptPolicy) -> Self {
        Self {
            mode,
            manual_policy,
            ranges: Vec::new(),
            intro_ranges: Arc::from([]),
            pending_prompt: None,
            active_prompt: None,
            prompt_sequence: 0,
        }
    }

    pub fn mode(&self) -> IntroSkipMode {
        self.mode
    }

    /// The real fetched ranges accepted by `replace_ranges`; empty while Off
    /// or before a fetch settles.
    pub fn ranges(&self) -> Arc<[IntroSkipRange]> {
        Arc::clone(&self.intro_ranges)
    }

    /// Off forgets fetched ranges. Re-enabling does not restore or refetch them.
    pub fn set_mode(&mut self, mode: IntroSkipMode) {
        if mode == IntroSkipMode::Off
            || (self.manual_policy == ManualPromptPolicy::WholeRange && self.mode != mode)
        {
            self.active_prompt = None;
            self.pending_prompt = None;
        }
        self.mode = mode;
        if mode == IntroSkipMode::Off {
            self.ranges.clear();
            self.intro_ranges = Arc::from([]);
        }
    }

    /// Accept a fetched range set after the caller has rejected stale fetch results.
    pub fn replace_ranges(&mut self, ranges: Vec<IntroSkipRange>) {
        self.dismiss_prompt();
        self.pending_prompt = None;
        if self.mode == IntroSkipMode::Off {
            self.ranges = Vec::new();
            self.intro_ranges = Arc::from([]);
        } else {
            self.intro_ranges = Arc::from(ranges.as_slice());
            self.ranges = ranges
                .into_iter()
                .map(|range| RangeState {
                    range,
                    consumed: false,
                    notified: false,
                    suppressed: false,
                    inside: false,
                    user_seek: None,
                })
                .collect();
        }
    }

    /// Preserve an explicit seek into every range containing its target.
    /// Automatic skipping stays suppressed until entry is observed and then
    /// departure is observed. Earlier out-of-range samples cannot end that stay.
    /// A new seek replaces this suppression without consuming automatic attempts
    /// or changing manual prompts or the selected mode. Invalid targets do nothing.
    pub fn note_user_seek(&mut self, position: f64) {
        if !position.is_finite() || position < 0.0 {
            return;
        }
        for state in &mut self.ranges {
            state.user_seek = (position >= state.range.start_seconds
                && position < state.range.end_seconds)
                .then_some(UserSeekStay::AwaitingEntry);
        }
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
        self.update_stays(position);
        let index = self.eligible_range(position, input)?;
        if matches!(input, IntroSkipInput::ManualSkip) {
            return self.manual_skip(index);
        }
        match self.mode {
            IntroSkipMode::Automatic => {
                let state = &mut self.ranges[index];
                state.consumed = true;
                state.notified = true;
                Some(IntroSkipAction::Seek(state.range.end_seconds))
            }
            IntroSkipMode::Manual => {
                if self.manual_policy == ManualPromptPolicy::Timed && self.ranges[index].notified {
                    return None;
                }
                self.ranges[index].notified = true;
                self.prompt_sequence = self.prompt_sequence.wrapping_add(1);
                let token = IntroPromptToken(self.prompt_sequence);
                self.pending_prompt = Some(PendingPrompt {
                    token,
                    range: index,
                });
                Some(IntroSkipAction::ShowPrompt {
                    token,
                    duration_ms: match self.manual_policy {
                        ManualPromptPolicy::Timed => PROMPT_DURATION_MS,
                        ManualPromptPolicy::WholeRange => PERSISTENT_PROMPT_DURATION_MS,
                    },
                })
            }
            IntroSkipMode::Off => None,
        }
    }

    /// The range a position observation or manual skip applies to.
    fn eligible_range(&self, position: f64, input: IntroSkipInput) -> Option<usize> {
        let contains = |state: &RangeState| {
            position >= state.range.start_seconds && position < state.range.end_seconds
        };
        if matches!(input, IntroSkipInput::ManualSkip) {
            if self.manual_policy == ManualPromptPolicy::WholeRange {
                return self.active_prompt.as_ref().and_then(|prompt| {
                    contains(&self.ranges[prompt.range]).then_some(prompt.range)
                });
            }
            return self
                .ranges
                .iter()
                .position(|state| !state.consumed && contains(state));
        }
        if self.mode == IntroSkipMode::Manual
            && self.manual_policy == ManualPromptPolicy::WholeRange
        {
            // One prompt at a time; stay-scoped suppression does not block
            // another overlapping range or consume its automatic attempt.
            if self.pending_prompt.is_some() || self.active_prompt.is_some() {
                return None;
            }
            return self
                .ranges
                .iter()
                .position(|state| !state.suppressed && contains(state));
        }
        self.ranges.iter().position(|state| {
            !state.consumed
                && contains(state)
                && (self.mode != IntroSkipMode::Automatic || state.user_seek.is_none())
        })
    }

    fn manual_skip(&mut self, index: usize) -> Option<IntroSkipAction> {
        if self.mode != IntroSkipMode::Manual
            || self
                .active_prompt
                .as_ref()
                .is_none_or(|prompt| prompt.range != index)
        {
            return None;
        }
        let state = &mut self.ranges[index];
        match self.manual_policy {
            // Legacy: using the prompt consumes the range for the session.
            ManualPromptPolicy::Timed => {
                state.consumed = true;
                state.notified = true;
            }
            // Desktop: the skip suppresses only the current stay; the range
            // rearms on re-entry and keeps its automatic attempt.
            ManualPromptPolicy::WholeRange => state.suppressed = true,
        }
        self.active_prompt = None;
        Some(IntroSkipAction::ManualSkip(state.range.end_seconds))
    }

    /// Record an authoritative playback position without issuing skip actions.
    /// Confirmed seeks must update stays even between periodic observations.
    pub fn update_stays(&mut self, position: f64) {
        if !position.is_finite() {
            return;
        }
        for index in 0..self.ranges.len() {
            let inside = position >= self.ranges[index].range.start_seconds
                && position < self.ranges[index].range.end_seconds;
            self.ranges[index].user_seek = match (self.ranges[index].user_seek, inside) {
                (Some(UserSeekStay::AwaitingEntry), true) => Some(UserSeekStay::Inside),
                (Some(UserSeekStay::Inside), false) => None,
                (stay, _) => stay,
            };
            if self.ranges[index].inside == inside {
                continue;
            }
            self.ranges[index].inside = inside;
            if inside || self.manual_policy != ManualPromptPolicy::WholeRange {
                continue;
            }
            self.ranges[index].suppressed = false;
            if self
                .active_prompt
                .as_ref()
                .is_some_and(|prompt| prompt.range == index)
            {
                self.active_prompt = None;
            }
            if self
                .pending_prompt
                .as_ref()
                .is_some_and(|prompt| prompt.range == index)
            {
                self.pending_prompt = None;
            }
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
        let state = &self.ranges[pending.range];
        let live = presented
            && self.mode == IntroSkipMode::Manual
            && match self.manual_policy {
                ManualPromptPolicy::Timed => !state.consumed,
                ManualPromptPolicy::WholeRange => state.inside && !state.suppressed,
            };
        if live {
            self.active_prompt = Some(ActivePrompt {
                range: pending.range,
                expires_at: (self.manual_policy == ManualPromptPolicy::Timed)
                    .then(|| now + Duration::from_millis(u64::from(PROMPT_DURATION_MS))),
            });
        }
    }

    pub fn advance_time(&mut self, now: Instant) {
        if self
            .active_prompt
            .as_ref()
            .is_some_and(|prompt| prompt.expires_at.is_some_and(|deadline| now >= deadline))
        {
            self.active_prompt = None;
        }
    }

    /// Dismiss the presented prompt. Whole-range suppression lasts only for
    /// the current stay; timed prompts stay spent for the session.
    pub fn dismiss_prompt(&mut self) {
        if let Some(active) = self.active_prompt.take() {
            self.ranges[active.range].suppressed = true;
        }
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

    fn desktop(mode: IntroSkipMode) -> IntroSkipper {
        let mut policy = IntroSkipper::with_manual_policy(mode, ManualPromptPolicy::WholeRange);
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
    fn user_seek_suppresses_all_containing_ranges_until_each_observed_departure() {
        let now = Instant::now();
        let mut policy = IntroSkipper::new(IntroSkipMode::Automatic);
        policy.replace_ranges(vec![
            range(IntroSkipKind::Introduction, 10.0, 30.0),
            range(IntroSkipKind::Credits, 20.0, 40.0),
            range(IntroSkipKind::Credits, 50.0, 60.0),
        ]);
        policy.note_user_seek(25.0);
        // Queued pre-seek positions cannot clear suppression before arrival.
        assert_eq!(policy.observe(0.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(5.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(25.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(26.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(35.0, now, IntroSkipInput::Position), None);
        // Leaving the intro rearms it without rearming overlapping credits.
        assert_eq!(
            policy.observe(25.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
        assert_eq!(policy.observe(25.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(40.0, now, IntroSkipInput::Position), None);
        assert_eq!(
            policy.observe(25.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(40.0))
        );
        assert_eq!(
            policy.observe(50.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(60.0))
        );
        assert_eq!(policy.mode(), IntroSkipMode::Automatic);
    }

    #[test]
    fn newer_user_seek_replaces_pending_suppression_without_spending_either_range() {
        let now = Instant::now();
        let mut policy = policy(IntroSkipMode::Automatic);
        policy.replace_ranges(vec![
            range(IntroSkipKind::Introduction, 10.0, 30.0),
            range(IntroSkipKind::Credits, 50.0, 60.0),
        ]);
        policy.note_user_seek(20.0);
        policy.note_user_seek(55.0);
        assert_eq!(
            policy.observe(20.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
        assert_eq!(policy.observe(55.0, now, IntroSkipInput::Position), None);
        policy.note_user_seek(40.0);
        assert_eq!(
            policy.observe(55.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(60.0))
        );
    }

    #[test]
    fn user_seek_keeps_manual_prompts_and_explicit_skipping_available() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        policy.note_user_seek(15.0);
        let token = prompt(&mut policy, now, 15.0);
        policy.prompt_settled(token, true, now);
        assert_eq!(
            policy.observe(15.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(policy.observe(15.0, now, IntroSkipInput::Position), None);
        policy.update_stays(30.0);
        assert_eq!(
            policy.observe(15.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
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

    #[test]
    fn whole_range_prompt_outlives_the_timed_deadline_while_inside() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        policy.advance_time(now + Duration::from_secs(600));
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Introduction));
        assert_eq!(
            policy.observe(
                10.0,
                now + Duration::from_secs(600),
                IntroSkipInput::ManualSkip
            ),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
    }

    #[test]
    fn whole_range_exit_retires_the_prompt_and_reentry_rearms_it() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        assert_eq!(policy.observe(35.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.prompt_kind(), None);
        let rearmed = prompt(&mut policy, now, 15.0);
        policy.prompt_settled(rearmed, true, now);
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Introduction));
    }

    #[test]
    fn whole_range_dismissal_and_use_suppress_only_the_current_stay() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        policy.dismiss_prompt();
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(35.0, now, IntroSkipInput::Position), None);
        // Re-entry rearms after a dismissal.
        let rearmed = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(rearmed, true, now);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
        // A used prompt also suppresses only until the stay ends.
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
        assert_eq!(policy.observe(35.0, now, IntroSkipInput::Position), None);
        assert!(matches!(
            policy.observe(10.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::ShowPrompt { .. })
        ));
    }

    #[test]
    fn whole_range_manual_skip_preserves_the_automatic_attempt() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
        assert_eq!(policy.observe(35.0, now, IntroSkipInput::Position), None);
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
    }

    #[test]
    fn whole_range_pending_prompt_dies_when_its_range_is_left() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        assert_eq!(policy.observe(35.0, now, IntroSkipInput::Position), None);
        // The late settlement is rejected: the stay it belonged to is over.
        policy.prompt_settled(token, true, now);
        assert_eq!(policy.prompt_kind(), None);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::ManualSkip), None);
        // Re-entry issues a fresh presentation rather than reviving the stale one.
        let rearmed = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(rearmed, true, now);
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Introduction));
    }

    #[test]
    fn whole_range_failed_presentation_retries_while_the_stay_continues() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, false, now);
        let retried = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(retried, true, now);
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Introduction));
    }

    #[test]
    fn whole_range_suppressed_range_never_blocks_an_overlapping_range() {
        let now = Instant::now();
        let mut policy =
            IntroSkipper::with_manual_policy(IntroSkipMode::Manual, ManualPromptPolicy::WholeRange);
        policy.replace_ranges(vec![
            range(IntroSkipKind::Introduction, 10.0, 30.0),
            range(IntroSkipKind::Credits, 20.0, 40.0),
        ]);
        let token = prompt(&mut policy, now, 25.0);
        policy.prompt_settled(token, true, now);
        policy.dismiss_prompt();
        // The overlapping credits range still offers its own prompt.
        let credits = prompt(&mut policy, now, 25.0);
        policy.prompt_settled(credits, true, now);
        assert_eq!(policy.prompt_kind(), Some(IntroSkipKind::Credits));
        assert_eq!(
            policy.observe(25.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(40.0))
        );
    }

    #[test]
    fn off_forgets_live_prompts_under_both_manual_policies() {
        let now = Instant::now();
        for mut policy in [
            desktop(IntroSkipMode::Manual),
            policy(IntroSkipMode::Manual),
        ] {
            let token = prompt(&mut policy, now, 10.0);
            policy.prompt_settled(token, true, now);
            policy.set_mode(IntroSkipMode::Off);
            assert_eq!(policy.prompt_kind(), None);
            assert!(policy.ranges().is_empty());
            policy.set_mode(IntroSkipMode::Manual);
            assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
        }
    }

    #[test]
    fn desktop_mode_changes_retire_prompts_but_keep_manual_and_automatic_attempts_independent() {
        let now = Instant::now();
        let mut policy = desktop(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(policy.prompt_kind(), None);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::Position),
            Some(IntroSkipAction::Seek(30.0))
        );
        // Even if that automatic seek failed, Manual can offer an action.
        policy.set_mode(IntroSkipMode::Manual);
        let token = prompt(&mut policy, now, 10.0);
        policy.prompt_settled(token, true, now);
        assert_eq!(
            policy.observe(10.0, now, IntroSkipInput::ManualSkip),
            Some(IntroSkipAction::ManualSkip(30.0))
        );
        policy.update_stays(30.0);
        policy.set_mode(IntroSkipMode::Automatic);
        assert_eq!(policy.observe(10.0, now, IntroSkipInput::Position), None);
    }
}
