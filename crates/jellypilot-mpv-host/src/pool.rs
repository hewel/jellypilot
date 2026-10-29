//! Slot ownership adapted from iced's published mpv_sdr_gpu reference.
//! Only completed host submissions may return a successful producer lease to Idle.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Idle,
    Reserved,
    Producer,
    Ready,
    Copying,
    Failed,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Lease {
    state: State,
    completed: u64,
}

impl Lease {
    pub fn is_idle(self) -> bool {
        self.state == State::Idle
    }

    pub fn acquire(&mut self) -> Option<(u64, u64)> {
        if !self.is_idle() || self.completed.checked_add(2).is_none() {
            return None;
        }
        self.state = State::Producer;
        Some((self.completed, self.completed + 1))
    }

    pub fn release(&mut self, success: bool) -> bool {
        if self.state != State::Producer {
            return false;
        }
        self.state = if success { State::Ready } else { State::Failed };
        true
    }

    pub fn copy(&mut self) -> Option<u64> {
        if self.state != State::Ready {
            return None;
        }
        self.state = State::Copying;
        Some(self.completed + 1)
    }

    pub fn complete(&mut self) -> bool {
        if self.state != State::Copying {
            return false;
        }
        self.completed += 2;
        self.state = State::Idle;
        true
    }

    pub fn reserve(&mut self) -> bool {
        if !self.is_idle() {
            return false;
        }
        self.state = State::Reserved;
        true
    }

    pub fn cancel_reservation(&mut self) -> bool {
        if self.state != State::Reserved {
            return false;
        }
        self.state = State::Idle;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_release_does_not_recycle_before_copy_completion() {
        let mut slot = Lease::default();
        assert_eq!(slot.acquire(), Some((0, 1)));
        assert_eq!(slot.copy(), None);
        assert!(!slot.complete());
        assert!(slot.release(true));
        assert_eq!(slot.acquire(), None);
        assert!(!slot.release(true));
        assert_eq!(slot.copy(), Some(1));
        assert_eq!(slot.copy(), None);
        assert_eq!(slot.acquire(), None);
        assert!(slot.complete());
        assert!(!slot.complete());
        assert_eq!(slot.acquire(), Some((2, 3)));
    }

    #[test]
    fn failed_release_never_waits_or_reuses_a_missing_signal() {
        let mut slot = Lease::default();
        slot.acquire().unwrap();
        assert!(slot.release(false));
        assert!(!slot.release(true));
        assert_eq!(slot.copy(), None);
        assert!(!slot.complete());
        assert_eq!(slot.acquire(), None);
        assert!(!slot.reserve());
    }

    #[test]
    fn resize_reservation_preserves_timeline_and_excludes_acquire() {
        let mut slot = Lease::default();
        slot.acquire();
        slot.release(true);
        slot.copy();
        slot.complete();
        assert!(slot.reserve());
        assert_eq!(slot.acquire(), None);
        assert!(slot.cancel_reservation());
        assert_eq!(slot.acquire(), Some((2, 3)));
    }

    #[test]
    fn timeline_exhaustion_cannot_wrap_or_leave_a_partial_lease() {
        let mut slot = Lease {
            completed: u64::MAX - 3,
            ..Lease::default()
        };
        assert_eq!(slot.acquire(), Some((u64::MAX - 3, u64::MAX - 2)));
        assert!(slot.release(true));
        assert_eq!(slot.copy(), Some(u64::MAX - 2));
        assert!(slot.complete());
        assert_eq!(slot.acquire(), None);
        assert!(slot.is_idle());
    }
}
