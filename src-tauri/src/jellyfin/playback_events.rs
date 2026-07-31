//! Playback Target MPV event and progress orchestration.
//!
//! One internal module owns the MPV event stream: property-change handling
//! for pause, volume, mute, position, and duration; immediate versus
//! throttled progress reporting; Intro Skipper decisions; end-file and
//! client-message handling; and disconnect cleanup when the event receiver
//! closes. The loop owns when to refresh but delegates Now Playing state
//! projection and emission to the shared owner, so SessionManager carries no
//! second Now Playing builder and no new Tauri event adapter; event names
//! and payloads stay unchanged.

use std::sync::Arc;

use parking_lot::RwLock;
use tauri::AppHandle;
use tokio::sync::mpsc;

use super::intro_skipper::{
  evaluate_manual_skip, evaluate_skip, evaluate_skip_prompt, IntroSkipKind,
};
use super::mpv_action::MpvAction;
use super::mpv_event::{
  apply_property_update, client_message_direction, is_natural_end, property_report_decision,
  should_report_progress, PropertyReportDecision,
};
use super::session::{PlayContext, SessionManager, SessionState};
use super::types::*;
use crate::config::{AppConfig, IntroSkipperMode};
use crate::hls_proxy::HlsProxyState;
use crate::jellyfin::client::JellyfinClient;
use crate::mpv::MpvClient;

/// Start the Playback Target MPV event listener for property changes,
/// end-of-file detection, and keyboard shortcuts. This is the main
/// event-driven loop that handles:
/// - Property observations (pause, volume, mute) for immediate UI sync
/// - Periodic time-pos reporting for progress
/// - End-file events for auto-play next episode
/// - Client-message events for keyboard shortcuts
/// - Receiver closure as MPV disconnect
///
/// The loop owns when to refresh; state projection and emission delegate to
/// the shared Now Playing owner.
#[allow(clippy::too_many_arguments)]
pub(super) fn start_mpv_event_listener(
  mpv: MpvClient,
  client: Arc<JellyfinClient>,
  state: Arc<RwLock<SessionState>>,
  action_tx: mpsc::Sender<MpvAction>,
  config: Arc<RwLock<AppConfig>>,
  app_handle: AppHandle,
  hls: HlsProxyState,
) {
  tokio::spawn(async move {
    log::info!("MPV event listener started");

    // Wait a bit for MPV to connect before trying to get events
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let ctx = PlayContext {
      client: client.clone(),
      state: state.clone(),
      action_tx: action_tx.clone(),
      hls: hls.clone(),
      app: Some(app_handle.clone()),
      config: config.clone(),
    };

    loop {
      // Try to get the event receiver
      let event_rx = match mpv.events() {
        Some(rx) => rx,
        None => {
          // MPV not connected yet, wait and retry
          tokio::time::sleep(std::time::Duration::from_secs(2)).await;
          continue;
        }
      };

      log::info!("Got MPV event receiver, setting up property observations...");

      // Observer IDs for different properties
      const OBS_PAUSE: i64 = 1;
      const OBS_VOLUME: i64 = 2;
      const OBS_MUTE: i64 = 3;
      const OBS_TIME_POS: i64 = 4;
      const OBS_DURATION: i64 = 5;

      // Set up property observations
      if let Err(e) = mpv.observe_property(OBS_PAUSE, "pause").await {
        log::warn!("Failed to observe pause: {}", e);
      }
      if let Err(e) = mpv.observe_property(OBS_VOLUME, "volume").await {
        log::warn!("Failed to observe volume: {}", e);
      }
      if let Err(e) = mpv.observe_property(OBS_MUTE, "mute").await {
        log::warn!("Failed to observe mute: {}", e);
      }
      if let Err(e) = mpv.observe_property(OBS_TIME_POS, "time-pos").await {
        log::warn!("Failed to observe time-pos: {}", e);
      }
      if let Err(e) = mpv.observe_property(OBS_DURATION, "duration").await {
        log::warn!("Failed to observe duration: {}", e);
      }

      log::info!("Property observations set up, listening for events...");

      // Track last progress report time to throttle time-pos updates
      let mut last_progress_report = std::time::Instant::now();
      let progress_report_interval = std::time::Duration::from_secs(5);

      // Process events
      while let Ok(event) = event_rx.recv().await {
        match event.event.as_str() {
          "property-change" => {
            let property_name = event.name.as_deref().unwrap_or("");
            // Every observed property feeds the Now Playing transport
            // snapshot, including ones that never trigger a report.
            update_transport_from_property(&state, &event);
            let decision = property_report_decision(property_name);
            let should_report = if decision == PropertyReportDecision::Ignore {
              false
            } else {
              update_state_from_property(&state, &event);
              if property_name == "time-pos" {
                apply_intro_skipper(&state, &action_tx, &event).await;
              }

              let now = std::time::Instant::now();
              let should_report = should_report_progress(
                decision,
                now,
                last_progress_report,
                progress_report_interval,
              );
              if should_report && decision == PropertyReportDecision::ReportWhenThrottleElapsed {
                last_progress_report = now;
              }
              should_report
            };

            if should_report {
              report_progress(&client, &state).await;
              SessionManager::emit_now_playing_changed(&app_handle, &state).await;
            }
          }
          "end-file" => {
            handle_end_file_event(&event, &ctx).await;
            SessionManager::emit_now_playing_changed(&app_handle, &state).await;
          }
          "client-message" => {
            handle_client_message_event(&event, &ctx).await;
            SessionManager::emit_now_playing_changed(&app_handle, &state).await;
          }
          "seek" => {
            // A seek invalidates every prefetched lookahead window
            let proxy_session_id = {
              let s = state.read();
              s.playback
                .as_ref()
                .and_then(|playback| playback.hls_proxy_session_id.clone())
            };
            if let Some(proxy_session_id) = proxy_session_id {
              if let Ok(proxy) = hls.current() {
                proxy.cancel_prefetch(&proxy_session_id);
              }
            }
          }
          _ => {
            // Ignore other events
          }
        }
      }

      // MPV event receiver closed - this means MPV died or disconnected
      // Clear playback context and notify Jellyfin
      log::warn!("MPV event receiver closed, clearing playback context...");
      clear_playback_context(&client, &state, &hls).await;
      SessionManager::emit_now_playing_changed(&app_handle, &state).await;
      tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
  });
}

/// Update the Now Playing transport snapshot from a property-change event.
pub(super) fn update_transport_from_property(
  state: &RwLock<SessionState>,
  event: &crate::mpv::MpvEvent,
) {
  let property_name = event.name.as_deref().unwrap_or("");
  let Some(data) = event.data.as_ref() else {
    return;
  };

  state.write().transport.apply_property(property_name, data);
}

/// Update session state from a property-change event.
pub(super) fn update_state_from_property(
  state: &RwLock<SessionState>,
  event: &crate::mpv::MpvEvent,
) {
  let property_name = event.name.as_deref().unwrap_or("");
  let data = match &event.data {
    Some(d) => d,
    None => return,
  };

  let mut s = state.write();
  let playback = match s.playback.as_mut() {
    Some(p) => p,
    None => return,
  };

  apply_property_update(playback, property_name, data);
}

/// Apply Intro Skipper seek decisions for a time-position update.
pub(super) async fn apply_intro_skipper(
  state: &RwLock<SessionState>,
  action_tx: &mpsc::Sender<MpvAction>,
  event: &crate::mpv::MpvEvent,
) {
  let intro_skipper_config = {
    let state = state.read();
    state.effective_intro_skipper_config.clone()
  };

  if intro_skipper_config.mode == IntroSkipperMode::Off {
    return;
  }

  if event.name.as_deref() != Some("time-pos") {
    return;
  }

  let Some(position_seconds) = event.data.as_ref().and_then(|data| data.as_f64()) else {
    return;
  };

  match intro_skipper_config.mode {
    IntroSkipperMode::Automatic => {
      let seek_target = {
        let mut s = state.write();
        s.playback
          .as_mut()
          .and_then(|playback| evaluate_skip(position_seconds, &mut playback.intro_skipper_ranges))
      };

      if let Some(seek_target) = seek_target {
        log::info!(
          "Intro Skipper seeking from {:.3}s to {:.3}s",
          position_seconds,
          seek_target
        );
        let _ = action_tx.send(MpvAction::Seek(seek_target)).await;
      }
    }
    IntroSkipperMode::Manual => {
      let prompt_kind = {
        let mut s = state.write();
        s.playback.as_mut().and_then(|playback| {
          evaluate_skip_prompt(position_seconds, &mut playback.intro_skipper_ranges)
        })
      };

      if let Some(kind) = prompt_kind {
        let _ = action_tx
          .send(MpvAction::ShowText {
            text: format!(
              "{} available - press {} to skip",
              intro_skipper_label(kind),
              intro_skipper_config.keybind_intro_skip
            ),
            duration_ms: 3000,
          })
          .await;
      }
    }
    IntroSkipperMode::Off => {}
  }
}

/// Report current playback progress to Jellyfin.
pub(super) async fn report_progress(client: &JellyfinClient, state: &RwLock<SessionState>) {
  let session = {
    let s = state.read();
    s.playback.clone()
  };

  let Some(session) = session else {
    return;
  };

  if session.hls_recovering {
    // Progress belongs to the old transcode generation during recovery
    return;
  }

  let progress = PlaybackProgressInfo {
    item_id: session.item_id.clone(),
    media_source_id: session.media_source_id.clone(),
    play_session_id: session.play_session_id.clone(),
    position_ticks: Some(session.position_ticks),
    is_paused: session.is_paused,
    is_muted: session.is_muted,
    volume_level: session.volume,
    audio_stream_index: session.audio_stream_index,
    subtitle_stream_index: session.subtitle_stream_index,
    play_method: session.play_method,
    can_seek: true,
  };

  log::debug!("Progress payload: {:?}", progress);

  if let Err(e) = client.playback().report_playback_progress(&progress).await {
    log::error!("Failed to report playback progress: {}", e);
  }
}

/// Handle MPV end-file event for auto-play next episode.
pub(super) async fn handle_end_file_event(event: &crate::mpv::MpvEvent, ctx: &PlayContext) {
  let reason = event.reason.as_deref().unwrap_or("");
  log::info!("MPV end-file event, reason: {}", reason);

  // "eof" means natural end of file, "stop" means user stopped
  if !is_natural_end(event.reason.as_deref()) {
    return;
  }

  // Get current item for next episode lookup
  let current_item = {
    let s = ctx.state.read();
    s.current_item.clone()
  };

  let Some(item) = current_item else {
    return;
  };

  log::info!("Playback ended naturally, checking for next episode...");

  // Report playback stopped to Jellyfin
  SessionManager::report_playback_stopped(&ctx.client, &ctx.state, &ctx.hls).await;

  // Try to get next episode
  if let Err(e) = SessionManager::play_adjacent_episode(ctx, &item, true, false).await {
    log::info!("Natural end did not start an adjacent episode: {}", e);
  }
}

/// Handle MPV client-message event for keyboard shortcuts.
///
/// Users can add to their input.conf:
///   Shift+> script-message jellypilot-next
///   Shift+< script-message jellypilot-prev
pub(super) async fn handle_client_message_event(event: &crate::mpv::MpvEvent, ctx: &PlayContext) {
  let args = match &event.args {
    Some(args) if !args.is_empty() => args,
    _ => return,
  };

  if args[0] == "jellypilot-skip-intro" {
    handle_manual_intro_skip(&ctx.state, &ctx.action_tx).await;
    return;
  }

  let Some(direction) = client_message_direction(args) else {
    log::debug!("Unknown client-message command: {}", args[0]);
    return;
  };

  let current_item = {
    let s = ctx.state.read();
    s.current_item.clone()
  };

  let Some(item) = current_item else {
    log::warn!("{}: No current item", args[0]);
    return;
  };

  let next = direction == crate::playback_control::AdjacentDirection::Next;
  log::info!(
    "Keyboard shortcut: playing {} episode",
    if next { "next" } else { "previous" }
  );
  if let Err(e) = SessionManager::play_adjacent_episode(ctx, &item, next, true).await {
    log::warn!("Keyboard shortcut {} unavailable: {}", args[0], e);
  }
}

pub(super) async fn handle_manual_intro_skip(
  state: &RwLock<SessionState>,
  action_tx: &mpsc::Sender<MpvAction>,
) {
  if state.read().effective_intro_skipper_config.mode != IntroSkipperMode::Manual {
    let _ = action_tx
      .send(MpvAction::ShowText {
        text: "No intro or credits to skip".to_string(),
        duration_ms: 1200,
      })
      .await;
    return;
  }

  let decision = {
    let mut s = state.write();
    s.playback.as_mut().and_then(|playback| {
      evaluate_manual_skip(
        ticks_to_seconds(playback.position_ticks),
        &mut playback.intro_skipper_ranges,
      )
    })
  };

  if let Some(decision) = decision {
    let _ = action_tx.send(MpvAction::Seek(decision.seek_target)).await;
    let _ = action_tx
      .send(MpvAction::ShowText {
        text: format!("Skipped {}", intro_skipper_label_lower(decision.kind)),
        duration_ms: 1500,
      })
      .await;
  } else {
    let _ = action_tx
      .send(MpvAction::ShowText {
        text: "No intro or credits to skip".to_string(),
        duration_ms: 1200,
      })
      .await;
  }
}

/// Clear all playback context - reports stop to Jellyfin and clears all state.
/// Call this when MPV dies unexpectedly or WebSocket disconnects during playback.
pub(super) async fn clear_playback_context(
  client: &JellyfinClient,
  state: &RwLock<SessionState>,
  hls: &HlsProxyState,
) {
  // First report stopped to Jellyfin
  SessionManager::report_playback_stopped(client, state, hls).await;

  // Then clear all related state
  let mut s = state.write();
  s.current_item = None;
  s.current_series_id = None;
  s.current_media_streams.clear();
  s.transport.clear();
  log::info!("Playback context cleared");
}

fn intro_skipper_label(kind: IntroSkipKind) -> &'static str {
  match kind {
    IntroSkipKind::Introduction => "Intro",
    IntroSkipKind::Credits => "Credits",
  }
}

fn intro_skipper_label_lower(kind: IntroSkipKind) -> &'static str {
  match kind {
    IntroSkipKind::Introduction => "intro",
    IntroSkipKind::Credits => "credits",
  }
}
