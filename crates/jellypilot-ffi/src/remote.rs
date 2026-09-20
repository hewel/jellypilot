use crate::{JellypilotSdk, OperationToken, SdkError};
use jellypilot_sdk::remote as sdk;
use std::sync::Arc;

#[derive(Clone, Debug, uniffi::Enum)]
pub enum RemoteCommand {
    Start {
        item_id: String,
        start_position_seconds: Option<f64>,
        media_source_id: Option<String>,
        audio_stream_index: Option<i32>,
        subtitle_stream_index: Option<i32>,
    },
    Pause,
    Resume,
    TogglePause,
    Stop,
    Seek {
        seconds: f64,
    },
    Next,
    Previous,
    SetVolume {
        volume: f64,
    },
    ToggleMute,
    SetAudioTrack {
        index: i32,
    },
    SetSubtitleTrack {
        index: i32,
    },
}

impl From<sdk::RemoteCommand> for RemoteCommand {
    fn from(value: sdk::RemoteCommand) -> Self {
        match value {
            sdk::RemoteCommand::Start {
                item_id,
                start_position_ticks,
                media_source_id,
                audio_stream_index,
                subtitle_stream_index,
            } => Self::Start {
                item_id,
                start_position_seconds: start_position_ticks
                    .map(jellypilot_media_server::ticks_to_seconds),
                media_source_id,
                audio_stream_index,
                subtitle_stream_index,
            },
            sdk::RemoteCommand::Pause => Self::Pause,
            sdk::RemoteCommand::Resume => Self::Resume,
            sdk::RemoteCommand::TogglePause => Self::TogglePause,
            sdk::RemoteCommand::Stop => Self::Stop,
            sdk::RemoteCommand::Seek { seconds } => Self::Seek { seconds },
            sdk::RemoteCommand::Next => Self::Next,
            sdk::RemoteCommand::Previous => Self::Previous,
            sdk::RemoteCommand::SetVolume { volume } => Self::SetVolume { volume },
            sdk::RemoteCommand::ToggleMute => Self::ToggleMute,
            sdk::RemoteCommand::SetAudioTrack { index } => Self::SetAudioTrack { index },
            sdk::RemoteCommand::SetSubtitleTrack { index } => Self::SetSubtitleTrack { index },
        }
    }
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum RemoteTargetState {
    Connecting,
    Available,
    Lost,
    Closed,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteTargetEvent {
    pub state: RemoteTargetState,
    pub command: Option<RemoteCommand>,
    pub generation: u64,
}

#[derive(uniffi::Object)]
pub struct RemoteTarget {
    inner: Arc<sdk::RemoteTarget>,
}

#[uniffi::export(async_runtime = "tokio")]
impl RemoteTarget {
    pub fn is_command_current(&self, generation: u64) -> bool {
        self.inner.is_command_current(generation)
    }
    /// Terminal revocation; call before the generated object handle is disposed.
    pub fn stop(&self) {
        self.inner.close();
    }
    pub async fn next_event(&self) -> Result<RemoteTargetEvent, SdkError> {
        let event = self.inner.next_event().await.map_err(SdkError::from)?;
        Ok(RemoteTargetEvent {
            state: match event.state {
                sdk::RemoteTargetState::Connecting => RemoteTargetState::Connecting,
                sdk::RemoteTargetState::Available => RemoteTargetState::Available,
                sdk::RemoteTargetState::Lost => RemoteTargetState::Lost,
                sdk::RemoteTargetState::Closed => RemoteTargetState::Closed,
            },
            command: event.command.map(Into::into),
            generation: event.generation,
        })
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl JellypilotSdk {
    pub async fn open_remote_target(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Arc<RemoteTarget>, SdkError> {
        self.sdk
            .open_remote_target(Arc::clone(&token.inner))
            .await
            .map(|inner| Arc::new(RemoteTarget { inner }))
            .map_err(Into::into)
    }
}
