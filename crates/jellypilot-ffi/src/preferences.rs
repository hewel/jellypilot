use crate::{JellypilotSdk, SdkError};

#[derive(Clone, Debug, uniffi::Record)]
pub struct LoginPrefill {
    pub server_url: String,
    pub username: String,
    pub provider: crate::Provider,
    pub remember: bool,
}

impl From<jellypilot_sdk::LoginPrefill> for LoginPrefill {
    fn from(value: jellypilot_sdk::LoginPrefill) -> Self {
        Self {
            server_url: value.server_url,
            username: value.username,
            provider: value.provider.into(),
            remember: value.remember,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum IntroSkipMode {
    Automatic,
    Manual,
    Off,
}

impl From<IntroSkipMode> for jellypilot_core::intro_skipper::IntroSkipMode {
    fn from(value: IntroSkipMode) -> Self {
        match value {
            IntroSkipMode::Automatic => Self::Automatic,
            IntroSkipMode::Manual => Self::Manual,
            IntroSkipMode::Off => Self::Off,
        }
    }
}

impl From<jellypilot_core::intro_skipper::IntroSkipMode> for IntroSkipMode {
    fn from(value: jellypilot_core::intro_skipper::IntroSkipMode) -> Self {
        match value {
            jellypilot_core::intro_skipper::IntroSkipMode::Automatic => Self::Automatic,
            jellypilot_core::intro_skipper::IntroSkipMode::Manual => Self::Manual,
            jellypilot_core::intro_skipper::IntroSkipMode::Off => Self::Off,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct BusinessPreferences {
    pub auto_login: bool,
    pub playback_target_name: Option<String>,
    pub subtitle_languages: Vec<String>,
    pub prefer_original_audio: bool,
    pub remember_season_volume: bool,
    pub intro_mode: IntroSkipMode,
    pub auto_play_next: bool,
}

impl From<jellypilot_sdk::BusinessPreferences> for BusinessPreferences {
    fn from(value: jellypilot_sdk::BusinessPreferences) -> Self {
        Self {
            auto_login: value.auto_login,
            playback_target_name: value.playback_target_name,
            subtitle_languages: value.subtitle_languages,
            prefer_original_audio: value.prefer_original_audio,
            remember_season_volume: value.remember_season_volume,
            intro_mode: value.intro_mode.into(),
            auto_play_next: value.auto_play_next,
        }
    }
}

#[uniffi::export]
impl JellypilotSdk {
    pub fn login_prefill(&self) -> Result<LoginPrefill, SdkError> {
        self.sdk.login_prefill().map(Into::into).map_err(Into::into)
    }
    pub fn save_login_prefill(
        &self,
        server_url: String,
        username: String,
        provider: crate::Provider,
        remember: bool,
    ) -> Result<LoginPrefill, SdkError> {
        self.sdk
            .save_login_prefill(server_url, username, provider.into(), remember)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn business_preferences(&self) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .business_preferences()
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn set_auto_login(&self, enabled: bool) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .set_auto_login(enabled)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn set_playback_target_name(&self, name: String) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .set_playback_target_name(name)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn set_prefer_original_audio(
        &self,
        enabled: bool,
    ) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .set_prefer_original_audio(enabled)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn set_remember_season_volume(
        &self,
        enabled: bool,
    ) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .set_remember_season_volume(enabled)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn add_subtitle_language(&self, language: String) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .add_subtitle_language(language)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn set_subtitle_languages(
        &self,
        languages: Vec<String>,
    ) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .set_subtitle_languages(languages)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn remove_subtitle_language(&self, index: u32) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .remove_subtitle_language(index)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn move_subtitle_language(
        &self,
        index: u32,
        offset: i32,
    ) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .move_subtitle_language(index, offset)
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn set_intro_mode(&self, mode: IntroSkipMode) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .set_intro_mode(mode.into())
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn set_auto_play_next(&self, enabled: bool) -> Result<BusinessPreferences, SdkError> {
        self.sdk
            .set_auto_play_next(enabled)
            .map(Into::into)
            .map_err(Into::into)
    }
}
