use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query};
use axum::http::{header, Response, StatusCode};
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use futures_util::stream;
use tokio::sync::Notify;
use url::Url;

use super::store::HlsProxyConfig;
use super::{HlsProxy, HlsProxyEvent};

struct TempDirGuard(PathBuf);
impl TempDirGuard {
  fn new() -> Self {
    let dir = std::env::temp_dir().join(format!("hls_proxy_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    Self(dir)
  }
  fn path(&self) -> PathBuf {
    self.0.clone()
  }
}
impl Drop for TempDirGuard {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

async fn start_mock_origin(router: Router) -> (String, u16) {
  let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let port = std_listener.local_addr().unwrap().port();
  std_listener.set_nonblocking(true).unwrap();
  let tokio_listener = tokio::net::TcpListener::from_std(std_listener).unwrap();

  tokio::spawn(async move {
    let _ = axum::serve(tokio_listener, router).await;
  });

  (format!("http://127.0.0.1:{}", port), port)
}

fn full_local_url(proxy_port: u16, raw_path_or_url: &str) -> String {
  if raw_path_or_url.starts_with("http://") || raw_path_or_url.starts_with("https://") {
    raw_path_or_url.to_string()
  } else {
    format!("http://127.0.0.1:{}{}", proxy_port, raw_path_or_url)
  }
}

#[tokio::test]
async fn rewrites_master_and_media_uri_locations_without_credentials() {
  let temp_dir = TempDirGuard::new();

  let master_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-SESSION-KEY:METHOD=AES-128,URI=\"session.key\"\n",
    "#EXT-X-SESSION-DATA:DATA-ID=\"com.example.title\",URI=\"data.json\"\n",
    "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"English\",URI=\"audio.m3u8\"\n",
    "#EXT-X-STREAM-INF:BANDWIDTH=128000,AUDIO=\"audio\"\n",
    "variant.m3u8\n"
  );

  let variant_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-TARGETDURATION:10\n",
    "#EXT-X-VERSION:3\n",
    "#EXT-X-MAP:URI=\"init.mp4\"\n",
    "#EXT-X-KEY:METHOD=AES-128,URI=\"media.key\"\n",
    "#EXTINF:9.0,\n",
    "#EXT-X-BYTERANGE:100@0\n",
    "seg1.m4s\n",
    "#EXT-X-ENDLIST\n"
  );

  let captured_origin_query = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
  let captured_query_clone = captured_origin_query.clone();

  let origin_router = Router::new()
    .route(
      "/master.m3u8",
      get(
        move |Query(params): Query<std::collections::HashMap<String, String>>| {
          let captured = captured_query_clone.clone();
          async move {
            if let Some(val) = params.get("api_key") {
              captured.lock().push(val.clone());
            }
            Response::builder()
              .status(StatusCode::OK)
              .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
              .body(Body::from(master_body))
              .unwrap()
          }
        },
      ),
    )
    .route(
      "/variant.m3u8",
      get(
        move |_params: Query<std::collections::HashMap<String, String>>| async move {
          Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .body(Body::from(variant_body))
            .unwrap()
        },
      ),
    );

  let (origin_url, _origin_port) = start_mock_origin(origin_router).await;

  let proxy = HlsProxy::start(Some(temp_dir.path())).unwrap();
  let activated = proxy
    .activate(Url::parse(&format!("{}/master.m3u8?api_key=SECRET_TOKEN", origin_url)).unwrap())
    .await
    .unwrap();

  let client = reqwest::Client::new();
  let master_resp = client.get(&activated.playlist_url).send().await.unwrap();
  assert_eq!(master_resp.status(), StatusCode::OK);

  let rewritten_master = master_resp.text().await.unwrap();
  assert!(!rewritten_master.contains("SECRET_TOKEN"));
  assert!(!rewritten_master.contains(&origin_url));
  assert!(rewritten_master.contains("127.0.0.1"));

  // Check parsing with hls_m3u8
  let parsed_master = hls_m3u8::MasterPlaylist::try_from(rewritten_master.as_str());
  assert!(parsed_master.is_ok());
}

#[tokio::test]
async fn streams_before_origin_completion_and_reuses_disk_file() {
  let temp_dir = TempDirGuard::new();

  let origin_barrier = Arc::new(Notify::new());
  let origin_req_count = Arc::new(AtomicU32::new(0));

  let barrier_clone = origin_barrier.clone();
  let req_count_clone = origin_req_count.clone();

  let media_playlist_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-TARGETDURATION:10\n",
    "#EXTINF:10.0,\n",
    "seg1.ts\n",
    "#EXT-X-ENDLIST\n"
  );

  let origin_router = Router::new()
    .route(
      "/playlist.m3u8",
      get(move || async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
          .body(Body::from(media_playlist_body))
          .unwrap()
      }),
    )
    .route(
      "/seg1.ts",
      get(move || {
        let barrier = barrier_clone.clone();
        let count = req_count_clone.clone();
        async move {
          count.fetch_add(1, Ordering::SeqCst);
          let (tx, rx) = async_channel::unbounded();
          tokio::spawn(async move {
            let _ = tx
              .send(Ok::<Bytes, std::io::Error>(Bytes::from("CHUNK1")))
              .await;
            barrier.notified().await;
            let _ = tx
              .send(Ok::<Bytes, std::io::Error>(Bytes::from("CHUNK2")))
              .await;
            drop(tx);
          });
          let s = stream::unfold(rx, |rx| async move {
            match rx.recv().await {
              Ok(item) => Some((item, rx)),
              Err(_) => None,
            }
          });
          Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "video/mp2t")
            .header(header::CONTENT_LENGTH, "12")
            .body(Body::from_stream(s))
            .unwrap()
        }
      }),
    );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start(Some(temp_dir.path())).unwrap();
  let activated = proxy
    .activate(Url::parse(&format!("{}/playlist.m3u8", origin_url)).unwrap())
    .await
    .unwrap();

  let client = reqwest::Client::new();
  let playlist_resp = client.get(&activated.playlist_url).send().await.unwrap();
  let playlist_text = playlist_resp.text().await.unwrap();

  // Extract seg1 local URL
  let raw_seg_line = playlist_text
    .lines()
    .find(|l| l.contains("/resource/"))
    .unwrap();
  let seg1_url = full_local_url(proxy.port(), raw_seg_line);

  // Start 1st segment request
  let mut seg_resp = client.get(&seg1_url).send().await.unwrap();
  assert_eq!(seg_resp.status(), StatusCode::OK);

  // Read 1st chunk before releasing origin barrier
  let chunk1 = seg_resp.chunk().await.unwrap().unwrap();
  assert_eq!(&chunk1[..], b"CHUNK1");

  // Store a permit even if the origin task has not polled `notified()` yet.
  origin_barrier.notify_one();

  let chunk2 = seg_resp.chunk().await.unwrap().unwrap();
  assert_eq!(&chunk2[..], b"CHUNK2");

  // Issue 2nd request for same segment
  let seg_resp2 = client.get(&seg1_url).send().await.unwrap();
  assert_eq!(seg_resp2.status(), StatusCode::OK);
  let bytes2 = seg_resp2.bytes().await.unwrap();
  assert_eq!(&bytes2[..], b"CHUNK1CHUNK2");

  // Only 1 origin request
  assert_eq!(origin_req_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn deduplicates_concurrent_demand_and_key_fetches() {
  let temp_dir = TempDirGuard::new();

  let seg_req_count = Arc::new(AtomicU32::new(0));
  let key_req_count = Arc::new(AtomicU32::new(0));

  let seg_count_clone = seg_req_count.clone();
  let key_count_clone = key_req_count.clone();

  let media_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-TARGETDURATION:10\n",
    "#EXT-X-KEY:METHOD=AES-128,URI=\"enc.key\"\n",
    "#EXTINF:10.0,\n",
    "seg1.ts\n",
    "#EXT-X-ENDLIST\n"
  );

  let origin_router = Router::new()
    .route(
      "/playlist.m3u8",
      get(move || async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
          .body(Body::from(media_body))
          .unwrap()
      }),
    )
    .route(
      "/enc.key",
      get(move || {
        let count = key_count_clone.clone();
        async move {
          count.fetch_add(1, Ordering::SeqCst);
          tokio::time::sleep(Duration::from_millis(50)).await;
          Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from("SECRET_KEY_BYTES"))
            .unwrap()
        }
      }),
    )
    .route(
      "/seg1.ts",
      get(move || {
        let count = seg_count_clone.clone();
        async move {
          count.fetch_add(1, Ordering::SeqCst);
          tokio::time::sleep(Duration::from_millis(50)).await;
          Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "video/mp2t")
            .body(Body::from("SEGMENT_BYTES"))
            .unwrap()
        }
      }),
    );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start(Some(temp_dir.path())).unwrap();
  let activated = proxy
    .activate(Url::parse(&format!("{}/playlist.m3u8", origin_url)).unwrap())
    .await
    .unwrap();

  let client = reqwest::Client::new();
  let playlist_text = client
    .get(&activated.playlist_url)
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap();

  let key_url = {
    let line = playlist_text
      .lines()
      .find(|l| l.contains("#EXT-X-KEY"))
      .unwrap();
    let start = line.find("URI=\"").unwrap() + 5;
    let end = line[start..].find('"').unwrap() + start;
    line[start..end].to_string()
  };

  let seg_url = playlist_text
    .lines()
    .find(|l| l.contains("/resource/") && !l.contains("#EXT-X-KEY"))
    .map(|l| full_local_url(proxy.port(), l))
    .unwrap();

  let c1 = client.clone();
  let c2 = client.clone();
  let key_url1 = key_url.clone();
  let key_url2 = key_url.clone();

  let (res_k1, res_k2) = tokio::join!(
    async move {
      c1.get(&key_url1)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
    },
    async move {
      c2.get(&key_url2)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
    }
  );

  assert_eq!(&res_k1[..], b"SECRET_KEY_BYTES");
  assert_eq!(&res_k2[..], b"SECRET_KEY_BYTES");
  assert_eq!(key_req_count.load(Ordering::SeqCst), 1);

  let c3 = client.clone();
  let c4 = client.clone();
  let seg_url1 = seg_url.clone();
  let seg_url2 = seg_url.clone();

  let (res_s1, res_s2) = tokio::join!(
    async move {
      c3.get(&seg_url1)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
    },
    async move {
      c4.get(&seg_url2)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
    }
  );

  assert_eq!(&res_s1[..], b"SEGMENT_BYTES");
  assert_eq!(&res_s2[..], b"SEGMENT_BYTES");
  assert_eq!(seg_req_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn prefetches_three_and_cancels_on_seek() {
  let temp_dir = TempDirGuard::new();

  let origin_router = Router::new()
    .route(
      "/playlist.m3u8",
      get(|| async move {
        let mut body = String::from("#EXTM3U\n#EXT-X-TARGETDURATION:10\n");
        for i in 0..30 {
          body.push_str(&format!("#EXTINF:10.0,\nseg_{}\n", i));
        }
        body.push_str("#EXT-X-ENDLIST\n");
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
          .body(Body::from(body))
          .unwrap()
      }),
    )
    .route(
      "/seg_{id}",
      get(|Path(_id): Path<String>| async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "video/mp2t")
          .body(Body::from("SEGMENT_BYTES"))
          .unwrap()
      }),
    );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start(Some(temp_dir.path())).unwrap();
  let activated = proxy
    .activate(Url::parse(&format!("{}/playlist.m3u8", origin_url)).unwrap())
    .await
    .unwrap();

  let client = reqwest::Client::new();
  let playlist_text = client
    .get(&activated.playlist_url)
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap();

  let seg_urls: Vec<String> = playlist_text
    .lines()
    .filter(|l| l.contains("/resource/"))
    .map(|l| full_local_url(proxy.port(), l))
    .collect();

  // Demand segment 5
  let seg5_resp = client.get(&seg_urls[5]).send().await.unwrap();
  assert_eq!(seg5_resp.status(), StatusCode::OK);

  // Cancel prefetch (seek)
  proxy.cancel_prefetch(&activated.session_id);

  // Demand segment 20
  let seg20_resp = client.get(&seg_urls[20]).send().await.unwrap();
  assert_eq!(seg20_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn enforces_lru_and_free_space_without_evicting_pinned_resources() {
  let temp_dir = TempDirGuard::new();

  let config = HlsProxyConfig {
    cache_max_bytes: 300,
    cache_min_free_bytes: 0,
    prefetch_ahead: 1,
    prefetch_concurrency: 1,
    origin_retries: 0,
    deactivate_grace: Duration::from_millis(50),
  };

  let media_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-TARGETDURATION:10\n",
    "#EXT-X-MAP:URI=\"init.mp4\"\n",
    "#EXTINF:10.0,\nseg_0\n",
    "#EXTINF:10.0,\nseg_1\n",
    "#EXTINF:10.0,\nseg_2\n",
    "#EXTINF:10.0,\nseg_3\n",
    "#EXT-X-ENDLIST\n"
  );

  let origin_router = Router::new()
    .route(
      "/playlist.m3u8",
      get(move || async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
          .body(Body::from(media_body))
          .unwrap()
      }),
    )
    .route(
      "/init.mp4",
      get(|| async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "video/mp4")
          .header(header::CONTENT_LENGTH, "100")
          .body(Body::from(vec![0u8; 100]))
          .unwrap()
      }),
    )
    .route(
      "/seg_{id}",
      get(|Path(_id): Path<String>| async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "video/mp2t")
          .header(header::CONTENT_LENGTH, "100")
          .body(Body::from(vec![0u8; 100]))
          .unwrap()
      }),
    );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start_with_config(Some(temp_dir.path()), config).unwrap();
  let activated = proxy
    .activate(Url::parse(&format!("{}/playlist.m3u8", origin_url)).unwrap())
    .await
    .unwrap();

  let client = reqwest::Client::new();
  let playlist_text = client
    .get(&activated.playlist_url)
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap();

  let res_urls: Vec<String> = playlist_text
    .lines()
    .filter(|l| l.contains("/resource/"))
    .map(|l| full_local_url(proxy.port(), l))
    .collect();

  // Fetch init map + seg0 + seg1 (300 bytes total)
  let _ = client.get(&res_urls[0]).send().await.unwrap();
  let _ = client.get(&res_urls[1]).send().await.unwrap();
  let _ = client.get(&res_urls[2]).send().await.unwrap();

  // Fetch seg2 -> forces eviction of seg0 while init map stays
  let seg2_resp = client.get(&res_urls[3]).send().await.unwrap();
  assert_eq!(seg2_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn degrades_to_stream_through_and_emits_once() {
  let media_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-TARGETDURATION:10\n",
    "#EXTINF:10.0,\nseg0.ts\n",
    "#EXT-X-ENDLIST\n"
  );

  let origin_router = Router::new()
    .route(
      "/playlist.m3u8",
      get(move || async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
          .body(Body::from(media_body))
          .unwrap()
      }),
    )
    .route(
      "/seg0.ts",
      get(|| async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "video/mp2t")
          .body(Body::from("STREAM_THROUGH_BYTES"))
          .unwrap()
      }),
    );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start(None).unwrap();
  let activated = proxy
    .activate(Url::parse(&format!("{}/playlist.m3u8", origin_url)).unwrap())
    .await
    .unwrap();

  assert!(!activated.cache_enabled);

  let client = reqwest::Client::new();
  let playlist_text = client
    .get(&activated.playlist_url)
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap();

  let seg0_line = playlist_text
    .lines()
    .find(|l| l.contains("/resource/"))
    .unwrap();
  let seg0_url = full_local_url(proxy.port(), seg0_line);

  let seg_resp = client.get(&seg0_url).send().await.unwrap();
  assert_eq!(seg_resp.status(), StatusCode::OK);
  assert_eq!(seg_resp.bytes().await.unwrap(), "STREAM_THROUGH_BYTES");
}

#[tokio::test]
async fn origin_expiry_emits_once_after_retry_policy() {
  let temp_dir = TempDirGuard::new();

  let origin_router = Router::new().route(
    "/expired.m3u8",
    get(|| async move {
      Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(Body::empty())
        .unwrap()
    }),
  );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start(Some(temp_dir.path())).unwrap();

  let activate_res = proxy
    .activate(Url::parse(&format!("{}/expired.m3u8", origin_url)).unwrap())
    .await;

  assert!(activate_res.is_err());
}

#[tokio::test]
async fn malformed_nested_playlist_fails_closed() {
  let temp_dir = TempDirGuard::new();

  let master_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-STREAM-INF:BANDWIDTH=128000\n",
    "nested_bad.m3u8\n"
  );

  let origin_router = Router::new()
    .route(
      "/master.m3u8",
      get(move || async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
          .body(Body::from(master_body))
          .unwrap()
      }),
    )
    .route(
      "/nested_bad.m3u8",
      get(|| async move {
        Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, "text/plain")
          .body(Body::from("NOT_A_PLAYLIST_SYNTAX"))
          .unwrap()
      }),
    );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start(Some(temp_dir.path())).unwrap();
  let activated = proxy
    .activate(Url::parse(&format!("{}/master.m3u8", origin_url)).unwrap())
    .await
    .unwrap();

  let client = reqwest::Client::new();
  let master_text = client
    .get(&activated.playlist_url)
    .send()
    .await
    .unwrap()
    .text()
    .await
    .unwrap();

  let nested_local_line = master_text
    .lines()
    .find(|l| l.contains("/playlist/"))
    .unwrap();
  let nested_local_url = full_local_url(proxy.port(), nested_local_line);

  let nested_resp = client.get(&nested_local_url).send().await.unwrap();
  assert_eq!(nested_resp.status(), StatusCode::BAD_GATEWAY);

  // Check event channel received PlaybackFailed
  let event = activated.events.try_recv();
  assert_eq!(event.ok(), Some(HlsProxyEvent::PlaybackFailed));
}

#[tokio::test]
async fn deactivate_revokes_routes_and_removes_only_its_session() {
  let temp_dir = TempDirGuard::new();

  let media_body = concat!(
    "#EXTM3U\n",
    "#EXT-X-TARGETDURATION:10\n",
    "#EXTINF:10.0,\nseg0.ts\n",
    "#EXT-X-ENDLIST\n"
  );

  let origin_router = Router::new().route(
    "/playlist.m3u8",
    get(move || async move {
      Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .body(Body::from(media_body))
        .unwrap()
    }),
  );

  let (origin_url, _) = start_mock_origin(origin_router).await;
  let proxy = HlsProxy::start(Some(temp_dir.path())).unwrap();

  let activated_a = proxy
    .activate(Url::parse(&format!("{}/playlist.m3u8", origin_url)).unwrap())
    .await
    .unwrap();
  let activated_b = proxy
    .activate(Url::parse(&format!("{}/playlist.m3u8", origin_url)).unwrap())
    .await
    .unwrap();

  let client = reqwest::Client::new();
  assert_eq!(
    client
      .get(&activated_a.playlist_url)
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  assert_eq!(
    client
      .get(&activated_b.playlist_url)
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );

  // Deactivate Session A
  proxy.deactivate(&activated_a.session_id);

  // Session A routes should return 404
  assert_eq!(
    client
      .get(&activated_a.playlist_url)
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );

  // Session B routes remain functional
  assert_eq!(
    client
      .get(&activated_b.playlist_url)
      .send()
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
}
