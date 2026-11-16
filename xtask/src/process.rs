use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use command_group::{CommandGroup, GroupChild};

const CAPTURE_BYTES: usize = 2_000_000;
const POLL_INTERVAL: Duration = Duration::from_millis(25);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Default)]
pub struct RunOptions {
  pub capture: bool,
  pub allow_failure: bool,
  pub timeout: Option<Duration>,
}

#[derive(Debug)]
enum ProcessStop {
  Interrupted,
  TimedOut,
}

impl std::fmt::Display for ProcessStop {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(match self {
      Self::Interrupted => "Task interrupted after cleanup.",
      Self::TimedOut => "Child process timed out.",
    })
  }
}

impl std::error::Error for ProcessStop {}

pub fn install_signal_handler() -> Result<()> {
  ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::SeqCst))
    .context("Installing task interruption handler")
}

pub fn interrupted() -> bool {
  INTERRUPTED.load(Ordering::SeqCst)
}

pub fn sleep_interruptible(duration: Duration) -> Result<()> {
  let started = Instant::now();
  loop {
    if interrupted() {
      bail!("Task interrupted after cleanup.");
    }
    let Some(remaining) = duration.checked_sub(started.elapsed()) else {
      return Ok(());
    };
    thread::sleep(remaining.min(POLL_INTERVAL));
  }
}

pub fn command(program: impl AsRef<OsStr>) -> Command {
  let mut command = Command::new(program);
  command.current_dir(crate::root());
  command
}

pub fn cargo() -> Command {
  command(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

pub fn run(command: &mut Command) -> Result<Output> {
  run_with(command, RunOptions::default())
}

pub fn capture(command: &mut Command) -> Result<Output> {
  run_with(
    command,
    RunOptions {
      capture: true,
      ..RunOptions::default()
    },
  )
}

// Own the group until cleanup, including early returns while setting up readers.
struct OwnedGroup(Option<GroupChild>);

impl OwnedGroup {
  fn cleanup(&mut self) {
    let Some(mut child) = self.0.take() else {
      return;
    };
    #[cfg(unix)]
    {
      use command_group::UnixChildExt;
      use nix::sys::signal::{killpg, Signal};
      use nix::unistd::Pid;

      if child.signal(Signal::SIGTERM).is_ok() {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
          // Reap the leader while allowing its descendants a graceful shutdown.
          let _ = child.inner().try_wait();
          if killpg(Pid::from_raw(child.id() as i32), None).is_err() {
            break;
          }
          thread::sleep(POLL_INTERVAL);
        }
      }
    }
    // GroupChild uses a Job Object on Windows, not just the immediate process.
    let _ = child.kill();
    let _ = child.inner().wait();
  }
}

impl Drop for OwnedGroup {
  fn drop(&mut self) {
    self.cleanup();
  }
}

fn pump(mut source: impl Read) -> io::Result<Vec<u8>> {
  let mut tail = VecDeque::new();
  let mut chunk = [0_u8; 8192];
  loop {
    let count = match source.read(&mut chunk) {
      Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
      result => result?,
    };
    if count == 0 {
      break;
    }
    let excess = (tail.len() + count).saturating_sub(CAPTURE_BYTES);
    tail.drain(..excess);
    tail.extend(&chunk[..count]);
  }
  Ok(tail.into())
}

fn wait_leader(
  child: &mut GroupChild,
  timeout: Option<Duration>,
  cancelled: &AtomicBool,
) -> Result<ExitStatus> {
  let started = Instant::now();
  loop {
    if cancelled.load(Ordering::SeqCst) {
      return Err(ProcessStop::Interrupted.into());
    }
    if timeout.is_some_and(|limit| started.elapsed() >= limit) {
      return Err(ProcessStop::TimedOut.into());
    }
    // Watch the leader rather than wait for possibly leaked descendants. Cleanup
    // terminates remaining group members before joining the output readers.
    if let Some(status) = child.inner().try_wait()? {
      return Ok(status);
    }
    thread::sleep(POLL_INTERVAL);
  }
}

pub fn run_with(command: &mut Command, options: RunOptions) -> Result<Output> {
  run_cancellable(command, options, &INTERRUPTED)
}

fn run_cancellable(
  command: &mut Command,
  options: RunOptions,
  cancelled: &AtomicBool,
) -> Result<Output> {
  if cancelled.load(Ordering::SeqCst) {
    return Err(ProcessStop::Interrupted.into());
  }
  let invocation = format!(
    "{} {}",
    command.get_program().to_string_lossy(),
    command
      .get_args()
      .map(|arg| format!("{:?}", arg))
      .collect::<Vec<_>>()
      .join(" ")
  );
  if options.capture {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
  } else {
    // Let the OS forward streams. A blocked downstream reader must not block
    // xtask's own capture threads and prevent timeout/cancellation cleanup.
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
  }
  let mut group = OwnedGroup(Some(
    command
      .group_spawn()
      .with_context(|| format!("Starting {invocation}"))?,
  ));
  let child = group.0.as_mut().context("Missing owned child process")?;
  let readers = if options.capture {
    let stdout = child
      .inner()
      .stdout
      .take()
      .context("Missing child stdout pipe")?;
    let stderr = child
      .inner()
      .stderr
      .take()
      .context("Missing child stderr pipe")?;
    let stdout = thread::Builder::new()
      .name("xtask-stdout".into())
      .spawn(move || pump(stdout))?;
    let stderr = thread::Builder::new()
      .name("xtask-stderr".into())
      .spawn(move || pump(stderr))?;
    Some((stdout, stderr))
  } else {
    None
  };
  let status = wait_leader(child, options.timeout, cancelled);
  group.cleanup();
  let (stdout, stderr) = match readers {
    Some((stdout, stderr)) => (
      stdout
        .join()
        .map_err(|_| anyhow!("stdout reader panicked"))?,
      stderr
        .join()
        .map_err(|_| anyhow!("stderr reader panicked"))?,
    ),
    None => (Ok(Vec::new()), Ok(Vec::new())),
  };
  let status = status.with_context(|| format!("Running {invocation}"))?;
  let output = Output {
    status,
    stdout: stdout?,
    stderr: stderr?,
  };
  if !output.status.success() && !options.allow_failure {
    bail!(
      "Command failed ({}) : {invocation}\n{}{}",
      output.status,
      String::from_utf8_lossy(&output.stdout),
      String::from_utf8_lossy(&output.stderr)
    );
  }
  Ok(output)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn capture_retains_the_last_bytes() {
    let mut input = vec![b'x'; CAPTURE_BYTES + 37];
    input.extend_from_slice(b"last output");
    let output = pump(input.as_slice()).unwrap();
    assert_eq!(output, input[input.len() - CAPTURE_BYTES..]);
  }

  #[cfg(unix)]
  #[test]
  fn inherited_output_backpressure_does_not_suspend_timeout() {
    const FIXTURE: &str = "JELLYPILOT_XTASK_BLOCKED_OUTPUT_TEST";
    if std::env::var_os(FIXTURE).is_some() {
      let result = run_with(
        command("sh").args([
          "-c",
          "while :; do printf 'fill the unread output pipe\\n'; done",
        ]),
        RunOptions {
          timeout: Some(Duration::from_millis(100)),
          ..RunOptions::default()
        },
      );
      // The test harness would itself print into the deliberately full pipe.
      let timed_out = result.err().is_some_and(|error| {
        matches!(
          error.downcast_ref::<ProcessStop>(),
          Some(ProcessStop::TimedOut)
        )
      });
      std::process::exit(if timed_out { 0 } else { 1 });
    }
    let mut child = OwnedGroup(Some(
      Command::new(std::env::current_exe().unwrap())
        .args([
          "--exact",
          "process::tests::inherited_output_backpressure_does_not_suspend_timeout",
          "--nocapture",
        ])
        .env(FIXTURE, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .group_spawn()
        .unwrap(),
    ));
    let result = wait_leader(
      child.0.as_mut().unwrap(),
      Some(Duration::from_secs(5)),
      &AtomicBool::new(false),
    );
    child.cleanup();
    assert!(result.unwrap().success());
  }

  #[cfg(unix)]
  #[test]
  fn failure_retains_both_streams_and_exit_status() {
    let output = run_with(
      command("sh").args(["-c", "printf stdout; printf stderr >&2; exit 7"]),
      RunOptions {
        capture: true,
        allow_failure: true,
        timeout: Some(Duration::from_secs(5)),
      },
    )
    .unwrap();
    assert_eq!(
      (
        output.status.code(),
        output.stdout.as_slice(),
        output.stderr.as_slice()
      ),
      (Some(7), b"stdout".as_slice(), b"stderr".as_slice())
    );
  }

  #[cfg(unix)]
  #[test]
  fn drains_large_stdout_and_stderr_without_blocking() {
    let output = run_with(
      command("sh").args([
        "-c",
        "head -c 300000 /dev/zero & head -c 300000 /dev/zero >&2; wait",
      ]),
      RunOptions {
        capture: true,
        timeout: Some(Duration::from_secs(5)),
        ..RunOptions::default()
      },
    )
    .unwrap();
    assert_eq!((output.stdout.len(), output.stderr.len()), (300000, 300000));
  }

  #[cfg(unix)]
  #[test]
  fn timeout_terminates_descendants_holding_output_pipes() {
    let started = Instant::now();
    let result = run_with(
      command("sh").args(["-c", "sleep 60 & wait"]),
      RunOptions {
        capture: true,
        timeout: Some(Duration::from_millis(100)),
        ..RunOptions::default()
      },
    );
    assert!(matches!(
      result.unwrap_err().downcast_ref::<ProcessStop>(),
      Some(ProcessStop::TimedOut)
    ));
    assert!(started.elapsed() < Duration::from_secs(5));
  }

  #[cfg(unix)]
  #[test]
  fn cancellation_cleans_owned_child_group() {
    let cancelled = AtomicBool::new(false);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("started");
    thread::scope(|scope| {
      let marker_ref = &marker;
      let flag = &cancelled;
      scope.spawn(move || {
        ready_rx.recv().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker_ref.exists() && Instant::now() < deadline {
          thread::sleep(Duration::from_millis(5));
        }
        flag.store(true, Ordering::SeqCst);
      });
      ready_tx.send(()).unwrap();
      let result = run_cancellable(
        command("sh")
          .arg("-c")
          .arg("sleep 60 & touch \"$1\"; wait")
          .arg("sh")
          .arg(&marker),
        RunOptions {
          capture: true,
          timeout: Some(Duration::from_secs(10)),
          ..RunOptions::default()
        },
        &cancelled,
      );
      assert!(marker.exists());
      assert!(matches!(
        result.unwrap_err().downcast_ref::<ProcessStop>(),
        Some(ProcessStop::Interrupted)
      ));
      assert!(cancelled.load(Ordering::SeqCst));
    });
  }
}
