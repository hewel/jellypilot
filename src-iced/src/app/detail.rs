//! Detail surface (ADR 0029): item/show detail loading, auxiliary shelves,
//! season episode paging, user-data actions, and the detail artwork pipeline.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::i18n::UiText;
use iced::Task;
use jellypilot_core::detail::{
  apply_user_data_update, detail_episode_key, detail_similar_key, detail_user_data, initial_season,
  load_detail_content, load_season_neighbors, load_similar_items, season_for_number,
  selected_season_request, DetailContent,
};
use jellypilot_core::request_gate::{DetailAuxKind, DetailToken, RequestGate};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::{
  VideoLibraryItem, VideoSeasonEpisodesPage, VideoUserDataAction, VideoUserDataUpdate,
};

use super::artwork::{ImageCollection, ImageSpec};
use super::kernel::Kernel;
use super::message::{DetailMessage, Message};
use super::state::{DetailState, TrackMenu, UserDataActionKind};

const DETAIL_FAILURE: &str = "detail-load-error";
const SEASON_FAILURE: &str = "detail-season-error";
const SIMILAR_FAILURE: &str = "detail-similar-error";
pub(crate) const USER_DATA_FAILURE: &str = "detail-user-data-error";

pub(crate) const DETAIL_LOGO_KEY: &str = "detail-logo";
pub(crate) const DETAIL_BACKDROP_KEY: &str = "detail-backdrop";

/// Detail surface slice: the library items opened into Detail, the loaded
/// detail view state, and the artwork cells bound for hero and shelf artwork.
#[derive(Default)]
pub struct Surface {
  pub items: HashMap<String, VideoLibraryItem>,
  pub data: DetailState,
  pub artwork: ImageCollection,
  pub(crate) season_menu_open: bool,
  /// The Media Specifications track list currently open; at most one at a time.
  pub(crate) track_menu_open: Option<TrackMenu>,
  /// Season preselection requested by the navigation that opened the current
  /// detail item: (detail item id, originating season number). Consumed by the
  /// next load; a mismatched or stale request is dropped, never applied.
  pub(crate) pending_season: Option<(String, i32)>,
  /// Item the current detail view presents; set by `start_load`/`restore`.
  view_item_id: Option<String>,
  /// Identity of the current detail view instance, minted from a process-wide
  /// counter so a recreated surface can never collide with a pending write's
  /// recorded origin.
  view_generation: u64,
  refresh_token: Option<DetailToken>,
}

/// Process-wide detail view identity source: pending writes outlive a Detail
/// surface (app-mode switches drop and recreate `FullUi`), so generations must
/// never restart at zero.
static NEXT_VIEW_GENERATION: AtomicU64 = AtomicU64::new(0);

fn next_view_generation() -> u64 {
  NEXT_VIEW_GENERATION
    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
      value.checked_add(1)
    })
    .expect("detail view identity exhausted")
    + 1
}
/// Updates Detail content and its locally owned images.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  detail_item_id: Option<&str>,
  message: DetailMessage,
) -> Task<Message> {
  match message {
    // Handled entirely by the top-level router: navigation writes the shared
    // destination stack and drives the other surfaces' leave/enter hooks.
    DetailMessage::Back | DetailMessage::OpenSeries => Task::none(),
    DetailMessage::Retry => start_load(surface, kernel, detail_item_id),
    DetailMessage::RetryNeighbors => start_followup(surface, kernel, false),
    DetailMessage::RetrySeason => start_selected_season_load(surface, kernel),
    DetailMessage::OverviewToggled => {
      surface.data.overview_expanded = !surface.data.overview_expanded;
      Task::none()
    }
    DetailMessage::EpisodeOverviewToggled(item_id) => {
      if !surface.data.expanded_episode_ids.remove(&item_id) {
        surface.data.expanded_episode_ids.insert(item_id);
      }
      Task::none()
    }
    DetailMessage::SeasonMenuToggled => {
      surface.season_menu_open = !surface.season_menu_open
        && matches!(&surface.data.content, jellypilot_core::LoadState::Ready(DetailContent::Show(show)) if !show.seasons.is_empty())
        && !matches!(
          surface.data.season_episodes,
          jellypilot_core::LoadState::Loading
        );
      Task::none()
    }
    DetailMessage::SeasonMenuDismissed => {
      surface.season_menu_open = false;
      Task::none()
    }
    DetailMessage::SeasonSelected(season_id) => {
      surface.season_menu_open = false;
      if !select_season(&mut surface.data, &season_id) {
        return Task::none();
      }
      start_selected_season_load(surface, kernel)
    }
    DetailMessage::TrackMenuToggled(menu) => {
      surface.track_menu_open = (surface.track_menu_open != Some(menu)).then_some(menu);
      Task::none()
    }
    DetailMessage::TrackMenuDismissed => {
      surface.track_menu_open = None;
      Task::none()
    }
    DetailMessage::Loaded { token, result } => {
      if surface.refresh_token == Some(token) {
        surface.refresh_token = None;
      }
      let failed_refresh =
        result.is_err() && matches!(surface.data.content, jellypilot_core::LoadState::Ready(_));
      if !settle_load(&mut surface.data, &mut kernel.request_gate, token, *result) {
        return Task::none();
      }
      if failed_refresh {
        return kernel.show_toast(
          super::state::NoticeLevel::Error,
          UiText::new("detail-refresh-error"),
        );
      }
      let followup = start_followup(surface, kernel, false);
      Task::batch([followup, prepare_artwork(surface)])
    }
    DetailMessage::SeasonLoaded { token, result } => {
      if !settle_season_load(&mut surface.data, &mut kernel.request_gate, token, result) {
        return Task::none();
      }
      prepare_artwork(surface)
    }
    DetailMessage::NeighborsLoaded { token, result } => {
      if !kernel.request_gate.finish_detail_aux(token) {
        return Task::none();
      }
      surface.data.season_neighbors = match result {
        Ok(items) => jellypilot_core::LoadState::Ready(items),
        Err(error) => {
          tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error.as_str()), "Detail request failed");
          jellypilot_core::LoadState::Failed(UiText::new(SEASON_FAILURE))
        }
      };
      prepare_artwork(surface)
    }
    DetailMessage::SimilarLoaded { token, result } => {
      if !kernel.request_gate.finish_detail_aux(token) {
        return Task::none();
      }
      surface.data.similar_items = match result {
        Ok(items) => jellypilot_core::LoadState::Ready(items),
        Err(error) => {
          tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error.as_str()), "Detail request failed");
          jellypilot_core::LoadState::Failed(UiText::new(SIMILAR_FAILURE))
        }
      };
      prepare_artwork(surface)
    }
    DetailMessage::ArtworkLoaded(completion) => {
      surface
        .artwork
        .settle(kernel.request_gate.current_session(), completion);
      Task::none()
    }
  }
}

/// Starts (or reloads) the detail load for `item_id`. The top-level router
/// also calls this when navigating to a Detail destination.
pub fn start_load(
  surface: &mut Surface,
  kernel: &mut Kernel,
  item_id: Option<&str>,
) -> Task<Message> {
  surface.season_menu_open = false;
  surface.track_menu_open = None;
  surface.view_generation = next_view_generation();
  surface.view_item_id = item_id.map(str::to_owned);
  let Some(item_id) = item_id else {
    return Task::none();
  };
  let Some(item) = surface.items.get(item_id).cloned() else {
    surface.data.content = jellypilot_core::LoadState::Failed(UiText::new(DETAIL_FAILURE));
    return Task::none();
  };
  // DetailState belongs to the current history entry. Preserve unresolved
  // preselection through retries/restoration; leaving the entry clears it.
  let requested_season_number = match surface.pending_season.take() {
    Some((id, season_number)) => (id == item_id).then_some(season_number),
    None => surface.data.requested_season_number,
  };
  surface.data.clear();
  surface.data.requested_season_number = requested_season_number;
  surface.artwork.clear();
  kernel
    .request_gate
    .set_detail_item(Some(item_id.to_owned()));
  let token = kernel.request_gate.begin_detail();
  surface.data.content = jellypilot_core::LoadState::Loading;
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.data.content = jellypilot_core::LoadState::Failed(UiText::new(DETAIL_FAILURE));
    return Task::none();
  };

  Task::perform(
    async move {
      load_detail_content(client, item)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| {
      Message::Detail(DetailMessage::Loaded {
        token,
        result: Box::new(result),
      })
    },
  )
}

pub(crate) fn restore(
  surface: &mut Surface,
  kernel: &mut Kernel,
  item_id: &str,
  data: DetailState,
) -> Task<Message> {
  surface.data = data;
  surface.view_generation = next_view_generation();
  surface.view_item_id = Some(item_id.to_owned());
  surface.refresh_token = None;
  kernel
    .request_gate
    .set_detail_item(Some(item_id.to_owned()));
  if !matches!(surface.data.content, jellypilot_core::LoadState::Ready(_)) {
    return start_load(surface, kernel, Some(item_id));
  }
  Task::batch([
    start_followup(surface, kernel, true),
    prepare_artwork(surface),
  ])
}

fn settle_load(
  detail: &mut DetailState,
  gate: &mut RequestGate,
  token: DetailToken,
  result: Result<DetailContent, String>,
) -> bool {
  if !gate.finish_detail(token) {
    return false;
  }
  match result {
    Ok(mut content) => {
      if let (
        DetailContent::Show(show),
        jellypilot_core::LoadState::Ready(DetailContent::Show(previous)),
      ) = (&mut content, &detail.content)
      {
        if let Some(next) = &mut show.next_episode {
          if next.artwork_image_id.is_none() {
            next.artwork_image_id = previous
              .next_episode
              .as_ref()
              .filter(|previous| previous.id == next.id)
              .and_then(|previous| previous.artwork_image_id.clone());
          }
        }
      }
      detail.content = jellypilot_core::LoadState::Ready(content);
    }
    Err(error) if matches!(detail.content, jellypilot_core::LoadState::Ready(_)) => {
      tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error.as_str()), "Detail refresh failed");
    }
    Err(error) => {
      tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error.as_str()), "Detail request failed");
      detail.content = jellypilot_core::LoadState::Failed(UiText::new(DETAIL_FAILURE));
    }
  }
  true
}

pub(crate) fn refresh(surface: &mut Surface, kernel: &mut Kernel, item_id: &str) -> Task<Message> {
  if kernel.item_actions.pending(item_id).is_some()
    || matches!(
      surface.data.season_episodes,
      jellypilot_core::LoadState::Loading
    )
  {
    return Task::none();
  }
  if !matches!(surface.data.content, jellypilot_core::LoadState::Ready(_)) {
    return start_load(surface, kernel, Some(item_id));
  }
  let Some(item) = surface.items.get(item_id).cloned() else {
    return Task::none();
  };
  let Some(client) = kernel.client.clone() else {
    return Task::none();
  };
  surface.track_menu_open = None;
  let token = kernel.request_gate.begin_detail();
  surface.refresh_token = Some(token);
  Task::perform(
    async move {
      load_detail_content(client, item)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| {
      Message::Detail(DetailMessage::Loaded {
        token,
        result: Box::new(result),
      })
    },
  )
}

fn start_followup(surface: &mut Surface, kernel: &mut Kernel, only_missing: bool) -> Task<Message> {
  enum Followup {
    Episode {
      item_id: String,
      series_id: Option<String>,
      season_number: Option<i32>,
    },
    Movie(String),
    Show {
      item_id: String,
      selected_season_id: Option<String>,
    },
    None,
  }

  // A season requested by the navigation that opened this detail (e.g. the
  // originating season of an episode's parent series) is consumed once: it
  // resolves only against a season the loaded show actually lists.
  let requested_season = if matches!(surface.data.content, jellypilot_core::LoadState::Ready(_)) {
    surface.data.requested_season_number.take()
  } else {
    None
  };
  let followup = match &surface.data.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item))
      if item.item_type.eq_ignore_ascii_case("episode") =>
    {
      Followup::Episode {
        item_id: item.id.clone(),
        series_id: item.series_id.clone(),
        season_number: item.season_number,
      }
    }
    jellypilot_core::LoadState::Ready(DetailContent::Item(item))
      if item.item_type.eq_ignore_ascii_case("movie") =>
    {
      Followup::Movie(item.id.clone())
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => Followup::Show {
      item_id: show.id.clone(),
      selected_season_id: requested_season
        .and_then(|season_number| {
          season_for_number(show, season_number).map(|season| season.id.clone())
        })
        .or_else(|| {
          surface
            .data
            .selected_season_id
            .as_ref()
            .filter(|id| show.seasons.iter().any(|season| &season.id == *id))
            .cloned()
        })
        .or_else(|| initial_season(show).map(|season| season.id.clone())),
    },
    jellypilot_core::LoadState::Ready(DetailContent::Item(_))
    | jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => Followup::None,
  };

  match followup {
    Followup::Episode {
      item_id,
      series_id,
      season_number,
    } => {
      let similar = if !only_missing
        || matches!(
          surface.data.similar_items,
          jellypilot_core::LoadState::Idle | jellypilot_core::LoadState::Loading
        ) {
        start_similar_load(
          surface,
          kernel,
          series_id.as_ref().unwrap_or(&item_id).clone(),
        )
      } else {
        Task::none()
      };
      let neighbors = if !only_missing
        || matches!(
          surface.data.season_neighbors,
          jellypilot_core::LoadState::Idle | jellypilot_core::LoadState::Loading
        ) {
        if let (Some(series_id), Some(season_number)) = (series_id, season_number) {
          start_neighbors_load(surface, kernel, item_id, series_id, season_number)
        } else {
          surface.data.season_neighbors = jellypilot_core::LoadState::Idle;
          Task::none()
        }
      } else {
        Task::none()
      };
      Task::batch([neighbors, similar])
    }
    Followup::Movie(item_id) => {
      if only_missing
        && !matches!(
          surface.data.similar_items,
          jellypilot_core::LoadState::Idle | jellypilot_core::LoadState::Loading
        )
      {
        return Task::none();
      }
      surface.data.season_neighbors = jellypilot_core::LoadState::Idle;
      start_similar_load(surface, kernel, item_id)
    }
    Followup::Show {
      item_id,
      selected_season_id,
    } => {
      surface.data.selected_season_id = selected_season_id;
      let episodes = if !only_missing
        || matches!(
          surface.data.season_episodes,
          jellypilot_core::LoadState::Idle | jellypilot_core::LoadState::Loading
        ) {
        start_selected_season_load(surface, kernel)
      } else {
        Task::none()
      };
      let similar = if !only_missing
        || matches!(
          surface.data.similar_items,
          jellypilot_core::LoadState::Idle | jellypilot_core::LoadState::Loading
        ) {
        start_similar_load(surface, kernel, item_id)
      } else {
        Task::none()
      };
      Task::batch([episodes, similar])
    }
    Followup::None => {
      surface.data.season_neighbors = jellypilot_core::LoadState::Idle;
      surface.data.similar_items = jellypilot_core::LoadState::Idle;
      Task::none()
    }
  }
}

fn start_neighbors_load(
  surface: &mut Surface,
  kernel: &mut Kernel,
  item_id: String,
  series_id: String,
  season_number: i32,
) -> Task<Message> {
  let Some(token) = kernel
    .request_gate
    .begin_detail_aux(DetailAuxKind::SeasonNeighbors)
  else {
    return Task::none();
  };
  surface.data.season_neighbors = jellypilot_core::LoadState::Loading;
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.data.season_neighbors = jellypilot_core::LoadState::Failed(UiText::new(SEASON_FAILURE));
    return Task::none();
  };
  Task::perform(
    async move {
      load_season_neighbors(client, item_id, series_id, season_number)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| Message::Detail(DetailMessage::NeighborsLoaded { token, result }),
  )
}

fn start_similar_load(
  surface: &mut Surface,
  kernel: &mut Kernel,
  item_id: String,
) -> Task<Message> {
  let Some(token) = kernel
    .request_gate
    .begin_detail_aux(DetailAuxKind::SimilarItems)
  else {
    return Task::none();
  };
  surface.data.similar_items = jellypilot_core::LoadState::Loading;
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.data.similar_items = jellypilot_core::LoadState::Failed(UiText::new(SIMILAR_FAILURE));
    return Task::none();
  };
  Task::perform(
    async move {
      load_similar_items(client.as_ref(), item_id)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| Message::Detail(DetailMessage::SimilarLoaded { token, result }),
  )
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

fn start_selected_season_load(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let Some(request) = selected_season_request(
    &surface.data.content,
    surface.data.selected_season_id.as_deref(),
  ) else {
    surface.data.season_episodes = jellypilot_core::LoadState::Idle;
    return Task::none();
  };
  let token = kernel.request_gate.begin_detail();
  surface.data.season_episodes = jellypilot_core::LoadState::Loading;
  drop(prepare_artwork(surface));
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.data.season_episodes = jellypilot_core::LoadState::Failed(UiText::new(SEASON_FAILURE));
    return Task::none();
  };
  Task::perform(
    async move {
      client
        .library()
        .season_episodes_page(request)
        .await
        .map_err(|error| error.to_string())
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
    Ok(page) => {
      // Preserve richer season metadata for Next Up across season switches.
      if let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &mut detail.content {
        if let Some(next) = &mut show.next_episode {
          if next.artwork_image_id.is_none() {
            next.artwork_image_id = page
              .episodes
              .iter()
              .find(|episode| episode.id == next.id)
              .and_then(|episode| episode.artwork_image_id.clone());
          }
        }
      }
      jellypilot_core::LoadState::Ready(page)
    }
    Err(error) => {
      tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error.as_str()), "Detail request failed");
      jellypilot_core::LoadState::Failed(UiText::new(SEASON_FAILURE))
    }
  };
  true
}

/// Resolves the current detail content's user-data flags into the server write
/// a `kind` intent requests; `None` while no content is ready.
pub(crate) fn action_target(
  surface: &Surface,
  kind: UserDataActionKind,
) -> Option<(String, VideoUserDataAction)> {
  let (item_id, played, favorite) = detail_user_data(&surface.data.content)?;
  let action = match kind {
    UserDataActionKind::Favorite if favorite => VideoUserDataAction::Unfavorite,
    UserDataActionKind::Favorite => VideoUserDataAction::Favorite,
    UserDataActionKind::Played if played => VideoUserDataAction::MarkUnplayed,
    UserDataActionKind::Played => VideoUserDataAction::MarkPlayed,
  };
  Some((item_id, action))
}

/// Identity of the current detail view instance; bumped on every load start,
/// restore, and leave so a write's origin can never alias a later view of the
/// same item.
pub(crate) fn view_generation(surface: &Surface) -> u64 {
  surface.view_generation
}

/// A confirmed write must not be overwritten by an older metadata refresh.
pub(crate) fn cancel_refresh(surface: &mut Surface, kernel: &mut Kernel) {
  if let Some(token) = surface.refresh_token.take() {
    let _ = kernel.request_gate.finish_detail(token);
  }
}

/// Prepares the surface for an admitted write on `item_id`: an in-flight
/// refresh of the presented item is cancelled (its response predates the
/// write) and the current inline error clears.
pub(crate) fn prepare_mutation(surface: &mut Surface, kernel: &mut Kernel, item_id: &str) {
  if surface.view_item_id.as_deref() == Some(item_id) {
    cancel_refresh(surface, kernel);
  }
  surface.data.user_data_error = None;
}

/// Updates live projections and supersedes pending reads that could contain
/// pre-write flags. Navigation snapshots are updated by their shell owner.
pub(crate) fn apply_confirmed(
  surface: &mut Surface,
  kernel: &mut Kernel,
  update: &VideoUserDataUpdate,
  refresh: bool,
) -> Task<Message> {
  let was_refreshing = surface.refresh_token.is_some();
  cancel_refresh(surface, kernel);
  apply_snapshot_update(&mut surface.data, update);
  if let Some(item) = surface.items.get_mut(&update.item_id) {
    overlay_item(item, update);
  }
  let Some(current_id) = surface.view_item_id.clone() else {
    return Task::none();
  };
  if matches!(surface.data.content, jellypilot_core::LoadState::Loading) {
    return start_load(surface, kernel, Some(&current_id));
  }
  let followup = start_followup(surface, kernel, true);
  let reload = if was_refreshing || (refresh && current_id == update.item_id) {
    self::refresh(surface, kernel, &current_id)
  } else {
    Task::none()
  };
  Task::batch([followup, reload])
}

/// Marks the inline error of a failed write that originated from this view.
/// Returns `false` when the origin view is gone or superseded so the caller
/// can fall back to a toast.
pub(crate) fn mutation_failed(surface: &mut Surface, item_id: &str, generation: u64) -> bool {
  let current = surface.view_generation == generation
    && detail_user_data(&surface.data.content).is_some_and(|(id, _, _)| id == item_id);
  if !current {
    return false;
  }
  surface.data.user_data_error = Some(UiText::new(USER_DATA_FAILURE));
  true
}

fn overlay_item(item: &mut VideoLibraryItem, update: &VideoUserDataUpdate) {
  if item.id == update.item_id {
    item.played = update.played;
    item.favorite = update.favorite;
  }
}

/// Patches a live or saved Detail projection without retaining mutation state.
pub(crate) fn apply_snapshot_update(data: &mut DetailState, update: &VideoUserDataUpdate) {
  apply_user_data_update(&mut data.content, update);
  if let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &mut data.content {
    if let Some(next) = &mut show.next_episode {
      overlay_item(next, update);
    }
  }
  if let jellypilot_core::LoadState::Ready(page) = &mut data.season_episodes {
    for item in &mut page.episodes {
      overlay_item(item, update);
    }
  }
  for state in [&mut data.season_neighbors, &mut data.similar_items] {
    if let jellypilot_core::LoadState::Ready(items) = state {
      for item in items {
        overlay_item(item, update);
      }
    }
  }
}

fn prepare_artwork(surface: &mut Surface) -> Task<Message> {
  let mut specs = Vec::new();
  if let jellypilot_core::LoadState::Ready(content) = &surface.data.content {
    specs.extend(hero_image_spec(content, DETAIL_LOGO_KEY));
    specs.extend(hero_image_spec(content, DETAIL_BACKDROP_KEY));
    let cast = match content {
      DetailContent::Item(item) => &item.metadata.cast,
      DetailContent::Show(show) => &show.metadata.cast,
    };
    specs.extend(
      cast
        .iter()
        .enumerate()
        .filter_map(|(index, member)| cast_image_spec(index, member)),
    );
    match content {
      DetailContent::Item(_) => {
        if let jellypilot_core::LoadState::Ready(items) = &surface.data.season_neighbors {
          specs.extend(
            items
              .iter()
              .filter_map(|item| card_image_spec(detail_episode_key(&item.id), item)),
          );
        }
      }
      DetailContent::Show(show) => {
        if let Some(next) = &show.next_episode {
          specs.extend(card_image_spec(detail_next_up_key(&next.id), next));
        }
        if let jellypilot_core::LoadState::Ready(page) = &surface.data.season_episodes {
          specs.extend(page.episodes.iter().filter_map(|item| {
            episode_image_spec(&surface.data, detail_episode_key(&item.id), item)
          }));
        }
      }
    }
    if let jellypilot_core::LoadState::Ready(items) = &surface.data.similar_items {
      specs.extend(
        items
          .iter()
          .filter_map(|item| card_image_spec(detail_similar_key(&item.id), item)),
      );
    }
  }
  surface.artwork.retain(&specs);
  Task::none()
}

pub(crate) fn detail_next_up_key(item_id: &str) -> String {
  format!("detail-next-up:{item_id}")
}

pub(crate) fn card_image_spec(key: String, item: &VideoLibraryItem) -> Option<ImageSpec> {
  Some(ImageSpec {
    key,
    image_id: item.artwork_image_id.clone()?,
    size_class: ArtworkSizeClass::Card,
    derived: DerivedArtwork::default(),
  })
}

pub(crate) fn cast_image_spec(
  index: usize,
  member: &jellypilot_media_server::VideoCastMember,
) -> Option<ImageSpec> {
  let image_id = member.image_id.as_ref()?;
  Some(ImageSpec {
    key: format!("detail-cast:{index}"),
    image_id: image_id.clone(),
    size_class: ArtworkSizeClass::Card,
    derived: DerivedArtwork::default(),
  })
}

pub(crate) fn hero_image_spec(content: &DetailContent, key: &str) -> Option<ImageSpec> {
  let (logo, backdrop, primary) = match content {
    DetailContent::Item(item) => (
      &item.logo_image_id,
      &item.backdrop_image_id,
      &item.artwork_image_id,
    ),
    DetailContent::Show(show) => (
      &show.logo_image_id,
      &show.backdrop_image_id,
      &show.artwork_image_id,
    ),
  };
  let is_logo = key == DETAIL_LOGO_KEY;
  Some(ImageSpec {
    key: key.to_owned(),
    image_id: if is_logo {
      logo.as_ref()
    } else {
      backdrop.as_ref().or(primary.as_ref())
    }?
    .clone(),
    size_class: if is_logo {
      ArtworkSizeClass::Hero
    } else {
      ArtworkSizeClass::Backdrop
    },
    derived: DerivedArtwork {
      logo_shadow: is_logo,
    },
  })
}

pub(crate) fn episode_image_spec(
  data: &DetailState,
  key: String,
  item: &VideoLibraryItem,
) -> Option<ImageSpec> {
  if let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &data.content {
    if let Some(next) = show
      .next_episode
      .as_ref()
      .filter(|next| next.id == item.id && next.artwork_image_id.is_some())
    {
      return card_image_spec(key, next);
    }
  }
  card_image_spec(key, item)
}

/// Leaving Detail revokes only its own images and invalidates metadata work.
/// The view generation advances so a write that outlives this view can never
/// deliver its failure to the next view of the same item.
pub(crate) fn leave_view(surface: &mut Surface, kernel: &mut Kernel) {
  surface.season_menu_open = false;
  surface.track_menu_open = None;
  surface.view_generation = next_view_generation();
  surface.view_item_id = None;
  kernel.request_gate.navigate();
  surface.artwork.clear();
  surface.data.clear();
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::{JellyfinClient, VideoSeason};

  use super::*;

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let auth_store = crate::app::kernel::test_auth_store();
    let (sdk, sdk_handoff) = crate::app::kernel::test_account_runtime(&auth_store);
    let kernel = Kernel {
      item_actions: Default::default(),
      settings,
      locale: crate::i18n::Localizer::default(),
      diagnostics: Diagnostics::default(),
      auth_store,
      sdk,
      sdk_handoff,
      request_gate: RequestGate::default(),
      client: None,
      connection: ConnectionPhase::SignedOut,
      connected_identity: None,
      active_profile: None,
      notice: None,
      active_toast: None,
      next_toast_id: 0,
      tray: None,
      artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      profile_avatars: Default::default(),
    };
    (Surface::default(), kernel)
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
      logo_image_id: None,
      series_poster_image_id: None,
      media_info: None,
      metadata: Default::default(),
      original_language: None,
    }
  }

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      community_rating: None,
      episode_count: None,
      last_played_date: None,
      id: id.to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      premiere_date: None,
      runtime_seconds: Some(1_800.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      logo_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: Some(season_number),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
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
      logo_image_id: None,
      next_episode: Some(episode("episode-2", 2)),
      seasons: vec![season("season-1", 1), season("season-2", 2)],
      metadata: Default::default(),
      original_language: None,
    }
  }

  #[test]
  fn restoring_detail_preserves_expansion_and_ready_related_content() {
    let (mut surface, mut kernel) = test_fixture();
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("original"))));
    surface.data.similar_items = jellypilot_core::LoadState::Ready(vec![episode("related", 1)]);
    surface.data.overview_expanded = true;
    surface
      .data
      .expanded_episode_ids
      .insert("related".to_owned());
    let saved = std::mem::take(&mut surface.data);
    leave_view(&mut surface, &mut kernel);
    drop(start_load(&mut surface, &mut kernel, Some("other")));
    drop(restore(&mut surface, &mut kernel, "original", saved));
    assert!(
      matches!(&surface.data.content, jellypilot_core::LoadState::Ready(DetailContent::Item(item)) if item.id == "original")
    );
    assert!(surface.data.overview_expanded);
    assert!(surface.data.expanded_episode_ids.contains("related"));
    assert!(
      matches!(&surface.data.similar_items, jellypilot_core::LoadState::Ready(items) if items[0].id == "related")
    );
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

    assert!(!settle_load(
      &mut detail,
      &mut gate,
      stale,
      Ok(DetailContent::Item(Box::new(video_item("stale")))),
    ));
    assert!(matches!(
      detail.content,
      jellypilot_core::LoadState::Loading
    ));
    assert!(settle_load(
      &mut detail,
      &mut gate,
      current,
      Ok(DetailContent::Item(Box::new(video_item("current")))),
    ));
    assert!(matches!(
      &detail.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.id == "current"
    ));
  }

  fn test_scope() -> jellypilot_core::watchlist::ProfileScope {
    jellypilot_core::watchlist::ProfileScope::new(
      jellypilot_media_server::MediaServerProvider::Jellyfin,
      "https://media.example.test",
      "user-1",
    )
    .expect("valid test scope")
  }

  fn admit_write(
    kernel: &mut Kernel,
    item_id: &str,
    action: jellypilot_core::item_actions::Action,
  ) -> jellypilot_sdk::item_actions::Admission {
    let session = kernel.request_gate.current_session();
    kernel.item_actions.set_scope(session, Some(test_scope()));
    kernel
      .item_actions
      .begin(item_id, action)
      .expect("write admission")
  }

  fn confirmed_update(item_id: &str, played: bool, favorite: bool) -> VideoUserDataUpdate {
    VideoUserDataUpdate {
      item_id: item_id.to_owned(),
      played,
      favorite,
    }
  }

  #[test]
  fn confirmed_user_data_updates_every_projection_and_failure_marks_the_inline_error() {
    let (mut surface, mut kernel) = test_fixture();
    surface.view_item_id = Some("item-1".to_owned());
    surface.view_generation = next_view_generation();
    let generation = surface.view_generation;
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));
    surface
      .items
      .insert("item-1".to_owned(), episode("item-1", 1));
    surface.data.season_neighbors = jellypilot_core::LoadState::Ready(vec![episode("item-1", 1)]);
    surface.data.similar_items = jellypilot_core::LoadState::Ready(vec![episode("item-1", 1)]);

    prepare_mutation(&mut surface, &mut kernel, "item-1");
    drop(apply_confirmed(
      &mut surface,
      &mut kernel,
      &confirmed_update("item-1", false, true),
      false,
    ));

    assert!(matches!(
      &surface.data.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.favorite && !item.played
    ));
    assert!(surface.items["item-1"].favorite);
    assert!(matches!(
      &surface.data.season_neighbors,
      jellypilot_core::LoadState::Ready(items) if items[0].favorite
    ));
    assert!(matches!(
      &surface.data.similar_items,
      jellypilot_core::LoadState::Ready(items) if items[0].favorite
    ));

    assert!(mutation_failed(&mut surface, "item-1", generation));
    assert!(surface.data.user_data_error.is_some());
    assert!(matches!(
      &surface.data.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if item.favorite && !item.played
    ));
  }

  #[test]
  fn show_refresh_preserves_the_selected_season_and_does_not_supersede_its_load() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    kernel
      .request_gate
      .set_detail_item(Some("show-1".to_owned()));
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Show(Box::new(show_detail())));
    surface.data.selected_season_id = Some("season-2".to_owned());
    surface
      .items
      .insert("show-1".to_owned(), episode("show-1", 1));
    drop(start_followup(&mut surface, &mut kernel, false));
    assert_eq!(surface.data.selected_season_id.as_deref(), Some("season-2"));
    let season_token = kernel.request_gate.begin_detail();
    drop(refresh(&mut surface, &mut kernel, "show-1"));
    assert!(kernel.request_gate.finish_detail(season_token));
    assert!(surface.refresh_token.is_none());
  }

  #[test]
  fn a_pending_write_rejects_an_older_refresh_and_blocks_an_overlapping_refresh() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    surface.view_item_id = Some("item-1".to_owned());
    surface
      .items
      .insert("item-1".to_owned(), episode("item-1", 1));
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));
    let _ = refresh(&mut surface, &mut kernel, "item-1");
    let stale = surface.refresh_token.expect("refresh starts");

    let admission = admit_write(
      &mut kernel,
      "item-1",
      jellypilot_core::item_actions::Action::Favorite(true),
    );
    prepare_mutation(&mut surface, &mut kernel, "item-1");
    assert!(surface.refresh_token.is_none());
    assert!(!settle_load(
      &mut surface.data,
      &mut kernel.request_gate,
      stale,
      Ok(DetailContent::Item(Box::new(video_item("stale"))))
    ));
    let _ = refresh(&mut surface, &mut kernel, "item-1");
    assert!(surface.refresh_token.is_none());
    assert!(
      matches!(&surface.data.content, jellypilot_core::LoadState::Ready(DetailContent::Item(item)) if item.id == "item-1")
    );

    drop(admission);
    drop(apply_confirmed(
      &mut surface,
      &mut kernel,
      &confirmed_update("item-1", false, true),
      false,
    ));
    assert!(matches!(
      &surface.data.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item)) if item.favorite
    ));
  }

  #[test]
  fn a_failed_write_marks_only_the_originating_view() {
    let (mut surface, mut kernel) = test_fixture();
    kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    surface.view_item_id = Some("item-1".to_owned());
    surface.view_generation = next_view_generation();
    let origin_generation = surface.view_generation;
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));

    leave_view(&mut surface, &mut kernel);
    kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    surface.view_item_id = Some("item-1".to_owned());
    surface.view_generation = next_view_generation();
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));

    assert!(!mutation_failed(&mut surface, "item-1", origin_generation));
    assert!(surface.data.user_data_error.is_none());
    assert!(matches!(
      &surface.data.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if !item.played && !item.favorite
    ));

    // A recreated surface (app-mode switch drops FullUi) can never collide
    // with a pending write's recorded origin generation.
    let mut recreated = Surface {
      view_item_id: Some("item-1".to_owned()),
      ..Surface::default()
    };
    recreated.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));
    assert!(!mutation_failed(
      &mut recreated,
      "item-1",
      origin_generation
    ));
    assert!(recreated.data.user_data_error.is_none());
  }

  #[test]
  fn a_late_pre_write_load_cannot_overwrite_a_confirmed_update() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface
      .items
      .insert("item-1".to_owned(), episode("item-1", 1));
    drop(start_load(&mut surface, &mut kernel, Some("item-1")));
    let token = kernel.request_gate.begin_detail();
    assert!(matches!(
      surface.data.content,
      jellypilot_core::LoadState::Loading
    ));

    // A write admitted from another surface confirms while the load is in
    // flight; the stale response must not reintroduce the old flags.
    prepare_mutation(&mut surface, &mut kernel, "item-1");
    drop(apply_confirmed(
      &mut surface,
      &mut kernel,
      &confirmed_update("item-1", false, true),
      false,
    ));

    drop(update(
      &mut surface,
      &mut kernel,
      Some("item-1"),
      DetailMessage::Loaded {
        token,
        result: Box::new(Ok(DetailContent::Item(Box::new(video_item("item-1"))))),
      },
    ));

    assert!(matches!(
      &surface.data.content,
      jellypilot_core::LoadState::Loading
    ));
  }

  #[test]
  fn season_menu_closes_on_selection_and_does_not_restart_the_current_season() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    kernel
      .request_gate
      .set_detail_item(Some("show-1".to_owned()));
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Show(Box::new(show_detail())));
    surface.data.selected_season_id = Some("season-1".to_owned());
    surface.data.season_episodes = jellypilot_core::LoadState::Failed(UiText::new(SEASON_FAILURE));
    drop(update(
      &mut surface,
      &mut kernel,
      Some("show-1"),
      DetailMessage::SeasonMenuToggled,
    ));
    assert!(surface.season_menu_open);

    drop(update(
      &mut surface,
      &mut kernel,
      Some("show-1"),
      DetailMessage::SeasonSelected("season-1".to_owned()),
    ));
    assert!(!surface.season_menu_open);
    assert!(matches!(
      surface.data.season_episodes,
      jellypilot_core::LoadState::Failed(_)
    ));

    drop(update(
      &mut surface,
      &mut kernel,
      Some("show-1"),
      DetailMessage::SeasonMenuToggled,
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      Some("show-1"),
      DetailMessage::SeasonSelected("season-2".to_owned()),
    ));
    assert!(!surface.season_menu_open);
    assert_eq!(surface.data.selected_season_id.as_deref(), Some("season-2"));
    assert!(matches!(
      surface.data.season_episodes,
      jellypilot_core::LoadState::Loading
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      Some("show-1"),
      DetailMessage::SeasonMenuToggled,
    ));
    assert!(
      !surface.season_menu_open,
      "a pending season cannot open another selection"
    );
  }

  #[test]
  fn track_menus_are_exclusive_and_dismiss_together() {
    let (mut surface, mut kernel) = test_fixture();

    drop(update(
      &mut surface,
      &mut kernel,
      None,
      DetailMessage::TrackMenuToggled(TrackMenu::Audio),
    ));
    assert_eq!(surface.track_menu_open, Some(TrackMenu::Audio));

    drop(update(
      &mut surface,
      &mut kernel,
      None,
      DetailMessage::TrackMenuToggled(TrackMenu::Subtitles),
    ));
    assert_eq!(surface.track_menu_open, Some(TrackMenu::Subtitles));

    drop(update(
      &mut surface,
      &mut kernel,
      None,
      DetailMessage::TrackMenuToggled(TrackMenu::Subtitles),
    ));
    assert_eq!(surface.track_menu_open, None);

    drop(update(
      &mut surface,
      &mut kernel,
      None,
      DetailMessage::TrackMenuToggled(TrackMenu::Audio),
    ));
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      DetailMessage::TrackMenuDismissed,
    ));
    assert_eq!(surface.track_menu_open, None);
  }

  #[test]
  fn track_menu_closes_on_reload_and_when_leaving_detail() {
    let (mut surface, mut kernel) = test_fixture();
    surface
      .items
      .insert("item-1".to_owned(), episode("item-1", 1));
    surface.track_menu_open = Some(TrackMenu::Audio);

    drop(start_load(&mut surface, &mut kernel, Some("item-1")));
    assert_eq!(surface.track_menu_open, None);

    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));
    surface.track_menu_open = Some(TrackMenu::Audio);
    drop(refresh(&mut surface, &mut kernel, "item-1"));
    assert_eq!(surface.track_menu_open, None);

    surface.track_menu_open = Some(TrackMenu::Subtitles);
    leave_view(&mut surface, &mut kernel);
    assert_eq!(surface.track_menu_open, None);
  }

  #[test]
  fn a_requested_season_preselects_the_matching_show_season_once() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface
      .items
      .insert("show-1".to_owned(), episode("show-1", 1));
    surface.pending_season = Some(("show-1".to_owned(), 1));

    drop(start_load(&mut surface, &mut kernel, Some("show-1")));
    assert_eq!(surface.data.requested_season_number, Some(1));
    assert_eq!(surface.pending_season, None);

    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Show(Box::new(show_detail())));
    drop(start_followup(&mut surface, &mut kernel, false));

    // The originating season wins over the next-up episode's season.
    assert_eq!(surface.data.selected_season_id.as_deref(), Some("season-1"));
    assert_eq!(surface.data.requested_season_number, None);

    // A later refresh keeps the resolved selection rather than re-requesting.
    drop(start_followup(&mut surface, &mut kernel, false));
    assert_eq!(surface.data.selected_season_id.as_deref(), Some("season-1"));
  }

  #[test]
  fn an_unresolvable_or_stale_season_request_falls_back_to_the_default() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    surface
      .items
      .insert("show-1".to_owned(), episode("show-1", 1));
    kernel
      .request_gate
      .set_detail_item(Some("show-1".to_owned()));

    // A season number the show does not list falls back to the normal default.
    surface.data.requested_season_number = Some(9);
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Show(Box::new(show_detail())));
    drop(start_followup(&mut surface, &mut kernel, false));
    assert_eq!(surface.data.selected_season_id.as_deref(), Some("season-2"));

    // A pending request for another item is dropped, not applied to this show.
    surface.pending_season = Some(("other-show".to_owned(), 1));
    drop(start_load(&mut surface, &mut kernel, Some("show-1")));
    assert_eq!(surface.data.requested_season_number, None);
    assert_eq!(surface.pending_season, None);
  }

  #[test]
  fn originating_season_survives_failed_load_retry_and_loading_history_restore() {
    for restore_history in [false, true] {
      let (mut surface, mut kernel) = test_fixture();
      kernel.client = Some(Arc::new(JellyfinClient::new()));
      surface
        .items
        .insert("show-1".to_owned(), episode("show-1", 1));
      surface.pending_season = Some(("show-1".to_owned(), 1));
      drop(start_load(&mut surface, &mut kernel, Some("show-1")));

      if restore_history {
        let saved = std::mem::take(&mut surface.data);
        leave_view(&mut surface, &mut kernel);
        drop(restore(&mut surface, &mut kernel, "show-1", saved));
      } else {
        let token = kernel.request_gate.begin_detail();
        drop(update(
          &mut surface,
          &mut kernel,
          Some("show-1"),
          DetailMessage::Loaded {
            token,
            result: Box::new(Err("temporary network failure".to_owned())),
          },
        ));
        drop(update(
          &mut surface,
          &mut kernel,
          Some("show-1"),
          DetailMessage::Retry,
        ));
      }

      let token = kernel.request_gate.begin_detail();
      drop(update(
        &mut surface,
        &mut kernel,
        Some("show-1"),
        DetailMessage::Loaded {
          token,
          result: Box::new(Ok(DetailContent::Show(Box::new(show_detail())))),
        },
      ));
      assert_eq!(
        surface.data.selected_season_id.as_deref(),
        Some("season-1"),
        "originating season must survive history restore={restore_history}"
      );
    }
  }

  #[test]
  fn season_switching_uses_the_selected_seasons_exact_identity() {
    let show = show_detail();
    assert_eq!(
      initial_season(&show).map(|season| season.id.as_str()),
      Some("season-2")
    );
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Ready(DetailContent::Show(Box::new(show))),
      selected_season_id: Some("season-2".to_owned()),
      ..DetailState::default()
    };

    assert!(select_season(&mut detail, "season-1"));
    let request = selected_season_request(&detail.content, detail.selected_season_id.as_deref())
      .expect("selected season should produce a page");
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
  fn episode_restore_loads_missing_recommendations_without_reloading_neighbors() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("episode-1");
    item.item_type = "Episode".to_owned();
    item.series_id = Some("show-1".to_owned());
    item.season_number = Some(1);
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(item)));
    surface.data.season_neighbors =
      jellypilot_core::LoadState::Ready(vec![episode("episode-2", 1)]);
    kernel
      .request_gate
      .set_detail_item(Some("episode-1".to_owned()));

    drop(start_followup(&mut surface, &mut kernel, true));

    assert!(matches!(
      surface.data.similar_items,
      jellypilot_core::LoadState::Loading
    ));
    assert!(matches!(
      &surface.data.season_neighbors,
      jellypilot_core::LoadState::Ready(items) if items.iter().any(|item| item.id == "episode-2")
    ));
  }

  #[test]
  fn episode_overview_toggle_is_per_item_and_clear_resets_detail_state() {
    let (mut surface, mut kernel) = test_fixture();

    drop(update(
      &mut surface,
      &mut kernel,
      None,
      DetailMessage::EpisodeOverviewToggled("episode-1".to_owned()),
    ));
    surface.data.similar_items = jellypilot_core::LoadState::Ready(Vec::new());
    assert!(surface.data.expanded_episode_ids.contains("episode-1"));

    surface.data.clear();

    assert!(surface.data.expanded_episode_ids.is_empty());
    assert!(matches!(
      surface.data.similar_items,
      jellypilot_core::LoadState::Idle
    ));
  }

  #[test]
  fn next_up_keeps_season_image_after_switching_seasons_and_refreshing() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let next_up = episode("next-up", 1);
    let mut season_next_up = next_up.clone();
    season_next_up.artwork_image_id = Some("next-up-image".to_owned());
    let mut show = show_detail();
    show.next_episode = Some(next_up);
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Show(Box::new(show)));
    let token = kernel.request_gate.begin_detail();
    assert!(settle_season_load(
      &mut surface.data,
      &mut kernel.request_gate,
      token,
      Ok(VideoSeasonEpisodesPage {
        series_id: "show-1".to_owned(),
        season_id: Some("season-1".to_owned()),
        season_number: Some(1),
        start_index: 0,
        limit: 30,
        total_record_count: 1,
        next_start_index: 1,
        has_more: false,
        episodes: vec![season_next_up],
      })
    ));

    drop(prepare_artwork(&mut surface));
    let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &surface.data.content else {
      panic!("show remains ready")
    };
    assert_eq!(
      show
        .next_episode
        .as_ref()
        .and_then(|next| next.artwork_image_id.as_deref()),
      Some("next-up-image")
    );

    let token = kernel.request_gate.begin_detail();
    assert!(settle_season_load(
      &mut surface.data,
      &mut kernel.request_gate,
      token,
      Ok(VideoSeasonEpisodesPage {
        series_id: "show-1".to_owned(),
        season_id: Some("season-2".to_owned()),
        season_number: Some(2),
        start_index: 0,
        limit: 30,
        total_record_count: 1,
        next_start_index: 1,
        has_more: false,
        episodes: vec![episode("season-2-episode", 2)],
      })
    ));

    drop(prepare_artwork(&mut surface));

    let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &surface.data.content else {
      panic!("show remains ready")
    };
    assert_eq!(
      show
        .next_episode
        .as_ref()
        .and_then(|next| next.artwork_image_id.as_deref()),
      Some("next-up-image")
    );

    let mut refreshed = show_detail();
    refreshed.next_episode = Some(episode("next-up", 1));
    let token = kernel.request_gate.begin_detail();
    assert!(settle_load(
      &mut surface.data,
      &mut kernel.request_gate,
      token,
      Ok(DetailContent::Show(Box::new(refreshed)))
    ));
    let jellypilot_core::LoadState::Ready(DetailContent::Show(show)) = &surface.data.content else {
      panic!("show remains ready")
    };
    assert_eq!(
      show
        .next_episode
        .as_ref()
        .and_then(|next| next.artwork_image_id.as_deref()),
      Some("next-up-image")
    );
  }
}
