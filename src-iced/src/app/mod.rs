pub mod accounts;
pub mod artwork;
pub(crate) mod avatars;
pub mod browse;
pub(crate) mod collections;
pub mod detail;
pub mod embedded_player;
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

pub fn boot(smoke: bool, instance: Option<crate::instance::Guard>) -> (State, Task<Message>) {
  let mut state = State::boot(smoke);
  state.instance = instance;
  state.kernel.tray = if crate::regression::active() {
    match crate::tray::Tray::new(state.kernel.locale) {
      Ok(tray) => {
        crate::regression::check("real tray initialized before embedded Host creation");
        Some(tray)
      }
      Err(error) => {
        crate::regression::unavailable(format!("Real tray initialization failed: {error}"));
        return (state, iced::exit());
      }
    }
  } else {
    (!smoke)
      .then(|| crate::tray::Tray::new(state.kernel.locale).ok())
      .flatten()
  };
  if let Some(tray) = &state.kernel.tray {
    tray.sync(&state.playback.view, false, state.kernel.locale);
  }

  let start_hidden = crate::should_start_hidden(
    state.kernel.settings.snapshot().start_minimized(),
    state.kernel.tray.is_some(),
    smoke,
  );
  if start_hidden {
    state.shell.images_visible = false;
    playback::suspend_artwork(&mut state.playback);
  }
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
      geometry.size = if crate::regression::active() {
        geometry.min_size = None;
        iced::Size::new(480.0, 320.0)
      } else {
        crate::smoke_window_size()
      };
    }
    state.shell.window_size = geometry.size;
    let (_id, open) = iced::window::open(window_settings(geometry));
    tasks.push(open.map(|id| Message::Window(message::WindowMessage::ShowRequested(Some(id)))));
  }
  (state, Task::batch(tasks))
}

fn window_settings(geometry: shell::ModeGeometry) -> iced::window::Settings {
  // Fixed probe windows use the same floating hint as Control-Only mode;
  // changing compositor settings or accepting a tiled desktop-sized readback is not a probe.
  iced::window::Settings {
    size: geometry.size,
    min_size: if crate::regression::active() {
      Some(geometry.size)
    } else {
      geometry.min_size
    },
    max_size: if crate::regression::active() {
      Some(geometry.size)
    } else {
      geometry.max_size
    },
    resizable: !crate::regression::active() && geometry.resizable,
    icon: crate::window_icon(),
    // The close request is handled by the shell so Full mode can preserve its
    // hide-to-tray behavior and Control-Only can destroy the window.
    exit_on_close_request: false,
    ..iced::window::Settings::default()
  }
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  if let Some(task) = crate::regression::update(state, &message) {
    return task;
  }
  update::update(state, message)
}

pub fn view(state: &State, _window_id: iced::window::Id) -> iced::Element<'_, Message> {
  if let Some(size) = crate::regression::gpu_size() {
    return iced::widget::container(crate::embedded::view())
      .width(size.width)
      .height(size.height)
      .into();
  }
  jellypilot_ui::widgets::focus_scope::focus_scope(
    view::image_observer::observe_images(view::view(state)),
    state.shell.focus_visibility.clone(),
  )
}

pub fn subscription(state: &State) -> Subscription<Message> {
  if crate::regression::active() {
    return Subscription::batch([
      subscriptions::subscription(state),
      crate::regression::subscription(),
    ]);
  }
  subscriptions::subscription(state)
}

pub fn theme(state: &State, _window_id: iced::window::Id) -> Theme {
  jellypilot_ui::theme::theme(state.theme_mode())
}
