mod download;
mod playlist;
mod store;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use parking_lot::{Mutex, RwLock};

use playlist::{
  extract_emby_credentials, parse_and_rewrite_playlist, ParsedPlaylist, PlaylistEntry,
  ResourceKind, ResourceTable,
};
use store::{HlsProxyConfig, StoreManager};
use tokio_util::sync::CancellationToken;
use url::Url;

type ProxyResult = Result<Arc<HlsProxy>, String>;

#[derive(Clone, Default)]
pub struct HlsProxyState {
  inner: Arc<RwLock<Option<ProxyResult>>>,
}

impl HlsProxyState {
  pub fn install(&self, result: Result<Arc<HlsProxy>, HlsProxyError>) {
    let mut guard = self.inner.write();
    *guard = Some(result.map_err(|e| e.to_string()));
  }

  pub fn current(&self) -> Result<Arc<HlsProxy>, String> {
    let guard = self.inner.read();
    match &*guard {
      Some(Ok(proxy)) => Ok(proxy.clone()),
      Some(Err(err)) => Err(err.clone()),
      None => Err("HLS proxy listener unavailable".to_string()),
    }
  }
}

pub struct ActivatedHls {
  pub session_id: String,
  pub playlist_url: String,
  pub events: async_channel::Receiver<HlsProxyEvent>,
  pub cache_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlsProxyEvent {
  CacheDisabled,
  PlaybackFailed,
  OriginExpired,
}

#[derive(Debug, thiserror::Error)]
pub enum HlsProxyError {
  #[error("HLS proxy listener unavailable: {0}")]
  Bind(#[source] std::io::Error),
  #[error("HLS origin request failed")]
  Origin(#[source] reqwest::Error),
  #[error("HLS origin returned HTTP {0}")]
  OriginStatus(reqwest::StatusCode),
  #[error("remote response is not an HLS playlist")]
  UnsupportedContent,
  #[error("invalid HLS playlist: {0}")]
  Playlist(String),
  #[error("HLS cache unavailable: {0}")]
  Cache(#[source] std::io::Error),
}

pub struct ProxySession {
  #[allow(dead_code)]
  pub session_id: String,
  pub captured_creds: Vec<(String, String)>,
  pub resource_table: Arc<Mutex<ResourceTable>>,
  pub playlist_entries: Arc<Mutex<HashMap<String, PlaylistEntry>>>,
  pub download_state: Arc<download::SessionDownloadState>,
  pub prefetch_cancel_token: Arc<Mutex<CancellationToken>>,
  pub cancel_token: CancellationToken,
}

impl ProxySession {
  pub fn cancel_prefetch(&self) {
    let mut guard = self.prefetch_cancel_token.lock();
    guard.cancel();
    *guard = CancellationToken::new();
  }

  pub fn trigger_prefetch(
    &self,
    store: Arc<StoreManager>,
    _client: reqwest::Client,
    current_segment_idx: usize,
    _port: u16,
    session_nonce: &str,
  ) {
    let table = self.resource_table.lock();
    let total_segments = table.segment_sequence.len();
    if current_segment_idx >= total_segments {
      return;
    }

    let prefetch_ahead = store.config().prefetch_ahead;
    let end_idx = (current_segment_idx + 1 + prefetch_ahead).min(total_segments);

    let next_ids: Vec<String> = table.segment_sequence[current_segment_idx + 1..end_idx].to_vec();
    drop(table);

    let cancel_token = self.prefetch_cancel_token.lock().clone();
    let download_state = self.download_state.clone();
    let nonce = session_nonce.to_string();

    tokio::spawn(async move {
      for seg_id in next_ids {
        if cancel_token.is_cancelled() {
          break;
        }

        let is_in_flight_or_cached = {
          let in_flight = download_state.in_flight.lock();
          if in_flight.contains_key(&seg_id) {
            true
          } else if let Some(bin_path) = store.bin_path(&nonce, &seg_id) {
            bin_path.exists()
          } else {
            false
          }
        };

        if is_in_flight_or_cached {
          continue;
        }
      }
    });
  }
}

pub struct HlsProxyInner {
  pub port: u16,
  pub store: Arc<StoreManager>,
  pub sessions: RwLock<HashMap<String, Arc<ProxySession>>>,
  pub reqwest_client: reqwest::Client,
}

pub struct HlsProxy {
  inner: Arc<HlsProxyInner>,
  shutdown_token: CancellationToken,
}

impl HlsProxy {
  pub fn start(cache_root: Option<PathBuf>) -> Result<Arc<Self>, HlsProxyError> {
    Self::start_with_config(cache_root, HlsProxyConfig::default())
  }

  pub fn start_with_config(
    cache_root: Option<PathBuf>,
    config: HlsProxyConfig,
  ) -> Result<Arc<Self>, HlsProxyError> {
    let std_listener = TcpListener::bind("127.0.0.1:0").map_err(HlsProxyError::Bind)?;
    let port = std_listener
      .local_addr()
      .map_err(HlsProxyError::Bind)?
      .port();
    std_listener
      .set_nonblocking(true)
      .map_err(HlsProxyError::Bind)?;

    let store = StoreManager::new(cache_root, config).map_err(HlsProxyError::Cache)?;
    let reqwest_client = download::make_reqwest_client();
    let shutdown_token = CancellationToken::new();

    let inner = Arc::new(HlsProxyInner {
      port,
      store,
      sessions: RwLock::new(HashMap::new()),
      reqwest_client,
    });

    let proxy = Arc::new(Self {
      inner: inner.clone(),
      shutdown_token: shutdown_token.clone(),
    });

    let app_router = Router::new()
      .route(
        "/hls/{session_nonce}/playlist/{resource_filename}",
        get(handle_playlist).with_state(inner.clone()),
      )
      .route(
        "/hls/{session_nonce}/resource/{resource_id}",
        get(handle_resource).with_state(inner.clone()),
      );

    let shutdown_clone = shutdown_token.clone();
    tauri::async_runtime::spawn(async move {
      // `from_std` registers the socket with a runtime reactor; the Tauri
      // setup hook has no Tokio context, so convert inside the runtime task.
      let tokio_listener = match tokio::net::TcpListener::from_std(std_listener) {
        Ok(listener) => listener,
        Err(e) => {
          log::error!("HLS proxy listener registration failed: {}", e);
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

  #[allow(dead_code)]
  pub fn port(&self) -> u16 {
    self.inner.port
  }

  pub async fn activate(&self, origin_playlist: Url) -> Result<ActivatedHls, HlsProxyError> {
    let session_nonce = uuid::Uuid::new_v4().to_string();

    let resp = download::execute_origin_request_with_retries(
      &self.inner.reqwest_client,
      &origin_playlist,
      None,
      self.inner.store.config().origin_retries,
    )
    .await?;

    let final_url = resp.url().clone();
    let body_text = resp
      .text()
      .await
      .map_err(|e| HlsProxyError::Playlist(e.to_string()))?;

    let captured_creds = extract_emby_credentials(&final_url);
    let mut resource_table = ResourceTable::default();

    let parsed = parse_and_rewrite_playlist(
      &body_text,
      &final_url,
      &captured_creds,
      self.inner.port,
      &session_nonce,
      &mut resource_table,
    )?;

    let (has_end_list, target_duration_secs, rewritten_body) = match &parsed {
      ParsedPlaylist::Master { rewritten_body } => (true, 0.0, rewritten_body.clone()),
      ParsedPlaylist::Media {
        has_end_list,
        target_duration_secs,
        rewritten_body,
      } => (*has_end_list, *target_duration_secs, rewritten_body.clone()),
    };

    let root_res_id = playlist::compute_resource_id(ResourceKind::Playlist, &final_url, None);

    let mut playlist_entries = HashMap::new();
    playlist_entries.insert(
      root_res_id.clone(),
      PlaylistEntry {
        upstream_url: final_url.clone(),
        target_duration_secs,
        has_end_list,
        last_fetch_time: std::time::Instant::now(),
        cached_rewritten_body: rewritten_body,
      },
    );

    let (events_tx, events_rx) = async_channel::unbounded();

    let download_state = Arc::new(download::SessionDownloadState {
      session_nonce: session_nonce.clone(),
      origin_expired_emitted: AtomicBool::new(false),
      cache_disabled_emitted: AtomicBool::new(false),
      playback_failed_emitted: AtomicBool::new(false),
      stream_through_only: AtomicBool::new(self.inner.store.cache_root().is_none()),
      events_tx,
      in_flight: Mutex::new(HashMap::new()),
      keys_cache: Mutex::new(HashMap::new()),
    });

    let _session_dir = self.inner.store.create_session_dir(&session_nonce);
    let cache_enabled = self.inner.store.cache_root().is_some();

    let session = Arc::new(ProxySession {
      session_id: session_nonce.clone(),
      captured_creds,
      resource_table: Arc::new(Mutex::new(resource_table)),
      playlist_entries: Arc::new(Mutex::new(playlist_entries)),
      download_state,
      prefetch_cancel_token: Arc::new(Mutex::new(CancellationToken::new())),
      cancel_token: CancellationToken::new(),
    });

    self
      .inner
      .sessions
      .write()
      .insert(session_nonce.clone(), session);

    let playlist_url = playlist::make_local_url(
      self.inner.port,
      &session_nonce,
      ResourceKind::Playlist,
      &root_res_id,
    );

    Ok(ActivatedHls {
      session_id: session_nonce,
      playlist_url,
      events: events_rx,
      cache_enabled,
    })
  }

  pub fn cancel_prefetch(&self, session_id: &str) {
    if let Some(session) = self.inner.sessions.read().get(session_id) {
      session.cancel_prefetch();
    }
  }

  pub fn deactivate(&self, session_id: &str) {
    let session_opt = self.inner.sessions.write().remove(session_id);
    if let Some(session) = session_opt {
      session.cancel_token.cancel();
      let store = self.inner.store.clone();
      let nonce = session_id.to_string();
      let grace = store.config().deactivate_grace;
      tauri::async_runtime::spawn(async move {
        tokio::time::sleep(grace).await;
        store.cleanup_session(&nonce);
      });
    }
  }
}

impl Drop for HlsProxy {
  fn drop(&mut self) {
    self.shutdown_token.cancel();
  }
}

async fn handle_playlist(
  State(inner): State<Arc<HlsProxyInner>>,
  Path((session_nonce, resource_filename)): Path<(String, String)>,
) -> Response {
  let resource_id = match resource_filename.strip_suffix(".m3u8") {
    Some(id) => id,
    None => return (StatusCode::NOT_FOUND, "").into_response(),
  };

  let session = match inner.sessions.read().get(&session_nonce).cloned() {
    Some(s) => s,
    None => return (StatusCode::NOT_FOUND, "").into_response(),
  };

  enum Action {
    ReturnCached { body: String, cache_control: bool },
    Refetch { upstream_url: Url },
    NotFound,
  }

  let action = {
    let mut entries_guard = session.playlist_entries.lock();
    if let Some(entry) = entries_guard.get_mut(resource_id) {
      if entry.has_end_list {
        Action::ReturnCached {
          body: entry.cached_rewritten_body.clone(),
          cache_control: false,
        }
      } else {
        let min_interval = std::cmp::max_by(
          std::time::Duration::from_millis(500),
          std::time::Duration::from_secs_f64(entry.target_duration_secs / 2.0),
          |a, b| a.cmp(b),
        );

        if entry.last_fetch_time.elapsed() < min_interval {
          Action::ReturnCached {
            body: entry.cached_rewritten_body.clone(),
            cache_control: true,
          }
        } else {
          Action::Refetch {
            upstream_url: entry.upstream_url.clone(),
          }
        }
      }
    } else {
      let table_guard = session.resource_table.lock();
      if let Some(res_info) = table_guard.resources.get(resource_id) {
        if res_info.kind == ResourceKind::Playlist {
          Action::Refetch {
            upstream_url: res_info.upstream_url.clone(),
          }
        } else {
          Action::NotFound
        }
      } else {
        Action::NotFound
      }
    }
  };

  match action {
    Action::ReturnCached {
      body,
      cache_control,
    } => {
      if cache_control {
        (
          StatusCode::OK,
          [
            (header::CONTENT_TYPE, "application/vnd.apple.mpegurl"),
            (header::CACHE_CONTROL, "no-store"),
          ],
          body,
        )
          .into_response()
      } else {
        (
          StatusCode::OK,
          [(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")],
          body,
        )
          .into_response()
      }
    }
    Action::NotFound => (StatusCode::NOT_FOUND, "").into_response(),
    Action::Refetch { upstream_url } => {
      let resp_res = download::execute_origin_request_with_retries(
        &inner.reqwest_client,
        &upstream_url,
        None,
        inner.store.config().origin_retries,
      )
      .await;

      if let Ok(resp) = resp_res {
        let final_url = resp.url().clone();
        if let Ok(body_text) = resp.text().await {
          let mut table_guard = session.resource_table.lock();
          if let Ok(parsed) = parse_and_rewrite_playlist(
            &body_text,
            &final_url,
            &session.captured_creds,
            inner.port,
            &session_nonce,
            &mut table_guard,
          ) {
            let (has_end_list, target_duration_secs, rewritten) = match parsed {
              ParsedPlaylist::Master { rewritten_body } => (true, 0.0, rewritten_body),
              ParsedPlaylist::Media {
                has_end_list,
                target_duration_secs,
                rewritten_body,
              } => (has_end_list, target_duration_secs, rewritten_body),
            };
            drop(table_guard);

            let mut entries_guard = session.playlist_entries.lock();
            entries_guard.insert(
              resource_id.to_string(),
              PlaylistEntry {
                upstream_url: final_url,
                target_duration_secs,
                has_end_list,
                last_fetch_time: std::time::Instant::now(),
                cached_rewritten_body: rewritten.clone(),
              },
            );
            drop(entries_guard);

            return (
              StatusCode::OK,
              [
                (header::CONTENT_TYPE, "application/vnd.apple.mpegurl"),
                (header::CACHE_CONTROL, "no-store"),
              ],
              rewritten,
            )
              .into_response();
          }
        }
      }

      if !session
        .download_state
        .playback_failed_emitted
        .swap(true, Ordering::SeqCst)
      {
        let _ = session
          .download_state
          .events_tx
          .try_send(HlsProxyEvent::PlaybackFailed);
      }
      (StatusCode::BAD_GATEWAY, "").into_response()
    }
  }
}

async fn handle_resource(
  State(inner): State<Arc<HlsProxyInner>>,
  Path((session_nonce, resource_id)): Path<(String, String)>,
  headers: HeaderMap,
) -> Response {
  let session = match inner.sessions.read().get(&session_nonce).cloned() {
    Some(s) => s,
    None => return (StatusCode::NOT_FOUND, "").into_response(),
  };

  let res_info = match session
    .resource_table
    .lock()
    .resources
    .get(&resource_id)
    .cloned()
  {
    Some(r) => r,
    None => return (StatusCode::NOT_FOUND, "").into_response(),
  };

  if let Some(idx) = res_info.segment_index {
    session.trigger_prefetch(
      inner.store.clone(),
      inner.reqwest_client.clone(),
      idx,
      inner.port,
      &session_nonce,
    );
  }

  // Collect pinned paths
  let mut pinned = Vec::new();
  {
    let table = session.resource_table.lock();
    for r in table.resources.values() {
      if r.kind == ResourceKind::Map {
        if let Some(p) = inner.store.bin_path(&session_nonce, &r.resource_id) {
          pinned.push(p);
        }
      }
    }
    if let Some(curr_path) = inner.store.bin_path(&session_nonce, &resource_id) {
      pinned.push(curr_path);
    }
    if let Some(idx) = res_info.segment_index {
      let total = table.segment_sequence.len();
      let end = (idx + 1 + inner.store.config().prefetch_ahead).min(total);
      for seg_id in &table.segment_sequence[idx + 1..end] {
        if let Some(p) = inner.store.bin_path(&session_nonce, seg_id) {
          pinned.push(p);
        }
      }
    }
  }

  let range_header = headers.get(header::RANGE).cloned();

  download::handle_resource_request(
    inner.store.clone(),
    session.download_state.clone(),
    inner.reqwest_client.clone(),
    res_info,
    Method::GET,
    range_header,
    pinned,
  )
  .await
}
