//! Bounded collection metadata for the featured item and the current playback item.

use std::collections::HashMap;
use std::sync::Arc;

use iced::{task, Task};
use jellypilot_core::collections::{collection_target, CollectionTarget};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_media_server::{VideoLibraryItem, VideoUserDataUpdate};
use jellypilot_mpv::playback::Playable;

use super::accounts;
use super::message::Message;
use super::personal_lists;
use super::state::{Destination, FullUi, State};

#[derive(Clone, Copy)]
pub(crate) enum Source {
  Hero,
  NowPlaying,
}

pub(crate) struct Controls {
  pub favorite: Option<bool>,
  pub watchlisted: Option<bool>,
  pub favorite_action: Option<Message>,
  pub watchlist_action: Option<Message>,
  pub favorite_label: String,
  pub watchlist_label: String,
}

struct Entry {
  generation: u64,
  is_series: bool,
  item: Option<VideoLibraryItem>,
  pending: Option<task::Handle>,
  confirmed_during_load: Option<VideoUserDataUpdate>,
}

impl Drop for Entry {
  fn drop(&mut self) {
    if let Some(handle) = &self.pending {
      handle.abort();
    }
  }
}

#[derive(Default)]
pub(crate) struct Surface {
  session: Option<SessionToken>,
  entries: HashMap<String, Entry>,
}

#[derive(Clone)]
pub(crate) enum CollectionMessage {
  Loaded {
    session: SessionToken,
    generation: u64,
    item_id: String,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  Favorite {
    session: SessionToken,
    item_id: String,
    favorite: bool,
  },
  Watchlist {
    session: SessionToken,
    item_id: String,
    watchlisted: bool,
  },
}

fn hero_target(home: &super::home::Surface) -> Option<CollectionTarget<'_>> {
  let item = home.data.featured_item()?;
  collection_target(&item.id, &item.item_type, item.series_id.as_deref())
}

fn playing_target(playback: &super::playback::Surface) -> Option<CollectionTarget<'_>> {
  let now = playback.view.now_playing.as_ref()?;
  let playable = playback.playable.as_ref()?;
  if playable.item_id() != now.item.item_id {
    return None;
  }
  match playable {
    Playable::Library(item) => {
      collection_target(&item.id, &item.item_type, item.series_id.as_deref())
    }
    Playable::Detail(item) => {
      collection_target(&item.id, &item.item_type, item.series_id.as_deref())
    }
    Playable::Media(item) => {
      collection_target(&item.id, &item.item_type, item.series_id.as_deref())
    }
  }
}

fn target(state: &State, source: Source) -> Option<CollectionTarget<'_>> {
  match source {
    Source::Hero => hero_target(&state.full.as_ref()?.home),
    Source::NowPlaying => playing_target(&state.playback),
  }
}

pub(crate) fn controls(state: &State, source: Source) -> Controls {
  let current = target(state, source);
  let session = state.kernel.request_gate.current_session();
  let full = state.full.as_ref();
  let entry = current.and_then(|target| {
    full
      .filter(|full| full.collections.session == Some(session))?
      .collections
      .entries
      .get(target.item_id)
  });
  let item = entry.and_then(|entry| entry.item.as_ref());
  let favorite = item.map(|item| item.favorite);
  let watchlisted = full.and_then(|full| {
    item
      .filter(|_| full.personal_lists.membership_loaded)
      .map(|item| full.personal_lists.watchlist_ids.contains(&item.id))
  });
  let busy = current.is_some_and(|target| super::item_actions::busy(&state.kernel, target.item_id));
  let enabled = !busy
    && !accounts::content_mutations_blocked(&state.kernel)
    && !state.shell.quit_requested
    && state.kernel.client.is_some();
  let favorite_action = item.filter(|_| enabled).map(|item| {
    Message::Collections(CollectionMessage::Favorite {
      session,
      item_id: item.id.clone(),
      favorite: !item.favorite,
    })
  });
  let watchlist_action = item.filter(|_| enabled).and_then(|item| {
    watchlisted.map(|value| {
      Message::Collections(CollectionMessage::Watchlist {
        session,
        item_id: item.id.clone(),
        watchlisted: !value,
      })
    })
  });
  let is_series = current.is_some_and(|target| target.is_series);
  let unavailable = if entry.is_some_and(|entry| entry.pending.is_some()) {
    "collection-loading"
  } else {
    "collection-unavailable"
  };
  let favorite_label = state.t(if busy {
    "collection-updating"
  } else {
    match (favorite, is_series) {
      (Some(true), true) => "collection-series-unfavorite",
      (Some(false), true) => "collection-series-favorite",
      (Some(true), false) => "collection-movie-unfavorite",
      (Some(false), false) => "collection-movie-favorite",
      (None, _) => unavailable,
    }
  });
  let watchlist_label = state.t(if busy {
    "collection-updating"
  } else {
    match (watchlisted, is_series) {
      (Some(true), true) => "collection-series-watchlist-remove",
      (Some(false), true) => "collection-series-watchlist-add",
      (Some(true), false) => "collection-movie-watchlist-remove",
      (Some(false), false) => "collection-movie-watchlist-add",
      (None, _) => unavailable,
    }
  });
  Controls {
    favorite,
    watchlisted,
    favorite_action,
    watchlist_action,
    favorite_label,
    watchlist_label,
  }
}

/// Admits at most two metadata requests for the currently presented collection targets.
pub(crate) fn reconcile(state: &mut State) -> Task<Message> {
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  let session = state.kernel.request_gate.current_session();
  if full.collections.session != Some(session) || state.kernel.client.is_none() {
    full.collections = Surface {
      session: Some(session),
      ..Surface::default()
    };
  }
  let mut tasks = Vec::new();
  let desired = [hero_target(&full.home), playing_target(&state.playback)];
  full
    .collections
    .entries
    .retain(|id, _| desired.iter().flatten().any(|target| target.item_id == id));
  if !state.shell.images_visible || accounts::content_mutations_blocked(&state.kernel) {
    return Task::batch(tasks);
  }
  let Some(client) = state.kernel.client.as_ref() else {
    return Task::batch(tasks);
  };
  for target in desired.into_iter().flatten() {
    if full.collections.entries.contains_key(target.item_id)
      || super::item_actions::busy(&state.kernel, target.item_id)
    {
      continue;
    }
    let generation = state.watchlist.next_generation();
    let item_id = target.item_id.to_owned();
    let result_id = item_id.clone();
    let request_id = item_id.clone();
    let client = Arc::clone(client);
    let (task, pending) = Task::perform(
      async move {
        client
          .library()
          .video_items_by_ids(vec![request_id])
          .await
          .map_err(|error| error.to_string())
      },
      move |result| {
        Message::Collections(CollectionMessage::Loaded {
          session,
          generation,
          item_id: result_id,
          result,
        })
      },
    )
    .abortable();
    full.collections.entries.insert(
      item_id,
      Entry {
        generation,
        is_series: target.is_series,
        item: None,
        pending: Some(pending),
        confirmed_during_load: None,
      },
    );
    tasks.push(task);
  }
  Task::batch(tasks)
}

pub(crate) fn apply_confirmed(surface: &mut Surface, update: &VideoUserDataUpdate) {
  if let Some(entry) = surface.entries.get_mut(&update.item_id) {
    if let Some(item) = &mut entry.item {
      item.favorite = update.favorite;
      item.played = update.played;
    }
    if entry.pending.is_some() {
      entry.confirmed_during_load = Some(update.clone());
    }
  }
}

pub(crate) fn invalidate(state: &mut State) {
  if let Some(full) = state.full.as_mut() {
    full.collections.entries.clear();
  }
}

pub(crate) fn update(state: &mut State, message: CollectionMessage) -> Task<Message> {
  if let CollectionMessage::Loaded {
    session,
    generation,
    item_id,
    result,
  } = message
  {
    if !state.kernel.request_gate.is_current_session(session) {
      return Task::none();
    }
    let Some(entry) = state.full.as_mut().and_then(|full| {
      full
        .collections
        .entries
        .get_mut(&item_id)
        .filter(|entry| entry.generation == generation)
    }) else {
      return Task::none();
    };
    entry.pending = None;
    match result {
      Ok(items) => {
        entry.item = items.into_iter().find(|item| {
          item.id == item_id
            && item
              .item_type
              .eq_ignore_ascii_case(if entry.is_series { "series" } else { "movie" })
        });
        if let (Some(item), Some(update)) = (&mut entry.item, entry.confirmed_during_load.take()) {
          item.favorite = update.favorite;
          item.played = update.played;
        }
      }
      Err(error) => {
        tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(&error), "Collection metadata request failed");
      }
    }
    return Task::none();
  }
  let (session, item_id) = match &message {
    CollectionMessage::Favorite {
      session, item_id, ..
    }
    | CollectionMessage::Watchlist {
      session, item_id, ..
    } => (*session, item_id),
    CollectionMessage::Loaded { .. } => return Task::none(),
  };
  if !state.kernel.request_gate.is_current_session(session) || state.shell.quit_requested {
    return Task::none();
  }
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  if super::item_actions::busy(&state.kernel, item_id) {
    return Task::none();
  }
  match message {
    CollectionMessage::Favorite {
      item_id, favorite, ..
    } => {
      if !favorite_is_known(full, &state.shell.destination, session, &item_id) {
        return Task::none();
      }
      super::item_actions::start_server(
        state,
        item_id,
        if favorite {
          jellypilot_media_server::VideoUserDataAction::Favorite
        } else {
          jellypilot_media_server::VideoUserDataAction::Unfavorite
        },
        super::item_actions::Origin::Other,
      )
    }
    CollectionMessage::Watchlist { watchlisted, .. } => {
      if full.collections.session != Some(session) {
        return Task::none();
      }
      let Some(item) = full
        .collections
        .entries
        .get(item_id)
        .and_then(|entry| entry.item.as_ref())
      else {
        return Task::none();
      };
      if !full.personal_lists.membership_loaded
        || full.personal_lists.watchlist_ids.contains(&item.id) == watchlisted
      {
        return Task::none();
      }
      let item = item.clone();
      super::item_actions::update(
        state,
        super::item_actions::Message::WatchlistToggle(Box::new(item)),
      )
    }
    CollectionMessage::Loaded { .. } => Task::none(),
  }
}

fn favorite_is_known(
  full: &FullUi,
  destination: &Destination,
  session: SessionToken,
  item_id: &str,
) -> bool {
  if full.collections.session == Some(session)
    && full
      .collections
      .entries
      .get(item_id)
      .is_some_and(|entry| entry.item.is_some())
  {
    return true;
  }
  match destination {
    Destination::Library { .. } | Destination::Search(_) => {
      let jellypilot_core::browse_model::LibraryBrowseView::Ready { visible_items, .. } =
        &full.browse.view
      else {
        return false;
      };
      visible_items
        .iter()
        .any(|slot| slot.item.as_ref().is_some_and(|item| item.id == item_id))
    }
    Destination::PersonalLists(route) => {
      let lists = &full.personal_lists;
      let pages: &[&personal_lists::ListPage] = match route {
        personal_lists::Route::Overview => &[&lists.favorites, &lists.watchlist, &lists.history],
        personal_lists::Route::Favorites => &[&lists.favorites],
        personal_lists::Route::Watchlist => &[&lists.watchlist],
        personal_lists::Route::History => &[&lists.history],
      };
      pages.iter().any(|page| {
        page
          .entries
          .iter()
          .any(|entry| entry.item.as_ref().is_some_and(|item| item.id == item_id))
      })
    }
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::{item_actions, state::UserDataActionKind};
  use iced::futures::StreamExt;
  use jellypilot_core::{detail::DetailContent, LoadState};

  fn fixture(server_url: &str) -> State {
    let mut state = State::boot(false);
    state.full = Some(FullUi::default());
    state.kernel.client = Some(Arc::new(jellypilot_media_server::JellyfinClient::new()));
    state
      .kernel
      .client
      .as_ref()
      .unwrap()
      .login()
      .adopt_validated_session(&jellypilot_media_server::SavedSession {
        provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
        server_url: server_url.to_owned(),
        user_id: "user".to_owned(),
        user_name: "User".to_owned(),
        access_token: "test-token".to_owned(),
        server_name: None,
        device_id: None,
      });
    state
      .kernel
      .request_gate
      .set_detail_item(Some("series".to_owned()));
    state.shell.destination = Destination::Detail("series".to_owned());
    let episode = VideoLibraryItem {
      community_rating: None,
      episode_count: None,
      last_played_date: None,
      premiere_date: None,
      id: "episode".to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      series_id: Some("series".to_owned()),
      series_name: Some("Series".to_owned()),
      production_year: None,
      runtime_seconds: None,
      played: false,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      logo_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: Some(1),
      episode_number: Some(1),
      resume_position_seconds: Some(20.0),
      played_percentage: None,
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
    };
    let series = VideoLibraryItem {
      premiere_date: None,
      id: "series".to_owned(),
      item_type: "Series".to_owned(),
      ..episode.clone()
    };
    let full = state.full.as_mut().unwrap();
    full
      .home
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![episode],
        next_up: Vec::new(),
      }));
    full.collections.session = Some(state.kernel.request_gate.current_session());
    full.collections.entries.insert(
      "series".to_owned(),
      Entry {
        generation: 1,
        is_series: true,
        item: Some(series),
        pending: None,
        confirmed_during_load: None,
      },
    );
    full.detail.data.content = LoadState::Ready(DetailContent::Show(Box::new(
      jellypilot_media_server::VideoShowDetail {
        id: "series".to_owned(),
        name: "Series".to_owned(),
        overview: None,
        production_year: None,
        genres: Vec::new(),
        played: false,
        favorite: false,
        can_play: true,
        artwork_image_id: None,
        backdrop_image_id: None,
        logo_image_id: None,
        next_episode: None,
        seasons: Vec::new(),
        metadata: Default::default(),
        original_language: None,
      },
    )));
    state
  }

  async fn server(response: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
      let (socket, _) = listener.accept().await.unwrap();
      let mut socket = BufReader::new(socket);
      loop {
        let mut line = String::new();
        assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
        if line == "\r\n" {
          break;
        }
      }
      socket.get_mut().write_all(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
        response.len(),
      ).as_bytes()).await.unwrap();
    });
    (format!("http://{address}"), peer)
  }

  async fn completion(task: Task<Message>) -> Message {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
      let mut stream = iced_runtime::task::into_stream(task).expect("action emits work");
      while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(
          message @ Message::ItemActions(item_actions::Message::Settled { .. }),
        ) = action
        {
          return message;
        }
      }
      panic!("action ended without a settlement");
    })
    .await
    .expect("action settles")
  }

  #[tokio::test]
  async fn detail_confirmation_after_navigation_updates_the_hero_collection() {
    let (url, peer) = server(r#"{"Key":"series","IsFavorite":true,"Played":false}"#).await;
    let mut state = fixture(&url);
    let task = crate::app::update::update(
      &mut state,
      Message::ItemActions(item_actions::Message::Detail(UserDataActionKind::Favorite)),
    );
    assert_eq!(controls(&state, Source::Hero).favorite, Some(false));
    let full = state.full.as_mut().unwrap();
    super::super::detail::leave_view(&mut full.detail, &mut state.kernel);
    state.shell.destination = Destination::Home;
    assert!(controls(&state, Source::Hero).favorite_action.is_none());
    let message = completion(task).await;
    drop(crate::app::update::update(&mut state, message));
    peer.await.unwrap();
    let hero = controls(&state, Source::Hero);
    assert_eq!(hero.favorite, Some(true));
    assert!(matches!(hero.favorite_action, Some(Message::Collections(
      CollectionMessage::Favorite { item_id, favorite: false, .. }
    )) if item_id == "series"));
  }

  #[tokio::test]
  async fn confirmation_updates_detail_saved_in_navigation_history() {
    let (url, peer) = server(r#"{"Key":"series","IsFavorite":true,"Played":false}"#).await;
    let mut state = fixture(&url);
    let task = crate::app::update::update(
      &mut state,
      Message::ItemActions(item_actions::Message::Detail(UserDataActionKind::Favorite)),
    );
    drop(crate::app::update::update(
      &mut state,
      Message::Home(crate::app::message::HomeMessage::Navigate(
        Destination::Detail("other".to_owned()),
      )),
    ));
    let message = completion(task).await;
    drop(crate::app::update::update(&mut state, message));
    peer.await.unwrap();
    drop(crate::app::update::update(
      &mut state,
      Message::Detail(crate::app::message::DetailMessage::Back),
    ));
    assert_eq!(
      state.shell.destination,
      Destination::Detail("series".to_owned())
    );
    assert!(matches!(
      &state.full.as_ref().unwrap().detail.data.content,
      LoadState::Ready(DetailContent::Show(show)) if show.favorite
    ));
  }

  #[tokio::test]
  async fn detail_cannot_start_a_conflicting_write_while_collection_mutation_is_pending() {
    let mut state = fixture("http://127.0.0.1:9");
    let session = state.kernel.request_gate.current_session();
    let pending = update(
      &mut state,
      CollectionMessage::Favorite {
        session,
        item_id: "series".to_owned(),
        favorite: true,
      },
    );
    let task = crate::app::update::update(
      &mut state,
      Message::ItemActions(item_actions::Message::Detail(UserDataActionKind::Played)),
    );
    if let Some(mut stream) = iced_runtime::task::into_stream(task) {
      assert!(
        stream.next().await.is_none(),
        "conflicting mutation emitted work"
      );
    }
    assert!(controls(&state, Source::Hero).favorite_action.is_none());
    drop(pending);
  }

  #[tokio::test]
  async fn unconfirmed_detail_action_keeps_flags_and_reports_inline_failure() {
    let (url, peer) = server(r#"{"Key":"series","IsFavorite":false,"Played":false}"#).await;
    let mut state = fixture(&url);
    let task = crate::app::update::update(
      &mut state,
      Message::ItemActions(item_actions::Message::Detail(UserDataActionKind::Favorite)),
    );
    let message = completion(task).await;
    drop(crate::app::update::update(&mut state, message));
    peer.await.unwrap();
    assert_eq!(controls(&state, Source::Hero).favorite, Some(false));
    assert!(controls(&state, Source::Hero).favorite_action.is_some());
    assert!(state
      .full
      .as_ref()
      .unwrap()
      .detail
      .data
      .user_data_error
      .is_some());
    assert!(state.kernel.active_toast.is_none());
  }

  #[tokio::test]
  async fn previous_session_confirmation_cannot_update_current_collections() {
    let (url, peer) = server(r#"{"Key":"series","IsFavorite":true,"Played":false}"#).await;
    let mut state = fixture(&url);
    let task = crate::app::update::update(
      &mut state,
      Message::ItemActions(item_actions::Message::Detail(UserDataActionKind::Favorite)),
    );
    let message = completion(task).await;
    peer.await.unwrap();
    state.kernel.request_gate.disconnect();
    state.full.as_mut().unwrap().collections.session =
      Some(state.kernel.request_gate.current_session());
    drop(crate::app::update::update(&mut state, message));
    assert_eq!(controls(&state, Source::Hero).favorite, Some(false));
    assert!(controls(&state, Source::Hero).favorite_action.is_some());
  }
}
