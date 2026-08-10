//! Deterministic direct-source and FFmpeg-HLS delivery planning.

use crate::{
    AudioChannelLayout, FfmpegAudioCodec, FfmpegAudioProfile, FfmpegCandidate, FfmpegEncoder,
    FfmpegPlan, FfmpegPlanError, FfmpegPlanRequest, FfmpegPlatform, FfmpegSoftwareH264Profile,
    FfmpegVideoEncoder, FfmpegVideoProfile, MediaDelivery, MediaProbeFacts, MediaProbeResult,
    PreservedMediaProperties, ProbedAudioCodec, ProbedContainer, ProbedDynamicRange,
    ProbedPixelFormat, ProbedVideoCodec, ProbedVideoSampleEntry, RollingHlsProfile, StreamDecision,
    AAC_MULTICHANNEL_BITRATE_BPS, AAC_STEREO_BITRATE_BPS, H264_SDR_CRF, H264_SDR_PRESET,
};

/// Produces a policy-compliant ordered delivery plan from normalized probe facts.
///
/// Strict MP4 direct source is preferred, followed by HLS remux, partial
/// transcode, or full transcode as required. Video and audio decisions are
/// independent. HDR is accepted only as browser-decodable HEVC Main 10 and is
/// never tone mapped.
///
/// # Errors
///
/// Returns [`FfmpegPlanError`] when probing failed, browser capabilities cannot
/// consume a safe output, or HDR cannot be preserved without tone mapping.
pub fn plan_ffmpeg(request: FfmpegPlanRequest) -> Result<FfmpegPlan, FfmpegPlanError> {
    let probe = match request.probe {
        MediaProbeResult::Facts(probe) => probe,
        MediaProbeResult::Failed(failure) => return Err(FfmpegPlanError::ProbeFailed(failure)),
    };
    validate_probe(request, probe)?;

    let video_copy = video_is_browser_compatible(request, probe);
    let audio_copy = audio_is_browser_compatible(request, probe);
    let video = target_video_profile(probe);
    let audio = probe.audio.map(|stream| {
        let channel_layout = AudioChannelLayout::from_channel_count(stream.channels);
        FfmpegAudioProfile {
            codec: FfmpegAudioCodec::Aac,
            channel_layout,
            bitrate_bps: if stream.channels <= 2 {
                AAC_STEREO_BITRATE_BPS
            } else {
                AAC_MULTICHANNEL_BITRATE_BPS
            },
        }
    });
    let candidates = candidates(request, probe, video_copy, audio_copy);
    if candidates.is_empty() {
        return Err(FfmpegPlanError::NoCompatibleDelivery);
    }

    Ok(FfmpegPlan {
        probe,
        hls: RollingHlsProfile::rolling(),
        video,
        audio,
        preserved: PreservedMediaProperties {
            resolution: true,
            frame_rate: true,
            aspect_ratio: true,
            audio_channel_layout: true,
        },
        candidates,
    })
}

fn validate_probe(
    request: FfmpegPlanRequest,
    probe: MediaProbeFacts,
) -> Result<(), FfmpegPlanError> {
    if let Some(audio) = probe.audio {
        if audio.channels == 0 {
            return Err(FfmpegPlanError::InvalidAudioChannelCount);
        }
        if !request.browser.aac {
            return Err(FfmpegPlanError::AacUnsupported);
        }
        if audio.channels > request.browser.max_audio_channels {
            return Err(FfmpegPlanError::AudioChannelCountUnsupported {
                source_channels: audio.channels,
                browser_max_channels: request.browser.max_audio_channels,
            });
        }
    }

    match probe.video.dynamic_range {
        ProbedDynamicRange::Sdr if !request.browser.h264_sdr => {
            Err(FfmpegPlanError::H264SdrUnsupported)
        }
        ProbedDynamicRange::Hdr
            if probe.video.codec != ProbedVideoCodec::Hevc
                || !probe.video.hevc_main10
                || probe.video.pixel_format != ProbedPixelFormat::TenBit420 =>
        {
            Err(FfmpegPlanError::UnsupportedHdrSource)
        }
        ProbedDynamicRange::Hdr if !request.browser.hevc_main10_hdr => {
            Err(FfmpegPlanError::HevcMain10HdrUnsupported)
        }
        ProbedDynamicRange::Sdr | ProbedDynamicRange::Hdr => Ok(()),
    }
}

const fn target_video_profile(probe: MediaProbeFacts) -> FfmpegVideoProfile {
    match probe.video.dynamic_range {
        ProbedDynamicRange::Sdr => FfmpegVideoProfile::H264Sdr {
            pixel_format: crate::FfmpegPixelFormat::Yuv420p,
            software: FfmpegSoftwareH264Profile {
                encoder_name: "libx264",
                preset: H264_SDR_PRESET,
                crf: H264_SDR_CRF,
            },
        },
        ProbedDynamicRange::Hdr => FfmpegVideoProfile::HevcMain10Hdr {
            pixel_format: crate::FfmpegPixelFormat::P010Le,
            preserve_hdr_metadata: true,
            tone_mapping: false,
        },
    }
}

const fn video_is_browser_compatible(request: FfmpegPlanRequest, probe: MediaProbeFacts) -> bool {
    match probe.video.dynamic_range {
        ProbedDynamicRange::Sdr => {
            request.browser.h264_sdr
                && matches!(probe.video.codec, ProbedVideoCodec::H264)
                && matches!(probe.video.pixel_format, ProbedPixelFormat::Yuv420p)
        }
        ProbedDynamicRange::Hdr => {
            request.browser.hevc_main10_hdr
                && matches!(probe.video.codec, ProbedVideoCodec::Hevc)
                && probe.video.hevc_main10
                && matches!(probe.video.pixel_format, ProbedPixelFormat::TenBit420)
        }
    }
}

const fn audio_is_browser_compatible(request: FfmpegPlanRequest, probe: MediaProbeFacts) -> bool {
    match probe.audio {
        None => true,
        Some(audio) => {
            request.browser.aac
                && matches!(audio.codec, ProbedAudioCodec::Aac)
                && audio.channels > 0
                && audio.channels <= request.browser.max_audio_channels
        }
    }
}

const fn strict_direct_source_eligible(
    probe: MediaProbeFacts,
    video_copy: bool,
    audio_copy: bool,
) -> bool {
    let sample_entry_compatible = match probe.video.codec {
        ProbedVideoCodec::H264 => matches!(probe.video.sample_entry, ProbedVideoSampleEntry::Avc1),
        ProbedVideoCodec::Hevc => matches!(probe.video.sample_entry, ProbedVideoSampleEntry::Hvc1),
        ProbedVideoCodec::Other => false,
    };
    matches!(probe.container, ProbedContainer::Mp4)
        && probe.video_stream_count == 1
        && probe.audio_stream_count <= 1
        && video_copy
        && audio_copy
        && sample_entry_compatible
}

fn candidates(
    request: FfmpegPlanRequest,
    probe: MediaProbeFacts,
    video_copy: bool,
    audio_copy: bool,
) -> Vec<FfmpegCandidate> {
    let mut candidates = Vec::new();
    let audio_decision = probe.audio.map(|_| {
        if audio_copy {
            StreamDecision::Copy
        } else {
            StreamDecision::Transcode
        }
    });

    if strict_direct_source_eligible(probe, video_copy, audio_copy) {
        candidates.push(FfmpegCandidate {
            delivery: MediaDelivery::DirectSource,
            video: StreamDecision::Copy,
            audio: audio_decision,
            encoder: None,
            video_encoder: None,
        });
    }

    if !request.browser.fmp4_hls {
        return candidates;
    }

    let video_decision = if video_copy {
        StreamDecision::Copy
    } else {
        StreamDecision::Transcode
    };
    let delivery = match (video_decision, audio_decision) {
        (StreamDecision::Copy, None | Some(StreamDecision::Copy)) => MediaDelivery::HlsRemux,
        (StreamDecision::Transcode, None | Some(StreamDecision::Transcode)) => {
            MediaDelivery::HlsFullTranscode
        }
        (StreamDecision::Copy, Some(StreamDecision::Transcode))
        | (StreamDecision::Transcode, Some(StreamDecision::Copy)) => {
            MediaDelivery::HlsPartialTranscode
        }
    };

    if video_decision == StreamDecision::Copy {
        candidates.push(copy_video_candidate(delivery, audio_decision));
    } else {
        push_video_transcode_candidates(&mut candidates, request, probe, delivery, audio_decision);
    }

    if probe.video.dynamic_range == ProbedDynamicRange::Hdr {
        return candidates;
    }

    match (video_decision, audio_decision) {
        (StreamDecision::Copy, None) => push_video_transcode_candidates(
            &mut candidates,
            request,
            probe,
            MediaDelivery::HlsFullTranscode,
            None,
        ),
        (StreamDecision::Copy, Some(StreamDecision::Copy)) => {
            push_video_transcode_candidates(
                &mut candidates,
                request,
                probe,
                MediaDelivery::HlsPartialTranscode,
                Some(StreamDecision::Copy),
            );
            push_video_transcode_candidates(
                &mut candidates,
                request,
                probe,
                MediaDelivery::HlsFullTranscode,
                Some(StreamDecision::Transcode),
            );
        }
        (StreamDecision::Copy, Some(StreamDecision::Transcode))
        | (StreamDecision::Transcode, Some(StreamDecision::Copy)) => {
            push_video_transcode_candidates(
                &mut candidates,
                request,
                probe,
                MediaDelivery::HlsFullTranscode,
                Some(StreamDecision::Transcode),
            );
        }
        (StreamDecision::Transcode, None | Some(StreamDecision::Transcode)) => {}
    }
    candidates
}

const fn copy_video_candidate(
    delivery: MediaDelivery,
    audio: Option<StreamDecision>,
) -> FfmpegCandidate {
    FfmpegCandidate {
        delivery,
        video: StreamDecision::Copy,
        audio,
        encoder: None,
        video_encoder: None,
    }
}

fn push_video_transcode_candidates(
    candidates: &mut Vec<FfmpegCandidate>,
    request: FfmpegPlanRequest,
    probe: MediaProbeFacts,
    delivery: MediaDelivery,
    audio: Option<StreamDecision>,
) {
    candidates.extend(
        ordered_video_encoders(request).map(|encoder| FfmpegCandidate {
            delivery,
            video: StreamDecision::Transcode,
            audio,
            encoder: Some(encoder),
            video_encoder: Some(video_encoder(probe.video.dynamic_range, encoder)),
        }),
    );
}

fn ordered_video_encoders(request: FfmpegPlanRequest) -> impl Iterator<Item = FfmpegEncoder> {
    let available = request.encoders;
    let ordered = match request.platform {
        FfmpegPlatform::MacOs => [
            (available.videotoolbox, Some(FfmpegEncoder::VideoToolbox)),
            (false, None),
            (false, None),
            (false, None),
            (false, None),
        ],
        FfmpegPlatform::Windows => [
            (available.quick_sync, Some(FfmpegEncoder::QuickSync)),
            (available.nvenc, Some(FfmpegEncoder::Nvenc)),
            (available.amf, Some(FfmpegEncoder::Amf)),
            (false, None),
            (false, None),
        ],
        FfmpegPlatform::Linux => [
            (available.vaapi, Some(FfmpegEncoder::Vaapi)),
            (available.quick_sync, Some(FfmpegEncoder::QuickSync)),
            (available.nvenc, Some(FfmpegEncoder::Nvenc)),
            (false, None),
            (false, None),
        ],
    };

    ordered
        .into_iter()
        .filter_map(|(enabled, encoder)| enabled.then_some(encoder).flatten())
        .chain(std::iter::once(FfmpegEncoder::Software))
}

const fn video_encoder(
    dynamic_range: ProbedDynamicRange,
    encoder: FfmpegEncoder,
) -> FfmpegVideoEncoder {
    match (dynamic_range, encoder) {
        (ProbedDynamicRange::Sdr, FfmpegEncoder::VideoToolbox) => {
            FfmpegVideoEncoder::H264VideoToolbox
        }
        (ProbedDynamicRange::Hdr, FfmpegEncoder::VideoToolbox) => {
            FfmpegVideoEncoder::HevcVideoToolbox
        }
        (ProbedDynamicRange::Sdr, FfmpegEncoder::QuickSync) => FfmpegVideoEncoder::H264QuickSync,
        (ProbedDynamicRange::Hdr, FfmpegEncoder::QuickSync) => FfmpegVideoEncoder::HevcQuickSync,
        (ProbedDynamicRange::Sdr, FfmpegEncoder::Nvenc) => FfmpegVideoEncoder::H264Nvenc,
        (ProbedDynamicRange::Hdr, FfmpegEncoder::Nvenc) => FfmpegVideoEncoder::HevcNvenc,
        (ProbedDynamicRange::Sdr, FfmpegEncoder::Amf) => FfmpegVideoEncoder::H264Amf,
        (ProbedDynamicRange::Hdr, FfmpegEncoder::Amf) => FfmpegVideoEncoder::HevcAmf,
        (ProbedDynamicRange::Sdr, FfmpegEncoder::Vaapi) => FfmpegVideoEncoder::H264Vaapi,
        (ProbedDynamicRange::Hdr, FfmpegEncoder::Vaapi) => FfmpegVideoEncoder::HevcVaapi,
        (ProbedDynamicRange::Sdr, FfmpegEncoder::Software) => FfmpegVideoEncoder::Libx264,
        (ProbedDynamicRange::Hdr, FfmpegEncoder::Software) => FfmpegVideoEncoder::Libx265,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserPlaybackCapabilities, FfmpegEncoderAvailability, MediaProbeResult,
        ProbedAudioStream, ProbedVideoStream,
    };

    fn probe() -> MediaProbeFacts {
        MediaProbeFacts {
            container: ProbedContainer::Mp4,
            video: ProbedVideoStream {
                stream_index: 0,
                codec: ProbedVideoCodec::H264,
                pixel_format: ProbedPixelFormat::Yuv420p,
                sample_entry: ProbedVideoSampleEntry::Avc1,
                dynamic_range: ProbedDynamicRange::Sdr,
                hevc_main10: false,
            },
            audio: Some(ProbedAudioStream {
                stream_index: 1,
                codec: ProbedAudioCodec::Aac,
                channels: 2,
            }),
            video_stream_count: 1,
            audio_stream_count: 1,
        }
    }

    fn request(platform: FfmpegPlatform, probe: MediaProbeFacts) -> FfmpegPlanRequest {
        FfmpegPlanRequest {
            platform,
            encoders: FfmpegEncoderAvailability {
                videotoolbox: true,
                quick_sync: true,
                nvenc: true,
                amf: true,
                vaapi: true,
            },
            browser: BrowserPlaybackCapabilities {
                fmp4_hls: true,
                h264_sdr: true,
                hevc_main10_hdr: true,
                aac: true,
                max_audio_channels: 8,
            },
            probe: MediaProbeResult::Facts(probe),
        }
    }

    #[test]
    fn strict_mp4_is_direct_then_hls_remux() {
        let plan = plan_ffmpeg(request(FfmpegPlatform::Linux, probe()))
            .expect("strict MP4 should be supported");

        assert_eq!(
            (
                plan.candidates[0].delivery,
                plan.candidates[1].delivery,
                plan.candidates.last().map(|candidate| candidate.delivery),
            ),
            (
                MediaDelivery::DirectSource,
                MediaDelivery::HlsRemux,
                Some(MediaDelivery::HlsFullTranscode),
            )
        );
    }

    #[test]
    fn incompatible_audio_selects_partial_transcode_without_video_encoder() {
        let mut facts = probe();
        facts.container = ProbedContainer::Other;
        facts.audio.as_mut().expect("audio").codec = ProbedAudioCodec::Other;

        let plan = plan_ffmpeg(request(FfmpegPlatform::Linux, facts))
            .expect("audio transcode should be supported");

        assert_eq!(
            (plan.candidates[0], plan.candidates.last().copied()),
            (
                FfmpegCandidate {
                    delivery: MediaDelivery::HlsPartialTranscode,
                    video: StreamDecision::Copy,
                    audio: Some(StreamDecision::Transcode),
                    encoder: None,
                    video_encoder: None,
                },
                Some(FfmpegCandidate {
                    delivery: MediaDelivery::HlsFullTranscode,
                    video: StreamDecision::Transcode,
                    audio: Some(StreamDecision::Transcode),
                    encoder: Some(FfmpegEncoder::Software),
                    video_encoder: Some(FfmpegVideoEncoder::Libx264),
                }),
            )
        );
    }

    #[test]
    fn incompatible_video_uses_ordered_hardware_then_software_candidates() {
        let mut facts = probe();
        facts.container = ProbedContainer::Other;
        facts.video.codec = ProbedVideoCodec::Other;

        let plan =
            plan_ffmpeg(request(FfmpegPlatform::Linux, facts)).expect("SDR video should transcode");

        assert_eq!(
            plan.candidates
                .iter()
                .take(4)
                .map(|candidate| candidate.encoder)
                .collect::<Vec<_>>(),
            vec![
                Some(FfmpegEncoder::Vaapi),
                Some(FfmpegEncoder::QuickSync),
                Some(FfmpegEncoder::Nvenc),
                Some(FfmpegEncoder::Software),
            ]
        );
        assert_eq!(
            plan.candidates.last().map(|candidate| candidate.delivery),
            Some(MediaDelivery::HlsFullTranscode)
        );
    }

    #[test]
    fn hdr_that_is_not_exact_hevc_main10_fails_closed() {
        let mut facts = probe();
        facts.video.dynamic_range = ProbedDynamicRange::Hdr;
        facts.video.codec = ProbedVideoCodec::Other;
        facts.video.pixel_format = ProbedPixelFormat::TenBit420;

        let error = plan_ffmpeg(request(FfmpegPlatform::Linux, facts))
            .expect_err("HDR must never be tone mapped implicitly");

        assert_eq!(error, FfmpegPlanError::UnsupportedHdrSource);
    }

    #[test]
    fn direct_eligibility_rejects_multiple_audio_streams() {
        let mut facts = probe();
        facts.audio_stream_count = 2;

        let plan = plan_ffmpeg(request(FfmpegPlatform::Linux, facts))
            .expect("HLS remux should remain available");

        assert_eq!(plan.candidates[0].delivery, MediaDelivery::HlsRemux);
    }
}
