//! How far Haku goes to keep memory down, as the user configured it.
//!
//! The pool always has to make room when every slot is taken. These settings
//! govern what happens to background tabs before that: whether they keep
//! running, are frozen in their slot, or are discarded on Haku's own initiative.

use serde::{Deserialize, Serialize};
use specta::Type;

/// How eagerly one optimization is applied to background tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Policy {
    Never,
    /// Haku decides from what it can observe of the tab and the system.
    #[default]
    Smart,
    /// Every background tab, including one that is playing audio. Keeping a tab
    /// loaded is how a user exempts it.
    Always,
}

/// A named bundle of optimization settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    /// One page loaded at a time: every background tab is discarded.
    SaveMemory,
    Balanced,
    /// Background tabs stay loaded, frozen, until every slot is taken.
    Performance,
}

/// The values a preset sets. Slots and freezing are meaningless when every
/// background tab is discarded, so [`Preset::SaveMemory`] leaves them alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresetValues {
    pub slots: Option<usize>,
    pub freeze: Option<Policy>,
    pub discard: Policy,
}

const GIB: u64 = 1024 * 1024 * 1024;

/// Slot counts by installed memory, as (memory below, balanced, performance).
///
/// A starting point rather than a measurement: a typical page costs a few
/// hundred megabytes, and the operating system and other programs need most of
/// a small machine's memory.
const SLOT_TIERS: &[(u64, usize, usize)] = &[(12 * GIB, 1, 2), (24 * GIB, 2, 4)];
const LARGEST_TIER: (usize, usize) = (3, 6);

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::SaveMemory, Preset::Balanced, Preset::Performance];

    /// @param total_memory - Installed memory in bytes, when it is known.
    pub fn values(self, total_memory: Option<u64>) -> PresetValues {
        match self {
            Preset::SaveMemory => PresetValues {
                slots: None,
                freeze: None,
                discard: Policy::Always,
            },
            Preset::Balanced => PresetValues {
                slots: Some(slots_for(total_memory).0),
                freeze: Some(Policy::Smart),
                discard: Policy::Smart,
            },
            Preset::Performance => PresetValues {
                slots: Some(slots_for(total_memory).1),
                freeze: Some(Policy::Smart),
                discard: Policy::Never,
            },
        }
    }
}

/// Balanced and performance slot counts for a machine.
///
/// Unknown memory gets the smallest tier, which errs towards using less.
fn slots_for(total_memory: Option<u64>) -> (usize, usize) {
    let Some(total) = total_memory else {
        let (_, balanced, performance) = SLOT_TIERS[0];
        return (balanced, performance);
    };
    SLOT_TIERS
        .iter()
        .find(|(below, _, _)| total < *below)
        .map_or(LARGEST_TIER, |&(_, balanced, performance)| (balanced, performance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_with_little_memory_keeps_one_page_loaded_when_balanced() {
        assert_eq!(Preset::Balanced.values(Some(8 * GIB)).slots, Some(1));
    }

    #[test]
    fn more_memory_allows_more_slots() {
        assert_eq!(Preset::Balanced.values(Some(16 * GIB)).slots, Some(2));
        assert_eq!(Preset::Balanced.values(Some(64 * GIB)).slots, Some(3));
        assert_eq!(Preset::Performance.values(Some(64 * GIB)).slots, Some(6));
    }

    #[test]
    fn unknown_memory_is_treated_as_the_smallest_machine() {
        assert_eq!(Preset::Balanced.values(None).slots, Some(1));
    }

    #[test]
    fn saving_memory_discards_everything_and_leaves_the_rest_alone() {
        let values = Preset::SaveMemory.values(Some(64 * GIB));
        assert_eq!(
            values,
            PresetValues {
                slots: None,
                freeze: None,
                discard: Policy::Always
            }
        );
    }
}
