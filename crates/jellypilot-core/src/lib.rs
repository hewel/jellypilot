//! Framework-independent application state and presentation logic for JellyPilot.
//!
//! This crate owns browse and request state machines, settings persistence,
//! Intro Skipper policy, diagnostic buffering, artwork correlation, and display-free page helpers.

#[cfg(feature = "portable")]
pub mod audio_tracks;
#[cfg(feature = "portable")]
pub mod browse;
#[cfg(feature = "portable")]
pub mod browse_model;
#[cfg(feature = "portable")]
pub mod browse_window;
#[cfg(feature = "portable")]
pub mod cards;
pub mod collections;
#[cfg(feature = "portable")]
pub mod config;
#[cfg(feature = "portable")]
pub mod detail;
pub mod diagnostics;
#[cfg(feature = "portable")]
pub mod home_hero;
#[cfg(feature = "portable")]
pub mod image_lifecycle;
#[cfg(feature = "portable")]
pub mod intro_skipper;
mod load_state;
pub mod locale;
#[cfg(feature = "portable")]
pub mod logs;
pub mod player_logs;
pub mod request_gate;
#[cfg(feature = "portable")]
pub mod settings;
pub mod skeleton;
#[cfg(feature = "portable")]
pub mod volume_memory;
#[cfg(feature = "portable")]
pub mod watchlist;

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
