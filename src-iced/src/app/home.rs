//! Home surface (ADR 0029): Video Home section/shortcut loading and the home
//! artwork pipeline (featured hero plus the section card rows).

use std::collections::HashSet;
use std::sync::Arc;

use iced::widget::image;
use iced::Task;
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::artwork_loader::{visible_row_cards, PlannedArtworkLoad};
use jellypilot_core::request_gate::{HomeToken, RequestGate};
use jellypilot_media_server::artwork::{
  ArtworkLoadObservation, ArtworkLoadSummary, ArtworkSizeClass,
};
use jellypilot_media_server::home::{load_home_data, HomeDataResult};
use jellypilot_media_server::VideoLibraryItem;
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::tokens::TOKENS;

use super::artwork::stream_artwork_loads;
use super::kernel::Kernel;
use super::message::{ArtworkLoadCompletion, HomeMessage, Message};
use super::state::{ArtworkCell, ArtworkCellState, HomeArtwork, HomeSection, HomeState};
use super::view::home::{content_width, section_frame_size};

/// Home surface slice: Video Home section data plus the artwork cells bound
/// for the hero and the section card rows.
#[derive(Default)]
pub struct Surface {
  pub data: HomeState,
  pub artwork: HomeArtwork,
}

/// `playback_idle` is the playback surface's `now_playing.is_none()` fact and
/// `window_width` the shell's tracked window width; both are computed by the
/// top-level router so this module never reads playback or shell state (ADR
/// 0029). The router also retains artwork handles across all surfaces after a
/// `Loaded` settlement re-prepares the pipeline, because retention reads
/// every surface's slot set.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  playback_idle: bool,
  window_width: f32,
  message: HomeMessage,
) -> Task<Message> {
  match message {
    // Handled entirely by the top-level router: navigation writes the shared
    // destination stack and drives the other surfaces' leave/enter hooks.
    HomeMessage::Navigate(_) => Task::none(),
    HomeMessage::Retry => start_load(surface, kernel, playback_idle),
    HomeMessage::CardHoverEnter(item_id) => {
      surface.data.hovered_card = Some(item_id);
      Task::none()
    }
    HomeMessage::CardHoverExit(item_id) => {
      if surface.data.hovered_card.as_deref() == Some(item_id.as_str()) {
        surface.data.hovered_card = None;
      }
      Task::none()
    }
    HomeMessage::Loaded { token, result } => {
      if !settle(&mut surface.data, &mut kernel.request_gate, token, result) {
        return Task::none();
      }
      surface.data.hovered_card = None;
      prepare_artwork(surface, kernel, window_width)
    }
    HomeMessage::ArtworkLoaded {
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
    .settle(completion.slot, ArtworkSurface::Home, session_ok)
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

/// Starts (or refreshes) the Video Home load. The top-level router also calls
/// this after connect and when navigating to Home.
pub fn start_load(
  surface: &mut Surface,
  kernel: &mut Kernel,
  playback_idle: bool,
) -> Task<Message> {
  if !surface.data.has_ready_content() {
    surface.data.begin_load();
  }
  if playback_idle {
    kernel.artwork_adapter.cancel_pending();
    surface.artwork.prune_unready();
  }
  let token = kernel.request_gate.begin_home();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    let error = "The connected media server session is unavailable.".to_owned();
    surface.data.settle_video_home(Err(error.clone()));
    surface.data.settle_shortcuts(Err(error));
    return Task::none();
  };

  Task::perform(load_home_data(client), move |result| {
    Message::Home(HomeMessage::Loaded { token, result })
  })
}

fn settle(
  data: &mut HomeState,
  request_gate: &mut RequestGate,
  token: HomeToken,
  result: HomeDataResult,
) -> bool {
  if !request_gate.finish_home(token) {
    return false;
  }
  let (video_home, shortcuts) = result;
  data.settle_video_home(video_home);
  data.settle_shortcuts(shortcuts);
  true
}

/// Home leave hook, invoked by the top-level router when the destination
/// switches away from Home: invalidates the in-flight home load so a late
/// settlement cannot apply, and releases pending artwork while playback is
/// idle.
pub(crate) fn leave_view(surface: &mut Surface, kernel: &mut Kernel, playback_idle: bool) {
  surface.data.hovered_card = None;
  kernel.request_gate.begin_home();
  if playback_idle {
    kernel.artwork_adapter.cancel_pending();
    surface.artwork.prune_unready();
  }
}

#[derive(Clone, Copy)]
enum ArtworkPlacement {
  Hero,
  HeroBackdrop,
  Card(HomeSection),
}

struct ArtworkLoadSpec {
  placement: ArtworkPlacement,
  item_id: String,
  image_id: String,
  visible: bool,
}

impl ArtworkLoadSpec {
  fn size_class(&self) -> ArtworkSizeClass {
    match self.placement {
      ArtworkPlacement::Hero => ArtworkSizeClass::Hero,
      ArtworkPlacement::HeroBackdrop => ArtworkSizeClass::Backdrop,
      ArtworkPlacement::Card(_) => ArtworkSizeClass::Card,
    }
  }
}

fn prepare_artwork(surface: &mut Surface, kernel: &mut Kernel, window_width: f32) -> Task<Message> {
  if !surface.data.has_ready_content() {
    return Task::none();
  }
  let specs = artwork_specs(&surface.data, window_width);
  let hero_item_id = specs
    .iter()
    .find(|spec| matches!(spec.placement, ArtworkPlacement::Hero))
    .map(|spec| spec.item_id.as_str());
  let hero_backdrop_item_id = specs
    .iter()
    .find(|spec| matches!(spec.placement, ArtworkPlacement::HeroBackdrop))
    .map(|spec| spec.item_id.as_str());
  let mut section_item_ids: [HashSet<&str>; 4] = Default::default();
  for spec in &specs {
    if let ArtworkPlacement::Card(section) = spec.placement {
      section_item_ids[section.index()].insert(spec.item_id.as_str());
    }
  }
  surface
    .artwork
    .retain_items(hero_item_id, hero_backdrop_item_id, &section_item_ids);

  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let adapter = Arc::clone(&kernel.artwork_adapter);
  let mut summary = ArtworkLoadSummary::default();
  let mut load_specs = Vec::new();

  for spec in specs {
    let existing_cell = match spec.placement {
      ArtworkPlacement::Hero => surface.artwork.hero(&spec.item_id),
      ArtworkPlacement::HeroBackdrop => surface.artwork.hero_backdrop(&spec.item_id),
      ArtworkPlacement::Card(section) => surface.artwork.card(section, &spec.item_id),
    };
    if let Some(cell) = existing_cell {
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

    if let Some(raster) = adapter.cached(&spec.image_id, spec.size_class()) {
      summary.record(&ArtworkLoadObservation::raster_hit(raster.byte_len() as u64));
      let slot = kernel.artwork_binder.bind_settled();
      let handle = image::Handle::from_rgba(raster.width(), raster.height(), raster.into_pixels());
      kernel
        .artwork_handles
        .insert(slot, spec.image_id.clone(), handle);
      let cell = ArtworkCell {
        slot,
        image_id: spec.image_id,
        state: ArtworkCellState::Ready,
      };
      match spec.placement {
        ArtworkPlacement::Hero => surface.artwork.insert_hero(spec.item_id, cell),
        ArtworkPlacement::HeroBackdrop => {
          surface.artwork.insert_hero_backdrop(spec.item_id, cell);
        }
        ArtworkPlacement::Card(section) => {
          surface.artwork.insert_card(section, spec.item_id, cell);
        }
      }
      continue;
    }

    let slot = kernel.artwork_binder.bind(ArtworkSurface::Home);
    let size_class = spec.size_class();
    let cell = ArtworkCell {
      slot,
      image_id: spec.image_id.clone(),
      state: ArtworkCellState::Loading,
    };
    match spec.placement {
      ArtworkPlacement::Hero => surface.artwork.insert_hero(spec.item_id, cell),
      ArtworkPlacement::HeroBackdrop => {
        surface.artwork.insert_hero_backdrop(spec.item_id, cell);
      }
      ArtworkPlacement::Card(section) => {
        surface.artwork.insert_card(section, spec.item_id, cell);
      }
    }
    load_specs.push(PlannedArtworkLoad {
      slot,
      image_id: spec.image_id,
      size_class,
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
      Message::Home(HomeMessage::ArtworkLoaded {
        session,
        slot: completion.slot,
        image_id: completion.image_id,
        result: completion.result,
      })
    },
  )
}

fn artwork_specs(data: &HomeState, window_width: f32) -> Vec<ArtworkLoadSpec> {
  let mut specs = Vec::new();
  let featured_item = data.featured_item();
  if let Some(item) = featured_item {
    push_artwork_spec(&mut specs, ArtworkPlacement::Hero, item, true);
    if let Some(image_id) = &item.backdrop_image_id {
      specs.push(ArtworkLoadSpec {
        placement: ArtworkPlacement::HeroBackdrop,
        item_id: item.id.clone(),
        image_id: image_id.clone(),
        visible: true,
      });
    }
  }
  let featured_item_id = featured_item.map(|item| item.id.as_str());
  let class = SizeClass::from_width(window_width);
  let content_width = content_width(window_width, class);
  for section in HomeSection::ALL {
    if let jellypilot_core::LoadState::Ready(items) = data.section(section) {
      let (card_width, _) = section_frame_size(section);
      let visible_cards = visible_row_cards(content_width, card_width, TOKENS.spacing.s4);
      for (index, item) in items
        .iter()
        .filter(|item| Some(item.id.as_str()) != featured_item_id)
        .enumerate()
      {
        push_artwork_spec(
          &mut specs,
          ArtworkPlacement::Card(section),
          item,
          index < visible_cards,
        );
      }
    }
  }
  specs
}

fn push_artwork_spec(
  specs: &mut Vec<ArtworkLoadSpec>,
  placement: ArtworkPlacement,
  item: &VideoLibraryItem,
  visible: bool,
) {
  if let Some(image_id) = &item.artwork_image_id {
    specs.push(ArtworkLoadSpec {
      placement,
      item_id: item.id.clone(),
      image_id: image_id.clone(),
      visible,
    });
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_auth::AuthStore;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::JellyfinClient;

  use super::*;
  use crate::app::state::ArtworkHandleRetention;

  /// Matches the 1600px window width of the old update.rs `test_state`.
  const WINDOW_WIDTH: f32 = 1600.0;
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

  #[test]
  fn stale_home_settlement_does_not_replace_the_current_loading_state() {
    let mut home = HomeState::default();
    let mut gate = RequestGate::default();
    let stale = gate.begin_home();
    let _current = gate.begin_home();
    home.begin_load();

    let applied = settle(
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
  fn leave_and_return_with_unchanged_data_preserves_ready_artwork_and_avoids_loading_reset() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let item = episode("item-1", 1);
    let mut item_with_art = item.clone();
    item_with_art.artwork_image_id = Some("art-1".to_owned());
    surface
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item_with_art.clone()],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    surface.data.settle_shortcuts(Ok(Vec::new()));

    drop(prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH));
    let slot = surface
      .artwork
      .card(HomeSection::ContinueWatching, "item-1")
      .expect("card slot exists")
      .slot;

    let session = kernel.request_gate.current_session();
    drop(update(
      &mut surface,
      &mut kernel,
      PLAYBACK_IDLE,
      WINDOW_WIDTH,
      HomeMessage::ArtworkLoaded {
        session,
        slot,
        image_id: "art-1".to_owned(),
        result: Ok(
          jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
            1,
            1,
            vec![1, 2, 3, 4],
          ),
        ),
      },
    ));
    let initial_handle_id = kernel
      .artwork_handles
      .get(slot, "art-1")
      .expect("initial handle exists")
      .id();

    // Leave Home (the router's leave hook)
    leave_view(&mut surface, &mut kernel, PLAYBACK_IDLE);
    assert!(surface.data.has_ready_content());

    // Return to Home (the router's enter hook)
    drop(start_load(&mut surface, &mut kernel, PLAYBACK_IDLE));
    assert!(surface.data.has_ready_content());
    assert_eq!(
      surface
        .artwork
        .card(HomeSection::ContinueWatching, "item-1")
        .map(|cell| cell.state),
      Some(ArtworkCellState::Ready)
    );
    let post_nav_handle_id = kernel
      .artwork_handles
      .get(slot, "art-1")
      .expect("post-nav handle exists")
      .id();
    assert_eq!(initial_handle_id, post_nav_handle_id);

    // Identical refetch settles without resetting cell to Loading
    let warm_task = prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH);
    assert_eq!(warm_task.units(), 0);
    assert_eq!(
      surface
        .artwork
        .card(HomeSection::ContinueWatching, "item-1")
        .map(|cell| cell.state),
      Some(ArtworkCellState::Ready)
    );
    let refetch_handle_id = kernel
      .artwork_handles
      .get(slot, "art-1")
      .expect("refetched handle exists")
      .id();
    assert_eq!(initial_handle_id, refetch_handle_id);
  }

  #[test]
  fn home_memory_cache_hit_synchronously_settles_without_retained_handle() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let item = episode("item-2", 1);
    let mut item_with_art = item.clone();
    item_with_art.artwork_image_id = Some("cached-art-2".to_owned());
    surface
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item_with_art],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    surface.data.settle_shortcuts(Ok(Vec::new()));

    // Seed the raster cache directly on a fresh fixture, so no handle is
    // retained for the image when the prepare pass runs.
    kernel.artwork_adapter.seed_raster_for_test(
      "cached-art-2",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
        1,
        1,
        vec![10, 20, 30, 40],
      ),
    );
    let warm_task = prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH);
    // One sentinel unit reports the aggregate cache-hit telemetry event.
    assert_eq!(warm_task.units(), 1);

    let card_cell = surface
      .artwork
      .card(HomeSection::ContinueWatching, "item-2")
      .expect("card cell exists");
    assert_eq!(card_cell.state, ArtworkCellState::Ready);
    assert!(kernel
      .artwork_handles
      .get(card_cell.slot, "cached-art-2")
      .is_some());
  }

  #[test]
  fn artwork_stream_settlement_applies_each_completion_as_it_arrives() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut items = Vec::new();
    for index in 1..=3 {
      let mut item = episode(&format!("batch-item-{index}"), index);
      item.artwork_image_id = Some(format!("batch-art-{index}"));
      items.push(item);
    }
    surface
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: items,
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    surface.data.settle_shortcuts(Ok(Vec::new()));
    drop(prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH));

    let image_ids = (1..=3)
      .map(|index| format!("batch-art-{index}"))
      .collect::<Vec<_>>();
    let completions = image_ids
      .iter()
      .enumerate()
      .map(|(index, image_id)| {
        let item_id = image_id.replacen("batch-art", "batch-item", 1);
        let slot = surface
          .artwork
          .card(HomeSection::ContinueWatching, &item_id)
          .expect("batch card exists")
          .slot;
        let result = if index == 1 {
          Err(jellypilot_media_server::artwork::ArtworkError::FetchFailed)
        } else {
          Ok(
            jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
              1,
              1,
              vec![1, 2, 3, 4],
            ),
          )
        };
        ArtworkLoadCompletion {
          slot,
          image_id: image_id.clone(),
          result,
        }
      })
      .collect::<Vec<ArtworkLoadCompletion>>();
    let session = kernel.request_gate.current_session();
    for completion in completions {
      drop(update(
        &mut surface,
        &mut kernel,
        PLAYBACK_IDLE,
        WINDOW_WIDTH,
        HomeMessage::ArtworkLoaded {
          session,
          slot: completion.slot,
          image_id: completion.image_id,
          result: completion.result,
        },
      ));
    }

    for index in 1..=3 {
      let item_id = format!("batch-item-{index}");
      let image_id = format!("batch-art-{index}");
      let cell = surface
        .artwork
        .card(HomeSection::ContinueWatching, &item_id)
        .expect("settled batch card exists");
      if index == 2 {
        assert_eq!(cell.state, ArtworkCellState::Failed);
        assert!(kernel.artwork_handles.get(cell.slot, &image_id).is_none());
      } else {
        assert_eq!(cell.state, ArtworkCellState::Ready);
        assert!(kernel.artwork_handles.get(cell.slot, &image_id).is_some());
      }
    }
  }

  #[test]
  fn cancelled_artwork_load_does_not_mark_cell_failed_and_is_reloaded_on_revisit() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = episode("item-cancel", 1);
    item.artwork_image_id = Some("art-cancel".to_owned());
    surface
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    surface.data.settle_shortcuts(Ok(Vec::new()));

    drop(prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH));
    let slot = surface
      .artwork
      .card(HomeSection::ContinueWatching, "item-cancel")
      .expect("card slot exists")
      .slot;

    // Leaving Home cancels pending loads and prunes unready cells
    leave_view(&mut surface, &mut kernel, PLAYBACK_IDLE);
    assert!(surface
      .artwork
      .card(HomeSection::ContinueWatching, "item-cancel")
      .is_none());

    // Late cancelled message arriving after leave does not mark failed
    let session = kernel.request_gate.current_session();
    drop(update(
      &mut surface,
      &mut kernel,
      PLAYBACK_IDLE,
      WINDOW_WIDTH,
      HomeMessage::ArtworkLoaded {
        session,
        slot,
        image_id: "art-cancel".to_owned(),
        result: Err(jellypilot_media_server::artwork::ArtworkError::Cancelled),
      },
    ));

    // Returning to Home re-prepares and binds a fresh load
    drop(start_load(&mut surface, &mut kernel, PLAYBACK_IDLE));
    drop(prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH));
    let new_cell = surface
      .artwork
      .card(HomeSection::ContinueWatching, "item-cancel")
      .expect("card cell is recreated on revisit");
    assert_eq!(new_cell.state, ArtworkCellState::Loading);
  }

  #[test]
  fn repeated_warm_prepares_maintain_zero_tracked_live_slots() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let item = episode("item-warm", 1);
    let mut item_with_art = item.clone();
    item_with_art.artwork_image_id = Some("art-warm".to_owned());
    surface
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![item_with_art],
        latest_movies: Vec::new(),
        next_up: Vec::new(),
        latest_episodes: Vec::new(),
      }));
    surface.data.settle_shortcuts(Ok(Vec::new()));

    // Cold prepare allocates 1 live in-flight slot
    drop(prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH));
    let cold_slot = surface
      .artwork
      .card(HomeSection::ContinueWatching, "item-warm")
      .unwrap()
      .slot;
    assert_eq!(kernel.artwork_binder.live_slots_count(), 1);

    // Settle the cold load -> live_slots_count becomes 0
    let session = kernel.request_gate.current_session();
    drop(update(
      &mut surface,
      &mut kernel,
      PLAYBACK_IDLE,
      WINDOW_WIDTH,
      HomeMessage::ArtworkLoaded {
        session,
        slot: cold_slot,
        image_id: "art-warm".to_owned(),
        result: Ok(
          jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
            1,
            1,
            vec![1, 2, 3, 4],
          ),
        ),
      },
    ));
    assert_eq!(kernel.artwork_binder.live_slots_count(), 0);

    // Repeated warm prepares (handle reuse / cached hit) must NOT leak live slots in ArtworkBinder
    for _ in 0..10 {
      let warm_task = prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH);
      assert_eq!(warm_task.units(), 0);
      assert_eq!(kernel.artwork_binder.live_slots_count(), 0);
    }
  }
}
