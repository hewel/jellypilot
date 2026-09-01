use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use iced::widget::image;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_auth::{AuthStore, SavedProfileKey, SavedProfileSummary, SensitiveSavedSession};
use jellypilot_core::artwork_binder::{ArtworkBinder, ArtworkSlot};
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_core::config::{
  AppMode, IntroMode, LoginPrefill, Settings, SettingsStore, ShortcutKind, ThemeMode,
};
use jellypilot_core::detail::DetailContent;
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel, Diagnostics};
use jellypilot_core::request_gate::{RemoteToken, RequestGate};
use jellypilot_core::LoadState;
use jellypilot_media_server::artwork::ArtworkAdapter;
use jellypilot_media_server::{
  MediaServerProvider, VideoLibraryItem, VideoLibraryShortcut, VideoSeasonEpisodesPage,
};
use jellypilot_mpv::playback::PlaybackController;
use jellypilot_session::{IntroSkipMode, JellyfinWebSocket, JellyfinWebSocketEvent};
use jellypilot_ui::theme::ThemeMode as UiThemeMode;
use jellypilot_ui::tokens::{ThemePalette, DARK_PALETTE, LIGHT_PALETTE};
use zeroize::Zeroizing;

use super::kernel::Kernel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeLevel {
  Warning,
  Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastNotice {
  pub id: u64,
  pub message: String,
  pub level: NoticeLevel,
}

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
  Detail(String),
  /// Full-window Now Playing; the Control-Only root destination, unused in
  /// Full mode where the player is a bar above the shell content.
  NowPlaying,
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

  pub const fn index(self) -> usize {
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
  pub hovered_card: Option<String>,
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

  pub fn has_ready_content(&self) -> bool {
    matches!(self.continue_watching, LoadState::Ready(_))
      || matches!(self.latest_movies, LoadState::Ready(_))
      || matches!(self.next_up, LoadState::Ready(_))
      || matches!(self.latest_episodes, LoadState::Ready(_))
  }
}

fn ready_items(state: &LoadState<Vec<VideoLibraryItem>>) -> Option<&[VideoLibraryItem]> {
  match state {
    LoadState::Ready(items) => Some(items),
    LoadState::Idle | LoadState::Loading | LoadState::Failed(_) => None,
  }
}

pub fn has_resume_position(item: &VideoLibraryItem) -> bool {
  !item.played
    && item.resume_position_seconds.is_some_and(|position| {
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

#[derive(Clone, Debug, PartialEq)]
pub struct ArtworkCell {
  pub slot: ArtworkSlot,
  pub image_id: String,
  pub state: ArtworkCellState,
}

#[derive(Default)]
pub struct HomeArtwork {
  hero: Option<(String, ArtworkCell)>,
  hero_backdrop: Option<(String, ArtworkCell)>,
  sections: [HashMap<String, ArtworkCell>; 4],
}

impl HomeArtwork {
  pub fn insert_hero(&mut self, item_id: String, cell: ArtworkCell) {
    self.hero = Some((item_id, cell));
  }
  pub fn insert_hero_backdrop(&mut self, item_id: String, cell: ArtworkCell) {
    self.hero_backdrop = Some((item_id, cell));
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
  pub fn hero_backdrop(&self, item_id: &str) -> Option<&ArtworkCell> {
    self
      .hero_backdrop
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
    if let Some((_, cell)) = &mut self.hero_backdrop {
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

  pub fn slots(&self) -> impl Iterator<Item = ArtworkSlot> + '_ {
    self
      .hero
      .as_ref()
      .map(|(_, cell)| cell.slot)
      .into_iter()
      .chain(self.hero_backdrop.as_ref().map(|(_, cell)| cell.slot))
      .chain(
        self
          .sections
          .iter()
          .flat_map(HashMap::values)
          .map(|cell| cell.slot),
      )
  }

  pub fn retain_items(
    &mut self,
    hero_item_id: Option<&str>,
    hero_backdrop_item_id: Option<&str>,
    section_item_ids: &[HashSet<&str>; 4],
  ) {
    if let Some((bound_item_id, _)) = &self.hero {
      if hero_item_id != Some(bound_item_id.as_str()) {
        self.hero = None;
      }
    }
    if let Some((bound_item_id, _)) = &self.hero_backdrop {
      if hero_backdrop_item_id != Some(bound_item_id.as_str()) {
        self.hero_backdrop = None;
      }
    }
    for (i, section) in self.sections.iter_mut().enumerate() {
      let allowed = &section_item_ids[i];
      section.retain(|item_id, _| allowed.contains(item_id.as_str()));
    }
  }

  pub fn prune_unready(&mut self) {
    if let Some((_, cell)) = &self.hero {
      if cell.state != ArtworkCellState::Ready {
        self.hero = None;
      }
    }
    if let Some((_, cell)) = &self.hero_backdrop {
      if cell.state != ArtworkCellState::Ready {
        self.hero_backdrop = None;
      }
    }
    for section in &mut self.sections {
      section.retain(|_, cell| cell.state == ArtworkCellState::Ready);
    }
  }

  pub fn has_loading(&self) -> bool {
    self
      .hero
      .as_ref()
      .is_some_and(|(_, cell)| cell.state == ArtworkCellState::Loading)
      || self
        .hero_backdrop
        .as_ref()
        .is_some_and(|(_, cell)| cell.state == ArtworkCellState::Loading)
      || self
        .sections
        .iter()
        .flat_map(HashMap::values)
        .any(|cell| cell.state == ArtworkCellState::Loading)
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

  pub fn has_loading(&self) -> bool {
    self
      .cells
      .values()
      .any(|cell| cell.state == ArtworkCellState::Loading)
  }
}
#[derive(Default)]
pub struct DetailArtwork {
  cells: HashMap<String, ArtworkCell>,
}

impl DetailArtwork {
  pub fn clear(&mut self) {
    self.cells.clear();
  }

  pub fn insert(&mut self, key: String, cell: ArtworkCell) {
    self.cells.insert(key, cell);
  }

  pub fn get(&self, key: &str) -> Option<&ArtworkCell> {
    self.cells.get(key)
  }

  pub fn cell_mut(&mut self, slot: ArtworkSlot, image_id: &str) -> Option<&mut ArtworkCell> {
    self
      .cells
      .values_mut()
      .find(|cell| cell.slot == slot && cell.image_id == image_id)
  }

  pub fn retain_keys(&mut self, keys: &HashSet<&str>) {
    self.cells.retain(|key, _| keys.contains(key.as_str()));
  }

  pub fn slots(&self) -> impl Iterator<Item = ArtworkSlot> + '_ {
    self.cells.values().map(|cell| cell.slot)
  }

  pub fn has_loading(&self) -> bool {
    self
      .cells
      .values()
      .any(|cell| cell.state == ArtworkCellState::Loading)
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserDataActionKind {
  Favorite,
  Played,
}

#[derive(Default)]
pub struct DetailState {
  pub content: LoadState<DetailContent>,
  pub season_neighbors: LoadState<Vec<VideoLibraryItem>>,
  pub season_episodes: LoadState<VideoSeasonEpisodesPage>,
  pub selected_season_id: Option<String>,
  pub overview_expanded: bool,
  pub user_data_busy: Option<UserDataActionKind>,
  pub user_data_error: Option<String>,
}

impl DetailState {
  pub fn clear(&mut self) {
    *self = Self::default();
  }
}

#[derive(Clone, Copy, Debug)]
pub struct BrowseViewport {
  pub offset_y: f32,
  pub height: f32,
}

impl Default for BrowseViewport {
  fn default() -> Self {
    Self {
      offset_y: 0.0,
      height: 720.0,
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

  pub fn remove(&mut self, slot: ArtworkSlot) {
    self.entries.remove(&slot);
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
pub type PlaybackControllerHandle = Arc<Mutex<PlaybackController>>;
#[derive(Clone)]
pub struct RemoteSessionHandle {
  pub websocket: Arc<JellyfinWebSocket>,
  pub lifecycle: Arc<Mutex<()>>,
}

#[derive(Clone)]
pub struct RemoteEventChannel {
  pub remote: RemoteToken,
  pub receiver: Arc<Mutex<mpsc::UnboundedReceiver<JellyfinWebSocketEvent>>>,
}

impl Hash for RemoteEventChannel {
  fn hash<H: Hasher>(&self, state: &mut H) {
    Arc::as_ptr(&self.receiver).hash(state);
  }
}

pub fn intro_skip_mode(mode: IntroMode) -> IntroSkipMode {
  match mode {
    IntroMode::Automatic => IntroSkipMode::Automatic,
    IntroMode::Manual => IntroSkipMode::Manual,
    IntroMode::Off => IntroSkipMode::Off,
  }
}

pub struct SettingsState {
  pub mpv_path_input: String,
  pub mpv_args_input: String,
  pub playback_target_name_input: String,
  pub intro_menu_open: bool,
  pub subtitle_menu_open: bool,
  pub diagnostic_level_menu_open: bool,
  pub diagnostic_category_menu_open: bool,
  pub diagnostic_level: Option<DiagnosticLevel>,
  pub diagnostic_category: Option<DiagnosticCategory>,
  pub shortcut_capture: Option<ShortcutKind>,
  pub error: Option<&'static str>,
  pub saved: Option<&'static str>,
}

impl SettingsState {
  pub fn from_settings(settings: &Settings) -> Self {
    Self {
      mpv_path_input: settings.mpv_path().unwrap_or_default().to_owned(),
      mpv_args_input: settings.mpv_args().join(" "),
      playback_target_name_input: settings
        .playback_target_name()
        .unwrap_or_default()
        .to_owned(),
      intro_menu_open: false,
      subtitle_menu_open: false,
      diagnostic_level_menu_open: false,
      diagnostic_category_menu_open: false,
      diagnostic_level: None,
      diagnostic_category: None,
      shortcut_capture: None,
      error: None,
      saved: None,
    }
  }
}

pub fn diagnostic_matches(
  level_filter: Option<DiagnosticLevel>,
  category_filter: Option<DiagnosticCategory>,
  level: DiagnosticLevel,
  category: DiagnosticCategory,
) -> bool {
  level_filter.is_none_or(|filter| filter == level)
    && category_filter.is_none_or(|filter| filter == category)
}

pub struct State {
  pub kernel: Kernel,
  /// Latest OS light/dark mode report; `None` until the boot one-shot task
  /// resolves. Read only while the theme mode setting is `System`.
  pub system_theme: iced::theme::Mode,
  pub login: crate::app::login::Surface,
  pub settings: crate::app::settings::Surface,
  pub home: crate::app::home::Surface,
  pub detail: crate::app::detail::Surface,
  pub browse: crate::app::browse::Surface,
  pub playback: crate::app::playback::Surface,
  pub shell: crate::app::shell::Surface,
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
    login.error = settings_error.clone();
    let settings_view = SettingsState::from_settings(settings.snapshot());
    let mut diagnostics = Diagnostics::default();
    if let Some(error) = &settings_error {
      diagnostics.record(DiagnosticLevel::Error, DiagnosticCategory::Config, error);
    }
    let mut request_gate = RequestGate::default();
    let playback = crate::app::playback::Surface::new(&mut request_gate);
    let artwork_adapter = Arc::new(ArtworkAdapter::new());
    artwork_adapter.set_disk_cache_enabled(settings.snapshot().image_cache_enabled());

    let mut state = Self {
      system_theme: iced::theme::Mode::None,
      kernel: Kernel {
        settings,
        diagnostics,
        auth_store: AuthStore::default(),
        request_gate,
        client: None,
        connection: ConnectionPhase::SignedOut,
        connected_identity: None,
        active_profile: None,

        notice: None,
        active_toast: None,
        next_toast_id: 0,
        tray: None,
        artwork_adapter,
        artwork_binder: ArtworkBinder::default(),
        artwork_handles: ArtworkHandleRetention::default(),
      },
      login: crate::app::login::Surface {
        flow: login,
        quick_connect_task: None,
      },
      settings: crate::app::settings::Surface {
        view: settings_view,
      },
      home: crate::app::home::Surface::default(),
      detail: crate::app::detail::Surface::default(),

      browse: crate::app::browse::Surface::default(),
      playback,
      shell: crate::app::shell::Surface::new(smoke),
    };
    // Control-Only boots straight into the full-window Now Playing root; the
    // Library Browser destinations stay unreachable (router guard).
    if state.app_mode() == AppMode::ControlOnly {
      state.shell.destination = Destination::NowPlaying;
    }
    state
  }
  pub fn all_artwork_slots(&self) -> impl Iterator<Item = ArtworkSlot> + '_ {
    self
      .home
      .artwork
      .slots()
      .chain(self.browse.artwork.slots())
      .chain(self.detail.artwork.slots())
      .chain(self.playback.artwork.as_ref().map(|cell| cell.slot))
  }

  /// True while any surface renders skeleton placeholders or while any artwork
  /// cell is loading. Gates the frame subscription that advances `skeleton_phase`,
  /// so the shell never redraws at display refresh just for an invisible animation.
  pub(crate) fn skeletons_active(&self) -> bool {
    let home_loading = HomeSection::ALL
      .iter()
      .any(|section| matches!(self.home.data.section(*section), LoadState::Loading))
      || matches!(self.home.data.shortcuts, LoadState::Loading);
    let browse_loading = match &self.browse.view {
      LibraryBrowseView::Loading => true,
      // A ready grid can still hold unloaded placeholder slots while more
      // pages stream in.
      LibraryBrowseView::Ready { visible_items, .. } => {
        visible_items.iter().any(|slot| slot.item.is_none())
      }
      LibraryBrowseView::Inactive | LibraryBrowseView::Empty | LibraryBrowseView::Failed { .. } => {
        false
      }
    };
    // Detail renders episode skeletons for season episodes and continue-watching
    // neighbors independently of the main content load state; all three keep the
    // shimmer frames subscription alive.
    let detail_loading = matches!(self.detail.data.content, LoadState::Loading)
      || matches!(self.detail.data.season_episodes, LoadState::Loading)
      || matches!(self.detail.data.season_neighbors, LoadState::Loading);
    // Artwork cells also display breathing skeleton pulse blocks while in the
    // Loading state across Home hero/sections, Browse grid, and Detail views.
    let artwork_loading = self.home.artwork.has_loading()
      || self.browse.artwork.has_loading()
      || self.detail.artwork.has_loading();
    home_loading || browse_loading || detail_loading || artwork_loading
  }
  /// Effective UI theme mode: the explicit setting, or the OS mode while the
  /// setting is `System` (an unreported OS mode falls back to Dark).
  pub fn theme_mode(&self) -> UiThemeMode {
    match self.kernel.settings.snapshot().theme_mode() {
      ThemeMode::Dark => UiThemeMode::Dark,
      ThemeMode::Light => UiThemeMode::Light,
      ThemeMode::System => match self.system_theme {
        iced::theme::Mode::Light => UiThemeMode::Light,
        iced::theme::Mode::None | iced::theme::Mode::Dark => UiThemeMode::Dark,
      },
    }
  }

  /// Semantic colors and shadows for the effective theme mode.
  pub fn palette(&self) -> &'static ThemePalette {
    match self.theme_mode() {
      UiThemeMode::Dark => &DARK_PALETTE,
      UiThemeMode::Light => &LIGHT_PALETTE,
    }
  }
  /// Persisted app mode: Full (Library Browser shell) or Control-Only
  /// (compact Now Playing controller).
  pub fn app_mode(&self) -> AppMode {
    self.kernel.settings.snapshot().app_mode()
  }

  pub fn retain_artwork_handles(&mut self) {
    let slots: HashSet<_> = self.all_artwork_slots().collect();
    self.kernel.artwork_handles.retain_slots(slots);
  }

  /// Dismisses the kernel toast and clears the playback surface's notice;
  /// cross-surface so it stays on `State` (ADR 0029).
  pub fn dismiss_toast(&mut self, id: u64) {
    if self
      .kernel
      .active_toast
      .as_ref()
      .is_some_and(|toast| toast.id == id)
    {
      self.kernel.active_toast = None;
      self.kernel.notice = None;
      self.playback.notice = None;
    }
  }

  #[allow(dead_code)]
  pub fn clear_toast(&mut self) {
    self.kernel.active_toast = None;
    self.kernel.notice = None;
    self.playback.notice = None;
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
  #[test]
  fn diagnostic_filters_match_level_and_category_independently() {
    assert!(diagnostic_matches(
      Some(DiagnosticLevel::Warning),
      Some(DiagnosticCategory::Playback),
      DiagnosticLevel::Warning,
      DiagnosticCategory::Playback,
    ));
    assert!(!diagnostic_matches(
      Some(DiagnosticLevel::Error),
      None,
      DiagnosticLevel::Warning,
      DiagnosticCategory::Playback,
    ));
    assert!(!diagnostic_matches(
      None,
      Some(DiagnosticCategory::Auth),
      DiagnosticLevel::Error,
      DiagnosticCategory::Config,
    ));
  }
  #[test]
  fn skeletons_active_returns_true_when_artwork_cells_are_loading() {
    let mut state = State::boot(false);
    assert!(!state.skeletons_active());

    let mut binder = ArtworkBinder::default();
    let slot_1 = binder.bind(ArtworkSurface::Home);
    let slot_2 = binder.bind(ArtworkSurface::Home);
    let slot_3 = binder.bind(ArtworkSurface::Browse);
    let slot_4 = binder.bind(ArtworkSurface::Detail);

    // Home hero loading
    state.home.artwork.insert_hero(
      "item-hero".to_owned(),
      ArtworkCell {
        slot: slot_1,
        image_id: "img-hero".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    assert!(state.skeletons_active());
    state.home.artwork.prune_unready();
    assert!(!state.skeletons_active());

    // Home card loading
    state.home.artwork.insert_card(
      HomeSection::ContinueWatching,
      "item-card".to_owned(),
      ArtworkCell {
        slot: slot_2,
        image_id: "img-card".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    assert!(state.skeletons_active());
    state.home.artwork.prune_unready();
    assert!(!state.skeletons_active());

    // Browse artwork loading
    state.browse.artwork.insert(
      "item-browse".to_owned(),
      ArtworkCell {
        slot: slot_3,
        image_id: "img-browse".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    assert!(state.skeletons_active());
    state.browse.artwork.clear();
    assert!(!state.skeletons_active());

    // Detail artwork loading
    state.detail.artwork.insert(
      "detail-poster".to_owned(),
      ArtworkCell {
        slot: slot_4,
        image_id: "img-detail".to_owned(),
        state: ArtworkCellState::Loading,
      },
    );
    assert!(state.skeletons_active());
    state.detail.artwork.clear();
    assert!(!state.skeletons_active());
  }
}
