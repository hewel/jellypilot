use std::time::Duration;

use iced::{event, keyboard, time, window, Event, Subscription};

use super::message::{Message, PlaybackMessage, WindowMessage};
use super::state::State;
use jellypilot_mpv::playback_session::{AdjacentDirection, PlaybackIntent};

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
  let mut subscriptions = vec![window_events];
  if state.playback_view.now_playing.is_some() {
    subscriptions.push(
      time::every(Duration::from_secs(1))
        .map(|_| Message::Playback(PlaybackMessage::Intent(PlaybackIntent::Tick))),
    );
    subscriptions.push(
      event::listen()
        .with((
          state.settings.snapshot().key_next_episode().to_owned(),
          state.settings.snapshot().key_previous_episode().to_owned(),
        ))
        .filter_map(playback_shortcut),
    );
  }
  if state.tray.is_some() {
    subscriptions.push(time::every(Duration::from_millis(100)).map(|_| Message::TrayPoll));
  }

  if state.smoke {
    subscriptions.push(window::frames().map(|_| Message::Window(WindowMessage::FrameRendered)));
  }
  Subscription::batch(subscriptions)
}

fn playback_shortcut(((next, previous), event): ((String, String), Event)) -> Option<Message> {
  let Event::Keyboard(keyboard::Event::KeyPressed {
    modified_key,
    modifiers,
    repeat: false,
    ..
  }) = event
  else {
    return None;
  };
  let direction = if shortcut_matches(&next, &modified_key, modifiers) {
    AdjacentDirection::Next
  } else if shortcut_matches(&previous, &modified_key, modifiers) {
    AdjacentDirection::Previous
  } else {
    return None;
  };
  Some(Message::Playback(PlaybackMessage::Intent(
    PlaybackIntent::PlayAdjacent(direction),
  )))
}

fn shortcut_matches(binding: &str, key: &keyboard::Key, modifiers: keyboard::Modifiers) -> bool {
  let mut parts = binding
    .split('+')
    .map(str::trim)
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>();
  let Some(expected_key) = parts.pop() else {
    return false;
  };
  let mut expected_modifiers = keyboard::Modifiers::NONE;
  for modifier in parts {
    if modifier.eq_ignore_ascii_case("shift") {
      expected_modifiers.insert(keyboard::Modifiers::SHIFT);
    } else if modifier.eq_ignore_ascii_case("ctrl") || modifier.eq_ignore_ascii_case("control") {
      expected_modifiers.insert(keyboard::Modifiers::CTRL);
    } else if modifier.eq_ignore_ascii_case("alt") {
      expected_modifiers.insert(keyboard::Modifiers::ALT);
    } else if modifier.eq_ignore_ascii_case("super")
      || modifier.eq_ignore_ascii_case("meta")
      || modifier.eq_ignore_ascii_case("command")
    {
      expected_modifiers.insert(keyboard::Modifiers::LOGO);
    } else {
      return false;
    }
  }
  if modifiers != expected_modifiers {
    return false;
  }
  match key.as_ref() {
    keyboard::Key::Character(value) => value.eq_ignore_ascii_case(expected_key),
    keyboard::Key::Named(value) => format!("{value:?}").eq_ignore_ascii_case(expected_key),
    keyboard::Key::Unidentified => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_episode_shortcuts_match_modified_character_keys() {
    assert!(shortcut_matches(
      "Shift+>",
      &keyboard::Key::Character(">".into()),
      keyboard::Modifiers::SHIFT,
    ));
    assert!(shortcut_matches(
      "Shift+<",
      &keyboard::Key::Character("<".into()),
      keyboard::Modifiers::SHIFT,
    ));
  }

  #[test]
  fn shortcut_matching_rejects_missing_and_extra_modifiers() {
    let key = keyboard::Key::Character(">".into());
    assert!(!shortcut_matches(
      "Shift+>",
      &key,
      keyboard::Modifiers::NONE,
    ));
    assert!(!shortcut_matches(
      "Shift+>",
      &key,
      keyboard::Modifiers::SHIFT | keyboard::Modifiers::CTRL,
    ));
  }
}
