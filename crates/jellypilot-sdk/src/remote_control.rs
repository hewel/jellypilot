//! Outbound, scope-bound control of server-authorized playback targets.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jellypilot_media_server::{JellyfinClient, RemoteControlRequest, RemoteSession};
use tokio::sync::{watch, Mutex as AsyncMutex, Notify};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use crate::playback::{checked_seconds_to_ticks, PlaybackStartPosition};
use crate::{OperationToken, ProfileScopeRef, Sdk, SdkError, SdkInner};

pub use jellypilot_media_server::{
    RemoteControlCapabilities, RemoteControlNowPlaying, RemoteControlTarget, RemoteControlTargetKey,
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteControllerStatus {
    Inactive,
    Loading,
    Ready,
    Failed,
    Closed,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, PartialEq)]
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

/// Server acceptance is not proof of a target's observed playback state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteControlReceipt {
    pub command_sequence: u64,
    pub server_accepted: bool,
}

struct State {
    revision: u64,
    generation: u64,
    active: bool,
    status: RemoteControllerStatus,
    sessions: Vec<RemoteSession>,
    // Kept across inactivity/failure only as a revalidation preference. A
    // public selected target always also belongs to fresh, authorized data.
    desired: Option<RemoteControlTargetKey>,
    refreshing: bool,
    pending: Option<u64>,
    sequence: u64,
    error: Option<String>,
    work: CancellationToken,
}

struct Shared {
    sdk: Arc<SdkInner>,
    token: Arc<OperationToken>,
    client: Arc<JellyfinClient>,
    state: Mutex<State>,
    changed: watch::Sender<()>,
    wake: Notify,
    closed: CancellationToken,
    io: AsyncMutex<()>,
}

/// One controller page owns this handle. It begins inactive; only a visible,
/// unlocked page may activate it. Dropping it cancels its independent worker.
pub struct RemoteController {
    shared: Arc<Shared>,
}

impl RemoteController {
    pub fn snapshot(&self) -> Result<RemoteControlSnapshot, SdkError> {
        self.shared.snapshot()
    }

    pub async fn next_snapshot(
        &self,
        after_revision: u64,
    ) -> Result<RemoteControlSnapshot, SdkError> {
        let mut changed = self.shared.changed.subscribe();
        loop {
            let snapshot = self.snapshot()?;
            if snapshot.revision != after_revision {
                return Ok(snapshot);
            }
            if snapshot.status == RemoteControllerStatus::Closed {
                return Err(SdkError::Closed);
            }
            tokio::select! {
                biased;
                () = self.shared.closed.cancelled() => {},
                () = self.shared.token.cancel.cancelled() => self.shared.check_scope(false)?,
                result = changed.changed() => result.map_err(|_| SdkError::Closed)?,
            }
        }
    }

    /// Invalidates all waiting work before changing visibility. Reactivation
    /// refreshes immediately and never replays an old command.
    pub fn set_active(&self, active: bool) -> Result<(), SdkError> {
        self.shared.check_scope(false)?;
        let mut state = self.shared.state.lock().map_err(|_| SdkError::Closed)?;
        if self.shared.closed.is_cancelled() {
            return Err(SdkError::Closed);
        }
        if state.active == active {
            return Ok(());
        }
        retire(&mut state);
        state.active = active;
        state.sessions.clear();
        state.error = None;
        state.status = if active {
            RemoteControllerStatus::Loading
        } else {
            RemoteControllerStatus::Inactive
        };
        bump(&mut state);
        drop(state);
        self.shared.publish();
        self.shared.wake.notify_one();
        Ok(())
    }

    pub fn refresh(&self) -> Result<(), SdkError> {
        self.shared.check_scope(false)?;
        let state = self.shared.state.lock().map_err(|_| SdkError::Closed)?;
        if !state.active || self.shared.closed.is_cancelled() {
            return Err(SdkError::Cancelled);
        }
        drop(state);
        self.shared.wake.notify_one();
        Ok(())
    }

    /// Accepts only a target present in the current successful discovery.
    pub fn select_target(&self, key: RemoteControlTargetKey) -> Result<(), SdkError> {
        self.shared.check_scope(true)?;
        let mut state = self.shared.state.lock().map_err(|_| SdkError::Closed)?;
        if !state.active
            || state.status != RemoteControllerStatus::Ready
            || !state
                .sessions
                .iter()
                .any(|session| session.view().key == key)
        {
            return Err(SdkError::InvalidInput(
                "the target is no longer available".into(),
            ));
        }
        if state.desired.as_ref() == Some(&key) {
            return Ok(());
        }
        retire(&mut state);
        state.desired = Some(key);
        state.error = None;
        bump(&mut state);
        drop(state);
        self.shared.publish();
        self.shared.wake.notify_one();
        Ok(())
    }

    /// Executes against the exact generation and target displayed at input
    /// time. An accepted request cannot be recalled if cancellation races it.
    pub async fn execute(
        &self,
        generation: u64,
        target: RemoteControlTargetKey,
        command: RemoteControlCommand,
    ) -> Result<RemoteControlReceipt, SdkError> {
        validate_command(&command)?;
        let work = self.shared.admit(generation, &target)?;
        let shared = Arc::clone(&self.shared);
        let guard = CommandGuard {
            shared: Arc::clone(&shared),
            generation,
        };
        let task = AbortOnDropHandle::new(self.shared.sdk.handle.spawn(async move {
            let _guard = guard;
            shared.execute(generation, target, command, work).await
        }));
        task.await.map_err(|_| SdkError::Closed)?
    }

    pub fn close(&self) {
        self.shared.close();
    }
}

impl Drop for RemoteController {
    fn drop(&mut self) {
        self.shared.close();
    }
}

fn retire(state: &mut State) {
    state.work.cancel();
    state.work = CancellationToken::new();
    state.generation = next_generation();
    state.pending = None;
    state.refreshing = false;
}

fn bump(state: &mut State) {
    state.revision = state.revision.wrapping_add(1);
}

impl Shared {
    fn publish(&self) {
        self.changed.send_replace(());
    }

    fn check_scope(&self, writing: bool) -> Result<(), SdkError> {
        let state = self.sdk.state.lock().map_err(|_| SdkError::Closed)?;
        if state.closed {
            return Err(SdkError::Closed);
        }
        if state.epoch != self.token.epoch {
            return Err(SdkError::Stale);
        }
        if self.token.is_cancelled() {
            return Err(SdkError::Cancelled);
        }
        if state.active.is_none() {
            return Err(SdkError::NoActiveProfile);
        }
        if writing && (state.handoff_in_progress || state.sign_out_cleanup_pending) {
            return Err(SdkError::OperationInProgress);
        }
        Ok(())
    }

    fn check_target(
        &self,
        generation: u64,
        target: &RemoteControlTargetKey,
    ) -> Result<(), SdkError> {
        self.check_scope(true)?;
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if self.closed.is_cancelled() || !state.active {
            return Err(SdkError::Cancelled);
        }
        if state.generation != generation || state.desired.as_ref() != Some(target) {
            return Err(SdkError::Stale);
        }
        if state.status != RemoteControllerStatus::Ready
            || !state
                .sessions
                .iter()
                .any(|session| &session.view().key == target)
        {
            return Err(SdkError::InvalidInput(
                "the target is no longer available".into(),
            ));
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<RemoteControlSnapshot, SdkError> {
        if !self.closed.is_cancelled() {
            self.check_scope(false)?;
        }
        let writable = self.check_scope(true).is_ok();
        let state = self.state.lock().map_err(|_| SdkError::Closed)?;
        let targets = state
            .sessions
            .iter()
            .map(|session| {
                let mut target = session.view().clone();
                if !writable {
                    target.capabilities = RemoteControlCapabilities::default();
                }
                target
            })
            .collect();
        Ok(RemoteControlSnapshot {
            revision: state.revision,
            scope: self.token.scope.clone(),
            generation: state.generation,
            status: state.status,
            targets,
            selected: state
                .desired
                .as_ref()
                .filter(|key| {
                    state
                        .sessions
                        .iter()
                        .any(|session| &session.view().key == *key)
                })
                .cloned(),
            refreshing: state.refreshing,
            command_pending: state.pending.is_some(),
            error: state.error.clone(),
        })
    }

    fn admit(
        &self,
        generation: u64,
        target: &RemoteControlTargetKey,
    ) -> Result<CancellationToken, SdkError> {
        self.check_target(generation, target)?;
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.generation != generation || self.closed.is_cancelled() {
            return Err(SdkError::Stale);
        }
        if state.pending.is_some() {
            return Err(SdkError::OperationInProgress);
        }
        state.pending = Some(generation);
        let work = state.work.clone();
        bump(&mut state);
        drop(state);
        self.publish();
        Ok(work)
    }

    fn close(&self) {
        if self.closed.is_cancelled() {
            return;
        }
        self.closed.cancel();
        if let Ok(mut state) = self.state.lock() {
            retire(&mut state);
            state.active = false;
            state.sessions.clear();
            state.status = RemoteControllerStatus::Closed;
            bump(&mut state);
        }
        self.publish();
    }

    async fn wait<T>(
        &self,
        work: &CancellationToken,
        future: impl Future<Output = Result<T, SdkError>>,
    ) -> Result<T, SdkError> {
        tokio::select! {
            biased;
            () = self.closed.cancelled() => Err(SdkError::Closed),
            () = self.token.cancel.cancelled() => {
                self.check_scope(false)?;
                Err(SdkError::Cancelled)
            },
            () = work.cancelled() => Err(SdkError::Cancelled),
            result = tokio::time::timeout(REQUEST_TIMEOUT, future) => result
                .map_err(|_| SdkError::Request("the remote target request timed out".into()))?,
        }
    }

    async fn discover(&self, work: &CancellationToken) -> Result<Vec<RemoteSession>, SdkError> {
        self.check_scope(false)?;
        let sessions = self
            .wait(work, async {
                self.client
                    .remote_control()
                    .sessions()
                    .await
                    .map_err(Into::into)
            })
            .await?;
        self.check_scope(false)?;
        Ok(sessions)
    }

    fn settle_refresh(
        &self,
        generation: u64,
        result: Result<Vec<RemoteSession>, SdkError>,
    ) -> Result<(), SdkError> {
        self.check_scope(false)?;
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.generation != generation || !state.active || self.closed.is_cancelled() {
            return Err(SdkError::Stale);
        }
        state.refreshing = false;
        match result {
            Ok(sessions) => {
                let missing = state
                    .desired
                    .as_ref()
                    .is_some_and(|key| !sessions.iter().any(|session| &session.view().key == key));
                let item_changed = state.desired.as_ref().is_some_and(|key| {
                    let item_id = |sessions: &[RemoteSession]| {
                        sessions
                            .iter()
                            .find(|session| &session.view().key == key)
                            .and_then(|session| {
                                session
                                    .view()
                                    .now_playing
                                    .as_ref()
                                    .map(|playing| playing.item_id.clone())
                            })
                    };
                    !state.sessions.is_empty() && item_id(&state.sessions) != item_id(&sessions)
                });
                state.sessions = sessions;
                state.status = RemoteControllerStatus::Ready;
                state.error = missing.then(|| "the selected target is no longer available".into());
                if missing || item_changed {
                    retire(&mut state);
                }
            }
            Err(error) => {
                retire(&mut state);
                state.sessions.clear();
                state.status = RemoteControllerStatus::Failed;
                state.error = Some(error.to_string());
            }
        }
        bump(&mut state);
        drop(state);
        self.publish();
        Ok(())
    }

    async fn refresh(&self, generation: u64, work: &CancellationToken) {
        let Ok(_io) = self.wait(work, async { Ok(self.io.lock().await) }).await else {
            return;
        };
        {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if !state.active || state.generation != generation {
                return;
            }
            state.refreshing = true;
            bump(&mut state);
        }
        self.publish();
        let result = self.discover(work).await;
        if work.is_cancelled() || self.closed.is_cancelled() {
            return;
        }
        let _ = self.settle_refresh(generation, result);
    }

    async fn execute(
        &self,
        generation: u64,
        target: RemoteControlTargetKey,
        command: RemoteControlCommand,
        work: CancellationToken,
    ) -> Result<RemoteControlReceipt, SdkError> {
        let _io = self.wait(&work, async { Ok(self.io.lock().await) }).await?;
        self.check_target(generation, &target)?;
        // Resolve only identity/start-position metadata, never a local
        // playback descriptor, player admission, or reporting session.
        let request = match command {
            RemoteControlCommand::PlayNow { item_id, position } => {
                let item_id = self
                    .client
                    .remote_control()
                    .normalize_item_id(&item_id)
                    .map_err(|_| SdkError::InvalidInput("invalid media identity".into()))?;
                let detail = self
                    .wait(&work, async {
                        self.client
                            .library()
                            .item_detail(item_id)
                            .await
                            .map_err(SdkError::from)
                    })
                    .await?;
                if !detail.can_play || !matches!(detail.item_type.as_str(), "Movie" | "Episode") {
                    return Err(SdkError::InvalidInput(
                        "the selected item is not a playable movie or episode".into(),
                    ));
                }
                let seconds = match position {
                    PlaybackStartPosition::Beginning => 0.0,
                    PlaybackStartPosition::Resume => {
                        if detail.can_resume {
                            detail.resume_position_seconds.unwrap_or(0.0)
                        } else {
                            0.0
                        }
                    }
                    PlaybackStartPosition::At(seconds) => seconds,
                };
                if detail
                    .runtime_seconds
                    .is_some_and(|duration| seconds > duration)
                {
                    return Err(SdkError::InvalidInput(
                        "the start position exceeds the item duration".into(),
                    ));
                }
                RemoteControlRequest::PlayNow {
                    item_id: detail.id,
                    start_position_ticks: Some(ticks(seconds)?),
                }
            }
            RemoteControlCommand::Pause => RemoteControlRequest::Pause,
            RemoteControlCommand::Resume => RemoteControlRequest::Resume,
            RemoteControlCommand::Stop => RemoteControlRequest::Stop,
            RemoteControlCommand::Seek { seconds } => RemoteControlRequest::Seek {
                position_ticks: ticks(seconds)?,
            },
            RemoteControlCommand::SetVolume { volume } => RemoteControlRequest::SetVolume {
                volume: u8::try_from(volume)
                    .map_err(|_| SdkError::InvalidInput("volume must be in 0..=100".into()))?,
            },
        };
        self.check_target(generation, &target)?;
        let refreshed = self.discover(&work).await;
        let refresh_error = refreshed.as_ref().err().cloned();
        let session = refreshed
            .as_ref()
            .ok()
            .and_then(|sessions| sessions.iter().find(|session| session.view().key == target))
            .cloned();
        self.settle_refresh(generation, refreshed)?;
        if let Some(error) = refresh_error {
            return Err(error);
        }
        self.check_target(generation, &target)?;
        let session = session
            .ok_or_else(|| SdkError::InvalidInput("the target is no longer available".into()))?;
        // The existing account-transition fence admits only the bounded POST.
        // Target refresh and metadata reads never delay credential handoff.
        let _execution = self
            .wait(&work, async {
                Ok(self.sdk.playback_execution.read().await)
            })
            .await?;
        self.check_target(generation, &target)?;
        let result = self
            .wait(&work, async {
                self.client
                    .remote_control()
                    .send(&session, request)
                    .await
                    .map_err(SdkError::from)
            })
            .await;
        if let Err(error) = result {
            if !work.is_cancelled() {
                let _ = self.settle_refresh(generation, Err(error.clone()));
            }
            return Err(error);
        }
        self.check_target(generation, &target)?;
        let mut state = self.state.lock().map_err(|_| SdkError::Closed)?;
        if state.generation != generation {
            return Err(SdkError::Stale);
        }
        state.sequence = state.sequence.wrapping_add(1);
        let receipt = RemoteControlReceipt {
            command_sequence: state.sequence,
            server_accepted: true,
        };
        drop(state);
        self.wake.notify_one();
        Ok(receipt)
    }
}

struct CommandGuard {
    shared: Arc<Shared>,
    generation: u64,
}

impl Drop for CommandGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            if state.pending == Some(self.generation) {
                state.pending = None;
                bump(&mut state);
                drop(state);
                self.shared.publish();
            }
        }
    }
}

fn ticks(seconds: f64) -> Result<i64, SdkError> {
    checked_seconds_to_ticks(seconds)
        .map_err(|_| SdkError::InvalidInput("invalid playback position".into()))
}

fn validate_command(command: &RemoteControlCommand) -> Result<(), SdkError> {
    match command {
        RemoteControlCommand::Seek { seconds } => {
            ticks(*seconds)?;
        }
        RemoteControlCommand::SetVolume { volume } if *volume > 100 => {
            return Err(SdkError::InvalidInput("volume must be in 0..=100".into()))
        }
        RemoteControlCommand::PlayNow { item_id, position } => {
            if item_id.is_empty() || item_id.len() > 128 {
                return Err(SdkError::InvalidInput("invalid media identity".into()));
            }
            if let PlaybackStartPosition::At(seconds) = position {
                ticks(*seconds)?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn run(shared: Arc<Shared>) {
    loop {
        let active = shared.state.lock().is_ok_and(|state| state.active);
        tokio::select! {
            biased;
            () = shared.closed.cancelled() => break,
            () = shared.token.cancel.cancelled() => break,
            () = shared.wake.notified() => {},
            () = tokio::time::sleep(REFRESH_INTERVAL), if active => {},
        }
        let demand = shared
            .state
            .lock()
            .ok()
            .and_then(|state| state.active.then(|| (state.generation, state.work.clone())));
        if let Some((generation, work)) = &demand {
            shared.refresh(*generation, work).await;
        }
    }
}

impl Sdk {
    /// Opens an inactive controller under the current authenticated scope.
    pub fn open_remote_controller(&self) -> Result<Arc<RemoteController>, SdkError> {
        let token = self.new_operation_token()?;
        let state = self.inner.state.lock().map_err(|_| SdkError::Closed)?;
        if state.epoch != token.epoch {
            return Err(SdkError::Stale);
        }
        let client = Arc::clone(
            &state
                .active
                .as_ref()
                .ok_or(SdkError::NoActiveProfile)?
                .client,
        );
        let (changed, _) = watch::channel(());
        let shared = Arc::new(Shared {
            sdk: Arc::clone(&self.inner),
            token,
            client,
            changed,
            state: Mutex::new(State {
                revision: 1,
                generation: next_generation(),
                active: false,
                status: RemoteControllerStatus::Inactive,
                sessions: Vec::new(),
                desired: None,
                refreshing: false,
                pending: None,
                sequence: 0,
                error: None,
                work: CancellationToken::new(),
            }),
            wake: Notify::new(),
            closed: CancellationToken::new(),
            io: AsyncMutex::new(()),
        });
        drop(state);
        // The worker owns Shared, never RemoteController. Last-handle Drop
        // always cancels it, including during a response or a 3-second wait.
        drop(self.inner.handle.spawn(run(Arc::clone(&shared))));
        Ok(Arc::new(RemoteController { shared }))
    }
}
