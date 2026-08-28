use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::task;
use iced::widget::image;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_auth::{AuthStore, SavedProfileKey, SavedProfileSummary, SensitiveSavedSession};
use jellypilot_core::artwork_binder::{ArtworkBinder, ArtworkSlot};
use jellypilot_core::browse_model::{BrowseModel, LibraryBrowseView};
use jellypilot_core::config::{LoginPrefill, Settings, SettingsStore};
use jellypilot_core::request_gate::RequestGate;
use jellypilot_core::{LibraryBrowseLoadToken, LoadState};
use jellypilot_media_server::artwork::{ArtworkAdapter, RawArtworkDecoder};
use jellypilot_media_server::{
  JellyfinClient, MediaServerProvider, VideoLibraryItem, VideoLibraryShortcut,
};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginMethod {
  QuickConnect,
  Password,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum QuickConnectState {
  #[default]
  Idle,
  Requesting,
  Waiting(String),
  Approving,
  Failed,
}

pub struct LoginState {
  pub provider: MediaServerProvider,
  pub method: LoginMethod,
  pub server_url: String,
  pub username: String,
  pub password: Zeroizing<String>,
  pub remember: bool,
  pub quick_connect: QuickConnectState,
  pub profiles: Vec<SavedProfileSummary>,
  pub profiles_loading: bool,
  pub profiles_revision: u64,
  pub busy_profile: Option<SavedProfileKey>,
  pub forget_confirmation: Option<SavedProfileKey>,
  pub error: Option<String>,
}

impl LoginState {
  pub fn from_settings(settings: &Settings) -> Self {
    let provider = if settings.login_provider().eq_ignore_ascii_case("emby") {
      MediaServerProvider::Emby
    } else {
      MediaServerProvider::Jellyfin
    };
    let mut state = Self {
      provider,
      method: LoginMethod::QuickConnect,
      server_url: String::new(),
      username: String::new(),
      password: Zeroizing::new(String::new()),
      remember: settings.remembers_login_prefill(),
      quick_connect: QuickConnectState::Idle,
      profiles: Vec::new(),
      profiles_loading: true,
      profiles_revision: 0,
      busy_profile: None,
      forget_confirmation: None,
      error: None,
    };
    state.force_supported_method();
    if settings.remembers_login_prefill() {
      state.apply_prefill(Some(settings.login_prefill()));
    }
    state
  }

  pub fn apply_prefill(&mut self, prefill: Option<LoginPrefill>) {
    if let Some(prefill) = prefill {
      self.server_url = prefill.server_url().to_owned();
      self.username = prefill.username().to_owned();
      self.remember = true;
    } else {
      self.clear_prefill();
    }
  }

  pub fn clear_prefill(&mut self) {
    self.server_url.clear();
    self.username.clear();
    self.remember = false;
  }

  pub fn select_provider(&mut self, provider: MediaServerProvider) {
    self.provider = provider;
    self.force_supported_method();
    self.reset_quick_connect();
  }

  pub fn force_supported_method(&mut self) {
    if self.provider == MediaServerProvider::Emby {
      self.method = LoginMethod::Password;
    }
  }

  pub fn reset_quick_connect(&mut self) {
    self.quick_connect = QuickConnectState::Idle;
  }
}

#[derive(Clone)]
pub struct ConnectedIdentity {
  pub user_name: String,
  pub server: String,
}

impl ConnectedIdentity {
  pub fn from_session(session: &SensitiveSavedSession) -> Self {
    Self {
      user_name: session.user_name.clone(),
      server: session
        .server_name
        .clone()
        .unwrap_or_else(|| session.server_url.clone()),
    }
  }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Destination {
  #[default]
  Home,
  Library {
    library_id: String,
    collection_type: String,
  },
  Search(String),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HomeSection {
  ContinueWatching,
  LatestMovies,
  NextUp,
  LatestEpisodes,
}

impl HomeSection {
  pub const ALL: [Self; 4] = [
    Self::ContinueWatching,
    Self::LatestMovies,
    Self::NextUp,
    Self::LatestEpisodes,
  ];

  pub const fn title(self) -> &'static str {
    match self {
      Self::ContinueWatching => "Continue Watching",
      Self::LatestMovies => "Latest Movies",
      Self::NextUp => "Next Up",
      Self::LatestEpisodes => "Latest Episodes",
    }
  }

  const fn index(self) -> usize {
    match self {
      Self::ContinueWatching => 0,
      Self::LatestMovies => 1,
      Self::NextUp => 2,
      Self::LatestEpisodes => 3,
    }
  }
}

#[derive(Default)]
pub struct HomeState {
  pub continue_watching: LoadState<Vec<VideoLibraryItem>>,
  pub latest_movies: LoadState<Vec<VideoLibraryItem>>,
  pub next_up: LoadState<Vec<VideoLibraryItem>>,
  pub latest_episodes: LoadState<Vec<VideoLibraryItem>>,
  pub shortcuts: LoadState<Vec<VideoLibraryShortcut>>,
}

impl HomeState {
  pub fn begin_load(&mut self) {
    self.continue_watching = LoadState::Loading;
    self.latest_movies = LoadState::Loading;
    self.next_up = LoadState::Loading;
    self.latest_episodes = LoadState::Loading;
    self.shortcuts = LoadState::Loading;
  }

  pub fn settle_video_home(&mut self, result: Result<jellypilot_media_server::VideoHome, String>) {
    match result {
      Ok(home) => {
        self.continue_watching = LoadState::Ready(home.continue_watching);
        self.latest_movies = LoadState::Ready(home.latest_movies);
        self.next_up = LoadState::Ready(home.next_up);
        self.latest_episodes = LoadState::Ready(home.latest_episodes);
      }
      Err(error) => {
        self.continue_watching = LoadState::Failed(error.clone());
        self.latest_movies = LoadState::Failed(error.clone());
        self.next_up = LoadState::Failed(error.clone());
        self.latest_episodes = LoadState::Failed(error);
      }
    }
  }

  pub fn settle_shortcuts(&mut self, result: Result<Vec<VideoLibraryShortcut>, String>) {
    self.shortcuts = match result {
      Ok(shortcuts) => LoadState::Ready(shortcuts),
      Err(error) => LoadState::Failed(error),
    };
  }

  pub fn section(&self, section: HomeSection) -> &LoadState<Vec<VideoLibraryItem>> {
    match section {
      HomeSection::ContinueWatching => &self.continue_watching,
      HomeSection::LatestMovies => &self.latest_movies,
      HomeSection::NextUp => &self.next_up,
      HomeSection::LatestEpisodes => &self.latest_episodes,
    }
  }

  pub fn featured_item(&self) -> Option<&VideoLibraryItem> {
    ready_items(&self.continue_watching)
      .and_then(|items| items.iter().find(|item| has_resume_position(item)))
      .or_else(|| ready_items(&self.next_up).and_then(|items| items.first()))
      .or_else(|| ready_items(&self.latest_movies).and_then(|items| items.first()))
  }
}

fn ready_items(state: &LoadState<Vec<VideoLibraryItem>>) -> Option<&[VideoLibraryItem]> {
  match state {
    LoadState::Ready(items) => Some(items),
    LoadState::Idle | LoadState::Loading | LoadState::Failed(_) => None,
  }
}

pub fn has_resume_position(item: &VideoLibraryItem) -> bool {
  item.resume_position_seconds.is_some_and(|position| {
    position.is_finite()
      && position > 0.0
      && item
        .runtime_seconds
        .is_none_or(|runtime| !runtime.is_finite() || runtime <= 0.0 || position < runtime)
  })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkCellState {
  Loading,
  Ready,
  Failed,
}

#[derive(Clone)]
pub struct ArtworkCell {
  pub slot: ArtworkSlot,
  pub image_id: String,
  pub state: ArtworkCellState,
}

#[derive(Default)]
pub struct HomeArtwork {
  hero: Option<(String, ArtworkCell)>,
  sections: [HashMap<String, ArtworkCell>; 4],
}

impl HomeArtwork {
  pub fn clear(&mut self) {
    self.hero = None;
    for section in &mut self.sections {
      section.clear();
    }
  }

  pub fn insert_hero(&mut self, item_id: String, cell: ArtworkCell) {
    self.hero = Some((item_id, cell));
  }

  pub fn insert_card(&mut self, section: HomeSection, item_id: String, cell: ArtworkCell) {
    self.sections[section.index()].insert(item_id, cell);
  }

  pub fn hero(&self, item_id: &str) -> Option<&ArtworkCell> {
    self
      .hero
      .as_ref()
      .filter(|(bound_item_id, _)| bound_item_id == item_id)
      .map(|(_, cell)| cell)
  }

  pub fn card(&self, section: HomeSection, item_id: &str) -> Option<&ArtworkCell> {
    self.sections[section.index()].get(item_id)
  }

  pub fn cell_mut(&mut self, slot: ArtworkSlot, image_id: &str) -> Option<&mut ArtworkCell> {
    if let Some((_, cell)) = &mut self.hero {
      if cell.slot == slot && cell.image_id == image_id {
        return Some(cell);
      }
    }
    self
      .sections
      .iter_mut()
      .flat_map(HashMap::values_mut)
      .find(|cell| cell.slot == slot && cell.image_id == image_id)
  }
}

#[derive(Default)]
pub struct BrowseArtwork {
  cells: HashMap<String, ArtworkCell>,
}

impl BrowseArtwork {
  pub fn clear(&mut self) {
    self.cells.clear();
  }

  pub fn insert(&mut self, item_id: String, cell: ArtworkCell) {
    self.cells.insert(item_id, cell);
  }

  pub fn get(&self, item_id: &str) -> Option<&ArtworkCell> {
    self.cells.get(item_id)
  }

  pub fn cell_mut(&mut self, slot: ArtworkSlot, image_id: &str) -> Option<&mut ArtworkCell> {
    self
      .cells
      .values_mut()
      .find(|cell| cell.slot == slot && cell.image_id == image_id)
  }

  pub fn retain_items(&mut self, item_ids: &HashSet<&str>) {
    self
      .cells
      .retain(|item_id, _| item_ids.contains(item_id.as_str()));
  }

  pub fn slots(&self) -> impl Iterator<Item = ArtworkSlot> + '_ {
    self.cells.values().map(|cell| cell.slot)
  }
}

#[derive(Clone, Copy, Debug)]
pub struct BrowseViewport {
  pub offset_y: f32,
  pub height: f32,
  pub content_height: f32,
  pub width: f32,
}

impl Default for BrowseViewport {
  fn default() -> Self {
    Self {
      offset_y: 0.0,
      height: 720.0,
      content_height: 0.0,
      width: 960.0,
    }
  }
}

struct RetainedHandle<T> {
  image_id: String,
  value: T,
}

pub struct HandleRetention<T> {
  entries: HashMap<ArtworkSlot, RetainedHandle<T>>,
}

impl<T> Default for HandleRetention<T> {
  fn default() -> Self {
    Self {
      entries: HashMap::new(),
    }
  }
}

impl<T> HandleRetention<T> {
  pub fn insert(&mut self, slot: ArtworkSlot, image_id: String, value: T) {
    self
      .entries
      .insert(slot, RetainedHandle { image_id, value });
  }

  pub fn get(&self, slot: ArtworkSlot, image_id: &str) -> Option<&T> {
    self
      .entries
      .get(&slot)
      .filter(|entry| entry.image_id == image_id)
      .map(|entry| &entry.value)
  }

  pub fn retain_slots(&mut self, slots: impl IntoIterator<Item = ArtworkSlot>) {
    let slots: HashSet<_> = slots.into_iter().collect();
    self.entries.retain(|slot, _| slots.contains(slot));
  }

  pub fn clear(&mut self) {
    self.entries.clear();
  }

  #[cfg(test)]
  fn len(&self) -> usize {
    self.entries.len()
  }
}

pub type ArtworkHandleRetention = HandleRetention<image::Handle>;

pub struct State {
  pub smoke: bool,
  pub settings: SettingsStore,
  pub auth_store: AuthStore,
  pub request_gate: RequestGate,
  pub client: Option<Arc<JellyfinClient>>,
  pub connection: ConnectionPhase,
  pub login: LoginState,
  pub connected_identity: Option<ConnectedIdentity>,
  pub active_profile: Option<SavedProfileKey>,
  pub quick_connect_task: Option<task::Handle>,
  pub notice: Option<String>,
  pub destination: Destination,
  pub home: HomeState,
  pub artwork_adapter: Arc<ArtworkAdapter<RawArtworkDecoder>>,
  pub artwork_binder: ArtworkBinder,
  pub home_artwork: HomeArtwork,
  pub artwork_handles: ArtworkHandleRetention,
  pub browse: BrowseModel,
  pub browse_view: LibraryBrowseView,
  pub browse_artwork: BrowseArtwork,
  pub browse_page_tasks: HashMap<LibraryBrowseLoadToken, task::Handle>,
  pub browse_viewport: BrowseViewport,
  pub browse_scroll_id: iced::widget::Id,
  pub browse_sort_menu_open: bool,
  pub search_input: String,
}

impl State {
  pub fn boot(smoke: bool) -> Self {
    let (settings, settings_error) = match SettingsStore::load() {
      Ok(settings) => (settings, None),
      Err(error) => (
        SettingsStore::default(),
        Some(format!("Could not load saved settings: {error}")),
      ),
    };
    let mut login = LoginState::from_settings(settings.snapshot());
    login.error = settings_error;

    Self {
      smoke,
      settings,
      auth_store: AuthStore::default(),
      request_gate: RequestGate::default(),
      client: None,
      connection: ConnectionPhase::SignedOut,
      login,
      connected_identity: None,
      active_profile: None,
      quick_connect_task: None,
      notice: None,
      destination: Destination::Home,
      home: HomeState::default(),
      artwork_adapter: Arc::new(ArtworkAdapter::new()),
      artwork_binder: ArtworkBinder::default(),
      home_artwork: HomeArtwork::default(),
      artwork_handles: ArtworkHandleRetention::default(),
      browse: BrowseModel::default(),
      browse_view: LibraryBrowseView::Inactive,
      browse_artwork: BrowseArtwork::default(),
      browse_page_tasks: HashMap::new(),
      browse_viewport: BrowseViewport::default(),
      browse_scroll_id: iced::widget::Id::unique(),
      browse_sort_menu_open: false,
      search_input: String::new(),
    }
  }
}

#[cfg(test)]
mod tests {
  use jellypilot_core::artwork_binder::ArtworkSurface;

  use super::*;

  #[test]
  fn handle_retention_evicts_slots_outside_the_current_window() {
    let mut binder = ArtworkBinder::default();
    let retained_slot = binder.bind(ArtworkSurface::Home);
    let evicted_slot = binder.bind(ArtworkSurface::Home);
    let mut handles = HandleRetention::default();
    handles.insert(retained_slot, "retained".to_owned(), 1_u8);
    handles.insert(evicted_slot, "evicted".to_owned(), 2_u8);

    handles.retain_slots([retained_slot]);

    assert_eq!(
      (
        handles.len(),
        handles.get(retained_slot, "retained"),
        handles.get(evicted_slot, "evicted"),
      ),
      (1, Some(&1), None)
    );
  }

  #[test]
  fn handle_retention_rejects_a_reused_slot_with_the_wrong_image_id() {
    let mut binder = ArtworkBinder::default();
    let slot = binder.bind(ArtworkSurface::Home);
    let mut handles = HandleRetention::default();
    handles.insert(slot, "current".to_owned(), 1_u8);

    assert!(handles.get(slot, "stale").is_none());
  }

  #[test]
  fn home_sections_transition_from_loading_to_independent_results() {
    let mut home = HomeState::default();
    home.begin_load();
    home.settle_video_home(Err("home failed".to_owned()));
    home.settle_shortcuts(Ok(Vec::new()));

    assert!(matches!(
      (
        &home.continue_watching,
        &home.latest_movies,
        &home.next_up,
        &home.latest_episodes,
        &home.shortcuts,
      ),
      (
        LoadState::Failed(_),
        LoadState::Failed(_),
        LoadState::Failed(_),
        LoadState::Failed(_),
        LoadState::Ready(shortcuts),
      ) if shortcuts.is_empty()
    ));
  }

  #[test]
  fn home_sections_settle_ready_even_when_every_section_is_empty() {
    let mut home = HomeState::default();
    home.begin_load();
    home.settle_video_home(Ok(jellypilot_media_server::VideoHome {
      continue_watching: Vec::new(),
      next_up: Vec::new(),
      latest_movies: Vec::new(),
      latest_episodes: Vec::new(),
    }));

    assert!(HomeSection::ALL.iter().all(
      |section| matches!(home.section(*section), LoadState::Ready(items) if items.is_empty())
    ));
  }
}
