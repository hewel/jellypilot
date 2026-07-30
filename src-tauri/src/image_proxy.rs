//! Localhost image proxy server for decoupled media artwork loading.
//!
//! Uncached images stream from the matching media-server origin immediately
//! while an elected, bounded, best-effort writer persists the completed
//! original to the SQLite-backed disk cache. Later requests reuse the file
//! through the global catalog. Every failure mode fails open to origin.

use axum::{
  body::Body,
  extract::{Path, State},
  http::{header, Method, StatusCode},
  response::Response,
  routing::get,
  Router,
};
use bytes::Bytes;
use futures_util::stream;
use futures_util::StreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::TcpListener, sync::Arc, time::Duration};
use tokio::sync::{broadcast, watch, Mutex as TokioMutex};
use tokio_util::sync::CancellationToken;

use crate::config::AppConfig;
use crate::image_cache::{ImageCache, ImageCachePartition, StreamWriter};
use crate::image_ref::{decode_image_id, normalize_server_url};
use crate::jellyfin::JellyfinClient;

/// App local services state reported to frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppLocalServices {
  pub image_proxy_base: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageProxyError {
  #[error("Failed to bind proxy TCP listener: {0}")]
  Bind(#[from] std::io::Error),
}

type ProxyResult = Result<Arc<ImageProxy>, String>;
type BodyChunk = Result<Bytes, String>;

/// Thread-safe process state container for localhost image proxy.
#[derive(Clone, Default)]
pub struct ImageProxyState {
  inner: Arc<RwLock<Option<ProxyResult>>>,
}

impl ImageProxyState {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn install(&self, result: Result<Arc<ImageProxy>, ImageProxyError>) {
    let mut guard = self.inner.write();
    *guard = Some(result.map_err(|e| e.to_string()));
  }

  pub fn base_url(&self) -> Option<String> {
    let guard = self.inner.read();
    match &*guard {
      Some(Ok(proxy)) => Some(proxy.base_url.clone()),
      _ => None,
    }
  }

  pub fn local_services(&self) -> AppLocalServices {
    AppLocalServices {
      image_proxy_base: self.base_url(),
    }
  }
}

#[derive(Debug, Clone)]
pub struct CoalescedHeaders {
  pub status: StatusCode,
  pub content_type: Option<String>,
  pub content_length: Option<u64>,
  pub etag: Option<String>,
  pub last_modified: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FetchResult {
  Headers(CoalescedHeaders),
  Error(StatusCode, String),
}

struct InflightFetch {
  header_tx: watch::Sender<Option<FetchResult>>,
  body_tx: broadcast::Sender<BodyChunk>,
}
/// RAII guard that marks an active origin image fetch for the conversion
/// worker's foreground gate.
struct FetchGateGuard(Arc<crate::avif_worker::ForegroundGate>);

impl FetchGateGuard {
  fn new(gate: Arc<crate::avif_worker::ForegroundGate>) -> Self {
    gate.image_fetch_started();
    Self(gate)
  }
}

impl Drop for FetchGateGuard {
  fn drop(&mut self) {
    self.0.image_fetch_finished();
  }
}

pub struct ImageProxyInner {
  pub port: u16,
  pub base_url: String,
  pub client: Arc<JellyfinClient>,
  cache: Option<Arc<ImageCache>>,
  config: Arc<RwLock<AppConfig>>,
  gate: Arc<crate::avif_worker::ForegroundGate>,
  capability: crate::avif_worker::AvifCapability,
  coalescer: Arc<TokioMutex<HashMap<String, Arc<InflightFetch>>>>,
}

pub struct ImageProxy {
  pub base_url: String,
  pub shutdown_token: CancellationToken,
}

impl Drop for ImageProxy {
  fn drop(&mut self) {
    self.shutdown_token.cancel();
  }
}

impl ImageProxy {
  pub fn start(
    client: Arc<JellyfinClient>,
    cache: Option<Arc<ImageCache>>,
    config: Arc<RwLock<AppConfig>>,
    gate: Arc<crate::avif_worker::ForegroundGate>,
    capability: crate::avif_worker::AvifCapability,
  ) -> Result<Arc<Self>, ImageProxyError> {
    Self::start_on_addr("127.0.0.1:0", client, cache, config, gate, capability)
  }

  pub fn start_on_addr(
    addr: &str,
    client: Arc<JellyfinClient>,
    cache: Option<Arc<ImageCache>>,
    config: Arc<RwLock<AppConfig>>,
    gate: Arc<crate::avif_worker::ForegroundGate>,
    capability: crate::avif_worker::AvifCapability,
  ) -> Result<Arc<Self>, ImageProxyError> {
    let std_listener = TcpListener::bind(addr)?;
    let port = std_listener.local_addr()?.port();
    std_listener.set_nonblocking(true)?;

    let base_url = format!("http://127.0.0.1:{port}");
    let shutdown_token = CancellationToken::new();
    let inner = Arc::new(ImageProxyInner {
      port,
      base_url: base_url.clone(),
      client,
      cache,
      config,
      gate,
      capability,
      coalescer: Arc::new(TokioMutex::new(HashMap::new())),
    });

    let proxy = Arc::new(Self {
      base_url: base_url.clone(),
      shutdown_token: shutdown_token.clone(),
    });

    let app_router = Router::new()
      .route(
        "/image/{*token}",
        get(handle_image)
          .options(handle_options)
          .fallback(handle_method_not_allowed),
      )
      .fallback(handle_not_found)
      .with_state(inner);

    let shutdown_clone = shutdown_token.clone();
    tauri::async_runtime::spawn(async move {
      let tokio_listener = match tokio::net::TcpListener::from_std(std_listener) {
        Ok(listener) => listener,
        Err(e) => {
          log::error!("Image proxy listener registration failed: {}", e);
          return;
        }
      };
      let _ = axum::serve(tokio_listener, app_router)
        .with_graceful_shutdown(async move {
          shutdown_clone.cancelled().await;
        })
        .await;
    });

    Ok(proxy)
  }
}

async fn handle_options() -> Response {
  Response::builder()
    .status(StatusCode::NO_CONTENT)
    .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
    .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
    .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
    .body(Body::empty())
    .unwrap_or_else(|_| Response::new(Body::empty()))
}

async fn handle_method_not_allowed(req: axum::http::Request<Body>) -> Response {
  if req.method() == Method::OPTIONS {
    return handle_options().await;
  }
  text_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed")
}

async fn handle_not_found() -> Response {
  text_response(StatusCode::NOT_FOUND, "not found")
}

async fn handle_image(
  State(inner): State<Arc<ImageProxyInner>>,
  Path(token): Path<String>,
  headers: axum::http::HeaderMap,
) -> Response {
  let raw_token = token.trim_start_matches('/');
  if raw_token.is_empty() {
    return text_response(StatusCode::BAD_REQUEST, "missing image token");
  }

  let payload = match decode_image_id(raw_token) {
    Ok(payload) => payload,
    Err(err) => return text_response(StatusCode::BAD_REQUEST, err.to_string()),
  };

  let connection = inner.client.login().connection_state();
  if !connection.connected {
    return text_response(StatusCode::UNAUTHORIZED, "media server is not connected");
  }
  if connection.provider != payload.provider {
    return text_response(StatusCode::FORBIDDEN, "image reference provider mismatch");
  }
  let Some(server_url) = connection.server_url.as_deref() else {
    return text_response(StatusCode::UNAUTHORIZED, "media server URL is unavailable");
  };
  if normalize_server_url(server_url) != normalize_server_url(&payload.server_url) {
    return text_response(StatusCode::FORBIDDEN, "image reference server mismatch");
  }

  let if_none_match = headers
    .get(header::IF_NONE_MATCH)
    .and_then(|v| v.to_str().ok())
    .map(str::trim)
    .map(ToString::to_string);

  let partition = ImageCache::partition(payload.provider, server_url);
  inner
    .serve_image(&payload.remote_url, partition, if_none_match)
    .await
}

impl ImageProxyInner {
  fn cache_enabled(&self) -> bool {
    self.config.read().image_disk_cache_enabled
  }

  async fn serve_image(
    &self,
    remote_url: &str,
    partition: ImageCachePartition,
    if_none_match: Option<String>,
  ) -> Response {
    // Cache hit: serve from disk without touching the origin, unless the
    // active representation is AVIF and this platform's WebView cannot decode
    // it. In that case we drop the reader and fall through to re-fetch the
    // origin, which re-activates it (the worker stays gated off until a
    // positive capability probe, so no encode loop forms).
    if self.cache_enabled() {
      if let Some(cache) = &self.cache {
        if let Some(reader) = cache.open_reader(&partition, remote_url).await {
          let avif_unsupported =
            reader.content_type() == Some("image/avif") && !self.capability.is_supported();
          if avif_unsupported {
            drop(reader);
          } else {
            return serve_cached_file(reader, if_none_match).await;
          }
        }
      }
    }

    self.coalesce_and_fetch(remote_url, partition).await
  }

  async fn coalesce_and_fetch(&self, remote_url: &str, partition: ImageCachePartition) -> Response {
    let (body_rx, header_rx, is_initiator) = {
      let mut map = self.coalescer.lock().await;
      if let Some(inflight) = map.get(remote_url) {
        (
          inflight.body_tx.subscribe(),
          inflight.header_tx.subscribe(),
          false,
        )
      } else {
        let (header_tx, header_rx) = watch::channel(None);
        let (body_tx, body_rx) = broadcast::channel(32);
        let inflight = Arc::new(InflightFetch { header_tx, body_tx });
        map.insert(remote_url.to_string(), inflight);
        (body_rx, header_rx, true)
      }
    };

    if is_initiator {
      let inner_self = self.clone_self();
      let remote_url_owned = remote_url.to_string();
      tokio::spawn(async move {
        inner_self
          .perform_origin_fetch(&remote_url_owned, partition)
          .await;
      });
    }

    let mut sub = header_rx;
    let header_result = loop {
      if let Some(val) = sub.borrow().clone() {
        break val;
      }
      if sub.changed().await.is_err() {
        return text_response(StatusCode::BAD_GATEWAY, "origin fetch cancelled");
      }
    };

    match header_result {
      FetchResult::Error(status, msg) => text_response(status, msg),
      FetchResult::Headers(headers) => {
        let body_stream = stream::unfold(body_rx, |mut rx| async move {
          match rx.recv().await {
            Ok(item) => Some((item.map_err(std::io::Error::other), rx)),
            Err(broadcast::error::RecvError::Closed) => None,
            Err(broadcast::error::RecvError::Lagged(skipped)) => Some((
              Err(std::io::Error::other(format!(
                "image proxy client lagged by {skipped} chunks"
              ))),
              rx,
            )),
          }
        });
        let body = Body::from_stream(body_stream);

        let mut builder = response_builder(headers.status);
        if let Some(ct) = headers.content_type {
          builder = builder.header(header::CONTENT_TYPE, ct);
        } else {
          builder = builder.header(header::CONTENT_TYPE, "application/octet-stream");
        }
        if let Some(cl) = headers.content_length {
          builder = builder.header(header::CONTENT_LENGTH, cl);
        }
        if let Some(etag) = headers.etag {
          builder = builder.header(header::ETAG, etag);
        }
        if let Some(lm) = headers.last_modified {
          builder = builder.header(header::LAST_MODIFIED, lm);
        }

        builder
          .body(body)
          .unwrap_or_else(|_| Response::new(Body::empty()))
      }
    }
  }

  fn clone_self(&self) -> Arc<Self> {
    Arc::new(Self {
      port: self.port,
      base_url: self.base_url.clone(),
      client: self.client.clone(),
      cache: self.cache.clone(),
      config: self.config.clone(),
      gate: self.gate.clone(),
      capability: self.capability.clone(),
      coalescer: self.coalescer.clone(),
    })
  }

  async fn perform_origin_fetch(&self, remote_url: &str, partition: ImageCachePartition) {
    // Mark an active origin image fetch so the conversion worker yields.
    let _fetch_guard = FetchGateGuard::new(self.gate.clone());
    let client = self.client.clone();
    let remote_url_owned = remote_url.to_string();

    // 30-second total first-byte timeout covering GET request transmission,
    // header receipt, AND receipt of the first body chunk inside ONE deadline.
    let first_byte_res = tokio::time::timeout(Duration::from_secs(30), async move {
      let response = client
        .fetch_origin_image(&remote_url_owned)
        .await
        .map_err(|e| e.to_string())?;
      let status = response.status();
      if !status.is_success() {
        return Err(format!("origin returned status {status}"));
      }

      let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
      let content_length = response.content_length();
      let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
      let last_modified = response
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

      let headers = CoalescedHeaders {
        status,
        content_type,
        content_length,
        etag,
        last_modified,
      };

      let mut stream = response.bytes_stream();
      let first_chunk = match stream.next().await {
        Some(Ok(bytes)) => Some(bytes),
        Some(Err(err)) => return Err(format!("origin body stream error: {err}")),
        None => None,
      };

      Ok((headers, first_chunk, stream))
    })
    .await;

    let (headers, first_chunk, mut stream) = match first_byte_res {
      Ok(Ok(val)) => val,
      Ok(Err(err_msg)) => {
        let inflight = self.coalescer.lock().await.remove(remote_url);
        if let Some(inflight) = inflight {
          let _ = inflight
            .header_tx
            .send(Some(FetchResult::Error(StatusCode::BAD_GATEWAY, err_msg)));
        }
        return;
      }
      Err(_) => {
        let inflight = self.coalescer.lock().await.remove(remote_url);
        if let Some(inflight) = inflight {
          let _ = inflight.header_tx.send(Some(FetchResult::Error(
            StatusCode::GATEWAY_TIMEOUT,
            "origin first-byte timeout (send + response + first chunk exceeded 30s)".to_string(),
          )));
        }
        return;
      }
    };

    // Elect a disk writer for this miss (best-effort, non-blocking).
    let mut writer: Option<StreamWriter> = if self.cache_enabled() {
      if let Some(cache) = &self.cache {
        cache
          .try_begin_writer(
            &partition,
            remote_url,
            headers.content_type.as_deref(),
            headers.content_length,
          )
          .await
      } else {
        None
      }
    } else {
      None
    };

    // Close registration in coalescer map BEFORE broadcasting headers and first chunk.
    // This guarantees that any late-joining request will start its own fresh request from byte 0.
    let inflight = self.coalescer.lock().await.remove(remote_url);
    let Some(inflight) = inflight else { return };

    // Broadcast headers to all pre-stream waiters
    let _ = inflight.header_tx.send(Some(FetchResult::Headers(headers)));

    // Broadcast chunks without allowing a slow client to block other waiters.
    if let Some(bytes) = first_chunk {
      if let Some(w) = writer.as_mut() {
        w.try_push(bytes.clone());
      }
      if inflight.body_tx.send(Ok(bytes)).is_err() {
        return;
      }
    }

    // Stream remaining body chunks without an overall deadline.
    while let Some(chunk_res) = stream.next().await {
      match chunk_res {
        Ok(bytes) => {
          if let Some(w) = writer.as_mut() {
            w.try_push(bytes.clone());
          }
          if inflight.body_tx.send(Ok(bytes)).is_err() {
            break;
          }
        }
        Err(err) => {
          let _ = inflight.body_tx.send(Err(err.to_string()));
          break;
        }
      }
    }

    // Signal clean completion to the writer so it commits.
    if let Some(w) = writer.take() {
      w.finish();
    }
  }
}

/// Serve a cache-hit file as a streaming response, honoring `If-None-Match`.
///
/// The ETag identifies the active representation: it is derived from the
/// recorded content digest, so an original and a later AVIF produce distinct
/// validators and a stale original validator never yields a false `304`.
async fn serve_cached_file(
  reader: crate::image_cache::CacheReaderGuard,
  if_none_match: Option<String>,
) -> Response {
  let content_type = reader
    .content_type()
    .unwrap_or("application/octet-stream")
    .to_string();
  let size = reader.size_bytes();
  let path = reader.path().to_path_buf();
  let etag = reader.content_digest().map(representation_etag);

  if let (Some(etag), Some(if_none_match)) = (&etag, &if_none_match) {
    if etag_matches(etag, if_none_match) {
      return response_builder(StatusCode::NOT_MODIFIED)
        .header(header::ETAG, etag.clone())
        .body(Body::empty())
        .unwrap_or_else(|_| Response::new(Body::empty()));
    }
  }

  let file = match tokio::fs::File::open(&path).await {
    Ok(f) => f,
    Err(_) => return text_response(StatusCode::INTERNAL_SERVER_ERROR, "cache file unreadable"),
  };

  let stream = tokio_util::io::ReaderStream::new(file);
  let body = Body::from_stream(stream);

  let mut builder = response_builder(StatusCode::OK)
    .header(header::CONTENT_TYPE, content_type)
    .header(header::CONTENT_LENGTH, size);
  if let Some(etag) = etag {
    builder = builder.header(header::ETAG, etag);
  }
  builder
    .body(body)
    .unwrap_or_else(|_| Response::new(Body::empty()))
}

/// Build a strong, representation-specific ETag from a content digest.
fn representation_etag(digest: &str) -> String {
  format!("\"v1-{digest}\"")
}

/// Compare a strong ETag against an `If-None-Match` header value (`*` or a
/// comma-separated list of validators).
fn etag_matches(etag: &str, if_none_match: &str) -> bool {
  if if_none_match.trim() == "*" {
    return true;
  }
  if_none_match
    .split(',')
    .any(|candidate| candidate.trim() == etag)
}

fn response_builder(status: StatusCode) -> axum::http::response::Builder {
  Response::builder()
    .status(status)
    .header(header::CACHE_CONTROL, "public, max-age=0, must-revalidate")
    .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
    .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
    .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
}

fn text_response(status: StatusCode, message: impl Into<String>) -> Response {
  Response::builder()
    .status(status)
    .header(header::CACHE_CONTROL, "no-store")
    .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
    .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
    .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(Body::from(message.into()))
    .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::image_ref::{image_id_for_url, ImageRefKind};
  use crate::jellyfin::MediaServerProvider;
  use reqwest::Client as ReqwestClient;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use tokio::net::TcpListener as TokioTcpListener;

  fn test_config() -> Arc<RwLock<AppConfig>> {
    Arc::new(RwLock::new(AppConfig::default()))
  }

  fn test_gate() -> Arc<crate::avif_worker::ForegroundGate> {
    Arc::new(crate::avif_worker::ForegroundGate::new())
  }
  fn test_capability() -> crate::avif_worker::AvifCapability {
    crate::avif_worker::AvifCapability::new()
  }

  #[tokio::test]
  async fn test_bind_and_base_url_surface() {
    let client = Arc::new(JellyfinClient::new());
    let proxy = ImageProxy::start(client, None, test_config(), test_gate(), test_capability())
      .expect("should bind port");
    assert!(proxy.base_url.starts_with("http://127.0.0.1:"));

    let state = ImageProxyState::new();
    state.install(Ok(proxy.clone()));
    let services = state.local_services();
    assert_eq!(services.image_proxy_base, Some(proxy.base_url.clone()));
  }

  #[tokio::test]
  async fn test_forced_bind_failure_null_service_state() {
    let held_listener = TcpListener::bind("127.0.0.1:0").expect("bind held listener");
    let held_port = held_listener.local_addr().expect("addr").port();

    let client = Arc::new(JellyfinClient::new());
    let bind_res = ImageProxy::start_on_addr(
      &format!("127.0.0.1:{held_port}"),
      client,
      None,
      test_config(),
      test_gate(),
      test_capability(),
    );
    assert!(bind_res.is_err());

    let state_failed = ImageProxyState::new();
    state_failed.install(bind_res);
    let failed_services = state_failed.local_services();
    assert_eq!(failed_services.image_proxy_base, None);
  }

  #[tokio::test]
  async fn test_bad_token_route_and_method() {
    let client = Arc::new(JellyfinClient::new());
    let proxy = ImageProxy::start(client, None, test_config(), test_gate(), test_capability())
      .expect("start proxy");

    let http = ReqwestClient::new();
    let bad_url = format!("{}/image/invalid-token-12345", proxy.base_url);

    let resp = http.get(&bad_url).send().await.expect("send get");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
      resp.headers().get(header::CACHE_CONTROL),
      Some(&header::HeaderValue::from_static("no-store"))
    );

    let post_resp = http.post(&bad_url).send().await.expect("send post");
    assert_eq!(post_resp.status(), StatusCode::METHOD_NOT_ALLOWED);

    let non_image_url = format!("{}/invalid-route", proxy.base_url);
    let not_found_resp = http
      .get(&non_image_url)
      .send()
      .await
      .expect("send get non-image");
    assert_eq!(not_found_resp.status(), StatusCode::NOT_FOUND);
  }

  #[tokio::test]
  async fn test_auth_mismatch_and_provider_server_mismatch() {
    let client = Arc::new(JellyfinClient::new());
    let proxy = ImageProxy::start(
      client.clone(),
      None,
      test_config(),
      test_gate(),
      test_capability(),
    )
    .expect("start proxy");

    let server_url = "http://127.0.0.1:9999";
    let token = image_id_for_url(
      MediaServerProvider::Jellyfin,
      server_url,
      format!("{}/Items/123/Images/Primary", server_url),
      ImageRefKind::Artwork,
    )
    .expect("sign token");

    let http = ReqwestClient::new();
    let req_url = format!("{}/image/{}", proxy.base_url, token);

    // Case 1: Disconnected -> 401
    let resp = http.get(&req_url).send().await.expect("send get");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Case 2: Adopt session with provider mismatch (Emby token vs Jellyfin client)
    let emby_token = image_id_for_url(
      MediaServerProvider::Emby,
      server_url,
      format!("{}/Items/123/Images/Primary", server_url),
      ImageRefKind::Artwork,
    )
    .expect("sign emby token");
    client
      .login()
      .adopt_validated_session(&crate::jellyfin::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: server_url.to_string(),
        access_token: "test-token".to_string(),
        user_id: "user-1".to_string(),
        user_name: "Test".to_string(),
        server_name: Some("Mock Server".to_string()),
        device_id: None,
      });

    let emby_req_url = format!("{}/image/{}", proxy.base_url, emby_token);
    let resp_mismatch = http.get(&emby_req_url).send().await.expect("send emby req");
    assert_eq!(resp_mismatch.status(), StatusCode::FORBIDDEN);

    // Case 3: Server URL mismatch (token signed for server_url vs client logged into different_url)
    let different_url = "http://127.0.0.1:8888";
    let diff_token = image_id_for_url(
      MediaServerProvider::Jellyfin,
      different_url,
      format!("{}/Items/123/Images/Primary", different_url),
      ImageRefKind::Artwork,
    )
    .expect("sign diff token");
    let diff_req_url = format!("{}/image/{}", proxy.base_url, diff_token);
    let resp_server_mismatch = http.get(&diff_req_url).send().await.expect("send diff req");
    assert_eq!(resp_server_mismatch.status(), StatusCode::FORBIDDEN);
  }

  #[tokio::test]
  async fn test_success_streaming_headers_cors_and_coalescing() {
    let listener = TokioTcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind mock server");
    let origin_port = listener.local_addr().expect("port").port();
    let origin_base = format!("http://127.0.0.1:{origin_port}");

    let origin_hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = origin_hits.clone();

    let mock_app = Router::new().route(
      "/Items/img1/Images/Primary",
      get(move || {
        let hits = hits_clone.clone();
        async move {
          hits.fetch_add(1, Ordering::SeqCst);
          tokio::time::sleep(Duration::from_millis(50)).await;
          (
            [
              (header::CONTENT_TYPE, "image/jpeg"),
              (header::ETAG, "\"test-etag\""),
            ],
            "fake-image-binary-data",
          )
        }
      }),
    );

    tokio::spawn(async move {
      let _ = axum::serve(listener, mock_app).await;
    });

    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&crate::jellyfin::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: origin_base.clone(),
        access_token: "test-token".to_string(),
        user_id: "user-1".to_string(),
        user_name: "Test".to_string(),
        server_name: Some("Mock Server".to_string()),
        device_id: None,
      });

    let proxy = ImageProxy::start(client, None, test_config(), test_gate(), test_capability())
      .expect("start proxy");

    let remote_url = format!("{origin_base}/Items/img1/Images/Primary");
    let signed_token = image_id_for_url(
      MediaServerProvider::Jellyfin,
      &origin_base,
      remote_url,
      ImageRefKind::Artwork,
    )
    .expect("sign token");

    let proxy_req_url = format!("{}/image/{}", proxy.base_url, signed_token);
    let http = ReqwestClient::new();

    // 1. Test single request for streaming, headers, and CORS
    let single_resp = http.get(&proxy_req_url).send().await.expect("send get");
    assert_eq!(single_resp.status(), StatusCode::OK);
    assert_eq!(
      single_resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap(),
      "image/jpeg"
    );
    assert_eq!(
      single_resp
        .headers()
        .get(header::CACHE_CONTROL)
        .unwrap()
        .to_str()
        .unwrap(),
      "public, max-age=0, must-revalidate"
    );
    assert_eq!(
      single_resp
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .unwrap()
        .to_str()
        .unwrap(),
      "*"
    );
    assert!(single_resp.headers().get(header::AUTHORIZATION).is_none());

    let body_text = single_resp.text().await.expect("read body");
    assert_eq!(body_text, "fake-image-binary-data");

    // 2. Test request coalescing with 5 concurrent requests
    origin_hits.store(0, Ordering::SeqCst);
    let mut tasks = Vec::new();
    for _ in 0..5 {
      let client = http.clone();
      let url = proxy_req_url.clone();
      tasks.push(tokio::spawn(async move {
        let resp = client.get(&url).send().await.expect("coalesced req");
        assert_eq!(resp.status(), StatusCode::OK);
        let text = resp.text().await.expect("coalesced body");
        assert_eq!(text, "fake-image-binary-data");
      }));
    }

    for task in tasks {
      task.await.expect("task join");
    }

    // Coalescing assertion: 5 concurrent requests hit origin exactly 1 time!
    assert_eq!(origin_hits.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn test_cache_miss_then_hit_serves_from_disk() {
    let dir = std::env::temp_dir().join(format!("proxy_cache_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);

    let listener = TokioTcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind mock server");
    let origin_port = listener.local_addr().expect("port").port();
    let origin_base = format!("http://127.0.0.1:{origin_port}");

    let origin_hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = origin_hits.clone();

    let mock_app = Router::new().route(
      "/Items/cached/Images/Primary",
      get(move || {
        let hits = hits_clone.clone();
        async move {
          hits.fetch_add(1, Ordering::SeqCst);
          ([(header::CONTENT_TYPE, "image/png")], "cached-png-bytes")
        }
      }),
    );

    tokio::spawn(async move {
      let _ = axum::serve(listener, mock_app).await;
    });

    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&crate::jellyfin::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: origin_base.clone(),
        access_token: "test-token".to_string(),
        user_id: "user-1".to_string(),
        user_name: "Test".to_string(),
        server_name: Some("Mock Server".to_string()),
        device_id: None,
      });

    let cache =
      crate::image_cache::ImageCache::init(dir.clone(), crate::image_cache::IMAGE_CACHE_MAX_BYTES)
        .await
        .expect("init cache");

    let proxy = ImageProxy::start(
      client,
      Some(cache),
      test_config(),
      test_gate(),
      test_capability(),
    )
    .expect("start proxy");

    let remote_url = format!("{origin_base}/Items/cached/Images/Primary");
    let signed_token = image_id_for_url(
      MediaServerProvider::Jellyfin,
      &origin_base,
      remote_url,
      ImageRefKind::Artwork,
    )
    .expect("sign token");

    let proxy_req_url = format!("{}/image/{}", proxy.base_url, signed_token);
    let http = ReqwestClient::new();

    // First request: miss, streams from origin, writes to cache.
    let resp1 = http.get(&proxy_req_url).send().await.expect("first req");
    assert_eq!(resp1.status(), StatusCode::OK);
    let body1 = resp1.text().await.expect("body1");
    assert_eq!(body1, "cached-png-bytes");
    assert_eq!(origin_hits.load(Ordering::SeqCst), 1);

    // Wait for the background writer to commit.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Second request: hit, served from disk without touching origin.
    let resp2 = http.get(&proxy_req_url).send().await.expect("second req");
    assert_eq!(resp2.status(), StatusCode::OK);
    let body2 = resp2.text().await.expect("body2");
    assert_eq!(body2, "cached-png-bytes");
    assert_eq!(
      origin_hits.load(Ordering::SeqCst),
      1,
      "cache hit must not touch origin"
    );

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[tokio::test]
  async fn test_disabled_cache_bypasses_reads_and_writes() {
    let dir = std::env::temp_dir().join(format!("proxy_disabled_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);

    let listener = TokioTcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind mock server");
    let origin_port = listener.local_addr().expect("port").port();
    let origin_base = format!("http://127.0.0.1:{origin_port}");

    let origin_hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = origin_hits.clone();

    let mock_app = Router::new().route(
      "/Items/disabled/Images/Primary",
      get(move || {
        let hits = hits_clone.clone();
        async move {
          hits.fetch_add(1, Ordering::SeqCst);
          (
            [(header::CONTENT_TYPE, "image/jpeg")],
            "disabled-cache-bytes",
          )
        }
      }),
    );

    tokio::spawn(async move {
      let _ = axum::serve(listener, mock_app).await;
    });

    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&crate::jellyfin::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: origin_base.clone(),
        access_token: "test-token".to_string(),
        user_id: "user-1".to_string(),
        user_name: "Test".to_string(),
        server_name: Some("Mock Server".to_string()),
        device_id: None,
      });

    let cache =
      crate::image_cache::ImageCache::init(dir.clone(), crate::image_cache::IMAGE_CACHE_MAX_BYTES)
        .await
        .expect("init cache");

    let config = test_config();
    config.write().image_disk_cache_enabled = false;

    let proxy = ImageProxy::start(client, Some(cache), config, test_gate(), test_capability())
      .expect("start proxy");

    let remote_url = format!("{origin_base}/Items/disabled/Images/Primary");
    let signed_token = image_id_for_url(
      MediaServerProvider::Jellyfin,
      &origin_base,
      remote_url,
      ImageRefKind::Artwork,
    )
    .expect("sign token");

    let proxy_req_url = format!("{}/image/{}", proxy.base_url, signed_token);
    let http = ReqwestClient::new();

    // Both requests hit origin because cache is disabled.
    let resp1 = http.get(&proxy_req_url).send().await.expect("req1");
    assert_eq!(resp1.status(), StatusCode::OK);
    let _ = resp1.text().await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let resp2 = http.get(&proxy_req_url).send().await.expect("req2");
    assert_eq!(resp2.status(), StatusCode::OK);
    let _ = resp2.text().await;

    assert_eq!(
      origin_hits.load(Ordering::SeqCst),
      2,
      "disabled cache must not serve from disk"
    );

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[tokio::test]
  async fn test_etag_revalidation_304_and_stale_validator() {
    let dir = std::env::temp_dir().join(format!("proxy_etag_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);

    let listener = TokioTcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind mock server");
    let origin_port = listener.local_addr().expect("port").port();
    let origin_base = format!("http://127.0.0.1:{origin_port}");

    let mock_app = Router::new().route(
      "/Items/etag/Images/Primary",
      get(|| async { ([(header::CONTENT_TYPE, "image/png")], "etag-png-bytes") }),
    );
    tokio::spawn(async move {
      let _ = axum::serve(listener, mock_app).await;
    });

    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&crate::jellyfin::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: origin_base.clone(),
        access_token: "test-token".to_string(),
        user_id: "user-1".to_string(),
        user_name: "Test".to_string(),
        server_name: Some("Mock Server".to_string()),
        device_id: None,
      });

    let cache =
      crate::image_cache::ImageCache::init(dir.clone(), crate::image_cache::IMAGE_CACHE_MAX_BYTES)
        .await
        .expect("init cache");
    let proxy = ImageProxy::start(
      client,
      Some(cache),
      test_config(),
      test_gate(),
      test_capability(),
    )
    .expect("start proxy");

    let remote_url = format!("{origin_base}/Items/etag/Images/Primary");
    let signed_token = image_id_for_url(
      MediaServerProvider::Jellyfin,
      &origin_base,
      remote_url,
      ImageRefKind::Artwork,
    )
    .expect("sign token");
    let proxy_req_url = format!("{}/image/{}", proxy.base_url, signed_token);
    let http = ReqwestClient::new();

    // Prime the cache (miss -> origin -> commit).
    let prime = http.get(&proxy_req_url).send().await.expect("prime");
    assert_eq!(prime.status(), StatusCode::OK);
    let _ = prime.text().await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Cache hit: 200 with a strong, representation-specific ETag and CORS.
    let hit = http.get(&proxy_req_url).send().await.expect("hit");
    assert_eq!(hit.status(), StatusCode::OK);
    assert_eq!(
      hit
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .unwrap(),
      "*"
    );
    let etag = hit
      .headers()
      .get(header::ETAG)
      .expect("cache hit must carry an ETag")
      .to_str()
      .unwrap()
      .to_string();
    assert!(
      etag.starts_with("\"v1-"),
      "ETag must be strong and versioned: {etag}"
    );
    let body = hit.text().await.expect("hit body");
    assert_eq!(body, "etag-png-bytes");

    // Matching If-None-Match -> 304 with no body but ETag + CORS intact.
    let not_modified = http
      .get(&proxy_req_url)
      .header(header::IF_NONE_MATCH, &etag)
      .send()
      .await
      .expect("conditional");
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
      not_modified
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap(),
      etag
    );
    assert_eq!(
      not_modified
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .unwrap(),
      "*"
    );
    let nm_body = not_modified.text().await.expect("304 body");
    assert!(nm_body.is_empty(), "304 must not carry a body");

    // A stale validator (different digest) must NOT 304; it returns the bytes.
    let stale = "\"v1-0000000000000000000000000000000000000000000000000000000000000000\"";
    let stale_resp = http
      .get(&proxy_req_url)
      .header(header::IF_NONE_MATCH, stale)
      .send()
      .await
      .expect("stale conditional");
    assert_eq!(stale_resp.status(), StatusCode::OK);
    assert_eq!(
      stale_resp
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap(),
      etag,
      "stale validator must return the current representation's ETag"
    );
    let stale_body = stale_resp.text().await.expect("stale body");
    assert_eq!(stale_body, "etag-png-bytes");

    let _ = std::fs::remove_dir_all(&dir);
  }

  fn test_capability_supported() -> crate::avif_worker::AvifCapability {
    let cap = crate::avif_worker::AvifCapability::new();
    cap.set_supported(true);
    cap
  }

  // Seed an entry whose active representation is AVIF (file on disk + catalog
  // switched), mirroring a completed background conversion.
  async fn seed_avif_active(
    cache: &Arc<crate::image_cache::ImageCache>,
    server_url: &str,
    url: &str,
  ) -> (String, Vec<u8>) {
    let partition =
      crate::image_cache::ImageCache::partition(MediaServerProvider::Jellyfin, server_url);
    let origin_body = b"original-jpeg-bytes".to_vec();
    let mut writer = cache
      .try_begin_writer(
        &partition,
        url,
        Some("image/jpeg"),
        Some(origin_body.len() as u64),
      )
      .await
      .expect("writer admitted");
    assert!(writer.try_push(bytes::Bytes::copy_from_slice(&origin_body)));
    writer.finish();
    let reader = loop {
      if let Some(r) = cache.open_reader(&partition, url).await {
        break r;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let original_name = reader
      .path()
      .file_name()
      .unwrap()
      .to_str()
      .unwrap()
      .to_string();
    drop(reader);

    let avif_name = crate::image_cache::ImageCache::avif_file_name_for(&original_name);
    let avif_bytes = b"fake-avif-bytes-for-proxy".to_vec();
    tokio::fs::write(cache.path_for(&avif_name), &avif_bytes)
      .await
      .unwrap();
    let cache_key = crate::image_cache::ImageCache::cache_key(&partition, url);
    cache
      .activate_avif(
        &cache_key,
        &avif_name,
        avif_bytes.len() as u64,
        "abc123",
        "image/avif",
      )
      .await
      .unwrap();
    (cache_key, avif_bytes)
  }

  #[tokio::test]
  async fn test_unsupported_capability_refetches_avif_only_entry() {
    let dir = std::env::temp_dir().join(format!("proxy_cap_unsup_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);

    let listener = TokioTcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind mock");
    let origin_port = listener.local_addr().expect("port").port();
    let origin_base = format!("http://127.0.0.1:{origin_port}");
    let origin_hits = Arc::new(AtomicUsize::new(0));
    let hits = origin_hits.clone();
    let mock_app = Router::new().route(
      "/Items/cap/Images/Primary",
      get(move || {
        let hits = hits.clone();
        async move {
          hits.fetch_add(1, Ordering::SeqCst);
          (
            [(header::CONTENT_TYPE, "image/jpeg")],
            "refetched-origin-bytes",
          )
        }
      }),
    );
    tokio::spawn(async move {
      let _ = axum::serve(listener, mock_app).await;
    });

    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&crate::jellyfin::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: origin_base.clone(),
        access_token: "t".into(),
        user_id: "u".into(),
        user_name: "T".into(),
        server_name: None,
        device_id: None,
      });

    let cache =
      crate::image_cache::ImageCache::init(dir.clone(), crate::image_cache::IMAGE_CACHE_MAX_BYTES)
        .await
        .expect("init cache");
    let remote_url = format!("{origin_base}/Items/cap/Images/Primary");
    let _ = seed_avif_active(&cache, &origin_base, &remote_url).await;

    // Capability defaults to unsupported: an AVIF-active entry must NOT be
    // served; the proxy re-fetches the origin instead.
    let proxy = ImageProxy::start(
      client,
      Some(cache),
      test_config(),
      test_gate(),
      test_capability(),
    )
    .expect("start proxy");

    let signed = image_id_for_url(
      MediaServerProvider::Jellyfin,
      &origin_base,
      remote_url.clone(),
      ImageRefKind::Artwork,
    )
    .expect("sign");
    let url = format!("{}/image/{}", proxy.base_url, signed);
    let http = ReqwestClient::new();
    let resp = http.get(&url).send().await.expect("req");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
      resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok()),
      Some("image/jpeg"),
      "unsupported platform must receive the re-fetched origin, not AVIF"
    );
    let body = resp.text().await.expect("body");
    assert_eq!(body, "refetched-origin-bytes");
    assert_eq!(
      origin_hits.load(Ordering::SeqCst),
      1,
      "unsupported capability must trigger an origin re-fetch"
    );

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[tokio::test]
  async fn test_supported_capability_serves_cached_avif() {
    let dir = std::env::temp_dir().join(format!("proxy_cap_sup_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);

    let listener = TokioTcpListener::bind("127.0.0.1:0")
      .await
      .expect("bind mock");
    let origin_port = listener.local_addr().expect("port").port();
    let origin_base = format!("http://127.0.0.1:{origin_port}");
    let origin_hits = Arc::new(AtomicUsize::new(0));
    let hits = origin_hits.clone();
    let mock_app = Router::new().route(
      "/Items/cap2/Images/Primary",
      get(move || {
        let hits = hits.clone();
        async move {
          hits.fetch_add(1, Ordering::SeqCst);
          ([(header::CONTENT_TYPE, "image/jpeg")], "should-not-serve")
        }
      }),
    );
    tokio::spawn(async move {
      let _ = axum::serve(listener, mock_app).await;
    });

    let client = Arc::new(JellyfinClient::new());
    client
      .login()
      .adopt_validated_session(&crate::jellyfin::SavedSession {
        provider: MediaServerProvider::Jellyfin,
        server_url: origin_base.clone(),
        access_token: "t".into(),
        user_id: "u".into(),
        user_name: "T".into(),
        server_name: None,
        device_id: None,
      });

    let cache =
      crate::image_cache::ImageCache::init(dir.clone(), crate::image_cache::IMAGE_CACHE_MAX_BYTES)
        .await
        .expect("init cache");
    let remote_url = format!("{origin_base}/Items/cap2/Images/Primary");
    let (_key, avif_bytes) = seed_avif_active(&cache, &origin_base, &remote_url).await;

    let proxy = ImageProxy::start(
      client,
      Some(cache),
      test_config(),
      test_gate(),
      test_capability_supported(),
    )
    .expect("start proxy");

    let signed = image_id_for_url(
      MediaServerProvider::Jellyfin,
      &origin_base,
      remote_url.clone(),
      ImageRefKind::Artwork,
    )
    .expect("sign");
    let url = format!("{}/image/{}", proxy.base_url, signed);
    let http = ReqwestClient::new();
    let resp = http.get(&url).send().await.expect("req");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
      resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok()),
      Some("image/avif"),
      "supported platform must receive the cached AVIF"
    );
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), avif_bytes.as_slice());
    assert_eq!(
      origin_hits.load(Ordering::SeqCst),
      0,
      "supported capability must serve from cache without origin"
    );

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn representation_etag_is_digest_specific() {
    let a = representation_etag("aaaa");
    let b = representation_etag("bbbb");
    assert_ne!(a, b, "distinct digests must yield distinct validators");
    assert!(
      a.starts_with('"') && a.ends_with('"'),
      "ETag must be strong (quoted)"
    );
    assert!(!a.contains("W/"), "ETag must not be weak");
  }

  #[test]
  fn etag_matches_handles_wildcard_list_and_mismatch() {
    let etag = "\"v1-abc\"";
    assert!(etag_matches(etag, "*"));
    assert!(etag_matches(etag, "\"v1-abc\""));
    assert!(etag_matches(etag, "\"v1-zzz\", \"v1-abc\""));
    assert!(!etag_matches(etag, "\"v1-other\""));
    assert!(!etag_matches(etag, ""));
  }
}
