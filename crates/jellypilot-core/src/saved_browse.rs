//! Device-local named browse queries, isolated by the complete profile identity.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jellypilot_media_server::{MediaServerProvider, VideoLibraryKind};
use serde::{Deserialize, Serialize};

use crate::browse_model::BrowsePreferences;
use crate::config::{save_json_to, ConfigError};
use crate::watchlist::ProfileScope;

const STORAGE_FILE: &str = "saved-browse.json";
const STORAGE_VERSION: u32 = 1;
const MAX_NAME_CHARACTERS: usize = 80;
const MAX_RECORDS_PER_SCOPE: usize = 50;

/// Stable identity that is never reassigned after a saved filter is deleted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SavedBrowseId(pub u64);

/// A saved query, including its exact library and all applied conditions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedBrowseFilter {
    pub id: SavedBrowseId,
    pub name: String,
    #[serde(deserialize_with = "deserialize_scope")]
    pub scope: ProfileScope,
    pub library_id: String,
    pub collection_type: VideoLibraryKind,
    /// Presentation fallback only; this name never establishes library identity.
    pub library_name: String,
    pub preferences: BrowsePreferences,
}

/// A new query whose ownership is supplied separately by the active SDK scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedBrowseDraft {
    pub name: String,
    pub library_id: String,
    pub collection_type: VideoLibraryKind,
    pub library_name: String,
    pub preferences: BrowsePreferences,
}

#[derive(Debug)]
pub enum SavedBrowseStoreError {
    InvalidName,
    DuplicateName,
    LimitReached,
    Missing,
    InvalidLibrary,
    Storage(ConfigError),
    InvalidData(String),
    UnsupportedVersion(u32),
}

impl fmt::Display for SavedBrowseStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => formatter.write_str(
                "saved filter name must contain 1 to 80 characters without control characters",
            ),
            Self::DuplicateName => {
                formatter.write_str("a saved filter with this name already exists")
            }
            Self::LimitReached => formatter.write_str("this profile already has 50 saved filters"),
            Self::Missing => formatter.write_str("saved filter is unavailable in this profile"),
            Self::InvalidLibrary => {
                formatter.write_str("saved filter requires an exact nonempty library ID")
            }
            Self::Storage(error) => error.fmt(formatter),
            Self::InvalidData(message) => {
                write!(formatter, "saved filter data is invalid: {message}")
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported saved filter storage version: {version}"
            ),
        }
    }
}

impl std::error::Error for SavedBrowseStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ConfigError> for SavedBrowseStoreError {
    fn from(error: ConfigError) -> Self {
        Self::Storage(error)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredFilters {
    version: u32,
    next_id: u64,
    records: Vec<SavedBrowseFilter>,
}

impl Default for StoredFilters {
    fn default() -> Self {
        Self {
            version: STORAGE_VERSION,
            next_id: 1,
            records: Vec::new(),
        }
    }
}

/// Synchronous snapshot store. Mutations reread the file before committing and
/// retain the live snapshot if validation or persistence fails. The caller must
/// serialize writers to the same storage directory.
#[derive(Debug)]
pub struct SavedBrowseStore {
    path: PathBuf,
    stored: StoredFilters,
}

impl SavedBrowseStore {
    /// Loads `saved-browse.json`. Only a missing file represents an empty store;
    /// corrupt or unsupported files return an error without being rewritten.
    pub fn load_in_dir(storage_dir: PathBuf) -> Result<Self, SavedBrowseStoreError> {
        let path = storage_dir.join(STORAGE_FILE);
        let stored = read_from(&path)?;
        Ok(Self { path, stored })
    }

    /// Returns this scope's records in creation order.
    pub fn records_for(&self, scope: &ProfileScope) -> Vec<SavedBrowseFilter> {
        self.stored
            .records
            .iter()
            .filter(|record| record.scope == *scope)
            .cloned()
            .collect()
    }

    /// A valid ID from another profile is still absent from this scope.
    pub fn get(&self, scope: &ProfileScope, id: SavedBrowseId) -> Option<&SavedBrowseFilter> {
        self.stored
            .records
            .iter()
            .find(|record| record.scope == *scope && record.id == id)
    }

    /// Adds a distinct name without changing or normalizing any query conditions.
    pub fn save(
        &mut self,
        scope: &ProfileScope,
        draft: SavedBrowseDraft,
    ) -> Result<SavedBrowseFilter, SavedBrowseStoreError> {
        let name = validated_name(&draft.name)?;
        validate_library(&draft.library_id)?;
        let mut candidate = read_from(&self.path)?;
        ensure_unique_name(&candidate.records, scope, &name, None)?;
        if candidate
            .records
            .iter()
            .filter(|record| record.scope == *scope)
            .count()
            >= MAX_RECORDS_PER_SCOPE
        {
            return Err(SavedBrowseStoreError::LimitReached);
        }
        let id = SavedBrowseId(candidate.next_id);
        candidate.next_id = candidate.next_id.checked_add(1).ok_or_else(|| {
            SavedBrowseStoreError::InvalidData("saved filter ID sequence exhausted".to_owned())
        })?;
        let record = SavedBrowseFilter {
            id,
            name,
            scope: scope.clone(),
            library_id: draft.library_id,
            collection_type: draft.collection_type,
            library_name: draft.library_name,
            preferences: draft.preferences,
        };
        candidate.records.push(record.clone());
        self.commit(candidate)?;
        Ok(record)
    }

    /// Changes only the name, preserving the stable identity and query.
    pub fn rename(
        &mut self,
        scope: &ProfileScope,
        id: SavedBrowseId,
        name: &str,
    ) -> Result<SavedBrowseFilter, SavedBrowseStoreError> {
        let name = validated_name(name)?;
        let mut candidate = read_from(&self.path)?;
        let index = candidate
            .records
            .iter()
            .position(|record| record.scope == *scope && record.id == id)
            .ok_or(SavedBrowseStoreError::Missing)?;
        ensure_unique_name(&candidate.records, scope, &name, Some(id))?;
        candidate.records[index].name = name;
        let record = candidate.records[index].clone();
        self.commit(candidate)?;
        Ok(record)
    }

    /// Removes only a local definition. Returns false if it is absent in this scope.
    pub fn remove(
        &mut self,
        scope: &ProfileScope,
        id: SavedBrowseId,
    ) -> Result<bool, SavedBrowseStoreError> {
        let mut candidate = read_from(&self.path)?;
        let Some(index) = candidate
            .records
            .iter()
            .position(|record| record.scope == *scope && record.id == id)
        else {
            return Ok(false);
        };
        candidate.records.remove(index);
        self.commit(candidate)?;
        Ok(true)
    }

    fn commit(&mut self, candidate: StoredFilters) -> Result<(), SavedBrowseStoreError> {
        save_json_to(&self.path, &candidate)?;
        self.stored = candidate;
        Ok(())
    }
}

fn validated_name(name: &str) -> Result<String, SavedBrowseStoreError> {
    if name.chars().any(char::is_control) {
        return Err(SavedBrowseStoreError::InvalidName);
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARACTERS {
        return Err(SavedBrowseStoreError::InvalidName);
    }
    Ok(name.to_owned())
}

fn validate_library(library_id: &str) -> Result<(), SavedBrowseStoreError> {
    if library_id.is_empty()
        || library_id.trim() != library_id
        || library_id.chars().any(char::is_control)
    {
        return Err(SavedBrowseStoreError::InvalidLibrary);
    }
    Ok(())
}

fn ensure_unique_name(
    records: &[SavedBrowseFilter],
    scope: &ProfileScope,
    name: &str,
    except: Option<SavedBrowseId>,
) -> Result<(), SavedBrowseStoreError> {
    let key = name.to_lowercase();
    if records.iter().any(|record| {
        record.scope == *scope && Some(record.id) != except && record.name.to_lowercase() == key
    }) {
        return Err(SavedBrowseStoreError::DuplicateName);
    }
    Ok(())
}

fn deserialize_scope<'de, D>(deserializer: D) -> Result<ProfileScope, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct StoredScope {
        provider: MediaServerProvider,
        server_url: String,
        user_id: String,
    }
    let stored = StoredScope::deserialize(deserializer)?;
    let scope = ProfileScope::new(stored.provider, &stored.server_url, &stored.user_id)
        .map_err(serde::de::Error::custom)?;
    if scope.server_url() != stored.server_url || scope.user_id() != stored.user_id {
        return Err(serde::de::Error::custom(
            "saved filter scope must be canonical",
        ));
    }
    Ok(scope)
}

fn read_from(path: &Path) -> Result<StoredFilters, SavedBrowseStoreError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(StoredFilters::default())
        }
        Err(error) => return Err(ConfigError::from(error).into()),
    };
    #[derive(Deserialize)]
    struct Version {
        version: u32,
    }
    let version: Version = serde_json::from_str(&contents).map_err(ConfigError::from)?;
    if version.version != STORAGE_VERSION {
        return Err(SavedBrowseStoreError::UnsupportedVersion(version.version));
    }
    let stored: StoredFilters = serde_json::from_str(&contents).map_err(ConfigError::from)?;
    if stored.next_id == 0 {
        return Err(SavedBrowseStoreError::InvalidData(
            "next ID must be nonzero".to_owned(),
        ));
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut scope_counts = HashMap::new();
    for record in &stored.records {
        if record.id.0 == 0 || record.id.0 >= stored.next_id || !ids.insert(record.id) {
            return Err(SavedBrowseStoreError::InvalidData(
                "record IDs must be unique and precede the next ID".to_owned(),
            ));
        }
        if !matches!(validated_name(&record.name), Ok(name) if name == record.name) {
            return Err(SavedBrowseStoreError::InvalidData(
                "record name is invalid or untrimmed".to_owned(),
            ));
        }
        validate_library(&record.library_id).map_err(|_| {
            SavedBrowseStoreError::InvalidData("record library ID is invalid".to_owned())
        })?;
        let scope_key = (
            std::mem::discriminant(&record.scope.provider()),
            record.scope.server_url(),
            record.scope.user_id(),
        );
        if !names.insert((scope_key, record.name.to_lowercase())) {
            return Err(SavedBrowseStoreError::InvalidData(
                "duplicate record name in profile scope".to_owned(),
            ));
        }
        let count = scope_counts.entry(scope_key).or_insert(0);
        *count += 1;
        if *count > MAX_RECORDS_PER_SCOPE {
            return Err(SavedBrowseStoreError::InvalidData(
                "too many saved filters in profile scope".to_owned(),
            ));
        }
    }
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    use jellypilot_media_server::{
        VideoLibraryFilters, VideoLibraryPlayedFilter, VideoLibraryQuality, VideoLibrarySort,
        VideoLibrarySortDirection,
    };
    use serde_json::json;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "jellypilot-saved-browse-{}-{timestamp}-{nonce}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn store(&self) -> SavedBrowseStore {
            SavedBrowseStore::load_in_dir(self.0.clone()).unwrap()
        }

        fn file(&self) -> PathBuf {
            self.0.join(STORAGE_FILE)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scope() -> ProfileScope {
        ProfileScope::new(
            MediaServerProvider::Jellyfin,
            "https://media.example",
            "alice",
        )
        .unwrap()
    }

    fn draft(name: &str) -> SavedBrowseDraft {
        SavedBrowseDraft {
            name: name.to_owned(),
            library_id: "same-library".to_owned(),
            collection_type: VideoLibraryKind::Movies,
            library_name: "Library snapshot".to_owned(),
            preferences: BrowsePreferences::default(),
        }
    }

    #[test]
    fn restart_preserves_complete_queries_and_unavailable_facet_values() {
        let directory = TestDirectory::new();
        let mut store = directory.store();
        let scope = scope();
        let variants = [
            (
                VideoLibrarySort::Title,
                VideoLibrarySortDirection::Ascending,
                VideoLibraryPlayedFilter::All,
                None,
            ),
            (
                VideoLibrarySort::RecentlyAdded,
                VideoLibrarySortDirection::Descending,
                VideoLibraryPlayedFilter::Played,
                Some(VideoLibraryQuality::Hd),
            ),
            (
                VideoLibrarySort::ReleaseDate,
                VideoLibrarySortDirection::Ascending,
                VideoLibraryPlayedFilter::Unplayed,
                Some(VideoLibraryQuality::FullHd),
            ),
            (
                VideoLibrarySort::Title,
                VideoLibrarySortDirection::Descending,
                VideoLibraryPlayedFilter::All,
                Some(VideoLibraryQuality::Uhd),
            ),
            (
                VideoLibrarySort::RecentlyAdded,
                VideoLibrarySortDirection::Ascending,
                VideoLibraryPlayedFilter::Played,
                Some(VideoLibraryQuality::DolbyVision),
            ),
        ];
        let mut records = Vec::new();
        for (index, (sort, sort_direction, played_filter, quality)) in
            variants.into_iter().enumerate()
        {
            let mut draft = draft(&format!("Filter {index}"));
            draft.collection_type = if index % 2 == 0 {
                VideoLibraryKind::Movies
            } else {
                VideoLibraryKind::TvShows
            };
            draft.preferences = BrowsePreferences {
                sort,
                sort_direction,
                played_filter,
                favorites_only: index % 2 == 0,
                filters: VideoLibraryFilters {
                    quality,
                    country: Some(" Formerland ".to_owned()),
                    genre: Some("Retired: 类型".to_owned()),
                },
            };
            records.push(store.save(&scope, draft).unwrap());
        }
        drop(store);
        assert_eq!(directory.store().records_for(&scope), records);
    }

    #[test]
    fn complete_scope_isolation_survives_writes_from_stale_snapshots() {
        let directory = TestDirectory::new();
        let scopes = [
            scope(),
            ProfileScope::new(MediaServerProvider::Emby, "https://media.example", "alice").unwrap(),
            ProfileScope::new(
                MediaServerProvider::Jellyfin,
                "https://other.example",
                "alice",
            )
            .unwrap(),
            ProfileScope::new(
                MediaServerProvider::Jellyfin,
                "https://media.example",
                "bob",
            )
            .unwrap(),
        ];
        let mut stores: Vec<_> = scopes.iter().map(|_| directory.store()).collect();
        let mut records = Vec::new();
        for (store, scope) in stores.iter_mut().zip(&scopes) {
            records.push(store.save(scope, draft("Same name")).unwrap());
        }
        let mut reopened = directory.store();
        for (scope, record) in scopes.iter().zip(&records) {
            assert_eq!(reopened.records_for(scope), vec![record.clone()]);
            for other in scopes.iter().filter(|other| *other != scope) {
                assert!(reopened.get(other, record.id).is_none());
                assert!(matches!(
                    reopened.rename(other, record.id, "Wrong profile"),
                    Err(SavedBrowseStoreError::Missing)
                ));
                assert!(!reopened.remove(other, record.id).unwrap());
            }
        }
        assert_eq!(
            records
                .iter()
                .map(|record| record.id)
                .collect::<HashSet<_>>()
                .len(),
            scopes.len()
        );
    }

    #[test]
    fn names_are_trimmed_unicode_bounded_and_unique_within_their_scope() {
        let directory = TestDirectory::new();
        let mut store = directory.store();
        let scope = scope();
        let record = store.save(&scope, draft("  ÖLD Movies  ")).unwrap();
        assert_eq!(record.name, "ÖLD Movies");
        assert!(matches!(
            store.save(&scope, draft("öld movies")),
            Err(SavedBrowseStoreError::DuplicateName)
        ));
        for name in [
            "".to_owned(),
            "  ".to_owned(),
            "漢".repeat(81),
            "name\n".to_owned(),
            "\tname".to_owned(),
            "na\u{7f}me".to_owned(),
        ] {
            assert!(matches!(
                store.save(&scope, draft(&name)),
                Err(SavedBrowseStoreError::InvalidName)
            ));
            assert!(matches!(
                store.rename(&scope, record.id, &name),
                Err(SavedBrowseStoreError::InvalidName)
            ));
        }
        let long = store.save(&scope, draft(&"漢".repeat(80))).unwrap();
        assert!(matches!(
            store.rename(&scope, long.id, "öld movies"),
            Err(SavedBrowseStoreError::DuplicateName)
        ));
        let renamed = store.rename(&scope, record.id, "  öLd Movies  ").unwrap();
        assert_eq!(renamed.id, record.id);
        assert_eq!(renamed.preferences, record.preferences);
        assert_eq!(directory.store().get(&scope, record.id), Some(&renamed));
    }

    #[test]
    fn capacity_is_per_scope_and_deletion_never_reuses_ids_after_restart() {
        let directory = TestDirectory::new();
        let mut store = directory.store();
        let scope = scope();
        let mut records = Vec::new();
        for index in 0..MAX_RECORDS_PER_SCOPE {
            records.push(
                store
                    .save(&scope, draft(&format!("Filter {index}")))
                    .unwrap(),
            );
        }
        assert!(matches!(
            store.save(&scope, draft("Overflow")),
            Err(SavedBrowseStoreError::LimitReached)
        ));
        let other_scope = ProfileScope::new(
            MediaServerProvider::Jellyfin,
            "https://media.example",
            "bob",
        )
        .unwrap();
        let other_record = store.save(&other_scope, draft("Filter 0")).unwrap();
        for record in &records {
            assert!(store.remove(&scope, record.id).unwrap());
        }
        assert!(store.remove(&other_scope, other_record.id).unwrap());
        drop(store);
        let mut reopened = directory.store();
        let replacement = reopened.save(&scope, draft("Filter 0")).unwrap();
        assert!(replacement.id.0 > other_record.id.0);
        assert_eq!(reopened.records_for(&scope), vec![replacement]);
        assert!(reopened.records_for(&other_scope).is_empty());
    }

    #[test]
    fn failed_save_rename_and_remove_preserve_disk_memory_and_id_sequence() {
        let directory = TestDirectory::new();
        let mut store = directory.store();
        let scope = scope();
        let record = store.save(&scope, draft("Original")).unwrap();
        let committed = fs::read(directory.file()).unwrap();
        let temporary = directory.file().with_extension("json.tmp");
        // A directory at the staging path fails regardless of test process privileges.
        fs::create_dir(&temporary).unwrap();
        assert!(matches!(
            store.save(&scope, draft("New")),
            Err(SavedBrowseStoreError::Storage(_))
        ));
        assert!(matches!(
            store.rename(&scope, record.id, "Renamed"),
            Err(SavedBrowseStoreError::Storage(_))
        ));
        assert!(matches!(
            store.remove(&scope, record.id),
            Err(SavedBrowseStoreError::Storage(_))
        ));
        assert_eq!(store.records_for(&scope), vec![record.clone()]);
        assert_eq!(fs::read(directory.file()).unwrap(), committed);
        assert_eq!(directory.store().records_for(&scope), vec![record.clone()]);
        fs::remove_dir(temporary).unwrap();
        let next = store.save(&scope, draft("New")).unwrap();
        assert_eq!(next.id.0, record.id.0 + 1);
    }

    #[test]
    fn invalid_library_ids_are_rejected_without_replacing_the_exact_id() {
        let directory = TestDirectory::new();
        let mut store = directory.store();
        for library_id in ["", " ", " library", "library ", "library\n"] {
            let mut invalid = draft("Query");
            invalid.library_id = library_id.to_owned();
            assert!(matches!(
                store.save(&scope(), invalid),
                Err(SavedBrowseStoreError::InvalidLibrary)
            ));
        }
        assert!(!directory.file().exists());
    }

    #[test]
    fn corrupt_storage_is_rejected_and_cannot_be_overwritten_by_a_live_store() {
        let directory = TestDirectory::new();
        let mut store = directory.store();
        let scope = scope();
        let record = store.save(&scope, draft("Original")).unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(&fs::read(directory.file()).unwrap()).unwrap();
        let changes = [
            ("/nextId", json!(0)),
            ("/nextId", json!(record.id.0)),
            ("/records/0/id", json!(0)),
            ("/records/0/name", json!(" untrimmed")),
            ("/records/0/name", json!("")),
            ("/records/0/libraryId", json!("")),
            ("/records/0/collectionType", json!("unsupported")),
            (
                "/records/0/scope/serverUrl",
                json!("https://media.example/"),
            ),
            ("/records/0/scope/userId", json!(" alice")),
            ("/records/0/scope/userId", json!("")),
            ("/records/0/preferences/sort", json!("unknown-sort")),
            ("/records/0/preferences/filters", json!(null)),
        ];
        let mut invalid_files: Vec<_> = changes
            .into_iter()
            .map(|(pointer, value)| {
                let mut modified = original.clone();
                *modified.pointer_mut(pointer).unwrap() = value;
                modified.to_string()
            })
            .collect();
        let mut missing_preferences = original.clone();
        missing_preferences["records"][0]
            .as_object_mut()
            .unwrap()
            .remove("preferences");
        invalid_files.push(missing_preferences.to_string());
        let mut duplicate_id = original.clone();
        duplicate_id["records"]
            .as_array_mut()
            .unwrap()
            .push(original["records"][0].clone());
        invalid_files.push(duplicate_id.to_string());
        let mut duplicate_name = duplicate_id;
        duplicate_name["nextId"] = json!(3);
        duplicate_name["records"][1]["id"] = json!(2);
        duplicate_name["records"][1]["name"] = json!("ORIGINAL");
        invalid_files.push(duplicate_name.to_string());
        invalid_files.push("not JSON".to_owned());
        for invalid in invalid_files {
            fs::write(directory.file(), &invalid).unwrap();
            assert!(
                SavedBrowseStore::load_in_dir(directory.0.clone()).is_err(),
                "accepted invalid data: {invalid}"
            );
            assert!(store.save(&scope, draft("Do not overwrite")).is_err());
            assert_eq!(store.records_for(&scope), vec![record.clone()]);
            assert_eq!(fs::read_to_string(directory.file()).unwrap(), invalid);
        }
    }

    #[test]
    fn unsupported_versions_and_exhausted_ids_remain_read_only() {
        let directory = TestDirectory::new();
        let unsupported = r#"{"version":2,"futureShape":true}"#;
        fs::write(directory.file(), unsupported).unwrap();
        assert!(matches!(
            SavedBrowseStore::load_in_dir(directory.0.clone()),
            Err(SavedBrowseStoreError::UnsupportedVersion(2))
        ));
        assert_eq!(fs::read_to_string(directory.file()).unwrap(), unsupported);
        let exhausted = json!({"version":1,"nextId":u64::MAX,"records":[]}).to_string();
        fs::write(directory.file(), &exhausted).unwrap();
        let mut store = directory.store();
        assert!(matches!(
            store.save(&scope(), draft("Exhausted")),
            Err(SavedBrowseStoreError::InvalidData(_))
        ));
        assert!(store.records_for(&scope()).is_empty());
        assert_eq!(fs::read_to_string(directory.file()).unwrap(), exhausted);
    }
}
