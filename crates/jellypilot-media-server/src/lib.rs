//! UI-independent Jellyfin and Emby clients, artwork loading policy, and media data helpers.

pub mod artwork;
mod artwork_cache;
mod client;
mod error;
pub mod home;
mod image_ref;
mod intro_skipper;
mod types;

pub use artwork_cache::{
  artwork_cache_key, ArtworkCacheStats, ArtworkDiskCache, MAX_DISK_CACHE_BYTES,
};

pub use client::{
  JellyfinClient, JellyfinLibrary, JellyfinLogin, JellyfinPlayback, LibraryImageRequest,
};
pub use error::JellyfinError;
pub use image_ref::{
  image_id_for_url, normalize_server_url, sized_origin_url, ImageRefError, ImageRefKind,
};
pub use intro_skipper::{
  evaluate_manual_skip, evaluate_skip, evaluate_skip_prompt, IntroSkipDecision, IntroSkipKind,
  IntroSkipRange,
};
pub use types::*;
