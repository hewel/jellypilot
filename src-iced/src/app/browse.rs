//! Browse surface (ADR 0029): Library Browser paging, filter/sort
//! preferences, the scroll-driven display window, the sidebar search input,
//! and the browse artwork pipeline (grid cards).

use crate::i18n::UiText;
use iced::widget::operation;
use iced::Task;
use jellypilot_core::browse_model::{BrowsePreferences, BrowseSource, LibraryBrowseView};
use jellypilot_core::browse_window::visible_display_range;
use jellypilot_core::config::BrowseFilterSettings;
use jellypilot_core::diagnostics::{sanitize_message, DiagnosticCategory, DiagnosticLevel};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::VideoLibrarySortDirection;
use jellypilot_sdk::browse::{BrowseWork, Browser};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::widgets::artwork_grid::ArtworkGridViewport;

use super::artwork::{ImageCollection, ImageSpec};
use super::kernel::Kernel;
use super::message::{BrowseMessage, Message};
use super::state::BrowseViewport;
use super::view::browse::{browse_metrics, grid_available_width};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewMode {
  #[default]
  Grid,
  List,
}

struct ScrollSnapshot {
  viewport: BrowseViewport,
  grid_viewport: Option<ArtworkGridViewport>,
  id: iced::widget::Id,
}

/// Browse surface slice: the SDK-owned Library Browser and its derived view,
/// the artwork cells bound for the grid cards, the tracked scroll viewport,
/// and the sidebar search input text.
pub struct Surface {
  pub browser: Browser,
  pub view: LibraryBrowseView,
  pub artwork: ImageCollection,
  pub viewport: BrowseViewport,
  pub grid_viewport: Option<ArtworkGridViewport>,
  pub scroll_id: iced::widget::Id,
  pub sort_menu_open: bool,
  pub search_input: String,
  pub filters: Option<BrowseFilterSettings>,
  pub mode: ViewMode,
  alternate_scroll: Option<ScrollSnapshot>,
}

impl Default for Surface {
  fn default() -> Self {
    Self {
      browser: Browser::default(),
      view: LibraryBrowseView::Inactive,
      artwork: ImageCollection::default(),
      viewport: BrowseViewport::default(),
      grid_viewport: None,
      scroll_id: iced::widget::Id::unique(),
      sort_menu_open: false,
      search_input: String::new(),
      filters: None,
      mode: ViewMode::Grid,
      alternate_scroll: None,
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

/// Browser and query context owned by one navigation-history entry.
pub(crate) struct Snapshot {
  browser: Browser,
  viewport: BrowseViewport,
  grid_viewport: Option<ArtworkGridViewport>,
  scroll_id: iced::widget::Id,
  filters: Option<BrowseFilterSettings>,
  search_input: String,
  mode: ViewMode,
  alternate_scroll: Option<ScrollSnapshot>,
}

pub(crate) fn snapshot(surface: &mut Surface, submitted_query: Option<&str>) -> Snapshot {
  // Suspending cancels the in-flight page requests; their late settlements
  // arrive rejected and are dropped by the currency check.
  surface.browser.suspend();
  let browser = std::mem::take(&mut surface.browser);
  sync_view(surface);
  Snapshot {
    browser,
    viewport: surface.viewport,
    grid_viewport: surface.grid_viewport,
    scroll_id: surface.scroll_id.clone(),
    filters: surface.filters,
    search_input: submitted_query.unwrap_or(&surface.search_input).to_owned(),
    mode: surface.mode,
    alternate_scroll: surface.alternate_scroll.take(),
  }
}

pub(crate) fn restore(
  surface: &mut Surface,
  kernel: &mut Kernel,
  snapshot: Snapshot,
  window_size: iced::Size,
) -> Task<Message> {
  begin_artwork_view(surface);
  // Replacing the browser drops the previous owner, cancelling its requests.
  surface.browser = snapshot.browser;
  surface.viewport = snapshot.viewport;
  surface.grid_viewport = snapshot.grid_viewport;
  surface.scroll_id = snapshot.scroll_id;
  surface.filters = snapshot.filters;
  surface.search_input = snapshot.search_input;
  surface.mode = snapshot.mode;
  surface.alternate_scroll = snapshot.alternate_scroll;
  surface.sort_menu_open = false;
  let work = match surface.browser.resume() {
    Ok(work) => work,
    Err(error) => {
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Connection,
        format!("Could not restore library browsing: {error}"),
      );
      kernel.notice = Some(
        UiText::new("browse-open-failed").arg("details", sanitize_message(&error.to_string())),
      );
      sync_view(surface);
      return prepare_artwork(surface);
    }
  };
  sync_view(surface);
  Task::batch([
    apply_work(surface, work),
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
    BrowseMessage::ViewModeSelected(mode) => {
      if surface.mode == mode {
        return Task::none();
      }
      let next = surface
        .alternate_scroll
        .take()
        .unwrap_or_else(|| ScrollSnapshot {
          viewport: BrowseViewport::default(),
          grid_viewport: None,
          id: iced::widget::Id::unique(),
        });
      surface.alternate_scroll = Some(ScrollSnapshot {
        viewport: surface.viewport,
        grid_viewport: surface.grid_viewport,
        id: surface.scroll_id.clone(),
      });
      surface.mode = mode;
      surface.viewport = next.viewport;
      surface.grid_viewport = next.grid_viewport;
      surface.scroll_id = next.id;
      surface.sort_menu_open = false;
      // A fresh observer epoch rejects geometry and image visibility events
      // emitted by the departed presentation, without discarding metadata.
      surface.artwork.clear();
      Task::batch([
        sync_scroll_window(surface, kernel, window_size),
        prepare_artwork(surface),
      ])
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
      surface.sort_menu_open = false;
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
      let work = match surface.browser.retry() {
        Ok(work) => work,
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
      apply_work(surface, work)
    }
    BrowseMessage::PageSettled(settlement) => {
      if !surface.browser.model().is_current_settlement(&settlement) {
        return Task::none();
      }
      if let Err(error) = &settlement.result {
        kernel.diagnostics.record(
          DiagnosticLevel::Error,
          DiagnosticCategory::Connection,
          format!("Browse page load failed: {error}"),
        );
        if surface.browser.model().is_refreshing() {
          kernel.notice = Some(
            UiText::new("browse-refresh-failed")
              .arg("details", sanitize_message(&error.to_string())),
          );
        }
      }
      let work = match surface.browser.settle(settlement) {
        Ok(work) => work,
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
        apply_work(surface, work),
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
  let Some(source) = source else {
    begin_artwork_view(surface);
    surface.browser.reset();
    sync_view(surface);
    kernel.notice = Some(UiText::new("browse-library-unavailable"));
    return Task::none();
  };
  let filters = *surface
    .filters
    .get_or_insert_with(|| kernel.settings.snapshot().browse_filters());
  let preferences = BrowsePreferences::from(filters);
  let work = match surface
    .browser
    .configure(kernel.client.clone(), source, preferences)
  {
    Ok(work) => work,
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
  // re-selecting the active filter) emits no work: the model retains its
  // pages, so no page settlement will re-drive artwork preparation. Tearing
  // the artwork view down here would strand a settled grid on placeholders,
  // but the view is still re-synced so an in-flight load keeps reflecting
  // Loading.
  if work.is_empty() {
    sync_view(surface);
    return Task::none();
  }
  begin_artwork_view(surface);
  sync_view(surface);
  apply_work(surface, work)
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
  let metrics = browse_metrics(grid_available_width(window_size.width, class), surface.mode);
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
  if surface.browser.model().peek_display_range().as_ref() == Some(&range) {
    return Task::none();
  }
  let work = match surface.browser.set_display_range(range) {
    Ok(work) => work,
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
  Task::batch([apply_work(surface, work), prepare_artwork(surface)])
}

fn sync_view(surface: &mut Surface) {
  surface.view = surface.browser.model().view();
}

/// Refreshes the model's stored query without releasing usable image demand.
pub(crate) fn refresh(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let work = match surface.browser.refresh() {
    Ok(work) => work,
    Err(error) => {
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
  };
  sync_view(surface);
  Task::batch([apply_work(surface, work), prepare_artwork(surface)])
}

pub(crate) fn apply_user_data_update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  update: &jellypilot_media_server::VideoUserDataUpdate,
) -> Task<Message> {
  let work = match surface.browser.apply_user_data_update(update) {
    Ok(work) => work,
    Err(error) => {
      kernel.diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Connection,
        format!("Could not refresh confirmed library changes: {error}"),
      );
      kernel.notice = Some(
        UiText::new("browse-refresh-failed").arg("details", sanitize_message(&error.to_string())),
      );
      sync_view(surface);
      return Task::none();
    }
  };
  sync_view(surface);
  Task::batch([apply_work(surface, work), prepare_artwork(surface)])
}

fn apply_work(surface: &mut Surface, work: BrowseWork) -> Task<Message> {
  // Viewport resets must land before page requests: Task::batch runs in
  // parallel, so a fast settlement could evaluate the stale near-tail offset
  // and advance another window before scroll-to-zero is applied.
  let mut reset = Task::none();
  if work.reset_viewport {
    surface.viewport.offset_y = 0.0;
    surface.grid_viewport = None;
    surface.alternate_scroll = None;
    // Loading placeholders have no ready-grid ID. A new query identity
    // also resets remembered offsets when its first real layout appears.
    surface.scroll_id = iced::widget::Id::unique();
    reset = operation::scroll_to(
      surface.scroll_id.clone(),
      operation::AbsoluteOffset { x: 0.0, y: 0.0 },
    );
  }
  // The SDK owns request cancellation; the futures only deliver settlements.
  // `run` is a lazy async fn, so requests start when the chained task polls
  // them — after the viewport reset has been emitted.
  let requests = Task::batch(work.requests.into_iter().map(|request| {
    Task::perform(request.run(), |settlement| {
      Message::Browse(BrowseMessage::PageSettled(settlement))
    })
  }));
  reset.chain(requests)
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
/// switches away from Library/Search: cancels in-flight page requests,
/// releases this view's image demand, and resets the browser.
pub(crate) fn leave_view(surface: &mut Surface) {
  begin_artwork_view(surface);
  surface.browser.reset();
  sync_view(surface);
  surface.alternate_scroll = None;
}

/// Browse portion of the router's connected-surface reset: cancels in-flight
/// page requests, drops the artwork cells, and resets the browser and view.
pub(crate) fn reset(surface: &mut Surface) {
  leave_view(surface);
  surface.filters = None;
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::VideoLibraryItem;

  use crate::app::test_support::{
    dispatch_settlement, drain_task, fixture_item_id, page_settled, BrowseFixture, FixtureReply,
  };

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
      undo: crate::app::undo::Runtime::default(),
      next_toast_id: 0,
      tray: None,
      artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      profile_avatars: Default::default(),
    };
    (Surface::default(), kernel)
  }

  /// A surface whose kernel client is bound to a controlled local server.
  fn fixture_surface() -> (Surface, Kernel, BrowseFixture) {
    let (surface, mut kernel) = test_fixture();
    let fixture = BrowseFixture::new();
    kernel.client = Some(fixture.client());
    (surface, kernel, fixture)
  }

  fn search_source(kernel: &Kernel, query: &str) -> BrowseSource {
    BrowseSource::Search {
      session: kernel.request_gate.current_session(),
      query: query.to_owned(),
    }
  }

  #[tokio::test]
  async fn view_modes_retain_the_query_and_restore_independent_navigation_scrolls() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "retained query");
    let task = start(&mut surface, &mut kernel, Some(source));
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 240,
        artwork: false,
      },
    )
    .await;
    surface.search_input = "retained query".to_owned();
    surface.filters = Some(kernel.settings.snapshot().browse_filters());
    surface.viewport.offset_y = 600.0;
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 600.0,
      height: 500.0,
    });
    let grid_id = surface.scroll_id.clone();
    let filters = surface.filters;
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::ViewModeSelected(ViewMode::List),
    ));
    let list_id = surface.scroll_id.clone();
    assert_ne!(list_id, grid_id);
    let epoch = surface.artwork.epoch();
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        offset_y: 810.0,
        height: 405.0,
      },
    ));
    let expected = visible_display_range(810.0, 405.0, 1, 81.0, 240);
    assert_eq!(surface.browser.model().peek_display_range(), Some(expected));
    surface.viewport.offset_y = 810.0;
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::ViewModeSelected(ViewMode::Grid),
    ));
    assert_eq!(surface.viewport.offset_y, 600.0);
    assert_eq!(surface.scroll_id, grid_id);
    assert_eq!(surface.filters, filters);
    assert_eq!(surface.search_input, "retained query");
    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::ViewModeSelected(ViewMode::List),
    ));
    let saved = snapshot(&mut surface, Some("retained query"));
    leave_view(&mut surface);
    drop(restore(&mut surface, &mut kernel, saved, window_size()));
    assert_eq!(surface.mode, ViewMode::List);
    assert_eq!(surface.viewport.offset_y, 810.0);
    assert_eq!(surface.scroll_id, list_id);
    assert_eq!(surface.search_input, "retained query");
  }

  #[tokio::test]
  async fn resumed_refresh_rejects_departed_results_and_retains_cards_on_failure() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "original");
    let task = start(&mut surface, &mut kernel, Some(source));
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 240,
        artwork: false,
      },
    )
    .await;
    // Load the deep window the failed refresh must retain.
    let mut pending = surface
      .browser
      .set_display_range(192..216)
      .expect("display range applies")
      .requests;
    while let Some(request) = pending.pop() {
      let settlement = fixture
        .run_request(
          request,
          FixtureReply::Page {
            total: 240,
            artwork: false,
          },
        )
        .await;
      pending.extend(
        surface
          .browser
          .settle(settlement)
          .expect("deep page settles")
          .requests,
      );
    }
    sync_view(&mut surface);
    surface.viewport.offset_y = 8_000.0;
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 8_000.0,
      height: WINDOW_HEIGHT,
    });
    let scroll_id = surface.scroll_id.clone();

    let mut refresh_stream = iced_runtime::task::into_stream(refresh(&mut surface, &mut kernel))
      .expect("refresh emits page work");
    let departed_request = fixture.next_request(&mut refresh_stream).await;
    let saved = snapshot(&mut surface, Some("original"));
    departed_request.reply(FixtureReply::Failure);
    let departed = fixture
      .run_stream(&mut refresh_stream, |_| FixtureReply::Failure)
      .await
      .into_iter()
      .find_map(page_settled)
      .expect("cancelled refresh still settles");
    assert!(!saved.browser.model().is_current_settlement(&departed));

    let mut restore_stream =
      iced_runtime::task::into_stream(restore(&mut surface, &mut kernel, saved, window_size()))
        .expect("restore resumes page work");
    let settlements = fixture
      .run_stream(&mut restore_stream, |_| FixtureReply::Failure)
      .await
      .into_iter()
      .filter_map(page_settled)
      .collect::<Vec<_>>();
    // The departed settlement is rejected while the resumed refresh is still
    // in flight; the current failures then complete it.
    drop(dispatch_settlement(
      &mut surface,
      &mut kernel,
      window_size(),
      departed,
    ));
    assert!(surface.browser.model().is_refreshing());
    for settlement in settlements {
      let task = dispatch_settlement(&mut surface, &mut kernel, window_size(), settlement);
      drain_task(
        &mut surface,
        &mut kernel,
        &mut fixture,
        window_size(),
        task,
        &mut |_| FixtureReply::Failure,
      )
      .await;
    }
    assert!(!surface.browser.model().is_refreshing());
    assert!(surface.browser.model().refresh_failure().is_some());
    assert_eq!(surface.viewport.offset_y, 8_000.0);
    assert_eq!(surface.scroll_id, scroll_id);
    surface
      .browser
      .set_display_range(192..216)
      .expect("display range applies");
    sync_view(&mut surface);
    assert_eq!(
      surface.grid_viewport,
      Some(ArtworkGridViewport {
        offset_y: 8_000.0,
        height: WINDOW_HEIGHT,
      })
    );
    let LibraryBrowseView::Ready { visible_items, .. } = &surface.view else {
      panic!("failed refresh must preserve deep cached pages");
    };
    assert_eq!(
      visible_items[0].item.as_ref().unwrap().id,
      fixture_item_id(192)
    );
    let retry = update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::Retry,
    );
    assert!(surface.browser.model().is_refreshing());
    let retry = fixture
      .run_task(retry, |_| FixtureReply::Failure)
      .await
      .into_iter()
      .find_map(page_settled)
      .expect("retry emits a settlement");
    assert!(surface.browser.model().is_current_settlement(&retry));
  }

  #[tokio::test]
  async fn empty_refresh_failure_survives_history_and_retries_without_resetting_scroll() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "empty");
    let task = start(&mut surface, &mut kernel, Some(source));
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 0,
        artwork: false,
      },
    )
    .await;
    let failure = fixture
      .run_task(refresh(&mut surface, &mut kernel), |_| {
        FixtureReply::Failure
      })
      .await
      .into_iter()
      .find_map(page_settled)
      .expect("refresh emits a settlement");
    drop(dispatch_settlement(
      &mut surface,
      &mut kernel,
      window_size(),
      failure,
    ));
    let saved = snapshot(&mut surface, Some("empty"));
    drop(restore(&mut surface, &mut kernel, saved, window_size()));
    assert!(matches!(surface.view, LibraryBrowseView::Empty));
    assert!(surface.browser.model().refresh_failure().is_some());
    let scroll_id = surface.scroll_id.clone();
    let retry = update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::Retry,
    );
    assert!(surface.browser.model().is_refreshing());
    assert_eq!(surface.scroll_id, scroll_id);
    for message in fixture.run_task(retry, |_| FixtureReply::Failure).await {
      if let Some(settlement) = page_settled(message) {
        drop(dispatch_settlement(
          &mut surface,
          &mut kernel,
          window_size(),
          settlement,
        ));
      }
    }
    assert!(matches!(surface.view, LibraryBrowseView::Empty));
    assert!(surface.browser.model().refresh_failure().is_some());
  }

  #[tokio::test]
  async fn snapshot_cancels_owned_delivery_without_a_separate_leave_hook() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "pending");
    let mut stream =
      iced_runtime::task::into_stream(start(&mut surface, &mut kernel, Some(source)))
        .expect("start emits page work");
    let request = fixture.next_request(&mut stream).await;
    let mut saved = snapshot(&mut surface, Some("pending"));
    request.reply(FixtureReply::Failure);
    let settlement = fixture
      .run_stream(&mut stream, |_| FixtureReply::Failure)
      .await
      .into_iter()
      .find_map(page_settled)
      .expect("cancelled delivery still settles");
    assert!(!saved.browser.model().is_current_settlement(&settlement));
    let work = saved.browser.resume().expect("resume reissues work");
    let request = work.requests.into_iter().next().expect("resumed request");
    let resumed = fixture.run_request(request, FixtureReply::Failure).await;
    assert!(saved.browser.model().is_current_settlement(&resumed));
  }

  #[tokio::test]
  async fn refresh_start_and_failure_preserve_overlapping_image_demand() {
    use jellypilot_core::image_lifecycle::ImagePriority;
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "images");
    let task = start(&mut surface, &mut kernel, Some(source));
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 1,
        artwork: true,
      },
    )
    .await;
    let LibraryBrowseView::Ready { visible_items, .. } = &surface.view else {
      panic!("fixture page must ready the view");
    };
    let item = visible_items[0]
      .item
      .clone()
      .expect("fixture item is present");
    drop(surface.artwork.observe(
      kernel.request_gate.current_session(),
      ImageSpec {
        key: item.id.clone(),
        image_id: item.artwork_image_id.clone().expect("fixture artwork"),
        size_class: ArtworkSizeClass::Card,
        derived: DerivedArtwork::default(),
      },
      Some(ImagePriority::Visible),
      fixture.client(),
      Arc::clone(&kernel.artwork_adapter),
      |completion| Message::Browse(BrowseMessage::ArtworkLoaded(completion)),
    ));
    let epoch = surface.artwork.epoch();
    let task = refresh(&mut surface, &mut kernel);
    assert!(surface.artwork.get(&item.id).is_some());
    assert_eq!(surface.artwork.epoch(), epoch);
    for message in fixture.run_task(task, |_| FixtureReply::Failure).await {
      if let Some(settlement) = page_settled(message) {
        drop(dispatch_settlement(
          &mut surface,
          &mut kernel,
          window_size(),
          settlement,
        ));
      }
    }
    assert!(surface.artwork.get(&item.id).is_some());
    assert_eq!(surface.artwork.epoch(), epoch);
  }

  #[tokio::test]
  async fn identical_browse_resubmit_starts_no_new_page_request() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "arrival");
    let mut stream =
      iced_runtime::task::into_stream(start(&mut surface, &mut kernel, Some(source.clone())))
        .expect("start emits page work");
    let request = fixture.next_request(&mut stream).await;

    let resubmit = start(&mut surface, &mut kernel, Some(source));

    assert!(iced_runtime::task::into_stream(resubmit).is_none());
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
    drop(request);
  }

  #[tokio::test]
  async fn stale_settlement_is_rejected_without_touching_the_reopened_query() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "arrival");
    let mut first_stream =
      iced_runtime::task::into_stream(start(&mut surface, &mut kernel, Some(source.clone())))
        .expect("first start emits page work");
    let first_request = fixture.next_request(&mut first_stream).await;
    leave_view(&mut surface);
    first_request.reply(FixtureReply::Failure);
    let stale = fixture
      .run_stream(&mut first_stream, |_| FixtureReply::Failure)
      .await
      .into_iter()
      .find_map(page_settled)
      .expect("cancelled request still settles");

    let mut reopened_stream =
      iced_runtime::task::into_stream(start(&mut surface, &mut kernel, Some(source)))
        .expect("reopen emits page work");
    let reopened_request = fixture.next_request(&mut reopened_stream).await;

    drop(update(
      &mut surface,
      &mut kernel,
      None,
      false,
      window_size(),
      BrowseMessage::PageSettled(stale),
    ));

    assert!(matches!(surface.view, LibraryBrowseView::Loading));
    reopened_request.reply(FixtureReply::Failure);
    let reopened = fixture
      .run_stream(&mut reopened_stream, |_| FixtureReply::Failure)
      .await
      .into_iter()
      .find_map(page_settled)
      .expect("reopened request settles");
    assert!(surface.browser.model().is_current_settlement(&reopened));
  }

  #[tokio::test]
  async fn reset_viewport_work_clears_scroll_before_page_requests_start() {
    use iced::futures::StreamExt;
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    surface.viewport.offset_y = 640.0;
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 640.0,
      height: 300.0,
    });
    let source = search_source(&kernel, "reset");
    let mut stream =
      iced_runtime::task::into_stream(start(&mut surface, &mut kernel, Some(source)))
        .expect("configure emits page work");

    let first = stream.next().await.expect("scroll reset is emitted first");
    assert!(matches!(first, iced_runtime::Action::Widget(_)));
    assert_eq!(surface.viewport.offset_y, 0.0);
    assert_eq!(surface.grid_viewport, None);
    // The page request reaches the server only after the reset action.
    let request = fixture.next_request(&mut stream).await;
    assert!(request.target().to_lowercase().contains("startindex=0"));
    request.reply(FixtureReply::Page {
      total: 0,
      artwork: false,
    });
    drop(
      fixture
        .run_stream(&mut stream, |_| FixtureReply::Failure)
        .await,
    );
  }

  #[tokio::test]
  async fn browse_scroll_position_drives_the_display_window() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
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
    let task = start(&mut surface, &mut kernel, Some(library));
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 264,
        artwork: false,
      },
    )
    .await;

    let metrics = browse_metrics(
      grid_available_width(WINDOW_WIDTH, SizeClass::from_width(WINDOW_WIDTH)),
      surface.mode,
    );
    let initial = surface
      .browser
      .model()
      .display_range()
      .expect("metadata window");
    assert_eq!(initial.start, 0);
    assert!(
      (initial.end as usize / metrics.columns) as f32 * metrics.row_height >= 2.0 * WINDOW_HEIGHT
    );
    assert!(initial.end < 264, "scroll projection must remain sparse");

    let epoch = surface.artwork.epoch();
    let task = update(
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
    );
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 264,
        artwork: false,
      },
    )
    .await;
    let measured_initial = surface
      .browser
      .model()
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
    let task = update(
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
    );
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 264,
        artwork: false,
      },
    )
    .await;
    let scrolled = surface
      .browser
      .model()
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
    let outputs = fixture
      .run_task(
        sync_scroll_window(&mut surface, &mut kernel, window_size()),
        |_| panic!("unchanged window must not request pages"),
      )
      .await;
    assert!(outputs.is_empty());
    assert_eq!(surface.browser.model().display_range(), Some(scrolled));

    // Scrolling back up restores the earlier window.
    let task = update(
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
    );
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Page {
        total: 264,
        artwork: false,
      },
    )
    .await;
    assert_eq!(
      surface.browser.model().display_range(),
      Some(measured_initial)
    );
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
    let metrics = browse_metrics(
      grid_available_width(WINDOW_WIDTH, SizeClass::from_width(WINDOW_WIDTH)),
      surface.mode,
    );
    let built = std::cell::RefCell::new(Vec::new());
    let _grid: iced::Element<'_, Message> = jellypilot_ui::widgets::artwork_grid::artwork_grid(
      3,
      metrics,
      surface.grid_viewport(window_size()),
      super::super::view::browse::GRID_COLUMN_GAP,
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
    leave_view(&mut surface);
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

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      premiere_date: None,
      community_rating: None,
      episode_count: None,
      last_played_date: None,
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
}
