use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::time::Instant;

use hls_m3u8::tags::{ExtXKey, SessionData, VariantStream};
use hls_m3u8::{MasterPlaylist, MediaPlaylist};
use sha2::{Digest, Sha256};
use url::Url;

use super::HlsProxyError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
  Playlist,
  Segment,
  Map,
  Key,
  Rendition,
}

impl ResourceKind {
  pub fn as_str(&self) -> &'static str {
    match self {
      ResourceKind::Playlist => "playlist",
      ResourceKind::Segment => "segment",
      ResourceKind::Map => "map",
      ResourceKind::Key => "key",
      ResourceKind::Rendition => "rendition",
    }
  }
}

#[derive(Clone, Debug)]
pub struct ResourceInfo {
  pub resource_id: String,
  pub kind: ResourceKind,
  pub upstream_url: Url,
  pub effective_byte_range: Option<(u64, u64)>, // (offset, length)
  pub is_key: bool,
  pub segment_index: Option<usize>,
}

#[derive(Default, Debug)]
pub struct ResourceTable {
  pub resources: HashMap<String, ResourceInfo>,
  pub segment_sequence: Vec<String>, // Resource IDs in ordinal order
}

pub struct PlaylistEntry {
  pub upstream_url: Url,
  pub target_duration_secs: f64,
  pub has_end_list: bool,
  pub last_fetch_time: Instant,
  pub cached_rewritten_body: String,
}

const EMBY_CREDENTIAL_PARAMS: &[&str] = &[
  "api_key",
  "X-Emby-Token",
  "X-MediaBrowser-Token",
  "access_token",
];

pub fn extract_emby_credentials(url: &Url) -> Vec<(String, String)> {
  url
    .query_pairs()
    .filter(|(k, _)| EMBY_CREDENTIAL_PARAMS.contains(&k.as_ref()))
    .map(|(k, v)| (k.into_owned(), v.into_owned()))
    .collect()
}

pub fn resolve_child_url(
  parent_response_url: &Url,
  child_uri_str: &str,
  captured_creds: &[(String, String)],
) -> Result<Url, HlsProxyError> {
  let resolved = parent_response_url
    .join(child_uri_str)
    .map_err(|_| HlsProxyError::Playlist("Failed to resolve child URI".to_string()))?;

  let is_same_origin = resolved.scheme() == parent_response_url.scheme()
    && resolved.host_str() == parent_response_url.host_str()
    && resolved.port_or_known_default() == parent_response_url.port_or_known_default();

  if is_same_origin {
    let mut final_url = resolved;
    let existing_keys: HashSet<String> = final_url
      .query_pairs()
      .map(|(k, _)| k.into_owned())
      .collect();
    let mut pairs: Vec<(String, String)> = final_url
      .query_pairs()
      .map(|(k, v)| (k.into_owned(), v.into_owned()))
      .collect();
    for (k, v) in captured_creds {
      if !existing_keys.contains(k) {
        pairs.push((k.clone(), v.clone()));
      }
    }
    final_url.query_pairs_mut().clear();
    for (k, v) in pairs {
      final_url.query_pairs_mut().append_pair(&k, &v);
    }
    Ok(final_url)
  } else {
    Ok(resolved)
  }
}

pub fn compute_resource_id(
  kind: ResourceKind,
  upstream_url: &Url,
  range: Option<(u64, u64)>,
) -> String {
  let mut hasher = Sha256::new();
  hasher.update(kind.as_str().as_bytes());
  hasher.update(b":");
  hasher.update(upstream_url.as_str().as_bytes());
  hasher.update(b":");
  if let Some((offset, length)) = range {
    hasher.update(format!("{}-{}", offset, length).as_bytes());
  } else {
    hasher.update(b"none");
  }
  format!("{:x}", hasher.finalize())
}

pub fn make_local_url(
  port: u16,
  session_nonce: &str,
  kind: ResourceKind,
  resource_id: &str,
) -> String {
  match kind {
    ResourceKind::Playlist => format!(
      "http://127.0.0.1:{}/hls/{}/playlist/{}.m3u8",
      port, session_nonce, resource_id
    ),
    _ => format!(
      "http://127.0.0.1:{}/hls/{}/resource/{}",
      port, session_nonce, resource_id
    ),
  }
}

pub enum ParsedPlaylist {
  Master {
    rewritten_body: String,
  },
  Media {
    rewritten_body: String,
    target_duration_secs: f64,
    has_end_list: bool,
  },
}

pub fn parse_and_rewrite_playlist(
  raw_body: &str,
  response_url: &Url,
  captured_creds: &[(String, String)],
  port: u16,
  session_nonce: &str,
  resource_table: &mut ResourceTable,
) -> Result<ParsedPlaylist, HlsProxyError> {
  if let Ok(mut master) = MasterPlaylist::try_from(raw_body) {
    // 1. Variant Streams
    let mut new_variants = Vec::new();
    for v in master.variant_streams {
      match v {
        VariantStream::ExtXStreamInf {
          uri,
          frame_rate,
          audio,
          subtitles,
          closed_captions,
          stream_data,
        } => {
          let upstream = resolve_child_url(response_url, &uri, captured_creds)?;
          let res_id = compute_resource_id(ResourceKind::Playlist, &upstream, None);
          resource_table.resources.insert(
            res_id.clone(),
            ResourceInfo {
              resource_id: res_id.clone(),
              kind: ResourceKind::Playlist,
              upstream_url: upstream,
              effective_byte_range: None,
              is_key: false,
              segment_index: None,
            },
          );
          let local_uri = make_local_url(port, session_nonce, ResourceKind::Playlist, &res_id);
          new_variants.push(VariantStream::ExtXStreamInf {
            uri: local_uri.into(),
            frame_rate,
            audio,
            subtitles,
            closed_captions,
            stream_data,
          });
        }
        VariantStream::ExtXIFrame { uri, stream_data } => {
          let upstream = resolve_child_url(response_url, &uri, captured_creds)?;
          let res_id = compute_resource_id(ResourceKind::Playlist, &upstream, None);
          resource_table.resources.insert(
            res_id.clone(),
            ResourceInfo {
              resource_id: res_id.clone(),
              kind: ResourceKind::Playlist,
              upstream_url: upstream,
              effective_byte_range: None,
              is_key: false,
              segment_index: None,
            },
          );
          let local_uri = make_local_url(port, session_nonce, ResourceKind::Playlist, &res_id);
          new_variants.push(VariantStream::ExtXIFrame {
            uri: local_uri.into(),
            stream_data,
          });
        }
      }
    }
    master.variant_streams = new_variants;

    // 2. Media Tags (Alternate Renditions)
    for m in &mut master.media {
      if let Some(uri) = m.uri() {
        let upstream = resolve_child_url(response_url, uri, captured_creds)?;
        let res_id = compute_resource_id(ResourceKind::Rendition, &upstream, None);
        resource_table.resources.insert(
          res_id.clone(),
          ResourceInfo {
            resource_id: res_id.clone(),
            kind: ResourceKind::Rendition,
            upstream_url: upstream,
            effective_byte_range: None,
            is_key: false,
            segment_index: None,
          },
        );
        let local_uri = make_local_url(port, session_nonce, ResourceKind::Rendition, &res_id);
        m.set_uri(Some(local_uri));
      }
    }

    // 3. Session Data
    for sd in &mut master.session_data {
      if let SessionData::Uri(ref uri) = sd.data {
        let upstream = resolve_child_url(response_url, uri, captured_creds)?;
        let res_id = compute_resource_id(ResourceKind::Playlist, &upstream, None);
        resource_table.resources.insert(
          res_id.clone(),
          ResourceInfo {
            resource_id: res_id.clone(),
            kind: ResourceKind::Playlist,
            upstream_url: upstream,
            effective_byte_range: None,
            is_key: false,
            segment_index: None,
          },
        );
        let local_uri = make_local_url(port, session_nonce, ResourceKind::Playlist, &res_id);
        sd.data = SessionData::Uri(local_uri.into());
      }
    }

    // 4. Session Keys
    for sk in &mut master.session_keys {
      let key_uri = sk.0.uri();
      let upstream = resolve_child_url(response_url, key_uri, captured_creds)?;
      let res_id = compute_resource_id(ResourceKind::Key, &upstream, None);
      resource_table.resources.insert(
        res_id.clone(),
        ResourceInfo {
          resource_id: res_id.clone(),
          kind: ResourceKind::Key,
          upstream_url: upstream,
          effective_byte_range: None,
          is_key: true,
          segment_index: None,
        },
      );
      let local_uri = make_local_url(port, session_nonce, ResourceKind::Key, &res_id);
      sk.0.set_uri(local_uri);
    }

    Ok(ParsedPlaylist::Master {
      rewritten_body: master.to_string(),
    })
  } else if let Ok(mut media) = MediaPlaylist::try_from(raw_body) {
    let target_duration_secs = media.target_duration.as_secs_f64();
    let has_end_list = media.has_end_list;

    let mut last_byte_range_end: u64 = 0;

    for (seg_idx, (_, seg)) in media.segments.iter_mut().enumerate() {
      // 1. Byte Range calculation
      let effective_range = if let Some(br) = seg.byte_range {
        let offset = br.start().unwrap_or(last_byte_range_end as usize) as u64;
        let length = br.len() as u64;
        last_byte_range_end = offset + length;
        Some((offset, length))
      } else {
        None
      };

      // 2. Segment URI
      let seg_uri = seg.uri();
      let upstream_seg = resolve_child_url(response_url, seg_uri, captured_creds)?;
      let seg_res_id = compute_resource_id(ResourceKind::Segment, &upstream_seg, effective_range);

      resource_table.resources.insert(
        seg_res_id.clone(),
        ResourceInfo {
          resource_id: seg_res_id.clone(),
          kind: ResourceKind::Segment,
          upstream_url: upstream_seg,
          effective_byte_range: effective_range,
          is_key: false,
          segment_index: Some(seg_idx),
        },
      );
      if !resource_table.segment_sequence.contains(&seg_res_id) {
        resource_table.segment_sequence.push(seg_res_id.clone());
      }

      let local_seg_uri = make_local_url(port, session_nonce, ResourceKind::Segment, &seg_res_id);
      seg.set_uri(local_seg_uri);

      // 3. Map tag (init section)
      if let Some(map) = &mut seg.map {
        let map_uri = map.uri();
        let upstream_map = resolve_child_url(response_url, map_uri, captured_creds)?;
        let map_res_id = compute_resource_id(ResourceKind::Map, &upstream_map, None);
        resource_table.resources.insert(
          map_res_id.clone(),
          ResourceInfo {
            resource_id: map_res_id.clone(),
            kind: ResourceKind::Map,
            upstream_url: upstream_map,
            effective_byte_range: None,
            is_key: false,
            segment_index: None,
          },
        );
        let local_map_uri = make_local_url(port, session_nonce, ResourceKind::Map, &map_res_id);
        map.set_uri(local_map_uri);
      }

      // 4. Key tags
      for key in &mut seg.keys {
        if let ExtXKey(Some(dec_key)) = key {
          let key_uri = dec_key.uri();
          let upstream_key = resolve_child_url(response_url, key_uri, captured_creds)?;
          let key_res_id = compute_resource_id(ResourceKind::Key, &upstream_key, None);
          resource_table.resources.insert(
            key_res_id.clone(),
            ResourceInfo {
              resource_id: key_res_id.clone(),
              kind: ResourceKind::Key,
              upstream_url: upstream_key,
              effective_byte_range: None,
              is_key: true,
              segment_index: None,
            },
          );
          let local_key_uri = make_local_url(port, session_nonce, ResourceKind::Key, &key_res_id);
          dec_key.set_uri(local_key_uri);
        }
      }
    }

    Ok(ParsedPlaylist::Media {
      rewritten_body: media.to_string(),
      target_duration_secs,
      has_end_list,
    })
  } else {
    Err(HlsProxyError::UnsupportedContent)
  }
}
