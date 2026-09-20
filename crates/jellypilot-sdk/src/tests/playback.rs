use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

use super::{controlled_http_server, test_sdk, test_session, HttpRequest};
use crate::playback::{
    PlaybackObservation, PlaybackSelection, PlaybackSession, PlaybackStartPosition,
};
use crate::{Sdk, SdkError};

const ITEM: &str = "00000000000000000000000000000001";
const DETAIL: &str = r#"{"Id":"00000000000000000000000000000001","Name":"Film","Type":"Movie","RunTimeTicks":6000000000,"OriginalLanguage":"en","UserData":{"Key":"item","PlaybackPositionTicks":200000000,"Played":false}}"#;
const MEDIA: &str = r#"{"PlaySessionId":"server-session","MediaSources":[{"Id":"source","Protocol":"Http","Container":"mkv","SupportsTranscoding":true,"TranscodingUrl":"/must-not-transcode.m3u8","MediaStreams":[{"Type":"Audio","Index":2,"Language":"eng"},{"Type":"Subtitle","Index":5,"Language":"eng","IsExternal":true,"Codec":"ass"}]}]}"#;

type Requests = UnboundedReceiver<HttpRequest<&'static str>>;

async fn request(requests: &mut Requests) -> HttpRequest<&'static str> {
    tokio::time::timeout(Duration::from_secs(10), requests.recv())
        .await
        .expect("request arrives")
        .expect("server live")
}

async fn prepared(sdk: &Sdk, requests: &mut Requests) -> Arc<PlaybackSession> {
    prepared_item(sdk, requests, DETAIL, false).await
}

async fn prepared_item(
    sdk: &Sdk,
    requests: &mut Requests,
    detail_json: &'static str,
    episode: bool,
) -> Arc<PlaybackSession> {
    sdk.set_prefer_original_audio(false)
        .expect("disable unrelated context lookup");
    let token = sdk.new_operation_token().expect("scope token");
    let preparation = sdk.prepare_playback(
        token,
        ITEM.into(),
        PlaybackStartPosition::Resume,
        PlaybackSelection::default(),
    );
    let server = async {
        let detail = request(requests).await;
        assert!(detail.headers.starts_with("GET /Items/"));
        detail.reply.send(detail_json).expect("detail reply");
        let media = request(requests).await;
        assert!(media.headers.starts_with("POST /Items/"));
        let payload: serde_json::Value =
            serde_json::from_str(&media.body).expect("playback request");
        assert_eq!(payload["EnableTranscoding"], false);
        assert_eq!(payload["AutoOpenLiveStream"], false);
        media.reply.send(MEDIA).expect("media reply");
        if episode {
            let segments = request(requests).await;
            assert!(segments.headers.starts_with("GET /MediaSegments/"));
            segments.reply.send(r#"{"Items":[{"Type":"Intro","StartTicks":100000000,"EndTicks":300000000},{"Type":"Outro","StartTicks":500000000,"EndTicks":600000000}]}"#).expect("real ranges");
        }
    };
    let (session, ()) = tokio::join!(
        async {
            preparation
                .await
                .unwrap_or_else(|error| panic!("preparation failed: {error:?}"))
        },
        server
    );
    session
}

fn observation(sequence: u64, position: f64) -> PlaybackObservation {
    PlaybackObservation {
        sequence,
        position_seconds: position,
        paused: false,
        muted: false,
        volume: 73.0,
        audio_stream_index: Some(2),
        subtitle_stream_index: Some(-1),
    }
}

#[tokio::test]
async fn playback_uses_original_media_reports_observed_state_and_clears_recovery_on_exit() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared(&sdk, &mut requests).await;
    let plan = session.plan();
    assert_eq!(plan.start_position_seconds, 20.0);
    assert!(!plan.stream_url.as_str().contains("must-not-transcode"));
    assert!(plan.stream_url.as_str().contains("Static=true"));
    assert!(!format!("{plan:?}").contains("token-ada"));
    assert!(plan.external_subtitles[0]
        .url
        .as_str()
        .contains("Stream.ass"));
    let server = async {
        let start = request(&mut requests).await;
        assert!(start.headers.starts_with("POST /Sessions/Playing "));
        let payload: serde_json::Value = serde_json::from_str(&start.body).expect("start report");
        assert_eq!(payload["PositionTicks"], 220000000);
        assert_eq!(payload["MediaSourceId"], "source");
        assert_eq!(payload["PlaySessionId"], "server-session");
        assert_eq!(payload["AudioStreamIndex"], 2);
        start.reply.send("").expect("start accepted");
    };
    let (update, ()) = tokio::join!(session.observe(observation(1, 22.0), false), server);
    assert!(update.expect("observation accepted").reported);
    let token = sdk.new_operation_token().expect("token");
    assert_eq!(
        sdk.local_playback_recovery(token.clone())
            .await
            .expect("recovery")
            .expect("saved recovery")
            .position_seconds,
        22.0
    );
    assert_eq!(
        session
            .observe(observation(1, 2.0), false)
            .await
            .expect_err("replayed observation"),
        SdkError::Stale
    );
    assert!(
        !session
            .observe(observation(2, 23.0), false)
            .await
            .expect("passive observation")
            .reported
    );
    let server = async {
        let stop = request(&mut requests).await;
        assert!(stop.headers.starts_with("POST /Sessions/Playing/Stopped "));
        let payload: serde_json::Value = serde_json::from_str(&stop.body).expect("stop report");
        assert_eq!(payload["PositionTicks"], 240000000);
        stop.reply.send("").expect("stop accepted");
    };
    let (finished, ()) = tokio::join!(session.finish(observation(3, 24.0), false), server);
    assert!(finished.expect("finished").report_error.is_none());
    assert!(!session.is_active());
    assert!(sdk
        .local_playback_recovery(token)
        .await
        .expect("recovery read")
        .is_none());
}

#[tokio::test]
async fn failed_start_report_is_not_success_and_retries_without_losing_recovery() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared(&sdk, &mut requests).await;
    let fail = async {
        drop(request(&mut requests).await.reply);
    };
    let (update, ()) = tokio::join!(session.observe(observation(1, 35.0), false), fail);
    let update = update.expect("player observation retained");
    assert!(!update.reported);
    assert!(update.report_error.is_some());
    let token = sdk.new_operation_token().expect("token");
    assert_eq!(
        sdk.local_playback_recovery(token)
            .await
            .expect("recovery read")
            .expect("recovery survives")
            .position_seconds,
        35.0
    );
    let retry = async {
        let start = request(&mut requests).await;
        assert!(start.headers.starts_with("POST /Sessions/Playing "));
        start.reply.send("").expect("retry accepted");
    };
    let (update, ()) = tokio::join!(session.observe(observation(2, 36.0), false), retry);
    assert!(update.expect("retry").reported);
}

#[tokio::test]
async fn old_session_cannot_report_or_execute_after_scope_replacement() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let old = prepared(&sdk, &mut requests).await;
    sdk.adopt_test_session(test_session("grace", &url));
    let current = prepared(&sdk, &mut requests).await;
    assert!(!old.is_active());
    assert!(current.is_active());
    assert_eq!(
        old.observe(observation(1, 44.0), true)
            .await
            .expect_err("old callback"),
        SdkError::Stale
    );
    assert_eq!(
        old.run_admitted(async { panic!("stale native operation must not run") })
            .await
            .expect_err("old native action"),
        SdkError::Stale
    );
    assert!(
        requests.try_recv().is_err(),
        "stale work never reaches provider"
    );
}

#[tokio::test]
async fn cancelled_preparation_does_not_leave_a_session_that_blocks_retry() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let token = sdk.new_operation_token().expect("token");
    let pending = sdk.prepare_playback(
        token.clone(),
        ITEM.into(),
        PlaybackStartPosition::Beginning,
        PlaybackSelection::default(),
    );
    let cancel = async {
        let request = request(&mut requests).await;
        token.cancel();
        request.disconnected.await.expect("cancel closes request");
    };
    let (result, ()) = tokio::join!(pending, cancel);
    assert!(matches!(result, Err(SdkError::Cancelled)));
    assert!(prepared(&sdk, &mut requests).await.is_active());
}

const EPISODE: &str = r#"{"Id":"00000000000000000000000000000001","Name":"Episode","Type":"Episode","SeriesId":"00000000000000000000000000000002","SeriesName":"Show","IndexNumber":1,"ParentIndexNumber":1,"OriginalLanguage":"en","UserData":{"Key":"episode","Played":false}}"#;

#[tokio::test]
async fn manual_intro_requires_presentation_and_credit_skip_does_not_advance_an_episode() {
    use crate::playback::IntroSkipMode;
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    sdk.set_intro_mode(IntroSkipMode::Manual)
        .expect("manual initial preference");
    let session = prepared_item(&sdk, &mut requests, EPISODE, true).await;
    let server = async {
        request(&mut requests)
            .await
            .reply
            .send("")
            .expect("started");
    };
    let (update, ()) = tokio::join!(session.observe(observation(1, 10.0), false), server);
    let prompt = update
        .expect("in range")
        .skip_prompt
        .expect("manual prompt");
    assert_eq!(prompt.end_seconds, 30.0);
    assert_eq!(session.skip_intro().expect("unpresented prompt"), None);
    session.acknowledge_skip_prompt(true).expect("shown");
    assert_eq!(session.skip_intro().expect("manual skip"), Some(30.0));
    session
        .set_intro_mode(IntroSkipMode::Automatic)
        .expect("session-only override");
    let credit = session
        .observe(observation(2, 50.0), false)
        .await
        .expect("credit observation");
    assert_eq!(credit.seek_to, Some(60.0));
    assert!(session
        .observe(observation(3, 51.0), false)
        .await
        .expect("repeat position")
        .seek_to
        .is_none());
    assert!(
        requests.try_recv().is_err(),
        "credit skip performs no episode lookup"
    );
    assert_eq!(
        sdk.business_preferences()
            .expect("persistent preference")
            .intro_mode,
        IntroSkipMode::Manual
    );
}

#[tokio::test]
async fn only_natural_completion_returns_the_real_adjacent_episode_and_clears_recovery() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared_item(&sdk, &mut requests, EPISODE, true).await;
    let server = async {
        request(&mut requests)
            .await
            .reply
            .send("")
            .expect("started");
    };
    let (started, ()) = tokio::join!(session.observe(observation(1, 80.0), false), server);
    started.expect("started");
    let server = async {
        let stop = request(&mut requests).await;
        assert!(stop.headers.starts_with("POST /Sessions/Playing/Stopped "));
        stop.reply.send("").expect("stopped");
        let item = request(&mut requests).await;
        assert!(item.headers.starts_with("GET /Users/"));
        item.reply.send(EPISODE).expect("episode context");
        let adjacent = request(&mut requests).await;
        assert!(adjacent.headers.starts_with("GET /Shows/"));
        adjacent.reply.send(r#"{"Items":[{"Id":"00000000000000000000000000000001","Name":"Current","Type":"Episode"},{"Id":"00000000000000000000000000000003","Name":"Next","Type":"Episode"}],"TotalRecordCount":2}"#).expect("adjacent response");
    };
    let (finished, ()) = tokio::join!(session.finish(observation(2, 90.0), true), server);
    assert_eq!(
        finished.expect("natural end").next_item_id.as_deref(),
        Some("00000000000000000000000000000003")
    );
    assert!(sdk
        .local_playback_recovery(sdk.new_operation_token().expect("token"))
        .await
        .expect("recovery read")
        .is_none());
}

#[tokio::test]
async fn recovery_survives_restart_and_is_removed_by_successful_account_exit() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared(&sdk, &mut requests).await;
    let server = async {
        request(&mut requests)
            .await
            .reply
            .send("")
            .expect("started");
    };
    let (started, ()) = tokio::join!(session.observe(observation(1, 53.0), false), server);
    started.expect("observed");
    drop(session);
    sdk.close();
    let reopened = Sdk::new(
        crate::SdkConfig {
            storage_dir: dir.path().into(),
            device_name: "reopened".into(),
        },
        Arc::new(super::MemoryCredential::default()),
        None,
    )
    .expect("reopened SDK");
    reopened.adopt_test_session(test_session("ada", &url));
    assert_eq!(
        reopened
            .local_playback_recovery(reopened.new_operation_token().expect("token"))
            .await
            .expect("restored record")
            .expect("recovery")
            .position_seconds,
        53.0
    );
    reopened.disconnect().await.expect("explicit account exit");
    reopened.adopt_test_session(test_session("ada", &url));
    assert!(reopened
        .local_playback_recovery(reopened.new_operation_token().expect("token"))
        .await
        .expect("recovery read")
        .is_none());
}

#[tokio::test]
async fn disabling_auto_next_during_playback_takes_effect_at_that_items_natural_end() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared_item(&sdk, &mut requests, EPISODE, true).await;
    let server = async {
        request(&mut requests)
            .await
            .reply
            .send("")
            .expect("started");
    };
    let (started, ()) = tokio::join!(session.observe(observation(1, 80.0), false), server);
    started.expect("started");
    sdk.set_auto_play_next(false)
        .expect("disable while playing");
    let server = async {
        let stop = request(&mut requests).await;
        assert!(stop.headers.starts_with("POST /Sessions/Playing/Stopped "));
        stop.reply.send("").expect("stop accepted");
    };
    let (finished, ()) = tokio::join!(session.finish(observation(2, 90.0), true), server);
    assert!(finished.expect("natural finish").next_item_id.is_none());
    assert!(
        requests.try_recv().is_err(),
        "disabled auto-next never queries another episode"
    );
}

#[tokio::test]
async fn cancelled_host_waiter_keeps_admission_until_physical_operation_settles() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    let sdk = Arc::new(sdk);
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared(&sdk, &mut requests).await;
    let (entered, entered_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = tokio::sync::oneshot::channel();
    let host_session = Arc::clone(&session);
    let waiter = tokio::spawn(async move {
        host_session
            .run_admitted(async move {
                entered.send(()).expect("host entered");
                release_rx.await.expect("physical drain receipt");
                true
            })
            .await
    });
    entered_rx.await.expect("admitted host started");
    waiter.abort();
    assert!(waiter.await.expect_err("waiter cancelled").is_cancelled());
    assert!(
        sdk.inner.playback_execution.try_write().is_err(),
        "cancelled waiter must not release physical admission"
    );
    let disconnect_sdk = Arc::clone(&sdk);
    let disconnect = tokio::spawn(async move { disconnect_sdk.disconnect().await });
    while !sdk.content_mutations_blocked() {
        tokio::task::yield_now().await;
    }
    assert!(
        sdk.active_profile().is_some(),
        "handoff cannot disconnect an undrained native player"
    );
    release.send(()).expect("physical operation completed");
    let stop = request(&mut requests).await;
    assert!(stop.headers.starts_with("POST /Sessions/Playing/Stopped "));
    stop.reply.send("").expect("stop reported");
    disconnect
        .await
        .expect("handoff task")
        .expect("handoff settled");
    assert!(sdk.active_profile().is_none());
}

#[tokio::test]
async fn sdk_close_keeps_an_admitted_host_operation_and_its_authentication_until_settlement() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared(&sdk, &mut requests).await;
    let (entered, entered_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = tokio::sync::oneshot::channel();
    let waiter = tokio::spawn(async move {
        session
            .run_admitted(async move {
                entered.send(()).expect("host entered");
                release_rx.await.expect("physical drain receipt");
                true
            })
            .await
    });
    entered_rx.await.expect("admitted host started");
    sdk.close();
    assert!(
        sdk.active_profile().is_some(),
        "SDK close retains authentication during admitted physical work"
    );
    assert!(sdk.inner.playback_execution.try_write().is_err());
    release.send(()).expect("physical operation completed");
    assert!(waiter
        .await
        .expect("host worker outlives SDK runtime")
        .expect("physical operation settled"));
    assert!(
        sdk.active_profile().is_none(),
        "last committed worker settles terminal close"
    );
}

#[tokio::test]
async fn unexpected_playback_end_preserves_latest_recovery_even_when_stop_report_fails() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let session = prepared_item(&sdk, &mut requests, EPISODE, true).await;
    let server = async {
        request(&mut requests)
            .await
            .reply
            .send("")
            .expect("started");
    };
    let (started, ()) = tokio::join!(session.observe(observation(1, 65.0), false), server);
    started.expect("loaded player observed");
    let server = async {
        let stopped = request(&mut requests).await;
        assert!(stopped
            .headers
            .starts_with("POST /Sessions/Playing/Stopped "));
        drop(stopped.reply);
    };
    let (interrupted, ()) = tokio::join!(session.interrupt(observation(2, 72.0)), server);
    let interrupted = interrupted.expect("session interrupted");
    assert!(interrupted.report_error.is_some());
    assert!(interrupted.next_item_id.is_none());
    assert!(!session.is_active());
    assert_eq!(
        sdk.local_playback_recovery(sdk.new_operation_token().expect("token"))
            .await
            .expect("recovery")
            .expect("interrupted session retained")
            .position_seconds,
        72.0
    );
    assert!(
        requests.try_recv().is_err(),
        "interruption does not query next episode"
    );
}
