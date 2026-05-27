//! MPV process detection and spawning.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessError {
  #[error("MPV executable not found")]
  NotFound,
  #[error("Failed to spawn MPV: {0}")]
  SpawnFailed(#[from] std::io::Error),
}

/// Get the IPC socket/pipe path for MPV.
/// Uses PID suffix to prevent collisions when multiple JMSR instances run.
///
/// On Linux, respects `XDG_RUNTIME_DIR` for AppImage/Flatpak compatibility
/// where `/tmp` may be inaccessible inside sandboxes.
pub fn ipc_path() -> String {
  let pid = std::process::id();
  #[cfg(windows)]
  {
    format!(r"\\.\pipe\jmsr-mpv-{}", pid)
  }
  #[cfg(not(windows))]
  {
    let base_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    format!("{}/jmsr-mpv-{}.sock", base_dir, pid)
  }
}

/// Get the path to JMSR's custom input.conf for MPV keybindings.
pub fn jmsr_input_conf_path() -> Option<PathBuf> {
  dirs::config_dir().map(|p| p.join("jmsr").join("input.conf"))
}

/// Write JMSR's input.conf with the specified keybindings.
/// Always overwrites the file with the provided keybindings.
pub fn write_input_conf(
  keybind_next: &str,
  keybind_prev: &str,
  keybind_intro_skip: &str,
) -> Option<PathBuf> {
  let path = jmsr_input_conf_path()?;

  // Create parent directory if needed
  if let Some(parent) = path.parent() {
    if !parent.exists() {
      if let Err(e) = std::fs::create_dir_all(parent) {
        log::warn!("Failed to create JMSR config directory: {}", e);
        return None;
      }
    }
  }

  let bindings = format!(
    r#"# JMSR MPV Keybindings
# These keybindings are used by JMSR to control episode navigation.
# You can customize these bindings in JMSR Settings.

{} script-message jmsr-next    # Play next episode
{} script-message jmsr-prev    # Play previous episode
{} script-message jmsr-skip-intro    # Skip active Intro Skipper segment
"#,
    keybind_next, keybind_prev, keybind_intro_skip
  );

  if let Err(e) = std::fs::write(&path, bindings) {
    log::warn!("Failed to write JMSR input.conf: {}", e);
    return None;
  }
  log::info!("Updated JMSR input.conf at {:?}", path);

  Some(path)
}

/// Ensure JMSR's input.conf exists with default keybindings.
fn ensure_input_conf() -> Option<PathBuf> {
  let path = jmsr_input_conf_path()?;

  // Only create if it doesn't exist (preserve user customizations via config)
  if !path.exists() {
    return write_input_conf("Shift+>", "Shift+<", "g");
  }

  Some(path)
}

/// Canonicalize a path to resolve symlinks and junctions.
/// This fixes "os error 448" (Untrusted Mount Point) with Scoop's `current` junction.
fn canonicalize_path(path: PathBuf) -> PathBuf {
  match path.canonicalize() {
    Ok(canonical) => {
      log::debug!("Canonicalized {:?} -> {:?}", path, canonical);
      canonical
    }
    Err(e) => {
      log::warn!("Failed to canonicalize {:?}: {}", path, e);
      path
    }
  }
}

/// On Windows, ensure we use mpv.exe instead of mpv.com.
/// mpv.com is a console wrapper that spawns a black terminal window.
#[cfg(windows)]
fn ensure_mpv_exe(path: PathBuf) -> PathBuf {
  if path.extension().map(|e| e == "com").unwrap_or(false) {
    let exe_path = path.with_extension("exe");
    if exe_path.exists() {
      log::info!("Switching from mpv.com to mpv.exe: {:?}", exe_path);
      return exe_path;
    }
  }
  path
}

#[cfg(not(windows))]
fn ensure_mpv_exe(path: PathBuf) -> PathBuf {
  path
}

/// Find MPV executable in common locations.
pub fn find_mpv() -> Option<PathBuf> {
  // Check PATH first
  if let Ok(path) = which::which("mpv") {
    let path = ensure_mpv_exe(path);
    let path = canonicalize_path(path);
    return Some(path);
  }

  // Platform-specific common locations
  #[cfg(windows)]
  {
    let common_paths = [
      r"C:\Program Files\mpv\mpv.exe",
      r"C:\Program Files (x86)\mpv\mpv.exe",
      r"C:\mpv\mpv.exe",
    ];
    for path in common_paths {
      let p = PathBuf::from(path);
      if p.exists() {
        return Some(canonicalize_path(p));
      }
    }
  }

  #[cfg(target_os = "macos")]
  {
    let common_paths = [
      "/usr/local/bin/mpv",
      "/opt/homebrew/bin/mpv",
      "/Applications/mpv.app/Contents/MacOS/mpv",
    ];
    for path in common_paths {
      let p = PathBuf::from(path);
      if p.exists() {
        return Some(canonicalize_path(p));
      }
    }
  }

  #[cfg(target_os = "linux")]
  {
    let common_paths = ["/usr/bin/mpv", "/usr/local/bin/mpv"];
    for path in common_paths {
      let p = PathBuf::from(path);
      if p.exists() {
        return Some(canonicalize_path(p));
      }
    }
  }

  None
}

/// Spawn MPV process with IPC server enabled.
pub fn spawn_mpv(mpv_path: Option<&PathBuf>, extra_args: &[String]) -> Result<Child, ProcessError> {
  let mpv_exe = mpv_path
    .cloned()
    .or_else(find_mpv)
    .ok_or(ProcessError::NotFound)?;

  let ipc = ipc_path();

  log::info!("Spawning MPV: {:?} with IPC: {}", mpv_exe, ipc);
  if !extra_args.is_empty() {
    log::info!("Extra MPV args: {:?}", extra_args);
  }

  let mut cmd = Command::new(&mpv_exe);
  cmd
    .arg(format!("--input-ipc-server={}", ipc))
    .arg("--idle")
    .arg("--force-window")
    .arg("--keep-open=no")
    .arg("--no-terminal")
    .arg("--osc");

  // Add JMSR keybindings via input.conf
  // Using --input-conf appends to (not replaces) the user's input.conf
  if let Some(input_conf) = ensure_input_conf() {
    cmd.arg(format!("--input-conf={}", input_conf.display()));
    log::info!("Using JMSR input.conf: {:?}", input_conf);
  }

  // Add user-specified extra arguments
  for arg in extra_args {
    cmd.arg(arg);
  }

  let child = cmd
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()?;

  Ok(child)
}

/// Kill MPV process and cleanup socket.
pub fn cleanup_ipc() {
  #[cfg(not(windows))]
  {
    let path = ipc_path();
    let _ = std::fs::remove_file(&path);
  }
  // Windows named pipes are cleaned up automatically
}
