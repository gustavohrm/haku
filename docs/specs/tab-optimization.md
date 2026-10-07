---
status: IMPLEMENTED
last_updated: 2026-10-07
scope: Which tabs hold a webview, which of those run, how Haku reacts to memory pressure, and what a discarded tab gets back when it reloads.
---

# Tab optimization

## Goal

**Haku uses little memory whether or not memory is scarce.** Free memory is never a reason to keep a page loaded. A tab holds a webview only while giving it up would cost the user something, and only within a fixed budget. Memory pressure can make that footprint smaller; nothing makes it larger.

The price is that reloading becomes the common way a tab comes back, so the second half of this document is about making a reload cheap to look at and lossless where it can be.

## Relation to the webview pool spec

[Webview pool](webview-pool.md) describes the code as it is. This document replaces these parts of it, and each is rewritten there in the change that implements it, not before:

| Webview pool section               | Replaced by                                                       |
| ---------------------------------- | ----------------------------------------------------------------- |
| Capacity (rewritten)               | [Effective capacity](#effective-capacity)                         |
| Eviction (rewritten)               | [Eviction](#eviction)                                             |
| Optimization › Presets (rewritten) | [Settings and presets](#settings-and-presets)                     |
| Optimization › Policy (rewritten)  | [The rule](#the-rule)                                             |
| Optimization › Signals (rewritten) | [Memory pressure](#memory-pressure)                               |
| Scroll, Form contents (rewritten)  | [What a discarded tab gets back](#what-a-discarded-tab-gets-back) |

Everything else there stands: the four states, what discarding is, freezing, parking on `about:blank`, fixed tabs, service workers, session restore. A tab opened in the background still takes no webview until it is first shown.

It also reverses one decision recorded there: _the pool is never resized with free memory_. That stays true for growing. For shrinking it is dropped, because a reload the user did not expect is better than a crash that also takes down whatever else was running.

## The rule

Every web tab that is not visible is in one of three positions:

| Position      | State     | Who                                                                    |
| ------------- | --------- | ---------------------------------------------------------------------- |
| **Must run**  | Live      | Fixed tabs, and tabs with a [must-run signal](#must-run)               |
| **Kept**      | Frozen    | Tabs with [something to lose](#loss), within the [budget](#the-budget) |
| **Discarded** | Discarded | Everything else                                                        |

Freezing is not rationed. A kept tab is frozen because a frozen page costs no more than a running one; the memory decision is whether it holds a webview at all.

### Must run

A background tab must run while any of these holds:

- it is **fixed** (the user's _Keep loaded_);
- it is **playing audio**;
- it is **capturing** the camera, the microphone or the screen.

A must-run tab is never frozen, discarded or evicted, and it reserves a slot for as long as the signal lasts (see [Effective capacity](#effective-capacity)). When the signal ends it becomes an ordinary background tab, timed from that moment.

_Freeze: always_ and _Discard: always_ still mean always: under either, audio and capture are not honoured, and fixing the tab is how a user exempts it.

### Loss

What discarding a tab would cost is one of three levels, the highest that applies:

| Level     | Meaning                               | Signals                                                                                                                            |
| --------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| **Work**  | Something the user made would be gone | Unsaved text; an armed `beforeunload`; the page is the result of a form submission; the tab's host is in [kept sites](#kept-sites) |
| **State** | The page would come back different    | Ten or more interactions since the URL last changed; media paused partway through; a page that could not be read                   |
| **None**  | A reload shows the same thing         | Nothing above                                                                                                                      |

The signals are defined in [Signals](#signals).

An open WebSocket is deliberately not a signal. Analytics, chat widgets and live-update channels open one on ordinary pages, GitHub's included, so it would keep nearly everything. The applications it was meant to catch, such as messengers, are caught by interactions that do not change the URL.

### Grace

A tab left less than `GRACE` ago is kept whatever its loss, so that flipping back to the tab just left is instant. Grace is the only concession made to speed, and it does not apply under pressure.

A tab is timed from when it was left, or from when it fell silent if that is later. A tab that has never been timed, such as one left before the first tick, is timed from the next tick.

### The budget

Kept tabs together may hold at most the **background memory** setting, measured as described in [Slot memory](#slot-memory). A tab whose memory is unknown counts as zero, so on a platform without measurement only the slot count limits kept tabs.

### Deciding

`Browser` decides for every background tab holding a slot that is not must-run, whenever anything changes and on every [tick](#the-tick). Visible tabs, must-run tabs and tabs paused on a dialog are left alone.

With _Discard: smart_ and normal pressure, in order:

1. In grace: keep.
2. Loss is None: discard.
3. Loss is Work: keep.
4. Loss is State: keep while the total memory of all kept tabs stays within the budget, taking the most recently shown first. Discard the rest: once one does not fit, no less recent State tab is kept, even a smaller one.

Tabs kept by steps 1 and 3 count against the budget before step 4 spends what is left, but are not themselves discarded for exceeding it.

A kept tab is then frozen, or left running under _Freeze: never_.

With _Discard: never_ nothing is discarded by this rule, pressure included; only eviction reloads a tab. With _Discard: always_ every background tab is discarded as it is left.

### Effective capacity

```
effective_capacity = max(configured_capacity, visible_count + fixed_count + must_run_count)
```

`must_run_count` is the background tabs currently playing audio or capturing. Music therefore survives a tab switch at a configured capacity of 1. A tab that starts playing while frozen cannot exist; one that starts while visible is already in a slot, so the reservation never has to find a webview for a page mid-playback.

### Eviction

When a tab needs a slot and the pool may not grow, the victim is the unprotected occupant with the lowest loss, and among equals the least recently shown. Protected tabs are the visible and must-run ones.

`WebviewPool` stays ignorant of loss: `Browser` names the victim, as it already names the protected tabs.

A Work tab is evicted only when every other occupant is protected. It is still evicted then: the tab the user just selected must get a webview, and letting Work tabs grow the pool would make every draft a fixed tab.

### Parked webviews

Discarding often leaves a webview parked on `about:blank`. At normal pressure one parked webview is kept as a warm spare and any others are destroyed. Under pressure none is kept.

## Memory pressure

Pressure is Haku's own reading of the system, not `LowMemoryResourceNotification`, which fires too late to act on.

```
headroom = min(available_physical / total_physical, (commit_limit - commit_total) / commit_limit)
```

Both terms come from `GetPerformanceInfo`. Commit is included because running out of commit, not physical memory, is what makes allocations fail and processes die.

| Level        | Headroom     | What changes                                                   |
| ------------ | ------------ | -------------------------------------------------------------- |
| **Normal**   | 15 % or more | [The rule](#deciding) as written                               |
| **Tight**    | under 15 %   | No grace. State tabs are discarded. No parked webview is kept. |
| **Critical** | under 7 %    | As Tight, and Work tabs are discarded too, all at once.        |

A level is left only once headroom is 3 points above its threshold, so a reading that hovers does not flap.

Critical discards unsaved work because the alternative is a crash that loses it anyway, along with everything else on the machine. Visible and must-run tabs are never touched at any level.

A tab discarded at Tight or Critical is marked `relieved` until it is next loaded, and the tab strip shows a marker with a tooltip saying it was unloaded to free memory. Nothing reloads when pressure eases; a tab comes back when it is selected.

### The tick

One thread replaces `relieve_memory_periodically`. Every `TICK` (every `TICK_PRESSED` while pressure is not Normal) it:

1. reads the pressure level;
2. reads each slot's [memory](#slot-memory);
3. reads the [page state](#page-state) of every running background tab, which is how the end of a capture is noticed;
4. calls `Browser::tick(now, pressure, samples)`, which re-times the visible tab and applies the rule.

Pressure is also read when a tab is selected or opened, before it takes a slot, because a burst of tab switches is exactly when memory runs out faster than a timer notices. That reading takes only the system figures, not slot memory, so the rule then weighs the slot memory of the last tick.

A pass that changes no tab writes nothing and emits no `StateChanged`, as now. Every pass emits `MemoryChanged` with the [memory report](#hakumemory), whose figures move on every tick.

## Settings and presets

| Setting                 | Values               | Meaning                                                         |
| ----------------------- | -------------------- | --------------------------------------------------------------- |
| Loaded tabs             | a number             | The configured capacity.                                        |
| Background memory       | megabytes            | The [budget](#the-budget) for kept tabs.                        |
| Freeze background tabs  | never, smart, always | Unchanged, except that smart also spares a capturing tab.       |
| Discard background tabs | never, smart, always | Smart is [the rule](#the-rule). Never and always are unchanged. |
| Kept sites              | a list of hosts      | See [Kept sites](#kept-sites). Edited from the site menu.       |

_Discard: always_ disables Loaded tabs, Background memory and Freeze, as it disables the first and last today.

| Preset      | Loaded tabs | Background memory | Freeze    | Discard |
| ----------- | ----------- | ----------------- | --------- | ------- |
| Save memory | unchanged   | unchanged         | unchanged | always  |
| Balanced    | 2           | 512 MB            | smart     | smart   |
| Performance | 4           | 1536 MB           | smart     | smart   |

Presets no longer depend on installed memory: the footprint is an absolute amount, the same on every machine. `Preset::values` and `Settings::preset` lose their `total_memory` parameter and the slot tiers are removed.

Performance no longer means _Discard: never_. That combination is still available as Custom. Settings saved under the old Performance preset keep their values and show as Custom.

`Settings` gains `kept_memory_mb` (default 512) and `kept_sites` (default empty). Both are layered over defaults by `read_json`, so existing files need no migration. A first launch still applies Balanced.

### Kept sites

The site menu gains a second option under _Keep loaded_: **Don't unload this site**. It adds or removes the tab's host in `kept_sites`. A tab on a listed host has loss Work, whatever the page reports.

It is the remedy for a page whose loss Haku cannot see. It differs from _Keep loaded_ in two ways: it applies to every tab on the host, now and later, and it keeps the page without keeping it running.

A host is the URL's host name, lowercased and without a port, as `URL.hostname` gives it in the interface and `model::host_of` in Rust. The site menu writes the list through `set_settings`, like any other setting, so the option needs no command of its own. `haku://memory` names it as the signal _kept site_.

## Signals

Pages still report nothing. Everything is read from Rust, behind `platform/`.

### Page state

Every content webview runs a new `inject::page_state_script()`. Like the navigation log, it keeps a record inside the page and exposes one drain expression that Rust evaluates with `ExecuteScript`. Only the top frame records.

| Field          | How the page fills it                                                                                                                                                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `unsaved`      | A trusted `input` event landed on a field or `contenteditable` that is still in the document, non-empty, not a secret field by form memory's rules, and whose form has not been submitted since. Evaluated when read, not when typed. |
| `unloadArmed`  | `window` has a `beforeunload` listener or `onbeforeunload` handler, and `navigator.userActivation.hasBeenActive` is true. Listeners are counted by wrapping `addEventListener` and `removeEventListener` on the `window` object only. |
| `interactions` | Trusted `click` and `keydown` events since the URL last changed. Reset by the same `currententrychange` the navigation log listens to.                                                                                                |
| `mediaPaused`  | Some `video` or `audio` element is paused, not ended, past its start, and longer than a minute. Evaluated when read.                                                                                                                  |
| `capturing`    | Some track returned by `getUserMedia` or `getDisplayMedia` is still `live`. Both methods are wrapped to remember their tracks.                                                                                                        |
| `scroll`       | `scrollX`, `scrollY`.                                                                                                                                                                                                                 |
| `draft`        | What the form memory script stores today, as one JSON string.                                                                                                                                                                         |

The record reaches `Browser::report_state(tab, PageState)`, keyed by tab because the slot may have changed hands by the time the read completes. It is read at three moments: when a page is [left](#leaving-a-page), on the tick for running background tabs, and on the tick for the visible tab.

A frozen page is never read. Evaluating script in a suspended webview is not something to rely on, and nothing on a frozen page can change.

A page can lie in its own record. The most it gains is keeping its own tab in memory within the budget, or losing its own draft.

**Result of a form submission** is the one loss signal not in the record. `platform/` reads it in `NavigationStarting`: a navigation carrying a `Content-Type` request header has a body. It is reported with the navigation and cleared by the next one. See [To verify first](#to-verify-first).

### Slot memory

`platform::slot_memory` returns bytes per slot. On Windows:

1. `ICoreWebView2Environment13::GetProcessExtendedInfos` lists every process with the frames it hosts.
2. Each frame is followed through `ParentFrameInfo` to its main frame, whose `FrameId` is matched against each slot webview's own frame id.
3. A process's memory is its `PrivateUsage` from `GetProcessMemoryInfo`: commit, which is what pressure is measured in and what freezing does not give back.
4. A process hosting frames of several slots is split equally between them.

A slot remembers the processes last attributed to it, and a slot missing from a snapshot keeps its previous figure. A frozen page may not be listed as an active frame, but its process is still there.

The browser, GPU and utility processes are not attributed to any slot. They are shown on [`haku://memory`](#hakumemory) and are outside the budget, which governs only what discarding can free.

Other platforms return nothing, and the budget is then not enforced.

### Audio

Unchanged: `IsDocumentPlayingAudioChanged`.

## Leaving a page

A page is read, and captured if it was visible, **before** it is frozen, hidden behind another tab's navigation, parked, or destroyed by shrinking the pool. `Browser` emits `Effect::Leave { slot, tab, capture }` ahead of the `Freeze`, `Blank`, `EnsureSlot` or `Destroy` that follows.

`capture` is set for the tab that was visible, and only for it. `Browser` remembers which slot it last showed and for which tab; when that tab is no longer the visible one and still has its page running, the reconciliation emits its `Leave` first, before any `Hide`, `Freeze` or navigation, since a page can only be captured while it is on screen. The same pass does not read it again.

`webview/` applies it by evaluating the drain expression and, when `capture` is set, calling `CapturePreview` as JPEG, then waiting for both up to `LEAVE_TIMEOUT` before applying the next effect. Commands that drive webviews are already `async`, so the wait is off the main thread. On timeout the next effect proceeds and the tab keeps the state from its last tick.

This is why the visible tab is also read on the tick: a page that hangs at the moment it is left still has a record at most `TICK` old.

`Leave` is not emitted for a slot whose tab is closing, for a parked slot, or for a frozen page, which was read when it was frozen.

## What a discarded tab gets back

### Scroll and drafts live in Rust

Scroll and form contents move out of `sessionStorage` into `Tab`, so they follow the tab into whichever slot it is restored in. That removes the limit that a tab restored into a different slot starts at the top with empty forms, which eager discarding would otherwise make the normal case.

- `Tab::scroll` already exists and is filled from the record. `Tab::draft` is new, is skipped when the tab is serialised, and so never reaches the interface or the session file.
- Both belong to the current history entry and are cleared by a `push` commit, as scroll is today.
- A draft over `DRAFT_LIMIT` is dropped, not truncated.
- The rules about what is never stored are unchanged, and are applied in the page before anything is read.

When `bind` navigates a slot for a tab with either, it emits `Effect::RestoreState { slot, url, scroll, draft }` in place of the unused `RestoreScroll`. `webview/` holds it for the slot, drops it if the slot is navigated elsewhere first, and evaluates the restore expression when the document reaches `DOMContentLoaded`. The page-side script then applies it with the retries it has today, for pages that lay out or build their forms late, and only if the page is on the origin of `url`: a reload that redirects to another site, such as a sign-in page, must not receive text typed on this one.

`scroll_memory_script` and `form_memory_script` are folded into `page_state_script` and stop writing to `sessionStorage`. A draft is no longer restored on an ordinary return to the same URL in the same webview; only a reload of a discarded tab restores.

### Previews

The capture taken by `Leave` is held in `AppState`, keyed by tab, as JPEG bytes. At most `PREVIEW_LIMIT` are kept, least recently shown dropped first, and a tab's capture is dropped when it closes. Captures are never written to disk.

`bind` sets `Tab::restoring` when it navigates a slot for a tab that held none: a discarded tab, including a new one, which starts discarded. `Browser::report_loaded(slot, url)` clears it on `DOMContentLoaded`, ignoring `about:blank`, which a slot shows when it is created or parked. While the active tab is restoring, the viewport draws a cover over the page: the tab's capture, fetched with a `tab_preview` command, or the plain surface colour when there is none. The cover is removed when `restoring` clears or after `COVER_TIMEOUT`, whichever is first.

The cover is what removes the flash of the slot's previous page. It is drawn by the chrome, opaque and hard-edged as [Chrome layering](chrome-layering.md) requires. The chrome can only be seen where its native region includes it, and that region is also where it takes input, so while the cover shows the chrome is kept solid over the viewport exactly as it is for an internal page (`useChromeLayout`'s `covered`). Clicks on a page that is still loading therefore land on the cover and do nothing. An earlier version of this section said the cover would take no input; that cannot be had without the cover being invisible.

The capture is scaled to the viewport's width and anchored at its top-left, as the page was.

`tab_preview` is a command, not a custom protocol, because a protocol would be reachable from content webviews and a capture shows another tab's page. It returns the capture as a `data:` URL, which the cover's `<img>` shows directly. Captures of tabs that no longer exist are dropped after every change.

Navigating within a live tab shows no cover; that is an ordinary page load.

## `haku://memory`

An internal page listing, per slot: the tab, its state, its position and loss level with the signals behind it, and its memory. Above the list: the pressure level and headroom, the budget and how much of it is used, and the unattributed processes.

It exists so the constants below can be tuned against real pages, and so a user can see why a tab was or was not kept. It reads a `memory_report` command and refreshes on the tick's event.

## Constants

Starting values. Each is a named constant, and each is expected to move once `haku://memory` shows real numbers.

| Constant                | Value    | Meaning                                             |
| ----------------------- | -------- | --------------------------------------------------- |
| `GRACE`                 | 60 s     | How long a tab just left is kept regardless of loss |
| `TICK`                  | 5 s      | Interval at normal pressure                         |
| `TICK_PRESSED`          | 1 s      | Interval under pressure                             |
| `TIGHT_HEADROOM`        | 15 %     |                                                     |
| `CRITICAL_HEADROOM`     | 7 %      |                                                     |
| `HEADROOM_HYSTERESIS`   | 3 points |                                                     |
| `INTERACTION_THRESHOLD` | 10       | Interactions without a URL change that mean State   |
| `LEAVE_TIMEOUT`         | 150 ms   | Longest a tab switch waits for the read and capture |
| `COVER_TIMEOUT`         | 4 s      | Longest the preview covers a loading page           |
| `PREVIEW_LIMIT`         | 20       | Captures held                                       |
| `DRAFT_LIMIT`           | 64 KB    | Largest draft held per tab                          |

## Build order

Each step is a change that can ship on its own, and each leaves the documents agreeing with the code.

1. **Measure.** _Built._ `platform::memory_status`, `platform::slot_memory`, the pressure level, the tick replacing the 30-second timer with unchanged discarding behaviour, and `haku://memory`.
2. **Must run and eviction.** _Built._ A tab playing audio reserves a slot, and `Browser` names the eviction victim. Capture and loss are not known yet, so only audio counts as must-run and the order is recency alone.
3. **Page state and restore.** _Built._ `page_state_script`, `Effect::Leave` without capture, `report_state`, scroll and drafts in `Tab`, `Effect::RestoreState`. Capture joins must-run. Loss appears on `haku://memory` before anything acts on it.
4. **The rule.** _Built._ Smart discarding as specified, the budget, pressure levels acting, the `relieved` marker, kept sites, the new settings and presets.
5. **Previews.** _Built._ Capture in `Leave`, the preview store, `restoring` and the cover.

Step 4 must not land before step 3: eager discarding without cross-slot restore loses scroll and drafts on most tab switches.

## Tests

The rule is pure and is tested in `browser_tests.rs` with supplied memory samples, page states, times and pressure levels. At least:

- a tab with no loss is discarded once grace ends, and not before;
- a Work tab is kept past the budget; a State tab is not;
- State tabs are kept most recent first until the budget is spent;
- an audible or capturing background tab is neither frozen nor evicted, and raises the effective capacity;
- eviction takes the lowest loss, then the least recent, and a Work tab only when nothing else is unprotected;
- Tight discards State tabs inside grace; Critical discards Work tabs; neither touches a visible or must-run tab;
- pressure levels hold through the hysteresis band;
- _Discard: never_ discards nothing at Critical;
- `Leave` precedes `Freeze`, `Blank` and another tab's `EnsureSlot`, and is absent for a frozen page;
- a `push` commit clears scroll and draft; a restored tab emits `RestoreState` in any slot;
- `Tab::draft` is absent from the serialised tab.

`page_state_script` is tested as the other injected scripts are.

## To verify first

Facts this design assumes and that have not been checked against the WebView2 runtime Haku ships on. Each is checked at the start of the step that needs it, and each has a fallback that keeps the design intact.

| Assumption                                                                                                                                                                                                                                                                                            | Step | If false                                                                                    |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- | ------------------------------------------------------------------------------------------- |
| **Verified 2026-10-06.** A slot webview's main frame id can be read and matches `FrameInfo::FrameId`                                                                                                                                                                                                  | 1    | Attribute by process: read each webview's renderer process id through the DevTools protocol |
| **Verified 2026-10-06.** `NavigationStarting` exposes a `Content-Type` request header for a form submission                                                                                                                                                                                           | 3    | Drop the signal; such tabs are judged by the rest                                           |
| `ExecuteScript` (**verified 2026-10-06**, 1–26 ms on local test pages) and `CapturePreview` (**verified 2026-10-07**: read and capture together in 50–62 ms, about 22 KB of JPEG for a full-window local page) complete on a webview that is still visible within `LEAVE_TIMEOUT` on an ordinary page | 3, 5 | Raise the timeout, or capture on the tick as well as on leaving                             |
| `DOMContentLoaded` is late enough that removing the cover does not show a blank page. **Not yet observed at the moment the cover comes off**: a dev build showed the cover over a slot still holding the previous page, and the new page after it, but not the frame between                          | 5    | Remove the cover on `NavigationCompleted`                                                   |

## Known limits

- **Loss is a heuristic.** A page holding state Haku cannot see is discarded and reloads. Kept sites is the remedy, and it is manual.
- **A Work tab can still be evicted** when every other slot is protected, and is discarded at Critical. Its form fields come back through the draft; a `contenteditable` draft does not.
- **Only the top frame is read.** Text typed into an embedded editor in a cross-origin frame is not seen.
- **`beforeunload` listeners added through `EventTarget.prototype` directly are not counted.**
- **A frozen messenger receives nothing.** Keeping a page is not keeping it running; _Keep loaded_ is.
- **A preview shows the page as it was left**, which may differ from what the reload renders.
- **The cover takes clicks** while a tab is restoring, for up to `COVER_TIMEOUT`.
- **A page evicted while it was still loading** can fire `DOMContentLoaded` after its slot was handed to another tab, before that tab's navigation commits, and so lift the new tab's cover early. The slot then shows the evicted page until the new one commits.

## Out of scope

- **The back-forward cache.** It holds about what a frozen page holds, evicts on its own schedule, and excludes the pages most worth preserving. Using it would also mean sharing one webview's history between tabs.
- **Growing with free memory**, in any form.
- **Learning per-site behaviour** from past reloads.
- **Persisting scroll or drafts across a restart.**
