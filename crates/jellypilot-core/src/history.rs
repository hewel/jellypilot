//! Device-local History visibility. A hidden item never changes server user data.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::{save_json_to, ConfigError};
use crate::watchlist::ProfileScope;

#[derive(Clone, Serialize, Deserialize)]
struct HiddenItem {
    scope: ProfileScope,
    item_id: String,
}

#[derive(Serialize, Deserialize)]
struct StoredVisibility {
    version: u32,
    items: Vec<HiddenItem>,
}

/// One storage transaction owner for local, profile-scoped History visibility.
pub struct HistoryVisibilityStore {
    path: PathBuf,
    items: Vec<HiddenItem>,
}

impl HistoryVisibilityStore {
    pub fn load_in_dir(directory: PathBuf) -> Result<Self, ConfigError> {
        let path = directory.join("history-visibility.json");
        let items = match fs::read(&path) {
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
                stored.items
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, items })
    }

    pub fn hidden_item_ids(&self, scope: &ProfileScope) -> HashSet<String> {
        self.items
            .iter()
            .filter(|item| &item.scope == scope)
            .map(|item| item.item_id.clone())
            .collect()
    }

    /// Commits only visibility membership. Timestamps, Played and resume data
    /// are not stored here and cannot be mutated by this operation.
    pub fn set_hidden(
        &mut self,
        scope: &ProfileScope,
        item_id: &str,
        hidden: bool,
    ) -> Result<bool, ConfigError> {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return Err(
                io::Error::new(io::ErrorKind::InvalidInput, "history item id is empty").into(),
            );
        }
        let matches = |item: &HiddenItem| &item.scope == scope && item.item_id == item_id;
        if self.items.iter().any(matches) == hidden {
            return Ok(false);
        }
        let mut items = self.items.clone();
        items.retain(|item| !matches(item));
        if hidden {
            items.push(HiddenItem {
                scope: scope.clone(),
                item_id: item_id.to_owned(),
            });
        }
        save_json_to(
            &self.path,
            &StoredVisibility {
                version: 1,
                items: items.clone(),
            },
        )?;
        self.items = items;
        Ok(true)
    }
}
