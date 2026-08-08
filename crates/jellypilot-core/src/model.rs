//! Public data contract for the library browse reducer.

use std::fmt;

/// Number of item records requested in one server page.
pub const LIBRARY_BROWSE_PAGE_SIZE: u32 = 24;

/// Result counts above this value use random-access virtual paging.
pub const LIBRARY_BROWSE_VIRTUAL_THRESHOLD: u32 = 100;

/// Maximum number of page loads the reducer allows at once.
pub const LIBRARY_BROWSE_MAX_CONCURRENT_LOADS: usize = 2;

/// Number of speculative pages retained after the visible page window.
pub const LIBRARY_BROWSE_LOOKAHEAD_PAGES: u32 = 1;

/// Input or token-space errors that leave reducer state unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryBrowseCoreError {
    /// An enabled configuration must identify a non-empty source.
    EmptySourceId,
    /// No later source generation can be represented by the token contract.
    GenerationExhausted,
    /// No later load sequence can be represented in the current generation.
    SequenceExhausted,
}

impl fmt::Display for LibraryBrowseCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySourceId => formatter.write_str("enabled library browse source id is empty"),
            Self::GenerationExhausted => {
                formatter.write_str("library browse load-token generation is exhausted")
            }
            Self::SequenceExhausted => {
                formatter.write_str("library browse load-token sequence is exhausted")
            }
        }
    }
}

impl std::error::Error for LibraryBrowseCoreError {}

/// Identifies a load within one configured browse generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LibraryBrowseLoadToken {
    /// Configuration generation that issued the load.
    pub generation: u32,
    /// Monotonic load number within the generation.
    pub sequence: u32,
}

/// Stable failure metadata retained by the reducer until retry or release.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryBrowseFailure {
    /// Human-readable description supplied by the load boundary.
    pub message: String,
    /// Whether an explicit retry may issue the page again.
    pub retryable: bool,
}

/// Metadata returned when a requested page settles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryBrowsePageOutcome {
    /// A page was loaded successfully.
    Loaded {
        /// Server offset represented by the page.
        start_index: u32,
        /// Requested record limit.
        limit: u32,
        /// Total records in the configured browse result.
        total_record_count: u32,
        /// Records returned for this page.
        item_count: u32,
        /// Whether the server reports a later sequential page.
        has_more: bool,
    },
    /// A page load failed.
    Failed {
        /// Failure information displayed by the caller.
        failure: LibraryBrowseFailure,
    },
}

/// Inputs accepted by [`crate::LibraryBrowseCore::dispatch`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryBrowseAction {
    /// Selects the complete browse identity and whether loading is enabled.
    Configure {
        /// Opaque identity covering server, library, filters, and ordering.
        source_id: String,
        /// Whether the source may issue page loads.
        enabled: bool,
    },
    /// Replaces the visible and overscanned display-index window.
    WindowChanged {
        /// Display indexes in render priority order.
        display_indexes: Vec<u32>,
    },
    /// Requests one later page in normal mode.
    LoadNext,
    /// Retries all retained retryable failures, bounded by concurrency.
    Retry,
    /// Returns metadata for a previously issued load token.
    PageSettled {
        /// Token from the corresponding load command.
        token: LibraryBrowseLoadToken,
        /// Success or failure metadata.
        outcome: LibraryBrowsePageOutcome,
    },
}

/// Scheduling priority attached to a page load command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryBrowseLoadPriority {
    /// The page-zero request that establishes total count and mode.
    Bootstrap,
    /// A user-driven continuation in normal mode.
    Sequential,
    /// A page containing a visible or overscanned display slot.
    Visible,
    /// A speculative page after the current window.
    Prefetch,
    /// An explicit retry of a retained failure.
    Retry,
}

/// Cache behavior requested for a page load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryBrowseCacheMode {
    /// Reuse an already successful page before accessing the network.
    ReuseSuccess,
    /// Bypass a retained failure and reload the page.
    Reload,
}

/// Side effects emitted by the pure reducer for the caller to execute in order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryBrowseCommand {
    /// Moves the shared browse viewport to its initial position.
    ResetViewport,
    /// Loads one metadata-addressed page.
    LoadPage {
        /// Token that must accompany the settlement event.
        token: LibraryBrowseLoadToken,
        /// Server record offset.
        start_index: u32,
        /// Maximum records requested.
        limit: u32,
        /// Scheduling priority.
        priority: LibraryBrowseLoadPriority,
        /// Cache behavior.
        cache_mode: LibraryBrowseCacheMode,
    },
    /// Cancels work that is no longer relevant to the current source or window.
    CancelLoad {
        /// Token of the obsolete page load.
        token: LibraryBrowseLoadToken,
    },
    /// Releases caller-owned item payloads whose metadata left retention.
    ReleasePages {
        /// Sorted page starts to remove from the caller's page store.
        page_starts: Vec<u32>,
    },
}

/// Rendering strategy established by a successful page zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryBrowseMode {
    /// Contiguous sentinel-driven paging for small results.
    Normal,
    /// Random-access paging for large virtualized results.
    Virtual,
}

/// User-visible reducer status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryBrowseStatus {
    /// Loading is disabled for the configured source.
    Inactive,
    /// Page zero has not settled successfully or failed yet.
    Loading,
    /// Page zero established an empty result set.
    Empty {
        /// Total count reported by the server.
        total_record_count: u32,
    },
    /// Page zero failed, so no continuation work can start.
    InitialFailure {
        /// Retained failure for page zero.
        failure: LibraryBrowseFailure,
        /// Whether an explicit retry is queued or in flight.
        retry_busy: bool,
    },
    /// Page zero established a non-empty browse result.
    Ready {
        /// Active small- or large-result strategy.
        mode: LibraryBrowseMode,
        /// Total count reported by page zero.
        total_record_count: u32,
        /// Whether a non-bootstrap page is currently loading.
        is_fetching_more: bool,
        /// Whether normal mode has another sequential page to request.
        can_load_next: bool,
        /// First retained failure after page zero, if any.
        load_more_failure: Option<LibraryBrowseFailure>,
        /// Whether explicit retry work is queued or in flight.
        retry_busy: bool,
    },
}

/// Maps one virtual display position to caller-owned page data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LibraryBrowseSlot {
    /// Position in display order.
    pub display_index: u32,
    /// Server page containing the position.
    pub page_start: u32,
    /// Offset inside the server page.
    pub index_within_page: u32,
}

/// Immutable view returned after every reducer transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryBrowseSnapshot {
    /// User-visible lifecycle state.
    pub status: LibraryBrowseStatus,
    /// Current virtual-window address mappings in display order.
    pub slots: Vec<LibraryBrowseSlot>,
    /// Number of issued page loads that have not settled or been cancelled.
    pub pending_count: u32,
}

/// One reducer transition, including its new snapshot and ordered effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryBrowseUpdate {
    /// State after applying the action and scheduling eligible work.
    pub snapshot: LibraryBrowseSnapshot,
    /// Effects the caller must execute in vector order.
    pub commands: Vec<LibraryBrowseCommand>,
}
