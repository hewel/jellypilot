//! Shell surface (ADR 0029): window lifecycle (show/hide/close, resize
//! tracking, smoke-run exit), the skeleton shimmer clock, the quit-handshake
//! flag, and destination navigation with its stack. Navigation and the
//! connected-surface reset choreograph the other surfaces' leave/enter hooks;
//! they live here because the destination stack they mutate is this surface's
//! state.

use std::time::Instant;

use iced::Task;
use jellypilot_core::browse_model::BrowseSource;
use jellypilot_core::config::AppMode;
use jellypilot_core::skeleton::skeleton_phase_at;
use jellypilot_media_server::VideoLibraryItem;

use super::browse;
use super::detail;
use super::home;
use super::kernel::Kernel;
use super::message::{Message, WindowMessage};
use super::playback;
use super::state::{Destination, State};

/// Shell surface slice: the window and navigation state behind the shell
/// frame (sidebar, content routing, toast layer).
pub struct Surface {
  pub smoke: bool,
  /// Latest known logical window size; drives size-class layout decisions.
  pub window_size: iced::Size,
  /// Full-mode window size stashed when entering Control-Only mode; restored
  /// on the way back. In-memory only — never persisted.
  pub full_window_size: Option<iced::Size>,
  /// Shimmer sweep phase in [0, 1) for skeleton placeholders; advanced by
  /// each `FrameTick` while skeletons are on screen.
  pub skeleton_phase: f32,
  /// Animation clock origin for the shimmer sweep. `None` while no skeletons
  /// are visible so the next loading burst restarts the sweep from phase 0.
  /// `pub(crate)` because update tests construct `State` literals.
  pub(crate) skeleton_animation_start: Option<Instant>,
  pub quit_requested: bool,
  pub settings_open: bool,
  pub destination: Destination,
  pub navigation_stack: Vec<Destination>,
}

impl Surface {
  pub fn new(smoke: bool) -> Self {
    Self {
      smoke,
      window_size: iced::Size::new(1600.0, 900.0),
      full_window_size: None,
      skeleton_phase: 0.0,
      skeleton_animation_start: None,
      quit_requested: false,
      settings_open: false,
      destination: Destination::Home,
      navigation_stack: Vec::new(),
    }
  }

  pub fn navigate_to(&mut self, destination: Destination) -> bool {
    if self.destination == destination {
      return false;
    }
    if !matches!(&destination, Destination::Detail(_)) {
      if let Some(index) = self
        .navigation_stack
        .iter()
        .rposition(|entry| entry == &destination)
      {
        self.navigation_stack.truncate(index);
        self.destination = destination;
        return true;
      }
    }
    self.navigation_stack.push(self.destination.clone());
    self.destination = destination;
    true
  }

  pub fn navigate_back(&mut self) -> bool {
    let Some(destination) = self.navigation_stack.pop() else {
      return false;
    };
    self.destination = destination;
    true
  }
  pub fn open_settings(&mut self) {
    self.settings_open = true;
  }

  pub fn close_settings(&mut self) {
    self.settings_open = false;
  }
}
/// Fixed logical window size in Control-Only mode. `set_min_size ==
/// set_max_size` plus `set_resizable(false)` is the float hint tiling WMs
/// (i3/Sway) recognize; there is no standard float API.
pub const CONTROL_ONLY_WINDOW_SIZE: iced::Size = iced::Size::new(480.0, 760.0);
/// Minimum logical window size in Full mode.
pub const FULL_MIN_WINDOW_SIZE: iced::Size = iced::Size::new(1024.0, 640.0);
/// Full-mode window size applied when no stashed size exists.
pub const FULL_DEFAULT_WINDOW_SIZE: iced::Size = iced::Size::new(1600.0, 900.0);

/// Window geometry decision for one app mode. Pure so the fixed/restore
/// policy is testable without executing window tasks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ModeGeometry {
  pub size: iced::Size,
  pub min_size: Option<iced::Size>,
  pub max_size: Option<iced::Size>,
  pub resizable: bool,
}

pub(crate) fn mode_geometry(mode: AppMode, restore_size: Option<iced::Size>) -> ModeGeometry {
  match mode {
    AppMode::ControlOnly => ModeGeometry {
      size: CONTROL_ONLY_WINDOW_SIZE,
      min_size: Some(CONTROL_ONLY_WINDOW_SIZE),
      max_size: Some(CONTROL_ONLY_WINDOW_SIZE),
      resizable: false,
    },
    AppMode::Full => ModeGeometry {
      size: restore_size.unwrap_or(FULL_DEFAULT_WINDOW_SIZE),
      min_size: Some(FULL_MIN_WINDOW_SIZE),
      max_size: None,
      resizable: true,
    },
  }
}

/// Whether a destination is reachable in the given app mode. Control-Only has
/// no Library Browser, so the router rejects Home/Library/Search/Detail
/// navigation before the shell touches its stack.
pub(crate) fn destination_allowed(mode: AppMode, destination: &Destination) -> bool {
  mode == AppMode::Full || matches!(destination, Destination::NowPlaying)
}

/// Applies the window geometry decision to the live window.
fn window_geometry_task(geometry: ModeGeometry) -> Task<Message> {
  iced::window::latest().and_then(move |id| {
    Task::batch([
      iced::window::set_resizable(id, geometry.resizable),
      iced::window::set_min_size(id, geometry.min_size),
      iced::window::set_max_size(id, geometry.max_size),
      iced::window::resize(id, geometry.size),
    ])
  })
}

/// App-mode switch routine, invoked by the top-level router when the App Mode
/// setting changes. It lives here because it mutates this surface's
/// destination stack and drives the window (ADR 0029). Entering Control-Only
/// aborts in-flight Library Browser work, drops the home/browse/detail
/// surfaces, stashes the window size, and pins the window to the fixed
/// controller size; entering Full restores the window and lands on Home
/// through the normal activation path.
pub(crate) fn apply_app_mode(state: &mut State, mode: AppMode) -> Task<Message> {
  match mode {
    AppMode::ControlOnly => {
      close_settings(state);
      browse::reset(&mut state.browse, &mut state.kernel);
      if state.playback.view.now_playing.is_none() {
        state.kernel.artwork_adapter.cancel_pending();
      }
      state.home = home::Surface::default();
      state.detail = detail::Surface::default();
      state.retain_artwork_handles();
      state.shell.full_window_size = Some(state.shell.window_size);
      state.shell.navigation_stack.clear();
      state.shell.destination = Destination::NowPlaying;
      window_geometry_task(mode_geometry(mode, None))
    }
    AppMode::Full => {
      let geometry = mode_geometry(mode, state.shell.full_window_size.take());
      let previous = std::mem::replace(&mut state.shell.destination, Destination::Home);
      state.shell.navigation_stack.clear();
      let activation = activate_destination(state, previous);
      Task::batch([window_geometry_task(geometry), activation])
    }
  }
}

/// Shell surface entry point: reduces a [`WindowMessage`]. Cross-surface
/// follow-ups are hoisted to the top-level router (ADR 0029): a close without
/// an available tray runs the playback quit handshake there, and a resize
/// re-syncs the browse scroll window and re-retains artwork handles there.
/// `skeletons_active` is the router-computed read across every surface's load
/// states, so this module never reads home/browse/detail state.
pub fn update(
  surface: &mut Surface,
  kernel: &mut Kernel,
  skeletons_active: bool,
  message: WindowMessage,
) -> Task<Message> {
  match message {
    WindowMessage::CloseRequested(id) if kernel.tray.is_some() => {
      iced::window::set_mode(id, iced::window::Mode::Hidden)
    }
    WindowMessage::CloseRequested(_) => {
      surface.quit_requested = true;
      Task::none()
    }
    WindowMessage::ShowRequested(id) => id.map_or_else(Task::none, |id| {
      iced::window::set_mode(id, iced::window::Mode::Windowed).chain(iced::window::gain_focus(id))
    }),
    WindowMessage::Resized(size) => {
      surface.window_size = size;
      Task::none()
    }
    WindowMessage::FrameTick(now) => {
      // Smoke runs only need proof that the first frame rendered.
      if surface.smoke {
        surface.smoke = false;
        return iced::exit();
      }
      if skeletons_active {
        let start = surface.skeleton_animation_start.get_or_insert(now);
        surface.skeleton_phase = skeleton_phase_at(now.duration_since(*start));
      } else {
        // Restart the sweep from phase 0 on the next loading burst.
        surface.skeleton_animation_start = None;
        surface.skeleton_phase = 0.0;
      }
      Task::none()
    }
  }
}

pub(crate) fn navigate(state: &mut State, destination: Destination) -> Task<Message> {
  let previous = state.shell.destination.clone();
  if !state.shell.navigate_to(destination) {
    return Task::none();
  }
  activate_destination(state, previous)
}

pub(crate) fn open_detail(state: &mut State, item: VideoLibraryItem) -> Task<Message> {
  let item_id = item.id.clone();
  state.detail.items.insert(item_id.clone(), item);
  navigate(state, Destination::Detail(item_id))
}

pub(crate) fn navigate_back(state: &mut State) -> Task<Message> {
  let previous = state.shell.destination.clone();
  if !state.shell.navigate_back() {
    return Task::none();
  }
  activate_destination(state, previous)
}
pub(crate) fn open_settings(state: &mut State) {
  state.shell.open_settings();
}

pub(crate) fn close_settings(state: &mut State) {
  state.shell.close_settings();
  state.settings.view.shortcut_capture = None;
  state.settings.view.intro_menu_open = false;
  state.settings.view.subtitle_menu_open = false;
  state.settings.view.diagnostic_level_menu_open = false;
  state.settings.view.diagnostic_category_menu_open = false;
}

fn activate_destination(state: &mut State, previous: Destination) -> Task<Message> {
  if previous == Destination::Home && state.shell.destination != Destination::Home {
    home::leave_view(
      &mut state.home,
      &mut state.kernel,
      state.playback.view.now_playing.is_none(),
    );
  } else if matches!(
    previous,
    Destination::Library { .. } | Destination::Search(_)
  ) && !matches!(
    state.shell.destination,
    Destination::Library { .. } | Destination::Search(_)
  ) {
    browse::leave_view(
      &mut state.browse,
      &mut state.kernel,
      state.playback.view.now_playing.is_none(),
    );
  } else if matches!(previous, Destination::Detail(_)) && previous != state.shell.destination {
    detail::leave_view(
      &mut state.detail,
      &mut state.kernel,
      state.playback.view.now_playing.is_none(),
    );
  }

  match &state.shell.destination {
    Destination::Home => home::start_load(
      &mut state.home,
      &mut state.kernel,
      state.playback.view.now_playing.is_none(),
    ),
    Destination::Library { .. } => {
      state.browse.search_input.clear();
      let source = browse_source(state);
      browse::start(
        &mut state.browse,
        &mut state.kernel,
        source,
        state.playback.view.now_playing.is_none(),
      )
    }
    Destination::Search(_) => {
      let source = browse_source(state);
      browse::start(
        &mut state.browse,
        &mut state.kernel,
        source,
        state.playback.view.now_playing.is_none(),
      )
    }
    Destination::Detail(item_id) => {
      detail::start_load(&mut state.detail, &mut state.kernel, Some(item_id))
    }
    Destination::NowPlaying => Task::none(),
  }
}

pub(crate) fn browse_source(state: &State) -> Option<BrowseSource> {
  let session = state.kernel.request_gate.current_session();
  match &state.shell.destination {
    Destination::Library {
      library_id,
      collection_type,
    } => {
      let jellypilot_core::LoadState::Ready(shortcuts) = &state.home.data.shortcuts else {
        return None;
      };
      shortcuts
        .iter()
        .find(|shortcut| shortcut.id == *library_id && shortcut.collection_type == *collection_type)
        .cloned()
        .map(|shortcut| BrowseSource::Library { session, shortcut })
    }
    Destination::Search(query) => Some(BrowseSource::Search {
      session,
      query: query.clone(),
    }),
    Destination::Home | Destination::Detail(_) | Destination::NowPlaying => None,
  }
}

/// Router-level connected-surface reset, invoked on disconnect; it touches
/// every surface, so it takes `&mut State`, but it lives here because the
/// navigation reset is this surface's state (ADR 0029).
pub(crate) fn reset_connected_surface(state: &mut State) -> Task<Message> {
  let playback_task = playback::disconnect(
    &mut state.playback,
    &mut state.kernel,
    state.shell.quit_requested,
  );
  browse::reset(&mut state.browse, &mut state.kernel);
  state.kernel.artwork_adapter.reset_session();
  state.kernel.artwork_binder.reset();
  state.home = home::Surface::default();
  state.detail = detail::Surface::default();
  state.kernel.artwork_handles.clear();
  state.shell.navigation_stack.clear();
  state.shell.destination = Destination::Home;
  close_settings(state);
  playback_task
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::time::Duration;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_auth::AuthStore;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::diagnostics::Diagnostics;
  use jellypilot_core::request_gate::RequestGate;

  use super::*;
  use crate::app::state::ArtworkHandleRetention;

  fn test_fixture() -> (Surface, Kernel) {
    let kernel = Kernel {
      settings: SettingsStore::default(),
      diagnostics: Diagnostics::default(),
      auth_store: AuthStore::default(),
      request_gate: RequestGate::default(),
      client: None,
      connection: ConnectionPhase::SignedOut,
      connected_identity: None,
      active_profile: None,
      notice: None,
      active_toast: None,
      next_toast_id: 0,
      tray: None,
      artwork_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      artwork_binder: Default::default(),
      artwork_handles: ArtworkHandleRetention::default(),
    };
    (Surface::new(false), kernel)
  }

  #[test]
  fn frame_tick_advances_phase_while_skeletons_load_and_resets_after() {
    let (mut surface, mut kernel) = test_fixture();

    let start = Instant::now();
    drop(update(
      &mut surface,
      &mut kernel,
      true,
      WindowMessage::FrameTick(start),
    ));
    assert_eq!(surface.skeleton_phase, 0.0);
    assert_eq!(surface.skeleton_animation_start, Some(start));

    drop(update(
      &mut surface,
      &mut kernel,
      true,
      WindowMessage::FrameTick(start + Duration::from_millis(800)),
    ));
    assert_eq!(surface.skeleton_phase, 0.5);

    // Once nothing loads, the clock resets so the next burst starts at 0.
    drop(update(
      &mut surface,
      &mut kernel,
      false,
      WindowMessage::FrameTick(start + Duration::from_millis(1200)),
    ));
    assert_eq!(surface.skeleton_phase, 0.0);
    assert_eq!(surface.skeleton_animation_start, None);
  }

  #[test]
  fn window_resize_updates_the_tracked_window_size() {
    let (mut surface, mut kernel) = test_fixture();
    let size = iced::Size::new(1024.0, 768.0);

    drop(update(
      &mut surface,
      &mut kernel,
      false,
      WindowMessage::Resized(size),
    ));

    assert_eq!(surface.window_size, size);
  }
  #[test]
  fn control_only_geometry_pins_the_window_to_a_fixed_size() {
    let geometry = mode_geometry(AppMode::ControlOnly, Some(iced::Size::new(1440.0, 810.0)));

    assert_eq!(geometry.size, CONTROL_ONLY_WINDOW_SIZE);
    assert_eq!(geometry.min_size, Some(CONTROL_ONLY_WINDOW_SIZE));
    assert_eq!(geometry.max_size, Some(CONTROL_ONLY_WINDOW_SIZE));
    assert!(!geometry.resizable);
  }

  #[test]
  fn full_geometry_restores_the_stashed_size_or_the_default() {
    let stashed = iced::Size::new(1280.0, 800.0);

    let restored = mode_geometry(AppMode::Full, Some(stashed));
    assert_eq!(restored.size, stashed);
    assert_eq!(restored.min_size, Some(FULL_MIN_WINDOW_SIZE));
    assert_eq!(restored.max_size, None);
    assert!(restored.resizable);

    let fresh = mode_geometry(AppMode::Full, None);
    assert_eq!(fresh.size, FULL_DEFAULT_WINDOW_SIZE);
    assert_eq!(fresh.min_size, Some(FULL_MIN_WINDOW_SIZE));
    assert_eq!(fresh.max_size, None);
    assert!(fresh.resizable);
  }

  #[test]
  fn control_only_navigation_is_limited_to_now_playing() {
    let library = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };
    for destination in [
      Destination::Home,
      library,
      Destination::Search("term".to_owned()),
      Destination::Detail("movie-1".to_owned()),
    ] {
      assert!(!destination_allowed(AppMode::ControlOnly, &destination));
      assert!(destination_allowed(AppMode::Full, &destination));
    }
    assert!(destination_allowed(
      AppMode::ControlOnly,
      &Destination::NowPlaying
    ));
    assert!(destination_allowed(AppMode::Full, &Destination::NowPlaying));
  }

  #[test]
  fn shell_surface_tracks_settings_open_and_close() {
    let mut surface = Surface::new(false);
    assert!(!surface.settings_open);
    surface.open_settings();
    assert!(surface.settings_open);
    surface.close_settings();
    assert!(!surface.settings_open);
  }

  #[test]
  fn navigation_stack_restores_each_previous_destination_in_order() {
    let mut surface = Surface::new(false);
    let library = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };
    assert!(surface.navigate_to(library.clone()));
    assert!(surface.navigate_to(Destination::Detail("movie-1".to_owned())));
    assert!(surface.navigate_back());
    assert_eq!(surface.destination, library);
    assert!(surface.navigate_back());
    assert_eq!(surface.destination, Destination::Home);
    assert!(!surface.navigate_back());
  }

  #[test]
  fn navigating_to_the_current_destination_does_not_create_a_false_back_entry() {
    let mut surface = Surface::new(false);

    assert!(!surface.navigate_to(Destination::Home));
    assert!(surface.navigation_stack.is_empty());
  }

  #[test]
  fn sidebar_cycles_keep_the_navigation_stack_bounded() {
    let mut surface = Surface::new(false);
    let library = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };

    assert!(surface.navigate_to(library.clone()));
    assert!(surface.navigate_to(Destination::Home));
    assert!(surface.navigate_to(library.clone()));
    assert!(surface.navigate_to(Destination::Home));
    assert!(surface.navigate_to(library.clone()));

    assert_eq!(surface.destination, library);
    assert_eq!(surface.navigation_stack, vec![Destination::Home]);
  }

  #[test]
  fn back_after_cycle_collapse_visits_each_destination_once() {
    let mut surface = Surface::new(false);
    let library = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };
    assert!(surface.navigate_to(library.clone()));
    assert!(surface.navigate_to(Destination::Home));
    assert!(surface.navigate_to(library.clone()));

    assert!(surface.navigate_back());
    assert_eq!(surface.destination, Destination::Home);
    assert!(!surface.navigate_back());
  }

  #[test]
  fn detail_navigation_pushes_when_the_item_is_already_in_history() {
    let mut surface = Surface::new(false);
    let detail = Destination::Detail("movie-1".to_owned());
    let library = Destination::Library {
      library_id: "movies".to_owned(),
      collection_type: "movies".to_owned(),
    };
    assert!(surface.navigate_to(detail.clone()));
    assert!(surface.navigate_to(library.clone()));

    assert!(surface.navigate_to(detail.clone()));

    assert_eq!(
      surface.navigation_stack,
      vec![Destination::Home, detail, library.clone()]
    );
    assert!(surface.navigate_back());
    assert_eq!(surface.destination, library);
  }
}
