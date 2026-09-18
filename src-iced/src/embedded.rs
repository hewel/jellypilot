//! Embedded MPV startup on Linux and Windows: load the pinned fork unless External is selected.

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) mod compositor;
pub(crate) mod idle;
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
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{LazyLock, OnceLock};

use jellypilot_core::config::{HdrOutput, PlaybackBackend, SettingsStore};

static OPTIONS: OnceLock<Options> = OnceLock::new();

/// Mirrors the persisted HDR output preference for the compositor, which runs
/// outside application state. Written at startup and on settings edits; read
/// on surface (re)configuration. Defaults to Auto until initialize runs.
static HDR_OUTPUT: AtomicU8 = AtomicU8::new(0);

pub(crate) fn set_hdr_output(mode: HdrOutput) {
  HDR_OUTPUT.store(mode as u8, Ordering::Relaxed);
}

pub(crate) fn hdr_output() -> HdrOutput {
  match HDR_OUTPUT.load(Ordering::Relaxed) {
    1 => HdrOutput::On,
    2 => HdrOutput::Off,
    _ => HdrOutput::Auto,
  }
}

static HDR_CONTENT: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_hdr_content(hdr: bool) {
  HDR_CONTENT.store(hdr, Ordering::Relaxed);
}

pub(crate) fn hdr_content() -> bool {
  HDR_CONTENT.load(Ordering::Relaxed)
}

/// Decoder transfer notifications are independent of controls visibility,
/// account sessions and pause. A connection loss must not leave Auto in HDR.
pub(crate) fn hdr_content_events() -> impl iced::futures::Stream<Item = bool> {
  use iced::futures::SinkExt;
  use jellypilot_mpv::video_source::VideoSourceObserver;

  iced::stream::channel(1, async move |mut output| {
    let Some(options) = options() else {
      return;
    };
    let mut current = false;
    if output.send(current).await.is_err() {
      return;
    }
    loop {
      if let Ok(mut observer) = VideoSourceObserver::connect(&options.ipc).await {
        while let Ok(hdr) = observer.next_hdr().await {
          if hdr != current {
            current = hdr;
            if output.send(current).await.is_err() {
              return;
            }
          }
        }
      }
      if current {
        current = false;
        if output.send(current).await.is_err() {
          return;
        }
      }
      // The compositor may not yet exist at subscription startup. The same
      // endpoint is reused if the retained host is recreated.
      tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
  })
}

/// Compositor-observed HDR presentation state, surfaced in the diagnostics
/// information string and the settings page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HdrState {
  /// SDR presentation: HDR off, or no HDR content under Auto.
  Sdr,
  /// HDR10 PQ presentation active.
  Active,
  /// HDR requested (On, or Auto with HDR content) but the chain cannot signal it.
  Unavailable,
}

static HDR_STATE: LazyLock<tokio::sync::watch::Sender<HdrState>> =
  LazyLock::new(|| tokio::sync::watch::channel(HdrState::Sdr).0);

pub(crate) fn set_hdr_state(state: HdrState) {
  HDR_STATE.send_if_modified(|current| {
    if *current == state {
      return false;
    }
    *current = state;
    true
  });
}

pub(crate) fn hdr_state() -> HdrState {
  *HDR_STATE.borrow()
}

/// Event-driven status projection: a compositor transition must rebuild the
/// settings view even when no input, playback tick or animation is active.
pub(crate) fn hdr_events() -> impl iced::futures::Stream<Item = HdrState> {
  use iced::futures::SinkExt;
  iced::stream::channel(1, async move |mut output| {
    let mut receiver = HDR_STATE.subscribe();
    loop {
      let state = *receiver.borrow_and_update();
      if output.send(state).await.is_err() || receiver.changed().await.is_err() {
        break;
      }
    }
  })
}

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
  // Load once for both backend selection and the initial HDR preference.
  // The ordinary boot path owns load-error diagnostics and recovery.
  let settings = if explicit_external || (smoke && !explicit_embedded) {
    None
  } else {
    SettingsStore::load().ok()
  };
  let selected = if explicit_external || (smoke && !explicit_embedded) {
    PlaybackBackend::External
  } else if explicit_embedded {
    PlaybackBackend::Embedded
  } else {
    settings
      .as_ref()
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
    // The compositor reads this mirror on every surface (re)configuration;
    // settings edits update it without restarting embedded playback.
    set_hdr_output(
      settings
        .as_ref()
        .map(|store| store.snapshot().hdr_output())
        .unwrap_or_default(),
    );
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
