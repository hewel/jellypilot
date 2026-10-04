//! Thin bindings for the SDK-owned outbound remote controller.
use crate::{JellypilotSdk, PlaybackStartPosition, ProfileScopeRef, SdkError};
use jellypilot_sdk::remote_control as sdk;
use std::sync::Arc;

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteControlTargetKey {
    pub session_id: String,
    pub device_id: String,
}
impl From<RemoteControlTargetKey> for sdk::RemoteControlTargetKey {
    fn from(v: RemoteControlTargetKey) -> Self {
        Self {
            session_id: v.session_id,
            device_id: v.device_id,
        }
    }
}
impl From<sdk::RemoteControlTargetKey> for RemoteControlTargetKey {
    fn from(v: sdk::RemoteControlTargetKey) -> Self {
        Self {
            session_id: v.session_id,
            device_id: v.device_id,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteControlCapabilities {
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_stop: bool,
    pub can_seek: bool,
    pub can_set_volume: bool,
    pub can_play_now: bool,
}
impl From<sdk::RemoteControlCapabilities> for RemoteControlCapabilities {
    fn from(v: sdk::RemoteControlCapabilities) -> Self {
        Self {
            can_pause: v.can_pause,
            can_resume: v.can_resume,
            can_stop: v.can_stop,
            can_seek: v.can_seek,
            can_set_volume: v.can_set_volume,
            can_play_now: v.can_play_now,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteControlNowPlaying {
    pub item_id: String,
    pub title: String,
    pub position_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub paused: Option<bool>,
}
impl From<sdk::RemoteControlNowPlaying> for RemoteControlNowPlaying {
    fn from(v: sdk::RemoteControlNowPlaying) -> Self {
        Self {
            item_id: v.item_id,
            title: v.title,
            position_seconds: v.position_seconds,
            duration_seconds: v.duration_seconds,
            paused: v.paused,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteControlTarget {
    pub key: RemoteControlTargetKey,
    pub device_name: String,
    pub client_name: String,
    pub user_name: Option<String>,
    pub now_playing: Option<RemoteControlNowPlaying>,
    pub volume: Option<u32>,
    pub capabilities: RemoteControlCapabilities,
}
impl From<sdk::RemoteControlTarget> for RemoteControlTarget {
    fn from(v: sdk::RemoteControlTarget) -> Self {
        Self {
            key: v.key.into(),
            device_name: v.device_name,
            client_name: v.client_name,
            user_name: v.user_name,
            now_playing: v.now_playing.map(Into::into),
            volume: v.volume,
            capabilities: v.capabilities.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum RemoteControllerStatus {
    Inactive,
    Loading,
    Ready,
    Failed,
    Closed,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteControlSnapshot {
    pub revision: u64,
    pub scope: ProfileScopeRef,
    pub generation: u64,
    pub status: RemoteControllerStatus,
    pub targets: Vec<RemoteControlTarget>,
    pub selected: Option<RemoteControlTargetKey>,
    pub refreshing: bool,
    pub command_pending: bool,
    pub error: Option<String>,
}
impl From<sdk::RemoteControlSnapshot> for RemoteControlSnapshot {
    fn from(v: sdk::RemoteControlSnapshot) -> Self {
        Self {
            revision: v.revision,
            scope: v.scope.into(),
            generation: v.generation,
            status: match v.status {
                sdk::RemoteControllerStatus::Inactive => RemoteControllerStatus::Inactive,
                sdk::RemoteControllerStatus::Loading => RemoteControllerStatus::Loading,
                sdk::RemoteControllerStatus::Ready => RemoteControllerStatus::Ready,
                sdk::RemoteControllerStatus::Failed => RemoteControllerStatus::Failed,
                sdk::RemoteControllerStatus::Closed => RemoteControllerStatus::Closed,
            },
            targets: v.targets.into_iter().map(Into::into).collect(),
            selected: v.selected.map(Into::into),
            refreshing: v.refreshing,
            command_pending: v.command_pending,
            error: v.error,
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum RemoteControlCommand {
    Pause,
    Resume,
    Stop,
    Seek {
        seconds: f64,
    },
    SetVolume {
        volume: u32,
    },
    PlayNow {
        item_id: String,
        position: PlaybackStartPosition,
    },
}
impl From<RemoteControlCommand> for sdk::RemoteControlCommand {
    fn from(v: RemoteControlCommand) -> Self {
        match v {
            RemoteControlCommand::Pause => Self::Pause,
            RemoteControlCommand::Resume => Self::Resume,
            RemoteControlCommand::Stop => Self::Stop,
            RemoteControlCommand::Seek { seconds } => Self::Seek { seconds },
            RemoteControlCommand::SetVolume { volume } => Self::SetVolume { volume },
            RemoteControlCommand::PlayNow { item_id, position } => Self::PlayNow {
                item_id,
                position: position.into(),
            },
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteControlReceipt {
    pub command_sequence: u64,
    pub server_accepted: bool,
}

#[derive(uniffi::Object)]
pub struct RemoteController {
    inner: Arc<sdk::RemoteController>,
}

#[uniffi::export(async_runtime = "tokio")]
impl RemoteController {
    pub fn snapshot(&self) -> Result<RemoteControlSnapshot, SdkError> {
        self.inner.snapshot().map(Into::into).map_err(Into::into)
    }
    pub async fn next_snapshot(
        &self,
        after_revision: u64,
    ) -> Result<RemoteControlSnapshot, SdkError> {
        self.inner
            .next_snapshot(after_revision)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn refresh(&self) -> Result<(), SdkError> {
        self.inner.refresh().map_err(Into::into)
    }
    pub fn select_target(&self, key: RemoteControlTargetKey) -> Result<(), SdkError> {
        self.inner.select_target(key.into()).map_err(Into::into)
    }
    pub fn set_active(&self, active: bool) -> Result<(), SdkError> {
        self.inner.set_active(active).map_err(Into::into)
    }
    pub async fn execute(
        &self,
        generation: u64,
        target: RemoteControlTargetKey,
        command: RemoteControlCommand,
    ) -> Result<RemoteControlReceipt, SdkError> {
        self.inner
            .execute(generation, target.into(), command.into())
            .await
            .map(|receipt| RemoteControlReceipt {
                command_sequence: receipt.command_sequence,
                server_accepted: receipt.server_accepted,
            })
            .map_err(Into::into)
    }
    /// Ends polling and all waiting work before disposing this FFI handle.
    pub fn shutdown(&self) {
        self.inner.close();
    }
}

#[uniffi::export]
impl JellypilotSdk {
    pub fn open_remote_controller(&self) -> Result<Arc<RemoteController>, SdkError> {
        self.sdk
            .open_remote_controller()
            .map(|inner| Arc::new(RemoteController { inner }))
            .map_err(Into::into)
    }
}
