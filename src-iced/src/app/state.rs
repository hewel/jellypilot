use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use iced::widget::image;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_auth::{AuthStore, SavedProfileKey, SavedProfileSummary, SensitiveSavedSession};
use jellypilot_core::browse_model::LibraryBrowseView;
use jellypilot_core::config::{
  AppMode, IntroMode, LoginPrefill, Settings, SettingsStore, ShortcutKind, ThemeMode,
};
use jellypilot_core::detail::DetailContent;
use jellypilot_core::diagnostics::{DiagnosticCategory, DiagnosticLevel, Diagnostics};
use jellypilot_core::home_hero::{self, HeroCandidate, HeroSource};
use jellypilot_core::request_gate::{RemoteToken, RequestGate};
use jellypilot_core::LoadState;
use jellypilot_media_server::artwork::ArtworkAdapter;
use jellypilot_media_server::{
  LibraryLatestRow, MediaServerProvider, VideoLibraryItem, VideoLibraryShortcut,
  VideoSeasonEpisodesPage,
};
use jellypilot_mpv::playback::PlaybackController;
use jellypilot_session::{IntroSkipMode, JellyfinWebSocket, JellyfinWebSocketEvent};
use jellypilot_ui::theme::ThemeMode as UiThemeMode;
use jellypilot_ui::tokens::{ThemePalette, DARK_PALETTE, LIGHT_PALETTE};
use zeroize::Zeroizing;

use super::kernel::Kernel;
use crate::i18n::{FluentValue, Localizer, UiText};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeLevel {
  Warning,
  Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToastNotice {
  pub id: u64,
  pub message: UiText,
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
  pub auto_login_attempted: bool,
  pub busy_profile: Option<SavedProfileKey>,
  pub error: Option<UiText>,
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
      auto_login_attempted: false,
      busy_profile: None,
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
  pub provider: jellypilot_media_server::MediaServerProvider,
  pub server_url: String,
  pub server_name: Option<String>,
}

impl ConnectedIdentity {
  pub fn from_session(session: &SensitiveSavedSession) -> Self {
    Self {
      user_name: session.user_name.clone(),
      provider: session.provider,
      server_url: session.server_url.clone(),
      server_name: session.server_name.clone(),
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
  PersonalLists(crate::app::personal_lists::Route),
  Detail(String),
  /// Full-window Now Playing; the Control-Only root destination, unused in
  /// Full mode where the player is a bar above the shell content.
  NowPlaying,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HomeSection {
  ContinueWatching,
  NextUp,
  Latest(usize),
}

impl HomeSection {
  pub const fn index(self) -> usize {
    match self {
      Self::ContinueWatching => 0,
      Self::NextUp => 1,
      Self::Latest(index) => index + 2,
    }
  }

  pub const fn is_latest(self) -> bool {
    matches!(self, Self::Latest(_))
  }

  pub const fn is_action(self) -> bool {
    matches!(self, Self::ContinueWatching | Self::NextUp)
  }
}

pub struct HomeRow {
  pub section: HomeSection,
  pub title: UiText,
  pub items: LoadState<Vec<VideoLibraryItem>, UiText>,
}

impl HomeRow {
  fn new(
    section: HomeSection,
    title: UiText,
    items: LoadState<Vec<VideoLibraryItem>, UiText>,
  ) -> Self {
    Self {
      section,
      title,
      items,
    }
  }
}

pub struct HomeState {
  pub rows: Vec<HomeRow>,
  pub shortcuts: LoadState<Vec<VideoLibraryShortcut>, UiText>,
  pub hovered_card: Option<String>,
  hero_candidates: Vec<HeroCandidate>,
  selected_hero_id: Option<String>,
}

impl Default for HomeState {
  fn default() -> Self {
    Self {
      rows: vec![
        HomeRow::new(
          HomeSection::ContinueWatching,
          UiText::new("home-continue-watching"),
          LoadState::Idle,
        ),
        HomeRow::new(
          HomeSection::NextUp,
          UiText::new("home-next-up"),
          LoadState::Idle,
        ),
      ],
      shortcuts: LoadState::Idle,
      hovered_card: None,
      hero_candidates: Vec::new(),
      selected_hero_id: None,
    }
  }
}

impl HomeState {
  pub fn begin_load(&mut self) {
    for row in &mut self.rows {
      row.items = LoadState::Loading;
    }
    if !matches!(self.shortcuts, LoadState::Ready(_)) {
      self.shortcuts = LoadState::Loading;
    }
  }

  pub fn settle_video_home(&mut self, result: Result<jellypilot_media_server::VideoHome, String>) {
    match result {
      Ok(home) => {
        self.rows[HomeSection::ContinueWatching.index()].items =
          LoadState::Ready(home.continue_watching);
        self.rows[HomeSection::NextUp.index()].items = LoadState::Ready(home.next_up);
      }
      Err(error) => {
        let error = jellypilot_core::diagnostics::sanitize_message(&error);
        for section in [HomeSection::ContinueWatching, HomeSection::NextUp] {
          if !matches!(self.rows[section.index()].items, LoadState::Ready(_)) {
            self.rows[section.index()].items = LoadState::Failed(
              UiText::new("home-content-load-failed").arg("details", error.clone()),
            );
          }
        }
      }
    }
    self.refresh_hero_candidates();
  }

  pub fn settle_latest_rows(&mut self, result: Result<Vec<LibraryLatestRow>, String>) {
    self.rows.truncate(2);
    let Ok(latest_rows) = result else {
      self.refresh_hero_candidates();
      return;
    };
    self
      .rows
      .extend(latest_rows.into_iter().enumerate().map(|(index, row)| {
        HomeRow::new(
          HomeSection::Latest(index),
          UiText::new("home-latest").arg("name", row.library_name),
          row.result.map_or_else(
            |error| {
              LoadState::Failed(UiText::new("home-content-load-failed").arg(
                "details",
                jellypilot_core::diagnostics::sanitize_message(&error),
              ))
            },
            LoadState::Ready,
          ),
        )
      }));
    self.refresh_hero_candidates();
  }

  pub fn settle_shortcuts(&mut self, result: Result<Vec<VideoLibraryShortcut>, String>) {
    self.shortcuts = match result {
      Ok(shortcuts) => LoadState::Ready(shortcuts),
      Err(error) => LoadState::Failed(UiText::new("home-libraries-load-failed").arg(
        "details",
        jellypilot_core::diagnostics::sanitize_message(&error),
      )),
    };
  }

  pub fn rows(&self) -> &[HomeRow] {
    &self.rows
  }

  pub fn row(&self, section: HomeSection) -> Option<&HomeRow> {
    self
      .rows
      .get(section.index())
      .filter(|row| row.section == section)
  }

  pub fn hero_candidates(&self) -> impl Iterator<Item = (HomeSection, &VideoLibraryItem)> {
    self.hero_candidates.iter().filter_map(|candidate| {
      let section = match candidate.source {
        HeroSource::ContinueWatching => HomeSection::ContinueWatching,
        HeroSource::NextUp => HomeSection::NextUp,
        HeroSource::Latest(index) => HomeSection::Latest(index),
      };
      let items = ready_items(&self.row(section)?.items)?;
      Some((section, items.get(candidate.item_index)?))
    })
  }

  pub fn featured_item(&self) -> Option<&VideoLibraryItem> {
    self.featured_candidate().map(|(_, item)| item)
  }

  pub fn featured_section(&self) -> Option<HomeSection> {
    self.featured_candidate().map(|(section, _)| section)
  }

  fn featured_candidate(&self) -> Option<(HomeSection, &VideoLibraryItem)> {
    self
      .hero_candidates()
      .find(|(_, item)| Some(item.id.as_str()) == self.selected_hero_id.as_deref())
      .or_else(|| self.hero_candidates().next())
  }

  pub fn select_hero(&mut self, item_id: &str) -> Option<usize> {
    let index = self
      .hero_candidates()
      .position(|(_, item)| item.id == item_id)?;
    if self.selected_hero_id.as_deref() != Some(item_id) {
      self.selected_hero_id = Some(item_id.to_owned());
    }
    Some(index)
  }

  fn refresh_hero_candidates(&mut self) {
    self.hero_candidates = home_hero::candidates(
      &self.rows[HomeSection::ContinueWatching.index()].items,
      &self.rows[HomeSection::NextUp.index()].items,
      self.rows.iter().skip(2).map(|row| &row.items),
    );
  }

  /// Reconcile only after the complete Home response has replaced its sources.
  /// A selected item may move from continuation into the latest-content fallback.
  pub fn reconcile_hero_selection(&mut self) {
    let selected = home_hero::retained_selection(
      self.selected_hero_id.as_deref(),
      self.hero_candidates().map(|(_, item)| item),
    );
    if selected != self.selected_hero_id.as_deref() {
      self.selected_hero_id = selected.map(str::to_owned);
    }
  }

  pub fn has_ready_content(&self) -> bool {
    self
      .rows
      .iter()
      .any(|row| matches!(row.items, LoadState::Ready(_)))
  }
}
fn ready_items(state: &LoadState<Vec<VideoLibraryItem>, UiText>) -> Option<&[VideoLibraryItem]> {
  match state {
    LoadState::Ready(items) => Some(items),
    LoadState::Idle | LoadState::Loading | LoadState::Failed(_) => None,
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserDataActionKind {
  Favorite,
  Played,
}

#[derive(Default)]
pub struct DetailState {
  pub content: LoadState<DetailContent, UiText>,
  pub season_neighbors: LoadState<Vec<VideoLibraryItem>, UiText>,
  pub similar_items: LoadState<Vec<VideoLibraryItem>, UiText>,
  pub season_episodes: LoadState<VideoSeasonEpisodesPage, UiText>,
  pub selected_season_id: Option<String>,
  pub overview_expanded: bool,
  pub expanded_episode_ids: HashSet<String>,
  pub user_data_busy: Option<UserDataActionKind>,
  pub user_data_error: Option<UiText>,
}

impl DetailState {
  pub fn clear(&mut self) {
    *self = Self::default();
  }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowseViewport {
  pub offset_y: f32,
}

/// Decoded user photos for saved profiles, keyed by profile identity.
///
/// Linear scans: profile counts stay in the single digits, and
/// `SavedProfileKey` deliberately has no `Hash`.
#[derive(Default)]
pub struct ProfileAvatarHandles {
  loaded: Vec<(SavedProfileKey, image::Handle)>,
  loading: Vec<SavedProfileKey>,
}

impl ProfileAvatarHandles {
  pub fn get(&self, key: &SavedProfileKey) -> Option<&image::Handle> {
    self
      .loaded
      .iter()
      .find(|(loaded_key, _)| loaded_key == key)
      .map(|(_, handle)| handle)
  }

  /// Marks a profile's avatar as in flight; `false` when the photo is already
  /// cached or the load is already running, so callers fire at most one load.
  pub fn begin_loading(&mut self, key: SavedProfileKey) -> bool {
    if self.get(&key).is_some() || self.loading.contains(&key) {
      return false;
    }
    self.loading.push(key);
    true
  }

  /// Stores a settled load, replacing any previous photo for the profile.
  pub fn insert(&mut self, key: SavedProfileKey, handle: image::Handle) {
    self.loading.retain(|loading_key| loading_key != &key);
    if let Some((_, existing)) = self
      .loaded
      .iter_mut()
      .find(|(loaded_key, _)| loaded_key == &key)
    {
      *existing = handle;
    } else {
      self.loaded.push((key, handle));
    }
  }

  /// Clears the in-flight mark without storing a photo (load failed); the
  /// next refresh may retry.
  pub fn finish_loading(&mut self, key: &SavedProfileKey) {
    self.loading.retain(|loading_key| loading_key != key);
  }

  /// Drops photos and in-flight marks for profiles no longer saved.
  pub fn retain(&mut self, keys: &[SavedProfileKey]) {
    self.loaded.retain(|(key, _)| keys.contains(key));
    self.loading.retain(|key| keys.contains(key));
  }
}
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SettingsSection {
  #[default]
  Account,
  Mpv,
  Playback,
  Subtitles,
  Shortcuts,
  Appearance,
  Storage,
  Diagnostics,
}

impl SettingsSection {
  pub const ALL: [Self; 8] = [
    Self::Account,
    Self::Mpv,
    Self::Playback,
    Self::Subtitles,
    Self::Shortcuts,
    Self::Appearance,
    Self::Storage,
    Self::Diagnostics,
  ];

  pub fn label(self, locale: Localizer) -> String {
    locale.text(match self {
      Self::Account => "settings-account",
      Self::Mpv => "settings-mpv",
      Self::Playback => "settings-playback",
      Self::Subtitles => "settings-subtitles",
      Self::Shortcuts => "settings-shortcuts",
      Self::Appearance => "settings-interface",
      Self::Storage => "settings-storage",
      Self::Diagnostics => "settings-diagnostics",
    })
  }
}

pub struct SettingsState {
  pub active_section: SettingsSection,
  pub mpv_path_input: String,
  pub mpv_args_input: String,
  pub playback_target_name_input: String,
  pub intro_menu_open: bool,
  pub subtitle_menu_open: bool,
  pub diagnostic_level_menu_open: bool,
  pub diagnostic_category_menu_open: bool,
  pub font_licenses_expanded: bool,
  pub diagnostic_level: Option<DiagnosticLevel>,
  pub diagnostic_category: Option<DiagnosticCategory>,
  pub shortcut_capture: Option<ShortcutKind>,
  pub error: Option<UiText>,
  pub saved: Option<UiText>,
}

impl SettingsState {
  pub fn from_settings(settings: &Settings) -> Self {
    Self {
      active_section: SettingsSection::default(),
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
      font_licenses_expanded: false,
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

#[derive(Default)]
pub struct FullUi {
  pub personal_lists: crate::app::personal_lists::Surface,
  pub home: crate::app::home::Surface,
  pub browse: crate::app::browse::Surface,
  pub detail: crate::app::detail::Surface,
}

pub struct State {
  pub kernel: Kernel,
  pub image_diagnostics: super::artwork::ImageDiagnostics,
  /// Latest OS light/dark mode report; `None` until the boot one-shot task
  /// resolves. Read only while the theme mode setting is `System`.
  pub system_theme: iced::theme::Mode,
  pub login: crate::app::login::Surface,
  pub settings: crate::app::settings::Surface,
  pub instance: Option<crate::instance::Guard>,
  pub full: Option<FullUi>,
  pub playback: crate::app::playback::Surface,
  pub shell: crate::app::shell::Surface,
  pub watchlist: crate::app::personal_lists::Runtime,
  pub accounts: crate::app::accounts::Surface,
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
    login.error = settings_error
      .as_ref()
      .map(|_| UiText::new("startup-settings-load-failed"));
    login.auto_login_attempted = smoke;
    let settings_view = SettingsState::from_settings(settings.snapshot());
    let full_ui = (settings.snapshot().app_mode() == AppMode::Full).then(FullUi::default);
    let mut diagnostics = Diagnostics::default();
    if let Some(error) = &settings_error {
      diagnostics.record(DiagnosticLevel::Error, DiagnosticCategory::Config, error);
    }
    let mut request_gate = RequestGate::default();
    let playback = crate::app::playback::Surface::new(&mut request_gate);
    let artwork_adapter = Arc::new(ArtworkAdapter::new());
    artwork_adapter.set_disk_cache_enabled(settings.snapshot().image_cache_enabled());
    let avatar_adapter = Arc::new(ArtworkAdapter::new());
    avatar_adapter.set_disk_cache_enabled(settings.snapshot().image_cache_enabled());
    let locale = Localizer::resolve(settings.snapshot().ui_language());

    let mut state = Self {
      image_diagnostics: Default::default(),
      system_theme: iced::theme::Mode::None,
      kernel: Kernel {
        settings,
        locale,
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
        avatar_adapter,
        profile_avatars: Default::default(),
      },
      login: crate::app::login::Surface {
        flow: login,
        quick_connect_task: None,
      },
      settings: crate::app::settings::Surface {
        view: settings_view,
      },
      instance: None,
      full: full_ui,
      playback,
      shell: crate::app::shell::Surface::new(smoke),
      watchlist: crate::app::personal_lists::Runtime::default(),
      accounts: crate::app::accounts::Surface::new(),
    };
    // Control-Only boots straight into the full-window Now Playing root; the
    // Library Browser destinations stay unreachable (router guard).
    if state.app_mode() == AppMode::ControlOnly {
      state.shell.destination = Destination::NowPlaying;
    }
    state
  }

  pub fn t(&self, id: &str) -> String {
    self.kernel.locale.text(id)
  }

  pub fn format(&self, id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    self.kernel.locale.format(id, args)
  }

  /// True while any full UI surface renders skeleton placeholders or while
  /// any artwork cell is loading.
  pub(crate) fn skeletons_active(&self) -> bool {
    let Some(full) = &self.full else {
      return false;
    };
    let home_loading = full
      .home
      .data
      .rows()
      .iter()
      .any(|row| matches!(row.items, LoadState::Loading))
      || matches!(full.home.data.shortcuts, LoadState::Loading);
    let browse_loading = match &full.browse.view {
      LibraryBrowseView::Loading => true,
      LibraryBrowseView::Ready { visible_items, .. } => {
        visible_items.iter().any(|slot| slot.item.is_none())
      }
      LibraryBrowseView::Inactive | LibraryBrowseView::Empty | LibraryBrowseView::Failed { .. } => {
        false
      }
    };
    let detail_loading = matches!(full.detail.data.content, LoadState::Loading)
      || matches!(full.detail.data.season_episodes, LoadState::Loading)
      || matches!(full.detail.data.season_neighbors, LoadState::Loading);
    let artwork_loading = full.home.artwork.has_loading()
      || full.browse.artwork.has_loading()
      || full.detail.artwork.has_loading();
    let lists_loading = full.personal_lists.favorites.loading
      || full.personal_lists.watchlist.loading
      || full.personal_lists.artwork.has_loading();
    home_loading || browse_loading || detail_loading || artwork_loading || lists_loading
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

  use super::*;

  #[test]
  fn home_sections_transition_from_loading_to_independent_results() {
    let mut home = HomeState::default();
    home.begin_load();
    home.settle_video_home(Err("home failed".to_owned()));
    home.settle_shortcuts(Ok(Vec::new()));

    assert!(matches!(
      (
        &home.rows()[0].items,
        &home.rows()[1].items,
        &home.shortcuts,
      ),
      (
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
    }));
    home.settle_latest_rows(Ok(vec![LibraryLatestRow {
      library_id: "shows".to_owned(),
      library_name: "Shows".to_owned(),
      result: Ok(Vec::new()),
    }]));

    assert!(home
      .rows()
      .iter()
      .all(|row| matches!(&row.items, LoadState::Ready(items) if items.is_empty())));
  }

  #[test]
  fn home_rows_keep_fixed_rows_before_latest_libraries_in_server_order() {
    let mut home = HomeState::default();
    home.settle_latest_rows(Ok(vec![
      LibraryLatestRow {
        library_id: "movies".to_owned(),
        library_name: "Movies".to_owned(),
        result: Ok(Vec::new()),
      },
      LibraryLatestRow {
        library_id: "shows".to_owned(),
        library_name: "TV Shows".to_owned(),
        result: Ok(Vec::new()),
      },
    ]));

    assert_eq!(
      home
        .rows()
        .iter()
        .map(|row| row.section)
        .collect::<Vec<_>>(),
      vec![
        HomeSection::ContinueWatching,
        HomeSection::NextUp,
        HomeSection::Latest(0),
        HomeSection::Latest(1),
      ]
    );
    for language in [
      jellypilot_core::locale::UiLanguage::English,
      jellypilot_core::locale::UiLanguage::SimplifiedChinese,
    ] {
      let locale = Localizer::new(language);
      assert!(locale.message(&home.rows()[2].title).contains("Movies"));
      assert!(locale.message(&home.rows()[3].title).contains("TV Shows"));
    }
  }

  #[test]
  fn latest_rows_replace_stale_identities_and_isolate_library_failures() {
    let mut home = HomeState::default();
    home.settle_latest_rows(Ok(vec![LibraryLatestRow {
      library_id: "old".to_owned(),
      library_name: "Old Library".to_owned(),
      result: Ok(Vec::new()),
    }]));

    home.settle_latest_rows(Ok(vec![
      LibraryLatestRow {
        library_id: "movies".to_owned(),
        library_name: "Movies".to_owned(),
        result: Err("movies failed".to_owned()),
      },
      LibraryLatestRow {
        library_id: "shows".to_owned(),
        library_name: "Shows".to_owned(),
        result: Ok(Vec::new()),
      },
    ]));

    assert!(matches!(
      home.rows(),
      [_, _, HomeRow {
        title: movies_title,
        items: LoadState::Failed(_),
        ..
      }, HomeRow {
        title: shows_title,
        items: LoadState::Ready(shows),
        ..
      }] if Localizer::default().message(movies_title).contains("Movies")
        && Localizer::default().message(shows_title).contains("Shows")
        && shows.is_empty()
    ));
  }

  #[test]
  fn latest_rows_clear_stale_identities_when_row_metadata_is_unavailable() {
    let mut home = HomeState::default();
    home.settle_latest_rows(Ok(vec![LibraryLatestRow {
      library_id: "old".to_owned(),
      library_name: "Old Library".to_owned(),
      result: Ok(Vec::new()),
    }]));

    home.settle_latest_rows(Err("shortcuts failed".to_owned()));

    assert_eq!(home.rows().len(), 2);
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
}
