use jellypilot_playback_core::{
    AudioChannelLayout, BrowserObservation, BrowserPlaybackCapabilities, BrowserPlaybackState,
    EmbeddedPlaybackCore, FfmpegEncoder, FfmpegEncoderAvailability, FfmpegPlanRequest,
    FfmpegPlatform, MediaDelivery, MediaProbeFacts, MediaProbeResult, MpvFallbackReason,
    PlaybackAction, PlaybackCommand, PlaybackFailureStage, PlaybackGeneration,
    PlaybackObservationDisposition, PlaybackObservationToken, PlaybackPhase, PlaybackSession,
    PlaybackStartReason, PlaybackStopReason, ProbedAudioCodec, ProbedAudioStream, ProbedContainer,
    ProbedDynamicRange, ProbedPixelFormat, ProbedVideoCodec, ProbedVideoSampleEntry,
    ProbedVideoStream,
};

fn browser_capabilities() -> BrowserPlaybackCapabilities {
    BrowserPlaybackCapabilities {
        fmp4_hls: true,
        h264_sdr: true,
        hevc_main10_hdr: true,
        aac: true,
        max_audio_channels: 8,
    }
}

fn session(platform: FfmpegPlatform, encoders: FfmpegEncoderAvailability) -> PlaybackSession {
    PlaybackSession {
        item_id: "episode-1".to_owned(),
        media_source_id: Some("source-1".to_owned()),
        play_session_id: Some("play-1".to_owned()),
        start_position_ticks: 100,
        duration_ticks: Some(10_000),
        plan_request: FfmpegPlanRequest {
            platform,
            encoders,
            browser: browser_capabilities(),
            probe: MediaProbeResult::Facts(MediaProbeFacts {
                container: ProbedContainer::Other,
                video: ProbedVideoStream {
                    stream_index: 0,
                    codec: ProbedVideoCodec::Other,
                    pixel_format: ProbedPixelFormat::Yuv420p,
                    sample_entry: ProbedVideoSampleEntry::Other,
                    dynamic_range: ProbedDynamicRange::Sdr,
                    hevc_main10: false,
                },
                audio: Some(ProbedAudioStream {
                    stream_index: 1,
                    codec: ProbedAudioCodec::Aac,
                    channels: AudioChannelLayout::Stereo.channel_count(),
                }),
                video_stream_count: 1,
                audio_stream_count: 1,
            }),
        },
        mpv_fallback_available: true,
    }
}

fn play(core: &mut EmbeddedPlaybackCore, session: PlaybackSession) -> PlaybackGeneration {
    let update = core
        .dispatch(PlaybackAction::Play(session))
        .expect("play should dispatch");
    match update.commands.as_slice() {
        [PlaybackCommand::StartEmbedded { attempt }] => attempt.generation,
        commands => panic!("expected one start command, found {commands:?}"),
    }
}

fn observe(
    core: &mut EmbeddedPlaybackCore,
    generation: PlaybackGeneration,
    sequence: u64,
    state: BrowserPlaybackState,
    position_ticks: u64,
) -> jellypilot_playback_core::PlaybackUpdate {
    core.dispatch(PlaybackAction::BrowserObserved(BrowserObservation {
        token: PlaybackObservationToken {
            generation,
            sequence,
        },
        state,
        position_ticks,
    }))
    .expect("observation should dispatch")
}

fn next_generation(commands: &[PlaybackCommand]) -> PlaybackGeneration {
    commands
        .iter()
        .find_map(|command| match command {
            PlaybackCommand::StartEmbedded { attempt } => Some(attempt.generation),
            PlaybackCommand::SetPaused { .. }
            | PlaybackCommand::StopEmbedded { .. }
            | PlaybackCommand::ReportStarted { .. }
            | PlaybackCommand::ReportProgress { .. }
            | PlaybackCommand::ReportStopped { .. } => None,
        })
        .expect("commands should contain a start attempt")
}

#[test]
fn play_emits_self_contained_first_candidate_attempt() {
    let mut core = EmbeddedPlaybackCore::new();
    let input = session(
        FfmpegPlatform::MacOs,
        FfmpegEncoderAvailability {
            videotoolbox: true,
            ..FfmpegEncoderAvailability::default()
        },
    );

    let update = core
        .dispatch(PlaybackAction::Play(input))
        .expect("play should dispatch");

    let [PlaybackCommand::StartEmbedded { attempt }] = update.commands.as_slice() else {
        panic!("expected a single embedded start command");
    };
    assert_eq!(
        (
            update.snapshot.phase,
            attempt.generation,
            attempt.reason,
            attempt.start_position_ticks,
            attempt.candidate_index,
            attempt.candidate.encoder,
            attempt.plan.hls.segment_duration_seconds,
            attempt.plan.hls.window_segments,
        ),
        (
            PlaybackPhase::Starting,
            PlaybackGeneration(1),
            PlaybackStartReason::Play,
            100,
            0,
            Some(FfmpegEncoder::VideoToolbox),
            4,
            15,
        )
    );
}

#[test]
fn stale_browser_sequence_cannot_mutate_active_state() {
    let mut core = EmbeddedPlaybackCore::new();
    let generation = play(
        &mut core,
        session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default()),
    );
    observe(&mut core, generation, 2, BrowserPlaybackState::Playing, 400);
    let before = core.snapshot();

    let update = observe(&mut core, generation, 1, BrowserPlaybackState::Paused, 900);

    assert_eq!(
        (update.observation, update.commands, update.snapshot),
        (
            PlaybackObservationDisposition::IgnoredStaleSequence,
            Vec::new(),
            before,
        )
    );
}

#[test]
fn old_generation_observation_is_rejected_after_seek() {
    let mut core = EmbeddedPlaybackCore::new();
    let old_generation = play(
        &mut core,
        session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default()),
    );
    observe(
        &mut core,
        old_generation,
        1,
        BrowserPlaybackState::Playing,
        400,
    );
    let seek = core
        .dispatch(PlaybackAction::Seek {
            position_ticks: 2_000,
        })
        .expect("seek should dispatch");
    let new_generation = next_generation(&seek.commands);
    let before = core.snapshot();

    let update = observe(
        &mut core,
        old_generation,
        2,
        BrowserPlaybackState::Ended,
        10_000,
    );

    assert_eq!(
        (
            new_generation,
            update.observation,
            update.commands,
            update.snapshot,
        ),
        (
            PlaybackGeneration(2),
            PlaybackObservationDisposition::IgnoredStaleGeneration,
            Vec::new(),
            before,
        )
    );
}

#[test]
fn reporting_starts_once_survives_seek_and_stops_once() {
    let mut core = EmbeddedPlaybackCore::new();
    let generation = play(
        &mut core,
        session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default()),
    );

    let started = observe(&mut core, generation, 1, BrowserPlaybackState::Playing, 200);
    let progress = observe(&mut core, generation, 2, BrowserPlaybackState::Playing, 300);
    let seek = core
        .dispatch(PlaybackAction::Seek {
            position_ticks: 2_000,
        })
        .expect("seek should dispatch");
    let replacement = next_generation(&seek.commands);
    let replacement_progress = observe(
        &mut core,
        replacement,
        1,
        BrowserPlaybackState::Playing,
        2_050,
    );
    let stopped = core
        .dispatch(PlaybackAction::Stop)
        .expect("stop should dispatch");
    let acknowledged = observe(
        &mut core,
        replacement,
        2,
        BrowserPlaybackState::Stopped,
        2_050,
    );

    assert!(matches!(
        (
            started.commands.as_slice(),
            progress.commands.as_slice(),
            replacement_progress.commands.as_slice(),
            stopped.commands.as_slice(),
            acknowledged.commands.as_slice(),
            acknowledged.snapshot.phase,
        ),
        (
            [PlaybackCommand::ReportStarted { .. }],
            [PlaybackCommand::ReportProgress { .. }],
            [PlaybackCommand::ReportProgress { .. }],
            [
                PlaybackCommand::StopEmbedded { .. },
                PlaybackCommand::ReportStopped {
                    reason: PlaybackStopReason::User,
                    ..
                }
            ],
            [],
            PlaybackPhase::Stopped,
        )
    ));
}

#[test]
fn pause_resume_and_restart_emit_transport_commands() {
    let mut core = EmbeddedPlaybackCore::new();
    let generation = play(
        &mut core,
        session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default()),
    );
    observe(&mut core, generation, 1, BrowserPlaybackState::Playing, 700);

    let paused = core
        .dispatch(PlaybackAction::Pause)
        .expect("playing media should pause");
    let resumed = core
        .dispatch(PlaybackAction::Resume)
        .expect("paused media should resume");
    observe(&mut core, generation, 2, BrowserPlaybackState::Playing, 800);
    let restarted = core
        .dispatch(PlaybackAction::Restart)
        .expect("active media should restart");

    assert!(matches!(
        (
            paused.commands.as_slice(),
            paused.snapshot.phase,
            resumed.commands.as_slice(),
            resumed.snapshot.phase,
            restarted.commands.as_slice(),
            restarted.snapshot.phase,
        ),
        (
            [PlaybackCommand::SetPaused {
                generation: paused_generation,
                paused: true,
            }],
            PlaybackPhase::Paused,
            [PlaybackCommand::SetPaused {
                generation: resumed_generation,
                paused: false,
            }],
            PlaybackPhase::Buffering,
            [
                PlaybackCommand::StopEmbedded {
                    generation: retired_generation,
                },
                PlaybackCommand::StartEmbedded { attempt },
            ],
            PlaybackPhase::Restarting,
        ) if *paused_generation == generation
            && *resumed_generation == generation
            && *retired_generation == generation
            && attempt.generation == PlaybackGeneration(2)
            && attempt.reason == PlaybackStartReason::Restart
            && attempt.start_position_ticks == 800
    ));
}

#[test]
fn startup_failures_advance_candidates_then_expose_mpv_fallback() {
    let mut core = EmbeddedPlaybackCore::new();
    let first = play(
        &mut core,
        session(
            FfmpegPlatform::Windows,
            FfmpegEncoderAvailability {
                quick_sync: true,
                nvenc: true,
                ..FfmpegEncoderAvailability::default()
            },
        ),
    );

    let candidate_count = core
        .snapshot()
        .active_plan
        .as_ref()
        .map(|plan| plan.candidates.len())
        .expect("active plan");
    let mut generation = first;
    let mut terminal = None;
    for index in 0..candidate_count {
        let update = core
            .dispatch(PlaybackAction::StartupFailed {
                generation,
                message: format!("candidate {index} failed"),
                retryable: true,
            })
            .expect("candidate failure should dispatch");
        if index + 1 == candidate_count {
            terminal = Some(update);
        } else {
            generation = next_generation(&update.commands);
        }
    }
    let failed = terminal.expect("last candidate should fail terminally");

    assert_eq!(
        (
            failed.snapshot.phase,
            failed.snapshot.attempt_failures.len(),
            failed
                .snapshot
                .mpv_fallback
                .as_ref()
                .map(|fallback| (fallback.available, fallback.reason)),
        ),
        (
            PlaybackPhase::Failed,
            candidate_count,
            Some((true, MpvFallbackReason::FfmpegCandidatesExhausted)),
        )
    );
}

#[test]
fn direct_source_failure_before_playing_advances_to_hls_in_new_generation() {
    let mut core = EmbeddedPlaybackCore::new();
    let mut input = session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default());
    let MediaProbeResult::Facts(facts) = &mut input.plan_request.probe else {
        panic!("test session must contain probe facts");
    };
    facts.container = ProbedContainer::Mp4;
    facts.video.codec = ProbedVideoCodec::H264;
    facts.video.sample_entry = ProbedVideoSampleEntry::Avc1;
    let direct_generation = play(&mut core, input);

    let fallback = observe(
        &mut core,
        direct_generation,
        1,
        BrowserPlaybackState::Failed {
            message: "native MP4 decode failed".to_owned(),
        },
        0,
    );

    assert!(matches!(
        fallback.commands.as_slice(),
        [
            PlaybackCommand::StopEmbedded { generation },
            PlaybackCommand::StartEmbedded { attempt },
        ] if *generation == direct_generation
            && attempt.generation == PlaybackGeneration(2)
            && attempt.candidate.delivery == MediaDelivery::HlsRemux
            && attempt.start_position_ticks == 100
            && fallback.snapshot.phase == PlaybackPhase::Starting
    ));
}

#[test]
fn terminal_startup_failure_does_not_advance_delivery_candidates() {
    let mut core = EmbeddedPlaybackCore::new();
    let generation = play(
        &mut core,
        session(
            FfmpegPlatform::Windows,
            FfmpegEncoderAvailability {
                quick_sync: true,
                nvenc: true,
                ..FfmpegEncoderAvailability::default()
            },
        ),
    );

    let failed = core
        .dispatch(PlaybackAction::StartupFailed {
            generation,
            message: "source authorization failed".to_owned(),
            retryable: false,
        })
        .expect("terminal failure should become visible");

    assert!(matches!(
        (
            failed.commands.as_slice(),
            failed.snapshot.phase,
            failed.snapshot.failure.as_ref(),
        ),
        (
            [PlaybackCommand::StopEmbedded { generation: stopped }],
            PlaybackPhase::Failed,
            Some(failure),
        ) if *stopped == generation && !failure.retryable
    ));
}

#[test]
fn runtime_failure_does_not_advance_ffmpeg_candidate() {
    let mut core = EmbeddedPlaybackCore::new();
    let generation = play(
        &mut core,
        session(
            FfmpegPlatform::Windows,
            FfmpegEncoderAvailability {
                quick_sync: true,
                nvenc: true,
                ..FfmpegEncoderAvailability::default()
            },
        ),
    );
    observe(&mut core, generation, 1, BrowserPlaybackState::Playing, 500);

    let failed = observe(
        &mut core,
        generation,
        2,
        BrowserPlaybackState::Failed {
            message: "media element decode error".to_owned(),
        },
        600,
    );

    assert_eq!(
        (
            failed.snapshot.phase,
            failed.snapshot.generation,
            failed.snapshot.attempt_failures.len(),
            failed
                .snapshot
                .failure
                .as_ref()
                .map(|failure| failure.stage),
            failed
                .snapshot
                .mpv_fallback
                .as_ref()
                .map(|fallback| fallback.reason),
        ),
        (
            PlaybackPhase::Failed,
            Some(generation),
            0,
            Some(PlaybackFailureStage::Runtime),
            Some(MpvFallbackReason::RuntimeFailure),
        )
    );
}

#[test]
fn browser_stall_is_visible_and_offers_resume_position_to_mpv() {
    let mut core = EmbeddedPlaybackCore::new();
    let generation = play(
        &mut core,
        session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default()),
    );
    observe(
        &mut core,
        generation,
        1,
        BrowserPlaybackState::Playing,
        1_000,
    );

    let stalled = observe(
        &mut core,
        generation,
        2,
        BrowserPlaybackState::Stalled {
            message: "no progress within threshold".to_owned(),
        },
        1_200,
    );

    assert_eq!(
        (
            stalled.snapshot.phase,
            stalled
                .snapshot
                .failure
                .as_ref()
                .map(|failure| failure.stage),
            stalled
                .snapshot
                .mpv_fallback
                .as_ref()
                .map(|fallback| (fallback.reason, fallback.resume_position_ticks)),
        ),
        (
            PlaybackPhase::Failed,
            Some(PlaybackFailureStage::Stall),
            Some((MpvFallbackReason::BrowserStall, 1_200)),
        )
    );
}

#[test]
fn unsupported_hdr_becomes_explicit_planning_failure_without_start_command() {
    let mut core = EmbeddedPlaybackCore::new();
    let mut input = session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default());
    let MediaProbeResult::Facts(facts) = &mut input.plan_request.probe else {
        panic!("test session must contain probe facts");
    };
    facts.video.codec = ProbedVideoCodec::Hevc;
    facts.video.pixel_format = ProbedPixelFormat::TenBit420;
    facts.video.sample_entry = ProbedVideoSampleEntry::Hvc1;
    facts.video.dynamic_range = ProbedDynamicRange::Hdr;
    facts.video.hevc_main10 = true;
    input.plan_request.browser.hevc_main10_hdr = false;

    let update = core
        .dispatch(PlaybackAction::Play(input))
        .expect("unsupported media is visible state, not reducer misuse");

    assert_eq!(
        (
            update.snapshot.phase,
            update.commands,
            update
                .snapshot
                .failure
                .as_ref()
                .map(|failure| failure.stage),
            update
                .snapshot
                .mpv_fallback
                .as_ref()
                .map(|fallback| fallback.reason),
        ),
        (
            PlaybackPhase::Failed,
            Vec::new(),
            Some(PlaybackFailureStage::Planning),
            Some(MpvFallbackReason::UnsupportedBrowserCapabilities),
        )
    );
}

#[test]
fn replay_starts_new_reporting_lifecycle_from_zero() {
    let mut core = EmbeddedPlaybackCore::new();
    let generation = play(
        &mut core,
        session(FfmpegPlatform::Linux, FfmpegEncoderAvailability::default()),
    );
    observe(
        &mut core,
        generation,
        1,
        BrowserPlaybackState::Playing,
        9_000,
    );
    observe(
        &mut core,
        generation,
        2,
        BrowserPlaybackState::Ended,
        10_000,
    );

    let replay = core
        .dispatch(PlaybackAction::Replay)
        .expect("ended media should replay");
    let replay_generation = next_generation(&replay.commands);
    let replay_started = observe(
        &mut core,
        replay_generation,
        1,
        BrowserPlaybackState::Playing,
        0,
    );

    assert!(matches!(
        (replay.commands.as_slice(), replay_started.commands.as_slice()),
        (
            [PlaybackCommand::StartEmbedded { attempt }],
            [PlaybackCommand::ReportStarted { .. }],
        ) if attempt.reason == PlaybackStartReason::Replay
            && attempt.start_position_ticks == 0
            && replay_generation == PlaybackGeneration(2)
    ));
}
