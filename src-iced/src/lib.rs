//! ADR 0027 cross-platform iced frontend skeleton for JellyPilot.

use iced::widget::{container, text};
use iced::{window, Element, Fill, Subscription, Task, Theme};

struct State {
  smoke: bool,
}

#[derive(Debug, Clone, Copy)]
enum Message {
  FrameRendered,
}

/// Starts the cross-platform iced application and blocks until its window closes.
pub fn run() -> iced::Result {
  run_application(false)
}

/// Starts the iced application and exits after its first rendered window frame.
pub fn run_smoke() -> iced::Result {
  run_application(true)
}

fn run_application(smoke: bool) -> iced::Result {
  iced::application(move || (State { smoke }, Task::none()), update, view)
    .title("JellyPilot")
    .subscription(subscription)
    .theme(theme)
    .run()
}

fn update(state: &mut State, message: Message) -> Task<Message> {
  match message {
    Message::FrameRendered => {
      state.smoke = false;
      iced::exit()
    }
  }
}

fn subscription(state: &State) -> Subscription<Message> {
  if state.smoke {
    window::frames().map(|_| Message::FrameRendered)
  } else {
    Subscription::none()
  }
}

fn theme(_state: &State) -> Theme {
  jellypilot_ui::theme::theme()
}

fn view(_state: &State) -> Element<'_, Message> {
  container(text("JellyPilot"))
    .width(Fill)
    .height(Fill)
    .into()
}
