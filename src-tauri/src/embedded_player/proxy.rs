use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{
  ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE,
  CONTENT_TYPE, ORIGIN, RANGE,
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use parking_lot::RwLock;
use tokio_util::io::ReaderStream;

use super::EmbeddedPlayerError;

#[derive(Clone)]
struct ProxySession {
  source_nonce: String,
  hls_nonce: String,
  upstream_url: String,
  output_dir: PathBuf,
}

#[derive(Clone)]
struct ProxyState {
  client: reqwest::Client,
  active: Arc<RwLock<Option<ProxySession>>>,
  diagnostic: Arc<RwLock<SourceProxyDiagnostic>>,
}

#[derive(Default)]
struct SourceProxyDiagnostic {
  started_at: Option<Instant>,
  has_range: bool,
  summary: String,
}

impl SourceProxyDiagnostic {
  fn reset(&mut self) {
    self.started_at = None;
    self.has_range = false;
    self.summary = "source proxy received no request".to_string();
  }

  fn started(&mut self, method: &Method, has_range: bool) {
    self.started_at = Some(Instant::now());
    self.has_range = has_range;
    self.summary = format!(
      "source proxy {method} request pending (range: {})",
      if has_range { "yes" } else { "no" }
    );
  }

  fn completed(&mut self, status: StatusCode) {
    self.summary = format!(
      "source proxy returned HTTP {} after {} ms (range: {})",
      status.as_u16(),
      self.elapsed_millis(),
      if self.has_range { "yes" } else { "no" },
    );
  }

  fn failed(&mut self) {
    self.summary = format!(
      "source proxy could not reach the provider after {} ms",
      self.elapsed_millis()
    );
  }

  fn elapsed_millis(&self) -> u128 {
    self
      .started_at
      .map_or(0, |started| started.elapsed().as_millis())
  }
}

/// One loopback server with independently authorized source and HLS routes.
pub(super) struct LoopbackMediaServer {
  base_url: String,
  active: Arc<RwLock<Option<ProxySession>>>,
  diagnostic: Arc<RwLock<SourceProxyDiagnostic>>,
}

impl LoopbackMediaServer {
  pub(super) async fn start() -> Result<Self, EmbeddedPlayerError> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
      .await
      .map_err(EmbeddedPlayerError::LoopbackBind)?;
    let address = listener
      .local_addr()
      .map_err(EmbeddedPlayerError::LoopbackBind)?;
    let active = Arc::new(RwLock::new(None));
    let diagnostic = Arc::new(RwLock::new(SourceProxyDiagnostic::default()));
    let state = ProxyState {
      client: reqwest::Client::new(),
      active: Arc::clone(&active),
      diagnostic: Arc::clone(&diagnostic),
    };
    let router = Router::new()
      .route("/source/{nonce}", get(proxy_source).head(proxy_source))
      .route("/hls/{nonce}/{*file}", get(serve_hls_file))
      .with_state(state);

    tauri::async_runtime::spawn(async move {
      if let Err(error) = axum::serve(listener, router).await {
        log::error!("Embedded media loopback server stopped: {error}");
      }
    });

    Ok(Self {
      base_url: format!("http://{address}"),
      active,
      diagnostic,
    })
  }

  pub(super) fn activate(
    &self,
    source_nonce: String,
    hls_nonce: String,
    upstream_url: String,
    output_dir: PathBuf,
  ) {
    self.diagnostic.write().reset();
    *self.active.write() = Some(ProxySession {
      source_nonce,
      hls_nonce,
      upstream_url,
      output_dir,
    });
  }

  pub(super) fn revoke(&self) {
    self.active.write().take();
  }

  pub(super) fn source_url(&self, nonce: &str) -> String {
    format!("{}/source/{nonce}", self.base_url)
  }

  pub(super) fn playlist_url(&self, nonce: &str) -> String {
    format!("{}/hls/{nonce}/master.m3u8", self.base_url)
  }

  pub(super) fn source_diagnostic_summary(&self) -> String {
    self.diagnostic.read().summary.clone()
  }
}

async fn proxy_source(
  State(state): State<ProxyState>,
  Path(nonce): Path<String>,
  request: Request<Body>,
) -> Response {
  if request.headers().contains_key(ORIGIN) {
    return StatusCode::FORBIDDEN.into_response();
  }
  let Some(session) = authorized_source_session(&state, &nonce) else {
    return StatusCode::NOT_FOUND.into_response();
  };
  let method = match *request.method() {
    Method::GET => reqwest::Method::GET,
    Method::HEAD => reqwest::Method::HEAD,
    _ => return StatusCode::METHOD_NOT_ALLOWED.into_response(),
  };
  let mut upstream = source_upstream_request(&state.client, method, &session);
  let range = request.headers().get(RANGE);
  state
    .diagnostic
    .write()
    .started(request.method(), range.is_some());
  if let Some(range) = range {
    upstream = upstream.header(RANGE, range);
  }

  let upstream = match upstream.send().await {
    Ok(response) => response,
    Err(_) => {
      // reqwest errors can include the authenticated upstream URL. Never log it.
      state.diagnostic.write().failed();
      log::warn!("Embedded source proxy request failed");
      return StatusCode::BAD_GATEWAY.into_response();
    }
  };
  let status = upstream.status();
  state.diagnostic.write().completed(status);
  let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
  let content_length = upstream.headers().get(CONTENT_LENGTH).cloned();
  let content_range = upstream.headers().get(CONTENT_RANGE).cloned();
  let accept_ranges = upstream.headers().get(ACCEPT_RANGES).cloned();
  let mut response = Response::new(if request.method() == Method::HEAD {
    Body::empty()
  } else {
    Body::from_stream(upstream.bytes_stream())
  });
  *response.status_mut() = status;
  insert_header(response.headers_mut(), CONTENT_TYPE, content_type);
  insert_header(response.headers_mut(), CONTENT_LENGTH, content_length);
  insert_header(response.headers_mut(), CONTENT_RANGE, content_range);
  insert_header(response.headers_mut(), ACCEPT_RANGES, accept_ranges);
  response
}

fn source_upstream_request(
  client: &reqwest::Client,
  method: reqwest::Method,
  session: &ProxySession,
) -> reqwest::RequestBuilder {
  // Some Emby deployments gate the static stream route by the media client
  // identity. Match the identity used by the working external MPV path while
  // keeping the authenticated provider URL hidden behind the loopback proxy.
  client
    .request(method, session.upstream_url.clone())
    .header(reqwest::header::USER_AGENT, "libmpv")
}

async fn serve_hls_file(
  State(state): State<ProxyState>,
  Path((nonce, file)): Path<(String, String)>,
  headers: HeaderMap,
) -> Response {
  if !origin_allowed(&headers) {
    return StatusCode::FORBIDDEN.into_response();
  }
  let Some(session) = authorized_hls_session(&state, &nonce) else {
    return StatusCode::NOT_FOUND.into_response();
  };
  let Some(content_type) = hls_content_type(&file) else {
    return StatusCode::NOT_FOUND.into_response();
  };
  let path = session.output_dir.join(&file);
  let (file_handle, metadata) =
    match tokio::try_join!(tokio::fs::File::open(&path), tokio::fs::metadata(&path)) {
      Ok(result) => result,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
        return StatusCode::NOT_FOUND.into_response();
      }
      Err(error) => {
        log::warn!(
          "Failed to serve embedded HLS file {}: {error}",
          path.display()
        );
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
      }
    };
  if !metadata.is_file() {
    return StatusCode::NOT_FOUND.into_response();
  }

  let stream = ReaderStream::new(file_handle);
  let mut response = Response::new(Body::from_stream(stream));
  response
    .headers_mut()
    .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
  response.headers_mut().insert(
    CONTENT_LENGTH,
    HeaderValue::from_str(&metadata.len().to_string())
      .unwrap_or_else(|_| HeaderValue::from_static("0")),
  );
  response
    .headers_mut()
    .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
  if let Some(origin) = headers.get(ORIGIN).cloned() {
    response
      .headers_mut()
      .insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
  }
  response
}

fn authorized_source_session(state: &ProxyState, nonce: &str) -> Option<ProxySession> {
  let active = state.active.read();
  active
    .as_ref()
    .filter(|session| session.source_nonce == nonce)
    .cloned()
}

fn authorized_hls_session(state: &ProxyState, nonce: &str) -> Option<ProxySession> {
  let active = state.active.read();
  active
    .as_ref()
    .filter(|session| session.hls_nonce == nonce)
    .cloned()
}

fn origin_allowed(headers: &HeaderMap) -> bool {
  let Some(origin) = headers.get(ORIGIN) else {
    return false;
  };
  let Ok(origin) = origin.to_str() else {
    return false;
  };
  matches!(
    origin,
    "tauri://localhost"
      | "http://tauri.localhost"
      | "https://tauri.localhost"
      | "http://localhost:3000"
      | "http://127.0.0.1:3000"
  )
}

fn hls_content_type(file: &str) -> Option<&'static str> {
  if file == "master.m3u8" {
    return Some("application/vnd.apple.mpegurl");
  }
  if file == "init.mp4" {
    return Some("video/mp4");
  }
  let segment_number = file.strip_prefix("segment_")?.strip_suffix(".m4s")?;
  if !segment_number.is_empty() && segment_number.bytes().all(|byte| byte.is_ascii_digit()) {
    Some("video/iso.segment")
  } else {
    None
  }
}

fn insert_header(target: &mut HeaderMap, name: axum::http::HeaderName, value: Option<HeaderValue>) {
  if let Some(value) = value {
    target.insert(name, value);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hls_file_allowlist_rejects_traversal_and_unknown_files() {
    assert_eq!(hls_content_type("../master.m3u8"), None);
    assert_eq!(hls_content_type("segment_x.m4s"), None);
  }

  #[test]
  fn hls_file_allowlist_accepts_only_generated_artifacts() {
    assert_eq!(
      hls_content_type("master.m3u8"),
      Some("application/vnd.apple.mpegurl")
    );
    assert_eq!(
      hls_content_type("segment_000042.m4s"),
      Some("video/iso.segment")
    );
  }

  #[test]
  fn source_and_hls_nonces_do_not_share_authority() {
    let state = ProxyState {
      client: reqwest::Client::new(),
      active: Arc::new(RwLock::new(Some(ProxySession {
        source_nonce: "source-only".to_string(),
        hls_nonce: "browser-only".to_string(),
        upstream_url: "https://media.invalid/source".to_string(),
        output_dir: PathBuf::from("/tmp/embedded-authority-test"),
      }))),
      diagnostic: Arc::new(RwLock::new(SourceProxyDiagnostic::default())),
    };

    assert!(authorized_source_session(&state, "source-only").is_some());
    assert!(authorized_source_session(&state, "browser-only").is_none());
    assert!(authorized_hls_session(&state, "browser-only").is_some());
    assert!(authorized_hls_session(&state, "source-only").is_none());
  }

  #[test]
  fn hls_origin_allowlist_rejects_missing_and_unknown_origins() {
    assert!(!origin_allowed(&HeaderMap::new()));
    let mut headers = HeaderMap::new();
    headers.insert(ORIGIN, HeaderValue::from_static("https://attacker.invalid"));
    assert!(!origin_allowed(&headers));
    headers.insert(ORIGIN, HeaderValue::from_static("tauri://localhost"));
    assert!(origin_allowed(&headers));
  }

  #[test]
  fn revoke_removes_both_nonce_authorities() {
    let active = Arc::new(RwLock::new(Some(ProxySession {
      source_nonce: "source-only".to_string(),
      hls_nonce: "browser-only".to_string(),
      upstream_url: "https://media.invalid/source".to_string(),
      output_dir: PathBuf::from("/tmp/embedded-revoke-test"),
    })));
    let state = ProxyState {
      client: reqwest::Client::new(),
      active: Arc::clone(&active),
      diagnostic: Arc::new(RwLock::new(SourceProxyDiagnostic::default())),
    };
    let server = LoopbackMediaServer {
      base_url: "http://127.0.0.1:1".to_string(),
      active,
      diagnostic: Arc::new(RwLock::new(SourceProxyDiagnostic::default())),
    };

    server.revoke();

    assert!(authorized_source_session(&state, "source-only").is_none());
    assert!(authorized_hls_session(&state, "browser-only").is_none());
  }

  #[test]
  fn source_diagnostic_reports_status_without_upstream_identity() {
    let mut diagnostic = SourceProxyDiagnostic::default();

    diagnostic.started(&Method::GET, true);
    diagnostic.completed(StatusCode::PARTIAL_CONTENT);

    assert!(diagnostic
      .summary
      .starts_with("source proxy returned HTTP 206 after "));
  }

  #[test]
  fn source_request_uses_the_working_emby_media_identity_without_auth_headers() {
    let session = ProxySession {
      source_nonce: "source-only".to_string(),
      hls_nonce: "browser-only".to_string(),
      upstream_url: "https://media.invalid/source?api_key=secret".to_string(),
      output_dir: PathBuf::from("/tmp/embedded-request-test"),
    };

    let request = source_upstream_request(&reqwest::Client::new(), reqwest::Method::GET, &session)
      .build()
      .expect("source request");

    assert_eq!(request.headers()[reqwest::header::USER_AGENT], "libmpv");
    assert!(!request.headers().contains_key("X-Emby-Authorization"));
  }
}
