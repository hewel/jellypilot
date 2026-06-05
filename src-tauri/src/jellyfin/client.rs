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
/// Login/session lifecycle interface for the Jellyfin HTTP adapter.
pub struct JellyfinLogin<'a> {
  client: &'a JellyfinClient,
}

/// Playback/media interface for the Jellyfin HTTP adapter.
pub struct JellyfinPlayback<'a> {
  client: &'a JellyfinClient,
}

/// Library Browser interface for Jellyfin video browsing data.
pub struct JellyfinLibrary<'a> {
  client: &'a JellyfinClient,
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
  /// Login/session lifecycle operations.
  pub fn login(&self) -> JellyfinLogin<'_> {
    JellyfinLogin { client: self }
  }

  /// Playback/media operations used by the playback target session.
  pub fn playback(&self) -> JellyfinPlayback<'_> {
    JellyfinPlayback { client: self }
  }

  /// Library Browser operations used by the authenticated shell.
  pub fn library(&self) -> JellyfinLibrary<'_> {
    JellyfinLibrary { client: self }
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

  fn openapi_configuration(
    &self,
    server_url: &str,
    token: Option<&str>,
  ) -> Result<jellyfin_api::apis::configuration::Configuration, JellyfinError> {
    let mut headers = header::HeaderMap::new();
    let auth_header = header::HeaderValue::from_str(&self.auth_header(token)).map_err(|err| {
      JellyfinError::HttpError(format!("Invalid Jellyfin authorization header: {err}"))
    })?;
    headers.insert("X-Emby-Authorization", auth_header);

    let mut configuration = jellyfin_api::apis::configuration::Configuration::new();
    configuration.base_path = server_url.to_string();
    configuration.user_agent = Some(format!("{CLIENT_NAME}/{CLIENT_VERSION}"));
    configuration.client = Client::builder()
      .timeout(std::time::Duration::from_secs(30))
      .default_headers(headers)
      .build()?;

    Ok(configuration)
  }

  fn openapi_error<T: std::fmt::Debug>(
    context: &str,
    err: jellyfin_api::apis::Error<T>,
  ) -> JellyfinError {
    match err {
      jellyfin_api::apis::Error::Reqwest(err) => JellyfinError::Http(err),
      jellyfin_api::apis::Error::Serde(err) => JellyfinError::Json(err),
      jellyfin_api::apis::Error::Io(err) => {
        JellyfinError::HttpError(format!("{context} failed: {err}"))
      }
      jellyfin_api::apis::Error::ResponseError(response) => JellyfinError::HttpError(format!(
        "{context} failed: HTTP {} - {}",
        response.status, response.content
      )),
    }
  }

  fn openapi_auth_error<T: std::fmt::Debug>(
    context: &str,
    err: jellyfin_api::apis::Error<T>,
  ) -> JellyfinError {
    match Self::openapi_error(context, err) {
      JellyfinError::HttpError(message) => JellyfinError::AuthFailed(message),
      err => err,
    }
  }

  fn missing_openapi_field(context: &str, field: &str) -> JellyfinError {
    JellyfinError::HttpError(format!("{context} response missing {field}"))
  }

  fn auth_response_from_openapi(
    auth: jellyfin_api::models::AuthenticationResult,
  ) -> Result<AuthResponse, JellyfinError> {
    let user = auth
      .user
      .flatten()
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "User"))?;
    let id = user
      .id
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "User.Id"))?;
    let name = user
      .name
      .flatten()
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "User.Name"))?;
    let access_token = auth
      .access_token
      .flatten()
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "AccessToken"))?;
    let server_id = auth
      .server_id
      .flatten()
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "ServerId"))?;

    Ok(AuthResponse {
      user: User {
        id: id.to_string(),
        name,
      },
      access_token,
      server_id,
    })
  }

  fn server_info_from_openapi(
    info: jellyfin_api::models::PublicSystemInfo,
  ) -> Result<ServerInfo, JellyfinError> {
    let server_name = info
      .server_name
      .flatten()
      .ok_or_else(|| Self::missing_openapi_field("System public info", "ServerName"))?;
    let version = info
      .version
      .flatten()
      .ok_or_else(|| Self::missing_openapi_field("System public info", "Version"))?;
    let id = info
      .id
      .flatten()
      .ok_or_else(|| Self::missing_openapi_field("System public info", "Id"))?;

    Ok(ServerInfo {
      server_name,
      version,
      id,
    })
  }

  /// Authenticate with Jellyfin server.
  pub async fn authenticate(&self, creds: &Credentials) -> Result<AuthResponse, JellyfinError> {
    let server_url = Self::normalize_server_url(&creds.server_url)?;
    let configuration = self.openapi_configuration(&server_url, None)?;

    let auth = jellyfin_api::apis::user_api::authenticate_user_by_name(
      &configuration,
      jellyfin_api::apis::user_api::AuthenticateUserByNameParams {
        authenticate_user_by_name: jellyfin_api::models::AuthenticateUserByName {
          username: Some(Some(creds.username.clone())),
          pw: Some(Some(creds.password.clone())),
        },
      },
    )
    .await
    .map_err(|err| Self::openapi_auth_error("Password authentication", err))
    .and_then(Self::auth_response_from_openapi)?;

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
    let configuration = self.openapi_configuration(&server_url, None)?;

    let request = jellyfin_api::apis::quick_connect_api::initiate_quick_connect(&configuration)
      .await
      .map_err(|err| match err {
        jellyfin_api::apis::Error::ResponseError(response)
          if response.status == reqwest::StatusCode::UNAUTHORIZED =>
        {
          JellyfinError::QuickConnectUnavailable
        }
        err => Self::openapi_error("Quick Connect initiation", err),
      })?;

    Ok(QuickConnectRequest {
      code: request
        .code
        .ok_or_else(|| Self::missing_openapi_field("Quick Connect initiation", "Code"))?,
      secret: request
        .secret
        .ok_or_else(|| Self::missing_openapi_field("Quick Connect initiation", "Secret"))?,
    })
  }

  /// Check whether a Quick Connect request has been approved.
  pub async fn quick_connect_check(
    &self,
    server_url: &str,
    secret: &str,
  ) -> Result<QuickConnectStatus, JellyfinError> {
    let server_url = Self::normalize_server_url(server_url)?;
    let configuration = self.openapi_configuration(&server_url, None)?;

    let state = jellyfin_api::apis::quick_connect_api::get_quick_connect_state(
      &configuration,
      jellyfin_api::apis::quick_connect_api::GetQuickConnectStateParams {
        secret: secret.to_string(),
      },
    )
    .await
    .map_err(|err| Self::openapi_error("Quick Connect status", err))?;

    if state.authenticated.unwrap_or(false) {
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
    let configuration = self.openapi_configuration(&server_url, None)?;

    let auth = jellyfin_api::apis::user_api::authenticate_with_quick_connect(
      &configuration,
      jellyfin_api::apis::user_api::AuthenticateWithQuickConnectParams {
        quick_connect_dto: jellyfin_api::models::QuickConnectDto {
          secret: secret.to_string(),
        },
      },
    )
    .await
    .map_err(|err| Self::openapi_auth_error("Quick Connect authentication", err))
    .and_then(Self::auth_response_from_openapi)?;

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
    let configuration = self.openapi_configuration(&server_url, None)?;

    let info = jellyfin_api::apis::system_api::get_public_system_info(&configuration)
      .await
      .map_err(|err| Self::openapi_error("System public info", err))
      .and_then(Self::server_info_from_openapi)?;

    {
      let mut state = self.state.write();
      state.server_name = Some(info.server_name.clone());
    }

    Ok(info)
  }

  async fn validate_saved_token(&self) -> Result<(), JellyfinError> {
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let configuration = self.openapi_configuration(&server_url, Some(&token))?;

    jellyfin_api::apis::user_api::get_current_user(&configuration)
      .await
      .map_err(|err| Self::openapi_auth_error("Saved session validation", err))?;

    Ok(())
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

    // Validate the token with an authenticated endpoint, then refresh public
    // server info for connection state.
    let validation_result = async {
      self.validate_saved_token().await?;
      self.fetch_server_info().await?;
      Ok::<(), JellyfinError>(())
    }
    .await;

    match validation_result {
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
    let configuration = self.openapi_configuration(&server_url, Some(&token))?;

    let sessions = jellyfin_api::apis::session_api::get_sessions(
      &configuration,
      jellyfin_api::apis::session_api::GetSessionsParams {
        controllable_by_user_id: None,
        device_id: None,
        active_within_seconds: None,
      },
    )
    .await
    .map_err(|err| Self::openapi_error("Session validation", err))?;

    // Look for our device in the session list
    for session in &sessions {
      if let Some(session_device_id) = session.device_id.as_ref().and_then(|id| id.as_ref()) {
        if session_device_id == &device_id {
          // Found our session! Check if it supports media control
          let supports_media_control = session.supports_media_control.unwrap_or(false);
          let supports_remote_control = session.supports_remote_control.unwrap_or(false);

          log::info!(
            "Found our session: DeviceId={}, SupportsMediaControl={}, SupportsRemoteControl={}",
            device_id,
            supports_media_control,
            supports_remote_control
          );

          log::debug!("Session details: {:?}", session);

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
        .device_id
        .as_ref()
        .and_then(|id| id.as_deref())
        .unwrap_or("?");
      let sess_device_name = session
        .device_name
        .as_ref()
        .and_then(|name| name.as_deref())
        .unwrap_or("?");
      let sess_client = session
        .client
        .as_ref()
        .and_then(|client| client.as_deref())
        .unwrap_or("?");
      let supports_media = session.supports_media_control.unwrap_or(false);
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

impl<'a> JellyfinLogin<'a> {
  pub async fn authenticate(&self, creds: &Credentials) -> Result<AuthResponse, JellyfinError> {
    self.client.authenticate(creds).await
  }

  pub async fn quick_connect_start(
    &self,
    server_url: &str,
  ) -> Result<QuickConnectRequest, JellyfinError> {
    self.client.quick_connect_start(server_url).await
  }

  pub async fn quick_connect_check(
    &self,
    server_url: &str,
    secret: &str,
  ) -> Result<QuickConnectStatus, JellyfinError> {
    self.client.quick_connect_check(server_url, secret).await
  }

  pub async fn quick_connect_authenticate(
    &self,
    server_url: &str,
    secret: &str,
  ) -> Result<AuthResponse, JellyfinError> {
    self
      .client
      .quick_connect_authenticate(server_url, secret)
      .await
  }

  pub async fn restore_session(&self, session: &SavedSession) -> Result<(), JellyfinError> {
    self.client.restore_session(session).await
  }

  pub fn disconnect(&self) {
    self.client.disconnect();
  }

  pub fn get_saved_session(&self) -> Option<SavedSession> {
    self.client.get_saved_session()
  }

  pub fn is_connected(&self) -> bool {
    self.client.is_connected()
  }

  pub fn connection_state(&self) -> ConnectionState {
    self.client.connection_state()
  }
}

impl<'a> JellyfinPlayback<'a> {
  pub fn device_id(&self) -> String {
    self.client.device_id()
  }

  pub async fn get_item(&self, item_id: &str) -> Result<MediaItem, JellyfinError> {
    self.client.get_item(item_id).await
  }

  pub async fn get_playback_info(
    &self,
    item_id: &str,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
  ) -> Result<PlaybackInfoResponse, JellyfinError> {
    self
      .client
      .get_playback_info(item_id, audio_stream_index, subtitle_stream_index)
      .await
  }

  pub async fn get_intro_skipper_ranges(
    &self,
    item_id: &str,
  ) -> Result<Vec<IntroSkipRange>, JellyfinError> {
    self.client.get_intro_skipper_ranges(item_id).await
  }

  pub fn build_stream_url(&self, item_id: &str, media_source: &MediaSource) -> Option<String> {
    self.client.build_stream_url(item_id, media_source)
  }

  pub fn build_subtitle_url(
    &self,
    item_id: &str,
    media_source_id: &str,
    stream: &MediaStream,
  ) -> Option<String> {
    self
      .client
      .build_subtitle_url(item_id, media_source_id, stream)
  }

  pub fn websocket_url(&self) -> Result<String, JellyfinError> {
    self.client.websocket_url()
  }

  pub async fn report_playback_start(&self, info: &PlaybackStartInfo) -> Result<(), JellyfinError> {
    self.client.report_playback_start(info).await
  }

  pub async fn report_playback_progress(
    &self,
    info: &PlaybackProgressInfo,
  ) -> Result<(), JellyfinError> {
    self.client.report_playback_progress(info).await
  }

  pub async fn report_playback_stop(&self, info: &PlaybackStopInfo) -> Result<(), JellyfinError> {
    self.client.report_playback_stop(info).await
  }

  pub async fn report_capabilities(&self) -> Result<(), JellyfinError> {
    self.client.report_capabilities().await
  }

  pub async fn get_next_episode(
    &self,
    current_item: &MediaItem,
  ) -> Result<Option<MediaItem>, JellyfinError> {
    self.client.get_next_episode(current_item).await
  }

  pub async fn get_previous_episode(
    &self,
    current_item: &MediaItem,
  ) -> Result<Option<MediaItem>, JellyfinError> {
    self.client.get_previous_episode(current_item).await
  }

  pub async fn validate_session(&self) -> Result<(), JellyfinError> {
    self.client.validate_session().await
  }
}

impl<'a> JellyfinLibrary<'a> {
  pub async fn video_home(&self) -> Result<VideoHome, JellyfinError> {
    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;

    let continue_watching = jellyfin_api::apis::items_api::get_resume_items(
      &configuration,
      jellyfin_api::apis::items_api::GetResumeItemsParams {
        user_id: Some(user_id.clone()),
        start_index: Some(0),
        limit: Some(12),
        search_term: None,
        parent_id: None,
        fields: Some(video_home_fields()),
        media_types: Some(vec![jellyfin_api::models::MediaType::Video]),
        enable_user_data: Some(true),
        image_type_limit: Some(1),
        enable_image_types: Some(vec![jellyfin_api::models::ImageType::Primary]),
        exclude_item_types: None,
        include_item_types: Some(vec![
          jellyfin_api::models::BaseItemKind::Movie,
          jellyfin_api::models::BaseItemKind::Episode,
        ]),
        enable_total_record_count: Some(false),
        enable_images: Some(true),
        exclude_active_sessions: Some(false),
      },
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video Home continue watching", err))?
    .items
    .unwrap_or_default()
    .into_iter()
    .filter_map(|item| map_video_home_item(&server_url, item))
    .collect();

    let next_up = jellyfin_api::apis::tv_shows_api::get_next_up(
      &configuration,
      jellyfin_api::apis::tv_shows_api::GetNextUpParams {
        user_id: Some(user_id.clone()),
        start_index: Some(0),
        limit: Some(12),
        fields: Some(video_home_fields()),
        series_id: None,
        parent_id: None,
        enable_images: Some(true),
        image_type_limit: Some(1),
        enable_image_types: Some(vec![jellyfin_api::models::ImageType::Primary]),
        enable_user_data: Some(true),
        next_up_date_cutoff: None,
        enable_total_record_count: Some(false),
        disable_first_episode: Some(false),
        enable_resumable: Some(true),
        enable_rewatching: Some(false),
      },
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video Home next up", err))?
    .items
    .unwrap_or_default()
    .into_iter()
    .filter_map(|item| map_video_home_item(&server_url, item))
    .collect();

    let latest_movies = latest_video_items(
      &configuration,
      &server_url,
      &user_id,
      jellyfin_api::models::BaseItemKind::Movie,
      "Video Home latest movies",
    )
    .await?;

    let latest_episodes = latest_video_items(
      &configuration,
      &server_url,
      &user_id,
      jellyfin_api::models::BaseItemKind::Episode,
      "Video Home latest episodes",
    )
    .await?;

    let library_shortcuts = jellyfin_api::apis::user_views_api::get_user_views(
      &configuration,
      jellyfin_api::apis::user_views_api::GetUserViewsParams {
        user_id: Some(user_id),
        include_external_content: Some(false),
        preset_views: Some(vec![
          jellyfin_api::models::CollectionType::Movies,
          jellyfin_api::models::CollectionType::Tvshows,
        ]),
        include_hidden: Some(false),
      },
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video Home library shortcuts", err))?
    .items
    .unwrap_or_default()
    .into_iter()
    .filter_map(|item| map_video_library_shortcut(&server_url, item))
    .collect();

    Ok(VideoHome {
      continue_watching,
      next_up,
      latest_movies,
      latest_episodes,
      library_shortcuts,
    })
  }
}

async fn latest_video_items(
  configuration: &jellyfin_api::apis::configuration::Configuration,
  server_url: &str,
  user_id: &str,
  item_type: jellyfin_api::models::BaseItemKind,
  context: &str,
) -> Result<Vec<VideoHomeItem>, JellyfinError> {
  let items = jellyfin_api::apis::user_library_api::get_latest_media(
    configuration,
    jellyfin_api::apis::user_library_api::GetLatestMediaParams {
      user_id: Some(user_id.to_string()),
      parent_id: None,
      fields: Some(video_home_fields()),
      include_item_types: Some(vec![item_type]),
      is_played: None,
      enable_images: Some(true),
      image_type_limit: Some(1),
      enable_image_types: Some(vec![jellyfin_api::models::ImageType::Primary]),
      enable_user_data: Some(true),
      limit: Some(12),
      group_items: Some(false),
    },
  )
  .await
  .map_err(|err| JellyfinClient::openapi_error(context, err))?;

  Ok(
    items
      .into_iter()
      .filter_map(|item| map_video_home_item(server_url, item))
      .collect(),
  )
}

fn video_home_fields() -> Vec<jellyfin_api::models::ItemFields> {
  vec![
    jellyfin_api::models::ItemFields::PrimaryImageAspectRatio,
    jellyfin_api::models::ItemFields::Overview,
    jellyfin_api::models::ItemFields::DateCreated,
  ]
}

fn map_video_home_item(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoHomeItem> {
  let id = item.id?.to_string();
  let item_type = item.r#type?.to_string();
  let user_data = item.user_data.flatten();
  let artwork_url = primary_artwork_url(server_url, &id, item.image_tags.flatten());

  Some(VideoHomeItem {
    id,
    name: item
      .name
      .flatten()
      .unwrap_or_else(|| "Untitled".to_string()),
    item_type,
    series_id: item.series_id.flatten().map(|id| id.to_string()),
    series_name: item.series_name.flatten(),
    season_number: item.parent_index_number.flatten(),
    episode_number: item.index_number.flatten(),
    production_year: item.production_year.flatten(),
    runtime_seconds: item.run_time_ticks.flatten().map(ticks_to_seconds),
    resume_position_seconds: user_data
      .as_ref()
      .and_then(|data| data.playback_position_ticks)
      .map(ticks_to_seconds),
    played_percentage: user_data
      .as_ref()
      .and_then(|data| data.played_percentage.flatten()),
    played: user_data
      .as_ref()
      .and_then(|data| data.played)
      .unwrap_or(false),
    favorite: user_data
      .as_ref()
      .and_then(|data| data.is_favorite)
      .unwrap_or(false),
    artwork_url,
  })
}

fn map_video_library_shortcut(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoLibraryShortcut> {
  let collection_type = item.collection_type.flatten()?;
  if !matches!(
    collection_type,
    jellyfin_api::models::CollectionType::Movies | jellyfin_api::models::CollectionType::Tvshows
  ) {
    return None;
  }

  let id = item.id?.to_string();
  let artwork_url = primary_artwork_url(server_url, &id, item.image_tags.flatten());

  Some(VideoLibraryShortcut {
    id,
    name: item
      .name
      .flatten()
      .unwrap_or_else(|| "Untitled".to_string()),
    collection_type: collection_type.to_string(),
    item_count: item.recursive_item_count.flatten(),
    artwork_url,
  })
}

fn primary_artwork_url(
  server_url: &str,
  item_id: &str,
  image_tags: Option<std::collections::HashMap<String, String>>,
) -> Option<String> {
  let tag = image_tags?.get("Primary")?.clone();
  Some(format!(
    "{}/Items/{}/Images/Primary?tag={}",
    server_url, item_id, tag
  ))
}

fn ticks_to_seconds(ticks: i64) -> f64 {
  ticks as f64 / 10_000_000.0
}
impl Default for JellyfinClient {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::Arc;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;

  type RequestLog = Arc<parking_lot::Mutex<Vec<String>>>;

  async fn serve_once(status: &'static str, response_body: &'static str) -> String {
    serve_responses(vec![(status, response_body)]).await
  }

  async fn serve_responses(responses: Vec<(&'static str, &'static str)>) -> String {
    serve_responses_with_requests(responses).await.0
  }

  async fn serve_responses_with_requests(
    responses: Vec<(&'static str, &'static str)>,
  ) -> (String, RequestLog) {
    let responses = responses
      .into_iter()
      .map(|(status, body)| (status.to_string(), body.to_string()))
      .collect();
    serve_owned_responses_with_requests(responses).await
  }

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
        let mut buffer = [0; 4096];
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

  #[tokio::test]
  async fn authenticate_creates_saved_session_and_stores_server_name() {
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"User":{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#,
      ),
      (
        "200 OK",
        r#"{"ServerName":"Jellyfin Home","Version":"10.10.0","Id":"server-1"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    client
      .authenticate(&Credentials {
        server_url: server_url.clone(),
        username: "Ada".to_string(),
        password: "correct horse battery staple".to_string(),
      })
      .await
      .expect("password authentication should succeed");

    let session = client
      .get_saved_session()
      .expect("authentication should create saved session");

    assert_eq!(session.server_name.as_deref(), Some("Jellyfin Home"));

    let captured = requests.lock();
    let auth_request = captured.first().expect("auth request should be captured");
    assert!(auth_request.starts_with("POST /Users/AuthenticateByName "));
    assert!(auth_request.contains("Client=\"Jellyfin MPV Shim Rust\""));
    assert!(auth_request.contains(r#""Username":"Ada""#));
    assert!(auth_request.contains(r#""Pw":"correct horse battery staple""#));
    let info_request = captured
      .get(1)
      .expect("public info request should be captured");
    assert!(info_request.starts_with("GET /System/Info/Public "));
  }

  #[tokio::test]
  async fn restore_session_validates_token_and_refreshes_server_name() {
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"}"#,
      ),
      (
        "200 OK",
        r#"{"ServerName":"Jellyfin Home","Version":"10.10.0","Id":"server-1"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    client
      .restore_session(&SavedSession {
        server_url,
        access_token: "token-1".to_string(),
        user_id: "00000000-0000-0000-0000-000000000001".to_string(),
        user_name: "Ada".to_string(),
        server_name: None,
        device_id: Some("jmsr-saved-device".to_string()),
      })
      .await
      .expect("restore should validate token and refresh server info");

    let session = client
      .get_saved_session()
      .expect("restore should keep saved session");
    assert_eq!(session.server_name.as_deref(), Some("Jellyfin Home"));

    let captured = requests.lock();
    let validation_request = captured
      .first()
      .expect("token validation request should be captured");
    assert!(validation_request.starts_with("GET /Users/Me "));
    assert!(validation_request.contains("Token=\"token-1\""));
  }

  #[tokio::test]
  async fn restore_session_clears_state_when_token_validation_fails() {
    let server_url = serve_once("401 Unauthorized", r#"{"Message":"revoked"}"#).await;
    let client = JellyfinClient::new();

    let err = client
      .restore_session(&SavedSession {
        server_url,
        access_token: "token-1".to_string(),
        user_id: "00000000-0000-0000-0000-000000000001".to_string(),
        user_name: "Ada".to_string(),
        server_name: Some("Jellyfin Home".to_string()),
        device_id: Some("jmsr-saved-device".to_string()),
      })
      .await
      .expect_err("restore should report validation failure");

    assert!(
      matches!(err, JellyfinError::AuthFailed(_)),
      "expected auth failure, got {err:?}"
    );
    assert!(!client.is_connected());
  }

  #[tokio::test]
  async fn quick_connect_start_returns_code_and_secret_from_server() {
    let (server_url, requests) = serve_responses_with_requests(vec![(
      "200 OK",
      r#"{"Code":"ABCD12","Secret":"secret-123"}"#,
    )])
    .await;
    let client = JellyfinClient::new();

    let request = client
      .quick_connect_start(&server_url)
      .await
      .expect("quick connect request should start");

    assert_eq!(request.code, "ABCD12");
    assert_eq!(request.secret, "secret-123");

    let captured = requests.lock();
    let request = captured
      .first()
      .expect("quick connect start request should be captured");
    assert!(request.starts_with("POST /QuickConnect/Initiate "));
    assert!(request.contains("Client=\"Jellyfin MPV Shim Rust\""));
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
    let (server_url, requests) = serve_responses_with_requests(vec![(
      "200 OK",
      r#"{"Authenticated":true,"Code":"ABCD12","Secret":"secret-123"}"#,
    )])
    .await;
    let client = JellyfinClient::new();

    let status = client
      .quick_connect_check(&server_url, "secret-123")
      .await
      .expect("quick connect state should load");

    assert!(matches!(status, QuickConnectStatus::Approved));

    let captured = requests.lock();
    let request = captured
      .first()
      .expect("quick connect check request should be captured");
    assert!(request.starts_with("GET /QuickConnect/Connect?secret=secret-123 "));
  }

  #[tokio::test]
  async fn quick_connect_authenticate_creates_saved_session() {
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"User":{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#,
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

    let captured = requests.lock();
    let auth_request = captured
      .first()
      .expect("quick connect auth request should be captured");
    assert!(auth_request.starts_with("POST /Users/AuthenticateWithQuickConnect "));
    assert!(auth_request.contains(r#""Secret":"secret-123""#));
    let info_request = captured
      .get(1)
      .expect("public info request should be captured");
    assert!(info_request.starts_with("GET /System/Info/Public "));
  }

  fn connect_test_client(client: &JellyfinClient, server_url: String) {
    let mut state = client.state.write();
    state.server_url = Some(server_url);
    state.access_token = Some("token-1".to_string());
    state.user_id = Some("00000000-0000-0000-0000-000000000001".to_string());
  }

  #[tokio::test]
  async fn validate_session_accepts_current_device_with_media_control() {
    let client = JellyfinClient::new();
    let device_id = client.device_id();
    let body = format!(
      r#"[{{"DeviceId":"{}","DeviceName":"JMSR","Client":"Jellyfin MPV Shim Rust","SupportsMediaControl":true,"SupportsRemoteControl":true}}]"#,
      device_id
    );
    let (server_url, requests) =
      serve_owned_responses_with_requests(vec![("200 OK".to_string(), body)]).await;
    connect_test_client(&client, server_url);

    client
      .validate_session()
      .await
      .expect("current session should be accepted");

    let captured = requests.lock();
    let request = captured
      .first()
      .expect("validation request should be captured");
    let request_lower = request.to_ascii_lowercase();
    assert!(request.starts_with("GET /Sessions "));
    assert!(request_lower.contains("x-emby-authorization:"));
    assert!(request.contains("Client=\"Jellyfin MPV Shim Rust\""));
    assert!(request.contains("Token=\"token-1\""));
    assert!(request.contains(&format!("DeviceId=\"{}\"", device_id)));
  }

  #[tokio::test]
  async fn validate_session_rejects_current_device_without_media_control() {
    let client = JellyfinClient::new();
    let device_id = client.device_id();
    let body = format!(
      r#"[{{"DeviceId":"{}","DeviceName":"JMSR","Client":"Jellyfin MPV Shim Rust","SupportsMediaControl":false,"SupportsRemoteControl":true}}]"#,
      device_id
    );
    let server_url = serve_owned_responses_with_requests(vec![("200 OK".to_string(), body)])
      .await
      .0;
    connect_test_client(&client, server_url);

    let err = client
      .validate_session()
      .await
      .expect_err("session without media control should be rejected");

    assert!(
      matches!(err, JellyfinError::SessionNotFound),
      "expected missing session, got {err:?}"
    );
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

  #[tokio::test]
  async fn video_home_loads_real_rows_and_video_library_shortcuts() {
    let movie_id = "00000000-0000-0000-0000-000000000010";
    let episode_id = "00000000-0000-0000-0000-000000000011";
    let series_id = "00000000-0000-0000-0000-000000000012";
    let movie_library_id = "00000000-0000-0000-0000-000000000020";
    let shows_library_id = "00000000-0000-0000-0000-000000000021";
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000010","Name":"Resume Movie","Type":"Movie","ProductionYear":2024,"RunTimeTicks":72000000000,"ImageTags":{"Primary":"poster-1"},"UserData":{"PlaybackPositionTicks":1200000000,"PlayedPercentage":25.0,"IsFavorite":true,"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000011","Name":"Next Episode","Type":"Episode","SeriesName":"Example Show","SeriesId":"00000000-0000-0000-0000-000000000012","ParentIndexNumber":1,"IndexNumber":2,"ImageTags":{"Primary":"poster-2"},"UserData":{"PlaybackPositionTicks":0,"PlayedPercentage":0.0,"IsFavorite":false,"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "200 OK",
        r#"[{"Id":"00000000-0000-0000-0000-000000000010","Name":"Latest Movie","Type":"Movie","ImageTags":{"Primary":"poster-3"}}]"#,
      ),
      (
        "200 OK",
        r#"[{"Id":"00000000-0000-0000-0000-000000000011","Name":"Latest Episode","Type":"Episode","SeriesName":"Example Show","ParentIndexNumber":1,"IndexNumber":3}]"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000020","Name":"Movies","Type":"CollectionFolder","CollectionType":"movies","RecursiveItemCount":8},{"Id":"00000000-0000-0000-0000-000000000021","Name":"Shows","Type":"CollectionFolder","CollectionType":"tvshows","RecursiveItemCount":5},{"Id":"00000000-0000-0000-0000-000000000022","Name":"Music","Type":"CollectionFolder","CollectionType":"music","RecursiveItemCount":99}],"TotalRecordCount":3}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url.clone());

    let home = client
      .library()
      .video_home()
      .await
      .expect("video home should load from generated Jellyfin endpoints");

    assert_eq!(home.continue_watching[0].id, movie_id);
    assert_eq!(home.continue_watching[0].name, "Resume Movie");
    let expected_artwork = format!("{server_url}/Items/{movie_id}/Images/Primary?tag=poster-1");
    assert_eq!(
      home.continue_watching[0].artwork_url.as_deref(),
      Some(expected_artwork.as_str())
    );
    assert_eq!(home.next_up[0].id, episode_id);
    assert_eq!(home.next_up[0].series_id.as_deref(), Some(series_id));
    assert_eq!(home.latest_movies[0].name, "Latest Movie");
    assert_eq!(home.latest_episodes[0].name, "Latest Episode");
    assert_eq!(
      home
        .library_shortcuts
        .iter()
        .map(|library| (library.id.as_str(), library.collection_type.as_str()))
        .collect::<Vec<_>>(),
      vec![(movie_library_id, "movies"), (shows_library_id, "tvshows")]
    );

    let captured = requests.lock();
    assert!(captured[0].starts_with("GET /UserItems/Resume?"));
    assert!(captured[0].contains("includeItemTypes=Movie"));
    assert!(captured[0].contains("includeItemTypes=Episode"));
    assert!(captured[1].starts_with("GET /Shows/NextUp?"));
    assert!(captured[2].starts_with("GET /Items/Latest?"));
    assert!(captured[2].contains("includeItemTypes=Movie"));
    assert!(captured[3].contains("includeItemTypes=Episode"));
    assert!(captured[4].starts_with("GET /UserViews?"));
    assert!(captured[4].contains("presetViews=movies"));
    assert!(captured[4].contains("presetViews=tvshows"));
  }

  #[test]
  fn login_and_playback_interfaces_are_separate() {
    let client = JellyfinClient::new();

    crate::jellyfin::client_facade::assert_login_interface(&client);
    crate::jellyfin::client_facade::assert_playback_interface(&client);
  }
}
