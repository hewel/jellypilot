//! Jellyfin HTTP client for REST API calls.

use parking_lot::RwLock;
use reqwest::{header, Client, Method};
use std::sync::Arc;
use uuid::Uuid;

use super::error::JellyfinError;
use super::intro_skipper::{
  parse_intro_skipper_ranges, IntroSkipRange, IntroSkipperPluginResponse,
};
use super::types::*;

/// Device info for Jellyfin client identification.
const DEFAULT_DEVICE_NAME: &str = "JellyPilot";
const DEVICE_ID_PREFIX: &str = "jellypilot-";
const CLIENT_NAME: &str = "JellyPilot";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUPPORTED_REMOTE_COMMANDS: &[&str] = &[
  "Play",
  "Playstate",
  "SetVolume",
  "ToggleMute",
  "ToggleFullscreen",
  "SetAudioStreamIndex",
  "SetSubtitleStreamIndex",
];

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
  provider: MediaServerProvider,
  remote_control_available: bool,
  remote_control_warning: Option<String>,
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
        provider: MediaServerProvider::Jellyfin,
        remote_control_available: false,
        remote_control_warning: None,
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

  fn app_user_agent() -> String {
    format!("{CLIENT_NAME}/{CLIENT_VERSION}")
  }

  fn emby_chrome_user_agent() -> String {
    format!(
      "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 {CLIENT_NAME}/{CLIENT_VERSION}"
    )
  }

  fn request_user_agent(&self) -> String {
    if self.provider() == MediaServerProvider::Emby {
      Self::emby_chrome_user_agent()
    } else {
      Self::app_user_agent()
    }
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
    configuration.user_agent = Some(Self::app_user_agent());
    configuration.client = Client::builder()
      .timeout(std::time::Duration::from_secs(30))
      .default_headers(headers)
      .build()?;

    Ok(configuration)
  }

  fn emby_openapi_configuration(
    &self,
    server_url: &str,
    token: Option<&str>,
  ) -> Result<emby_api::apis::configuration::Configuration, JellyfinError> {
    let mut headers = header::HeaderMap::new();
    let auth_header = header::HeaderValue::from_str(&self.auth_header(token)).map_err(|err| {
      JellyfinError::HttpError(format!("Invalid Emby authorization header: {err}"))
    })?;
    headers.insert("X-Emby-Authorization", auth_header);

    let mut configuration = emby_api::apis::configuration::Configuration::new();
    configuration.base_path = server_url.to_string();
    configuration.user_agent = Some(Self::emby_chrome_user_agent());
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

  fn emby_openapi_error<T: std::fmt::Debug>(
    context: &str,
    err: emby_api::apis::Error<T>,
  ) -> JellyfinError {
    match err {
      emby_api::apis::Error::Reqwest(err) => JellyfinError::Http(err),
      emby_api::apis::Error::Serde(err) => {
        JellyfinError::HttpError(format!("{context} returned malformed JSON: {err}"))
      }
      emby_api::apis::Error::Io(err) => {
        JellyfinError::HttpError(format!("{context} failed: {err}"))
      }
      emby_api::apis::Error::ResponseError(response) => JellyfinError::HttpError(format!(
        "{context} failed: HTTP {} - {}",
        response.status, response.content
      )),
    }
  }

  fn emby_openapi_auth_error<T: std::fmt::Debug>(
    context: &str,
    err: emby_api::apis::Error<T>,
  ) -> JellyfinError {
    match Self::emby_openapi_error(context, err) {
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

  fn emby_auth_response_from_openapi(
    auth: emby_api::models::AuthenticationAuthenticationResult,
  ) -> Result<AuthResponse, JellyfinError> {
    let user = auth
      .user
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "User"))?;
    let id = user
      .id
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "User.Id"))?;
    let name = user
      .name
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "User.Name"))?;
    let access_token = auth
      .access_token
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "AccessToken"))?;
    let server_id = auth
      .server_id
      .ok_or_else(|| Self::missing_openapi_field("Authentication", "ServerId"))?;

    Ok(AuthResponse {
      user: User { id, name },
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

  fn emby_server_info_from_openapi(
    info: emby_api::models::PublicSystemInfo,
  ) -> Result<ServerInfo, JellyfinError> {
    let server_name = info
      .server_name
      .ok_or_else(|| Self::missing_openapi_field("System public info", "ServerName"))?;
    let version = info
      .version
      .ok_or_else(|| Self::missing_openapi_field("System public info", "Version"))?;
    let id = info
      .id
      .ok_or_else(|| Self::missing_openapi_field("System public info", "Id"))?;

    Ok(ServerInfo {
      server_name,
      version,
      id,
    })
  }

  fn emby_server_info_from_authenticated_openapi(
    info: emby_api::models::SystemInfo,
  ) -> Result<ServerInfo, JellyfinError> {
    let server_name = info
      .server_name
      .ok_or_else(|| Self::missing_openapi_field("System info", "ServerName"))?;
    let version = info
      .version
      .ok_or_else(|| Self::missing_openapi_field("System info", "Version"))?;
    let id = info
      .id
      .ok_or_else(|| Self::missing_openapi_field("System info", "Id"))?;

    Ok(ServerInfo {
      server_name,
      version,
      id,
    })
  }

  /// Authenticate with Jellyfin server.
  pub async fn authenticate(&self, creds: &Credentials) -> Result<AuthResponse, JellyfinError> {
    match creds.provider {
      MediaServerProvider::Jellyfin => self.authenticate_jellyfin(creds).await,
      MediaServerProvider::Emby => self.authenticate_emby(creds).await,
    }
  }

  async fn authenticate_jellyfin(
    &self,
    creds: &Credentials,
  ) -> Result<AuthResponse, JellyfinError> {
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
      state.provider = MediaServerProvider::Jellyfin;
      state.remote_control_available = false;
      state.remote_control_warning = None;
      state.server_url = Some(server_url);
      state.access_token = Some(auth.access_token.clone());
      state.user_id = Some(auth.user.id.clone());
      state.user_name = Some(auth.user.name.clone());
    }

    // Fetch server info
    self.fetch_server_info().await.ok();

    Ok(auth)
  }

  async fn authenticate_emby(&self, creds: &Credentials) -> Result<AuthResponse, JellyfinError> {
    let (server_url, auth, info) = self.authenticate_emby_with_discovery(creds).await?;

    {
      let mut state = self.state.write();
      state.provider = MediaServerProvider::Emby;
      state.remote_control_available = false;
      state.remote_control_warning = None;
      state.server_url = Some(server_url);
      state.access_token = Some(auth.access_token.clone());
      state.user_id = Some(auth.user.id.clone());
      state.user_name = Some(auth.user.name.clone());
      state.server_name = info.map(|info| info.server_name);
    }

    Ok(auth)
  }

  async fn authenticate_emby_with_discovery(
    &self,
    creds: &Credentials,
  ) -> Result<(String, AuthResponse, Option<ServerInfo>), JellyfinError> {
    let candidates = Self::emby_api_base_candidates(&creds.server_url)?;
    let mut public_info_failures = Vec::new();

    for candidate in &candidates {
      let configuration = self.emby_openapi_configuration(candidate, None)?;
      match emby_api::apis::system_service_api::get_system_info_public(&configuration)
        .await
        .map_err(|err| Self::emby_openapi_error("System public info", err))
        .and_then(Self::emby_server_info_from_openapi)
      {
        Ok(info) => {
          let auth = self.authenticate_emby_at_base(candidate, creds).await?;
          return Ok((candidate.clone(), auth, Some(info)));
        }
        Err(err) => public_info_failures.push(format!("{candidate}: {err}")),
      }
    }

    let mut auth_failures = Vec::new();

    for candidate in candidates {
      match self.authenticate_emby_at_base(&candidate, creds).await {
        Ok(auth) => {
          let info = self
            .fetch_authenticated_emby_server_info(&candidate, &auth.access_token)
            .await
            .ok();
          return Ok((candidate, auth, info));
        }
        Err(JellyfinError::AuthFailed(message)) => {
          auth_failures.push(format!("{candidate}: {message}"));
        }
        Err(err) => auth_failures.push(format!("{candidate}: {err}")),
      }
    }

    if auth_failures
      .iter()
      .any(|failure| failure.contains("HTTP 401 Unauthorized"))
    {
      return Err(JellyfinError::AuthFailed(format!(
        "Password authentication failed. {}",
        auth_failures.join("; ")
      )));
    }

    Err(JellyfinError::HttpError(format!(
      "Unable to discover Emby API base URL. {}; authenticated fallback failed. {}",
      public_info_failures.join("; "),
      auth_failures.join("; ")
    )))
  }

  async fn authenticate_emby_at_base(
    &self,
    server_url: &str,
    creds: &Credentials,
  ) -> Result<AuthResponse, JellyfinError> {
    let configuration = self.emby_openapi_configuration(server_url, None)?;

    emby_api::apis::user_service_api::post_users_authenticatebyname(
      &configuration,
      emby_api::apis::user_service_api::PostUsersAuthenticatebynameParams {
        x_emby_authorization: self.auth_header(None),
        authenticate_user_by_name: emby_api::models::AuthenticateUserByName {
          username: Some(creds.username.clone()),
          pw: Some(creds.password.clone()),
        },
      },
    )
    .await
    .map_err(|err| Self::emby_openapi_auth_error("Password authentication", err))
    .and_then(Self::emby_auth_response_from_openapi)
  }

  async fn fetch_authenticated_emby_server_info(
    &self,
    server_url: &str,
    token: &str,
  ) -> Result<ServerInfo, JellyfinError> {
    let configuration = self.emby_openapi_configuration(server_url, Some(token))?;

    emby_api::apis::system_service_api::get_system_info(&configuration)
      .await
      .map_err(|err| Self::emby_openapi_error("System info", err))
      .and_then(Self::emby_server_info_from_authenticated_openapi)
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
    let provider = self.state.read().provider;

    let info = match provider {
      MediaServerProvider::Jellyfin => {
        let configuration = self.openapi_configuration(&server_url, None)?;

        jellyfin_api::apis::system_api::get_public_system_info(&configuration)
          .await
          .map_err(|err| Self::openapi_error("System public info", err))
          .and_then(Self::server_info_from_openapi)?
      }
      MediaServerProvider::Emby => {
        let configuration = self.emby_openapi_configuration(&server_url, None)?;

        emby_api::apis::system_service_api::get_system_info_public(&configuration)
          .await
          .map_err(|err| Self::emby_openapi_error("System public info", err))
          .and_then(Self::emby_server_info_from_openapi)?
      }
    };

    {
      let mut state = self.state.write();
      state.server_name = Some(info.server_name.clone());
    }

    Ok(info)
  }

  async fn validate_saved_token(&self) -> Result<(), JellyfinError> {
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let provider = self.state.read().provider;

    match provider {
      MediaServerProvider::Jellyfin => {
        let configuration = self.openapi_configuration(&server_url, Some(&token))?;

        jellyfin_api::apis::user_api::get_current_user(&configuration)
          .await
          .map_err(|err| Self::openapi_auth_error("Saved session validation", err))?;
      }
      MediaServerProvider::Emby => {
        let user_id = self.user_id()?;
        let configuration = self.emby_openapi_configuration(&server_url, Some(&token))?;

        emby_api::apis::user_service_api::get_users_by_id(
          &configuration,
          emby_api::apis::user_service_api::GetUsersByIdParams { id: user_id },
        )
        .await
        .map_err(|err| Self::emby_openapi_auth_error("Saved session validation", err))?;
      }
    }

    Ok(())
  }

  /// Disconnect from server.
  pub fn disconnect(&self) {
    let mut state = self.state.write();
    state.provider = MediaServerProvider::Jellyfin;
    state.remote_control_available = false;
    state.remote_control_warning = None;
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
      state.provider = session.provider;
      state.remote_control_available = false;
      state.remote_control_warning = None;
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
      if matches!(session.provider, MediaServerProvider::Jellyfin) {
        self.fetch_server_info().await?;
      }
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
        provider: state.provider,
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
      provider: state.provider,
      capabilities: Self::provider_capabilities(&state),
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
    let parsed = reqwest::Url::parse(&server_url)
      .map_err(|err| JellyfinError::InvalidUrl(format!("URL could not be parsed: {err}")))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
      return Err(JellyfinError::InvalidUrl(
        "URL must start with http:// or https://".to_string(),
      ));
    }
    if parsed.host_str().is_none() {
      return Err(JellyfinError::InvalidUrl(
        "URL must include a hostname".to_string(),
      ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
      return Err(JellyfinError::InvalidUrl(
        "URL must not include a query string or fragment".to_string(),
      ));
    }

    Ok(server_url)
  }

  fn emby_api_base_candidates(server_url: &str) -> Result<Vec<String>, JellyfinError> {
    let server_url = Self::normalize_server_url(server_url)?;
    let mut candidates = vec![server_url.clone()];

    if !server_url.ends_with("/emby") {
      candidates.push(format!("{server_url}/emby"));
    }

    Ok(candidates)
  }

  fn provider_capabilities(state: &ClientState) -> ProviderCapabilities {
    match state.provider {
      MediaServerProvider::Jellyfin => ProviderCapabilities {
        quick_connect: true,
        intro_skipper: true,
        remote_control: true,
        remote_control_available: state.remote_control_available,
        remote_control_warning: state.remote_control_warning.clone(),
      },
      MediaServerProvider::Emby => ProviderCapabilities {
        quick_connect: false,
        intro_skipper: false,
        remote_control: state.remote_control_warning.is_none(),
        remote_control_available: state.remote_control_available,
        remote_control_warning: state.remote_control_warning.clone(),
      },
    }
  }

  pub fn supports_remote_control(&self) -> bool {
    let state = self.state.read();
    Self::provider_capabilities(&state).remote_control
  }

  fn provider(&self) -> MediaServerProvider {
    self.state.read().provider
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
      .header(header::USER_AGENT, self.request_user_agent())
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

  async fn get_with_query<T: serde::de::DeserializeOwned>(
    &self,
    path: &str,
    query: &[(&str, String)],
  ) -> Result<T, JellyfinError> {
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let url = format!("{}{}", server_url, path);

    let response = self
      .http
      .get(&url)
      .header(header::USER_AGENT, self.request_user_agent())
      .header("X-Emby-Authorization", self.auth_header(Some(&token)))
      .query(query)
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

  async fn request_without_body<T: serde::de::DeserializeOwned>(
    &self,
    method: Method,
    path: &str,
  ) -> Result<T, JellyfinError> {
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let url = format!("{}{}", server_url, path);

    let response = self
      .http
      .request(method.clone(), &url)
      .header(header::USER_AGENT, self.request_user_agent())
      .header("X-Emby-Authorization", self.auth_header(Some(&token)))
      .send()
      .await?;

    let status = response.status();
    if !status.is_success() {
      let body = response.text().await.unwrap_or_default();
      return Err(JellyfinError::HttpError(format!(
        "{} {} failed: HTTP {} - {}",
        method, path, status, body
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
      .header(header::USER_AGENT, self.request_user_agent())
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
      .header(header::USER_AGENT, self.request_user_agent())
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

    if !media_source.supports_direct_play {
      if media_source.supports_direct_stream {
        if let Some(url) = media_source.direct_stream_url.as_deref() {
          let url = absolute_server_url(server_url, url);
          return Some(append_api_key_if_missing(&url, token));
        }
      }

      if media_source.supports_transcoding {
        if let Some(url) = media_source.transcoding_url.as_deref() {
          let url = absolute_server_url(server_url, url);
          return Some(append_api_key_if_missing(&url, token));
        }
      }
    }

    // Build streaming URL - always use HTTP, never raw file paths.
    // The file path in media_source.path is on the server, not locally accessible.
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
      "SupportedCommands": SUPPORTED_REMOTE_COMMANDS,
      "SupportsMediaControl": true,
      "SupportsPersistentIdentifier": true,
    });

    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let url = format!("{}/Sessions/Capabilities/Full", server_url);

    let response = self
      .http
      .post(&url)
      .header(header::USER_AGENT, self.request_user_agent())
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
    match self.provider() {
      MediaServerProvider::Jellyfin => self.validate_jellyfin_session().await,
      MediaServerProvider::Emby => self.validate_emby_session().await,
    }
  }

  async fn validate_jellyfin_session(&self) -> Result<(), JellyfinError> {
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
            let mut state = self.state.write();
            state.remote_control_available = true;
            state.remote_control_warning = None;
            return Ok(());
          } else {
            let mut state = self.state.write();
            state.remote_control_available = false;
            state.remote_control_warning = Some(
              "Remote control is unavailable because the server did not grant media control."
                .to_string(),
            );
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
    {
      let mut state = self.state.write();
      state.remote_control_available = false;
      state.remote_control_warning = Some(
        "Remote control is unavailable because the session is not visible to the server."
          .to_string(),
      );
    }
    Err(JellyfinError::SessionNotFound)
  }

  async fn validate_emby_session(&self) -> Result<(), JellyfinError> {
    let device_id = self.device_id();
    let server_url = self.server_url()?;
    let token = self.access_token()?;
    let configuration = self.emby_openapi_configuration(&server_url, Some(&token))?;

    let sessions = emby_api::apis::sessions_service_api::get_sessions(
      &configuration,
      emby_api::apis::sessions_service_api::GetSessionsParams {
        controllable_by_user_id: None,
        device_id: None,
        id: None,
      },
    )
    .await
    .map_err(|err| Self::emby_openapi_error("Emby session validation", err))?;

    for session in &sessions {
      if let Some(session_device_id) = session.device_id.as_ref() {
        if session_device_id == &device_id {
          let supports_remote_control = session.supports_remote_control.unwrap_or(false);

          log::info!(
            "Found our Emby session: DeviceId={}, SupportsRemoteControl={}",
            device_id,
            supports_remote_control
          );

          log::debug!("Emby session details: {:?}", session);

          if supports_remote_control {
            let mut state = self.state.write();
            state.remote_control_available = true;
            state.remote_control_warning = None;
            return Ok(());
          } else {
            let mut state = self.state.write();
            state.remote_control_available = false;
            state.remote_control_warning = Some(
              "Remote control is unavailable because the server did not grant remote control."
                .to_string(),
            );
            return Err(JellyfinError::SessionNotFound);
          }
        }
      }
    }

    log::warn!(
      "Our Emby session not found in session list. Our DeviceId={}, Total sessions={}",
      device_id,
      sessions.len()
    );
    for (i, session) in sessions.iter().enumerate() {
      let sess_device_id = session.device_id.as_deref().unwrap_or("?");
      let sess_device_name = session.device_name.as_deref().unwrap_or("?");
      let sess_client = session.client.as_deref().unwrap_or("?");
      let supports_remote = session.supports_remote_control.unwrap_or(false);
      log::info!(
        "Emby Session[{}]: DeviceId={}, DeviceName={}, Client={}, SupportsRemoteControl={}",
        i,
        sess_device_id,
        sess_device_name,
        sess_client,
        supports_remote
      );
    }
    {
      let mut state = self.state.write();
      state.remote_control_available = false;
      state.remote_control_warning = Some(
        "Remote control is unavailable because the session is not visible to the server."
          .to_string(),
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

  pub fn websocket_user_agent(&self) -> String {
    self.client.request_user_agent()
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
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_video_home().await;
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;

    let (continue_watching, next_up, latest_movies, latest_episodes) = tokio::try_join!(
      continue_watching_items(&configuration, &server_url, &user_id),
      next_up_items(&configuration, &server_url, &user_id),
      latest_video_items(
        &configuration,
        &server_url,
        &user_id,
        jellyfin_api::models::BaseItemKind::Movie,
        "Video Home latest movies",
      ),
      latest_video_items(
        &configuration,
        &server_url,
        &user_id,
        jellyfin_api::models::BaseItemKind::Episode,
        "Video Home latest episodes",
      ),
    )?;

    Ok(VideoHome {
      continue_watching,
      next_up,
      latest_movies,
      latest_episodes,
    })
  }

  pub async fn library_shortcuts(&self) -> Result<Vec<VideoLibraryShortcut>, JellyfinError> {
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_library_shortcuts().await;
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;

    video_library_shortcuts(&configuration, &server_url, &user_id).await
  }

  pub async fn browse_video(
    &self,
    request: VideoLibraryPageRequest,
  ) -> Result<VideoLibraryPage, JellyfinError> {
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_browse_video(request).await;
    }

    if request.library_id.trim().is_empty() {
      return Err(JellyfinError::HttpError(
        "Library id is required for video browsing".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;
    let start_index = request.start_index.max(0);
    let limit = request.limit.clamp(1, 100);
    let (include_item_types, media_types) = match request.collection_type {
      VideoLibraryKind::Movies => (
        vec![jellyfin_api::models::BaseItemKind::Movie],
        Some(vec![jellyfin_api::models::MediaType::Video]),
      ),
      VideoLibraryKind::TvShows => (vec![jellyfin_api::models::BaseItemKind::Series], None),
    };

    let response = jellyfin_api::apis::items_api::get_items(
      &configuration,
      video_browse_items_params(VideoBrowseItemsQuery {
        user_id,
        library_id: request.library_id.clone(),
        start_index,
        limit,
        include_item_types,
        media_types,
        sort: request.sort,
        played_filter: request.played_filter,
        favorites_only: request.favorites_only,
      }),
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video library browse", err))?;

    let total_record_count = response.total_record_count.unwrap_or(0).max(0);
    let items = response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_video_library_item(&server_url, item))
      .collect::<Vec<_>>();
    let returned_count = i32::try_from(items.len()).unwrap_or(i32::MAX);

    Ok(VideoLibraryPage {
      library_id: request.library_id,
      collection_type: request.collection_type,
      start_index,
      limit,
      total_record_count,
      has_more: start_index.saturating_add(returned_count) < total_record_count,
      items,
    })
  }

  pub async fn search_video(
    &self,
    request: VideoSearchRequest,
  ) -> Result<VideoSearchPage, JellyfinError> {
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_search_video(request).await;
    }

    let query = request.query.trim().to_string();
    if query.is_empty() {
      return Err(JellyfinError::HttpError(
        "Search text is required for video search".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;
    let start_index = request.start_index.max(0);
    let limit = request.limit.clamp(1, 100);

    let response = jellyfin_api::apis::items_api::get_items(
      &configuration,
      video_search_items_params(VideoSearchItemsQuery {
        user_id,
        query: query.clone(),
        start_index,
        limit,
      }),
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video library search", err))?;

    let total_record_count = response.total_record_count.unwrap_or(0).max(0);
    let items = response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_video_library_item(&server_url, item))
      .collect::<Vec<_>>();
    let returned_count = i32::try_from(items.len()).unwrap_or(i32::MAX);

    Ok(VideoSearchPage {
      query,
      start_index,
      limit,
      total_record_count,
      has_more: start_index.saturating_add(returned_count) < total_record_count,
      items,
    })
  }

  pub async fn item_detail(&self, item_id: String) -> Result<VideoItemDetail, JellyfinError> {
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_item_detail(item_id).await;
    }

    if item_id.trim().is_empty() {
      return Err(JellyfinError::HttpError(
        "Item id is required for video details".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;
    let item = self
      .client
      .get(&format!(
        "/Items/{item_id}?userId={user_id}&fields=MediaStreams"
      ))
      .await?;

    map_video_item_detail(&server_url, item).ok_or_else(|| {
      JellyfinError::HttpError(
        "Only Movie and Episode details are supported by the Library Browser".to_string(),
      )
    })
  }

  pub async fn show_detail(&self, series_id: String) -> Result<VideoShowDetail, JellyfinError> {
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_show_detail(series_id).await;
    }

    let series_id = series_id.trim().to_string();
    if series_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Series id is required for show details".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;

    let show_item = jellyfin_api::apis::user_library_api::get_item(
      &configuration,
      jellyfin_api::apis::user_library_api::GetItemParams {
        item_id: series_id.clone(),
        user_id: Some(user_id.clone()),
      },
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video show detail", err))?;
    let mut detail = map_video_show_detail(&server_url, show_item).ok_or_else(|| {
      JellyfinError::HttpError(
        "Only Series details are supported by the Show Library Browser".to_string(),
      )
    })?;

    detail.seasons = jellyfin_api::apis::tv_shows_api::get_seasons(
      &configuration,
      jellyfin_api::apis::tv_shows_api::GetSeasonsParams {
        series_id: series_id.clone(),
        user_id: Some(user_id.clone()),
        fields: Some(video_home_fields()),
        is_special_season: None,
        is_missing: Some(false),
        adjacent_to: None,
        enable_images: Some(true),
        image_type_limit: Some(1),
        enable_image_types: Some(vec![jellyfin_api::models::ImageType::Primary]),
        enable_user_data: Some(true),
      },
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video show seasons", err))?
    .items
    .unwrap_or_default()
    .into_iter()
    .filter_map(|item| map_video_season(&server_url, item))
    .collect();

    detail.next_episode = jellyfin_api::apis::tv_shows_api::get_next_up(
      &configuration,
      jellyfin_api::apis::tv_shows_api::GetNextUpParams {
        user_id: Some(user_id),
        start_index: Some(0),
        limit: Some(1),
        fields: Some(video_home_fields()),
        series_id: Some(series_id),
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
    .map_err(|err| JellyfinClient::openapi_error("Video show next episode", err))?
    .items
    .unwrap_or_default()
    .into_iter()
    .find_map(|item| map_video_library_item(&server_url, item));

    detail.can_play = detail.next_episode.is_some();

    Ok(detail)
  }

  pub async fn season_episodes(
    &self,
    request: VideoSeasonEpisodesRequest,
  ) -> Result<VideoSeasonEpisodes, JellyfinError> {
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_season_episodes(request).await;
    }

    let series_id = request.series_id.trim().to_string();
    if series_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Series id is required for episode browsing".to_string(),
      ));
    }
    let season_id = request.season_id.and_then(|id| {
      let trimmed = id.trim().to_string();
      (!trimmed.is_empty()).then_some(trimmed)
    });
    if season_id.is_none() && request.season_number.is_none() {
      return Err(JellyfinError::HttpError(
        "Season id or number is required for episode browsing".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;

    let episodes = jellyfin_api::apis::tv_shows_api::get_episodes(
      &configuration,
      jellyfin_api::apis::tv_shows_api::GetEpisodesParams {
        series_id: series_id.clone(),
        user_id: Some(user_id),
        fields: Some(video_home_fields()),
        season: request.season_number,
        season_id: season_id.clone(),
        is_missing: Some(false),
        adjacent_to: None,
        start_item_id: None,
        start_index: None,
        limit: None,
        enable_images: Some(true),
        image_type_limit: Some(1),
        enable_image_types: Some(vec![jellyfin_api::models::ImageType::Primary]),
        enable_user_data: Some(true),
        sort_by: Some("ParentIndexNumber,IndexNumber".to_string()),
      },
    )
    .await
    .map_err(|err| JellyfinClient::openapi_error("Video season episodes", err))?
    .items
    .unwrap_or_default()
    .into_iter()
    .filter_map(|item| map_video_library_item(&server_url, item))
    .collect();

    Ok(VideoSeasonEpisodes {
      series_id,
      season_id,
      season_number: request.season_number,
      episodes,
    })
  }

  pub(crate) async fn next_playable_episode(
    &self,
    series_id: String,
  ) -> Result<Option<VideoPlaybackTarget>, JellyfinError> {
    let series_id = series_id.trim().to_string();
    if series_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Series id is required for show playback".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;

    Ok(
      jellyfin_api::apis::tv_shows_api::get_next_up(
        &configuration,
        jellyfin_api::apis::tv_shows_api::GetNextUpParams {
          user_id: Some(user_id),
          start_index: Some(0),
          limit: Some(1),
          fields: Some(video_home_fields()),
          series_id: Some(series_id),
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
      .map_err(|err| JellyfinClient::openapi_error("Video show playback target", err))?
      .items
      .unwrap_or_default()
      .into_iter()
      .find_map(map_video_playback_target),
    )
  }

  pub async fn update_user_data(
    &self,
    request: VideoUserDataUpdateRequest,
  ) -> Result<VideoUserDataUpdate, JellyfinError> {
    if self.client.provider() == MediaServerProvider::Emby {
      return self.emby_update_user_data(request).await;
    }

    let item_id = request.item_id.trim().to_string();
    if item_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Item id is required for user data updates".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let token = self.client.access_token()?;
    let user_id = self.client.user_id()?;
    let configuration = self
      .client
      .openapi_configuration(&server_url, Some(&token))?;

    let user_data = match request.action {
      VideoUserDataAction::Favorite => jellyfin_api::apis::user_library_api::mark_favorite_item(
        &configuration,
        jellyfin_api::apis::user_library_api::MarkFavoriteItemParams {
          item_id: item_id.clone(),
          user_id: Some(user_id),
        },
      )
      .await
      .map_err(|err| JellyfinClient::openapi_error("Mark favorite", err))?,
      VideoUserDataAction::Unfavorite => {
        jellyfin_api::apis::user_library_api::unmark_favorite_item(
          &configuration,
          jellyfin_api::apis::user_library_api::UnmarkFavoriteItemParams {
            item_id: item_id.clone(),
            user_id: Some(user_id),
          },
        )
        .await
        .map_err(|err| JellyfinClient::openapi_error("Unmark favorite", err))?
      }
      VideoUserDataAction::MarkPlayed => jellyfin_api::apis::playstate_api::mark_played_item(
        &configuration,
        jellyfin_api::apis::playstate_api::MarkPlayedItemParams {
          item_id: item_id.clone(),
          user_id: Some(user_id),
          date_played: None,
        },
      )
      .await
      .map_err(|err| JellyfinClient::openapi_error("Mark played", err))?,
      VideoUserDataAction::MarkUnplayed => jellyfin_api::apis::playstate_api::mark_unplayed_item(
        &configuration,
        jellyfin_api::apis::playstate_api::MarkUnplayedItemParams {
          item_id: item_id.clone(),
          user_id: Some(user_id),
        },
      )
      .await
      .map_err(|err| JellyfinClient::openapi_error("Mark unplayed", err))?,
    };

    Ok(map_video_user_data_update(item_id, user_data))
  }
}

impl<'a> JellyfinLibrary<'a> {
  async fn emby_video_home(&self) -> Result<VideoHome, JellyfinError> {
    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;

    let (continue_watching, next_up, latest_movies, latest_episodes) = tokio::try_join!(
      emby_continue_watching_items(self.client, &server_url, &user_id),
      emby_next_up_items(self.client, &server_url, &user_id, None, 12),
      emby_latest_video_items(self.client, &server_url, &user_id, "Movie"),
      emby_latest_video_items(self.client, &server_url, &user_id, "Episode"),
    )?;

    Ok(VideoHome {
      continue_watching,
      next_up,
      latest_movies,
      latest_episodes,
    })
  }

  async fn emby_library_shortcuts(&self) -> Result<Vec<VideoLibraryShortcut>, JellyfinError> {
    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;
    let query = vec![("IncludeExternalContent", "false".to_string())];
    let response = self
      .client
      .get_with_query::<emby_api::models::QueryResultBaseItemDto>(
        &format!("/Users/{user_id}/Views"),
        &query,
      )
      .await?;

    Ok(
      response
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| map_emby_video_library_shortcut(&server_url, item))
        .collect(),
    )
  }

  async fn emby_browse_video(
    &self,
    request: VideoLibraryPageRequest,
  ) -> Result<VideoLibraryPage, JellyfinError> {
    if request.library_id.trim().is_empty() {
      return Err(JellyfinError::HttpError(
        "Library id is required for video browsing".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;
    let start_index = request.start_index.max(0);
    let limit = request.limit.clamp(1, 100);
    let response = self
      .client
      .get_with_query::<emby_api::models::QueryResultBaseItemDto>(
        &format!("/Users/{user_id}/Items"),
        &emby_browse_items_query(EmbyBrowseItemsQuery {
          library_id: Some(request.library_id.clone()),
          collection_type: request.collection_type,
          search_term: None,
          start_index,
          limit,
          sort: request.sort,
          played_filter: request.played_filter,
          favorites_only: request.favorites_only,
        }),
      )
      .await?;

    let total_record_count = response.total_record_count.unwrap_or(0).max(0);
    let items = response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_emby_video_library_item(&server_url, item))
      .collect::<Vec<_>>();
    let returned_count = i32::try_from(items.len()).unwrap_or(i32::MAX);

    Ok(VideoLibraryPage {
      library_id: request.library_id,
      collection_type: request.collection_type,
      start_index,
      limit,
      total_record_count,
      has_more: start_index.saturating_add(returned_count) < total_record_count,
      items,
    })
  }

  async fn emby_search_video(
    &self,
    request: VideoSearchRequest,
  ) -> Result<VideoSearchPage, JellyfinError> {
    let query = request.query.trim().to_string();
    if query.is_empty() {
      return Err(JellyfinError::HttpError(
        "Search text is required for video search".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;
    let start_index = request.start_index.max(0);
    let limit = request.limit.clamp(1, 100);
    let response = self
      .client
      .get_with_query::<emby_api::models::QueryResultBaseItemDto>(
        &format!("/Users/{user_id}/Items"),
        &emby_search_items_query(EmbySearchItemsQuery {
          query: query.clone(),
          start_index,
          limit,
        }),
      )
      .await?;

    let total_record_count = response.total_record_count.unwrap_or(0).max(0);
    let items = response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_emby_video_library_item(&server_url, item))
      .collect::<Vec<_>>();
    let returned_count = i32::try_from(items.len()).unwrap_or(i32::MAX);

    Ok(VideoSearchPage {
      query,
      start_index,
      limit,
      total_record_count,
      has_more: start_index.saturating_add(returned_count) < total_record_count,
      items,
    })
  }

  async fn emby_item_detail(&self, item_id: String) -> Result<VideoItemDetail, JellyfinError> {
    let item_id = item_id.trim().to_string();
    if item_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Item id is required for video details".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;
    let item = self
      .client
      .get_with_query::<emby_api::models::BaseItemDto>(
        &format!("/Users/{user_id}/Items/{item_id}"),
        &emby_detail_query(),
      )
      .await?;

    map_emby_video_item_detail(&server_url, item).ok_or_else(|| {
      JellyfinError::HttpError(
        "Only Movie and Episode details are supported by the Library Browser".to_string(),
      )
    })
  }

  async fn emby_show_detail(&self, series_id: String) -> Result<VideoShowDetail, JellyfinError> {
    let series_id = series_id.trim().to_string();
    if series_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Series id is required for show details".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;
    let show_item = self
      .client
      .get_with_query::<emby_api::models::BaseItemDto>(
        &format!("/Users/{user_id}/Items/{series_id}"),
        &emby_detail_query(),
      )
      .await?;
    let mut detail = map_emby_video_show_detail(&server_url, show_item).ok_or_else(|| {
      JellyfinError::HttpError(
        "Only Series details are supported by the Show Library Browser".to_string(),
      )
    })?;

    detail.seasons = self
      .client
      .get_with_query::<emby_api::models::QueryResultBaseItemDto>(
        &format!("/Shows/{series_id}/Seasons"),
        &emby_season_query(&user_id),
      )
      .await?
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_emby_video_season(&server_url, item))
      .collect();

    detail.next_episode =
      emby_next_up_items(self.client, &server_url, &user_id, Some(&series_id), 1)
        .await?
        .into_iter()
        .next()
        .map(video_home_item_to_library_item);

    detail.can_play = detail.next_episode.is_some();

    Ok(detail)
  }

  async fn emby_season_episodes(
    &self,
    request: VideoSeasonEpisodesRequest,
  ) -> Result<VideoSeasonEpisodes, JellyfinError> {
    let series_id = request.series_id.trim().to_string();
    if series_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Series id is required for episode browsing".to_string(),
      ));
    }
    let season_id = request.season_id.and_then(|id| {
      let trimmed = id.trim().to_string();
      (!trimmed.is_empty()).then_some(trimmed)
    });
    if season_id.is_none() && request.season_number.is_none() {
      return Err(JellyfinError::HttpError(
        "Season id or number is required for episode browsing".to_string(),
      ));
    }

    let server_url = self.client.server_url()?;
    let user_id = self.client.user_id()?;
    let episodes = self
      .client
      .get_with_query::<emby_api::models::QueryResultBaseItemDto>(
        &format!("/Shows/{series_id}/Episodes"),
        &emby_episodes_query(&user_id, season_id.as_deref(), request.season_number),
      )
      .await?
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_emby_video_library_item(&server_url, item))
      .collect();

    Ok(VideoSeasonEpisodes {
      series_id,
      season_id,
      season_number: request.season_number,
      episodes,
    })
  }

  async fn emby_update_user_data(
    &self,
    request: VideoUserDataUpdateRequest,
  ) -> Result<VideoUserDataUpdate, JellyfinError> {
    let item_id = request.item_id.trim().to_string();
    if item_id.is_empty() {
      return Err(JellyfinError::HttpError(
        "Item id is required for user data updates".to_string(),
      ));
    }

    let user_id = self.client.user_id()?;
    let (method, path) = match request.action {
      VideoUserDataAction::Favorite => (
        Method::POST,
        format!("/Users/{user_id}/FavoriteItems/{item_id}"),
      ),
      VideoUserDataAction::Unfavorite => (
        Method::DELETE,
        format!("/Users/{user_id}/FavoriteItems/{item_id}"),
      ),
      VideoUserDataAction::MarkPlayed => (
        Method::POST,
        format!("/Users/{user_id}/PlayedItems/{item_id}"),
      ),
      VideoUserDataAction::MarkUnplayed => (
        Method::DELETE,
        format!("/Users/{user_id}/PlayedItems/{item_id}"),
      ),
    };
    let user_data = self
      .client
      .request_without_body::<emby_api::models::UserItemDataDto>(method, &path)
      .await?;

    Ok(map_emby_video_user_data_update(item_id, user_data))
  }
}

async fn continue_watching_items(
  configuration: &jellyfin_api::apis::configuration::Configuration,
  server_url: &str,
  user_id: &str,
) -> Result<Vec<VideoHomeItem>, JellyfinError> {
  let response = jellyfin_api::apis::items_api::get_resume_items(
    configuration,
    jellyfin_api::apis::items_api::GetResumeItemsParams {
      user_id: Some(user_id.to_string()),
      start_index: Some(0),
      limit: Some(12),
      search_term: None,
      parent_id: None,
      fields: Some(video_home_fields()),
      media_types: Some(vec![jellyfin_api::models::MediaType::Video]),
      enable_user_data: Some(true),
      image_type_limit: Some(1),
      enable_image_types: Some(vec![
        jellyfin_api::models::ImageType::Thumb,
        jellyfin_api::models::ImageType::Primary,
      ]),
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
  .map_err(|err| JellyfinClient::openapi_error("Video Home continue watching", err))?;

  Ok(
    response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_continue_watching_item(server_url, item))
      .collect(),
  )
}

async fn next_up_items(
  configuration: &jellyfin_api::apis::configuration::Configuration,
  server_url: &str,
  user_id: &str,
) -> Result<Vec<VideoHomeItem>, JellyfinError> {
  let response = jellyfin_api::apis::tv_shows_api::get_next_up(
    configuration,
    jellyfin_api::apis::tv_shows_api::GetNextUpParams {
      user_id: Some(user_id.to_string()),
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
  .map_err(|err| JellyfinClient::openapi_error("Video Home next up", err))?;

  Ok(
    response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| {
        map_video_home_item(server_url, item, jellyfin_api::models::ImageType::Primary)
      })
      .collect(),
  )
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
      .filter_map(|item| {
        map_video_home_item(server_url, item, jellyfin_api::models::ImageType::Primary)
      })
      .collect(),
  )
}

async fn video_library_shortcuts(
  configuration: &jellyfin_api::apis::configuration::Configuration,
  server_url: &str,
  user_id: &str,
) -> Result<Vec<VideoLibraryShortcut>, JellyfinError> {
  let response = jellyfin_api::apis::user_views_api::get_user_views(
    configuration,
    jellyfin_api::apis::user_views_api::GetUserViewsParams {
      user_id: Some(user_id.to_string()),
      include_external_content: Some(false),
      preset_views: Some(vec![
        jellyfin_api::models::CollectionType::Movies,
        jellyfin_api::models::CollectionType::Tvshows,
      ]),
      include_hidden: Some(false),
    },
  )
  .await
  .map_err(|err| JellyfinClient::openapi_error("Video Home library shortcuts", err))?;

  Ok(
    response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_video_library_shortcut(server_url, item))
      .collect(),
  )
}

struct VideoBrowseItemsQuery {
  user_id: String,
  library_id: String,
  start_index: i32,
  limit: i32,
  include_item_types: Vec<jellyfin_api::models::BaseItemKind>,
  media_types: Option<Vec<jellyfin_api::models::MediaType>>,
  sort: VideoLibrarySort,
  played_filter: VideoLibraryPlayedFilter,
  favorites_only: bool,
}

fn video_browse_items_params(
  query: VideoBrowseItemsQuery,
) -> jellyfin_api::apis::items_api::GetItemsParams {
  let (sort_by, sort_order) = match query.sort {
    VideoLibrarySort::Title => (
      jellyfin_api::models::ItemSortBy::SortName,
      jellyfin_api::models::SortOrder::Ascending,
    ),
    VideoLibrarySort::RecentlyAdded => (
      jellyfin_api::models::ItemSortBy::DateCreated,
      jellyfin_api::models::SortOrder::Descending,
    ),
    VideoLibrarySort::ReleaseDate => (
      jellyfin_api::models::ItemSortBy::PremiereDate,
      jellyfin_api::models::SortOrder::Descending,
    ),
  };
  let is_played = match query.played_filter {
    VideoLibraryPlayedFilter::All => None,
    VideoLibraryPlayedFilter::Played => Some(true),
    VideoLibraryPlayedFilter::Unplayed => Some(false),
  };

  jellyfin_api::apis::items_api::GetItemsParams {
    user_id: Some(query.user_id),
    max_official_rating: None,
    has_theme_song: None,
    has_theme_video: None,
    has_subtitles: None,
    has_special_feature: None,
    has_trailer: None,
    adjacent_to: None,
    index_number: None,
    parent_index_number: None,
    has_parental_rating: None,
    is_hd: None,
    is4_k: None,
    location_types: None,
    exclude_location_types: None,
    is_missing: None,
    is_unaired: None,
    min_community_rating: None,
    min_critic_rating: None,
    min_premiere_date: None,
    min_date_last_saved: None,
    min_date_last_saved_for_user: None,
    max_premiere_date: None,
    has_overview: None,
    has_imdb_id: None,
    has_tmdb_id: None,
    has_tvdb_id: None,
    is_movie: None,
    is_series: None,
    is_news: None,
    is_kids: None,
    is_sports: None,
    exclude_item_ids: None,
    start_index: Some(query.start_index),
    limit: Some(query.limit),
    recursive: Some(true),
    search_term: None,
    sort_order: Some(vec![sort_order]),
    parent_id: Some(query.library_id),
    fields: Some(video_home_fields()),
    exclude_item_types: None,
    include_item_types: Some(query.include_item_types),
    filters: None,
    is_favorite: query.favorites_only.then_some(true),
    media_types: query.media_types,
    image_types: None,
    sort_by: Some(vec![sort_by]),
    is_played,
    genres: None,
    official_ratings: None,
    tags: None,
    years: None,
    enable_user_data: Some(true),
    image_type_limit: Some(1),
    enable_image_types: Some(vec![jellyfin_api::models::ImageType::Primary]),
    person: None,
    person_ids: None,
    person_types: None,
    studios: None,
    artists: None,
    exclude_artist_ids: None,
    artist_ids: None,
    album_artist_ids: None,
    contributing_artist_ids: None,
    albums: None,
    album_ids: None,
    ids: None,
    video_types: None,
    min_official_rating: None,
    is_locked: None,
    is_place_holder: None,
    has_official_rating: None,
    collapse_box_set_items: Some(false),
    min_width: None,
    min_height: None,
    max_width: None,
    max_height: None,
    is3_d: None,
    series_status: None,
    name_starts_with_or_greater: None,
    name_starts_with: None,
    name_less_than: None,
    studio_ids: None,
    genre_ids: None,
    enable_total_record_count: Some(true),
    enable_images: Some(true),
  }
}

struct VideoSearchItemsQuery {
  user_id: String,
  query: String,
  start_index: i32,
  limit: i32,
}

fn video_search_items_params(
  query: VideoSearchItemsQuery,
) -> jellyfin_api::apis::items_api::GetItemsParams {
  let mut params = video_browse_items_params(VideoBrowseItemsQuery {
    user_id: query.user_id,
    library_id: String::new(),
    start_index: query.start_index,
    limit: query.limit,
    include_item_types: vec![
      jellyfin_api::models::BaseItemKind::Movie,
      jellyfin_api::models::BaseItemKind::Series,
      jellyfin_api::models::BaseItemKind::Episode,
    ],
    media_types: None,
    sort: VideoLibrarySort::Title,
    played_filter: VideoLibraryPlayedFilter::All,
    favorites_only: false,
  });
  params.parent_id = None;
  params.search_term = Some(query.query);
  params
}

fn video_home_fields() -> Vec<jellyfin_api::models::ItemFields> {
  vec![
    jellyfin_api::models::ItemFields::PrimaryImageAspectRatio,
    jellyfin_api::models::ItemFields::Overview,
    jellyfin_api::models::ItemFields::DateCreated,
  ]
}

fn map_continue_watching_item(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoHomeItem> {
  let image_type = match item.r#type? {
    jellyfin_api::models::BaseItemKind::Episode | jellyfin_api::models::BaseItemKind::Series => {
      jellyfin_api::models::ImageType::Primary
    }
    _ => jellyfin_api::models::ImageType::Thumb,
  };

  map_video_home_item(server_url, item, image_type)
}

fn map_video_home_item(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
  image_type: jellyfin_api::models::ImageType,
) -> Option<VideoHomeItem> {
  let id = item.id?.to_string();
  let item_type = item.r#type?.to_string();
  let user_data = item.user_data.flatten();
  let artwork_url = artwork_url(server_url, &id, item.image_tags.flatten(), image_type);

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

fn map_video_library_item(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoLibraryItem> {
  let id = item.id?.to_string();
  let item_type = item.r#type?.to_string();
  let user_data = item.user_data.flatten();
  let artwork_url = artwork_url(
    server_url,
    &id,
    item.image_tags.flatten(),
    jellyfin_api::models::ImageType::Primary,
  );

  let user_data_ref = user_data.as_ref();
  let played = user_data_ref.and_then(|data| data.played).unwrap_or(false);
  let resume_ticks = user_data_ref
    .and_then(|data| data.playback_position_ticks)
    .filter(|&ticks| ticks > 0);
  let resume_position_seconds = if played {
    None
  } else {
    resume_ticks.map(ticks_to_seconds)
  };

  Some(VideoLibraryItem {
    id,
    name: item
      .name
      .flatten()
      .unwrap_or_else(|| "Untitled".to_string()),
    item_type,
    production_year: item.production_year.flatten(),
    runtime_seconds: item.run_time_ticks.flatten().map(ticks_to_seconds),
    played,
    favorite: user_data_ref
      .and_then(|data| data.is_favorite)
      .unwrap_or(false),
    artwork_url,
    season_number: item.parent_index_number.flatten(),
    episode_number: item.index_number.flatten(),
    series_id: item.series_id.flatten().map(|id| id.to_string()),
    series_name: item.series_name.flatten(),
    resume_position_seconds,
    played_percentage: user_data_ref.and_then(|data| data.played_percentage.flatten()),
  })
}

fn map_video_show_detail(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoShowDetail> {
  if !matches!(item.r#type?, jellyfin_api::models::BaseItemKind::Series) {
    return None;
  }

  let id = item.id?.to_string();
  let user_data = item.user_data.flatten();

  Some(VideoShowDetail {
    id: id.clone(),
    name: item
      .name
      .flatten()
      .unwrap_or_else(|| "Untitled".to_string()),
    overview: item.overview.flatten(),
    production_year: item.production_year.flatten(),
    genres: item.genres.flatten().unwrap_or_default(),
    played: user_data
      .as_ref()
      .and_then(|data| data.played)
      .unwrap_or(false),
    favorite: user_data
      .as_ref()
      .and_then(|data| data.is_favorite)
      .unwrap_or(false),
    can_play: false,
    artwork_url: artwork_url(
      server_url,
      &id,
      item.image_tags.flatten(),
      jellyfin_api::models::ImageType::Primary,
    ),
    next_episode: None,
    seasons: Vec::new(),
  })
}

fn map_video_season(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoSeason> {
  let id = item.id?.to_string();
  let user_data = item.user_data.flatten();

  Some(VideoSeason {
    id: id.clone(),
    name: item
      .name
      .flatten()
      .unwrap_or_else(|| "Untitled".to_string()),
    season_number: item.index_number.flatten(),
    played: user_data
      .as_ref()
      .and_then(|data| data.played)
      .unwrap_or(false),
    favorite: user_data
      .as_ref()
      .and_then(|data| data.is_favorite)
      .unwrap_or(false),
    artwork_url: artwork_url(
      server_url,
      &id,
      item.image_tags.flatten(),
      jellyfin_api::models::ImageType::Primary,
    ),
  })
}

fn map_video_playback_target(
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoPlaybackTarget> {
  let item_id = item.id?.to_string();
  let user_data = item.user_data.flatten();
  let played = user_data
    .as_ref()
    .and_then(|data| data.played)
    .unwrap_or(false);
  let start_position_ticks = if played {
    None
  } else {
    user_data.and_then(|data| data.playback_position_ticks)
  };

  Some(VideoPlaybackTarget {
    item_id,
    start_position_ticks,
  })
}

fn map_video_user_data_update(
  item_id: String,
  user_data: jellyfin_api::models::UserItemDataDto,
) -> VideoUserDataUpdate {
  VideoUserDataUpdate {
    item_id,
    played: user_data.played.unwrap_or(false),
    favorite: user_data.is_favorite.unwrap_or(false),
  }
}

fn map_video_item_detail(
  server_url: &str,
  item: jellyfin_api::models::BaseItemDto,
) -> Option<VideoItemDetail> {
  let item_kind = item.r#type?;
  if !matches!(
    item_kind,
    jellyfin_api::models::BaseItemKind::Movie | jellyfin_api::models::BaseItemKind::Episode
  ) {
    return None;
  }

  let id = item.id?.to_string();
  let user_data = item.user_data.flatten();
  let (audio_streams, subtitle_streams) =
    map_video_playback_streams(item.media_streams.flatten().unwrap_or_default());
  let resume_position_seconds = user_data
    .as_ref()
    .and_then(|data| data.playback_position_ticks)
    .map(ticks_to_seconds);
  let played = user_data
    .as_ref()
    .and_then(|data| data.played)
    .unwrap_or(false);

  Some(VideoItemDetail {
    id: id.clone(),
    name: item
      .name
      .flatten()
      .unwrap_or_else(|| "Untitled".to_string()),
    item_type: item_kind.to_string(),
    overview: item.overview.flatten(),
    production_year: item.production_year.flatten(),
    runtime_seconds: item.run_time_ticks.flatten().map(ticks_to_seconds),
    series_id: item.series_id.flatten().map(|id| id.to_string()),
    series_name: item.series_name.flatten(),
    season_number: item.parent_index_number.flatten(),
    episode_number: item.index_number.flatten(),
    genres: item.genres.flatten().unwrap_or_default(),
    played,
    favorite: user_data
      .as_ref()
      .and_then(|data| data.is_favorite)
      .unwrap_or(false),
    played_percentage: user_data
      .as_ref()
      .and_then(|data| data.played_percentage.flatten()),
    resume_position_seconds,
    can_resume: resume_position_seconds.unwrap_or(0.0) > 0.0 && !played,
    can_play: true,
    artwork_url: artwork_url(
      server_url,
      &id,
      item.image_tags.flatten(),
      jellyfin_api::models::ImageType::Primary,
    ),
    audio_streams,
    subtitle_streams,
  })
}

fn map_video_playback_streams(
  streams: Vec<jellyfin_api::models::MediaStream>,
) -> (
  Vec<VideoPlaybackStreamOption>,
  Vec<VideoPlaybackStreamOption>,
) {
  let mut audio_streams = Vec::new();
  let mut subtitle_streams = Vec::new();

  for stream in streams {
    match stream.r#type {
      Some(jellyfin_api::models::MediaStreamType::Audio) => {
        if let Some(option) = map_video_playback_stream_option(stream) {
          audio_streams.push(option);
        }
      }
      Some(jellyfin_api::models::MediaStreamType::Subtitle) => {
        if let Some(option) = map_video_playback_stream_option(stream) {
          subtitle_streams.push(option);
        }
      }
      _ => {}
    }
  }

  (audio_streams, subtitle_streams)
}

fn map_video_playback_stream_option(
  stream: jellyfin_api::models::MediaStream,
) -> Option<VideoPlaybackStreamOption> {
  let index = stream.index?;
  let language = stream.language.flatten();
  let codec = stream.codec.flatten();
  let display_title = stream.display_title.flatten();
  let fallback_label = match (language.as_deref(), codec.as_deref()) {
    (Some(language), Some(codec)) => format!("{language} · {codec}"),
    (Some(language), None) => language.to_string(),
    (None, Some(codec)) => codec.to_string(),
    (None, None) => format!("Stream {index}"),
  };

  Some(VideoPlaybackStreamOption {
    index,
    label: display_title.unwrap_or(fallback_label),
    language,
    codec,
    is_default: stream.is_default.unwrap_or(false),
    is_external: stream.is_external.unwrap_or(false),
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
  let artwork_url = artwork_url(
    server_url,
    &id,
    item.image_tags.flatten(),
    jellyfin_api::models::ImageType::Primary,
  );

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

fn artwork_url(
  server_url: &str,
  item_id: &str,
  image_tags: Option<std::collections::HashMap<String, String>>,
  image_type: jellyfin_api::models::ImageType,
) -> Option<String> {
  let image_type = image_type.to_string();
  let tag = image_tags?.get(&image_type)?.clone();
  Some(format!(
    "{}/Items/{}/Images/{}?tag={}",
    server_url, item_id, image_type, tag
  ))
}

fn ticks_to_seconds(ticks: i64) -> f64 {
  ticks as f64 / 10_000_000.0
}

fn absolute_server_url(server_url: &str, path_or_url: &str) -> String {
  if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
    return path_or_url.to_string();
  }

  if path_or_url.starts_with('/') {
    format!("{server_url}{path_or_url}")
  } else {
    format!("{server_url}/{path_or_url}")
  }
}

fn append_api_key_if_missing(url: &str, token: &str) -> String {
  if url.contains("api_key=") {
    return url.to_string();
  }

  let separator = if url.contains('?') { '&' } else { '?' };
  format!("{url}{separator}api_key={token}")
}

struct EmbyBrowseItemsQuery {
  library_id: Option<String>,
  collection_type: VideoLibraryKind,
  search_term: Option<String>,
  start_index: i32,
  limit: i32,
  sort: VideoLibrarySort,
  played_filter: VideoLibraryPlayedFilter,
  favorites_only: bool,
}

struct EmbySearchItemsQuery {
  query: String,
  start_index: i32,
  limit: i32,
}

async fn emby_continue_watching_items(
  client: &JellyfinClient,
  server_url: &str,
  user_id: &str,
) -> Result<Vec<VideoHomeItem>, JellyfinError> {
  let query = vec![
    ("StartIndex", "0".to_string()),
    ("Limit", "12".to_string()),
    ("MediaTypes", "Video".to_string()),
    ("IncludeItemTypes", "Movie,Episode".to_string()),
    ("Fields", emby_home_fields()),
    ("EnableUserData", "true".to_string()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Thumb,Primary".to_string()),
  ];
  let response = client
    .get_with_query::<emby_api::models::QueryResultBaseItemDto>(
      &format!("/Users/{user_id}/Items/Resume"),
      &query,
    )
    .await?;

  Ok(
    response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_emby_continue_watching_item(server_url, item))
      .collect(),
  )
}

async fn emby_next_up_items(
  client: &JellyfinClient,
  server_url: &str,
  user_id: &str,
  series_id: Option<&str>,
  limit: i32,
) -> Result<Vec<VideoHomeItem>, JellyfinError> {
  let mut query = vec![
    ("UserId", user_id.to_string()),
    ("StartIndex", "0".to_string()),
    ("Limit", limit.to_string()),
    ("Fields", emby_home_fields()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Primary".to_string()),
    ("EnableUserData", "true".to_string()),
    ("EnableResumable", "true".to_string()),
    ("EnableRewatching", "false".to_string()),
  ];
  if let Some(series_id) = series_id {
    query.push(("SeriesId", series_id.to_string()));
  }

  let response = client
    .get_with_query::<emby_api::models::QueryResultBaseItemDto>("/Shows/NextUp", &query)
    .await?;

  Ok(
    response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_emby_video_home_item(server_url, item, "Primary"))
      .collect(),
  )
}

async fn emby_latest_video_items(
  client: &JellyfinClient,
  server_url: &str,
  user_id: &str,
  item_type: &str,
) -> Result<Vec<VideoHomeItem>, JellyfinError> {
  let query = vec![
    ("Limit", "12".to_string()),
    ("Fields", emby_home_fields()),
    ("IncludeItemTypes", item_type.to_string()),
    ("MediaTypes", "Video".to_string()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Primary".to_string()),
    ("EnableUserData", "true".to_string()),
    ("GroupItems", "false".to_string()),
  ];
  let response = client
    .get_with_query::<emby_api::models::QueryResultBaseItemDto>(
      &format!("/Users/{user_id}/Items/Latest"),
      &query,
    )
    .await?;

  Ok(
    response
      .items
      .unwrap_or_default()
      .into_iter()
      .filter_map(|item| map_emby_video_home_item(server_url, item, "Primary"))
      .collect(),
  )
}

fn emby_browse_items_query(query: EmbyBrowseItemsQuery) -> Vec<(&'static str, String)> {
  let (sort_by, sort_order) = match query.sort {
    VideoLibrarySort::Title => ("SortName", "Ascending"),
    VideoLibrarySort::RecentlyAdded => ("DateCreated", "Descending"),
    VideoLibrarySort::ReleaseDate => ("PremiereDate", "Descending"),
  };
  let include_item_types = match query.collection_type {
    VideoLibraryKind::Movies => "Movie",
    VideoLibraryKind::TvShows => "Series",
  };
  let mut params = vec![
    ("StartIndex", query.start_index.to_string()),
    ("Limit", query.limit.to_string()),
    ("Recursive", "true".to_string()),
    ("IncludeItemTypes", include_item_types.to_string()),
    ("SortBy", sort_by.to_string()),
    ("SortOrder", sort_order.to_string()),
    ("Fields", emby_home_fields()),
    ("EnableUserData", "true".to_string()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Primary".to_string()),
    ("EnableTotalRecordCount", "true".to_string()),
    ("GroupItemsIntoCollections", "false".to_string()),
  ];
  if let Some(library_id) = query.library_id {
    params.push(("ParentId", library_id));
  }
  if matches!(query.collection_type, VideoLibraryKind::Movies) {
    params.push(("MediaTypes", "Video".to_string()));
  }
  if let Some(search_term) = query.search_term {
    params.push(("SearchTerm", search_term));
  }
  match query.played_filter {
    VideoLibraryPlayedFilter::All => {}
    VideoLibraryPlayedFilter::Played => params.push(("IsPlayed", "true".to_string())),
    VideoLibraryPlayedFilter::Unplayed => params.push(("IsPlayed", "false".to_string())),
  }
  if query.favorites_only {
    params.push(("IsFavorite", "true".to_string()));
  }
  params
}

fn emby_search_items_query(query: EmbySearchItemsQuery) -> Vec<(&'static str, String)> {
  vec![
    ("StartIndex", query.start_index.to_string()),
    ("Limit", query.limit.to_string()),
    ("Recursive", "true".to_string()),
    ("SearchTerm", query.query),
    ("IncludeItemTypes", "Movie,Series,Episode".to_string()),
    ("SortBy", "SortName".to_string()),
    ("SortOrder", "Ascending".to_string()),
    ("Fields", emby_home_fields()),
    ("EnableUserData", "true".to_string()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Primary".to_string()),
    ("EnableTotalRecordCount", "true".to_string()),
  ]
}

fn emby_detail_query() -> Vec<(&'static str, String)> {
  vec![
    (
      "Fields",
      "MediaStreams,Overview,Genres,PrimaryImageAspectRatio".to_string(),
    ),
    ("EnableUserData", "true".to_string()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Primary".to_string()),
  ]
}

fn emby_season_query(user_id: &str) -> Vec<(&'static str, String)> {
  vec![
    ("UserId", user_id.to_string()),
    ("Fields", emby_home_fields()),
    ("IsMissing", "false".to_string()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Primary".to_string()),
    ("EnableUserData", "true".to_string()),
    ("IncludeItemTypes", "Season".to_string()),
  ]
}

fn emby_episodes_query(
  user_id: &str,
  season_id: Option<&str>,
  season_number: Option<i32>,
) -> Vec<(&'static str, String)> {
  let mut params = vec![
    ("UserId", user_id.to_string()),
    ("Fields", emby_home_fields()),
    ("IsMissing", "false".to_string()),
    ("EnableImages", "true".to_string()),
    ("ImageTypeLimit", "1".to_string()),
    ("EnableImageTypes", "Primary".to_string()),
    ("EnableUserData", "true".to_string()),
    ("IncludeItemTypes", "Episode".to_string()),
    ("SortBy", "ParentIndexNumber,IndexNumber".to_string()),
    ("SortOrder", "Ascending".to_string()),
  ];
  if let Some(season_id) = season_id {
    params.push(("SeasonId", season_id.to_string()));
    params.push(("ParentId", season_id.to_string()));
  }
  if let Some(season_number) = season_number {
    params.push(("Season", season_number.to_string()));
  }
  params
}

fn emby_home_fields() -> String {
  "PrimaryImageAspectRatio,Overview,DateCreated".to_string()
}

fn map_emby_continue_watching_item(
  server_url: &str,
  item: emby_api::models::BaseItemDto,
) -> Option<VideoHomeItem> {
  let image_type = match item.r#type.as_deref()? {
    "Episode" | "Series" => "Primary",
    _ => "Thumb",
  };

  map_emby_video_home_item(server_url, item, image_type)
}

fn map_emby_video_home_item(
  server_url: &str,
  item: emby_api::models::BaseItemDto,
  image_type: &str,
) -> Option<VideoHomeItem> {
  let id = item.id?;
  let item_type = item.r#type?;
  let user_data = item.user_data.as_deref();
  let artwork_url = emby_artwork_url(
    server_url,
    &id,
    item.image_tags,
    item.primary_image_item_id,
    item.primary_image_tag,
    image_type,
  );

  Some(VideoHomeItem {
    id,
    name: item.name.unwrap_or_else(|| "Untitled".to_string()),
    item_type,
    series_id: item.series_id,
    series_name: item.series_name,
    season_number: item.parent_index_number.flatten(),
    episode_number: item.index_number.flatten(),
    production_year: item.production_year.flatten(),
    runtime_seconds: item.run_time_ticks.flatten().map(ticks_to_seconds),
    resume_position_seconds: user_data
      .and_then(|data| data.playback_position_ticks)
      .map(ticks_to_seconds),
    played_percentage: user_data.and_then(|data| data.played_percentage.flatten()),
    played: user_data.and_then(|data| data.played).unwrap_or(false),
    favorite: user_data.and_then(|data| data.is_favorite).unwrap_or(false),
    artwork_url,
  })
}

fn map_emby_video_library_item(
  server_url: &str,
  item: emby_api::models::BaseItemDto,
) -> Option<VideoLibraryItem> {
  let id = item.id?;
  let item_type = item.r#type?;
  let user_data = item.user_data.as_deref();
  let artwork_url = emby_artwork_url(
    server_url,
    &id,
    item.image_tags,
    item.primary_image_item_id,
    item.primary_image_tag,
    "Primary",
  );

  let played = user_data.and_then(|data| data.played).unwrap_or(false);
  let resume_ticks = user_data
    .and_then(|data| data.playback_position_ticks)
    .filter(|&ticks| ticks > 0);
  let resume_position_seconds = if played {
    None
  } else {
    resume_ticks.map(ticks_to_seconds)
  };

  Some(VideoLibraryItem {
    id,
    name: item.name.unwrap_or_else(|| "Untitled".to_string()),
    item_type,
    production_year: item.production_year.flatten(),
    runtime_seconds: item.run_time_ticks.flatten().map(ticks_to_seconds),
    played,
    favorite: user_data.and_then(|data| data.is_favorite).unwrap_or(false),
    artwork_url,
    season_number: item.parent_index_number.flatten(),
    episode_number: item.index_number.flatten(),
    series_id: item.series_id,
    series_name: item.series_name,
    resume_position_seconds,
    played_percentage: user_data.and_then(|data| data.played_percentage.flatten()),
  })
}

fn map_emby_video_show_detail(
  server_url: &str,
  item: emby_api::models::BaseItemDto,
) -> Option<VideoShowDetail> {
  if item.r#type.as_deref()? != "Series" {
    return None;
  }

  let id = item.id?;
  let user_data = item.user_data.as_deref();
  let artwork_url = emby_artwork_url(
    server_url,
    &id,
    item.image_tags,
    item.primary_image_item_id,
    item.primary_image_tag,
    "Primary",
  );

  Some(VideoShowDetail {
    id,
    name: item.name.unwrap_or_else(|| "Untitled".to_string()),
    overview: item.overview,
    production_year: item.production_year.flatten(),
    genres: item.genres.unwrap_or_default(),
    played: user_data.and_then(|data| data.played).unwrap_or(false),
    favorite: user_data.and_then(|data| data.is_favorite).unwrap_or(false),
    can_play: false,
    artwork_url,
    next_episode: None,
    seasons: Vec::new(),
  })
}

fn map_emby_video_season(
  server_url: &str,
  item: emby_api::models::BaseItemDto,
) -> Option<VideoSeason> {
  let id = item.id?;
  let user_data = item.user_data.as_deref();
  let artwork_url = emby_artwork_url(
    server_url,
    &id,
    item.image_tags,
    item.primary_image_item_id,
    item.primary_image_tag,
    "Primary",
  );

  Some(VideoSeason {
    id,
    name: item.name.unwrap_or_else(|| "Untitled".to_string()),
    season_number: item.index_number.flatten(),
    played: user_data.and_then(|data| data.played).unwrap_or(false),
    favorite: user_data.and_then(|data| data.is_favorite).unwrap_or(false),
    artwork_url,
  })
}

fn map_emby_video_item_detail(
  server_url: &str,
  item: emby_api::models::BaseItemDto,
) -> Option<VideoItemDetail> {
  let item_type = item.r#type?;
  if !matches!(item_type.as_str(), "Movie" | "Episode") {
    return None;
  }

  let id = item.id?;
  let user_data = item.user_data.as_deref();
  let (audio_streams, subtitle_streams) =
    map_emby_video_playback_streams(item.media_streams.unwrap_or_default());
  let resume_position_seconds = user_data
    .and_then(|data| data.playback_position_ticks)
    .map(ticks_to_seconds);
  let played = user_data.and_then(|data| data.played).unwrap_or(false);
  let artwork_url = emby_artwork_url(
    server_url,
    &id,
    item.image_tags,
    item.primary_image_item_id,
    item.primary_image_tag,
    "Primary",
  );

  Some(VideoItemDetail {
    id,
    name: item.name.unwrap_or_else(|| "Untitled".to_string()),
    item_type,
    overview: item.overview,
    production_year: item.production_year.flatten(),
    runtime_seconds: item.run_time_ticks.flatten().map(ticks_to_seconds),
    series_id: item.series_id,
    series_name: item.series_name,
    season_number: item.parent_index_number.flatten(),
    episode_number: item.index_number.flatten(),
    genres: item.genres.unwrap_or_default(),
    played,
    favorite: user_data.and_then(|data| data.is_favorite).unwrap_or(false),
    played_percentage: user_data.and_then(|data| data.played_percentage.flatten()),
    resume_position_seconds,
    can_resume: resume_position_seconds.unwrap_or(0.0) > 0.0 && !played,
    can_play: true,
    artwork_url,
    audio_streams,
    subtitle_streams,
  })
}

fn map_emby_video_playback_streams(
  streams: Vec<emby_api::models::MediaStream>,
) -> (
  Vec<VideoPlaybackStreamOption>,
  Vec<VideoPlaybackStreamOption>,
) {
  let mut audio_streams = Vec::new();
  let mut subtitle_streams = Vec::new();

  for stream in streams {
    match stream.r#type {
      Some(emby_api::models::MediaStreamType::Audio) => {
        if let Some(option) = map_emby_video_playback_stream_option(stream) {
          audio_streams.push(option);
        }
      }
      Some(emby_api::models::MediaStreamType::Subtitle) => {
        if let Some(option) = map_emby_video_playback_stream_option(stream) {
          subtitle_streams.push(option);
        }
      }
      _ => {}
    }
  }

  (audio_streams, subtitle_streams)
}

fn map_emby_video_playback_stream_option(
  stream: emby_api::models::MediaStream,
) -> Option<VideoPlaybackStreamOption> {
  let index = stream.index?;
  let language = stream.language;
  let codec = stream.codec;
  let display_title = stream.display_title.or(stream.title);
  let fallback_label = match (language.as_deref(), codec.as_deref()) {
    (Some(language), Some(codec)) => format!("{language} · {codec}"),
    (Some(language), None) => language.to_string(),
    (None, Some(codec)) => codec.to_string(),
    (None, None) => format!("Stream {index}"),
  };

  Some(VideoPlaybackStreamOption {
    index,
    label: display_title.unwrap_or(fallback_label),
    language,
    codec,
    is_default: stream.is_default.unwrap_or(false),
    is_external: stream.is_external.unwrap_or(false),
  })
}

fn map_emby_video_library_shortcut(
  server_url: &str,
  item: emby_api::models::BaseItemDto,
) -> Option<VideoLibraryShortcut> {
  let collection_type = item.collection_type?;
  if !matches!(collection_type.as_str(), "movies" | "tvshows") {
    return None;
  }

  let id = item.id?;
  let artwork_url = emby_artwork_url(
    server_url,
    &id,
    item.image_tags,
    item.primary_image_item_id,
    item.primary_image_tag,
    "Primary",
  );

  Some(VideoLibraryShortcut {
    id,
    name: item.name.unwrap_or_else(|| "Untitled".to_string()),
    collection_type,
    item_count: item.recursive_item_count.flatten(),
    artwork_url,
  })
}

fn map_emby_video_user_data_update(
  item_id: String,
  user_data: emby_api::models::UserItemDataDto,
) -> VideoUserDataUpdate {
  VideoUserDataUpdate {
    item_id,
    played: user_data.played.unwrap_or(false),
    favorite: user_data.is_favorite.unwrap_or(false),
  }
}

fn emby_artwork_url(
  server_url: &str,
  item_id: &str,
  image_tags: Option<std::collections::HashMap<String, String>>,
  primary_image_item_id: Option<String>,
  primary_image_tag: Option<String>,
  image_type: &str,
) -> Option<String> {
  let (image_item_id, tag) = image_tags
    .and_then(|tags| tags.get(image_type).cloned())
    .map(|tag| (item_id.to_string(), tag))
    .or_else(|| {
      (image_type == "Primary").then(|| {
        primary_image_tag.map(|tag| {
          (
            primary_image_item_id.unwrap_or_else(|| item_id.to_string()),
            tag,
          )
        })
      })?
    })?;

  Some(format!(
    "{}/Items/{}/Images/{}?tag={}",
    server_url, image_item_id, image_type, tag
  ))
}

fn video_home_item_to_library_item(item: VideoHomeItem) -> VideoLibraryItem {
  VideoLibraryItem {
    id: item.id,
    name: item.name,
    item_type: item.item_type,
    production_year: item.production_year,
    runtime_seconds: item.runtime_seconds,
    played: item.played,
    favorite: item.favorite,
    artwork_url: item.artwork_url,
    season_number: item.season_number,
    episode_number: item.episode_number,
    series_id: item.series_id,
    series_name: item.series_name,
    resume_position_seconds: item.resume_position_seconds,
    played_percentage: item.played_percentage,
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
  use std::sync::Arc;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;

  type RequestLog = Arc<parking_lot::Mutex<Vec<String>>>;

  fn assert_chrome_jellypilot_user_agent(request: &str) {
    let request = request.to_ascii_lowercase();
    assert!(request.contains("user-agent: mozilla/5.0"));
    assert!(request.contains("applewebkit/537.36"));
    assert!(request.contains("chrome/"));
    assert!(request.contains("safari/537.36"));
    assert!(request.contains("jellypilot/"));
  }

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

  async fn serve_route_responses_with_requests(
    responses: Vec<(&'static str, &'static str, &'static str)>,
  ) -> (String, RequestLog) {
    let responses = responses
      .into_iter()
      .map(|(request_match, status, body)| {
        (
          request_match.to_string(),
          status.to_string(),
          body.to_string(),
        )
      })
      .collect::<Vec<_>>();
    let request_count = responses.len();
    let listener = TcpListener::bind("127.0.0.1:0")
      .await
      .expect("test server should bind");
    let addr = listener.local_addr().expect("test server should have addr");
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&requests);

    tokio::spawn(async move {
      for _ in 0..request_count {
        let (mut stream, _) = listener.accept().await.expect("test server should accept");
        let mut buffer = [0; 4096];
        let bytes_read = stream
          .read(&mut buffer)
          .await
          .expect("test server should read request");
        let request = String::from_utf8_lossy(&buffer[..bytes_read]).into_owned();
        let response_spec = responses
          .iter()
          .find(|(request_match, _, _)| request.contains(request_match));
        captured_requests.lock().push(request);
        let (status, response_body) = response_spec.map_or(
          ("404 Not Found", r#"{"Message":"missing test route"}"#),
          |(_, status, body)| (status.as_str(), body.as_str()),
        );
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
        provider: MediaServerProvider::Jellyfin,
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
    assert!(auth_request.contains("Client=\"JellyPilot\""));
    assert!(auth_request.contains(r#""Username":"Ada""#));
    assert!(auth_request.contains(r#""Pw":"correct horse battery staple""#));
    let info_request = captured
      .get(1)
      .expect("public info request should be captured");
    assert!(info_request.starts_with("GET /System/Info/Public "));
  }

  #[tokio::test]
  async fn emby_authentication_discovers_emby_api_base_under_reverse_proxy_prefix() {
    let (server_url, requests) = serve_route_responses_with_requests(vec![
      (
        "GET /proxy/System/Info/Public ",
        "404 Not Found",
        r#"{"Message":"missing"}"#,
      ),
      (
        "GET /proxy/emby/System/Info/Public ",
        "200 OK",
        r#"{"ServerName":"Emby Home","Version":"4.9.3.0","Id":"emby-server"}"#,
      ),
      (
        "POST /proxy/emby/Users/AuthenticateByName ",
        "200 OK",
        r#"{"User":{"Id":"emby-user-1","Name":"Ada"},"AccessToken":"emby-token","ServerId":"emby-server"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    client
      .authenticate(&Credentials {
        provider: MediaServerProvider::Emby,
        server_url: format!("{server_url}/proxy"),
        username: "Ada".to_string(),
        password: "correct horse battery staple".to_string(),
      })
      .await
      .expect("emby password authentication should succeed");

    let session = client
      .get_saved_session()
      .expect("authentication should create saved session");
    assert_eq!(session.provider, MediaServerProvider::Emby);
    assert_eq!(session.server_url, format!("{server_url}/proxy/emby"));
    assert_eq!(session.server_name.as_deref(), Some("Emby Home"));
    assert_eq!(session.access_token, "emby-token");
    let state = client.connection_state();
    assert!(!state.capabilities.quick_connect);
    assert!(!state.capabilities.intro_skipper);
    assert!(state.capabilities.remote_control);
    assert!(!state.capabilities.remote_control_available);
    assert!(state.capabilities.remote_control_warning.is_none());

    let captured = requests.lock();
    let public_info_request = captured
      .first()
      .expect("public info request should be captured before auth");
    assert_chrome_jellypilot_user_agent(public_info_request);
    let auth_request = captured
      .get(2)
      .expect("auth request should be captured after probing");
    assert!(auth_request.contains("Client=\"JellyPilot\""));
    assert_chrome_jellypilot_user_agent(auth_request);
    assert!(auth_request.contains(r#""Username":"Ada""#));
    assert!(auth_request.contains(r#""Pw":"correct horse battery staple""#));
  }

  #[tokio::test]
  async fn emby_authentication_falls_back_to_password_when_public_info_is_forbidden() {
    let forbidden = "<html><head><title>403 Forbidden</title></head><body><center><h1>403 Forbidden</h1></center><hr><center>nginx</center></body></html>";
    let (server_url, requests) = serve_route_responses_with_requests(vec![
      ("GET /System/Info/Public ", "403 Forbidden", forbidden),
      ("GET /emby/System/Info/Public ", "403 Forbidden", forbidden),
      ("POST /Users/AuthenticateByName ", "403 Forbidden", forbidden),
      (
        "POST /emby/Users/AuthenticateByName ",
        "200 OK",
        r#"{"User":{"Id":"emby-user-1","Name":"Ada"},"AccessToken":"emby-token","ServerId":"emby-server"}"#,
      ),
      (
        "GET /emby/System/Info ",
        "200 OK",
        r#"{"ServerName":"Emby Home","Version":"4.9.3.0","Id":"emby-server"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    client
      .authenticate(&Credentials {
        provider: MediaServerProvider::Emby,
        server_url,
        username: "Ada".to_string(),
        password: "correct horse battery staple".to_string(),
      })
      .await
      .expect("emby password authentication should succeed after public info is blocked");

    let session = client
      .get_saved_session()
      .expect("authentication should create saved session");
    assert_eq!(session.provider, MediaServerProvider::Emby);
    assert!(session.server_url.ends_with("/emby"));
    assert_eq!(session.server_name.as_deref(), Some("Emby Home"));

    let captured = requests.lock();
    let auth_request = captured
      .get(3)
      .expect("auth fallback should try the /emby candidate");
    assert!(auth_request.starts_with("POST /emby/Users/AuthenticateByName "));
    assert_chrome_jellypilot_user_agent(auth_request);
    assert!(auth_request.contains(r#""Username":"Ada""#));
    assert!(auth_request.contains(r#""Pw":"correct horse battery staple""#));
    let info_request = captured
      .get(4)
      .expect("authenticated server info should refresh the server name");
    assert!(info_request.starts_with("GET /emby/System/Info "));
    assert!(info_request.contains("Token=\"emby-token\""));
  }

  #[tokio::test]
  async fn emby_restore_session_validates_token_and_preserves_saved_device_id() {
    let (server_url, requests) = serve_route_responses_with_requests(vec![(
      "GET /emby/Users/emby-user-1 ",
      "200 OK",
      r#"{"Id":"emby-user-1","Name":"Ada"}"#,
    )])
    .await;
    let client = JellyfinClient::new();

    client
      .restore_session(&SavedSession {
        provider: MediaServerProvider::Emby,
        server_url: format!("{server_url}/emby"),
        access_token: "emby-token".to_string(),
        user_id: "emby-user-1".to_string(),
        user_name: "Ada".to_string(),
        server_name: Some("Emby Home".to_string()),
        device_id: Some("jellypilot-saved-emby-device".to_string()),
      })
      .await
      .expect("emby restore should validate token");

    let session = client
      .get_saved_session()
      .expect("restore should keep saved session");
    assert_eq!(session.provider, MediaServerProvider::Emby);
    assert_eq!(
      session.device_id.as_deref(),
      Some("jellypilot-saved-emby-device")
    );

    let captured = requests.lock();
    let validation_request = captured
      .first()
      .expect("token validation request should be captured");
    assert!(validation_request.starts_with("GET /emby/Users/emby-user-1 "));
    assert!(validation_request.contains("Token=\"emby-token\""));
    assert!(validation_request.contains("DeviceId=\"jellypilot-saved-emby-device\""));
  }

  #[tokio::test]
  async fn emby_authentication_reports_discovery_failure_when_no_api_base_responds() {
    let (server_url, _requests) = serve_route_responses_with_requests(vec![
      (
        "GET /System/Info/Public ",
        "404 Not Found",
        r#"{"Message":"missing"}"#,
      ),
      (
        "GET /emby/System/Info/Public ",
        "404 Not Found",
        r#"{"Message":"missing"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    let err = client
      .authenticate(&Credentials {
        provider: MediaServerProvider::Emby,
        server_url,
        username: "Ada".to_string(),
        password: "wrong".to_string(),
      })
      .await
      .expect_err("missing Emby base should fail before authentication");

    assert!(
      matches!(err, JellyfinError::HttpError(ref message) if message.contains("Unable to discover Emby API base URL")),
      "expected Emby discovery failure, got {err:?}"
    );
  }

  #[tokio::test]
  async fn emby_authentication_maps_unauthorized_to_auth_failed() {
    let (server_url, _requests) = serve_route_responses_with_requests(vec![
      (
        "GET /System/Info/Public ",
        "200 OK",
        r#"{"ServerName":"Emby Home","Version":"4.9.3.0","Id":"emby-server"}"#,
      ),
      (
        "POST /Users/AuthenticateByName ",
        "401 Unauthorized",
        r#"{"Message":"bad credentials"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    let err = client
      .authenticate(&Credentials {
        provider: MediaServerProvider::Emby,
        server_url,
        username: "Ada".to_string(),
        password: "wrong".to_string(),
      })
      .await
      .expect_err("bad Emby credentials should fail");

    assert!(
      matches!(err, JellyfinError::AuthFailed(ref message) if message.contains("Password authentication failed")),
      "expected auth failure, got {err:?}"
    );
  }

  #[tokio::test]
  async fn emby_authentication_reports_malformed_response_as_http_error() {
    let (server_url, _requests) = serve_route_responses_with_requests(vec![
      (
        "GET /System/Info/Public ",
        "200 OK",
        r#"{"ServerName":"Emby Home","Version":"4.9.3.0","Id":"emby-server"}"#,
      ),
      (
        "POST /Users/AuthenticateByName ",
        "200 OK",
        r#"{"User":{"Id":"emby-user-1","Name":"Ada"},"ServerId":"emby-server"}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();

    let err = client
      .authenticate(&Credentials {
        provider: MediaServerProvider::Emby,
        server_url,
        username: "Ada".to_string(),
        password: "wrong".to_string(),
      })
      .await
      .expect_err("missing token should fail");

    assert!(
      matches!(err, JellyfinError::HttpError(ref message) if message.contains("Authentication response missing AccessToken")),
      "expected malformed response error, got {err:?}"
    );
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
        provider: MediaServerProvider::Jellyfin,
        server_url,
        access_token: "token-1".to_string(),
        user_id: "00000000-0000-0000-0000-000000000001".to_string(),
        user_name: "Ada".to_string(),
        server_name: None,
        device_id: Some("jellypilot-saved-device".to_string()),
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
        provider: MediaServerProvider::Jellyfin,
        server_url,
        access_token: "token-1".to_string(),
        user_id: "00000000-0000-0000-0000-000000000001".to_string(),
        user_name: "Ada".to_string(),
        server_name: Some("Jellyfin Home".to_string()),
        device_id: Some("jellypilot-saved-device".to_string()),
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
    assert!(request.contains("Client=\"JellyPilot\""));
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

  fn connect_test_client_as_emby(client: &JellyfinClient, server_url: String) {
    let mut state = client.state.write();
    state.provider = MediaServerProvider::Emby;
    state.server_url = Some(server_url);
    state.access_token = Some("emby-token".to_string());
    state.user_id = Some("00000000-0000-0000-0000-000000000001".to_string());
  }

  #[tokio::test]
  async fn validate_session_accepts_current_device_with_media_control() {
    let client = JellyfinClient::new();
    let device_id = client.device_id();
    let body = format!(
      r#"[{{"DeviceId":"{}","DeviceName":"JellyPilot","Client":"JellyPilot","SupportsMediaControl":true,"SupportsRemoteControl":true}}]"#,
      device_id
    );
    let (server_url, requests) =
      serve_owned_responses_with_requests(vec![("200 OK".to_string(), body)]).await;
    connect_test_client(&client, server_url);

    client
      .validate_session()
      .await
      .expect("current session should be accepted");
    let state = client.connection_state();
    assert!(state.capabilities.remote_control);
    assert!(state.capabilities.remote_control_available);
    assert!(state.capabilities.remote_control_warning.is_none());

    let captured = requests.lock();
    let request = captured
      .first()
      .expect("validation request should be captured");
    let request_lower = request.to_ascii_lowercase();
    assert!(request.starts_with("GET /Sessions "));
    assert!(request_lower.contains("x-emby-authorization:"));
    assert!(request.contains("Client=\"JellyPilot\""));
    assert!(request.contains("Token=\"token-1\""));
    assert!(request.contains(&format!("DeviceId=\"{}\"", device_id)));
  }

  #[tokio::test]
  async fn emby_validate_session_accepts_current_device_with_remote_control() {
    let client = JellyfinClient::new();
    let device_id = client.device_id();
    let body = format!(
      r#"[{{"DeviceId":"{}","DeviceName":"JellyPilot","Client":"JellyPilot","SupportsRemoteControl":true}}]"#,
      device_id
    );
    let (server_url, requests) =
      serve_owned_responses_with_requests(vec![("200 OK".to_string(), body)]).await;
    connect_test_client_as_emby(&client, server_url);

    client
      .validate_session()
      .await
      .expect("current Emby session should be accepted");
    let state = client.connection_state();
    assert!(state.capabilities.remote_control);
    assert!(state.capabilities.remote_control_available);
    assert!(state.capabilities.remote_control_warning.is_none());

    let captured = requests.lock();
    let request = captured
      .first()
      .expect("Emby validation request should be captured");
    assert!(request.starts_with("GET /Sessions "));
    assert!(request.contains("Client=\"JellyPilot\""));
    assert!(request.contains("Token=\"emby-token\""));
    assert!(request.contains(&format!("DeviceId=\"{}\"", device_id)));
  }

  #[test]
  fn emby_websocket_url_preserves_api_base_and_stable_device_id() {
    let client = JellyfinClient::new();
    let device_id = client.device_id();
    {
      let mut state = client.state.write();
      state.provider = MediaServerProvider::Emby;
      state.server_url = Some("https://media.example.test/emby".to_string());
      state.access_token = Some("emby-token".to_string());
    }

    let url = client
      .websocket_url()
      .expect("Emby websocket URL should be built from connection state");

    assert_eq!(
      url,
      format!("wss://media.example.test/emby/socket?api_key=emby-token&deviceId={device_id}")
    );
  }

  #[tokio::test]
  async fn emby_validate_session_failure_keeps_connection_alive_with_warning() {
    let client = JellyfinClient::new();
    let device_id = client.device_id();
    let body = format!(
      r#"[{{"DeviceId":"{}","DeviceName":"JellyPilot","Client":"JellyPilot","SupportsRemoteControl":false}}]"#,
      device_id
    );
    let server_url = serve_owned_responses_with_requests(vec![("200 OK".to_string(), body)])
      .await
      .0;
    connect_test_client_as_emby(&client, server_url);

    let err = client
      .validate_session()
      .await
      .expect_err("Emby session without remote control should be rejected");

    assert!(
      matches!(err, JellyfinError::SessionNotFound),
      "expected missing session, got {err:?}"
    );
    assert!(client.is_connected());
    let state = client.connection_state();
    assert!(state.connected);
    assert!(!state.capabilities.remote_control);
    assert!(!state.capabilities.remote_control_available);
    assert!(state.capabilities.remote_control_warning.is_some());
  }

  #[tokio::test]
  async fn emby_capability_registration_advertises_only_supported_commands() {
    let client = JellyfinClient::new();
    let (server_url, requests) =
      serve_owned_responses_with_requests(vec![("204 No Content".to_string(), String::new())])
        .await;
    connect_test_client_as_emby(&client, server_url);

    client
      .report_capabilities()
      .await
      .expect("Emby capability registration should post supported commands");

    let captured = requests.lock();
    let request = captured
      .first()
      .expect("capability registration request should be captured");
    assert!(request.starts_with("POST /Sessions/Capabilities/Full "));
    assert!(request.contains(r#""SupportedCommands":["Play","Playstate","SetVolume","ToggleMute","ToggleFullscreen","SetAudioStreamIndex","SetSubtitleStreamIndex"]"#));
    assert!(!request.contains("MoveUp"));
    assert!(!request.contains("PlayNext"));
    assert!(!request.contains("PlayMediaSource"));
  }

  #[tokio::test]
  async fn validate_session_rejects_current_device_without_media_control() {
    let client = JellyfinClient::new();
    let device_id = client.device_id();
    let body = format!(
      r#"[{{"DeviceId":"{}","DeviceName":"JellyPilot","Client":"JellyPilot","SupportsMediaControl":false,"SupportsRemoteControl":true}}]"#,
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
    let state = client.connection_state();
    assert!(state.capabilities.remote_control);
    assert!(!state.capabilities.remote_control_available);
    assert!(state.capabilities.remote_control_warning.is_some());
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
  async fn video_home_loads_real_rows_without_library_shortcuts() {
    let movie_id = "00000000-0000-0000-0000-000000000010";
    let episode_id = "00000000-0000-0000-0000-000000000011";
    let series_id = "00000000-0000-0000-0000-000000000012";
    let resume_episode_id = "00000000-0000-0000-0000-000000000013";
    let (server_url, requests) = serve_route_responses_with_requests(vec![
      (
        "/UserItems/Resume",
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000010","Name":"Resume Movie","Type":"Movie","ProductionYear":2024,"RunTimeTicks":72000000000,"ImageTags":{"Thumb":"thumb-1"},"UserData":{"PlaybackPositionTicks":1200000000,"PlayedPercentage":25.0,"IsFavorite":true,"Played":false}},{"Id":"00000000-0000-0000-0000-000000000013","Name":"Resume Episode","Type":"Episode","SeriesName":"Example Show","SeriesId":"00000000-0000-0000-0000-000000000012","ParentIndexNumber":1,"IndexNumber":1,"ImageTags":{"Primary":"episode-primary"},"UserData":{"PlaybackPositionTicks":600000000,"PlayedPercentage":10.0,"IsFavorite":false,"Played":false}}],"TotalRecordCount":2}"#,
      ),
      (
        "/Shows/NextUp",
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000011","Name":"Next Episode","Type":"Episode","SeriesName":"Example Show","SeriesId":"00000000-0000-0000-0000-000000000012","ParentIndexNumber":1,"IndexNumber":2,"ImageTags":{"Primary":"poster-2"},"UserData":{"PlaybackPositionTicks":0,"PlayedPercentage":0.0,"IsFavorite":false,"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "includeItemTypes=Movie",
        "200 OK",
        r#"[{"Id":"00000000-0000-0000-0000-000000000010","Name":"Latest Movie","Type":"Movie","ImageTags":{"Primary":"poster-3"}}]"#,
      ),
      (
        "includeItemTypes=Episode",
        "200 OK",
        r#"[{"Id":"00000000-0000-0000-0000-000000000011","Name":"Latest Episode","Type":"Episode","SeriesName":"Example Show","ParentIndexNumber":1,"IndexNumber":3}]"#,
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
    let expected_artwork = format!("{server_url}/Items/{movie_id}/Images/Thumb?tag=thumb-1");
    assert_eq!(
      home.continue_watching[0].artwork_url.as_deref(),
      Some(expected_artwork.as_str())
    );
    let expected_episode_artwork =
      format!("{server_url}/Items/{resume_episode_id}/Images/Primary?tag=episode-primary");
    assert_eq!(
      home.continue_watching[1].artwork_url.as_deref(),
      Some(expected_episode_artwork.as_str())
    );
    assert_eq!(home.next_up[0].id, episode_id);
    assert_eq!(home.next_up[0].series_id.as_deref(), Some(series_id));
    assert_eq!(home.latest_movies[0].name, "Latest Movie");
    assert_eq!(home.latest_episodes[0].name, "Latest Episode");

    let captured = requests.lock();
    let resume_request = captured
      .iter()
      .find(|request| request.starts_with("GET /UserItems/Resume?"))
      .expect("resume request should be captured");
    assert!(resume_request.contains("enableImageTypes=Thumb"));
    assert!(resume_request.contains("enableImageTypes=Primary"));
    assert!(resume_request.contains("includeItemTypes=Movie"));
    assert!(resume_request.contains("includeItemTypes=Episode"));
    assert!(captured
      .iter()
      .any(|request| request.starts_with("GET /Shows/NextUp?")));
    assert!(captured
      .iter()
      .any(|request| request.starts_with("GET /Items/Latest?")
        && request.contains("includeItemTypes=Movie")));
    assert!(captured
      .iter()
      .any(|request| request.starts_with("GET /Items/Latest?")
        && request.contains("includeItemTypes=Episode")));
    assert!(!captured
      .iter()
      .any(|request| request.starts_with("GET /UserViews?")));
  }

  #[tokio::test]
  async fn library_shortcuts_loads_movies_and_shows_only() {
    let movie_library_id = "00000000-0000-0000-0000-000000000020";
    let shows_library_id = "00000000-0000-0000-0000-000000000021";
    let (server_url, requests) = serve_responses_with_requests(vec![(
      "200 OK",
      r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000020","Name":"Movies","Type":"CollectionFolder","CollectionType":"movies","RecursiveItemCount":8},{"Id":"00000000-0000-0000-0000-000000000021","Name":"Shows","Type":"CollectionFolder","CollectionType":"tvshows","RecursiveItemCount":5},{"Id":"00000000-0000-0000-0000-000000000022","Name":"Music","Type":"CollectionFolder","CollectionType":"music","RecursiveItemCount":99}],"TotalRecordCount":3}"#,
    )])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url);

    let shortcuts = client
      .library()
      .library_shortcuts()
      .await
      .expect("library shortcuts should load from user views");

    assert_eq!(
      shortcuts
        .iter()
        .map(|library| (library.id.as_str(), library.collection_type.as_str()))
        .collect::<Vec<_>>(),
      vec![(movie_library_id, "movies"), (shows_library_id, "tvshows")]
    );

    let captured = requests.lock();
    let shortcut_request = captured
      .iter()
      .find(|request| request.starts_with("GET /UserViews?"))
      .expect("library shortcuts request should be captured");
    assert!(shortcut_request.contains("presetViews=movies"));
    assert!(shortcut_request.contains("presetViews=tvshows"));
  }

  #[tokio::test]
  async fn browse_video_maps_movies_and_shows_to_paged_library_queries() {
    let movie_library_id = "00000000-0000-0000-0000-000000000020";
    let shows_library_id = "00000000-0000-0000-0000-000000000021";
    let movie_id = "00000000-0000-0000-0000-000000000030";
    let show_id = "00000000-0000-0000-0000-000000000031";
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000030","Name":"Paged Movie","Type":"Movie","ProductionYear":2025,"RunTimeTicks":54000000000,"ImageTags":{"Primary":"poster-movie"},"UserData":{"IsFavorite":true,"Played":false}}],"TotalRecordCount":24,"StartIndex":20}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000031","Name":"Paged Show","Type":"Series","ImageTags":{"Primary":"poster-show"},"UserData":{"IsFavorite":false,"Played":true}}],"TotalRecordCount":1,"StartIndex":0}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000030","Name":"Filtered Movie","Type":"Movie","UserData":{"IsFavorite":true,"Played":true}}],"TotalRecordCount":1,"StartIndex":0}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url.clone());

    let movies = client
      .library()
      .browse_video(VideoLibraryPageRequest {
        library_id: movie_library_id.to_string(),
        collection_type: VideoLibraryKind::Movies,
        start_index: 20,
        limit: 2,
        sort: VideoLibrarySort::Title,
        played_filter: VideoLibraryPlayedFilter::All,
        favorites_only: false,
      })
      .await
      .expect("movies page should load from generated item listing endpoint");
    let shows = client
      .library()
      .browse_video(VideoLibraryPageRequest {
        library_id: shows_library_id.to_string(),
        collection_type: VideoLibraryKind::TvShows,
        start_index: -4,
        limit: 500,
        sort: VideoLibrarySort::RecentlyAdded,
        played_filter: VideoLibraryPlayedFilter::All,
        favorites_only: false,
      })
      .await
      .expect("shows page should load from generated item listing endpoint");
    let filtered_movies = client
      .library()
      .browse_video(VideoLibraryPageRequest {
        library_id: movie_library_id.to_string(),
        collection_type: VideoLibraryKind::Movies,
        start_index: 0,
        limit: 24,
        sort: VideoLibrarySort::ReleaseDate,
        played_filter: VideoLibraryPlayedFilter::Played,
        favorites_only: true,
      })
      .await
      .expect("filtered movies page should load from generated item listing endpoint");

    assert_eq!(movies.start_index, 20);
    assert_eq!(movies.limit, 2);
    assert_eq!(movies.total_record_count, 24);
    assert!(movies.has_more);
    assert_eq!(movies.items[0].id, movie_id);
    assert_eq!(movies.items[0].name, "Paged Movie");
    assert_eq!(movies.items[0].runtime_seconds, Some(5400.0));
    let expected_movie_artwork =
      format!("{server_url}/Items/{movie_id}/Images/Primary?tag=poster-movie");
    assert_eq!(
      movies.items[0].artwork_url.as_deref(),
      Some(expected_movie_artwork.as_str())
    );

    assert_eq!(shows.start_index, 0);
    assert_eq!(shows.limit, 100);
    assert_eq!(shows.total_record_count, 1);
    assert!(!shows.has_more);
    assert_eq!(shows.items[0].id, show_id);
    assert_eq!(shows.items[0].item_type, "Series");
    assert_eq!(filtered_movies.items[0].name, "Filtered Movie");

    let captured = requests.lock();
    assert!(captured[0].starts_with("GET /Items?"));
    assert!(captured[0].contains("parentId=00000000-0000-0000-0000-000000000020"));
    assert!(captured[0].contains("startIndex=20"));
    assert!(captured[0].contains("limit=2"));
    assert!(captured[0].contains("recursive=true"));
    assert!(captured[0].contains("includeItemTypes=Movie"));
    assert!(captured[0].contains("mediaTypes=Video"));
    assert!(captured[0].contains("sortBy=SortName"));
    assert!(captured[0].contains("sortOrder=Ascending"));
    assert!(captured[0].contains("enableTotalRecordCount=true"));
    assert!(captured[1].starts_with("GET /Items?"));
    assert!(captured[1].contains("parentId=00000000-0000-0000-0000-000000000021"));
    assert!(captured[1].contains("startIndex=0"));
    assert!(captured[1].contains("limit=100"));
    assert!(captured[1].contains("includeItemTypes=Series"));
    assert!(captured[1].contains("sortBy=DateCreated"));
    assert!(captured[1].contains("sortOrder=Descending"));
    assert!(!captured[1].contains("mediaTypes=Video"));
    assert!(captured[2].contains("sortBy=PremiereDate"));
    assert!(captured[2].contains("sortOrder=Descending"));
    assert!(captured[2].contains("isPlayed=true"));
    assert!(captured[2].contains("isFavorite=true"));
  }

  #[tokio::test]
  async fn browse_video_rejects_missing_library_id() {
    let client = JellyfinClient::new();
    connect_test_client(&client, "http://127.0.0.1:8096".to_string());

    let err = client
      .library()
      .browse_video(VideoLibraryPageRequest {
        library_id: "  ".to_string(),
        collection_type: VideoLibraryKind::Movies,
        start_index: 0,
        limit: 24,
        sort: VideoLibrarySort::Title,
        played_filter: VideoLibraryPlayedFilter::All,
        favorites_only: false,
      })
      .await
      .expect_err("missing library id should return a clear command error");

    assert_eq!(
      err.to_string(),
      "HTTP error: Library id is required for video browsing"
    );
  }

  #[tokio::test]
  async fn search_video_maps_query_to_video_only_paged_items() {
    let movie_id = "00000000-0000-0000-0000-000000000040";
    let show_id = "00000000-0000-0000-0000-000000000041";
    let episode_id = "00000000-0000-0000-0000-000000000042";
    let (server_url, requests) = serve_responses_with_requests(vec![(
      "200 OK",
      r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000040","Name":"Search Movie","Type":"Movie","ImageTags":{"Primary":"poster-search"}},{"Id":"00000000-0000-0000-0000-000000000041","Name":"Search Show","Type":"Series"},{"Id":"00000000-0000-0000-0000-000000000042","Name":"Search Episode","Type":"Episode","UserData":{"Played":false}}],"TotalRecordCount":5,"StartIndex":0}"#,
    )])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url.clone());

    let page = client
      .library()
      .search_video(VideoSearchRequest {
        query: " search text ".to_string(),
        start_index: 0,
        limit: 3,
      })
      .await
      .expect("video search should load from generated item listing endpoint");

    assert_eq!(page.query, "search text");
    assert_eq!(page.start_index, 0);
    assert_eq!(page.limit, 3);
    assert_eq!(page.total_record_count, 5);
    assert!(page.has_more);
    assert_eq!(
      page
        .items
        .iter()
        .map(|item| (item.id.as_str(), item.item_type.as_str()))
        .collect::<Vec<_>>(),
      vec![
        (movie_id, "Movie"),
        (show_id, "Series"),
        (episode_id, "Episode")
      ]
    );

    let captured = requests.lock();
    assert!(captured[0].starts_with("GET /Items?"));
    assert!(captured[0].contains("searchTerm=search+text"));
    assert!(captured[0].contains("startIndex=0"));
    assert!(captured[0].contains("limit=3"));
    assert!(captured[0].contains("includeItemTypes=Movie"));
    assert!(captured[0].contains("includeItemTypes=Series"));
    assert!(captured[0].contains("includeItemTypes=Episode"));
    assert!(!captured[0].contains("includeItemTypes=Audio"));
    assert!(!captured[0].contains("mediaTypes=Audio"));
    assert!(!captured[0].contains("parentId="));
  }

  #[tokio::test]
  async fn search_video_rejects_empty_query() {
    let client = JellyfinClient::new();
    connect_test_client(&client, "http://127.0.0.1:8096".to_string());

    let err = client
      .library()
      .search_video(VideoSearchRequest {
        query: "  ".to_string(),
        start_index: 0,
        limit: 24,
      })
      .await
      .expect_err("empty search should return a clear command error");

    assert_eq!(
      err.to_string(),
      "HTTP error: Search text is required for video search"
    );
  }

  #[tokio::test]
  async fn item_detail_maps_movie_and_episode_metadata() {
    let movie_id = "00000000-0000-0000-0000-000000000050";
    let episode_id = "00000000-0000-0000-0000-000000000051";
    let series_id = "00000000-0000-0000-0000-000000000052";
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000050","Name":"Detail Movie","Type":"Movie","Overview":"A movie overview.","ProductionYear":2024,"RunTimeTicks":72000000000,"Genres":["Drama","Mystery"],"ImageTags":{"Primary":"poster-detail"},"UserData":{"PlaybackPositionTicks":1200000000,"PlayedPercentage":25.0,"IsFavorite":true,"Played":false},"MediaStreams":[{"Index":0,"Type":"Video","Codec":"h264"},{"Index":1,"Type":"Audio","Language":"eng","DisplayTitle":"English - AAC 2.0","Codec":"aac","IsDefault":true},{"Index":2,"Type":"Audio","Language":"jpn","Codec":"flac"},{"Index":3,"Type":"Subtitle","Language":"eng","DisplayTitle":"English - SRT","Codec":"srt","IsExternal":true}]}"#,
      ),
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000051","Name":"Detail Episode","Type":"Episode","SeriesId":"00000000-0000-0000-0000-000000000052","SeriesName":"Example Show","ParentIndexNumber":2,"IndexNumber":3,"Genres":["Sci-Fi"],"UserData":{"PlaybackPositionTicks":0,"PlayedPercentage":0.0,"IsFavorite":false,"Played":true}}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url.clone());

    let movie = client
      .library()
      .item_detail(movie_id.to_string())
      .await
      .expect("movie detail should load from generated item endpoint");
    let episode = client
      .library()
      .item_detail(episode_id.to_string())
      .await
      .expect("episode detail should load from generated item endpoint");

    assert_eq!(movie.name, "Detail Movie");
    assert_eq!(movie.item_type, "Movie");
    assert_eq!(movie.overview.as_deref(), Some("A movie overview."));
    assert_eq!(movie.runtime_seconds, Some(7200.0));
    assert_eq!(movie.genres, vec!["Drama", "Mystery"]);
    assert_eq!(movie.resume_position_seconds, Some(120.0));
    assert!(movie.can_resume);
    assert!(movie.can_play);
    assert!(movie.favorite);
    let expected_artwork =
      format!("{server_url}/Items/{movie_id}/Images/Primary?tag=poster-detail");
    assert_eq!(
      movie.artwork_url.as_deref(),
      Some(expected_artwork.as_str())
    );
    assert_eq!(movie.audio_streams.len(), 2);
    assert_eq!(movie.audio_streams[0].index, 1);
    assert_eq!(movie.audio_streams[0].label, "English - AAC 2.0");
    assert_eq!(movie.audio_streams[0].language.as_deref(), Some("eng"));
    assert!(movie.audio_streams[0].is_default);
    assert_eq!(movie.audio_streams[1].label, "jpn · flac");
    assert_eq!(movie.subtitle_streams.len(), 1);
    assert_eq!(movie.subtitle_streams[0].index, 3);
    assert_eq!(movie.subtitle_streams[0].label, "English - SRT");
    assert!(movie.subtitle_streams[0].is_external);

    assert_eq!(episode.item_type, "Episode");
    assert_eq!(episode.series_id.as_deref(), Some(series_id));
    assert_eq!(episode.series_name.as_deref(), Some("Example Show"));
    assert_eq!(episode.season_number, Some(2));
    assert_eq!(episode.episode_number, Some(3));
    assert!(episode.played);
    assert!(!episode.can_resume);
    assert_eq!(episode.artwork_url, None);

    let captured = requests.lock();
    assert!(captured[0].starts_with("GET /Items/00000000-0000-0000-0000-000000000050?"));
    assert!(captured[0].contains("userId=00000000-0000-0000-0000-000000000001"));
    assert!(captured[0].contains("fields=MediaStreams"));
    assert!(captured[1].starts_with("GET /Items/00000000-0000-0000-0000-000000000051?"));
    assert!(captured[1].contains("userId=00000000-0000-0000-0000-000000000001"));
    assert!(captured[1].contains("fields=MediaStreams"));
  }

  #[tokio::test]
  async fn item_detail_rejects_unsupported_item_kinds() {
    let (server_url, _) = serve_responses_with_requests(vec![(
      "200 OK",
      r#"{"Id":"00000000-0000-0000-0000-000000000053","Name":"A Show","Type":"Series"}"#,
    )])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url);

    let err = client
      .library()
      .item_detail("00000000-0000-0000-0000-000000000053".to_string())
      .await
      .expect_err("unsupported item kind should return a clear command error");

    assert_eq!(
      err.to_string(),
      "HTTP error: Only Movie and Episode details are supported by the Library Browser"
    );
  }

  #[tokio::test]
  async fn show_detail_loads_show_seasons_and_next_playable_episode() {
    let series_id = "00000000-0000-0000-0000-000000000060";
    let season_id = "00000000-0000-0000-0000-000000000061";
    let next_episode_id = "00000000-0000-0000-0000-000000000062";
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000060","Name":"Example Show","Type":"Series","Overview":"A show overview.","ProductionYear":2023,"Genres":["Drama"],"ImageTags":{"Primary":"poster-show"},"UserData":{"IsFavorite":true,"Played":false}}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000061","Name":"Season 1","Type":"Season","IndexNumber":1,"ImageTags":{"Primary":"poster-season"},"UserData":{"IsFavorite":false,"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000062","Name":"Next Episode","Type":"Episode","ProductionYear":2023,"UserData":{"PlaybackPositionTicks":300000000,"Played":false}}],"TotalRecordCount":1}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url.clone());

    let detail = client
      .library()
      .show_detail(series_id.to_string())
      .await
      .expect("show detail should load series metadata, seasons, and next episode");

    assert_eq!(detail.id, series_id);
    assert_eq!(detail.name, "Example Show");
    assert_eq!(detail.overview.as_deref(), Some("A show overview."));
    assert_eq!(detail.genres, vec!["Drama"]);
    assert!(detail.favorite);
    assert!(detail.can_play);
    assert_eq!(detail.seasons.len(), 1);
    assert_eq!(detail.seasons[0].id, season_id);
    assert_eq!(detail.seasons[0].season_number, Some(1));
    assert_eq!(
      detail
        .next_episode
        .as_ref()
        .map(|episode| episode.id.as_str()),
      Some(next_episode_id)
    );
    let expected_artwork = format!("{server_url}/Items/{series_id}/Images/Primary?tag=poster-show");
    assert_eq!(
      detail.artwork_url.as_deref(),
      Some(expected_artwork.as_str())
    );

    let captured = requests.lock();
    assert!(captured[0].starts_with("GET /Items/00000000-0000-0000-0000-000000000060?"));
    assert!(captured[0].contains("userId=00000000-0000-0000-0000-000000000001"));
    assert!(captured[1].starts_with("GET /Shows/00000000-0000-0000-0000-000000000060/Seasons?"));
    assert!(captured[1].contains("enableUserData=true"));
    assert!(captured[1].contains("isMissing=false"));
    assert!(captured[2].starts_with("GET /Shows/NextUp?"));
    assert!(captured[2].contains("seriesId=00000000-0000-0000-0000-000000000060"));
    assert!(captured[2].contains("limit=1"));
    assert!(captured[2].contains("enableResumable=true"));
    assert!(captured[2].contains("enableRewatching=false"));
  }

  #[tokio::test]
  async fn season_episodes_loads_exact_season_episode_cards() {
    let series_id = "00000000-0000-0000-0000-000000000070";
    let season_id = "00000000-0000-0000-0000-000000000071";
    let episode_id = "00000000-0000-0000-0000-000000000072";
    let (server_url, requests) = serve_responses_with_requests(vec![(
      "200 OK",
      r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000072","Name":"Exact Episode","Type":"Episode","RunTimeTicks":18000000000,"ImageTags":{"Primary":"poster-episode"},"UserData":{"IsFavorite":false,"Played":false}}],"TotalRecordCount":1}"#,
    )])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url.clone());

    let page = client
      .library()
      .season_episodes(VideoSeasonEpisodesRequest {
        series_id: series_id.to_string(),
        season_id: Some(season_id.to_string()),
        season_number: Some(1),
      })
      .await
      .expect("season episodes should load exact season listing");

    assert_eq!(page.series_id, series_id);
    assert_eq!(page.season_id.as_deref(), Some(season_id));
    assert_eq!(page.season_number, Some(1));
    assert_eq!(page.episodes.len(), 1);
    assert_eq!(page.episodes[0].id, episode_id);
    assert_eq!(page.episodes[0].name, "Exact Episode");
    assert_eq!(page.episodes[0].runtime_seconds, Some(1800.0));
    let expected_artwork =
      format!("{server_url}/Items/{episode_id}/Images/Primary?tag=poster-episode");
    assert_eq!(
      page.episodes[0].artwork_url.as_deref(),
      Some(expected_artwork.as_str())
    );

    let captured = requests.lock();
    assert!(captured[0].starts_with("GET /Shows/00000000-0000-0000-0000-000000000070/Episodes?"));
    assert!(captured[0].contains("season=1"));
    assert!(captured[0].contains("seasonId=00000000-0000-0000-0000-000000000071"));
    assert!(captured[0].contains("enableUserData=true"));
    assert!(captured[0].contains("sortBy=ParentIndexNumber%2CIndexNumber"));
  }

  #[tokio::test]
  async fn update_user_data_maps_library_actions_to_jellyfin_userdata_endpoints() {
    let item_id = "00000000-0000-0000-0000-000000000080";
    let (server_url, requests) = serve_responses_with_requests(vec![
      ("200 OK", r#"{"IsFavorite":true,"Played":false}"#),
      ("200 OK", r#"{"IsFavorite":false,"Played":false}"#),
      ("200 OK", r#"{"IsFavorite":false,"Played":true}"#),
      ("200 OK", r#"{"IsFavorite":false,"Played":false}"#),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client(&client, server_url);

    let favorite = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::Favorite,
      })
      .await
      .expect("favorite should update user data");
    let unfavorite = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::Unfavorite,
      })
      .await
      .expect("unfavorite should update user data");
    let played = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::MarkPlayed,
      })
      .await
      .expect("mark played should update user data");
    let unplayed = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::MarkUnplayed,
      })
      .await
      .expect("mark unplayed should update user data");

    assert!(favorite.favorite);
    assert!(!unfavorite.favorite);
    assert!(played.played);
    assert!(!unplayed.played);

    let captured = requests.lock();
    assert!(
      captured[0].starts_with("POST /UserFavoriteItems/00000000-0000-0000-0000-000000000080?")
    );
    assert!(
      captured[1].starts_with("DELETE /UserFavoriteItems/00000000-0000-0000-0000-000000000080?")
    );
    assert!(captured[2].starts_with("POST /UserPlayedItems/00000000-0000-0000-0000-000000000080?"));
    assert!(
      captured[3].starts_with("DELETE /UserPlayedItems/00000000-0000-0000-0000-000000000080?")
    );
  }

  #[tokio::test]
  async fn emby_video_home_and_shortcuts_load_shared_browser_rows() {
    let movie_library_id = "00000000-0000-0000-0000-000000000220";
    let shows_library_id = "00000000-0000-0000-0000-000000000221";
    let movie_id = "00000000-0000-0000-0000-000000000210";
    let episode_id = "00000000-0000-0000-0000-000000000211";
    let (server_url, requests) = serve_route_responses_with_requests(vec![
      (
        "/Users/00000000-0000-0000-0000-000000000001/Items/Resume",
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000210","Name":"Emby Resume Movie","Type":"Movie","ImageTags":{"Thumb":"thumb-emby"},"UserData":{"PlaybackPositionTicks":1500000000,"PlayedPercentage":20.0,"IsFavorite":true,"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "/Shows/NextUp",
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000211","Name":"Emby Next Episode","Type":"Episode","SeriesId":"00000000-0000-0000-0000-000000000212","SeriesName":"Emby Show","ParentIndexNumber":1,"IndexNumber":2,"ImageTags":{"Primary":"next-primary"},"UserData":{"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "IncludeItemTypes=Movie",
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000210","Name":"Latest Emby Movie","Type":"Movie","ImageTags":{"Primary":"latest-movie"}}],"TotalRecordCount":1}"#,
      ),
      (
        "IncludeItemTypes=Episode",
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000211","Name":"Latest Emby Episode","Type":"Episode","SeriesName":"Emby Show"}],"TotalRecordCount":1}"#,
      ),
      (
        "/Users/00000000-0000-0000-0000-000000000001/Views",
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000220","Name":"Emby Movies","Type":"CollectionFolder","CollectionType":"movies","RecursiveItemCount":4,"ImageTags":{"Primary":"movies-primary"}},{"Id":"00000000-0000-0000-0000-000000000221","Name":"Emby Shows","Type":"CollectionFolder","CollectionType":"tvshows","RecursiveItemCount":7},{"Id":"00000000-0000-0000-0000-000000000222","Name":"Music","Type":"CollectionFolder","CollectionType":"music"}],"TotalRecordCount":3}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();
    let emby_base = format!("{server_url}/emby");
    connect_test_client_as_emby(&client, emby_base.clone());

    let home = client
      .library()
      .video_home()
      .await
      .expect("Emby video home should load through shared DTOs");
    let shortcuts = client
      .library()
      .library_shortcuts()
      .await
      .expect("Emby library shortcuts should load from user views");

    assert_eq!(home.continue_watching[0].id, movie_id);
    assert_eq!(home.continue_watching[0].name, "Emby Resume Movie");
    assert_eq!(
      home.continue_watching[0].resume_position_seconds,
      Some(150.0)
    );
    assert_eq!(
      home.continue_watching[0].artwork_url.as_deref(),
      Some(format!("{emby_base}/Items/{movie_id}/Images/Thumb?tag=thumb-emby").as_str())
    );
    assert_eq!(home.next_up[0].id, episode_id);
    assert_eq!(home.latest_movies[0].name, "Latest Emby Movie");
    assert_eq!(home.latest_episodes[0].name, "Latest Emby Episode");
    assert_eq!(
      shortcuts
        .iter()
        .map(|library| (library.id.as_str(), library.collection_type.as_str()))
        .collect::<Vec<_>>(),
      vec![(movie_library_id, "movies"), (shows_library_id, "tvshows")]
    );

    let captured = requests.lock();
    assert!(captured.iter().any(|request| request
      .starts_with("GET /emby/Users/00000000-0000-0000-0000-000000000001/Items/Resume?")));
    assert!(captured
      .iter()
      .any(|request| request
        .starts_with("GET /emby/Users/00000000-0000-0000-0000-000000000001/Views?")));
  }

  #[tokio::test]
  async fn emby_browse_and_search_video_map_to_shared_library_pages() {
    let movie_library_id = "00000000-0000-0000-0000-000000000220";
    let movie_id = "00000000-0000-0000-0000-000000000230";
    let show_id = "00000000-0000-0000-0000-000000000231";
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000230","Name":"Emby Paged Movie","Type":"Movie","ProductionYear":2026,"RunTimeTicks":54000000000,"ImageTags":{"Primary":"movie-primary"},"UserData":{"IsFavorite":true,"Played":false}}],"TotalRecordCount":24}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000231","Name":"Emby Search Show","Type":"Series"},{"Id":"00000000-0000-0000-0000-000000000230","Name":"Emby Search Movie","Type":"Movie","UserData":{"Played":false}}],"TotalRecordCount":2}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client_as_emby(&client, server_url.clone());

    let movies = client
      .library()
      .browse_video(VideoLibraryPageRequest {
        library_id: movie_library_id.to_string(),
        collection_type: VideoLibraryKind::Movies,
        start_index: 20,
        limit: 2,
        sort: VideoLibrarySort::ReleaseDate,
        played_filter: VideoLibraryPlayedFilter::Unplayed,
        favorites_only: true,
      })
      .await
      .expect("Emby movies browse should map item pages");
    let search = client
      .library()
      .search_video(VideoSearchRequest {
        query: " emby show ".to_string(),
        start_index: 0,
        limit: 10,
      })
      .await
      .expect("Emby video search should map item pages");

    assert_eq!(movies.items[0].id, movie_id);
    assert_eq!(movies.items[0].runtime_seconds, Some(5400.0));
    assert!(movies.items[0].favorite);
    assert!(movies.has_more);
    assert_eq!(
      search
        .items
        .iter()
        .map(|item| (item.id.as_str(), item.item_type.as_str()))
        .collect::<Vec<_>>(),
      vec![(show_id, "Series"), (movie_id, "Movie")]
    );

    let captured = requests.lock();
    assert!(captured[0].starts_with("GET /Users/00000000-0000-0000-0000-000000000001/Items?"));
    assert_chrome_jellypilot_user_agent(&captured[0]);
    assert!(captured[0].contains("ParentId=00000000-0000-0000-0000-000000000220"));
    assert!(captured[0].contains("IncludeItemTypes=Movie"));
    assert!(captured[0].contains("MediaTypes=Video"));
    assert!(captured[0].contains("SortBy=PremiereDate"));
    assert!(captured[0].contains("SortOrder=Descending"));
    assert!(captured[0].contains("IsPlayed=false"));
    assert!(captured[0].contains("IsFavorite=true"));
    assert!(captured[1].contains("SearchTerm=emby+show"));
    assert!(captured[1].contains("IncludeItemTypes=Movie%2CSeries%2CEpisode"));
  }

  #[tokio::test]
  async fn emby_details_show_and_episodes_tolerate_missing_optional_fields() {
    let movie_id = "00000000-0000-0000-0000-000000000250";
    let series_id = "00000000-0000-0000-0000-000000000260";
    let season_id = "00000000-0000-0000-0000-000000000261";
    let episode_id = "00000000-0000-0000-0000-000000000262";
    let (server_url, requests) = serve_responses_with_requests(vec![
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000250","Name":"Emby Detail Movie","Type":"Movie","RunTimeTicks":72000000000,"UserData":{"PlaybackPositionTicks":600000000,"Played":false},"MediaStreams":[{"Index":1,"Type":"Audio","Language":"eng","Codec":"aac","IsDefault":true},{"Index":2,"Type":"Subtitle","Codec":"srt","IsExternal":true}]}"#,
      ),
      (
        "200 OK",
        r#"{"Id":"00000000-0000-0000-0000-000000000260","Name":"Emby Show","Type":"Series","ImageTags":{"Primary":"show-primary"},"UserData":{"IsFavorite":true}}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000261","Name":"Season 1","Type":"Season","IndexNumber":1}],"TotalRecordCount":1}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000262","Name":"Next Emby Episode","Type":"Episode","UserData":{"PlaybackPositionTicks":300000000,"Played":false}}],"TotalRecordCount":1}"#,
      ),
      (
        "200 OK",
        r#"{"Items":[{"Id":"00000000-0000-0000-0000-000000000262","Name":"Episode One","Type":"Episode","RunTimeTicks":18000000000,"ParentIndexNumber":1,"IndexNumber":1}],"TotalRecordCount":1}"#,
      ),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client_as_emby(&client, server_url.clone());

    let movie = client
      .library()
      .item_detail(movie_id.to_string())
      .await
      .expect("Emby movie detail should map playable metadata");
    let show = client
      .library()
      .show_detail(series_id.to_string())
      .await
      .expect("Emby show detail should map seasons and next episode");
    let episodes = client
      .library()
      .season_episodes(VideoSeasonEpisodesRequest {
        series_id: series_id.to_string(),
        season_id: Some(season_id.to_string()),
        season_number: Some(1),
      })
      .await
      .expect("Emby season episodes should map episode rows");

    assert_eq!(movie.name, "Emby Detail Movie");
    assert_eq!(movie.resume_position_seconds, Some(60.0));
    assert!(movie.can_resume);
    assert_eq!(movie.audio_streams[0].label, "eng · aac");
    assert_eq!(movie.subtitle_streams[0].label, "srt");
    assert_eq!(show.id, series_id);
    assert!(show.favorite);
    assert!(show.can_play);
    assert_eq!(show.seasons[0].id, season_id);
    assert_eq!(
      show.next_episode.as_ref().map(|item| item.id.as_str()),
      Some(episode_id)
    );
    assert_eq!(episodes.episodes[0].id, episode_id);
    assert_eq!(episodes.episodes[0].season_number, Some(1));
    assert_eq!(episodes.episodes[0].episode_number, Some(1));

    let captured = requests.lock();
    assert!(captured[0].starts_with(
      "GET /Users/00000000-0000-0000-0000-000000000001/Items/00000000-0000-0000-0000-000000000250?"
    ));
    assert!(captured[0].contains("Fields=MediaStreams"));
    assert!(captured[2].contains("IncludeItemTypes=Season"));
    assert!(captured[3].starts_with("GET /Shows/NextUp?"));
    assert!(captured[3].contains("SeriesId=00000000-0000-0000-0000-000000000260"));
    assert!(captured[3].contains("EnableResumable=true"));
    assert!(captured[4].contains("ParentId=00000000-0000-0000-0000-000000000261"));
    assert!(captured[4].contains("IncludeItemTypes=Episode"));
  }

  #[tokio::test]
  async fn emby_update_user_data_maps_supported_library_actions() {
    let item_id = "00000000-0000-0000-0000-000000000280";
    let (server_url, requests) = serve_responses_with_requests(vec![
      ("200 OK", r#"{"IsFavorite":true,"Played":false}"#),
      ("200 OK", r#"{"IsFavorite":false,"Played":false}"#),
      ("200 OK", r#"{"IsFavorite":false,"Played":true}"#),
      ("200 OK", r#"{"IsFavorite":false,"Played":false}"#),
    ])
    .await;
    let client = JellyfinClient::new();
    connect_test_client_as_emby(&client, server_url);

    let favorite = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::Favorite,
      })
      .await
      .expect("Emby favorite should update user data");
    let unfavorite = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::Unfavorite,
      })
      .await
      .expect("Emby unfavorite should update user data");
    let played = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::MarkPlayed,
      })
      .await
      .expect("Emby mark played should update user data");
    let unplayed = client
      .library()
      .update_user_data(VideoUserDataUpdateRequest {
        item_id: item_id.to_string(),
        action: VideoUserDataAction::MarkUnplayed,
      })
      .await
      .expect("Emby mark unplayed should update user data");

    assert!(favorite.favorite);
    assert!(!unfavorite.favorite);
    assert!(played.played);
    assert!(!unplayed.played);

    let captured = requests.lock();
    assert!(captured[0].starts_with("POST /Users/00000000-0000-0000-0000-000000000001/FavoriteItems/00000000-0000-0000-0000-000000000280"));
    assert!(captured[1].starts_with("DELETE /Users/00000000-0000-0000-0000-000000000001/FavoriteItems/00000000-0000-0000-0000-000000000280"));
    assert!(captured[2].starts_with("POST /Users/00000000-0000-0000-0000-000000000001/PlayedItems/00000000-0000-0000-0000-000000000280"));
    assert!(captured[3].starts_with("DELETE /Users/00000000-0000-0000-0000-000000000001/PlayedItems/00000000-0000-0000-0000-000000000280"));
  }

  #[test]
  fn emby_stream_urls_prefer_direct_play_then_provider_fallbacks() {
    let client = JellyfinClient::new();
    connect_test_client_as_emby(&client, "http://media.example.test/emby".to_string());
    let direct_play = MediaSource {
      id: "source-1".to_string(),
      path: None,
      protocol: "Http".to_string(),
      container: Some("mkv".to_string()),
      run_time_ticks: None,
      media_streams: Vec::new(),
      supports_direct_play: true,
      supports_direct_stream: true,
      supports_transcoding: true,
      direct_stream_url: Some("/videos/direct-stream.mp4?MediaSourceId=source-1".to_string()),
      add_api_key_to_direct_stream_url: Some(true),
      transcoding_url: Some("/videos/transcoded.m3u8".to_string()),
    };
    let direct_stream = MediaSource {
      supports_direct_play: false,
      direct_stream_url: Some("/videos/direct-stream.mp4?MediaSourceId=source-1".to_string()),
      ..direct_play.clone()
    };
    let transcode = MediaSource {
      supports_direct_play: false,
      supports_direct_stream: false,
      direct_stream_url: None,
      ..direct_play.clone()
    };

    assert_eq!(
      client
        .build_stream_url("movie-1", &direct_play)
        .expect("direct play URL"),
      "http://media.example.test/emby/Videos/movie-1/stream.mkv?Static=true&MediaSourceId=source-1&api_key=emby-token"
    );
    assert_eq!(
      client
        .build_stream_url("movie-1", &direct_stream)
        .expect("direct stream URL"),
      "http://media.example.test/emby/videos/direct-stream.mp4?MediaSourceId=source-1&api_key=emby-token"
    );
    assert_eq!(
      client
        .build_stream_url("movie-1", &transcode)
        .expect("transcoding URL"),
      "http://media.example.test/emby/videos/transcoded.m3u8?api_key=emby-token"
    );
  }

  #[test]
  fn login_and_playback_interfaces_are_separate() {
    let client = JellyfinClient::new();

    crate::jellyfin::client_facade::assert_login_interface(&client);
    crate::jellyfin::client_facade::assert_playback_interface(&client);
  }
}
