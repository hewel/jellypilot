//! Detail surface (ADR 0029): item/show detail loading, auxiliary shelves,
//! season episode paging, user-data actions, and the detail artwork pipeline.

use std::collections::HashMap;
use std::sync::Arc;

use crate::i18n::UiText;
use iced::Task;
use jellypilot_core::detail::{
  apply_user_data_update, detail_episode_key, detail_similar_key, detail_user_data, initial_season,
  load_detail_content, load_season_neighbors, load_similar_items, selected_season_request,
  DetailContent,
};
use jellypilot_core::request_gate::{DetailAuxKind, DetailToken, RequestGate};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::{
  VideoLibraryItem, VideoSeasonEpisodesPage, VideoUserDataAction, VideoUserDataUpdate,
  VideoUserDataUpdateRequest,
};

use super::artwork::{ImageCollection, ImageSpec};
use super::kernel::Kernel;
use super::message::{DetailMessage, Message};
use super::state::{DetailState, UserDataActionKind};

const DETAIL_FAILURE: &str = "detail-load-error";
const SEASON_FAILURE: &str = "detail-season-error";
const SIMILAR_FAILURE: &str = "detail-similar-error";
const USER_DATA_FAILURE: &str = "detail-user-data-error";

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
  pub(crate) collection_change: Option<super::collections::Change>,
  pub(crate) pending_user_data: HashMap<String, jellypilot_core::request_gate::DetailAuxToken>,
  refresh_token: Option<DetailToken>,
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
    DetailMessage::Back | DetailMessage::WatchlistToggled => Task::none(),
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
    DetailMessage::FavoriteToggled => {
      start_user_data_update(surface, kernel, UserDataActionKind::Favorite)
    }
    DetailMessage::PlayedToggled => {
      start_user_data_update(surface, kernel, UserDataActionKind::Played)
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
    DetailMessage::UserDataUpdated { token, result } => {
      if !kernel.request_gate.is_current_session(token.session())
        || surface.pending_user_data.get(token.item_id()) != Some(&token)
      {
        return Task::none();
      }
      surface.pending_user_data.remove(token.item_id());
      let result = result.and_then(|update| {
        if update.item_id == token.item_id() {
          Ok(update)
        } else {
          Err("User-data response targeted another item".to_owned())
        }
      });
      // A server write survives navigation; only Detail presentation is view-scoped.
      if let Ok(update) = &result {
        surface.collection_change = Some(super::collections::Change::Confirmed(update.clone()));
      }
      let failed = result.is_err();
      let settled =
        settle_user_data_update(&mut surface.data, &mut kernel.request_gate, token, result);
      if failed && settled.is_none() {
        return kernel.show_toast(
          super::state::NoticeLevel::Error,
          UiText::new(USER_DATA_FAILURE),
        );
      }
      Task::none()
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
  let Some(item_id) = item_id else {
    return Task::none();
  };
  let Some(item) = surface.items.get(item_id).cloned() else {
    surface.data.content = jellypilot_core::LoadState::Failed(UiText::new(DETAIL_FAILURE));
    return Task::none();
  };
  surface.data.clear();
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
  surface.data.user_data_busy = None;
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
  if surface.data.user_data_busy.is_some()
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
      selected_season_id: surface
        .data
        .selected_season_id
        .as_ref()
        .filter(|id| show.seasons.iter().any(|season| &season.id == *id))
        .cloned()
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
  if surface.pending_user_data.contains_key(&item_id) {
    return Task::none();
  }
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
    surface.data.user_data_error = Some(UiText::new(USER_DATA_FAILURE));
    return Task::none();
  };
  surface.data.user_data_busy = Some(kind);
  surface
    .pending_user_data
    .insert(item_id.clone(), token.clone());
  cancel_refresh(surface, kernel);
  surface.data.user_data_error = None;
  let request = VideoUserDataUpdateRequest { item_id, action };
  Task::perform(
    async move {
      client
        .library()
        .update_user_data(request)
        .await
        .map_err(|error| error.to_string())
    },
    move |result| Message::Detail(DetailMessage::UserDataUpdated { token, result }),
  )
}

/// A confirmed write must not be overwritten by an older metadata refresh.
pub(crate) fn cancel_refresh(surface: &mut Surface, kernel: &mut Kernel) {
  if let Some(token) = surface.refresh_token.take() {
    let _ = kernel.request_gate.finish_detail(token);
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
  if let Err(error) = &result {
    tracing::warn!(error = %jellypilot_core::diagnostics::sanitize_message(error.as_str()), "Detail user data update failed");
  }
  match result {
    Ok(update) if apply_user_data_update(&mut detail.content, &update) => {
      detail.user_data_error = None;
      Some(Some(update))
    }
    Ok(_) | Err(_) => {
      detail.user_data_error = Some(UiText::new(USER_DATA_FAILURE));
      Some(None)
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
pub(crate) fn leave_view(surface: &mut Surface, kernel: &mut Kernel) {
  surface.season_menu_open = false;
  kernel.request_gate.navigate();
  surface.artwork.clear();
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

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let kernel = Kernel {
      settings,
      locale: crate::i18n::Localizer::default(),
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
    assert!(detail.user_data_error.is_some());
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
  fn user_data_write_rejects_an_older_refresh_and_blocks_an_overlapping_refresh() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    kernel
      .request_gate
      .set_detail_item(Some("item-1".to_owned()));
    surface
      .items
      .insert("item-1".to_owned(), episode("item-1", 1));
    surface.data.content =
      jellypilot_core::LoadState::Ready(DetailContent::Item(Box::new(video_item("item-1"))));
    let _ = refresh(&mut surface, &mut kernel, "item-1");
    let stale = surface.refresh_token.expect("refresh starts");
    let _ = start_user_data_update(&mut surface, &mut kernel, UserDataActionKind::Favorite);
    assert!(surface.data.user_data_busy.is_some());
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

    leave_view(&mut surface, &mut kernel);
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
