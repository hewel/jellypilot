use super::*;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

fn runtime() -> tokio::runtime::Runtime {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
}

fn server(responses: Vec<String>) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let url = format!("http://{}", listener.local_addr().unwrap());
  let (send, receive) = mpsc::channel();
  let join = thread::spawn(move || {
    for response in responses {
      let (mut socket, _) = listener.accept().unwrap();
      socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
      let mut request = Vec::new();
      let mut byte = [0];
      while !request.ends_with(b"\r\n\r\n") {
        socket.read_exact(&mut byte).unwrap();
        request.push(byte[0]);
        assert!(request.len() < 32768);
      }
      send.send(String::from_utf8(request).unwrap()).unwrap();
      socket.write_all(response.as_bytes()).unwrap();
    }
  });
  (url, receive, join)
}

fn response(status: &str, headers: &str, body: &str) -> String {
  format!(
    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
    body.len()
  )
}

#[test]
fn range_authentication_and_response_metadata_survive_without_cookies() {
  let body = "\0\0\0\u{18}ftypisom\0\0\0\0";
  let (url, requests, server) = server(vec![response(
    "206 Partial Content",
    "Content-Range: bytes 16-31/32\r\nAccept-Ranges: bytes\r\nSet-Cookie: upstream-secret\r\n",
    body,
  )]);
  let source = NetworkSource::new(format!("{url}/movie?token=query-secret"))
    .unwrap()
    .with_header("Authorization", "Bearer header-secret")
    .unwrap();
  assert!(!format!("{source:?}").contains("secret"));
  let session = NetworkSession::start(source).unwrap();
  runtime().block_on(async {
    let reply = reqwest::Client::new()
      .get(session.uri())
      .header("Range", "bytes=16-31")
      .send()
      .await
      .unwrap();
    assert_eq!(reply.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(reply.headers()[header::CONTENT_RANGE], "bytes 16-31/32");
    assert_eq!(reply.headers()[header::ACCEPT_RANGES], "bytes");
    assert!(!reply.headers().contains_key(header::SET_COOKIE));
    assert_eq!(reply.bytes().await.unwrap(), body.as_bytes());
  });
  let request = requests
    .recv_timeout(Duration::from_secs(5))
    .unwrap()
    .to_ascii_lowercase();
  assert!(request.contains("range: bytes=16-31\r\n"));
  assert!(request.contains("authorization: bearer header-secret\r\n"));
  session.shutdown().unwrap();
  server.join().unwrap();
}

#[test]
fn redirected_hls_resolves_all_children_against_final_url_with_headers() {
  let playlist = "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"Audio\",URI=\"audio.m3u8?child=1\"\n#EXT-X-KEY:METHOD=AES-128,URI=\"key\"\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXT-X-PART:DURATION=1,URI=\"part.ts\"\n#EXTINF:1,\nsegment.ts\n#EXT-X-ENDLIST\n";
  // Segment transport cannot require a container signature: encrypted media is opaque.
  let media = "opaque segment bytes without a container signature";
  let (url, requests, server) = server(vec![
    response("302 Found", "Location: /final/index.m3u8\r\n", ""),
    response("200 OK", "", playlist),
    response("200 OK", "", "#EXTM3U\n#EXT-X-ENDLIST\n"),
    response("200 OK", "", "0123456789abcdef"),
    response("200 OK", "", media),
    response("200 OK", "", media),
    response("200 OK", "", media),
  ]);
  let source = NetworkSource::new(format!("{url}/original?parent=secret"))
    .unwrap()
    .with_header("X-Token", "private")
    .unwrap();
  let session = NetworkSession::start(source).unwrap();
  runtime().block_on(async {
    let client = reqwest::Client::new();
    let rewritten = client
      .get(session.uri())
      .send()
      .await
      .unwrap()
      .text()
      .await
      .unwrap();
    assert!(!rewritten.contains("parent=secret"));
    let mut children = Vec::new();
    for line in rewritten.lines() {
      if let Some((_, value)) = line.split_once("URI=\"") {
        children.push(value.split('"').next().unwrap());
      } else if line.starts_with("http://") {
        children.push(line);
      }
    }
    assert_eq!(children.len(), 5);
    for child in children {
      let reply = client.get(child).send().await.unwrap();
      assert_eq!(reply.status(), StatusCode::OK);
      reply.bytes().await.unwrap();
    }
  });
  let captured: Vec<_> = (0..7)
    .map(|_| requests.recv_timeout(Duration::from_secs(5)).unwrap())
    .collect();
  assert!(captured
    .iter()
    .all(|r| r.to_ascii_lowercase().contains("x-token: private\r\n")));
  for (request, path) in captured[2..].iter().zip([
    "audio.m3u8?child=1",
    "key",
    "init.mp4",
    "part.ts",
    "segment.ts",
  ]) {
    assert!(request.starts_with(&format!("GET /final/{path} HTTP/1.1\r\n")));
  }
  session.shutdown().unwrap();
  server.join().unwrap();
}

#[test]
fn cross_origin_redirect_never_contacts_destination_or_exposes_secrets() {
  let forbidden = TcpListener::bind("127.0.0.1:0").unwrap();
  forbidden.set_nonblocking(true).unwrap();
  let location = format!(
    "Location: http://{}/stolen?secret=value\r\n",
    forbidden.local_addr().unwrap()
  );
  let (url, requests, server) = server(vec![response("302 Found", &location, "")]);
  let session = NetworkSession::start(
    NetworkSource::new(url)
      .unwrap()
      .with_header("Authorization", "private")
      .unwrap(),
  )
  .unwrap();
  runtime().block_on(async {
    assert_eq!(
      reqwest::get(session.uri()).await.unwrap().status(),
      StatusCode::BAD_GATEWAY
    );
  });
  assert_eq!(
    forbidden.accept().unwrap_err().kind(),
    io::ErrorKind::WouldBlock
  );
  let error = session.error().unwrap().to_string();
  assert!(error.contains("Cross-origin"));
  assert!(!error.contains("secret") && !error.contains("private") && !error.contains("stolen"));
  requests.recv_timeout(Duration::from_secs(5)).unwrap();
  session.shutdown().unwrap();
  server.join().unwrap();
}

#[test]
fn unknown_and_cross_origin_playlist_forms_fail_closed() {
  for playlist in [
    "#EXTM3U\n#EXT-X-CONTENT-STEERING:SERVER-URI=\"https://elsewhere.test/steer\"\n",
    "#EXTM3U\n#EXT-X-DEFINE:NAME=\"escape\",VALUE=\"https://elsewhere.test\"\n",
    "#EXTM3U\n#EXT-X-SERVER-CONTROL:CAN-BLOCK-RELOAD=YES,CAN-SKIP-UNTIL=12\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nsegment.ts\n",
    "#EXTM3U\n#EXT-X-PRELOAD-HINT:TYPE=PART,URI=\"https://elsewhere.test/part\"\n",
    "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,URI=https://elsewhere.test/audio\n",
    "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1,FUTURE-URI=\"https://elsewhere.test/a\"\na.ts\n",
    "<?xml version=\"1.0\"?><MPD><BaseURL>https://elsewhere.test/</BaseURL></MPD>",
  ] {
    let (url, requests, server) = server(vec![response(
      "200 OK",
      "Content-Type: video/mp4\r\n",
      playlist,
    )]);
    let session = NetworkSession::start(NetworkSource::new(url).unwrap()).unwrap();
    runtime().block_on(async {
      assert_eq!(
        reqwest::get(session.uri()).await.unwrap().status(),
        StatusCode::BAD_GATEWAY
      );
    });
    assert!(session.error().is_some());
    requests.recv_timeout(Duration::from_secs(5)).unwrap();
    session.shutdown().unwrap();
    server.join().unwrap();
  }
}

#[test]
fn upstream_http_errors_are_actionable_without_response_body_or_url() {
  let (url, requests, server) =
    server(vec![response("401 Unauthorized", "", "private diagnostic")]);
  let session =
    NetworkSession::start(NetworkSource::new(format!("{url}/secret?token=private")).unwrap())
      .unwrap();
  runtime().block_on(async {
    assert_eq!(
      reqwest::get(session.uri()).await.unwrap().status(),
      StatusCode::BAD_GATEWAY
    );
  });
  let error = session.error().unwrap().to_string();
  assert!(error.contains("401"));
  assert!(!error.contains("private") && !error.contains("secret"));
  requests.recv_timeout(Duration::from_secs(5)).unwrap();
  session.shutdown().unwrap();
  server.join().unwrap();
}

#[test]
fn cancellation_interrupts_stalled_upstream_and_removes_listener() {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let url = format!("http://{}/movie", listener.local_addr().unwrap());
  let (accepted, observed) = mpsc::channel();
  let (release, released) = mpsc::channel();
  let upstream = thread::spawn(move || {
    let (socket, _) = listener.accept().unwrap();
    accepted.send(()).unwrap();
    released.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(socket);
  });
  let session = NetworkSession::start(NetworkSource::new(url).unwrap()).unwrap();
  let uri = session.uri().to_owned();
  let client_uri = uri.clone();
  let client = thread::spawn(move || runtime().block_on(reqwest::get(client_uri)));
  observed.recv_timeout(Duration::from_secs(5)).unwrap();
  let started = std::time::Instant::now();
  session.shutdown().unwrap();
  assert!(started.elapsed() < Duration::from_secs(2));
  assert!(client.join().unwrap().is_err());
  runtime().block_on(async {
    assert!(reqwest::get(uri).await.is_err());
  });
  release.send(()).unwrap();
  upstream.join().unwrap();
}

#[test]
fn oversized_playlist_is_rejected_before_decoder_receives_it() {
  let playlist = format!("#EXTM3U\n{}", "# comment\n".repeat(MANIFEST_LIMIT / 9 + 1));
  let (url, requests, server) = server(vec![response("200 OK", "", &playlist)]);
  let session = NetworkSession::start(NetworkSource::new(url).unwrap()).unwrap();
  runtime().block_on(async {
    assert_eq!(
      reqwest::get(session.uri()).await.unwrap().status(),
      StatusCode::BAD_GATEWAY
    );
  });
  assert!(session.error().unwrap().to_string().contains("1 MiB"));
  requests.recv_timeout(Duration::from_secs(5)).unwrap();
  session.shutdown().unwrap();
  server.join().unwrap();
}

#[test]
fn source_rejects_request_smuggling_and_secret_bearing_invalid_urls() {
  for url in [
    "ftp://host/file",
    "https://user:private@host/file",
    "https://host/file#private",
    "https://host/file\r\nprivate",
  ] {
    let error = NetworkSource::new(url).unwrap_err();
    assert!(!error.to_string().contains("private"));
  }
  for name in [
    "Host",
    "Range",
    "Connection",
    "Transfer-Encoding",
    "Proxy-Authorization",
    "Accept-Encoding",
  ] {
    assert!(NetworkSource::new("https://host/file")
      .unwrap()
      .with_header(name, "private")
      .is_err());
  }
  assert!(NetworkSource::new("https://host/file")
    .unwrap()
    .with_header("X-Token", "private\r\nInjected: yes")
    .is_err());
}

#[test]
fn arbitrary_range_offsets_use_bounded_authenticated_format_probe() {
  let data = "arbitrary byte offset without container signature";
  let (url, requests, server) = server(vec![
    response(
      "206 Partial Content",
      "Content-Range: bytes 100-147/1024\r\n",
      data,
    ),
    response(
      "206 Partial Content",
      "Content-Range: bytes 0-15/1024\r\n",
      "\0\0\0\u{18}ftypisom\0\0\0\0",
    ),
  ]);
  let session = NetworkSession::start(
    NetworkSource::new(url)
      .unwrap()
      .with_header("X-Auth", "private")
      .unwrap(),
  )
  .unwrap();
  runtime().block_on(async {
    let result = reqwest::Client::new()
      .get(session.uri())
      .header("Range", "bytes=100-147")
      .send()
      .await
      .unwrap();
    assert_eq!(result.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(result.bytes().await.unwrap(), data.as_bytes());
  });
  let original = requests
    .recv_timeout(Duration::from_secs(5))
    .unwrap()
    .to_ascii_lowercase();
  let probe = requests
    .recv_timeout(Duration::from_secs(5))
    .unwrap()
    .to_ascii_lowercase();
  assert!(original.contains("range: bytes=100-147\r\n"));
  assert!(probe.contains("range: bytes=0-511\r\n") && probe.contains("x-auth: private\r\n"));
  session.shutdown().unwrap();
  server.join().unwrap();
}

#[test]
fn stalled_stream_body_times_out_and_latches_sanitized_error() {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let url = format!("http://{}/private", listener.local_addr().unwrap());
  let (release, released) = mpsc::channel();
  let upstream = thread::spawn(move || {
    let (mut socket, _) = listener.accept().unwrap();
    socket
      .set_read_timeout(Some(Duration::from_secs(5)))
      .unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
      socket.read_exact(&mut byte).unwrap();
      bytes.push(byte[0]);
    }
    socket
      .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4096\r\n\r\n")
      .unwrap();
    let mut prefix = [0; 512];
    prefix[4..8].copy_from_slice(b"ftyp");
    socket.write_all(&prefix).unwrap();
    released.recv_timeout(Duration::from_secs(5)).unwrap();
  });
  let source = NetworkSource::new(url)
    .unwrap()
    .with_timeouts(crate::source::NetworkTimeouts {
      read: Duration::from_millis(150),
      ..Default::default()
    })
    .unwrap();
  let session = NetworkSession::start(source).unwrap();
  runtime().block_on(async {
    let reply = reqwest::get(session.uri()).await.unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    assert!(reply.bytes().await.is_err());
  });
  let error = session.error().unwrap().to_string();
  assert!(error.contains("timeout") && !error.contains("private"));
  session.shutdown().unwrap();
  release.send(()).unwrap();
  upstream.join().unwrap();
}
