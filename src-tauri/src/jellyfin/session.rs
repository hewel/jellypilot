//! Session manager - coordinates Jellyfin commands with MPV player.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio::sync::mpsc;

use super::client::JellyfinClient;
use super::error::JellyfinError;
use super::hls_lifecycle;
use super::intro_skipper::{
  evaluate_manual_skip, evaluate_skip, evaluate_skip_prompt, IntroSkipKind,
};
use super::mpv_action::{MpvAction, MpvActionExecutor};
use super::mpv_event::{
  apply_property_update, client_message_direction, is_natural_end, property_report_decision,
  should_report_progress, PropertyReportDecision,
};
use super::play_resolution::{
  jellyfin_to_mpv_track_index, resolve_play_request, PlayResolutionConfig,
};
use super::types::*;
use super::websocket::{JellyfinCommand, JellyfinWebSocket, JellyfinWebSocketEvent};
use crate::command::{AppNotification, NowPlayingChanged, NowPlayingState};
use crate::config::{AppConfig, IntroSkipperMode};
use crate::hls_proxy::HlsProxyState;
use crate::mpv::MpvClient;
use crate::now_playing::{build_now_playing_state, PlaybackContext, TransportSnapshot};
use tauri_specta::Event;

const PREFERENCES_STORE_FILE: &str = "preferences.json";
const SERIES_PREFERENCES_KEY: &str = "series_track_preferences";

/// Shared playback orchestration context threaded through play command handlers.
#[derive(Clone)]
pub(super) struct PlayContext {
  pub(super) client: Arc<JellyfinClient>,
  pub(super) state: Arc<RwLock<SessionState>>,
  pub(super) action_tx: mpsc::Sender<MpvAction>,
  pub(super) hls: HlsProxyState,
  pub(super) app: Option<AppHandle>,
  pub(super) config: Arc<RwLock<AppConfig>>,
}

/// Session manager state.
pub(super) struct SessionState {
  pub(super) playback: Option<PlaybackSession>,
  /// Now Playing transport snapshot maintained from MPV property observations.
  pub(super) transport: TransportSnapshot,
  pub(super) last_report_time: std::time::Instant,
  /// Intro Skipper settings captured when the current MPV process started.
  pub(super) effective_intro_skipper_config: IntroSkipperRuntimeConfig,
  /// Current series ID being played (for track preference saving).
  pub(super) current_series_id: Option<String>,
  /// Current item being played (for next episode lookup).
  pub(super) current_item: Option<MediaItem>,
  /// Current media streams (for looking up track languages).
  pub(super) current_media_streams: Vec<MediaStream>,
  /// Track preferences per series (key: series_id).
  pub(super) series_preferences: HashMap<String, TrackPreference>,
  /// Notifications captured when no AppHandle is available (request-capture tests).
  pub(super) recorded_notifications: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IntroSkipperRuntimeConfig {
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
  hls_proxy: HlsProxyState,
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
    hls_proxy: HlsProxyState,
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
      hls_proxy,
      state: Arc::new(RwLock::new(SessionState {
        playback: None,
        transport: TransportSnapshot::default(),
        last_report_time: std::time::Instant::now(),
        effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&*config.read()),
        current_series_id: None,
        current_item: None,
        current_media_streams: Vec::new(),
        series_preferences,
        recorded_notifications: Vec::new(),
      })),
      action_tx,
      action_rx: Arc::new(RwLock::new(Some(action_rx))),
    }
  }

  /// Bundle the shared orchestration context for command handlers.
  fn play_context(&self) -> PlayContext {
    PlayContext {
      client: self.client.clone(),
      state: self.state.clone(),
      action_tx: self.action_tx.clone(),
      hls: self.hls_proxy.clone(),
      app: Some(self.app_handle.clone()),
      config: self.config.clone(),
    }
  }

  /// Return current media item metadata for user-facing Now Playing state.
  pub fn current_item(&self) -> Option<MediaItem> {
    self.state.read().current_item.clone()
  }

  /// Project the current Now Playing state from the maintained transport
  /// snapshot. This is the single projection owner for every emission path;
  /// it never issues MPV property queries.
  fn project_now_playing(state: &RwLock<SessionState>) -> NowPlayingState {
    let s = state.read();
    let media_runtime_seconds = s
      .current_item
      .as_ref()
      .and_then(|item| item.run_time_ticks)
      .map(ticks_to_seconds);
    let player = s.transport.project(media_runtime_seconds);
    build_now_playing_state(
      player,
      PlaybackContext {
        has_active_session: s.playback.is_some(),
        current_item: s.current_item.as_ref(),
      },
    )
  }

  /// Shared emission owner for Now Playing changes: projects from the
  /// transport snapshot and emits without resampling MPV properties.
  async fn emit_now_playing_changed(app_handle: &AppHandle, state: &RwLock<SessionState>) {
    let event = NowPlayingChanged {
      state: Self::project_now_playing(state),
    };

    if let Err(e) = event.emit(app_handle) {
      log::error!("Failed to emit Now Playing state: {}", e);
    }
  }

  /// Command/tray entry point into the shared emission owner.
  pub async fn emit_now_playing_snapshot(&self) {
    Self::emit_now_playing_changed(&self.app_handle, &self.state).await;
  }

  /// Record a command-driven transport mutation so command-path emission
  /// reflects it before the corresponding MPV observation lands.
  pub fn seed_transport(&self, update: impl FnOnce(&mut TransportSnapshot)) {
    let mut s = self.state.write();
    update(&mut s.transport);
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
    let hls = self.hls_proxy.clone();

    tokio::spawn(async move {
      let Some(mut event_rx) = websocket.take_event_receiver() else {
        log::warn!("No WebSocket event receiver available");
        return;
      };

      let ctx = PlayContext {
        client,
        state,
        action_tx,
        hls,
        app: Some(app_handle.clone()),
        config,
      };

      log::info!("WebSocket command stream consumer started");
      while let Some(event) = event_rx.recv().await {
        match event {
          JellyfinWebSocketEvent::Connected => {
            log::info!("Jellyfin WebSocket connected");
          }
          JellyfinWebSocketEvent::ConnectionLost => {
            log::warn!("Jellyfin WebSocket connection lost");
            Self::clear_playback_context(&ctx.client, &ctx.state, &ctx.hls).await;
            AppNotification::warning(&app_handle, "Connection lost. Reconnecting...");
          }
          JellyfinWebSocketEvent::Reconnected => {
            log::info!("WebSocket reconnected successfully");
            AppNotification::info(&app_handle, "Reconnected to Jellyfin");

            if let Err(e) = ctx.client.playback().report_capabilities().await {
              log::error!("Failed to report capabilities after reconnect: {}", e);
            }
          }
          JellyfinWebSocketEvent::Command(cmd) => {
            if let Err(e) = Self::handle_command(&ctx, &mpv, cmd).await {
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
      let executor = MpvActionExecutor::from_session(
        self.mpv.as_ref().clone(),
        self.config.clone(),
        self.app_handle.clone(),
        {
          let state = self.state.clone();
          let config = self.config.clone();
          move || {
            state.write().effective_intro_skipper_config =
              IntroSkipperRuntimeConfig::from(&*config.read());
          }
        },
      );

      tokio::spawn(async move {
        log::info!("MPV action consumer started, waiting for actions...");
        while let Some(action) = action_rx.recv().await {
          log::info!("Processing MPV action: {:?}", action);
          executor.execute(action).await;
        }
      });
    }
  }

  /// Handle a Jellyfin command.
  async fn handle_command(
    ctx: &PlayContext,
    mpv: &MpvClient,
    cmd: JellyfinCommand,
  ) -> Result<(), JellyfinError> {
    match cmd {
      JellyfinCommand::Play(request) => {
        Self::handle_play(ctx, mpv.is_connected(), request).await?;
      }
      JellyfinCommand::Playstate(request) => {
        Self::handle_playstate(ctx, mpv, request).await?;
      }
      JellyfinCommand::GeneralCommand(request) => {
        Self::handle_general_command(
          &ctx.client,
          &ctx.state,
          &ctx.action_tx,
          ctx.app.as_ref(),
          request,
        )
        .await?;
      }
    }
    Ok(())
  }

  /// Handle Play command.
  async fn handle_play(
    ctx: &PlayContext,
    mpv_connected: bool,
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
    let item = ctx.client.playback().get_item(item_id).await?;
    let title = Self::format_title(&item);
    log::info!("Media title: {}", title);

    // Get playback info
    let start_time_ticks = if ctx.client.provider() == MediaServerProvider::Emby {
      request.start_position_ticks
    } else {
      None
    };
    let playback_info = ctx
      .client
      .playback()
      .get_playback_info(
        item_id,
        start_time_ticks,
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
      let s = ctx.state.read();
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
      let config_guard = ctx.config.read();
      let intro_skipper_config = if mpv_connected {
        ctx.state.read().effective_intro_skipper_config.clone()
      } else {
        IntroSkipperRuntimeConfig::from(&*config_guard)
      };
      (
        config_guard.preferred_subtitle_languages.clone(),
        // The Intro Skipper segments endpoint only exists on Jellyfin
        ctx.client.supports_intro_skipper() && intro_skipper_config.mode != IntroSkipperMode::Off,
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
    let url = ctx
      .client
      .playback()
      .build_stream_url(item_id, media_source)
      .ok_or(JellyfinError::NotConnected)?;
    log::info!("Built stream URL: {}", redact_url(&url));

    // Route finite Emby HLS transcodes through the local proxy when available
    let decision = hls_lifecycle::resolve_playback_url(
      &ctx.client,
      &ctx.hls,
      &item,
      resolution.play_method,
      url,
    )
    .await;
    if let Some(warning) = decision.warning {
      hls_lifecycle::notify(ctx.app.as_ref(), &ctx.state, "warning", warning);
    }
    let url = decision.url;

    let intro_skipper_ranges = if resolution.should_fetch_intro_skipper_ranges {
      match ctx
        .client
        .playback()
        .get_intro_skipper_ranges(item_id)
        .await
      {
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
    let replaced_hls_session_id = {
      let mut s = ctx.state.write();
      let replaced = s
        .playback
        .as_ref()
        .and_then(|playback| playback.hls_proxy_session_id.clone());
      s.current_series_id = item.series_id.clone();
      s.current_item = Some(item.clone());
      s.current_media_streams = media_source.media_streams.clone();
      // Re-seed Now Playing transport for the new session so stale per-item
      // state never leaks across sessions; observations reconcile the rest.
      s.transport
        .reset_for_new_session(ticks_to_seconds(resolution.position_ticks));
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
        hls_proxy_session_id: decision
          .activated
          .as_ref()
          .map(|activated| activated.session_id.clone()),
        hls_recovery_attempted: false,
        hls_recovering: false,
      });
      s.last_report_time = std::time::Instant::now();
      replaced
    };

    // Deactivate the replaced proxy generation only after the new session is stored
    if let Some(replaced_id) = replaced_hls_session_id {
      if let Ok(proxy) = ctx.hls.current() {
        proxy.deactivate(&replaced_id);
      }
    }

    // Consume proxy events for the new activation once its session ID is stored
    if let Some(activated) = decision.activated {
      hls_lifecycle::start_hls_event_consumer(activated, ctx.clone());
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
    ctx
      .client
      .playback()
      .report_playback_start(&start_info)
      .await?;

    // Send action to MPV with converted indices
    log::info!(
      "Sending MpvAction::Play: audio_index {:?} (Jellyfin) -> {:?} (MPV), subtitle_index {:?} (Jellyfin) -> {:?} (MPV)",
      resolution.audio_stream_index,
      resolution.mpv_audio_index,
      resolution.subtitle_stream_index,
      resolution.mpv_subtitle_index
    );
    let _ = ctx
      .action_tx
      .send(MpvAction::Play {
        url,
        play_method: resolution.play_method,
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
        ctx
          .client
          .playback()
          .build_subtitle_url(item_id, &media_source.id, ext_sub_stream)
      {
        log::info!(
          "Loading external subtitle: codec={:?}, url={}",
          ext_sub_stream.codec,
          redact_url(&sub_url)
        );
        let _ = ctx
          .action_tx
          .send(MpvAction::AddExternalSubtitle(sub_url))
          .await;
      } else {
        log::warn!("Failed to build external subtitle URL");
      }
    }

    Ok(())
  }

  /// Format media title for display in MPV.
  pub(super) fn format_title(item: &MediaItem) -> String {
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
    ctx: &PlayContext,
    mpv: &MpvClient,
    request: PlaystateRequest,
  ) -> Result<(), JellyfinError> {
    log::info!("handle_playstate: command={}", request.command);
    match request.command.as_str() {
      "Pause" => {
        log::info!("Processing Pause command");
        {
          let mut s = ctx.state.write();
          if let Some(playback) = s.playback.as_mut() {
            playback.is_paused = true;
          }
        }
        let _ = ctx.action_tx.send(MpvAction::Pause).await;
      }
      "Unpause" => {
        log::info!("Processing Unpause command");
        {
          let mut s = ctx.state.write();
          if let Some(playback) = s.playback.as_mut() {
            playback.is_paused = false;
          }
        }
        let _ = ctx.action_tx.send(MpvAction::Resume).await;
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
            let s = ctx.state.read();
            s.playback.as_ref().map(|p| p.is_paused).unwrap_or(false)
          }
        };
        log::info!("Processing PlayPause command, MPV paused={}", is_paused);
        if is_paused {
          {
            let mut s = ctx.state.write();
            if let Some(playback) = s.playback.as_mut() {
              playback.is_paused = false;
            }
          }
          let _ = ctx.action_tx.send(MpvAction::Resume).await;
        } else {
          {
            let mut s = ctx.state.write();
            if let Some(playback) = s.playback.as_mut() {
              playback.is_paused = true;
            }
          }
          let _ = ctx.action_tx.send(MpvAction::Pause).await;
        }
      }
      "Seek" => {
        if let Some(ticks) = request.seek_position_ticks {
          let position = ticks_to_seconds(ticks);
          {
            let mut s = ctx.state.write();
            if let Some(playback) = s.playback.as_mut() {
              playback.position_ticks = ticks;
            }
          }
          let _ = ctx.action_tx.send(MpvAction::Seek(position)).await;
        }
      }
      "Stop" => {
        log::info!("Processing Stop command");
        // Take the playback session and report stop to Jellyfin
        Self::report_playback_stopped(&ctx.client, &ctx.state, &ctx.hls).await;

        let _ = ctx.action_tx.send(MpvAction::Stop).await;
      }
      "NextTrack" => {
        log::info!("Processing NextTrack command");
        let current_item = {
          let s = ctx.state.read();
          s.current_item.clone()
        };

        if let Some(item) = current_item {
          if let Err(e) = Self::play_adjacent_episode(ctx, &item, true, true).await {
            log::warn!("NextTrack unavailable: {}", e);
          }
        } else {
          log::warn!("NextTrack: No current item to get next episode from");
        }
      }
      "PreviousTrack" => {
        log::info!("Processing PreviousTrack command");
        let current_item = {
          let s = ctx.state.read();
          s.current_item.clone()
        };

        if let Some(item) = current_item {
          if let Err(e) = Self::play_adjacent_episode(ctx, &item, false, true).await {
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
    app: Option<&AppHandle>,
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
      if let Some(app) = app {
        Self::save_preferences_static(state, app);
      }
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
    let hls = self.hls_proxy.clone();

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
              Self::update_transport_from_property(&state, &event);
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
                Self::emit_now_playing_changed(&app_handle, &state).await;
              }
            }
            "end-file" => {
              Self::handle_end_file_event(&event, &ctx).await;
              Self::emit_now_playing_changed(&app_handle, &state).await;
            }
            "client-message" => {
              Self::handle_client_message_event(&event, &ctx).await;
              Self::emit_now_playing_changed(&app_handle, &state).await;
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
        Self::clear_playback_context(&client, &state, &hls).await;
        Self::emit_now_playing_changed(&app_handle, &state).await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
      }
    });
  }

  /// Update the Now Playing transport snapshot from a property-change event.
  fn update_transport_from_property(state: &RwLock<SessionState>, event: &crate::mpv::MpvEvent) {
    let property_name = event.name.as_deref().unwrap_or("");
    let Some(data) = event.data.as_ref() else {
      return;
    };

    state.write().transport.apply_property(property_name, data);
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
  async fn handle_end_file_event(event: &crate::mpv::MpvEvent, ctx: &PlayContext) {
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
    Self::report_playback_stopped(&ctx.client, &ctx.state, &ctx.hls).await;

    // Try to get next episode
    if let Err(e) = Self::play_adjacent_episode(ctx, &item, true, false).await {
      log::info!("Natural end did not start an adjacent episode: {}", e);
    }
  }

  /// Handle MPV client-message event for keyboard shortcuts.
  ///
  /// Users can add to their input.conf:
  ///   Shift+> script-message jellypilot-next
  ///   Shift+< script-message jellypilot-prev
  async fn handle_client_message_event(event: &crate::mpv::MpvEvent, ctx: &PlayContext) {
    let args = match &event.args {
      Some(args) if !args.is_empty() => args,
      _ => return,
    };

    if args[0] == "jellypilot-skip-intro" {
      Self::handle_manual_intro_skip(&ctx.state, &ctx.action_tx).await;
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
    if let Err(e) = Self::play_adjacent_episode(ctx, &item, next, true).await {
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
  async fn report_playback_stopped(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    hls: &HlsProxyState,
  ) {
    let session = {
      let mut s = state.write();
      s.playback.take()
    };

    if let Some(session) = session {
      if let Some(proxy_session_id) = session.hls_proxy_session_id.clone() {
        if let Ok(proxy) = hls.current() {
          proxy.deactivate(&proxy_session_id);
        }
      }
      if session.hls_recovering && session.play_session_id.is_none() {
        // An unrecovered Emby transcode already reported (or terminally failed)
        // its stop; never report the same generation twice.
        log::info!("Skipping remote stop for unrecovered Emby transcode session");
        return;
      }
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
  async fn clear_playback_context(
    client: &JellyfinClient,
    state: &RwLock<SessionState>,
    hls: &HlsProxyState,
  ) {
    // First report stopped to Jellyfin
    Self::report_playback_stopped(client, state, hls).await;

    // Then clear all related state
    let mut s = state.write();
    s.current_item = None;
    s.current_series_id = None;
    s.current_media_streams.clear();
    s.transport.clear();
    log::info!("Playback context cleared");
  }

  /// Play the next or previous episode.
  async fn play_adjacent_episode(
    ctx: &PlayContext,
    current_item: &MediaItem,
    next: bool,
    report_current_stopped: bool,
  ) -> Result<(), String> {
    let result = if next {
      ctx.client.playback().get_next_episode(current_item).await
    } else {
      ctx
        .client
        .playback()
        .get_previous_episode(current_item)
        .await
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
          Self::report_playback_stopped(&ctx.client, &ctx.state, &ctx.hls).await;
        }

        let play_request = PlayRequest {
          item_ids: vec![adjacent_item.id.clone()],
          start_position_ticks: None,
          play_command: "PlayNow".to_string(),
          media_source_id: None,
          audio_stream_index: None,
          subtitle_stream_index: None,
        };

        Self::handle_play(ctx, true, play_request)
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
    ctx: &PlayContext,
    mpv_connected: bool,
    request: VideoLibraryPlayRequest,
  ) -> Result<(), JellyfinError> {
    let play_request = Self::resolve_library_play_request(&ctx.client, request).await?;

    Self::report_playback_stopped(&ctx.client, &ctx.state, &ctx.hls).await;
    Self::handle_play(ctx, mpv_connected, play_request).await
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
    Self::play_library_request(&self.play_context(), self.mpv.is_connected(), request).await
  }

  /// Play the next episode. Called from system tray or UI.
  pub async fn play_next_episode(&self) -> Result<(), String> {
    let current_item = {
      let s = self.state.read();
      s.current_item.clone()
    };

    if let Some(item) = current_item {
      log::info!("Tray/UI: playing next episode");
      Self::play_adjacent_episode(&self.play_context(), &item, true, true).await
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
      Self::play_adjacent_episode(&self.play_context(), &item, false, true).await
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
      if let Some(proxy_session_id) = session.hls_proxy_session_id.clone() {
        if let Ok(proxy) = self.hls_proxy.current() {
          proxy.deactivate(&proxy_session_id);
        }
      }
      if !(session.hls_recovering && session.play_session_id.is_none()) {
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
pub(super) fn redact_url(url: &str) -> String {
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

  pub(super) async fn connected_test_client<S: Into<String>, B: Into<String>>(
    responses: Vec<(S, B)>,
  ) -> (JellyfinClient, RequestLog) {
    let responses = responses
      .into_iter()
      .map(|(status, body)| (status.into(), body.into()))
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

  pub(super) fn test_config() -> RwLock<AppConfig> {
    RwLock::new(AppConfig {
      intro_skipper_mode: IntroSkipperMode::Off,
      ..Default::default()
    })
  }

  pub(super) fn empty_test_state() -> RwLock<SessionState> {
    RwLock::new(SessionState {
      playback: None,
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig {
        mode: IntroSkipperMode::Off,
        keybind_intro_skip: String::new(),
      },
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
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
        hls_proxy_session_id: None,
        hls_recovery_attempted: false,
        hls_recovering: false,
      }),
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
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
        hls_proxy_session_id: None,
        hls_recovery_attempted: false,
        hls_recovering: false,
      }),
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
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
    let client = Arc::new(client);
    let state = Arc::new(test_state_with_active_playback());
    let config = Arc::new(test_config());
    let hls = HlsProxyState::default();
    let (action_tx, mut action_rx) = mpsc::channel(4);
    let ctx = PlayContext {
      client,
      state: state.clone(),
      action_tx,
      hls,
      app: None,
      config,
    };

    SessionManager::play_library_request(
      &ctx,
      true,
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
    let client = Arc::new(client);
    let state = Arc::new(empty_test_state());
    let config = Arc::new(test_config());
    let hls = HlsProxyState::default();
    let (action_tx, mut action_rx) = mpsc::channel(4);
    let ctx = PlayContext {
      client,
      state: state.clone(),
      action_tx,
      hls,
      app: None,
      config,
    };

    SessionManager::play_library_request(
      &ctx,
      false,
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
    let client = Arc::new(client);
    let state = Arc::new(empty_test_state());
    let config = Arc::new(test_config());
    let hls = HlsProxyState::default();
    let (action_tx, mut action_rx) = mpsc::channel(4);
    let ctx = PlayContext {
      client,
      state: state.clone(),
      action_tx,
      hls,
      app: None,
      config,
    };

    SessionManager::play_library_request(
      &ctx,
      false,
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
        hls_proxy_session_id: None,
        hls_recovery_attempted: false,
        hls_recovering: false,
      }),
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
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
        hls_proxy_session_id: None,
        hls_recovery_attempted: false,
        hls_recovering: false,
      }),
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
    });

    SessionManager::report_playback_stopped(&client, &state, &HlsProxyState::default()).await;

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
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
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
mod emby_hls_tests {
  use super::tests::{connected_test_client, empty_test_state, test_config};
  use super::*;
  use crate::hls_proxy::{HlsProxy, HlsProxyState};
  use std::time::Duration;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;

  type RequestLog = Arc<parking_lot::Mutex<Vec<String>>>;

  const EMBY_USER_JSON: &str = r#"{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"}"#;
  const HLS_CONTENT_TYPE: &str = "application/vnd.apple.mpegurl";
  const JSON_CONTENT_TYPE: &str = "application/json";

  const MEDIA_PLAYLIST_1: &str = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:8\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:8.0,\nseg1.ts\n#EXT-X-ENDLIST\n";
  const MEDIA_PLAYLIST_2: &str = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:8\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:8.0,\nseg2.ts\n#EXT-X-ENDLIST\n";

  async fn serve_typed_responses_with_requests(
    responses: Vec<(String, String, String)>,
  ) -> (String, RequestLog) {
    let listener = TcpListener::bind("127.0.0.1:0")
      .await
      .expect("test server should bind");
    let addr = listener.local_addr().expect("test server should have addr");
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&requests);

    tokio::spawn(async move {
      for (status, content_type, response_body) in responses {
        let (mut stream, _) = listener.accept().await.expect("test server should accept");
        let mut buffer = [0; 8192];
        let bytes_read = stream
          .read(&mut buffer)
          .await
          .expect("test server should read request");
        let request = String::from_utf8_lossy(&buffer[..bytes_read]).into_owned();
        captured_requests.lock().push(request);
        let response = format!(
          "HTTP/1.1 {}\r\ncontent-type: {}\r\ncontent-length: {}\r\n\r\n{}",
          status,
          content_type,
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

  async fn connected_emby_client_typed(
    responses: Vec<(String, String, String)>,
  ) -> (JellyfinClient, RequestLog) {
    let (server_url, requests) = serve_typed_responses_with_requests(responses).await;
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

  fn started_hls_state() -> HlsProxyState {
    let cache_root =
      std::env::temp_dir().join(format!("jellypilot-hls-test-{}", uuid::Uuid::new_v4()));
    let state = HlsProxyState::default();
    state.install(HlsProxy::start(Some(cache_root)));
    state.current().expect("HLS proxy should start for tests");
    state
  }

  async fn wait_until(description: &str, condition: impl Fn() -> bool) {
    for _ in 0..400 {
      if condition() {
        return;
      }
      tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {}", description);
  }

  fn hls_playback_info(source_id: &str, transcoding_url: &str, play_session_id: &str) -> String {
    format!(
      r#"{{"MediaSources":[{{"Id":"{}","Protocol":"Http","Container":"ts","SupportsDirectPlay":false,"SupportsDirectStream":false,"SupportsTranscoding":true,"TranscodingUrl":"{}","MediaStreams":[{{"Index":1,"Type":"Audio","Language":"eng","DisplayTitle":"English AAC","Codec":"aac","IsDefault":true}}]}}],"PlaySessionId":"{}"}}"#,
      source_id, transcoding_url, play_session_id
    )
  }

  fn default_playback_info(source: &str) -> String {
    hls_playback_info(source, "/videos/transcode.m3u8", "play-hls-1")
  }

  async fn http_get(url: &str) -> (reqwest::StatusCode, String) {
    let response = reqwest::Client::new()
      .get(url)
      .send()
      .await
      .expect("local proxy request should complete");
    let status = response.status();
    let body = response.text().await.expect("response body should read");
    (status, body)
  }

  fn first_segment_url(playlist_body: &str) -> String {
    playlist_body
      .lines()
      .map(str::trim)
      .find(|line| !line.is_empty() && !line.starts_with('#'))
      .expect("rewritten playlist should contain a segment URI")
      .to_string()
  }

  async fn recv_play_action(
    action_rx: &mut mpsc::Receiver<MpvAction>,
    description: &str,
  ) -> (String, f64) {
    let action = tokio::time::timeout(Duration::from_secs(10), action_rx.recv())
      .await
      .unwrap_or_else(|_| panic!("timed out waiting for {}", description))
      .expect("action channel should stay open");
    match action {
      MpvAction::Play {
        url,
        start_position,
        ..
      } => (url, start_position),
      other => panic!("expected play action for {}, got {:?}", description, other),
    }
  }

  struct PlayHarness {
    client: Arc<JellyfinClient>,
    state: Arc<RwLock<SessionState>>,
    config: Arc<RwLock<AppConfig>>,
    hls: HlsProxyState,
    action_tx: mpsc::Sender<MpvAction>,
    action_rx: mpsc::Receiver<MpvAction>,
  }

  impl PlayHarness {
    fn new(client: JellyfinClient) -> Self {
      let (action_tx, action_rx) = mpsc::channel(8);
      Self {
        client: Arc::new(client),
        state: Arc::new(empty_test_state()),
        config: Arc::new(test_config()),
        hls: started_hls_state(),
        action_tx,
        action_rx,
      }
    }

    async fn play_start(&mut self, item_id: &str) -> (String, f64) {
      let ctx = PlayContext {
        client: self.client.clone(),
        state: self.state.clone(),
        action_tx: self.action_tx.clone(),
        hls: self.hls.clone(),
        app: None,
        config: self.config.clone(),
      };
      SessionManager::play_library_request(
        &ctx,
        false,
        VideoLibraryPlayRequest {
          item_id: item_id.to_string(),
          mode: VideoLibraryPlayMode::Start,
          start_position_seconds: None,
          audio_stream_index: Some(1),
          subtitle_stream_index: None,
        },
      )
      .await
      .unwrap_or_else(|e| panic!("Emby HLS playback should start for {}: {}", item_id, e));
      recv_play_action(&mut self.action_rx, "initial play action").await
    }
  }

  fn assert_local_playlist_url(url: &str) {
    assert!(
      url.starts_with("http://127.0.0.1:"),
      "local URL expected, got {}",
      url
    );
    assert!(url.contains("/hls/"), "proxy route expected, got {}", url);
    assert!(
      url.ends_with(".m3u8"),
      "playlist route expected, got {}",
      url
    );
    assert!(
      !url.contains("emby-token") && !url.contains("api_key"),
      "local URL must not carry credentials: {}",
      url
    );
  }

  #[tokio::test]
  async fn emby_hls_transcode_uses_local_proxy_without_token() {
    let playback_info = default_playback_info("source-hls");
    let (client, requests) = connected_emby_client_typed(vec![
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        EMBY_USER_JSON.to_string(),
      ),
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        r#"{"Id":"movie-hls","Name":"HLS Movie","Type":"Movie","RunTimeTicks":72000000000}"#
          .to_string(),
      ),
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        playback_info,
      ),
      (
        "200 OK".to_string(),
        HLS_CONTENT_TYPE.to_string(),
        MEDIA_PLAYLIST_1.to_string(),
      ),
      (
        "204 No Content".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        String::new(),
      ),
    ])
    .await;
    let mut harness = PlayHarness::new(client);

    let (url, _) = harness.play_start("movie-hls").await;

    assert_local_playlist_url(&url);
    let playback = harness
      .state
      .read()
      .playback
      .clone()
      .expect("playback state");
    assert_eq!(playback.play_method, "Transcode");
    assert!(playback.hls_proxy_session_id.is_some());
    assert!(harness.state.read().recorded_notifications.is_empty());

    let (status, rewritten) = http_get(&url).await;
    assert!(status.is_success());
    let segment_url = first_segment_url(&rewritten);
    assert!(segment_url.starts_with("http://127.0.0.1:"));
    assert!(segment_url.contains("/resource/"));
    assert!(!rewritten.contains("emby-token"));
    assert!(!segment_url.contains("emby-token"));

    let captured = requests.lock();
    assert!(captured[3].starts_with("GET /videos/transcode.m3u8?"));
    assert!(captured[3].contains("api_key=emby-token"));
    assert!(captured[4].starts_with("POST /Sessions/Playing "));
    assert!(captured[4].contains(r#""PlayMethod":"Transcode""#));
  }

  #[tokio::test]
  async fn emby_hls_expiry_restarts_once_at_current_position() {
    let playback_info = default_playback_info("source-hls");
    let fresh_playback_info =
      hls_playback_info("source-hls", "/videos/transcode2.m3u8", "play-hls-2");
    let responses = vec![
      ("200 OK", JSON_CONTENT_TYPE, EMBY_USER_JSON),
      (
        "200 OK",
        JSON_CONTENT_TYPE,
        r#"{"Id":"movie-hls","Name":"HLS Movie","Type":"Movie","RunTimeTicks":72000000000}"#,
      ),
      ("200 OK", JSON_CONTENT_TYPE, playback_info.as_str()),
      ("200 OK", HLS_CONTENT_TYPE, MEDIA_PLAYLIST_1),
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      // The old generation's segment suddenly expires
      ("401 Unauthorized", JSON_CONTENT_TYPE, ""),
      // Recovery: stop old, refresh playback info, activate fresh proxy, start new
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      ("200 OK", JSON_CONTENT_TYPE, fresh_playback_info.as_str()),
      ("200 OK", HLS_CONTENT_TYPE, MEDIA_PLAYLIST_2),
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      // The replacement generation expires as well: terminal, no third attempt
      ("401 Unauthorized", JSON_CONTENT_TYPE, ""),
    ]
    .into_iter()
    .map(|(status, content_type, body)| {
      (
        status.to_string(),
        content_type.to_string(),
        body.to_string(),
      )
    })
    .collect();
    let (client, requests) = connected_emby_client_typed(responses).await;
    let mut harness = PlayHarness::new(client);

    let (first_url, _) = harness.play_start("movie-hls").await;
    assert_local_playlist_url(&first_url);

    // Reach a known position before the origin session expires
    {
      let mut s = harness.state.write();
      s.playback.as_mut().expect("playback state").position_ticks = 950_000_000;
    }

    let (_, rewritten) = http_get(&first_url).await;
    let segment_url = first_segment_url(&rewritten);
    let (segment_status, _) = http_get(&segment_url).await;
    assert!(!segment_status.is_success());

    // Recovery must replace playback exactly once at the captured position
    let (second_url, second_start) =
      recv_play_action(&mut harness.action_rx, "recovery play action").await;
    assert_local_playlist_url(&second_url);
    assert_ne!(first_url, second_url);
    assert_eq!(second_start, 95.0);

    let playback = harness
      .state
      .read()
      .playback
      .clone()
      .expect("playback state after recovery");
    assert_eq!(playback.play_session_id.as_deref(), Some("play-hls-2"));
    assert!(playback.hls_recovery_attempted);
    assert!(!playback.hls_recovering);
    assert!(playback.hls_proxy_session_id.is_some());

    wait_until("recovery requests to complete", || {
      requests.lock().len() >= 10
    })
    .await;
    {
      let captured = requests.lock();
      assert!(captured[5].starts_with("GET /videos/seg1.ts?"));
      assert!(captured[6].starts_with("POST /Sessions/Playing/Stopped "));
      assert!(captured[6].contains(r#""PlaySessionId":"play-hls-1""#));
      assert!(captured[6].contains(r#""PositionTicks":950000000"#));
      assert!(captured[7].starts_with("POST /Items/movie-hls/PlaybackInfo "));
      assert!(captured[7].contains(r#""StartTimeTicks":950000000"#));
      assert!(captured[7].contains(r#""AudioStreamIndex":1"#));
      assert!(captured[8].starts_with("GET /videos/transcode2.m3u8?"));
      assert!(captured[9].starts_with("POST /Sessions/Playing "));
      assert!(captured[9].contains(r#""PlaySessionId":"play-hls-2""#));
      assert!(captured[9].contains(r#""PositionTicks":950000000"#));
    }

    // Expire the replacement generation: terminal notification, no third attempt
    let (_, second_rewritten) = http_get(&second_url).await;
    let second_segment_url = first_segment_url(&second_rewritten);
    let (expired_status, _) = http_get(&second_segment_url).await;
    assert!(!expired_status.is_success());

    wait_until("terminal recovery notification", || {
      harness
        .state
        .read()
        .recorded_notifications
        .iter()
        .any(|(level, message)| {
          level == "error"
            && message == "The Emby transcode session expired again. Restart playback to continue."
        })
    })
    .await;

    let playback = harness
      .state
      .read()
      .playback
      .clone()
      .expect("playback state after terminal expiry");
    assert!(playback.hls_recovering);
    assert_eq!(playback.play_session_id, None);

    // No third playback-info request may ever be issued
    tokio::time::sleep(Duration::from_millis(300)).await;
    let captured = requests.lock();
    assert_eq!(captured.len(), 11);
    assert!(
      captured
        .iter()
        .filter(|request| request.starts_with("POST /Items/movie-hls/PlaybackInfo "))
        .count()
        == 2
    );
  }

  #[tokio::test]
  async fn emby_hls_non_hls_transcode_bypasses_proxy_silently() {
    let playback_info = default_playback_info("source-hls");
    let (client, requests) = connected_emby_client_typed(vec![
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        EMBY_USER_JSON.to_string(),
      ),
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        r#"{"Id":"movie-mp4","Name":"MP4 Movie","Type":"Movie","RunTimeTicks":72000000000}"#
          .to_string(),
      ),
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        playback_info,
      ),
      // The transcode endpoint returns progressive MP4, not HLS
      (
        "200 OK".to_string(),
        "video/mp4".to_string(),
        "not a playlist".to_string(),
      ),
      (
        "204 No Content".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        String::new(),
      ),
    ])
    .await;
    let mut harness = PlayHarness::new(client);

    let (url, _) = harness.play_start("movie-mp4").await;

    assert!(url.ends_with("/videos/transcode.m3u8?api_key=emby-token"));
    let playback = harness
      .state
      .read()
      .playback
      .clone()
      .expect("playback state");
    assert_eq!(playback.play_method, "Transcode");
    assert_eq!(playback.hls_proxy_session_id, None);
    assert!(harness.state.read().recorded_notifications.is_empty());
    let captured = requests.lock();
    assert!(captured[3].starts_with("GET /videos/transcode.m3u8?"));
    assert!(captured[4].starts_with("POST /Sessions/Playing "));
  }

  #[tokio::test]
  async fn emby_hls_transcode_without_runtime_bypasses_proxy() {
    let playback_info = default_playback_info("source-hls");
    let (client, requests) = connected_emby_client_typed(vec![
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        EMBY_USER_JSON.to_string(),
      ),
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        r#"{"Id":"movie-live","Name":"Live Movie","Type":"Movie"}"#.to_string(),
      ),
      (
        "200 OK".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        playback_info,
      ),
      (
        "204 No Content".to_string(),
        JSON_CONTENT_TYPE.to_string(),
        String::new(),
      ),
    ])
    .await;
    let mut harness = PlayHarness::new(client);

    let (url, _) = harness.play_start("movie-live").await;

    assert!(url.ends_with("/videos/transcode.m3u8?api_key=emby-token"));
    let playback = harness
      .state
      .read()
      .playback
      .clone()
      .expect("playback state");
    assert_eq!(playback.hls_proxy_session_id, None);
    let captured = requests.lock();
    assert_eq!(captured.len(), 4);
    assert!(captured[3].starts_with("POST /Sessions/Playing "));
  }

  #[tokio::test]
  async fn jellyfin_transcode_bypasses_proxy() {
    let playback_info = default_playback_info("source-hls");
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
        r#"{"Id":"movie-jf","Name":"JF Movie","Type":"Movie","RunTimeTicks":72000000000}"#,
      ),
      ("200 OK", playback_info.as_str()),
      ("204 No Content", ""),
    ])
    .await;
    let mut harness = PlayHarness::new(client);

    let (url, _) = harness.play_start("movie-jf").await;

    assert!(url.contains("/videos/transcode.m3u8"));
    assert!(url.contains("api_key=token-1"));
    assert!(
      !url.contains("/hls/"),
      "Jellyfin playback must not enter the HLS proxy: {}",
      url
    );
    let playback = harness
      .state
      .read()
      .playback
      .clone()
      .expect("playback state");
    assert_eq!(playback.play_method, "Transcode");
    assert_eq!(playback.hls_proxy_session_id, None);
    let captured = requests.lock();
    assert_eq!(captured.len(), 5);
    assert!(captured[4].starts_with("POST /Sessions/Playing "));
  }

  #[tokio::test]
  async fn emby_hls_cleanup_deactivates_proxy_sessions_on_every_exit() {
    let playback_info = default_playback_info("source-hls");
    let episode_info = hls_playback_info("source-ep", "/videos/episode.m3u8", "play-ep-2");
    let next_episode_info = hls_playback_info("source-ep3", "/videos/episode3.m3u8", "play-ep-3");
    let final_info = hls_playback_info("source-final", "/videos/final.m3u8", "play-final");
    let responses = vec![
      ("200 OK", JSON_CONTENT_TYPE, EMBY_USER_JSON),
      // First playback: a movie
      (
        "200 OK",
        JSON_CONTENT_TYPE,
        r#"{"Id":"movie-hls","Name":"HLS Movie","Type":"Movie","RunTimeTicks":72000000000}"#,
      ),
      ("200 OK", JSON_CONTENT_TYPE, playback_info.as_str()),
      ("200 OK", HLS_CONTENT_TYPE, MEDIA_PLAYLIST_1),
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      // Explicit replacement: stop report then an episode
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      (
        "200 OK",
        JSON_CONTENT_TYPE,
        r#"{"Id":"ep-2","Name":"Episode 2","Type":"Episode","SeriesId":"series-hls","SeriesName":"HLS Show","ParentIndexNumber":1,"IndexNumber":2,"RunTimeTicks":36000000000}"#,
      ),
      ("200 OK", JSON_CONTENT_TYPE, episode_info.as_str()),
      ("200 OK", HLS_CONTENT_TYPE, MEDIA_PLAYLIST_1),
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      // Natural end: stop report, adjacent-episode lookup, then it plays
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      (
        "200 OK",
        JSON_CONTENT_TYPE,
        r#"{"Items":[{"Id":"ep-2","Name":"Episode 2","Type":"Episode","SeriesId":"series-hls","SeriesName":"HLS Show","ParentIndexNumber":1,"IndexNumber":2,"RunTimeTicks":36000000000},{"Id":"ep-3","Name":"Episode 3","Type":"Episode","SeriesId":"series-hls","SeriesName":"HLS Show","ParentIndexNumber":1,"IndexNumber":3,"RunTimeTicks":36000000000,"UserData":{"PlaybackPositionTicks":0,"Played":false}}],"TotalRecordCount":2}"#,
      ),
      (
        "200 OK",
        JSON_CONTENT_TYPE,
        r#"{"Id":"ep-3","Name":"Episode 3","Type":"Episode","SeriesId":"series-hls","SeriesName":"HLS Show","ParentIndexNumber":1,"IndexNumber":3,"RunTimeTicks":36000000000}"#,
      ),
      ("200 OK", JSON_CONTENT_TYPE, next_episode_info.as_str()),
      ("200 OK", HLS_CONTENT_TYPE, MEDIA_PLAYLIST_2),
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      // MPV disconnect: stop report while clearing the context
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      // Final playback, ended by an explicit stop
      (
        "200 OK",
        JSON_CONTENT_TYPE,
        r#"{"Id":"movie-final","Name":"Final Movie","Type":"Movie","RunTimeTicks":72000000000}"#,
      ),
      ("200 OK", JSON_CONTENT_TYPE, final_info.as_str()),
      ("200 OK", HLS_CONTENT_TYPE, MEDIA_PLAYLIST_1),
      ("204 No Content", JSON_CONTENT_TYPE, ""),
      ("204 No Content", JSON_CONTENT_TYPE, ""),
    ]
    .into_iter()
    .map(|(status, content_type, body)| {
      (
        status.to_string(),
        content_type.to_string(),
        body.to_string(),
      )
    })
    .collect();
    let (client, requests) = connected_emby_client_typed(responses).await;
    let mut harness = PlayHarness::new(client);

    // Even in Automatic mode, Emby must never call the Jellyfin-only
    // Intro Skipper plugin endpoint (it 404s on every episode)
    harness.config.write().intro_skipper_mode = IntroSkipperMode::Automatic;
    harness.state.write().effective_intro_skipper_config = IntroSkipperRuntimeConfig {
      mode: IntroSkipperMode::Automatic,
      keybind_intro_skip: "i".to_string(),
    };

    // 1. Explicit replacement deactivates the first proxy session
    let (url_a, _) = harness.play_start("movie-hls").await;
    let (url_b, _) = harness.play_start("ep-2").await;
    assert_ne!(url_a, url_b);
    let (status_a, _) = http_get(&url_a).await;
    assert_eq!(status_a, reqwest::StatusCode::NOT_FOUND);

    // 2. Natural end plays the adjacent episode and deactivates its predecessor
    let end_event = crate::mpv::MpvEvent {
      event: "end-file".to_string(),
      id: None,
      name: None,
      data: None,
      reason: Some("eof".to_string()),
      args: None,
    };
    let end_ctx = PlayContext {
      client: harness.client.clone(),
      state: harness.state.clone(),
      action_tx: harness.action_tx.clone(),
      hls: harness.hls.clone(),
      app: None,
      config: harness.config.clone(),
    };
    SessionManager::handle_end_file_event(&end_event, &end_ctx).await;
    let (url_c, _) = recv_play_action(&mut harness.action_rx, "adjacent episode play action").await;
    assert_ne!(url_b, url_c);
    let (status_b, _) = http_get(&url_b).await;
    assert_eq!(status_b, reqwest::StatusCode::NOT_FOUND);

    // 3. MPV disconnect clears the context and deactivates the proxy session
    SessionManager::clear_playback_context(&harness.client, &harness.state, &harness.hls).await;
    let (status_c, _) = http_get(&url_c).await;
    assert_eq!(status_c, reqwest::StatusCode::NOT_FOUND);

    // 4. Explicit stop deactivates the final proxy session
    let (url_d, _) = harness.play_start("movie-final").await;
    SessionManager::report_playback_stopped(&harness.client, &harness.state, &harness.hls).await;
    let (status_d, _) = http_get(&url_d).await;
    assert_eq!(status_d, reqwest::StatusCode::NOT_FOUND);

    // Replacement, natural end, MPV disconnect, and explicit stop each report once
    let captured = requests.lock();
    let stopped_reports = captured
      .iter()
      .filter(|request| request.starts_with("POST /Sessions/Playing/Stopped "))
      .count();
    assert_eq!(stopped_reports, 4);
    assert!(
      !captured
        .iter()
        .any(|request| request.contains("IntroSkipperSegments")),
      "Emby playback must not query the Intro Skipper plugin endpoint"
    );
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
        hls_proxy_session_id: None,
        hls_recovery_attempted: false,
        hls_recovering: false,
      }),
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: None,
      current_item: None,
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
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

  fn transport_observation(name: &str, data: serde_json::Value) -> crate::mpv::MpvEvent {
    crate::mpv::MpvEvent {
      event: "property-change".to_string(),
      id: Some(1),
      name: Some(name.to_string()),
      data: Some(data),
      reason: None,
      args: None,
    }
  }

  fn test_state_with_episode_playback() -> RwLock<SessionState> {
    RwLock::new(SessionState {
      playback: Some(PlaybackSession {
        item_id: "episode-1".to_string(),
        media_source_id: Some("source-1".to_string()),
        play_session_id: Some("play-1".to_string()),
        intro_skipper_ranges: Vec::new(),
        position_ticks: seconds_to_ticks(42.5),
        is_paused: true,
        is_muted: true,
        volume: 64,
        audio_stream_index: None,
        subtitle_stream_index: None,
        play_method: "DirectPlay".to_string(),
        hls_proxy_session_id: None,
        hls_recovery_attempted: false,
        hls_recovering: false,
      }),
      transport: TransportSnapshot::default(),
      last_report_time: std::time::Instant::now(),
      effective_intro_skipper_config: IntroSkipperRuntimeConfig::from(&AppConfig::default()),
      current_series_id: Some("series-1".to_string()),
      current_item: Some(MediaItem {
        id: "episode-1".to_string(),
        name: "The Pilot".to_string(),
        item_type: "Episode".to_string(),
        series_id: Some("series-1".to_string()),
        series_name: Some("Example Show".to_string()),
        season_name: Some("Season 1".to_string()),
        index_number: Some(1),
        parent_index_number: Some(1),
        run_time_ticks: Some(15_000_000_000),
        overview: None,
      }),
      current_media_streams: Vec::new(),
      series_preferences: HashMap::new(),
      recorded_notifications: Vec::new(),
    })
  }

  #[tokio::test]
  async fn snapshot_projection_emits_full_state_without_mpv_property_queries() {
    use crate::command::NowPlayingStatus;
    use crate::mpv::MpvIpc;
    use crate::now_playing::collect_player_state;
    use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

    // MPV test seam: in-memory duplex IPC with a recording peer that answers
    // every command so property queries would succeed if any were issued.
    let mpv = MpvClient::new(None);
    let (client_stream, peer_stream) = duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(client_stream);
    let ipc = MpvIpc::from_io_for_test(reader, writer)
      .await
      .expect("test IPC should be constructed");
    mpv.install_ipc_for_test(ipc);

    let wire_log = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let peer_log = Arc::clone(&wire_log);
    let (peer_reader, mut peer_writer) = tokio::io::split(peer_stream);
    let peer = tokio::spawn(async move {
      let mut lines = BufReader::new(peer_reader).lines();
      while let Ok(Some(line)) = lines.next_line().await {
        peer_log.lock().expect("wire log").push(line.clone());
        let request_id = serde_json::from_str::<serde_json::Value>(&line)
          .ok()
          .and_then(|value| value.get("request_id").and_then(|id| id.as_i64()));
        if let Some(request_id) = request_id {
          let _ = peer_writer
            .write_all(
              format!(
                r#"{{"request_id":{},"error":"success","data":null}}"#,
                request_id
              )
              .as_bytes(),
            )
            .await;
          let _ = peer_writer.write_all(b"\n").await;
        }
      }
    });

    // Vacuity check: the legacy live collection fans out five property
    // queries, proving the recording peer would observe any query.
    let _ = collect_player_state(&mpv).await;
    let live_queries = wire_log
      .lock()
      .expect("wire log")
      .iter()
      .filter(|line| line.contains("get_property"))
      .count();
    assert_eq!(live_queries, 5);
    wire_log.lock().expect("wire log").clear();

    // Hot path: MPV observations feed the snapshot, then emission projects it.
    let state = test_state_with_episode_playback();
    for (name, data) in [
      ("pause", serde_json::json!(true)),
      ("volume", serde_json::json!(64.0)),
      ("mute", serde_json::json!(true)),
      ("time-pos", serde_json::json!(42.5)),
      ("duration", serde_json::json!(1420.0)),
    ] {
      SessionManager::update_transport_from_property(&state, &transport_observation(name, data));
    }

    let projected = SessionManager::project_now_playing(&state);

    let hot_queries = wire_log
      .lock()
      .expect("wire log")
      .iter()
      .filter(|line| line.contains("get_property"))
      .count();
    assert_eq!(hot_queries, 0);

    assert!(matches!(projected.status, NowPlayingStatus::Paused));
    assert!(projected.player.connected);
    assert!(projected.player.paused);
    assert!(projected.player.muted);
    assert_eq!(projected.player.volume, 64.0);
    assert_eq!(projected.player.time_pos, 42.5);
    assert_eq!(projected.player.duration, 1420.0);
    let media = projected.media.expect("episode media");
    assert_eq!(media.item_id, "episode-1");
    assert_eq!(media.name, "The Pilot");
    assert!(projected.can_play_next);
    assert!(projected.can_play_previous);

    // Missing observed duration falls back to the current media runtime,
    // still without property queries.
    SessionManager::update_transport_from_property(
      &state,
      &transport_observation("duration", serde_json::json!(null)),
    );
    let fallback = SessionManager::project_now_playing(&state);
    assert_eq!(fallback.player.duration, 1500.0);
    let fallback_queries = wire_log
      .lock()
      .expect("wire log")
      .iter()
      .filter(|line| line.contains("get_property"))
      .count();
    assert_eq!(fallback_queries, 0);

    peer.abort();
  }
}
