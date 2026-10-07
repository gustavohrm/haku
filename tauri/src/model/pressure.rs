//! How short of memory the machine is, by Haku's own reading.
//!
//! Not the system's `LowMemoryResourceNotification`, which fires too late to act
//! on. Both physical memory and commit are read, because running out of commit,
//! not physical memory, is what makes allocations fail and processes die.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Below this share of headroom, pressure is [`Pressure::Tight`].
pub const TIGHT_HEADROOM: f64 = 0.15;

/// Below this share of headroom, pressure is [`Pressure::Critical`].
pub const CRITICAL_HEADROOM: f64 = 0.07;

/// How far above a level's threshold headroom must climb before the level is
/// left, so a reading that hovers around a threshold does not flap.
pub const HEADROOM_HYSTERESIS: f64 = 0.03;

/// The system's memory figures, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryStatus {
    pub total_physical: u64,
    pub available_physical: u64,
    pub commit_limit: u64,
    pub commit_total: u64,
}

impl MemoryStatus {
    /// The smaller of the free shares of physical memory and of commit, from 0
    /// to 1. Zero when a total is zero, since nothing can be said to be free.
    pub fn headroom(&self) -> f64 {
        let physical = share(self.available_physical, self.total_physical);
        let commit = share(self.commit_limit.saturating_sub(self.commit_total), self.commit_limit);
        physical.min(commit)
    }
}

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    (part as f64 / whole as f64).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Pressure {
    #[default]
    Normal,
    Tight,
    Critical,
}

impl Pressure {
    /// The level a new headroom reading puts the machine at, coming from `self`.
    ///
    /// Entering a level takes headroom below its threshold; leaving it takes
    /// headroom [`HEADROOM_HYSTERESIS`] above it.
    pub fn next(self, headroom: f64) -> Pressure {
        let exit = |threshold: f64| threshold + HEADROOM_HYSTERESIS;

        if headroom < CRITICAL_HEADROOM || (self == Pressure::Critical && headroom < exit(CRITICAL_HEADROOM)) {
            return Pressure::Critical;
        }
        if headroom < TIGHT_HEADROOM || (self >= Pressure::Tight && headroom < exit(TIGHT_HEADROOM)) {
            return Pressure::Tight;
        }
        Pressure::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    fn status(available_physical: u64, commit_total: u64) -> MemoryStatus {
        MemoryStatus {
            total_physical: 16 * GB,
            available_physical,
            commit_limit: 32 * GB,
            commit_total,
        }
    }

    #[test]
    fn headroom_is_the_scarcer_of_physical_and_commit() {
        assert_eq!(status(8 * GB, 8 * GB).headroom(), 0.5);
        assert_eq!(status(8 * GB, 28 * GB).headroom(), 0.125);
    }

    #[test]
    fn headroom_is_zero_when_a_total_is_unknown() {
        assert_eq!(MemoryStatus::default().headroom(), 0.0);
    }

    #[test]
    fn headroom_is_zero_when_commit_exceeds_its_limit() {
        assert_eq!(status(8 * GB, 40 * GB).headroom(), 0.0);
    }

    #[test]
    fn levels_are_entered_below_their_thresholds() {
        assert_eq!(Pressure::Normal.next(0.15), Pressure::Normal);
        assert_eq!(Pressure::Normal.next(0.14), Pressure::Tight);
        assert_eq!(Pressure::Normal.next(0.06), Pressure::Critical);
    }

    #[test]
    fn tight_holds_through_the_hysteresis_band() {
        assert_eq!(Pressure::Tight.next(0.175), Pressure::Tight);
        assert_eq!(Pressure::Tight.next(0.185), Pressure::Normal);
    }

    #[test]
    fn critical_eases_to_tight_through_its_own_band() {
        assert_eq!(Pressure::Critical.next(0.095), Pressure::Critical);
        assert_eq!(Pressure::Critical.next(0.105), Pressure::Tight);
        assert_eq!(Pressure::Critical.next(0.20), Pressure::Normal);
    }

    #[test]
    fn critical_easing_into_the_tight_band_stays_tight() {
        assert_eq!(Pressure::Critical.next(0.16), Pressure::Tight);
    }
}
