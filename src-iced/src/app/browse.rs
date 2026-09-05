//! Browse surface (ADR 0029): Library Browser paging, filter/sort
//! preferences, the scroll-driven display window, the sidebar search input,
//! and the browse artwork pipeline (grid cards).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::widget::operation;
use iced::{task, Task};
use jellypilot_core::artwork_binder::{ArtworkSettlement, ArtworkSurface};
use jellypilot_core::artwork_loader::{
  grid_cell_visible, visible_display_range, PlannedArtworkLoad,
};
use jellypilot_core::browse::fetch_browse_page;
use jellypilot_core::browse_model::{
  BrowseEffect, BrowseModel, BrowsePageRequest, BrowsePageSettlement, BrowsePreferences,
  BrowseSource, LibraryBrowseView,
};
use jellypilot_core::config::BrowseFilterSettings;
use jellypilot_core::LibraryBrowseLoadToken;
use jellypilot_media_server::artwork::{
  ArtworkLoadObservation, ArtworkLoadSummary, ArtworkSizeClass, DerivedArtwork,
};
use jellypilot_media_server::VideoLibrarySortDirection;
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::widgets::artwork_grid::{ArtworkGridMetrics, ArtworkGridViewport};

use super::artwork::stream_artwork_loads;
use super::kernel::Kernel;
use super::message::{ArtworkLoadCompletion, BrowseMessage, Message};
use super::state::{ArtworkCell, ArtworkCellState, BrowseArtwork, BrowseViewport};
use super::view::browse::{grid_available_width, CARD_COPY_HEIGHT};

/// Browse surface slice: the Library Browser model and its derived view, the
/// artwork cells bound for the grid cards, the in-flight page request
/// handles, the tracked scroll viewport, and the sidebar search input text.
pub struct Surface {
  pub data: BrowseModel,
  pub view: LibraryBrowseView,
  pub artwork: BrowseArtwork,
  pub page_tasks: HashMap<LibraryBrowseLoadToken, task::Handle>,
  pub viewport: BrowseViewport,
  pub scroll_id: iced::widget::Id,
  pub sort_menu_open: bool,
  pub search_input: String,
  refresh_fallback: Option<LibraryBrowseView>,
}

impl Default for Surface {
  fn default() -> Self {
    Self {
      data: BrowseModel::default(),
      view: LibraryBrowseView::Inactive,
      artwork: BrowseArtwork::default(),
      page_tasks: HashMap::new(),
      viewport: BrowseViewport::default(),
      scroll_id: iced::widget::Id::unique(),
      sort_menu_open: false,
      search_input: String::new(),
      refresh_fallback: None,
    }
  }
}

/// `source` is the router-resolved browse source for the current destination
/// (computed only for the filter-mutation messages, the only arms that
/// restart browsing), `in_library` whether the current destination is a
/// Library route (filter mutations apply nowhere else), `playback_idle` the
/// playback surface's `now_playing.is_none()` fact, and `window_size` the
/// shell's tracked window size; all are computed by the top-level router so
/// this module never reads navigation, home, playback, or shell state (ADR
/// 0029). The router also retains artwork handles across all surfaces after a
/// page settlement or scroll-window sync re-prepares the pipeline, because
/// retention reads every surface's slot set.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
  in_library: bool,
  playback_idle: bool,
  window_size: iced::Size,
  message: BrowseMessage,
) -> Task<Message> {
  match message {
    // Handled entirely by the top-level router: submission reads the search
    // input and navigates to a Search destination, which drives this
    // surface's enter hook.
    BrowseMessage::SearchSubmitted => Task::none(),
    BrowseMessage::SearchInputChanged(value) => {
      surface.search_input = value;
      Task::none()
    }
    BrowseMessage::SortMenuToggled => {
      surface.sort_menu_open = !surface.sort_menu_open;
      Task::none()
    }
    BrowseMessage::SortMenuDismissed => {
      surface.sort_menu_open = false;
      Task::none()
    }
    BrowseMessage::SortChanged(sort) => {
      surface.sort_menu_open = false;
      persist_filters(
        surface,
        kernel,
        source,
        in_library,
        playback_idle,
        |filters| filters.with_sort(sort),
      )
    }
    BrowseMessage::SortDirectionToggled => persist_filters(
      surface,
      kernel,
      source,
      in_library,
      playback_idle,
      |filters| {
        let direction = match filters.sort_direction() {
          VideoLibrarySortDirection::Ascending => VideoLibrarySortDirection::Descending,
          VideoLibrarySortDirection::Descending => VideoLibrarySortDirection::Ascending,
        };
        filters.with_sort_direction(direction)
      },
    ),
    BrowseMessage::PlayedFilterChanged(played_filter) => persist_filters(
      surface,
      kernel,
      source,
      in_library,
      playback_idle,
      |filters| filters.with_played_filter(played_filter),
    ),
    BrowseMessage::FavoritesToggled => persist_filters(
      surface,
      kernel,
      source,
      in_library,
      playback_idle,
      |filters| filters.with_favorites_only(!filters.favorites_only()),
    ),
    BrowseMessage::Scrolled(viewport) => {
      let bounds = viewport.bounds();
      let offset = viewport.absolute_offset();
      surface.viewport = BrowseViewport {
        offset_y: offset.y,
        height: bounds.height,
      };
      sync_scroll_window(surface, kernel, window_size)
    }
    BrowseMessage::Retry => {
      let effects = match surface.data.retry() {
        Ok(effects) => effects,
        Err(error) => {
          kernel.notice = Some(format!("Could not retry library browsing: {error}"));
          return Task::none();
        }
      };
      sync_view(surface);
      apply_effects(surface, kernel, effects)
    }
    BrowseMessage::PageSettled(settlement) => {
      let current = surface.data.is_current_settlement(&settlement);
      if current {
        surface.page_tasks.remove(&settlement.token);
        if surface.refresh_fallback.is_some() {
          if let Err(error) = &settlement.result {
            kernel.notice = Some(format!("Could not refresh this page: {error}"));
          }
        }
      }
      let effects = match surface.data.settle(settlement) {
        Ok(effects) => effects,
        Err(error) => {
          kernel.notice = Some(format!("Could not apply library results: {error}"));
          return Task::none();
        }
      };
      sync_view(surface);
      Task::batch([
        apply_effects(surface, kernel, effects),
        sync_scroll_window(surface, kernel, window_size),
        prepare_artwork(surface, kernel, window_size.width),
      ])
    }
    BrowseMessage::ArtworkLoaded {
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
    .settle(completion.slot, ArtworkSurface::Browse, session_ok)
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

fn persist_filters(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
  in_library: bool,
  playback_idle: bool,
  mutation: impl FnOnce(BrowseFilterSettings) -> BrowseFilterSettings,
) -> Task<Message> {
  if !in_library {
    return Task::none();
  }
  let filters = mutation(kernel.settings.snapshot().browse_filters());
  if let Err(error) = kernel.settings.set_browse_filters(filters) {
    kernel.notice = Some(format!("Could not save library filters: {error}"));
    return Task::none();
  }
  start(surface, kernel, source, playback_idle)
}

/// Starts (or reconfigures) the Library Browser for `source`. The top-level
/// router also calls this when navigating to a Library or Search
/// destination.
pub fn start(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
  playback_idle: bool,
) -> Task<Message> {
  surface.refresh_fallback = None;
  let Some(source) = source else {
    abort_pages(surface);
    if let Err(error) = surface.data.reset() {
      kernel.notice = Some(format!("Could not reset library browsing: {error}"));
      return Task::none();
    }
    sync_view(surface);
    kernel.notice = Some("The selected library is no longer available.".to_owned());
    return Task::none();
  };
  let preferences = BrowsePreferences::from(kernel.settings.snapshot().browse_filters());
  let effects = match surface.data.configure_with_preferences(source, preferences) {
    Ok(effects) => effects,
    Err(error) => {
      kernel.notice = Some(format!("Could not open library browsing: {error}"));
      sync_view(surface);
      return Task::none();
    }
  };
  if playback_idle {
    kernel.artwork_adapter.cancel_pending();
  }
  begin_artwork_view(surface, kernel);
  sync_view(surface);
  apply_effects(surface, kernel, effects)
}

/// Recomputes the scroll-driven display window and loads newly visible pages.
///
/// The model no-ops an unchanged range, so callers may invoke this freely
/// after scroll, resize, and page-settlement events.
/// `pub(crate)` because the top-level router also invokes this on window
/// resize; when the range changed it re-prepares the artwork pipeline, so the
/// router retains artwork handles afterwards (retention reads every surface's
/// slot set, ADR 0029).
pub(crate) fn sync_scroll_window(
  surface: &mut Surface,
  kernel: &mut Kernel,
  window_size: iced::Size,
) -> Task<Message> {
  let LibraryBrowseView::Ready {
    total_record_count, ..
  } = &surface.view
  else {
    return Task::none();
  };
  let total = *total_record_count;
  let class = SizeClass::from_width(window_size.width);
  let metrics = ArtworkGridMetrics::for_cards(
    grid_available_width(window_size.width, class),
    CARD_COPY_HEIGHT,
  );
  // iced only publishes scroll viewport geometry for overflowing content, so
  // short libraries would never report a height; the window height is a safe
  // upper bound that keeps the auto-fill trigger alive for them.
  let viewport_height = surface.viewport.height.max(window_size.height);
  let range = visible_display_range(
    surface.viewport.offset_y,
    viewport_height,
    metrics.columns,
    metrics.row_height,
    total,
  );
  // Metadata-only peek: the hot scroll path must not clone the window's
  // items via `display_range()` just to compare the range.
  if surface.data.peek_display_range().as_ref() == Some(&range) {
    return Task::none();
  }
  let effects = match surface.data.set_display_range(range, total) {
    Ok(effects) => effects,
    Err(error) => {
      kernel.notice = Some(format!("Could not load more library items: {error}"));
      return Task::none();
    }
  };
  sync_view(surface);
  Task::batch([
    apply_effects(surface, kernel, effects),
    prepare_artwork(surface, kernel, window_size.width),
  ])
}

fn sync_view(surface: &mut Surface) {
  let view = surface.data.view();
  if matches!(
    view,
    LibraryBrowseView::Loading | LibraryBrowseView::Failed { .. }
  ) {
    surface.view = surface.refresh_fallback.clone().unwrap_or(view);
  } else {
    surface.refresh_fallback = None;
    surface.view = view;
  }
}

/// Reloads the current request identity while keeping usable cards on a failed refresh.
pub(crate) fn refresh(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
  playback_idle: bool,
) -> Task<Message> {
  let fallback = matches!(
    surface.view,
    LibraryBrowseView::Ready { .. } | LibraryBrowseView::Empty
  )
  .then(|| surface.view.clone());
  abort_pages(surface);
  if let Err(error) = surface.data.reset() {
    kernel.notice = Some(format!("Could not refresh this page: {error}"));
    return Task::none();
  }
  let task = start(surface, kernel, source, playback_idle);
  surface.refresh_fallback = fallback;
  sync_view(surface);
  task
}

fn apply_effects(
  surface: &mut Surface,
  kernel: &mut Kernel,
  effects: Vec<BrowseEffect>,
) -> Task<Message> {
  // Viewport resets must land before page requests: Task::batch runs in
  // parallel, so a fast settlement could evaluate the stale near-tail offset
  // and advance another window before scroll-to-zero is applied.
  let mut resets = Vec::new();
  let mut tasks = Vec::with_capacity(effects.len());
  for effect in effects {
    match effect {
      BrowseEffect::ResetViewport => {
        surface.viewport.offset_y = 0.0;
        resets.push(operation::scroll_to(
          surface.scroll_id.clone(),
          operation::AbsoluteOffset { x: 0.0, y: 0.0 },
        ));
      }
      BrowseEffect::RequestPage(request) => {
        tasks.push(start_page_request(surface, kernel, request));
      }
      BrowseEffect::CancelPage { token } => {
        if let Some(handle) = surface.page_tasks.remove(&token) {
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

fn start_page_request(
  surface: &mut Surface,
  kernel: &mut Kernel,
  request: BrowsePageRequest,
) -> Task<Message> {
  let token = request.token;
  let failure_message = failure_message(&request.source);
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::done(Message::Browse(BrowseMessage::PageSettled(
      BrowsePageSettlement {
        source_id: request.source_id,
        token,
        result: Err(failure_message.to_owned()),
      },
    )));
  };
  let (task, handle) = Task::perform(
    async move { fixed_failure(fetch_browse_page(client, request).await, failure_message) },
    |settlement| Message::Browse(BrowseMessage::PageSettled(settlement)),
  )
  .abortable();
  surface.page_tasks.insert(token, handle);
  task
}

const fn failure_message(source: &BrowseSource) -> &'static str {
  match source {
    BrowseSource::Library { .. } => "Could not load this library. Try again.",
    BrowseSource::Search { .. } => "Could not load these search results. Try again.",
  }
}

fn fixed_failure(
  mut settlement: BrowsePageSettlement,
  failure_message: &'static str,
) -> BrowsePageSettlement {
  if settlement.result.is_err() {
    settlement.result = Err(failure_message.to_owned());
  }
  settlement
}

/// `pub(crate)` so the router-level re-navigation test in `update.rs` can
/// drive the pipeline directly. Unlike the old `prepare_browse_artwork`,
/// this does not retain artwork handles: retention reads every surface's
/// slot set, so the top-level router performs it after the messages that
/// re-prepare the pipeline (ADR 0029).
pub(crate) fn prepare_artwork(
  surface: &mut Surface,
  kernel: &mut Kernel,
  window_width: f32,
) -> Task<Message> {
  let LibraryBrowseView::Ready {
    visible_items,
    visible_start,
    ..
  } = &surface.view
  else {
    return Task::none();
  };
  let visible_start = usize::try_from(*visible_start).unwrap_or(usize::MAX);
  let specs = visible_items
    .iter()
    .enumerate()
    .filter_map(|(index, slot)| {
      slot
        .item
        .as_ref()
        .map(|item| (index, item.id.clone(), item.artwork_image_id.clone()))
    })
    .collect::<Vec<_>>();
  let visible_ids = specs
    .iter()
    .map(|(_, item_id, _)| item_id.as_str())
    .collect::<HashSet<_>>();
  surface.artwork.retain_items(&visible_ids);
  drop(visible_ids);
  let session = kernel.request_gate.current_session();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::none();
  };
  let adapter = Arc::clone(&kernel.artwork_adapter);
  let class = SizeClass::from_width(window_width);
  let available_width = grid_available_width(window_width, class);
  let metrics = ArtworkGridMetrics::for_cards(available_width, CARD_COPY_HEIGHT);
  // The grid is the first scrollable child, so scroll coordinates are already
  // grid-local; the window start shifts slot positions to global grid indexes.
  let grid_viewport = ArtworkGridViewport::from_scroll_geometry(
    surface.viewport.offset_y,
    surface.viewport.height,
    0.0,
  );
  let mut summary = ArtworkLoadSummary::default();
  let mut load_specs = Vec::new();

  for (index, item_id, image_id) in specs {
    let Some(image_id) = image_id else {
      continue;
    };
    if let Some(cell) = surface.artwork.get(&item_id) {
      if cell.image_id == image_id {
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

    if let Some(raster) = adapter.cached(&image_id, ArtworkSizeClass::Card) {
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

    let slot = kernel.artwork_binder.bind(ArtworkSurface::Browse);
    surface.artwork.insert(
      item_id,
      ArtworkCell {
        slot,
        image_id: image_id.clone(),
        state: ArtworkCellState::Loading,
      },
    );
    load_specs.push(PlannedArtworkLoad {
      slot,
      image_id,
      size_class: ArtworkSizeClass::Card,
      visible: grid_cell_visible(
        visible_start.saturating_add(index),
        metrics.columns,
        grid_viewport.offset_y,
        grid_viewport.height,
        metrics.row_height,
      ),
      derived: DerivedArtwork::default(),
    });
  }

  stream_artwork_loads(
    adapter,
    client,
    session,
    load_specs,
    summary,
    |session, completion| {
      Message::Browse(BrowseMessage::ArtworkLoaded {
        session,
        slot: completion.slot,
        image_id: completion.image_id,
        result: completion.result,
      })
    },
  )
}

fn begin_artwork_view(surface: &mut Surface, kernel: &mut Kernel) {
  kernel.artwork_binder.begin_view(ArtworkSurface::Browse);
  surface.artwork.clear();
}

/// Browse leave hook, invoked by the top-level router when the destination
/// switches away from Library/Search: aborts in-flight page requests,
/// cancels pending artwork while playback is idle, and resets the model.
pub(crate) fn leave_view(surface: &mut Surface, kernel: &mut Kernel, playback_idle: bool) {
  abort_pages(surface);
  if playback_idle {
    kernel.artwork_adapter.cancel_pending();
  }
  begin_artwork_view(surface, kernel);
  if let Err(error) = surface.data.reset() {
    kernel.notice = Some(format!("Could not reset library browsing: {error}"));
  }
  sync_view(surface);
}

/// Browse portion of the router's connected-surface reset: aborts in-flight
/// page requests, drops the artwork cells, and resets the model and view.
pub(crate) fn reset(surface: &mut Surface, kernel: &mut Kernel) {
  surface.refresh_fallback = None;
  abort_pages(surface);
  surface.artwork = BrowseArtwork::default();
  if let Err(error) = surface.data.reset() {
    kernel.notice = Some(format!("Could not reset library browsing: {error}"));
  }
  surface.view = LibraryBrowseView::Inactive;
}

fn abort_pages(surface: &mut Surface) {
  for (_, handle) in surface.page_tasks.drain() {
    handle.abort();
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
  use jellypilot_media_server::{JellyfinClient, VideoLibraryItem};

  use super::*;
  use crate::app::state::ArtworkHandleRetention;

  /// Matches the 1600px window width of the old update.rs `test_state`.
  const WINDOW_WIDTH: f32 = 1600.0;
  /// Matches the 900px window height of the old update.rs `test_state`.
  const WINDOW_HEIGHT: f32 = 900.0;
  /// The old `test_state` has no now-playing entry, so playback is idle.
  const PLAYBACK_IDLE: bool = true;

  fn window_size() -> iced::Size {
    iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT)
  }

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

  fn browse_request(effects: Vec<BrowseEffect>) -> BrowsePageRequest {
    effects
      .into_iter()
      .find_map(|effect| match effect {
        BrowseEffect::RequestPage(request) => Some(request),
        BrowseEffect::ResetViewport | BrowseEffect::CancelPage { .. } => None,
      })
      .expect("browse request should be emitted")
  }

  fn search_source(kernel: &Kernel, query: &str) -> BrowseSource {
    BrowseSource::Search {
      session: kernel.request_gate.current_session(),
      query: query.to_owned(),
    }
  }

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      logo_image_id: None,
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

  #[test]
  fn identical_browse_resubmit_keeps_the_in_flight_request_handle() {
    let (mut surface, mut kernel) = test_fixture();
    let source = search_source(&kernel, "arrival");
    let request = browse_request(
      surface
        .data
        .configure(source.clone())
        .expect("search should configure"),
    );
    let (_, handle) = Task::<Message>::none().abortable();
    surface.page_tasks.insert(request.token, handle);

    drop(start(
      &mut surface,
      &mut kernel,
      Some(source),
      PLAYBACK_IDLE,
    ));

    assert!(surface.page_tasks.contains_key(&request.token));
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
  }

  #[test]
  fn stale_same_session_settlement_keeps_the_reopened_request_handle() {
    let (mut surface, mut kernel) = test_fixture();
    let source = search_source(&kernel, "arrival");
    let stale = browse_request(
      surface
        .data
        .configure(source.clone())
        .expect("first search should configure"),
    );
    surface.data.reset().expect("browse epoch should advance");
    let current = browse_request(
      surface
        .data
        .configure(source)
        .expect("search should reopen"),
    );
    let (_, handle) = Task::<Message>::none().abortable();
    surface.page_tasks.insert(current.token, handle);

    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      PLAYBACK_IDLE,
      window_size(),
      BrowseMessage::PageSettled(BrowsePageSettlement {
        source_id: stale.source_id,
        token: stale.token,
        result: Err("stale server response".to_owned()),
      }),
    ));

    assert!(surface.page_tasks.contains_key(&current.token));
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
  }

  #[test]
  fn browse_failure_messages_are_fixed_for_library_and_search_sources() {
    let (_, kernel) = test_fixture();
    let library = BrowseSource::Library {
      session: kernel.request_gate.current_session(),
      shortcut: jellypilot_media_server::VideoLibraryShortcut {
        id: "library-1".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: None,
        artwork_image_id: None,
      },
    };
    let search = search_source(&kernel, "arrival");

    assert_eq!(
      failure_message(&library),
      "Could not load this library. Try again."
    );
    assert_eq!(
      failure_message(&search),
      "Could not load these search results. Try again."
    );
    let settlement = fixed_failure(
      BrowsePageSettlement {
        source_id: "source".to_owned(),
        token: jellypilot_core::LibraryBrowseLoadToken {
          generation: 1,
          sequence: 1,
        },
        result: Err("HTTP 500: raw server response body".to_owned()),
      },
      failure_message(&search),
    );
    assert_eq!(
      settlement.result.as_ref().err().map(String::as_str),
      Some("Could not load these search results. Try again.")
    );
  }

  #[test]
  fn reset_viewport_effect_clears_the_recorded_scroll_offset() {
    let (mut surface, mut kernel) = test_fixture();
    surface.viewport.offset_y = 640.0;

    drop(apply_effects(
      &mut surface,
      &mut kernel,
      vec![BrowseEffect::ResetViewport],
    ));

    assert_eq!(surface.viewport.offset_y, 0.0);
  }

  #[test]
  fn browse_scroll_position_drives_the_display_window() {
    // 1600×900 window: 1248px grid, 8 columns, 275px rows; the 900px window
    // height covers 4 rows, so the settled bootstrap expands to 6 rows.
    let (mut surface, mut kernel) = test_fixture();
    let library = BrowseSource::Library {
      session: kernel.request_gate.current_session(),
      shortcut: jellypilot_media_server::VideoLibraryShortcut {
        id: "library-1".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: Some(264),
        artwork_image_id: None,
      },
    };
    let initial_request = browse_request(
      surface
        .data
        .configure(library)
        .expect("library should configure"),
    );
    sync_view(&mut surface);

    let settlement = BrowsePageSettlement {
      source_id: initial_request.source_id.clone(),
      token: initial_request.token,
      result: Ok(jellypilot_core::browse_model::BrowsePagePayload {
        start_index: 0,
        limit: 24,
        total_record_count: 264,
        has_more: true,
        items: (0..24)
          .map(|index| VideoLibraryItem {
            logo_image_id: None,
            id: format!("item-{index}"),
            name: format!("Item {index}"),
            item_type: "Movie".to_owned(),
            production_year: None,
            runtime_seconds: None,
            played: false,
            favorite: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            series_poster_image_id: None,
            episode_thumb_image_id: None,
            series_thumb_image_id: None,
            series_backdrop_image_id: None,
            season_number: None,
            episode_number: None,
            series_id: None,
            series_name: None,
            resume_position_seconds: None,
            played_percentage: None,
            overview: None,
            index_number_end: None,
            season_poster_image_id: None,
            end_year: None,
            series_continuing: false,
            unplayed_item_count: None,
          })
          .collect(),
      }),
    };
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      PLAYBACK_IDLE,
      window_size(),
      BrowseMessage::PageSettled(settlement),
    ));

    // Settlement triggers a scroll-window sync that fills the viewport.
    assert_eq!(surface.data.display_range(), Some(0..54));

    // Scrolling ten rows down shifts the window without resetting it.
    surface.viewport = BrowseViewport {
      offset_y: 2750.0,
      height: 800.0,
    };
    drop(sync_scroll_window(&mut surface, &mut kernel, window_size()));
    assert_eq!(surface.data.display_range(), Some(72..153));

    // An unchanged viewport keeps the window and emits no page requests.
    let pending_before = surface.page_tasks.len();
    drop(sync_scroll_window(&mut surface, &mut kernel, window_size()));
    assert_eq!(surface.data.display_range(), Some(72..153));
    assert_eq!(surface.page_tasks.len(), pending_before);

    // Scrolling back up restores the earlier window.
    surface.viewport = BrowseViewport {
      offset_y: 0.0,
      height: 800.0,
    };
    drop(sync_scroll_window(&mut surface, &mut kernel, window_size()));
    assert_eq!(surface.data.display_range(), Some(0..54));
  }

  #[test]
  fn browse_memory_cache_hit_synchronously_settles_without_retained_handle() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut item = episode("browse-cache-item-1", 1);
    item.artwork_image_id = Some("browse-cache-art-1".to_owned());

    // Seed the raster cache directly
    kernel.artwork_adapter.seed_raster_for_test(
      "browse-cache-art-1",
      jellypilot_media_server::artwork::ArtworkSizeClass::Card,
      jellypilot_media_server::artwork::ArtworkRaster::from_raw_for_test(
        1,
        1,
        vec![10, 20, 30, 40],
      ),
    );

    // Wipe artwork_handles completely so there is NO retained handle in artwork_handles
    kernel.artwork_handles.clear();

    surface.view = LibraryBrowseView::Ready {
      visible_items: vec![jellypilot_core::browse_model::LibraryItemSlot { item: Some(item) }],
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 1,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };

    let warm_task = prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH);
    // One sentinel unit reports the aggregate cache-hit telemetry event.
    assert_eq!(warm_task.units(), 1);

    let browse_cell = surface
      .artwork
      .get("browse-cache-item-1")
      .expect("browse cell exists");
    assert_eq!(browse_cell.state, ArtworkCellState::Ready);
    assert!(kernel
      .artwork_handles
      .get(browse_cell.slot, "browse-cache-art-1")
      .is_some());
  }

  #[test]
  fn page_settle_spawns_all_24_visible_browse_loads_at_once() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let items = (0..24)
      .map(|i| {
        let mut item = episode(&format!("item-{i}"), 1);
        item.artwork_image_id = Some(format!("art-{i}"));
        jellypilot_core::browse_model::LibraryItemSlot { item: Some(item) }
      })
      .collect::<Vec<_>>();

    surface.view = LibraryBrowseView::Ready {
      visible_items: items,
      visible_start: 0,
      mode: jellypilot_core::LibraryBrowseMode::Normal,
      total_record_count: 24,
      is_fetching_more: false,
      load_more_failure: None,
      retry_busy: false,
    };

    drop(prepare_artwork(&mut surface, &mut kernel, WINDOW_WIDTH));

    for i in 0..24 {
      let cell = surface
        .artwork
        .get(&format!("item-{i}"))
        .expect("all 24 cells must be present in browse_artwork");
      assert_eq!(
        cell.state,
        ArtworkCellState::Loading,
        "all 24 cells must be admitted into Loading in the same pass"
      );
    }
  }
}
