//! Bounded collection metadata for the featured item and the current playback item.

use std::collections::HashMap;
use std::sync::Arc;

use iced::{task, Task};
use jellypilot_core::collections::{collection_target, CollectionTarget};
use jellypilot_core::detail::{apply_user_data_update, DetailContent};
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::LoadState;
use jellypilot_media_server::{VideoLibraryItem, VideoUserDataUpdate};
use jellypilot_mpv::playback::Playable;

use super::accounts;
use super::message::Message;
use super::personal_lists::{self, PersonalListsMessage};
use super::state::{Destination, FullUi, NoticeLevel, State};
use crate::i18n::UiText;

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

pub(crate) enum Change {
  Confirmed(VideoUserDataUpdate),
  Invalidated(String),
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

pub(crate) fn busy(full: &FullUi, item_id: &str) -> bool {
  full.personal_lists.busy_items.contains(item_id) || detail_busy(full, item_id)
}
fn detail_busy(full: &FullUi, item_id: &str) -> bool {
  full.detail.pending_user_data.contains_key(item_id)
    || full.detail.data.user_data_busy.is_some()
      && match &full.detail.data.content {
        LoadState::Ready(DetailContent::Item(item)) => item.id == item_id,
        LoadState::Ready(DetailContent::Show(show)) => show.id == item_id,
        _ => false,
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
  let busy = current.is_some_and(|target| {
    full.is_some_and(|full| {
      full.personal_lists.busy_items.contains(target.item_id) || detail_busy(full, target.item_id)
    })
  });
  let enabled = !busy
    && !accounts::content_mutations_blocked(&state.accounts)
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

/// Applies accepted cross-surface changes and admits at most two metadata requests.
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
  for (change, refresh_detail) in [
    (full.detail.collection_change.take(), false),
    (full.personal_lists.collection_change.take(), true),
  ] {
    match change {
      Some(Change::Confirmed(update)) => {
        tasks.push(super::browse::apply_user_data_update(
          &mut full.browse,
          &mut state.kernel,
          &update,
        ));
        if let Some(entry) = full.collections.entries.get_mut(&update.item_id) {
          if let Some(item) = &mut entry.item {
            item.favorite = update.favorite;
            item.played = update.played;
          }
          if entry.pending.is_some() {
            entry.confirmed_during_load = Some(update.clone());
          }
        }
        apply_user_data_update(&mut full.detail.data.content, &update);
        if let Some(item) = full.detail.items.get_mut(&update.item_id) {
          item.favorite = update.favorite;
          item.played = update.played;
        }
        if refresh_detail
          && matches!(&state.shell.destination, Destination::Detail(id) if id == &update.item_id)
        {
          super::detail::cancel_refresh(&mut full.detail, &mut state.kernel);
          tasks.push(super::detail::refresh(
            &mut full.detail,
            &mut state.kernel,
            &update.item_id,
          ));
        }
      }
      Some(Change::Invalidated(item_id)) => {
        full.collections.entries.remove(&item_id);
      }
      None => {}
    }
  }
  let desired = [hero_target(&full.home), playing_target(&state.playback)];
  full
    .collections
    .entries
    .retain(|id, _| desired.iter().flatten().any(|target| target.item_id == id));
  if !state.shell.images_visible || accounts::content_mutations_blocked(&state.accounts) {
    return Task::batch(tasks);
  }
  let Some(client) = state.kernel.client.as_ref() else {
    return Task::batch(tasks);
  };
  for target in desired.into_iter().flatten() {
    if full.collections.entries.contains_key(target.item_id)
      || full.personal_lists.busy_items.contains(target.item_id)
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
  if accounts::content_mutations_blocked(&state.accounts) {
    return state.kernel.show_toast(
      NoticeLevel::Warning,
      UiText::new("shell-account-change-lists"),
    );
  }
  let Some(full) = state.full.as_mut() else {
    return Task::none();
  };
  if full.personal_lists.busy_items.contains(item_id) || detail_busy(full, item_id) {
    return Task::none();
  }
  match message {
    CollectionMessage::Favorite {
      item_id, favorite, ..
    } => {
      if !favorite_is_known(full, &state.shell.destination, session, &item_id) {
        return Task::none();
      }
      personal_lists::set_favorite(
        &mut full.personal_lists,
        &mut state.kernel,
        &state.watchlist,
        item_id,
        favorite,
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
      personal_lists::update(
        &mut full.personal_lists,
        &mut state.kernel,
        &state.watchlist,
        PersonalListsMessage::ToggleWatchlist(Box::new(item)),
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
  use crate::app::message::DetailMessage;

  fn fixture() -> State {
    let mut state = State::boot(false);
    state.full = Some(FullUi::default());
    state.kernel.client = Some(Arc::new(jellypilot_media_server::JellyfinClient::new()));
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
      },
    )));
    state
  }

  #[test]
  fn detail_confirmation_after_navigation_updates_the_hero_collection() {
    let mut state = fixture();
    drop(crate::app::update::update(
      &mut state,
      Message::Detail(DetailMessage::FavoriteToggled),
    ));
    let full = state.full.as_mut().unwrap();
    let token = full.detail.pending_user_data["series"].clone();
    super::super::detail::leave_view(&mut full.detail, &mut state.kernel);
    state.shell.destination = Destination::Home;
    assert!(controls(&state, Source::Hero).favorite_action.is_none());
    drop(crate::app::update::update(
      &mut state,
      Message::Detail(DetailMessage::UserDataUpdated {
        token,
        result: Ok(VideoUserDataUpdate {
          item_id: "series".to_owned(),
          favorite: true,
          played: false,
        }),
      }),
    ));
    let hero = controls(&state, Source::Hero);
    assert_eq!(hero.favorite, Some(true));
    assert!(matches!(hero.favorite_action, Some(Message::Collections(
      CollectionMessage::Favorite { item_id, favorite: false, .. }
    )) if item_id == "series"));
  }

  #[tokio::test]
  async fn detail_cannot_start_a_conflicting_write_while_collection_mutation_is_pending() {
    use iced::futures::StreamExt;
    let mut state = fixture();
    state
      .full
      .as_mut()
      .unwrap()
      .personal_lists
      .busy_items
      .insert("series".to_owned());
    let task =
      crate::app::update::update(&mut state, Message::Detail(DetailMessage::PlayedToggled));
    if let Some(mut stream) = iced_runtime::task::into_stream(task) {
      assert!(
        stream.next().await.is_none(),
        "conflicting mutation emitted work"
      );
    }
    assert!(controls(&state, Source::Hero).favorite_action.is_none());
  }
}
