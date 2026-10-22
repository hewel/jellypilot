//! Browse presentation: query ownership and execution live in the SDK.

use std::sync::Arc;

use crate::i18n::UiText;
use iced::widget::operation;
use iced::Task;
use jellypilot_core::browse_model::{BrowsePreferences, BrowseSource, LibraryBrowseView};
use jellypilot_core::browse_window::visible_display_range;
use jellypilot_core::config::BrowseFilterSettings;
use jellypilot_core::diagnostics::{sanitize_message, DiagnosticCategory, DiagnosticLevel};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::{VideoLibraryFilters, VideoLibrarySortDirection};
use jellypilot_sdk::browse::{
  AppliedSavedBrowse, BrowsePresentation, BrowseQuery, BrowseSession, BrowseSnapshot,
};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::widgets::artwork_grid::{ArtworkGridMetrics, ArtworkGridViewport};

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

pub struct Surface {
  pub session: Option<Arc<BrowseSession>>,
  /// Read-only query metadata projected from SDK notifications.
  pub projection: Option<BrowseProjection>,
  pub view: LibraryBrowseView,
  pub artwork: ImageCollection,
  pub viewport: BrowseViewport,
  pub grid_viewport: Option<ArtworkGridViewport>,
  pub scroll_id: iced::widget::Id,
  pub sort_menu_open: bool,
  pub search_input: String,
  pub mode: ViewMode,
  pub presentation: BrowsePresentation,
  pub presentation_grid: Option<ArtworkGridMetrics>,
  alternate_scroll: Option<ScrollSnapshot>,
}

/// Cached SDK metadata; the item window is moved into `Surface::view`.
pub struct BrowseProjection {
  pub revision: u64,
  pub identity: String,
  pub source: BrowseSource,
  pub preferences: BrowsePreferences,
  pub saved: Option<AppliedSavedBrowse>,
  pub viewport_revision: u64,
  pub refreshing: bool,
  pub refresh_failure: Option<jellypilot_core::LibraryBrowseFailure>,
}

fn install_projection(surface: &mut Surface, snapshot: BrowseSnapshot) {
  surface.view = snapshot.view;
  surface.projection = Some(BrowseProjection {
    revision: snapshot.revision,
    identity: snapshot.identity,
    source: snapshot.source,
    preferences: snapshot.preferences,
    saved: snapshot.saved,
    viewport_revision: snapshot.viewport_revision,
    refreshing: snapshot.refreshing,
    refresh_failure: snapshot.refresh_failure,
  });
}

impl Default for Surface {
  fn default() -> Self {
    Self {
      session: None,
      projection: None,
      view: LibraryBrowseView::Inactive,
      artwork: ImageCollection::default(),
      viewport: BrowseViewport::default(),
      grid_viewport: None,
      scroll_id: iced::widget::Id::unique(),
      sort_menu_open: false,
      search_input: String::new(),
      mode: ViewMode::Grid,
      presentation: BrowsePresentation::Desktop,
      presentation_grid: None,
      alternate_scroll: None,
    }
  }
}

impl Surface {
  pub(crate) fn grid_viewport(&self, window_size: iced::Size) -> ArtworkGridViewport {
    self.grid_viewport.unwrap_or_else(|| {
      ArtworkGridViewport::from_scroll_geometry(self.viewport.offset_y, window_size.height, 0.0)
    })
  }

  pub(crate) fn preferences(&self) -> BrowsePreferences {
    self
      .projection
      .as_ref()
      .map(|snapshot| snapshot.preferences.clone())
      .unwrap_or_default()
  }

  pub(crate) fn filter_settings(&self) -> BrowseFilterSettings {
    self
      .projection
      .as_ref()
      .map_or_else(BrowseFilterSettings::default, |projection| {
        let preferences = &projection.preferences;
        BrowseFilterSettings::default()
          .with_sort(preferences.sort)
          .with_sort_direction(preferences.sort_direction)
          .with_played_filter(preferences.played_filter)
          .with_favorites_only(preferences.favorites_only)
      })
  }

  pub(crate) fn saved(&self) -> Option<&AppliedSavedBrowse> {
    self.projection.as_ref()?.saved.as_ref()
  }

  pub(crate) fn identity(&self) -> Option<&str> {
    self
      .projection
      .as_ref()
      .map(|snapshot| snapshot.identity.as_str())
  }

  pub(crate) fn refreshing(&self) -> bool {
    self
      .projection
      .as_ref()
      .is_some_and(|snapshot| snapshot.refreshing)
  }

  pub(crate) fn viewport_revision(&self) -> u64 {
    self
      .projection
      .as_ref()
      .map_or(0, |snapshot| snapshot.viewport_revision)
  }
}

/// Navigation retains the same query owner, never a reconstructed query.
pub(crate) struct Snapshot {
  session: Option<Arc<BrowseSession>>,
  viewport: BrowseViewport,
  grid_viewport: Option<ArtworkGridViewport>,
  scroll_id: iced::widget::Id,
  search_input: String,
  mode: ViewMode,
  viewport_revision: Option<u64>,
  saved: bool,
  alternate_scroll: Option<ScrollSnapshot>,
}

pub(crate) fn snapshot_uses_saved(snapshot: &Snapshot) -> bool {
  snapshot.saved
}

pub(crate) fn snapshot(surface: &mut Surface, submitted_query: Option<&str>) -> Snapshot {
  if let Some(session) = &surface.session {
    let _ = session.suspend();
  }
  let snapshot = Snapshot {
    session: surface.session.take(),
    viewport: surface.viewport,
    grid_viewport: surface.grid_viewport,
    scroll_id: surface.scroll_id.clone(),
    search_input: submitted_query.unwrap_or(&surface.search_input).to_owned(),
    mode: surface.mode,
    viewport_revision: surface
      .projection
      .as_ref()
      .map(|snapshot| snapshot.viewport_revision),
    saved: surface.saved().is_some(),
    alternate_scroll: surface.alternate_scroll.take(),
  };
  surface.projection = None;
  surface.view = LibraryBrowseView::Inactive;
  snapshot
}

pub(crate) fn restore(
  surface: &mut Surface,
  kernel: &mut Kernel,
  snapshot: Snapshot,
  window_size: iced::Size,
  tv: bool,
) -> Task<Message> {
  leave_view(surface);
  surface.session = snapshot.session;
  surface.viewport = snapshot.viewport;
  surface.grid_viewport = snapshot.grid_viewport;
  surface.scroll_id = snapshot.scroll_id;
  surface.search_input = snapshot.search_input;
  surface.mode = snapshot.mode;
  surface.alternate_scroll = snapshot.alternate_scroll;
  surface.sort_menu_open = false;
  surface.presentation = if tv {
    BrowsePresentation::Tv
  } else {
    BrowsePresentation::Desktop
  };
  let Some(session) = surface.session.clone() else {
    return Task::none();
  };
  // Restore the old revision before presentation normalization so a reset
  // invalidates the retained geometry even if requests finish immediately.
  if let Ok(projection) = session.snapshot() {
    install_projection(surface, projection);
    if let (Some(projection), Some(revision)) =
      (&mut surface.projection, snapshot.viewport_revision)
    {
      projection.viewport_revision = revision;
    }
  }
  if let Err(error) = session
    .set_presentation(surface.presentation)
    .and_then(|()| session.resume())
  {
    return report_error(kernel, "browse-open-failed", error);
  }
  Task::batch([
    sync_snapshot(surface, kernel),
    sync_scroll_window(surface, kernel, window_size),
    prepare_artwork(surface),
  ])
}

pub(crate) fn set_presentation(
  surface: &mut Surface,
  kernel: &mut Kernel,
  presentation: BrowsePresentation,
) -> Task<Message> {
  surface.presentation = presentation;
  if let Some(session) = &surface.session {
    if let Err(error) = session.set_presentation(presentation) {
      return report_error(kernel, "browse-open-failed", error);
    }
  }
  sync_snapshot(surface, kernel)
}

pub(crate) fn commit_preferences(
  surface: &mut Surface,
  kernel: &mut Kernel,
  preferences: BrowsePreferences,
) -> Task<Message> {
  let Some(session) = &surface.session else {
    return Task::none();
  };
  if let Err(error) = session.set_preferences(preferences) {
    return report_error(kernel, "browse-save-filters-failed", error);
  }
  sync_snapshot(surface, kernel)
}

/// Route admission remains in the shell; mutations apply only to Library routes.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
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
      persist_filters(surface, kernel, in_library, |filters| {
        filters.with_sort(sort)
      })
    }
    BrowseMessage::SortDirectionToggled => {
      surface.sort_menu_open = false;
      persist_filters(surface, kernel, in_library, |filters| {
        let direction = match filters.sort_direction() {
          VideoLibrarySortDirection::Ascending => VideoLibrarySortDirection::Descending,
          VideoLibrarySortDirection::Descending => VideoLibrarySortDirection::Ascending,
        };
        filters.with_sort_direction(direction)
      })
    }
    BrowseMessage::PlayedFilterChanged(played_filter) => {
      persist_filters(surface, kernel, in_library, |filters| {
        filters.with_played_filter(played_filter)
      })
    }
    BrowseMessage::FavoritesToggled => persist_filters(surface, kernel, in_library, |filters| {
      filters.with_favorites_only(!filters.favorites_only())
    }),
    BrowseMessage::Scrolled {
      epoch,
      viewport_revision,
      viewport,
    } => {
      if !current_geometry(surface, epoch, viewport_revision) {
        return Task::none();
      }
      let offset = viewport.absolute_offset();
      surface.viewport = BrowseViewport { offset_y: offset.y };
      Task::none()
    }
    BrowseMessage::GridViewportMeasured {
      epoch,
      viewport_revision,
      offset_y,
      height,
    } => {
      if !current_geometry(surface, epoch, viewport_revision) {
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
      let Some(session) = &surface.session else {
        return Task::none();
      };
      if let Err(error) = session.retry() {
        return report_error(kernel, "browse-retry-failed", error);
      }
      sync_snapshot(surface, kernel)
    }
    BrowseMessage::SnapshotChanged {
      session_id,
      snapshot,
    } => {
      if surface
        .session
        .as_ref()
        .is_none_or(|session| session.id() != session_id)
        || surface
          .projection
          .as_ref()
          .is_some_and(|current| current.revision >= snapshot.revision)
      {
        return Task::none();
      }
      // A queued notification must not revive a revoked profile/session.
      if surface
        .session
        .as_ref()
        .is_none_or(|session| session.viewport_revision().is_err())
      {
        return Task::none();
      }
      let reset = apply_snapshot(surface, *snapshot, kernel);
      Task::batch([
        reset,
        sync_scroll_window(surface, kernel, window_size),
        prepare_artwork(surface),
      ])
    }
    BrowseMessage::SnapshotFailed {
      session_id,
      revision,
      error,
    } => {
      if surface.session.as_ref().is_none_or(|session| {
        session.id() != session_id || session.revision().ok() != Some(revision)
      }) || surface
        .projection
        .as_ref()
        .is_some_and(|current| current.revision >= revision)
      {
        return Task::none();
      }
      let message = sanitize_message(&error.to_string());
      if matches!(surface.view, LibraryBrowseView::Loading) {
        surface.view = LibraryBrowseView::Failed {
          message: message.clone(),
          retryable: true,
          retry_busy: false,
        };
      }
      if let Some(projection) = &mut surface.projection {
        projection.revision = revision;
        projection.refreshing = false;
        projection.refresh_failure = Some(jellypilot_core::LibraryBrowseFailure {
          message,
          retryable: true,
        });
      }
      report_error(kernel, "browse-apply-failed", error)
    }
    BrowseMessage::ArtworkLoaded(completion) => {
      surface
        .artwork
        .settle(kernel.request_gate.current_session(), completion);
      Task::none()
    }
  }
}

fn current_geometry(surface: &Surface, epoch: u64, viewport_revision: u64) -> bool {
  epoch == surface.artwork.epoch()
    && viewport_revision == surface.viewport_revision()
    && surface.session.as_ref().is_some_and(|session| {
      session
        .viewport_revision()
        .is_ok_and(|current| current == viewport_revision)
    })
}

/// Commits a TV filter draft to the current SDK query.
pub(crate) fn apply_advanced_filters(
  surface: &mut Surface,
  kernel: &mut Kernel,
  filters: VideoLibraryFilters,
) -> Task<Message> {
  if !surface
    .projection
    .as_ref()
    .is_some_and(|projection| matches!(projection.source, BrowseSource::Library { .. }))
  {
    return Task::none();
  }
  let mut preferences = surface.preferences();
  preferences.filters = filters;
  commit_preferences(surface, kernel, preferences)
}

fn persist_filters(
  surface: &mut Surface,
  kernel: &mut Kernel,
  in_library: bool,
  mutation: impl FnOnce(BrowseFilterSettings) -> BrowseFilterSettings,
) -> Task<Message> {
  if !in_library {
    return Task::none();
  }
  let filters = mutation(surface.filter_settings());
  let mut preferences = surface.preferences();
  preferences.sort = filters.sort();
  preferences.sort_direction = filters.sort_direction();
  preferences.played_filter = filters.played_filter();
  preferences.favorites_only = filters.favorites_only();
  commit_preferences(surface, kernel, preferences)
}

/// Opens ordinary browsing with the SDK's persisted default policy.
pub fn start(
  surface: &mut Surface,
  kernel: &mut Kernel,
  source: Option<BrowseSource>,
) -> Task<Message> {
  let Some(source) = source else {
    leave_view(surface);
    kernel.notice = Some(UiText::new("browse-library-unavailable"));
    return Task::none();
  };
  if surface.saved().is_none()
    && surface
      .projection
      .as_ref()
      .is_some_and(|snapshot| match (&snapshot.source, &source) {
        (
          BrowseSource::Library {
            shortcut: current, ..
          },
          BrowseSource::Library { shortcut: next, .. },
        ) => current.id == next.id && current.collection_type == next.collection_type,
        (BrowseSource::Search { query: current, .. }, BrowseSource::Search { query: next, .. }) => {
          current == next
        }
        _ => false,
      })
  {
    return sync_snapshot(surface, kernel);
  }
  let opened = match source {
    BrowseSource::Library { shortcut, .. } => kernel
      .sdk
      .open_library_browser(shortcut, surface.presentation),
    BrowseSource::Search { query, .. } => kernel
      .sdk
      .open_browser(BrowseQuery::Search { query }, surface.presentation),
  };
  match opened {
    Ok(session) => adopt(surface, kernel, session),
    Err(error) => report_error(kernel, "browse-open-failed", error),
  }
}

pub(crate) fn adopt(
  surface: &mut Surface,
  kernel: &mut Kernel,
  session: Arc<BrowseSession>,
) -> Task<Message> {
  leave_view(surface);
  surface.session = Some(session);
  sync_snapshot(surface, kernel)
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
  let metrics = surface.presentation_grid.unwrap_or_else(|| {
    browse_metrics(grid_available_width(window_size.width, class), surface.mode)
  });
  let viewport = surface.grid_viewport(window_size);
  let range = visible_display_range(
    viewport.offset_y,
    viewport.height,
    metrics.columns,
    metrics.row_height,
    total,
  );
  set_display_range(surface, kernel, range)
}

/// Both presentations share the SDK request/cancellation and artwork ownership boundary.
pub(crate) fn set_display_range(
  surface: &mut Surface,
  kernel: &mut Kernel,
  range: std::ops::Range<u32>,
) -> Task<Message> {
  let (Some(session), Some(projection)) = (&surface.session, &surface.projection) else {
    return Task::none();
  };
  if let LibraryBrowseView::Ready {
    visible_start,
    visible_items,
    ..
  } = &surface.view
  {
    if *visible_start == range.start && visible_items.len() == (range.end - range.start) as usize {
      return Task::none();
    }
  }
  if let Err(error) =
    session.set_display_range_for_revision(projection.viewport_revision, range.start, range.end)
  {
    return report_error(kernel, "browse-load-more-failed", error);
  }
  sync_snapshot(surface, kernel)
}

pub(crate) fn sync_snapshot(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let Some(session) = &surface.session else {
    return Task::none();
  };
  match session.snapshot() {
    Ok(snapshot) => {
      if surface
        .projection
        .as_ref()
        .is_some_and(|current| current.revision == snapshot.revision)
      {
        return Task::none();
      }
      let reset = apply_snapshot(surface, snapshot, kernel);
      Task::batch([reset, prepare_artwork(surface)])
    }
    Err(error) => report_error(kernel, "browse-open-failed", error),
  }
}

fn apply_snapshot(
  surface: &mut Surface,
  snapshot: BrowseSnapshot,
  kernel: &mut Kernel,
) -> Task<Message> {
  let reset_viewport = surface
    .projection
    .as_ref()
    .is_none_or(|current| current.viewport_revision != snapshot.viewport_revision);
  if snapshot.refresh_failure.is_some()
    && surface
      .projection
      .as_ref()
      .is_some_and(|current| current.refreshing)
  {
    if let Some(error) = &snapshot.refresh_failure {
      kernel.notice =
        Some(UiText::new("browse-refresh-failed").arg("details", sanitize_message(&error.message)));
    }
  }
  install_projection(surface, snapshot);
  if !reset_viewport {
    return Task::none();
  }
  surface.viewport.offset_y = 0.0;
  surface.grid_viewport = None;
  surface.alternate_scroll = None;
  surface.scroll_id = iced::widget::Id::unique();
  // Retire geometry callbacks before accepting any new-query page projection.
  surface.artwork.clear();
  operation::scrollable::scroll_to(
    surface.scroll_id.clone(),
    operation::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
    iced::advanced::widget::operation::Animation::Instant,
  )
}

pub(crate) fn refresh(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  let Some(session) = &surface.session else {
    return Task::none();
  };
  if let Err(error) = session.refresh() {
    return report_error(kernel, "browse-refresh-failed", error);
  }
  sync_snapshot(surface, kernel)
}

fn report_error(
  kernel: &mut Kernel,
  label: &'static str,
  error: jellypilot_sdk::SdkError,
) -> Task<Message> {
  kernel.diagnostics.record(
    DiagnosticLevel::Error,
    DiagnosticCategory::Connection,
    format!("Library browsing failed: {error}"),
  );
  kernel.notice = Some(UiText::new(label).arg("details", sanitize_message(&error.to_string())));
  Task::none()
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
  if let Some(session) = surface.session.take() {
    session.close();
  }
  surface.projection = None;
  surface.view = LibraryBrowseView::Inactive;
  surface.alternate_scroll = None;
}

/// Browse portion of the router's connected-surface reset: cancels in-flight
/// page requests, drops the artwork cells, and resets the browser and view.
pub(crate) fn reset(surface: &mut Surface) {
  leave_view(surface);
}

#[cfg(test)]
mod tests {
  use iced::widget::Widget as _;
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::VideoLibraryItem;

  use crate::app::test_support::{
    activate_sdk_fixture, drain_task, fixture_item_id, BrowseFixture, FixtureReply,
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
    activate_sdk_fixture(&mut kernel, &fixture);
    (surface, kernel, fixture)
  }

  fn search_source(kernel: &Kernel, query: &str) -> BrowseSource {
    BrowseSource::Search {
      session: kernel.request_gate.current_session(),
      query: query.to_owned(),
    }
  }
  fn display_range(surface: &Surface) -> std::ops::Range<u32> {
    let LibraryBrowseView::Ready {
      visible_start,
      visible_items,
      ..
    } = &surface.view
    else {
      panic!("ready projection required");
    };
    *visible_start..*visible_start + visible_items.len() as u32
  }

  async fn settle(
    surface: &mut Surface,
    kernel: &mut Kernel,
    fixture: &mut BrowseFixture,
    total: u32,
  ) {
    drain_task(
      surface,
      kernel,
      fixture,
      window_size(),
      Task::none(),
      &mut |_| FixtureReply::Page {
        total,
        artwork: false,
      },
    )
    .await;
  }

  #[tokio::test]
  async fn advanced_filters_restore_in_tv_and_restart_unfiltered_in_desktop() {
    let (mut surface, mut kernel, _fixture) = fixture_surface();
    surface.presentation = BrowsePresentation::Tv;
    let source = BrowseSource::Library {
      session: kernel.request_gate.current_session(),
      shortcut: jellypilot_media_server::VideoLibraryShortcut {
        id: "movies".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: None,
        artwork_image_id: None,
      },
    };
    drop(start(&mut surface, &mut kernel, Some(source)));
    let filters = VideoLibraryFilters {
      country: Some("Japan".to_owned()),
      ..Default::default()
    };
    drop(apply_advanced_filters(
      &mut surface,
      &mut kernel,
      filters.clone(),
    ));
    let identity = surface.identity().expect("filtered query").to_owned();
    surface.viewport.offset_y = 600.0;
    let saved = snapshot(&mut surface, None);
    leave_view(&mut surface);
    drop(restore(
      &mut surface,
      &mut kernel,
      saved,
      window_size(),
      true,
    ));
    assert_eq!(surface.preferences().filters, filters);
    assert_eq!(surface.identity(), Some(identity.as_str()));
    assert_eq!(surface.viewport.offset_y, 600.0);
    let saved = snapshot(&mut surface, None);
    leave_view(&mut surface);
    let task = restore(&mut surface, &mut kernel, saved, window_size(), false);
    assert_eq!(
      surface.preferences().filters,
      VideoLibraryFilters::default()
    );
    assert_ne!(surface.identity(), Some(identity.as_str()));
    assert_eq!(surface.viewport.offset_y, 0.0);
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
    assert!(
      iced_runtime::task::into_stream(task).is_some(),
      "desktop restore requests its first unfiltered page"
    );
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
    surface.viewport.offset_y = 600.0;
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 600.0,
      height: 500.0,
    });
    let grid_id = surface.scroll_id.clone();
    let filters = surface.filter_settings();
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::ViewModeSelected(ViewMode::List),
    ));
    let list_id = surface.scroll_id.clone();
    assert_ne!(list_id, grid_id);
    let epoch = surface.artwork.epoch();
    let viewport_revision = surface.viewport_revision();
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        viewport_revision,
        offset_y: 810.0,
        height: 405.0,
      },
    ));
    let expected = visible_display_range(810.0, 405.0, 1, 81.0, 240);
    assert_eq!(display_range(&surface), expected);
    surface.viewport.offset_y = 810.0;
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::ViewModeSelected(ViewMode::Grid),
    ));
    assert_eq!(surface.viewport.offset_y, 600.0);
    assert_eq!(surface.scroll_id, grid_id);
    assert_eq!(surface.filter_settings(), filters);
    assert_eq!(surface.search_input, "retained query");
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::ViewModeSelected(ViewMode::List),
    ));
    let saved = snapshot(&mut surface, Some("retained query"));
    leave_view(&mut surface);
    drop(restore(
      &mut surface,
      &mut kernel,
      saved,
      window_size(),
      true,
    ));
    assert_eq!(surface.mode, ViewMode::List);
    assert_eq!(surface.viewport.offset_y, 810.0);
    assert_eq!(surface.scroll_id, list_id);
    assert_eq!(surface.search_input, "retained query");
  }

  #[tokio::test]
  async fn resumed_refresh_rejects_departed_results_and_retains_cards_on_failure() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "original");
    drop(start(&mut surface, &mut kernel, Some(source)));
    settle(&mut surface, &mut kernel, &mut fixture, 240).await;
    drop(set_display_range(&mut surface, &mut kernel, 192..216));
    let session = surface.session.clone().unwrap();
    fixture
      .settle_session(&session, |_| FixtureReply::Page {
        total: 240,
        artwork: false,
      })
      .await;
    drop(sync_snapshot(&mut surface, &mut kernel));
    surface.viewport.offset_y = 8_000.0;
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 8_000.0,
      height: WINDOW_HEIGHT,
    });
    let scroll_id = surface.scroll_id.clone();
    drop(refresh(&mut surface, &mut kernel));
    let departed_request = fixture.receive_request().await;
    let departed = session.snapshot().unwrap();
    let saved = snapshot(&mut surface, Some("original"));
    departed_request.reply(FixtureReply::Failure);
    drop(restore(
      &mut surface,
      &mut kernel,
      saved,
      window_size(),
      true,
    ));
    let revision = surface.projection.as_ref().unwrap().revision;
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::SnapshotChanged {
        session_id: session.id(),
        snapshot: Box::new(departed),
      },
    ));
    assert_eq!(surface.projection.as_ref().unwrap().revision, revision);
    fixture
      .settle_session(&session, |_| FixtureReply::Failure)
      .await;
    drop(sync_snapshot(&mut surface, &mut kernel));
    assert!(!surface.refreshing());
    assert!(surface
      .projection
      .as_ref()
      .unwrap()
      .refresh_failure
      .is_some());
    assert_eq!(surface.viewport.offset_y, 8_000.0);
    assert_eq!(surface.scroll_id, scroll_id);
    drop(set_display_range(&mut surface, &mut kernel, 192..216));
    let LibraryBrowseView::Ready { visible_items, .. } = &surface.view else {
      panic!("retained cards");
    };
    assert_eq!(
      visible_items[0].item.as_ref().unwrap().id,
      fixture_item_id(192)
    );
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::Retry,
    ));
    assert!(surface.refreshing());
    fixture
      .settle_session(&session, |_| FixtureReply::Failure)
      .await;
  }

  #[tokio::test]
  async fn empty_refresh_failure_survives_history_and_retries_without_resetting_scroll() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "empty");
    drop(start(&mut surface, &mut kernel, Some(source)));
    settle(&mut surface, &mut kernel, &mut fixture, 0).await;
    let session = surface.session.clone().unwrap();
    drop(refresh(&mut surface, &mut kernel));
    fixture
      .settle_session(&session, |_| FixtureReply::Failure)
      .await;
    drop(sync_snapshot(&mut surface, &mut kernel));
    let saved = snapshot(&mut surface, Some("empty"));
    drop(restore(
      &mut surface,
      &mut kernel,
      saved,
      window_size(),
      true,
    ));
    assert!(matches!(surface.view, LibraryBrowseView::Empty));
    assert!(surface
      .projection
      .as_ref()
      .unwrap()
      .refresh_failure
      .is_some());
    let scroll_id = surface.scroll_id.clone();
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::Retry,
    ));
    assert!(surface.refreshing());
    assert_eq!(surface.scroll_id, scroll_id);
    fixture
      .settle_session(&session, |_| FixtureReply::Failure)
      .await;
    drop(sync_snapshot(&mut surface, &mut kernel));
    assert!(matches!(surface.view, LibraryBrowseView::Empty));
    assert!(surface
      .projection
      .as_ref()
      .unwrap()
      .refresh_failure
      .is_some());
  }

  #[tokio::test]
  async fn snapshot_suspends_owned_delivery_without_a_separate_leave_hook() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "pending");
    drop(start(&mut surface, &mut kernel, Some(source)));
    let request = fixture.receive_request().await;
    let session = surface.session.clone().unwrap();
    let saved = snapshot(&mut surface, Some("pending"));
    request.reply(FixtureReply::Failure);
    assert!(surface.session.is_none());
    drop(restore(
      &mut surface,
      &mut kernel,
      saved,
      window_size(),
      false,
    ));
    assert_eq!(surface.session.as_ref().unwrap().id(), session.id());
    fixture
      .settle_session(&session, |_| FixtureReply::Page {
        total: 1,
        artwork: false,
      })
      .await;
    drop(sync_snapshot(&mut surface, &mut kernel));
    assert!(matches!(surface.view, LibraryBrowseView::Ready { .. }));
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
    drain_task(
      &mut surface,
      &mut kernel,
      &mut fixture,
      window_size(),
      task,
      &mut |_| FixtureReply::Failure,
    )
    .await;
    assert!(surface.artwork.get(&item.id).is_some());
    assert_eq!(surface.artwork.epoch(), epoch);
  }

  #[tokio::test]
  async fn identical_browse_resubmit_starts_no_new_page_request() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "arrival");
    drop(start(&mut surface, &mut kernel, Some(source.clone())));
    let request = fixture.receive_request().await;

    let resubmit = start(&mut surface, &mut kernel, Some(source));

    assert!(iced_runtime::task::into_stream(resubmit).is_none());
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
    drop(request);
  }

  #[tokio::test]
  async fn departed_snapshot_is_rejected_after_reopening_the_same_query() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "arrival");
    drop(start(&mut surface, &mut kernel, Some(source.clone())));
    let request = fixture.receive_request().await;
    let departed = surface.session.clone().unwrap();
    let stale = departed.snapshot().unwrap();
    leave_view(&mut surface);
    request.reply(FixtureReply::Failure);
    drop(start(&mut surface, &mut kernel, Some(source)));
    let current = surface.session.clone().unwrap();
    assert_ne!(current.id(), departed.id());
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::SnapshotChanged {
        session_id: departed.id(),
        snapshot: Box::new(stale),
      },
    ));
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
    fixture
      .settle_session(&current, |_| FixtureReply::Page {
        total: 1,
        artwork: false,
      })
      .await;
    drop(sync_snapshot(&mut surface, &mut kernel));
    assert!(matches!(surface.view, LibraryBrowseView::Ready { .. }));
  }

  #[tokio::test]
  async fn scoped_snapshot_failure_exposes_retry_and_late_failure_cannot_replace_recovery() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = search_source(&kernel, "failure delivery");
    drop(start(&mut surface, &mut kernel, Some(source)));
    let request = fixture.receive_request().await;
    let session = surface.session.clone().unwrap();
    session.suspend().unwrap();
    let revision = session.revision().unwrap();
    let failure = BrowseMessage::SnapshotFailed {
      session_id: session.id(),
      revision,
      error: jellypilot_sdk::SdkError::Request("Could not apply browse results".into()),
    };
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      failure.clone(),
    ));
    assert!(matches!(
      surface.view,
      LibraryBrowseView::Failed {
        retryable: true,
        retry_busy: false,
        ..
      }
    ));
    assert!(kernel.notice.is_some());
    session.resume().unwrap();
    drop(sync_snapshot(&mut surface, &mut kernel));
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      failure,
    ));
    assert!(matches!(surface.view, LibraryBrowseView::Loading));
    request.reply(FixtureReply::Failure);
  }

  #[tokio::test]
  async fn query_reset_retires_old_geometry_before_new_pages_can_advance_the_window() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let epoch = surface.artwork.epoch();
    let viewport_revision = surface.viewport_revision();
    surface.viewport.offset_y = 8_000.0;
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 8_000.0,
      height: 300.0,
    });
    let source = search_source(&kernel, "reset");
    drop(start(&mut surface, &mut kernel, Some(source)));
    assert_eq!(surface.viewport.offset_y, 0.0);
    assert_eq!(surface.grid_viewport, None);
    settle(&mut surface, &mut kernel, &mut fixture, 264).await;
    let before = display_range(&surface);
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        viewport_revision,
        offset_y: 8_000.0,
        height: 300.0,
      },
    ));
    assert_eq!(surface.grid_viewport, None);
    assert_eq!(display_range(&surface), before);
    assert_eq!(before.start, 0);
  }

  #[tokio::test]
  async fn sdk_query_reset_rejects_geometry_before_its_notification_is_consumed() {
    let (mut surface, mut kernel, mut fixture) = fixture_surface();
    let source = BrowseSource::Library {
      session: kernel.request_gate.current_session(),
      shortcut: jellypilot_media_server::VideoLibraryShortcut {
        id: "movies".into(),
        name: "Movies".into(),
        collection_type: "movies".into(),
        item_count: None,
        artwork_image_id: None,
      },
    };
    drop(start(&mut surface, &mut kernel, Some(source)));
    settle(&mut surface, &mut kernel, &mut fixture, 264).await;
    let epoch = surface.artwork.epoch();
    let viewport_revision = surface.viewport_revision();
    let session = surface.session.clone().unwrap();
    let mut preferences = session.snapshot().unwrap().preferences;
    preferences.sort = jellypilot_media_server::VideoLibrarySort::ReleaseDate;
    session.set_preferences(preferences).unwrap();
    // The callback still has the current artwork epoch, but belongs to the
    // previous SDK viewport revision and must not alter even cached geometry.
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        viewport_revision,
        offset_y: 8_000.0,
        height: 300.0,
      },
    ));
    assert_eq!(surface.grid_viewport, None);
    drop(sync_snapshot(&mut surface, &mut kernel));
    assert_ne!(surface.viewport_revision(), viewport_revision);
    settle(&mut surface, &mut kernel, &mut fixture, 264).await;
    assert_eq!(display_range(&surface).start, 0);
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
    let initial = display_range(&surface);
    assert_eq!(initial.start, 0);
    assert!(
      (initial.end as usize / metrics.columns) as f32 * metrics.row_height >= 2.0 * WINDOW_HEIGHT
    );
    assert!(initial.end < 264, "scroll projection must remain sparse");

    let epoch = surface.artwork.epoch();
    let viewport_revision = surface.viewport_revision();
    let task = update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        viewport_revision,
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
    let measured_initial = display_range(&surface);
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
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        viewport_revision,
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
    let scrolled = display_range(&surface);
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
    assert_eq!(display_range(&surface), scrolled);

    // Scrolling back up restores the earlier window.
    let task = update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        viewport_revision,
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
    assert_eq!(display_range(&surface), measured_initial);
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
        iced::widget::Space::new().boxed()
      },
    );
    assert_eq!(*built.borrow(), vec![0, 1, 2]);
    assert!(surface.artwork.is_empty());
  }

  #[test]
  fn recreated_grid_ignores_previous_epoch_geometry() {
    let (mut surface, mut kernel) = test_fixture();
    let epoch = surface.artwork.epoch();
    let viewport_revision = surface.viewport_revision();
    surface.grid_viewport = Some(ArtworkGridViewport {
      offset_y: 400.0,
      height: 300.0,
    });
    leave_view(&mut surface);
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      window_size(),
      BrowseMessage::GridViewportMeasured {
        epoch,
        viewport_revision,
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
