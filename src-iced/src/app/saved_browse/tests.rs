use super::*;
use crate::app::message::BrowseMessage;
use crate::app::test_support::{page_settled, BrowseFixture, FixtureReply};
use crate::app::{accounts, browse, shell};
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_core::config::UiMode;
use jellypilot_media_server::{
  MediaServerProvider, SavedSession, VideoLibraryFilters, VideoLibraryShortcut,
};

const LIBRARY: &str = "00000000000000000000000000000001";

fn state(fixture: &BrowseFixture) -> State {
  let mut state = crate::app::update::tests::test_state();
  state.kernel.sdk.adopt_test_session(SavedSession {
    provider: MediaServerProvider::Jellyfin,
    server_url: fixture.server_url().into(),
    access_token: "fixture-token".into(),
    user_id: "fixture-user".into(),
    user_name: "Fixture".into(),
    server_name: None,
    device_id: None,
  });
  let profile = state.kernel.sdk.active_profile().unwrap();
  accounts::sync_activated(&mut state.kernel, &profile);
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

async fn save(state: &State, name: &str, preferences: BrowsePreferences) -> SavedBrowseFilter {
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
      let source = shell::browse_source(state);
      let full = state.full.as_mut().unwrap();
      browse::update(
        &mut full.browse,
        &mut state.kernel,
        source,
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

fn resolved(filter: SavedBrowseFilter) -> ResolvedSavedBrowse {
  ResolvedSavedBrowse {
    filter,
    library: library(),
  }
}

#[tokio::test]
async fn apply_resolves_fresh_directory_then_installs_one_complete_query_and_rejects_old_page() {
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  let defaults = state.kernel.settings.snapshot().browse_filters();
  let filter = save(&state, "Everything", preferences()).await;
  let ordinary = shell::navigate(
    &mut state,
    Destination::Library {
      library_id: LIBRARY.into(),
      collection_type: "movies".into(),
    },
  );
  let mut old_stream = iced_runtime::task::into_stream(ordinary).unwrap();
  let old_request = fixture.next_request(&mut old_stream).await;
  let load = shell::navigate(&mut state, Destination::SavedBrowse);
  for message in fixture
    .run_task(load, |_| panic!("loading definitions is local"))
    .await
  {
    drop(settle(&mut state, message));
  }
  let apply = action(
    &mut state,
    Action::Record {
      id: filter.id,
      action: RecordAction::Apply,
    },
  );
  assert_eq!(state.shell.destination, Destination::SavedBrowse);
  let mut directories = 0;
  let messages = fixture.run_task(apply, |request| {
    directories += 1;
    assert!(request.target().starts_with("/UserViews?"));
    FixtureReply::Json(serde_json::json!({"Items":[{"Id":LIBRARY,"Name":"Fresh Cinema","CollectionType":"movies"}]}).to_string())
  }).await;
  assert_eq!(directories, 1);
  assert_eq!(
    state.shell.destination,
    Destination::SavedBrowse,
    "directory delivery does not partially install preferences"
  );
  assert_eq!(messages.len(), 1);
  let query = settle(&mut state, messages.into_iter().next().unwrap());
  assert_eq!(current_preferences(&state), Some(preferences()));
  assert_eq!(state.kernel.settings.snapshot().browse_filters(), defaults);
  assert!(!modified(&state));
  let mut requests = 0;
  let pages = fixture
    .run_task(query, |request| {
      requests += 1;
      assert!(request.target().contains("/Items?"));
      FixtureReply::Page {
        total: 1,
        artwork: false,
      }
    })
    .await;
  assert_eq!(
    requests, 1,
    "one initial Browser query, with no intermediate default query"
  );
  for message in pages {
    drop(settle(&mut state, message));
  }
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
  let stale = fixture
    .run_stream(&mut old_stream, |_| FixtureReply::Failure)
    .await
    .into_iter()
    .find_map(page_settled)
    .unwrap();
  assert!(!state
    .full
    .as_ref()
    .unwrap()
    .browse
    .browser
    .model()
    .is_current_settlement(&stale));
  drop(settle(
    &mut state,
    AppMessage::Browse(BrowseMessage::PageSettled(stale)),
  ));
  assert!(matches!(
    state.full.as_ref().unwrap().browse.view,
    LibraryBrowseView::Empty
  ));
}

#[tokio::test]
async fn apply_failure_keeps_route_and_complete_context_and_can_retry() {
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  let filter = save(&state, "Unavailable", preferences()).await;
  drop(shell::apply_saved_browse(
    &mut state,
    resolved(filter.clone()),
  ));
  state.saved_browse.records = vec![filter.clone()];
  let identity = state
    .full
    .as_ref()
    .unwrap()
    .browse
    .browser
    .model()
    .identity()
    .unwrap()
    .to_owned();
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
    assert_eq!(
      state
        .full
        .as_ref()
        .unwrap()
        .browse
        .browser
        .model()
        .identity(),
      Some(identity.as_str())
    );
    assert_eq!(state.saved_browse.records, vec![filter.clone()]);
    assert!(
      !state
        .full
        .as_ref()
        .unwrap()
        .browse
        .saved
        .as_ref()
        .unwrap()
        .deleted
    );
  }
}

#[tokio::test]
async fn saved_context_survives_detail_modes_and_deleted_definition_until_ordinary_navigation() {
  let fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  let filter = save(&state, "Retained", preferences()).await;
  drop(shell::apply_saved_browse(
    &mut state,
    resolved(filter.clone()),
  ));
  state.saved_browse.loaded = true;
  state.saved_browse.records = vec![filter.clone()];
  let original_defaults = state.kernel.settings.snapshot().browse_filters();
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
    assert_eq!(current_preferences(&state), Some(preferences()));
  }
  state
    .kernel
    .sdk
    .saved_browse_delete(state.kernel.sdk.new_operation_token().unwrap(), filter.id)
    .await
    .unwrap();
  state.saved_browse.records.clear();
  reconcile(&mut state);
  assert!(
    state
      .full
      .as_ref()
      .unwrap()
      .browse
      .saved
      .as_ref()
      .unwrap()
      .deleted
  );
  drop(shell::navigate(
    &mut state,
    Destination::Detail("another-item".into()),
  ));
  drop(shell::navigate_back(&mut state));
  assert_eq!(current_preferences(&state), Some(preferences()));
  assert!(
    state
      .full
      .as_ref()
      .unwrap()
      .browse
      .saved
      .as_ref()
      .unwrap()
      .deleted
  );
  assert_eq!(
    state.kernel.settings.snapshot().browse_filters(),
    original_defaults
  );
  drop(shell::navigate(
    &mut state,
    Destination::Library {
      library_id: LIBRARY.into(),
      collection_type: "movies".into(),
    },
  ));
  assert!(state.full.as_ref().unwrap().browse.saved.is_none());
  assert_eq!(
    current_preferences(&state).unwrap().filters,
    VideoLibraryFilters::default()
  );
}

#[tokio::test]
async fn clearing_ordinary_conditions_changes_real_query_and_defaults_but_saved_clear_does_not() {
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  drop(shell::navigate(
    &mut state,
    Destination::Library {
      library_id: LIBRARY.into(),
      collection_type: "movies".into(),
    },
  ));
  let source = shell::browse_source(&state);
  drop(browse::commit_preferences(
    &mut state.full.as_mut().unwrap().browse,
    &mut state.kernel,
    source,
    preferences(),
  ));
  let before = state
    .full
    .as_ref()
    .unwrap()
    .browse
    .browser
    .model()
    .identity()
    .unwrap()
    .to_owned();
  let query = action(&mut state, Action::ClearConditions);
  let mut requests = 0;
  for message in fixture
    .run_task(query, |_| {
      requests += 1;
      FixtureReply::Page {
        total: 1,
        artwork: false,
      }
    })
    .await
  {
    drop(settle(&mut state, message));
  }
  assert_eq!(requests, 1);
  assert!(matches!(
    state.full.as_ref().unwrap().browse.view,
    LibraryBrowseView::Ready { .. }
  ));
  assert_ne!(
    state
      .full
      .as_ref()
      .unwrap()
      .browse
      .browser
      .model()
      .identity(),
    Some(before.as_str())
  );
  assert_eq!(
    current_preferences(&state),
    Some(BrowsePreferences::default())
  );
  assert_eq!(
    BrowsePreferences::from(state.kernel.settings.snapshot().browse_filters()),
    BrowsePreferences::default()
  );
  let filter = save(&state, "Default", BrowsePreferences::default()).await;
  drop(shell::apply_saved_browse(
    &mut state,
    resolved(filter.clone()),
  ));
  let ordinary_defaults = state
    .kernel
    .settings
    .snapshot()
    .browse_filters()
    .with_favorites_only(true)
    .with_sort(VideoLibrarySort::RecentlyAdded);
  state
    .kernel
    .settings
    .set_browse_filters(ordinary_defaults)
    .unwrap();
  browse::install_preferences(&mut state.full.as_mut().unwrap().browse, preferences());
  assert!(modified(&state));
  drop(action(&mut state, Action::ClearConditions));
  assert!(
    !modified(&state),
    "Clear compares against the saved definition rather than forcing dirty"
  );
  assert_eq!(
    state.kernel.settings.snapshot().browse_filters(),
    ordinary_defaults
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
}

#[tokio::test]
async fn failed_rename_preserves_draft_and_context_and_stale_editor_or_profile_actions_do_nothing()
{
  let mut fixture = BrowseFixture::new();
  let mut state = state(&fixture);
  let filter = save(&state, "Keep", preferences()).await;
  let other = save(&state, "Duplicate", BrowsePreferences::default()).await;
  drop(shell::apply_saved_browse(
    &mut state,
    resolved(filter.clone()),
  ));
  state.saved_browse.records = vec![filter.clone(), other];
  let identity = state
    .full
    .as_ref()
    .unwrap()
    .browse
    .browser
    .model()
    .identity()
    .unwrap()
    .to_owned();
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
  assert_eq!(
    state
      .full
      .as_ref()
      .unwrap()
      .browse
      .browser
      .model()
      .identity(),
    Some(identity.as_str())
  );
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
