//! Saved-profile wire formats, including v3 records written by MoonTVPlus builds.

use jellypilot_media_server::{MediaServerProvider, SavedSession};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use super::{
    profile_key, profile_key_for_identity, AuthStorageError, ProfileScope, SavedProfileKey,
    SavedProfileSummary, SavedProfilesSnapshot, SensitiveSavedSession,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProfileState {
    version: u32,
    profiles: Vec<StoredProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_successfully_activated: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    next_incarnation: u64,
}

// Storage knows about MoonTVPlus without exposing an unsupported provider to
// the runtime. Keep its records when this build changes a Jellyfin/Emby login.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum StoredProvider {
    #[default]
    Jellyfin,
    Emby,
    MoonTvPlus,
}

impl StoredProvider {
    fn supported(self) -> Option<MediaServerProvider> {
        match self {
            Self::Jellyfin => Some(MediaServerProvider::Jellyfin),
            Self::Emby => Some(MediaServerProvider::Emby),
            Self::MoonTvPlus => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProfile {
    #[serde(default)]
    provider: StoredProvider,
    server_url: String,
    access_token: String,
    user_id: String,
    user_name: String,
    server_name: Option<String>,
    device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy_token: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    incarnation: u64,
}

impl Drop for StoredProfile {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.proxy_token.zeroize();
    }
}

impl StoredProfile {
    fn from_session(mut protected: SensitiveSavedSession) -> Self {
        let session = protected.take_for_storage();
        Self {
            provider: match session.provider {
                MediaServerProvider::Jellyfin => StoredProvider::Jellyfin,
                MediaServerProvider::Emby => StoredProvider::Emby,
            },
            server_url: session.server_url,
            access_token: session.access_token,
            user_id: session.user_id,
            user_name: session.user_name,
            server_name: session.server_name,
            device_id: session.device_id,
            proxy_token: None,
            incarnation: 0,
        }
    }

    fn session(&self) -> Option<SensitiveSavedSession> {
        Some(SensitiveSavedSession::new(SavedSession {
            provider: self.provider.supported()?,
            server_url: self.server_url.clone(),
            access_token: self.access_token.clone(),
            user_id: self.user_id.clone(),
            user_name: self.user_name.clone(),
            server_name: self.server_name.clone(),
            device_id: self.device_id.clone(),
        }))
    }

    fn supports_key(&self, key: &SavedProfileKey) -> bool {
        self.provider.supported().is_some_and(|provider| {
            profile_key_for_identity(provider, &self.server_url, &self.user_id) == *key
        })
    }

    fn valid(&self) -> bool {
        !self.server_url.trim().trim_end_matches('/').is_empty()
            && !self.user_id.trim().is_empty()
            && !self.user_name.trim().is_empty()
            && !self.access_token.trim().is_empty()
    }
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl Default for ProfileState {
    fn default() -> Self {
        Self {
            version: 2,
            profiles: Vec::new(),
            last_successfully_activated: None,
            next_incarnation: 0,
        }
    }
}

impl ProfileState {
    pub(super) fn decode(secret: &[u8]) -> Result<Self, AuthStorageError> {
        let mut state: Self =
            serde_json::from_slice(secret).map_err(|_| AuthStorageError::Corrupt)?;
        if !matches!(state.version, 1..=3) || !state.profiles.iter().all(StoredProfile::valid) {
            return Err(AuthStorageError::Corrupt);
        }
        if state.version == 1 {
            state.last_successfully_activated = None;
            state.version = 2;
        }
        if state.version == 3 {
            // Match v3's allocation rule, including legacy records whose
            // incarnation has not been assigned yet. Never reuse a fence.
            let minimum_next = state
                .profiles
                .iter()
                .map(|profile| profile.incarnation)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(AuthStorageError::Corrupt)?;
            state.next_incarnation = state.next_incarnation.max(minimum_next);
            for profile in &mut state.profiles {
                if profile.incarnation == 0 {
                    profile.incarnation = state.next_incarnation;
                    state.next_incarnation = state
                        .next_incarnation
                        .checked_add(1)
                        .ok_or(AuthStorageError::Corrupt)?;
                }
            }
        }
        Ok(state)
    }

    pub(super) fn encode(&self) -> Result<Zeroizing<Vec<u8>>, AuthStorageError> {
        serde_json::to_vec(self)
            .map(Zeroizing::new)
            .map_err(|_| AuthStorageError::WriteFailed)
    }

    pub(super) fn snapshot(&self) -> Result<SavedProfilesSnapshot, AuthStorageError> {
        let profiles = self.summaries()?;
        let last_successfully_activated = self
            .last_successfully_activated
            .as_ref()
            .filter(|key| profiles.iter().any(|profile| profile.key.0 == **key))
            .cloned()
            .map(SavedProfileKey);
        Ok(SavedProfilesSnapshot {
            profiles,
            last_successfully_activated,
        })
    }

    pub(super) fn summaries(&self) -> Result<Vec<SavedProfileSummary>, AuthStorageError> {
        self.profiles
            .iter()
            .filter_map(|profile| {
                profile
                    .provider
                    .supported()
                    .map(|provider| (profile, provider))
            })
            .map(|(profile, provider)| {
                let scope = ProfileScope::new(provider, &profile.server_url, &profile.user_id)
                    .map_err(|_| AuthStorageError::Corrupt)?;
                Ok(SavedProfileSummary {
                    key: SavedProfileKey::for_scope(&scope),
                    provider,
                    server_url: profile.server_url.clone(),
                    server_name: profile.server_name.clone(),
                    user_name: profile.user_name.clone(),
                    scope,
                })
            })
            .collect()
    }

    pub(super) fn session(
        &self,
        key: &SavedProfileKey,
    ) -> Result<SensitiveSavedSession, AuthStorageError> {
        self.profiles
            .iter()
            .find(|profile| profile.supports_key(key))
            .and_then(StoredProfile::session)
            .ok_or(AuthStorageError::ProfileNotFound)
    }

    pub(super) fn save(
        &mut self,
        session: SensitiveSavedSession,
    ) -> Result<SavedProfileKey, AuthStorageError> {
        let key = profile_key(&session);
        let mut profile = StoredProfile::from_session(session);
        if !profile.valid() {
            return Err(AuthStorageError::Corrupt);
        }
        if self.version == 3 {
            profile.incarnation = self.next_incarnation;
            self.next_incarnation = self
                .next_incarnation
                .checked_add(1)
                .ok_or(AuthStorageError::Corrupt)?;
        }
        if let Some(index) = self
            .profiles
            .iter()
            .position(|saved| saved.supports_key(&key))
        {
            profile.proxy_token = self.profiles.remove(index).proxy_token.take();
        }
        self.profiles.insert(0, profile);
        Ok(key)
    }

    pub(super) fn remove(&mut self, key: &SavedProfileKey) -> Result<(), AuthStorageError> {
        let index = self
            .profiles
            .iter()
            .position(|profile| profile.supports_key(key))
            .ok_or(AuthStorageError::ProfileNotFound)?;
        self.profiles.remove(index);
        if self.last_successfully_activated.as_deref() == Some(key.as_str()) {
            self.last_successfully_activated = None;
        }
        Ok(())
    }

    pub(super) fn record_activation(
        &mut self,
        key: &SavedProfileKey,
    ) -> Result<bool, AuthStorageError> {
        if !self
            .profiles
            .iter()
            .any(|profile| profile.supports_key(key))
        {
            return Err(AuthStorageError::ProfileNotFound);
        }
        if self.last_successfully_activated.as_deref() == Some(key.as_str()) {
            return Ok(false);
        }
        self.last_successfully_activated = Some(key.0.clone());
        Ok(true)
    }

    pub(super) fn can_delete(&self) -> bool {
        // Even after Sign Out removes every credential, v3 must retain its
        // counter so a later login cannot match an old callback's fence.
        self.version < 3 && self.profiles.is_empty()
    }
}
