//! Framework-independent application state and presentation logic for JellyPilot.
//!
//! This crate owns browse and request state machines, settings persistence,
//! Intro Skipper policy, diagnostic buffering, artwork correlation, and display-free page helpers.

#[cfg(feature = "native")]
pub mod audio_tracks;
#[cfg(feature = "native")]
pub mod browse;
#[cfg(feature = "native")]
pub mod browse_model;
#[cfg(feature = "native")]
pub mod browse_window;
#[cfg(feature = "native")]
pub mod cards;
pub mod collections;
#[cfg(feature = "native")]
pub mod config;
#[cfg(feature = "native")]
pub mod detail;
pub mod diagnostics;
#[cfg(feature = "native")]
pub mod home_hero;
#[cfg(feature = "native")]
pub mod image_lifecycle;
#[cfg(feature = "native")]
pub mod intro_skipper;
mod load_state;
pub mod locale;
#[cfg(feature = "native")]
pub mod logs;
pub mod player_logs;
pub mod request_gate;
#[cfg(feature = "native")]
pub mod settings;
pub mod skeleton;
#[cfg(feature = "native")]
pub mod volume_memory;
#[cfg(feature = "native")]
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
