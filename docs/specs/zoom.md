---
status: IMPLEMENTED
last_updated: 2026-11-01
scope: Zooming a page, how the level is kept per site, and how it is shown and reset.
---

# Zoom

Approved by the maintainer on 2026-11-01. It adds a field to `Tab`, a command, a shortcut and a persisted setting, so it is a contract decision under [Agent instructions § Judgment](../../AGENTS.md#judgment). It implements the Zoom row of [First release § Features](../first-release.md#features).

## Problem

Tauri turns WebView2's zoom off unless asked, so Ctrl+plus, Ctrl+minus and Ctrl+wheel did nothing. Turning the engine's zoom on is not enough. The engine keeps a zoom the user makes for the current page only, and a webview is shared by every tab that uses its slot ([Webview pool](webview-pool.md)), so the level would follow the webview instead of the site.

## Behaviour

**Zooming.** Ctrl+plus, Ctrl+minus, Ctrl+wheel and a touchpad pinch zoom the page in the engine's steps, from 25% to 500%. They are the engine's, not Haku's: the keymap cannot bind punctuation keys ([Shortcuts § Changing bindings](shortcuts.md#changing-bindings)), and the wheel and pinch never reach it.

**Per site.** The level belongs to the page's host, as Chrome keeps it: every tab on `example.com`, now and later, shows the level last chosen there, whatever the scheme, port or path. Zooming one such tab zooms the others at once. A page with no host, such as a local file, keeps its level in its tab only.

**Resetting.** **Reset zoom** (Ctrl+0) returns the page's host to 100%. It is a Haku shortcut, rebindable like the others, because the engine's own Ctrl+0 returns to the level Haku last set, not to 100%.

**Showing.** While the active tab's page is not at 100%, the address field shows its level, such as "125%", as a button. Clicking it resets the zoom.

**Kept.** Levels other than 100% are stored in settings by host. `haku://settings/browsing` lists them under **Zoom levels**, beside **Site permissions**, and removes them one at a time.

**Private windows.** They show the stored levels. A level chosen in a private window is kept in memory for the private session only and applies ahead of the stored one there, as private visits never write a site-derived setting ([Settings page § Sections](settings-page.md#sections)).

## Mechanism

**The engine.** Content webviews are built with zoom enabled (`zoom_hotkeys_enabled`, which sets WebView2's `IsZoomControlEnabled` and `IsPinchZoomEnabled`). The chrome is not: the interface does not zoom.

**What Haku sets.** WebView2 keeps one zoom factor per webview. One Haku sets becomes the webview's default and lasts across navigations, and setting it raises no event. One the user makes raises `ZoomFactorChanged` and lasts only until the next navigation. `Browser` keeps the factor it believes each slot has, and on every reconciliation sets the level of each live tab's page on its slot where the two differ. That covers a slot handed to another tab, a tab navigating to another host, and the other tabs of a host the user just zoomed. A frozen slot is set when it resumes.

**What the page reports.** `platform/` reports `ZoomFactorChanged` as a page signal, with the level in whole percent and whether it followed a navigation. The engine raises it after `ContentLoading` when a navigation changes the factor, for example back to the webview's default; such a report is not the user's, so Haku only records what the slot now shows and sets the site's level again. A report is taken as the user's when none of these hold:

- it came between `ContentLoading` and the first of the next zoom report, `DOMContentLoaded` or `NavigationCompleted`;
- it matches the level Haku expects;
- it was made on a host the tab has since left.

A user's level is recorded for the host and set on the slot again, so it becomes the webview's default and survives the next navigation within the site.

**The contract.** `Tab` gains `zoom: number`, the level its page shows, in percent. `Settings` gains `siteZoom: { host: string; percent: number }[]`. A new command, `reset_zoom(tab)`, resets the tab's host as the shortcut does. A new shortcut action, `resetZoom`, is bound to Ctrl+0.

## Decisions

- **Per host, as Chrome.** Chrome, Edge and Firefox keep zoom per site. Per tab would lose the level on every new tab; per origin would split `http` from `https` for no gain a reader would notice.
- **Haku owns the level, the engine owns the gestures.** Binding plus, minus and the wheel would need keys the keymap cannot express and a script in every page to catch the wheel. The engine already does both; Haku only needs to hear the result.
- **A report after a navigation is never the user's.** When unsure, Haku reverts a zoom rather than stores one: a zoom made while a page is still loading may be undone, which the user sees and can redo, while a stored level picked up from a navigation would follow the site for good.
- **Whole percent.** Levels are compared and stored as whole percentages, so a factor the engine rounds, such as 33.3%, matches the level Haku set and no tolerance is needed.

## Known limits

- Ctrl+Numpad0 is not bound, as the keymap reads only the main row's digits; the engine handles it and returns to the level Haku last set, which is the site's level.
- A zoom made between a navigation's `ContentLoading` and its document loading is undone.
