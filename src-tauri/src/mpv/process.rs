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
/// Uses PID suffix to prevent collisions when multiple JellyPilot instances run.
///
/// On Linux, respects `XDG_RUNTIME_DIR` for AppImage/Flatpak compatibility
/// where `/tmp` may be inaccessible inside sandboxes.
pub fn ipc_path() -> String {
  let pid = std::process::id();
  #[cfg(windows)]
  {
    format!(r"\\.\pipe\jellypilot-mpv-{}", pid)
  }
  #[cfg(not(windows))]
  {
    let base_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    format!("{}/jellypilot-mpv-{}.sock", base_dir, pid)
  }
}

/// Get the path to JellyPilot's custom input.conf for MPV keybindings.
pub fn jellypilot_input_conf_path() -> Option<PathBuf> {
  dirs::config_dir().map(|p| p.join("jellypilot").join("input.conf"))
}

fn legacy_input_conf_path() -> Option<PathBuf> {
  dirs::config_dir().map(|p| p.join("jmsr").join("input.conf"))
}

fn legacy_key_for_command(input: &str, command: &str, fallback: &str) -> String {
  input
    .lines()
    .find_map(|line| {
      line
        .split_once(command)
        .map(|(key, _)| key.trim().to_string())
    })
    .filter(|key| !key.is_empty())
    .unwrap_or_else(|| fallback.to_string())
}

fn migrated_legacy_keybindings(input: &str) -> (String, String, String) {
  (
    legacy_key_for_command(input, "script-message jmsr-next", "Shift+>"),
    legacy_key_for_command(input, "script-message jmsr-prev", "Shift+<"),
    legacy_key_for_command(input, "script-message jmsr-skip-intro", "g"),
  )
}

/// Write JellyPilot's input.conf with the specified keybindings.
/// Always overwrites the file with the provided keybindings.
pub fn write_input_conf(
  keybind_next: &str,
  keybind_prev: &str,
  keybind_intro_skip: &str,
) -> Option<PathBuf> {
  let path = jellypilot_input_conf_path()?;

  // Create parent directory if needed
  if let Some(parent) = path.parent() {
    if !parent.exists() {
      if let Err(e) = std::fs::create_dir_all(parent) {
        log::warn!("Failed to create JellyPilot config directory: {}", e);
        return None;
      }
    }
  }

  let bindings = format!(
    r#"# JellyPilot MPV Keybindings
# These keybindings are used by JellyPilot to control episode navigation.
# You can customize these bindings in JellyPilot Settings.

{} script-message jellypilot-next    # Play next episode
{} script-message jellypilot-prev    # Play previous episode
{} script-message jellypilot-skip-intro    # Skip active Intro Skipper segment
"#,
    keybind_next, keybind_prev, keybind_intro_skip
  );

  if let Err(e) = std::fs::write(&path, bindings) {
    log::warn!("Failed to write JellyPilot input.conf: {}", e);
    return None;
  }
  log::info!("Updated JellyPilot input.conf at {:?}", path);

  Some(path)
}

/// Ensure JellyPilot's input.conf exists with default keybindings.
fn ensure_input_conf() -> Option<PathBuf> {
  let path = jellypilot_input_conf_path()?;

  // Only create if it doesn't exist (preserve user customizations via config)
  if !path.exists() {
    if let Some((next, prev, intro)) = legacy_input_conf_path()
      .filter(|legacy_path| legacy_path.exists())
      .and_then(|legacy_path| std::fs::read_to_string(legacy_path).ok())
      .map(|legacy| migrated_legacy_keybindings(&legacy))
    {
      return write_input_conf(&next, &prev, &intro);
    }
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

  // Add JellyPilot keybindings via input.conf
  // Using --input-conf appends to (not replaces) the user's input.conf
  if let Some(input_conf) = ensure_input_conf() {
    cmd.arg(format!("--input-conf={}", input_conf.display()));
    log::info!("Using JellyPilot input.conf: {:?}", input_conf);
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

#[cfg(test)]
mod tests {
  use super::migrated_legacy_keybindings;

  #[test]
  fn migrated_legacy_keybindings_maps_old_script_messages_to_new_writer_keys() {
    let legacy = r#"
Alt+n script-message jmsr-next
Alt+p script-message jmsr-prev
i script-message jmsr-skip-intro
"#;

    assert_eq!(
      migrated_legacy_keybindings(legacy),
      ("Alt+n".to_string(), "Alt+p".to_string(), "i".to_string())
    );
  }
}
