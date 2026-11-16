//! Tests the console executable itself, not the desktop launch route.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "jellypilot-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jellypilot-diagnostics"));
        command
            .current_dir(&self.root)
            .env("HOME", &self.root)
            .env("USERPROFILE", &self.root)
            .env("XDG_CONFIG_HOME", &self.root)
            .env("APPDATA", &self.root)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env(
                "JELLYPILOT_LIBMPV",
                self.root.join("missing-private-runtime"),
            )
            .env(
                "JELLYPILOT_MPV_BASELINE",
                self.root.join("missing-private-baseline"),
            );
        command
    }

    fn report_command(&self) -> Command {
        let mut command = self.command();
        command
            .arg("--config")
            .arg(self.root.join("jellypilot/config.json"));
        command
    }

    fn config(&self, contents: &[u8]) -> PathBuf {
        let directory = self.root.join("jellypilot");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.json");
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(output: &[u8]) -> &str {
    std::str::from_utf8(output).unwrap()
}

fn assert_command_error(output: &Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn output_never_overwrites_existing_file_or_echoes_its_private_name() {
    let fixture = Fixture::new();
    let path = fixture.root.join("private-token-output.txt");
    fs::write(&path, b"original-private-content").unwrap();
    let output = fixture
        .report_command()
        .arg("--output")
        .arg(&path)
        .output()
        .unwrap();
    assert_command_error(&output);
    assert_eq!(fs::read(&path).unwrap(), b"original-private-content");
    assert!(!text(&output.stderr).contains("private-token"));
    assert!(!text(&output.stderr).contains(&fixture.root.to_string_lossy().to_string()));
}

#[test]
fn headless_corrupt_config_produces_private_read_only_report_with_findings() {
    let fixture = Fixture::new();
    let contents = b"{private-invalid-config https://user:password@example.invalid/?token=secret";
    let path = fixture.config(contents);
    let output = fixture.report_command().output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report = text(&output.stdout);
    assert!(report.contains("[issue] Configuration"));
    assert!(report.contains("[not probed] GPU and desktop"));
    assert!(!report.contains("private-invalid-config"));
    assert!(!report.contains("password"));
    assert!(!report.contains(&fixture.root.to_string_lossy().to_string()));
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(&path).unwrap(), contents);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn output_creates_new_file_and_preserves_findings_exit_code() {
    let fixture = Fixture::new();
    fixture.config(b"not json");
    let path = fixture.root.join("report.txt");
    let output = fixture
        .report_command()
        .arg("--output")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(fs::read_to_string(path)
        .unwrap()
        .contains("[issue] Configuration"));
}

#[test]
fn command_and_output_errors_do_not_echo_untrusted_arguments_or_paths() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .arg("--private-credential=secret")
        .output()
        .unwrap();
    assert_command_error(&output);
    assert!(!text(&output.stderr).contains("secret"));
    let output = fixture
        .report_command()
        .arg("--output")
        .arg(fixture.root.join("private-missing-dir/report.txt"))
        .output()
        .unwrap();
    assert_command_error(&output);
    assert!(!text(&output.stderr).contains("private-missing-dir"));
}

#[test]
fn help_and_version_do_not_require_a_desktop_or_valid_configuration() {
    let fixture = Fixture::new();
    fixture.config(b"not json");
    for argument in ["--help", "--version"] {
        let output = fixture.command().arg(argument).output().unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(!text(&output.stdout).contains("[issue]"));
    }
}

#[cfg(unix)]
#[test]
fn configured_mpv_is_never_executed_and_external_selection_can_pass_locally() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    // Relative explicit path avoids embedding or escaping any private fixture path.
    let player = fixture.root.join("do-not-run-mpv");
    fs::write(&player, b"#!/bin/sh\ntouch player-was-executed\n").unwrap();
    fs::set_permissions(&player, fs::Permissions::from_mode(0o700)).unwrap();
    fixture.config(br#"{"remember":false,"server_url":"","provider":"","username":"","playback_backend":"external","settings_revision":1,"mpv_path":"./do-not-run-mpv"}"#);
    let output = fixture
        .report_command()
        .env("DISPLAY", "private-display-value")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stdout));
    assert!(!fixture.root.join("player-was-executed").exists());
    assert!(!text(&output.stdout).contains("private-display-value"));
}

#[test]
fn missing_application_still_produces_a_capture_report() {
    let fixture = Fixture::new();
    let report = fixture.root.join("startup.txt");
    let output = fixture
        .command()
        .args(["--capture-startup", "--app"])
        .arg(fixture.root.join("private-missing-application"))
        .arg("--output")
        .arg(&report)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let contents = fs::read_to_string(report).unwrap();
    assert!(contents.contains("Spawn failed:"));
    assert!(contents.contains("NotFound"));
    assert!(!contents.contains("private-missing-application"));
    assert!(!contents.contains(&fixture.root.to_string_lossy().to_string()));
    assert!(output.stdout.is_empty());
}

#[cfg(feature = "desktop")]
#[test]
fn capture_reports_real_application_startup_error_before_gui_initialization() {
    let fixture = Fixture::new();
    // Conflicting backend flags fail before settings are loaded. Smoke only
    // bypasses activation of another user's running instance; no GUI is reached.
    let output = fixture
        .command()
        .args(["--capture-startup", "--timeout-secs", "10", "--app"])
        .arg(env!("CARGO_BIN_EXE_jellypilot"))
        .args(["--", "--smoke-test", "--embedded", "--external"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stderr));
    let report = text(&output.stdout);
    assert!(
        report.contains("Choose either --embedded or --external"),
        "{report}"
    );
    assert!(report.contains("backend-init-enter"), "{report}");
    assert!(!report.contains(&fixture.root.to_string_lossy().to_string()));
    assert!(!fixture.root.join("jellypilot").exists());
}

#[cfg(unix)]
#[test]
fn existing_capture_output_is_rejected_before_application_can_run() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let application = fixture.root.join("application");
    fs::write(&application, b"#!/bin/sh\ntouch application-was-started\n").unwrap();
    fs::set_permissions(&application, fs::Permissions::from_mode(0o700)).unwrap();
    let report = fixture.root.join("report.txt");
    fs::write(&report, b"original report").unwrap();
    let output = fixture
        .command()
        .args(["--capture-startup", "--app"])
        .arg(application)
        .arg("--output")
        .arg(&report)
        .output()
        .unwrap();
    assert_command_error(&output);
    assert_eq!(fs::read(report).unwrap(), b"original report");
    assert!(!fixture.root.join("application-was-started").exists());
}

#[test]
fn capture_deadline_includes_runtime_shutdown_with_inherited_pipes() {
    let fixture = Fixture::new();
    let started = std::time::Instant::now();
    let output = fixture
        .command()
        .args(["--capture-startup", "--timeout-secs", "1", "--app"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--",
            "--ignored",
            "--exact",
            "inherited_pipe_parent",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
    assert_eq!(output.status.code(), Some(1), "{}", text(&output.stdout));
    assert!(
        text(&output.stdout).contains("Child exited with code 0."),
        "{}",
        text(&output.stdout)
    );
}

#[test]
#[ignore = "subprocess fixture for capture deadlines"]
fn inherited_pipe_parent() {
    let _child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "inherited_pipe_holder",
            "--nocapture",
        ])
        .spawn()
        .unwrap();
    // The parent must exit without reaping to exercise inherited pipe ownership.
    #[expect(
        clippy::disallowed_methods,
        reason = "subprocess fixture deliberately exits without reaping child to exercise inherited pipe ownership"
    )]
    std::process::exit(0);
}

#[test]
#[ignore = "subprocess fixture for inherited handles"]
fn inherited_pipe_holder() {
    std::thread::sleep(std::time::Duration::from_secs(5));
}
