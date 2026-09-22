use iced::{event, keyboard, Event};
use jellypilot_core::tv_navigation::Input;

use super::{AppMessage, Message};

pub fn keyboard(event: Event, status: event::Status) -> Option<AppMessage> {
  if matches!(event, Event::Window(iced::window::Event::Unfocused)) {
    return Some(AppMessage::Tv(Message::CancelPress));
  }
  if matches!(
    event,
    Event::Keyboard(keyboard::Event::KeyReleased {
      key: keyboard::Key::Named(keyboard::key::Named::Enter),
      ..
    })
  ) {
    return Some(AppMessage::Tv(Message::ConfirmReleased));
  }
  if status == event::Status::Captured {
    return None;
  }
  let Event::Keyboard(keyboard::Event::KeyPressed {
    key,
    modifiers,
    repeat,
    ..
  }) = event
  else {
    return None;
  };
  if modifiers.command() || modifiers.alt() {
    return None;
  }
  if !repeat
    && (matches!(
      key.as_ref(),
      keyboard::Key::Named(keyboard::key::Named::ContextMenu)
    ) || (modifiers.shift()
      && matches!(
        key.as_ref(),
        keyboard::Key::Named(keyboard::key::Named::F10)
      )))
  {
    return Some(AppMessage::Tv(Message::OpenMenu));
  }
  if matches!(
    key.as_ref(),
    keyboard::Key::Named(keyboard::key::Named::Enter)
  ) {
    return (!repeat).then_some(AppMessage::Tv(Message::ConfirmPressed));
  }
  let input = match key.as_ref() {
    keyboard::Key::Named(keyboard::key::Named::ArrowUp) => Input::Up,
    keyboard::Key::Named(keyboard::key::Named::ArrowDown) => Input::Down,
    keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => Input::Left,
    keyboard::Key::Named(keyboard::key::Named::ArrowRight) => Input::Right,
    keyboard::Key::Named(
      keyboard::key::Named::Escape
      | keyboard::key::Named::Backspace
      | keyboard::key::Named::BrowserBack,
    ) => Input::Back,
    keyboard::Key::Named(keyboard::key::Named::MediaPlayPause | keyboard::key::Named::Space) => {
      Input::PlayPause
    }
    _ => return None,
  };
  if repeat && !matches!(input, Input::Up | Input::Down | Input::Left | Input::Right) {
    return None;
  }
  Some(AppMessage::Tv(Message::Input(input)))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn pressed(named: keyboard::key::Named, repeat: bool) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
      key: keyboard::Key::Named(named),
      modified_key: keyboard::Key::Named(named),
      physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
      location: keyboard::Location::Standard,
      modifiers: keyboard::Modifiers::default(),
      text: None,
      repeat,
    })
  }

  #[test]
  fn held_confirmation_is_one_transaction_but_directional_repeat_keeps_moving() {
    for key in [
      keyboard::key::Named::Enter,
      keyboard::key::Named::Escape,
      keyboard::key::Named::MediaPlayPause,
    ] {
      assert!(keyboard(pressed(key, false), event::Status::Ignored).is_some());
      assert!(keyboard(pressed(key, true), event::Status::Ignored).is_none());
    }
    assert!(matches!(
      keyboard(
        pressed(keyboard::key::Named::ArrowDown, true),
        event::Status::Ignored
      ),
      Some(AppMessage::Tv(Message::Input(Input::Down)))
    ));
    assert!(keyboard(
      pressed(keyboard::key::Named::ArrowDown, false),
      event::Status::Captured
    )
    .is_none());
  }
}
