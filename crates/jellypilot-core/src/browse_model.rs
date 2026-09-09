//! Library/Search result-set lifecycle over the portable pagination reducer.
//!
//! [`BrowseModel`] owns query identity, retained data, refresh replacement and
//! delivery correlation. A suspended model can move into navigation history;
//! resuming it reissues unfinished work without reusing physical delivery IDs.
//! Runtime task handles, layout geometry and image demand remain in the shell.
//! Refresh retains a complete usable result set until the captured replacement
//! window is ready, so failure does not turn a display projection into a second
//! source of lifecycle state.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    LibraryBrowseAction, LibraryBrowseCommand, LibraryBrowseCore, LibraryBrowseCoreError,
    LibraryBrowseFailure, LibraryBrowseLoadToken, LibraryBrowseMode, LibraryBrowsePageOutcome,
    LibraryBrowseStatus, LIBRARY_BROWSE_PAGE_SIZE,
};
use jellypilot_media_server::{
    VideoLibraryItem, VideoLibraryPage, VideoLibraryPlayedFilter, VideoLibraryShortcut,
    VideoLibrarySort, VideoLibrarySortDirection, VideoSearchPage,
};

use crate::request_gate::SessionToken;

/// Complete identity of one active native browse result.
#[derive(Clone, Debug)]
pub enum BrowseSource {
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
    pub fn identity(&self) -> String {
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

/// Process-unique identity of one physical page delivery, independent of its query.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BrowseDeliveryToken(u64);

impl BrowseDeliveryToken {
    fn next() -> Result<Self, LibraryBrowseCoreError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map(Self)
        .map_err(|_| LibraryBrowseCoreError::SequenceExhausted)
    }
}

/// Environment work emitted by the display-free browse model.
#[derive(Clone, Debug)]
pub enum BrowseEffect {
    ResetViewport,
    RequestPage(BrowsePageRequest),
    CancelPage { token: BrowseDeliveryToken },
}

#[derive(Clone, Copy, Debug)]
pub struct BrowsePreferences {
    pub sort: VideoLibrarySort,
    pub sort_direction: VideoLibrarySortDirection,
    pub played_filter: VideoLibraryPlayedFilter,
    pub favorites_only: bool,
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
pub struct BrowsePageRequest {
    pub source_id: String,
    pub source: BrowseSource,
    pub token: BrowseDeliveryToken,
    pub start_index: u32,
    pub limit: u32,
    pub preferences: BrowsePreferences,
}

/// Provider-neutral payload returned to the browse reducer.
#[derive(Clone, Debug)]
pub struct BrowsePagePayload {
    pub start_index: u32,
    pub limit: u32,
    pub total_record_count: u32,
    pub has_more: bool,
    pub items: Vec<VideoLibraryItem>,
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
#[derive(Clone, Debug)]
pub struct BrowsePageSettlement {
    pub source_id: String,
    pub token: BrowseDeliveryToken,
    pub result: Result<BrowsePagePayload, String>,
}
/// One display position and its payload, if the corresponding page is loaded.
#[derive(Clone, Debug)]
pub struct LibraryItemSlot {
    pub item: Option<VideoLibraryItem>,
}

/// Display-free projection of the portable Library Browser state.
#[derive(Clone, Debug)]
pub enum LibraryBrowseView {
    Inactive,
    Loading,
    Empty,
    Failed {
        message: String,
        retryable: bool,
        retry_busy: bool,
    },
    Ready {
        visible_items: Vec<LibraryItemSlot>,
        visible_start: u32,
        mode: LibraryBrowseMode,
        total_record_count: u32,
        is_fetching_more: bool,
        load_more_failure: Option<LibraryBrowseFailure>,
        retry_busy: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingPage {
    start_index: u32,
    limit: u32,
    delivery: Option<BrowseDeliveryToken>,
}

/// Browse lifecycle authority. Result sets are replaced atomically on refresh.
#[derive(Debug, Default)]
pub struct BrowseModel {
    committed: BrowseResultSet,
    refresh: Option<BrowseRefresh>,
    refresh_failure: Option<LibraryBrowseFailure>,
    suspended: bool,
}

#[derive(Debug)]
struct BrowseRefresh {
    replacement: BrowseResultSet,
    required_range: Range<u32>,
}

impl BrowseModel {
    /// Discards all data and delivery identities. The adapter owns task cancellation.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn configure(
        &mut self,
        source: BrowseSource,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        self.configure_with_preferences(source, BrowsePreferences::default())
    }

    pub fn configure_with_preferences(
        &mut self,
        source: BrowseSource,
        preferences: BrowsePreferences,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        let identity = format!(
            "{}:preferences:{}",
            source.identity(),
            preferences.identity()
        );
        if self.identity() == Some(identity.as_str()) {
            return Ok(Vec::new());
        }
        let mut next = BrowseResultSet {
            suspended: self.suspended,
            ..BrowseResultSet::default()
        };
        let loads = next.configure_with_preferences(source, preferences)?;
        let mut effects = self.committed.suspend();
        if let Some(refresh) = &mut self.refresh {
            effects.extend(refresh.replacement.suspend());
        }
        self.committed = next;
        self.refresh = None;
        self.refresh_failure = None;
        effects.extend(loads);
        Ok(effects)
    }

    /// Logical query identity; unchanged by physical delivery or refresh cycles.
    #[must_use]
    pub fn identity(&self) -> Option<&str> {
        self.committed.source_id.as_deref()
    }

    /// Pauses physical work without discarding data or unfinished paging intent.
    pub fn suspend(&mut self) -> Vec<BrowseEffect> {
        self.suspended = true;
        let mut effects = self.committed.suspend();
        if let Some(refresh) = &mut self.refresh {
            effects.extend(refresh.replacement.suspend());
        }
        effects
    }

    /// Reissues unfinished work with fresh delivery identities, preserving the viewport.
    pub fn resume(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        self.suspended = false;
        if let Some(refresh) = &mut self.refresh {
            refresh.replacement.resume()
        } else {
            self.committed.resume()
        }
    }

    /// Refreshes the stored query, retaining the complete usable result until replacement.
    pub fn refresh(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        if self.refresh.is_some() {
            return Ok(Vec::new());
        }
        let Some(source) = self.committed.source.clone() else {
            return Ok(Vec::new());
        };
        let required_range = self
            .committed
            .virtual_window
            .clone()
            .unwrap_or(0..LIBRARY_BROWSE_PAGE_SIZE);
        let mut replacement = BrowseResultSet {
            suspended: self.suspended,
            ..BrowseResultSet::default()
        };
        let loads = replacement.configure_with_preferences(source, self.committed.preferences)?;
        let mut effects = self.committed.suspend();
        effects.extend(
            loads
                .into_iter()
                .filter(|effect| !matches!(effect, BrowseEffect::ResetViewport)),
        );
        self.refresh = Some(BrowseRefresh {
            replacement,
            required_range,
        });
        self.refresh_failure = None;
        Ok(effects)
    }

    #[must_use]
    pub fn is_refreshing(&self) -> bool {
        self.refresh.is_some()
    }

    #[must_use]
    pub fn refresh_failure(&self) -> Option<&LibraryBrowseFailure> {
        self.refresh_failure.as_ref()
    }

    pub fn retry(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        if self
            .refresh_failure
            .as_ref()
            .is_some_and(|failure| failure.retryable)
        {
            self.refresh()
        } else if self.refresh.is_some() || self.refresh_failure.is_some() {
            Ok(Vec::new())
        } else {
            self.committed.retry()
        }
    }

    #[must_use]
    pub fn is_current_settlement(&self, settlement: &BrowsePageSettlement) -> bool {
        self.refresh.as_ref().map_or_else(
            || self.committed.is_current_settlement(settlement),
            |refresh| refresh.replacement.is_current_settlement(settlement),
        )
    }

    pub fn settle(
        &mut self,
        settlement: BrowsePageSettlement,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        if !self.is_current_settlement(&settlement) {
            return Ok(Vec::new());
        }
        let Some(refresh) = &mut self.refresh else {
            return self.committed.settle(settlement);
        };
        let settled_start = refresh
            .replacement
            .pending
            .values()
            .find(|page| page.delivery == Some(settlement.token))
            .map(|page| page.start_index);
        let transport_failure = settlement.result.as_ref().err().cloned();
        let mut effects = refresh.replacement.settle(settlement)?;
        let snapshot = refresh.replacement.core.snapshot();
        let total = match snapshot.status {
            LibraryBrowseStatus::Ready {
                total_record_count, ..
            } => Some(total_record_count),
            LibraryBrowseStatus::Empty { .. } => Some(0),
            LibraryBrowseStatus::InitialFailure { .. }
            | LibraryBrowseStatus::Inactive
            | LibraryBrowseStatus::Loading => None,
        };
        let required = total.map_or_else(
            || refresh.required_range.clone(),
            |total| replacement_display_range(refresh.required_range.clone(), total),
        );
        let required_failed = settled_start.is_some_and(|start| {
            (start == 0
                || (start < required.end
                    && start.saturating_add(LIBRARY_BROWSE_PAGE_SIZE) > required.start))
                && !refresh.replacement.pages.contains_key(&start)
        });
        let incomplete_page = required.clone().any(|index| {
            let start = index / LIBRARY_BROWSE_PAGE_SIZE * LIBRARY_BROWSE_PAGE_SIZE;
            refresh.replacement.pages.get(&start).is_some_and(|items| {
                usize::try_from(index - start).is_ok_and(|offset| offset >= items.len())
            })
        });
        let failure = if required_failed {
            Some(match transport_failure {
                Some(message) => LibraryBrowseFailure {
                    message,
                    retryable: true,
                },
                None => LibraryBrowseFailure {
                    message: "Media server returned invalid refresh page metadata.".to_owned(),
                    retryable: false,
                },
            })
        } else {
            incomplete_page.then(|| LibraryBrowseFailure {
                message: "Media server returned an incomplete refresh display window.".to_owned(),
                retryable: false,
            })
        };
        if let Some(failure) = failure {
            // Newly scheduled replacement work must never escape after rollback.
            effects.retain(|effect| !matches!(effect, BrowseEffect::RequestPage(_)));
            effects.extend(refresh.replacement.suspend());
            let usable = matches!(
                self.committed.core.snapshot().status,
                LibraryBrowseStatus::Ready { .. } | LibraryBrowseStatus::Empty { .. }
            );
            if let Some(refresh) = self.refresh.take() {
                if !usable {
                    self.committed = refresh.replacement;
                }
            }
            self.refresh_failure = Some(failure);
            if !self.suspended {
                effects.extend(self.committed.resume()?);
            }
            return Ok(effects);
        }
        let Some(total) = total else {
            return Ok(effects);
        };
        effects.extend(
            refresh
                .replacement
                .set_display_range(required.clone(), total)?,
        );
        let complete = required.clone().all(|index| {
            let start = index / LIBRARY_BROWSE_PAGE_SIZE * LIBRARY_BROWSE_PAGE_SIZE;
            refresh.replacement.pages.get(&start).is_some_and(|items| {
                usize::try_from(index - start).is_ok_and(|offset| offset < items.len())
            })
        });
        if complete {
            let latest = self.committed.virtual_window.clone().unwrap_or(required);
            let Some(refresh) = self.refresh.take() else {
                return Ok(effects);
            };
            self.committed = refresh.replacement;
            effects.extend(
                self.committed
                    .set_display_range(replacement_display_range(latest, total), total)?,
            );
        }
        Ok(effects)
    }

    #[must_use]
    pub fn view(&self) -> LibraryBrowseView {
        self.committed.view()
    }

    pub fn set_display_range(
        &mut self,
        range: Range<u32>,
        total_record_count: u32,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        self.committed.set_display_range(range, total_record_count)
    }

    #[must_use]
    pub fn peek_display_range(&self) -> Option<Range<u32>> {
        self.committed.peek_display_range()
    }

    #[must_use]
    pub fn display_range(&self) -> Option<Range<u32>> {
        self.committed.display_range()
    }

    #[cfg(test)]
    fn retained_page_count(&self) -> usize {
        self.committed.retained_page_count()
    }

    #[cfg(test)]
    fn retained_item_count(&self) -> usize {
        self.committed.retained_item_count()
    }
}

fn replacement_display_range(range: Range<u32>, total: u32) -> Range<u32> {
    if range.is_empty() || range.start >= total {
        virtual_window_range(range.start, total)
    } else {
        clamped_display_range(range, total)
    }
}

/// One complete result set, including retained payloads and unfinished paging intent.
#[derive(Debug, Default)]
struct BrowseResultSet {
    core: LibraryBrowseCore,
    pages: BTreeMap<u32, Vec<VideoLibraryItem>>,
    pending: BTreeMap<LibraryBrowseLoadToken, PendingPage>,
    source: Option<BrowseSource>,
    source_id: Option<String>,
    virtual_window_start: u32,
    /// The exact virtual display range currently projected by the reducer.
    ///
    /// `None` means that no virtual window has been established yet (for
    /// example, before the bootstrap page settles).
    virtual_window: Option<Range<u32>>,
    preferences: BrowsePreferences,
    suspended: bool,
}

impl BrowseResultSet {
    pub fn configure_with_preferences(
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
        // The native shell accumulates loaded pages across window moves; the
        // core default (eviction) remains the wasm/web contract.
        self.core.set_retain_loaded_pages(true);
        let update = self.core.dispatch(LibraryBrowseAction::Configure {
            source_id: source_id.clone(),
            enabled: true,
        })?;
        if source_changed {
            self.virtual_window_start = 0;
            self.virtual_window = None;
        }
        self.source = Some(source);
        self.source_id = Some(source_id);
        self.preferences = preferences;
        self.apply_commands(update.commands)
    }

    pub fn retry(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        self.dispatch_action(LibraryBrowseAction::Retry)
    }

    pub fn settle(
        &mut self,
        settlement: BrowsePageSettlement,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        if !self.is_current_settlement(&settlement) {
            return Ok(Vec::new());
        }

        let Some(token) = self.pending.iter().find_map(|(token, pending)| {
            (pending.delivery == Some(settlement.token)).then_some(*token)
        }) else {
            return Ok(Vec::new());
        };
        let mut effects = match settlement.result {
            Ok(page) => self.settle_loaded(token, page)?,
            Err(message) => self.settle_failed(token, message)?,
        };
        if let LibraryBrowseStatus::Ready {
            mode: LibraryBrowseMode::Virtual,
            total_record_count,
            ..
        } = self.core.snapshot().status
        {
            effects.extend(self.update_virtual_window(total_record_count)?);
        }
        Ok(effects)
    }
    /// Returns whether a settlement belongs to a currently pending request.
    #[must_use]
    pub fn is_current_settlement(&self, settlement: &BrowsePageSettlement) -> bool {
        self.source_id.as_deref() == Some(settlement.source_id.as_str())
            && self
                .pending
                .values()
                .any(|pending| pending.delivery == Some(settlement.token))
    }

    #[must_use]
    pub fn view(&self) -> LibraryBrowseView {
        let snapshot = self.core.snapshot();
        match snapshot.status {
            LibraryBrowseStatus::Inactive => LibraryBrowseView::Inactive,
            LibraryBrowseStatus::Loading => LibraryBrowseView::Loading,
            LibraryBrowseStatus::Empty { .. } => LibraryBrowseView::Empty,
            LibraryBrowseStatus::InitialFailure {
                failure,
                retry_busy,
            } => LibraryBrowseView::Failed {
                message: failure.message,
                retryable: failure.retryable,
                retry_busy,
            },
            LibraryBrowseStatus::Ready {
                mode,
                total_record_count,
                is_fetching_more,
                load_more_failure,
                retry_busy,
                ..
            } => {
                let visible_start = snapshot.slots.first().map_or(0, |slot| slot.display_index);
                LibraryBrowseView::Ready {
                    visible_items: snapshot
                        .slots
                        .iter()
                        .map(|slot| {
                            let item =
                                usize::try_from(slot.index_within_page)
                                    .ok()
                                    .and_then(|index| {
                                        self.pages.get(&slot.page_start)?.get(index).cloned()
                                    });
                            LibraryItemSlot { item }
                        })
                        .collect(),
                    visible_start,
                    mode,
                    total_record_count,
                    is_fetching_more,
                    load_more_failure,
                    retry_busy,
                }
            }
        }
    }

    /// Changes the sparse virtual display window without resetting the
    /// viewport.
    ///
    /// Scroll-driven window updates must preserve the caller's scroll
    /// position; `ResetViewport` remains reserved for configure/sort resets.
    pub fn set_display_range(
        &mut self,
        range: Range<u32>,
        total_record_count: u32,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        let range = clamped_display_range(range, total_record_count);
        if self.virtual_window.as_ref() == Some(&range) {
            return Ok(Vec::new());
        }

        let update = self.core.dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: range.clone().collect(),
        })?;
        self.virtual_window_start = range.start;
        self.virtual_window = Some(range);
        self.apply_commands(update.commands)
    }

    /// Returns the projected virtual display range without cloning any items.
    ///
    /// Matches [`Self::display_range`] but is built from core metadata only
    /// (slot bounds, mode, and total record count), so callers that need the
    /// range on every scroll tick avoid cloning the visible page payloads.
    #[must_use]
    pub fn peek_display_range(&self) -> Option<Range<u32>> {
        let snapshot = self.core.snapshot();
        let LibraryBrowseStatus::Ready {
            mode: LibraryBrowseMode::Virtual,
            total_record_count,
            ..
        } = snapshot.status
        else {
            return None;
        };
        match (snapshot.slots.first(), snapshot.slots.last()) {
            (Some(first), Some(last)) => Some(first.display_index..last.display_index + 1),
            _ => Some(self.virtual_window.clone().map_or_else(
                || virtual_window_range(self.virtual_window_start, total_record_count),
                |range| clamped_display_range(range, total_record_count),
            )),
        }
    }

    #[must_use]
    pub fn display_range(&self) -> Option<Range<u32>> {
        match self.view() {
            LibraryBrowseView::Ready {
                mode: LibraryBrowseMode::Virtual,
                total_record_count,
                ..
            } => Some(self.virtual_window.clone().map_or_else(
                || virtual_window_range(self.virtual_window_start, total_record_count),
                |range| clamped_display_range(range, total_record_count),
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

    #[cfg(test)]
    fn retained_page_count(&self) -> usize {
        self.pages.len()
    }

    #[cfg(test)]
    fn retained_item_count(&self) -> usize {
        self.pages.values().map(Vec::len).sum()
    }

    fn settle_loaded(
        &mut self,
        token: LibraryBrowseLoadToken,
        page: BrowsePagePayload,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        let pending = self.pending.get(&token).copied();
        let item_count = u32::try_from(page.items.len()).unwrap_or(u32::MAX);
        let update = self.core.dispatch(LibraryBrowseAction::PageSettled {
            token,
            outcome: LibraryBrowsePageOutcome::Loaded {
                start_index: page.start_index,
                limit: page.limit,
                total_record_count: page.total_record_count,
                item_count,
                has_more: page.has_more,
            },
        })?;
        self.pending.remove(&token);
        let page_is_valid = !update.commands.iter().any(|command| {
            matches!(
              command,
              LibraryBrowseCommand::ReleasePages { page_starts }
                if page_starts.contains(&page.start_index)
            )
        });
        if page_is_valid
            && pending.is_some_and(|pending| {
                pending.start_index == page.start_index && pending.limit == page.limit
            })
        {
            self.pages.insert(page.start_index, page.items);
        }
        self.apply_commands(update.commands)
    }

    fn settle_failed(
        &mut self,
        token: LibraryBrowseLoadToken,
        message: String,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        let update = self.core.dispatch(LibraryBrowseAction::PageSettled {
            token,
            outcome: LibraryBrowsePageOutcome::Failed {
                failure: crate::LibraryBrowseFailure {
                    message,
                    retryable: true,
                },
            },
        })?;
        self.pending.remove(&token);
        self.apply_commands(update.commands)
    }

    fn update_virtual_window(
        &mut self,
        total_record_count: u32,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        let range = self
            .virtual_window
            .clone()
            .unwrap_or_else(|| virtual_window_range(self.virtual_window_start, total_record_count));
        self.set_display_range(range, total_record_count)
    }

    fn dispatch_action(
        &mut self,
        action: LibraryBrowseAction,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        let update = self.core.dispatch(action)?;
        self.apply_commands(update.commands)
    }

    fn apply_commands(
        &mut self,
        commands: Vec<LibraryBrowseCommand>,
    ) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        let mut effects = Vec::new();
        for command in commands {
            match command {
                LibraryBrowseCommand::ResetViewport => effects.push(BrowseEffect::ResetViewport),
                LibraryBrowseCommand::LoadPage {
                    token,
                    start_index,
                    limit,
                    ..
                } => {
                    self.pending.insert(
                        token,
                        PendingPage {
                            start_index,
                            limit,
                            delivery: None,
                        },
                    );
                }
                LibraryBrowseCommand::CancelLoad { token } => {
                    if let Some(delivery) =
                        self.pending.remove(&token).and_then(|page| page.delivery)
                    {
                        effects.push(BrowseEffect::CancelPage { token: delivery });
                    }
                }
                LibraryBrowseCommand::ReleasePages { page_starts } => {
                    for page_start in page_starts {
                        self.pages.remove(&page_start);
                    }
                }
            }
        }
        effects.extend(self.issue_pending()?);
        Ok(effects)
    }

    fn issue_pending(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        if self.suspended {
            return Ok(Vec::new());
        }
        let (Some(source), Some(source_id)) = (&self.source, &self.source_id) else {
            return Ok(Vec::new());
        };
        let mut effects = Vec::new();
        for page in self
            .pending
            .values_mut()
            .filter(|page| page.delivery.is_none())
        {
            let token = BrowseDeliveryToken::next()?;
            page.delivery = Some(token);
            effects.push(BrowseEffect::RequestPage(BrowsePageRequest {
                source_id: source_id.clone(),
                source: source.clone(),
                token,
                start_index: page.start_index,
                limit: page.limit,
                preferences: self.preferences,
            }));
        }
        Ok(effects)
    }

    fn suspend(&mut self) -> Vec<BrowseEffect> {
        self.suspended = true;
        self.pending
            .values_mut()
            .filter_map(|page| {
                page.delivery
                    .take()
                    .map(|token| BrowseEffect::CancelPage { token })
            })
            .collect()
    }

    fn resume(&mut self) -> Result<Vec<BrowseEffect>, LibraryBrowseCoreError> {
        self.suspended = false;
        self.issue_pending()
    }
}

fn clamped_display_range(range: Range<u32>, total_record_count: u32) -> Range<u32> {
    let start = range.start.min(total_record_count);
    let end = range.end.min(total_record_count).max(start);
    start..end
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
                premiere_date: None,
                id: format!("item-{index}"),
                name: format!("Item {index}"),
                item_type: "Movie".to_owned(),
                production_year: None,
                runtime_seconds: None,
                played: false,
                favorite: false,
                artwork_image_id: None,
                backdrop_image_id: None,
                logo_image_id: None,
                series_poster_image_id: None,
                season_number: None,
                episode_thumb_image_id: None,
                series_thumb_image_id: None,
                series_backdrop_image_id: None,
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
            .collect()
    }

    fn request(effects: Vec<BrowseEffect>) -> BrowsePageRequest {
        effects
            .into_iter()
            .find_map(|effect| match effect {
                BrowseEffect::RequestPage(request) => Some(request),
                BrowseEffect::ResetViewport | BrowseEffect::CancelPage { .. } => None,
            })
            .expect("page request should be emitted")
    }

    fn requests(effects: &[BrowseEffect]) -> Vec<BrowsePageRequest> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                BrowseEffect::RequestPage(request) => Some(request.clone()),
                BrowseEffect::ResetViewport | BrowseEffect::CancelPage { .. } => None,
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
        model
            .committed
            .core
            .snapshot()
            .slots
            .iter()
            .map(|slot| slot.display_index)
            .collect()
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
        let lookahead = settle(&mut model, &first, 30, 24);
        let next = request(lookahead);
        assert!(matches!(
          next.source,
          BrowseSource::Search { ref query, .. } if query == "arrival"
        ));
        settle(&mut model, &next, 30, 6);

        let advance = model
            .set_display_range(24..30, 30)
            .expect("window advance should succeed");
        assert!(advance
            .iter()
            .all(|effect| !matches!(effect, BrowseEffect::ResetViewport)));
        assert_eq!(model.display_range(), Some(24..30));
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
        let lookahead = settle(&mut model, &first, 30, 24);
        let failed = request(lookahead);
        model
            .settle(BrowsePageSettlement {
                source_id: failed.source_id.clone(),
                token: failed.token,
                result: Err("temporary failure".to_owned()),
            })
            .expect("failure should settle");

        let _ = model
            .set_display_range(24..30, 30)
            .expect("window advance should succeed");
        assert!(matches!(
          model.view(),
          LibraryBrowseView::Ready {
            ref load_more_failure,
            ..
          } if load_more_failure.as_ref().map(|failure| failure.message.as_str())
              == Some("temporary failure")
            && load_more_failure.as_ref().is_some_and(|failure| failure.retryable)
        ));
        let retry = request(model.retry().expect("retry should schedule"));
        assert_eq!(retry.start_index, 24);
        assert!(matches!(
          retry.source,
          BrowseSource::Search { ref query, .. } if query == "dune"
        ));
    }

    #[test]
    fn malformed_continuation_preserves_non_retryable_failure_metadata() {
        let mut model = BrowseModel::default();
        let first = request(
            model
                .configure(BrowseSource::Search {
                    session: session(),
                    query: "dune".to_owned(),
                })
                .expect("search should configure"),
        );
        let lookahead = settle(&mut model, &first, 30, LIBRARY_BROWSE_PAGE_SIZE);
        let next = request(lookahead);

        model
            .settle(BrowsePageSettlement {
                source_id: next.source_id,
                token: next.token,
                result: Ok(BrowsePagePayload {
                    start_index: next.start_index.saturating_add(1),
                    limit: next.limit,
                    total_record_count: 30,
                    has_more: false,
                    items: items(next.start_index, 6),
                }),
            })
            .expect("malformed continuation should settle as a failure");

        let _ = model
            .set_display_range(24..30, 30)
            .expect("window advance should succeed");
        assert!(matches!(
            model.view(),
            LibraryBrowseView::Ready {
                load_more_failure: Some(LibraryBrowseFailure {
                    retryable: false,
                    ..
                }),
                ..
            }
        ));
        assert!(model
            .retry()
            .expect("non-retryable failure should be ignored")
            .is_empty());
    }

    #[test]
    fn virtual_window_accumulates_loaded_pages_without_refetching() {
        const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 80;
        const WINDOWS: u32 = 50;
        let mut model = BrowseModel::default();
        let bootstrap = model
            .configure(BrowseSource::Library {
                session: session(),
                shortcut: shortcut(),
            })
            .expect("library should configure");
        settle_all_requests(&mut model, bootstrap, TOTAL);

        let mut max_visible_slots = visible_indexes(&model).len();
        for window in 1..WINDOWS {
            let start = window * LIBRARY_BROWSE_PAGE_SIZE;
            let effects = model
                .set_display_range(start..start + LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
                .expect("next window should load");
            settle_all_requests(&mut model, effects, TOTAL);
            max_visible_slots = max_visible_slots.max(visible_indexes(&model).len());
        }

        assert_eq!(max_visible_slots, LIBRARY_BROWSE_PAGE_SIZE as usize);
        // Bootstrap loads page 0 and its lookahead; every window then adds its
        // own lookahead page, so pages 0..=WINDOWS stay resident.
        assert_eq!(model.retained_page_count(), (WINDOWS + 1) as usize);
        assert_eq!(
            model.retained_item_count(),
            (LIBRARY_BROWSE_PAGE_SIZE * (WINDOWS + 1)) as usize
        );

        for window in (0..WINDOWS).rev() {
            let start = window * LIBRARY_BROWSE_PAGE_SIZE;
            let effects = model
                .set_display_range(start..start + LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
                .expect("previous window should be retained");
            assert!(
                effects.is_empty(),
                "retained window {window} should not emit effects"
            );
        }
    }

    #[test]
    fn moving_the_window_back_reuses_retained_pages_without_refetching() {
        const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10;
        let mut model = BrowseModel::default();
        let bootstrap = model
            .configure(BrowseSource::Library {
                session: session(),
                shortcut: shortcut(),
            })
            .expect("library should configure");
        settle_all_requests(&mut model, bootstrap, TOTAL);
        let first_advance = model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2,
                TOTAL,
            )
            .expect("first next window should load");
        settle_all_requests(&mut model, first_advance, TOTAL);
        let second_advance = model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE * 2..LIBRARY_BROWSE_PAGE_SIZE * 3,
                TOTAL,
            )
            .expect("second next window should load");
        settle_all_requests(&mut model, second_advance, TOTAL);

        let previous = model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2,
                TOTAL,
            )
            .expect("previous window should already be retained");
        assert!(
            previous.is_empty(),
            "retained window should not emit requests or viewport resets"
        );

        assert_eq!(
            model.display_range(),
            Some(LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2)
        );
        assert_eq!(
            visible_indexes(&model),
            (LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2).collect::<Vec<_>>()
        );
        assert!(matches!(
          model.view(),
          LibraryBrowseView::Ready {
            ref visible_items,
            ..
          } if visible_items.iter().all(|slot| slot.item.is_some())
        ));
    }

    #[test]
    fn peek_display_range_matches_display_range_across_window_positions() {
        const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10 + 5;
        let mut model = BrowseModel::default();
        assert_eq!(model.peek_display_range(), None);
        let bootstrap = model
            .configure(BrowseSource::Library {
                session: session(),
                shortcut: shortcut(),
            })
            .expect("library should configure");
        assert_eq!(model.peek_display_range(), model.display_range());
        settle_all_requests(&mut model, bootstrap, TOTAL);
        assert_eq!(model.peek_display_range(), model.display_range());

        for window in 1..10 {
            let start = window * LIBRARY_BROWSE_PAGE_SIZE;
            let effects = model
                .set_display_range(start..start + LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
                .expect("window advance should succeed");
            assert_eq!(model.peek_display_range(), model.display_range());
            settle_all_requests(&mut model, effects, TOTAL);
            assert_eq!(
                model.peek_display_range(),
                Some(start..(start + LIBRARY_BROWSE_PAGE_SIZE).min(TOTAL))
            );
            assert_eq!(model.peek_display_range(), model.display_range());
        }

        let tail = model
            .set_display_range(7..TOTAL + LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
            .expect("unaligned tail window should clamp");
        settle_all_requests(&mut model, tail, TOTAL);
        assert_eq!(model.peek_display_range(), Some(7..TOTAL));
        assert_eq!(model.peek_display_range(), model.display_range());
    }

    #[test]
    fn final_display_range_clamps_to_total_and_is_stable() {
        const TOTAL: u32 = 101;
        let mut model = BrowseModel::default();
        let bootstrap = model
            .configure(BrowseSource::Library {
                session: session(),
                shortcut: shortcut(),
            })
            .expect("library should configure");
        settle_all_requests(&mut model, bootstrap, TOTAL);
        let effects = model
            .set_display_range(96..101, TOTAL)
            .expect("final window should load");
        settle_all_requests(&mut model, effects, TOTAL);

        assert_eq!(model.display_range(), Some(96..101));
        assert_eq!(visible_indexes(&model), (96..101).collect::<Vec<_>>());
        assert!(model
            .set_display_range(96..101, TOTAL)
            .expect("unchanged window should remain stable")
            .is_empty());
    }

    #[test]
    fn settlement_for_a_window_that_moved_on_is_retained_without_replacing_the_view() {
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
        let second_page = request(
            model
                .set_display_range(
                    LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2,
                    TOTAL,
                )
                .expect("first next window should load"),
        );
        model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE * 2..LIBRARY_BROWSE_PAGE_SIZE * 3,
                TOTAL,
            )
            .expect("window should move past the first prefetch");

        let replacement = settle(&mut model, &first_prefetch, TOTAL, LIBRARY_BROWSE_PAGE_SIZE);

        assert_eq!(
            model.display_range(),
            Some(LIBRARY_BROWSE_PAGE_SIZE * 2..LIBRARY_BROWSE_PAGE_SIZE * 3)
        );
        // Pages 0 and the late-settling prefetch stay resident under retention.
        assert_eq!(model.retained_page_count(), 2);
        assert_eq!(
            model.retained_item_count(),
            (LIBRARY_BROWSE_PAGE_SIZE * 2) as usize
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

        let back = model
            .set_display_range(0..LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
            .expect("retained prefetch should still serve the earlier window");
        assert!(back.is_empty());
        assert!(matches!(
          model.view(),
          LibraryBrowseView::Ready {
            ref visible_items,
            ..
          } if visible_items.iter().all(|slot| slot.item.is_some())
        ));
    }

    #[test]
    fn failed_page_leaving_the_window_is_released_and_refetched_on_return() {
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
        let lookahead = request(settle(
            &mut model,
            &bootstrap,
            TOTAL,
            LIBRARY_BROWSE_PAGE_SIZE,
        ));
        model
            .settle(BrowsePageSettlement {
                source_id: lookahead.source_id.clone(),
                token: lookahead.token,
                result: Err("flaky page".to_owned()),
            })
            .expect("failed page should settle");
        let advance = model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE * 2..LIBRARY_BROWSE_PAGE_SIZE * 3,
                TOTAL,
            )
            .expect("window should move past the failed page");
        settle_all_requests(&mut model, advance, TOTAL);

        let back = model
            .set_display_range(0..LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
            .expect("window should return to the released page");
        let refetch = request(back);
        assert_eq!(refetch.start_index, LIBRARY_BROWSE_PAGE_SIZE);

        settle_all_requests(&mut model, vec![BrowseEffect::RequestPage(refetch)], TOTAL);
        let settled = model
            .set_display_range(0..LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
            .expect("refetched page should now be retained");
        assert!(settled.is_empty());
        assert!(matches!(
          model.view(),
          LibraryBrowseView::Ready {
            ref visible_items,
            ..
          } if visible_items.iter().all(|slot| slot.item.is_some())
        ));
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
        let next_window = model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE..LIBRARY_BROWSE_PAGE_SIZE * 2,
                TOTAL,
            )
            .expect("next search window should load");

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
        for window in 1..=2_u32 {
            let start = window * LIBRARY_BROWSE_PAGE_SIZE;
            let effects = model
                .set_display_range(start..start + LIBRARY_BROWSE_PAGE_SIZE, TOTAL)
                .expect("later window should load");
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

        let next = model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE * 3..LIBRARY_BROWSE_PAGE_SIZE * 4,
                TOTAL,
            )
            .expect("next window should load");
        settle_all_requests(&mut model, next, TOTAL);
        assert_eq!(
            model.display_range(),
            Some(LIBRARY_BROWSE_PAGE_SIZE * 3..LIBRARY_BROWSE_PAGE_SIZE * 4)
        );

        let previous = model
            .set_display_range(later_range.clone(), TOTAL)
            .expect("previous window should load");
        settle_all_requests(&mut model, previous, TOTAL);
        assert_eq!(model.display_range(), Some(later_range.clone()));
        assert_eq!(visible_indexes(&model), later_range.collect::<Vec<_>>());
    }

    #[test]
    fn stale_settlement_after_same_session_route_reset_is_ignored() {
        let mut model = BrowseModel::default();
        let source = BrowseSource::Library {
            session: session(),
            shortcut: shortcut(),
        };
        let stale = request(
            model
                .configure(source.clone())
                .expect("first source should configure"),
        );
        model.reset();
        let current = request(model.configure(source).expect("same source should reopen"));

        settle(&mut model, &stale, 1, 1);

        assert!(model.is_current_settlement(&BrowsePageSettlement {
            source_id: current.source_id,
            token: current.token,
            result: Err(String::new()),
        }));
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

    #[test]
    fn configure_requests_bootstrap_page_through_portable_core() {
        let mut model = BrowseModel::default();

        let effects = model
            .configure(BrowseSource::Library {
                session: session(),
                shortcut: shortcut(),
            })
            .expect("source should configure");

        assert!(matches!(effects.first(), Some(BrowseEffect::ResetViewport)));
        assert_eq!(request(effects).start_index, 0);
        assert!(matches!(model.view(), LibraryBrowseView::Loading));
    }

    #[test]
    fn loaded_page_is_projected_with_caller_owned_payloads() {
        let mut model = BrowseModel::default();
        let page_request = request(
            model
                .configure(BrowseSource::Library {
                    session: session(),
                    shortcut: shortcut(),
                })
                .expect("source should configure"),
        );

        settle(&mut model, &page_request, 30, 24);

        assert!(matches!(
          model.view(),
          LibraryBrowseView::Ready {
            ref visible_items,
            total_record_count: 30,
            ..
          } if visible_items.len() == 24
            && visible_items.iter().all(|slot| slot.item.is_some())
        ));
    }

    #[test]
    fn virtual_window_loads_and_projects_requested_slots() {
        const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10;
        let mut model = BrowseModel::default();
        let bootstrap = request(
            model
                .configure(BrowseSource::Library {
                    session: session(),
                    shortcut: shortcut(),
                })
                .expect("source should configure"),
        );
        settle(&mut model, &bootstrap, TOTAL, LIBRARY_BROWSE_PAGE_SIZE);

        let window = model
            .set_display_range(
                LIBRARY_BROWSE_PAGE_SIZE * 2..LIBRARY_BROWSE_PAGE_SIZE * 3,
                TOTAL,
            )
            .expect("virtual window should dispatch");
        let page_request = request(window);
        assert_eq!(page_request.start_index, LIBRARY_BROWSE_PAGE_SIZE * 2);
        settle(&mut model, &page_request, TOTAL, LIBRARY_BROWSE_PAGE_SIZE);

        assert!(matches!(
          model.view(),
          LibraryBrowseView::Ready {
            ref visible_items,
            mode: LibraryBrowseMode::Virtual,
            ..
          } if visible_items
            .iter()
            .filter_map(|slot| slot.item.as_ref().map(|item| item.id.as_str()))
            .eq((48..72).map(|index| format!("item-{index}")).collect::<Vec<_>>())
        ));
    }

    #[test]
    fn failed_bootstrap_can_be_retried() {
        let mut model = BrowseModel::default();
        let bootstrap = request(
            model
                .configure(BrowseSource::Library {
                    session: session(),
                    shortcut: shortcut(),
                })
                .expect("source should configure"),
        );
        model
            .settle(BrowsePageSettlement {
                source_id: bootstrap.source_id.clone(),
                token: bootstrap.token,
                result: Err("temporary source failure".to_owned()),
            })
            .expect("failure should settle");

        assert!(matches!(
            model.view(),
            LibraryBrowseView::Failed {
                retryable: true,
                retry_busy: false,
                ..
            }
        ));
        assert_eq!(
            request(model.retry().expect("retry should dispatch")).start_index,
            0
        );
    }

    #[test]
    fn stale_completion_does_not_replace_active_source() {
        let mut model = BrowseModel::default();
        let stale = request(
            model
                .configure(BrowseSource::Library {
                    session: session(),
                    shortcut: shortcut(),
                })
                .expect("first source should configure"),
        );
        model
            .configure(BrowseSource::Search {
                session: session(),
                query: "shows".to_owned(),
            })
            .expect("second source should configure");

        settle(&mut model, &stale, 1, 1);

        assert!(matches!(model.view(), LibraryBrowseView::Loading));
        assert!(model.committed.pages.is_empty());
    }

    #[test]
    fn malformed_page_metadata_releases_unusable_payload() {
        let mut model = BrowseModel::default();
        let bootstrap = request(
            model
                .configure(BrowseSource::Library {
                    session: session(),
                    shortcut: shortcut(),
                })
                .expect("source should configure"),
        );
        model
            .settle(BrowsePageSettlement {
                source_id: bootstrap.source_id.clone(),
                token: bootstrap.token,
                result: Ok(BrowsePagePayload {
                    start_index: bootstrap.start_index,
                    limit: bootstrap.limit - 1,
                    total_record_count: 1,
                    has_more: false,
                    items: items(0, 1),
                }),
            })
            .expect("malformed completion should become a retained failure");

        assert!(model.committed.pages.is_empty());
        assert!(matches!(
            model.view(),
            LibraryBrowseView::Failed {
                retryable: false,
                ..
            }
        ));
    }

    #[test]
    fn reconfigure_emits_the_cancelled_page_token() {
        let mut model = BrowseModel::default();
        let pending = request(
            model
                .configure(BrowseSource::Library {
                    session: session(),
                    shortcut: shortcut(),
                })
                .expect("first source should configure"),
        );

        let effects = model
            .configure(BrowseSource::Search {
                session: session(),
                query: "replacement".to_owned(),
            })
            .expect("replacement source should configure");

        assert!(effects.iter().any(
            |effect| matches!(effect, BrowseEffect::CancelPage { token } if *token == pending.token)
        ));
    }

    #[test]
    fn multi_page_library_display_ranges_walk_all_windows_forward_and_backward() {
        let total = 264_u32;
        let mut model = BrowseModel::default();
        let bootstrap = request(
            model
                .configure(BrowseSource::Library {
                    session: session(),
                    shortcut: VideoLibraryShortcut {
                        id: "large-library".to_owned(),
                        name: "Large Library".to_owned(),
                        collection_type: "movies".to_owned(),
                        item_count: Some(264),
                        artwork_image_id: None,
                    },
                })
                .expect("source should configure"),
        );

        let initial_effects = settle(&mut model, &bootstrap, total, 24);
        settle_all_requests(&mut model, initial_effects, total);

        // Window 0 of 11 (0..24)
        assert_eq!(model.display_range(), Some(0..24));

        // Scrolling forward walks all 11 windows (windows 1 through 10)
        for expected_window in 1..11 {
            let start = expected_window * 24;
            let end = (start + 24).min(total);
            let next_effects = model
                .set_display_range(start..end, total)
                .expect("set_display_range should succeed");
            settle_all_requests(&mut model, next_effects, total);
            assert_eq!(model.display_range(), Some(start..end));
        }

        // Scrolling back walks windows 9 down to 0
        for expected_window in (0..10).rev() {
            let start = expected_window * 24;
            let end = start + 24;
            let prev_effects = model
                .set_display_range(start..end, total)
                .expect("set_display_range should succeed");
            settle_all_requests(&mut model, prev_effects, total);
            assert_eq!(model.display_range(), Some(start..end));
        }

        // Advance to window 3 (72..96)
        let effects = model
            .set_display_range(72..96, total)
            .expect("set_display_range should succeed");
        settle_all_requests(&mut model, effects, total);
        assert_eq!(model.display_range(), Some(72..96));

        // Filter / preference change resets to first window
        let preferences = BrowsePreferences {
            sort: jellypilot_media_server::VideoLibrarySort::ReleaseDate,
            ..Default::default()
        };
        let reconfig = model
            .configure_with_preferences(
                BrowseSource::Library {
                    session: session(),
                    shortcut: VideoLibraryShortcut {
                        id: "large-library".to_owned(),
                        name: "Large Library".to_owned(),
                        collection_type: "movies".to_owned(),
                        item_count: Some(264),
                        artwork_image_id: None,
                    },
                },
                preferences,
            )
            .expect("reconfigure should succeed");
        let bootstrap2 = request(reconfig);
        let reconfig_effects = settle(&mut model, &bootstrap2, total, 24);
        settle_all_requests(&mut model, reconfig_effects, total);

        assert_eq!(model.display_range(), Some(0..24));
    }

    #[test]
    fn set_display_range_clamps_to_total_and_is_idempotent() {
        let total = 30;
        let mut model = BrowseModel::default();
        let bootstrap = model
            .configure(BrowseSource::Library {
                session: session(),
                shortcut: shortcut(),
            })
            .expect("library should configure");
        settle_all_requests(&mut model, bootstrap, total);

        let effects = model
            .set_display_range(20..u32::MAX, total)
            .expect("display range should dispatch");
        assert!(effects
            .iter()
            .all(|effect| !matches!(effect, BrowseEffect::ResetViewport)));
        assert_eq!(model.display_range(), Some(20..30));
        assert_eq!(
            match model.view() {
                LibraryBrowseView::Ready { visible_start, .. } => Some(visible_start),
                _ => None,
            },
            Some(20)
        );

        assert!(model
            .set_display_range(20..u32::MAX, total)
            .expect("unchanged display range should be accepted")
            .is_empty());

        model
            .set_display_range(total..u32::MAX, total)
            .expect("empty display range should be accepted");
        assert_eq!(
            match model.view() {
                LibraryBrowseView::Ready { visible_start, .. } => Some(visible_start),
                _ => None,
            },
            Some(0)
        );
    }

    #[test]
    fn set_display_range_loads_visible_page_and_lookahead_without_reset() {
        let total = 600;
        let mut model = BrowseModel::default();
        let bootstrap = model
            .configure(BrowseSource::Library {
                session: session(),
                shortcut: shortcut(),
            })
            .expect("library should configure");
        settle_all_requests(&mut model, bootstrap, total);

        let effects = model
            .set_display_range(480..504, total)
            .expect("far display range should dispatch");
        assert!(effects
            .iter()
            .all(|effect| !matches!(effect, BrowseEffect::ResetViewport)));
        assert_eq!(
            requests(&effects)
                .into_iter()
                .map(|request| request.start_index)
                .collect::<Vec<_>>(),
            vec![480, 504]
        );
        assert_eq!(
            match model.view() {
                LibraryBrowseView::Ready { visible_start, .. } => Some(visible_start),
                _ => None,
            },
            Some(480)
        );
    }
    fn loaded_search(total: u32, range: Range<u32>) -> BrowseModel {
        let mut model = BrowseModel::default();
        let effects = model
            .configure(BrowseSource::Search {
                session: session(),
                query: "arrival".to_owned(),
            })
            .unwrap();
        settle_all_requests(&mut model, effects, total);
        let effects = model.set_display_range(range, total).unwrap();
        settle_all_requests(&mut model, effects, total);
        model
    }

    fn fail(model: &mut BrowseModel, request: &BrowsePageRequest) -> Vec<BrowseEffect> {
        model
            .settle(BrowsePageSettlement {
                source_id: request.source_id.clone(),
                token: request.token,
                result: Err("offline".to_owned()),
            })
            .unwrap()
    }

    fn visible_ids(model: &BrowseModel) -> Vec<String> {
        let LibraryBrowseView::Ready { visible_items, .. } = model.view() else {
            panic!("expected usable result");
        };
        visible_items
            .into_iter()
            .map(|slot| slot.item.expect("loaded slot").id)
            .collect()
    }

    #[test]
    fn default_recreation_rejects_departed_delivery_for_identical_query() {
        let source = BrowseSource::Search {
            session: session(),
            query: "arrival".to_owned(),
        };
        let mut departed = BrowseModel::default();
        let old = request(departed.configure(source.clone()).unwrap());
        let mut current = BrowseModel::default();
        let live = request(current.configure(source).unwrap());
        assert!(settle(&mut current, &old, 1, 1).is_empty());
        assert!(matches!(current.view(), LibraryBrowseView::Loading));
        settle(&mut current, &live, 1, 1);
        assert_eq!(visible_ids(&current), vec!["item-0"]);
    }

    #[test]
    fn suspend_resume_preserves_ready_pages_and_rejects_cancelled_delivery() {
        let mut model = loaded_search(240, 48..72);
        let before = visible_ids(&model);
        let pending = model.set_display_range(120..144, 240).unwrap();
        let old = requests(&pending);
        let cancelled = model.suspend();
        assert_eq!(cancelled.len(), old.len());
        assert!(cancelled
            .iter()
            .all(|effect| matches!(effect, BrowseEffect::CancelPage { .. })));
        assert!(model.suspend().is_empty());
        for request in &old {
            assert!(settle(&mut model, request, 240, 24).is_empty());
        }
        assert!(model.set_display_range(48..72, 240).unwrap().is_empty());
        assert_eq!(visible_ids(&model), before);
        let resumed = model.resume().unwrap();
        assert!(resumed
            .iter()
            .all(|effect| matches!(effect, BrowseEffect::RequestPage(_))));
        assert!(model.resume().unwrap().is_empty());
        settle_all_requests(&mut model, resumed, 240);
        assert_eq!(visible_ids(&model), before);
    }

    #[test]
    fn failed_refresh_retains_deep_result_set_and_retry_replaces_it() {
        let mut model = loaded_search(240, 120..144);
        let deep = visible_ids(&model);
        let identity = model.identity().unwrap().to_owned();
        let refresh = request(model.refresh().unwrap());
        fail(&mut model, &refresh);
        assert!(!model.is_refreshing());
        assert!(model.refresh_failure().is_some());
        assert_eq!(visible_ids(&model), deep);
        assert!(model.set_display_range(0..24, 240).unwrap().is_empty());
        assert!(model.set_display_range(120..144, 240).unwrap().is_empty());
        assert_eq!(visible_ids(&model), deep);
        model.suspend();
        assert!(model.resume().unwrap().is_empty());
        assert!(model.refresh_failure().is_some());
        let retry = model.retry().unwrap();
        assert!(model.is_refreshing());
        settle_all_requests(&mut model, retry, 3);
        assert!(!model.is_refreshing());
        assert!(model.refresh_failure().is_none());
        assert_eq!(model.identity(), Some(identity.as_str()));
        assert_eq!(model.display_range(), Some(0..3));
        assert_eq!(visible_ids(&model), vec!["item-0", "item-1", "item-2"]);
        assert_eq!(model.retained_item_count(), 3);
    }

    #[test]
    fn refresh_waits_for_captured_window_not_later_scroll_or_prefetch() {
        let mut model = loaded_search(240, 120..144);
        let bootstrap = request(model.refresh().unwrap());
        let mut pending = requests(&settle(&mut model, &bootstrap, 240, 24));
        assert!(model.is_refreshing());
        model.set_display_range(192..216, 240).unwrap();
        while !pending.iter().any(|request| request.start_index == 120) {
            let next = pending.remove(0);
            pending.extend(requests(&settle(&mut model, &next, 240, 24)));
            assert!(model.is_refreshing());
        }
        let index = pending
            .iter()
            .position(|request| request.start_index == 120)
            .unwrap();
        let required = pending.remove(index);
        let effects = settle(&mut model, &required, 240, 24);
        assert!(!model.is_refreshing());
        assert_eq!(model.display_range(), Some(192..216));
        pending.extend(requests(&effects));
        while let Some(request) = pending.pop() {
            pending.extend(requests(&settle(&mut model, &request, 240, 24)));
        }
        assert_eq!(
            visible_ids(&model).first().map(String::as_str),
            Some("item-192")
        );
    }

    #[test]
    fn suspended_refresh_resumes_required_work_and_same_query_is_noop() {
        let mut model = loaded_search(240, 120..144);
        let original = visible_ids(&model);
        let bootstrap = request(model.refresh().unwrap());
        let effects = settle(&mut model, &bootstrap, 240, 24);
        model.suspend();
        assert!(model
            .configure(BrowseSource::Search {
                session: session(),
                query: "arrival".to_owned()
            })
            .unwrap()
            .is_empty());
        assert!(model.refresh().unwrap().is_empty());
        for request in requests(&effects) {
            assert!(fail(&mut model, &request).is_empty());
        }
        assert!(model.is_refreshing());
        assert_eq!(visible_ids(&model), original);
        let resumed = model.resume().unwrap();
        settle_all_requests(&mut model, resumed, 240);
        assert!(!model.is_refreshing());
        assert_eq!(visible_ids(&model), original);
    }

    #[test]
    fn refresh_atomically_replaces_ready_result_with_empty_and_retains_empty_on_failure() {
        let mut model = loaded_search(240, 120..144);
        let refresh = request(model.refresh().unwrap());
        settle(&mut model, &refresh, 0, 0);
        assert!(!model.is_refreshing());
        assert!(matches!(model.view(), LibraryBrowseView::Empty));
        assert_eq!(model.retained_item_count(), 0);
        let refresh = request(model.refresh().unwrap());
        fail(&mut model, &refresh);
        assert!(matches!(model.view(), LibraryBrowseView::Empty));
        assert!(model
            .refresh_failure()
            .is_some_and(|failure| failure.retryable));
        let retry = model.retry().unwrap();
        settle_all_requests(&mut model, retry, 1);
        assert_eq!(visible_ids(&model), vec!["item-0"]);
    }

    #[test]
    fn required_refresh_page_failure_rolls_back_and_retries_the_refresh() {
        let mut model = loaded_search(240, 120..144);
        let original = visible_ids(&model);
        let bootstrap = request(model.refresh().unwrap());
        let mut pending = requests(&settle(&mut model, &bootstrap, 240, 24));
        while !pending.iter().any(|request| request.start_index == 120) {
            let next = pending.remove(0);
            pending.extend(requests(&settle(&mut model, &next, 240, 24)));
        }
        let required = pending.remove(
            pending
                .iter()
                .position(|request| request.start_index == 120)
                .unwrap(),
        );
        fail(&mut model, &required);
        assert!(!model.is_refreshing());
        assert!(model.refresh_failure().is_some());
        assert_eq!(visible_ids(&model), original);
        for departed in pending {
            assert!(settle(&mut model, &departed, 240, 24).is_empty());
        }
        let retry = request(model.retry().unwrap());
        assert_eq!(retry.start_index, 0);
        assert!(model.is_refreshing());
    }

    #[test]
    fn refreshed_result_never_mixes_old_and_new_pages() {
        let mut model = loaded_search(240, 120..144);
        let original = visible_ids(&model);
        let effects = model.refresh().unwrap();
        let mut pending = requests(&effects);
        while let Some(request) = pending.pop() {
            let mut replacement_items = items(request.start_index, 24);
            for item in &mut replacement_items {
                item.id = format!("new-{}", item.id);
            }
            let effects = model
                .settle(BrowsePageSettlement {
                    source_id: request.source_id,
                    token: request.token,
                    result: Ok(BrowsePagePayload {
                        start_index: request.start_index,
                        limit: request.limit,
                        total_record_count: 240,
                        has_more: request.start_index + 24 < 240,
                        items: replacement_items,
                    }),
                })
                .unwrap();
            pending.extend(requests(&effects));
            if model.is_refreshing() {
                assert_eq!(visible_ids(&model), original);
            } else {
                assert!(visible_ids(&model).iter().all(|id| id.starts_with("new-")));
            }
        }
    }
    #[test]
    fn incomplete_refresh_window_fails_instead_of_waiting_forever() {
        let mut model = loaded_search(240, 0..24);
        let original = visible_ids(&model);
        let bootstrap = request(model.refresh().unwrap());
        settle(&mut model, &bootstrap, 240, 1);
        assert!(!model.is_refreshing());
        assert!(model
            .refresh_failure()
            .is_some_and(|failure| !failure.retryable));
        assert_eq!(visible_ids(&model), original);
    }
}
