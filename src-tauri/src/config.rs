//! Application configuration with persistence.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
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

  /// Enable automatic Intro Skipper plugin skips.
  #[serde(default = "default_intro_skipper_enabled")]
  pub intro_skipper_enabled: bool,

  /// Ordered subtitle language codes to prefer when Jellyfin does not request a track.
  #[serde(default)]
  pub preferred_subtitle_languages: Vec<String>,

  /// Keybinding for next episode in MPV.
  #[serde(default = "default_keybind_next")]
  pub keybind_next: String,

  /// Keybinding for previous episode in MPV.
  #[serde(default = "default_keybind_prev")]
  pub keybind_prev: String,
}

fn default_device_name() -> String {
  "JMSR".to_string()
}

fn default_progress_interval() -> u32 {
  5
}

fn default_keybind_next() -> String {
  "Shift+n".to_string()
}

fn default_keybind_prev() -> String {
  "Shift+p".to_string()
}

fn default_intro_skipper_enabled() -> bool {
  true
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      mpv_path: None,
      mpv_args: Vec::new(),
      device_name: default_device_name(),
      progress_interval: default_progress_interval(),
      start_minimized: false,
      intro_skipper_enabled: default_intro_skipper_enabled(),
      preferred_subtitle_languages: Vec::new(),
      keybind_next: default_keybind_next(),
      keybind_prev: default_keybind_prev(),
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
  fn older_saved_config_deserializes_with_intro_skipper_enabled() {
    let config: AppConfig = serde_json::from_str(
      r#"{
        "deviceName": "JMSR",
        "progressInterval": 5,
        "startMinimized": false,
        "keybindNext": "Shift+n",
        "keybindPrev": "Shift+p"
      }"#,
    )
    .expect("older config should deserialize");

    assert!(config.intro_skipper_enabled);
    assert!(config.preferred_subtitle_languages.is_empty());
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
