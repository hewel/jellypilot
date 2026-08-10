//! Deterministic FFmpeg command-line argument generation.
//!
//! Converts an [`FfmpegPlan`] attempt into the exact argv a local FFmpeg
//! process should receive. The caller supplies the concrete source URL,
//! output directory, and runtime selections; the core produces a
//! reproducible argument vector that honours the plan's codec, container,
//! and HLS packaging policy.

use std::path::Path;

use crate::{
    FfmpegCandidate, FfmpegEncoder, FfmpegPlan, FfmpegVideoProfile, MediaDelivery,
    ProbedVideoCodec, RollingHlsProfile, StreamDecision,
};

/// Invalid runtime inputs that prevent a safe FFmpeg command from being built.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegCliError {
    /// A VAAPI candidate was selected without the exact render node that passed
    /// the runtime encoder smoke test.
    MissingVerifiedVaapiDevice,
}

impl std::fmt::Display for FfmpegCliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVerifiedVaapiDevice => {
                formatter.write_str("VAAPI encoding requires a verified render node")
            }
        }
    }
}

impl std::error::Error for FfmpegCliError {}

/// Dynamic-range class derived from the source video profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicRange {
    /// Standard-dynamic-range output.
    Sdr,
    /// High-dynamic-range output that must not be tone-mapped.
    Hdr,
}

impl DynamicRange {
    /// Derives the dynamic-range class from a concrete video profile.
    #[must_use]
    pub const fn from_video_profile(profile: &FfmpegVideoProfile) -> Self {
        match profile {
            FfmpegVideoProfile::H264Sdr { .. } => Self::Sdr,
            FfmpegVideoProfile::HevcMain10Hdr { .. } => Self::Hdr,
        }
    }
}

/// Inputs required to render a complete FFmpeg argv.
#[derive(Clone, Debug)]
pub struct FfmpegCliRequest<'a> {
    /// Source URL visible to FFmpeg (typically the loopback proxy).
    pub source_url: &'a str,
    /// Directory where FFmpeg writes the rolling HLS output.
    pub output_dir: &'a Path,
    /// Seek position in seconds before the input.
    pub start_position_seconds: f64,
    /// Exact VAAPI render node proven by the runtime smoke, when selected.
    pub vaapi_device: Option<&'a Path>,
    /// Active pipeline candidate selected by the reducer.
    pub candidate: FfmpegCandidate,
    /// Complete plan produced by the planner.
    pub plan: &'a FfmpegPlan,
}

/// Produces the ordered argv for one FFmpeg sidecar invocation.
///
/// The returned vector includes `-nostdin`, `-hide_banner`, and `-loglevel
/// warning` preamble flags, optional `-ss` seek, input mapping, video and
/// audio encoder arguments, forced-keyframe alignment, and the rolling-HLS
/// muxer options derived from the plan.
pub fn ffmpeg_argv(request: &FfmpegCliRequest<'_>) -> Result<Vec<String>, FfmpegCliError> {
    if request.candidate.delivery == MediaDelivery::DirectSource {
        return Ok(Vec::new());
    }
    let dynamic_range = DynamicRange::from_video_profile(&request.plan.video);
    let mut argv = vec![
        "-nostdin".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "warning".to_string(),
        "-y".to_string(),
    ];
    if request.start_position_seconds > 0.0 {
        argv.extend([
            "-ss".to_string(),
            format!("{:.3}", request.start_position_seconds),
        ]);
    }
    argv.extend([
        "-re".to_string(),
        "-i".to_string(),
        request.source_url.to_string(),
        "-map".to_string(),
        format!("0:{}", request.plan.probe.video.stream_index),
        "-map_metadata".to_string(),
        "0".to_string(),
        "-map_metadata:s:v:0".to_string(),
        format!("0:s:{}", request.plan.probe.video.stream_index),
        "-sn".to_string(),
        "-dn".to_string(),
    ]);
    if let Some(audio) = request.plan.probe.audio {
        argv.extend(["-map".to_string(), format!("0:{}", audio.stream_index)]);
    }
    match request.candidate.video {
        StreamDecision::Copy => argv.extend(copy_video_argv(request.plan.probe.video.codec)),
        StreamDecision::Transcode => {
            argv.extend(video_argv(
                &request.candidate,
                dynamic_range,
                &request.plan.video,
                request.vaapi_device,
            )?);
            argv.extend(forced_keyframe_argv());
        }
    }
    argv.extend(audio_argv(request));
    argv.extend(hls_argv(request.plan.hls, request.output_dir));
    Ok(argv)
}

fn copy_video_argv(codec: ProbedVideoCodec) -> Vec<String> {
    let mut argv = vec!["-c:v".to_string(), "copy".to_string()];
    match codec {
        ProbedVideoCodec::H264 => argv.extend(["-tag:v".to_string(), "avc1".to_string()]),
        ProbedVideoCodec::Hevc => argv.extend(["-tag:v".to_string(), "hvc1".to_string()]),
        ProbedVideoCodec::Other => {}
    }
    argv
}

fn video_argv(
    candidate: &FfmpegCandidate,
    dynamic_range: DynamicRange,
    plan_video: &FfmpegVideoProfile,
    vaapi_device: Option<&Path>,
) -> Result<Vec<String>, FfmpegCliError> {
    let hdr = dynamic_range == DynamicRange::Hdr;
    Ok(match candidate.encoder {
        Some(FfmpegEncoder::VideoToolbox) => vec![
            "-c:v".into(),
            if hdr {
                "hevc_videotoolbox"
            } else {
                "h264_videotoolbox"
            }
            .into(),
            "-pix_fmt".into(),
            if hdr { "p010le" } else { "yuv420p" }.into(),
            "-q:v".into(),
            "65".into(),
            "-tag:v".into(),
            if hdr { "hvc1" } else { "avc1" }.into(),
        ],
        Some(FfmpegEncoder::Vaapi) => {
            let device = vaapi_device.ok_or(FfmpegCliError::MissingVerifiedVaapiDevice)?;
            vec![
                "-vaapi_device".into(),
                device.to_string_lossy().into_owned(),
                "-vf".into(),
                if hdr {
                    "format=p010,hwupload"
                } else {
                    "format=nv12,hwupload"
                }
                .into(),
                "-c:v".into(),
                if hdr { "hevc_vaapi" } else { "h264_vaapi" }.into(),
                "-qp".into(),
                "20".into(),
                "-tag:v".into(),
                if hdr { "hvc1" } else { "avc1" }.into(),
            ]
        }
        Some(FfmpegEncoder::QuickSync) => vec![
            "-c:v".into(),
            if hdr { "hevc_qsv" } else { "h264_qsv" }.into(),
            "-preset".into(),
            "veryfast".into(),
            "-global_quality".into(),
            "20".into(),
            "-pix_fmt".into(),
            if hdr { "p010le" } else { "nv12" }.into(),
            "-tag:v".into(),
            if hdr { "hvc1" } else { "avc1" }.into(),
        ],
        Some(FfmpegEncoder::Nvenc) => vec![
            "-c:v".into(),
            if hdr { "hevc_nvenc" } else { "h264_nvenc" }.into(),
            "-preset".into(),
            "p4".into(),
            "-cq".into(),
            "20".into(),
            "-pix_fmt".into(),
            if hdr { "p010le" } else { "yuv420p" }.into(),
            "-tag:v".into(),
            if hdr { "hvc1" } else { "avc1" }.into(),
        ],
        Some(FfmpegEncoder::Amf) => vec![
            "-c:v".into(),
            if hdr { "hevc_amf" } else { "h264_amf" }.into(),
            "-quality".into(),
            "balanced".into(),
            "-qp_i".into(),
            "20".into(),
            "-qp_p".into(),
            "20".into(),
            "-pix_fmt".into(),
            if hdr { "p010le" } else { "yuv420p" }.into(),
            "-tag:v".into(),
            if hdr { "hvc1" } else { "avc1" }.into(),
        ],
        Some(FfmpegEncoder::Software) => software_argv(plan_video, hdr),
        None => copy_video_argv(requested_codec_for_profile(plan_video)),
    })
}

const fn requested_codec_for_profile(profile: &FfmpegVideoProfile) -> ProbedVideoCodec {
    match profile {
        FfmpegVideoProfile::H264Sdr { .. } => ProbedVideoCodec::H264,
        FfmpegVideoProfile::HevcMain10Hdr { .. } => ProbedVideoCodec::Hevc,
    }
}

fn software_argv(plan_video: &FfmpegVideoProfile, _hdr: bool) -> Vec<String> {
    match plan_video {
        FfmpegVideoProfile::H264Sdr { software, .. } => vec![
            "-c:v".into(),
            software.encoder_name.into(),
            "-preset".into(),
            software.preset.into(),
            "-crf".into(),
            software.crf.to_string(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-tag:v".into(),
            "avc1".into(),
        ],
        FfmpegVideoProfile::HevcMain10Hdr { .. } => vec![
            "-c:v".into(),
            "libx265".into(),
            "-preset".into(),
            "veryfast".into(),
            "-crf".into(),
            "20".into(),
            "-pix_fmt".into(),
            "yuv420p10le".into(),
            "-tag:v".into(),
            "hvc1".into(),
        ],
    }
}

fn audio_argv(request: &FfmpegCliRequest<'_>) -> Vec<String> {
    match request.candidate.audio {
        None => vec!["-an".to_string()],
        Some(StreamDecision::Copy) => vec!["-c:a".to_string(), "copy".to_string()],
        Some(StreamDecision::Transcode) => {
            let bitrate = request
                .plan
                .audio
                .as_ref()
                .map_or(crate::AAC_STEREO_BITRATE_BPS, |audio| audio.bitrate_bps);
            vec![
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                format!("{}k", bitrate / 1000),
            ]
        }
    }
}

fn forced_keyframe_argv() -> Vec<String> {
    vec![
        "-force_key_frames".to_string(),
        format!(
            "expr:gte(t,n_forced*{})",
            crate::ROLLING_HLS_SEGMENT_DURATION_SECONDS
        ),
    ]
}

fn hls_argv(profile: RollingHlsProfile, output_dir: &Path) -> Vec<String> {
    vec![
        "-f".to_string(),
        "hls".to_string(),
        "-hls_time".to_string(),
        profile.segment_duration_seconds.to_string(),
        "-hls_list_size".to_string(),
        profile.window_segments.to_string(),
        "-hls_delete_threshold".to_string(),
        "2".to_string(),
        "-hls_flags".to_string(),
        "delete_segments+append_list+independent_segments".to_string(),
        "-hls_segment_type".to_string(),
        "fmp4".to_string(),
        "-hls_fmp4_init_filename".to_string(),
        "init.mp4".to_string(),
        "-hls_segment_filename".to_string(),
        output_dir
            .join("segment_%06d.m4s")
            .to_string_lossy()
            .into_owned(),
        output_dir
            .join("master.m3u8")
            .to_string_lossy()
            .into_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserPlaybackCapabilities, FfmpegEncoderAvailability, FfmpegPlanRequest, FfmpegPlatform,
        MediaProbeFacts, MediaProbeResult, ProbedAudioCodec, ProbedAudioStream, ProbedContainer,
        ProbedDynamicRange, ProbedPixelFormat, ProbedVideoCodec, ProbedVideoSampleEntry,
        ProbedVideoStream,
    };

    fn plan(video: ProbedVideoCodec, audio: ProbedAudioCodec) -> FfmpegPlan {
        crate::plan_ffmpeg(FfmpegPlanRequest {
            platform: FfmpegPlatform::Linux,
            encoders: FfmpegEncoderAvailability {
                vaapi: true,
                ..FfmpegEncoderAvailability::default()
            },
            browser: BrowserPlaybackCapabilities {
                fmp4_hls: true,
                h264_sdr: true,
                hevc_main10_hdr: false,
                aac: true,
                max_audio_channels: 8,
            },
            probe: MediaProbeResult::Facts(MediaProbeFacts {
                container: ProbedContainer::Other,
                video: ProbedVideoStream {
                    stream_index: 3,
                    codec: video,
                    pixel_format: ProbedPixelFormat::Yuv420p,
                    sample_entry: ProbedVideoSampleEntry::Other,
                    dynamic_range: ProbedDynamicRange::Sdr,
                    hevc_main10: false,
                },
                audio: Some(ProbedAudioStream {
                    stream_index: 7,
                    codec: audio,
                    channels: 6,
                }),
                video_stream_count: 1,
                audio_stream_count: 1,
            }),
        })
        .expect("SDR plan should succeed")
    }

    fn candidate(plan: &FfmpegPlan, encoder: Option<FfmpegEncoder>) -> FfmpegCandidate {
        plan.candidates
            .iter()
            .find(|c| c.encoder == encoder)
            .copied()
            .unwrap_or_else(|| *plan.candidates.last().expect("candidate"))
    }

    #[test]
    fn full_transcode_maps_selected_global_streams_and_emits_both_codecs() {
        let plan = plan(ProbedVideoCodec::Other, ProbedAudioCodec::Other);
        let argv = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 0.0,
            vaapi_device: None,
            candidate: candidate(&plan, Some(FfmpegEncoder::Software)),
            plan: &plan,
        })
        .expect("software FFmpeg command should be valid");

        assert!(
            argv.windows(2).any(|pair| pair == ["-map", "0:3"])
                && argv.windows(2).any(|pair| pair == ["-map", "0:7"])
                && argv
                    .windows(2)
                    .any(|pair| pair == ["-map_metadata:s:v:0", "0:s:3"])
                && argv.windows(2).any(|pair| pair == ["-c:v", "libx264"])
                && argv.windows(2).any(|pair| pair == ["-c:a", "aac"])
                && argv.iter().any(|argument| argument == "-force_key_frames")
        );
    }

    #[test]
    fn remux_copies_both_streams_without_forced_keyframes() {
        let plan = plan(ProbedVideoCodec::H264, ProbedAudioCodec::Aac);
        let argv = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 0.0,
            vaapi_device: None,
            candidate: candidate(&plan, None),
            plan: &plan,
        })
        .expect("remux FFmpeg command should be valid");

        assert!(
            argv.windows(2).any(|pair| pair == ["-c:v", "copy"])
                && argv.windows(2).any(|pair| pair == ["-c:a", "copy"])
                && !argv.iter().any(|argument| argument == "-force_key_frames")
        );
    }

    #[test]
    fn audio_only_partial_transcode_keeps_video_packets() {
        let plan = plan(ProbedVideoCodec::H264, ProbedAudioCodec::Other);
        let argv = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 0.0,
            vaapi_device: None,
            candidate: candidate(&plan, None),
            plan: &plan,
        })
        .expect("partial-transcode FFmpeg command should be valid");

        assert!(
            argv.windows(2).any(|pair| pair == ["-c:v", "copy"])
                && argv.windows(2).any(|pair| pair == ["-c:a", "aac"])
                && argv.windows(2).any(|pair| pair == ["-b:a", "384k"])
        );
    }

    #[test]
    fn vaapi_uses_the_exact_smoked_render_node() {
        let plan = plan(ProbedVideoCodec::Other, ProbedAudioCodec::Aac);
        let argv = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 0.0,
            vaapi_device: Some(Path::new("/dev/dri/renderD129")),
            candidate: candidate(&plan, Some(FfmpegEncoder::Vaapi)),
            plan: &plan,
        })
        .expect("smoked VAAPI device should be accepted");

        assert!(argv
            .windows(2)
            .any(|pair| pair == ["-vaapi_device", "/dev/dri/renderD129"]));
    }

    #[test]
    fn vaapi_without_a_smoked_render_node_fails_closed() {
        let plan = plan(ProbedVideoCodec::Other, ProbedAudioCodec::Aac);
        let error = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 0.0,
            vaapi_device: None,
            candidate: candidate(&plan, Some(FfmpegEncoder::Vaapi)),
            plan: &plan,
        })
        .expect_err("an unverified VAAPI device must never be guessed");

        assert_eq!(error, FfmpegCliError::MissingVerifiedVaapiDevice);
    }
}
