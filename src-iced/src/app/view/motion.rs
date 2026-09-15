//! View-side helpers for the shared `widgets::motion` primitives.
//!
//! The app root wraps the whole tree in `motion::scope`, which carries the
//! reduced-motion/hidden-window policy to every descendant including overlay
//! layers. Callers here therefore pass `enabled: true` and keep only the
//! duration layering and key derivation local.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use iced::Element;
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::widgets::motion;

pub(crate) use jellypilot_ui::widgets::motion::Axis;

use crate::app::message::Message;
use crate::app::personal_lists::Route;
use crate::app::state::{Destination, State};

/// An owned, sanitized snapshot of the account modal's last visible content.
///
/// `motion::reveal` animates the supplied child, so a dismissed dialog needs
/// retained data to keep drawing during its exit. The snapshot carries only
/// display facts — the password field survives as a length for masking, never
/// as text — and is overwritten by the next modal's own snapshot.
#[derive(Clone, Debug)]
pub(crate) enum RetainedModal {
  Confirmation {
    kind: crate::app::accounts::ConfirmationKind,
    account: Option<String>,
    delete_watchlist: bool,
    active_profile: bool,
  },
  AddAccount(RetainedAddAccount),
}

/// The retained add-account form without its secrets: the password is reduced
/// to a length so the exiting dialog can mask it.
#[derive(Clone, Debug)]
pub(crate) struct RetainedAddAccount {
  pub provider: jellypilot_media_server::MediaServerProvider,
  pub method: crate::app::state::LoginMethod,
  pub server_url: String,
  pub username: String,
  pub password_len: usize,
  pub remember: bool,
  pub quick_connect: crate::app::state::QuickConnectState,
  pub busy: bool,
  pub error: Option<crate::i18n::UiText>,
}

/// An owned snapshot of the docked player bar's last visible content.
///
/// Captured once when `now_playing` clears so the bar's exit reveal can keep
/// drawing the real presentation; the artwork handle keeps the thumbnail
/// alive without new image demand.
#[derive(Clone, Debug)]
pub(crate) struct RetainedPlayerBar {
  pub now_playing: jellypilot_mpv::playback_session::NowPlayingView,
  pub caption: String,
  pub artwork: Option<iced::widget::image::Handle>,
  pub intro_prompt: Option<jellypilot_mpv::playback_session::IntroPromptView>,
}

/// Hover/press-scale surfaces: menus, dialogs, account surfaces, and
/// player-control visibility.
pub(crate) const SURFACE_DURATION: std::time::Duration = TOKENS.durations.ms200;
/// Structural changes: page navigation, Sidebar collapse, grid reflow, Hero
/// changes, and long-overview expansion.
pub(crate) const STRUCTURAL_DURATION: std::time::Duration = TOKENS.durations.ms300;

/// A stable key for the current page so `motion::transition` animates only
/// real navigation, never data refreshes on the same destination.
pub(crate) fn page_key(destination: &Destination) -> u64 {
  let mut hasher = DefaultHasher::new();
  match destination {
    Destination::Home => 0u8.hash(&mut hasher),
    Destination::Library {
      library_id,
      collection_type,
    } => {
      1u8.hash(&mut hasher);
      library_id.hash(&mut hasher);
      collection_type.hash(&mut hasher);
    }
    Destination::Search(query) => {
      2u8.hash(&mut hasher);
      query.hash(&mut hasher);
    }
    Destination::PersonalLists(route) => {
      3u8.hash(&mut hasher);
      route_key(*route).hash(&mut hasher);
    }
    Destination::Detail(item_id) => {
      4u8.hash(&mut hasher);
      item_id.hash(&mut hasher);
    }
    Destination::NowPlaying => 5u8.hash(&mut hasher),
  }
  hasher.finish()
}

const fn route_key(route: Route) -> u8 {
  match route {
    Route::Overview => 0,
    Route::Favorites => 1,
    Route::Watchlist => 2,
    Route::History => 3,
  }
}

/// A stable key for the featured Home hero. The revision advances only on a
/// user hero selection, so data refreshes, first content arrival, and
/// reconciliation of a stale selection never animate.
pub(crate) fn hero_key(state: &State) -> u64 {
  state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .hero_motion_revision()
}

/// Reveals a surface over the surface duration, keeping its exit presentation
/// until the animation settles.
pub(crate) fn reveal<'a>(
  content: impl Into<Element<'a, Message>>,
  visible: bool,
) -> Element<'a, Message> {
  motion::reveal(content, visible, true, SURFACE_DURATION)
}

/// Transitions between keyed targets over the structural duration.
pub(crate) fn transition<'a>(
  content: impl Into<Element<'a, Message>>,
  key: u64,
) -> Element<'a, Message> {
  motion::transition(content, key, true, STRUCTURAL_DURATION)
}

/// Animates a discrete structural size change over the structural duration.
pub(crate) fn resize<'a>(
  content: impl Into<Element<'a, Message>>,
  key: u64,
  axis: Axis,
) -> Element<'a, Message> {
  motion::resize(content, key, true, STRUCTURAL_DURATION, axis)
}

/// Collapses docked flow content over the surface duration so surrounding
/// layout shrinks with it (e.g. the player bar's slot under the content).
pub(crate) fn collapse<'a>(
  content: impl Into<Element<'a, Message>>,
  visible: bool,
) -> Element<'a, Message> {
  motion::collapse(content, visible, true, SURFACE_DURATION)
}
