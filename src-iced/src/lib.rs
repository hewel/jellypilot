//! Cross-platform iced application shell for JellyPilot.

mod app;
mod tray;

use std::cell::RefCell;

use iced::{window, Size};

/// Default logical window size at startup.
const DEFAULT_WINDOW_SIZE: Size = app::shell::FULL_DEFAULT_WINDOW_SIZE;
/// Minimum allowable logical window size in Full mode.
const MIN_WINDOW_SIZE: Size = app::shell::FULL_MIN_WINDOW_SIZE;
/// Fixed logical window size in Control-Only mode (min == max, non-resizable).
const CONTROL_ONLY_WINDOW_SIZE: Size = app::shell::CONTROL_ONLY_WINDOW_SIZE;

/// Bundled 256×256 application icon shown in the window decorations and taskbar.
const WINDOW_ICON_PNG: &[u8] = include_bytes!("../../assets/icons/128x128@2x.png");

/// Bundled 128×128 application icon shown in the system tray.
pub(crate) const TRAY_ICON_PNG: &[u8] = include_bytes!("../../assets/icons/128x128.png");

/// Decodes a bundled PNG into RGBA pixels for window and tray icons.
pub(crate) fn decode_icon(png: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
  let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
    .ok()?
    .to_rgba8();
  let (width, height) = image.dimensions();
  Some((image.into_raw(), width, height))
}

/// Builds the window icon from the bundled asset; absent only if decoding fails.
fn window_icon() -> Option<window::Icon> {
  let (rgba, width, height) = decode_icon(WINDOW_ICON_PNG)?;
  window::icon::from_rgba(rgba, width, height).ok()
}

/// Starts the cross-platform iced application and blocks until its window closes.
pub fn run() -> iced::Result {
  run_application(false)
}

/// Starts the iced application and exits after its first rendered window frame.
pub fn run_smoke() -> iced::Result {
  run_application(true)
}

/// Parses a `WxH` geometry string (e.g., `"1024x640"`) into an [`iced::Size`].
///
/// Trims surrounding and delimiter-adjacent whitespace. Rejects missing delimiter,
/// non-numeric values, non-positive numbers (<= 0.0), and non-finite floats (NaN, infinity).
pub(crate) fn parse_smoke_size(input: &str) -> Option<Size> {
  let trimmed = input.trim();
  let (width_str, height_str) = trimmed.split_once(['x', 'X'])?;
  let width: f32 = width_str.trim().parse().ok()?;
  let height: f32 = height_str.trim().parse().ok()?;
  if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
    return None;
  }
  Some(Size::new(width, height))
}

fn smoke_window_size() -> Size {
  match std::env::var("JELLYPILOT_SMOKE_SIZE") {
    Ok(raw) => match parse_smoke_size(&raw) {
      Some(size) => size,
      None => {
        eprintln!(
          "Warning: invalid JELLYPILOT_SMOKE_SIZE {raw:?}; using default 1600x900 window size."
        );
        DEFAULT_WINDOW_SIZE
      }
    },
    Err(_) => DEFAULT_WINDOW_SIZE,
  }
}

fn run_application(smoke: bool) -> iced::Result {
  tracing::debug!(smoke, "application booting");
  let tray = (!smoke).then(|| tray::Tray::new().ok()).flatten();
  let settings = jellypilot_core::config::SettingsStore::load().unwrap_or_default();
  let start_minimized = settings.snapshot().start_minimized();
  let control_only =
    !smoke && settings.snapshot().app_mode() == jellypilot_core::config::AppMode::ControlOnly;
  let start_hidden = should_start_hidden(start_minimized, tray.is_some());
  let window_size = if smoke {
    smoke_window_size()
  } else if control_only {
    CONTROL_ONLY_WINDOW_SIZE
  } else {
    DEFAULT_WINDOW_SIZE
  };
  let tray = RefCell::new(tray);
  let mut application = iced::application(
    move || app::boot(smoke, tray.borrow_mut().take()),
    app::update,
    app::view,
  )
  .title("JellyPilot")
  .subscription(app::subscription)
  .theme(app::theme)
  .window(window::Settings {
    size: window_size,
    min_size: Some(if control_only {
      CONTROL_ONLY_WINDOW_SIZE
    } else {
      MIN_WINDOW_SIZE
    }),
    max_size: control_only.then_some(CONTROL_ONLY_WINDOW_SIZE),
    icon: window_icon(),
    visible: !start_hidden,
    resizable: !control_only,
    ..window::Settings::default()
  })
  .exit_on_close_request(false)
  .default_font(jellypilot_ui::fonts::INTER_FONT);

  for font in jellypilot_ui::fonts::fonts() {
    application = application.font(font);
  }

  application.run()
}

const fn should_start_hidden(start_minimized: bool, tray_initialized: bool) -> bool {
  start_minimized && tray_initialized
}

#[cfg(test)]
mod tests {
  use super::should_start_hidden;

  #[test]
  fn start_minimized_requires_an_initialized_tray() {
    assert!(should_start_hidden(true, true));
    assert!(!should_start_hidden(true, false));
    assert!(!should_start_hidden(false, true));
  }

  use super::parse_smoke_size;
  use super::{decode_icon, TRAY_ICON_PNG, WINDOW_ICON_PNG};
  use iced::Size;

  #[test]
  fn parse_smoke_size_accepts_valid_dimensions() {
    assert_eq!(parse_smoke_size("1024x640"), Some(Size::new(1024.0, 640.0)));
    assert_eq!(
      parse_smoke_size("1920X1080"),
      Some(Size::new(1920.0, 1080.0))
    );
    assert_eq!(
      parse_smoke_size("800.5x600.25"),
      Some(Size::new(800.5, 600.25))
    );
  }

  #[test]
  fn parse_smoke_size_handles_whitespace_padding() {
    assert_eq!(
      parse_smoke_size("  1024x640  "),
      Some(Size::new(1024.0, 640.0))
    );
    assert_eq!(
      parse_smoke_size("1024 x 640"),
      Some(Size::new(1024.0, 640.0))
    );
    assert_eq!(
      parse_smoke_size(" \t 1280 \t X \t 720 \t "),
      Some(Size::new(1280.0, 720.0))
    );
  }

  #[test]
  fn parse_smoke_size_rejects_missing_delimiter() {
    assert_eq!(parse_smoke_size("1024"), None);
    assert_eq!(parse_smoke_size("1024 640"), None);
    assert_eq!(parse_smoke_size("1024,640"), None);
    assert_eq!(parse_smoke_size("1024:640"), None);
  }

  #[test]
  fn bundled_icons_decode_to_rgba() {
    let (rgba, width, height) = decode_icon(WINDOW_ICON_PNG).expect("window icon decodes");
    assert_eq!(rgba.len(), (width * height * 4) as usize);
    let (rgba, width, height) = decode_icon(TRAY_ICON_PNG).expect("tray icon decodes");
    assert_eq!(rgba.len(), (width * height * 4) as usize);
  }

  #[test]
  fn parse_smoke_size_rejects_non_numeric_input() {
    assert_eq!(parse_smoke_size(""), None);
    assert_eq!(parse_smoke_size("   "), None);
    assert_eq!(parse_smoke_size("abc"), None);
    assert_eq!(parse_smoke_size("1024xabc"), None);
    assert_eq!(parse_smoke_size("abcx640"), None);
    assert_eq!(parse_smoke_size("x"), None);
    assert_eq!(parse_smoke_size("1024x"), None);
    assert_eq!(parse_smoke_size("x640"), None);
  }

  #[test]
  fn parse_smoke_size_rejects_zero_and_negative_dimensions() {
    assert_eq!(parse_smoke_size("0x640"), None);
    assert_eq!(parse_smoke_size("1024x0"), None);
    assert_eq!(parse_smoke_size("0x0"), None);
    assert_eq!(parse_smoke_size("-1024x640"), None);
    assert_eq!(parse_smoke_size("1024x-640"), None);
    assert_eq!(parse_smoke_size("-1024x-640"), None);
  }

  #[test]
  fn parse_smoke_size_rejects_non_finite_values() {
    assert_eq!(parse_smoke_size("NaNx640"), None);
    assert_eq!(parse_smoke_size("1024xNaN"), None);
    assert_eq!(parse_smoke_size("infx640"), None);
    assert_eq!(parse_smoke_size("1024xinf"), None);
    assert_eq!(parse_smoke_size("-infx640"), None);
  }

  #[test]
  fn parse_smoke_size_rejects_multiple_delimiters() {
    assert_eq!(parse_smoke_size("1024x640x720"), None);
  }
}
