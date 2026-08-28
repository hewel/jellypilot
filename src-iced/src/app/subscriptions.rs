use iced::{event, window, Event, Subscription};

use super::message::{Message, WindowMessage};
use super::state::State;

pub fn subscription(state: &State) -> Subscription<Message> {
  let window_events = event::listen_with(|event, status, window_id| {
    if status == event::Status::Captured {
      return None;
    }
    match event {
      Event::Window(window::Event::CloseRequested) => {
        Some(Message::Window(WindowMessage::CloseRequested(window_id)))
      }
      _ => None,
    }
  });

  if state.smoke {
    Subscription::batch([
      window_events,
      window::frames().map(|_| Message::Window(WindowMessage::FrameRendered)),
    ])
  } else {
    window_events
  }
}
