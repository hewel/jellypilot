//! Async IPC connection to MPV.
//!
//! Handles platform-specific socket/pipe connections.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use parking_lot::Mutex;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(any(test, feature = "test-utils"))]
use tokio::sync::Notify;
use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;

use jellypilot_core::player_logs::PlayerLogs;

static CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

use super::protocol::{MpvCommand, MpvEvent, MpvMessage, MpvResponse};

fn parse_error_log_summary(error: &serde_json::Error, message_bytes: usize) -> String {
  format!(
    "category={:?}, line={}, column={}, bytes={message_bytes}",
    error.classify(),
    error.line(),
    error.column()
  )
}

fn response_trace_summary(result: &Result<MpvResponse, IpcError>) -> String {
  match result {
    Ok(response) => format!(
      "request_id={}, success={}",
      response.request_id,
      response.is_success()
    ),
    Err(IpcError::ConnectionFailed(_)) => "error=connection-failed".to_owned(),
    Err(IpcError::WriteFailed(_)) => "error=write-failed".to_owned(),
    Err(IpcError::Timeout) => "error=timeout".to_owned(),
    Err(IpcError::Disconnected) => "error=disconnected".to_owned(),
  }
}

fn command_trace_summary(command: &MpvCommand) -> String {
  format!(
    "request_id={}, command={}",
    command.request_id,
    command.command_name()
  )
}

#[derive(Error, Debug)]
pub(crate) enum IpcError {
  #[error("Connection failed: {0}")]
  ConnectionFailed(String),
  #[error("Write failed: {0}")]
  WriteFailed(#[from] std::io::Error),
  #[error("Command timeout")]
  Timeout,
  #[error("Disconnected")]
  Disconnected,
}

/// Pending request waiting for response.
type PendingRequest = oneshot::Sender<Result<MpvResponse, IpcError>>;

/// IPC connection state shared between writer and reader.
struct IpcState {
  pending: HashMap<i64, PendingRequest>,
}

impl IpcState {
  /// Drain all pending requests with Disconnected error.
  fn drain_pending(&mut self) {
    let pending = std::mem::take(&mut self.pending);
    for (request_id, tx) in pending {
      log::debug!("Draining pending request {}", request_id);
      let _ = tx.send(Err(IpcError::Disconnected));
    }
  }
}

/// A command already admitted to the writer queue, awaiting MPV's response.
///
/// The request is registered and the wire message queued before this exists,
/// so [`PendingAck::wait`] only waits for the acknowledgement. Dropping it
/// without waiting withdraws the pending request; the queued bytes may still
/// be written, but no response is delivered.
pub(crate) struct PendingAck {
  request_id: i64,
  state: Arc<Mutex<IpcState>>,
  rx: oneshot::Receiver<Result<MpvResponse, IpcError>>,
  capture: Option<(Arc<PlayerLogs>, u64)>,
}

impl PendingAck {
  /// Wait for MPV's acknowledgement with the standard command timeout.
  pub(crate) async fn wait(mut self) -> Result<MpvResponse, IpcError> {
    let result = match tokio::time::timeout(Duration::from_secs(5), &mut self.rx).await {
      Ok(Ok(result)) => {
        log::trace!("MPV response received: {}", response_trace_summary(&result));
        result
      }
      Ok(Err(_)) => {
        // Channel was closed (sender dropped) - connection died
        log::error!("MPV IPC channel closed unexpectedly");
        Err(IpcError::Disconnected)
      }
      Err(_) => {
        log::error!(
          "MPV command timeout after 5 seconds, request_id={}",
          self.request_id
        );
        Err(IpcError::Timeout)
      }
    };
    if let Some((logs, connection_id)) = self.capture.take() {
      let success = result.as_ref().is_ok_and(MpvResponse::is_success);
      logs.record(
        connection_id,
        "command",
        if success { "info" } else { "error" },
        &format!("request_id={}, success={success}", self.request_id),
      );
    }
    result
  }
}

impl Drop for PendingAck {
  fn drop(&mut self) {
    // Reclaim the pending slot when the waiter goes away (timeout, dropped
    // future). A response that already arrived removed it first.
    self.state.lock().pending.remove(&self.request_id);
  }
}

/// Writer channel message.
enum WriteMessage {
  Command {
    data: Vec<u8>,
    /// Queue slot held until the writer dequeues this message. Ordinary
    /// commands stay bounded even though the channel itself is not.
    permit: Option<OwnedSemaphorePermit>,
  },
  /// A `set_property pause false` request decided at dequeue time: when the
  /// hold is raised the wire command becomes `pause true`, so a resume
  /// revoked while queued cannot unpause behind a held presentation.
  GuardedResume {
    command: MpvCommand,
    hold: Arc<AtomicBool>,
    permit: Option<OwnedSemaphorePermit>,
  },
  /// Test seam: the writer parks until `gate` fires, holding every message
  /// queued behind it so tests can observe dequeue-time decisions.
  #[cfg(any(test, feature = "test-utils"))]
  Barrier(Arc<Notify>),
  Close,
}
/// MPV IPC connection.
pub struct MpvIpc {
  state: Arc<Mutex<IpcState>>,
  write_tx: async_channel::Sender<WriteMessage>,
  /// Bounds ordinary commands in flight to the writer: a permit is acquired
  /// before enqueue and released when the writer dequeues the message.
  /// Urgent synchronous enqueues (the close pause) bypass it.
  write_permits: Arc<Semaphore>,
  event_rx: Receiver<MpvEvent>,
  closed: Arc<AtomicBool>,
  reader_handle: JoinHandle<()>,
  writer_handle: JoinHandle<()>,
  log_handle: Option<JoinHandle<()>>,
  logs: Arc<PlayerLogs>,
  connection_id: u64,
}

impl MpvIpc {
  /// Connect to MPV IPC socket/pipe.
  pub(crate) async fn connect(path: &str, retry_count: u32) -> Result<Self, IpcError> {
    let mut last_error = None;

    for attempt in 0..retry_count {
      if attempt > 0 {
        tokio::time::sleep(Duration::from_millis(100 * (attempt as u64 + 1))).await;
      }

      match Self::try_connect(path, true).await {
        Ok(ipc) => return Ok(ipc),
        Err(e) => {
          log::debug!("IPC connect attempt {} failed: {}", attempt + 1, e);
          last_error = Some(e);
        }
      }
    }

    Err(last_error.unwrap_or_else(|| IpcError::ConnectionFailed("Unknown error".into())))
  }

  /// A dedicated property observer must not duplicate the control client's logs.
  pub(crate) async fn connect_observer(path: &str) -> Result<Self, IpcError> {
    Self::try_connect(path, false).await
  }

  #[cfg(windows)]
  async fn try_connect(path: &str, capture_logs: bool) -> Result<Self, IpcError> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let client = ClientOptions::new()
      .open(path)
      .map_err(|e| IpcError::ConnectionFailed(format!("Failed to open pipe: {}", e)))?;

    let (reader, writer) = tokio::io::split(client);
    Self::setup(reader, writer, capture_logs).await
  }

  #[cfg(not(windows))]
  async fn try_connect(path: &str, capture_logs: bool) -> Result<Self, IpcError> {
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(path)
      .await
      .map_err(|e| IpcError::ConnectionFailed(e.to_string()))?;

    let (reader, writer) = tokio::io::split(stream);
    Self::setup(reader, writer, capture_logs).await
  }

  async fn setup<R, W>(reader: R, writer: W, capture_logs: bool) -> Result<Self, IpcError>
  where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
  {
    Self::setup_with_logs(
      reader,
      writer,
      jellypilot_core::player_logs::global().clone(),
      capture_logs,
    )
    .await
  }

  async fn setup_with_logs<R, W>(
    reader: R,
    writer: W,
    logs: Arc<PlayerLogs>,
    capture_logs: bool,
  ) -> Result<Self, IpcError>
  where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
  {
    let connection_id = CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
    let state = Arc::new(Mutex::new(IpcState {
      pending: HashMap::new(),
    }));

    let closed = Arc::new(AtomicBool::new(false));

    let (event_tx, event_rx) = async_channel::bounded(100); // Bounded to prevent memory bloat
                                                            // Unbounded so a synchronous enqueue (e.g. the close pause) can never be
                                                            // refused on capacity; ordinary commands are bounded by write_permits.
    let (write_tx, write_rx) = async_channel::unbounded::<WriteMessage>();
    let write_permits = Arc::new(Semaphore::new(100));

    // Spawn reader task
    let reader_state = state.clone();
    let reader_closed = closed.clone();
    let reader_logs = logs.clone();
    let reader_handle = tokio::spawn(async move {
      Self::reader_loop(
        reader,
        reader_state,
        event_tx,
        reader_closed,
        reader_logs,
        connection_id,
        capture_logs,
      )
      .await;
    });

    // Spawn writer task - pass state and closed for error handling
    let writer_state = state.clone();
    let writer_closed = closed.clone();
    let writer_handle = tokio::spawn(async move {
      Self::writer_loop(writer, write_rx, writer_state, writer_closed).await;
    });

    // Own the spawned tasks before awaiting the optional handshake, so a
    // cancelled connection attempt still closes the socket and aborts them.
    let mut ipc = Self {
      state,
      write_tx,
      write_permits,
      event_rx,
      closed,
      reader_handle,
      writer_handle,
      log_handle: None,
      logs,
      connection_id,
    };
    if !capture_logs {
      return Ok(ipc);
    }
    let initially_enabled = ipc.logs.enabled();
    if initially_enabled {
      // Subscribe before the caller can load media when capture was pre-enabled.
      Self::update_log_subscription(
        &ipc.state,
        &ipc.write_tx,
        &ipc.write_permits,
        &ipc.closed,
        &ipc.logs,
        connection_id,
        true,
      )
      .await;
    }
    let log_state = ipc.state.clone();
    let log_write = ipc.write_tx.clone();
    let log_permits = ipc.write_permits.clone();
    let log_closed = ipc.closed.clone();
    let log_capture = ipc.logs.clone();
    ipc.log_handle = Some(tokio::spawn(async move {
      Self::log_subscription_loop(
        log_state,
        log_write,
        log_permits,
        log_closed,
        log_capture,
        connection_id,
        initially_enabled,
      )
      .await;
    }));
    Ok(ipc)
  }

  /// Queue a barrier that parks the writer until `gate` fires. Everything
  /// already queued is written first; everything queued after stays pending
  /// until the gate opens. Test seam for dequeue-time ordering assertions.
  #[cfg(any(test, feature = "test-utils"))]
  #[doc(hidden)]
  pub(crate) fn enqueue_writer_barrier(&self, gate: Arc<Notify>) -> Result<(), IpcError> {
    self
      .write_tx
      .try_send(WriteMessage::Barrier(gate))
      .map_err(|_| IpcError::Disconnected)
  }
  #[cfg(any(test, feature = "test-utils"))]
  pub(crate) async fn from_io_for_test<R, W>(reader: R, writer: W) -> Result<Self, IpcError>
  where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
  {
    Self::setup(reader, writer, true).await
  }

  async fn reader_loop<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    state: Arc<Mutex<IpcState>>,
    event_tx: Sender<MpvEvent>,
    closed: Arc<AtomicBool>,
    logs: Arc<PlayerLogs>,
    connection_id: u64,
    capture_logs: bool,
  ) {
    log::info!("MPV IPC reader loop started");
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
      // Check if we should exit
      if closed.load(Ordering::Acquire) {
        log::info!("MPV IPC reader loop: close signal received");
        break;
      }

      line.clear();
      match buf_reader.read_line(&mut line).await {
        Ok(0) => {
          log::info!("MPV IPC connection closed (EOF)");
          break;
        }
        Ok(_) => {
          let trimmed = line.trim();
          if trimmed.is_empty() {
            continue;
          }

          match MpvMessage::parse(trimmed) {
            Ok(MpvMessage::Response(response)) => {
              log::trace!(
                "MPV reader: received response for request_id={}",
                response.request_id
              );
              let mut state = state.lock();
              if let Some(tx) = state.pending.remove(&response.request_id) {
                let _ = tx.send(Ok(response));
              }
            }
            Ok(MpvMessage::Log(message)) => {
              if capture_logs {
                logs.record(
                  connection_id,
                  &message.prefix,
                  &message.level,
                  &message.text,
                );
              }
            }
            Ok(MpvMessage::Event(event)) => {
              if !capture_logs && event.event == "event-queue-overflow" {
                // An observer must reconnect for a fresh initial value after
                // either MPV's queue or our delivery queue loses a change.
                break;
              }
              if capture_logs {
                if matches!(
                  event.event.as_str(),
                  "file-loaded"
                    | "seek"
                    | "playback-restart"
                    | "audio-reconfig"
                    | "video-reconfig"
                    | "tracks-changed"
                    | "track-switched"
                    | "end-file"
                    | "shutdown"
                ) {
                  logs.record(connection_id, "ipc", "info", &event.event);
                } else if event.event == "event-queue-overflow" {
                  logs.record(
                    connection_id,
                    "ipc",
                    "warn",
                    "MPV event queue overflow; player log may be incomplete",
                  );
                }
              }
              log::debug!("MPV event: {} (reason={:?})", event.event, event.reason);
              // Use try_send to avoid blocking if channel is full
              if event_tx.try_send(event).is_err() {
                if !capture_logs {
                  break;
                }
                log::warn!("Event channel full, dropping event");
              }
            }
            Err(e) => {
              log::warn!(
                "Failed to parse MPV message ({})",
                parse_error_log_summary(&e, trimmed.len())
              );
            }
          }
        }
        Err(e) => {
          log::error!("MPV IPC read error: {}", e);
          break;
        }
      }
    }

    // Mark as closed
    closed.store(true, Ordering::Release);

    // Drain all pending requests on exit - they will never get responses
    log::info!("MPV IPC reader exiting, draining pending requests");
    state.lock().drain_pending();

    // Close event channel by dropping sender (happens automatically when task ends)
    log::info!("MPV IPC reader loop ended");
  }

  async fn writer_loop<W: tokio::io::AsyncWrite + Unpin>(
    mut writer: W,
    write_rx: async_channel::Receiver<WriteMessage>,
    state: Arc<Mutex<IpcState>>,
    closed: Arc<AtomicBool>,
  ) {
    log::info!("MPV IPC writer loop started");

    while let Ok(msg) = write_rx.recv().await {
      let data = match msg {
        WriteMessage::Command { data, permit } => {
          // Dequeued: release the queue slot before writing.
          drop(permit);
          data
        }
        WriteMessage::GuardedResume {
          mut command,
          hold,
          permit,
        } => {
          drop(permit);
          if hold.load(Ordering::Acquire) {
            // The hold was raised after this resume was queued: write the
            // pause instead so the engine never unpauses behind a held
            // presentation. The request_id is preserved so the caller's
            // acknowledgement still resolves normally.
            log::info!(
              "MPV guarded resume revoked before write; pausing instead (request_id={})",
              command.request_id
            );
            command.command[2] = serde_json::Value::Bool(true);
          }
          match serde_json::to_string(&command) {
            Ok(json) => json.into_bytes(),
            Err(e) => {
              log::error!("MPV guarded resume serialization failed: {}", e);
              if let Some(tx) = state.lock().pending.remove(&command.request_id) {
                let _ = tx.send(Err(IpcError::WriteFailed(std::io::Error::new(
                  std::io::ErrorKind::InvalidData,
                  e,
                ))));
              }
              continue;
            }
          }
        }
        #[cfg(any(test, feature = "test-utils"))]
        WriteMessage::Barrier(gate) => {
          gate.notified().await;
          continue;
        }
        WriteMessage::Close => {
          log::info!("MPV IPC writer closing");
          break;
        }
      };
      if let Err(e) = writer.write_all(&data).await {
        log::error!("MPV IPC write error: {}", e);
        break;
      }
      if let Err(e) = writer.write_all(b"\n").await {
        log::error!("MPV IPC write newline error: {}", e);
        break;
      }
      if let Err(e) = writer.flush().await {
        log::error!("MPV IPC flush error: {}", e);
        break;
      }
      log::trace!("MPV command written to pipe");
    }

    // On any exit (IO error or close), mark closed and drain pending
    // so callers get immediate Disconnected instead of 5s timeout
    log::info!("MPV IPC writer exiting, marking closed and draining pending");
    closed.store(true, Ordering::Release);
    state.lock().drain_pending();

    log::info!("MPV IPC writer loop ended");
  }

  /// Check if the connection is closed.
  pub(crate) fn is_closed(&self) -> bool {
    self.closed.load(Ordering::Acquire)
  }

  /// Send a command to MPV and wait for response.
  pub(crate) async fn send_command(&self, cmd: MpvCommand) -> Result<MpvResponse, IpcError> {
    let permit = Self::acquire_write_permit(&self.write_permits, &self.closed).await?;
    self.enqueue_command(cmd, Some(permit))?.wait().await
  }

  /// Wait for an ordinary-command queue slot. Fails fast when the connection
  /// is closed, and `close()`/`Drop` close the semaphore so a blocked waiter
  /// wakes with `Disconnected` instead of hanging.
  async fn acquire_write_permit(
    permits: &Arc<Semaphore>,
    closed: &AtomicBool,
  ) -> Result<OwnedSemaphorePermit, IpcError> {
    if closed.load(Ordering::Acquire) {
      return Err(IpcError::Disconnected);
    }
    permits
      .clone()
      .acquire_owned()
      .await
      .map_err(|_| IpcError::Disconnected)
  }

  /// Register and queue a command synchronously; the returned [`PendingAck`]
  /// only waits for MPV's acknowledgement. `permit` is `Some` for ordinary
  /// commands (released when the writer dequeues the message) and `None` for
  /// urgent enqueues that bypass the bound.
  pub(crate) fn enqueue_command(
    &self,
    cmd: MpvCommand,
    permit: Option<OwnedSemaphorePermit>,
  ) -> Result<PendingAck, IpcError> {
    let capture = self.command_capture(&cmd);
    if let Some((logs, connection_id)) = &capture {
      let mut summary = command_trace_summary(&cmd);
      for arg in cmd.command.iter().skip(1) {
        if arg.is_number()
          || arg.is_boolean()
          || arg
            .as_str()
            .is_some_and(|p| matches!(p, "aid" | "sid" | "pause"))
        {
          summary.push_str(&format!(" {arg}"));
        }
      }
      logs.record(*connection_id, "command", "info", &summary);
    }
    let request_id = cmd.request_id;
    let message = Self::serialize_command(&cmd, permit)?;
    Self::enqueue_on(
      &self.state,
      &self.write_tx,
      &self.closed,
      request_id,
      message,
      capture,
    )
  }

  /// Queue a `set_property pause false` whose wire value is decided by the
  /// writer at dequeue time: a raised `hold` downgrades it to `pause true`.
  ///
  /// The hold is checked by the single serialized writer, so a resume queued
  /// before or after the hold was raised can never be written as an unpause
  /// once a later pause has been admitted behind it.
  pub(crate) async fn enqueue_guarded_resume(
    &self,
    hold: Arc<AtomicBool>,
  ) -> Result<PendingAck, IpcError> {
    let cmd = MpvCommand::set_pause(false);
    let permit = Self::acquire_write_permit(&self.write_permits, &self.closed).await?;
    let capture = self.command_capture(&cmd);
    if let Some((logs, connection_id)) = &capture {
      logs.record(
        *connection_id,
        "command",
        "info",
        &format!(
          "request_id={}, command=set_property pause false",
          cmd.request_id
        ),
      );
    }
    let request_id = cmd.request_id;
    Self::enqueue_on(
      &self.state,
      &self.write_tx,
      &self.closed,
      request_id,
      WriteMessage::GuardedResume {
        command: cmd,
        hold,
        permit: Some(permit),
      },
      capture,
    )
  }

  /// Whether this command is recorded in the player log.
  fn command_capture(&self, cmd: &MpvCommand) -> Option<(Arc<PlayerLogs>, u64)> {
    (self.logs.enabled()
      && (matches!(cmd.command_name(), "loadfile" | "stop" | "seek" | "quit")
        || (cmd.command_name() == "set_property"
          && cmd
            .command
            .get(1)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|p| matches!(p, "aid" | "sid" | "pause")))))
    .then(|| (self.logs.clone(), self.connection_id))
  }

  fn serialize_command(
    cmd: &MpvCommand,
    permit: Option<OwnedSemaphorePermit>,
  ) -> Result<WriteMessage, IpcError> {
    serde_json::to_string(cmd)
      .map(|json| WriteMessage::Command {
        data: json.into_bytes(),
        permit,
      })
      .map_err(|e| IpcError::WriteFailed(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
  }

  /// Register `request_id` and push `message` onto the writer queue without
  /// awaiting. The writer channel is unbounded, so admission fails only when
  /// the connection is already closed.
  fn enqueue_on(
    state: &Arc<Mutex<IpcState>>,
    write_tx: &Sender<WriteMessage>,
    closed: &AtomicBool,
    request_id: i64,
    message: WriteMessage,
    capture: Option<(Arc<PlayerLogs>, u64)>,
  ) -> Result<PendingAck, IpcError> {
    // Early check for closed connection
    if closed.load(Ordering::Acquire) {
      return Err(IpcError::Disconnected);
    }

    // Create response channel
    let (tx, rx) = oneshot::channel();

    // Register pending request
    {
      let mut state = state.lock();
      state.pending.insert(request_id, tx);
    }

    // Re-check closed after inserting to handle race with close()/drain_pending()
    // If closed was set between our first check and insert, drain_pending() already ran
    // and won't drain our newly inserted pending - we'd timeout after 5s instead of
    // getting immediate Disconnected
    if closed.load(Ordering::Acquire) {
      if let Some(tx) = state.lock().pending.remove(&request_id) {
        let _ = tx.send(Err(IpcError::Disconnected));
      }
      return Err(IpcError::Disconnected);
    }

    // Queue for the writer task - if this fails, remove pending and return error
    if write_tx.try_send(message).is_err() {
      if let Some(tx) = state.lock().pending.remove(&request_id) {
        let _ = tx.send(Err(IpcError::Disconnected));
      }
      return Err(IpcError::Disconnected);
    }

    log::trace!("MPV command queued, waiting for response...");

    Ok(PendingAck {
      request_id,
      state: state.clone(),
      rx,
      capture,
    })
  }

  async fn send(
    state: &Arc<Mutex<IpcState>>,
    write_tx: &Sender<WriteMessage>,
    write_permits: &Arc<Semaphore>,
    closed: &AtomicBool,
    cmd: MpvCommand,
  ) -> Result<MpvResponse, IpcError> {
    let request_id = cmd.request_id;
    log::trace!("Sending MPV command: {}", command_trace_summary(&cmd));
    let permit = Self::acquire_write_permit(write_permits, closed).await?;
    let message = Self::serialize_command(&cmd, Some(permit))?;
    Self::enqueue_on(state, write_tx, closed, request_id, message, None)?
      .wait()
      .await
  }

  async fn log_subscription_loop(
    state: Arc<Mutex<IpcState>>,
    write_tx: Sender<WriteMessage>,
    write_permits: Arc<Semaphore>,
    closed: Arc<AtomicBool>,
    logs: Arc<PlayerLogs>,
    connection_id: u64,
    mut applied: bool,
  ) {
    // UI changes are observed outside the GPU path. There are no media-property
    // polls here, and subscription commands use the ordinary async IPC writer.
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
      tick.tick().await;
      if closed.load(Ordering::Acquire) {
        break;
      }
      let enabled = logs.enabled();
      if enabled == applied {
        continue;
      }
      applied = enabled;
      Self::update_log_subscription(
        &state,
        &write_tx,
        &write_permits,
        &closed,
        &logs,
        connection_id,
        enabled,
      )
      .await;
    }
  }

  async fn update_log_subscription(
    state: &Arc<Mutex<IpcState>>,
    write_tx: &Sender<WriteMessage>,
    write_permits: &Arc<Semaphore>,
    closed: &AtomicBool,
    logs: &PlayerLogs,
    connection_id: u64,
    enabled: bool,
  ) {
    match Self::send(
      state,
      write_tx,
      write_permits,
      closed,
      MpvCommand::request_log_messages(enabled),
    )
    .await
    {
      Ok(response) if response.is_success() => {
        if enabled {
          logs.record(
            connection_id,
            "ipc",
            "info",
            "Player log subscription active (mpv level=v)",
          );
        }
      }
      _ => logs.record(
        connection_id,
        "ipc",
        "error",
        "Player log subscription failed; toggle capture to retry",
      ),
    }
  }

  /// Get the event receiver for property changes and other events.
  pub(crate) fn events(&self) -> Receiver<MpvEvent> {
    self.event_rx.clone()
  }

  /// Close the connection gracefully.
  /// Note: This signals shutdown but tasks may not stop immediately if blocked on I/O.
  /// Drop will abort tasks forcefully.
  pub(crate) fn close(&self) {
    // Signal closed state with Release ordering so reader/writer see the change
    self.closed.store(true, Ordering::Release);

    // Wake any task blocked acquiring a write permit
    self.write_permits.close();

    // Send close message first (before closing channel)
    let _ = self.write_tx.try_send(WriteMessage::Close);

    // Close the write channel - this will cause writer_loop to exit on next recv
    self.write_tx.close();

    // Drain pending requests immediately
    self.state.lock().drain_pending();

    log::info!("MpvIpc::close() completed");
  }
}

impl Drop for MpvIpc {
  fn drop(&mut self) {
    log::info!("MpvIpc::drop() - cleaning up");

    // Signal closed with Release ordering
    self.closed.store(true, Ordering::Release);

    // Wake any task blocked acquiring a write permit
    self.write_permits.close();

    // Close write channel
    self.write_tx.close();

    // Abort tasks to release socket handles
    self.reader_handle.abort();
    self.writer_handle.abort();
    if let Some(handle) = &self.log_handle {
      handle.abort();
    }

    // Drain any remaining pending requests
    self.state.lock().drain_pending();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::future::{poll_fn, Future};

  #[tokio::test]
  async fn observer_does_not_duplicate_player_log_records() {
    let logs = Arc::new(PlayerLogs::new(4096));
    logs.set_enabled(true);
    let before = logs.snapshot();
    let (client, mut peer) = tokio::io::duplex(4096);
    let (reader, writer) = tokio::io::split(client);
    let ipc = MpvIpc::setup_with_logs(reader, writer, logs.clone(), false)
      .await
      .unwrap();
    peer
      .write_all(
        b"{\"event\":\"log-message\",\"prefix\":\"vo\",\"level\":\"info\",\"text\":\"frame\"}\n\
          {\"event\":\"file-loaded\"}\n",
      )
      .await
      .unwrap();
    tokio::time::timeout(Duration::from_secs(2), ipc.events().recv())
      .await
      .unwrap()
      .unwrap();
    assert_eq!(logs.snapshot(), before);
  }

  #[tokio::test]
  async fn observer_disconnects_when_either_event_queue_overflows() {
    for events in [
      "{\"event\":\"video-reconfig\"}\n".repeat(101),
      "{\"event\":\"event-queue-overflow\"}\n".to_owned(),
    ] {
      let (client, mut peer) = tokio::io::duplex(64 * 1024);
      let (reader, writer) = tokio::io::split(client);
      let mut ipc = MpvIpc::setup_with_logs(reader, writer, Arc::new(PlayerLogs::new(4096)), false)
        .await
        .unwrap();
      // Keep the peer open and withhold consumption until overflow: EOF must
      // not be what invalidates this observer's cached source classification.
      peer.write_all(events.as_bytes()).await.unwrap();
      tokio::time::timeout(Duration::from_secs(2), &mut ipc.reader_handle)
        .await
        .unwrap()
        .unwrap();
      assert!(matches!(
        ipc
          .send_command(MpvCommand::observe_property(1, "video-params/gamma"))
          .await,
        Err(IpcError::Disconnected)
      ));
    }
  }

  #[tokio::test]
  async fn player_logs_toggle_over_ipc_without_consuming_playback_events() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let logs = Arc::new(PlayerLogs::new(64 * 1024));
    logs.set_enabled(true);
    let (client, server) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(client);
    let (ready_tx, ready_rx) = oneshot::channel();
    let (disabled_tx, disabled_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
      let (reader, mut writer) = tokio::io::split(server);
      let mut reader = BufReader::new(reader).lines();
      let command: serde_json::Value =
        serde_json::from_str(&reader.next_line().await.unwrap().unwrap()).unwrap();
      assert_eq!(
        command["command"],
        serde_json::json!(["request_log_messages", "v"])
      );
      writer
        .write_all(
          format!(
            "{{\"request_id\":{},\"error\":\"success\"}}\n",
            command["request_id"]
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      // More log records than the normal event queue capacity. Payload text
      // deliberately resembles a response to check envelope-based dispatch.
      for _ in 0..150 {
        writer.write_all(b"{\"event\":\"log-message\",\"prefix\":\"cplayer\",\"level\":\"v\",\"text\":\"delaying audio start 23 vs. 1, diff=22 request_id\"}\n").await.unwrap();
      }
      writer.write_all(b"{\"event\":\"log-message\",\"prefix\":\"ffmpeg\",\"level\":\"v\",\"text\":\"http-header-fields: X-Emby-Token: secret-player-credential\"}\n{\"event\":\"file-loaded\"}\n").await.unwrap();
      ready_tx.send(()).unwrap();
      let command: serde_json::Value =
        serde_json::from_str(&reader.next_line().await.unwrap().unwrap()).unwrap();
      assert_eq!(
        command["command"],
        serde_json::json!(["get_property", "aid"])
      );
      writer
        .write_all(
          format!(
            "{{\"request_id\":{},\"error\":\"success\",\"data\":6}}\n",
            command["request_id"]
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      let command: serde_json::Value =
        serde_json::from_str(&reader.next_line().await.unwrap().unwrap()).unwrap();
      assert_eq!(
        command["command"],
        serde_json::json!(["request_log_messages", "no"])
      );
      writer
        .write_all(
          format!(
            "{{\"request_id\":{},\"error\":\"success\"}}\n",
            command["request_id"]
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      disabled_tx.send(()).unwrap();
      assert!(reader.next_line().await.unwrap().is_none());
    });
    let ipc = MpvIpc::setup_with_logs(reader, writer, logs.clone(), true)
      .await
      .unwrap();
    assert!(logs.snapshot().contains("Player log subscription active"));
    tokio::time::timeout(Duration::from_secs(2), ready_rx)
      .await
      .unwrap()
      .unwrap();
    let response = ipc
      .send_command(MpvCommand::get_property("aid"))
      .await
      .unwrap();
    assert_eq!(response.data, Some(serde_json::json!(6)));
    let events = ipc.events();
    assert_eq!(events.try_recv().unwrap().event, "file-loaded");
    assert!(events.try_recv().is_err());
    let captured = logs.snapshot();
    assert_eq!(captured.matches("delaying audio start").count(), 150);
    assert!(!captured.contains("secret-player-credential"));
    logs.set_enabled(false);
    tokio::time::timeout(Duration::from_secs(2), disabled_rx)
      .await
      .unwrap()
      .unwrap();
    assert!(logs.snapshot().contains("delaying audio start"));
    drop(ipc);
    tokio::time::timeout(Duration::from_secs(2), server)
      .await
      .unwrap()
      .unwrap();
  }

  /// A peer that withholds reads until released, then acknowledges every
  /// command and returns the raw wire lines in arrival order.
  fn gated_peer(
    server: tokio::io::DuplexStream,
  ) -> (oneshot::Sender<()>, JoinHandle<Vec<serde_json::Value>>) {
    let (release_tx, release_rx) = oneshot::channel();
    let peer = tokio::spawn(async move {
      let (reader, mut writer) = tokio::io::split(server);
      let mut lines = BufReader::new(reader).lines();
      let mut commands = Vec::new();
      release_rx.await.expect("test must release the peer");
      while let Ok(Some(line)) = lines.next_line().await {
        let command: serde_json::Value = serde_json::from_str(&line).unwrap();
        writer
          .write_all(
            format!(
              "{{\"request_id\":{},\"error\":\"success\"}}\n",
              command["request_id"]
            )
            .as_bytes(),
          )
          .await
          .unwrap();
        commands.push(command);
      }
      commands
    });
    (release_tx, peer)
  }

  #[tokio::test]
  async fn a_revoked_guarded_resume_is_written_as_pause() {
    let (client_stream, server_stream) = tokio::io::duplex(8);
    let (reader, writer) = tokio::io::split(client_stream);
    let (release_tx, peer) = gated_peer(server_stream);
    let ipc = MpvIpc::from_io_for_test(reader, writer).await.unwrap();

    // Occupy the writer with a command the gated peer never reads, so the
    // guarded resume stays queued until after the hold is raised.
    let filler = ipc
      .enqueue_command(MpvCommand::get_property("aid"), None)
      .unwrap();
    let hold = Arc::new(AtomicBool::new(false));
    let resume = ipc.enqueue_guarded_resume(Arc::clone(&hold)).await.unwrap();
    hold.store(true, Ordering::Release);
    let pause = ipc
      .enqueue_command(MpvCommand::set_pause(true), None)
      .unwrap();

    release_tx.send(()).unwrap();
    for ack in [filler, resume, pause] {
      assert!(ack.wait().await.unwrap().is_success());
    }
    drop(ipc);
    let commands = tokio::time::timeout(Duration::from_secs(2), peer)
      .await
      .unwrap()
      .unwrap();

    assert_eq!(commands.len(), 3);
    assert_eq!(
      commands[0]["command"],
      serde_json::json!(["get_property", "aid"])
    );
    assert_eq!(
      commands[1]["command"],
      serde_json::json!(["set_property", "pause", true]),
      "a resume revoked while queued must reach the engine as pause=true"
    );
    assert_eq!(
      commands[2]["command"],
      serde_json::json!(["set_property", "pause", true])
    );
  }

  #[tokio::test]
  async fn a_new_lease_does_not_revive_a_revoked_guard() {
    let (client_stream, server_stream) = tokio::io::duplex(8);
    let (reader, writer) = tokio::io::split(client_stream);
    let (release_tx, peer) = gated_peer(server_stream);
    let ipc = MpvIpc::from_io_for_test(reader, writer).await.unwrap();

    let filler = ipc
      .enqueue_command(MpvCommand::get_property("aid"), None)
      .unwrap();
    let lease = Arc::new(AtomicBool::new(false));
    let stale_resume = ipc
      .enqueue_guarded_resume(Arc::clone(&lease))
      .await
      .unwrap();
    lease.store(true, Ordering::Release);
    // A later lease (e.g. the window being shown again) must not change the
    // fate of a resume captured under the revoked one.
    let next_lease = Arc::new(AtomicBool::new(false));
    let fresh_resume = ipc
      .enqueue_guarded_resume(Arc::clone(&next_lease))
      .await
      .unwrap();

    release_tx.send(()).unwrap();
    for ack in [filler, stale_resume, fresh_resume] {
      assert!(ack.wait().await.unwrap().is_success());
    }
    drop(ipc);
    let commands = tokio::time::timeout(Duration::from_secs(2), peer)
      .await
      .unwrap()
      .unwrap();

    assert_eq!(
      commands[1]["command"],
      serde_json::json!(["set_property", "pause", true]),
      "the revoked lease's resume must stay a pause"
    );
    assert_eq!(
      commands[2]["command"],
      serde_json::json!(["set_property", "pause", false]),
      "the live lease's resume must reach the engine as an unpause"
    );
  }

  #[tokio::test]
  async fn enqueue_after_close_fails_without_leaking_pending() {
    let (client_stream, server_stream) = tokio::io::duplex(64);
    let (reader, writer) = tokio::io::split(client_stream);
    let ipc = MpvIpc::from_io_for_test(reader, writer).await.unwrap();
    drop(server_stream);
    ipc.close();

    assert!(matches!(
      ipc.enqueue_command(MpvCommand::set_pause(true), None),
      Err(IpcError::Disconnected)
    ));
    assert!(matches!(
      ipc
        .enqueue_guarded_resume(Arc::new(AtomicBool::new(false)))
        .await,
      Err(IpcError::Disconnected)
    ));
    assert!(ipc.state.lock().pending.is_empty());
  }

  /// Poll a future once; returns whether it completed.
  async fn poll_once<F: Future + Unpin>(future: &mut F) -> bool {
    poll_fn(|cx| std::task::Poll::Ready(std::pin::Pin::new(&mut *future).poll(cx).is_ready())).await
  }

  #[tokio::test]
  async fn a_full_command_queue_defers_admission_behind_urgent_pauses() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (reader, writer) = tokio::io::split(client_stream);
    let (release_tx, peer) = gated_peer(server_stream);
    let ipc = MpvIpc::from_io_for_test(reader, writer).await.unwrap();

    // Park the writer so every command below stays queued in order.
    let gate = Arc::new(Notify::new());
    ipc.enqueue_writer_barrier(Arc::clone(&gate)).unwrap();

    // Fill the ordinary-command bound through the real admission path: each
    // future acquires a slot and enqueues on its first poll, then waits for
    // the response.
    let mut queued = Vec::new();
    for _ in 0..100 {
      let mut command = Box::pin(ipc.send_command(MpvCommand::get_property("aid")));
      // First poll admits the command, then it pends on the response.
      assert!(!poll_once(&mut command).await);
      queued.push(command);
    }

    // The 101st ordinary command cannot be admitted until a slot frees.
    let mut overflow = Box::pin(ipc.send_command(MpvCommand::get_property("sid")));
    assert!(
      !poll_once(&mut overflow).await,
      "a full queue must defer ordinary admission"
    );

    // The urgent close pause bypasses the bound entirely.
    let urgent = ipc
      .enqueue_command(MpvCommand::set_pause(true), None)
      .expect("urgent enqueue must bypass the bound");

    gate.notify_one();
    release_tx.send(()).unwrap();
    for command in &mut queued {
      assert!(command.await.unwrap().is_success());
    }
    assert!(overflow.await.unwrap().is_success());
    assert!(urgent.wait().await.unwrap().is_success());
    drop(queued);
    drop(ipc);
    let commands = tokio::time::timeout(Duration::from_secs(2), peer)
      .await
      .unwrap()
      .unwrap();

    assert_eq!(commands.len(), 102);
    assert!(
      commands[..100]
        .iter()
        .all(|c| c["command"] == serde_json::json!(["get_property", "aid"])),
      "the first hundred wire commands must be the queued batch"
    );
    assert_eq!(
      commands[100]["command"],
      serde_json::json!(["set_property", "pause", true]),
      "the urgent pause must be written ahead of the deferred command"
    );
    assert_eq!(
      commands[101]["command"],
      serde_json::json!(["get_property", "sid"]),
      "the deferred command is admitted only after a slot frees"
    );
  }

  #[tokio::test]
  async fn close_releases_a_command_waiting_for_a_queue_slot() {
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (reader, writer) = tokio::io::split(client_stream);
    let (release_tx, peer) = gated_peer(server_stream);
    let ipc = MpvIpc::from_io_for_test(reader, writer).await.unwrap();

    let gate = Arc::new(Notify::new());
    ipc.enqueue_writer_barrier(Arc::clone(&gate)).unwrap();

    let mut queued = Vec::new();
    for _ in 0..100 {
      let mut command = Box::pin(ipc.send_command(MpvCommand::get_property("aid")));
      assert!(!poll_once(&mut command).await);
      queued.push(command);
    }
    let mut overflow = Box::pin(ipc.send_command(MpvCommand::get_property("sid")));
    assert!(
      !poll_once(&mut overflow).await,
      "a full queue must defer ordinary admission"
    );

    // Closing while a command waits for a slot must fail it, not hang it.
    ipc.close();
    assert!(matches!(overflow.await, Err(IpcError::Disconnected)));

    // The queued commands resolve Disconnected too; the writer never ran.
    for command in &mut queued {
      assert!(matches!(command.await, Err(IpcError::Disconnected)));
    }
    drop(queued);
    drop(ipc);
    release_tx.send(()).unwrap();
    assert!(peer.await.unwrap().is_empty());
  }

  #[test]
  fn command_trace_summary_omits_token_bearing_arguments() {
    let secret_url = "https://media.example/video?api_key=secret-token";
    let command = MpvCommand::loadfile(secret_url);

    let summary = command_trace_summary(&command);

    assert!(summary.contains("command=loadfile"));
    assert!(!summary.contains(secret_url));
    assert!(!summary.contains("secret-token"));
  }

  #[test]
  fn malformed_message_summary_omits_raw_token_bearing_input() {
    let raw = r#"{"request_id":1,"data":"api_key=secret-token""#;
    let error = serde_json::from_str::<serde_json::Value>(raw).expect_err("input is malformed");

    let summary = parse_error_log_summary(&error, raw.len());

    assert!(summary.contains(&format!("bytes={}", raw.len())));
    assert!(!summary.contains("api_key"));
    assert!(!summary.contains("secret-token"));
  }

  #[test]
  fn response_summary_omits_property_data() {
    let secret = "https://media.example/video?api_key=secret-token";
    let result = Ok(MpvResponse {
      error: "success".to_owned(),
      data: Some(serde_json::Value::String(secret.to_owned())),
      request_id: 41,
    });

    let summary = response_trace_summary(&result);

    assert_eq!(summary, "request_id=41, success=true");
    assert!(!summary.contains(secret));
    assert!(!summary.contains("secret-token"));
  }
}
