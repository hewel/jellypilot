//! Cross-platform iced application shell for JellyPilot.

mod app;

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
  let mut application = iced::application(move || app::boot(smoke), app::update, app::view)
    .title("JellyPilot")
    .subscription(app::subscription)
    .theme(app::theme)
    .window(window::Settings {
      size: Size::new(1600.0, 900.0),
      min_size: Some(Size::new(1280.0, 720.0)),
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
