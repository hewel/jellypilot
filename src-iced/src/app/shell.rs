//! Shell surface (ADR 0029): window lifecycle (show/hide/close, resize
//! tracking, smoke-run exit), the skeleton shimmer clock, the quit-handshake
//! flag, and destination navigation with its stack. Navigation and the
//! connected-surface reset choreograph the other surfaces' leave/enter hooks;
//! they live here because the destination stack they mutate is this surface's
//! state.

use std::time::Instant;

use iced::Task;
use jellypilot_core::browse_model::BrowseSource;
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
  /// Shimmer sweep phase in [0, 1) for skeleton placeholders; advanced by
  /// each `FrameTick` while skeletons are on screen.
  pub skeleton_phase: f32,
  /// Animation clock origin for the shimmer sweep. `None` while no skeletons
  /// are visible so the next loading burst restarts the sweep from phase 0.
  /// `pub(crate)` because update tests construct `State` literals.
  pub(crate) skeleton_animation_start: Option<Instant>,
  pub quit_requested: bool,
  pub destination: Destination,
  pub navigation_stack: Vec<Destination>,
}

impl Surface {
  pub fn new(smoke: bool) -> Self {
    Self {
      smoke,
      window_size: iced::Size::new(1600.0, 900.0),
      skeleton_phase: 0.0,
      skeleton_animation_start: None,
      quit_requested: false,
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

fn activate_destination(state: &mut State, previous: Destination) -> Task<Message> {
  if previous == Destination::Settings && state.shell.destination != Destination::Settings {
    state.settings.view.shortcut_capture = None;
  }
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
    Destination::Settings => Task::none(),
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
    Destination::Home | Destination::Detail(_) | Destination::Settings => None,
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
