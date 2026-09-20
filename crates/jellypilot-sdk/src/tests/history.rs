use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

use super::{controlled_http_server, test_sdk, test_session, HttpRequest};
use crate::{Sdk, SdkConfig, SdkError};

type Requests = UnboundedReceiver<HttpRequest<String>>;

fn item_id(index: u32) -> String {
    format!("{index:032x}")
}

fn history_response(played: bool) -> String {
    let indexes = if played { [1, 2] } else { [3, 4] };
    serde_json::json!({
        "Items": indexes.map(|index| serde_json::json!({
            "Id": item_id(index), "Type": "Movie", "Name": format!("Film {index}"), "RunTimeTicks": 6000000000_i64,
            "UserData": { "Key": item_id(index), "Played": played, "PlaybackPositionTicks": if played { 0_i64 } else { 1200000000_i64 }, "LastPlayedDate": format!("2026-09-2{}T12:00:00Z", 5 - index) }
        })),
        "TotalRecordCount": 2, "StartIndex": 0,
    }).to_string()
}

async fn next_request(requests: &mut Requests) -> HttpRequest<String> {
    tokio::time::timeout(Duration::from_secs(10), requests.recv())
        .await
        .expect("history request arrives")
        .expect("history server alive")
}

fn answer(request: HttpRequest<String>) {
    assert!(
        request.headers.starts_with("GET /Items?"),
        "History reads/hiding must never issue server writes"
    );
    let played = request
        .headers
        .to_ascii_lowercase()
        .contains("isplayed=true");
    request
        .reply
        .send(history_response(played))
        .expect("history reply accepted");
}

async fn answer_reads(requests: &mut Requests, count: usize) {
    for _ in 0..count {
        answer(next_request(requests).await);
    }
}

#[tokio::test]
async fn local_history_hide_fills_hidden_pages_and_undo_preserves_order_and_progress() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let token = sdk.new_operation_token().unwrap();
    let first = sdk
        .hide_history_item(Arc::clone(&token), item_id(1))
        .await
        .unwrap();
    let second = sdk
        .hide_history_item(Arc::clone(&token), item_id(2))
        .await
        .unwrap();
    assert!(requests.try_recv().is_err(), "hiding is device-local");
    let (page, ()) = tokio::join!(
        async { sdk.watch_history(Arc::clone(&token), 0, 2).await.unwrap() },
        answer_reads(&mut requests, 4)
    );
    assert_eq!(
        page.items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        [item_id(3), item_id(4)]
    );
    assert_eq!(page.next_start_index, 4);
    assert!(!page.has_more);
    assert_eq!(page.items[0].resume_position_seconds, Some(120.0));
    assert!(!page.items[0].played);

    assert!(first.undo().await.unwrap());
    assert!(!first.undo().await.unwrap());
    assert!(
        requests.try_recv().is_err(),
        "undo must not report or reset progress"
    );
    let (page, ()) = tokio::join!(
        async { sdk.watch_history(Arc::clone(&token), 0, 2).await.unwrap() },
        answer_reads(&mut requests, 4)
    );
    assert_eq!(
        page.items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        [item_id(1), item_id(3)]
    );
    assert!(
        page.items[0].played,
        "restoring history must preserve Played"
    );
    assert_eq!(page.next_start_index, 3);
    assert!(page.has_more);
    let (last, ()) = tokio::join!(
        async {
            sdk.watch_history(Arc::clone(&token), page.next_start_index, 2)
                .await
                .unwrap()
        },
        answer_reads(&mut requests, 2)
    );
    assert_eq!(last.items[0].id, item_id(4));
    assert_eq!(last.next_start_index, 4);
    assert!(!last.has_more);

    sdk.adopt_test_session(test_session("grace", &url));
    assert_eq!(second.undo().await, Err(SdkError::Stale));
    let (other, ()) = tokio::join!(
        async {
            sdk.watch_history(sdk.new_operation_token().unwrap(), 0, 2)
                .await
                .unwrap()
        },
        answer_reads(&mut requests, 2)
    );
    assert_eq!(
        other
            .items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        [item_id(1), item_id(2)]
    );
}

#[tokio::test]
async fn history_visibility_survives_reopening_and_failed_undo_remains_retryable() {
    let (sdk, dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let removal = sdk
        .hide_history_item(sdk.new_operation_token().unwrap(), item_id(1))
        .await
        .unwrap();
    let original = std::fs::read(dir.path().join("history-visibility.json")).unwrap();
    let temporary = dir.path().join("history-visibility.json.tmp");
    std::fs::create_dir(&temporary).unwrap();
    assert!(matches!(removal.undo().await, Err(SdkError::Storage(_))));
    assert_eq!(
        std::fs::read(dir.path().join("history-visibility.json")).unwrap(),
        original
    );
    std::fs::remove_dir(&temporary).unwrap();
    let reopened = Sdk::new(
        SdkConfig {
            storage_dir: dir.path().to_owned(),
            device_name: "reopened".into(),
        },
        Arc::new(super::MemoryCredential::default()),
        None,
    )
    .unwrap();
    reopened.adopt_test_session(test_session("ada", "https://media.example.test"));
    let repeated = reopened
        .hide_history_item(reopened.new_operation_token().unwrap(), item_id(1))
        .await
        .unwrap();
    assert!(
        !repeated.undo().await.unwrap(),
        "repeated hide cannot undo another removal"
    );
    assert!(
        removal.undo().await.unwrap(),
        "failed disk write must not consume undo"
    );
}

#[tokio::test]
async fn a_pending_history_page_cannot_reintroduce_an_item_hidden_during_the_request() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let pending = sdk.watch_history(sdk.new_operation_token().unwrap(), 0, 2);
    let server = async {
        let request = next_request(&mut requests).await;
        sdk.hide_history_item(sdk.new_operation_token().unwrap(), item_id(1))
            .await
            .unwrap();
        answer(request);
        answer(next_request(&mut requests).await);
    };
    let (result, ()) = tokio::join!(pending, server);
    assert!(matches!(result, Err(SdkError::Stale)));
}

#[tokio::test]
async fn desktop_history_reappears_for_newer_server_observation_without_changing_android_reads() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let removal = sdk
        .hide_desktop_history_item(
            sdk.new_operation_token().unwrap(),
            item_id(1),
            Some("2026-09-23T12:00:00Z".to_owned()),
        )
        .await
        .unwrap();
    let (android, ()) = tokio::join!(
        async {
            sdk.watch_history(sdk.new_operation_token().unwrap(), 0, 2)
                .await
                .unwrap()
        },
        answer_reads(&mut requests, 4)
    );
    assert_eq!(
        android
            .items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        [item_id(2), item_id(3)]
    );
    let (desktop, ()) = tokio::join!(
        async {
            sdk.desktop_watch_history(sdk.new_operation_token().unwrap(), 0, 2)
                .await
                .unwrap()
        },
        answer_reads(&mut requests, 2)
    );
    assert_eq!(
        desktop
            .items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        [item_id(1), item_id(2)]
    );
    assert_eq!(desktop.total_record_count, Some(4));
    assert!(
        !removal.undo().await.unwrap(),
        "server reappearance already consumed the hidden membership"
    );
}

#[tokio::test]
async fn desktop_partial_history_omits_unknown_total_and_retains_the_server_cursor() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    sdk.hide_desktop_history_item(
        sdk.new_operation_token().unwrap(),
        item_id(1),
        Some("2026-09-24T12:00:00Z".to_owned()),
    )
    .await
    .unwrap();
    // This stale stored ID must not be subtracted from the server count.
    sdk.hide_history_item(sdk.new_operation_token().unwrap(), item_id(99))
        .await
        .unwrap();
    let (first, ()) = tokio::join!(
        async {
            sdk.desktop_watch_history(sdk.new_operation_token().unwrap(), 0, 2)
                .await
                .unwrap()
        },
        answer_reads(&mut requests, 4)
    );
    assert_eq!(first.total_record_count, None);
    assert_eq!(first.next_start_index, 3);
    assert!(first.has_more);
    assert_eq!(
        first
            .items
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>(),
        [item_id(2), item_id(3)]
    );
    assert!(
        requests.try_recv().is_err(),
        "counting must not scan beyond the requested page"
    );
    let (last, ()) = tokio::join!(
        async {
            sdk.desktop_watch_history(
                sdk.new_operation_token().unwrap(),
                first.next_start_index,
                2,
            )
            .await
            .unwrap()
        },
        answer_reads(&mut requests, 2)
    );
    assert_eq!(last.total_record_count, None);
    assert_eq!(last.next_start_index, 4);
    assert!(!last.has_more);
    let (complete, ()) = tokio::join!(
        async {
            sdk.desktop_watch_history(sdk.new_operation_token().unwrap(), 0, 10)
                .await
                .unwrap()
        },
        answer_reads(&mut requests, 2)
    );
    assert_eq!(complete.total_record_count, Some(3));
}

#[tokio::test]
async fn playback_reappearance_and_later_removal_retire_old_undo_without_crossing_scope() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().unwrap();
    let old = sdk
        .hide_desktop_history_item(Arc::clone(&token), item_id(1), None)
        .await
        .unwrap();
    assert!(sdk
        .restore_desktop_history_for_playback(Arc::clone(&token), item_id(1))
        .await
        .unwrap());
    let current = sdk
        .hide_desktop_history_item(Arc::clone(&token), item_id(1), None)
        .await
        .unwrap();
    assert!(!old.undo().await.unwrap());
    sdk.inner.state.lock().unwrap().handoff_in_progress = true;
    assert_eq!(current.undo().await, Err(SdkError::OperationInProgress));
    assert_eq!(
        sdk.restore_desktop_history_for_playback(Arc::clone(&token), item_id(1))
            .await,
        Err(SdkError::OperationInProgress)
    );
    sdk.inner.state.lock().unwrap().handoff_in_progress = false;
    assert!(
        current.undo().await.unwrap(),
        "cancelled handoff retains the removal receipt"
    );
    sdk.adopt_test_session(test_session("grace", "https://media.example.test"));
    sdk.hide_desktop_history_item(sdk.new_operation_token().unwrap(), item_id(1), None)
        .await
        .unwrap();
    assert_eq!(
        sdk.restore_desktop_history_for_playback(token, item_id(1))
            .await,
        Err(SdkError::Stale)
    );
}

#[tokio::test]
async fn desktop_server_observation_cannot_commit_across_a_new_hide_or_profile_switch() {
    for switch_profile in [false, true] {
        let (url, mut requests, _server) = controlled_http_server().await;
        let (sdk, _dir) = test_sdk();
        sdk.adopt_test_session(test_session("ada", &url));
        sdk.hide_desktop_history_item(
            sdk.new_operation_token().unwrap(),
            item_id(1),
            Some("2026-09-20T12:00:00Z".to_owned()),
        )
        .await
        .unwrap();
        let pending = sdk.desktop_watch_history(sdk.new_operation_token().unwrap(), 0, 2);
        let server = async {
            let request = next_request(&mut requests).await;
            if switch_profile {
                sdk.adopt_test_session(test_session("grace", &url));
            } else {
                sdk.hide_desktop_history_item(sdk.new_operation_token().unwrap(), item_id(2), None)
                    .await
                    .unwrap();
            }
            answer(request);
            if !switch_profile {
                answer(next_request(&mut requests).await);
            }
        };
        let (result, ()) = tokio::join!(pending, server);
        assert!(matches!(
            result,
            Err(SdkError::Stale) | Err(SdkError::Cancelled)
        ));
    }
}
