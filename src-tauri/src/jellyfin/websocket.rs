//! WebSocket handler for Jellyfin remote control.

use futures_util::{SinkExt, StreamExt};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

use super::error::JellyfinError;
use super::types::*;

/// Commands that can be received from Jellyfin.
#[derive(Debug, Clone)]
pub enum JellyfinCommand {
  /// Play media items.
  Play(PlayRequest),
  /// Playstate command (pause, unpause, seek, stop).
  Playstate(PlaystateRequest),
  /// General command (volume, mute, etc.).
  GeneralCommand(GeneralCommand),
}

/// Stream events emitted by the restartable Jellyfin WebSocket command stream.
#[derive(Debug, Clone)]
pub enum JellyfinWebSocketEvent {
  /// Initial socket connection has been established.
  Connected,
  /// The active socket was lost. A reconnect may follow unless shutdown was requested.
  ConnectionLost,
  /// A lost socket has reconnected successfully.
  Reconnected,
  /// A Jellyfin command received from the active socket.
  Command(JellyfinCommand),
}

/// Internal state for the command stream receiver.
struct ChannelState {
  event_tx: Option<mpsc::Sender<JellyfinWebSocketEvent>>,
  event_rx: Option<mpsc::Receiver<JellyfinWebSocketEvent>>,
}

/// WebSocket connection to Jellyfin server.
pub struct JellyfinWebSocket {
  channel: Arc<RwLock<ChannelState>>,
  connected: Arc<RwLock<bool>>,
  cancel_token: Arc<RwLock<Option<CancellationToken>>>,
  task_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl JellyfinWebSocket {
  /// Create a new WebSocket handler.
  pub fn new() -> Self {
    let (event_tx, event_rx) = mpsc::channel(32);
    Self {
      channel: Arc::new(RwLock::new(ChannelState {
        event_tx: Some(event_tx),
        event_rx: Some(event_rx),
      })),
      connected: Arc::new(RwLock::new(false)),
      cancel_token: Arc::new(RwLock::new(None)),
      task_handle: Arc::new(RwLock::new(None)),
    }
  }

  /// Connect to Jellyfin WebSocket and own reconnects until explicit shutdown.
  pub async fn connect(&self, url: &str) -> Result<(), JellyfinError> {
    self.stop_task(false).await;

    let Some(event_tx) = self.channel.read().event_tx.clone() else {
      return Err(JellyfinError::NotConnected);
    };

    let cancel_token = CancellationToken::new();
    *self.cancel_token.write() = Some(cancel_token.clone());

    let connected = self.connected.clone();
    let url = url.to_string();
    let (initial_tx, initial_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
      Self::run_command_stream(url, event_tx, connected, cancel_token, Some(initial_tx)).await;
    });
    *self.task_handle.write() = Some(handle);

    initial_rx.await.unwrap_or(Err(JellyfinError::NotConnected))
  }

  async fn run_command_stream(
    url: String,
    event_tx: mpsc::Sender<JellyfinWebSocketEvent>,
    connected: Arc<RwLock<bool>>,
    cancel_token: CancellationToken,
    mut initial_tx: Option<oneshot::Sender<Result<(), JellyfinError>>>,
  ) {
    let mut reconnect_attempt = 0usize;
    let mut has_connected = false;

    loop {
      if cancel_token.is_cancelled() {
        break;
      }

      let connection = tokio::select! {
        _ = cancel_token.cancelled() => break,
        connection = connect_async(&url) => connection,
      };

      let (ws_stream, _) = match connection {
        Ok(connection) => connection,
        Err(error) => {
          *connected.write() = false;
          if let Some(initial_tx) = initial_tx.take() {
            let _ = initial_tx.send(Err(error.into()));
            break;
          }
          log::error!("WebSocket reconnection failed: {}", error);
          let delay = reconnect_delay(reconnect_attempt);
          reconnect_attempt = reconnect_attempt.saturating_add(1);
          if wait_for_reconnect_delay(delay, &cancel_token).await {
            break;
          }
          continue;
        }
      };

      *connected.write() = true;
      reconnect_attempt = 0;
      if has_connected {
        if Self::send_event(
          &event_tx,
          JellyfinWebSocketEvent::Reconnected,
          &cancel_token,
        )
        .await
        {
          break;
        }
      } else {
        has_connected = true;
        if let Some(initial_tx) = initial_tx.take() {
          let _ = initial_tx.send(Ok(()));
        }
        if Self::send_event(&event_tx, JellyfinWebSocketEvent::Connected, &cancel_token).await {
          break;
        }
      }

      let lost = Self::run_socket(ws_stream, &event_tx, &cancel_token).await;
      *connected.write() = false;

      if !lost || cancel_token.is_cancelled() {
        break;
      }

      if Self::send_event(
        &event_tx,
        JellyfinWebSocketEvent::ConnectionLost,
        &cancel_token,
      )
      .await
      {
        break;
      }
      let delay = reconnect_delay(reconnect_attempt);
      reconnect_attempt = reconnect_attempt.saturating_add(1);
      log::info!(
        "Attempting WebSocket reconnection in {} seconds (attempt {})",
        delay.as_secs(),
        reconnect_attempt
      );
      if wait_for_reconnect_delay(delay, &cancel_token).await {
        break;
      }
    }

    *connected.write() = false;
  }

  async fn run_socket<S>(
    ws_stream: tokio_tungstenite::WebSocketStream<S>,
    event_tx: &mpsc::Sender<JellyfinWebSocketEvent>,
    cancel_token: &CancellationToken,
  ) -> bool
  where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
  {
    let (mut write, mut read) = ws_stream.split();

    // "1000,1000" tells Jellyfin we are an active remote-control client.
    let session_start = serde_json::json!({
      "MessageType": "SessionsStart",
      "Data": "1000,1000"
    });
    if let Err(e) = write
      .send(Message::Text(session_start.to_string().into()))
      .await
    {
      log::error!("Failed to send SessionsStart: {}", e);
      return true;
    }

    let mut keepalive_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
      tokio::select! {
        _ = cancel_token.cancelled() => {
          log::info!("WebSocket shutdown requested via cancellation");
          let _ = write.close().await;
          return false;
        }
        msg = read.next() => {
          match msg {
            Some(Ok(Message::Text(text))) => {
              if let Err(e) = Self::handle_socket_message(&text, event_tx, cancel_token).await {
                log::error!("Failed to handle WebSocket message: {}", e);
              }
            }
            Some(Ok(Message::Close(_))) => {
              log::info!("WebSocket closed by server");
              return true;
            }
            Some(Err(e)) => {
              log::error!("WebSocket error: {}", e);
              return true;
            }
            None => {
              log::info!("WebSocket stream ended");
              return true;
            }
            _ => {}
          }
        }
        _ = keepalive_interval.tick() => {
          let keepalive = serde_json::json!({
            "MessageType": "KeepAlive"
          });
          if let Err(e) = write.send(Message::Text(keepalive.to_string().into())).await {
            log::error!("Failed to send keepalive: {}", e);
            return true;
          }
        }
      }
    }
  }

  async fn send_event(
    event_tx: &mpsc::Sender<JellyfinWebSocketEvent>,
    event: JellyfinWebSocketEvent,
    cancel_token: &CancellationToken,
  ) -> bool {
    if cancel_token.is_cancelled() {
      return true;
    }

    tokio::select! {
      biased;
      _ = cancel_token.cancelled() => true,
      result = event_tx.send(event) => result.is_err(),
    }
  }

  async fn handle_socket_message(
    text: &str,
    event_tx: &mpsc::Sender<JellyfinWebSocketEvent>,
    cancel_token: &CancellationToken,
  ) -> Result<(), JellyfinError> {
    if let Some(command) = Self::parse_message(text)? {
      let _ = Self::send_event(
        event_tx,
        JellyfinWebSocketEvent::Command(command),
        cancel_token,
      )
      .await;
    }

    Ok(())
  }

  #[cfg(test)]
  /// Handle incoming WebSocket message.
  async fn handle_message(
    text: &str,
    event_tx: &mpsc::Sender<JellyfinWebSocketEvent>,
  ) -> Result<(), JellyfinError> {
    if let Some(command) = Self::parse_message(text)? {
      let _ = event_tx
        .send(JellyfinWebSocketEvent::Command(command))
        .await;
    }

    Ok(())
  }

  fn parse_message(text: &str) -> Result<Option<JellyfinCommand>, JellyfinError> {
    let msg: WsMessage = serde_json::from_str(text)?;

    match msg.message_type.as_str() {
      "Play" => {
        if let Some(data) = msg.data {
          let play_request: PlayRequest = serde_json::from_value(data)?;
          log::info!("Received Play command: {:?}", play_request);
          Ok(Some(JellyfinCommand::Play(play_request)))
        } else {
          Ok(None)
        }
      }
      "Playstate" => {
        if let Some(data) = msg.data {
          let playstate: PlaystateRequest = serde_json::from_value(data)?;
          log::info!("Received Playstate command: {:?}", playstate);
          Ok(Some(JellyfinCommand::Playstate(playstate)))
        } else {
          Ok(None)
        }
      }
      "GeneralCommand" => {
        if let Some(data) = msg.data {
          let command: GeneralCommand = serde_json::from_value(data)?;
          log::info!("Received GeneralCommand: {:?}", command);
          Ok(Some(JellyfinCommand::GeneralCommand(command)))
        } else {
          Ok(None)
        }
      }
      "ForceKeepAlive" | "KeepAlive" => Ok(None),
      _ => {
        log::debug!("Unhandled WebSocket message type: {}", msg.message_type);
        Ok(None)
      }
    }
  }

  async fn stop_task(&self, close_delivery: bool) {
    if let Some(token) = self.cancel_token.write().take() {
      token.cancel();
    }

    let handle = self.task_handle.write().take();
    if let Some(handle) = handle {
      let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

    *self.connected.write() = false;

    if close_delivery {
      self.channel.write().event_tx.take();
    }
  }

  /// Disconnect from WebSocket and close command delivery for this stream.
  pub async fn disconnect(&self) {
    self.stop_task(true).await;
  }

  /// Check if connected.
  #[allow(dead_code)]
  pub fn is_connected(&self) -> bool {
    *self.connected.read()
  }

  /// Take the restartable command stream receiver.
  pub fn take_event_receiver(&self) -> Option<mpsc::Receiver<JellyfinWebSocketEvent>> {
    self.channel.write().event_rx.take()
  }
}

fn reconnect_delay(attempt: usize) -> Duration {
  #[cfg(not(test))]
  const RECONNECT_DELAYS: &[u64] = &[1, 2, 5, 10, 30, 60];
  #[cfg(test)]
  const TEST_RECONNECT_DELAYS: &[u64] = &[0, 0, 0, 0, 0, 0];

  #[cfg(test)]
  let delays = TEST_RECONNECT_DELAYS;
  #[cfg(not(test))]
  let delays = RECONNECT_DELAYS;

  Duration::from_secs(delays[attempt.min(delays.len() - 1)])
}

async fn wait_for_reconnect_delay(delay: Duration, cancel_token: &CancellationToken) -> bool {
  tokio::select! {
    _ = cancel_token.cancelled() => true,
    _ = tokio::time::sleep(delay) => false,
  }
}

impl Default for JellyfinWebSocket {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::net::{TcpListener, TcpStream};
  use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

  async fn send_text(stream: &mut WebSocketStream<TcpStream>, value: serde_json::Value) {
    stream
      .send(Message::Text(value.to_string().into()))
      .await
      .expect("send websocket message");
  }

  async fn expect_sessions_start(stream: &mut WebSocketStream<TcpStream>) {
    let Some(Ok(Message::Text(text))) = stream.next().await else {
      panic!("expected SessionsStart");
    };
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(value["MessageType"], "SessionsStart");
  }

  async fn next_event(rx: &mut mpsc::Receiver<JellyfinWebSocketEvent>) -> JellyfinWebSocketEvent {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
      .await
      .expect("event before timeout")
      .expect("stream still open")
  }

  #[tokio::test]
  async fn command_delivery_preserves_supported_messages_and_skips_bad_messages() {
    let (event_tx, mut event_rx) = mpsc::channel(8);

    JellyfinWebSocket::handle_message(
      r#"{"MessageType":"Play","Data":{"ItemIds":["item-1"],"PlayCommand":"PlayNow"}}"#,
      &event_tx,
    )
    .await
    .expect("play message handled");
    JellyfinWebSocket::handle_message(
      r#"{"MessageType":"Unsupported","Data":{"Name":"ignored"}}"#,
      &event_tx,
    )
    .await
    .expect("unsupported message skipped");
    assert!(JellyfinWebSocket::handle_message(
      r#"{"MessageType":"Playstate","Data":{"Command":42}}"#,
      &event_tx,
    )
    .await
    .is_err());
    JellyfinWebSocket::handle_message(
      r#"{"MessageType":"Playstate","Data":{"Command":"Pause"}}"#,
      &event_tx,
    )
    .await
    .expect("playstate message handled");
    JellyfinWebSocket::handle_message(
      r#"{"MessageType":"GeneralCommand","Data":{"Name":"SetVolume","Arguments":{"Volume":"50"}}}"#,
      &event_tx,
    )
    .await
    .expect("general command handled");

    match next_event(&mut event_rx).await {
      JellyfinWebSocketEvent::Command(JellyfinCommand::Play(request)) => {
        assert_eq!(request.item_ids, ["item-1"]);
        assert_eq!(request.play_command, "PlayNow");
      }
      event => panic!("unexpected event: {event:?}"),
    }
    match next_event(&mut event_rx).await {
      JellyfinWebSocketEvent::Command(JellyfinCommand::Playstate(request)) => {
        assert_eq!(request.command, "Pause");
      }
      event => panic!("unexpected event: {event:?}"),
    }
    match next_event(&mut event_rx).await {
      JellyfinWebSocketEvent::Command(JellyfinCommand::GeneralCommand(command)) => {
        assert_eq!(command.name, "SetVolume");
      }
      event => panic!("unexpected event: {event:?}"),
    }
    assert!(event_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn command_stream_reconnects_and_delivers_lifecycle_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("ws://{}", listener.local_addr().expect("addr"));

    let server = tokio::spawn(async move {
      let (first_socket, _) = listener.accept().await.expect("first accept");
      let mut first = accept_async(first_socket).await.expect("first websocket");
      expect_sessions_start(&mut first).await;
      send_text(
        &mut first,
        serde_json::json!({
          "MessageType": "Play",
          "Data": { "ItemIds": ["item-1"], "PlayCommand": "PlayNow" }
        }),
      )
      .await;
      first.close(None).await.expect("close first");

      let (second_socket, _) = listener.accept().await.expect("second accept");
      let mut second = accept_async(second_socket).await.expect("second websocket");
      expect_sessions_start(&mut second).await;
      send_text(
        &mut second,
        serde_json::json!({
          "MessageType": "Playstate",
          "Data": { "Command": "Pause" }
        }),
      )
      .await;
      send_text(
        &mut second,
        serde_json::json!({
          "MessageType": "GeneralCommand",
          "Data": { "Name": "Mute" }
        }),
      )
      .await;
      second.next().await;
    });

    let websocket = JellyfinWebSocket::new();
    let mut rx = websocket.take_event_receiver().expect("event receiver");
    websocket.connect(&url).await.expect("initial connect");

    assert!(matches!(
      next_event(&mut rx).await,
      JellyfinWebSocketEvent::Connected
    ));
    assert!(matches!(
      next_event(&mut rx).await,
      JellyfinWebSocketEvent::Command(JellyfinCommand::Play(_))
    ));
    assert!(matches!(
      next_event(&mut rx).await,
      JellyfinWebSocketEvent::ConnectionLost
    ));
    assert!(matches!(
      next_event(&mut rx).await,
      JellyfinWebSocketEvent::Reconnected
    ));
    assert!(matches!(
      next_event(&mut rx).await,
      JellyfinWebSocketEvent::Command(JellyfinCommand::Playstate(_))
    ));
    assert!(matches!(
      next_event(&mut rx).await,
      JellyfinWebSocketEvent::Command(JellyfinCommand::GeneralCommand(_))
    ));

    websocket.disconnect().await;
    server.await.expect("server done");
    assert!(!websocket.is_connected());
  }

  #[tokio::test]
  async fn explicit_shutdown_does_not_schedule_reconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("ws://{}", listener.local_addr().expect("addr"));

    let server = tokio::spawn(async move {
      let (socket, _) = listener.accept().await.expect("accept");
      let mut stream = accept_async(socket).await.expect("websocket");
      expect_sessions_start(&mut stream).await;
      stream.next().await;
    });

    let websocket = JellyfinWebSocket::new();
    let mut rx = websocket.take_event_receiver().expect("event receiver");
    websocket.connect(&url).await.expect("connect");
    assert!(matches!(
      next_event(&mut rx).await,
      JellyfinWebSocketEvent::Connected
    ));

    websocket.disconnect().await;
    server.await.expect("server done");
    assert!(!websocket.is_connected());
    assert!(rx.recv().await.is_none());
  }
}
