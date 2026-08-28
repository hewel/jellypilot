use jellypilot_mpv::playback_session::{AdjacentAvailability, SessionView};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

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

pub struct Tray {
  _icon: TrayIcon,
  play_pause: MenuItem,
  next: MenuItem,
  previous: MenuItem,
  mute: MenuItem,
  quit: MenuItem,
}

impl Tray {
  pub fn new() -> Result<Self, String> {
    #[cfg(target_os = "linux")]
    gtk::init().map_err(|error| error.to_string())?;
    let play_pause = MenuItem::with_id(PLAY_PAUSE_ID, "Play", false, None);
    let next = MenuItem::with_id(NEXT_ID, "Next", false, None);
    let previous = MenuItem::with_id(PREVIOUS_ID, "Previous", false, None);
    let mute = MenuItem::with_id(MUTE_ID, "Mute", false, None);
    let show = MenuItem::with_id(SHOW_ID, "Show", true, None);
    let quit = MenuItem::with_id(QUIT_ID, "Quit", true, None);
    let separator = PredefinedMenuItem::separator();
    let menu = Menu::with_items(&[
      &play_pause,
      &next,
      &previous,
      &mute,
      &separator,
      &show,
      &quit,
    ])
    .map_err(|error| error.to_string())?;
    let icon = TrayIconBuilder::new()
      .with_tooltip("JellyPilot")
      .with_icon(tray_icon_image()?)
      .with_menu(Box::new(menu))
      .with_menu_on_left_click(false)
      .build()
      .map_err(|error| error.to_string())?;

    Ok(Self {
      _icon: icon,
      play_pause,
      next,
      previous,
      mute,
      quit,
    })
  }

  pub fn sync(&self, view: &SessionView, quitting: bool) {
    let menu = tray_menu_state(view, quitting);
    self.play_pause.set_text(menu.play_pause_label);
    self.play_pause.set_enabled(menu.play_pause_enabled);
    self.next.set_enabled(menu.next_enabled);
    self.previous.set_enabled(menu.previous_enabled);
    self.mute.set_text(menu.mute_label);
    self.mute.set_enabled(menu.mute_enabled);
    self.quit.set_enabled(!quitting);
  }
}

pub fn next_action() -> Option<TrayAction> {
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
    if action.is_some() {
      return action;
    }
  }
  while let Ok(event) = TrayIconEvent::receiver().try_recv() {
    if matches!(
      event,
      TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
      }
    ) {
      return Some(TrayAction::Show);
    }
  }
  None
}

struct TrayMenuState {
  play_pause_label: &'static str,
  play_pause_enabled: bool,
  next_enabled: bool,
  previous_enabled: bool,
  mute_label: &'static str,
  mute_enabled: bool,
}

fn tray_menu_state(view: &SessionView, quitting: bool) -> TrayMenuState {
  let active = view.now_playing.as_ref();
  let controls_enabled = active.is_some() && !view.busy && !quitting;
  TrayMenuState {
    play_pause_label: if active.is_some_and(|playing| !playing.paused) {
      "Pause"
    } else {
      "Play"
    },
    play_pause_enabled: controls_enabled,
    next_enabled: controls_enabled
      && matches!(view.adjacent.next, AdjacentAvailability::Available { .. }),
    previous_enabled: controls_enabled
      && matches!(
        view.adjacent.previous,
        AdjacentAvailability::Available { .. }
      ),
    mute_label: if active.is_some_and(|playing| playing.muted) {
      "Unmute"
    } else {
      "Mute"
    },
    mute_enabled: controls_enabled,
  }
}

fn tray_icon_image() -> Result<Icon, String> {
  const SIZE: usize = 32;
  let mut rgba = vec![0_u8; SIZE * SIZE * 4];
  for pixel in rgba.chunks_exact_mut(4) {
    pixel.copy_from_slice(&[24, 35, 43, 255]);
  }
  for y in 7..25 {
    for x in 19..24 {
      set_pixel(&mut rgba, x, y, [89, 214, 196, 255]);
    }
  }
  for y in 20..25 {
    for x in 9..24 {
      set_pixel(&mut rgba, x, y, [89, 214, 196, 255]);
    }
  }
  for y in 16..25 {
    for x in 8..13 {
      set_pixel(&mut rgba, x, y, [89, 214, 196, 255]);
    }
  }
  Icon::from_rgba(rgba, SIZE as u32, SIZE as u32).map_err(|error| error.to_string())
}

fn set_pixel(rgba: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
  let offset = (y * 32 + x) * 4;
  rgba[offset..offset + 4].copy_from_slice(&color);
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
    let menu = tray_menu_state(&view, false);

    assert_eq!(menu.play_pause_label, "Play");
    assert!(!menu.play_pause_enabled);
  }

  #[test]
  fn menu_state_uses_live_transport_and_adjacent_availability() {
    let menu = tray_menu_state(&session_view(true, true, false), false);

    assert_eq!(menu.play_pause_label, "Play");
    assert!(menu.play_pause_enabled);
    assert!(menu.next_enabled);
    assert!(!menu.previous_enabled);
    assert_eq!(menu.mute_label, "Unmute");
    assert!(menu.mute_enabled);
  }

  #[test]
  fn menu_state_disables_playback_actions_during_quit() {
    let menu = tray_menu_state(&session_view(false, false, false), true);

    assert!(!menu.play_pause_enabled);
    assert!(!menu.next_enabled);
    assert!(!menu.previous_enabled);
    assert!(!menu.mute_enabled);
  }
}
