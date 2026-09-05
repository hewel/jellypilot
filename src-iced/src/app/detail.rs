//! Detail surface (ADR 0029): item/show detail loading, auxiliary shelves,
//! season episode paging, user-data actions, and the detail artwork pipeline.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::Task;
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::artwork_loader::PlannedArtworkLoad;
use jellypilot_core::detail::{
  apply_user_data_update, detail_episode_key, detail_similar_key, detail_user_data, initial_season,
  load_detail_content, load_season_neighbors, load_similar_items, selected_season_request,
  DetailContent,
};
use jellypilot_core::request_gate::{DetailAuxKind, DetailToken, RequestGate};
use jellypilot_media_server::artwork::{
  ArtworkLoadObservation, ArtworkLoadSummary, ArtworkSizeClass, DerivedArtwork,
};
use jellypilot_media_server::{
  VideoLibraryItem, VideoSeasonEpisodesPage, VideoUserDataAction, VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
};

use super::artwork::stream_artwork_loads;
use super::kernel::Kernel;
use super::message::{ArtworkLoadCompletion, DetailMessage, Message};
use super::state::{ArtworkCell, ArtworkCellState, DetailArtwork, DetailState, UserDataActionKind};

const DETAIL_FAILURE: &str = "Could not load this item. Try again.";
const SEASON_FAILURE: &str = "Could not load this season. Try again.";
const SIMILAR_FAILURE: &str = "Could not load similar items.";
const USER_DATA_FAILURE: &str = "Could not update user data. Try again.";

const DETAIL_LOGO_KEY: &str = "detail-logo";
const DETAIL_BACKDROP_KEY: &str = "detail-backdrop";

/// Detail surface slice: the library items opened into Detail, the loaded
/// detail view state, and the artwork cells bound for hero and shelf artwork.
#[derive(Default)]
pub struct Surface {
  pub items: HashMap<String, VideoLibraryItem>,
  pub data: DetailState,
  pub artwork: DetailArtwork,
}
/// `detail_item_id` is the shell's current `Destination::Detail` item id,
/// computed by the top-level router so this module never reads navigation
/// state (ADR 0029). The router also retains artwork handles across all
/// surfaces after content or auxiliary settlements re-prepare the pipeline,
/// because retention reads every surface's slot set.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  detail_item_id: Option<&str>,
  message: DetailMessage,
) -> Task<Message> {
  match message {
    // Handled entirely by the top-level router: navigation writes the shared
    // destination stack and drives the other surfaces' leave/enter hooks.
    DetailMessage::Back => Task::none(),
    DetailMessage::Retry => start_load(surface, kernel, detail_item_id),
    DetailMessage::RetryNeighbors => start_followup(surface, kernel),
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
    DetailMessage::SeasonSelected(season_id) => {
      if !select_season(&mut surface.data, &season_id) {
        return Task::none();
      }
      start_selected_season_load(surface, kernel)
    }
    DetailMessage::FavoriteToggled => {
      start_user_data_update(surface, kernel, UserDataActionKind::Favorite)
    }
    DetailMessage::PlayedToggled => {
      start_user_data_update(surface, kernel, UserDataActionKind::Played)
    }
    DetailMessage::Loaded { token, result } => {
      if !settle_load(&mut surface.data, &mut kernel.request_gate, token, *result) {
        return Task::none();
      }
      let followup = start_followup(surface, kernel);
      Task::batch([followup, prepare_artwork(surface, kernel)])
    }
    DetailMessage::SeasonLoaded { token, result } => {
      if !settle_season_load(&mut surface.data, &mut kernel.request_gate, token, result) {
        return Task::none();
      }
      prepare_artwork(surface, kernel)
    }
    DetailMessage::NeighborsLoaded { token, result } => {
      if !kernel.request_gate.finish_detail_aux(token) {
        return Task::none();
      }
      surface.data.season_neighbors = match result {
        Ok(items) => jellypilot_core::LoadState::Ready(items),
        Err(_) => jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned()),
      };
      prepare_artwork(surface, kernel)
    }
    DetailMessage::SimilarLoaded { token, result } => {
      if !kernel.request_gate.finish_detail_aux(token) {
        return Task::none();
      }
      surface.data.similar_items = match result {
        Ok(items) => jellypilot_core::LoadState::Ready(items),
        Err(_) => jellypilot_core::LoadState::Failed(SIMILAR_FAILURE.to_owned()),
      };
      prepare_artwork(surface, kernel)
    }
    DetailMessage::UserDataUpdated { token, result } => {
      let Some(update) =
        settle_user_data_update(&mut surface.data, &mut kernel.request_gate, token, result)
      else {
        return Task::none();
      };
      if let Some(update) = update {
        if let Some(item) = surface.items.get_mut(&update.item_id) {
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

fn apply_artwork_completion(
  surface: &mut Surface,
  kernel: &mut Kernel,
  session_ok: bool,
  completion: ArtworkLoadCompletion,
) {
  if kernel
    .artwork_binder
    .settle(completion.slot, ArtworkSurface::Detail, session_ok)
    != ArtworkSettlement::Apply
  {
    return;
  }
  let Some(cell) = surface
    .artwork
    .cell_mut(completion.slot, &completion.image_id)
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

/// Starts (or reloads) the detail load for `item_id`. The top-level router
/// also calls this when navigating to a Detail destination.
pub fn start_load(
  surface: &mut Surface,
  kernel: &mut Kernel,
  item_id: Option<&str>,
) -> Task<Message> {
  let Some(item_id) = item_id else {
    return Task::none();
  };
  let Some(item) = surface.items.get(item_id).cloned() else {
    surface.data.content = jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned());
    return Task::none();
  };
  surface.data.clear();
  begin_artwork_view(surface, kernel);
  kernel
    .request_gate
    .set_detail_item(Some(item_id.to_owned()));
  let token = kernel.request_gate.begin_detail();
  surface.data.content = jellypilot_core::LoadState::Loading;
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.data.content = jellypilot_core::LoadState::Failed(DETAIL_FAILURE.to_owned());
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

fn settle_load(
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

fn start_followup(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  enum Followup {
    Episode {
      item_id: String,
      series_id: String,
      season_number: i32,
    },
    Movie(String),
    Show {
      item_id: String,
      selected_season_id: Option<String>,
    },
    None,
  }

  let followup = match &surface.data.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item))
      if item.item_type.eq_ignore_ascii_case("episode") =>
    {
      match (item.series_id.as_ref(), item.season_number) {
        (Some(series_id), Some(season_number)) => Followup::Episode {
          item_id: item.id.clone(),
          series_id: series_id.clone(),
          season_number,
        },
        _ => Followup::None,
      }
    }
    jellypilot_core::LoadState::Ready(DetailContent::Item(item))
      if item.item_type.eq_ignore_ascii_case("movie") =>
    {
      Followup::Movie(item.id.clone())
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => Followup::Show {
      item_id: show.id.clone(),
      selected_season_id: initial_season(show).map(|season| season.id.clone()),
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
      surface.data.similar_items = jellypilot_core::LoadState::Idle;
      start_neighbors_load(surface, kernel, item_id, series_id, season_number)
    }
    Followup::Movie(item_id) => {
      surface.data.season_neighbors = jellypilot_core::LoadState::Idle;
      start_similar_load(surface, kernel, item_id)
    }
    Followup::Show {
      item_id,
      selected_season_id,
    } => {
      surface.data.selected_season_id = selected_season_id;
      Task::batch([
        start_selected_season_load(surface, kernel),
        start_similar_load(surface, kernel, item_id),
      ])
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
    surface.data.season_neighbors = jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned());
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
    surface.data.similar_items = jellypilot_core::LoadState::Failed(SIMILAR_FAILURE.to_owned());
    return Task::none();
  };
  Task::perform(
    async move {
      load_similar_items(client.as_ref(), item_id)
        .await
        .map_err(|_| SIMILAR_FAILURE.to_owned())
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
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.data.season_episodes = jellypilot_core::LoadState::Failed(SEASON_FAILURE.to_owned());
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

fn start_user_data_update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  kind: UserDataActionKind,
) -> Task<Message> {
  if surface.data.user_data_busy.is_some() {
    return Task::none();
  }
  let Some((item_id, played, favorite)) = detail_user_data(&surface.data.content) else {
    return Task::none();
  };
  let action = match kind {
    UserDataActionKind::Favorite if favorite => VideoUserDataAction::Unfavorite,
    UserDataActionKind::Favorite => VideoUserDataAction::Favorite,
    UserDataActionKind::Played if played => VideoUserDataAction::MarkUnplayed,
    UserDataActionKind::Played => VideoUserDataAction::MarkPlayed,
  };
  let Some(token) = kernel
    .request_gate
    .begin_detail_aux(DetailAuxKind::UserData)
  else {
    return Task::none();
  };
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    surface.data.user_data_error = Some(USER_DATA_FAILURE.to_owned());
    return Task::none();
  };
  surface.data.user_data_busy = Some(kind);
  surface.data.user_data_error = None;
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

struct ArtworkLoadSpec {
  key: String,
  image_id: String,
  size_class: ArtworkSizeClass,
  visible: bool,
}

fn prepare_artwork(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let mut specs = Vec::new();
  let mut next_episode_retention_key = None;
  match &surface.data.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      push_artwork_spec(
        &mut specs,
        DETAIL_LOGO_KEY.to_owned(),
        &item.logo_image_id,
        ArtworkSizeClass::Hero,
        true,
      );
      // Fall back to the item's own artwork (episode still / poster) when the
      // server carries no backdrop, so the hero never settles empty.
      let hero_backdrop = item
        .backdrop_image_id
        .clone()
        .or_else(|| item.artwork_image_id.clone());
      push_artwork_spec(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &hero_backdrop,
        ArtworkSizeClass::Backdrop,
        true,
      );
      if let jellypilot_core::LoadState::Ready(neighbors) = &surface.data.season_neighbors {
        for episode in neighbors {
          push_artwork_spec(
            &mut specs,
            detail_episode_key(&episode.id),
            &episode.artwork_image_id,
            ArtworkSizeClass::Card,
            false,
          );
        }
      }
      if let jellypilot_core::LoadState::Ready(items) = &surface.data.similar_items {
        for item in items {
          push_artwork_spec(
            &mut specs,
            detail_similar_key(&item.id),
            &item.artwork_image_id,
            ArtworkSizeClass::Card,
            false,
          );
        }
      }
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      push_artwork_spec(
        &mut specs,
        DETAIL_LOGO_KEY.to_owned(),
        &show.logo_image_id,
        ArtworkSizeClass::Hero,
        true,
      );
      // Fall back to the show's own poster artwork when the server carries no
      // backdrop, so the hero never settles empty.
      let hero_backdrop = show
        .backdrop_image_id
        .clone()
        .or_else(|| show.artwork_image_id.clone());
      push_artwork_spec(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &hero_backdrop,
        ArtworkSizeClass::Backdrop,
        true,
      );
      if let Some(next_episode) = &show.next_episode {
        if next_episode.artwork_image_id.is_some() {
          push_artwork_spec(
            &mut specs,
            detail_episode_key(&next_episode.id),
            &next_episode.artwork_image_id,
            ArtworkSizeClass::Card,
            true,
          );
        } else {
          // Show and season endpoints can disagree on image fields. Keep a
          // season-populated cell for the always-visible Next Up card.
          next_episode_retention_key = Some(detail_episode_key(&next_episode.id));
        }
      }
      if let jellypilot_core::LoadState::Ready(page) = &surface.data.season_episodes {
        for episode in &page.episodes {
          if show.next_episode.as_ref().is_some_and(|next_episode| {
            next_episode.id == episode.id && next_episode.artwork_image_id.is_some()
          }) {
            continue;
          }
          push_artwork_spec(
            &mut specs,
            detail_episode_key(&episode.id),
            &episode.artwork_image_id,
            ArtworkSizeClass::Card,
            false,
          );
        }
      }
      if let jellypilot_core::LoadState::Ready(items) = &surface.data.similar_items {
        for item in items {
          push_artwork_spec(
            &mut specs,
            detail_similar_key(&item.id),
            &item.artwork_image_id,
            ArtworkSizeClass::Card,
            false,
          );
        }
      }
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => return Task::none(),
  }

  let mut retained_keys = specs
    .iter()
    .map(|spec| spec.key.as_str())
    .collect::<HashSet<_>>();
  if let Some(key) = next_episode_retention_key.as_deref() {
    retained_keys.insert(key);
  }
  surface.artwork.retain_keys(&retained_keys);
  drop(retained_keys);
  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let adapter = Arc::clone(&kernel.artwork_adapter);
  let mut summary = ArtworkLoadSummary::default();
  let mut load_specs = Vec::new();
  for spec in specs {
    let derived = DerivedArtwork {
      frosted_strip: None,
      logo_shadow: spec.key == DETAIL_LOGO_KEY,
    };
    if let Some(cell) = surface.artwork.get(&spec.key) {
      if cell.image_id == spec.image_id {
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
    }

    if let Some(raster) = adapter.cached_with_derived(&spec.image_id, spec.size_class, derived) {
      summary.record(&ArtworkLoadObservation::raster_hit(raster.byte_len() as u64));
      let slot = kernel.artwork_binder.bind_settled();
      kernel.artwork_handles.insert(
        slot,
        spec.image_id.clone(),
        super::state::ArtworkHandles::from_raster(raster),
      );
      surface.artwork.insert(
        spec.key,
        ArtworkCell {
          slot,
          image_id: spec.image_id,
          state: ArtworkCellState::Ready,
        },
      );
      continue;
    }

    let slot = kernel.artwork_binder.bind(ArtworkSurface::Detail);
    surface.artwork.insert(
      spec.key,
      ArtworkCell {
        slot,
        image_id: spec.image_id.clone(),
        state: ArtworkCellState::Loading,
      },
    );
    load_specs.push(PlannedArtworkLoad {
      slot,
      image_id: spec.image_id,
      size_class: spec.size_class,
      visible: spec.visible,
      derived,
    });
  }
  stream_artwork_loads(
    adapter,
    client,
    session,
    load_specs,
    summary,
    |session, completion| {
      Message::Detail(DetailMessage::ArtworkLoaded {
        session,
        slot: completion.slot,
        image_id: completion.image_id,
        result: completion.result,
      })
    },
  )
}

fn push_artwork_spec(
  specs: &mut Vec<ArtworkLoadSpec>,
  key: String,
  image_id: &Option<String>,
  size_class: ArtworkSizeClass,
  visible: bool,
) {
  if let Some(image_id) = image_id {
    specs.push(ArtworkLoadSpec {
      key,
      image_id: image_id.clone(),
      size_class,
      visible,
    });
  }
}

fn begin_artwork_view(surface: &mut Surface, kernel: &mut Kernel) {
  kernel.artwork_binder.begin_view(ArtworkSurface::Detail);
  surface.artwork.clear();
}

/// Detail leave hook, invoked by the top-level router when the destination
/// switches away from Detail: invalidates in-flight detail requests so a late
/// settlement cannot apply, and cancels pending artwork while playback is
/// idle.
pub(crate) fn leave_view(surface: &mut Surface, kernel: &mut Kernel, playback_idle: bool) {
  kernel.request_gate.navigate();
  if playback_idle {
    kernel.artwork_adapter.cancel_pending();
  }
  begin_artwork_view(surface, kernel);
  surface.data.clear();
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_auth::AuthStore;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::{JellyfinClient, VideoSeason};

  use super::*;
  use crate::app::state::ArtworkHandleRetention;

  /// The old `test_state` has no now-playing entry, so playback is idle.
  const PLAYBACK_IDLE: bool = true;

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let kernel = Kernel {
      settings,
      diagnostics: Diagnostics::default(),
      auth_store: AuthStore::default(),
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
      artwork_binder: Default::default(),
      artwork_handles: ArtworkHandleRetention::default(),
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

  #[test]
  fn user_data_transition_waits_for_confirmation_and_preserves_data_on_failure() {
    let mut detail = DetailState {
      content: jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item(
        "item-1",
      )))),
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
    let (mut surface, mut kernel) = test_fixture();
    kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    let stale = kernel
      .request_gate
      .begin_detail_aux(DetailAuxKind::UserData)
      .expect("detail item should permit user-data update");

    leave_view(&mut surface, &mut kernel, PLAYBACK_IDLE);
    kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));
    surface.data.user_data_busy = Some(UserDataActionKind::Favorite);

    let settlement = settle_user_data_update(
      &mut surface.data,
      &mut kernel.request_gate,
      stale,
      Ok(VideoUserDataUpdate {
        item_id: "item-1".to_owned(),
        played: true,
        favorite: true,
      }),
    );

    assert!(settlement.is_none());
    assert!(matches!(
      &surface.data.content,
      jellypilot_core::LoadState::Ready(DetailContent::Item(item))
        if !item.played && !item.favorite
    ));
    assert_eq!(
      surface.data.user_data_busy,
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
  fn episode_followup_never_starts_similar_items() {
    let (mut surface, mut kernel) = test_fixture();
    let mut item = video_item("episode-1");
    item.item_type = "Episode".to_owned();
    item.series_id = Some("show-1".to_owned());
    item.season_number = Some(1);
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(item)));
    kernel
      .request_gate
      .set_detail_item(Some("episode-1".to_owned()));

    drop(start_followup(&mut surface, &mut kernel));

    assert!(matches!(
      surface.data.similar_items,
      jellypilot_core::LoadState::Idle
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
  fn prepare_artwork_registers_the_logo_and_does_not_reissue_its_inflight_load() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-item-1");
    item.logo_image_id = Some("detail-logo-1".to_owned());
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(item)));

    // First prepare starts the initial load
    drop(prepare_artwork(&mut surface, &mut kernel));
    let cell = surface
      .artwork
      .get(DETAIL_LOGO_KEY)
      .expect("logo cell exists");
    let original_slot = cell.slot;
    assert_eq!(cell.state, ArtworkCellState::Loading);

    // Follow-up prepare (e.g. neighbors loaded) does not re-issue or replace the slot
    surface.data.season_neighbors = jellypilot_core::LoadState::Ready(Vec::new());
    let warm_task = prepare_artwork(&mut surface, &mut kernel);
    assert_eq!(warm_task.units(), 0);
    let second_cell = surface
      .artwork
      .get(DETAIL_LOGO_KEY)
      .expect("logo cell exists");
    assert_eq!(second_cell.slot, original_slot);
    assert_eq!(second_cell.state, ArtworkCellState::Loading);
  }

  #[test]
  fn prepare_artwork_keeps_next_up_image_when_selected_season_changes() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let next_up = episode("next-up", 1);
    let mut season_next_up = next_up.clone();
    season_next_up.artwork_image_id = Some("next-up-image".to_owned());
    let mut show = show_detail();
    show.next_episode = Some(next_up);
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Show(Box::new(show)));
    surface.data.season_episodes = jellypilot_core::LoadState::Ready(VideoSeasonEpisodesPage {
      series_id: "show-1".to_owned(),
      season_id: Some("season-1".to_owned()),
      season_number: Some(1),
      start_index: 0,
      limit: 30,
      total_record_count: 1,
      next_start_index: 1,
      has_more: false,
      episodes: vec![season_next_up],
    });

    drop(prepare_artwork(&mut surface, &mut kernel));
    let next_up_key = detail_episode_key("next-up");
    assert!(surface.artwork.get(&next_up_key).is_some());

    surface.data.season_episodes = jellypilot_core::LoadState::Ready(VideoSeasonEpisodesPage {
      series_id: "show-1".to_owned(),
      season_id: Some("season-2".to_owned()),
      season_number: Some(2),
      start_index: 0,
      limit: 30,
      total_record_count: 1,
      next_start_index: 1,
      has_more: false,
      episodes: vec![episode("season-2-episode", 2)],
    });

    drop(prepare_artwork(&mut surface, &mut kernel));

    assert!(surface.artwork.get(&next_up_key).is_some());
  }

  #[test]
  fn prepare_artwork_falls_back_to_primary_artwork_when_backdrop_is_missing() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-item-1");
    item.artwork_image_id = Some("detail-primary-1".to_owned());
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(item)));

    drop(prepare_artwork(&mut surface, &mut kernel));

    let cell = surface
      .artwork
      .get(DETAIL_BACKDROP_KEY)
      .expect("backdrop cell exists");
    assert_eq!(cell.image_id, "detail-primary-1");
  }

  #[test]
  fn prepare_artwork_prefers_the_real_backdrop_over_primary_artwork() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-item-1");
    item.backdrop_image_id = Some("detail-backdrop-1".to_owned());
    item.artwork_image_id = Some("detail-primary-1".to_owned());
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(item)));

    drop(prepare_artwork(&mut surface, &mut kernel));

    let cell = surface
      .artwork
      .get(DETAIL_BACKDROP_KEY)
      .expect("backdrop cell exists");
    assert_eq!(cell.image_id, "detail-backdrop-1");
  }

  #[test]
  fn detail_memory_cache_hit_synchronously_settles_without_retained_handle() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-cache-item");
    item.logo_image_id = Some("detail-cache-logo".to_owned());
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(item)));
    kernel.artwork_adapter.seed_raster_with_derived_for_test(
      "detail-cache-logo",
      jellypilot_media_server::artwork::ArtworkSizeClass::Hero,
      jellypilot_media_server::artwork::DerivedArtwork {
        logo_shadow: true,
        ..jellypilot_media_server::artwork::DerivedArtwork::default()
      },
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
        1,
        1,
        vec![10, 20, 30, 40],
      ),
    );
    kernel.artwork_handles.clear();
    let warm_task = prepare_artwork(&mut surface, &mut kernel);
    // One sentinel unit reports the aggregate cache-hit telemetry event.
    assert_eq!(warm_task.units(), 1);

    let logo = surface
      .artwork
      .get(DETAIL_LOGO_KEY)
      .expect("detail logo exists");
    assert_eq!(logo.state, ArtworkCellState::Ready);
    assert!(kernel
      .artwork_handles
      .get(logo.slot, "detail-cache-logo")
      .is_some());
  }
}
