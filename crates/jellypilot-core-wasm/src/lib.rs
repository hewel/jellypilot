//! Direct TypeScript bindings for the JellyPilot library browse core.

use jellypilot_core::{
    LibraryBrowseAction as CoreAction, LibraryBrowseCacheMode as CoreCacheMode,
    LibraryBrowseCommand as CoreCommand, LibraryBrowseCore as Core,
    LibraryBrowseFailure as CoreFailure, LibraryBrowseLoadPriority as CoreLoadPriority,
    LibraryBrowseLoadToken as CoreLoadToken, LibraryBrowseMode as CoreMode,
    LibraryBrowsePageOutcome as CorePageOutcome, LibraryBrowseSlot as CoreSlot,
    LibraryBrowseSnapshot as CoreSnapshot, LibraryBrowseStatus as CoreStatus,
    LibraryBrowseUpdate as CoreUpdate,
};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// Load identity carried unchanged between a command and settlement event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub struct LibraryBrowseLoadToken {
    /// Configuration generation that issued the load.
    pub generation: u32,
    /// Monotonic load number within the generation.
    pub sequence: u32,
}

/// Stable page failure metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub struct LibraryBrowseFailure {
    /// Human-readable failure description.
    pub message: String,
    /// Whether an explicit retry may issue the page again.
    pub retryable: bool,
}

/// Page metadata returned by the JavaScript load boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(tag = "tag", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum LibraryBrowsePageOutcome {
    /// A page loaded and its metadata was observed.
    Loaded {
        /// Server record offset.
        start_index: u32,
        /// Requested record limit.
        limit: u32,
        /// Complete result count.
        total_record_count: u32,
        /// Number of records returned.
        item_count: u32,
        /// Whether a later sequential page exists.
        has_more: bool,
    },
    /// The page load failed.
    Failed {
        /// Retained error metadata.
        failure: LibraryBrowseFailure,
    },
}

/// Synchronous reducer events dispatched from TypeScript.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(tag = "tag", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum LibraryBrowseEvent {
    /// Selects the complete browse identity and loading readiness.
    Configure {
        /// Opaque identity for the current server query.
        source_id: String,
        /// Whether page loads may be issued.
        enabled: bool,
    },
    /// Replaces the visible and overscanned display window.
    WindowChanged {
        /// Display indexes in render priority order.
        display_indexes: Vec<u32>,
    },
    /// Requests one normal-mode continuation page.
    LoadNext,
    /// Retries retained retryable failures.
    Retry,
    /// Settles a command issued by the reducer.
    PageSettled {
        /// Command token.
        token: LibraryBrowseLoadToken,
        /// Page success or failure metadata.
        outcome: LibraryBrowsePageOutcome,
    },
}

/// Page load scheduling priority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum LibraryBrowseLoadPriority {
    /// Page-zero mode bootstrap.
    Bootstrap,
    /// Normal-mode continuation.
    Sequential,
    /// Visible or overscanned virtual page.
    Visible,
    /// Speculative virtual look-ahead.
    Prefetch,
    /// Explicit failure retry.
    Retry,
}

/// Cache behavior for a page load.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum LibraryBrowseCacheMode {
    /// Reuse an existing successful page first.
    ReuseSuccess,
    /// Reload instead of retaining a prior failure.
    Reload,
}

/// Ordered side effects returned to the TypeScript adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(tag = "tag", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum LibraryBrowseCommand {
    /// Reset the shared viewport after a source identity change.
    ResetViewport,
    /// Load one page and settle it with the included token.
    LoadPage {
        /// Unique load identity.
        token: LibraryBrowseLoadToken,
        /// Server record offset.
        start_index: u32,
        /// Maximum records requested.
        limit: u32,
        /// Scheduling priority.
        priority: LibraryBrowseLoadPriority,
        /// Requested cache behavior.
        cache_mode: LibraryBrowseCacheMode,
    },
    /// Cancel obsolete in-flight work.
    CancelLoad {
        /// Obsolete load identity.
        token: LibraryBrowseLoadToken,
    },
    /// Drop item payloads that left the retained window.
    ReleasePages {
        /// Sorted server page starts.
        page_starts: Vec<u32>,
    },
}

/// Active browse rendering strategy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum LibraryBrowseMode {
    /// Contiguous small-result paging.
    Normal,
    /// Random-access virtual paging.
    Virtual,
}

/// User-visible reducer lifecycle state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(tag = "tag", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum LibraryBrowseStatus {
    /// Loading is disabled.
    Inactive,
    /// Page zero has not settled yet.
    Loading,
    /// The source contains no records.
    Empty {
        /// Complete result count.
        total_record_count: u32,
    },
    /// Page zero failed.
    InitialFailure {
        /// Retained page-zero failure.
        failure: LibraryBrowseFailure,
        /// Whether retry work is queued or in flight.
        retry_busy: bool,
    },
    /// Page zero established a non-empty result.
    Ready {
        /// Normal or virtual rendering mode.
        mode: LibraryBrowseMode,
        /// Complete result count.
        total_record_count: u32,
        /// Whether a continuation page is loading.
        is_fetching_more: bool,
        /// Whether normal mode can request another sequential page.
        can_load_next: bool,
        /// First retained continuation failure.
        load_more_failure: Option<LibraryBrowseFailure>,
        /// Whether retry work is queued or in flight.
        retry_busy: bool,
    },
}

/// Virtual display-index mapping into the external page store.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub struct LibraryBrowseSlot {
    /// Position in display order.
    pub display_index: u32,
    /// Server page containing the position.
    pub page_start: u32,
    /// Offset inside the server page.
    pub index_within_page: u32,
}

/// Immutable view of the reducer state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub struct LibraryBrowseSnapshot {
    /// User-visible lifecycle state.
    pub status: LibraryBrowseStatus,
    /// Current virtual display mappings.
    pub slots: Vec<LibraryBrowseSlot>,
    /// Number of issued loads awaiting settlement.
    pub pending_count: u32,
}

/// Reducer transition returned to TypeScript.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub struct LibraryBrowseUpdate {
    /// State after applying the event.
    pub snapshot: LibraryBrowseSnapshot,
    /// Effects to execute in order.
    pub commands: Vec<LibraryBrowseCommand>,
}

/// Stateful WebAssembly façade over the synchronous Rust reducer.
#[wasm_bindgen]
pub struct LibraryBrowseCore {
    inner: Core,
}

#[wasm_bindgen]
impl LibraryBrowseCore {
    /// Creates an inactive library browse core.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self { inner: Core::new() }
    }

    /// Applies one typed event and returns the typed reducer update.
    #[cfg(target_arch = "wasm32")]
    pub fn dispatch(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = LibraryBrowseEvent)] event: JsValue,
    ) -> Result<LibraryBrowseUpdate, JsError> {
        // Converting inside the body lets wasm-bindgen release its mutable receiver borrow when
        // malformed JavaScript is rejected; FromWasmAbi conversion would throw before cleanup.
        let event = LibraryBrowseEvent::from_js(event)
            .map_err(|error| JsError::new(&format!("invalid library browse event: {error}")))?;
        self.dispatch_typed(event)
    }

    /// Applies one typed event and returns the typed reducer update.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn dispatch(&mut self, event: LibraryBrowseEvent) -> Result<LibraryBrowseUpdate, JsError> {
        self.dispatch_typed(event)
    }

    fn dispatch_typed(
        &mut self,
        event: LibraryBrowseEvent,
    ) -> Result<LibraryBrowseUpdate, JsError> {
        self.inner
            .dispatch(event.into())
            .map(Into::into)
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// Returns the current typed reducer snapshot.
    #[must_use]
    pub fn snapshot(&self) -> LibraryBrowseSnapshot {
        self.inner.snapshot().into()
    }
}

impl Default for LibraryBrowseCore {
    fn default() -> Self {
        Self::new()
    }
}

impl From<LibraryBrowseLoadToken> for CoreLoadToken {
    fn from(value: LibraryBrowseLoadToken) -> Self {
        Self {
            generation: value.generation,
            sequence: value.sequence,
        }
    }
}

impl From<CoreLoadToken> for LibraryBrowseLoadToken {
    fn from(value: CoreLoadToken) -> Self {
        Self {
            generation: value.generation,
            sequence: value.sequence,
        }
    }
}

impl From<LibraryBrowseFailure> for CoreFailure {
    fn from(value: LibraryBrowseFailure) -> Self {
        Self {
            message: value.message,
            retryable: value.retryable,
        }
    }
}

impl From<CoreFailure> for LibraryBrowseFailure {
    fn from(value: CoreFailure) -> Self {
        Self {
            message: value.message,
            retryable: value.retryable,
        }
    }
}

impl From<LibraryBrowsePageOutcome> for CorePageOutcome {
    fn from(value: LibraryBrowsePageOutcome) -> Self {
        match value {
            LibraryBrowsePageOutcome::Loaded {
                start_index,
                limit,
                total_record_count,
                item_count,
                has_more,
            } => Self::Loaded {
                start_index,
                limit,
                total_record_count,
                item_count,
                has_more,
            },
            LibraryBrowsePageOutcome::Failed { failure } => Self::Failed {
                failure: failure.into(),
            },
        }
    }
}

impl From<LibraryBrowseEvent> for CoreAction {
    fn from(value: LibraryBrowseEvent) -> Self {
        match value {
            LibraryBrowseEvent::Configure { source_id, enabled } => {
                Self::Configure { source_id, enabled }
            }
            LibraryBrowseEvent::WindowChanged { display_indexes } => {
                Self::WindowChanged { display_indexes }
            }
            LibraryBrowseEvent::LoadNext => Self::LoadNext,
            LibraryBrowseEvent::Retry => Self::Retry,
            LibraryBrowseEvent::PageSettled { token, outcome } => Self::PageSettled {
                token: token.into(),
                outcome: outcome.into(),
            },
        }
    }
}

impl From<CoreLoadPriority> for LibraryBrowseLoadPriority {
    fn from(value: CoreLoadPriority) -> Self {
        match value {
            CoreLoadPriority::Bootstrap => Self::Bootstrap,
            CoreLoadPriority::Sequential => Self::Sequential,
            CoreLoadPriority::Visible => Self::Visible,
            CoreLoadPriority::Prefetch => Self::Prefetch,
            CoreLoadPriority::Retry => Self::Retry,
        }
    }
}

impl From<CoreCacheMode> for LibraryBrowseCacheMode {
    fn from(value: CoreCacheMode) -> Self {
        match value {
            CoreCacheMode::ReuseSuccess => Self::ReuseSuccess,
            CoreCacheMode::Reload => Self::Reload,
        }
    }
}

impl From<CoreCommand> for LibraryBrowseCommand {
    fn from(value: CoreCommand) -> Self {
        match value {
            CoreCommand::ResetViewport => Self::ResetViewport,
            CoreCommand::LoadPage {
                token,
                start_index,
                limit,
                priority,
                cache_mode,
            } => Self::LoadPage {
                token: token.into(),
                start_index,
                limit,
                priority: priority.into(),
                cache_mode: cache_mode.into(),
            },
            CoreCommand::CancelLoad { token } => Self::CancelLoad {
                token: token.into(),
            },
            CoreCommand::ReleasePages { page_starts } => Self::ReleasePages { page_starts },
        }
    }
}

impl From<CoreMode> for LibraryBrowseMode {
    fn from(value: CoreMode) -> Self {
        match value {
            CoreMode::Normal => Self::Normal,
            CoreMode::Virtual => Self::Virtual,
        }
    }
}

impl From<CoreStatus> for LibraryBrowseStatus {
    fn from(value: CoreStatus) -> Self {
        match value {
            CoreStatus::Inactive => Self::Inactive,
            CoreStatus::Loading => Self::Loading,
            CoreStatus::Empty { total_record_count } => Self::Empty { total_record_count },
            CoreStatus::InitialFailure {
                failure,
                retry_busy,
            } => Self::InitialFailure {
                failure: failure.into(),
                retry_busy,
            },
            CoreStatus::Ready {
                mode,
                total_record_count,
                is_fetching_more,
                can_load_next,
                load_more_failure,
                retry_busy,
            } => Self::Ready {
                mode: mode.into(),
                total_record_count,
                is_fetching_more,
                can_load_next,
                load_more_failure: load_more_failure.map(Into::into),
                retry_busy,
            },
        }
    }
}

impl From<CoreSlot> for LibraryBrowseSlot {
    fn from(value: CoreSlot) -> Self {
        Self {
            display_index: value.display_index,
            page_start: value.page_start,
            index_within_page: value.index_within_page,
        }
    }
}

impl From<CoreSnapshot> for LibraryBrowseSnapshot {
    fn from(value: CoreSnapshot) -> Self {
        Self {
            status: value.status.into(),
            slots: value.slots.into_iter().map(Into::into).collect(),
            pending_count: value.pending_count,
        }
    }
}

impl From<CoreUpdate> for LibraryBrowseUpdate {
    fn from(value: CoreUpdate) -> Self {
        Self {
            snapshot: value.snapshot.into(),
            commands: value.commands.into_iter().map(Into::into).collect(),
        }
    }
}
