//! Shared media preparation, correlated observations and playback reporting.

mod preparation;
pub(crate) mod recovery;
mod session;

pub use jellypilot_core::intro_skipper::IntroSkipMode;
pub use preparation::*;
pub use recovery::LocalPlaybackRecovery;
pub use session::{
    passive_progress_report_due, reporting_succeeded_with_timeout, volume_level, PlaybackFinish,
    PlaybackObservation, PlaybackPlan, PlaybackSession, PlaybackTrack, PlaybackUpdate, SkipPrompt,
    PASSIVE_PROGRESS_REPORT_INTERVAL, PLAYBACK_REPORT_TIMEOUT,
};

use std::sync::{Arc, Weak};

use jellypilot_core::audio_tracks::{AudioTrackKey, AudioTrackStore};
use jellypilot_core::intro_skipper::IntroSkipper;
use jellypilot_core::volume_memory::{SeasonVolumeKey, SeasonVolumeStore};
use jellypilot_media_server::JellyfinClient;

use crate::{OperationToken, Sdk, SdkError};

#[derive(Default)]
pub(crate) struct PlaybackRegistry {
    sequence: u64,
    active: Option<Weak<PlaybackSession>>,
}

impl Sdk {
    /// Prepares original/direct media for an Android player. Preparation does not
    /// report a start: the host must submit its first loaded observation.
    /// An existing session must be finished before admitting its replacement.
    pub async fn prepare_playback(
        &self,
        token: Arc<OperationToken>,
        item_id: String,
        position: PlaybackStartPosition,
        selection: PlaybackSelection,
    ) -> Result<Arc<PlaybackSession>, SdkError> {
        if item_id.trim().is_empty() {
            return Err(SdkError::InvalidInput(
                "an item identity is required".into(),
            ));
        }
        if let PlaybackStartPosition::At(seconds) = position {
            checked_seconds_to_ticks(seconds).map_err(playback_error)?;
        }
        let admission = self.playback_admission()?;
        let preferences = self.business_preferences()?;
        let scope = token.scope_ref()?;
        let sequence = {
            let state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
            if state.epoch != token.epoch
                || state
                    .active
                    .as_ref()
                    .is_none_or(|active| active.key.as_str() != scope.profile_key)
            {
                return Err(SdkError::Stale);
            }
            if token.is_cancelled() {
                return Err(SdkError::Cancelled);
            }
            if state.closed || state.handoff_in_progress || state.sign_out_cleanup_pending {
                return Err(SdkError::OperationInProgress);
            }
            let mut registry = self.inner.playback.lock().map_err(|_| SdkError::Closed)?;
            if registry
                .active
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|active| active.scope == scope)
            {
                return Err(SdkError::OperationInProgress);
            }
            registry.active = None;
            registry.sequence = registry.sequence.wrapping_add(1);
            registry.sequence
        };
        let inner = Arc::clone(&self.inner);
        self.inner
            .scoped(&token, move |client| async move {
                let detail = client.library().item_detail(item_id.clone()).await?;
                if !detail.can_play || !matches!(detail.item_type.as_str(), "Movie" | "Episode") {
                    return Err(SdkError::InvalidInput(
                        "the selected item is not playable".into(),
                    ));
                }
                let start_position_seconds = match position {
                    PlaybackStartPosition::Beginning => 0.0,
                    PlaybackStartPosition::Resume => detail.resume_position_seconds.unwrap_or(0.0),
                    PlaybackStartPosition::At(seconds) => seconds,
                };
                checked_seconds_to_ticks(start_position_seconds).map_err(playback_error)?;
                let profile = {
                    let state = inner.state.lock().map_err(|_| SdkError::Closed)?;
                    state
                        .active
                        .as_ref()
                        .map(|active| active.scope.clone())
                        .ok_or(SdkError::NoActiveProfile)?
                };
                let audio_key =
                    AudioTrackKey::new(detail.series_id.as_deref().unwrap_or(&detail.id));
                let tracks =
                    AudioTrackStore::load_in_dir(inner.config.storage_dir.clone(), profile.clone())
                        .map_err(|error| SdkError::Storage(error.to_string()))?;
                let volume_memory =
                    if preferences.remember_season_volume && detail.item_type == "Episode" {
                        detail
                            .series_id
                            .as_deref()
                            .zip(detail.season_number)
                            .and_then(|(series, season)| SeasonVolumeKey::new(series, season))
                            .map(|key| {
                                SeasonVolumeStore::load_in_dir(
                                    inner.config.storage_dir.clone(),
                                    profile.clone(),
                                )
                                .map(|store| (key, store))
                            })
                            .transpose()
                            .map_err(|error| SdkError::Storage(error.to_string()))?
                    } else {
                        None
                    };
                let prepared = prepare_media(
                    &JellyfinPlaybackServer::from(Arc::clone(&client)),
                    PreparationRequest {
                        item_id: detail.id.clone(),
                        start_position_seconds,
                        selection,
                        runtime_seconds: detail.runtime_seconds,
                        original_language: detail.original_language.clone(),
                    },
                    PreparationPreferences {
                        original_audio_enabled: preferences.prefer_original_audio,
                        subtitle_languages: preferences.subtitle_languages,
                        remembered_audio: audio_key.as_ref().and_then(|key| tracks.get(key)),
                        remembered_subtitle: audio_key
                            .as_ref()
                            .and_then(|key| tracks.get_subtitle(key)),
                    },
                )
                .await
                .map_err(playback_error)?;
                let mut intro = IntroSkipper::new(preferences.intro_mode);
                let ranges = if detail.item_type == "Episode" && client.supports_intro_skipper() {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        client.playback().get_intro_skipper_ranges(&item_id),
                    )
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or_default()
                } else {
                    Vec::new()
                };
                intro.replace_ranges(ranges.clone());
                if !admission.is_current() || !inner.scope_is_active(&scope) {
                    return Err(SdkError::Stale);
                }
                let session = Arc::new(PlaybackSession::new(
                    Arc::clone(&inner),
                    client,
                    scope,
                    sequence,
                    detail,
                    prepared,
                    start_position_seconds,
                    intro,
                    ranges,
                    tracks,
                    audio_key,
                    volume_memory,
                ));
                let state = inner.state.lock().map_err(|_| SdkError::Closed)?;
                if state.closed
                    || state.epoch != session.scope.generation
                    || state.playback_generation != admission.generation
                    || state.handoff_in_progress
                    || state.sign_out_cleanup_pending
                {
                    return Err(SdkError::Stale);
                }
                let mut registry = inner.playback.lock().map_err(|_| SdkError::Closed)?;
                if registry.sequence != sequence {
                    return Err(SdkError::Stale);
                }
                registry.active = Some(Arc::downgrade(&session));
                Ok(session)
            })
            .await
    }
}

fn playback_error(error: PlaybackError) -> SdkError {
    match error {
        PlaybackError::InvalidStartPosition
        | PlaybackError::UnsupportedItemType
        | PlaybackError::ItemNotPlayable
        | PlaybackError::TrackUnavailable => SdkError::InvalidInput(error.to_string()),
        _ => SdkError::Request(error.to_string()),
    }
}

async fn adjacent_item(
    client: &JellyfinClient,
    item_id: &str,
    next: bool,
) -> Result<Option<String>, SdkError> {
    let item = client.playback().get_item(item_id).await?;
    let adjacent = if next {
        client.playback().get_next_episode(&item).await?
    } else {
        client.playback().get_previous_episode(&item).await?
    };
    Ok(adjacent.map(|item| item.id))
}

impl crate::SdkInner {
    /// Completes shared playback after a platform handoff succeeded. A platform
    /// hook may already have finished it while disposing its native player.
    pub(crate) async fn clear_playback_for_handoff(&self) -> Result<(), SdkError> {
        let active = self
            .playback
            .lock()
            .map_err(|_| SdkError::Closed)?
            .active
            .as_ref()
            .and_then(Weak::upgrade);
        if let Some(active) = active {
            active.finish_for_handoff().await?;
        }
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if let Some(active) = &state.active {
            recovery::clear_profile(self, active.key.as_str())?;
        }
        Ok(())
    }
}
