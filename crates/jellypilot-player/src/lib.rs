//! Local video playback and a compositable iced video surface.
//!
//! [`Player`] owns request ordering, frame invalidation, the GStreamer worker and GPU
//! resources. Callers keep interaction state (for example a dragged timeline value),
//! not frame tokens or worker handles. The renderer requires the pinned iced fork's
//! **wgpu** backend; software rendering does not support its custom shader.
//!
//! Native GStreamer 1.28+ and decoding/audio plugins must be installed separately.
//! This is an SDR CPU-readable RGBA path, not a zero-copy or HDR contract. It does
//! not replace JellyPilot's production external MPV playback.
//!
//! Consume the notification stream and call [`Player::refresh`] on each wake. Wakes
//! coalesce; the latest status and frame are retained independently. Compose
//! [`Player::view`] beneath your own controls, then execute [`Player::close`]'s task
//! before closing the native window. Dropping a player requests stop without joining.
//!
//! ```no_run
//! use iced::Task;
//! use jellypilot_player::{AudioOutput, PlaybackError, Player};
//!
//! #[derive(Debug, Clone)]
//! enum Message { Changed, Stopped(Result<(), PlaybackError>) }
//!
//! fn boot() -> Result<(Player, Task<Message>), PlaybackError> {
//!   let (mut player, notifications) = Player::new(AudioOutput::System)?;
//!   player.open("movie.mp4")?;
//!   Ok((player, Task::run(notifications, |()| Message::Changed)))
//! }
//!
//! fn update(player: &mut Player, message: Message) -> Result<(), PlaybackError> {
//!   if let Message::Changed = message { player.refresh()?; }
//!   Ok(())
//! }
//!
//! fn before_window_close(player: &mut Player) -> Task<Message> {
//!   player.close().map(Message::Stopped)
//! }
//! ```

mod playback;
mod video;

#[cfg(test)]
mod tests;
use iced::{
  futures::{stream, Stream},
  widget::{shader, Space},
  Element,
  Length::Fill,
  Task,
};
use playback::{Command, FrameToken, PlaybackWorker, Snapshot};
use std::{path::PathBuf, sync::Arc, time::Duration};
use video::{Video, VideoSurface};

pub use playback::{PlaybackError, PlaybackPhase};

/// Audio routing for this player instance; it never changes on file replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutput {
  /// Synchronized audio through GStreamer's system audio sink.
  System,
  /// Synchronized discard sink for unattended verification; not audible playback.
  Discard,
}

/// Latest observable player state. Obtain a read-only reference with [`Player::status`].
#[derive(Debug, Clone)]
pub struct Status {
  /// Backend-observed phase, or `Closing` after a close request.
  pub phase: PlaybackPhase,
  /// Current playing intent; may precede the observed phase during transitions.
  pub playing: bool,
  /// Backend clock position, unknown until a successful query.
  pub position: Option<Duration>,
  /// Media duration, unknown rather than zero when unavailable.
  pub duration: Option<Duration>,
  /// Whether the backend supports time seeking.
  pub seekable: bool,
  /// Linear volume in 0..=1, retained across file replacements and mute changes.
  pub volume: f64,
  /// Mute state, independent of saved volume.
  pub muted: bool,
  /// Latest asynchronous error, cleared when a new open or seek is submitted.
  pub error: Option<PlaybackError>,
  /// A submitted local path is waiting for backend validation.
  pub opening: bool,
  /// A submitted seek is waiting for acceptance or its new frame.
  pub seeking: bool,
  /// Last queried appsink buffer count (configured maximum: two).
  pub queue_depth: u64,
  /// Last queried appsink drop count for the active file.
  pub dropped: u64,
}
impl Default for Status {
  fn default() -> Self {
    Self {
      phase: PlaybackPhase::Idle,
      playing: false,
      position: None,
      duration: None,
      seekable: false,
      volume: 1.0,
      muted: false,
      error: None,
      opening: false,
      seeking: false,
      queue_depth: 0,
      dropped: 0,
    }
  }
}
impl Status {
  /// Whether transport controls are available for the active media.
  pub fn ready(&self) -> bool {
    matches!(
      self.phase,
      PlaybackPhase::Playing | PlaybackPhase::Paused | PlaybackPhase::Ended
    )
  }
  /// Whether an interactive timeline can commit a seek.
  pub fn can_seek(&self) -> bool {
    self.ready() && self.seekable && self.duration.is_some_and(|duration| !duration.is_zero())
  }
}

/// A single local-video player and its iced rendering resources.
///
/// Control methods are nonblocking submissions. Immediate errors are returned;
/// asynchronous validation/decoding errors appear in [`Status::error`] on refresh.
/// Invalid paths preserve active playback. A valid replacement stops the old media
/// before loading the new one. No filesystem or playback settings are persisted.
pub struct Player {
  worker: Option<PlaybackWorker>,
  snapshot: Snapshot,
  status: Status,
  surface: Option<Arc<VideoSurface>>,
  request_sequence: u64,
  seek_sequence: u64,
  pending_open: Option<u64>,
  pending_seek: Option<FrameToken>,
  pending_play: Option<(u64, u64)>,
}
impl Player {
  /// Starts a worker and returns its coalescing change stream.
  ///
  /// Worker initialization is asynchronous; its errors are reported by refresh.
  /// The stream carries no pixels. It also wakes after a GPU frame upload.
  pub fn new(
    audio: AudioOutput,
  ) -> Result<(Self, impl Stream<Item = ()> + Send + 'static), PlaybackError> {
    let (worker, notifications) = PlaybackWorker::start(audio == AudioOutput::Discard)?;
    let notifications = stream::unfold(notifications, |mut receiver| async move {
      receiver.recv().await.map(|()| ((), receiver))
    });
    Ok((
      Self {
        worker: Some(worker),
        snapshot: Snapshot::default(),
        status: Status::default(),
        surface: None,
        request_sequence: 0,
        seek_sequence: 0,
        pending_open: None,
        pending_seek: None,
        pending_play: None,
      },
      notifications,
    ))
  }

  /// Returns the latest status, including locally submitted transport intent.
  pub fn status(&self) -> &Status {
    &self.status
  }

  /// Applies the worker's latest snapshot and updates frame/resource ownership.
  ///
  /// Call after notification. Calling more frequently is safe: an older snapshot
  /// cannot acknowledge a newer request. Asynchronous media errors are retained in
  /// status instead of being returned as failures of this synchronization operation.
  pub fn refresh(&mut self) -> Result<(), PlaybackError> {
    let Some(worker) = &self.worker else {
      return Ok(());
    };
    let snapshot = worker.backend.snapshot()?;
    if snapshot.token.generation != self.snapshot.token.generation {
      self.retire_surface()?;
      self.surface = Some(Arc::new(VideoSurface::new(snapshot.frames.clone())));
      self.pending_seek = None;
      self.pending_play = None;
      self.status.playing = snapshot.playing;
    }
    self.seek_sequence = self.seek_sequence.max(snapshot.token.seek_generation);
    if let Some((generation, result)) = &snapshot.open_result {
      if self.pending_open == Some(*generation) {
        self.pending_open = None;
        self.status.error = result.as_ref().err().cloned();
      }
    }
    if let Some((sequence, result)) = &snapshot.seek_result {
      if self.pending_seek
        == Some(FrameToken {
          generation: snapshot.token.generation,
          seek_generation: *sequence,
        })
      {
        self.pending_seek = None;
        self.status.error = result.as_ref().err().cloned();
      }
    }
    if let Some(sequence) = snapshot.play_result {
      if self.pending_play == Some((snapshot.token.generation, sequence)) {
        self.pending_play = None;
      }
    }
    if self.pending_play.is_none() {
      self.status.playing = snapshot.playing;
    }
    if let Some(error) = &snapshot.error {
      if self.pending_open.is_none()
        && (snapshot.error != self.snapshot.error
          || snapshot.token.generation != self.snapshot.token.generation)
      {
        self.status.error = Some(error.clone());
      }
      self.status.playing = false;
      self.pending_seek = None;
      self.pending_play = None;
      self.retire_surface()?;
    }
    self.status.phase = snapshot.phase;
    self.status.position = snapshot.position;
    self.status.duration = snapshot.duration;
    self.status.seekable = snapshot.seekable;
    self.status.queue_depth = snapshot.queue_depth;
    self.status.dropped = snapshot.dropped;
    self.status.opening = self.pending_open.is_some();
    self.status.seeking = self.pending_seek.is_some()
      || (self.surface.is_some()
        && snapshot.frames.latest(snapshot.token)?.is_none()
        && snapshot.token.seek_generation != 0
        && snapshot.phase != PlaybackPhase::Ended);
    self.snapshot = snapshot;
    Ok(())
  }

  /// Opens a local path asynchronously, starting valid media from the beginning.
  ///
  /// Empty paths are rejected immediately. Invalid paths keep the previous media;
  /// readable regular files that cannot decode stop it and enter `Error`.
  pub fn open(&mut self, path: impl Into<PathBuf>) -> Result<(), PlaybackError> {
    let path = path.into();
    if path.as_os_str().is_empty() {
      return Err(PlaybackError::Open {
        path,
        reason: "empty local path".into(),
      });
    }
    self.request_sequence = next_sequence(self.request_sequence, "open")?;
    self.send(Command::Open {
      generation: self.request_sequence,
      path,
    })?;
    self.pending_open = Some(self.request_sequence);
    self.status.opening = true;
    self.status.error = None;
    Ok(())
  }

  /// Sets playing intent without rebuilding the pipeline; true at EOS replays.
  pub fn set_playing(&mut self, playing: bool) -> Result<(), PlaybackError> {
    if !self.status.ready() {
      return Err(control_error("play/pause", "media is not ready"));
    }
    self.seek_sequence = next_sequence(self.seek_sequence, "play/pause")?;
    self.send(Command::SetPlaying {
      generation: self.snapshot.token.generation,
      playing,
      seek_generation: self.seek_sequence,
    })?;
    self.pending_play = Some((self.snapshot.token.generation, self.seek_sequence));
    self.status.playing = playing;
    Ok(())
  }

  /// Commits an accurate flushing seek, clamped to the known duration.
  ///
  /// Paused playback stays paused and updates the preview frame. Seeking from EOS
  /// previews while paused. Call on timeline release, not on every drag change.
  pub fn seek(&mut self, position: Duration) -> Result<(), PlaybackError> {
    if !self.status.can_seek() {
      return Err(control_error("seek", "media is not seekable"));
    }
    self.seek_sequence = next_sequence(self.seek_sequence, "seek")?;
    let position = self
      .status
      .duration
      .map_or(position, |duration| position.min(duration));
    let token = FrameToken {
      generation: self.snapshot.token.generation,
      seek_generation: self.seek_sequence,
    };
    self.send(Command::Seek {
      generation: token.generation,
      seek_generation: token.seek_generation,
      position,
    })?;
    self.pending_seek = Some(token);
    self.status.seeking = true;
    self.status.error = None;
    Ok(())
  }

  /// Sets linear volume, clamping finite values to 0..=1 without unmuting.
  pub fn set_volume(&mut self, volume: f64) -> Result<(), PlaybackError> {
    if !volume.is_finite() {
      return Err(control_error("volume", "volume must be finite"));
    }
    let volume = volume.clamp(0.0, 1.0);
    self.send(Command::SetVolume(volume))?;
    self.status.volume = volume;
    Ok(())
  }

  /// Changes mute independently of the saved volume.
  pub fn set_muted(&mut self, muted: bool) -> Result<(), PlaybackError> {
    self.send(Command::SetMuted(muted))?;
    self.status.muted = muted;
    Ok(())
  }

  /// Fills the available space with a contained video surface, or an empty widget.
  ///
  /// Compose this beneath controls in an iced stack. Layout changes never control
  /// playback. The application must explicitly select iced's wgpu backend.
  pub fn view<Message: Clone + 'static>(&self) -> Element<'_, Message> {
    match (&self.surface, &self.worker) {
      (Some(surface), Some(worker)) => shader(Video {
        surface: surface.clone(),
        token: self.displayed_token(),
        backend: worker.backend.clone(),
      })
      .width(Fill)
      .height(Fill)
      .into(),
      _ => Space::new().width(Fill).height(Fill).into(),
    }
  }

  /// Returns a sequence only after the currently visible frame was uploaded by wgpu.
  ///
  /// This is a smoke/diagnostic observation, not proof of presentation or audio output.
  /// Pending seeks never report the previous frame as their upload.
  pub fn uploaded_frame(&self) -> Result<Option<u64>, PlaybackError> {
    self
      .surface
      .as_ref()
      .map_or(Ok(None), |surface| surface.uploaded(self.displayed_token()))
  }

  /// Requests stop immediately, then joins the worker off the UI thread in this task.
  ///
  /// Execute the returned task and wait for its result before closing the window.
  /// Later calls return an empty task; the first task owns completion. No controls
  /// are accepted after this call, even if its task has not yet been polled.
  pub fn close(&mut self) -> Task<Result<(), PlaybackError>> {
    if self.status.phase == PlaybackPhase::Closing {
      return Task::none();
    }
    self.status.phase = PlaybackPhase::Closing;
    self.status.playing = false;
    self.status.opening = false;
    self.status.seeking = false;
    let retired = self.retire_surface();
    let worker = self.worker.take();
    if let Some(worker) = &worker {
      let _ = worker.backend.send(Command::Shutdown);
    }
    Task::perform(
      async move {
        let stopped = if let Some(worker) = worker {
          tokio::task::spawn_blocking(move || worker.shutdown())
            .await
            .map_err(|error| PlaybackError::Worker(error.to_string()))?
        } else {
          Ok(())
        };
        stopped.and(retired)
      },
      std::convert::identity,
    )
  }

  fn send(&self, command: Command) -> Result<(), PlaybackError> {
    self
      .worker
      .as_ref()
      .ok_or_else(|| control_error("control", "player is closed"))?
      .backend
      .send(command)
  }
  fn displayed_token(&self) -> FrameToken {
    self.pending_seek.unwrap_or(self.snapshot.token)
  }
  fn retire_surface(&mut self) -> Result<(), PlaybackError> {
    if let Some(surface) = self.surface.take() {
      surface.clear()?;
    }
    Ok(())
  }
}
impl Drop for Player {
  fn drop(&mut self) {
    // Never join from Drop (iced may drop its state on the UI thread).
    if let Some(worker) = &self.worker {
      let _ = worker.backend.send(Command::Shutdown);
    }
    if let Err(error) = self.retire_surface() {
      eprintln!("{error}");
    }
  }
}
fn control_error(operation: &'static str, reason: &str) -> PlaybackError {
  PlaybackError::Control {
    operation,
    reason: reason.into(),
  }
}
fn next_sequence(current: u64, operation: &'static str) -> Result<u64, PlaybackError> {
  current
    .checked_add(1)
    .ok_or_else(|| control_error(operation, "request sequence exhausted"))
}
