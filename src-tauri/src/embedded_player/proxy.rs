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
use futures_util::StreamExt;
use parking_lot::RwLock;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;

use super::EmbeddedPlayerError;

#[derive(Clone)]
struct ProxySession {
  source_nonce: String,
  direct_media_nonce: Option<String>,
  hls_nonce: Option<String>,
  upstream_url: String,
  output_dir: Option<PathBuf>,
  cancellation: CancellationToken,
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
  terminal_failure: bool,
}

#[derive(Clone)]
pub(super) struct SourceProxySnapshot {
  pub(super) summary: String,
  pub(super) terminal_failure: bool,
}

impl SourceProxyDiagnostic {
  fn reset(&mut self) {
    self.started_at = None;
    self.has_range = false;
    self.summary = "source proxy received no request".to_string();
    self.terminal_failure = false;
  }

  fn started(&mut self, method: &Method, has_range: bool) {
    self.started_at = Some(Instant::now());
    self.has_range = has_range;
    self.terminal_failure = false;
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
    self.terminal_failure = status.is_client_error() || status.is_server_error();
  }

  fn failed(&mut self) {
    self.summary = format!(
      "source proxy could not reach the provider after {} ms",
      self.elapsed_millis()
    );
    self.terminal_failure = true;
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
      .route(
        "/media/{nonce}",
        get(proxy_direct_media).head(proxy_direct_media),
      )
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
    direct_media_nonce: Option<String>,
    hls_nonce: Option<String>,
    upstream_url: String,
    output_dir: Option<PathBuf>,
  ) {
    self.diagnostic.write().reset();
    let replacement = ProxySession {
      source_nonce,
      direct_media_nonce,
      hls_nonce,
      upstream_url,
      output_dir,
      cancellation: CancellationToken::new(),
    };
    if let Some(previous) = self.active.write().replace(replacement) {
      previous.cancellation.cancel();
    }
  }

  pub(super) fn revoke(&self) {
    if let Some(session) = self.active.write().take() {
      session.cancellation.cancel();
    }
  }

  pub(super) fn source_url(&self, nonce: &str) -> String {
    format!("{}/source/{nonce}", self.base_url)
  }

  pub(super) fn playlist_url(&self, nonce: &str) -> String {
    format!("{}/hls/{nonce}/master.m3u8", self.base_url)
  }

  pub(super) fn direct_media_url(&self, nonce: &str) -> String {
    format!("{}/media/{nonce}", self.base_url)
  }

  pub(super) fn source_diagnostic(&self) -> SourceProxySnapshot {
    let diagnostic = self.diagnostic.read();
    SourceProxySnapshot {
      summary: diagnostic.summary.clone(),
      terminal_failure: diagnostic.terminal_failure,
    }
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
  forward_upstream(&state, session, request, None, true).await
}

async fn proxy_direct_media(
  State(state): State<ProxyState>,
  Path(nonce): Path<String>,
  request: Request<Body>,
) -> Response {
  if !origin_allowed(request.headers()) {
    return StatusCode::FORBIDDEN.into_response();
  }
  let Some(session) = authorized_direct_media_session(&state, &nonce) else {
    return StatusCode::NOT_FOUND.into_response();
  };
  let origin = request.headers().get(ORIGIN).cloned();
  forward_upstream(&state, session, request, origin, false).await
}

async fn forward_upstream(
  state: &ProxyState,
  session: ProxySession,
  request: Request<Body>,
  browser_origin: Option<HeaderValue>,
  record_diagnostic: bool,
) -> Response {
  let method = match *request.method() {
    Method::GET => reqwest::Method::GET,
    Method::HEAD => reqwest::Method::HEAD,
    _ => return StatusCode::METHOD_NOT_ALLOWED.into_response(),
  };
  let mut upstream = source_upstream_request(&state.client, method, &session);
  let cancellation = session.cancellation.clone();
  let range = request.headers().get(RANGE);
  if record_diagnostic {
    state
      .diagnostic
      .write()
      .started(request.method(), range.is_some());
  }
  if let Some(range) = range {
    upstream = upstream.header(RANGE, range);
  }

  let upstream = match upstream.send().await {
    Ok(response) => response,
    Err(_) => {
      // reqwest errors can include the authenticated upstream URL. Never log it.
      if record_diagnostic {
        state.diagnostic.write().failed();
      }
      log::warn!("Embedded source proxy request failed");
      return StatusCode::BAD_GATEWAY.into_response();
    }
  };
  let status = upstream.status();
  if record_diagnostic {
    state.diagnostic.write().completed(status);
  }
  let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
  let content_length = upstream.headers().get(CONTENT_LENGTH).cloned();
  let content_range = upstream.headers().get(CONTENT_RANGE).cloned();
  let accept_ranges = upstream.headers().get(ACCEPT_RANGES).cloned();
  let mut response = Response::new(if request.method() == Method::HEAD {
    Body::empty()
  } else {
    Body::from_stream(
      upstream
        .bytes_stream()
        .take_until(cancellation.cancelled_owned()),
    )
  });
  *response.status_mut() = status;
  insert_header(response.headers_mut(), CONTENT_TYPE, content_type);
  insert_header(response.headers_mut(), CONTENT_LENGTH, content_length);
  insert_header(response.headers_mut(), CONTENT_RANGE, content_range);
  insert_header(response.headers_mut(), ACCEPT_RANGES, accept_ranges);
  if let Some(origin) = browser_origin {
    response
      .headers_mut()
      .insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    response
      .headers_mut()
      .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
  }
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
  let Some(output_dir) = session.output_dir else {
    return StatusCode::NOT_FOUND.into_response();
  };
  let path = output_dir.join(&file);
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
    .filter(|session| session.hls_nonce.as_deref() == Some(nonce))
    .cloned()
}

fn authorized_direct_media_session(state: &ProxyState, nonce: &str) -> Option<ProxySession> {
  let active = state.active.read();
  active
    .as_ref()
    .filter(|session| session.direct_media_nonce.as_deref() == Some(nonce))
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
  use std::{convert::Infallible, time::Duration};

  use axum::body::to_bytes;
  use bytes::Bytes;
  use futures_util::stream;

  use super::*;

  async fn upstream_media(method: Method, headers: HeaderMap) -> Response {
    let partial = headers.get(RANGE).is_some();
    let mut response = Response::new(if method == Method::HEAD {
      Body::empty()
    } else {
      Body::from("234")
    });
    *response.status_mut() = if partial {
      StatusCode::PARTIAL_CONTENT
    } else {
      StatusCode::OK
    };
    response
      .headers_mut()
      .insert(CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    response
      .headers_mut()
      .insert(CONTENT_LENGTH, HeaderValue::from_static("3"));
    response
      .headers_mut()
      .insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if partial {
      response
        .headers_mut()
        .insert(CONTENT_RANGE, HeaderValue::from_static("bytes 2-4/10"));
    }
    response
  }

  async fn slow_upstream() -> Response {
    let body = stream::once(async { Ok::<_, Infallible>(Bytes::from_static(b"first")) })
      .chain(stream::pending());
    Response::new(Body::from_stream(body))
  }

  async fn spawn_upstream(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
      .await
      .expect("test upstream should bind");
    let address = listener.local_addr().expect("test upstream address");
    tokio::spawn(async move {
      axum::serve(listener, router)
        .await
        .expect("test upstream should serve");
    });
    format!("http://{address}")
  }

  fn direct_state(upstream_url: String, cancellation: CancellationToken) -> ProxyState {
    ProxyState {
      client: reqwest::Client::new(),
      active: Arc::new(RwLock::new(Some(ProxySession {
        source_nonce: "source-only".to_string(),
        direct_media_nonce: Some("direct-only".to_string()),
        hls_nonce: None,
        upstream_url,
        output_dir: None,
        cancellation,
      }))),
      diagnostic: Arc::new(RwLock::new(SourceProxyDiagnostic::default())),
    }
  }

  fn browser_request(method: Method) -> Request<Body> {
    Request::builder()
      .method(method)
      .header(ORIGIN, "tauri://localhost")
      .body(Body::empty())
      .expect("browser request")
  }

  #[tokio::test]
  async fn direct_media_forwards_get_head_range_and_response_headers() {
    let base_url =
      spawn_upstream(Router::new().route("/media", axum::routing::any(upstream_media))).await;
    let state = direct_state(format!("{base_url}/media"), CancellationToken::new());
    let mut range_request = browser_request(Method::GET);
    range_request
      .headers_mut()
      .insert(RANGE, HeaderValue::from_static("bytes=2-4"));

    let response = proxy_direct_media(
      State(state.clone()),
      Path("direct-only".to_string()),
      range_request,
    )
    .await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[CONTENT_TYPE], "video/mp4");
    assert_eq!(response.headers()[CONTENT_RANGE], "bytes 2-4/10");
    assert_eq!(response.headers()[ACCEPT_RANGES], "bytes");
    assert_eq!(
      response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
      "tauri://localhost"
    );
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(
      to_bytes(response.into_body(), 16)
        .await
        .expect("range body"),
      Bytes::from_static(b"234")
    );

    let response = proxy_direct_media(
      State(state),
      Path("direct-only".to_string()),
      browser_request(Method::HEAD),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_LENGTH], "3");
    assert!(to_bytes(response.into_body(), 16)
      .await
      .expect("HEAD body")
      .is_empty());
  }

  #[tokio::test]
  async fn revoking_a_direct_session_terminates_its_in_flight_body() {
    let base_url =
      spawn_upstream(Router::new().route("/slow", axum::routing::get(slow_upstream))).await;
    let cancellation = CancellationToken::new();
    let state = direct_state(format!("{base_url}/slow"), cancellation.clone());
    let response = proxy_direct_media(
      State(state),
      Path("direct-only".to_string()),
      browser_request(Method::GET),
    )
    .await;

    cancellation.cancel();

    tokio::time::timeout(Duration::from_secs(1), to_bytes(response.into_body(), 64))
      .await
      .expect("revocation should terminate the body")
      .expect("cancelled body should close cleanly");
  }

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
        direct_media_nonce: Some("direct-only".to_string()),
        hls_nonce: Some("browser-only".to_string()),
        upstream_url: "https://media.invalid/source".to_string(),
        output_dir: Some(PathBuf::from("/tmp/embedded-authority-test")),
        cancellation: CancellationToken::new(),
      }))),
      diagnostic: Arc::new(RwLock::new(SourceProxyDiagnostic::default())),
    };

    assert!(authorized_source_session(&state, "source-only").is_some());
    assert!(authorized_source_session(&state, "browser-only").is_none());
    assert!(authorized_hls_session(&state, "browser-only").is_some());
    assert!(authorized_hls_session(&state, "source-only").is_none());
    assert!(authorized_direct_media_session(&state, "direct-only").is_some());
    assert!(authorized_direct_media_session(&state, "source-only").is_none());
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
    let cancellation = CancellationToken::new();
    let active = Arc::new(RwLock::new(Some(ProxySession {
      source_nonce: "source-only".to_string(),
      direct_media_nonce: Some("direct-only".to_string()),
      hls_nonce: Some("browser-only".to_string()),
      upstream_url: "https://media.invalid/source".to_string(),
      output_dir: Some(PathBuf::from("/tmp/embedded-revoke-test")),
      cancellation: cancellation.clone(),
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

    assert!(cancellation.is_cancelled());
    assert!(authorized_source_session(&state, "source-only").is_none());
    assert!(authorized_hls_session(&state, "browser-only").is_none());
    assert!(authorized_direct_media_session(&state, "direct-only").is_none());
  }

  #[test]
  fn source_diagnostic_reports_status_without_upstream_identity() {
    let mut diagnostic = SourceProxyDiagnostic::default();

    diagnostic.started(&Method::GET, true);
    diagnostic.completed(StatusCode::PARTIAL_CONTENT);

    assert!(diagnostic
      .summary
      .starts_with("source proxy returned HTTP 206 after "));
    assert!(!diagnostic.terminal_failure);

    diagnostic.completed(StatusCode::UNAUTHORIZED);
    assert!(diagnostic.terminal_failure);
  }

  #[test]
  fn source_request_uses_the_working_emby_media_identity_without_auth_headers() {
    let session = ProxySession {
      source_nonce: "source-only".to_string(),
      direct_media_nonce: Some("direct-only".to_string()),
      hls_nonce: Some("browser-only".to_string()),
      upstream_url: "https://media.invalid/source?api_key=secret".to_string(),
      output_dir: Some(PathBuf::from("/tmp/embedded-request-test")),
      cancellation: CancellationToken::new(),
    };

    let request = source_upstream_request(&reqwest::Client::new(), reqwest::Method::GET, &session)
      .build()
      .expect("source request");

    assert_eq!(request.headers()[reqwest::header::USER_AGENT], "libmpv");
    assert!(!request.headers().contains_key("X-Emby-Authorization"));
  }
}
