pub(crate) mod browse;
mod detail;
pub(crate) mod home;
mod login;
mod player;
mod settings;
pub(crate) mod shell;

use iced::Element;
use jellypilot_auth::login::ConnectionPhase;

use super::message::Message;
use super::state::State;

pub fn view(state: &State) -> Element<'_, Message> {
  if state.connection == ConnectionPhase::Connected {
    shell::view(state)
  } else {
    login::view(state)
  }
}
