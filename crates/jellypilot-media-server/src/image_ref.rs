//! Signed image references for decoupled media artwork loading.

use std::sync::OnceLock;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::types::MediaServerProvider;

const TOKEN_VERSION: u8 = 1;
const HMAC_BLOCK_SIZE: usize = 64;

static SIGNER: OnceLock<ImageRefSigner> = OnceLock::new();

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImageRefKind {
  Artwork,
  Backdrop,
}

impl ImageRefKind {
  /// Maximum width requested from the origin for this reference kind.
  pub(crate) const fn max_width(self) -> u16 {
    match self {
      Self::Artwork => 600,
      Self::Backdrop => 1920,
    }
  }

  /// JPEG/WebP quality requested from the origin for this reference kind.
  pub(crate) const fn quality(self) -> u8 {
    90
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageRefPayload {
  pub(crate) version: u8,
  pub(crate) provider: MediaServerProvider,
  pub(crate) server_url: String,
  pub(crate) remote_url: String,
  pub(crate) kind: ImageRefKind,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageRefError {
  #[error("invalid image reference")]
  Invalid,
  #[error("invalid image remote URL")]
  InvalidRemoteUrl,
  #[error("image reference signature mismatch")]
  SignatureMismatch,
  #[error("unsupported image reference version")]
  UnsupportedVersion,
  #[error("image reference serialization failed: {0}")]
  Json(#[from] serde_json::Error),
}

struct ImageRefSigner {
  key: [u8; 32],
}

impl ImageRefSigner {
  fn new() -> Self {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut key = [0_u8; 32];
    key[..16].copy_from_slice(first.as_bytes());
    key[16..].copy_from_slice(second.as_bytes());
    Self { key }
  }

  fn encode(&self, payload: &ImageRefPayload) -> Result<String, ImageRefError> {
    let payload_bytes = serde_json::to_vec(payload)?;
    let payload_part = URL_SAFE_NO_PAD.encode(&payload_bytes);
    let signature = hmac_sha256(&self.key, payload_part.as_bytes());
    let signature_part = URL_SAFE_NO_PAD.encode(signature);
    Ok(format!("{payload_part}.{signature_part}"))
  }

  fn decode(&self, token: &str) -> Result<ImageRefPayload, ImageRefError> {
    let (payload_part, signature_part) = token.split_once('.').ok_or(ImageRefError::Invalid)?;
    let expected = hmac_sha256(&self.key, payload_part.as_bytes());
    let actual = URL_SAFE_NO_PAD
      .decode(signature_part)
      .map_err(|_| ImageRefError::Invalid)?;
    if !constant_time_eq(&expected, &actual) {
      return Err(ImageRefError::SignatureMismatch);
    }

    let payload_bytes = URL_SAFE_NO_PAD
      .decode(payload_part)
      .map_err(|_| ImageRefError::Invalid)?;
    let payload: ImageRefPayload = serde_json::from_slice(&payload_bytes)?;
    if payload.version != TOKEN_VERSION {
      return Err(ImageRefError::UnsupportedVersion);
    }
    Ok(payload)
  }
}

pub fn image_id_for_url(
  provider: MediaServerProvider,
  server_url: &str,
  remote_url: String,
  kind: ImageRefKind,
) -> Result<String, ImageRefError> {
  let payload = ImageRefPayload {
    version: TOKEN_VERSION,
    provider,
    server_url: normalize_server_url(server_url).to_string(),
    remote_url,
    kind,
  };
  signer().encode(&payload)
}

pub(crate) fn decode_image_id(token: &str) -> Result<ImageRefPayload, ImageRefError> {
  signer().decode(token)
}

pub fn normalize_server_url(server_url: &str) -> &str {
  server_url.trim_end_matches('/')
}

/// Apply the image kind's provider-specific size profile to an origin URL.
///
/// Retained non-sizing query segments stay byte-for-byte identical to the
/// signed URL. Existing sizing keys are removed case-insensitively after
/// percent-decoding only their key, then the canonical provider casing is
/// appended.
pub fn sized_origin_url(
  remote_url: &str,
  kind: ImageRefKind,
  provider: MediaServerProvider,
) -> Result<String, ImageRefError> {
  sized_origin_url_for_width(
    remote_url,
    provider,
    u32::from(kind.max_width()),
    u32::from(kind.quality()),
  )
}

/// Apply an explicit server-side resize profile to an origin URL.
///
/// Same query handling as [`sized_origin_url`], but the caller picks the
/// width, e.g. to clamp a Backdrop-kind reference down to a smaller decode
/// class's source width.
pub fn sized_origin_url_for_width(
  remote_url: &str,
  provider: MediaServerProvider,
  max_width: u32,
  quality: u32,
) -> Result<String, ImageRefError> {
  let parsed = Url::parse(remote_url).map_err(|_| ImageRefError::InvalidRemoteUrl)?;
  if !is_http_url_without_credentials(&parsed) {
    return Err(ImageRefError::InvalidRemoteUrl);
  }

  let (max_width_key, quality_key) = match provider {
    MediaServerProvider::Jellyfin => ("maxWidth", "quality"),
    MediaServerProvider::Emby => ("MaxWidth", "Quality"),
  };

  let (before_fragment, fragment) = match remote_url.split_once('#') {
    Some((base, frag)) => (base, Some(frag)),
    None => (remote_url, None),
  };
  let (base, raw_query) = match before_fragment.split_once('?') {
    Some((base, query)) => (base, Some(query)),
    None => (before_fragment, None),
  };

  let mut new_query = String::new();
  let mut retained_any_segment = false;
  if let Some(query) = raw_query {
    // Bare `?` has zero pairs; explicit `?&` / `?&&` retain their empty pairs.
    if !query.is_empty() {
      for segment in query.split('&') {
        let raw_key = segment
          .split_once('=')
          .map(|(key, _)| key)
          .unwrap_or(segment);
        if is_sizing_query_key(raw_key) {
          continue;
        }
        if retained_any_segment {
          new_query.push('&');
        }
        new_query.push_str(segment);
        retained_any_segment = true;
      }
    }
  }
  if retained_any_segment {
    new_query.push('&');
  }
  new_query.push_str(max_width_key);
  new_query.push('=');
  new_query.push_str(&max_width.to_string());
  new_query.push('&');
  new_query.push_str(quality_key);
  new_query.push('=');
  new_query.push_str(&quality.to_string());

  let mut sized = String::with_capacity(
    base.len() + 1 + new_query.len() + fragment.map(|f| f.len() + 1).unwrap_or(0),
  );
  sized.push_str(base);
  sized.push('?');
  sized.push_str(&new_query);
  if let Some(fragment) = fragment {
    sized.push('#');
    sized.push_str(fragment);
  }
  Ok(sized)
}

pub(crate) fn validate_remote_url_for_server(
  server_url: &str,
  remote_url: &str,
) -> Result<(), ImageRefError> {
  let server = Url::parse(server_url).map_err(|_| ImageRefError::InvalidRemoteUrl)?;
  let remote = Url::parse(remote_url).map_err(|_| ImageRefError::InvalidRemoteUrl)?;
  if !is_http_url_without_credentials(&server)
    || server.query().is_some()
    || server.fragment().is_some()
    || !is_http_url_without_credentials(&remote)
    || server.scheme() != remote.scheme()
    || server.host() != remote.host()
    || server.port_or_known_default() != remote.port_or_known_default()
    || !path_is_within_server_base(server.path(), remote.path())
  {
    return Err(ImageRefError::InvalidRemoteUrl);
  }
  Ok(())
}

fn is_http_url_without_credentials(url: &Url) -> bool {
  matches!(url.scheme(), "http" | "https")
    && url.host().is_some()
    && url.username().is_empty()
    && url.password().is_none()
}

fn path_is_within_server_base(server_path: &str, remote_path: &str) -> bool {
  let server_path = server_path.trim_end_matches('/');
  server_path.is_empty()
    || remote_path == server_path
    || remote_path
      .strip_prefix(server_path)
      .is_some_and(|suffix| suffix.starts_with('/'))
}

/// Percent-decode a raw query key only far enough to match sizing param names.
fn is_sizing_query_key(raw_key: &str) -> bool {
  let decoded = url::form_urlencoded::parse(format!("{raw_key}=").as_bytes())
    .next()
    .map(|(key, _)| key.into_owned())
    .unwrap_or_else(|| raw_key.to_owned());
  decoded.eq_ignore_ascii_case("maxWidth") || decoded.eq_ignore_ascii_case("quality")
}

fn signer() -> &'static ImageRefSigner {
  SIGNER.get_or_init(ImageRefSigner::new)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
  let mut normalized_key = [0_u8; HMAC_BLOCK_SIZE];
  if key.len() > HMAC_BLOCK_SIZE {
    let hashed = Sha256::digest(key);
    normalized_key[..hashed.len()].copy_from_slice(&hashed);
  } else {
    normalized_key[..key.len()].copy_from_slice(key);
  }

  let mut outer_key_pad = [0x5c_u8; HMAC_BLOCK_SIZE];
  let mut inner_key_pad = [0x36_u8; HMAC_BLOCK_SIZE];
  for index in 0..HMAC_BLOCK_SIZE {
    outer_key_pad[index] ^= normalized_key[index];
    inner_key_pad[index] ^= normalized_key[index];
  }

  let mut inner = Sha256::new();
  inner.update(inner_key_pad);
  inner.update(data);
  let inner_hash = inner.finalize();

  let mut outer = Sha256::new();
  outer.update(outer_key_pad);
  outer.update(inner_hash);
  outer.finalize().into()
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
  if expected.len() != actual.len() {
    return false;
  }

  let mut diff = 0_u8;
  for (left, right) in expected.iter().zip(actual.iter()) {
    diff |= left ^ right;
  }
  diff == 0
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn image_id_round_trips_signed_payload() {
    let token = image_id_for_url(
      MediaServerProvider::Jellyfin,
      "https://media.example.com/",
      "https://media.example.com/Items/1/Images/Primary?tag=a".to_string(),
      ImageRefKind::Artwork,
    )
    .expect("image ref should encode");

    let payload = decode_image_id(&token).expect("image ref should decode");

    assert_eq!(
      payload.remote_url,
      "https://media.example.com/Items/1/Images/Primary?tag=a"
    );
    assert_eq!(payload.server_url, "https://media.example.com");
    assert_eq!(payload.kind, ImageRefKind::Artwork);
  }

  #[test]
  fn image_ref_kind_profiles_match_contract() {
    assert_eq!(ImageRefKind::Artwork.max_width(), 600);
    assert_eq!(ImageRefKind::Backdrop.max_width(), 1920);
    assert_eq!(ImageRefKind::Artwork.quality(), 90);
    assert_eq!(ImageRefKind::Backdrop.quality(), 90);
  }

  #[test]
  fn image_id_rejects_tampered_payload() {
    let token = image_id_for_url(
      MediaServerProvider::Emby,
      "https://media.example.com",
      "https://media.example.com/Items/1/Images/Primary?tag=a".to_string(),
      ImageRefKind::Artwork,
    )
    .expect("image ref should encode");
    let (payload, signature) = token.split_once('.').expect("token should have signature");
    let tampered = format!("{payload}x.{signature}");

    let error = decode_image_id(&tampered).expect_err("tampered ref should fail");

    assert!(matches!(
      error,
      ImageRefError::Invalid | ImageRefError::SignatureMismatch
    ));
  }

  #[test]
  fn remote_url_validation_accepts_default_port_and_reverse_proxy_base_path() {
    let result = validate_remote_url_for_server(
      "https://media.example.com/emby",
      "https://media.example.com:443/emby/Items/1/Images/Primary",
    );

    assert!(result.is_ok(), "same origin and base path should be valid");
  }

  #[test]
  fn remote_url_validation_rejects_different_origin_port() {
    let error = validate_remote_url_for_server(
      "https://media.example.com/emby",
      "https://media.example.com:444/emby/Items/1/Images/Primary",
    )
    .expect_err("different effective port should be rejected");

    assert!(matches!(error, ImageRefError::InvalidRemoteUrl));
  }

  #[test]
  fn remote_url_validation_rejects_different_origin_scheme() {
    let error = validate_remote_url_for_server(
      "https://media.example.com/emby",
      "http://media.example.com/emby/Items/1/Images/Primary",
    )
    .expect_err("different scheme should be rejected");

    assert!(matches!(error, ImageRefError::InvalidRemoteUrl));
  }

  #[test]
  fn remote_url_validation_rejects_reverse_proxy_sibling_path() {
    let error = validate_remote_url_for_server(
      "https://media.example.com/emby",
      "https://media.example.com/emby-admin/Items/1/Images/Primary",
    )
    .expect_err("sibling path should be rejected");

    assert!(matches!(error, ImageRefError::InvalidRemoteUrl));
  }
}
