//! Applies one-launch renderer policy without mutating the process environment.

use std::sync::{OnceLock, RwLock};

use iced::advanced::graphics::{compositor, Compositor, Shell};
use iced::backend;
use iced_wgpu::wgpu;
use jellypilot_core::config::SettingsStore;
use jellypilot_core::rendering::RenderBackend;

pub(crate) struct RendererSelection {
  pub configured: RenderBackend,
  pub requested: RenderBackend,
  pub source: &'static str,
  backends: wgpu::Backends,
}

static SELECTION: OnceLock<RendererSelection> = OnceLock::new();
static ACTUAL: RwLock<Option<RenderBackend>> = RwLock::new(None);

pub(crate) fn selection() -> Option<&'static RendererSelection> {
  SELECTION.get()
}

pub(crate) fn actual_backend() -> Option<String> {
  ACTUAL
    .read()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
    .map(|backend| backend.as_str().to_owned())
}

pub(crate) fn initialize(
  override_backend: Option<RenderBackend>,
  embedded: bool,
) -> Result<(), &'static str> {
  let configured = match SettingsStore::load() {
    Ok(store) => store.snapshot().render_backend(),
    Err(_) => {
      // The ordinary boot path reports configuration recovery; never expose file contents here.
      tracing::warn!("Renderer settings unavailable; using automatic renderer selection");
      RenderBackend::Auto
    }
  };
  let iced_backend = std::env::var_os("ICED_BACKEND");
  let wgpu_backend = std::env::var_os("WGPU_BACKEND");
  let selected = resolve(
    configured,
    override_backend,
    iced_backend.as_deref(),
    wgpu_backend.as_deref(),
    std::env::consts::OS,
    embedded,
  )?;
  tracing::info!(
    configured = selected.configured.as_str(),
    requested = selected.requested.as_str(),
    source = selected.source,
    backends = ?selected.backends,
    "Interface renderer selected; GPU initialization pending"
  );
  emit_capture(&selected, "pending");
  SELECTION
    .set(selected)
    .map_err(|_| "Interface renderer was already initialized")
}

fn resolve(
  configured: RenderBackend,
  override_backend: Option<RenderBackend>,
  iced_backend: Option<&std::ffi::OsStr>,
  wgpu_backend: Option<&std::ffi::OsStr>,
  platform: &str,
  embedded: bool,
) -> Result<RendererSelection, &'static str> {
  let (requested, source, override_mask) = if let Some(backend) = override_backend {
    (backend, "cli", None)
  } else if iced_backend.is_some() || wgpu_backend.is_some() {
    let software = match iced_backend {
      None => false,
      Some(value) if value == "wgpu" => false,
      Some(value) if value == "tiny-skia" || value == "tiny_skia" => true,
      Some(_) => {
        return Err("ICED_BACKEND must be wgpu or tiny-skia; use --renderer auto to override it")
      }
    };
    if software {
      (RenderBackend::Software, "environment", None)
    } else if let Some(value) = wgpu_backend {
      let value = value
        .to_str()
        .ok_or("WGPU_BACKEND must contain a valid graphics API name")?;
      let mut mask = wgpu::Backends::empty();
      let mut requested = None;
      for name in value.split(',') {
        let backend = match name.trim().to_ascii_lowercase().as_str() {
          "dx12" | "d3d12" => RenderBackend::Dx12,
          "vulkan" | "vk" => RenderBackend::Vulkan,
          "metal" | "mtl" => RenderBackend::Metal,
          "gl" | "gles" | "opengl" => RenderBackend::OpenGl,
          _ => return Err("Unsupported WGPU_BACKEND; use --renderer auto to override it"),
        };
        validate(backend, platform, embedded)?;
        requested = match requested {
          None => Some(backend),
          Some(previous) if previous == backend => Some(previous),
          Some(_) => Some(RenderBackend::Auto),
        };
        mask |= backend_mask(backend);
      }
      (
        requested.unwrap_or(RenderBackend::Auto),
        "environment",
        Some(mask),
      )
    } else {
      (RenderBackend::Auto, "environment", None)
    }
  } else {
    (configured, "settings", None)
  };
  validate(requested, platform, embedded)?;
  let requested = if override_mask.is_some() {
    requested
  } else {
    requested.resolve_auto(platform, embedded)
  };
  Ok(RendererSelection {
    configured,
    requested,
    source,
    backends: override_mask.unwrap_or_else(|| backend_mask(requested)),
  })
}

fn validate(backend: RenderBackend, platform: &str, embedded: bool) -> Result<(), &'static str> {
  if !backend.supported(platform, embedded) {
    return Err(if embedded {
      "Embedded MPV requires the Vulkan interface renderer. Choose --renderer auto or --renderer vulkan, or use --external --renderer software for recovery."
    } else {
      "The interface renderer is not supported on this platform. Use --renderer auto or --renderer software for recovery."
    });
  }
  Ok(())
}

fn backend_mask(backend: RenderBackend) -> wgpu::Backends {
  match backend {
    RenderBackend::Dx12 => wgpu::Backends::DX12,
    RenderBackend::Vulkan => wgpu::Backends::VULKAN,
    RenderBackend::Metal => wgpu::Backends::METAL,
    RenderBackend::OpenGl => wgpu::Backends::GL,
    RenderBackend::Auto | RenderBackend::Software => wgpu::Backends::empty(),
  }
}

pub(crate) async fn create_compositor(
  mut settings: backend::Settings,
  display: impl compositor::Display + Clone,
  window: impl compositor::Window + Clone,
  shell: Shell,
) -> Result<iced_renderer::Compositor, backend::Error> {
  let selected = selection().ok_or_else(|| backend::Error::GraphicsAdapterNotFound {
    backend: "jellypilot",
    reason: backend::Reason::DidNotMatch {
      preferred_backend: settings.backend.clone(),
    },
  })?;
  let compositor = if selected.requested == RenderBackend::Software {
    settings.backend = backend::Backend::Software;
    iced_renderer::Compositor::new(settings, display, window, shell).await?
  } else {
    // Construct directly: iced's convenience constructor re-reads WGPU_BACKEND,
    // which would otherwise override the explicit recovery command-line choice.
    let mut settings = iced_wgpu::window::compositor::Settings::from(settings);
    settings.backends = selected.backends;
    if let Some(mode) = iced_wgpu::window::compositor::present_mode_from_env() {
      settings.present_mode = mode;
    }
    let hardware = iced_wgpu::window::Compositor::request(settings, display, window, shell).await?;
    iced_renderer::fallback::Compositor::Primary(hardware)
  };
  observe(compositor.information());
  Ok(compositor)
}

pub(crate) fn observe(information: compositor::Information) {
  let actual = match information.backend.as_str() {
    "Vulkan" => Some(RenderBackend::Vulkan),
    "Dx12" => Some(RenderBackend::Dx12),
    "Metal" => Some(RenderBackend::Metal),
    "Gl" => Some(RenderBackend::OpenGl),
    "tiny-skia" => Some(RenderBackend::Software),
    name if name.starts_with("Vulkan · embedded MPV ABI ") => Some(RenderBackend::Vulkan),
    _ => None,
  };
  *ACTUAL
    .write()
    .unwrap_or_else(std::sync::PoisonError::into_inner) = actual;
  let label = actual.map_or("unknown", RenderBackend::as_str);
  tracing::info!(actual = label, "Interface renderer initialized");
  if let Some(selected) = selection() {
    emit_capture(selected, label);
  }
}

fn emit_capture(selected: &RendererSelection, actual: &str) {
  if std::env::var_os("JELLYPILOT_STARTUP_CAPTURE").as_deref() == Some(std::ffi::OsStr::new("1")) {
    eprintln!(
      "JELLYPILOT_RENDERER configured={} requested={} source={} actual={actual}",
      selected.configured.as_str(),
      selected.requested.as_str(),
      selected.source,
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::ffi::OsStr;

  #[test]
  fn command_line_recovers_from_saved_and_environment_backends() {
    let selected = resolve(
      RenderBackend::Vulkan,
      Some(RenderBackend::Auto),
      Some(OsStr::new("invalid")),
      Some(OsStr::new("vulkan")),
      "windows",
      false,
    )
    .unwrap();
    assert_eq!(selected.requested, RenderBackend::Dx12);
    assert_eq!(selected.backends, wgpu::Backends::DX12);
    assert_eq!(selected.configured, RenderBackend::Vulkan);
    assert_eq!(selected.source, "cli");
  }

  #[test]
  fn environment_mask_is_honored_but_cannot_escape_embedded_vulkan() {
    let selected = resolve(
      RenderBackend::Software,
      None,
      Some(OsStr::new("wgpu")),
      Some(OsStr::new("dx12,vulkan")),
      "windows",
      false,
    )
    .unwrap();
    assert_eq!(
      selected.backends,
      wgpu::Backends::DX12 | wgpu::Backends::VULKAN
    );
    assert_eq!(selected.source, "environment");
    assert!(resolve(
      RenderBackend::Auto,
      None,
      None,
      Some(OsStr::new("dx12,vulkan")),
      "windows",
      true
    )
    .is_err());
  }

  #[test]
  fn software_override_ignores_hardware_environment_without_changing_settings() {
    let selected = resolve(
      RenderBackend::Vulkan,
      None,
      Some(OsStr::new("tiny-skia")),
      Some(OsStr::new("invalid")),
      "linux",
      false,
    )
    .unwrap();
    assert_eq!(selected.requested, RenderBackend::Software);
    assert!(selected.backends.is_empty());
    assert!(resolve(
      RenderBackend::Auto,
      Some(RenderBackend::Software),
      None,
      None,
      "linux",
      true
    )
    .is_err());
  }
}
