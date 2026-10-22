use super::*;
use crate::app::message::BrowseMessage;
use crate::app::test_support::{activate_sdk_fixture, BrowseFixture, FixtureReply};
use crate::app::{browse, shell};
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_core::config::UiMode;
use jellypilot_media_server::{VideoLibraryFilters, VideoLibraryShortcut};
use jellypilot_sdk::browse::BrowseSession;

const LIBRARY: &str = "00000000000000000000000000000001";

pub(super) fn state(fixture: &BrowseFixture) -> State {
  let mut state = crate::app::update::tests::test_state();
  activate_sdk_fixture(&mut state.kernel, fixture);
  state.full.as_mut().unwrap().home.data.shortcuts =
    jellypilot_core::LoadState::Ready(vec![library()]);
  state
}

fn library() -> VideoLibraryShortcut {
  VideoLibraryShortcut {
    id: LIBRARY.into(),
    name: "Cinema".into(),
    collection_type: "movies".into(),
    artwork_image_id: None,
    item_count: None,
  }
}

fn preferences() -> BrowsePreferences {
  BrowsePreferences {
    sort: VideoLibrarySort::ReleaseDate,
    sort_direction: VideoLibrarySortDirection::Descending,
    played_filter: VideoLibraryPlayedFilter::Played,
    favorites_only: true,
    filters: VideoLibraryFilters {
      quality: Some(VideoLibraryQuality::Uhd),
      country: Some("Retained country".into()),
      genre: Some("Retained genre".into()),
    },
  }
}

pub(super) async fn save(
  state: &State,
  name: &str,
  preferences: BrowsePreferences,
) -> SavedBrowseFilter {
  state
    .kernel
    .sdk
    .saved_browse_save(
      state.kernel.sdk.new_operation_token().unwrap(),
      SavedBrowseDraft {
        name: name.into(),
        library_id: LIBRARY.into(),
        collection_type: VideoLibraryKind::Movies,
        library_name: "Cinema".into(),
        preferences,
      },
    )
    .await
    .unwrap()
}

fn action(state: &mut State, action: Action) -> Task<AppMessage> {
  update(
    state,
    Message::Action {
      target: target(state),
      action,
    },
  )
}

fn settle(state: &mut State, message: AppMessage) -> Task<AppMessage> {
  let task = match message {
    AppMessage::SavedBrowse(message) => update(state, message),
    AppMessage::Browse(message) => {
      let full = state.full.as_mut().unwrap();
      browse::update(
        &mut full.browse,
        &mut state.kernel,
        true,
        state.shell.window_size,
        message,
      )
    }
    _ => Task::none(),
  };
  reconcile(state);
  task
}

pub(super) fn session(state: &State) -> Arc<BrowseSession> {
  state.full.as_ref().unwrap().browse.session.clone().unwrap()
}

pub(super) fn sync(state: &mut State) {
  drop(browse::sync_snapshot(
    &mut state.full.as_mut().unwrap().browse,
    &mut state.kernel,
  ));
}

fn directory() -> FixtureReply {
  FixtureReply::Json(
    serde_json::json!({"Items":[{
      "Id":LIBRARY,"Name":"Fresh Cinema","CollectionType":"movies"
    }]})
    .to_string(),
  )
}

async fn finish_query(state: &mut State, fixture: &mut BrowseFixture) {
  let session = session(state);
  let snapshot = fixture
    .settle_session(&session, |_| FixtureReply::Page {
      total: 1,
      artwork: false,
    })
    .await;
  drop(settle(
    state,
    AppMessage::Browse(BrowseMessage::SnapshotChanged {
      session_id: session.id(),
      snapshot: Box::new(snapshot),
    }),
  ));
}

pub(super) async fn apply(state: &mut State, fixture: &mut BrowseFixture, id: SavedBrowseId) {
  let load = action(state, Action::Reload);
  for message in fixture
    .run_task(load, |_| FixtureReply::Page {
      total: 1,
      artwork: false,
    })
    .await
  {
    drop(settle(state, message));
  }
  let task = action(
    state,
    Action::Record {
      id,
      action: RecordAction::Apply,
    },
  );
  for message in fixture
    .run_task(task, |request| {
      if request.target().starts_with("/UserViews?") {
        directory()
      } else {
        FixtureReply::Page {
          total: 1,
          artwork: false,
        }
      }
    })
    .await
  {
    drop(settle(state, message));
  }
  finish_query(state, fixture).await;
}

fn ordinary(state: &mut State) {
  drop(shell::navigate(
    state,
    Destination::Library {
      library_id: LIBRARY.into(),
      collection_type: "movies".into(),
    },
  ));
}

#[tokio::test]
async fn apply_resolves_fresh_directory_then_installs_one_complete_query_and_rejects_old_snapshot()
{
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  let filter = save(&state, "Everything", preferences()).await;
  ordinary(&mut state);
  let old = session(&state);
  let old_snapshot = old.snapshot().unwrap();
  let defaults = old_snapshot.preferences.clone();
  let old_request = fixture.receive_request().await;
  let load = shell::navigate(&mut state, Destination::SavedBrowse);
  for message in fixture
    .run_task(load, |_| panic!("definition loading is local"))
    .await
  {
    drop(settle(&mut state, message));
  }
  let task = action(
    &mut state,
    Action::Record {
      id: filter.id,
      action: RecordAction::Apply,
    },
  );
  let mut directories = 0;
  let mut pages = 0;
  let mut respond = |request: &crate::app::test_support::FixtureRequest| {
    if request.target().starts_with("/UserViews?") {
      directories += 1;
      directory()
    } else {
      assert!(request.target().contains("/Items?"));
      pages += 1;
      FixtureReply::Page {
        total: 1,
        artwork: false,
      }
    }
  };
  let messages = fixture.run_task(task, &mut respond).await;
  assert_eq!(state.shell.destination, Destination::SavedBrowse);
  for message in messages {
    drop(settle(&mut state, message));
  }
  assert_eq!(current_preferences(&state), Some(preferences()));
  assert!(!modified(&state));
  let current = session(&state);
  let snapshot = fixture.settle_session(&current, &mut respond).await;
  assert_eq!(directories, 1);
  assert_eq!(
    pages, 1,
    "one complete initial query, never an intermediate default query"
  );
  drop(settle(
    &mut state,
    AppMessage::Browse(BrowseMessage::SnapshotChanged {
      session_id: current.id(),
      snapshot: Box::new(snapshot),
    }),
  ));
  assert!(
    matches!(
      state.full.as_ref().unwrap().browse.view,
      LibraryBrowseView::Empty
    ),
    "unmatched saved advanced conditions must not broaden into default results"
  );
  old_request.reply(FixtureReply::Page {
    total: 20,
    artwork: false,
  });
  drop(settle(
    &mut state,
    AppMessage::Browse(BrowseMessage::SnapshotChanged {
      session_id: old.id(),
      snapshot: Box::new(old_snapshot),
    }),
  ));
  assert_eq!(session(&state).id(), current.id());
  assert!(matches!(
    state.full.as_ref().unwrap().browse.view,
    LibraryBrowseView::Empty
  ));
  ordinary(&mut state);
  assert_eq!(current_preferences(&state), Some(defaults));
}

#[tokio::test]
async fn apply_failure_keeps_route_and_complete_context_and_can_retry() {
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  let filter = save(&state, "Unavailable", preferences()).await;
  apply(&mut state, &mut fixture, filter.id).await;
  state.saved_browse.records = vec![filter.clone()];
  let previous = session(&state).id();
  for response in [
    FixtureReply::Failure,
    FixtureReply::Json("{\"Items\":[]}".into()),
  ] {
    let task = action(
      &mut state,
      Action::Record {
        id: filter.id,
        action: RecordAction::Apply,
      },
    );
    for message in fixture.run_task(task, |_| response.clone()).await {
      drop(settle(&mut state, message));
    }
    assert!(state.saved_browse.pending.is_none());
    assert!(state.saved_browse.error.is_some());
    assert_eq!(current_preferences(&state), Some(preferences()));
    assert_eq!(session(&state).id(), previous);
    assert_eq!(state.saved_browse.records, vec![filter.clone()]);
    assert!(!session(&state).snapshot().unwrap().saved.unwrap().deleted);
  }
  apply(&mut state, &mut fixture, filter.id).await;
  assert_ne!(session(&state).id(), previous);
  assert!(state.saved_browse.error.is_none());
}

#[tokio::test]
async fn saved_context_survives_detail_modes_and_deleted_definition_until_ordinary_navigation() {
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  ordinary(&mut state);
  let defaults = current_preferences(&state).unwrap();
  finish_query(&mut state, &mut fixture).await;
  let filter = save(&state, "Retained", preferences()).await;
  apply(&mut state, &mut fixture, filter.id).await;
  let owner = session(&state).id();
  for mode in [UiMode::Tv, UiMode::Desktop] {
    drop(shell::apply_ui_mode(&mut state, mode));
    assert_eq!(current_preferences(&state), Some(preferences()));
    drop(shell::navigate(
      &mut state,
      Destination::Detail("another-item".into()),
    ));
    drop(shell::apply_ui_mode(
      &mut state,
      if mode == UiMode::Tv {
        UiMode::Desktop
      } else {
        UiMode::Tv
      },
    ));
    drop(shell::navigate_back(&mut state));
    assert_eq!(session(&state).id(), owner);
    assert_eq!(current_preferences(&state), Some(preferences()));
  }
  state
    .kernel
    .sdk
    .saved_browse_delete(state.kernel.sdk.new_operation_token().unwrap(), filter.id)
    .await
    .unwrap();
  sync(&mut state);
  assert!(session(&state).snapshot().unwrap().saved.unwrap().deleted);
  drop(shell::navigate(
    &mut state,
    Destination::Detail("another-item".into()),
  ));
  drop(shell::navigate_back(&mut state));
  assert_eq!(current_preferences(&state), Some(preferences()));
  assert!(session(&state).snapshot().unwrap().saved.unwrap().deleted);
  ordinary(&mut state);
  assert!(session(&state).snapshot().unwrap().saved.is_none());
  assert_eq!(current_preferences(&state), Some(defaults));
}

#[tokio::test]
async fn clearing_ordinary_conditions_changes_real_query_and_defaults_but_saved_clear_does_not() {
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  ordinary(&mut state);
  finish_query(&mut state, &mut fixture).await;
  session(&state).set_preferences(preferences()).unwrap();
  sync(&mut state);
  finish_query(&mut state, &mut fixture).await;
  let queued = session(&state).snapshot().unwrap();
  let before = queued.identity.clone();
  drop(action(&mut state, Action::ClearConditions));
  finish_query(&mut state, &mut fixture).await;
  let session_id = session(&state).id();
  drop(settle(
    &mut state,
    AppMessage::Browse(BrowseMessage::SnapshotChanged {
      session_id,
      snapshot: Box::new(queued),
    }),
  ));
  assert!(matches!(
    state.full.as_ref().unwrap().browse.view,
    LibraryBrowseView::Ready { .. }
  ));
  assert_ne!(session(&state).snapshot().unwrap().identity, before);
  assert_eq!(
    current_preferences(&state),
    Some(BrowsePreferences::default())
  );

  let ordinary_defaults = BrowsePreferences {
    sort: VideoLibrarySort::RecentlyAdded,
    favorites_only: true,
    ..Default::default()
  };
  session(&state)
    .set_preferences(ordinary_defaults.clone())
    .unwrap();
  sync(&mut state);
  finish_query(&mut state, &mut fixture).await;
  let filter = save(&state, "Default", BrowsePreferences::default()).await;
  apply(&mut state, &mut fixture, filter.id).await;
  session(&state).set_preferences(preferences()).unwrap();
  sync(&mut state);
  assert!(modified(&state));
  drop(action(&mut state, Action::ClearConditions));
  assert!(
    !modified(&state),
    "reverting conditions restores the saved baseline"
  );
  assert_eq!(
    state
      .kernel
      .sdk
      .saved_browse_list(state.kernel.sdk.new_operation_token().unwrap())
      .await
      .unwrap(),
    vec![filter]
  );
  ordinary(&mut state);
  assert_eq!(current_preferences(&state), Some(ordinary_defaults));
}

#[tokio::test]
async fn failed_rename_preserves_draft_and_context_and_stale_editor_or_profile_actions_do_nothing()
{
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  let filter = save(&state, "Keep", preferences()).await;
  let other = save(&state, "Duplicate", BrowsePreferences::default()).await;
  apply(&mut state, &mut fixture, filter.id).await;
  state.saved_browse.records = vec![filter.clone(), other];
  let identity = session(&state).snapshot().unwrap().identity;
  drop(action(
    &mut state,
    Action::Record {
      id: filter.id,
      action: RecordAction::Rename,
    },
  ));
  drop(action(&mut state, Action::NameChanged("duplicate".into())));
  let submit = action(&mut state, Action::Submit);
  for message in fixture
    .run_task(submit, |_| panic!("rename is local"))
    .await
  {
    drop(settle(&mut state, message));
  }
  let editor = state.saved_browse.editor.as_ref().unwrap();
  assert_eq!(editor.name, "duplicate");
  assert_eq!(
    editor.error.as_ref().unwrap().id(),
    "saved-filters-duplicate-name"
  );
  assert_eq!(state.saved_browse.records[0], filter);
  assert_eq!(session(&state).snapshot().unwrap().identity, identity);
  let stale = target(&state);
  drop(action(&mut state, Action::CloseEditor));
  drop(action(
    &mut state,
    Action::Record {
      id: filter.id,
      action: RecordAction::Rename,
    },
  ));
  drop(update(
    &mut state,
    Message::Action {
      target: stale,
      action: Action::NameChanged("old".into()),
    },
  ));
  assert_eq!(state.saved_browse.editor.as_ref().unwrap().name, "Keep");
  let old_profile = target(&state);
  state.kernel.request_gate.disconnect();
  drop(update(
    &mut state,
    Message::Action {
      target: old_profile,
      action: Action::Submit,
    },
  ));
  assert!(state.saved_browse.pending.is_none());
}
