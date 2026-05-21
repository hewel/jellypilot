//! Jellyfin HTTP client for REST API calls.

use parking_lot::RwLock;
use reqwest::{header, Client};
use std::sync::Arc;
use uuid::Uuid;

use super::error::JellyfinError;
use super::intro_skipper::{
  parse_intro_skipper_ranges, IntroSkipRange, IntroSkipperPluginResponse,
};
use super::types::*;

/// Device info for Jellyfin client identification.
const DEFAULT_DEVICE_NAME: &str = "JMSR";
const DEVICE_ID_PREFIX: &str = "jmsr-";
const CLIENT_NAME: &str = "Jellyfin MPV Shim Rust";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Jellyfin HTTP API client.
pub struct JellyfinClient {
  http: Client,
  state: Arc<RwLock<ClientState>>,
}

/// Internal connection state.
struct ClientState {
  server_url: Option<String>,
  access_token: Option<String>,
  user_id: Option<String>,
  user_name: Option<String>,
  server_name: Option<String>,
  device_id: String,
  device_name: String,
}

impl JellyfinClient {
  /// Create a new Jellyfin client.
  pub fn new() -> Self {
    let device_id = format!("{}{}", DEVICE_ID_PREFIX, Uuid::new_v4());

    Self {
      http: Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client"),
      state: Arc::new(RwLock::new(ClientState {
        server_url: None,
        access_token: None,
        user_id: None,
        user_name: None,
        server_name: None,
        device_id,
        device_name: DEFAULT_DEVICE_NAME.to_string(),
      })),
    }
  }

  /// Set the device name (shown in Jellyfin cast menu).
  pub fn set_device_name(&self, name: String) {
    self.state.write().device_name = name;
  }

  /// Get the device ID.
  pub fn device_id(&self) -> String {
    self.state.read().device_id.clone()
  }

  /// Build authorization header value.
  fn auth_header(&self, token: Option<&str>) -> String {
    let state = self.state.read();
    let mut header = format!(
      r#"MediaBrowser Client="{}", Device="{}", DeviceId="{}", Version="{}""#,
      CLIENT_NAME, state.device_name, state.device_id, CLIENT_VERSION
    );
    if let Some(token) = token {
      header.push_str(&format!(r#", Token="{}""#, token));
    }
    header
  }

  /// Authenticate with Jellyfin server.
  pub async fn authenticate(&self, creds: &Credentials) -> Result<AuthResponse, JellyfinError> {
    let server_url = Self::normalize_server_url(&creds.server_url)?;
    let url = format!("{}/Users/AuthenticateByName", server_url);

    let body = serde_json::json!({
      "Username": creds.username,
      "Pw": creds.password
    });

    let response = self
      .http
      .post(&url)
      .header(header::CONTENT_TYPE, "application/json")
      .header("X-Emby-Authorization", self.auth_header(None))
      .json(&body)
      .send()
      .await?;

    if !response.status().is_success() {
      let status = response.status();
      let text = response.text().await.unwrap_or_default();
      return Err(JellyfinError::AuthFailed(format!(
        "HTTP {}: {}",
        status, text
      )));
    }

    let auth: AuthResponse = response.json().await?;

    // Store connection state
    {
      let mut state = self.state.write();
      state.server_url = Some(server_url);
      state.access_token = Some(auth.access_token.clone());
      state.user_id = Some(auth.user.id.clone());
      state.user_name = Some(auth.user.name.clone());
    }

    // Fetch server info
    self.fetch_server_info().await.ok();

    Ok(auth)
  }

  /// Start a Quick Connect request on a Jellyfin server.
  pub async fn quick_connect_start(
    &self,
    server_url: &str,
  ) -> Result<QuickConnectRequest, JellyfinError> {
    let server_url = Self::normalize_server_url(server_url)?;
    let url = format!("{}/QuickConnect/Initiate", server_url);

    let response = self
      .http
      .post(&url)
      .header("X-Emby-Authorization", self.auth_header(None))
      .send()
      .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
      return Err(JellyfinError::QuickConnectUnavailable);
    }

    if !response.status().is_success() {
      let status = response.status();
      let text = response.text().await.unwrap_or_default();
      return Err(JellyfinError::HttpError(format!(
        "HTTP {}: {}",
        status, text
      )));
    }

    let request: QuickConnectInitiateResponse = response.json().await?;
    Ok(QuickConnectRequest {
      code: request.code,
      secret: request.secret,
    })
  }

  /// Check whether a Quick Connect request has been approved.
  pub async fn quick_connect_check(
    &self,
    server_url: &str,
    secret: &str,
  ) -> Result<QuickConnectStatus, JellyfinError> {
    let server_url = Self::normalize_server_url(server_url)?;
    let url = format!("{}/QuickConnect/Connect", server_url);

    let response = self
      .http
      .get(&url)
      .query(&[("secret", secret)])
      .header("X-Emby-Authorization", self.auth_header(None))
      .send()
      .await?;

    if !response.status().is_success() {
      let status = response.status();
      let text = response.text().await.unwrap_or_default();
      return Err(JellyfinError::HttpError(format!(
        "HTTP {}: {}",
        status, text
      )));
    }

    let state: QuickConnectState = response.json().await?;
    if state.authenticated {
      Ok(QuickConnectStatus::Approved)
    } else {
      Ok(QuickConnectStatus::Waiting)
    }
  }

  /// Complete Quick Connect authentication after the request is approved.
  pub async fn quick_connect_authenticate(
    &self,
    server_url: &str,
    secret: &str,
  ) -> Result<AuthResponse, JellyfinError> {
    let server_url = Self::normalize_server_url(server_url)?;
    let url = format!("{}/Users/AuthenticateWithQuickConnect", server_url);
    let body = serde_json::json!({ "Secret": secret });

    let response = self
      .http
      .post(&url)
      .header(header::CONTENT_TYPE, "application/json")
      .header("X-Emby-Authorization", self.auth_header(None))
      .json(&body)
      .send()
      .await?;

    if !response.status().is_success() {
      let status = response.status();
      let text = response.text().await.unwrap_or_default();
      return Err(JellyfinError::AuthFailed(format!(
        "HTTP {}: {}",
        status, text
      )));
    }

    let auth: AuthResponse = response.json().await?;

    {
      let mut state = self.state.write();
      state.server_url = Some(server_url);
      state.access_token = Some(auth.access_token.clone());
      state.user_id = Some(auth.user.id.clone());
      state.user_name = Some(auth.user.name.clone());
    }

    self.fetch_server_info().await.ok();

    Ok(auth)
  }

  /// Fetch server public info.
  async fn fetch_server_info(&self) -> Result<ServerInfo, JellyfinError> {
    let server_url = self.server_url()?;
    let url = format!("{}/System/Info/Public", server_url);

    let response = self.http.get(&url).send().await?;
    let info: ServerInfo = response.json().await?;

    {
      let mut state = self.state.write();
      state.server_name = Some(info.server_name.clone());
    }

    Ok(info)
  }

  /// Disconnect from server.
  pub fn disconnect(&self) {
    let mut state = self.state.write();
    state.server_url = None;
    state.access_token = None;
    state.user_id = None;
    state.user_name = None;
    state.server_name = None;
  }

  /// Restore a session from saved data.
  ///
  /// Validates the token by making a test API call.
  pub async fn restore_session(&self, session: &SavedSession) -> Result<(), JellyfinError> {
    // Set the state first
    {
      let mut state = self.state.write();
      state.server_url = Some(session.server_url.clone());
      state.access_token = Some(session.access_token.clone());
      state.user_id = Some(session.user_id.clone());
      state.user_name = Some(session.user_name.clone());
      state.server_name = session.server_name.clone();
      // Restore device_id if present, otherwise keep the generated one
      if let Some(saved_device_id) = &session.device_id {
        state.device_id = saved_device_id.clone();
      }
    }

    // Validate the token by calling /System/Info/Public
    // If this fails, clear the state and return error
    match self.fetch_server_info().await {
      Ok(_) => Ok(()),
      Err(e) => {
        self.disconnect();
        Err(JellyfinError::AuthFailed(format!(
          "Session validation failed: {}",
          e
        )))
      }
    }
  }

  /// Get current session data for persistence.
  pub fn get_saved_session(&self) -> Option<SavedSession> {
    let state = self.state.read();
    if let (Some(server_url), Some(access_token), Some(user_id), Some(user_name)) = (
      state.server_url.clone(),
      state.access_token.clone(),
      state.user_id.clone(),
      state.user_name.clone(),
    ) {
      Some(SavedSession {
        server_url,
        access_token,
        user_id,
        user_name,
        server_name: state.server_name.clone(),
        device_id: Some(state.device_id.clone()),
      })
    } else {
      None
    }
  }

  /// Check if connected.
  pub fn is_connected(&self) -> bool {
    let state = self.state.read();
    state.access_token.is_some()
  }

  /// Get current connection state.
  pub fn connection_state(&self) -> ConnectionState {
    let state = self.state.read();
    ConnectionState {
      connected: state.access_token.is_some(),
      server_url: state.server_url.clone(),
      server_name: state.server_name.clone(),
      user_name: state.user_name.clone(),
    }
  }

  /// Get server URL or error if not connected.
  fn server_url(&self) -> Result<String, JellyfinError> {
    self
      .state
      .read()
      .server_url
      .clone()
      .ok_or(JellyfinError::NotConnected)
  }

  fn normalize_server_url(server_url: &str) -> Result<String, JellyfinError> {
    let server_url = server_url.trim_end_matches('/').to_string();
    if !server_url.starts_with("http://") && !server_url.starts_with("https://") {
      return Err(JellyfinError::InvalidUrl(
        "URL must start with http:// or https://".to_string(),
      ));
    }

    Ok(server_url)
  }

  /// Get access token or error if not connected.
  fn access_token(&self) -> Result<String, JellyfinError> {
    self
      .state
      .read()
      .access_token
      .clone()
      .ok_or(JellyfinError::NotConnected)
  }

  /// Get user ID or error if not connected.
  pub fn user_id(&self) -> Result<String, JellyfinError> {
    self
      .state
      .read()
      .user_id
      .clone()
      .ok_or(JellyfinError::NotConnected)
  }

  /// Make an authenticated GET request.
  pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, JellyfinError> {
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let url = format!("{}{}", server_url, path);

    let response = self
      .http
      .get(&url)
      .header("X-Emby-Authorization", self.auth_header(Some(&token)))
      .send()
      .await?;

    let status = response.status();
    if !status.is_success() {
      let body = response.text().await.unwrap_or_default();
      return Err(JellyfinError::HttpError(format!(
        "GET {} failed: HTTP {} - {}",
        path, status, body
      )));
    }

    Ok(response.json().await?)
  }

  /// Make an authenticated POST request.
  pub async fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    &self,
    path: &str,
    body: &B,
  ) -> Result<T, JellyfinError> {
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let url = format!("{}{}", server_url, path);

    let response = self
      .http
      .post(&url)
      .header(header::CONTENT_TYPE, "application/json")
      .header("X-Emby-Authorization", self.auth_header(Some(&token)))
      .json(body)
      .send()
      .await?;

    let status = response.status();
    if !status.is_success() {
      let body = response.text().await.unwrap_or_default();
      return Err(JellyfinError::HttpError(format!(
        "POST {} failed: HTTP {} - {}",
        path, status, body
      )));
    }

    Ok(response.json().await?)
  }

  /// Make an authenticated POST request without expecting a response body.
  pub async fn post_empty<B: serde::Serialize + std::fmt::Debug>(
    &self,
    path: &str,
    body: &B,
  ) -> Result<(), JellyfinError> {
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let url = format!("{}{}", server_url, path);

    log::debug!("POST {} with body: {:?}", path, body);

    let response = self
      .http
      .post(&url)
      .header(header::CONTENT_TYPE, "application/json")
      .header("X-Emby-Authorization", self.auth_header(Some(&token)))
      .json(body)
      .send()
      .await?;

    let status = response.status();
    if !status.is_success() {
      let body = response.text().await.unwrap_or_default();
      log::error!("POST {} failed with status {}: {}", path, status, body);
      return Err(JellyfinError::HttpError(format!(
        "HTTP {} - {}",
        status, body
      )));
    }

    Ok(())
  }

  /// Get media item by ID.
  pub async fn get_item(&self, item_id: &str) -> Result<MediaItem, JellyfinError> {
    let user_id = self.user_id()?;
    self
      .get(&format!("/Users/{}/Items/{}", user_id, item_id))
      .await
  }

  /// Get playback info for a media item.
  pub async fn get_playback_info(
    &self,
    item_id: &str,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
  ) -> Result<PlaybackInfoResponse, JellyfinError> {
    let user_id = self.user_id()?;
    let path = format!("/Items/{}/PlaybackInfo", item_id);

    let request = PlaybackInfoRequest {
      user_id,
      device_id: self.device_id(),
      max_streaming_bitrate: Some(140_000_000), // 140 Mbps
      start_time_ticks: None,
      audio_stream_index,
      subtitle_stream_index,
      enable_direct_play: true,
      enable_direct_stream: true,
      enable_transcoding: true,
      auto_open_live_stream: true,
    };

    self.post(&path, &request).await
  }

  /// Fetch active Intro Skipper plugin ranges for a media item.
  ///
  /// Missing, disabled, invalid, or failing plugin endpoints are treated as no
  /// ranges so playback can continue normally.
  pub async fn get_intro_skipper_ranges(
    &self,
    item_id: &str,
  ) -> Result<Vec<IntroSkipRange>, JellyfinError> {
    let path = format!("/Episode/{}/IntroSkipperSegments", item_id);
    let response = self.get::<IntroSkipperPluginResponse>(&path).await?;

    Ok(parse_intro_skipper_ranges(response))
  }

  /// Build the direct play URL for a media source.
  /// Always uses HTTP streaming URL - even for "File" protocol sources,
  /// since the file path is on the server, not accessible locally.
  pub fn build_stream_url(&self, item_id: &str, media_source: &MediaSource) -> Option<String> {
    let state = self.state.read();
    let server_url = state.server_url.as_ref()?;
    let token = state.access_token.as_ref()?;

    // Build streaming URL - always use HTTP, never raw file paths
    // The file path in media_source.path is on the server, not locally accessible
    let container = media_source.container.as_deref().unwrap_or("mkv");
    Some(format!(
      "{}/Videos/{}/stream.{}?Static=true&MediaSourceId={}&api_key={}",
      server_url, item_id, container, media_source.id, token
    ))
  }

  /// Build external subtitle URL with correct format extension.
  ///
  /// Uses the subtitle's codec to determine the file extension (ass, ssa, srt, vtt).
  /// This prevents Jellyfin from attempting to transcode the subtitle, which can fail
  /// for formats like ASS/SSA when requesting as SRT.
  ///
  /// MPV natively supports all these formats, so we should always request the original.
  pub fn build_subtitle_url(
    &self,
    item_id: &str,
    media_source_id: &str,
    stream: &MediaStream,
  ) -> Option<String> {
    let state = self.state.read();
    let server_url = state.server_url.as_ref()?;
    let token = state.access_token.as_ref()?;

    // Normalize codec to lowercase for case-insensitive matching.
    // Jellyfin can report codecs in various cases (e.g., "PGSSUB", "ass", "subrip").
    let codec = stream.codec.as_deref().unwrap_or("").to_ascii_lowercase();

    // Map codec to file extension (prevents transcoding)
    let ext = match codec.as_str() {
      "ass" => "ass",
      "ssa" => "ssa",
      "subrip" | "srt" => "srt",
      "webvtt" | "vtt" => "vtt",
      // Bitmap subtitle formats
      "pgs" | "pgssub" | "hdmv_pgs_subtitle" => "sup",
      "dvdsub" | "dvd_subtitle" | "vobsub" => "sub",
      "dvbsub" | "dvb_subtitle" => "sub",
      // Other text formats
      "mov_text" => "srt", // MP4 timed text - request as SRT
      "ttml" => "ttml",
      _ => "srt", // fallback for unknown codecs
    };

    // Jellyfin subtitle endpoint format:
    // /Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.{format}
    Some(format!(
      "{}/Videos/{}/{}/Subtitles/{}/Stream.{}?api_key={}",
      server_url, item_id, media_source_id, stream.index, ext, token
    ))
  }

  /// Get WebSocket URL for session.
  pub fn websocket_url(&self) -> Result<String, JellyfinError> {
    let state = self.state.read();
    let server_url = state
      .server_url
      .as_ref()
      .ok_or(JellyfinError::NotConnected)?;
    let token = state
      .access_token
      .as_ref()
      .ok_or(JellyfinError::NotConnected)?;

    // Convert http(s) to ws(s)
    let ws_url = if server_url.starts_with("https://") {
      server_url.replace("https://", "wss://")
    } else {
      server_url.replace("http://", "ws://")
    };

    Ok(format!(
      "{}/socket?api_key={}&deviceId={}",
      ws_url, token, state.device_id
    ))
  }

  /// Report playback started.
  pub async fn report_playback_start(&self, info: &PlaybackStartInfo) -> Result<(), JellyfinError> {
    self.post_empty("/Sessions/Playing", info).await
  }

  /// Report playback progress.
  pub async fn report_playback_progress(
    &self,
    info: &PlaybackProgressInfo,
  ) -> Result<(), JellyfinError> {
    self.post_empty("/Sessions/Playing/Progress", info).await
  }

  /// Report playback stopped.
  pub async fn report_playback_stop(&self, info: &PlaybackStopInfo) -> Result<(), JellyfinError> {
    self.post_empty("/Sessions/Playing/Stopped", info).await
  }

  /// Report session capabilities to Jellyfin via HTTP.
  ///
  /// This makes the client appear as a controllable cast target.
  pub async fn report_capabilities(&self) -> Result<(), JellyfinError> {
    let capabilities = serde_json::json!({
      "PlayableMediaTypes": ["Video", "Audio"],
      "SupportedCommands": [
        "MoveUp", "MoveDown", "MoveLeft", "MoveRight", "Select",
        "Back", "ToggleFullscreen", "GoHome", "GoToSettings",
        "VolumeUp", "VolumeDown", "ToggleMute", "Mute", "Unmute", "SetVolume",
        "SetAudioStreamIndex", "SetSubtitleStreamIndex",
        "DisplayContent", "Play", "Playstate", "PlayNext", "PlayMediaSource"
      ],
      "SupportsMediaControl": true,
      "SupportsPersistentIdentifier": true,
    });

    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let url = format!("{}/Sessions/Capabilities/Full", server_url);

    let response = self
      .http
      .post(&url)
      .header(reqwest::header::CONTENT_TYPE, "application/json")
      .header("X-Emby-Authorization", self.auth_header(Some(&token)))
      .json(&capabilities)
      .send()
      .await?;

    log::info!("Capabilities POST response status: {}", response.status());
    if !response.status().is_success() {
      let status = response.status();
      let text = response.text().await.unwrap_or_default();
      log::error!("Capabilities POST failed: HTTP {} - {}", status, text);
    }

    Ok(())
  }

  /// Get the next episode in a series after the given episode.
  ///
  /// Uses the /Shows/{seriesId}/Episodes endpoint with StartItemId to get adjacent episodes.
  /// Returns None if there's no next episode or if the item is not an episode.
  pub async fn get_next_episode(
    &self,
    current_item: &MediaItem,
  ) -> Result<Option<MediaItem>, JellyfinError> {
    // Only works for episodes
    if current_item.item_type != "Episode" {
      log::debug!("get_next_episode: not an episode, skipping");
      return Ok(None);
    }

    let series_id = match &current_item.series_id {
      Some(id) => id,
      None => {
        log::debug!("get_next_episode: no series_id, skipping");
        return Ok(None);
      }
    };

    let user_id = self.user_id()?;

    // Get episodes starting from current, limit 2 (current + next)
    let path = format!(
      "/Shows/{}/Episodes?UserId={}&StartItemId={}&Limit=2&Fields=MediaSources,MediaStreams",
      series_id, user_id, current_item.id
    );

    let response: EpisodesResponse = self.get(&path).await?;

    // The response includes the current episode and the next one (if exists)
    // We want the second item (index 1) which is the next episode
    if response.items.len() >= 2 {
      let next_ep = response.items.into_iter().nth(1);
      if let Some(ref ep) = next_ep {
        log::info!(
          "Found next episode: {} - S{:02}E{:02} - {}",
          ep.series_name.as_deref().unwrap_or("Unknown"),
          ep.parent_index_number.unwrap_or(0),
          ep.index_number.unwrap_or(0),
          ep.name
        );
      }
      Ok(next_ep)
    } else {
      log::info!("No next episode available (end of series or season)");
      Ok(None)
    }
  }

  /// Get the previous episode in a series before the given episode.
  ///
  /// Uses the /Shows/{seriesId}/Episodes endpoint to find adjacent episodes.
  /// Returns None if there's no previous episode or if the item is not an episode.
  pub async fn get_previous_episode(
    &self,
    current_item: &MediaItem,
  ) -> Result<Option<MediaItem>, JellyfinError> {
    // Only works for episodes
    if current_item.item_type != "Episode" {
      log::debug!("get_previous_episode: not an episode, skipping");
      return Ok(None);
    }

    let series_id = match &current_item.series_id {
      Some(id) => id,
      None => {
        log::debug!("get_previous_episode: no series_id, skipping");
        return Ok(None);
      }
    };

    let user_id = self.user_id()?;

    // Get all episodes for the series to find the previous one
    // We need to fetch episodes and find the one before current
    let path = format!(
      "/Shows/{}/Episodes?UserId={}&Fields=MediaSources,MediaStreams",
      series_id, user_id
    );

    let response: EpisodesResponse = self.get(&path).await?;

    // Find the current episode index and return the previous one
    let mut prev_ep: Option<MediaItem> = None;
    for ep in response.items {
      if ep.id == current_item.id {
        // Found current, return the previous one (if any)
        if let Some(ref prev) = prev_ep {
          log::info!(
            "Found previous episode: {} - S{:02}E{:02} - {}",
            prev.series_name.as_deref().unwrap_or("Unknown"),
            prev.parent_index_number.unwrap_or(0),
            prev.index_number.unwrap_or(0),
            prev.name
          );
        }
        return Ok(prev_ep);
      }
      prev_ep = Some(ep);
    }

    log::info!("No previous episode available (start of series)");
    Ok(None)
  }

  /// Validate that our session appears in the Jellyfin session list.
  /// This checks if we're visible as a cast target.
  pub async fn validate_session(&self) -> Result<(), JellyfinError> {
    let device_id = self.device_id();
    let server_url = self.server_url()?;
    let token = self.access_token()?;

    // Query all sessions
    let url = format!("{}/Sessions", server_url);
    let response = self
      .http
      .get(&url)
      .header("X-Emby-Authorization", self.auth_header(Some(&token)))
      .send()
      .await?;

    let sessions: Vec<serde_json::Value> = response.json().await?;

    // Look for our device in the session list
    for session in &sessions {
      if let Some(session_device_id) = session.get("DeviceId").and_then(|v| v.as_str()) {
        if session_device_id == device_id {
          // Found our session! Check if it supports media control
          let supports_media_control = session
            .get("SupportsMediaControl")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
          let supports_remote_control = session
            .get("SupportsRemoteControl")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

          log::info!(
            "Found our session: DeviceId={}, SupportsMediaControl={}, SupportsRemoteControl={}",
            device_id,
            supports_media_control,
            supports_remote_control
          );

          // Log full session for debugging
          log::debug!(
            "Session details: {}",
            serde_json::to_string_pretty(session).unwrap_or_default()
          );

          if supports_media_control {
            return Ok(());
          } else {
            return Err(JellyfinError::SessionNotFound);
          }
        }
      }
    }

    // Log all sessions for debugging
    log::warn!(
      "Our session not found in session list. Our DeviceId={}, Total sessions={}",
      device_id,
      sessions.len()
    );
    for (i, session) in sessions.iter().enumerate() {
      let sess_device_id = session
        .get("DeviceId")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
      let sess_device_name = session
        .get("DeviceName")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
      let sess_client = session
        .get("Client")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
      let supports_media = session
        .get("SupportsMediaControl")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
      log::info!(
        "Session[{}]: DeviceId={}, DeviceName={}, Client={}, SupportsMediaControl={}",
        i,
        sess_device_id,
        sess_device_name,
        sess_client,
        supports_media
      );
    }
    Err(JellyfinError::SessionNotFound)
  }
}

impl Default for JellyfinClient {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;

  async fn serve_once(status: &'static str, response_body: &'static str) -> String {
    serve_responses(vec![(status, response_body)]).await
  }

  async fn serve_responses(responses: Vec<(&'static str, &'static str)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
      .await
      .expect("test server should bind");
    let addr = listener.local_addr().expect("test server should have addr");

    tokio::spawn(async move {
      for (status, response_body) in responses {
        let (mut stream, _) = listener.accept().await.expect("test server should accept");
        let mut buffer = [0; 1024];
        let _ = stream
          .read(&mut buffer)
          .await
          .expect("test server should read request");
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

    format!("http://{}", addr)
  }

  #[tokio::test]
  async fn quick_connect_start_returns_code_and_secret_from_server() {
    let server_url = serve_once("200 OK", r#"{"Code":"ABCD12","Secret":"secret-123"}"#).await;
    let client = JellyfinClient::new();

    let request = client
      .quick_connect_start(&server_url)
      .await
      .expect("quick connect request should start");

    assert_eq!(request.code, "ABCD12");
    assert_eq!(request.secret, "secret-123");
  }

  #[tokio::test]
  async fn quick_connect_start_returns_unavailable_when_server_rejects_request() {
    let server_url = serve_once(
      "401 Unauthorized",
      r#"{"Message":"Quick Connect is disabled"}"#,
    )
    .await;
    let client = JellyfinClient::new();

    let err = client
      .quick_connect_start(&server_url)
      .await
      .expect_err("quick connect should report unavailable");

    assert!(
      matches!(err, JellyfinError::QuickConnectUnavailable),
      "expected quick connect unavailable, got {err:?}"
    );
  }

  #[tokio::test]
  async fn quick_connect_check_returns_approved_when_server_authenticated_request() {
    let server_url = serve_once(
      "200 OK",
      r#"{"Authenticated":true,"Code":"ABCD12","Secret":"secret-123"}"#,
    )
    .await;
    let client = JellyfinClient::new();

    let status = client
      .quick_connect_check(&server_url, "secret-123")
      .await
      .expect("quick connect state should load");

    assert!(matches!(status, QuickConnectStatus::Approved));
  }

  #[tokio::test]
  async fn quick_connect_authenticate_creates_saved_session() {
    let server_url = serve_responses(vec![
      (
        "200 OK",
        r#"{"User":{"Id":"user-1","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#,
      ),
      (
        "200 OK",
        r#"{"ServerName":"Jellyfin Home","Version":"10.10.0","Id":"server-1"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    client
      .quick_connect_authenticate(&server_url, "secret-123")
      .await
      .expect("quick connect authentication should succeed");

    let session = client
      .get_saved_session()
      .expect("quick connect should create saved session");

    assert_eq!(session.access_token, "token-1");
  }

  fn connect_test_client(client: &JellyfinClient, server_url: String) {
    let mut state = client.state.write();
    state.server_url = Some(server_url);
    state.access_token = Some("token-1".to_string());
    state.user_id = Some("user-1".to_string());
  }

  #[tokio::test]
  async fn intro_skipper_ranges_parse_valid_introduction_response() {
    let server_url = serve_once(
      "200 OK",
      r#"{"Introduction":{"EpisodeId":"00000000-0000-0000-0000-000000000001","Start":8.5,"End":68.25}}"#,
    )
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url);

    let ranges = client
      .get_intro_skipper_ranges("item-1")
      .await
      .expect("intro skipper response should parse");

    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start_seconds, 8.5);
    assert_eq!(ranges[0].end_seconds, 68.25);
  }

  #[tokio::test]
  async fn intro_skipper_ranges_return_empty_for_unsupported_or_invalid_segments() {
    let server_url = serve_once(
      "200 OK",
      r#"{"Recap":{"Start":1200.0,"End":1260.0},"Preview":{"Start":1.0,"End":20.0},"Commercial":{"Start":30.0,"End":45.0},"Introduction":{"Start":90.0,"End":80.0}}"#,
    )
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url);

    let ranges = client
      .get_intro_skipper_ranges("item-1")
      .await
      .expect("unsupported segments should be ignored");

    assert!(ranges.is_empty());
  }

  #[tokio::test]
  async fn intro_skipper_ranges_return_empty_for_empty_plugin_response() {
    let server_url = serve_once("200 OK", r#"{}"#).await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url);

    let ranges = client
      .get_intro_skipper_ranges("item-1")
      .await
      .expect("empty plugin response should parse");

    assert!(ranges.is_empty());
  }

  #[tokio::test]
  async fn intro_skipper_ranges_report_endpoint_failure_to_caller() {
    let server_url = serve_once("404 Not Found", r#"{"Message":"missing plugin"}"#).await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url);

    let err = client
      .get_intro_skipper_ranges("item-1")
      .await
      .expect_err("missing plugin endpoint should be an HTTP error");

    assert!(
      matches!(err, JellyfinError::HttpError(_)),
      "expected HTTP error for missing endpoint, got {err:?}"
    );
  }
}
