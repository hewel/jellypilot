//! Home surface (ADR 0029): Video Home section/shortcut loading and the home
//! artwork pipeline (featured hero plus the section card rows).

use std::sync::Arc;

use super::artwork::{ImageCollection, ImageSpec};
use super::kernel::Kernel;
use super::message::{HomeMessage, Message};
use super::state::{HomeSection, HomeState};
use crate::i18n::UiText;
use iced::Task;
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel, Diagnostics};
use jellypilot_core::request_gate::{HomeToken, RequestGate};
use jellypilot_media_server::artwork::{ArtworkSizeClass, DerivedArtwork};
use jellypilot_media_server::home::{load_home_data, HomeDataResult};
use jellypilot_media_server::VideoLibraryItem;

/// Home surface slice: Video Home section data plus the artwork cells bound
/// for the hero and the section card rows.
#[derive(Default)]
pub struct Surface {
  pub data: HomeState,
  pub artwork: ImageCollection,
}

/// Updates Home content and its locally owned images.
pub fn update(surface: &mut Surface, kernel: &mut Kernel, message: HomeMessage) -> Task<Message> {
  match message {
    // Handled entirely by the top-level router: navigation writes the shared
    // destination stack and drives the other surfaces' leave/enter hooks.
    HomeMessage::Navigate(_) => Task::none(),
    HomeMessage::Retry => start_load(surface, kernel),
    HomeMessage::HeroSelected(item_id) => {
      let Some(index) = surface.data.select_hero(&item_id) else {
        return Task::none();
      };
      Task::batch([
        prepare_artwork(surface),
        super::view::home::reveal_hero_selection(index),
      ])
    }
    HomeMessage::CardHoverEnter(item_id) => {
      surface.data.hovered_card = Some(item_id);
      Task::none()
    }
    HomeMessage::CardHoverExit(item_id) => {
      if surface.data.hovered_card.as_deref() == Some(item_id.as_str()) {
        surface.data.hovered_card = None;
      }
      Task::none()
    }
    HomeMessage::Loaded { token, result } => {
      let failed = result.0.is_err()
        || result.1.is_err()
        || result
          .2
          .as_ref()
          .map_or(true, |rows| rows.iter().any(|row| row.result.is_err()));
      if !settle(
        &mut surface.data,
        &mut kernel.request_gate,
        &mut kernel.diagnostics,
        token,
        result,
      ) {
        return Task::none();
      }
      surface.data.hovered_card = None;
      let artwork = prepare_artwork(surface);
      if failed {
        Task::batch([
          artwork,
          kernel.show_toast(
            super::state::NoticeLevel::Error,
            UiText::new("home-refresh-failed"),
          ),
        ])
      } else {
        artwork
      }
    }
    HomeMessage::ArtworkLoaded(completion) => {
      surface
        .artwork
        .settle(kernel.request_gate.current_session(), completion);
      Task::none()
    }
  }
}

/// Starts (or refreshes) the Video Home load. The top-level router also calls
/// this after connect and when navigating to Home.
pub fn start_load(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  if !surface.data.has_ready_content() {
    surface.data.begin_load();
  }
  let token = kernel.request_gate.begin_home();
  let Some(client) = kernel.client.as_ref().map(Arc::clone) else {
    let error = "media-server-session-unavailable".to_owned();
    settle(
      &mut surface.data,
      &mut kernel.request_gate,
      &mut kernel.diagnostics,
      token,
      (Err(error.clone()), Err(error.clone()), Err(error)),
    );
    return Task::none();
  };

  Task::perform(load_home_data(client), move |result| {
    Message::Home(HomeMessage::Loaded { token, result })
  })
}

pub(crate) fn restore(surface: &mut Surface, kernel: &mut Kernel) -> Task<Message> {
  if !surface.data.has_ready_content()
    || surface
      .data
      .rows()
      .iter()
      .any(|row| matches!(row.items, jellypilot_core::LoadState::Loading))
  {
    start_load(surface, kernel)
  } else {
    prepare_artwork(surface)
  }
}

fn settle(
  data: &mut HomeState,
  request_gate: &mut RequestGate,
  diagnostics: &mut Diagnostics,
  token: HomeToken,
  result: HomeDataResult,
) -> bool {
  if !request_gate.finish_home(token) {
    return false;
  }
  let (video_home, shortcuts, latest_rows) = result;
  if let Err(error) = &video_home {
    diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Connection,
      format!("Home content load failed: {error}"),
    );
  }
  if let Err(error) = &shortcuts {
    diagnostics.record(
      DiagnosticLevel::Error,
      DiagnosticCategory::Connection,
      format!("Home libraries load failed: {error}"),
    );
  }
  match &latest_rows {
    Err(error) => {
      diagnostics.record(
        DiagnosticLevel::Error,
        DiagnosticCategory::Connection,
        format!("Home latest rows load failed: {error}"),
      );
    }
    Ok(rows) => {
      for row in rows {
        if let Err(error) = &row.result {
          diagnostics.record(
            DiagnosticLevel::Error,
            DiagnosticCategory::Connection,
            format!("Home latest row {} load failed: {error}", row.library_id),
          );
        }
      }
    }
  }
  data.settle_video_home(video_home);
  if shortcuts.is_ok() || !matches!(data.shortcuts, jellypilot_core::LoadState::Ready(_)) {
    data.settle_shortcuts(shortcuts);
  }
  let latest_ready = data
    .rows()
    .iter()
    .skip(2)
    .any(|row| matches!(row.items, jellypilot_core::LoadState::Ready(_)));
  let latest_failed = latest_rows
    .as_ref()
    .map_or(true, |rows| rows.iter().any(|row| row.result.is_err()));
  if !latest_failed || !latest_ready {
    data.settle_latest_rows(latest_rows);
  }
  data.reconcile_hero_selection();
  true
}

/// Leaving Home revokes only its own image demand and invalidates metadata work.
pub(crate) fn leave_view(surface: &mut Surface, kernel: &mut Kernel) {
  surface.data.hovered_card = None;
  kernel.request_gate.begin_home();
  surface.artwork.clear();
}

#[derive(Clone, Copy)]
pub(crate) enum ArtworkPlacement {
  Hero,
  HeroBackdrop,
  Card(HomeSection),
  Selection(HomeSection),
}

impl ArtworkPlacement {
  pub(crate) fn key(self, item_id: &str) -> String {
    match self {
      Self::Hero => format!("home-logo:{item_id}"),
      Self::HeroBackdrop => format!("home-backdrop:{item_id}"),
      Self::Card(section) => format!("home-card:{}:{item_id}", section.index()),
      Self::Selection(section) => format!("home-selection:{}:{item_id}", section.index()),
    }
  }

  pub(crate) fn spec(self, item: &VideoLibraryItem) -> Option<ImageSpec> {
    Some(ImageSpec {
      key: self.key(&item.id),
      image_id: artwork_image_id(self, item)?.to_owned(),
      size_class: match self {
        Self::Hero => ArtworkSizeClass::Hero,
        Self::HeroBackdrop => ArtworkSizeClass::Backdrop,
        Self::Card(_) | Self::Selection(_) => ArtworkSizeClass::Card,
      },
      derived: DerivedArtwork {
        logo_shadow: matches!(self, Self::Hero),
      },
    })
  }
}

fn prepare_artwork(surface: &mut Surface) -> Task<Message> {
  surface.artwork.retain(&artwork_specs(&surface.data));
  Task::none()
}

fn artwork_specs(data: &HomeState) -> Vec<ImageSpec> {
  let mut specs = Vec::new();
  if let Some(item) = data.featured_item() {
    specs.extend(ArtworkPlacement::Hero.spec(item));
    specs.extend(ArtworkPlacement::HeroBackdrop.spec(item));
  }
  for row in data.rows() {
    if let jellypilot_core::LoadState::Ready(items) = &row.items {
      specs.extend(
        items
          .iter()
          .filter_map(|item| ArtworkPlacement::Card(row.section).spec(item)),
      );
    }
  }
  let mut candidates = data.hero_candidates().peekable();
  if let Some((section, item)) = candidates.next() {
    if candidates.peek().is_some() {
      specs.extend(ArtworkPlacement::Selection(section).spec(item));
      specs.extend(
        candidates.filter_map(|(section, item)| ArtworkPlacement::Selection(section).spec(item)),
      );
    }
  }
  specs
}

fn artwork_image_id(placement: ArtworkPlacement, item: &VideoLibraryItem) -> Option<&str> {
  match placement {
    ArtworkPlacement::Hero => item.logo_image_id.as_deref(),
    ArtworkPlacement::HeroBackdrop if item.item_type.eq_ignore_ascii_case("Episode") => item
      .series_backdrop_image_id
      .as_deref()
      .or(item.backdrop_image_id.as_deref()),
    ArtworkPlacement::HeroBackdrop => item.backdrop_image_id.as_deref(),
    ArtworkPlacement::Card(section) if section.is_action() => landscape_image_id(item),
    ArtworkPlacement::Selection(_) => landscape_image_id(item),
    ArtworkPlacement::Card(_) if item.item_type.eq_ignore_ascii_case("Episode") => item
      .season_poster_image_id
      .as_deref()
      .or(item.series_poster_image_id.as_deref()),
    ArtworkPlacement::Card(_) => item.artwork_image_id.as_deref(),
  }
}

pub(super) fn landscape_image_id(item: &VideoLibraryItem) -> Option<&str> {
  if item.item_type.eq_ignore_ascii_case("Episode") {
    item
      .artwork_image_id
      .as_deref()
      .or(item.episode_thumb_image_id.as_deref())
      .or(item.series_thumb_image_id.as_deref())
      .or(item.series_backdrop_image_id.as_deref())
  } else {
    item
      .backdrop_image_id
      .as_deref()
      .or(item.artwork_image_id.as_deref())
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use jellypilot_auth::login::ConnectionPhase;
  use jellypilot_auth::AuthStore;
  use jellypilot_core::config::SettingsStore;
  use jellypilot_core::request_gate::RequestGate;
  use jellypilot_media_server::JellyfinClient;

  use super::*;

  fn test_fixture() -> (Surface, Kernel) {
    let settings = SettingsStore::default();
    let kernel = Kernel {
      settings,
      locale: crate::i18n::Localizer::default(),
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
      avatar_adapter: Arc::new(jellypilot_media_server::artwork::ArtworkAdapter::new()),
      profile_avatars: Default::default(),
    };
    (Surface::default(), kernel)
  }

  fn episode(id: &str, season_number: i32) -> VideoLibraryItem {
    VideoLibraryItem {
      id: id.to_owned(),
      name: "Episode".to_owned(),
      item_type: "Episode".to_owned(),
      production_year: None,
      runtime_seconds: Some(1_800.0),
      played: false,
      favorite: false,
      artwork_image_id: None,
      logo_image_id: None,
      backdrop_image_id: None,
      series_poster_image_id: None,
      episode_thumb_image_id: None,
      series_thumb_image_id: None,
      series_backdrop_image_id: None,
      season_number: Some(season_number),
      episode_number: Some(1),
      series_id: Some("show-1".to_owned()),
      series_name: Some("Show".to_owned()),
      resume_position_seconds: None,
      played_percentage: None,
      overview: None,
      index_number_end: None,
      season_poster_image_id: None,
      end_year: None,
      series_continuing: false,
      unplayed_item_count: None,
    }
  }

  #[test]
  fn manual_hero_selection_survives_refresh() {
    let (mut surface, mut kernel) = test_fixture();
    kernel.client = Some(Arc::new(JellyfinClient::new()));
    let mut a = episode("a", 1);
    a.series_id = Some("series-a".to_owned());
    a.resume_position_seconds = Some(120.0);
    a.series_backdrop_image_id = Some("backdrop-a".to_owned());
    let mut b = episode("b", 1);
    b.series_id = Some("series-b".to_owned());
    b.resume_position_seconds = Some(360.0);
    b.series_backdrop_image_id = Some("backdrop-b".to_owned());
    surface
      .data
      .settle_video_home(Ok(jellypilot_media_server::VideoHome {
        continue_watching: vec![a.clone(), b.clone()],
        next_up: Vec::new(),
      }));

    drop(update(
      &mut surface,
      &mut kernel,
      HomeMessage::HeroSelected("b".to_owned()),
    ));
    assert_eq!(
      surface.data.featured_item().map(|item| item.id.as_str()),
      Some("b")
    );

    let token = kernel.request_gate.begin_home();
    assert!(settle(
      &mut surface.data,
      &mut kernel.request_gate,
      &mut kernel.diagnostics,
      token,
      (
        Ok(jellypilot_media_server::VideoHome {
          continue_watching: vec![b, a],
          next_up: Vec::new(),
        }),
        Ok(Vec::new()),
        Ok(Vec::new()),
      ),
    ));
    assert_eq!(
      surface.data.featured_item().map(|item| item.id.as_str()),
      Some("b")
    );
    assert!(matches!(
      &surface.data.rows()[HomeSection::ContinueWatching.index()].items,
      jellypilot_core::LoadState::Ready(items) if items.iter().any(|item| item.id == "a")
    ));
  }

  #[test]
  fn selected_identity_survives_moving_between_home_sources_in_one_response() {
    let mut home = HomeState::default();
    let mut gate = RequestGate::default();
    let mut a = episode("a", 1);
    a.item_type = "Movie".to_owned();
    let mut b = episode("b", 1);
    b.item_type = "Movie".to_owned();
    b.resume_position_seconds = Some(120.0);
    let mut c = episode("c", 1);
    c.item_type = "Movie".to_owned();
    let latest = |items| {
      vec![jellypilot_media_server::LibraryLatestRow {
        library_id: "movies".to_owned(),
        library_name: "Movies".to_owned(),
        result: Ok(items),
      }]
    };
    let token = gate.begin_home();
    assert!(settle(
      &mut home,
      &mut gate,
      &mut Diagnostics::default(),
      token,
      (
        Ok(jellypilot_media_server::VideoHome {
          continue_watching: vec![b.clone()],
          next_up: Vec::new(),
        }),
        Ok(Vec::new()),
        Ok(latest(vec![a.clone()])),
      )
    ));
    home.select_hero("b").expect("resumable candidate");
    let token = gate.begin_home();
    assert!(settle(
      &mut home,
      &mut gate,
      &mut Diagnostics::default(),
      token,
      (
        Ok(jellypilot_media_server::VideoHome {
          continue_watching: Vec::new(),
          next_up: Vec::new(),
        }),
        Ok(Vec::new()),
        Ok(latest(vec![c.clone(), b, a.clone()])),
      )
    ));
    assert_eq!(home.featured_item().map(|item| item.id.as_str()), Some("b"));

    let token = gate.begin_home();
    assert!(settle(
      &mut home,
      &mut gate,
      &mut Diagnostics::default(),
      token,
      (
        Ok(jellypilot_media_server::VideoHome {
          continue_watching: Vec::new(),
          next_up: Vec::new(),
        }),
        Ok(Vec::new()),
        Ok(latest(vec![c, a])),
      )
    ));
    assert_eq!(home.featured_item().map(|item| item.id.as_str()), Some("c"));
  }

  #[test]
  fn refreshing_failed_home_rows_retains_an_already_loaded_directory() {
    let (mut surface, mut kernel) = test_fixture();
    surface
      .data
      .settle_video_home(Err("unavailable rows".to_owned()));
    surface
      .data
      .settle_shortcuts(Ok(vec![jellypilot_media_server::VideoLibraryShortcut {
        id: "movies".to_owned(),
        name: "Movies".to_owned(),
        collection_type: "movies".to_owned(),
        item_count: Some(3),
        artwork_image_id: None,
      }]));
    assert!(!surface.data.has_ready_content());
    drop(start_load(&mut surface, &mut kernel));
    assert!(
      matches!(&surface.data.shortcuts, jellypilot_core::LoadState::Ready(shortcuts)
      if shortcuts.len() == 1 && shortcuts[0].id == "movies")
    );
  }

  #[test]
  fn failed_refresh_keeps_usable_home_rows_and_directory() {
    let mut home = HomeState::default();
    let mut gate = RequestGate::default();
    home.settle_video_home(Ok(jellypilot_media_server::VideoHome {
      continue_watching: vec![episode("kept", 1)],
      next_up: Vec::new(),
    }));
    home.settle_shortcuts(Ok(Vec::new()));
    home.settle_latest_rows(Ok(vec![jellypilot_media_server::LibraryLatestRow {
      library_id: "movies".to_owned(),
      library_name: "Movies".to_owned(),
      result: Ok(vec![episode("latest", 1)]),
    }]));
    let token = gate.begin_home();
    assert!(settle(
      &mut home,
      &mut gate,
      &mut Diagnostics::default(),
      token,
      (
        Err("offline".to_owned()),
        Err("offline".to_owned()),
        Err("offline".to_owned()),
      )
    ));
    assert!(
      matches!(&home.rows()[0].items, jellypilot_core::LoadState::Ready(items) if items[0].id == "kept")
    );
    assert!(
      matches!(&home.rows()[2].items, jellypilot_core::LoadState::Ready(items) if items[0].id == "latest")
    );
    assert!(matches!(
      home.shortcuts,
      jellypilot_core::LoadState::Ready(_)
    ));
  }

  #[test]
  fn stale_home_settlement_does_not_replace_the_current_loading_state() {
    let mut home = HomeState::default();
    let mut gate = RequestGate::default();
    let stale = gate.begin_home();
    let _current = gate.begin_home();
    home.begin_load();

    let applied = settle(
      &mut home,
      &mut gate,
      &mut Diagnostics::default(),
      stale,
      (
        Err("stale home".to_owned()),
        Err("stale shortcuts".to_owned()),
        Err("stale latest rows".to_owned()),
      ),
    );

    assert!(matches!(
      (applied, &home.rows()[0].items, &home.shortcuts),
      (
        false,
        jellypilot_core::LoadState::Loading,
        jellypilot_core::LoadState::Loading
      )
    ));
  }

  #[test]
  fn settle_consumes_latest_rows_in_server_order() {
    let mut home = HomeState::default();
    let mut gate = RequestGate::default();
    let token = gate.begin_home();

    assert!(settle(
      &mut home,
      &mut gate,
      &mut Diagnostics::default(),
      token,
      (
        Ok(jellypilot_media_server::VideoHome {
          continue_watching: Vec::new(),
          next_up: Vec::new(),
        }),
        Ok(Vec::new()),
        Ok(vec![
          jellypilot_media_server::LibraryLatestRow {
            library_id: "movies".to_owned(),
            library_name: "Movies".to_owned(),
            result: Ok(vec![episode("movie-item", 1)]),
          },
          jellypilot_media_server::LibraryLatestRow {
            library_id: "shows".to_owned(),
            library_name: "Shows".to_owned(),
            result: Ok(vec![episode("show-item", 1)]),
          },
        ]),
      ),
    ));
    assert_eq!(
      home
        .rows()
        .iter()
        .skip(2)
        .map(|row| match &row.items {
          jellypilot_core::LoadState::Ready(items) => items[0].id.as_str(),
          _ => panic!("latest row must be ready"),
        })
        .collect::<Vec<_>>(),
      vec!["movie-item", "show-item"]
    );
  }

  #[test]
  fn episode_action_card_artwork_prefers_primary_then_episode_and_series_fallbacks() {
    let mut item = episode("episode-art", 1);
    item.artwork_image_id = Some("episode-primary".to_owned());
    item.episode_thumb_image_id = Some("episode-thumb".to_owned());
    item.series_thumb_image_id = Some("series-thumb".to_owned());
    item.series_backdrop_image_id = Some("series-backdrop".to_owned());

    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::ContinueWatching), &item,),
      Some("episode-primary")
    );
    item.artwork_image_id = None;
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::NextUp), &item),
      Some("episode-thumb")
    );
    item.episode_thumb_image_id = None;
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::NextUp), &item),
      Some("series-thumb")
    );
    item.series_thumb_image_id = None;
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::NextUp), &item),
      Some("series-backdrop")
    );
    item.series_backdrop_image_id = None;
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::NextUp), &item),
      None
    );
  }

  #[test]
  fn card_artwork_selection_keeps_latest_and_non_episode_action_fallbacks() {
    let mut item = episode("episode-art", 1);
    item.season_poster_image_id = Some("season-poster".to_owned());
    item.series_poster_image_id = Some("series-poster".to_owned());

    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::Latest(0)), &item),
      Some("season-poster")
    );
    item.season_poster_image_id = None;
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::Latest(0)), &item),
      Some("series-poster")
    );
    item.series_poster_image_id = None;
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::Latest(0)), &item),
      None
    );

    item.item_type = "Movie".to_owned();
    item.backdrop_image_id = Some("movie-backdrop".to_owned());
    item.artwork_image_id = Some("movie-poster".to_owned());
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::ContinueWatching), &item,),
      Some("movie-backdrop")
    );
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::Latest(0)), &item),
      Some("movie-poster")
    );

    item.item_type = "Series".to_owned();
    assert_eq!(
      artwork_image_id(ArtworkPlacement::Card(HomeSection::Latest(0)), &item),
      Some("movie-poster")
    );
  }
}
