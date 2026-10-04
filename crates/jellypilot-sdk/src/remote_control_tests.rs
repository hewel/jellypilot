use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

use super::{controlled_http_server, test_sdk, test_session, HttpRequest};
use crate::playback::PlaybackStartPosition;
use crate::remote_control::{
    RemoteControlCommand as Command, RemoteControlSnapshot, RemoteControlTargetKey,
    RemoteController, RemoteControllerStatus as Status,
};
use crate::{Sdk, SdkError};

const USER: &str = "00000000000000000000000000000009";
const ITEM: &str = "00000000000000000000000000000002";
type Requests = UnboundedReceiver<HttpRequest<String>>;

fn targets(item: &str) -> String {
    serde_json::json!([{
        "Id":"living-room", "DeviceId":"television", "DeviceName":"Living room",
        "Client":"Target", "UserName":"Other user", "IsActive":true,
        "SupportsRemoteControl":true, "SupportsMediaControl":true,
        "PlayableMediaTypes":["Video"], "SupportedCommands":["SetVolume"],
        "NowPlayingItem":{"Id":item,"Name":"Movie","RunTimeTicks":1200000000_i64},
        "PlayState":{"PositionTicks":200000000,"IsPaused":false,"CanSeek":true,"VolumeLevel":30}
    }, {
        "Id":"bedroom", "DeviceId":"second-tv", "DeviceName":"Bedroom", "Client":"Target",
        "SupportsRemoteControl":true,"SupportsMediaControl":true,"PlayableMediaTypes":["Video"]
    }])
    .to_string()
}

fn key() -> RemoteControlTargetKey {
    RemoteControlTargetKey {
        session_id: "living-room".into(),
        device_id: "television".into(),
    }
}

async fn request(requests: &mut Requests) -> HttpRequest<String> {
    tokio::time::timeout(Duration::from_secs(10), requests.recv())
        .await
        .expect("request arrives")
        .expect("server alive")
}

async fn wait(
    controller: &RemoteController,
    predicate: impl Fn(&RemoteControlSnapshot) -> bool,
) -> RemoteControlSnapshot {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut snapshot = controller.snapshot().expect("live scope");
        loop {
            if predicate(&snapshot) {
                return snapshot;
            }
            snapshot = controller
                .next_snapshot(snapshot.revision)
                .await
                .expect("next snapshot");
        }
    })
    .await
    .expect("controller reaches expected state")
}

async fn ready(sdk: &Sdk, requests: &mut Requests) -> Arc<RemoteController> {
    let controller = sdk.open_remote_controller().expect("open");
    assert_eq!(controller.snapshot().unwrap().status, Status::Inactive);
    assert!(requests.try_recv().is_err());
    controller.set_active(true).unwrap();
    request(requests).await.reply.send(targets(ITEM)).unwrap();
    wait(&controller, |s| s.status == Status::Ready && !s.refreshing).await;
    controller.select_target(key()).unwrap();
    request(requests).await.reply.send(targets(ITEM)).unwrap();
    wait(&controller, |s| s.status == Status::Ready && !s.refreshing).await;
    controller
}

fn execute(
    controller: &Arc<RemoteController>,
    command: Command,
) -> tokio::task::JoinHandle<Result<crate::remote_control::RemoteControlReceipt, SdkError>> {
    let generation = controller.snapshot().unwrap().generation;
    let controller = Arc::clone(controller);
    tokio::spawn(async move { controller.execute(generation, key(), command).await })
}

#[tokio::test]
async fn activation_coalesces_refresh_and_inactivation_cancels_the_socket() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = sdk.open_remote_controller().unwrap();
    assert_eq!(controller.refresh(), Err(SdkError::Cancelled));
    controller.set_active(true).unwrap();
    let first = request(&mut requests).await;
    for _ in 0..8 {
        controller.refresh().unwrap();
    }
    assert!(
        requests.try_recv().is_err(),
        "one discovery remains in flight"
    );
    first.reply.send(targets(ITEM)).unwrap();
    let second = request(&mut requests).await;
    controller.set_active(false).unwrap();
    tokio::time::timeout(Duration::from_secs(2), second.disconnected)
        .await
        .unwrap()
        .unwrap();
    let inactive = controller.snapshot().unwrap();
    assert_eq!(inactive.status, Status::Inactive);
    assert!(inactive.targets.is_empty());
    assert!(!inactive.refreshing);
    controller.set_active(true).unwrap();
    request(&mut requests)
        .await
        .reply
        .send(targets(ITEM))
        .unwrap();
    wait(&controller, |s| s.status == Status::Ready).await;
    assert!(
        requests.try_recv().is_err(),
        "old refresh demand is not replayed"
    );
}

#[tokio::test]
async fn command_refreshes_authority_and_reports_acceptance_without_inventing_playback() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let command = execute(&controller, Command::Pause);
    let refresh = request(&mut requests).await;
    assert!(refresh.headers.starts_with("GET /Sessions?"));
    assert_eq!(
        controller
            .execute(
                controller.snapshot().unwrap().generation,
                key(),
                Command::Stop
            )
            .await,
        Err(SdkError::OperationInProgress)
    );
    refresh.reply.send(targets(ITEM)).unwrap();
    let post = request(&mut requests).await;
    assert!(post
        .headers
        .starts_with("POST /Sessions/living-room/Playing/Pause?"));
    post.reply.send(String::new()).unwrap();
    assert!(command.await.unwrap().unwrap().server_accepted);
    let after = controller.snapshot().unwrap();
    assert_eq!(
        after.targets[0].now_playing.as_ref().unwrap().paused,
        Some(false)
    );
    request(&mut requests)
        .await
        .reply
        .send(targets(ITEM))
        .unwrap();
    wait(&controller, |s| !s.refreshing && !s.command_pending).await;
}

#[tokio::test]
async fn selected_item_change_rejects_a_seek_captured_for_the_previous_movie() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let generation = controller.snapshot().unwrap().generation;
    let command = execute(&controller, Command::Seek { seconds: 30.0 });
    request(&mut requests)
        .await
        .reply
        .send(targets("00000000000000000000000000000003"))
        .unwrap();
    assert_eq!(command.await.unwrap(), Err(SdkError::Stale));
    assert_ne!(controller.snapshot().unwrap().generation, generation);
    assert!(requests.try_recv().is_err(), "no old seek reached new item");
}

#[tokio::test]
async fn target_replacement_cancels_waiting_command_without_redirecting_it() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let command = execute(&controller, Command::Stop);
    let refresh = request(&mut requests).await;
    controller
        .select_target(RemoteControlTargetKey {
            session_id: "bedroom".into(),
            device_id: "second-tv".into(),
        })
        .unwrap();
    assert_eq!(command.await.unwrap(), Err(SdkError::Stale));
    tokio::time::timeout(Duration::from_secs(2), refresh.disconnected)
        .await
        .unwrap()
        .unwrap();
    let new_refresh = request(&mut requests).await;
    assert!(new_refresh.headers.starts_with("GET /Sessions?"));
    controller.close();
}

#[tokio::test]
async fn handoff_and_pending_credential_cleanup_are_rechecked_after_discovery() {
    for cleanup in [false, true] {
        let (server, mut requests, _server) = controlled_http_server().await;
        let (sdk, _dir) = test_sdk();
        sdk.adopt_test_session(test_session(USER, &server));
        let controller = ready(&sdk, &mut requests).await;
        let command = execute(&controller, Command::Pause);
        let refresh = request(&mut requests).await;
        {
            let mut state = sdk.inner.state.lock().unwrap();
            state.handoff_in_progress = !cleanup;
            state.sign_out_cleanup_pending = cleanup;
        }
        refresh.reply.send(targets(ITEM)).unwrap();
        assert_eq!(command.await.unwrap(), Err(SdkError::OperationInProgress));
        assert!(controller
            .snapshot()
            .unwrap()
            .targets
            .iter()
            .all(|target| !target.capabilities.can_pause));
        assert!(
            requests.try_recv().is_err(),
            "no POST during account transition"
        );
        let mut state = sdk.inner.state.lock().unwrap();
        state.handoff_in_progress = false;
        state.sign_out_cleanup_pending = false;
    }
}

#[tokio::test]
async fn scope_replacement_cancels_network_and_snapshot_waiters() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let command = execute(&controller, Command::Pause);
    let refresh = request(&mut requests).await;
    sdk.adopt_test_session(test_session("00000000000000000000000000000008", &server));
    assert_eq!(command.await.unwrap(), Err(SdkError::Stale));
    tokio::time::timeout(Duration::from_secs(2), refresh.disconnected)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        controller.next_snapshot(0).await,
        Err(SdkError::Stale)
    ));
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn failed_or_missing_target_refresh_disables_controls_and_retry_revalidates() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let generation = controller.snapshot().unwrap().generation;
    controller.refresh().unwrap();
    request(&mut requests)
        .await
        .reply
        .send("not json".into())
        .unwrap();
    let failed = wait(&controller, |s| s.status == Status::Failed).await;
    assert!(failed.targets.is_empty() && failed.selected.is_none());
    assert!(failed.error.is_some());
    assert_eq!(
        controller.execute(generation, key(), Command::Stop).await,
        Err(SdkError::Stale)
    );
    controller.refresh().unwrap();
    request(&mut requests)
        .await
        .reply
        .send(targets(ITEM))
        .unwrap();
    wait(&controller, |s| {
        s.status == Status::Ready && s.selected == Some(key())
    })
    .await;
    let command = execute(&controller, Command::Stop);
    request(&mut requests)
        .await
        .reply
        .send("[]".into())
        .unwrap();
    assert_eq!(command.await.unwrap(), Err(SdkError::Stale));
    let missing = controller.snapshot().unwrap();
    assert!(missing.selected.is_none() && missing.targets.is_empty());
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn invalid_values_never_start_network_and_old_page_generations_never_match() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let old = controller.snapshot().unwrap().generation;
    for value in [f64::NAN, f64::INFINITY, -1.0, f64::MAX] {
        for command in [
            Command::Seek { seconds: value },
            Command::PlayNow {
                item_id: ITEM.into(),
                position: PlaybackStartPosition::At(value),
            },
        ] {
            assert!(matches!(
                controller.execute(old, key(), command).await,
                Err(SdkError::InvalidInput(_))
            ));
        }
    }
    assert!(matches!(
        controller
            .execute(old, key(), Command::SetVolume { volume: 101 })
            .await,
        Err(SdkError::InvalidInput(_))
    ));
    for item_id in ["../Sessions", "item?userId=other", "invalid"] {
        assert!(matches!(
            controller
                .execute(
                    old,
                    key(),
                    Command::PlayNow {
                        item_id: item_id.into(),
                        position: PlaybackStartPosition::Beginning
                    }
                )
                .await,
            Err(SdkError::InvalidInput(_))
        ));
    }
    assert!(requests.try_recv().is_err());
    controller.close();
    let replacement = ready(&sdk, &mut requests).await;
    assert_eq!(
        replacement.execute(old, key(), Command::Pause).await,
        Err(SdkError::Stale)
    );
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn play_now_resolves_real_movie_resume_without_local_playback_preparation() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let command = execute(
        &controller,
        Command::PlayNow {
            item_id: ITEM.into(),
            position: PlaybackStartPosition::Resume,
        },
    );
    let detail = request(&mut requests).await;
    assert!(detail.headers.starts_with(&format!("GET /Items/{ITEM}?")));
    detail.reply.send(serde_json::json!({"Id":ITEM,"Name":"Film","Type":"Movie","RunTimeTicks":6000000000_i64,"OriginalLanguage":"en","UserData":{"Key":"item","PlaybackPositionTicks":200000000,"Played":false}}).to_string()).unwrap();
    let refresh = request(&mut requests).await;
    assert!(refresh.headers.starts_with("GET /Sessions?"));
    refresh.reply.send(targets(ITEM)).unwrap();
    let post = request(&mut requests).await;
    assert!(post
        .headers
        .starts_with("POST /Sessions/living-room/Playing?"));
    assert!(post.headers.contains("playCommand=PlayNow"));
    assert!(post.headers.contains("startPositionTicks=200000000"));
    post.reply.send(String::new()).unwrap();
    assert!(command.await.unwrap().unwrap().server_accepted);
    controller.close();
}

#[tokio::test]
async fn dropping_the_only_handle_cancels_its_worker_during_discovery() {
    let (server, mut requests, _server) = controlled_http_server::<String>().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = sdk.open_remote_controller().unwrap();
    controller.set_active(true).unwrap();
    let pending = request(&mut requests).await;
    drop(controller);
    tokio::time::timeout(Duration::from_secs(2), pending.disconnected)
        .await
        .unwrap()
        .unwrap();
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn cancelling_a_command_waiter_releases_busy_state_and_aborts_its_request() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let command = execute(&controller, Command::Pause);
    let pending = request(&mut requests).await;
    command.abort();
    assert!(command.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2), pending.disconnected)
        .await
        .unwrap()
        .unwrap();
    wait(&controller, |snapshot| !snapshot.command_pending).await;
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn play_now_rejects_series_without_preparing_an_arbitrary_episode() {
    let (server, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session(USER, &server));
    let controller = ready(&sdk, &mut requests).await;
    let command = execute(
        &controller,
        Command::PlayNow {
            item_id: ITEM.into(),
            position: PlaybackStartPosition::Beginning,
        },
    );
    request(&mut requests).await.reply.send(serde_json::json!({"Id":ITEM,"Name":"Series","Type":"Series","OriginalLanguage":"en","UserData":{"Key":"item","Played":false}}).to_string()).unwrap();
    assert!(matches!(command.await.unwrap(), Err(SdkError::Request(_))));
    assert!(requests.try_recv().is_err());
}
