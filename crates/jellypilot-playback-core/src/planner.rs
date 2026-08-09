//! Deterministic FFmpeg profile and candidate planning.

use crate::{
    FfmpegAudioCodec, FfmpegAudioProfile, FfmpegCandidate, FfmpegEncoder, FfmpegPlan,
    FfmpegPlanError, FfmpegPlanRequest, FfmpegPlatform, FfmpegSoftwareH264Profile,
    FfmpegVideoEncoder, FfmpegVideoProfile, PreservedMediaProperties, RollingHlsProfile,
    SourceVideoProfile, AAC_MULTICHANNEL_BITRATE_BPS, AAC_STEREO_BITRATE_BPS, H264_SDR_CRF,
    H264_SDR_PRESET,
};

/// Produces a policy-compliant FFmpeg profile and ordered startup candidates.
///
/// Hardware candidates are filtered by explicit compatible availability and
/// ordered by platform. Software is always the final candidate. HDR requests
/// fail instead of silently tone mapping when the browser cannot decode HEVC
/// Main10 HDR.
///
/// # Errors
///
/// Returns [`FfmpegPlanError`] when browser capabilities cannot preserve the
/// requested media profile or when the source audio layout is invalid.
pub fn plan_ffmpeg(request: FfmpegPlanRequest) -> Result<FfmpegPlan, FfmpegPlanError> {
    validate_browser(request)?;

    let video = match request.video {
        SourceVideoProfile::H264Sdr => FfmpegVideoProfile::H264Sdr {
            pixel_format: crate::FfmpegPixelFormat::Yuv420p,
            software: FfmpegSoftwareH264Profile {
                encoder_name: "libx264",
                preset: H264_SDR_PRESET,
                crf: H264_SDR_CRF,
            },
        },
        SourceVideoProfile::HevcMain10Hdr => FfmpegVideoProfile::HevcMain10Hdr {
            pixel_format: crate::FfmpegPixelFormat::P010Le,
            preserve_hdr_metadata: true,
            tone_mapping: false,
        },
    };
    let audio = request.audio.map(|channel_layout| FfmpegAudioProfile {
        codec: FfmpegAudioCodec::Aac,
        channel_layout,
        bitrate_bps: if channel_layout.channel_count() <= 2 {
            AAC_STEREO_BITRATE_BPS
        } else {
            AAC_MULTICHANNEL_BITRATE_BPS
        },
    });

    Ok(FfmpegPlan {
        hls: RollingHlsProfile::rolling(),
        video,
        audio,
        preserved: PreservedMediaProperties {
            resolution: true,
            frame_rate: true,
            aspect_ratio: true,
            audio_channel_layout: true,
        },
        candidates: candidates(request),
    })
}

fn validate_browser(request: FfmpegPlanRequest) -> Result<(), FfmpegPlanError> {
    if !request.browser.fmp4_hls {
        return Err(FfmpegPlanError::FragmentedMp4HlsUnsupported);
    }
    match request.video {
        SourceVideoProfile::H264Sdr if !request.browser.h264_sdr => {
            return Err(FfmpegPlanError::H264SdrUnsupported);
        }
        SourceVideoProfile::HevcMain10Hdr if !request.browser.hevc_main10_hdr => {
            return Err(FfmpegPlanError::HevcMain10HdrUnsupported);
        }
        SourceVideoProfile::H264Sdr | SourceVideoProfile::HevcMain10Hdr => {}
    }

    let Some(audio) = request.audio else {
        return Ok(());
    };
    if !request.browser.aac {
        return Err(FfmpegPlanError::AacUnsupported);
    }
    let source_channels = audio.channel_count();
    if source_channels == 0 {
        return Err(FfmpegPlanError::InvalidAudioChannelCount);
    }
    if source_channels > request.browser.max_audio_channels {
        return Err(FfmpegPlanError::AudioChannelCountUnsupported {
            source_channels,
            browser_max_channels: request.browser.max_audio_channels,
        });
    }
    Ok(())
}

fn candidates(request: FfmpegPlanRequest) -> Vec<FfmpegCandidate> {
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
        .map(|encoder| FfmpegCandidate {
            encoder,
            video_encoder: video_encoder(request.video, encoder),
        })
        .collect()
}

const fn video_encoder(source: SourceVideoProfile, encoder: FfmpegEncoder) -> FfmpegVideoEncoder {
    match (source, encoder) {
        (SourceVideoProfile::H264Sdr, FfmpegEncoder::VideoToolbox) => {
            FfmpegVideoEncoder::H264VideoToolbox
        }
        (SourceVideoProfile::HevcMain10Hdr, FfmpegEncoder::VideoToolbox) => {
            FfmpegVideoEncoder::HevcVideoToolbox
        }
        (SourceVideoProfile::H264Sdr, FfmpegEncoder::QuickSync) => {
            FfmpegVideoEncoder::H264QuickSync
        }
        (SourceVideoProfile::HevcMain10Hdr, FfmpegEncoder::QuickSync) => {
            FfmpegVideoEncoder::HevcQuickSync
        }
        (SourceVideoProfile::H264Sdr, FfmpegEncoder::Nvenc) => FfmpegVideoEncoder::H264Nvenc,
        (SourceVideoProfile::HevcMain10Hdr, FfmpegEncoder::Nvenc) => FfmpegVideoEncoder::HevcNvenc,
        (SourceVideoProfile::H264Sdr, FfmpegEncoder::Amf) => FfmpegVideoEncoder::H264Amf,
        (SourceVideoProfile::HevcMain10Hdr, FfmpegEncoder::Amf) => FfmpegVideoEncoder::HevcAmf,
        (SourceVideoProfile::H264Sdr, FfmpegEncoder::Vaapi) => FfmpegVideoEncoder::H264Vaapi,
        (SourceVideoProfile::HevcMain10Hdr, FfmpegEncoder::Vaapi) => FfmpegVideoEncoder::HevcVaapi,
        (SourceVideoProfile::H264Sdr, FfmpegEncoder::Software) => FfmpegVideoEncoder::Libx264,
        (SourceVideoProfile::HevcMain10Hdr, FfmpegEncoder::Software) => FfmpegVideoEncoder::Libx265,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AudioChannelLayout, BrowserPlaybackCapabilities, FfmpegEncoderAvailability,
        FfmpegPixelFormat,
    };

    fn capabilities() -> BrowserPlaybackCapabilities {
        BrowserPlaybackCapabilities {
            fmp4_hls: true,
            h264_sdr: true,
            hevc_main10_hdr: true,
            aac: true,
            max_audio_channels: 8,
        }
    }

    fn request(platform: FfmpegPlatform) -> FfmpegPlanRequest {
        FfmpegPlanRequest {
            platform,
            encoders: FfmpegEncoderAvailability {
                videotoolbox: true,
                quick_sync: true,
                nvenc: true,
                amf: true,
                vaapi: true,
            },
            browser: capabilities(),
            video: SourceVideoProfile::H264Sdr,
            audio: Some(AudioChannelLayout::Stereo),
        }
    }

    #[test]
    fn windows_candidates_follow_locked_order_and_end_in_software() {
        let plan = plan_ffmpeg(request(FfmpegPlatform::Windows)).expect("plan should be supported");

        assert_eq!(
            plan.candidates
                .iter()
                .map(|candidate| candidate.encoder)
                .collect::<Vec<_>>(),
            vec![
                FfmpegEncoder::QuickSync,
                FfmpegEncoder::Nvenc,
                FfmpegEncoder::Amf,
                FfmpegEncoder::Software,
            ]
        );
    }

    #[test]
    fn linux_candidates_filter_unavailable_encoders_without_reordering() {
        let mut input = request(FfmpegPlatform::Linux);
        input.encoders.quick_sync = false;

        let plan = plan_ffmpeg(input).expect("plan should be supported");

        assert_eq!(
            plan.candidates
                .iter()
                .map(|candidate| candidate.encoder)
                .collect::<Vec<_>>(),
            vec![
                FfmpegEncoder::Vaapi,
                FfmpegEncoder::Nvenc,
                FfmpegEncoder::Software,
            ]
        );
    }

    #[test]
    fn h264_sdr_profile_locks_yuv420p_libx264_veryfast_crf20() {
        let plan = plan_ffmpeg(request(FfmpegPlatform::MacOs)).expect("plan should be supported");

        assert_eq!(
            plan.video,
            FfmpegVideoProfile::H264Sdr {
                pixel_format: FfmpegPixelFormat::Yuv420p,
                software: FfmpegSoftwareH264Profile {
                    encoder_name: "libx264",
                    preset: "veryfast",
                    crf: 20,
                },
            }
        );
    }

    #[test]
    fn hevc_hdr_without_browser_support_returns_explicit_error() {
        let mut input = request(FfmpegPlatform::Linux);
        input.video = SourceVideoProfile::HevcMain10Hdr;
        input.browser.hevc_main10_hdr = false;

        let error = plan_ffmpeg(input).expect_err("HDR must not be tone mapped implicitly");

        assert_eq!(error, FfmpegPlanError::HevcMain10HdrUnsupported);
    }

    #[test]
    fn multichannel_aac_uses_384_kbps_and_retains_layout() {
        let mut input = request(FfmpegPlatform::Linux);
        input.audio = Some(AudioChannelLayout::Surround51);

        let plan = plan_ffmpeg(input).expect("multichannel audio should be supported");

        assert_eq!(
            plan.audio,
            Some(FfmpegAudioProfile {
                codec: FfmpegAudioCodec::Aac,
                channel_layout: AudioChannelLayout::Surround51,
                bitrate_bps: 384_000,
            })
        );
    }
}
