use std::time::Duration;

#[cfg(test)]
use iced::futures::StreamExt;
use iced::futures::{SinkExt, Stream};
use iced::{event, keyboard, time, window, Event, Subscription};
use jellypilot_core::config::ThemeMode;
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
  subscriptions
    .push(window::resize_events().map(|(_id, size)| Message::Window(WindowMessage::Resized(size))));
  if state.playback.view.now_playing.is_some() {
    subscriptions.push(
      time::every(Duration::from_secs(1))
        .map(|_| Message::Playback(PlaybackMessage::Intent(PlaybackIntent::Tick))),
    );
  }
  if state.settings.view.shortcut_capture.is_some() {
    subscriptions.push(event::listen_with(shortcut_capture));
  } else if state.shell.settings_open {
    subscriptions.push(event::listen_with(settings_modal_events));
  } else if state.playback.view.now_playing.is_some() {
    subscriptions.push(
      event::listen()
        .with(playback_shortcuts(state.kernel.settings.snapshot()))
        .filter_map(playback_shortcut),
    );
  }
  if let Some(channel) = state.playback.remote_events.clone() {
    subscriptions.push(Subscription::run_with(channel, remote_event_stream));
  }
  if let Some(tray) = &state.kernel.tray {
    subscriptions.push(Subscription::run_with(tray.channel(), tray_event_stream));
  }
  // Follow OS light/dark flips only while the theme mode setting is System;
  // an explicit Dark/Light setting makes the subscription dead weight.
  if state.kernel.settings.snapshot().theme_mode() == ThemeMode::System {
    subscriptions.push(iced::system::theme_changes().map(Message::SystemThemeChanged));
  }

  // Drive the shimmer phase only while skeletons are actually on screen (or a
  // smoke run waits on its first frame); an always-on frames subscription
  // would redraw the shell at display refresh for no visible change.
  if state.shell.smoke
    || (state.skeletons_active() && !state.kernel.settings.snapshot().reduced_motion())
  {
    subscriptions
      .push(window::frames().map(|instant| Message::Window(WindowMessage::FrameTick(instant))));
  }
  Subscription::batch(subscriptions)
}

fn playback_shortcuts(settings: &jellypilot_core::config::Settings) -> (String, String, String) {
  (
    settings.key_next_episode().to_owned(),
    settings.key_previous_episode().to_owned(),
    settings.key_intro_skip().to_owned(),
  )
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

fn shortcut_capture(
  event: Event,
  _status: event::Status,
  _window_id: window::Id,
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
  let key = match modified_key.as_ref() {
    keyboard::Key::Named(keyboard::key::Named::Escape) => {
      return Some(Message::Settings(
        super::message::SettingsMessage::CancelShortcutCapture,
      ));
    }
    keyboard::Key::Character("+") => "Plus".to_owned(),
    keyboard::Key::Character(value) => value.to_owned(),
    keyboard::Key::Named(
      keyboard::key::Named::Alt
      | keyboard::key::Named::AltGraph
      | keyboard::key::Named::Control
      | keyboard::key::Named::Shift
      | keyboard::key::Named::Meta
      | keyboard::key::Named::Super,
    )
    | keyboard::Key::Unidentified => return None,
    keyboard::Key::Named(value) => format!("{value:?}"),
  };
  let mut binding = String::new();
  if modifiers.control() {
    binding.push_str("Ctrl+");
  }
  if modifiers.alt() {
    binding.push_str("Alt+");
  }
  if modifiers.logo() {
    binding.push_str("Super+");
  }
  if modifiers.shift() {
    binding.push_str("Shift+");
  }
  binding.push_str(&key);
  Some(Message::Settings(
    super::message::SettingsMessage::ShortcutCaptured(binding),
  ))
}

fn settings_modal_events(
  event: Event,
  status: event::Status,
  _window_id: window::Id,
) -> Option<Message> {
  if status == event::Status::Captured {
    return None;
  }
  let Event::Keyboard(keyboard::Event::KeyPressed {
    modified_key,
    repeat: false,
    ..
  }) = event
  else {
    return None;
  };
  if let keyboard::Key::Named(keyboard::key::Named::Escape) = modified_key.as_ref() {
    Some(Message::Settings(super::message::SettingsMessage::Close))
  } else {
    None
  }
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

fn tray_event_stream(channel: &crate::tray::TrayEventChannel) -> impl Stream<Item = Message> {
  let channel = channel.clone();
  iced::stream::channel(1, async move |mut output| loop {
    let Some(action) = channel.receiver.lock().await.recv().await else {
      break;
    };
    if output.send(Message::Tray(action)).await.is_err() {
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
    keyboard::Key::Character(value) => {
      value.eq_ignore_ascii_case(expected_key)
        || (value == "+" && expected_key.eq_ignore_ascii_case("Plus"))
    }
    keyboard::Key::Named(value) => format!("{value:?}").eq_ignore_ascii_case(expected_key),
    keyboard::Key::Unidentified => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use jellypilot_core::LoadState;

  fn key_pressed(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
      modified_key: key.clone(),
      key,
      physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyA),
      location: keyboard::Location::Standard,
      modifiers,
      text: None,
      repeat: false,
    })
  }

  #[test]
  fn shortcut_capture_keeps_playback_tick_subscribed() {
    let mut state = State::boot(false);
    state.playback.view.now_playing = Some(jellypilot_mpv::playback_session::NowPlayingView {
      item: jellypilot_mpv::playback::NowPlayingItem {
        item_id: "episode-1".to_owned(),
        title: "Pilot".to_owned(),
        item_type: "Episode".to_owned(),
        runtime_seconds: Some(1_800.0),
        start_position_seconds: 0.0,
        play_method: "DirectPlay".to_owned(),
      },
      paused: false,
      position_seconds: 10.0,
      duration_seconds: Some(1_800.0),
      volume: 75.0,
      muted: false,
    });
    state.settings.view.shortcut_capture = Some(jellypilot_core::config::ShortcutKind::Next);

    // window events, resize events, playback tick, shortcut capture, theme changes
    assert_eq!(subscription(&state).units(), 5);
  }
  #[test]
  fn frames_subscription_only_runs_for_smoke_or_active_skeletons() {
    let mut state = State::boot(false);
    // window events, resize events, theme changes; no frames while nothing loads.
    assert_eq!(subscription(&state).units(), 3);

    state.shell.smoke = true;
    assert_eq!(subscription(&state).units(), 4);
    state.shell.smoke = false;

    state.home.data.begin_load();
    assert_eq!(subscription(&state).units(), 4);
    // Episode/neighbor loads render shimmer skeletons independently of the
    // main detail content state; the frames subscription must stay alive.
    let mut detail_state = State::boot(false);
    detail_state.detail.data.season_episodes = LoadState::Loading;
    assert!(detail_state.skeletons_active());
    assert_eq!(subscription(&detail_state).units(), 4);
    detail_state.detail.data.season_episodes = LoadState::Idle;
    detail_state.detail.data.season_neighbors = LoadState::Loading;
    assert_eq!(subscription(&detail_state).units(), 4);

    // Reduced motion renders static skeletons, so no frame ticks are needed.
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-frames-{}.json",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    state.kernel.settings = jellypilot_core::config::SettingsStore::for_test(path.clone());
    state.kernel.settings.set_reduced_motion(true).unwrap();
    assert_eq!(subscription(&state).units(), 3);
    std::fs::remove_file(path).unwrap();
  }

  #[test]
  fn theme_changes_subscription_only_runs_in_system_mode() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-theme-subscription-{}.json",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut state = State::boot(false);
    state.kernel.settings = jellypilot_core::config::SettingsStore::for_test(path.clone());

    let system_units = subscription(&state).units();
    state
      .kernel
      .settings
      .set_theme_mode(ThemeMode::Dark)
      .unwrap();
    assert_eq!(subscription(&state).units(), system_units - 1);
    state
      .kernel
      .settings
      .set_theme_mode(ThemeMode::Light)
      .unwrap();
    assert_eq!(subscription(&state).units(), system_units - 1);
    state
      .kernel
      .settings
      .set_theme_mode(ThemeMode::System)
      .unwrap();
    assert_eq!(subscription(&state).units(), system_units);
    std::fs::remove_file(path).unwrap();
  }

  #[test]
  fn captured_widget_key_reaches_shortcut_capture() {
    let message = shortcut_capture(
      key_pressed(
        keyboard::Key::Character("k".into()),
        keyboard::Modifiers::CTRL,
      ),
      event::Status::Captured,
      window::Id::unique(),
    );

    assert!(matches!(
      message,
      Some(Message::Settings(
        super::super::message::SettingsMessage::ShortcutCaptured(binding)
      )) if binding == "Ctrl+k"
    ));
  }

  #[test]
  fn escape_cancels_shortcut_capture_without_persisting_a_binding() {
    let message = shortcut_capture(
      key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Escape),
        keyboard::Modifiers::NONE,
      ),
      event::Status::Captured,
      window::Id::unique(),
    );

    assert!(matches!(
      message,
      Some(Message::Settings(
        super::super::message::SettingsMessage::CancelShortcutCapture
      ))
    ));
  }

  #[test]
  fn escape_closes_settings_modal_when_capture_inactive() {
    let message = settings_modal_events(
      key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Escape),
        keyboard::Modifiers::NONE,
      ),
      event::Status::Ignored,
      window::Id::unique(),
    );

    assert!(matches!(
      message,
      Some(Message::Settings(
        super::super::message::SettingsMessage::Close
      ))
    ));
  }

  #[test]
  fn non_escape_key_does_not_close_settings_modal() {
    let message = settings_modal_events(
      key_pressed(
        keyboard::Key::Character("x".into()),
        keyboard::Modifiers::NONE,
      ),
      event::Status::Ignored,
      window::Id::unique(),
    );

    assert!(message.is_none());
  }

  #[test]
  fn captured_escape_does_not_close_settings_modal() {
    let message = settings_modal_events(
      key_pressed(
        keyboard::Key::Named(keyboard::key::Named::Escape),
        keyboard::Modifiers::NONE,
      ),
      event::Status::Captured,
      window::Id::unique(),
    );

    assert!(message.is_none());
  }

  #[test]
  fn captured_plus_key_round_trips_through_persistence_and_matching() {
    let Some(Message::Settings(super::super::message::SettingsMessage::ShortcutCaptured(binding))) =
      shortcut_capture(
        key_pressed(
          keyboard::Key::Character("+".into()),
          keyboard::Modifiers::SHIFT,
        ),
        event::Status::Captured,
        window::Id::unique(),
      )
    else {
      panic!("plus key should produce a captured shortcut");
    };
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-plus-shortcut-{}.json",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut settings = jellypilot_core::config::SettingsStore::for_test(path.clone());

    settings
      .set_shortcut(jellypilot_core::config::ShortcutKind::Next, binding)
      .unwrap();
    let persisted = playback_shortcuts(settings.snapshot()).0;

    assert_eq!(persisted, "Shift+Plus");
    assert!(shortcut_matches(
      &persisted,
      &keyboard::Key::Character("+".into()),
      keyboard::Modifiers::SHIFT,
    ));
    std::fs::remove_file(path).unwrap();
  }

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
  fn tray_event_stream_stays_silent_when_no_actions_are_pending() {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .expect("test runtime should build");
    runtime.block_on(async {
      let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
      let channel = crate::tray::TrayEventChannel {
        receiver: std::sync::Arc::new(tokio::sync::Mutex::new(rx)),
      };
      let mut stream = std::pin::pin!(tray_event_stream(&channel));
      let polled = tokio::time::timeout(Duration::from_millis(150), stream.next()).await;
      assert!(
        polled.is_err(),
        "tray stream should produce no messages when no tray actions occur"
      );
    });
  }

  #[test]
  fn tray_event_stream_yields_on_action() {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_time()
      .build()
      .expect("test runtime should build");
    runtime.block_on(async {
      let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
      let channel = crate::tray::TrayEventChannel {
        receiver: std::sync::Arc::new(tokio::sync::Mutex::new(rx)),
      };
      let mut stream = std::pin::pin!(tray_event_stream(&channel));
      tx.send(crate::tray::TrayAction::PlayPause).unwrap();
      let polled = tokio::time::timeout(Duration::from_millis(150), stream.next()).await;
      assert!(matches!(
        polled,
        Ok(Some(Message::Tray(crate::tray::TrayAction::PlayPause)))
      ));
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
  #[test]
  fn playback_shortcuts_reread_persisted_bindings_after_mutation() {
    let path = std::env::temp_dir().join(format!(
      "jellypilot-iced-shortcuts-{}.json",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut settings = jellypilot_core::config::SettingsStore::for_test(path.clone());

    settings
      .set_shortcut(
        jellypilot_core::config::ShortcutKind::Next,
        "Ctrl+n".to_owned(),
      )
      .unwrap();

    assert_eq!(playback_shortcuts(settings.snapshot()).0, "Ctrl+n");
    std::fs::remove_file(path).unwrap();
  }
}
