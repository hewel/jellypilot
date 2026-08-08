//! Framework-independent state machines for JellyPilot.
//!
//! The library browse core is a synchronous, metadata-only reducer. Callers
//! execute its commands and return page metadata through settlement events;
//! item payloads remain outside this crate.

mod model;
mod reducer;

pub use model::{
    LibraryBrowseAction, LibraryBrowseCacheMode, LibraryBrowseCommand, LibraryBrowseCoreError,
    LibraryBrowseFailure, LibraryBrowseLoadPriority, LibraryBrowseLoadToken, LibraryBrowseMode,
    LibraryBrowsePageOutcome, LibraryBrowseSlot, LibraryBrowseSnapshot, LibraryBrowseStatus,
    LibraryBrowseUpdate, LIBRARY_BROWSE_LOOKAHEAD_PAGES, LIBRARY_BROWSE_MAX_CONCURRENT_LOADS,
    LIBRARY_BROWSE_PAGE_SIZE, LIBRARY_BROWSE_VIRTUAL_THRESHOLD,
};
pub use reducer::LibraryBrowseCore;
