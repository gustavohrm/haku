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
    /// More background tabs kept, and more memory for them.
    Performance,
}

/// The values a preset sets. Slots, the budget and freezing are meaningless
/// when every background tab is discarded, so [`Preset::SaveMemory`] leaves
/// them alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresetValues {
    pub slots: Option<usize>,
    pub kept_memory_mb: Option<u32>,
    pub freeze: Option<Policy>,
    pub discard: Policy,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::SaveMemory, Preset::Balanced, Preset::Performance];

    /// The same on every machine: what kept tabs may hold is an absolute
    /// amount, not a share of what is installed.
    pub fn values(self) -> PresetValues {
        match self {
            Preset::SaveMemory => PresetValues {
                slots: None,
                kept_memory_mb: None,
                freeze: None,
                discard: Policy::Always,
            },
            Preset::Balanced => PresetValues {
                slots: Some(2),
                kept_memory_mb: Some(512),
                freeze: Some(Policy::Smart),
                discard: Policy::Smart,
            },
            Preset::Performance => PresetValues {
                slots: Some(4),
                kept_memory_mb: Some(1536),
                freeze: Some(Policy::Smart),
                discard: Policy::Smart,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performance_keeps_more_than_balanced_and_still_discards_smartly() {
        let balanced = Preset::Balanced.values();
        let performance = Preset::Performance.values();
        assert!(performance.slots > balanced.slots);
        assert!(performance.kept_memory_mb > balanced.kept_memory_mb);
        assert_eq!(performance.discard, Policy::Smart);
    }

    #[test]
    fn saving_memory_discards_everything_and_leaves_the_rest_alone() {
        assert_eq!(
            Preset::SaveMemory.values(),
            PresetValues {
                slots: None,
                kept_memory_mb: None,
                freeze: None,
                discard: Policy::Always
            }
        );
    }
}
