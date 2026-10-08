---
status: IMPLEMENTED
last_updated: 2026-10-06
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

Scroll position and form contents survive — see [Scroll and form contents](#scroll-and-form-contents) below.

## States

A tab is in exactly one of four states:

- **Live** — bound to a pool slot, backed by a real webview, running.
- **Frozen** — bound to a pool slot, its page paused. Showing it resumes the page without a reload.
- **Discarded** — no webview. Reactivating reloads the URL.
- **Internal** — a `haku://` page drawn by the chrome. Never consumes a slot.

## Capacity

```
effective_capacity = max(configured_capacity, 1 + fixed_count + must_run_count)
```

The active tab, every fixed tab, and every other background tab that [must run](#must-run) reserve a slot, so **pinning tabs can never leave the active tab without a webview**, and music survives a tab switch at a configured capacity of 1. Pinning is never refused; the pool grows to accommodate it. A reservation lasts as long as its reason: a tab that falls silent stops counting.

While every background tab is discarded, the configured capacity is treated as 1 (`Settings::pool_capacity`): there is nothing for extra slots to hold, and parked webviews would only cost memory. The user's number is kept for when they change their mind.

Lowering the configured capacity destroys surplus webviews immediately rather than waiting for the next tab switch, because the reason to lower it is to release memory now. Parked webviews go first, then the slots of the tabs eviction would pick, so a protected tab never loses its webview to it.

### Must run

A background tab must run while it is **fixed**, or while it is **playing audio** and neither Freeze nor Discard is _always_. A must-run tab is protected from eviction, never frozen, and never discarded by smart discarding.

## Eviction

When a tab needs a slot and none is free:

1. If the pool may still grow, it grows.
2. Otherwise `Browser` names a victim: the occupant that is not protected and would **lose least** if discarded ([Tab optimization § Loss](tab-optimization.md#loss)), and among equals was **shown least recently**. Its slot is taken and it is discarded. `WebviewPool` takes whichever tab it is given, so the choice stays with the code that knows why a tab matters. A tab holding work is therefore taken only when every other occupant is protected.
3. If every resident is protected, the request fails with `NoSlotAvailable` and the tab stays discarded.

Protected tabs are the active tab and every tab that must run.

Eviction navigates the slot to `about:blank` rather than destroying the webview. That frees the page while keeping the slot warm, which is far cheaper than recreating a webview on every tab switch. Webviews are destroyed when capacity shrinks, and a parked webview beyond one warm spare is destroyed as soon as it is parked; under [memory pressure](tab-optimization.md#memory-pressure) no spare is kept.

Eviction happens whatever the optimization settings say. _Discard: never_ means Haku never discards on its own initiative, not that a tab is never reloaded.

## Optimization

The **Optimization** section of settings holds four values, and the site menu a fifth:

| Setting                 | Values               | Meaning                                                                        |
| ----------------------- | -------------------- | ------------------------------------------------------------------------------ |
| Loaded tabs             | a number             | The configured capacity.                                                       |
| Background memory       | megabytes            | The [budget](tab-optimization.md#the-budget) for kept tabs (`kept_memory_mb`). |
| Freeze background tabs  | never, smart, always | Whether a background tab still in a slot is frozen.                            |
| Discard background tabs | never, smart, always | Whether Haku discards a background tab on its own accord.                      |
| Kept sites              | a list of hosts      | [Don't unload this site](tab-optimization.md#kept-sites), from the site menu.  |

_Discard: always_ makes the first three meaningless, so it disables them, each with a tooltip saying why. The tooltip hangs off a wrapping element, because a disabled control receives no pointer events.

### Presets

A preset fills the three values. Which preset is in effect is derived, never stored (`Settings::preset`), so changing any value shows _Custom_ rather than naming values that are not in effect.

| Preset      | Loaded tabs | Background memory | Freeze    | Discard |
| ----------- | ----------- | ----------------- | --------- | ------- |
| Save memory | unchanged   | unchanged         | unchanged | always  |
| Balanced    | 2           | 512 MB            | smart     | smart   |
| Performance | 4           | 1536 MB           | smart     | smart   |

The values are the same on every machine: what kept tabs hold is an absolute amount, not a share of installed memory, and they are a starting point, not a measurement. The pool is never grown with free memory, because a pool that follows other programs' memory use reloads pages for reasons the user cannot see; [memory pressure](tab-optimization.md#memory-pressure) only ever shrinks what is kept. A first launch applies Balanced. Settings saved under the earlier Performance preset, which never discarded, keep their values and show as Custom.

Presets live in Rust (`model::optimization`), so their values have one definition.

### Policy

Visible tabs and fixed tabs are exempt from all of it. Every other tab holding a slot is settled whenever anything changes (`Browser::settle`), after the pool's slots are shown and hidden:

1. **Discard: always** — discard it.
2. **Discard: smart**, and [the rule](tab-optimization.md#the-rule) gives it up — discard it. A tab that must run or has a dialog open is never given up.
3. **Freeze: never** — leave it running, resuming it if it was frozen.
4. It [must run](#must-run) — leave it running.
5. It has a **dialog open** — leave it; the page is already paused on it.
6. Otherwise — freeze it.

**Always means always.** A tab playing music is frozen or discarded like any other; keeping it loaded is how a user keeps it running.

**Smart discarding** is [the rule](tab-optimization.md#the-rule): a tab is kept for a minute after it is left, then only for what discarding it would lose, within the background memory budget, and less of it under [memory pressure](tab-optimization.md#memory-pressure). It depends on time and memory as well as on what just changed, so it is applied both on every change and on the [tick](tab-optimization.md#the-tick) (`Browser::tick`), in Rust, because memory running out is a reason to act whether or not anyone is looking at the window. A tick that changes no tab writes nothing and emits no `StateChanged`.

A tab discarded while memory is short carries a marker in the tab strip until it is loaded again, so a reload is never unexplained.

### Signals

Pages report nothing; everything is observed from Rust, through `platform/`:

- **Playing audio** — WebView2's `IsDocumentPlayingAudioChanged`. Older runtimes without it treat every page as silent.
- **Memory pressure** and **slot memory** — read on the tick; see [Tab optimization § Memory pressure](tab-optimization.md#memory-pressure) and [§ Slot memory](tab-optimization.md#slot-memory).
- **Page state** — what a page would lose, read by script; see [Tab optimization § Page state](tab-optimization.md#page-state).

### Freezing

Freezing is WebView2's `TrySuspend` on a hidden slot, with `MemoryUsageTargetLevel` lowered; resuming sets it back to normal and calls `Resume`. Neither is waited on. The engine may decline, and the page then keeps running, which costs memory but loses nothing.

`Effect::Resume` precedes anything else done to a frozen slot — showing it, navigating it for another tab, or parking it — so the browser never relies on the engine resuming implicitly.

Measured once in a dev build, on a Wikipedia article in a child webview of Haku's window: `TrySuspend` succeeded, the page stayed suspended until shown, and showing it resumed it. Working set across Haku's WebView2 processes fell by about 145 MB and private memory by about 55 MB. That is one page on one machine, not a benchmark.

## Closing the last tab

The browser always has a tab. Closing the last one opens the home page in its place, as the new-tab button would.

## Service workers

A service worker outlives the page that started it, and it holds a renderer process while it runs. Chromium is meant to stop an idle worker after about 30 seconds, but a busy site's worker can keep itself alive far longer. Measured on a release build: after YouTube's tab was closed, its worker kept a 160 MB process running for more than two minutes, with no debugger attached. The same worker stayed alive after YouTube was evicted from the slot by another tab.

Haku therefore stops any worker that has been running with no page using it for 15 seconds (`platform::workers`). The chrome webview watches the DevTools protocol's `ServiceWorker` events, since it shares the profile with every content webview. The grace period covers a worker legitimately running without a page: while it installs, and while it serves a navigation before the page exists. A stopped worker starts again when a page needs it.

An extension's background worker is exempt. It never has a page, so it would always look idle, and stopping it disconnects the extension's popup mid-task: Bitwarden's login failed with "Attempting to use a disconnected port object" until it was exempted. Chromium manages extension workers' lifetime itself.

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

Below it, **Don't unload this site** adds or removes the page's host in [kept sites](tab-optimization.md#kept-sites). It overrides nothing but discarding, so it carries a plain explanation under the switch rather than a warning.

The warning is a tooltip on an icon beside the option. Its bubble can reach past the menu's rectangle, so its open state is tracked in code rather than left to CSS, and it registers as an overlay of its own while it shows.

## Scroll and form contents

A discarded tab reloads, which would otherwise return to the top of the page with its forms empty. The page's scroll offset and changed form fields are read as it is left, kept in `Tab` by Rust, and handed back when the tab reloads, in whichever slot it lands. Nothing is stored in the page or on the site. [Tab optimization § What a discarded tab gets back](tab-optimization.md#what-a-discarded-tab-gets-back) owns how; what is kept of a form is:

- **Only changes are kept.** A field still holding what the page loaded with is not stored, so a reload never overwrites the page's own values. On restore, a field the page has already changed is left alone, and a page whose fields no longer line up by position and name is skipped field by field.
- **Secrets are never stored:** password, hidden and file fields, anything whose own or form's `autocomplete` is `off`, a payment (`cc-*`) or password token, or `one-time-code`, and fields named like card numbers or security codes.
- **A submitted form has no draft** until it is edited again.
- **Restored values are typed in properly**: set through the element's own setter and followed by `input` and `change` events, so a framework that owns its fields' state sees them. Those synthetic events are not mistaken for an edit.
- **Only on the same origin.** A reload that lands on another origin, such as a sign-in page, receives nothing, so text typed on one site never reaches another's fields.

Restoring is tried at `load` and again shortly after, for pages that build their forms with script. Single-page apps that render a form long after load, or rebuild it per route, may not be restored. Only a discarded tab's reload restores; returning to a page some other way does not.

## Reloads and the HTTP cache

A discarded tab reloads through the ordinary HTTP cache: anything still fresh comes from disk, and stale responses are revalidated. Browsers' back and forward go further and accept stale responses without asking the server, but WebView2 gives a navigation no way to ask for that. `NavigateWithWebResourceRequest` can set headers, but only on the document request, not on the scripts, styles and images that make up most of a reload, so it is not used.

## Restoring a session

`Session::restore` builds a browser with every tab **discarded** and no slot state at all. It deliberately does not go through the ordinary open-and-select path: that path assumes its effects will be applied to real webviews, and at startup there are none and nowhere to put them.

Getting this wrong is silent. The browser records tabs as live and remembers which slot holds which URL, the effects are discarded, and reconciling later finds nothing to do — leaving a window that never loads anything. Two tests in `storage/session.rs` guard against it.
