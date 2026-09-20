//! Personal Lists presentation state and account-scoped asynchronous work.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::i18n::media::media_type;
use crate::i18n::{Localizer, UiText};
use iced::Task;
use jellypilot_core::item_actions::Action;
use jellypilot_core::request_gate::SessionToken;
use jellypilot_core::watchlist::{ProfileScope, WatchlistRecord, WatchlistStore};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::{
  FavoritesPage, FavoritesPageRequest, VideoLibraryItem, VideoUserDataUpdate,
};
use jellypilot_sdk::item_actions::{
  Admission, ItemActionError, WatchlistSnapshot, WatchlistWrite, WatchlistWriteAction,
};
use jellypilot_sdk::SdkError;

use super::artwork::{ImageCollection, ImageCompletion, ImageSpec};
use super::kernel::Kernel;
use super::message::Message;

pub const PAGE_SIZE: usize = 24;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Route {
  #[default]
  Overview,
  Favorites,
  Watchlist,
  History,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
  Favorites,
  Watchlist,
  History,
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
  pub subtitle: ListSubtitle,
  pub item: Option<VideoLibraryItem>,
  pub availability: ItemAvailability,
}

#[derive(Default)]
pub struct ListPage {
  pub entries: Vec<ListEntry>,
  pub total: usize,
  /// Exact visible count; History may remain unknown after local filtering.
  pub known_total: Option<usize>,
  pub next_offset: Option<usize>,
  pub previous_offsets: Vec<usize>,
  pub loading: bool,
  pub error: Option<UiText>,
  pub offset: usize,
}

#[derive(Default)]
pub struct Surface {
  pub favorites: ListPage,
  pub watchlist: ListPage,
  pub history: ListPage,
  pub watchlist_ids: HashSet<String>,
  pub(crate) membership_loaded: bool,
  pub mutation_error: Option<UiText>,
  pub artwork: ImageCollection,
  scope: Option<ProfileScope>,
  watchlist_records: Vec<WatchlistRecord>,
  store_revision: u64,
  favorites_generation: u64,
  watchlist_generation: u64,
  history_generation: u64,
  membership_generation: u64,
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
  pub(crate) fn next_generation(&self) -> u64 {
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

  /// Executes one admitted Watchlist write inside the blocking store worker.
  ///
  /// The admission moves into the worker itself, so dropping this future
  /// detaches the write instead of releasing the item early: settlement
  /// happens inside `finish_watchlist` once the real I/O ends, on success
  /// or failure. `expected_scope_epoch` is the caller's admission-time
  /// fence; a superseded write fails without touching membership.
  pub(crate) async fn apply_write(
    &self,
    admission: Admission,
    write: WatchlistWrite,
    expected_scope_epoch: u64,
  ) -> Result<WatchlistSnapshot, ItemActionError> {
    let runtime = self.clone();
    let worker = tokio::task::spawn_blocking(move || {
      let mut state = runtime
        .inner
        .store
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
      let result = runtime
        .write_locked(&mut state, admission.scope(), write, expected_scope_epoch)
        .map_err(SdkError::Storage);
      admission.finish_watchlist(result)
    });
    worker.await.map_err(|join| {
      if join.is_cancelled() {
        ItemActionError::Failed(SdkError::Closed)
      } else {
        ItemActionError::Failed(SdkError::Storage(format!(
          "Watchlist worker failed: {join}"
        )))
      }
    })?
  }

  /// Removal and its exact restoration record share the serialized write boundary.
  pub(crate) async fn remove_undoable(
    &self,
    admission: Admission,
    expected_scope_epoch: u64,
  ) -> Result<(WatchlistSnapshot, Option<WatchlistRecord>), ItemActionError> {
    let runtime = self.clone();
    tokio::task::spawn_blocking(move || {
      let mut state = runtime
        .inner
        .store
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
      let result = (|| {
        let store = loaded_store(&mut state.store).map_err(SdkError::Storage)?;
        if !runtime.scope_epoch_is_current(admission.scope(), expected_scope_epoch) {
          return Err(SdkError::Stale);
        }
        let removed = store
          .remove_items(admission.scope(), &[admission.item_id().to_owned()])
          .map_err(|error| SdkError::Storage(error.to_string()))?;
        let records = store.records_for(admission.scope());
        let changed = !removed.is_empty();
        if changed {
          state.revision = state.revision.wrapping_add(1);
        }
        Ok((
          WatchlistSnapshot {
            revision: state.revision,
            records,
            changed,
          },
          removed.into_iter().next(),
        ))
      })();
      match result {
        Ok((snapshot, record)) => admission
          .finish_watchlist(Ok(snapshot))
          .map(|snapshot| (snapshot, record)),
        Err(error) => admission
          .finish_watchlist(Err(error))
          .map(|snapshot| (snapshot, None)),
      }
    })
    .await
    .map_err(|error| ItemActionError::Failed(SdkError::Storage(error.to_string())))?
  }

  pub(crate) async fn restore_removed(
    &self,
    admission: Admission,
    record: WatchlistRecord,
    expected_scope_epoch: u64,
  ) -> Result<WatchlistSnapshot, ItemActionError> {
    let runtime = self.clone();
    tokio::task::spawn_blocking(move || {
      let mut state = runtime
        .inner
        .store
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
      let result = (|| {
        let store = loaded_store(&mut state.store).map_err(SdkError::Storage)?;
        if !runtime.scope_epoch_is_current(admission.scope(), expected_scope_epoch)
          || record.scope() != admission.scope()
          || record.item_id() != admission.item_id()
        {
          return Err(SdkError::Stale);
        }
        let changed = store
          .restore_items(&[record])
          .map_err(|error| SdkError::Storage(error.to_string()))?;
        let records = store.records_for(admission.scope());
        if changed {
          state.revision = state.revision.wrapping_add(1);
        }
        Ok(WatchlistSnapshot {
          revision: state.revision,
          records,
          changed,
        })
      })();
      admission.finish_watchlist(result)
    })
    .await
    .map_err(|error| ItemActionError::Failed(SdkError::Storage(error.to_string())))?
  }

  /// Applies `write` under the store lock: loads the store on first use,
  /// rejects superseded scope epochs, and advances the revision only when
  /// membership actually changed.
  fn write_locked(
    &self,
    state: &mut RuntimeStore,
    scope: &ProfileScope,
    write: WatchlistWrite,
    expected_scope_epoch: u64,
  ) -> Result<WatchlistSnapshot, String> {
    let store = loaded_store(&mut state.store)?;
    if !self.scope_epoch_is_current(scope, expected_scope_epoch) {
      return Err("Watchlist operation was superseded.".to_owned());
    }
    let changed = match write.action {
      WatchlistWriteAction::Add(item) => {
        let elapsed = SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?;
        let added_at = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        let record = WatchlistRecord::from_item(scope.clone(), &item, added_at)
          .map_err(|error| error.to_string())?;
        store.add(record).map_err(|error| error.to_string())?
      }
      WatchlistWriteAction::Remove => store
        .remove(scope, &write.item_id)
        .map_err(|error| error.to_string())?,
    };
    if changed {
      state.revision = state.revision.wrapping_add(1);
    }
    Ok(WatchlistSnapshot {
      revision: state.revision,
      records: store.records_for(scope),
      changed,
    })
  }

  async fn run_store<T, F>(&self, operation: F) -> Result<T, String>
  where
    T: Send + 'static,
    F: FnOnce(&mut RuntimeStore) -> Result<T, String> + Send + 'static,
  {
    let runtime = Arc::clone(&self.inner);
    tokio::task::spawn_blocking(move || {
      let mut state = runtime.store.lock().unwrap_or_else(PoisonError::into_inner);
      loaded_store(&mut state.store)?;
      operation(&mut state)
    })
    .await
    .map_err(|error| format!("Watchlist worker failed: {error}"))?
  }

  pub(crate) fn scope_epoch(&self, scope: &ProfileScope) -> u64 {
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

/// Returns the lazily loaded store, reading it from disk on first use.
fn loaded_store(store: &mut Option<WatchlistStore>) -> Result<&mut WatchlistStore, String> {
  if store.is_none() {
    *store = Some(WatchlistStore::load().map_err(|error| error.to_string())?);
  }
  Ok(store.as_mut().expect("store loaded above"))
}

#[derive(Clone)]
pub enum PersonalListsMessage {
  Retry(Kind),
  NextPage(Kind),
  PreviousPage(Kind),
  FavoritesLoaded {
    session: SessionToken,
    generation: u64,
    scope: ProfileScope,
    result: Result<FavoritesPage, String>,
  },
  HistoryLoaded {
    session: SessionToken,
    generation: u64,
    scope: ProfileScope,
    result: Result<jellypilot_sdk::DesktopHistoryPage, String>,
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
  ArtworkLoaded(ImageCompletion),
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
      for page in [
        &mut surface.favorites,
        &mut surface.watchlist,
        &mut surface.history,
      ] {
        if page.offset != 0 {
          page.entries.clear();
        }
        page.offset = 0;
        page.previous_offsets.clear();
      }
      Task::batch([
        load_favorites(surface, kernel, runtime, scope.clone()),
        load_membership_for_scope(surface, kernel, runtime, scope.clone()),
        load_history(surface, kernel, runtime, scope),
      ])
    }
    Route::Favorites => load_favorites(surface, kernel, runtime, scope),
    Route::Watchlist => load_membership_for_scope(surface, kernel, runtime, scope),
    Route::History => load_history(surface, kernel, runtime, scope),
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
    load_watchlist_metadata(surface, kernel, runtime, scope.clone()),
    load_history(surface, kernel, runtime, scope),
  ])
}

/// Invalidates work tied to the route being left and releases its artwork bindings.
pub fn leave_view(surface: &mut Surface) {
  surface.favorites_generation = 0;
  surface.watchlist_generation = 0;
  surface.history_generation = 0;
  surface.favorites.loading = false;
  surface.watchlist.loading = false;
  surface.history.loading = false;
  begin_artwork_view(surface);
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
    PersonalListsMessage::Retry(Kind::History) => {
      let Some(scope) = current_scope(surface, kernel) else {
        return Task::none();
      };
      load_history(surface, kernel, runtime, scope)
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
        Err(error) => surface.favorites.error = Some(list_failure("lists-favorites-error", &error)),
      }
      prepare_artwork(surface)
    }
    PersonalListsMessage::HistoryLoaded {
      session,
      generation,
      scope,
      result,
    } => {
      if !settlement_is_current(surface, kernel, Kind::History, session, generation, &scope) {
        return Task::none();
      }
      surface.history.loading = false;
      surface.history_generation = 0;
      match result {
        Ok(page) => {
          surface.history.known_total = page.total_record_count.map(|total| total.max(0) as usize);
          surface.history.next_offset = page
            .has_more
            .then_some(page.next_start_index.max(0) as usize);
          surface.history.entries = page.items.into_iter().map(entry_from_item).collect();
          surface.history.error = None;
          if surface.history.entries.is_empty() && !page.has_more {
            if let Some(previous) = surface.history.previous_offsets.pop() {
              surface.history.offset = previous;
              return load_history(surface, kernel, runtime, scope);
            }
          }
        }
        Err(error) => surface.history.error = Some(list_failure("lists-history-error", &error)),
      }
      prepare_artwork(surface)
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
          surface.watchlist.error = Some(list_failure("lists-watchlist-error", &error));
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
      prepare_artwork(surface)
    }
    PersonalListsMessage::ArtworkLoaded(completion) => {
      surface
        .artwork
        .settle(kernel.request_gate.current_session(), completion);
      Task::none()
    }
  }
}

fn prepare_scope(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
) -> Result<ProfileScope, UiText> {
  let scope = active_scope(kernel)?;
  if surface.scope.as_ref() != Some(&scope) {
    reset_for_scope(surface, runtime, scope.clone());
  }
  Ok(scope)
}

pub(crate) fn active_scope(kernel: &Kernel) -> Result<ProfileScope, UiText> {
  let client = kernel
    .client
    .as_ref()
    .ok_or_else(|| UiText::new("lists-session-error"))?;
  let connection = client.login().connection_state();
  if !connection.connected {
    return Err(UiText::new("lists-session-error"));
  }
  let server_url = connection
    .server_url
    .ok_or_else(|| UiText::new("lists-address-error"))?;
  let user_id = connection
    .user_id
    .ok_or_else(|| UiText::new("lists-user-error"))?;
  ProfileScope::new(connection.provider, server_url, user_id).map_err(|error| {
    tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(&error.to_string()), "Invalid personal list profile scope");
    UiText::new("lists-scope-error")
  })
}

pub(crate) fn current_scope(surface: &Surface, kernel: &Kernel) -> Option<ProfileScope> {
  let scope = active_scope(kernel).ok()?;
  (surface.scope.as_ref() == Some(&scope)).then_some(scope)
}

fn reset_for_scope(surface: &mut Surface, runtime: &Runtime, scope: ProfileScope) {
  begin_artwork_view(surface);
  *surface = Surface {
    scope: Some(scope),
    favorites_generation: runtime.next_generation(),
    watchlist_generation: runtime.next_generation(),
    ..Surface::default()
  };
}

fn settle_missing_connection(surface: &mut Surface, error: UiText) {
  surface.favorites.loading = false;
  surface.watchlist.loading = false;
  surface.history.loading = false;
  surface.history.error = Some(error.clone());
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
    surface.favorites.error = Some(UiText::new("lists-session-error"));
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

fn load_history(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  scope: ProfileScope,
) -> Task<Message> {
  let generation = runtime.next_generation();
  surface.history_generation = generation;
  surface.history.loading = true;
  surface.history.error = None;
  let session = kernel.request_gate.current_session();
  let sdk = Arc::clone(&kernel.sdk);
  let token = match sdk.new_operation_token() {
    Ok(token) => token,
    Err(error) => {
      surface.history.loading = false;
      surface.history.error = Some(list_failure("lists-history-error", &error.to_string()));
      return Task::none();
    }
  };
  let start_index = i32::try_from(surface.history.offset).unwrap_or(i32::MAX);
  Task::perform(
    async move {
      sdk
        .desktop_watch_history(token, start_index, PAGE_SIZE as i32)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| {
      Message::PersonalLists(PersonalListsMessage::HistoryLoaded {
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
    return prepare_artwork(surface);
  }
  surface.watchlist.loading = true;
  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.watchlist.loading = false;
    surface.watchlist.error = Some(UiText::new("lists-session-error"));
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
    Kind::History => &mut surface.history,
  };
  if page.loading {
    return Task::none();
  }
  let new_offset = if kind == Kind::History {
    if forward {
      let Some(next) = page.next_offset else {
        return Task::none();
      };
      page.previous_offsets.push(page.offset);
      next
    } else {
      let Some(previous) = page.previous_offsets.pop() else {
        return Task::none();
      };
      previous
    }
  } else if forward {
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
  page.entries.clear();
  begin_artwork_view(surface);
  match kind {
    Kind::Favorites => load_favorites(surface, kernel, runtime, scope),
    Kind::Watchlist => load_watchlist_metadata(surface, kernel, runtime, scope),
    Kind::History => load_history(surface, kernel, runtime, scope),
  }
}

/// Clears the inline mutation error when the coordinator admits a write.
/// In-flight reads stay viable: a failed write leaves their data current, and
/// an accepted write supersedes pending reads with fresh generations.
pub(crate) fn prepare_mutation(surface: &mut Surface) {
  surface.mutation_error = None;
}

/// Propagates accepted flags and supersedes every pending server read, so no
/// pre-write response can restore stale flags on another list.
pub(crate) fn apply_user_data_update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  update: &VideoUserDataUpdate,
  action: Action,
) -> Task<Message> {
  let Some(scope) = current_scope(surface, kernel) else {
    return Task::none();
  };
  let item_id = update.item_id.as_str();
  for entry in surface
    .favorites
    .entries
    .iter_mut()
    .chain(&mut surface.watchlist.entries)
    .chain(&mut surface.history.entries)
  {
    if let Some(item) = entry.item.as_mut().filter(|item| item.id == item_id) {
      item.favorite = update.favorite;
      item.played = update.played;
    }
  }
  match action {
    Action::Favorite(false) => remove_entry(&mut surface.favorites, item_id),
    Action::Played(false) => {
      surface.history.entries.retain(|entry| entry.id != item_id);
      surface.history.known_total = None;
    }
    _ => {}
  }
  let favorites = if surface.favorites.loading || matches!(action, Action::Favorite(_)) {
    load_favorites(surface, kernel, runtime, scope.clone())
  } else {
    Task::none()
  };
  let history = if surface.history.loading || matches!(action, Action::Played(_)) {
    load_history(surface, kernel, runtime, scope.clone())
  } else {
    Task::none()
  };
  let watchlist = if surface.watchlist.loading {
    load_watchlist_metadata(surface, kernel, runtime, scope)
  } else {
    Task::none()
  };
  Task::batch([favorites, history, watchlist])
}

/// Applies an accepted local Watchlist snapshot and refreshes the page's
/// server metadata. `revision` fences stale snapshots: older revisions never
/// overwrite newer membership.
pub(crate) fn apply_watchlist_snapshot(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  scope: &ProfileScope,
  revision: u64,
  records: Vec<WatchlistRecord>,
) -> Task<Message> {
  if active_scope(kernel).ok().as_ref() != Some(scope) {
    return Task::none();
  }
  if surface.scope.as_ref() != Some(scope) {
    reset_for_scope(surface, runtime, scope.clone());
  }
  if !apply_store_snapshot(surface, revision, records) {
    return Task::none();
  }
  load_watchlist_metadata(surface, kernel, runtime, scope.clone())
}

pub(crate) fn history_changed(
  surface: &mut Surface,
  kernel: &mut Kernel,
  runtime: &Runtime,
  hidden_item: Option<&str>,
) -> Task<Message> {
  if let Some(item_id) = hidden_item {
    surface.history.entries.retain(|entry| entry.id != item_id);
  }
  surface.history.known_total = None;
  let Some(scope) = current_scope(surface, kernel) else {
    return Task::none();
  };
  load_history(surface, kernel, runtime, scope)
}

/// Records the inline error for a rejected or failed mutation. Toast reporting
/// stays with the item-action coordinator.
pub(crate) fn mutation_failed(surface: &mut Surface, error: UiText) {
  surface.mutation_error = Some(error);
}

/// Drops `item_id` from a page whose membership the confirmed write ended and
/// clamps the offset so a now-empty final page falls back to the previous one.
fn remove_entry(page: &mut ListPage, item_id: &str) {
  if !page.entries.iter().any(|entry| entry.id == item_id) {
    return;
  }
  page.entries.retain(|entry| entry.id != item_id);
  page.total = page.total.saturating_sub(1);
  page.offset = page
    .offset
    .min((page.total.saturating_sub(1) / PAGE_SIZE) * PAGE_SIZE);
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
    Kind::History => surface.history_generation,
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
  apply_server_page(
    target,
    page.start_index,
    page.total_record_count,
    page.items,
  )
}

fn apply_server_page(
  target: &mut ListPage,
  start_index: i32,
  total_record_count: i32,
  items: Vec<VideoLibraryItem>,
) -> bool {
  let offset = usize::try_from(start_index.max(0)).unwrap_or(usize::MAX);
  let total = usize::try_from(total_record_count.max(0)).unwrap_or(usize::MAX);
  if total > 0 && offset >= total {
    target.entries.clear();
    target.offset = ((total - 1) / PAGE_SIZE) * PAGE_SIZE;
    target.total = total;
    target.known_total = Some(total);
    target.error = None;
    return target.offset != offset;
  }
  target.offset = if total == 0 { 0 } else { offset };
  target.total = total;
  target.known_total = Some(total);
  target.entries = items.into_iter().map(entry_from_item).collect();
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
  surface.membership_loaded = true;
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
  surface.watchlist.known_total = Some(surface.watchlist.total);
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
      page.error = Some(list_failure("lists-metadata-error", &error));
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

pub(crate) fn entry_from_item(item: VideoLibraryItem) -> ListEntry {
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

pub struct ListSubtitle {
  item_type: String,
  production_year: Option<i32>,
  series_name: Option<String>,
  season_number: Option<i32>,
  episode_number: Option<i32>,
}

impl ListSubtitle {
  pub fn text(&self, locale: Localizer) -> String {
    let kind = media_type(locale, &self.item_type);
    if self.item_type.eq_ignore_ascii_case("Episode") {
      let series = self.series_name.as_deref().unwrap_or(&kind);
      return match (self.season_number, self.episode_number) {
        (Some(season), Some(episode)) => format!("{series} · S{season:02}E{episode:02}"),
        (None, Some(episode)) => format!("{series} · E{episode:02}"),
        _ => series.to_owned(),
      };
    }
    match self.production_year {
      Some(year) => format!("{kind} · {year}"),
      None => kind,
    }
  }
}

fn subtitle_from_item(item: &VideoLibraryItem) -> ListSubtitle {
  ListSubtitle {
    item_type: item.item_type.clone(),
    production_year: item.production_year,
    series_name: item.series_name.clone(),
    season_number: item.season_number,
    episode_number: item.episode_number,
  }
}

fn subtitle_from_record(record: &WatchlistRecord) -> ListSubtitle {
  ListSubtitle {
    item_type: record.item_type().to_owned(),
    production_year: None,
    series_name: record.series_name().map(str::to_owned),
    season_number: record.season_number(),
    episode_number: record.episode_number(),
  }
}

fn list_failure(id: &'static str, error: &str) -> UiText {
  tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error), message_id = id, "Personal list operation failed");
  UiText::new(id)
}

fn prepare_artwork(surface: &mut Surface) -> Task<Message> {
  let specs = [
    (Kind::Favorites, &surface.favorites),
    (Kind::Watchlist, &surface.watchlist),
    (Kind::History, &surface.history),
  ]
  .into_iter()
  .flat_map(|(kind, page)| {
    page
      .entries
      .iter()
      .filter_map(move |entry| artwork_spec(kind, entry))
  })
  .collect::<Vec<_>>();
  surface.artwork.retain(&specs);
  Task::none()
}

pub(crate) fn artwork_key(kind: Kind, item_id: &str) -> String {
  let list = match kind {
    Kind::Favorites => "favorites",
    Kind::Watchlist => "watchlist",
    Kind::History => "history",
  };
  format!("{list}:{item_id}")
}

pub(crate) fn artwork_spec(kind: Kind, entry: &ListEntry) -> Option<ImageSpec> {
  let item = entry.item.as_ref()?;
  Some(ImageSpec {
    key: artwork_key(kind, &entry.id),
    image_id: match kind {
      Kind::Favorites => list_artwork_image_id(item),
      Kind::Watchlist | Kind::History => item
        .episode_thumb_image_id
        .as_deref()
        .or(item.backdrop_image_id.as_deref())
        .or(item.series_thumb_image_id.as_deref())
        .or(item.series_backdrop_image_id.as_deref())
        .or(item.artwork_image_id.as_deref()),
    }?
    .to_owned(),
    size_class: ArtworkSizeClass::Card,
    derived: DerivedArtwork::default(),
  })
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

fn begin_artwork_view(surface: &mut Surface) {
  surface.artwork.clear();
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn leaving_favorites_does_not_revoke_the_same_watchlist_image() {
    use jellypilot_core::image_lifecycle::{ImageLifecycle, ImagePriority, ImageStatus};

    let mut metadata = item("shared-item", "Shared");
    metadata.artwork_image_id = Some("shared-poster".to_owned());
    let entry = entry_from_item(metadata);
    let favorite = artwork_spec(Kind::Favorites, &entry).expect("favorite artwork");
    let watchlist = artwork_spec(Kind::Watchlist, &entry).expect("watchlist artwork");
    let mut lifecycle = ImageLifecycle::default();
    drop(lifecycle.observe(favorite.clone(), Some(ImagePriority::Visible)));
    drop(lifecycle.observe(watchlist.clone(), Some(ImagePriority::Visible)));
    drop(lifecycle.observe(favorite, None));
    assert_eq!(lifecycle.status(&watchlist.key), Some(ImageStatus::Loading));
  }

  fn item(id: &str, name: &str) -> VideoLibraryItem {
    VideoLibraryItem {
      premiere_date: None,
      id: id.to_owned(),
      name: name.to_owned(),
      item_type: "Movie".to_owned(),
      production_year: Some(2026),
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
    assert!(surface.watchlist.error.is_some());
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
    let session = state.kernel.request_gate.current_session();
    drop(apply_user_data_update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      &VideoUserDataUpdate {
        item_id: "last".to_owned(),
        played: false,
        favorite: false,
      },
      Action::Favorite(false),
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

    leave_view(&mut surface);
    let scope = active_scope(&state.kernel).expect("active scope");
    mutation_failed(&mut surface, UiText::new("lists-watchlist-update-error"));
    assert!(surface.mutation_error.is_some());
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
    leave_view(&mut surface);
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

  #[test]
  fn confirmation_supersedes_reads_across_lists_in_any_completion_order() {
    let mut state = connected_state();
    let runtime = Runtime::default();
    let scope = active_scope(&state.kernel).expect("scope");
    let session = state.kernel.request_gate.current_session();
    let mut surface = Surface {
      scope: Some(scope.clone()),
      watchlist_records: vec![record("one")],
      ..Surface::default()
    };
    drop(load_watchlist_metadata(
      &mut surface,
      &mut state.kernel,
      &runtime,
      scope.clone(),
    ));
    let stale_generation = surface.watchlist_generation;
    drop(apply_user_data_update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      &VideoUserDataUpdate {
        item_id: "one".to_owned(),
        favorite: true,
        played: true,
      },
      Action::Favorite(true),
    ));
    let current_generation = surface.watchlist_generation;
    let mut confirmed = item("one", "Current");
    confirmed.favorite = true;
    confirmed.played = true;
    let favorites_generation = surface.favorites_generation;
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::FavoritesLoaded {
        session,
        generation: favorites_generation,
        scope: scope.clone(),
        result: Ok(FavoritesPage {
          items: vec![confirmed.clone()],
          start_index: 0,
          total_record_count: 1,
          limit: PAGE_SIZE as i32,
          has_more: false,
        }),
      },
    ));
    // A fresh response on one list must not make an older response on another safe.
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::WatchlistMetadataLoaded {
        session,
        generation: stale_generation,
        scope: scope.clone(),
        result: Ok(vec![item("one", "Stale")]),
      },
    ));
    assert!(surface.watchlist.entries[0].item.is_none());
    assert!(surface.watchlist.loading);
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::WatchlistMetadataLoaded {
        session,
        generation: current_generation,
        scope,
        result: Ok(vec![confirmed]),
      },
    ));
    let loaded = surface.watchlist.entries[0]
      .item
      .as_ref()
      .expect("metadata");
    assert_eq!(loaded.name, "Current");
    assert!(loaded.favorite && loaded.played);
    assert!(!surface.watchlist.loading);
  }

  #[test]
  fn unplayed_write_removes_the_item_and_refreshes_history_cursor() {
    let mut state = connected_state();
    let runtime = Runtime::default();
    let mut surface = Surface {
      scope: Some(active_scope(&state.kernel).expect("scope")),
      history: ListPage {
        entries: vec![entry_from_item(item("watched", "Watched"))],
        known_total: Some(PAGE_SIZE + 1),
        offset: PAGE_SIZE,
        previous_offsets: vec![0],
        ..ListPage::default()
      },
      ..Surface::default()
    };

    drop(apply_user_data_update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      &VideoUserDataUpdate {
        item_id: "watched".to_owned(),
        played: false,
        favorite: false,
      },
      Action::Played(false),
    ));

    assert!(surface.history.entries.is_empty());
    assert_eq!(surface.history.offset, PAGE_SIZE);
    assert_eq!(surface.history.previous_offsets, vec![0]);
    assert!(surface.history.loading);
  }

  #[test]
  fn watchlist_snapshot_applies_without_initialized_surface_scope() {
    let mut state = connected_state();
    let runtime = Runtime::default();
    let mut surface = Surface::default();
    let scope = active_scope(&state.kernel).expect("scope");

    drop(apply_watchlist_snapshot(
      &mut surface,
      &mut state.kernel,
      &runtime,
      &scope,
      7,
      vec![record("one")],
    ));

    assert!(surface.watchlist_ids.contains("one"));
    assert_eq!(surface.store_revision, 7);
    assert_eq!(surface.scope.as_ref(), Some(&scope));

    // A stale revision never overwrites newer membership.
    drop(apply_watchlist_snapshot(
      &mut surface,
      &mut state.kernel,
      &runtime,
      &scope,
      3,
      Vec::new(),
    ));
    assert!(surface.watchlist_ids.contains("one"));
  }

  fn connected_state() -> crate::app::state::State {
    let mut state = crate::app::state::State::boot(false);
    let client = Arc::new(jellypilot_media_server::JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&jellypilot_media_server::SavedSession {
        provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
        server_url: "https://media.example.test/".to_owned(),
        user_id: "user-1".to_owned(),
        user_name: "User".to_owned(),
        access_token: "test-token".to_owned(),
        server_name: None,
        device_id: None,
      });
    state.kernel.client = Some(client);
    state
  }

  #[test]
  fn history_rejects_superseded_route_account_and_session_completions() {
    for boundary in ["new request", "leave", "account", "session"] {
      let mut state = connected_state();
      let runtime = Runtime::default();
      let mut surface = Surface::default();
      drop(start(
        &mut surface,
        &mut state.kernel,
        &runtime,
        Route::History,
      ));
      let session = state.kernel.request_gate.current_session();
      let generation = surface.history_generation;
      let scope = surface.scope.clone().expect("scope");
      match boundary {
        "new request" => {
          drop(load_history(
            &mut surface,
            &mut state.kernel,
            &runtime,
            scope.clone(),
          ));
        }
        "leave" => leave_view(&mut surface),
        "account" => {
          surface.scope = Some(
            ProfileScope::new(scope.provider(), scope.server_url(), "other-user")
              .expect("other scope"),
          )
        }
        "session" => state.kernel.request_gate.disconnect(),
        _ => unreachable!(),
      }
      drop(update(
        &mut surface,
        &mut state.kernel,
        &runtime,
        PersonalListsMessage::HistoryLoaded {
          session,
          generation,
          scope,
          result: Ok(jellypilot_sdk::DesktopHistoryPage {
            items: vec![item("stale", "Stale")],
            total_record_count: Some(1),
            start_index: 0,
            next_start_index: 1,
            limit: PAGE_SIZE as i32,
            has_more: false,
          }),
        },
      ));
      assert!(surface.history.entries.is_empty(), "{boundary}");
      assert_eq!(surface.history.total, 0, "{boundary}");
    }
  }

  #[test]
  fn history_page_shrink_reloads_correct_offset_and_keeps_retryable_errors() {
    let mut state = connected_state();
    let runtime = Runtime::default();
    let mut surface = Surface::default();
    drop(start(
      &mut surface,
      &mut state.kernel,
      &runtime,
      Route::History,
    ));
    let session = state.kernel.request_gate.current_session();
    let scope = surface.scope.clone().expect("scope");
    let mut watched = item("watched", "Watched");
    watched.last_played_date = Some("2026-09-08T20:15:00Z".to_owned());
    let generation = surface.history_generation;
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::HistoryLoaded {
        session,
        generation,
        scope: scope.clone(),
        result: Ok(jellypilot_sdk::DesktopHistoryPage {
          items: vec![watched],
          total_record_count: Some(25),
          start_index: 0,
          next_start_index: 24,
          limit: 24,
          has_more: true,
        }),
      },
    ));
    assert_eq!(
      surface.history.entries[0]
        .item
        .as_ref()
        .expect("watched item")
        .last_played_date
        .as_deref(),
      Some("2026-09-08T20:15:00Z")
    );
    drop(change_page(
      &mut surface,
      &mut state.kernel,
      &runtime,
      Kind::History,
      true,
    ));
    assert!(surface.history.entries.is_empty());
    assert!(surface.history.loading);
    let generation = surface.history_generation;
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::HistoryLoaded {
        session,
        generation,
        scope: scope.clone(),
        result: Ok(jellypilot_sdk::DesktopHistoryPage {
          items: Vec::new(),
          total_record_count: Some(24),
          start_index: 24,
          next_start_index: 24,
          limit: 24,
          has_more: false,
        }),
      },
    ));
    assert_eq!(surface.history.offset, 0);
    assert!(surface.history.loading);
    let generation = surface.history_generation;
    drop(update(
      &mut surface,
      &mut state.kernel,
      &runtime,
      PersonalListsMessage::HistoryLoaded {
        session,
        generation,
        scope,
        result: Err("server unavailable".to_owned()),
      },
    ));
    assert!(!surface.history.loading);
    assert!(surface.history.error.is_some());
    assert_eq!(surface.history.known_total, Some(24));
  }

  #[test]
  fn history_and_watchlist_artwork_do_not_share_admission_identity() {
    use jellypilot_core::image_lifecycle::{ImageLifecycle, ImagePriority, ImageStatus};
    let mut metadata = item("same", "Same");
    metadata.backdrop_image_id = Some("landscape".to_owned());
    let entry = entry_from_item(metadata);
    let history = artwork_spec(Kind::History, &entry).expect("history artwork");
    let watchlist = artwork_spec(Kind::Watchlist, &entry).expect("watchlist artwork");
    let mut lifecycle = ImageLifecycle::default();
    drop(lifecycle.observe(history.clone(), Some(ImagePriority::Visible)));
    drop(lifecycle.observe(watchlist.clone(), Some(ImagePriority::Visible)));
    drop(lifecycle.observe(history, None));
    assert_eq!(lifecycle.status(&watchlist.key), Some(ImageStatus::Loading));
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
