//! Cross-platform iced application shell for JellyPilot.

mod app;
mod tray;

use std::cell::RefCell;

use iced::{window, Size};

/// Default logical window size at startup.
const DEFAULT_WINDOW_SIZE: Size = Size::new(1600.0, 900.0);

/// Minimum allowable logical window size.
const MIN_WINDOW_SIZE: Size = Size::new(1024.0, 640.0);

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
  let tray = (!smoke).then(|| tray::Tray::new().ok()).flatten();
  let start_minimized = jellypilot_core::config::SettingsStore::load()
    .unwrap_or_default()
    .snapshot()
    .start_minimized();
  let start_hidden = should_start_hidden(start_minimized, tray.is_some());
  let window_size = if smoke {
    smoke_window_size()
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
    min_size: Some(MIN_WINDOW_SIZE),
    visible: !start_hidden,
    resizable: true,
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
