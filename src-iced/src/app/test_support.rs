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
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_media_server::{JellyfinClient, MediaServerProvider, SavedSession};
use jellypilot_sdk::browse::{BrowsePresentation, BrowseSession, BrowseSnapshot};
use jellypilot_sdk::saved_browse::SavedBrowseDraft;

use super::browse::{self, Surface};
use super::kernel::Kernel;
use super::message::{BrowseMessage, Message};
use super::state::State;

pub(crate) fn activate_sdk_fixture(kernel: &mut Kernel, fixture: &BrowseFixture) {
  kernel.sdk.adopt_test_session(SavedSession {
    provider: MediaServerProvider::Jellyfin,
    server_url: fixture.server_url().into(),
    access_token: "fixture-token".into(),
    user_id: "fixture-user".into(),
    user_name: "Fixture".into(),
    server_name: None,
    device_id: None,
  });
  let profile = kernel.sdk.active_profile().expect("active fixture profile");
  super::accounts::sync_activated(kernel, &profile);
}

pub(crate) async fn open_saved_fixture(
  kernel: &mut Kernel,
  fixture: &mut BrowseFixture,
  draft: SavedBrowseDraft,
) -> Arc<BrowseSession> {
  let library = serde_json::json!({
    "Id": draft.library_id,
    "Name": draft.library_name,
    "CollectionType": match draft.collection_type {
      jellypilot_media_server::VideoLibraryKind::Movies => "movies",
      jellypilot_media_server::VideoLibraryKind::TvShows => "tvshows",
    },
  });
  let filter = kernel
    .sdk
    .saved_browse_save(kernel.sdk.new_operation_token().unwrap(), draft)
    .await
    .expect("save fixture definition");
  let opened = kernel.sdk.open_saved_browser(
    kernel.sdk.new_operation_token().unwrap(),
    filter.id,
    BrowsePresentation::Desktop,
  );
  tokio::pin!(opened);
  let deadline = tokio::time::sleep(std::time::Duration::from_secs(10));
  tokio::pin!(deadline);
  loop {
    tokio::select! {
      () = &mut deadline => panic!("saved query did not resolve"),
      result = &mut opened => return result.expect("fresh saved query"),
      request = fixture.requests.recv() => {
        let request = request.expect("browse fixture remains available");
        if request.target().starts_with("/UserViews?") {
          request.reply(FixtureReply::Json(serde_json::json!({"Items":[library]}).to_string()));
        } else {
          request.reply(FixtureReply::Page { total: 1, artwork: false });
        }
      }
    }
  }
}

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

  /// Waits for an independently executing SDK query to reach the HTTP seam.
  pub(crate) async fn receive_request(&mut self) -> FixtureRequest {
    tokio::time::timeout(std::time::Duration::from_secs(10), self.requests.recv())
      .await
      .expect("SDK query reaches the fixture")
      .expect("browse fixture remains available")
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

  /// Drives real SDK requests until the requested observable snapshot arrives.
  pub(crate) async fn wait_for_snapshot(
    &mut self,
    session: &Arc<BrowseSession>,
    mut ready: impl FnMut(&BrowseSnapshot) -> bool,
    mut respond: impl FnMut(&FixtureRequest) -> FixtureReply,
  ) -> BrowseSnapshot {
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(deadline);
    let mut snapshot = session.snapshot().expect("active query snapshot");
    loop {
      if ready(&snapshot) {
        return snapshot;
      }
      tokio::select! {
        () = &mut deadline => panic!("SDK query did not reach the expected snapshot: {snapshot:?}"),
        next = session.next_snapshot(snapshot.revision) => {
          snapshot = next.expect("active query notification");
        }
        request = self.requests.recv() => {
          let request = request.expect("browse fixture remains available");
          let reply = respond(&request);
          request.reply(reply);
        }
      }
    }
  }

  pub(crate) async fn settle_session(
    &mut self,
    session: &Arc<BrowseSession>,
    respond: impl FnMut(&FixtureRequest) -> FixtureReply,
  ) -> BrowseSnapshot {
    self
      .wait_for_snapshot(
        session,
        |snapshot| {
          !snapshot.refreshing
            && match &snapshot.view {
              LibraryBrowseView::Loading => false,
              LibraryBrowseView::Failed { retry_busy, .. } => !retry_busy,
              LibraryBrowseView::Ready {
                is_fetching_more,
                retry_busy,
                ..
              } => !is_fetching_more && !retry_busy,
              _ => true,
            }
        },
        respond,
      )
      .await
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

/// Drives native tasks and the current SDK query through observable snapshots.
pub(crate) async fn drain_task(
  surface: &mut Surface,
  kernel: &mut Kernel,
  fixture: &mut BrowseFixture,
  window_size: iced::Size,
  task: Task<Message>,
  respond: &mut dyn FnMut(&FixtureRequest) -> FixtureReply,
) {
  let mut tasks = vec![task];
  loop {
    while let Some(task) = tasks.pop() {
      for message in fixture.run_task(task, &mut *respond).await {
        if let Message::Browse(message) = message {
          tasks.push(browse::update(surface, kernel, false, window_size, message));
        }
      }
    }
    let Some(session) = surface.session.clone() else {
      break;
    };
    let snapshot = fixture.settle_session(&session, &mut *respond).await;
    if surface
      .projection
      .as_ref()
      .is_some_and(|current| current.revision == snapshot.revision)
    {
      break;
    }
    tasks.push(browse::update(
      surface,
      kernel,
      false,
      window_size,
      BrowseMessage::SnapshotChanged {
        session_id: session.id(),
        snapshot: Box::new(snapshot),
      },
    ));
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
  loop {
    while let Some(task) = tasks.pop() {
      for message in fixture.run_task(task, &mut *respond).await {
        tasks.push(super::update::update(state, message));
      }
    }
    let Some(session) = state
      .full
      .as_ref()
      .and_then(|full| full.browse.session.clone())
    else {
      break;
    };
    let snapshot = fixture.settle_session(&session, &mut *respond).await;
    if state
      .full
      .as_ref()
      .and_then(|full| full.browse.projection.as_ref())
      .is_some_and(|current| current.revision == snapshot.revision)
    {
      break;
    }
    tasks.push(super::update::update(
      state,
      Message::Browse(BrowseMessage::SnapshotChanged {
        session_id: session.id(),
        snapshot: Box::new(snapshot),
      }),
    ));
  }
}

/// Captures the geometry produced by a real widget layout without retaining its
/// state or borrowing the widget tree. Child bounds remain relative to parents.
#[derive(Debug)]
pub(crate) struct LayoutGeometry {
  bounds: iced::Rectangle,
  children: Vec<LayoutGeometry>,
}

impl LayoutGeometry {
  pub(crate) fn from_tree(tree: &iced::advanced::widget::Tree) -> Self {
    Self {
      bounds: iced::Rectangle::new(iced::Point::ORIGIN + tree.translation, tree.size),
      children: tree.children.iter().map(Self::from_tree).collect(),
    }
  }

  pub(crate) fn move_to(mut self, position: iced::Point) -> Self {
    self.bounds.x = position.x;
    self.bounds.y = position.y;
    self
  }

  pub(crate) fn bounds(&self) -> iced::Rectangle {
    self.bounds
  }
  pub(crate) fn size(&self) -> iced::Size {
    self.bounds.size()
  }
  pub(crate) fn children(&self) -> &[Self] {
    &self.children
  }
  pub(crate) fn layout(&self) -> iced::advanced::Layout {
    iced::advanced::Layout::new(self.size()).move_to(self.bounds.position())
  }

  pub(crate) fn absolute_bounds(&self) -> Vec<iced::Rectangle> {
    fn visit(node: &LayoutGeometry, parent: iced::Vector, result: &mut Vec<iced::Rectangle>) {
      let bounds = node.bounds + parent;
      result.push(bounds);
      for child in &node.children {
        visit(child, iced::Vector::new(bounds.x, bounds.y), result);
      }
    }
    let mut result = Vec::new();
    visit(self, iced::Vector::ZERO, &mut result);
    result
  }
}

pub(crate) fn layout_widget(
  widget: &mut dyn iced::advanced::Widget<Message, iced::Theme, iced::Renderer>,
  tree: &mut iced::advanced::widget::Tree,
  renderer: &iced::Renderer,
  limits: &iced::advanced::layout::Limits,
) -> LayoutGeometry {
  widget.layout(tree, renderer, limits);
  LayoutGeometry::from_tree(tree)
}

/// Process the presentation frame after a layout or widget operation, as the native
/// runtime does before drawing. Returns messages for callers that also advance
/// application state; geometry-only fixtures can discard them.
pub(crate) fn redraw_ui(
  ui: &mut iced_runtime::user_interface::UserInterface<'_, Message, iced::Theme, iced::Renderer>,
  renderer: &mut iced::Renderer,
) -> Vec<Message> {
  let mut bus = iced::advanced::shell::Bus::new();
  ui.update(
    &iced::window::Headless,
    &iced::advanced::shell::Waker::noop(),
    &[iced::Event::Window(iced::window::Event::RedrawRequested(
      iced::time::Instant::now(),
    ))],
    iced::mouse::Cursor::Unavailable,
    renderer,
    &mut bus,
  );
  bus.drain().map(|(message, _)| message).collect()
}
