//! Device-local History visibility. A hidden item never changes server user data.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::config::{save_json_to, ConfigError};
use crate::watchlist::ProfileScope;

#[derive(Clone, Serialize, Deserialize)]
struct HiddenItem {
    scope: ProfileScope,
    item_id: String,
    #[serde(default)]
    generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_played_date: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredVisibility {
    version: u32,
    #[serde(default)]
    generation: u64,
    items: Vec<HiddenItem>,
}

/// One storage transaction owner for local, profile-scoped History visibility.
pub struct HistoryVisibilityStore {
    path: PathBuf,
    generation: u64,
    items: Vec<HiddenItem>,
}

impl HistoryVisibilityStore {
    pub fn load_in_dir(directory: PathBuf) -> Result<Self, ConfigError> {
        let path = directory.join("history-visibility.json");
        let (generation, items) = match fs::read(&path) {
            Ok(bytes) => {
                let stored: StoredVisibility = serde_json::from_slice(&bytes)?;
                if stored.version != 1
                    || stored
                        .items
                        .iter()
                        .any(|item| item.item_id.trim().is_empty())
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "history visibility data is invalid",
                    )
                    .into());
                }
                let generation = stored
                    .items
                    .iter()
                    .map(|item| item.generation)
                    .max()
                    .unwrap_or(0)
                    .max(stored.generation);
                (generation, stored.items)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (0, Vec::new()),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            generation,
            items,
        })
    }

    pub fn hidden_item_ids(&self, scope: &ProfileScope) -> HashSet<String> {
        self.items
            .iter()
            .filter(|item| &item.scope == scope)
            .map(|item| item.item_id.clone())
            .collect()
    }

    /// Commits item-only visibility membership, without observing playback data.
    /// This preserves the Android policy: reading or playing never implies Undo.
    pub fn set_hidden(
        &mut self,
        scope: &ProfileScope,
        item_id: &str,
        hidden: bool,
    ) -> Result<bool, ConfigError> {
        if hidden {
            return self
                .hide_with_observation(scope, item_id, None)
                .map(|key| key.is_some());
        }
        self.restore(scope, item_id, None)
    }

    /// Hides the observed desktop History state and returns its removal identity.
    /// A repeated hide does not replace the first removal or its observation.
    /// Missing or invalid timestamps remain hidden until a usable baseline and
    /// a later timestamp, explicit playback, or Undo restores visibility.
    pub fn hide_with_observation(
        &mut self,
        scope: &ProfileScope,
        item_id: &str,
        last_played_date: Option<&str>,
    ) -> Result<Option<u64>, ConfigError> {
        let item_id = required_item_id(item_id)?;
        if self
            .items
            .iter()
            .any(|item| &item.scope == scope && item.item_id == item_id)
        {
            return Ok(None);
        }
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "history removal identity overflow",
            )
        })?;
        let mut items = self.items.clone();
        items.push(HiddenItem {
            scope: scope.clone(),
            item_id: item_id.to_owned(),
            generation,
            last_played_date: last_played_date
                .filter(|date| timestamp(date).is_some())
                .map(str::to_owned),
        });
        self.save(items, generation)?;
        Ok(Some(generation))
    }

    /// Restores only the specified removal. A later hide of the same item wins.
    pub fn restore_removal(
        &mut self,
        scope: &ProfileScope,
        item_id: &str,
        generation: u64,
    ) -> Result<bool, ConfigError> {
        self.restore(scope, item_id, Some(generation))
    }

    /// Applies desktop server observations in one persistent transaction. Only a
    /// strictly newer valid server timestamp releases a hide. The first usable
    /// timestamp on a legacy/baseline-free entry establishes a baseline instead.
    pub fn observe_last_played<'a>(
        &mut self,
        scope: &ProfileScope,
        observations: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    ) -> Result<bool, ConfigError> {
        let mut items = self.items.clone();
        let mut changed = false;
        for (item_id, date) in observations {
            let Some(observed) = date.and_then(timestamp) else {
                continue;
            };
            let Some(index) = items
                .iter()
                .position(|item| &item.scope == scope && item.item_id == item_id)
            else {
                continue;
            };
            let hidden = &mut items[index];
            match hidden.last_played_date.as_deref().and_then(timestamp) {
                Some(baseline) if observed > baseline => {
                    items.remove(index);
                    changed = true;
                }
                None => {
                    hidden.last_played_date = date.map(str::to_owned);
                    changed = true;
                }
                Some(_) => {}
            }
        }
        if changed {
            self.save(items, self.generation)?;
        }
        Ok(changed)
    }

    fn restore(
        &mut self,
        scope: &ProfileScope,
        item_id: &str,
        generation: Option<u64>,
    ) -> Result<bool, ConfigError> {
        let item_id = required_item_id(item_id)?;
        let mut items = self.items.clone();
        items.retain(|item| {
            !(&item.scope == scope
                && item.item_id == item_id
                && generation.is_none_or(|generation| item.generation == generation))
        });
        if items.len() == self.items.len() {
            return Ok(false);
        }
        self.save(items, self.generation)?;
        Ok(true)
    }

    fn save(&mut self, items: Vec<HiddenItem>, generation: u64) -> Result<(), ConfigError> {
        save_json_to(
            &self.path,
            &StoredVisibility {
                version: 1,
                generation,
                items: items.clone(),
            },
        )?;
        self.items = items;
        self.generation = generation;
        Ok(())
    }
}

fn required_item_id(item_id: &str) -> Result<&str, ConfigError> {
    let item_id = item_id.trim();
    if item_id.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "history item id is empty").into());
    }
    Ok(item_id)
}

fn timestamp(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value).ok()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use jellypilot_media_server::MediaServerProvider;

    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            Self(std::env::temp_dir().join(format!(
                "jellypilot-history-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }

        fn store(&self) -> HistoryVisibilityStore {
            HistoryVisibilityStore::load_in_dir(self.0.clone()).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scope(user: &str) -> ProfileScope {
        ProfileScope::new(MediaServerProvider::Jellyfin, "https://media.example", user).unwrap()
    }

    #[test]
    fn only_a_strictly_newer_server_instant_restores_persisted_visibility() {
        let fixture = Fixture::new();
        let scope = scope("ada");
        fixture
            .store()
            .hide_with_observation(&scope, "film", Some("2026-09-21T12:00:00.1234567Z"))
            .unwrap();
        let mut store = fixture.store();
        for date in [
            None,
            Some("invalid"),
            Some("2026-09-21"),
            Some("2026-09-21T12:00:00Z"),
            Some("2026-09-21T20:00:00.1234567+08:00"),
        ] {
            assert!(!store.observe_last_played(&scope, [("film", date)]).unwrap());
            assert!(store.hidden_item_ids(&scope).contains("film"));
        }
        assert!(store
            .observe_last_played(&scope, [("film", Some("2026-09-21T12:00:00.1234568Z"))])
            .unwrap());
        assert!(fixture.store().hidden_item_ids(&scope).is_empty());
    }

    #[test]
    fn legacy_hides_establish_a_persistent_baseline_without_changing_other_profiles() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.0).unwrap();
        let ada = scope("ada");
        let grace = scope("grace");
        fs::write(
            fixture.0.join("history-visibility.json"),
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "items": [{"scope": ada, "item_id": "film"}, {"scope": grace, "item_id": "film"}]
            }))
            .unwrap(),
        )
        .unwrap();
        let mut store = fixture.store();
        assert!(!store
            .observe_last_played(&ada, [("film", None), ("film", Some("invalid"))])
            .unwrap());
        assert!(store
            .observe_last_played(&ada, [("film", Some("2026-09-21T12:00:00Z"))])
            .unwrap());
        let mut reopened = fixture.store();
        assert!(reopened.hidden_item_ids(&ada).contains("film"));
        assert!(!reopened
            .observe_last_played(&ada, [("film", Some("2026-09-21T12:00:00Z"))])
            .unwrap());
        assert!(reopened
            .observe_last_played(&ada, [("film", Some("2026-09-21T12:01:00Z"))])
            .unwrap());
        assert!(reopened.hidden_item_ids(&ada).is_empty());
        assert!(reopened.hidden_item_ids(&grace).contains("film"));
    }

    #[test]
    fn old_undo_cannot_restore_a_newer_hide_even_after_reopening_an_empty_store() {
        let fixture = Fixture::new();
        let scope = scope("ada");
        let mut store = fixture.store();
        let old = store
            .hide_with_observation(&scope, "film", None)
            .unwrap()
            .unwrap();
        assert!(store.set_hidden(&scope, "film", false).unwrap());
        let mut reopened = fixture.store();
        let current = reopened
            .hide_with_observation(&scope, "film", None)
            .unwrap()
            .unwrap();
        assert!(!reopened.restore_removal(&scope, "film", old).unwrap());
        assert!(reopened.hidden_item_ids(&scope).contains("film"));
        assert!(reopened.restore_removal(&scope, "film", current).unwrap());
    }

    #[test]
    fn failed_observation_persistence_keeps_the_original_hidden_state_retryable() {
        let fixture = Fixture::new();
        let scope = scope("ada");
        let mut store = fixture.store();
        store
            .hide_with_observation(&scope, "film", Some("2026-09-21T12:00:00Z"))
            .unwrap();
        let temporary = fixture.0.join("history-visibility.json.tmp");
        fs::create_dir(&temporary).unwrap();
        assert!(store
            .observe_last_played(&scope, [("film", Some("2026-09-22T12:00:00Z"))])
            .is_err());
        assert!(store.hidden_item_ids(&scope).contains("film"));
        fs::remove_dir(&temporary).unwrap();
        assert!(store
            .observe_last_played(&scope, [("film", Some("2026-09-22T12:00:00Z"))])
            .unwrap());
        assert!(fixture.store().hidden_item_ids(&scope).is_empty());
    }
}
