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
use tokio::sync::oneshot;
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

/// Writer channel message.
enum WriteMessage {
  Command(Vec<u8>),
  Close,
}

/// MPV IPC connection.
pub struct MpvIpc {
  state: Arc<Mutex<IpcState>>,
  write_tx: async_channel::Sender<WriteMessage>,
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

      match Self::try_connect(path).await {
        Ok(ipc) => return Ok(ipc),
        Err(e) => {
          log::debug!("IPC connect attempt {} failed: {}", attempt + 1, e);
          last_error = Some(e);
        }
      }
    }

    Err(last_error.unwrap_or_else(|| IpcError::ConnectionFailed("Unknown error".into())))
  }

  #[cfg(windows)]
  async fn try_connect(path: &str) -> Result<Self, IpcError> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let client = ClientOptions::new()
      .open(path)
      .map_err(|e| IpcError::ConnectionFailed(format!("Failed to open pipe: {}", e)))?;

    let (reader, writer) = tokio::io::split(client);
    Self::setup(reader, writer).await
  }

  #[cfg(not(windows))]
  async fn try_connect(path: &str) -> Result<Self, IpcError> {
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(path)
      .await
      .map_err(|e| IpcError::ConnectionFailed(e.to_string()))?;

    let (reader, writer) = tokio::io::split(stream);
    Self::setup(reader, writer).await
  }

  async fn setup<R, W>(reader: R, writer: W) -> Result<Self, IpcError>
  where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
  {
    Self::setup_with_logs(
      reader,
      writer,
      jellypilot_core::player_logs::global().clone(),
    )
    .await
  }

  async fn setup_with_logs<R, W>(
    reader: R,
    writer: W,
    logs: Arc<PlayerLogs>,
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
    let (write_tx, write_rx) = async_channel::bounded::<WriteMessage>(100); // Bounded to prevent OOM

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
      event_rx,
      closed,
      reader_handle,
      writer_handle,
      log_handle: None,
      logs,
      connection_id,
    };
    let initially_enabled = ipc.logs.enabled();
    if initially_enabled {
      // Subscribe before the caller can load media when capture was pre-enabled.
      Self::update_log_subscription(
        &ipc.state,
        &ipc.write_tx,
        &ipc.closed,
        &ipc.logs,
        connection_id,
        true,
      )
      .await;
    }
    let log_state = ipc.state.clone();
    let log_write = ipc.write_tx.clone();
    let log_closed = ipc.closed.clone();
    let log_capture = ipc.logs.clone();
    ipc.log_handle = Some(tokio::spawn(async move {
      Self::log_subscription_loop(
        log_state,
        log_write,
        log_closed,
        log_capture,
        connection_id,
        initially_enabled,
      )
      .await;
    }));
    Ok(ipc)
  }

  #[cfg(any(test, feature = "test-utils"))]
  pub(crate) async fn from_io_for_test<R, W>(reader: R, writer: W) -> Result<Self, IpcError>
  where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
  {
    Self::setup(reader, writer).await
  }

  async fn reader_loop<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    state: Arc<Mutex<IpcState>>,
    event_tx: Sender<MpvEvent>,
    closed: Arc<AtomicBool>,
    logs: Arc<PlayerLogs>,
    connection_id: u64,
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
              logs.record(
                connection_id,
                &message.prefix,
                &message.level,
                &message.text,
              );
            }
            Ok(MpvMessage::Event(event)) => {
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
              } else if event.event == "queue-overflow" {
                logs.record(
                  connection_id,
                  "ipc",
                  "warn",
                  "MPV event queue overflow; player log may be incomplete",
                );
              }
              log::debug!("MPV event: {} (reason={:?})", event.event, event.reason);
              // Use try_send to avoid blocking if channel is full
              if event_tx.try_send(event).is_err() {
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
      match msg {
        WriteMessage::Command(data) => {
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
        WriteMessage::Close => {
          log::info!("MPV IPC writer closing");
          break;
        }
      }
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
    let capture_command = self.logs.enabled()
      && (matches!(cmd.command_name(), "loadfile" | "stop" | "seek" | "quit")
        || (cmd.command_name() == "set_property"
          && cmd
            .command
            .get(1)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|p| matches!(p, "aid" | "sid" | "pause"))));
    let request_id = cmd.request_id;
    if capture_command {
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
      self
        .logs
        .record(self.connection_id, "command", "info", &summary);
    }
    let result = Self::send(&self.state, &self.write_tx, &self.closed, cmd).await;
    if capture_command {
      let success = result.as_ref().is_ok_and(MpvResponse::is_success);
      self.logs.record(
        self.connection_id,
        "command",
        if success { "info" } else { "error" },
        &format!("request_id={request_id}, success={success}"),
      );
    }
    result
  }

  async fn send(
    state: &Mutex<IpcState>,
    write_tx: &Sender<WriteMessage>,
    closed: &AtomicBool,
    cmd: MpvCommand,
  ) -> Result<MpvResponse, IpcError> {
    // Early check for closed connection
    if closed.load(Ordering::Acquire) {
      return Err(IpcError::Disconnected);
    }

    let request_id = cmd.request_id;

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

    // Serialize command - if this fails, remove pending and return error
    let json = match serde_json::to_string(&cmd) {
      Ok(j) => j,
      Err(e) => {
        state.lock().pending.remove(&request_id);
        return Err(IpcError::WriteFailed(std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          e,
        )));
      }
    };

    log::trace!("Sending MPV command: {}", command_trace_summary(&cmd));

    // Send to writer task - if this fails, remove pending and return error
    if write_tx
      .send(WriteMessage::Command(json.into_bytes()))
      .await
      .is_err()
    {
      if let Some(tx) = state.lock().pending.remove(&request_id) {
        let _ = tx.send(Err(IpcError::Disconnected));
      }
      return Err(IpcError::Disconnected);
    }

    log::trace!("MPV command queued, waiting for response...");

    // Wait for response with timeout
    match tokio::time::timeout(Duration::from_secs(5), rx).await {
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
        // Timeout - remove pending request
        log::error!(
          "MPV command timeout after 5 seconds, request_id={}",
          request_id
        );
        state.lock().pending.remove(&request_id);
        Err(IpcError::Timeout)
      }
    }
  }

  async fn log_subscription_loop(
    state: Arc<Mutex<IpcState>>,
    write_tx: Sender<WriteMessage>,
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
      Self::update_log_subscription(&state, &write_tx, &closed, &logs, connection_id, enabled)
        .await;
    }
  }

  async fn update_log_subscription(
    state: &Mutex<IpcState>,
    write_tx: &Sender<WriteMessage>,
    closed: &AtomicBool,
    logs: &PlayerLogs,
    connection_id: u64,
    enabled: bool,
  ) {
    match Self::send(
      state,
      write_tx,
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
    let ipc = MpvIpc::setup_with_logs(reader, writer, logs.clone())
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
