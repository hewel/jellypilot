//! Explicit embedded playback startup selection; the default path never loads libmpv.

#[cfg(target_os = "linux")]
pub(crate) mod compositor;
#[cfg(target_os = "linux")]
pub(crate) mod retained;
#[cfg(target_os = "linux")]
mod video;

pub(crate) fn view<'a, Message: 'a>() -> iced::Element<'a, Message> {
  #[cfg(target_os = "linux")]
  {
    iced::widget::shader(video::Video)
      .width(iced::Fill)
      .height(iced::Fill)
      .into()
  }
  #[cfg(not(target_os = "linux"))]
  {
    iced::widget::text("Embedded MPV is unavailable on this platform").into()
  }
}

#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use jellypilot_core::config::{PlaybackBackend, SettingsStore};

static OPTIONS: OnceLock<Options> = OnceLock::new();

#[derive(Debug)]
pub(crate) struct Options {
  #[cfg(target_os = "linux")]
  pub libmpv: PathBuf,
  #[cfg(target_os = "linux")]
  pub baseline: PathBuf,
  pub ipc: PathBuf,
  #[cfg(target_os = "linux")]
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
  let Some(options) = options() else { return };
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

#[cfg(target_os = "linux")]
fn asset(variable: &str, root: &Path, relative: &str) -> Result<PathBuf, String> {
  let path = std::env::var_os(variable).map_or_else(|| root.join(relative), PathBuf::from);
  if !path.is_absolute() {
    return Err(format!("{variable} must be an absolute path"));
  }
  path.canonicalize().map_err(|_| {
    format!("Embedded MPV asset missing: {}. Run the pinned MPV build task or set {variable}; system libmpv is not used.", path.display())
  })
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
  #[cfg(not(target_os = "linux"))]
  return Err("Embedded MPV is available only on Linux Vulkan. Start with --external to keep using external MPV.".into());
  #[cfg(target_os = "linux")]
  {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let root = executable
      .parent()
      .ok_or("Application directory unavailable")?;
    let libmpv = asset("JELLYPILOT_LIBMPV", root, "lib/jellypilot/libmpv.so")?;
    let baseline = asset(
      "JELLYPILOT_MPV_BASELINE",
      root,
      "share/jellypilot/mpv-baseline.conf",
    )?;
    // A process-private directory prevents another local user replacing the IPC socket.
    let directory =
      std::env::temp_dir().join(format!("jellypilot-embedded-{}", std::process::id()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
      use std::os::unix::fs::DirBuilderExt;
      builder.mode(0o700);
    }
    builder
      .create(&directory)
      .map_err(|error| error.to_string())?;
    OPTIONS
      .set(Options {
        libmpv,
        baseline,
        ipc: directory.join("mpv.sock"),
        engine_factory: _engine_factory,
      })
      .map_err(|_| "Embedded playback already initialized".to_owned())
  }
}
