//! Explicit, bounded native probes. No probe is selected by environment variables.
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::app::message::{Message, WindowMessage};
use iced::{Subscription, Task};
use serde_json::json;

static RUN: OnceLock<Mutex<Run>> = OnceLock::new();

struct Sustained {
  requested: Duration,
  started: Instant,
  completed_cycles: u32,
  finished_at: Option<Instant>,
}

impl Sustained {
  fn parse(value: &str) -> Result<Self, String> {
    let seconds = value
      .bytes()
      .all(|byte| byte.is_ascii_digit())
      .then(|| value.parse::<u64>().ok())
      .flatten()
      .filter(|seconds| (1..=3600).contains(seconds))
      .ok_or("--regression-duration-secs requires an integer from 1 through 3600")?;
    Ok(Self {
      requested: Duration::from_secs(seconds),
      started: Instant::now(),
      completed_cycles: 0,
      finished_at: None,
    })
  }

  fn complete_cycle(&mut self, now: Instant) -> bool {
    self.completed_cycles += 1;
    if now.duration_since(self.started) >= self.requested {
      self.finished_at = Some(now);
      true
    } else {
      false
    }
  }

  fn elapsed(&self) -> Duration {
    self
      .finished_at
      .unwrap_or_else(Instant::now)
      .duration_since(self.started)
  }
}

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
  requested_host_abi: &'static str,
  actual_host_abi: Option<u32>,
  action: Action,
  hosts: usize,
  dropped_hosts: usize,
  renderers: usize,
  surfaces: usize,
  closed: bool,
  complete: bool,
  finished: bool,
  started: Instant,
  host_started: Option<Instant>,
  sustained: Option<Sustained>,
  phase: u8,
  phase_deadline: Option<Instant>,
  last_present: Option<Instant>,
  cycle: u32,
  phase_history: Vec<serde_json::Value>,
  presentations: u64,
  copied_frames: u64,
  pip: PipProbe,
}

#[derive(Clone, Debug)]
pub(crate) enum NativeMessage {
  WindowOpened(iced::window::Id),
  TargetResolved(
    iced::window::Id,
    Option<crate::embedded::PresentationWindow>,
  ),
}

#[derive(Default)]
struct PipProbe {
  main: Option<iced::window::Id>,
  window: Option<iced::window::Id>,
  main_target: Option<crate::embedded::PresentationWindow>,
  target: Option<crate::embedded::PresentationWindow>,
  displaying: Option<iced::window::Id>,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
  None,
  Resize,
  Close,
  Closing,
  Reopened,
  Exit,
  PipOpen,
  PipSwitch,
  PipPlaying,
  PipReturn,
  PipClosing,
  PipReturned,
}

fn with_run<T>(f: impl FnOnce(&mut Run) -> T) -> Option<T> {
  RUN.get().map(|run| {
    f(&mut run
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner))
  })
}

impl Run {
  fn advance_phase(&mut self, phase: u8, now: Instant) {
    self.phase = phase;
    self.phase_deadline =
      if phase == 9 || self.complete || self.finished || self.action == Action::Exit {
        None
      } else {
        let dwell = self
          .sustained
          .as_ref()
          .filter(|_| phase == 4)
          .map_or(Duration::ZERO, |sustained| {
            sustained.requested.min(Duration::from_secs(30))
          });
        Some(now + Duration::from_secs(20) + dwell)
      };
  }

  fn phase_timeout(&self, now: Instant) -> Option<String> {
    if self.scenario != "gpu" || self.complete || self.finished || self.action == Action::Exit {
      return None;
    }
    self
      .phase_deadline
      .filter(|deadline| now > *deadline)
      .map(|_| format!("GPU phase {} exceeded its bounded deadline", self.phase))
  }

  fn record_failure(&mut self, error: String, now: Instant) -> bool {
    if self.finished || (self.action == Action::Exit && self.error.is_some()) {
      return false;
    }
    self.error = Some(if self.scenario == "gpu" {
      format!(
        "{error}; GPU progress: phase={} cycle={} last_present_age={:?} presentations={} copies={} live_surfaces={}",
        self.phase,
        self.cycle,
        self.last_present.map(|last| now.saturating_duration_since(last)),
        self.presentations,
        self.copied_frames,
        self.surfaces,
      )
    } else {
      error
    });
    self.phase_deadline = None;
    self.action = Action::Exit;
    true
  }

  fn check_phase_deadline(&mut self, now: Instant) -> bool {
    self
      .phase_timeout(now)
      .is_some_and(|error| self.record_failure(error, now))
  }

  fn save(&self, passed: bool) -> Result<(), String> {
    let mut report = json!({"schemaVersion": 1, "scenario": self.scenario, "runId": self.run_id,
      "status": if passed { "pass" } else if self.unavailable { "unavailable" } else { "fail" }, "checks": self.checks});
    report["requestedHostAbi"] = json!(self.requested_host_abi);
    report["actualHostAbi"] = json!(self.actual_host_abi);
    report["processId"] = json!(std::process::id());
    report["hostsCreated"] = json!(self.hosts);
    report["hostsDropped"] = json!(self.dropped_hosts);
    report["hostIdentity"] = json!(format!(
      "{}:{}:{}",
      self.run_id,
      std::process::id(),
      self.hosts
    ));
    report["phase"] = json!(self.phase);
    report["cycle"] = json!(self.cycle);
    report["phaseHistory"] = json!(self.phase_history);
    report["presentations"] = json!(self.presentations);
    report["copiedFrames"] = json!(self.copied_frames);
    report["lastPresentElapsedSeconds"] = json!(self
      .last_present
      .map(|last| { last.saturating_duration_since(self.started).as_secs_f64() }));
    report["sustained"] = self
      .sustained
      .as_ref()
      .map_or(serde_json::Value::Null, |sustained| {
        json!({
          "requestedSeconds": sustained.requested.as_secs(),
          "elapsedSeconds": sustained.elapsed().as_secs_f64(),
          "completedCycles": sustained.completed_cycles
        })
      });
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
  let duration = value("--regression-duration-secs")?;
  let Some(scenario) = scenario else {
    if report.is_some()
      || media.is_some()
      || run_id.is_some()
      || hwdec.is_some()
      || duration.is_some()
    {
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
  let requested_host_abi = match std::env::var("JELLYPILOT_MPV_HOST_ABI") {
    Err(std::env::VarError::NotPresent) => "legacy",
    Ok(value) if value == "legacy" => "legacy",
    Ok(value) if value == "3" => "3",
    _ => return Err("JELLYPILOT_MPV_HOST_ABI must be legacy or 3".into()),
  };
  let sustained = duration.as_deref().map(Sustained::parse).transpose()?;
  if sustained.is_some() && scenario != "gpu" {
    return Err("--regression-duration-secs is only valid for gpu".into());
  }
  let process_timeout = sustained
    .as_ref()
    .map_or(120, |run| run.requested.as_secs() + 120);
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
    requested_host_abi,
    actual_host_abi: None,
    action: Action::None,
    pip: PipProbe::default(),
    hosts: 0,
    dropped_hosts: 0,
    renderers: 0,
    surfaces: 0,
    closed: false,
    complete: false,
    finished: false,
    started: Instant::now(),
    host_started: None,
    sustained,
    phase: 0,
    phase_deadline: None,
    last_present: None,
    cycle: 1,
    phase_history: Vec::new(),
    presentations: 0,
    copied_frames: 0,
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
  std::thread::spawn(move || {
    std::thread::sleep(Duration::from_secs(process_timeout));
    let expired = with_run(|run| !run.finished).unwrap_or(false);
    if expired {
      fail(format!(
        "Native regression exceeded its {process_timeout}-second process deadline"
      ));
      #[expect(
        clippy::disallowed_methods,
        reason = "native regression watchdog terminates a potentially wedged compositor event loop after recording the failure report"
      )]
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
pub(crate) fn gpu_size_for(id: iced::window::Id) -> Option<iced::Size> {
  with_run(|run| {
    (run.scenario == "gpu" && run.pip.displaying.is_none_or(|owner| owner == id))
      .then_some(run.video_size)
  })
  .flatten()
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
    if run.record_failure(error, Instant::now()) {
      let _ = run.save(false);
    }
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
    if run.sustained.as_ref().is_some_and(|sustained| {
      sustained.finished_at.is_none()
        || sustained.elapsed() < sustained.requested
        || sustained.completed_cycles == 0
    }) && run.error.is_none()
    {
      run.error =
        Some("Sustained GPU regression exited before its duration and complete cycle".into());
    }
    if (run.hosts != 1 || run.dropped_hosts != 1) && run.error.is_none() {
      run.error =
        Some("Daemon did not create and release exactly one retained embedded host".into());
    }
    if crate::embedded::options()
      .is_some_and(|options| crate::embedded::ipc_endpoint_alive(&options.ipc))
      && run.error.is_none()
    {
      run.error = Some("Embedded IPC socket remained after daemon cleanup".into());
    }
    if run.error.is_none() {
      run
        .checks
        .push("daemon exited and retained compositor cleanup completed".into());
    }
    run.finished = true;
    run.phase_deadline = None;
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
  if let Message::NativeProbe(message) = message {
    return Some(
      with_run(|run| {
        match *message {
          NativeMessage::WindowOpened(id) if run.pip.window == Some(id) => {
            return crate::embedded::presentation_window(id)
              .map(move |target| Message::NativeProbe(NativeMessage::TargetResolved(id, target)));
          }
          NativeMessage::TargetResolved(id, target) if run.action == Action::PipSwitch => {
            if run.pip.main != Some(id) && run.pip.window != Some(id) {
              return Task::none();
            }
            let Some(target) = target else {
              run.record_failure("PiP native target was not usable".into(), Instant::now());
              return Task::none();
            };
            if run.pip.main == Some(id) {
              run.pip.main_target = Some(target);
            } else {
              run.pip.target = Some(target);
            }
            if let (Some(_), Some(target)) = (run.pip.main_target, run.pip.target) {
              if crate::embedded::set_presentation_window(Some(target)) {
                run.pip.displaying = run.pip.window;
                run.video_size = iced::Size::new(320.0, 180.0);
                run.action = Action::PipPlaying;
              } else {
                run.record_failure("PiP target selection failed".into(), Instant::now());
              }
            }
          }
          _ => {}
        }
        Task::none()
      })
      .unwrap_or_else(Task::none),
    );
  }
  if let Message::Window(WindowMessage::ShowRequested(None)) = message {
    let reopen = with_run(|run| {
      if run.action != Action::Closing {
        return false;
      }
      if run.surfaces != 0 {
        run.record_failure(
          "Last-window Closed arrived with a live surface".into(),
          Instant::now(),
        );
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
  #[cfg(target_os = "linux")]
  if gpu() {
    gpu_probe::drive_idle_session(state);
  }
  // Presentation may be gated by the compositor; the existing tick must still
  // enforce the same phase deadline. Keep the idle driver's earlier failure.
  with_run(|run| {
    if run.check_phase_deadline(Instant::now()) {
      let _ = run.save(false);
    }
  });
  Some(
    with_run(|run| match run.action {
      Action::Exit => iced::exit(),
      Action::PipOpen => {
        let Some(main) = state.shell.window_id else {
          run.record_failure("PiP probe lost the main window".into(), Instant::now());
          return Task::none();
        };
        let (id, open) = iced::window::open(iced::window::Settings {
          size: iced::Size::new(320.0, 180.0),
          min_size: Some(iced::Size::new(320.0, 180.0)),
          max_size: Some(iced::Size::new(320.0, 180.0)),
          resizable: false,
          exit_on_close_request: false,
          ..Default::default()
        });
        run.pip = PipProbe {
          main: Some(main),
          window: Some(id),
          displaying: Some(main),
          ..Default::default()
        };
        run.action = Action::PipSwitch;
        Task::batch([
          open.map(|id| Message::NativeProbe(NativeMessage::WindowOpened(id))),
          crate::embedded::presentation_window(main)
            .map(move |target| Message::NativeProbe(NativeMessage::TargetResolved(main, target))),
        ])
      }
      Action::PipReturn => {
        if run
          .pip
          .main_target
          .is_some_and(|target| crate::embedded::set_presentation_window(Some(target)))
        {
          run.pip.displaying = run.pip.main;
          run.video_size = iced::Size::new(448.0, 288.0);
          run.action = Action::PipReturned;
        } else {
          run.record_failure(
            "PiP could not return to the live main surface".into(),
            Instant::now(),
          );
        }
        Task::none()
      }
      Action::PipClosing => {
        run.action = Action::None;
        run
          .pip
          .window
          .take()
          .map_or_else(Task::none, iced::window::close)
      }
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

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn host_created(adapter: String, host_abi: u32) {
  with_run(|run| {
    run.hosts += 1;
    run.host_started.get_or_insert_with(Instant::now);
    if run.hosts != 1 {
      run.record_failure(
        "GPU regression replaced its retained embedded host".into(),
        Instant::now(),
      );
    }
    run.adapter = Some(adapter);
    run.actual_host_abi = Some(host_abi);
    let matches = if run.requested_host_abi == "3" {
      host_abi == 3
    } else {
      matches!(host_abi, 1 | 2)
    };
    if matches {
      run.checks.push(format!(
        "requested host ABI {} matched actual ABI {host_abi}",
        run.requested_host_abi
      ));
    } else {
      run.record_failure(
        format!(
          "Requested host ABI {}, but embedded host registered ABI {host_abi}",
          run.requested_host_abi
        ),
        Instant::now(),
      );
    }
  });
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn compositor_dropped() {
  with_run(|run| run.dropped_hosts += 1);
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn renderer_created() {
  with_run(|run| run.renderers += 1);
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn surface_created() {
  with_run(|run| run.surfaces += 1);
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn surface_dropped() {
  with_run(|run| run.surfaces = run.surfaces.saturating_sub(1));
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
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

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod gpu_probe {
  use super::*;
  use iced::advanced::graphics::Viewport;
  use iced::advanced::Renderer as _;
  use std::io::{BufRead, BufReader, Write};
  #[cfg(unix)]
  use std::os::unix::net::UnixStream;
  use std::time::Instant;

  #[cfg(unix)]
  type IpcStream = UnixStream;
  #[cfg(windows)]
  type IpcStream = std::fs::File;

  #[cfg(unix)]
  fn ipc_connect() -> Result<BufReader<IpcStream>, String> {
    let options = crate::embedded::options().ok_or("Embedded options unavailable")?;
    let stream =
      UnixStream::connect(&options.ipc).map_err(|_| "Cannot connect native regression IPC")?;
    stream
      .set_read_timeout(Some(Duration::from_secs(2)))
      .map_err(|e| e.to_string())?;
    stream
      .set_write_timeout(Some(Duration::from_secs(2)))
      .map_err(|e| e.to_string())?;
    Ok(BufReader::new(stream))
  }

  fn ipc_command(
    ipc: &mut BufReader<IpcStream>,
    request: &mut u64,
    command: serde_json::Value,
  ) -> Result<serde_json::Value, String> {
    // loadfile success only acknowledges enqueueing. A previous cycle can still
    // supply a copied frame while the replacement file's properties are absent.
    let loading = command[0] == "loadfile";
    let mut loaded = !loading;
    let mut acknowledged = None;
    *request += 1;
    let bytes = serde_json::to_vec(&json!({"command": command, "request_id": *request}))
      .map_err(|e| e.to_string())?;
    ipc
      .get_mut()
      .write_all(&bytes)
      .and_then(|()| ipc.get_mut().write_all(b"\n"))
      .map_err(|_| "Native IPC write failed")?;
    let end = Instant::now() + Duration::from_secs(if loading { 20 } else { 2 });
    loop {
      if Instant::now() >= end {
        return Err("Native IPC acknowledgement deadline exceeded".into());
      }
      let mut line = String::new();
      if ipc
        .read_line(&mut line)
        .map_err(|_| "Native IPC response timed out")?
        == 0
      {
        return Err("Native IPC disconnected".into());
      }
      let value: serde_json::Value =
        serde_json::from_str(&line).map_err(|_| "Invalid native IPC response")?;
      if loading && value["event"] == "file-loaded" {
        loaded = true;
      }
      if loading && value["event"] == "end-file" && value["reason"] == "error" {
        return Err("Native regression media failed to load".into());
      }
      if value["request_id"].as_u64() == Some(*request) {
        if value["error"] != "success" {
          return Err(format!(
            "Native IPC command was rejected: {}",
            value["error"]
          ));
        }
        acknowledged = Some(value["data"].clone());
      }
      if loaded {
        if let Some(data) = acknowledged.take() {
          return Ok(data);
        }
      }
    }
  }

  pub(crate) struct Probe {
    ipc: BufReader<IpcStream>,
    request: u64,
    phase: u8,
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
      #[cfg(unix)]
      let ipc = ipc_connect()?;
      // Windows named pipes are opened as files; blocking I/O has no per-read
      // timeout. The per-phase deadline still applies between commands and the
      // runner's outer process timeout remains the final bound.
      #[cfg(windows)]
      let ipc = {
        let options = crate::embedded::options().ok_or("Embedded options unavailable")?;
        let stream = std::fs::OpenOptions::new()
          .read(true)
          .write(true)
          .open(&options.ipc)
          .map_err(|_| "Cannot connect native regression IPC")?;
        BufReader::new(stream)
      };
      let mut probe = Self {
        ipc,
        request: 0,
        phase: 0,
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
      with_run(|run| {
        if let Some(sustained) = &mut run.sustained {
          sustained.started = Instant::now();
        }
      });
      probe.advance(0);
      probe.command(json!(["set_property", "pause", true]))?;
      // A rejected input is a recoverable playback error, not evidence of a
      // failed GPU handoff. Observe the real end-file before loading valid media.
      let missing = with_run(|run| run.report.with_extension("missing-media"))
        .ok_or("Regression report unavailable")?;
      if missing.exists() {
        return Err("Regression missing-media path already exists".into());
      }
      probe.request += 1;
      let request = probe.request;
      let bytes =
        serde_json::to_vec(&json!({"command": ["loadfile", missing], "request_id": request}))
          .map_err(|error| error.to_string())?;
      probe
        .ipc
        .get_mut()
        .write_all(&bytes)
        .and_then(|()| probe.ipc.get_mut().write_all(b"\n"))
        .map_err(|_| "Native IPC recovery request failed")?;
      let deadline = Instant::now() + Duration::from_secs(2);
      let (mut acknowledged, mut rejected) = (false, false);
      while !acknowledged || !rejected {
        if Instant::now() >= deadline {
          return Err("Missing-media rejection deadline exceeded".into());
        }
        let mut line = String::new();
        if probe
          .ipc
          .read_line(&mut line)
          .map_err(|_| "Missing-media rejection response timed out")?
          == 0
        {
          return Err("Native IPC disconnected during recovery probe".into());
        }
        let event: serde_json::Value =
          serde_json::from_str(&line).map_err(|_| "Invalid native recovery response")?;
        if event["request_id"].as_u64() == Some(request) {
          if event["error"] != "success" {
            return Err("Missing-media load command was not acknowledged".into());
          }
          acknowledged = true;
        }
        if event["event"] == "end-file" {
          if event["reason"] != "error" {
            return Err("Missing media ended without a playback error".into());
          }
          rejected = true;
        }
      }
      check("missing local media produced an acknowledged playback error");
      probe.load_media()?;
      Ok(probe)
    }
    fn load_media(&mut self) -> Result<(), String> {
      let media = with_run(|run| run.media.clone())
        .flatten()
        .ok_or("Missing regression media")?;
      self.command(json!([
        "loadfile",
        media,
        "replace",
        -1,
        "cache=yes,cache-on-disk=yes,demuxer-seekable-cache=yes"
      ]))?;
      Ok(())
    }
    fn command(&mut self, command: serde_json::Value) -> Result<serde_json::Value, String> {
      ipc_command(&mut self.ipc, &mut self.request, command)
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
      with_run(|run| {
        sample["cycle"] = json!(run.cycle);
        sample["elapsedSeconds"] = json!(run.started.elapsed().as_secs_f64());
        sample["hostIdentity"] = json!(format!(
          "{}:{}:{}",
          run.run_id,
          std::process::id(),
          run.hosts
        ));
        sample["actualHostAbi"] = json!(run.actual_host_abi);
      });
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
        "demuxer-cache-state",
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
      self.copied = false;
      with_run(|run| {
        run.advance_phase(phase, self.since);
        run.phase_history.push(json!({
          "phase": phase,
          "cycle": run.cycle,
          "elapsedSeconds": run.started.elapsed().as_secs_f64(),
          "hostElapsedSeconds": run.host_started.map(|start| start.elapsed().as_secs_f64()),
          "hostsCreated": run.hosts,
          "hostsDropped": run.dropped_hosts,
          "renderersCreated": run.renderers,
          "liveSurfaces": run.surfaces,
          "actualHostAbi": run.actual_host_abi
        }));
        let _ = run.save(false);
      });
    }
    fn playback_dwell(&self) -> Duration {
      with_run(|run| {
        run
          .sustained
          .as_ref()
          .map_or(Duration::from_millis(500), |sustained| {
            sustained.requested.min(Duration::from_secs(30))
          })
      })
      .unwrap_or(Duration::from_millis(500))
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
      let now = Instant::now();
      let size = viewport.physical_size();
      with_run(|run| {
        if let Some(error) = run.phase_timeout(now) {
          return Err(error);
        }
        self.copied |= copied;
        self.copy_count += u64::from(copied);
        run.last_present = Some(now);
        run.physical_size = Some((size.width, size.height));
        run.presentations += 1;
        run.copied_frames = self.copy_count;
        Ok(())
      })
      .unwrap_or(Ok(()))?;
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
          if with_run(|run| run.sustained.is_some()).unwrap_or(false)
            && duration <= self.seek + self.playback_dwell().as_secs_f64() + 2.0
          {
            return Err("Sustained media must extend beyond the seek target plus playback dwell and two seconds for reopen".into());
          }
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
          let cache = self.command(json!(["get_property", "demuxer-cache-state"]))?;
          if cache["file-cache-bytes"].as_u64().unwrap_or(0) == 0 {
            return Err("Embedded MPV did not create a populated disk cache".into());
          }
          check("embedded MPV buffered media in its application disk cache");
          check("media loaded, decoded, copied and rendered through the video shader");
          if with_run(|run| run.cycle == 1).unwrap_or(false) {
            check("valid media rendered after a rejected load without recreating the host");
          }
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
        4 if self.since.elapsed() >= self.playback_dwell()
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
          self.clock = self.clock()?;
          with_run(|run| run.action = Action::PipOpen);
          self.advance(10);
        }
        10 if self.copied && self.clock()? > self.clock + 0.2 => {
          let ready = with_run(|run| {
            run.action == Action::PipPlaying
              && run.hosts == 1
              && run.surfaces == 2
              && run.pip.target.is_some_and(|target| {
                crate::embedded::presentation_status(target)
                  == crate::embedded::PresentationStatus::Ready
              })
          })
          .unwrap_or(false);
          if !ready || !nonuniform(&renderer.screenshot(viewport, background)) {
            return Ok(());
          }
          check("PiP presented advancing video on a second surface while retaining the main window and one MPV host");
          self.sample_decoder("picture-in-picture")?;
          self.clock = self.clock()?;
          with_run(|run| run.action = Action::PipReturn);
          self.advance(11);
        }
        11 if self.copied && self.clock()? > self.clock + 0.2 => {
          let ready = with_run(|run| {
            run.action == Action::PipReturned
              && run.hosts == 1
              && run.surfaces == 2
              && run.pip.main_target.is_some_and(|target| {
                crate::embedded::presentation_status(target)
                  == crate::embedded::PresentationStatus::Ready
              })
          })
          .unwrap_or(false);
          if !ready || !nonuniform(&renderer.screenshot(viewport, background)) {
            return Ok(());
          }
          check(
            "PiP returned advancing video to the original main surface without reloading the host",
          );
          with_run(|run| run.action = Action::PipClosing);
          self.advance(12);
        }
        12 if self.copied && with_run(|run| run.surfaces == 1).unwrap_or(false) => {
          self.generation = generation;
          crate::embedded::set_presentation_window(None);
          with_run(|run| {
            run.pip = PipProbe::default();
            run.action = Action::Close;
          });
          self.advance(5);
        }
        5 if self.copied
          && generation != self.generation
          && with_run(|run| run.closed && run.action == Action::Reopened).unwrap_or(false) =>
        {
          if !with_run(|run| {
            run.hosts == 1 && run.renderers == 2 * run.cycle as usize + 1 && run.surfaces == 1
          })
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
          if !with_run(|run| run.error.is_none()).unwrap_or(false) {
            return Ok(());
          }
          #[cfg(target_os = "linux")]
          if !idle_session_done() {
            return Ok(());
          }
          check("native stop acknowledged and idle state observed");
          let complete = with_run(|run| {
            let complete = run
              .sustained
              .as_mut()
              .is_none_or(|sustained| sustained.complete_cycle(Instant::now()));
            if complete {
              run.complete = true;
              run.action = Action::Exit;
            } else {
              run.cycle += 1;
              run.closed = false;
              run.action = Action::None;
            }
            complete
          })
          .unwrap_or(true);
          if !complete {
            self.command(json!(["set_property", "pause", true]))?;
            self.pixels.clear();
            self.image = None;
            self.upload = None;
            self.load_media()?;
            #[cfg(target_os = "linux")]
            idle_session::restart();
            self.advance(0);
            return Ok(());
          }
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

  /// Drives a real `PlaybackSession` on its own surface through
  /// `session.handle`, mirrors each projection through the production
  /// `sync_playback_projection` — the function that calls
  /// `embedded::idle::set_desired` — and asserts the resulting platform
  /// inhibitor state. Settlements carry transport state queried live from the
  /// embedded mpv over a second IPC connection; nothing fabricates playback.
  #[cfg(target_os = "linux")]
  mod idle_session {
    use super::*;
    use crate::app::kernel::Kernel;
    use crate::app::playback::{sync_playback_projection, Surface};
    use crate::app::State;
    use jellypilot_core::request_gate::RequestGate;
    use jellypilot_media_server::MediaItem;
    use jellypilot_mpv::playback::{
      NowPlayingItem, PlaybackError, PlaybackOutcome, PlaybackSnapshot, PlaybackStartOutcome,
      PlaybackStartPosition, PlaybackStopOutcome,
    };
    use jellypilot_mpv::playback_session::{
      ControllerSettlement, EffectId, IntroAvailability, PlaybackEffect, PlaybackEvent,
      PlaybackInput, PlaybackIntent,
    };
    use jellypilot_mpv::PlayerState;

    static DRIVER: Mutex<Option<Driver>> = Mutex::new(None);
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    /// Whether the session driver finished its stop-release check; the probe's
    /// final phase waits on this so the run cannot complete early.
    pub(crate) fn done() -> bool {
      DONE.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(super) fn restart() {
      *DRIVER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
      DONE.store(false, std::sync::atomic::Ordering::Release);
    }

    /// Polls the session driver once per frame tick. Errors fail the run.
    pub(crate) fn drive(state: &mut State) {
      let mut guard = DRIVER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      let driver = guard.get_or_insert_with(|| Driver::new(&mut state.kernel.request_gate));
      if let Err(error) = driver.step(&state.kernel) {
        fail(format!("Idle session driver: {error}"));
      }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Step {
      Begin,
      AwaitResume,
      CyclePause,
      CycleResume,
      AwaitReopen,
      Stop,
      Done,
    }

    struct Driver {
      ipc: Option<BufReader<IpcStream>>,
      request: u64,
      step: Step,
      deadline: Instant,
      item_id: String,
      title: String,
      runtime: f64,
      // The driver owns its own playback surface so the application's
      // periodic Tick/controller loop can never erase its session state.
      surface: Surface,
    }

    impl Driver {
      fn new(request_gate: &mut RequestGate) -> Self {
        Self {
          ipc: None,
          request: 0,
          step: Step::Begin,
          deadline: Instant::now()
            + Duration::from_secs(
              with_run(|run| if run.sustained.is_some() { 90 } else { 20 }).unwrap_or(20),
            ),
          item_id: String::new(),
          title: String::new(),
          runtime: 0.0,
          surface: Surface::new(request_gate),
        }
      }

      fn command(&mut self, command: serde_json::Value) -> Result<serde_json::Value, String> {
        let ipc = self
          .ipc
          .as_mut()
          .ok_or_else(|| "IPC not connected".to_owned())?;
        ipc_command(ipc, &mut self.request, command)
      }

      /// Best-effort property read; `None` means the value is not ready yet.
      fn property(&mut self, name: &str) -> Option<serde_json::Value> {
        self.command(json!(["get_property", name])).ok()
      }

      fn paused(&mut self) -> Option<bool> {
        self.property("pause")?.as_bool()
      }

      fn number(&mut self, name: &str) -> Option<f64> {
        self.property(name)?.as_f64().filter(|v| v.is_finite())
      }

      /// Transport state queried live from mpv for a settlement snapshot.
      /// Every required property must read back a valid value; a missing or
      /// malformed property is an explicit error, never a fabricated field.
      fn snapshot(&mut self) -> Result<PlaybackSnapshot, String> {
        let paused = self.paused().ok_or("mpv pause property unavailable")?;
        let muted = self
          .property("mute")
          .and_then(|v| v.as_bool())
          .ok_or("mpv mute property unavailable")?;
        let time_pos = self
          .number("time-pos")
          .ok_or("mpv time-pos property unavailable")?;
        let volume = self
          .number("volume")
          .ok_or("mpv volume property unavailable")?;
        Ok(PlaybackSnapshot {
          tools: Default::default(),
          now_playing: Some(NowPlayingItem {
            item_id: self.item_id.clone(),
            title: self.title.clone(),
            item_type: "Video".to_owned(),
            series_id: None,
            runtime_seconds: Some(self.runtime),
            start_position_seconds: 0.0,
            play_method: "DirectPlay".to_owned(),
            original_language: None,
          }),
          transport: PlayerState {
            connected: true,
            paused,
            muted,
            time_pos,
            duration: self.runtime,
            volume,
          },
        })
      }

      /// Feeds one input into the driver's own session and mirrors the
      /// resulting projection through the production sync path. Returns the
      /// controller effect id the input dispatched, if any.
      fn feed(&mut self, kernel: &Kernel, input: PlaybackInput) -> Option<EffectId> {
        let step = self.surface.session.handle(input, Instant::now());
        let mut controller = None;
        for effect in step.effects {
          if let PlaybackEffect::Controller(id, _) = effect {
            controller = Some(id);
          }
        }
        sync_playback_projection(&mut self.surface, kernel, false);
        controller
      }

      fn settle(&mut self, kernel: &Kernel, id: EffectId, settlement: ControllerSettlement) {
        self.feed(
          kernel,
          PlaybackInput::Event(Box::new(PlaybackEvent::ControllerSettled {
            id,
            settlement,
          })),
        );
      }

      fn expect_inhibited(&self, expected: bool, what: &str) -> Result<(), String> {
        if crate::embedded::idle::inhibited() != expected {
          return Err(format!(
            "{what}: {}",
            crate::embedded::idle::last_error().unwrap_or_else(|| "no bind error recorded".into())
          ));
        }
        Ok(())
      }

      fn step(&mut self, kernel: &Kernel) -> Result<(), String> {
        if self.step == Step::Done {
          return Ok(());
        }
        if Instant::now() > self.deadline {
          return Err(format!(
            "step {:?} exceeded its bounded cycle deadline",
            self.step_name()
          ));
        }
        match self.step {
          Step::Begin => {
            if self.ipc.is_none() {
              self.ipc = Some(ipc_connect()?);
            }
            let Some(duration) = self.number("duration") else {
              return Ok(());
            };
            if duration <= 0.0 {
              return Ok(());
            }
            self.runtime = duration;
            self.item_id = "native-regression-media".to_owned();
            self.title = self
              .property("filename")
              .and_then(|v| v.as_str().map(str::to_owned))
              .unwrap_or_else(|| "regression media".to_owned());
            self.feed(
              kernel,
              PlaybackInput::Event(Box::new(PlaybackEvent::EngineAvailability(true))),
            );
            let id = self
              .feed(
                kernel,
                PlaybackInput::Intent(Box::new(PlaybackIntent::Start {
                  item: jellypilot_mpv::playback::Playable::Media(MediaItem {
                    id: self.item_id.clone(),
                    name: self.title.clone(),
                    item_type: "Video".to_owned(),
                    series_id: None,
                    series_name: None,
                    season_name: None,
                    index_number: None,
                    parent_index_number: None,
                    run_time_ticks: Some((self.runtime * 10_000_000.0) as i64),
                    overview: None,
                    series_primary_image_tag: None,
                  }),
                  position: PlaybackStartPosition::Beginning,
                  intro: IntroAvailability {
                    mode: jellypilot_core::intro_skipper::IntroSkipMode::Off,
                    skipper_available: false,
                  },
                  selection: Box::default(),
                })),
              )
              .ok_or("Start intent produced no in-flight controller command")?;
            let snapshot = self.snapshot()?;
            self.settle(
              kernel,
              id,
              ControllerSettlement::Started(Ok(PlaybackStartOutcome {
                playback: PlaybackOutcome {
                  snapshot,
                  warnings: Vec::new(),
                },
                tracks: Err(PlaybackError::TrackUnavailable),
              })),
            );
            // The probe loaded the media paused: a paused session must not
            // hold the inhibitor.
            self.expect_inhibited(false, "paused playback session bound the idle inhibitor")?;
            check("paused playback session left the idle inhibitor unbound");
            self.step = Step::AwaitResume;
          }
          Step::AwaitResume => {
            if self.paused() != Some(false) {
              return Ok(());
            }
            let id = self
              .feed(
                kernel,
                PlaybackInput::Intent(Box::new(PlaybackIntent::SetPaused(false))),
              )
              .ok_or("resume intent produced no in-flight command")?;
            let snapshot = self.snapshot()?;
            self.settle(
              kernel,
              id,
              ControllerSettlement::Controlled(Ok(PlaybackOutcome {
                snapshot,
                warnings: Vec::new(),
              })),
            );
            self.expect_inhibited(
              true,
              "resumed playback session did not bind the idle inhibitor",
            )?;
            check("resumed playback session bound the idle inhibitor");
            self.step = Step::CyclePause;
          }
          Step::CyclePause => {
            // Pause through the real transport: the driver issues the same
            // mpv command the controller would, then settles with the
            // observed state.
            self.command(json!(["set_property", "pause", true]))?;
            if self.paused() != Some(true) {
              return Ok(());
            }
            let id = self
              .feed(
                kernel,
                PlaybackInput::Intent(Box::new(PlaybackIntent::SetPaused(true))),
              )
              .ok_or("pause intent produced no in-flight command")?;
            let snapshot = self.snapshot()?;
            self.settle(
              kernel,
              id,
              ControllerSettlement::Controlled(Ok(PlaybackOutcome {
                snapshot,
                warnings: Vec::new(),
              })),
            );
            self.expect_inhibited(false, "paused playback session kept the idle inhibitor")?;
            check("paused playback session released the idle inhibitor");
            self.step = Step::CycleResume;
          }
          Step::CycleResume => {
            self.command(json!(["set_property", "pause", false]))?;
            if self.paused() != Some(false) {
              return Ok(());
            }
            let id = self
              .feed(
                kernel,
                PlaybackInput::Intent(Box::new(PlaybackIntent::SetPaused(false))),
              )
              .ok_or("resume intent produced no in-flight command")?;
            let snapshot = self.snapshot()?;
            self.settle(
              kernel,
              id,
              ControllerSettlement::Controlled(Ok(PlaybackOutcome {
                snapshot,
                warnings: Vec::new(),
              })),
            );
            self.expect_inhibited(
              true,
              "resumed playback session did not rebind the idle inhibitor",
            )?;
            check("resumed playback session rebound the idle inhibitor");
            self.step = Step::AwaitReopen;
          }
          Step::AwaitReopen => {
            let reopened = with_run(|run| run.closed && run.surfaces == 1).unwrap_or(false);
            if !reopened {
              return Ok(());
            }
            self.expect_inhibited(
              true,
              "idle inhibitor did not rebind to the reopened window's surface",
            )?;
            check("idle inhibitor rebound to the recreated window surface");
            self.step = Step::Stop;
          }
          Step::Stop => {
            // The probe's own `stop` command drives mpv to idle-active; only
            // then does the session-level Stop settle, so the release check
            // follows the real native stop instead of preceding it.
            if self.property("idle-active").and_then(|v| v.as_bool()) != Some(true) {
              return Ok(());
            }
            let id = self
              .feed(
                kernel,
                PlaybackInput::Intent(Box::new(PlaybackIntent::Stop)),
              )
              .ok_or("stop intent produced no in-flight command")?;
            self.settle(
              kernel,
              id,
              ControllerSettlement::Stopped(Ok(PlaybackStopOutcome {
                warnings: Vec::new(),
              })),
            );
            self.expect_inhibited(false, "stopped playback session kept the idle inhibitor")?;
            check("stopped playback session released the idle inhibitor");
            self.step = Step::Done;
            DONE.store(true, std::sync::atomic::Ordering::Release);
          }
          Step::Done => {}
        }
        Ok(())
      }

      fn step_name(&self) -> &'static str {
        match self.step {
          Step::Begin => "begin",
          Step::AwaitResume => "await-resume",
          Step::CyclePause => "cycle-pause",
          Step::CycleResume => "cycle-resume",
          Step::AwaitReopen => "await-reopen",
          Step::Stop => "stop",
          Step::Done => "done",
        }
      }
    }
  }

  #[cfg(all(test, unix))]
  mod ipc_tests {
    use super::*;

    fn exchange(responses: &[serde_json::Value]) -> (Result<serde_json::Value, String>, String) {
      let (client, mut peer) = UnixStream::pair().expect("socket pair");
      client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read deadline");
      for response in responses {
        writeln!(peer, "{response}").expect("write response");
      }
      writeln!(peer, "{{\"event\":\"after-load\"}}").expect("write sentinel");
      let mut reader = BufReader::new(client);
      let result = ipc_command(&mut reader, &mut 0, json!(["loadfile", "/fixture.mp4"]));
      let mut next = String::new();
      reader.read_line(&mut next).expect("read next event");
      (result, next)
    }

    #[test]
    fn load_waits_for_acknowledgement_and_file_loaded_in_either_order() {
      let ack = json!({"request_id": 1, "error": "success", "data": null});
      let loaded = json!({"event": "file-loaded"});
      for responses in [[ack.clone(), loaded.clone()], [loaded, ack]] {
        let (result, next) = exchange(&responses);
        assert_eq!(result.expect("settled load"), serde_json::Value::Null);
        assert_eq!(
          serde_json::from_str::<serde_json::Value>(&next).expect("next event")["event"],
          "after-load"
        );
      }
    }

    #[test]
    fn acknowledged_load_still_fails_when_media_open_fails() {
      let (result, _) = exchange(&[
        json!({"request_id": 1, "error": "success"}),
        json!({"event": "end-file", "reason": "error"}),
      ]);
      assert!(result.is_err());
    }
  }

  #[cfg(target_os = "linux")]
  pub(crate) use idle_session::{done as idle_session_done, drive as drive_idle_session};
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) use gpu_probe::Probe;

#[cfg(test)]
mod tests {
  use super::*;

  fn gpu_run(started: Instant) -> Run {
    Run {
      scenario: "gpu".into(),
      run_id: "deadline-test".into(),
      unavailable: false,
      physical_size: None,
      video_size: iced::Size::new(480.0, 320.0),
      report: PathBuf::new(),
      media: None,
      hwdec: None,
      checks: Vec::new(),
      decoder_samples: Vec::new(),
      error: None,
      adapter: None,
      requested_host_abi: "3",
      actual_host_abi: None,
      action: Action::None,
      pip: PipProbe::default(),
      hosts: 1,
      dropped_hosts: 0,
      renderers: 1,
      surfaces: 1,
      closed: false,
      complete: false,
      finished: false,
      started,
      host_started: Some(started),
      sustained: None,
      phase: 0,
      phase_deadline: None,
      last_present: None,
      cycle: 1,
      phase_history: Vec::new(),
      presentations: 0,
      copied_frames: 0,
    }
  }

  #[test]
  fn frame_ticks_expire_a_gpu_phase_without_presentations() {
    let start = Instant::now();
    let mut run = gpu_run(start);
    run.advance_phase(8, start);
    assert!(!run.check_phase_deadline(start + Duration::from_secs(20)));
    assert!(run.check_phase_deadline(start + Duration::from_millis(20_001)));
    assert!(run.action == Action::Exit);
    assert_eq!(run.presentations, 0);
    assert!(run.error.as_ref().is_some_and(|error| {
      error.contains("phase=8") && error.contains("last_present_age=None")
    }));
    assert!(!run.check_phase_deadline(start + Duration::from_secs(90)));
  }

  #[test]
  fn progress_replaces_deadlines_and_completion_disarms_them() {
    let start = Instant::now();
    let mut run = gpu_run(start);
    run.advance_phase(0, start);
    run.advance_phase(8, start + Duration::from_secs(19));
    assert!(!run.check_phase_deadline(start + Duration::from_secs(21)));

    run.advance_phase(5, start + Duration::from_secs(22));
    run.action = Action::Closing;
    run.surfaces = 0;
    assert!(!run.check_phase_deadline(start + Duration::from_secs(40)));
    run.action = Action::Reopened;
    run.surfaces = 1;
    run.advance_phase(6, start + Duration::from_secs(41));
    assert!(!run.check_phase_deadline(start + Duration::from_secs(43)));
    run.advance_phase(7, start + Duration::from_secs(44));
    run.complete = true;
    run.action = Action::Exit;
    run.advance_phase(9, start + Duration::from_secs(45));
    assert!(!run.check_phase_deadline(start + Duration::from_secs(200)));
    assert!(run.phase_deadline.is_none());
    assert!(run.error.is_none());
  }

  #[test]
  fn presentations_do_not_extend_a_phase_budget() {
    let start = Instant::now();
    let mut run = gpu_run(start);
    run.sustained = Some(Sustained::parse("900").expect("bounded duration"));
    run.advance_phase(4, start);
    run.last_present = Some(start + Duration::from_secs(49));
    run.presentations = 100;
    assert!(!run.check_phase_deadline(start + Duration::from_secs(50)));
    assert!(run.check_phase_deadline(start + Duration::from_millis(50_001)));
    assert!(run.action == Action::Exit);
  }

  #[test]
  fn earlier_idle_failure_keeps_its_error_and_gpu_progress() {
    let start = Instant::now();
    let mut run = gpu_run(start);
    run.advance_phase(0, start);
    run.advance_phase(8, start + Duration::from_secs(15));
    run.last_present = Some(start + Duration::from_secs(16));
    let original = "Idle session driver: step \"await-resume\" exceeded its bounded cycle deadline";
    assert!(run.record_failure(original.into(), start + Duration::from_secs(21)));
    let first_error = run.error.clone().expect("idle failure");
    assert!(first_error.starts_with(original));
    assert!(first_error.contains("phase=8"));
    assert!(first_error.contains("last_present_age=Some(5s)"));
    assert!(!run.check_phase_deadline(start + Duration::from_secs(36)));
    assert!(!run.record_failure("later failure".into(), start + Duration::from_secs(40)));
    assert_eq!(run.error.as_deref(), Some(first_error.as_str()));
  }

  #[test]
  fn native_duration_rejects_unbounded_and_malformed_values() {
    for value in [
      "",
      "0",
      "3601",
      "-1",
      "+1",
      "1.5",
      "1e3",
      "NaN",
      "Infinity",
      " 1",
      "1 ",
      "１",
      "18446744073709551616",
    ] {
      assert!(Sustained::parse(value).is_err(), "accepted {value:?}");
    }
    for value in ["1", "900", "3600"] {
      assert!(Sustained::parse(value).is_ok(), "rejected {value:?}");
    }
  }

  #[test]
  fn sustained_cycles_cannot_finish_before_requested_time() {
    let mut run = Sustained::parse("900").expect("valid duration");
    let start = run.started;
    assert!(!run.complete_cycle(start + Duration::from_secs(30)));
    assert!(!run.complete_cycle(start + Duration::from_secs(899)));
    assert!(run.finished_at.is_none());
    assert!(run.complete_cycle(start + Duration::from_secs(900)));
    assert_eq!(run.completed_cycles, 3);
    assert_eq!(run.elapsed(), Duration::from_secs(900));
  }

  #[test]
  fn late_cycle_completion_records_observed_time_without_truncation() {
    let mut run = Sustained::parse("1").expect("valid duration");
    let finished = run.started + Duration::from_millis(3250);
    assert!(run.complete_cycle(finished));
    assert_eq!(run.finished_at, Some(finished));
    assert_eq!(run.elapsed(), Duration::from_millis(3250));
  }
}
