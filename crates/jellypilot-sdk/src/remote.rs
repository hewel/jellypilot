//! Scope-bound foreground playback target over the shared provider protocol.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use jellypilot_media_server::JellyfinClient;
use jellypilot_session::{
    finalize_remote_target, remote_index_value, remote_volume_value, JellyfinCommand,
    JellyfinWebSocket, JellyfinWebSocketEvent,
};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::{OperationToken, Sdk, SdkError, SdkInner};

static NEXT_TARGET_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub enum RemoteCommand {
    Start {
        item_id: String,
        start_position_ticks: Option<i64>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteTargetState {
    Connecting,
    Available,
    Lost,
    Closed,
}

#[derive(Clone, Debug)]
pub struct RemoteTargetEvent {
    pub state: RemoteTargetState,
    pub command: Option<RemoteCommand>,
    pub generation: u64,
}

/// A remote target exists only while the platform is visible and unlocked.
/// Closing it permanently revokes every queued command. Start operations must
/// recheck [`Self::is_command_current`] after asynchronous preparation.
pub struct RemoteTarget {
    inner: Arc<SdkInner>,
    token: Arc<OperationToken>,
    client: Arc<JellyfinClient>,
    socket: Arc<JellyfinWebSocket>,
    events: Mutex<mpsc::Receiver<JellyfinWebSocketEvent>>,
    closed: AtomicBool,
    cancel: CancellationToken,
    generation: u64,
}

impl RemoteTarget {
    pub fn is_command_current(&self, generation: u64) -> bool {
        generation == self.generation
            && !self.closed.load(Ordering::Acquire)
            && !self.token.is_cancelled()
            && self.inner.scope_is_active(&self.token.scope)
            && self
                .inner
                .state
                .lock()
                .is_ok_and(|state| !state.handoff_in_progress && !state.sign_out_cleanup_pending)
    }

    /// Revokes command admission immediately; transport teardown runs on the SDK runtime.
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.cancel.cancel();
            let socket = Arc::clone(&self.socket);
            self.inner.handle.spawn(async move {
                socket.disconnect().await;
            });
        }
    }

    pub async fn next_event(&self) -> Result<RemoteTargetEvent, SdkError> {
        let mut events = self.events.lock().await;
        loop {
            if !self.is_command_current(self.generation) {
                self.close();
                return Ok(self.event(RemoteTargetState::Closed, None));
            }
            let event = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Ok(self.event(RemoteTargetState::Closed, None)),
                () = self.token.cancel.cancelled() => {
                    self.close();
                    return Ok(self.event(RemoteTargetState::Closed, None));
                },
                event = events.recv() => event,
            };
            let event = match event {
                Some(JellyfinWebSocketEvent::Connected) => {
                    self.event(RemoteTargetState::Available, None)
                }
                Some(JellyfinWebSocketEvent::Reconnected) => {
                    finalize_remote_target(&self.client)
                        .await
                        .map_err(|error| SdkError::Request(error.to_string()))?;
                    self.event(RemoteTargetState::Available, None)
                }
                Some(JellyfinWebSocketEvent::ConnectionLost) => {
                    self.event(RemoteTargetState::Lost, None)
                }
                Some(JellyfinWebSocketEvent::Command(command)) => {
                    let Some(command) = translate_remote_command(command) else {
                        continue;
                    };
                    self.event(RemoteTargetState::Available, Some(command))
                }
                None => {
                    self.close();
                    self.event(RemoteTargetState::Closed, None)
                }
            };
            if event.state != RemoteTargetState::Closed
                && !self.is_command_current(event.generation)
            {
                self.close();
                return Ok(self.event(RemoteTargetState::Closed, None));
            }
            return Ok(event);
        }
    }

    fn event(&self, state: RemoteTargetState, command: Option<RemoteCommand>) -> RemoteTargetEvent {
        RemoteTargetEvent {
            state,
            command,
            generation: self.generation,
        }
    }
}

impl Drop for RemoteTarget {
    fn drop(&mut self) {
        self.close();
    }
}

impl Sdk {
    pub async fn open_remote_target(
        &self,
        token: Arc<OperationToken>,
    ) -> Result<Arc<RemoteTarget>, SdkError> {
        let inner = Arc::clone(&self.inner);
        let worker_token = Arc::clone(&token);
        self.inner
            .scoped(&token, move |client| async move {
                if !client
                    .login()
                    .connection_state()
                    .capabilities
                    .remote_control
                {
                    return Err(SdkError::InvalidInput(
                        "remote playback is unavailable for this provider".to_owned(),
                    ));
                }
                let socket = Arc::new(JellyfinWebSocket::new());
                let events = socket.take_event_receiver().ok_or(SdkError::Closed)?;
                let target = Arc::new(RemoteTarget {
                    inner,
                    token: worker_token,
                    client,
                    socket,
                    events: Mutex::new(events),
                    closed: AtomicBool::new(false),
                    cancel: CancellationToken::new(),
                    generation: NEXT_TARGET_GENERATION.fetch_add(1, Ordering::Relaxed),
                });
                let url = target.client.playback().websocket_url().map_err(|_| {
                    SdkError::Request("remote target session is unavailable".to_owned())
                })?;
                let user_agent = target.client.playback().websocket_user_agent();
                target
                    .socket
                    .connect_with_user_agent(&url, Some(&user_agent))
                    .await
                    .map_err(|_| SdkError::Request("remote target connection failed".to_owned()))?;
                finalize_remote_target(&target.client)
                    .await
                    .map_err(|error| SdkError::Request(error.to_string()))?;
                Ok(target)
            })
            .await
    }
}

/// Interprets provider commands once for Android and the desktop adapter.
pub fn translate_remote_command(command: JellyfinCommand) -> Option<RemoteCommand> {
    match command {
        JellyfinCommand::Play(request) => Some(RemoteCommand::Start {
            item_id: request.item_ids.into_iter().next()?,
            start_position_ticks: request.start_position_ticks,
            media_source_id: request.media_source_id,
            audio_stream_index: request.audio_stream_index,
            subtitle_stream_index: request.subtitle_stream_index,
        }),
        JellyfinCommand::Playstate(request) => match request.command.as_str() {
            "Pause" => Some(RemoteCommand::Pause),
            "Unpause" => Some(RemoteCommand::Resume),
            "PlayPause" => Some(RemoteCommand::TogglePause),
            "Stop" => Some(RemoteCommand::Stop),
            "Seek" => Some(RemoteCommand::Seek {
                seconds: request.seek_position_ticks?.max(0) as f64 / 10_000_000.0,
            }),
            "NextTrack" => Some(RemoteCommand::Next),
            "PreviousTrack" => Some(RemoteCommand::Previous),
            _ => None,
        },
        JellyfinCommand::GeneralCommand(request) => {
            let argument = |key: &str| {
                request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get(key))
            };
            match request.name.as_str() {
                "SetVolume" => Some(RemoteCommand::SetVolume {
                    volume: remote_volume_value(argument("Volume"))?,
                }),
                "ToggleMute" => Some(RemoteCommand::ToggleMute),
                "SetAudioStreamIndex" => Some(RemoteCommand::SetAudioTrack {
                    index: i32::try_from(remote_index_value(argument("Index"))?).ok()?,
                }),
                "SetSubtitleStreamIndex" => Some(RemoteCommand::SetSubtitleTrack {
                    index: i32::try_from(remote_index_value(argument("Index"))?).ok()?,
                }),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{test_sdk, test_session};
    use jellypilot_media_server::PlayRequest;

    fn target(sdk: &Sdk) -> (RemoteTarget, mpsc::Sender<JellyfinWebSocketEvent>) {
        let (sender, events) = mpsc::channel(4);
        (
            RemoteTarget {
                inner: Arc::clone(&sdk.inner),
                token: sdk.new_operation_token().unwrap(),
                client: sdk.active_client().unwrap(),
                socket: Arc::new(JellyfinWebSocket::new()),
                events: Mutex::new(events),
                closed: AtomicBool::new(false),
                cancel: CancellationToken::new(),
                generation: NEXT_TARGET_GENERATION.fetch_add(1, Ordering::Relaxed),
            },
            sender,
        )
    }

    fn start() -> JellyfinWebSocketEvent {
        JellyfinWebSocketEvent::Command(JellyfinCommand::Play(PlayRequest {
            item_ids: vec!["episode".into()],
            start_position_ticks: Some(200_000_000),
            play_command: "PlayNow".into(),
            media_source_id: Some("original".into()),
            audio_stream_index: Some(3),
            subtitle_stream_index: Some(-1),
        }))
    }

    #[tokio::test]
    async fn queued_start_is_revoked_by_visibility_exit_and_by_profile_handoff() {
        let (sdk, _dir) = test_sdk();
        sdk.adopt_test_session(test_session("ada", "https://media.example.test"));
        let (target, sender) = target(&sdk);
        sender.send(start()).await.unwrap();
        let issued = target.next_event().await.unwrap();
        assert_eq!(
            issued.command,
            Some(RemoteCommand::Start {
                item_id: "episode".into(),
                start_position_ticks: Some(200_000_000),
                media_source_id: Some("original".into()),
                audio_stream_index: Some(3),
                subtitle_stream_index: Some(-1)
            })
        );
        assert!(target.is_command_current(issued.generation));
        sender.send(start()).await.unwrap();
        target.close();
        assert!(
            !target.is_command_current(issued.generation),
            "already-delivered preparation may not start after visibility ends"
        );
        assert_eq!(
            target.next_event().await.unwrap().state,
            RemoteTargetState::Closed
        );

        let (target, sender) = self::target(&sdk);
        assert!(
            !target.is_command_current(issued.generation),
            "reopening in the same profile never readmits a previous target's delivered work"
        );
        sender.send(start()).await.unwrap();
        sdk.adopt_test_session(test_session("grace", "https://media.example.test"));
        let event = target.next_event().await.unwrap();
        assert_eq!(event.state, RemoteTargetState::Closed);
        assert!(
            event.command.is_none(),
            "old account's buffered start is never delivered to the new account"
        );
    }
}
