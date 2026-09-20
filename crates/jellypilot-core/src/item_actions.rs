//! Single-owner admission and settlement for per-item write actions.
//!
//! Server writes (Favorite, Played) and device-local Watchlist writes share one
//! admission map: at most one pending write per item across every entry point,
//! while different items stay independently admissible. There is no queue,
//! retry, or optimistic state — a write is either in flight or it is not.
//!
//! The coordinator owns admission and confirmation policy, not side effects.
//! Callers execute HTTP or local-store work and report through [`Coordinator::settle`];
//! a server result is confirmed only when it matches the requested item and flag.

use std::collections::HashMap;

use jellypilot_media_server::VideoUserDataUpdate;

use crate::request_gate::SessionToken;
use crate::watchlist::{ProfileScope, WatchlistRecord};

/// Write action admitted for one item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    /// Server write: set or clear the item's favorite flag.
    Favorite(bool),
    /// Server write: mark the item played or unplayed.
    Played(bool),
    /// Local write: add the item to or remove it from the device Watchlist.
    Watchlist(bool),
}

/// Identity of one admitted write, handed back through [`Coordinator::settle`].
#[derive(Clone, Debug)]
pub struct Receipt {
    session: SessionToken,
    scope: ProfileScope,
    item_id: String,
    action: Action,
    sequence: u64,
}

impl Receipt {
    /// Session that admitted the write.
    #[must_use]
    pub const fn session(&self) -> SessionToken {
        self.session
    }

    /// Profile scope the write was admitted under.
    #[must_use]
    pub const fn scope(&self) -> &ProfileScope {
        &self.scope
    }

    #[must_use]
    pub fn item_id(&self) -> &str {
        &self.item_id
    }

    /// Action the write was admitted for.
    #[must_use]
    pub const fn action(&self) -> Action {
        self.action
    }
}

/// Completed write result reported by the caller.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// Server-confirmed user data for a Favorite or Played write.
    Server(VideoUserDataUpdate),
    /// Full local Watchlist snapshot after an add or remove.
    Watchlist {
        revision: u64,
        records: Vec<WatchlistRecord>,
    },
}

/// Why a settled write is not confirmed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The underlying HTTP or local-store operation failed.
    Request(String),
    /// The operation completed but did not confirm the requested value:
    /// wrong response kind, wrong item, or a server flag that disagrees.
    NotConfirmed,
}

/// One in-flight write; the item id lives in the map key.
#[derive(Clone, Copy, Debug)]
struct Operation {
    action: Action,
    sequence: u64,
}

/// Admission owner for per-item writes.
///
/// `set_scope` binds the coordinator to a session/profile pair; pending writes
/// survive only while that pair stays unchanged. Operation sequences never
/// reset, so a receipt replayed after a scope round trip can never match — let
/// alone clear — a newer pending write for the same item.
///
/// `settled` records the sequence of the newest settled write per item so
/// [`Coordinator::acknowledge`] can gate result delivery: a queued completion
/// is delivered at most once and only while it is still the newest settled
/// write for its item, which keeps an older completion from overwriting a
/// newer write's accepted state when deliveries are reordered. Entries are
/// dropped with the binding that produced them.
#[derive(Debug, Default)]
pub struct Coordinator {
    session: Option<SessionToken>,
    scope: Option<ProfileScope>,
    sequence: u64,
    pending: HashMap<String, Operation>,
    settled: HashMap<String, Operation>,
}

impl Coordinator {
    /// Binds the coordinator to a session/profile pair. Pending writes are
    /// dropped when the pair changes; passing `None` rejects new starts.
    pub fn set_scope(&mut self, session: SessionToken, scope: Option<ProfileScope>) {
        if self.session == Some(session) && self.scope == scope {
            return;
        }
        self.session = Some(session);
        self.scope = scope;
        self.pending.clear();
        self.settled.clear();
    }

    /// Admits a write for a trimmed, nonempty item id. Returns `None` when
    /// unscoped, already pending, or operation identities are exhausted.
    pub fn begin(&mut self, item_id: &str, action: Action) -> Option<Receipt> {
        let item_id = item_id.trim();
        let session = self.session?;
        let scope = self.scope.as_ref()?;
        if item_id.is_empty() || self.pending.contains_key(item_id) {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        let scope = scope.clone();
        let operation = Operation {
            action,
            sequence: self.sequence,
        };
        self.pending.insert(item_id.to_owned(), operation);
        Some(Receipt {
            session,
            scope,
            item_id: item_id.to_owned(),
            action,
            sequence: operation.sequence,
        })
    }

    /// Action currently pending for `item_id`, if any.
    #[must_use]
    pub fn pending(&self, item_id: &str) -> Option<Action> {
        self.pending
            .get(item_id.trim())
            .map(|operation| operation.action)
    }

    /// Settled action whose result has not yet been acknowledged.
    #[must_use]
    pub fn awaiting_delivery(&self, item_id: &str) -> Option<Action> {
        self.settled
            .get(item_id.trim())
            .map(|operation| operation.action)
    }

    /// Whether `receipt` was minted under the binding currently in force.
    ///
    /// This checks only session and scope identity, not the pending map, so a
    /// caller can reject a queued result for a write that already settled.
    #[must_use]
    pub fn is_current(&self, receipt: &Receipt) -> bool {
        self.session == Some(receipt.session) && self.scope.as_ref() == Some(&receipt.scope)
    }

    /// Releases the admission a receipt identifies without reporting a result.
    ///
    /// Returns `true` when the receipt still owned a pending write. Use this
    /// when the underlying work was abandoned before it could produce a
    /// reportable outcome; a stale, foreign, or already-settled receipt
    /// changes nothing.
    pub fn release(&mut self, receipt: &Receipt) -> bool {
        if !self.is_current(receipt) {
            return false;
        }
        let Some(operation) = self.pending.get(receipt.item_id()) else {
            return false;
        };
        if operation.sequence != receipt.sequence || operation.action != receipt.action {
            return false;
        }
        self.pending.remove(receipt.item_id());
        true
    }

    /// Acknowledges delivery of a settled write's result.
    ///
    /// Returns `true` exactly once per settled write, and only while the
    /// receipt still names the newest settled write for its item under the
    /// current binding. A queued completion that arrives after a newer write
    /// settled — or a replayed one — is rejected, so consumers applying
    /// results out of order cannot overwrite newer accepted state.
    pub fn acknowledge(&mut self, receipt: &Receipt) -> bool {
        if !self.is_current(receipt)
            || self
                .settled
                .get(receipt.item_id())
                .map(|operation| operation.sequence)
                != Some(receipt.sequence)
        {
            return false;
        }
        self.settled.remove(receipt.item_id());
        true
    }

    /// Settles the write a receipt identifies.
    ///
    /// `None` means the receipt is stale, foreign, or already settled; the
    /// current pending write — if any — is untouched. A matching receipt always
    /// releases admission: request failures surface as [`Failure::Request`],
    /// and a result that does not confirm the requested value surfaces as
    /// [`Failure::NotConfirmed`] without adopting any reported flag.
    pub fn settle(
        &mut self,
        receipt: &Receipt,
        result: Result<Outcome, String>,
    ) -> Option<Result<Outcome, Failure>> {
        if self.session != Some(receipt.session) || self.scope.as_ref() != Some(&receipt.scope) {
            return None;
        }
        let operation = self.pending.get(receipt.item_id())?;
        if operation.sequence != receipt.sequence || operation.action != receipt.action {
            return None;
        }
        self.pending.remove(receipt.item_id());
        self.settled.insert(
            receipt.item_id().to_owned(),
            Operation {
                action: receipt.action,
                sequence: receipt.sequence,
            },
        );

        let result = result.map_err(Failure::Request);
        let confirmed = match (receipt.action, &result) {
            (Action::Favorite(requested), Ok(Outcome::Server(update))) => {
                update.item_id == receipt.item_id && update.favorite == requested
            }
            (Action::Played(requested), Ok(Outcome::Server(update))) => {
                update.item_id == receipt.item_id && update.played == requested
            }
            (Action::Watchlist(_), Ok(Outcome::Watchlist { .. })) => true,
            (_, Ok(_)) => false,
            (_, Err(_)) => true,
        };
        Some(if confirmed {
            result
        } else {
            Err(Failure::NotConfirmed)
        })
    }
}

#[cfg(test)]
mod tests {
    use jellypilot_media_server::MediaServerProvider;

    use super::*;
    use crate::request_gate::RequestGate;

    fn session() -> SessionToken {
        RequestGate::default().begin_login()
    }

    fn scope(user_id: &str) -> ProfileScope {
        ProfileScope::new(
            MediaServerProvider::Jellyfin,
            "https://server.example",
            user_id,
        )
        .expect("test scope should be valid")
    }

    fn update(item_id: &str, played: bool, favorite: bool) -> Outcome {
        Outcome::Server(VideoUserDataUpdate {
            item_id: item_id.to_owned(),
            played,
            favorite,
        })
    }

    fn watchlist() -> Outcome {
        Outcome::Watchlist {
            revision: 1,
            records: Vec::new(),
        }
    }

    fn server_update(
        settled: Option<Result<Outcome, Failure>>,
        item_id: &str,
        played: bool,
        favorite: bool,
    ) -> bool {
        matches!(
            settled,
            Some(Ok(Outcome::Server(update)))
                if update.item_id == item_id && update.played == played && update.favorite == favorite
        )
    }

    #[test]
    fn pending_write_blocks_other_kinds_on_the_same_item() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");

        assert!(coordinator.begin("item-1", Action::Played(true)).is_none());
        assert!(coordinator
            .begin("item-1", Action::Watchlist(true))
            .is_none());
        assert_eq!(coordinator.pending("item-1"), Some(Action::Favorite(true)));
    }

    #[test]
    fn different_items_are_independently_admissible() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let first = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");
        let second = coordinator
            .begin("item-2", Action::Played(false))
            .expect("a different item should admit its own write");

        assert!(server_update(
            coordinator.settle(&first, Ok(update("item-1", false, true))),
            "item-1",
            false,
            true
        ));
        assert_eq!(coordinator.pending("item-2"), Some(Action::Played(false)));
        assert!(coordinator
            .begin("item-1", Action::Watchlist(true))
            .is_some());
        assert!(server_update(
            coordinator.settle(&second, Ok(update("item-2", false, false))),
            "item-2",
            false,
            false
        ));
    }

    #[test]
    fn begin_is_rejected_without_a_scope() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), None);

        assert!(coordinator
            .begin("item-1", Action::Favorite(true))
            .is_none());
    }

    #[test]
    fn scope_change_clears_pending_and_rejects_the_old_receipt() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let stale = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");

        coordinator.set_scope(session(), Some(scope("user-2")));
        assert_eq!(coordinator.pending("item-1"), None);
        assert!(coordinator
            .settle(&stale, Ok(update("item-1", false, true)))
            .is_none());
    }

    #[test]
    fn same_scope_set_scope_keeps_pending_writes() {
        let session = session();
        let scope = scope("user-1");
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session, Some(scope.clone()));
        coordinator
            .begin("item-1", Action::Played(true))
            .expect("first write should be admitted");

        coordinator.set_scope(session, Some(scope));
        assert_eq!(coordinator.pending("item-1"), Some(Action::Played(true)));
    }

    #[test]
    fn stale_receipt_cannot_clear_a_newer_write_after_scope_round_trip() {
        let session = session();
        let profile = scope("user-1");
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session, Some(profile.clone()));
        let stale = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");

        coordinator.set_scope(session, Some(scope("user-2")));
        coordinator.set_scope(session, Some(profile));
        let current = coordinator
            .begin("item-1", Action::Played(true))
            .expect("new scope should admit a fresh write");

        assert!(coordinator
            .settle(&stale, Ok(update("item-1", false, true)))
            .is_none());
        assert_eq!(coordinator.pending("item-1"), Some(Action::Played(true)));
        assert!(server_update(
            coordinator.settle(&current, Ok(update("item-1", true, false))),
            "item-1",
            true,
            false
        ));
    }

    #[test]
    fn duplicate_settle_is_rejected() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");

        assert!(server_update(
            coordinator.settle(&receipt, Ok(update("item-1", false, true))),
            "item-1",
            false,
            true
        ));
        assert!(coordinator
            .settle(&receipt, Ok(update("item-1", false, true)))
            .is_none());
    }

    #[test]
    fn replayed_receipt_cannot_clear_a_newer_write_on_the_same_item() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let earlier = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");
        assert!(server_update(
            coordinator.settle(&earlier, Ok(update("item-1", false, true))),
            "item-1",
            false,
            true
        ));
        let current = coordinator
            .begin("item-1", Action::Favorite(false))
            .expect("settled item should admit a fresh write");

        assert!(coordinator
            .settle(&earlier, Ok(update("item-1", false, true)))
            .is_none());
        assert_eq!(coordinator.pending("item-1"), Some(Action::Favorite(false)));
        assert!(server_update(
            coordinator.settle(&current, Ok(update("item-1", false, false))),
            "item-1",
            false,
            false
        ));
    }

    #[test]
    fn server_flag_mismatch_is_not_confirmed_and_releases_admission() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");
        assert!(matches!(
            coordinator.settle(&receipt, Ok(update("item-1", false, false))),
            Some(Err(Failure::NotConfirmed))
        ));
        assert_eq!(coordinator.pending("item-1"), None);
    }

    #[test]
    fn server_update_for_another_item_is_not_confirmed() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Played(true))
            .expect("first write should be admitted");
        assert!(matches!(
            coordinator.settle(&receipt, Ok(update("item-2", true, false))),
            Some(Err(Failure::NotConfirmed))
        ));
        assert_eq!(coordinator.pending("item-1"), None);
    }

    #[test]
    fn watchlist_snapshot_does_not_confirm_a_server_write() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Played(true))
            .expect("first write should be admitted");
        assert!(matches!(
            coordinator.settle(&receipt, Ok(watchlist())),
            Some(Err(Failure::NotConfirmed))
        ));
        assert_eq!(coordinator.pending("item-1"), None);
    }

    #[test]
    fn server_update_does_not_confirm_a_watchlist_write() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Watchlist(true))
            .expect("first write should be admitted");
        assert!(matches!(
            coordinator.settle(&receipt, Ok(update("item-1", false, true))),
            Some(Err(Failure::NotConfirmed))
        ));
        assert_eq!(coordinator.pending("item-1"), None);
    }

    #[test]
    fn request_failure_releases_admission() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");
        assert!(matches!(
            coordinator.settle(&receipt, Err("offline".to_owned())),
            Some(Err(Failure::Request(error))) if error == "offline"
        ));
        assert_eq!(coordinator.pending("item-1"), None);
        assert!(coordinator
            .begin("item-1", Action::Favorite(true))
            .is_some());
    }

    #[test]
    fn foreign_session_or_profile_cannot_settle_a_matching_sequence() {
        let mut gate = RequestGate::default();
        let previous = gate.begin_login();
        let current = gate.begin_login();
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(current, Some(scope("current")));
        let receipt = coordinator.begin("item-1", Action::Favorite(true)).unwrap();
        for (session, profile) in [(previous, scope("current")), (current, scope("other"))] {
            let mut foreign = Coordinator::default();
            foreign.set_scope(session, Some(profile));
            let foreign_receipt = foreign.begin("item-1", Action::Favorite(true)).unwrap();
            assert!(coordinator
                .settle(&foreign_receipt, Ok(update("item-1", false, true)))
                .is_none());
            assert_eq!(coordinator.pending("item-1"), Some(Action::Favorite(true)));
        }
        assert!(server_update(
            coordinator.settle(&receipt, Ok(update("item-1", false, true))),
            "item-1",
            false,
            true,
        ));
    }

    #[test]
    fn release_frees_admission_without_a_result() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");

        assert!(coordinator.release(&receipt));
        assert_eq!(coordinator.pending("item-1"), None);
        assert!(!coordinator.release(&receipt));
        assert!(coordinator.begin("item-1", Action::Played(true)).is_some());
    }

    #[test]
    fn release_rejects_stale_receipts() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let stale = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");

        coordinator.set_scope(session(), Some(scope("user-2")));
        assert!(!coordinator.release(&stale));
    }

    #[test]
    fn is_current_tracks_binding_not_pending_state() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let receipt = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");

        assert!(coordinator.is_current(&receipt));
        assert!(server_update(
            coordinator.settle(&receipt, Ok(update("item-1", false, true))),
            "item-1",
            false,
            true
        ));
        // A settled receipt still identifies the live binding: queued results
        // stay acceptable while a foreign binding rejects them.
        assert!(coordinator.is_current(&receipt));
        coordinator.set_scope(session(), Some(scope("user-2")));
        assert!(!coordinator.is_current(&receipt));
    }

    #[test]
    fn acknowledge_delivers_only_the_newest_settled_write_once() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let first = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");
        assert!(server_update(
            coordinator.settle(&first, Ok(update("item-1", false, true))),
            "item-1",
            false,
            true
        ));
        let second = coordinator
            .begin("item-1", Action::Played(true))
            .expect("settled item should admit a fresh write");
        assert!(server_update(
            coordinator.settle(&second, Ok(update("item-1", true, true))),
            "item-1",
            true,
            true
        ));

        // The older completion arriving after the newer one settled is stale.
        assert!(!coordinator.acknowledge(&first));
        assert!(coordinator.acknowledge(&second));
        // Delivery is acknowledged exactly once: replays are rejected.
        assert!(!coordinator.acknowledge(&second));
    }

    #[test]
    fn acknowledge_rejects_foreign_and_unsettled_receipts() {
        let mut coordinator = Coordinator::default();
        coordinator.set_scope(session(), Some(scope("user-1")));
        let pending = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("first write should be admitted");
        assert!(!coordinator.acknowledge(&pending));

        coordinator.set_scope(session(), Some(scope("user-2")));
        let foreign = coordinator
            .begin("item-1", Action::Favorite(true))
            .expect("new scope admits a fresh write");
        assert!(server_update(
            coordinator.settle(&foreign, Ok(update("item-1", false, true))),
            "item-1",
            false,
            true
        ));
        assert!(!coordinator.acknowledge(&pending));
        assert!(coordinator.acknowledge(&foreign));
    }
}
