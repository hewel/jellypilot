//! Resource-owning runtime for the remote Playback Target (ADR 0029).
//!
//! One [`Runtime`] owns every remote resource: the WebSocket command socket,
//! the forwarded event channel, and the Jellyfin client the target registers
//! against. The playback surface feeds [`Input`]s and drains [`Update`]s;
//! asynchronous work leaves the runtime only as owned [`Work`] items whose
//! [`Completion`]s return through [`Input::Completed`].
//!
//! Resource identity, the remote gate generation, and the registration
//! revision are tracked separately so stale completions can neither restore
//! state nor fail a newer desired target. Physical work is never cancelled:
//! teardown requested while startup or registration is in flight defers the
//! close until that work completes, and every resource is closed exactly once
//! through the same cleanup path.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use jellypilot_core::request_gate::{RemoteToken, RequestGate};
use jellypilot_media_server::JellyfinClient;
use jellypilot_session::{
  finalize_remote_target, CapabilityRegistrationError, JellyfinCommand, JellyfinWebSocket,
  JellyfinWebSocketEvent, RemoteControlState,
};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

/// External party waiting for remote teardown to finish.
///
/// `Disconnect` only requests cleanup; it is never reported back through
/// [`Update::settled`]. `Quit` and `Account` join every tracked resource and
/// settle once the last resource they joined completes its cleanup.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Waiter {
  Disconnect,
  Quit,
  Account(u64),
}

/// Inputs the playback surface feeds into the remote runtime.
pub enum Input {
  /// Adopt a client and desired target name, then open the command socket.
  Start {
    client: Arc<JellyfinClient>,
    name: String,
  },
  /// Latest desired target name; re-registers when the target is live.
  Rename(String),
  /// One event delivered by the subscription for `remote`'s channel.
  Event {
    remote: RemoteToken,
    event: JellyfinWebSocketEvent,
  },
  /// A finished [`Work`] item returning its stamped result.
  Completed(Completion),
  /// Request teardown of the current target and join `waiter` to it.
  Retire(Waiter),
}

/// Everything one input produced: owned work to spawn, typed notices for the
/// surface to translate, accepted raw commands, and settled waiters.
#[derive(Default)]
pub struct Update {
  pub work: Vec<Work>,
  pub notices: Vec<Notice>,
  pub commands: Vec<(RemoteToken, JellyfinCommand)>,
  pub settled: Vec<Waiter>,
}

/// Typed remote lifecycle notices; the playback bridge owns their
/// presentation and diagnostics mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Notice {
  StartFailed(StartError),
  ValidationPending { reconnected: bool },
  ConnectionLost,
  ConnectionRestored,
  RegistrationFailed,
}

/// Why a remote target startup failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
  SessionUnavailable,
  ConnectionFailed,
  CapabilityRegistrationFailed,
}

impl StartError {
  pub const fn diagnostic(self) -> &'static str {
    match self {
      Self::SessionUnavailable => "Remote playback target session is unavailable.",
      Self::ConnectionFailed => "Remote playback target could not connect.",
      Self::CapabilityRegistrationFailed => {
        "Remote playback target capabilities could not be registered."
      }
    }
  }
}

/// Subscription identity for one resource's forwarded event stream. Hashing
/// the stable receiver pointer keeps the subscription alive across view
/// updates and WebSocket reconnects while a replacement resource produces a
/// distinct subscription.
#[derive(Clone)]
pub struct EventChannel {
  pub remote: RemoteToken,
  pub receiver: Arc<tokio::sync::Mutex<UnboundedReceiver<JellyfinWebSocketEvent>>>,
}

impl std::hash::Hash for EventChannel {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    Arc::as_ptr(&self.receiver).hash(state);
  }
}

/// Projected remote state for the playback surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct View {
  pub state: RemoteControlState,
  /// No live resource and no cleanup work remain; quit may proceed.
  pub quiescent: bool,
}

impl Default for View {
  fn default() -> Self {
    Self {
      state: RemoteControlState::Unavailable,
      quiescent: true,
    }
  }
}

/// One owned unit of asynchronous work. The runtime never keeps a handle to
/// in-flight work; its result returns only through [`Input::Completed`].
pub struct Work(WorkInner);

enum WorkInner {
  Startup {
    resource: u64,
    client: Arc<JellyfinClient>,
    websocket: Arc<JellyfinWebSocket>,
    socket_events: Option<tokio::sync::mpsc::Receiver<JellyfinWebSocketEvent>>,
    event_sender: tokio::sync::mpsc::UnboundedSender<JellyfinWebSocketEvent>,
    forwarder: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
    desired: Arc<AtomicU64>,
  },
  Registration {
    resource: u64,
    work: u64,
    reconnected: bool,
    client: Arc<JellyfinClient>,
    desired: Arc<AtomicU64>,
  },
  Cleanup {
    resource: u64,
    work: u64,
    websocket: Arc<JellyfinWebSocket>,
    forwarder: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
  },
}

impl Work {
  /// Runs the work to completion. Startup and registration never close the
  /// resource themselves; cleanup is the single close path.
  pub async fn run(self) -> Completion {
    match self.0 {
      WorkInner::Startup {
        resource,
        client,
        websocket,
        socket_events,
        event_sender,
        forwarder,
        desired,
      } => {
        let connected = startup(&client, websocket, socket_events, event_sender, forwarder).await;
        let revision = desired.load(Ordering::Acquire);
        let result = match connected {
          Ok(()) => finalize_remote_target(&client)
            .await
            .map_err(|_| StartError::CapabilityRegistrationFailed),
          Err(error) => Err(error),
        };
        Completion {
          resource,
          work: 0,
          kind: CompletionKind::Startup { revision, result },
        }
      }
      WorkInner::Registration {
        resource,
        work,
        reconnected,
        client,
        desired,
      } => Completion {
        resource,
        work,
        kind: CompletionKind::Registration {
          revision: desired.load(Ordering::Acquire),
          reconnected,
          result: finalize_remote_target(&client).await,
        },
      },
      WorkInner::Cleanup {
        resource,
        work,
        websocket,
        forwarder,
      } => {
        websocket.disconnect().await;
        if let Some(forwarder) = forwarder.lock().await.take() {
          let _ = forwarder.await;
        }
        Completion {
          resource,
          work,
          kind: CompletionKind::Cleanup,
        }
      }
    }
  }
}

/// Stamped result of a finished [`Work`] item. Carries only data: resources
/// are never re-adopted from a completion.
#[derive(Clone, Debug)]
pub struct Completion {
  resource: u64,
  work: u64,
  kind: CompletionKind,
}

#[derive(Clone, Debug)]
enum CompletionKind {
  Startup {
    revision: u64,
    result: Result<bool, StartError>,
  },
  Registration {
    revision: u64,
    reconnected: bool,
    result: Result<bool, CapabilityRegistrationError>,
  },
  Cleanup,
}

/// Connects the socket and starts forwarding events. Registration follows
/// separately so its revision is captured immediately before the HTTP request.
async fn startup(
  client: &JellyfinClient,
  websocket: Arc<JellyfinWebSocket>,
  socket_events: Option<tokio::sync::mpsc::Receiver<JellyfinWebSocketEvent>>,
  event_sender: tokio::sync::mpsc::UnboundedSender<JellyfinWebSocketEvent>,
  forwarder: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
) -> Result<(), StartError> {
  let Some(mut socket_events) = socket_events else {
    return Err(StartError::SessionUnavailable);
  };
  *forwarder.lock().await = Some(tokio::spawn(async move {
    while let Some(event) = socket_events.recv().await {
      if event_sender.send(event).is_err() {
        break;
      }
    }
  }));
  let websocket_url = client
    .playback()
    .websocket_url()
    .map_err(|_| StartError::SessionUnavailable)?;
  let user_agent = client.playback().websocket_user_agent();
  if websocket
    .connect_with_user_agent(&websocket_url, Some(&user_agent))
    .await
    .is_err()
  {
    return Err(StartError::ConnectionFailed);
  }
  Ok(())
}

/// Lifecycle stage of one tracked resource. A resource runs at most one unit
/// of work at a time: startup, one registration, or the final cleanup.
enum Stage {
  /// Startup work is in flight; the socket is not yet a usable target.
  Starting,
  /// Startup adopted; `registering` marks an in-flight registration.
  Live { registering: bool },
  /// Teardown was requested while startup or registration was in flight;
  /// the deferred cleanup waits for that completion.
  Draining,
  /// Cleanup work is in flight.
  Closing,
}

/// One remote resource: socket, forwarded event channel, client, and the
/// waiters joined to its teardown.
struct Resource {
  id: u64,
  client: Arc<JellyfinClient>,
  websocket: Arc<JellyfinWebSocket>,
  channel: EventChannel,
  forwarder: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
  control: RemoteControlState,
  /// Whether the current Connecting round follows an actual socket reconnect.
  reconnected: bool,
  stage: Stage,
  /// Latest registration revision this resource should reach. Shared with
  /// in-flight work so a completion records the revision its request
  /// actually observed at execution time.
  desired_registration: Arc<AtomicU64>,
  /// Identity of the physical work slot, independent of desired registration.
  work: u64,
  waiters: Vec<Waiter>,
}

/// The remote Playback Target runtime. Owns the current resource and every
/// resource still closing; survives surface initialization and profile
/// handoffs.
pub(crate) struct Runtime {
  /// The newest resource while it is still starting or live. Retired
  /// resources move to `retiring` immediately so the view never presents a
  /// teardown-bound target.
  current: Option<Resource>,
  /// Retired resources whose deferred or in-flight cleanup is not done.
  retiring: Vec<Resource>,
  /// Waiters joined to teardown that have not settled yet.
  open_waiters: HashSet<Waiter>,
  token: RemoteToken,
  next_resource_id: u64,
  registration_revision: u64,
}

impl Runtime {
  pub fn new(gate: &mut RequestGate) -> Self {
    Self {
      current: None,
      retiring: Vec::new(),
      open_waiters: HashSet::new(),
      token: gate.begin_remote(),
      next_resource_id: 0,
      registration_revision: 0,
    }
  }

  pub fn update(&mut self, input: Input, gate: &mut RequestGate) -> Update {
    match input {
      Input::Start { client, name } => self.start(client, name, gate),
      Input::Rename(name) => self.rename(name),
      Input::Event { remote, event } => self.event(remote, event),
      Input::Completed(completion) => self.completed(completion, gate),
      Input::Retire(waiter) => self.retire(waiter, gate),
    }
  }

  pub fn view(&self) -> View {
    View {
      state: self
        .current
        .as_ref()
        .map_or(RemoteControlState::Unavailable, |resource| resource.control),
      quiescent: self.current.is_none() && self.retiring.is_empty(),
    }
  }

  pub fn token(&self) -> RemoteToken {
    self.token
  }

  /// The current resource's event channel, or `None` once teardown starts.
  /// The channel is created with the resource and survives socket reconnects.
  pub fn events(&self) -> Option<EventChannel> {
    self
      .current
      .as_ref()
      .map(|resource| resource.channel.clone())
  }

  fn start(&mut self, client: Arc<JellyfinClient>, name: String, gate: &mut RequestGate) -> Update {
    let mut update = Update::default();
    if self.current.is_some() {
      self.retire_current(&mut update, gate);
    }
    client.set_device_name(name);
    self.token = gate.begin_remote();
    self.next_resource_id = self.next_resource_id.wrapping_add(1);
    let id = self.next_resource_id;
    let websocket = Arc::new(JellyfinWebSocket::new());
    let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();
    let resource = Resource {
      id,
      client: Arc::clone(&client),
      websocket: Arc::clone(&websocket),
      channel: EventChannel {
        remote: self.token,
        receiver: Arc::new(tokio::sync::Mutex::new(event_receiver)),
      },
      forwarder: Arc::new(tokio::sync::Mutex::new(None)),
      control: RemoteControlState::Connecting,
      reconnected: false,
      stage: Stage::Starting,
      desired_registration: Arc::new(AtomicU64::new(self.registration_revision)),
      work: 0,
      waiters: Vec::new(),
    };
    update.work.push(Work(WorkInner::Startup {
      resource: id,
      client,
      websocket: Arc::clone(&websocket),
      socket_events: websocket.take_event_receiver(),
      event_sender,
      forwarder: Arc::clone(&resource.forwarder),
      desired: Arc::clone(&resource.desired_registration),
    }));
    self.current = Some(resource);
    update
  }
  fn rename(&mut self, name: String) -> Update {
    let mut update = Update::default();
    let Some(resource) = self.current.as_mut() else {
      return update;
    };
    resource.client.set_device_name(name);
    // Every rename advances the desired registration, including while
    // connecting: the startup request may already have fired with an older
    // name, so convergence is checked when its completion lands. At most one
    // serial follow-up registration runs per burst.
    bump_desired(resource, &mut self.registration_revision);
    if matches!(resource.stage, Stage::Live { registering: false })
      && resource.control == RemoteControlState::Available
    {
      emit_registration(&mut update, resource);
    }
    update
  }

  fn event(&mut self, remote: RemoteToken, event: JellyfinWebSocketEvent) -> Update {
    let mut update = Update::default();
    if remote != self.token {
      return update;
    }
    let Some(resource) = self.current.as_mut() else {
      return update;
    };
    match event {
      JellyfinWebSocketEvent::Command(command) => {
        if resource.control == RemoteControlState::Available {
          update.commands.push((remote, command));
        }
      }
      JellyfinWebSocketEvent::ConnectionLost => {
        resource.control = RemoteControlState::Lost;
        bump_desired(resource, &mut self.registration_revision);
        update.notices.push(Notice::ConnectionLost);
      }
      JellyfinWebSocketEvent::Reconnected => {
        resource.control = RemoteControlState::Connecting;
        resource.reconnected = true;
        bump_desired(resource, &mut self.registration_revision);
        if matches!(resource.stage, Stage::Live { registering: false }) {
          emit_registration(&mut update, resource);
        }
      }
      JellyfinWebSocketEvent::Connected => {}
    }
    update
  }

  fn retire(&mut self, waiter: Waiter, gate: &mut RequestGate) -> Update {
    let mut update = Update::default();
    if self.current.is_none() && self.retiring.is_empty() {
      if waiter != Waiter::Disconnect {
        update.settled.push(waiter);
      }
      return update;
    }
    if waiter != Waiter::Disconnect {
      self.open_waiters.insert(waiter);
      self.join_waiter(waiter);
    }
    self.retire_current(&mut update, gate);
    update
  }

  /// Moves the current resource into `retiring`. Cleanup runs immediately
  /// unless startup or registration is still in flight; that work is never
  /// cancelled, so the close defers to its completion.
  fn retire_current(&mut self, update: &mut Update, gate: &mut RequestGate) {
    let Some(mut resource) = self.current.take() else {
      return;
    };
    self.token = gate.begin_remote();
    match resource.stage {
      Stage::Live { registering: false } => {
        begin_cleanup(update, &mut resource, &self.open_waiters);
      }
      // `current` never holds Closing; anything else still has work in
      // flight whose completion releases the deferred cleanup.
      _ => resource.stage = Stage::Draining,
    }
    self.retiring.push(resource);
  }

  fn join_waiter(&mut self, waiter: Waiter) {
    if let Some(resource) = self.current.as_mut() {
      if !resource.waiters.contains(&waiter) {
        resource.waiters.push(waiter);
      }
    }
    for resource in &mut self.retiring {
      if !resource.waiters.contains(&waiter) {
        resource.waiters.push(waiter);
      }
    }
  }

  fn completed(&mut self, completion: Completion, gate: &mut RequestGate) -> Update {
    let resource = self
      .current
      .as_ref()
      .filter(|resource| resource.id == completion.resource)
      .or_else(|| {
        self
          .retiring
          .iter()
          .find(|resource| resource.id == completion.resource)
      });
    if !resource.is_some_and(|resource| resource.work == completion.work) {
      return Update::default();
    }
    match completion.kind {
      CompletionKind::Startup { revision, result } => {
        self.startup_completed(completion.resource, revision, result, gate)
      }
      CompletionKind::Registration {
        revision,
        reconnected,
        result,
      } => self.registration_completed(completion.resource, revision, reconnected, result, gate),
      CompletionKind::Cleanup => self.cleanup_completed(completion.resource),
    }
  }

  fn startup_completed(
    &mut self,
    id: u64,
    revision: u64,
    result: Result<bool, StartError>,
    gate: &mut RequestGate,
  ) -> Update {
    let mut update = Update::default();
    if self
      .current
      .as_ref()
      .is_some_and(|resource| resource.id == id)
    {
      let resource = self.current.as_mut().expect("current checked above");
      if matches!(resource.stage, Stage::Starting) {
        if matches!(
          result,
          Ok(_) | Err(StartError::CapabilityRegistrationFailed)
        ) && (revision != resource.desired_registration.load(Ordering::Acquire)
          || resource.control == RemoteControlState::Lost)
        {
          resource.stage = Stage::Live { registering: false };
          if resource.control != RemoteControlState::Lost {
            emit_registration(&mut update, resource);
          }
          return update;
        }
        match result {
          Ok(validated) => {
            resource.stage = Stage::Live { registering: false };
            resource.control = RemoteControlState::Available;
            let reconnected = std::mem::take(&mut resource.reconnected);
            if reconnected && validated {
              update.notices.push(Notice::ConnectionRestored);
            }
            if !validated {
              update
                .notices
                .push(Notice::ValidationPending { reconnected });
            }
            return update;
          }
          Err(error) => {
            update.notices.push(Notice::StartFailed(error));
            let mut resource = self.current.take().expect("current checked above");
            self.token = gate.begin_remote();
            begin_cleanup(&mut update, &mut resource, &self.open_waiters);
            self.retiring.push(resource);
            return update;
          }
        }
      }
      return update;
    }
    if let Some(position) = self.retiring.iter().position(|resource| resource.id == id) {
      let resource = &mut self.retiring[position];
      if matches!(resource.stage, Stage::Draining) {
        let mut resource = self.retiring.remove(position);
        begin_cleanup(&mut update, &mut resource, &self.open_waiters);
        self.retiring.push(resource);
      }
    }
    update
  }

  fn registration_completed(
    &mut self,
    id: u64,
    revision: u64,
    reconnected: bool,
    result: Result<bool, CapabilityRegistrationError>,
    gate: &mut RequestGate,
  ) -> Update {
    let mut update = Update::default();
    if self
      .current
      .as_ref()
      .is_some_and(|resource| resource.id == id)
    {
      let resource = self.current.as_mut().expect("current checked above");
      if matches!(resource.stage, Stage::Live { registering: true }) {
        resource.stage = Stage::Live { registering: false };
        if revision != resource.desired_registration.load(Ordering::Acquire)
          || resource.control == RemoteControlState::Lost
        {
          if resource.control != RemoteControlState::Lost {
            emit_registration(&mut update, resource);
          }
          return update;
        }
        match result {
          Ok(validated) => {
            if resource.control == RemoteControlState::Connecting {
              resource.control = RemoteControlState::Available;
              resource.reconnected = false;
              if reconnected && validated {
                update.notices.push(Notice::ConnectionRestored);
              }
            }
            if !validated {
              update
                .notices
                .push(Notice::ValidationPending { reconnected });
            }
          }
          Err(_) => {
            update.notices.push(Notice::RegistrationFailed);
            let mut resource = self.current.take().expect("current checked above");
            self.token = gate.begin_remote();
            begin_cleanup(&mut update, &mut resource, &self.open_waiters);
            self.retiring.push(resource);
          }
        }
      }
      return update;
    }
    if let Some(position) = self.retiring.iter().position(|resource| resource.id == id) {
      let resource = &mut self.retiring[position];
      if matches!(resource.stage, Stage::Draining) {
        let mut resource = self.retiring.remove(position);
        begin_cleanup(&mut update, &mut resource, &self.open_waiters);
        self.retiring.push(resource);
      }
    }
    update
  }

  /// Cleanup completions key on resource identity, never on the current
  /// remote token: a cleanup issued before a newer lifecycle still counts.
  fn cleanup_completed(&mut self, id: u64) -> Update {
    let mut update = Update::default();
    let Some(position) = self.retiring.iter().position(|resource| resource.id == id) else {
      return update;
    };
    let resource = self.retiring.remove(position);
    for waiter in resource.waiters {
      // Settle only when no other tracked resource still carries this
      // waiter; it stays in `open_waiters` until its last cleanup lands.
      if !self.still_waiting(waiter) && self.open_waiters.remove(&waiter) {
        update.settled.push(waiter);
      }
    }
    update
  }

  fn still_waiting(&self, waiter: Waiter) -> bool {
    self
      .current
      .as_ref()
      .is_some_and(|resource| resource.waiters.contains(&waiter))
      || self
        .retiring
        .iter()
        .any(|resource| resource.waiters.contains(&waiter))
  }
}
/// Records that the newest desired registration is one revision ahead of
/// whatever this resource has applied.
fn bump_desired(resource: &mut Resource, revision: &mut u64) {
  *revision = revision.wrapping_add(1);
  resource
    .desired_registration
    .store(*revision, Ordering::Release);
}

/// Emits the next serial registration for a live resource. The request reads
/// the latest desired name from the shared client when it executes, and the
/// completion stamps the desired revision observed at that moment.
fn emit_registration(update: &mut Update, resource: &mut Resource) {
  resource.stage = Stage::Live { registering: true };
  resource.work = resource.work.wrapping_add(1);
  update.work.push(Work(WorkInner::Registration {
    resource: resource.id,
    work: resource.work,
    reconnected: resource.reconnected,
    client: Arc::clone(&resource.client),
    desired: Arc::clone(&resource.desired_registration),
  }));
}

/// Transitions a resource into its single closing pass. Waiters that joined
/// after this resource started closing still attach here so each settles at
/// true quiescence.
fn begin_cleanup(update: &mut Update, resource: &mut Resource, open_waiters: &HashSet<Waiter>) {
  for waiter in open_waiters {
    if !resource.waiters.contains(waiter) {
      resource.waiters.push(*waiter);
    }
  }
  resource.stage = Stage::Closing;
  resource.work = resource.work.wrapping_add(1);
  update.work.push(Work(WorkInner::Cleanup {
    resource: resource.id,
    work: resource.work,
    websocket: Arc::clone(&resource.websocket),
    forwarder: Arc::clone(&resource.forwarder),
  }));
}

#[cfg(test)]
mod tests {
  use std::io::{Read, Write};
  use std::net::{TcpListener, TcpStream};
  use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
  use std::sync::Mutex as StdMutex;
  use std::time::Duration;

  use jellypilot_media_server::{Credentials, MediaServerProvider};
  use jellypilot_session::{JellyfinCommand, PlaystateRequest};
  use tokio_tungstenite::tungstenite;

  use super::*;

  const AUTH_BODY: &str = r#"{"User":{"Id":"00000000-0000-0000-0000-000000000001","Name":"Ada"},"AccessToken":"token-1","ServerId":"server-1"}"#;
  const INFO_BODY: &str = r#"{"ServerName":"Fake","Version":"10.10.0","Id":"server-1"}"#;

  struct CapturedRequest {
    headline: String,
    authorization: Option<String>,
  }

  struct ServerControl {
    device_id: Option<String>,
    capabilities_status: &'static str,
    session_visible: bool,
    ws_writer: Option<tungstenite::WebSocket<TcpStream>>,
  }

  struct FakeServer {
    url: String,
    requests: tokio::sync::mpsc::UnboundedReceiver<CapturedRequest>,
    control: Arc<StdMutex<ServerControl>>,
    capabilities_gate: Arc<tokio::sync::Mutex<()>>,
    upgrade_gate: Arc<tokio::sync::Mutex<()>>,
    ws_sessions: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
  }

  impl FakeServer {
    fn start() -> Self {
      let listener = TcpListener::bind("127.0.0.1:0").expect("fake server binds");
      listener
        .set_nonblocking(true)
        .expect("listener nonblocking");
      let url = format!("http://{}", listener.local_addr().expect("address"));
      let (requests, request_rx) = tokio::sync::mpsc::unbounded_channel();
      let control = Arc::new(StdMutex::new(ServerControl {
        device_id: None,
        capabilities_status: "200 OK",
        session_visible: true,
        ws_writer: None,
      }));
      let capabilities_gate = Arc::new(tokio::sync::Mutex::new(()));
      let upgrade_gate = Arc::new(tokio::sync::Mutex::new(()));
      let ws_sessions = Arc::new(AtomicUsize::new(0));
      let shutdown = Arc::new(AtomicBool::new(false));
      let thread = {
        let control = Arc::clone(&control);
        let capabilities_gate = Arc::clone(&capabilities_gate);
        let upgrade_gate = Arc::clone(&upgrade_gate);
        let ws_sessions = Arc::clone(&ws_sessions);
        let shutdown = Arc::clone(&shutdown);
        std::thread::spawn(move || {
          while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
              Ok((stream, _)) => {
                let control = Arc::clone(&control);
                let capabilities_gate = Arc::clone(&capabilities_gate);
                let upgrade_gate = Arc::clone(&upgrade_gate);
                let ws_sessions = Arc::clone(&ws_sessions);
                let requests = requests.clone();
                std::thread::spawn(move || {
                  handle_connection(
                    stream,
                    control,
                    capabilities_gate,
                    upgrade_gate,
                    ws_sessions,
                    requests,
                  );
                });
              }
              Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2));
              }
              Err(_) => break,
            }
          }
        })
      };
      Self {
        url,
        requests: request_rx,
        control,
        capabilities_gate,
        upgrade_gate,
        ws_sessions,
        shutdown,
        thread: Some(thread),
      }
    }

    async fn next_request(&mut self) -> CapturedRequest {
      tokio::time::timeout(Duration::from_secs(10), self.requests.recv())
        .await
        .expect("request captured")
        .expect("request channel open")
    }

    fn send_text(&self, value: serde_json::Value) {
      let mut control = self
        .control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let writer = control.ws_writer.as_mut().expect("websocket connected");
      writer
        .send(tungstenite::Message::Text(value.to_string().into()))
        .expect("server message sends");
    }

    fn close_socket(&self) {
      let mut control = self
        .control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      if let Some(writer) = control.ws_writer.as_mut() {
        let _ = writer.close(None);
      }
    }

    fn set_capabilities_status(&self, status: &'static str) {
      self
        .control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .capabilities_status = status;
    }

    fn set_session_visible(&self, visible: bool) {
      self
        .control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .session_visible = visible;
    }

    fn ws_session_count(&self) -> usize {
      self.ws_sessions.load(Ordering::Relaxed)
    }

    fn stop(self) {}
  }

  impl Drop for FakeServer {
    fn drop(&mut self) {
      self.shutdown.store(true, Ordering::Relaxed);
      if let Some(thread) = self.thread.take() {
        let _ = thread.join();
      }
    }
  }

  fn handle_connection(
    stream: TcpStream,
    control: Arc<StdMutex<ServerControl>>,
    capabilities_gate: Arc<tokio::sync::Mutex<()>>,
    upgrade_gate: Arc<tokio::sync::Mutex<()>>,
    ws_sessions: Arc<AtomicUsize>,
    requests: tokio::sync::mpsc::UnboundedSender<CapturedRequest>,
  ) {
    stream
      .set_read_timeout(Some(Duration::from_secs(10)))
      .expect("read timeout");
    let mut buffer = [0_u8; 1024];
    let prefix = loop {
      match stream.peek(&mut buffer) {
        Ok(0) => return,
        Ok(count) => {
          let prefix = buffer[..count].to_vec();
          if prefix.len() >= 11 || prefix.windows(4).any(|w| w == b"\r\n\r\n") {
            break prefix;
          }
        }
        Err(_) => return,
      }
    };
    if prefix.starts_with(b"GET /socket") {
      serve_websocket(stream, control, upgrade_gate, ws_sessions);
    } else {
      serve_http(stream, control, capabilities_gate, requests);
    }
  }

  fn read_http_request(stream: &mut TcpStream) -> Option<(String, usize)> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let head_end = loop {
      match stream.read(&mut buffer) {
        Ok(0) => return None,
        Ok(count) => {
          bytes.extend_from_slice(&buffer[..count]);
          if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
          }
        }
        Err(_) => return None,
      }
    };
    let head = String::from_utf8_lossy(&bytes[..head_end]).into_owned();
    let content_length = head
      .lines()
      .find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name
          .eq_ignore_ascii_case("content-length")
          .then(|| value.trim().parse::<usize>().ok())
          .flatten()
      })
      .unwrap_or(0);
    while bytes.len() < head_end + content_length {
      match stream.read(&mut buffer) {
        Ok(0) => break,
        Ok(count) => bytes.extend_from_slice(&buffer[..count]),
        Err(_) => break,
      }
    }
    Some((head, head_end + content_length))
  }

  fn serve_http(
    mut stream: TcpStream,
    control: Arc<StdMutex<ServerControl>>,
    capabilities_gate: Arc<tokio::sync::Mutex<()>>,
    requests: tokio::sync::mpsc::UnboundedSender<CapturedRequest>,
  ) {
    let Some((head, _)) = read_http_request(&mut stream) else {
      return;
    };
    let headline = head.lines().next().unwrap_or_default().to_owned();
    let authorization = head.lines().find_map(|line| {
      let (name, value) = line.split_once(':')?;
      name
        .eq_ignore_ascii_case("authorization")
        .then(|| value.trim().to_owned())
    });
    let _ = requests.send(CapturedRequest {
      headline: headline.clone(),
      authorization,
    });
    let path = headline
      .split_whitespace()
      .nth(1)
      .unwrap_or_default()
      .to_owned();
    let (status, body) = route(&path, &control, &capabilities_gate);
    let response = format!(
      "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
      body.len()
    );
    let _ = stream.write_all(response.as_bytes());
  }

  fn route(
    path: &str,
    control: &Arc<StdMutex<ServerControl>>,
    capabilities_gate: &Arc<tokio::sync::Mutex<()>>,
  ) -> (&'static str, String) {
    if path.starts_with("/Users/AuthenticateByName") {
      return ("200 OK", AUTH_BODY.to_owned());
    }
    if path.starts_with("/System/Info") {
      return ("200 OK", INFO_BODY.to_owned());
    }
    if path.starts_with("/Sessions/Capabilities") {
      // The gate blocks the response, not the request: the captured request
      // already carries the device name the client sent.
      let _gate = capabilities_gate.blocking_lock();
      let status = control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .capabilities_status;
      return (status, String::new());
    }
    if path == "/Sessions" || path.starts_with("/Sessions?") {
      let control = control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let body = match (&control.device_id, control.session_visible) {
        (Some(device_id), true) => format!(
          r#"[{{"DeviceId":"{device_id}","SupportsMediaControl":true,"SupportsRemoteControl":true}}]"#
        ),
        _ => "[]".to_owned(),
      };
      return ("200 OK", body);
    }
    ("404 Not Found", "{}".to_owned())
  }

  struct CaptureDeviceId<'a>(&'a mut Option<String>);

  impl tungstenite::handshake::server::Callback for CaptureDeviceId<'_> {
    fn on_request(
      self,
      request: &tungstenite::handshake::server::Request,
      response: tungstenite::handshake::server::Response,
    ) -> Result<
      tungstenite::handshake::server::Response,
      tungstenite::handshake::server::ErrorResponse,
    > {
      *self.0 = request
        .uri()
        .query()
        .unwrap_or_default()
        .split('&')
        .find_map(|pair| pair.strip_prefix("deviceId="))
        .map(str::to_owned);
      Ok(response)
    }
  }

  fn serve_websocket(
    stream: TcpStream,
    control: Arc<StdMutex<ServerControl>>,
    upgrade_gate: Arc<tokio::sync::Mutex<()>>,
    ws_sessions: Arc<AtomicUsize>,
  ) {
    let _gate = upgrade_gate.blocking_lock();
    let writer_stream = match stream.try_clone() {
      Ok(clone) => clone,
      Err(_) => return,
    };
    let mut device_id = None;
    let mut socket = match tungstenite::accept_hdr(stream, CaptureDeviceId(&mut device_id)) {
      Ok(socket) => socket,
      Err(_) => return,
    };
    if let Some(id) = device_id {
      control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .device_id = Some(id);
    }
    {
      let writer = tungstenite::WebSocket::from_raw_socket(
        writer_stream,
        tungstenite::protocol::Role::Server,
        None,
      );
      control
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .ws_writer = Some(writer);
    }
    loop {
      match socket.read() {
        Ok(tungstenite::Message::Text(text)) => {
          if text.contains("SessionsStart") {
            ws_sessions.fetch_add(1, Ordering::Relaxed);
          }
        }
        Ok(tungstenite::Message::Close(_)) | Err(_) => break,
        Ok(_) => {}
      }
    }
    control
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .ws_writer = None;
  }

  async fn authenticated_client(server_url: &str) -> Arc<JellyfinClient> {
    let client = JellyfinClient::new();
    client
      .login()
      .authenticate(&Credentials {
        provider: MediaServerProvider::Jellyfin,
        server_url: server_url.to_owned(),
        username: "Ada".to_owned(),
        password: "correct horse battery staple".to_owned(),
      })
      .await
      .expect("authentication against the fake server succeeds");
    Arc::new(client)
  }

  struct Harness {
    runtime: Runtime,
    gate: RequestGate,
    completions: tokio::sync::mpsc::UnboundedReceiver<Completion>,
    sender: tokio::sync::mpsc::UnboundedSender<Completion>,
  }

  impl Harness {
    fn new() -> Self {
      let mut gate = RequestGate::default();
      let runtime = Runtime::new(&mut gate);
      let (sender, completions) = tokio::sync::mpsc::unbounded_channel();
      Self {
        runtime,
        gate,
        completions,
        sender,
      }
    }

    fn drive(&mut self, input: Input) -> Update {
      let mut update = self.runtime.update(input, &mut self.gate);
      for work in std::mem::take(&mut update.work) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
          let _ = sender.send(work.run().await);
        });
      }
      update
    }

    async fn next_completion(&mut self) -> Completion {
      tokio::time::timeout(Duration::from_secs(10), self.completions.recv())
        .await
        .expect("work completes")
        .expect("completion channel open")
    }

    async fn next_event(&mut self) -> JellyfinWebSocketEvent {
      let channel = self.runtime.events().expect("event channel");
      let event = tokio::time::timeout(
        Duration::from_secs(10),
        channel.receiver.lock().await.recv(),
      )
      .await
      .expect("event arrives")
      .expect("channel open");
      event
    }

    async fn finish_startup(&mut self) -> Update {
      let completion = self.next_completion().await;
      assert!(
        matches!(
          completion.kind,
          CompletionKind::Startup { result: Ok(_), .. }
        ),
        "startup should succeed"
      );
      self.drive(Input::Completed(completion))
    }
  }

  fn playstate(command: &str) -> JellyfinWebSocketEvent {
    JellyfinWebSocketEvent::Command(JellyfinCommand::Playstate(PlaystateRequest {
      command: command.to_owned(),
      seek_position_ticks: None,
    }))
  }

  #[tokio::test]
  async fn start_connects_registers_and_accepts_commands() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    assert_eq!(harness.runtime.view().state, RemoteControlState::Connecting);
    assert!(!harness.runtime.view().quiescent);
    assert!(harness.runtime.events().is_some());

    let connected = harness.next_event().await;
    assert!(matches!(connected, JellyfinWebSocketEvent::Connected));
    let update = harness.finish_startup().await;
    assert!(update.notices.is_empty());
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);

    let capabilities = server.next_request().await;
    assert!(capabilities
      .headline
      .starts_with("POST /Sessions/Capabilities"));
    let authorization = capabilities.authorization.expect("authorization header");
    assert!(authorization.contains(r#"Device="Living Room""#));
    let _validation = server.next_request().await;

    server.send_text(serde_json::json!({
      "MessageType": "Playstate",
      "Data": {"Command": "Pause"}
    }));
    let event = harness.next_event().await;
    let remote = harness.runtime.token();
    let update = harness.drive(Input::Event { remote, event });
    assert_eq!(update.commands.len(), 1);
    assert!(matches!(
      update.commands[0].1,
      JellyfinCommand::Playstate(_)
    ));

    let update = harness.drive(Input::Retire(Waiter::Disconnect));
    assert!(update.settled.is_empty());
    assert_eq!(
      harness.runtime.view().state,
      RemoteControlState::Unavailable
    );
    assert!(!harness.runtime.view().quiescent);
    let completion = harness.next_completion().await;
    assert!(matches!(completion.kind, CompletionKind::Cleanup));
    let update = harness.drive(Input::Completed(completion));
    assert!(update.settled.is_empty());
    assert!(harness.runtime.view().quiescent);
    server.stop();
  }

  #[tokio::test]
  async fn validation_failure_stays_available_with_soft_notice() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    server.set_session_visible(false);
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let _connected = harness.next_event().await;
    let update = harness.finish_startup().await;
    assert_eq!(
      update.notices,
      vec![Notice::ValidationPending { reconnected: false }]
    );
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);
    server.stop();
  }

  #[tokio::test]
  async fn rename_while_connecting_is_coalesced_into_startup_registration() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let update = harness.drive(Input::Rename("Bedroom".to_owned()));
    assert!(update.notices.is_empty());

    let _connected = harness.next_event().await;
    let update = harness.finish_startup().await;
    assert!(update.notices.is_empty());
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);

    let capabilities = server.next_request().await;
    let authorization = capabilities.authorization.expect("authorization header");
    assert!(authorization.contains(r#"Device="Bedroom""#));
    let _validation = server.next_request().await;
    server.stop();
  }

  #[tokio::test]
  async fn rename_during_startup_request_requires_latest_registration() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let gate = Arc::clone(&server.capabilities_gate);
    let gate = gate.lock().await;
    let mut harness = Harness::new();
    harness.drive(Input::Start {
      client,
      name: "Original".to_owned(),
    });
    let original = server.next_request().await;
    assert!(original
      .authorization
      .unwrap()
      .contains(r#"Device="Original""#));
    harness.drive(Input::Rename("Intermediate".to_owned()));
    harness.drive(Input::Rename("Latest".to_owned()));
    server.set_session_visible(false);
    drop(gate);
    harness.finish_startup().await;
    assert_eq!(harness.runtime.view().state, RemoteControlState::Connecting);
    let _validation = server.next_request().await;
    let latest = server.next_request().await;
    assert!(latest.authorization.unwrap().contains(r#"Device="Latest""#));
    let completion = harness.next_completion().await;
    let update = harness.drive(Input::Completed(completion));
    assert_eq!(
      update.notices,
      vec![Notice::ValidationPending { reconnected: false }]
    );
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);
  }

  #[tokio::test]
  async fn live_renames_re_register_serially_with_the_latest_name() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let _connected = harness.next_event().await;
    harness.finish_startup().await;
    let _capabilities = server.next_request().await;
    let _validation = server.next_request().await;

    let gate = Arc::clone(&server.capabilities_gate);
    let gate = gate.lock().await;
    let update = harness.drive(Input::Rename("Bedroom".to_owned()));
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);
    drop(update);
    let first = server.next_request().await;
    assert!(first
      .authorization
      .expect("authorization header")
      .contains(r#"Device="Bedroom""#));

    // A second rename while the first registration is in flight coalesces:
    // no additional work is emitted until the first completes.
    let update = harness.drive(Input::Rename("Kitchen".to_owned()));
    assert!(update.notices.is_empty());
    drop(gate);
    let completion = harness.next_completion().await;
    assert!(matches!(
      completion.kind,
      CompletionKind::Registration { .. }
    ));
    let update = harness.drive(Input::Completed(completion.clone()));
    assert!(update.notices.is_empty());
    let replay = harness
      .runtime
      .update(Input::Completed(completion), &mut harness.gate);
    assert!(
      replay.work.is_empty(),
      "an old completion cannot release the next physical work slot"
    );

    // The first registration's session validation lands before the
    // coalesced follow-up registration.
    let first_validation = server.next_request().await;
    assert!(first_validation.headline.starts_with("GET /Sessions"));
    let second = server.next_request().await;
    assert!(second.headline.starts_with("POST /Sessions/Capabilities"));
    assert!(second
      .authorization
      .expect("authorization header")
      .contains(r#"Device="Kitchen""#));
    let _validation = server.next_request().await;
    let completion = harness.next_completion().await;
    let update = harness.drive(Input::Completed(completion));
    assert!(update.notices.is_empty());
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);
    server.stop();
  }

  #[tokio::test]
  async fn obsolete_registration_failure_cannot_fail_the_latest_name() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();
    harness.drive(Input::Start {
      client,
      name: "Original".to_owned(),
    });
    harness.finish_startup().await;
    let _capabilities = server.next_request().await;
    let _validation = server.next_request().await;
    server.set_capabilities_status("500 Internal Server Error");
    harness.drive(Input::Rename("Obsolete".to_owned()));
    let _request = server.next_request().await;
    let failure = harness.next_completion().await;
    harness.drive(Input::Rename("Latest".to_owned()));
    server.set_capabilities_status("200 OK");
    let update = harness.drive(Input::Completed(failure));
    assert!(update.notices.is_empty());
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);
    let latest = server.next_request().await;
    assert!(latest.authorization.unwrap().contains(r#"Device="Latest""#));
    let completion = harness.next_completion().await;
    harness.drive(Input::Completed(completion));
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);
  }

  #[tokio::test]
  async fn reconnect_reregisters_and_rejects_stale_registration_results() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let _connected = harness.next_event().await;
    harness.finish_startup().await;
    let _capabilities = server.next_request().await;
    let _validation = server.next_request().await;
    let channel = harness.runtime.events().expect("event channel");
    let receiver = Arc::as_ptr(&channel.receiver);

    // A rename registration in flight when the socket drops must not mark
    // the reconnected session available: its result belongs to the old
    // socket session.
    let gate = Arc::clone(&server.capabilities_gate);
    let gate = gate.lock().await;
    harness.drive(Input::Rename("Bedroom".to_owned()));
    let _in_flight = server.next_request().await;
    server.close_socket();
    let lost = harness.next_event().await;
    let remote = harness.runtime.token();
    let update = harness.drive(Input::Event {
      remote,
      event: lost,
    });
    assert_eq!(update.notices, vec![Notice::ConnectionLost]);
    assert_eq!(harness.runtime.view().state, RemoteControlState::Lost);

    let reconnected = harness.next_event().await;
    assert!(matches!(reconnected, JellyfinWebSocketEvent::Reconnected));
    let update = harness.drive(Input::Event {
      remote,
      event: reconnected,
    });
    assert_eq!(harness.runtime.view().state, RemoteControlState::Connecting);
    assert!(update.notices.is_empty());

    drop(gate);
    let completion = harness.next_completion().await;
    let update = harness.drive(Input::Completed(completion));
    assert!(update.notices.is_empty());
    assert_eq!(harness.runtime.view().state, RemoteControlState::Connecting);

    // The in-flight registration's session validation lands before the
    // follow-up registration for the new socket session.
    let first_validation = server.next_request().await;
    assert!(first_validation.headline.starts_with("GET /Sessions"));
    let second = server.next_request().await;
    assert!(second.headline.starts_with("POST /Sessions/Capabilities"));
    assert!(second
      .authorization
      .expect("authorization header")
      .contains(r#"Device="Bedroom""#));
    let _validation = server.next_request().await;
    let completion = harness.next_completion().await;
    let update = harness.drive(Input::Completed(completion));
    assert!(update.notices.contains(&Notice::ConnectionRestored));
    assert_eq!(harness.runtime.view().state, RemoteControlState::Available);
    assert_eq!(server.ws_session_count(), 2);

    // The event channel is stable across the reconnect.
    let channel = harness.runtime.events().expect("event channel");
    assert_eq!(receiver, Arc::as_ptr(&channel.receiver));
    server.stop();
  }

  #[tokio::test]
  async fn teardown_during_startup_defers_cleanup_and_joins_late_waiters() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();

    let upgrade = Arc::clone(&server.upgrade_gate);
    let upgrade = upgrade.lock().await;
    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let update = harness.drive(Input::Retire(Waiter::Account(7)));
    assert!(update.settled.is_empty());
    assert!(harness.runtime.events().is_none());
    assert!(!harness.runtime.view().quiescent);
    let update = harness.drive(Input::Retire(Waiter::Quit));
    assert!(update.settled.is_empty());

    drop(upgrade);
    let completion = harness.next_completion().await;
    assert!(matches!(
      completion.kind,
      CompletionKind::Startup { result: Ok(_), .. }
    ));
    let update = harness.drive(Input::Completed(completion));
    assert!(update.settled.is_empty());
    assert!(update.notices.is_empty());
    assert_eq!(
      harness.runtime.view().state,
      RemoteControlState::Unavailable
    );
    assert!(!harness.runtime.view().quiescent);

    let _capabilities = server.next_request().await;
    let _validation = server.next_request().await;
    let completion = harness.next_completion().await;
    assert!(matches!(completion.kind, CompletionKind::Cleanup));
    let update = harness.drive(Input::Completed(completion.clone()));
    assert_eq!(update.settled, vec![Waiter::Account(7), Waiter::Quit]);
    assert!(harness.runtime.view().quiescent);

    // A duplicate cleanup completion for the same resource is ignored.
    let update = harness.drive(Input::Completed(completion));
    assert!(update.settled.is_empty());
    server.stop();
  }

  #[tokio::test]
  async fn teardown_during_registration_defers_cleanup_until_it_completes() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let _connected = harness.next_event().await;
    harness.finish_startup().await;
    let _capabilities = server.next_request().await;
    let _validation = server.next_request().await;

    let gate = Arc::clone(&server.capabilities_gate);
    let gate = gate.lock().await;
    harness.drive(Input::Rename("Bedroom".to_owned()));
    let _in_flight = server.next_request().await;
    let update = harness.drive(Input::Retire(Waiter::Quit));
    assert!(update.settled.is_empty());
    assert!(!harness.runtime.view().quiescent);

    drop(gate);
    let completion = harness.next_completion().await;
    assert!(matches!(
      completion.kind,
      CompletionKind::Registration { .. }
    ));
    let update = harness.drive(Input::Completed(completion));
    assert!(update.settled.is_empty());
    assert!(!harness.runtime.view().quiescent);

    let completion = harness.next_completion().await;
    assert!(matches!(completion.kind, CompletionKind::Cleanup));
    let update = harness.drive(Input::Completed(completion));
    assert_eq!(update.settled, vec![Waiter::Quit]);
    assert!(harness.runtime.view().quiescent);
    server.stop();
  }

  #[tokio::test]
  async fn failed_registration_tears_the_target_down() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let _connected = harness.next_event().await;
    harness.finish_startup().await;
    let _capabilities = server.next_request().await;
    let _validation = server.next_request().await;

    server.set_capabilities_status("500 Internal Server Error");
    harness.drive(Input::Rename("Bedroom".to_owned()));
    let _in_flight = server.next_request().await;
    let completion = harness.next_completion().await;
    let update = harness.drive(Input::Completed(completion));
    assert_eq!(update.notices, vec![Notice::RegistrationFailed]);
    assert_eq!(
      harness.runtime.view().state,
      RemoteControlState::Unavailable
    );
    assert!(!harness.runtime.view().quiescent);

    let completion = harness.next_completion().await;
    assert!(matches!(completion.kind, CompletionKind::Cleanup));
    harness.drive(Input::Completed(completion));
    server.stop();
  }

  #[tokio::test]
  async fn unreachable_server_reports_connection_failure_and_still_closes() {
    let mut server = FakeServer::start();
    let client = authenticated_client(&server.url).await;
    let _auth = server.next_request().await;
    let _info = server.next_request().await;
    server.stop();
    let mut harness = Harness::new();

    harness.drive(Input::Start {
      client,
      name: "Living Room".to_owned(),
    });
    let completion = harness.next_completion().await;
    assert!(matches!(
      completion.kind,
      CompletionKind::Startup {
        result: Err(StartError::ConnectionFailed),
        ..
      }
    ));
    let update = harness.drive(Input::Completed(completion));
    assert_eq!(
      update.notices,
      vec![Notice::StartFailed(StartError::ConnectionFailed)]
    );
    assert_eq!(
      harness.runtime.view().state,
      RemoteControlState::Unavailable
    );

    let completion = harness.next_completion().await;
    assert!(matches!(completion.kind, CompletionKind::Cleanup));
    harness.drive(Input::Completed(completion));
    assert!(harness.runtime.view().quiescent);
  }

  #[tokio::test]
  async fn unauthenticated_client_reports_session_unavailable() {
    let mut harness = Harness::new();
    harness.drive(Input::Start {
      client: Arc::new(JellyfinClient::new()),
      name: "Living Room".to_owned(),
    });
    let completion = harness.next_completion().await;
    assert!(matches!(
      completion.kind,
      CompletionKind::Startup {
        result: Err(StartError::SessionUnavailable),
        ..
      }
    ));
    let update = harness.drive(Input::Completed(completion));
    assert_eq!(
      update.notices,
      vec![Notice::StartFailed(StartError::SessionUnavailable)]
    );
    let completion = harness.next_completion().await;
    assert!(matches!(completion.kind, CompletionKind::Cleanup));
    harness.drive(Input::Completed(completion));
    assert!(harness.runtime.view().quiescent);
  }

  #[test]
  fn events_for_foreign_tokens_and_commands_while_starting_are_ignored() {
    let mut gate = RequestGate::default();
    let foreign = gate.begin_remote();
    let mut runtime = Runtime::new(&mut gate);

    let update = runtime.update(
      Input::Event {
        remote: foreign,
        event: playstate("Pause"),
      },
      &mut gate,
    );
    assert!(update.commands.is_empty());
    assert!(update.notices.is_empty());

    let update = runtime.update(
      Input::Event {
        remote: runtime.token(),
        event: playstate("Pause"),
      },
      &mut gate,
    );
    assert!(update.commands.is_empty());
  }
}
