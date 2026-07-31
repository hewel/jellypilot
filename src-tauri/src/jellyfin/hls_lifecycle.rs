//! Emby HLS lifecycle for Playback Target sessions.
//!
//! One internal module owns HLS proxy selection and activation for finite
//! Emby HLS transcodes, one-shot origin recovery at the current position,
//! the terminal state, proxy event consumption, and the notification copy for
//! each outcome. Origin tokens never reach MPV: only the local proxy playlist
//! URL does. Non-HLS Emby playback, missing runtimes, and Jellyfin playback
//! bypass the proxy path; cleanup on stop, replacement, next episode,
//! disconnect, and teardown stays in the session entry points that deactivate
//! the owned proxy session.

use parking_lot::RwLock;
use tauri::AppHandle;

use super::mpv_action::MpvAction;
use super::play_resolution::{resolve_play_request, PlayResolutionConfig};
use super::session::{PlayContext, SessionState};
use super::types::*;
use crate::command::AppNotification;
use crate::hls_proxy::{ActivatedHls, HlsProxyError, HlsProxyEvent, HlsProxyState};
use crate::jellyfin::client::JellyfinClient;

const HLS_PROXY_UNAVAILABLE_WARNING: &str =
  "HLS proxy is unavailable; streaming directly from Emby.";
const HLS_CACHE_UNAVAILABLE_WARNING: &str =
  "HLS cache is unavailable; streaming this transcode without disk caching.";
const HLS_PROXY_FAILED_ERROR: &str =
  "HLS proxy could not process the Emby stream. Restart playback to continue.";
const HLS_RECOVERY_FAILED_ERROR: &str =
  "The Emby transcode session expired again. Restart playback to continue.";

/// Playback URL choice after optional HLS proxy activation.
pub(super) struct PlaybackUrlDecision {
  pub(super) url: String,
  pub(super) activated: Option<ActivatedHls>,
  pub(super) warning: Option<&'static str>,
}

/// Snapshot of an expired Emby transcode generation used during recovery.
struct RecoverySnapshot {
  item_id: String,
  media_source_id: Option<String>,
  play_session_id: Option<String>,
  position_ticks: i64,
  audio_stream_index: Option<i32>,
  subtitle_stream_index: Option<i32>,
  is_paused: bool,
  is_muted: bool,
  volume: i32,
}

/// Emit a user-facing notification, or record it when no AppHandle is available.
pub(super) fn notify(
  app: Option<&AppHandle>,
  state: &RwLock<SessionState>,
  level: &str,
  message: &str,
) {
  match app {
    Some(app) if level == "error" => AppNotification::error(app, message),
    Some(app) => AppNotification::warning(app, message),
    None => state
      .write()
      .recorded_notifications
      .push((level.to_string(), message.to_string())),
  }
}

/// Choose the playback URL, activating the HLS proxy for finite Emby HLS transcodes.
pub(super) async fn resolve_playback_url(
  client: &JellyfinClient,
  hls: &HlsProxyState,
  item: &MediaItem,
  play_method: &str,
  remote_url: String,
) -> PlaybackUrlDecision {
  fn bypass(remote_url: String, warning: Option<&'static str>) -> PlaybackUrlDecision {
    PlaybackUrlDecision {
      url: remote_url,
      activated: None,
      warning,
    }
  }
  let eligible = client.provider() == MediaServerProvider::Emby
    && play_method == "Transcode"
    && item.run_time_ticks.is_some();
  if !eligible {
    return bypass(remote_url, None);
  }

  let proxy = match hls.current() {
    Ok(proxy) => proxy,
    Err(e) => {
      log::warn!("HLS proxy unavailable, streaming directly: {}", e);
      return bypass(remote_url, Some(HLS_PROXY_UNAVAILABLE_WARNING));
    }
  };
  let origin = match url::Url::parse(&remote_url) {
    Ok(origin) => origin,
    Err(e) => {
      log::warn!("Failed to parse Emby stream URL for HLS proxy: {}", e);
      return bypass(remote_url, Some(HLS_PROXY_UNAVAILABLE_WARNING));
    }
  };

  match proxy.activate(origin).await {
    Ok(activated) => {
      let warning = (!activated.cache_enabled).then_some(HLS_CACHE_UNAVAILABLE_WARNING);
      PlaybackUrlDecision {
        url: activated.playlist_url.clone(),
        activated: Some(activated),
        warning,
      }
    }
    Err(HlsProxyError::UnsupportedContent) => {
      log::info!("Emby transcode is not HLS; streaming the original URL");
      bypass(remote_url, None)
    }
    Err(e) => {
      log::warn!("HLS proxy activation failed, streaming directly: {}", e);
      bypass(remote_url, Some(HLS_PROXY_UNAVAILABLE_WARNING))
    }
  }
}

/// Consume events from one HLS proxy activation until its channel closes.
pub(super) fn start_hls_event_consumer(activated: ActivatedHls, ctx: PlayContext) {
  let session_id = activated.session_id.clone();
  tokio::spawn(async move {
    while let Ok(event) = activated.events.recv().await {
      let current = {
        let s = ctx.state.read();
        s.playback.as_ref().map(|playback| {
          (
            playback.hls_proxy_session_id.clone(),
            playback.hls_recovery_attempted,
          )
        })
      };
      let Some((current_id, recovery_attempted)) = current else {
        continue;
      };
      // Stale generations may still deliver events; ignore them
      if current_id.as_deref() != Some(session_id.as_str()) {
        continue;
      }
      match event {
        HlsProxyEvent::CacheDisabled => {
          notify(
            ctx.app.as_ref(),
            &ctx.state,
            "warning",
            HLS_CACHE_UNAVAILABLE_WARNING,
          );
        }
        HlsProxyEvent::PlaybackFailed => {
          notify(
            ctx.app.as_ref(),
            &ctx.state,
            "error",
            HLS_PROXY_FAILED_ERROR,
          );
        }
        HlsProxyEvent::OriginExpired => {
          if recovery_attempted {
            // The replacement generation expired again; never loop recovery
            enter_hls_terminal_state(&ctx.state, ctx.app.as_ref());
          } else {
            recover_expired_transcode(&session_id, &ctx).await;
          }
        }
      }
    }
  });
}

/// Move an unrecoverable transcode session into its terminal state.
pub(super) fn enter_hls_terminal_state(state: &RwLock<SessionState>, app: Option<&AppHandle>) {
  {
    let mut s = state.write();
    if let Some(playback) = s.playback.as_mut() {
      // No active play session: suppresses progress and duplicate stop reports
      playback.play_session_id = None;
      playback.hls_recovering = true;
    }
  }
  notify(app, state, "error", HLS_RECOVERY_FAILED_ERROR);
}

/// Recover an expired Emby transcode session once, at the current position.
async fn recover_expired_transcode(old_proxy_session_id: &str, ctx: &PlayContext) {
  // Mark the one-shot attempt and capture the old generation atomically
  let snapshot = {
    let mut s = ctx.state.write();
    let Some(playback) = s.playback.as_mut() else {
      return;
    };
    playback.hls_recovery_attempted = true;
    playback.hls_recovering = true;
    RecoverySnapshot {
      item_id: playback.item_id.clone(),
      media_source_id: playback.media_source_id.clone(),
      // Clearing the stored ID here guarantees no duplicate stop report later
      play_session_id: playback.play_session_id.take(),
      position_ticks: playback.position_ticks,
      audio_stream_index: playback.audio_stream_index,
      subtitle_stream_index: playback.subtitle_stream_index,
      is_paused: playback.is_paused,
      is_muted: playback.is_muted,
      volume: playback.volume,
    }
  };

  // 1. Stop the old Emby play session without clearing the current item
  if let Some(play_session_id) = snapshot.play_session_id.clone() {
    let stop_info = PlaybackStopInfo {
      item_id: snapshot.item_id.clone(),
      media_source_id: snapshot.media_source_id.clone(),
      play_session_id: Some(play_session_id),
      position_ticks: Some(snapshot.position_ticks),
    };
    if let Err(e) = ctx.client.playback().report_playback_stop(&stop_info).await {
      log::error!("Failed to stop expired Emby transcode session: {}", e);
      enter_hls_terminal_state(&ctx.state, ctx.app.as_ref());
      return;
    }
  }

  // 2. Request fresh playback info at the current position and selected streams
  let playback_info = match ctx
    .client
    .playback()
    .get_playback_info(
      &snapshot.item_id,
      Some(snapshot.position_ticks),
      snapshot.audio_stream_index,
      snapshot.subtitle_stream_index,
    )
    .await
  {
    Ok(info) => info,
    Err(e) => {
      log::error!("Failed to refresh Emby playback info after expiry: {}", e);
      enter_hls_terminal_state(&ctx.state, ctx.app.as_ref());
      return;
    }
  };

  // 3. Prefer the prior media source, falling back to the first one
  let media_source = playback_info
    .media_sources
    .iter()
    .find(|source| Some(&source.id) == snapshot.media_source_id.as_ref())
    .or_else(|| playback_info.media_sources.first());
  let Some(media_source) = media_source else {
    log::error!("Fresh Emby playback info has no media sources");
    enter_hls_terminal_state(&ctx.state, ctx.app.as_ref());
    return;
  };

  // 4. Resolve tracks and play method for the fresh media source
  let (item, series_preference, preferred_subtitle_languages) = {
    let s = ctx.state.read();
    let item = s.current_item.clone();
    let series_preference = item
      .as_ref()
      .and_then(|item| item.series_id.as_ref())
      .and_then(|series_id| s.series_preferences.get(series_id).cloned());
    let languages = ctx.config.read().preferred_subtitle_languages.clone();
    (item, series_preference, languages)
  };
  let Some(item) = item else {
    log::error!("No current item during Emby transcode recovery");
    enter_hls_terminal_state(&ctx.state, ctx.app.as_ref());
    return;
  };
  let play_request = PlayRequest {
    item_ids: vec![snapshot.item_id.clone()],
    start_position_ticks: Some(snapshot.position_ticks),
    play_command: "PlayNow".to_string(),
    media_source_id: Some(media_source.id.clone()),
    audio_stream_index: snapshot.audio_stream_index,
    subtitle_stream_index: snapshot.subtitle_stream_index,
  };
  let resolution = resolve_play_request(
    &play_request,
    &item,
    &playback_info,
    media_source,
    series_preference.as_ref(),
    PlayResolutionConfig {
      preferred_subtitle_languages: &preferred_subtitle_languages,
      // Existing intro ranges survive the recovery; nothing new is fetched
      intro_skipper_enabled: false,
    },
  );

  let remote_url = match ctx
    .client
    .playback()
    .build_stream_url(&snapshot.item_id, media_source)
  {
    Some(url) => url,
    None => {
      log::error!("Failed to build fresh Emby stream URL after expiry");
      enter_hls_terminal_state(&ctx.state, ctx.app.as_ref());
      return;
    }
  };

  // 5. Retire the old proxy generation before activating a fresh one
  if let Ok(proxy) = ctx.hls.current() {
    proxy.deactivate(old_proxy_session_id);
  }
  let decision = resolve_playback_url(
    &ctx.client,
    &ctx.hls,
    &item,
    resolution.play_method,
    remote_url,
  )
  .await;
  if let Some(warning) = decision.warning {
    notify(ctx.app.as_ref(), &ctx.state, "warning", warning);
  }

  // 6. Report the fresh play session, then swap stored identifiers atomically
  let start_info = PlaybackStartInfo {
    item_id: snapshot.item_id.clone(),
    media_source_id: Some(media_source.id.clone()),
    play_session_id: playback_info.play_session_id.clone(),
    position_ticks: Some(snapshot.position_ticks),
    is_paused: snapshot.is_paused,
    is_muted: snapshot.is_muted,
    volume_level: snapshot.volume,
    audio_stream_index: resolution.audio_stream_index,
    subtitle_stream_index: resolution.subtitle_stream_index,
    play_method: resolution.play_method.to_string(),
    can_seek: true,
  };
  if let Err(e) = ctx
    .client
    .playback()
    .report_playback_start(&start_info)
    .await
  {
    log::error!("Failed to report fresh Emby playback start: {}", e);
    enter_hls_terminal_state(&ctx.state, ctx.app.as_ref());
    return;
  }

  {
    let mut s = ctx.state.write();
    if let Some(playback) = s.playback.as_mut() {
      playback.media_source_id = Some(media_source.id.clone());
      playback.play_session_id = playback_info.play_session_id.clone();
      playback.hls_proxy_session_id = decision
        .activated
        .as_ref()
        .map(|activated| activated.session_id.clone());
      playback.audio_stream_index = resolution.audio_stream_index;
      playback.subtitle_stream_index = resolution.subtitle_stream_index;
      playback.play_method = resolution.play_method.to_string();
      playback.hls_recovery_attempted = true;
      playback.hls_recovering = false;
    }
    s.current_media_streams = media_source.media_streams.clone();
  }

  // Resume playback at the recovered position
  let _ = ctx
    .action_tx
    .send(MpvAction::Play {
      url: decision.url,
      play_method: resolution.play_method,
      start_position: ticks_to_seconds(snapshot.position_ticks),
      title: super::session::SessionManager::format_title(&item),
      audio_index: resolution.mpv_audio_index,
      subtitle_index: resolution.mpv_subtitle_index,
    })
    .await;
  if let Some(ext_sub_stream) = resolution.external_subtitle_stream {
    if let Some(sub_url) =
      ctx
        .client
        .playback()
        .build_subtitle_url(&snapshot.item_id, &media_source.id, ext_sub_stream)
    {
      let _ = ctx
        .action_tx
        .send(MpvAction::AddExternalSubtitle(sub_url))
        .await;
    }
  }

  // The replacement consumer starts only after the new session ID is stored
  if let Some(activated) = decision.activated {
    start_hls_event_consumer(activated, ctx.clone());
  }
}
