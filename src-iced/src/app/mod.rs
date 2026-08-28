pub mod message;
pub mod state;
mod subscriptions;
mod update;
mod view;

use iced::{Subscription, Task, Theme};

pub use message::Message;
pub use state::State;

pub fn boot(smoke: bool, tray: Option<crate::tray::Tray>) -> (State, Task<Message>) {
  let mut state = State::boot(smoke);
  state.tray = tray;
  if let Some(tray) = &state.tray {
    tray.sync(&state.playback_view, false);
  }
  let task = update::load_saved_profiles(&state).map(Message::Login);
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
