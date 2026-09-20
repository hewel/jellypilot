//! Shared business preferences backed by the existing domain settings store.

use jellypilot_core::config::{Settings, SettingsMutationError, SettingsStore};
use jellypilot_core::intro_skipper::IntroSkipMode;
use serde::{Deserialize, Serialize};

use crate::{Sdk, SdkError, SdkInner};

#[derive(Clone, Debug)]
pub struct LoginPrefill {
    pub server_url: String,
    pub username: String,
    pub provider: jellypilot_media_server::MediaServerProvider,
    pub remember: bool,
}

impl From<&Settings> for LoginPrefill {
    fn from(settings: &Settings) -> Self {
        let prefill = settings.login_prefill();
        let provider = if settings.login_provider() == "emby" {
            jellypilot_media_server::MediaServerProvider::Emby
        } else {
            jellypilot_media_server::MediaServerProvider::Jellyfin
        };
        Self {
            server_url: jellypilot_auth::login::validate_server_url(prefill.server_url(), provider)
                .unwrap_or_default(),
            username: prefill.username().to_owned(),
            provider,
            remember: settings.remembers_login_prefill(),
        }
    }
}

/// Device-wide business preferences. Android presentation preferences are
/// deliberately absent; a playback session owns its temporary Intro mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusinessPreferences {
    pub auto_login: bool,
    pub playback_target_name: Option<String>,
    pub subtitle_languages: Vec<String>,
    pub prefer_original_audio: bool,
    pub remember_season_volume: bool,
    pub intro_mode: IntroSkipMode,
    pub auto_play_next: bool,
}

impl From<&Settings> for BusinessPreferences {
    fn from(settings: &Settings) -> Self {
        Self {
            auto_login: settings.auto_login(),
            playback_target_name: settings.playback_target_name().map(str::to_owned),
            subtitle_languages: settings.subtitle_languages().to_vec(),
            prefer_original_audio: settings.prefer_original_audio(),
            remember_season_volume: settings.remember_season_volume(),
            intro_mode: IntroSkipMode::Automatic,
            auto_play_next: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct MobilePlaybackPreferences {
    intro_mode: String,
    auto_play_next: bool,
}

impl Default for MobilePlaybackPreferences {
    fn default() -> Self {
        Self {
            intro_mode: "automatic".to_owned(),
            auto_play_next: true,
        }
    }
}

impl MobilePlaybackPreferences {
    fn read(path: &std::path::Path) -> Result<Self, SdkError> {
        match std::fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|error| SdkError::Storage(error.to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(SdkError::Storage(error.to_string())),
        }
    }
}

impl Sdk {
    pub fn login_prefill(&self) -> Result<LoginPrefill, SdkError> {
        self.inner.check_open()?;
        let _guard = self
            .inner
            .preferences_gate
            .lock()
            .map_err(|_| SdkError::Closed)?;
        let store = SettingsStore::load_in_dir(self.inner.config.storage_dir.clone())
            .map_err(|error| SdkError::Storage(error.to_string()))?;
        Ok(store.snapshot().into())
    }

    pub fn save_login_prefill(
        &self,
        server_url: String,
        username: String,
        provider: jellypilot_media_server::MediaServerProvider,
        remember: bool,
    ) -> Result<LoginPrefill, SdkError> {
        self.inner.check_open()?;
        let server_url = if remember {
            jellypilot_auth::login::validate_server_url(&server_url, provider)
                .map_err(SdkError::InvalidInput)?
        } else {
            String::new()
        };
        let _guard = self
            .inner
            .preferences_gate
            .lock()
            .map_err(|_| SdkError::Closed)?;
        let mut store = SettingsStore::load_in_dir(self.inner.config.storage_dir.clone())
            .map_err(|error| SdkError::Storage(error.to_string()))?;
        let result = if remember {
            store.set_login_prefill(
                jellypilot_core::config::LoginPrefill::new(server_url, username),
                match provider {
                    jellypilot_media_server::MediaServerProvider::Jellyfin => "jellyfin",
                    jellypilot_media_server::MediaServerProvider::Emby => "emby",
                }
                .to_owned(),
            )
        } else {
            store.clear_login_prefill()
        };
        result.map_err(|error| match error {
            SettingsMutationError::Config(error) => SdkError::Storage(error.to_string()),
            error => SdkError::InvalidInput(error.to_string()),
        })?;
        Ok(store.snapshot().into())
    }
    /// Reads the shared preferences from this application's private storage.
    pub fn business_preferences(&self) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|_| Ok(false))
    }

    /// Enables or disables restoring the eligible saved account at startup.
    pub fn set_auto_login(&self, enabled: bool) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|store| store.set_auto_login(enabled))
    }

    /// Sets the name advertised to the server. Empty input restores the host name.
    pub fn set_playback_target_name(&self, name: String) -> Result<BusinessPreferences, SdkError> {
        let preferences = self.mutate_preferences(|store| store.set_playback_target_name(name))?;
        if let Some(client) = self.active_client() {
            client.set_device_name(
                preferences
                    .playback_target_name
                    .clone()
                    .unwrap_or_else(|| self.inner.config.device_name.clone()),
            );
        }
        Ok(preferences)
    }

    /// Selects original audio when it is available and no explicit preference wins.
    pub fn set_prefer_original_audio(
        &self,
        enabled: bool,
    ) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|store| store.set_prefer_original_audio(enabled))
    }

    /// Enables the existing profile/season volume-memory policy.
    pub fn set_remember_season_volume(
        &self,
        enabled: bool,
    ) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|store| store.set_remember_season_volume(enabled))
    }

    /// Adds a validated language code to the ordered subtitle preference list.
    pub fn add_subtitle_language(&self, language: String) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|store| store.add_subtitle_language(language))
    }

    pub fn set_subtitle_languages(
        &self,
        languages: Vec<String>,
    ) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|store| store.set_subtitle_languages(languages))
    }

    pub fn remove_subtitle_language(&self, index: u32) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|store| store.remove_subtitle_language(index as usize))
    }

    pub fn move_subtitle_language(
        &self,
        index: u32,
        offset: i32,
    ) -> Result<BusinessPreferences, SdkError> {
        self.mutate_preferences(|store| store.move_subtitle_language(index as usize, offset))
    }

    /// Changes the initial Android session mode without changing desktop series overrides.
    pub fn set_intro_mode(&self, mode: IntroSkipMode) -> Result<BusinessPreferences, SdkError> {
        self.mutate_mobile_preferences(|preferences| {
            preferences.intro_mode = match mode {
                IntroSkipMode::Automatic => "automatic",
                IntroSkipMode::Manual => "manual",
                IntroSkipMode::Off => "off",
            }
            .to_owned()
        })
    }

    /// Controls natural-end episode advance independently of Intro Skipper.
    pub fn set_auto_play_next(&self, enabled: bool) -> Result<BusinessPreferences, SdkError> {
        self.mutate_mobile_preferences(|preferences| preferences.auto_play_next = enabled)
    }

    fn mutate_mobile_preferences(
        &self,
        mutation: impl FnOnce(&mut MobilePlaybackPreferences),
    ) -> Result<BusinessPreferences, SdkError> {
        self.inner.check_open()?;
        {
            let _guard = self
                .inner
                .preferences_gate
                .lock()
                .map_err(|_| SdkError::Closed)?;
            let path = self
                .inner
                .config
                .storage_dir
                .join("mobile-playback-preferences.json");
            let mut preferences = MobilePlaybackPreferences::read(&path)?;
            mutation(&mut preferences);
            let bytes = serde_json::to_vec(&preferences)
                .map_err(|error| SdkError::Storage(error.to_string()))?;
            std::fs::create_dir_all(&self.inner.config.storage_dir)
                .map_err(|error| SdkError::Storage(error.to_string()))?;
            let temporary = path.with_extension("tmp");
            std::fs::write(&temporary, bytes)
                .and_then(|()| std::fs::rename(&temporary, &path))
                .map_err(|error| SdkError::Storage(error.to_string()))?;
        }
        self.business_preferences()
    }

    fn mutate_preferences(
        &self,
        mutation: impl FnOnce(&mut SettingsStore) -> Result<bool, SettingsMutationError>,
    ) -> Result<BusinessPreferences, SdkError> {
        self.inner.check_open()?;
        let _guard = self
            .inner
            .preferences_gate
            .lock()
            .map_err(|_| SdkError::Closed)?;
        let mut store = SettingsStore::load_in_dir(self.inner.config.storage_dir.clone())
            .map_err(|error| SdkError::Storage(error.to_string()))?;
        let mobile = MobilePlaybackPreferences::read(
            &self
                .inner
                .config
                .storage_dir
                .join("mobile-playback-preferences.json"),
        )?;
        mutation(&mut store).map_err(|error| match error {
            SettingsMutationError::Config(error) => SdkError::Storage(error.to_string()),
            error => SdkError::InvalidInput(error.to_string()),
        })?;
        let mut preferences = BusinessPreferences::from(store.snapshot());
        preferences.intro_mode = match mobile.intro_mode.as_str() {
            "off" => IntroSkipMode::Off,
            "manual" => IntroSkipMode::Manual,
            _ => IntroSkipMode::Automatic,
        };
        preferences.auto_play_next = mobile.auto_play_next;
        Ok(preferences)
    }
}

impl SdkInner {
    pub(crate) fn auto_play_next_preference(&self) -> Result<bool, SdkError> {
        let _guard = self.preferences_gate.lock().map_err(|_| SdkError::Closed)?;
        MobilePlaybackPreferences::read(
            &self
                .config
                .storage_dir
                .join("mobile-playback-preferences.json"),
        )
        .map(|preferences| preferences.auto_play_next)
    }

    pub(crate) fn configured_target_name(&self) -> Result<String, SdkError> {
        let _guard = self.preferences_gate.lock().map_err(|_| SdkError::Closed)?;
        let store = SettingsStore::load_in_dir(self.config.storage_dir.clone())
            .map_err(|error| SdkError::Storage(error.to_string()))?;
        Ok(store
            .snapshot()
            .playback_target_name()
            .unwrap_or(&self.config.device_name)
            .to_owned())
    }
}
