use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const CONFIG_DIRECTORY: &str = "jellypilot";
const CONFIG_FILE: &str = "config.json";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum IntroMode {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoginPrefill {
  server_url: String,
  username: String,
}

impl LoginPrefill {
  pub(crate) fn new(server_url: String, username: String) -> Self {
    Self {
      server_url,
      username,
    }
  }

  pub(crate) fn server_url(&self) -> &str {
    &self.server_url
  }

  pub(crate) fn username(&self) -> &str {
    &self.username
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Settings {
  remember: bool,
  server_url: String,
  provider: String,
  username: String,
  #[serde(default)]
  intro_mode: IntroMode,
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
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      remember: false,
      server_url: String::new(),
      provider: String::new(),
      username: String::new(),
      intro_mode: IntroMode::Automatic,
      mpv_path: None,
      mpv_args: Vec::new(),
      playback_target_name: None,
      subtitle_languages: Vec::new(),
      key_next_episode: default_key_next_episode(),
      key_previous_episode: default_key_previous_episode(),
      key_intro_skip: default_key_intro_skip(),
      image_cache_enabled: default_image_cache_enabled(),
    }
  }
}

impl Settings {
  pub(crate) fn login_prefill(&self) -> LoginPrefill {
    LoginPrefill::new(self.server_url.clone(), self.username.clone())
  }

  pub(crate) fn remembers_login_prefill(&self) -> bool {
    self.remember
  }

  pub(crate) fn login_provider(&self) -> &str {
    &self.provider
  }

  pub(crate) const fn intro_mode(&self) -> IntroMode {
    self.intro_mode
  }

  pub(crate) fn mpv_path(&self) -> Option<&str> {
    self.mpv_path.as_deref()
  }

  pub(crate) fn mpv_args(&self) -> &[String] {
    &self.mpv_args
  }

  pub(crate) fn playback_target_name(&self) -> Option<&str> {
    self.playback_target_name.as_deref()
  }

  pub(crate) fn subtitle_languages(&self) -> &[String] {
    &self.subtitle_languages
  }

  pub(crate) fn key_next_episode(&self) -> &str {
    &self.key_next_episode
  }

  pub(crate) fn key_previous_episode(&self) -> &str {
    &self.key_previous_episode
  }

  pub(crate) fn key_intro_skip(&self) -> &str {
    &self.key_intro_skip
  }

  pub(crate) const fn image_cache_enabled(&self) -> bool {
    self.image_cache_enabled
  }

  fn validate(&mut self) {
    if !self.remember {
      self.server_url.clear();
      self.provider.clear();
      self.username.clear();
    } else {
      self.server_url = self.server_url.trim().to_owned();
      self.username = self.username.trim().to_owned();
      if self.server_url.is_empty() || self.username.is_empty() {
        self.remember = false;
        self.server_url.clear();
        self.provider.clear();
        self.username.clear();
      }
    }
    self.mpv_path = self.mpv_path.take().and_then(non_empty_setting);
    self.playback_target_name = self.playback_target_name.take().and_then(non_empty_setting);
    self.mpv_args = self
      .mpv_args
      .drain(..)
      .filter_map(non_empty_setting)
      .collect();

    let mut languages = Vec::with_capacity(self.subtitle_languages.len());
    for language in self.subtitle_languages.drain(..) {
      let language = language.trim().to_ascii_lowercase();
      if valid_subtitle_language(&language)
        && !languages
          .iter()
          .any(|existing: &String| existing.eq_ignore_ascii_case(&language))
      {
        languages.push(language);
      }
    }
    self.subtitle_languages = languages;

    self.key_next_episode = non_empty_setting(std::mem::take(&mut self.key_next_episode))
      .unwrap_or_else(default_key_next_episode);
    self.key_previous_episode = non_empty_setting(std::mem::take(&mut self.key_previous_episode))
      .unwrap_or_else(default_key_previous_episode);
    self.key_intro_skip = non_empty_setting(std::mem::take(&mut self.key_intro_skip))
      .unwrap_or_else(default_key_intro_skip);
    if shortcut_collision(
      &self.key_next_episode,
      &self.key_previous_episode,
      &self.key_intro_skip,
    ) {
      self.key_next_episode = default_key_next_episode();
      self.key_previous_episode = default_key_previous_episode();
      self.key_intro_skip = default_key_intro_skip();
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShortcutKind {
  Next,
  Previous,
  IntroSkip,
}

#[derive(Debug)]
pub(crate) enum SettingsMutationError {
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
      Self::DuplicateSubtitleLanguage => formatter.write_str("subtitle language is duplicated"),
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

pub(crate) struct SettingsStore {
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
  pub(crate) fn load() -> Result<Self, ConfigError> {
    let path = config_path();
    let settings = load_from(&path)?;
    Ok(Self { path, settings })
  }

  pub(crate) fn snapshot(&self) -> &Settings {
    &self.settings
  }

  pub(crate) fn set_login_prefill(
    &mut self,
    prefill: LoginPrefill,
    provider: String,
  ) -> Result<bool, SettingsMutationError> {
    let server_url =
      non_empty_setting(prefill.server_url).ok_or(SettingsMutationError::InvalidLoginPrefill)?;
    let username =
      non_empty_setting(prefill.username).ok_or(SettingsMutationError::InvalidLoginPrefill)?;
    let provider = provider.trim().to_ascii_lowercase();
    if !matches!(provider.as_str(), "jellyfin" | "emby") {
      return Err(SettingsMutationError::InvalidProvider);
    }
    self.update(move |settings| {
      settings.remember = true;
      settings.server_url = server_url;
      settings.provider = provider;
      settings.username = username;
    })
  }

  pub(crate) fn clear_login_prefill(&mut self) -> Result<bool, SettingsMutationError> {
    self.update(|settings| {
      settings.remember = false;
      settings.server_url.clear();
      settings.provider.clear();
      settings.username.clear();
    })
  }

  pub(crate) fn set_intro_mode(&mut self, mode: IntroMode) -> Result<bool, SettingsMutationError> {
    self.update(|settings| settings.intro_mode = mode)
  }

  pub(crate) fn set_mpv_path(&mut self, path: String) -> Result<bool, SettingsMutationError> {
    self.update(|settings| settings.mpv_path = non_empty_setting(path))
  }

  pub(crate) fn set_mpv_args(&mut self, args: &str) -> Result<bool, SettingsMutationError> {
    let args = parse_mpv_args(args);
    self.update(|settings| settings.mpv_args = args)
  }

  pub(crate) fn set_playback_target_name(
    &mut self,
    name: String,
  ) -> Result<bool, SettingsMutationError> {
    self.update(|settings| settings.playback_target_name = non_empty_setting(name))
  }

  pub(crate) fn add_subtitle_language(
    &mut self,
    language: String,
  ) -> Result<bool, SettingsMutationError> {
    let language = language.trim().to_ascii_lowercase();
    if !valid_subtitle_language(&language) {
      return Err(SettingsMutationError::InvalidSubtitleLanguage);
    }
    if self
      .settings
      .subtitle_languages
      .iter()
      .any(|existing| existing.eq_ignore_ascii_case(&language))
    {
      return Err(SettingsMutationError::DuplicateSubtitleLanguage);
    }
    self.update(|settings| settings.subtitle_languages.push(language))
  }

  pub(crate) fn move_subtitle_language(
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
    if index >= self.settings.subtitle_languages.len()
      || target >= self.settings.subtitle_languages.len()
    {
      return Ok(false);
    }
    self.update(|settings| settings.subtitle_languages.swap(index, target))
  }

  pub(crate) fn remove_subtitle_language(
    &mut self,
    index: usize,
  ) -> Result<bool, SettingsMutationError> {
    if index >= self.settings.subtitle_languages.len() {
      return Ok(false);
    }
    self.update(|settings| {
      settings.subtitle_languages.remove(index);
    })
  }

  pub(crate) fn clear_subtitle_languages(&mut self) -> Result<bool, SettingsMutationError> {
    self.update(|settings| settings.subtitle_languages.clear())
  }

  pub(crate) fn set_shortcut(
    &mut self,
    kind: ShortcutKind,
    key: String,
  ) -> Result<bool, SettingsMutationError> {
    let key = non_empty_setting(key).ok_or(SettingsMutationError::EmptyShortcut)?;
    let bindings = &self.settings;
    let collision = match kind {
      ShortcutKind::Next => {
        binding_matches(&bindings.key_previous_episode, &key)
          || binding_matches(&bindings.key_intro_skip, &key)
      }
      ShortcutKind::Previous => {
        binding_matches(&bindings.key_next_episode, &key)
          || binding_matches(&bindings.key_intro_skip, &key)
      }
      ShortcutKind::IntroSkip => {
        binding_matches(&bindings.key_next_episode, &key)
          || binding_matches(&bindings.key_previous_episode, &key)
      }
    };
    if collision {
      return Err(SettingsMutationError::ShortcutCollision);
    }
    self.update(|settings| match kind {
      ShortcutKind::Next => settings.key_next_episode = key,
      ShortcutKind::Previous => settings.key_previous_episode = key,
      ShortcutKind::IntroSkip => settings.key_intro_skip = key,
    })
  }

  pub(crate) fn set_image_cache_enabled(
    &mut self,
    enabled: bool,
  ) -> Result<bool, SettingsMutationError> {
    self.update(|settings| settings.image_cache_enabled = enabled)
  }

  fn update(
    &mut self,
    mutation: impl FnOnce(&mut Settings),
  ) -> Result<bool, SettingsMutationError> {
    let mut candidate = self.settings.clone();
    mutation(&mut candidate);
    candidate.validate();
    if candidate == self.settings {
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

fn shortcut_collision(next: &str, previous: &str, intro_skip: &str) -> bool {
  binding_matches(next, previous)
    || binding_matches(next, intro_skip)
    || binding_matches(previous, intro_skip)
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
  Ok(
    value
      .as_array()
      .into_iter()
      .flatten()
      .filter_map(|value| value.as_str().map(str::to_owned))
      .collect(),
  )
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
  Ok(
    value
      .as_str()
      .map(str::trim)
      .filter(|value| !value.is_empty())
      .map_or_else(fallback, str::to_owned),
  )
}

fn deserialize_image_cache_enabled<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
  D: serde::Deserializer<'de>,
{
  let value = serde_json::Value::deserialize(deserializer)?;
  Ok(value.as_bool().unwrap_or_else(default_image_cache_enabled))
}

#[derive(Debug)]
pub(crate) enum ConfigError {
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
  relm4::gtk::glib::user_config_dir()
    .join(CONFIG_DIRECTORY)
    .join(CONFIG_FILE)
}

fn temporary_path(path: &Path) -> PathBuf {
  path.with_extension("json.tmp")
}

fn load_from(path: &Path) -> Result<Settings, ConfigError> {
  let contents = match fs::read_to_string(path) {
    Ok(contents) => contents,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
    Err(error) => return Err(error.into()),
  };
  let mut settings: Settings = serde_json::from_str(&contents)?;
  settings.validate();
  Ok(settings)
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
      mpv_path: Some("/usr/bin/mpv".to_owned()),
      mpv_args: vec!["--fullscreen".to_owned(), "--profile=gpu-hq".to_owned()],
      playback_target_name: Some("Living Room".to_owned()),
      subtitle_languages: vec!["eng".to_owned(), "spa".to_owned()],
      key_next_episode: "N".to_owned(),
      key_previous_episode: "P".to_owned(),
      key_intro_skip: "I".to_owned(),
      image_cache_enabled: false,
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
    assert_eq!(settings.mpv_path(), None);
    assert!(settings.mpv_args().is_empty());
    assert_eq!(settings.playback_target_name(), None);
    assert!(settings.subtitle_languages().is_empty());
    assert_eq!(settings.key_next_episode(), "Shift+>");
    assert_eq!(settings.key_previous_episode(), "Shift+<");
    assert_eq!(settings.key_intro_skip(), "g");
    assert!(settings.image_cache_enabled());
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
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn load_normalizes_semantically_invalid_settings() {
    let path = test_path("semantic-validity");
    let _ = fs::remove_file(&path);
    fs::write(
      &path,
      r#"{"remember":false,"server_url":"stale","provider":"jellyfin","username":"stale","mpv_path":"  ","mpv_args":[" --fullscreen "," "],"playback_target_name":" Living Room ","subtitle_languages":[" ENG ","eng","bad,list","pt-br"],"key_next_episode":" x ","key_previous_episode":"X","key_intro_skip":"g"}"#,
    )
    .unwrap();

    let settings = load_from(&path).unwrap();

    assert_eq!(
      settings.login_prefill(),
      LoginPrefill::new(String::new(), String::new())
    );
    assert_eq!(settings.mpv_path(), None);
    assert_eq!(settings.mpv_args(), &["--fullscreen"]);
    assert_eq!(settings.playback_target_name(), Some("Living Room"));
    assert_eq!(settings.subtitle_languages(), &["eng", "pt-br"]);
    assert_eq!(settings.key_next_episode(), "Shift+>");
    assert_eq!(settings.key_previous_episode(), "Shift+<");
    assert_eq!(settings.key_intro_skip(), "g");
    fs::remove_file(path).unwrap();
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
