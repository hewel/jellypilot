//! Application configuration with persistence.

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

/// Intro Skipper behavior mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum IntroSkipperMode {
  Automatic,
  Manual,
  Off,
}

/// User-selected app theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
  System,
  Light,
  Dark,
}

/// Application configuration.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
  /// Custom MPV executable path (None = auto-detect).
  #[serde(default)]
  pub mpv_path: Option<String>,

  /// Additional MPV command-line arguments.
  #[serde(default)]
  pub mpv_args: Vec<String>,

  /// Device name shown in Jellyfin cast menu.
  #[serde(default = "default_device_name")]
  pub device_name: String,

  /// Progress reporting interval in seconds.
  #[serde(default = "default_progress_interval")]
  pub progress_interval: u32,

  /// Start minimized to system tray.
  #[serde(default)]
  pub start_minimized: bool,

  /// Intro Skipper plugin behavior mode.
  #[serde(default = "default_intro_skipper_mode")]
  pub intro_skipper_mode: IntroSkipperMode,

  /// Ordered subtitle language codes to prefer when Jellyfin does not request a track.
  #[serde(default)]
  pub preferred_subtitle_languages: Vec<String>,

  /// Cache Library Browser images on disk for faster repeat browsing.
  #[serde(default = "default_image_disk_cache_enabled")]
  pub image_disk_cache_enabled: bool,

  /// Keybinding for next episode in MPV.
  #[serde(default = "default_keybind_next")]
  pub keybind_next: String,

  /// Keybinding for previous episode in MPV.
  #[serde(default = "default_keybind_prev")]
  pub keybind_prev: String,

  /// Keybinding for manual Intro Skipper seek in MPV.
  #[serde(default = "default_keybind_intro_skip")]
  pub keybind_intro_skip: String,

  /// Light, dark, or OS-following theme preference.
  #[serde(default = "default_theme_preference")]
  pub theme_preference: ThemePreference,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppConfigWire {
  #[serde(default)]
  mpv_path: Option<String>,
  #[serde(default)]
  mpv_args: Vec<String>,
  #[serde(default = "default_device_name")]
  device_name: String,
  #[serde(default = "default_progress_interval")]
  progress_interval: u32,
  #[serde(default)]
  start_minimized: bool,
  #[serde(default)]
  intro_skipper_mode: Option<IntroSkipperMode>,
  #[serde(default)]
  intro_skipper_enabled: Option<bool>,
  #[serde(default)]
  preferred_subtitle_languages: Vec<String>,
  #[serde(default = "default_image_disk_cache_enabled")]
  image_disk_cache_enabled: bool,
  #[serde(default = "default_keybind_next")]
  keybind_next: String,
  #[serde(default = "default_keybind_prev")]
  keybind_prev: String,
  #[serde(default = "default_keybind_intro_skip")]
  keybind_intro_skip: String,
  #[serde(default = "default_theme_preference")]
  theme_preference: ThemePreference,
}

impl<'de> Deserialize<'de> for AppConfig {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = AppConfigWire::deserialize(deserializer)?;
    let intro_skipper_mode =
      wire
        .intro_skipper_mode
        .unwrap_or_else(|| match wire.intro_skipper_enabled.unwrap_or(true) {
          true => IntroSkipperMode::Automatic,
          false => IntroSkipperMode::Off,
        });

    Ok(Self {
      mpv_path: wire.mpv_path,
      mpv_args: wire.mpv_args,
      device_name: wire.device_name,
      progress_interval: wire.progress_interval,
      start_minimized: wire.start_minimized,
      intro_skipper_mode,
      preferred_subtitle_languages: wire.preferred_subtitle_languages,
      image_disk_cache_enabled: wire.image_disk_cache_enabled,
      keybind_next: wire.keybind_next,
      keybind_prev: wire.keybind_prev,
      keybind_intro_skip: wire.keybind_intro_skip,
      theme_preference: wire.theme_preference,
    })
  }
}

fn default_device_name() -> String {
  "JellyPilot".to_string()
}

fn default_progress_interval() -> u32 {
  5
}

fn default_keybind_next() -> String {
  "Shift+>".to_string()
}

fn default_keybind_prev() -> String {
  "Shift+<".to_string()
}

fn default_keybind_intro_skip() -> String {
  "g".to_string()
}

fn default_intro_skipper_mode() -> IntroSkipperMode {
  IntroSkipperMode::Automatic
}

fn default_image_disk_cache_enabled() -> bool {
  true
}

fn default_theme_preference() -> ThemePreference {
  ThemePreference::System
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      mpv_path: None,
      mpv_args: Vec::new(),
      device_name: default_device_name(),
      progress_interval: default_progress_interval(),
      start_minimized: false,
      intro_skipper_mode: default_intro_skipper_mode(),
      preferred_subtitle_languages: Vec::new(),
      image_disk_cache_enabled: default_image_disk_cache_enabled(),
      keybind_next: default_keybind_next(),
      keybind_prev: default_keybind_prev(),
      keybind_intro_skip: default_keybind_intro_skip(),
      theme_preference: default_theme_preference(),
    }
  }
}

impl AppConfig {
  /// Validate configuration values.
  pub fn validate(&self) -> Result<(), String> {
    if self.device_name.trim().is_empty() {
      return Err("Device name cannot be empty".to_string());
    }
    if self.progress_interval < 1 || self.progress_interval > 60 {
      return Err("Progress interval must be between 1 and 60 seconds".to_string());
    }
    if self.keybind_next.trim().is_empty() {
      return Err("Next episode keybinding cannot be empty".to_string());
    }
    if self.keybind_prev.trim().is_empty() {
      return Err("Previous episode keybinding cannot be empty".to_string());
    }
    if self.keybind_intro_skip.trim().is_empty() {
      return Err("Intro skip keybinding cannot be empty".to_string());
    }
    if self
      .preferred_subtitle_languages
      .iter()
      .any(|language| language.trim().is_empty())
    {
      return Err("Preferred subtitle languages cannot contain empty entries".to_string());
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_config_uses_angle_bracket_episode_keybindings() {
    let config = AppConfig::default();

    assert_eq!(config.keybind_next, "Shift+>");
    assert_eq!(config.keybind_prev, "Shift+<");
  }

  #[test]
  fn older_saved_config_deserializes_with_default_automatic_intro_skipper_mode() {
    let config: AppConfig = serde_json::from_str(
      r#"{
        "deviceName": "JellyPilot",
        "progressInterval": 5,
        "startMinimized": false,
        "keybindNext": "Shift+n",
        "keybindPrev": "Shift+p"
      }"#,
    )
    .expect("older config should deserialize");

    assert_eq!(config.intro_skipper_mode, IntroSkipperMode::Automatic);
    assert!(config.preferred_subtitle_languages.is_empty());
    assert!(config.image_disk_cache_enabled);
    assert_eq!(config.theme_preference, ThemePreference::System);
  }

  #[test]
  fn explicit_theme_preference_deserializes_from_saved_config() {
    let config: AppConfig =
      serde_json::from_str(r#"{"themePreference":"dark"}"#).expect("config should deserialize");

    assert_eq!(config.theme_preference, ThemePreference::Dark);
  }

  #[test]
  fn legacy_enabled_intro_skipper_config_deserializes_as_automatic() {
    let config: AppConfig =
      serde_json::from_str(r#"{"introSkipperEnabled":true}"#).expect("config should deserialize");

    assert_eq!(config.intro_skipper_mode, IntroSkipperMode::Automatic);
  }

  #[test]
  fn legacy_disabled_intro_skipper_config_deserializes_as_off() {
    let config: AppConfig =
      serde_json::from_str(r#"{"introSkipperEnabled":false}"#).expect("config should deserialize");

    assert_eq!(config.intro_skipper_mode, IntroSkipperMode::Off);
  }

  #[test]
  fn config_rejects_empty_preferred_subtitle_language() {
    let mut config = AppConfig::default();
    config.preferred_subtitle_languages.push(" ".to_string());

    let err = config.validate().expect_err("empty language should fail");

    assert_eq!(
      err,
      "Preferred subtitle languages cannot contain empty entries"
    );
  }
}
