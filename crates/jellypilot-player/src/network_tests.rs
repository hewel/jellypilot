use super::*;
use gstreamer::{self as gst, prelude::*};
use parking_lot::{Condvar, Mutex};
use std::{
  collections::HashMap,
  io::{BufRead, BufReader, Write},
  net::{TcpListener, TcpStream},
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
  },
  thread,
};

#[derive(Clone)]
struct Resource {
  body: Arc<[u8]>,
  mime: &'static str,
  stall: bool,
}
struct Request {
  path: String,
  authorized: bool,
  range: bool,
}
struct Server {
  address: std::net::SocketAddr,
  stop: Arc<AtomicBool>,
  release: Arc<(Mutex<bool>, Condvar)>,
  requests: mpsc::Receiver<Request>,
  thread: Option<thread::JoinHandle<()>>,
}
impl Server {
  fn start(resources: HashMap<String, Resource>) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (seen, requests) = mpsc::channel();
    let quit = stop.clone();
    let gate = release.clone();
    let worker = thread::spawn(move || {
      let mut connections = Vec::new();
      for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        if quit.load(Ordering::Acquire) {
          break;
        }
        let resources = resources.clone();
        let seen = seen.clone();
        let gate = gate.clone();
        connections.push(thread::spawn(move || {
          stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
          stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
          let mut reader = BufReader::new(&stream);
          let mut first = String::new();
          if reader.read_line(&mut first).is_err() { return; }
          let mut fields = first.split_whitespace();
          let method = fields.next().unwrap_or("");
          let target = fields.next().unwrap_or("/");
          let path = target.split('?').next().unwrap().to_owned();
          let mut authorized = false;
          let mut range = None;
          loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" { break; }
            if let Some((name, value)) = line.split_once(':') {
              if name.eq_ignore_ascii_case("authorization") { authorized = value.trim() == "Bearer network-test-secret"; }
              if name.eq_ignore_ascii_case("range") { range = Some(value.trim().to_owned()); }
            }
          }
          drop(reader);
          let resource = resources.get(&path);
          if !authorized || resource.is_none() {
            let code = if authorized { "404 Not Found" } else { "401 Unauthorized" };
            let _ = write!(stream, "HTTP/1.1 {code}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = seen.send(Request { path, authorized, range: range.is_some() });
            return;
          }
          let resource = resource.unwrap();
          let total = resource.body.len();
          let (start, end) = range.as_deref().and_then(|value| value.strip_prefix("bytes="))
            .and_then(|value| value.split_once('-'))
            .map_or((0, total.saturating_sub(1)), |(start,end)| (start.parse::<usize>().unwrap(), end.parse::<usize>().unwrap_or(total.saturating_sub(1))));
          if start >= total {
            let _ = write!(stream, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return;
          }
          let end = end.min(total - 1);
          let partial = range.is_some();
          let code = if partial { "206 Partial Content" } else { "200 OK" };
          let _ = write!(stream, "HTTP/1.1 {code}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n", resource.mime, end - start + 1);
          if partial { let _ = write!(stream, "Content-Range: bytes {start}-{end}/{total}\r\n"); }
          let _ = write!(stream, "\r\n");
          let _ = stream.flush();
          let _ = seen.send(Request { path, authorized, range: partial });
          if resource.stall {
            let (lock, condition) = &*gate;
            let mut released = lock.lock();
            while !*released { condition.wait(&mut released); }
          }
          if method != "HEAD" { let _ = stream.write_all(&resource.body[start..=end]); }
        }));
      }
      for connection in connections {
        connection.join().unwrap();
      }
    });
    Self {
      address,
      stop,
      release,
      requests,
      thread: Some(worker),
    }
  }
  fn source(&self, path: &str) -> NetworkSource {
    NetworkSource::new(format!(
      "http://{}{path}?api_key=network-query-secret",
      self.address
    ))
    .unwrap()
    .with_header("Authorization", "Bearer network-test-secret")
    .unwrap()
  }
}
impl Drop for Server {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Release);
    *self.release.0.lock() = true;
    self.release.1.notify_all();
    let _ = TcpStream::connect(self.address);
    self.thread.take().unwrap().join().unwrap();
  }
}
fn runtime() -> tokio::runtime::Runtime {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
}

#[test]
fn http_source_authorizes_range_seek_and_recovers_after_remote_error() {
  let (_directory, path) = playback::tests::fixture();
  let bytes: Arc<[u8]> = std::fs::read(&path).unwrap().into();
  let server = Server::start(HashMap::from([(
    "/movie.webm".into(),
    Resource {
      body: bytes,
      mime: "video/webm",
      stall: false,
    },
  )]));
  runtime().block_on(async {
    let (mut player, notifications) = Player::new(AudioOutput::Discard).unwrap();
    let mut notifications = Box::pin(notifications);
    player
      .open(PlaybackSource::Network(server.source("/movie.webm")))
      .unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      s.phase == PlaybackPhase::Playing && s.can_seek()
    })
    .await;
    player.set_playing(false).unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      s.phase == PlaybackPhase::Paused
    })
    .await;
    player.seek(Duration::from_secs(1)).unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      !s.seeking
        && s
          .position
          .is_some_and(|p| p.abs_diff(Duration::from_secs(1)) < Duration::from_millis(170))
    })
    .await;
    assert!(!player.status().playing);
    assert_eq!(player.status().phase, PlaybackPhase::Paused);
    player
      .open(PlaybackSource::Network(server.source("/missing.webm")))
      .unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      s.phase == PlaybackPhase::Error
    })
    .await;
    let diagnostic = format!("{:?}", player.status().error);
    assert!(!diagnostic.contains("network-query-secret"));
    assert!(!diagnostic.contains("network-test-secret"));
    assert!(
      diagnostic.contains("404"),
      "upstream status must remain actionable: {diagnostic}"
    );
    player.open(PlaybackSource::Local(path)).unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      s.phase == PlaybackPhase::Playing
    })
    .await;
    assert!(player.status().error.is_none());
    tests::close(&mut player).await;
  });
  let requests: Vec<_> = server.requests.try_iter().collect();
  assert!(requests.iter().all(|request| request.authorized));
  // A seek must reach the HTTP seam as a Range, not a local full-file download.
  assert!(requests
    .iter()
    .any(|request| request.path == "/movie.webm" && request.range));
}

fn hls_resources() -> (tempfile::TempDir, HashMap<String, Resource>) {
  gst::init().unwrap();
  let directory = tempfile::tempdir().unwrap();
  let path = directory.path().join("segment.ts");
  let pipeline = gst::parse::launch("videotestsrc num-buffers=60 ! video/x-raw,width=64,height=48,framerate=10/1 ! videoconvert ! x264enc key-int-max=10 tune=zerolatency ! h264parse ! mpegtsmux ! filesink name=output").unwrap().downcast::<gst::Pipeline>().unwrap();
  pipeline
    .by_name("output")
    .unwrap()
    .set_property("location", path.to_str().unwrap());
  pipeline.set_state(gst::State::Playing).unwrap();
  let message = pipeline.bus().unwrap().timed_pop_filtered(
    gst::ClockTime::from_seconds(15),
    &[gst::MessageType::Eos, gst::MessageType::Error],
  );
  pipeline.set_state(gst::State::Null).unwrap();
  assert!(
    matches!(message.unwrap().view(), gst::MessageView::Eos(_)),
    "HLS fixture encoding failed"
  );
  let playlist: Arc<[u8]> = Arc::from(b"#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:6\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:6.0,\nsegment.ts?part=one\n#EXT-X-ENDLIST\n".as_slice());
  let resources = HashMap::from([
    (
      "/hls/index.m3u8".into(),
      Resource {
        body: playlist,
        mime: "application/vnd.apple.mpegurl",
        stall: false,
      },
    ),
    (
      "/hls/segment.ts".into(),
      Resource {
        body: std::fs::read(path).unwrap().into(),
        mime: "video/mp2t",
        stall: false,
      },
    ),
  ]);
  (directory, resources)
}

#[test]
fn hls_manifest_and_segments_share_scoped_headers_and_seek_while_paused() {
  let (_directory, resources) = hls_resources();
  let server = Server::start(resources);
  runtime().block_on(async {
    let (mut player, notifications) = Player::new(AudioOutput::Discard).unwrap();
    let mut notifications = Box::pin(notifications);
    player
      .open(PlaybackSource::Network(server.source("/hls/index.m3u8")))
      .unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      s.phase == PlaybackPhase::Playing && s.can_seek()
    })
    .await;
    player.set_playing(false).unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      s.phase == PlaybackPhase::Paused
    })
    .await;
    player.seek(Duration::from_secs(4)).unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      !s.seeking
        && s
          .position
          .is_some_and(|p| p.abs_diff(Duration::from_secs(4)) < Duration::from_millis(120))
    })
    .await;
    assert_eq!(player.status().phase, PlaybackPhase::Paused);
    assert!(!player.status().playing);
    tests::close(&mut player).await;
  });
  let requests: Vec<_> = server.requests.try_iter().collect();
  assert!(requests.iter().all(|request| request.authorized));
  assert!(requests
    .iter()
    .any(|request| request.path == "/hls/segment.ts"));
}

#[test]
fn closing_during_stalled_network_read_cancels_without_waiting_for_read_timeout() {
  let server = Server::start(HashMap::from([(
    "/stall.mp4".into(),
    Resource {
      body: Arc::from([0u8; 1024]),
      mime: "video/mp4",
      stall: true,
    },
  )]));
  runtime().block_on(async {
    let (mut player, _notifications) = Player::new(AudioOutput::Discard).unwrap();
    player
      .open(PlaybackSource::Network(server.source("/stall.mp4")))
      .unwrap();
    let request = server
      .requests
      .recv_timeout(Duration::from_secs(5))
      .unwrap();
    assert!(request.authorized);
    tokio::time::timeout(Duration::from_secs(3), tests::close(&mut player))
      .await
      .expect("close must cancel the stalled 15-second read");
  });
}

#[test]
fn stalled_network_read_reports_sanitized_timeout_and_can_close() {
  let server = Server::start(HashMap::from([(
    "/stall.mp4".into(),
    Resource {
      body: Arc::from([0u8; 1024]),
      mime: "video/mp4",
      stall: true,
    },
  )]));
  runtime().block_on(async {
    let (mut player, notifications) = Player::new(AudioOutput::Discard).unwrap();
    let mut notifications = Box::pin(notifications);
    let source = server
      .source("/stall.mp4")
      .with_timeouts(NetworkTimeouts {
        read: Duration::from_millis(200),
        ..NetworkTimeouts::default()
      })
      .unwrap();
    player.open(PlaybackSource::Network(source)).unwrap();
    tests::wait_for(&mut player, &mut notifications, |s| {
      s.phase == PlaybackPhase::Error
    })
    .await;
    let diagnostic = format!("{:?}", player.status().error);
    assert!(
      diagnostic.to_ascii_lowercase().contains("time"),
      "{diagnostic}"
    );
    assert!(!diagnostic.contains("network-query-secret"));
    tests::close(&mut player).await;
  });
}
