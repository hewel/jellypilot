//! High-level MPV client with command methods.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_channel::Receiver;
use parking_lot::Mutex;
use thiserror::Error;
use tokio::process::Child;
use tokio::sync::Mutex as AsyncMutex;

use super::ipc::{IpcError, MpvIpc};
use super::process::{cleanup_ipc, ipc_path, spawn_mpv, ProcessError};
use super::protocol::{MpvCommand, MpvEvent, MpvResponse, PropertyValue};

#[derive(Error, Debug)]
pub enum MpvError {
  #[error("MPV executable not found")]
  ExecutableNotFound,
  #[error("Failed to spawn MPV: {0}")]
  SpawnFailed(#[source] std::io::Error),
  #[error("MPV IPC connection failed: {0}")]
  IpcConnectionFailed(String),
  #[error("MPV IPC write failed: {0}")]
  IpcWriteFailed(#[source] std::io::Error),
  #[error("MPV IPC command timed out")]
  IpcTimeout,
  #[error("MPV IPC disconnected")]
  IpcDisconnected,
  #[error("MPV command failed")]
  CommandFailed,
  #[error("Not connected")]
  NotConnected,
  #[error("MPV is already running or starting")]
  AlreadyRunning,
}

impl From<ProcessError> for MpvError {
  fn from(error: ProcessError) -> Self {
    match error {
      ProcessError::NotFound => Self::ExecutableNotFound,
      ProcessError::SpawnFailed(error) => Self::SpawnFailed(error),
    }
  }
}

impl From<IpcError> for MpvError {
  fn from(error: IpcError) -> Self {
    match error {
      IpcError::ConnectionFailed(message) => Self::IpcConnectionFailed(message),
      IpcError::WriteFailed(error) => Self::IpcWriteFailed(error),
      IpcError::Timeout => Self::IpcTimeout,
      IpcError::Disconnected => Self::IpcDisconnected,
    }
  }
}

#[doc(hidden)]
pub fn has_mpv_option(configured_args: &[String], option_name: &str) -> bool {
  configured_args.iter().any(|arg| {
    let Some(raw) = arg.trim().strip_prefix("--") else {
      return false;
    };
    let normalized = raw.strip_prefix("no-").unwrap_or(raw);
    let configured_name = normalized
      .split(|character: char| character == '=' || character.is_ascii_whitespace())
      .next()
      .unwrap_or(normalized);
    configured_name == option_name
  })
}

fn mpv_spawn_args(configured_args: &[String], demuxer_cache_dir: Option<&Path>) -> Vec<String> {
  let mut args =
    Vec::with_capacity(configured_args.len() + usize::from(demuxer_cache_dir.is_some()));
  if let Some(cache_dir) =
    demuxer_cache_dir.filter(|_| !has_mpv_option(configured_args, "demuxer-cache-dir"))
  {
    args.push(format!("--demuxer-cache-dir={}", cache_dir.display()));
  }
  args.extend_from_slice(configured_args);
  args
}

fn option_log_summary(options: &[String]) -> String {
  let names = options
    .iter()
    .map(|option| {
      option
        .split_once('=')
        .map(|(name, _)| name.trim_start_matches("--"))
        .filter(|name| {
          !name.is_empty()
            && name
              .bytes()
              .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .unwrap_or("<opaque>")
    })
    .collect::<Vec<_>>()
    .join(",");
  format!("count={}, names=[{names}]", options.len())
}

/// High-level MPV client.
pub struct MpvClient {
  mpv_path: Arc<Mutex<Option<PathBuf>>>,
  extra_args: Arc<Mutex<Vec<String>>>,
  demuxer_cache_dir: Arc<Mutex<Option<PathBuf>>>,
  runtime: Arc<Mutex<RuntimeState>>,
  lifecycle: Arc<AsyncMutex<()>>,
}

#[derive(Default)]
struct RuntimeState {
  process: Option<Child>,
  ipc: Option<Arc<MpvIpc>>,
  #[cfg(test)]
  fail_next_cleanup: bool,
}

struct StartFailure {
  source: IpcError,
  process_reaped: bool,
}

impl StartFailure {
  fn into_mpv_error(self) -> MpvError {
    if !self.process_reaped {
      log::error!("Failed MPV start left process cleanup unconfirmed");
    }
    self.source.into()
  }
}

async fn terminate_child(child: &mut Child) -> bool {
  let pid = child.id();
  log::info!("Killing MPV process (pid: {pid:?})");
  if let Err(error) = child.start_kill() {
    log::warn!("Failed to signal MPV process: {error}");
  }

  match child.wait().await {
    Ok(status) => {
      log::info!("MPV process exited with: {status}");
      true
    }
    Err(error) => {
      log::error!("Failed to reap MPV process: {error}");
      false
    }
  }
}

impl MpvClient {
  /// Create a new MPV client.
  pub fn new(mpv_path: Option<PathBuf>) -> Self {
    Self {
      mpv_path: Arc::new(Mutex::new(mpv_path)),
      extra_args: Arc::new(Mutex::new(Vec::new())),
      demuxer_cache_dir: Arc::new(Mutex::new(None)),
      runtime: Arc::new(Mutex::new(RuntimeState::default())),
      lifecycle: Arc::new(AsyncMutex::new(())),
    }
  }

  /// Update MPV path (takes effect on next start).
  pub fn set_mpv_path(&self, path: Option<PathBuf>) {
    *self.mpv_path.lock() = path;
  }

  /// Update extra MPV arguments (takes effect on next start).
  pub fn set_extra_args(&self, args: Vec<String>) {
    *self.extra_args.lock() = args;
  }

  /// Set the application cache directory for MPV's temporary demuxer cache files.
  pub fn set_demuxer_cache_dir(&self, path: PathBuf) {
    *self.demuxer_cache_dir.lock() = Some(path);
  }

  /// Start MPV and connect to IPC.
  pub async fn start(&self) -> Result<(), MpvError> {
    let _lifecycle = self.lifecycle.lock().await;
    self.refresh_runtime();
    {
      let runtime = self.runtime.lock();
      if runtime.process.is_some() || runtime.ipc.is_some() {
        return Err(MpvError::AlreadyRunning);
      }
    }

    cleanup_ipc();

    let mpv_path = self.mpv_path.lock().clone();
    let configured_args = self.extra_args.lock().clone();
    let demuxer_cache_dir = self.demuxer_cache_dir.lock().clone();
    let spawn_args = mpv_spawn_args(&configured_args, demuxer_cache_dir.as_deref());

    let child = spawn_mpv(mpv_path.as_ref(), &spawn_args).map_err(MpvError::from)?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let ipc_result = MpvIpc::connect(&ipc_path(), 10).await;
    self
      .finish_start(child, ipc_result)
      .await
      .map_err(StartFailure::into_mpv_error)?;

    log::info!("MPV client connected");
    Ok(())
  }

  async fn finish_start(
    &self,
    mut child: Child,
    ipc_result: Result<MpvIpc, IpcError>,
  ) -> Result<(), StartFailure> {
    match ipc_result {
      Ok(ipc) => {
        let mut runtime = self.runtime.lock();
        runtime.process = Some(child);
        runtime.ipc = Some(Arc::new(ipc));
        Ok(())
      }
      Err(source) => {
        let process_reaped = terminate_child(&mut child).await;
        cleanup_ipc();
        Err(StartFailure {
          source,
          process_reaped,
        })
      }
    }
  }

  /// Stop MPV and disconnect.
  pub async fn stop(&self) {
    let _ = self.stop_and_confirm_cleanup().await;
  }

  async fn stop_and_confirm_cleanup(&self) -> bool {
    let _lifecycle = self.lifecycle.lock().await;
    log::info!("stop() called - closing IPC connection");
    let (ipc, mut child, cleanup_failure_injected) = {
      let mut runtime = self.runtime.lock();
      #[cfg(test)]
      let cleanup_failure_injected = std::mem::take(&mut runtime.fail_next_cleanup);
      #[cfg(not(test))]
      let cleanup_failure_injected = false;
      (
        runtime.ipc.take(),
        runtime.process.take(),
        cleanup_failure_injected,
      )
    };

    if let Some(ipc) = ipc {
      log::info!("Closing IPC connection");
      ipc.close();
    } else {
      log::warn!("No IPC connection to close");
    }

    let process_reaped = if cleanup_failure_injected {
      false
    } else if let Some(child) = child.as_mut() {
      terminate_child(child).await
    } else {
      log::warn!("No MPV process handle to kill");
      true
    };

    if !process_reaped {
      if let Some(child) = child {
        self.runtime.lock().process = Some(child);
      }
      log::error!("MPV process cleanup is incomplete; retaining process handle for retry");
    }

    cleanup_ipc();
    if process_reaped {
      log::info!("MPV client stopped");
    }
    process_reaped
  }

  fn refresh_runtime(&self) {
    let mut runtime = self.runtime.lock();
    if runtime.ipc.as_ref().is_some_and(|ipc| ipc.is_closed()) {
      log::info!("Dropping closed MPV IPC connection");
      if let Some(ipc) = runtime.ipc.take() {
        ipc.close();
      }
    }

    let Some(child) = runtime.process.as_mut() else {
      return;
    };

    match child.try_wait() {
      Ok(Some(status)) => {
        log::info!("Observed MPV process exit: {}", status);
        runtime.process = None;
      }
      Ok(None) => {}
      Err(e) => {
        log::warn!("Failed to check MPV process status: {}", e);
      }
    }
  }

  /// Check if connected.
  pub fn is_connected(&self) -> bool {
    self.refresh_runtime();

    let runtime = self.runtime.lock();
    let connected = runtime.ipc.is_some();
    let has_process = runtime.process.is_some();
    log::debug!(
      "is_connected check: ipc={}, process={}",
      connected,
      has_process
    );
    connected
  }

  /// Get a clone of the IPC connection.
  fn get_ipc(&self) -> Result<Arc<MpvIpc>, MpvError> {
    self
      .runtime
      .lock()
      .ipc
      .clone()
      .ok_or(MpvError::NotConnected)
  }

  /// Send a command to MPV.
  async fn send(&self, cmd: MpvCommand) -> Result<MpvResponse, MpvError> {
    let ipc = self.get_ipc()?;
    let response = ipc.send_command(cmd).await?;

    if !response.is_success() {
      return Err(MpvError::CommandFailed);
    }

    Ok(response)
  }

  /// Load a file for playback.
  pub async fn loadfile(&self, url: &str) -> Result<(), MpvError> {
    log::info!("Loading media URL");
    self.send(MpvCommand::loadfile(url)).await?;
    Ok(())
  }

  /// Load a file for playback with options.
  /// Options like start position, audio/subtitle track are applied atomically with the file load.
  pub async fn loadfile_with_options(
    &self,
    url: &str,
    start: Option<f64>,
    audio_index: Option<i64>,
    subtitle_index: Option<i64>,
    file_options: Vec<String>,
  ) -> Result<(), MpvError> {
    let mut options = Vec::new();

    if let Some(start) = start {
      if start > 0.0 {
        options.push(format!("start={}", start));
      }
    }

    if let Some(aid) = audio_index {
      options.push(format!("aid={}", aid));
    }

    match subtitle_index {
      Some(-1) => {
        // Disable subtitles
        options.push("sid=no".to_string());
      }
      Some(sid) => {
        options.push(format!("sid={}", sid));
      }
      None => {}
    }

    options.extend(file_options);

    if options.is_empty() {
      log::info!("Loading media URL");
      self.send(MpvCommand::loadfile(url)).await?;
    } else {
      let options_str = options.join(",");
      log::info!(
        "Loading media URL with options: {}",
        option_log_summary(&options)
      );
      self
        .send(MpvCommand::loadfile_with_options(url, &options_str))
        .await?;
    }

    Ok(())
  }

  /// Seek to absolute position in seconds.
  pub async fn seek(&self, time: f64) -> Result<(), MpvError> {
    self.send(MpvCommand::seek(time)).await?;
    Ok(())
  }

  /// Show text on MPV's on-screen display.
  pub async fn show_text(&self, text: &str, duration_ms: i64) -> Result<(), MpvError> {
    self.send(MpvCommand::show_text(text, duration_ms)).await?;
    Ok(())
  }

  /// Set pause state.
  pub async fn set_pause(&self, paused: bool) -> Result<(), MpvError> {
    self.send(MpvCommand::set_pause(paused)).await?;
    Ok(())
  }

  /// Set volume (0-100).
  pub async fn set_volume(&self, volume: f64) -> Result<(), MpvError> {
    self.send(MpvCommand::set_volume(volume)).await?;
    Ok(())
  }

  /// Set mute state.
  pub async fn set_mute(&self, muted: bool) -> Result<(), MpvError> {
    self.send(MpvCommand::set_mute(muted)).await?;
    Ok(())
  }

  /// Set fullscreen state.
  pub async fn set_fullscreen(&self, fullscreen: bool) -> Result<(), MpvError> {
    self.send(MpvCommand::set_fullscreen(fullscreen)).await?;
    Ok(())
  }

  /// Set audio track by ID.
  pub async fn set_audio_track(&self, id: i64) -> Result<(), MpvError> {
    self.send(MpvCommand::set_audio_track(id)).await?;
    Ok(())
  }

  /// Set subtitle track by ID.
  pub async fn set_subtitle_track(&self, id: i64) -> Result<(), MpvError> {
    self.send(MpvCommand::set_subtitle_track(id)).await?;
    Ok(())
  }

  /// Get a property value.
  pub async fn get_property(&self, name: &str) -> Result<PropertyValue, MpvError> {
    let response = self.send(MpvCommand::get_property(name)).await?;
    Ok(
      response
        .data
        .map(PropertyValue::from)
        .unwrap_or(PropertyValue::Null),
    )
  }

  /// Get current time position in seconds.
  #[allow(dead_code)]
  pub async fn get_time_pos(&self) -> Result<f64, MpvError> {
    match self.get_property("time-pos").await? {
      PropertyValue::Number(n) => Ok(n),
      _ => Ok(0.0),
    }
  }

  /// Get current pause state.
  pub async fn get_pause(&self) -> Result<bool, MpvError> {
    match self.get_property("pause").await? {
      PropertyValue::Bool(b) => Ok(b),
      _ => Ok(true),
    }
  }

  /// Get current volume (0-100).
  #[allow(dead_code)]
  pub async fn get_volume(&self) -> Result<f64, MpvError> {
    match self.get_property("volume").await? {
      PropertyValue::Number(n) => Ok(n),
      _ => Ok(100.0),
    }
  }

  /// Get current mute state.
  #[allow(dead_code)]
  pub async fn get_mute(&self) -> Result<bool, MpvError> {
    match self.get_property("mute").await? {
      PropertyValue::Bool(b) => Ok(b),
      _ => Ok(false),
    }
  }

  /// Toggle mute state.
  pub async fn toggle_mute(&self) -> Result<(), MpvError> {
    self.send(MpvCommand::cycle("mute")).await?;
    Ok(())
  }

  /// Toggle fullscreen state.
  pub async fn toggle_fullscreen(&self) -> Result<(), MpvError> {
    self.send(MpvCommand::cycle("fullscreen")).await?;
    Ok(())
  }

  /// Set a string property (e.g., force-media-title).
  pub async fn set_property_string(&self, name: &str, value: &str) -> Result<(), MpvError> {
    self
      .send(MpvCommand::set_property_string(name, value))
      .await?;
    Ok(())
  }

  /// Disable a track (set sid/aid to "no").
  pub async fn disable_track(&self, property: &str) -> Result<(), MpvError> {
    self.send(MpvCommand::disable_track(property)).await?;
    Ok(())
  }

  /// Add an external subtitle file and optionally select it.
  ///
  /// When `select` is true, the subtitle is immediately selected after loading.
  pub async fn sub_add(
    &self,
    url: &str,
    select: bool,
    title: Option<&str>,
    language: Option<&str>,
  ) -> Result<(), MpvError> {
    log::info!("Adding external subtitle (select={select})");
    let flags = if select { "select" } else { "auto" };
    self
      .send(MpvCommand::sub_add(url, flags, title, language))
      .await?;
    Ok(())
  }

  /// Quit MPV gracefully.
  pub async fn quit(&self) -> Result<(), MpvError> {
    let result = self.send(MpvCommand::quit()).await.map(|_| ());
    self.stop().await;
    result
  }

  /// Attempt graceful quit and confirm that every owned process was reaped.
  ///
  /// The IPC command result is deliberately advisory: the local process
  /// cleanup establishes whether ownership can safely cross a handoff.
  pub(crate) async fn quit_and_confirm_cleanup(&self) -> bool {
    let _ = self.send(MpvCommand::quit()).await;
    self.stop_and_confirm_cleanup().await
  }

  /// Observe a property for changes.
  /// Returns events via the events() receiver with event="property-change".
  pub async fn observe_property(&self, observer_id: i64, property: &str) -> Result<(), MpvError> {
    self
      .send(MpvCommand::observe_property(observer_id, property))
      .await?;
    Ok(())
  }

  /// Get event receiver for property changes and other events.
  pub fn events(&self) -> Option<Receiver<MpvEvent>> {
    self.refresh_runtime();
    self.runtime.lock().ipc.as_ref().map(|ipc| ipc.events())
  }

  /// Create a client around an in-memory transport for tests.
  #[cfg(any(test, feature = "test-utils"))]
  #[doc(hidden)]
  pub async fn from_io_for_test<R, W>(reader: R, writer: W) -> Result<Self, MpvError>
  where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
  {
    let ipc = MpvIpc::from_io_for_test(reader, writer).await?;
    let client = Self::new(None);
    client.runtime.lock().ipc = Some(Arc::new(ipc));
    Ok(client)
  }

  /// Move a test transport from another client into this client.
  #[cfg(any(test, feature = "test-utils"))]
  #[doc(hidden)]
  pub fn install_ipc_for_test(&self, transport: Self) {
    let ipc = transport.runtime.lock().ipc.take();
    self.runtime.lock().ipc = ipc;
  }

  #[cfg(test)]
  pub(crate) fn fail_next_cleanup_for_test(&self) {
    self.runtime.lock().fail_next_cleanup = true;
  }
}

// Need to implement Clone manually because Child doesn't implement Clone
impl Clone for MpvClient {
  fn clone(&self) -> Self {
    Self {
      mpv_path: self.mpv_path.clone(),
      extra_args: self.extra_args.clone(),
      demuxer_cache_dir: self.demuxer_cache_dir.clone(),
      runtime: self.runtime.clone(),
      lifecycle: self.lifecycle.clone(),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::process::Stdio;
  use std::time::Duration;

  use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};
  use tokio::process::Command;

  use super::*;

  #[test]
  fn spawn_args_use_app_cache_dir_when_user_has_no_override() {
    assert_eq!(
      mpv_spawn_args(&["--fullscreen".to_string()], Some(Path::new("app-cache"))),
      vec![
        "--demuxer-cache-dir=app-cache".to_string(),
        "--fullscreen".to_string(),
      ]
    );
  }

  #[test]
  fn spawn_args_preserve_explicit_user_cache_dir() {
    assert_eq!(
      mpv_spawn_args(
        &["--demuxer-cache-dir=user-cache".to_string()],
        Some(Path::new("app-cache")),
      ),
      vec!["--demuxer-cache-dir=user-cache".to_string()]
    );
  }

  #[test]
  fn option_log_summary_omits_token_bearing_values() {
    let options = vec![
      "http-header-fields=Authorization: Bearer secret-token".to_owned(),
      "demuxer-cache-dir=/secret/cache/path".to_owned(),
      "token-without-a-name".to_owned(),
    ];

    let summary = option_log_summary(&options);

    assert_eq!(
      summary,
      "count=3, names=[http-header-fields,demuxer-cache-dir,<opaque>]"
    );
    assert!(!summary.contains("secret-token"));
    assert!(!summary.contains("/secret/cache/path"));
  }

  fn lifecycle_test_child() -> Child {
    let executable = std::env::current_exe().expect("test executable path");
    let mut command = Command::new(executable);
    command
      .args([
        "--ignored",
        "--exact",
        "client::tests::lifecycle_child_process",
      ])
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .kill_on_drop(true);
    command.spawn().expect("controlled lifecycle child")
  }

  async fn client_with_command_peer(
    response_error: &'static str,
  ) -> (MpvClient, tokio::task::JoinHandle<()>) {
    let (client_stream, peer_stream) = duplex(1024);
    let (reader, writer) = tokio::io::split(client_stream);
    let client = MpvClient::from_io_for_test(reader, writer)
      .await
      .expect("test client should be constructed");
    let (peer_reader, mut peer_writer) = tokio::io::split(peer_stream);
    let peer = tokio::spawn(async move {
      let mut lines = BufReader::new(peer_reader).lines();
      let command = lines
        .next_line()
        .await
        .expect("test peer should read the command")
        .expect("test peer should receive a command");
      let request_id = serde_json::from_str::<serde_json::Value>(&command)
        .expect("command should be valid JSON")
        .get("request_id")
        .and_then(serde_json::Value::as_i64)
        .expect("command should contain a request ID");
      let response = serde_json::json!({
        "error": response_error,
        "data": null,
        "request_id": request_id,
      });
      peer_writer
        .write_all(format!("{response}\n").as_bytes())
        .await
        .expect("test peer should write the response");
    });

    (client, peer)
  }

  #[test]
  #[ignore = "helper process launched by lifecycle tests"]
  fn lifecycle_child_process() {
    std::thread::sleep(Duration::from_secs(60));
  }

  #[tokio::test]
  async fn failed_ipc_handshake_reaps_child_and_leaves_runtime_empty() {
    let client = MpvClient::new(None);
    #[cfg(not(windows))]
    let ipc_marker = std::fs::write(ipc_path(), b"stale socket marker").is_ok();

    let result = client
      .finish_start(
        lifecycle_test_child(),
        Err(IpcError::ConnectionFailed("test handshake failure".into())),
      )
      .await;
    let Err(failure) = result else {
      panic!("failed IPC handshake must not commit the child");
    };

    assert!(failure.process_reaped);
    let runtime = client.runtime.lock();
    assert!(runtime.process.is_none());
    assert!(runtime.ipc.is_none());
    #[cfg(not(windows))]
    if ipc_marker {
      assert!(!Path::new(&ipc_path()).exists());
    }
  }

  #[tokio::test]
  async fn repeated_start_rejects_running_child_without_replacing_it() {
    let client = MpvClient::new(None);
    let child = lifecycle_test_child();
    let original_pid = child.id();
    client.runtime.lock().process = Some(child);

    let error = client
      .start()
      .await
      .expect_err("running MPV must reject restart");

    assert!(matches!(error, MpvError::AlreadyRunning));
    assert_eq!(
      client.runtime.lock().process.as_ref().and_then(Child::id),
      original_pid
    );
    client.stop().await;
  }

  #[tokio::test]
  async fn quit_returns_sanitized_command_failure() {
    let (client, peer) = client_with_command_peer("failure containing secret-token").await;

    let error = client
      .quit()
      .await
      .expect_err("failed quit command should be returned");
    peer.await.expect("test peer should finish");

    assert_eq!(error.to_string(), "MPV command failed");
  }

  #[tokio::test]
  async fn quit_stops_local_process_when_command_fails() {
    let (client, peer) = client_with_command_peer("failure containing secret-token").await;
    client.runtime.lock().process = Some(lifecycle_test_child());

    let _error = client
      .quit()
      .await
      .expect_err("failed quit command should be returned after cleanup");
    peer.await.expect("test peer should finish");

    assert!(client.runtime.lock().process.is_none());
  }

  #[tokio::test]
  async fn checked_cleanup_retains_an_unreaped_process_for_retry() {
    let (client, peer) = client_with_command_peer("success").await;
    let child = lifecycle_test_child();
    let original_pid = child.id();
    client.runtime.lock().process = Some(child);
    client.fail_next_cleanup_for_test();

    let first_cleanup = client.quit_and_confirm_cleanup().await;
    peer.await.expect("test peer should finish");

    assert!(!first_cleanup);
    assert_eq!(
      client.runtime.lock().process.as_ref().and_then(Child::id),
      original_pid
    );
    assert!(client.quit_and_confirm_cleanup().await);
    assert!(client.runtime.lock().process.is_none());
  }

  #[tokio::test]
  async fn is_connected_returns_false_after_ipc_eof() {
    let (client_stream, peer_stream) = duplex(64);
    let (reader, writer) = tokio::io::split(client_stream);
    let client = MpvClient::from_io_for_test(reader, writer)
      .await
      .expect("test client should be constructed");

    drop(peer_stream);

    tokio::time::timeout(Duration::from_secs(1), async {
      while client
        .runtime
        .lock()
        .ipc
        .as_ref()
        .is_some_and(|ipc| !ipc.is_closed())
      {
        tokio::time::sleep(Duration::from_millis(10)).await;
      }
    })
    .await
    .expect("IPC reader should observe EOF");

    assert!(!client.is_connected());
  }
}
