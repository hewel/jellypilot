//! Independent, presentation-time expiry for a bounded FIFO of Undo notices.

use std::collections::VecDeque;
use std::time::Duration;

const DISPLAY_DURATION: Duration = Duration::from_secs(8);
const MAX_VISIBLE: usize = 3;

/// Independent reasons a visible notice must retain its remaining time.
#[derive(Clone, Copy, Debug)]
pub enum UndoPause {
    Hovered,
    Focused,
    Pending,
}

#[derive(Debug)]
struct Notice {
    id: u64,
    remaining: Duration,
    hovered: bool,
    focused: bool,
    pending: bool,
}

impl Notice {
    fn paused(&self) -> bool {
        self.hovered || self.focused || self.pending
    }
}

/// The caller owns each notice's text and Undo operation, keyed by its unique ID.
///
/// Call [`Self::advance`] before processing each event, then apply changes and
/// present [`Self::visible`]. Returned expired IDs must retire their operations.
/// All timestamps use one caller-owned monotonic clock origin. Newly presented
/// notices begin at the last advanced time, never at the time they were queued.
#[derive(Debug)]
pub struct UndoNoticeQueue {
    notices: VecDeque<Notice>,
    capacity: usize,
    now: Duration,
}

impl Default for UndoNoticeQueue {
    fn default() -> Self {
        Self::new(MAX_VISIBLE)
    }
}

impl UndoNoticeQueue {
    /// Zero capacity suspends presentation; capacities above three are clamped.
    pub fn new(capacity: usize) -> Self {
        Self {
            notices: VecDeque::new(),
            capacity: capacity.min(MAX_VISIBLE),
            now: Duration::ZERO,
        }
    }

    /// Accounts only for time the current visible notices actually received.
    /// Replacement notices are first presented at `now`, even after a late tick.
    pub fn advance(&mut self, now: Duration) -> Vec<u64> {
        let now = now.max(self.now);
        let elapsed = now - self.now;
        self.now = now;
        for notice in self.notices.iter_mut().take(self.capacity) {
            if !notice.paused() {
                notice.remaining = notice.remaining.saturating_sub(elapsed);
            }
        }
        let mut expired = Vec::new();
        self.notices.retain(|notice| {
            if notice.remaining.is_zero() {
                expired.push(notice.id);
                false
            } else {
                true
            }
        });
        expired
    }

    /// Appends a new opportunity without replacing or combining earlier ones.
    /// Reusing a live ID is rejected and does not restart its timer.
    pub fn push(&mut self, id: u64) -> bool {
        if self.contains(id) {
            return false;
        }
        self.notices.push_back(Notice {
            id,
            remaining: DISPLAY_DURATION,
            hovered: false,
            focused: false,
            pending: false,
        });
        true
    }

    /// Removes an opportunity after dismissal, successful Undo, or supersession.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let Some(index) = self.notices.iter().position(|notice| notice.id == id) else {
            return false;
        };
        self.notices.remove(index);
        true
    }

    /// Ends every opportunity, including those still awaiting presentation.
    pub fn clear(&mut self) -> Vec<u64> {
        self.notices.drain(..).map(|notice| notice.id).collect()
    }

    /// Requeues excess notices with their remaining time intact.
    /// Their old pointer/focus state cannot pause them when presented again.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.min(MAX_VISIBLE);
        for notice in self.notices.iter_mut().skip(self.capacity) {
            notice.hovered = false;
            notice.focused = false;
        }
    }

    /// Updates one pause reason without clearing the others.
    /// Pointer/focus events for queued or retired notices are ignored.
    pub fn set_paused(&mut self, id: u64, reason: UndoPause, paused: bool) -> bool {
        let Some((index, notice)) = self
            .notices
            .iter_mut()
            .enumerate()
            .find(|(_, notice)| notice.id == id)
        else {
            return false;
        };
        if index >= self.capacity && !matches!(reason, UndoPause::Pending) {
            return false;
        }
        match reason {
            UndoPause::Hovered => notice.hovered = paused,
            UndoPause::Focused => notice.focused = paused,
            UndoPause::Pending => notice.pending = paused,
        }
        true
    }

    pub fn contains(&self, id: u64) -> bool {
        self.notices.iter().any(|notice| notice.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.notices.is_empty()
    }

    /// IDs in presentation order. Queue order is never changed by interaction.
    pub fn visible(&self) -> impl Iterator<Item = u64> + '_ {
        self.notices
            .iter()
            .take(self.capacity)
            .map(|notice| notice.id)
    }

    /// Earliest absolute expiry on the caller's monotonic clock.
    pub fn next_expiration(&self) -> Option<Duration> {
        self.notices
            .iter()
            .take(self.capacity)
            .filter(|notice| !notice.paused())
            .map(|notice| self.now.saturating_add(notice.remaining))
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::{UndoNoticeQueue, UndoPause};
    use std::time::Duration;

    fn seconds(value: u64) -> Duration {
        Duration::from_secs(value)
    }

    #[test]
    fn queued_notices_receive_a_full_window_after_first_presentation() {
        let mut queue = UndoNoticeQueue::default();
        for id in 1..=5 {
            queue.push(id);
        }
        assert_eq!(queue.advance(seconds(20)), vec![1, 2, 3]);
        assert_eq!(queue.visible().collect::<Vec<_>>(), vec![4, 5]);
        assert!(queue.advance(seconds(27)).is_empty());
        assert_eq!(queue.advance(seconds(28)), vec![4, 5]);
    }

    #[test]
    fn pause_reasons_are_independent_and_do_not_hold_other_notices() {
        let mut queue = UndoNoticeQueue::default();
        queue.push(1);
        queue.push(2);
        queue.advance(seconds(2));
        queue.set_paused(1, UndoPause::Hovered, true);
        queue.set_paused(1, UndoPause::Focused, true);
        assert_eq!(queue.advance(seconds(8)), vec![2]);
        queue.set_paused(1, UndoPause::Hovered, false);
        assert!(queue.advance(seconds(20)).is_empty());
        queue.set_paused(1, UndoPause::Pending, true);
        queue.set_paused(1, UndoPause::Focused, false);
        assert!(queue.advance(seconds(30)).is_empty());
        queue.set_paused(1, UndoPause::Pending, false);
        assert_eq!(queue.next_expiration(), Some(seconds(36)));
        assert_eq!(queue.advance(seconds(36)), vec![1]);
    }

    #[test]
    fn resize_requeues_remaining_time_without_stale_focus() {
        let mut queue = UndoNoticeQueue::default();
        queue.push(1);
        queue.push(2);
        queue.advance(seconds(3));
        queue.set_paused(2, UndoPause::Focused, true);
        queue.set_capacity(1);
        assert_eq!(queue.advance(seconds(20)), vec![1]);
        assert_eq!(queue.next_expiration(), Some(seconds(25)));
        assert_eq!(queue.advance(seconds(25)), vec![2]);
    }

    #[test]
    fn dismissal_promotes_fifo_and_repeated_ids_never_extend_an_opportunity() {
        let mut queue = UndoNoticeQueue::new(1);
        queue.push(1);
        queue.push(2);
        queue.advance(seconds(3));
        assert!(!queue.push(1));
        assert_eq!(queue.next_expiration(), Some(seconds(8)));
        assert!(queue.dismiss(1));
        assert_eq!(queue.visible().collect::<Vec<_>>(), vec![2]);
        assert_eq!(queue.next_expiration(), Some(seconds(11)));
        assert_eq!(queue.clear(), vec![2]);
        assert!(!queue.set_paused(2, UndoPause::Focused, true));
        assert!(queue.advance(seconds(100)).is_empty());
    }

    #[test]
    fn zero_capacity_suspends_all_timers_and_preserves_pending_undo() {
        let mut queue = UndoNoticeQueue::new(1);
        queue.push(1);
        queue.advance(seconds(3));
        queue.set_paused(1, UndoPause::Pending, true);
        queue.set_capacity(0);
        assert!(!queue.set_paused(1, UndoPause::Hovered, true));
        assert!(queue.advance(seconds(20)).is_empty());
        queue.set_capacity(3);
        assert_eq!(queue.next_expiration(), None);
        queue.set_paused(1, UndoPause::Pending, false);
        assert_eq!(queue.advance(seconds(25)), vec![1]);
    }
}
