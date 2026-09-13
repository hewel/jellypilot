//! Session idle inhibition for Embedded MPV Playback on Linux.
//!
//! The embedded libmpv host context cannot reach mpv's own screensaver
//! handling, so the application owns inhibition: while a Playback Session is
//! active and unpaused, the native inhibitor bound by
//! [`jellypilot_mpv_host::IdleInhibit`] keeps the session from auto-locking.
//! External MPV Playback inhibits itself and never registers a surface here.
//!
//! The compositor registers its display once and each surface as it is
//! created; [`set_desired`] follows the projected Now Playing state. With no
//! registered surface (window closed to the tray) nothing is bound — Wayland
//! idle inhibition is defined on a presentation surface, and reopening the
//! window rebinds automatically.

use jellypilot_mpv::playback_session::NowPlayingView;

/// Whether the projected playback state wants the session kept awake:
/// a loaded Now Playing item that is not paused, mirroring mpv's
/// `playback_active` semantics.
pub(crate) fn wants_inhibit(now_playing: Option<&NowPlayingView>) -> bool {
  now_playing.is_some_and(|view| !view.paused)
}

#[cfg(target_os = "linux")]
mod imp {
  use iced::advanced::graphics::compositor;
  use std::sync::Mutex;

  static INHIBIT: Mutex<jellypilot_mpv_host::IdleInhibit> =
    Mutex::new(jellypilot_mpv_host::IdleInhibit::new());

  fn lock() -> std::sync::MutexGuard<'static, jellypilot_mpv_host::IdleInhibit> {
    INHIBIT
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  pub(crate) fn display_changed(display: impl compositor::Display) {
    lock().display_changed(display);
  }

  pub(crate) fn surface_changed(window: impl compositor::Window + Clone) -> u64 {
    lock().surface_changed(window)
  }

  pub(crate) fn surface_dropped(token: u64) {
    lock().surface_dropped(token);
  }

  pub(crate) fn set_desired(desired: bool) {
    lock().set_desired(desired);
  }

  pub(crate) fn shutdown() {
    lock().shutdown();
  }

  /// Whether a platform inhibitor is bound right now. Read by the native
  /// regression probe to record real acquisition evidence.
  pub(crate) fn inhibited() -> bool {
    lock().inhibited()
  }

  /// The last bind failure, surfaced into the regression report.
  pub(crate) fn last_error() -> Option<String> {
    lock().last_error().map(str::to_owned)
  }
}

#[cfg(not(target_os = "linux"))]
mod imp {
  pub(crate) fn set_desired(_desired: bool) {}

  #[cfg(target_os = "windows")]
  mod windows {
    use iced::advanced::graphics::compositor;

    pub(crate) fn display_changed(_display: impl compositor::Display) {}

    pub(crate) fn surface_changed(_window: impl compositor::Window + Clone) -> u64 {
      0
    }

    pub(crate) fn surface_dropped(_token: u64) {}

    pub(crate) fn shutdown() {}
  }

  #[cfg(target_os = "windows")]
  pub(crate) use windows::*;
}

pub(crate) use imp::*;
