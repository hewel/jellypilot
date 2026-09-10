use std::sync::Arc;

use crate::i18n::Localizer;
use jellypilot_mpv::playback_session::{AdjacentAvailability, SessionView};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

const PLAY_PAUSE_ID: &str = "play-pause";
const NEXT_ID: &str = "next";
const PREVIOUS_ID: &str = "previous";
const MUTE_ID: &str = "mute";
const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayAction {
  PlayPause,
  Next,
  Previous,
  Mute,
  Show,
  Quit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrayMenuState {
  pub play_pause_label: String,
  pub play_pause_enabled: bool,
  pub next_label: String,
  pub previous_label: String,
  pub show_label: String,
  pub quit_label: String,
  pub next_enabled: bool,
  pub previous_enabled: bool,
  pub mute_label: String,
  pub mute_enabled: bool,
  pub show_enabled: bool,
  pub quit_enabled: bool,
}

enum TrayCommand {
  Sync(TrayMenuState),
  Shutdown,
}

#[derive(Clone, Debug)]
pub struct TrayEventChannel {
  pub receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<TrayAction>>>,
}

impl std::hash::Hash for TrayEventChannel {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    Arc::as_ptr(&self.receiver).hash(state);
  }
}

impl PartialEq for TrayEventChannel {
  fn eq(&self, other: &Self) -> bool {
    Arc::ptr_eq(&self.receiver, &other.receiver)
  }
}

impl Eq for TrayEventChannel {}

pub struct Tray {
  cmd_tx: tokio::sync::mpsc::UnboundedSender<TrayCommand>,
  action_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<TrayAction>>>,
  #[cfg(target_os = "linux")]
  glib_context: gtk::glib::MainContext,
  thread: Option<std::thread::JoinHandle<()>>,
}

impl Tray {
  pub fn new(locale: Localizer) -> Result<Self, String> {
    let (init_tx, init_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<TrayCommand>();
    let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<TrayAction>();

    #[cfg(target_os = "linux")]
    let glib_context = gtk::glib::MainContext::default();

    let thread = std::thread::Builder::new()
      .name("tray".to_owned())
      .spawn(move || {
        #[cfg(target_os = "linux")]
        {
          // GTK's default setlocale(LC_ALL, "") changes process-wide numeric
          // parsing and makes libmpv reject startup. UI language is owned by
          // Localizer; keep Rust's initial C locale instead of mutating it here.
          gtk::disable_setlocale();
          if let Err(error) = gtk::init() {
            let _ = init_tx.send(Err(error.to_string()));
            return;
          }
        }

        let play_pause = MenuItem::with_id(PLAY_PAUSE_ID, locale.text("common-play"), false, None);
        let next = MenuItem::with_id(NEXT_ID, locale.text("common-next"), false, None);
        let previous = MenuItem::with_id(PREVIOUS_ID, locale.text("common-previous"), false, None);
        let mute = MenuItem::with_id(MUTE_ID, locale.text("player-mute"), false, None);
        let show = MenuItem::with_id(SHOW_ID, locale.text("tray-show"), true, None);
        let quit = MenuItem::with_id(QUIT_ID, locale.text("tray-quit"), true, None);
        let separator = PredefinedMenuItem::separator();
        let menu = match Menu::with_items(&[
          &play_pause,
          &next,
          &previous,
          &mute,
          &separator,
          &show,
          &quit,
        ]) {
          Ok(menu) => menu,
          Err(error) => {
            let _ = init_tx.send(Err(error.to_string()));
            return;
          }
        };

        let icon = match tray_icon_image() {
          Ok(icon) => icon,
          Err(error) => {
            let _ = init_tx.send(Err(error));
            return;
          }
        };

        let _icon = match TrayIconBuilder::new()
          .with_tooltip("JellyPilot")
          .with_icon(icon)
          .with_menu(Box::new(menu))
          .with_menu_on_left_click(false)
          .build()
        {
          Ok(icon) => icon,
          Err(error) => {
            let _ = init_tx.send(Err(error.to_string()));
            return;
          }
        };

        if init_tx.send(Ok(())).is_err() {
          return;
        }

        loop {
          while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
              TrayCommand::Sync(menu_state) => {
                play_pause.set_text(menu_state.play_pause_label);
                play_pause.set_enabled(menu_state.play_pause_enabled);
                next.set_text(menu_state.next_label);
                previous.set_text(menu_state.previous_label);
                show.set_text(menu_state.show_label);
                quit.set_text(menu_state.quit_label);
                next.set_enabled(menu_state.next_enabled);
                previous.set_enabled(menu_state.previous_enabled);
                mute.set_text(menu_state.mute_label);
                mute.set_enabled(menu_state.mute_enabled);
                show.set_enabled(menu_state.show_enabled);
                quit.set_enabled(menu_state.quit_enabled);
              }
              TrayCommand::Shutdown => return,
            }
          }

          #[cfg(target_os = "linux")]
          {
            let context = gtk::glib::MainContext::default();
            while context.pending() {
              context.iteration(false);
            }
          }

          while let Ok(event) = MenuEvent::receiver().try_recv() {
            let action = match event.id.as_ref() {
              PLAY_PAUSE_ID => Some(TrayAction::PlayPause),
              NEXT_ID => Some(TrayAction::Next),
              PREVIOUS_ID => Some(TrayAction::Previous),
              MUTE_ID => Some(TrayAction::Mute),
              SHOW_ID => Some(TrayAction::Show),
              QUIT_ID => Some(TrayAction::Quit),
              _ => None,
            };
            if let Some(action) = action {
              if action_tx.send(action).is_err() {
                return;
              }
            }
          }
          while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            let is_click = matches!(
              event,
              TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
              }
            );
            if is_click && action_tx.send(TrayAction::Show).is_err() {
              return;
            }
          }
          #[cfg(target_os = "linux")]
          {
            let context = gtk::glib::MainContext::default();
            context.iteration(true);
          }
          #[cfg(not(target_os = "linux"))]
          {
            match cmd_rx.blocking_recv() {
              Some(TrayCommand::Sync(menu_state)) => {
                play_pause.set_text(menu_state.play_pause_label);
                play_pause.set_enabled(menu_state.play_pause_enabled);
                next.set_text(menu_state.next_label);
                previous.set_text(menu_state.previous_label);
                show.set_text(menu_state.show_label);
                quit.set_text(menu_state.quit_label);
                next.set_enabled(menu_state.next_enabled);
                previous.set_enabled(menu_state.previous_enabled);
                mute.set_text(menu_state.mute_label);
                mute.set_enabled(menu_state.mute_enabled);
                show.set_enabled(menu_state.show_enabled);
                quit.set_enabled(menu_state.quit_enabled);
              }
              Some(TrayCommand::Shutdown) | None => return,
            }
          }
        }
      })
      .map_err(|error| error.to_string())?;

    match init_rx.recv() {
      Ok(Ok(())) => Ok(Self {
        cmd_tx,
        action_rx: Arc::new(tokio::sync::Mutex::new(action_rx)),
        #[cfg(target_os = "linux")]
        glib_context,
        thread: Some(thread),
      }),
      Ok(Err(error)) => {
        let _ = thread.join();
        Err(error)
      }
      Err(_) => {
        let _ = thread.join();
        Err("Tray thread initialization disconnected".to_owned())
      }
    }
  }

  pub fn sync(&self, view: &SessionView, quitting: bool, locale: Localizer) {
    let menu = tray_menu_state(view, quitting, locale);
    let _ = self.cmd_tx.send(TrayCommand::Sync(menu));
    #[cfg(target_os = "linux")]
    self.glib_context.wakeup();
  }

  pub fn channel(&self) -> TrayEventChannel {
    TrayEventChannel {
      receiver: Arc::clone(&self.action_rx),
    }
  }
}

impl Drop for Tray {
  fn drop(&mut self) {
    let _ = self.cmd_tx.send(TrayCommand::Shutdown);
    #[cfg(target_os = "linux")]
    self.glib_context.wakeup();
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

pub(crate) fn tray_menu_state(
  view: &SessionView,
  quitting: bool,
  locale: Localizer,
) -> TrayMenuState {
  let active = view.now_playing.as_ref();
  let controls_enabled = active.is_some() && view.engine_available && !quitting;
  TrayMenuState {
    play_pause_label: locale.text(if active.is_some_and(|playing| !playing.paused) {
      "common-pause"
    } else {
      "common-play"
    }),
    next_label: locale.text("common-next"),
    previous_label: locale.text("common-previous"),
    show_label: locale.text("tray-show"),
    quit_label: locale.text("tray-quit"),
    play_pause_enabled: controls_enabled,
    next_enabled: controls_enabled
      && matches!(view.adjacent.next, AdjacentAvailability::Available { .. }),
    previous_enabled: controls_enabled
      && matches!(
        view.adjacent.previous,
        AdjacentAvailability::Available { .. }
      ),
    mute_label: locale.text(if active.is_some_and(|playing| playing.muted) {
      "player-unmute"
    } else {
      "player-mute"
    }),
    mute_enabled: controls_enabled,
    show_enabled: true,
    quit_enabled: !quitting,
  }
}

fn tray_icon_image() -> Result<Icon, String> {
  let (rgba, width, height) = crate::decode_icon(crate::TRAY_ICON_PNG)
    .ok_or_else(|| "bundled tray icon failed to decode".to_owned())?;
  Icon::from_rgba(rgba, width, height).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
  use jellypilot_mpv::playback::NowPlayingItem;
  use jellypilot_mpv::playback_session::{AdjacentView, NowPlayingView, SessionView, TracksView};

  use super::*;

  fn session_view(paused: bool, muted: bool, busy: bool) -> SessionView {
    SessionView {
      now_playing: Some(NowPlayingView {
        item: NowPlayingItem {
          item_id: "episode-1".to_owned(),
          title: "Pilot".to_owned(),
          item_type: "Episode".to_owned(),
          runtime_seconds: Some(1_800.0),
          start_position_seconds: 0.0,
          play_method: "Transcode".to_owned(),
          original_language: None,
        },
        paused,
        position_seconds: 120.0,
        duration_seconds: Some(1_800.0),
        volume: 75.0,
        muted,
      }),
      tracks: TracksView::Unavailable,
      adjacent: AdjacentView {
        previous: AdjacentAvailability::Unavailable,
        next: AdjacentAvailability::Available {
          title: "Next".to_owned(),
        },
      },
      intro_prompt: None,
      notice: None,
      engine_available: true,
      busy,
      can_start_login: true,
      quit_may_proceed: false,
    }
  }

  #[test]
  fn menu_state_shows_disabled_play_when_no_session_is_active() {
    let view = jellypilot_mpv::playback_session::PlaybackSession::default().view();
    let menu = tray_menu_state(&view, false, Localizer::default());

    assert!(!menu.play_pause_enabled);
  }

  #[test]
  fn menu_state_uses_live_transport_and_adjacent_availability() {
    let menu = tray_menu_state(
      &session_view(true, true, false),
      false,
      Localizer::default(),
    );

    assert!(menu.play_pause_enabled);
    assert!(menu.next_enabled);
    assert!(!menu.previous_enabled);
    assert!(menu.mute_enabled);
  }

  #[test]
  fn menu_state_keeps_playback_controls_enabled_when_busy() {
    let menu = tray_menu_state(
      &session_view(true, false, true),
      false,
      Localizer::default(),
    );

    assert!(menu.play_pause_enabled);
    assert!(menu.next_enabled);
    assert!(!menu.previous_enabled);
    assert!(menu.mute_enabled);
  }

  #[test]
  fn menu_state_disables_playback_actions_during_quit() {
    let menu = tray_menu_state(
      &session_view(false, false, false),
      true,
      Localizer::default(),
    );

    assert!(!menu.play_pause_enabled);
    assert!(!menu.next_enabled);
    assert!(!menu.previous_enabled);
    assert!(!menu.mute_enabled);
    assert!(!menu.quit_enabled);
    assert!(menu.show_enabled);
  }

  #[test]
  fn locale_change_refreshes_every_label_without_changing_availability() {
    use jellypilot_core::locale::UiLanguage;
    let view = session_view(false, true, false);
    let english = tray_menu_state(&view, false, Localizer::new(UiLanguage::English));
    let chinese = tray_menu_state(&view, false, Localizer::new(UiLanguage::SimplifiedChinese));
    assert_ne!(english.play_pause_label, chinese.play_pause_label);
    assert_ne!(english.next_label, chinese.next_label);
    assert_ne!(english.previous_label, chinese.previous_label);
    assert_ne!(english.mute_label, chinese.mute_label);
    assert_ne!(english.show_label, chinese.show_label);
    assert_ne!(english.quit_label, chinese.quit_label);
    assert_eq!(english.play_pause_enabled, chinese.play_pause_enabled);
    assert_eq!(english.next_enabled, chinese.next_enabled);
    assert_eq!(english.previous_enabled, chinese.previous_enabled);
    assert_eq!(english.mute_enabled, chinese.mute_enabled);
    assert_eq!(english.show_enabled, chinese.show_enabled);
    assert_eq!(english.quit_enabled, chinese.quit_enabled);
  }

  #[test]
  fn unavailable_engine_disables_transport_but_keeps_window_actions() {
    let mut view = session_view(false, false, false);
    view.engine_available = false;
    let menu = tray_menu_state(&view, false, Localizer::default());
    assert!(!menu.play_pause_enabled);
    assert!(!menu.next_enabled);
    assert!(!menu.previous_enabled);
    assert!(!menu.mute_enabled);
    assert!(menu.show_enabled);
    assert!(menu.quit_enabled);
  }

  #[test]
  fn transport_change_updates_only_transport_labels() {
    let locale = Localizer::default();
    let playing = tray_menu_state(&session_view(false, false, false), false, locale);
    let paused = tray_menu_state(&session_view(true, true, false), false, locale);
    assert_ne!(playing.play_pause_label, paused.play_pause_label);
    assert_ne!(playing.mute_label, paused.mute_label);
    assert_eq!(playing.next_label, paused.next_label);
    assert_eq!(playing.show_label, paused.show_label);
  }

  #[test]
  fn tray_event_channel_implements_equality_and_hash_by_receiver() {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let (_tx1, rx1) = tokio::sync::mpsc::unbounded_channel();
    let channel1 = TrayEventChannel {
      receiver: Arc::new(tokio::sync::Mutex::new(rx1)),
    };
    let channel1_clone = channel1.clone();

    let (_tx2, rx2) = tokio::sync::mpsc::unbounded_channel();
    let channel2 = TrayEventChannel {
      receiver: Arc::new(tokio::sync::Mutex::new(rx2)),
    };

    assert_eq!(channel1, channel1_clone);
    assert_ne!(channel1, channel2);

    let hash_channel = |channel: &TrayEventChannel| {
      let mut hasher = DefaultHasher::new();
      channel.hash(&mut hasher);
      hasher.finish()
    };

    assert_eq!(hash_channel(&channel1), hash_channel(&channel1_clone));
    assert_ne!(hash_channel(&channel1), hash_channel(&channel2));
  }
}
