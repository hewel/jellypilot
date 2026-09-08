//! Device-local per-season MPV volume memory, independent of login credentials and settings.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{save_json_to, ConfigError, CONFIG_DIRECTORY};
use crate::watchlist::ProfileScope;

const STORAGE_VERSION: u32 = 1;
const STORAGE_FILE: &str = "season-volumes.json";

/// A series and one of its seasons; season zero represents specials.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonVolumeKey {
    series_id: String,
    season_number: i32,
}

impl SeasonVolumeKey {
    pub fn new(series_id: &str, season_number: i32) -> Option<Self> {
        if series_id.trim().is_empty() || season_number < 0 {
            return None;
        }
        Some(Self {
            series_id: series_id.to_owned(),
            season_number,
        })
    }

    fn is_valid(&self) -> bool {
        !self.series_id.trim().is_empty() && self.season_number >= 0
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct VolumeRecord {
    scope: ProfileScope,
    key: SeasonVolumeKey,
    volume: f64,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredVolumes {
    version: u32,
    records: Vec<VolumeRecord>,
}

/// Synchronous, single-writer storage. Reads are snapshots; writes reload other scopes
/// before atomically replacing the file. Simultaneous cross-process writers are unsupported.
#[derive(Debug)]
pub struct SeasonVolumeStore {
    path: PathBuf,
    scope: ProfileScope,
    stored: StoredVolumes,
}

impl SeasonVolumeStore {
    pub fn load(scope: ProfileScope) -> Result<Self, ConfigError> {
        let path = dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(CONFIG_DIRECTORY)
            .join(STORAGE_FILE);
        Self::load_from(path, scope)
    }

    /// Loads a specific local file. Only a missing file means empty memory; malformed,
    /// unsupported, or unreadable files return errors and are never silently replaced.
    pub fn load_from(path: PathBuf, scope: ProfileScope) -> Result<Self, ConfigError> {
        let stored = read_from(&path)?;
        Ok(Self {
            path,
            scope,
            stored,
        })
    }

    pub fn get(&self, key: &SeasonVolumeKey) -> Option<f64> {
        self.stored
            .records
            .iter()
            .find(|record| record.scope == self.scope && record.key == *key)
            .map(|record| record.volume)
    }

    /// Remembers finite nonnegative MPV volume, including zero and values above 100.
    /// Invalid inputs return an InvalidInput I/O error without writing or changing memory.
    /// A failed read or write also leaves the live snapshot unchanged.
    pub fn remember(&mut self, key: &SeasonVolumeKey, volume: f64) -> Result<(), ConfigError> {
        if !key.is_valid() || !volume.is_finite() || volume < 0.0 {
            return Err(invalid_input(
                "season volume requires a valid season and finite nonnegative volume",
            ));
        }
        let mut candidate = read_from(&self.path)?;
        if let Some(record) = candidate
            .records
            .iter_mut()
            .find(|record| record.scope == self.scope && record.key == *key)
        {
            if record.volume == volume {
                self.stored = candidate;
                return Ok(());
            }
            record.volume = volume;
        } else {
            candidate.records.push(VolumeRecord {
                scope: self.scope.clone(),
                key: key.clone(),
                volume,
            });
        }
        save_json_to(&self.path, &candidate)?;
        self.stored = candidate;
        Ok(())
    }
}

fn invalid_input(message: &str) -> ConfigError {
    io::Error::new(io::ErrorKind::InvalidInput, message).into()
}

fn read_from(path: &Path) -> Result<StoredVolumes, ConfigError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(StoredVolumes {
                version: STORAGE_VERSION,
                records: Vec::new(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let stored: StoredVolumes = serde_json::from_str(&contents)?;
    if stored.version != STORAGE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported season volume storage version",
        )
        .into());
    }
    let mut identities = HashSet::with_capacity(stored.records.len());
    for record in &stored.records {
        if !record.key.is_valid()
            || !record.volume.is_finite()
            || record.volume < 0.0
            || !identities.insert((
                std::mem::discriminant(&record.scope.provider()),
                record.scope.server_url(),
                record.scope.user_id(),
                &record.key,
            ))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid or duplicate season volume record",
            )
            .into());
        }
    }
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellypilot_media_server::MediaServerProvider;

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jellypilot-season-volume-{}-{name}.json",
            std::process::id()
        ))
    }

    fn scope() -> ProfileScope {
        ProfileScope::new(
            MediaServerProvider::Jellyfin,
            "https://media.example",
            "alice",
        )
        .unwrap()
    }

    #[test]
    fn reopen_preserves_fractional_volume_and_specials() {
        let path = path("exact");
        let _ = fs::remove_file(&path);
        let mut store = SeasonVolumeStore::load_from(path.clone(), scope()).unwrap();
        let volumes = [0.0, 135.125, 42.5];
        for (season, volume) in (0..).zip(volumes) {
            store
                .remember(&SeasonVolumeKey::new("series", season).unwrap(), volume)
                .unwrap();
        }
        let reopened = SeasonVolumeStore::load_from(path.clone(), scope()).unwrap();
        for (season, volume) in (0..).zip(volumes) {
            assert_eq!(
                reopened.get(&SeasonVolumeKey::new("series", season).unwrap()),
                Some(volume)
            );
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn stale_scoped_stores_preserve_independent_identities() {
        let path = path("scopes");
        let _ = fs::remove_file(&path);
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
        let keys = [
            SeasonVolumeKey::new("series", 1).unwrap(),
            SeasonVolumeKey::new("series", 2).unwrap(),
            SeasonVolumeKey::new("other", 1).unwrap(),
        ];
        let mut stores: Vec<_> = scopes
            .iter()
            .map(|scope| SeasonVolumeStore::load_from(path.clone(), scope.clone()).unwrap())
            .collect();
        for (index, store) in stores.iter_mut().enumerate() {
            for (season, key) in keys.iter().enumerate() {
                store.remember(key, (index * 10 + season) as f64).unwrap();
            }
        }
        for (index, scope) in scopes.into_iter().enumerate() {
            let reopened = SeasonVolumeStore::load_from(path.clone(), scope).unwrap();
            for (season, key) in keys.iter().enumerate() {
                assert_eq!(reopened.get(key), Some((index * 10 + season) as f64));
            }
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_values_and_failed_writes_preserve_committed_memory() {
        let path = path("failures");
        let _ = fs::remove_file(&path);
        let key = SeasonVolumeKey::new("series", 0).unwrap();
        let mut store = SeasonVolumeStore::load_from(path.clone(), scope()).unwrap();
        store.remember(&key, 42.5).unwrap();
        let bytes = fs::read(&path).unwrap();
        for volume in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                matches!(store.remember(&key, volume), Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
            );
            assert_eq!(store.get(&key), Some(42.5));
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let temporary = path.with_extension("json.tmp");
        fs::create_dir(&temporary).unwrap();
        store.remember(&key, 42.5).unwrap(); // Unchanged values do not attempt a write.
        assert!(store.remember(&key, 60.0).is_err());
        assert_eq!(store.get(&key), Some(42.5));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_or_unreadable_files_are_not_empty_memory() {
        let path = path("corrupt");
        let _ = fs::remove_file(&path);
        let key = SeasonVolumeKey::new("series", 1).unwrap();
        let mut store = SeasonVolumeStore::load_from(path.clone(), scope()).unwrap();
        store.remember(&key, 25.0).unwrap();
        fs::write(&path, "not json").unwrap();
        assert!(matches!(
            SeasonVolumeStore::load_from(path.clone(), scope()),
            Err(ConfigError::Json(_))
        ));
        assert!(store.remember(&key, 30.0).is_err());
        assert_eq!(store.get(&key), Some(25.0));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(matches!(
            SeasonVolumeStore::load_from(path.clone(), scope()),
            Err(ConfigError::Io(_))
        ));
        fs::remove_dir(path).unwrap();
    }
}
