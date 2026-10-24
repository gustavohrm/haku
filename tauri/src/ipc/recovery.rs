//! What Haku does when a process behind the interface or the engine exits.
//!
//! A page's own process is the browser model's to answer, as a crashed tab;
//! this is the rest of [Crash recovery](../../../docs/specs/crash-recovery.md).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tauri::Manager;

use crate::model::WindowId;
use crate::platform::{self, FailureSink, ProcessFailure};
use crate::state::{now_ms, AppState};

/// How many times a window's interface is reloaded within [`RELOAD_WINDOW`]
/// before it is left as it is.
const MAX_RELOADS: usize = 3;
const RELOAD_WINDOW: Duration = Duration::from_secs(60);

/// How soon after a restart another engine failure counts as the engine
/// failing as it starts.
const RESTART_WINDOW_MS: u64 = 60_000;

/// The argument a restart after an engine failure passes the new process,
/// with the time of the restart in milliseconds since the Unix epoch.
const RESTARTED_AT: &str = "--restarted-after-engine-failure=";

/// Makes the sink for a window chrome's process failures.
pub fn chrome_failures(app: &tauri::AppHandle, window: WindowId) -> FailureSink {
    let app = app.clone();
    Arc::new(move |failure| match failure {
        ProcessFailure::Page(_) => reload_interface(&app, window),
        ProcessFailure::Engine => engine_failed(&app),
    })
}

/// Reloads a window's interface after its process exited, unless it has been
/// reloaded too often lately: it is then failing as it starts, and reloading
/// again would only loop. Alt+F4 still closes the window.
fn reload_interface(app: &tauri::AppHandle, window: WindowId) {
    static RELOADS: LazyLock<Mutex<HashMap<WindowId, Vec<Instant>>>> = LazyLock::new(Mutex::default);
    let allowed = {
        let mut reloads = RELOADS.lock().unwrap_or_else(PoisonError::into_inner);
        allow_reload(reloads.entry(window).or_default(), Instant::now())
    };
    if !allowed {
        return;
    }
    let app = app.clone();
    // Called on the UI thread; reloading dispatches to it.
    std::thread::spawn(move || {
        if let Some(chrome) = app.get_webview(&window.label()) {
            let _ = chrome.reload();
        }
    });
}

/// Records a reload at `now` in `reloads` if fewer than [`MAX_RELOADS`]
/// happened within [`RELOAD_WINDOW`] before it.
fn allow_reload(reloads: &mut Vec<Instant>, now: Instant) -> bool {
    reloads.retain(|&at| now.duration_since(at) < RELOAD_WINDOW);
    if reloads.len() >= MAX_RELOADS {
        return false;
    }
    reloads.push(now);
    true
}

/// Restarts Haku after the engine's browser process exited, taking every
/// webview with it.
///
/// Every webview reports the failure, so only the first report is acted on.
/// When the engine already failed within a minute of the last restart, it is
/// failing as it starts: Haku says so and quits rather than restart forever.
pub fn engine_failed(app: &tauri::AppHandle) {
    static HANDLED: AtomicBool = AtomicBool::new(false);
    if HANDLED.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    // Called on the UI thread; saving waits on the disk.
    std::thread::spawn(move || {
        if let Some(state) = app.try_state::<AppState>() {
            let _ = state.save_session();
        }
        let now = now_ms();
        if restarted_recently(std::env::args(), now) {
            platform::show_fatal("Haku's engine keeps stopping, so Haku has closed.");
            app.exit(1);
            return;
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            // Released first: the new process would otherwise take itself for
            // a second launch, hand over to this one and quit.
            tauri_plugin_single_instance::destroy(&handle);
            let restarted = std::env::current_exe().and_then(|exe| {
                std::process::Command::new(exe)
                    .arg(format!("{RESTARTED_AT}{now}"))
                    .spawn()
            });
            if restarted.is_err() {
                platform::show_fatal("Haku's engine stopped, and Haku could not start again. Open it to continue.");
                handle.exit(1);
                return;
            }
            handle.exit(0);
        });
    });
}

/// Whether this process was started by a restart less than
/// [`RESTART_WINDOW_MS`] before `now`.
fn restarted_recently(args: impl IntoIterator<Item = String>, now: u64) -> bool {
    args.into_iter()
        .find_map(|arg| arg.strip_prefix(RESTARTED_AT)?.parse::<u64>().ok())
        .is_some_and(|at| now.saturating_sub(at) < RESTART_WINDOW_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interface_is_reloaded_three_times_a_minute_at_most() {
        let start = Instant::now();
        let mut reloads = Vec::new();

        let allowed: Vec<bool> = (0..4)
            .map(|n| allow_reload(&mut reloads, start + Duration::from_secs(n)))
            .collect();

        assert_eq!(allowed, vec![true, true, true, false]);
        assert!(allow_reload(
            &mut reloads,
            start + RELOAD_WINDOW + Duration::from_secs(1)
        ));
    }

    #[test]
    fn a_failure_soon_after_a_restart_is_the_engine_failing_as_it_starts() {
        let args = |at: u64| vec!["haku.exe".to_string(), format!("{RESTARTED_AT}{at}")];

        assert!(restarted_recently(args(1_000), 30_000));
        assert!(!restarted_recently(args(1_000), 1_000 + RESTART_WINDOW_MS));
        assert!(!restarted_recently(vec!["haku.exe".to_string()], 30_000));
    }
}
