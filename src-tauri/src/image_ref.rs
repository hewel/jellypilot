//! Signed image references for decoupled media artwork loading.

use std::sync::OnceLock;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::jellyfin::MediaServerProvider;

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
pub struct ImageRefPayload {
  pub version: u8,
  pub provider: MediaServerProvider,
  pub server_url: String,
  pub remote_url: String,
  pub kind: ImageRefKind,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageRefError {
  #[error("invalid image reference")]
  Invalid,
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

pub fn decode_image_id(token: &str) -> Result<ImageRefPayload, ImageRefError> {
  signer().decode(token)
}

pub fn normalize_server_url(server_url: &str) -> &str {
  server_url.trim_end_matches('/')
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
}
