use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jellypilot_media_server::{normalize_server_url, MediaServerProvider, VideoLibraryItem};
use serde::{Deserialize, Serialize};

use crate::config::CONFIG_DIRECTORY;

const WATCHLIST_FILE: &str = "watchlist.json";
const STORAGE_VERSION: u32 = 1;

/// Stable identity for content owned by one user on one media server.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileScope {
    provider: MediaServerProvider,
    server_url: String,
    user_id: String,
}

impl ProfileScope {
    pub fn new(
        provider: MediaServerProvider,
        server_url: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Result<Self, ProfileScopeError> {
        let server_url = server_url.into();
        let server_url = normalize_server_url(server_url.trim()).to_owned();
        if server_url.is_empty() {
            return Err(ProfileScopeError::EmptyServerUrl);
        }

        let user_id = user_id.into();
        let user_id = user_id.trim().to_owned();
        if user_id.is_empty() {
            return Err(ProfileScopeError::EmptyUserId);
        }

        Ok(Self {
            provider,
            server_url,
            user_id,
        })
    }

    pub const fn provider(&self) -> MediaServerProvider {
        self.provider
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }
}

impl<'de> Deserialize<'de> for ProfileScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SerializedProfileScope {
            provider: MediaServerProvider,
            server_url: String,
            user_id: String,
        }

        let serialized = SerializedProfileScope::deserialize(deserializer)?;
        Self::new(
            serialized.provider,
            serialized.server_url,
            serialized.user_id,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileScopeError {
    EmptyServerUrl,
    EmptyUserId,
}

impl fmt::Display for ProfileScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyServerUrl => formatter.write_str("profile scope server URL is empty"),
            Self::EmptyUserId => formatter.write_str("profile scope user id is empty"),
        }
    }
}

impl std::error::Error for ProfileScopeError {}

/// Device-local Watchlist membership and fallback presentation data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchlistRecord {
    scope: ProfileScope,
    item_id: String,
    added_at_unix_millis: u64,
    name: String,
    item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    series_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    season_number: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    episode_number: Option<i32>,
}

impl WatchlistRecord {
    /// Captures enough server data to keep an unavailable item identifiable.
    pub fn from_item(
        scope: ProfileScope,
        item: &VideoLibraryItem,
        added_at_unix_millis: u64,
    ) -> Result<Self, WatchlistError> {
        Self {
            scope,
            item_id: item.id.clone(),
            added_at_unix_millis,
            name: item.name.clone(),
            item_type: item.item_type.clone(),
            series_name: item.series_name.clone(),
            season_number: item.season_number,
            episode_number: item.episode_number,
        }
        .normalized()
    }

    pub const fn scope(&self) -> &ProfileScope {
        &self.scope
    }

    pub fn item_id(&self) -> &str {
        &self.item_id
    }

    pub const fn added_at_unix_millis(&self) -> u64 {
        self.added_at_unix_millis
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn item_type(&self) -> &str {
        &self.item_type
    }

    pub fn series_name(&self) -> Option<&str> {
        self.series_name.as_deref()
    }

    pub const fn season_number(&self) -> Option<i32> {
        self.season_number
    }

    pub const fn episode_number(&self) -> Option<i32> {
        self.episode_number
    }

    fn normalized(mut self) -> Result<Self, WatchlistError> {
        self.item_id = required_text(self.item_id, "watchlist item id")?;
        self.name = required_text(self.name, "watchlist item name")?;
        self.item_type = match self.item_type.trim().to_ascii_lowercase().as_str() {
            "movie" => "Movie".to_owned(),
            "series" => "Series".to_owned(),
            "episode" => "Episode".to_owned(),
            _ => {
                return Err(WatchlistError::InvalidRecord(
                    "watchlist item type must be Movie, Series, or Episode".to_owned(),
                ));
            }
        };
        self.series_name = self
            .series_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        Ok(self)
    }

    fn has_identity(&self, scope: &ProfileScope, item_id: &str) -> bool {
        self.scope == *scope && self.item_id == item_id
    }
}

/// Synchronous, single-owner persistence boundary for the device Watchlist.
pub struct WatchlistStore {
    path: PathBuf,
    records: Vec<WatchlistRecord>,
}

impl WatchlistStore {
    pub fn load() -> Result<Self, WatchlistError> {
        Self::load_at(watchlist_path())
    }

    /// Creates an isolated store for cross-crate tests.
    #[cfg(feature = "test-utils")]
    #[doc(hidden)]
    pub fn for_test(path: PathBuf) -> Result<Self, WatchlistError> {
        Self::load_at(path)
    }

    /// Returns this scope's records in most-recently-added-first order.
    pub fn records_for(&self, scope: &ProfileScope) -> Vec<WatchlistRecord> {
        self.records
            .iter()
            .filter(|record| record.scope() == scope)
            .cloned()
            .collect()
    }

    pub fn contains(&self, scope: &ProfileScope, item_id: &str) -> bool {
        let item_id = item_id.trim();
        self.records
            .iter()
            .any(|record| record.has_identity(scope, item_id))
    }

    /// Adds one membership without changing the timestamp of an existing record.
    pub fn add(&mut self, record: WatchlistRecord) -> Result<bool, WatchlistError> {
        let record = record.normalized()?;
        if self.contains(record.scope(), record.item_id()) {
            return Ok(false);
        }

        let mut candidate = self.records.clone();
        candidate.push(record);
        sort_records(&mut candidate);
        save_to(&self.path, &candidate)?;
        self.records = candidate;
        Ok(true)
    }

    pub fn remove(&mut self, scope: &ProfileScope, item_id: &str) -> Result<bool, WatchlistError> {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return Err(WatchlistError::InvalidRecord(
                "watchlist item id is empty".to_owned(),
            ));
        }

        let mut candidate = self.records.clone();
        candidate.retain(|record| !record.has_identity(scope, item_id));
        if candidate.len() == self.records.len() {
            return Ok(false);
        }

        save_to(&self.path, &candidate)?;
        self.records = candidate;
        Ok(true)
    }

    /// Removes only the selected account's local records.
    pub fn remove_scope(&mut self, scope: &ProfileScope) -> Result<usize, WatchlistError> {
        let mut candidate = self.records.clone();
        candidate.retain(|record| record.scope() != scope);
        let removed = self.records.len().saturating_sub(candidate.len());
        if removed == 0 {
            return Ok(0);
        }

        save_to(&self.path, &candidate)?;
        self.records = candidate;
        Ok(removed)
    }

    fn load_at(path: PathBuf) -> Result<Self, WatchlistError> {
        let records = load_from(&path)?;
        Ok(Self { path, records })
    }
}

#[derive(Debug)]
pub enum WatchlistError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedVersion(u32),
    InvalidRecord(String),
}

impl fmt::Display for WatchlistError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "watchlist I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "watchlist data is invalid: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "watchlist version {version} is not supported")
            }
            Self::InvalidRecord(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for WatchlistError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::UnsupportedVersion(_) | Self::InvalidRecord(_) => None,
        }
    }
}

impl From<io::Error> for WatchlistError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for WatchlistError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredWatchlist {
    version: u32,
    records: Vec<WatchlistRecord>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredWatchlistRef<'a> {
    version: u32,
    records: &'a [WatchlistRecord],
}

fn watchlist_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(CONFIG_DIRECTORY)
        .join(WATCHLIST_FILE)
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn load_from(path: &Path) -> Result<Vec<WatchlistRecord>, WatchlistError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let stored: StoredWatchlist = serde_json::from_str(&contents)?;
    if stored.version != STORAGE_VERSION {
        return Err(WatchlistError::UnsupportedVersion(stored.version));
    }

    let mut records = stored
        .records
        .into_iter()
        .map(WatchlistRecord::normalized)
        .collect::<Result<Vec<_>, _>>()?;
    sort_records(&mut records);
    deduplicate_records(&mut records);
    Ok(records)
}

fn save_to(path: &Path, records: &[WatchlistRecord]) -> Result<(), WatchlistError> {
    let contents = serde_json::to_string_pretty(&StoredWatchlistRef {
        version: STORAGE_VERSION,
        records,
    })?;
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

fn sort_records(records: &mut [WatchlistRecord]) {
    records.sort_by(|left, right| {
        right
            .added_at_unix_millis()
            .cmp(&left.added_at_unix_millis())
    });
}

fn deduplicate_records(records: &mut Vec<WatchlistRecord>) {
    let mut deduplicated = Vec::with_capacity(records.len());
    for record in records.drain(..) {
        if !deduplicated.iter().any(|existing: &WatchlistRecord| {
            existing.has_identity(record.scope(), record.item_id())
        }) {
            deduplicated.push(record);
        }
    }
    *records = deduplicated;
}

fn required_text(value: String, name: &str) -> Result<String, WatchlistError> {
    let value = value.trim();
    if value.is_empty() {
        Err(WatchlistError::InvalidRecord(format!("{name} is empty")))
    } else {
        Ok(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jellypilot-watchlist-{}-{name}.json",
            std::process::id()
        ))
    }

    fn scope(provider: MediaServerProvider, server_url: &str, user_id: &str) -> ProfileScope {
        ProfileScope::new(provider, server_url, user_id).expect("profile scope should be valid")
    }

    fn item(id: &str, name: &str, item_type: &str) -> VideoLibraryItem {
        VideoLibraryItem {
            id: id.to_owned(),
            name: name.to_owned(),
            item_type: item_type.to_owned(),
            production_year: None,
            runtime_seconds: None,
            played: false,
            favorite: false,
            artwork_image_id: None,
            backdrop_image_id: None,
            logo_image_id: None,
            series_poster_image_id: None,
            episode_thumb_image_id: None,
            series_thumb_image_id: None,
            series_backdrop_image_id: None,
            season_poster_image_id: None,
            season_number: None,
            episode_number: None,
            index_number_end: None,
            series_id: None,
            series_name: None,
            end_year: None,
            series_continuing: false,
            unplayed_item_count: None,
            resume_position_seconds: None,
            played_percentage: None,
            overview: None,
        }
    }

    fn record(scope: ProfileScope, id: &str, added_at: u64) -> WatchlistRecord {
        WatchlistRecord::from_item(scope, &item(id, id, "Movie"), added_at)
            .expect("watchlist record should be valid")
    }

    fn clean(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(temporary_path(path));
        let _ = fs::remove_dir_all(temporary_path(path));
    }

    #[test]
    fn profile_scope_normalizes_identity_without_merging_distinct_accounts() {
        let normalized = scope(
            MediaServerProvider::Jellyfin,
            " https://example.test/base/// ",
            " user-1 ",
        );
        let other_provider = scope(
            MediaServerProvider::Emby,
            "https://example.test/base",
            "user-1",
        );
        let other_user = scope(
            MediaServerProvider::Jellyfin,
            "https://example.test/base",
            "user-2",
        );

        assert_eq!(normalized.server_url(), "https://example.test/base");
        assert_eq!(normalized.user_id(), "user-1");
        assert_ne!(normalized, other_provider);
        assert_ne!(normalized, other_user);
    }

    #[test]
    fn missing_file_loads_an_empty_store() {
        let path = test_path("missing");
        clean(&path);
        let store = WatchlistStore::load_at(path.clone()).expect("missing watchlist should load");

        assert!(store
            .records_for(&scope(
                MediaServerProvider::Jellyfin,
                "https://one.test",
                "user"
            ))
            .is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn add_is_idempotent_and_records_are_newest_first_after_reload() {
        let path = test_path("add-order");
        clean(&path);
        let profile = scope(MediaServerProvider::Jellyfin, "https://one.test", "user");
        let mut store = WatchlistStore::load_at(path.clone()).expect("store should load");

        assert!(store
            .add(record(profile.clone(), "older", 10))
            .expect("older record should save"));
        assert!(store
            .add(record(profile.clone(), "newer", 20))
            .expect("newer record should save"));
        assert!(!store
            .add(record(profile.clone(), "older", 30))
            .expect("duplicate add should be a no-op"));

        let reloaded = WatchlistStore::load_at(path.clone()).expect("saved store should reload");
        assert_eq!(
            reloaded
                .records_for(&profile)
                .iter()
                .map(WatchlistRecord::item_id)
                .collect::<Vec<_>>(),
            vec!["newer", "older"]
        );
        assert_eq!(reloaded.records_for(&profile)[1].added_at_unix_millis(), 10);
        clean(&path);
    }

    #[test]
    fn matching_item_ids_remain_isolated_by_profile_scope() {
        let path = test_path("scope-isolation");
        clean(&path);
        let first = scope(MediaServerProvider::Jellyfin, "https://one.test", "user");
        let second = scope(MediaServerProvider::Jellyfin, "https://two.test", "user");
        let mut store = WatchlistStore::load_at(path.clone()).expect("store should load");

        store
            .add(record(first.clone(), "same-id", 10))
            .expect("first account should save");
        store
            .add(record(second.clone(), "same-id", 20))
            .expect("second account should save");
        store
            .remove(&first, "same-id")
            .expect("first account should remove");

        assert!(!store.contains(&first, "same-id"));
        assert!(store.contains(&second, "same-id"));
        clean(&path);
    }

    #[test]
    fn remove_scope_preserves_every_other_profile() {
        let path = test_path("scope-cleanup");
        clean(&path);
        let selected = scope(MediaServerProvider::Emby, "https://one.test", "user-1");
        let retained = scope(MediaServerProvider::Emby, "https://one.test", "user-2");
        let mut store = WatchlistStore::load_at(path.clone()).expect("store should load");
        store
            .add(record(selected.clone(), "one", 10))
            .expect("selected record should save");
        store
            .add(record(selected.clone(), "two", 20))
            .expect("selected record should save");
        store
            .add(record(retained.clone(), "three", 30))
            .expect("retained record should save");

        assert_eq!(
            store
                .remove_scope(&selected)
                .expect("selected scope should clear"),
            2
        );
        assert!(store.records_for(&selected).is_empty());
        assert_eq!(store.records_for(&retained).len(), 1);
        clean(&path);
    }

    #[test]
    fn episode_fallback_metadata_survives_serialization() {
        let path = test_path("episode-fallback");
        clean(&path);
        let profile = scope(MediaServerProvider::Jellyfin, "https://one.test", "user");
        let mut episode = item("episode", "Pilot", "Episode");
        episode.series_name = Some("Example Show".to_owned());
        episode.season_number = Some(1);
        episode.episode_number = Some(2);
        let mut store = WatchlistStore::load_at(path.clone()).expect("store should load");
        store
            .add(
                WatchlistRecord::from_item(profile.clone(), &episode, 10)
                    .expect("episode should map"),
            )
            .expect("episode should save");

        let reloaded = WatchlistStore::load_at(path.clone()).expect("store should reload");
        let saved = &reloaded.records_for(&profile)[0];
        assert_eq!(saved.name(), "Pilot");
        assert_eq!(saved.item_type(), "Episode");
        assert_eq!(saved.series_name(), Some("Example Show"));
        assert_eq!(saved.season_number(), Some(1));
        assert_eq!(saved.episode_number(), Some(2));
        clean(&path);
    }

    #[test]
    fn unsupported_version_is_rejected_without_rewriting_the_file() {
        let path = test_path("unsupported-version");
        clean(&path);
        let contents = r#"{"version":2,"records":[]}"#;
        fs::write(&path, contents).expect("fixture should write");

        let error = WatchlistStore::load_at(path.clone())
            .err()
            .expect("unknown version should fail");

        assert!(matches!(error, WatchlistError::UnsupportedVersion(2)));
        assert_eq!(
            fs::read_to_string(&path).expect("fixture should remain"),
            contents
        );
        clean(&path);
    }

    #[test]
    fn failed_atomic_write_preserves_memory_and_existing_file() {
        let path = test_path("atomic-failure");
        clean(&path);
        let profile = scope(MediaServerProvider::Jellyfin, "https://one.test", "user");
        let mut store = WatchlistStore::load_at(path.clone()).expect("store should load");
        store
            .add(record(profile.clone(), "original", 10))
            .expect("original record should save");
        let original_file = fs::read_to_string(&path).expect("original file should exist");
        fs::create_dir(temporary_path(&path)).expect("blocking temp directory should exist");

        assert!(store.add(record(profile.clone(), "candidate", 20)).is_err());
        assert!(!store.contains(&profile, "candidate"));
        assert_eq!(
            fs::read_to_string(&path).expect("original file should remain"),
            original_file
        );
        clean(&path);
    }
}
