//! UniFFI DTOs mirroring the shared domain types.
//!
//! These records are the Kotlin-facing shapes; conversion to and from the
//! media-server domain types lives here so the FFI surface stays typed
//! instead of collapsing to strings.

use jellypilot_media_server as ms;

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum Provider {
    Jellyfin,
    Emby,
}

impl From<Provider> for ms::MediaServerProvider {
    fn from(provider: Provider) -> Self {
        match provider {
            Provider::Jellyfin => Self::Jellyfin,
            Provider::Emby => Self::Emby,
        }
    }
}

impl From<ms::MediaServerProvider> for Provider {
    fn from(provider: ms::MediaServerProvider) -> Self {
        match provider {
            ms::MediaServerProvider::Jellyfin => Self::Jellyfin,
            ms::MediaServerProvider::Emby => Self::Emby,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SdkConfig {
    pub storage_dir: String,
    pub device_name: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ProviderCapabilities {
    pub quick_connect: bool,
    pub intro_skipper: bool,
    pub remote_control: bool,
    pub remote_control_available: bool,
    pub remote_control_warning: Option<String>,
}

impl From<ms::ProviderCapabilities> for ProviderCapabilities {
    fn from(capabilities: ms::ProviderCapabilities) -> Self {
        Self {
            quick_connect: capabilities.quick_connect,
            intro_skipper: capabilities.intro_skipper,
            remote_control: capabilities.remote_control,
            remote_control_available: capabilities.remote_control_available,
            remote_control_warning: capabilities.remote_control_warning,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SavedProfile {
    pub key: String,
    pub provider: Provider,
    pub server_url: String,
    pub server_name: Option<String>,
    pub user_name: String,
    /// Redacted `user@server` label for list presentation.
    pub title: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SavedProfilesSnapshot {
    pub profiles: Vec<SavedProfile>,
    /// Key of the only profile eligible for startup restore.
    pub last_activated_key: Option<String>,
}

impl From<&jellypilot_auth::SavedProfileSummary> for SavedProfile {
    fn from(profile: &jellypilot_auth::SavedProfileSummary) -> Self {
        Self {
            key: profile.key.as_str().to_owned(),
            provider: profile.provider().into(),
            server_url: profile.server_url.clone(),
            server_name: profile.server_name.clone(),
            user_name: profile.user_name.clone(),
            title: profile.title(),
        }
    }
}

impl From<jellypilot_auth::SavedProfilesSnapshot> for SavedProfilesSnapshot {
    fn from(snapshot: jellypilot_auth::SavedProfilesSnapshot) -> Self {
        Self {
            profiles: snapshot.profiles().iter().map(SavedProfile::from).collect(),
            last_activated_key: snapshot
                .last_successfully_activated()
                .map(|key| key.as_str().to_owned()),
        }
    }
}

/// Identity of the profile scope an `OperationToken` was minted under.
///
/// Pass it back to `JellypilotSdk.is_scope_active` or
/// `JellypilotSdk.image_target` so the SDK can bind follow-up work to the
/// exact account that issued it.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Record)]
pub struct ProfileScopeRef {
    /// Stable saved-profile key of the scope's profile.
    pub profile_key: String,
    /// Scope epoch captured when the token was minted.
    pub generation: u64,
}

impl From<jellypilot_sdk::ProfileScopeRef> for ProfileScopeRef {
    fn from(scope: jellypilot_sdk::ProfileScopeRef) -> Self {
        Self {
            profile_key: scope.profile_key,
            generation: scope.generation,
        }
    }
}

impl From<ProfileScopeRef> for jellypilot_sdk::ProfileScopeRef {
    fn from(scope: ProfileScopeRef) -> Self {
        Self {
            profile_key: scope.profile_key,
            generation: scope.generation,
        }
    }
}

/// Committed result of `activate_candidate`.
///
/// `profile` is the live active profile even when `persistence_warning` is
/// set; the warning means session or startup-restore persistence failed
/// after the committed swap, not that activation was rolled back.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ActivationOutcome {
    /// The newly active profile.
    pub profile: ActiveProfile,
    /// Non-fatal persistence failure recorded after the committed swap.
    pub persistence_warning: Option<crate::SdkError>,
}

impl From<jellypilot_sdk::ActivationOutcome> for ActivationOutcome {
    fn from(outcome: jellypilot_sdk::ActivationOutcome) -> Self {
        Self {
            profile: outcome.profile.into(),
            persistence_warning: outcome.persistence_warning.map(crate::SdkError::from),
        }
    }
}

/// Committed result of `sign_out`.
///
/// The credential deletion is committed before this value is produced.
/// `teardown_error` and `watchlist_error` report post-commit cleanup that
/// failed independently; neither implies the deletion was rolled back.
#[derive(Clone, Debug, uniffi::Record)]
pub struct SignOutOutcome {
    /// Saved profiles remaining after the deletion.
    pub remaining: Vec<SavedProfile>,
    /// Startup-restore selection after the deletion, when one remains.
    pub last_activated_key: Option<String>,
    /// Platform teardown failure recorded after the committed deletion.
    pub teardown_error: Option<crate::SdkError>,
    /// Watchlist cleanup failure recorded after the committed deletion.
    pub watchlist_error: Option<crate::SdkError>,
}

impl From<jellypilot_sdk::SignOutOutcome> for SignOutOutcome {
    fn from(outcome: jellypilot_sdk::SignOutOutcome) -> Self {
        Self {
            remaining: outcome.remaining.iter().map(SavedProfile::from).collect(),
            last_activated_key: outcome
                .last_activated_key
                .map(|key| key.as_str().to_owned()),
            teardown_error: outcome.teardown_error.map(crate::SdkError::from),
            watchlist_error: outcome.watchlist_error.map(crate::SdkError::from),
        }
    }
}

/// Committed result of `remove_saved_profile`: the storage mutation's own
/// result rather than a later reload.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ProfileRemovalOutcome {
    /// Saved profiles remaining after the deletion.
    pub remaining: Vec<SavedProfile>,
    /// Startup-restore selection after the deletion, when one remains.
    pub last_activated_key: Option<String>,
}

impl From<jellypilot_sdk::ProfileRemovalOutcome> for ProfileRemovalOutcome {
    fn from(outcome: jellypilot_sdk::ProfileRemovalOutcome) -> Self {
        Self {
            remaining: outcome.remaining.iter().map(SavedProfile::from).collect(),
            last_activated_key: outcome
                .last_activated_key
                .map(|key| key.as_str().to_owned()),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ActiveProfile {
    pub key: String,
    pub provider: Provider,
    pub server_url: String,
    pub server_name: Option<String>,
    pub user_id: String,
    pub user_name: String,
    pub capabilities: ProviderCapabilities,
}

impl From<jellypilot_sdk::ActiveProfile> for ActiveProfile {
    fn from(profile: jellypilot_sdk::ActiveProfile) -> Self {
        Self {
            key: profile.key,
            provider: profile.provider.into(),
            server_url: profile.server_url,
            server_name: profile.server_name,
            user_id: profile.user_id,
            user_name: profile.user_name,
            capabilities: profile.capabilities.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum VideoLibraryKind {
    Movies,
    TvShows,
}

impl From<VideoLibraryKind> for ms::VideoLibraryKind {
    fn from(kind: VideoLibraryKind) -> Self {
        match kind {
            VideoLibraryKind::Movies => Self::Movies,
            VideoLibraryKind::TvShows => Self::TvShows,
        }
    }
}

impl From<ms::VideoLibraryKind> for VideoLibraryKind {
    fn from(kind: ms::VideoLibraryKind) -> Self {
        match kind {
            ms::VideoLibraryKind::Movies => Self::Movies,
            ms::VideoLibraryKind::TvShows => Self::TvShows,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum VideoLibrarySort {
    Title,
    RecentlyAdded,
    ReleaseDate,
}

impl From<VideoLibrarySort> for ms::VideoLibrarySort {
    fn from(sort: VideoLibrarySort) -> Self {
        match sort {
            VideoLibrarySort::Title => Self::Title,
            VideoLibrarySort::RecentlyAdded => Self::RecentlyAdded,
            VideoLibrarySort::ReleaseDate => Self::ReleaseDate,
        }
    }
}

impl From<ms::VideoLibrarySort> for VideoLibrarySort {
    fn from(sort: ms::VideoLibrarySort) -> Self {
        match sort {
            ms::VideoLibrarySort::Title => Self::Title,
            ms::VideoLibrarySort::RecentlyAdded => Self::RecentlyAdded,
            ms::VideoLibrarySort::ReleaseDate => Self::ReleaseDate,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum VideoLibrarySortDirection {
    Ascending,
    Descending,
}

impl From<VideoLibrarySortDirection> for ms::VideoLibrarySortDirection {
    fn from(direction: VideoLibrarySortDirection) -> Self {
        match direction {
            VideoLibrarySortDirection::Ascending => Self::Ascending,
            VideoLibrarySortDirection::Descending => Self::Descending,
        }
    }
}

impl From<ms::VideoLibrarySortDirection> for VideoLibrarySortDirection {
    fn from(direction: ms::VideoLibrarySortDirection) -> Self {
        match direction {
            ms::VideoLibrarySortDirection::Ascending => Self::Ascending,
            ms::VideoLibrarySortDirection::Descending => Self::Descending,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum VideoLibraryPlayedFilter {
    All,
    Played,
    Unplayed,
}

impl From<VideoLibraryPlayedFilter> for ms::VideoLibraryPlayedFilter {
    fn from(filter: VideoLibraryPlayedFilter) -> Self {
        match filter {
            VideoLibraryPlayedFilter::All => Self::All,
            VideoLibraryPlayedFilter::Played => Self::Played,
            VideoLibraryPlayedFilter::Unplayed => Self::Unplayed,
        }
    }
}

impl From<ms::VideoLibraryPlayedFilter> for VideoLibraryPlayedFilter {
    fn from(filter: ms::VideoLibraryPlayedFilter) -> Self {
        match filter {
            ms::VideoLibraryPlayedFilter::All => Self::All,
            ms::VideoLibraryPlayedFilter::Played => Self::Played,
            ms::VideoLibraryPlayedFilter::Unplayed => Self::Unplayed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum VideoUserDataAction {
    Favorite,
    Unfavorite,
    MarkPlayed,
    MarkUnplayed,
}

impl From<VideoUserDataAction> for ms::VideoUserDataAction {
    fn from(action: VideoUserDataAction) -> Self {
        match action {
            VideoUserDataAction::Favorite => Self::Favorite,
            VideoUserDataAction::Unfavorite => Self::Unfavorite,
            VideoUserDataAction::MarkPlayed => Self::MarkPlayed,
            VideoUserDataAction::MarkUnplayed => Self::MarkUnplayed,
        }
    }
}

/// Media card summary shared by home rows, browse results, episode rows, and
/// recommendation shelves.
#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoLibraryItem {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub production_year: Option<i32>,
    /// Premiere timestamp supplied by the server, RFC 3339.
    pub premiere_date: Option<String>,
    pub community_rating: Option<f32>,
    pub episode_count: Option<u32>,
    /// Current user's last played timestamp, RFC 3339.
    pub last_played_date: Option<String>,
    pub runtime_seconds: Option<f64>,
    pub played: bool,
    pub favorite: bool,
    pub artwork_image_id: Option<String>,
    pub backdrop_image_id: Option<String>,
    pub logo_image_id: Option<String>,
    pub series_poster_image_id: Option<String>,
    pub episode_thumb_image_id: Option<String>,
    pub series_thumb_image_id: Option<String>,
    pub series_backdrop_image_id: Option<String>,
    pub season_poster_image_id: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub index_number_end: Option<u32>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub end_year: Option<i32>,
    pub series_continuing: bool,
    pub unplayed_item_count: Option<u32>,
    pub resume_position_seconds: Option<f64>,
    pub played_percentage: Option<f64>,
    pub overview: Option<String>,
}

impl From<ms::VideoLibraryItem> for VideoLibraryItem {
    fn from(item: ms::VideoLibraryItem) -> Self {
        Self {
            id: item.id,
            name: item.name,
            item_type: item.item_type,
            production_year: item.production_year,
            premiere_date: item.premiere_date,
            community_rating: item.community_rating,
            episode_count: item.episode_count,
            last_played_date: item.last_played_date,
            runtime_seconds: item.runtime_seconds,
            played: item.played,
            favorite: item.favorite,
            artwork_image_id: item.artwork_image_id,
            backdrop_image_id: item.backdrop_image_id,
            logo_image_id: item.logo_image_id,
            series_poster_image_id: item.series_poster_image_id,
            episode_thumb_image_id: item.episode_thumb_image_id,
            series_thumb_image_id: item.series_thumb_image_id,
            series_backdrop_image_id: item.series_backdrop_image_id,
            season_poster_image_id: item.season_poster_image_id,
            season_number: item.season_number,
            episode_number: item.episode_number,
            index_number_end: item.index_number_end,
            series_id: item.series_id,
            series_name: item.series_name,
            end_year: item.end_year,
            series_continuing: item.series_continuing,
            unplayed_item_count: item.unplayed_item_count,
            resume_position_seconds: item.resume_position_seconds,
            played_percentage: item.played_percentage,
            overview: item.overview,
        }
    }
}

impl From<VideoLibraryItem> for ms::VideoLibraryItem {
    fn from(item: VideoLibraryItem) -> Self {
        Self {
            id: item.id,
            name: item.name,
            item_type: item.item_type,
            production_year: item.production_year,
            premiere_date: item.premiere_date,
            community_rating: item.community_rating,
            episode_count: item.episode_count,
            last_played_date: item.last_played_date,
            runtime_seconds: item.runtime_seconds,
            played: item.played,
            favorite: item.favorite,
            artwork_image_id: item.artwork_image_id,
            backdrop_image_id: item.backdrop_image_id,
            logo_image_id: item.logo_image_id,
            series_poster_image_id: item.series_poster_image_id,
            episode_thumb_image_id: item.episode_thumb_image_id,
            series_thumb_image_id: item.series_thumb_image_id,
            series_backdrop_image_id: item.series_backdrop_image_id,
            season_poster_image_id: item.season_poster_image_id,
            season_number: item.season_number,
            episode_number: item.episode_number,
            index_number_end: item.index_number_end,
            series_id: item.series_id,
            series_name: item.series_name,
            end_year: item.end_year,
            series_continuing: item.series_continuing,
            unplayed_item_count: item.unplayed_item_count,
            resume_position_seconds: item.resume_position_seconds,
            played_percentage: item.played_percentage,
            overview: item.overview,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoHome {
    pub continue_watching: Vec<VideoLibraryItem>,
    pub next_up: Vec<VideoLibraryItem>,
}

impl From<ms::VideoHome> for VideoHome {
    fn from(home: ms::VideoHome) -> Self {
        Self {
            continue_watching: home
                .continue_watching
                .into_iter()
                .map(VideoLibraryItem::from)
                .collect(),
            next_up: home
                .next_up
                .into_iter()
                .map(VideoLibraryItem::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoLibraryShortcut {
    pub id: String,
    pub name: String,
    pub collection_type: String,
    pub item_count: Option<i32>,
    pub artwork_image_id: Option<String>,
}

impl From<ms::VideoLibraryShortcut> for VideoLibraryShortcut {
    fn from(shortcut: ms::VideoLibraryShortcut) -> Self {
        Self {
            id: shortcut.id,
            name: shortcut.name,
            collection_type: shortcut.collection_type,
            item_count: shortcut.item_count,
            artwork_image_id: shortcut.artwork_image_id,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
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

impl From<VideoLibraryPageRequest> for ms::VideoLibraryPageRequest {
    fn from(request: VideoLibraryPageRequest) -> Self {
        Self {
            library_id: request.library_id,
            collection_type: request.collection_type.into(),
            start_index: request.start_index,
            limit: request.limit,
            sort: request.sort.into(),
            sort_direction: request.sort_direction.into(),
            played_filter: request.played_filter.into(),
            favorites_only: request.favorites_only,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoLibraryPage {
    pub library_id: String,
    pub collection_type: VideoLibraryKind,
    pub start_index: i32,
    pub limit: i32,
    pub total_record_count: i32,
    pub has_more: bool,
    pub items: Vec<VideoLibraryItem>,
}

impl From<ms::VideoLibraryPage> for VideoLibraryPage {
    fn from(page: ms::VideoLibraryPage) -> Self {
        Self {
            library_id: page.library_id,
            collection_type: page.collection_type.into(),
            start_index: page.start_index,
            limit: page.limit,
            total_record_count: page.total_record_count,
            has_more: page.has_more,
            items: page.items.into_iter().map(VideoLibraryItem::from).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoSearchRequest {
    pub query: String,
    pub start_index: i32,
    pub limit: i32,
}

impl From<VideoSearchRequest> for ms::VideoSearchRequest {
    fn from(request: VideoSearchRequest) -> Self {
        Self {
            query: request.query,
            start_index: request.start_index,
            limit: request.limit,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoSearchPage {
    pub query: String,
    pub start_index: i32,
    pub limit: i32,
    pub total_record_count: i32,
    pub has_more: bool,
    pub items: Vec<VideoLibraryItem>,
}

impl From<ms::VideoSearchPage> for VideoSearchPage {
    fn from(page: ms::VideoSearchPage) -> Self {
        Self {
            query: page.query,
            start_index: page.start_index,
            limit: page.limit,
            total_record_count: page.total_record_count,
            has_more: page.has_more,
            items: page.items.into_iter().map(VideoLibraryItem::from).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FavoritesPage {
    pub start_index: i32,
    pub limit: i32,
    pub total_record_count: i32,
    pub has_more: bool,
    pub items: Vec<VideoLibraryItem>,
}

impl From<ms::FavoritesPage> for FavoritesPage {
    fn from(page: ms::FavoritesPage) -> Self {
        Self {
            start_index: page.start_index,
            limit: page.limit,
            total_record_count: page.total_record_count,
            has_more: page.has_more,
            items: page.items.into_iter().map(VideoLibraryItem::from).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WatchHistoryPage {
    pub start_index: i32,
    pub limit: i32,
    pub total_record_count: i32,
    pub has_more: bool,
    pub items: Vec<VideoLibraryItem>,
}

impl From<ms::WatchHistoryPage> for WatchHistoryPage {
    fn from(page: ms::WatchHistoryPage) -> Self {
        Self {
            start_index: page.start_index,
            limit: page.limit,
            total_record_count: page.total_record_count,
            has_more: page.has_more,
            items: page.items.into_iter().map(VideoLibraryItem::from).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoCastMember {
    pub name: String,
    pub role: Option<String>,
    pub image_id: Option<String>,
}

impl From<ms::VideoCastMember> for VideoCastMember {
    fn from(member: ms::VideoCastMember) -> Self {
        Self {
            name: member.name,
            role: member.role,
            image_id: member.image_id,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoDetailMetadata {
    pub community_rating: Option<f32>,
    pub official_rating: Option<String>,
    pub creators: Vec<String>,
    pub cast: Vec<VideoCastMember>,
}

impl From<ms::VideoDetailMetadata> for VideoDetailMetadata {
    fn from(metadata: ms::VideoDetailMetadata) -> Self {
        Self {
            community_rating: metadata.community_rating,
            official_rating: metadata.official_rating,
            creators: metadata.creators,
            cast: metadata
                .cast
                .into_iter()
                .map(VideoCastMember::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoStreamInfo {
    pub codec: Option<String>,
    pub language: Option<String>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub is_default: bool,
    pub display_title: Option<String>,
}

impl From<ms::VideoStreamInfo> for VideoStreamInfo {
    fn from(stream: ms::VideoStreamInfo) -> Self {
        Self {
            codec: stream.codec,
            language: stream.language,
            channels: stream.channels,
            channel_layout: stream.channel_layout,
            is_default: stream.is_default,
            display_title: stream.display_title,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoMediaInfo {
    pub container: Option<String>,
    pub size_bytes: Option<u64>,
    pub bitrate_bps: Option<u64>,
    pub video_codec: Option<String>,
    pub video_width: Option<u32>,
    pub video_height: Option<u32>,
    pub video_frame_rate: Option<f32>,
    /// e.g. "SDR", "HDR10", "DoVi" from the video stream range metadata.
    pub video_range: Option<String>,
    pub media_source_count: u32,
    pub streams_known: bool,
    pub audio_streams: Vec<VideoStreamInfo>,
    pub subtitle_streams: Vec<VideoStreamInfo>,
}

impl From<ms::VideoMediaInfo> for VideoMediaInfo {
    fn from(info: ms::VideoMediaInfo) -> Self {
        Self {
            container: info.container,
            size_bytes: info.size_bytes,
            bitrate_bps: info.bitrate_bps,
            video_codec: info.video_codec,
            video_width: info.video_width,
            video_height: info.video_height,
            video_frame_rate: info.video_frame_rate,
            video_range: info.video_range,
            media_source_count: info.media_source_count,
            streams_known: info.streams_known,
            audio_streams: info
                .audio_streams
                .into_iter()
                .map(VideoStreamInfo::from)
                .collect(),
            subtitle_streams: info
                .subtitle_streams
                .into_iter()
                .map(VideoStreamInfo::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoItemDetail {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub overview: Option<String>,
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
    pub logo_image_id: Option<String>,
    pub series_poster_image_id: Option<String>,
    pub media_info: Option<VideoMediaInfo>,
    pub metadata: VideoDetailMetadata,
}

impl From<ms::VideoItemDetail> for VideoItemDetail {
    fn from(detail: ms::VideoItemDetail) -> Self {
        Self {
            id: detail.id,
            name: detail.name,
            item_type: detail.item_type,
            overview: detail.overview,
            original_language: detail.original_language,
            production_year: detail.production_year,
            runtime_seconds: detail.runtime_seconds,
            series_id: detail.series_id,
            series_name: detail.series_name,
            season_number: detail.season_number,
            episode_number: detail.episode_number,
            genres: detail.genres,
            played: detail.played,
            favorite: detail.favorite,
            played_percentage: detail.played_percentage,
            resume_position_seconds: detail.resume_position_seconds,
            can_resume: detail.can_resume,
            can_play: detail.can_play,
            artwork_image_id: detail.artwork_image_id,
            backdrop_image_id: detail.backdrop_image_id,
            logo_image_id: detail.logo_image_id,
            series_poster_image_id: detail.series_poster_image_id,
            media_info: detail.media_info.map(VideoMediaInfo::from),
            metadata: detail.metadata.into(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoPlaybackStreamOption {
    pub index: i32,
    pub label: String,
    pub language: Option<String>,
    pub codec: Option<String>,
    pub is_default: bool,
    pub is_external: bool,
}

impl From<ms::VideoPlaybackStreamOption> for VideoPlaybackStreamOption {
    fn from(option: ms::VideoPlaybackStreamOption) -> Self {
        Self {
            index: option.index,
            label: option.label,
            language: option.language,
            codec: option.codec,
            is_default: option.is_default,
            is_external: option.is_external,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoItemStreams {
    pub audio_streams: Vec<VideoPlaybackStreamOption>,
    pub subtitle_streams: Vec<VideoPlaybackStreamOption>,
}

impl From<ms::VideoItemStreams> for VideoItemStreams {
    fn from(streams: ms::VideoItemStreams) -> Self {
        Self {
            audio_streams: streams
                .audio_streams
                .into_iter()
                .map(VideoPlaybackStreamOption::from)
                .collect(),
            subtitle_streams: streams
                .subtitle_streams
                .into_iter()
                .map(VideoPlaybackStreamOption::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoSeason {
    pub id: String,
    pub name: String,
    pub season_number: Option<i32>,
    pub played: bool,
    pub favorite: bool,
    pub artwork_image_id: Option<String>,
}

impl From<ms::VideoSeason> for VideoSeason {
    fn from(season: ms::VideoSeason) -> Self {
        Self {
            id: season.id,
            name: season.name,
            season_number: season.season_number,
            played: season.played,
            favorite: season.favorite,
            artwork_image_id: season.artwork_image_id,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoShowDetail {
    pub id: String,
    pub name: String,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub genres: Vec<String>,
    pub played: bool,
    pub original_language: Option<String>,
    pub favorite: bool,
    pub can_play: bool,
    pub artwork_image_id: Option<String>,
    pub backdrop_image_id: Option<String>,
    pub logo_image_id: Option<String>,
    pub next_episode: Option<VideoLibraryItem>,
    pub seasons: Vec<VideoSeason>,
    pub metadata: VideoDetailMetadata,
}

impl From<ms::VideoShowDetail> for VideoShowDetail {
    fn from(detail: ms::VideoShowDetail) -> Self {
        Self {
            id: detail.id,
            name: detail.name,
            overview: detail.overview,
            production_year: detail.production_year,
            genres: detail.genres,
            played: detail.played,
            original_language: detail.original_language,
            favorite: detail.favorite,
            can_play: detail.can_play,
            artwork_image_id: detail.artwork_image_id,
            backdrop_image_id: detail.backdrop_image_id,
            logo_image_id: detail.logo_image_id,
            next_episode: detail.next_episode.map(VideoLibraryItem::from),
            seasons: detail.seasons.into_iter().map(VideoSeason::from).collect(),
            metadata: detail.metadata.into(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoSeasonEpisodesPageRequest {
    pub series_id: String,
    pub season_id: Option<String>,
    pub season_number: Option<i32>,
    pub start_index: i32,
    pub limit: i32,
}

impl From<VideoSeasonEpisodesPageRequest> for ms::VideoSeasonEpisodesPageRequest {
    fn from(request: VideoSeasonEpisodesPageRequest) -> Self {
        Self {
            series_id: request.series_id,
            season_id: request.season_id,
            season_number: request.season_number,
            start_index: request.start_index,
            limit: request.limit,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
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

impl From<ms::VideoSeasonEpisodesPage> for VideoSeasonEpisodesPage {
    fn from(page: ms::VideoSeasonEpisodesPage) -> Self {
        Self {
            series_id: page.series_id,
            season_id: page.season_id,
            season_number: page.season_number,
            start_index: page.start_index,
            limit: page.limit,
            total_record_count: page.total_record_count,
            next_start_index: page.next_start_index,
            has_more: page.has_more,
            episodes: page
                .episodes
                .into_iter()
                .map(VideoLibraryItem::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoPlaybackTarget {
    pub item_id: String,
    pub start_position_ticks: Option<i64>,
}

impl From<ms::VideoPlaybackTarget> for VideoPlaybackTarget {
    fn from(target: ms::VideoPlaybackTarget) -> Self {
        Self {
            item_id: target.item_id,
            start_position_ticks: target.start_position_ticks,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VideoUserDataUpdate {
    pub item_id: String,
    pub played: bool,
    pub favorite: bool,
}

impl From<ms::VideoUserDataUpdate> for VideoUserDataUpdate {
    fn from(update: ms::VideoUserDataUpdate) -> Self {
        Self {
            item_id: update.item_id,
            played: update.played,
            favorite: update.favorite,
        }
    }
}

/// Device-local watchlist entry for the active profile.
#[derive(Clone, Debug, uniffi::Record)]
pub struct WatchlistEntry {
    pub item_id: String,
    pub added_at_unix_millis: u64,
    pub name: String,
    pub item_type: String,
    pub series_name: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
}

impl From<jellypilot_core::watchlist::WatchlistRecord> for WatchlistEntry {
    fn from(record: jellypilot_core::watchlist::WatchlistRecord) -> Self {
        Self {
            item_id: record.item_id().to_owned(),
            added_at_unix_millis: record.added_at_unix_millis(),
            name: record.name().to_owned(),
            item_type: record.item_type().to_owned(),
            series_name: record.series_name().map(str::to_owned),
            season_number: record.season_number(),
            episode_number: record.episode_number(),
        }
    }
}

/// Resolved image request for the platform image fetcher.
///
/// `authorization` is secret-bearing: send it with the request only, never
/// in UI state, logs, or persisted records.
#[derive(Clone, Debug, uniffi::Record)]
pub struct LibraryImageTarget {
    pub url: String,
    pub authorization: String,
    pub user_agent: String,
    pub accept: String,
}

impl From<jellypilot_sdk::LibraryImageTarget> for LibraryImageTarget {
    fn from(target: jellypilot_sdk::LibraryImageTarget) -> Self {
        Self {
            url: target.url,
            authorization: target.authorization,
            user_agent: target.user_agent,
            accept: target.accept,
        }
    }
}
