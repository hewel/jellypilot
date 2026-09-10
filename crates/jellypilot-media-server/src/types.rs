//! Jellyfin API types.
//!
//! These types mirror the Jellyfin API responses and requests.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use super::intro_skipper::IntroSkipRange;

/// Playback implementation selected for new sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackEngineKind {
  EmbeddedWeb,
  ExternalMpv,
}

/// Authentication response from Jellyfin.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)] // API response fields - may be used later
pub struct AuthResponse {
  pub user: User,
  pub access_token: String,
  pub server_id: String,
}

impl fmt::Debug for AuthResponse {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("AuthResponse")
      .field("user", &self.user)
      .field("access_token", &"[redacted]")
      .field("server_id", &self.server_id)
      .finish()
  }
}

impl Drop for AuthResponse {
  fn drop(&mut self) {
    self.access_token.zeroize();
  }
}

/// Jellyfin user information.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct User {
  pub id: String,
  pub name: String,
}

/// Server information.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ServerInfo {
  pub server_name: String,
  pub version: String,
  pub id: String,
}

/// Connection state exposed to frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionState {
  pub provider: MediaServerProvider,
  pub capabilities: ProviderCapabilities,
  pub connected: bool,
  pub server_url: Option<String>,
  pub server_name: Option<String>,
  pub user_id: Option<String>,
  pub user_name: Option<String>,
}

/// Feature capabilities exposed by the active or selected media server provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
  pub quick_connect: bool,
  pub intro_skipper: bool,
  pub remote_control: bool,
  pub remote_control_available: bool,
  pub remote_control_warning: Option<String>,
}

/// Media server provider selected for a connection or saved service profile.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MediaServerProvider {
  Jellyfin,
  Emby,
}

impl MediaServerProvider {
  pub const fn jellyfin() -> Self {
    Self::Jellyfin
  }
}

/// Library Browser landing data exposed to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoHome {
  pub continue_watching: Vec<VideoLibraryItem>,
  pub next_up: Vec<VideoLibraryItem>,
}

/// Video library shortcut for drilling into Movies or Shows libraries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoLibraryShortcut {
  pub id: String,
  pub name: String,
  pub collection_type: String,
  pub item_count: Option<i32>,
  pub artwork_image_id: Option<String>,
}

/// Latest media for one video library, ordered with the user's library shortcuts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLatestRow {
  pub library_id: String,
  pub library_name: String,
  pub result: Result<Vec<VideoLibraryItem>, String>,
}

/// Supported video library browse families.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum VideoLibraryKind {
  #[serde(rename = "movies")]
  Movies,
  #[serde(rename = "tvshows")]
  TvShows,
}

/// Paged Library Browser listing request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoLibraryPageRequest {
  pub library_id: String,
  pub collection_type: VideoLibraryKind,
  pub start_index: i32,
  pub limit: i32,
  pub sort: VideoLibrarySort,
  pub sort_direction: VideoLibrarySortDirection,
  pub played_filter: VideoLibraryPlayedFilter,
  pub favorites_only: bool,
}

/// Supported Library Browser sort options.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum VideoLibrarySort {
  #[serde(rename = "title")]
  Title,
  #[serde(rename = "recentlyAdded")]
  RecentlyAdded,
  #[serde(rename = "releaseDate")]
  ReleaseDate,
}

/// Supported Library Browser sort directions.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum VideoLibrarySortDirection {
  #[serde(rename = "asc")]
  Ascending,
  #[serde(rename = "desc")]
  Descending,
}

/// Supported played-state filters for Library Browser results.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum VideoLibraryPlayedFilter {
  #[serde(rename = "all")]
  All,
  #[serde(rename = "played")]
  Played,
  #[serde(rename = "unplayed")]
  Unplayed,
}

/// Paged Library Browser listing result.
#[derive(Debug, Clone, Serialize)]
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

/// Root-level page request for the current user's video Favorites.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesPageRequest {
  pub start_index: i32,
  pub limit: i32,
}

/// Root-level page of the current user's favorite videos.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesPage {
  pub start_index: i32,
  pub limit: i32,
  pub total_record_count: i32,
  pub has_more: bool,
  pub items: Vec<VideoLibraryItem>,
}

/// Root-level request for the current user's played and resumable Movie/Episode records.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchHistoryPageRequest {
  pub start_index: i32,
  pub limit: i32,
}

/// Disjoint played and unplayed-resumable items merged by descending DatePlayed.
///
/// Missing dates sort last. This is a latest-item listing, not a playback event log.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchHistoryPage {
  pub start_index: i32,
  pub limit: i32,
  pub total_record_count: i32,
  pub has_more: bool,
  pub items: Vec<VideoLibraryItem>,
}

/// Media card summary for Video Home rows, Movies and Shows browse results, episode rows, and recommendation shelves.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoLibraryItem {
  pub id: String,
  pub name: String,
  pub item_type: String,
  pub production_year: Option<i32>,
  /// Premiere timestamp supplied by the server, encoded as RFC 3339.
  pub premiere_date: Option<String>,
  /// Community rating supplied by the provider.
  pub community_rating: Option<f32>,
  /// Server episode count for Series items; unavailable when provider count semantics are ambiguous.
  pub episode_count: Option<u32>,
  /// Current user's last played timestamp from server user data, encoded as RFC 3339.
  pub last_played_date: Option<String>,
  pub runtime_seconds: Option<f64>,
  pub played: bool,
  pub favorite: bool,
  pub artwork_image_id: Option<String>,
  pub backdrop_image_id: Option<String>,
  /// Title Logo image; episode items carry the parent series logo.
  pub logo_image_id: Option<String>,
  /// Series primary poster for episode player-bar thumbs; absent for movies.
  pub series_poster_image_id: Option<String>,
  /// Episode's own Thumb image; absent for non-episode items or when no Thumb exists.
  pub episode_thumb_image_id: Option<String>,
  /// Series-level Thumb image for episode cards.
  pub series_thumb_image_id: Option<String>,
  /// First series backdrop for episode cards.
  pub series_backdrop_image_id: Option<String>,
  /// Season primary poster for episode cards.
  pub season_poster_image_id: Option<String>,
  /// Episode metadata: season number (1-based), available for Episode items.
  pub season_number: Option<i32>,
  /// Episode metadata: episode number within season (1-based), available for Episode items.
  pub episode_number: Option<i32>,
  /// Last episode number when a DTO represents an episode range.
  pub index_number_end: Option<u32>,
  /// Episode metadata: parent series id, available for Episode items.
  pub series_id: Option<String>,
  /// Episode metadata: parent series name, available for Episode items.
  pub series_name: Option<String>,
  /// Ending year for a completed series.
  pub end_year: Option<i32>,
  /// Whether the series status is Continuing.
  pub series_continuing: bool,
  /// Total unplayed children reported in user data.
  pub unplayed_item_count: Option<u32>,
  /// Resume position in seconds, populated for episode rows.
  pub resume_position_seconds: Option<f64>,
  /// Percentage watched (0–100), populated for episode rows.
  pub played_percentage: Option<f64>,
  /// Synopsis text for rich detail rows and recommendation cards.
  pub overview: Option<String>,
}

/// Paged video-only Library search request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSearchRequest {
  pub query: String,
  pub start_index: i32,
  pub limit: i32,
}

/// Paged video-only Library search result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSearchPage {
  pub query: String,
  pub start_index: i32,
  pub limit: i32,
  pub total_record_count: i32,
  pub has_more: bool,
  pub items: Vec<VideoLibraryItem>,
}

/// A credited actor with optional character and server-provided portrait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoCastMember {
  pub name: String,
  pub role: Option<String>,
  /// Encoded Library Image reference; absent without a person ID and image tag.
  pub image_id: Option<String>,
}

/// Provider-neutral credits and ratings shared by item and show detail views.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetailMetadata {
  pub community_rating: Option<f32>,
  pub official_rating: Option<String>,
  pub creators: Vec<String>,
  pub cast: Vec<VideoCastMember>,
}

/// File-level media facts for the detail media-info section.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMediaInfo {
  pub container: Option<String>,
  pub size_bytes: Option<u64>,
  pub bitrate_bps: Option<u64>,
  pub video_codec: Option<String>,
  pub video_width: Option<u32>,
  pub video_height: Option<u32>,
  /// e.g. "SDR", "HDR10", "DoVi" from the video stream range metadata.
  pub video_range: Option<String>,
  pub audio_streams: Vec<VideoStreamInfo>,
  pub subtitle_streams: Vec<VideoStreamInfo>,
}

/// Audio or subtitle stream facts for the detail media-info section.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStreamInfo {
  pub codec: Option<String>,
  pub language: Option<String>,
  /// Audio channel count; absent for subtitles.
  pub channels: Option<u32>,
  /// Server-formatted label, e.g. "English - AAC 2.0".
  pub display_title: Option<String>,
}

/// Playable Movie or Episode detail data exposed to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoItemDetail {
  pub id: String,
  pub name: String,
  pub item_type: String,
  pub overview: Option<String>,
  /// ISO language code of the item's original audio, when the server exposes one
  /// (Jellyfin only; episodes inherit the series value via a lookup in the client).
  pub original_language: Option<String>,
  pub production_year: Option<i32>,
  pub runtime_seconds: Option<f64>,
  pub series_id: Option<String>,
  pub series_name: Option<String>,
  pub season_number: Option<i32>,
  pub episode_number: Option<i32>,
  pub genres: Vec<String>,
  pub played: bool,
  pub favorite: bool,
  pub played_percentage: Option<f64>,
  pub resume_position_seconds: Option<f64>,
  pub can_resume: bool,
  pub can_play: bool,
  pub artwork_image_id: Option<String>,
  pub backdrop_image_id: Option<String>,
  /// Title Logo image; episode items carry the parent series logo.
  pub logo_image_id: Option<String>,
  /// Series primary poster for episode player-bar thumbs; absent for movies.
  pub series_poster_image_id: Option<String>,
  pub media_info: Option<VideoMediaInfo>,
  pub metadata: VideoDetailMetadata,
}

/// Selectable audio or subtitle stream exposed before Library playback starts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPlaybackStreamOption {
  pub index: i32,
  pub label: String,
  pub language: Option<String>,
  pub codec: Option<String>,
  pub is_default: bool,
  pub is_external: bool,
}

/// Audio and subtitle metadata loaded after the critical item detail.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoItemStreams {
  pub audio_streams: Vec<VideoPlaybackStreamOption>,
  pub subtitle_streams: Vec<VideoPlaybackStreamOption>,
}

/// Show detail data with seasons and Jellyfin next playable episode.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoShowDetail {
  pub id: String,
  pub name: String,
  pub overview: Option<String>,
  pub production_year: Option<i32>,
  pub genres: Vec<String>,
  pub played: bool,
  /// ISO language code of the show's original audio, when the server exposes one
  /// (Jellyfin only).
  pub original_language: Option<String>,
  pub favorite: bool,
  pub can_play: bool,
  pub artwork_image_id: Option<String>,
  pub backdrop_image_id: Option<String>,
  /// Title Logo image.
  pub logo_image_id: Option<String>,
  pub next_episode: Option<VideoLibraryItem>,
  pub seasons: Vec<VideoSeason>,
  pub metadata: VideoDetailMetadata,
}

/// Season summary for a Show detail page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSeason {
  pub id: String,
  pub name: String,
  pub season_number: Option<i32>,
  pub played: bool,
  pub favorite: bool,
  pub artwork_image_id: Option<String>,
}

/// Request for episodes inside a show season.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSeasonEpisodesRequest {
  pub series_id: String,
  pub season_id: Option<String>,
  pub season_number: Option<i32>,
}

/// Episode list for a selected season.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSeasonEpisodes {
  pub series_id: String,
  pub season_id: Option<String>,
  pub season_number: Option<i32>,
  pub episodes: Vec<VideoLibraryItem>,
}

/// Bounded request for a page of episodes inside a show season.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSeasonEpisodesPageRequest {
  pub series_id: String,
  pub season_id: Option<String>,
  pub season_number: Option<i32>,
  pub start_index: i32,
  pub limit: i32,
}

/// Bounded page of episodes for a selected season.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSeasonEpisodesPage {
  pub series_id: String,
  pub season_id: Option<String>,
  pub season_number: Option<i32>,
  pub start_index: i32,
  pub limit: i32,
  pub total_record_count: i32,
  pub next_start_index: i32,
  pub has_more: bool,
  pub episodes: Vec<VideoLibraryItem>,
}

#[derive(Debug, Clone)]
pub struct VideoPlaybackTarget {
  pub item_id: String,
  pub start_position_ticks: Option<i64>,
}

/// User data action supported by Library Browser detail views.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum VideoUserDataAction {
  #[serde(rename = "favorite")]
  Favorite,
  #[serde(rename = "unfavorite")]
  Unfavorite,
  #[serde(rename = "markPlayed")]
  MarkPlayed,
  #[serde(rename = "markUnplayed")]
  MarkUnplayed,
}

/// User-scoped Jellyfin user data mutation request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoUserDataUpdateRequest {
  pub item_id: String,
  pub action: VideoUserDataAction,
}

/// Updated user data returned by Jellyfin after a mutation succeeds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoUserDataUpdate {
  pub item_id: String,
  pub played: bool,
  pub favorite: bool,
}

/// Credentials for authentication.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
  #[serde(default = "MediaServerProvider::jellyfin")]
  pub provider: MediaServerProvider,
  pub server_url: String,
  pub username: String,
  pub password: String,
}

impl fmt::Debug for Credentials {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("Credentials")
      .field("provider", &self.provider)
      .field("server_url", &"[redacted]")
      .field("username", &self.username)
      .field("password", &"[redacted]")
      .finish()
  }
}

/// Quick Connect request created by the server.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickConnectRequest {
  pub code: String,
  pub secret: String,
}

impl QuickConnectRequest {
  /// Consume the response while transferring its public code and sensitive pairing secret.
  ///
  /// The remaining in-struct secret is empty before [`Drop`] runs.
  pub fn into_parts(mut self) -> (String, String) {
    (
      std::mem::take(&mut self.code),
      std::mem::take(&mut self.secret),
    )
  }
}

impl fmt::Debug for QuickConnectRequest {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("QuickConnectRequest")
      .field("code", &"[redacted]")
      .field("secret", &"[redacted]")
      .finish()
  }
}

impl Drop for QuickConnectRequest {
  fn drop(&mut self) {
    self.secret.zeroize();
  }
}

/// Quick Connect request status exposed to the frontend.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Deserialize, Serialize)]
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
  #[serde(default)]
  pub series_primary_image_tag: Option<String>,
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
  pub default_subtitle_stream_index: Option<i32>,
  /// Server-computed default audio stream (user-preference aware), playback-info only.
  #[serde(default)]
  pub default_audio_stream_index: Option<i32>,
  #[serde(default)]
  pub supports_direct_play: bool,
  #[serde(default)]
  pub supports_direct_stream: bool,
  #[serde(default)]
  pub supports_transcoding: bool,
  #[serde(default)]
  pub direct_stream_url: Option<String>,
  #[serde(default)]
  pub add_api_key_to_direct_stream_url: Option<bool>,
  #[serde(default)]
  pub transcoding_url: Option<String>,
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
  pub video_range: Option<String>,
  #[serde(default)]
  pub video_range_type: Option<String>,
  #[serde(default)]
  pub color_transfer: Option<String>,
  #[serde(default)]
  pub bit_depth: Option<i32>,
  #[serde(default)]
  pub channels: Option<i32>,
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
pub(crate) struct PlaybackInfoRequest {
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
  pub play_method: String,
  /// Active HLS proxy generation for Emby VOD transcodes.
  pub hls_proxy_session_id: Option<String>,
  /// Whether a one-shot Emby transcode expiry recovery was already attempted.
  pub hls_recovery_attempted: bool,
  /// True while a transcode expiry recovery is in flight; suppresses progress reports.
  pub hls_recovering: bool,
}

/// Ticks conversion helpers (1 tick = 100 nanoseconds).
const TICKS_PER_SECOND: i64 = 10_000_000;

/// Convert seconds to ticks.
pub fn seconds_to_ticks(seconds: f64) -> i64 {
  (seconds * TICKS_PER_SECOND as f64) as i64
}

/// Convert ticks to seconds.
pub fn ticks_to_seconds(ticks: i64) -> f64 {
  ticks as f64 / TICKS_PER_SECOND as f64
}

/// Saved session data for persistence.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSession {
  #[serde(default = "MediaServerProvider::jellyfin")]
  pub provider: MediaServerProvider,
  pub server_url: String,
  pub access_token: String,
  pub user_id: String,
  pub user_name: String,
  pub server_name: Option<String>,
  pub device_id: Option<String>,
}

impl fmt::Debug for SavedSession {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("SavedSession")
      .field("provider", &self.provider)
      .field("server_url", &"[redacted]")
      .field("access_token", &"[redacted]")
      .field("user_id", &self.user_id)
      .field("user_name", &self.user_name)
      .field("server_name", &self.server_name)
      .field("device_id", &self.device_id)
      .finish()
  }
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
fn find_stream_by_lang(streams: &[MediaStream], stream_type: &str, lang: &str) -> Option<i32> {
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
fn find_stream_by_language_priority(
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

/// Original-language code and selectable audio streams used to choose an audio
/// track automatically before playback starts.
#[derive(Debug, Clone, Default)]
pub struct PlaybackAudioContext {
  pub original_language: Option<String>,
  pub audio_streams: Vec<VideoPlaybackStreamOption>,
}

/// Borrowed audio-stream facts needed to choose a track automatically.
#[derive(Debug, Clone, Copy)]
pub struct AudioStreamChoice<'a> {
  pub index: i32,
  pub language: Option<&'a str>,
  pub display_title: Option<&'a str>,
  pub is_default: bool,
}

impl<'a> From<&'a MediaStream> for AudioStreamChoice<'a> {
  fn from(stream: &'a MediaStream) -> Self {
    Self {
      index: stream.index,
      language: stream.language.as_deref(),
      display_title: stream.display_title.as_deref(),
      is_default: stream.is_default,
    }
  }
}

impl<'a> From<&'a VideoPlaybackStreamOption> for AudioStreamChoice<'a> {
  /// The option label stands in for the display title; on the rare generated
  /// fallback label a stored title match simply misses and language matching applies.
  fn from(stream: &'a VideoPlaybackStreamOption) -> Self {
    Self {
      index: stream.index,
      language: stream.language.as_deref(),
      display_title: Some(stream.label.as_str()),
      is_default: stream.is_default,
    }
  }
}

/// Normalize a language tag for comparison: lowercase, region subtag stripped, and
/// ISO 639-2 (bibliographic and terminology) reduced to ISO 639-1 when a two-letter
/// code exists. Unrecognized values survive lowercased so identical raw tags still
/// match each other; empty input yields no language.
#[must_use]
pub fn normalize_language(code: &str) -> Option<String> {
  let base = code
    .trim()
    .split(['-', '_'])
    .next()
    .unwrap_or("")
    .trim()
    .to_lowercase();
  if base.is_empty() {
    return None;
  }
  Some(match iso_639_2_to_1(&base) {
    Some(mapped) => mapped.to_owned(),
    None => base,
  })
}

/// Compare two server-supplied language tags (e.g. "ja" vs "jpn", "zh-CN" vs "zho").
#[must_use]
pub fn languages_match(a: &str, b: &str) -> bool {
  match (normalize_language(a), normalize_language(b)) {
    (Some(a), Some(b)) => a == b,
    _ => false,
  }
}

fn iso_639_2_to_1(code: &str) -> Option<&'static str> {
  Some(match code {
    "aar" => "aa",
    "abk" => "ab",
    "afr" => "af",
    "aka" => "ak",
    "amh" => "am",
    "ara" => "ar",
    "arg" => "an",
    "asm" => "as",
    "ava" => "av",
    "ave" => "ae",
    "aym" => "ay",
    "aze" => "az",
    "bak" => "ba",
    "bam" => "bm",
    "bel" => "be",
    "ben" => "bn",
    "bis" => "bi",
    "bod" | "tib" => "bo",
    "bos" => "bs",
    "bre" => "br",
    "bul" => "bg",
    "cat" => "ca",
    "ces" | "cze" => "cs",
    "cha" => "ch",
    "che" => "ce",
    "chv" => "cv",
    "cor" => "kw",
    "cos" => "co",
    "cre" => "cr",
    "cym" | "wel" => "cy",
    "dan" => "da",
    "deu" | "ger" => "de",
    "div" => "dv",
    "dzo" => "dz",
    "ell" | "gre" => "el",
    "eng" => "en",
    "epo" => "eo",
    "est" => "et",
    "eus" | "baq" => "eu",
    "ewe" => "ee",
    "fao" => "fo",
    "fas" | "per" => "fa",
    "fij" => "fj",
    "fin" => "fi",
    "fra" | "fre" => "fr",
    "fry" => "fy",
    "ful" => "ff",
    "gla" => "gd",
    "gle" => "ga",
    "glg" => "gl",
    "glv" => "gv",
    "grn" => "gn",
    "guj" => "gu",
    "hat" => "ht",
    "hau" => "ha",
    "heb" => "he",
    "her" => "hz",
    "hin" => "hi",
    "hmo" => "ho",
    "hrv" => "hr",
    "hun" => "hu",
    "hye" | "arm" => "hy",
    "ibo" => "ig",
    "ido" => "io",
    "iku" => "iu",
    "ile" => "ie",
    "ina" => "ia",
    "ind" => "id",
    "ipk" => "ik",
    "isl" | "ice" => "is",
    "ita" => "it",
    "jav" => "jv",
    "jpn" => "ja",
    "kal" => "kl",
    "kan" => "kn",
    "kas" => "ks",
    "kat" | "geo" => "ka",
    "kaz" => "kk",
    "khm" => "km",
    "kik" => "ki",
    "kin" => "rw",
    "kir" => "ky",
    "kom" => "kv",
    "kon" => "kg",
    "kor" => "ko",
    "kua" => "kj",
    "kur" => "ku",
    "lao" => "lo",
    "lat" => "la",
    "lav" => "lv",
    "lim" => "li",
    "lin" => "ln",
    "lit" => "lt",
    "ltz" => "lb",
    "lub" => "lu",
    "mah" => "mh",
    "mal" => "ml",
    "mar" => "mr",
    "mkd" | "mac" => "mk",
    "mlg" => "mg",
    "mlt" => "mt",
    "mon" => "mn",
    "mri" | "mao" => "mi",
    "msa" | "may" => "ms",
    "mya" | "bur" => "my",
    "nau" => "na",
    "nav" => "nv",
    "nbl" => "nr",
    "nde" => "nd",
    "ndo" => "ng",
    "nep" => "ne",
    "nld" | "dut" => "nl",
    "nno" => "nn",
    "nob" => "nb",
    "nor" => "no",
    "nya" => "ny",
    "oci" => "oc",
    "oji" => "oj",
    "ori" => "or",
    "orm" => "om",
    "oss" => "os",
    "pan" => "pa",
    "pli" => "pi",
    "pol" => "pl",
    "por" => "pt",
    "pus" => "ps",
    "que" => "qu",
    "roh" => "rm",
    "ron" | "rum" => "ro",
    "run" => "rn",
    "rus" => "ru",
    "sag" => "sg",
    "san" => "sa",
    "sin" => "si",
    "slk" | "slo" => "sk",
    "slv" => "sl",
    "sme" => "se",
    "smo" => "sm",
    "sna" => "sn",
    "snd" => "sd",
    "som" => "so",
    "sot" => "st",
    "spa" => "es",
    "sqi" | "alb" => "sq",
    "srp" => "sr",
    "ssw" => "ss",
    "sun" => "su",
    "swa" => "sw",
    "swe" => "sv",
    "tah" => "ty",
    "tam" => "ta",
    "tat" => "tt",
    "tel" => "te",
    "tgk" => "tg",
    "tgl" => "tl",
    "tha" => "th",
    "tir" => "ti",
    "ton" => "to",
    "tsn" => "tn",
    "tso" => "ts",
    "tuk" => "tk",
    "tur" => "tr",
    "twi" => "tw",
    "uig" => "ug",
    "ukr" => "uk",
    "urd" => "ur",
    "uzb" => "uz",
    "ven" => "ve",
    "vie" => "vi",
    "vol" => "vo",
    "wln" => "wa",
    "wol" => "wo",
    "xho" => "xh",
    "yid" => "yi",
    "yor" => "yo",
    "zha" => "za",
    "zho" | "chi" => "zh",
    "zul" => "zu",
    _ => return None,
  })
}

/// Commentary and similar non-primary tracks are never the original-language choice.
#[must_use]
pub fn is_commentary_title(title: &str) -> bool {
  let title = title.to_lowercase();
  ["commentary", "评论", "解说", "コメンタリー", "評論"]
    .iter()
    .any(|keyword| title.contains(keyword))
}

/// Select the audio stream for the item's original language. The server's own
/// default audio stream (user-preference aware, when known) wins when it matches,
/// then the container-default stream, then the first non-commentary stream in that
/// language. No match yields `None` so playback keeps its default.
#[must_use]
pub fn select_native_audio_stream(
  streams: &[AudioStreamChoice<'_>],
  original_language: &str,
  server_default_index: Option<i32>,
) -> Option<i32> {
  if let Some(stream) =
    server_default_index.and_then(|index| streams.iter().find(|stream| stream.index == index))
  {
    if stream
      .language
      .is_some_and(|l| languages_match(l, original_language))
      && !stream.display_title.is_some_and(is_commentary_title)
    {
      return Some(stream.index);
    }
  }
  let mut first: Option<&AudioStreamChoice<'_>> = None;
  for stream in streams {
    let Some(language) = stream.language else {
      continue;
    };
    if !languages_match(language, original_language) {
      continue;
    }
    if stream.display_title.is_some_and(is_commentary_title) {
      continue;
    }
    if stream.is_default {
      return Some(stream.index);
    }
    first = first.or(Some(stream));
  }
  first.map(|stream| stream.index)
}

/// Match a remembered audio choice: exact language and display title first (handles
/// same-language variants), then language only. Returns `None` when nothing matches,
/// e.g. the track vanished from a new encode.
#[must_use]
pub fn select_audio_stream_by_memory(
  streams: &[AudioStreamChoice<'_>],
  language: &str,
  title: Option<&str>,
) -> Option<i32> {
  if let Some(title) = title {
    if let Some(stream) = streams.iter().find(|stream| {
      stream
        .language
        .is_some_and(|l| languages_match(l, language))
        && stream.display_title == Some(title)
    }) {
      return Some(stream.index);
    }
  }
  streams
    .iter()
    .find(|stream| {
      stream
        .language
        .is_some_and(|l| languages_match(l, language))
    })
    .map(|stream| stream.index)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn saved_session_defaults_missing_provider_to_jellyfin() {
    let session: SavedSession = serde_json::from_value(serde_json::json!({
      "serverUrl": "https://jellyfin.example.com",
      "accessToken": "token-1",
      "userId": "user-1",
      "userName": "Ada",
      "serverName": "Jellyfin Home",
      "deviceId": "device-1"
    }))
    .expect("legacy saved session should deserialize");

    assert_eq!(session.provider, MediaServerProvider::Jellyfin);
  }

  #[test]
  fn credentials_default_missing_provider_to_jellyfin() {
    let credentials: Credentials = serde_json::from_value(serde_json::json!({
      "serverUrl": "https://jellyfin.example.com",
      "username": "Ada",
      "password": "secret"
    }))
    .expect("legacy credentials should deserialize");

    assert_eq!(credentials.provider, MediaServerProvider::Jellyfin);
  }

  #[test]
  fn auth_response_debug_redacts_access_token() {
    let response = AuthResponse {
      user: User {
        id: "user-1".to_string(),
        name: "Ada".to_string(),
      },
      access_token: "auth-secret-token".to_string(),
      server_id: "server-1".to_string(),
    };

    assert!(!format!("{response:?}").contains("auth-secret-token"));
  }

  #[test]
  fn credentials_debug_redacts_password_and_server_url() {
    let credentials = Credentials {
      provider: MediaServerProvider::Jellyfin,
      server_url: "https://user:url-secret@media.example.com".to_string(),
      username: "Ada".to_string(),
      password: "password-secret".to_string(),
    };
    let debug = format!("{credentials:?}");

    assert!(!debug.contains("password-secret") && !debug.contains("url-secret"));
  }

  #[test]
  fn quick_connect_request_debug_redacts_pairing_material() {
    let request = QuickConnectRequest {
      code: "PAIR-CODE".to_string(),
      secret: "pairing-secret".to_string(),
    };
    let debug = format!("{request:?}");

    assert!(!debug.contains("PAIR-CODE") && !debug.contains("pairing-secret"));
  }

  #[test]
  fn saved_session_debug_redacts_access_token_and_server_url() {
    let session = SavedSession {
      provider: MediaServerProvider::Emby,
      server_url: "https://user:url-secret@media.example.com/emby".to_string(),
      access_token: "session-secret-token".to_string(),
      user_id: "user-1".to_string(),
      user_name: "Ada".to_string(),
      server_name: Some("Home".to_string()),
      device_id: Some("device-1".to_string()),
    };
    let debug = format!("{session:?}");

    assert!(!debug.contains("session-secret-token") && !debug.contains("url-secret"));
  }

  #[test]
  fn playback_progress_serializes_to_shared_server_payload_shape() {
    let progress = PlaybackProgressInfo {
      item_id: "movie-1".to_string(),
      media_source_id: Some("source-1".to_string()),
      play_session_id: Some("play-1".to_string()),
      position_ticks: Some(900_000_000),
      is_paused: true,
      is_muted: false,
      volume_level: 65,
      audio_stream_index: Some(1),
      subtitle_stream_index: Some(2),
      play_method: "DirectStream".to_string(),
      can_seek: true,
    };

    let payload = serde_json::to_value(progress).expect("progress should serialize");

    assert_eq!(
      payload,
      serde_json::json!({
        "ItemId": "movie-1",
        "MediaSourceId": "source-1",
        "PlaySessionId": "play-1",
        "PositionTicks": 900000000,
        "IsPaused": true,
        "IsMuted": false,
        "VolumeLevel": 65,
        "AudioStreamIndex": 1,
        "SubtitleStreamIndex": 2,
        "PlayMethod": "DirectStream",
        "CanSeek": true
      })
    );
  }

  #[test]
  fn playback_stop_serializes_to_shared_server_payload_shape() {
    let stopped = PlaybackStopInfo {
      item_id: "movie-1".to_string(),
      media_source_id: Some("source-1".to_string()),
      play_session_id: Some("play-1".to_string()),
      position_ticks: Some(1_230_000_000),
    };

    let payload = serde_json::to_value(stopped).expect("stop should serialize");

    assert_eq!(
      payload,
      serde_json::json!({
        "ItemId": "movie-1",
        "MediaSourceId": "source-1",
        "PlaySessionId": "play-1",
        "PositionTicks": 1230000000
      })
    );
  }

  fn stream(index: i32, stream_type: &str, language: Option<&str>) -> MediaStream {
    MediaStream {
      index,
      stream_type: stream_type.to_string(),
      codec: None,
      video_range: None,
      video_range_type: None,
      color_transfer: None,
      bit_depth: None,
      channels: None,
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

  fn choice<'a>(
    index: i32,
    language: Option<&'a str>,
    display_title: Option<&'a str>,
    is_default: bool,
  ) -> AudioStreamChoice<'a> {
    AudioStreamChoice {
      index,
      language,
      display_title,
      is_default,
    }
  }

  #[test]
  fn normalize_language_reduces_codes_to_a_comparable_base() {
    let cases = [
      ("ja", Some("ja")),
      ("jpn", Some("ja")),
      ("JPN", Some("ja")),
      ("eng", Some("en")),
      ("EN", Some("en")),
      ("zh-CN", Some("zh")),
      ("zh_Hans", Some("zh")),
      ("zho", Some("zh")),
      ("chi", Some("zh")),
      ("fre", Some("fr")),
      ("fra", Some("fr")),
      ("ger", Some("de")),
      ("deu", Some("de")),
      ("pt-BR", Some("pt")),
      ("por", Some("pt")),
      ("und", Some("und")),
      ("nob", Some("nb")),
      ("nno", Some("nn")),
      ("nb", Some("nb")),
      ("", None),
      ("  ", None),
    ];
    for (input, expected) in cases {
      assert_eq!(
        normalize_language(input).as_deref(),
        expected,
        "input {input:?}"
      );
    }
  }

  #[test]
  fn languages_match_handles_code_families_and_regions() {
    assert!(languages_match("ja", "jpn"));
    assert!(languages_match("eng", "en"));
    assert!(languages_match("zh-Hans", "chi"));
    assert!(!languages_match("jpn", "eng"));
    assert!(!languages_match("", "jpn"));
    assert!(!languages_match("jpn", ""));
  }

  #[test]
  fn commentary_titles_are_detected_across_languages() {
    assert!(is_commentary_title("Director's Commentary"));
    assert!(is_commentary_title("Japanese commentary track"));
    assert!(is_commentary_title("评论音轨"));
    assert!(is_commentary_title("导演解说"));
    assert!(is_commentary_title("コメンタリー版"));
    assert!(!is_commentary_title("Japanese - AAC 5.1"));
  }

  #[test]
  fn native_selection_prefers_the_server_default_matching_stream() {
    // The matching default sorts last: it must still win over the earlier match.
    let streams = [
      choice(1, Some("jpn"), Some("Japanese - AAC 2.0"), false),
      choice(2, Some("eng"), Some("English - AAC 5.1"), true),
      choice(3, Some("jpn"), Some("Japanese - DTS 5.1"), true),
    ];
    assert_eq!(select_native_audio_stream(&streams, "ja", None), Some(3));
    assert_eq!(select_native_audio_stream(&streams, "jpn", None), Some(3));
  }

  #[test]
  fn native_selection_prefers_the_server_default_audio_index() {
    // The server's user-preference-aware default (index 2) wins over the
    // container-default flag (index 3) when both match the original language.
    let streams = [
      choice(1, Some("eng"), Some("English - AAC 5.1"), false),
      choice(2, Some("jpn"), Some("Japanese - AAC 2.0"), false),
      choice(3, Some("jpn"), Some("Japanese - DTS 5.1"), true),
    ];
    assert_eq!(select_native_audio_stream(&streams, "ja", Some(2)), Some(2));
    // A server default in another language is ignored: the original language rules.
    assert_eq!(select_native_audio_stream(&streams, "ja", Some(1)), Some(3));
    // A commentary server default never wins, even in the original language.
    let commentary_default = [choice(4, Some("jpn"), Some("Japanese Commentary"), false)];
    assert_eq!(
      select_native_audio_stream(&commentary_default, "ja", Some(4)),
      None
    );
  }

  #[test]
  fn native_selection_falls_back_to_first_non_commentary_match() {
    let streams = [
      choice(1, Some("eng"), Some("English - AAC 5.1"), true),
      choice(2, Some("jpn"), Some("Japanese Commentary"), false),
      choice(3, Some("jpn"), Some("Japanese - AAC 2.0"), false),
      choice(4, None, Some("Unknown"), false),
    ];
    assert_eq!(select_native_audio_stream(&streams, "ja", None), Some(3));
  }

  #[test]
  fn native_selection_returns_none_without_a_usable_match() {
    let only_commentary = [choice(2, Some("jpn"), Some("Japanese Commentary"), false)];
    assert_eq!(
      select_native_audio_stream(&only_commentary, "ja", None),
      None
    );
    let no_match = [choice(1, Some("eng"), None, true)];
    assert_eq!(select_native_audio_stream(&no_match, "ja", None), None);
  }

  #[test]
  fn memory_selection_matches_title_then_language() {
    let streams = [
      choice(1, Some("jpn"), Some("Japanese - AAC 2.0"), false),
      choice(2, Some("jpn"), Some("Japanese - DTS 5.1"), false),
      choice(3, Some("eng"), Some("English - AAC 5.1"), true),
    ];
    assert_eq!(
      select_audio_stream_by_memory(&streams, "ja", Some("Japanese - DTS 5.1")),
      Some(2)
    );
    // Title from a previous encode no longer exists: language still applies.
    assert_eq!(
      select_audio_stream_by_memory(&streams, "ja", Some("Japanese - FLAC 7.1")),
      Some(1)
    );
    assert_eq!(select_audio_stream_by_memory(&streams, "ja", None), Some(1));
    assert_eq!(select_audio_stream_by_memory(&streams, "ko", None), None);
  }
}

/// Response from /Shows/{seriesId}/Episodes endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)] // API response fields - may be used later
pub(crate) struct EpisodesResponse {
  pub items: Vec<MediaItem>,
  pub total_record_count: i32,
}
