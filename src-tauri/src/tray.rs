//! System tray implementation for JMSR.
//!
//! Provides a tray icon with menu items:
//! - Play/Pause: Toggle playback
//! - Next: Play next episode
//! - Previous: Play previous episode
//! - Mute: Toggle mute
//! - Show Operations Console: Opens/focuses the main window
//! - Quit: Exits the application

use tauri::{
  menu::{Menu, MenuItem, PredefinedMenuItem},
  tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
  Manager,
};

use crate::command::{JellyfinState, MpvState};

/// Menu item IDs
const MENU_PLAY_PAUSE: &str = "play_pause";
const MENU_NEXT: &str = "next";
const MENU_PREVIOUS: &str = "previous";
const MENU_MUTE: &str = "mute";
const MENU_SHOW: &str = "show_console";
const MENU_QUIT: &str = "quit";

/// Sets up the system tray icon with menu.
///
/// # Menu Items
/// - **Play/Pause**: Toggle playback state
/// - **Next**: Play next episode
/// - **Previous**: Play previous episode
/// - **Mute**: Toggle mute
/// - **Show Operations Console**: Shows and focuses the main window
/// - **Quit**: Exits the application
///
/// # Tray Click Behavior
/// - Left-click: Shows and focuses the main window
pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
  // Create menu items
  let play_pause_item = MenuItem::with_id(app, MENU_PLAY_PAUSE, "Play/Pause", true, None::<&str>)?;
  let next_item = MenuItem::with_id(app, MENU_NEXT, "Next", true, None::<&str>)?;
  let previous_item = MenuItem::with_id(app, MENU_PREVIOUS, "Previous", true, None::<&str>)?;
  let mute_item = MenuItem::with_id(app, MENU_MUTE, "Mute", true, None::<&str>)?;
  let separator = PredefinedMenuItem::separator(app)?;
  let show_item = MenuItem::with_id(
    app,
    MENU_SHOW,
    "Show Operations Console",
    true,
    None::<&str>,
  )?;
  let quit_item = MenuItem::with_id(app, MENU_QUIT, "Quit", true, None::<&str>)?;

  // Build the menu
  let menu = Menu::with_items(
    app,
    &[
      &play_pause_item,
      &next_item,
      &previous_item,
      &mute_item,
      &separator,
      &show_item,
      &quit_item,
    ],
  )?;

  // Create tray icon
  let _tray = TrayIconBuilder::new()
    .icon(app.default_window_icon().unwrap().clone())
    .menu(&menu)
    .tooltip("JMSR - Jellyfin MPV Shim")
    .show_menu_on_left_click(false) // Left-click shows window, right-click shows menu
    .on_menu_event(|app, event| {
      match event.id.as_ref() {
        MENU_PLAY_PAUSE => {
          let mpv_state = app.state::<MpvState>();
          let mpv = mpv_state.0.clone();
          tauri::async_runtime::spawn(async move {
            // Toggle pause state
            match mpv.get_pause().await {
              Ok(is_paused) => {
                if let Err(e) = mpv.set_pause(!is_paused).await {
                  log::error!("Failed to toggle pause: {}", e);
                }
              }
              Err(e) => {
                log::warn!("MPV not connected or error getting pause state: {}", e);
              }
            }
          });
        }
        MENU_NEXT => {
          let jellyfin_state = app.state::<JellyfinState>();
          let session = jellyfin_state.session.read().clone();
          if let Some(session) = session {
            tauri::async_runtime::spawn(async move {
              if let Err(e) = session.play_next_episode().await {
                log::warn!("Failed to play next episode: {}", e);
              }
            });
          } else {
            log::warn!("No active Jellyfin session for next episode");
          }
        }
        MENU_PREVIOUS => {
          let jellyfin_state = app.state::<JellyfinState>();
          let session = jellyfin_state.session.read().clone();
          if let Some(session) = session {
            tauri::async_runtime::spawn(async move {
              if let Err(e) = session.play_previous_episode().await {
                log::warn!("Failed to play previous episode: {}", e);
              }
            });
          } else {
            log::warn!("No active Jellyfin session for previous episode");
          }
        }
        MENU_MUTE => {
          let mpv_state = app.state::<MpvState>();
          let mpv = mpv_state.0.clone();
          tauri::async_runtime::spawn(async move {
            if let Err(e) = mpv.toggle_mute().await {
              log::error!("Failed to toggle mute: {}", e);
            }
          });
        }
        MENU_SHOW => {
          if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
          }
        }
        MENU_QUIT => {
          app.exit(0);
        }
        _ => {}
      }
    })
    .on_tray_icon_event(|tray, event| {
      // Left-click on tray icon shows/focuses the window
      if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
      } = event
      {
        let app = tray.app_handle();
        if let Some(window) = app.get_webview_window("main") {
          let _ = window.show();
          let _ = window.set_focus();
        }
      }
    })
    .build(app)?;

  Ok(())
}
