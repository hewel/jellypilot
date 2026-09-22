use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jellypilot_media_server::{
    VideoLibraryPlayedFilter, VideoLibrarySort, VideoLibrarySortDirection,
};
use serde::{Deserialize, Serialize};

use crate::browse_model::BrowsePreferences;
use crate::locale::LanguagePreference;
use crate::watchlist::ProfileScope;

#[cfg(feature = "native")]
pub(crate) const CONFIG_DIRECTORY: &str = "jellypilot";
const CONFIG_FILE: &str = "config.json";
/// Revision 1 applies the Linux Embedded MPV default to files that still record External.
const CURRENT_SETTINGS_REVISION: u32 = 1;

/// Global Intro Skipper behavior (ADR 0045): Automatic skips detected ranges,
/// Manual presents the skip action. The retired Off value migrates to Manual.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IntroMode {
    #[default]
    Automatic,
    Manual,
}

impl<'de> Deserialize<'de> for IntroMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some(mode) if mode.eq_ignore_ascii_case("manual") => Self::Manual,
            // Legacy "off" migrates to Manual: former Off users gain the manual
            // action but are never opted into automatic skipping.
            Some(mode) if mode.eq_ignore_ascii_case("off") => Self::Manual,
            _ => Self::Automatic,
        })
    }
}
/// A profile-scoped per-series Intro Skipper choice (ADR 0045). A record exists
/// only while the choice differs from the global setting; selecting the global
/// value clears it, and a global change clears every record that now matches.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeriesIntroPreference {
    scope: ProfileScope,
    series_id: String,
    mode: IntroMode,
}

/// Preferred color scheme: follow the OS, or pin the dark or light theme.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

impl<'de> Deserialize<'de> for ThemeMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some(mode) if mode.eq_ignore_ascii_case("dark") => Self::Dark,
            Some(mode) if mode.eq_ignore_ascii_case("light") => Self::Light,
            _ => Self::System,
        })
    }
}
/// Top-level operating mode: the full Library Browser shell, or the compact
/// Control-Only controller window (Now Playing and Settings only).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    #[default]
    Full,
    #[serde(rename = "controlonly")]
    ControlOnly,
}

impl<'de> Deserialize<'de> for AppMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some(mode) if mode.eq_ignore_ascii_case("controlonly") => Self::ControlOnly,
            _ => Self::Full,
        })
    }
}

/// Presentation preference, independent of the application's playback capabilities.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UiMode {
    #[default]
    Desktop,
    Tv,
}

impl<'de> Deserialize<'de> for UiMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some(mode) if mode.eq_ignore_ascii_case("tv") => Self::Tv,
            _ => Self::Desktop,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginPrefill {
    server_url: String,
    username: String,
}

impl LoginPrefill {
    pub fn new(server_url: String, username: String) -> Self {
        Self {
            server_url,
            username,
        }
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseFilterSettings {
    sort: VideoLibrarySort,
    played_filter: VideoLibraryPlayedFilter,
    favorites_only: bool,
    sort_direction: VideoLibrarySortDirection,
}

impl Default for BrowseFilterSettings {
    fn default() -> Self {
        Self {
            sort: VideoLibrarySort::Title,
            played_filter: VideoLibraryPlayedFilter::All,
            favorites_only: false,
            sort_direction: VideoLibrarySortDirection::Ascending,
        }
    }
}

impl<'de> Deserialize<'de> for BrowseFilterSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let Some(object) = value.as_object() else {
            return Ok(Self::default());
        };
        let sort = match object.get("sort").and_then(serde_json::Value::as_str) {
            Some("recentlyAdded") => VideoLibrarySort::RecentlyAdded,
            Some("releaseDate") => VideoLibrarySort::ReleaseDate,
            _ => VideoLibrarySort::Title,
        };
        let played_filter = match object
            .get("playedFilter")
            .and_then(serde_json::Value::as_str)
        {
            Some("played") => VideoLibraryPlayedFilter::Played,
            Some("unplayed") => VideoLibraryPlayedFilter::Unplayed,
            _ => VideoLibraryPlayedFilter::All,
        };
        let sort_direction = match object
            .get("sortDirection")
            .and_then(serde_json::Value::as_str)
        {
            Some("desc") => VideoLibrarySortDirection::Descending,
            _ => VideoLibrarySortDirection::Ascending,
        };

        Ok(Self {
            sort,
            played_filter,
            favorites_only: object
                .get("favoritesOnly")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            sort_direction,
        })
    }
}

impl BrowseFilterSettings {
    pub const fn sort(self) -> VideoLibrarySort {
        self.sort
    }

    pub const fn played_filter(self) -> VideoLibraryPlayedFilter {
        self.played_filter
    }

    pub const fn favorites_only(self) -> bool {
        self.favorites_only
    }

    pub const fn sort_direction(self) -> VideoLibrarySortDirection {
        self.sort_direction
    }

    #[must_use]
    pub const fn with_sort(mut self, sort: VideoLibrarySort) -> Self {
        self.sort = sort;
        self
    }

    #[must_use]
    pub const fn with_played_filter(mut self, played_filter: VideoLibraryPlayedFilter) -> Self {
        self.played_filter = played_filter;
        self
    }

    #[must_use]
    pub const fn with_favorites_only(mut self, favorites_only: bool) -> Self {
        self.favorites_only = favorites_only;
        self
    }

    #[must_use]
    pub const fn with_sort_direction(mut self, sort_direction: VideoLibrarySortDirection) -> Self {
        self.sort_direction = sort_direction;
        self
    }
}

impl From<BrowseFilterSettings> for BrowsePreferences {
    fn from(settings: BrowseFilterSettings) -> Self {
        Self {
            sort: settings.sort,
            sort_direction: settings.sort_direction,
            played_filter: settings.played_filter,
            favorites_only: settings.favorites_only,
            filters: Default::default(),
        }
    }
}

/// Playback presentation backend. Changes take effect on the next application start.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaybackBackend {
    External,
    Embedded,
}

impl Default for PlaybackBackend {
    fn default() -> Self {
        if cfg!(target_os = "linux") {
            Self::Embedded
        } else {
            Self::External
        }
    }
}

/// HDR presentation mode for the embedded Linux/Wayland Vulkan host. SDR
/// presentation is unaffected; changes apply on the next output configuration.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HdrOutput {
    /// Enter HDR only while HDR content plays and the display signals HDR10 support.
    #[default]
    Auto,
    /// Keep the window in HDR whenever the display signals support; SDR content
    /// is mapped into the PQ container at the 203 nit reference white.
    On,
    /// Never present HDR.
    Off,
}

impl HdrOutput {
    /// Resolves whether HDR presentation is active. Every mode requires the
    /// display chain to advertise HDR10; Auto additionally requires HDR
    /// content. Detection of both belongs to the presentation layer.
    pub const fn active(self, display_hdr10: bool, content_hdr: bool) -> bool {
        display_hdr10
            && match self {
                Self::Auto => content_hdr,
                Self::On => true,
                Self::Off => false,
            }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Settings {
    remember: bool,
    server_url: String,
    provider: String,
    username: String,
    #[serde(
        default = "default_auto_login",
        deserialize_with = "deserialize_auto_login"
    )]
    auto_login: bool,
    #[serde(default)]
    intro_mode: IntroMode,
    #[serde(default = "default_auto_login")]
    auto_next_episode: bool,
    #[serde(default = "default_progress_sync_seconds")]
    progress_sync_seconds: u64,
    /// Profile-scoped per-series Intro Skipper choices (ADR 0045). Records
    /// exist only while a choice differs from the global `intro_mode`; a
    /// global change clears every record that now matches, across all scopes.
    #[serde(default, deserialize_with = "deserialize_series_intro_modes")]
    series_intro_modes: Vec<SeriesIntroPreference>,
    #[serde(default)]
    theme_mode: ThemeMode,
    #[serde(default)]
    ui_language: LanguagePreference,
    #[serde(default)]
    app_mode: AppMode,
    #[serde(default)]
    ui_mode: UiMode,
    #[serde(default)]
    playback_backend: PlaybackBackend,
    #[serde(default)]
    hdr_output: HdrOutput,
    #[serde(default)]
    settings_revision: u32,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    mpv_path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    mpv_args: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    playback_target_name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    subtitle_languages: Vec<String>,
    #[serde(
        default = "default_key_next_episode",
        deserialize_with = "deserialize_key_next_episode"
    )]
    key_next_episode: String,
    #[serde(
        default = "default_key_previous_episode",
        deserialize_with = "deserialize_key_previous_episode"
    )]
    key_previous_episode: String,
    #[serde(
        default = "default_key_intro_skip",
        deserialize_with = "deserialize_key_intro_skip"
    )]
    key_intro_skip: String,
    #[serde(
        default = "default_image_cache_enabled",
        deserialize_with = "deserialize_image_cache_enabled"
    )]
    image_cache_enabled: bool,
    #[serde(
        default = "default_remember_season_volume",
        deserialize_with = "deserialize_remember_season_volume"
    )]
    remember_season_volume: bool,
    #[serde(default, deserialize_with = "deserialize_start_minimized")]
    start_minimized: bool,
    #[serde(default, deserialize_with = "deserialize_reduced_motion")]
    reduced_motion: bool,
    #[serde(default)]
    prefer_original_audio: bool,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    tmdb_api_key: Option<String>,
    #[serde(default)]
    library_filters: BrowseFilterSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remember: false,
            server_url: String::new(),
            provider: String::new(),
            username: String::new(),
            auto_login: default_auto_login(),
            intro_mode: IntroMode::Automatic,
            auto_next_episode: true,
            progress_sync_seconds: default_progress_sync_seconds(),
            series_intro_modes: Vec::new(),
            theme_mode: ThemeMode::System,
            ui_language: LanguagePreference::System,
            app_mode: AppMode::Full,
            ui_mode: UiMode::Desktop,
            playback_backend: PlaybackBackend::default(),
            hdr_output: HdrOutput::default(),
            settings_revision: CURRENT_SETTINGS_REVISION,
            mpv_path: None,
            mpv_args: Vec::new(),
            playback_target_name: None,
            subtitle_languages: Vec::new(),
            key_next_episode: default_key_next_episode(),
            key_previous_episode: default_key_previous_episode(),
            key_intro_skip: default_key_intro_skip(),
            image_cache_enabled: default_image_cache_enabled(),
            remember_season_volume: default_remember_season_volume(),
            start_minimized: false,
            reduced_motion: false,
            prefer_original_audio: false,
            library_filters: BrowseFilterSettings::default(),
            tmdb_api_key: None,
        }
    }
}

impl Settings {
    pub fn login_prefill(&self) -> LoginPrefill {
        LoginPrefill::new(self.server_url.clone(), self.username.clone())
    }

    pub fn remembers_login_prefill(&self) -> bool {
        self.remember
    }

    pub fn login_provider(&self) -> &str {
        &self.provider
    }
    pub const fn auto_login(&self) -> bool {
        self.auto_login
    }

    pub const fn intro_mode(&self) -> IntroMode {
        self.intro_mode
    }
    pub const fn auto_next_episode(&self) -> bool {
        self.auto_next_episode
    }

    pub const fn progress_sync_seconds(&self) -> u64 {
        match self.progress_sync_seconds {
            3 | 5 | 10 | 30 => self.progress_sync_seconds,
            _ => default_progress_sync_seconds(),
        }
    }

    pub const fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }
    pub const fn ui_language(&self) -> LanguagePreference {
        self.ui_language
    }
    pub const fn app_mode(&self) -> AppMode {
        self.app_mode
    }

    pub const fn ui_mode(&self) -> UiMode {
        self.ui_mode
    }

    pub const fn playback_backend(&self) -> PlaybackBackend {
        self.playback_backend
    }

    pub const fn hdr_output(&self) -> HdrOutput {
        self.hdr_output
    }

    pub fn mpv_path(&self) -> Option<&str> {
        self.mpv_path.as_deref()
    }

    pub fn mpv_args(&self) -> &[String] {
        &self.mpv_args
    }

    pub fn playback_target_name(&self) -> Option<&str> {
        self.playback_target_name.as_deref()
    }

    pub fn tmdb_api_key(&self) -> Option<&str> {
        self.tmdb_api_key.as_deref()
    }

    pub fn subtitle_languages(&self) -> &[String] {
        &self.subtitle_languages
    }

    pub fn key_next_episode(&self) -> &str {
        &self.key_next_episode
    }

    pub fn key_previous_episode(&self) -> &str {
        &self.key_previous_episode
    }

    pub fn key_intro_skip(&self) -> &str {
        &self.key_intro_skip
    }

    pub const fn image_cache_enabled(&self) -> bool {
        self.image_cache_enabled
    }

    pub const fn remember_season_volume(&self) -> bool {
        self.remember_season_volume
    }

    pub const fn start_minimized(&self) -> bool {
        self.start_minimized
    }

    pub const fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    pub const fn prefer_original_audio(&self) -> bool {
        self.prefer_original_audio
    }

    pub const fn browse_filters(&self) -> BrowseFilterSettings {
        self.library_filters
    }

    /// The effective Intro Skipper mode for a series in a profile scope: the
    /// recorded override, or the global setting when none is recorded.
    #[must_use]
    pub fn series_intro_mode(&self, series_id: &str, scope: &ProfileScope) -> IntroMode {
        self.series_intro_modes
            .iter()
            .find(|record| &record.scope == scope && record.series_id == series_id)
            .map_or(self.intro_mode, |record| record.mode)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutKind {
    Next,
    Previous,
    IntroSkip,
}

#[derive(Debug)]
pub enum SettingsMutationError {
    Config(ConfigError),
    InvalidLoginPrefill,
    InvalidProvider,
    InvalidSubtitleLanguage,
    DuplicateSubtitleLanguage,
    EmptyShortcut,
    ShortcutCollision,
}

impl fmt::Display for SettingsMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => error.fmt(formatter),
            Self::InvalidLoginPrefill => formatter.write_str("login prefill is incomplete"),
            Self::InvalidProvider => formatter.write_str("login provider is invalid"),
            Self::InvalidSubtitleLanguage => formatter.write_str("subtitle language is invalid"),
            Self::DuplicateSubtitleLanguage => {
                formatter.write_str("subtitle language is duplicated")
            }
            Self::EmptyShortcut => formatter.write_str("shortcut is empty"),
            Self::ShortcutCollision => formatter.write_str("shortcut is already assigned"),
        }
    }
}

impl std::error::Error for SettingsMutationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ConfigError> for SettingsMutationError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

/// Preferences that can be committed without adopting unrelated disk edits live.
/// Presentation-owned callers use this boundary when account and backend lifecycle
/// changes must remain explicit. Existing generic setters keep their merge contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalPreference {
    IntroMode(IntroMode),
    ThemeMode(ThemeMode),
    AutoLogin(bool),
    PlaybackBackend(PlaybackBackend),
    HdrOutput(HdrOutput),
    SubtitleLanguages(Vec<String>),
    ImageCache(bool),
    RememberSeasonVolume(bool),
    PreferOriginalAudio(bool),
    ReducedMotion(bool),
}

fn apply_local_preference(
    settings: &mut Settings,
    preference: &LocalPreference,
) -> Result<(), SettingsMutationError> {
    match preference {
        LocalPreference::IntroMode(mode) => {
            settings.intro_mode = *mode;
            settings
                .series_intro_modes
                .retain(|record| record.mode != *mode);
        }
        LocalPreference::ThemeMode(mode) => settings.theme_mode = *mode,
        LocalPreference::AutoLogin(enabled) => settings.auto_login = *enabled,
        LocalPreference::PlaybackBackend(backend) => settings.playback_backend = *backend,
        LocalPreference::HdrOutput(output) => settings.hdr_output = *output,
        LocalPreference::SubtitleLanguages(languages) => {
            let languages: Vec<_> = languages
                .iter()
                .map(|language| language.trim().to_ascii_lowercase())
                .collect();
            for (index, language) in languages.iter().enumerate() {
                if !valid_subtitle_language(language) {
                    return Err(SettingsMutationError::InvalidSubtitleLanguage);
                }
                if languages[..index].contains(language) {
                    return Err(SettingsMutationError::DuplicateSubtitleLanguage);
                }
            }
            settings.subtitle_languages = languages;
        }
        LocalPreference::ImageCache(enabled) => settings.image_cache_enabled = *enabled,
        LocalPreference::RememberSeasonVolume(enabled) => {
            settings.remember_season_volume = *enabled
        }
        LocalPreference::PreferOriginalAudio(enabled) => settings.prefer_original_audio = *enabled,
        LocalPreference::ReducedMotion(enabled) => settings.reduced_motion = *enabled,
    }
    Ok(())
}

pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
}

#[cfg(feature = "native")]
impl Default for SettingsStore {
    fn default() -> Self {
        Self {
            path: config_path(),
            settings: Settings::default(),
        }
    }
}

impl SettingsStore {
    /// Loads the store bound to the platform configuration directory.
    #[cfg(feature = "native")]
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_in_dir(
            dirs::config_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join(CONFIG_DIRECTORY),
        )
    }

    /// Loads the store bound to an explicit storage directory holding
    /// `config.json`. Callers without platform directory discovery (Android)
    /// pass their private storage root.
    pub fn load_in_dir(storage_dir: PathBuf) -> Result<Self, ConfigError> {
        let path = storage_dir.join(CONFIG_FILE);
        let settings = load_from(&path)?;
        Ok(Self { path, settings })
    }

    /// Creates an isolated store for cross-crate tests.
    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn for_test(path: PathBuf) -> Self {
        Self {
            path,
            settings: Settings::default(),
        }
    }

    pub fn snapshot(&self) -> &Settings {
        &self.settings
    }

    pub fn set_login_prefill(
        &mut self,
        prefill: LoginPrefill,
        provider: String,
    ) -> Result<bool, SettingsMutationError> {
        let server_url = non_empty_setting(prefill.server_url)
            .ok_or(SettingsMutationError::InvalidLoginPrefill)?;
        let username = non_empty_setting(prefill.username)
            .ok_or(SettingsMutationError::InvalidLoginPrefill)?;
        let provider = provider.trim().to_ascii_lowercase();
        if !matches!(provider.as_str(), "jellyfin" | "emby") {
            return Err(SettingsMutationError::InvalidProvider);
        }
        self.update(move |settings| {
            settings.remember = true;
            settings.server_url = server_url;
            settings.provider = provider;
            settings.username = username;
            Ok(())
        })
    }

    pub fn clear_login_prefill(&mut self) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.remember = false;
            settings.server_url.clear();
            settings.provider.clear();
            settings.username.clear();
            Ok(())
        })
    }

    /// Sets the global Intro Skipper mode and clears every per-series record
    /// that now matches it — across all profile scopes — in the same atomic
    /// write, so overrides pinned to the new global never resurrect.
    pub fn set_intro_mode(&mut self, mode: IntroMode) -> Result<bool, SettingsMutationError> {
        self.update(|settings| apply_local_preference(settings, &LocalPreference::IntroMode(mode)))
    }
    pub fn set_theme_mode(&mut self, mode: ThemeMode) -> Result<bool, SettingsMutationError> {
        self.update(|settings| apply_local_preference(settings, &LocalPreference::ThemeMode(mode)))
    }

    /// Persists against the latest disk settings, but commits only the live language.
    /// Returns whether the live preference changed, even if disk already matched.
    pub fn set_ui_language(
        &mut self,
        preference: LanguagePreference,
    ) -> Result<bool, SettingsMutationError> {
        let (mut candidate, missing) = match read_from(&self.path) {
            Ok(settings) => (settings, false),
            Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                (self.settings.clone(), true)
            }
            Err(error) => return Err(error.into()),
        };
        if missing || candidate.ui_language != preference {
            candidate.ui_language = preference;
            save_to(&self.path, &candidate)?;
        }
        let changed = self.settings.ui_language != preference;
        self.settings.ui_language = preference;
        Ok(changed)
    }

    /// Persists a per-series Intro Skipper choice for a profile scope.
    /// Selecting the global value clears the series' record instead of pinning
    /// a redundant one. Returns whether the effective mode for the series
    /// changed; a failed write leaves the live snapshot unchanged.
    pub fn set_series_intro_mode(
        &mut self,
        series_id: &str,
        scope: &ProfileScope,
        mode: IntroMode,
    ) -> Result<bool, SettingsMutationError> {
        if series_id.trim().is_empty() {
            return Err(ConfigError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "series intro mode requires a series id",
            ))
            .into());
        }
        self.update(|settings| {
            if settings.series_intro_mode(series_id, scope) == mode {
                return Ok(());
            }
            settings
                .series_intro_modes
                .retain(|record| !(&record.scope == scope && record.series_id == series_id));
            if mode != settings.intro_mode {
                settings.series_intro_modes.push(SeriesIntroPreference {
                    scope: scope.clone(),
                    series_id: series_id.to_owned(),
                    mode,
                });
            }
            Ok(())
        })
    }

    pub fn set_auto_login(&mut self, enabled: bool) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::AutoLogin(enabled))
        })
    }
    pub fn set_app_mode(&mut self, mode: AppMode) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.app_mode = mode;
            Ok(())
        })
    }

    /// Persists against the latest disk settings, but commits only the live presentation.
    /// Returns whether the live preference changed, even if disk already matched.
    pub fn set_ui_mode(&mut self, mode: UiMode) -> Result<bool, SettingsMutationError> {
        let (mut candidate, missing) = match read_from(&self.path) {
            Ok(settings) => (settings, false),
            Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                (self.settings.clone(), true)
            }
            Err(error) => return Err(error.into()),
        };
        if missing || candidate.ui_mode != mode {
            candidate.ui_mode = mode;
            save_to(&self.path, &candidate)?;
        }
        let changed = self.settings.ui_mode != mode;
        self.settings.ui_mode = mode;
        Ok(changed)
    }

    pub fn set_playback_backend(
        &mut self,
        backend: PlaybackBackend,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::PlaybackBackend(backend))
        })
    }

    pub fn set_hdr_output(&mut self, output: HdrOutput) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::HdrOutput(output))
        })
    }

    pub fn set_mpv_path(&mut self, path: String) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.mpv_path = non_empty_setting(path);
            Ok(())
        })
    }

    pub fn set_mpv_args(&mut self, args: &str) -> Result<bool, SettingsMutationError> {
        let args = parse_mpv_args(args);
        self.update(|settings| {
            settings.mpv_args = args;
            Ok(())
        })
    }

    /// Changes one supported MPV preference without splitting or rewriting other arguments.
    pub fn set_mpv_option(
        &mut self,
        name: &str,
        value: &str,
    ) -> Result<bool, SettingsMutationError> {
        let supported = match name {
            "hwdec" => matches!(value, "auto-safe" | "no"),
            "demuxer-max-bytes" => matches!(value, "128MiB" | "256MiB" | "512MiB" | "1GiB"),
            "audio-spdif" => matches!(value, "" | "ac3,dts,eac3,truehd,dts-hd"),
            _ => false,
        };
        if !supported {
            return Err(ConfigError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported MPV preference",
            ))
            .into());
        }
        self.update_isolated(|settings| {
            let mut arguments = settings.mpv_args.iter().peekable();
            let mut retained = Vec::with_capacity(settings.mpv_args.len() + 1);
            while let Some(argument) = arguments.next() {
                let Some(option) = argument.strip_prefix("--") else {
                    retained.push(argument.clone());
                    continue;
                };
                let (key, separate) = option
                    .split_once('=')
                    .map_or((option, true), |(key, _)| (key, false));
                if key == name {
                    if separate
                        && arguments
                            .peek()
                            .is_some_and(|value| !value.starts_with('-'))
                    {
                        arguments.next();
                    }
                } else {
                    retained.push(argument.clone());
                }
            }
            retained.push(format!("--{name}={value}"));
            settings.mpv_args = retained;
            Ok(())
        })
    }

    pub fn set_auto_next_episode(&mut self, enabled: bool) -> Result<bool, SettingsMutationError> {
        self.update_isolated(|settings| {
            settings.auto_next_episode = enabled;
            Ok(())
        })
    }

    pub fn set_progress_sync_seconds(
        &mut self,
        seconds: u64,
    ) -> Result<bool, SettingsMutationError> {
        if !matches!(seconds, 3 | 5 | 10 | 30) {
            return Err(ConfigError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported progress interval",
            ))
            .into());
        }
        self.update_isolated(|settings| {
            settings.progress_sync_seconds = seconds;
            Ok(())
        })
    }

    pub fn set_playback_target_name(
        &mut self,
        name: String,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.playback_target_name = non_empty_setting(name);
            Ok(())
        })
    }

    pub fn set_tmdb_api_key(&mut self, key: String) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.tmdb_api_key = non_empty_setting(key);
            Ok(())
        })
    }

    pub fn add_subtitle_language(
        &mut self,
        language: String,
    ) -> Result<bool, SettingsMutationError> {
        let language = language.trim().to_ascii_lowercase();
        if !valid_subtitle_language(&language) {
            return Err(SettingsMutationError::InvalidSubtitleLanguage);
        }
        self.update(|settings| {
            if settings
                .subtitle_languages
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&language))
            {
                return Err(SettingsMutationError::DuplicateSubtitleLanguage);
            }
            settings.subtitle_languages.push(language);
            Ok(())
        })
    }

    /// Replaces the ordered language preferences in one validated atomic write.
    pub fn set_subtitle_languages(
        &mut self,
        languages: Vec<String>,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::SubtitleLanguages(languages))
        })
    }

    pub fn move_subtitle_language(
        &mut self,
        index: usize,
        offset: i32,
    ) -> Result<bool, SettingsMutationError> {
        let Ok(index_i32) = i32::try_from(index) else {
            return Ok(false);
        };
        let target = index_i32.saturating_add(offset);
        let Ok(target) = usize::try_from(target) else {
            return Ok(false);
        };
        self.update(|settings| {
            if index >= settings.subtitle_languages.len()
                || target >= settings.subtitle_languages.len()
            {
                return Ok(());
            }
            settings.subtitle_languages.swap(index, target);
            Ok(())
        })
    }

    pub fn remove_subtitle_language(
        &mut self,
        index: usize,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            if index >= settings.subtitle_languages.len() {
                return Ok(());
            }
            settings.subtitle_languages.remove(index);
            Ok(())
        })
    }

    pub fn clear_subtitle_languages(&mut self) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.subtitle_languages.clear();
            Ok(())
        })
    }

    pub fn set_shortcut(
        &mut self,
        kind: ShortcutKind,
        key: String,
    ) -> Result<bool, SettingsMutationError> {
        let key = non_empty_setting(key).ok_or(SettingsMutationError::EmptyShortcut)?;
        self.update(|settings| {
            let collision = match kind {
                ShortcutKind::Next => {
                    binding_matches(&settings.key_previous_episode, &key)
                        || binding_matches(&settings.key_intro_skip, &key)
                }
                ShortcutKind::Previous => {
                    binding_matches(&settings.key_next_episode, &key)
                        || binding_matches(&settings.key_intro_skip, &key)
                }
                ShortcutKind::IntroSkip => {
                    binding_matches(&settings.key_next_episode, &key)
                        || binding_matches(&settings.key_previous_episode, &key)
                }
            };
            if collision {
                return Err(SettingsMutationError::ShortcutCollision);
            }
            match kind {
                ShortcutKind::Next => settings.key_next_episode = key,
                ShortcutKind::Previous => settings.key_previous_episode = key,
                ShortcutKind::IntroSkip => settings.key_intro_skip = key,
            }
            Ok(())
        })
    }

    pub fn set_image_cache_enabled(
        &mut self,
        enabled: bool,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::ImageCache(enabled))
        })
    }

    pub fn set_remember_season_volume(
        &mut self,
        enabled: bool,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::RememberSeasonVolume(enabled))
        })
    }

    pub fn set_start_minimized(
        &mut self,
        start_minimized: bool,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.start_minimized = start_minimized;
            Ok(())
        })
    }

    pub fn set_prefer_original_audio(
        &mut self,
        enabled: bool,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::PreferOriginalAudio(enabled))
        })
    }

    pub fn set_reduced_motion(&mut self, enabled: bool) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            apply_local_preference(settings, &LocalPreference::ReducedMotion(enabled))
        })
    }

    pub fn set_browse_filters(
        &mut self,
        filters: BrowseFilterSettings,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.library_filters = filters;
            Ok(())
        })
    }

    /// Saves a preference atomically while keeping unrelated live settings unchanged.
    pub fn set_local_preference(
        &mut self,
        preference: LocalPreference,
    ) -> Result<bool, SettingsMutationError> {
        self.update_isolated(|settings| apply_local_preference(settings, &preference))
    }

    /// Apply the same explicit preference to disk and live snapshots independently.
    /// Unrelated edits from another process remain on disk until their own lifecycle applies them.
    fn update_isolated(
        &mut self,
        mutation: impl Fn(&mut Settings) -> Result<(), SettingsMutationError>,
    ) -> Result<bool, SettingsMutationError> {
        let mut live = self.settings.clone();
        mutation(&mut live)?;
        let (mut disk, missing) = match read_from(&self.path) {
            Ok(settings) => (settings, false),
            Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                (self.settings.clone(), true)
            }
            Err(error) => return Err(error.into()),
        };
        let previous_disk = disk.clone();
        mutation(&mut disk)?;
        if missing || disk != previous_disk {
            save_to(&self.path, &disk)?;
        }
        let changed = live != self.settings;
        self.settings = live;
        Ok(changed)
    }

    fn update(
        &mut self,
        mutation: impl FnOnce(&mut Settings) -> Result<(), SettingsMutationError>,
    ) -> Result<bool, SettingsMutationError> {
        let mut candidate = read_from(&self.path).unwrap_or_else(|_| self.settings.clone());
        let previous = candidate.clone();
        mutation(&mut candidate)?;
        let changed = candidate != previous;
        if changed {
            save_to(&self.path, &candidate)?;
        }
        // Only explicit language selection commits this new field live. Preserve
        // pending disk language edits without importing them during other saves.
        candidate.ui_language = self.settings.ui_language;
        self.settings = candidate;
        Ok(changed)
    }
}

const fn default_progress_sync_seconds() -> u64 {
    10
}

fn default_key_next_episode() -> String {
    "Shift+>".to_owned()
}

fn default_key_previous_episode() -> String {
    "Shift+<".to_owned()
}

fn default_key_intro_skip() -> String {
    "g".to_owned()
}

const fn default_image_cache_enabled() -> bool {
    true
}

const fn default_remember_season_volume() -> bool {
    true
}

const fn default_auto_login() -> bool {
    true
}

fn non_empty_setting(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_mpv_args(value: &str) -> Vec<String> {
    value.split_whitespace().map(str::to_owned).collect()
}

fn valid_subtitle_language(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn binding_matches(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_str().map(str::to_owned))
}

fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect())
}

fn deserialize_key_next_episode<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_string_or(deserializer, default_key_next_episode)
}

fn deserialize_key_previous_episode<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_string_or(deserializer, default_key_previous_episode)
}

fn deserialize_key_intro_skip<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_string_or(deserializer, default_key_intro_skip)
}

fn deserialize_string_or<'de, D>(
    deserializer: D,
    fallback: fn() -> String,
) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(fallback, str::to_owned))
}

fn deserialize_image_cache_enabled<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_else(default_image_cache_enabled))
}

fn deserialize_remember_season_volume<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_bool()
        .unwrap_or_else(default_remember_season_volume))
}

fn deserialize_auto_login<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_else(default_auto_login))
}

fn deserialize_start_minimized<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_default())
}

/// Leniently decodes per-series intro records: entries with an invalid scope,
/// blank series id, or unknown mode are dropped rather than failing the whole
/// settings file.
fn deserialize_series_intro_modes<'de, D>(
    deserializer: D,
) -> Result<Vec<SeriesIntroPreference>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|value| serde_json::from_value::<SeriesIntroPreference>(value).ok())
        .filter(|record| !record.series_id.trim().is_empty())
        .collect())
}

fn deserialize_reduced_motion<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_default())
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "configuration I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "configuration JSON is invalid: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(feature = "native")]
fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(CONFIG_DIRECTORY)
        .join(CONFIG_FILE)
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn load_from(path: &Path) -> Result<Settings, ConfigError> {
    match read_and_migrate(path) {
        Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            Ok(Settings::default())
        }
        Ok((settings, migrated)) => {
            if migrated {
                save_to(path, &settings)?;
            }
            Ok(settings)
        }
        Err(error) => Err(error),
    }
}

fn read_from(path: &Path) -> Result<Settings, ConfigError> {
    Ok(read_and_migrate(path)?.0)
}

fn read_and_migrate(path: &Path) -> Result<(Settings, bool), ConfigError> {
    let contents = fs::read_to_string(path)?;
    let settings = serde_json::from_str(&contents)?;
    let (mut settings, migrated) = migrate_settings(settings);
    let normalized = normalize_series_intro_modes(&mut settings);
    Ok((settings, migrated || normalized))
}

fn migrate_settings(mut settings: Settings) -> (Settings, bool) {
    if settings.settings_revision >= CURRENT_SETTINGS_REVISION {
        return (settings, false);
    }
    #[cfg(target_os = "linux")]
    if settings.playback_backend == PlaybackBackend::External {
        settings.playback_backend = PlaybackBackend::Embedded;
    }
    settings.settings_revision = CURRENT_SETTINGS_REVISION;
    (settings, true)
}

/// Drops duplicate and global-matching per-series records so a stale file
/// cannot resurrect an override the global setting already covers. Returns
/// whether the settings changed.
fn normalize_series_intro_modes(settings: &mut Settings) -> bool {
    let records = &mut settings.series_intro_modes;
    let original_len = records.len();
    let mut kept = 0;
    for index in 0..original_len {
        let record = &records[index];
        if record.mode != settings.intro_mode
            && !records[..kept].iter().any(|existing| {
                existing.scope == record.scope && existing.series_id == record.series_id
            })
        {
            records.swap(kept, index);
            kept += 1;
        }
    }
    records.truncate(kept);
    kept != original_len
}

fn save_to(path: &Path, settings: &Settings) -> Result<(), ConfigError> {
    save_json_to(path, settings)
}

pub(crate) fn save_json_to<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> Result<(), ConfigError> {
    let contents = serde_json::to_string_pretty(value)?;
    if fs::read_to_string(path).ok().as_deref() == Some(contents.as_str()) {
        return Ok(());
    }
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let temporary = temporary_path(path);
    if let Err(error) = fs::write(&temporary, contents) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::UiLanguage;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jellypilot-settings-{}-{name}.json",
            std::process::id()
        ))
    }

    #[test]
    fn tv_preferences_preserve_disk_siblings_without_adopting_them_live() {
        let path = test_path("tv-isolated-preferences");
        let initial = Settings::default();
        save_to(&path, &initial).unwrap();
        let mut store = store_at(path.clone(), initial.clone());
        let mut external = initial.clone();
        external.app_mode = AppMode::ControlOnly;
        external.playback_backend = PlaybackBackend::External;
        external.username = "other-process".to_owned();
        external.mpv_args = vec!["--sub-font=Source Sans 3".to_owned()];
        save_to(&path, &external).unwrap();
        store.set_mpv_option("hwdec", "no").unwrap();
        store.set_auto_next_episode(false).unwrap();
        store.set_progress_sync_seconds(3).unwrap();
        store
            .set_local_preference(LocalPreference::ReducedMotion(true))
            .unwrap();
        assert_eq!(store.snapshot().app_mode(), initial.app_mode());
        assert_eq!(
            store.snapshot().playback_backend(),
            initial.playback_backend()
        );
        assert_eq!(store.snapshot().login_prefill(), initial.login_prefill());
        assert_eq!(store.snapshot().mpv_args(), &["--hwdec=no"]);
        let disk = load_from(&path).unwrap();
        assert_eq!(disk.app_mode(), external.app_mode());
        assert_eq!(disk.playback_backend(), external.playback_backend());
        assert_eq!(disk.mpv_args(), &["--sub-font=Source Sans 3", "--hwdec=no"]);
        assert!(!disk.auto_next_episode());
        assert_eq!(disk.progress_sync_seconds(), 3);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn mpv_preference_replaces_duplicate_and_separate_options_without_losing_other_arguments() {
        let path = test_path("tv-mpv-option");
        let initial = Settings {
            mpv_args: vec![
                "--title".to_owned(),
                "hwdec".to_owned(),
                "--hwdec".to_owned(),
                "auto".to_owned(),
                "--sub-font=Source Sans 3".to_owned(),
                "--hwdec=vaapi".to_owned(),
            ],
            ..Settings::default()
        };
        save_to(&path, &initial).unwrap();
        let mut store = store_at(path.clone(), initial);
        store.set_mpv_option("hwdec", "no").unwrap();
        assert_eq!(
            store.snapshot().mpv_args(),
            &["--title", "hwdec", "--sub-font=Source Sans 3", "--hwdec=no"]
        );
        assert!(store.set_mpv_option("hwdec", "no,pause=no").is_err());
        assert!(store.set_progress_sync_seconds(0).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn volume_memory_setting_recovers_legacy_values_and_persists_independently() {
        let path = test_path("season-volume-setting");
        let expected = remembered_settings();
        for invalid in [None, Some(serde_json::json!("false"))] {
            let mut value = serde_json::to_value(&expected).unwrap();
            let object = value.as_object_mut().unwrap();
            if let Some(invalid) = invalid {
                object.insert("remember_season_volume".to_owned(), invalid);
            } else {
                object.remove("remember_season_volume");
            }
            fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            let recovered = load_from(&path).unwrap();
            assert!(recovered.remember_season_volume());
            let mut store = store_at(path.clone(), recovered);
            assert!(store.set_remember_season_volume(false).unwrap());
            assert_eq!(load_from(&path).unwrap(), expected);
            assert!(!store.set_remember_season_volume(false).unwrap());
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn generic_mutations_preserve_live_language_and_pending_disk_language() {
        for changed in [false, true] {
            let path = test_path(if changed {
                "language-generic-change"
            } else {
                "language-generic-noop"
            });
            let live = Settings {
                ui_language: LanguagePreference::Fixed(UiLanguage::English),
                theme_mode: ThemeMode::Dark,
                ..Settings::default()
            };
            save_to(&path, &live).unwrap();
            let mut store = store_at(path.clone(), live);
            let mut disk = read_from(&path).unwrap();
            disk.ui_language = LanguagePreference::Fixed(UiLanguage::SimplifiedChinese);
            disk.app_mode = AppMode::ControlOnly;
            save_to(&path, &disk).unwrap();

            store
                .set_theme_mode(if changed {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                })
                .unwrap();
            assert_eq!(
                store.snapshot().ui_language(),
                LanguagePreference::Fixed(UiLanguage::English)
            );
            assert_eq!(store.snapshot().app_mode(), AppMode::ControlOnly);
            assert_eq!(
                read_from(&path).unwrap().ui_language(),
                LanguagePreference::Fixed(UiLanguage::SimplifiedChinese)
            );
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn playback_backend_defaults_to_embedded_on_linux() {
        if cfg!(target_os = "linux") {
            assert_eq!(PlaybackBackend::default(), PlaybackBackend::Embedded);
            assert_eq!(
                Settings::default().playback_backend(),
                PlaybackBackend::Embedded
            );
        } else {
            assert_eq!(PlaybackBackend::default(), PlaybackBackend::External);
        }
    }

    #[test]
    fn hdr_output_activation_requires_capability_and_respects_content() {
        for (mode, display_hdr10, content_hdr, expected) in [
            (HdrOutput::Auto, true, true, true),
            (HdrOutput::Auto, true, false, false),
            (HdrOutput::Auto, false, true, false),
            (HdrOutput::On, true, false, true),
            (HdrOutput::On, false, true, false),
            (HdrOutput::Off, true, true, false),
        ] {
            assert_eq!(mode.active(display_hdr10, content_hdr), expected);
        }
    }

    #[test]
    fn linux_legacy_external_settings_adopt_embedded_default_once() {
        let path = test_path("linux-embedded-default");
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            r#"{"remember":false,"server_url":"","provider":"","username":"","playback_backend":"external"}"#,
        )
        .unwrap();

        let loaded = load_from(&path).unwrap();
        if cfg!(target_os = "linux") {
            assert_eq!(loaded.playback_backend(), PlaybackBackend::Embedded);
            let mut store = store_at(path.clone(), loaded);
            assert!(store
                .set_playback_backend(PlaybackBackend::External)
                .unwrap());
            assert_eq!(
                load_from(&path).unwrap().playback_backend(),
                PlaybackBackend::External
            );
        } else {
            assert_eq!(loaded.playback_backend(), PlaybackBackend::External);
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn embedded_selection_preserves_external_configuration_across_reload() {
        let path = test_path("playback-backend");
        let original = remembered_settings();
        let mut store = store_at(path.clone(), original.clone());
        assert!(store
            .set_playback_backend(PlaybackBackend::Embedded)
            .unwrap());
        let embedded = read_from(&path).unwrap();
        assert_eq!(embedded.playback_backend(), PlaybackBackend::Embedded);
        assert_eq!(embedded.mpv_path(), original.mpv_path());
        assert_eq!(embedded.mpv_args(), original.mpv_args());
        assert!(store
            .set_playback_backend(PlaybackBackend::External)
            .unwrap());
        assert_eq!(read_from(&path).unwrap(), original);
        fs::remove_file(path).unwrap();
    }

    fn remembered_settings() -> Settings {
        Settings {
            remember: true,
            server_url: "https://media.example.com".to_owned(),
            provider: "jellyfin".to_owned(),
            username: "alice".to_owned(),
            auto_login: false,
            intro_mode: IntroMode::Manual,
            auto_next_episode: true,
            progress_sync_seconds: default_progress_sync_seconds(),
            series_intro_modes: Vec::new(),
            theme_mode: ThemeMode::Dark,
            ui_language: LanguagePreference::System,
            app_mode: AppMode::ControlOnly,
            ui_mode: UiMode::Desktop,
            tmdb_api_key: Some("tmdb-key".to_owned()),
            playback_backend: PlaybackBackend::External,
            hdr_output: HdrOutput::On,
            settings_revision: CURRENT_SETTINGS_REVISION,
            mpv_path: Some("/usr/bin/mpv".to_owned()),
            mpv_args: vec!["--fullscreen".to_owned(), "--profile=gpu-hq".to_owned()],
            playback_target_name: Some("Living Room".to_owned()),
            subtitle_languages: vec!["eng".to_owned(), "spa".to_owned()],
            key_next_episode: "N".to_owned(),
            key_previous_episode: "P".to_owned(),
            key_intro_skip: "I".to_owned(),
            image_cache_enabled: false,
            remember_season_volume: false,
            start_minimized: true,
            reduced_motion: false,
            prefer_original_audio: true,
            library_filters: BrowseFilterSettings::default()
                .with_sort(VideoLibrarySort::ReleaseDate)
                .with_played_filter(VideoLibraryPlayedFilter::Unplayed)
                .with_favorites_only(true)
                .with_sort_direction(VideoLibrarySortDirection::Descending),
        }
    }

    fn store_at(path: PathBuf, settings: Settings) -> SettingsStore {
        SettingsStore { path, settings }
    }

    #[test]
    fn invalid_or_missing_language_recovers_without_losing_other_settings() {
        let expected = remembered_settings();
        for invalid in [
            None,
            Some(serde_json::json!("zh-Hant")),
            Some(serde_json::json!(null)),
            Some(serde_json::json!(17)),
            Some(serde_json::json!(["en-US"])),
            Some(serde_json::json!({"language": "en-US"})),
        ] {
            let mut value = serde_json::to_value(&expected).unwrap();
            let object = value.as_object_mut().unwrap();
            if let Some(invalid) = invalid {
                object.insert("ui_language".to_owned(), invalid);
            } else {
                object.remove("ui_language");
            }
            let recovered: Settings = serde_json::from_value(value).unwrap();
            assert_eq!(recovered, expected);
        }
    }

    #[test]
    fn language_save_preserves_independent_disk_and_live_settings() {
        let path = test_path("language-isolated-commit");
        let mut live = remembered_settings();
        live.app_mode = AppMode::Full;
        let mut disk = live.clone();
        disk.app_mode = AppMode::ControlOnly;
        disk.subtitle_languages = vec!["zho".to_owned()];
        disk.username = "disk-user".to_owned();
        save_to(&path, &disk).unwrap();
        let mut store = store_at(path.clone(), live.clone());
        let preference = LanguagePreference::Fixed(UiLanguage::SimplifiedChinese);

        assert!(store.set_ui_language(preference).unwrap());

        live.ui_language = preference;
        disk.ui_language = preference;
        assert_eq!(store.snapshot(), &live);
        assert_eq!(load_from(&path).unwrap(), disk);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn disk_equal_language_commits_live_without_rewriting_settings() {
        let path = test_path("language-disk-already-selected");
        let mut live = remembered_settings();
        let preference = LanguagePreference::Fixed(UiLanguage::English);
        let mut disk = live.clone();
        disk.ui_language = preference;
        disk.subtitle_languages = vec!["fra".to_owned()];
        save_to(&path, &disk).unwrap();
        let persisted = fs::read(&path).unwrap();
        let temporary = temporary_path(&path);
        fs::create_dir(&temporary).unwrap();
        let mut store = store_at(path.clone(), live.clone());

        assert!(store.set_ui_language(preference).unwrap());
        live.ui_language = preference;
        assert_eq!(store.snapshot(), &live);
        assert_eq!(fs::read(&path).unwrap(), persisted);
        assert!(!store.set_ui_language(preference).unwrap());
        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unchanged_live_language_repairs_disk_without_reporting_a_live_change() {
        let path = test_path("language-live-already-selected");
        let live = remembered_settings();
        let mut disk = live.clone();
        disk.ui_language = LanguagePreference::Fixed(UiLanguage::English);
        save_to(&path, &disk).unwrap();
        let mut store = store_at(path.clone(), live.clone());

        assert!(!store.set_ui_language(LanguagePreference::System).unwrap());
        assert_eq!(load_from(&path).unwrap(), live);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_language_save_keeps_the_live_snapshot_and_disk_intact() {
        let path = test_path("language-save-failure");
        let live = remembered_settings();
        save_to(&path, &live).unwrap();
        let temporary = temporary_path(&path);
        // A staging-path directory forces a write failure even under privileged tests.
        fs::create_dir(&temporary).unwrap();
        let mut store = store_at(path.clone(), live.clone());

        let result = store.set_ui_language(LanguagePreference::Fixed(UiLanguage::English));

        assert!(matches!(result, Err(SettingsMutationError::Config(_))));
        assert_eq!(store.snapshot(), &live);
        assert_eq!(load_from(&path).unwrap(), live);
        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn language_reselection_recreates_a_removed_settings_file() {
        let path = test_path("language-missing-file");
        let mut live = remembered_settings();
        live.ui_language = LanguagePreference::Fixed(UiLanguage::SimplifiedChinese);
        save_to(&path, &live).unwrap();
        let mut store = store_at(path.clone(), live.clone());
        fs::remove_file(&path).unwrap();

        assert!(!store.set_ui_language(live.ui_language).unwrap());
        assert_eq!(load_from(&path).unwrap(), live);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn language_save_does_not_replace_unreadable_settings_from_a_stale_snapshot() {
        let path = test_path("language-invalid-file");
        let live = remembered_settings();
        let disk = b"{\"app_mode\":\"control-only\",\"subtitle_languages\":[\"zho\"]";
        fs::write(&path, disk).unwrap();
        let mut store = store_at(path.clone(), live.clone());

        let result = store.set_ui_language(LanguagePreference::Fixed(UiLanguage::English));

        assert!(matches!(
            result,
            Err(SettingsMutationError::Config(ConfigError::Json(_)))
        ));
        assert_eq!(store.snapshot(), &live);
        assert_eq!(fs::read(&path).unwrap(), disk);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_config_defaults_new_application_settings() {
        let path = test_path("legacy");
        let _ = fs::remove_file(&path);
        fs::write(
      &path,
      r#"{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice"}"#,
    )
    .unwrap();

        let settings = load_from(&path).unwrap();

        assert_eq!(settings.intro_mode(), IntroMode::Automatic);
        assert_eq!(settings.theme_mode(), ThemeMode::System);
        assert_eq!(settings.app_mode(), AppMode::Full);
        assert!(settings.auto_login());
        assert_eq!(settings.mpv_path(), None);
        assert!(settings.mpv_args().is_empty());
        assert_eq!(settings.playback_target_name(), None);
        assert!(settings.subtitle_languages().is_empty());
        assert_eq!(settings.key_next_episode(), "Shift+>");
        assert_eq!(settings.key_previous_episode(), "Shift+<");
        assert_eq!(settings.key_intro_skip(), "g");
        assert!(settings.image_cache_enabled());
        assert!(!settings.start_minimized());
        assert!(!settings.reduced_motion());
        assert_eq!(settings.browse_filters(), BrowseFilterSettings::default());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn theme_mode_is_persisted_and_defaults_to_system() {
        let path = test_path("theme-mode");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert_eq!(Settings::default().theme_mode(), ThemeMode::System);
        assert!(store.set_theme_mode(ThemeMode::Light).unwrap());
        assert_eq!(load_from(&path).unwrap().theme_mode(), ThemeMode::Light);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unknown_theme_mode_defaults_to_system() {
        let path = test_path("unknown-theme-mode");
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            r#"{"remember":false,"server_url":"","provider":"","username":"","theme_mode":"neon"}"#,
        )
        .unwrap();

        assert_eq!(load_from(&path).unwrap().theme_mode(), ThemeMode::System);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn theme_mode_serializes_as_lowercase_strings() {
        let settings = Settings {
            theme_mode: ThemeMode::Light,
            ..Settings::default()
        };

        let contents = serde_json::to_string(&settings).unwrap();

        assert!(contents.contains(r#""theme_mode":"light""#));
    }

    #[test]
    fn auto_login_serde_defaults_true_and_preserves_false() {
        let missing: Settings = serde_json::from_str(
            r#"{"remember":false,"server_url":"","provider":"","username":""}"#,
        )
        .unwrap();
        let explicit_false: Settings = serde_json::from_str(
            r#"{"remember":false,"server_url":"","provider":"","username":"","auto_login":false}"#,
        )
        .unwrap();
        let malformed: Settings = serde_json::from_str(
            r#"{"remember":false,"server_url":"","provider":"","username":"","auto_login":"sometimes"}"#,
        )
        .unwrap();

        assert!(missing.auto_login());
        assert!(!explicit_false.auto_login());
        assert!(malformed.auto_login());
    }

    #[test]
    fn auto_login_is_persisted() {
        let path = test_path("auto-login");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert!(store.set_auto_login(false).unwrap());
        assert!(!load_from(&path).unwrap().auto_login());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn app_mode_is_persisted_and_defaults_to_full() {
        let path = test_path("app-mode");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert_eq!(Settings::default().app_mode(), AppMode::Full);
        assert!(store.set_app_mode(AppMode::ControlOnly).unwrap());
        assert_eq!(load_from(&path).unwrap().app_mode(), AppMode::ControlOnly);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unknown_app_mode_defaults_to_full() {
        let path = test_path("unknown-app-mode");
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            r#"{"remember":false,"server_url":"","provider":"","username":"","app_mode":"theater"}"#,
        )
        .unwrap();

        assert_eq!(load_from(&path).unwrap().app_mode(), AppMode::Full);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn ui_mode_recovers_old_settings_and_persists_without_changing_capabilities() {
        let path = test_path("ui-mode");
        let expected = remembered_settings();
        for invalid in [
            None,
            Some(serde_json::json!("cinema")),
            Some(serde_json::json!(true)),
        ] {
            let mut value = serde_json::to_value(&expected).unwrap();
            let object = value.as_object_mut().unwrap();
            if let Some(invalid) = invalid {
                object.insert("ui_mode".to_owned(), invalid);
            } else {
                object.remove("ui_mode");
            }
            fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            let recovered = load_from(&path).unwrap();
            assert_eq!(recovered, expected);
            let mut store = store_at(path.clone(), recovered);
            assert!(store.set_ui_mode(UiMode::Tv).unwrap());
            assert!(!store.set_ui_mode(UiMode::Tv).unwrap());
            let mut tv_settings = expected.clone();
            tv_settings.ui_mode = UiMode::Tv;
            assert_eq!(load_from(&path).unwrap(), tv_settings);
            assert!(store.set_ui_mode(UiMode::Desktop).unwrap());
            assert_eq!(load_from(&path).unwrap(), expected);
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn ui_mode_save_preserves_independent_disk_and_live_capabilities() {
        let path = test_path("ui-mode-isolated-commit");
        for disk_mode in [UiMode::Desktop, UiMode::Tv] {
            let mut live = remembered_settings();
            let mut disk = live.clone();
            disk.ui_mode = disk_mode;
            disk.app_mode = AppMode::Full;
            disk.playback_backend = PlaybackBackend::Embedded;
            disk.username = "disk-user".to_owned();
            save_to(&path, &disk).unwrap();
            let mut store = store_at(path.clone(), live.clone());

            assert!(store.set_ui_mode(UiMode::Tv).unwrap());

            live.ui_mode = UiMode::Tv;
            disk.ui_mode = UiMode::Tv;
            assert_eq!(store.snapshot(), &live);
            assert_eq!(load_from(&path).unwrap(), disk);
            assert!(!store.set_ui_mode(UiMode::Tv).unwrap());
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn app_mode_serializes_as_lowercase_strings() {
        let settings = Settings {
            app_mode: AppMode::ControlOnly,
            ..Settings::default()
        };

        let contents = serde_json::to_string(&settings).unwrap();

        assert!(contents.contains(r#""app_mode":"controlonly""#));
    }

    #[test]
    fn start_minimized_is_persisted_and_defaults_false() {
        let path = test_path("start-minimized");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert!(store.set_start_minimized(true).unwrap());
        assert!(load_from(&path).unwrap().start_minimized());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reduced_motion_is_persisted_and_defaults_false() {
        let path = test_path("reduced-motion");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert!(!Settings::default().reduced_motion());
        assert!(store.set_reduced_motion(true).unwrap());
        assert!(load_from(&path).unwrap().reduced_motion());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn tmdb_api_key_is_persisted_and_blank_becomes_none() {
        let path = test_path("tmdb-key");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert_eq!(Settings::default().tmdb_api_key(), None);
        assert!(store.set_tmdb_api_key("  tmdb-key-1  ".to_owned()).unwrap());
        assert_eq!(load_from(&path).unwrap().tmdb_api_key(), Some("tmdb-key-1"));
        assert!(store.set_tmdb_api_key("   ".to_owned()).unwrap());
        assert_eq!(load_from(&path).unwrap().tmdb_api_key(), None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn prefer_original_audio_is_persisted_and_defaults_false() {
        let path = test_path("original-audio");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert!(!Settings::default().prefer_original_audio());
        assert!(store.set_prefer_original_audio(true).unwrap());
        assert!(load_from(&path).unwrap().prefer_original_audio());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn malformed_new_settings_preserve_valid_login_and_use_total_fallbacks() {
        let path = test_path("malformed-settings");
        let _ = fs::remove_file(&path);
        fs::write(
      &path,
      r#"{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice","mpv_path":42,"mpv_args":"bad","playback_target_name":[],"subtitle_languages":false,"key_next_episode":null,"key_previous_episode":3,"key_intro_skip":{},"image_cache_enabled":"yes"}"#,
    )
    .unwrap();

        let settings = load_from(&path).unwrap();

        assert!(settings.remembers_login_prefill());
        assert_eq!(
            settings.login_prefill().server_url(),
            "https://media.example.com"
        );
        assert_eq!(settings.login_prefill().username(), "alice");
        assert_eq!(settings.mpv_path(), None);
        assert!(settings.mpv_args().is_empty());
        assert_eq!(settings.playback_target_name(), None);
        assert!(settings.subtitle_languages().is_empty());
        assert_eq!(settings.key_next_episode(), "Shift+>");
        assert_eq!(settings.key_previous_episode(), "Shift+<");
        assert_eq!(settings.key_intro_skip(), "g");
        assert!(settings.image_cache_enabled());
        assert_eq!(settings.browse_filters(), BrowseFilterSettings::default());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn malformed_browse_filters_fall_back_field_by_field() {
        let path = test_path("malformed-browse-filters");
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            r#"{"remember":false,"server_url":"","provider":"","username":"","library_filters":{"sort":"releaseDate","playedFilter":"invalid","favoritesOnly":"yes","sortDirection":"desc"}}"#,
        )
        .unwrap();

        let filters = load_from(&path).unwrap().browse_filters();

        assert_eq!(
            filters,
            BrowseFilterSettings::default()
                .with_sort(VideoLibrarySort::ReleaseDate)
                .with_sort_direction(VideoLibrarySortDirection::Descending)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn browse_filter_mutation_persists_validated_shape_and_maps_to_source_preferences() {
        let path = test_path("browse-filter-persistence");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());
        let filters = BrowseFilterSettings::default()
            .with_sort(VideoLibrarySort::RecentlyAdded)
            .with_played_filter(VideoLibraryPlayedFilter::Played)
            .with_favorites_only(true)
            .with_sort_direction(VideoLibrarySortDirection::Descending);

        assert!(store.set_browse_filters(filters).unwrap());

        let saved = load_from(&path).unwrap().browse_filters();
        let preferences = BrowsePreferences::from(saved);
        assert_eq!(saved, filters);
        assert_eq!(preferences.sort, VideoLibrarySort::RecentlyAdded);
        assert_eq!(
            preferences.sort_direction,
            VideoLibrarySortDirection::Descending
        );
        assert_eq!(preferences.played_filter, VideoLibraryPlayedFilter::Played);
        assert!(preferences.favorites_only);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_and_unrelated_mutation_preserve_legacy_strict_values() {
        let path = test_path("legacy-strict-values");
        let _ = fs::remove_file(&path);
        fs::write(
      &path,
      r#"{"remember":false,"server_url":"","provider":"","username":"","subtitle_languages":["English (CC)"],"key_next_episode":"x","key_previous_episode":"X","key_intro_skip":"g"}"#,
    )
    .unwrap();

        let settings = load_from(&path).unwrap();

        assert_eq!(settings.subtitle_languages(), &["English (CC)"]);
        assert_eq!(settings.key_next_episode(), "x");
        assert_eq!(settings.key_previous_episode(), "X");
        let mut store = store_at(path.clone(), settings);
        assert!(store.set_intro_mode(IntroMode::Manual).unwrap());
        let saved = load_from(&path).unwrap();
        assert_eq!(saved.subtitle_languages(), &["English (CC)"]);
        assert_eq!(saved.key_next_episode(), "x");
        assert_eq!(saved.key_previous_episode(), "X");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn new_subtitle_language_mutation_rejects_legacy_invalid_value() {
        let path = test_path("invalid-new-language");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path, Settings::default());

        assert!(matches!(
            store.add_subtitle_language("English (CC)".to_owned()),
            Err(SettingsMutationError::InvalidSubtitleLanguage)
        ));
    }

    #[test]
    fn new_shortcut_mutation_rejects_legacy_collision() {
        let path = test_path("invalid-new-shortcut");
        let _ = fs::remove_file(&path);
        let settings = Settings {
            key_previous_episode: "X".to_owned(),
            ..Settings::default()
        };
        let mut store = store_at(path, settings);

        assert!(matches!(
            store.set_shortcut(ShortcutKind::Next, "x".to_owned()),
            Err(SettingsMutationError::ShortcutCollision)
        ));
    }

    #[test]
    fn unknown_intro_mode_defaults_to_automatic() {
        let path = test_path("unknown-intro-mode");
        let _ = fs::remove_file(&path);
        fs::write(
      &path,
      r#"{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice","intro_mode":"invalid"}"#,
    )
    .unwrap();

        assert_eq!(load_from(&path).unwrap().intro_mode(), IntroMode::Automatic);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_off_intro_mode_migrates_to_manual() {
        let path = test_path("legacy-off-intro-mode");
        let _ = fs::remove_file(&path);
        fs::write(
      &path,
      r#"{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice","intro_mode":"off"}"#,
    )
    .unwrap();

        assert_eq!(load_from(&path).unwrap().intro_mode(), IntroMode::Manual);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn malformed_start_minimized_defaults_without_discarding_login_fields() {
        let path = test_path("malformed-start-minimized");
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            r#"{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice","start_minimized":"sometimes"}"#,
        )
        .unwrap();

        let settings = load_from(&path).unwrap();

        assert!(!settings.start_minimized());
        assert!(settings.remembers_login_prefill());
        assert_eq!(
            settings.login_prefill().server_url(),
            "https://media.example.com"
        );
        assert_eq!(settings.login_prefill().username(), "alice");
        assert_eq!(settings.login_provider(), "jellyfin");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_config_defaults_to_empty_settings() {
        let path = test_path("missing");
        let _ = fs::remove_file(&path);
        assert_eq!(load_from(&path).unwrap(), Settings::default());
    }

    #[test]
    fn config_round_trip_excludes_credentials_and_preserves_settings() {
        let path = test_path("round-trip");
        let _ = fs::remove_file(&path);
        let settings = remembered_settings();
        save_to(&path, &settings).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(!contents.contains("password"));
        assert_eq!(load_from(&path).unwrap(), settings);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn clearing_remembered_login_preserves_application_settings() {
        let path = test_path("clear");
        let _ = fs::remove_file(&path);
        let settings = remembered_settings();
        save_to(&path, &settings).unwrap();
        let mut store = store_at(path.clone(), settings.clone());

        assert!(store.clear_login_prefill().unwrap());

        let mut expected = settings;
        expected.remember = false;
        expected.server_url.clear();
        expected.provider.clear();
        expected.username.clear();
        assert_eq!(load_from(&path).unwrap(), expected);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn mutation_merges_unrelated_on_disk_edits_after_startup() {
        let path = test_path("external-edit");
        let _ = fs::remove_file(&path);
        let startup = remembered_settings();
        save_to(&path, &startup).unwrap();
        let mut store = store_at(path.clone(), startup.clone());
        let mut external = startup;
        external.playback_target_name = Some("Bedroom".to_owned());
        save_to(&path, &external).unwrap();

        assert!(store.set_intro_mode(IntroMode::Automatic).unwrap());

        external.intro_mode = IntroMode::Automatic;
        assert_eq!(load_from(&path).unwrap(), external);
        assert_eq!(store.snapshot(), &external);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn mutations_parse_and_validate_before_saving() {
        let path = test_path("validated-mutations");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert!(store
            .set_mpv_args(" --fullscreen   --profile=gpu-hq ")
            .unwrap());
        assert!(store.add_subtitle_language(" PT-BR ".to_owned()).unwrap());
        assert!(matches!(
            store.add_subtitle_language("eng,spa".to_owned()),
            Err(SettingsMutationError::InvalidSubtitleLanguage)
        ));
        assert!(matches!(
            store.add_subtitle_language("pt-br".to_owned()),
            Err(SettingsMutationError::DuplicateSubtitleLanguage)
        ));
        assert!(matches!(
            store.set_shortcut(ShortcutKind::Next, " shift+< ".to_owned()),
            Err(SettingsMutationError::ShortcutCollision)
        ));
        assert_eq!(
            load_from(&path).unwrap().mpv_args(),
            &["--fullscreen", "--profile=gpu-hq"]
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unchanged_mutation_does_not_save() {
        let path = test_path("unchanged");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());

        assert!(!store.set_intro_mode(IntroMode::Automatic).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn failed_atomic_write_preserves_snapshot_and_existing_config() {
        let path = test_path("atomic-failure");
        let temporary = temporary_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&temporary);
        let original = remembered_settings();
        save_to(&path, &original).unwrap();
        fs::create_dir(&temporary).unwrap();
        let mut store = store_at(path.clone(), original.clone());

        assert!(store.set_intro_mode(IntroMode::Automatic).is_err());
        assert_eq!(store.snapshot(), &original);
        assert_eq!(load_from(&path).unwrap(), original);
        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }

    fn test_scope(user_id: &str) -> ProfileScope {
        ProfileScope::new(
            jellypilot_media_server::MediaServerProvider::Jellyfin,
            "https://media.example.com",
            user_id,
        )
        .unwrap()
    }

    #[test]
    fn series_intro_mode_persists_per_scope_and_clears_on_global_choice() {
        let path = test_path("series-intro-persist");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());
        let scope_a = test_scope("user-a");
        let scope_b = test_scope("user-b");

        assert!(store
            .set_series_intro_mode("series-1", &scope_a, IntroMode::Manual)
            .unwrap());
        assert!(!store
            .set_series_intro_mode("series-1", &scope_b, IntroMode::Automatic)
            .unwrap());

        let reloaded = load_from(&path).unwrap();
        assert_eq!(
            reloaded.series_intro_mode("series-1", &scope_a),
            IntroMode::Manual
        );
        // The scope-b record matched the global and was never pinned.
        assert_eq!(
            reloaded.series_intro_mode("series-1", &scope_b),
            IntroMode::Automatic
        );
        assert_eq!(
            reloaded.series_intro_mode("other-series", &scope_a),
            IntroMode::Automatic
        );

        // Selecting the global value clears the override.
        assert!(store
            .set_series_intro_mode("series-1", &scope_a, IntroMode::Automatic)
            .unwrap());
        assert_eq!(
            load_from(&path)
                .unwrap()
                .series_intro_mode("series-1", &scope_a),
            IntroMode::Automatic
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn global_intro_change_normalizes_every_scope_atomically() {
        let path = test_path("series-intro-global-normalize");
        let _ = fs::remove_file(&path);
        let mut store = store_at(path.clone(), Settings::default());
        let scope_a = test_scope("user-a");
        let scope_b = test_scope("user-b");
        store
            .set_series_intro_mode("series-1", &scope_a, IntroMode::Manual)
            .unwrap();
        store
            .set_series_intro_mode("series-2", &scope_b, IntroMode::Manual)
            .unwrap();

        // Global Manual clears both scopes' Manual records in the same write.
        assert!(store.set_intro_mode(IntroMode::Manual).unwrap());
        let reloaded = load_from(&path).unwrap();
        assert_eq!(
            reloaded.series_intro_mode("series-1", &scope_a),
            IntroMode::Manual
        );
        assert_eq!(
            reloaded.series_intro_mode("series-2", &scope_b),
            IntroMode::Manual
        );

        // Back to Automatic: no stale record resurrects an override.
        assert!(store.set_intro_mode(IntroMode::Automatic).unwrap());
        let reloaded = load_from(&path).unwrap();
        assert_eq!(
            reloaded.series_intro_mode("series-1", &scope_a),
            IntroMode::Automatic
        );
        assert_eq!(
            reloaded.series_intro_mode("series-2", &scope_b),
            IntroMode::Automatic
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_global_write_preserves_settings_and_overrides() {
        let path = test_path("series-intro-atomic-failure");
        let temporary = temporary_path(&path);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&temporary);
        let mut store = store_at(path.clone(), Settings::default());
        let scope = test_scope("user-a");
        store
            .set_series_intro_mode("series-1", &scope, IntroMode::Manual)
            .unwrap();
        let original = store.snapshot().clone();
        fs::create_dir(&temporary).unwrap();

        assert!(store.set_intro_mode(IntroMode::Manual).is_err());
        assert_eq!(store.snapshot(), &original);
        let reloaded = load_from(&path).unwrap();
        assert_eq!(reloaded.intro_mode(), IntroMode::Automatic);
        assert_eq!(
            reloaded.series_intro_mode("series-1", &scope),
            IntroMode::Manual
        );
        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn loading_normalizes_stale_series_records() {
        let path = test_path("series-intro-load-normalize");
        let _ = fs::remove_file(&path);
        let scope = test_scope("user-a");
        let settings = Settings {
            series_intro_modes: vec![
                SeriesIntroPreference {
                    scope: scope.clone(),
                    series_id: "series-1".to_owned(),
                    mode: IntroMode::Automatic, // matches global: stale
                },
                SeriesIntroPreference {
                    scope: scope.clone(),
                    series_id: "series-2".to_owned(),
                    mode: IntroMode::Manual,
                },
                SeriesIntroPreference {
                    scope,
                    series_id: "series-2".to_owned(),
                    mode: IntroMode::Automatic, // duplicate: dropped
                },
            ],
            ..Settings::default()
        };
        save_to(&path, &settings).unwrap();

        let loaded = load_from(&path).unwrap();
        assert_eq!(
            loaded.series_intro_mode("series-1", &test_scope("user-a")),
            IntroMode::Automatic
        );
        assert_eq!(
            loaded.series_intro_mode("series-2", &test_scope("user-a")),
            IntroMode::Manual
        );
        fs::remove_file(path).unwrap();
    }
}
