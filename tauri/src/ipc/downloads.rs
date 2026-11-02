//! Follows the downloads content webviews start: lists them, stores the
//! ordinary ones, tells each window's interface, and keeps the webviews that
//! run them. [Downloads](../../../docs/specs/downloads.md).
//!
//! The engine reports downloads on the UI thread, which must not wait on
//! storage or on a change to the browser. Each report updates the list there
//! and hands the rest to one thread, in order.

use std::sync::{mpsc, Arc, MutexGuard, OnceLock, PoisonError};

use tauri::Manager;
use tauri_specta::Event;

use crate::browser::{Browser, Effect};
use crate::error::{HakuError, Result};
use crate::model::{DownloadId, Downloading, Downloads, Profile, SlotId};
use crate::platform::{self, DownloadSignal, DownloadStart};
use crate::state::{now_ms, AppState};

use super::commands::{mutate, report_failure};
use super::events::DownloadsChanged;

/// Takes over the downloads of every content webview created from here on.
///
/// # Errors
/// Returns [`crate::error::HakuError::Unsupported`] on platforms without an
/// implementation.
pub fn observe(app: &tauri::AppHandle) -> Result<()> {
    let (on_start, on_change) = (app.clone(), app.clone());
    platform::observe_downloads(
        Arc::new(move |slot, start| started(&on_start, slot, start)),
        Arc::new(move |id, signal| changed(&on_change, id, signal)),
    )
}

/// The download list. Held briefly, and never while waiting on anything.
pub fn lock(state: &AppState) -> MutexGuard<'_, Downloads> {
    // Every change to the list completes under the lock, so a panic leaves
    // nothing half-written.
    state.downloads.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Gives the browser what downloads hold back now, before it decides a
/// change: a download that just started is not reported to it until the
/// queue gets to it, and closing or trimming must not miss it meanwhile.
///
/// Takes the list's lock under the browser's, which is safe because the
/// list is never held while waiting on anything.
pub fn hold(state: &AppState, browser: &mut Browser) -> Vec<Effect> {
    browser.set_downloading(lock(state).downloading())
}

fn started(app: &tauri::AppHandle, slot: Option<SlotId>, start: DownloadStart) -> DownloadId {
    let state = app.state::<AppState>();
    // A webview Haku does not place in a profile is treated as private, so
    // what it downloads is never written down.
    let private = slot
        .and_then(|slot| {
            let browser = state.browser.read().ok()?;
            let entry = browser.slots().iter().find(|entry| entry.id == slot)?;
            Some(entry.profile == Profile::Private)
        })
        .unwrap_or(true);
    let id = lock(&state)
        .start(slot, private, start.url, start.path, start.total, now_ms())
        .id;
    queue(app, vec![id]);
    id
}

fn changed(app: &tauri::AppHandle, id: DownloadId, signal: DownloadSignal) {
    let state = app.state::<AppState>();
    let report = {
        let mut downloads = lock(&state);
        match signal {
            DownloadSignal::Progress { received, total } => downloads.progress(id, received, total, now_ms()),
            DownloadSignal::Settled(settled) => downloads.settle(id, settled).is_some(),
            DownloadSignal::Restarted { path, total } => downloads.restart(id, path, total).is_some(),
        }
    };
    if report {
        queue(app, vec![id]);
    }
}

/// Records that a webview went away, taking the downloads in it.
pub fn slot_gone(app: &tauri::AppHandle, state: &AppState, slot: SlotId) {
    let lost: Vec<DownloadId> = lock(state).slot_gone(slot).iter().map(|download| download.id).collect();
    if !lost.is_empty() {
        queue(app, lost);
    }
}

/// Unlists the private downloads once no private tab is left.
pub fn end_private_session(app: &tauri::AppHandle, state: &AppState) {
    if lock(state).end_private_session() {
        queue(app, Vec::new());
    }
}

/// Stores the downloads that changed as they are now, tells every window,
/// and lets the browser know which webviews downloads keep. In order, off the
/// thread that asked.
pub fn queue(app: &tauri::AppHandle, changed: Vec<DownloadId>) {
    static QUEUE: OnceLock<mpsc::Sender<Vec<DownloadId>>> = OnceLock::new();
    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Vec<DownloadId>>();
        let app = app.clone();
        std::thread::spawn(move || {
            let mut held = Downloading::default();
            let mut failing = false;
            while let Ok(mut changed) = receiver.recv() {
                for more in receiver.try_iter() {
                    changed.extend(more);
                }
                held = settle(&app, &changed, held, &mut failing);
            }
        });
        sender
    });
    let _ = queue.send(changed);
}

/// @param failing - Whether the last store failed, so a failure that lasts
/// is reported once.
/// @returns What downloads hold back now.
fn settle(app: &tauri::AppHandle, changed: &[DownloadId], held: Downloading, failing: &mut bool) -> Downloading {
    let state = app.state::<AppState>();
    // Read under the store's lock, so a download cleared meanwhile is not
    // written back. The list is never waited on while storing.
    let history = state.history.lock();
    let (stored, downloading) = {
        let downloads = lock(&state);
        let stored: Vec<_> = changed.iter().filter_map(|id| downloads.entry(*id).cloned()).collect();
        (stored, downloads.downloading())
    };
    let outcome = match history {
        _ if stored.is_empty() => None,
        Ok(history) => Some(store(&history, &stored)),
        Err(_) => Some(Err(HakuError::Storage(
            "downloads were not kept for the next start: the history lock is poisoned".into(),
        ))),
    };
    if let Some(failure) = first_failure(failing, outcome) {
        // Not a command's failure, and no window asked: the one used last
        // shows it, as it would a tab opened from nowhere in particular.
        let window = state.browser.read().map(|browser| browser.main_window());
        if let Ok(window) = window {
            report_failure(app, window, failure);
        }
    }
    report(app, &state);
    if downloading != held {
        let summary = downloading.clone();
        let _ = mutate(app, &state, |browser| Ok(browser.set_downloading(summary)));
    }
    downloading
}

/// Stores each download, trying every one.
///
/// # Errors
/// Returns the first failure, naming its download.
fn store(history: &crate::storage::HistoryDb, downloads: &[crate::model::Download]) -> Result<()> {
    let mut first = Ok(());
    for download in downloads {
        if let Err(error) = history.save_download(download) {
            if first.is_ok() {
                let reason = match error {
                    HakuError::Storage(reason) => reason,
                    other => other.to_string(),
                };
                first = Err(HakuError::Storage(format!(
                    "{} was not kept for the next start: {reason}",
                    download.name()
                )));
            }
        }
    }
    first
}

/// The failure to report from storing downloads: the first of a run of
/// failures, and none after it until a store succeeds. A running download is
/// stored up to every 250 ms, so a full disk would otherwise report each time.
///
/// @param outcome - The store's outcome, or `None` when nothing was stored.
fn first_failure(failing: &mut bool, outcome: Option<Result<()>>) -> Option<HakuError> {
    let error = outcome?.err();
    let report = error.is_some() && !*failing;
    *failing = error.is_some();
    error.filter(|_| report)
}

/// Sends each window the downloads of its profile.
pub fn report(app: &tauri::AppHandle, state: &AppState) {
    let windows: Vec<_> = match state.browser.read() {
        Ok(browser) => browser
            .window_ids()
            .into_iter()
            .map(|window| (window, browser.is_private(window)))
            .collect(),
        Err(_) => return,
    };
    let (ordinary, private) = {
        let downloads = lock(state);
        (downloads.list(false), downloads.list(true))
    };
    for (window, is_private) in windows {
        let list = if is_private { private.clone() } else { ordinary.clone() };
        let _ = DownloadsChanged(list).emit_to(app, window.label());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed() -> Option<Result<()>> {
        Some(Err(HakuError::Storage("disk full".into())))
    }

    #[test]
    fn a_lasting_store_failure_is_reported_once_until_a_store_succeeds() {
        let mut failing = false;
        assert!(first_failure(&mut failing, failed()).is_some());
        assert!(first_failure(&mut failing, failed()).is_none());
        assert!(
            first_failure(&mut failing, None).is_none(),
            "nothing stored is not a success"
        );
        assert!(first_failure(&mut failing, failed()).is_none());
        assert!(first_failure(&mut failing, Some(Ok(()))).is_none());
        assert!(first_failure(&mut failing, failed()).is_some());
    }
}
