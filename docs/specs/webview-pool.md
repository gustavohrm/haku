# Webview pool

**Status:** IMPLEMENTED
**Last updated:** 2026-10-02
**Scope:** How tabs share a limited number of webviews.

## The idea

Haku uses a small, configurable number of real webviews — one by default — shared between any number of tabs.
Memory therefore stays close to flat as tabs accumulate, instead of growing with every tab opened.

## What discarding actually is

**Reactivating a discarded tab reloads the page. It does not resume it.**

This is worth stating plainly because the feature is easy to imagine as pausing a tab and waking it later. No
engine offers that: destroying a webview loses the JavaScript heap, the DOM, open sockets and media playback,
and nothing can serialise a live page and rehydrate it.

What a discarded tab keeps is its URL, title, favicon and place in history. What it loses is everything the
page was doing. This is the same trade Chrome's tab discarding and Edge's sleeping tabs make.

Scroll position survives through a script the page runs on its own — see [Scroll](#scroll) below.

## States

A tab is in exactly one of three states:

- **Live** — bound to a pool slot, backed by a real webview.
- **Discarded** — no webview. Reactivating reloads the URL.
- **Internal** — a `haku://` page drawn by the chrome. Never consumes a slot.

## Capacity

```
effective_capacity = max(configured_capacity, fixed_count + 1)
```

Configured capacity defaults to 1. Every fixed tab reserves a slot, plus one for whichever tab is active, so
**pinning tabs can never leave the active tab without a webview**. Pinning is never refused; the pool grows to
accommodate it.

Lowering the configured capacity destroys surplus webviews immediately rather than waiting for the next tab
switch, because the reason to lower it is to release memory now.

## Eviction

When a tab needs a slot and none is free:

1. If the pool may still grow, it grows.
2. Otherwise the **least recently used** slot whose occupant is not protected is taken, and its occupant is
   discarded.
3. If every resident is protected, the request fails with `NoSlotAvailable` and the tab stays discarded.

Protected tabs are the active tab and every fixed tab.

Eviction navigates the slot to `about:blank` rather than destroying the webview. That frees the page while
keeping the slot warm, which is far cheaper than recreating a webview on every tab switch. Webviews are only
destroyed when capacity shrinks.

## Closing the last tab

The browser always has a tab. Closing the last one opens the home page in its place, as the new-tab button
would.

## Service workers

A service worker outlives the page that started it, and it holds a renderer process while it runs. Chromium is
meant to stop an idle worker after about 30 seconds, but a busy site's worker can keep itself alive far longer.
Measured on a release build: after YouTube's tab was closed, its worker kept a 160 MB process running for more
than two minutes, with no debugger attached. The same worker stayed alive after YouTube was evicted from the
slot by another tab.

Haku therefore stops any worker that has been running with no page using it for 15 seconds
(`platform::workers`). The chrome webview watches the DevTools protocol's `ServiceWorker` events, since it
shares the profile with every content webview. The grace period covers a worker legitimately running without a
page: while it installs, and while it serves a navigation before the page exists. A stopped worker starts again
when a page needs it.

| Release build, YouTube then TabNews opened and both closed | Before | After  |
| ---------------------------------------------------------- | ------ | ------ |
| Settings only, idle                                        | 117 MB | 118 MB |
| Both pages open, Settings active                           | 429 MB | 249 MB |
| Both closed, 60 s later                                    | 375 MB | 193 MB |

What remains above the idle figure after closing is the GPU process's caches and the parked slot itself.

## Fixed tabs

Pinning a tab is the user saying it must stay loaded. It raises the effective capacity and protects the tab
from eviction.

A fixed tab that has gone quiet gives its slot back anyway: pinning promises the tab will not be reloaded while
it is _doing_ something, and once it is idle there is nothing left to preserve. `release_idle_tabs` is driven
by the interface on a timer, so the policy only runs while there is someone to see the result.

### What "idle" currently means

Activity is stamped when a tab is **viewed**. Richer signals — media playing, recent interaction — would
require a channel from the page back into the application, and Haku deliberately grants remote pages none (see
[Architecture § Security](../architecture.md#security)). The policy and its stamp are in place, so a better
signal is a change of source, not a change of design.

## Scroll

A discarded tab reloads, which would otherwise return to the top of the page. The injected script saves and
restores the offset in `sessionStorage`, entirely within the page: no IPC, and nothing left on the site after
the browsing session, unlike `localStorage`.

The trade-off is that the offset lives in the webview that saved it, so a tab restored into a _different_ pool
slot starts at the top.

## Restoring a session

`Session::restore` builds a browser with every tab **discarded** and no slot state at all. It deliberately does
not go through the ordinary open-and-select path: that path assumes its effects will be applied to real
webviews, and at startup there are none and nowhere to put them.

Getting this wrong is silent. The browser records tabs as live and remembers which slot holds which URL, the
effects are discarded, and reconciling later finds nothing to do — leaving a window that never loads anything.
Two tests in `storage/session.rs` guard against it.
