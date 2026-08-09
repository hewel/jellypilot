//! Pure embedded playback lifecycle reducer.

use crate::{
    plan_ffmpeg, BrowserObservation, BrowserPlaybackState, FfmpegCandidate, FfmpegPlan,
    MpvFallbackMetadata, MpvFallbackReason, PlaybackAction, PlaybackActionKind, PlaybackAttempt,
    PlaybackAttemptFailure, PlaybackCapabilities, PlaybackCommand, PlaybackCoreError,
    PlaybackFailure, PlaybackFailureStage, PlaybackGeneration, PlaybackObservationDisposition,
    PlaybackPhase, PlaybackReport, PlaybackSession, PlaybackSessionSummary, PlaybackSnapshot,
    PlaybackStartReason, PlaybackStopReason, PlaybackUpdate, EMBEDDED_PLAYBACK_CAPABILITIES,
};

/// Synchronous owner of metadata for one active embedded playback session.
///
/// The core performs no I/O. Consumers execute emitted [`PlaybackCommand`]s in
/// order and feed browser state back through [`PlaybackAction::BrowserObserved`].
/// Every pipeline replacement receives a new generation, while server reporting
/// remains exactly once per play/replay lifecycle.
#[derive(Clone, Debug)]
pub struct EmbeddedPlaybackCore {
    next_generation: u64,
    generation: Option<PlaybackGeneration>,
    phase: PlaybackPhase,
    session: Option<PlaybackSession>,
    plan: Option<FfmpegPlan>,
    candidate_index: usize,
    position_ticks: u64,
    paused: bool,
    last_observation_sequence: Option<u64>,
    attempt_failures: Vec<PlaybackAttemptFailure>,
    failure: Option<PlaybackFailure>,
    mpv_fallback: Option<MpvFallbackMetadata>,
    report_started: bool,
    report_stopped: bool,
    pending_stop_reason: Option<PlaybackStopReason>,
}

impl Default for EmbeddedPlaybackCore {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddedPlaybackCore {
    /// Creates an idle playback core with generation zero unissued.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_generation: 0,
            generation: None,
            phase: PlaybackPhase::Idle,
            session: None,
            plan: None,
            candidate_index: 0,
            position_ticks: 0,
            paused: false,
            last_observation_sequence: None,
            attempt_failures: Vec::new(),
            failure: None,
            mpv_fallback: None,
            report_started: false,
            report_stopped: false,
            pending_stop_reason: None,
        }
    }

    /// Returns the immutable current view without changing reducer state.
    #[must_use]
    pub fn snapshot(&self) -> PlaybackSnapshot {
        PlaybackSnapshot {
            phase: self.phase,
            generation: self.generation,
            session: self.session.as_ref().map(PlaybackSessionSummary::from),
            position_ticks: self.position_ticks,
            duration_ticks: self
                .session
                .as_ref()
                .and_then(|session| session.duration_ticks),
            paused: self.paused,
            last_observation_sequence: self.last_observation_sequence,
            active_plan: self.plan.clone(),
            active_candidate: self.active_candidate(),
            attempt_failures: self.attempt_failures.clone(),
            failure: self.failure.clone(),
            mpv_fallback: self.mpv_fallback.clone(),
            capabilities: EMBEDDED_PLAYBACK_CAPABILITIES,
        }
    }

    /// Returns transport and fallback capabilities implemented by this core.
    #[must_use]
    pub const fn capabilities(&self) -> PlaybackCapabilities {
        EMBEDDED_PLAYBACK_CAPABILITIES
    }

    /// Applies one action atomically and returns ordered commands.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackCoreError`] for invalid input, exhausted generations,
    /// or actions that are not valid in the current phase. On error, state is
    /// unchanged.
    pub fn dispatch(
        &mut self,
        action: PlaybackAction,
    ) -> Result<PlaybackUpdate, PlaybackCoreError> {
        let mut next = self.clone();
        let update = next.dispatch_inner(action)?;
        *self = next;
        Ok(update)
    }

    fn dispatch_inner(
        &mut self,
        action: PlaybackAction,
    ) -> Result<PlaybackUpdate, PlaybackCoreError> {
        let mut commands = Vec::new();
        let observation = match action {
            PlaybackAction::Play(session) => {
                self.play(session, &mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::Pause => {
                self.pause(&mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::Resume => {
                self.resume(&mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::Seek { position_ticks } => {
                self.seek(position_ticks, &mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::Restart => {
                self.restart(&mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::Stop => {
                self.stop(&mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::Replay => {
                self.replay(&mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::StartupFailed {
                generation,
                message,
            } => {
                self.startup_failed(generation, message, &mut commands)?;
                PlaybackObservationDisposition::NotObserved
            }
            PlaybackAction::BrowserObserved(observation) => {
                self.browser_observed(observation, &mut commands)
            }
        };

        Ok(PlaybackUpdate {
            snapshot: self.snapshot(),
            commands,
            observation,
        })
    }

    fn play(
        &mut self,
        session: PlaybackSession,
        commands: &mut Vec<PlaybackCommand>,
    ) -> Result<(), PlaybackCoreError> {
        validate_session(&session)?;
        if self.pipeline_is_active() {
            self.retire_pipeline(commands);
            self.close_reporting(PlaybackStopReason::Replaced, commands);
        }

        let generation = self.allocate_generation()?;
        let position_ticks = session.start_position_ticks;
        let plan_result = plan_ffmpeg(session.plan_request);

        self.generation = Some(generation);
        self.phase = PlaybackPhase::Starting;
        self.session = Some(session);
        self.plan = None;
        self.candidate_index = 0;
        self.position_ticks = position_ticks;
        self.paused = false;
        self.last_observation_sequence = None;
        self.attempt_failures.clear();
        self.failure = None;
        self.mpv_fallback = None;
        self.report_started = false;
        self.report_stopped = false;
        self.pending_stop_reason = None;

        match plan_result {
            Ok(plan) => {
                self.plan = Some(plan);
                self.push_start(PlaybackStartReason::Play, commands)?;
            }
            Err(error) => self.fail(
                PlaybackFailureStage::Planning,
                error.to_string(),
                None,
                MpvFallbackReason::UnsupportedBrowserCapabilities,
            ),
        }
        Ok(())
    }

    fn pause(&mut self, commands: &mut Vec<PlaybackCommand>) -> Result<(), PlaybackCoreError> {
        if !matches!(
            self.phase,
            PlaybackPhase::Playing | PlaybackPhase::Buffering
        ) {
            return Err(self.invalid_transition(PlaybackActionKind::Pause));
        }
        let Some(generation) = self.generation else {
            return Err(self.invalid_transition(PlaybackActionKind::Pause));
        };

        self.phase = PlaybackPhase::Paused;
        self.paused = true;
        commands.push(PlaybackCommand::SetPaused {
            generation,
            paused: true,
        });
        Ok(())
    }

    fn resume(&mut self, commands: &mut Vec<PlaybackCommand>) -> Result<(), PlaybackCoreError> {
        if self.phase != PlaybackPhase::Paused {
            return Err(self.invalid_transition(PlaybackActionKind::Resume));
        }
        let Some(generation) = self.generation else {
            return Err(self.invalid_transition(PlaybackActionKind::Resume));
        };

        self.phase = PlaybackPhase::Buffering;
        self.paused = false;
        commands.push(PlaybackCommand::SetPaused {
            generation,
            paused: false,
        });
        Ok(())
    }

    fn seek(
        &mut self,
        position_ticks: u64,
        commands: &mut Vec<PlaybackCommand>,
    ) -> Result<(), PlaybackCoreError> {
        if !matches!(
            self.phase,
            PlaybackPhase::Starting
                | PlaybackPhase::Playing
                | PlaybackPhase::Paused
                | PlaybackPhase::Buffering
                | PlaybackPhase::Seeking
                | PlaybackPhase::Restarting
        ) {
            return Err(self.invalid_transition(PlaybackActionKind::Seek));
        }
        self.validate_position(position_ticks)?;
        self.replace_pipeline(
            position_ticks,
            self.paused,
            PlaybackPhase::Seeking,
            PlaybackStartReason::Seek,
            commands,
        )
    }

    fn restart(&mut self, commands: &mut Vec<PlaybackCommand>) -> Result<(), PlaybackCoreError> {
        if !matches!(
            self.phase,
            PlaybackPhase::Starting
                | PlaybackPhase::Playing
                | PlaybackPhase::Paused
                | PlaybackPhase::Buffering
                | PlaybackPhase::Seeking
                | PlaybackPhase::Restarting
                | PlaybackPhase::Failed
        ) {
            return Err(self.invalid_transition(PlaybackActionKind::Restart));
        }
        let reset_reporting = self.phase == PlaybackPhase::Failed;
        self.restart_retained(
            self.position_ticks,
            self.paused,
            PlaybackPhase::Restarting,
            PlaybackStartReason::Restart,
            reset_reporting,
            commands,
        )
    }

    fn stop(&mut self, commands: &mut Vec<PlaybackCommand>) -> Result<(), PlaybackCoreError> {
        if !self.pipeline_is_active() {
            return Err(self.invalid_transition(PlaybackActionKind::Stop));
        }

        self.retire_pipeline(commands);
        self.close_reporting(PlaybackStopReason::User, commands);
        self.phase = PlaybackPhase::Stopping;
        self.paused = false;
        self.pending_stop_reason = Some(PlaybackStopReason::User);
        Ok(())
    }

    fn replay(&mut self, commands: &mut Vec<PlaybackCommand>) -> Result<(), PlaybackCoreError> {
        if !matches!(
            self.phase,
            PlaybackPhase::Stopped | PlaybackPhase::Ended | PlaybackPhase::Failed
        ) {
            return Err(self.invalid_transition(PlaybackActionKind::Replay));
        }
        self.restart_retained(
            0,
            false,
            PlaybackPhase::Starting,
            PlaybackStartReason::Replay,
            true,
            commands,
        )
    }

    fn restart_retained(
        &mut self,
        position_ticks: u64,
        paused: bool,
        phase: PlaybackPhase,
        reason: PlaybackStartReason,
        reset_reporting: bool,
        commands: &mut Vec<PlaybackCommand>,
    ) -> Result<(), PlaybackCoreError> {
        let Some(session) = self.session.as_ref() else {
            return Err(self.invalid_transition(match reason {
                PlaybackStartReason::Replay => PlaybackActionKind::Replay,
                PlaybackStartReason::Restart => PlaybackActionKind::Restart,
                PlaybackStartReason::Play
                | PlaybackStartReason::Seek
                | PlaybackStartReason::CandidateFallback => PlaybackActionKind::Restart,
            }));
        };
        self.validate_position(position_ticks)?;
        let plan_result = plan_ffmpeg(session.plan_request);

        if self.pipeline_is_active() {
            self.retire_pipeline(commands);
        }
        let generation = self.allocate_generation()?;
        self.generation = Some(generation);
        self.phase = phase;
        self.plan = None;
        self.candidate_index = 0;
        self.position_ticks = position_ticks;
        self.paused = paused;
        self.last_observation_sequence = None;
        self.attempt_failures.clear();
        self.failure = None;
        self.mpv_fallback = None;
        self.pending_stop_reason = None;
        if reset_reporting {
            self.report_started = false;
            self.report_stopped = false;
        }

        match plan_result {
            Ok(plan) => {
                self.plan = Some(plan);
                self.push_start(reason, commands)?;
            }
            Err(error) => self.fail(
                PlaybackFailureStage::Planning,
                error.to_string(),
                None,
                MpvFallbackReason::UnsupportedBrowserCapabilities,
            ),
        }
        Ok(())
    }

    fn replace_pipeline(
        &mut self,
        position_ticks: u64,
        paused: bool,
        phase: PlaybackPhase,
        reason: PlaybackStartReason,
        commands: &mut Vec<PlaybackCommand>,
    ) -> Result<(), PlaybackCoreError> {
        if self.plan.is_none() || self.session.is_none() {
            return Err(self.invalid_transition(PlaybackActionKind::Seek));
        }
        self.retire_pipeline(commands);
        let generation = self.allocate_generation()?;
        self.generation = Some(generation);
        self.phase = phase;
        self.candidate_index = 0;
        self.position_ticks = position_ticks;
        self.paused = paused;
        self.last_observation_sequence = None;
        self.attempt_failures.clear();
        self.failure = None;
        self.mpv_fallback = None;
        self.pending_stop_reason = None;
        self.push_start(reason, commands)
    }

    fn startup_failed(
        &mut self,
        generation: PlaybackGeneration,
        message: String,
        commands: &mut Vec<PlaybackCommand>,
    ) -> Result<(), PlaybackCoreError> {
        if self.generation != Some(generation) {
            return Ok(());
        }
        if !matches!(
            self.phase,
            PlaybackPhase::Starting | PlaybackPhase::Seeking | PlaybackPhase::Restarting
        ) {
            return Err(self.invalid_transition(PlaybackActionKind::StartupFailed));
        }
        let Some(candidate) = self.active_candidate() else {
            return Err(self.invalid_transition(PlaybackActionKind::StartupFailed));
        };
        self.attempt_failures.push(PlaybackAttemptFailure {
            generation,
            candidate,
            message: message.clone(),
        });

        let next_index = self.candidate_index.saturating_add(1);
        let has_next = self
            .plan
            .as_ref()
            .is_some_and(|plan| next_index < plan.candidates.len());
        if has_next {
            self.retire_pipeline(commands);
            let next_generation = self.allocate_generation()?;
            self.generation = Some(next_generation);
            self.candidate_index = next_index;
            self.last_observation_sequence = None;
            self.push_start(PlaybackStartReason::CandidateFallback, commands)?;
            return Ok(());
        }

        self.retire_pipeline(commands);
        self.fail(
            PlaybackFailureStage::Startup,
            message,
            Some(candidate),
            MpvFallbackReason::FfmpegCandidatesExhausted,
        );
        self.close_reporting(PlaybackStopReason::StartupFailed, commands);
        Ok(())
    }

    fn browser_observed(
        &mut self,
        observation: BrowserObservation,
        commands: &mut Vec<PlaybackCommand>,
    ) -> PlaybackObservationDisposition {
        if self.generation != Some(observation.token.generation) {
            return PlaybackObservationDisposition::IgnoredStaleGeneration;
        }
        if self
            .last_observation_sequence
            .is_some_and(|sequence| observation.token.sequence <= sequence)
        {
            return PlaybackObservationDisposition::IgnoredStaleSequence;
        }
        if matches!(
            self.phase,
            PlaybackPhase::Idle
                | PlaybackPhase::Stopped
                | PlaybackPhase::Ended
                | PlaybackPhase::Failed
        ) || (self.phase == PlaybackPhase::Stopping
            && !matches!(
                observation.state,
                BrowserPlaybackState::Stopped
                    | BrowserPlaybackState::Ended
                    | BrowserPlaybackState::Stalled { .. }
                    | BrowserPlaybackState::Failed { .. }
            ))
        {
            return PlaybackObservationDisposition::IgnoredTerminalPhase;
        }

        self.last_observation_sequence = Some(observation.token.sequence);
        self.position_ticks = self.clamp_position(observation.position_ticks);
        match observation.state {
            BrowserPlaybackState::Playing => {
                self.phase = PlaybackPhase::Playing;
                self.paused = false;
                self.report_started_or_progress(commands);
            }
            BrowserPlaybackState::Paused => {
                self.phase = PlaybackPhase::Paused;
                self.paused = true;
                self.report_progress(commands);
            }
            BrowserPlaybackState::Buffering => {
                self.phase = PlaybackPhase::Buffering;
                self.report_progress(commands);
            }
            BrowserPlaybackState::Ended => {
                self.phase = PlaybackPhase::Ended;
                self.paused = false;
                self.close_reporting(PlaybackStopReason::Ended, commands);
                self.pending_stop_reason = None;
            }
            BrowserPlaybackState::Stopped => {
                self.phase = PlaybackPhase::Stopped;
                self.paused = false;
                let reason = self
                    .pending_stop_reason
                    .take()
                    .unwrap_or(PlaybackStopReason::BrowserStopped);
                self.close_reporting(reason, commands);
            }
            BrowserPlaybackState::Stalled { message } => {
                self.retire_pipeline(commands);
                self.fail(
                    PlaybackFailureStage::Stall,
                    message,
                    self.active_candidate(),
                    MpvFallbackReason::BrowserStall,
                );
                self.close_reporting(PlaybackStopReason::Stalled, commands);
            }
            BrowserPlaybackState::Failed { message } => {
                self.retire_pipeline(commands);
                self.fail(
                    PlaybackFailureStage::Runtime,
                    message,
                    self.active_candidate(),
                    MpvFallbackReason::RuntimeFailure,
                );
                self.close_reporting(PlaybackStopReason::RuntimeFailure, commands);
            }
        }
        PlaybackObservationDisposition::Applied
    }

    fn push_start(
        &self,
        reason: PlaybackStartReason,
        commands: &mut Vec<PlaybackCommand>,
    ) -> Result<(), PlaybackCoreError> {
        let Some(generation) = self.generation else {
            return Err(self.invalid_transition(action_for_start_reason(reason)));
        };
        let Some(session) = self.session.as_ref() else {
            return Err(self.invalid_transition(action_for_start_reason(reason)));
        };
        let Some(plan) = self.plan.as_ref() else {
            return Err(self.invalid_transition(action_for_start_reason(reason)));
        };
        let Some(candidate) = plan.candidates.get(self.candidate_index).copied() else {
            return Err(self.invalid_transition(action_for_start_reason(reason)));
        };
        commands.push(PlaybackCommand::StartEmbedded {
            attempt: PlaybackAttempt {
                generation,
                reason,
                session: PlaybackSessionSummary::from(session),
                start_position_ticks: self.position_ticks,
                paused: self.paused,
                candidate_index: self.candidate_index,
                candidate,
                plan: plan.clone(),
            },
        });
        Ok(())
    }

    fn report_started_or_progress(&mut self, commands: &mut Vec<PlaybackCommand>) {
        if self.report_started {
            self.report_progress(commands);
            return;
        }
        let Some(report) = self.report() else {
            return;
        };
        self.report_started = true;
        self.report_stopped = false;
        commands.push(PlaybackCommand::ReportStarted { report });
    }

    fn report_progress(&self, commands: &mut Vec<PlaybackCommand>) {
        if !self.report_started || self.report_stopped {
            return;
        }
        if let Some(report) = self.report() {
            commands.push(PlaybackCommand::ReportProgress { report });
        }
    }

    fn close_reporting(&mut self, reason: PlaybackStopReason, commands: &mut Vec<PlaybackCommand>) {
        if !self.report_started || self.report_stopped {
            return;
        }
        let Some(report) = self.report() else {
            return;
        };
        self.report_stopped = true;
        commands.push(PlaybackCommand::ReportStopped { report, reason });
    }

    fn report(&self) -> Option<PlaybackReport> {
        Some(PlaybackReport {
            generation: self.generation?,
            session: PlaybackSessionSummary::from(self.session.as_ref()?),
            position_ticks: self.position_ticks,
            paused: self.paused,
        })
    }

    fn fail(
        &mut self,
        stage: PlaybackFailureStage,
        message: String,
        candidate: Option<FfmpegCandidate>,
        fallback_reason: MpvFallbackReason,
    ) {
        let Some(generation) = self.generation else {
            return;
        };
        self.phase = PlaybackPhase::Failed;
        self.paused = false;
        self.failure = Some(PlaybackFailure {
            stage,
            generation,
            candidate,
            message,
            retryable: true,
        });
        self.mpv_fallback = self.session.as_ref().map(|session| MpvFallbackMetadata {
            available: session.mpv_fallback_available,
            reason: fallback_reason,
            item_id: session.item_id.clone(),
            media_source_id: session.media_source_id.clone(),
            resume_position_ticks: self.position_ticks,
        });
        self.pending_stop_reason = None;
    }

    fn retire_pipeline(&self, commands: &mut Vec<PlaybackCommand>) {
        if let Some(generation) = self.generation {
            commands.push(PlaybackCommand::StopEmbedded { generation });
        }
    }

    fn active_candidate(&self) -> Option<FfmpegCandidate> {
        self.plan
            .as_ref()
            .and_then(|plan| plan.candidates.get(self.candidate_index))
            .copied()
    }

    fn pipeline_is_active(&self) -> bool {
        matches!(
            self.phase,
            PlaybackPhase::Starting
                | PlaybackPhase::Playing
                | PlaybackPhase::Paused
                | PlaybackPhase::Buffering
                | PlaybackPhase::Seeking
                | PlaybackPhase::Restarting
        )
    }

    fn allocate_generation(&mut self) -> Result<PlaybackGeneration, PlaybackCoreError> {
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(PlaybackCoreError::GenerationExhausted)?;
        Ok(PlaybackGeneration(self.next_generation))
    }

    fn validate_position(&self, position_ticks: u64) -> Result<(), PlaybackCoreError> {
        let Some(duration_ticks) = self
            .session
            .as_ref()
            .and_then(|session| session.duration_ticks)
        else {
            return Ok(());
        };
        if position_ticks > duration_ticks {
            return Err(PlaybackCoreError::PositionAfterDuration {
                position_ticks,
                duration_ticks,
            });
        }
        Ok(())
    }

    fn clamp_position(&self, position_ticks: u64) -> u64 {
        self.session
            .as_ref()
            .and_then(|session| session.duration_ticks)
            .map_or(position_ticks, |duration| position_ticks.min(duration))
    }

    const fn invalid_transition(&self, action: PlaybackActionKind) -> PlaybackCoreError {
        PlaybackCoreError::InvalidTransition {
            action,
            phase: self.phase,
        }
    }
}

fn validate_session(session: &PlaybackSession) -> Result<(), PlaybackCoreError> {
    if session.item_id.trim().is_empty() {
        return Err(PlaybackCoreError::EmptyItemId);
    }
    if let Some(duration_ticks) = session.duration_ticks {
        if session.start_position_ticks > duration_ticks {
            return Err(PlaybackCoreError::PositionAfterDuration {
                position_ticks: session.start_position_ticks,
                duration_ticks,
            });
        }
    }
    Ok(())
}

const fn action_for_start_reason(reason: PlaybackStartReason) -> PlaybackActionKind {
    match reason {
        PlaybackStartReason::Play => PlaybackActionKind::Play,
        PlaybackStartReason::Seek => PlaybackActionKind::Seek,
        PlaybackStartReason::Restart => PlaybackActionKind::Restart,
        PlaybackStartReason::Replay => PlaybackActionKind::Replay,
        PlaybackStartReason::CandidateFallback => PlaybackActionKind::StartupFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AudioChannelLayout, BrowserPlaybackCapabilities, FfmpegEncoderAvailability,
        FfmpegPlanRequest, FfmpegPlatform, SourceVideoProfile,
    };

    fn session() -> PlaybackSession {
        PlaybackSession {
            item_id: "episode-1".to_owned(),
            media_source_id: Some("source-1".to_owned()),
            play_session_id: Some("play-1".to_owned()),
            start_position_ticks: 10,
            duration_ticks: Some(1_000),
            plan_request: FfmpegPlanRequest {
                platform: FfmpegPlatform::Linux,
                encoders: FfmpegEncoderAvailability::default(),
                browser: BrowserPlaybackCapabilities {
                    fmp4_hls: true,
                    h264_sdr: true,
                    hevc_main10_hdr: false,
                    aac: true,
                    max_audio_channels: 2,
                },
                video: SourceVideoProfile::H264Sdr,
                audio: Some(AudioChannelLayout::Stereo),
            },
            mpv_fallback_available: true,
        }
    }

    #[test]
    fn generation_exhaustion_leaves_state_unchanged() {
        let mut core = EmbeddedPlaybackCore {
            next_generation: u64::MAX,
            ..EmbeddedPlaybackCore::new()
        };
        let before = core.snapshot();

        let error = core
            .dispatch(PlaybackAction::Play(session()))
            .expect_err("generation should be exhausted");

        assert_eq!(
            (error, core.snapshot()),
            (PlaybackCoreError::GenerationExhausted, before)
        );
    }
}
