use std::{fmt, path::PathBuf, time::Duration};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::PlaybackError;

/// An explicit local file or private, validated network request.
#[derive(Clone, Debug)]
pub enum PlaybackSource {
  Local(PathBuf),
  Network(NetworkSource),
}

/// Per-session deadlines. All values must be in (0, 300 seconds].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkTimeouts {
  pub connect: Duration,
  pub read: Duration,
  pub startup: Duration,
  pub seek: Duration,
}

impl Default for NetworkTimeouts {
  fn default() -> Self {
    Self {
      connect: Duration::from_secs(10),
      read: Duration::from_secs(15),
      startup: Duration::from_secs(30),
      seek: Duration::from_secs(15),
    }
  }
}

/// Request credentials stay in memory and are never included in Debug or errors.
#[derive(Clone)]
pub struct NetworkSource {
  pub(crate) url: Box<Url>,
  pub(crate) headers: HeaderMap,
  timeouts: NetworkTimeouts,
}

impl fmt::Debug for NetworkSource {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("NetworkSource").finish_non_exhaustive()
  }
}

impl NetworkSource {
  pub fn new(url: impl AsRef<str>) -> Result<Self, PlaybackError> {
    let text = url.as_ref();
    let authority_has_userinfo = text.split_once("://").is_some_and(|(_, rest)| {
      rest
        .split(['/', '?', '#'])
        .next()
        .is_some_and(|authority| authority.contains('@'))
    });
    if text.len() > 8192
      || authority_has_userinfo
      || text
        .bytes()
        .any(|b| b.is_ascii_control() || b == b' ' || b == b'\\')
    {
      return Err(network_error("source", "Invalid HTTP(S) URL"));
    }
    let url = Url::parse(text).map_err(|_| network_error("source", "Invalid HTTP(S) URL"))?;
    validate_url(&url)?;
    Ok(Self {
      url: Box::new(url),
      headers: HeaderMap::new(),
      timeouts: NetworkTimeouts::default(),
    })
  }

  pub fn with_header(
    mut self,
    name: impl AsRef<str>,
    value: impl AsRef<str>,
  ) -> Result<Self, PlaybackError> {
    let name = HeaderName::from_bytes(name.as_ref().as_bytes())
      .map_err(|_| network_error("source", "Invalid request header name"))?;
    if matches!(
      name.as_str(),
      "host"
        | "range"
        | "connection"
        | "keep-alive"
        | "proxy-authenticate"
        | "proxy-authorization"
        | "te"
        | "trailer"
        | "transfer-encoding"
        | "upgrade"
        | "content-length"
        | "accept-encoding"
    ) || name.as_str().starts_with("proxy-")
    {
      return Err(network_error(
        "source",
        "Reserved request header is not allowed",
      ));
    }
    if value.as_ref().len() > 8192 || self.headers.len() >= 64 {
      return Err(network_error(
        "source",
        "Request headers exceed session limits",
      ));
    }
    let mut value = HeaderValue::from_str(value.as_ref())
      .map_err(|_| network_error("source", "Invalid request header value"))?;
    value.set_sensitive(true);
    self.headers.insert(name, value);
    Ok(self)
  }

  pub fn with_timeouts(mut self, timeouts: NetworkTimeouts) -> Result<Self, PlaybackError> {
    if [
      timeouts.connect,
      timeouts.read,
      timeouts.startup,
      timeouts.seek,
    ]
    .into_iter()
    .any(|value| value.is_zero() || value > Duration::from_secs(300))
    {
      return Err(network_error(
        "source",
        "Timeouts must be greater than zero and at most 300 seconds",
      ));
    }
    self.timeouts = timeouts;
    Ok(self)
  }

  pub fn timeouts(&self) -> NetworkTimeouts {
    self.timeouts
  }

  /// Origin-only presentation; paths as well as queries may contain credentials.
  pub fn redacted_url(&self) -> String {
    self.url.origin().ascii_serialization()
  }
}

pub(crate) fn validate_url(url: &Url) -> Result<(), PlaybackError> {
  if !matches!(url.scheme(), "http" | "https")
    || url.host_str().is_none()
    || !url.username().is_empty()
    || url.password().is_some()
    || url.fragment().is_some()
  {
    return Err(network_error(
      "source",
      "Only HTTP(S) URLs without userinfo or fragments are supported",
    ));
  }
  Ok(())
}

pub(crate) fn network_error(operation: &'static str, reason: impl Into<String>) -> PlaybackError {
  PlaybackError::Network {
    operation,
    reason: reason.into(),
  }
}
