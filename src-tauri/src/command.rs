use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::specta;
#[cfg(debug_assertions)]
use specta_typescript::Typescript;
use std::sync::Arc;
use tauri::State;
use tauri_specta::{collect_commands, collect_events, Builder, Event};

use crate::auth_profiles::{load_profiles, save_profiles, SavedServiceProfiles};
use crate::config::AppConfig;
use crate::jellyfin::{
  ConnectionState, Credentials, JellyfinClient, JellyfinError, QuickConnectRequest,
  QuickConnectStatus, SavedSession, SessionManager, VideoHome, VideoItemDetail, VideoLibraryPage,
  VideoLibraryPageRequest, VideoLibraryPlayRequest, VideoLibraryShortcut, VideoSearchPage,
  VideoSearchRequest, VideoSeasonEpisodes, VideoSeasonEpisodesRequest, VideoShowDetail,
  VideoUserDataUpdate, VideoUserDataUpdateRequest,
};
use crate::mpv::{write_input_conf, MpvClient, PropertyValue};
use crate::playback_control;

// ============================================================================
// Events
// ============================================================================

/// Notification level for UI display.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum NotificationLevel {
  Error,
  Warning,
  Info,
  Success,
}

/// App notification event emitted to frontend.
#[derive(Debug, Clone, Serialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct AppNotification {
  pub level: NotificationLevel,
  pub message: String,
}

impl AppNotification {
  /// Emit an error notification to the frontend.
  pub fn error(app: &tauri::AppHandle, message: impl Into<String>) {
    let notification = Self {
      level: NotificationLevel::Error,
      message: message.into(),
    };
    if let Err(e) = notification.emit(app) {
      log::error!("Failed to emit error notification: {}", e);
    }
  }

  /// Emit a warning notification to the frontend.
  pub fn warning(app: &tauri::AppHandle, message: impl Into<String>) {
    let notification = Self {
      level: NotificationLevel::Warning,
      message: message.into(),
    };
    if let Err(e) = notification.emit(app) {
      log::error!("Failed to emit warning notification: {}", e);
    }
  }

  /// Emit an info notification to the frontend.
  #[allow(dead_code)]
  pub fn info(app: &tauri::AppHandle, message: impl Into<String>) {
    let notification = Self {
      level: NotificationLevel::Info,
      message: message.into(),
    };
    if let Err(e) = notification.emit(app) {
      log::error!("Failed to emit info notification: {}", e);
    }
  }

  /// Emit a success notification to the frontend.
  #[allow(dead_code)]
  pub fn success(app: &tauri::AppHandle, message: impl Into<String>) {
    let notification = Self {
      level: NotificationLevel::Success,
      message: message.into(),
    };
    if let Err(e) = notification.emit(app) {
      log::error!("Failed to emit success notification: {}", e);
    }
  }
}

// ============================================================================
// Errors
// ============================================================================

/// Error codes for frontend to distinguish error types.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum CommandErrorCode {
  /// MPV player is not connected.
  NotConnected,
  /// Resource not found (e.g., MPV executable, media file).
  NotFound,
  /// Invalid input provided by the caller.
  InvalidInput,
  /// Network or connection error.
  Network,
  /// Authentication failed.
  AuthFailed,
  /// Internal error (catch-all).
  Internal,
}

/// Typed command error for better frontend error handling.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
  pub code: CommandErrorCode,
  pub message: String,
}

impl CommandError {
  #[allow(dead_code)]
  pub fn not_connected(message: impl Into<String>) -> Self {
    Self {
      code: CommandErrorCode::NotConnected,
      message: message.into(),
    }
  }

  #[allow(dead_code)]
  pub fn not_found(message: impl Into<String>) -> Self {
    Self {
      code: CommandErrorCode::NotFound,
      message: message.into(),
    }
  }

  pub fn invalid_input(message: impl Into<String>) -> Self {
    Self {
      code: CommandErrorCode::InvalidInput,
      message: message.into(),
    }
  }

  pub fn network(message: impl Into<String>) -> Self {
    Self {
      code: CommandErrorCode::Network,
      message: message.into(),
    }
  }

  pub fn auth_failed(message: impl Into<String>) -> Self {
    Self {
      code: CommandErrorCode::AuthFailed,
      message: message.into(),
    }
  }

  pub fn internal(message: impl Into<String>) -> Self {
    Self {
      code: CommandErrorCode::Internal,
      message: message.into(),
    }
  }
}

impl std::fmt::Display for CommandError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}: {}", self.code, self.message)
  }
}

impl std::error::Error for CommandError {}

/// Helper to convert any error to CommandError::Internal
fn internal_err(e: impl std::fmt::Display) -> CommandError {
  CommandError::internal(e.to_string())
}

fn jellyfin_err(e: JellyfinError) -> CommandError {
  match e {
    JellyfinError::InvalidUrl(message) => CommandError::invalid_input(message),
    JellyfinError::QuickConnectUnavailable => {
      CommandError::auth_failed("Quick Connect is not enabled on this server")
    }
    JellyfinError::AuthFailed(message) => CommandError::auth_failed(message),
    JellyfinError::Http(_) | JellyfinError::HttpError(_) => CommandError::network(e.to_string()),
    JellyfinError::NotConnected | JellyfinError::SessionNotFound => {
      CommandError::not_connected(e.to_string())
    }
    JellyfinError::WebSocket(_) | JellyfinError::Json(_) => internal_err(e),
  }
}

async fn start_remote_control_session_if_supported(
  app: &tauri::AppHandle,
  state: &JellyfinState,
  config_state: &ConfigState,
) -> Result<(), CommandError> {
  let new_session = Arc::new(SessionManager::new(
    state.client.clone(),
    state.mpv.clone(),
    config_state.0.clone(),
    app.clone(),
  ));

  if !state.client.supports_remote_control() {
    new_session.start_local().await.map_err(internal_err)?;
    let old_session = state.session.write().replace(new_session);
    if let Some(old) = old_session {
      if let Err(e) = old.stop().await {
        log::warn!("Failed to stop old session: {}", e);
      }
    }
    playback_control::emit_now_playing_changed(app, state).await;
    return Ok(());
  }

  new_session.start().await.map_err(internal_err)?;

  let old_session = state.session.write().replace(new_session);
  if let Some(old) = old_session {
    if let Err(e) = old.stop().await {
      log::warn!("Failed to stop old session: {}", e);
    }
  }
  playback_control::emit_now_playing_changed(app, state).await;

  Ok(())
}

// ============================================================================
// Types
// ============================================================================

/// Player transport state returned to frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
  pub connected: bool,
  pub paused: bool,
  pub muted: bool,
  pub time_pos: f64,
  pub duration: f64,
  pub volume: f64,
}

impl Default for PlayerState {
  fn default() -> Self {
    Self {
      connected: false,
      paused: true,
      muted: false,
      time_pos: 0.0,
      duration: 0.0,
      volume: 100.0,
    }
  }
}

/// User-facing Now Playing status.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NowPlayingStatus {
  Offline,
  Idle,
  Playing,
  Paused,
  Unknown,
}

/// Reason an adjacent episode control is unavailable.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AdjacentEpisodeUnavailableReason {
  NoSession,
  NoCurrentItem,
  NotEpisode,
  Unknown,
}

/// Minimal current media metadata safe for UI display.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingMedia {
  pub item_id: String,
  pub name: String,
  pub item_type: String,
  pub series_name: Option<String>,
  pub season_number: Option<i32>,
  pub episode_number: Option<i32>,
}

/// User-facing playback state for the Operations Console.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingState {
  pub status: NowPlayingStatus,
  pub player: PlayerState,
  pub media: Option<NowPlayingMedia>,
  pub can_play_next: bool,
  pub can_play_previous: bool,
  pub next_unavailable_reason: Option<AdjacentEpisodeUnavailableReason>,
  pub previous_unavailable_reason: Option<AdjacentEpisodeUnavailableReason>,
}

/// Now Playing state event emitted to frontend.
#[derive(Debug, Clone, Serialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct NowPlayingChanged {
  pub state: NowPlayingState,
}

/// MPV client state managed by Tauri.
pub struct MpvState(pub Arc<MpvClient>);

/// Jellyfin client state managed by Tauri.
pub struct JellyfinState {
  pub client: Arc<JellyfinClient>,
  pub mpv: Arc<MpvClient>,
  pub session: RwLock<Option<Arc<SessionManager>>>,
}

impl JellyfinState {
  pub fn new(client: Arc<JellyfinClient>, mpv: Arc<MpvClient>) -> Self {
    Self {
      client,
      mpv,
      session: RwLock::new(None),
    }
  }
}

// ============================================================================
// MPV Commands
// ============================================================================

/// Start the MPV player.
#[tauri::command]
#[specta]
pub async fn mpv_start(
  app: tauri::AppHandle,
  state: State<'_, MpvState>,
  jellyfin_state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  state.0.start().await.map_err(internal_err)?;
  playback_control::emit_now_playing_changed(&app, &jellyfin_state).await;
  Ok(())
}

/// Stop the MPV player.
#[tauri::command]
#[specta]
pub async fn mpv_stop(
  app: tauri::AppHandle,
  state: State<'_, MpvState>,
  jellyfin_state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  state.0.stop().await;
  playback_control::emit_now_playing_changed(&app, &jellyfin_state).await;
  Ok(())
}

/// Load a media file/URL for playback.
#[tauri::command]
#[specta]
pub async fn mpv_loadfile(state: State<'_, MpvState>, url: String) -> Result<(), CommandError> {
  // Validate URL scheme for security
  if !url.starts_with("http://") && !url.starts_with("https://") {
    return Err(CommandError::invalid_input(
      "Only http:// and https:// URLs are allowed",
    ));
  }
  state.0.loadfile(&url).await.map_err(internal_err)
}

/// Seek to absolute position in seconds.
#[tauri::command]
#[specta]
pub async fn mpv_seek(
  app: tauri::AppHandle,
  state: State<'_, MpvState>,
  jellyfin_state: State<'_, JellyfinState>,
  time: f64,
) -> Result<(), CommandError> {
  if time < 0.0 {
    return Err(CommandError::invalid_input("Seek time cannot be negative"));
  }
  state.0.seek(time).await.map_err(internal_err)?;
  playback_control::emit_now_playing_changed(&app, &jellyfin_state).await;
  Ok(())
}

/// Set pause state.
#[tauri::command]
#[specta]
pub async fn mpv_set_pause(
  app: tauri::AppHandle,
  state: State<'_, MpvState>,
  jellyfin_state: State<'_, JellyfinState>,
  paused: bool,
) -> Result<(), CommandError> {
  playback_control::set_pause(&app, &state.0, &jellyfin_state, paused).await
}

/// Set volume (0-100).
#[tauri::command]
#[specta]
pub async fn mpv_set_volume(
  app: tauri::AppHandle,
  state: State<'_, MpvState>,
  jellyfin_state: State<'_, JellyfinState>,
  volume: f64,
) -> Result<(), CommandError> {
  if !(0.0..=100.0).contains(&volume) {
    return Err(CommandError::invalid_input(
      "Volume must be between 0 and 100",
    ));
  }
  state.0.set_volume(volume).await.map_err(internal_err)?;
  playback_control::emit_now_playing_changed(&app, &jellyfin_state).await;
  Ok(())
}

/// Set audio track by ID.
#[tauri::command]
#[specta]
pub async fn mpv_set_audio_track(state: State<'_, MpvState>, id: i32) -> Result<(), CommandError> {
  state
    .0
    .set_audio_track(id as i64)
    .await
    .map_err(internal_err)
}

/// Set subtitle track by ID, or disable subtitles with a negative ID.
#[tauri::command]
#[specta]
pub async fn mpv_set_subtitle_track(
  state: State<'_, MpvState>,
  id: i32,
) -> Result<(), CommandError> {
  if id < 0 {
    return state.0.disable_track("sid").await.map_err(internal_err);
  }
  state
    .0
    .set_subtitle_track(id as i64)
    .await
    .map_err(internal_err)
}

/// Get a property value from MPV.
#[tauri::command]
#[specta]
pub async fn mpv_get_property(
  state: State<'_, MpvState>,
  name: String,
) -> Result<PropertyValue, CommandError> {
  state.0.get_property(&name).await.map_err(internal_err)
}

/// Toggle mute state.
#[tauri::command]
#[specta]
pub async fn mpv_toggle_mute(
  app: tauri::AppHandle,
  state: State<'_, MpvState>,
  jellyfin_state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  playback_control::toggle_mute(&app, &state.0, &jellyfin_state).await
}

/// Get current player state.
#[tauri::command]
#[specta]
pub async fn mpv_get_state(state: State<'_, MpvState>) -> Result<PlayerState, CommandError> {
  Ok(crate::now_playing::collect_player_state(&state.0).await)
}

/// Get current user-facing Now Playing state.
#[tauri::command]
#[specta]
pub async fn now_playing_get_state(
  state: State<'_, JellyfinState>,
) -> Result<NowPlayingState, CommandError> {
  Ok(playback_control::collect_now_playing_state(&state).await)
}

/// Check if MPV is connected.
#[tauri::command]
#[specta]
pub fn mpv_is_connected(state: State<'_, MpvState>) -> bool {
  state.0.is_connected()
}

// ============================================================================
// Jellyfin Commands
// ============================================================================

/// Connect to a Jellyfin server.
#[tauri::command]
#[specta]
pub async fn jellyfin_connect(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  config_state: State<'_, ConfigState>,
  credentials: Credentials,
) -> Result<(), CommandError> {
  // Authenticate with server
  state
    .client
    .login()
    .authenticate(&credentials)
    .await
    .map_err(jellyfin_err)?;

  start_remote_control_session_if_supported(&app, &state, &config_state).await
}

/// Start a Jellyfin Quick Connect request.
#[tauri::command]
#[specta]
pub async fn jellyfin_quick_connect_start(
  state: State<'_, JellyfinState>,
  server_url: String,
) -> Result<QuickConnectRequest, CommandError> {
  state
    .client
    .login()
    .quick_connect_start(&server_url)
    .await
    .map_err(jellyfin_err)
}

/// Check whether a Jellyfin Quick Connect request has been approved.
#[tauri::command]
#[specta]
pub async fn jellyfin_quick_connect_check(
  state: State<'_, JellyfinState>,
  server_url: String,
  secret: String,
) -> Result<QuickConnectStatus, CommandError> {
  state
    .client
    .login()
    .quick_connect_check(&server_url, &secret)
    .await
    .map_err(jellyfin_err)
}

/// Complete Jellyfin Quick Connect authentication.
#[tauri::command]
#[specta]
pub async fn jellyfin_quick_connect_authenticate(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  config_state: State<'_, ConfigState>,
  server_url: String,
  secret: String,
) -> Result<(), CommandError> {
  state
    .client
    .login()
    .quick_connect_authenticate(&server_url, &secret)
    .await
    .map_err(jellyfin_err)?;

  start_remote_control_session_if_supported(&app, &state, &config_state).await
}

/// Disconnect from Jellyfin server.
#[tauri::command]
#[specta]
pub async fn jellyfin_disconnect(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  // Take session without holding lock across await
  let session = state.session.write().take();

  // Stop session if active
  if let Some(session) = session {
    session.stop().await.map_err(internal_err)?;
  }

  // Disconnect client
  state.client.login().disconnect();
  playback_control::emit_now_playing_changed(&app, &state).await;

  Ok(())
}

/// Get Jellyfin connection state.
#[tauri::command]
#[specta]
pub fn jellyfin_get_state(state: State<'_, JellyfinState>) -> ConnectionState {
  state.client.login().connection_state()
}

/// Check if connected to Jellyfin.
#[tauri::command]
#[specta]
pub fn jellyfin_is_connected(state: State<'_, JellyfinState>) -> bool {
  state.client.login().is_connected()
}

/// Load the Library Browser Video Home dashboard data.
#[tauri::command]
#[specta]
pub async fn library_video_home(
  state: State<'_, JellyfinState>,
) -> Result<VideoHome, CommandError> {
  state
    .client
    .library()
    .video_home()
    .await
    .map_err(jellyfin_err)
}

/// Load Movies and Shows library shortcuts for Library Browser navigation.
#[tauri::command]
#[specta]
pub async fn library_video_shortcuts(
  state: State<'_, JellyfinState>,
) -> Result<Vec<VideoLibraryShortcut>, CommandError> {
  state
    .client
    .library()
    .library_shortcuts()
    .await
    .map_err(jellyfin_err)
}

/// Resolve the Movies or Shows library shortcut containing the given item, if any.
#[tauri::command]
#[specta]
pub async fn library_item_shortcut(
  state: State<'_, JellyfinState>,
  item_id: String,
) -> Result<Option<VideoLibraryShortcut>, CommandError> {
  state
    .client
    .library()
    .item_shortcut(item_id)
    .await
    .map_err(jellyfin_err)
}

/// Load one server-paged Movies or Shows library result page.
#[tauri::command]
#[specta]
pub async fn library_browse_video(
  state: State<'_, JellyfinState>,
  request: VideoLibraryPageRequest,
) -> Result<VideoLibraryPage, CommandError> {
  state
    .client
    .library()
    .browse_video(request)
    .await
    .map_err(jellyfin_err)
}

/// Search Movies, Shows, and Episodes with server paging.
#[tauri::command]
#[specta]
pub async fn library_search_video(
  state: State<'_, JellyfinState>,
  request: VideoSearchRequest,
) -> Result<VideoSearchPage, CommandError> {
  state
    .client
    .library()
    .search_video(request)
    .await
    .map_err(jellyfin_err)
}

/// Load Movie or Episode details for the Library Browser.
#[tauri::command]
#[specta]
pub async fn library_item_detail(
  state: State<'_, JellyfinState>,
  item_id: String,
) -> Result<VideoItemDetail, CommandError> {
  state
    .client
    .library()
    .item_detail(item_id)
    .await
    .map_err(jellyfin_err)
}

/// Load Show details with seasons and the Jellyfin next playable episode.
#[tauri::command]
#[specta]
pub async fn library_show_detail(
  state: State<'_, JellyfinState>,
  series_id: String,
) -> Result<VideoShowDetail, CommandError> {
  state
    .client
    .library()
    .show_detail(series_id)
    .await
    .map_err(jellyfin_err)
}

/// Load Episodes for one Show season.
#[tauri::command]
#[specta]
pub async fn library_season_episodes(
  state: State<'_, JellyfinState>,
  request: VideoSeasonEpisodesRequest,
) -> Result<VideoSeasonEpisodes, CommandError> {
  state
    .client
    .library()
    .season_episodes(request)
    .await
    .map_err(jellyfin_err)
}

/// Start explicit Library Browser playback through the active Jellyfin session.
#[tauri::command]
#[specta]
pub async fn library_play(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  request: VideoLibraryPlayRequest,
) -> Result<(), CommandError> {
  let session = state
    .session
    .read()
    .clone()
    .ok_or_else(|| CommandError::invalid_input("Library playback requires an active session"))?;

  session.play_library(request).await.map_err(jellyfin_err)?;
  playback_control::emit_now_playing_changed(&app, &state).await;

  Ok(())
}

/// Mutate Jellyfin user data for a Library Browser item.
#[tauri::command]
#[specta]
pub async fn library_update_user_data(
  state: State<'_, JellyfinState>,
  request: VideoUserDataUpdateRequest,
) -> Result<VideoUserDataUpdate, CommandError> {
  state
    .client
    .library()
    .update_user_data(request)
    .await
    .map_err(jellyfin_err)
}

/// Get the current session data for saving.
#[tauri::command]
#[specta]
pub fn jellyfin_get_session(state: State<'_, JellyfinState>) -> Option<SavedSession> {
  state.client.login().get_saved_session()
}

/// Restore a session from saved data.
#[tauri::command]
#[specta]
pub async fn jellyfin_restore_session(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  config_state: State<'_, ConfigState>,
  session: SavedSession,
) -> Result<(), CommandError> {
  // Restore connection from saved session
  state
    .client
    .login()
    .restore_session(&session)
    .await
    .map_err(jellyfin_err)?;

  start_remote_control_session_if_supported(&app, &state, &config_state).await
}

/// Clear/logout from the current session.
///
/// This disconnects from the server. Saved service profile removal is handled
/// by the profile-store commands.
#[tauri::command]
#[specta]
pub async fn jellyfin_clear_session(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  // Take session without holding lock across await
  let session = state.session.write().take();

  // Stop session if active
  if let Some(session) = session {
    session.stop().await.map_err(internal_err)?;
  }

  // Disconnect client (clears internal state)
  state.client.login().disconnect();

  log::info!("Session cleared");
  playback_control::emit_now_playing_changed(&app, &state).await;
  Ok(())
}

/// Play the next episode from the active Jellyfin session.
#[tauri::command]
#[specta]
pub async fn jellyfin_play_next_episode(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  playback_control::play_adjacent_episode(&app, &state, playback_control::AdjacentDirection::Next)
    .await
}

/// Play the previous episode from the active Jellyfin session.
#[tauri::command]
#[specta]
pub async fn jellyfin_play_previous_episode(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  playback_control::play_adjacent_episode(
    &app,
    &state,
    playback_control::AdjacentDirection::Previous,
  )
  .await
}

// ============================================================================
// Provider-neutral media server commands
// ============================================================================

/// Connect to the selected media server provider.
#[tauri::command]
#[specta]
pub async fn server_connect(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  config_state: State<'_, ConfigState>,
  credentials: Credentials,
) -> Result<(), CommandError> {
  jellyfin_connect(app, state, config_state, credentials).await
}

/// Disconnect from the active media server provider.
#[tauri::command]
#[specta]
pub async fn server_disconnect(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  jellyfin_disconnect(app, state).await
}

/// Get current media server connection state.
#[tauri::command]
#[specta]
pub fn server_get_state(state: State<'_, JellyfinState>) -> ConnectionState {
  jellyfin_get_state(state)
}

/// Check if a media server provider is connected.
#[tauri::command]
#[specta]
pub fn server_is_connected(state: State<'_, JellyfinState>) -> bool {
  jellyfin_is_connected(state)
}

/// Get the current media server session data for saving.
#[tauri::command]
#[specta]
pub fn server_get_session(state: State<'_, JellyfinState>) -> Option<SavedSession> {
  jellyfin_get_session(state)
}

/// Restore a media server session from saved data.
#[tauri::command]
#[specta]
pub async fn server_restore_session(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  config_state: State<'_, ConfigState>,
  session: SavedSession,
) -> Result<(), CommandError> {
  jellyfin_restore_session(app, state, config_state, session).await
}

/// Clear/logout from the current media server session.
#[tauri::command]
#[specta]
pub async fn server_clear_session(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
) -> Result<(), CommandError> {
  jellyfin_clear_session(app, state).await
}

/// List saved media server profiles.
#[tauri::command]
#[specta]
pub fn server_profiles_get(app: tauri::AppHandle) -> Result<SavedServiceProfiles, CommandError> {
  load_profiles(&app)
    .map(|profiles| profiles.summary())
    .map_err(internal_err)
}

/// Import a legacy single saved session into the saved service profile store.
#[tauri::command]
#[specta]
pub fn server_profiles_import_legacy(
  app: tauri::AppHandle,
  session: SavedSession,
) -> Result<SavedServiceProfiles, CommandError> {
  let mut profiles = load_profiles(&app).map_err(internal_err)?;
  profiles.upsert_active(session);
  save_profiles(&app, &profiles).map_err(internal_err)?;
  Ok(profiles.summary())
}

/// Save the currently authenticated session as the active saved service profile.
#[tauri::command]
#[specta]
pub fn server_profiles_save_current(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
) -> Result<SavedServiceProfiles, CommandError> {
  let session = state
    .client
    .login()
    .get_saved_session()
    .ok_or_else(|| CommandError::not_connected("No active media server session to save"))?;
  let mut profiles = load_profiles(&app).map_err(internal_err)?;
  profiles.upsert_active(session);
  save_profiles(&app, &profiles).map_err(internal_err)?;
  Ok(profiles.summary())
}

/// Activate a saved service profile and make it the only live media server connection.
#[tauri::command]
#[specta]
pub async fn server_profiles_activate(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  config_state: State<'_, ConfigState>,
  key: String,
) -> Result<SavedServiceProfiles, CommandError> {
  let mut profiles = load_profiles(&app).map_err(internal_err)?;
  let session = profiles
    .session_for_key(&key)
    .ok_or_else(|| CommandError::not_found("Saved service profile was not found"))?;

  stop_active_media_server_session(&app, &state).await?;

  let restore_result = state
    .client
    .login()
    .restore_session(&session)
    .await
    .map_err(jellyfin_err);

  if let Err(err) = restore_result {
    profiles.mark_restore_failed(&key, err.message.clone());
    save_profiles(&app, &profiles).map_err(internal_err)?;
    playback_control::emit_now_playing_changed(&app, &state).await;
    return Err(err);
  }

  if let Err(err) = start_remote_control_session_if_supported(&app, &state, &config_state).await {
    profiles.mark_restore_failed(&key, err.message.clone());
    save_profiles(&app, &profiles).map_err(internal_err)?;
    return Err(err);
  }

  profiles.mark_active_restored(&key);
  save_profiles(&app, &profiles).map_err(internal_err)?;
  Ok(profiles.summary())
}

/// Remove a saved service profile. Removing the active profile also disconnects it.
#[tauri::command]
#[specta]
pub async fn server_profiles_remove(
  app: tauri::AppHandle,
  state: State<'_, JellyfinState>,
  key: String,
) -> Result<SavedServiceProfiles, CommandError> {
  let mut profiles = load_profiles(&app).map_err(internal_err)?;
  if profiles.active_profile_key() == Some(key.as_str()) {
    stop_active_media_server_session(&app, &state).await?;
  }

  if !profiles.remove_profile(&key) {
    return Err(CommandError::not_found(
      "Saved service profile was not found",
    ));
  }

  save_profiles(&app, &profiles).map_err(internal_err)?;
  Ok(profiles.summary())
}

async fn stop_active_media_server_session(
  app: &tauri::AppHandle,
  state: &JellyfinState,
) -> Result<(), CommandError> {
  let session = state.session.write().take();
  if let Some(session) = session {
    session.stop().await.map_err(internal_err)?;
  }

  state.client.login().disconnect();
  playback_control::emit_now_playing_changed(app, state).await;
  Ok(())
}

// ============================================================================
// Config Commands
// ============================================================================

/// Config state managed by Tauri.
pub struct ConfigState(pub Arc<RwLock<AppConfig>>);

const CONFIG_STORE_FILE: &str = "config.json";
const CONFIG_STORE_KEY: &str = "app_config";

/// Get the current app configuration.
#[tauri::command]
#[specta]
pub fn config_get(state: State<'_, ConfigState>) -> AppConfig {
  state.0.read().clone()
}

/// Update the app configuration, apply changes live, and persist to disk.
#[tauri::command]
#[specta]
pub async fn config_set(
  app: tauri::AppHandle,
  state: State<'_, ConfigState>,
  mpv_state: State<'_, MpvState>,
  jellyfin_state: State<'_, JellyfinState>,
  config: AppConfig,
) -> Result<(), CommandError> {
  use std::path::PathBuf;
  use tauri_plugin_store::StoreExt;

  config.validate().map_err(CommandError::invalid_input)?;

  // Update in-memory state
  *state.0.write() = config.clone();

  // Apply MPV config changes (takes effect on next MPV spawn)
  let mpv_path = config
    .mpv_path
    .as_ref()
    .filter(|s| !s.is_empty())
    .map(PathBuf::from);
  mpv_state.0.set_mpv_path(mpv_path);
  mpv_state.0.set_extra_args(config.mpv_args.clone());
  log::info!("MPV config updated (applies on next spawn)");

  // Apply Jellyfin device name change if connected
  if jellyfin_state.client.login().is_connected() {
    jellyfin_state
      .client
      .set_device_name(config.device_name.clone());
    // Re-register capabilities with new device name
    if let Err(e) = jellyfin_state.client.playback().report_capabilities().await {
      log::warn!("Failed to re-register capabilities: {}", e);
    } else {
      log::info!("Jellyfin capabilities re-registered with new device name");
    }
  }

  // Update MPV keybindings file (blocking I/O, run in spawn_blocking)
  let keybind_next = config.keybind_next.clone();
  let keybind_prev = config.keybind_prev.clone();
  let keybind_intro_skip = config.keybind_intro_skip.clone();
  tauri::async_runtime::spawn_blocking(move || {
    write_input_conf(&keybind_next, &keybind_prev, &keybind_intro_skip);
  })
  .await
  .map_err(|e| CommandError::internal(format!("Failed to write input.conf: {}", e)))?;

  // Persist to disk
  let store = app.store(CONFIG_STORE_FILE).map_err(internal_err)?;
  store.set(
    CONFIG_STORE_KEY.to_string(),
    serde_json::to_value(&config).map_err(internal_err)?,
  );
  // Note: store.save() is synchronous but typically fast for small configs.
  // For larger data, consider spawn_blocking.
  store.save().map_err(internal_err)?;

  log::info!("Config saved to disk");
  Ok(())
}

/// Get the default configuration.
#[tauri::command]
#[specta]
pub fn config_default() -> AppConfig {
  AppConfig::default()
}

/// Detect MPV path automatically.
#[tauri::command]
#[specta]
pub fn config_detect_mpv() -> Option<String> {
  crate::mpv::find_mpv().map(|p| {
    let s = p.to_string_lossy().to_string();
    // Strip Windows extended-length path prefix for cleaner display
    s.strip_prefix(r"\\?\").map(String::from).unwrap_or(s)
  })
}

/// Load config from disk. Called internally during app setup.
pub fn load_config_from_store(app: &tauri::AppHandle) -> AppConfig {
  use tauri_plugin_store::StoreExt;

  match app.store(CONFIG_STORE_FILE) {
    Ok(store) => {
      if let Some(value) = store.get(CONFIG_STORE_KEY) {
        match serde_json::from_value::<AppConfig>(value.clone()) {
          Ok(config) => {
            log::info!("Config loaded from disk");
            return config;
          }
          Err(e) => {
            log::warn!("Failed to parse stored config, using defaults: {}", e);
          }
        }
      } else {
        log::info!("No stored config found, using defaults");
      }
    }
    Err(e) => {
      log::warn!("Failed to open config store, using defaults: {}", e);
    }
  }

  AppConfig::default()
}

pub fn specta_builder() -> Builder<tauri::Wry> {
  let builder = Builder::<tauri::Wry>::new()
    .commands(collect_commands![
      // MPV commands
      mpv_start,
      mpv_stop,
      mpv_loadfile,
      mpv_seek,
      mpv_set_pause,
      mpv_set_volume,
      mpv_toggle_mute,
      mpv_set_audio_track,
      mpv_set_subtitle_track,
      mpv_get_property,
      mpv_get_state,
      mpv_is_connected,
      now_playing_get_state,
      library_video_home,
      library_video_shortcuts,
      library_item_shortcut,
      library_browse_video,
      library_search_video,
      library_item_detail,
      library_show_detail,
      library_season_episodes,
      library_play,
      library_update_user_data,
      // Jellyfin commands
      jellyfin_connect,
      jellyfin_disconnect,
      jellyfin_get_state,
      jellyfin_is_connected,
      jellyfin_get_session,
      jellyfin_restore_session,
      jellyfin_clear_session,
      jellyfin_play_next_episode,
      jellyfin_play_previous_episode,
      jellyfin_quick_connect_start,
      jellyfin_quick_connect_check,
      jellyfin_quick_connect_authenticate,
      // Provider-neutral server commands
      server_connect,
      server_disconnect,
      server_get_state,
      server_is_connected,
      server_get_session,
      server_restore_session,
      server_clear_session,
      server_profiles_get,
      server_profiles_import_legacy,
      server_profiles_save_current,
      server_profiles_activate,
      server_profiles_remove,
      // Config commands
      config_get,
      config_set,
      config_default,
      config_detect_mpv,
    ])
    .events(collect_events![AppNotification, NowPlayingChanged]);

  #[cfg(debug_assertions)] // <- Only export on non-release builds
  {
    let mut bindings_path = std::env::current_dir().unwrap();
    bindings_path.push("../src/bindings.ts");

    builder
      .export(Typescript::default(), &bindings_path)
      .expect("Failed to export typescript bindings");
  }

  builder
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn jellyfin_err_maps_auth_failures_to_auth_failed_code() {
    let err = jellyfin_err(JellyfinError::AuthFailed("revoked".to_string()));

    assert!(matches!(err.code, CommandErrorCode::AuthFailed));
  }

  #[test]
  fn jellyfin_err_maps_invalid_urls_to_invalid_input_code() {
    let err = jellyfin_err(JellyfinError::InvalidUrl("bad url".to_string()));

    assert!(matches!(err.code, CommandErrorCode::InvalidInput));
    assert_eq!(err.message, "bad url");
  }

  #[test]
  fn jellyfin_err_maps_server_response_failures_to_network_code() {
    let err = jellyfin_err(JellyfinError::HttpError(
      "Unable to discover Emby API base URL".to_string(),
    ));

    assert!(matches!(err.code, CommandErrorCode::Network));
    assert!(err.message.contains("Unable to discover Emby API base URL"));
  }

  #[test]
  fn export_bindings() {
    // This test triggers binding generation
    let _ = specta_builder();
  }
}
