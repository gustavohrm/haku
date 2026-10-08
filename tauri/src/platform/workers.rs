//! Deciding which service workers to stop.
//!
//! A service worker outlives the page that started it. Chromium is meant to
//! stop one about 30 seconds after it goes idle, but a busy site's worker can
//! keep itself running far longer: measured in Haku, YouTube's kept a 160 MB
//! process alive more than two minutes after its tab was closed. In a browser
//! whose point is memory staying flat, that is the largest single leak there is.
//!
//! So Haku stops any worker that has been running with no page using it for
//! [`IDLE_GRACE`]. The grace period covers the moments a worker legitimately
//! runs without a page: installing, and serving a navigation before the page it
//! is loading exists. A stopped worker starts again by itself when a page needs
//! it.
//!
//! The decision is made here, from the DevTools protocol's
//! `ServiceWorker.workerVersionUpdated` events, so it is testable without a
//! webview; the platform backend only delivers events and sends the stop.

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

/// How long a worker may run with no page using it before it is stopped.
pub const IDLE_GRACE: Duration = Duration::from_secs(15);

#[derive(Deserialize)]
struct VersionsUpdated {
    versions: Vec<Version>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Version {
    version_id: String,
    running_status: String,
    #[serde(default)]
    controlled_clients: Vec<String>,
    #[serde(rename = "scriptURL", default)]
    script_url: String,
}

/// Scheme of an extension's own pages and workers.
const EXTENSION_SCHEME: &str = "chrome-extension://";

/// Tracks which running workers no page is using.
///
/// Each worker that becomes idle is given a token. The stop is sent only if the
/// same token is still current once the grace period has passed, so a worker
/// that a page picked up again in the meantime is left alone.
#[derive(Debug, Default)]
pub struct WorkerTracker {
    idle: HashMap<String, u64>,
    next_token: u64,
}

impl WorkerTracker {
    /// Records a `ServiceWorker.workerVersionUpdated` event's parameters.
    ///
    /// @returns The workers that just became idle, each with the token to pass
    /// to [`Self::confirm`] once the grace period is over.
    pub fn update(&mut self, json: &str) -> Vec<(String, u64)> {
        let Ok(updated) = serde_json::from_str::<VersionsUpdated>(json) else {
            return Vec::new();
        };

        let mut newly_idle = Vec::new();
        for version in updated.versions {
            // An extension's background worker never has a page, so it would
            // always look idle; stopping it cuts its popup off mid-task. The
            // engine already manages its lifetime.
            let idle = version.running_status == "running"
                && version.controlled_clients.is_empty()
                && !version.script_url.starts_with(EXTENSION_SCHEME);
            if !idle {
                self.idle.remove(&version.version_id);
            } else if !self.idle.contains_key(&version.version_id) {
                self.next_token += 1;
                self.idle.insert(version.version_id.clone(), self.next_token);
                newly_idle.push((version.version_id, self.next_token));
            }
        }
        newly_idle
    }

    /// Whether a worker has stayed idle since `token` was issued.
    ///
    /// A `true` answer is final: the worker is forgotten until it reports
    /// itself again.
    pub fn confirm(&mut self, version: &str, token: u64) -> bool {
        if self.idle.get(version) != Some(&token) {
            return false;
        }
        self.idle.remove(version);
        true
    }
}

/// Parameters of the `ServiceWorker.stopWorker` call for one worker.
pub fn stop_params(version: &str) -> String {
    serde_json::json!({ "versionId": version }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(version: &str, status: &str, clients: &[&str]) -> String {
        event_at("https://a.test/sw.js", version, status, clients)
    }

    fn event_at(script: &str, version: &str, status: &str, clients: &[&str]) -> String {
        serde_json::json!({
            "versions": [{
                "versionId": version,
                "registrationId": "1",
                "scriptURL": script,
                "runningStatus": status,
                "status": "activated",
                "controlledClients": clients,
            }]
        })
        .to_string()
    }

    #[test]
    fn a_worker_left_running_with_no_page_becomes_idle() {
        let mut tracker = WorkerTracker::default();

        let idle = tracker.update(&event("0", "running", &[]));

        assert_eq!(idle.len(), 1);
        assert!(tracker.confirm("0", idle[0].1));
    }

    #[test]
    fn an_extension_worker_is_never_idle() {
        let mut tracker = WorkerTracker::default();
        let event = event_at("chrome-extension://abc/background.js", "7", "running", &[]);

        assert!(tracker.update(&event).is_empty());
    }

    #[test]
    fn a_worker_a_page_is_using_is_never_idle() {
        let mut tracker = WorkerTracker::default();

        assert!(tracker.update(&event("0", "running", &["page"])).is_empty());
    }

    #[test]
    fn a_worker_that_is_not_running_needs_no_stopping() {
        let mut tracker = WorkerTracker::default();

        assert!(tracker.update(&event("0", "stopped", &[])).is_empty());
        assert!(tracker.update(&event("0", "starting", &[])).is_empty());
    }

    #[test]
    fn a_page_picking_the_worker_up_again_cancels_the_pending_stop() {
        let mut tracker = WorkerTracker::default();
        let idle = tracker.update(&event("0", "running", &[]));

        tracker.update(&event("0", "running", &["page"]));

        assert!(!tracker.confirm("0", idle[0].1));
    }

    #[test]
    fn going_idle_again_issues_a_new_token_so_the_earlier_wait_does_not_count() {
        let mut tracker = WorkerTracker::default();
        let first = tracker.update(&event("0", "running", &[]))[0].1;
        tracker.update(&event("0", "running", &["page"]));
        let second = tracker.update(&event("0", "running", &[]))[0].1;

        assert!(!tracker.confirm("0", first));
        assert!(tracker.confirm("0", second));
    }

    #[test]
    fn repeated_reports_of_the_same_idle_worker_keep_its_original_wait() {
        let mut tracker = WorkerTracker::default();
        let first = tracker.update(&event("0", "running", &[]))[0].1;

        assert!(tracker.update(&event("0", "running", &[])).is_empty());
        assert!(tracker.confirm("0", first));
    }

    #[test]
    fn an_unreadable_event_changes_nothing() {
        let mut tracker = WorkerTracker::default();

        assert!(tracker.update("not json").is_empty());
    }

    #[test]
    fn the_stop_call_names_the_worker() {
        assert_eq!(stop_params("12"), r#"{"versionId":"12"}"#);
    }
}
