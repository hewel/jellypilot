//! Shared playback controls used by both Tauri commands and tray actions.

use tauri_specta::Event;

use crate::command::{CommandError, JellyfinState, NowPlayingChanged, NowPlayingState};
use crate::mpv::MpvClient;
use crate::now_playing::{build_now_playing_state, collect_player_state, PlaybackContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjacentDirection {
  Next,
  Previous,
}

impl AdjacentDirection {
  fn unavailable_message(self) -> &'static str {
    match self {
      Self::Next => "Next episode is available during episode playback",
      Self::Previous => "Previous episode is available during episode playback",
    }
  }
}

pub async fn collect_now_playing_state(state: &JellyfinState) -> NowPlayingState {
  let player = collect_player_state(&state.mpv).await;
  let session = state.session.read().clone();
  let current_item = session.as_ref().and_then(|session| session.current_item());

  build_now_playing_state(
    player,
    PlaybackContext {
      has_active_session: session.is_some(),
      current_item: current_item.as_ref(),
    },
  )
}

pub async fn emit_now_playing_changed(app: &tauri::AppHandle, state: &JellyfinState) {
  let session = state.session.read().clone();
  match session {
    // Every session-backed path emits through the shared snapshot owner.
    Some(session) => session.emit_now_playing_snapshot().await,
    // Without a session there is no snapshot owner; keep the cold
    // live-collection reconciliation for the disconnected projection.
    None => {
      let event = NowPlayingChanged {
        state: collect_now_playing_state(state).await,
      };
      if let Err(e) = event.emit(app) {
        log::error!("Failed to emit now playing state: {}", e);
      }
    }
  }
}

pub async fn set_pause(
  app: &tauri::AppHandle,
  mpv: &MpvClient,
  jellyfin_state: &JellyfinState,
  paused: bool,
) -> Result<(), CommandError> {
  mpv
    .set_pause(paused)
    .await
    .map_err(|e| CommandError::internal(e.to_string()))?;
  if let Some(session) = jellyfin_state.session.read().clone() {
    session.seed_transport(|transport| {
      transport.apply_property("pause", &serde_json::json!(paused));
    });
  }
  emit_now_playing_changed(app, jellyfin_state).await;
  Ok(())
}

pub async fn toggle_pause(
  app: &tauri::AppHandle,
  mpv: &MpvClient,
  jellyfin_state: &JellyfinState,
) -> Result<(), CommandError> {
  let is_paused = mpv
    .get_pause()
    .await
    .map_err(|e| CommandError::internal(e.to_string()))?;
  set_pause(app, mpv, jellyfin_state, !is_paused).await
}

pub async fn toggle_mute(
  app: &tauri::AppHandle,
  mpv: &MpvClient,
  jellyfin_state: &JellyfinState,
) -> Result<(), CommandError> {
  mpv
    .toggle_mute()
    .await
    .map_err(|e| CommandError::internal(e.to_string()))?;
  if let Some(session) = jellyfin_state.session.read().clone() {
    session.seed_transport(|transport| {
      let muted = !transport.muted();
      transport.apply_property("mute", &serde_json::json!(muted));
    });
  }
  emit_now_playing_changed(app, jellyfin_state).await;
  Ok(())
}

pub async fn play_adjacent_episode(
  app: &tauri::AppHandle,
  state: &JellyfinState,
  direction: AdjacentDirection,
) -> Result<(), CommandError> {
  let session = state
    .session
    .read()
    .clone()
    .ok_or_else(|| CommandError::invalid_input(direction.unavailable_message()))?;

  let result = match direction {
    AdjacentDirection::Next => session.play_next_episode().await,
    AdjacentDirection::Previous => session.play_previous_episode().await,
  };
  result.map_err(CommandError::invalid_input)?;
  emit_now_playing_changed(app, state).await;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn adjacent_control_errors_match_ui_contract() {
    assert_eq!(
      AdjacentDirection::Next.unavailable_message(),
      "Next episode is available during episode playback"
    );
    assert_eq!(
      AdjacentDirection::Previous.unavailable_message(),
      "Previous episode is available during episode playback"
    );
  }
}
