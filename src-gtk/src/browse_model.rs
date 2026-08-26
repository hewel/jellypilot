use std::ops::Range;

use jellypilot_core::{
  LibraryBrowseCoreError, LibraryBrowseLoadToken, LibraryBrowseMode, LIBRARY_BROWSE_PAGE_SIZE,
};
use jellypilot_media_server::{
  VideoLibraryItem, VideoLibraryPage, VideoLibraryPlayedFilter, VideoLibraryShortcut,
  VideoLibrarySort, VideoLibrarySortDirection, VideoSearchPage,
};

use crate::library_browse::{
  LibraryBrowseEffect, LibraryBrowseInput, LibraryBrowseView, NativeLibraryBrowse,
};
use crate::request_gate::SessionToken;

/// Complete identity of one active native browse result.
#[derive(Clone, Debug)]
pub(crate) enum BrowseSource {
  Library {
    session: SessionToken,
    shortcut: VideoLibraryShortcut,
  },
  Search {
    session: SessionToken,
    query: String,
  },
}

impl BrowseSource {
  #[must_use]
  pub(crate) fn identity(&self) -> String {
    match self {
      Self::Library { session, shortcut } => format!(
        "session:{session:?}:library:{}:{}:{}",
        shortcut.id.len(),
        shortcut.id,
        shortcut.collection_type
      ),
      Self::Search { session, query } => {
        format!("session:{session:?}:search:{}:{query}", query.len())
      }
    }
  }
}

/// Environment work emitted by the display-free browse model.
#[derive(Clone, Debug)]
pub(crate) enum BrowseEffect {
  ResetViewport,
  RequestPage(BrowsePageRequest),
  CancelPage,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BrowsePreferences {
  pub(crate) sort: VideoLibrarySort,
  pub(crate) sort_direction: VideoLibrarySortDirection,
  pub(crate) played_filter: VideoLibraryPlayedFilter,
  pub(crate) favorites_only: bool,
}

impl Default for BrowsePreferences {
  fn default() -> Self {
    Self {
      sort: VideoLibrarySort::Title,
      sort_direction: VideoLibrarySortDirection::Ascending,
      played_filter: VideoLibraryPlayedFilter::All,
      favorites_only: false,
    }
  }
}

impl BrowsePreferences {
  fn identity(self) -> String {
    let sort = match self.sort {
      VideoLibrarySort::Title => "title",
      VideoLibrarySort::RecentlyAdded => "added",
      VideoLibrarySort::ReleaseDate => "release",
    };
    let direction = match self.sort_direction {
      VideoLibrarySortDirection::Ascending => "asc",
      VideoLibrarySortDirection::Descending => "desc",
    };
    let played = match self.played_filter {
      VideoLibraryPlayedFilter::All => "all",
      VideoLibraryPlayedFilter::Played => "played",
      VideoLibraryPlayedFilter::Unplayed => "unplayed",
    };
    let favorites = if self.favorites_only {
      "favorites"
    } else {
      "all"
    };
    format!("{sort}:{direction}:{played}:{favorites}")
  }
}

/// One token-correlated request for the media-server adapter.
#[derive(Clone, Debug)]
pub(crate) struct BrowsePageRequest {
  pub(crate) source_id: String,
  pub(crate) source: BrowseSource,
  pub(crate) token: LibraryBrowseLoadToken,
  pub(crate) start_index: u32,
  pub(crate) limit: u32,
  pub(crate) preferences: BrowsePreferences,
}

/// Provider-neutral payload returned to the browse reducer.
#[derive(Clone, Debug)]
pub(crate) struct BrowsePagePayload {
  pub(crate) start_index: u32,
  pub(crate) limit: u32,
  pub(crate) total_record_count: u32,
  pub(crate) has_more: bool,
  pub(crate) items: Vec<VideoLibraryItem>,
}

impl TryFrom<VideoLibraryPage> for BrowsePagePayload {
  type Error = String;

  fn try_from(page: VideoLibraryPage) -> Result<Self, Self::Error> {
    Ok(Self {
      start_index: unsigned_page_value(page.start_index, "start index")?,
      limit: unsigned_page_value(page.limit, "limit")?,
      total_record_count: unsigned_page_value(page.total_record_count, "total record count")?,
      has_more: page.has_more,
      items: page.items,
    })
  }
}

impl TryFrom<VideoSearchPage> for BrowsePagePayload {
  type Error = String;

  fn try_from(page: VideoSearchPage) -> Result<Self, Self::Error> {
    Ok(Self {
      start_index: unsigned_page_value(page.start_index, "start index")?,
      limit: unsigned_page_value(page.limit, "limit")?,
      total_record_count: unsigned_page_value(page.total_record_count, "total record count")?,
      has_more: page.has_more,
      items: page.items,
    })
  }
}

/// A completed request, including the source identity that issued it.
#[derive(Debug)]
pub(crate) struct BrowsePageSettlement {
  pub(crate) source_id: String,
  pub(crate) token: LibraryBrowseLoadToken,
  pub(crate) result: Result<BrowsePagePayload, String>,
}

/// Display-free native browse state used by the live GTK shell.
#[derive(Clone, Debug, Default)]
pub(crate) struct BrowseModel {
  adapter: NativeLibraryBrowse<VideoLibraryItem>,
  source: Option<BrowseSource>,
  source_id: Option<String>,
  virtual_window_start: u32,
  preferences: BrowsePreferences,
}

impl BrowseModel {
  pub(crate) fn reset(&mut self) {
    *self = Self::default();
  }

  pub(crate) fn configure(
    &mut self,
    source: BrowseSource,
  ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
    self.configure_with_preferences(source, BrowsePreferences::default())
  }

  pub(crate) fn configure_with_preferences(
    &mut self,
    source: BrowseSource,
    preferences: BrowsePreferences,
  ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
    let source_id = format!(
      "{}:preferences:{}",
      source.identity(),
      preferences.identity()
    );
    let source_changed = self.source_id.as_deref() != Some(source_id.as_str());
    let effects = self.adapter.handle(LibraryBrowseInput::Configure {
      source_id: source_id.clone(),
    })?;
    if source_changed {
      self.virtual_window_start = 0;
    }
    self.source = Some(source);
    self.source_id = Some(source_id);
    self.preferences = preferences;
    Ok(self.translate(effects))
  }

  pub(crate) fn load_next(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
    let effects = match self.adapter.view() {
      LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Normal,
        ..
      } => self.adapter.handle(LibraryBrowseInput::LoadNext)?,
      LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Virtual,
        total_record_count,
        ..
      } => {
        let next_start = self
          .virtual_window_start
          .checked_add(LIBRARY_BROWSE_PAGE_SIZE)
          .unwrap_or(total_record_count)
          .min(last_virtual_window_start(total_record_count));
        if next_start == self.virtual_window_start {
          Vec::new()
        } else {
          self.set_virtual_window(next_start, total_record_count)?
        }
      }
      LibraryBrowseView::Inactive
      | LibraryBrowseView::Loading
      | LibraryBrowseView::Empty
      | LibraryBrowseView::Failed { .. } => Vec::new(),
    };
    Ok(self.translate(effects))
  }

  pub(crate) fn load_previous(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
    let effects = match self.adapter.view() {
      LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Virtual,
        total_record_count,
        ..
      } => {
        let previous_start = self
          .virtual_window_start
          .saturating_sub(LIBRARY_BROWSE_PAGE_SIZE);
        if previous_start == self.virtual_window_start {
          Vec::new()
        } else {
          self.set_virtual_window(previous_start, total_record_count)?
        }
      }
      LibraryBrowseView::Inactive
      | LibraryBrowseView::Loading
      | LibraryBrowseView::Empty
      | LibraryBrowseView::Failed { .. }
      | LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Normal,
        ..
      } => Vec::new(),
    };
    Ok(self.translate(effects))
  }

  pub(crate) fn retry(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
    let effects = self.adapter.handle(LibraryBrowseInput::Retry)?;
    Ok(self.translate(effects))
  }

  pub(crate) fn settle(
    &mut self,
    settlement: BrowsePageSettlement,
  ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
    if self.source_id.as_deref() != Some(settlement.source_id.as_str()) {
      return Ok(Vec::new());
    }

    let input = match settlement.result {
      Ok(page) => LibraryBrowseInput::PageLoaded {
        token: settlement.token,
        start_index: page.start_index,
        limit: page.limit,
        total_record_count: page.total_record_count,
        has_more: page.has_more,
        items: page.items,
      },
      Err(message) => LibraryBrowseInput::PageFailed {
        token: settlement.token,
        message,
        retryable: true,
      },
    };
    let mut effects = self.adapter.handle(input)?;
    if let LibraryBrowseView::Ready {
      mode: LibraryBrowseMode::Virtual,
      total_record_count,
      ..
    } = self.adapter.view()
    {
      effects.extend(self.update_virtual_window(total_record_count)?);
    }
    Ok(self.translate(effects))
  }

  #[must_use]
  pub(crate) fn view(&self) -> LibraryBrowseView<VideoLibraryItem> {
    self.adapter.view()
  }

  #[must_use]
  pub(crate) fn can_load_more(&self) -> bool {
    match self.adapter.view() {
      LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Normal,
        can_load_next,
        ..
      } => can_load_next,
      LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Virtual,
        total_record_count,
        ..
      } => self.virtual_window_start < last_virtual_window_start(total_record_count),
      LibraryBrowseView::Inactive
      | LibraryBrowseView::Loading
      | LibraryBrowseView::Empty
      | LibraryBrowseView::Failed { .. } => false,
    }
  }

  #[must_use]
  pub(crate) fn can_load_previous(&self) -> bool {
    matches!(
      self.adapter.view(),
      LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Virtual,
        ..
      } if self.virtual_window_start > 0
    )
  }

  #[must_use]
  pub(crate) fn display_range(&self) -> Option<Range<u32>> {
    match self.adapter.view() {
      LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Virtual,
        total_record_count,
        ..
      } => Some(virtual_window_range(
        self.virtual_window_start,
        total_record_count,
      )),
      LibraryBrowseView::Inactive
      | LibraryBrowseView::Loading
      | LibraryBrowseView::Empty
      | LibraryBrowseView::Failed { .. }
      | LibraryBrowseView::Ready {
        mode: LibraryBrowseMode::Normal,
        ..
      } => None,
    }
  }

  fn update_virtual_window(
    &mut self,
    total_record_count: u32,
  ) -> Result<Vec<LibraryBrowseEffect>, LibraryBrowseCoreError> {
    self.set_virtual_window(self.virtual_window_start, total_record_count)
  }

  fn set_virtual_window(
    &mut self,
    start: u32,
    total_record_count: u32,
  ) -> Result<Vec<LibraryBrowseEffect>, LibraryBrowseCoreError> {
    let range = virtual_window_range(start, total_record_count);
    let effects = self.adapter.handle(LibraryBrowseInput::WindowChanged {
      display_indexes: range.clone().collect(),
    })?;
    self.virtual_window_start = range.start;
    Ok(effects)
  }

  fn translate(&self, effects: Vec<LibraryBrowseEffect>) -> Vec<BrowseEffect> {
    effects
      .into_iter()
      .filter_map(|effect| match effect {
        LibraryBrowseEffect::ResetViewport => Some(BrowseEffect::ResetViewport),
        LibraryBrowseEffect::CancelPage { .. } => Some(BrowseEffect::CancelPage),
        LibraryBrowseEffect::RequestPage {
          token,
          start_index,
          limit,
        } => Some(BrowseEffect::RequestPage(BrowsePageRequest {
          source_id: self.source_id.clone()?,
          source: self.source.clone()?,
          token,
          start_index,
          limit,
          preferences: self.preferences,
        })),
      })
      .collect()
  }
}

fn last_virtual_window_start(total_record_count: u32) -> u32 {
  total_record_count.checked_sub(1).map_or(0, |last_index| {
    (last_index / LIBRARY_BROWSE_PAGE_SIZE) * LIBRARY_BROWSE_PAGE_SIZE
  })
}

fn virtual_window_range(start: u32, total_record_count: u32) -> Range<u32> {
  let start = start.min(last_virtual_window_start(total_record_count));
  let end = start
    .checked_add(LIBRARY_BROWSE_PAGE_SIZE)
    .unwrap_or(total_record_count)
    .min(total_record_count);
  start..end
}

fn unsigned_page_value(value: i32, name: &str) -> Result<u32, String> {
  u32::try_from(value).map_err(|_| format!("Media server returned a negative page {name}."))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::request_gate::RequestGate;

  fn session() -> SessionToken {
    RequestGate::default().current_session()
  }

  fn shortcut() -> VideoLibraryShortcut {
    VideoLibraryShortcut {
      id: "library-1".to_owned(),
      name: "Movies".to_owned(),
      collection_type: "movies".to_owned(),
      item_count: Some(30),
      artwork_image_id: None,
    }
  }

  fn items(start: u32, count: u32) -> Vec<VideoLibraryItem> {
    (start..start + count)
      .map(|index| VideoLibraryItem {
        id: format!("item-{index}"),
        name: format!("Item {index}"),
        item_type: "Movie".to_owned(),
        production_year: None,
        runtime_seconds: None,
        played: false,
        favorite: false,
        artwork_image_id: None,
        series_poster_image_id: None,
        season_number: None,
        episode_number: None,
        series_id: None,
        series_name: None,
        resume_position_seconds: None,
        played_percentage: None,
        overview: None,
      })
      .collect()
  }

  fn request(effects: Vec<BrowseEffect>) -> BrowsePageRequest {
    effects
      .into_iter()
      .find_map(|effect| match effect {
        BrowseEffect::RequestPage(request) => Some(request),
        BrowseEffect::ResetViewport | BrowseEffect::CancelPage => None,
      })
      .expect("page request should be emitted")
  }

  fn requests(effects: &[BrowseEffect]) -> Vec<BrowsePageRequest> {
    effects
      .iter()
      .filter_map(|effect| match effect {
        BrowseEffect::RequestPage(request) => Some(request.clone()),
        BrowseEffect::ResetViewport | BrowseEffect::CancelPage => None,
      })
      .collect()
  }

  fn settle(
    model: &mut BrowseModel,
    request: &BrowsePageRequest,
    total: u32,
    count: u32,
  ) -> Vec<BrowseEffect> {
    model
      .settle(BrowsePageSettlement {
        source_id: request.source_id.clone(),
        token: request.token,
        result: Ok(BrowsePagePayload {
          start_index: request.start_index,
          limit: request.limit,
          total_record_count: total,
          has_more: request
            .start_index
            .checked_add(count)
            .is_some_and(|end| end < total),
          items: items(request.start_index, count),
        }),
      })
      .expect("page should settle")
  }

  fn settle_all_requests(model: &mut BrowseModel, effects: Vec<BrowseEffect>, total: u32) {
    let mut pending = requests(&effects);
    while let Some(request) = pending.pop() {
      let count = total.saturating_sub(request.start_index).min(request.limit);
      pending.extend(requests(&settle(model, &request, total, count)));
    }
  }

  fn visible_indexes(model: &BrowseModel) -> Vec<u32> {
    match model.view() {
      LibraryBrowseView::Ready { visible_items, .. } => visible_items
        .into_iter()
        .map(|slot| slot.display_index)
        .collect(),
      LibraryBrowseView::Inactive
      | LibraryBrowseView::Loading
      | LibraryBrowseView::Empty
      | LibraryBrowseView::Failed { .. } => Vec::new(),
    }
  }

  #[test]
  fn search_continuation_appends_and_retains_query() {
    let mut model = BrowseModel::default();
    let first = request(
      model
        .configure(BrowseSource::Search {
          session: session(),
          query: "arrival".to_owned(),
        })
        .expect("search should configure"),
    );
    settle(&mut model, &first, 30, 24);

    let next = request(model.load_next().expect("continuation should schedule"));
    assert!(matches!(
      next.source,
      BrowseSource::Search { ref query, .. } if query == "arrival"
    ));
    settle(&mut model, &next, 30, 6);

    assert!(matches!(
      model.view(),
      LibraryBrowseView::Ready {
        ref visible_items,
        can_load_next: false,
        ..
      } if visible_items.len() == 30
    ));
  }

  #[test]
  fn failed_continuation_keeps_items_and_retries_same_search_page() {
    let mut model = BrowseModel::default();
    let first = request(
      model
        .configure(BrowseSource::Search {
          session: session(),
          query: "dune".to_owned(),
        })
        .expect("search should configure"),
    );
    settle(&mut model, &first, 30, 24);
    let failed = request(model.load_next().expect("continuation should schedule"));
    model
      .settle(BrowsePageSettlement {
        source_id: failed.source_id.clone(),
        token: failed.token,
        result: Err("temporary failure".to_owned()),
      })
      .expect("failure should settle");

    assert!(matches!(
      model.view(),
      LibraryBrowseView::Ready {
        ref visible_items,
        ref load_more_failure,
        ..
      } if visible_items.len() == 24 && load_more_failure.as_deref() == Some("temporary failure")
    ));
    let retry = request(model.retry().expect("retry should schedule"));
    assert_eq!(retry.start_index, 24);
    assert!(matches!(
      retry.source,
      BrowseSource::Search { ref query, .. } if query == "dune"
    ));
  }

  #[test]
  fn virtual_window_keeps_slots_and_retained_payloads_bounded_after_many_pages() {
    const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 80;
    let mut model = BrowseModel::default();
    let bootstrap = model
      .configure(BrowseSource::Library {
        session: session(),
        shortcut: shortcut(),
      })
      .expect("library should configure");
    settle_all_requests(&mut model, bootstrap, TOTAL);

    let mut max_visible_slots = visible_indexes(&model).len();
    let mut max_retained_pages = model.adapter.retained_page_count();
    let mut max_retained_items = model.adapter.retained_item_count();
    for _ in 0..50 {
      let effects = model.load_next().expect("next window should load");
      settle_all_requests(&mut model, effects, TOTAL);
      max_visible_slots = max_visible_slots.max(visible_indexes(&model).len());
      max_retained_pages = max_retained_pages.max(model.adapter.retained_page_count());
      max_retained_items = max_retained_items.max(model.adapter.retained_item_count());
    }

    assert_eq!(max_visible_slots, LIBRARY_BROWSE_PAGE_SIZE as usize);
    assert_eq!(max_retained_pages, 3);
    assert_eq!(max_retained_items, (LIBRARY_BROWSE_PAGE_SIZE * 3) as usize);
  }

  #[test]
  fn load_previous_restores_a_released_earlier_window() {
    const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10;
    let mut model = BrowseModel::default();
    let bootstrap = model
      .configure(BrowseSource::Library {
        session: session(),
        shortcut: shortcut(),
      })
      .expect("library should configure");
    settle_all_requests(&mut model, bootstrap, TOTAL);
    assert!(!model.can_load_previous());
    let first_advance = model.load_next().expect("first next window should load");
    settle_all_requests(&mut model, first_advance, TOTAL);
    let second_advance = model.load_next().expect("second next window should load");
    settle_all_requests(&mut model, second_advance, TOTAL);

    let previous = model.load_previous().expect("previous window should load");
    assert_eq!(
      request(previous.clone()).start_index,
      LIBRARY_BROWSE_PAGE_SIZE
    );
    settle_all_requests(&mut model, previous, TOTAL);

    assert_eq!(
      model.display_range(),
      Some(LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2)
    );
    assert_eq!(
      visible_indexes(&model),
      (LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2).collect::<Vec<_>>()
    );
    assert!(model.can_load_previous());
  }

  #[test]
  fn final_virtual_window_clamps_its_range_and_disables_next() {
    const TOTAL: u32 = 101;
    let mut model = BrowseModel::default();
    let bootstrap = model
      .configure(BrowseSource::Library {
        session: session(),
        shortcut: shortcut(),
      })
      .expect("library should configure");
    settle_all_requests(&mut model, bootstrap, TOTAL);
    while model.can_load_more() {
      let effects = model.load_next().expect("next window should load");
      settle_all_requests(&mut model, effects, TOTAL);
    }

    assert_eq!(model.display_range(), Some(96..101));
    assert_eq!(visible_indexes(&model), (96..101).collect::<Vec<_>>());
    assert!(model
      .load_next()
      .expect("last window should remain stable")
      .is_empty());
  }

  #[test]
  fn settlement_for_a_window_that_moved_on_does_not_retain_its_payload() {
    const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10;
    let mut model = BrowseModel::default();
    let bootstrap = request(
      model
        .configure(BrowseSource::Library {
          session: session(),
          shortcut: shortcut(),
        })
        .expect("library should configure"),
    );
    let first_prefetch = request(settle(
      &mut model,
      &bootstrap,
      TOTAL,
      LIBRARY_BROWSE_PAGE_SIZE,
    ));
    let second_page = request(model.load_next().expect("first next window should load"));
    model
      .load_next()
      .expect("window should move past the first prefetch");

    let replacement = settle(&mut model, &first_prefetch, TOTAL, LIBRARY_BROWSE_PAGE_SIZE);

    assert_eq!(
      model.display_range(),
      Some(LIBRARY_BROWSE_PAGE_SIZE * 2..LIBRARY_BROWSE_PAGE_SIZE * 3)
    );
    assert_eq!(
      model.adapter.retained_item_count(),
      LIBRARY_BROWSE_PAGE_SIZE as usize
    );
    assert!(matches!(
      model.view(),
      LibraryBrowseView::Ready {
        ref visible_items,
        ..
      } if visible_items.iter().all(|slot| slot.item.is_none())
    ));
    assert_eq!(
      request(replacement).start_index,
      LIBRARY_BROWSE_PAGE_SIZE * 3
    );
    assert_eq!(second_page.start_index, LIBRARY_BROWSE_PAGE_SIZE * 2);
  }

  #[test]
  fn virtual_search_requests_keep_the_query_and_project_the_current_range() {
    const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10;
    let mut model = BrowseModel::default();
    let bootstrap = request(
      model
        .configure(BrowseSource::Search {
          session: session(),
          query: "blade runner".to_owned(),
        })
        .expect("search should configure"),
    );
    let initial_window = settle(&mut model, &bootstrap, TOTAL, LIBRARY_BROWSE_PAGE_SIZE);
    settle_all_requests(&mut model, initial_window, TOTAL);
    let next_window = model.load_next().expect("next search window should load");

    let search_requests = requests(&next_window);
    assert!(
      !search_requests.is_empty()
        && search_requests.iter().all(|request| matches!(
          &request.source,
          BrowseSource::Search { query, .. } if query == "blade runner"
        ))
    );
    settle_all_requests(&mut model, next_window, TOTAL);
    assert_eq!(
      model.display_range(),
      Some(LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2)
    );
  }

  #[test]
  fn reconfiguring_identical_virtual_search_preserves_window_navigation() {
    const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10;
    let mut model = BrowseModel::default();
    let source = BrowseSource::Search {
      session: session(),
      query: "arrival".to_owned(),
    };
    let bootstrap = model
      .configure(source.clone())
      .expect("search should configure");
    settle_all_requests(&mut model, bootstrap, TOTAL);
    for _ in 0..2 {
      let effects = model.load_next().expect("later window should load");
      settle_all_requests(&mut model, effects, TOTAL);
    }
    let later_range = LIBRARY_BROWSE_PAGE_SIZE * 2..LIBRARY_BROWSE_PAGE_SIZE * 3;
    assert_eq!(model.display_range(), Some(later_range.clone()));

    let effects = model
      .configure(source)
      .expect("identical search should remain configured");

    assert!(effects.is_empty());
    assert_eq!(model.display_range(), Some(later_range.clone()));
    assert_eq!(
      visible_indexes(&model),
      later_range.clone().collect::<Vec<_>>()
    );
    assert!(model.can_load_previous());
    assert!(model.can_load_more());

    let next = model.load_next().expect("next window should load");
    settle_all_requests(&mut model, next, TOTAL);
    assert_eq!(
      model.display_range(),
      Some(LIBRARY_BROWSE_PAGE_SIZE * 3..LIBRARY_BROWSE_PAGE_SIZE * 4)
    );

    let previous = model.load_previous().expect("previous window should load");
    settle_all_requests(&mut model, previous, TOTAL);
    assert_eq!(model.display_range(), Some(later_range.clone()));
    assert_eq!(visible_indexes(&model), later_range.collect::<Vec<_>>());
  }

  #[test]
  fn stale_settlement_after_session_reset_is_ignored() {
    let mut model = BrowseModel::default();
    let stale = request(
      model
        .configure(BrowseSource::Library {
          session: session(),
          shortcut: shortcut(),
        })
        .expect("first source should configure"),
    );
    model.reset();
    let mut gate = RequestGate::default();
    gate.disconnect();
    model
      .configure(BrowseSource::Library {
        session: gate.current_session(),
        shortcut: shortcut(),
      })
      .expect("second source should configure");

    settle(&mut model, &stale, 1, 1);

    assert!(matches!(model.view(), LibraryBrowseView::Loading));
  }

  #[test]
  fn library_preferences_are_part_of_request_identity_and_payload() {
    let mut model = BrowseModel::default();
    let source = BrowseSource::Library {
      session: session(),
      shortcut: shortcut(),
    };
    let first = request(
      model
        .configure(source.clone())
        .expect("default library should configure"),
    );
    let preferences = BrowsePreferences {
      sort: VideoLibrarySort::RecentlyAdded,
      sort_direction: VideoLibrarySortDirection::Descending,
      played_filter: VideoLibraryPlayedFilter::Unplayed,
      favorites_only: true,
    };
    let filtered = request(
      model
        .configure_with_preferences(source, preferences)
        .expect("filtered library should reconfigure"),
    );

    assert_ne!(first.source_id, filtered.source_id);
    assert!(matches!(
      filtered.preferences.sort,
      VideoLibrarySort::RecentlyAdded
    ));
    assert!(matches!(
      filtered.preferences.played_filter,
      VideoLibraryPlayedFilter::Unplayed
    ));
    assert!(filtered.preferences.favorites_only);
  }
}
