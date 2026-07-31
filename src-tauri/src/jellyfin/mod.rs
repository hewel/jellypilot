//! Jellyfin API client module.
//!
//! Handles authentication, WebSocket remote control, and playback reporting.

mod client;
#[cfg(test)]
mod client_facade;
mod error;
mod hls_lifecycle;
mod intro_skipper;
mod mpv_action;
mod mpv_event;
mod play_resolution;
mod playback_events;
mod session;
mod types;
mod websocket;

pub use client::JellyfinClient;
pub use error::JellyfinError;
pub use session::SessionManager;
pub use types::*;
