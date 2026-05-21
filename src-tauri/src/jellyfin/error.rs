//! Jellyfin error types.

use thiserror::Error;

/// Errors that can occur when interacting with Jellyfin.
#[derive(Debug, Error)]
pub enum JellyfinError {
  #[error("HTTP request failed: {0}")]
  Http(#[from] reqwest::Error),

  #[error("HTTP error: {0}")]
  HttpError(String),

  #[error("WebSocket error: {0}")]
  WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

  #[error("JSON serialization error: {0}")]
  Json(#[from] serde_json::Error),

  #[error("Authentication failed: {0}")]
  AuthFailed(String),

  #[error("Quick Connect is not enabled on this server")]
  QuickConnectUnavailable,

  #[error("Not connected to server")]
  NotConnected,

  #[error("Invalid server URL: {0}")]
  InvalidUrl(String),

  #[error("Session not found")]
  SessionNotFound,
}
