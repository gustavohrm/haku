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

The pool keeps memory flat by giving up page state. Users choose how far that goes, from "one page loaded at a
time" to "keep everything running". [Webview pool](specs/webview-pool.md) is rewritten to this when it is
implemented.

#### States and terms

- **Live** — in a slot and running.
- **Frozen** — in a slot, paused. Scripts and timers stop; the page keeps its state and most of its memory.
  Showing it resumes it without a reload.
- **Discarded** — no slot. Only the URL, title, favicon and history are kept; showing it reloads the page.
  This is the state formerly called _suspended_, renamed to the term other browsers use.
- **Internal** — a `haku://` page drawn by the chrome. Never consumes a slot.

**Evicting** keeps its meaning: taking a tab's slot for another tab, which leaves the evicted tab discarded.

#### Settings

An **Optimization** section in settings holds three values:

| Setting                 | Values               | Meaning                                                   |
| ----------------------- | -------------------- | --------------------------------------------------------- |
| Slots                   | a number             | The configured capacity.                                  |
| Freeze background tabs  | never, smart, always | Whether a background tab still in a slot is frozen.       |
| Discard background tabs | never, smart, always | Whether Haku discards a background tab on its own accord. |

**Discard: always** makes the other two meaningless — nothing stays loaded in the background to freeze or to fill
extra slots — so it disables both, each with a tooltip saying why, and the configured capacity is treated as 1. A
disabled control receives no pointer events, so the tooltip belongs to an element wrapping it.

A **preset** fills the three values. Changing any of them afterwards shows the preset as _Custom_, so it never
names values that are not in effect.

| Preset      | Slots                      | Freeze   | Discard |
| ----------- | -------------------------- | -------- | ------- |
| Save memory | disabled                   | disabled | always  |
| Balanced    | from total RAM             | smart    | smart   |
| Performance | from total RAM, set higher | smart    | never   |

The slot counts per amount of RAM are set by measurement when this is implemented. Total RAM chooses the count
once, when a preset is applied; it is never recomputed from free memory, because a pool that resizes with other
programs' memory use reloads pages for reasons the user cannot see.

#### Policy

Visible tabs and fixed tabs are exempt from all of it. For every other tab in a slot, in order:

1. **Discard: always** — discard it.
2. It **needs to keep running** and Freeze is not _always_ — leave it running.
3. It is **likely to be shown again soon** and Freeze allows it — freeze it.
4. **Discard: smart** and it is **worth freeing** — discard it.
5. Otherwise, leave it as it is.

**Always means always.** A tab playing music is frozen or discarded like any other; fixing it is how a user keeps
it running.

Independently of all three settings, a tab that needs a slot when every slot is taken evicts the least recently
used unprotected tab, as today. _Discard: never_ means Haku never discards on its own initiative, not that a tab is
never reloaded, and its description says so.

#### Signals

Smart decisions are only as good as what Rust can observe, and pages report nothing:

- **Needs to keep running** — the page is playing audio (WebView2's `IsDocumentPlayingAudio`).
- **Likely to be shown again soon** — it was shown recently. Recency is the first signal; better ones are a change
  of source, not of design.
- **Worth freeing** — Windows reports low memory (`CreateMemoryResourceNotification`), or the tab has not been
  shown for a long time. Under pressure, tabs are discarded largest first, measured from the memory of the
  processes WebView2 reports for each slot.

Total RAM comes from `GlobalMemoryStatusEx`. All of these sit in `platform/`, behind the same interface as the
other native operations.

Freezing is WebView2's `TrySuspend` on a hidden slot, with its memory target lowered. Whether it works on a slot
parented into Tauri's window is verified first; if it does not, Freeze does not ship and the settings keep only
Slots and Discard.

#### Cheaper reloads

A discarded tab still reloads, so the reload is made to cost less:

- **Form contents** are saved and restored by the injected script in `sessionStorage`, as scroll already is, with
  the same limit: restoring into a different slot starts empty. Password fields, payment fields and fields
  marked `autocomplete="off"` are never saved.
- **Cached responses** are preferred over revalidation when a discarded tab reloads, as back and forward do —
  provided WebView2 lets a navigation ask for it, which is verified when this is implemented.

#### Fixed tabs

Fixing a tab means it is never frozen and never discarded by policy; only closing it or unfixing it releases its
slot. `release_idle_tabs` and the interface timer driving it are removed, because a tab the user asked to keep
should not be dropped for being quiet.

The control leaves the tab strip. The address-bar icon becomes a **site menu** — an overlay, registered with
`useOverlay` — and fixing the tab is an option there, with a warning that it keeps the page in memory and is meant
for pages the automatic policy does not suit. A fixed tab keeps a marker in the tab strip.

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
- A rounded overlay needs a rounded region, cut the same way as the viewport's corners.
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
