//! Scoped, bounded Next Up enrichment shared by Library and Personal Lists.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use iced::Task;
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_sdk::OperationToken;

use super::message::Message as AppMessage;
use super::personal_lists::Route;
use super::state::{Destination, State};

// FullUi can be recreated without changing the account session. Request
// identity must survive that remount to reject already-queued old responses.
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> u64 {
  NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
}

#[derive(Default)]
pub struct Surface {
  session: Option<SessionToken>,
  series: BTreeMap<String, Entry>,
}

struct Entry {
  request: u64,
  state: Resolution,
}

enum Resolution {
  Queued,
  Loading(Arc<OperationToken>),
  Ready(Option<Box<VideoLibraryItem>>),
  Failed(String),
}

impl Drop for Entry {
  fn drop(&mut self) {
    if let Resolution::Loading(token) = &self.state {
      token.cancel();
    }
  }
}

#[derive(Clone)]
pub enum Message {
  Retry(String),
  Settled {
    session: SessionToken,
    request: u64,
    series_id: String,
    result: Result<Option<Box<VideoLibraryItem>>, String>,
  },
}

pub enum Target<'a> {
  Ready(&'a VideoLibraryItem),
  Loading,
  Failed(&'a str),
  NoNextUp,
  Unavailable,
}

pub fn target<'a>(state: &'a State, item: &'a VideoLibraryItem) -> Target<'a> {
  if item.item_type.eq_ignore_ascii_case("Series") {
    let Some(full) = state.full.as_ref() else {
      return Target::Unavailable;
    };
    match full
      .list_playback
      .series
      .get(&item.id)
      .map(|entry| &entry.state)
    {
      Some(Resolution::Ready(Some(item))) => Target::Ready(item),
      Some(Resolution::Ready(None)) => Target::NoNextUp,
      Some(Resolution::Failed(error)) => Target::Failed(error),
      _ => Target::Loading,
    }
  } else if item.item_type.eq_ignore_ascii_case("Movie")
    || item.item_type.eq_ignore_ascii_case("Episode")
  {
    Target::Ready(item)
  } else {
    Target::Unavailable
  }
}

/// Clears cached Next Up after a confirmed playback/user-data change.
pub fn invalidate(state: &mut State) {
  if let Some(full) = state.full.as_mut() {
    full.list_playback.series.clear();
  }
}

pub fn update(state: &mut State, message: Message) -> Task<AppMessage> {
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  match message {
    Message::Retry(id) => {
      if let Some(entry) = full.list_playback.series.get_mut(&id) {
        if matches!(entry.state, Resolution::Failed(_)) {
          entry.state = Resolution::Queued;
        }
      }
    }
    Message::Settled {
      session,
      request,
      series_id,
      result,
    } => {
      if !state.kernel.request_gate.is_current_session(session) {
        return Task::none();
      }
      if let Some(entry) = full.list_playback.series.get_mut(&series_id) {
        if entry.request == request && matches!(entry.state, Resolution::Loading(_)) {
          entry.state = match result {
            Ok(item) => Resolution::Ready(item),
            Err(error) => Resolution::Failed(error),
          };
        }
      }
    }
  }
  Task::none()
}

/// Called after routing: only the current page's retained items are enriched,
/// with four in-flight queries and cancellation when an item leaves the page.
pub fn sync(state: &mut State) -> Task<AppMessage> {
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  let session = state.kernel.request_gate.current_session();
  let surface = &mut full.list_playback;
  if surface.session != Some(session) {
    surface.series.clear();
    surface.session = Some(session);
  }
  let mut ids = BTreeSet::new();
  let mut admit = |item: &VideoLibraryItem| {
    if item.item_type.eq_ignore_ascii_case("Series") {
      ids.insert(item.id.clone());
    }
  };
  match &state.shell.destination {
    Destination::Library { .. } | Destination::Search(_) => {
      if let LibraryBrowseView::Ready { visible_items, .. } = &full.browse.view {
        for item in visible_items.iter().filter_map(|slot| slot.item.as_ref()) {
          admit(item);
        }
      }
    }
    Destination::PersonalLists(route) => {
      let pages = [
        (Route::Watchlist, &full.personal_lists.watchlist),
        (Route::Favorites, &full.personal_lists.favorites),
        (Route::History, &full.personal_lists.history),
      ];
      for (kind, page) in pages {
        if *route == Route::Overview || *route == kind {
          for item in page.entries.iter().filter_map(|entry| entry.item.as_ref()) {
            admit(item);
          }
        }
      }
    }
    _ => {}
  }
  surface.series.retain(|id, _| ids.contains(id));
  for id in ids {
    surface.series.entry(id).or_insert(Entry {
      request: 0,
      state: Resolution::Queued,
    });
  }
  if state.shell.quit_requested
    || super::accounts::content_mutations_blocked(&state.kernel)
    || state.kernel.client.is_none()
  {
    return Task::none();
  }
  let loading = surface
    .series
    .values()
    .filter(|entry| matches!(entry.state, Resolution::Loading(_)))
    .count();
  let mut tasks = Vec::new();
  for (id, entry) in surface
    .series
    .iter_mut()
    .filter(|(_, entry)| matches!(entry.state, Resolution::Queued))
    .take(4_usize.saturating_sub(loading))
  {
    let token = match state.kernel.sdk.new_operation_token() {
      Ok(token) => token,
      Err(error) => {
        entry.state = Resolution::Failed(error.to_string());
        continue;
      }
    };
    let request = next_request_id();
    entry.request = request;
    entry.state = Resolution::Loading(token.clone());
    let sdk = state.kernel.sdk.clone();
    let series_id = id.clone();
    tasks.push(Task::perform(
      async move {
        let result = sdk
          .next_episode_item(token, series_id.clone())
          .await
          .map(|item| item.map(Box::new))
          .map_err(|error| jellypilot_core::diagnostics::sanitize_message(&error.to_string()));
        Message::Settled {
          session,
          request,
          series_id,
          result,
        }
      },
      AppMessage::ListPlayback,
    ));
  }
  Task::batch(tasks)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::state::FullUi;
  use jellypilot_media_server::{MediaServerProvider, SavedSession};

  #[test]
  fn a_response_queued_before_full_ui_recreation_cannot_settle_its_new_request() {
    let mut state = crate::app::update::tests::test_state();
    state.kernel.sdk.adopt_test_session(SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: "https://next-up.example.test".to_owned(),
      access_token: "fixture-token".to_owned(),
      user_id: "fixture-user".to_owned(),
      user_name: "Fixture".to_owned(),
      server_name: None,
      device_id: None,
    });
    let session = state.kernel.request_gate.current_session();
    let old_request = next_request_id();
    let old_token = state
      .kernel
      .sdk
      .new_operation_token()
      .expect("old operation");
    state.full = Some(FullUi::default());
    state.full.as_mut().unwrap().list_playback.series.insert(
      "series".to_owned(),
      Entry {
        request: old_request,
        state: Resolution::Loading(old_token.clone()),
      },
    );

    state.full = Some(FullUi::default());
    assert!(
      old_token.is_cancelled(),
      "departed surface cancels outstanding network work"
    );
    let new_request = next_request_id();
    let new_token = state
      .kernel
      .sdk
      .new_operation_token()
      .expect("new operation");
    state.full.as_mut().unwrap().list_playback.series.insert(
      "series".to_owned(),
      Entry {
        request: new_request,
        state: Resolution::Loading(new_token),
      },
    );
    // The old network result may have reached the UI queue before cancellation.
    let _ = update(
      &mut state,
      Message::Settled {
        session,
        request: old_request,
        series_id: "series".to_owned(),
        result: Ok(None),
      },
    );
    assert!(matches!(
      state.full.as_ref().unwrap().list_playback.series["series"].state,
      Resolution::Loading(_)
    ));
    let _ = update(
      &mut state,
      Message::Settled {
        session,
        request: new_request,
        series_id: "series".to_owned(),
        result: Ok(None),
      },
    );
    assert!(matches!(
      state.full.as_ref().unwrap().list_playback.series["series"].state,
      Resolution::Ready(None)
    ));
  }
}
