//! Embedded MPV startup on Linux and Windows: load the pinned fork unless External is selected.

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) mod compositor;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) mod retained;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod video;

pub(crate) fn view<'a, Message: 'a>() -> iced::Element<'a, Message> {
  #[cfg(any(target_os = "linux", target_os = "windows"))]
  {
    iced::widget::shader(video::Video)
      .width(iced::Fill)
      .height(iced::Fill)
      .into()
  }
  #[cfg(not(any(target_os = "linux", target_os = "windows")))]
  {
    iced::widget::text("Embedded MPV is unavailable on this platform").into()
  }
}

use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use jellypilot_core::config::{PlaybackBackend, SettingsStore};

static OPTIONS: OnceLock<Options> = OnceLock::new();

#[derive(Debug)]
pub(crate) struct Options {
  #[cfg(any(target_os = "linux", target_os = "windows"))]
  pub libmpv: PathBuf,
  #[cfg(any(target_os = "linux", target_os = "windows"))]
  pub baseline: PathBuf,
  #[cfg(any(target_os = "linux", target_os = "windows"))]
  pub demuxer_cache_dir: PathBuf,
  pub ipc: PathBuf,
  #[cfg(any(target_os = "linux", target_os = "windows"))]
  pub engine_factory: crate::EmbeddedEngineFactory,
}

pub(crate) fn options() -> Option<&'static Options> {
  OPTIONS.get()
}

pub(crate) fn enabled() -> bool {
  options().is_some()
}

/// Called only after the daemon has dropped its compositor and terminated libmpv.
pub(crate) fn cleanup() {
  // Windows named pipes are kernel objects owned by mpv; they vanish with the
  // process, so only the Linux filesystem socket needs removal.
  #[cfg(target_os = "linux")]
  if let Some(options) = options() {
    if let Err(error) = std::fs::remove_file(&options.ipc) {
      if error.kind() != std::io::ErrorKind::NotFound {
        tracing::warn!(%error, "Could not remove embedded IPC socket");
      }
    }
    if let Some(directory) = options.ipc.parent() {
      if let Err(error) = std::fs::remove_dir(directory) {
        if error.kind() != std::io::ErrorKind::NotFound {
          tracing::warn!(%error, "Could not remove embedded IPC directory");
        }
      }
    }
  }
}

/// Whether the embedded IPC endpoint is still live after daemon cleanup.
/// Linux checks the socket file; a live Windows named pipe accepts CreateFile
/// while a destroyed one fails to open.
#[cfg(target_os = "linux")]
pub(crate) fn ipc_endpoint_alive(path: &Path) -> bool {
  path.exists()
}
#[cfg(target_os = "windows")]
pub(crate) fn ipc_endpoint_alive(path: &Path) -> bool {
  std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .open(path)
    .is_ok()
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(crate) fn ipc_endpoint_alive(_path: &Path) -> bool {
  false
}

/// Packaged libmpv location relative to the executable or its prefix.
#[cfg(target_os = "linux")]
const LIBMPV_RELATIVE: &str = "lib/jellypilot/libmpv.so";
#[cfg(target_os = "windows")]
const LIBMPV_RELATIVE: &str = "lib/jellypilot/libmpv-2.dll";

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn bundled_asset_candidates(exe_dir: &Path, relative: &str) -> [PathBuf; 2] {
  let beside = exe_dir.join(relative);
  let prefixed = exe_dir
    .parent()
    .map_or_else(|| beside.clone(), |prefix| prefix.join(relative));
  [beside, prefixed]
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn resolve_asset(
  override_path: Option<PathBuf>,
  exe_dir: &Path,
  relative: &str,
  variable: &str,
) -> Result<PathBuf, String> {
  if let Some(path) = override_path {
    if !path.is_absolute() {
      return Err(format!("{variable} must be an absolute path"));
    }
    return path.canonicalize().map_err(|_| {
      format!(
        "Embedded MPV asset missing: {}. Run the pinned MPV build task or set {variable}; system libmpv is not used.",
        path.display()
      )
    });
  }
  let candidates = bundled_asset_candidates(exe_dir, relative);
  for candidate in &candidates {
    if let Ok(path) = candidate.canonicalize() {
      return Ok(path);
    }
  }
  Err(format!(
    "Embedded MPV asset missing: {} (also tried {}). Linux packages install the pinned fork under the executable prefix; development builds stage target/embedded-mpv. Set {variable} only for a trusted pinned ABI. System libmpv is not used.",
    candidates[0].display(),
    candidates[1].display()
  ))
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn asset(variable: &str, root: &Path, relative: &str) -> Result<PathBuf, String> {
  resolve_asset(
    std::env::var_os(variable).map(PathBuf::from),
    root,
    relative,
    variable,
  )
}

pub(crate) fn initialize(
  smoke: bool,
  _engine_factory: crate::EmbeddedEngineFactory,
) -> Result<(), String> {
  let arguments: Vec<_> = std::env::args().collect();
  let explicit_embedded = arguments.iter().any(|arg| arg == "--embedded");
  let explicit_external = arguments.iter().any(|arg| arg == "--external");
  if explicit_embedded && explicit_external {
    return Err("Choose either --embedded or --external, not both".into());
  }
  let selected = if explicit_external || (smoke && !explicit_embedded) {
    PlaybackBackend::External
  } else if explicit_embedded {
    PlaybackBackend::Embedded
  } else {
    // The ordinary boot path owns configuration-load diagnostics and recovery.
    SettingsStore::load()
      .map(|store| store.snapshot().playback_backend())
      .unwrap_or_default()
  };
  if selected == PlaybackBackend::External {
    return Ok(());
  }
  #[cfg(not(any(target_os = "linux", target_os = "windows")))]
  return Err("Embedded MPV is available only on Linux and Windows Vulkan. Start with --external to keep using external MPV.".into());
  #[cfg(any(target_os = "linux", target_os = "windows"))]
  {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let root = executable
      .parent()
      .ok_or("Application directory unavailable")?;
    let libmpv = asset("JELLYPILOT_LIBMPV", root, LIBMPV_RELATIVE)?;
    let baseline = asset(
      "JELLYPILOT_MPV_BASELINE",
      root,
      "share/jellypilot/mpv-baseline.conf",
    )?;
    let demuxer_cache_dir = dirs::cache_dir()
      .ok_or("Application cache directory unavailable for embedded MPV")?
      .join("jellypilot")
      .join("mpv");
    // Linux uses a filesystem socket inside a process-private directory so no
    // other local user can replace it. Windows uses a per-process named pipe,
    // matching the external mpv endpoint convention in process.rs.
    #[cfg(target_os = "linux")]
    let ipc = {
      let directory =
        std::env::temp_dir().join(format!("jellypilot-embedded-{}", std::process::id()));
      let mut builder = std::fs::DirBuilder::new();
      {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
      }
      builder
        .create(&directory)
        .map_err(|error| error.to_string())?;
      directory.join("mpv.sock")
    };
    #[cfg(target_os = "windows")]
    let ipc = PathBuf::from(format!(
      r"\\.\pipe\jellypilot-embedded-{}",
      std::process::id()
    ));
    OPTIONS
      .set(Options {
        libmpv,
        baseline,
        demuxer_cache_dir,
        ipc,
        engine_factory: _engine_factory,
      })
      .map_err(|_| "Embedded playback already initialized".to_owned())
  }
}

#[cfg(all(test, any(target_os = "linux", target_os = "windows")))]
mod tests {
  use super::{bundled_asset_candidates, resolve_asset, LIBMPV_RELATIVE};
  use std::fs;
  use std::path::{Path, PathBuf};

  fn write_file(path: &Path) {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, b"asset").unwrap();
  }

  #[test]
  fn prefix_install_uses_usr_lib_not_usr_bin_lib() {
    let root =
      std::env::temp_dir().join(format!("jellypilot-embedded-prefix-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let exe_dir = root.join("usr/bin");
    let library = root.join("usr").join(LIBMPV_RELATIVE);
    write_file(&library);
    let resolved = resolve_asset(None, &exe_dir, LIBMPV_RELATIVE, "JELLYPILOT_LIBMPV").unwrap();
    assert_eq!(resolved, library.canonicalize().unwrap());
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn development_layout_keeps_assets_beside_the_binary() {
    let root = std::env::temp_dir().join(format!("jellypilot-embedded-dev-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let exe_dir = root.join("release");
    let library = exe_dir.join(LIBMPV_RELATIVE);
    write_file(&library);
    let resolved = resolve_asset(None, &exe_dir, LIBMPV_RELATIVE, "JELLYPILOT_LIBMPV").unwrap();
    assert_eq!(resolved, library.canonicalize().unwrap());
    let candidates = bundled_asset_candidates(&exe_dir, LIBMPV_RELATIVE);
    assert_eq!(candidates[0], library);
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn override_must_be_an_existing_absolute_file() {
    let exe_dir = Path::new("/usr/bin");
    let error = resolve_asset(
      Some(PathBuf::from("relative.so")),
      exe_dir,
      LIBMPV_RELATIVE,
      "JELLYPILOT_LIBMPV",
    )
    .unwrap_err();
    assert!(error.contains("must be an absolute path"));
  }
}
