use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_plugin_store::StoreExt;

use crate::jellyfin::{MediaServerProvider, SavedSession};

const AUTH_PROFILES_STORE_FILE: &str = "auth.json";
const AUTH_PROFILES_STORE_KEY: &str = "saved_service_profiles";

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedServiceProfiles {
  pub active_profile_key: Option<String>,
  pub profiles: Vec<SavedServiceProfileSummary>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedServiceProfileSummary {
  pub key: String,
  pub provider: MediaServerProvider,
  pub server_url: String,
  pub server_name: Option<String>,
  pub user_name: String,
  pub active: bool,
  pub last_restore_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedServiceProfileStore {
  pub active_profile_key: Option<String>,
  pub profiles: Vec<StoredSavedServiceProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredSavedServiceProfile {
  pub session: SavedSession,
  pub last_restore_error: Option<String>,
}

pub(crate) fn profile_key(session: &SavedSession) -> String {
  format!(
    "{}|{}|{}",
    provider_key(session.provider),
    session.server_url.trim_end_matches('/'),
    session.user_name
  )
}

fn provider_key(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "jellyfin",
    MediaServerProvider::Emby => "emby",
  }
}

impl SavedServiceProfileStore {
  pub(crate) fn summary(&self) -> SavedServiceProfiles {
    SavedServiceProfiles {
      active_profile_key: self.active_profile_key.clone(),
      profiles: self
        .profiles
        .iter()
        .map(|profile| profile.summary(self.active_profile_key.as_deref()))
        .collect(),
    }
  }

  pub(crate) fn upsert_active(&mut self, session: SavedSession) -> String {
    let key = profile_key(&session);
    if let Some(profile) = self
      .profiles
      .iter_mut()
      .find(|profile| profile_key(&profile.session) == key)
    {
      profile.session = session;
      profile.last_restore_error = None;
    } else {
      self.profiles.push(StoredSavedServiceProfile {
        session,
        last_restore_error: None,
      });
    }
    self.active_profile_key = Some(key.clone());
    key
  }

  pub(crate) fn session_for_key(&self, key: &str) -> Option<SavedSession> {
    self
      .profiles
      .iter()
      .find(|profile| profile_key(&profile.session) == key)
      .map(|profile| profile.session.clone())
  }

  pub(crate) fn mark_active_restored(&mut self, key: &str) -> bool {
    let Some(profile) = self
      .profiles
      .iter_mut()
      .find(|profile| profile_key(&profile.session) == key)
    else {
      return false;
    };
    profile.last_restore_error = None;
    self.active_profile_key = Some(key.to_string());
    true
  }

  pub(crate) fn mark_restore_failed(&mut self, key: &str, message: String) -> bool {
    let Some(profile) = self
      .profiles
      .iter_mut()
      .find(|profile| profile_key(&profile.session) == key)
    else {
      return false;
    };
    profile.last_restore_error = Some(message);
    self.active_profile_key = Some(key.to_string());
    true
  }

  pub(crate) fn remove_profile(&mut self, key: &str) -> bool {
    let initial_len = self.profiles.len();
    self
      .profiles
      .retain(|profile| profile_key(&profile.session) != key);
    let removed = self.profiles.len() != initial_len;
    if self.active_profile_key.as_deref() == Some(key) {
      self.active_profile_key = None;
    }
    removed
  }

  pub(crate) fn active_profile_key(&self) -> Option<&str> {
    self.active_profile_key.as_deref()
  }
}

impl StoredSavedServiceProfile {
  fn summary(&self, active_profile_key: Option<&str>) -> SavedServiceProfileSummary {
    let key = profile_key(&self.session);
    SavedServiceProfileSummary {
      active: active_profile_key == Some(key.as_str()),
      key,
      provider: self.session.provider,
      server_name: self.session.server_name.clone(),
      server_url: self.session.server_url.clone(),
      user_name: self.session.user_name.clone(),
      last_restore_error: self.last_restore_error.clone(),
    }
  }
}

pub(crate) fn load_profiles(app: &tauri::AppHandle) -> Result<SavedServiceProfileStore, String> {
  let store = app
    .store(AUTH_PROFILES_STORE_FILE)
    .map_err(|err| err.to_string())?;
  let Some(value) = store.get(AUTH_PROFILES_STORE_KEY) else {
    return Ok(SavedServiceProfileStore::default());
  };
  serde_json::from_value(value.clone()).map_err(|err| err.to_string())
}

pub(crate) fn save_profiles(
  app: &tauri::AppHandle,
  profiles: &SavedServiceProfileStore,
) -> Result<(), String> {
  let store = app
    .store(AUTH_PROFILES_STORE_FILE)
    .map_err(|err| err.to_string())?;
  store.set(
    AUTH_PROFILES_STORE_KEY.to_string(),
    serde_json::to_value(profiles).map_err(|err| err.to_string())?,
  );
  store.save().map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn session(
    provider: MediaServerProvider,
    server_url: &str,
    user_name: &str,
    token: &str,
  ) -> SavedSession {
    SavedSession {
      access_token: token.to_string(),
      device_id: Some("device-1".to_string()),
      provider,
      server_name: Some("Media Home".to_string()),
      server_url: server_url.to_string(),
      user_id: format!("user-{user_name}"),
      user_name: user_name.to_string(),
    }
  }

  #[test]
  fn upsert_active_replaces_matching_provider_server_and_user_profile() {
    let mut store = SavedServiceProfileStore::default();
    let first = session(
      MediaServerProvider::Jellyfin,
      "https://media.example.com",
      "Ada",
      "token-1",
    );
    let updated = session(
      MediaServerProvider::Jellyfin,
      "https://media.example.com/",
      "Ada",
      "token-2",
    );

    let key = store.upsert_active(first);
    let updated_key = store.upsert_active(updated);

    assert_eq!(key, updated_key);
    assert_eq!(store.profiles.len(), 1);
    assert_eq!(store.profiles[0].session.access_token, "token-2");
    assert_eq!(store.active_profile_key(), Some(key.as_str()));
  }

  #[test]
  fn summary_does_not_expose_access_token() {
    let mut store = SavedServiceProfileStore::default();
    store.upsert_active(session(
      MediaServerProvider::Emby,
      "https://emby.example.com",
      "Grace",
      "secret-token",
    ));

    let encoded = serde_json::to_string(&store.summary()).expect("summary should serialize");

    assert!(!encoded.contains("secret-token"));
  }

  #[test]
  fn mark_restore_failed_keeps_profile_and_active_key() {
    let mut store = SavedServiceProfileStore::default();
    let key = store.upsert_active(session(
      MediaServerProvider::Jellyfin,
      "https://media.example.com",
      "Ada",
      "token-1",
    ));

    assert!(store.mark_restore_failed(&key, "expired".to_string()));

    let summary = store.summary();
    assert_eq!(summary.active_profile_key, Some(key));
    assert_eq!(
      summary.profiles[0].last_restore_error.as_deref(),
      Some("expired")
    );
  }

  #[test]
  fn remove_active_profile_keeps_other_profiles_without_auto_selecting_next() {
    let mut store = SavedServiceProfileStore::default();
    let active = store.upsert_active(session(
      MediaServerProvider::Jellyfin,
      "https://media.example.com",
      "Ada",
      "token-1",
    ));
    store.upsert_active(session(
      MediaServerProvider::Emby,
      "https://emby.example.com",
      "Grace",
      "token-2",
    ));
    store.active_profile_key = Some(active.clone());

    assert!(store.remove_profile(&active));

    assert!(store.active_profile_key.is_none());
    assert_eq!(store.profiles.len(), 1);
  }
}
