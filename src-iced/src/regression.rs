//! Explicit, bounded native probes. No probe is selected by environment variables.
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::app::message::{Message, WindowMessage};
use iced::{Subscription, Task};
use serde_json::json;

static RUN: OnceLock<Mutex<Run>> = OnceLock::new();

struct Run {
  scenario: String,
  run_id: String,
  unavailable: bool,
  physical_size: Option<(u32, u32)>,
  video_size: iced::Size,
  report: PathBuf,
  media: Option<PathBuf>,
  hwdec: Option<String>,
  checks: Vec<String>,
  decoder_samples: Vec<serde_json::Value>,
  error: Option<String>,
  adapter: Option<String>,
  action: Action,
  hosts: usize,
  dropped_hosts: usize,
  renderers: usize,
  surfaces: usize,
  closed: bool,
  complete: bool,
  finished: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
  None,
  Resize,
  Close,
  Closing,
  Reopened,
  Exit,
}

fn with_run<T>(f: impl FnOnce(&mut Run) -> T) -> Option<T> {
  RUN.get().map(|run| {
    f(&mut run
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner))
  })
}

impl Run {
  fn save(&self, passed: bool) -> Result<(), String> {
    let mut report = json!({"schemaVersion": 1, "scenario": self.scenario, "runId": self.run_id,
      "status": if passed { "pass" } else if self.unavailable { "unavailable" } else { "fail" }, "checks": self.checks});
    if let Some(error) = &self.error {
      report["error"] = json!(error);
    }
    if let Some(adapter) = &self.adapter {
      report["adapter"] = json!(adapter);
    }
    if self.scenario == "gpu" {
      report["decoderSamples"] = json!(self.decoder_samples);
      report["hwdecOverride"] = json!(self.hwdec);
    }
    if let Some((width, height)) = self.physical_size {
      report["physicalSize"] = json!({"width": width, "height": height});
    }
    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    let temporary = self.report.with_extension(format!("{}.tmp", self.run_id));
    std::fs::write(&temporary, bytes)
      .and_then(|()| std::fs::rename(&temporary, &self.report))
      .map_err(|error| format!("Cannot write regression report: {error}"))
  }
}

pub(crate) fn initialize(arguments: &[String]) -> Result<(), String> {
  let value = |flag: &str| -> Result<Option<String>, String> {
    let indices: Vec<_> = arguments
      .iter()
      .enumerate()
      .filter(|(_, arg)| *arg == flag)
      .collect();
    if indices.len() > 1 {
      return Err(format!("Duplicate {flag}"));
    }
    indices
      .first()
      .map(|(index, _)| {
        arguments
          .get(index + 1)
          .filter(|value| !value.starts_with("--"))
          .cloned()
          .ok_or_else(|| format!("Missing {flag} value"))
      })
      .transpose()
  };
  let scenario = value("--native-regression")?;
  let report = value("--regression-report")?;
  let media = value("--regression-media")?;
  let run_id = value("--regression-run-id")?;
  let hwdec = value("--regression-hwdec")?;
  let Some(scenario) = scenario else {
    if report.is_some() || media.is_some() || run_id.is_some() || hwdec.is_some() {
      return Err("Regression arguments require --native-regression".into());
    }
    return Ok(());
  };
  let report = PathBuf::from(report.ok_or("--native-regression requires --regression-report")?);
  if !report.is_absolute() {
    return Err("--regression-report must be absolute".into());
  }
  let run_id = run_id.ok_or("--native-regression requires --regression-run-id")?;
  if run_id.len() != 36
    || !run_id.bytes().enumerate().all(|(index, byte)| {
      if matches!(index, 8 | 13 | 18 | 23) {
        byte == b'-'
      } else {
        byte.is_ascii_hexdigit()
      }
    })
  {
    return Err("--regression-run-id must be a UUID".into());
  }
  let run = Run {
    scenario,
    run_id,
    unavailable: false,
    physical_size: None,
    video_size: iced::Size::new(480.0, 320.0),
    report,
    media: media.map(PathBuf::from),
    hwdec,
    checks: Vec::new(),
    decoder_samples: Vec::new(),
    error: Some("Native regression has not completed".into()),
    adapter: None,
    action: Action::None,
    hosts: 0,
    dropped_hosts: 0,
    renderers: 0,
    surfaces: 0,
    closed: false,
    complete: false,
    finished: false,
  };
  run.save(false)?;
  RUN
    .set(Mutex::new(run))
    .map_err(|_| "Regression already initialized")?;
  let validation = with_run(|run| {
    if !matches!(run.scenario.as_str(), "tray" | "gpu") {
      return Err("Native scenario must be tray or gpu".to_owned());
    }
    if !arguments.iter().any(|arg| arg == "--smoke-test")
      || !arguments.iter().any(|arg| arg == "--embedded")
      || arguments.iter().any(|arg| arg == "--external")
    {
      return Err(
        "Native regressions require --smoke-test --embedded and forbid --external".into(),
      );
    }
    if !cfg!(target_os = "linux") {
      run.unavailable = true;
      return Err("Native regressions require Linux Vulkan".into());
    }
    if run.scenario == "gpu" {
      if run
        .hwdec
        .as_deref()
        .is_some_and(|value| !matches!(value, "no" | "vaapi" | "vaapi-copy"))
      {
        return Err("--regression-hwdec must be no, vaapi or vaapi-copy".into());
      }
      let media = run
        .media
        .as_ref()
        .ok_or("GPU regression requires --regression-media")?;
      if !media.is_absolute() || !media.is_file() || media.to_str().is_none() {
        run.unavailable = true;
        return Err("Regression media must be an absolute UTF-8 regular file".into());
      }
    } else if run.media.is_some() || run.hwdec.is_some() {
      return Err("--regression-media and --regression-hwdec are only valid for gpu".into());
    }
    run.error = None;
    Ok(())
  })
  .unwrap_or(Ok(()));
  if let Err(error) = validation {
    fail(error.clone());
    return Err(error);
  }
  std::thread::spawn(|| {
    std::thread::sleep(Duration::from_secs(120));
    let expired = with_run(|run| !run.finished).unwrap_or(false);
    if expired {
      fail("Native regression exceeded its 120-second process deadline".into());
      std::process::exit(1);
    }
  });
  Ok(())
}

pub(crate) fn active() -> bool {
  RUN.get().is_some()
}
pub(crate) fn gpu() -> bool {
  with_run(|run| run.scenario == "gpu").unwrap_or(false)
}
pub(crate) fn host_extra_args() -> Vec<String> {
  with_run(|run| {
    run
      .hwdec
      .as_ref()
      .filter(|_| run.scenario == "gpu")
      .map(|value| vec![format!("--hwdec={value}")])
      .unwrap_or_default()
  })
  .unwrap_or_default()
}
pub(crate) fn gpu_size() -> Option<iced::Size> {
  with_run(|run| (run.scenario == "gpu").then_some(run.video_size)).flatten()
}
pub(crate) fn tray() -> bool {
  with_run(|run| run.scenario == "tray").unwrap_or(false)
}
pub(crate) fn check(check: &str) {
  with_run(|run| {
    run.checks.push(check.into());
    let _ = run.save(false);
  });
}
pub(crate) fn fail(error: String) {
  with_run(|run| {
    run.error = Some(error);
    run.action = Action::Exit;
    let _ = run.save(false);
  });
}
pub(crate) fn unavailable(error: String) {
  with_run(|run| run.unavailable = true);
  fail(error);
}

pub(crate) fn finish(
  result: Result<(), Box<dyn std::error::Error + Send + Sync>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  if !active() {
    return result;
  }
  with_run(|run| {
    if let Err(error) = result {
      run.unavailable = run.hosts == 0;
      run.error.get_or_insert_with(|| error.to_string());
    }
    if !run.complete && run.error.is_none() {
      run.error = Some("Daemon exited before all native phases completed".into());
    }
    if run.surfaces != 0 && run.error.is_none() {
      run.error = Some("Daemon returned with live native surfaces".into());
    }
    if (run.hosts != 1 || run.dropped_hosts != 1) && run.error.is_none() {
      run.error =
        Some("Daemon did not create and release exactly one retained embedded host".into());
    }
    if crate::embedded::options().is_some_and(|options| options.ipc.exists()) && run.error.is_none()
    {
      run.error = Some("Embedded IPC socket remained after daemon cleanup".into());
    }
    if run.error.is_none() {
      run
        .checks
        .push("daemon exited and retained compositor cleanup completed".into());
    }
    run.finished = true;
    run.save(run.error.is_none())?;
    run.error.clone().map_or(Ok(()), Err)
  })
  .unwrap_or(Ok(()))
  .map_err(Into::into)
}

pub(crate) fn subscription() -> Subscription<Message> {
  Subscription::batch([
    iced::time::every(Duration::from_millis(25))
      .map(|now| Message::Window(WindowMessage::FrameTick(now))),
    iced::event::listen_with(|event, _, _| {
      matches!(event, iced::Event::Window(iced::window::Event::Closed))
        .then_some(Message::Window(WindowMessage::ShowRequested(None)))
    }),
  ])
}

pub(crate) fn update(state: &mut crate::app::State, message: &Message) -> Option<Task<Message>> {
  if !active() {
    return None;
  }
  if let Message::Window(WindowMessage::ShowRequested(None)) = message {
    let reopen = with_run(|run| {
      if run.action != Action::Closing {
        return false;
      }
      if run.surfaces != 0 {
        run.error = Some("Last-window Closed arrived with a live surface".into());
        run.action = Action::Exit;
        return false;
      }
      run.closed = true;
      run.video_size = iced::Size::new(480.0, 320.0);
      run
        .checks
        .push("actual last window closed with zero live surfaces".into());
      run.action = Action::Reopened;
      true
    })
    .unwrap_or(false);
    if reopen {
      // Reuse the same shell ShowRequested route as the real tray action.
      state.shell.full_window_size = Some(iced::Size::new(480.0, 320.0));
      return None;
    }
  }
  if !matches!(message, Message::Window(WindowMessage::FrameTick(_))) {
    return None;
  }
  Some(
    with_run(|run| match run.action {
      Action::Exit => iced::exit(),
      Action::Resize => {
        run.action = Action::None;
        // Exercise the real shader/layout-to-copy-target resize boundary,
        // independently of a window manager's programmatic sizing policy.
        run.video_size = iced::Size::new(448.0, 288.0);
        Task::none()
      }
      Action::Close => {
        if let Some(id) = state.shell.window_id.take() {
          run.action = Action::Closing;
          iced::window::close(id)
        } else {
          Task::none()
        }
      }
      _ => Task::none(),
    })
    .unwrap_or_else(Task::none),
  )
}

#[cfg(target_os = "linux")]
pub(crate) fn host_created(adapter: String) {
  with_run(|run| {
    run.hosts += 1;
    run.adapter = Some(adapter);
  });
}
#[cfg(target_os = "linux")]
pub(crate) fn compositor_dropped() {
  with_run(|run| run.dropped_hosts += 1);
}
#[cfg(target_os = "linux")]
pub(crate) fn renderer_created() {
  with_run(|run| run.renderers += 1);
}
#[cfg(target_os = "linux")]
pub(crate) fn surface_created() {
  with_run(|run| run.surfaces += 1);
}
#[cfg(target_os = "linux")]
pub(crate) fn surface_dropped() {
  with_run(|run| run.surfaces = run.surfaces.saturating_sub(1));
}
#[cfg(target_os = "linux")]
pub(crate) fn tray_presented() {
  if tray() {
    with_run(|run| {
      if !run.complete && run.error.is_none() {
        run.checks.push(
          "embedded compositor created and native frame presented after real tray initialization"
            .into(),
        );
        run.complete = true;
        run.action = Action::Exit;
      }
    });
  }
}

#[cfg(target_os = "linux")]
mod gpu_probe {
  use super::*;
  use iced::advanced::graphics::Viewport;
  use iced::advanced::Renderer as _;
  use std::io::{BufRead, BufReader, Write};
  use std::os::unix::net::UnixStream;
  use std::time::Instant;

  pub(crate) struct Probe {
    ipc: BufReader<UnixStream>,
    request: u64,
    phase: u8,
    deadline: Instant,
    since: Instant,
    clock: f64,
    seek: f64,
    pixels: Vec<u8>,
    generation: u64,
    copied: bool,
    copy_count: u64,
    upload: Option<std::sync::mpsc::Receiver<bool>>,
    image: Option<iced::advanced::image::Handle>,
  }
  impl Probe {
    pub(crate) fn new() -> Result<Self, String> {
      let options = crate::embedded::options().ok_or("Embedded options unavailable")?;
      let stream =
        UnixStream::connect(&options.ipc).map_err(|_| "Cannot connect native regression IPC")?;
      stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
      stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
      let mut probe = Self {
        ipc: BufReader::new(stream),
        request: 0,
        phase: 0,
        deadline: Instant::now() + Duration::from_secs(20),
        since: Instant::now(),
        clock: 0.0,
        seek: 0.0,
        pixels: Vec::new(),
        generation: 0,
        copied: false,
        copy_count: 0,
        upload: None,
        image: None,
      };
      probe.command(json!(["set_property", "pause", true]))?;
      let media = with_run(|run| run.media.clone())
        .flatten()
        .ok_or("Missing regression media")?;
      probe.command(json!(["loadfile", media, "replace"]))?;
      Ok(probe)
    }
    fn command(&mut self, command: serde_json::Value) -> Result<serde_json::Value, String> {
      self.request += 1;
      let bytes = serde_json::to_vec(&json!({"command": command, "request_id": self.request}))
        .map_err(|e| e.to_string())?;
      self
        .ipc
        .get_mut()
        .write_all(&bytes)
        .and_then(|()| self.ipc.get_mut().write_all(b"\n"))
        .map_err(|_| "Native IPC write failed")?;
      let end = Instant::now() + Duration::from_secs(2);
      loop {
        if Instant::now() >= end {
          return Err("Native IPC acknowledgement deadline exceeded".into());
        }
        let mut line = String::new();
        if self
          .ipc
          .read_line(&mut line)
          .map_err(|_| "Native IPC response timed out")?
          == 0
        {
          return Err("Native IPC disconnected".into());
        }
        let value: serde_json::Value =
          serde_json::from_str(&line).map_err(|_| "Invalid native IPC response")?;
        if value["request_id"].as_u64() != Some(self.request) {
          continue;
        }
        if value["error"] != "success" {
          return Err("Native IPC command was rejected".into());
        }
        return Ok(value["data"].clone());
      }
    }
    fn clock(&mut self) -> Result<f64, String> {
      self
        .command(json!(["get_property", "time-pos"]))?
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or("Media clock unavailable".into())
    }
    fn sample_decoder(&mut self, stage: &str) -> Result<(), String> {
      let mut sample = json!({ "stage": stage, "copyCount": self.copy_count,
        "copiedSincePhaseStart": self.copied });
      // Query the embedded client's actual decoder at settled lifecycle boundaries;
      // the requested hwdec option alone cannot distinguish direct decoding from copy.
      for property in [
        "hwdec",
        "hwdec-current",
        "video-params",
        "video-out-params",
        "options/ao",
        "current-ao",
        "audio-params",
        "audio-out-params",
        "time-pos",
        "pause",
        "seeking",
        "video-pts",
        "osd-level",
        "decoder-frame-drop-count",
        "frame-drop-count",
      ] {
        sample[property] = match self.command(json!(["get_property", property])) {
          Ok(value) => json!({ "status": "available", "value": value }),
          Err(error) => json!({ "status": "unavailable", "error": error }),
        };
      }
      // A strict baseline AO selection must not pass after mpv disables audio
      // or chooses another backend. A trailing empty entry permits fallback.
      let requested = &sample["options/ao"]["value"];
      let actual = &sample["current-ao"]["value"];
      let mismatch = requested.as_array().and_then(|outputs| {
        if outputs.len() == 1 {
          outputs[0]["name"].as_str().filter(|name| !name.is_empty() && *name != "null")
        } else {
          None
        }
      }).filter(|expected| actual.as_str() != Some(*expected))
        .map(|expected| format!("Requested audio backend {expected} is not active at {stage}; check session socket access"));
      with_run(|run| run.decoder_samples.push(sample));
      mismatch.map_or(Ok(()), Err)
    }
    fn advance(&mut self, phase: u8) {
      self.phase = phase;
      self.since = Instant::now();
      self.deadline = self.since + Duration::from_secs(20);
      self.copied = false;
    }
    pub(crate) fn present(
      &mut self,
      renderer: &mut iced_wgpu::Renderer,
      viewport: &Viewport,
      background: iced::Color,
      copied: bool,
      generation: u64,
    ) -> Result<(), String> {
      if self.phase == 9 {
        return Ok(());
      }
      if Instant::now() > self.deadline {
        return Err(format!("GPU phase {} exceeded its 20-second deadline; use moving, nonblack media longer than four seconds", self.phase));
      }
      self.copied |= copied;
      self.copy_count += u64::from(copied);
      let size = viewport.physical_size();
      with_run(|run| run.physical_size = Some((size.width, size.height)));
      if size.width > 1024 || size.height > 768 {
        return Err(format!(
          "Regression physical viewport {}x{} exceeds bounded 1024x768 readback limit",
          size.width, size.height
        ));
      }
      let phase = self.phase;
      match phase {
        0 if self.copied => {
          let duration = self
            .command(json!(["get_property", "duration"]))?
            .as_f64()
            .filter(|value| value.is_finite() && *value > 4.0)
            .ok_or("Regression media must be longer than four seconds")?;
          self.seek = (duration * 0.5).min(10.0);
          self.command(json!(["seek", 1.0, "absolute+exact"]))?;
          self.advance(8);
        }
        8 if self.copied && (self.clock()? - 1.0).abs() < 0.25 => {
          let pixels = renderer.screenshot(viewport, background);
          if !nonuniform(&pixels) {
            return Ok(());
          }
          // The seek acknowledgement/clock precedes scheduler-released copies.
          // Establish a settled picture before measuring the paused hold; the
          // overall phase deadline still rejects a picture that never settles.
          if pixels != self.pixels || generation != self.generation {
            self.pixels = pixels;
            self.generation = generation;
            self.since = Instant::now();
            return Ok(());
          }
          if self.since.elapsed() < Duration::from_millis(500) {
            return Ok(());
          }
          self.clock = self.clock()?;
          check("media loaded, decoded, copied and rendered through the video shader");
          self.sample_decoder("loaded-paused")?;
          self.advance(1);
        }
        1 if self.since.elapsed() >= Duration::from_millis(500) => {
          if (self.clock()? - self.clock).abs() > 0.05 {
            return Err("Paused media clock advanced".into());
          }
          let pixels = renderer.screenshot(viewport, background);
          if pixels != self.pixels {
            let changed = pixels
              .iter()
              .zip(&self.pixels)
              .filter(|(a, b)| a != b)
              .count();
            let max_delta = pixels
              .iter()
              .zip(&self.pixels)
              .map(|(a, b)| a.abs_diff(*b))
              .max();
            let mut bounds = [size.width, size.height, 0, 0];
            let mut changed_pixels = 0;
            for (index, (new, old)) in pixels
              .as_chunks::<4>()
              .0
              .iter()
              .zip(self.pixels.as_chunks::<4>().0)
              .enumerate()
            {
              if new != old {
                let index = u32::try_from(index).map_err(|_| "Readback pixel index overflow")?;
                let x = index % size.width;
                let y = index / size.width;
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
                changed_pixels += 1;
              }
            }
            self.sample_decoder("paused-mismatch")?;
            return Err(format!("Paused copied frame changed without seek: readback bytes {} -> {}, destination generation {} -> {}, changed bytes {changed}, changed pixels {changed_pixels}, max byte delta {max_delta:?}, diff bounds {bounds:?}, copied this present {copied}, total copies {}", self.pixels.len(), pixels.len(), self.generation, generation, self.copy_count));
          }
          check("paused clock and rendered frame remained stable for 500ms");
          self.command(json!(["seek", self.seek, "absolute+exact"]))?;
          self.advance(2);
        }
        2 if self.copied && (self.clock()? - self.seek).abs() < 0.25 => {
          let pixels = renderer.screenshot(viewport, background);
          if pixels == self.pixels || !nonuniform(&pixels) {
            return Ok(());
          }
          check("acknowledged paused seek changed actual shader readback");
          self.sample_decoder("paused-seek")?;
          self.clock = self.clock()?;
          self.generation = generation;
          with_run(|run| run.action = Action::Resize);
          self.advance(3);
        }
        3 if self.copied && generation != self.generation => {
          if (self.clock()? - self.clock).abs() > 0.05 {
            return Err("Paused resize advanced media clock".into());
          }
          self.pixels = renderer.screenshot(viewport, background);
          if !nonuniform(&self.pixels) {
            return Ok(());
          }
          check("paused video-region resize replaced and refreshed the copied destination");
          self.sample_decoder("paused-resize")?;
          self.command(json!(["set_property", "pause", false]))?;
          let image = iced::advanced::image::Handle::from_rgba(64, 64, vec![127; 64 * 64 * 4]);
          let (sender, receiver) = std::sync::mpsc::channel();
          renderer.allocate_image(&image, move |allocation| {
            let _ = sender.send(allocation.is_ok());
          });
          self.image = Some(image);
          self.upload = Some(receiver);
          self.advance(4);
        }
        4 if self.since.elapsed() >= Duration::from_millis(500)
          && self.clock()? > self.clock + 0.2
          && self.copied =>
        {
          let pixels = renderer.screenshot(viewport, background);
          if pixels == self.pixels || !nonuniform(&pixels) {
            return Ok(());
          }
          let uploaded = self
            .upload
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
          match uploaded {
            Some(true) => {}
            Some(false) => return Err("Concurrent renderer image upload failed".into()),
            None => return Ok(()),
          }
          check("resumed mpv clock, frame copies and shader pixels advanced while renderer image upload callback completed");
          self.sample_decoder("resumed")?;
          self.generation = generation;
          with_run(|run| run.action = Action::Close);
          self.advance(5);
        }
        5 if self.copied
          && generation != self.generation
          && with_run(|run| run.closed && run.action == Action::Reopened).unwrap_or(false) =>
        {
          if !with_run(|run| run.hosts == 1 && run.renderers == 2 && run.surfaces == 1)
            .unwrap_or(false)
          {
            return Err("Reopen did not retain exactly one host with a new renderer".into());
          }
          self.pixels = renderer.screenshot(viewport, background);
          if !nonuniform(&self.pixels) {
            return Ok(());
          }
          self.clock = self.clock()?;
          self.advance(6);
        }
        6 if self.copied && self.clock()? > self.clock + 0.2 => {
          let pixels = renderer.screenshot(viewport, background);
          if pixels == self.pixels || !nonuniform(&pixels) {
            return Ok(());
          }
          check("last-window reopen retained the IPC session and host; new renderer displayed advancing video");
          self.sample_decoder("reopened")?;
          self.command(json!(["stop"]))?;
          self.advance(7);
        }
        7 => {
          if self.command(json!(["get_property", "idle-active"]))? != true {
            return Ok(());
          }
          check("native stop acknowledged and idle state observed");
          with_run(|run| {
            run.complete = true;
            run.action = Action::Exit;
          });
          self.advance(9);
        }
        _ => {}
      }
      Ok(())
    }
  }
  fn nonuniform(pixels: &[u8]) -> bool {
    let Some(first) = pixels.get(..4) else {
      return false;
    };
    pixels
      .as_chunks::<4>()
      .0
      .iter()
      .any(|pixel| pixel[..3] != first[..3])
  }
}
#[cfg(target_os = "linux")]
pub(crate) use gpu_probe::Probe;
