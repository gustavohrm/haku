---
status: IMPLEMENTED
last_updated: 2026-10-30
scope: Browser windows and popups, what happens when a page asks for a new window, by a link or by script, and when it asks to close one.
---

# Windows and popups

Haku has any number of browser windows, sharing one webview pool as [First release § One pool across windows](../first-release.md#one-pool-across-windows) decided. A page's popup opens in a window of its own, still connected to the page that opened it.

## Windows

Each browser window and popup has an immutable `private` flag, exposed in `BrowserState`. Ctrl+Shift+N opens a private browser window. `open_window(url, isPrivate)` opens the requested profile explicitly; passing `null` inherits the calling window, so the existing link menu keeps its profile and **Open link in private window** passes `true`. A page's requests capture its opener's profile before queuing and preserve it even if the opener closes. Popup tabs route to the last-used browser window of that profile, creating one when none remains. Default and private tabs never mix.

Private windows share one ephemeral session. Their last tab closes the window rather than opening a replacement home tab (decided 2026-10-30). A closed private tab is never remembered. Closing the last private tab across all windows destroys every private slot. Private windows and popups are omitted before session records and indices are built; engine restart loses them. See [Private windows](../first-release.md#private-windows).

Private browser windows carry a persistent **Private** marker beside the address field, an incognito icon with its label; private popups carry the same marker beside their read-only address. The marker uses the current theme's raised chrome surface and text tokens, rather than a separate palette or a colour-only signal. Private tabs use a generic icon even if a favicon URL reaches the interface, so the default-profile chrome never requests a private site's favicon. The private new-tab page, under the same icon, lists what Haku does not keep (history, site data once the last private window closes, tabs to reopen or restore), who may still see the activity (the sites visited and the network), and that extensions do not run.

There are two kinds of window.

- A **browser window** has a tab strip and an address field. Ctrl+N opens one on the home page, and so do Shift+clicking a link and launching Haku again once it has started.
- A **popup** shows the one page a page opened in it, under a bar with that page's address, read-only, and the window controls. It never takes another tab: a tab opened from a popup, by a link or by a shortcut, opens in the browser window of the same profile used last, which is brought to the front.

Every tab belongs to one window. Each window shows its own active tab, so every window's visible tab is protected as [Webview pool § Capacity](webview-pool.md#capacity) protects the active one, and capacity grows with the number of windows rather than starving one of them. A webview stays in the window it was last used in. When a tab in another window takes it, it is moved there with `Webview::reparent`, and that window's chrome is raised over it, as when a webview is created.

Rust creates the windows; `tauri.conf.json` declares none. A window and its chrome webview share the label `window-<id>`, which is also how a command knows which window called it: commands act for the window whose chrome called them, and each chrome receives only its own window's `StateChanged`. The shared work of the profile, such as installing extensions and watching service workers, runs on the chrome of the window opened first among those still open.

Within each profile, the browser window used last is the browser window most recently focused; a popup is never it. A tab opened from nowhere in particular, such as by a popup's shortcut, opens there.

An extension's toolbar popup, which [Extensions § Popups](extensions.md#popups) describes, is neither kind: it is not a tab, and the browser state has no tab or window for it. A window it asks for is answered as a page's would be, as though from no tab: a tab opens in the browser window used last, a popup in a window of its own, and the toolbar popup closes.

## The request

WebView2 raises `NewWindowRequested` for `target="_blank"` links, `window.open`, and the engine's own "open in new tab". `platform/` takes a deferral on it, so the page waits, and reports `PageSignal::WindowRequested`. The request is then answered once, either by handing it a webview or by refusing it, and the engine never opens a window of its own.

| The request                                         | What opens                                                                                                                                                                                            |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Made by the page on its own, without a click or key | Nothing, as other browsers block it                                                                                                                                                                   |
| For a `haku://` page                                | Nothing: a page never opens Haku's own pages                                                                                                                                                          |
| A popup: asked for with a size or position          | A popup window, connected to the page                                                                                                                                                                 |
| For anything but an `http` or `https` address       | A new tab in the page's window, or for a page in a popup in the browser window used last, made active and connected to the page, since only the engine can load a `blob:` or blank page on its behalf |
| A web address, with Shift held but not Ctrl         | A new browser window                                                                                                                                                                                  |
| A web address, with Ctrl held but not Shift         | A tab left for later, loaded when it is selected                                                                                                                                                      |
| A web address otherwise                             | A new tab, made active                                                                                                                                                                                |

Every row that opens something opens it anew: a page's request never replaces the page that made it, whatever its target. Without Shift, a request without a size or position, whether from a link or from `window.open`, opens a tab rather than a window, as in Chrome.

A click counts for about five seconds: Chromium lets a page act on one for that long, so a request made a moment after the click is not blocked.

Ctrl+Shift opens a web address in a new tab, made active, as in Chrome.

Requests are answered one at a time, in the order they were made. A tab opened from a page goes right after it, and after the tabs that page opened before, so links opened for later line up in the order they were clicked.

Middle-clicking a link opens it in a new tab, made active. The engine does not say which button was used, and by the time the request arrives the button is already up.

### Where a popup goes

A popup's window is put where the page asked, in CSS pixels, with its page at the size the page asked for and the window taller by its bar. A popup that asks for no position is centred, and one that asks for no size gets a page of 500 by 600. It can be resized, down to 240 by 160.

## Connected pages

A page that opens a sign-in popup waits for it to report back through `window.opener`, so a popup, or a tab opened for a page that only the engine can load, is given a webview handed to the request, which keeps that connection.

That webview has never loaded anything: the engine only accepts one in that state for the request. It is therefore never a parked webview or one taken from another tab. The pool grows by one for it and is trimmed back to capacity as eviction would, parked webviews first. The connected page is shown as it opens, so it reserves a slot as every visible page does, and the trim never reaches it. Should the pool still have no room for it, which the capacity rule rules out, the request is refused and the page loads its address like any other, unconnected.

While a connected page is open, the tab that opened it [must run](webview-pool.md#must-run), whatever the optimization policies say, _Discard: always_ included, so the page a sign-in reports back to is still there. A connected tab that loses its page, by being discarded or evicted, loses its connection with it, and its opener no longer has to run. A link opened in a new browser window is not connected.

## Closing

A page opened by script may close itself with `window.close()`, as a sign-in popup does when it is done. WebView2 raises `WindowCloseRequested`, and its tab is closed. Closing a popup's tab closes the popup. Closing a tab a page opened, by any means, returns to the tab that opened it if that tab is still open in the same window, rather than to the tab on its right.

Closing a browser window closes its tabs, which are not remembered for reopening, and destroys the webviews in it. An ordinary browser window's last tab closing opens the home page in its place; the window stays. A private window's last tab closes that window.

Ordinary and private browser windows both count toward the last browser window. Closing that last window quits Haku, popups included. Its tabs are kept for the next launch only if it is ordinary; private tabs are never persisted ([Private windows](../first-release.md#private-windows)). While only private windows are open the session is not written, so when the last ordinary window closes before them, the next launch restores that window, as Chrome does (decided 2026-10-30): saving then would store no window at all and lose its tabs.

## Launching again

One Haku runs at a time, through `tauri-plugin-single-instance`, registered before anything else so a second launch exits before it opens the profile, the session or the history file. Two processes would otherwise share all three: each would write the session over the other's, and one would find the history file locked ([Errors](errors.md#failures-that-must-not-propagate)).

Once it has started, the running Haku answers a second launch by opening a browser window on the home page, as Ctrl+N does (decided 2026-10-23). Launches answered close together open their windows one after another, so the last launch's window ends up in front. A launch while Haku is still starting opens nothing: it is most often the same launch repeated while Haku is slow to appear, and the windows starting up already answer it. It is what Chrome, Edge and Firefox do, and launching a browser again asks for a browser: a new window is an answer the user sees. Bringing the last window used to the front was rejected, because that window may be on another screen or virtual desktop, where the launch would seem to do nothing.

The arguments of a second launch are ignored. Nothing passes Haku an address yet; opening one as a tab belongs with registering Haku as a browser other applications can open links in.

`pnpm tauri dev` and an installed Haku share the guard, so while one runs, starting the other opens a window in the running one instead.

## The session

The session stores each ordinary browser window, in the order they were last used, with its tabs, its active tab and where it was on screen. Popups are not stored: the page that opened one reloads on the next launch, and the connection cannot come back. A window that was never moved or resized opens maximized.

A session stored before Haku had windows, a flat list of tabs, is read as one window and written back in the new shape.

The JSON shape is `{ windows: [{ tabs, active, bounds, splits? }] }`. `tabs` stores URL/title/fixed records in strip order; `active` is an index into that window's stored tabs. `bounds` is unchanged. Each optional `splits` entry is `{ tabs: [leftIndex, rightIndex], ratio }`, with indices into the same stored order and the pair's committed ratio. Every pair is captured, including background pairs; omit `splits` when there are none. No version field is stored. Older readers ignore the additive field and restore the selected tab alone. Both windows without `splits` and the older flat `{ tabs, active }` remain readable. Unknown single-`split` data is ignored; no shipped writer used it.

A wrong `splits` type means no pairs. Malformed entries are dropped independently: indices must be distinct in-range integers adjacent in stored left/right order, and the ratio must be finite within 0.2–0.8. The first valid entry claims its members; later overlaps are dropped. Indices map through URL filtering, and filtering either member drops the pair without substituting a neighbor. Active-index filtering and fallback are independent, so invalid pair metadata never loses the selected tab. Empty windows are still omitted; when none survive, one window opens the configured home page.

Restore builds tabs and pairs through `Browser::restored` with fresh identities and no slots; external tabs start discarded. The selected tab determines the visible single tab or pair and focused half. Each restored window waits for an accepted layout at its current revision before loading pages. Background pair membership adds no loading or protection; existing fixed-tab and must-run policy still applies. Slots, native focus, drafts, previews, dialogs, geometry and popup connections are not persisted. [Split view](split-view.md#session-persistence) records the approved persistence decision and its acceptance tests.

## Not built

- Tearing a tab off into its own window, and moving tabs between windows.
- Reopening a closed window.
- A window restored onto a screen that is no longer attached is not moved back onto one.
- A new window's address field is not focused: the window's interface is not listening yet when Ctrl+N asks for it.
- A connected tab left in the background can be discarded like any other, which cuts its connection for good. Reloading cannot restore it.
- There is no indicator for a blocked request.
