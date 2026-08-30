//! Framework-independent application state and presentation logic for JellyPilot.
//!
//! This crate owns browse and request state machines, settings persistence,
//! diagnostic buffering, artwork correlation, and display-free page helpers.

pub mod artwork_binder;
#[cfg(feature = "native")]
pub mod artwork_loader;
#[cfg(feature = "native")]
pub mod browse;
#[cfg(feature = "native")]
pub mod browse_model;
#[cfg(feature = "native")]
pub mod cards;
#[cfg(feature = "native")]
pub mod config;
#[cfg(feature = "native")]
pub mod detail;
pub mod diagnostics;
mod load_state;
pub mod request_gate;
#[cfg(feature = "native")]
pub mod settings;
pub mod skeleton;

mod model;
mod reducer;

pub use load_state::LoadState;
pub use model::{
    LibraryBrowseAction, LibraryBrowseCacheMode, LibraryBrowseCommand, LibraryBrowseCoreError,
    LibraryBrowseFailure, LibraryBrowseLoadPriority, LibraryBrowseLoadToken, LibraryBrowseMode,
    LibraryBrowsePageOutcome, LibraryBrowseSlot, LibraryBrowseSnapshot, LibraryBrowseStatus,
    LibraryBrowseUpdate, LIBRARY_BROWSE_LOOKAHEAD_PAGES, LIBRARY_BROWSE_MAX_CONCURRENT_LOADS,
    LIBRARY_BROWSE_PAGE_SIZE, LIBRARY_BROWSE_VIRTUAL_THRESHOLD,
};
pub use reducer::LibraryBrowseCore;
