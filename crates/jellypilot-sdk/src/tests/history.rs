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
