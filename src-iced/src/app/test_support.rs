//! Shared test doubles for app-surface tests.
//!
//! [`BrowseFixture`] is a controlled local media server: every accepted
//! connection is parked until the test replies, so request ordering and
//! cancellation are exercised without timing the network. Page replies are
//! synthesized from the request's own `startIndex`/`limit` query parameters.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use iced::futures::StreamExt;
use iced::Task;
use iced_runtime::Action;
use jellypilot_core::browse_model::BrowsePageSettlement;
use jellypilot_media_server::{JellyfinClient, MediaServerProvider, SavedSession};
use jellypilot_sdk::browse::PageRequest;

use super::browse::{self, Surface};
use super::kernel::Kernel;
use super::message::{BrowseMessage, Message};
use super::state::State;

/// One accepted HTTP request, parked until the test replies or drops it.
/// Dropping without replying closes the connection, which resolves the
/// client's request as a failure.
pub(crate) struct FixtureRequest {
  target: String,
  start_index: u32,
  limit: u32,
  reply: std::sync::mpsc::Sender<(u16, String)>,
}

impl FixtureRequest {
  /// The raw request target (`/Items?...`), for assertions on paging queries.
  pub(crate) fn target(&self) -> &str {
    &self.target
  }
  pub(crate) fn reply(self, reply: FixtureReply) {
    let (status, body) = match reply {
      FixtureReply::Page { total, artwork } => {
        (200, page_body(self.start_index, self.limit, total, artwork))
      }
      FixtureReply::Failure => (500, "{}".to_owned()),
      FixtureReply::Json(body) => (200, body),
    };
    // A cancelled request may already have dropped the server thread's
    // receiver; the reply is best-effort.
    let _ = self.reply.send((status, body));
  }
}

/// What the fixture sends back for one parked request.
#[derive(Clone)]
pub(crate) enum FixtureReply {
  /// `BaseItemDtoQueryResult` covering the request's own range, with
  /// deterministic item ids (`fixture_item_id`).
  Page {
    total: u32,
    artwork: bool,
  },
  /// HTTP 500 with an empty JSON body.
  Failure,
  Json(String),
}

/// The item id the fixture generates for global index `index`.
pub(crate) fn fixture_item_id(index: u32) -> String {
  format!("{index:032x}")
}

/// A local HTTP server that hands every accepted request to the test.
pub(crate) struct BrowseFixture {
  server_url: String,
  requests: tokio::sync::mpsc::UnboundedReceiver<FixtureRequest>,
  shutdown: Arc<AtomicBool>,
  accept_thread: Option<std::thread::JoinHandle<()>>,
}

impl BrowseFixture {
  pub(crate) fn server_url(&self) -> &str {
    &self.server_url
  }
  pub(crate) fn new() -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind browse fixture");
    let address = listener.local_addr().expect("fixture address");
    let (requests, received) = tokio::sync::mpsc::unbounded_channel();
    let shutdown = Arc::new(AtomicBool::new(false));
    let accept_thread = std::thread::spawn({
      let shutdown = Arc::clone(&shutdown);
      move || {
        for stream in listener.incoming() {
          if shutdown.load(Ordering::Relaxed) {
            break;
          }
          let Ok(stream) = stream else { continue };
          let requests = requests.clone();
          std::thread::spawn(move || serve(stream, requests));
        }
      }
    });
    Self {
      server_url: format!("http://{address}"),
      requests: received,
      shutdown,
      accept_thread: Some(accept_thread),
    }
  }

  /// A media-server client authenticated against this fixture.
  pub(crate) fn client(&self) -> Arc<JellyfinClient> {
    let client = Arc::new(JellyfinClient::new());
    client.login().adopt_validated_session(&SavedSession {
      provider: MediaServerProvider::Jellyfin,
      server_url: self.server_url.clone(),
      access_token: "fixture-token".to_owned(),
      user_id: "fixture-user".to_owned(),
      user_name: "fixture-user".to_owned(),
      server_name: None,
      device_id: None,
    });
    client
  }

  /// Waits for the next request while driving `stream`, so the task's
  /// futures can reach the server. Stream actions are consumed.
  pub(crate) async fn next_request(
    &mut self,
    stream: &mut iced::futures::stream::BoxStream<'static, Action<Message>>,
  ) -> FixtureRequest {
    let mut deadline = std::pin::pin!(tokio::time::sleep(std::time::Duration::from_secs(10)));
    let mut stream_done = false;
    loop {
      tokio::select! {
        () = &mut deadline => panic!("browse task did not issue an HTTP request"),
        action = stream.next(), if !stream_done => stream_done = action.is_none(),
        request = self.requests.recv() => {
          break request.expect("browse fixture server remains available");
        }
      }
    }
  }

  /// Drives the task to completion, replying to each fixture request with
  /// `respond`, and returns the emitted output messages.
  pub(crate) async fn run_task(
    &mut self,
    task: Task<Message>,
    respond: impl FnMut(&FixtureRequest) -> FixtureReply,
  ) -> Vec<Message> {
    let Some(mut stream) = iced_runtime::task::into_stream(task) else {
      return Vec::new();
    };
    self.run_stream(&mut stream, respond).await
  }

  /// Drives an existing task stream to completion; see [`Self::run_task`].
  pub(crate) async fn run_stream(
    &mut self,
    stream: &mut iced::futures::stream::BoxStream<'static, Action<Message>>,
    mut respond: impl FnMut(&FixtureRequest) -> FixtureReply,
  ) -> Vec<Message> {
    let mut deadline = std::pin::pin!(tokio::time::sleep(std::time::Duration::from_secs(10)));
    let mut outputs = Vec::new();
    loop {
      tokio::select! {
        () = &mut deadline => panic!("browse task stream did not settle"),
        action = stream.next() => {
          match action {
            Some(Action::Output(message)) => outputs.push(message),
            Some(_) => {}
            None => break,
          }
        }
        request = self.requests.recv() => {
          match request {
            Some(request) => {
              let reply = respond(&request);
              request.reply(reply);
            }
            None => panic!("browse fixture server stopped before task completion"),
          }
        }
      }
    }
    outputs
  }

  /// Runs one SDK page request to its settlement, replying `reply` to every
  /// request that reaches the fixture.
  pub(crate) async fn run_request(
    &mut self,
    request: PageRequest,
    reply: FixtureReply,
  ) -> BrowsePageSettlement {
    let mut deadline = std::pin::pin!(tokio::time::sleep(std::time::Duration::from_secs(10)));
    let mut run = std::pin::pin!(request.run());
    loop {
      tokio::select! {
        () = &mut deadline => panic!("SDK page request did not settle"),
        settlement = &mut run => break settlement,
        received = self.requests.recv() => {
          match received {
            Some(received) => received.reply(reply.clone()),
            None => panic!("browse fixture server stopped before page settlement"),
          }
        }
      }
    }
  }
}

impl Drop for BrowseFixture {
  fn drop(&mut self) {
    self.shutdown.store(true, Ordering::Relaxed);
    // Wake the blocking accept loop so the thread can observe shutdown.
    let _ = TcpStream::connect(&self.server_url["http://".len()..]);
    if let Some(thread) = self.accept_thread.take() {
      let _ = thread.join();
    }
  }
}

fn serve(mut stream: TcpStream, requests: tokio::sync::mpsc::UnboundedSender<FixtureRequest>) {
  let mut headers = Vec::new();
  let mut byte = [0u8; 1];
  loop {
    match stream.read(&mut byte) {
      Ok(0) | Err(_) => return,
      Ok(_) => {
        headers.push(byte[0]);
        if headers.ends_with(b"\r\n\r\n") {
          break;
        }
        if headers.len() >= 16_384 {
          return;
        }
      }
    }
  }
  let Ok(headers) = std::str::from_utf8(&headers) else {
    return;
  };
  let Some(target) = headers
    .lines()
    .next()
    .and_then(|line| line.split_whitespace().nth(1))
  else {
    return;
  };
  let (reply, response) = std::sync::mpsc::channel();
  let request = FixtureRequest {
    target: target.to_owned(),
    start_index: query_param(target, "startindex").unwrap_or(0),
    limit: query_param(target, "limit").unwrap_or(24),
    reply,
  };
  if requests.send(request).is_err() {
    return;
  }
  let Ok((status, body)) = response.recv() else {
    return;
  };
  let response = format!(
    "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
    body.len(),
    status_text = if status == 200 { "OK" } else { "Error" },
  );
  let _ = stream.write_all(response.as_bytes());
}

fn query_param(target: &str, name: &str) -> Option<u32> {
  let query = target.split_once('?')?.1;
  query.split('&').find_map(|pair| {
    let (key, value) = pair.split_once('=')?;
    (key.eq_ignore_ascii_case(name))
      .then(|| value.parse().ok())
      .flatten()
  })
}

fn page_body(start_index: u32, limit: u32, total: u32, artwork: bool) -> String {
  let end = start_index.saturating_add(limit).min(total);
  let items = (start_index..end)
    .map(|index| {
      let image_tags = if artwork {
        format!(r#","ImageTags":{{"Primary":"tag-{index}"}}"#)
      } else {
        String::new()
      };
      format!(
        r#"{{"Id":"{}","Name":"Item {index}","Type":"Movie"{image_tags}}}"#,
        fixture_item_id(index)
      )
    })
    .collect::<Vec<_>>()
    .join(",");
  format!(r#"{{"Items":[{items}],"TotalRecordCount":{total},"StartIndex":{start_index}}}"#)
}

/// Extracts the page settlement carried by a task output message.
pub(crate) fn page_settled(message: Message) -> Option<BrowsePageSettlement> {
  if let Message::Browse(BrowseMessage::PageSettled(settlement)) = message {
    Some(settlement)
  } else {
    None
  }
}

/// Dispatches a settlement through the browse update boundary and returns the
/// follow-up task for the caller to drain or drop.
pub(crate) fn dispatch_settlement(
  surface: &mut Surface,
  kernel: &mut Kernel,
  window_size: iced::Size,
  settlement: BrowsePageSettlement,
) -> Task<Message> {
  browse::update(
    surface,
    kernel,
    None,
    false,
    window_size,
    BrowseMessage::PageSettled(settlement),
  )
}

/// Drives `task` and every follow-up task produced by dispatching its page
/// settlements through `browse::update`, until no work remains.
pub(crate) async fn drain_task(
  surface: &mut Surface,
  kernel: &mut Kernel,
  fixture: &mut BrowseFixture,
  window_size: iced::Size,
  task: Task<Message>,
  respond: &mut dyn FnMut(&FixtureRequest) -> FixtureReply,
) {
  let mut tasks = vec![task];
  while let Some(task) = tasks.pop() {
    for message in fixture.run_task(task, &mut *respond).await {
      if let Message::Browse(BrowseMessage::PageSettled(settlement)) = message {
        tasks.push(dispatch_settlement(
          surface,
          kernel,
          window_size,
          settlement,
        ));
      }
    }
  }
}

/// State-level variant of [`drain_task`] that dispatches every emitted
/// message through `update::update`.
pub(crate) async fn drain_state_task(
  state: &mut State,
  fixture: &mut BrowseFixture,
  task: Task<Message>,
  respond: &mut dyn FnMut(&FixtureRequest) -> FixtureReply,
) {
  let mut tasks = vec![task];
  while let Some(task) = tasks.pop() {
    for message in fixture.run_task(task, &mut *respond).await {
      tasks.push(super::update::update(state, message));
    }
  }
}
