use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jellypilot_media_server::{
    VideoLibraryPlayedFilter, VideoLibrarySort, VideoLibrarySortDirection,
};
use serde::{Deserialize, Serialize};

use crate::browse_model::BrowsePreferences;

const CONFIG_DIRECTORY: &str = "jellypilot";
const CONFIG_FILE: &str = "config.json";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IntroMode {
    #[default]
    Automatic,
    Manual,
    Off,
}

impl<'de> Deserialize<'de> for IntroMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some(mode) if mode.eq_ignore_ascii_case("manual") => Self::Manual,
            Some(mode) if mode.eq_ignore_ascii_case("off") => Self::Off,
            _ => Self::Automatic,
        })
    }
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
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Settings {
    remember: bool,
    server_url: String,
    provider: String,
    username: String,
    #[serde(default)]
    intro_mode: IntroMode,
    #[serde(default)]
    theme_mode: ThemeMode,
    #[serde(default)]
    app_mode: AppMode,
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
    #[serde(default, deserialize_with = "deserialize_start_minimized")]
    start_minimized: bool,
    #[serde(default, deserialize_with = "deserialize_reduced_motion")]
    reduced_motion: bool,
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
            intro_mode: IntroMode::Automatic,
            theme_mode: ThemeMode::System,
            app_mode: AppMode::Full,
            mpv_path: None,
            mpv_args: Vec::new(),
            playback_target_name: None,
            subtitle_languages: Vec::new(),
            key_next_episode: default_key_next_episode(),
            key_previous_episode: default_key_previous_episode(),
            key_intro_skip: default_key_intro_skip(),
            image_cache_enabled: default_image_cache_enabled(),
            start_minimized: false,
            reduced_motion: false,
            library_filters: BrowseFilterSettings::default(),
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

    pub const fn intro_mode(&self) -> IntroMode {
        self.intro_mode
    }
    pub const fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }
    pub const fn app_mode(&self) -> AppMode {
        self.app_mode
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

    pub const fn start_minimized(&self) -> bool {
        self.start_minimized
    }

    pub const fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    pub const fn browse_filters(&self) -> BrowseFilterSettings {
        self.library_filters
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

pub struct SettingsStore {
    path: PathBuf,
    settings: Settings,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self {
            path: config_path(),
            settings: Settings::default(),
        }
    }
}

impl SettingsStore {
    pub fn load() -> Result<Self, ConfigError> {
        let path = config_path();
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

    pub fn set_intro_mode(&mut self, mode: IntroMode) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.intro_mode = mode;
            Ok(())
        })
    }
    pub fn set_theme_mode(&mut self, mode: ThemeMode) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.theme_mode = mode;
            Ok(())
        })
    }
    pub fn set_app_mode(&mut self, mode: AppMode) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.app_mode = mode;
            Ok(())
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

    pub fn set_playback_target_name(
        &mut self,
        name: String,
    ) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.playback_target_name = non_empty_setting(name);
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
            settings.image_cache_enabled = enabled;
            Ok(())
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

    pub fn set_reduced_motion(&mut self, enabled: bool) -> Result<bool, SettingsMutationError> {
        self.update(|settings| {
            settings.reduced_motion = enabled;
            Ok(())
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

    fn update(
        &mut self,
        mutation: impl FnOnce(&mut Settings) -> Result<(), SettingsMutationError>,
    ) -> Result<bool, SettingsMutationError> {
        let mut candidate = read_from(&self.path).unwrap_or_else(|_| self.settings.clone());
        let previous = candidate.clone();
        mutation(&mut candidate)?;
        if candidate == previous {
            self.settings = candidate;
            return Ok(false);
        }
        save_to(&self.path, &candidate)?;
        self.settings = candidate;
        Ok(true)
    }
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

fn deserialize_start_minimized<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_default())
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
    match read_from(path) {
        Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            Ok(Settings::default())
        }
        result => result,
    }
}

fn read_from(path: &Path) -> Result<Settings, ConfigError> {
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

fn save_to(path: &Path, settings: &Settings) -> Result<(), ConfigError> {
    let contents = serde_json::to_string_pretty(settings)?;
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

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jellypilot-settings-{}-{name}.json",
            std::process::id()
        ))
    }

    fn remembered_settings() -> Settings {
        Settings {
            remember: true,
            server_url: "https://media.example.com".to_owned(),
            provider: "jellyfin".to_owned(),
            username: "alice".to_owned(),
            intro_mode: IntroMode::Manual,
            theme_mode: ThemeMode::Dark,
            app_mode: AppMode::ControlOnly,
            mpv_path: Some("/usr/bin/mpv".to_owned()),
            mpv_args: vec!["--fullscreen".to_owned(), "--profile=gpu-hq".to_owned()],
            playback_target_name: Some("Living Room".to_owned()),
            subtitle_languages: vec!["eng".to_owned(), "spa".to_owned()],
            key_next_episode: "N".to_owned(),
            key_previous_episode: "P".to_owned(),
            key_intro_skip: "I".to_owned(),
            image_cache_enabled: false,
            start_minimized: true,
            reduced_motion: false,
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

        assert!(store.set_intro_mode(IntroMode::Off).unwrap());

        external.intro_mode = IntroMode::Off;
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

        assert!(store.set_intro_mode(IntroMode::Off).is_err());
        assert_eq!(store.snapshot(), &original);
        assert_eq!(load_from(&path).unwrap(), original);
        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }
}
