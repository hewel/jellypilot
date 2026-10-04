//! Session-scoped outbound remote control through the generated provider APIs.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Weak};
use std::time::Duration;

use super::{ClientState, JellyfinClient, JellyfinError, MediaServerProvider};

/// Identity of one server session and its device, never a media item identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RemoteControlTargetKey {
  pub session_id: String,
  pub device_id: String,
}

/// Controls supported by the latest observed target state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RemoteControlCapabilities {
  pub can_pause: bool,
  pub can_resume: bool,
  pub can_stop: bool,
  pub can_seek: bool,
  pub can_set_volume: bool,
  pub can_play_now: bool,
}

/// Server-reported playback state; absent values remain unknown.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteControlNowPlaying {
  pub item_id: String,
  pub title: String,
  pub position_seconds: Option<f64>,
  pub duration_seconds: Option<f64>,
  pub paused: Option<bool>,
}

/// Token-free projection of a discovered remote session.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteControlTarget {
  pub key: RemoteControlTargetKey,
  pub device_name: String,
  pub client_name: String,
  pub user_name: Option<String>,
  pub now_playing: Option<RemoteControlNowPlaying>,
  pub volume: Option<u32>,
  pub capabilities: RemoteControlCapabilities,
}

/// Supported outbound commands. HTTP success acknowledges dispatch, not playback.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteControlRequest {
  Pause,
  Resume,
  Stop,
  Seek {
    position_ticks: i64,
  },
  SetVolume {
    volume: u8,
  },
  PlayNow {
    item_id: String,
    start_position_ticks: Option<i64>,
  },
}

/// An unforgeable target observation bound to its originating client session.
#[derive(Clone)]
pub struct RemoteSession {
  scope: Scope,
  view: RemoteControlTarget,
}

impl RemoteSession {
  /// Token-free display state observed during discovery.
  pub fn view(&self) -> &RemoteControlTarget {
    &self.view
  }
}

impl fmt::Debug for RemoteSession {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("RemoteSession").finish_non_exhaustive()
  }
}

/// Discovery and outbound control, separate from this client's receiver role.
pub struct JellyfinRemoteControl<'a> {
  client: &'a JellyfinClient,
}

#[derive(Clone)]
struct Scope {
  client: Weak<parking_lot::RwLock<ClientState>>,
  provider: MediaServerProvider,
  server_url: String,
  user_id: String,
  local_device_id: String,
  epoch: u64,
}

enum Configuration {
  Jellyfin(jellyfin_api::apis::configuration::Configuration),
  Emby(emby_api::apis::configuration::Configuration),
}

impl Scope {
  fn capture(client: &JellyfinClient) -> Result<(Self, Configuration), JellyfinError> {
    let state = client.state.read();
    let scope = Self {
      client: Arc::downgrade(&client.state),
      provider: state.provider,
      server_url: state
        .server_url
        .clone()
        .ok_or(JellyfinError::NotConnected)?,
      user_id: state.user_id.clone().ok_or(JellyfinError::NotConnected)?,
      local_device_id: state.device_id.clone(),
      epoch: state.authentication_epoch,
    };
    let configuration = scope.configuration(client, &state)?;
    Ok((scope, configuration))
  }

  fn validate(&self, client: &JellyfinClient, state: &ClientState) -> Result<(), JellyfinError> {
    if !self.client.ptr_eq(&Arc::downgrade(&client.state))
      || self.provider != state.provider
      || state.server_url.as_deref() != Some(self.server_url.as_str())
      || state.user_id.as_deref() != Some(self.user_id.as_str())
      || state.device_id != self.local_device_id
      || state.authentication_epoch != self.epoch
      || state.access_token.is_none()
    {
      return Err(JellyfinError::NotConnected);
    }
    Ok(())
  }

  fn configuration(
    &self,
    client: &JellyfinClient,
    state: &ClientState,
  ) -> Result<Configuration, JellyfinError> {
    self.validate(client, state)?;
    let authorization = JellyfinClient::auth_header_from_parts(
      &state.device_name,
      &state.device_id,
      state.access_token.as_deref(),
    );
    let policy = JellyfinClient::reject_redirect_policy();
    let timeout = Duration::from_secs(5);
    match self.provider {
      MediaServerProvider::Jellyfin => JellyfinClient::openapi_configuration_with_authorization(
        &self.server_url,
        &authorization,
        policy,
        timeout,
      )
      .map(Configuration::Jellyfin),
      MediaServerProvider::Emby => JellyfinClient::emby_openapi_configuration_with_authorization(
        &self.server_url,
        &authorization,
        policy,
        timeout,
      )
      .map(Configuration::Emby),
    }
    .map_err(|_| failed("Remote control configuration failed"))
  }
}

impl<'a> JellyfinRemoteControl<'a> {
  /// Checks provider identity before callers use it for detail lookup.
  pub fn normalize_item_id(&self, item_id: &str) -> Result<String, JellyfinError> {
    match self.client.provider() {
      MediaServerProvider::Jellyfin => jellyfin_item_id(item_id).map(|id| id.simple().to_string()),
      MediaServerProvider::Emby => emby_item_id(item_id).map(|id| id.to_string()),
    }
  }

  pub(super) fn new(client: &'a JellyfinClient) -> Self {
    Self { client }
  }

  /// Lists sessions the current user may control, excluding this device and
  /// explicitly inactive Jellyfin sessions. Unknown provider fields remain unknown.
  pub async fn sessions(&self) -> Result<Vec<RemoteSession>, JellyfinError> {
    let (scope, configuration) = Scope::capture(self.client)?;
    let result: Result<Vec<RemoteControlTarget>, JellyfinError> = match configuration {
      Configuration::Jellyfin(configuration) => jellyfin_api::apis::session_api::get_sessions(
        &configuration,
        jellyfin_api::apis::session_api::GetSessionsParams {
          controllable_by_user_id: Some(scope.user_id.clone()),
          device_id: None,
          active_within_seconds: None,
        },
      )
      .await
      .map(|sessions| {
        sessions
          .into_iter()
          .filter_map(|session| jellyfin_target(session, &scope.local_device_id))
          .collect()
      })
      .map_err(|error| {
        JellyfinClient::saved_session_openapi_error("Remote discovery", error, true)
      }),
      Configuration::Emby(configuration) => emby_api::apis::sessions_service_api::get_sessions(
        &configuration,
        emby_api::apis::sessions_service_api::GetSessionsParams {
          controllable_by_user_id: Some(scope.user_id.clone()),
          device_id: None,
          id: None,
        },
      )
      .await
      .map(|sessions| {
        sessions
          .into_iter()
          .filter_map(|session| emby_target(session, &scope.local_device_id))
          .collect()
      })
      .map_err(|error| {
        JellyfinClient::saved_session_emby_openapi_error("Remote discovery", error, true)
      }),
    };
    scope.validate(self.client, &self.client.state.read())?;
    Ok(
      result?
        .into_iter()
        .map(|view| RemoteSession {
          scope: scope.clone(),
          view,
        })
        .collect(),
    )
  }

  /// Sends a capability-checked command to the observed target. Callers refresh
  /// discovery before commands; no request is retried. Retired sessions reject
  /// both dispatch and completion, even when a server response was successful.
  pub async fn send(
    &self,
    target: &RemoteSession,
    request: RemoteControlRequest,
  ) -> Result<(), JellyfinError> {
    let configuration = target
      .scope
      .configuration(self.client, &self.client.state.read())?;
    validate_request(&target.view, &request)?;
    let result = match configuration {
      Configuration::Jellyfin(configuration) => {
        send_jellyfin(
          &configuration,
          &target.view.key.session_id,
          &target.scope.user_id,
          request,
        )
        .await
      }
      Configuration::Emby(configuration) => {
        send_emby(
          &configuration,
          &target.view.key.session_id,
          &target.scope.user_id,
          request,
        )
        .await
      }
    };
    target
      .scope
      .validate(self.client, &self.client.state.read())?;
    result
  }
}

fn target_key(
  session_id: Option<String>,
  device_id: Option<String>,
  local: &str,
) -> Option<RemoteControlTargetKey> {
  let session_id = session_id.filter(|id| !id.trim().is_empty())?;
  let device_id = device_id.filter(|id| !id.trim().is_empty() && id != local)?;
  Some(RemoteControlTargetKey {
    session_id,
    device_id,
  })
}

fn seconds(ticks: Option<i64>) -> Option<f64> {
  ticks
    .filter(|ticks| *ticks >= 0)
    .map(|ticks| ticks as f64 / 10_000_000.0)
}

fn volume(value: Option<i32>) -> Option<u32> {
  value
    .filter(|volume| (0..=100).contains(volume))
    .map(|volume| volume as u32)
}

fn capabilities(
  control: bool,
  playing: Option<&RemoteControlNowPlaying>,
  can_seek: bool,
  set_volume: bool,
  volume: Option<u32>,
  video: bool,
) -> RemoteControlCapabilities {
  RemoteControlCapabilities {
    can_pause: control && playing.is_some_and(|playing| playing.paused == Some(false)),
    can_resume: control && playing.is_some_and(|playing| playing.paused == Some(true)),
    can_stop: control && playing.is_some(),
    can_seek: control
      && can_seek
      && playing.is_some_and(|playing| {
        playing
          .duration_seconds
          .is_some_and(|duration| duration > 0.0)
      }),
    can_set_volume: control && set_volume && volume.is_some(),
    can_play_now: control && video,
  }
}

fn jellyfin_target(
  session: jellyfin_api::models::SessionInfoDto,
  local: &str,
) -> Option<RemoteControlTarget> {
  use jellyfin_api::models::{GeneralCommandType, MediaType};
  if session.is_active == Some(false) {
    return None;
  }
  let key = target_key(session.id.flatten(), session.device_id.flatten(), local)?;
  let state = session.play_state.flatten();
  let advertised = session.capabilities.flatten();
  let control = session.supports_remote_control == Some(true)
    && session.supports_media_control.or_else(|| {
      advertised
        .as_ref()
        .and_then(|capabilities| capabilities.supports_media_control)
    }) == Some(true);
  let commands = session.supported_commands.as_ref().or_else(|| {
    advertised
      .as_ref()
      .and_then(|capabilities| capabilities.supported_commands.as_ref())
  });
  let media_types = session.playable_media_types.as_ref().or_else(|| {
    advertised
      .as_ref()
      .and_then(|capabilities| capabilities.playable_media_types.as_ref())
  });
  let now_playing = session.now_playing_item.flatten().and_then(|item| {
    Some(RemoteControlNowPlaying {
      item_id: item.id?.simple().to_string(),
      title: item.name.flatten().unwrap_or_default(),
      position_seconds: seconds(
        state
          .as_ref()
          .and_then(|state| state.position_ticks.flatten()),
      ),
      duration_seconds: seconds(item.run_time_ticks.flatten()),
      paused: state.as_ref().and_then(|state| state.is_paused),
    })
  });
  let volume = volume(
    state
      .as_ref()
      .and_then(|state| state.volume_level.flatten()),
  );
  let capabilities = capabilities(
    control,
    now_playing.as_ref(),
    state.as_ref().and_then(|state| state.can_seek) == Some(true),
    commands.is_some_and(|commands| commands.contains(&GeneralCommandType::SetVolume)),
    volume,
    media_types.is_some_and(|types| types.contains(&MediaType::Video)),
  );
  Some(RemoteControlTarget {
    key,
    device_name: session.device_name.flatten().unwrap_or_default(),
    client_name: session.client.flatten().unwrap_or_default(),
    user_name: session.user_name.flatten(),
    now_playing,
    volume,
    capabilities,
  })
}

fn emby_target(
  session: emby_api::models::SessionSessionInfo,
  local: &str,
) -> Option<RemoteControlTarget> {
  let key = target_key(session.id, session.device_id, local)?;
  let state = session.play_state;
  let now_playing = session.now_playing_item.and_then(|item| {
    Some(RemoteControlNowPlaying {
      item_id: item.id.filter(|id| !id.trim().is_empty())?,
      title: item.name.unwrap_or_default(),
      position_seconds: seconds(
        state
          .as_ref()
          .and_then(|state| state.position_ticks.flatten()),
      ),
      duration_seconds: seconds(item.run_time_ticks.flatten()),
      paused: state.as_ref().and_then(|state| state.is_paused),
    })
  });
  let volume = volume(
    state
      .as_ref()
      .and_then(|state| state.volume_level.flatten()),
  );
  let capabilities = capabilities(
    session.supports_remote_control == Some(true),
    now_playing.as_ref(),
    state.as_ref().and_then(|state| state.can_seek) == Some(true),
    session
      .supported_commands
      .as_ref()
      .is_some_and(|commands| commands.iter().any(|command| command == "SetVolume")),
    volume,
    session
      .playable_media_types
      .as_ref()
      .is_some_and(|types| types.iter().any(|kind| kind == "Video")),
  );
  Some(RemoteControlTarget {
    key,
    device_name: session.device_name.unwrap_or_default(),
    client_name: session.client.unwrap_or_default(),
    user_name: session.user_name,
    now_playing,
    volume,
    capabilities,
  })
}

fn validate_request(
  target: &RemoteControlTarget,
  request: &RemoteControlRequest,
) -> Result<(), JellyfinError> {
  let capabilities = &target.capabilities;
  let supported = match request {
    RemoteControlRequest::Pause => capabilities.can_pause,
    RemoteControlRequest::Resume => capabilities.can_resume,
    RemoteControlRequest::Stop => capabilities.can_stop,
    RemoteControlRequest::Seek { position_ticks } => {
      if *position_ticks < 0
        || target
          .now_playing
          .as_ref()
          .and_then(|playing| playing.duration_seconds)
          .is_none_or(|duration| *position_ticks as f64 / 10_000_000.0 > duration)
      {
        return Err(failed("Invalid remote seek position"));
      }
      capabilities.can_seek
    }
    RemoteControlRequest::SetVolume { volume } => {
      if *volume > 100 {
        return Err(failed("Invalid remote volume"));
      }
      capabilities.can_set_volume
    }
    RemoteControlRequest::PlayNow {
      item_id,
      start_position_ticks,
    } => {
      if item_id.trim().is_empty() || start_position_ticks.is_some_and(|ticks| ticks < 0) {
        return Err(failed("Invalid remote play request"));
      }
      capabilities.can_play_now
    }
  };
  if supported {
    Ok(())
  } else {
    Err(failed("Remote target does not support this command"))
  }
}

async fn send_jellyfin(
  configuration: &jellyfin_api::apis::configuration::Configuration,
  session_id: &str,
  user_id: &str,
  request: RemoteControlRequest,
) -> Result<(), JellyfinError> {
  use jellyfin_api::apis::session_api as api;
  let session_id = session_id.to_owned();
  match request {
    RemoteControlRequest::PlayNow {
      item_id,
      start_position_ticks,
    } => {
      let item_id = jellyfin_item_id(&item_id)?;
      api::play(
        configuration,
        api::PlayParams {
          session_id,
          play_command: "PlayNow".to_owned(),
          item_ids: vec![item_id],
          start_position_ticks,
          media_source_id: None,
          audio_stream_index: None,
          subtitle_stream_index: None,
          start_index: None,
        },
      )
      .await
      .map_err(|error| JellyfinClient::saved_session_openapi_error("Remote play", error, true))
    }
    RemoteControlRequest::SetVolume { volume } => api::send_full_general_command(
      configuration,
      api::SendFullGeneralCommandParams {
        session_id,
        general_command: jellyfin_api::models::GeneralCommand {
          name: Some(jellyfin_api::models::GeneralCommandType::SetVolume),
          controlling_user_id: None,
          arguments: Some(HashMap::from([("Volume".to_owned(), volume.to_string())])),
        },
      },
    )
    .await
    .map_err(|error| JellyfinClient::saved_session_openapi_error("Remote volume", error, true)),
    request => {
      let (command, seek_position_ticks) = playstate(&request)?;
      api::send_playstate_command(
        configuration,
        api::SendPlaystateCommandParams {
          session_id,
          command: command.to_owned(),
          seek_position_ticks,
          controlling_user_id: Some(user_id.to_owned()),
        },
      )
      .await
      .map_err(|error| JellyfinClient::saved_session_openapi_error("Remote playstate", error, true))
    }
  }
}

async fn send_emby(
  configuration: &emby_api::apis::configuration::Configuration,
  session_id: &str,
  user_id: &str,
  request: RemoteControlRequest,
) -> Result<(), JellyfinError> {
  use emby_api::apis::sessions_service_api as api;
  use emby_api::models::{PlayCommand, PlaystateCommand};
  let id = session_id.to_owned();
  match request {
    RemoteControlRequest::PlayNow {
      item_id,
      start_position_ticks,
    } => {
      let item_id = emby_item_id(&item_id)?;
      api::post_sessions_by_id_playing(
        configuration,
        api::PostSessionsByIdPlayingParams {
          id,
          item_ids: vec![item_id],
          play_command: PlayCommand::PlayNow,
          start_position_ticks,
          play_request: emby_api::models::PlayRequest {
            controlling_user_id: Some(user_id.to_owned()),
            ..Default::default()
          },
        },
      )
      .await
      .map_err(|error| JellyfinClient::saved_session_emby_openapi_error("Remote play", error, true))
    }
    RemoteControlRequest::SetVolume { volume } => api::post_sessions_by_id_command(
      configuration,
      api::PostSessionsByIdCommandParams {
        id,
        general_command: emby_api::models::GeneralCommand {
          name: Some("SetVolume".to_owned()),
          controlling_user_id: Some(user_id.to_owned()),
          arguments: Some(HashMap::from([("Volume".to_owned(), volume.to_string())])),
        },
      },
    )
    .await
    .map_err(|error| {
      JellyfinClient::saved_session_emby_openapi_error("Remote volume", error, true)
    }),
    request => {
      let (command, seek_position_ticks) = match request {
        RemoteControlRequest::Pause => (PlaystateCommand::Pause, None),
        RemoteControlRequest::Resume => (PlaystateCommand::Unpause, None),
        RemoteControlRequest::Stop => (PlaystateCommand::Stop, None),
        RemoteControlRequest::Seek { position_ticks } => {
          (PlaystateCommand::Seek, Some(position_ticks))
        }
        _ => return Err(failed("Invalid remote playstate command")),
      };
      api::post_sessions_by_id_playing_by_command(
        configuration,
        api::PostSessionsByIdPlayingByCommandParams {
          id,
          command,
          playstate_request: emby_api::models::PlaystateRequest {
            command: Some(command),
            seek_position_ticks: seek_position_ticks.map(Some),
            controlling_user_id: Some(user_id.to_owned()),
          },
        },
      )
      .await
      .map_err(|error| {
        JellyfinClient::saved_session_emby_openapi_error("Remote playstate", error, true)
      })
    }
  }
}

fn playstate(request: &RemoteControlRequest) -> Result<(&'static str, Option<i64>), JellyfinError> {
  match request {
    RemoteControlRequest::Pause => Ok(("Pause", None)),
    RemoteControlRequest::Resume => Ok(("Unpause", None)),
    RemoteControlRequest::Stop => Ok(("Stop", None)),
    RemoteControlRequest::Seek { position_ticks } => Ok(("Seek", Some(*position_ticks))),
    _ => Err(failed("Invalid remote playstate command")),
  }
}

fn failed(message: &str) -> JellyfinError {
  JellyfinError::HttpError(message.to_owned())
}

fn jellyfin_item_id(item_id: &str) -> Result<uuid::Uuid, JellyfinError> {
  uuid::Uuid::parse_str(item_id).map_err(|_| failed("Invalid Jellyfin item identity"))
}

fn emby_item_id(item_id: &str) -> Result<i64, JellyfinError> {
  item_id
    .parse::<i64>()
    .ok()
    .filter(|id| *id > 0)
    .ok_or_else(|| failed("Invalid Emby item identity"))
}

#[cfg(test)]
mod tests {
  use serde_json::{json, Value};
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::{TcpListener, TcpStream};

  use super::*;

  const USER: &str = "00000000000000000000000000000001";
  const JELLYFIN_ITEM: &str = "00000000000000000000000000000002";

  struct Server {
    listener: TcpListener,
    client: JellyfinClient,
    url: String,
    provider: MediaServerProvider,
  }

  impl Server {
    async fn new(provider: MediaServerProvider) -> Self {
      let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind server");
      let url = format!("http://{}/proxy", listener.local_addr().expect("address"));
      let client = JellyfinClient::with_storage_dir(std::env::temp_dir());
      {
        let mut state = client.state.write();
        state.provider = provider;
        state.server_url = Some(url.clone());
        state.user_id = Some(USER.to_owned());
        state.device_id = "local-device".to_owned();
        state.replace_access_token(Some("private-token".to_owned()));
      }
      Self {
        listener,
        client,
        url,
        provider,
      }
    }

    async fn request(&self) -> (TcpStream, String) {
      tokio::time::timeout(Duration::from_secs(2), async {
        let (mut socket, _) = self.listener.accept().await.expect("accept");
        let mut bytes = Vec::new();
        let mut buffer = [0; 2048];
        loop {
          let count = socket.read(&mut buffer).await.expect("read");
          assert!(count > 0);
          bytes.extend_from_slice(&buffer[..count]);
          if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            let content_length = headers
              .lines()
              .find_map(|line| line.strip_prefix("content-length: "))
              .map(|length| length.parse::<usize>().expect("content length"))
              .unwrap_or(0);
            if bytes.len() >= end + 4 + content_length {
              break;
            }
          }
        }
        (socket, String::from_utf8(bytes).expect("text request"))
      })
      .await
      .expect("request begins")
    }

    async fn sessions(&self, sessions: Value) -> Vec<RemoteSession> {
      let facade = self.client.remote_control();
      let (result, _) = tokio::join!(facade.sessions(), async {
        let (socket, _) = self.request().await;
        respond(socket, "200 OK", &[], &sessions.to_string()).await;
      });
      result.expect("discovery")
    }

    async fn send(&self, target: &RemoteSession, command: RemoteControlRequest) -> String {
      let facade = self.client.remote_control();
      let (result, request) = tokio::join!(facade.send(target, command), async {
        let (socket, request) = self.request().await;
        respond(socket, "204 No Content", &[], "").await;
        request
      });
      result.expect("dispatch");
      request
    }

    fn playing(&self, paused: bool) -> Value {
      json!({
        "Id":if paused { "paused-session" } else { "playing-session" }, "DeviceId":"remote-device",
        "DeviceName":"Living room", "Client":"Remote player", "UserName":"Other user",
        "UserId":"00000000000000000000000000000009", "IsActive":true,
        "SupportsRemoteControl":true, "SupportsMediaControl":true,
        "PlayableMediaTypes":["Video"], "SupportedCommands":["SetVolume"],
        "NowPlayingItem":{"Id":if self.provider == MediaServerProvider::Jellyfin { JELLYFIN_ITEM } else { "42" },
          "Name":"Movie", "RunTimeTicks":1200000000_i64},
        "PlayState":{"PositionTicks":200000000_i64,"IsPaused":paused,"CanSeek":true,"VolumeLevel":30}
      })
    }
  }

  async fn respond(mut socket: TcpStream, status: &str, headers: &[(&str, &str)], body: &str) {
    let mut head = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (name, value) in headers {
      head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    socket
      .write_all(head.as_bytes())
      .await
      .expect("write headers");
    socket.write_all(body.as_bytes()).await.expect("write body");
  }

  fn request_url(request: &str) -> url::Url {
    url::Url::parse(&format!(
      "http://localhost{}",
      request.split_whitespace().nth(1).expect("request target")
    ))
    .expect("request URL")
  }

  fn request_body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").expect("headers").1)
      .expect("JSON request body")
  }

  fn assert_auth(request: &str, provider: MediaServerProvider) {
    let header = match provider {
      MediaServerProvider::Jellyfin => "\r\nauthorization: mediabrowser ",
      MediaServerProvider::Emby => "\r\nx-emby-authorization: mediabrowser ",
    };
    assert!(request.to_lowercase().contains(header));
    assert!(request.contains("Token=\"private-token\""));
    let url = request_url(request);
    assert!(!url.as_str().contains("private-token") && !url.as_str().contains("api_key"));
  }

  #[tokio::test]
  async fn discovery_uses_current_user_filter_and_preserves_unknown_state_for_both_providers() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let server = Server::new(provider).await;
      let mut local = server.playing(false);
      local["DeviceId"] = json!("local-device");
      let unknown = json!({"Id":"unknown-session","DeviceId":"unknown-device","SupportsRemoteControl":true,"SupportsMediaControl":true});
      let mut list = vec![server.playing(false), local, unknown];
      if provider == MediaServerProvider::Jellyfin {
        let mut inactive = server.playing(false);
        inactive["Id"] = json!("inactive");
        inactive["IsActive"] = json!(false);
        list.push(inactive);
      }
      let facade = server.client.remote_control();
      let (result, request) = tokio::join!(facade.sessions(), async {
        let (socket, request) = server.request().await;
        respond(socket, "200 OK", &[], &json!(list).to_string()).await;
        request
      });
      let sessions = result.expect("discovery");
      assert_eq!(sessions.len(), 2);
      assert_auth(&request, provider);
      let url = request_url(&request);
      assert_eq!(url.path(), "/proxy/Sessions");
      assert!(url
        .query_pairs()
        .any(|(key, value)| key.eq_ignore_ascii_case("controllableByUserId") && value == USER));
      let target = sessions[0].view();
      assert_eq!(target.user_name.as_deref(), Some("Other user"));
      assert_eq!(
        target
          .now_playing
          .as_ref()
          .and_then(|playing| playing.position_seconds),
        Some(20.0)
      );
      assert!(
        target.capabilities.can_pause
          && target.capabilities.can_stop
          && target.capabilities.can_seek
      );
      assert!(!target.capabilities.can_resume);
      assert_eq!(
        sessions[1].view().capabilities,
        RemoteControlCapabilities::default()
      );
      assert!(sessions[1].view().volume.is_none() && sessions[1].view().now_playing.is_none());
      assert_eq!(format!("{:?}", sessions[0]), "RemoteSession { .. }");
    }
  }

  #[tokio::test]
  async fn generated_commands_use_provider_specific_query_and_body_contracts() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let server = Server::new(provider).await;
      let targets = server
        .sessions(json!([server.playing(false), server.playing(true)]))
        .await;
      for (target, command, expected) in [
        (&targets[0], RemoteControlRequest::Pause, "Pause"),
        (&targets[1], RemoteControlRequest::Resume, "Unpause"),
        (&targets[0], RemoteControlRequest::Stop, "Stop"),
        (
          &targets[0],
          RemoteControlRequest::Seek {
            position_ticks: 300000000,
          },
          "Seek",
        ),
      ] {
        let request = server.send(target, command).await;
        assert_auth(&request, provider);
        let url = request_url(&request);
        assert_eq!(
          url.path(),
          format!(
            "/proxy/Sessions/{}/Playing/{expected}",
            target.view().key.session_id
          )
        );
        if provider == MediaServerProvider::Jellyfin {
          assert!(url
            .query_pairs()
            .any(|(key, value)| key == "controllingUserId" && value == USER));
          if expected == "Seek" {
            assert!(url
              .query_pairs()
              .any(|(key, value)| key == "seekPositionTicks" && value == "300000000"));
          }
        } else {
          let body = request_body(&request);
          assert_eq!(body["Command"], expected);
          assert_eq!(body["ControllingUserId"], USER);
          if expected == "Seek" {
            assert_eq!(body["SeekPositionTicks"], 300000000_i64);
          }
        }
      }
      let request = server
        .send(&targets[0], RemoteControlRequest::SetVolume { volume: 45 })
        .await;
      assert_eq!(
        request_url(&request).path(),
        "/proxy/Sessions/playing-session/Command"
      );
      assert_eq!(request_body(&request)["Name"], "SetVolume");
      assert_eq!(request_body(&request)["Arguments"]["Volume"], "45");
      let item_id = if provider == MediaServerProvider::Jellyfin {
        JELLYFIN_ITEM
      } else {
        "42"
      };
      let request = server
        .send(
          &targets[0],
          RemoteControlRequest::PlayNow {
            item_id: item_id.to_owned(),
            start_position_ticks: Some(100000000),
          },
        )
        .await;
      assert_auth(&request, provider);
      let url = request_url(&request);
      assert_eq!(url.path(), "/proxy/Sessions/playing-session/Playing");
      let query: HashMap<_, _> = url
        .query_pairs()
        .map(|(key, value)| (key.to_lowercase(), value.into_owned()))
        .collect();
      assert_eq!(query["playcommand"], "PlayNow");
      assert_eq!(query["startpositionticks"], "100000000");
      if provider == MediaServerProvider::Jellyfin {
        assert_eq!(
          uuid::Uuid::parse_str(&query["itemids"]).expect("typed UUID"),
          uuid::Uuid::parse_str(JELLYFIN_ITEM).expect("fixture UUID")
        );
      } else {
        assert_eq!(query["itemids"], "42");
        assert_eq!(request_body(&request)["ControllingUserId"], USER);
      }
    }
  }

  #[tokio::test]
  async fn invalid_arguments_and_unavailable_capabilities_fail_before_network_dispatch() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let server = Server::new(provider).await;
      let targets = server
        .sessions(json!([server.playing(false),{"Id":"idle","DeviceId":"idle-device"}]))
        .await;
      let facade = server.client.remote_control();
      for command in [
        RemoteControlRequest::Resume,
        RemoteControlRequest::Seek { position_ticks: -1 },
        RemoteControlRequest::Seek {
          position_ticks: 1200000001,
        },
        RemoteControlRequest::SetVolume { volume: 101 },
        RemoteControlRequest::PlayNow {
          item_id: "invalid-item-secret".into(),
          start_position_ticks: None,
        },
        RemoteControlRequest::PlayNow {
          item_id: JELLYFIN_ITEM.into(),
          start_position_ticks: Some(-1),
        },
      ] {
        let error = facade
          .send(&targets[0], command)
          .await
          .expect_err("invalid request");
        assert!(!error.to_string().contains("invalid-item-secret"));
      }
      for command in [
        RemoteControlRequest::Pause,
        RemoteControlRequest::Stop,
        RemoteControlRequest::SetVolume { volume: 50 },
      ] {
        assert!(facade.send(&targets[1], command).await.is_err());
      }
    }
  }

  #[tokio::test]
  async fn remote_target_rejects_other_client_token_server_user_and_provider_scopes() {
    let server = Server::new(MediaServerProvider::Jellyfin).await;
    let other = Server::new(MediaServerProvider::Jellyfin).await;
    let target = server
      .sessions(json!([server.playing(false)]))
      .await
      .remove(0);
    assert!(matches!(
      other
        .client
        .remote_control()
        .send(&target, RemoteControlRequest::Pause)
        .await,
      Err(JellyfinError::NotConnected)
    ));
    for change in 0..4 {
      {
        let mut state = server.client.state.write();
        state.provider = MediaServerProvider::Jellyfin;
        state.server_url = Some(server.url.clone());
        state.user_id = Some(USER.into());
      }
      let fresh = server
        .sessions(json!([server.playing(false)]))
        .await
        .remove(0);
      {
        let mut state = server.client.state.write();
        match change {
          0 => state.replace_access_token(Some("new-token".into())),
          1 => state.server_url = Some(other.url.clone()),
          2 => state.user_id = Some("other-user".into()),
          _ => state.provider = MediaServerProvider::Emby,
        }
      }
      assert!(matches!(
        server
          .client
          .remote_control()
          .send(&fresh, RemoteControlRequest::Pause)
          .await,
        Err(JellyfinError::NotConnected)
      ));
    }
  }

  #[tokio::test]
  async fn profile_switch_during_discovery_or_command_rejects_successful_completion() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let server = Server::new(provider).await;
      let facade = server.client.remote_control();
      let (result, _) = tokio::join!(facade.sessions(), async {
        let (socket, _) = server.request().await;
        server
          .client
          .state
          .write()
          .replace_access_token(Some("next-token".into()));
        respond(
          socket,
          "200 OK",
          &[],
          &json!([server.playing(false)]).to_string(),
        )
        .await;
      });
      assert!(matches!(result, Err(JellyfinError::NotConnected)));
      let targets = server.sessions(json!([server.playing(false)])).await;
      let (result, _) = tokio::join!(
        facade.send(&targets[0], RemoteControlRequest::Pause),
        async {
          let (socket, _) = server.request().await;
          server.client.state.write().replace_access_token(None);
          respond(socket, "204 No Content", &[], "").await;
        }
      );
      assert!(matches!(result, Err(JellyfinError::NotConnected)));
    }
  }

  #[tokio::test]
  async fn provider_errors_and_strict_deserialization_are_redacted_without_fallback() {
    for provider in [MediaServerProvider::Jellyfin, MediaServerProvider::Emby] {
      let server = Server::new(provider).await;
      let facade = server.client.remote_control();
      for status in [
        "401 Unauthorized",
        "403 Forbidden",
        "500 Internal Server Error",
        "302 Found",
      ] {
        let location = format!("{}/private-token", server.url);
        let (result, _) = tokio::join!(facade.sessions(), async {
          let (socket, _) = server.request().await;
          respond(
            socket,
            status,
            &[("Location", &location)],
            "private-token response",
          )
          .await;
        });
        let error = result.expect_err("provider rejects");
        assert!(!error.to_string().contains("private-token"));
        if status.starts_with("401") || status.starts_with("403") {
          assert!(matches!(error, JellyfinError::AuthFailed(_)));
        }
      }
      let (result, _) = tokio::join!(facade.sessions(), async {
        let (socket, _) = server.request().await;
        respond(socket, "200 OK", &[], "private-token invalid JSON").await;
      });
      assert!(!result
        .expect_err("malformed")
        .to_string()
        .contains("private-token"));
      if provider == MediaServerProvider::Jellyfin {
        let mut unknown_command = server.playing(false);
        unknown_command["SupportedCommands"] = json!(["FutureCommand"]);
        let (result, _) = tokio::join!(facade.sessions(), async {
          let (socket, _) = server.request().await;
          respond(socket, "200 OK", &[], &json!([unknown_command]).to_string()).await;
        });
        assert!(result.is_err());
      }
    }
  }
}
