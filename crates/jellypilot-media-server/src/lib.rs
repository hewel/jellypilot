//! Concrete, UI-independent Jellyfin and Emby HTTP client used by JellyPilot frontends.

mod client;
mod error;
mod image_ref;
mod intro_skipper;
mod types;

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
