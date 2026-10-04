use super::*;
use iced::futures::StreamExt;
use jellypilot_media_server::{MediaServerProvider, SavedSession};
use std::time::Duration;

fn connected_state() -> State {
  let mut state = crate::app::update::tests::test_state();
  state.kernel.sdk.adopt_test_session(SavedSession {
    provider: MediaServerProvider::Jellyfin,
    server_url: "https://saved-handoff.example.test".into(),
    user_id: "saved-handoff-user".into(),
    user_name: "Saved handoff user".into(),
    access_token: "test-token".into(),
    server_name: None,
    device_id: None,
  });
  let profile = state
    .kernel
    .sdk
    .active_profile()
    .expect("active test profile");
  crate::app::accounts::sync_activated(&mut state.kernel, &profile);
  state.shell.destination = Destination::SavedBrowse;
  state
}

async fn output(task: Task<AppMessage>) -> AppMessage {
  let mut stream = iced_runtime::task::into_stream(task).expect("accepted saved-filter task");
  while let Some(action) = stream.next().await {
    if let iced_runtime::Action::Output(message) = action {
      return message;
    }
  }
  panic!("saved-filter task completed without a result");
}

async fn reject_handoff_with_queued_completion(state: &mut State, completion: AppMessage) {
  let sdk = Arc::clone(&state.kernel.sdk);
  let previous_profile = sdk.active_profile().expect("active profile").key;
  let previous_session = state.kernel.request_gate.current_session();
  let previous_token = state
    .saved_browse
    .token
    .as_ref()
    .expect("pending request")
    .clone();
  let disconnect = sdk.disconnect();
  tokio::pin!(disconnect);
  let hook = tokio::select! {
    _ = &mut disconnect => panic!("handoff must wait for physical teardown"),
    request = async { state.kernel.sdk_handoff.receiver.lock().await.recv().await } => request.expect("handoff hook"),
  };
  assert!(sdk.content_mutations_blocked());

  // The result was already queued when the account transition blocked writes.
  // Run the real app dispatch/reconcile path, then decline physical teardown.
  drop(crate::app::update::update(state, completion));
  assert!(state.saved_browse.pending.is_none());
  assert!(previous_token.is_cancelled());
  drop(hook);
  assert!(disconnect.await.is_err());
  assert!(!sdk.content_mutations_blocked());
  assert_eq!(sdk.active_profile().unwrap().key, previous_profile);
  assert_eq!(
    state.kernel.request_gate.current_session(),
    previous_session
  );
}

#[tokio::test]
async fn interrupted_initial_load_can_retry_after_failed_handoff() {
  tokio::time::timeout(Duration::from_secs(5), async {
    let mut state = connected_state();
    let completion = output(open(&mut state)).await;
    assert_eq!(state.saved_browse.pending, Some(PendingAction::Load));
    assert!(!state.saved_browse.loaded);

    reject_handoff_with_queued_completion(&mut state, completion).await;
    assert!(!state.saved_browse.loaded);
    assert!(
      state.saved_browse.error.is_some(),
      "the initial load must expose Retry"
    );
    let receipt = target(&state);
    let retry = update(
      &mut state,
      Message::Action {
        target: receipt,
        action: Action::Reload,
      },
    );
    let completion = output(retry).await;
    drop(crate::app::update::update(&mut state, completion));
    assert!(state.saved_browse.loaded);
    assert!(state.saved_browse.pending.is_none());
    assert!(state.saved_browse.error.is_none());
  })
  .await
  .expect("handoff and retry finish without sleeps");
}

#[tokio::test]
async fn interrupted_rename_preserves_draft_records_and_applied_query_for_retry() {
  tokio::time::timeout(Duration::from_secs(5), async {
    let mut state = connected_state();
    let preferences = BrowsePreferences {
      played_filter: VideoLibraryPlayedFilter::Played,
      filters: jellypilot_media_server::VideoLibraryFilters {
        genre: Some("Genre absent from current facets".into()),
        ..Default::default()
      },
      ..Default::default()
    };
    let record = state
      .kernel
      .sdk
      .saved_browse_save(
        state.kernel.sdk.new_operation_token().unwrap(),
        SavedBrowseDraft {
          name: "Original".into(),
          library_id: "movies".into(),
          library_name: "Movies".into(),
          collection_type: VideoLibraryKind::Movies,
          preferences: preferences.clone(),
        },
      )
      .await
      .unwrap();
    state.saved_browse.loaded = true;
    state.saved_browse.records = vec![record.clone()];
    state.full.as_mut().unwrap().browse.saved = Some(AppliedFilter {
      filter: record.clone(),
      deleted: false,
    });
    state.full.as_mut().unwrap().browse.advanced_filters = preferences.filters.clone();
    let receipt = target(&state);
    drop(update(
      &mut state,
      Message::Action {
        target: receipt,
        action: Action::Record {
          id: record.id,
          action: RecordAction::Rename,
        },
      },
    ));
    let receipt = target(&state);
    drop(update(
      &mut state,
      Message::Action {
        target: receipt,
        action: Action::NameChanged("After rollback".into()),
      },
    ));
    let receipt = target(&state);
    let completion = output(update(
      &mut state,
      Message::Action {
        target: receipt,
        action: Action::Submit,
      },
    ))
    .await;
    reject_handoff_with_queued_completion(&mut state, completion).await;

    let editor = state
      .saved_browse
      .editor
      .as_ref()
      .expect("unsaved draft retained");
    assert_eq!(editor.name, "After rollback");
    assert!(editor.error.is_some());
    assert_eq!(state.saved_browse.records, vec![record.clone()]);
    let applied = state.full.as_ref().unwrap().browse.saved.as_ref().unwrap();
    assert_eq!(applied.filter.preferences, preferences);
    assert!(!applied.deleted);
    assert_eq!(
      state.full.as_ref().unwrap().browse.advanced_filters,
      preferences.filters
    );

    let receipt = target(&state);
    let completion = output(update(
      &mut state,
      Message::Action {
        target: receipt,
        action: Action::Submit,
      },
    ))
    .await;
    drop(crate::app::update::update(&mut state, completion));
    assert!(state.saved_browse.editor.is_none());
    assert!(state.saved_browse.pending.is_none());
    assert_eq!(state.saved_browse.records[0].id, record.id);
    assert_eq!(state.saved_browse.records[0].name, "After rollback");
    assert_eq!(state.saved_browse.records[0].preferences, preferences);
  })
  .await
  .expect("handoff and rename retry finish without sleeps");
}
