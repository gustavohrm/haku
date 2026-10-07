//! Attributing the engine's process memory to pool slots.
//!
//! WebView2 runs pages in renderer processes it shares out as it likes: one
//! process may host several slots' pages, and a page's cross-origin frames may
//! live in processes of their own. The engine lists every process with the
//! frames it hosts, and the backend reduces each frame to the main frame it
//! belongs to. What is left is decided here, so it is testable without a
//! webview.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::model::SlotId;

/// What an engine process is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProcessKind {
    Browser,
    Renderer,
    Gpu,
    Utility,
    Other,
}

/// One engine process, as the backend read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSample {
    pub pid: u32,
    pub kind: ProcessKind,
    /// Commit charge: what pressure is measured in, and what freezing does not
    /// give back. Zero when the process could not be read.
    pub private_bytes: u64,
    /// The main frames of every frame the process hosts.
    pub main_frames: Vec<u32>,
}

/// What the backend read of the engine in one pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineSnapshot {
    /// Each slot with its webview's main frame id, or nothing when that could
    /// not be read.
    pub frames: Vec<(SlotId, Option<u32>)>,
    pub processes: Vec<ProcessSample>,
}

/// A process no slot accounts for: the browser, GPU and utility processes, and
/// the interface's own renderer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnattributedProcess {
    pub pid: u32,
    pub kind: ProcessKind,
    #[specta(type = specta_typescript::Number)]
    pub bytes: u64,
}

/// One snapshot's attribution.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attribution {
    pub slots: HashMap<SlotId, u64>,
    pub unattributed: Vec<UnattributedProcess>,
}

#[derive(Clone, Debug, Default)]
struct Remembered {
    pids: Vec<u32>,
    bytes: u64,
}

/// Attributes snapshots to slots, remembering each slot's processes.
///
/// A frozen page may not be listed as an active frame while its process is
/// still there, holding its memory. A slot missing from a snapshot is therefore
/// attributed the processes it had last time, if they still exist, and keeps
/// its previous figure if they do not.
#[derive(Debug, Default)]
pub struct SlotMemoryTracker {
    remembered: HashMap<SlotId, Remembered>,
}

impl SlotMemoryTracker {
    /// @param slots - Each slot that exists, with its webview's main frame id,
    /// or nothing when that could not be read.
    /// @param processes - Every engine process in the snapshot.
    pub fn attribute(&mut self, slots: &[(SlotId, Option<u32>)], processes: &[ProcessSample]) -> Attribution {
        let listed: HashSet<u32> = processes.iter().map(|process| process.pid).collect();

        let mut pids_of: HashMap<SlotId, Vec<u32>> = HashMap::new();
        for &(slot, frame) in slots {
            let found: Vec<u32> = frame
                .map(|frame| {
                    processes
                        .iter()
                        .filter(|process| process.main_frames.contains(&frame))
                        .map(|process| process.pid)
                        .collect()
                })
                .unwrap_or_default();
            let pids = if found.is_empty() {
                self.remembered
                    .get(&slot)
                    .map(|remembered| {
                        remembered
                            .pids
                            .iter()
                            .copied()
                            .filter(|pid| listed.contains(pid))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                found
            };
            pids_of.insert(slot, pids);
        }

        // A process shared by several slots is split equally between them.
        let mut sharers: HashMap<u32, u64> = HashMap::new();
        for pid in pids_of.values().flatten() {
            *sharers.entry(*pid).or_default() += 1;
        }
        let bytes_of: HashMap<u32, u64> = processes
            .iter()
            .map(|process| (process.pid, process.private_bytes))
            .collect();

        let mut attribution = Attribution::default();
        let mut remembered = HashMap::new();
        for (slot, pids) in pids_of {
            let bytes = if pids.is_empty() {
                self.remembered.get(&slot).map_or(0, |remembered| remembered.bytes)
            } else {
                pids.iter()
                    .map(|pid| bytes_of.get(pid).copied().unwrap_or(0) / sharers[pid])
                    .sum()
            };
            attribution.slots.insert(slot, bytes);
            remembered.insert(slot, Remembered { pids, bytes });
        }
        // Slots that no longer exist are forgotten with this assignment.
        self.remembered = remembered;

        attribution.unattributed = processes
            .iter()
            .filter(|process| !sharers.contains_key(&process.pid))
            .map(|process| UnattributedProcess {
                pid: process.pid,
                kind: process.kind,
                bytes: process.private_bytes,
            })
            .collect();
        attribution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1 << 20;

    fn renderer(pid: u32, mb: u64, main_frames: &[u32]) -> ProcessSample {
        ProcessSample {
            pid,
            kind: ProcessKind::Renderer,
            private_bytes: mb * MB,
            main_frames: main_frames.to_vec(),
        }
    }

    fn browser_process(pid: u32, mb: u64) -> ProcessSample {
        ProcessSample {
            kind: ProcessKind::Browser,
            ..renderer(pid, mb, &[])
        }
    }

    #[test]
    fn a_slot_is_attributed_every_process_hosting_its_frames() {
        let mut tracker = SlotMemoryTracker::default();
        let attribution = tracker.attribute(
            &[(SlotId(0), Some(7))],
            &[renderer(10, 100, &[7]), renderer(11, 30, &[7]), renderer(12, 50, &[9])],
        );
        assert_eq!(attribution.slots[&SlotId(0)], 130 * MB);
    }

    #[test]
    fn a_process_shared_by_slots_is_split_equally() {
        let mut tracker = SlotMemoryTracker::default();
        let attribution = tracker.attribute(
            &[(SlotId(0), Some(7)), (SlotId(1), Some(8))],
            &[renderer(10, 100, &[7, 8])],
        );
        assert_eq!(attribution.slots[&SlotId(0)], 50 * MB);
        assert_eq!(attribution.slots[&SlotId(1)], 50 * MB);
    }

    #[test]
    fn processes_hosting_no_slot_are_unattributed() {
        let mut tracker = SlotMemoryTracker::default();
        let attribution = tracker.attribute(
            &[(SlotId(0), Some(7))],
            &[browser_process(1, 80), renderer(10, 100, &[7]), renderer(12, 40, &[3])],
        );
        let pids: Vec<u32> = attribution.unattributed.iter().map(|process| process.pid).collect();
        assert_eq!(pids, vec![1, 12]);
    }

    #[test]
    fn a_slot_missing_from_a_snapshot_keeps_its_processes_while_they_exist() {
        let mut tracker = SlotMemoryTracker::default();
        tracker.attribute(&[(SlotId(0), Some(7))], &[renderer(10, 100, &[7])]);

        let attribution = tracker.attribute(&[(SlotId(0), Some(7))], &[renderer(10, 60, &[])]);
        assert_eq!(attribution.slots[&SlotId(0)], 60 * MB);
        assert!(attribution.unattributed.is_empty());
    }

    #[test]
    fn a_slot_whose_processes_are_gone_keeps_its_previous_figure() {
        let mut tracker = SlotMemoryTracker::default();
        tracker.attribute(&[(SlotId(0), Some(7))], &[renderer(10, 100, &[7])]);

        let attribution = tracker.attribute(&[(SlotId(0), Some(7))], &[renderer(11, 60, &[])]);
        assert_eq!(attribution.slots[&SlotId(0)], 100 * MB);
    }

    #[test]
    fn a_slot_without_a_frame_id_falls_back_on_its_processes() {
        let mut tracker = SlotMemoryTracker::default();
        tracker.attribute(&[(SlotId(0), Some(7))], &[renderer(10, 100, &[7])]);

        let attribution = tracker.attribute(&[(SlotId(0), None)], &[renderer(10, 90, &[7])]);
        assert_eq!(attribution.slots[&SlotId(0)], 90 * MB);
    }

    #[test]
    fn a_slot_never_measured_counts_as_zero() {
        let mut tracker = SlotMemoryTracker::default();
        let attribution = tracker.attribute(&[(SlotId(0), Some(7))], &[]);
        assert_eq!(attribution.slots[&SlotId(0)], 0);
    }

    #[test]
    fn a_slot_that_no_longer_exists_is_forgotten() {
        let mut tracker = SlotMemoryTracker::default();
        tracker.attribute(&[(SlotId(0), Some(7))], &[renderer(10, 100, &[7])]);
        tracker.attribute(&[], &[]);

        let attribution = tracker.attribute(&[(SlotId(0), Some(8))], &[]);
        assert_eq!(attribution.slots[&SlotId(0)], 0);
    }
}
