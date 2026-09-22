use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jellypilot_core::audio_tracks::{
    AudioTrackKey, AudioTrackPreference, AudioTrackStore, SubtitleTrackPreference,
};
use jellypilot_core::intro_skipper::{
    IntroPromptToken, IntroSkipAction, IntroSkipInput, IntroSkipper,
};
use jellypilot_core::volume_memory::{SeasonVolumeKey, SeasonVolumeStore};
use jellypilot_media_server::{IntroSkipKind, IntroSkipRange, JellyfinClient, VideoItemDetail};
use tokio::sync::Mutex as AsyncMutex;

use super::*;
use crate::{ProfileScopeRef, SdkInner};

pub const PASSIVE_PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(10);
pub const PLAYBACK_REPORT_TIMEOUT: Duration = Duration::from_secs(2);

/// Origin access is deliberately separated from ordinary presentation state.
/// Debug output for URLs is redacted and plans must never be persisted.
#[derive(Clone, Debug)]
pub struct PlaybackPlan {
    pub item_id: String,
    pub title: String,
    pub item_type: String,
    pub series_id: Option<String>,
    pub runtime_seconds: Option<f64>,
    pub start_position_seconds: f64,
    pub initial_volume: Option<f64>,
    pub media_source_id: String,
    pub play_session_id: Option<String>,
    pub stream_url: AuthenticatedUrl,
    pub external_subtitles: Vec<ExternalSubtitle>,
    pub tracks: Vec<PlaybackTrack>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub mpv_audio_index: Option<i64>,
    pub mpv_subtitle_index: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct PlaybackTrack {
    pub provider_index: i32,
    /// Initial type-local MPV id; external subtitles are assigned at load time.
    pub player_index: Option<i64>,
    pub kind: String,
    pub title: Option<String>,
    pub language: Option<String>,
    pub external: bool,
}

/// One loaded-player observation. Sequence increases within this session,
/// including the final observation; callbacks from replaced items are rejected.
#[derive(Clone, Debug)]
pub struct PlaybackObservation {
    pub sequence: u64,
    pub position_seconds: f64,
    pub paused: bool,
    pub muted: bool,
    pub volume: f64,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct SkipPrompt {
    pub kind: IntroSkipKind,
    pub end_seconds: f64,
    pub duration_ms: u32,
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackUpdate {
    /// True only after the provider accepted a start/progress report.
    pub reported: bool,
    pub report_error: Option<String>,
    pub seek_to: Option<f64>,
    pub skip_prompt: Option<SkipPrompt>,
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackFinish {
    pub report_error: Option<String>,
    pub next_item_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EndReason {
    Explicit,
    Natural,
    Interrupted,
}

struct SessionState {
    last: Option<PlaybackObservation>,
    started: bool,
    ended: bool,
    last_report: Option<Instant>,
    intro: IntroSkipper,
    pending_prompt: Option<IntroPromptToken>,
    prompt: Option<SkipPrompt>,
    audio_tracks: AudioTrackStore,
    volume_memory: Option<(SeasonVolumeKey, SeasonVolumeStore)>,
}

/// Business lifetime of one presentation. Holds no native player or surface.
/// A host must finish an old session before preparing another, and submit only
/// observations correlated with this object and its loaded media.
pub struct PlaybackSession {
    inner: Arc<SdkInner>,
    client: Arc<JellyfinClient>,
    pub(super) scope: ProfileScopeRef,
    sequence: u64,
    plan: PlaybackPlan,
    play_method: String,
    ranges: Vec<IntroSkipRange>,
    audio_key: Option<AudioTrackKey>,
    state: Mutex<SessionState>,
    operation: AsyncMutex<()>,
}

impl PlaybackSession {
    #[expect(
        clippy::too_many_arguments,
        reason = "private constructor collects the prepared session ownership once"
    )]
    pub(super) fn new(
        inner: Arc<SdkInner>,
        client: Arc<JellyfinClient>,
        scope: ProfileScopeRef,
        sequence: u64,
        detail: VideoItemDetail,
        prepared: PreparedMedia,
        start_position_seconds: f64,
        intro: IntroSkipper,
        ranges: Vec<IntroSkipRange>,
        audio_tracks: AudioTrackStore,
        audio_key: Option<AudioTrackKey>,
        volume_memory: Option<(SeasonVolumeKey, SeasonVolumeStore)>,
    ) -> Self {
        let source = &prepared.resolution.media_source;
        let play_method = play_method(source).to_owned();
        let tracks = source
            .media_streams
            .iter()
            .filter(|stream| matches!(stream.stream_type.as_str(), "Audio" | "Subtitle"))
            .map(|stream| PlaybackTrack {
                provider_index: stream.index,
                player_index: if stream.is_external {
                    None
                } else {
                    type_local_track_index(&source.media_streams, &stream.stream_type, stream.index)
                        .ok()
                },
                kind: stream.stream_type.clone(),
                title: stream.display_title.clone(),
                language: stream.language.clone(),
                external: stream.is_external,
            })
            .collect();
        let plan = PlaybackPlan {
            title: item_title(
                &detail.name,
                &detail.item_type,
                detail.series_name.as_deref(),
                detail.season_number,
                detail.episode_number,
            ),
            item_id: detail.id,
            item_type: detail.item_type,
            series_id: detail.series_id,
            runtime_seconds: prepared.runtime_seconds,
            start_position_seconds,
            initial_volume: volume_memory
                .as_ref()
                .and_then(|(key, store)| store.get(key)),
            media_source_id: source.id.clone(),
            play_session_id: prepared.resolution.play_session_id,
            stream_url: prepared.resolution.stream_url,
            external_subtitles: prepared.resolution.external_subtitles,
            tracks,
            audio_stream_index: prepared.audio_stream_index,
            subtitle_stream_index: prepared.subtitle_stream_index,
            mpv_audio_index: prepared.mpv_audio_index,
            mpv_subtitle_index: prepared.mpv_subtitle_index,
        };
        Self {
            inner,
            client,
            scope,
            sequence,
            plan,
            play_method,
            ranges,
            audio_key,
            state: Mutex::new(SessionState {
                last: None,
                started: false,
                ended: false,
                last_report: None,
                intro,
                pending_prompt: None,
                prompt: None,
                audio_tracks,
                volume_memory,
            }),
            operation: AsyncMutex::new(()),
        }
    }

    pub fn plan(&self) -> PlaybackPlan {
        self.plan.clone()
    }

    /// Checks both scope/session correlation and current start/resume admission.
    /// The host must additionally gate on Android visibility and unlocked state.
    pub fn is_active(&self) -> bool {
        self.validate_current().is_ok()
            && self
                .inner
                .state
                .lock()
                .is_ok_and(|state| !state.handoff_in_progress && !state.sign_out_cleanup_pending)
    }

    /// Holds the account-handoff boundary until the host acknowledges a physical
    /// load/resume, so credential deletion cannot race a partially loaded player.
    /// The committed worker survives cancellation of the FFI waiter and SDK close;
    /// the host must return only after execution or acknowledged physical cleanup.
    pub async fn run_admitted<T: Send + 'static>(
        self: &Arc<Self>,
        operation: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Result<T, SdkError> {
        let generation = {
            let state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
            if state.closed {
                return Err(SdkError::Closed);
            }
            if state.handoff_in_progress || state.sign_out_cleanup_pending {
                return Err(SdkError::OperationInProgress);
            }
            state.playback_generation
        };
        let admission = crate::PlaybackAdmission {
            inner: Arc::clone(&self.inner),
            generation,
        };
        let session = Arc::clone(self);
        let receiver = self.inner.spawn_committed(move || async move {
            admission
                .run(async move {
                    session.validate_current()?;
                    Ok(operation.await)
                })
                .await?
        })?;
        receiver.await.map_err(|_| {
            SdkError::Request("the admitted player operation failed unexpectedly".into())
        })?
    }

    fn validate_current(&self) -> Result<(), SdkError> {
        if !self.inner.scope_is_active(&self.scope) {
            return Err(SdkError::Stale);
        }
        let registry = self.inner.playback.lock().map_err(|_| SdkError::Closed)?;
        if registry
            .active
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .is_none_or(|active| active.sequence != self.sequence)
        {
            return Err(SdkError::Stale);
        }
        if self.state.lock().map_err(|_| SdkError::Closed)?.ended {
            return Err(SdkError::Stale);
        }
        Ok(())
    }

    /// Persists recovery and issues ordered provider reports on the SDK runtime.
    /// Report failure is data, so it does not pretend the native player stopped.
    pub async fn observe(
        self: &Arc<Self>,
        observation: PlaybackObservation,
        force_report: bool,
    ) -> Result<PlaybackUpdate, SdkError> {
        let session = Arc::clone(self);
        self.inner
            .handle
            .spawn(async move { session.observe_inner(observation, force_report).await })
            .await
            .map_err(|_| SdkError::Closed)?
    }

    async fn observe_inner(
        &self,
        observation: PlaybackObservation,
        force_report: bool,
    ) -> Result<PlaybackUpdate, SdkError> {
        let _operation = self.operation.lock().await;
        self.validate_current()?;
        let now = Instant::now();
        let (start, should_report, mut update) = {
            let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
            validate_observation(&state, &observation)?;
            let volume_changed = state
                .last
                .as_ref()
                .is_none_or(|last| last.volume != observation.volume);
            if volume_changed {
                if let Some((key, store)) = &mut state.volume_memory {
                    store
                        .remember(key, observation.volume)
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                }
            }
            let changed = state.last.as_ref().is_some_and(|last| {
                last.paused != observation.paused
                    || last.muted != observation.muted
                    || last.audio_stream_index != observation.audio_stream_index
                    || last.subtitle_stream_index != observation.subtitle_stream_index
            });
            let start = !state.started;
            let should_report = start
                || force_report
                || changed
                || passive_progress_report_due(
                    state.last_report,
                    now,
                    PASSIVE_PROGRESS_REPORT_INTERVAL,
                );
            let mut update = PlaybackUpdate::default();
            match state
                .intro
                .observe(observation.position_seconds, now, IntroSkipInput::Position)
            {
                Some(IntroSkipAction::Seek(position) | IntroSkipAction::ManualSkip(position)) => {
                    update.seek_to = Some(position)
                }
                Some(IntroSkipAction::ShowPrompt { token, duration_ms }) => {
                    if let Some(range) = self.ranges.iter().find(|range| {
                        observation.position_seconds >= range.start_seconds
                            && observation.position_seconds < range.end_seconds
                    }) {
                        state.pending_prompt = Some(token);
                        state.prompt = Some(SkipPrompt {
                            kind: range.kind,
                            end_seconds: range.end_seconds,
                            duration_ms,
                        });
                    }
                }
                None => {}
            }
            if state.pending_prompt.is_some() || state.intro.prompt_kind().is_some() {
                update.skip_prompt = state.prompt.clone();
            } else {
                state.prompt = None;
            }
            state.last = Some(observation.clone());
            (start, should_report, update)
        };
        // Serialize persistence with clear/finish, rejecting a replaced scope.
        self.save_recovery(&observation)?;
        if should_report {
            let server = JellyfinPlaybackServer::from(Arc::clone(&self.client));
            let report = self.report(&observation);
            let succeeded = if start {
                reporting_succeeded_with_timeout(
                    PLAYBACK_REPORT_TIMEOUT,
                    server.report_playback_start(report),
                )
                .await
            } else {
                reporting_succeeded_with_timeout(
                    PLAYBACK_REPORT_TIMEOUT,
                    server.report_playback_progress(report),
                )
                .await
            };
            self.validate_current()?;
            let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
            state.last_report = Some(now);
            if succeeded {
                state.started = true;
                update.reported = true;
            } else {
                update.report_error = Some(
                    if start {
                        "Playback start was not reported"
                    } else {
                        "Playback progress was not reported"
                    }
                    .into(),
                );
            }
        }
        Ok(update)
    }

    fn save_recovery(&self, observation: &PlaybackObservation) -> Result<(), SdkError> {
        super::recovery::save(
            &self.inner,
            &self.scope,
            self.sequence,
            LocalPlaybackRecovery {
                item_id: self.plan.item_id.clone(),
                title: self.plan.title.clone(),
                position_seconds: observation.position_seconds,
            },
        )
    }

    fn clear_recovery(&self) -> Result<(), SdkError> {
        let state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
        if state.epoch != self.scope.generation {
            return Err(SdkError::Stale);
        }
        super::recovery::clear_profile(&self.inner, &self.scope.profile_key)
    }

    fn report(&self, observation: &PlaybackObservation) -> PlaybackReport {
        PlaybackReport {
            item_id: self.plan.item_id.clone(),
            media_source_id: self.plan.media_source_id.clone(),
            play_session_id: self.plan.play_session_id.clone(),
            position_ticks: checked_seconds_to_ticks(observation.position_seconds).ok(),
            is_paused: observation.paused,
            is_muted: observation.muted,
            volume_level: volume_level(observation.volume),
            audio_stream_index: observation.audio_stream_index,
            subtitle_stream_index: observation.subtitle_stream_index,
            play_method: self.play_method.clone(),
        }
    }

    /// Completes teardown even if the caller drops its wait. Cleanup remains
    /// admitted while account handoff blocks new starts. Only a natural end may
    /// request the next episode; errors and explicit exit never auto-advance.
    pub async fn finish(
        self: &Arc<Self>,
        observation: PlaybackObservation,
        natural_end: bool,
    ) -> Result<PlaybackFinish, SdkError> {
        let session = Arc::clone(self);
        let reason = if natural_end {
            EndReason::Natural
        } else {
            EndReason::Explicit
        };
        self.inner
            .handle
            .spawn(async move { session.finish_inner(observation, reason).await })
            .await
            .map_err(|_| SdkError::Closed)?
    }

    /// Ends failed/interrupted playback while retaining its latest recovery point.
    /// It never advances episodes and never conflates failure with explicit exit.
    pub async fn interrupt(
        self: &Arc<Self>,
        observation: PlaybackObservation,
    ) -> Result<PlaybackFinish, SdkError> {
        let session = Arc::clone(self);
        self.inner
            .handle
            .spawn(async move {
                session
                    .finish_inner(observation, EndReason::Interrupted)
                    .await
            })
            .await
            .map_err(|_| SdkError::Closed)?
    }

    async fn finish_inner(
        &self,
        observation: PlaybackObservation,
        reason: EndReason,
    ) -> Result<PlaybackFinish, SdkError> {
        let _operation = self.operation.lock().await;
        self.validate_current()?;
        {
            let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
            validate_observation(&state, &observation)?;
            state.ended = true;
            state.last = Some(observation.clone());
        }
        let server = JellyfinPlaybackServer::from(Arc::clone(&self.client));
        let reported = reporting_succeeded_with_timeout(
            PLAYBACK_REPORT_TIMEOUT,
            server.report_playback_stop(PlaybackStopReport {
                item_id: self.plan.item_id.clone(),
                media_source_id: self.plan.media_source_id.clone(),
                play_session_id: self.plan.play_session_id.clone(),
                position_ticks: checked_seconds_to_ticks(observation.position_seconds).ok(),
            }),
        )
        .await;
        let recovery_result = if reason == EndReason::Interrupted {
            self.save_recovery(&observation)
        } else {
            self.clear_recovery()
        };
        {
            let mut registry = self.inner.playback.lock().map_err(|_| SdkError::Closed)?;
            if registry
                .active
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|active| active.sequence == self.sequence)
            {
                registry.active = None;
            }
        }
        recovery_result?;
        let next_item_id = if reason == EndReason::Natural
            && self.inner.scope_is_active(&self.scope)
            && self.inner.auto_play_next_preference()?
        {
            adjacent_item(&self.client, &self.plan.item_id, true).await?
        } else {
            None
        };
        if !self.inner.scope_is_active(&self.scope)
            || self
                .inner
                .playback
                .lock()
                .map_err(|_| SdkError::Closed)?
                .sequence
                != self.sequence
        {
            return Err(SdkError::Stale);
        }
        Ok(PlaybackFinish {
            report_error: (!reported).then(|| "Playback stop was not reported".into()),
            next_item_id,
        })
    }

    pub(crate) async fn finish_for_handoff(self: &Arc<Self>) -> Result<(), SdkError> {
        let observation = {
            let state = self.state.lock().map_err(|_| SdkError::Closed)?;
            if state.ended {
                return Ok(());
            }
            let mut observation = state.last.clone().unwrap_or(PlaybackObservation {
                sequence: 0,
                position_seconds: self.plan.start_position_seconds,
                paused: true,
                muted: false,
                volume: 100.0,
                audio_stream_index: self.plan.audio_stream_index,
                subtitle_stream_index: self.plan.subtitle_stream_index,
            });
            observation.sequence = observation.sequence.saturating_add(1);
            observation
        };
        let _outcome = self.finish(observation, false).await?;
        Ok(())
    }

    pub async fn adjacent(self: &Arc<Self>, next: bool) -> Result<Option<String>, SdkError> {
        self.validate_current()?;
        let session = Arc::clone(self);
        let result = self
            .inner
            .handle
            .spawn(async move { adjacent_item(&session.client, &session.plan.item_id, next).await })
            .await
            .map_err(|_| SdkError::Closed)??;
        self.validate_current()?;
        Ok(result)
    }

    /// Records an explicit user seek before the host submits it to the player.
    /// Automatic skips are suppressed for containing markers until playback
    /// enters and leaves them; mode and other markers remain unchanged.
    /// The host must discard any older in-flight observation's returned action.
    ///
    /// Returns `InvalidInput` for an invalid position and `Stale` after this
    /// session ends or loses its active profile/session ownership.
    pub fn note_user_seek(&self, position_seconds: f64) -> Result<(), SdkError> {
        checked_seconds_to_ticks(position_seconds).map_err(playback_error)?;
        self.validate_current()?;
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.ended {
            return Err(SdkError::Stale);
        }
        state.intro.note_user_seek(position_seconds);
        Ok(())
    }

    pub fn set_intro_mode(&self, mode: IntroSkipMode) -> Result<(), SdkError> {
        self.validate_current()?;
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        let was_off = state.intro.mode() == IntroSkipMode::Off;
        state.intro.set_mode(mode);
        if was_off && mode != IntroSkipMode::Off {
            state.intro.replace_ranges(self.ranges.clone());
        }
        if mode == IntroSkipMode::Off {
            state.pending_prompt = None;
            state.prompt = None;
        }
        Ok(())
    }

    /// Records actual prompt presentation; request issuance alone is insufficient.
    pub fn acknowledge_skip_prompt(&self, presented: bool) -> Result<(), SdkError> {
        self.validate_current()?;
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if let Some(token) = state.pending_prompt.take() {
            state.intro.prompt_settled(token, presented, Instant::now());
        }
        Ok(())
    }

    pub fn skip_intro(&self) -> Result<Option<f64>, SdkError> {
        self.validate_current()?;
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        let Some(position) = state.last.as_ref().map(|last| last.position_seconds) else {
            return Ok(None);
        };
        Ok(
            match state
                .intro
                .observe(position, Instant::now(), IntroSkipInput::ManualSkip)
            {
                Some(IntroSkipAction::ManualSkip(position)) => Some(position),
                _ => None,
            },
        )
    }

    /// Called only after the player accepted an explicit user track choice.
    pub fn remember_track(&self, kind: String, provider_index: i32) -> Result<(), SdkError> {
        self.validate_current()?;
        let Some(key) = &self.audio_key else {
            return Ok(());
        };
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if kind == "Subtitle" && provider_index < 0 {
            return state
                .audio_tracks
                .remember_subtitle(key, SubtitleTrackPreference::disabled())
                .map_err(|error| SdkError::Storage(error.to_string()));
        }
        let track = self
            .plan
            .tracks
            .iter()
            .find(|track| track.kind == kind && track.provider_index == provider_index)
            .ok_or_else(|| {
                SdkError::InvalidInput("the selected media track is unavailable".into())
            })?;
        let Some(language) = track.language.as_deref() else {
            return Ok(());
        };
        match kind.as_str() {
            "Audio" => {
                if let Some(preference) =
                    AudioTrackPreference::new(language, track.title.as_deref())
                {
                    state
                        .audio_tracks
                        .remember(key, preference)
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                }
            }
            "Subtitle" => {
                if let Some(preference) =
                    SubtitleTrackPreference::enabled(language, track.title.as_deref())
                {
                    state
                        .audio_tracks
                        .remember_subtitle(key, preference)
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                }
            }
            _ => return Err(SdkError::InvalidInput("invalid track kind".into())),
        }
        Ok(())
    }
}

fn validate_observation(
    state: &SessionState,
    observation: &PlaybackObservation,
) -> Result<(), SdkError> {
    checked_seconds_to_ticks(observation.position_seconds).map_err(playback_error)?;
    if !observation.volume.is_finite() || observation.volume < 0.0 {
        return Err(SdkError::InvalidInput("invalid player volume".into()));
    }
    if state
        .last
        .as_ref()
        .is_some_and(|last| observation.sequence <= last.sequence)
    {
        return Err(SdkError::Stale);
    }
    Ok(())
}

pub async fn reporting_succeeded_with_timeout<E>(
    timeout: Duration,
    report: impl std::future::Future<Output = Result<(), E>>,
) -> bool {
    tokio::time::timeout(timeout, report)
        .await
        .is_ok_and(|result| result.is_ok())
}

pub fn passive_progress_report_due(
    last_report_at: Option<Instant>,
    now: Instant,
    interval: Duration,
) -> bool {
    last_report_at.is_some_and(|last| now.saturating_duration_since(last) >= interval)
}

pub fn volume_level(volume: f64) -> i32 {
    if volume.is_finite() {
        volume.clamp(0.0, 100.0).round() as i32
    } else {
        100
    }
}
