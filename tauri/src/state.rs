use std::collections::HashMap;
use std::sync::{Mutex, PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::browser::{Browser, Effect, Position};
use crate::chrome::Layout;
use crate::error::{HakuError, Result};
use crate::model::{
    Downloads, Extensions, Loss, LossSignal, MemoryStatus, Pressure, Previews, SlotId, TabId, WindowId,
};
use crate::platform::memory::{Attribution, UnattributedProcess};
use crate::storage::{HistoryDb, Paths, Session, Settings, WindowBounds};
use crate::webview::Viewports;

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

    /// Each slot's measured memory, as the browser's rule weighs it.
    pub fn slot_bytes(&self) -> HashMap<SlotId, u64> {
        self.attribution.slots.clone()
    }

    /// The reading laid out against the pool's slots and their tabs as they
    /// are now.
    pub fn report(&self, browser: &Browser) -> MemoryReport {
        let slots: Vec<SlotMemory> = browser
            .slots()
            .iter()
            .map(|slot| {
                let signals = slot.occupant.map(|tab| browser.loss_signals(tab));
                SlotMemory {
                    slot: slot.id,
                    tab: slot.occupant,
                    position: slot.occupant.and_then(|tab| browser.position(tab)),
                    bytes: self.attribution.slots.get(&slot.id).copied(),
                    loss: signals
                        .as_ref()
                        .map(|signals| signals.iter().map(|signal| signal.level()).max().unwrap_or_default()),
                    signals: signals.unwrap_or_default(),
                }
            })
            .collect();
        let kept = slots
            .iter()
            .filter(|slot| slot.position == Some(Position::Kept))
            .filter_map(|slot| slot.bytes)
            .sum();
        MemoryReport {
            pressure: self.pressure,
            headroom: self.headroom,
            budget: browser.budget(),
            kept,
            slots,
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
    /// What kept tabs may hold together, in bytes.
    #[specta(type = specta_typescript::Number)]
    pub budget: u64,
    /// What kept tabs hold now, in bytes, as far as it was measured.
    #[specta(type = specta_typescript::Number)]
    pub kept: u64,
    pub slots: Vec<SlotMemory>,
    pub unattributed: Vec<UnattributedProcess>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SlotMemory {
    pub slot: SlotId,
    /// Nothing while the slot is parked.
    pub tab: Option<TabId>,
    /// Why the tab holds the slot. Nothing while the slot is parked.
    pub position: Option<Position>,
    /// Commit charge in bytes, or nothing when the slot has not been measured.
    #[specta(type = Option<specta_typescript::Number>)]
    pub bytes: Option<u64>,
    /// What discarding the tab would cost. Nothing while the slot is parked.
    pub loss: Option<Loss>,
    /// The reasons behind `loss`.
    pub signals: Vec<LossSignal>,
}

/// Lets one change to the browser that produces effects through at a time,
/// from deciding it to applying those effects to webviews. A report that
/// produces none may write the browser directly.
///
/// Effects name slots, not tabs, so a batch is only right while the browser
/// is as it was when the batch was decided. Two changes decided in one order
/// and applied in the other leave webviews showing something the browser no
/// longer believes: a tick that discards a tab, applied after a later select
/// that reused its slot, blanks the selected page.
#[derive(Default)]
pub struct Changes(Mutex<()>);

impl Changes {
    /// Decides a change under the browser's write lock, then applies its
    /// effects after releasing it, while still holding off every other change.
    ///
    /// Never call it from the UI thread: applying waits on that thread.
    ///
    /// # Errors
    /// Returns what `decide` or `apply` returns, or [`HakuError::Storage`]
    /// when the browser's lock is poisoned.
    pub fn run<T>(
        &self,
        browser: &RwLock<Browser>,
        decide: impl FnOnce(&mut Browser) -> Result<Vec<Effect>>,
        apply: impl FnOnce(&[Effect]) -> Result<T>,
    ) -> Result<T> {
        // Guards no data, so a change that panicked leaves nothing to distrust.
        let _turn = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let effects = {
            let mut browser = browser
                .write()
                .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
            decide(&mut browser)?
        };
        apply(&effects)
    }
}

/// Failures that happened before any window's interface could listen, held
/// until one can.
///
/// An event sent before the interface has attached its listener is lost, and
/// at startup there is not even a window yet. The interface attaches its
/// listeners before asking for its first state, so the first window to ask
/// takes these, and no other window is sent them again.
#[derive(Default)]
pub struct HeldFailures(Mutex<Vec<HakuError>>);

impl HeldFailures {
    pub fn hold(&self, error: HakuError) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).push(error);
    }

    /// Every failure held, in the order it happened, leaving none held.
    pub fn take(&self) -> Vec<HakuError> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// Everything the command layer needs, behind the locks that make it shareable.
///
/// The browser is the only mutable authority over tabs. A change takes the
/// write lock, mutates, collects effects, and releases it before touching
/// webviews, so a slow native call never blocks a read. Changes themselves go
/// one at a time through [`Changes`].
pub struct AppState {
    pub browser: RwLock<Browser>,
    pub changes: Changes,
    pub settings: RwLock<Settings>,
    pub history: Mutex<HistoryDb>,
    /// Every download listed, of both profiles. Reached from the UI thread,
    /// so only ever held briefly.
    pub downloads: Mutex<Downloads>,
    /// Where each window's page content goes.
    pub viewports: Viewports,
    /// Where each window was last seen on screen, for the session. Kept as
    /// windows move rather than read when the session is saved, because it is
    /// saved from the UI thread too, where reading a window waits on itself.
    pub bounds: RwLock<HashMap<WindowId, WindowBounds>>,
    pub memory: RwLock<MemoryReading>,
    /// What each tab showed as it was left, to cover its page while it
    /// reloads. Never written to disk.
    pub previews: Mutex<Previews>,
    /// What the extensions folder installed at startup.
    pub extensions: RwLock<Extensions>,
    pub paths: Paths,
    pub held_failures: HeldFailures,
    /// Held from capturing the session to writing it, so saves land in the
    /// order they captured and an older session never overwrites a newer one.
    session_saves: Mutex<()>,
}

impl AppState {
    pub fn new(browser: Browser, settings: Settings, history: HistoryDb, paths: Paths) -> Self {
        // A download listed last time ran in a webview that is gone.
        let downloads = Downloads::restored(history.downloads().unwrap_or_default());
        Self {
            browser: RwLock::new(browser),
            changes: Changes::default(),
            settings: RwLock::new(settings),
            history: Mutex::new(history),
            downloads: Mutex::new(downloads),
            viewports: Viewports::default(),
            bounds: RwLock::new(HashMap::new()),
            memory: RwLock::new(MemoryReading::default()),
            previews: Mutex::new(Previews::default()),
            extensions: RwLock::new(Extensions::default()),
            paths,
            held_failures: HeldFailures::default(),
            session_saves: Mutex::new(()),
        }
    }

    /// Validates and records layout under the same serialized turn as its effects.
    ///
    /// An accepted layout also releases a restored window's startup hold, so
    /// its shown pages may load; the caller must reconcile afterward for them to.
    ///
    /// # Errors
    /// Returns [`HakuError::WindowMissing`] for an unknown window, or the layout
    /// validation error without changing any cached geometry.
    pub fn set_layout(&self, browser: &mut Browser, window: WindowId, layout: &Layout) -> Result<bool> {
        let current = browser
            .state_in(window)
            .ok_or_else(|| HakuError::WindowMissing(window.label()))?;
        let accepted = self
            .viewports
            .record(window, layout, &current.viewports, current.layout_revision)?;
        if accepted {
            browser.layout_ready(window);
        }
        Ok(accepted)
    }

    /// Persists the open windows and tabs so the next launch restores them.
    ///
    /// # Errors
    /// Returns [`HakuError::Storage`] when the session file cannot be written.
    pub fn save_session(&self) -> Result<()> {
        let _turn = self.session_saves.lock().unwrap_or_else(PoisonError::into_inner);
        // Captured under the browser's lock and written after it is released,
        // so a page report waiting on the browser does not wait on the disk.
        let session = {
            let browser = self
                .browser
                .read()
                .map_err(|_| HakuError::Storage("browser lock poisoned".into()))?;
            let bounds = self
                .bounds
                .read()
                .map_err(|_| HakuError::Storage("bounds lock poisoned".into()))?;
            Session::capture(&browser, &bounds)
        };
        if !session.replaces_stored() {
            return Ok(());
        }
        crate::storage::write_json(&self.paths.session, &session)
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
        let mut browser = Browser::new(2).with_keeping(1024, vec!["b.test".into()]);
        let (first, _) = browser.open_tab("https://b.test/", true);
        browser.open_tab("https://a.test/", true);
        browser.report_state(first, None);
        let reading = MemoryReading::default().next(
            None,
            Some(Attribution {
                slots: [(SlotId(0), 42), (SlotId(1), 7)].into(),
                unattributed: Vec::new(),
            }),
        );

        let report = reading.report(&browser);

        assert_eq!(report.slots[0].tab, Some(first));
        assert_eq!(report.slots[0].position, Some(Position::Kept));
        assert_eq!(report.slots[0].bytes, Some(42));
        assert_eq!(report.slots[0].loss, Some(Loss::Work));
        assert_eq!(
            report.slots[0].signals,
            vec![LossSignal::KeptSite, LossSignal::Unreadable]
        );
        assert_eq!(report.slots[1].position, Some(Position::Visible));
        assert_eq!(report.budget, 1024);
        assert_eq!(report.kept, 42, "only kept tabs count against the budget");
    }

    #[test]
    fn held_failures_are_taken_once_in_the_order_they_happened() {
        let held = HeldFailures::default();
        held.hold(HakuError::Storage("first".into()));
        held.hold(HakuError::Storage("second".into()));

        assert_eq!(
            held.take(),
            vec![HakuError::Storage("first".into()), HakuError::Storage("second".into())]
        );
        assert!(held.take().is_empty(), "a second window must not be sent them again");
    }

    #[test]
    fn a_failure_held_after_one_was_taken_waits_for_the_next() {
        let held = HeldFailures::default();
        held.hold(HakuError::Storage("first".into()));
        held.take();
        held.hold(HakuError::Storage("later".into()));

        assert_eq!(held.take(), vec![HakuError::Storage("later".into())]);
    }

    /// A tick decided before a select and applied after it blanks the
    /// selected page, so a change waits for the one before it to be applied
    /// before it is even decided.
    #[test]
    fn a_change_is_not_decided_while_another_is_being_applied() {
        use std::sync::mpsc;
        use std::time::Duration;

        let (changes, browser) = (&Changes::default(), &RwLock::new(Browser::new(1)));
        let (applying, first_applying) = mpsc::channel();
        let (release, first_released) = mpsc::channel::<()>();
        let (decided, second_decided) = mpsc::channel();

        std::thread::scope(|scope| {
            scope.spawn(move || {
                changes.run(
                    browser,
                    |_| Ok(Vec::new()),
                    |_| {
                        applying.send(()).unwrap();
                        first_released.recv().unwrap();
                        Ok(())
                    },
                )
            });
            first_applying.recv().unwrap();

            scope.spawn(move || {
                changes.run(
                    browser,
                    |_| {
                        decided.send(()).unwrap();
                        Ok(Vec::new())
                    },
                    |_| Ok(()),
                )
            });
            // Released before asserting, or a failure would leave the first
            // change waiting and the scope joining it forever.
            let early = second_decided.recv_timeout(Duration::from_millis(200)).is_ok();
            release.send(()).unwrap();

            assert!(!early, "the second change was decided while the first was applying");
            assert!(second_decided.recv_timeout(Duration::from_secs(5)).is_ok());
        });
    }
}
