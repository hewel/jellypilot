//! Device-local per-series and per-item audio track memory, independent of login
//! credentials and settings.
//!
//! A memory records the audio track a user explicitly picked for a series (episode
//! playback) or an individual item (movies), so later plays of the same series or
//! item restore that choice ahead of any global rule.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{save_json_to, ConfigError, CONFIG_DIRECTORY};
use crate::watchlist::ProfileScope;

const STORAGE_VERSION: u32 = 1;
const STORAGE_FILE: &str = "audio-tracks.json";

/// The series an episode belongs to, or the item itself for movies and other videos.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrackKey {
    series_or_item_id: String,
}

impl AudioTrackKey {
    pub fn new(series_or_item_id: &str) -> Option<Self> {
        if series_or_item_id.trim().is_empty() {
            return None;
        }
        Some(Self {
            series_or_item_id: series_or_item_id.to_owned(),
        })
    }

    fn is_valid(&self) -> bool {
        !self.series_or_item_id.trim().is_empty()
    }
}

/// A remembered audio choice: the track language, plus the server display title to
/// disambiguate tracks that share a language (e.g. "Japanese" vs "Japanese Commentary").
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrackPreference {
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl AudioTrackPreference {
    pub fn new(language: &str, title: Option<&str>) -> Option<Self> {
        if language.trim().is_empty() {
            return None;
        }
        Some(Self {
            language: language.to_owned(),
            title: title
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_owned),
        })
    }

    fn is_valid(&self) -> bool {
        !self.language.trim().is_empty()
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct AudioTrackRecord {
    scope: ProfileScope,
    key: AudioTrackKey,
    #[serde(default)]
    preference: Option<AudioTrackPreference>,
    #[serde(default)]
    subtitle: Option<SubtitleTrackPreference>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StoredAudioTracks {
    version: u32,
    records: Vec<AudioTrackRecord>,
}

/// A remembered subtitle choice for the same series or item: which track, or that
/// subtitles were switched off deliberately.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrackPreference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub enabled: bool,
}

impl SubtitleTrackPreference {
    /// An explicit track choice; the language is required so the track can be
    /// matched again on a different encode.
    pub fn enabled(language: &str, title: Option<&str>) -> Option<Self> {
        if language.trim().is_empty() {
            return None;
        }
        Some(Self {
            language: Some(language.to_owned()),
            title: title
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_owned),
            enabled: true,
        })
    }

    pub fn disabled() -> Self {
        Self {
            language: None,
            title: None,
            enabled: false,
        }
    }

    fn is_valid(&self) -> bool {
        !self.enabled
            || self
                .language
                .as_deref()
                .is_some_and(|language| !language.trim().is_empty())
    }
}
/// Synchronous, single-writer storage. Reads are snapshots; writes reload other scopes
/// before atomically replacing the file. Simultaneous cross-process writers are unsupported.
#[derive(Debug)]
pub struct AudioTrackStore {
    path: PathBuf,
    scope: ProfileScope,
    stored: StoredAudioTracks,
}

impl AudioTrackStore {
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

    pub fn get(&self, key: &AudioTrackKey) -> Option<AudioTrackPreference> {
        self.stored
            .records
            .iter()
            .find(|record| record.scope == self.scope && record.key == *key)
            .and_then(|record| record.preference.clone())
    }
    /// Remembers a valid audio preference. Invalid inputs return an InvalidInput I/O
    /// error without writing or changing memory. A failed read or write also leaves
    /// the live snapshot unchanged.
    pub fn remember(
        &mut self,
        key: &AudioTrackKey,
        preference: AudioTrackPreference,
    ) -> Result<(), ConfigError> {
        if !key.is_valid() || !preference.is_valid() {
            return Err(invalid_input(
                "audio track memory requires a valid key and nonempty language",
            ));
        }
        let mut candidate = read_from(&self.path)?;
        if let Some(record) = candidate
            .records
            .iter_mut()
            .find(|record| record.scope == self.scope && record.key == *key)
        {
            if record.preference.as_ref() == Some(&preference) {
                self.stored = candidate;
                return Ok(());
            }
            record.preference = Some(preference);
        } else {
            candidate.records.push(AudioTrackRecord {
                scope: self.scope.clone(),
                key: key.clone(),
                preference: Some(preference),
                subtitle: None,
            });
        }
        save_json_to(&self.path, &candidate)?;
        self.stored = candidate;
        Ok(())
    }

    pub fn get_subtitle(&self, key: &AudioTrackKey) -> Option<SubtitleTrackPreference> {
        self.stored
            .records
            .iter()
            .find(|record| record.scope == self.scope && record.key == *key)
            .and_then(|record| record.subtitle.clone())
    }

    /// Remembers a subtitle choice (or deliberate disable) without touching the
    /// remembered audio track for the same key.
    pub fn remember_subtitle(
        &mut self,
        key: &AudioTrackKey,
        preference: SubtitleTrackPreference,
    ) -> Result<(), ConfigError> {
        if !key.is_valid() || !preference.is_valid() {
            return Err(invalid_input(
                "subtitle memory requires a valid key and, when enabled, a language",
            ));
        }
        let mut candidate = read_from(&self.path)?;
        if let Some(record) = candidate
            .records
            .iter_mut()
            .find(|record| record.scope == self.scope && record.key == *key)
        {
            if record.subtitle.as_ref() == Some(&preference) {
                self.stored = candidate;
                return Ok(());
            }
            record.subtitle = Some(preference);
        } else {
            candidate.records.push(AudioTrackRecord {
                scope: self.scope.clone(),
                key: key.clone(),
                preference: None,
                subtitle: Some(preference),
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

fn read_from(path: &Path) -> Result<StoredAudioTracks, ConfigError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(StoredAudioTracks {
                version: STORAGE_VERSION,
                records: Vec::new(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let stored: StoredAudioTracks = serde_json::from_str(&contents)?;
    if stored.version != STORAGE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported audio track storage version",
        )
        .into());
    }
    let mut identities = HashSet::with_capacity(stored.records.len());
    for record in &stored.records {
        if !record.key.is_valid()
            || record.preference.as_ref().is_some_and(|p| !p.is_valid())
            || record.subtitle.as_ref().is_some_and(|s| !s.is_valid())
            || (record.preference.is_none() && record.subtitle.is_none())
            || !identities.insert((
                std::mem::discriminant(&record.scope.provider()),
                record.scope.server_url(),
                record.scope.user_id(),
                &record.key,
            ))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid or duplicate audio track record",
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
            "jellypilot-audio-tracks-{}-{name}.json",
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
    fn reopen_preserves_language_and_title() {
        let path = path("exact");
        let _ = fs::remove_file(&path);
        let mut store = AudioTrackStore::load_from(path.clone(), scope()).unwrap();
        let series_key = AudioTrackKey::new("series-1").unwrap();
        let movie_key = AudioTrackKey::new("movie-1").unwrap();
        store
            .remember(
                &series_key,
                AudioTrackPreference::new("jpn", Some("Japanese - AAC 5.1")).unwrap(),
            )
            .unwrap();
        store
            .remember(&movie_key, AudioTrackPreference::new("eng", None).unwrap())
            .unwrap();

        let reopened = AudioTrackStore::load_from(path.clone(), scope()).unwrap();
        assert_eq!(
            reopened.get(&series_key),
            Some(AudioTrackPreference {
                language: "jpn".to_owned(),
                title: Some("Japanese - AAC 5.1".to_owned()),
            })
        );
        assert_eq!(
            reopened.get(&movie_key),
            Some(AudioTrackPreference {
                language: "eng".to_owned(),
                title: None,
            })
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn subtitle_preferences_coexist_with_audio_and_roundtrip() {
        let path = path("subtitle");
        let _ = fs::remove_file(&path);
        let mut store = AudioTrackStore::load_from(path.clone(), scope()).unwrap();
        let key = AudioTrackKey::new("series-1").unwrap();
        let other = AudioTrackKey::new("movie-1").unwrap();

        store
            .remember_subtitle(
                &key,
                SubtitleTrackPreference::enabled("zho", Some("Chinese - SRT")).unwrap(),
            )
            .unwrap();
        // Updating the audio choice for the same key keeps the subtitle choice.
        store
            .remember(&key, AudioTrackPreference::new("kor", None).unwrap())
            .unwrap();
        store
            .remember_subtitle(&other, SubtitleTrackPreference::disabled())
            .unwrap();

        let reopened = AudioTrackStore::load_from(path.clone(), scope()).unwrap();
        assert_eq!(
            reopened.get_subtitle(&key),
            Some(SubtitleTrackPreference {
                language: Some("zho".to_owned()),
                title: Some("Chinese - SRT".to_owned()),
                enabled: true,
            })
        );
        assert_eq!(
            reopened.get(&key).map(|p| p.language),
            Some("kor".to_owned())
        );
        assert_eq!(
            reopened.get_subtitle(&other),
            Some(SubtitleTrackPreference::disabled())
        );
        assert_eq!(reopened.get(&other), None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn preference_normalizes_blank_title_to_none() {
        assert_eq!(
            AudioTrackPreference::new("jpn", Some("  ")),
            Some(AudioTrackPreference {
                language: "jpn".to_owned(),
                title: None,
            })
        );
        assert!(AudioTrackPreference::new(" ", None).is_none());
        assert!(AudioTrackKey::new(" ").is_none());
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
        let key = AudioTrackKey::new("series-1").unwrap();
        let mut stores: Vec<_> = scopes
            .iter()
            .map(|scope| AudioTrackStore::load_from(path.clone(), scope.clone()).unwrap())
            .collect();
        for (index, store) in stores.iter_mut().enumerate() {
            store
                .remember(
                    &key,
                    AudioTrackPreference::new(&format!("lang{index}"), None).unwrap(),
                )
                .unwrap();
        }
        for (index, scope) in scopes.into_iter().enumerate() {
            let reopened = AudioTrackStore::load_from(path.clone(), scope).unwrap();
            assert_eq!(
                reopened.get(&key).map(|preference| preference.language),
                Some(format!("lang{index}"))
            );
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_values_and_failed_writes_preserve_committed_memory() {
        let path = path("failures");
        let _ = fs::remove_file(&path);
        let key = AudioTrackKey::new("series-1").unwrap();
        let mut store = AudioTrackStore::load_from(path.clone(), scope()).unwrap();
        let committed = AudioTrackPreference::new("jpn", None).unwrap();
        store.remember(&key, committed.clone()).unwrap();
        let bytes = fs::read(&path).unwrap();

        let invalid = AudioTrackPreference {
            language: " ".to_owned(),
            title: None,
        };
        assert!(
            matches!(store.remember(&key, invalid), Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
        );
        assert_eq!(store.get(&key), Some(committed.clone()));
        assert_eq!(fs::read(&path).unwrap(), bytes);

        let temporary = path.with_extension("json.tmp");
        fs::create_dir(&temporary).unwrap();
        store.remember(&key, committed.clone()).unwrap(); // Unchanged values do not attempt a write.
        assert!(store
            .remember(&key, AudioTrackPreference::new("eng", None).unwrap())
            .is_err());
        assert_eq!(store.get(&key), Some(committed));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_dir(temporary).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_or_unreadable_files_are_not_empty_memory() {
        let path = path("corrupt");
        let _ = fs::remove_file(&path);
        let key = AudioTrackKey::new("series-1").unwrap();
        let mut store = AudioTrackStore::load_from(path.clone(), scope()).unwrap();
        store
            .remember(&key, AudioTrackPreference::new("jpn", None).unwrap())
            .unwrap();
        fs::write(&path, "not json").unwrap();
        assert!(matches!(
            AudioTrackStore::load_from(path.clone(), scope()),
            Err(ConfigError::Json(_))
        ));
        assert!(store
            .remember(&key, AudioTrackPreference::new("eng", None).unwrap())
            .is_err());
        assert_eq!(
            store.get(&key).map(|preference| preference.language),
            Some("jpn".to_owned())
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(matches!(
            AudioTrackStore::load_from(path.clone(), scope()),
            Err(ConfigError::Io(_))
        ));
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn duplicate_or_invalid_records_are_rejected() {
        let path = path("duplicates");
        let _ = fs::remove_file(&path);
        let profile_scope = scope();
        let record = serde_json::json!({
            "scope": profile_scope,
            "key": { "seriesOrItemId": "series-1" },
            "preference": { "language": "jpn" },
        });
        fs::write(
            &path,
            serde_json::json!({
                "version": 1,
                "records": [record.clone(), record],
            })
            .to_string(),
        )
        .unwrap();
        assert!(matches!(
            AudioTrackStore::load_from(path.clone(), scope()),
            Err(ConfigError::Io(error)) if error.kind() == io::ErrorKind::InvalidData
        ));
        fs::remove_file(path).unwrap();
    }
}
