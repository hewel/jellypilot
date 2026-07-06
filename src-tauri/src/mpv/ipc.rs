//! Async IPC connection to MPV.
//!
//! Handles platform-specific socket/pipe connections.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use parking_lot::Mutex;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::protocol::{MpvCommand, MpvEvent, MpvMessage, MpvResponse};

#[derive(Error, Debug)]
pub enum IpcError {
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
}

impl MpvIpc {
  /// Connect to MPV IPC socket/pipe.
  pub async fn connect(path: &str, retry_count: u32) -> Result<Self, IpcError> {
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
    let state = Arc::new(Mutex::new(IpcState {
      pending: HashMap::new(),
    }));

    let closed = Arc::new(AtomicBool::new(false));

    let (event_tx, event_rx) = async_channel::bounded(100); // Bounded to prevent memory bloat
    let (write_tx, write_rx) = async_channel::bounded::<WriteMessage>(100); // Bounded to prevent OOM

    // Spawn reader task
    let reader_state = state.clone();
    let reader_closed = closed.clone();
    let reader_handle = tokio::spawn(async move {
      Self::reader_loop(reader, reader_state, event_tx, reader_closed).await;
    });

    // Spawn writer task - pass state and closed for error handling
    let writer_state = state.clone();
    let writer_closed = closed.clone();
    let writer_handle = tokio::spawn(async move {
      Self::writer_loop(writer, write_rx, writer_state, writer_closed).await;
    });

    Ok(Self {
      state,
      write_tx,
      event_rx,
      closed,
      reader_handle,
      writer_handle,
    })
  }

  #[cfg(test)]
  pub(super) async fn from_io_for_test<R, W>(reader: R, writer: W) -> Result<Self, IpcError>
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
            Ok(MpvMessage::Event(event)) => {
              log::debug!("MPV event: {} (reason={:?})", event.event, event.reason);
              // Use try_send to avoid blocking if channel is full
              if event_tx.try_send(event).is_err() {
                log::warn!("Event channel full, dropping event");
              }
            }
            Err(e) => {
              log::warn!("Failed to parse MPV message: {} - {}", e, trimmed);
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
  pub fn is_closed(&self) -> bool {
    self.closed.load(Ordering::Acquire)
  }

  /// Send a command to MPV and wait for response.
  pub async fn send_command(&self, cmd: MpvCommand) -> Result<MpvResponse, IpcError> {
    // Early check for closed connection
    if self.is_closed() {
      return Err(IpcError::Disconnected);
    }

    let request_id = cmd.request_id;

    // Create response channel
    let (tx, rx) = oneshot::channel();

    // Register pending request
    {
      let mut state = self.state.lock();
      state.pending.insert(request_id, tx);
    }

    // Re-check closed after inserting to handle race with close()/drain_pending()
    // If closed was set between our first check and insert, drain_pending() already ran
    // and won't drain our newly inserted pending - we'd timeout after 5s instead of
    // getting immediate Disconnected
    if self.is_closed() {
      if let Some(tx) = self.state.lock().pending.remove(&request_id) {
        let _ = tx.send(Err(IpcError::Disconnected));
      }
      return Err(IpcError::Disconnected);
    }

    // Serialize command - if this fails, remove pending and return error
    let json = match serde_json::to_string(&cmd) {
      Ok(j) => j,
      Err(e) => {
        self.state.lock().pending.remove(&request_id);
        return Err(IpcError::WriteFailed(std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          e,
        )));
      }
    };

    log::trace!("Sending MPV command: {}", json);

    // Send to writer task - if this fails, remove pending and return error
    if self
      .write_tx
      .send(WriteMessage::Command(json.into_bytes()))
      .await
      .is_err()
    {
      if let Some(tx) = self.state.lock().pending.remove(&request_id) {
        let _ = tx.send(Err(IpcError::Disconnected));
      }
      return Err(IpcError::Disconnected);
    }

    log::trace!("MPV command queued, waiting for response...");

    // Wait for response with timeout
    match tokio::time::timeout(Duration::from_secs(5), rx).await {
      Ok(Ok(result)) => {
        log::trace!("MPV response received: {:?}", result);
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
        self.state.lock().pending.remove(&request_id);
        Err(IpcError::Timeout)
      }
    }
  }

  /// Get the event receiver for property changes and other events.
  pub fn events(&self) -> Receiver<MpvEvent> {
    self.event_rx.clone()
  }

  /// Close the connection gracefully.
  /// Note: This signals shutdown but tasks may not stop immediately if blocked on I/O.
  /// Drop will abort tasks forcefully.
  pub fn close(&self) {
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

    // Drain any remaining pending requests
    self.state.lock().drain_pending();
  }
}
