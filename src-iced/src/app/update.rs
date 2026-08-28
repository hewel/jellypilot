use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use iced::widget::{image, operation};
use iced::Task;
use jellypilot_auth::login::{
  can_start_login, quick_connect_workflow, should_disconnect_after_forget, ConnectionPhase,
  LoginError, LoginEvent, QUICK_CONNECT_POLL_INTERVAL, QUICK_CONNECT_TIMEOUT,
};
use jellypilot_auth::AuthStore;
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::browse::fetch_browse_page;
use jellypilot_core::browse_model::{
  BrowseEffect, BrowsePageRequest, BrowsePageSettlement, BrowsePreferences, BrowseSource,
  LibraryBrowseView,
};
use jellypilot_core::config::{BrowseFilterSettings, LoginPrefill};
use jellypilot_core::request_gate::{HomeToken, RequestGate};
use jellypilot_media_server::home::{load_home_data, HomeDataResult};
use jellypilot_media_server::{
  Credentials, JellyfinClient, MediaServerProvider, VideoLibraryItem, VideoLibrarySortDirection,
};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use super::message::{
  BrowseMessage, HomeMessage, LoginMessage, Message, PasswordSubmission, ProtectedSavedSession,
  SensitiveSessionPayload, WindowMessage,
};
use super::state::{
  ArtworkCell, ArtworkCellState, BrowseArtwork, BrowseViewport, ConnectedIdentity, Destination,
  HomeArtwork, HomeSection, HomeState, LoginMethod, QuickConnectState, State,
};

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  match message {
    Message::Window(message) => update_window(state, message),
    Message::Login(message) => {
      let was_connected = state.connection == ConnectionPhase::Connected;
      let login_task = update_login(state, message).map(Message::Login);
      let is_connected = state.connection == ConnectionPhase::Connected;
      if !was_connected && is_connected {
        state.destination = Destination::Home;
        Task::batch([login_task, start_home_load(state)])
      } else {
        if was_connected && !is_connected {
          reset_connected_surface(state);
        }
        login_task
      }
    }
    Message::Home(message) => update_home(state, message),
    Message::Browse(message) => update_browse(state, message),
  }
}

fn update_window(state: &mut State, message: WindowMessage) -> Task<Message> {
  match message {
    WindowMessage::CloseRequested(id) => {
      cancel_quick_connect(state);
      iced::window::close(id)
    }
    WindowMessage::FrameRendered => {
      state.smoke = false;
      iced::exit()
    }
  }
}

fn update_home(state: &mut State, message: HomeMessage) -> Task<Message> {
  match message {
    HomeMessage::Navigate(destination) => navigate(state, destination),
    HomeMessage::Retry => start_home_load(state),
    HomeMessage::Loaded { token, result } => {
      if !settle_home(&mut state.home, &mut state.request_gate, token, result) {
        return Task::none();
      }
      prepare_home_artwork(state)
    }
    HomeMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = state.request_gate.is_current_session(session);
      if state
        .artwork_binder
        .settle(slot, ArtworkSurface::Home, session_ok)
        != ArtworkSettlement::Apply
      {
        return Task::none();
      }
      let Some(cell) = state.home_artwork.cell_mut(slot, &image_id) else {
        return Task::none();
      };
      match result {
        Ok(bytes) => {
          cell.state = ArtworkCellState::Ready;
          state.artwork_handles.insert(
            slot,
            image_id,
            image::Handle::from_bytes(bytes.as_slice().to_vec()),
          );
        }
        Err(_) => cell.state = ArtworkCellState::Failed,
      }
      Task::none()
    }
  }
}

fn start_home_load(state: &mut State) -> Task<Message> {
  state.home.begin_load();
  begin_home_artwork_view(state);
  state.artwork_adapter.cancel_pending();
  let token = state.request_gate.begin_home();
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    let error = "The connected media server session is unavailable.".to_owned();
    state.home.settle_video_home(Err(error.clone()));
    state.home.settle_shortcuts(Err(error));
    return Task::none();
  };

  Task::perform(load_home_data(client), move |result| {
    Message::Home(HomeMessage::Loaded { token, result })
  })
}

fn settle_home(
  home: &mut HomeState,
  request_gate: &mut RequestGate,
  token: HomeToken,
  result: HomeDataResult,
) -> bool {
  if !request_gate.finish_home(token) {
    return false;
  }
  let (video_home, shortcuts) = result;
  home.settle_video_home(video_home);
  home.settle_shortcuts(shortcuts);
  true
}

#[derive(Clone, Copy)]
enum ArtworkPlacement {
  Hero,
  Card(HomeSection),
}

struct ArtworkLoadSpec {
  placement: ArtworkPlacement,
  item_id: String,
  image_id: String,
}

fn prepare_home_artwork(state: &mut State) -> Task<Message> {
  let specs = home_artwork_specs(&state.home);
  begin_home_artwork_view(state);
  let session = state.request_gate.current_session();
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let adapter = Arc::clone(&state.artwork_adapter);
  let mut tasks = Vec::with_capacity(specs.len());

  for spec in specs {
    let slot = state.artwork_binder.bind(ArtworkSurface::Home);
    let cell = ArtworkCell {
      slot,
      image_id: spec.image_id.clone(),
      state: ArtworkCellState::Loading,
    };
    match spec.placement {
      ArtworkPlacement::Hero => state.home_artwork.insert_hero(spec.item_id, cell),
      ArtworkPlacement::Card(section) => {
        state.home_artwork.insert_card(section, spec.item_id, cell);
      }
    }

    let client = Arc::clone(&client);
    let adapter = Arc::clone(&adapter);
    let image_id = spec.image_id;
    let completion_image_id = image_id.clone();
    tasks.push(Task::perform(
      async move { adapter.load(&client, &image_id).await },
      move |result| {
        Message::Home(HomeMessage::ArtworkLoaded {
          session,
          slot,
          image_id: completion_image_id,
          result,
        })
      },
    ));
  }

  Task::batch(tasks)
}

fn home_artwork_specs(home: &HomeState) -> Vec<ArtworkLoadSpec> {
  let mut specs = Vec::new();
  if let Some(item) = home.featured_item() {
    push_artwork_spec(&mut specs, ArtworkPlacement::Hero, item);
  }
  for section in HomeSection::ALL {
    if let jellypilot_core::LoadState::Ready(items) = home.section(section) {
      for item in items {
        push_artwork_spec(&mut specs, ArtworkPlacement::Card(section), item);
      }
    }
  }
  specs
}

fn push_artwork_spec(
  specs: &mut Vec<ArtworkLoadSpec>,
  placement: ArtworkPlacement,
  item: &VideoLibraryItem,
) {
  if let Some(image_id) = &item.artwork_image_id {
    specs.push(ArtworkLoadSpec {
      placement,
      item_id: item.id.clone(),
      image_id: image_id.clone(),
    });
  }
}

fn begin_home_artwork_view(state: &mut State) {
  state.artwork_binder.begin_view(ArtworkSurface::Home);
  state.home_artwork.clear();
  state.artwork_handles.retain_slots(std::iter::empty());
}

fn leave_home_view(state: &mut State) {
  state.request_gate.begin_home();
  state.artwork_adapter.cancel_pending();
  begin_home_artwork_view(state);
}

fn navigate(state: &mut State, destination: Destination) -> Task<Message> {
  let was_home = state.destination == Destination::Home;
  let opens_home = destination == Destination::Home;
  if was_home && !opens_home {
    leave_home_view(state);
  } else if !was_home && opens_home {
    leave_browse_view(state);
  }
  if matches!(destination, Destination::Library { .. }) {
    state.search_input.clear();
  }
  state.destination = destination;

  if opens_home {
    start_home_load(state)
  } else {
    start_browse(state)
  }
}

fn update_browse(state: &mut State, message: BrowseMessage) -> Task<Message> {
  match message {
    BrowseMessage::SearchInputChanged(value) => {
      state.search_input = value;
      Task::none()
    }
    BrowseMessage::SearchSubmitted => {
      let query = state.search_input.trim();
      if query.is_empty() {
        return Task::none();
      }
      navigate(state, Destination::Search(query.to_owned()))
    }
    BrowseMessage::SortMenuToggled => {
      state.browse_sort_menu_open = !state.browse_sort_menu_open;
      Task::none()
    }
    BrowseMessage::SortMenuDismissed => {
      state.browse_sort_menu_open = false;
      Task::none()
    }
    BrowseMessage::SortChanged(sort) => {
      state.browse_sort_menu_open = false;
      persist_browse_filters(state, |filters| filters.with_sort(sort))
    }
    BrowseMessage::SortDirectionToggled => persist_browse_filters(state, |filters| {
      let direction = match filters.sort_direction() {
        VideoLibrarySortDirection::Ascending => VideoLibrarySortDirection::Descending,
        VideoLibrarySortDirection::Descending => VideoLibrarySortDirection::Ascending,
      };
      filters.with_sort_direction(direction)
    }),
    BrowseMessage::PlayedFilterChanged(played_filter) => {
      persist_browse_filters(state, |filters| filters.with_played_filter(played_filter))
    }
    BrowseMessage::FavoritesToggled => persist_browse_filters(state, |filters| {
      filters.with_favorites_only(!filters.favorites_only())
    }),
    BrowseMessage::Scrolled(viewport) => {
      let bounds = viewport.bounds();
      let content_bounds = viewport.content_bounds();
      let offset = viewport.absolute_offset();
      state.browse_viewport = BrowseViewport {
        offset_y: offset.y,
        height: bounds.height,
        content_height: content_bounds.height,
        width: bounds.width,
      };
      let is_fetching_more = matches!(
        state.browse_view,
        LibraryBrowseView::Ready {
          is_fetching_more: true,
          ..
        }
      );
      if should_load_next(
        state.browse_viewport,
        state.browse.can_load_more(),
        is_fetching_more,
      ) {
        load_next_browse_page(state)
      } else {
        Task::none()
      }
    }
    BrowseMessage::Retry => {
      let effects = match state.browse.retry() {
        Ok(effects) => effects,
        Err(error) => {
          state.notice = Some(format!("Could not retry library browsing: {error}"));
          return Task::none();
        }
      };
      sync_browse_view(state);
      apply_browse_effects(state, effects)
    }
    BrowseMessage::LoadPrevious => {
      let effects = match state.browse.load_previous() {
        Ok(effects) => effects,
        Err(error) => {
          state.notice = Some(format!("Could not load the previous library page: {error}"));
          return Task::none();
        }
      };
      sync_browse_view(state);
      Task::batch([
        apply_browse_effects(state, effects),
        prepare_browse_artwork(state),
      ])
    }
    BrowseMessage::PageSettled(settlement) => {
      if state.browse.is_current_settlement(&settlement) {
        state.browse_page_tasks.remove(&settlement.token);
      }
      let effects = match state.browse.settle(settlement) {
        Ok(effects) => effects,
        Err(error) => {
          state.notice = Some(format!("Could not apply library results: {error}"));
          return Task::none();
        }
      };
      sync_browse_view(state);
      Task::batch([
        apply_browse_effects(state, effects),
        prepare_browse_artwork(state),
      ])
    }
    BrowseMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = state.request_gate.is_current_session(session);
      if state
        .artwork_binder
        .settle(slot, ArtworkSurface::Browse, session_ok)
        != ArtworkSettlement::Apply
      {
        return Task::none();
      }
      let Some(cell) = state.browse_artwork.cell_mut(slot, &image_id) else {
        return Task::none();
      };
      match result {
        Ok(bytes) => {
          cell.state = ArtworkCellState::Ready;
          state.artwork_handles.insert(
            slot,
            image_id,
            image::Handle::from_bytes(bytes.as_slice().to_vec()),
          );
        }
        Err(_) => cell.state = ArtworkCellState::Failed,
      }
      Task::none()
    }
  }
}

fn persist_browse_filters(
  state: &mut State,
  mutation: impl FnOnce(BrowseFilterSettings) -> BrowseFilterSettings,
) -> Task<Message> {
  if !matches!(state.destination, Destination::Library { .. }) {
    return Task::none();
  }
  let filters = mutation(state.settings.snapshot().browse_filters());
  if let Err(error) = state.settings.set_browse_filters(filters) {
    state.notice = Some(format!("Could not save library filters: {error}"));
    return Task::none();
  }
  start_browse(state)
}

fn start_browse(state: &mut State) -> Task<Message> {
  let Some(source) = browse_source(state) else {
    abort_browse_pages(state);
    if let Err(error) = state.browse.reset() {
      state.notice = Some(format!("Could not reset library browsing: {error}"));
      return Task::none();
    }
    sync_browse_view(state);
    state.notice = Some("The selected library is no longer available.".to_owned());
    return Task::none();
  };
  let preferences = BrowsePreferences::from(state.settings.snapshot().browse_filters());
  let effects = match state.browse.configure_with_preferences(source, preferences) {
    Ok(effects) => effects,
    Err(error) => {
      state.notice = Some(format!("Could not open library browsing: {error}"));
      sync_browse_view(state);
      return Task::none();
    }
  };
  state.artwork_adapter.cancel_pending();
  begin_browse_artwork_view(state);
  sync_browse_view(state);
  Task::batch([
    apply_browse_effects(state, effects),
    prepare_browse_artwork(state),
  ])
}

fn browse_source(state: &State) -> Option<BrowseSource> {
  let session = state.request_gate.current_session();
  match &state.destination {
    Destination::Library {
      library_id,
      collection_type,
    } => {
      let jellypilot_core::LoadState::Ready(shortcuts) = &state.home.shortcuts else {
        return None;
      };
      shortcuts
        .iter()
        .find(|shortcut| shortcut.id == *library_id && shortcut.collection_type == *collection_type)
        .cloned()
        .map(|shortcut| BrowseSource::Library { session, shortcut })
    }
    Destination::Search(query) => Some(BrowseSource::Search {
      session,
      query: query.clone(),
    }),
    Destination::Home => None,
  }
}

fn load_next_browse_page(state: &mut State) -> Task<Message> {
  let effects = match state.browse.load_next() {
    Ok(effects) => effects,
    Err(error) => {
      state.notice = Some(format!("Could not load more library items: {error}"));
      return Task::none();
    }
  };
  sync_browse_view(state);
  Task::batch([
    apply_browse_effects(state, effects),
    prepare_browse_artwork(state),
  ])
}

fn sync_browse_view(state: &mut State) {
  state.browse_view = state.browse.view();
}

fn apply_browse_effects(state: &mut State, effects: Vec<BrowseEffect>) -> Task<Message> {
  // Viewport resets must land before page requests: Task::batch runs in
  // parallel, so a fast settlement could evaluate the stale near-tail offset
  // and advance another window before scroll-to-zero is applied.
  let mut resets = Vec::new();
  let mut tasks = Vec::with_capacity(effects.len());
  for effect in effects {
    match effect {
      BrowseEffect::ResetViewport => {
        state.browse_viewport.offset_y = 0.0;
        resets.push(operation::scroll_to(
          state.browse_scroll_id.clone(),
          operation::AbsoluteOffset { x: 0.0, y: 0.0 },
        ));
      }
      BrowseEffect::RequestPage(request) => {
        tasks.push(start_browse_page_request(state, request));
      }
      BrowseEffect::CancelPage { token } => {
        if let Some(handle) = state.browse_page_tasks.remove(&token) {
          handle.abort();
        }
      }
    }
  }
  let tasks = Task::batch(tasks);
  if resets.is_empty() {
    tasks
  } else {
    Task::batch(resets).chain(tasks)
  }
}

fn start_browse_page_request(state: &mut State, request: BrowsePageRequest) -> Task<Message> {
  let token = request.token;
  let failure_message = browse_failure_message(&request.source);
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    return Task::done(Message::Browse(BrowseMessage::PageSettled(
      BrowsePageSettlement {
        source_id: request.source_id,
        token,
        result: Err(failure_message.to_owned()),
      },
    )));
  };
  let (task, handle) = Task::perform(
    async move { fixed_browse_failure(fetch_browse_page(client, request).await, failure_message) },
    |settlement| Message::Browse(BrowseMessage::PageSettled(settlement)),
  )
  .abortable();
  state.browse_page_tasks.insert(token, handle);
  task
}

const fn browse_failure_message(source: &BrowseSource) -> &'static str {
  match source {
    BrowseSource::Library { .. } => "Could not load this library. Try again.",
    BrowseSource::Search { .. } => "Could not load these search results. Try again.",
  }
}

fn fixed_browse_failure(
  mut settlement: BrowsePageSettlement,
  failure_message: &'static str,
) -> BrowsePageSettlement {
  if settlement.result.is_err() {
    settlement.result = Err(failure_message.to_owned());
  }
  settlement
}

fn prepare_browse_artwork(state: &mut State) -> Task<Message> {
  let specs = match &state.browse_view {
    LibraryBrowseView::Ready { visible_items, .. } => visible_items
      .iter()
      .filter_map(|slot| slot.item.as_ref())
      .map(|item| (item.id.clone(), item.artwork_image_id.clone()))
      .collect::<Vec<_>>(),
    LibraryBrowseView::Inactive
    | LibraryBrowseView::Loading
    | LibraryBrowseView::Empty
    | LibraryBrowseView::Failed { .. } => Vec::new(),
  };
  let visible_ids = specs
    .iter()
    .map(|(item_id, _)| item_id.as_str())
    .collect::<HashSet<_>>();
  state.browse_artwork.retain_items(&visible_ids);
  drop(visible_ids);
  let session = state.request_gate.current_session();
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let adapter = Arc::clone(&state.artwork_adapter);
  let mut tasks = Vec::new();

  for (item_id, image_id) in specs {
    let Some(image_id) = image_id else {
      continue;
    };
    if state
      .browse_artwork
      .get(&item_id)
      .is_some_and(|cell| cell.image_id == image_id)
    {
      continue;
    }
    let slot = state.artwork_binder.bind(ArtworkSurface::Browse);
    state.browse_artwork.insert(
      item_id,
      ArtworkCell {
        slot,
        image_id: image_id.clone(),
        state: ArtworkCellState::Loading,
      },
    );
    let client = Arc::clone(&client);
    let adapter = Arc::clone(&adapter);
    let completion_image_id = image_id.clone();
    tasks.push(Task::perform(
      async move { adapter.load(&client, &image_id).await },
      move |result| {
        Message::Browse(BrowseMessage::ArtworkLoaded {
          session,
          slot,
          image_id: completion_image_id,
          result,
        })
      },
    ));
  }

  state
    .artwork_handles
    .retain_slots(state.browse_artwork.slots());
  Task::batch(tasks)
}

fn should_load_next(viewport: BrowseViewport, can_load_next: bool, is_fetching_more: bool) -> bool {
  if !can_load_next
    || is_fetching_more
    || !viewport.offset_y.is_finite()
    || !viewport.height.is_finite()
    || !viewport.content_height.is_finite()
    || viewport.height <= 0.0
    || viewport.content_height <= viewport.height
  {
    return false;
  }
  let viewport_end = viewport.offset_y.max(0.0) + viewport.height;
  let remaining = viewport.content_height - viewport_end;
  remaining <= viewport.height
}

fn begin_browse_artwork_view(state: &mut State) {
  state.artwork_binder.begin_view(ArtworkSurface::Browse);
  state.browse_artwork.clear();
  state.artwork_handles.retain_slots(std::iter::empty());
}

fn leave_browse_view(state: &mut State) {
  abort_browse_pages(state);
  state.artwork_adapter.cancel_pending();
  begin_browse_artwork_view(state);
  if let Err(error) = state.browse.reset() {
    state.notice = Some(format!("Could not reset library browsing: {error}"));
  }
  sync_browse_view(state);
}

fn abort_browse_pages(state: &mut State) {
  for (_, handle) in state.browse_page_tasks.drain() {
    handle.abort();
  }
}

fn reset_connected_surface(state: &mut State) {
  abort_browse_pages(state);
  state.artwork_adapter.reset_session();
  state.artwork_binder.reset();
  state.home_artwork = HomeArtwork::default();
  state.browse_artwork = BrowseArtwork::default();
  state.artwork_handles.clear();
  state.home = HomeState::default();
  if let Err(error) = state.browse.reset() {
    state.notice = Some(format!("Could not reset library browsing: {error}"));
  }
  state.browse_view = LibraryBrowseView::Inactive;
  state.destination = Destination::Home;
}

pub fn update_login(state: &mut State, message: LoginMessage) -> Task<LoginMessage> {
  match message {
    LoginMessage::ProviderSelected(provider) => {
      interrupt_quick_connect(state);
      state.login.select_provider(provider);
      state.login.error = None;
      Task::none()
    }
    LoginMessage::MethodSelected(method) => {
      if state.login.provider == MediaServerProvider::Jellyfin {
        if method == LoginMethod::Password {
          interrupt_quick_connect(state);
        }
        state.login.method = method;
        state.login.error = None;
      }
      Task::none()
    }
    LoginMessage::ServerUrlChanged(value) => {
      state.login.server_url = value;
      state.login.error = None;
      Task::none()
    }
    LoginMessage::UsernameChanged(value) => {
      state.login.username = value;
      state.login.error = None;
      Task::none()
    }
    LoginMessage::PasswordChanged(value) => {
      state.login.password = Zeroizing::new(value);
      state.login.error = None;
      Task::none()
    }
    LoginMessage::RememberToggled => {
      state.login.remember = !state.login.remember;
      Task::none()
    }
    LoginMessage::QuickConnectSubmitted => start_quick_connect(state),
    LoginMessage::QuickConnectCancelled => {
      cancel_quick_connect(state);
      state.connection = ConnectionPhase::SignedOut;
      state.login.reset_quick_connect();
      state.login.error = None;
      state.request_gate.disconnect();
      Task::none()
    }
    LoginMessage::PasswordSubmitted => start_password_login(state),
    LoginMessage::ProfilesLoaded { revision, result } => {
      state.login.profiles_loading = false;
      if revision != state.login.profiles_revision {
        return Task::none();
      }
      match result {
        Ok(profiles) => state.login.profiles = profiles,
        Err(error) => {
          state.login.error = Some(LoginError::AuthStorage(error).to_string());
        }
      }
      Task::none()
    }
    LoginMessage::WorkflowEvent(event) => handle_workflow_event(state, event),
    LoginMessage::PasswordFinished {
      session,
      client,
      result,
      submission,
    } => {
      if !state.request_gate.finish_login(session) {
        return Task::none();
      }
      match result {
        Ok(saved_session) => {
          let Some(saved_session) = saved_session.take() else {
            return Task::none();
          };
          complete_authentication(state, session, client, saved_session, Some(submission))
        }
        Err(error) => {
          fail_password_login(state, &error);
          Task::none()
        }
      }
    }
    LoginMessage::SavedSessionStored { session, result } => {
      let current = state.request_gate.is_current_session(session);
      match result {
        Ok((key, profiles)) => {
          state.login.profiles_revision = state.login.profiles_revision.wrapping_add(1);
          state.login.profiles = profiles;
          if current {
            state.active_profile = Some(key);
          }
        }
        Err(error) if current => {
          state.notice = Some(LoginError::AuthStorage(error).to_string());
        }
        Err(_) => {}
      }
      Task::none()
    }
    LoginMessage::RestoreProfile(key) => start_restore(state, key),
    LoginMessage::RestoreFinished {
      session,
      key,
      result,
    } => {
      if !state.request_gate.finish_login(session) {
        return Task::none();
      }
      if state.login.busy_profile.as_ref() == Some(&key) {
        state.login.busy_profile = None;
      }
      match result {
        Ok(saved_session) => {
          let Some(saved_session) = saved_session.take() else {
            return Task::none();
          };
          let client = Arc::new(JellyfinClient::new());
          client.login().adopt_validated_session(&saved_session);
          state.connection = ConnectionPhase::Connected;
          state.connected_identity = Some(ConnectedIdentity::from_session(&saved_session));
          state.client = Some(client);
          state.active_profile = Some(key);
          state.login.error = None;
        }
        Err(error) => fail_restore(state, &error),
      }
      Task::none()
    }
    LoginMessage::AskForgetProfile(key) => {
      if state.login.busy_profile.is_none() {
        state.login.forget_confirmation = Some(key);
      }
      Task::none()
    }
    LoginMessage::CancelForgetProfile => {
      state.login.forget_confirmation = None;
      Task::none()
    }
    LoginMessage::ConfirmForgetProfile(key) => start_forget(state, key).unwrap_or_else(Task::none),
    LoginMessage::ForgetFinished {
      session,
      key,
      sign_out,
      result,
    } => {
      if state.login.busy_profile.as_ref() == Some(&key) {
        state.login.busy_profile = None;
      }
      if state.login.forget_confirmation.as_ref() == Some(&key) {
        state.login.forget_confirmation = None;
      }
      let active_matches = state.active_profile.as_ref() == Some(&key);
      let disconnect = should_disconnect_after_forget(
        sign_out,
        session,
        state.request_gate.current_session(),
        state.connection,
        active_matches,
      );
      match result {
        Ok(profiles) => {
          state.login.profiles_revision = state.login.profiles_revision.wrapping_add(1);
          state.login.profiles = profiles;
          if disconnect {
            if let Some(client) = state.client.take() {
              client.login().disconnect();
            }
            state.request_gate.disconnect();
            state.connection = ConnectionPhase::SignedOut;
            state.connected_identity = None;
            state.active_profile = None;
          }
        }
        Err(error) => state.login.error = Some(LoginError::AuthStorage(error).to_string()),
      }
      Task::none()
    }
  }
}

pub fn load_saved_profiles(state: &State) -> Task<LoginMessage> {
  let store = state.auth_store.clone();
  let revision = state.login.profiles_revision;
  Task::perform(async move { store.load_profiles().await }, move |result| {
    LoginMessage::ProfilesLoaded { revision, result }
  })
}

fn start_quick_connect(state: &mut State) -> Task<LoginMessage> {
  if !can_start_login(state.connection) {
    return Task::none();
  }
  if state.login.provider != MediaServerProvider::Jellyfin {
    state.login.method = LoginMethod::Password;
    return Task::none();
  }
  let server_url = match validate_server_url(&state.login.server_url, state.login.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      state.login.error = Some(error);
      return Task::none();
    }
  };
  state.login.server_url = server_url.clone();

  cancel_quick_connect(state);
  let session = state.request_gate.begin_login();
  state.connection = ConnectionPhase::Connecting;
  state.login.quick_connect = QuickConnectState::Requesting;
  state.login.error = None;
  let client = Arc::new(JellyfinClient::new());
  let stream = iced::stream::channel(16, async move |sender| {
    let sender = Arc::new(Mutex::new(sender));
    quick_connect_workflow(
      client,
      server_url,
      session,
      move |event| {
        sender
          .lock()
          .is_ok_and(|mut sender| sender.try_send(event).is_ok())
      },
      QUICK_CONNECT_POLL_INTERVAL,
      QUICK_CONNECT_TIMEOUT,
    )
    .await;
  });
  let (task, handle) = Task::run(stream, LoginMessage::WorkflowEvent).abortable();
  state.quick_connect_task = Some(handle);
  task
}

fn start_password_login(state: &mut State) -> Task<LoginMessage> {
  if !can_start_login(state.connection) {
    return Task::none();
  }
  let server_url = match validate_server_url(&state.login.server_url, state.login.provider) {
    Ok(server_url) => server_url,
    Err(error) => {
      state.login.error = Some(error);
      return Task::none();
    }
  };
  state.login.server_url = server_url.clone();
  let username = state.login.username.trim().to_owned();
  if username.is_empty() {
    state.login.error = Some("Enter your username before signing in.".to_owned());
    return Task::none();
  }

  cancel_quick_connect(state);
  let session = state.request_gate.begin_login();
  state.connection = ConnectionPhase::Connecting;
  state.login.error = None;
  let client = Arc::new(JellyfinClient::new());
  let command_client = Arc::clone(&client);
  let submission = password_submission(state, server_url.clone(), username.clone());
  let credentials = AuthStore::protect_credentials(Credentials {
    provider: state.login.provider,
    server_url,
    username,
    password: std::mem::take(&mut *state.login.password),
  });
  Task::perform(
    async move {
      let result = async {
        let mut response = command_client
          .login()
          .authenticate(&credentials)
          .await
          .map_err(|_| LoginError::Request("Password authentication failed.".to_owned()))?;
        response.access_token.zeroize();
        jellypilot_auth::SensitiveSavedSession::from_client(&command_client)
          .map(ProtectedSavedSession::new)
          .ok_or_else(|| LoginError::Request("Password authentication failed.".to_owned()))
      }
      .await;
      (client, result)
    },
    move |(client, result)| LoginMessage::PasswordFinished {
      session,
      client,
      result,
      submission,
    },
  )
}

fn password_submission(state: &State, server_url: String, username: String) -> PasswordSubmission {
  PasswordSubmission {
    remember: state.login.remember,
    prefill: LoginPrefill::new(server_url, username),
    provider: state.login.provider,
  }
}

fn handle_workflow_event(state: &mut State, event: LoginEvent) -> Task<LoginMessage> {
  match event {
    LoginEvent::QuickConnectCode { session, code } => {
      if state.request_gate.is_current_login(session) {
        state.login.quick_connect = QuickConnectState::Waiting(code);
      }
      Task::none()
    }
    LoginEvent::QuickConnectApproving { session } => {
      if state.request_gate.is_current_login(session) {
        state.login.quick_connect = QuickConnectState::Approving;
      }
      Task::none()
    }
    LoginEvent::Login {
      session,
      client,
      result,
    } => {
      if !state.request_gate.finish_login(session) {
        return Task::none();
      }
      state.quick_connect_task = None;
      match result {
        Ok(()) => match jellypilot_auth::SensitiveSavedSession::from_client(&client) {
          Some(saved_session) => {
            complete_authentication(state, session, client, saved_session, None)
          }
          None => {
            fail_login(
              state,
              LoginError::Request("Quick Connect returned no session.".to_owned()),
            );
            Task::none()
          }
        },
        Err(error) => {
          fail_login(state, error);
          state.login.quick_connect = QuickConnectState::Failed;
          Task::none()
        }
      }
    }
    LoginEvent::SavedProfiles(result) => update_login(
      state,
      LoginMessage::ProfilesLoaded {
        revision: state.login.profiles_revision,
        result,
      },
    ),
    LoginEvent::SavedSessionStored { session, result } => {
      update_login(state, LoginMessage::SavedSessionStored { session, result })
    }
    LoginEvent::ForgotProfile {
      session,
      key,
      sign_out,
      result,
    } => update_login(
      state,
      LoginMessage::ForgetFinished {
        session,
        key,
        sign_out,
        result,
      },
    ),
  }
}

fn complete_authentication(
  state: &mut State,
  session: jellypilot_core::request_gate::SessionToken,
  client: Arc<JellyfinClient>,
  saved_session: SensitiveSessionPayload,
  submission: Option<PasswordSubmission>,
) -> Task<LoginMessage> {
  let identity = ConnectedIdentity::from_session(&saved_session);
  if let Some(submission) = submission {
    persist_password_submission(state, submission);
  }

  state.connection = ConnectionPhase::Connected;
  state.connected_identity = Some(identity);
  state.client = Some(client);
  state.login.password.clear();
  state.login.error = None;
  state.login.reset_quick_connect();
  let store = state.auth_store.clone();

  Task::perform(
    async move { store.save_session(saved_session).await },
    move |result| LoginMessage::SavedSessionStored { session, result },
  )
}

fn persist_password_submission(state: &mut State, submission: PasswordSubmission) {
  let settings_result = if submission.remember {
    state.settings.set_login_prefill(
      submission.prefill,
      provider_key(submission.provider).to_owned(),
    )
  } else {
    state.settings.clear_login_prefill()
  };
  if let Err(error) = settings_result {
    state.notice = Some(format!("Could not update remembered sign-in: {error}"));
  }
}

fn start_restore(state: &mut State, key: jellypilot_auth::SavedProfileKey) -> Task<LoginMessage> {
  interrupt_quick_connect(state);
  let session = state.request_gate.begin_login();
  state.connection = ConnectionPhase::Connecting;
  state.login.busy_profile = Some(key.clone());
  state.login.error = None;
  let store = state.auth_store.clone();
  Task::perform(
    async move {
      let result = async {
        let sensitive = store.load_session(key.clone()).await?;
        let candidate = JellyfinClient::for_saved_profile(&sensitive);
        candidate
          .login()
          .restore_session(&sensitive)
          .await
          .map_err(|_| LoginError::Request("Saved sign-in validation failed.".to_owned()))?;
        jellypilot_auth::SensitiveSavedSession::from_client(&candidate)
          .map(ProtectedSavedSession::new)
          .ok_or_else(|| LoginError::Request("Saved sign-in validation failed.".to_owned()))
      }
      .await;
      (key, result)
    },
    move |(key, result)| LoginMessage::RestoreFinished {
      session,
      key,
      result,
    },
  )
}

fn start_forget(
  state: &mut State,
  key: jellypilot_auth::SavedProfileKey,
) -> Option<Task<LoginMessage>> {
  if state.login.busy_profile.is_some() {
    return None;
  }
  state.login.forget_confirmation = None;
  state.login.busy_profile = Some(key.clone());
  let session = state.request_gate.current_session();
  let sign_out = state.active_profile.as_ref() == Some(&key);
  let store = state.auth_store.clone();
  Some(Task::perform(
    async move {
      let result = store.remove_profile(key.clone()).await;
      (key, result)
    },
    move |(key, result)| LoginMessage::ForgetFinished {
      session,
      key,
      sign_out,
      result,
    },
  ))
}

fn cancel_quick_connect(state: &mut State) {
  if let Some(handle) = state.quick_connect_task.take() {
    handle.abort();
  }
}

fn interrupt_quick_connect(state: &mut State) {
  if state.quick_connect_task.is_some()
    || !matches!(state.login.quick_connect, QuickConnectState::Idle)
  {
    cancel_quick_connect(state);
    state.request_gate.disconnect();
    state.connection = ConnectionPhase::SignedOut;
    state.login.reset_quick_connect();
  }
}

fn fail_login(state: &mut State, error: LoginError) {
  state.connection = ConnectionPhase::Failed;
  state.login.error = Some(error.to_string());
}

fn fail_password_login(state: &mut State, _error: &LoginError) {
  state.connection = ConnectionPhase::Failed;
  state.login.error =
    Some("Sign-in failed. Check your server, username, and password, then try again.".to_owned());
}

fn fail_restore(state: &mut State, _error: &LoginError) {
  state.connection = ConnectionPhase::Failed;
  state.login.error =
    Some("Could not restore this saved sign-in. Sign in again to refresh it.".to_owned());
}

fn provider_key(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "jellyfin",
    MediaServerProvider::Emby => "emby",
  }
}

fn validate_server_url(raw: &str, provider: MediaServerProvider) -> Result<String, String> {
  let server_url = raw.trim().trim_end_matches('/');
  let invalid = || format!("Enter a valid {} server URL.", provider_label(provider));
  if server_url.is_empty() || !raw_path_is_safe(server_url) {
    return Err(invalid());
  }
  let parsed = Url::parse(server_url).map_err(|_| invalid())?;
  if !matches!(parsed.scheme(), "http" | "https")
    || parsed.host_str().is_none()
    || !parsed.username().is_empty()
    || parsed.password().is_some()
    || parsed.query().is_some()
    || parsed.fragment().is_some()
    || !path_segments_are_safe(parsed.path())
  {
    return Err(invalid());
  }
  Ok(server_url.to_owned())
}

fn raw_path_is_safe(url: &str) -> bool {
  let without_fragment = url.split('#').next().unwrap_or_default();
  let without_query = without_fragment.split('?').next().unwrap_or_default();
  let path = without_query
    .split_once("://")
    .and_then(|(_, authority_and_path)| {
      authority_and_path
        .find('/')
        .map(|at| &authority_and_path[at..])
    })
    .unwrap_or(without_query);
  path_segments_are_safe(path)
}

fn path_segments_are_safe(path: &str) -> bool {
  !path.split('/').any(|segment| {
    let segment = segment.to_ascii_lowercase();
    segment.contains("%2f")
      || segment.contains("%5c")
      || matches!(segment.replace("%2e", ".").as_str(), "." | "..")
  })
}

fn provider_label(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "Jellyfin",
    MediaServerProvider::Emby => "Emby",
  }
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::path::PathBuf;

  use super::*;
  use crate::app::state::LoginState;
  use jellypilot_auth::{AuthStorageError, SavedProfileKey};
  use jellypilot_core::config::SettingsStore;

  fn test_state() -> State {
    let settings = SettingsStore::default();
    State {
      smoke: false,
      login: LoginState::from_settings(settings.snapshot()),
      settings,
      auth_store: AuthStore::default(),
      request_gate: Default::default(),
      client: None,
      connection: ConnectionPhase::SignedOut,
      connected_identity: None,
      active_profile: None,
      quick_connect_task: None,
      notice: None,
      destination: Destination::Home,
      home: HomeState::default(),
      artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      artwork_binder: Default::default(),
      home_artwork: HomeArtwork::default(),
      artwork_handles: Default::default(),
      browse: Default::default(),
      browse_view: LibraryBrowseView::Inactive,
      browse_artwork: Default::default(),
      browse_page_tasks: Default::default(),
      browse_viewport: BrowseViewport::default(),
      browse_scroll_id: iced::widget::Id::unique(),
      browse_sort_menu_open: false,
      search_input: String::new(),
    }
  }

  fn browse_request(effects: Vec<BrowseEffect>) -> BrowsePageRequest {
    effects
      .into_iter()
      .find_map(|effect| match effect {
        BrowseEffect::RequestPage(request) => Some(request),
        BrowseEffect::ResetViewport | BrowseEffect::CancelPage { .. } => None,
      })
      .expect("browse request should be emitted")
  }

  fn search_source(state: &State, query: &str) -> BrowseSource {
    BrowseSource::Search {
      session: state.request_gate.current_session(),
      query: query.to_owned(),
    }
  }

  #[test]
  fn identical_browse_resubmit_keeps_the_in_flight_request_handle() {
    let mut state = test_state();
    state.destination = Destination::Search("arrival".to_owned());
    let request = browse_request(
      state
        .browse
        .configure(search_source(&state, "arrival"))
        .expect("search should configure"),
    );
    let (_, handle) = Task::<Message>::none().abortable();
    state.browse_page_tasks.insert(request.token, handle);

    drop(start_browse(&mut state));

    assert!(state.browse_page_tasks.contains_key(&request.token));
    assert!(matches!(state.browse_view, LibraryBrowseView::Loading));
  }

  #[test]
  fn stale_same_session_settlement_keeps_the_reopened_request_handle() {
    let mut state = test_state();
    let source = search_source(&state, "arrival");
    let stale = browse_request(
      state
        .browse
        .configure(source.clone())
        .expect("first search should configure"),
    );
    state.browse.reset().expect("browse epoch should advance");
    let current = browse_request(
      state
        .browse
        .configure(source)
        .expect("search should reopen"),
    );
    let (_, handle) = Task::<Message>::none().abortable();
    state.browse_page_tasks.insert(current.token, handle);

    drop(update_browse(
      &mut state,
      BrowseMessage::PageSettled(BrowsePageSettlement {
        source_id: stale.source_id,
        token: stale.token,
        result: Err("stale server response".to_owned()),
      }),
    ));

    assert!(state.browse_page_tasks.contains_key(&current.token));
    assert!(matches!(state.browse_view, LibraryBrowseView::Loading));
  }

  #[test]
  fn browse_failure_messages_are_fixed_for_library_and_search_sources() {
    let state = test_state();
    let library = BrowseSource::Library {
      session: state.request_gate.current_session(),
      shortcut: jellypilot_media_server::VideoLibraryShortcut {
        id: "library-1".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: None,
        artwork_image_id: None,
      },
    };
    let search = search_source(&state, "arrival");

    assert_eq!(
      browse_failure_message(&library),
      "Could not load this library. Try again."
    );
    assert_eq!(
      browse_failure_message(&search),
      "Could not load these search results. Try again."
    );
    let settlement = fixed_browse_failure(
      BrowsePageSettlement {
        source_id: "source".to_owned(),
        token: jellypilot_core::LibraryBrowseLoadToken {
          generation: 1,
          sequence: 1,
        },
        result: Err("HTTP 500: raw server response body".to_owned()),
      },
      browse_failure_message(&search),
    );
    assert_eq!(
      settlement.result.as_ref().err().map(String::as_str),
      Some("Could not load these search results. Try again.")
    );
  }

  #[test]
  fn reset_viewport_effect_clears_the_recorded_scroll_offset() {
    let mut state = test_state();
    state.browse_viewport.offset_y = 640.0;

    drop(apply_browse_effects(
      &mut state,
      vec![BrowseEffect::ResetViewport],
    ));

    assert_eq!(state.browse_viewport.offset_y, 0.0);
  }

  #[test]
  fn stale_home_settlement_does_not_replace_the_current_loading_state() {
    let mut home = HomeState::default();
    let mut gate = RequestGate::default();
    let stale = gate.begin_home();
    let _current = gate.begin_home();
    home.begin_load();

    let applied = settle_home(
      &mut home,
      &mut gate,
      stale,
      (
        Err("stale home".to_owned()),
        Err("stale shortcuts".to_owned()),
      ),
    );

    assert!(matches!(
      (applied, &home.continue_watching, &home.shortcuts),
      (
        false,
        jellypilot_core::LoadState::Loading,
        jellypilot_core::LoadState::Loading
      )
    ));
  }

  #[test]
  fn tail_trigger_requires_an_approaching_idle_loadable_tail() {
    let approaching = BrowseViewport {
      offset_y: 500.0,
      height: 400.0,
      content_height: 1_200.0,
      width: 900.0,
    };
    let distant = BrowseViewport {
      offset_y: 100.0,
      ..approaching
    };

    assert!(should_load_next(approaching, true, false));
    assert!(!should_load_next(distant, true, false));
    assert!(!should_load_next(approaching, false, false));
    assert!(!should_load_next(approaching, true, true));
  }

  #[test]
  fn tail_trigger_rejects_content_that_does_not_overflow_the_viewport() {
    let viewport = BrowseViewport {
      offset_y: 0.0,
      height: 400.0,
      content_height: 400.0,
      width: 900.0,
    };

    assert!(!should_load_next(viewport, true, false));
  }

  struct TestSettingsFile(PathBuf);

  impl Drop for TestSettingsFile {
    fn drop(&mut self) {
      let _ = fs::remove_file(&self.0);
    }
  }

  fn isolated_settings(name: &str) -> (SettingsStore, TestSettingsFile) {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-settings-{}-{name}.json",
      std::process::id()
    ));
    let _ = fs::remove_file(&path);
    (
      SettingsStore::for_test(path.clone()),
      TestSettingsFile(path),
    )
  }

  fn profile_key(name: &str) -> SavedProfileKey {
    let server_url = format!("https://{name}.example.test");
    let user_id = format!("{name}-user-id");
    SavedProfileKey::for_identity(MediaServerProvider::Jellyfin, &server_url, &user_id)
  }

  #[test]
  fn invalid_server_url_is_rejected_before_a_login_token_is_created() {
    let mut state = test_state();
    state.login.server_url = "not a server".to_owned();
    let session_before = state.request_gate.current_session();

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));

    assert_eq!(state.request_gate.current_session(), session_before);
    assert_eq!(
      state.login.error.as_deref(),
      Some("Enter a valid Jellyfin server URL.")
    );
  }

  #[test]
  fn quick_connect_cancel_and_retry_reset_display_state_and_replace_request() {
    let mut state = test_state();
    state.login.server_url = "https://media.example.test".to_owned();
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    let first_session = state.request_gate.current_session();
    state.login.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectCancelled,
    ));
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    assert_eq!(state.login.quick_connect, QuickConnectState::Requesting);
    assert_ne!(state.request_gate.current_session(), first_session);
  }

  #[test]
  fn remembered_prefill_can_be_applied_and_cleared_without_display_state() {
    let mut state = test_state();
    state.login.apply_prefill(Some(LoginPrefill::new(
      "https://media.example.test".to_owned(),
      "ada".to_owned(),
    )));
    assert_eq!(state.login.username, "ada");

    state.login.apply_prefill(None);
    assert!(state.login.server_url.is_empty());
    assert!(state.login.username.is_empty());
    assert!(!state.login.remember);
  }

  #[test]
  fn selecting_emby_forces_password_and_hides_quick_connect_state() {
    let mut state = test_state();
    state.login.method = LoginMethod::QuickConnect;
    state.login.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());

    drop(update_login(
      &mut state,
      LoginMessage::ProviderSelected(MediaServerProvider::Emby),
    ));

    assert_eq!(state.login.method, LoginMethod::Password);
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);
  }

  #[test]
  fn stale_quick_connect_completion_does_not_clear_retry_abort_handle() {
    let mut state = test_state();
    state.login.server_url = "https://media.example.test".to_owned();
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    let stale_session = state.request_gate.current_session();
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectCancelled,
    ));
    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));

    drop(handle_workflow_event(
      &mut state,
      LoginEvent::Login {
        session: stale_session,
        client: Arc::new(JellyfinClient::new()),
        result: Err(LoginError::Request("stale failure".to_owned())),
      },
    ));

    assert!(state.quick_connect_task.is_some());
    assert!(state.connection == ConnectionPhase::Connecting);
    assert_eq!(state.login.quick_connect, QuickConnectState::Requesting);
  }

  #[test]
  fn stale_profile_load_is_rejected_after_session_storage_completes() {
    let mut state = test_state();
    let session = state.request_gate.current_session();
    let key = profile_key("new");

    drop(update_login(
      &mut state,
      LoginMessage::SavedSessionStored {
        session,
        result: Ok((key.clone(), Vec::new())),
      },
    ));
    drop(update_login(
      &mut state,
      LoginMessage::ProfilesLoaded {
        revision: 0,
        result: Err(AuthStorageError::Corrupt),
      },
    ));

    assert_eq!(state.login.profiles_revision, 1);
    assert_eq!(state.active_profile.as_ref(), Some(&key));
    assert!(state.login.error.is_none());
    assert!(!state.login.profiles_loading);
  }

  #[test]
  fn forget_result_is_applied_after_a_new_login_session_starts() {
    let mut state = test_state();
    let key = profile_key("forgotten");
    let forget_session = state.request_gate.begin_login();
    state.connection = ConnectionPhase::Connected;
    state.active_profile = Some(key.clone());
    state.login.busy_profile = Some(key.clone());
    state.login.forget_confirmation = Some(key.clone());
    let current_session = state.request_gate.begin_login();
    state.connection = ConnectionPhase::Connecting;

    drop(update_login(
      &mut state,
      LoginMessage::ForgetFinished {
        session: forget_session,
        key: key.clone(),
        sign_out: true,
        result: Ok(Vec::new()),
      },
    ));

    assert_eq!(state.request_gate.current_session(), current_session);
    assert_eq!(state.login.profiles_revision, 1);
    assert!(state.login.busy_profile.is_none());
    assert!(state.login.forget_confirmation.is_none());
    assert_eq!(state.active_profile.as_ref(), Some(&key));
    assert!(state.connection == ConnectionPhase::Connecting);
  }

  #[test]
  fn stale_restore_completion_does_not_clear_new_restore_busy_key() {
    let mut state = test_state();
    let first_key = profile_key("first");
    let second_key = profile_key("second");
    drop(start_restore(&mut state, first_key.clone()));
    let first_session = state.request_gate.current_session();
    drop(start_restore(&mut state, second_key.clone()));
    let second_session = state.request_gate.current_session();

    drop(update_login(
      &mut state,
      LoginMessage::RestoreFinished {
        session: first_session,
        key: first_key,
        result: Err(LoginError::Request("stale failure".to_owned())),
      },
    ));

    assert_eq!(state.request_gate.current_session(), second_session);
    assert_eq!(state.login.busy_profile.as_ref(), Some(&second_key));
    assert!(state.connection == ConnectionPhase::Connecting);
    assert!(state.login.error.is_none());
  }

  #[test]
  fn duplicate_forget_confirmation_returns_no_second_task_while_profile_is_busy() {
    let mut state = test_state();
    let key = profile_key("duplicate");
    state.login.forget_confirmation = Some(key.clone());

    let first_task = start_forget(&mut state, key.clone());
    assert!(first_task.is_some());
    drop(first_task);
    let second_task = start_forget(&mut state, key.clone());

    assert!(second_task.is_none());
    assert_eq!(state.login.busy_profile.as_ref(), Some(&key));
    assert!(state.login.forget_confirmation.is_none());
  }

  #[test]
  fn starting_restore_fully_interrupts_quick_connect_state() {
    let mut state = test_state();
    state.login.server_url = "https://media.example.test".to_owned();
    drop(start_quick_connect(&mut state));
    state.login.quick_connect = QuickConnectState::Waiting("ABC123".to_owned());
    let quick_connect_session = state.request_gate.current_session();
    let key = profile_key("restore");

    drop(start_restore(&mut state, key.clone()));

    assert_ne!(state.request_gate.current_session(), quick_connect_session);
    assert!(state.quick_connect_task.is_none());
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);
    assert_eq!(state.login.busy_profile.as_ref(), Some(&key));
  }

  #[test]
  fn login_submit_handlers_reject_requests_while_connecting() {
    let mut state = test_state();
    state.connection = ConnectionPhase::Connecting;
    state.login.server_url = "https://media.example.test".to_owned();
    state.login.username = "ada".to_owned();
    state.login.password = Zeroizing::new("secret".to_owned());
    let session = state.request_gate.current_session();

    drop(update_login(
      &mut state,
      LoginMessage::QuickConnectSubmitted,
    ));
    drop(update_login(&mut state, LoginMessage::PasswordSubmitted));

    assert_eq!(state.request_gate.current_session(), session);
    assert_eq!(state.login.password.as_str(), "secret");
    assert_eq!(state.login.quick_connect, QuickConnectState::Idle);
  }

  #[test]
  fn password_completion_persists_submitted_snapshot_after_form_edits() {
    let mut state = test_state();
    let (settings, _settings_file) = isolated_settings("password-snapshot");
    state.settings = settings;
    state.login.remember = true;
    state.login.provider = MediaServerProvider::Jellyfin;
    let submission = password_submission(
      &state,
      "https://submitted.example.test".to_owned(),
      "submitted-user".to_owned(),
    );

    state.login.server_url = "https://edited.example.test".to_owned();
    state.login.username = "edited-user".to_owned();
    state.login.remember = false;
    state.login.provider = MediaServerProvider::Emby;

    persist_password_submission(&mut state, submission);

    let persisted = state.settings.snapshot();
    assert!(persisted.remembers_login_prefill());
    assert_eq!(
      persisted.login_prefill().server_url(),
      "https://submitted.example.test"
    );
    assert_eq!(persisted.login_prefill().username(), "submitted-user");
    assert_eq!(persisted.login_provider(), "jellyfin");
  }

  #[test]
  fn password_and_restore_failures_use_fixed_user_messages() {
    let mut password_state = test_state();
    let password_session = password_state.request_gate.begin_login();
    password_state.connection = ConnectionPhase::Connecting;
    let submission = password_submission(
      &password_state,
      "https://media.example.test".to_owned(),
      "ada".to_owned(),
    );
    drop(update_login(
      &mut password_state,
      LoginMessage::PasswordFinished {
        session: password_session,
        client: Arc::new(JellyfinClient::new()),
        result: Err(LoginError::Request(
          "response included password=secret".to_owned(),
        )),
        submission,
      },
    ));

    let mut restore_state = test_state();
    let key = profile_key("restore-error");
    let restore_session = restore_state.request_gate.begin_login();
    restore_state.connection = ConnectionPhase::Connecting;
    restore_state.login.busy_profile = Some(key.clone());
    drop(update_login(
      &mut restore_state,
      LoginMessage::RestoreFinished {
        session: restore_session,
        key,
        result: Err(LoginError::Request(
          "response included access_token=secret".to_owned(),
        )),
      },
    ));

    assert_eq!(
      password_state.login.error.as_deref(),
      Some("Sign-in failed. Check your server, username, and password, then try again.")
    );
    assert_eq!(
      restore_state.login.error.as_deref(),
      Some("Could not restore this saved sign-in. Sign in again to refresh it.")
    );
  }
}
