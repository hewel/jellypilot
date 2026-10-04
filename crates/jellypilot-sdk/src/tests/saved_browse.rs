use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use jellypilot_auth::AuthStore;
use jellypilot_core::browse_model::BrowsePreferences;
use jellypilot_media_server::{
    VideoLibraryFilters, VideoLibraryKind, VideoLibraryPlayedFilter, VideoLibraryQuality,
    VideoLibrarySort, VideoLibrarySortDirection,
};

use super::{
    controlled_http_server, test_candidate, test_sdk, test_session, HttpRequest, MemoryCredential,
};
use crate::saved_browse::{SavedBrowseDraft, SavedBrowseError};
use crate::{Sdk, SdkConfig, SdkError, SdkHooks};

fn library_id() -> String {
    "00000000000000000000000000000001".to_owned()
}

fn draft(name: &str) -> SavedBrowseDraft {
    SavedBrowseDraft {
        name: name.to_owned(),
        library_id: library_id(),
        collection_type: VideoLibraryKind::Movies,
        library_name: "Original name".to_owned(),
        preferences: BrowsePreferences {
            sort: VideoLibrarySort::ReleaseDate,
            sort_direction: VideoLibrarySortDirection::Descending,
            played_filter: VideoLibraryPlayedFilter::Unplayed,
            favorites_only: true,
            filters: VideoLibraryFilters {
                quality: Some(VideoLibraryQuality::DolbyVision),
                country: Some("Old country".into()),
                genre: Some("Old genre".into()),
            },
        },
    }
}

fn directory(id: &str, kind: &str) -> String {
    serde_json::json!({"Items": [{"Id": id, "Name": "Renamed library", "CollectionType": kind, "Type": "CollectionFolder"}]}).to_string()
}

async fn request(
    requests: &mut tokio::sync::mpsc::UnboundedReceiver<HttpRequest<String>>,
) -> HttpRequest<String> {
    let request = tokio::time::timeout(Duration::from_secs(5), requests.recv())
        .await
        .expect("directory request arrives")
        .expect("fixture alive");
    assert!(
        request.headers.starts_with("GET /UserViews?"),
        "Apply must refresh the real directory: {}",
        request.headers.lines().next().unwrap_or_default()
    );
    request
}

#[tokio::test]
async fn scoped_local_crud_survives_profile_round_trip_and_rejects_foreign_tokens() {
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let old_token = sdk.new_operation_token().unwrap();
    let first = sdk
        .saved_browse_save(Arc::clone(&old_token), draft(" Cinema "))
        .await
        .unwrap();
    let renamed = sdk
        .saved_browse_rename(Arc::clone(&old_token), first.id, "Watch later".into())
        .await
        .unwrap();
    assert_eq!(renamed.id, first.id);
    sdk.adopt_test_session(test_session("grace", "https://media.example.test"));
    let second = sdk
        .saved_browse_save(sdk.new_operation_token().unwrap(), draft("Watch later"))
        .await
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(
        sdk.saved_browse_delete(old_token, first.id).await,
        Err(SavedBrowseError::Sdk(SdkError::Stale))
    );
    assert_eq!(
        sdk.saved_browse_list(sdk.new_operation_token().unwrap())
            .await
            .unwrap(),
        vec![second]
    );
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    assert_eq!(
        sdk.saved_browse_list(sdk.new_operation_token().unwrap())
            .await
            .unwrap(),
        vec![renamed]
    );
    let (other, _other_dir) = test_sdk();
    other.adopt_test_session(test_session("ada", "https://media.example.test"));
    other.adopt_test_session(test_session("grace", "https://media.example.test"));
    other.adopt_test_session(test_session("ada", "https://media.example.test"));
    let foreign = sdk.new_operation_token().unwrap();
    assert_eq!(
        foreign.scope_ref().unwrap(),
        other.new_operation_token().unwrap().scope_ref().unwrap()
    );
    assert!(matches!(
        other.saved_browse_save(foreign, draft("foreign")).await,
        Err(SavedBrowseError::Sdk(SdkError::Stale))
    ));
}

#[tokio::test]
async fn apply_always_refreshes_exact_library_and_preserves_unlisted_conditions() {
    let (url, mut requests, _server) = controlled_http_server().await;
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let filter = sdk
        .saved_browse_save(sdk.new_operation_token().unwrap(), draft("Cinema"))
        .await
        .unwrap();
    for response in [
        directory(&library_id(), "movies"),
        directory(&library_id(), "tvshows"),
        directory("00000000000000000000000000000002", "movies"),
    ] {
        let success = response == directory(&library_id(), "movies");
        let (resolved, ()) = tokio::join!(
            sdk.saved_browse_resolve_for_apply(sdk.new_operation_token().unwrap(), filter.id),
            async {
                request(&mut requests).await.reply.send(response).unwrap();
            },
        );
        if success {
            let resolved = resolved.unwrap();
            assert_eq!(resolved.filter, filter);
            assert_eq!(resolved.library.name, "Renamed library");
            assert_eq!(
                resolved.filter.preferences.filters.genre.as_deref(),
                Some("Old genre")
            );
        } else {
            assert!(matches!(
                resolved,
                Err(SavedBrowseError::LibraryUnavailable)
            ));
        }
    }
    let (failed, ()) = tokio::join!(
        sdk.saved_browse_resolve_for_apply(sdk.new_operation_token().unwrap(), filter.id),
        async {
            request(&mut requests)
                .await
                .reply
                .send("not a directory response".to_owned())
                .unwrap();
        },
    );
    assert!(matches!(
        failed,
        Err(SavedBrowseError::Sdk(SdkError::Request(_)))
    ));
    assert_eq!(
        sdk.saved_browse_list(sdk.new_operation_token().unwrap())
            .await
            .unwrap(),
        vec![filter]
    );
}

#[tokio::test]
async fn apply_retires_directory_results_after_scope_change_or_explicit_cancel() {
    for switch_scope in [false, true] {
        let (url, mut requests, _server) = controlled_http_server().await;
        let (sdk, _dir) = test_sdk();
        sdk.adopt_test_session(test_session("ada", &url));
        let filter = sdk
            .saved_browse_save(sdk.new_operation_token().unwrap(), draft("Cinema"))
            .await
            .unwrap();
        let token = sdk.new_operation_token().unwrap();
        let (result, ()) = tokio::join!(
            sdk.saved_browse_resolve_for_apply(Arc::clone(&token), filter.id),
            async {
                let request = request(&mut requests).await;
                if switch_scope {
                    sdk.adopt_test_session(test_session("grace", &url));
                } else {
                    token.cancel();
                }
                let _ = request.reply.send(directory(&library_id(), "movies"));
            },
        );
        assert!(matches!(
            result,
            Err(SavedBrowseError::Sdk(SdkError::Cancelled | SdkError::Stale))
        ));
    }
}

#[tokio::test]
async fn disconnected_directory_remains_a_retryable_request_failure() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let (sdk, _dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", &url));
    let filter = sdk
        .saved_browse_save(sdk.new_operation_token().unwrap(), draft("Offline library"))
        .await
        .unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        sdk.saved_browse_resolve_for_apply(sdk.new_operation_token().unwrap(), filter.id),
    )
    .await
    .expect("connection refusal is bounded");
    assert!(matches!(
        result,
        Err(SavedBrowseError::Sdk(SdkError::Request(_)))
    ));
    assert_eq!(
        sdk.saved_browse_list(sdk.new_operation_token().unwrap())
            .await
            .unwrap(),
        vec![filter]
    );
}

#[tokio::test]
async fn rename_or_delete_during_apply_does_not_deliver_an_older_definition() {
    for delete in [false, true] {
        let (url, mut requests, _server) = controlled_http_server().await;
        let (sdk, _dir) = test_sdk();
        sdk.adopt_test_session(test_session("ada", &url));
        let filter = sdk
            .saved_browse_save(sdk.new_operation_token().unwrap(), draft("Cinema"))
            .await
            .unwrap();
        let (result, ()) = tokio::join!(
            sdk.saved_browse_resolve_for_apply(sdk.new_operation_token().unwrap(), filter.id),
            async {
                let request = request(&mut requests).await;
                if delete {
                    assert!(sdk
                        .saved_browse_delete(sdk.new_operation_token().unwrap(), filter.id)
                        .await
                        .unwrap());
                    let replacement = sdk
                        .saved_browse_save(sdk.new_operation_token().unwrap(), draft("Cinema"))
                        .await
                        .unwrap();
                    assert_ne!(replacement.id, filter.id);
                } else {
                    sdk.saved_browse_rename(
                        sdk.new_operation_token().unwrap(),
                        filter.id,
                        "Renamed".into(),
                    )
                    .await
                    .unwrap();
                }
                request
                    .reply
                    .send(directory(&library_id(), "movies"))
                    .unwrap();
            },
        );
        if delete {
            assert!(matches!(result, Err(SavedBrowseError::Missing)));
        } else {
            assert!(matches!(
                result,
                Err(SavedBrowseError::Sdk(SdkError::Stale))
            ));
        }
    }
}

#[tokio::test]
async fn storage_failure_keeps_definition_and_retry_input_reusable() {
    let (sdk, dir) = test_sdk();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let saved = sdk
        .saved_browse_save(sdk.new_operation_token().unwrap(), draft("Cinema"))
        .await
        .unwrap();
    let before = std::fs::read(dir.path().join("saved-browse.json")).unwrap();
    let temporary = dir.path().join("saved-browse.json.tmp");
    std::fs::create_dir(&temporary).unwrap();
    assert!(matches!(
        sdk.saved_browse_rename(
            sdk.new_operation_token().unwrap(),
            saved.id,
            "Retry this name".into()
        )
        .await,
        Err(SavedBrowseError::Sdk(SdkError::Storage(_)))
    ));
    assert_eq!(
        std::fs::read(dir.path().join("saved-browse.json")).unwrap(),
        before
    );
    assert_eq!(
        sdk.saved_browse_list(sdk.new_operation_token().unwrap())
            .await
            .unwrap(),
        vec![saved.clone()]
    );
    std::fs::remove_dir(temporary).unwrap();
    assert_eq!(
        sdk.saved_browse_rename(
            sdk.new_operation_token().unwrap(),
            saved.id,
            "Retry this name".into()
        )
        .await
        .unwrap()
        .name,
        "Retry this name"
    );
}

#[test]
fn queued_write_checks_scope_cancellation_and_handoff_at_the_physical_mutation() {
    for retire in ["scope", "cancel", "handoff"] {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let sdk = Sdk::with_handle(
                SdkConfig {
                    storage_dir: dir.path().to_path_buf(),
                    device_name: "queued-saved-filter".into(),
                },
                AuthStore::with_credential(Arc::new(MemoryCredential::default())),
                None,
                runtime.handle().clone(),
            );
            sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
            let token = sdk.new_operation_token().unwrap();
            let (entered, entered_rx) = tokio::sync::oneshot::channel();
            let (release, release_rx) = std::sync::mpsc::channel();
            let blocker = runtime.handle().spawn_blocking(move || {
                let _ = entered.send(());
                let _ = release_rx.recv();
            });
            entered_rx.await.unwrap();
            let mut write = Box::pin(sdk.saved_browse_save(Arc::clone(&token), draft("Queued")));
            std::future::poll_fn(|context| {
                assert!(write.as_mut().poll(context).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            match retire {
                "scope" => {
                    sdk.adopt_test_session(test_session("grace", "https://media.example.test"))
                }
                "cancel" => token.cancel(),
                _ => sdk.inner.state.lock().unwrap().handoff_in_progress = true,
            }
            release.send(()).unwrap();
            blocker.await.unwrap();
            assert!(write.await.is_err());
            assert!(!dir.path().join("saved-browse.json").exists());
        });
    }
}

#[tokio::test]
async fn failed_account_handoff_keeps_previous_filters_and_query_definition() {
    struct Declined;
    #[async_trait::async_trait]
    impl SdkHooks for Declined {
        async fn before_profile_handoff(&self) -> bool {
            false
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let sdk = Sdk::new(
        SdkConfig {
            storage_dir: dir.path().to_path_buf(),
            device_name: "failed-handoff".into(),
        },
        Arc::new(MemoryCredential::default()),
        Some(Arc::new(Declined)),
    )
    .unwrap();
    sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
    let token = sdk.new_operation_token().unwrap();
    let saved = sdk
        .saved_browse_save(Arc::clone(&token), draft("Cinema"))
        .await
        .unwrap();
    let result = sdk
        .activate_candidate(
            test_candidate("grace", "https://other.example.test", dir.path()),
            true,
        )
        .await;
    assert!(matches!(result, Err(SdkError::HandoffAborted)));
    assert_eq!(sdk.saved_browse_list(token).await.unwrap(), vec![saved]);
}
