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
use jellypilot_core::detail::{
  apply_user_data_update, load_detail_content, load_season_neighbors, season_page_request,
  DetailContent,
};
use jellypilot_core::request_gate::{DetailAuxKind, DetailToken, HomeToken, RequestGate};
use jellypilot_media_server::home::{load_home_data, HomeDataResult};
use jellypilot_media_server::{
  Credentials, JellyfinClient, MediaServerProvider, VideoLibraryItem, VideoLibrarySortDirection,
  VideoSeason, VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest, VideoUserDataAction,
  VideoUserDataUpdate, VideoUserDataUpdateRequest,
};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use super::message::{
  BrowseMessage, DetailMessage, HomeMessage, LoginMessage, Message, PasswordSubmission,
  ProtectedSavedSession, SensitiveSessionPayload, WindowMessage,
};
use super::state::{
  ArtworkCell, ArtworkCellState, BrowseArtwork, BrowseViewport, ConnectedIdentity, Destination,
  DetailArtwork, DetailState, HomeArtwork, HomeSection, HomeState, LoginMethod, QuickConnectState,
  State, UserDataActionKind,
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
    Message::OpenDetail(item) => open_detail(state, item),
    Message::Detail(message) => update_detail(state, message),
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
  let previous = state.destination.clone();
  if !state.navigate_to(destination) {
    return Task::none();
  }
  activate_destination(state, previous)
}

fn open_detail(state: &mut State, item: VideoLibraryItem) -> Task<Message> {
  let item_id = item.id.clone();
  state.detail_items.insert(item_id.clone(), item);
  navigate(state, Destination::Detail(item_id))
}

fn navigate_back(state: &mut State) -> Task<Message> {
  let previous = state.destination.clone();
  if !state.navigate_back() {
    return Task::none();
  }
  activate_destination(state, previous)
}

fn activate_destination(state: &mut State, previous: Destination) -> Task<Message> {
  if previous == Destination::Home && state.destination != Destination::Home {
    leave_home_view(state);
  } else if matches!(
    previous,
    Destination::Library { .. } | Destination::Search(_)
  ) && !matches!(
    state.destination,
    Destination::Library { .. } | Destination::Search(_)
  ) {
    leave_browse_view(state);
  } else if matches!(previous, Destination::Detail(_)) && previous != state.destination {
    leave_detail_view(state);
  }

  match &state.destination {
    Destination::Home => start_home_load(state),
    Destination::Library { .. } => {
      state.search_input.clear();
      start_browse(state)
    }
    Destination::Search(_) => start_browse(state),
    Destination::Detail(_) => start_detail_load(state),
  }
}

const DETAIL_FAILURE: &str = "Could not load this item. Try again.";
const SEASON_FAILURE: &str = "Could not load this season. Try again.";
const USER_DATA_FAILURE: &str = "Could not update user data. Try again.";

fn update_detail(state: &mut State, message: DetailMessage) -> Task<Message> {
  match message {
    DetailMessage::Back => navigate_back(state),
    DetailMessage::Retry => start_detail_load(state),
    DetailMessage::RetryNeighbors => start_detail_followup(state),
    DetailMessage::RetrySeason => start_selected_season_load(state),
    DetailMessage::OverviewToggled => {
      state.detail.overview_expanded = !state.detail.overview_expanded;
      Task::none()
    }
    DetailMessage::SeasonSelected(season_id) => {
      if !select_season(&mut state.detail, &season_id) {
        return Task::none();
      }
      start_selected_season_load(state)
    }
    DetailMessage::FavoriteToggled => start_user_data_update(state, UserDataActionKind::Favorite),
    DetailMessage::PlayedToggled => start_user_data_update(state, UserDataActionKind::Played),
    DetailMessage::Loaded { token, result } => {
      if !settle_detail_load(&mut state.detail, &mut state.request_gate, token, *result) {
        return Task::none();
      }
      let followup = start_detail_followup(state);
      Task::batch([followup, prepare_detail_artwork(state)])
    }
    DetailMessage::SeasonLoaded { token, result } => {
      if !settle_season_load(&mut state.detail, &mut state.request_gate, token, result) {
        return Task::none();
      }
      prepare_detail_artwork(state)
    }
    DetailMessage::NeighborsLoaded { token, result } => {
      if !state.request_gate.finish_detail_aux(token) {
        return Task::none();
      }
      state.detail.season_neighbors = match result {
        Ok(items) => jellypilot_core::LoadState::Ready(items),
        Err(_) => jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned()),
      };
      prepare_detail_artwork(state)
    }
    DetailMessage::UserDataUpdated { token, result } => {
      let Some(update) =
        settle_user_data_update(&mut state.detail, &mut state.request_gate, token, result)
      else {
        return Task::none();
      };
      if let Some(update) = update {
        if let Some(item) = state.detail_items.get_mut(&update.item_id) {
          item.played = update.played;
          item.favorite = update.favorite;
        }
      }
      Task::none()
    }
    DetailMessage::ArtworkLoaded {
      session,
      slot,
      image_id,
      result,
    } => {
      let session_ok = state.request_gate.is_current_session(session);
      if state
        .artwork_binder
        .settle(slot, ArtworkSurface::Detail, session_ok)
        != ArtworkSettlement::Apply
      {
        return Task::none();
      }
      let Some(cell) = state.detail_artwork.cell_mut(slot, &image_id) else {
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

fn start_detail_load(state: &mut State) -> Task<Message> {
  let Destination::Detail(item_id) = &state.destination else {
    return Task::none();
  };
  let item_id = item_id.clone();
  let Some(item) = state.detail_items.get(&item_id).cloned() else {
    state.detail.content = jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned());
    return Task::none();
  };
  state.detail.clear();
  begin_detail_artwork_view(state);
  state.request_gate.set_detail_item(Some(item_id));
  let token = state.request_gate.begin_detail();
  state.detail.content = jellypilot_core::LoadState::Loading;
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    state.detail.content = jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned());
    return Task::none();
  };

  Task::perform(
    async move {
      load_detail_content(client, item)
        .await
        .map_err(|_| DETAIL_FAILURE.to_owned())
    },
    move |result| {
      Message::Detail(DetailMessage::Loaded {
        token,
        result: Box::new(result),
      })
    },
  )
}

fn settle_detail_load(
  detail: &mut DetailState,
  gate: &mut RequestGate,
  token: DetailToken,
  result: Result<DetailContent, String>,
) -> bool {
  if !gate.finish_detail(token) {
    return false;
  }
  detail.content = match result {
    Ok(content) => jellypilot_core::LoadState::Ready(content),
    Err(_) => jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned()),
  };
  true
}

fn start_detail_followup(state: &mut State) -> Task<Message> {
  match &state.detail.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      let request = match (
        item.series_id.as_ref(),
        item.season_number,
        item.item_type.eq_ignore_ascii_case("episode"),
      ) {
        (Some(series_id), Some(season_number), true) => {
          Some((item.id.clone(), series_id.clone(), season_number))
        }
        _ => None,
      };
      let Some((item_id, series_id, season_number)) = request else {
        state.detail.season_neighbors = jellypilot_core::LoadState::Idle;
        return Task::none();
      };
      let Some(token) = state
        .request_gate
        .begin_detail_aux(DetailAuxKind::SeasonNeighbors)
      else {
        return Task::none();
      };
      state.detail.season_neighbors = jellypilot_core::LoadState::Loading;
      let Some(client) = state.client.as_ref().map(Arc::clone) else {
        state.detail.season_neighbors =
          jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned());
        return Task::none();
      };
      Task::perform(
        async move {
          load_season_neighbors(client, item_id, series_id, season_number)
            .await
            .map_err(|_| SEASON_FAILURE.to_owned())
        },
        move |result| Message::Detail(DetailMessage::NeighborsLoaded { token, result }),
      )
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      state.detail.selected_season_id = initial_season(show).map(|season| season.id.clone());
      start_selected_season_load(state)
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => Task::none(),
  }
}

fn initial_season(show: &jellypilot_media_server::VideoShowDetail) -> Option<&VideoSeason> {
  show
    .next_episode
    .as_ref()
    .and_then(|episode| episode.season_number)
    .and_then(|season_number| {
      show
        .seasons
        .iter()
        .find(|season| season.season_number == Some(season_number))
    })
    .or_else(|| show.seasons.first())
}

fn select_season(detail: &mut DetailState, season_id: &str) -> bool {
  let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &detail.content else {
    return false;
  };
  if detail.selected_season_id.as_deref() == Some(season_id)
    || !show.seasons.iter().any(|season| season.id == season_id)
  {
    return false;
  }
  detail.selected_season_id = Some(season_id.to_owned());
  true
}

fn selected_season_request(detail: &DetailState) -> Option<VideoSeasonEpisodesPageRequest> {
  let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &detail.content else {
    return None;
  };
  let selected_id = detail.selected_season_id.as_deref()?;
  let season = show
    .seasons
    .iter()
    .find(|season| season.id == selected_id)?;
  Some(season_page_request(&show.id, season, 0))
}

fn start_selected_season_load(state: &mut State) -> Task<Message> {
  let Some(request) = selected_season_request(&state.detail) else {
    state.detail.season_episodes = jellypilot_core::LoadState::Idle;
    return Task::none();
  };
  let token = state.request_gate.begin_detail();
  state.detail.season_episodes = jellypilot_core::LoadState::Loading;
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    state.detail.season_episodes = jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned());
    return Task::none();
  };
  Task::perform(
    async move {
      client
        .library()
        .season_episodes_page(request)
        .await
        .map_err(|_| SEASON_FAILURE.to_owned())
    },
    move |result| Message::Detail(DetailMessage::SeasonLoaded { token, result }),
  )
}

fn settle_season_load(
  detail: &mut DetailState,
  gate: &mut RequestGate,
  token: DetailToken,
  result: Result<VideoSeasonEpisodesPage, String>,
) -> bool {
  if !gate.finish_detail(token) {
    return false;
  }
  detail.season_episodes = match result {
    Ok(page) => jellypilot_core::LoadState::Ready(page),
    Err(_) => jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned()),
  };
  true
}

fn start_user_data_update(state: &mut State, kind: UserDataActionKind) -> Task<Message> {
  if state.detail.user_data_busy.is_some() {
    return Task::none();
  }
  let Some((item_id, played, favorite)) = detail_user_data(&state.detail.content) else {
    return Task::none();
  };
  let action = match kind {
    UserDataActionKind::Favorite if favorite => VideoUserDataAction::Unfavorite,
    UserDataActionKind::Favorite => VideoUserDataAction::Favorite,
    UserDataActionKind::Played if played => VideoUserDataAction::MarkUnplayed,
    UserDataActionKind::Played => VideoUserDataAction::MarkPlayed,
  };
  let Some(token) = state.request_gate.begin_detail_aux(DetailAuxKind::UserData) else {
    return Task::none();
  };
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    state.detail.user_data_error = Some(USER_DATA_FAILURE.to_owned());
    return Task::none();
  };
  state.detail.user_data_busy = Some(kind);
  state.detail.user_data_error = None;
  let request = VideoUserDataUpdateRequest { item_id, action };
  Task::perform(
    async move {
      client
        .library()
        .update_user_data(request)
        .await
        .map_err(|_| USER_DATA_FAILURE.to_owned())
    },
    move |result| Message::Detail(DetailMessage::UserDataUpdated { token, result }),
  )
}

fn detail_user_data(
  detail: &jellypilot_core::LoadState<DetailContent>,
) -> Option<(String, bool, bool)> {
  match detail {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      Some((item.id.clone(), item.played, item.favorite))
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      Some((show.id.clone(), show.played, show.favorite))
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => None,
  }
}

fn settle_user_data_update(
  detail: &mut DetailState,
  gate: &mut RequestGate,
  token: jellypilot_core::request_gate::DetailAuxToken,
  result: Result<VideoUserDataUpdate, String>,
) -> Option<Option<VideoUserDataUpdate>> {
  if !gate.finish_detail_aux(token) {
    return None;
  }
  detail.user_data_busy = None;
  match result {
    Ok(update) if apply_user_data_update(&mut detail.content, &update) => {
      detail.user_data_error = None;
      Some(Some(update))
    }
    Ok(_) | Err(_) => {
      detail.user_data_error = Some(USER_DATA_FAILURE.to_owned());
      Some(None)
    }
  }
}

const DETAIL_POSTER_KEY: &str = "detail-poster";
const DETAIL_BACKDROP_KEY: &str = "detail-backdrop";

struct DetailArtworkSpec {
  key: String,
  image_id: String,
}

fn prepare_detail_artwork(state: &mut State) -> Task<Message> {
  let mut specs = Vec::new();
  match &state.detail.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      push_detail_artwork(
        &mut specs,
        DETAIL_POSTER_KEY.to_owned(),
        &item.artwork_image_id,
      );
      push_detail_artwork(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &item.backdrop_image_id,
      );
      if let jellypilot_core::LoadState::Ready(neighbors) = &state.detail.season_neighbors {
        for episode in neighbors {
          push_detail_artwork(
            &mut specs,
            detail_episode_key(&episode.id),
            &episode.artwork_image_id,
          );
        }
      }
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      push_detail_artwork(
        &mut specs,
        DETAIL_POSTER_KEY.to_owned(),
        &show.artwork_image_id,
      );
      push_detail_artwork(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &show.backdrop_image_id,
      );
      if let Some(next) = &show.next_episode {
        push_detail_artwork(
          &mut specs,
          detail_episode_key(&next.id),
          &next.artwork_image_id,
        );
      }
      if let jellypilot_core::LoadState::Ready(page) = &state.detail.season_episodes {
        for episode in &page.episodes {
          push_detail_artwork(
            &mut specs,
            detail_episode_key(&episode.id),
            &episode.artwork_image_id,
          );
        }
      }
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => {}
  }

  let retained_keys = specs
    .iter()
    .map(|spec| spec.key.as_str())
    .collect::<HashSet<_>>();
  state.detail_artwork.retain_keys(&retained_keys);
  drop(retained_keys);
  let session = state.request_gate.current_session();
  let Some(client) = state.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let adapter = Arc::clone(&state.artwork_adapter);
  let mut tasks = Vec::new();
  for spec in specs {
    if state
      .detail_artwork
      .get(&spec.key)
      .is_some_and(|cell| cell.image_id == spec.image_id)
    {
      continue;
    }
    let slot = state.artwork_binder.bind(ArtworkSurface::Detail);
    state.detail_artwork.insert(
      spec.key,
      ArtworkCell {
        slot,
        image_id: spec.image_id.clone(),
        state: ArtworkCellState::Loading,
      },
    );
    let client = Arc::clone(&client);
    let adapter = Arc::clone(&adapter);
    let completion_image_id = spec.image_id.clone();
    tasks.push(Task::perform(
      async move { adapter.load(&client, &spec.image_id).await },
      move |result| {
        Message::Detail(DetailMessage::ArtworkLoaded {
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
    .retain_slots(state.detail_artwork.slots());
  Task::batch(tasks)
}

fn push_detail_artwork(specs: &mut Vec<DetailArtworkSpec>, key: String, image_id: &Option<String>) {
  if let Some(image_id) = image_id {
    specs.push(DetailArtworkSpec {
      key,
      image_id: image_id.clone(),
    });
  }
}

fn detail_episode_key(item_id: &str) -> String {
  format!("detail-episode:{item_id}")
}

fn begin_detail_artwork_view(state: &mut State) {
  state.artwork_binder.begin_view(ArtworkSurface::Detail);
  state.detail_artwork.clear();
  state.artwork_handles.retain_slots(std::iter::empty());
}

fn leave_detail_view(state: &mut State) {
  state.request_gate.navigate();
  state.artwork_adapter.cancel_pending();
  begin_detail_artwork_view(state);
  state.detail.clear();
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
    Destination::Home | Destination::Detail(_) => None,
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
  state.detail_artwork = DetailArtwork::default();
  state.artwork_handles.clear();
  state.home = HomeState::default();
  state.detail.clear();
  state.detail_items.clear();
  state.navigation_stack.clear();
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
      navigation_stack: Vec::new(),
      detail_items: Default::default(),
      detail: DetailState::default(),
      detail_artwork: DetailArtwork::default(),
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

  fn video_item(id: &str) -> jellypilot_media_server::VideoItemDetail {
    jellypilot_media_server::VideoItemDetail {
      id: id.to_owned(),
      name: "Arrival".to_owned(),
      item_type: "Movie".to_owned(),
      overview: None,
      production_year: Some(2016),
      runtime_seconds: Some(116.0 * 60.0),
      series_id: None,
      series_name: None,
      season_number: None,
      episode_number: None,
      genres: vec!["Science Fiction".to_owned()],
      played: false,
      favorite: false,
      played_percentage: None,
      resume_position_seconds: None,
      can_resume: false,
      can_play: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      metadata: Default::default(),
    }
  }

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      id: id.to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      runtime_seconds: Some(1_800.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      series_poster_image_id: None,
      season_number: Some(season_number),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
    }
  }

  fn season(id: &str, number: i32) -> VideoSeason {
    VideoSeason {
      id: id.to_owned(),
      name: format!("Season {number}"),
      season_number: Some(number),
      played: false,
      favorite: false,
      artwork_image_id: None,
    }
  }

  fn show_detail() -> jellypilot_media_server::VideoShowDetail {
    jellypilot_media_server::VideoShowDetail {
      id: "show-1".to_owned(),
      name: "Show".to_owned(),
      overview: None,
      production_year: None,
      genres: Vec::new(),
      played: false,
      favorite: false,
      can_play: true,
      artwork_image_id: None,
      backdrop_image_id: None,
      next_episode: Some(episode("episode-2", 2)),
      seasons: vec![season("season-1", 1), season("season-2", 2)],
      metadata: Default::default(),
    }
  }

  #[test]
  fn stale_detail_settlement_cannot_replace_the_current_request() {
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Loading,
      ..DetailState::default()
    };
    let mut gate = RequestGate::default();
    let stale = gate.begin_detail();
    let current = gate.begin_detail();

    assert!(!settle_detail_load(
      &mut detail,
      &mut gate,
      stale,
      Ok(DetailContent::Item(video_item("stale"))),
    ));
    assert!(matches!(
      detail.content,
      jellypilot_core::LoadState::Loading
    ));
    assert!(settle_detail_load(
      &mut detail,
      &mut gate,
      current,
      Ok(DetailContent::Item(video_item("current"))),
    ));
    assert!(matches!(
      &detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.id == "current"
    ));
  }

  #[test]
  fn user_data_transition_waits_for_confirmation_and_preserves_data_on_failure() {
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Ready(DetailContent::Item(video_item("item-1"))),
      user_data_busy: Some(UserDataActionKind::Favorite),
      ..DetailState::default()
    };
    let mut gate = RequestGate::default();
    gate.set_detail_item(Some("item-1".to_owned()));
    let stale = gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("detail item should permit user-data update");
    let success = gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("detail item should permit user-data update");

    assert!(settle_user_data_update(
      &mut detail,
      &mut gate,
      stale,
      Ok(VideoUserDataUpdate {
        item_id: "item-1".to_owned(),
        played: true,
        favorite: true,
      }),
    )
    .is_none());
    assert_eq!(detail.user_data_busy, Some(UserDataActionKind::Favorite));

    let applied = settle_user_data_update(
      &mut detail,
      &mut gate,
      success,
      Ok(VideoUserDataUpdate {
        item_id: "item-1".to_owned(),
        played: false,
        favorite: true,
      }),
    );
    assert!(matches!(applied, Some(Some(_))));
    assert!(matches!(
      &detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.favorite && !item.played
    ));
    assert!(detail.user_data_busy.is_none());

    detail.user_data_busy = Some(UserDataActionKind::Played);
    let failure = gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("retry should mint a fresh token");
    assert!(matches!(
      settle_user_data_update(
        &mut detail,
        &mut gate,
        failure,
        Err("raw server response".to_owned()),
      ),
      Some(None)
    ));
    assert!(matches!(
      &detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.favorite && !item.played
    ));
    assert_eq!(detail.user_data_error.as_deref(), Some(USER_DATA_FAILURE));
  }

  #[test]
  fn leaving_detail_rejects_pending_user_data_after_reopening_same_item() {
    let mut state = test_state();
    state.destination = Destination::Detail("item-1".to_owned());
    state
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    let stale = state
      .request_gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("detail item should permit user-data update");

    leave_detail_view(&mut state);
    state.destination = Destination::Detail("item-1".to_owned());
    state
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    state.detail.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(video_item("item-1")));
    state.detail.user_data_busy = Some(UserDataActionKind::Favorite);

    let settlement = settle_user_data_update(
      &mut state.detail,
      &mut state.request_gate,
      stale,
      Ok(VideoUserDataUpdate {
        item_id: "item-1".to_owned(),
        played: true,
        favorite: true,
      }),
    );

    assert!(settlement.is_none());
    assert!(matches!(
      &state.detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if !item.played && !item.favorite
    ));
    assert_eq!(
      state.detail.user_data_busy,
      Some(UserDataActionKind::Favorite)
    );
  }

  #[test]
  fn season_switching_uses_the_selected_seasons_exact_identity() {
    let show = show_detail();
    assert_eq!(
      initial_season(&show).map(|season| season.id.as_str()),
      Some("season-2")
    );
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Ready(DetailContent::Show(show)),
      selected_season_id: Some("season-2".to_owned()),
      ..DetailState::default()
    };

    assert!(select_season(&mut detail, "season-1"));
    let request = selected_season_request(&detail).expect("selected season should produce a page");
    assert_eq!(request.series_id, "show-1");
    assert_eq!(request.season_id.as_deref(), Some("season-1"));
    assert_eq!(request.season_number, Some(1));
    assert_eq!(request.start_index, 0);
    assert_eq!(
      request.limit,
      jellypilot_core::detail::SEASON_EPISODE_PAGE_SIZE
    );
    assert!(!select_season(&mut detail, "season-1"));
    assert!(!select_season(&mut detail, "missing-season"));
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
