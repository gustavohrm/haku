use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashSet;

use super::tab::TabId;
use crate::error::{HakuError, Result};

/// Prefix for the Tauri webview label backing a pool slot.
const SLOT_LABEL_PREFIX: &str = "content-";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub enum Profile {
    #[default]
    Default,
    Private,
}

pub const DEFAULT_CAPACITY: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
pub struct SlotId(#[specta(type = specta_typescript::Number)] pub usize);

impl SlotId {
    /// Label of the Tauri webview backing this slot.
    pub fn label(self) -> String {
        format!("{SLOT_LABEL_PREFIX}{}", self.0)
    }

    /// Recovers a slot from a webview label.
    ///
    /// Signals arriving from a content webview are attributed by the label they
    /// came in on, never by anything the page claims, so a page cannot report
    /// itself as a different tab.
    pub fn from_label(label: &str) -> Option<Self> {
        label.strip_prefix(SLOT_LABEL_PREFIX)?.parse().ok().map(Self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Slot {
    pub id: SlotId,
    pub occupant: Option<TabId>,
    pub profile: Profile,
    /// Monotonic stamp of the last time this slot was claimed or focused.
    /// Only ordering matters, so a counter is used rather than a wall clock.
    pub used_at: u64,
}

/// The outcome of claiming a slot for a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acquired {
    /// The tab already had this slot; nothing else changed.
    Held(SlotId),
    Replaced {
        slot: SlotId,
        previous: SlotId,
        evicted: Option<TabId>,
    },
    /// A slot was free, or the pool grew to make one.
    Free(SlotId),
    /// A resident tab was displaced and must be discarded before reuse.
    Evicted { slot: SlotId, evicted: TabId },
}

impl Acquired {
    pub fn slot(self) -> SlotId {
        match self {
            Self::Held(slot) | Self::Free(slot) => slot,
            Self::Evicted { slot, .. } | Self::Replaced { slot, .. } => slot,
        }
    }
}

/// A fixed-size set of reusable webviews shared by every web tab.
///
/// The pool is deliberately ignorant of tabs beyond their identity. Callers
/// decide what the effective capacity is and which tab gives up its slot when
/// the pool is full, which keeps the eviction policy testable without a live
/// Tauri runtime.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct WebviewPool {
    capacity: usize,
    slots: Vec<Slot>,
    clock: u64,
    next_id: usize,
    pub keep_private: bool,
    /// A scheduled private webview cannot replace a native session anchor
    /// until creation and InPrivate verification have succeeded.
    #[serde(skip)]
    #[specta(skip)]
    pending_private: HashSet<SlotId>,
}

impl WebviewPool {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            slots: Vec::new(),
            clock: 0,
            next_id: 0,
            keep_private: false,
            pending_private: HashSet::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// How many slots the pool may hold right now.
    ///
    /// @param reserved - Slots that must exist whatever the configured
    ///   capacity: one per visible, fixed or must-run tab, so none of them can
    ///   be left without a webview.
    pub fn effective_capacity(&self, reserved: usize) -> usize {
        self.capacity.max(reserved)
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
    }

    pub fn slot_of(&self, tab: TabId) -> Option<SlotId> {
        self.slots
            .iter()
            .find(|slot| slot.occupant == Some(tab))
            .map(|slot| slot.id)
    }

    /// Marks a slot as most recently used, so eviction picks a colder one.
    pub fn touch(&mut self, tab: TabId) {
        self.clock += 1;
        let clock = self.clock;
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.occupant == Some(tab)) {
            slot.used_at = clock;
        }
    }

    /// Claims a slot for `tab`, reusing a free one, growing the pool, or
    /// taking `victim`'s, in that order.
    ///
    /// @param victim - The tab to displace if the pool is full. The caller
    ///   chooses it, since only the caller knows what each tab would lose.
    ///
    /// # Errors
    /// Returns [`HakuError::NoSlotAvailable`] when the pool is full and no
    /// victim holding a slot was named.
    pub fn acquire(
        &mut self,
        tab: TabId,
        effective_capacity: usize,
        victim: Option<TabId>,
        profile: Profile,
    ) -> Result<Acquired> {
        let held = self.slots.iter().position(|slot| slot.occupant == Some(tab));
        let free = self
            .slots
            .iter()
            .position(|slot| slot.occupant.is_none() && slot.profile == profile)
            .or_else(|| {
                self.slots
                    .iter()
                    .position(|slot| slot.occupant.is_none() && !self.is_anchor(slot.id))
            });
        let index = if let Some(index) = held.or(free) {
            index
        } else if self.slots.len() < effective_capacity {
            return Ok(Acquired::Free(self.acquire_new(tab, profile)));
        } else {
            self.slots
                .iter()
                .position(|slot| {
                    slot.occupant == victim && victim.is_some() && (slot.profile == profile || !self.is_anchor(slot.id))
                })
                .ok_or(HakuError::NoSlotAvailable)?
        };
        self.clock += 1;
        let previous = self.slots[index].clone();
        if previous.profile != profile {
            let id = self.unused_id();
            self.pending_private.remove(&previous.id);
            if profile == Profile::Private {
                self.pending_private.insert(id);
            }
            self.slots[index] = Slot {
                id,
                occupant: Some(tab),
                profile,
                used_at: self.clock,
            };
            return Ok(Acquired::Replaced {
                slot: id,
                previous: previous.id,
                evicted: previous.occupant.filter(|id| *id != tab),
            });
        }
        let slot = &mut self.slots[index];
        slot.occupant = Some(tab);
        slot.used_at = self.clock;
        Ok(if held.is_some() {
            Acquired::Held(slot.id)
        } else if let Some(evicted) = previous.occupant {
            Acquired::Evicted { slot: slot.id, evicted }
        } else {
            Acquired::Free(slot.id)
        })
    }

    /// Pending private slots cannot yet replace the final established anchor.
    pub fn is_anchor(&self, id: SlotId) -> bool {
        self.keep_private
            && self
                .slots
                .iter()
                .any(|slot| slot.id == id && slot.profile == Profile::Private)
            && (self.pending_private.contains(&id)
                || self
                    .slots
                    .iter()
                    .filter(|slot| slot.profile == Profile::Private && !self.pending_private.contains(&slot.id))
                    .count()
                    == 1)
    }

    /// Acknowledges successful native creation, including profile verification.
    pub fn slot_created(&mut self, id: SlotId) {
        self.pending_private.remove(&id);
    }

    /// Gives `tab` a slot of its own that has never held a webview, growing the
    /// pool past its capacity if need be. The caller trims the pool back.
    ///
    /// For a webview handed to a page that asked for a new window: the engine
    /// only accepts one that has never loaded anything, so neither a parked
    /// webview nor an evicted tab's will do.
    pub fn acquire_new(&mut self, tab: TabId, profile: Profile) -> SlotId {
        self.clock += 1;
        let id = self.unused_id();
        self.slots.push(Slot {
            id,
            occupant: Some(tab),
            profile,
            used_at: self.clock,
        });
        if profile == Profile::Private {
            self.pending_private.insert(id);
        }
        id
    }

    /// Labels are never reused: native destruction completes asynchronously.
    fn unused_id(&mut self) -> SlotId {
        let id = SlotId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Frees whatever slot `tab` held, if any.
    pub fn release(&mut self, tab: TabId) -> Option<SlotId> {
        let slot = self.slots.iter_mut().find(|slot| slot.occupant == Some(tab))?;
        slot.occupant = None;
        Some(slot.id)
    }

    /// Removes a slot whose webview the caller will destroy.
    ///
    /// @returns The removed slot, with the tab it displaced if it had one.
    pub fn remove(&mut self, id: SlotId) -> Option<Slot> {
        let index = self.slots.iter().position(|slot| slot.id == id)?;
        self.pending_private.remove(&id);
        Some(self.slots.remove(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_free_slot_wins_over_an_earlier_mismatched_slot() {
        for profile in [Profile::Default, Profile::Private] {
            let other = if profile == Profile::Default {
                Profile::Private
            } else {
                Profile::Default
            };
            let mut pool = WebviewPool::new(2);
            pool.acquire_new(tab(1), other);
            let matching = pool.acquire_new(tab(2), profile);
            pool.release(tab(1));
            pool.release(tab(2));

            assert_eq!(
                pool.acquire(tab(3), 2, None, profile).unwrap(),
                Acquired::Free(matching)
            );
        }
    }

    #[test]
    fn profile_mismatch_replaces_free_and_occupied_slots_in_both_directions() {
        for (from, to) in [
            (Profile::Default, Profile::Private),
            (Profile::Private, Profile::Default),
        ] {
            for release in [false, true] {
                let mut pool = WebviewPool::new(1);
                let old = pool.acquire(tab(1), 1, None, from).unwrap().slot();
                if release {
                    pool.release(tab(1));
                }
                let acquired = pool.acquire(tab(2), 1, Some(tab(1)), to).unwrap();
                assert!(matches!(acquired, Acquired::Replaced { previous, .. } if previous == old));
                assert_ne!(acquired.slot(), old, "a closing native label must not be reused");
                assert_eq!(pool.slots().len(), 1, "capacity is global");
                assert_eq!(pool.slots()[0].profile, to);
            }
        }
    }

    fn tab(id: u64) -> TabId {
        TabId(id)
    }

    #[test]
    fn a_slot_label_identifies_its_backing_webview() {
        assert_eq!(SlotId(2).label(), "content-2");
    }

    #[test]
    fn a_slot_is_recovered_from_its_webview_label() {
        assert_eq!(SlotId::from_label("content-7"), Some(SlotId(7)));
    }

    #[test]
    fn a_label_that_is_not_a_content_slot_is_rejected() {
        assert_eq!(SlotId::from_label("main"), None);
        assert_eq!(SlotId::from_label("content-abc"), None);
        assert_eq!(SlotId::from_label("content-"), None);
    }

    #[test]
    fn a_new_slot_is_never_a_free_one_and_may_exceed_capacity() {
        let mut pool = WebviewPool::new(1);
        pool.acquire(tab(1), 1, None, Profile::Default).unwrap();
        pool.release(tab(1));

        let slot = pool.acquire_new(tab(2), Profile::Default);

        assert_ne!(slot, SlotId(0));
        assert_eq!(pool.slots().len(), 2);
        assert_eq!(pool.slot_of(tab(2)), Some(slot));
    }

    #[test]
    fn capacity_never_drops_below_one_so_the_active_tab_always_renders() {
        assert_eq!(WebviewPool::new(0).capacity(), 1);

        let mut pool = WebviewPool::new(4);
        pool.set_capacity(0);
        assert_eq!(pool.capacity(), 1);
    }

    #[test]
    fn reserved_slots_raise_effective_capacity_above_the_configured_value() {
        let pool = WebviewPool::new(1);
        assert_eq!(pool.effective_capacity(1), 1);
        assert_eq!(pool.effective_capacity(4), 4);
    }

    #[test]
    fn a_configured_capacity_larger_than_the_reservation_wins() {
        let pool = WebviewPool::new(8);
        assert_eq!(pool.effective_capacity(3), 8);
    }

    #[test]
    fn the_pool_grows_lazily_up_to_the_effective_capacity() {
        let mut pool = WebviewPool::new(2);

        assert_eq!(
            pool.acquire(tab(1), 2, None, Profile::Default).unwrap(),
            Acquired::Free(SlotId(0))
        );
        assert_eq!(
            pool.acquire(tab(2), 2, None, Profile::Default).unwrap(),
            Acquired::Free(SlotId(1))
        );
        assert_eq!(pool.slots().len(), 2);
    }

    #[test]
    fn acquiring_a_slot_the_tab_already_holds_changes_nothing() {
        let mut pool = WebviewPool::new(2);
        let first = pool.acquire(tab(1), 2, None, Profile::Default).unwrap().slot();

        assert_eq!(
            pool.acquire(tab(1), 2, None, Profile::Default).unwrap(),
            Acquired::Held(first)
        );
        assert_eq!(pool.slots().len(), 1);
    }

    #[test]
    fn a_full_pool_evicts_the_named_victim() {
        let mut pool = WebviewPool::new(2);
        pool.acquire(tab(1), 2, None, Profile::Default).unwrap();
        pool.acquire(tab(2), 2, None, Profile::Default).unwrap();

        let acquired = pool.acquire(tab(3), 2, Some(tab(1)), Profile::Default).unwrap();
        assert_eq!(
            acquired,
            Acquired::Evicted {
                slot: SlotId(0),
                evicted: tab(1)
            }
        );
    }

    #[test]
    fn a_full_pool_refuses_when_no_victim_is_named() {
        let mut pool = WebviewPool::new(2);
        pool.acquire(tab(1), 2, None, Profile::Default).unwrap();
        pool.acquire(tab(2), 2, None, Profile::Default).unwrap();

        let error = pool.acquire(tab(3), 2, None, Profile::Default).unwrap_err();
        assert!(matches!(error, HakuError::NoSlotAvailable));
    }

    #[test]
    fn a_full_pool_refuses_a_victim_that_holds_no_slot() {
        let mut pool = WebviewPool::new(1);
        pool.acquire(tab(1), 1, None, Profile::Default).unwrap();

        let error = pool.acquire(tab(2), 1, Some(tab(9)), Profile::Default).unwrap_err();
        assert!(matches!(error, HakuError::NoSlotAvailable));
    }

    #[test]
    fn a_free_slot_is_used_before_any_victim() {
        let mut pool = WebviewPool::new(2);
        pool.acquire(tab(1), 2, None, Profile::Default).unwrap();
        pool.acquire(tab(2), 2, None, Profile::Default).unwrap();
        pool.release(tab(2));

        assert_eq!(
            pool.acquire(tab(3), 2, Some(tab(1)), Profile::Default).unwrap(),
            Acquired::Free(SlotId(1))
        );
    }

    #[test]
    fn a_released_slot_is_reused_before_the_pool_grows() {
        let mut pool = WebviewPool::new(3);
        pool.acquire(tab(1), 3, None, Profile::Default).unwrap();
        pool.acquire(tab(2), 3, None, Profile::Default).unwrap();
        pool.release(tab(1));

        assert_eq!(
            pool.acquire(tab(3), 3, None, Profile::Default).unwrap(),
            Acquired::Free(SlotId(0))
        );
        assert_eq!(pool.slots().len(), 2);
    }

    #[test]
    fn removing_a_slot_reports_the_tab_it_held() {
        let mut pool = WebviewPool::new(2);
        pool.acquire(tab(1), 2, None, Profile::Default).unwrap();
        pool.acquire(tab(2), 2, None, Profile::Default).unwrap();

        let removed = pool.remove(SlotId(0)).unwrap();

        assert_eq!(removed.occupant, Some(tab(1)));
        assert_eq!(pool.slot_of(tab(2)), Some(SlotId(1)));
        assert_eq!(pool.slots().len(), 1);
    }

    #[test]
    fn growing_after_a_removal_uses_a_fresh_label() {
        let mut pool = WebviewPool::new(2);
        pool.acquire(tab(1), 2, None, Profile::Default).unwrap();
        pool.acquire(tab(2), 2, None, Profile::Default).unwrap();
        pool.remove(SlotId(0));

        assert_eq!(
            pool.acquire(tab(3), 2, None, Profile::Default).unwrap(),
            Acquired::Free(SlotId(2))
        );
    }

    #[test]
    fn releasing_a_tab_that_holds_no_slot_is_harmless() {
        let mut pool = WebviewPool::new(1);
        assert!(pool.release(tab(99)).is_none());
    }
}
