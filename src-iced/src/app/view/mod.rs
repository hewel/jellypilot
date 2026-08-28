mod home;
mod login;

use iced::Element;
use jellypilot_auth::login::ConnectionPhase;

use super::message::Message;
use super::state::State;

pub fn view(state: &State) -> Element<'_, Message> {
  if state.connection == ConnectionPhase::Connected {
    home::view(state)
  } else {
    login::view(state)
  }
}
