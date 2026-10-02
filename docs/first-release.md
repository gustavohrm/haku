# First release

**Status:** APPROVED
**Last updated:** 2026-10-02
**Scope:** What the first release includes, and the structural decisions it rests on.

Most of this is not built yet. It is approved so that features are built on these decisions instead of
retrofitted onto the single-window, single-viewport shape the code has today.

Where a decision here changes an IMPLEMENTED spec, that spec is updated in the change that implements it, not
before, so the two never disagree about what the code does now.

## Structural decisions

### Internal pages are `haku://` URLs

**Implemented.** Internal pages are `haku://<page>`, routed by host. Anything after the host — a path, a query,
a fragment — belongs to the page, so `haku://history?q=…` and `haku://settings/appearance` need no new routing.

`haku:` without slashes was dropped: it has no host, and `haku:settings` reads like `host:port` to address
parsing.

### Viewports, not a viewport

The interface reports a list of named viewport rectangles in `set_layout`, not one. The browser tracks the
**visible tabs**, one per viewport, instead of a single active tab.

Pool protection generalises with it, and the guarantee in [Webview pool § Capacity](specs/webview-pool.md#capacity)
holds for every viewport:

```
effective_capacity = max(configured_capacity, fixed_count + visible_count)
```

Split view is built on this. No other feature may assume there is exactly one page on screen.

### Keyboard input while a page has focus

When a content webview has focus, keys go to it, and the page has no channel into the application. Browser
shortcuts are therefore intercepted **natively**: `platform/` handles the content webview's accelerator-key
event (WebView2's `AcceleratorKeyPressed`) and hands the key to Rust.

There is one keymap, in Rust. Keys pressed while the chrome has focus go through the same command path, so a
shortcut behaves identically wherever focus is. Pages still get no IPC.

### Webviews belong to a profile

A webview's cookies, storage and cache come from its profile, and a webview cannot change profile. Slots are
therefore keyed by profile, and a slot of the wrong profile is replaced rather than reused.

- **Default** — the ordinary profile.
- **Private** — in-memory. Nothing it does is persisted, recorded in history, or restored with the session.
- **Containers** — named, persistent profiles with their own logins (Work, Personal).

Capacity stays a single global count across profiles: memory is the constraint, not the profile.

**Workspaces** are a grouping of tabs in the browser model, drawn by the interface. A workspace may name a
default container for tabs opened in it. It is not itself a profile.

Whether Tauri exposes WebView2 profiles on its webview builder is checked when this is implemented. If it does
not, `platform/` sets them. A separate `data_directory` per container is not an acceptable substitute, because
each one starts its own browser process.

### One pool across windows

Haku may have several browser windows, including a tab torn off into its own window. They share **one** pool,
so memory stays flat as windows are added.

Every tab belongs to a window. A slot shown in a different window is moved there with `Webview::reparent`, and
the destination's chrome is raised over it afterwards, as on creation. `MAIN_WINDOW_LABEL` stops being an
assumption anywhere outside startup.

### Tab optimization

**Implemented.** The pool keeps memory flat by giving up page state, and users choose how far that goes, from
"one page loaded at a time" to "keep everything running". The states, the Optimization settings and their
presets, the freeze and discard policy, the signals it reads, fixed tabs and the site menu, and how a discarded
tab keeps its form contents are specified in [Webview pool](specs/webview-pool.md).

Three decisions changed in implementation:

- **Under memory pressure, every eligible tab is discarded**, rather than one at a time, largest first. Measuring
  each slot's memory would need process accounting the policy did not otherwise need, and low memory is rare
  enough that freeing everything not shown recently is the safer answer.
- **Freezing a tab does not wait for it to have been shown recently.** With _Freeze: smart_, a background tab is
  frozen as soon as it is left, unless it is playing audio; recency decides only what smart discarding spares.
- **Reloads do not prefer stale cached responses.** WebView2 gives a navigation no way to ask for it; see
  [Webview pool § Reloads and the HTTP cache](specs/webview-pool.md#reloads-and-the-http-cache). Form contents
  are kept as planned.

### Running in the background

Closing the last window may leave Haku running in the tray (a setting). While no browser window exists, every
content webview is destroyed: there is nothing to show, and the reason to keep running is the launcher, not
pages.

The **launcher** is a small window whose chrome webview renders a launcher route, so it shares the interface's
theme and translations. A global hotkey (`RegisterHotKey`, in `platform/`) shows it. Submitting resolves the
input exactly as the address field does, bangs included, and opens a tab in the last window used, or a new one.

The resident cost is WebView2's base processes, which is accepted in exchange for one interface rather than a
second, native one per platform.

### Developer tools

Developer tools are WebView2's own window. They are **never docked**. Docking would require starting WebView2
with a remote-debugging port, which stays open for the whole session and lets any local program drive every
tab. That cost is out of proportion to the convenience.

### Observed, not reported

Favicons and tab previews follow [Page observation](specs/page-observation.md): Rust reads them, pages report
nothing.

- **Favicons.** Rust reads the page's declared icons, falling back to `/favicon.ico`, fetches the image itself
  and caches it in SQLite. The interface shows the cached image, never a remote URL, so an icon does not
  depend on its tab being live.
- **Tab previews.** Before a slot is handed to another tab, Rust captures it (WebView2's `CapturePreview`). The
  viewport shows the incoming tab's last capture while its page loads. Captures live in memory, bounded, and
  are not persisted: keeping memory flat is the point of the pool.

### What the chrome can draw over a page

The [Chrome layering](specs/chrome-layering.md) mechanism sets limits every appearance must respect:

- Anything over the page is **opaque**, with **hard edges**. No translucency, blur or shadow falls onto page
  content.
- A rounded overlay needs a rounded region, cut the same way as the viewport's corners. **Implemented**: an
  overlay's radius is read from its own style; see [Chrome layering](specs/chrome-layering.md#rounded-page-corners).
- A bar that floats over the page and reveals on hover keeps a thin strip at the window edge inside the
  chrome's region. Otherwise the page owns those pixels and the chrome never sees the pointer arrive.

### Appearance is data

Appearance is a set of settings, not a set of stylesheets. The shell maps them to attributes and tokens, and
the layout follows from CSS, because the viewport rectangles are measured, not computed.

| Setting  | Values                                                  |
| -------- | ------------------------------------------------------- |
| Tabs     | horizontal, vertical                                    |
| Bars     | docked (take space), floating (over the page, on hover) |
| Density  | compact, normal, spacious                               |
| Corners  | rounded, pill                                           |
| Surfaces | solid, soft, edged, ghost                               |
| Palette  | colour scheme and accent                                |

Looking like Chrome, Helium, Firefox or Zen is a **preset**: a named bundle of these settings. Users may add
their own CSS to the chrome. They may not add script to it, because the chrome is the one webview with access to
the application.

### Extensions

There is no store. Extensions are installed from disk, at the user's own risk, in two tiers:

1. **User scripts and user styles**, injected by Rust into pages matching a URL pattern. They run as part of
   the page and gain no channel into the application.
2. **Unpacked Chrome extensions**, through WebView2's extension support. Chromium sees each pool webview as a
   tab, not Haku's tabs, so extensions built on tab APIs will misbehave. Content-script and request-filtering
   extensions are the expected fit. Bitwarden is the motivating case and its compatibility is unverified.

### Passwords and autofill

Haku does not manage passwords or autofill. Storing credentials is a responsibility it does not take on; a
password-manager extension is the recommended route. WebView2's built-in password saving and autofill are
turned off, so nothing is stored by default either.

## Features

| Feature                              | Where it lives                                                                                                                                                                                                                                |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| History                              | One filter type — date range, hosts, wildcards, text — shared by search and pruning so the two cannot drift. A `host` column added by migration. Recording on/off and a blocklist, both checked in `record_visit`. Private tabs never record. |
| Bookmarks and shortcuts              | SQLite. Shortcuts are bookmarks shown on the new-tab page.                                                                                                                                                                                    |
| Appearance                           | See [Appearance is data](#appearance-is-data).                                                                                                                                                                                                |
| Split view                           | See [Viewports, not a viewport](#viewports-not-a-viewport).                                                                                                                                                                                   |
| Print, save page                     | WebView2's print and save-as dialogs, driven from Rust.                                                                                                                                                                                       |
| Zoom                                 | `set_zoom`, persisted per host.                                                                                                                                                                                                               |
| Find in page                         | WebView2's find API, with a bar drawn by the chrome.                                                                                                                                                                                          |
| Downloads                            | WebView2's download event in `platform/`; progress drawn by the chrome; `haku://downloads`.                                                                                                                                                   |
| Bangs                                | `resolve_target`, from a bundled list, resolved locally so a query never reaches a third party first.                                                                                                                                         |
| Launcher                             | See [Running in the background](#running-in-the-background).                                                                                                                                                                                  |
| New windows and popups               | WebView2's new-window event. Links open as tabs. A scripted popup gets a fresh slot handed back to the request, so `window.opener` survives — sign-in popups depend on it.                                                                    |
| Permissions                          | Camera, microphone, location, notifications and the rest, asked by a chrome-drawn prompt titled with the site, as page dialogs are. Decisions stored per site.                                                                                |
| Fullscreen                           | A page entering fullscreen hides the bars and its viewport fills the window.                                                                                                                                                                  |
| Context menu                         | WebView2's context-menu event, drawn by the chrome with Haku's own items.                                                                                                                                                                     |
| HTTP login, certificates             | Login prompts and certificate errors drawn by the chrome, never by the page.                                                                                                                                                                  |
| Crashed pages                        | A webview whose process dies leaves its tab in a crashed state with a reload, not a blank viewport.                                                                                                                                           |
| Private tabs, containers, workspaces | See [Webviews belong to a profile](#webviews-belong-to-a-profile).                                                                                                                                                                            |
| Windows and tab tear-off             | See [One pool across windows](#one-pool-across-windows).                                                                                                                                                                                      |
| Tab optimization, site menu          | See [Tab optimization](#tab-optimization).                                                                                                                                                                                                    |
| Favicons, tab previews               | See [Observed, not reported](#observed-not-reported).                                                                                                                                                                                         |
| Extensions                           | See [Extensions](#extensions).                                                                                                                                                                                                                |
| Developer tools                      | See [Developer tools](#developer-tools).                                                                                                                                                                                                      |

## Out of scope

- Password management and autofill.
- Docked developer tools.
- An extension store.
