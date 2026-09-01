//! Detail surface (ADR 0029): item/show detail loading, season episode
//! paging, user-data actions, and the detail artwork pipeline (poster,
//! backdrop, and episode cards).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::widget::image;
use iced::Task;
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::artwork_loader::PlannedArtworkLoad;
use jellypilot_core::detail::{
  apply_user_data_update, detail_episode_key, detail_user_data, initial_season,
  load_detail_content, load_season_neighbors, selected_season_request, DetailContent,
};
use jellypilot_core::request_gate::{DetailAuxKind, DetailToken, RequestGate};
use jellypilot_media_server::artwork::{
  ArtworkLoadObservation, ArtworkLoadSummary, ArtworkSizeClass,
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
const USER_DATA_FAILURE: &str = "Could not update user data. Try again.";

const DETAIL_POSTER_KEY: &str = "detail-poster";
const DETAIL_BACKDROP_KEY: &str = "detail-backdrop";

/// Detail surface slice: the library items opened into Detail, the loaded
/// detail view state, and the artwork cells bound for the poster, backdrop,
/// and episode cards.
#[derive(Default)]
pub struct Surface {
  pub items: HashMap<String, VideoLibraryItem>,
  pub data: DetailState,
  pub artwork: DetailArtwork,
}

/// `detail_item_id` is the shell's current `Destination::Detail` item id,
/// computed by the top-level router so this module never reads navigation
/// state (ADR 0029). The router also retains artwork handles across all
/// surfaces after a content/season/neighbors settlement re-prepares the
/// pipeline, because retention reads every surface's slot set.
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
        image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels()),
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
  match &surface.data.content {
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
        surface.data.season_neighbors = jellypilot_core::LoadState::Idle;
        return Task::none();
      };
      let Some(token) = kernel
        .request_gate
        .begin_detail_aux(DetailAuxKind::SeasonNeighbors)
      else {
        return Task::none();
      };
      surface.data.season_neighbors = jellypilot_core::LoadState::Loading;
      let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
        surface.data.season_neighbors =
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
      surface.data.selected_season_id = initial_season(show).map(|season| season.id.clone());
      start_selected_season_load(surface, kernel)
    }
    jellypilot_core::LoadState::Idle
    | jellypilot_core::LoadState::Loading
    | jellypilot_core::LoadState::Failed(_) => Task::none(),
  }
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
  match &surface.data.content {
    jellypilot_core::LoadState::Ready(DetailContent::Item(item)) => {
      push_artwork_spec(
        &mut specs,
        DETAIL_POSTER_KEY.to_owned(),
        &item.artwork_image_id,
        ArtworkSizeClass::Hero,
        true,
      );
      push_artwork_spec(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &item.backdrop_image_id,
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
    }
    jellypilot_core::LoadState::Ready(DetailContent::Show(show)) => {
      push_artwork_spec(
        &mut specs,
        DETAIL_POSTER_KEY.to_owned(),
        &show.artwork_image_id,
        ArtworkSizeClass::Hero,
        true,
      );
      push_artwork_spec(
        &mut specs,
        DETAIL_BACKDROP_KEY.to_owned(),
        &show.backdrop_image_id,
        ArtworkSizeClass::Backdrop,
        true,
      );
      if let Some(next) = &show.next_episode {
        push_artwork_spec(
          &mut specs,
          detail_episode_key(&next.id),
          &next.artwork_image_id,
          ArtworkSizeClass::Card,
          false,
        );
      }
      if let jellypilot_core::LoadState::Ready(page) = &surface.data.season_episodes {
        for episode in &page.episodes {
          push_artwork_spec(
            &mut specs,
            detail_episode_key(&episode.id),
            &episode.artwork_image_id,
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

  let retained_keys = specs
    .iter()
    .map(|spec| spec.key.as_str())
    .collect::<HashSet<_>>();
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

    if let Some(raster) = adapter.cached(&spec.image_id, spec.size_class) {
      summary.record(&ArtworkLoadObservation::raster_hit(raster.byte_len() as u64));
      let slot = kernel.artwork_binder.bind_settled();
      let handle = image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels());
      kernel
        .artwork_handles
        .insert(slot, spec.image_id.clone(), handle);
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
      backdrop_image_id: None,
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

    assert!(!settle_load(
      &mut detail,
      &mut gate,
      stale,
      Ok(DetailContent::Item(video_item("stale"))),
    ));
    assert!(matches!(
      detail.content,
      jellypilot_core::LoadState::Loading
    ));
    assert!(settle_load(
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
      jellypilot_core::LoadState::Ready(DetailContent::Item(video_item("item-1")));
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
      content: jellypilot_core::LoadState::Ready(DetailContent::Show(show)),
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
  fn inflight_loading_artwork_is_not_re_issued_on_followup_prepare() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-item-1");
    item.artwork_image_id = Some("detail-art-1".to_owned());
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(item));

    // First prepare starts the initial load
    drop(prepare_artwork(&mut surface, &mut kernel));
    let cell = surface
      .artwork
      .get(DETAIL_POSTER_KEY)
      .expect("poster cell exists");
    let original_slot = cell.slot;
    assert_eq!(cell.state, ArtworkCellState::Loading);

    // Follow-up prepare (e.g. neighbors loaded) does not re-issue or replace the slot
    surface.data.season_neighbors = jellypilot_core::LoadState::Ready(Vec::new());
    let warm_task = prepare_artwork(&mut surface, &mut kernel);
    assert_eq!(warm_task.units(), 0);
    let second_cell = surface
      .artwork
      .get(DETAIL_POSTER_KEY)
      .expect("poster cell exists");
    assert_eq!(second_cell.slot, original_slot);
    assert_eq!(second_cell.state, ArtworkCellState::Loading);
  }

  #[test]
  fn detail_memory_cache_hit_synchronously_settles_without_retained_handle() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = video_item("detail-cache-item");
    item.artwork_image_id = Some("detail-cache-art".to_owned());
    surface.data.content = jellypilot_core::LoadState::Ready(DetailContent::Item(item));
    kernel.artwork_adapter.seed_raster_for_test(
      "detail-cache-art",
      jellypilot_media_server::artwork::ArtworkSizeClass::Hero,
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

    let poster = surface
      .artwork
      .get(DETAIL_POSTER_KEY)
      .expect("detail poster exists");
    assert_eq!(poster.state, ArtworkCellState::Ready);
    assert!(kernel
      .artwork_handles
      .get(poster.slot, "detail-cache-art")
      .is_some());
  }
}
