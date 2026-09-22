use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use iced::futures::StreamExt;
use jellypilot_core::watchlist::WatchlistStore;
use jellypilot_media_server::{MediaServerProvider, SavedSession};
use jellypilot_sdk::item_actions::{WatchlistWrite, WatchlistWriteAction};

use super::*;
use crate::app::message::HomeMessage;
use crate::app::state::Destination;

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

struct StoreDirectory(PathBuf);

impl StoreDirectory {
  fn new() -> Self {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-desktop-undo-{}-{}",
      std::process::id(),
      NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).expect("isolated Undo store directory");
    Self(path)
  }

  fn path(&self) -> PathBuf {
    self.0.join("watchlist.json")
  }

  fn records(&self, scope: &ProfileScope) -> Vec<WatchlistRecord> {
    WatchlistStore::for_test(self.path())
      .expect("persisted Watchlist")
      .records_for(scope)
  }
}

impl Drop for StoreDirectory {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

fn connected_state() -> State {
  let mut state = crate::app::update::tests::test_state();
  state.kernel.sdk.adopt_test_session(SavedSession {
    provider: MediaServerProvider::Jellyfin,
    server_url: "https://undo.example.test".to_owned(),
    access_token: "test-token".to_owned(),
    user_id: "undo-user".to_owned(),
    user_name: "Undo test".to_owned(),
    server_name: None,
    device_id: None,
  });
  let profile = state.kernel.sdk.active_profile().expect("active profile");
  accounts::sync_activated(&mut state.kernel, &profile);
  item_actions::sync_scope(&mut state.kernel);
  state
}

#[test]
fn hidden_tv_notices_keep_their_remaining_undo_time() {
  use crate::app::tv;
  use jellypilot_core::config::UiMode;
  use jellypilot_core::tv_navigation::Input;

  for surface in ["settings", "search", "player", "menu"] {
    let mut state = connected_state();
    state.shell.ui_mode = UiMode::Tv;
    state.shell.window_id = Some(iced::window::Id::unique());
    state.shell.window_size = iced::Size::new(1920.0, 1080.0);
    state.kernel.undo.push(Removal::Favorite {
      item_id: "removed".into(),
      name: "Removed film".into(),
    });
    reconcile(&mut state);
    let id = state.kernel.undo.next_id;
    assert_eq!(state.kernel.undo.queue.visible().collect::<Vec<_>>(), [id]);
    state.kernel.undo.queue.advance(Duration::from_secs(1));
    let remaining = state.kernel.undo.queue.next_expiration().unwrap() - Duration::from_secs(1);

    match surface {
      "settings" => state.tv.settings.open = true,
      "search" => state.tv.search.open = true,
      "player" => {
        state.shell.destination = Destination::NowPlaying;
        state.playback.view.lifecycle.playback_active = true;
      }
      "menu" => drop(tv::lists::open_menu(&mut state, item("other"))),
      _ => unreachable!(),
    }
    reconcile(&mut state);
    assert_eq!(state.kernel.undo.queue.visible().count(), 0, "{surface}");
    let hidden_until = Duration::from_secs(61);
    assert!(state.kernel.undo.queue.advance(hidden_until).is_empty());
    assert!(state.kernel.undo.queue.contains(id));

    state.tv.settings.open = false;
    state.tv.search.open = false;
    state.playback.view.lifecycle.playback_active = false;
    drop(tv::lists::input(&mut state, Input::Back));
    reconcile(&mut state);
    assert_eq!(state.kernel.undo.queue.visible().collect::<Vec<_>>(), [id]);
    assert_eq!(
      state.kernel.undo.queue.next_expiration(),
      Some(hidden_until + remaining),
      "{surface} must preserve the remaining visible exposure"
    );
    assert_eq!(
      state.kernel.undo.queue.advance(hidden_until + remaining),
      [id]
    );
  }
}

fn item(id: &str) -> VideoLibraryItem {
  VideoLibraryItem {
    id: id.to_owned(),
    name: format!("Item {id}"),
    item_type: "Movie".to_owned(),
    production_year: None,
    runtime_seconds: None,
    community_rating: None,
    episode_count: None,
    last_played_date: None,
    played: false,
    favorite: false,
    artwork_image_id: None,
    backdrop_image_id: None,
    logo_image_id: None,
    series_poster_image_id: None,
    episode_thumb_image_id: None,
    series_thumb_image_id: None,
    series_backdrop_image_id: None,
    season_poster_image_id: None,
    season_number: None,
    episode_number: None,
    index_number_end: None,
    series_id: None,
    series_name: None,
    end_year: None,
    series_continuing: false,
    unplayed_item_count: None,
    resume_position_seconds: None,
    played_percentage: None,
    overview: None,
    premiere_date: None,
  }
}

fn watchlist_state() -> (State, StoreDirectory, ProfileScope, Vec<WatchlistRecord>) {
  let mut state = connected_state();
  // Operations outlive the originating page; omit presentation to avoid metadata HTTP work.
  state.full = None;
  let scope = personal_lists::active_scope(&state.kernel).expect("scope");
  let directory = StoreDirectory::new();
  let mut store = WatchlistStore::for_test(directory.path()).expect("Watchlist store");
  for (id, timestamp) in [("older", 10), ("removed", 20), ("newer", 30)] {
    store
      .add(WatchlistRecord::from_item(scope.clone(), &item(id), timestamp).unwrap())
      .unwrap();
  }
  let original = store.records_for(&scope);
  state.watchlist = personal_lists::Runtime::for_test(store);
  (state, directory, scope, original)
}

async fn outputs(task: Task<AppMessage>) -> Vec<AppMessage> {
  let Some(mut stream) = iced_runtime::task::into_stream(task) else {
    return Vec::new();
  };
  tokio::time::timeout(Duration::from_secs(5), async move {
    let mut messages = Vec::new();
    while let Some(action) = stream.next().await {
      if let iced_runtime::Action::Output(message) = action {
        messages.push(message);
      }
    }
    messages
  })
  .await
  .expect("local Undo operation settles without network")
}

async fn settle_item_task(state: &mut State, task: Task<AppMessage>) {
  let messages = outputs(task).await;
  assert_eq!(messages.len(), 1);
  for message in messages {
    let AppMessage::ItemActions(message) = message else {
      panic!("expected item-action settlement");
    };
    drop(item_actions::update(state, message));
  }
}

async fn remove_watchlist(state: &mut State, scope: &ProfileScope, item_id: &str) -> u64 {
  let admission = state
    .kernel
    .item_actions
    .begin(item_id, Action::Watchlist(false))
    .unwrap();
  let receipt = admission.receipt().clone();
  let (snapshot, removed) = state
    .watchlist
    .remove_undoable(admission, state.watchlist.scope_epoch(scope))
    .await
    .expect("persisted removal");
  drop(item_actions::update(
    state,
    item_actions::Message::Settled {
      receipt,
      origin: item_actions::Origin::Other,
      result: Ok(WriteOutcome::Watchlist(snapshot)),
      removed: removed.map(|record| Box::new(Removal::Watchlist(record))),
    },
  ));
  state.kernel.undo.next_id
}

#[tokio::test]
async fn watchlist_undo_restores_the_exact_record_and_original_sort_position() {
  let (mut state, directory, scope, original) = watchlist_state();
  let id = remove_watchlist(&mut state, &scope, "removed").await;
  assert_eq!(directory.records(&scope).len(), 2);
  let task = update(&mut state, Message::Restore(id));
  assert!(state.kernel.undo.notices[&id].pending);
  assert!(item_actions::busy(&state.kernel, "removed"));
  assert!(outputs(update(&mut state, Message::Restore(id)))
    .await
    .is_empty());
  settle_item_task(&mut state, task).await;
  assert_eq!(directory.records(&scope), original);
  assert!(!state.kernel.undo.queue.contains(id));
  assert!(!item_actions::busy(&state.kernel, "removed"));
}

#[tokio::test]
async fn failed_watchlist_undo_keeps_its_receipt_and_can_retry_after_storage_recovers() {
  let (mut state, directory, scope, original) = watchlist_state();
  let id = remove_watchlist(&mut state, &scope, "removed").await;
  let backup = directory.0.join("watchlist-backup.json");
  std::fs::rename(directory.path(), &backup).unwrap();
  std::fs::create_dir(directory.path()).unwrap();

  let task = update(&mut state, Message::Restore(id));
  settle_item_task(&mut state, task).await;
  let notice = &state.kernel.undo.notices[&id];
  assert!(notice.failed);
  assert!(!notice.pending);
  assert!(state.kernel.undo.queue.contains(id));
  assert!(!item_actions::busy(&state.kernel, "removed"));

  std::fs::remove_dir(directory.path()).unwrap();
  std::fs::rename(backup, directory.path()).unwrap();
  let task = update(&mut state, Message::Restore(id));
  settle_item_task(&mut state, task).await;
  assert_eq!(directory.records(&scope), original);
  assert!(!state.kernel.undo.notices.contains_key(&id));
}

#[tokio::test]
async fn newer_confirmed_watchlist_mutation_retires_only_the_obsolete_collection_receipt() {
  let (mut state, directory, scope, _) = watchlist_state();
  let obsolete = remove_watchlist(&mut state, &scope, "removed").await;
  state.kernel.undo.push(Removal::Favorite {
    item_id: "removed".to_owned(),
    name: "Same item, separate collection".to_owned(),
  });
  let favorite = state.kernel.undo.next_id;
  let admission = state
    .kernel
    .item_actions
    .begin("removed", Action::Watchlist(true))
    .unwrap();
  let receipt = admission.receipt().clone();
  let result = state
    .watchlist
    .apply_write(
      admission,
      WatchlistWrite {
        item_id: "removed".to_owned(),
        action: WatchlistWriteAction::Add(Box::new(item("removed"))),
      },
      state.watchlist.scope_epoch(&scope),
    )
    .await
    .map(WriteOutcome::Watchlist);
  assert!(result.is_ok());
  drop(item_actions::update(
    &mut state,
    item_actions::Message::Settled {
      receipt,
      origin: item_actions::Origin::Other,
      result,
      removed: None,
    },
  ));
  assert!(!state.kernel.undo.queue.contains(obsolete));
  assert!(state.kernel.undo.queue.contains(favorite));
  let current = directory.records(&scope);
  assert_eq!(current[0].item_id(), "removed");
  assert!(current[0].added_at_unix_millis() > 30);
  assert!(outputs(update(&mut state, Message::Restore(obsolete)))
    .await
    .is_empty());
  assert_eq!(directory.records(&scope), current);
}

async fn disconnect(state: &mut State, allowed: bool) {
  let task = crate::app::update::update(state, AppMessage::Account(accounts::Message::Disconnect));
  let channel = Arc::clone(&state.kernel.sdk_handoff.receiver);
  let respond = async move {
    channel
      .lock()
      .await
      .recv()
      .await
      .expect("profile handoff")
      .resolve(allowed);
  };
  let (messages, ()) = tokio::join!(outputs(task), respond);
  assert!(messages.iter().any(|message| matches!(
    message,
    AppMessage::Account(accounts::Message::DisconnectFinished { .. })
  )));
  for message in messages {
    drop(crate::app::update::update(state, message));
  }
}

#[tokio::test]
async fn navigation_and_aborted_handoff_keep_visible_and_queued_undo_but_disconnect_ends_all() {
  let mut state = connected_state();
  for index in 0..5 {
    state.kernel.undo.push(Removal::Favorite {
      item_id: format!("item-{index}"),
      name: format!("Item {index}"),
    });
  }
  let all: Vec<_> = state.kernel.undo.notices.keys().copied().collect();
  assert_eq!(state.kernel.undo.queue.visible().count(), 3);
  let destination = Destination::PersonalLists(personal_lists::Route::Watchlist);
  drop(crate::app::update::update(
    &mut state,
    AppMessage::Home(HomeMessage::Navigate(destination.clone())),
  ));
  assert_eq!(state.shell.destination, destination);
  assert!(all.iter().all(|id| state.kernel.undo.queue.contains(*id)));
  state.full = None;
  disconnect(&mut state, false).await;
  assert!(state.kernel.sdk.active_profile().is_some());
  assert!(all.iter().all(|id| state.kernel.undo.queue.contains(*id)));
  disconnect(&mut state, true).await;
  assert!(state.kernel.sdk.active_profile().is_none());
  assert!(state.kernel.undo.queue.is_empty());
  assert!(state.kernel.undo.notices.is_empty());
}

async fn settle_history_task(state: &mut State, task: Task<AppMessage>) -> Task<AppMessage> {
  let messages = outputs(task).await;
  assert_eq!(messages.len(), 1);
  let mut followups = Vec::new();
  for message in messages {
    let AppMessage::Undo(message) = message else {
      panic!("expected history settlement")
    };
    followups.push(update(state, message));
  }
  Task::batch(followups)
}

#[tokio::test]
async fn playback_restore_holds_history_admission_until_its_settlement() {
  let mut state = connected_state();
  state.full = None;
  let playback = playback_started(&mut state.kernel, "film".to_owned());
  assert!(history_busy(&state.kernel, "film"));
  assert!(outputs(update(
    &mut state,
    Message::RemoveHistory(Box::new(item("film")))
  ))
  .await
  .is_empty());
  drop(settle_history_task(&mut state, playback).await);
  assert!(!history_busy(&state.kernel, "film"));

  let removal = update(&mut state, Message::RemoveHistory(Box::new(item("film"))));
  drop(settle_history_task(&mut state, removal).await);
  assert_eq!(state.kernel.undo.notices.len(), 1);
  let Removal::History { receipt, .. } =
    &state.kernel.undo.notices[&state.kernel.undo.next_id].removal
  else {
    panic!("history receipt")
  };
  assert!(
    receipt.changed(),
    "a later removal is not erased by the earlier playback"
  );
}

#[tokio::test]
async fn playback_arriving_during_history_removal_waits_for_the_write_and_restores_afterward() {
  let mut state = connected_state();
  state.full = None;
  let removal = update(&mut state, Message::RemoveHistory(Box::new(item("film"))));
  assert!(history_busy(&state.kernel, "film"));
  assert!(
    outputs(playback_started(&mut state.kernel, "film".to_owned()))
      .await
      .is_empty()
  );
  assert!(state.kernel.undo.playback_after_history.contains("film"));
  let playback = settle_history_task(&mut state, removal).await;
  assert!(history_busy(&state.kernel, "film"));
  assert!(!state.kernel.undo.playback_after_history.contains("film"));
  let Removal::History { receipt, .. } =
    &state.kernel.undo.notices[&state.kernel.undo.next_id].removal
  else {
    panic!("persisted removal")
  };
  let receipt = Arc::clone(receipt);
  assert!(outputs(update(
    &mut state,
    Message::RemoveHistory(Box::new(item("film")))
  ))
  .await
  .is_empty());
  drop(settle_history_task(&mut state, playback).await);
  assert!(!history_busy(&state.kernel, "film"));
  assert!(state.kernel.undo.notices.is_empty());
  assert!(
    !receipt.undo().await.unwrap(),
    "playback already restored visibility"
  );
}

#[tokio::test]
async fn playback_arriving_during_history_undo_keeps_admission_until_both_writes_settle() {
  let mut state = connected_state();
  state.full = None;
  let removal = update(&mut state, Message::RemoveHistory(Box::new(item("film"))));
  drop(settle_history_task(&mut state, removal).await);
  let id = state.kernel.undo.next_id;
  let undo = update(&mut state, Message::Restore(id));
  assert!(state.kernel.undo.notices[&id].pending);
  assert!(
    outputs(playback_started(&mut state.kernel, "film".to_owned()))
      .await
      .is_empty()
  );
  let playback = settle_history_task(&mut state, undo).await;
  assert!(history_busy(&state.kernel, "film"));
  assert!(!state.kernel.undo.queue.contains(id));
  assert!(outputs(update(
    &mut state,
    Message::RemoveHistory(Box::new(item("film")))
  ))
  .await
  .is_empty());
  drop(settle_history_task(&mut state, playback).await);
  assert!(!history_busy(&state.kernel, "film"));
}
