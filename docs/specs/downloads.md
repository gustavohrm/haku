---
status: IMPLEMENTED
last_updated: 2026-11-02
scope: How Haku follows the downloads pages start, what keeps their webviews alive, closing with downloads running, and where downloads are stored and shown.
---

# Downloads

[First release](../first-release.md#features) lists downloads as "WebView2's download event in `platform/`; progress drawn by the chrome; `haku://downloads`". This spec says how, and what Haku does about the one thing that makes downloads hard in Haku: a download runs inside a webview, and Haku destroys webviews that tabs no longer need ([Webview pool](webview-pool.md)).

## Measurements

Measured on 2026-10-30 with Tauri 2.12.5, tauri-runtime-wry 2.12.3, wry 0.57.0, webview2-com 0.39.1 and WebView2 156.0.4314.50, on Windows 11. A debug build was driven over the DevTools protocol, downloading from a local server that sends 40 MB at 512 kB/s, honours `Range`, and logs each request and when its client goes away.

| Done to the webview running a download, or to Haku | What happened to the download                                                                                                                                                           |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Navigated, parked on `about:blank`, frozen, moved  | Kept running and reporting.                                                                                                                                                             |
| Destroyed                                          | The transfer kept running, but its `DownloadOperation` went stale: no more events, and `Pause` failed with `0x8007139F`. Haku can no longer follow or control it.                       |
| Last private tab closed (private session ended)    | Cancelled, its partial file deleted, with no event.                                                                                                                                     |
| Haku quit                                          | Aborted, its partial file deleted.                                                                                                                                                      |
| Network failure, then `Resume`                     | Reported as failed with `canResume`. `Resume` makes the engine start it again as a new `DownloadStarting`, under a new name, continuing from the bytes received with a `Range` request. |

Also measured:

- Private downloads go to the same Downloads folder as ordinary ones.
- A second download a page starts without a click is held by the engine's _multiple automatic downloads_ gate, which Haku asks about like any other permission ([Permissions](permissions.md)). Until 2026-11-01 that prompt was left to the engine.
- **Save link as** and **Save image as** show the engine's native Save As dialog, and the transfer starts while it is open. Once it is accepted, `DownloadStarting` fires with the chosen path, so Haku follows it like any download.
- **Save as** on a page shows the same dialog and writes the page and its `_files` folder, but `DownloadStarting` never fires: Haku cannot follow it.
- After a resume replaces a failed operation, releasing the old operation stops the new one's events, though its file keeps growing. Haku keeps the old one until the download ends.

## Contract

Downloads are owned by Rust (`model/download.rs`, followed from `ipc/downloads.rs`). The interface only shows them and asks for actions.

- Every content webview handles `DownloadStarting` with `Handled = true`: the engine's download UI never shows, and the file goes where the engine would put it, under the name it chose. Haku has no download folder setting and asks no question before a download, except the permission a page needs to start several automatically ([Permissions](permissions.md)). A WebView2 runtime too old to have the event (`ICoreWebView2_4`) keeps the engine's own download interface, and Haku lists nothing from it.
- `DownloadsChanged(Download[])` is sent to a window whenever the downloads of its profile change other than by progress: one starts, changes state, restarts into a new file, is lost or unlisted, or the private session ends. It carries the whole list of that profile, newest first. `downloads()` returns the same list for a window that has just started.
- `DownloadProgressed(Download)` is sent to the windows of a download's profile when only its bytes received or its size changed, with that one entry. It is reported at most every 250 ms per download (`PROGRESS_INTERVAL_MS`). The interface replaces the listed entry with the same id, and ignores one it does not list: the next `DownloadsChanged`, or the `downloads()` snapshot, brings it. A change of state is always sent as `DownloadsChanged`, so the interface's list can lag behind Rust's only in a progress figure, and only until the next report.
- Commands, all chrome-only and scoped to the calling window's profile: `pause_download`, `resume_download`, `cancel_download`, `open_download`, `show_download` take an id; `clear_downloads` unlists the profile's ended downloads; `confirm_close(tab)` answers [a held close](#closing). An id not listed for the window's profile is an `Unsupported` error.
- A download has a state (`inProgress`, `paused`, `completed`, `cancelled`, `failed`), and a `failure` when failed: `network`, `server`, `file`, `other`, or `lost`, which is Haku's own: the engine stopped reporting the download, because its webview went away or Haku quit while it ran. `canResume` is the engine's, for a paused or failed download.
- **Retry** is `resume_download` on a failed download the engine can resume. A resumed download stays the same entry, though the engine starts it as a new operation into a new file; its path is updated to the new file.
- `recent` marks a download started since Haku launched. The toolbar shows those; `haku://downloads` shows all.
- Failures appear in the download's own entry, not as a notice: the entry is where the user looks for it, and a download can fail long after its page was left.

A webview Haku does not place in a profile is treated as private, so a download from it is never written to disk by Haku. No such webview is known; the rule only decides which way an unknown case fails.

## Webviews

A download dies with its webview's `DownloadOperation`, so the webview running an active (in progress or paused) download is held: the pool keeps it whatever happens to its tab ([Webview pool § Capacity](webview-pool.md#capacity)).

- Closing or discarding its tab parks the webview as usual; it is not trimmed while held, at any pressure.
- A tab of the same profile may load into it: navigating does not end the download. A tab of the other profile cannot take it, since changing profile destroys the webview, so the pool grows past it for that tab.
- Closing a window moves its held webviews of the ordinary profile to another browser window, as the private anchor is moved. A private one moves with the other private webviews to another private window, or, when none is left, ends with the session, after [the close is confirmed](#closing).
- Once no active download holds it, the webview is trimmed like any parked webview.

A failed download the engine can resume keeps its webview's slot recorded but does not hold it: if the webview goes before the user retries, the download is lost and can no longer resume.

The hold is per webview, not per tab, because the download outlives the tab: a tab closed, discarded or navigated away no longer says anything about it.

The hold starts with the download. The engine reports a start on the UI thread, which must not change the browser ([Architecture § Threading](../architecture.md#threading)), so every change to the browser first reads which webviews the list holds now. A close or trim decided in the moment after a download starts sees it, and does not wait for the queue that stores and reports it.

## Closing

Quitting cancels every download, and ending the private session cancels the private ones; the engine deletes their partial files. A close that would do either while downloads are active is held: Rust closes nothing and sends the window `CloseConfirmationRequested { tab, ending, downloads }`, where `ending` is `quit` or `privateSession` and `downloads` is how many would be cancelled. The interface asks, with **Close and cancel**, or **Quit and cancel** when quitting, and **Keep downloading**. Confirming calls `confirm_close(tab)`, which cancels those downloads, records them cancelled at once, since the engine may not report it in time, and then closes. Keeping downloading does nothing.

Closing an ordinary tab or a window that is not the last never asks: its downloads keep running in their held webviews.

## Storage

Downloads are stored in `history.sqlite`, in a `downloads` table added by the database's second migration, and restored at launch. One that was active when Haku last stopped is restored as failed (`lost`): the engine aborted it. Restored downloads are not `recent`. A download that cannot be stored, because the disk is full or another program holds the database, stays listed until Haku quits and is gone after a restart. The first such failure is reported as a notice in the browser window used last, and no other until a store succeeds again: a running download is stored up to every 250 ms, and the download itself has not failed, so its entry is not the place for it. Clearing history does not clear downloads; **Clear finished** on `haku://downloads` unlists and forgets the ended ones, and leaves their files.

A private download is never written: `save_download` refuses it, whoever asks. Private downloads are listed only in private windows, and unlisted when the private session ends. Their files stay in the Downloads folder: the engine puts them there, and deleting a file the user downloaded is not Haku's to do.

## Opening files

The chrome has no shell access ([Security](../guidelines/security.md)). Opening goes through Rust, for a download Haku listed, never for a path the caller names:

- `open_download` opens a completed download's file with the program Windows associates with it (`ShellExecuteW`), if the file is still there.
- `show_download` opens Explorer with the file selected, or the folder when the file is gone.

A page can neither call these commands nor name a file: the commands are chrome-only, and the ids are Haku's.

## Context menu

**Save link as** and **Save image as** are kept in the page menu ([Context menus](context-menus.md#the-menu-component)): they start a download Haku follows, after the engine's own Save As dialog. The engine's own **Save as** and **Print** stay withheld; Haku's **Save as…** and **Print…** replace them ([Find, print and save](find-print-save.md)). A page saved that way still does not appear in Downloads, because the engine writes it without a `DownloadStarting` Haku could follow.

## Interface

- **Toolbar**: a downloads button, shown once a download has started since launch, accented while one runs. It opens a list of this session's downloads, each with its name, progress or status, and its actions, and a **Show all downloads** item that opens `haku://downloads`.
- **`haku://downloads`**: every listed download of the window's profile, the same entries and actions, and **Clear finished**.
- An entry re-renders only when its own download changed, so a running download redraws one entry however many are listed.
- Actions by state: in progress: pause, cancel; paused: resume, cancel; completed: open, show in folder; failed: retry and cancel when it can resume, otherwise show in folder; cancelled: none.

## Rejected alternatives

- **Holding the tab rather than the webview.** The download outlives its tab: closing the tab would have to keep a closed tab around, and navigating the tab would release a webview the download still runs in.
- **Counting held webviews in the configured capacity.** It would let a download take a slot from the tab the user is opening. Held webviews are kept on top of it, like the private anchor; a tab of the other profile grows the pool past them.
- **Only the whole list, for progress too.** Recorded first, on the grounds that the list is small and that sending it whole keeps the interface's copy from drifting. It did not weigh that stored downloads stay until **Clear finished**: with about 500 of them, a running download sent roughly 100 KB to each window four times a second, and every entry re-rendered (issue #75). Progress now travels alone, and everything that adds, removes or ends an entry still sends the whole list, which keeps the drift to a progress figure.
- **Only this session's downloads in the event, with `haku://downloads` fetching the rest.** The toolbar would get less, but an open downloads page would fetch the whole list on every report, which moves the cost rather than removing it.
- **A database of its own.** Downloads and history are both records of what was fetched, grow alike, and are kept alike, as in Chrome; a migration costs less than a second file to open, back up and clear.
- **A `force` flag on the close commands.** `confirm_close` cancels first and closes through the ordinary path, so no close path learns to skip the check.
- **Retry by fetching the URL again.** WebView2 has no API to start a download, and navigating to the URL would replay a request that may need its page's state. Retry is the engine's resume only.
- **Following page Save as.** Not possible: no `DownloadStarting` fires.

## Testing

- The model is unit-tested in `model/download.rs`: listing, progress throttling, settling, a resumed failure staying one entry, losing a webview, private downloads, and restoring. `storage/history_db.rs` tests the migration, that private downloads are never written, and forgetting. Pool and browser tests cover the hold, the pool growing past another profile's held webview, and the held close.
- The interface is tested with Vitest: the toolbar button, its list and actions, and `haku://downloads`.
- In the app, checked on 2026-10-30 with a debug build: starting, pausing, resuming and cancelling a download; closing its tab while it runs; quitting with one running, both answers; a private window's download, listed only there, not stored, and the close of its window; a network failure retried to completion as one entry; **Save image as**; and **Show in folder**. **Open file** was not run in the app.

## Known limits

- What a crashed page renderer does to a download running in its webview has not been checked. A crash blanks the webview rather than destroying it, so the download is expected to go on.
- While a download runs, its entry is written to the database each time progress is reported, at most every 250 ms. It is one small row, but writes that only move `received` could be batched further.
