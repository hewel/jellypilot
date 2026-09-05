pub mod accounts;
pub mod artwork;
pub mod browse;
pub mod detail;
pub mod home;
pub mod kernel;
pub mod login;
pub mod message;
pub mod personal_lists;
pub mod playback;
pub mod settings;
pub mod shell;
pub mod state;
mod subscriptions;
mod update;
mod view;

use iced::{Subscription, Task, Theme};
use jellypilot_core::config::AppMode;

pub use message::Message;
pub use state::State;

pub fn boot(
  smoke: bool,
  tray: Option<crate::tray::Tray>,
  instance: Option<crate::instance::Guard>,
) -> (State, Task<Message>) {
  let mut state = State::boot(smoke);
  state.instance = instance;
  state.kernel.tray = tray;
  if let Some(tray) = &state.kernel.tray {
    tray.sync(&state.playback.view, false);
  }

  let start_hidden = crate::should_start_hidden(
    state.kernel.settings.snapshot().start_minimized(),
    state.kernel.tray.is_some(),
    smoke,
  );
  let mut tasks = vec![
    login::load_saved_profiles(&state.login, &state.kernel).map(Message::Login),
    iced::system::theme().map(Message::SystemThemeDiscovered),
  ];
  if !start_hidden {
    let mut geometry = shell::mode_geometry(
      if smoke {
        AppMode::Full
      } else {
        state.app_mode()
      },
      None,
    );
    if smoke {
      geometry.size = crate::smoke_window_size();
    }
    state.shell.window_size = geometry.size;
    let (_id, open) = iced::window::open(window_settings(geometry));
    tasks.push(open.map(|id| Message::Window(message::WindowMessage::ShowRequested(Some(id)))));
  }
  (state, Task::batch(tasks))
}

fn window_settings(geometry: shell::ModeGeometry) -> iced::window::Settings {
  iced::window::Settings {
    size: geometry.size,
    min_size: geometry.min_size,
    max_size: geometry.max_size,
    resizable: geometry.resizable,
    icon: crate::window_icon(),
    // The close request is handled by the shell so Full mode can preserve its
    // hide-to-tray behavior and Control-Only can destroy the window.
    exit_on_close_request: false,
    ..iced::window::Settings::default()
  }
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  update::update(state, message)
}

pub fn view(state: &State, _window_id: iced::window::Id) -> iced::Element<'_, Message> {
  jellypilot_ui::widgets::focus_scope::focus_scope(
    view::view(state),
    state.shell.focus_visibility.clone(),
  )
}

pub fn subscription(state: &State) -> Subscription<Message> {
  subscriptions::subscription(state)
}

pub fn theme(state: &State, _window_id: iced::window::Id) -> Theme {
  jellypilot_ui::theme::theme(state.theme_mode())
}
