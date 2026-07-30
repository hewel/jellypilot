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

pub struct ImageProxyInner {
  pub port: u16,
  pub base_url: String,
  pub client: Arc<JellyfinClient>,
  cache: Option<Arc<ImageCache>>,
  config: Arc<RwLock<AppConfig>>,
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
  ) -> Result<Arc<Self>, ImageProxyError> {
    Self::start_on_addr("127.0.0.1:0", client, cache, config)
  }

  pub fn start_on_addr(
    addr: &str,
    client: Arc<JellyfinClient>,
    cache: Option<Arc<ImageCache>>,
    config: Arc<RwLock<AppConfig>>,
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

  let partition = ImageCache::partition(payload.provider, server_url);
  inner.serve_image(&payload.remote_url, partition).await
}

impl ImageProxyInner {
  fn cache_enabled(&self) -> bool {
    self.config.read().image_disk_cache_enabled
  }

  async fn serve_image(&self, remote_url: &str, partition: ImageCachePartition) -> Response {
    // Cache hit: serve from disk without touching the origin.
    if self.cache_enabled() {
      if let Some(cache) = &self.cache {
        if let Some(reader) = cache.open_reader(&partition, remote_url).await {
          return serve_cached_file(reader).await;
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
      coalescer: self.coalescer.clone(),
    })
  }

  async fn perform_origin_fetch(&self, remote_url: &str, partition: ImageCachePartition) {
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

/// Serve a cache-hit file as a streaming response.
async fn serve_cached_file(reader: crate::image_cache::CacheReaderGuard) -> Response {
  let content_type = reader
    .content_type()
    .unwrap_or("application/octet-stream")
    .to_string();
  let size = reader.size_bytes();
  let path = reader.path().to_path_buf();

  let file = match tokio::fs::File::open(&path).await {
    Ok(f) => f,
    Err(_) => return text_response(StatusCode::INTERNAL_SERVER_ERROR, "cache file unreadable"),
  };

  let stream = tokio_util::io::ReaderStream::new(file);
  let body = Body::from_stream(stream);

  response_builder(StatusCode::OK)
    .header(header::CONTENT_TYPE, content_type)
    .header(header::CONTENT_LENGTH, size)
    .body(body)
    .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn response_builder(status: StatusCode) -> axum::http::response::Builder {
  Response::builder()
    .status(status)
    .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
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

  #[tokio::test]
  async fn test_bind_and_base_url_surface() {
    let client = Arc::new(JellyfinClient::new());
    let proxy = ImageProxy::start(client, None, test_config()).expect("should bind port");
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
    let proxy = ImageProxy::start(client, None, test_config()).expect("start proxy");

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
    let proxy = ImageProxy::start(client.clone(), None, test_config()).expect("start proxy");

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

    let proxy = ImageProxy::start(client, None, test_config()).expect("start proxy");

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
      "public, max-age=31536000, immutable"
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

    let proxy = ImageProxy::start(client, Some(cache), test_config()).expect("start proxy");

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

    let proxy = ImageProxy::start(client, Some(cache), config).expect("start proxy");

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
}
