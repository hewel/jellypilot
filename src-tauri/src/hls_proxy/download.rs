use std::fs;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::Response;
use bytes::Bytes;
use futures_util::stream::{unfold, Stream};
use futures_util::StreamExt;
use parking_lot::Mutex;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Notify;
use tokio_util::io::ReaderStream;
use url::Url;

use super::playlist::ResourceInfo;
use super::store::StoreManager;
use super::{HlsProxyError, HlsProxyEvent};

pub struct TransferConfig {
  pub status_code: StatusCode,
  pub headers: HeaderMap,
  pub total_length: Option<u64>,
  pub part_path: Option<PathBuf>,
  pub bin_path: Option<PathBuf>,
  pub is_stream_through: bool,
}

pub struct InFlightTransfer {
  pub config: Mutex<TransferConfig>,
  pub bytes_written: AtomicU64,
  pub completed: AtomicBool,
  pub failed: Mutex<Option<String>>,
  pub notify: Notify,
  pub stream_through_chunks: Mutex<Vec<Bytes>>,
}

pub struct SessionDownloadState {
  pub session_nonce: String,
  pub origin_expired_emitted: AtomicBool,
  pub cache_disabled_emitted: AtomicBool,
  pub playback_failed_emitted: AtomicBool,
  pub stream_through_only: AtomicBool,
  pub events_tx: async_channel::Sender<HlsProxyEvent>,
  pub in_flight: Mutex<std::collections::HashMap<String, Arc<InFlightTransfer>>>,
  pub keys_cache: Mutex<std::collections::HashMap<String, Bytes>>,
}

pub fn make_reqwest_client() -> reqwest::Client {
  reqwest::Client::builder()
    .pool_max_idle_per_host(0)
    .connect_timeout(Duration::from_secs(10))
    .build()
    .unwrap_or_default()
}

pub async fn execute_origin_request_with_retries(
  client: &reqwest::Client,
  upstream_url: &Url,
  range: Option<(u64, u64)>,
  max_retries: u32,
) -> Result<reqwest::Response, HlsProxyError> {
  let mut attempts = 0;
  loop {
    attempts += 1;
    let mut req_builder = client.get(upstream_url.clone());
    if let Some((offset, length)) = range {
      req_builder = req_builder.header(
        header::RANGE,
        format!("bytes={}-{}", offset, offset + length - 1),
      );
    }

    let res_fut = req_builder.send();
    let res_result = tokio::time::timeout(Duration::from_secs(15), res_fut).await;

    match res_result {
      Ok(Ok(resp)) => {
        let status = resp.status();
        if status.is_success() || status == StatusCode::PARTIAL_CONTENT {
          return Ok(resp);
        } else if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
          if attempts <= max_retries + 1 {
            let mut backoff = if attempts == 1 {
              Duration::from_millis(100)
            } else {
              Duration::from_millis(300)
            };
            if let Some(retry_after) = resp.headers().get(header::RETRY_AFTER) {
              if let Ok(s) = retry_after.to_str() {
                if let Ok(secs) = s.parse::<u64>() {
                  backoff = Duration::from_secs(secs).min(Duration::from_secs(1));
                }
              }
            }
            tokio::time::sleep(backoff).await;
            continue;
          }
          return Err(HlsProxyError::OriginStatus(status));
        } else {
          return Err(HlsProxyError::OriginStatus(status));
        }
      }
      Ok(Err(err)) => {
        if attempts <= max_retries + 1 {
          let backoff = if attempts == 1 {
            Duration::from_millis(100)
          } else {
            Duration::from_millis(300)
          };
          tokio::time::sleep(backoff).await;
          continue;
        }
        return Err(HlsProxyError::Origin(err));
      }
      Err(_) => {
        if attempts <= max_retries + 1 {
          let backoff = if attempts == 1 {
            Duration::from_millis(100)
          } else {
            Duration::from_millis(300)
          };
          tokio::time::sleep(backoff).await;
          continue;
        }
        return Err(HlsProxyError::Playlist("Origin header timeout".to_string()));
      }
    }
  }
}

pub fn filter_representation_headers(orig: &HeaderMap) -> HeaderMap {
  let mut headers = HeaderMap::new();
  for name in &[
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CONTENT_RANGE,
    header::ETAG,
    header::LAST_MODIFIED,
  ] {
    if let Some(val) = orig.get(name) {
      headers.insert(name.clone(), val.clone());
    }
  }
  headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
  headers
}

fn new_placeholder_transfer() -> Arc<InFlightTransfer> {
  Arc::new(InFlightTransfer {
    config: Mutex::new(TransferConfig {
      status_code: StatusCode::OK,
      headers: HeaderMap::new(),
      total_length: None,
      part_path: None,
      bin_path: None,
      is_stream_through: false,
    }),
    bytes_written: AtomicU64::new(0),
    completed: AtomicBool::new(false),
    failed: Mutex::new(None),
    notify: Notify::new(),
    stream_through_chunks: Mutex::new(Vec::new()),
  })
}

pub async fn handle_resource_request(
  store: Arc<StoreManager>,
  session_state: Arc<SessionDownloadState>,
  client: reqwest::Client,
  res_info: ResourceInfo,
  method: Method,
  client_range_header: Option<HeaderValue>,
  pinned_paths: Vec<PathBuf>,
) -> Response<Body> {
  let is_head = method == Method::HEAD;

  // 1. Encryption Key handling with single flight dedup
  if res_info.is_key {
    let key_bytes = loop {
      if let Some(b) = session_state
        .keys_cache
        .lock()
        .get(&res_info.resource_id)
        .cloned()
      {
        break b;
      }

      let (transfer, is_first) = {
        let mut map = session_state.in_flight.lock();
        if let Some(existing) = map.get(&res_info.resource_id) {
          (existing.clone(), false)
        } else {
          let t = new_placeholder_transfer();
          map.insert(res_info.resource_id.clone(), t.clone());
          (t, true)
        }
      };

      let mut notified = std::pin::pin!(transfer.notify.notified());
      notified.as_mut().enable();

      if is_first {
        let resp = match execute_origin_request_with_retries(
          &client,
          &res_info.upstream_url,
          None,
          store.config().origin_retries,
        )
        .await
        {
          Ok(r) => r,
          Err(err) => {
            check_and_emit_origin_expired(&session_state, &err);
            session_state.in_flight.lock().remove(&res_info.resource_id);
            transfer.notify.notify_waiters();
            let status = match err {
              HlsProxyError::OriginStatus(s) => s,
              _ => StatusCode::BAD_GATEWAY,
            };
            return Response::builder()
              .status(status)
              .body(Body::empty())
              .unwrap();
          }
        };
        let b = match resp.bytes().await {
          Ok(b) => b,
          Err(_) => {
            session_state.in_flight.lock().remove(&res_info.resource_id);
            transfer.notify.notify_waiters();
            return Response::builder()
              .status(StatusCode::BAD_GATEWAY)
              .body(Body::empty())
              .unwrap();
          }
        };
        session_state
          .keys_cache
          .lock()
          .insert(res_info.resource_id.clone(), b.clone());
        session_state.in_flight.lock().remove(&res_info.resource_id);
        transfer.notify.notify_waiters();
        break b;
      } else {
        notified.await;
      }
    };

    let builder = Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, "application/octet-stream")
      .header(header::CONTENT_LENGTH, key_bytes.len().to_string());
    if is_head {
      return builder.body(Body::empty()).unwrap();
    } else {
      return builder.body(Body::from(key_bytes)).unwrap();
    }
  }

  // 2. Ready hit on disk (.bin file)
  if let Some(bin_path) = store.bin_path(&session_state.session_nonce, &res_info.resource_id) {
    if bin_path.exists() {
      store.record_read(&bin_path);
      let _guard = store.register_reader(&bin_path);

      let metadata = match fs::metadata(&bin_path) {
        Ok(m) => m,
        Err(_) => {
          return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap()
        }
      };
      let total_len = metadata.len();

      return build_file_or_range_response(bin_path, total_len, client_range_header, is_head).await;
    }
  }

  // 3. In-flight check or single-flight create
  let (transfer, is_first) = {
    let mut map = session_state.in_flight.lock();
    if let Some(existing) = map.get(&res_info.resource_id) {
      (existing.clone(), false)
    } else {
      let t = new_placeholder_transfer();
      map.insert(res_info.resource_id.clone(), t.clone());
      (t, true)
    }
  };

  if is_first {
    let origin_resp = match execute_origin_request_with_retries(
      &client,
      &res_info.upstream_url,
      res_info.effective_byte_range,
      store.config().origin_retries,
    )
    .await
    {
      Ok(r) => r,
      Err(err) => {
        check_and_emit_origin_expired(&session_state, &err);
        session_state.in_flight.lock().remove(&res_info.resource_id);
        transfer.notify.notify_waiters();
        let status = match err {
          HlsProxyError::OriginStatus(s) => s,
          _ => StatusCode::BAD_GATEWAY,
        };
        return Response::builder()
          .status(status)
          .body(Body::empty())
          .unwrap();
      }
    };

    let headers = filter_representation_headers(origin_resp.headers());
    let content_length = origin_resp.content_length();

    let stream_through_only = session_state.stream_through_only.load(Ordering::Relaxed);
    let admits_to_disk = if stream_through_only {
      false
    } else {
      store.check_space_and_admit(content_length, &pinned_paths)
    };

    let part_path = if admits_to_disk {
      store.part_path(&session_state.session_nonce, &res_info.resource_id)
    } else {
      None
    };

    let bin_path = if admits_to_disk {
      store.bin_path(&session_state.session_nonce, &res_info.resource_id)
    } else {
      None
    };

    {
      let mut cfg = transfer.config.lock();
      cfg.status_code = origin_resp.status();
      cfg.headers = headers;
      cfg.total_length = content_length;
      cfg.part_path = part_path;
      cfg.bin_path = bin_path;
      cfg.is_stream_through = !admits_to_disk;
    }

    // Spawn download task
    let session_state_clone = session_state.clone();
    let res_id_clone = res_info.resource_id.clone();
    let transfer_clone = transfer.clone();

    tokio::spawn(async move {
      run_download_task(
        origin_resp,
        transfer_clone,
        session_state_clone,
        res_id_clone,
      )
      .await;
    });
  }

  // Build response for in-flight transfer
  build_in_flight_response(transfer, client_range_header, is_head).await
}

fn check_and_emit_origin_expired(session_state: &SessionDownloadState, err: &HlsProxyError) {
  if let HlsProxyError::OriginStatus(s) = err {
    if matches!(
      *s,
      StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND | StatusCode::GONE
    ) && !session_state
      .origin_expired_emitted
      .swap(true, Ordering::SeqCst)
    {
      let _ = session_state
        .events_tx
        .try_send(HlsProxyEvent::OriginExpired);
    }
  }
}

async fn run_download_task(
  resp: reqwest::Response,
  transfer: Arc<InFlightTransfer>,
  session_state: Arc<SessionDownloadState>,
  resource_id: String,
) {
  let mut stream = resp.bytes_stream();
  let mut file_opt = {
    let part_path = transfer.config.lock().part_path.clone();
    if let Some(ref path) = part_path {
      if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
      }
      File::create(path).await.ok()
    } else {
      None
    }
  };

  let mut failed = false;

  while let Ok(Some(chunk_res)) = tokio::time::timeout(Duration::from_secs(15), stream.next()).await
  {
    match chunk_res {
      Ok(chunk) => {
        let len = chunk.len() as u64;
        if let Some(ref mut file) = file_opt {
          if file.write_all(&chunk).await.is_err() {
            failed = true;
            break;
          }
        } else {
          transfer.stream_through_chunks.lock().push(chunk);
        }
        transfer.bytes_written.fetch_add(len, Ordering::SeqCst);
        transfer.notify.notify_waiters();
      }
      Err(_) => {
        failed = true;
        break;
      }
    }
  }

  if failed {
    let part_path = transfer.config.lock().part_path.clone();
    if let Some(ref path) = part_path {
      let _ = tokio::fs::remove_file(path).await;
    }
    *transfer.failed.lock() = Some("Download stream failed".to_string());
    session_state
      .stream_through_only
      .store(true, Ordering::SeqCst);
    if !session_state
      .cache_disabled_emitted
      .swap(true, Ordering::SeqCst)
    {
      let _ = session_state
        .events_tx
        .try_send(HlsProxyEvent::CacheDisabled);
    }
  } else {
    if let Some(ref mut file) = file_opt {
      let _ = file.flush().await;
      drop(file_opt);
      let (part_path, bin_path) = {
        let cfg = transfer.config.lock();
        (cfg.part_path.clone(), cfg.bin_path.clone())
      };
      if let (Some(part), Some(bin)) = (part_path, bin_path) {
        let _ = tokio::fs::rename(part, bin).await;
      }
    }
    transfer.completed.store(true, Ordering::SeqCst);
  }

  transfer.notify.notify_waiters();
  session_state.in_flight.lock().remove(&resource_id);
}

async fn build_file_or_range_response(
  path: PathBuf,
  total_len: u64,
  range_header: Option<HeaderValue>,
  is_head: bool,
) -> Response<Body> {
  let file = match File::open(&path).await {
    Ok(f) => f,
    Err(_) => {
      return Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::empty())
        .unwrap()
    }
  };

  if let Some(range_val) = range_header {
    if let Ok(range_str) = range_val.to_str() {
      if let Some((start, end)) = parse_bytes_range(range_str, total_len) {
        let len = end - start + 1;
        let builder = Response::builder()
          .status(StatusCode::PARTIAL_CONTENT)
          .header(header::CONTENT_TYPE, "video/mp4")
          .header(header::CONTENT_LENGTH, len.to_string())
          .header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, total_len),
          )
          .header(header::ACCEPT_RANGES, "bytes");

        if is_head {
          return builder.body(Body::empty()).unwrap();
        } else {
          let mut f = file;
          let _ = f.seek(SeekFrom::Start(start)).await;
          let limited = f.take(len);
          let stream = ReaderStream::new(limited);
          return builder.body(Body::from_stream(stream)).unwrap();
        }
      } else {
        return Response::builder()
          .status(StatusCode::RANGE_NOT_SATISFIABLE)
          .header(header::CONTENT_RANGE, format!("bytes */{}", total_len))
          .body(Body::empty())
          .unwrap();
      }
    }
  }

  let builder = Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, "video/mp4")
    .header(header::CONTENT_LENGTH, total_len.to_string())
    .header(header::ACCEPT_RANGES, "bytes");

  if is_head {
    builder.body(Body::empty()).unwrap()
  } else {
    let stream = ReaderStream::new(file);
    builder.body(Body::from_stream(stream)).unwrap()
  }
}

async fn build_in_flight_response(
  transfer: Arc<InFlightTransfer>,
  range_header: Option<HeaderValue>,
  is_head: bool,
) -> Response<Body> {
  let (status_code, headers, total_len) = {
    let cfg = transfer.config.lock();
    (
      cfg.status_code,
      cfg.headers.clone(),
      cfg.total_length.unwrap_or(0),
    )
  };

  if let Some(range_val) = range_header {
    if total_len > 0 {
      if let Ok(range_str) = range_val.to_str() {
        if let Some((start, end)) = parse_bytes_range(range_str, total_len) {
          let len = end - start + 1;
          let builder = Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, "video/mp4")
            .header(header::CONTENT_LENGTH, len.to_string())
            .header(
              header::CONTENT_RANGE,
              format!("bytes {}-{}/{}", start, end, total_len),
            )
            .header(header::ACCEPT_RANGES, "bytes");

          if is_head {
            return builder.body(Body::empty()).unwrap();
          } else {
            let stream = make_in_flight_stream(transfer, start, len);
            return builder.body(Body::from_stream(stream)).unwrap();
          }
        }
      }
    }
  }

  let mut builder = Response::builder().status(status_code);
  for (k, v) in &headers {
    builder = builder.header(k, v);
  }

  if is_head {
    builder.body(Body::empty()).unwrap()
  } else {
    let stream = make_in_flight_stream(transfer, 0, total_len);
    builder.body(Body::from_stream(stream)).unwrap()
  }
}

struct StreamState {
  transfer: Arc<InFlightTransfer>,
  current_offset: u64,
  remaining: Option<u64>,
}

fn make_in_flight_stream(
  transfer: Arc<InFlightTransfer>,
  offset: u64,
  length: u64,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
  let initial_state = StreamState {
    transfer,
    current_offset: offset,
    remaining: if length > 0 { Some(length) } else { None },
  };

  unfold(initial_state, |mut state| async move {
    loop {
      if let Some(rem) = state.remaining {
        if rem == 0 {
          return None;
        }
      }

      // Clone the Arc so `notified` borrows a local, not `state` —
      // this lets us move `state` in return values without borrow conflicts.
      // enable() eagerly registers interest BEFORE checking conditions,
      // preventing lost wakeups between the check and the await.
      let transfer_arc = state.transfer.clone();
      let mut notified = std::pin::pin!(transfer_arc.notify.notified());
      notified.as_mut().enable();

      if state.transfer.failed.lock().is_some() {
        return Some((Err(std::io::Error::other("Download failed")), state));
      }

      let written = state.transfer.bytes_written.load(Ordering::SeqCst);
      if state.current_offset < written {
        let is_stream_through = state.transfer.config.lock().is_stream_through;
        if is_stream_through {
          let chunks = state.transfer.stream_through_chunks.lock().clone();
          let mut pos = 0;
          for chunk in chunks {
            let c_len = chunk.len() as u64;
            if state.current_offset >= pos && state.current_offset < pos + c_len {
              let slice_start = (state.current_offset - pos) as usize;
              let avail = (c_len as usize) - slice_start;
              let to_take = if let Some(rem) = state.remaining {
                (rem as usize).min(avail)
              } else {
                avail
              };
              let slice = chunk.slice(slice_start..slice_start + to_take);
              state.current_offset += to_take as u64;
              if let Some(ref mut rem) = state.remaining {
                *rem -= to_take as u64;
              }
              return Some((Ok(slice), state));
            }
            pos += c_len;
          }
        } else {
          let part_path = state.transfer.config.lock().part_path.clone();
          if let Some(ref part_path) = part_path {
            if let Ok(mut file) = File::open(part_path).await {
              let _ = file.seek(SeekFrom::Start(state.current_offset)).await;
              let mut buf = vec![0u8; 64 * 1024];
              let to_read = if let Some(rem) = state.remaining {
                (rem as usize).min(buf.len())
              } else {
                buf.len()
              };
              if let Ok(n) = file.read(&mut buf[..to_read]).await {
                if n > 0 {
                  let bytes = Bytes::copy_from_slice(&buf[..n]);
                  state.current_offset += n as u64;
                  if let Some(ref mut rem) = state.remaining {
                    *rem -= n as u64;
                  }
                  return Some((Ok(bytes), state));
                }
              }
            }
          }
        }
      } else if state.transfer.completed.load(Ordering::SeqCst) {
        return None;
      }

      // Wait for notification (registered before checks above)
      notified.await;
    }
  })
}

fn parse_bytes_range(header_val: &str, total_len: u64) -> Option<(u64, u64)> {
  if !header_val.starts_with("bytes=") {
    return None;
  }
  let s = &header_val["bytes=".len()..];
  let parts: Vec<&str> = s.split('-').collect();
  if parts.len() != 2 {
    return None;
  }
  let start: u64 = parts[0].parse().ok()?;
  let end: u64 = if parts[1].is_empty() {
    total_len.checked_sub(1)?
  } else {
    parts[1].parse().ok()?
  };
  if start <= end && end < total_len {
    Some((start, end))
  } else {
    None
  }
}
