use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Wake, Waker};
use std::time::Duration;

use super::{BrowseSession, BrowseSnapshot, Browser, SessionState};
use crate::tests::{test_sdk, test_session};
use crate::{ProfileCandidate, Sdk, SdkError};

// No page task exists: only the operation under test can wake the waiter.
// This keeps the real synchronization boundary without a bootstrap race.
fn idle_session(sdk: &Sdk) -> Arc<BrowseSession> {
    let (changed, _) = tokio::sync::watch::channel(());
    Arc::new(BrowseSession {
        state: std::sync::Mutex::new(SessionState {
            browser: Browser::default(),
            revision: 1,
            failure: None,
        }),
        token: sdk.new_operation_token().expect("active scope token"),
        handle: sdk.inner.handle.clone(),
        changed,
        closed: std::sync::atomic::AtomicBool::new(false),
    })
}

struct InlineSnapshotReader {
    session: Arc<BrowseSession>,
    previous_revision: u64,
    snapshots: std::sync::mpsc::Sender<(u64, Result<BrowseSnapshot, SdkError>)>,
}

impl Wake for InlineSnapshotReader {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // UniFFI may resume a Kotlin continuation immediately on the caller's
        // thread. Reading here must not wait for the notifying call to return.
        let _ = self
            .snapshots
            .send((self.previous_revision, self.session.snapshot()));
    }
}

fn inline_snapshot_after(
    operation: impl FnOnce(&Sdk, &Arc<BrowseSession>) + Send + 'static,
) -> (u64, Result<BrowseSnapshot, SdkError>) {
    let (snapshots, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let (sdk, _dir) = test_sdk();
        sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
        let session = idle_session(&sdk);
        let revision = session.snapshot().expect("initial snapshot").revision;
        let waker = Waker::from(Arc::new(InlineSnapshotReader {
            session: Arc::clone(&session),
            previous_revision: revision,
            snapshots,
        }));
        let mut context = Context::from_waker(&waker);
        let mut pending = Box::pin(session.next_snapshot(revision));
        assert!(pending.as_mut().poll(&mut context).is_pending());

        operation(&sdk, &session);

        assert!(pending.as_mut().poll(&mut context).is_ready());
    });
    // A separate OS thread bounds the failure even when reentry blocks a
    // synchronous Rust mutex and no async timeout could be polled.
    let snapshot = received
        .recv_timeout(Duration::from_secs(2))
        .expect("inline snapshot reader must not deadlock the notifying thread");
    worker.join().expect("notifying call completes");
    snapshot
}

#[test]
fn browse_notification_allows_inline_snapshot_reentry() {
    let (before, snapshot) = inline_snapshot_after(|_, session| {
        session.suspend().expect("notify inline reader");
    });
    let snapshot = snapshot.expect("inline snapshot remains active");
    assert_eq!(snapshot.revision, before + 1);
}

#[test]
fn cancelling_scope_operations_allows_inline_cancelled_snapshot() {
    let error = inline_snapshot_after(|sdk, _| sdk.cancel_scope_operations())
        .1
        .expect_err("cancelled browser rejects snapshots");
    assert_eq!(error, SdkError::Cancelled);
}

#[test]
fn disconnect_allows_inline_stale_snapshot() {
    let error = inline_snapshot_after(|sdk, _| {
        sdk.inner
            .handle
            .block_on(sdk.disconnect())
            .expect("disconnect active scope");
    })
    .1
    .expect_err("ended scope rejects snapshots");
    assert_eq!(error, SdkError::Stale);
}

#[test]
fn profile_replacement_allows_inline_stale_snapshot() {
    let error = inline_snapshot_after(|sdk, _| {
        sdk.adopt_test_session(test_session("grace", "https://media.example.test"));
    })
    .1
    .expect_err("replaced scope rejects snapshots");
    assert_eq!(error, SdkError::Stale);
}

#[test]
fn sdk_close_allows_inline_closed_snapshot() {
    let error = inline_snapshot_after(|sdk, _| sdk.close())
        .1
        .expect_err("closed SDK rejects snapshots");
    assert_eq!(error, SdkError::Closed);
}

#[test]
fn activation_allows_inline_stale_snapshot() {
    let error = inline_snapshot_after(|sdk, _| {
        let client = Arc::new(jellypilot_media_server::JellyfinClient::with_storage_dir(
            sdk.inner.config.storage_dir.clone(),
        ));
        client
            .login()
            .adopt_validated_session(&test_session("grace", "https://media.example.test"));
        let candidate =
            jellypilot_auth::login::ValidatedProfileCandidate::from_authenticated_client(client)
                .unwrap_or_else(|_| panic!("fixture session is authenticated"));
        sdk.inner
            .handle
            .block_on(sdk.activate_candidate(ProfileCandidate::new(candidate), false))
            .expect("activate replacement profile");
    })
    .1
    .expect_err("replaced scope rejects snapshots");
    assert_eq!(error, SdkError::Stale);
}

#[test]
fn failure_notification_allows_inline_error_snapshot() {
    let expected = SdkError::Request("fixture failure".to_owned());
    let failure = expected.clone();
    let error = inline_snapshot_after(move |_, session| session.fail(failure))
        .1
        .expect_err("failure remains visible in reentrant snapshot");
    assert_eq!(error, expected);
}

#[test]
fn delayed_notifications_do_not_repeat_an_already_consumed_snapshot() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let session = idle_session(&sdk);
    let before = session.snapshot().expect("initial snapshot").revision;
    session.suspend().expect("commit before subscriber exists");
    let mut context = Context::from_waker(Waker::noop());
    let mut first = Box::pin(session.next_snapshot(before));
    let std::task::Poll::Ready(Ok(snapshot)) = first.as_mut().poll(&mut context) else {
        panic!("committed change must not require another notification");
    };
    drop(first);
    let mut pending = Box::pin(session.next_snapshot(snapshot.revision));
    assert!(pending.as_mut().poll(&mut context).is_pending());
    // An older writer can publish after this latest snapshot was consumed.
    session.changed.send_replace(());
    assert!(pending.as_mut().poll(&mut context).is_pending());
    session.resume().expect("subsequent change");
    assert!(pending.as_mut().poll(&mut context).is_ready());
}
