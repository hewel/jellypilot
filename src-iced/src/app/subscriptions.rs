use std::time::Duration;

#[cfg(test)]
use iced::futures::StreamExt;
use iced::futures::{SinkExt, Stream};
use iced::{event, keyboard, time, window, Event, Subscription};
use jellypilot_mpv::playback_session::{AdjacentDirection, PlaybackIntent};

use super::message::{Message, PlaybackMessage, RemoteMessage, WindowMessage};
use super::state::{RemoteEventChannel, State};

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
          state.settings.snapshot().key_intro_skip().to_owned(),
        ))
        .filter_map(playback_shortcut),
    );
  }
  if let Some(channel) = state.remote_events.clone() {
    subscriptions.push(Subscription::run_with(channel, remote_event_stream));
  }
  if state.tray.is_some() {
    subscriptions.push(time::every(Duration::from_millis(100)).map(|_| Message::TrayPoll));
  }

  if state.smoke {
    subscriptions.push(window::frames().map(|_| Message::Window(WindowMessage::FrameRendered)));
  }
  Subscription::batch(subscriptions)
}

fn playback_shortcut(
  ((next, previous, intro_skip), event): ((String, String, String), Event),
) -> Option<Message> {
  let Event::Keyboard(keyboard::Event::KeyPressed {
    modified_key,
    modifiers,
    repeat: false,
    ..
  }) = event
  else {
    return None;
  };
  let intent = if shortcut_matches(&next, &modified_key, modifiers) {
    PlaybackIntent::PlayAdjacent(AdjacentDirection::Next)
  } else if shortcut_matches(&previous, &modified_key, modifiers) {
    PlaybackIntent::PlayAdjacent(AdjacentDirection::Previous)
  } else if shortcut_matches(&intro_skip, &modified_key, modifiers) {
    PlaybackIntent::SkipIntro
  } else {
    return None;
  };
  Some(Message::Playback(PlaybackMessage::Intent(intent)))
}

fn remote_event_stream(channel: &RemoteEventChannel) -> impl Stream<Item = Message> {
  let channel = channel.clone();
  iced::stream::channel(32, async move |mut output| loop {
    let Some(event) = channel.receiver.lock().await.recv().await else {
      break;
    };
    if output
      .send(Message::Remote(RemoteMessage::Event {
        remote: channel.remote,
        event,
      }))
      .await
      .is_err()
    {
      break;
    }
  })
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
  fn remote_event_stream_backpressures_without_dropping_bursts() {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .expect("test runtime should build");
    runtime.block_on(async {
      let mut gate = jellypilot_core::request_gate::RequestGate::default();
      let remote = gate.begin_remote();
      let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
      let channel = RemoteEventChannel {
        remote,
        receiver: std::sync::Arc::new(tokio::sync::Mutex::new(receiver)),
      };
      for _ in 0..64 {
        sender
          .send(jellypilot_session::JellyfinWebSocketEvent::Connected)
          .expect("test receiver should remain open");
      }
      drop(sender);

      let messages = remote_event_stream(&channel)
        .take(64)
        .collect::<Vec<_>>()
        .await;

      assert_eq!(messages.len(), 64);
    });
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
