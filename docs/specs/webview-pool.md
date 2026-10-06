---
status: IMPLEMENTED
last_updated: 2026-10-02
scope: How tabs share a limited number of webviews.
---

# Webview pool

## The idea

Haku uses a small, configurable number of real webviews shared between any number of tabs. Memory therefore stays close to flat as tabs accumulate, instead of growing with every tab opened. How far it goes to keep memory down is the user's choice; see [Optimization](#optimization).

## What discarding actually is

**Reactivating a discarded tab reloads the page. It does not resume it.**

This is worth stating plainly because the feature is easy to imagine as pausing a tab and waking it later. Destroying or reusing a webview loses the JavaScript heap, the DOM, open sockets and media playback, and nothing can serialise a live page and rehydrate it.

What a discarded tab keeps is its URL, title, favicon and place in history. What it loses is everything the page was doing. This is the same trade Chrome's tab discarding and Edge's sleeping tabs make.

Pausing a page _is_ possible while it keeps its webview — that is [freezing](#freezing) — but a frozen page still holds most of its memory. A page's state is its memory: only discarding gives it back.

Scroll position survives through a script the page runs on its own — see [Scroll](#scroll) below.

## States

A tab is in exactly one of four states:

- **Live** — bound to a pool slot, backed by a real webview, running.
- **Frozen** — bound to a pool slot, its page paused. Showing it resumes the page without a reload.
- **Discarded** — no webview. Reactivating reloads the URL.
- **Internal** — a `haku://` page drawn by the chrome. Never consumes a slot.

## Capacity

```
effective_capacity = max(configured_capacity, fixed_count + 1)
```

Every fixed tab reserves a slot, plus one for whichever tab is active, so **pinning tabs can never leave the active tab without a webview**. Pinning is never refused; the pool grows to accommodate it.

While every background tab is discarded, the configured capacity is treated as 1 (`Settings::pool_capacity`): there is nothing for extra slots to hold, and parked webviews would only cost memory. The user's number is kept for when they change their mind.

Lowering the configured capacity destroys surplus webviews immediately rather than waiting for the next tab switch, because the reason to lower it is to release memory now.

## Eviction

When a tab needs a slot and none is free:

1. If the pool may still grow, it grows.
2. Otherwise the **least recently used** slot whose occupant is not protected is taken, and its occupant is discarded.
3. If every resident is protected, the request fails with `NoSlotAvailable` and the tab stays discarded.

Protected tabs are the active tab and every fixed tab.

Eviction navigates the slot to `about:blank` rather than destroying the webview. That frees the page while keeping the slot warm, which is far cheaper than recreating a webview on every tab switch. Webviews are only destroyed when capacity shrinks.

Eviction happens whatever the optimization settings say. _Discard: never_ means Haku never discards on its own initiative, not that a tab is never reloaded.

## Optimization

The **Optimization** section of settings holds three values:

| Setting                 | Values               | Meaning                                                   |
| ----------------------- | -------------------- | --------------------------------------------------------- |
| Loaded tabs             | a number             | The configured capacity.                                  |
| Freeze background tabs  | never, smart, always | Whether a background tab still in a slot is frozen.       |
| Discard background tabs | never, smart, always | Whether Haku discards a background tab on its own accord. |

_Discard: always_ makes the other two meaningless, so it disables both, each with a tooltip saying why. The tooltip hangs off a wrapping element, because a disabled control receives no pointer events.

### Presets

A preset fills the three values. Which preset is in effect is derived, never stored (`Settings::preset`), so changing any value shows _Custom_ rather than naming values that are not in effect.

| Preset      | Loaded tabs                | Freeze    | Discard |
| ----------- | -------------------------- | --------- | ------- |
| Save memory | unchanged                  | unchanged | always  |
| Balanced    | from total RAM             | smart     | smart   |
| Performance | from total RAM, set higher | smart     | never   |

| Installed memory | Balanced | Performance |
| ---------------- | -------- | ----------- |
| Under 12 GB      | 1        | 2           |
| 12 to 24 GB      | 2        | 4           |
| 24 GB and more   | 3        | 6           |

These counts are a starting point, not a measurement. Total RAM chooses them only when a preset is applied; the pool is never resized with free memory, because a pool that follows other programs' memory use reloads pages for reasons the user cannot see. A first launch applies Balanced.

Presets live in Rust (`model::optimization`), since only Rust can read the machine's memory.

### Policy

Visible tabs and fixed tabs are exempt from all of it. Every other tab holding a slot is settled whenever anything changes (`Browser::settle`), after the pool's slots are shown and hidden:

1. **Discard: always** — discard it.
2. **Freeze: never** — leave it running, resuming it if it was frozen.
3. It is **playing audio** and Freeze is not _always_ — leave it running.
4. It has a **dialog open** — leave it; the page is already paused on it.
5. Otherwise — freeze it.

**Always means always.** A tab playing music is frozen or discarded like any other; keeping it loaded is how a user keeps it running.

**Smart discarding** depends on time and memory rather than on what just changed, so it runs on its own timer every 30 seconds (`Browser::relieve`), in Rust, because low memory is a reason to act whether or not anyone is looking at the window. It discards a background tab that is not playing audio and either:

- has not been shown for 30 minutes, or
- has not been shown for 5 minutes while Windows reports low memory.

A tab is timed from when it was last selected, and the visible tab is re-timed on every pass. A pass that discards nothing writes nothing and emits no event.

### Signals

Pages report nothing; everything is observed from Rust, through `platform/`:

- **Playing audio** — WebView2's `IsDocumentPlayingAudioChanged`. Older runtimes without it treat every page as silent.
- **Low memory** — `QueryMemoryResourceNotification` on a `LowMemoryResourceNotification`: the system's own judgement, which accounts for everything else running on the machine.
- **Total memory** — `GlobalMemoryStatusEx`.

### Freezing

Freezing is WebView2's `TrySuspend` on a hidden slot, with `MemoryUsageTargetLevel` lowered; resuming sets it back to normal and calls `Resume`. Neither is waited on. The engine may decline, and the page then keeps running, which costs memory but loses nothing.

`Effect::Resume` precedes anything else done to a frozen slot — showing it, navigating it for another tab, or parking it — so the browser never relies on the engine resuming implicitly.

Measured once in a dev build, on a Wikipedia article in a child webview of Haku's window: `TrySuspend` succeeded, the page stayed suspended until shown, and showing it resumed it. Working set across Haku's WebView2 processes fell by about 145 MB and private memory by about 55 MB. That is one page on one machine, not a benchmark.

## Closing the last tab

The browser always has a tab. Closing the last one opens the home page in its place, as the new-tab button would.

## Service workers

A service worker outlives the page that started it, and it holds a renderer process while it runs. Chromium is meant to stop an idle worker after about 30 seconds, but a busy site's worker can keep itself alive far longer. Measured on a release build: after YouTube's tab was closed, its worker kept a 160 MB process running for more than two minutes, with no debugger attached. The same worker stayed alive after YouTube was evicted from the slot by another tab.

Haku therefore stops any worker that has been running with no page using it for 15 seconds (`platform::workers`). The chrome webview watches the DevTools protocol's `ServiceWorker` events, since it shares the profile with every content webview. The grace period covers a worker legitimately running without a page: while it installs, and while it serves a navigation before the page exists. A stopped worker starts again when a page needs it.

| Release build, YouTube then TabNews opened and both closed | Before | After  |
| ---------------------------------------------------------- | ------ | ------ |
| Settings only, idle                                        | 117 MB | 118 MB |
| Both pages open, Settings active                           | 429 MB | 249 MB |
| Both closed, 60 s later                                    | 375 MB | 193 MB |

What remains above the idle figure after closing is the GPU process's caches and the parked slot itself.

## Fixed tabs

Keeping a tab loaded is the user saying it must stay resident and running. It raises the effective capacity, protects the tab from eviction, and exempts it from freezing and discarding; nothing gives its slot back but closing the tab or turning the option off. There is no idle release: a tab the user asked to keep is not dropped for being quiet.

It is set from the **site menu**, which the address field's leading icon opens, alongside a warning that the tab stays in memory whatever the optimization settings say. It is not on the tab itself, so overriding the pool takes a deliberate step. A fixed tab carries a marker in the tab strip.

The menu hangs over page content, so it registers with `useOverlay`. It is opaque, with no shadow, because the input mask cannot show either over the page; its rounded corners are cut into the mask from its own style. Its text starts directly under the lock glyph that opens it. It closes when the chrome loses focus: a click on the page never reaches the chrome.

The warning is a tooltip on an icon beside the option. Its bubble can reach past the menu's rectangle, so its open state is tracked in code rather than left to CSS, and it registers as an overlay of its own while it shows.

## Scroll

A discarded tab reloads, which would otherwise return to the top of the page. The injected script saves and restores the offset in `sessionStorage`, entirely within the page: no IPC, and nothing left on the site after the browsing session, unlike `localStorage`.

The trade-off is that the offset lives in the webview that saved it, so a tab restored into a _different_ pool slot starts at the top.

## Form contents

A discarded tab would also lose whatever was typed into its forms. A second injected script (`inject::form_memory_script`) keeps it the same way scroll is kept: in `sessionStorage`, inside the page, with the same limit that a different slot starts empty.

- **Only changes are kept.** A field still holding what the page loaded with is not stored, so a reload never overwrites the page's own values. On restore, a field the page has already changed is left alone, and a page whose fields no longer line up by position and name is skipped field by field.
- **Secrets are never stored:** password, hidden and file fields, anything whose own or form's `autocomplete` is `off`, a payment (`cc-*`) or password token, or `one-time-code`, and fields named like card numbers or security codes.
- **Submitting forgets the draft**, and leaving the page right after does not save it back.
- **Restored values are typed in properly**: set through the element's own setter and followed by `input` and `change` events, so a framework that owns its fields' state sees them. Those synthetic events are not mistaken for an edit.

Restoring is tried at `load` and again shortly after, for pages that build their forms with script. Single-page apps that render a form long after load, or rebuild it per route, may not be restored.

The script cannot tell a discarded tab's reload from any other visit to the same URL in the same webview, so a draft also comes back when the user returns to that page some other way during the session, as browsers do on back and forward.

## Reloads and the HTTP cache

A discarded tab reloads through the ordinary HTTP cache: anything still fresh comes from disk, and stale responses are revalidated. Browsers' back and forward go further and accept stale responses without asking the server, but WebView2 gives a navigation no way to ask for that. `NavigateWithWebResourceRequest` can set headers, but only on the document request, not on the scripts, styles and images that make up most of a reload, so it is not used.

## Restoring a session

`Session::restore` builds a browser with every tab **discarded** and no slot state at all. It deliberately does not go through the ordinary open-and-select path: that path assumes its effects will be applied to real webviews, and at startup there are none and nowhere to put them.

Getting this wrong is silent. The browser records tabs as live and remembers which slot holds which URL, the effects are discarded, and reconciling later finds nothing to do — leaving a window that never loads anything. Two tests in `storage/session.rs` guard against it.
