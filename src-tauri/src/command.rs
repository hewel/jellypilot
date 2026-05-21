use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::specta;
#[cfg(debug_assertions)]
use specta_typescript::Typescript;
use std::sync::Arc;
use tauri::State;
use tauri_specta::{collect_commands, collect_events, Builder, Event};

use crate::config::AppConfig;
use crate::jellyfin::{
  ConnectionState, Credentials, JellyfinClient, JellyfinError, QuickConnectRequest,
  QuickConnectStatus, SavedSession, SessionManager,
};
use crate::mpv::{write_input_conf, MpvClient, PropertyValue};

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

// ============================================================================
// Types
// ============================================================================

/// Player state returned to frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct PlayerState {
  pub connected: bool,
  pub paused: bool,
  pub time_pos: f64,
  pub duration: f64,
  pub volume: f64,
}

impl Default for PlayerState {
  fn default() -> Self {
    Self {
      connected: false,
      paused: true,
      time_pos: 0.0,
      duration: 0.0,
      volume: 100.0,
    }
  }
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
pub async fn mpv_start(state: State<'_, MpvState>) -> Result<(), CommandError> {
  state.0.start().await.map_err(internal_err)
}

/// Stop the MPV player.
#[tauri::command]
#[specta]
pub async fn mpv_stop(state: State<'_, MpvState>) -> Result<(), CommandError> {
  state.0.stop().await;
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
pub async fn mpv_seek(state: State<'_, MpvState>, time: f64) -> Result<(), CommandError> {
  if time < 0.0 {
    return Err(CommandError::invalid_input("Seek time cannot be negative"));
  }
  state.0.seek(time).await.map_err(internal_err)
}

/// Set pause state.
#[tauri::command]
#[specta]
pub async fn mpv_set_pause(state: State<'_, MpvState>, paused: bool) -> Result<(), CommandError> {
  state.0.set_pause(paused).await.map_err(internal_err)
}

/// Set volume (0-100).
#[tauri::command]
#[specta]
pub async fn mpv_set_volume(state: State<'_, MpvState>, volume: f64) -> Result<(), CommandError> {
  if !(0.0..=100.0).contains(&volume) {
    return Err(CommandError::invalid_input(
      "Volume must be between 0 and 100",
    ));
  }
  state.0.set_volume(volume).await.map_err(internal_err)
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

/// Set subtitle track by ID.
#[tauri::command]
#[specta]
pub async fn mpv_set_subtitle_track(
  state: State<'_, MpvState>,
  id: i32,
) -> Result<(), CommandError> {
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

/// Get current player state.
#[tauri::command]
#[specta]
pub async fn mpv_get_state(state: State<'_, MpvState>) -> Result<PlayerState, CommandError> {
  if !state.0.is_connected() {
    return Ok(PlayerState::default());
  }

  // Fetch all properties in parallel for better performance
  let (paused_res, time_pos_res, duration_res, volume_res) = tokio::join!(
    state.0.get_property("pause"),
    state.0.get_property("time-pos"),
    state.0.get_property("duration"),
    state.0.get_property("volume"),
  );

  let paused = match paused_res {
    Ok(PropertyValue::Bool(b)) => b,
    Ok(_) => true,
    Err(e) => {
      log::warn!("Failed to get pause property: {}", e);
      true
    }
  };

  let time_pos = match time_pos_res {
    Ok(PropertyValue::Number(n)) => n,
    Ok(_) => 0.0,
    Err(e) => {
      log::warn!("Failed to get time-pos property: {}", e);
      0.0
    }
  };

  let duration = match duration_res {
    Ok(PropertyValue::Number(n)) => n,
    Ok(_) => 0.0,
    Err(e) => {
      log::warn!("Failed to get duration property: {}", e);
      0.0
    }
  };

  let volume = match volume_res {
    Ok(PropertyValue::Number(n)) => n,
    Ok(_) => 100.0,
    Err(e) => {
      log::warn!("Failed to get volume property: {}", e);
      100.0
    }
  };

  Ok(PlayerState {
    connected: true,
    paused,
    time_pos,
    duration,
    volume,
  })
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
    .authenticate(&credentials)
    .await
    .map_err(|e| CommandError::auth_failed(e.to_string()))?;

  // Create and start session manager
  let new_session = Arc::new(SessionManager::new(
    state.client.clone(),
    state.mpv.clone(),
    config_state.0.clone(),
    app,
  ));
  new_session.start().await.map_err(internal_err)?;

  // Stop existing session before replacing (idempotent connect)
  let old_session = state.session.write().replace(new_session);
  if let Some(old) = old_session {
    if let Err(e) = old.stop().await {
      log::warn!("Failed to stop old session: {}", e);
    }
  }

  Ok(())
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
    .quick_connect_authenticate(&server_url, &secret)
    .await
    .map_err(jellyfin_err)?;

  // Create and start session manager
  let new_session = Arc::new(SessionManager::new(
    state.client.clone(),
    state.mpv.clone(),
    config_state.0.clone(),
    app,
  ));
  new_session.start().await.map_err(internal_err)?;

  // Stop existing session before replacing (idempotent connect)
  let old_session = state.session.write().replace(new_session);
  if let Some(old) = old_session {
    if let Err(e) = old.stop().await {
      log::warn!("Failed to stop old session: {}", e);
    }
  }

  Ok(())
}

/// Disconnect from Jellyfin server.
#[tauri::command]
#[specta]
pub async fn jellyfin_disconnect(state: State<'_, JellyfinState>) -> Result<(), CommandError> {
  // Take session without holding lock across await
  let session = state.session.write().take();

  // Stop session if active
  if let Some(session) = session {
    session.stop().await.map_err(internal_err)?;
  }

  // Disconnect client
  state.client.disconnect();

  Ok(())
}

/// Get Jellyfin connection state.
#[tauri::command]
#[specta]
pub fn jellyfin_get_state(state: State<'_, JellyfinState>) -> ConnectionState {
  state.client.connection_state()
}

/// Check if connected to Jellyfin.
#[tauri::command]
#[specta]
pub fn jellyfin_is_connected(state: State<'_, JellyfinState>) -> bool {
  state.client.is_connected()
}

/// Get the current session data for saving.
#[tauri::command]
#[specta]
pub fn jellyfin_get_session(state: State<'_, JellyfinState>) -> Option<SavedSession> {
  state.client.get_saved_session()
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
    .restore_session(&session)
    .await
    .map_err(|e| CommandError::network(e.to_string()))?;

  // Create and start session manager
  let new_session = Arc::new(SessionManager::new(
    state.client.clone(),
    state.mpv.clone(),
    config_state.0.clone(),
    app,
  ));
  new_session.start().await.map_err(internal_err)?;

  // Stop existing session before replacing (idempotent restore)
  let old_session = state.session.write().replace(new_session);
  if let Some(old) = old_session {
    if let Err(e) = old.stop().await {
      log::warn!("Failed to stop old session: {}", e);
    }
  }

  Ok(())
}

/// Clear/logout from the current session.
///
/// This disconnects from the server and should be paired with
/// clearing the saved session from localStorage on the frontend.
#[tauri::command]
#[specta]
pub async fn jellyfin_clear_session(state: State<'_, JellyfinState>) -> Result<(), CommandError> {
  // Take session without holding lock across await
  let session = state.session.write().take();

  // Stop session if active
  if let Some(session) = session {
    session.stop().await.map_err(internal_err)?;
  }

  // Disconnect client (clears internal state)
  state.client.disconnect();

  log::info!("Session cleared");
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
  if jellyfin_state.client.is_connected() {
    jellyfin_state
      .client
      .set_device_name(config.device_name.clone());
    // Re-register capabilities with new device name
    if let Err(e) = jellyfin_state.client.report_capabilities().await {
      log::warn!("Failed to re-register capabilities: {}", e);
    } else {
      log::info!("Jellyfin capabilities re-registered with new device name");
    }
  }

  // Update MPV keybindings file (blocking I/O, run in spawn_blocking)
  let keybind_next = config.keybind_next.clone();
  let keybind_prev = config.keybind_prev.clone();
  tauri::async_runtime::spawn_blocking(move || {
    write_input_conf(&keybind_next, &keybind_prev);
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

pub fn specta_builder() -> Builder {
  let builder = Builder::<tauri::Wry>::new()
    .commands(collect_commands![
      // MPV commands
      mpv_start,
      mpv_stop,
      mpv_loadfile,
      mpv_seek,
      mpv_set_pause,
      mpv_set_volume,
      mpv_set_audio_track,
      mpv_set_subtitle_track,
      mpv_get_property,
      mpv_get_state,
      mpv_is_connected,
      // Jellyfin commands
      jellyfin_connect,
      jellyfin_disconnect,
      jellyfin_get_state,
      jellyfin_is_connected,
      jellyfin_get_session,
      jellyfin_restore_session,
      jellyfin_clear_session,
      jellyfin_quick_connect_start,
      jellyfin_quick_connect_check,
      jellyfin_quick_connect_authenticate,
      // Config commands
      config_get,
      config_set,
      config_default,
      config_detect_mpv,
    ])
    .events(collect_events![AppNotification]);

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
  fn export_bindings() {
    // This test triggers binding generation
    let _ = specta_builder();
  }
}
