//! Session-bound reads of existing Jellyfin trickplay assets.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Cursor;
use std::sync::Weak;
use std::time::Duration;

use bytes::Bytes;
use image::ImageDecoder as _;
use jellyfin_api::models::TrickplayInfoDto;
use serde::Deserialize;

use super::{ClientState, JellyfinClient, JellyfinError, JellyfinPlayback, MediaServerProvider};

const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_ATLAS_BYTES: usize = 8 * 1024 * 1024;
const MAX_ATLAS_PIXELS: u64 = 16 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
static DECODE_SLOTS: std::sync::LazyLock<std::sync::Arc<tokio::sync::Semaphore>> =
  std::sync::LazyLock::new(|| std::sync::Arc::new(tokio::sync::Semaphore::new(2)));

/// Thumbnail grid and timing from one validated trickplay resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrickplayGeometry {
  pub width: u32,
  pub height: u32,
  pub columns: u32,
  pub rows: u32,
  pub thumbnail_count: u32,
  pub interval_ms: u32,
}

/// Opaque asset identity tied to the client and authentication session that read it.
/// Contains no access token; debug output excludes server, user, and media identities.
#[derive(Clone)]
pub struct TrickplayManifest {
  scope: Scope,
  item_id: String,
  media_source_id: String,
  geometry: TrickplayGeometry,
}

impl TrickplayManifest {
  /// Validated grid and timing for choosing an atlas and cropping a thumbnail.
  pub const fn geometry(&self) -> TrickplayGeometry {
    self.geometry
  }
}

impl fmt::Debug for TrickplayManifest {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("TrickplayManifest")
      .field("geometry", &self.geometry)
      .finish_non_exhaustive()
  }
}

/// Decoded RGBA atlas. Cloning shares the pixel buffer.
#[derive(Clone)]
pub struct TrickplayAtlas {
  pub width: u32,
  pub height: u32,
  pub pixels: Bytes,
}

impl fmt::Debug for TrickplayAtlas {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("TrickplayAtlas")
      .field("width", &self.width)
      .field("height", &self.height)
      .finish_non_exhaustive()
  }
}

#[derive(Clone)]
struct Scope {
  client: Weak<parking_lot::RwLock<ClientState>>,
  server_url: String,
  user_id: String,
  epoch: u64,
}

impl Scope {
  fn capture(client: &JellyfinClient) -> Result<Option<Self>, JellyfinError> {
    let state = client.state.read();
    if state.provider != MediaServerProvider::Jellyfin {
      return Ok(None);
    }
    if state.access_token.is_none() {
      return Err(JellyfinError::NotConnected);
    }
    Ok(Some(Self {
      client: std::sync::Arc::downgrade(&client.state),
      server_url: state
        .server_url
        .clone()
        .ok_or(JellyfinError::NotConnected)?,
      user_id: state.user_id.clone().ok_or(JellyfinError::NotConnected)?,
      epoch: state.authentication_epoch,
    }))
  }

  fn validate(&self, client: &JellyfinClient, state: &ClientState) -> Result<(), JellyfinError> {
    if !self
      .client
      .ptr_eq(&std::sync::Arc::downgrade(&client.state))
      || state.provider != MediaServerProvider::Jellyfin
      || state.server_url.as_deref() != Some(self.server_url.as_str())
      || state.user_id.as_deref() != Some(self.user_id.as_str())
      || state.authentication_epoch != self.epoch
      || state.access_token.is_none()
    {
      return Err(JellyfinError::NotConnected);
    }
    Ok(())
  }

  fn url(&self, segments: &[&str]) -> Result<url::Url, JellyfinError> {
    let normalized = JellyfinClient::normalize_server_url(&self.server_url)
      .map_err(|_| failed("Invalid trickplay server URL"))?;
    let mut url =
      url::Url::parse(&normalized).map_err(|_| failed("Invalid trickplay server URL"))?;
    url
      .path_segments_mut()
      .map_err(|_| failed("Invalid trickplay server URL"))?
      .pop_if_empty()
      .extend(segments);
    Ok(url)
  }

  async fn read(
    &self,
    client: &JellyfinClient,
    url: url::Url,
    limit: usize,
    accept: &'static str,
  ) -> Result<Option<Vec<u8>>, JellyfinError> {
    // Session validation and header capture share one lock: a profile switch cannot
    // attach the new account's token to this scope's old server URL.
    let request = {
      let state = client.state.read();
      self.validate(client, &state)?;
      let authorization = JellyfinClient::auth_header_from_parts(
        &state.device_name,
        &state.device_id,
        state.access_token.as_deref(),
      );
      client
        .image_http
        .get(url)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .header(
          reqwest::header::USER_AGENT,
          JellyfinClient::app_user_agent(),
        )
        .header(reqwest::header::ACCEPT, accept)
        .timeout(REQUEST_TIMEOUT)
    };
    let mut response = request
      .send()
      .await
      .map_err(|_| failed("Trickplay request failed"))?;
    self.validate(client, &client.state.read())?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
      return Ok(None);
    }
    if !status.is_success() {
      // Do not read error bodies: they are untrusted and may contain credentials.
      return Err(JellyfinClient::redacted_response_error(
        "Trickplay",
        status,
        true,
      ));
    }
    if response
      .content_length()
      .is_some_and(|length| length > limit as u64)
    {
      return Err(failed("Trickplay response exceeds the byte limit"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
      .chunk()
      .await
      .map_err(|_| failed("Trickplay response failed"))?
    {
      self.validate(client, &client.state.read())?;
      if chunk.len() > limit.saturating_sub(bytes.len()) {
        return Err(failed("Trickplay response exceeds the byte limit"));
      }
      bytes.extend_from_slice(&chunk);
    }
    self.validate(client, &client.state.read())?;
    Ok(Some(bytes))
  }
}

#[derive(Deserialize)]
struct ItemMetadata {
  #[serde(rename = "Trickplay")]
  trickplay: Option<BTreeMap<String, BTreeMap<String, TrickplayInfoDto>>>,
}

impl JellyfinPlayback<'_> {
  /// Reads existing Jellyfin trickplay metadata without generating assets.
  /// Emby, missing assets, and unavailable source/resolution return `None`.
  /// A known media source must match; an unknown source is accepted only when
  /// exactly one source is advertised. Reads are bounded to 2 MiB and 5 seconds.
  pub async fn trickplay_manifest(
    &self,
    item_id: &str,
    media_source_id: Option<&str>,
  ) -> Result<Option<TrickplayManifest>, JellyfinError> {
    let Some(scope) = Scope::capture(self.client)? else {
      return Ok(None);
    };
    let item_id = uuid::Uuid::parse_str(item_id)
      .map_err(|_| failed("Invalid trickplay item identity"))?
      .simple()
      .to_string();
    let mut url = scope.url(&["Items", &item_id])?;
    url.query_pairs_mut().append_pair("userId", &scope.user_id);
    let Some(bytes) = scope
      .read(self.client, url, MAX_METADATA_BYTES, "application/json")
      .await?
    else {
      return Ok(None);
    };
    let metadata: ItemMetadata =
      serde_json::from_slice(&bytes).map_err(|_| failed("Malformed trickplay metadata"))?;
    let Some(mut sources) = metadata.trickplay else {
      return Ok(None);
    };
    let selected = match media_source_id {
      Some(source_id) => sources.remove_entry(source_id).or_else(|| {
        let source_id = uuid::Uuid::parse_str(source_id).ok()?;
        let matching = sources
          .keys()
          .find(|key| uuid::Uuid::parse_str(key).ok() == Some(source_id))?
          .clone();
        sources.remove_entry(&matching)
      }),
      None if sources.len() == 1 => sources.pop_first(),
      None => None,
    };
    let Some((media_source_id, resolutions)) = selected else {
      return Ok(None);
    };
    let media_source_id = uuid::Uuid::parse_str(&media_source_id)
      .map_err(|_| failed("Invalid trickplay source identity"))?
      .simple()
      .to_string();
    let geometry = resolutions
      .into_iter()
      .filter_map(|(key, info)| {
        let geometry = geometry(info)?;
        (key.parse::<u32>().ok() == Some(geometry.width)).then_some(geometry)
      })
      .min_by_key(|geometry| {
        let below_preview_width = geometry.width < 240;
        (
          below_preview_width,
          if below_preview_width {
            u32::MAX - geometry.width
          } else {
            geometry.width
          },
        )
      });
    scope.validate(self.client, &self.client.state.read())?;
    Ok(geometry.map(|geometry| TrickplayManifest {
      scope,
      item_id,
      media_source_id,
      geometry,
    }))
  }

  /// Fetches one JPEG atlas from a still-current manifest and decodes it off the
  /// async executor. The encoded response is limited to 8 MiB / 5 seconds and
  /// the grid to 16 Mi pixels. A retired session fails before credentials attach.
  pub async fn trickplay_atlas(
    &self,
    manifest: &TrickplayManifest,
    index: u32,
  ) -> Result<TrickplayAtlas, JellyfinError> {
    manifest
      .scope
      .validate(self.client, &self.client.state.read())?;
    let geometry = manifest.geometry;
    let per_atlas = geometry.columns * geometry.rows;
    if index >= geometry.thumbnail_count.div_ceil(per_atlas) {
      return Err(failed("Trickplay atlas index is out of range"));
    }
    let mut url = manifest.scope.url(&[
      "Videos",
      &manifest.item_id,
      "Trickplay",
      &geometry.width.to_string(),
      &format!("{index}.jpg"),
    ])?;
    url
      .query_pairs_mut()
      .append_pair("mediaSourceId", &manifest.media_source_id);
    let bytes = manifest
      .scope
      .read(self.client, url, MAX_ATLAS_BYTES, "image/jpeg")
      .await?
      .ok_or_else(|| failed("Trickplay atlas is unavailable"))?;
    // A cancelled subscriber cannot stop a running blocking decode. The permit
    // stays in that job so repeated hover cancellation cannot grow the workload.
    let permit = std::sync::Arc::clone(&DECODE_SLOTS)
      .acquire_owned()
      .await
      .map_err(|_| failed("Trickplay decoder is unavailable"))?;
    manifest
      .scope
      .validate(self.client, &self.client.state.read())?;
    let atlas = tokio::task::spawn_blocking(move || {
      let _permit = permit;
      decode(bytes, geometry, index)
    })
    .await
    .map_err(|_| failed("Trickplay decode failed"))??;
    manifest
      .scope
      .validate(self.client, &self.client.state.read())?;
    Ok(atlas)
  }
}

fn geometry(info: TrickplayInfoDto) -> Option<TrickplayGeometry> {
  let positive = |value: Option<i32>| u32::try_from(value?).ok().filter(|value| *value > 0);
  let geometry = TrickplayGeometry {
    width: positive(info.width)?,
    height: positive(info.height)?,
    columns: positive(info.tile_width)?,
    rows: positive(info.tile_height)?,
    thumbnail_count: positive(info.thumbnail_count)?,
    interval_ms: positive(info.interval)?,
  };
  let width = geometry.width.checked_mul(geometry.columns)?;
  let height = geometry.height.checked_mul(geometry.rows)?;
  (geometry.width <= 512
    && width <= 8192
    && height <= 8192
    && u64::from(width) * u64::from(height) <= MAX_ATLAS_PIXELS)
    .then_some(geometry)
}

fn decode(
  bytes: Vec<u8>,
  geometry: TrickplayGeometry,
  index: u32,
) -> Result<TrickplayAtlas, JellyfinError> {
  let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes))
    .map_err(|_| failed("Invalid trickplay JPEG"))?;
  let (width, height) = decoder.dimensions();
  let per_atlas = geometry.columns * geometry.rows;
  let remaining = (geometry.thumbnail_count - index * per_atlas).min(per_atlas);
  let minimum_rows = remaining.div_ceil(geometry.columns);
  if width != geometry.width * geometry.columns
    || height < minimum_rows * geometry.height
    || height > geometry.height * geometry.rows
    || !height.is_multiple_of(geometry.height)
    || u64::from(width) * u64::from(height) > MAX_ATLAS_PIXELS
  {
    return Err(failed("Trickplay atlas dimensions do not match its grid"));
  }
  let mut limits = image::Limits::default();
  limits.max_image_width = Some(width);
  limits.max_image_height = Some(height);
  limits.max_alloc = Some(MAX_ATLAS_PIXELS * 8);
  decoder
    .set_limits(limits)
    .map_err(|_| failed("Trickplay decode exceeds its limit"))?;
  let rgba = image::DynamicImage::from_decoder(decoder)
    .map_err(|_| failed("Invalid trickplay JPEG"))?
    .into_rgba8();
  Ok(TrickplayAtlas {
    width,
    height,
    pixels: Bytes::from(rgba.into_raw()),
  })
}

fn failed(message: &str) -> JellyfinError {
  JellyfinError::HttpError(message.to_owned())
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use image::codecs::jpeg::JpegEncoder;
  use serde_json::{json, Value};
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::{TcpListener, TcpStream};

  use super::*;

  const ITEM: &str = "00000000000000000000000000000002";
  const SOURCE: &str = "00000000000000000000000000000003";
  const OTHER_SOURCE: &str = "00000000000000000000000000000004";

  struct Server {
    listener: TcpListener,
    url: String,
    client: Arc<JellyfinClient>,
  }

  impl Server {
    async fn new() -> Self {
      let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
      let url = format!(
        "http://{}/jellyfin",
        listener.local_addr().expect("address")
      );
      let client = Arc::new(JellyfinClient::with_storage_dir(std::env::temp_dir()));
      {
        let mut state = client.state.write();
        state.server_url = Some(url.clone());
        state.user_id = Some("user".into());
        state.replace_access_token(Some("private-token".into()));
      }
      Self {
        listener,
        url,
        client,
      }
    }

    async fn request(&self) -> (TcpStream, String) {
      tokio::time::timeout(Duration::from_secs(2), async {
        let (mut socket, _) = self.listener.accept().await.expect("accept request");
        let mut request = Vec::new();
        let mut buffer = [0; 2048];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
          let count = socket.read(&mut buffer).await.expect("read request");
          assert!(count > 0);
          request.extend_from_slice(&buffer[..count]);
        }
        (socket, String::from_utf8(request).expect("HTTP headers"))
      })
      .await
      .expect("request starts")
    }

    async fn manifest(
      &self,
      body: Value,
      source: Option<&str>,
    ) -> Result<Option<TrickplayManifest>, JellyfinError> {
      let playback = self.client.playback();
      let (result, _) = tokio::join!(playback.trickplay_manifest(ITEM, source), async {
        let (socket, _) = self.request().await;
        respond(socket, "200 OK", &[], body.to_string().as_bytes()).await;
      });
      result
    }

    async fn atlas(
      &self,
      manifest: &TrickplayManifest,
      bytes: &[u8],
    ) -> Result<TrickplayAtlas, JellyfinError> {
      let playback = self.client.playback();
      let (result, _) = tokio::join!(playback.trickplay_atlas(manifest, 0), async {
        let (socket, _) = self.request().await;
        respond(socket, "200 OK", &[], bytes).await;
      });
      result
    }
  }

  async fn respond(mut socket: TcpStream, status: &str, extra: &[(&str, String)], body: &[u8]) {
    let mut head = format!(
      "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n",
      body.len()
    );
    for (key, value) in extra {
      head.push_str(&format!("{key}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = socket.write_all(head.as_bytes()).await;
    let _ = socket.write_all(body).await;
  }

  fn info(width: u32) -> Value {
    json!({"Width":width,"Height":90,"TileWidth":2,"TileHeight":2,"ThumbnailCount":5,"Interval":10000})
  }

  fn metadata() -> Value {
    json!({"Trickplay":{SOURCE:{"160":info(160)}}})
  }

  fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    JpegEncoder::new(&mut bytes)
      .encode(
        &vec![64; (width * height * 3) as usize],
        width,
        height,
        image::ExtendedColorType::Rgb8,
      )
      .expect("JPEG fixture");
    bytes
  }

  #[tokio::test]
  async fn http_seam_preserves_base_path_source_and_auth_and_decodes_last_atlas() {
    let server = Server::new().await;
    let playback = server.client.playback();
    let (manifest, request) =
      tokio::join!(playback.trickplay_manifest(ITEM, Some(SOURCE)), async {
        let (socket, request) = server.request().await;
        respond(socket, "200 OK", &[], metadata().to_string().as_bytes()).await;
        request
      });
    let manifest = manifest.expect("metadata succeeds").expect("manifest");
    assert!(request.starts_with(&format!("GET /jellyfin/Items/{ITEM}?userId=user ")));
    assert!(request
      .to_lowercase()
      .contains("\r\nauthorization: mediabrowser "));
    assert!(request.contains("Token=\"private-token\""));
    let debug = format!("{manifest:?}");
    assert!(
      !debug.contains(&server.url) && !debug.contains(SOURCE) && !debug.contains("private-token")
    );
    let (atlas, request) = tokio::join!(playback.trickplay_atlas(&manifest, 1), async {
      let (socket, request) = server.request().await;
      respond(socket, "200 OK", &[], &jpeg(320, 90)).await;
      request
    });
    let atlas = atlas.expect("last atlas decodes");
    assert!(request.starts_with(&format!(
      "GET /jellyfin/Videos/{ITEM}/Trickplay/160/1.jpg?mediaSourceId={SOURCE} "
    )));
    assert!(!request
      .lines()
      .next()
      .expect("request line")
      .contains("private-token"));
    assert_eq!(
      (atlas.width, atlas.height, atlas.pixels.len()),
      (320, 90, 320 * 90 * 4)
    );
    assert_eq!(atlas.pixels.as_ptr(), atlas.clone().pixels.as_ptr());
  }

  #[tokio::test]
  async fn source_selection_never_substitutes_an_alternate_and_chooses_a_bounded_resolution() {
    let server = Server::new().await;
    let multiple = json!({"Trickplay":{SOURCE:{"160":info(160)},OTHER_SOURCE:{"320":info(320)}}});
    assert!(server
      .manifest(multiple.clone(), None)
      .await
      .expect("metadata")
      .is_none());
    assert!(server
      .manifest(multiple, Some(ITEM))
      .await
      .expect("metadata")
      .is_none());
    let selected = json!({"Trickplay":{SOURCE:{"80":info(80),"160":info(160),"320":info(320),"1024":info(1024)}}});
    let manifest = server
      .manifest(selected, Some("00000000-0000-0000-0000-000000000003"))
      .await
      .expect("metadata")
      .expect("same UUID");
    assert_eq!(manifest.geometry().width, 320);
    let low_resolution = json!({"Trickplay":{SOURCE:{"128":info(128)}}});
    let manifest = server
      .manifest(low_resolution, None)
      .await
      .expect("metadata")
      .expect("existing low resolution");
    assert_eq!(manifest.geometry().width, 128);
    let invalid = json!({"Trickplay":{SOURCE:{"160":{ "Width":160,"Height":90,"TileWidth":i32::MAX,"TileHeight":2,"ThumbnailCount":5,"Interval":10000}}}});
    assert!(server
      .manifest(invalid, None)
      .await
      .expect("metadata")
      .is_none());
  }

  #[tokio::test]
  async fn emby_and_missing_assets_are_optional_but_malformed_payloads_are_safe_errors() {
    let server = Server::new().await;
    for body in [
      json!({}),
      json!({"Trickplay":null}),
      json!({"Trickplay":{}}),
    ] {
      assert!(server
        .manifest(body, None)
        .await
        .expect("missing metadata")
        .is_none());
    }
    let error = server
      .manifest(json!({"Trickplay":"private-token"}), None)
      .await
      .expect_err("malformed");
    assert!(matches!(error, JellyfinError::HttpError(_)));
    assert!(!error.to_string().contains("private-token"));
    let playback = server.client.playback();
    let (result, _) = tokio::join!(playback.trickplay_manifest(ITEM, None), async {
      let (socket, _) = server.request().await;
      respond(socket, "404 Not Found", &[], b"private-token").await;
    });
    assert!(result.expect("missing endpoint").is_none());
    server.client.state.write().provider = MediaServerProvider::Emby;
    assert!(playback
      .trickplay_manifest(ITEM, None)
      .await
      .expect("Emby unsupported")
      .is_none());
  }

  #[tokio::test]
  async fn manifest_cannot_cross_client_authentication_or_profile_boundaries() {
    let server = Server::new().await;
    let manifest = server
      .manifest(metadata(), None)
      .await
      .expect("metadata")
      .expect("manifest");
    let other = Server::new().await;
    assert!(matches!(
      other.client.playback().trickplay_atlas(&manifest, 0).await,
      Err(JellyfinError::NotConnected)
    ));
    server
      .client
      .state
      .write()
      .replace_access_token(Some("replacement-token".into()));
    assert!(matches!(
      server.client.playback().trickplay_atlas(&manifest, 0).await,
      Err(JellyfinError::NotConnected)
    ));
    let manifest = server
      .manifest(metadata(), None)
      .await
      .expect("metadata")
      .expect("manifest");
    server.client.state.write().server_url = Some(other.url.clone());
    assert!(matches!(
      server.client.playback().trickplay_atlas(&manifest, 0).await,
      Err(JellyfinError::NotConnected)
    ));
  }

  #[tokio::test]
  async fn session_switch_during_http_cannot_publish_metadata_or_pixels() {
    let server = Server::new().await;
    let playback = server.client.playback();
    let (result, _) = tokio::join!(playback.trickplay_manifest(ITEM, None), async {
      let (socket, _) = server.request().await;
      server
        .client
        .state
        .write()
        .replace_access_token(Some("second-token".into()));
      respond(socket, "200 OK", &[], metadata().to_string().as_bytes()).await;
    });
    assert!(matches!(result, Err(JellyfinError::NotConnected)));
    let manifest = server
      .manifest(metadata(), None)
      .await
      .expect("metadata")
      .expect("manifest");
    let (result, _) = tokio::join!(playback.trickplay_atlas(&manifest, 0), async {
      let (socket, _) = server.request().await;
      server.client.state.write().replace_access_token(None);
      respond(socket, "200 OK", &[], &jpeg(320, 180)).await;
    });
    assert!(matches!(result, Err(JellyfinError::NotConnected)));
  }

  #[tokio::test]
  async fn redirects_and_error_bodies_are_never_followed_or_exposed() {
    let server = Server::new().await;
    let playback = server.client.playback();
    for status in ["302 Found", "401 Unauthorized", "500 Internal Server Error"] {
      let (result, request) = tokio::join!(playback.trickplay_manifest(ITEM, None), async {
        let (socket, request) = server.request().await;
        respond(
          socket,
          status,
          &[("Location", format!("{}/private-token", server.url))],
          &vec![b'x'; MAX_METADATA_BYTES + 1],
        )
        .await;
        request
      });
      let error = result.expect_err("HTTP status rejects");
      assert!(!error.to_string().contains("private-token"));
      assert!(request.starts_with(&format!("GET /jellyfin/Items/{ITEM}?")));
    }
  }

  #[tokio::test]
  async fn metadata_and_atlas_body_limits_reject_declared_and_chunked_overruns() {
    let server = Server::new().await;
    let playback = server.client.playback();
    let (result, _) = tokio::join!(playback.trickplay_manifest(ITEM, None), async {
      let (mut socket, _) = server.request().await;
      socket
        .write_all(
          format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_METADATA_BYTES + 1
          )
          .as_bytes(),
        )
        .await
        .expect("headers");
    });
    assert!(matches!(result, Err(JellyfinError::HttpError(_))));
    let manifest = server
      .manifest(metadata(), None)
      .await
      .expect("metadata")
      .expect("manifest");
    let (result, _) = tokio::join!(playback.trickplay_atlas(&manifest, 0), async {
      let (mut socket, _) = server.request().await;
      let _ = socket
        .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n")
        .await;
      let _ = socket
        .write_all(format!("{:x}\r\n", MAX_ATLAS_BYTES + 1).as_bytes())
        .await;
      let _ = socket.write_all(&vec![b'x'; MAX_ATLAS_BYTES + 1]).await;
      let _ = socket.write_all(b"\r\n0\r\n\r\n").await;
    });
    assert!(matches!(result, Err(JellyfinError::HttpError(_))));
  }

  #[tokio::test]
  async fn invalid_jpeg_grid_and_atlas_indices_are_rejected() {
    let server = Server::new().await;
    let manifest = server
      .manifest(metadata(), None)
      .await
      .expect("metadata")
      .expect("manifest");
    for body in [
      b"not a jpeg".to_vec(),
      jpeg(320, 90),
      jpeg(321, 180),
      jpeg(320, 270),
    ] {
      assert!(matches!(
        server.atlas(&manifest, &body).await,
        Err(JellyfinError::HttpError(_))
      ));
    }
    assert!(matches!(
      server.client.playback().trickplay_atlas(&manifest, 2).await,
      Err(JellyfinError::HttpError(_))
    ));
    let atlas = server
      .atlas(&manifest, &jpeg(320, 180))
      .await
      .expect("valid grid");
    assert_eq!(atlas.pixels.len(), 320 * 180 * 4);
  }
}
