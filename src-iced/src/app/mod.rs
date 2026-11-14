pub mod accounts;
pub mod artwork;
pub(crate) mod avatars;
pub mod browse;
pub(crate) mod collections;
pub mod detail;
pub mod embedded_player;
pub mod home;
pub(crate) mod item_actions;
pub mod kernel;
pub(crate) mod list_playback;
pub mod login;
pub mod message;
pub(crate) mod motion;
pub mod personal_lists;
pub mod pip;
pub mod playback;
pub(crate) mod saved_browse;
pub mod settings;
pub mod shell;
pub mod state;
mod subscriptions;
#[cfg(target_os = "linux")]
pub(crate) mod system_media;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod tv;
pub(crate) mod undo;
mod update;
mod view;

use iced::widget::Widget as _;
use iced::{Subscription, Task, Theme};
use jellypilot_core::config::{AppMode, UiMode};
use jellypilot_core::startup_capture::emit_startup_stage;
use jellypilot_core::window_geometry::{WindowGeometry, MONITOR_EDGE_FRACTION};

pub use message::Message;
pub use state::State;

pub fn boot(
  smoke: bool,
  tv: bool,
  instance: Option<crate::instance::Guard>,
) -> (State, Task<Message>) {
  emit_startup_stage("app-boot-enter");
  let mut state = State::boot(smoke);
  if tv {
    state.shell.ui_mode = UiMode::Tv;
    state.full.get_or_insert_with(state::FullUi::default);
    state.shell.destination = state::Destination::Home;
  }
  state.instance = instance;
  if !smoke || crate::regression::active() {
    emit_startup_stage("tray-init-enter");
  } else {
    emit_startup_stage("tray-skipped");
  }
  state.kernel.tray = if crate::regression::active() {
    match crate::tray::Tray::new(state.kernel.locale) {
      Ok(tray) => {
        crate::regression::check("real tray initialized before embedded Host creation");
        Some(tray)
      }
      Err(error) => {
        emit_startup_stage("tray-unavailable");
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
    emit_startup_stage("tray-ready");
    tray.sync(&state.playback.view, false, state.kernel.locale);
  }
  if state.kernel.tray.is_none() && (!smoke || crate::regression::active()) {
    emit_startup_stage("tray-unavailable");
  }

  let start_hidden = !state.tv_mode()
    && crate::should_start_hidden(
      state.kernel.settings.snapshot().start_minimized(),
      state.kernel.tray.is_some(),
      smoke,
    );
  if start_hidden {
    emit_startup_stage("window-start-hidden");
    state.shell.images_visible = false;
    playback::suspend_artwork(&mut state.playback);
  }
  let mut tasks = vec![
    login::load_saved_profiles(&state.login.flow, &state.kernel).map(Message::Login),
    iced::system::theme().map(Message::SystemThemeDiscovered),
  ];
  if !start_hidden {
    let mut geometry = WindowGeometry::for_mode(
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
        (480.0, 320.0)
      } else {
        let size = crate::smoke_window_size();
        (size.width, size.height)
      };
    }
    state.shell.window_size = geometry.size.into();
    emit_startup_stage("window-open-requested");
    let (id, open) = open_window(geometry);
    state.shell.pending_window_id = Some(id);
    tasks.push(open.map(|id| {
      emit_startup_stage("window-opened");
      Message::Window(message::WindowMessage::ShowRequested(Some(id)))
    }));
    tasks.push(shell::open_timeout(id));
  }
  emit_startup_stage("app-boot-exit");
  (state, Task::batch(tasks))
}

fn open_window(geometry: WindowGeometry) -> (iced::window::Id, Task<iced::window::Id>) {
  let probe = crate::regression::active();
  // Select the native monitor with a small, initially hidden window. Apply its
  // logical-pixel constraints before ShowRequested makes the main window visible.
  let (id, open) = iced::window::open(iced::window::Settings {
    size: if probe {
      geometry.size.into()
    } else {
      iced::Size::new(480.0, 320.0)
    },
    min_size: probe.then_some(geometry.size.into()),
    max_size: probe.then_some(geometry.size.into()),
    resizable: !probe && geometry.resizable,
    visible: probe,
    position: if probe {
      iced::window::Position::Default
    } else {
      iced::window::Position::SpecificWith(|_, monitor| {
        iced::Point::new(
          monitor.width * MONITOR_EDGE_FRACTION,
          monitor.height * MONITOR_EDGE_FRACTION,
        )
      })
    },
    icon: crate::window_icon(),
    // Both App Modes close the window while the tray/runtime remain available.
    exit_on_close_request: false,
    ..iced::window::Settings::default()
  });
  let open = if probe {
    open
  } else {
    open.then(move |id| shell::window_geometry_task(Some(id), geometry).chain(Task::done(id)))
  };
  (id, open)
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
  #[cfg(target_os = "linux")]
  let sample_controls = system_media::should_sample(&message);
  let task = update::update(state, message);
  #[cfg(target_os = "linux")]
  let task = Task::batch([task, system_media::sync(state, sample_controls)]);
  let after = state.theme_mode();
  let enabled = motion_enabled(state) && !discovered;
  state.motion.sync(after, enabled, now);
  let modal_is_open = accounts::blocking_modal(&state.accounts);
  if !enabled || (!modal_was_open && modal_is_open) {
    state.motion.retained_modal = None;
  } else if modal_was_open && !modal_is_open {
    state.motion.retained_modal = retained;
  }
  if state.playback.view.lifecycle.playback_active {
    // Discard popup state, including its exit snapshot, so playback notices
    // cannot interrupt viewing or reappear when the session ends.
    state.kernel.active_toast = None;
    state.motion.toast = None;
  } else if let Some(toast) = &state.kernel.active_toast {
    if state.motion.toast.as_ref().map(|last| last.id) != Some(toast.id) {
      state.motion.toast = Some(toast.clone());
    }
  }
  task
}

pub fn view(state: &State, window_id: iced::window::Id) -> iced::Element<'_, Message> {
  if pip::window_id(state) == Some(window_id) {
    let generation = pip::generation(state);
    return view::pip::view(state)
      .map(move |message| pip::bind(window_id, generation, message))
      .boxed();
  }
  if let Some(size) = crate::regression::gpu_size_for(window_id) {
    return iced::widget::container(crate::embedded::view())
      .width(size.width)
      .height(size.height)
      .boxed();
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
  iced::widget::container(jellypilot_ui::widgets::motion::scope(
    jellypilot_ui::widgets::focus_scope::focus_scope(
      view::image_observer::observe_images(content),
      state.shell.focus_visibility.clone(),
    ),
    enabled,
  ))
  .id("main-root")
  .width(iced::Fill)
  .height(iced::Fill)
  .boxed()
}

pub fn subscription(state: &State) -> Subscription<Message> {
  let base = subscriptions::subscription(state);
  #[cfg(target_os = "linux")]
  let base = if !state.shell.smoke && !state.shell.quit_requested {
    Subscription::batch([base, system_media::subscription(&state.system_media)])
  } else {
    base
  };
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
