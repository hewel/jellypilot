//! Public metadata contracts shared by the planner and playback reducer.

use std::fmt;

/// Duration of one fragmented-MP4 HLS media segment.
pub const ROLLING_HLS_SEGMENT_DURATION_SECONDS: u32 = 4;

/// Number of segments retained in the live rolling playlist.
pub const ROLLING_HLS_WINDOW_SEGMENTS: u32 = 15;

/// Approximate duration retained by the rolling playlist.
pub const ROLLING_HLS_WINDOW_DURATION_SECONDS: u32 =
    ROLLING_HLS_SEGMENT_DURATION_SECONDS * ROLLING_HLS_WINDOW_SEGMENTS;

/// Constant-rate-factor used by the software H.264 fallback.
pub const H264_SDR_CRF: u8 = 20;

/// Preset used by the software H.264 fallback.
pub const H264_SDR_PRESET: &str = "veryfast";

/// AAC bitrate used for mono or stereo output.
pub const AAC_STEREO_BITRATE_BPS: u32 = 192_000;

/// AAC bitrate used for output with more than two channels.
pub const AAC_MULTICHANNEL_BITRATE_BPS: u32 = 384_000;

/// Capabilities provided by the embedded playback state machine.
pub const EMBEDDED_PLAYBACK_CAPABILITIES: PlaybackCapabilities = PlaybackCapabilities {
    play: true,
    pause: true,
    resume: true,
    seek: true,
    restart: true,
    stop: true,
    replay: true,
    rolling_hls: true,
    mpv_fallback: true,
};

/// Container used by the generated HLS media playlist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HlsContainer {
    /// ISO fragmented MP4 segments with an initialization section.
    FragmentedMp4,
}

/// Fixed rolling-HLS packaging policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RollingHlsProfile {
    /// Media-segment container.
    pub container: HlsContainer,
    /// Target duration of each segment.
    pub segment_duration_seconds: u32,
    /// Number of segments retained in the playlist.
    pub window_segments: u32,
}

impl RollingHlsProfile {
    /// Returns JellyPilot's fixed fragmented-MP4 rolling-HLS policy.
    #[must_use]
    pub const fn rolling() -> Self {
        Self {
            container: HlsContainer::FragmentedMp4,
            segment_duration_seconds: ROLLING_HLS_SEGMENT_DURATION_SECONDS,
            window_segments: ROLLING_HLS_WINDOW_SEGMENTS,
        }
    }
}

impl Default for RollingHlsProfile {
    fn default() -> Self {
        Self::rolling()
    }
}

/// Desktop operating-system family used for deterministic encoder ordering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegPlatform {
    /// Apple macOS.
    MacOs,
    /// Microsoft Windows.
    Windows,
    /// Linux desktop distributions.
    Linux,
}

/// FFmpeg acceleration backend selected for one startup attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegEncoder {
    /// Apple VideoToolbox.
    VideoToolbox,
    /// Intel Quick Sync Video.
    QuickSync,
    /// NVIDIA NVENC.
    Nvenc,
    /// AMD Advanced Media Framework.
    Amf,
    /// Linux Video Acceleration API.
    Vaapi,
    /// CPU software encoder, always the final candidate.
    Software,
}

/// Compatible host encoders discovered before planning.
///
/// A `true` field means the encoder is usable for the requested source video
/// profile, not merely that the FFmpeg binary lists the encoder by name.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FfmpegEncoderAvailability {
    /// Whether a compatible VideoToolbox encoder is available.
    pub videotoolbox: bool,
    /// Whether a compatible Quick Sync encoder is available.
    pub quick_sync: bool,
    /// Whether a compatible NVENC encoder is available.
    pub nvenc: bool,
    /// Whether a compatible AMF encoder is available.
    pub amf: bool,
    /// Whether a compatible VAAPI encoder is available.
    pub vaapi: bool,
}

/// Browser decode and media-source capabilities relevant to embedded playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserPlaybackCapabilities {
    /// Whether fragmented-MP4 HLS can be consumed.
    pub fmp4_hls: bool,
    /// Whether H.264 SDR in `yuv420p` can be decoded.
    pub h264_sdr: bool,
    /// Whether HDR HEVC Main10 can be decoded without tone mapping.
    pub hevc_main10_hdr: bool,
    /// Whether AAC audio can be decoded.
    pub aac: bool,
    /// Maximum supported decoded audio-channel count.
    pub max_audio_channels: u8,
}

/// Container class normalized from the FFprobe format and major-brand facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbedContainer {
    /// ISO Base Media File Format with an MP4-compatible major brand.
    Mp4,
    /// Any container that is not strictly eligible for direct MP4 delivery.
    Other,
}

/// Video codec normalized from one selected FFprobe stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbedVideoCodec {
    /// Advanced Video Coding.
    H264,
    /// High Efficiency Video Coding.
    Hevc,
    /// A codec outside the embedded browser policy.
    Other,
}

/// Audio codec normalized from one selected FFprobe stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbedAudioCodec {
    /// Advanced Audio Coding.
    Aac,
    /// A codec outside the embedded browser policy.
    Other,
}

/// Pixel-layout class relevant to browser compatibility and HDR preservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbedPixelFormat {
    /// Eight-bit planar 4:2:0.
    Yuv420p,
    /// Ten-bit 4:2:0 in a planar or semi-planar representation.
    TenBit420,
    /// Any other or unknown pixel format.
    Other,
}

/// MP4 video sample entry normalized from `codec_tag_string`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbedVideoSampleEntry {
    /// `avc1` H.264 sample entry.
    Avc1,
    /// `hvc1` HEVC sample entry.
    Hvc1,
    /// Any other or unknown sample entry.
    Other,
}

/// Source dynamic range normalized from transfer characteristics and metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbedDynamicRange {
    /// Standard dynamic range.
    Sdr,
    /// HDR that must never be silently tone mapped.
    Hdr,
}

/// Normalized facts for the selected global video stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbedVideoStream {
    /// Global FFmpeg stream index.
    pub stream_index: u32,
    /// Normalized codec.
    pub codec: ProbedVideoCodec,
    /// Normalized pixel layout.
    pub pixel_format: ProbedPixelFormat,
    /// Normalized MP4 sample entry.
    pub sample_entry: ProbedVideoSampleEntry,
    /// Source dynamic range.
    pub dynamic_range: ProbedDynamicRange,
    /// Whether FFprobe identified the HEVC Main 10 profile.
    pub hevc_main10: bool,
}

/// Normalized facts for the selected global audio stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbedAudioStream {
    /// Global FFmpeg stream index.
    pub stream_index: u32,
    /// Normalized codec.
    pub codec: ProbedAudioCodec,
    /// Positive channel count.
    pub channels: u8,
}

/// Stable policy facts derived from one bounded FFprobe invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaProbeFacts {
    /// Normalized source container.
    pub container: ProbedContainer,
    /// Selected video stream.
    pub video: ProbedVideoStream,
    /// Selected audio stream, if the source contains audio.
    pub audio: Option<ProbedAudioStream>,
    /// Total number of video streams in the source.
    pub video_stream_count: u16,
    /// Total number of audio streams in the source.
    pub audio_stream_count: u16,
}

/// Safe, non-secret reason a media probe did not produce policy facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaProbeFailure {
    /// The FFprobe sidecar could not be started.
    SidecarUnavailable,
    /// FFprobe exceeded its fixed deadline.
    Timeout,
    /// FFprobe exited unsuccessfully.
    ProcessFailed,
    /// FFprobe output exceeded the bounded capture limit.
    OutputTooLarge,
    /// FFprobe returned malformed or incomplete JSON.
    InvalidOutput,
    /// The source contained no video stream.
    MissingVideoStream,
    /// The requested global audio stream was absent.
    SelectedAudioStreamMissing,
    /// The selected audio stream had an invalid channel count.
    InvalidAudioChannelCount,
}

/// Probe input consumed by the pure delivery planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaProbeResult {
    /// FFprobe produced normalized facts.
    Facts(MediaProbeFacts),
    /// Probing failed without exposing provider URLs or sidecar output.
    Failed(MediaProbeFailure),
}

/// Source video class supported by the embedded transcode policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceVideoProfile {
    /// Standard-dynamic-range video targeting H.264 `yuv420p`.
    H264Sdr,
    /// HDR video that must remain HEVC Main10 with no tone mapping.
    HevcMain10Hdr,
}

/// Source audio layout retained by the transcode profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioChannelLayout {
    /// One center channel.
    Mono,
    /// Left and right channels.
    Stereo,
    /// 5.1 surround sound.
    Surround51,
    /// 7.1 surround sound.
    Surround71,
    /// Another explicit positive channel count.
    Other(u8),
}

impl AudioChannelLayout {
    /// Returns the number of channels represented by this layout.
    #[must_use]
    pub const fn channel_count(self) -> u8 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
            Self::Other(channels) => channels,
        }
    }

    /// Builds the closest explicit layout representation for a channel count.
    #[must_use]
    pub const fn from_channel_count(channels: u8) -> Self {
        match channels {
            1 => Self::Mono,
            2 => Self::Stereo,
            6 => Self::Surround51,
            8 => Self::Surround71,
            other => Self::Other(other),
        }
    }
}

/// Input to [`crate::plan_ffmpeg`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FfmpegPlanRequest {
    /// Host platform that determines candidate order.
    pub platform: FfmpegPlatform,
    /// Compatible FFmpeg encoders present on the host.
    pub encoders: FfmpegEncoderAvailability,
    /// Browser playback capabilities that the profile must satisfy.
    pub browser: BrowserPlaybackCapabilities,
    /// Normalized probe facts, or a safe probe failure category.
    pub probe: MediaProbeResult,
}

/// Pixel format selected for video output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegPixelFormat {
    /// Eight-bit 4:2:0 planar YUV used for SDR H.264.
    Yuv420p,
    /// Ten-bit 4:2:0 semi-planar YUV used for HDR HEVC Main10.
    P010Le,
}

/// Locked settings for the software H.264 fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FfmpegSoftwareH264Profile {
    /// FFmpeg encoder name.
    pub encoder_name: &'static str,
    /// Software encoder speed/quality preset.
    pub preset: &'static str,
    /// Constant-rate-factor quality value.
    pub crf: u8,
}

/// Target video profile shared by all compatible encoder candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegVideoProfile {
    /// H.264 SDR output in broadly compatible eight-bit 4:2:0.
    H264Sdr {
        /// Required output pixel format.
        pixel_format: FfmpegPixelFormat,
        /// Locked software fallback settings.
        software: FfmpegSoftwareH264Profile,
    },
    /// HDR-preserving HEVC Main10 output with no tone mapping.
    HevcMain10Hdr {
        /// Required ten-bit output pixel format.
        pixel_format: FfmpegPixelFormat,
        /// Whether HDR metadata must be retained.
        preserve_hdr_metadata: bool,
        /// Whether tone mapping is enabled.
        tone_mapping: bool,
    },
}

/// AAC is the only audio output codec in the embedded profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegAudioCodec {
    /// Advanced Audio Coding.
    Aac,
}

/// Planned AAC output while retaining the source layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FfmpegAudioProfile {
    /// Output audio codec.
    pub codec: FfmpegAudioCodec,
    /// Channel layout retained from the source.
    pub channel_layout: AudioChannelLayout,
    /// Selected bitrate in bits per second.
    pub bitrate_bps: u32,
}

/// Concrete FFmpeg video encoder used by a startup candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegVideoEncoder {
    /// `h264_videotoolbox`.
    H264VideoToolbox,
    /// `hevc_videotoolbox`.
    HevcVideoToolbox,
    /// `h264_qsv`.
    H264QuickSync,
    /// `hevc_qsv`.
    HevcQuickSync,
    /// `h264_nvenc`.
    H264Nvenc,
    /// `hevc_nvenc`.
    HevcNvenc,
    /// `h264_amf`.
    H264Amf,
    /// `hevc_amf`.
    HevcAmf,
    /// `h264_vaapi`.
    H264Vaapi,
    /// `hevc_vaapi`.
    HevcVaapi,
    /// `libx264`.
    Libx264,
    /// `libx265`.
    Libx265,
}

impl FfmpegVideoEncoder {
    /// Returns the FFmpeg encoder name used on the command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::H264VideoToolbox => "h264_videotoolbox",
            Self::HevcVideoToolbox => "hevc_videotoolbox",
            Self::H264QuickSync => "h264_qsv",
            Self::HevcQuickSync => "hevc_qsv",
            Self::H264Nvenc => "h264_nvenc",
            Self::HevcNvenc => "hevc_nvenc",
            Self::H264Amf => "h264_amf",
            Self::HevcAmf => "hevc_amf",
            Self::H264Vaapi => "h264_vaapi",
            Self::HevcVaapi => "hevc_vaapi",
            Self::Libx264 => "libx264",
            Self::Libx265 => "libx265",
        }
    }
}

/// Browser delivery selected for one ordered startup candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaDelivery {
    /// Browser receives the strict MP4 source through an authorized loopback URL.
    DirectSource,
    /// Both selected streams are copied into fragmented-MP4 HLS.
    HlsRemux,
    /// Exactly one selected stream is transcoded into fragmented-MP4 HLS.
    HlsPartialTranscode,
    /// Both selected streams are transcoded into fragmented-MP4 HLS.
    HlsFullTranscode,
}

/// Independent codec decision for one selected stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamDecision {
    /// Preserve compressed packets without re-encoding.
    Copy,
    /// Re-encode into the plan's browser-compatible target.
    Transcode,
}

/// One ordered direct-source or FFmpeg-HLS startup candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FfmpegCandidate {
    /// Browser delivery family represented by this attempt.
    pub delivery: MediaDelivery,
    /// Independent video codec decision.
    pub video: StreamDecision,
    /// Independent audio codec decision, or `None` for silent media.
    pub audio: Option<StreamDecision>,
    /// Acceleration family used when video is transcoded.
    pub encoder: Option<FfmpegEncoder>,
    /// Concrete FFmpeg encoder used when video is transcoded.
    pub video_encoder: Option<FfmpegVideoEncoder>,
}

/// Source properties intentionally left unchanged by the transcode policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreservedMediaProperties {
    /// Whether source resolution is retained.
    pub resolution: bool,
    /// Whether source frame rate is retained.
    pub frame_rate: bool,
    /// Whether source display aspect ratio is retained.
    pub aspect_ratio: bool,
    /// Whether source audio layout is retained.
    pub audio_channel_layout: bool,
}

/// Deterministic FFmpeg profile and ordered startup candidates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FfmpegPlan {
    /// Normalized source facts used to select and map streams.
    pub probe: MediaProbeFacts,
    /// Fixed rolling-HLS packaging settings.
    pub hls: RollingHlsProfile,
    /// Target video profile.
    pub video: FfmpegVideoProfile,
    /// Target audio profile, if the source has audio.
    pub audio: Option<FfmpegAudioProfile>,
    /// Media properties that must not be transformed.
    pub preserved: PreservedMediaProperties,
    /// Compatible startup candidates in deterministic preference order.
    pub candidates: Vec<FfmpegCandidate>,
}

/// Explicit reason FFmpeg planning cannot produce embedded playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegPlanError {
    /// The media probe failed before delivery planning.
    ProbeFailed(MediaProbeFailure),
    /// The browser cannot consume fragmented-MP4 HLS.
    FragmentedMp4HlsUnsupported,
    /// The browser cannot decode the required H.264 SDR profile.
    H264SdrUnsupported,
    /// The browser cannot preserve and decode HEVC Main10 HDR.
    HevcMain10HdrUnsupported,
    /// The browser cannot decode AAC audio.
    AacUnsupported,
    /// The source contains a zero-channel custom audio layout.
    InvalidAudioChannelCount,
    /// The browser cannot decode the retained source channel layout.
    AudioChannelCountUnsupported {
        /// Source channel count that policy refuses to downmix.
        source_channels: u8,
        /// Maximum channel count exposed by the browser.
        browser_max_channels: u8,
    },
    /// HDR input was not exact HEVC Main 10 and cannot be preserved safely.
    UnsupportedHdrSource,
    /// Neither strict direct MP4 nor fragmented-MP4 HLS can be delivered.
    NoCompatibleDelivery,
}

impl fmt::Display for FfmpegPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProbeFailed(failure) => write!(formatter, "media probe failed: {failure:?}"),
            Self::FragmentedMp4HlsUnsupported => {
                formatter.write_str("browser does not support fragmented-MP4 HLS")
            }
            Self::H264SdrUnsupported => {
                formatter.write_str("browser does not support H.264 SDR yuv420p playback")
            }
            Self::HevcMain10HdrUnsupported => formatter
                .write_str("browser cannot preserve HEVC Main10 HDR without tone mapping"),
            Self::AacUnsupported => formatter.write_str("browser does not support AAC audio"),
            Self::InvalidAudioChannelCount => {
                formatter.write_str("source audio channel count must be positive")
            }
            Self::AudioChannelCountUnsupported {
                source_channels,
                browser_max_channels,
            } => write!(
                formatter,
                "browser supports {browser_max_channels} audio channels but source layout requires {source_channels}"
            ),
            Self::UnsupportedHdrSource => formatter
                .write_str("HDR source is not preservable HEVC Main 10; tone mapping is disabled"),
            Self::NoCompatibleDelivery => formatter
                .write_str("source has no compatible direct-source or HLS delivery plan"),
        }
    }
}

impl std::error::Error for FfmpegPlanError {}

/// Stable generation identifying one embedded pipeline attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlaybackGeneration(
    /// Monotonic generation number scoped to one core instance.
    pub u64,
);

/// Identifies one ordered browser observation within a pipeline generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackObservationToken {
    /// Pipeline generation observed by the browser.
    pub generation: PlaybackGeneration,
    /// Strictly increasing sequence within that generation.
    pub sequence: u64,
}

/// Browser media state carried by an observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserPlaybackState {
    /// Media is advancing.
    Playing,
    /// Media is intentionally paused.
    Paused,
    /// Media is temporarily buffering.
    Buffering,
    /// Media reached its natural end.
    Ended,
    /// Media stopped outside a natural end.
    Stopped,
    /// Media exceeded the caller's stall threshold.
    Stalled {
        /// Human-readable stall diagnosis.
        message: String,
    },
    /// Browser playback failed at runtime.
    Failed {
        /// Human-readable browser failure.
        message: String,
    },
}

/// One browser observation fed back to the reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserObservation {
    /// Generation and monotonic sequence used for stale rejection.
    pub token: PlaybackObservationToken,
    /// Observed browser media state.
    pub state: BrowserPlaybackState,
    /// Best-known playback position in Jellyfin ticks.
    pub position_ticks: u64,
}

/// Media and policy metadata needed to start one playback lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSession {
    /// Jellyfin or Emby item identifier.
    pub item_id: String,
    /// Selected media-source identifier, when supplied by the server.
    pub media_source_id: Option<String>,
    /// Server playback-session identifier, when supplied by the server.
    pub play_session_id: Option<String>,
    /// Initial playback position in Jellyfin ticks.
    pub start_position_ticks: u64,
    /// Known media duration in Jellyfin ticks.
    pub duration_ticks: Option<u64>,
    /// Explicit browser, source, platform, and encoder planning input.
    pub plan_request: FfmpegPlanRequest,
    /// Whether the host can offer external MPV after embedded failure.
    pub mpv_fallback_available: bool,
}

/// Stable session identity exposed by commands and snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSessionSummary {
    /// Jellyfin or Emby item identifier.
    pub item_id: String,
    /// Selected media-source identifier.
    pub media_source_id: Option<String>,
    /// Server playback-session identifier.
    pub play_session_id: Option<String>,
    /// Known media duration in Jellyfin ticks.
    pub duration_ticks: Option<u64>,
}

impl From<&PlaybackSession> for PlaybackSessionSummary {
    fn from(session: &PlaybackSession) -> Self {
        Self {
            item_id: session.item_id.clone(),
            media_source_id: session.media_source_id.clone(),
            play_session_id: session.play_session_id.clone(),
            duration_ticks: session.duration_ticks,
        }
    }
}

/// User-visible lifecycle phase for the active or most recent session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackPhase {
    /// No playback has been requested.
    Idle,
    /// The first pipeline generation is starting.
    Starting,
    /// Browser media is advancing.
    Playing,
    /// Browser media is paused.
    Paused,
    /// Browser media is buffering.
    Buffering,
    /// A replacement generation is starting at a requested position.
    Seeking,
    /// A replacement generation is restarting at the current position.
    Restarting,
    /// A stop was requested and the browser acknowledgement is pending.
    Stopping,
    /// Playback stopped without reaching the natural end.
    Stopped,
    /// Playback reached its natural end.
    Ended,
    /// Planning, startup, runtime, or stall failure is visible.
    Failed,
}

/// Action category used in typed invalid-transition errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackActionKind {
    /// Start or replace playback with a session.
    Play,
    /// Pause active playback.
    Pause,
    /// Resume paused playback.
    Resume,
    /// Seek active playback.
    Seek,
    /// Restart active playback.
    Restart,
    /// Stop active playback.
    Stop,
    /// Replay the retained session from zero.
    Replay,
    /// Settle a failed startup candidate.
    StartupFailed,
    /// Apply browser-observed media state.
    BrowserObserved,
}

/// Inputs accepted by [`crate::EmbeddedPlaybackCore::dispatch`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaybackAction {
    /// Starts a new playback lifecycle, replacing an active one if necessary.
    Play(PlaybackSession),
    /// Requests pause.
    Pause,
    /// Requests resume from pause.
    Resume,
    /// Replaces the pipeline at an absolute position.
    Seek {
        /// Target position in Jellyfin ticks.
        position_ticks: u64,
    },
    /// Replaces the pipeline at the current position.
    Restart,
    /// Stops the active pipeline and closes its reporting lifecycle.
    Stop,
    /// Starts a new reporting lifecycle for the retained session from zero.
    Replay,
    /// Reports that the active FFmpeg candidate failed before browser playback.
    StartupFailed {
        /// Generation whose startup failed.
        generation: PlaybackGeneration,
        /// Human-readable startup failure.
        message: String,
        /// Whether another ordered delivery candidate may safely be attempted.
        retryable: bool,
    },
    /// Applies browser media state with generation and sequence protection.
    BrowserObserved(BrowserObservation),
}

impl PlaybackAction {
    /// Returns the category of this action.
    #[must_use]
    pub const fn kind(&self) -> PlaybackActionKind {
        match self {
            Self::Play(_) => PlaybackActionKind::Play,
            Self::Pause => PlaybackActionKind::Pause,
            Self::Resume => PlaybackActionKind::Resume,
            Self::Seek { .. } => PlaybackActionKind::Seek,
            Self::Restart => PlaybackActionKind::Restart,
            Self::Stop => PlaybackActionKind::Stop,
            Self::Replay => PlaybackActionKind::Replay,
            Self::StartupFailed { .. } => PlaybackActionKind::StartupFailed,
            Self::BrowserObserved(_) => PlaybackActionKind::BrowserObserved,
        }
    }
}

/// Reason a pipeline generation is starting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackStartReason {
    /// A new item was requested.
    Play,
    /// Playback moved to an absolute position.
    Seek,
    /// The current item restarted at its retained position.
    Restart,
    /// A terminal session replayed from zero.
    Replay,
    /// Startup advanced to the next ordered FFmpeg candidate.
    CandidateFallback,
}

/// Self-contained metadata for starting one embedded pipeline attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackAttempt {
    /// New pipeline generation.
    pub generation: PlaybackGeneration,
    /// Why this generation was created.
    pub reason: PlaybackStartReason,
    /// Stable media and reporting identity.
    pub session: PlaybackSessionSummary,
    /// Absolute start position in Jellyfin ticks.
    pub start_position_ticks: u64,
    /// Whether the browser should remain paused after startup.
    pub paused: bool,
    /// Zero-based candidate index within the plan.
    pub candidate_index: usize,
    /// Concrete candidate selected for this attempt.
    pub candidate: FfmpegCandidate,
    /// Complete deterministic FFmpeg plan.
    pub plan: FfmpegPlan,
}

/// Server reporting metadata emitted after browser observations or user stop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackReport {
    /// Pipeline generation that produced the report.
    pub generation: PlaybackGeneration,
    /// Stable media and reporting identity.
    pub session: PlaybackSessionSummary,
    /// Best-known playback position in Jellyfin ticks.
    pub position_ticks: u64,
    /// Whether playback is paused.
    pub paused: bool,
}

/// Reason a server playback lifecycle stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackStopReason {
    /// The user explicitly stopped playback.
    User,
    /// Media reached its natural end.
    Ended,
    /// A new play request replaced the active item.
    Replaced,
    /// Every compatible FFmpeg startup candidate failed.
    StartupFailed,
    /// Browser playback failed after startup.
    RuntimeFailure,
    /// Browser playback exceeded its stall threshold.
    Stalled,
    /// The browser stopped without another known reason.
    BrowserStopped,
}

/// Ordered side effects emitted by the pure reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaybackCommand {
    /// Starts a new FFmpeg and browser pipeline generation.
    StartEmbedded {
        /// Self-contained attempt metadata.
        attempt: PlaybackAttempt,
    },
    /// Changes browser pause state without replacing the generation.
    SetPaused {
        /// Active generation to mutate.
        generation: PlaybackGeneration,
        /// Desired pause state.
        paused: bool,
    },
    /// Stops and retires one pipeline generation.
    StopEmbedded {
        /// Generation to retire.
        generation: PlaybackGeneration,
    },
    /// Reports the first confirmed playing observation to the server.
    ReportStarted {
        /// Reporting metadata.
        report: PlaybackReport,
    },
    /// Reports confirmed position or pause progress to the server.
    ReportProgress {
        /// Reporting metadata.
        report: PlaybackReport,
    },
    /// Closes the server reporting lifecycle exactly once.
    ReportStopped {
        /// Reporting metadata.
        report: PlaybackReport,
        /// Terminal reason.
        reason: PlaybackStopReason,
    },
}

/// Stage at which terminal embedded playback failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackFailureStage {
    /// Browser/source capabilities could not produce a policy-compliant plan.
    Planning,
    /// All compatible FFmpeg candidates failed to start.
    Startup,
    /// Browser playback failed after startup.
    Runtime,
    /// Browser playback exceeded the caller's stall threshold.
    Stall,
}

/// One retained non-terminal FFmpeg startup failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackAttemptFailure {
    /// Generation that failed.
    pub generation: PlaybackGeneration,
    /// Candidate that failed.
    pub candidate: FfmpegCandidate,
    /// Human-readable startup failure.
    pub message: String,
}

/// Terminal playback failure retained in the snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackFailure {
    /// Failure stage.
    pub stage: PlaybackFailureStage,
    /// Generation active at failure time.
    pub generation: PlaybackGeneration,
    /// Candidate active at failure time, if planning reached a candidate.
    pub candidate: Option<FfmpegCandidate>,
    /// Human-readable failure description.
    pub message: String,
    /// Whether retry or replay may re-run embedded policy.
    pub retryable: bool,
}

/// Reason an explicit external-MPV fallback is being offered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MpvFallbackReason {
    /// Browser capabilities cannot satisfy the required media profile.
    UnsupportedBrowserCapabilities,
    /// Every ordered FFmpeg startup candidate failed.
    FfmpegCandidatesExhausted,
    /// Browser playback failed after starting.
    RuntimeFailure,
    /// Browser playback stalled beyond the caller's threshold.
    BrowserStall,
}

/// Explicit fallback metadata retained with a terminal failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MpvFallbackMetadata {
    /// Whether external MPV is present and may be offered.
    pub available: bool,
    /// Why embedded playback cannot continue.
    pub reason: MpvFallbackReason,
    /// Item identifier to transfer to MPV.
    pub item_id: String,
    /// Selected media-source identifier to transfer to MPV.
    pub media_source_id: Option<String>,
    /// Best-known resume position in Jellyfin ticks.
    pub resume_position_ticks: u64,
}

/// Transport and fallback features implemented by the core contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackCapabilities {
    /// A new item can be played.
    pub play: bool,
    /// Active playback can be paused.
    pub pause: bool,
    /// Paused playback can be resumed.
    pub resume: bool,
    /// Active playback can seek.
    pub seek: bool,
    /// Active playback can restart.
    pub restart: bool,
    /// Active playback can stop.
    pub stop: bool,
    /// A terminal retained session can replay.
    pub replay: bool,
    /// Playback uses bounded rolling HLS.
    pub rolling_hls: bool,
    /// Terminal embedded failures carry MPV fallback metadata.
    pub mpv_fallback: bool,
}

/// Compatibility name emphasizing that capabilities belong to embedded playback.
pub type EmbeddedPlaybackCapabilities = PlaybackCapabilities;

/// Result of attempting to apply a browser observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackObservationDisposition {
    /// The dispatched action was not a browser observation.
    NotObserved,
    /// The observation mutated active state.
    Applied,
    /// The observation belongs to an older or unknown generation.
    IgnoredStaleGeneration,
    /// Its sequence was not greater than the last accepted sequence.
    IgnoredStaleSequence,
    /// The active phase is terminal and cannot accept observations.
    IgnoredTerminalPhase,
}

/// Immutable view returned after every transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSnapshot {
    /// Current lifecycle phase.
    pub phase: PlaybackPhase,
    /// Current or most recent pipeline generation.
    pub generation: Option<PlaybackGeneration>,
    /// Current or retained session identity.
    pub session: Option<PlaybackSessionSummary>,
    /// Best-known playback position in Jellyfin ticks.
    pub position_ticks: u64,
    /// Known media duration in Jellyfin ticks.
    pub duration_ticks: Option<u64>,
    /// Desired or observed pause state.
    pub paused: bool,
    /// Last accepted browser sequence for the current generation.
    pub last_observation_sequence: Option<u64>,
    /// Whether the current generation has emitted a confirmed playing observation.
    pub generation_has_played: bool,
    /// Complete active FFmpeg plan.
    pub active_plan: Option<FfmpegPlan>,
    /// Candidate active for the current generation.
    pub active_candidate: Option<FfmpegCandidate>,
    /// Startup failures retained across ordered candidate advancement.
    pub attempt_failures: Vec<PlaybackAttemptFailure>,
    /// Terminal embedded failure, if any.
    pub failure: Option<PlaybackFailure>,
    /// Explicit MPV fallback metadata paired with a terminal failure.
    pub mpv_fallback: Option<MpvFallbackMetadata>,
    /// Features provided by this core contract.
    pub capabilities: PlaybackCapabilities,
}

/// One pure reducer transition and its ordered effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackUpdate {
    /// State after applying the action.
    pub snapshot: PlaybackSnapshot,
    /// Effects the caller must execute in vector order.
    pub commands: Vec<PlaybackCommand>,
    /// Stale-rejection result for browser observations.
    pub observation: PlaybackObservationDisposition,
}

/// Invalid input or transition that leaves core state unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackCoreError {
    /// A playback session must identify a non-empty item.
    EmptyItemId,
    /// Initial or seek position exceeds the known duration.
    PositionAfterDuration {
        /// Rejected position in Jellyfin ticks.
        position_ticks: u64,
        /// Known duration in Jellyfin ticks.
        duration_ticks: u64,
    },
    /// No later pipeline generation can be represented.
    GenerationExhausted,
    /// The requested action is not valid in the current phase.
    InvalidTransition {
        /// Rejected action category.
        action: PlaybackActionKind,
        /// Phase that rejected the action.
        phase: PlaybackPhase,
    },
}

impl fmt::Display for PlaybackCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyItemId => formatter.write_str("playback item id is empty"),
            Self::PositionAfterDuration {
                position_ticks,
                duration_ticks,
            } => write!(
                formatter,
                "playback position {position_ticks} exceeds duration {duration_ticks}"
            ),
            Self::GenerationExhausted => {
                formatter.write_str("embedded playback generation is exhausted")
            }
            Self::InvalidTransition { action, phase } => {
                write!(
                    formatter,
                    "action {action:?} is invalid while playback is {phase:?}"
                )
            }
        }
    }
}

impl std::error::Error for PlaybackCoreError {}
