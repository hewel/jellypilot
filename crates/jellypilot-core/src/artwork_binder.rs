//! Display-free artwork correlation: per-surface view epochs and slot liveness.
//!
//! Pages clear their `HashMap<ArtworkSlot, ArtworkTarget>` on `begin_view` and
//! rebind during the following render. Completions for a previous epoch are
//! [`ArtworkSettlement::Drop`]. PlayerBar slots are minted by
//! [`ArtworkBinder::bind_player_bar`] and survive every page `begin_view`.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ArtworkSlot(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ArtworkSurface {
    Home,
    Browse,
    Detail,
    PlayerBar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkSettlement {
    Apply,
    Drop,
}

#[derive(Clone, Copy)]
struct SlotRecord {
    surface: ArtworkSurface,
    epoch: u64,
}

#[derive(Default)]
pub struct ArtworkBinder {
    next_slot: u64,
    home_epoch: u64,
    browse_epoch: u64,
    detail_epoch: u64,
    player_bar_epoch: u64,
    slots: HashMap<ArtworkSlot, SlotRecord>,
}

impl ArtworkBinder {
    /// Bumps the surface's view epoch: every outstanding slot on that surface
    /// becomes stale. PlayerBar is never affected.
    pub fn begin_view(&mut self, surface: ArtworkSurface) {
        if surface == ArtworkSurface::PlayerBar {
            return;
        }
        self.bump(surface);
    }

    /// Allocates a slot on the surface's current epoch.
    pub fn bind(&mut self, surface: ArtworkSurface) -> ArtworkSlot {
        self.allocate(surface)
    }

    /// PlayerBar slots live outside the view-epoch policy. Rebinding invalidates
    /// only the previous player-bar generation (today's dedicated view counter).
    pub fn bind_player_bar(&mut self) -> ArtworkSlot {
        self.bump(ArtworkSurface::PlayerBar);
        self.allocate(ArtworkSurface::PlayerBar)
    }

    /// Settles a completion: current epoch + live slot + `session_ok` => Apply.
    pub fn settle(
        &mut self,
        slot: ArtworkSlot,
        surface: ArtworkSurface,
        session_ok: bool,
    ) -> ArtworkSettlement {
        if !session_ok {
            self.slots.remove(&slot);
            return ArtworkSettlement::Drop;
        }
        let Some(record) = self.slots.remove(&slot) else {
            return ArtworkSettlement::Drop;
        };
        if record.surface == surface && record.epoch == self.epoch(surface) {
            ArtworkSettlement::Apply
        } else {
            ArtworkSettlement::Drop
        }
    }

    /// Invalidates every surface, including PlayerBar. Used on disconnect.
    pub fn reset(&mut self) {
        self.bump(ArtworkSurface::Home);
        self.bump(ArtworkSurface::Browse);
        self.bump(ArtworkSurface::Detail);
        self.bump(ArtworkSurface::PlayerBar);
    }

    fn allocate(&mut self, surface: ArtworkSurface) -> ArtworkSlot {
        self.next_slot = self.next_slot.saturating_add(1);
        let slot = ArtworkSlot(self.next_slot);
        self.slots.insert(
            slot,
            SlotRecord {
                surface,
                epoch: self.epoch(surface),
            },
        );
        slot
    }

    fn bump(&mut self, surface: ArtworkSurface) {
        match surface {
            ArtworkSurface::Home => self.home_epoch = self.home_epoch.saturating_add(1),
            ArtworkSurface::Browse => self.browse_epoch = self.browse_epoch.saturating_add(1),
            ArtworkSurface::Detail => self.detail_epoch = self.detail_epoch.saturating_add(1),
            ArtworkSurface::PlayerBar => {
                self.player_bar_epoch = self.player_bar_epoch.saturating_add(1)
            }
        }
        let epoch = self.epoch(surface);
        self.slots
            .retain(|_, record| record.surface != surface || record.epoch == epoch);
    }

    const fn epoch(&self, surface: ArtworkSurface) -> u64 {
        match surface {
            ArtworkSurface::Home => self.home_epoch,
            ArtworkSurface::Browse => self.browse_epoch,
            ArtworkSurface::Detail => self.detail_epoch,
            ArtworkSurface::PlayerBar => self.player_bar_epoch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_bump_drops_stale_slots_on_that_surface_only() {
        let mut binder = ArtworkBinder::default();
        let home = binder.bind(ArtworkSurface::Home);
        let browse = binder.bind(ArtworkSurface::Browse);
        binder.begin_view(ArtworkSurface::Home);
        assert_eq!(
            binder.settle(home, ArtworkSurface::Home, true),
            ArtworkSettlement::Drop
        );
        assert_eq!(
            binder.settle(browse, ArtworkSurface::Browse, true),
            ArtworkSettlement::Apply
        );
    }

    #[test]
    fn player_bar_survives_any_begin_view() {
        let mut binder = ArtworkBinder::default();
        let player = binder.bind_player_bar();
        binder.begin_view(ArtworkSurface::Home);
        binder.begin_view(ArtworkSurface::Browse);
        binder.begin_view(ArtworkSurface::Detail);
        binder.begin_view(ArtworkSurface::PlayerBar);
        assert_eq!(
            binder.settle(player, ArtworkSurface::PlayerBar, true),
            ArtworkSettlement::Apply
        );
    }

    #[test]
    fn settle_after_cancel_drops() {
        let mut binder = ArtworkBinder::default();
        let slot = binder.bind(ArtworkSurface::Detail);
        binder.begin_view(ArtworkSurface::Detail);
        assert_eq!(
            binder.settle(slot, ArtworkSurface::Detail, true),
            ArtworkSettlement::Drop
        );
    }

    #[test]
    fn slot_reuse_after_rebind_applies_only_the_new_slot() {
        let mut binder = ArtworkBinder::default();
        let stale = binder.bind(ArtworkSurface::Home);
        binder.begin_view(ArtworkSurface::Home);
        let live = binder.bind(ArtworkSurface::Home);
        assert_eq!(
            binder.settle(stale, ArtworkSurface::Home, true),
            ArtworkSettlement::Drop
        );
        assert_eq!(
            binder.settle(live, ArtworkSurface::Home, true),
            ArtworkSettlement::Apply
        );
    }

    #[test]
    fn session_not_ok_drops() {
        let mut binder = ArtworkBinder::default();
        let slot = binder.bind(ArtworkSurface::Browse);
        assert_eq!(
            binder.settle(slot, ArtworkSurface::Browse, false),
            ArtworkSettlement::Drop
        );
    }

    #[test]
    fn bind_player_bar_invalidates_the_previous_player_bar_slot() {
        let mut binder = ArtworkBinder::default();
        let first = binder.bind_player_bar();
        let second = binder.bind_player_bar();
        assert_eq!(
            binder.settle(first, ArtworkSurface::PlayerBar, true),
            ArtworkSettlement::Drop
        );
        assert_eq!(
            binder.settle(second, ArtworkSurface::PlayerBar, true),
            ArtworkSettlement::Apply
        );
    }
}
