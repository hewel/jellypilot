use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::{
    LibraryBrowseAction, LibraryBrowseCacheMode, LibraryBrowseCommand, LibraryBrowseCoreError,
    LibraryBrowseFailure, LibraryBrowseLoadPriority, LibraryBrowseLoadToken, LibraryBrowseMode,
    LibraryBrowsePageOutcome, LibraryBrowseSlot, LibraryBrowseSnapshot, LibraryBrowseStatus,
    LibraryBrowseUpdate, LIBRARY_BROWSE_LOOKAHEAD_PAGES, LIBRARY_BROWSE_MAX_CONCURRENT_LOADS,
    LIBRARY_BROWSE_PAGE_SIZE, LIBRARY_BROWSE_VIRTUAL_THRESHOLD,
};

const MALFORMED_PAGE_MESSAGE: &str = "Library page metadata did not match the requested page.";

#[derive(Clone, Debug, Eq, PartialEq)]
struct LoadedPage {
    total_record_count: u32,
    item_count: u32,
    has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StoredPage {
    Loaded(LoadedPage),
    Failed(LibraryBrowseFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingLoad {
    start_index: u32,
    limit: u32,
    priority: LibraryBrowseLoadPriority,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlannedPage {
    start_index: u32,
    priority: LibraryBrowseLoadPriority,
}

/// Synchronous owner of library paging metadata and load scheduling.
///
/// The core never stores item payloads and performs no I/O. Consumers execute
/// emitted commands in order, retain page items externally, and dispatch a
/// [`LibraryBrowseAction::PageSettled`] event for every completed load.
#[derive(Clone, Debug)]
pub struct LibraryBrowseCore {
    source_id: Option<String>,
    enabled: bool,
    generation: u32,
    next_sequence: u32,
    pages: BTreeMap<u32, StoredPage>,
    pending: BTreeMap<LibraryBrowseLoadToken, PendingLoad>,
    retry_requested: BTreeSet<u32>,
    display_indexes: Vec<u32>,
    /// When set, loaded pages are kept after leaving the virtual window.
    ///
    /// Defaults to `false` (evict on window exit), the behavior the wasm/web
    /// contract relies on. Native shells may opt into accumulation so
    /// revisiting an earlier window does not refetch; failed pages are always
    /// released when they leave the window so returning to them refetches.
    retain_loaded_pages: bool,
}

impl Default for LibraryBrowseCore {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryBrowseCore {
    /// Creates an inactive reducer without a configured source.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            source_id: None,
            enabled: false,
            generation: 0,
            next_sequence: 0,
            pages: BTreeMap::new(),
            pending: BTreeMap::new(),
            retry_requested: BTreeSet::new(),
            display_indexes: Vec::new(),
            retain_loaded_pages: false,
        }
    }

    /// Applies one action and returns the resulting snapshot and ordered effects.
    pub fn dispatch(
        &mut self,
        action: LibraryBrowseAction,
    ) -> Result<LibraryBrowseUpdate, LibraryBrowseCoreError> {
        let mut next = self.clone();
        let update = next.dispatch_inner(action)?;
        *self = next;
        Ok(update)
    }

    fn dispatch_inner(
        &mut self,
        action: LibraryBrowseAction,
    ) -> Result<LibraryBrowseUpdate, LibraryBrowseCoreError> {
        let mut commands = Vec::new();

        match action {
            LibraryBrowseAction::Configure { source_id, enabled } => {
                self.configure(source_id, enabled, &mut commands)?;
            }
            LibraryBrowseAction::WindowChanged { display_indexes } => {
                self.display_indexes = deduplicate(display_indexes);
                self.reconcile_virtual_window(&mut commands)?;
            }
            LibraryBrowseAction::LoadNext => self.load_next(&mut commands)?,
            LibraryBrowseAction::Retry => self.retry(&mut commands)?,
            LibraryBrowseAction::PageSettled { token, outcome } => {
                self.page_settled(token, outcome, &mut commands)?;
            }
        }

        Ok(LibraryBrowseUpdate {
            snapshot: self.snapshot(),
            commands,
        })
    }

    /// Selects whether loaded pages are retained after leaving the window.
    ///
    /// The setting is metadata-only, survives configure cycles, and never
    /// alters the serialized snapshot or command shapes.
    pub fn set_retain_loaded_pages(&mut self, retain: bool) {
        self.retain_loaded_pages = retain;
    }

    /// Returns the current immutable reducer view without changing state.
    #[must_use]
    pub fn snapshot(&self) -> LibraryBrowseSnapshot {
        LibraryBrowseSnapshot {
            status: self.status(),
            slots: self.slots(),
            pending_count: u32::try_from(self.pending.len()).unwrap_or(u32::MAX),
        }
    }

    fn configure(
        &mut self,
        source_id: String,
        enabled: bool,
        commands: &mut Vec<LibraryBrowseCommand>,
    ) -> Result<(), LibraryBrowseCoreError> {
        if enabled && source_id.trim().is_empty() {
            return Err(LibraryBrowseCoreError::EmptySourceId);
        }
        let source_changed = self.source_id.as_ref() != Some(&source_id);
        if !source_changed && self.enabled == enabled {
            return Ok(());
        }

        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(LibraryBrowseCoreError::GenerationExhausted)?;
        self.next_sequence = 0;

        commands.extend(
            self.pending
                .keys()
                .copied()
                .map(|token| LibraryBrowseCommand::CancelLoad { token }),
        );
        if !self.pages.is_empty() {
            commands.push(LibraryBrowseCommand::ReleasePages {
                page_starts: self.pages.keys().copied().collect(),
            });
        }
        if source_changed {
            commands.push(LibraryBrowseCommand::ResetViewport);
        }

        self.source_id = Some(source_id);
        self.enabled = enabled;
        self.pages.clear();
        self.pending.clear();
        self.retry_requested.clear();
        self.display_indexes.clear();

        if enabled {
            self.schedule_load(
                0,
                LibraryBrowseLoadPriority::Bootstrap,
                LibraryBrowseCacheMode::ReuseSuccess,
                commands,
            )?;
        }
        Ok(())
    }

    fn load_next(
        &mut self,
        commands: &mut Vec<LibraryBrowseCommand>,
    ) -> Result<(), LibraryBrowseCoreError> {
        if !self.enabled || self.mode() != Some(LibraryBrowseMode::Normal) {
            return Ok(());
        }

        let Some(start_index) = self.next_sequential_page() else {
            return Ok(());
        };
        self.schedule_load(
            start_index,
            LibraryBrowseLoadPriority::Sequential,
            LibraryBrowseCacheMode::ReuseSuccess,
            commands,
        )
    }

    fn retry(
        &mut self,
        commands: &mut Vec<LibraryBrowseCommand>,
    ) -> Result<(), LibraryBrowseCoreError> {
        if !self.enabled {
            return Ok(());
        }

        self.retry_requested
            .extend(self.pages.iter().filter_map(|(start_index, page)| {
                matches!(page, StoredPage::Failed(failure) if failure.retryable)
                    .then_some(*start_index)
            }));
        self.schedule_retries(commands)
    }

    fn page_settled(
        &mut self,
        token: LibraryBrowseLoadToken,
        outcome: LibraryBrowsePageOutcome,
        commands: &mut Vec<LibraryBrowseCommand>,
    ) -> Result<(), LibraryBrowseCoreError> {
        let Some(pending) = self.pending.remove(&token) else {
            return Ok(());
        };

        if pending.priority == LibraryBrowseLoadPriority::Retry {
            self.retry_requested.remove(&pending.start_index);
        }

        let (page, malformed_loaded_page) = match outcome {
            LibraryBrowsePageOutcome::Loaded {
                start_index,
                limit,
                total_record_count,
                item_count,
                has_more,
            } => match self.validate_loaded_page(
                pending,
                start_index,
                limit,
                total_record_count,
                item_count,
                has_more,
            ) {
                Ok(page) => (StoredPage::Loaded(page), false),
                Err(failure) => (StoredPage::Failed(failure), true),
            },
            LibraryBrowsePageOutcome::Failed { failure } => (StoredPage::Failed(failure), false),
        };
        self.pages.insert(pending.start_index, page);
        if malformed_loaded_page {
            commands.push(LibraryBrowseCommand::ReleasePages {
                page_starts: vec![pending.start_index],
            });
        }

        self.reconcile_virtual_window(commands)?;
        self.schedule_retries(commands)
    }

    fn validate_loaded_page(
        &self,
        pending: PendingLoad,
        start_index: u32,
        limit: u32,
        total_record_count: u32,
        item_count: u32,
        has_more: bool,
    ) -> Result<LoadedPage, LibraryBrowseFailure> {
        let end_index = start_index.checked_add(item_count);
        let expected_has_more = start_index
            .checked_add(item_count)
            .is_some_and(|end_index| end_index < total_record_count);
        let page_zero_total = self.page_zero().map(|page| page.total_record_count);

        let valid = start_index == pending.start_index
            && limit == pending.limit
            && start_index.is_multiple_of(LIBRARY_BROWSE_PAGE_SIZE)
            && start_index <= total_record_count
            && item_count <= limit
            && end_index.is_some_and(|end_index| end_index <= total_record_count)
            && has_more == expected_has_more
            && (start_index == 0 || page_zero_total == Some(total_record_count));

        if !valid {
            return Err(LibraryBrowseFailure {
                message: MALFORMED_PAGE_MESSAGE.to_owned(),
                retryable: false,
            });
        }

        Ok(LoadedPage {
            total_record_count,
            item_count,
            has_more,
        })
    }

    fn reconcile_virtual_window(
        &mut self,
        commands: &mut Vec<LibraryBrowseCommand>,
    ) -> Result<(), LibraryBrowseCoreError> {
        if self.mode() != Some(LibraryBrowseMode::Virtual) {
            return Ok(());
        }

        let planned_pages = self.planned_virtual_pages();
        let retained: BTreeSet<u32> = std::iter::once(0)
            .chain(planned_pages.iter().map(|page| page.start_index))
            .collect();

        let released: Vec<u32> = self
            .pages
            .iter()
            .filter(|(start_index, page)| {
                !retained.contains(*start_index)
                    && (!self.retain_loaded_pages || matches!(page, StoredPage::Failed(_)))
            })
            .map(|(start_index, _)| *start_index)
            .collect();
        if !released.is_empty() {
            for start_index in &released {
                self.pages.remove(start_index);
                self.retry_requested.remove(start_index);
            }
            commands.push(LibraryBrowseCommand::ReleasePages {
                page_starts: released,
            });
        }
        self.retry_requested
            .retain(|start_index| retained.contains(start_index));

        self.schedule_retries(commands)?;
        for page in planned_pages {
            if self.available_load_slots() == 0 {
                break;
            }
            if !self.pages.contains_key(&page.start_index)
                && !self.has_pending_page(page.start_index)
            {
                self.schedule_load(
                    page.start_index,
                    page.priority,
                    LibraryBrowseCacheMode::ReuseSuccess,
                    commands,
                )?;
            }
        }
        Ok(())
    }

    fn schedule_retries(
        &mut self,
        commands: &mut Vec<LibraryBrowseCommand>,
    ) -> Result<(), LibraryBrowseCoreError> {
        if !self.enabled {
            return Ok(());
        }

        let requested: Vec<u32> = self.retry_requested.iter().copied().collect();
        for start_index in requested {
            if self.available_load_slots() == 0 {
                break;
            }
            if start_index != 0 && self.page_zero().is_none() {
                continue;
            }
            if self.has_pending_page(start_index) {
                continue;
            }
            if !matches!(
                self.pages.get(&start_index),
                Some(StoredPage::Failed(failure)) if failure.retryable
            ) {
                self.retry_requested.remove(&start_index);
                continue;
            }
            self.schedule_load(
                start_index,
                LibraryBrowseLoadPriority::Retry,
                LibraryBrowseCacheMode::Reload,
                commands,
            )?;
        }
        Ok(())
    }

    fn schedule_load(
        &mut self,
        start_index: u32,
        priority: LibraryBrowseLoadPriority,
        cache_mode: LibraryBrowseCacheMode,
        commands: &mut Vec<LibraryBrowseCommand>,
    ) -> Result<(), LibraryBrowseCoreError> {
        if self.available_load_slots() == 0 || self.has_pending_page(start_index) {
            return Ok(());
        }

        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(LibraryBrowseCoreError::SequenceExhausted)?;
        let token = LibraryBrowseLoadToken {
            generation: self.generation,
            sequence: self.next_sequence,
        };
        self.pending.insert(
            token,
            PendingLoad {
                start_index,
                limit: LIBRARY_BROWSE_PAGE_SIZE,
                priority,
            },
        );
        commands.push(LibraryBrowseCommand::LoadPage {
            token,
            start_index,
            limit: LIBRARY_BROWSE_PAGE_SIZE,
            priority,
            cache_mode,
        });
        Ok(())
    }

    fn next_sequential_page(&self) -> Option<u32> {
        let mut start_index = 0;
        loop {
            match self.pages.get(&start_index) {
                Some(StoredPage::Loaded(page)) if page.has_more => {
                    start_index = start_index.checked_add(LIBRARY_BROWSE_PAGE_SIZE)?;
                }
                Some(StoredPage::Loaded(_)) | Some(StoredPage::Failed(_)) | None => return None,
            }

            if self.has_pending_page(start_index) || self.pages.contains_key(&start_index) {
                continue;
            }
            return Some(start_index);
        }
    }

    fn planned_virtual_pages(&self) -> Vec<PlannedPage> {
        let Some(total_record_count) = self.total_record_count() else {
            return Vec::new();
        };

        let mut seen = BTreeSet::new();
        let mut planned = Vec::new();
        for display_index in &self.display_indexes {
            if *display_index >= total_record_count {
                continue;
            }
            let page_start = (*display_index / LIBRARY_BROWSE_PAGE_SIZE) * LIBRARY_BROWSE_PAGE_SIZE;
            if seen.insert(page_start) {
                planned.push(PlannedPage {
                    start_index: page_start,
                    priority: LibraryBrowseLoadPriority::Visible,
                });
            }
        }

        if let Some(highest_visible) = planned.iter().map(|page| page.start_index).max() {
            let lookahead_distance =
                LIBRARY_BROWSE_PAGE_SIZE.saturating_mul(LIBRARY_BROWSE_LOOKAHEAD_PAGES);
            if let Some(lookahead_start) = highest_visible.checked_add(lookahead_distance) {
                let last_page_start = ((total_record_count - 1) / LIBRARY_BROWSE_PAGE_SIZE)
                    * LIBRARY_BROWSE_PAGE_SIZE;
                if lookahead_start <= last_page_start && seen.insert(lookahead_start) {
                    planned.push(PlannedPage {
                        start_index: lookahead_start,
                        priority: LibraryBrowseLoadPriority::Prefetch,
                    });
                }
            }
        }

        planned
    }

    fn slots(&self) -> Vec<LibraryBrowseSlot> {
        if self.mode() == Some(LibraryBrowseMode::Normal) {
            return self.normal_slots();
        }
        if self.mode() != Some(LibraryBrowseMode::Virtual) {
            return Vec::new();
        }
        let Some(total_record_count) = self.total_record_count() else {
            return Vec::new();
        };

        self.display_indexes
            .iter()
            .copied()
            .filter(|display_index| *display_index < total_record_count)
            .map(|display_index| {
                let page_start =
                    (display_index / LIBRARY_BROWSE_PAGE_SIZE) * LIBRARY_BROWSE_PAGE_SIZE;
                LibraryBrowseSlot {
                    display_index,
                    page_start,
                    index_within_page: display_index - page_start,
                }
            })
            .collect()
    }

    fn normal_slots(&self) -> Vec<LibraryBrowseSlot> {
        let mut slots = Vec::new();
        let mut page_start = 0;
        let mut display_index = 0;

        while let Some(StoredPage::Loaded(page)) = self.pages.get(&page_start) {
            slots.extend((0..page.item_count).map(|index_within_page| {
                let slot = LibraryBrowseSlot {
                    display_index,
                    page_start,
                    index_within_page,
                };
                display_index = display_index.saturating_add(1);
                slot
            }));
            if !page.has_more {
                break;
            }
            let Some(next_page_start) = page_start.checked_add(LIBRARY_BROWSE_PAGE_SIZE) else {
                break;
            };
            page_start = next_page_start;
        }
        slots
    }

    fn status(&self) -> LibraryBrowseStatus {
        if !self.enabled {
            return LibraryBrowseStatus::Inactive;
        }

        match self.pages.get(&0) {
            Some(StoredPage::Failed(failure)) => LibraryBrowseStatus::InitialFailure {
                failure: failure.clone(),
                retry_busy: self.retry_busy(),
            },
            Some(StoredPage::Loaded(page)) if page.total_record_count == 0 => {
                LibraryBrowseStatus::Empty {
                    total_record_count: page.total_record_count,
                }
            }
            Some(StoredPage::Loaded(page)) => LibraryBrowseStatus::Ready {
                mode: mode_for_total(page.total_record_count),
                total_record_count: page.total_record_count,
                is_fetching_more: self
                    .pending
                    .values()
                    .any(|pending| pending.start_index != 0),
                can_load_next: self.mode() == Some(LibraryBrowseMode::Normal)
                    && self.next_sequential_page().is_some(),
                load_more_failure: self.pages.iter().find_map(|(start_index, stored)| {
                    if *start_index == 0 {
                        return None;
                    }
                    match stored {
                        StoredPage::Failed(failure) => Some(failure.clone()),
                        StoredPage::Loaded(_) => None,
                    }
                }),
                retry_busy: self.retry_busy(),
            },
            None => LibraryBrowseStatus::Loading,
        }
    }

    fn retry_busy(&self) -> bool {
        !self.retry_requested.is_empty()
            || self
                .pending
                .values()
                .any(|pending| pending.priority == LibraryBrowseLoadPriority::Retry)
    }

    fn page_zero(&self) -> Option<&LoadedPage> {
        match self.pages.get(&0) {
            Some(StoredPage::Loaded(page)) => Some(page),
            Some(StoredPage::Failed(_)) | None => None,
        }
    }

    fn total_record_count(&self) -> Option<u32> {
        self.page_zero().map(|page| page.total_record_count)
    }

    fn mode(&self) -> Option<LibraryBrowseMode> {
        self.total_record_count().map(mode_for_total)
    }

    fn has_pending_page(&self, start_index: u32) -> bool {
        self.pending
            .values()
            .any(|pending| pending.start_index == start_index)
    }

    fn available_load_slots(&self) -> usize {
        LIBRARY_BROWSE_MAX_CONCURRENT_LOADS.saturating_sub(self.pending.len())
    }
}

fn deduplicate(display_indexes: Vec<u32>) -> Vec<u32> {
    let mut seen = HashSet::with_capacity(display_indexes.len());
    display_indexes
        .into_iter()
        .filter(|display_index| seen.insert(*display_index))
        .collect()
}

const fn mode_for_total(total_record_count: u32) -> LibraryBrowseMode {
    if total_record_count > LIBRARY_BROWSE_VIRTUAL_THRESHOLD {
        LibraryBrowseMode::Virtual
    } else {
        LibraryBrowseMode::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_returns_generation_exhaustion_without_mutation() {
        let mut core = LibraryBrowseCore {
            generation: u32::MAX,
            ..LibraryBrowseCore::new()
        };
        let before = core.snapshot();

        let error = core
            .dispatch(LibraryBrowseAction::Configure {
                source_id: "movies".to_owned(),
                enabled: true,
            })
            .expect_err("generation should be exhausted");

        assert_eq!(
            (error, core.snapshot()),
            (LibraryBrowseCoreError::GenerationExhausted, before)
        );
    }

    #[test]
    fn scheduling_returns_sequence_exhaustion_without_mutation() {
        let mut core = LibraryBrowseCore {
            source_id: Some("movies".to_owned()),
            enabled: true,
            generation: 1,
            next_sequence: u32::MAX,
            pages: BTreeMap::from([(
                0,
                StoredPage::Loaded(LoadedPage {
                    total_record_count: 80,
                    item_count: 24,
                    has_more: true,
                }),
            )]),
            pending: BTreeMap::new(),
            retry_requested: BTreeSet::new(),
            display_indexes: Vec::new(),
            retain_loaded_pages: false,
        };
        let before = core.snapshot();

        let error = core
            .dispatch(LibraryBrowseAction::WindowChanged {
                display_indexes: vec![24],
            })
            .expect_err("sequence should be exhausted");
        assert_eq!(
            (error, core.snapshot()),
            (LibraryBrowseCoreError::SequenceExhausted, before)
        );
    }

    const TOTAL: u32 = LIBRARY_BROWSE_PAGE_SIZE * 10;

    fn configure_enabled(core: &mut LibraryBrowseCore) -> LibraryBrowseLoadToken {
        let update = core
            .dispatch(LibraryBrowseAction::Configure {
                source_id: "movies".to_owned(),
                enabled: true,
            })
            .expect("configure should succeed");
        update
            .commands
            .iter()
            .find_map(|command| match command {
                LibraryBrowseCommand::LoadPage { token, .. } => Some(*token),
                LibraryBrowseCommand::ResetViewport
                | LibraryBrowseCommand::CancelLoad { .. }
                | LibraryBrowseCommand::ReleasePages { .. } => None,
            })
            .expect("bootstrap load should be scheduled")
    }

    fn settle_loaded(
        core: &mut LibraryBrowseCore,
        token: LibraryBrowseLoadToken,
        start_index: u32,
    ) -> LibraryBrowseUpdate {
        core.dispatch(LibraryBrowseAction::PageSettled {
            token,
            outcome: LibraryBrowsePageOutcome::Loaded {
                start_index,
                limit: LIBRARY_BROWSE_PAGE_SIZE,
                total_record_count: TOTAL,
                item_count: LIBRARY_BROWSE_PAGE_SIZE,
                has_more: start_index + LIBRARY_BROWSE_PAGE_SIZE < TOTAL,
            },
        })
        .expect("loaded page should settle")
    }

    fn settle_failed(core: &mut LibraryBrowseCore, token: LibraryBrowseLoadToken) {
        core.dispatch(LibraryBrowseAction::PageSettled {
            token,
            outcome: LibraryBrowsePageOutcome::Failed {
                failure: LibraryBrowseFailure {
                    message: "flaky".to_owned(),
                    retryable: true,
                },
            },
        })
        .expect("failed page should settle");
    }

    fn move_window(core: &mut LibraryBrowseCore, start: u32) -> LibraryBrowseUpdate {
        core.dispatch(LibraryBrowseAction::WindowChanged {
            display_indexes: (start..start + LIBRARY_BROWSE_PAGE_SIZE).collect(),
        })
        .expect("window change should dispatch")
    }

    fn settle_all_loads(core: &mut LibraryBrowseCore, update: &LibraryBrowseUpdate) {
        let loads: Vec<(LibraryBrowseLoadToken, u32)> = update
            .commands
            .iter()
            .filter_map(|command| match command {
                LibraryBrowseCommand::LoadPage {
                    token, start_index, ..
                } => Some((*token, *start_index)),
                LibraryBrowseCommand::ResetViewport
                | LibraryBrowseCommand::CancelLoad { .. }
                | LibraryBrowseCommand::ReleasePages { .. } => None,
            })
            .collect();
        for (token, start_index) in loads {
            settle_loaded(core, token, start_index);
        }
    }

    fn requested_starts(update: &LibraryBrowseUpdate) -> Vec<u32> {
        update
            .commands
            .iter()
            .filter_map(|command| match command {
                LibraryBrowseCommand::LoadPage { start_index, .. } => Some(*start_index),
                LibraryBrowseCommand::ResetViewport
                | LibraryBrowseCommand::CancelLoad { .. }
                | LibraryBrowseCommand::ReleasePages { .. } => None,
            })
            .collect()
    }

    fn released_starts(update: &LibraryBrowseUpdate) -> Vec<u32> {
        let mut released = Vec::new();
        for command in &update.commands {
            if let LibraryBrowseCommand::ReleasePages { page_starts } = command {
                released.extend(page_starts.iter().copied());
            }
        }
        released
    }

    #[test]
    fn default_policy_evicts_loaded_pages_outside_the_window() {
        let mut core = LibraryBrowseCore::new();
        let bootstrap = configure_enabled(&mut core);
        settle_loaded(&mut core, bootstrap, 0);
        let forward = move_window(&mut core, LIBRARY_BROWSE_PAGE_SIZE);
        settle_all_loads(&mut core, &forward);

        let further = move_window(&mut core, LIBRARY_BROWSE_PAGE_SIZE * 2);
        assert_eq!(released_starts(&further), vec![LIBRARY_BROWSE_PAGE_SIZE]);
        settle_all_loads(&mut core, &further);

        let back = move_window(&mut core, LIBRARY_BROWSE_PAGE_SIZE);
        assert!(
            requested_starts(&back).contains(&LIBRARY_BROWSE_PAGE_SIZE),
            "evicted page should be refetched on return"
        );
    }

    #[test]
    fn retain_policy_keeps_loaded_pages_across_window_moves() {
        let mut core = LibraryBrowseCore::new();
        core.set_retain_loaded_pages(true);
        let bootstrap = configure_enabled(&mut core);
        settle_loaded(&mut core, bootstrap, 0);
        let forward = move_window(&mut core, LIBRARY_BROWSE_PAGE_SIZE);
        settle_all_loads(&mut core, &forward);

        let further = move_window(&mut core, LIBRARY_BROWSE_PAGE_SIZE * 2);
        assert!(released_starts(&further).is_empty());
        settle_all_loads(&mut core, &further);

        let back = move_window(&mut core, LIBRARY_BROWSE_PAGE_SIZE);
        assert!(released_starts(&back).is_empty());
        assert!(
            requested_starts(&back).is_empty(),
            "retained page should not be refetched on return"
        );
    }

    #[test]
    fn retain_policy_still_releases_failed_pages_outside_the_window() {
        let mut core = LibraryBrowseCore::new();
        core.set_retain_loaded_pages(true);
        let bootstrap = configure_enabled(&mut core);
        settle_loaded(&mut core, bootstrap, 0);
        let initial = move_window(&mut core, 0);
        let lookahead = initial
            .commands
            .iter()
            .find_map(|command| match command {
                LibraryBrowseCommand::LoadPage { token, .. } => Some(*token),
                LibraryBrowseCommand::ResetViewport
                | LibraryBrowseCommand::CancelLoad { .. }
                | LibraryBrowseCommand::ReleasePages { .. } => None,
            })
            .expect("lookahead load should be scheduled");
        settle_failed(&mut core, lookahead);

        let away = move_window(&mut core, LIBRARY_BROWSE_PAGE_SIZE * 2);
        assert_eq!(released_starts(&away), vec![LIBRARY_BROWSE_PAGE_SIZE]);
        settle_all_loads(&mut core, &away);

        let back = move_window(&mut core, 0);
        assert!(
            requested_starts(&back).contains(&LIBRARY_BROWSE_PAGE_SIZE),
            "released failed page should be refetched on return"
        );
    }
}
