//! Browse surface (ADR 0029): Library Browser paging, filter/sort
//! preferences, the scroll-driven display window, the sidebar search input,
//! and the browse artwork pipeline (grid cards).

use std::collections::HashMap;
use std::sync::Arc;

use crate::i18n::UiText;
use iced::widget::operation;
use iced::{task, Task};
use jellypilot_core::browse::fetch_browse_page;
use jellypilot_core::browse_model::{
  BrowseEffect, BrowseModel, BrowsePageRequest, BrowsePageSettlement, BrowsePreferences,
  BrowseSource, LibraryBrowseView,
};
use jellypilot_core::browse_window::visible_display_range;
use jellypilot_core::config::BrowseFilterSettings;
use jellypilot_core::diagnostics::{sanitize_message, DiagnosticCategory, DiagnosticLevel};
use jellypilot_core::LibraryBrowseLoadToken;
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::VideoLibrarySortDirection;
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::widgets::artwork_grid::{ArtworkGridMetrics, ArtworkGridViewport};

use super::artwork::{ImageCollection, ImageSpec};
use super::kernel::Kernel;
use super::message::{BrowseMessage, Message};
use super::state::BrowseViewport;
use super::view::browse::{grid_available_width, CARD_COPY_HEIGHT};

/// Browse surface slice: the Library Browser model and its derived view, the
/// artwork cells bound for the grid cards, the in-flight page request
/// handles, the tracked scroll viewport, and the sidebar search input text.
pub struct Surface {
  pub data: BrowseModel,
  pub view: LibraryBrowseView,
  pub artwork: ImageCollection,
  pub page_tasks: HashMap<LibraryBrowseLoadToken, task::Handle>,
  pub viewport: BrowseViewport,
  pub grid_viewport: Option<ArtworkGridViewport>,
  pub scroll_id: iced::widget::Id,
  pub sort_menu_open: bool,
  pub search_input: String,
  pub filters: Option<BrowseFilterSettings>,
  pub request_generation: u64,
  page_requests: HashMap<LibraryBrowseLoadToken, BrowsePageRequest>,
  refresh_fallback: Option<LibraryBrowseView>,
}

impl Default for Surface {
  fn default() -> Self {
    Self {
      data: BrowseModel::default(),
      view: LibraryBrowseView::Inactive,
      artwork: ImageCollection::default(),
      page_tasks: HashMap::new(),
      viewport: BrowseViewport::default(),
      grid_viewport: None,
      scroll_id: iced::widget::Id::unique(),
      sort_menu_open: false,
      search_input: String::new(),
      filters: None,
      request_generation: 0,
      page_requests: HashMap::new(),
      refresh_fallback: None,
    }
  }
}

impl Surface {
  /// Before layout, bound materialization by the window rather than waiting for
  /// scroll input. Only the image observers may turn these cells into demand.
  pub(crate) fn grid_viewport(&self, window_size: iced::Size) -> ArtworkGridViewport {
    self.grid_viewport.unwrap_or_else(|| {
      ArtworkGridViewport::from_scroll_geometry(self.viewport.offset_y, window_size.height, 0.0)
    })
  }
}

/// Data and query context owned by one navigation-history entry.
pub(crate) struct Snapshot {
  data: BrowseModel,
  view: LibraryBrowseView,
  viewport: BrowseViewport,
  scroll_id: iced::widget::Id,
  filters: Option<BrowseFilterSettings>,
  search_input: String,
  requests: Vec<BrowsePageRequest>,
  refresh_fallback: Option<LibraryBrowseView>,
}

pub(crate) fn snapshot(surface: &mut Surface, submitted_query: Option<&str>) -> Snapshot {
  Snapshot {
    data: std::mem::take(&mut surface.data),
    view: std::mem::replace(&mut surface.view, LibraryBrowseView::Inactive),
    viewport: surface.viewport,
    scroll_id: surface.scroll_id.clone(),
    filters: surface.filters,
    search_input: submitted_query.unwrap_or(&surface.search_input).to_owned(),
    requests: surface.page_requests.values().cloned().collect(),
    refresh_fallback: surface.refresh_fallback.take(),
  }
}

pub(crate) fn restore(
  surface: &mut Surface,
  kernel: &mut Kernel,
  snapshot: Snapshot,
  window_size: iced::Size,
) -> Task<Message> {
  abort_pages(surface);
  begin_artwork_view(surface);
  surface.data = snapshot.data;
  surface.view = snapshot.view;
  surface.viewport = snapshot.viewport;
  surface.scroll_id = snapshot.scroll_id;
  surface.filters = snapshot.filters;
  surface.search_input = snapshot.search_input;
  surface.refresh_fallback = snapshot.refresh_fallback;
  surface.sort_menu_open = false;
  let requests = snapshot
    .requests
    .into_iter()
    .map(|request| start_page_request(surface, kernel, request))
    .collect::<Vec<_>>();
  Task::batch([
    Task::batch(requests),
    sync_scroll_window(surface, kernel, window_size),
    prepare_artwork(surface),
  ])
}

/// `source` is the router-resolved browse source for the current destination
/// (computed only for the filter-mutation messages, the only arms that
/// restart browsing), `in_library` whether the current destination is a
/// Library route (filter mutations apply nowhere else), and `window_size` the
/// shell's tracked window size. These are computed by the top-level router so
/// this module never reads navigation, home, playback, or shell state (ADR
/// 0029).
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
  in_library: bool,
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
      persist_filters(surface, kernel, source, in_library, |filters| {
        filters.with_sort(sort)
      })
    }
    BrowseMessage::SortDirectionToggled => {
      persist_filters(surface, kernel, source, in_library, |filters| {
        let direction = match filters.sort_direction() {
          VideoLibrarySortDirection::Ascending => VideoLibrarySortDirection::Descending,
          VideoLibrarySortDirection::Descending => VideoLibrarySortDirection::Ascending,
        };
        filters.with_sort_direction(direction)
      })
    }
    BrowseMessage::PlayedFilterChanged(played_filter) => {
      persist_filters(surface, kernel, source, in_library, |filters| {
        filters.with_played_filter(played_filter)
      })
    }
    BrowseMessage::FavoritesToggled => {
      persist_filters(surface, kernel, source, in_library, |filters| {
        filters.with_favorites_only(!filters.favorites_only())
      })
    }
    BrowseMessage::Scrolled(viewport) => {
      let offset = viewport.absolute_offset();
      surface.viewport = BrowseViewport { offset_y: offset.y };
      Task::none()
    }
    BrowseMessage::GridViewportMeasured {
      epoch,
      offset_y,
      height,
    } => {
      if epoch != surface.artwork.epoch() {
        return Task::none();
      }
      let measured = ArtworkGridViewport { offset_y, height };
      if surface.grid_viewport == Some(measured) {
        return Task::none();
      }
      surface.grid_viewport = Some(measured);
      sync_scroll_window(surface, kernel, window_size)
    }
    BrowseMessage::Retry => {
      let effects = match surface.data.retry() {
        Ok(effects) => effects,
        Err(error) => {
          kernel.diagnostics.record(
            DiagnosticLevel::Error,
            DiagnosticCategory::Connection,
            format!("Could not retry library browsing: {error}"),
          );
          kernel.notice = Some(
            UiText::new("browse-retry-failed").arg("details", sanitize_message(&error.to_string())),
          );
          return Task::none();
        }
      };
      sync_view(surface);
      apply_effects(surface, kernel, effects)
    }
    BrowseMessage::PageSettled(generation, settlement) => {
      if generation != surface.request_generation {
        return Task::none();
      }
      let current = surface.data.is_current_settlement(&settlement);
      if current {
        if let Err(error) = &settlement.result {
          kernel.diagnostics.record(
            DiagnosticLevel::Error,
            DiagnosticCategory::Connection,
            format!("Browse page load failed: {error}"),
          );
        }
        surface.page_tasks.remove(&settlement.token);
        surface.page_requests.remove(&settlement.token);
        if surface.refresh_fallback.is_some() {
          if let Err(error) = &settlement.result {
            kernel.diagnostics.record(
              DiagnosticLevel::Error,
              DiagnosticCategory::Connection,
              format!("Could not refresh this page: {error}"),
            );
            kernel.notice = Some(
              UiText::new("browse-refresh-failed")
                .arg("details", sanitize_message(&error.to_string())),
            );
          }
        }
      }
      let effects = match surface.data.settle(settlement) {
        Ok(effects) => effects,
        Err(error) => {
          kernel.diagnostics.record(
            DiagnosticLevel::Error,
            DiagnosticCategory::Connection,
            format!("Could not apply library results: {error}"),
          );
          kernel.notice = Some(
            UiText::new("browse-apply-failed").arg("details", sanitize_message(&error.to_string())),
          );
          return Task::none();
        }
      };
      sync_view(surface);
      Task::batch([
        apply_effects(surface, kernel, effects),
        sync_scroll_window(surface, kernel, window_size),
        prepare_artwork(surface),
      ])
    }
    BrowseMessage::ArtworkLoaded(completion) => {
      surface
        .artwork
        .settle(kernel.request_gate.current_session(), completion);
      Task::none()
    }
  }
}

fn persist_filters(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
  in_library: bool,
  mutation: impl FnOnce(BrowseFilterSettings) -> BrowseFilterSettings,
) -> Task<Message> {
  if !in_library {
    return Task::none();
  }
  let filters = mutation(
    surface
      .filters
      .unwrap_or_else(|| kernel.settings.snapshot().browse_filters()),
  );
  if let Err(error) = kernel.settings.set_browse_filters(filters) {
    kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Connection,
      format!("Could not save library filters: {error}"),
    );
    kernel.notice = Some(
      UiText::new("browse-save-filters-failed")
        .arg("details", sanitize_message(&error.to_string())),
    );
    return Task::none();
  }
  surface.filters = Some(filters);
  start(surface, kernel, source)
}

/// Starts (or reconfigures) the Library Browser for `source`. The top-level
/// router also calls this when navigating to a Library or Search
/// destination.
pub fn start(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
) -> Task<Message> {
  surface.refresh_fallback = None;
  let Some(source) = source else {
    abort_pages(surface);
    begin_artwork_view(surface);
    if let Err(error) = surface.data.reset() {
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Connection,
        format!("Could not reset library browsing: {error}"),
      );
      kernel.notice = Some(
        UiText::new("browse-reset-failed").arg("details", sanitize_message(&error.to_string())),
      );
      return Task::none();
    }
    sync_view(surface);
    kernel.notice = Some(UiText::new("browse-library-unavailable"));
    return Task::none();
  };
  let filters = *surface
    .filters
    .get_or_insert_with(|| kernel.settings.snapshot().browse_filters());
  let preferences = BrowsePreferences::from(filters);
  let effects = match surface.data.configure_with_preferences(source, preferences) {
    Ok(effects) => effects,
    Err(error) => {
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Connection,
        format!("Could not open library browsing: {error}"),
      );
      kernel.notice = Some(
        UiText::new("browse-open-failed").arg("details", sanitize_message(&error.to_string())),
      );
      sync_view(surface);
      return Task::none();
    }
  };
  // A reconfigure that changes nothing (same source and preferences, e.g.
  // re-selecting the active filter) emits no commands: the model retains its
  // pages, so no page settlement will re-drive artwork preparation. Tearing
  // the artwork view down here would strand a settled grid on placeholders,
  // but the view is still re-synced so an in-flight load keeps reflecting
  // Loading.
  if effects.is_empty() {
    sync_view(surface);
    return Task::none();
  }
  begin_artwork_view(surface);
  sync_view(surface);
  apply_effects(surface, kernel, effects)
}

/// Recomputes the scroll-driven display window and loads newly visible pages.
///
/// The model no-ops an unchanged range, so callers may invoke this freely
/// after scroll, resize, and page-settlement events.
/// `pub(crate)` because the top-level router also invokes this on window resize.
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
  let viewport = surface.grid_viewport(window_size);
  let range = visible_display_range(
    viewport.offset_y,
    viewport.height,
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
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Connection,
        format!("Could not load more library items: {error}"),
      );
      kernel.notice = Some(
        UiText::new("browse-load-more-failed").arg("details", sanitize_message(&error.to_string())),
      );
      return Task::none();
    }
  };
  sync_view(surface);
  Task::batch([
    apply_effects(surface, kernel, effects),
    prepare_artwork(surface),
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
) -> Task<Message> {
  let fallback = matches!(
    surface.view,
    LibraryBrowseView::Ready { .. } | LibraryBrowseView::Empty
  )
  .then(|| surface.view.clone());
  abort_pages(surface);
  if let Err(error) = surface.data.reset() {
    kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Connection,
      format!("Could not refresh this page: {error}"),
    );
    kernel.notice = Some(
      UiText::new("browse-refresh-failed").arg("details", sanitize_message(&error.to_string())),
    );
    return Task::none();
  }
  let task = start(surface, kernel, source);
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
        surface.grid_viewport = None;
        // Loading placeholders have no ready-grid ID. A new query identity
        // also resets remembered offsets when its first real layout appears.
        surface.scroll_id = iced::widget::Id::unique();
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
        surface.page_requests.remove(&token);
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
  let generation = surface.request_generation;
  surface.page_requests.insert(token, request.clone());
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    return Task::done(Message::Browse(BrowseMessage::PageSettled(
      generation,
      BrowsePageSettlement {
        source_id: request.source_id,
        token,
        result: Err("media-server-session-unavailable".to_owned()),
      },
    )));
  };
  let (task, handle) = Task::perform(fetch_browse_page(client, request), move |settlement| {
    Message::Browse(BrowseMessage::PageSettled(generation, settlement))
  })
  .abortable();
  surface.page_tasks.insert(token, handle);
  task
}

/// Retains candidates in the sparse metadata window; measured images start loads.
pub(crate) fn prepare_artwork(surface: &mut Surface) -> Task<Message> {
  let specs = match &surface.view {
    LibraryBrowseView::Ready { visible_items, .. } => visible_items
      .iter()
      .filter_map(|slot| slot.item.as_ref())
      .filter_map(|item| {
        Some(ImageSpec {
          key: item.id.clone(),
          image_id: item.artwork_image_id.clone()?,
          size_class: ArtworkSizeClass::Card,
          derived: DerivedArtwork::default(),
        })
      })
      .collect::<Vec<_>>(),
    _ => Vec::new(),
  };
  surface.artwork.retain(&specs);
  Task::none()
}

fn begin_artwork_view(surface: &mut Surface) {
  surface.artwork.clear();
  surface.grid_viewport = None;
}

/// Browse leave hook, invoked by the top-level router when the destination
/// switches away from Library/Search: aborts in-flight page requests,
/// releases this view's image demand, and resets the model.
pub(crate) fn leave_view(surface: &mut Surface, kernel: &mut Kernel) {
  abort_pages(surface);
  begin_artwork_view(surface);
  if let Err(error) = surface.data.reset() {
    kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Connection,
      format!("Could not reset library browsing: {error}"),
    );
    kernel.notice =
      Some(UiText::new("browse-reset-failed").arg("details", sanitize_message(&error.to_string())));
  }
  sync_view(surface);
}

/// Browse portion of the router's connected-surface reset: aborts in-flight
/// page requests, drops the artwork cells, and resets the model and view.
pub(crate) fn reset(surface: &mut Surface, kernel: &mut Kernel) {
  surface.refresh_fallback = None;
  surface.filters = None;
  abort_pages(surface);
  begin_artwork_view(surface);
  if let Err(error) = surface.data.reset() {
    kernel.diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Connection,
      format!("Could not reset library browsing: {error}"),
    );
    kernel.notice =
      Some(UiText::new("browse-reset-failed").arg("details", sanitize_message(&error.to_string())));
  }
  surface.view = LibraryBrowseView::Inactive;
}

fn abort_pages(surface: &mut Surface) {
  surface.request_generation = surface.request_generation.wrapping_add(1);
  surface.page_requests.clear();
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
  use jellypilot_media_server::VideoLibraryItem;

  use super::*;

  /// Matches the 1600px window width of the old update.rs `test_state`.
  const WINDOW_WIDTH: f32 = 1600.0;
  /// Matches the 900px window height of the old update.rs `test_state`.
  const WINDOW_HEIGHT: f32 = 900.0;

  fn window_size() -> iced::Size {
    iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT)
  }

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

  #[test]
  fn resumed_refresh_rejects_departed_results_and_retains_cards_on_failure() {
    use jellypilot_core::browse_model::BrowsePagePayload;
    let (mut surface, mut kernel) = test_fixture();
    let source = search_source(&kernel, "original");
    let first = browse_request(surface.data.configure(source.clone()).unwrap());
    surface
      .data
      .settle(BrowsePageSettlement {
        source_id: first.source_id,
        token: first.token,
        result: Ok(BrowsePagePayload {
          start_index: 0,
          limit: 24,
          total_record_count: 1,
          has_more: false,
          items: vec![episode("cached", 1)],
        }),
      })
      .unwrap();
    sync_view(&mut surface);
    drop(refresh(&mut surface, &mut kernel, Some(source)));
    let request = surface.page_requests.values().next().unwrap().clone();
    let departed = surface.request_generation;
    let saved = snapshot(&mut surface, Some("original"));
    leave_view(&mut surface, &mut kernel);
    drop(restore(&mut surface, &mut kernel, saved, window_size()));
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::PageSettled(
        departed,
        BrowsePageSettlement {
          source_id: request.source_id.clone(),
          token: request.token,
          result: Ok(BrowsePagePayload {
            start_index: 0,
            limit: 24,
            total_record_count: 1,
            has_more: false,
            items: vec![episode("stale", 1)],
          }),
        },
      ),
    ));
    let current = surface.request_generation;
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::PageSettled(
        current,
        BrowsePageSettlement {
          source_id: request.source_id,
          token: request.token,
          result: Err("refresh unavailable".to_owned()),
        },
      ),
    ));
    let LibraryBrowseView::Ready { visible_items, .. } = &surface.view else {
      panic!("returning during a failed refresh must preserve usable cards");
    };
    assert_eq!(visible_items[0].item.as_ref().unwrap().id, "cached");
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

    drop(start(&mut surface, &mut kernel, Some(source)));

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
      window_size(),
      BrowseMessage::PageSettled(
        0,
        BrowsePageSettlement {
          source_id: stale.source_id,
          token: stale.token,
          result: Err("stale server response".to_owned()),
        },
      ),
    ));

    assert!(surface.page_tasks.contains_key(&current.token));
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
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
      window_size(),
      BrowseMessage::PageSettled(0, settlement),
    ));

    let metrics = ArtworkGridMetrics::for_cards(
      grid_available_width(WINDOW_WIDTH, SizeClass::from_width(WINDOW_WIDTH)),
      CARD_COPY_HEIGHT,
    );
    let initial = surface.data.display_range().expect("metadata window");
    assert_eq!(initial.start, 0);
    assert!(
      (initial.end as usize / metrics.columns) as f32 * metrics.row_height >= 2.0 * WINDOW_HEIGHT
    );
    assert!(initial.end < 264, "scroll projection must remain sparse");

    let epoch = surface.artwork.epoch();
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        offset_y: -120.0,
        height: WINDOW_HEIGHT,
      },
    ));
    let measured_initial = surface
      .data
      .display_range()
      .expect("measured metadata window");
    assert_eq!(
      measured_initial,
      visible_display_range(
        -120.0,
        WINDOW_HEIGHT,
        metrics.columns,
        metrics.row_height,
        264
      )
    );

    // Scrolling ten rows down shifts the window without resetting it.
    surface.viewport.offset_y = 2870.0;
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        offset_y: 2750.0,
        height: WINDOW_HEIGHT,
      },
    ));
    let scrolled = surface
      .data
      .display_range()
      .expect("scrolled metadata window");
    let first_row = scrolled.start as usize / metrics.columns;
    let end_row = scrolled.end as usize / metrics.columns;
    assert!(first_row as f32 * metrics.row_height <= 2750.0 - WINDOW_HEIGHT);
    assert!(end_row as f32 * metrics.row_height >= 2750.0 + 2.0 * WINDOW_HEIGHT);
    assert!(first_row as f32 * metrics.row_height + metrics.row_height > 2750.0 - WINDOW_HEIGHT);
    assert!(
      end_row as f32 * metrics.row_height - metrics.row_height < 2750.0 + 2.0 * WINDOW_HEIGHT
    );
    assert_eq!(surface.viewport.offset_y, 2870.0);
    assert!(scrolled.start > 0 && scrolled.end < 264);

    // An unchanged viewport keeps the window and emits no page requests.
    let pending_before = surface.page_tasks.len();
    drop(sync_scroll_window(&mut surface, &mut kernel, window_size()));
    assert_eq!(surface.data.display_range(), Some(scrolled));
    assert_eq!(surface.page_tasks.len(), pending_before);

    // Scrolling back up restores the earlier window.
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        offset_y: -120.0,
        height: WINDOW_HEIGHT,
      },
    ));
    assert_eq!(surface.data.display_range(), Some(measured_initial));
  }

  #[test]
  fn preparing_metadata_does_not_admit_unobserved_images() {
    let (mut surface, _) = test_fixture();
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

    drop(prepare_artwork(&mut surface));

    assert!(surface.artwork.is_empty());
  }

  #[test]
  fn initial_short_grid_materializes_cards_without_scroll_input_or_image_demand() {
    let (surface, _) = test_fixture();
    let metrics = ArtworkGridMetrics::for_cards(
      grid_available_width(WINDOW_WIDTH, SizeClass::from_width(WINDOW_WIDTH)),
      CARD_COPY_HEIGHT,
    );
    let built = std::cell::RefCell::new(Vec::new());
    let _grid: iced::Element<'_, Message> = jellypilot_ui::widgets::artwork_grid::artwork_grid(
      3,
      metrics,
      surface.grid_viewport(window_size()),
      |index| {
        built.borrow_mut().push(index);
        iced::widget::Space::new().into()
      },
    );
    assert_eq!(*built.borrow(), vec![0, 1, 2]);
    assert!(surface.artwork.is_empty());
  }

  #[test]
  fn recreated_grid_ignores_previous_epoch_geometry() {
    let (mut surface, mut kernel) = test_fixture();
    let epoch = surface.artwork.epoch();
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 400.0,
      height: 300.0,
    });
    leave_view(&mut surface, &mut kernel);
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        offset_y: 400.0,
        height: 300.0,
      },
    ));
    assert_eq!(surface.grid_viewport, None);
  }
}
