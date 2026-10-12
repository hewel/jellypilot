use crate::{IntroSkipMode, JellypilotSdk, OperationToken, SdkError};
use jellypilot_sdk::playback as sdk;
use std::sync::Arc;

/// Explicit sensitive-value access avoids URLs in generated record debug output.
#[derive(uniffi::Object)]
pub struct AuthenticatedUrl {
    inner: sdk::AuthenticatedUrl,
}

#[uniffi::export]
impl AuthenticatedUrl {
    /// Pass directly to the player. Never log, display or persist this value.
    pub fn value(&self) -> String {
        self.inner.as_str().to_owned()
    }
}

impl From<sdk::AuthenticatedUrl> for AuthenticatedUrl {
    fn from(inner: sdk::AuthenticatedUrl) -> Self {
        Self { inner }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum PlaybackStartPosition {
    Beginning,
    Resume,
    At { seconds: f64 },
}

impl From<PlaybackStartPosition> for sdk::PlaybackStartPosition {
    fn from(value: PlaybackStartPosition) -> Self {
        match value {
            PlaybackStartPosition::Beginning => Self::Beginning,
            PlaybackStartPosition::Resume => Self::Resume,
            PlaybackStartPosition::At { seconds } => Self::At(seconds),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaybackSelection {
    pub media_source_id: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

impl From<PlaybackSelection> for sdk::PlaybackSelection {
    fn from(value: PlaybackSelection) -> Self {
        Self {
            media_source_id: value.media_source_id,
            audio_stream_index: value.audio_stream_index,
            subtitle_stream_index: value.subtitle_stream_index,
        }
    }
}

#[derive(Clone, uniffi::Record)]
pub struct ExternalSubtitle {
    pub provider_index: i32,
    pub url: Arc<AuthenticatedUrl>,
    pub title: Option<String>,
    pub language: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaybackTrack {
    pub provider_index: i32,
    pub player_index: Option<i64>,
    pub kind: String,
    pub title: Option<String>,
    pub language: Option<String>,
    pub external: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct PlaybackPlan {
    pub item_id: String,
    pub title: String,
    pub item_type: String,
    pub series_id: Option<String>,
    pub runtime_seconds: Option<f64>,
    pub start_position_seconds: f64,
    pub media_source_id: String,
    pub play_session_id: Option<String>,
    pub stream_url: Arc<AuthenticatedUrl>,
    pub external_subtitles: Vec<ExternalSubtitle>,
    pub tracks: Vec<PlaybackTrack>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub mpv_audio_index: Option<i64>,
    pub mpv_subtitle_index: Option<i64>,
    pub initial_volume: Option<f64>,
}

impl From<sdk::PlaybackPlan> for PlaybackPlan {
    fn from(value: sdk::PlaybackPlan) -> Self {
        Self {
            item_id: value.item_id,
            title: value.title,
            item_type: value.item_type,
            series_id: value.series_id,
            runtime_seconds: value.runtime_seconds,
            start_position_seconds: value.start_position_seconds,
            media_source_id: value.media_source_id,
            play_session_id: value.play_session_id,
            stream_url: Arc::new(value.stream_url.into()),
            external_subtitles: value
                .external_subtitles
                .into_iter()
                .map(|subtitle| ExternalSubtitle {
                    provider_index: subtitle.provider_index,
                    url: Arc::new(subtitle.url.into()),
                    title: subtitle.title,
                    language: subtitle.language,
                })
                .collect(),
            tracks: value
                .tracks
                .into_iter()
                .map(|track| PlaybackTrack {
                    provider_index: track.provider_index,
                    player_index: track.player_index,
                    kind: track.kind,
                    title: track.title,
                    language: track.language,
                    external: track.external,
                })
                .collect(),
            audio_stream_index: value.audio_stream_index,
            subtitle_stream_index: value.subtitle_stream_index,
            mpv_audio_index: value.mpv_audio_index,
            mpv_subtitle_index: value.mpv_subtitle_index,
            initial_volume: value.initial_volume,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaybackObservation {
    pub sequence: u64,
    pub position_seconds: f64,
    pub paused: bool,
    pub muted: bool,
    pub volume: f64,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

impl From<PlaybackObservation> for sdk::PlaybackObservation {
    fn from(value: PlaybackObservation) -> Self {
        Self {
            sequence: value.sequence,
            position_seconds: value.position_seconds,
            paused: value.paused,
            muted: value.muted,
            volume: value.volume,
            audio_stream_index: value.audio_stream_index,
            subtitle_stream_index: value.subtitle_stream_index,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SkipPrompt {
    pub kind: IntroSkipKind,
    pub end_seconds: f64,
    pub duration_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum IntroSkipKind {
    Introduction,
    Credits,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaybackUpdate {
    pub reported: bool,
    pub report_error: Option<String>,
    pub seek_to: Option<f64>,
    pub skip_prompt: Option<SkipPrompt>,
}

impl From<sdk::PlaybackUpdate> for PlaybackUpdate {
    fn from(value: sdk::PlaybackUpdate) -> Self {
        Self {
            reported: value.reported,
            report_error: value.report_error,
            seek_to: value.seek_to,
            skip_prompt: value.skip_prompt.map(|prompt| SkipPrompt {
                kind: match prompt.kind {
                    jellypilot_media_server::IntroSkipKind::Introduction => {
                        IntroSkipKind::Introduction
                    }
                    jellypilot_media_server::IntroSkipKind::Credits => IntroSkipKind::Credits,
                },
                end_seconds: prompt.end_seconds,
                duration_ms: prompt.duration_ms,
            }),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PlaybackFinish {
    pub report_error: Option<String>,
    pub next_item_id: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LocalPlaybackRecovery {
    pub item_id: String,
    pub title: String,
    pub position_seconds: f64,
}

#[derive(uniffi::Object)]
pub struct PlaybackSession {
    inner: Arc<sdk::PlaybackSession>,
}

/// A physical native load or resume, settled only after the player acknowledges it.
#[uniffi::export(with_foreign)]
#[allow(
    clippy::double_must_use,
    reason = "async-trait injects must_use on boxed futures; rust-clippy#17529"
)]
#[async_trait::async_trait]
pub trait PlaybackHostOperation: Send + Sync {
    async fn execute(&self) -> bool;
}

#[uniffi::export(async_runtime = "tokio")]
impl PlaybackSession {
    pub async fn run_admitted(
        &self,
        operation: Arc<dyn PlaybackHostOperation>,
    ) -> Result<bool, SdkError> {
        self.inner
            .run_admitted(async move { operation.execute().await })
            .await
            .map_err(Into::into)
    }
    pub fn plan(&self) -> PlaybackPlan {
        self.inner.plan().into()
    }
    pub fn is_active(&self) -> bool {
        self.inner.is_active()
    }
    pub async fn observe(
        &self,
        observation: PlaybackObservation,
        force_report: bool,
    ) -> Result<PlaybackUpdate, SdkError> {
        self.inner
            .observe(observation.into(), force_report)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
    pub async fn finish(
        &self,
        observation: PlaybackObservation,
        natural_end: bool,
    ) -> Result<PlaybackFinish, SdkError> {
        self.inner
            .finish(observation.into(), natural_end)
            .await
            .map(|value| PlaybackFinish {
                report_error: value.report_error,
                next_item_id: value.next_item_id,
            })
            .map_err(Into::into)
    }
    pub async fn adjacent(&self, next: bool) -> Result<Option<String>, SdkError> {
        self.inner.adjacent(next).await.map_err(Into::into)
    }
    pub async fn interrupt(
        &self,
        observation: PlaybackObservation,
    ) -> Result<PlaybackFinish, SdkError> {
        self.inner
            .interrupt(observation.into())
            .await
            .map(|value| PlaybackFinish {
                report_error: value.report_error,
                next_item_id: value.next_item_id,
            })
            .map_err(Into::into)
    }
    pub fn note_user_seek(&self, position_seconds: f64) -> Result<(), SdkError> {
        self.inner
            .note_user_seek(position_seconds)
            .map_err(Into::into)
    }
    pub fn set_intro_mode(&self, mode: IntroSkipMode) -> Result<(), SdkError> {
        self.inner.set_intro_mode(mode.into()).map_err(Into::into)
    }
    pub fn acknowledge_skip_prompt(&self, presented: bool) -> Result<(), SdkError> {
        self.inner
            .acknowledge_skip_prompt(presented)
            .map_err(Into::into)
    }
    pub fn skip_intro(&self) -> Result<Option<f64>, SdkError> {
        self.inner.skip_intro().map_err(Into::into)
    }
    pub fn remember_track(&self, kind: String, provider_index: i32) -> Result<(), SdkError> {
        self.inner
            .remember_track(kind, provider_index)
            .map_err(Into::into)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl JellypilotSdk {
    pub async fn prepare_playback(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
        position: PlaybackStartPosition,
        selection: PlaybackSelection,
    ) -> Result<Arc<PlaybackSession>, SdkError> {
        self.sdk
            .prepare_playback(
                Arc::clone(&token.inner),
                item_id,
                position.into(),
                selection.into(),
            )
            .await
            .map(|inner| Arc::new(PlaybackSession { inner }))
            .map_err(Into::into)
    }
    pub async fn local_playback_recovery(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Option<LocalPlaybackRecovery>, SdkError> {
        self.sdk
            .local_playback_recovery(Arc::clone(&token.inner))
            .await
            .map(|value| {
                value.map(|recovery| LocalPlaybackRecovery {
                    item_id: recovery.item_id,
                    title: recovery.title,
                    position_seconds: recovery.position_seconds,
                })
            })
            .map_err(Into::into)
    }
    pub async fn clear_local_playback_recovery(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<(), SdkError> {
        self.sdk
            .clear_local_playback_recovery(Arc::clone(&token.inner))
            .await
            .map_err(Into::into)
    }
}
