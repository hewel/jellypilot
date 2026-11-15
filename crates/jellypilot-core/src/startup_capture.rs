//! Opt-in real-process startup evidence, without desktop initialization or log files.
//!
//! Raw bytes are assembled in at most 16 KiB per stream. Only complete, sanitized
//! lines enter a 1 MiB tail per stream. Redaction is conservative, not a guarantee
//! that arbitrary application output is secret-free: review reports before sharing.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Write};
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::{self, Instant};

use crate::diagnostics::sanitize_message;
use crate::rendering::RenderBackend;

const MAX_LINE_BYTES: usize = 16 * 1024;
const MAX_TAIL_BYTES: usize = 1024 * 1024;
const DRAIN_TIMEOUT: Duration = Duration::from_millis(250);
const REAP_TIMEOUT: Duration = Duration::from_secs(2);
const REDACTED_LINE: &str = "[omitted: sensitive field, URL, private path, or terminal control]";

/// A normal executable invocation: inherited cwd/environment, no shell, null stdin.
/// Observation is capped at 300 seconds, followed by up to two seconds for reaping
/// and 250 ms for pipe draining. Only the directly spawned child is killed.
pub struct StartupCaptureOptions<'a> {
    pub executable: &'a Path,
    pub arguments: &'a [OsString],
    pub timeout: Duration,
}

/// Evidence of process termination and captured output, never desktop readiness.
/// Invocation paths, arguments and environment values are not added as metadata.
#[derive(Debug)]
pub struct StartupCaptureReport {
    elapsed: Duration,
    timeout: Duration,
    timed_out: bool,
    status: Option<ExitStatus>,
    spawn_error: Option<SafeIoError>,
    wait_error: Option<SafeIoError>,
    kill_error: Option<SafeIoError>,
    reap_incomplete: bool,
    stdout: CapturedStream,
    stderr: CapturedStream,
    stages: ObservedStages,
}

impl StartupCaptureReport {
    /// True for process failures, inconclusive existing-instance handoff, or lost
    /// evidence. A false result means only an observed exit 0 with complete output.
    #[must_use]
    pub fn has_failure(&self) -> bool {
        self.spawn_error.is_some()
            || self.wait_error.is_some()
            || self.kill_error.is_some()
            || self.reap_incomplete
            || self.timed_out
            || self.status.is_none_or(|status| !status.success())
            || self.stages.existing_instance
            || self.stdout.has_loss()
            || self.stderr.has_loss()
    }
}

impl fmt::Display for StartupCaptureReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Real startup capture ===")?;
        writeln!(
            f,
            "Review before sharing: free-form output may still contain private information."
        )?;
        writeln!(
            f,
            "Evidence only; exit 0 or a startup stage does not prove desktop readiness."
        )?;
        writeln!(
            f,
            "Only the owned child is stopped; descendants may outlive this capture."
        )?;
        writeln!(
            f,
            "Elapsed: {} ms; observation deadline: {} ms",
            self.elapsed.as_millis(),
            self.timeout.as_millis()
        )?;
        if let Some(error) = &self.spawn_error {
            writeln!(f, "Spawn failed: {error}")?;
        }
        if self.timed_out {
            writeln!(
                f,
                "Timeout: observation deadline reached; stop requested for owned child only."
            )?;
        }
        if let Some(status) = self.status {
            if let Some(code) = status.code() {
                write!(f, "Child exited with code {code}")?;
                #[cfg(windows)]
                write!(f, " (0x{:08X})", code as u32)?;
                writeln!(f, ".")?;
            } else {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    writeln!(
                        f,
                        "Child terminated by Unix signal {}.",
                        status.signal().unwrap_or_default()
                    )?;
                }
                #[cfg(not(unix))]
                writeln!(f, "Child terminated without an exit code.")?;
            }
        } else if self.spawn_error.is_none() {
            writeln!(f, "Child exit status unavailable.")?;
        }
        if let Some(error) = &self.wait_error {
            writeln!(f, "Wait failed: {error}")?;
        }
        if let Some(error) = &self.kill_error {
            writeln!(f, "Kill failed: {error}")?;
        }
        if self.reap_incomplete {
            writeln!(
                f,
                "[lost evidence: owned child was not reaped within cleanup deadline]"
            )?;
        }
        writeln!(
            f,
            "Last observed opt-in stage: {}",
            self.stages.last.unwrap_or("none observed")
        )?;
        writeln!(
            f,
            "Last substantive startup phase: {}",
            self.stages.last_phase.unwrap_or("none observed")
        )?;
        if let Some(renderer) = &self.stages.renderer {
            writeln!(
                f,
                "Renderer: configured={} requested={} source={} actual={}",
                renderer.configured.as_str(),
                renderer.requested.as_str(),
                renderer.source,
                renderer.actual,
            )?;
        } else {
            writeln!(f, "Renderer: not observed; actual backend unknown.")?;
        }
        if self.stages.existing_instance {
            writeln!(f, "Inconclusive: existing-instance handoff; this child did not establish a new desktop startup.")?;
        }
        writeln!(f, "--- stdout (sanitized tail; 1 MiB limit) ---")?;
        self.stdout.fmt(f)?;
        writeln!(f, "--- stderr (sanitized tail; 1 MiB limit) ---")?;
        self.stderr.fmt(f)
    }
}

#[derive(Debug)]
struct SafeIoError {
    kind: io::ErrorKind,
    os_code: Option<i32>,
}

impl From<io::Error> for SafeIoError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            os_code: error.raw_os_error(),
        }
    }
}

impl fmt::Display for SafeIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.kind {
            io::ErrorKind::NotFound => "executable, interpreter, or required loader not found",
            io::ErrorKind::PermissionDenied => "execution or process access denied",
            io::ErrorKind::InvalidInput => "invalid process invocation",
            io::ErrorKind::OutOfMemory => "insufficient memory",
            io::ErrorKind::ResourceBusy => "process resource busy",
            io::ErrorKind::Unsupported => "operation unsupported",
            _ => "operating-system process or pipe operation failed",
        };
        write!(f, "kind={:?}, OS code=", self.kind)?;
        if let Some(code) = self.os_code {
            write!(f, "{code}")?;
        } else {
            write!(f, "unavailable")?;
        }
        write!(f, "; {reason}")
    }
}

/// Emits an opt-in stage to stderr without panicking on a closed pipe.
/// Callers must use fixed, non-private stage literals, never runtime values.
pub fn emit_startup_stage(stage: &'static str) {
    if std::env::var_os("JELLYPILOT_STARTUP_CAPTURE").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        let _ = writeln!(io::stderr().lock(), "JELLYPILOT_STARTUP_STAGE {stage}");
    }
}

/// Launches the real executable with normal settings and inherited environment.
/// This is not read-only: the application may migrate settings, log in, or write
/// its usual files. No raw capture logs are written to disk. Requires a Tokio
/// runtime with IO/time enabled. Cancellation requests a kill of the owned child.
/// Windows pipe reads may outlive cancellation if descendants retain the handles;
/// the caller must use bounded runtime shutdown rather than blocking runtime Drop.
pub async fn capture_startup(options: StartupCaptureOptions<'_>) -> StartupCaptureReport {
    let mut command = Command::new(options.executable);
    command.args(options.arguments);
    capture_command(command, options.timeout).await
}

async fn capture_command(mut command: Command, timeout: Duration) -> StartupCaptureReport {
    let timeout = timeout.min(Duration::from_secs(300));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("JELLYPILOT_STARTUP_CAPTURE", "1")
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .env("CLICOLOR_FORCE", "0");
    for (key, default) in [
        (
            "JELLYPILOT_LOG",
            // iced_winit logs WindowAttributes, including every icon pixel, at info.
            "info,jellypilot_iced=debug,wgpu_hal=info,naga=warn,iced_winit=warn",
        ),
        ("RUST_BACKTRACE", "1"),
    ] {
        // var_os preserves even invalid Unicode and explicitly empty/custom values.
        if std::env::var_os(key).is_none()
            && !command.as_std().get_envs().any(|(name, _)| name == key)
        {
            command.env(key, default);
        }
    }
    let started = Instant::now();
    let mut report = StartupCaptureReport {
        elapsed: Duration::ZERO,
        timeout,
        timed_out: false,
        status: None,
        spawn_error: None,
        wait_error: None,
        kill_error: None,
        reap_incomplete: false,
        stdout: CapturedStream::default(),
        stderr: CapturedStream::default(),
        stages: ObservedStages::default(),
    };
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            report.spawn_error = Some(error.into());
            report.elapsed = started.elapsed();
            return report;
        }
    };
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let mut out_buffer = [0; 4096];
    let mut err_buffer = [0; 4096];
    let deadline = time::sleep(timeout);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
          () = &mut deadline => {
            report.timed_out = true;
            break;
          }
          result = child.wait() => {
            match result {
              Ok(status) => report.status = Some(status),
              Err(error) => report.wait_error = Some(error.into()),
            }
            break;
          }
          result = read_pipe(&mut stdout, &mut out_buffer) => {
            report.observe_output(true, result, &out_buffer);
            if report.stdout.closed { stdout = None; }
          }
          result = read_pipe(&mut stderr, &mut err_buffer) => {
            report.observe_output(false, result, &err_buffer);
            if report.stderr.closed { stderr = None; }
          }
        }
    }
    if report.status.is_none() {
        if let Err(error) = child.start_kill() {
            report.kill_error = Some(error.into());
        }
        match time::timeout(REAP_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) => report.status = Some(status),
            Ok(Err(error)) => {
                report.wait_error = Some(error.into());
                report.reap_incomplete = true;
            }
            Err(_) => report.reap_incomplete = true,
        }
    }
    // Descendants may inherit pipes. Bound observation even without EOF; on
    // Windows the caller must also bound runtime shutdown for pending pipe IO.
    let drain = time::sleep(DRAIN_TIMEOUT);
    tokio::pin!(drain);
    while stdout.is_some() || stderr.is_some() {
        tokio::select! {
          () = &mut drain => break,
          result = read_pipe(&mut stdout, &mut out_buffer) => {
            report.observe_output(true, result, &out_buffer);
            if report.stdout.closed { stdout = None; }
          }
          result = read_pipe(&mut stderr, &mut err_buffer) => {
            report.observe_output(false, result, &err_buffer);
            if report.stderr.closed { stderr = None; }
          }
        }
    }
    report.stdout.finish_incomplete();
    report.stderr.finish_incomplete();
    report.elapsed = started.elapsed();
    report
}

async fn read_pipe<R: AsyncRead + Unpin>(
    pipe: &mut Option<R>,
    buffer: &mut [u8],
) -> io::Result<usize> {
    match pipe {
        Some(pipe) => pipe.read(buffer).await,
        None => std::future::pending().await,
    }
}

impl StartupCaptureReport {
    fn observe_output(&mut self, stdout: bool, result: io::Result<usize>, buffer: &[u8]) {
        let stream = if stdout {
            &mut self.stdout
        } else {
            &mut self.stderr
        };
        match result {
            Ok(0) => {
                stream.finish_line(&mut self.stages);
                stream.closed = true;
            }
            Ok(length) => stream.feed(&buffer[..length], &mut self.stages),
            Err(error) => {
                stream.read_error = Some(error.into());
                stream.closed = true;
                stream.pending.clear();
            }
        }
    }
}

#[derive(Debug, Default)]
struct CapturedStream {
    pending: Vec<u8>,
    oversized: bool,
    tail: VecDeque<u8>,
    closed: bool,
    incomplete: bool,
    read_error: Option<SafeIoError>,
    oversized_lines: u64,
    invalid_lines: u64,
    redacted_lines: u64,
    evicted_lines: u64,
    sensitive_continuation: bool,
    sensitive_value_pending: bool,
}

impl CapturedStream {
    fn has_loss(&self) -> bool {
        self.incomplete
            || self.read_error.is_some()
            || self.oversized_lines > 0
            || self.invalid_lines > 0
            || self.evicted_lines > 0
    }

    fn feed(&mut self, bytes: &[u8], stages: &mut ObservedStages) {
        for &byte in bytes {
            if byte == b'\n' {
                self.finish_line(stages);
            } else if !self.oversized {
                if self.pending.len() == MAX_LINE_BYTES {
                    // Never retain a prefix: a credential may straddle the size boundary.
                    self.pending.clear();
                    self.oversized = true;
                } else {
                    self.pending.push(byte);
                }
            }
        }
    }

    fn finish_line(&mut self, stages: &mut ObservedStages) {
        if self.oversized {
            self.oversized_lines = self.oversized_lines.saturating_add(1);
            self.sensitive_value_pending = true;
            self.sensitive_continuation = true;
        } else if !self.pending.is_empty() {
            match std::str::from_utf8(&self.pending) {
                Ok(raw) => {
                    let raw = raw.strip_suffix('\r').unwrap_or(raw);
                    if let Some(observed) = recognized_stage(raw) {
                        stages.last = Some(observed);
                        stages.existing_instance |= observed == "existing-instance";
                        if !matches!(
                            observed,
                            "main-error" | "main-returned" | "event-loop-returned"
                        ) {
                            stages.last_phase = Some(observed);
                        }
                    }
                    let renderer_marker = raw.starts_with("JELLYPILOT_RENDERER");
                    let renderer = recognized_renderer(raw);
                    if let Some(renderer) = renderer {
                        stages.renderer = Some(renderer);
                    }
                    // Decide on complete lines, never independently read pipe fragments.
                    // Omit credential fields; mask locations without erasing the failure.
                    let sensitive_header = sensitive_line(raw);
                    let sensitive = sensitive_header
                        || self.sensitive_value_pending
                        || (self.sensitive_continuation && raw.starts_with(char::is_whitespace));
                    self.sensitive_value_pending =
                        sensitive_header && raw.trim_end().ends_with([':', '=']);
                    self.sensitive_continuation = sensitive;
                    let line = if sensitive || (renderer_marker && renderer.is_none()) {
                        Cow::Borrowed(REDACTED_LINE)
                    } else {
                        Cow::Owned(redact_locations(raw))
                    };
                    if line.as_ref() != raw {
                        self.redacted_lines = self.redacted_lines.saturating_add(1);
                    }
                    self.retain_line(&line);
                }
                Err(_) => {
                    self.invalid_lines = self.invalid_lines.saturating_add(1);
                    self.sensitive_value_pending = true;
                    self.sensitive_continuation = true;
                }
            }
        }
        self.pending.clear();
        self.oversized = false;
    }

    fn retain_line(&mut self, line: &str) {
        while self.tail.len() + line.len() + 1 > MAX_TAIL_BYTES {
            while let Some(byte) = self.tail.pop_front() {
                if byte == b'\n' {
                    break;
                }
            }
            self.evicted_lines = self.evicted_lines.saturating_add(1);
        }
        self.tail.extend(line.bytes());
        self.tail.push_back(b'\n');
    }

    fn finish_incomplete(&mut self) {
        if !self.closed {
            self.incomplete = true;
            self.pending.clear();
            if self.oversized {
                self.oversized_lines = self.oversized_lines.saturating_add(1);
            }
            self.oversized = false;
        }
    }

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.evicted_lines > 0 {
            writeln!(
                f,
                "[truncated: {} older sanitized lines discarded]",
                self.evicted_lines
            )?;
        }
        if self.oversized_lines > 0 {
            writeln!(
                f,
                "[lost output: {} whole oversized lines omitted; 16 KiB raw limit]",
                self.oversized_lines
            )?;
        }
        if self.invalid_lines > 0 {
            writeln!(
                f,
                "[lost output: {} non-UTF-8 lines omitted]",
                self.invalid_lines
            )?;
        }
        if self.redacted_lines > 0 {
            writeln!(
                f,
                "[redaction: {} sensitive lines redacted or omitted]",
                self.redacted_lines
            )?;
        }
        if let Some(error) = &self.read_error {
            writeln!(f, "[lost output: pipe read failed: {error}]")?;
        }
        if self.incomplete {
            writeln!(
                f,
                "[lost output: pipe not closed before drain deadline; unfinished line omitted]"
            )?;
        }
        let (first, second) = self.tail.as_slices();
        // Only the deque boundary can split a code point; bridge at most four bytes
        // rather than allocating/copying the entire retained tail for presentation.
        let boundary =
            std::str::from_utf8(first).map_or_else(|error| error.valid_up_to(), str::len);
        f.write_str(std::str::from_utf8(&first[..boundary]).map_err(|_| fmt::Error)?)?;
        let remaining = &first[boundary..];
        let continuation = second
            .iter()
            .take_while(|byte| **byte & 0xc0 == 0x80)
            .count();
        let mut bridge = [0; 4];
        let bridge_len = remaining.len() + continuation;
        bridge[..remaining.len()].copy_from_slice(remaining);
        bridge[remaining.len()..bridge_len].copy_from_slice(&second[..continuation]);
        f.write_str(std::str::from_utf8(&bridge[..bridge_len]).map_err(|_| fmt::Error)?)?;
        f.write_str(std::str::from_utf8(&second[continuation..]).map_err(|_| fmt::Error)?)
    }
}

fn sensitive_line(line: &str) -> bool {
    if line.chars().any(|c| {
    (c.is_control() && c != '\t')
      || matches!(c, '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
  }) {
    return true;
  }
    let compact: String = line
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_ascii_punctuation())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    [
        "auth", "cookie", "token", "password", "passwd", "apikey", "server", "account", "username",
        "secret", "bearer", "www",
    ]
    .iter()
    .any(|field| compact.contains(field))
        || line
            .split_whitespace()
            .any(|word| word.to_ascii_lowercase().starts_with("data:"))
}

fn redact_locations(line: &str) -> String {
    if !line.contains(['/', '\\', '@']) {
        return sanitize_message(line);
    }
    let mut result = String::with_capacity(line.len());
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if quote.is_some() && character == '\\' && !escaped {
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') && !escaped {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none()
                && (index == start || matches!(line.as_bytes().get(index - 1), Some(b'=' | b':')))
            {
                quote = Some(character);
            }
        }
        escaped = false;
        if quote.is_none()
            && (character.is_whitespace() || matches!(character, '(' | ')' | ',' | ';'))
        {
            let token = &line[start..index];
            result.push_str(if token.contains(['/', '\\', '@']) {
                "[LOCATION]"
            } else {
                token
            });
            result.push(character);
            start = index + character.len_utf8();
        }
    }
    let token = &line[start..];
    result.push_str(if token.contains(['/', '\\', '@']) {
        "[LOCATION]"
    } else {
        token
    });
    result
}

#[derive(Debug, Default)]
struct ObservedStages {
    last: Option<&'static str>,
    last_phase: Option<&'static str>,
    existing_instance: bool,
    renderer: Option<RendererMetadata>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RendererMetadata {
    configured: RenderBackend,
    requested: RenderBackend,
    source: &'static str,
    actual: &'static str,
}

fn recognized_renderer(line: &str) -> Option<RendererMetadata> {
    let mut tokens = line.strip_prefix("JELLYPILOT_RENDERER ")?.split(' ');
    let canonical =
        |value: &str| RenderBackend::parse(value).filter(|backend| backend.as_str() == value);
    let configured = canonical(tokens.next()?.strip_prefix("configured=")?)?;
    let requested = canonical(tokens.next()?.strip_prefix("requested=")?)?;
    let source = match tokens.next()?.strip_prefix("source=")? {
        "cli" => "cli",
        "environment" => "environment",
        "settings" => "settings",
        _ => return None,
    };
    let actual = match tokens.next()?.strip_prefix("actual=")? {
        "pending" => "pending",
        "unknown" => "unknown",
        value => canonical(value)?.as_str(),
    };
    if tokens.next().is_some() {
        return None;
    }
    Some(RendererMetadata {
        configured,
        requested,
        source,
        actual,
    })
}

fn recognized_stage(line: &str) -> Option<&'static str> {
    let stage = line.strip_prefix("JELLYPILOT_STARTUP_STAGE ")?;
    // Only fixed application stages may escape the free-form sanitizer.
    const STAGES: &[&str] = &[
        "main-enter",
        "main-returned",
        "main-error",
        "tracing-init-enter",
        "tracing-init-exit",
        "instance-enter",
        "instance-primary",
        "instance-unavailable",
        "instance-skipped",
        "existing-instance",
        "backend-init-enter",
        "backend-init-exit",
        "backend-settings-enter",
        "backend-settings-exit",
        "backend-settings-failed",
        "renderer-init-enter",
        "renderer-init-exit",
        "backend-external",
        "backend-embedded",
        "embedded-assets-enter",
        "embedded-assets-exit",
        "fonts-init-enter",
        "fonts-init-exit",
        "preview-init-enter",
        "preview-init-exit",
        "event-loop-enter",
        "event-loop-returned",
        "app-boot-enter",
        "app-boot-exit",
        "app-settings-enter",
        "app-settings-exit",
        "app-settings-failed",
        "tray-init-enter",
        "tray-ready",
        "tray-unavailable",
        "tray-skipped",
        "gtk-init-enter",
        "gtk-init-exit",
        "window-open-requested",
        "window-opened",
        "window-start-hidden",
        "compositor-init-enter",
        "compositor-init-exit",
        "gpu-instance-enter",
        "gpu-instance-exit",
        "gpu-device-enter",
        "gpu-device-exit",
        "embedded-engine-enter",
        "embedded-engine-exit",
        "embedded-host-enter",
        "embedded-host-exit",
    ];
    STAGES.iter().copied().find(|known| *known == stage)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "startup_capture::tests::subprocess_fixture";

    fn fixture_command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command.args(["--exact", FIXTURE, "--ignored", "--nocapture"]);
        command.env("JELLYPILOT_CAPTURE_FIXTURE", mode);
        command
    }

    async fn fixture(mode: &str) -> StartupCaptureReport {
        capture_command(fixture_command(mode), Duration::from_secs(5)).await
    }

    #[test]
    #[ignore = "subprocess fixture; invoked only by capture tests"]
    fn subprocess_fixture() {
        let mode = std::env::var("JELLYPILOT_CAPTURE_FIXTURE").expect("fixture mode");
        match mode.as_str() {
            "failure" => {
                println!("renderer initialization failed: adapter unavailable");
                eprintln!("JELLYPILOT_STARTUP_STAGE backend-init-enter");
                eprintln!("JELLYPILOT_RENDERER configured=auto requested=vulkan source=settings actual=pending");
                eprintln!("native loader failed: missing library");
                eprintln!("JELLYPILOT_STARTUP_STAGE main-error");
                std::process::exit(23);
            }
            "zero" => std::process::exit(0),
            "environment" => {
                assert_eq!(std::env::var_os("JELLYPILOT_LOG"), Some(OsString::new()));
                assert_eq!(
                    std::env::var_os("RUST_BACKTRACE"),
                    Some(OsString::from("custom-invalid-value"))
                );
                assert_eq!(
                    std::env::var_os("JELLYPILOT_STARTUP_CAPTURE"),
                    Some(OsString::from("1"))
                );
                assert_eq!(std::env::var_os("NO_COLOR"), Some(OsString::from("1")));
                emit_startup_stage("main-enter");
            }
            #[cfg(unix)]
            "invalid-environment" => {
                use std::os::unix::ffi::OsStrExt;
                assert_eq!(
                    std::env::var_os("JELLYPILOT_LOG").unwrap().as_bytes(),
                    &[0xff]
                );
                assert_eq!(
                    std::env::var_os("RUST_BACKTRACE").unwrap().as_bytes(),
                    &[0xfe]
                );
            }
            "existing" => {
                eprintln!("JELLYPILOT_STARTUP_STAGE existing-instance");
                std::process::exit(0);
            }
            "stall" => {
                eprintln!("waiting for native initialization");
                std::thread::sleep(Duration::from_secs(30));
            }
            "oversized-stall" => {
                let mut out = io::stdout().lock();
                out.write_all(&vec![b'x'; MAX_LINE_BYTES + 1]).unwrap();
                out.write_all(b"unclosed-private-fragment").unwrap();
                out.flush().unwrap();
                std::thread::sleep(Duration::from_secs(30));
            }
            "descendant" => {
                let mut child = std::process::Command::new(std::env::current_exe().unwrap());
                child.args(["--exact", FIXTURE, "--ignored", "--nocapture"]);
                child.env("JELLYPILOT_CAPTURE_FIXTURE", "hold-pipes");
                child.spawn().expect("descendant");
                std::process::exit(0);
            }
            "hold-pipes" => std::thread::sleep(Duration::from_secs(1)),
            "flood" => {
                let mut out = io::stdout().lock();
                let block = vec![b'x'; 4096];
                writeln!(out, "JELLYPILOT_RENDERER configured=auto requested=vulkan source=cli actual=pending").unwrap();
                writeln!(out, "JELLYPILOT_RENDERER configured=auto requested=vulkan source=cli actual=software").unwrap();
                writeln!(out, "JELLYPILOT_RENDERER configured=private-token requested=vulkan source=cli actual=software").unwrap();
                for _ in 0..8 {
                    out.write_all(&block).unwrap();
                }
                writeln!(out, "fragment-secret").unwrap();
                let mut err = io::stderr().lock();
                for index in 0..18000 {
                    writeln!(
                        out,
                        "renderer error: adapter unavailable after initialization attempt"
                    )
                    .unwrap();
                    if index % 32 == 0 {
                        writeln!(err, "native stderr evidence during stdout flood").unwrap();
                    }
                }
                writeln!(out, "final actionable error: native initialization failed").unwrap();
            }
            "credentials" => {
                let mut out = io::stdout().lock();
                for fragment in [
                    b"access_to".as_slice(),
                    b"ken=split-secret\n",
                    b"https:",
                    b"//user:password@private.example/item\n",
                    b"api-",
                    b"key = quoted secret value\n",
                    b"private path /home/alice/library\n",
                    b"\x1b[31mterminal-secret\x1b[0m\n",
                    b"renderer error: ",
                    &[0xe8],
                    &[0xaf, 0xad],
                    b"\nfinal error without newline",
                ] {
                    out.write_all(fragment).unwrap();
                    out.flush().unwrap();
                }
                std::process::exit(1);
            }
            #[cfg(unix)]
            "signal" => {
                std::process::Command::new("kill")
                    .args(["-TERM", &std::process::id().to_string()])
                    .status()
                    .unwrap();
                panic!("signal did not terminate fixture");
            }
            _ => panic!("unknown fixture mode"),
        }
    }

    #[tokio::test]
    async fn nonzero_retains_both_sources_and_last_stage() {
        let report = fixture("failure").await;
        let text = report.to_string();
        assert!(report.has_failure());
        assert_eq!(report.status.and_then(|status| status.code()), Some(23));
        assert_eq!(report.stages.last, Some("main-error"));
        assert_eq!(report.stages.last_phase, Some("backend-init-enter"));
        assert!(text.contains("renderer initialization failed"));
        assert!(text.contains("native loader failed"));
        assert!(text.contains("--- stdout") && text.contains("--- stderr"));
        assert!(text
            .contains("Renderer: configured=auto requested=vulkan source=settings actual=pending"));
    }

    #[tokio::test]
    async fn exit_zero_is_not_desktop_readiness() {
        let report = fixture("zero").await;
        assert!(!report.has_failure(), "{report}");
        assert!(report.to_string().contains("Child exited with code 0."));
        assert!(report
            .to_string()
            .contains("does not prove desktop readiness"));
    }

    #[tokio::test]
    async fn existing_instance_handoff_is_inconclusive() {
        let report = fixture("existing").await;
        assert!(report.has_failure());
        assert!(report
            .to_string()
            .contains("Inconclusive: existing-instance"));
    }

    #[tokio::test]
    async fn missing_executable_reports_safe_kind_and_os_code() {
        let path = std::env::temp_dir()
            .join(format!("private-capture-missing-{}", std::process::id()))
            .join("not-present");
        let report = capture_startup(StartupCaptureOptions {
            executable: &path,
            arguments: &[],
            timeout: Duration::from_secs(1),
        })
        .await;
        let error = report.spawn_error.as_ref().unwrap();
        assert_eq!(error.kind, io::ErrorKind::NotFound);
        assert!(error.os_code.is_some());
        assert!(report.has_failure());
        assert!(!report.to_string().contains("private-capture-missing"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn non_executable_file_reports_permission_without_path() {
        let path =
            std::env::temp_dir().join(format!("private-capture-permission-{}", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        drop(file);
        let report = capture_startup(StartupCaptureOptions {
            executable: &path,
            arguments: &[],
            timeout: Duration::from_secs(1),
        })
        .await;
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            report.spawn_error.as_ref().unwrap().kind,
            io::ErrorKind::PermissionDenied
        );
        assert!(!report.to_string().contains("private-capture-permission"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn signal_exit_preserves_signal_evidence() {
        use std::os::unix::process::ExitStatusExt;
        let report = fixture("signal").await;
        assert_eq!(report.status.and_then(|status| status.signal()), Some(15));
        assert!(report.has_failure());
        assert!(report.to_string().contains("Unix signal 15"));
    }

    #[tokio::test]
    async fn timeout_kills_and_reaps_owned_child() {
        let report = capture_command(fixture_command("stall"), Duration::from_millis(100)).await;
        assert!(
            report.timed_out && report.status.is_some() && !report.reap_incomplete,
            "{report}"
        );
        assert!(report.has_failure());
        assert!(report.elapsed < Duration::from_secs(4));
    }

    #[tokio::test]
    async fn oversized_unfinished_output_is_not_published_at_timeout() {
        let report =
            capture_command(fixture_command("oversized-stall"), Duration::from_secs(1)).await;
        assert!(report.timed_out && report.status.is_some(), "{report}");
        assert_eq!(report.stdout.oversized_lines, 1);
        assert!(!report.to_string().contains("unclosed-private-fragment"));
    }

    #[tokio::test]
    async fn descendants_holding_pipes_cannot_block_return_or_runtime_drop() {
        let report = fixture("descendant").await;
        assert_eq!(report.status.and_then(|status| status.code()), Some(0));
        assert!(
            report.stdout.incomplete || report.stderr.incomplete,
            "{report}"
        );
        assert!(report.has_failure());
        assert!(report.elapsed < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn flood_keeps_bounded_tail_and_omits_entire_oversized_line() {
        let report = fixture("flood").await;
        assert!(report.stdout.tail.len() <= MAX_TAIL_BYTES);
        assert_eq!(report.stdout.oversized_lines, 1);
        assert!(report.stdout.evicted_lines > 0);
        let text = report.to_string();
        assert!(!text.contains("fragment-secret"));
        assert!(text.contains("final actionable error"));
        assert!(text.contains("native stderr evidence during stdout flood"));
        assert!(
            text.contains("Renderer: configured=auto requested=vulkan source=cli actual=software")
        );
        assert!(!text.contains("private-token"));
        assert!(!text
            .split("--- stdout")
            .nth(1)
            .unwrap()
            .contains("JELLYPILOT_RENDERER"));
        assert!(report.has_failure());
    }

    #[tokio::test]
    async fn split_credentials_controls_utf8_and_eof_are_handled_as_complete_lines() {
        let report = fixture("credentials").await;
        let text = report.to_string();
        for secret in [
            "split-secret",
            "private.example",
            "quoted secret",
            "alice",
            "terminal-secret",
        ] {
            assert!(!text.contains(secret), "leaked {secret}");
        }
        assert!(text.contains("renderer error: 语"));
        assert!(text.contains("final error without newline"));
        assert!(report.stdout.redacted_lines >= 5);
    }

    #[tokio::test]
    async fn child_environment_keeps_empty_and_custom_values_without_global_mutation() {
        let mut command = fixture_command("environment");
        command
            .env("JELLYPILOT_LOG", "")
            .env("RUST_BACKTRACE", "custom-invalid-value");
        let report = capture_command(command, Duration::from_secs(5)).await;
        assert!(!report.has_failure(), "{report}");
        assert_eq!(report.stages.last, Some("main-enter"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_environment_preserves_invalid_unicode_values() {
        use std::os::unix::ffi::OsStringExt;
        let mut command = fixture_command("invalid-environment");
        command.env("JELLYPILOT_LOG", OsString::from_vec(vec![0xff]));
        command.env("RUST_BACKTRACE", OsString::from_vec(vec![0xfe]));
        let report = capture_command(command, Duration::from_secs(5)).await;
        assert!(!report.has_failure(), "{report}");
    }

    #[test]
    fn every_fragment_boundary_preserves_redaction_and_utf8() {
        let raw = "api-key = private credential\nrenderer: 语\n".as_bytes();
        for boundary in 0..=raw.len() {
            let mut stream = CapturedStream::default();
            let mut stages = ObservedStages::default();
            stream.feed(&raw[..boundary], &mut stages);
            stream.feed(&raw[boundary..], &mut stages);
            let text = String::from_utf8(stream.tail.into_iter().collect()).unwrap();
            assert!(!text.contains("private credential"));
            assert!(text.contains("renderer: 语"));
        }
    }

    #[test]
    fn renderer_metadata_accepts_only_complete_canonical_allowlisted_tokens() {
        let valid = b"JELLYPILOT_RENDERER configured=auto requested=vulkan source=environment actual=unknown\n";
        for boundary in 0..=valid.len() {
            let mut stream = CapturedStream::default();
            let mut stages = ObservedStages::default();
            stream.feed(&valid[..boundary], &mut stages);
            stream.feed(&valid[boundary..], &mut stages);
            let expected = stages.renderer.expect("complete renderer marker");
            for invalid in [
                "JELLYPILOT_RENDERER configured=private-value requested=vulkan source=cli actual=pending\n",
                "JELLYPILOT_RENDERER configured=auto requested=gl source=cli actual=pending\n",
                "JELLYPILOT_RENDERER configured=auto requested=vulkan source=private-value actual=pending\n",
                "JELLYPILOT_RENDERER configured=auto requested=vulkan source=cli actual=private-value\n",
                "JELLYPILOT_RENDERER configured=auto requested=vulkan source=cli actual=vulkan extra=private-value\n",
                "JELLYPILOT_RENDERER configured=auto requested=vulkan source=cli\n",
            ] {
                stream.feed(invalid.as_bytes(), &mut stages);
                assert_eq!(stages.renderer, Some(expected));
            }
            let tail = String::from_utf8(stream.tail.into_iter().collect()).unwrap();
            assert!(!tail.contains("private-value"));
            assert!(tail.contains(REDACTED_LINE));
        }
    }

    #[test]
    fn multiline_sensitive_fields_and_indented_values_are_omitted() {
        let raw =
            b"password:\nstandalone-private-value\n  continued-private-value\nrenderer failed\n";
        let mut stream = CapturedStream::default();
        stream.feed(raw, &mut ObservedStages::default());
        let text = String::from_utf8(stream.tail.into_iter().collect()).unwrap();
        assert!(!text.contains("private-value"));
        assert!(text.contains("renderer failed"));
    }

    #[test]
    fn invalid_utf8_and_incomplete_lines_are_loss_not_partial_text() {
        let mut stream = CapturedStream::default();
        stream.feed(
            b"prefix\xffcredential\nunclosed-private-value",
            &mut ObservedStages::default(),
        );
        stream.finish_incomplete();
        assert_eq!(stream.invalid_lines, 1);
        assert!(stream.has_loss());
        assert!(stream.tail.is_empty());
        assert!(stream.pending.is_empty());
    }

    #[test]
    fn location_redaction_retains_loader_failure_reason_and_quoted_path_privacy() {
        let mut stream = CapturedStream::default();
        stream.feed(
            b"Error: Embedded MPV asset missing: \"/home/private user/libmpv.so\". Reinstall the pinned runtime.\nError: dlopen(C:\\private\\libmpv.dll): access denied\nError: missing \"/home/a\\\" private-title\"\nError: couldn't load /home/private/file: incompatible format\n",
            &mut ObservedStages::default(),
        );
        let text = String::from_utf8(stream.tail.into_iter().collect()).unwrap();
        assert!(text.contains("Embedded MPV asset missing"));
        assert!(text.contains("Reinstall the pinned runtime"));
        assert!(text.contains("access denied"));
        assert!(text.contains("couldn't load"));
        assert!(text.contains("incompatible format"));
        assert!(!text.contains("private"));
        assert!(!text.contains("/home"));
        assert!(!text.contains("C:"));
    }

    #[test]
    fn discarded_credential_headers_do_not_expose_following_values() {
        for header in [
            b"password:\xff\n".to_vec(),
            [vec![b'x'; MAX_LINE_BYTES + 1], b"password:\n".to_vec()].concat(),
        ] {
            let mut stream = CapturedStream::default();
            let mut stages = ObservedStages::default();
            stream.feed(&header, &mut stages);
            stream.feed(
                b"private-value\n  continued-private-value\nrenderer failed\n",
                &mut stages,
            );
            let text = String::from_utf8(stream.tail.into_iter().collect()).unwrap();
            assert!(!text.contains("private-value"));
            assert!(text.contains("renderer failed"));
        }
    }
}
