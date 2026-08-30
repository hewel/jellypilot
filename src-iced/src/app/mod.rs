pub mod artwork;
pub mod browse;
pub mod detail;
pub mod home;
pub mod kernel;
pub mod login;
pub mod message;
pub mod playback;
pub mod settings;
pub mod shell;
pub mod state;
mod subscriptions;
mod update;
mod view;

use iced::{Subscription, Task, Theme};

pub use message::Message;
pub use state::State;

pub fn boot(smoke: bool, tray: Option<crate::tray::Tray>) -> (State, Task<Message>) {
  let mut state = State::boot(smoke);
  state.kernel.tray = tray;
  if let Some(tray) = &state.kernel.tray {
    tray.sync(&state.playback.view, false);
  }
  let task = login::load_saved_profiles(&state.login, &state.kernel).map(Message::Login);
  let task = Task::batch([
    task,
    iced::window::latest()
      .and_then(iced::window::size)
      .map(|size| Message::Window(message::WindowMessage::Resized(size))),
  ]);
  (state, task)
}

pub fn update(state: &mut State, message: Message) -> Task<Message> {
  update::update(state, message)
}

pub fn view(state: &State) -> iced::Element<'_, Message> {
  view::view(state)
}

pub fn subscription(state: &State) -> Subscription<Message> {
  subscriptions::subscription(state)
}

pub fn theme(_state: &State) -> Theme {
  jellypilot_ui::theme::theme()
}
