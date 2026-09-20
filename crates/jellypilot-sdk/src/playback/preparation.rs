//! Shared playback preparation and reporting, independent of platform player ownership.

use jellypilot_core::audio_tracks::{AudioTrackPreference, SubtitleTrackPreference};
use jellypilot_media_server::{
    select_audio_stream_by_memory, select_native_audio_stream, select_subtitle_stream_index,
    ticks_to_seconds, AudioStreamChoice, JellyfinClient, MediaServerProvider, MediaSource,
    MediaStream, PlaybackAudioContext, PlaybackProgressInfo, PlaybackStartInfo, PlaybackStopInfo,
    TrackPreference,
};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MEDIA_TICKS_PER_SECOND: i64 = 10_000_000;

/// How a new item chooses its initial position.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PlaybackStartPosition {
    /// Start at the beginning.
    #[default]
    Beginning,
    /// Use the resume position from the Library item or detail.
    Resume,
    /// Start at an explicit number of seconds.
    At(f64),
}

/// Provider selections attached to a remote playback request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaybackSelection {
    pub media_source_id: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

/// Sanitized playback failure. Authenticated URLs and dependency error payloads
/// deliberately never cross this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackError {
    MpvNotFound,
    UnsupportedItemType,
    ItemNotPlayable,
    InvalidStartPosition,
    InvalidVolume,
    PlaybackInfoUnavailable,
    MediaSourceUnavailable,
    StreamUrlUnavailable,
    SubtitleUrlUnavailable,
    TrackUnavailable,
    MpvStartFailed,
    MpvLoadFailed,
    MpvControlFailed,
    NoActivePlayback,
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MpvNotFound => "MPV executable not found",
            Self::UnsupportedItemType => "only movies and episodes can be played",
            Self::ItemNotPlayable => "the selected item is not playable",
            Self::InvalidStartPosition => "playback start position is invalid",
            Self::InvalidVolume => "volume must be a finite nonnegative value",
            Self::PlaybackInfoUnavailable => "playback information is unavailable",
            Self::MediaSourceUnavailable => "no playable media source is available",
            Self::StreamUrlUnavailable => "the authenticated media stream is unavailable",
            Self::SubtitleUrlUnavailable => "the authenticated subtitle stream is unavailable",
            Self::TrackUnavailable => "the selected media track is unavailable",
            Self::MpvStartFailed => "MPV could not be started",
            Self::MpvLoadFailed => "MPV could not load the selected item",
            Self::MpvControlFailed => "MPV could not apply the transport command",
            Self::NoActivePlayback => "there is no active playback",
        })
    }
}

impl std::error::Error for PlaybackError {}
/// Boxed asynchronous operation exposed by [`PlaybackServer`].
pub type PlaybackServerFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Token-free request for resolving an item into playable media.
#[derive(Debug, Clone)]
pub struct PlaybackResolutionRequest {
    pub item_id: String,
    pub start_time_ticks: Option<i64>,
    pub selection: PlaybackSelection,
}

/// Authenticated external subtitle and provider metadata needed by MPV.
#[derive(Clone, Debug)]
pub struct ExternalSubtitle {
    pub provider_index: i32,
    pub url: AuthenticatedUrl,
    pub title: Option<String>,
    pub language: Option<String>,
}

pub fn discover_external_subtitles(
    streams: &[MediaStream],
    mut url_for_stream: impl FnMut(&MediaStream) -> Option<AuthenticatedUrl>,
) -> Vec<ExternalSubtitle> {
    streams
        .iter()
        .filter(|stream| stream.stream_type == "Subtitle" && stream.is_external)
        .filter_map(|stream| {
            url_for_stream(stream).map(|url| ExternalSubtitle {
                provider_index: stream.index,
                url,
                title: stream.display_title.clone(),
                language: stream.language.clone(),
            })
        })
        .collect()
}

/// Media-server data required to load a resolved item.
#[derive(Clone)]
pub struct PlaybackResolution {
    pub media_source: MediaSource,
    pub play_session_id: Option<String>,
    pub stream_url: AuthenticatedUrl,
    pub external_subtitles: Vec<ExternalSubtitle>,
}
// MediaSource embeds tokenized stream URLs; keep them out of Debug output.
impl fmt::Debug for PlaybackResolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaybackResolution")
            .field("media_source_id", &self.media_source.id)
            .field("play_session_id", &self.play_session_id)
            .field("stream_url", &self.stream_url)
            .finish()
    }
}

/// Token-free state sent for playback start and progress reports.
#[derive(Debug, Clone)]
pub struct PlaybackReport {
    pub item_id: String,
    pub media_source_id: String,
    pub play_session_id: Option<String>,
    pub position_ticks: Option<i64>,
    pub is_paused: bool,
    pub is_muted: bool,
    pub volume_level: i32,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub play_method: String,
}

/// Token-free state sent when playback stops.
#[derive(Debug, Clone)]
pub struct PlaybackStopReport {
    pub item_id: String,
    pub media_source_id: String,
    pub play_session_id: Option<String>,
    pub position_ticks: Option<i64>,
}

/// Media-server operations required by the playback controller.
pub trait PlaybackServer: Send + Sync {
    fn provider(&self) -> MediaServerProvider;

    fn resolve(
        &self,
        request: PlaybackResolutionRequest,
    ) -> PlaybackServerFuture<'_, Result<PlaybackResolution, PlaybackError>>;

    fn report_playback_start(
        &self,
        report: PlaybackReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>>;

    fn report_playback_progress(
        &self,
        report: PlaybackReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>>;

    fn report_playback_stop(
        &self,
        report: PlaybackStopReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>>;

    /// Original-language and audio-stream facts for automatic audio selection.
    /// Servers that cannot provide them (Emby, lookup failures) yield `None`.
    fn playback_audio_context(
        &self,
        item_id: &str,
    ) -> PlaybackServerFuture<'_, Option<PlaybackAudioContext>> {
        let _ = item_id;
        Box::pin(async { None })
    }
}

/// Production playback-server adapter backed by an authenticated Jellyfin client.
pub struct JellyfinPlaybackServer(Arc<JellyfinClient>);

impl From<Arc<JellyfinClient>> for JellyfinPlaybackServer {
    fn from(server: Arc<JellyfinClient>) -> Self {
        Self(server)
    }
}

impl PlaybackServer for JellyfinPlaybackServer {
    fn provider(&self) -> MediaServerProvider {
        self.0.provider()
    }

    fn resolve(
        &self,
        request: PlaybackResolutionRequest,
    ) -> PlaybackServerFuture<'_, Result<PlaybackResolution, PlaybackError>> {
        Box::pin(async move {
            let playback = self
                .0
                .playback()
                .get_playback_info(
                    &request.item_id,
                    request.start_time_ticks,
                    request.selection.audio_stream_index,
                    request.selection.subtitle_stream_index,
                )
                .await
                .map_err(|error| {
                    log::warn!("playback info request failed: {error}");
                    PlaybackError::PlaybackInfoUnavailable
                })?;
            let media_source = select_media_source(
                &playback.media_sources,
                request.selection.media_source_id.as_deref(),
            )?
            .clone();
            let stream_url = self
                .0
                .playback()
                .build_stream_url(&request.item_id, &media_source)
                .map(AuthenticatedUrl::new)
                .ok_or(PlaybackError::StreamUrlUnavailable)?;
            let external_subtitles =
                discover_external_subtitles(&media_source.media_streams, |stream| {
                    self.0
                        .playback()
                        .build_subtitle_url(&request.item_id, &media_source.id, stream)
                        .map(AuthenticatedUrl::new)
                });

            Ok(PlaybackResolution {
                media_source,
                play_session_id: playback.play_session_id,
                stream_url,
                external_subtitles,
            })
        })
    }

    fn playback_audio_context(
        &self,
        item_id: &str,
    ) -> PlaybackServerFuture<'_, Option<PlaybackAudioContext>> {
        let item_id = item_id.to_owned();
        Box::pin(async move {
            match self.0.library().playback_audio_context(&item_id).await {
                Ok(context) => Some(context),
                Err(error) => {
                    log::warn!("playback audio context request failed: {error}");
                    None
                }
            }
        })
    }

    fn report_playback_start(
        &self,
        report: PlaybackReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>> {
        Box::pin(async move {
            self.0
                .playback()
                .report_playback_start(&PlaybackStartInfo::from(report))
                .await
                .map_err(|_| ())
        })
    }

    fn report_playback_progress(
        &self,
        report: PlaybackReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>> {
        Box::pin(async move {
            self.0
                .playback()
                .report_playback_progress(&PlaybackProgressInfo::from(report))
                .await
                .map_err(|_| ())
        })
    }

    fn report_playback_stop(
        &self,
        report: PlaybackStopReport,
    ) -> PlaybackServerFuture<'_, Result<(), ()>> {
        Box::pin(async move {
            self.0
                .playback()
                .report_playback_stop(&PlaybackStopInfo::from(report))
                .await
                .map_err(|_| ())
        })
    }
}

impl From<PlaybackReport> for PlaybackStartInfo {
    fn from(report: PlaybackReport) -> Self {
        Self {
            item_id: report.item_id,
            media_source_id: Some(report.media_source_id),
            play_session_id: report.play_session_id,
            position_ticks: report.position_ticks,
            is_paused: report.is_paused,
            is_muted: report.is_muted,
            volume_level: report.volume_level,
            audio_stream_index: report.audio_stream_index,
            subtitle_stream_index: report.subtitle_stream_index,
            play_method: report.play_method,
            can_seek: true,
        }
    }
}

impl From<PlaybackReport> for PlaybackProgressInfo {
    fn from(report: PlaybackReport) -> Self {
        Self {
            item_id: report.item_id,
            media_source_id: Some(report.media_source_id),
            play_session_id: report.play_session_id,
            position_ticks: report.position_ticks,
            is_paused: report.is_paused,
            is_muted: report.is_muted,
            volume_level: report.volume_level,
            audio_stream_index: report.audio_stream_index,
            subtitle_stream_index: report.subtitle_stream_index,
            play_method: report.play_method,
            can_seek: true,
        }
    }
}

impl From<PlaybackStopReport> for PlaybackStopInfo {
    fn from(report: PlaybackStopReport) -> Self {
        Self {
            item_id: report.item_id,
            media_source_id: Some(report.media_source_id),
            play_session_id: report.play_session_id,
            position_ticks: report.position_ticks,
        }
    }
}

/// Authenticated playback URL whose debug output never exposes credentials.
#[derive(Clone)]
pub struct AuthenticatedUrl(String);

impl AuthenticatedUrl {
    /// Wrap an authenticated playback URL.
    #[must_use]
    pub fn new(url: String) -> Self {
        Self(url)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthenticatedUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthenticatedUrl([redacted])")
    }
}

pub fn checked_seconds_to_ticks(seconds: f64) -> Result<i64, PlaybackError> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(PlaybackError::InvalidStartPosition);
    }
    let ticks = seconds * MEDIA_TICKS_PER_SECOND as f64;
    if ticks > i64::MAX as f64 {
        return Err(PlaybackError::InvalidStartPosition);
    }
    Ok(ticks.round() as i64)
}

pub fn select_media_source<'a>(
    media_sources: &'a [MediaSource],
    selected_id: Option<&str>,
) -> Result<&'a MediaSource, PlaybackError> {
    match selected_id {
        Some(id) => media_sources.iter().find(|source| source.id == id),
        None => media_sources.first(),
    }
    .ok_or(PlaybackError::MediaSourceUnavailable)
}

/// Automatic audio choice shared by the pre-request and post-resolve passes:
/// a remembered per-series or per-item track first, then the original language.
pub fn auto_audio_index(
    choices: &[AudioStreamChoice<'_>],
    memory: Option<&AudioTrackPreference>,
    native: Option<&str>,
    server_default_index: Option<i32>,
) -> Option<i32> {
    memory
        .and_then(|preference| {
            select_audio_stream_by_memory(
                choices,
                &preference.language,
                preference.title.as_deref(),
            )
        })
        .or_else(|| {
            native.and_then(|language| {
                select_native_audio_stream(choices, language, server_default_index)
            })
        })
}

pub fn find_stream<'a>(
    streams: &'a [MediaStream],
    stream_type: &str,
    provider_index: i32,
) -> Result<&'a MediaStream, PlaybackError> {
    streams
        .iter()
        .find(|stream| stream.stream_type == stream_type && stream.index == provider_index)
        .ok_or(PlaybackError::TrackUnavailable)
}

pub fn resolve_mpv_track(
    streams: &[MediaStream],
    stream_type: &str,
    selected_index: Option<i32>,
) -> Result<Option<i64>, PlaybackError> {
    selected_index
        .map(|index| {
            if index < 0 {
                Ok(i64::from(index))
            } else {
                type_local_track_index(streams, stream_type, index)
            }
        })
        .transpose()
}

pub fn type_local_track_index(
    streams: &[MediaStream],
    stream_type: &str,
    provider_index: i32,
) -> Result<i64, PlaybackError> {
    find_stream(streams, stream_type, provider_index)?;
    streams
        .iter()
        .filter(|stream| {
            stream.stream_type == stream_type && (stream_type != "Subtitle" || !stream.is_external)
        })
        .position(|stream| stream.index == provider_index)
        .and_then(|position| i64::try_from(position).ok())
        .and_then(|position| position.checked_add(1))
        .ok_or(PlaybackError::TrackUnavailable)
}

pub fn play_method(media_source: &MediaSource) -> &'static str {
    if media_source.supports_direct_play {
        "DirectPlay"
    } else if media_source.supports_direct_stream && media_source.direct_stream_url.is_some() {
        "DirectStream"
    } else {
        "DirectPlay"
    }
}

pub fn item_title(
    name: &str,
    item_type: &str,
    series_name: Option<&str>,
    season_number: Option<i32>,
    episode_number: Option<i32>,
) -> String {
    if item_type != "Episode" {
        return name.to_owned();
    }
    let Some(series_name) = series_name else {
        return name.to_owned();
    };
    match (season_number, episode_number) {
        (Some(season), Some(episode)) => {
            format!("{series_name} - S{season:02}E{episode:02} - {name}")
        }
        _ => format!("{series_name} - {name}"),
    }
}

/// Frontend-independent preparation facts. The caller owns presentation metadata.
pub struct PreparationRequest {
    pub item_id: String,
    pub start_position_seconds: f64,
    pub selection: PlaybackSelection,
    pub runtime_seconds: Option<f64>,
    pub original_language: Option<String>,
}

#[derive(Default)]
pub struct PreparationPreferences {
    pub original_audio_enabled: bool,
    pub subtitle_languages: Vec<String>,
    pub remembered_audio: Option<AudioTrackPreference>,
    pub remembered_subtitle: Option<SubtitleTrackPreference>,
}

/// Resolved original/direct media plus the common initial track decisions.
#[derive(Debug)]
pub struct PreparedMedia {
    pub resolution: PlaybackResolution,
    pub runtime_seconds: Option<f64>,
    pub original_language: Option<String>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub selected_external_subtitle_index: Option<i32>,
    pub external_subtitle_unavailable: bool,
    pub mpv_audio_index: Option<i64>,
    pub mpv_subtitle_index: Option<i64>,
}

/// Desktop and Android share source, language memory, subtitle and track mapping rules.
pub async fn prepare_media(
    server: &dyn PlaybackServer,
    request: PreparationRequest,
    preferences: PreparationPreferences,
) -> Result<PreparedMedia, PlaybackError> {
    let start_position_seconds = request.start_position_seconds;
    let start_position_ticks = checked_seconds_to_ticks(start_position_seconds)?;
    let server_start_ticks =
        matches!(server.provider(), MediaServerProvider::Emby).then_some(start_position_ticks);
    let mut selection = request.selection;
    let mut original_language = request.original_language.clone();
    // Automatic audio selection (memory first, then original language) needs
    // provider stream facts before the playback info request; explicit selections
    // and media-source choices always win. The context resolves original languages
    // via TMDb, so both providers work.
    let mut auto_audio: Option<(Option<AudioTrackPreference>, Option<String>)> = None;
    if selection.audio_stream_index.is_none() && selection.media_source_id.is_none() {
        let remembered = preferences.remembered_audio.clone();
        if remembered.is_some() || preferences.original_audio_enabled {
            if let Some(context) = server.playback_audio_context(&request.item_id).await {
                let native = if preferences.original_audio_enabled {
                    context
                        .original_language
                        .clone()
                        .or_else(|| request.original_language.clone())
                } else {
                    None
                };
                let choices: Vec<AudioStreamChoice<'_>> =
                    context.audio_streams.iter().map(Into::into).collect();
                selection.audio_stream_index =
                    auto_audio_index(&choices, remembered.as_ref(), native.as_deref(), None);
                auto_audio = Some((remembered, native));
                original_language = original_language.or(context.original_language);
            }
        }
    }
    let PlaybackResolution {
        media_source,
        play_session_id,
        stream_url,
        external_subtitles,
    } = server
        .resolve(PlaybackResolutionRequest {
            item_id: request.item_id.clone(),
            start_time_ticks: server_start_ticks,
            selection: selection.clone(),
        })
        .await?;
    if let Some((memory, native)) = &auto_audio {
        // Re-derive against the resolved source: playback info can refresh streams
        // (e.g. .strm metadata), so a stale pre-request index must not abort
        // playback, and the server's own default audio index wins over the
        // container flag.
        let choices: Vec<AudioStreamChoice<'_>> = media_source
            .media_streams
            .iter()
            .filter(|stream| stream.stream_type == "Audio")
            .map(Into::into)
            .collect();
        selection.audio_stream_index = auto_audio_index(
            &choices,
            memory.as_ref(),
            native.as_deref(),
            media_source.default_audio_stream_index,
        );
    }
    let runtime_seconds = request.runtime_seconds.or_else(|| {
        media_source
            .run_time_ticks
            .filter(|ticks| *ticks >= 0)
            .map(ticks_to_seconds)
    });
    let external_subtitle_unavailable = media_source
        .media_streams
        .iter()
        .filter(|stream| stream.stream_type == "Subtitle" && stream.is_external)
        .count()
        != external_subtitles.len();
    let series_subtitle_preference =
        preferences
            .remembered_subtitle
            .as_ref()
            .map(|preference| TrackPreference {
                subtitle_language: preference.language.clone(),
                subtitle_title: preference.title.clone(),
                subtitle_preference_set: true,
                is_subtitle_enabled: preference.enabled,
                ..TrackPreference::default()
            });
    let effective_subtitle_index = select_subtitle_stream_index(
        selection.subtitle_stream_index,
        series_subtitle_preference.as_ref(),
        &media_source.media_streams,
        &preferences.subtitle_languages,
    )
    .or(media_source.default_subtitle_stream_index)
    .or_else(|| {
        media_source
            .media_streams
            .iter()
            .find(|stream| stream.stream_type == "Subtitle" && stream.is_default)
            .map(|stream| stream.index)
    });
    let selected_external_subtitle_index = effective_subtitle_index
        .filter(|index| *index >= 0)
        .map(|index| {
            find_stream(&media_source.media_streams, "Subtitle", index)
                .map(|stream| (index, stream))
        })
        .transpose()?
        .filter(|(_, stream)| stream.is_external)
        .map(|(index, _)| index);
    if selected_external_subtitle_index.is_some_and(|selected| {
        !external_subtitles
            .iter()
            .any(|subtitle| subtitle.provider_index == selected)
    }) {
        return Err(PlaybackError::SubtitleUrlUnavailable);
    }
    let mpv_audio_index = resolve_mpv_track(
        &media_source.media_streams,
        "Audio",
        selection.audio_stream_index,
    )?;
    let mpv_subtitle_index = if selected_external_subtitle_index.is_some() {
        None
    } else {
        resolve_mpv_track(
            &media_source.media_streams,
            "Subtitle",
            effective_subtitle_index,
        )?
    };

    Ok(PreparedMedia {
        resolution: PlaybackResolution {
            media_source,
            play_session_id,
            stream_url,
            external_subtitles,
        },
        runtime_seconds,
        original_language,
        audio_stream_index: selection.audio_stream_index,
        subtitle_stream_index: effective_subtitle_index,
        selected_external_subtitle_index,
        external_subtitle_unavailable,
        mpv_audio_index,
        mpv_subtitle_index,
    })
}
