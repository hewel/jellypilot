//! Jellyfin API client module.
//!
//! Handles authentication, WebSocket remote control, and playback reporting.

mod client;
#[cfg(test)]
mod client_facade;
mod error;
mod hls_lifecycle;
mod mpv_action;
mod mpv_event;
mod play_resolution;
mod playback_events;
mod session;
mod types;

pub use client::JellyfinClient;
pub use error::JellyfinError;
pub use session::SessionManager;
pub use types::*;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn login_and_playback_facets_remain_available_through_tauri_adapter() {
    let client = JellyfinClient::new();

    super::client_facade::assert_login_interface(&client);
    super::client_facade::assert_playback_interface(&client);
  }
}
