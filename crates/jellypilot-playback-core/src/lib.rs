//! UI-agnostic policy for one active embedded playback session.
//!
//! [`EmbeddedPlaybackCore`] owns lifecycle metadata and emits ordered commands;
//! it performs no I/O, starts no process, and stores no media payloads. The
//! accompanying FFmpeg planner selects deterministic rolling-HLS profiles and
//! encoder candidates from explicit host and browser capabilities. The
//! [`ffmpeg_cli`] module converts a plan and candidate into the exact argv a
//! local FFmpeg process should receive.

#![deny(missing_docs)]

mod ffmpeg_cli;
mod model;
mod planner;
mod reducer;

pub use ffmpeg_cli::{ffmpeg_argv, DynamicRange, FfmpegCliError, FfmpegCliRequest};
pub use model::{
    AudioChannelLayout, BrowserObservation, BrowserPlaybackCapabilities, BrowserPlaybackState,
    EmbeddedPlaybackCapabilities, FfmpegAudioCodec, FfmpegAudioProfile, FfmpegCandidate,
    FfmpegEncoder, FfmpegEncoderAvailability, FfmpegPixelFormat, FfmpegPlan, FfmpegPlanError,
    FfmpegPlanRequest, FfmpegPlatform, FfmpegSoftwareH264Profile, FfmpegVideoEncoder,
    FfmpegVideoProfile, HlsContainer, MediaDelivery, MediaProbeFacts, MediaProbeFailure,
    MediaProbeResult, MpvFallbackMetadata, MpvFallbackReason, PlaybackAction, PlaybackActionKind,
    PlaybackAttempt, PlaybackAttemptFailure, PlaybackCapabilities, PlaybackCommand,
    PlaybackCoreError, PlaybackFailure, PlaybackFailureStage, PlaybackGeneration,
    PlaybackObservationDisposition, PlaybackObservationToken, PlaybackPhase, PlaybackReport,
    PlaybackSession, PlaybackSessionSummary, PlaybackSnapshot, PlaybackStartReason,
    PlaybackStopReason, PlaybackUpdate, PreservedMediaProperties, ProbedAudioCodec,
    ProbedAudioStream, ProbedContainer, ProbedDynamicRange, ProbedPixelFormat, ProbedVideoCodec,
    ProbedVideoSampleEntry, ProbedVideoStream, RollingHlsProfile, SourceVideoProfile,
    StreamDecision, AAC_MULTICHANNEL_BITRATE_BPS, AAC_STEREO_BITRATE_BPS,
    EMBEDDED_PLAYBACK_CAPABILITIES, H264_SDR_CRF, H264_SDR_PRESET,
    ROLLING_HLS_SEGMENT_DURATION_SECONDS, ROLLING_HLS_WINDOW_DURATION_SECONDS,
    ROLLING_HLS_WINDOW_SEGMENTS,
};
pub use planner::plan_ffmpeg;
pub use reducer::EmbeddedPlaybackCore;
