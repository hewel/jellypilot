//! Console-only entry point. Deliberately references no desktop or player crates.
#![forbid(unsafe_code)]
#![cfg_attr(target_os = "windows", windows_subsystem = "console")]

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use jellypilot_core::startup_capture::{capture_startup, StartupCaptureOptions};
use jellypilot_core::startup_diagnostics::collect_startup_report;

const HELP: &str = "jellypilot-diagnostics [--config PATH] [--output PATH]
jellypilot-diagnostics --capture-startup [--app PATH] [--timeout-secs N] [--output PATH] [-- APP_ARGS...]

Default: read-only local checks, without desktop, player, keyring or network initialization.
Capture: launches the real application with its normal settings and environment.
It may open windows, migrate settings and auto-login. Quit existing JellyPilot instances first.
At the deadline only the spawned application process is terminated; this is not proof of a hang.

Options:
  --config PATH      Inspect a configuration copy; cannot combine with --capture-startup.
  --capture-startup  Capture sanitized output, startup stages, renderer metadata and exit status.
  --app PATH         Application to reproduce; default is jellypilot beside this helper.
  --timeout-secs N   Capture deadline, 1-300 seconds (default: 30); capture mode only.
  --output PATH      Create a NEW report file, never overwrite; otherwise use stdout.
  -- APP_ARGS...     Forward arguments literally to the application; capture mode only.
  --help            Show this help.
  --version         Show the application version.

Capture preserves existing JELLYPILOT_LOG/RUST_BACKTRACE. When absent, defaults are
info,jellypilot_iced=debug,wgpu_hal=info,naga=warn,iced_winit=warn and 1 respectively.
Raw capture is held in bounded memory, not written to disk. Review reports before sharing:
automatic redaction cannot recognize every possible secret in third-party output.

Exit codes:
  0  No blocking local findings, or captured child exited zero (not a readiness guarantee).
  1  Local findings, failed/inconclusive capture, timeout or incomplete capture.
  2  Command-line or report-output error.
";

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Report {
        config: Option<PathBuf>,
        output: Option<PathBuf>,
    },
    Capture {
        application: Option<PathBuf>,
        arguments: Vec<OsString>,
        timeout: Duration,
        output: Option<PathBuf>,
    },
    Help,
    Version,
}

fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, &'static str> {
    let mut arguments = arguments.into_iter();
    let mut output = None;
    let mut config = None;
    let mut application = None;
    let mut timeout = None;
    let mut capture = false;
    let mut forwarded = None;
    let mut informational = None;
    while let Some(argument) = arguments.next() {
        if informational.is_some() {
            return Err("Use --help or --version alone.");
        }
        if argument == "--output" || argument == "--config" || argument == "--app" {
            let destination = if argument == "--output" {
                &mut output
            } else if argument == "--config" {
                &mut config
            } else {
                &mut application
            };
            if destination.is_some() {
                return Err("Use each path option once.");
            }
            let path = arguments
                .next()
                .ok_or("Path option requires a file path.")?;
            if path.is_empty() || path.to_str().is_some_and(|path| path.starts_with("--")) {
                return Err("Path option requires a file path.");
            }
            *destination = Some(PathBuf::from(path));
        } else if argument == "--capture-startup" {
            if capture {
                return Err("Use --capture-startup once.");
            }
            capture = true;
        } else if argument == "--timeout-secs" {
            if timeout.is_some() {
                return Err("Use --timeout-secs once.");
            }
            let value = arguments
                .next()
                .ok_or("--timeout-secs requires 1-300 seconds.")?;
            let seconds = value
                .to_str()
                .filter(|value| {
                    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                })
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|seconds| (1..=300).contains(seconds))
                .ok_or("--timeout-secs requires 1-300 seconds.")?;
            timeout = Some(Duration::from_secs(seconds));
        } else if argument == "--" {
            forwarded = Some(arguments.collect());
            break;
        } else if argument == "--help" || argument == "--version" {
            if output.is_some()
                || config.is_some()
                || application.is_some()
                || timeout.is_some()
                || capture
            {
                return Err("Use --help or --version alone.");
            }
            informational = Some(if argument == "--help" {
                Command::Help
            } else {
                Command::Version
            });
        } else {
            return Err("Unknown argument. Use --help for usage.");
        }
    }
    if let Some(command) = informational {
        return Ok(command);
    }
    if capture {
        if config.is_some() {
            return Err("--config is read-only inspection only; capture uses the application's real configuration.");
        }
        Ok(Command::Capture {
            application,
            arguments: forwarded.unwrap_or_default(),
            timeout: timeout.unwrap_or(Duration::from_secs(30)),
            output,
        })
    } else if application.is_some() || timeout.is_some() || forwarded.is_some() {
        Err("--app, --timeout-secs and forwarded arguments require --capture-startup.")
    } else {
        Ok(Command::Report { config, output })
    }
}

fn create_report_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn report_output_error(error: io::Error) -> &'static str {
    if error.kind() == io::ErrorKind::AlreadyExists {
        "Output already exists; choose a new file. Nothing overwritten."
    } else {
        "Could not create or write the report output. Check the parent directory and permissions; a partial new file may remain."
    }
}

fn write_new_report(path: &Path, report: &impl std::fmt::Display) -> io::Result<()> {
    let mut file = create_report_file(path)?;
    write!(file, "{report}")?;
    file.flush()
}

fn write_stdout(value: &impl std::fmt::Display) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    write!(stdout, "{value}")?;
    stdout.flush()
}

fn run() -> Result<u8, &'static str> {
    match parse_arguments(std::env::args_os().skip(1))? {
        Command::Help => {
            write_stdout(&HELP).map_err(|_| "Could not write help to stdout.")?;
            Ok(0)
        }
        Command::Version => {
            write_stdout(&format_args!(
                "jellypilot-diagnostics {}\n",
                env!("CARGO_PKG_VERSION")
            ))
            .map_err(|_| "Could not write version to stdout.")?;
            Ok(0)
        }
        Command::Report { config, output } => {
            let report = collect_startup_report(config.as_deref());
            if let Some(path) = output {
                write_new_report(&path, &report).map_err(report_output_error)?;
            } else {
                write_stdout(&report).map_err(|_| "Could not write the report to stdout.")?;
            }
            Ok(u8::from(report.has_blocking_findings()))
        }
        Command::Capture {
            application,
            arguments,
            timeout,
            output,
        } => {
            let current_exe = std::env::current_exe().map_err(|_| {
                "Could not locate the diagnostics executable; cannot select an application."
            })?;
            let application = match application {
                Some(path) => path,
                None => current_exe
                    .parent()
                    .ok_or("Could not locate the application directory.")?
                    .join(if cfg!(windows) {
                        "jellypilot.exe"
                    } else {
                        "jellypilot"
                    }),
            };
            if application.canonicalize().ok().as_ref() == Some(&current_exe) {
                return Err("--app must select the application, not this diagnostics helper.");
            }
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Could not initialize the capture runtime.")?;
            // Refuse bad/existing output before starting an application with side effects.
            let file = output
                .as_deref()
                .map(create_report_file)
                .transpose()
                .map_err(report_output_error)?;
            let _ = writeln!(io::stderr().lock(),
                "Capturing real startup for up to {} seconds. Normal settings and auto-login apply; the spawned process is stopped at the deadline.",
                timeout.as_secs());
            let report = runtime.block_on(capture_startup(StartupCaptureOptions {
                executable: &application,
                arguments: &arguments,
                timeout,
            }));
            // Windows pipe IO may retain blocking reads while descendants hold
            // inherited handles. Never let runtime Drop defeat the capture deadline.
            runtime.shutdown_timeout(Duration::from_millis(250));
            if let Some(mut file) = file {
                write!(file, "{report}")
                    .and_then(|()| file.flush())
                    .map_err(report_output_error)?;
            } else {
                write_stdout(&report)
                    .map_err(|_| "Could not write the capture report to stdout.")?;
            }
            Ok(u8::from(report.has_failure()))
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            // Error strings are fixed text: never print arguments, paths or raw errors.
            let _ = writeln!(io::stderr().lock(), "jellypilot-diagnostics: {message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Command, &'static str> {
        parse_arguments(arguments.iter().map(|argument| OsString::from(*argument)))
    }

    #[test]
    fn rejects_duplicate_output_and_mixed_informational_arguments() {
        for arguments in [
            vec!["--output", "first", "--output", "second"],
            vec!["--output", "file", "--help"],
            vec!["--version", "--output", "file"],
            vec!["--help", "--version"],
            vec!["--output"],
            vec!["--config", "first", "--config", "second"],
            vec!["--config", "file", "--version"],
            vec!["--config"],
            vec!["--capture-startup", "--config", "private-config"],
            vec!["--app", "program"],
            vec!["--timeout-secs", "1"],
            vec!["--capture-startup", "--timeout-secs", "0"],
            vec!["--capture-startup", "--timeout-secs", "301"],
            vec!["--capture-startup", "--timeout-secs", "+1"],
            vec!["--capture-startup", "--timeout-secs", "1.5"],
            vec!["--capture-startup", "--help"],
            vec!["--", "--external"],
        ] {
            assert!(parse(&arguments).is_err());
        }
    }

    #[test]
    fn unknown_arguments_are_not_echoed() {
        let error = parse(&["--private-token=secret"]).unwrap_err();
        assert!(!error.contains("secret"));
    }

    #[cfg(unix)]
    #[test]
    fn accepts_non_unicode_output_path() {
        use std::os::unix::ffi::OsStringExt;
        let path = OsString::from_vec(vec![b'r', 0xff]);
        assert_eq!(
            parse_arguments(["--output".into(), path.clone()]),
            Ok(Command::Report {
                config: None,
                output: Some(path.into())
            })
        );
    }
}
