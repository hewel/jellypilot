//! Cross-platform iced application shell for JellyPilot.

mod app;
mod tray;

use std::cell::RefCell;

use iced::{window, Size};

/// Starts the cross-platform iced application and blocks until its window closes.
pub fn run() -> iced::Result {
  run_application(false)
}

/// Starts the iced application and exits after its first rendered window frame.
pub fn run_smoke() -> iced::Result {
  run_application(true)
}

fn run_application(smoke: bool) -> iced::Result {
  let tray = (!smoke).then(|| tray::Tray::new().ok()).flatten();
  let start_minimized = jellypilot_core::config::SettingsStore::load()
    .unwrap_or_default()
    .snapshot()
    .start_minimized();
  let start_hidden = should_start_hidden(start_minimized, tray.is_some());
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
    size: Size::new(1600.0, 900.0),
    min_size: Some(Size::new(1280.0, 720.0)),
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
}
