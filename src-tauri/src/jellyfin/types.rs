//! Jellyfin API types.
//!
//! These types mirror the Jellyfin API responses and requests.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::intro_skipper::IntroSkipRange;

/// Authentication response from Jellyfin.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)] // API response fields - may be used later
pub struct AuthResponse {
  pub user: User,
  pub access_token: String,
  pub server_id: String,
}

/// Jellyfin user information.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct User {
  pub id: String,
  pub name: String,
}

/// Server information.
#[derive(Debug, Clone, Deserialize, Serialize, Type)]
#[serde(rename_all = "PascalCase")]
pub struct ServerInfo {
  pub server_name: String,
  pub version: String,
  pub id: String,
}

/// Connection state exposed to frontend.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionState {
  pub connected: bool,
  pub server_url: Option<String>,
  pub server_name: Option<String>,
  pub user_name: Option<String>,
}

/// Library Browser landing data exposed to the frontend.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoHome {
  pub continue_watching: Vec<VideoHomeItem>,
  pub next_up: Vec<VideoHomeItem>,
  pub latest_movies: Vec<VideoHomeItem>,
  pub latest_episodes: Vec<VideoHomeItem>,
  pub library_shortcuts: Vec<VideoLibraryShortcut>,
}

/// Video item summary for Video Home rows.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoHomeItem {
  pub id: String,
  pub name: String,
  pub item_type: String,
  pub series_id: Option<String>,
  pub series_name: Option<String>,
  pub season_number: Option<i32>,
  pub episode_number: Option<i32>,
  pub production_year: Option<i32>,
  pub runtime_seconds: Option<f64>,
  pub resume_position_seconds: Option<f64>,
  pub played_percentage: Option<f64>,
  pub played: bool,
  pub favorite: bool,
  pub artwork_url: Option<String>,
}

/// Video library shortcut for drilling into Movies or Shows libraries.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoLibraryShortcut {
  pub id: String,
  pub name: String,
  pub collection_type: String,
  pub item_count: Option<i32>,
  pub artwork_url: Option<String>,
}

/// Supported video library browse families.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Type)]
pub enum VideoLibraryKind {
  #[serde(rename = "movies")]
  Movies,
  #[serde(rename = "tvshows")]
  TvShows,
}

/// Paged Library Browser listing request.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoLibraryPageRequest {
  pub library_id: String,
  pub collection_type: VideoLibraryKind,
  pub start_index: i32,
  pub limit: i32,
  pub sort: VideoLibrarySort,
  pub played_filter: VideoLibraryPlayedFilter,
  pub favorites_only: bool,
}

/// Supported Library Browser sort options.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Type)]
pub enum VideoLibrarySort {
  #[serde(rename = "title")]
  Title,
  #[serde(rename = "recentlyAdded")]
  RecentlyAdded,
  #[serde(rename = "releaseDate")]
  ReleaseDate,
}

/// Supported played-state filters for Library Browser results.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Type)]
pub enum VideoLibraryPlayedFilter {
  #[serde(rename = "all")]
  All,
  #[serde(rename = "played")]
  Played,
  #[serde(rename = "unplayed")]
  Unplayed,
}

/// Paged Library Browser listing result.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoLibraryPage {
  pub library_id: String,
  pub collection_type: VideoLibraryKind,
  pub start_index: i32,
  pub limit: i32,
  pub total_record_count: i32,
  pub has_more: bool,
  pub items: Vec<VideoLibraryItem>,
}

/// Media card summary for Movies and Shows browse results.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoLibraryItem {
  pub id: String,
  pub name: String,
  pub item_type: String,
  pub production_year: Option<i32>,
  pub runtime_seconds: Option<f64>,
  pub played: bool,
  pub favorite: bool,
  pub artwork_url: Option<String>,
}

/// Credentials for authentication.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
  pub server_url: String,
  pub username: String,
  pub password: String,
}

/// Quick Connect request created by the server.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QuickConnectRequest {
  pub code: String,
  pub secret: String,
}

/// Quick Connect request status exposed to the frontend.
#[derive(Debug, Clone, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QuickConnectStatus {
  Waiting,
  Approved,
}

/// WebSocket message types from Jellyfin server.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WsMessage {
  pub message_type: String,
  #[serde(default)]
  pub data: Option<serde_json::Value>,
}

/// Play command from Jellyfin (via WebSocket).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)] // API response fields - may be used later
pub struct PlayRequest {
  pub item_ids: Vec<String>,
  pub start_position_ticks: Option<i64>,
  pub play_command: String,
  #[serde(default)]
  pub media_source_id: Option<String>,
  #[serde(default)]
  pub audio_stream_index: Option<i32>,
  #[serde(default)]
  pub subtitle_stream_index: Option<i32>,
}

/// Playstate command from Jellyfin.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaystateRequest {
  pub command: String,
  #[serde(default)]
  pub seek_position_ticks: Option<i64>,
}

/// General command from Jellyfin.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GeneralCommand {
  pub name: String,
  #[serde(default)]
  pub arguments: Option<serde_json::Value>,
}

/// Media item (movie, episode, etc.).
#[derive(Debug, Clone, Deserialize, Serialize, Type)]
#[serde(rename_all = "PascalCase")]
pub struct MediaItem {
  pub id: String,
  pub name: String,
  #[serde(rename = "Type")]
  pub item_type: String,
  #[serde(default)]
  pub series_id: Option<String>,
  #[serde(default)]
  pub series_name: Option<String>,
  #[serde(default)]
  pub season_name: Option<String>,
  #[serde(default)]
  pub index_number: Option<i32>,
  #[serde(default)]
  pub parent_index_number: Option<i32>,
  #[serde(default)]
  pub run_time_ticks: Option<i64>,
  #[serde(default)]
  pub overview: Option<String>,
}

/// Media source for playback.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)] // API response fields - may be used later
pub struct MediaSource {
  pub id: String,
  pub path: Option<String>,
  pub protocol: String,
  #[serde(default)]
  pub container: Option<String>,
  #[serde(default)]
  pub run_time_ticks: Option<i64>,
  #[serde(default)]
  pub media_streams: Vec<MediaStream>,
  #[serde(default)]
  pub supports_direct_play: bool,
  #[serde(default)]
  pub supports_direct_stream: bool,
  #[serde(default)]
  pub supports_transcoding: bool,
}

/// Individual stream (video, audio, subtitle).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)] // API response fields - may be used later
pub struct MediaStream {
  pub index: i32,
  #[serde(rename = "Type")]
  pub stream_type: String,
  #[serde(default)]
  pub codec: Option<String>,
  #[serde(default)]
  pub language: Option<String>,
  #[serde(default)]
  pub display_title: Option<String>,
  #[serde(default)]
  pub is_default: bool,
  #[serde(default)]
  pub is_external: bool,
}

/// Playback info request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoRequest {
  pub user_id: String,
  pub device_id: String,
  #[serde(default)]
  pub max_streaming_bitrate: Option<i64>,
  #[serde(default)]
  pub start_time_ticks: Option<i64>,
  #[serde(default)]
  pub audio_stream_index: Option<i32>,
  #[serde(default)]
  pub subtitle_stream_index: Option<i32>,
  pub enable_direct_play: bool,
  pub enable_direct_stream: bool,
  pub enable_transcoding: bool,
  pub auto_open_live_stream: bool,
}

/// Playback info response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoResponse {
  pub media_sources: Vec<MediaSource>,
  #[serde(default)]
  pub play_session_id: Option<String>,
}

/// Playback start info (sent to Jellyfin when playback starts).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStartInfo {
  pub item_id: String,
  #[serde(default)]
  pub media_source_id: Option<String>,
  #[serde(default)]
  pub play_session_id: Option<String>,
  #[serde(default)]
  pub position_ticks: Option<i64>,
  pub is_paused: bool,
  pub is_muted: bool,
  pub volume_level: i32,
  #[serde(default)]
  pub audio_stream_index: Option<i32>,
  #[serde(default)]
  pub subtitle_stream_index: Option<i32>,
  pub play_method: String,
  pub can_seek: bool,
}

/// Playback progress info (sent periodically to Jellyfin).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackProgressInfo {
  pub item_id: String,
  #[serde(default)]
  pub media_source_id: Option<String>,
  #[serde(default)]
  pub play_session_id: Option<String>,
  #[serde(default)]
  pub position_ticks: Option<i64>,
  pub is_paused: bool,
  pub is_muted: bool,
  pub volume_level: i32,
  #[serde(default)]
  pub audio_stream_index: Option<i32>,
  #[serde(default)]
  pub subtitle_stream_index: Option<i32>,
  pub play_method: String,
  pub can_seek: bool,
}

/// Playback stop info (sent when playback ends).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStopInfo {
  pub item_id: String,
  #[serde(default)]
  pub media_source_id: Option<String>,
  #[serde(default)]
  pub play_session_id: Option<String>,
  #[serde(default)]
  pub position_ticks: Option<i64>,
}

/// Active playback session state.
#[derive(Debug, Clone)]
pub struct PlaybackSession {
  pub item_id: String,
  pub media_source_id: Option<String>,
  pub play_session_id: Option<String>,
  pub intro_skipper_ranges: Vec<IntroSkipRange>,
  pub position_ticks: i64,
  pub is_paused: bool,
  pub is_muted: bool,
  pub volume: i32,
  pub audio_stream_index: Option<i32>,
  pub subtitle_stream_index: Option<i32>,
}

/// Ticks conversion helpers (1 tick = 100 nanoseconds).
pub const TICKS_PER_SECOND: i64 = 10_000_000;

/// Convert seconds to ticks.
pub fn seconds_to_ticks(seconds: f64) -> i64 {
  (seconds * TICKS_PER_SECOND as f64) as i64
}

/// Convert ticks to seconds.
pub fn ticks_to_seconds(ticks: i64) -> f64 {
  ticks as f64 / TICKS_PER_SECOND as f64
}

/// Saved session data for persistence.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedSession {
  pub server_url: String,
  pub access_token: String,
  pub user_id: String,
  pub user_name: String,
  pub server_name: Option<String>,
  pub device_id: Option<String>,
}

/// Track preference for a series (audio/subtitle language).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackPreference {
  /// Preferred audio language code (e.g., "jpn", "eng").
  pub audio_language: Option<String>,
  /// Preferred audio display title (e.g., "Japanese - AAC 2.0").
  #[serde(default)]
  pub audio_title: Option<String>,
  /// Preferred subtitle language code (e.g., "chi", "eng").
  pub subtitle_language: Option<String>,
  /// Preferred subtitle display title (e.g., "English - SRT", "English SDH").
  #[serde(default)]
  pub subtitle_title: Option<String>,
  /// Whether a subtitle preference was explicitly saved for this series.
  #[serde(default)]
  pub subtitle_preference_set: bool,
  /// Whether subtitles should be enabled when a subtitle preference is set.
  #[serde(default)]
  pub is_subtitle_enabled: bool,
}

impl TrackPreference {
  /// Normalize preferences loaded from older stores that predate `subtitle_preference_set`.
  pub fn normalize_loaded(&mut self) {
    if self.subtitle_preference_set {
      return;
    }

    self.subtitle_preference_set = self.subtitle_language.is_some()
      || self.subtitle_title.is_some()
      || (!self.is_subtitle_enabled && self.audio_language.is_none() && self.audio_title.is_none());
  }
}

/// Find a stream by language and type.
/// Returns the stream index if found.
pub fn find_stream_by_lang(streams: &[MediaStream], stream_type: &str, lang: &str) -> Option<i32> {
  streams
    .iter()
    .find(|s| {
      s.stream_type == stream_type
        && s
          .language
          .as_deref()
          .map(|l| l.eq_ignore_ascii_case(lang))
          .unwrap_or(false)
    })
    .map(|s| s.index)
}

/// Find a stream by language and optionally title.
/// Tries to match both language and title first, then falls back to language-only.
/// This handles cases where multiple tracks share the same language (e.g., "English" vs "English SDH").
pub fn find_stream_by_preference(
  streams: &[MediaStream],
  stream_type: &str,
  lang: &str,
  title: Option<&str>,
) -> Option<i32> {
  // First, try to match both language and title (if title is provided)
  if let Some(title) = title {
    if let Some(stream) = streams.iter().find(|s| {
      s.stream_type == stream_type
        && s
          .language
          .as_deref()
          .map(|l| l.eq_ignore_ascii_case(lang))
          .unwrap_or(false)
        && s
          .display_title
          .as_deref()
          .map(|t| t == title)
          .unwrap_or(false)
    }) {
      return Some(stream.index);
    }
  }

  // Fall back to language-only match
  find_stream_by_lang(streams, stream_type, lang)
}

/// Find the first stream matching an ordered language priority list.
pub fn find_stream_by_language_priority(
  streams: &[MediaStream],
  stream_type: &str,
  languages: &[String],
) -> Option<i32> {
  languages.iter().find_map(|language| {
    let language = language.trim();
    if language.is_empty() {
      None
    } else {
      find_stream_by_lang(streams, stream_type, language)
    }
  })
}

/// Select a subtitle stream using request, series, then global language preference precedence.
pub fn select_subtitle_stream_index(
  request_subtitle_index: Option<i32>,
  series_preference: Option<&TrackPreference>,
  streams: &[MediaStream],
  preferred_languages: &[String],
) -> Option<i32> {
  if request_subtitle_index.is_some() {
    return request_subtitle_index;
  }

  if let Some(pref) = series_preference {
    if pref.subtitle_preference_set {
      if !pref.is_subtitle_enabled {
        return Some(-1);
      }

      if let Some(ref lang) = pref.subtitle_language {
        if let Some(idx) =
          find_stream_by_preference(streams, "Subtitle", lang, pref.subtitle_title.as_deref())
        {
          return Some(idx);
        }
      }
    }
  }

  find_stream_by_language_priority(streams, "Subtitle", preferred_languages)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn stream(index: i32, stream_type: &str, language: Option<&str>) -> MediaStream {
    MediaStream {
      index,
      stream_type: stream_type.to_string(),
      codec: None,
      language: language.map(str::to_string),
      display_title: None,
      is_default: false,
      is_external: false,
    }
  }

  #[test]
  fn find_stream_by_language_priority_uses_configured_order() {
    let streams = vec![
      stream(2, "Subtitle", Some("eng")),
      stream(4, "Subtitle", Some("jpn")),
    ];
    let languages = vec!["jpn".to_string(), "eng".to_string()];

    let index = find_stream_by_language_priority(&streams, "Subtitle", &languages);

    assert_eq!(index, Some(4));
  }

  #[test]
  fn find_stream_by_language_priority_matches_case_insensitively() {
    let streams = vec![stream(9, "Subtitle", Some("ENG"))];
    let languages = vec!["eng".to_string()];

    let index = find_stream_by_language_priority(&streams, "Subtitle", &languages);

    assert_eq!(index, Some(9));
  }

  #[test]
  fn find_stream_by_language_priority_ignores_surrounding_whitespace() {
    let streams = vec![stream(7, "Subtitle", Some("eng"))];
    let languages = vec![" eng ".to_string()];

    let index = find_stream_by_language_priority(&streams, "Subtitle", &languages);

    assert_eq!(index, Some(7));
  }

  #[test]
  fn select_subtitle_stream_index_keeps_explicit_request() {
    let streams = vec![stream(2, "Subtitle", Some("jpn"))];
    let preference = TrackPreference {
      subtitle_preference_set: true,
      is_subtitle_enabled: false,
      ..TrackPreference::default()
    };
    let languages = vec!["jpn".to_string()];

    let index = select_subtitle_stream_index(Some(12), Some(&preference), &streams, &languages);

    assert_eq!(index, Some(12));
  }

  #[test]
  fn select_subtitle_stream_index_keeps_series_disabled_preference() {
    let streams = vec![stream(2, "Subtitle", Some("jpn"))];
    let preference = TrackPreference {
      subtitle_preference_set: true,
      is_subtitle_enabled: false,
      ..TrackPreference::default()
    };
    let languages = vec!["jpn".to_string()];

    let index = select_subtitle_stream_index(None, Some(&preference), &streams, &languages);

    assert_eq!(index, Some(-1));
  }

  #[test]
  fn select_subtitle_stream_index_prefers_series_language_over_global_language() {
    let streams = vec![
      stream(2, "Subtitle", Some("eng")),
      stream(4, "Subtitle", Some("jpn")),
    ];
    let preference = TrackPreference {
      subtitle_language: Some("jpn".to_string()),
      subtitle_preference_set: true,
      is_subtitle_enabled: true,
      ..TrackPreference::default()
    };
    let languages = vec!["eng".to_string()];

    let index = select_subtitle_stream_index(None, Some(&preference), &streams, &languages);

    assert_eq!(index, Some(4));
  }

  #[test]
  fn select_subtitle_stream_index_ignores_audio_only_series_preference() {
    let streams = vec![stream(2, "Subtitle", Some("eng"))];
    let preference = TrackPreference {
      audio_language: Some("jpn".to_string()),
      ..TrackPreference::default()
    };
    let languages = vec!["eng".to_string()];

    let index = select_subtitle_stream_index(None, Some(&preference), &streams, &languages);

    assert_eq!(index, Some(2));
  }

  #[test]
  fn normalize_loaded_keeps_legacy_audio_only_preference_without_subtitle_preference() {
    let mut preference = TrackPreference {
      audio_language: Some("jpn".to_string()),
      is_subtitle_enabled: false,
      ..TrackPreference::default()
    };

    preference.normalize_loaded();

    assert!(!preference.subtitle_preference_set);
  }

  #[test]
  fn normalize_loaded_marks_legacy_subtitle_only_disabled_preference() {
    let mut preference = TrackPreference {
      is_subtitle_enabled: false,
      ..TrackPreference::default()
    };

    preference.normalize_loaded();

    assert!(preference.subtitle_preference_set);
  }

  #[test]
  fn select_subtitle_stream_index_falls_back_to_global_language() {
    let streams = vec![stream(2, "Subtitle", Some("eng"))];
    let preference = TrackPreference {
      subtitle_language: Some("jpn".to_string()),
      subtitle_preference_set: true,
      is_subtitle_enabled: true,
      ..TrackPreference::default()
    };
    let languages = vec!["eng".to_string()];

    let index = select_subtitle_stream_index(None, Some(&preference), &streams, &languages);

    assert_eq!(index, Some(2));
  }
}

/// Response from /Shows/{seriesId}/Episodes endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)] // API response fields - may be used later
pub struct EpisodesResponse {
  pub items: Vec<MediaItem>,
  pub total_record_count: i32,
}
