//! Jellyfin API client module.
//!
//! Handles authentication, WebSocket remote control, and playback reporting.

mod client;
mod error;
mod session;
mod types;
mod websocket;

pub use client::JellyfinClient;
pub use error::JellyfinError;
pub use session::SessionManager;
pub use types::*;
