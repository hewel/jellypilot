use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{OperationToken, ProfileScopeRef, Sdk, SdkError, SdkInner};

/// A token-free, device-local interrupted session. Server Continue Watching
/// retains its own resume position; restoration uses this value explicitly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalPlaybackRecovery {
    pub item_id: String,
    pub title: String,
    pub position_seconds: f64,
}

#[derive(Default, Serialize, Deserialize)]
struct StoredRecovery {
    records: Vec<RecoveryRecord>,
}

#[derive(Serialize, Deserialize)]
struct RecoveryRecord {
    profile_key: String,
    recovery: LocalPlaybackRecovery,
}

fn recovery_path(inner: &SdkInner) -> PathBuf {
    inner.config.storage_dir.join("playback-recovery.json")
}

fn read(inner: &SdkInner) -> Result<StoredRecovery, SdkError> {
    match fs::read(recovery_path(inner)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| SdkError::Storage("playback recovery data is invalid".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StoredRecovery::default()),
        Err(_) => Err(SdkError::Storage(
            "playback recovery could not be read".into(),
        )),
    }
}

fn write(inner: &SdkInner, stored: &StoredRecovery) -> Result<(), SdkError> {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(&inner.config.storage_dir)?;
        let path = recovery_path(inner);
        let temporary = path.with_extension("json.tmp");
        let file = fs::File::create(&temporary)?;
        serde_json::to_writer(&file, stored)?;
        file.sync_all()?;
        fs::rename(temporary, path)?;
        Ok(())
    })();
    result.map_err(|_| SdkError::Storage("playback recovery could not be saved".into()))
}

pub(super) fn save(
    inner: &SdkInner,
    scope: &ProfileScopeRef,
    sequence: u64,
    recovery: LocalPlaybackRecovery,
) -> Result<(), SdkError> {
    let state = inner.state.lock().map_err(|_| SdkError::Closed)?;
    if state.closed
        || state.epoch != scope.generation
        || state
            .active
            .as_ref()
            .is_none_or(|active| active.key.as_str() != scope.profile_key)
    {
        return Err(SdkError::Stale);
    }
    let registry = inner.playback.lock().map_err(|_| SdkError::Closed)?;
    if registry.sequence != sequence {
        return Err(SdkError::Stale);
    }
    let mut stored = read(inner)?;
    stored
        .records
        .retain(|record| record.profile_key != scope.profile_key);
    stored.records.push(RecoveryRecord {
        profile_key: scope.profile_key.clone(),
        recovery,
    });
    write(inner, &stored)
}

/// Called with SDK state held when committing an account transition; it also
/// serializes with playback persistence so a late callback cannot recreate it.
pub(crate) fn clear_profile(inner: &SdkInner, profile_key: &str) -> Result<(), SdkError> {
    let _registry = inner.playback.lock().map_err(|_| SdkError::Closed)?;
    let mut stored = read(inner)?;
    let before = stored.records.len();
    stored
        .records
        .retain(|record| record.profile_key != profile_key);
    if stored.records.len() != before {
        write(inner, &stored)?;
    }
    Ok(())
}

impl Sdk {
    pub async fn local_playback_recovery(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Option<LocalPlaybackRecovery>, SdkError> {
        let inner = Arc::clone(&self.inner);
        let scope = token.scope_ref()?;
        self.inner
            .scoped(&token, move |_| async move {
                let _registry = inner.playback.lock().map_err(|_| SdkError::Closed)?;
                let stored = read(&inner)?;
                Ok(stored
                    .records
                    .into_iter()
                    .find(|record| record.profile_key == scope.profile_key)
                    .map(|record| record.recovery)
                    .filter(|recovery| {
                        !recovery.item_id.trim().is_empty()
                            && super::checked_seconds_to_ticks(recovery.position_seconds).is_ok()
                    }))
            })
            .await
    }

    pub async fn clear_local_playback_recovery(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<(), SdkError> {
        let inner = Arc::clone(&self.inner);
        let scope = token.scope_ref()?;
        self.inner
            .scoped(&token, move |_| async move {
                let state = inner.state.lock().map_err(|_| SdkError::Closed)?;
                if state.epoch != scope.generation {
                    return Err(SdkError::Stale);
                }
                clear_profile(&inner, &scope.profile_key)
            })
            .await
    }
}
