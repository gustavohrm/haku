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

use crate::error::Result;
use crate::model::{DownloadId, Downloading, Downloads, Profile, SlotId};
use crate::platform::{self, DownloadSignal, DownloadStart};
use crate::state::{now_ms, AppState};

use super::commands::mutate;
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
            while let Ok(mut changed) = receiver.recv() {
                for more in receiver.try_iter() {
                    changed.extend(more);
                }
                held = settle(&app, &changed, held);
            }
        });
        sender
    });
    let _ = queue.send(changed);
}

/// @returns What downloads hold back now.
fn settle(app: &tauri::AppHandle, changed: &[DownloadId], held: Downloading) -> Downloading {
    let state = app.state::<AppState>();
    let (stored, downloading) = {
        let downloads = lock(&state);
        let stored: Vec<_> = changed.iter().filter_map(|id| downloads.entry(*id).cloned()).collect();
        (stored, downloads.downloading())
    };
    if let Ok(history) = state.history.lock() {
        for download in &stored {
            let _ = history.save_download(download);
        }
    }
    report(app, &state);
    if downloading != held {
        let summary = downloading.clone();
        let _ = mutate(app, &state, |browser| Ok(browser.set_downloading(summary)));
    }
    downloading
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
