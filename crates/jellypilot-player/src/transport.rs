use std::{
  convert::Infallible,
  io,
  net::TcpListener,
  sync::{Arc, Mutex},
  thread,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use bytes::Bytes;
use futures_util::stream;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::{
  body::{Frame, Incoming},
  header,
  service::service_fn,
  Method, Request, Response, StatusCode,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::{net::TcpListener as AsyncListener, task::JoinSet};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
  source::{network_error, validate_url, NetworkSource},
  PlaybackError,
};

const MANIFEST_LIMIT: usize = 1024 * 1024;
const PREFIX_LIMIT: usize = 512;
const CONNECTION_LIMIT: usize = 32;
type Body = UnsyncBoxBody<Bytes, io::Error>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RouteKind {
  Auto,
  Key,
  Segment,
}

/// A dedicated runtime owns every connection and request; shutdown aborts and joins all of them.
pub(crate) struct NetworkSession {
  uri: String,
  cancel: CancellationToken,
  error: Arc<Mutex<Option<PlaybackError>>>,
  join: Option<thread::JoinHandle<Result<(), PlaybackError>>>,
}

impl NetworkSession {
  pub(crate) fn start(source: NetworkSource) -> Result<Self, PlaybackError> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
      .map_err(|_| network_error("relay", "Could not bind session listener"))?;
    listener
      .set_nonblocking(true)
      .map_err(|_| network_error("relay", "Could not configure listener"))?;
    let address = listener
      .local_addr()
      .map_err(|_| network_error("relay", "Could not inspect listener"))?;
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)
      .map_err(|_| network_error("relay", "Secure session randomness unavailable"))?;
    let base = format!("http://{address}/{}/", URL_SAFE_NO_PAD.encode(nonce));
    let uri = format!("{base}root");
    let cancel = CancellationToken::new();
    let error = Arc::new(Mutex::new(None));
    let state = Arc::new(Relay {
      source,
      base,
      cancel: cancel.clone(),
      error: error.clone(),
    });
    // Runtime construction is local only; no upstream request occurs until the decoder asks.
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .map_err(|_| network_error("relay", "Could not create session runtime"))?;
    let join = thread::Builder::new()
      .name("player-network".into())
      .spawn(move || {
        let result = runtime.block_on(serve(listener, state.clone()));
        if let Err(error) = &result {
          state.latch(error.clone());
        }
        result
      })
      .map_err(|_| network_error("relay", "Could not start session thread"))?;
    Ok(Self {
      uri,
      cancel,
      error,
      join: Some(join),
    })
  }

  pub(crate) fn uri(&self) -> &str {
    &self.uri
  }

  pub(crate) fn error(&self) -> Option<PlaybackError> {
    match self.error.lock() {
      Ok(error) => error.clone(),
      Err(_) => Some(network_error("relay", "Session error state unavailable")),
    }
  }

  pub(crate) fn cancel(&self) {
    self.cancel.cancel();
  }

  pub(crate) fn shutdown(mut self) -> Result<(), PlaybackError> {
    self.cancel();
    if let Some(join) = self.join.take() {
      join
        .join()
        .map_err(|_| network_error("relay", "Session thread failed"))??;
    }
    Ok(())
  }
}

impl Drop for NetworkSession {
  fn drop(&mut self) {
    self.cancel();
  }
}

struct Relay {
  source: NetworkSource,
  base: String,
  cancel: CancellationToken,
  error: Arc<Mutex<Option<PlaybackError>>>,
}

impl Relay {
  fn latch(&self, error: PlaybackError) {
    if let Ok(mut current) = self.error.lock() {
      // Keep the first actionable error, not collateral cancellation failures.
      if current.is_none() {
        *current = Some(error);
      }
    }
  }

  fn check_origin(&self, url: &Url) -> Result<(), PlaybackError> {
    validate_url(url)?;
    if url.origin() != self.source.url.origin() {
      return Err(network_error(
        "origin",
        "Cross-origin request or redirect denied",
      ));
    }
    Ok(())
  }

  fn route(&self, base: &Url, reference: &str, kind: RouteKind) -> Result<String, PlaybackError> {
    if reference.is_empty() || reference.contains("{$") || reference.len() > 8192 {
      return Err(network_error(
        "playlist",
        "Unsupported or oversized playlist URI",
      ));
    }
    let url = base
      .join(reference)
      .map_err(|_| network_error("playlist", "Malformed playlist URI"))?;
    self.check_origin(&url)?;
    let prefix = match kind {
      RouteKind::Auto => "u/",
      RouteKind::Key => "k/",
      RouteKind::Segment => "s/",
    };
    Ok(format!(
      "{}{prefix}{encoded}",
      self.base,
      encoded = URL_SAFE_NO_PAD.encode(url.as_str())
    ))
  }

  fn resolve(&self, request: &Request<Incoming>) -> Option<(Url, RouteKind)> {
    if request.uri().query().is_some() {
      return None;
    }
    let prefix = self.base.strip_prefix("http://")?.split_once('/')?.1;
    let path = request
      .uri()
      .path()
      .strip_prefix('/')?
      .strip_prefix(prefix)?;
    if path == "root" {
      return Some((self.source.url.as_ref().clone(), RouteKind::Auto));
    }
    let (kind, encoded) = path.split_once('/')?;
    let kind = match kind {
      "u" => RouteKind::Auto,
      "k" => RouteKind::Key,
      "s" => RouteKind::Segment,
      _ => return None,
    };
    if encoded.len() > 12000 {
      return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let url = Url::parse(text).ok()?;
    self.check_origin(&url).ok()?;
    Some((url, kind))
  }
}

async fn serve(listener: TcpListener, relay: Arc<Relay>) -> Result<(), PlaybackError> {
  let listener = AsyncListener::from_std(listener)
    .map_err(|_| network_error("relay", "Could not start listener"))?;
  let client = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .retry(reqwest::retry::never())
    .referer(false)
    .no_proxy()
    .connect_timeout(relay.source.timeouts().connect)
    .read_timeout(relay.source.timeouts().read)
    .build()
    .map_err(|_| network_error("relay", "Could not initialize HTTPS client"))?;
  let mut connections = JoinSet::new();
  let result = loop {
    tokio::select! {
      biased;
      _ = relay.cancel.cancelled() => break Ok(()),
      Some(_) = connections.join_next(), if !connections.is_empty() => {},
      accepted = listener.accept(), if connections.len() < CONNECTION_LIMIT => {
        let (socket, _) = match accepted {
          Ok(value) => value,
          Err(_) => break Err(network_error("relay", "Session listener failed")),
        };
        let state = relay.clone();
        let client = client.clone();
        connections.spawn(async move {
          let service_state = state.clone();
          let service = service_fn(move |request| handle(request, service_state.clone(), client.clone()));
          let mut builder = hyper::server::conn::http1::Builder::new();
          builder.max_buf_size(32 * 1024).keep_alive(false)
            .timer(TokioTimer::new()).header_read_timeout(state.source.timeouts().read);
          tokio::select! {
            _ = state.cancel.cancelled() => {},
            _ = builder.serve_connection(TokioIo::new(socket), service) => {},
          }
        });
      }
    }
  };
  drop(listener);
  connections.abort_all();
  while connections.join_next().await.is_some() {}
  result
}

fn full(bytes: impl Into<Bytes>) -> Body {
  Full::new(bytes.into())
    .map_err(|never| match never {})
    .boxed_unsync()
}

fn rejection(status: StatusCode) -> Response<Body> {
  let mut response = Response::new(full(Bytes::new()));
  *response.status_mut() = status;
  response
}

async fn handle(
  request: Request<Incoming>,
  relay: Arc<Relay>,
  client: reqwest::Client,
) -> Result<Response<Body>, Infallible> {
  if !matches!(*request.method(), Method::GET | Method::HEAD) {
    return Ok(rejection(StatusCode::METHOD_NOT_ALLOWED));
  }
  let Some((url, kind)) = relay.resolve(&request) else {
    return Ok(rejection(StatusCode::NOT_FOUND));
  };
  let result = proxy(&request, &relay, &client, url, kind).await;
  Ok(match result {
    Ok(response) => response,
    Err(error) => {
      relay.latch(error);
      rejection(StatusCode::BAD_GATEWAY)
    }
  })
}

fn request_error(error: reqwest::Error) -> PlaybackError {
  network_error(
    "request",
    if error.is_timeout() {
      "Upstream read/connect timeout"
    } else if error.is_connect() {
      "Upstream connection or TLS handshake failed"
    } else {
      "Upstream HTTP transfer failed"
    },
  )
}

async fn fetch(
  client: &reqwest::Client,
  relay: &Relay,
  method: Method,
  mut url: Url,
  range: Option<&header::HeaderValue>,
) -> Result<reqwest::Response, PlaybackError> {
  for redirects in 0..=5 {
    relay.check_origin(&url)?;
    let mut request = client
      .request(method.clone(), url.clone())
      .headers(relay.source.headers.clone())
      .header(header::ACCEPT_ENCODING, "identity");
    if let Some(range) = range {
      request = request.header(header::RANGE, range);
    }
    let response = request.send().await.map_err(request_error)?;
    if response.status().is_redirection() {
      if redirects == 5 {
        return Err(network_error(
          "redirect",
          "Upstream redirect limit exceeded",
        ));
      }
      let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| network_error("redirect", "Upstream redirect has no valid location"))?;
      url = url
        .join(location)
        .map_err(|_| network_error("redirect", "Malformed upstream redirect"))?;
      relay.check_origin(&url)?;
      continue;
    }
    if !response.status().is_success() {
      return Err(network_error(
        "request",
        format!("Upstream HTTP {}", response.status().as_u16()),
      ));
    }
    if response
      .headers()
      .get(header::CONTENT_ENCODING)
      .is_some_and(|v| v != "identity")
    {
      return Err(network_error(
        "request",
        "Unsupported upstream content encoding",
      ));
    }
    return Ok(response);
  }
  Err(network_error(
    "redirect",
    "Upstream redirect limit exceeded",
  ))
}

async fn chunk(response: &mut reqwest::Response) -> Result<Option<Bytes>, PlaybackError> {
  response.chunk().await.map_err(request_error)
}

async fn proxy(
  request: &Request<Incoming>,
  relay: &Arc<Relay>,
  client: &reqwest::Client,
  url: Url,
  kind: RouteKind,
) -> Result<Response<Body>, PlaybackError> {
  let mut upstream = fetch(
    client,
    relay,
    request.method().clone(),
    url,
    request.headers().get(header::RANGE),
  )
  .await?;
  let status = upstream.status();
  let headers = upstream.headers().clone();
  let final_url = upstream.url().clone();
  let mut response = Response::new(full(Bytes::new()));
  *response.status_mut() = status;
  // Never forward cookies, Location, authentication challenges or arbitrary upstream metadata.
  for name in [
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CONTENT_RANGE,
    header::ACCEPT_RANGES,
  ] {
    if let Some(value) = headers.get(&name) {
      response.headers_mut().insert(name, value.clone());
    }
  }
  if request.method() == Method::HEAD {
    return Ok(response);
  }
  let (prefix, pending) = read_prefix(&mut upstream).await?;
  if kind == RouteKind::Key {
    if prefix.len() != 16 || !pending.is_empty() || chunk(&mut upstream).await?.is_some() {
      return Err(network_error(
        "playlist",
        "Only 16-byte AES-128 identity keys are supported",
      ));
    }
    *response.body_mut() = full(prefix);
    return Ok(response);
  }
  let text = String::from_utf8_lossy(&prefix);
  let trimmed = text.trim_start_matches('\u{feff}').trim_start();
  let mime = headers
    .get(header::CONTENT_TYPE)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")
    .split(';')
    .next()
    .unwrap_or("")
    .trim();
  let manifest = trimmed.starts_with("#EXTM3U")
    || matches!(
      mime,
      "application/vnd.apple.mpegurl"
        | "application/x-mpegURL"
        | "audio/mpegurl"
        | "audio/x-mpegurl"
    );
  if kind == RouteKind::Auto && manifest {
    if status == StatusCode::PARTIAL_CONTENT {
      return Err(network_error(
        "playlist",
        "Partial playlists are unsupported",
      ));
    }
    let bytes = collect_bounded(prefix, pending, &mut upstream, MANIFEST_LIMIT).await?;
    let playlist = std::str::from_utf8(&bytes)
      .map_err(|_| network_error("playlist", "Playlist is not UTF-8"))?;
    let rewritten = rewrite_playlist(playlist, &final_url, relay)?;
    response.headers_mut().remove(header::CONTENT_RANGE);
    response.headers_mut().remove(header::ACCEPT_RANGES);
    response.headers_mut().insert(
      header::CONTENT_TYPE,
      header::HeaderValue::from_static("application/vnd.apple.mpegurl"),
    );
    response.headers_mut().insert(
      header::CONTENT_LENGTH,
      header::HeaderValue::from(rewritten.len()),
    );
    *response.body_mut() = full(rewritten);
    return Ok(response);
  }
  // Typefinding never receives unknown text manifests (DASH, SmoothStreaming, ASX, PLS,
  // URI lists, HTML). Whitespace/BOM obfuscation is rejected too, even with a video MIME.
  // Segment routes are generated only in HLS media positions. They may carry
  // AES ciphertext; GStreamer's HLS decoder, not this transport, owns decryption.
  if kind == RouteKind::Auto && !binary_media(&prefix) {
    // An arbitrary byte-range offset need not have a container signature. Probe only
    // the bounded beginning of that same final resource before forwarding its bytes.
    let supported_range = if status == StatusCode::PARTIAL_CONTENT {
      let range = header::HeaderValue::from_static("bytes=0-511");
      let mut probe = fetch(client, relay, Method::GET, final_url, Some(&range)).await?;
      let (start, _) = read_prefix(&mut probe).await?;
      binary_media(&start)
    } else {
      false
    };
    if !supported_range {
      return Err(network_error(
        "format",
        "Unsupported network media or manifest format (only binary media and HLS supported)",
      ));
    }
  }
  let state = (
    Some(Bytes::from(prefix)),
    Some(pending),
    upstream,
    relay.clone(),
    false,
  );
  let body = stream::unfold(
    state,
    |(mut first, mut pending, mut upstream, relay, done)| async move {
      if done {
        return None;
      }
      let value = if let Some(bytes) = first.take() {
        Ok(Some(bytes))
      } else if let Some(bytes) = pending.take().filter(|b| !b.is_empty()) {
        Ok(Some(bytes))
      } else {
        chunk(&mut upstream).await
      };
      match value {
        Ok(Some(bytes)) => Some((
          Ok(Frame::data(bytes)),
          (first, pending, upstream, relay, false),
        )),
        Ok(None) => None,
        Err(error) => {
          relay.latch(error);
          Some((
            Err(io::Error::other("Upstream transfer failed")),
            (first, pending, upstream, relay, true),
          ))
        }
      }
    },
  );
  *response.body_mut() = StreamBody::new(body).boxed_unsync();
  Ok(response)
}

async fn read_prefix(upstream: &mut reqwest::Response) -> Result<(Vec<u8>, Bytes), PlaybackError> {
  let mut prefix = Vec::with_capacity(PREFIX_LIMIT);
  let mut pending = Bytes::new();
  while prefix.len() < PREFIX_LIMIT {
    let Some(mut next) = chunk(upstream).await? else {
      break;
    };
    let take = (PREFIX_LIMIT - prefix.len()).min(next.len());
    prefix.extend_from_slice(&next.split_to(take));
    if !next.is_empty() {
      pending = next;
      break;
    }
  }
  Ok((prefix, pending))
}

fn binary_media(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3])
    || bytes.starts_with(b"OggS")
    || bytes.starts_with(b"fLaC")
    || bytes.starts_with(b"ID3")
    || bytes.first() == Some(&0x47)
    || bytes
      .get(4..8)
      .is_some_and(|magic| matches!(magic, b"ftyp" | b"styp" | b"moof" | b"sidx" | b"mdat"))
    || (bytes.first() == Some(&0xff) && bytes.get(1).is_some_and(|b| b & 0xe0 == 0xe0))
}

async fn collect_bounded(
  mut bytes: Vec<u8>,
  pending: Bytes,
  upstream: &mut reqwest::Response,
  limit: usize,
) -> Result<Vec<u8>, PlaybackError> {
  if bytes.len() + pending.len() > limit {
    return Err(network_error("playlist", "Playlist exceeds 1 MiB limit"));
  }
  bytes.extend_from_slice(&pending);
  while let Some(next) = chunk(upstream).await? {
    if next.len() > limit.saturating_sub(bytes.len()) {
      return Err(network_error("playlist", "Playlist exceeds 1 MiB limit"));
    }
    bytes.extend_from_slice(&next);
  }
  Ok(bytes)
}

fn rewrite_playlist(text: &str, base: &Url, relay: &Relay) -> Result<String, PlaybackError> {
  let text = text.strip_prefix('\u{feff}').unwrap_or(text);
  let mut lines = text.lines();
  if lines.next() != Some("#EXTM3U")
    || text.contains("{$")
    || text
      .chars()
      .any(|c| c.is_control() && c != '\r' && c != '\n')
  {
    return Err(network_error(
      "playlist",
      "Malformed or unsupported HLS playlist",
    ));
  }
  let mut output = String::from("#EXTM3U\n");
  let mut next_kind = RouteKind::Auto;
  for line in lines {
    if line.is_empty() {
      continue;
    }
    if line.contains('\r') {
      return Err(network_error("playlist", "Malformed HLS line ending"));
    }
    if !line.starts_with('#') {
      output.push_str(&relay.route(base, line, next_kind)?);
      next_kind = RouteKind::Auto;
    } else if line.starts_with("#EXT") {
      let (tag, value) = line.split_once(':').unwrap_or((line, ""));
      if tag == "#EXTINF" {
        next_kind = RouteKind::Segment;
      }
      if tag == "#EXT-X-STREAM-INF" {
        next_kind = RouteKind::Auto;
      }
      match tag {
        "#EXTINF"
        | "#EXT-X-VERSION"
        | "#EXT-X-TARGETDURATION"
        | "#EXT-X-MEDIA-SEQUENCE"
        | "#EXT-X-DISCONTINUITY-SEQUENCE"
        | "#EXT-X-ENDLIST"
        | "#EXT-X-PLAYLIST-TYPE"
        | "#EXT-X-I-FRAMES-ONLY"
        | "#EXT-X-INDEPENDENT-SEGMENTS"
        | "#EXT-X-DISCONTINUITY"
        | "#EXT-X-GAP"
        | "#EXT-X-BYTERANGE"
        | "#EXT-X-PROGRAM-DATE-TIME" => output.push_str(line),
        "#EXT-X-KEY"
        | "#EXT-X-SESSION-KEY"
        | "#EXT-X-MAP"
        | "#EXT-X-MEDIA"
        | "#EXT-X-STREAM-INF"
        | "#EXT-X-I-FRAME-STREAM-INF"
        | "#EXT-X-SESSION-DATA"
        | "#EXT-X-START"
        | "#EXT-X-PART"
        | "#EXT-X-PART-INF"
        | "#EXT-X-PRELOAD-HINT"
        | "#EXT-X-RENDITION-REPORT"
        | "#EXT-X-DATERANGE" => {
          output.push_str(tag);
          output.push(':');
          output.push_str(&rewrite_attributes(tag, value, base, relay)?);
        }
        _ => {
          return Err(network_error(
            "playlist",
            "Unsupported HLS tag (external loading is denied)",
          ))
        }
      }
    } else {
      // Comments have no playback semantics and may contain private upstream addresses.
      continue;
    }
    output.push('\n');
    if output.len() > MANIFEST_LIMIT * 4 {
      return Err(network_error(
        "playlist",
        "Rewritten playlist exceeds session limit",
      ));
    }
  }
  Ok(output)
}

fn rewrite_attributes(
  tag: &str,
  mut text: &str,
  base: &Url,
  relay: &Relay,
) -> Result<String, PlaybackError> {
  let mut output = String::new();
  let mut seen = std::collections::HashSet::new();
  let key = matches!(tag, "#EXT-X-KEY" | "#EXT-X-SESSION-KEY");
  while !text.is_empty() {
    let (name, rest) = text
      .split_once('=')
      .ok_or_else(|| network_error("playlist", "Malformed HLS attribute list"))?;
    if !seen.insert(name) || !allowed_attribute(tag, name) {
      return Err(network_error(
        "playlist",
        "Unsupported or duplicate HLS attribute",
      ));
    }
    let quoted = rest.starts_with('"');
    let (value, remainder) = if let Some(rest) = rest.strip_prefix('"') {
      let end = rest
        .find('"')
        .ok_or_else(|| network_error("playlist", "Unterminated HLS attribute"))?;
      (&rest[..end], &rest[end + 1..])
    } else {
      rest
        .find(',')
        .map_or((rest, ""), |end| (&rest[..end], &rest[end..]))
    };
    if value.is_empty() || value.contains(['\r', '\n', '"']) {
      return Err(network_error("playlist", "Malformed HLS attribute"));
    }
    if key
      && ((name == "METHOD" && !matches!(value, "NONE" | "AES-128"))
        || (name == "KEYFORMAT" && value != "identity"))
    {
      return Err(network_error(
        "playlist",
        "Only AES-128 identity HLS encryption is supported",
      ));
    }
    if !output.is_empty() {
      output.push(',');
    }
    output.push_str(name);
    output.push('=');
    if name == "URI" {
      if !quoted {
        return Err(network_error("playlist", "HLS URI must be quoted"));
      }
      let kind = if key {
        RouteKind::Key
      } else if matches!(tag, "#EXT-X-MAP" | "#EXT-X-PART" | "#EXT-X-PRELOAD-HINT") {
        RouteKind::Segment
      } else {
        RouteKind::Auto
      };
      output.push('"');
      output.push_str(&relay.route(base, value, kind)?);
      output.push('"');
    } else {
      if quoted {
        output.push('"');
      }
      output.push_str(value);
      if quoted {
        output.push('"');
      }
    }
    text = if remainder.is_empty() {
      ""
    } else {
      let next = remainder
        .strip_prefix(',')
        .ok_or_else(|| network_error("playlist", "Malformed HLS attribute separator"))?;
      if next.is_empty() {
        return Err(network_error("playlist", "Malformed HLS trailing comma"));
      }
      next
    };
  }
  Ok(output)
}

fn allowed_attribute(tag: &str, name: &str) -> bool {
  let allowed: &[&str] = match tag {
    "#EXT-X-KEY" | "#EXT-X-SESSION-KEY" => {
      &["METHOD", "URI", "IV", "KEYFORMAT", "KEYFORMATVERSIONS"]
    }
    "#EXT-X-MAP" => &["URI", "BYTERANGE"],
    "#EXT-X-MEDIA" => &[
      "TYPE",
      "URI",
      "GROUP-ID",
      "LANGUAGE",
      "ASSOC-LANGUAGE",
      "NAME",
      "DEFAULT",
      "AUTOSELECT",
      "FORCED",
      "INSTREAM-ID",
      "CHARACTERISTICS",
      "CHANNELS",
      "STABLE-RENDITION-ID",
    ],
    "#EXT-X-STREAM-INF" | "#EXT-X-I-FRAME-STREAM-INF" => &[
      "URI",
      "BANDWIDTH",
      "AVERAGE-BANDWIDTH",
      "CODECS",
      "RESOLUTION",
      "FRAME-RATE",
      "HDCP-LEVEL",
      "AUDIO",
      "VIDEO",
      "SUBTITLES",
      "CLOSED-CAPTIONS",
      "VIDEO-RANGE",
      "STABLE-VARIANT-ID",
      "SCORE",
      "ALLOWED-CPC",
    ],
    "#EXT-X-SESSION-DATA" => &["DATA-ID", "VALUE", "URI", "LANGUAGE"],
    "#EXT-X-START" => &["TIME-OFFSET", "PRECISE"],
    "#EXT-X-PART" => &["URI", "DURATION", "INDEPENDENT", "BYTERANGE", "GAP"],
    "#EXT-X-PART-INF" => &["PART-TARGET"],
    "#EXT-X-PRELOAD-HINT" => &["TYPE", "URI", "BYTERANGE-START", "BYTERANGE-LENGTH"],
    "#EXT-X-RENDITION-REPORT" => &["URI", "LAST-MSN", "LAST-PART"],
    "#EXT-X-DATERANGE" => &[
      "ID",
      "CLASS",
      "START-DATE",
      "END-DATE",
      "DURATION",
      "PLANNED-DURATION",
      "SCTE35-CMD",
      "SCTE35-OUT",
      "SCTE35-IN",
      "END-ON-NEXT",
    ],
    _ => &[],
  };
  allowed.contains(&name)
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
