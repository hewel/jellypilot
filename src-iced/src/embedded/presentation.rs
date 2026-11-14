//! Native surface identity and exclusive video presentation ownership.

use std::sync::{LazyLock, Mutex, MutexGuard};

use iced::window::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeWindow {
  Wayland(usize),
  Xlib(usize),
  Xcb(u32),
  Win32(isize),
}

/// Identity only: no pointer is dereferenced or used to retain a native window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationWindow(NativeWindow);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PresentationStatus {
  Preparing,
  Ready,
  Failed,
  Unavailable,
}

struct Registration {
  window: PresentationWindow,
  token: u64,
  supported: bool,
  status: PresentationStatus,
}

#[derive(Default)]
struct Registry {
  surfaces: Vec<Registration>,
  default: Option<PresentationWindow>,
  selected: Option<PresentationWindow>,
  epoch: u64,
}

impl Registry {
  fn active(&self) -> Option<PresentationWindow> {
    self.selected.or(self.default)
  }

  fn register(&mut self, window: PresentationWindow, token: u64, supported: bool) {
    let previous = self.active();
    self.surfaces.retain(|surface| surface.window != window);
    self.surfaces.push(Registration {
      window,
      token,
      supported,
      status: PresentationStatus::Preparing,
    });
    if self.default.is_none() && supported {
      self.default = Some(window);
    }
    if self.active() != previous || self.active() == Some(window) {
      self.epoch += 1;
    }
  }

  fn select(&mut self, window: Option<PresentationWindow>) -> bool {
    let target = window.or(self.default);
    if target.is_some_and(|target| {
      !self
        .surfaces
        .iter()
        .any(|surface| surface.window == target && surface.supported)
    }) {
      return false;
    }
    if self.active() != target {
      self.epoch += 1;
      if let Some(surface) = self
        .surfaces
        .iter_mut()
        .find(|surface| Some(surface.window) == target)
      {
        surface.status = PresentationStatus::Preparing;
      }
    }
    self.selected = window;
    true
  }

  fn remove(&mut self, token: u64) {
    let previous = self.active();
    let removed = self
      .surfaces
      .iter()
      .find(|surface| surface.token == token)
      .map(|surface| surface.window);
    self.surfaces.retain(|surface| surface.token != token);
    if removed.is_some() && removed == self.default {
      self.default = None;
    }
    // Explicit ownership survives loss: a different window must never silently
    // inherit video or inhibition. Lifecycle observes Unavailable and rolls back.
    if removed.is_some() && removed == previous {
      self.epoch += 1;
    }
  }
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

fn registry() -> MutexGuard<'static, Registry> {
  REGISTRY
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn native_window(window: &(impl HasWindowHandle + ?Sized)) -> Option<PresentationWindow> {
  let handle = window.window_handle().ok()?.as_raw();
  Some(PresentationWindow(match handle {
    RawWindowHandle::Wayland(handle) => NativeWindow::Wayland(handle.surface.as_ptr() as usize),
    RawWindowHandle::Xlib(handle) => NativeWindow::Xlib(handle.window as usize),
    RawWindowHandle::Xcb(handle) => NativeWindow::Xcb(handle.window.get()),
    RawWindowHandle::Win32(handle) => NativeWindow::Win32(handle.hwnd.get()),
    _ => return None,
  }))
}

/// Resolves only after the compositor has created a supported native surface.
/// Iced cancels the callback if the window has already closed.
pub(crate) fn presentation_window(id: iced::window::Id) -> iced::Task<Option<PresentationWindow>> {
  iced::window::run(id, |window| {
    let key = native_window(window)?;
    registry()
      .surfaces
      .iter()
      .any(|surface| surface.window == key && surface.supported)
      .then_some(key)
  })
}

/// `None` restores the original/default surface, never the most recently opened.
/// Rejected selections leave the current owner untouched.
pub(crate) fn set_presentation_window(window: Option<PresentationWindow>) -> bool {
  let mut registry = registry();
  if !registry.select(window) {
    return false;
  }
  let token = registry
    .surfaces
    .iter()
    .find(|surface| Some(surface.window) == registry.active())
    .map(|surface| surface.token);
  super::idle::select_surface(token);
  let mut frame = super::video::FRAME
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner);
  frame.select(registry.epoch);
  if let Some(shell) = &frame.shell {
    shell.request_redraw();
  }
  true
}

pub(crate) fn presentation_status(window: PresentationWindow) -> PresentationStatus {
  registry()
    .surfaces
    .iter()
    .find(|surface| surface.window == window)
    .map_or(PresentationStatus::Unavailable, |surface| {
      if surface.supported {
        surface.status
      } else {
        PresentationStatus::Failed
      }
    })
}

pub(super) fn register(window: &impl HasWindowHandle, token: u64, supported: bool) {
  let Some(window) = native_window(window) else {
    return;
  };
  let mut registry = registry();
  registry.register(window, token, supported);
  if registry.active() == Some(window) {
    super::idle::select_surface(supported.then_some(token));
  }
  // Iced draws the shader primitive before compositor::present. Publish a
  // newly registered owner's epoch here so that first primitive is not born
  // stale (especially when paused playback supplies no further redraw).
  super::video::FRAME
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
    .select(registry.epoch);
}

pub(super) fn dropped(token: u64) {
  registry().remove(token);
}

pub(super) fn selection(token: u64) -> (bool, u64) {
  let registry = registry();
  (
    registry.surfaces.iter().any(|surface| {
      surface.token == token && surface.supported && Some(surface.window) == registry.active()
    }),
    registry.epoch,
  )
}

pub(super) fn status(token: u64, status: PresentationStatus) {
  let mut registry = registry();
  if let Some(surface) = registry
    .surfaces
    .iter_mut()
    .find(|surface| surface.token == token)
  {
    surface.status = status;
  }
}

pub(super) fn shutdown() {
  let mut registry = registry();
  registry.surfaces.clear();
  registry.default = None;
  registry.selected = None;
  registry.epoch += 1;
}

#[cfg(test)]
mod tests {
  use super::*;

  fn window(id: usize) -> PresentationWindow {
    PresentationWindow(NativeWindow::Wayland(id))
  }

  #[test]
  fn opening_browse_surface_does_not_steal_selection() {
    let mut registry = Registry::default();
    registry.register(window(1), 1, true);
    registry.register(window(2), 2, true);
    assert!(registry.select(Some(window(2))));
    registry.register(window(3), 3, true);
    assert_eq!(registry.active(), Some(window(2)));
    assert!(registry.select(None));
    assert_eq!(registry.active(), Some(window(1)));
  }

  #[test]
  fn unsupported_target_cannot_displace_current_surface() {
    let mut registry = Registry::default();
    registry.register(window(1), 1, true);
    registry.register(window(2), 2, false);
    assert!(!registry.select(Some(window(2))));
    assert_eq!(registry.active(), Some(window(1)));
  }

  #[test]
  fn stale_surface_drop_preserves_replacement_and_selection() {
    let mut registry = Registry::default();
    registry.register(window(1), 1, true);
    registry.register(window(2), 2, true);
    registry.select(Some(window(2)));
    registry.register(window(2), 3, true);
    registry.remove(2);
    assert_eq!(registry.active(), Some(window(2)));
    assert!(registry.surfaces.iter().any(|surface| surface.token == 3));
    registry.remove(3);
    assert_eq!(registry.active(), Some(window(2)));
    assert!(registry.select(None));
    assert_eq!(registry.active(), Some(window(1)));
  }

  #[test]
  fn recreated_main_does_not_interrupt_selected_pip() {
    let mut registry = Registry::default();
    registry.register(window(1), 1, true);
    registry.register(window(2), 2, true);
    registry.select(Some(window(2)));
    let epoch = registry.epoch;
    registry.remove(1);
    registry.register(window(3), 3, true);
    assert_eq!(registry.active(), Some(window(2)));
    assert_eq!(registry.epoch, epoch);
    assert!(registry.select(None));
    assert_eq!(registry.active(), Some(window(3)));
  }
}
