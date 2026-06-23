//! Session manager - coordinates Jellyfin commands with MPV player.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio::sync::mpsc;

use super::client::JellyfinClient;
use super::error::JellyfinError;
use super::intro_skipper::{
  evaluate_manual_skip, evaluate_skip, evaluate_skip_prompt, IntroSkipKind,
};
use super::mpv_event::{
  apply_property_update, client_message_direction, is_natural_end, property_report_decision,
  should_report_progress, PropertyReportDecision,
};
use super::play_resolution::{
  jellyfin_to_mpv_track_index, resolve_play_request, PlayResolutionConfig,
};
use super::types::*;
use super::websocket::{JellyfinCommand, JellyfinWebSocket, JellyfinWebSocketEvent};
use crate::command::{AppNotification, NowPlayingChanged};
use crate::config::{AppConfig, IntroSkipperMode};
use crate::mpv::MpvClient;
use crate::now_playing::{build_now_playing_state, collect_player_state, PlaybackContext};
use tauri_specta::Event;

const PREFERENCES_STORE_FILE: &str = "preferences.json";
const SERIES_PREFERENCES_KEY: &str = "series_track_preferences";

/// Actions to perform on MPV.
#[derive(Debug, Clone)]
pub enum MpvAction {
  /// Load and play a URL.
  Play {
    url: String,
    start_position: f64,
    title: String,
    audio_index: Option<i32>,
    subtitle_index: Option<i32>,
  },
  /// Add an external subtitle file.
  AddExternalSubtitle(String),
  /// Pause playback.
  Pause,
  /// Resume playback.
  Resume,
  /// Seek to position (seconds).
  Seek(f64),
  /// Show text on MPV's on-screen display.
  ShowText { text: String, duration_ms: i64 },
  /// Stop playback.
  Stop,
  /// Set volume (0-100).
  SetVolume(i32),
  /// Toggle mute.
  ToggleMute,
  /// Toggle fullscreen.
  ToggleFullscreen,
  /// Set audio track by stream index.
  SetAudioTrack(i32),
  /// Set subtitle track by stream index (-1 to disable).
  SetSubtitleTrack(i32),
}

/// Session manager state.
struct SessionState {
  playback: Option<PlaybackSession>,
  last_report_time: std::time::Instant,
  /// Intro Skipper settings captured when the current MPV process started.
  effective_intro_skipper_config: IntroSkipperRuntimeConfig,
  /// Current series ID being played (for track preference saving).
  current_series_id: Option<String>,
  /// Current item being played (for next episode lookup).
  current_item: Option<MediaItem>,
  /// Current media streams (for looking up track languages).
  current_media_streams: Vec<MediaStream>,
  /// Track preferences per series (key: series_id).
  series_preferences: HashMap<String, TrackPreference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IntroSkipperRuntimeConfig {
  mode: IntroSkipperMode,
  keybind_intro_skip: String,
}

impl From<&AppConfig> for IntroSkipperRuntimeConfig {
  fn from(config: &AppConfig) -> Self {
    Self {
      mode: config.intro_skipper_mode,
      keybind_intro_skip: config.keybind_intro_skip.clone(),
    }
  }
}

/// Manages the session between Jellyfin and MPV.
pub struct SessionManager {
  client: Arc<JellyfinClient>,
  websocket: Arc<JellyfinWebSocket>,
  mpv: Arc<MpvClient>,
  config: Arc<RwLock<AppConfig>>,
  app_handle: AppHandle,
  state: Arc<RwLock<SessionState>>,
  action_tx: mpsc::Sender<MpvAction>,
  action_rx: Arc<RwLock<Option<mpsc::Receiver<MpvAction>>>>,
}

impl SessionManager {
  /// Create a new session manager.
  pub fn new(
    client: Arc<JellyfinClient>,
    mpv: Arc<MpvClient>,
    config: Arc<RwLock<AppConfig>>,
    app_handle: AppHandle,
  ) -> Self {
    let (action_tx, action_rx) = mpsc::channel(32);

    // Load series preferences from disk
    let series_preferences = Self::load_preferences_from_store(&app_handle);

    Self {
      client,
      websocket: Arc::new(JellyfinWebSocket::new()),
      mpv,
      config: config.clone(),
      app_handle,
      state: Arc::new(RwLock::new(SessionState {
        playback: None,
        last_report_time: std::time::Instant::now(),
        effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&*config.read()),
        current_series_id: None,
        current_item: None,
        current_media_streams: Vec::new(),
        series_preferences,
      })),
      action_tx,
      action_rx: Arc::new(RwLock::new(Some(action_rx))),
    }
  }

  /// Return current media item metadata for user-facing Now Playing state.
  pub fn current_item(&self) -> Option<MediaItem> {
    self.state.read().current_item.clone()
  }

  async fn emit_now_playing_changed(
    app_handle: &AppHandle,
    mpv: &MpvClient,
    state: &RwLock<SessionState>,
  ) {
    let player = collect_player_state(mpv).await;
    let state = state.read();
    let now_playing = build_now_playing_state(
      player,
      PlaybackContext {
        has_active_session: true,
        current_item: state.current_item.as_ref(),
      },
    );

    let event = NowPlayingChanged { state: now_playing };

    if let Err(e) = event.emit(app_handle) {
      log::error!("Failed to emit Now Playing state: {}", e);
    }
  }

  /// Load series preferences from disk.
  fn load_preferences_from_store(app_handle: &AppHandle) -> HashMap<String, TrackPreference> {
    log::info!("Attempting to load series preferences from store...");
    match app_handle.store(PREFERENCES_STORE_FILE) {
      Ok(store) => {
        log::info!(
          "Store opened successfully, checking for key: {}",
          SERIES_PREFERENCES_KEY
        );
        if let Some(value) = store.get(SERIES_PREFERENCES_KEY) {
          log::info!("Found stored value: {:?}", value);
          match serde_json::from_value::<HashMap<String, TrackPreference>>(value.clone()) {
            Ok(mut prefs) => {
              for pref in prefs.values_mut() {
                pref.normalize_loaded();
              }
              log::info!("Loaded {} series track preferences from disk", prefs.len());
              return prefs;
            }
            Err(e) => {
              log::warn!("Failed to parse stored preferences: {}", e);
            }
          }
        } else {
          log::info!("No stored track preferences found (key not present)");
        }
      }
      Err(e) => {
        log::warn!("Failed to open preferences store: {}", e);
      }
    }
    HashMap::new()
  }

  /// Start the session (connect WebSocket and begin listening).
  pub async fn start(&self) -> Result<(), JellyfinError> {
    log::info!(
      "Starting session with Device ID: {}",
      self.client.playback().device_id()
    );

    // Connect WebSocket first
    let ws_url = self.client.playback().websocket_url()?;
    let ws_user_agent = self.client.playback().websocket_user_agent();
    self
      .websocket
      .connect_with_user_agent(&ws_url, Some(&ws_user_agent))
      .await?;

    // Then report capabilities via HTTP (must be after WebSocket is established)
    self.client.playback().report_capabilities().await?;

    if let Err(e) = self.client.playback().validate_session().await {
      log::warn!("Session validation failed: {} - cast may not work", e);
    } else {
      log::info!("Session validated - we should appear as cast target");
    }

    // Start WebSocket command consumer with auto-reconnect
    self.start_websocket_consumer();

    self.start_local().await
  }

  /// Start local MPV consumers without registering as a remote-control target.
  pub async fn start_local(&self) -> Result<(), JellyfinError> {
    // Start MPV action consumer
    self.start_action_consumer();

    // Start MPV event listener for end-of-file detection
    self.start_mpv_event_listener();

    Ok(())
  }

  /// Start WebSocket command stream consumer.
  fn start_websocket_consumer(&self) {
    let client = self.client.clone();
    let websocket = self.websocket.clone();
    let state = self.state.clone();
    let action_tx = self.action_tx.clone();
    let app_handle = self.app_handle.clone();
    let mpv = self.mpv.clone();
    let config = self.config.clone();

    tokio::spawn(async move {
      let Some(mut event_rx) = websocket.take_event_receiver() else {
        log::warn!("No WebSocket event receiver available");
        return;
      };

      log::info!("WebSocket command stream consumer started");
      while let Some(event) = event_rx.recv().await {
        match event {
          JellyfinWebSocketEvent::Connected => {
            log::info!("Jellyfin WebSocket connected");
          }
          JellyfinWebSocketEvent::ConnectionLost => {
            log::warn!("Jellyfin WebSocket connection lost");
            Self::clear_playback_context(&client, &state).await;
            AppNotification::warning(&app_handle, "Connection lost. Reconnecting...");
          }
          JellyfinWebSocketEvent::Reconnected => {
            log::info!("WebSocket reconnected successfully");
            AppNotification::info(&app_handle, "Reconnected to Jellyfin");

            if let Err(e) = client.playback().report_capabilities().await {
              log::error!("Failed to report capabilities after reconnect: {}", e);
            }
          }
          JellyfinWebSocketEvent::Command(cmd) => {
            if let Err(e) =
              Self::handle_command(&client, &state, &action_tx, &app_handle, &mpv, &config, cmd)
                .await
            {
              log::error!("Failed to handle Jellyfin command: {}", e);
              AppNotification::error(&app_handle, format!("Command failed: {}", e));
            }
          }
        }
      }
    });
  }

  /// Start the MPV action consumer task.
  fn start_action_consumer(&self) {
    if let Some(mut action_rx) = self.action_rx.write().take() {
      let mpv = self.mpv.clone();
      let app_handle = self.app_handle.clone();
      let config = self.config.clone();
      let state = self.state.clone();

      tokio::spawn(async move {
        log::info!("MPV action consumer started, waiting for actions...");
        while let Some(action) = action_rx.recv().await {
          log::info!("Processing MPV action: {:?}", action);

          match action {
            MpvAction::Play {
              url,
              start_position,
              title,
              audio_index,
              subtitle_index,
            } => {
              log::info!(
                "MpvAction::Play received, url={}, title={}",
                redact_url(&url),
                title
              );
              // Start MPV if not already running
              if !mpv.is_connected() {
                log::info!("MPV not connected, starting...");
                if let Err(e) = mpv.start().await {
                  log::error!("Failed to start MPV: {}", e);
                  AppNotification::error(&app_handle, format!("Failed to start MPV: {}", e));
                  continue;
                }
                state.write().effective_intro_skipper_config =
                  IntroSkipperRuntimeConfig::from(&*config.read());
                log::info!("MPV started successfully");
              }

              // Load the file with all options (start position, audio/subtitle tracks)
              // This ensures tracks are set atomically with the file load, avoiding race conditions
              log::info!(
                "Loading file into MPV: {} (start={}, aid={:?}, sid={:?})",
                redact_url(&url),
                start_position,
                audio_index,
                subtitle_index
              );
              if let Err(e) = mpv
                .loadfile_with_options(
                  &url,
                  Some(start_position),
                  audio_index.map(|i| i as i64),
                  subtitle_index.map(|i| i as i64),
                )
                .await
              {
                log::error!("Failed to load file: {}", e);
                AppNotification::error(&app_handle, format!("Failed to load media: {}", e));
                continue;
              }
              log::info!("File loaded successfully");

              // Set the media title (shown in MPV window)
              if let Err(e) = mpv.set_property_string("force-media-title", &title).await {
                log::warn!("Failed to set media title: {}", e);
              }

              log::info!("Started playback: {} - {}", title, redact_url(&url));
            }
            MpvAction::Pause => {
              log::info!("MpvAction::Pause - setting pause=true");
              if let Err(e) = mpv.set_pause(true).await {
                log::error!("Failed to pause: {}", e);
              } else {
                log::info!("MPV paused successfully");
              }
            }
            MpvAction::Resume => {
              log::info!("MpvAction::Resume - setting pause=false");
              if let Err(e) = mpv.set_pause(false).await {
                log::error!("Failed to resume: {}", e);
              } else {
                log::info!("MPV resumed successfully");
              }
            }
            MpvAction::Seek(position) => {
              if let Err(e) = mpv.seek(position).await {
                log::error!("Failed to seek: {}", e);
              }
            }
            MpvAction::ShowText { text, duration_ms } => {
              if let Err(e) = mpv.show_text(&text, duration_ms).await {
                log::warn!("Failed to show MPV text: {}", e);
              }
            }
            MpvAction::Stop => {
              log::info!("MpvAction::Stop - quitting MPV gracefully");
              if let Err(e) = mpv.quit().await {
                log::warn!("Failed to quit MPV gracefully: {}, forcing stop", e);
                mpv.stop().await;
              }
            }
            MpvAction::SetVolume(volume) => {
              if let Err(e) = mpv.set_volume(volume as f64).await {
                log::error!("Failed to set volume: {}", e);
              }
            }
            MpvAction::ToggleMute => {
              if let Err(e) = mpv.toggle_mute().await {
                log::error!("Failed to toggle mute: {}", e);
              }
            }
            MpvAction::ToggleFullscreen => {
              if let Err(e) = mpv.toggle_fullscreen().await {
                log::error!("Failed to toggle fullscreen: {}", e);
              }
            }
            MpvAction::SetAudioTrack(index) => {
              // index is already MPV's 1-based track ID
              if let Err(e) = mpv.set_audio_track(index as i64).await {
                log::error!("Failed to set audio track: {}", e);
              }
            }
            MpvAction::SetSubtitleTrack(index) => {
              if index == -1 {
                // Disable subtitles
                if let Err(e) = mpv.disable_track("sid").await {
                  log::error!("Failed to disable subtitles: {}", e);
                }
              } else {
                // index is already MPV's 1-based track ID
                if let Err(e) = mpv.set_subtitle_track(index as i64).await {
                  log::error!("Failed to set subtitle track: {}", e);
                }
              }
            }
            MpvAction::AddExternalSubtitle(url) => {
              log::info!("MpvAction::AddExternalSubtitle: {}", redact_url(&url));
              if let Err(e) = mpv.sub_add(&url, true).await {
                log::error!("Failed to add external subtitle: {}", e);
              }
            }
          }
        }
      });
    }
  }

  /// Handle a Jellyfin command.
  async fn handle_command(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    app_handle: &AppHandle,
    mpv: &MpvClient,
    config: &RwLock<AppConfig>,
    cmd: JellyfinCommand,
  ) -> Result<(), JellyfinError> {
    match cmd {
      JellyfinCommand::Play(request) => {
        Self::handle_play(
          client,
          state,
          action_tx,
          mpv.is_connected(),
          config,
          request,
        )
        .await?;
      }
      JellyfinCommand::Playstate(request) => {
        Self::handle_playstate(client, state, action_tx, mpv, config, request).await?;
      }
      JellyfinCommand::GeneralCommand(request) => {
        Self::handle_general_command(client, state, action_tx, app_handle, request).await?;
      }
    }
    Ok(())
  }

  /// Handle Play command.
  async fn handle_play(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    mpv_connected: bool,
    config: &RwLock<AppConfig>,
    request: PlayRequest,
  ) -> Result<(), JellyfinError> {
    log::info!("handle_play called with request: {:?}", request);

    // Get the first item ID
    let item_id = request
      .item_ids
      .first()
      .ok_or(JellyfinError::SessionNotFound)?;
    log::info!("Playing item_id: {}", item_id);

    // Fetch media item metadata for title
    let item = client.playback().get_item(item_id).await?;
    let title = Self::format_title(&item);
    log::info!("Media title: {}", title);

    // Get playback info
    let playback_info = client
      .playback()
      .get_playback_info(
        item_id,
        request.audio_stream_index,
        request.subtitle_stream_index,
      )
      .await?;
    log::info!(
      "Got playback info, media_sources count: {}",
      playback_info.media_sources.len()
    );

    // Get the best media source
    let media_source = playback_info
      .media_sources
      .first()
      .ok_or(JellyfinError::SessionNotFound)?;
    log::info!(
      "Using media_source: id={}, protocol={:?}",
      media_source.id,
      media_source.protocol
    );

    let series_preference = item.series_id.as_ref().and_then(|series_id| {
      let s = state.read();
      log::info!(
        "Looking up preferences for series_id={}, preference_count={}, has_preference={}",
        series_id,
        s.series_preferences.len(),
        s.series_preferences.contains_key(series_id)
      );
      s.series_preferences.get(series_id).cloned()
    });
    if let Some(ref pref) = series_preference {
      log::info!(
        "Found track preference for series {:?}: {:?}",
        item.series_id,
        pref
      );
    }

    let (preferred_subtitle_languages, intro_skipper_enabled) = {
      let config_guard = config.read();
      let intro_skipper_config = if mpv_connected {
        state.read().effective_intro_skipper_config.clone()
      } else {
        IntroSkipperRuntimeConfig::from(&*config_guard)
      };
      (
        config_guard.preferred_subtitle_languages.clone(),
        intro_skipper_config.mode != IntroSkipperMode::Off,
      )
    };
    let resolution = resolve_play_request(
      &request,
      &item,
      &playback_info,
      media_source,
      series_preference.as_ref(),
      PlayResolutionConfig {
        preferred_subtitle_languages: &preferred_subtitle_languages,
        intro_skipper_enabled,
      },
    );

    // Build stream URL
    let url = client
      .playback()
      .build_stream_url(item_id, media_source)
      .ok_or(JellyfinError::NotConnected)?;
    log::info!("Built stream URL: {}", redact_url(&url));

    let intro_skipper_ranges = if resolution.should_fetch_intro_skipper_ranges {
      match client.playback().get_intro_skipper_ranges(item_id).await {
        Ok(ranges) => {
          log::info!("Loaded {} Intro Skipper ranges", ranges.len());
          ranges
        }
        Err(e) => {
          log::warn!("Intro Skipper ranges unavailable for {}: {}", item_id, e);
          Vec::new()
        }
      }
    } else {
      log::debug!("Intro Skipper disabled or inapplicable; skipping range fetch");
      Vec::new()
    };

    // Store playback session and current series
    {
      let mut s = state.write();
      s.current_series_id = item.series_id.clone();
      s.current_item = Some(item.clone());
      s.current_media_streams = media_source.media_streams.clone();
      s.playback = Some(PlaybackSession {
        item_id: item_id.clone(),
        media_source_id: Some(media_source.id.clone()),
        play_session_id: playback_info.play_session_id.clone(),
        intro_skipper_ranges,
        position_ticks: resolution.position_ticks,
        is_paused: false,
        is_muted: false,
        volume: 100,
        audio_stream_index: resolution.audio_stream_index,
        subtitle_stream_index: resolution.subtitle_stream_index,
        play_method: resolution.play_method.to_string(),
      });
      s.last_report_time = std::time::Instant::now();
    }

    // Report playback started
    let start_info = PlaybackStartInfo {
      item_id: item_id.clone(),
      media_source_id: Some(media_source.id.clone()),
      play_session_id: playback_info.play_session_id.clone(),
      position_ticks: request.start_position_ticks,
      is_paused: false,
      is_muted: false,
      volume_level: 100,
      audio_stream_index: resolution.audio_stream_index,
      subtitle_stream_index: resolution.subtitle_stream_index,
      play_method: resolution.play_method.to_string(),
      can_seek: true,
    };
    client.playback().report_playback_start(&start_info).await?;

    // Send action to MPV with converted indices
    log::info!(
      "Sending MpvAction::Play: audio_index {:?} (Jellyfin) -> {:?} (MPV), subtitle_index {:?} (Jellyfin) -> {:?} (MPV)",
      resolution.audio_stream_index,
      resolution.mpv_audio_index,
      resolution.subtitle_stream_index,
      resolution.mpv_subtitle_index
    );
    let _ = action_tx
      .send(MpvAction::Play {
        url,
        start_position: resolution.start_position,
        title,
        audio_index: resolution.mpv_audio_index,
        subtitle_index: resolution.mpv_subtitle_index,
      })
      .await;
    log::info!("MpvAction::Play sent successfully");

    // Load external subtitle if the selected subtitle is external
    if let Some(ext_sub_stream) = resolution.external_subtitle_stream {
      if let Some(sub_url) =
        client
          .playback()
          .build_subtitle_url(item_id, &media_source.id, ext_sub_stream)
      {
        log::info!(
          "Loading external subtitle: codec={:?}, url={}",
          ext_sub_stream.codec,
          redact_url(&sub_url)
        );
        let _ = action_tx
          .send(MpvAction::AddExternalSubtitle(sub_url))
          .await;
      } else {
        log::warn!("Failed to build external subtitle URL");
      }
    }

    Ok(())
  }

  /// Format media title for display in MPV.
  fn format_title(item: &MediaItem) -> String {
    match item.item_type.as_str() {
      "Episode" => {
        let series = item.series_name.as_deref().unwrap_or("Unknown");
        let season = item.parent_index_number.unwrap_or(1);
        let episode = item.index_number.unwrap_or(1);
        format!("{} - S{:02}E{:02} - {}", series, season, episode, item.name)
      }
      _ => item.name.clone(),
    }
  }

  /// Handle Playstate command.
  async fn handle_playstate(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    mpv: &MpvClient,
    config: &RwLock<AppConfig>,
    request: PlaystateRequest,
  ) -> Result<(), JellyfinError> {
    log::info!("handle_playstate: command={}", request.command);
    match request.command.as_str() {
      "Pause" => {
        log::info!("Processing Pause command");
        {
          let mut s = state.write();
          if let Some(ref mut playback) = s.playback {
            playback.is_paused = true;
          }
        }
        let _ = action_tx.send(MpvAction::Pause).await;
      }
      "Unpause" => {
        log::info!("Processing Unpause command");
        {
          let mut s = state.write();
          if let Some(ref mut playback) = s.playback {
            playback.is_paused = false;
          }
        }
        let _ = action_tx.send(MpvAction::Resume).await;
      }
      "PlayPause" => {
        // Query actual MPV state to handle cases where user paused via MPV keyboard
        let is_paused = match mpv.get_pause().await {
          Ok(paused) => paused,
          Err(e) => {
            log::warn!(
              "Failed to get pause state from MPV: {}, using internal state",
              e
            );
            let s = state.read();
            s.playback.as_ref().map(|p| p.is_paused).unwrap_or(false)
          }
        };
        log::info!("Processing PlayPause command, MPV paused={}", is_paused);
        if is_paused {
          {
            let mut s = state.write();
            if let Some(ref mut playback) = s.playback {
              playback.is_paused = false;
            }
          }
          let _ = action_tx.send(MpvAction::Resume).await;
        } else {
          {
            let mut s = state.write();
            if let Some(ref mut playback) = s.playback {
              playback.is_paused = true;
            }
          }
          let _ = action_tx.send(MpvAction::Pause).await;
        }
      }
      "Seek" => {
        if let Some(ticks) = request.seek_position_ticks {
          let position = ticks_to_seconds(ticks);
          {
            let mut s = state.write();
            if let Some(ref mut playback) = s.playback {
              playback.position_ticks = ticks;
            }
          }
          let _ = action_tx.send(MpvAction::Seek(position)).await;
        }
      }
      "Stop" => {
        log::info!("Processing Stop command");
        // Take the playback session and report stop to Jellyfin
        let session = {
          let mut s = state.write();
          s.playback.take()
        };

        if let Some(session) = session {
          let stop_info = PlaybackStopInfo {
            item_id: session.item_id,
            media_source_id: session.media_source_id,
            play_session_id: session.play_session_id,
            position_ticks: Some(session.position_ticks),
          };
          if let Err(e) = client.playback().report_playback_stop(&stop_info).await {
            log::error!("Failed to report playback stop: {}", e);
          }
        }

        let _ = action_tx.send(MpvAction::Stop).await;
      }
      "NextTrack" => {
        log::info!("Processing NextTrack command");
        let current_item = {
          let s = state.read();
          s.current_item.clone()
        };

        if let Some(item) = current_item {
          if let Err(e) =
            Self::play_adjacent_episode(client, state, action_tx, config, &item, true, true).await
          {
            log::warn!("NextTrack unavailable: {}", e);
          }
        } else {
          log::warn!("NextTrack: No current item to get next episode from");
        }
      }
      "PreviousTrack" => {
        log::info!("Processing PreviousTrack command");
        let current_item = {
          let s = state.read();
          s.current_item.clone()
        };

        if let Some(item) = current_item {
          if let Err(e) =
            Self::play_adjacent_episode(client, state, action_tx, config, &item, false, true).await
          {
            log::warn!("PreviousTrack unavailable: {}", e);
          }
        } else {
          log::warn!("PreviousTrack: No current item to get previous episode from");
        }
      }
      _ => {
        log::warn!("Unhandled playstate command: {}", request.command);
      }
    }
    Ok(())
  }

  /// Handle GeneralCommand.
  async fn handle_general_command(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    app_handle: &AppHandle,
    request: GeneralCommand,
  ) -> Result<(), JellyfinError> {
    let mut should_save_prefs = false;

    match request.name.as_str() {
      "SetVolume" => {
        if let Some(args) = request.arguments {
          if let Some(volume) = parse_command_int(args.get("Volume")) {
            // Clamp to valid player range (0-100)
            let volume = volume.clamp(0, 100) as i32;
            // Update session state
            {
              let mut s = state.write();
              if let Some(ref mut playback) = s.playback {
                playback.volume = volume;
              }
            }
            let _ = action_tx.send(MpvAction::SetVolume(volume)).await;
          }
        }
      }
      "ToggleMute" => {
        let _ = action_tx.send(MpvAction::ToggleMute).await;
      }
      "ToggleFullscreen" => {
        let _ = action_tx.send(MpvAction::ToggleFullscreen).await;
      }
      "SetAudioStreamIndex" => {
        if let Some(args) = &request.arguments {
          let index = parse_command_int(args.get("Index"));
          if let Some(index) = index {
            log::info!("SetAudioStreamIndex: {} (Jellyfin index)", index);
            // Update playback state and save series preference
            let mpv_index = {
              let mut s = state.write();
              if let Some(ref mut playback) = s.playback {
                playback.audio_stream_index = Some(index as i32);
              }
              // Save preference for series (clone to avoid borrow issues)
              let series_id = s.current_series_id.clone();
              if let Some(series_id) = series_id {
                // Find the language and title of the selected track
                let track_info = s
                  .current_media_streams
                  .iter()
                  .find(|stream| stream.stream_type == "Audio" && stream.index == index as i32)
                  .map(|stream| (stream.language.clone(), stream.display_title.clone()));

                if let Some((lang, title)) = track_info {
                  log::info!(
                    "Saving audio preference for series {}: lang={:?}, title={:?}",
                    series_id,
                    lang,
                    title
                  );
                  let pref = s.series_preferences.entry(series_id).or_default();
                  pref.audio_language = lang;
                  pref.audio_title = title;
                  should_save_prefs = true;
                }
              }
              // Convert Jellyfin stream index to MPV track index
              jellyfin_to_mpv_track_index(&s.current_media_streams, "Audio", index as i32)
            };
            // Send to MPV with converted index
            log::info!("SetAudioStreamIndex: {} (MPV index)", mpv_index);
            let _ = action_tx.send(MpvAction::SetAudioTrack(mpv_index)).await;
          }
        }
      }
      "SetSubtitleStreamIndex" => {
        if let Some(args) = &request.arguments {
          let index = parse_command_int(args.get("Index"));
          if let Some(index) = index {
            log::info!("SetSubtitleStreamIndex: {} (Jellyfin index)", index);

            // Collect data we need while holding the lock
            let (mpv_action, item_id, media_source_id) = {
              let mut s = state.write();

              // Update playback state
              if let Some(ref mut playback) = s.playback {
                playback.subtitle_stream_index = Some(index as i32);
              }

              // Save preference for series
              let series_id = s.current_series_id.clone();
              if let Some(series_id) = series_id {
                if index == -1 {
                  log::info!(
                    "Saving subtitle disabled preference for series {}",
                    series_id
                  );
                  let pref = s.series_preferences.entry(series_id).or_default();
                  pref.is_subtitle_enabled = false;
                  pref.subtitle_preference_set = true;
                  pref.subtitle_language = None;
                  pref.subtitle_title = None;
                  should_save_prefs = true;
                } else {
                  let track_info = s
                    .current_media_streams
                    .iter()
                    .find(|stream| stream.stream_type == "Subtitle" && stream.index == index as i32)
                    .map(|stream| (stream.language.clone(), stream.display_title.clone()));

                  let pref = s.series_preferences.entry(series_id.clone()).or_default();
                  if let Some((lang, title)) = track_info {
                    log::info!(
                      "Saving subtitle preference for series {}: lang={:?}, title={:?}",
                      series_id,
                      lang,
                      title
                    );
                    pref.is_subtitle_enabled = true;
                    pref.subtitle_preference_set = true;
                    pref.subtitle_language = lang;
                    pref.subtitle_title = title;
                  } else {
                    pref.is_subtitle_enabled = true;
                    pref.subtitle_preference_set = true;
                  }
                  should_save_prefs = true;
                }
              }

              // Determine action: external subtitle via sub-add or internal via sid
              if index == -1 {
                // Disable subtitles
                (MpvAction::SetSubtitleTrack(-1), None, None)
              } else {
                // Find the subtitle stream
                let external_stream = s
                  .current_media_streams
                  .iter()
                  .find(|stream| {
                    stream.stream_type == "Subtitle"
                      && stream.index == index as i32
                      && stream.is_external
                  })
                  .cloned();

                if let Some(ext_stream) = external_stream {
                  // External subtitle - need to use sub-add
                  let item_id = s.playback.as_ref().map(|p| p.item_id.clone());
                  let media_source_id = s.playback.as_ref().and_then(|p| p.media_source_id.clone());
                  // Return placeholder action - we'll build the URL outside the lock
                  (
                    MpvAction::SetSubtitleTrack(-1),
                    item_id,
                    media_source_id.map(|id| (id, ext_stream)),
                  )
                } else {
                  // Internal subtitle - convert index and use sid
                  let mpv_idx =
                    jellyfin_to_mpv_track_index(&s.current_media_streams, "Subtitle", index as i32);
                  (MpvAction::SetSubtitleTrack(mpv_idx), None, None)
                }
              }
            };

            // Handle the action
            match (item_id, media_source_id) {
              (Some(item_id), Some((ms_id, ext_stream))) => {
                // External subtitle - build URL and use sub-add
                if let Some(sub_url) =
                  client
                    .playback()
                    .build_subtitle_url(&item_id, &ms_id, &ext_stream)
                {
                  log::info!("SetSubtitleStreamIndex: loading external subtitle via sub-add");
                  let _ = action_tx
                    .send(MpvAction::AddExternalSubtitle(sub_url))
                    .await;
                } else {
                  log::warn!("Failed to build external subtitle URL");
                }
              }
              _ => {
                // Internal subtitle or disable
                log::info!("SetSubtitleStreamIndex: sending {:?}", mpv_action);
                let _ = action_tx.send(mpv_action).await;
              }
            }
          }
        }
      }
      _ => {
        log::debug!("Unhandled general command: {}", request.name);
      }
    }

    // Persist preferences to disk if changed
    if should_save_prefs {
      Self::save_preferences_static(state, app_handle);
    }

    Ok(())
  }

  /// Save preferences to disk (static version for use in async contexts).
  fn save_preferences_static(state: &RwLock<SessionState>, app_handle: &AppHandle) {
    let prefs = {
      let s = state.read();
      s.series_preferences.clone()
    };

    match app_handle.store(PREFERENCES_STORE_FILE) {
      Ok(store) => match serde_json::to_value(&prefs) {
        Ok(value) => {
          store.set(SERIES_PREFERENCES_KEY.to_string(), value);
          if let Err(e) = store.save() {
            log::error!("Failed to save preferences to disk: {}", e);
          } else {
            log::debug!("Saved {} series track preferences to disk", prefs.len());
          }
        }
        Err(e) => {
          log::error!("Failed to serialize preferences: {}", e);
        }
      },
      Err(e) => {
        log::error!("Failed to open preferences store for writing: {}", e);
      }
    }
  }

  /// Start MPV event listener for property changes, end-of-file detection, and keyboard shortcuts.
  /// This is the main event-driven loop that handles:
  /// - Property observations (pause, volume, mute) for immediate UI sync
  /// - Periodic time-pos reporting (every 10s) for progress bar
  /// - End-file events for auto-play next episode
  /// - Client-message events for keyboard shortcuts
  fn start_mpv_event_listener(&self) {
    let mpv = self.mpv.clone();
    let client = self.client.clone();
    let state = self.state.clone();
    let action_tx = self.action_tx.clone();
    let config = self.config.clone();
    let app_handle = self.app_handle.clone();

    tokio::spawn(async move {
      log::info!("MPV event listener started");

      // Wait a bit for MPV to connect before trying to get events
      tokio::time::sleep(std::time::Duration::from_secs(1)).await;

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

        log::info!("Property observations set up, listening for events...");

        // Track last progress report time to throttle time-pos updates
        let mut last_progress_report = std::time::Instant::now();
        let progress_report_interval = std::time::Duration::from_secs(5);

        // Process events
        while let Ok(event) = event_rx.recv().await {
          match event.event.as_str() {
            "property-change" => {
              let property_name = event.name.as_deref().unwrap_or("");
              let decision = property_report_decision(property_name);
              let should_report = if decision == PropertyReportDecision::Ignore {
                false
              } else {
                Self::update_state_from_property(&state, &event);
                if property_name == "time-pos" {
                  Self::apply_intro_skipper(&state, &action_tx, &event).await;
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
                Self::report_progress(&client, &state).await;
                Self::emit_now_playing_changed(&app_handle, &mpv, &state).await;
              }
            }
            "end-file" => {
              Self::handle_end_file_event(&event, &client, &state, &action_tx, &config).await;
              Self::emit_now_playing_changed(&app_handle, &mpv, &state).await;
            }
            "client-message" => {
              Self::handle_client_message_event(&event, &client, &state, &action_tx, &config).await;
              Self::emit_now_playing_changed(&app_handle, &mpv, &state).await;
            }
            _ => {
              // Ignore other events
            }
          }
        }

        // MPV event receiver closed - this means MPV died or disconnected
        // Clear playback context and notify Jellyfin
        log::warn!("MPV event receiver closed, clearing playback context...");
        Self::clear_playback_context(&client, &state).await;
        Self::emit_now_playing_changed(&app_handle, &mpv, &state).await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
      }
    });
  }

  /// Update session state from a property-change event.
  fn update_state_from_property(state: &RwLock<SessionState>, event: &crate::mpv::MpvEvent) {
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
  async fn apply_intro_skipper(
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
          s.playback.as_mut().and_then(|playback| {
            evaluate_skip(position_seconds, &mut playback.intro_skipper_ranges)
          })
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
  async fn report_progress(client: &JellyfinClient, state: &RwLock<SessionState>) {
    let session = {
      let s = state.read();
      s.playback.clone()
    };

    let Some(session) = session else {
      return;
    };

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
  async fn handle_end_file_event(
    event: &crate::mpv::MpvEvent,
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    config: &RwLock<AppConfig>,
  ) {
    let reason = event.reason.as_deref().unwrap_or("");
    log::info!("MPV end-file event, reason: {}", reason);

    // "eof" means natural end of file, "stop" means user stopped
    if !is_natural_end(event.reason.as_deref()) {
      return;
    }

    // Get current item for next episode lookup
    let current_item = {
      let s = state.read();
      s.current_item.clone()
    };

    let Some(item) = current_item else {
      return;
    };

    log::info!("Playback ended naturally, checking for next episode...");

    // Report playback stopped to Jellyfin
    Self::report_playback_stopped(client, state).await;

    // Try to get next episode
    if let Err(e) =
      Self::play_adjacent_episode(client, state, action_tx, config, &item, true, false).await
    {
      log::info!("Natural end did not start an adjacent episode: {}", e);
    }
  }

  /// Handle MPV client-message event for keyboard shortcuts.
  ///
  /// Users can add to their input.conf:
  ///   Shift+> script-message jellypilot-next
  ///   Shift+< script-message jellypilot-prev
  async fn handle_client_message_event(
    event: &crate::mpv::MpvEvent,
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    config: &RwLock<AppConfig>,
  ) {
    let args = match &event.args {
      Some(args) if !args.is_empty() => args,
      _ => return,
    };

    if args[0] == "jellypilot-skip-intro" {
      Self::handle_manual_intro_skip(state, action_tx).await;
      return;
    }

    let Some(direction) = client_message_direction(args) else {
      log::debug!("Unknown client-message command: {}", args[0]);
      return;
    };

    let current_item = {
      let s = state.read();
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
    if let Err(e) =
      Self::play_adjacent_episode(client, state, action_tx, config, &item, next, true).await
    {
      log::warn!("Keyboard shortcut {} unavailable: {}", args[0], e);
    }
  }

  async fn handle_manual_intro_skip(
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

  /// Report playback stopped to Jellyfin and clear session.
  async fn report_playback_stopped(client: &JellyfinClient, state: &RwLock<SessionState>) {
    let session = {
      let mut s = state.write();
      s.playback.take()
    };

    if let Some(session) = session {
      let stop_info = PlaybackStopInfo {
        item_id: session.item_id,
        media_source_id: session.media_source_id,
        play_session_id: session.play_session_id,
        position_ticks: Some(session.position_ticks),
      };
      if let Err(e) = client.playback().report_playback_stop(&stop_info).await {
        log::error!("Failed to report playback stop: {}", e);
      }
    }
  }

  /// Clear all playback context - reports stop to Jellyfin and clears all state.
  /// Call this when MPV dies unexpectedly or WebSocket disconnects during playback.
  async fn clear_playback_context(client: &JellyfinClient, state: &RwLock<SessionState>) {
    // First report stopped to Jellyfin
    Self::report_playback_stopped(client, state).await;

    // Then clear all related state
    let mut s = state.write();
    s.current_item = None;
    s.current_series_id = None;
    s.current_media_streams.clear();
    log::info!("Playback context cleared");
  }

  /// Play the next or previous episode.
  async fn play_adjacent_episode(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    config: &RwLock<AppConfig>,
    current_item: &MediaItem,
    next: bool,
    report_current_stopped: bool,
  ) -> Result<(), String> {
    let result = if next {
      client.playback().get_next_episode(current_item).await
    } else {
      client.playback().get_previous_episode(current_item).await
    };

    match result {
      Ok(Some(adjacent_item)) => {
        log::info!(
          "Playing {} episode: {} - S{:02}E{:02}",
          if next { "next" } else { "previous" },
          adjacent_item.series_name.as_deref().unwrap_or("Unknown"),
          adjacent_item.parent_index_number.unwrap_or(0),
          adjacent_item.index_number.unwrap_or(0)
        );

        if report_current_stopped {
          Self::report_playback_stopped(client, state).await;
        }

        let play_request = PlayRequest {
          item_ids: vec![adjacent_item.id.clone()],
          start_position_ticks: None,
          play_command: "PlayNow".to_string(),
          media_source_id: None,
          audio_stream_index: None,
          subtitle_stream_index: None,
        };

        Self::handle_play(client, state, action_tx, true, config, play_request)
          .await
          .map_err(|e| {
            log::error!(
              "Failed to play {} episode: {}",
              if next { "next" } else { "previous" },
              e
            );
            format!(
              "Failed to play {} episode",
              if next { "next" } else { "previous" }
            )
          })
      }
      Ok(None) => {
        log::info!(
          "No {} episode available",
          if next { "next" } else { "previous" }
        );
        Err(format!(
          "No {} episode is available",
          if next { "next" } else { "previous" }
        ))
      }
      Err(e) => {
        log::error!(
          "Failed to get {} episode: {}",
          if next { "next" } else { "previous" },
          e
        );
        Err(format!(
          "Failed to find {} episode",
          if next { "next" } else { "previous" }
        ))
      }
    }
  }

  async fn play_library_request(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    action_tx: &mpsc::Sender<MpvAction>,
    mpv_connected: bool,
    config: &RwLock<AppConfig>,
    request: VideoLibraryPlayRequest,
  ) -> Result<(), JellyfinError> {
    let play_request = Self::resolve_library_play_request(client, request).await?;

    Self::report_playback_stopped(client, state).await;
    Self::handle_play(
      client,
      state,
      action_tx,
      mpv_connected,
      config,
      play_request,
    )
    .await
  }

  async fn resolve_library_play_request(
    client: &JellyfinClient,
    request: VideoLibraryPlayRequest,
  ) -> Result<PlayRequest, JellyfinError> {
    let item_id = request.item_id.trim().to_string();
    if item_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Item id is required for Library playback".to_string(),
      ));
    }

    let (item_id, start_position_ticks) = match request.mode {
      VideoLibraryPlayMode::Resume => {
        let ticks = request
          .start_position_seconds
          .map(seconds_to_ticks)
          .unwrap_or(0)
          .max(0);
        if ticks == 0 {
          return Err(JellyfinError::HttpError(
            "Resume playback requires a saved position".to_string(),
          ));
        }
        (item_id, Some(ticks))
      }
      VideoLibraryPlayMode::Start => (item_id, Some(0)),
      VideoLibraryPlayMode::Show => {
        let target = client
          .library()
          .next_playable_episode(item_id)
          .await?
          .ok_or_else(|| {
            JellyfinError::HttpError(
              "No playable next episode is available for this show".to_string(),
            )
          })?;
        (target.item_id, target.start_position_ticks)
      }
    };

    Ok(PlayRequest {
      item_ids: vec![item_id],
      start_position_ticks,
      play_command: "PlayNow".to_string(),
      media_source_id: None,
      audio_stream_index: request.audio_stream_index,
      subtitle_stream_index: request.subtitle_stream_index,
    })
  }

  /// Start explicit Library Browser playback through the existing playback target path.
  pub async fn play_library(&self, request: VideoLibraryPlayRequest) -> Result<(), JellyfinError> {
    Self::play_library_request(
      &self.client,
      &self.state,
      &self.action_tx,
      self.mpv.is_connected(),
      &self.config,
      request,
    )
    .await
  }

  /// Play the next episode. Called from system tray or UI.
  pub async fn play_next_episode(&self) -> Result<(), String> {
    let current_item = {
      let s = self.state.read();
      s.current_item.clone()
    };

    if let Some(item) = current_item {
      log::info!("Tray/UI: playing next episode");
      Self::play_adjacent_episode(
        &self.client,
        &self.state,
        &self.action_tx,
        &self.config,
        &item,
        true,
        true,
      )
      .await
    } else {
      log::warn!("play_next_episode: No current item");
      Err("Next episode is available during episode playback".to_string())
    }
  }

  /// Play the previous episode. Called from system tray or UI.
  pub async fn play_previous_episode(&self) -> Result<(), String> {
    let current_item = {
      let s = self.state.read();
      s.current_item.clone()
    };

    if let Some(item) = current_item {
      log::info!("Tray/UI: playing previous episode");
      Self::play_adjacent_episode(
        &self.client,
        &self.state,
        &self.action_tx,
        &self.config,
        &item,
        false,
        true,
      )
      .await
    } else {
      log::warn!("play_previous_episode: No current item");
      Err("Previous episode is available during episode playback".to_string())
    }
  }

  /// Stop the session.
  pub async fn stop(&self) -> Result<(), JellyfinError> {
    // Report playback stopped if there's an active session
    let session = {
      let mut s = self.state.write();
      s.playback.take()
    };

    if let Some(session) = session {
      let stop_info = PlaybackStopInfo {
        item_id: session.item_id,
        media_source_id: session.media_source_id,
        play_session_id: session.play_session_id,
        position_ticks: Some(session.position_ticks),
      };
      self
        .client
        .playback()
        .report_playback_stop(&stop_info)
        .await?;
    }

    self.websocket.disconnect().await;
    Ok(())
  }
}

/// Parse a Jellyfin command argument as an integer.
/// Accepts both JSON numbers and JSON strings containing an integer.
/// Returns `None` for missing, non-integer, or malformed values.
fn parse_command_int(value: Option<&serde_json::Value>) -> Option<i64> {
  value.and_then(|v| {
    v.as_i64()
      .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
  })
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

/// Redact sensitive URL/header fragments from log text.
fn redact_url(url: &str) -> String {
  const SENSITIVE_KEYS: &[&str] = &[
    "api_key",
    "access_token",
    "accesstoken",
    "token",
    "password",
    "pw",
  ];

  let mut output = String::with_capacity(url.len());
  let mut cursor = 0;

  while cursor < url.len() {
    let Some((_, key_end)) = find_sensitive_assignment(&url[cursor..], SENSITIVE_KEYS) else {
      output.push_str(&url[cursor..]);
      break;
    };

    let key_end = cursor + key_end;
    let value_start = key_end + 1;
    let quote = url[value_start..]
      .chars()
      .next()
      .filter(|ch| matches!(ch, '"' | '\''));
    let value_start = value_start + quote.map(char::len_utf8).unwrap_or(0);
    let value_end = find_assignment_value_end(url, value_start, quote);

    output.push_str(&url[cursor..value_start]);
    output.push_str("[REDACTED]");
    if let Some(quote) = quote {
      if value_end < url.len() && url[value_end..].starts_with(quote) {
        output.push(quote);
        cursor = value_end + quote.len_utf8();
        continue;
      }
    }
    cursor = value_end;
  }

  output
}

fn find_sensitive_assignment(text: &str, sensitive_keys: &[&str]) -> Option<(usize, usize)> {
  let bytes = text.as_bytes();
  let mut index = 0;

  while index < bytes.len() {
    if is_key_boundary(text, index) {
      let key_start = index + boundary_len(text, index);
      let mut key_end = key_start;
      while key_end < bytes.len() && is_assignment_key_byte(bytes[key_end]) {
        key_end += 1;
      }

      if key_end < bytes.len()
        && bytes[key_end] == b'='
        && sensitive_keys
          .iter()
          .any(|key| text[key_start..key_end].eq_ignore_ascii_case(key))
      {
        return Some((key_start, key_end));
      }

      index = key_end.saturating_add(1);
    } else {
      index += 1;
    }
  }

  None
}

fn is_key_boundary(text: &str, index: usize) -> bool {
  index == 0
    || matches!(
      text.as_bytes()[index],
      b'?' | b'&' | b',' | b' ' | b'\t' | b'\n'
    )
}

fn boundary_len(text: &str, index: usize) -> usize {
  if matches!(text.as_bytes()[index], b'?' | b'&') {
    1
  } else {
    0
  }
}

fn is_assignment_key_byte(byte: u8) -> bool {
  byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn find_assignment_value_end(text: &str, value_start: usize, quote: Option<char>) -> usize {
  if let Some(quote) = quote {
    text[value_start..]
      .find(quote)
      .map(|offset| value_start + offset)
      .unwrap_or(text.len())
  } else {
    text[value_start..]
      .find(['&', ' ', '\t', '\n', '\r', '"', '\''])
      .map(|offset| value_start + offset)
      .unwrap_or(text.len())
  }
}

#[cfg(test)]
mod tests {
  use super::super::intro_skipper::{IntroSkipKind, IntroSkipRange};
  use super::*;
  use std::sync::Arc;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;

  type RequestLog = Arc<parking_lot::Mutex<Vec<String>>>;

  async fn serve_owned_responses_with_requests(
    responses: Vec<(String, String)>,
  ) -> (String, RequestLog) {
    let listener = TcpListener::bind("127.0.0.1:0")
      .await
      .expect("test server should bind");
    let addr = listener.local_addr().expect("test server should have addr");
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&requests);

    tokio::spawn(async move {
      for (status, response_body) in responses {
        let (mut stream, _) = listener.accept().await.expect("test server should accept");
        let mut buffer = [0; 8192];
        let bytes_read = stream
          .read(&mut buffer)
          .await
          .expect("test server should read request");
        let request = String::from_utf8_lossy(&buffer[..bytes_read]).into_owned();
        captured_requests.lock().push(request);
        let response = format!(
          "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
          status,
          response_body.len(),
          response_body
        );
        stream
          .write_all(response.as_bytes())
          .await
          .expect("test server should write response");
      }
    });

    (format!("http://{}", addr), requests)
  }

  async fn connected_test_client(
    responses: Vec<(&'static str, &'static str)>,
  ) -> (JellyfinClient, RequestLog) {
    let responses = responses
      .into_iter()
      .map(|(status, body)| (status.to_string(), body.to_string()))
      .collect();
    let (server_url, requests) = serve_owned_responses_with_requests(responses).await;
    let client = JellyfinClient::new();
    client
      .login()
      .restore_session(&SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url,
        access_token: "token-1".to_string(),
        user_id: "00000000-0000-0000-0000-000000000001".to_string(),
        user_name: "Ada".to_string(),
        server_name: Some("Jellyfin Home".to_string()),
        device_id: Some("device-1".to_string()),
      })
      .await
      .expect("test client should restore saved session");

    (client, requests)
  }

  async fn connected_emby_test_client(
    responses: Vec<(&'static str, &'static str)>,
  ) -> (JellyfinClient, RequestLog) {
    let responses = responses
      .into_iter()
      .map(|(status, body)| (status.to_string(), body.to_string()))
      .collect();
    let (server_url, requests) = serve_owned_responses_with_requests(responses).await;
    let client = JellyfinClient::new();
    client
      .login()
      .restore_session(&SavedSession {
        provider: MediaServerProvider::Emby,
        server_url,
        access_token: "emby-token".to_string(),
        user_id: "00000000-0000-0000-0000-000000000001".to_string(),
        user_name: "Ada".to_string(),
        server_name: Some("Emby Home".to_string()),
        device_id: Some("device-1".to_string()),
      })
      .await
      .expect("test Emby client should restore saved session");

    (client, requests)
  }

  fn test_config() -> RwLock<AppConfig> {
    RwLock::new(AppConfig {
      intro_skipper_mode: IntroSkipperMode::Off,
      ..Default::default()
    })
  }

  fn empty_test_state() -> RwLock<SessionState> {
    RwLock::new(SessionState {
      playback: None,
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
    })
  }

  fn test_state_with_active_playback() -> RwLock<SessionState> {
    RwLock::new(SessionState {
      playback: Some(PlaybackSession {
        item_id: "old-movie".to_string(),
        media_source_id: Some("old-source".to_string()),
        play_session_id: Some("old-play".to_string()),
        intro_skipper_ranges: Vec::new(),
        position_ticks: 420_000_000,
        is_paused: false,
        is_muted: false,
        volume: 100,
        audio_stream_index: None,
        subtitle_stream_index: None,
        play_method: "DirectPlay".to_string(),
      }),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
    })
  }

  pub(super) fn test_state_with_intro_range() -> RwLock<SessionState> {
    test_state_with_range(IntroSkipKind::Introduction, 10.0, 80.0)
  }

  fn test_state_with_range(
    kind: IntroSkipKind,
    start_seconds: f64,
    end_seconds: f64,
  ) -> RwLock<SessionState> {
    RwLock::new(SessionState {
      playback: Some(PlaybackSession {
        item_id: "item-1".to_string(),
        media_source_id: Some("source-1".to_string()),
        play_session_id: Some("play-1".to_string()),
        intro_skipper_ranges: vec![IntroSkipRange {
          kind,
          start_seconds,
          end_seconds,
          notified: false,
          skipped: false,
        }],
        position_ticks: 0,
        is_paused: false,
        is_muted: false,
        volume: 100,
        audio_stream_index: None,
        subtitle_stream_index: None,
        play_method: "DirectPlay".to_string(),
      }),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
    })
  }

  #[tokio::test]
  async fn library_play_replaces_active_playback_and_resumes_from_saved_position() {
    let (client, requests) = connected_test_client(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"}"#,
      ),
      (
        "200 OK",
        r#"{"ServerName":"Jellyfin Home","Version":"10.10.0","Id":"server-1"}"#,
      ),
      ("204 No Content", ""),
      (
        "200 OK",
        r#"{"Id":"movie-1","Name":"Detail Movie","Type":"Movie"}"#,
      ),
      (
        "200 OK",
        r#"{"MediaSources":[{"Id":"source-1","Protocol":"Http","Container":"mkv","MediaStreams":[]}],"PlaySessionId":"play-2"}"#,
      ),
      ("204 No Content", ""),
    ])
    .await;
    let state = test_state_with_active_playback();
    let config = test_config();
    let (action_tx, mut action_rx) = mpsc::channel(4);

    SessionManager::play_library_request(
      &client,
      &state,
      &action_tx,
      true,
      &config,
      VideoLibraryPlayRequest {
        item_id: "movie-1".to_string(),
        mode: VideoLibraryPlayMode::Resume,
        start_position_seconds: Some(120.0),
        audio_stream_index: Some(1),
        subtitle_stream_index: Some(2),
      },
    )
    .await
    .expect("library resume should replace active playback");

    let action = action_rx
      .recv()
      .await
      .expect("library playback should send a play action");
    match action {
      MpvAction::Play {
        start_position,
        title,
        ..
      } => {
        assert_eq!(start_position, 120.0);
        assert_eq!(title, "Detail Movie");
      }
      other => panic!("expected play action, got {other:?}"),
    }

    let playback = state.read().playback.clone().expect("new playback state");
    assert_eq!(playback.item_id, "movie-1");
    assert_eq!(playback.position_ticks, 1_200_000_000);
    assert_eq!(playback.audio_stream_index, Some(1));
    assert_eq!(playback.subtitle_stream_index, Some(2));

    let captured = requests.lock();
    assert!(captured[2].starts_with("POST /Sessions/Playing/Stopped "));
    assert!(captured[2].contains(r#""ItemId":"old-movie""#));
    assert!(captured[2].contains(r#""PositionTicks":420000000"#));
    assert!(captured[5].starts_with("POST /Sessions/Playing "));
    assert!(captured[5].contains(r#""ItemId":"movie-1""#));
    assert!(captured[5].contains(r#""PositionTicks":1200000000"#));
  }

  #[tokio::test]
  async fn library_show_play_resolves_next_up_episode_before_playback() {
    let series_id = "00000000-0000-0000-0000-000000000071";
    let episode_id = "00000000-0000-0000-0000-000000000072";
    let (client, requests) = connected_test_client(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"}"#,
      ),
      (
        "200 OK",
        r#"{"ServerName":"Jellyfin Home","Version":"10.10.0","Id":"server-1"}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000072","Name":"Next Episode","Type":"Episode","UserData":{"PlaybackPositionTicks":900000000,"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000072","Name":"Next Episode","Type":"Episode","SeriesId":"00000000-0000-0000-0000-000000000071","SeriesName":"Example Show","ParentIndexNumber":1,"IndexNumber":2}"#,
      ),
      (
        "200 OK",
        r#"{"MediaSources":[{"Id":"source-2","Protocol":"Http","Container":"mkv","MediaStreams":[]}],"PlaySessionId":"play-3"}"#,
      ),
      ("204 No Content", ""),
    ])
    .await;
    let state = empty_test_state();
    let config = test_config();
    let (action_tx, mut action_rx) = mpsc::channel(4);

    SessionManager::play_library_request(
      &client,
      &state,
      &action_tx,
      false,
      &config,
      VideoLibraryPlayRequest {
        item_id: series_id.to_string(),
        mode: VideoLibraryPlayMode::Show,
        start_position_seconds: None,
        audio_stream_index: None,
        subtitle_stream_index: None,
      },
    )
    .await
    .expect("show play should resolve NextUp and start playback");

    let action = action_rx
      .recv()
      .await
      .expect("show playback should send a play action");
    match action {
      MpvAction::Play {
        start_position,
        title,
        ..
      } => {
        assert_eq!(start_position, 90.0);
        assert_eq!(title, "Example Show - S01E02 - Next Episode");
      }
      other => panic!("expected play action, got {other:?}"),
    }

    let playback = state.read().playback.clone().expect("new playback state");
    assert_eq!(playback.item_id, episode_id);
    assert_eq!(playback.position_ticks, 900_000_000);

    let captured = requests.lock();
    assert!(captured[2].starts_with("GET /Shows/NextUp?"));
    assert!(captured[2].contains("seriesId=00000000-0000-0000-0000-000000000071"));
    assert!(captured[2].contains("enableResumable=true"));
    assert!(captured[3].starts_with(
      "GET /Users/00000000-0000-0000-0000-000000000001/Items/00000000-0000-0000-0000-000000000072 "
    ));
    assert!(captured[5].starts_with("POST /Sessions/Playing "));
    assert!(captured[5].contains(r#""ItemId":"00000000-0000-0000-0000-000000000072""#));
    assert!(captured[5].contains(r#""PositionTicks":900000000"#));
  }

  #[tokio::test]
  async fn emby_library_play_uses_shared_playback_resolution_and_provider_urls() {
    let (client, requests) = connected_emby_test_client(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"}"#,
      ),
      (
        "200 OK",
        r#"{"Id":"movie-emby","Name":"Emby Movie","Type":"Movie"}"#,
      ),
      (
        "200 OK",
        r#"{"MediaSources":[{"Id":"source-emby","Protocol":"Http","Container":"mp4","SupportsDirectPlay":false,"SupportsDirectStream":true,"SupportsTranscoding":true,"DirectStreamUrl":"/videos/direct-stream.mp4?MediaSourceId=source-emby","TranscodingUrl":"/videos/transcode.m3u8","MediaStreams":[{"Index":1,"Type":"Audio","Language":"eng","DisplayTitle":"English AAC","Codec":"aac","IsDefault":true},{"Index":2,"Type":"Subtitle","Language":"eng","DisplayTitle":"English SRT","Codec":"srt","IsExternal":true}]}],"PlaySessionId":"play-emby"}"#,
      ),
      ("204 No Content", ""),
    ])
    .await;
    let state = empty_test_state();
    let config = test_config();
    let (action_tx, mut action_rx) = mpsc::channel(4);

    SessionManager::play_library_request(
      &client,
      &state,
      &action_tx,
      false,
      &config,
      VideoLibraryPlayRequest {
        item_id: "movie-emby".to_string(),
        mode: VideoLibraryPlayMode::Start,
        start_position_seconds: None,
        audio_stream_index: Some(1),
        subtitle_stream_index: Some(2),
      },
    )
    .await
    .expect("Emby library play should start playback through shared flow");

    let play_action = action_rx
      .recv()
      .await
      .expect("Emby library playback should send play action");
    match play_action {
      MpvAction::Play {
        url,
        title,
        audio_index,
        subtitle_index,
        ..
      } => {
        assert_eq!(title, "Emby Movie");
        assert_eq!(audio_index, Some(1));
        assert_eq!(subtitle_index, None);
        assert!(
          url.ends_with("/videos/direct-stream.mp4?MediaSourceId=source-emby&api_key=emby-token")
        );
      }
      other => panic!("expected play action, got {other:?}"),
    }

    let subtitle_action = action_rx
      .recv()
      .await
      .expect("external Emby subtitle should be loaded separately");
    match subtitle_action {
      MpvAction::AddExternalSubtitle(url) => {
        assert!(
          url.ends_with("/Videos/movie-emby/source-emby/Subtitles/2/Stream.srt?api_key=emby-token")
        );
      }
      other => panic!("expected external subtitle action, got {other:?}"),
    }

    let playback = state.read().playback.clone().expect("new playback state");
    assert_eq!(playback.item_id, "movie-emby");
    assert_eq!(playback.media_source_id.as_deref(), Some("source-emby"));
    assert_eq!(playback.play_session_id.as_deref(), Some("play-emby"));
    assert_eq!(playback.audio_stream_index, Some(1));
    assert_eq!(playback.subtitle_stream_index, Some(2));

    let captured = requests.lock();
    assert!(
      captured[1].starts_with("GET /Users/00000000-0000-0000-0000-000000000001/Items/movie-emby ")
    );
    assert!(captured[2].starts_with("POST /Items/movie-emby/PlaybackInfo "));
    assert!(captured[2].contains(r#""AudioStreamIndex":1"#));
    assert!(captured[2].contains(r#""SubtitleStreamIndex":2"#));
    assert!(captured[3].starts_with("POST /Sessions/Playing "));
    assert!(captured[3].contains(r#""PlayMethod":"DirectStream""#));
  }

  #[tokio::test]
  async fn emby_playback_progress_reports_resolved_play_method_and_session_fields() {
    let (client, requests) = connected_emby_test_client(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"}"#,
      ),
      ("204 No Content", ""),
    ])
    .await;
    let state = RwLock::new(SessionState {
      playback: Some(PlaybackSession {
        item_id: "movie-emby".to_string(),
        media_source_id: Some("source-emby".to_string()),
        play_session_id: Some("play-emby".to_string()),
        intro_skipper_ranges: Vec::new(),
        position_ticks: 900_000_000,
        is_paused: true,
        is_muted: true,
        volume: 65,
        audio_stream_index: Some(1),
        subtitle_stream_index: Some(2),
        play_method: "DirectStream".to_string(),
      }),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
    });

    SessionManager::report_progress(&client, &state).await;

    let captured = requests.lock();
    assert!(captured[1].starts_with("POST /Sessions/Playing/Progress "));
    assert!(captured[1].contains(r#""ItemId":"movie-emby""#));
    assert!(captured[1].contains(r#""MediaSourceId":"source-emby""#));
    assert!(captured[1].contains(r#""PlaySessionId":"play-emby""#));
    assert!(captured[1].contains(r#""PositionTicks":900000000"#));
    assert!(captured[1].contains(r#""IsPaused":true"#));
    assert!(captured[1].contains(r#""IsMuted":true"#));
    assert!(captured[1].contains(r#""VolumeLevel":65"#));
    assert!(captured[1].contains(r#""AudioStreamIndex":1"#));
    assert!(captured[1].contains(r#""SubtitleStreamIndex":2"#));
    assert!(captured[1].contains(r#""PlayMethod":"DirectStream""#));
    assert!(captured[1].contains(r#""CanSeek":true"#));
  }

  #[tokio::test]
  async fn emby_playback_stop_reports_session_identity_and_final_position() {
    let (client, requests) = connected_emby_test_client(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"}"#,
      ),
      ("204 No Content", ""),
    ])
    .await;
    let state = RwLock::new(SessionState {
      playback: Some(PlaybackSession {
        item_id: "movie-emby".to_string(),
        media_source_id: Some("source-emby".to_string()),
        play_session_id: Some("play-emby".to_string()),
        intro_skipper_ranges: Vec::new(),
        position_ticks: 1_230_000_000,
        is_paused: false,
        is_muted: false,
        volume: 100,
        audio_stream_index: Some(1),
        subtitle_stream_index: Some(2),
        play_method: "DirectStream".to_string(),
      }),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
    });

    SessionManager::report_playback_stopped(&client, &state).await;

    assert!(state.read().playback.is_none());
    let captured = requests.lock();
    assert!(captured[1].starts_with("POST /Sessions/Playing/Stopped "));
    assert!(captured[1].contains(r#""ItemId":"movie-emby""#));
    assert!(captured[1].contains(r#""MediaSourceId":"source-emby""#));
    assert!(captured[1].contains(r#""PlaySessionId":"play-emby""#));
    assert!(captured[1].contains(r#""PositionTicks":1230000000"#));
  }

  #[tokio::test]
  async fn time_pos_update_inside_intro_range_emits_seek_action() {
    let state = test_state_with_intro_range();
    let (action_tx, mut action_rx) = mpsc::channel(1);
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(10.0)),
      reason: None,
      args: None,
    };

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;

    assert!(matches!(
      action_rx.recv().await,
      Some(MpvAction::Seek(80.0))
    ));
  }

  #[tokio::test]
  async fn time_pos_update_inside_already_skipped_range_emits_no_second_seek() {
    let state = test_state_with_intro_range();
    let (action_tx, mut action_rx) = mpsc::channel(2);
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(10.0)),
      reason: None,
      args: None,
    };

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;
    assert!(matches!(
      action_rx.recv().await,
      Some(MpvAction::Seek(80.0))
    ));

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;

    assert!(action_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn time_pos_update_inside_credit_range_emits_seek_not_next_episode_action() {
    let state = test_state_with_range(IntroSkipKind::Credits, 1200.0, 1260.0);
    let (action_tx, mut action_rx) = mpsc::channel(1);
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(1200.0)),
      reason: None,
      args: None,
    };

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;

    assert!(matches!(
      action_rx.recv().await,
      Some(MpvAction::Seek(1260.0))
    ));
  }

  #[tokio::test]
  async fn time_pos_update_without_active_ranges_emits_no_seek_action() {
    let state = RwLock::new(SessionState {
      playback: None,
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
    });
    let (action_tx, mut action_rx) = mpsc::channel(1);
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(10.0)),
      reason: None,
      args: None,
    };

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;

    assert!(action_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn disabled_intro_skipper_setting_emits_no_seek_action() {
    let state = test_state_with_intro_range();
    let (action_tx, mut action_rx) = mpsc::channel(1);
    let config = AppConfig {
      intro_skipper_mode: IntroSkipperMode::Off,
      ..Default::default()
    };
    state.write().effective_intro_skipper_config = IntroSkipperRuntimeConfig::from(&config);
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(10.0)),
      reason: None,
      args: None,
    };

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;

    assert!(action_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn manual_intro_skipper_time_pos_emits_prompt_without_seek() {
    let state = test_state_with_intro_range();
    let (action_tx, mut action_rx) = mpsc::channel(1);
    let config = AppConfig {
      intro_skipper_mode: IntroSkipperMode::Manual,
      keybind_intro_skip: "g".to_string(),
      ..Default::default()
    };
    state.write().effective_intro_skipper_config = IntroSkipperRuntimeConfig::from(&config);
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(10.0)),
      reason: None,
      args: None,
    };

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;

    assert!(matches!(
      action_rx.recv().await,
      Some(MpvAction::ShowText { text, duration_ms: 3000 })
        if text == "Intro available - press g to skip"
    ));
    assert!(action_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn manual_intro_skip_shortcut_emits_seek_and_confirmation() {
    let state = test_state_with_intro_range();
    {
      let mut s = state.write();
      let playback = s.playback.as_mut().unwrap();
      playback.position_ticks = seconds_to_ticks(10.0);
    }
    let (action_tx, mut action_rx) = mpsc::channel(2);
    let config = AppConfig {
      intro_skipper_mode: IntroSkipperMode::Manual,
      ..Default::default()
    };
    state.write().effective_intro_skipper_config = IntroSkipperRuntimeConfig::from(&config);

    SessionManager::handle_manual_intro_skip(&state, &action_tx).await;

    assert!(matches!(
      action_rx.recv().await,
      Some(MpvAction::Seek(80.0))
    ));
    assert!(matches!(
      action_rx.recv().await,
      Some(MpvAction::ShowText { text, duration_ms: 1500 })
        if text == "Skipped intro"
    ));
  }

  #[tokio::test]
  async fn manual_intro_skip_shortcut_without_active_range_shows_unavailable_message() {
    let state = test_state_with_intro_range();
    let (action_tx, mut action_rx) = mpsc::channel(1);
    let config = AppConfig {
      intro_skipper_mode: IntroSkipperMode::Manual,
      ..Default::default()
    };
    state.write().effective_intro_skipper_config = IntroSkipperRuntimeConfig::from(&config);

    SessionManager::handle_manual_intro_skip(&state, &action_tx).await;

    assert!(matches!(
      action_rx.recv().await,
      Some(MpvAction::ShowText { text, duration_ms: 1200 })
        if text == "No intro or credits to skip"
    ));
  }

  #[tokio::test]
  async fn disabled_intro_skipper_setting_blocks_credit_seek_action() {
    let state = test_state_with_range(IntroSkipKind::Credits, 1200.0, 1260.0);
    let (action_tx, mut action_rx) = mpsc::channel(1);
    let config = AppConfig {
      intro_skipper_mode: IntroSkipperMode::Off,
      ..Default::default()
    };
    state.write().effective_intro_skipper_config = IntroSkipperRuntimeConfig::from(&config);
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(1200.0)),
      reason: None,
      args: None,
    };

    SessionManager::apply_intro_skipper(&state, &action_tx, &event).await;

    assert!(action_rx.try_recv().is_err());
  }
}

#[cfg(test)]
mod regression_tests {
  use super::*;

  #[test]
  fn playback_position_updates_to_seek_target_after_mpv_reports_new_time_pos() {
    let state = super::tests::test_state_with_intro_range();
    let event = crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(4),
      name: Some("time-pos".to_string()),
      data: Some(serde_json::json!(80.0)),
      reason: None,
      args: None,
    };

    SessionManager::update_state_from_property(&state, &event);

    let position_ticks = state
      .read()
      .playback
      .as_ref()
      .map(|playback| playback.position_ticks);
    assert_eq!(position_ticks, Some(seconds_to_ticks(80.0)));
  }

  #[test]
  fn parse_command_int_accepts_json_number() {
    let value = serde_json::json!(50);
    assert_eq!(parse_command_int(Some(&value)), Some(50));
  }

  #[test]
  fn parse_command_int_accepts_json_string_with_integer() {
    let value = serde_json::json!("50");
    assert_eq!(parse_command_int(Some(&value)), Some(50));
  }

  #[test]
  fn parse_command_int_returns_none_for_none_input() {
    assert_eq!(parse_command_int(None), None);
  }

  #[test]
  fn parse_command_int_returns_none_for_non_integer_string() {
    let value = serde_json::json!("abc");
    assert_eq!(parse_command_int(Some(&value)), None);
  }

  #[test]
  fn parse_command_int_returns_none_for_float_string() {
    let value = serde_json::json!("50.5");
    assert_eq!(parse_command_int(Some(&value)), None);
  }

  #[test]
  fn parse_command_int_returns_none_for_json_float() {
    let value = serde_json::json!(50.5);
    assert_eq!(parse_command_int(Some(&value)), None);
  }

  #[test]
  fn parse_command_int_returns_none_for_null() {
    let value = serde_json::json!(null);
    assert_eq!(parse_command_int(Some(&value)), None);
  }

  #[test]
  fn parse_command_int_accepts_negative_index() {
    let value = serde_json::json!("-1");
    assert_eq!(parse_command_int(Some(&value)), Some(-1));
  }

  #[test]
  fn parse_command_int_accepts_negative_number() {
    let value = serde_json::json!(-1);
    assert_eq!(parse_command_int(Some(&value)), Some(-1));
  }

  #[test]
  fn redact_url_removes_authenticated_stream_websocket_and_login_secrets() {
    let input = concat!(
      "http://media.test/Videos/1/stream.mkv?MediaSourceId=source-1",
      "&api_key=stream-token",
      "&AccessToken=access-token",
      "&password=login-secret",
      " ws://media.test/socket?api_key=socket-token&deviceId=device-1"
    );

    let redacted = redact_url(input);

    assert!(!redacted.contains("stream-token"));
    assert!(!redacted.contains("access-token"));
    assert!(!redacted.contains("login-secret"));
    assert!(!redacted.contains("socket-token"));
    assert!(redacted.contains("api_key=[REDACTED]"));
    assert!(redacted.contains("AccessToken=[REDACTED]"));
    assert!(redacted.contains("password=[REDACTED]"));
    assert!(redacted.contains("deviceId=device-1"));
  }

  #[test]
  fn jellyfin_general_command_volume_from_string_updates_session_and_sends_action() {
    let state = RwLock::new(SessionState {
      playback: Some(PlaybackSession {
        item_id: "item-1".to_string(),
        media_source_id: Some("source-1".to_string()),
        play_session_id: Some("play-1".to_string()),
        intro_skipper_ranges: vec![],
        position_ticks: 0,
        is_paused: false,
        is_muted: false,
        volume: 100,
        audio_stream_index: None,
        subtitle_stream_index: None,
        play_method: "DirectPlay".to_string(),
      }),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
    });
    let (action_tx, mut action_rx) = mpsc::channel(1);

    // Simulate a SetVolume command with Volume as a string (the real Jellyfin shape)
    let args = serde_json::json!({"Volume": "50"});
    let parsed_volume = parse_command_int(args.get("Volume"));
    assert_eq!(parsed_volume, Some(50));

    // Verify the volume would be clamped and applied
    let volume = parsed_volume.map(|v| v.clamp(0, 100) as i32).unwrap();
    {
      let mut s = state.write();
      if let Some(ref mut playback) = s.playback {
        playback.volume = volume;
      }
    }
    assert_eq!(state.read().playback.as_ref().unwrap().volume, 50);

    // Verify action would be sent
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
      action_tx.send(MpvAction::SetVolume(volume)).await.unwrap();
      assert!(matches!(
        action_rx.recv().await,
        Some(MpvAction::SetVolume(50))
      ));
    });
  }

  #[test]
  fn jellyfin_general_command_volume_from_number_still_works() {
    let args = serde_json::json!({"Volume": 75});
    let parsed_volume = parse_command_int(args.get("Volume"));
    assert_eq!(parsed_volume, Some(75));
  }

  #[test]
  fn jellyfin_general_command_volume_out_of_range_clamps_to_valid() {
    // Above 100
    let parsed = parse_command_int(serde_json::json!({"Volume": "150"}).get("Volume"));
    assert_eq!(parsed, Some(150));
    assert_eq!(parsed.map(|v| v.clamp(0, 100) as i32), Some(100));

    // Below 0
    let parsed = parse_command_int(serde_json::json!({"Volume": "-10"}).get("Volume"));
    assert_eq!(parsed, Some(-10));
    assert_eq!(parsed.map(|v| v.clamp(0, 100) as i32), Some(0));
  }

  #[test]
  fn jellyfin_general_command_volume_missing_and_malformed_ignored() {
    // Missing Volume key
    let args = serde_json::json!({"SomethingElse": "50"});
    assert_eq!(parse_command_int(args.get("Volume")), None);

    // Empty arguments
    let args = serde_json::json!({});
    assert_eq!(parse_command_int(args.get("Volume")), None);

    // Non-numeric string
    let args = serde_json::json!({"Volume": "half"});
    assert_eq!(parse_command_int(args.get("Volume")), None);

    // Null value
    let args = serde_json::json!({"Volume": null});
    assert_eq!(parse_command_int(args.get("Volume")), None);
  }

  #[test]
  fn jellyfin_track_index_from_string_still_works_with_parse_command_int() {
    // String Index
    let args = serde_json::json!({"Index": "2"});
    assert_eq!(parse_command_int(args.get("Index")), Some(2));

    // Number Index
    let args = serde_json::json!({"Index": 2});
    assert_eq!(parse_command_int(args.get("Index")), Some(2));

    // Negative string Index (subtitle disable)
    let args = serde_json::json!({"Index": "-1"});
    assert_eq!(parse_command_int(args.get("Index")), Some(-1));
  }
}
