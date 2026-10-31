//! The downloads Haku follows: what each one is, how far it got, and which
//! webview it runs in. [Downloads](../../../docs/specs/downloads.md).
//!
//! Pure, like the browser: the engine's events arrive as calls here, and
//! what they change is returned for the caller to show, store and act on.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::pool::{Profile, SlotId};

/// The shortest time between two progress reports of one download, in
/// milliseconds. The engine reports every chunk; the interface needs a few
/// frames a second.
pub const PROGRESS_INTERVAL_MS: u64 = 250;

/// Identifies one download, unique across runs: stored downloads keep theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
pub struct DownloadId(#[specta(type = specta_typescript::Number)] pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    InProgress,
    Paused,
    Completed,
    Cancelled,
    /// Stopped by a failure. [`Download::failure`] says which.
    Failed,
}

impl DownloadState {
    /// Whether the download may still move: it runs, or it waits to be
    /// resumed. Its webview must not be destroyed while it may.
    pub fn is_active(self) -> bool {
        matches!(self, Self::InProgress | Self::Paused)
    }
}

/// Why a download failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DownloadFailure {
    Network,
    /// The server refused or broke off the transfer.
    Server,
    /// The file could not be written: no space, no permission, a name too
    /// long, or a scan that blocked it.
    File,
    /// Haku stopped following it: its webview was destroyed, or Haku ended
    /// while it ran. What became of the file is unknown.
    Lost,
    Other,
}

/// One download, as the interface lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: DownloadId,
    pub url: String,
    /// Where the file is written, chosen by the engine.
    pub path: String,
    /// Bytes received so far.
    #[specta(type = specta_typescript::Number)]
    pub received: u64,
    /// The file's size, when the server said.
    #[specta(type = Option<specta_typescript::Number>)]
    pub total: Option<u64>,
    pub state: DownloadState,
    /// Set only while [`Self::state`] is [`DownloadState::Failed`].
    pub failure: Option<DownloadFailure>,
    /// Whether the engine can carry on from where it stopped: a paused
    /// download, or one a failure interrupted, that the server lets resume.
    pub can_resume: bool,
    /// Milliseconds since the Unix epoch.
    #[specta(type = specta_typescript::Number)]
    pub started_at: u64,
    /// Started since Haku last launched. The toolbar lists only these.
    pub recent: bool,
    /// Started in a private window. Never written to disk.
    pub private: bool,
    /// The webview it runs in, while Haku follows it.
    #[serde(skip)]
    #[specta(skip)]
    pub slot: Option<SlotId>,
}

impl Download {
    /// The file's name, for display.
    pub fn name(&self) -> String {
        Path::new(&self.path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn profile(&self) -> Profile {
        if self.private {
            Profile::Private
        } else {
            Profile::Default
        }
    }
}

/// What the engine said about a download's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settled {
    pub state: DownloadState,
    pub failure: Option<DownloadFailure>,
    pub received: u64,
    pub total: Option<u64>,
    pub can_resume: bool,
}

/// What active downloads hold back: the webviews they run in, which are not
/// destroyed, and how many run in each profile, which closing the last window
/// or the private session would cancel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Downloading {
    pub slots: HashSet<SlotId>,
    pub ordinary: usize,
    pub private: usize,
}

/// Every download Haku lists, newest first.
#[derive(Debug, Default)]
pub struct Downloads {
    entries: Vec<Download>,
    next_id: u64,
    /// When each running download was last reported.
    reported: HashMap<DownloadId, u64>,
}

impl Downloads {
    /// The downloads stored by earlier runs, newest first.
    ///
    /// One stored while it was still active was ended with Haku, which the
    /// engine does not let a download survive, and is listed as lost.
    pub fn restored(stored: Vec<Download>) -> Self {
        let next_id = stored.iter().map(|download| download.id.0 + 1).max().unwrap_or(1);
        let mut entries: Vec<Download> = stored
            .into_iter()
            .map(|mut download| {
                download.recent = false;
                download.slot = None;
                if download.state.is_active() {
                    download.state = DownloadState::Failed;
                    download.failure = Some(DownloadFailure::Lost);
                    download.can_resume = false;
                }
                download
            })
            .collect();
        entries.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
        Self {
            entries,
            next_id,
            reported: HashMap::new(),
        }
    }

    /// For when the stored downloads could not be read: lists none, and
    /// numbers new ones from `now`, in milliseconds, which is past any stored
    /// id, so none of them replaces a stored download.
    pub fn unread(now: u64) -> Self {
        Self {
            next_id: now,
            ..Self::default()
        }
    }

    /// Lists a download the engine started.
    pub fn start(
        &mut self,
        slot: Option<SlotId>,
        private: bool,
        url: String,
        path: String,
        total: Option<u64>,
        now: u64,
    ) -> &Download {
        let id = DownloadId(self.next_id);
        self.next_id += 1;
        self.reported.insert(id, now);
        self.entries.insert(
            0,
            Download {
                id,
                url,
                path,
                received: 0,
                total,
                state: DownloadState::InProgress,
                failure: None,
                can_resume: false,
                started_at: now,
                recent: true,
                private,
                slot,
            },
        );
        &self.entries[0]
    }

    /// Records bytes received.
    ///
    /// @returns Whether to report it, at most once per
    ///   [`PROGRESS_INTERVAL_MS`] for each download.
    pub fn progress(&mut self, id: DownloadId, received: u64, total: Option<u64>, now: u64) -> bool {
        // A late report must not change, or store again, one that has ended.
        let Some(download) = self.entry_mut(id).filter(|download| download.state.is_active()) else {
            return false;
        };
        download.received = received;
        download.total = total.or(download.total);
        let last = self.reported.get(&id).copied().unwrap_or(0);
        if now.saturating_sub(last) < PROGRESS_INTERVAL_MS {
            return false;
        }
        self.reported.insert(id, now);
        true
    }

    /// Records a change of state, which is always reported.
    ///
    /// A download that has ended stays ended: the engine may still report a
    /// webview Haku already gave up on, or a cancel Haku already recorded.
    ///
    /// @returns The download as it is now, or nothing when it is not listed
    ///   or had already ended.
    pub fn settle(&mut self, id: DownloadId, settled: Settled) -> Option<&Download> {
        let download = self.entry_mut(id)?;
        // A failure the engine can resume runs again once resumed.
        if !download.state.is_active() && !download.can_resume {
            return None;
        }
        download.state = settled.state;
        download.failure =
            (settled.state == DownloadState::Failed).then(|| settled.failure.unwrap_or(DownloadFailure::Other));
        download.received = settled.received;
        download.total = settled.total.or(download.total);
        download.can_resume =
            settled.can_resume && matches!(settled.state, DownloadState::Paused | DownloadState::Failed);
        // A download that may resume keeps its webview's slot, which runs it
        // again; only an active one holds that webview.
        if !download.state.is_active() && !download.can_resume {
            download.slot = None;
        }
        if download.state == DownloadState::Completed {
            download.total = Some(download.received);
        }
        self.reported.remove(&id);
        self.entry(id)
    }

    /// Records that a download runs again into a new file, as one the
    /// network interrupted does once resumed.
    ///
    /// @returns The download as it is now, or nothing when it is not listed
    ///   or had already ended.
    pub fn restart(&mut self, id: DownloadId, path: String, total: Option<u64>) -> Option<&Download> {
        let download = self.entry_mut(id)?;
        if !download.state.is_active() && !download.can_resume {
            return None;
        }
        download.path = path;
        download.total = total.or(download.total);
        download.state = DownloadState::InProgress;
        download.failure = None;
        download.can_resume = false;
        self.entry(id)
    }

    /// Records that the active downloads Haku is about to cancel itself are
    /// cancelled, without waiting for the engine to say so, which it may not
    /// once their webview is gone.
    ///
    /// @returns Those downloads as they are now.
    pub fn cancel_all(&mut self, ids: &[DownloadId]) -> Vec<Download> {
        ids.iter()
            .filter_map(|&id| {
                let settled = Settled {
                    state: DownloadState::Cancelled,
                    failure: None,
                    received: self.entry(id)?.received,
                    total: self.entry(id)?.total,
                    can_resume: false,
                };
                self.settle(id, settled).cloned()
            })
            .collect()
    }

    /// Records that a webview went away with the downloads in it, which the
    /// engine then stops reporting: they are lost.
    ///
    /// @returns Those downloads as they are now.
    pub fn slot_gone(&mut self, slot: SlotId) -> Vec<Download> {
        let ids: Vec<DownloadId> = self
            .entries
            .iter()
            .filter(|download| download.slot == Some(slot))
            .map(|download| download.id)
            .collect();
        ids.into_iter().filter_map(|id| self.lose(id).cloned()).collect()
    }

    /// Records that the engine no longer has a download to act on: one that
    /// was running is lost, and none can resume.
    pub fn lose(&mut self, id: DownloadId) -> Option<&Download> {
        let download = self.entry_mut(id)?;
        if download.state.is_active() {
            download.state = DownloadState::Failed;
            download.failure = Some(DownloadFailure::Lost);
        }
        download.can_resume = false;
        download.slot = None;
        self.reported.remove(&id);
        self.entry(id)
    }

    /// The ended downloads of one profile, which **Clear finished** forgets
    /// and unlists; their files stay.
    pub fn ended(&self, private: bool) -> Vec<DownloadId> {
        self.entries
            .iter()
            .filter(|download| download.private == private && !download.state.is_active())
            .map(|download| download.id)
            .collect()
    }

    /// Unlists downloads.
    pub fn unlist(&mut self, ids: &[DownloadId]) {
        self.entries.retain(|download| !ids.contains(&download.id));
    }

    /// Unlists every private download, when the private session ends: it
    /// leaves no trace in Haku, though its files stay.
    ///
    /// @returns Whether any was listed.
    pub fn end_private_session(&mut self) -> bool {
        let before = self.entries.len();
        self.entries.retain(|download| !download.private);
        self.entries.len() != before
    }

    pub fn entry(&self, id: DownloadId) -> Option<&Download> {
        self.entries.iter().find(|download| download.id == id)
    }

    fn entry_mut(&mut self, id: DownloadId) -> Option<&mut Download> {
        self.entries.iter_mut().find(|download| download.id == id)
    }

    /// What a window of one profile lists, newest first.
    pub fn list(&self, private: bool) -> Vec<Download> {
        self.entries
            .iter()
            .filter(|download| download.private == private)
            .cloned()
            .collect()
    }

    /// The active downloads of a profile, or of both.
    pub fn active(&self, profile: Option<Profile>) -> Vec<DownloadId> {
        self.entries
            .iter()
            .filter(|download| {
                download.state.is_active() && profile.is_none_or(|profile| download.profile() == profile)
            })
            .map(|download| download.id)
            .collect()
    }

    pub fn downloading(&self) -> Downloading {
        let mut downloading = Downloading::default();
        for download in self.entries.iter().filter(|download| download.state.is_active()) {
            downloading.slots.extend(download.slot);
            if download.private {
                downloading.private += 1;
            } else {
                downloading.ordinary += 1;
            }
        }
        downloading
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(downloads: &mut Downloads, slot: usize, private: bool) -> DownloadId {
        downloads
            .start(
                Some(SlotId(slot)),
                private,
                "https://a.test/file.zip".into(),
                r"C:\Users\me\Downloads\file.zip".into(),
                Some(100),
                1_000,
            )
            .id
    }

    fn settled(state: DownloadState, failure: Option<DownloadFailure>, can_resume: bool) -> Settled {
        Settled {
            state,
            failure,
            received: 40,
            total: Some(100),
            can_resume,
        }
    }

    #[test]
    fn a_started_download_is_listed_first_and_runs() {
        let mut downloads = Downloads::default();
        let older = started(&mut downloads, 0, false);
        let newer = started(&mut downloads, 0, false);

        let list = downloads.list(false);
        assert_eq!(
            list.iter().map(|download| download.id).collect::<Vec<_>>(),
            [newer, older]
        );
        assert_eq!(list[0].state, DownloadState::InProgress);
        assert!(list[0].recent);
        assert_eq!(list[0].name(), "file.zip");
    }

    #[test]
    fn each_window_lists_only_its_own_profiles_downloads() {
        let mut downloads = Downloads::default();
        let ordinary = started(&mut downloads, 0, false);
        let private = started(&mut downloads, 1, true);

        assert_eq!(
            downloads.list(false).iter().map(|d| d.id).collect::<Vec<_>>(),
            [ordinary]
        );
        assert_eq!(downloads.list(true).iter().map(|d| d.id).collect::<Vec<_>>(), [private]);
    }

    #[test]
    fn progress_is_reported_at_most_once_per_interval() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 0, false);

        assert!(!downloads.progress(id, 10, Some(100), 1_000 + PROGRESS_INTERVAL_MS - 1));
        assert!(downloads.progress(id, 20, Some(100), 1_000 + PROGRESS_INTERVAL_MS));
        assert!(!downloads.progress(id, 30, Some(100), 1_000 + PROGRESS_INTERVAL_MS + 1));
        assert_eq!(
            downloads.entry(id).unwrap().received,
            30,
            "bytes are kept even when not reported"
        );
    }

    #[test]
    fn a_completed_download_releases_its_webview_and_records_its_size() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 3, false);
        assert_eq!(downloads.downloading().slots, HashSet::from([SlotId(3)]));

        let done = downloads
            .settle(id, settled(DownloadState::Completed, None, false))
            .unwrap();

        assert_eq!(done.total, Some(40));
        assert_eq!(done.slot, None);
        assert!(downloads.downloading().slots.is_empty());
    }

    #[test]
    fn a_paused_download_keeps_its_webview() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 3, false);

        let paused = downloads
            .settle(id, settled(DownloadState::Paused, None, true))
            .unwrap();

        assert!(paused.can_resume);
        assert_eq!(downloads.downloading().slots, HashSet::from([SlotId(3)]));
    }

    #[test]
    fn a_failure_always_carries_a_reason() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 0, false);

        let failed = downloads
            .settle(id, settled(DownloadState::Failed, None, true))
            .unwrap();

        assert_eq!(failed.failure, Some(DownloadFailure::Other));
        assert!(failed.can_resume);
    }

    #[test]
    fn a_resumable_failure_runs_again_in_its_webview_until_that_goes() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 3, false);
        downloads.settle(id, settled(DownloadState::Failed, Some(DownloadFailure::Network), true));
        assert!(downloads.downloading().slots.is_empty(), "a failure holds no webview");

        let resumed = downloads
            .settle(id, settled(DownloadState::InProgress, None, false))
            .unwrap();
        assert_eq!(resumed.state, DownloadState::InProgress);
        assert_eq!(downloads.downloading().slots, HashSet::from([SlotId(3)]));

        downloads.settle(id, settled(DownloadState::Failed, Some(DownloadFailure::Network), true));
        let gone = downloads.slot_gone(SlotId(3));
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].failure, Some(DownloadFailure::Network));
        assert!(!gone[0].can_resume);
    }

    #[test]
    fn a_failure_resumed_into_a_new_file_stays_one_entry() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 3, false);
        downloads.settle(id, settled(DownloadState::Failed, Some(DownloadFailure::Network), true));

        let restarted = downloads
            .restart(id, String::from(r"C:\Users\me\Downloads\file (1).zip"), Some(10))
            .unwrap();
        assert_eq!(restarted.state, DownloadState::InProgress);
        assert_eq!(restarted.failure, None);
        assert_eq!(restarted.path, r"C:\Users\me\Downloads\file (1).zip");
        assert_eq!(downloads.list(false).len(), 1);

        downloads.cancel_all(&[id]);
        assert!(downloads.restart(id, String::from("elsewhere"), None).is_none());
    }

    #[test]
    fn an_ended_download_does_not_change_again() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 0, false);
        downloads.cancel_all(&[id]);

        assert!(downloads
            .settle(
                id,
                settled(DownloadState::Failed, Some(DownloadFailure::Network), false)
            )
            .is_none());
        assert_eq!(downloads.entry(id).unwrap().state, DownloadState::Cancelled);
    }

    #[test]
    fn a_webview_going_away_loses_only_its_active_downloads() {
        let mut downloads = Downloads::default();
        let done = started(&mut downloads, 1, false);
        downloads.settle(done, settled(DownloadState::Completed, None, false));
        let running = started(&mut downloads, 1, false);
        let elsewhere = started(&mut downloads, 2, false);

        let lost = downloads.slot_gone(SlotId(1));

        assert_eq!(lost.iter().map(|download| download.id).collect::<Vec<_>>(), [running]);
        assert_eq!(lost[0].failure, Some(DownloadFailure::Lost));
        assert_eq!(downloads.entry(done).unwrap().state, DownloadState::Completed);
        assert_eq!(downloads.entry(elsewhere).unwrap().state, DownloadState::InProgress);
    }

    #[test]
    fn downloading_counts_each_profiles_active_downloads() {
        let mut downloads = Downloads::default();
        started(&mut downloads, 0, false);
        let paused = started(&mut downloads, 0, false);
        downloads.settle(paused, settled(DownloadState::Paused, None, true));
        let done = started(&mut downloads, 1, false);
        downloads.settle(done, settled(DownloadState::Completed, None, false));
        started(&mut downloads, 2, true);

        let downloading = downloads.downloading();
        assert_eq!((downloading.ordinary, downloading.private), (2, 1));
        assert_eq!(downloading.slots, HashSet::from([SlotId(0), SlotId(2)]));
        assert_eq!(downloads.active(Some(Profile::Private)).len(), 1);
        assert_eq!(downloads.active(None).len(), 3);
    }

    #[test]
    fn progress_leaves_an_ended_download_as_it_ended() {
        let mut downloads = Downloads::default();
        let id = started(&mut downloads, 0, false);
        downloads.cancel_all(&[id]);

        assert!(!downloads.progress(id, 512, Some(1024), PROGRESS_INTERVAL_MS * 10));
        assert_eq!(downloads.entry(id).unwrap().received, 0);
    }

    #[test]
    fn clearing_unlists_the_ended_downloads_of_one_profile_only() {
        let mut downloads = Downloads::default();
        let done = started(&mut downloads, 0, false);
        downloads.settle(done, settled(DownloadState::Completed, None, false));
        let running = started(&mut downloads, 0, false);
        let private = started(&mut downloads, 1, true);
        downloads.cancel_all(&[private]);

        let ended = downloads.ended(false);
        assert_eq!(ended, [done]);
        downloads.unlist(&ended);
        assert_eq!(
            downloads.list(false).iter().map(|d| d.id).collect::<Vec<_>>(),
            [running]
        );
        assert_eq!(downloads.list(true).len(), 1);
    }

    #[test]
    fn ending_the_private_session_unlists_its_downloads() {
        let mut downloads = Downloads::default();
        let ordinary = started(&mut downloads, 0, false);
        started(&mut downloads, 1, true);

        downloads.end_private_session();

        assert!(downloads.list(true).is_empty());
        assert_eq!(downloads.list(false)[0].id, ordinary);
    }

    #[test]
    fn restored_downloads_are_not_recent_and_new_ids_follow_theirs() {
        let mut earlier = Downloads::default();
        let done = started(&mut earlier, 0, false);
        earlier.settle(done, settled(DownloadState::Completed, None, false));
        let running = started(&mut earlier, 0, false);

        let mut downloads = Downloads::restored(earlier.list(false));

        let list = downloads.list(false);
        assert!(list.iter().all(|download| !download.recent && download.slot.is_none()));
        let interrupted = downloads.entry(running).unwrap();
        assert_eq!(interrupted.state, DownloadState::Failed);
        assert_eq!(interrupted.failure, Some(DownloadFailure::Lost));
        assert!(downloads.downloading().slots.is_empty());
        assert!(started(&mut downloads, 0, false) > running);
    }
}
