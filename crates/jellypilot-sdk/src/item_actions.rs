//! Shared User Data Action execution for every JellyPilot frontend.
//!
//! One [`ItemActions`] per process owns per-item write admission on top of
//! `jellypilot_core::item_actions::Coordinator`: at most one pending write per
//! item across server (Favorite/Played) and device-local Watchlist writes,
//! while different items stay independently admissible. The module also owns
//! request execution and confirmation — callers never settle a receipt by
//! hand — so both the [`crate::Sdk`] methods consumed through FFI and the
//! desktop frontend share one admission authority, one confirmation check,
//! and one cancellation/drop cleanup path.
//!
//! Scope binding:
//! - [`ItemActions::set_scope`] binds the executor to a stable session
//!   identity plus profile scope (desktop's `RequestGate` session). Rebinding
//!   to the same pair is a no-op so pending writes survive.
//! - [`ItemActions::reset_scope`] mints a fresh session identity on every
//!   call. The SDK uses it on each scope-epoch transition so receipts minted
//!   under an ended epoch can never settle or acknowledge against a later
//!   epoch — including when the same profile becomes active again.
//!
//! Cancellation ends the local HTTP wait, not necessarily a server-side write.
//! Watchlist adapters carry admission into the actual storage worker, so a
//! dropped waiter cannot release it early. Deferred consumers acknowledge a
//! result after projection; until then its item remains busy.

use std::future::Future;
use std::sync::{Arc, Mutex};

use jellypilot_core::item_actions::{Action, Coordinator, Failure, Outcome, Receipt};
use jellypilot_core::request_gate::{RequestGate, SessionToken};
use jellypilot_core::watchlist::{ProfileScope, WatchlistRecord};
use jellypilot_media_server::{
    JellyfinClient, VideoLibraryItem, VideoUserDataAction, VideoUserDataUpdate,
    VideoUserDataUpdateRequest,
};

use crate::SdkError;

/// Why an admitted or requested item write did not produce a confirmed result.
///
/// `Busy` and `Inactive` are admission failures: no work ran. `NotConfirmed`
/// means the operation completed but its result did not confirm the requested
/// item and flag. `Failed` carries the underlying [`SdkError`], preserving
/// `Stale`/`Cancelled`/`Closed` for consumers that distinguish them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemActionError {
    /// Another write for the same item is already in flight.
    Busy,
    /// No profile scope is bound.
    Inactive,
    /// The operation completed but did not confirm the requested value:
    /// wrong response kind, wrong item, or a server flag that disagrees.
    NotConfirmed,
    /// The underlying request or storage operation failed, or the write's
    /// scope ended before settlement.
    Failed(SdkError),
}

impl ItemActionError {
    /// Whether the outcome carries no user-facing failure.
    ///
    /// Busy and inactive admissions are silent no-ops, and a write whose
    /// scope ended or was cancelled has nothing left to report to.
    #[must_use]
    pub fn is_silent(&self) -> bool {
        match self {
            Self::Busy | Self::Inactive => true,
            Self::NotConfirmed => false,
            Self::Failed(error) => {
                matches!(
                    error,
                    SdkError::Stale | SdkError::Cancelled | SdkError::Closed
                )
            }
        }
    }
}

impl From<ItemActionError> for SdkError {
    fn from(error: ItemActionError) -> Self {
        match error {
            ItemActionError::Busy => Self::OperationInProgress,
            ItemActionError::Inactive => Self::NoActiveProfile,
            ItemActionError::NotConfirmed => {
                Self::Request("the server response did not confirm the write".to_owned())
            }
            ItemActionError::Failed(error) => error,
        }
    }
}

/// Snapshot of the device-local Watchlist after a confirmed write.
#[derive(Clone, Debug)]
pub struct WatchlistSnapshot {
    /// Storage-owner revision fencing this snapshot against older ones.
    pub revision: u64,
    /// Full record set for the write's profile scope, newest first.
    pub records: Vec<WatchlistRecord>,
    /// Whether the write changed membership.
    pub changed: bool,
}

/// Confirmed result of an item write.
#[derive(Clone, Debug)]
pub enum WriteOutcome {
    /// Server-confirmed user data for a Favorite or Played write.
    Server(VideoUserDataUpdate),
    /// Confirmed local Watchlist snapshot after an add or remove.
    Watchlist(WatchlistSnapshot),
}

/// One device-local Watchlist write admitted for an item.
#[derive(Clone, Debug)]
pub struct WatchlistWrite {
    /// Item the write targets.
    pub item_id: String,
    /// Membership change to apply.
    pub action: WatchlistWriteAction,
}

/// Membership change a [`WatchlistWrite`] applies.
#[derive(Clone, Debug)]
pub enum WatchlistWriteAction {
    /// Add the item, storing its presentation data so an unavailable item
    /// stays identifiable.
    Add(Box<VideoLibraryItem>),
    /// Remove the item.
    Remove,
}

/// Platform storage seam for Watchlist writes.
///
/// Move `admission` into the actual I/O worker and call
/// [`Admission::finish_watchlist`] there, on success or failure. Holding it
/// only in a cancellable async waiter would release admission before a
/// detached blocking write finishes.
pub trait WatchlistStorage {
    fn apply(
        &self,
        admission: Admission,
        write: WatchlistWrite,
    ) -> impl Future<Output = Result<WatchlistSnapshot, ItemActionError>> + Send;
}

#[derive(Debug, Default)]
struct ExecutorState {
    coordinator: Coordinator,
    /// Mints a fresh session identity for every [`ItemActions::reset_scope`]
    /// call. `RequestGate::disconnect` advances the session counter, so two
    /// resets can never mint equal tokens — including when the same profile
    /// scope returns.
    gate: RequestGate,
}

/// Admission owner and executor for per-item User Data Action writes.
///
/// Cloneable and `Send + Sync`; every clone shares the same admission map.
/// Desktop keeps one in its kernel and binds it with [`Self::set_scope`];
/// [`crate::Sdk`] keeps one and rebinds it with [`Self::reset_scope`] on every
/// scope-epoch transition.
#[derive(Clone, Debug, Default)]
pub struct ItemActions {
    state: Arc<Mutex<ExecutorState>>,
}

impl ItemActions {
    /// Binds the executor to a session/profile pair.
    ///
    /// Rebinding to the identical pair is a no-op so pending writes survive;
    /// any other pair drops them. Desktop passes its `RequestGate` session so
    /// a reconnect under the same profile still invalidates queued receipts.
    pub fn set_scope(&self, session: SessionToken, scope: Option<ProfileScope>) {
        if let Ok(mut state) = self.state.lock() {
            state.coordinator.set_scope(session, scope);
        }
    }

    /// Rebinds the executor under a fresh session identity.
    ///
    /// Every call mints a new session token, so receipts minted before the
    /// call can never settle or acknowledge afterward — even when `scope`
    /// names the same profile as before. The SDK calls this on each
    /// scope-epoch transition.
    pub fn reset_scope(&self, scope: Option<ProfileScope>) {
        if let Ok(mut state) = self.state.lock() {
            state.gate.disconnect();
            let session = state.gate.current_session();
            state.coordinator.set_scope(session, scope);
        }
    }

    /// Action currently pending for `item_id`, if any.
    #[must_use]
    pub fn pending(&self, item_id: &str) -> Option<Action> {
        self.state.lock().ok().and_then(|state| {
            state
                .coordinator
                .pending(item_id)
                .or_else(|| state.coordinator.awaiting_delivery(item_id))
        })
    }

    /// Whether `receipt` was minted under the binding currently in force.
    ///
    /// Consumers holding a receipt across a queued delivery use this to
    /// reject results whose issuing session/scope pair is no longer bound.
    #[must_use]
    pub fn is_current(&self, receipt: &Receipt) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.coordinator.is_current(receipt))
    }

    /// Acknowledges delivery of a settled write's result.
    ///
    /// Returns `true` exactly once per settled write, and only while the
    /// receipt still names the newest settled write for its item under the
    /// current binding. Reordered or replayed deliveries are rejected.
    pub fn acknowledge(&self, receipt: &Receipt) -> bool {
        self.state
            .lock()
            .is_ok_and(|mut state| state.coordinator.acknowledge(receipt))
    }

    /// Admits a write until execution and result delivery finish.
    ///
    /// Dropping an unfinished admission releases its slot. A settled result
    /// remains busy until [`Self::acknowledge`] accepts its receipt.
    pub fn begin(&self, item_id: &str, action: Action) -> Result<Admission, ItemActionError> {
        let mut state = self.state.lock().map_err(|_| ItemActionError::Inactive)?;
        if item_id.trim().is_empty() {
            return Err(ItemActionError::Failed(SdkError::InvalidInput(
                "an item id is required".to_owned(),
            )));
        }
        if state.coordinator.awaiting_delivery(item_id).is_some() {
            return Err(ItemActionError::Busy);
        }
        match state.coordinator.begin(item_id, action) {
            Some(receipt) => Ok(Admission {
                state: Arc::clone(&self.state),
                receipt: Some(receipt),
                immediate: false,
            }),
            None if state.coordinator.pending(item_id).is_some() => Err(ItemActionError::Busy),
            None => Err(ItemActionError::Inactive),
        }
    }

    /// Executes an admitted server write and confirms the response.
    ///
    /// The request is derived from the admitted action, so the wire request
    /// can never disagree with what admission recorded. The returned future
    /// owns the admission: dropping it abandons the in-flight request and
    /// releases admission. A response that does not confirm the requested
    /// item and flag fails with [`ItemActionError::NotConfirmed`]; a receipt
    /// whose binding ended fails with `Failed(SdkError::Stale)`.
    pub fn run_server(
        &self,
        admission: Admission,
        client: Arc<JellyfinClient>,
    ) -> impl Future<Output = Result<VideoUserDataUpdate, ItemActionError>> + Send + 'static {
        let executor = self.clone();
        async move {
            if !executor.is_current(admission.receipt()) {
                return Err(ItemActionError::Failed(SdkError::Stale));
            }
            let request_action = match admission.action() {
                Action::Favorite(true) => VideoUserDataAction::Favorite,
                Action::Favorite(false) => VideoUserDataAction::Unfavorite,
                Action::Played(true) => VideoUserDataAction::MarkPlayed,
                Action::Played(false) => VideoUserDataAction::MarkUnplayed,
                Action::Watchlist(_) => {
                    return Err(ItemActionError::Failed(SdkError::InvalidInput(
                        "a watchlist admission cannot run as a server write".to_owned(),
                    )));
                }
            };
            let request = VideoUserDataUpdateRequest {
                item_id: admission.item_id().to_owned(),
                action: request_action,
            };
            let result = client
                .library()
                .update_user_data(request)
                .await
                .map(Outcome::Server)
                .map_err(|error| error.to_string());
            match admission.settle(result) {
                Ok(Outcome::Server(update)) => Ok(update),
                Ok(Outcome::Watchlist { .. }) => Err(ItemActionError::NotConfirmed),
                Err(error) => Err(error),
            }
        }
    }

    /// Validates an admitted Watchlist write before handing it to storage.
    pub fn run_watchlist<S: WatchlistStorage + Send + 'static>(
        &self,
        admission: Admission,
        storage: S,
        write: WatchlistWrite,
    ) -> impl Future<Output = Result<WatchlistSnapshot, ItemActionError>> + Send + 'static {
        let executor = self.clone();
        async move {
            if !executor.is_current(admission.receipt()) {
                return Err(ItemActionError::Failed(SdkError::Stale));
            }
            let (expected, actual_id) = match &write.action {
                WatchlistWriteAction::Add(item) => (Action::Watchlist(true), item.id.trim()),
                WatchlistWriteAction::Remove => (Action::Watchlist(false), write.item_id.trim()),
            };
            if admission.action() != expected
                || admission.item_id() != write.item_id.trim()
                || admission.item_id() != actual_id
            {
                return Err(ItemActionError::Failed(SdkError::InvalidInput(
                    "the watchlist write does not match its admission".to_owned(),
                )));
            }
            storage.apply(admission, write).await
        }
    }
}

/// Maps a wire-level user data action to its admission action.
#[must_use]
pub const fn server_action(action: VideoUserDataAction) -> Action {
    match action {
        VideoUserDataAction::Favorite => Action::Favorite(true),
        VideoUserDataAction::Unfavorite => Action::Favorite(false),
        VideoUserDataAction::MarkPlayed => Action::Played(true),
        VideoUserDataAction::MarkUnplayed => Action::Played(false),
    }
}

/// Ownership of one admitted write.
///
/// Dropping an admission without settling releases the pending slot. This is
/// the cancellation path: it is only safe because the owning future or task
/// drops the admission exactly when the underlying work ends.
#[derive(Debug)]
pub struct Admission {
    state: Arc<Mutex<ExecutorState>>,
    receipt: Option<Receipt>,
    immediate: bool,
}

impl Admission {
    pub(crate) fn immediate(mut self) -> Self {
        self.immediate = true;
        self
    }

    /// Identity of the admitted write.
    #[must_use]
    pub fn receipt(&self) -> &Receipt {
        self.receipt.as_ref().expect("receipt present until settle")
    }

    /// Item the write targets.
    #[must_use]
    pub fn item_id(&self) -> &str {
        self.receipt().item_id()
    }

    /// Action the write was admitted for.
    #[must_use]
    pub fn action(&self) -> Action {
        self.receipt().action()
    }

    /// Profile scope the write was admitted under.
    #[must_use]
    pub fn scope(&self) -> &ProfileScope {
        self.receipt().scope()
    }

    /// Reports the operation's result and confirms it against the request.
    ///
    /// A receipt whose binding ended reports `Failed(SdkError::Stale)`; a
    /// result that does not confirm the requested value reports
    /// [`ItemActionError::NotConfirmed`]. A matching receipt always releases
    /// admission.
    fn settle(mut self, result: Result<Outcome, String>) -> Result<Outcome, ItemActionError> {
        let receipt = self.take();
        let mut state = self
            .state
            .lock()
            .map_err(|_| ItemActionError::Failed(SdkError::Closed))?;
        let settled = state.coordinator.settle(&receipt, result);
        if self.immediate {
            state.coordinator.acknowledge(&receipt);
        }
        match settled {
            Some(Ok(outcome)) => Ok(outcome),
            Some(Err(Failure::Request(error))) => {
                Err(ItemActionError::Failed(SdkError::Request(error)))
            }
            Some(Err(Failure::NotConfirmed)) => Err(ItemActionError::NotConfirmed),
            None => Err(ItemActionError::Failed(SdkError::Stale)),
        }
    }

    /// Completes the actual Watchlist I/O, preserving typed storage errors.
    pub fn finish_watchlist(
        self,
        result: Result<WatchlistSnapshot, SdkError>,
    ) -> Result<WatchlistSnapshot, ItemActionError> {
        let (changed, error, outcome) = match result {
            Ok(snapshot) => (
                snapshot.changed,
                None,
                Ok(Outcome::Watchlist {
                    revision: snapshot.revision,
                    records: snapshot.records,
                }),
            ),
            Err(error) => (false, Some(error.clone()), Err(error.to_string())),
        };
        match (self.settle(outcome), error) {
            (Ok(Outcome::Watchlist { revision, records }), _) => Ok(WatchlistSnapshot {
                revision,
                records,
                changed,
            }),
            (Ok(Outcome::Server(_)), _) => Err(ItemActionError::NotConfirmed),
            (Err(ItemActionError::Failed(SdkError::Request(_))), Some(error)) => {
                Err(ItemActionError::Failed(error))
            }
            (Err(error), _) => Err(error),
        }
    }

    fn take(&mut self) -> Receipt {
        self.receipt.take().expect("receipt present until settle")
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        let Some(receipt) = self.receipt.take() else {
            return;
        };
        if let Ok(mut state) = self.state.lock() {
            state.coordinator.release(&receipt);
        }
    }
}
