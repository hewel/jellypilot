//! Deterministic FFmpeg command-line argument generation.
//!
//! Converts an [`FfmpegPlan`] attempt into the exact argv a local FFmpeg
//! process should receive. The caller supplies the concrete source URL,
//! output directory, and runtime selections; the core produces a
//! reproducible argument vector that honours the plan's codec, container,
//! and HLS packaging policy.

use std::path::Path;

use crate::{
    AudioChannelLayout, FfmpegCandidate, FfmpegEncoder, FfmpegPlan, FfmpegVideoProfile,
    RollingHlsProfile, AAC_MULTICHANNEL_BITRATE_BPS, AAC_STEREO_BITRATE_BPS,
};

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
    /// Optional zero-based audio stream index to map.
    pub audio_stream_index: Option<i32>,
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
#[must_use]
pub fn ffmpeg_argv(request: &FfmpegCliRequest<'_>) -> Vec<String> {
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
        "0:v:0".to_string(),
        "-map".to_string(),
        request
            .audio_stream_index
            .map_or_else(|| "0:a:0?".to_string(), |index| format!("0:{index}?")),
        "-map_metadata".to_string(),
        "0".to_string(),
        "-map_metadata:s:v:0".to_string(),
        "0:s:v:0".to_string(),
        "-sn".to_string(),
        "-dn".to_string(),
    ]);
    argv.extend(video_argv(
        &request.candidate,
        dynamic_range,
        &request.plan.video,
    ));
    argv.extend(audio_argv(
        request.plan.audio.as_ref().map(|p| p.channel_layout),
    ));
    argv.extend(forced_keyframe_argv());
    argv.extend(hls_argv(request.plan.hls, request.output_dir));
    argv
}

fn video_argv(
    candidate: &FfmpegCandidate,
    dynamic_range: DynamicRange,
    plan_video: &FfmpegVideoProfile,
) -> Vec<String> {
    let hdr = dynamic_range == DynamicRange::Hdr;
    match candidate.encoder {
        FfmpegEncoder::VideoToolbox => vec![
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
        FfmpegEncoder::Vaapi => vec![
            "-vaapi_device".into(),
            "/dev/dri/renderD128".into(),
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
        ],
        FfmpegEncoder::QuickSync => vec![
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
        FfmpegEncoder::Nvenc => vec![
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
        FfmpegEncoder::Amf => vec![
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
        FfmpegEncoder::Software => software_argv(plan_video, hdr),
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

fn audio_argv(layout: Option<AudioChannelLayout>) -> Vec<String> {
    let bitrate = match layout {
        Some(layout) if layout.channel_count() > 2 => AAC_MULTICHANNEL_BITRATE_BPS,
        _ => AAC_STEREO_BITRATE_BPS,
    };
    vec![
        "-c:a".to_string(),
        "aac".to_string(),
        "-b:a".to_string(),
        format!("{}k", bitrate / 1000),
    ]
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
        AudioChannelLayout, BrowserPlaybackCapabilities, FfmpegEncoder, FfmpegEncoderAvailability,
        FfmpegPlanRequest, FfmpegPlatform, SourceVideoProfile,
    };

    fn sdr_plan() -> FfmpegPlan {
        crate::plan_ffmpeg(FfmpegPlanRequest {
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
        })
        .expect("SDR plan should succeed")
    }

    fn candidate(plan: &FfmpegPlan, encoder: FfmpegEncoder) -> FfmpegCandidate {
        plan.candidates
            .iter()
            .find(|c| c.encoder == encoder)
            .copied()
            .unwrap_or_else(|| *plan.candidates.last().unwrap())
    }

    #[test]
    fn software_sdr_argv_uses_locked_hls_and_codec_policy() {
        let plan = sdr_plan();
        let argv = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 0.0,
            audio_stream_index: None,
            candidate: candidate(&plan, FfmpegEncoder::Software),
            plan: &plan,
        });

        assert!(argv.windows(2).any(|pair| pair == ["-c:v", "libx264"]));
        assert!(argv.windows(2).any(|pair| pair == ["-hls_time", "4"]));
        assert!(argv.windows(2).any(|pair| pair == ["-hls_list_size", "15"]));
        assert!(argv.windows(2).any(|pair| pair == ["-b:a", "192k"]));
        assert!(argv.iter().any(|argument| argument == "-re"));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["-map_metadata:s:v:0", "0:s:v:0"]));
    }

    #[test]
    fn seek_position_and_audio_stream_are_emitted() {
        let plan = sdr_plan();
        let argv = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 90.0,
            audio_stream_index: Some(2),
            candidate: candidate(&plan, FfmpegEncoder::Software),
            plan: &plan,
        });

        assert!(argv.windows(2).any(|pair| pair == ["-ss", "90.000"]));
        assert!(argv.contains(&"0:2?".to_string()));
    }

    #[test]
    fn multichannel_audio_uses_384k_bitrate() {
        let plan = crate::plan_ffmpeg(FfmpegPlanRequest {
            platform: FfmpegPlatform::Linux,
            encoders: FfmpegEncoderAvailability::default(),
            browser: BrowserPlaybackCapabilities {
                fmp4_hls: true,
                h264_sdr: true,
                hevc_main10_hdr: false,
                aac: true,
                max_audio_channels: 8,
            },
            video: SourceVideoProfile::H264Sdr,
            audio: Some(AudioChannelLayout::Surround51),
        })
        .expect("multichannel plan should succeed");

        let argv = ffmpeg_argv(&FfmpegCliRequest {
            source_url: "http://127.0.0.1:3210/source/nonce",
            output_dir: Path::new("/tmp/session"),
            start_position_seconds: 0.0,
            audio_stream_index: None,
            candidate: candidate(&plan, FfmpegEncoder::Software),
            plan: &plan,
        });

        assert!(argv.windows(2).any(|pair| pair == ["-b:a", "384k"]));
    }
}
