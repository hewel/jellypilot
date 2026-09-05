//! Personal Lists presentation state and account-scoped asynchronous work.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use iced::Task;
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::artwork_loader::PlannedArtworkLoad;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::watchlist::{ProfileScope, WatchlistRecord, WatchlistStore};
use jellypilot_media_server::artwork::{
  ArtworkLoadObservation, ArtworkLoadSummary, ArtworkSizeClass, DerivedArtwork,
};
use jellypilot_media_server::{
  FavoritesPage, FavoritesPageRequest, VideoLibraryItem, VideoUserDataAction, VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
};

use super::artwork::stream_artwork_loads;
use super::kernel::Kernel;
use super::message::{ArtworkLoadCompletion, Message};
use super::state::{ArtworkCell, ArtworkCellState, NoticeLevel};

pub const PAGE_SIZE: usize = 24;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Route {
  #[default]
  Overview,
  Favorites,
  Watchlist,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
  Favorites,
  Watchlist,
}

/// Whether current server metadata has resolved a locally stored item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemAvailability {
  Available,
  /// A successful batch response omitted the item as missing or inaccessible.
  Unavailable,
  /// Current metadata is unknown because it has not loaded or the request failed.
  Unknown,
}

pub struct ListEntry {
  pub id: String,
  pub name: String,
  pub subtitle: String,
  pub item: Option<VideoLibraryItem>,
  pub availability: ItemAvailability,
}

#[derive(Default)]
pub struct ListPage {
  pub entries: Vec<ListEntry>,
  pub total: usize,
  pub loading: bool,
  pub error: Option<String>,
  pub offset: usize,
}

#[derive(Default)]
pub struct Surface {
  pub favorites: ListPage,
  pub watchlist: ListPage,
  pub watchlist_ids: HashSet<String>,
  pub busy_items: HashSet<String>,
  pub mutation_error: Option<String>,
  pub artwork: HashMap<String, ArtworkCell>,
  scope: Option<ProfileScope>,
  watchlist_records: Vec<WatchlistRecord>,
  store_revision: u64,
  favorites_generation: u64,
  watchlist_generation: u64,
  membership_generation: u64,
  mutations: HashMap<String, u64>,
}

#[derive(Default)]
struct RuntimeStore {
  store: Option<WatchlistStore>,
  revision: u64,
}

struct RuntimeInner {
  store: Mutex<RuntimeStore>,
  scope_epochs: Mutex<Vec<(ProfileScope, u64)>>,
  next_generation: AtomicU64,
}

/// Cloneable runtime boundary serializing all Watchlist file operations.
#[derive(Clone)]
pub struct Runtime {
  inner: Arc<RuntimeInner>,
}

impl Default for Runtime {
  fn default() -> Self {
    Self {
      inner: Arc::new(RuntimeInner {
        store: Mutex::new(RuntimeStore::default()),
        scope_epochs: Mutex::new(Vec::new()),
        next_generation: AtomicU64::new(0),
      }),
    }
  }
}

impl Runtime {
  fn next_generation(&self) -> u64 {
    self
      .inner
      .next_generation
      .fetch_add(1, Ordering::Relaxed)
      .wrapping_add(1)
  }

  /// Deletes local Watchlist records for exactly one signed-out profile.
  pub(crate) async fn remove_scope(&self, scope: ProfileScope) -> Result<usize, String> {
    self.invalidate_scope(&scope);
    self
      .run_store(move |store| {
        let removed = store
          .store
          .as_mut()
          .expect("store is initialized before operations")
          .remove_scope(&scope)
          .map_err(|error| error.to_string())?;
        if removed > 0 {
          store.revision = store.revision.wrapping_add(1);
        }
        Ok(removed)
      })
      .await
  }

  async fn snapshot(&self, scope: ProfileScope) -> Result<(u64, Vec<WatchlistRecord>), String> {
    self
      .run_store(move |store| {
        let records = store
          .store
          .as_ref()
          .expect("store is initialized before operations")
          .records_for(&scope);
        Ok((store.revision, records))
      })
      .await
  }

  async fn set_membership(
    &self,
    scope: ProfileScope,
    item: VideoLibraryItem,
    should_add: bool,
    expected_scope_epoch: u64,
  ) -> Result<(u64, Vec<WatchlistRecord>), String> {
    let runtime = self.clone();
    self
      .run_store(move |store| {
        if !runtime.scope_epoch_is_current(&scope, expected_scope_epoch) {
          return Err("Watchlist operation was superseded.".to_owned());
        }
        let changed = {
          let watchlist = store
            .store
            .as_mut()
            .expect("store is initialized before operations");
          if should_add {
            let elapsed = SystemTime::now()
              .duration_since(UNIX_EPOCH)
              .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
            let added_at = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
            let record = WatchlistRecord::from_item(scope.clone(), &item, added_at)
              .map_err(|error| error.to_string())?;
            watchlist.add(record).map_err(|error| error.to_string())?
          } else {
            watchlist
              .remove(&scope, &item.id)
              .map_err(|error| error.to_string())?
          }
        };
        if changed {
          store.revision = store.revision.wrapping_add(1);
        }
        let records = store
          .store
          .as_ref()
          .expect("store is initialized before operations")
          .records_for(&scope);
        Ok((store.revision, records))
      })
      .await
  }

  async fn remove_item(
    &self,
    scope: ProfileScope,
    item_id: String,
    expected_scope_epoch: u64,
  ) -> Result<(u64, Vec<WatchlistRecord>), String> {
    let runtime = self.clone();
    self
      .run_store(move |store| {
        if !runtime.scope_epoch_is_current(&scope, expected_scope_epoch) {
          return Err("Watchlist operation was superseded.".to_owned());
        }
        let changed = store
          .store
          .as_mut()
          .expect("store is initialized before operations")
          .remove(&scope, &item_id)
          .map_err(|error| error.to_string())?;
        if changed {
          store.revision = store.revision.wrapping_add(1);
        }
        let records = store
          .store
          .as_ref()
          .expect("store is initialized before operations")
          .records_for(&scope);
        Ok((store.revision, records))
      })
      .await
  }

  async fn run_store<T, F>(&self, operation: F) -> Result<T, String>
  where
    T: Send + 'static,
    F: FnOnce(&mut RuntimeStore) -> Result<T, String> + Send + 'static,
  {
    let runtime = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || {
      let mut state = runtime.store.lock().unwrap_or_else(PoisonError::into_inner);
      if state.store.is_none() {
        state.store = Some(WatchlistStore::load().map_err(|error| error.to_string())?);
      }
      operation(&mut state)
    })
    .await
    .map_err(|error| format!("Watchlist worker failed: {error}"))?
  }

  fn scope_epoch(&self, scope: &ProfileScope) -> u64 {
    self
      .inner
      .scope_epochs
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .iter()
      .find(|(candidate, _)| candidate == scope)
      .map_or(0, |(_, epoch)| *epoch)
  }

  fn invalidate_scope(&self, scope: &ProfileScope) {
    let epoch = self.next_generation();
    let mut epochs = self
      .inner
      .scope_epochs
      .lock()
      .unwrap_or_else(PoisonError::into_inner);
    if let Some((_, existing)) = epochs.iter_mut().find(|(candidate, _)| candidate == scope) {
      *existing = epoch;
    } else {
      epochs.push((scope.clone(), epoch));
    }
  }

  fn scope_epoch_is_current(&self, scope: &ProfileScope, expected: u64) -> bool {
    self.scope_epoch(scope) == expected
  }

  #[cfg(test)]
  pub(crate) fn for_test(store: WatchlistStore) -> Self {
    Self {
      inner: Arc::new(RuntimeInner {
        store: Mutex::new(RuntimeStore {
          store: Some(store),
          revision: 0,
        }),
        scope_epochs: Mutex::new(Vec::new()),
        next_generation: AtomicU64::new(0),
      }),
    }
  }
}

#[derive(Clone)]
pub enum PersonalListsMessage {
  Retry(Kind),
  NextPage(Kind),
  PreviousPage(Kind),
  ToggleWatchlist(VideoLibraryItem),
  RemoveWatchlist(String),
  RemoveFavorite(VideoLibraryItem),
  FavoritesLoaded {
    session: SessionToken,
    generation: u64,
    scope: ProfileScope,
    result: Result<FavoritesPage, String>,
  },
  MembershipLoaded {
    session: SessionToken,
    generation: u64,
    scope: ProfileScope,
    result: Result<(u64, Vec<WatchlistRecord>), String>,
  },
  WatchlistMetadataLoaded {
    session: SessionToken,
    generation: u64,
    scope: ProfileScope,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  WatchlistMutationFinished {
    session: SessionToken,
    operation: u64,
    scope: ProfileScope,
    item_id: String,
    result: Result<(u64, Vec<WatchlistRecord>), String>,
  },
  FavoriteRemovalFinished {
    session: SessionToken,
    operation: u64,
    scope: ProfileScope,
    item_id: String,
    result: Result<VideoUserDataUpdate, String>,
  },
  ArtworkLoaded {
    session: SessionToken,
    slot: jellypilot_core::artwork_binder::ArtworkSlot,
    image_id: String,
    result: Result<
      jellypilot_media_server::artwork::ArtworkRaster,
      jellypilot_media_server::artwork::ArtworkError,
    >,
  },
}

/// Starts the requested Personal Lists route.
pub fn start(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  route: Route,
) -> Task<Message> {
  let scope = match prepare_scope(surface, kernel, runtime) {
    Ok(scope) => scope,
    Err(error) => {
      settle_missing_connection(surface, error);
      return Task::none();
    }
  };

  match route {
    Route::Overview => {
      surface.favorites.offset = 0;
      surface.watchlist.offset = 0;
      Task::batch([
        load_favorites(surface, kernel, runtime, scope.clone()),
        load_membership_for_scope(surface, kernel, runtime, scope),
      ])
    }
    Route::Favorites => load_favorites(surface, kernel, runtime, scope),
    Route::Watchlist => load_membership_for_scope(surface, kernel, runtime, scope),
  }
}

/// Refreshes server state without re-reading or changing local membership.
pub fn refresh(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  _route: Route,
) -> Task<Message> {
  let scope = match prepare_scope(surface, kernel, runtime) {
    Ok(scope) => scope,
    Err(error) => {
      settle_missing_connection(surface, error);
      return Task::none();
    }
  };
  Task::batch([
    load_favorites(surface, kernel, runtime, scope.clone()),
    load_watchlist_metadata(surface, kernel, runtime, scope),
  ])
}

/// Invalidates work tied to the route being left and releases its artwork bindings.
pub fn leave_view(surface: &mut Surface, kernel: &mut Kernel) {
  surface.favorites_generation = 0;
  surface.watchlist_generation = 0;
  surface.mutations.clear();
  surface.busy_items.clear();
  surface.favorites.loading = false;
  surface.watchlist.loading = false;
  begin_artwork_view(surface, kernel);
}

/// Loads device-local membership for the connected account.
pub fn load_membership(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
) -> Task<Message> {
  let scope = match prepare_scope(surface, kernel, runtime) {
    Ok(scope) => scope,
    Err(error) => {
      surface.watchlist.loading = false;
      surface.watchlist.error = Some(error);
      return Task::none();
    }
  };
  load_membership_for_scope(surface, kernel, runtime, scope)
}

pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  message: PersonalListsMessage,
) -> Task<Message> {
  match message {
    PersonalListsMessage::Retry(Kind::Favorites) => {
      let Some(scope) = current_scope(surface, kernel) else {
        return Task::none();
      };
      load_favorites(surface, kernel, runtime, scope)
    }
    PersonalListsMessage::Retry(Kind::Watchlist) => {
      let Some(scope) = current_scope(surface, kernel) else {
        return Task::none();
      };
      if surface.watchlist_records.is_empty() {
        load_membership_for_scope(surface, kernel, runtime, scope)
      } else {
        load_watchlist_metadata(surface, kernel, runtime, scope)
      }
    }
    PersonalListsMessage::NextPage(kind) => change_page(surface, kernel, runtime, kind, true),
    PersonalListsMessage::PreviousPage(kind) => change_page(surface, kernel, runtime, kind, false),
    PersonalListsMessage::ToggleWatchlist(item) => mutate_watchlist(surface, kernel, runtime, item),
    PersonalListsMessage::RemoveWatchlist(item_id) => {
      let Some(item_id) = nonempty(item_id) else {
        return Task::none();
      };
      mutate_watchlist_by_id(surface, kernel, runtime, item_id)
    }
    PersonalListsMessage::RemoveFavorite(item) => remove_favorite(surface, kernel, runtime, item),
    PersonalListsMessage::FavoritesLoaded {
      session,
      generation,
      scope,
      result,
    } => {
      if !settlement_is_current(
        surface,
        kernel,
        Kind::Favorites,
        session,
        generation,
        &scope,
      ) {
        return Task::none();
      }
      surface.favorites.loading = false;
      match result {
        Ok(page) => {
          if apply_favorites_page(&mut surface.favorites, page) {
            return load_favorites(surface, kernel, runtime, scope);
          }
        }
        Err(error) => surface.favorites.error = Some(error),
      }
      prepare_artwork(surface, kernel)
    }
    PersonalListsMessage::MembershipLoaded {
      session,
      generation,
      scope,
      result,
    } => {
      if generation == 0
        || generation != surface.membership_generation
        || !kernel.request_gate.is_current_session(session)
        || surface.scope.as_ref() != Some(&scope)
        || active_scope(kernel).ok().as_ref() != Some(&scope)
      {
        return Task::none();
      }
      surface.membership_generation = 0;
      match result {
        Ok((revision, records)) => {
          if !apply_store_snapshot(surface, revision, records) {
            return Task::none();
          }
          load_watchlist_metadata(surface, kernel, runtime, scope)
        }
        Err(error) => {
          surface.watchlist.loading = false;
          surface.watchlist.error = Some(error);
          Task::none()
        }
      }
    }
    PersonalListsMessage::WatchlistMetadataLoaded {
      session,
      generation,
      scope,
      result,
    } => {
      if !settlement_is_current(
        surface,
        kernel,
        Kind::Watchlist,
        session,
        generation,
        &scope,
      ) {
        return Task::none();
      }
      apply_watchlist_metadata(&mut surface.watchlist, result);
      prepare_artwork(surface, kernel)
    }
    PersonalListsMessage::WatchlistMutationFinished {
      session,
      operation,
      scope,
      item_id,
      result,
    } => settle_watchlist_mutation(
      surface,
      kernel,
      runtime,
      WatchlistMutationSettlement {
        session,
        operation,
        scope,
        item_id,
        result,
      },
    ),
    PersonalListsMessage::FavoriteRemovalFinished {
      session,
      operation,
      scope,
      item_id,
      result,
    } => settle_favorite_removal(
      surface,
      kernel,
      runtime,
      FavoriteRemovalSettlement {
        session,
        operation,
        scope,
        item_id,
        result,
      },
    ),
    PersonalListsMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = kernel.request_gate.is_current_session(session);
      apply_artwork_completion(
        surface,
        kernel,
        session_ok,
        ArtworkLoadCompletion {
          slot,
          image_id,
          result,
        },
      );
      Task::none()
    }
  }
}

fn prepare_scope(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
) -> Result<ProfileScope, String> {
  let scope = active_scope(kernel)?;
  if surface.scope.as_ref() != Some(&scope) {
    reset_for_scope(surface, kernel, runtime, scope.clone());
  }
  Ok(scope)
}

fn active_scope(kernel: &Kernel) -> Result<ProfileScope, String> {
  let client = kernel
    .client
    .as_ref()
    .ok_or_else(|| "The connected media server session is unavailable.".to_owned())?;
  let connection = client.login().connection_state();
  if !connection.connected {
    return Err("The connected media server session is unavailable.".to_owned());
  }
  let server_url = connection
    .server_url
    .ok_or_else(|| "The connected server address is unavailable.".to_owned())?;
  let user_id = connection
    .user_id
    .ok_or_else(|| "The connected server user is unavailable.".to_owned())?;
  ProfileScope::new(connection.provider, server_url, user_id).map_err(|error| error.to_string())
}

fn current_scope(surface: &Surface, kernel: &Kernel) -> Option<ProfileScope> {
  let scope = active_scope(kernel).ok()?;
  (surface.scope.as_ref() == Some(&scope)).then_some(scope)
}

fn reset_for_scope(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  scope: ProfileScope,
) {
  begin_artwork_view(surface, kernel);
  *surface = Surface {
    scope: Some(scope),
    favorites_generation: runtime.next_generation(),
    watchlist_generation: runtime.next_generation(),
    ..Surface::default()
  };
}

fn settle_missing_connection(surface: &mut Surface, error: String) {
  surface.favorites.loading = false;
  surface.watchlist.loading = false;
  surface.favorites.error = Some(error.clone());
  surface.watchlist.error = Some(error);
}

fn load_favorites(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  scope: ProfileScope,
) -> Task<Message> {
  let generation = runtime.next_generation();
  surface.favorites_generation = generation;
  surface.favorites.loading = true;
  surface.favorites.error = None;
  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.favorites.loading = false;
    surface.favorites.error = Some("The connected media server session is unavailable.".to_owned());
    return Task::none();
  };
  let start_index = i32::try_from(surface.favorites.offset).unwrap_or(i32::MAX);
  Task::perform(
    async move {
      client
        .library()
        .favorites(FavoritesPageRequest {
          start_index,
          limit: PAGE_SIZE as i32,
        })
        .await
        .map_err(|error| error.to_string())
    },
    move |result| {
      Message::PersonalLists(PersonalListsMessage::FavoritesLoaded {
        session,
        generation,
        scope,
        result,
      })
    },
  )
}

fn load_membership_for_scope(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  scope: ProfileScope,
) -> Task<Message> {
  let generation = runtime.next_generation();
  surface.watchlist_generation = generation;
  surface.membership_generation = generation;
  surface.watchlist.loading = true;
  surface.watchlist.error = None;
  let session = kernel.request_gate.current_session();
  let worker = runtime.clone();
  let task_scope = scope.clone();
  Task::perform(
    async move { worker.snapshot(task_scope).await },
    move |result| {
      Message::PersonalLists(PersonalListsMessage::MembershipLoaded {
        session,
        generation,
        scope,
        result,
      })
    },
  )
}

fn load_watchlist_metadata(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  scope: ProfileScope,
) -> Task<Message> {
  let generation = runtime.next_generation();
  surface.watchlist_generation = generation;
  surface.watchlist.error = None;
  rebuild_watchlist_page(surface);
  if surface.watchlist.entries.is_empty() {
    surface.watchlist.loading = false;
    return prepare_artwork(surface, kernel);
  }
  surface.watchlist.loading = true;
  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.watchlist.loading = false;
    surface.watchlist.error = Some("The connected media server session is unavailable.".to_owned());
    return Task::none();
  };
  let item_ids = surface
    .watchlist
    .entries
    .iter()
    .map(|entry| entry.id.clone())
    .collect();
  Task::perform(
    async move {
      client
        .library()
        .video_items_by_ids(item_ids)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| {
      Message::PersonalLists(PersonalListsMessage::WatchlistMetadataLoaded {
        session,
        generation,
        scope,
        result,
      })
    },
  )
}

fn change_page(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  kind: Kind,
  forward: bool,
) -> Task<Message> {
  let Some(scope) = current_scope(surface, kernel) else {
    return Task::none();
  };
  let page = match kind {
    Kind::Favorites => &mut surface.favorites,
    Kind::Watchlist => &mut surface.watchlist,
  };
  if page.loading {
    return Task::none();
  }
  let new_offset = if forward {
    let candidate = page.offset.saturating_add(PAGE_SIZE);
    if candidate >= page.total {
      return Task::none();
    }
    candidate
  } else {
    page.offset.saturating_sub(PAGE_SIZE)
  };
  if new_offset == page.offset {
    return Task::none();
  }
  page.offset = new_offset;
  match kind {
    Kind::Favorites => load_favorites(surface, kernel, runtime, scope),
    Kind::Watchlist => load_watchlist_metadata(surface, kernel, runtime, scope),
  }
}

fn mutate_watchlist(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  item: VideoLibraryItem,
) -> Task<Message> {
  let Some(scope) = current_scope(surface, kernel) else {
    return Task::none();
  };
  let item_id = item.id.trim().to_owned();
  if item_id.is_empty() || surface.busy_items.contains(&item_id) {
    return Task::none();
  }
  let should_add = !surface.watchlist_ids.contains(&item_id);
  begin_mutation(surface, runtime, &item_id);
  let operation = surface.mutations[&item_id];
  let session = kernel.request_gate.current_session();
  let worker = runtime.clone();
  let scope_epoch = runtime.scope_epoch(&scope);
  let result_scope = scope.clone();
  let result_id = item_id.clone();
  Task::perform(
    async move {
      worker
        .set_membership(scope, item, should_add, scope_epoch)
        .await
    },
    move |result| {
      Message::PersonalLists(PersonalListsMessage::WatchlistMutationFinished {
        session,
        operation,
        scope: result_scope,
        item_id: result_id,
        result,
      })
    },
  )
}

fn mutate_watchlist_by_id(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  item_id: String,
) -> Task<Message> {
  let Some(scope) = current_scope(surface, kernel) else {
    return Task::none();
  };
  if surface.busy_items.contains(&item_id) {
    return Task::none();
  }
  begin_mutation(surface, runtime, &item_id);
  let operation = surface.mutations[&item_id];
  let session = kernel.request_gate.current_session();
  let worker = runtime.clone();
  let scope_epoch = runtime.scope_epoch(&scope);
  let result_scope = scope.clone();
  let result_id = item_id.clone();
  Task::perform(
    async move { worker.remove_item(scope, item_id, scope_epoch).await },
    move |result| {
      Message::PersonalLists(PersonalListsMessage::WatchlistMutationFinished {
        session,
        operation,
        scope: result_scope,
        item_id: result_id,
        result,
      })
    },
  )
}

fn begin_mutation(surface: &mut Surface, runtime: &Runtime, item_id: &str) {
  let operation = runtime.next_generation();
  surface.mutations.insert(item_id.to_owned(), operation);
  surface.busy_items.insert(item_id.to_owned());
  surface.mutation_error = None;
  surface.watchlist_generation = 0;
  surface.membership_generation = 0;
  surface.watchlist.loading = false;
}

struct WatchlistMutationSettlement {
  session: SessionToken,
  operation: u64,
  scope: ProfileScope,
  item_id: String,
  result: Result<(u64, Vec<WatchlistRecord>), String>,
}

fn settle_watchlist_mutation(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  settlement: WatchlistMutationSettlement,
) -> Task<Message> {
  let WatchlistMutationSettlement {
    session,
    operation,
    scope,
    item_id,
    result,
  } = settlement;
  let session_ok = kernel.request_gate.is_current_session(session);
  let scope_ok =
    surface.scope.as_ref() == Some(&scope) && active_scope(kernel).ok() == Some(scope.clone());
  let operation_ok = surface.mutations.get(&item_id) == Some(&operation);
  if !session_ok || !scope_ok || !operation_ok {
    if session_ok && active_scope(kernel).ok().as_ref() == Some(&scope) {
      if let Err(error) = &result {
        return kernel.show_toast(
          NoticeLevel::Error,
          format!("Could not update Watchlist: {error}"),
        );
      }
    }
    if session_ok && scope_ok && result.is_ok() {
      return load_membership_for_scope(surface, kernel, runtime, scope);
    }
    return Task::none();
  }
  surface.mutations.remove(&item_id);
  surface.busy_items.remove(&item_id);
  match result {
    Ok((revision, records)) => {
      if apply_store_snapshot(surface, revision, records) {
        load_watchlist_metadata(surface, kernel, runtime, scope)
      } else {
        Task::none()
      }
    }
    Err(error) => {
      let error = format!("Could not update Watchlist: {error}");
      surface.mutation_error = Some(error.clone());
      kernel.show_toast(NoticeLevel::Error, error)
    }
  }
}

fn remove_favorite(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  item: VideoLibraryItem,
) -> Task<Message> {
  let Some(scope) = current_scope(surface, kernel) else {
    return Task::none();
  };
  let item_id = item.id.trim().to_owned();
  if item_id.is_empty() || surface.busy_items.contains(&item_id) {
    return Task::none();
  }
  let operation = runtime.next_generation();
  surface.mutations.insert(item_id.clone(), operation);
  surface.busy_items.insert(item_id.clone());
  surface.mutation_error = None;
  surface.favorites_generation = 0;
  surface.favorites.loading = false;
  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.mutations.remove(&item_id);
    surface.busy_items.remove(&item_id);
    surface.mutation_error = Some("The connected media server session is unavailable.".to_owned());
    return Task::none();
  };
  let result_scope = scope.clone();
  let result_id = item_id.clone();
  Task::perform(
    async move {
      client
        .library()
        .update_user_data(VideoUserDataUpdateRequest {
          item_id,
          action: VideoUserDataAction::Unfavorite,
        })
        .await
        .map_err(|error| error.to_string())
    },
    move |result| {
      Message::PersonalLists(PersonalListsMessage::FavoriteRemovalFinished {
        session,
        operation,
        scope: result_scope,
        item_id: result_id,
        result,
      })
    },
  )
}

struct FavoriteRemovalSettlement {
  session: SessionToken,
  operation: u64,
  scope: ProfileScope,
  item_id: String,
  result: Result<VideoUserDataUpdate, String>,
}

fn settle_favorite_removal(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  settlement: FavoriteRemovalSettlement,
) -> Task<Message> {
  let FavoriteRemovalSettlement {
    session,
    operation,
    scope,
    item_id,
    result,
  } = settlement;
  let session_ok = kernel.request_gate.is_current_session(session);
  let scope_ok =
    surface.scope.as_ref() == Some(&scope) && active_scope(kernel).ok() == Some(scope.clone());
  let operation_ok = surface.mutations.get(&item_id) == Some(&operation);
  if !session_ok || !scope_ok || !operation_ok {
    if session_ok && active_scope(kernel).ok().as_ref() == Some(&scope) {
      let error = match &result {
        Err(error) => Some(format!("Could not remove favorite: {error}")),
        Ok(update) if update.favorite => {
          Some("The server did not remove this favorite.".to_owned())
        }
        Ok(_) => None,
      };
      if let Some(error) = error {
        return kernel.show_toast(NoticeLevel::Error, error);
      }
    }
    if session_ok && scope_ok && result.is_ok() {
      return load_favorites(surface, kernel, runtime, scope);
    }
    return Task::none();
  }
  surface.mutations.remove(&item_id);
  surface.busy_items.remove(&item_id);
  match result {
    Ok(update) if !update.favorite => {
      surface
        .favorites
        .entries
        .retain(|entry| entry.id != item_id);
      surface.favorites.total = surface.favorites.total.saturating_sub(1);
      surface.favorites.offset = surface
        .favorites
        .offset
        .min((surface.favorites.total.saturating_sub(1) / PAGE_SIZE) * PAGE_SIZE);
      for entry in &mut surface.watchlist.entries {
        if let Some(item) = entry.item.as_mut().filter(|item| item.id == item_id) {
          item.favorite = false;
        }
      }
      load_favorites(surface, kernel, runtime, scope)
    }
    Ok(_) => {
      let error = "The server did not remove this favorite.".to_owned();
      surface.mutation_error = Some(error.clone());
      kernel.show_toast(NoticeLevel::Error, error)
    }
    Err(error) => {
      let error = format!("Could not remove favorite: {error}");
      surface.mutation_error = Some(error.clone());
      kernel.show_toast(NoticeLevel::Error, error)
    }
  }
}

fn settlement_is_current(
  surface: &Surface,
  kernel: &Kernel,
  kind: Kind,
  session: SessionToken,
  generation: u64,
  scope: &ProfileScope,
) -> bool {
  let expected_generation = match kind {
    Kind::Favorites => surface.favorites_generation,
    Kind::Watchlist => surface.watchlist_generation,
  };
  generation != 0
    && generation == expected_generation
    && kernel.request_gate.is_current_session(session)
    && surface.scope.as_ref() == Some(scope)
    && active_scope(kernel).ok().as_ref() == Some(scope)
}

/// Returns true when the requested page disappeared and must be loaded again
/// at the corrected final-page offset.
fn apply_favorites_page(target: &mut ListPage, page: FavoritesPage) -> bool {
  let offset = usize::try_from(page.start_index.max(0)).unwrap_or(usize::MAX);
  let total = usize::try_from(page.total_record_count.max(0)).unwrap_or(usize::MAX);
  if total > 0 && offset >= total {
    target.entries.clear();
    target.offset = ((total - 1) / PAGE_SIZE) * PAGE_SIZE;
    target.total = total;
    target.error = None;
    return target.offset != offset;
  }
  target.offset = if total == 0 { 0 } else { offset };
  target.total = total;
  target.entries = page.items.into_iter().map(entry_from_item).collect();
  target.error = None;
  false
}

fn apply_store_snapshot(
  surface: &mut Surface,
  revision: u64,
  records: Vec<WatchlistRecord>,
) -> bool {
  if revision < surface.store_revision {
    return false;
  }
  surface.store_revision = revision;
  surface.watchlist_records = records;
  surface.watchlist_ids = surface
    .watchlist_records
    .iter()
    .map(|record| record.item_id().to_owned())
    .collect();
  clamp_watchlist_offset(surface);
  rebuild_watchlist_page(surface);
  true
}

fn clamp_watchlist_offset(surface: &mut Surface) {
  let total = surface.watchlist_records.len();
  if total == 0 {
    surface.watchlist.offset = 0;
  } else if surface.watchlist.offset >= total {
    surface.watchlist.offset = ((total - 1) / PAGE_SIZE) * PAGE_SIZE;
  }
}

fn rebuild_watchlist_page(surface: &mut Surface) {
  let previous = surface
    .watchlist
    .entries
    .drain(..)
    .map(|entry| (entry.id.clone(), entry))
    .collect::<HashMap<_, _>>();
  let offset = surface.watchlist.offset;
  surface.watchlist.total = surface.watchlist_records.len();
  surface.watchlist.entries = surface
    .watchlist_records
    .iter()
    .skip(offset)
    .take(PAGE_SIZE)
    .map(|record| {
      let mut entry = entry_from_record(record);
      if let Some(old) = previous.get(record.item_id()) {
        entry.item.clone_from(&old.item);
        entry.availability = old.availability;
        if let Some(item) = &entry.item {
          entry.name.clone_from(&item.name);
          entry.subtitle = subtitle_from_item(item);
        }
      }
      entry
    })
    .collect();
  surface.watchlist.error = None;
}

fn apply_watchlist_metadata(page: &mut ListPage, result: Result<Vec<VideoLibraryItem>, String>) {
  page.loading = false;
  let items = match result {
    Ok(items) => items,
    Err(error) => {
      page.error = Some(error);
      return;
    }
  };
  let mut items = items
    .into_iter()
    .map(|item| (item.id.clone(), item))
    .collect::<HashMap<_, _>>();
  for entry in &mut page.entries {
    if let Some(item) = items.remove(&entry.id) {
      entry.name.clone_from(&item.name);
      entry.subtitle = subtitle_from_item(&item);
      entry.item = Some(item);
      entry.availability = ItemAvailability::Available;
    } else {
      entry.item = None;
      entry.availability = ItemAvailability::Unavailable;
    }
  }
  page.error = None;
}

fn entry_from_item(item: VideoLibraryItem) -> ListEntry {
  ListEntry {
    id: item.id.clone(),
    name: item.name.clone(),
    subtitle: subtitle_from_item(&item),
    item: Some(item),
    availability: ItemAvailability::Available,
  }
}

fn entry_from_record(record: &WatchlistRecord) -> ListEntry {
  ListEntry {
    id: record.item_id().to_owned(),
    name: record.name().to_owned(),
    subtitle: subtitle_from_record(record),
    item: None,
    availability: ItemAvailability::Unknown,
  }
}

fn subtitle_from_item(item: &VideoLibraryItem) -> String {
  if item.item_type.eq_ignore_ascii_case("Episode") {
    return episode_subtitle(
      item.series_name.as_deref(),
      item.season_number,
      item.episode_number,
    );
  }
  match item.production_year {
    Some(year) => format!("{} · {year}", item.item_type),
    None => item.item_type.clone(),
  }
}

fn subtitle_from_record(record: &WatchlistRecord) -> String {
  if record.item_type().eq_ignore_ascii_case("Episode") {
    episode_subtitle(
      record.series_name(),
      record.season_number(),
      record.episode_number(),
    )
  } else {
    record.item_type().to_owned()
  }
}

fn episode_subtitle(
  series_name: Option<&str>,
  season_number: Option<i32>,
  episode_number: Option<i32>,
) -> String {
  let series = series_name.unwrap_or("Episode");
  match (season_number, episode_number) {
    (Some(season), Some(episode)) => format!("{series} · S{season:02}E{episode:02}"),
    (None, Some(episode)) => format!("{series} · E{episode:02}"),
    _ => series.to_owned(),
  }
}

fn nonempty(value: String) -> Option<String> {
  let value = value.trim().to_owned();
  (!value.is_empty()).then_some(value)
}

fn prepare_artwork(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let mut seen = HashSet::new();
  let specs = surface
    .favorites
    .entries
    .iter()
    .chain(&surface.watchlist.entries)
    .filter_map(|entry| {
      let item = entry.item.as_ref()?;
      let image_id = list_artwork_image_id(item)?.to_owned();
      seen
        .insert(entry.id.clone())
        .then(|| (entry.id.clone(), image_id))
    })
    .collect::<Vec<_>>();
  let expected = specs.iter().cloned().collect::<HashMap<_, _>>();
  surface.artwork.retain(|item_id, cell| {
    expected
      .get(item_id)
      .is_some_and(|image_id| image_id == &cell.image_id)
  });

  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let adapter = Arc::clone(&kernel.artwork_adapter);
  let mut summary = ArtworkLoadSummary::default();
  let mut loads = Vec::new();
  for (index, (item_id, image_id)) in specs.into_iter().enumerate() {
    if let Some(cell) = surface.artwork.get(&item_id) {
      if cell.state == ArtworkCellState::Loading {
        continue;
      }
      if cell.state == ArtworkCellState::Ready
        && kernel
          .artwork_handles
          .get(cell.slot, &cell.image_id)
          .is_some()
      {
        continue;
      }
    }
    if let Some(raster) =
      adapter.cached_with_derived(&image_id, ArtworkSizeClass::Card, DerivedArtwork::default())
    {
      summary.record(&ArtworkLoadObservation::raster_hit(raster.byte_len() as u64));
      let slot = kernel.artwork_binder.bind_settled();
      kernel.artwork_handles.insert(
        slot,
        image_id.clone(),
        super::state::ArtworkHandles::from_raster(raster),
      );
      surface.artwork.insert(
        item_id,
        ArtworkCell {
          slot,
          image_id,
          state: ArtworkCellState::Ready,
        },
      );
      continue;
    }
    let slot = kernel.artwork_binder.bind(ArtworkSurface::PersonalLists);
    surface.artwork.insert(
      item_id,
      ArtworkCell {
        slot,
        image_id: image_id.clone(),
        state: ArtworkCellState::Loading,
      },
    );
    loads.push(PlannedArtworkLoad {
      slot,
      image_id,
      size_class: ArtworkSizeClass::Card,
      visible: index < 12,
      derived: DerivedArtwork::default(),
    });
  }
  stream_artwork_loads(
    adapter,
    client,
    session,
    loads,
    summary,
    |session, completion| {
      Message::PersonalLists(PersonalListsMessage::ArtworkLoaded {
        session,
        slot: completion.slot,
        image_id: completion.image_id,
        result: completion.result,
      })
    },
  )
}

fn list_artwork_image_id(item: &VideoLibraryItem) -> Option<&str> {
  if item.item_type.eq_ignore_ascii_case("Episode") {
    item
      .season_poster_image_id
      .as_deref()
      .or(item.series_poster_image_id.as_deref())
      .or(item.artwork_image_id.as_deref())
  } else {
    item.artwork_image_id.as_deref()
  }
}

fn apply_artwork_completion(
  surface: &mut Surface,
  kernel: &mut Kernel,
  session_ok: bool,
  completion: ArtworkLoadCompletion,
) {
  if kernel
    .artwork_binder
    .settle(completion.slot, ArtworkSurface::PersonalLists, session_ok)
    != ArtworkSettlement::Apply
  {
    return;
  }
  let Some(cell) = surface
    .artwork
    .values_mut()
    .find(|cell| cell.slot == completion.slot && cell.image_id == completion.image_id)
  else {
    return;
  };
  match completion.result {
    Ok(raster) => {
      cell.state = ArtworkCellState::Ready;
      kernel.artwork_handles.insert(
        completion.slot,
        completion.image_id,
        super::state::ArtworkHandles::from_raster(raster),
      );
    }
    Err(jellypilot_media_server::artwork::ArtworkError::Cancelled) => {}
    Err(_) => cell.state = ArtworkCellState::Failed,
  }
}

fn begin_artwork_view(surface: &mut Surface, kernel: &mut Kernel) {
  kernel
    .artwork_binder
    .begin_view(ArtworkSurface::PersonalLists);
  surface.artwork.clear();
}

#[cfg(test)]
mod tests {
  use super::*;

  fn item(id: &str, name: &str) -> VideoLibraryItem {
    VideoLibraryItem {
      id: id.to_owned(),
      name: name.to_owned(),
      item_type: "Movie".to_owned(),
      production_year: Some(2026),
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
    }
  }

  fn record(id: &str) -> WatchlistRecord {
    WatchlistRecord::from_item(
      ProfileScope::new(
        jellypilot_media_server::MediaServerProvider::Jellyfin,
        "https://media.example.test/",
        "user-1",
      )
      .expect("scope"),
      &item(id, id),
      1,
    )
    .expect("record")
  }

  #[test]
  fn runtime_generations_do_not_reset_with_surface_recreation() {
    let runtime = Runtime::default();
    let old = runtime.next_generation();
    let _recreated = Surface::default();
    let new = runtime.next_generation();

    assert_ne!(old, new);
    assert!(new > old);
  }

  #[test]
  fn scope_invalidation_supersedes_preexisting_store_operations() {
    let runtime = Runtime::default();
    let scope = ProfileScope::new(
      jellypilot_media_server::MediaServerProvider::Jellyfin,
      "https://media.example.test",
      "user-1",
    )
    .expect("scope");
    let old_epoch = runtime.scope_epoch(&scope);

    runtime.invalidate_scope(&scope);

    assert!(!runtime.scope_epoch_is_current(&scope, old_epoch));
  }

  #[test]
  fn metadata_failure_keeps_unknown_fallbacks() {
    let mut surface = Surface {
      watchlist_records: vec![record("one"), record("two")],
      ..Surface::default()
    };
    rebuild_watchlist_page(&mut surface);

    apply_watchlist_metadata(&mut surface.watchlist, Err("offline".to_owned()));

    assert!(surface
      .watchlist
      .entries
      .iter()
      .all(|entry| entry.availability == ItemAvailability::Unknown));
    assert_eq!(surface.watchlist.error.as_deref(), Some("offline"));
  }

  #[test]
  fn successful_subset_confirms_only_omitted_items_unavailable() {
    let mut surface = Surface {
      watchlist_records: vec![record("one"), record("two")],
      ..Surface::default()
    };
    rebuild_watchlist_page(&mut surface);

    apply_watchlist_metadata(&mut surface.watchlist, Ok(vec![item("one", "Current")]));

    assert_eq!(
      surface.watchlist.entries[0].availability,
      ItemAvailability::Available
    );
    assert_eq!(surface.watchlist.entries[0].name, "Current");
    assert_eq!(
      surface.watchlist.entries[1].availability,
      ItemAvailability::Unavailable
    );
  }

  #[test]
  fn older_store_snapshot_cannot_replace_newer_membership() {
    let mut surface = Surface {
      store_revision: 4,
      watchlist_records: vec![record("current")],
      ..Surface::default()
    };

    assert!(!apply_store_snapshot(
      &mut surface,
      3,
      vec![record("stale")]
    ));
    assert_eq!(surface.watchlist_records[0].item_id(), "current");
  }

  #[test]
  fn rebuilding_a_page_preserves_prior_authoritative_availability() {
    let mut surface = Surface {
      watchlist_records: vec![record("one")],
      ..Surface::default()
    };
    rebuild_watchlist_page(&mut surface);
    apply_watchlist_metadata(&mut surface.watchlist, Ok(Vec::new()));

    rebuild_watchlist_page(&mut surface);

    assert_eq!(
      surface.watchlist.entries[0].availability,
      ItemAvailability::Unavailable
    );
  }

  #[test]
  fn confirmed_last_page_removal_clamps_even_when_reload_fails() {
    let mut state = crate::app::state::State::boot(false);
    let client = Arc::new(jellypilot_media_server::JellyfinClient::new());
    let scope = record("last").scope().clone();
    client
      .login()
      .adopt_validated_session(&jellypilot_media_server::SavedSession {
        provider: scope.provider(),
        server_url: scope.server_url().to_owned(),
        user_id: scope.user_id().to_owned(),
        user_name: "User".to_owned(),
        access_token: "test-token".to_owned(),
        server_name: None,
        device_id: None,
      });
    state.kernel.client = Some(client);
    let runtime = Runtime::default();
    let mut surface = Surface {
      scope: Some(scope.clone()),
      favorites: ListPage {
        entries: vec![entry_from_item(item("last", "Last"))],
        total: PAGE_SIZE + 1,
        offset: PAGE_SIZE,
        ..ListPage::default()
      },
      ..Surface::default()
    };
    surface.mutations.insert("last".to_owned(), 9);
    let session = state.kernel.request_gate.current_session();
    drop(settle_favorite_removal(
      &mut surface,
      &mut state.kernel,
      &runtime,
      FavoriteRemovalSettlement {
        session,
        operation: 9,
        scope: scope.clone(),
        item_id: "last".to_owned(),
        result: Ok(VideoUserDataUpdate {
          item_id: "last".to_owned(),
          played: false,
          favorite: false,
        }),
      },
    ));
    assert_eq!(surface.favorites.offset, 0);
    let generation = surface.favorites_generation;
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::FavoritesLoaded {
        session,
        generation,
        scope,
        result: Err("offline".to_owned()),
      },
    ));
    assert_eq!(surface.favorites.total, PAGE_SIZE);
    assert_eq!(surface.favorites.offset, 0);
    assert!(surface.favorites.error.is_some());

    surface.favorites.entries = vec![entry_from_item(item("removed-elsewhere", "Old"))];
    surface.favorites.offset = PAGE_SIZE;
    assert!(apply_favorites_page(
      &mut surface.favorites,
      FavoritesPage {
        items: Vec::new(),
        start_index: PAGE_SIZE as i32,
        total_record_count: PAGE_SIZE as i32,
        limit: PAGE_SIZE as i32,
        has_more: false,
      }
    ));
    assert!(surface.favorites.entries.is_empty());

    leave_view(&mut surface, &mut state.kernel);
    let scope = active_scope(&state.kernel).expect("active scope");
    drop(settle_watchlist_mutation(
      &mut surface,
      &mut state.kernel,
      &runtime,
      WatchlistMutationSettlement {
        session,
        operation: 44,
        scope: scope.clone(),
        item_id: "late".to_owned(),
        result: Err("write failed".to_owned()),
      },
    ));
    assert!(state
      .kernel
      .active_toast
      .as_ref()
      .is_some_and(|toast| toast.message.contains("Could not update Watchlist")));
    drop(load_membership_for_scope(
      &mut surface,
      &mut state.kernel,
      &runtime,
      scope.clone(),
    ));
    let generation = surface.membership_generation;
    drop(load_watchlist_metadata(
      &mut surface,
      &mut state.kernel,
      &runtime,
      scope.clone(),
    ));
    leave_view(&mut surface, &mut state.kernel);
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::MembershipLoaded {
        session,
        generation,
        scope,
        result: Ok((50, vec![record("from-store")])),
      },
    ));
    assert!(surface.watchlist_ids.contains("from-store"));
  }

  #[test]
  fn watchlist_page_clamps_after_the_last_page_becomes_empty() {
    let mut surface = Surface {
      watchlist: ListPage {
        offset: PAGE_SIZE,
        ..ListPage::default()
      },
      watchlist_records: (0..PAGE_SIZE)
        .map(|index| record(&format!("item-{index}")))
        .collect(),
      ..Surface::default()
    };

    clamp_watchlist_offset(&mut surface);

    assert_eq!(surface.watchlist.offset, 0);
  }

  #[tokio::test]
  async fn runtime_test_store_supports_account_scoped_cleanup() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-personal-lists-{}-{}.json",
      std::process::id(),
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("test clock")
        .as_nanos()
    ));
    let store = WatchlistStore::for_test(path).expect("isolated store");
    let runtime = Runtime::for_test(store);
    let scope = ProfileScope::new(
      jellypilot_media_server::MediaServerProvider::Jellyfin,
      "https://media.example.test",
      "user-1",
    )
    .expect("scope");

    assert_eq!(runtime.remove_scope(scope).await.expect("cleanup"), 0);
  }
}
