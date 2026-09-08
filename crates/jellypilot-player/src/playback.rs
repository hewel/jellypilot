use crate::{transport::NetworkSession, NetworkTimeouts, PlaybackSource, SeekRange};
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use std::{
  fs::File,
  path::PathBuf,
  sync::{mpsc, Arc, Mutex},
  thread,
  time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlaybackError {
  #[error("GStreamer initialization: {0}")]
  Initialization(String),
  #[error("Open {path}: {reason}")]
  Open { path: PathBuf, reason: String },
  #[error("Missing GStreamer element: {0}")]
  MissingElement(&'static str),
  #[error("{element}: {message} ({debug:?})")]
  Pipeline {
    element: String,
    message: String,
    debug: Option<String>,
  },
  #[error("{operation}: {reason}")]
  Control {
    operation: &'static str,
    reason: String,
  },
  #[error("Video frame: {0}")]
  Frame(String),
  #[error("Playback worker: {0}")]
  Worker(String),
  #[error("Network {operation}: {reason}")]
  Network {
    operation: &'static str,
    reason: String,
  },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameToken {
  pub generation: u64,
  pub seek_generation: u64,
}
#[derive(Debug, Clone)]
pub struct Frame {
  pub token: FrameToken,
  pub sequence: u64,
  pub sample: gst::Sample,
}
#[derive(Debug, Default)]
pub struct FrameSlot(Mutex<Option<Frame>>);
impl FrameSlot {
  pub fn latest(&self, token: FrameToken) -> Result<Option<Frame>, PlaybackError> {
    Ok(
      self
        .0
        .lock()
        .map_err(|e| PlaybackError::Worker(e.to_string()))?
        .as_ref()
        .filter(|f| f.token == token)
        .cloned(),
    )
  }
  pub fn replace(&self, frame: Option<Frame>) -> Result<Option<Frame>, PlaybackError> {
    Ok(std::mem::replace(
      &mut *self
        .0
        .lock()
        .map_err(|e| PlaybackError::Worker(e.to_string()))?,
      frame,
    ))
  }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackPhase {
  Idle,
  Loading,
  Playing,
  Paused,
  Ended,
  Error,
  Closing,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
  pub token: FrameToken,
  pub phase: PlaybackPhase,
  pub playing: bool,
  pub position: Option<Duration>,
  pub duration: Option<Duration>,
  pub seekable: bool,
  pub seek_range: Option<SeekRange>,
  pub buffering: Option<u8>,
  pub frames: Arc<FrameSlot>,
  pub open_result: Option<(u64, Result<(), PlaybackError>)>,
  pub seek_result: Option<(u64, Result<(), PlaybackError>)>,
  pub play_result: Option<u64>,
  pub error: Option<PlaybackError>,
  pub queue_depth: u64,
  pub dropped: u64,
}
impl Default for Snapshot {
  fn default() -> Self {
    Self {
      token: FrameToken::default(),
      phase: PlaybackPhase::Idle,
      playing: false,
      position: None,
      duration: None,
      seekable: false,
      seek_range: None,
      buffering: None,
      frames: Arc::default(),
      open_result: None,
      seek_result: None,
      play_result: None,
      error: None,
      queue_depth: 0,
      dropped: 0,
    }
  }
}
#[derive(Debug)]
pub enum Command {
  Open {
    generation: u64,
    source: PlaybackSource,
  },
  SetPlaying {
    generation: u64,
    playing: bool,
    seek_generation: u64,
  },
  Seek {
    generation: u64,
    seek_generation: u64,
    position: Duration,
  },
  SetVolume(f64),
  SetMuted(bool),
  RenderFailed {
    generation: u64,
    seek_generation: u64,
    error: PlaybackError,
  },
  Shutdown,
}
#[derive(Debug, Clone)]
pub struct Backend {
  pub commands: mpsc::Sender<Command>,
  pub wake: tokio::sync::mpsc::Sender<()>,
  snapshot: Arc<Mutex<Snapshot>>,
}
impl Backend {
  pub fn snapshot(&self) -> Result<Snapshot, PlaybackError> {
    self
      .snapshot
      .lock()
      .map(|s| s.clone())
      .map_err(|e| PlaybackError::Worker(e.to_string()))
  }
  pub fn send(&self, command: Command) -> Result<(), PlaybackError> {
    self
      .commands
      .send(command)
      .map_err(|e| PlaybackError::Worker(e.to_string()))
  }
  pub fn notify(&self) {
    let _ = self.wake.try_send(());
  }
}
pub struct PlaybackWorker {
  pub backend: Backend,
  join: thread::JoinHandle<Result<(), PlaybackError>>,
}
impl PlaybackWorker {
  pub fn start(silent: bool) -> Result<(Self, tokio::sync::mpsc::Receiver<()>), PlaybackError> {
    let (commands, receiver) = mpsc::channel();
    let (wake, notifications) = tokio::sync::mpsc::channel(1);
    let snapshot = Arc::new(Mutex::new(Snapshot::default()));
    let backend = Backend {
      commands,
      wake: wake.clone(),
      snapshot: snapshot.clone(),
    };
    // The worker must not retain a command sender: disconnection is a shutdown request.
    let join = thread::Builder::new()
      .name("local-video".into())
      .spawn(move || {
        let mut worker = Worker {
          receiver,
          wake,
          shared: snapshot,
          state: Snapshot::default(),
          current: None,
          volume: 1.0,
          muted: false,
          silent,
          sequence: 0,
          last_query: Instant::now(),
        };
        let result = match gst::init() {
          Ok(()) => worker.run(),
          Err(error) => {
            let error = PlaybackError::Initialization(error.to_string());
            worker.fail(error.clone())?;
            Err(error)
          }
        };
        let stopped = worker.stop();
        result.and(stopped)
      })
      .map_err(|e| PlaybackError::Worker(e.to_string()))?;
    Ok((Self { backend, join }, notifications))
  }
  // Call only from a blocking task, never update/view/Drop.
  pub fn shutdown(self) -> Result<(), PlaybackError> {
    // Stop may already have been requested before the join task was scheduled.
    let _ = self.backend.send(Command::Shutdown);
    self
      .join
      .join()
      .map_err(|_| PlaybackError::Worker("thread panicked".into()))?
  }
}
struct Current {
  dovi: crate::dovi::Capture,
  pipeline: gst::Pipeline,
  sink: gst_app::AppSink,
  bus: gst::Bus,
  waiting: Option<Instant>,
  observed: gst::State,
  buffering: bool,
  live: bool,
  network: Option<NetworkSession>,
  timeouts: NetworkTimeouts,
  wait_timeout: Duration,
}
struct Worker {
  receiver: mpsc::Receiver<Command>,
  wake: tokio::sync::mpsc::Sender<()>,
  shared: Arc<Mutex<Snapshot>>,
  state: Snapshot,
  current: Option<Current>,
  volume: f64,
  muted: bool,
  silent: bool,
  sequence: u64,
  last_query: Instant,
}
fn control(operation: &'static str, reason: impl ToString) -> PlaybackError {
  PlaybackError::Control {
    operation,
    reason: reason.to_string(),
  }
}
fn element(name: &'static str) -> Result<gst::Element, PlaybackError> {
  gst::ElementFactory::make(name)
    .build()
    .map_err(|_| PlaybackError::MissingElement(name))
}
impl Worker {
  fn publish(&self) -> Result<(), PlaybackError> {
    *self
      .shared
      .lock()
      .map_err(|e| PlaybackError::Worker(e.to_string()))? = self.state.clone();
    let _ = self.wake.try_send(());
    Ok(())
  }
  fn stop(&mut self) -> Result<(), PlaybackError> {
    if let Some(mut current) = self.current.take() {
      // Cancel upstream reads first: Null must not wait for a stalled network body.
      if let Some(network) = &current.network {
        network.cancel();
      }
      let stopped = current
        .pipeline
        .set_state(gst::State::Null)
        .map_err(|e| control("stop", e));
      let joined = current
        .network
        .take()
        .map_or(Ok(()), NetworkSession::shutdown);
      let cleared = self.state.frames.replace(None).map(|_| ());
      stopped.and(joined).and(cleared)?;
    }
    self.state.buffering = None;
    Ok(())
  }
  fn fail(&mut self, error: PlaybackError) -> Result<(), PlaybackError> {
    eprintln!("{error}");
    let stopped = self.stop();
    self.state.phase = PlaybackPhase::Error;
    self.state.playing = false;
    self.state.seekable = false;
    self.state.seek_range = None;
    self.state.error = Some(error);
    if let Err(error) = stopped {
      self.state.error = Some(error);
    }
    self.publish()
  }
  fn run(&mut self) -> Result<(), PlaybackError> {
    loop {
      let command = if self.current.is_none() {
        match self.receiver.recv() {
          Ok(c) => Some(c),
          Err(_) => break,
        }
      } else {
        let active =
          self.current.as_ref().is_some_and(|c| c.waiting.is_some()) || self.state.playing;
        match self
          .receiver
          .recv_timeout(Duration::from_millis(if active { 10 } else { 250 }))
        {
          Ok(c) => Some(c),
          Err(mpsc::RecvTimeoutError::Timeout) => None,
          Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
      };
      if let Some(command) = command {
        if !self.command(command)? {
          break;
        }
      }
      while let Ok(command) = self.receiver.try_recv() {
        if !self.command(command)? {
          return Ok(());
        }
      }
      if let Err(error) = self.poll() {
        self.fail(error)?;
      }
    }
    Ok(())
  }
  fn command(&mut self, command: Command) -> Result<bool, PlaybackError> {
    match command {
      Command::Shutdown => {
        self.state.phase = PlaybackPhase::Closing;
        self.stop()?;
        self.publish()?;
        return Ok(false);
      }
      Command::Open { generation, source } => self.open(generation, source)?,
      Command::SetVolume(volume) => {
        if volume.is_finite() {
          self.volume = volume.clamp(0.0, 1.0);
          if let Some(c) = &self.current {
            c.pipeline.set_property("volume", self.volume);
          }
        }
      }
      Command::SetMuted(muted) => {
        self.muted = muted;
        if let Some(c) = &self.current {
          c.pipeline.set_property("mute", muted);
        }
      }
      Command::SetPlaying {
        generation,
        playing,
        seek_generation,
      } if generation == self.state.token.generation => {
        if self.current.is_some() {
          self.state.play_result = Some(seek_generation);
          if playing
            && self.state.phase == PlaybackPhase::Ended
            && !self.seek(seek_generation, Duration::ZERO)?
          {
            return Ok(true);
          }
          self.state.playing = playing;
          if let Some(c) = &self.current {
            if let Err(error) = c
              .pipeline
              .set_state(if playing && (!c.buffering || c.live) {
                gst::State::Playing
              } else {
                gst::State::Paused
              })
            {
              self.fail(control("play/pause", error))?;
            }
          }
          self.publish()?;
        }
      }
      Command::Seek {
        generation,
        seek_generation,
        position,
      } if generation == self.state.token.generation => {
        self.seek(seek_generation, position)?;
      }
      Command::RenderFailed {
        generation,
        seek_generation,
        error,
      } if self.state.token
        == (FrameToken {
          generation,
          seek_generation,
        }) =>
      {
        self.fail(error)?
      }
      _ => {}
    }
    Ok(true)
  }
  fn open(&mut self, generation: u64, source: PlaybackSource) -> Result<(), PlaybackError> {
    let prepared = (|| match source {
      PlaybackSource::Local(path) => {
        let uri = (|| {
          let canonical = path.canonicalize().map_err(|e| e.to_string())?;
          if !canonical.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("not a regular local file".into());
          }
          let file = File::open(&canonical).map_err(|e| e.to_string())?;
          if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("not a regular local file".into());
          }
          url::Url::from_file_path(canonical).map_err(|()| "cannot construct local file URI".into())
        })()
        .map_err(|reason| PlaybackError::Open { path, reason })?;
        Ok((
          uri.to_string(),
          None,
          NetworkTimeouts {
            startup: Duration::from_secs(10),
            seek: Duration::from_secs(10),
            ..NetworkTimeouts::default()
          },
        ))
      }
      PlaybackSource::Network(source) => {
        let timeouts = source.timeouts();
        let network = NetworkSession::start(source)?;
        Ok((network.uri().to_owned(), Some(network), timeouts))
      }
    })();
    let (uri, network, timeouts) = match prepared {
      Ok(prepared) => prepared,
      Err(error) => {
        self.state.open_result = Some((generation, Err(error)));
        return self.publish();
      }
    };
    if let Err(error) = self.stop() {
      self.fail(error)?;
      return Ok(());
    }
    self.state = Snapshot {
      token: FrameToken {
        generation,
        seek_generation: 0,
      },
      phase: PlaybackPhase::Loading,
      playing: true,
      open_result: Some((generation, Ok(()))),
      ..Snapshot::default()
    };
    self.publish()?;
    match self.build(&uri) {
      Ok(mut current) => {
        current.network = network;
        current.timeouts = timeouts;
        current.wait_timeout = timeouts.startup;
        self.current = Some(current);
        self.last_query = Instant::now() - Duration::from_secs(1);
      }
      Err(error) => {
        if let Some(network) = network {
          network.shutdown()?;
        }
        self.fail(error)?;
      }
    }
    Ok(())
  }
  fn build(&self, uri: &str) -> Result<Current, PlaybackError> {
    let pipeline = element("playbin3")?
      .downcast::<gst::Pipeline>()
      .map_err(|_| control("create", "playbin3 is not a pipeline"))?;
    let sink = element("appsink")?
      .downcast::<gst_app::AppSink>()
      .map_err(|_| control("create", "appsink has wrong type"))?;
    sink.set_caps(Some(
      &gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .build(),
    ));
    sink.set_max_buffers(2);
    sink.set_leaky_type(gst_app::AppLeakyType::Downstream);
    sink.set_property("sync", true);
    sink.set_property("enable-last-sample", false);
    sink.set_wait_on_eos(false);
    let convert = element("videoconvert")?;
    // This sink performs the measured P010 -> RGBA conversion, not the decoder.
    let conversion_threads = thread::available_parallelism().map_or(1, |n| n.get().min(4)) as u32;
    convert.set_property("n-threads", conversion_threads);
    let bin = gst::Bin::new();
    bin
      .add_many([&convert, sink.upcast_ref()])
      .map_err(|e| control("video sink", e))?;
    convert.link(&sink).map_err(|e| control("video sink", e))?;
    let pad = convert
      .static_pad("sink")
      .ok_or_else(|| control("video sink", "missing sink pad"))?;
    let ghost = gst::GhostPad::with_target(&pad).map_err(|e| control("video sink", e))?;
    bin.add_pad(&ghost).map_err(|e| control("video sink", e))?;
    pipeline.set_property("video-sink", &bin);
    let audio = element(if self.silent {
      "fakesink"
    } else {
      "autoaudiosink"
    })?;
    if self.silent {
      audio.set_property("sync", true);
    }
    pipeline.set_property("audio-sink", &audio);
    let flags = pipeline.property_value("flags");
    let class = gst::glib::FlagsClass::with_type(flags.type_())
      .ok_or_else(|| control("flags", "invalid playbin flags"))?;
    let flags = class
      .builder_with_value(flags)
      .ok_or_else(|| control("flags", "invalid default flags"))?
      .unset_by_nick("text")
      .unset_by_nick("vis")
      .build()
      .ok_or_else(|| control("flags", "cannot disable subtitles/visualization"))?;
    pipeline.set_property_from_value("flags", &flags);
    pipeline.set_property("uri", uri);
    pipeline.set_property("volume", self.volume);
    pipeline.set_property("mute", self.muted);
    let bus = pipeline
      .bus()
      .ok_or_else(|| control("create", "missing bus"))?;
    let dovi = crate::dovi::install(&pipeline, &sink);
    let transition = match pipeline.set_state(gst::State::Playing) {
      Ok(transition) => transition,
      Err(error) => {
        pipeline
          .set_state(gst::State::Null)
          .map_err(|e| control("stop failed startup", e))?;
        return Err(control("open playback", error));
      }
    };
    Ok(Current {
      dovi,
      pipeline,
      sink,
      bus,
      waiting: Some(Instant::now()),
      observed: gst::State::Null,
      buffering: false,
      live: transition == gst::StateChangeSuccess::NoPreroll,
      network: None,
      timeouts: NetworkTimeouts {
        startup: Duration::from_secs(10),
        seek: Duration::from_secs(10),
        ..NetworkTimeouts::default()
      },
      wait_timeout: Duration::from_secs(10),
    })
  }
  fn seek(&mut self, seek_generation: u64, position: Duration) -> Result<bool, PlaybackError> {
    let Some(current) = &mut self.current else {
      return Ok(false);
    };
    if seek_generation <= self.state.token.seek_generation {
      return Ok(false);
    }
    let Some(position) = self
      .state
      .seek_range
      .and_then(|range| range.clamp(position))
    else {
      self.state.seek_result = Some((
        seek_generation,
        Err(control("seek", "no finite seek window is available")),
      ));
      self.publish()?;
      return Ok(false);
    };
    let old = self.state.frames.replace(None)?;
    // No second producer: flushing seek and sample extraction run on this thread.
    let result = u64::try_from(position.as_nanos())
      .map_err(|e| control("seek", e))
      .and_then(|nanos| {
        current
          .pipeline
          .seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::ClockTime::from_nseconds(nanos),
          )
          .map_err(|e| control("seek", e))
      });
    if let Err(error) = result {
      self.state.frames.replace(old)?;
      self.state.seek_result = Some((seek_generation, Err(error)));
      self.publish()?;
      return Ok(false);
    }
    self.state.token.seek_generation = seek_generation;
    self.state.seek_result = Some((seek_generation, Ok(())));
    self.state.position = Some(position);
    if self.state.phase == PlaybackPhase::Ended {
      self.state.phase = PlaybackPhase::Paused;
    }
    current.waiting = Some(Instant::now());
    current.wait_timeout = current.timeouts.seek;
    self.publish()?;
    Ok(true)
  }
  fn query(&mut self) {
    if let Some(current) = &mut self.current {
      if self.state.phase != PlaybackPhase::Ended {
        self.state.position = current
          .pipeline
          .query_position::<gst::ClockTime>()
          .map(|v| Duration::from_nanos(v.nseconds()))
          .or(self.state.position);
      }
      self.state.duration = current
        .pipeline
        .query_duration::<gst::ClockTime>()
        .map(|v| Duration::from_nanos(v.nseconds()));
      let mut seeking = gst::query::Seeking::new(gst::Format::Time);
      self.state.seekable = current.pipeline.query(&mut seeking) && seeking.result().0;
      self.state.seek_range = match seeking.result() {
        (
          true,
          gst::GenericFormattedValue::Time(Some(start)),
          gst::GenericFormattedValue::Time(end),
        ) => Some(SeekRange {
          start: Duration::from_nanos(start.nseconds()),
          end: end.map(|end| Duration::from_nanos(end.nseconds())),
        }),
        _ => None,
      };
      // Latency queries describe the sink's scheduling too, not just a live source.
      current.live |= matches!(
        current.pipeline.state(gst::ClockTime::ZERO).0,
        Ok(gst::StateChangeSuccess::NoPreroll)
      );
      self.state.queue_depth = current.sink.current_level_buffers();
      self.state.dropped = current.sink.dropped();
      self.last_query = Instant::now();
    }
  }
  fn poll(&mut self) -> Result<(), PlaybackError> {
    if let Some(error) = self
      .current
      .as_ref()
      .and_then(|current| current.dovi.error())
    {
      return Err(error);
    }
    if let Some(error) = self
      .current
      .as_ref()
      .and_then(|current| current.network.as_ref())
      .and_then(NetworkSession::error)
    {
      return Err(error);
    }
    let mut changed = false;
    loop {
      let Some(message) = self.current.as_ref().and_then(|c| c.bus.pop()) else {
        break;
      };
      match message.view() {
        gst::MessageView::Error(error) => {
          if let Some(network) = self
            .current
            .as_ref()
            .and_then(|current| current.network.as_ref())
          {
            // Native debug strings may include token-bearing upstream or relay URLs.
            // Transport failures carry their own sanitized, actionable diagnostics.
            return Err(network.error().unwrap_or_else(|| PlaybackError::Network {
              operation: "decode",
              reason: format!(
                "GStreamer failed in {:?} (code {})",
                error.error().domain(),
                error.error().code()
              ),
            }));
          }
          return Err(PlaybackError::Pipeline {
            element: error
              .src()
              .map_or_else(|| "pipeline".into(), |s| s.path_string().to_string()),
            message: error.error().to_string(),
            debug: error.debug().map(|s| s.to_string()),
          });
        }
        gst::MessageView::Eos(_) => {
          self.query();
          self.state.phase = PlaybackPhase::Ended;
          self.state.playing = false;
          self.state.position = self.state.duration.or(self.state.position);
          self.state.buffering = None;
          if let Some(c) = &mut self.current {
            c.waiting = None;
            c.pipeline
              .set_state(gst::State::Paused)
              .map_err(|e| control("end playback", e))?;
          }
          changed = true;
        }
        gst::MessageView::StateChanged(state) => {
          if let Some(c) = &mut self.current {
            if state
              .src()
              .is_some_and(|s| s == c.pipeline.upcast_ref::<gst::Object>())
            {
              c.observed = state.current();
              changed = true;
            }
          }
        }
        gst::MessageView::AsyncDone(_) | gst::MessageView::DurationChanged(_) => {
          self.query();
          changed = true;
        }
        gst::MessageView::StreamCollection(streams) => {
          if !streams
            .stream_collection()
            .iter()
            .any(|s| s.stream_type().contains(gst::StreamType::VIDEO))
          {
            return Err(control("open", "所选文件没有视频流"));
          }
        }
        gst::MessageView::Buffering(buffer) => {
          if let Some(c) = &mut self.current {
            c.live |= buffer.buffering_stats().0 == gst::BufferingMode::Live;
            c.buffering = buffer.percent() < 100;
            self.state.buffering = c.buffering.then_some(buffer.percent().clamp(0, 99) as u8);
            changed = true;
            if !c.live {
              c.pipeline
                .set_state(if self.state.playing && !c.buffering {
                  gst::State::Playing
                } else {
                  gst::State::Paused
                })
                .map_err(|e| control("buffering", e))?;
            }
          }
        }
        _ => {}
      }
    }
    if let Some(c) = &mut self.current {
      // Pulling a regular sample clears appsink's preroll reference, even on timeout.
      let sample = if self.state.playing {
        c.sink.try_pull_sample(gst::ClockTime::ZERO)
      } else if c.waiting.is_some() {
        c.sink.try_pull_preroll(gst::ClockTime::ZERO)
      } else {
        None
      };
      if let Some(sample) = sample {
        if c.waiting.is_some() && self.state.token.seek_generation == 0 {
          eprintln!("Video negotiated: {:?}", sample.caps());
          for element in c.pipeline.iterate_recurse().into_iter().flatten() {
            if let Some(factory) = element.factory() {
              if factory.klass().contains("Decoder") {
                eprintln!("Decoder: {}", factory.name());
              }
            }
          }
        }
        self.sequence = self
          .sequence
          .checked_add(1)
          .ok_or_else(|| control("frame", "sequence exhausted"))?;
        self.state.frames.replace(Some(Frame {
          token: self.state.token,
          sequence: self.sequence,
          sample,
        }))?;
        c.waiting = None;
        changed = true;
      }
      if c
        .waiting
        .is_some_and(|start| start.elapsed() >= c.wait_timeout)
      {
        return Err(control(
          "load/seek",
          format!(
            "timed out waiting for a video frame ({} seconds)",
            c.wait_timeout.as_secs_f64()
          ),
        ));
      }
      if !matches!(
        self.state.phase,
        PlaybackPhase::Ended | PlaybackPhase::Error
      ) && self.state.frames.latest(self.state.token)?.is_some()
      {
        let phase = if c.buffering && self.state.playing || c.observed == gst::State::Playing {
          PlaybackPhase::Playing
        } else if c.observed == gst::State::Paused {
          PlaybackPhase::Paused
        } else {
          PlaybackPhase::Loading
        };
        changed |= self.state.phase != phase;
        self.state.phase = phase;
      }
    }
    if self.current.is_some() && self.last_query.elapsed() >= Duration::from_millis(250) {
      self.query();
      changed = true;
    }
    if changed {
      self.publish()?;
    }
    Ok(())
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};

  const DEADLINE: Duration = Duration::from_secs(15);

  struct NullOnDrop(gst::Pipeline);

  impl Drop for NullOnDrop {
    fn drop(&mut self) {
      let _ = self.0.set_state(gst::State::Null);
    }
  }

  pub(crate) fn fixture() -> (tempfile::TempDir, PathBuf) {
    gst::init().expect("initialize GStreamer");
    let directory = tempfile::tempdir().expect("create test directory");
    let path = directory.path().join("video 空格 #.webm");
    let pipeline = NullOnDrop(gst::Pipeline::new());
    let source = gst::ElementFactory::make("videotestsrc")
      .property("num-buffers", 12_i32)
      .build()
      .expect("videotestsrc is required");
    let caps = gst::ElementFactory::make("capsfilter")
      .property(
        "caps",
        gst::Caps::builder("video/x-raw")
          .field("width", 64_i32)
          .field("height", 48_i32)
          .field("framerate", gst::Fraction::new(6, 1))
          .build(),
      )
      .build()
      .expect("create capsfilter");
    let convert = element("videoconvert").expect("videoconvert is required");
    let encoder = gst::ElementFactory::make("vp8enc")
      .property("deadline", 1_i64)
      .build()
      .expect("vp8enc is required");
    let mux = element("webmmux").expect("webmmux is required");
    let sink = gst::ElementFactory::make("filesink")
      .property("location", path.to_str().expect("UTF-8 test path"))
      .build()
      .expect("filesink is required");
    let elements = [&source, &caps, &convert, &encoder, &mux, &sink];
    pipeline.0.add_many(elements).expect("add fixture elements");
    gst::Element::link_many(elements).expect("link fixture elements");
    let bus = pipeline.0.bus().expect("fixture bus");
    pipeline
      .0
      .set_state(gst::State::Playing)
      .expect("start fixture");
    let message = bus
      .timed_pop_filtered(
        gst::ClockTime::from_seconds(15),
        &[gst::MessageType::Eos, gst::MessageType::Error],
      )
      .expect("fixture generation timed out");
    if let gst::MessageView::Error(error) = message.view() {
      panic!(
        "fixture generation failed: {} ({:?})",
        error.error(),
        error.debug()
      );
    }
    pipeline
      .0
      .set_state(gst::State::Null)
      .expect("finish fixture");
    (directory, path)
  }

  fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .expect("test runtime")
  }

  fn wait_for(
    runtime: &tokio::runtime::Runtime,
    backend: &Backend,
    notifications: &mut tokio::sync::mpsc::Receiver<()>,
    description: &str,
    predicate: impl Fn(&Snapshot) -> bool,
  ) -> Snapshot {
    runtime.block_on(async {
      tokio::time::timeout(DEADLINE, async {
        loop {
          let snapshot = backend.snapshot().expect("read worker snapshot");
          if predicate(&snapshot) {
            return snapshot;
          }
          assert_ne!(
            snapshot.phase,
            PlaybackPhase::Error,
            "{description}: {:?}",
            snapshot.error
          );
          assert!(
            notifications.recv().await.is_some(),
            "worker closed while waiting for {description}"
          );
        }
      })
      .await
      .unwrap_or_else(|_| {
        panic!(
          "timed out waiting for {description}: {:?}",
          backend.snapshot()
        );
      })
    })
  }

  fn visible(snapshot: &Snapshot) -> Frame {
    snapshot
      .frames
      .latest(snapshot.token)
      .expect("read frame slot")
      .expect("visible frame")
  }

  fn finish(worker: PlaybackWorker, disconnect: bool) {
    let (done, completed) = mpsc::channel();
    thread::spawn(move || {
      let result = if disconnect {
        let PlaybackWorker { backend, join } = worker;
        drop(backend);
        join.join().expect("worker thread panicked")
      } else {
        worker.shutdown()
      };
      let _ = done.send(result);
    });
    completed
      .recv_timeout(DEADLINE)
      .expect("worker did not terminate before deadline")
      .expect("worker cleanup failed");
  }

  #[test]
  fn paused_seek_replacement_error_recovery_and_replay_use_real_media() {
    let (directory, path) = fixture();
    let runtime = runtime();
    let (worker, mut notifications) = PlaybackWorker::start(true).expect("start worker");
    let backend = &worker.backend;
    backend
      .send(Command::Open {
        generation: 1,
        source: PlaybackSource::Local(path.clone()),
      })
      .expect("open fixture");
    let playing = wait_for(
      &runtime,
      backend,
      &mut notifications,
      "playing video",
      |s| {
        s.phase == PlaybackPhase::Playing
          && s.seekable
          && s.frames.latest(s.token).unwrap().is_some()
      },
    );
    let first = visible(&playing);
    backend
      .send(Command::SetPlaying {
        generation: 1,
        playing: false,
        seek_generation: 0,
      })
      .expect("pause");
    wait_for(&runtime, backend, &mut notifications, "paused video", |s| {
      s.phase == PlaybackPhase::Paused && !s.playing
    });
    backend
      .send(Command::Seek {
        generation: 1,
        seek_generation: 1,
        position: Duration::from_secs(1),
      })
      .expect("paused seek");
    let sought = wait_for(
      &runtime,
      backend,
      &mut notifications,
      "new paused preroll",
      |s| {
        s.token.seek_generation == 1
          && s.phase == PlaybackPhase::Paused
          && s.frames.latest(s.token).unwrap().is_some()
      },
    );
    assert!(!sought.playing, "paused seek resumed playback");
    let frame = visible(&sought);
    let pts = frame
      .sample
      .buffer()
      .expect("preroll buffer")
      .pts()
      .expect("preroll PTS");
    assert!(
      pts.nseconds().abs_diff(1_000_000_000) <= 166_666_667,
      "unexpected seek PTS: {pts}"
    );
    assert!(frame.sequence > first.sequence);
    assert!(
      sought.frames.latest(first.token).unwrap().is_none(),
      "old seek image remained visible"
    );
    for stale in [
      first.token,
      FrameToken {
        generation: 0,
        seek_generation: 1,
      },
    ] {
      backend
        .send(Command::RenderFailed {
          generation: stale.generation,
          seek_generation: stale.seek_generation,
          error: PlaybackError::Frame("late upload failure from an obsolete frame".into()),
        })
        .expect("report stale renderer failure");
    }

    backend
      .send(Command::Open {
        generation: 2,
        source: PlaybackSource::Local(directory.path().join("missing.webm")),
      })
      .expect("open missing file");
    let rejected = wait_for(
      &runtime,
      backend,
      &mut notifications,
      "invalid replacement",
      |s| matches!(s.open_result, Some((2, Err(_)))),
    );
    assert_eq!(rejected.token, sought.token);
    assert_eq!(visible(&rejected).sequence, frame.sequence);
    assert_eq!(rejected.phase, PlaybackPhase::Paused);
    assert!(
      rejected.error.is_none(),
      "stale renderer failure stopped current playback"
    );

    let damaged = directory.path().join("damaged.webm");
    std::fs::write(&damaged, b"not a media container").expect("write damaged fixture");
    backend
      .send(Command::Open {
        generation: 3,
        source: PlaybackSource::Local(damaged),
      })
      .expect("open damaged file");
    let failed = wait_for(
      &runtime,
      backend,
      &mut notifications,
      "damaged media error",
      |s| s.token.generation == 3 && s.phase == PlaybackPhase::Error,
    );
    assert!(failed.error.is_some());
    assert!(!failed.playing);
    assert!(failed.frames.latest(failed.token).unwrap().is_none());
    assert!(
      sought.frames.latest(sought.token).unwrap().is_none(),
      "replacement retained old image"
    );

    backend
      .send(Command::Open {
        generation: 4,
        source: PlaybackSource::Local(path),
      })
      .expect("recover with valid file");
    // The previous Error snapshot may still be current until Open is accepted.
    runtime.block_on(async {
      tokio::time::timeout(DEADLINE, async {
        while backend.snapshot().unwrap().token.generation != 4 {
          assert!(notifications.recv().await.is_some());
        }
      })
      .await
      .expect("recovery was not accepted");
    });
    let recovered = wait_for(
      &runtime,
      backend,
      &mut notifications,
      "recovered playback",
      |s| s.phase == PlaybackPhase::Playing && s.frames.latest(s.token).unwrap().is_some(),
    );
    assert!(recovered.frames.latest(frame.token).unwrap().is_none());
    let ended = wait_for(&runtime, backend, &mut notifications, "EOS", |s| {
      s.phase == PlaybackPhase::Ended
    });
    assert!(!ended.playing);
    assert_eq!(ended.position, ended.duration);
    let last = visible(&ended);
    backend
      .send(Command::SetPlaying {
        generation: 4,
        playing: true,
        seek_generation: ended.token.seek_generation + 1,
      })
      .expect("replay");
    let replayed = wait_for(
      &runtime,
      backend,
      &mut notifications,
      "replayed frame",
      |s| {
        s.phase == PlaybackPhase::Playing
          && s.token.seek_generation > ended.token.seek_generation
          && s.frames.latest(s.token).unwrap().is_some()
      },
    );
    let replay_frame = visible(&replayed);
    assert!(replay_frame.sequence > last.sequence);
    assert!(replay_frame.sample.buffer().unwrap().pts().unwrap() < gst::ClockTime::from_seconds(1));
    assert!(replayed.frames.latest(last.token).unwrap().is_none());
    let retained = replayed.frames;
    let token = replayed.token;
    finish(worker, false);
    assert!(
      retained.latest(token).unwrap().is_none(),
      "shutdown retained video"
    );
  }

  #[test]
  fn command_sender_disconnection_terminates_public_worker() {
    let (_directory, path) = fixture();
    let runtime = runtime();
    let (worker, mut notifications) = PlaybackWorker::start(true).expect("start worker");
    worker
      .backend
      .send(Command::Open {
        generation: 1,
        source: PlaybackSource::Local(path),
      })
      .expect("open fixture");
    let playing = wait_for(
      &runtime,
      &worker.backend,
      &mut notifications,
      "playing before disconnect",
      |s| s.phase == PlaybackPhase::Playing && s.frames.latest(s.token).unwrap().is_some(),
    );
    finish(worker, true);
    assert!(
      playing.frames.latest(playing.token).unwrap().is_none(),
      "disconnect retained video"
    );
  }

  #[test]
  fn replay_then_seek_before_notifications_preserves_latest_target() {
    let (_directory, path) = fixture();
    let runtime = runtime();
    let (worker, mut notifications) = PlaybackWorker::start(true).unwrap();
    let backend = &worker.backend;
    backend
      .send(Command::Open {
        generation: 1,
        source: PlaybackSource::Local(path),
      })
      .unwrap();
    wait_for(&runtime, backend, &mut notifications, "EOS", |s| {
      s.phase == PlaybackPhase::Ended
    });
    // Both requests are issued without consuming the replay acknowledgement.
    backend
      .send(Command::SetPlaying {
        generation: 1,
        playing: true,
        seek_generation: 5,
      })
      .unwrap();
    backend
      .send(Command::Seek {
        generation: 1,
        seek_generation: 6,
        position: Duration::from_secs(1),
      })
      .unwrap();
    let sought = wait_for(
      &runtime,
      backend,
      &mut notifications,
      "seek after replay",
      |s| {
        s.token.seek_generation == 6
          && s.phase == PlaybackPhase::Playing
          && s.frames.latest(s.token).unwrap().is_some()
      },
    );
    let frame = visible(&sought);
    let pts = frame.sample.buffer().unwrap().pts().unwrap().nseconds();
    assert!(
      pts.abs_diff(1_000_000_000) <= 166_666_667,
      "wrong replay/seek PTS: {pts}"
    );
    finish(worker, false);
  }

  #[cfg(unix)]
  #[test]
  fn fifo_is_rejected_without_blocking_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let fifo = directory.path().join("video.fifo");
    assert!(std::process::Command::new("mkfifo")
      .arg(&fifo)
      .status()
      .unwrap()
      .success());
    let runtime = runtime();
    let (worker, mut notifications) = PlaybackWorker::start(true).unwrap();
    worker
      .backend
      .send(Command::Open {
        generation: 1,
        source: PlaybackSource::Local(fifo),
      })
      .unwrap();
    let rejected = wait_for(
      &runtime,
      &worker.backend,
      &mut notifications,
      "FIFO rejection",
      |s| {
        s.open_result
          .as_ref()
          .is_some_and(|(g, result)| *g == 1 && result.is_err())
      },
    );
    assert_eq!(rejected.phase, PlaybackPhase::Idle);
    finish(worker, false);
  }

  #[test]
  fn replacement_and_shutdown_put_observed_pipelines_in_null() {
    let (directory, path) = fixture();
    let (commands, receiver) = mpsc::channel();
    let (wake, _notifications) = tokio::sync::mpsc::channel(1);
    let mut worker = Worker {
      receiver,
      wake,
      shared: Arc::new(Mutex::new(Snapshot::default())),
      state: Snapshot::default(),
      current: None,
      volume: 1.0,
      muted: false,
      silent: true,
      sequence: 0,
      last_query: Instant::now(),
    };
    worker
      .command(Command::Open {
        generation: 1,
        source: PlaybackSource::Local(path.clone()),
      })
      .unwrap();
    let original = NullOnDrop(
      worker
        .current
        .as_ref()
        .expect("original pipeline")
        .pipeline
        .clone(),
    );
    let (result, state, _) = original.0.state(gst::ClockTime::from_seconds(15));
    result.expect("original pipeline state transition");
    assert_eq!(state, gst::State::Playing);
    worker
      .command(Command::Open {
        generation: 2,
        source: PlaybackSource::Local(directory.path().join("missing.webm")),
      })
      .unwrap();
    assert_eq!(
      original.0.current_state(),
      gst::State::Playing,
      "invalid path stopped active pipeline"
    );
    let damaged = directory.path().join("damaged.webm");
    std::fs::write(&damaged, b"not media").unwrap();
    worker
      .command(Command::Open {
        generation: 3,
        source: PlaybackSource::Local(damaged),
      })
      .unwrap();
    assert_eq!(
      original.0.current_state(),
      gst::State::Null,
      "replacement left old pipeline active"
    );
    worker
      .command(Command::Open {
        generation: 4,
        source: PlaybackSource::Local(path),
      })
      .unwrap();
    let replacement = NullOnDrop(
      worker
        .current
        .as_ref()
        .expect("replacement pipeline")
        .pipeline
        .clone(),
    );
    let (result, state, _) = replacement.0.state(gst::ClockTime::from_seconds(15));
    result.expect("replacement pipeline state transition");
    assert_eq!(state, gst::State::Playing);
    commands.send(Command::Shutdown).unwrap();
    let (done, completed) = mpsc::channel();
    thread::spawn(move || {
      let result = worker.run();
      let stopped = worker.stop();
      let _ = done.send(result.and(stopped));
    });
    completed
      .recv_timeout(DEADLINE)
      .expect("shutdown timed out")
      .expect("shutdown failed");
    assert_eq!(
      replacement.0.current_state(),
      gst::State::Null,
      "shutdown left pipeline active"
    );
  }

  #[test]
  fn buffering_preserves_play_intent_and_completion_respects_user_pause() {
    let (_directory, path) = fixture();
    let (_commands, receiver) = mpsc::channel();
    let (wake, _notifications) = tokio::sync::mpsc::channel(1);
    let mut worker = Worker {
      receiver,
      wake,
      shared: Arc::new(Mutex::new(Snapshot::default())),
      state: Snapshot::default(),
      current: None,
      volume: 1.0,
      muted: false,
      silent: true,
      sequence: 0,
      last_query: Instant::now(),
    };
    worker.open(1, PlaybackSource::Local(path)).unwrap();
    let pipeline = NullOnDrop(worker.current.as_ref().unwrap().pipeline.clone());
    pipeline
      .0
      .state(gst::ClockTime::from_seconds(15))
      .0
      .unwrap();
    worker.poll().unwrap();
    pipeline
      .0
      .post_message(
        gst::message::Buffering::builder(25)
          .src(&pipeline.0)
          .build(),
      )
      .unwrap();
    worker.poll().unwrap();
    pipeline
      .0
      .state(gst::ClockTime::from_seconds(15))
      .0
      .unwrap();
    worker.poll().unwrap();
    assert_eq!(
      pipeline.0.current_state(),
      gst::State::Paused,
      "live={}, buffering={:?}",
      worker.current.as_ref().unwrap().live,
      worker.state.buffering
    );
    assert!(worker.state.playing, "buffering is not a user pause");
    assert_eq!(worker.state.buffering, Some(25));
    assert_eq!(worker.state.phase, PlaybackPhase::Playing);

    worker
      .command(Command::SetPlaying {
        generation: 1,
        seek_generation: 1,
        playing: false,
      })
      .unwrap();
    pipeline
      .0
      .post_message(
        gst::message::Buffering::builder(100)
          .src(&pipeline.0)
          .build(),
      )
      .unwrap();
    worker.poll().unwrap();
    pipeline
      .0
      .state(gst::ClockTime::from_seconds(15))
      .0
      .unwrap();
    worker.poll().unwrap();
    assert!(!worker.state.playing);
    assert_eq!(worker.state.phase, PlaybackPhase::Paused);
    assert_eq!(
      pipeline.0.current_state(),
      gst::State::Paused,
      "buffer completion must not resume a user-paused stream"
    );
    assert_eq!(worker.state.buffering, None);
    worker
      .command(Command::SetPlaying {
        generation: 1,
        seek_generation: 2,
        playing: true,
      })
      .unwrap();
    pipeline
      .0
      .state(gst::ClockTime::from_seconds(15))
      .0
      .unwrap();
    pipeline
      .0
      .post_message(
        gst::message::Buffering::builder(25)
          .src(&pipeline.0)
          .stats(gst::BufferingMode::Live, -1, -1, -1)
          .build(),
      )
      .unwrap();
    worker.poll().unwrap();
    assert_eq!(
      pipeline.0.current_state(),
      gst::State::Playing,
      "a live source must not be paused to buffer"
    );
    assert_eq!(worker.state.buffering, Some(25));
    worker.stop().unwrap();
  }

  struct TrackedBytes {
    bytes: [u8; 4],
    live: Arc<AtomicUsize>,
  }

  impl AsRef<[u8]> for TrackedBytes {
    fn as_ref(&self) -> &[u8] {
      &self.bytes
    }
  }

  impl Drop for TrackedBytes {
    fn drop(&mut self) {
      self.live.fetch_sub(1, Ordering::SeqCst);
    }
  }

  #[test]
  fn stalled_consumer_retains_only_held_and_latest_frames_and_rejects_stale_tokens() {
    gst::init().expect("initialize GStreamer");
    let slot = Arc::new(FrameSlot::default());
    let live = Arc::new(AtomicUsize::new(0));
    let token = FrameToken {
      generation: 2,
      seek_generation: 3,
    };
    let (ready, produced) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let producer_slot = slot.clone();
    let producer_live = live.clone();
    let producer = thread::spawn(move || {
      for sequence in 1..=128 {
        producer_live.fetch_add(1, Ordering::SeqCst);
        let buffer = gst::Buffer::from_slice(TrackedBytes {
          bytes: [sequence as u8; 4],
          live: producer_live.clone(),
        });
        let sample = gst::Sample::builder().buffer(&buffer).build();
        drop(buffer);
        drop(
          producer_slot
            .replace(Some(Frame {
              token,
              sequence,
              sample,
            }))
            .unwrap(),
        );
        assert!(
          producer_live.load(Ordering::SeqCst) <= 2,
          "historical frames accumulated"
        );
        if sequence == 1 {
          ready.send(()).unwrap();
          resumed
            .recv_timeout(DEADLINE)
            .expect("consumer failed to retain first frame");
        }
      }
      ready.send(()).unwrap();
    });
    produced
      .recv_timeout(DEADLINE)
      .expect("first frame timed out");
    let held = slot.latest(token).unwrap().unwrap();
    resume.send(()).unwrap();
    produced
      .recv_timeout(DEADLINE)
      .expect("producer stalled behind consumer");
    producer.join().expect("producer panicked");
    let latest = slot.latest(token).unwrap().unwrap();
    assert_eq!(latest.sequence, 128);
    assert_eq!(
      latest
        .sample
        .buffer()
        .unwrap()
        .map_readable()
        .unwrap()
        .as_slice(),
      &[128; 4]
    );
    assert_eq!(live.load(Ordering::SeqCst), 2);
    assert!(slot
      .latest(FrameToken {
        generation: 1,
        ..token
      })
      .unwrap()
      .is_none());
    assert!(slot
      .latest(FrameToken {
        seek_generation: 2,
        ..token
      })
      .unwrap()
      .is_none());
    drop(held);
    assert_eq!(live.load(Ordering::SeqCst), 1);
    drop(latest);
    drop(slot.replace(None).unwrap());
    assert_eq!(
      live.load(Ordering::SeqCst),
      0,
      "cleared surface retained sample memory"
    );
  }
}
