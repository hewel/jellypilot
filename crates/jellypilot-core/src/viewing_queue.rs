//! Temporary ordering and consumption receipts, independent of playback execution.

use std::sync::atomic::{AtomicU64, Ordering};

pub const VIEWING_QUEUE_CAPACITY: usize = 100;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QueueEntryId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueuePlacement {
    Next,
    Last,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueMove {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueEditError {
    Stale,
    Busy,
    Full,
    Missing,
    InvalidItem,
}

#[derive(Debug)]
pub struct QueueEntry<T> {
    pub id: QueueEntryId,
    pub media_id: String,
    pub value: T,
}

/// One attempt to start an entry. Releasing it permanently retires the receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueClaim {
    entry: QueueEntryId,
    attempt: u64,
}

impl QueueClaim {
    pub fn entry_id(self) -> QueueEntryId {
        self.entry
    }
}

/// A bounded upcoming list. The caller owns profile admission and payload
/// validation; this model only grants one consumption claim at a time.
pub struct ViewingQueue<T> {
    revision: u64,
    entries: Vec<QueueEntry<T>>,
    pending: Option<QueueClaim>,
}

impl<T> Default for ViewingQueue<T> {
    fn default() -> Self {
        Self {
            revision: unique_id(),
            entries: Vec::new(),
            pending: None,
        }
    }
}

impl<T> ViewingQueue<T> {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn entries(&self) -> &[QueueEntry<T>] {
        &self.entries
    }
    pub fn pending(&self) -> Option<QueueEntryId> {
        self.pending.map(QueueClaim::entry_id)
    }

    fn editable(&self, revision: u64) -> Result<(), QueueEditError> {
        if revision != self.revision {
            return Err(QueueEditError::Stale);
        }
        if self.pending.is_some() {
            return Err(QueueEditError::Busy);
        }
        Ok(())
    }

    fn index(&self, id: QueueEntryId) -> Result<usize, QueueEditError> {
        self.entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or(QueueEditError::Missing)
    }

    /// Readding the same media repositions its existing identity, including
    /// at capacity, and replaces its explicit start-position payload.
    pub fn insert(
        &mut self,
        revision: u64,
        media_id: String,
        value: T,
        placement: QueuePlacement,
    ) -> Result<QueueEntryId, QueueEditError> {
        self.editable(revision)?;
        if media_id.trim().is_empty() {
            return Err(QueueEditError::InvalidItem);
        }
        let id = if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.media_id == media_id)
        {
            self.entries.remove(index).id
        } else {
            if self.entries.len() == VIEWING_QUEUE_CAPACITY {
                return Err(QueueEditError::Full);
            }
            QueueEntryId(unique_id())
        };
        let entry = QueueEntry {
            id,
            media_id,
            value,
        };
        match placement {
            QueuePlacement::Next => self.entries.insert(0, entry),
            QueuePlacement::Last => self.entries.push(entry),
        }
        self.revision = unique_id();
        Ok(id)
    }

    pub fn move_entry(
        &mut self,
        revision: u64,
        id: QueueEntryId,
        direction: QueueMove,
    ) -> Result<(), QueueEditError> {
        self.editable(revision)?;
        let index = self.index(id)?;
        let target = match direction {
            QueueMove::Up => index.checked_sub(1),
            QueueMove::Down => index
                .checked_add(1)
                .filter(|next| *next < self.entries.len()),
        };
        if let Some(target) = target {
            self.entries.swap(index, target);
            self.revision = unique_id();
        }
        Ok(())
    }

    pub fn remove(&mut self, revision: u64, id: QueueEntryId) -> Result<(), QueueEditError> {
        self.editable(revision)?;
        let index = self.index(id)?;
        self.entries.remove(index);
        self.revision = unique_id();
        Ok(())
    }

    pub fn clear(&mut self, revision: u64) -> Result<(), QueueEditError> {
        self.editable(revision)?;
        if !self.entries.is_empty() {
            self.reset();
        }
        Ok(())
    }

    pub fn claim(&mut self, revision: u64, id: QueueEntryId) -> Result<QueueClaim, QueueEditError> {
        self.editable(revision)?;
        self.index(id)?;
        let claim = QueueClaim {
            entry: id,
            attempt: unique_id(),
        };
        self.pending = Some(claim);
        self.revision = unique_id();
        Ok(claim)
    }

    pub fn claimed(&self, claim: QueueClaim) -> Option<&T> {
        if self.pending != Some(claim) {
            return None;
        }
        self.entries
            .iter()
            .find(|entry| entry.id == claim.entry)
            .map(|entry| &entry.value)
    }

    /// Only the exact live attempt may consume an entry. Failed attempts leave
    /// the item in place; stale settlements have no effects on a newer attempt.
    pub fn settle(&mut self, claim: QueueClaim, started: bool) -> bool {
        if self.pending != Some(claim) {
            return false;
        }
        self.pending = None;
        if started {
            self.entries.retain(|entry| entry.id != claim.entry);
        }
        self.revision = unique_id();
        true
    }

    pub fn release_pending(&mut self) -> bool {
        if self.pending.take().is_none() {
            return false;
        }
        self.revision = unique_id();
        true
    }

    /// Retires a profile's entire queue, including receipts already in flight.
    pub fn reset(&mut self) {
        self.pending = None;
        self.entries.clear();
        self.revision = unique_id();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(
        queue: &mut ViewingQueue<&'static str>,
        media: &str,
        placement: QueuePlacement,
    ) -> QueueEntryId {
        queue
            .insert(queue.revision(), media.into(), "beginning", placement)
            .unwrap()
    }

    #[test]
    fn reorder_uses_stable_identity_and_old_screen_edits_cannot_change_another_row() {
        let mut queue = ViewingQueue::default();
        let first = insert(&mut queue, "movie", QueuePlacement::Last);
        let second = insert(&mut queue, "episode", QueuePlacement::Last);
        let old_revision = queue.revision();
        queue
            .move_entry(old_revision, second, QueueMove::Up)
            .unwrap();
        assert_eq!(
            queue
                .entries()
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            [second, first]
        );
        assert_eq!(
            queue.remove(old_revision, second),
            Err(QueueEditError::Stale)
        );
        queue.remove(queue.revision(), second).unwrap();
        assert_eq!(queue.entries()[0].media_id, "movie");
        let revision = queue.revision();
        queue.move_entry(revision, first, QueueMove::Down).unwrap();
        queue.move_entry(revision, first, QueueMove::Up).unwrap();
        assert_eq!(queue.revision(), revision, "boundary moves are no-ops");
    }

    #[test]
    fn duplicate_repositions_and_updates_start_choice_even_when_full() {
        let mut queue = ViewingQueue::default();
        let first = insert(&mut queue, "movie-0", QueuePlacement::Last);
        for n in 1..VIEWING_QUEUE_CAPACITY {
            insert(&mut queue, &format!("movie-{n}"), QueuePlacement::Last);
        }
        assert_eq!(
            queue.insert(
                queue.revision(),
                "extra".into(),
                "beginning",
                QueuePlacement::Next
            ),
            Err(QueueEditError::Full)
        );
        assert_eq!(
            queue.insert(
                queue.revision(),
                "movie-0".into(),
                "resume",
                QueuePlacement::Last
            ),
            Ok(first)
        );
        assert_eq!(queue.entries().last().unwrap().id, first);
        assert_eq!(queue.entries().last().unwrap().value, "resume");
        assert_eq!(queue.entries().len(), VIEWING_QUEUE_CAPACITY);
        assert_eq!(insert(&mut queue, "movie-0", QueuePlacement::Next), first);
        assert_eq!(queue.entries()[0].id, first);
    }

    #[test]
    fn pending_start_locks_edits_failure_preserves_order_and_retry_consumes_exact_entry() {
        let mut queue = ViewingQueue::default();
        let first = insert(&mut queue, "movie", QueuePlacement::Last);
        let second = insert(&mut queue, "episode", QueuePlacement::Last);
        let claim = queue.claim(queue.revision(), second).unwrap();
        assert_eq!(queue.claimed(claim), Some(&"beginning"));
        assert_eq!(
            queue.remove(queue.revision(), first),
            Err(QueueEditError::Busy)
        );
        assert_eq!(queue.clear(queue.revision()), Err(QueueEditError::Busy));
        assert!(queue.settle(claim, false));
        assert_eq!(queue.entries()[1].id, second);
        let retry = queue.claim(queue.revision(), second).unwrap();
        assert!(
            !queue.settle(claim, true),
            "old completion cannot settle retry"
        );
        assert!(queue.settle(retry, true));
        assert_eq!(queue.entries().len(), 1);
        assert_eq!(queue.entries()[0].id, first);
    }

    #[test]
    fn released_and_retired_profile_claims_never_consume_new_entries() {
        let mut queue = ViewingQueue::default();
        let first = insert(&mut queue, "same-media", QueuePlacement::Next);
        let original = queue.claim(queue.revision(), first).unwrap();
        assert!(queue.release_pending());
        let retry = queue.claim(queue.revision(), first).unwrap();
        assert!(!queue.settle(original, true));
        queue.reset();
        let next = insert(&mut queue, "same-media", QueuePlacement::Next);
        assert_ne!(first, next);
        let current = queue.claim(queue.revision(), next).unwrap();
        assert!(!queue.settle(retry, true));
        assert!(queue.settle(current, true));
        assert!(queue.entries().is_empty());
    }

    #[test]
    fn a_receipt_or_revision_from_another_queue_cannot_authorize_edits() {
        let mut first = ViewingQueue::default();
        let mut second = ViewingQueue::default();
        let a = insert(&mut first, "same", QueuePlacement::Next);
        let b = insert(&mut second, "same", QueuePlacement::Next);
        let claim = first.claim(first.revision(), a).unwrap();
        assert_eq!(
            second.remove(first.revision(), b),
            Err(QueueEditError::Stale)
        );
        assert!(!second.settle(claim, true));
        assert_eq!(second.entries().len(), 1);
    }

    #[test]
    fn stateful_property_enforces_capacity_uniqueness_and_claim_locking() {
        use std::collections::HashSet;

        use crate::prop_test::{check_stateful, Rng};

        #[derive(Clone, Debug)]
        enum Action {
            Insert {
                media_pick: u8,
                value: u32,
                next: bool,
                stale_rev: bool,
            },
            Move {
                entry_pick: usize,
                up: bool,
                stale_rev: bool,
            },
            Remove {
                entry_pick: usize,
                stale_rev: bool,
            },
            Clear {
                stale_rev: bool,
            },
            Claim {
                entry_pick: usize,
                stale_rev: bool,
            },
            SettleActive {
                started: bool,
            },
            SettleStale {
                pick: usize,
                started: bool,
            },
            ReleasePending,
            Reset,
        }

        check_stateful(
            256,
            80,
            |rng: &mut Rng| match rng.below(9) {
                0 | 1 => Action::Insert {
                    media_pick: rng.below(106) as u8,
                    value: rng.below(1000) as u32,
                    next: rng.bool(),
                    stale_rev: rng.below(6) == 0,
                },
                2 => Action::Move {
                    entry_pick: rng.below(16),
                    up: rng.bool(),
                    stale_rev: rng.below(6) == 0,
                },
                3 => Action::Remove {
                    entry_pick: rng.below(16),
                    stale_rev: rng.below(6) == 0,
                },
                4 => Action::Clear {
                    stale_rev: rng.below(6) == 0,
                },
                5 => Action::Claim {
                    entry_pick: rng.below(16),
                    stale_rev: rng.below(6) == 0,
                },
                6 => Action::SettleActive {
                    started: rng.bool(),
                },
                7 => Action::SettleStale {
                    pick: rng.below(8),
                    started: rng.bool(),
                },
                _ => {
                    if rng.bool() {
                        Action::ReleasePending
                    } else {
                        Action::Reset
                    }
                }
            },
            |actions| {
                let mut queue = ViewingQueue::<u32>::default();
                let mut past_revisions = vec![queue.revision()];
                let mut active_claim: Option<QueueClaim> = None;
                let mut retired_claims: Vec<QueueClaim> = Vec::new();

                let stale_rev = |current: u64, past: &[u64]| -> u64 {
                    past.iter()
                        .copied()
                        .find(|r| *r != current)
                        .unwrap_or_else(|| current.wrapping_add(1))
                };
                let snapshot = |q: &ViewingQueue<u32>| -> (
                    u64,
                    Option<QueueEntryId>,
                    Vec<(QueueEntryId, String, u32)>,
                ) {
                    (
                        q.revision(),
                        q.pending(),
                        q.entries()
                            .iter()
                            .map(|e| (e.id, e.media_id.clone(), e.value))
                            .collect(),
                    )
                };

                for (step, action) in actions.iter().enumerate() {
                    let cur_rev = queue.revision();
                    past_revisions.push(cur_rev);
                    let before = snapshot(&queue);

                    match action {
                        Action::Insert {
                            media_pick,
                            value,
                            next,
                            stale_rev: use_stale,
                        } => {
                            let media_id = if *media_pick >= 104 {
                                "   ".to_owned()
                            } else {
                                format!("media-{media_pick}")
                            };
                            let rev = if *use_stale {
                                stale_rev(cur_rev, &past_revisions)
                            } else {
                                cur_rev
                            };
                            let placement = if *next {
                                QueuePlacement::Next
                            } else {
                                QueuePlacement::Last
                            };
                            let res = queue.insert(rev, media_id.clone(), *value, placement);
                            if *use_stale {
                                if res != Err(QueueEditError::Stale) || snapshot(&queue) != before {
                                    return Err(format!(
                                        "step {step}: stale insert did not return Stale cleanly"
                                    ));
                                }
                            } else if before.1.is_some() {
                                if res != Err(QueueEditError::Busy) || snapshot(&queue) != before {
                                    return Err(format!(
                                        "step {step}: insert while pending did not return Busy"
                                    ));
                                }
                            } else if media_id.trim().is_empty()
                                && res != Err(QueueEditError::InvalidItem)
                            {
                                return Err(format!(
                                    "step {step}: whitespace media did not return InvalidItem"
                                ));
                            }
                        }
                        Action::Move {
                            entry_pick,
                            up,
                            stale_rev: use_stale,
                        } => {
                            if queue.entries().is_empty() {
                                continue;
                            }
                            let id = queue.entries()[*entry_pick % queue.entries().len()].id;
                            let rev = if *use_stale {
                                stale_rev(cur_rev, &past_revisions)
                            } else {
                                cur_rev
                            };
                            let dir = if *up { QueueMove::Up } else { QueueMove::Down };
                            let res = queue.move_entry(rev, id, dir);
                            if *use_stale {
                                if res != Err(QueueEditError::Stale) || snapshot(&queue) != before {
                                    return Err(format!("step {step}: stale move failed check"));
                                }
                            } else if before.1.is_some()
                                && (res != Err(QueueEditError::Busy) || snapshot(&queue) != before)
                            {
                                return Err(format!("step {step}: busy move failed check"));
                            }
                        }
                        Action::Remove {
                            entry_pick,
                            stale_rev: use_stale,
                        } => {
                            if queue.entries().is_empty() {
                                continue;
                            }
                            let id = queue.entries()[*entry_pick % queue.entries().len()].id;
                            let rev = if *use_stale {
                                stale_rev(cur_rev, &past_revisions)
                            } else {
                                cur_rev
                            };
                            let res = queue.remove(rev, id);
                            if *use_stale {
                                if res != Err(QueueEditError::Stale) || snapshot(&queue) != before {
                                    return Err(format!("step {step}: stale remove failed check"));
                                }
                            } else if before.1.is_some()
                                && (res != Err(QueueEditError::Busy) || snapshot(&queue) != before)
                            {
                                return Err(format!("step {step}: busy remove failed check"));
                            }
                        }
                        Action::Clear {
                            stale_rev: use_stale,
                        } => {
                            let rev = if *use_stale {
                                stale_rev(cur_rev, &past_revisions)
                            } else {
                                cur_rev
                            };
                            let res = queue.clear(rev);
                            if *use_stale {
                                if res != Err(QueueEditError::Stale) || snapshot(&queue) != before {
                                    return Err(format!("step {step}: stale clear failed check"));
                                }
                            } else if before.1.is_some()
                                && (res != Err(QueueEditError::Busy) || snapshot(&queue) != before)
                            {
                                return Err(format!("step {step}: busy clear failed check"));
                            }
                        }
                        Action::Claim {
                            entry_pick,
                            stale_rev: use_stale,
                        } => {
                            if queue.entries().is_empty() {
                                continue;
                            }
                            let id = queue.entries()[*entry_pick % queue.entries().len()].id;
                            let rev = if *use_stale {
                                stale_rev(cur_rev, &past_revisions)
                            } else {
                                cur_rev
                            };
                            let res = queue.claim(rev, id);
                            if *use_stale {
                                if res != Err(QueueEditError::Stale) || snapshot(&queue) != before {
                                    return Err(format!("step {step}: stale claim failed check"));
                                }
                            } else if before.1.is_some() {
                                if res != Err(QueueEditError::Busy) || snapshot(&queue) != before {
                                    return Err(format!("step {step}: busy claim failed check"));
                                }
                            } else if let Ok(claim) = res {
                                if queue.claimed(claim).is_none() || queue.pending() != Some(id) {
                                    return Err(format!("step {step}: active claim missing entry"));
                                }
                                active_claim = Some(claim);
                            }
                        }
                        Action::SettleActive { started } => {
                            if let Some(claim) = active_claim.take() {
                                if !queue.settle(claim, *started) {
                                    return Err(format!(
                                        "step {step}: active claim failed to settle"
                                    ));
                                }
                                retired_claims.push(claim);
                                if queue.pending().is_some() {
                                    return Err(format!(
                                        "step {step}: pending stayed set after settle"
                                    ));
                                }
                                let remaining: Vec<_> = queue
                                    .entries()
                                    .iter()
                                    .map(|e| (e.id, e.media_id.clone(), e.value))
                                    .collect();
                                if *started {
                                    let expected: Vec<_> = before
                                        .2
                                        .into_iter()
                                        .filter(|(id, _, _)| *id != claim.entry_id())
                                        .collect();
                                    if remaining != expected {
                                        return Err(format!(
                                            "step {step}: started settle modified wrong entries"
                                        ));
                                    }
                                } else if remaining != before.2 {
                                    return Err(format!(
                                        "step {step}: failed settle dropped entries"
                                    ));
                                }
                            }
                        }
                        Action::SettleStale { pick, started } => {
                            if retired_claims.is_empty() {
                                continue;
                            }
                            let claim = retired_claims[*pick % retired_claims.len()];
                            if queue.settle(claim, *started) || snapshot(&queue) != before {
                                return Err(format!(
                                    "step {step}: stale claim settlement mutated queue"
                                ));
                            }
                        }
                        Action::ReleasePending => {
                            let released = queue.release_pending();
                            if released != before.1.is_some() {
                                return Err(format!(
                                    "step {step}: release_pending returned wrong bool"
                                ));
                            }
                            if let Some(claim) = active_claim.take() {
                                retired_claims.push(claim);
                            }
                        }
                        Action::Reset => {
                            queue.reset();
                            if let Some(claim) = active_claim.take() {
                                retired_claims.push(claim);
                            }
                            if !queue.entries().is_empty() || queue.pending().is_some() {
                                return Err(format!("step {step}: reset left non-empty state"));
                            }
                        }
                    }

                    if queue.entries().len() > VIEWING_QUEUE_CAPACITY {
                        return Err(format!("step {step}: capacity exceeded"));
                    }
                    let mut ids = HashSet::new();
                    let mut media_ids = HashSet::new();
                    for entry in queue.entries() {
                        if !ids.insert(entry.id) {
                            return Err(format!("step {step}: duplicate QueueEntryId"));
                        }
                        if !media_ids.insert(entry.media_id.as_str()) {
                            return Err(format!("step {step}: duplicate media_id"));
                        }
                    }
                }
                Ok(())
            },
        );
    }
}
