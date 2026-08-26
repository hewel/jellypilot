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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct LoginPrefill {
  pub(crate) remember: bool,
  pub(crate) server_url: String,
  pub(crate) provider: String,
  pub(crate) username: String,
  #[serde(default)]
  pub(crate) intro_mode: IntroMode,
  #[serde(default, deserialize_with = "deserialize_optional_string")]
  pub(crate) mpv_path: Option<String>,
  #[serde(default, deserialize_with = "deserialize_string_list")]
  pub(crate) mpv_args: Vec<String>,
  #[serde(default, deserialize_with = "deserialize_optional_string")]
  pub(crate) playback_target_name: Option<String>,
  #[serde(default, deserialize_with = "deserialize_string_list")]
  pub(crate) subtitle_languages: Vec<String>,
  #[serde(
    default = "default_key_next_episode",
    deserialize_with = "deserialize_key_next_episode"
  )]
  pub(crate) key_next_episode: String,
  #[serde(
    default = "default_key_previous_episode",
    deserialize_with = "deserialize_key_previous_episode"
  )]
  pub(crate) key_previous_episode: String,
  #[serde(
    default = "default_key_intro_skip",
    deserialize_with = "deserialize_key_intro_skip"
  )]
  pub(crate) key_intro_skip: String,
  #[serde(
    default = "default_image_cache_enabled",
    deserialize_with = "deserialize_image_cache_enabled"
  )]
  pub(crate) image_cache_enabled: bool,
}

impl Default for LoginPrefill {
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

pub(crate) fn load() -> LoginPrefill {
  load_checked().unwrap_or_default()
}

pub(crate) fn load_checked() -> Result<LoginPrefill, ConfigError> {
  load_from(&config_path())
}

pub(crate) fn save(prefill: &LoginPrefill) -> Result<(), ConfigError> {
  save_to(&config_path(), prefill)
}

pub(crate) fn clear() -> Result<(), ConfigError> {
  let mut config = load();
  config.remember = false;
  config.server_url.clear();
  config.provider.clear();
  config.username.clear();
  save(&config)
}

fn config_path() -> PathBuf {
  relm4::gtk::glib::user_config_dir()
    .join(CONFIG_DIRECTORY)
    .join(CONFIG_FILE)
}

fn temporary_path(path: &Path) -> PathBuf {
  path.with_extension("json.tmp")
}

fn load_from(path: &Path) -> Result<LoginPrefill, ConfigError> {
  let contents = match fs::read_to_string(path) {
    Ok(contents) => contents,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(LoginPrefill::default()),
    Err(error) => return Err(error.into()),
  };
  Ok(serde_json::from_str(&contents)?)
}

fn save_to(path: &Path, prefill: &LoginPrefill) -> Result<(), ConfigError> {
  let contents = serde_json::to_string_pretty(prefill)?;
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
fn clear_from(path: &Path) -> Result<(), ConfigError> {
  let mut config = load_from(path)?;
  config.remember = false;
  config.server_url.clear();
  config.provider.clear();
  config.username.clear();
  save_to(path, &config)
}
#[cfg(test)]
mod tests {
  use super::*;

  fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
      "jellypilot-login-prefill-{}-{name}.json",
      std::process::id()
    ))
  }

  fn remembered_prefill() -> LoginPrefill {
    LoginPrefill {
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

  #[test]
  fn legacy_config_defaults_new_application_settings() {
    let path = test_path("legacy");
    let _ = fs::remove_file(&path);
    fs::write(
      &path,
      r#"{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice"}"#,
    )
    .unwrap();

    let config = load_from(&path).unwrap();

    assert_eq!(config.intro_mode, IntroMode::Automatic);
    assert_eq!(config.mpv_path, None);
    assert!(config.mpv_args.is_empty());
    assert_eq!(config.playback_target_name, None);
    assert!(config.subtitle_languages.is_empty());
    assert_eq!(config.key_next_episode, "Shift+>");
    assert_eq!(config.key_previous_episode, "Shift+<");
    assert_eq!(config.key_intro_skip, "g");
    assert!(config.image_cache_enabled);
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

    let config = load_from(&path).unwrap();

    assert!(config.remember);
    assert_eq!(config.server_url, "https://media.example.com");
    assert_eq!(config.username, "alice");
    assert_eq!(config.mpv_path, None);
    assert!(config.mpv_args.is_empty());
    assert_eq!(config.playback_target_name, None);
    assert!(config.subtitle_languages.is_empty());
    assert_eq!(config.key_next_episode, "Shift+>");
    assert_eq!(config.key_previous_episode, "Shift+<");
    assert_eq!(config.key_intro_skip, "g");
    assert!(config.image_cache_enabled);
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn unknown_intro_mode_preserves_valid_prefill_and_defaults_to_automatic() {
    let path = test_path("unknown-intro-mode");
    let _ = fs::remove_file(&path);
    fs::write(
      &path,
      r#"{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice","intro_mode":"invalid"}"#,
    )
    .unwrap();

    let prefill = load_from(&path).unwrap();

    assert!(prefill.remember);
    assert_eq!(prefill.server_url, "https://media.example.com");
    assert_eq!(prefill.provider, "jellyfin");
    assert_eq!(prefill.username, "alice");
    assert_eq!(prefill.intro_mode, IntroMode::Automatic);
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn non_string_intro_mode_preserves_valid_prefill_and_defaults_to_automatic() {
    for (name, mode_json) in [
      ("null-intro-mode", "null"),
      ("numeric-intro-mode", "3"),
      ("object-intro-mode", r#"{"mode":"off"}"#),
    ] {
      let path = test_path(name);
      let _ = fs::remove_file(&path);
      fs::write(
        &path,
        format!(
          r#"{{"remember":true,"server_url":"https://media.example.com","provider":"jellyfin","username":"alice","intro_mode":{mode_json}}}"#
        ),
      )
      .unwrap();

      let prefill = load_from(&path).unwrap();

      assert!(prefill.remember);
      assert_eq!(prefill.server_url, "https://media.example.com");
      assert_eq!(prefill.username, "alice");
      assert_eq!(prefill.intro_mode, IntroMode::Automatic);
      fs::remove_file(path).unwrap();
    }
  }

  #[test]
  fn missing_config_defaults_to_no_prefill() {
    let path = test_path("missing");
    let _ = fs::remove_file(&path);
    assert_eq!(load_from(&path).unwrap(), LoginPrefill::default());
  }

  #[test]
  fn config_round_trip_excludes_credentials_and_preserves_settings() {
    let path = test_path("round-trip");
    let _ = fs::remove_file(&path);
    let prefill = remembered_prefill();
    save_to(&path, &prefill).unwrap();
    let contents = fs::read_to_string(&path).unwrap();
    assert!(!contents.contains("password"));
    assert_eq!(load_from(&path).unwrap(), prefill);
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn clearing_remembered_login_preserves_application_settings() {
    let path = test_path("clear");
    let _ = fs::remove_file(&path);
    let mut expected = remembered_prefill();
    save_to(&path, &expected).unwrap();
    clear_from(&path).unwrap();
    expected.remember = false;
    expected.server_url.clear();
    expected.provider.clear();
    expected.username.clear();
    assert_eq!(load_from(&path).unwrap(), expected);
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn failed_atomic_write_preserves_existing_config() {
    let path = test_path("atomic-failure");
    let temporary = temporary_path(&path);
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(&temporary);
    let original = remembered_prefill();
    save_to(&path, &original).unwrap();
    fs::create_dir(&temporary).unwrap();
    let replacement = LoginPrefill {
      remember: true,
      server_url: "https://replacement.example.com".to_owned(),
      provider: "emby".to_owned(),
      username: "bob".to_owned(),
      ..LoginPrefill::default()
    };
    assert!(save_to(&path, &replacement).is_err());
    assert_eq!(load_from(&path).unwrap(), original);
    fs::remove_dir(temporary).unwrap();
    fs::remove_file(path).unwrap();
  }
}
