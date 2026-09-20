use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use jellypilot_core::browse_model::{LibraryBrowseView, LibraryItemSlot};
use jellypilot_core::LIBRARY_BROWSE_PAGE_SIZE;
use jellypilot_media_server::VideoUserDataAction;
use tokio::sync::mpsc::UnboundedReceiver;

use super::{controlled_http_server, test_sdk, test_session, HttpRequest};
use crate::browse::{BrowseQuery, BrowseSession, BrowseSnapshot};
use crate::{Sdk, SdkError};

type Requests = UnboundedReceiver<HttpRequest<String>>;

const OLD_ID: &str = "00000000000000000000000000000001";
const NEW_ID: &str = "00000000000000000000000000000002";

fn page(start: u32, total: u32, id_base: u32, favorite: bool) -> String {
    let mut body = String::from(r#"{"Items":["#);
    for index in start..start.saturating_add(LIBRARY_BROWSE_PAGE_SIZE).min(total) {
        if index != start {
            body.push(',');
        }
        let id = id_base + index;
        write!(body, r#"{{"Id":"{id:032x}","Name":"Movie {index}","Type":"Movie","UserData":{{"Key":"{id:032x}","Played":false,"IsFavorite":{favorite}}}}}"#)
            .expect("write page JSON");
    }
    write!(
        body,
        r#"],"TotalRecordCount":{total},"StartIndex":{start}}}"#
    )
    .expect("write page metadata");
    body
}

async fn next_request(requests: &mut Requests) -> HttpRequest<String> {
    tokio::time::timeout(Duration::from_secs(10), requests.recv())
        .await
        .expect("request reaches server")
        .expect("server remains available")
}

fn start_index(request: &HttpRequest<String>) -> u32 {
    request
        .headers
        .lines()
        .next()
        .expect("request line")
        .split_ascii_whitespace()
        .nth(1)
        .expect("request URI")
        .split_once('?')
        .map_or("", |(_, query)| query)
        .split('&')
        .find_map(|parameter| {
            let (name, value) = parameter.split_once('=')?;
            name.eq_ignore_ascii_case("StartIndex")
                .then(|| value.parse().expect("page index"))
        })
        .unwrap_or(0)
}

fn slots(snapshot: &BrowseSnapshot) -> &[LibraryItemSlot] {
    match &snapshot.view {
        LibraryBrowseView::Ready { visible_items, .. } => visible_items,
        view => panic!("expected ready content, got {view:?}"),
    }
}

async fn wait_until(
    session: &BrowseSession,
    predicate: impl Fn(&BrowseSnapshot) -> bool,
) -> BrowseSnapshot {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut snapshot = session.snapshot().expect("active browser");
        loop {
            if predicate(&snapshot) {
                return snapshot;
            }
            snapshot = session
                .next_snapshot(snapshot.revision)
                .await
                .expect("next browser revision");
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "browser did not reach expected state: {:?}",
            session.snapshot()
        )
    })
}

fn open(sdk: &Sdk) -> Arc<BrowseSession> {
    sdk.open_browser(BrowseQuery::Search {
        query: "movie".to_owned(),
    })
    .expect("open browser")
}

async fn open_ready(sdk: &Sdk, requests: &mut Requests) -> Arc<BrowseSession> {
    let session = open(sdk);
    next_request(requests)
        .await
        .reply
        .send(page(0, 1, 1, false))
        .expect("initial response");
    wait_until(&session, |snapshot| {
        matches!(snapshot.view, LibraryBrowseView::Ready { .. })
    })
    .await;
    session
}

#[tokio::test]
async fn scope_end_cancels_http_and_wakes_snapshot_waiters() {
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &server));
    let session = open(&sdk);
    let request = next_request(&mut requests).await;
    let revision = session.snapshot().expect("initial snapshot").revision;
    let pending = session.next_snapshot(revision);
    tokio::pin!(pending);
    tokio::select! {
        biased;
        result = &mut pending => panic!("no page has settled: {result:?}"),
        () = std::future::ready(()) => {}
    }

    sdk.adopt_test_session(test_session("grace", &server));

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), pending)
            .await
            .expect("waiter wakes")
            .expect_err("ended scope cannot deliver"),
        SdkError::Stale
    );
    tokio::time::timeout(Duration::from_secs(10), request.disconnected)
        .await
        .expect("scope cancellation closes HTTP")
        .expect("server observes disconnect");
}

#[tokio::test]
async fn dropped_session_does_not_survive_through_its_http_task() {
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &server));
    let session = open(&sdk);
    let request = next_request(&mut requests).await;

    drop(session);

    tokio::time::timeout(Duration::from_secs(10), request.disconnected)
        .await
        .expect("dropping the last handle cancels HTTP")
        .expect("server observes disconnect");
    assert!(
        sdk.active_profile().is_some(),
        "closing a browser must not end the account session"
    );
}

#[tokio::test]
async fn failed_refresh_retains_content_and_retry_replaces_it_atomically() {
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &server));
    let session = open_ready(&sdk, &mut requests).await;

    session.refresh().expect("start refresh");
    next_request(&mut requests)
        .await
        .reply
        .send("invalid JSON".to_owned())
        .expect("failed response");
    let failed = wait_until(&session, |snapshot| snapshot.refresh_failure.is_some()).await;
    assert_eq!(
        slots(&failed)[0].item.as_ref().expect("retained item").id,
        OLD_ID
    );
    assert!(!failed.refreshing);

    session.retry().expect("retry refresh");
    let retrying = session.snapshot().expect("retrying snapshot");
    assert!(retrying.refreshing);
    assert_eq!(
        slots(&retrying)[0].item.as_ref().expect("retained item").id,
        OLD_ID
    );
    next_request(&mut requests)
        .await
        .reply
        .send(page(0, 1, 2, false))
        .expect("retry response");
    let replaced = wait_until(&session, |snapshot| !snapshot.refreshing).await;
    assert_eq!(
        slots(&replaced)[0]
            .item
            .as_ref()
            .expect("replacement item")
            .id,
        NEW_ID
    );
    assert!(replaced.refresh_failure.is_none());
}

#[tokio::test]
async fn suspend_preserves_refresh_intent_and_resume_reissues_physical_work() {
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &server));
    let session = open_ready(&sdk, &mut requests).await;
    session.refresh().expect("refresh");
    let interrupted = next_request(&mut requests).await;

    session.suspend().expect("suspend");
    tokio::time::timeout(Duration::from_secs(10), interrupted.disconnected)
        .await
        .expect("suspend cancels physical request")
        .expect("server observes disconnect");
    let suspended = session.snapshot().expect("suspended snapshot");
    assert_eq!(
        slots(&suspended)[0]
            .item
            .as_ref()
            .expect("retained item")
            .id,
        OLD_ID
    );

    session.resume().expect("resume");
    next_request(&mut requests)
        .await
        .reply
        .send(page(0, 1, 2, false))
        .expect("resumed response");
    let resumed = wait_until(&session, |snapshot| !snapshot.refreshing).await;
    assert_eq!(
        slots(&resumed)[0].item.as_ref().expect("resumed item").id,
        NEW_ID
    );
}

#[tokio::test]
async fn confirmed_user_data_cancels_older_refresh_and_updates_retained_content() {
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &server));
    let sdk = Arc::new(sdk);
    let session = open_ready(&sdk, &mut requests).await;
    session.refresh().expect("refresh before mutation");
    let stale_read = next_request(&mut requests).await;
    let mutation = {
        let sdk = Arc::clone(&sdk);
        let token = sdk.new_operation_token().expect("mutation token");
        tokio::spawn(async move {
            sdk.update_user_data(token, OLD_ID.to_owned(), VideoUserDataAction::Favorite)
                .await
        })
    };
    next_request(&mut requests)
        .await
        .reply
        .send(r#"{"Key":"old0","IsFavorite":true,"Played":false}"#.to_owned())
        .expect("confirmed mutation response");
    mutation
        .await
        .expect("mutation task")
        .expect("confirmed mutation");

    let confirmed = session.snapshot().expect("confirmed snapshot");
    assert!(
        slots(&confirmed)[0]
            .item
            .as_ref()
            .expect("retained item")
            .favorite
    );
    tokio::time::timeout(Duration::from_secs(10), stale_read.disconnected)
        .await
        .expect("pre-mutation read is cancelled")
        .expect("server observes disconnect");
    next_request(&mut requests)
        .await
        .reply
        .send(page(0, 1, 1, true))
        .expect("reconciled refresh");
    let reconciled = wait_until(&session, |snapshot| !snapshot.refreshing).await;
    assert!(
        slots(&reconciled)[0]
            .item
            .as_ref()
            .expect("reconciled item")
            .favorite
    );
}

#[tokio::test]
async fn far_viewport_delivers_sparse_absolute_slots_instead_of_the_whole_library() {
    let total = 200_000;
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &server));
    let session = open(&sdk);
    next_request(&mut requests)
        .await
        .reply
        .send(page(0, total, 0, false))
        .expect("initial page");
    wait_until(&session, |snapshot| {
        matches!(snapshot.view, LibraryBrowseView::Ready { .. })
    })
    .await;

    session
        .set_display_range(10_000, 10_012)
        .expect("move viewport");
    let sparse = session.snapshot().expect("sparse snapshot");
    assert!(matches!(
        sparse.view,
        LibraryBrowseView::Ready {
            visible_start: 10_000,
            total_record_count: 200_000,
            ..
        }
    ));
    assert_eq!(slots(&sparse).len(), 12);
    assert!(slots(&sparse).iter().all(|slot| slot.item.is_none()));

    let loaded = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = session.snapshot().expect("active viewport");
            if slots(&snapshot).iter().all(|slot| slot.item.is_some()) {
                break snapshot;
            }
            tokio::select! {
                request = requests.recv() => {
                    let request = request.expect("paging server");
                    let start = start_index(&request);
                    // A queued request can have been cancelled by the viewport jump.
                    let _ = request.reply.send(page(start, total, 0, false));
                }
                result = session.next_snapshot(snapshot.revision) => { result.expect("viewport settlement"); }
            }
        }
    }).await.expect("far viewport loads");
    let ids: Vec<_> = slots(&loaded)
        .iter()
        .map(|slot| slot.item.as_ref().expect("loaded slot").id.as_str())
        .collect();
    let expected: Vec<_> = (10_000..10_012)
        .map(|index| format!("{index:032x}"))
        .collect();
    assert_eq!(ids, expected);
}

#[tokio::test]
async fn unpolled_mutation_delivery_cannot_overwrite_a_newer_confirmation() {
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let dir = tempfile::tempdir().expect("storage");
    // Both SDK workers and the observer use the controlled single-thread
    // runtime: publication and acknowledgment finish before the observer wakes.
    let sdk = Arc::new(Sdk::with_handle(
        crate::SdkConfig {
            storage_dir: dir.path().to_owned(),
            device_name: "sdk-test".to_owned(),
        },
        jellypilot_auth::AuthStore::with_credential(Arc::new(super::MemoryCredential::default())),
        None,
        tokio::runtime::Handle::current(),
    ));
    sdk.adopt_test_session(test_session("ada", &server));
    let session = open_ready(&sdk, &mut requests).await;
    session
        .suspend()
        .expect("retain browser while detail writes run");
    let first = sdk.update_user_data(
        sdk.new_operation_token().expect("first token"),
        OLD_ID.to_owned(),
        VideoUserDataAction::Favorite,
    );
    tokio::pin!(first);
    let request = tokio::select! {
        result = &mut first => panic!("server has not replied: {result:?}"),
        request = next_request(&mut requests) => request,
    };
    request
        .reply
        .send(r#"{"Key":"old0","IsFavorite":true,"Played":false}"#.to_owned())
        .expect("first confirmation");
    // Do not poll `first` again until the opposite write has been delivered.
    wait_until(&session, |snapshot| {
        slots(snapshot)[0].item.as_ref().expect("item").favorite
    })
    .await;
    let second = {
        let sdk = Arc::clone(&sdk);
        tokio::spawn(async move {
            sdk.update_user_data(
                sdk.new_operation_token().expect("second token"),
                OLD_ID.to_owned(),
                VideoUserDataAction::Unfavorite,
            )
            .await
        })
    };
    next_request(&mut requests)
        .await
        .reply
        .send(r#"{"Key":"old0","IsFavorite":false,"Played":false}"#.to_owned())
        .expect("second confirmation");
    assert!(
        !second
            .await
            .expect("second task")
            .expect("second mutation")
            .favorite
    );
    assert!(first.await.expect("late first delivery").favorite);
    let final_snapshot = session.snapshot().expect("final snapshot");
    assert!(
        !slots(&final_snapshot)[0]
            .item
            .as_ref()
            .expect("latest item")
            .favorite
    );
}

#[tokio::test]
async fn cancelled_unpolled_mutation_does_not_publish_confirmed_flags() {
    let (server, mut requests, _server_task) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &server));
    let session = open_ready(&sdk, &mut requests).await;
    session.suspend().expect("retain browser");
    let token = sdk.new_operation_token().expect("mutation token");
    let mutation = sdk.update_user_data(
        Arc::clone(&token),
        OLD_ID.to_owned(),
        VideoUserDataAction::Favorite,
    );
    tokio::pin!(mutation);
    let request = tokio::select! {
        result = &mut mutation => panic!("server has not replied: {result:?}"),
        request = next_request(&mut requests) => request,
    };
    token.cancel();
    request
        .reply
        .send(r#"{"Key":"old0","IsFavorite":true,"Played":false}"#.to_owned())
        .expect("server accepted cancelled write");
    // Coordinate on actual worker completion while deliberately leaving the
    // outer future unpolled; do not use cancellation delivery as the barrier.
    tokio::time::timeout(Duration::from_secs(10), async {
        while sdk.inner.item_actions.pending(OLD_ID).is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("worker settles admission");
    let snapshot = session.snapshot().expect("retained snapshot");
    assert!(
        !slots(&snapshot)[0]
            .item
            .as_ref()
            .expect("retained item")
            .favorite
    );
    assert_eq!(
        mutation.await.expect_err("cancelled delivery"),
        SdkError::Cancelled
    );
}
