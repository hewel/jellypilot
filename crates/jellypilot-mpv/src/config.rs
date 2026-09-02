use jellypilot_core::config::Settings;

use crate::has_mpv_option;

/// Composes persisted MPV arguments with ordered subtitle-language preferences.
///
/// An explicit user-provided `--slang` option always takes precedence.
#[must_use]
pub fn configured_mpv_args(settings: &Settings) -> Vec<String> {
  let mut args = settings.mpv_args().to_vec();
  if !settings.subtitle_languages().is_empty() && !has_mpv_option(&args, "slang") {
    args.push(format!(
      "--slang={}",
      settings.subtitle_languages().join(",")
    ));
  }
  args
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn subtitle_preferences_reach_mpv_launch_without_overriding_user_slang() {
    let settings: Settings = serde_json::from_value(serde_json::json!({
        "remember": false,
        "server_url": "",
        "provider": "",
        "username": "",
        "mpv_args": ["--fullscreen"],
        "subtitle_languages": ["eng", "spa"]
    }))
    .expect("settings should deserialize");
    assert_eq!(
      configured_mpv_args(&settings),
      vec!["--fullscreen", "--slang=eng,spa"]
    );

    let explicit_slang: Settings = serde_json::from_value(serde_json::json!({
        "remember": false,
        "server_url": "",
        "provider": "",
        "username": "",
        "mpv_args": ["--fullscreen", "--slang=jpn"],
        "subtitle_languages": ["eng", "spa"]
    }))
    .expect("settings should deserialize");
    assert_eq!(
      configured_mpv_args(&explicit_slang),
      vec!["--fullscreen", "--slang=jpn"]
    );
  }
}
