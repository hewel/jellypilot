//! High-level MPV client with command methods.

use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use std::time::Duration;

use async_channel::Receiver;
use parking_lot::Mutex;
use thiserror::Error;

use super::ipc::{IpcError, MpvIpc};
use super::process::{cleanup_ipc, ipc_path, spawn_mpv, ProcessError};
use super::protocol::{MpvCommand, MpvEvent, MpvResponse, PropertyValue};

#[derive(Error, Debug)]
pub enum MpvError {
  #[error("Process error: {0}")]
  Process(#[from] ProcessError),
  #[error("IPC error: {0}")]
  Ipc(#[from] IpcError),
  #[error("MPV command failed: {0}")]
  CommandFailed(String),
  #[error("Not connected")]
  NotConnected,
}

/// High-level MPV client.
pub struct MpvClient {
  mpv_path: Arc<Mutex<Option<PathBuf>>>,
  extra_args: Arc<Mutex<Vec<String>>>,
  process: Arc<Mutex<Option<Child>>>,
  ipc: Arc<Mutex<Option<Arc<MpvIpc>>>>,
}

impl MpvClient {
  /// Create a new MPV client.
  pub fn new(mpv_path: Option<PathBuf>) -> Self {
    Self {
      mpv_path: Arc::new(Mutex::new(mpv_path)),
      extra_args: Arc::new(Mutex::new(Vec::new())),
      process: Arc::new(Mutex::new(None)),
      ipc: Arc::new(Mutex::new(None)),
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

  /// Start MPV and connect to IPC.
  pub async fn start(&self) -> Result<(), MpvError> {
    // Cleanup any existing socket
    cleanup_ipc();

    // Get current config
    let mpv_path = self.mpv_path.lock().clone();
    let extra_args = self.extra_args.lock().clone();

    // Spawn MPV process
    let child = spawn_mpv(mpv_path.as_ref(), &extra_args)?;
    {
      let mut process = self.process.lock();
      *process = Some(child);
    }

    // Wait a bit for MPV to create the socket
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Connect to IPC with retries
    let ipc_conn = MpvIpc::connect(&ipc_path(), 10).await?;
    {
      let mut ipc = self.ipc.lock();
      *ipc = Some(Arc::new(ipc_conn));
    }

    log::info!("MPV client connected");
    Ok(())
  }

  /// Stop MPV and disconnect.
  /// This is async to avoid blocking on process kill/wait.
  pub async fn stop(&self) {
    log::info!("stop() called - closing IPC connection");
    // Close IPC first
    {
      let mut ipc = self.ipc.lock();
      if let Some(conn) = ipc.take() {
        log::info!("Closing IPC connection");
        conn.close();
      } else {
        log::warn!("No IPC connection to close");
      }
    }

    // Kill process in spawn_blocking to avoid blocking async runtime
    let child = {
      let mut process = self.process.lock();
      process.take()
    };

    if let Some(mut child) = child {
      let pid = child.id();
      log::info!("Killing MPV process (pid: {:?})", pid);

      // Use spawn_blocking for blocking kill/wait operations
      let result = tokio::task::spawn_blocking(move || {
        let kill_result = child.kill();
        let wait_result = child.wait();
        (kill_result, wait_result)
      })
      .await;

      match result {
        Ok((kill_result, wait_result)) => {
          match kill_result {
            Ok(_) => log::info!("kill() succeeded"),
            Err(e) => log::error!("kill() failed: {}", e),
          }
          match wait_result {
            Ok(status) => log::info!("MPV process exited with: {}", status),
            Err(e) => log::error!("wait() failed: {}", e),
          }
        }
        Err(e) => {
          log::error!("spawn_blocking panicked during process cleanup: {}", e);
        }
      }
    } else {
      log::warn!("No MPV process handle to kill");
    }

    cleanup_ipc();
    log::info!("MPV client stopped");
  }

  /// Check if connected.
  pub fn is_connected(&self) -> bool {
    let connected = self.ipc.lock().is_some();
    let has_process = self.process.lock().is_some();
    log::debug!(
      "is_connected check: ipc={}, process={}",
      connected,
      has_process
    );
    connected
  }

  /// Get a clone of the IPC connection.
  fn get_ipc(&self) -> Result<Arc<MpvIpc>, MpvError> {
    let guard = self.ipc.lock();
    guard.clone().ok_or(MpvError::NotConnected)
  }

  /// Send a command to MPV.
  async fn send(&self, cmd: MpvCommand) -> Result<MpvResponse, MpvError> {
    let ipc = self.get_ipc()?;
    let response = ipc.send_command(cmd).await?;

    if !response.is_success() {
      return Err(MpvError::CommandFailed(response.error));
    }

    Ok(response)
  }

  /// Load a file for playback.
  pub async fn loadfile(&self, url: &str) -> Result<(), MpvError> {
    log::info!("Loading file: {}", url);
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

    if options.is_empty() {
      log::info!("Loading file: {}", url);
      self.send(MpvCommand::loadfile(url)).await?;
    } else {
      let options_str = options.join(",");
      log::info!("Loading file: {} with options: {}", url, options_str);
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
  pub async fn sub_add(&self, url: &str, select: bool) -> Result<(), MpvError> {
    log::info!("Adding external subtitle (select={}): {}", select, url);
    let flags = if select { Some("select") } else { None };
    self.send(MpvCommand::sub_add(url, flags)).await?;
    Ok(())
  }

  /// Quit MPV gracefully.
  pub async fn quit(&self) -> Result<(), MpvError> {
    let _ = self.send(MpvCommand::quit()).await;
    self.stop().await;
    Ok(())
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
    let guard = self.ipc.lock();
    guard.as_ref().map(|ipc| ipc.events())
  }
}

// Need to implement Clone manually because Child doesn't implement Clone
impl Clone for MpvClient {
  fn clone(&self) -> Self {
    Self {
      mpv_path: self.mpv_path.clone(),
      extra_args: self.extra_args.clone(),
      process: self.process.clone(),
      ipc: self.ipc.clone(),
    }
  }
}
