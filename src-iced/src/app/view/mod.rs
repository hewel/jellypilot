mod account;
pub(crate) mod browse;
mod detail;
pub(crate) mod home;
mod login;
mod personal_lists;
mod player;
mod settings;
pub(crate) mod shell;

use iced::Element;
use jellypilot_auth::login::ConnectionPhase;

use super::message::Message;
use super::state::State;

pub fn view(state: &State) -> Element<'_, Message> {
  let base = if state.kernel.connection == ConnectionPhase::Connected {
    shell::view(state)
  } else {
    login::view(state)
  };
  // Candidate login and confirmation are the active interaction layer. The
  // underlying screen stays in state, but is deliberately absent from the
  // widget tree so Tab cannot reach background controls.
  if let Some(modal) = account::modal_layer(state) {
    modal
  } else {
    base
  }
}
