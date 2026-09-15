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
pub(crate) mod motion;
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
    let (id, open) = iced::window::open(window_settings(geometry));
    state.shell.pending_window_id = Some(id);
    tasks.push(open.map(|id| Message::Window(message::WindowMessage::ShowRequested(Some(id)))));
    tasks.push(shell::open_timeout(id));
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
    // Both App Modes close the window while the tray/runtime remain available.
    exit_on_close_request: false,
    ..iced::window::Settings::default()
  }
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  if let Some(task) = crate::regression::update(state, &message) {
    return task;
  }
  let now = match &message {
    Message::Window(message::WindowMessage::FrameTick(now)) => *now,
    _ => std::time::Instant::now(),
  };
  // The first OS theme report is startup discovery, not a user transition.
  let discovered = matches!(message, Message::SystemThemeDiscovered(_));
  let before = state.theme_mode();
  let enabled = motion_enabled(state) && !discovered;
  state.motion.sync(before, enabled, now);
  let modal_was_open = accounts::blocking_modal(&state.accounts);
  let retained = if enabled && modal_was_open && view::account::may_close_modal(&message) {
    view::account::retained_modal_snapshot(state)
  } else {
    None
  };
  let task = update::update(state, message);
  let after = state.theme_mode();
  let enabled = motion_enabled(state) && !discovered;
  state.motion.sync(after, enabled, now);
  let modal_is_open = accounts::blocking_modal(&state.accounts);
  if !enabled || (!modal_was_open && modal_is_open) {
    state.motion.retained_modal = None;
  } else if modal_was_open && !modal_is_open {
    state.motion.retained_modal = retained;
  }
  if let Some(toast) = &state.kernel.active_toast {
    if state.motion.toast.as_ref().map(|last| last.id) != Some(toast.id) {
      state.motion.toast = Some(toast.clone());
    }
  }
  task
}

pub fn view(state: &State, _window_id: iced::window::Id) -> iced::Element<'_, Message> {
  if let Some(size) = crate::regression::gpu_size() {
    return iced::widget::container(crate::embedded::view())
      .width(size.width)
      .height(size.height)
      .into();
  }
  let enabled = motion_enabled(state);
  let content = jellypilot_ui::widgets::motion::transition(
    view::view(state),
    u64::from(state.app_mode() == AppMode::Full),
    enabled,
    jellypilot_ui::tokens::TOKENS.durations.ms300,
  );
  let content = if state.shell.pending_close.is_some() {
    jellypilot_ui::widgets::inert::inert(content)
  } else {
    content
  };
  jellypilot_ui::widgets::motion::scope(
    jellypilot_ui::widgets::focus_scope::focus_scope(
      view::image_observer::observe_images(content),
      state.shell.focus_visibility.clone(),
    ),
    enabled,
  )
}

pub fn subscription(state: &State) -> Subscription<Message> {
  let base = subscriptions::subscription(state);
  let regression = if crate::regression::active() {
    crate::regression::subscription()
  } else {
    Subscription::none()
  };
  Subscription::batch([base, regression])
}

fn motion_enabled(state: &State) -> bool {
  state.shell.window_id.is_some()
    && state.shell.images_visible
    && !state.kernel.settings.snapshot().reduced_motion()
}

pub fn theme(state: &State, _window_id: iced::window::Id) -> Theme {
  state.native_theme()
}
