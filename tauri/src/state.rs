use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::browser::Browser;
use crate::chrome::Layout;
use crate::error::{HakuError, Result};
use crate::model::{Loss, LossSignal, MemoryStatus, PageRecord, Pressure, Slot, SlotId, Tab, TabId};
use crate::platform::memory::{Attribution, UnattributedProcess};
use crate::storage::{HistoryDb, Paths, Session, Settings};
use crate::webview::Viewport;

/// Milliseconds since the Unix epoch.
///
/// Used for history timestamps and for when a tab was last shown.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

/// The last tick's reading of memory.
#[derive(Clone, Debug, Default)]
pub struct MemoryReading {
    pub pressure: Pressure,
    /// Nothing when the system's figures could not be read.
    pub headroom: Option<f64>,
    pub attribution: Attribution,
}

impl MemoryReading {
    /// Takes a new reading, carrying the pressure level and, when the engine
    /// could not be reached, the last attribution over from this one.
    pub fn next(&self, status: Option<MemoryStatus>, attribution: Option<Attribution>) -> MemoryReading {
        let headroom = status.map(|status| status.headroom());
        MemoryReading {
            pressure: headroom.map_or(Pressure::Normal, |headroom| self.pressure.next(headroom)),
            headroom,
            attribution: attribution.unwrap_or_else(|| self.attribution.clone()),
        }
    }

    /// The reading laid out against the pool's slots and their tabs as they
    /// are now.
    pub fn report(&self, slots: &[Slot], tabs: &[Tab]) -> MemoryReport {
        MemoryReport {
            pressure: self.pressure,
            headroom: self.headroom,
            slots: slots
                .iter()
                .map(|slot| {
                    let page = slot
                        .occupant
                        .and_then(|occupant| tabs.iter().find(|tab| tab.id == occupant))
                        .map(|tab| &tab.page);
                    SlotMemory {
                        slot: slot.id,
                        tab: slot.occupant,
                        bytes: self.attribution.slots.get(&slot.id).copied(),
                        loss: page.map(PageRecord::loss),
                        signals: page.map(PageRecord::signals).unwrap_or_default(),
                    }
                })
                .collect(),
            unattributed: self.attribution.unattributed.clone(),
        }
    }
}

/// What `haku://memory` shows.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReport {
    pub pressure: Pressure,
    /// The scarcer of free physical memory and free commit, from 0 to 1.
    pub headroom: Option<f64>,
    pub slots: Vec<SlotMemory>,
    pub unattributed: Vec<UnattributedProcess>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SlotMemory {
    pub slot: SlotId,
    /// Nothing while the slot is parked.
    pub tab: Option<TabId>,
    /// Commit charge in bytes, or nothing when the slot has not been measured.
    #[specta(type = Option<specta_typescript::Number>)]
    pub bytes: Option<u64>,
    /// What discarding the tab would cost. Nothing while the slot is parked.
    pub loss: Option<Loss>,
    /// The reasons behind `loss`.
    pub signals: Vec<LossSignal>,
}

/// Everything the command layer needs, behind the locks that make it shareable.
///
/// The browser is the only mutable authority over tabs. Commands take the write
/// lock, mutate, collect effects, and release it before touching webviews, so a
/// slow native call never blocks the next command.
pub struct AppState {
    pub browser: RwLock<Browser>,
    pub settings: RwLock<Settings>,
    pub history: Mutex<HistoryDb>,
    pub layout: RwLock<Layout>,
    /// The last rectangle page content was given.
    ///
    /// An internal page reports no viewport, because the chrome covers the whole
    /// window while one is open. Content webviews still belong at the last real
    /// rectangle, so remembering it keeps a webview created while an internal
    /// page is showing from being built at zero size and staying invisible.
    last_viewport: RwLock<Viewport>,
    pub memory: RwLock<MemoryReading>,
    pub paths: Paths,
}

impl AppState {
    pub fn new(browser: Browser, settings: Settings, history: HistoryDb, paths: Paths) -> Self {
        Self {
            browser: RwLock::new(browser),
            settings: RwLock::new(settings),
            history: Mutex::new(history),
            layout: RwLock::new(Layout::default()),
            last_viewport: RwLock::new(Viewport::default()),
            memory: RwLock::new(MemoryReading::default()),
            paths,
        }
    }

    /// Where content webviews belong.
    ///
    /// Zero until the interface has reported a layout, which keeps a webview
    /// from being created at a meaningless position during startup.
    pub fn viewport(&self) -> Viewport {
        self.last_viewport.read().map(|viewport| *viewport).unwrap_or_default()
    }

    /// Records a reported layout, keeping the last real viewport.
    pub fn set_layout(&self, layout: Layout) {
        if let Some(viewport) = layout.viewport {
            if let Ok(mut last) = self.last_viewport.write() {
                *last = viewport;
            }
        }
        if let Ok(mut current) = self.layout.write() {
            *current = layout;
        }
    }

    /// Persists the open tabs so the next launch restores them.
    ///
    /// # Errors
    /// Returns [`HakuError::Storage`] when the session file cannot be written.
    pub fn save_session(&self) -> Result<()> {
        let browser = self
            .browser
            .read()
            .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
        crate::storage::write_json(&self.paths.session, &Session::capture(&browser))
    }

    /// # Errors
    /// Returns [`HakuError::Storage`] when the settings file cannot be written.
    pub fn save_settings(&self) -> Result<()> {
        let settings = self
            .settings
            .read()
            .map_err(|_| HakuError::Storage("settings lock poisoned".into()))?;
        crate::storage::write_json(&self.paths.settings, &*settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(available_share: f64) -> MemoryStatus {
        let total = 1_000_000;
        MemoryStatus {
            total_physical: total,
            available_physical: (total as f64 * available_share) as u64,
            commit_limit: total,
            commit_total: 0,
        }
    }

    fn attribution(slot: SlotId, bytes: u64) -> Attribution {
        Attribution {
            slots: [(slot, bytes)].into(),
            unattributed: Vec::new(),
        }
    }

    #[test]
    fn a_reading_carries_the_pressure_level_through_hysteresis() {
        let tight = MemoryReading::default().next(Some(status(0.10)), None);
        assert_eq!(tight.pressure, Pressure::Tight);
        assert_eq!(tight.next(Some(status(0.16)), None).pressure, Pressure::Tight);
    }

    #[test]
    fn an_unreadable_system_reads_as_normal_without_headroom() {
        let tight = MemoryReading::default().next(Some(status(0.10)), None);
        let reading = tight.next(None, None);
        assert_eq!(reading.pressure, Pressure::Normal);
        assert_eq!(reading.headroom, None);
    }

    #[test]
    fn an_unreachable_engine_keeps_the_last_attribution() {
        let first = MemoryReading::default().next(None, Some(attribution(SlotId(0), 42)));
        assert_eq!(first.next(None, None).attribution, attribution(SlotId(0), 42));
    }

    #[test]
    fn the_report_follows_the_slots_as_they_are_now() {
        let reading = MemoryReading::default().next(None, Some(attribution(SlotId(0), 42)));
        let slots = [
            Slot {
                id: SlotId(0),
                occupant: Some(TabId(3)),
                used_at: 0,
            },
            Slot {
                id: SlotId(1),
                occupant: None,
                used_at: 0,
            },
        ];
        let mut tab = Tab::new(TabId(3), "https://a.test/");
        tab.page.unreadable = true;
        let report = reading.report(&slots, &[tab]);
        assert_eq!(report.slots[0].tab, Some(TabId(3)));
        assert_eq!(report.slots[0].bytes, Some(42));
        assert_eq!(report.slots[0].loss, Some(Loss::State));
        assert_eq!(report.slots[0].signals, vec![LossSignal::Unreadable]);
        assert_eq!(report.slots[1].bytes, None);
        assert_eq!(report.slots[1].loss, None);
    }
}
